# Report 5 — Order-Preserving, Table-Driven Pawn Generation

**Plan:** `docs/plans/perf_analysis7/plan5.md` (item 6 of `analysis.md`, §5.5 *bulk pawn generation,
but only with an ordering plan*, **option 3**)
**Result:** **KEPT.** Test #13 median **11 767 ms → 10 954 ms (−6.9 %)**, test #14 median
**1 497 ms → 1 393 ms (−6.9 %)**, node counts bit-identical, move order byte-identical, perft 41/41.
The gain is ~3× the plan's ~1–2.5 % estimate and was confirmed twice with a same-session A/B revert.

---

## 1. What changed

Only `src/attacks.rs` and `src/movegen.rs` in the library, plus additive goldens in
`tests/move_order_golden.rs`.

```text
 src/attacks.rs             | 183 +++++++++++++++++++++++++++++++++++++++++++++
 src/movegen.rs             | 109 +++++++++++++--------------
 tests/move_order_golden.rs |  63 ++++++++++++++++
 3 files changed, 296 insertions(+), 59 deletions(-)
```

No removals in `tests/move_order_golden.rs` — the file gained six positions, the ten pre-existing
goldens are untouched. `src/lib.rs` (`perft`), `src/board.rs`, `src/types.rs`, `src/bitboard.rs`
and `examples/` are unchanged, so the Plan 1 rollback and the Plan 3 `square_attacked_by` early exit
are undisturbed.

### 1.1 `src/attacks.rs`

Two new compile-time tables next to `PAWN_ATTACKS`, using the same `[square][color]` indexing
convention, each built by a `const fn` in the existing style:

- `compute_pawn_pushes() -> [[Bitboard; 2]; 64]` / `PAWN_PUSHES` — one target per square, empty on
  the pawn's last rank.
- `compute_pawn_double_pushes() -> [[Bitboard; 2]; 64]` / `PAWN_DOUBLE_PUSHES` — a target only on
  the pawn's start rank (white rank 2 index 1, black rank 7 index 6), empty everywhere else.

Both are exposed through `pub(crate)` `#[inline(always)]` accessors `pawn_pushes(c, sq)` and
`pawn_double_pushes(c, sq)`, mirroring `pawn_attacks`. Since they are `pub(crate)` only, they add
nothing to the documented public surface; the tables are total over all 64 squares so indexing is
always in bounds.

Four new unit tests in `attacks.rs::tests`:

- `test_pawn_pushes_start_rank` — white `d2d3`/`d2d4`, black `e7e6`/`e7e5`, and no cross-colour
  double push.
- `test_pawn_pushes_non_start_rank` — seventh-rank pawns push but do not double push; mid-board
  pawns push one square in the correct direction for both colours.
- `test_pawn_pushes_promotion_and_impossible_ranks` — empty on the last rank, single push (never a
  double push) onto the promotion rank, for both colours.
- `test_pawn_attacks_table` — the pre-existing `PAWN_ATTACKS` table (the plan assumed a pawn-attack
  test already existed; it did not) now has one that also pins the no-wrap behaviour of edge pawns.

### 1.2 `src/movegen.rs`

`for_each_pseudo_legal` hoists the per-node values instead of re-reading them per pawn:

```rust
let them_bb = board.pieces_color(them);
let ep_square = board.ep_square();
let target = !occupied | them_bb;
...
generate_pawn_moves_for(board, us, occupied, them_bb, ep_square, from, f)?;
```

`generate_pawn_moves_for` is fully table-driven:

| Stage | Old | New |
|-------|-----|-----|
| Single push | `from as i8 + 8`, `Square::from_index` bounds check, `board.empty(to)` array probe, `rank_of(to) == promo` | `pawn_pushes(us, from) & !occupied`, `promoting = rank_of(from) == R7/R2` |
| Double push | `+16`, bounds check, `board.empty`, start-rank test | `pawn_double_pushes(us, from) & !occupied`, gated on the single push |
| Captures | `for df in [-1, 1]` with file-range guard, wrap guard (`file_of(to) == target_f`), `board.pieces_color(them) & square_bb(to)` rebuilt per target | `pawn_attacks(us, from) & them_bb`, `pop_lsb` loop |
| En passant | per-pawn `board.ep_square()`, file arithmetic, `to_sq == ep_sq` | hoisted `ep_square`, membership `pawn_attacks(us, from) & square_bb(ep)`, unchanged enemy-pawn check |

Deleted with it: `from_idx`, `push_dir`, `push_double`, `promo_rank`, the `df` loop and all
`file_of`/`Square::from_index` arithmetic in this function. `file_of` is no longer referenced in
`movegen.rs` (the `use crate::types::*` glob means no unused-import warning appears); `rank_of` is
retained for the start-rank/promotion-rank tests only.

The signature is eight parameters deep only if `them` is passed, so `them` is derived inside via
`us.flip()` (one instruction) and the function stays at seven parameters — no
`#[allow(clippy::too_many_arguments)]` was needed.

`promoting` moved from the target rank to the origin rank as the plan specifies (equivalent: every
target of a pawn on its seventh rank is on the promotion rank, and no other pawn ever promotes).

### 1.3 One deviation from the plan: no `#[inline(always)]`

The plan's §4.2 draft carried `#[inline(always)]` on `generate_pawn_moves_for`. Measured with the
attribute removed (5 runs each, `/tmp` harness identical to the main loop):

| Variant | #13 median | #14 median |
|---------|-----------|-----------|
| with `#[inline(always)]` | 10 954 ms | 1 422 ms |
| without | 10 956 ms | 1 414 ms |

Identical within noise, so the attribute was dropped as unnecessary decoration; the final
measurement round (§3.4) was taken on the version without it and reproduces the same gain. The
`#[inline(always)]` on the small table accessors was kept as the plan requires.

---

## 2. Step 1: baseline and pre-change goldens

Baseline on the untouched Plan 4 tree (`cargo build --release --examples`), 7 runs, depth 6:

```text
#13  11654 11767 11718 11789 11653 11780 11799   median 11767 ms  (min 11653, max 11799)
#14   1477  1514  1497  1486  1490  1525  1505   median  1497 ms  (min  1477, max  1525)
```

Node counts were the expected `2 160 817 389` (#13) and `245 931 633` (#14) in every run, matching
the earlier plans. The whole-suite baseline was `41/41 passed`, total **77.654 s** (slowest #13
12.170 s).

Before any library edit, six pawn-rich positions were appended to `GOLDENS` in
`tests/move_order_golden.rs` with streams dumped from the **unmodified** implementation (throwaway
`examples/` binary, deleted afterwards, never regenerated later):

| Golden | FEN | Covers |
|--------|-----|--------|
| `perf13` | `rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5` | black pushes, `d5d4`/`f5f4` captures, both colours' pawn structure |
| `perf14` | `r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15` | black pawn with mixed capture options (`f4e3`) |
| `en_passant_white` | `8/8/8/1pP5/8/8/1k6/4K3 w - b6 0 1` | white en passant |
| `en_passant_black` | `4k3/8/8/8/3pP3/8/8/4K3 b - e3 0 1` | black en passant |
| `black_promotion_capture` | `4k3/8/8/8/8/8/1p6/R1N1K3 b - - 0 1` | black push promotion **and** two capture promotions, hence west→east order at rank 1 |
| `pawn_double_capture` | `4k3/8/2p1p3/3P4/8/8/8/4K3 w - - 0 1` | one white pawn capturing both flanks |

`cargo test --test move_order_golden` was green on the baseline tree before the rewrite, and was
never edited again. It is green afterwards with the same bytes — the strongest single check here,
because it pins the exact per-pawn, per-stage, west-before-east order for both colours.

---

## 3. Performance

All numbers are `target/release/examples/perft <FEN> 6`, wall clock around the process, on the same
host and session (AMD Ryzen 9 5950X, 4 vCPU).

### 3.1 Round 1 (first measurement after the change)

```text
#13  11136 10940 10913 11009 10936 11002 10939   median 10940 ms
#14   1392  1415  1392  1400  1403  1417  1432   median  1403 ms
```

### 3.2 A/B round (patch stashed → baseline, then re-applied → candidate)

Baseline, same session:

```text
#13  11831 11776 11748 11719 11713 11739 11714   median 11739 ms
#14   1502  1505  1517  1495  1516  1510  1502   median  1505 ms
```

Candidate, immediately after (`git stash pop` + rebuild):

```text
#13  10996 10948 11032 10954 10931 10881 11010   median 10954 ms
#14   1422  1424  1421  1388  1434  1446  1394   median  1422 ms
```

Pairwise per run (candidate − baseline), all seven pairs negative:

```text
#13  -835  -828  -716  -765  -782  -858  -704   ms   (7/7 faster)
#14   -80   -81   -96  -107   -82   -64  -108   ms   (7/7 faster)
```

Pairwise medians: **#13 −6.69 %**, **#14 −5.52 %**.

### 3.3 Full suite (context only)

| | baseline | after | delta |
|---|---------|-------|-------|
| total | 77.654 s | 73.215 s | −5.7 % |
| slowest (#13) | 12.170 s | 11.398 s | −6.3 % |
| result | 41/41 | 41/41 | — |

### 3.4 Final confirmation round (shipped version, without `#[inline(always)]`)

```text
#13  10970 10965 10943 10918 10954 10869 11096   median 10954 ms  (min 10869, max 11096)
#14   1399  1388  1393  1386  1416  1386  1432   median  1393 ms  (min  1386, max  1432)
```

Versus the session-1 baseline: **#13 11767 → 10954 ms = −6.91 %**, **#14 1497 → 1393 ms = −6.94 %**.

### 3.5 Interpretation

The plan expected 1–2.5 %; the measured gain is ~7 % on both positions, i.e. roughly 3× the
estimate and about twice analysis §5.5's ~2–4 % estimate for *order-breaking* bulk generation —
while keeping the order byte-for-byte. Two independent facts explain the size:

1. The removed work was mostly bookkeeping that the plan identified: the file-range guard, the
   wrap guard, the `Square::from_index` bounds check, and the per-target `pieces_color(them) &
   square_bb(to)` rebuild. Profiling (below) shows the pawn stage dropped by ~40 % in absolute
   per-node cost, not the ~20–30 % the plan implicitly assumed.
2. Hoisting `pieces_color(them)` and `ep_square()` out of the per-pawn loop removed two per-pawn
   `Board` reads from every node, which the plan lists as a secondary intent but did not price in.

No stable regression appeared on #14; it improved by the same factor, so the positive result is not
a #13-specific layout accident. That is notable given `report2.md` and `report4.md` both saw layout
sensitivity invert small expected gains on this host.

---

## 4. Profile evidence

Same-session `perf record -F 997 -g --call-graph dwarf` on the profiling profile, #13 at depth 6
(12 706 baseline samples, 11 806 candidate samples) — self+children share of total:

| Symbol | baseline | after |
|--------|----------|-------|
| `movegen::generate_pawn_moves_for` | 16.87 % | **10.32 %** |
| `movegen::generate_pseudo_legal` (all stages) | 35.51 % | 32.39 % |
| `movegen::generate_legal_with_state` | 58.07 % | 58.87 % |
| `board::is_move_trivially_legal` | 13.49 % | 14.33 % |

Per node, with the measured 6.7 % total speedup, pawn generation became
`(0.1032 × 0.933) / 0.1687 ≈ 0.57` of its old cost, i.e. **~43 % cheaper** — consistent with the
observed 7 % total (0.169 × 0.43 ≈ 7.3 % of run time removed).

The two pawn source lines that analysis §3.3 called out are gone from the hot-line list
(`perf report --sort=srcline`):

| Line (pre-change) | Meaning | baseline | after |
|-------------------|---------|----------|-------|
| `movegen.rs:164` | capture wrap guard `file_of(to) == target_f` | 1.95 % | — |
| `movegen.rs:155` | capture file-range guard | 0.26 % | — |
| `movegen.rs:179` | per-pawn `board.ep_square()` | 0.14 % | — |

Post-change, the only remaining pawn lines above 0.05 % are the en-passant block at
`movegen.rs:181/183` (0.05 % each). The hot lines that remain are in `generate_castling` and the
commoner/piece stages, which were out of scope.

---

## 5. Correctness verification

| Check | Result |
|-------|--------|
| `cargo fmt` | clean (`cargo fmt --check` passes) |
| `cargo clippy --all-targets` | no new warnings; only the 4 pre-existing `manual_range_contains` / `manual_contains` warnings in `examples/test_d5g8.rs` |
| `cargo test` | 58 lib + 1 golden + 4 perft + 1 verify_moves + 1 doc test, all pass |
| `cargo test --test move_order_golden` | passes with the **unchanged** pre-change goldens, including the six new pawn-rich positions |
| `cargo test --test verify_moves` | passes |
| `cargo doc` | builds, no warnings |
| `cargo run --release --example verify_perft` | **41/41** at depths 1–6, total 73.2 s |
| perft node counts #13 / #14 | `2 160 817 389` / `245 931 633` — identical in every run |

No golden, perft value or test expectation was modified after the rewrite.

---

## 6. Decision: keep

Kept, because every acceptance criterion in plan §5 Step 5 is met:

- #13 median improved by 6.9–7.0 % across three independent rounds (≥ 1 % required).
- No stable regression on #14 (−5.5 % to −6.9 %; 7/7 pairwise runs faster in the A/B round).
- Full suite 41/41 and faster (77.65 s → 73.22 s).
- Move order unchanged, verified by goldens captured before the change and never regenerated.

The change is confined to the pawn stage, keeps the public API, `Board`, `perft` and `StateInfo`
untouched, and deletes more code than it adds in `movegen.rs` (109 lines changed, net −12 lines
there; the +183 in `attacks.rs` is table construction and tests). Maintainability also improves:
the pawn generator no longer re-derives square arithmetic, and `file_of` disappeared from
`movegen.rs` entirely.

---

## 7. Problems encountered

1. **The plan assumed an existing pawn-attack test in `attacks.rs`.** There was none, so
   `test_pawn_attacks_table` was written as part of this work (it also documents the no-wrap
   behaviour that makes the table version order-safe). Worth correcting in future plans that cite
   test coverage.
2. **`#[allow(clippy::too_many_arguments)]` was avoided rather than added.** The plan's signature
   listed eight parameters; deriving `them` inside the function keeps it at seven and keeps the
   build free of suppressions.
3. **The `#[inline(always)]` in the plan's draft body is a no-op here** (measured, §1.3) and was
   dropped so the source carries no unjustified attribute.
4. **Absolute timings drift between sessions** as `report4.md` warned (session-1 baseline 11.77 s vs
   the 8.5 s of the original analysis). Every conclusion above is drawn from same-session pairs;
   the A/B round reproduced the round-1 result to within 0.2 pp.
5. **The plan suggested the hand-built FEN `8/8/8/1pP5/8/8/1k6/4K3 w - b6 0 1` for "a promotion
   capture and a black en passant".** It has the white en passant but no promotion; two extra
   positions (`black_promotion_capture`, `pawn_double_capture`) were added so promotion captures
   on the *black* side and the both-flanks capture order are pinned as well.

## 8. Unresolved parts / missing tests

- **No exhaustive order comparison beyond the goldens.** The goldens cover 16 positions; the
  argument that `pop_lsb` preserves west-before-east for both colours is structural (ascending
  square index) and confirmed by the sample, not proven for all 64 squares. A cheap future test
  would iterate all 48 legal pawn squares × both colours × blocked/occupied target combinations and
  compare the table-driven stream against a reference arithmetic generator. This was not added
  because the reference generator would have to be kept alive purely for the test.
- **`promoting`-by-origin-rank is validated by perft + goldens, not by a dedicated assertion.** A
  pawn could only reach a wrong rank through a corrupt board; `verify_perft` 41/41 is the guard.
- **Profiling undercounts tail effects**: the profile was taken at depth 6 on #13 only; #14 was not
  profiled, though its wall-clock gain matches.
- **`perf` samples are ~12 k**, so the `generate_pseudo_legal` percentage moved slightly in the
  wrong direction (35.51 → 32.39 is right, but `is_move_trivially_legal` 13.49 → 14.33 is noise from
  the denominator shrinking); only the `generate_pawn_moves_for` and hot-line results are used as
  conclusions.

---

## 9. Next steps

Analysis §8's next items, in the order the evidence now suggests:

1. **§5.6 — `StateInfo` commoner-bitboard cache** (attempted alone; it previously regressed only
   when bundled with a capture pre-filter, `perf_analysis6/report2.md`). `is_move_trivially_legal`
   plus `compute_checkers`/`compute_pinned` are now the largest remaining movegen-side blocks
   (~14 %, 3.3 %, ~1 % respectively in the fresh profile), so this is the natural next target.
2. **§5.7 / §5.9 — `compute_pinned` with the Fairy-Stockfish `slider_blockers` pattern and
   precomputed empty-board slider attacks.** Smaller (~1 %) but the profile still lists it.
3. **Castling is now surprisingly hot** in the post-change profile (`generate_castling` owns the
   top remaining `movegen.rs` source lines, ~0.4 % self). It is probably inlining/attribution
   noise, but a fresh profile before choosing §5.6 vs. a castling fast path would settle it.
4. **§5.5 options 1 and 2 are now much less attractive.** Their estimated 2–4 % has effectively
   been captured with order preserved, and the pawn share fell from 16.9 % to 10.3 %; the remaining
   cost is dominated by the `Move` construction and the `MoveList` push, which bulk generation does
   not remove. No maintainer decision on the move-order contract is needed on this evidence.
5. Do **not** retry §5.1 (no-compaction `perft`) or §5.2 (local `Board::legal` bitboard cache)
   without new profiling evidence, and do not retry anything in analysis §6.
