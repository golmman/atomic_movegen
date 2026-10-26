# Performance Analysis 7 — `atomic-movegen`

## Executive summary

The current hot path is still dominated by two things:

1. **Pseudo-legal move generation**, especially pawn generation and `MoveList::push`.
2. **Legality filtering**, especially `Board::legal` and the in-place compaction loop in
   `generate_legal_with_state`.

On the check-heavy test #13 (`2,160,817,389` nodes at depth 6), `generate_legal` accounts for
~88.6 % of sampled child time, `generate_pseudo_legal` ~37.2 %, `Board::legal` ~20.1 %,
and pawn generation ~15.8 %. On the quieter tactical test #14 (`245,931,633` nodes),
`Board::legal` is still ~20.3 % of self time, while the compaction loop alone is ~14 % of
source-line samples.

Two **safe, measured** improvements are available immediately:

- A `perft`-specific fast path that walks the pseudo-legal list and recurses on legal moves
  directly, without compacting a legal list first: **~3 % faster on test #13**, neutral on #14.
- Caching `occupied`, own-commoner, and enemy-commoner bitboards locally inside `Board::legal`:
  **~1–2 % faster** on both profiled positions.

Together with a boolean attack probe and a category-specialized legality path, a realistic
near-term target is **5–9 %** on the full `verify_perft` suite, with much less risk than another
`EVASIONS` rewrite.

Several previously proposed or intuitive micro-optimizations were measured this time and
**did not help**: skipping Zobrist hashing in perft, `MaybeUninit` move lists, removing the
`Square::NONE` branch from `piece_on`, and batching `MoveList` writes behind a local counter.
Those are documented below so they are not retried blindly.

---

## 1. Environment and baseline

| Item | Value |
|------|-------|
| Host | Linux `aarch64` virtual machine (Apple CPU model, 6 CPUs) |
| Rust | `rustc 1.99.1`, edition 2024 |
| Build | `cargo run --release --example verify_perft` |
| Profiling build | `RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft` |
| Profiler | Linux `perf` 7.2.11 (software `task-clock` sampling) |

The machine has no usable hardware PMU counters in this environment: `perf stat -d` only reports
`task-clock`, context switches, and page faults, not cycles, instructions, cache misses, or branch
misses. Kernel symbols are also restricted. The analysis therefore uses sampled function/source-line
attribution, disassembly, and repeated wall-clock measurements rather than hardware-counter data.

### Full-suite baseline is noisy

Three `verify_perft` runs after restoring the working tree:

| Run | Total time | Slowest test |
|-----|-----------:|--------------|
| 1 | 60.751 s | #13 (9.284 s) |
| 2 | 56.853 s | #13 (8.767 s) |
| 3 | 61.678 s | #13 (9.471 s) |

The full suite varies by roughly ±8 % between runs on this shared/virtual machine. For A/B testing,
short repeated depth-6 runs on fixed FENs were much more stable (typically <1 % spread). Those are
the primary measurements used below.

### Stable microbenchmark baseline

`target/release/examples/perft FEN 6`, 7 runs each, median:

| Position | FEN | Nodes | Median |
|----------|-----|------:|-------:|
| #13 (check-heavy) | `rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5` | 2,160,817,389 | **8.457 s** |
| #14 (tactical, quieter) | `r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15` | 245,931,633 | **1.053 s** |

---

## 2. Methodology

1. Built the `profiling` profile with frame pointers and debug symbols.
2. Recorded `perf` profiles for test #13 depth 6 (8.5 s, ~8.5k samples) and test #14 depth 6
   (five repetitions in one process, ~2.6k samples).
3. Inspected:
   - flat self-time by symbol,
   - child-time attribution with inlined frames (`--children`),
   - source-line attribution (`--sort=srcline`),
   - disassembly of `atomic_movegen::perft` and `Board::legal`.
4. Compared with the existing tests in `docs/plans/perf_analysis6/` and the reference
   implementation under `Fairy-Stockfish/src/`.
5. Ran targeted A/B experiments by temporarily modifying the source, rebuilding, benchmarking,
   and restoring the tree. All experiments below ended with the source restored; `cargo test`
   passes on the final tree.

The `--children` percentages are not additive: `generate_legal`, `generate_pseudo_legal`, etc.
are inlined frames nested inside `perft`, and a sample may contribute to several ancestors.

---

## 3. Profile results

### 3.1 Flat self time

| Symbol | Test #13 | Test #14 |
|--------|---------:|---------:|
| `atomic_movegen::perft` (contains all inlined movegen/legality code) | **77.95 %** | **77.17 %** |
| `<Board>::legal` (not inlined) | **20.11 %** | **20.35 %** |
| `__memset_zva64` | 1.54 % | 1.79 % |
| `memset@plt` | 0.35 % | 0.50 % |
| other / kernel / unresolved | <0.1 % | <0.2 % |

The `perft` symbol is a large inlined body. It contains pseudo-legal generation, compaction,
`is_move_trivially_legal`, `do_move`/`undo_move`, hashing, and state setup. The separate
`Board::legal` symbol is the only substantial non-inlined move-generation function.

### 3.2 Inlined child attribution, test #13

| Inlined frame | Child share |
|---------------|------------:|
| `generate_legal` / `generate_legal_with_state` | 88.63 % / 85.26 % |
| `generate_pseudo_legal` / `for_each_pseudo_legal` | 37.20 % |
| `Board::legal` (self) | 20.11 % |
| `generate_pawn_moves_for` | 15.81 % |
| `is_move_trivially_legal` | 7.07 % |
| `generate_piece_moves` (all piece types combined) | ~6.9 % |
| `Board::do_move` | 6.48 % |
| `Board::piece_on` | 6.12 % |
| `attackers_to` | 5.21 % |
| `u64::wrapping_mul` (mostly magic index multiplication) | 4.67 % |
| `populate_state` | 3.37 % |
| `magic::bishop_attacks` / `magic::rook_attacks` | 3.29 % / 2.79 % |
| `Board::commoners` | 2.67 % |
| `compute_checkers` | 2.04 % |
| `Board::undo_move` | 1.52 % |
| `compute_pinned` | 1.32 % |

### 3.3 Source-line hotspots

Selected hot source lines, with each profiled test's share:

| Source line | What it is | #13 | #14 |
|-------------|------------|----:|----:|
| `movegen.rs:301–303` | compaction read / legality dispatch / write | **~20.3 %** | **~14.0 %** |
| `movegen.rs` unknown/0 line | inlined pawn/piece generation code | 15.01 % | 16.23 % |
| `board.rs:519` | `piece_on` array load | 6.11 % | 6.26 % |
| `types.rs:787` | `MoveList::push` bounds check + length update | 2.86 % | 4.24 % |
| `board.rs:556` | `pieces_color_pt` (`by_color & by_type`) | 3.43 % | 4.16 % |
| `board.rs:1561` | commoner equality in `is_move_trivially_legal` | 3.40 % | 4.01 % |
| `types.rs:344` | `Bitboard::bitor` | 2.38 % | 3.13 % |
| `types.rs:251/267` | `file_of` / `rank_of` lookup tables | 3.77 % | 4.35 % |
| `attacks.rs:229` | `king_attacks` table load | 2.82 % | 2.83 % |
| `board.rs:1406` | `Board::legal` returns `true` | 1.54 % | 1.45 % |
| `magic.rs:470/478` | magic table load | ~1.35 % | ~1.0 % |
| `movegen.rs:179` | en-passant square check in pawn generation | 1.26 % | 1.26 % |
| `movegen.rs:155` | pawn capture file check | 0.85 % | 1.07 % |
| `lib.rs:79` | perft loop / `do_move` call site | 0.59 % | 0.99 % |
| `board.rs:1158` | en-passant square update | 0.58 % | 0.65 % |

### 3.4 What this means

There are two distinct cost regimes:

- **Check-heavy positions (#13):** pseudo-legal generation is expensive because the full candidate
  stream is generated and then most moves are filtered. Pawn generation alone is ~16 %. The
  compaction loop is ~20 % of source-line samples. `Board::legal` is 20 % self time.
- **Quieter tactical positions (#14):** pseudo-legal generation is cheaper (~8 % of child time),
  but `Board::legal` is still ~20 % self time because captures, commoner moves, and pinned moves
  take the full legality path. The compaction loop is still ~14 % of line samples.

So optimizing only the check path is insufficient, and the previous `EVASIONS` attempts failed for
that reason: they increased complexity and changed layout without reducing the dominant total work
enough. The next wins should target the common path shared by both regimes.

---

## 4. A/B experiments performed

All changes below were temporary; the source tree is restored. Medians are from 5–7 depth-6 runs.

| Experiment | Test #13 | Test #14 | Result |
|------------|---------:|---------:|--------|
| Baseline (before experiment) | 8.457 s | 1.053 s | — |
| **`perft` fast path without legal compaction** | **8.160 s (−3.0 %)** | 1.058 s (+0.2 %) | **Keep / implement** |
| **Cache `occupied` + commoner bitboards in `Board::legal`** | **8.278 s (−2.1 %)** | **1.041 s (−1.1 %)** | **Keep / implement** |
| Boolean `square_attacked_by` early-exit | 8.338 s (−0.9 %) | 1.050 s (−0.6 %) | Small win; combine with legal cache |
| Skip all Zobrist hash updates in make/unmake | 8.403 s (+1.1 %) | 1.055 s (+0.1 %) | No gain; do not pursue |
| `MaybeUninit<Move>` move list (no zeroing) | 8.534 s (+2.7 %) | 1.065 s (+1.0 %) | No gain; do not pursue |
| Direct `squares[sq]` without `Square::NONE` branch in `piece_on`/`empty` | 8.308 s (±0 %) | 1.055 s (±0 %) | No gain; compiler already handles it |
| Batched collector with local `MoveList` count | 8.703 s (+3.5 %) | 1.120 s (+6.1 %) | Regression; do not pursue |

### 4.1 Why the no-compaction perft path helps

`perft` currently calls `generate_legal`, which builds a pseudo-legal list and then rewrites it
in place as a legal list. The recursion then iterates the compact legal list. For a perft count,
the compact list is unnecessary: after testing each pseudo-legal move, the recursion can make it
immediately.

The experimental implementation:

```rust
let mut moves = MoveList::new();
let mut state = StateInfo::new();
board.populate_state(&mut state);
generate_pseudo_legal(board, &mut moves);

let mut child_state = StateInfo::new();
for &m in moves.as_slice() {
    if is_move_trivially_legal(board, m, &state) || board.legal(m, &state) {
        board.do_move(m, &mut child_state);
        total += perft(board, depth - 1);
        board.undo_move(m, &child_state);
    }
}
```

It passed all 41 perft positions. One full-suite run took **54.615 s**, versus same-session baseline
runs of **56.853 s** and **61.678 s**. The fixed-FEN median improvement is the more reliable signal:
~3 % on #13, neutral on #14.

This must remain a `perft`-specific fast path, not a change to public `generate_legal`, because
public move ordering is pinned by `tests/move_order_golden.rs` and consumers depend on it.

### 4.2 Why local caching inside `legal` helps

`Board::legal` currently recomputes:

- `self.occupied()` at least twice (castling path and the generic path),
- `self.commoners(us)` up to twice,
- `self.commoners(them)` up to twice,

inside one call. Loading them once at function entry and reusing them is semantically identical
because `legal` does not mutate the board. The measured gain is ~1–2 %.

This is **not** the same as the failed `report2.md` attempt: that attempt added extra
`StateInfo` fields and a capture pre-filter that did additional blast work for every capture.
The local cache adds no new work.

### 4.3 Why the failed experiments failed

- **Skipping Zobrist hashing** did not help. Hash updates are a small fraction of `do_move`, and
  removing them changed code layout/register pressure enough to be neutral or slightly worse.
- **`MaybeUninit` move list** did not remove enough work. `memset` is only ~1.5–2.3 % of samples,
  and the `MaybeUninit` version made `push`/slice handling worse in the optimizer.
- **Removing the `Square::NONE` branch from `piece_on`** had no measurable effect. The branch is
  predictable and the surrounding load dominates.
- **Batching `MoveList` writes behind a local counter** regressed significantly. The compiler's
  existing inlined `push` sequence is better than the explicit buffer/count version on this target.

---

## 5. Remaining opportunities, highest expected value first

### 5.1 [HIGH, measured] Add a perft fast path that avoids legal-list compaction

**Expected:** ~3 % on check-heavy positions; neutral on quiet positions; likely ~2–4 % on the
full suite.
**Risk:** Low. Perft-only; public move order unchanged.
**Effort:** Small, confined to `src/lib.rs::perft`.

Implement the no-compaction loop shown in §4.1. For `depth == 1`, count moves with the legality
predicate instead of calling `generate_legal`. Keep `generate_legal` public behavior untouched.

Potential follow-up: after this, try moving `StateInfo` allocation/reuse and `MoveList` initialization
only in the perft-specific path; do not change the public list API based on the failed experiments.

### 5.2 [HIGH, measured] Cache base bitboards in `Board::legal`

**Expected:** ~1–2 %.
**Risk:** Very low.
**Effort:** Small, ~10 lines in `src/board.rs`.

At the top of `legal`:

```rust
let occupied_before = self.occupied();
let our_commoners_before = self.commoners(us);
let them_commoners_before = self.commoners(them);
```

Use `occupied_before` for the castling path and for `pre_non_pawns`; use the commoner locals in
all survival/attack checks. Do not add a capture pre-filter at the same time.

### 5.3 [HIGH] Specialize the legality path by move category

**Expected:** ~2–4 % if implemented cleanly.
**Risk:** Medium; must preserve exact semantics and avoid the layout regression seen with
`EVASIONS`.
**Effort:** Medium.

`Board::legal` currently contains castling, en-passant, capture blast, commoner-move, pin, and
pseudo-royal logic in one large function. Most calls only need one category. A dispatch in
`generate_legal_with_state` could call smaller helpers:

- `legal_castling`
- `legal_en_passant`
- `legal_capture` (normal capture + blast)
- `legal_commoner_move`
- `legal_quiet` (pinned or in check)

Keep the public `Board::legal` wrapper for API compatibility. The goal is not a new generator;
it is to shrink the hot non-check path and avoid executing unrelated branches. Measure on both
#13 and #14 before accepting.

### 5.4 [MEDIUM] Boolean attack probe with early exit

**Expected:** ~0.5–1 % when combined with the local legal cache.
**Risk:** Low.
**Effort:** Small.

Add a private `square_attacked_by(board, sq, occupied, enemy_bb) -> bool` that checks pawns,
knights, bishops/queens, then rooks/queens and returns on the first hit. Use it in the pseudo-royal
loop and castling path. The standalone measurement showed ~0.9 % on #13 and ~0.6 % on #14, so this
is a secondary win, not a headline change.

### 5.5 [MEDIUM] Bulk pawn generation, but only with an ordering plan

**Expected:** ~2–4 % on pawn-heavy/check-heavy positions; less on #14.
**Risk:** Medium due to move-order contract.
**Effort:** Medium.

`generate_pawn_moves_for` is ~16 % of child time on #13 and is called once per pawn. Fairy-Stockfish
generates all pawn pushes/captures with bitboard shifts (`movegen.cpp:135–198`). The Rust code
currently does per-pawn `pop_lsb`, `Square::from_index`, file/rank table lookups, and bounds checks.

The obstacle is `tests/move_order_golden.rs`, which pins the exact pseudo-legal move order. A
bitboard-target-first generator naturally changes that order. Options:

1. Implement a perft-only bulk pawn generator and leave public move generation unchanged.
2. Change public order intentionally and update the goldens, after a decision that ordering is
   not part of the API contract.
3. Keep per-pawn iteration but precompute per-pawn attack targets more cheaply; this captures less
   of the benefit.

Do not silently break the order-golden tests.

### 5.6 [MEDIUM] Retry commoner-bitboard caching alone, not with a pre-filter

**Expected:** ~1–2 %.
**Risk:** Low–medium.
**Effort:** Small–medium.

`StateInfo` currently stores only commoner counts, so `legal` and `populate_state` repeatedly
recompute `by_color & by_type`. Caching `our_commoners`/`them_commoners` in `StateInfo` is still a
reasonable idea, but `report2.md` bundled it with a capture blast pre-filter and measured a 5.17 %
regression. Retry the cache **alone**, and keep the local legal cache from §5.2 first. If the
combination does not beat the local cache, stop.

### 5.7 [LOW–MEDIUM] `compute_pinned` occupancy-delta + `is_one`

**Expected:** ~0.5–1.5 %.
**Risk:** Low.
**Effort:** Small.

Use the Fairy-Stockfish `slider_blockers` pattern:

```rust
let occ = occupied ^ snipers;
let between = between_bb(ksq, sniper_sq) & occ;
if !between.is_empty() && !between.more_than_one() {
    pinned |= between & self.pieces_color(us);
}
```

Also consider precomputed empty-board rook/bishop attacks for the sniper discovery calls.
`compute_pinned` was only ~1.3 % of #13 child time, so this is polish, not a primary target.

### 5.8 [LOW] Cache `occupied` in `Board`

**Expected:** ~0.5–1 %.
**Risk:** Low, but requires careful incremental maintenance in move/remove/place and FEN setup.
**Effort:** Medium.

`occupied()` is called from generation, legality, state population, and perft. A maintained
`occupied` field would remove repeated ORs, but it adds writes to every make/unmake. Measure against
the local legal cache before keeping it.

### 5.9 [LOW] Precompute empty-board slider attacks

**Expected:** ~0.5 %.
**Risk:** Low.
**Effort:** Small.

`compute_pinned` calls `rook_attacks(sq, EMPTY)` and `bishop_attacks(sq, EMPTY)` for each commoner.
These depend only on the square and can be `const` arrays, as Fairy-Stockfish does with its
`PseudoAttacks` tables.

### 5.10 [LOW/OUT-OF-BOX] Wall-clock and build-level improvements

These do not reduce per-node work but may matter for users:

- **Parallel root perft:** split the root moves across `std::thread::scope` workers. This would
  dramatically reduce `verify_perft` wall time on multi-core machines but does not change the
  single-thread algorithm and is not a substitute for per-node optimization.
- **`-C target-cpu=native`/PGO:** may give a few percent on the build machine, but it is not
  portable and should not be forced by the crate.
- **PGO/BOLT:** possible for release artifacts, but not appropriate for a library without a
  benchmark-driven build pipeline.
- **Move-order-preserving bulk generation in perft only:** see §5.5.

---

## 6. Do not retry without a new design

| Attempt | Previous result | Why / recommendation |
|---------|-----------------|----------------------|
| `EVASIONS` / `NON_EVASIONS` split | Reverted twice; ~1.7 % to ~5–6 % regression | Check path did not dominate enough to offset layout/call overhead. Any retry must preserve the hot non-check path byte-for-byte and be measured on #13/#14, not just full-suite totals. |
| `pseudoRoyals` cache + capture blast pre-filter | Reverted; 5.17 % regression | The pre-filter added work for every capture and did not skip enough `legal` calls. Cache alone may be worth retrying (§5.6). |
| `MaybeUninit` move list | This analysis: slower | `memset` is only ~1.5–2.3 %; the replacement hurts push/slice codegen. |
| Skip Zobrist hashing in perft | This analysis: no gain | Hash work is not the bottleneck, and removing it perturbs layout. |
| Remove `Square::NONE` branch in `piece_on` | This analysis: no gain | Branch is predictable; the array load dominates. |
| Batched `MoveList` local counter | This analysis: significant regression | The compiler already inlines the simple push better. |

---

## 7. Comparison with Fairy-Stockfish

| Area | `atomic-movegen` | Fairy-Stockfish | Relevant note |
|------|------------------|-----------------|---------------|
| Legal generation | Pseudo-legal list + in-place legality compaction | `generate<EVASIONS/NON_EVASIONS>` + in-place legality compaction | FS splits generation; previous Rust attempts to copy this regressed. |
| Perft/legal list writes | `perft` uses `generate_legal`, so it pays the compaction | Engine search uses generated lists directly | The measured no-compaction perft fast path avoids this specific Rust cost. |
| Pawn generation | Per-pawn `pop_lsb` + square/file/rank helpers | Bulk bitboard shifts (`movegen.cpp:135–198`) | Potential 2–4 %, but conflicts with the pinned move-order tests. |
| Commoner/pseudo-royal state | Counts only; bitboards recomputed as needed | `st->pseudoRoyals` cached in `StateInfo` (`position.cpp:578–630`) | Cache alone may help; previous combined attempt regressed. |
| Pinned computation | `between.count() == 1` over occupancy including snipers | `slider_blockers` with `occupancy ^ snipers` and `more_than_one` | Small remaining gap (§5.7). |
| Attack probing | Full `attackers_to` bitboard, then emptiness test | `attackers_to` with attacker-candidate filtering and early legal special cases | Boolean early exit is small but safe. |
| Move list | Stack array with bounds-checked `push` | Raw `ExtMove*` pointer, no zeroing | Rust `MoveList` overhead is visible (~3–4 % on #14), but attempts to change it have not helped yet. |
| State setup | Recomputes checkers/pinned/counts per node | Recomputes `set_check_info` per `do_move` | Similar architecture; no obvious large win left. |

The reference is not simply "faster because of EVASIONS." It combines targeted generation, raw
move-list pointers, bulk pawn shifts, and precomputed state. Some of those ideas transfer to Rust;
some have already failed because the Rust code layout or tests constrain them.

---

## 8. Recommended implementation order

1. **Perft fast path without compaction** (§5.1). It is measured, low-risk, and isolated to
   `src/lib.rs`.
2. **Local bitboard cache in `Board::legal`** (§5.2). Very low risk, measured.
3. Re-baseline and profile both #13 and #14. If the combination is not additive, stop and
   investigate the next profile rather than stacking more changes.
4. Add the **boolean `square_attacked_by` early exit** (§5.4).
5. Attempt **category-specialized legality** (§5.3) only with before/after measurements on both
   fixed FENs and the full suite.
6. Consider **bulk pawn generation** (§5.5) only after deciding the move-order policy.
7. Polish with `compute_pinned`/empty-board attacks/`occupied` cache if the profile still shows
   those costs.

A conservative cumulative estimate for items 1–4 is **~5–7 %**; adding a successful specialist
legality split could reach **~7–9 %**. These estimates are much less certain than the direct
measurements because the full-suite environment is noisy and code-layout interactions have
repeatedly surprised earlier attempts.

---

## 9. Limitations of this analysis

- Hardware performance counters (cycles, instructions, cache, branch misses) were unavailable in
  the virtualized environment.
- The full `verify_perft` suite is noisy on this host; fixed-FEN depth-6 runs were used for A/B
  decisions, and full-suite totals should be treated as approximate.
- `perf` cannot resolve all inlined Rust frames perfectly; the source-line attribution is
  approximate. The `Board::legal` symbol is reliable because it is not inlined.
- Experiments were temporary and reverted. The final tree passes `cargo test`; no library changes
  from this analysis remain.

## 10. Files and commands used

Read/analyzed:

- `src/lib.rs`, `src/board.rs`, `src/movegen.rs`, `src/types.rs`, `src/attacks.rs`,
  `src/magic.rs`, `src/bitboard.rs`, `src/zobrist.rs`
- `docs/plans/perf_analysis6/analysis.md`, `plan1.md`, `plan2.md`, `plan3.md`,
  `report1.md`, `report2.md`, `report3.md`
- `docs/plans/perf_analysis5/analysis.md`, `report1.md`
- `docs/plans/perf_analysis4/report1.md`
- `docs/plans/perf_analysis3/analysis.md`, `report1.md`
- `docs/plans/cleanup/report-code-cleanup4.md`
- `tests/perft_values.md`, `tests/move_order_golden.rs`
- `Fairy-Stockfish/src/movegen.cpp`, `src/position.cpp`, `src/position.h`

Commands:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release --example perft --example verify_perft
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft

perf record -F 997 -g --call-graph dwarf -o /tmp/perf7.data \
  target/profiling/examples/perft \
  'rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5' 6

perf report -i /tmp/perf7.data --stdio --no-children --sort=symbol
perf report -i /tmp/perf7.data --stdio --children --sort=symbol
perf report -i /tmp/perf7.data --stdio --no-children --sort=srcline
perf annotate -i /tmp/perf7.data --stdio --symbol=atomic_movegen::perft

cargo run --release --example verify_perft
target/release/examples/perft '<FEN>' 6
cargo test
```
