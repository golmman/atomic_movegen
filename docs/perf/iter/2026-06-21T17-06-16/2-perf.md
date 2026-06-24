# Iteration 1 — Baseline measurement

## Baseline

```sh
cargo build --release && hyperfine \
  --warmup 3 \
  --min-runs 10 \
  --export-markdown docs/perf/iter/2026-06-21T17-06-16/hyperfine-baseline.md \
  'cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6'
```

## Analysis

The hot spot is `legal()`, which recomputes checkers/pins/counts from
scratch for every pseudo-legal move via inline attack computations.

- `self.commoners(us).count()` and `self.commoners(them).count()` each
  perform a bitboard load, AND, and POPCNT — called once per legal()
  invocation.
- Numerous `self.commoners(them)` bitboard queries for adjacency+
  attack checks are recomputed from `by_color`/`by_type` each time.
- No caching of results across the 30–40 legal() calls per position in
  perft.

See `board.rs:791-792` for the count calls and `board.rs:676,804,815`
for the bitboard queries.
