# Iteration 0 — Implementation

## Change

Added `[profile.release]` section to `Cargo.toml`:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
```

## Verification

- `cargo test` — all 45 tests pass (41 unit + 3 integration + 1 doc)
- `cargo clippy` — no new warnings (all pre-existing)
- `cargo fmt --check` — clean
- `cargo run --release --example verify_perft 5` — 41/41 pass
