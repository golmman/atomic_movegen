# Performance Analysis 6 — `atomic-movegen`

## Current baseline

| Metric | Value |
|--------|-------|
| Machine | Apple M4 Pro, macOS aarch64 |
| `verify_perft 6` (41 positions, depths 1–6) | **55.941 s** (all pass) |
| Slowest test | #13 (8.651 s), #33 (7.961 s), #2 (6.842 s) |
| Profiled position | Test #14: `r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15` |
| Perft result for profiled position | 245,931,633 nodes at depth 6 in **1.03 s** (`target/release/examples/perft`) |

This is faster than the last M4 snapshot in `docs/perf/m4/2026-07-09.txt` (59.194 s) and the review1 plan2 report (56.739 s), because the review1 cleanup (plan3–plan5) removed more dead code and tightened `Board`/`Move`/`Square` helpers without changing the move logic.

## Methodology

- `perft_profile.txt` was produced by `sample` on the `profiling` profile binary (`RUSTFLAGS="-C force-frame-pointer=yes" cargo build --profile profiling --example perft`).
- Source line numbers were verified with `atos`.
- Previous reports `docs/plans/perf_analysis5/analysis.md` and `docs/plans/perf_analysis5/report1.md` were read; the `EVASIONS` implementation attempted there is included here with a different, less invasive design.
- Fairy-Stockfish source was used as the oracle: `src/movegen.cpp`, `src/position.cpp` (`set_check_info`, `slider_blockers`, `attackers_to`, `Position::legal`).

## What `perft_profile.txt` says

The `perft` thread spent 922 samples in the dispatch queue. The main CPU consumers are:

| Symbol (top of stack) | Samples | Share | Source / meaning |
|-----------------------|---------|-------|------------------|
| `perft` | 704 | 76.3 % | `lib.rs` recursion; line 65 is the inlined `generate_legal` body (589 samples) |
| `Board::legal` | 200 | 21.7 % | `src/board.rs` full legality check for non-trivial moves |
| `platform_memset` / `__bzero` | 15 | 1.6 % | `MoveList::new`/`StateInfo::new` zeroing per node |
| `mach_msg2_trap` etc. | ~5 | — | idle / system noise |

`generate_legal` (589 samples) + `Board::legal` (200 samples) = **789 samples, 85.5 %** of the sampled thread time. Everything else (`do_move`/`undo_move` 59 samples, zeroing 15 samples, `attacks::init` etc.) is small in comparison.

### Hot lines inside `Board::legal`

From `src/board.rs` lines 962–1058, the most expensive call sites are:

| Line | Samples | What it does |
|------|---------|--------------|
| 1049 | 48 | `attackers_to(self, ksq, occupied, enemy_survivors) != Bitboard::EMPTY` |
| 1058 | 16 | final `true` return (post-loop) |
| 968 | 14 | `piece == NO_PIECE` check |
| 1016 | 10 | `occupied = occupied \| Bitboard::square_bb(kto)` (blast/capture setup) |
| 1047 | 21 | `if adjacent_enemy.is_empty() && ...` |
| 1023 | 6 | `our_commoners = self.commoners(us) & occupied` |
| 1046 | 6 | `adjacent_enemy = them_commoners & attacks::king_attacks(ksq)` |
| 1036 | 6 | `if our_pr_count <= 1` |

`Board::legal` is dominated by the **post-blast attack detection** on the pseudo-royal commoner(s): building `occupied`, fetching `commoners`, and then `attackers_to` (line 1049). The 48 samples at line 1049 are the *call site*; the actual `rook_attacks`/`bishop_attacks`/`knight_attacks`/`pawn_attacks` are inlined and not visible as separate symbols in the `sample` output.

### What the `sample` output does not show

`sample` on ARM64 does not break out inlined functions. Important costs are therefore hidden inside the top-level symbols:

- `OnceLock::get` for `magic::ROOK_TABLE`/`BISHOP_TABLE` is inlined into `attackers_to` and `generate_pseudo_legal`. It is not visible, but it is present on every `rook_attacks`/`bishop_attacks` call.
- `generate_pseudo_legal` (and `generate_pawn_moves_for`) is inlined into `perft` line 65. The individual pawn/sliding loops are not visible.
- `MoveList::push` is inlined into `perft` line 65.

The previous x86_64 `perf` run in `perf_analysis5` showed ~4.4 % of time in `OnceLock::get` for magic tables alone. With `sample` inlining hiding it on ARM, the real cost is likely at least that, and possibly larger because the profiled FEN is a non-check, sliding-piece-rich middlegame position.

## Comparison with Fairy-Stockfish

| Area | `atomic-movegen` | Fairy-Stockfish | Gap |
|------|-----------------|-----------------|-----|
| Attack table init | `OnceLock` for magic, `const` for leapers | `extern` static arrays, init once | Remove `OnceLock` for magic (build-time or `Box::leak` static) |
| Pseudo-royal cache | `StateInfo` only stores `commoners_count` and `them_commoners_count` | `st->pseudoRoyals` and `st->pseudoRoyalsTheirs` full `Bitboard` | Cache full `our_commoners`/`them_commoners` bitboards |
| Checkers/pinned | `compute_checkers` + `compute_pinned` separate; `between.count() == 1` | `slider_blockers` with `occupancy ^ slidingSnipers` and `!more_than_one(b)` | Adopt occupancy-delta and `is_one` |
| Move generation | Always `generate_pseudo_legal` + legality compaction | `generate<EVASIONS>`/`generate<NON_EVASIONS>` split | Add evasions path, carefully |
| Pawn generation | Per-pawn `pop_lsb` with `Square::from_index` | `shift<Up>(pawns)` bulk bitboards | Bulk bitboard pawn generation |
| `legal` attack check | `attackers_to(...) != Bitboard::EMPTY` (full OR) | `attackers_to(...)` then `bool` on `& attackerCandidates` | Use `is_square_attacked` early-exit bool |
| `MoveList` | Stack array `[Move; 256]` zeroed per node | Raw `ExtMove*` pointer, no zeroing | `MaybeUninit` or callback pattern |

The reference is fast because it does **single lookups** (no lazy-atomic), **precomputes everything**, and **prunes the search space algorithmically** (evasions). The Rust code already has `const` leaper tables and `BETWEEN_BB`; the remaining gaps are the magic-table `OnceLock`, the `pseudoRoyals` bitboard, the evasions split, and the `legal`/`attackers_to` micro-optimizations.

## Potential performance improvements

Sorted by estimated impact on `verify_perft 6` on the current M4 machine, with risk and effort.

### 1. [CRITICAL] Remove `OnceLock` from magic tables

**Problem:** `magic::ROOK_TABLE` and `BISHOP_TABLE` are `std::sync::OnceLock<&[Bitboard]>`; every `rook_attacks`/`bishop_attacks` does `ROOK_TABLE.get().expect(...)` (an `Acquire` atomic load + branch). These calls are made by `generate_pseudo_legal` for rook/queen/bishop moves, `compute_checkers`, `compute_pinned`, and `attackers_to` → `Board::legal`. With 245 M nodes and roughly 4–6 magic lookups per `legal` call, the aggregate `OnceLock` tax is large.

**Solutions, in order of preference:**

1. `build.rs` precomputation: compute the tables at compile time and emit `src/magic_tables.rs` (or `include!` from `OUT_DIR`) as a `static` array. No runtime init, no atomic, and fully safe.
2. `Box::leak` with an unsafe `static mut` pointer write: simplest to implement, but requires `unsafe` and a clear `AGENTS.md` note.
3. `std::sync::Once` + `&'static [Bitboard]` (similar to `ctor`) — still an atomic, but paid once per call rather than per lookup.

**Estimated speedup:** 3–8 % (possibly more because `sample` hides the inlined cost).
**Risk:** Low. Tables are already correctness-tested.
**Fairy-Stockfish precedent:** `extern Bitboard PseudoAttacks[...]`; plain indexed loads.

### 2. [HIGH] Cache `pseudoRoyals` bitboards and add a capture blast-illegal pre-filter

**Problem:** `Board::legal` calls `self.commoners(us)` and `self.commoners(them)` several times per call, each `commoners` doing `by_color[...] & by_type[Commoner]`. More importantly, `generate_legal` calls `Board::legal` for *every* capture, and in atomic many captures are illegal because the blast kills the last commoner.

**Fix:**

- Add `our_commoners: Bitboard` and `them_commoners: Bitboard` to `StateInfo`.
- `populate_state` computes them once and uses them in `compute_checkers`/`compute_pinned`.
- In `generate_legal`, before the `is_move_trivially_legal || Board::legal` check, add an `is_trivially_illegal` fast reject for captures:

```rust
if is_capture && m.move_type() != MoveType::Castling {
    let from_bb = Bitboard::square_bb(from);
    let survivors = (state.our_commoners & !from_bb) & !attacks::king_attacks(to);
    if survivors.is_empty() {
        continue; // blast destroys all our commoners
    }
}
```

- Use `state.our_commoners`/`state.them_commoners` inside `Board::legal` instead of recomputing `self.commoners`.

**Estimated speedup:** 3–7 % (capture pre-filter can cut `Board::legal` calls by 20–40 % in single-commoner positions; cache removes repeated `commoners` bitboard work).
**Risk:** Medium. The blast logic must handle the moved piece being the commoner (`from` is no longer occupied) and the `to` square not being in `our_commoners`.
**Fairy-Stockfish precedent:** `st->pseudoRoyals` and `st->pseudoRoyalsTheirs` used in `Position::legal`.

### 3. [HIGH] EVASIONS / NON_EVASIONS split (second attempt with a different layout)

**Problem:** `generate_legal` always calls `generate_pseudo_legal` and then filters. When in check, most pseudo-legal moves are illegal and are rejected by `is_move_trivially_legal`/`Board::legal`. The slow `verify_perft` tests (#2, #13, #33) are check-heavy. The previous attempt in `perf_analysis5` regressed 5–6 % because the branch and the `generate_evasions` code changed inlining/layout of the hot non-check path.

**Fix:** Keep the non-check code path byte-identical to today:

- Mark `generate_legal` `#[inline(never)]` so `perft` does not inline the whole generator; the non-check body remains the same sequence of `populate_state`, `generate_pseudo_legal`, and compaction.
- Move the check path into a `#[inline(never)]` `generate_legal_in_check` (or `generate_evasions`) in `movegen.rs`/`board.rs`.
- In `generate_legal`:

```rust
if state.checkers.is_empty() {
    generate_pseudo_legal(board, moves);
    // inline compaction
} else {
    generate_legal_in_check(board, &state, moves); // evasions only
}
```

- `generate_legal_in_check` should:
  - double check → only commoner moves (or none if no commoners)
  - single slider check → blocks between commoner and checker + captures
  - single leaper check → captures only
  - use `between_bb` from the already-const `BETWEEN_BB`

**Estimated speedup:** 4–10 % on `verify_perft` (up to 20 % on the check-heavy tests, offset by call/branch overhead).
**Risk:** High. The previous attempt failed due to code layout; this needs careful measurement, `#[inline(never)]`, and possibly `#[cold]`.
**Fairy-Stockfish precedent:** `movegen.cpp:510–526` `pos.checkers() ? generate<EVASIONS>(...) : generate<NON_EVASIONS>(...)`.

### 4. [HIGH] `is_square_attacked` boolean early-exit in `Board::legal`

**Problem:** `Board::legal` line 1049 calls `attackers_to(...)` and checks `!= Bitboard::EMPTY`. The function computes a full `Bitboard` OR of all attackers. `legal` only needs a `bool`, and in the castling pass-through check it also ORs in commoner adjacency separately.

**Fix:** Add a private `is_square_attacked(board, sq, occupied, enemy_bb) -> bool` that:

- checks pawns, knights, rook-attacks, bishop-attacks, and commoner attacks in order
- returns `true` as soon as any attacker is found
- replaces `adjacent_enemy.is_empty() && attackers_to(...) != EMPTY` in `legal`
- replaces the castling `attackers_to` + commoner-`king_attacks` OR

**Estimated speedup:** 2–5 % (reduces `Board::legal` cost).
**Risk:** Low/Medium. Must keep commoner adjacency semantics.
**Fairy-Stockfish precedent:** `Position::attackers_to` returns a `Bitboard`, but `Position::legal` ultimately uses it as a boolean (`attackers_to(...) & ...` then `if`); an early-exit bool is a Rust-specific micro-optimization.

### 5. [MEDIUM] Bulk bitboard pawn generation

**Problem:** `generate_pawn_moves_for` is called per pawn and uses `Square::from_index` for every push/capture. It is the commonest piece type and is on the hot path inside `generate_pseudo_legal`.

**Fix:** Compute target bitboards with shifts:

```rust
let pawns = board.pieces_color_pt(us, PieceType::Pawn);
let empty = !board.occupied();
let enemies = board.pieces_color(them);

let single_push = shift(pawns, push_dir) & empty;
let double_push = shift(shift(pawns & start_rank, push_dir) & empty, push_dir) & empty;
let cap_left = shift(pawns & !file_bb(0), push_dir + left) & enemies;
let cap_right = shift(pawns & !file_bb(7), push_dir + right) & enemies;
// then pop_lsb over each target set and generate moves
```

This removes the per-pawn loop and the `Square::from_index` bounds checks.

**Estimated speedup:** 2–5 %.
**Risk:** Low. Standard bitboard idiom.
**Fairy-Stockfish precedent:** `movegen.cpp` `shift<Up>(pawns)` / `shift<UpRight>(pawns)`.

### 6. [MEDIUM] `MoveList` with `MaybeUninit` to avoid zeroing

**Problem:** `perft_profile.txt` shows `platform_memset`/`__bzero` 15 samples (1.6 %). `MoveList::new` fills `[Move::NONE; 256]` (512 bytes) per node. `StateInfo::new` also zeroes the `captured` array.

**Fix:** Change `MoveList` storage to `[MaybeUninit<Move>; MAX_MOVES]` and `len: usize`; `as_slice` returns `std::slice::from_raw_parts` on the initialized prefix. `new()` only sets `len = 0`. `Move` is `Copy` and small, so `MaybeUninit` is safe.

**Estimated speedup:** 1–2 % (more on deeper searches).
**Risk:** Low/Medium. Requires `Debug`/`Clone` derives to be replaced or kept manually.
**Fairy-Stockfish precedent:** C++ `ExtMove*` list; no zeroing.

### 7. [MEDIUM] `compute_pinned` occupancy-delta + `is_one` instead of `count() == 1`

**Problem:** `compute_pinned` uses `between.count() == 1` (popcount) and includes all snipers in `occupied`, which can let one sniper block the line to another. `Bitboard::more_than_one` was removed in review1 plan4.

**Fix:**

```rust
let occ = occupied ^ snipers; // remove snipers before the between test
let between = between_bb(ksq, sniper_sq) & occ;
if !between.is_empty() && !between.more_than_one() {
    pinned |= between;
}
```

Add `Bitboard::is_one()` and/or `Bitboard::more_than_one()` helpers.

**Estimated speedup:** 1–3 %.
**Risk:** Low.
**Fairy-Stockfish precedent:** `slider_blockers` with `occupancy = pieces() ^ slidingSnipers` and `if (b && !more_than_one(b))`.

### 8. [MEDIUM] Precompute empty-board slider attacks

**Problem:** `compute_pinned` and `compute_checkers` call `attacks::rook_attacks(sq, EMPTY)` and `attacks::bishop_attacks(sq, EMPTY)` for each commoner. The result depends only on `sq`, so it can be a `const` array.

**Fix:** Add `const ROOK_ATTACKS_EMPTY: [Bitboard; 64]` and `BISHOP_ATTACKS_EMPTY: [Bitboard; 64]`, computed from `sliding_attack` at compile time, and use them for sniper discovery and `compute_checkers`/`compute_pinned`.

**Estimated speedup:** 0.5–2 %.
**Risk:** Low.
**Fairy-Stockfish precedent:** `PseudoAttacks` arrays for empty-board attacks.

### 9. [LOW] Hoist `by_color` loads in `generate_pseudo_legal`

**Problem:** `generate_pseudo_legal` calls `board.pieces_color_pt(us, PieceType::Pawn)` etc. for each piece type. `pieces_color_pt` loads `by_color[us]` every time.

**Fix:**

```rust
let our_pieces = board.pieces_color(us);
let our_pawns = our_pieces & board.by_type[PieceType::Pawn as usize];
let our_knights = our_pieces & board.by_type[PieceType::Knight as usize];
// ...
```

**Estimated speedup:** 1–2 %.
**Risk:** None.
**Fairy-Stockfish precedent:** `pos.pieces(Us)` loaded once and reused.

### 10. [LOW] `Square::from_index_unchecked` in hot pawn paths

**Problem:** `Square::from_index` checks `0..64` for every pawn push/capture target. In `generate_pawn_moves_for` the target file/rank is already validated; the bounds check is redundant.

**Fix:** Add `pub(crate) unsafe fn from_index_unchecked` (or `from_u8_unchecked`) and use it after the explicit file/rank checks. This is one of the few places where `unsafe` is justified by a clear, measurable win.

**Estimated speedup:** 1–2 %.
**Risk:** Low (`unsafe` localized).
**Fairy-Stockfish precedent:** `Square`/`make_square` is an integer in C++.

### 11. [LOW] Cache `occupied` in `Board`

**Problem:** `Board::occupied()` computes `by_color[0] | by_color[1]` every call. It is called by `generate_pseudo_legal`, `compute_checkers`, `compute_pinned`, and `Board::legal`.

**Fix:** Add `occupied: Bitboard` to `Board` and update it in `move_piece`/`remove_piece`/`place_piece`.

**Estimated speedup:** 0.5–1 %.
**Risk:** Low.
**Fairy-Stockfish precedent:** Maintains `byTypeBB[ALL_PIECES]` incrementally.

### 12. [LOW] `compute_checkers` adjacent-commoner check with bitboard shifts

**Problem:** `compute_checkers` loops over enemy commoners and checks `king_attacks(tksq) & commoners` one by one.

**Fix:** Compute all squares adjacent to enemy commoners in one expression with `shift` operations, then `&` with our commoners.

**Estimated speedup:** 0.5–1 %.
**Risk:** Low.
**Fairy-Stockfish precedent:** N/A (atomic-specific).

### 13. [LOW] Remove `MoveList::push` bounds check

**Problem:** `MoveList::push` checks `if self.len < MAX_MOVES` on every push. The upper bound is 256 and legal positions never reach it.

**Fix:** Use an unchecked pointer write with `debug_assert!(self.len < MAX_MOVES)`.

**Estimated speedup:** 0.5–1 % (current profile shows only 3 samples in `types.rs:0`, so the win is modest).
**Risk:** Low (`unsafe` localized).
**Fairy-Stockfish precedent:** `moveList++` pointer increment.

### 14. [LOW] Fuse `compute_checkers` and `compute_pinned` in `populate_state`

**Problem:** Both functions iterate over commoners and snipers and recompute `our_commoners`/`them_commoners`.

**Fix:** With the bitboards cached in `StateInfo`, `populate_state` can compute `checkers` and `pinned` in one pass, or at least pass `our_commoners`/`them_commoners` to both.

**Estimated speedup:** 0.5–1.5 %.
**Risk:** Low.
**Fairy-Stockfish precedent:** `set_check_info` computes `blockersForKing` and `checkersBB` together.

### 15. [LOW] Avoid `StateInfo` zeroing of `captured`

**Problem:** `StateInfo::new` zeroes the `captured` array. `do_move` sets `captured_count = 0` and `undo_move` only reads up to `captured_count`.

**Fix:** Use `MaybeUninit` for the `captured` array or document that only `captured[0..captured_count]` is valid.

**Estimated speedup:** 0.5–1 %.
**Risk:** Low.

## Out-of-the-box ideas

- **`for_each_legal_move` callback / generator pattern**: Instead of building a `MoveList` and then compacting it, expose a `for_each_legal_move(board, &mut f)` that takes a closure. `perft` can recurse directly inside the closure, avoiding `MoveList` allocation, zeroing, and the compaction loop. This is the most invasive change and can hurt inlining if the closure is not monomorphized; benchmark carefully.
- **Parallel `perft`**: Split the root moves across threads. This is not a single-thread speedup but would dramatically improve wall-clock time on the M4 (and especially on the 5950X). Not a substitute for per-node work reduction.
- **PEXT for Intel x86-64**: Re-add a PEXT path using `core::arch::x86_64::_pext_u64` behind `target_feature` and a non-default `pext` feature. Useful for the 5950X/Linux machine, not for the M4.
- **Precomputed `is_square_attacked` for commoner squares**: Too expensive in memory, but a small cache of `attackers_to` for the most recent commoner square could help in the `legal` loop for non-capture moves; correctness and eviction policy make it risky.

## Summary table

| # | Improvement | Est. speedup | Risk | Effort | Fairy-Stockfish / note |
|---|-------------|--------------|------|--------|------------------------|
| 1 | Remove `OnceLock` from magic tables | 3–8 % | Low | Medium | Build-time arrays |
| 2 | Cache pseudoRoyals + capture blast pre-filter | 3–7 % | Medium | Medium | `st->pseudoRoyals` |
| 3 | EVASIONS / NON_EVASIONS split | 4–10 % | High | High | `generate<EVASIONS>` |
| 4 | `is_square_attacked` bool early-exit | 2–5 % | Low/Medium | Low | `attackers_to` boolean use |
| 5 | Bulk bitboard pawn generation | 2–5 % | Low | Medium | `shift<Up>(pawns)` |
| 6 | `MoveList` with `MaybeUninit` | 1–2 % | Low/Medium | Low | C++ raw list |
| 7 | `compute_pinned` occupancy-delta + `is_one` | 1–3 % | Low | Low | `slider_blockers` |
| 8 | Precompute empty-board slider attacks | 0.5–2 % | Low | Low | `PseudoAttacks` |
| 9 | Hoist `by_color` in `generate_pseudo_legal` | 1–2 % | None | Low | `pos.pieces(Us)` |
| 10 | `Square::from_index_unchecked` | 1–2 % | Low | Low | `unsafe` localized |
| 11 | Cache `occupied` in `Board` | 0.5–1 % | Low | Low | `byTypeBB[ALL_PIECES]` |
| 12 | `compute_checkers` shift-based adjacency | 0.5–1 % | Low | Low | atomic-specific |
| 13 | Remove `MoveList::push` bounds check | 0.5–1 % | Low | Low | `unsafe` localized |
| 14 | Fuse `compute_checkers`/`compute_pinned` | 0.5–1.5 % | Low | Low | `set_check_info` |
| 15 | Avoid `StateInfo` `captured` zeroing | 0.5–1 % | Low | Low | — |

## Cumulative potential

If the top four items (1–4) are implemented and their effects stack multiplicatively:

```
1 - (1-0.05)(1-0.05)(1-0.07)(1-0.035)
≈ 1 - 0.95 * 0.95 * 0.93 * 0.965
≈ 1 - 0.808
≈ 19 %
```

That would bring the current 55.9 s `verify_perft 6` down to roughly **45 s**. Adding the medium items (5–8) could push it toward **40 s**, but the uncertainty grows because the lower items touch overlapping code and the compiler may already have optimized some of them.

## Recommended next steps

1. **Do item 1 first (`OnceLock` removal)**. It is the safest remaining large win and reduces the hidden cost in `Board::legal` and `generate_pseudo_legal`.
2. **Do item 2 in parallel** (cache pseudoRoyals + capture blast pre-filter). It is independent and directly targets the `Board::legal` bottleneck shown by `sample`.
3. **Benchmark the combination of 1+2** before touching the EVASIONS split. The EVASIONS change is the most likely to cause layout regressions and should be done with a clean baseline.
4. **Re-attempt EVASIONS (item 3)** only with `#[inline(never)]` on both `generate_legal` and the check path, and a micro-benchmark on the three slow check-heavy tests (#2, #13, #33).
5. **Pursue items 5–8** for the remaining medium wins after the top four.
6. **Items 9–15** are small polish; do them only if they survive a clean benchmark with no noise.

## Files read for this analysis

- `perft_profile.txt` (the `sample` output)
- `docs/plans/perf_analysis5/analysis.md` and `report1.md`
- `docs/plans/review1/plan2.md` through `report5.md`
- `docs/perf/m4/2026-07-09.txt` and `2026-07-09_plan2.txt`
- `src/lib.rs`, `src/board.rs`, `src/movegen.rs`, `src/types.rs`, `src/magic.rs`, `src/attacks.rs`
- `Fairy-Stockfish/src/movegen.cpp`, `Fairy-Stockfish/src/position.cpp`
