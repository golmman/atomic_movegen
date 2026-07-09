# Plan 3: FEN parsing and board-state correctness

## Goal

Make FEN parsing robust, fix castling-rights edge cases, and ensure move generation does not produce impossible moves from stale or malformed board state.

## Scope

- `src/board.rs` (`from_fen`, `update_castling_rights`, `Board` struct, `fen`)
- `src/movegen.rs` (`generate_castling`, `generate_pawn_moves_for`)
- `tests/verify_moves.rs` (remove `fix_halfmove_clock` hack)
- `Cargo.toml` (no changes unless new dependencies are needed)

## Current state (from review and previous plans)

- `rule50` is `u8` and can overflow/wrap after 255 moves.
- `from_fen` does not validate piece placement row sums, `col` bounds, or digit overflow.
- 5-field FEN is silently accepted.
- Castling rights are not validated against actual king/rook squares.
- `ep_square` is not validated against an actual pawn that just moved.
- `update_castling_rights` over-clears for non-capture moves to `A1`/`H1` and under-clears for captures of the commoner on `E1`/`E8`.
- `generate_castling` does not check that the king/commoner and rook are still present.
- `generate_pawn_moves_for` does not verify the EP victim square contains an enemy pawn.
- `Board::pinned` returns pieces of both colors.
- `do_move` no longer calls `populate_state` (plan 2).
- `Board::piece_on`/`empty` are safe for `Square::NONE` (plan 1).

## Prerequisites

- Read `report2.md` for any `do_move`/`populate_state` or table-initialization changes that might affect how `from_fen` is exercised.

## Tasks

1. **Change `rule50` from `u8` to `u16`**
   - Update `Board` struct field.
   - Update `from_fen` to parse `u16`.
   - Update `fen()` to output `u16`.
   - `do_move` `+= 1` is now safe for all realistic FEN inputs.

2. **Validate piece placement in `from_fen`**
   - After each row, assert `col == 8`; return `FenError::WrongRankCount` or a new `FenError::InvalidPlacement` if not.
   - When a digit is encountered, ensure `col + digit <= 8`.
   - When a piece is placed, ensure `sq_idx < 64` (or `col < 8`) before writing.
   - Add a new `FenError` variant for placement errors if `FenError` is still hand-written; otherwise use the existing `InvalidPlacement` (create if missing).

3. **Reject 5-field FEN**
   - Only accept 4 or 6 fields. A 5-field FEN should return `FenError::TooShort` or a new `FenError::InvalidFieldCount`.

4. **Validate castling rights**
   - If `WK_CASTLE` is set, verify `squares[E1] == W_COMMONER` and `squares[H1] == W_ROOK`.
   - If `WQ_CASTLE` is set, verify `squares[E1] == W_COMMONER` and `squares[A1] == W_ROOK`.
   - Same for black.
   - If a right is set but the pieces are missing, clear it or return an error. Returning an error is safer; if tests rely on stale rights, revisit.

5. **Validate `ep_square`**
   - `ep_square` must be on rank 3 for black or rank 6 for white.
   - The square immediately behind `ep_square` must contain an enemy pawn (`make_piece(them, Pawn)`).
   - Return `FenError::InvalidEpSquare` if not.

6. **Fix `update_castling_rights`**
   - Only clear rights based on `to` when `is_capture` is true (or en passant).
   - Add `to == Square::E1`/`Square::E8` checks to clear both rights for the affected color when the commoner is captured.
   - Keep the existing `from == E1/E8/H1/A1...` checks for king/rook moves.
   - The separate blast block already checks whether the rook squares are empty after capture; verify it still works after the `update_castling_rights` changes.

7. **Add `generate_castling` sanity checks**
   - Before pushing a castling move, verify:
     - `board.piece_on(king_sq) == make_piece(us, Commoner)`
     - `board.piece_on(king_side_rook_sq) == make_piece(us, Rook)` (or queen-side rook)
   - This prevents `generate_pseudo_legal` from returning impossible moves when castling rights are stale.

8. **Add `generate_pawn_moves_for` EP victim check**
   - Compute `ep_cap` as `to - 8` for white or `to + 8` for black.
   - Only generate the EP move if `board.piece_on(ep_cap) == make_piece(them, Pawn)`.

9. **Fix `Board::pinned` color filtering**
   - In `compute_pinned`, mask the `between` bitboard with `self.pieces_color(us)` before OR-ing into `pinned`.
   - This makes `Board::pinned(c)` return only pieces of color `c`.

10. **Remove `fix_halfmove_clock` workaround from `tests/verify_moves.rs`**
    - Since `rule50` will be `u16`, the test should no longer need to clamp `>255` to `0`.
    - Remove the helper or make it a no-op.

11. **Run tests and linting**
    - `cargo build`
    - `cargo clippy`
    - `cargo test`
    - `cargo run --release --example verify_perft 5`
    - `cargo run --release --example verify_perft 6` (optional, longer)
    - `cargo test --test verify_moves`

12. **Write `report3.md`**
    - Document any FENs in `tests/` or `perft_values.md` that failed validation and how they were fixed.
    - Note whether `update_castling_rights` changes affected any perft results.
    - Record the `rule50` `u16` migration impact on `Board` size and ` fen()` output.
    - Mention any surprising interaction between `generate_castling` checks and `from_fen` validation.

## Notes for plan 4

- `rule50` is now `u16`; `from_fen` and `fen()` are robust.
- Castling and EP generation are guarded against stale state.
- `Board::pinned` now returns color-filtered results.
- The `tests/verify_moves.rs` `fix_halfmove_clock` hack should be removed.
- Plan 4 will focus on dead-code removal, dependency cleanup, and code organization.
