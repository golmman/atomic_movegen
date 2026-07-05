# Publish-readiness plan

## Scope

Prepare `atomic-movegen` for publication on crates.io.  The Fairy-Stockfish
submodule and `docs/` directory are **excluded** from the package — only the
library crate itself goes to crates.io.  No CI config is required.

---

## Checklist

### 1. `Cargo.toml` metadata

Add the missing fields, following the decisions above:

| Field | Value |
|---|---|
| `description` | Short one-liner, e.g. `"Legal move generation for atomic chess."` |
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

Options for the fix (pick one):
- **A)** Remove the "Zero unsafe Rust" line from the Features list.
- **B)** If time permits, replace `transmute` with safe alternatives (e.g.
  `From<u8>` impls on `Square`, `PieceType`).

### 3. `.gitignore`: add `Cargo.lock`

`Cargo.lock` should not be committed for a library crate.  Add it to
`.gitignore` and `git rm --cached Cargo.lock`.

### 4. Visibility hygiene

Add `#[doc(hidden)]` / restrict `pub` on internal items that do not need to be
part of the public API:

| Module | Items to consider |
|---|---|
| `magic` | `init`, `sliding_attack`, `ROOK_DIRS`, `BISHOP_DIRS`, `ROOK_ENTRIES`, `BISHOP_ENTRIES`, `ROOK_MASKS`, `BISHOP_MASKS`, `ROOK_OFFSETS`, `BISHOP_OFFSETS`, `ROOK_TABLE_SIZE`, `BISHOP_TABLE_SIZE` — many of these are `pub(crate)` already. Double-check. |
| `pext` | `init`, `has_bmi2`, `bishop_attacks_pext`, `rook_attacks_pext` — the `*_attacks_pext` fns are `pub` but only called from `attacks::sliding_dispatch`.  They should be `pub(crate)`. |
| `types` | `SQUARES` is `pub(crate)` — fine. `MAX_MOVES`, `parse_sq`, `sq_str`, `move_to_str` (no such fn, but `sq_str`/`parse_sq` are public helpers).  Consider marking `parse_sq` / `sq_str` `pub(crate)` or keeping `pub` if users need them. |
| `bitboard` | `BETWEEN_BB` / `LINE_BB` already `pub(crate)` via `attacks` — but `bitboard.rs` re-exports `between_bb` / `line_bb` as `pub`.  That is fine (useful API). |

### 5. Remove `#![allow(dead_code)]` from `pext.rs`

The attribute is in the module body.  If dead-code warnings fire because
`pext_soft` / `compute_pext_layout` / `*_LAYOUT` are only called from `init()`
and tests, suppress them more narrowly with a per-item `#[allow(dead_code)]` or
a `#[cfg(test)]` gate.

### 6. Add `#[must_use]` to pure functions

Audit and add to:
- `Board::checkers()`, `Board::pinned()`, `Board::fen()`, `Board::piece_on()`,
  `Board::empty()`, `Board::occupied()`, `Board::side_to_move()`,
  `Board::castling_rights()`, `Board::ep_square()`, `Board::commoners()`,
  `Board::attackers_to()`, `Board::legal()`
- `Bitboard::is_empty()`, `Bitboard::count()`, `Bitboard::lsb()`,
  `Bitboard::more_than_one()`
- `Move::from_sq()`, `Move::to_sq()`, `Move::move_type()`,
  `Move::promotion_type()`
- `Piece::color()`, `Piece::type_of()`, `Piece::ascii_char()`
- `perft()` in `lib.rs`

### 7. Replace `String` error with a proper error type

`Board::from_fen` returns `Result<Self, String>`.  Introduce:

```rust
#[derive(Debug)]
pub enum FenError {
    TooShort { parts: usize },
    WrongRankCount { expected: u8, got: usize },
    InvalidSideToMove(String),
    // …
}

impl fmt::Display for FenError { … }
impl std::error::Error for FenError {}
```

No external dependency needed (skip `thiserror` to keep deps at zero).

### 8. Add `#[non_exhaustive]` to public enums

Prevent future breakage:

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

### 9. Inline documentation gaps

Add doc comments to:
- `magic::build_magic_table`
- `magic::sliding_attack`
- `pext::init`
- `pext::compute_pext_layout` / `PextLayout`
- `board::is_move_trivially_legal` (public but `pub(crate)`)
- `bitboard::aligned` (pub in `#[cfg(test)]` — fine)

### 10. Verify with `cargo publish --dry-run`

After all changes, run:

```sh
cargo publish --dry-run
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
