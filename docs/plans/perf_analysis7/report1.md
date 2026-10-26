# Implementation Report — Plan 1: Perft Fast Path Without Legal-List Compaction

**Plan:** `docs/plans/perf_analysis7/plan1.md`
**Status:** Implemented, measured, and **reverted** because the acceptance criteria were not met.

---

## 1. Summary

The planned optimization was implemented exactly in `src/lib.rs::perft`:

- Pseudo-legal moves are generated once per node with `generate_pseudo_legal`.
- `StateInfo` is populated explicitly with `board.populate_state`.
- Candidates are filtered directly with
  `is_move_trivially_legal(board, m, &state) || board.legal(m, &state)`.
- At depth 1, legal candidates are counted without compacting a legal move list.
- At depth >= 2, legal candidates are visited in pseudo-legal order and recursed into
  without first building a compact legal list.

No changes were made to `src/movegen.rs`, `src/board.rs`, public APIs, tests, or examples.
Public move generation and move ordering therefore remained unchanged.

Correctness passed in full: all 41 positions matched at depths 1–6, and all unit,
integration, golden-order, and doc tests passed.

Performance did **not** improve on this machine. Repeated fixed-FEN A/B measurements
showed a small regression on test #13 and a stable regression of about 3 % on test #14.
Following the plan's acceptance rule, the change was reverted rather than retained.

---

## 2. Baseline Measurements

The host did not provide `/usr/bin/time`, so timings were taken with
`date +%s%N` around each command. The machine was otherwise left idle between runs.

### 2.1 Full `verify_perft` suite, baseline

| Run | Wall time |
|-----|-----------|
| 1 | 76.508 s |
| 2 | 77.027 s |
| 3 | 79.499 s |
| **Median** | **77.027 s** |

All 3 runs passed 41/41. The suite's reported #13 times were 11.856 s, 11.978 s,
and 12.385 s.

### 2.2 Fixed-FEN depth 6, baseline

FEN #13:
`rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5`

| Run | Wall time |
|-----|-----------|
| 1 | 11.579 s |
| 2 | 11.591 s |
| 3 | 11.508 s |
| 4 | 11.653 s |
| 5 | 11.477 s |
| 6 | 11.405 s |
| 7 | 11.529 s |
| **Median** | **11.529 s** |

FEN #14:
`r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15`

| Run | Wall time |
|-----|-----------|
| 1 | 1.463 s |
| 2 | 1.453 s |
| 3 | 1.484 s |
| 4 | 1.471 s |
| 5 | 1.482 s |
| 6 | 1.466 s |
| 7 | 1.458 s |
| **Median** | **1.466 s** |

---

## 3. Patched Measurements

### 3.1 Full `verify_perft` suite, patched

| Run | Wall time | Result |
|-----|-----------|--------|
| 1 | 82.621 s | 41/41 passed |
| 2 | 78.147 s | 41/41 passed |
| 3 | 78.244 s | 41/41 passed |
| **Median** | **78.244 s** | **41/41** |

The suite's reported #13 times were 12.769 s, 12.107 s, and 12.026 s.
The full-suite median was about 1.6 % slower than baseline, though the first patched
run was visibly affected by system noise.

### 3.2 Fixed-FEN depth 6, patched

FEN #13:

| Run | Wall time |
|-----|-----------|
| 1 | 11.665 s |
| 2 | 11.711 s |
| 3 | 11.636 s |
| 4 | 11.620 s |
| 5 | 11.649 s |
| 6 | 11.546 s |
| 7 | 11.543 s |
| **Median** | **11.620 s** |

FEN #14:

| Run | Wall time |
|-----|-----------|
| 1 | 1.486 s |
| 2 | 1.498 s |
| 3 | 1.492 s |
| 4 | 1.503 s |
| 5 | 1.494 s |
| 6 | 1.496 s |
| 7 | 1.495 s |
| **Median** | **1.495 s** |

Node counts were unchanged: 2,160,817,389 for #13 and 245,931,633 for #14.

---

## 4. A/B Recheck

Because the first patched full-suite run was noisy, the baseline `perft` was restored,
rebuilt, and remeasured in the same session before making the final decision. The patched
version was then rebuilt and remeasured again.

| Measurement | Baseline recheck median | Patched recheck median | Change |
|-------------|--------------------------|-------------------------|--------|
| Fixed FEN #13, depth 6 | 11.551 s | 11.604 s | **+0.46 %** (slower) |
| Fixed FEN #14, depth 6 | 1.458 s | 1.507 s | **+3.36 %** (slower) |

The regression on #14 was consistent across both patched measurement rounds. Even #13,
where the plan expected roughly a 3 % win, showed no improvement. The optimization was
therefore rejected under the plan's acceptance criteria:

> If the fixed-FEN result contradicts the analysis (stable regression), revert the change
> and document the result rather than keeping a speculative optimization.

---

## 5. Correctness Verification

During the patched trial:

- `cargo fmt` — passed.
- `cargo clippy` — passed with no warnings.
- `cargo test` — all tests passed:
  - 54 unit tests
  - `move_order_golden`: 1 passed
  - `perft_tests`: 4 passed, including depth-1 counting and depth-4 all-position checks
  - `verify_moves`: 1 passed
  - doc tests: 1 passed
- `cargo doc` — passed.
- `cargo run --release --example verify_perft` — **41/41 passed** at depths 1–6.

After reverting, `cargo fmt`, `cargo clippy`, `cargo test`, and `cargo doc` were run
again on the final tree and all passed. `git diff` confirms `src/lib.rs` is back to its
original content; no source files remain modified.

---

## 6. Findings and Surprises

- The expected ~3 % win on #13 did not reproduce on this machine. The measured effect
  was essentially neutral on #13 and a stable regression on #14.
- The extra branch per pseudo-legal move appears to cost more than the removed compaction
  writes on these positions, despite the source-level profile suggesting compaction was
  expensive.
- Full-suite timings were noisy, as anticipated by the plan. The fixed-FEN repeated
  medians, including the explicit baseline recheck, were the basis for reverting.
- Correctness of the no-compaction design was never in doubt: all 41 oracle positions and
  the move-order golden test passed.

---

## 7. Unresolved / Missing

- No profile was captured for the patched binary after the regression was established,
  since the optimization was not retained.
- The exact reason the compaction removal regresses #14 was not investigated further.
  It may be code layout, branch-prediction behavior on rejected pseudo-legal moves, or
  a difference between this host and the host used for the analysis. This is left for
  future profiling if the approach is revisited.
- `perf` profiling was not run; no additional tools beyond the crate examples, `date`,
  `git diff`, and temporary copies of `src/lib.rs` were needed.

---

## 8. Next Steps

- Keep the current compacting `generate_legal`-based `perft` implementation.
- Proceed with **Plan 2**: the local `occupied` / commoner bitboard cache in
  `Board::legal` (analysis §5.2).
- Re-baseline #13 and #14 on this host before stacking further changes, because absolute
  timings here are materially different from the analysis (for example, #13 baseline is
  about 11.5 s rather than 8.457 s).
- Treat this Plan 1 result as a host-specific negative result; do not retry the same
  no-compaction shape without new profiling evidence.
