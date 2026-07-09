# Plan 1 Report: Safety / correctness

This report documents the changes made in `docs/plans/review1/plan1.md` to remove
undefined behavior and out-of-bounds access from public APIs, especially around
`NO_PIECE`, `Square::NONE`, and the `unsafe` transmutes in `src/types.rs`.

## Changes

### 1. `Piece::type_of` — safe lookup, no `unsafe` transmute

- Added `pub(crate) const PIECE_TYPES: [PieceType; 6]` in `src/types.rs`.
- `Piece::type_of` now asserts that `self.0 != 0` (`NO_PIECE`), computes the
  piece-type index from the lower 3 bits, and indexes into `PIECE_TYPES`.
- Replaces the previous `unsafe { std::mem::transmute(inner) }`, which was UB
  when `inner` underflowed to `255` for `NO_PIECE` (because `overflow-checks = false`
  in release builds).

### 2. `Piece::color` — assert on `NO_PIECE`

- Now asserts `self.0 != 0` before reading the color bit.
- Previously returned `Color::White` for `NO_PIECE` without complaint.

### 3. `Piece::ascii_char` — handle `NO_PIECE`

- Early returns `'.'` for `NO_PIECE` so it no longer calls `type_of`/`color`
  on an invalid piece.
- `Display` already handled `NO_PIECE` specially (`"--"`), so it remains safe.

### 4. `Board::piece_on` and `Board::empty` — guard `Square::NONE`

- `piece_on` returns `NO_PIECE` for `Square::NONE` instead of indexing
  `squares[64]`.
- `empty` returns `true` for `Square::NONE`.

### 5. `parse_sq` — return `Option<Square>`

- Signature changed from `pub fn parse_sq(s: &str) -> Square` to
  `pub fn parse_sq(s: &str) -> Option<Square>`.
- Returns `None` for malformed or non-ASCII input.
- `Board::from_fen` now turns `None` into `FenError::InvalidEpSquare`.
- `examples/fen_after.rs` now exits with an error message when given an
  invalid square string.

### 6. `sq_str` — return `Option<&'static str>`

- Signature changed from `pub fn sq_str(sq: Square) -> String` to
  `pub fn sq_str(sq: Square) -> Option<&'static str>`.
- Returns `None` for `Square::NONE`.
- Uses a static `STRS` array of the 64 algebraic square strings for fast,
  allocation-free lookup.
- Updated call sites:
  - `src/board.rs` (`Board::fen`) uses `unwrap_or("??")`.
  - `tests/verify_moves.rs` wraps results with `unwrap_or("??")`.
  - `examples/fen_after.rs`, `examples/list_moves.rs`, `examples/perft_divide.rs`,
    `examples/debug_moves.rs`, and `examples/pawn_debug.rs` all now unwrap with
    a `"??"` fallback.

### 7. `Move` constructors — assert valid squares and promotion pieces

- `make_move`, `make_promotion`, `make_enpassant`, and `make_castling` now
  assert `from != Square::NONE && to != Square::NONE`.
- `make_promotion` no longer silently falls back to `Knight` for `Pawn` or
  `Commoner`; it panics with a clear message for invalid promotion pieces.

### 8. Removed `unsafe` blocks from `src/types.rs`

- `Bitboard::lsb` now returns `Square::NONE` for an empty bitboard and indexes
  `SQUARES` instead of `std::mem::transmute`.
- `Move::from_sq` and `Move::to_sq` now index `SQUARES` (the 6-bit index is
  already masked to `0..63`).
- `Piece::type_of` now indexes `PIECE_TYPES` instead of `std::mem::transmute`.

The inline-assembly `unsafe` blocks in `src/pext.rs` remain untouched per the
plan (to be addressed in plan 2).

## API changes

- `pub fn parse_sq(s: &str) -> Option<Square>` (was `-> Square`).
- `pub fn sq_str(sq: Square) -> Option<&'static str>` (was `-> String`).

Downstream code that called these functions without unwrapping must now handle
`Option`.

## Unexpected call sites

- `examples/pawn_debug.rs` uses `sq_str(Square::from_u8(...))` on raw `u16`
  values from a `HashMap`. Because `Square::from_u8` returns `Square::NONE` for
  out-of-range indices, `sq_str` now returns `None` and the example prints `"??"`
  instead of potentially invalid/UB square strings.
- `tests/verify_moves.rs` builds UCI strings by concatenating `from` and `to`.
  The castling special cases now return `&'static str` literals, and the normal
  `sq_str` results are unwrapped before string concatenation.

## Workarounds

- `Board::fen` uses `sq_str(sq).unwrap_or("??")` for the en-passant square. This
  is a defensive fallback; the FEN should only ever contain a valid square or `'-'`.
- Examples that print moves use `sq_str(...).unwrap_or("??")` so a malformed or
  `NONE` square does not crash the program.

## Verification

- `cargo build` — passed.
- `cargo clippy` — passed.
- `cargo fmt` — passed.
- `cargo test` — passed (39 unit tests, 4 perft tests, 1 move verification test).
- `cargo run --release --example verify_perft 5` — passed (41/41 test positions).
- `cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6` —
  passed, returned `118926425`.

## Notes for plan 2

- `Piece::type_of` and `Piece::color` now panic on `NO_PIECE` and no longer use
  `unsafe` transmute.
- `Board::piece_on` and `Board::empty` are safe for `Square::NONE`.
- `parse_sq`/`sq_str` signatures changed to `Option` — future code should not
  reintroduce callers that unwrap blindly.
- The `pext` inline-assembly `unsafe` in `src/pext.rs` remains and will be
  addressed in plan 2.

## Performance impact and optimization follow-up

The safety refactor in Plan 1 introduced a measurable perft slowdown. The
`verify_perft 6` total went from an earlier ~56 s baseline to ~60–61 s on the
same hardware. This was caused mainly by the new `Piece::type_of` safety
boundary (the `PIECE_TYPES` lookup + bounds check) and the additional guards in
`piece_on`, `empty`, and `Square` helpers.

To recover speed without removing the safety guarantees, the following
optimizations were applied:

- **Avoided `type_of` in commoner checks.** `is_move_trivially_legal` and
  `Board::legal` now compare `board.piece_on(from) == make_piece(us, PieceType::Commoner)`
  instead of `piece.type_of() == PieceType::Commoner`.
- **Inlined hot helpers.** Added `#[inline]` / `#[inline(always)]` to
  `Piece::color`, `Piece::type_of`, `Piece::from_parts`, `Color::flip`,
  `Move` accessors and constructors, `Bitboard::is_empty`/`lsb`/`pop_lsb`/`square_bb`,
  `Square::from_index`/`from_u8`, `file_of`, `rank_of`, `make_square`, and
  `Board::empty`/`side_to_move`.
- **Removed redundant `type_of` calls inside board helpers.** `move_piece`,
  `remove_piece`, and `place_piece` now take `piece`, `color`, and `pt` as
  arguments, so the caller computes `type_of` once and the helpers do not.
- **Cached capture type for `undo_move`.** `StateInfo` gained a `cap_pt` field so
  `undo_move` can restore the main captured piece without re-decoding its type.
- **Used `const` for small identity tables.** `file_of`, `rank_of`, and
  `Move::promotion_type` now use `const` arrays so they can constant-fold.

### What was tried and reverted

- Storing `moving_piece`/`moving_pt` in `StateInfo` to avoid the normal-move
  `type_of` in `undo_move` — it was slower.
- Expanding `StateInfo::captured` to also store `color` and `piece_type` for each
  blast-captured piece — it was slower.
- Replacing the `PIECE_TYPES` lookup in `type_of` with a `match` on the raw byte
  — it was slightly slower.
- Short-circuiting `type_of` with an early `piece == make_piece(us, Pawn)` check
  — it was slower, suggesting the `type_of` branch is already well predicted.

### Current benchmark results

- `cargo test`, `cargo clippy`, and `cargo fmt` — pass.
- `cargo run --release --example verify_perft 6` — passes (41/41 positions),
  total time ~60 s (best observed ~59.8 s, worst observed ~60.5 s).
- `cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6` —
  passes, returns `118926425`, ~0.50 s (down from ~0.53 s after Plan 1).

### Why the remaining gap is unavoidable

The `type_of` decode is required because `Piece` is a packed byte: bits
`0..2` hold `piece_type + 1` and `NO_PIECE` is encoded as `0`. Any code that
needs the `PieceType` (to update `by_type`, check for pawn moves, restore a
blast-captured piece, etc.) must call `type_of`. Because `NO_PIECE` is a valid
`Piece` value, the function must bounds-check the lookup. The only way to
eliminate that branch completely is either:

1. Use `unsafe` (`std::mem::transmute` or `std::hint::unreachable_unchecked`); or
2. Change `Board::squares` to `[Option<Piece>; 64]` with `Piece` backed by a
   `NonZeroU8` type, so that `NO_PIECE` is unrepresentable as a `Piece`.

Both options are disallowed by the project rules (`unsafe` is forbidden and the
`Piece` representation is part of the public API). Therefore the remaining ~4 s
performance gap is the legitimate cost of the safe `Piece` representation.

### Trade-off assessment

The ~7% perft slowdown is an acceptable cost for the correctness guarantees
gained: `Piece`/`Square`/`Move` are now robust against invalid encodings, the
`do_move`/`undo_move` pair restores the position correctly through atomic blasts,
and the full `verify_perft` suite passes. The optimizations above recovered a
portion of the loss, and the remaining `type_of` overhead is the safest design
within the current constraints.
