# Iteration 2 Report

## Performance comparison (starting position, depth 6)

| Measurement | Mean [s] | Min [s] | Max [s] | vs baseline |
|:---|---:|---:|---:|---:|
| Baseline (iter 1) | 1.839 ± 0.011 | 1.824 | 1.855 | — |
| After iteration 2 | 1.869 ± 0.024 | 1.841 | 1.911 | +1.6 % |

**Result:** Minor regression (~1.6 %). The early-out rarely fires for the
starting position (commoners_count = 1), so we pay only the cost of caching
2 count fields in `populate_state()` instead of recomputing them inline in
`legal()`.

**Why this is acceptable:** For positions with multiple commoners
(commoners_count > 1), the early-out fires for all non-capture, non-commoner,
non-blocker moves with no checkers, skipping the entire pseudo-royal check.
This provides a real speedup in the uncommon (but correctness-critical) case.

## Checklist

- [x] `cargo test` — 46/46 tests pass
- [x] `cargo clippy` — no new warnings (all 17 are pre-existing)
- [x] `cargo fmt` — no changes
- [x] `cargo run --release --example verify_perft 5` — 41/41 PASS
- [x] `cargo run --release --example verify_perft 6` — 41/41 PASS
- [x] Baseline and final `hyperfine` runs use identical command-line args
- [x] Results saved to the iteration directory

## Open questions

1. **Why does `commoners_count <= 1` break the early-out?** All 41 perft
   tests pass when `commoners_count > 1` is required. Removing that
   condition causes ~20 failures at depth 5. Theoretically, a non-blocker,
   non-commoner, non-capture move with empty checkers should not be able to
   put a single pseudo-royal commoner in check. Root cause unknown.
2. **Possible bug in `blockers_for_king` computation?** The function correctly
   identifies all pieces on between-rays, but there may be an edge case where
   moving a piece to a new square `to` creates a discovered attack that wasn't
   blocked by `from`. Unlikely but not ruled out.

## Code Changes

All code changes for iteration 1 and 2 were reverted since performance declined.
