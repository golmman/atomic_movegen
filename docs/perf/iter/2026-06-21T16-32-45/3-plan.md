# Iteration 0 — Release profile tuning

| Risk | Impact | Effort |
|------|--------|--------|
| None | 10–25 % | 1 line |

## Problem

`Cargo.toml` has no `[profile.release]` section. Rust defaults to `opt-level = 3`
but `lto = "thin"` and `codegen-units = 16`, which leaves sizable performance on
the table.

## Fix

Add to `Cargo.toml`:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
```

`lto = "fat"` enables full cross-crate inlining even within this single-crate
project. `codegen-units = 1` prevents the thin-LTO/codegen-units heuristic from
splitting functions across codegen units, letting LLVM see the entire module at
once. Both are standard for Rust perf work.

## Verification

- [x] `cargo test` — all unit tests pass (baseline)
- [x] `cargo run --release --example verify_perft 5` — all 41 positions match (baseline)
- [ ] After change: `cargo test`, `cargo clippy`, `cargo fmt`
- [ ] After change: `cargo run --release --example verify_perft 5`
- [ ] After change: hyperfine re-run and comparison
