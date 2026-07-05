# Publish-readiness report

All 10 items from [`plan.md`](./plan.md) are complete.

## Summary of changes

| # | Item | Status |
|---|---|---|
| 1 | `Cargo.toml` metadata (`description`, `license`, `repository`, `rust-version`, `exclude`) + `[[example]]` removal | Done |
| 2 | `.gitignore` + `git rm --cached Cargo.lock` | Done |
| 3 | README: removed "Zero `unsafe` Rust" | Done |
| 4 | Visibility hygiene: `has_bmi2`, `bishop_attacks_pext`, `rook_attacks_pext` → `pub(crate)`; `sq_str`, `parse_sq` → `pub #[doc(hidden)]` (examples reuse them); magic.rs already clean | Done |
| 5 | Replaced `#![allow(dead_code)]` with `#![cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]` in `pext.rs` | Done |
| 6 | `#[must_use]` on 20+ pure functions across `Board`, `Bitboard`, `Move`, `Piece`, and `perft` | Done |
| 7 | `FenError` enum with 6 variants + `Display` + `Error` impl; `from_fen` signature changed to `Result<Self, FenError>` | Done |
| 8 | `#[non_exhaustive]` on `MoveType`, `PieceType`, `Color`, `File`, `Rank` | Done |
| 9 | Doc comments on `magic::build_magic_table`, `magic::sliding_attack`, `pext::PextLayout`, `pext::compute_pext_layout` | Done |
| 10 | `cargo test` (45/45), `cargo clippy` (0 warnings), `cargo publish --dry-run` (validates) | Done |

## Verification

```sh
cargo test      # 45 passed
cargo clippy    # 0 warnings
cargo publish --dry-run   # packaging and verification OK
```

The `Fairy-Stockfish/` and `docs/` directories are excluded from the published crate via `Cargo.toml` `exclude`.
