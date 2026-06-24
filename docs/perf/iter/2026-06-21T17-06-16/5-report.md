# Iteration 1 Report

## Post-change measurement

```sh
cargo build --release && hyperfine \
  --warmup 3 \
  --min-runs 10 \
  --export-markdown docs/perf/iter/2026-06-21T17-06-16/hyperfine-after.md \
  'cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6'
```

## Performance comparison (starting position, depth 6)

| Measurement | Mean [s] | Min [s] | Max [s] | vs baseline |
|:---|---:|---:|---:|---:|
| Baseline (iter 0, no state population) | 1.871 ± 0.041 | 1.816 | 1.928 | — |
| After iteration 1 | 1.927 ± 0.029 | 1.889 | 1.972 | +3.0 % |

**Result:** Minor regression (~3 %). The `populate_state()` overhead
(`compute_checkers()` + `compute_pinned()` once per position) slightly
exceeds the savings from caching 2 count fields in `legal()`.

**Why this is acceptable:** This iteration lays the architectural
foundation for iteration 2 — the fast early-out for non-capture moves.
Once `state.checkers` and `state.blockers_for_king` are populated (iter 2
will rename `state.pinned`), the early-out will skip `legal()` entirely
for the majority of moves, far outweighing this small cost.

## Checklist

- [x] `cargo test` — 45/45 tests pass
- [x] `cargo clippy` — no new warnings (all 17 are pre-existing)
- [x] `cargo fmt` — no changes
- [x] `cargo run --release --example verify_perft 5` — 41/41 PASS
- [x] `cargo run --release --example verify_perft 6` — 41/41 PASS
- [x] Baseline and final `hyperfine` runs use identical command-line args
- [x] Results saved to the iteration directory

## Code Changes

All code changes were reverted after iteration 2, since performance declined.
