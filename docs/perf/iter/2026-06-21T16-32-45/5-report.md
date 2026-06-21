# Iteration 0 — Report: Release profile tuning

## Results

| Measurement | Mean [ms] | Min [ms] | Max [ms] | vs baseline |
|:---|---:|---:|---:|---:|
| Baseline (old default profile) | 96.4 ± 16.6 | 74.1 | 121.4 | — |
| Final (`lto = "fat"`, `codegen-units = 1`) | 98.2 ± 17.4 | 76.1 | 134.1 | +1.9 % |

Delta is well within noise (CV ≈ 17 %). **No measurable improvement.**

## Analysis

The `[profile.release]` tuning was expected to yield 10–25 % but had zero
effect. This makes sense for a **single-crate** project:

- **`lto = "fat"`** — There are no external crate boundaries to optimize
  across. The only dependency is libstd, which is precompiled. No benefit.
- **`codegen-units = 1`** — With a single crate and `opt-level = 3`, LLVM
  already sees most of the module at once. The default `codegen-units = 16`
  does split functions, but in this codebase the hot loops are within large
  functions (e.g., `legal()` at 70+ lines, `do_move()` at 80+ lines) which
  LLVM keeps in one unit anyway.

The high variance (CV ≈ 17 %) suggests the benchmark is system-noise-limited,
not compute-bound. For future iterations, consider:
- Running on an isolated / quiet system
- Using `--warmup 10 --min-runs 50` to reduce variance
- Pinning CPU frequency (e.g., `cpupower`)

## Recommendation

Keep the release profile settings for consistency (no downside, and they will
matter if external crates are added later). Skip this for future iterations.

## Verification checklist

- [x] `cargo test` — all 45 tests pass (41 unit + 3 integration + 1 doc)
- [x] `cargo clippy` — no new warnings
- [x] `cargo run --release --example verify_perft 5` — 41/41 pass
- [x] Baseline and final hyperfine runs use identical command-line args
