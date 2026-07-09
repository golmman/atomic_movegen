# Plan 1: Safety / correctness — critical

## Goal

Remove all undefined behavior and out-of-bounds access from public APIs, especially around `NO_PIECE`, `Square::NONE`, and the `unsafe` transmutes in `types.rs`.

## Scope

- `src/types.rs` (`Piece`, `Move`, `Square` helpers, `parse_sq`, `sq_str`)
- `src/board.rs` (`piece_on`, `empty`)
- `examples/fen_after.rs` (uses `parse_sq`)
- `Cargo.toml` (no changes expected unless new `assert` requires debug-assertions)

## Current state (from review)

- `Piece::type_of` underflows to `255` for `NO_PIECE` and then `transmute`s it.
- `Board::piece_on` and `Board::empty` access `squares[64]` for `Square::NONE`.
- `parse_sq` silently returns `A1` for invalid input.
- `sq_str` returns `"a9"` for `Square::NONE`.
- `Piece::color` returns `Color::White` for `NO_PIECE`.
- `Move` constructors silently truncate `Square::NONE`.
- Several `unsafe` blocks in `types.rs` violate `AGENTS.md` zero-unsafe rule.

## Tasks

1. **`Piece::type_of` — safe, no `unsafe` transmute**
   - Add a `pub(crate) const PIECE_TYPES: [PieceType; 6] = [Pawn, Knight, Bishop, Rook, Queen, Commoner];` in `types.rs`.
   - Change `Piece::type_of` to:
     ```rust
     pub fn type_of(self) -> PieceType {
         assert!(self.0 != 0, "Piece::type_of called on NO_PIECE");
         let inner = (self.0 & 7) - 1;
         debug_assert!(inner < 6);
         PIECE_TYPES[inner as usize]
     }
     ```
   - This preserves the return type and panics on `NO_PIECE` instead of UB.

2. **`Piece::color` — assert on `NO_PIECE`**
   - Change to:
     ```rust
     pub fn color(self) -> Color {
         assert!(self.0 != 0, "Piece::color called on NO_PIECE");
         if self.0 & 8 == 0 { Color::White } else { Color::Black }
     }
     ```

3. **`Piece::ascii_char` — handle `NO_PIECE` before `type_of`/`color`**
   - Early return `'.'` (or `'-'`) if `self.0 == 0`.
   - Then call `type_of` and `color` on the known-valid piece.

4. **`Board::piece_on` and `Board::empty` — guard `Square::NONE`**
   - `piece_on` returns `NO_PIECE` for `Square::NONE`.
   - `empty` returns `true` for `Square::NONE`.

5. **`parse_sq` — return `Option<Square>`**
   - Change signature to `pub fn parse_sq(s: &str) -> Option<Square>`.
   - Return `None` for malformed input.
   - Update `Board::from_fen` to turn `None` into `FenError::InvalidEpSquare`.
   - Update `examples/fen_after.rs` to handle `None` (print an error and exit).

6. **`sq_str` — return `Option<String>` (or `Option<&'static str>`)**
   - Return `None` for `Square::NONE`.
   - Update `Board::fen` to use `unwrap_or("??")` or a fallback.
   - Update any examples that call `sq_str`.

7. **`Move` constructors — assert valid squares and promotion pieces**
   - Add `assert!(from != Square::NONE && to != Square::NONE);` to `make_move`, `make_promotion`, `make_enpassant`, `make_castling`.
   - In `make_promotion`, replace the `_ => 0` fallback with an exhaustive match or an `assert!` that `pt` is one of `Knight`, `Bishop`, `Rook`, `Queen`.

8. **Remove `unsafe` blocks from `types.rs`**
   - `Bitboard::lsb` can use a guarded `SQUARES[idx as usize]` lookup (returning `Square::NONE` for empty) or keep `trailing_zeros` and a safe `SQUARES` array.
   - `Move::from_sq`/`to_sq` can use `SQUARES[idx as usize]`; the `idx` is already masked to `0..63`.

9. **Run tests and linting**
   - `cargo build`
   - `cargo clippy`
   - `cargo test`
   - `cargo run --release --example verify_perft 5`
   - `cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6`

10. **Write `report1.md`**
    - Document which `unsafe` blocks were removed, what panics were introduced, and any API changes (`parse_sq`, `sq_str` now return `Option`).
    - Note any unexpected call sites that needed updating.
    - Record any workaround for `NO_PIECE` handling in `Board::fen` or examples.

## Notes for plan 2

- `Piece::type_of` and `Piece::color` now panic on `NO_PIECE` and no longer use `unsafe` transmute.
- `Board::piece_on` and `empty` are safe for `Square::NONE`.
- `parse_sq`/`sq_str` signatures changed to `Option` — plan 2 should not reintroduce callers that unwrap blindly.
- The `pext` inline-assembly `unsafe` in `src/pext.rs` remains and will be addressed in plan 2.
