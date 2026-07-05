# Publish-readiness plan

## Scope

Prepare `atomic-movegen` for publication on crates.io.  The Fairy-Stockfish
submodule and `docs/` directory are **excluded** from the package — only the
library crate itself goes to crates.io.  No CI config is required.

---

## Checklist

### 1. `Cargo.toml` metadata

Add the missing fields:

| Field | Value |
|---|---|
| `description` | `"Legal move generation for atomic chess."` |
| `license` | `"MIT"` |
| `repository` | The GitHub URL of this repo. |
| `rust-version` | `"1.85"` (needed for edition 2024) |
| `exclude` | `["Fairy-Stockfish/**", "docs/**"]` |

Remove the explicit `[[example]]` stanza — examples are auto-discovered by
Cargo and we do not want to advertise them for publish.

### 2. Fix the "zero unsafe" lie in README

The README says *"Zero `unsafe` Rust"* but `unsafe { std::mem::transmute }` is
used in:
- `types.rs` — `Bitboard::lsb`, `Piece::type_of`, `Move::from_sq`, `Move::to_sq`
- `pext.rs` — `bishop_attacks_pext_impl`, `rook_attacks_pext_impl` (declared
  `unsafe fn`)

Options (pick one):
- **A)** Remove the "Zero unsafe Rust" line from the Features list.
- **B)** Replace `transmute` with safe alternatives (e.g. `From<u8>` impls on
  `Square`, `PieceType`).

### 3. `.gitignore`: add `Cargo.lock`

`Cargo.lock` should not be committed for a library crate.  Add it to
`.gitignore` and `git rm --cached Cargo.lock`.

### 4. Visibility hygiene

Tighten visibility so internal machinery is not part of the public API.
Use `pub(crate)` to restrict — `#[doc(hidden)]` does not prevent downstream
code from accessing items and should not be used here.

| Module | Items to make `pub(crate)` |
|---|---|
| `pext` | `has_bmi2`, `bishop_attacks_pext`, `rook_attacks_pext`, `init` — all only called from `attacks::sliding_dispatch`. |
| `types` | `parse_sq`, `sq_str` — these are FEN-internal helpers. If external consumers need them they can be re-exported via `Board` later. |
| `magic` | Double-check that all items already marked `pub(crate)` are actually `pub(crate)`. The `init`, `sliding_attack`, `ROOK_DIRS`, `BISHOP_DIRS`, `ROOK_ENTRIES`, `BISHOP_ENTRIES`, `ROOK_MASKS`, `BISHOP_MASKS`, `ROOK_OFFSETS`, `BISHOP_OFFSETS`, `ROOK_TABLE_SIZE`, `BISHOP_TABLE_SIZE` should not leak. |

### 5. Remove `#![allow(dead_code)]` from `pext.rs`

The attribute is at module level.  If dead-code warnings fire because
`pext_soft` / `compute_pext_layout` / `*_LAYOUT` are only referenced from
`init()` and tests, suppress them per-item with
`#[allow(dead_code)]` on the specific function, or gate them
with `#[cfg(test)]` where possible.

### 6. Add `#[must_use]` to pure functions

Add `#[must_use]` to every public function whose return value is meaningful
when discarded:

- `Board`: `checkers`, `pinned`, `fen`, `piece_on`, `empty`, `occupied`,
  `side_to_move`, `castling_rights`, `ep_square`, `commoners`, `attackers_to`,
  `legal`
- `Bitboard`: `is_empty`, `count`, `lsb`, `more_than_one`
- `Move`: `from_sq`, `to_sq`, `move_type`, `promotion_type`
- `Piece`: `color`, `type_of`, `ascii_char`
- `perft` in `lib.rs`

### 7. Replace `String` error with a proper error type

`Board::from_fen` returns `Result<Self, String>`.  Introduce a dedicated type:

```rust
#[derive(Debug)]
pub enum FenError {
    TooShort { parts: usize },
    WrongRankCount { expected: u8, got: usize },
    InvalidSideToMove(String),
    InvalidCastling(String),
    InvalidEpSquare(String),
    ParseInt(String),
}

impl fmt::Display for FenError { … }
impl std::error::Error for FenError {}
```

No external dependency needed (`thiserror` is nice but keeping deps at zero is
better for a foundational crate).

### 8. Add `#[non_exhaustive]` to public enums

Prevent future additions from being a breaking change:

```rust
#[non_exhaustive]
pub enum MoveType { … }
#[non_exhaustive]
pub enum PieceType { … }
#[non_exhaustive]
pub enum Color { … }
#[non_exhaustive]
pub enum File { … }
#[non_exhaustive]
pub enum Rank { … }
```

### 9. Fill inline documentation gaps

Add doc comments to:
- `magic::build_magic_table`
- `magic::sliding_attack`
- `pext::init`
- `pext::compute_pext_layout` / `PextLayout`
- `board::is_move_trivially_legal` — `pub(crate)` but still worth documenting

### 10. Verify with `cargo publish --dry-run`

After all changes:

```sh
cargo test && cargo clippy && cargo publish --dry-run
```

This validates the manifest, checks that all files exist, and confirms nothing
unexpected is packaged.  It does **not** upload.

---

## Ordering

1. `Cargo.toml` metadata + `[[example]]` removal + `exclude`
2. `.gitignore` + `git rm --cached Cargo.lock`
3. Fix README "zero unsafe"
4. Visibility hygiene
5. `#[allow(dead_code)]` fix
6. `#[must_use]` audit
7. `FenError` type
8. `#[non_exhaustive]` audit
9. Doc comment gaps
10. `cargo test && cargo clippy && cargo publish --dry-run`

Steps 4–8 can be parallelised across files.
