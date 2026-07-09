# Plan 3 Report: FEN parsing and board-state correctness

This report documents the changes made in `docs/plans/review1/plan3.md` to make
FEN parsing robust, fix castling-rights edge cases, and ensure move generation
cannot produce impossible moves from stale or malformed board state.

## Changes

### 1. `rule50` migrated from `u8` to `u16`

- `Board::rule50` is now `u16`.
- `StateInfo::rule50` is now `u16`.
- `Board::from_fen` parses `parts[4]` as `u16`.
- `Board::fen()` already used `to_string()` and works for `u16`.
- `do_move` increments with `self.rule50 += 1` safely for all realistic FEN
  inputs.
- `Board` size remains 136 bytes on `aarch64-apple-darwin` and `StateInfo` size
  remains 56 bytes; the larger `u16` field fits into the existing padding.

### 2. Stricter FEN piece-placement validation

`Board::from_fen` now validates the board section in several ways:

- Rejects the character `'0'` as an empty-square count.
- Ensures `col + digit <= 8` for every digit.
- Ensures `col < 8` before placing a piece, so pieces cannot wrap onto the next
  rank.
- Errors on any unrecognized character.
- Errors if a rank does not sum to exactly 8 columns.
- New `FenError::InvalidPlacement(String)` carries a descriptive message.

### 3. Reject 5-field FEN

Only 4 or 6 space-separated fields are accepted. Any other count returns the
new `FenError::InvalidFieldCount { got }`. The `Display` message clearly states
"FEN must have 4 or 6 fields, got {got}".

### 4. Validate castling rights against actual pieces

`from_fen` now checks every set castling right against the squares it requires:

- `WK`/`WQ` need a white commoner on `E1` and the corresponding rook on `H1`
  or `A1`.
- `BK`/`BQ` need a black commoner on `E8` and the corresponding rook on `H8`
  or `A8`.

If a right is set without the required pieces, `FenError::InvalidCastling` is
returned.

### 5. Validate `ep_square` against the actual pawn

`from_fen` now checks that an `ep_square` is on the correct rank
(`R6` for white to move, `R3` for black to move) and that the square
immediately behind it contains an enemy pawn. Otherwise it returns
`FenError::InvalidEpSquare`.

### 6. Fixed `update_castling_rights`

- Added an `is_capture` parameter so rights are only cleared based on `to` when
  a capture or en-passant actually reaches the square.
- Added `to == Square::E1` / `to == Square::E8` checks to clear both white / both
  black castling rights when the commoner is captured on its starting square.
- `from == E1/E8/H1/A1...` checks for king/rook moves remain unchanged.
- The post-blast cleanup block also now clears all castling rights for a color
  if the commoner on `E1`/`E8` was destroyed by the blast, complementing the
  `to`-based capture checks.

### 7. `generate_castling` sanity checks

Before pushing a castling move, `generate_castling` now verifies that the
commoner and the relevant rook are actually present on their expected squares.
This prevents `generate_pseudo_legal` from returning impossible moves when
rights are stale during a search.

### 8. `generate_pawn_moves_for` EP victim check

Before generating an en-passant move, the generator now checks that the square
behind the `ep_square` contains an enemy pawn (`make_piece(them, Pawn)`). This
prevents stale `ep_square` values from producing phantom moves.

### 9. `Board::pinned` color filtering

`compute_pinned` now ORs `between & self.pieces_color(us)` into `pinned`, so
`Board::pinned(c)` returns only pieces of color `c`. Previously it could include
enemy pieces that happened to be between a commoner and a sniper.

### 10. Removed `fix_halfmove_clock` workaround

`tests/verify_moves.rs` no longer needs the `fix_halfmove_clock` helper. With
`rule50` as `u16`, FEN half-move clocks larger than 255 parse correctly.

## FENs that failed validation

All FENs in `perft_values.md` and `tests/moves.md` parsed successfully under the
new rules.

One hand-written test FEN needed correction:

- `src/board.rs` `test_en_passant_blast`: was
  `4k3/8/8/2Pp4/8/8/8/4K3 w KQkq d6 0 2`, which requests `KQkq` but has no
  rooks on the back rank. It was corrected to
  `4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2`.

## Performance / correctness impact

- `update_castling_rights` changes and the new `generate_castling` sanity checks
  did not change any perft node counts.
- `Board::pinned` color filtering did not change perft results (it is used only
  for legality filtering and now produces the correct color-restricted result).
- `from_fen` validation adds a small one-time parse cost; no impact on perft
  runtime because FEN parsing is not on the hot path.

## Unexpected observations

- `Board` and `StateInfo` sizes did not grow when `rule50` became `u16` because
  the wider field fits into the existing struct padding.
- `from_fen` validation and `generate_castling` sanity checks form complementary
  defenses: `from_fen` rejects malformed FENs with stale castling / EP rights,
  while `generate_castling` catches stale rights that can still arise during
  search (e.g., a commoner or rook destroyed by an atomic blast).
- The `verify_perft` suite validated that the `moves.md` and `perft_values.md`
  FENs are well-formed, which gives confidence that the stricter parser is
  compatible with the existing test data.

## Verification

- `cargo build` — passed.
- `cargo clippy` — passed.
- `cargo fmt` — passed.
- `cargo test` — passed (33 unit tests, 4 perft tests, 1 move verification test).
- `cargo test --test verify_moves` — passed.
- `cargo run --release --example verify_perft 5` — passed (41/41 positions).
- `cargo run --release --example verify_perft 6` — passed (41/41 positions),
  total time 56.832 s.

## Notes for plan 4

- `rule50` is `u16` and `from_fen` / `fen()` are robust.
- Castling and EP generation are guarded against stale state.
- `Board::pinned` now returns color-filtered results.
- The `tests/verify_moves.rs` `fix_halfmove_clock` hack has been removed.
