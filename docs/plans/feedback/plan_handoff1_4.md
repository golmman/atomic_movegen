# Plan 4 — Interchange: SAN and piece-set setup (MG8, MG9)

## Scope

Add the two remaining interchange pieces from R1:

- **MG8**: SAN parsing and formatting, board-aware and legality-checked.
- **MG9**: construction and inspection of a position from a piece set rather
  than a FEN string.

Both are additive. They depend on plan 2 only for SAN's check/mate suffix; the
move resolution itself can use the existing legal generator if plan 2 is not
yet merged.

## Current-tree assessment

- FEN input/output, UCI parsing/formatting, `Board::move_from_uci`, and
  `Move::write_uci` exist.
- There is no SAN parser or formatter.
- `Board` can only be constructed from `new`, `from_fen`, `from_fen_with`, or
  `Clone`. There is no public piece-by-piece builder and no piece iterator
  other than `piece_on` per square.
- The commoner uses the standard FEN letter `K`/`k`; SAN should use `K` for the
  commoner piece letter, not `C`.
- Castling is internally encoded as commoner-takes-rook; SAN must emit
  `O-O`/`O-O-O` and resolve those back to the internal move.

## Decisions and pushback

1. **SAN is board-aware and legality-checked.** `Board::move_from_san` returns
   `Some(Move)` only for exactly one legal move. Ambiguous SAN returns `None`;
   it is not resolved by an arbitrary order.
2. **The commoner's SAN letter is `K`.** This is consistent with the FEN label
   and with standard tools. Document that `K`/`k` mean the atomic commoner, not
   a FIDE king.
3. **Castling accepts `O-O`/`O-O-O` and the zero variants.** Accept `0-0` and
   `0-0-0` on input for interoperability; always emit `O-O`/`O-O-O`.
4. **Check/mate suffixes are optional on input and generated on output.** Parse
   strips one trailing `+` or `#` and otherwise ignores it. Formatting emits:
   - `#` when the move leaves the opponent with `Outcome::Loss` (extinction or
     no moves while attacked);
   - `+` when the opponent is in check but not lost;
   - no suffix otherwise.
   Use plan 2's `outcome_after` when available; otherwise a cloned
   `do_move` + `outcome()` + `checkers()` is acceptable for the first
   implementation.
5. **Do not add an error enum for SAN.** `Board::san` and
   `Board::move_from_san` return `Option`; `Board::write_san` may return
   `fmt::Result` and document that it expects a legal move. This keeps the
   surface small and matches the UCI helpers.
6. **`Setup` is a data builder, not a second board representation.** It stores
   a square array plus the six FEN-style state fields, and `Board::from_setup`
   constructs a `Board` and recomputes the hash. It must not duplicate movegen
   or legality.
7. **`Board::from_setup` mirrors `from_fen` permissiveness; validated
   construction is opt-in.** Add `from_setup_with(setup, policy)` using the
   existing `FenPolicy`/`ValidationErrors` machinery. `from_setup` remains
   permissive so terminal and artificial fixtures can be built, exactly as
   `from_fen` allows them.
8. **Inspection is allocation-free.** Add `Board::pieces()` returning an
   iterator over `(Square, Piece)` in ascending square order, skipping empty
   squares. Do not add a `Vec`-returning `piece_list` unless a consumer asks.

## Implementation

### `src/board.rs` — SAN

Add:

```rust
impl Board {
    /// Return the SAN representation of `m`, or `None` if `m` is not legal.
    ///
    /// Castling is `O-O`/`O-O-O`; pawn captures use the origin file; piece
    /// moves use standard disambiguation (file, then rank, then both); pawn
    /// promotions use `=Q`, `=R`, `=B`, `=N`. A trailing `#` or `+` is added
    /// according to the child outcome/check state.
    #[must_use]
    pub fn san(&self, m: Move) -> Option<String>;

    /// Write SAN without allocating.
    ///
    /// `m` must be legal. Returns the underlying `fmt::Error` if the writer
    /// fails.
    pub fn write_san(&self, m: Move, out: &mut impl fmt::Write) -> fmt::Result;

    /// Resolve SAN text to a legal move.
    ///
    /// Accepts `O-O`/`O-O-O` and `0-0`/`0-0-0`, optional `+`/`#`, optional
    /// capture `x`, standard piece letters (`N`, `B`, `R`, `Q`, `K`), optional
    /// disambiguation by file, rank, or both, and promotion `=Q`/`Q` forms.
    /// Returns `None` for ambiguous, malformed, or illegal input.
    #[must_use]
    pub fn move_from_san(&self, s: &str) -> Option<Move>;
}
```

Formatter algorithm:

1. Verify `self.is_legal(m)`; return `None` otherwise.
2. Castling: write `O-O` or `O-O-O` from the internal rook square.
3. Pawn:
   - non-capture: write the destination square;
   - capture: write the origin file, `x`, destination;
   - promotion: append `=Q`/`=R`/`=B`/`=N`.
4. Piece:
   - write the piece letter (`N`, `B`, `R`, `Q`, `K`);
   - collect all legal moves of the same piece type to the same destination
     (ignoring promotion piece differences; pawns are handled above);
   - if more than one, disambiguate by origin file if unique, else by origin
     rank if unique, else by both;
   - write `x` if `board.is_capture(m)`;
   - write the destination.
5. Suffix:
   - clone + `do_move` + `outcome()`; if `Outcome::Loss` for the opponent,
     write `#`; else if the child is in check, write `+`.

Parser algorithm:

1. Reject empty strings and non-ASCII.
2. Strip one trailing `+` or `#`; reject any other trailing punctuation.
3. Castling: match `O-O`, `O-O-O`, `0-0`, `0-0-0` (case-sensitive `O`/`0`),
   map to the side's `Board::move_from_uci("e1g1")`/`e1c1`/`e8g8`/`e8c8`; return
   the result only if legal.
4. Promotion suffix: strip `=Q`, `=R`, `=B`, `=N`, or a trailing `Q`, `R`, `B`,
   `N` after a pawn destination. Reject promotion to `K`/commoner.
5. Destination: the last two square characters.
6. Capture flag: a literal `x` before the destination.
7. Pawn move: if the remaining prefix is a file letter, it is a pawn capture;
   if empty, a pawn push. Reject a pawn move with a piece letter.
8. Piece move: the first character is a piece letter. The remaining prefix
   before the optional `x` is optional disambiguation. Accept a file letter, a
   rank digit, or a full square (e.g. `Nb1d2`).
9. Generate legal moves; filter by:
   - moving piece type;
   - destination square;
   - capture flag when present;
   - promotion piece when present;
   - origin file/rank/square when present.
10. Return `Some` only if exactly one candidate remains.

Do not use `to_uci()` string comparisons for resolution; compare `Move` fields
and the board's `is_capture`.

### `src/board.rs` / new `src/setup.rs` — piece-set construction

Add a new `Setup` type. Keep it in a new `src/setup.rs` to avoid growing
`board.rs`; expose it from `lib.rs`.

```rust
/// A piece-set description used to build a [`Board`] without a FEN string.
#[derive(Debug, Clone)]
pub struct Setup {
    squares: [Piece; 64],
    side_to_move: Color,
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u16,
    game_ply: u16,
}

impl Default for Setup { /* empty board, White to move, no rights/ep, clocks 0/1 */ }

impl Setup {
    /// Start from an empty board.
    pub fn new() -> Self;
    /// Start from an empty board (alias for `new`; keep one name).
    pub fn empty() -> Self;
    /// Place or replace the piece on `sq`.
    pub fn place(&mut self, sq: Square, piece: Piece) -> &mut Self;
    /// Remove the piece on `sq`.
    pub fn remove(&mut self, sq: Square) -> &mut Self;
    /// Set the side to move.
    pub fn set_side_to_move(&mut self, c: Color) -> &mut Self;
    /// Set the raw castling-rights bits.
    pub fn set_castling_rights(&mut self, rights: u8) -> &mut Self;
    /// Set the en-passant target.
    pub fn set_ep_square(&mut self, sq: Option<Square>) -> &mut Self;
    /// Set the half-move clock.
    pub fn set_rule50(&mut self, n: u16) -> &mut Self;
    /// Set the raw game ply (1-based).
    pub fn set_game_ply(&mut self, n: u16) -> &mut Self;
    /// Read a square.
    pub fn piece_on(&self, sq: Square) -> Piece;
    /// Iterate over occupied squares in ascending square order.
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_;
}

impl Board {
    /// Build a board from a piece-set description.
    ///
    /// Mirrors `from_fen` permissiveness: structural checks only (valid
    /// squares, castling rights with the required pieces, en-passant geometry).
    /// It does not require one commoner per side.
    pub fn from_setup(setup: &Setup) -> Result<Board, SetupError>;

    /// Build and apply a semantic validation policy.
    pub fn from_setup_with(
        setup: &Setup,
        policy: FenPolicy,
    ) -> Result<Board, FenLoadError>;

    /// Iterate over occupied squares in ascending square order.
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_;
}
```

Prefer reusing `FenLoadError`/`ValidationErrors` for `from_setup_with` so the
validation layer has one vocabulary. For `from_setup`, add a small
`SetupError` if a structural error cannot be expressed by `FenError`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    /// A castling right is set without the required commoner and rook.
    InvalidCastlingRight,
    /// The en-passant target is not on the expected rank or has no enemy pawn
    /// behind it.
    InvalidEnPassant,
    /// The game ply is zero (FEN fullmove numbers are 1-based).
    InvalidGamePly,
}
```

`from_setup` builds the `Board` fields directly (they are in the same module or
behind crate-private accessors), sets `hash = recompute_hash()`, and returns.
Do not route through FEN strings.

`Board::pieces()` uses `types::SQUARES` and `piece_on`, skipping `NO_PIECE`.

## Tests

### SAN

- `san_roundtrip_all_legal_moves`: over the order-golden corpus and seeded
  random play-outs, every legal move's SAN parses back to the same move.
- `san_start_position`: `e4`, `Nf3`, `Nc3`, etc.
- `san_disambiguation`: two knights to the same square (file/rank/both), two
  rooks on the same file/rank, and a pinned/blocked variant where only one is
  legal.
- `san_castling`: both sides/colors, `O-O`/`O-O-O`, zero variants on input.
- `san_pawn_capture_en_passant`: `exd5`, `exd6` en passant, promotion captures.
- `san_promotion`: all four promotion pieces, both push and capture, `=Q` and
  `Q` input forms, reject `=K`.
- `san_check_and_mate_suffix`: `+` for a check that is not mate, `#` for a
  child `Outcome::Loss`, no suffix otherwise. Include an extinction `#` if the
  chosen definition covers it.
- `san_rejects_ambiguous`: deliberately ambiguous text returns `None`.
- `san_rejects_illegal`: wrong side, blocked move, impossible destination,
  malformed text, trailing junk.
- `write_san_matches_san`: `write_san` into a `String` equals `san()`.

### Setup

- `setup_matches_fen`: for a set of FENs (startpos, middlegame, ep, castling,
  promotion, terminal), parse FEN → build `Setup` from `Board::pieces()` and
  state → `Board::from_setup` → compare FEN, hash, hash_with(Legal), side, ep,
  rule50, game_ply, castling rights.
- `setup_place_remove`: place/remove/replace pieces; `piece_on` and `pieces`
  reflect the final state.
- `setup_castling_and_ep_validation`: invalid rights and invalid ep geometry
  return the expected error.
- `setup_with_policy`: `from_setup_with` rejects an empty board under
  `FenPolicy::Strict` and accepts it under `Permissive`/`AllowTerminal`.
- `board_pieces_order`: ascending square order and no empty squares.
- `setup_is_send_sync` if plan 5 has landed.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (at least depth 5)
- `cargo test --test move_order_golden`
- `cargo test --test verify_moves`
- A SAN round-trip over every legal move in the 41 perft positions at depth 3
  (a new test or an example), to catch disambiguation and suffix drift.

## Files to touch

- `src/board.rs` — SAN parse/format, `Board::pieces`, docs.
- `src/setup.rs` — new `Setup`, `SetupError`, tests.
- `src/lib.rs` — modules/re-exports and docs.
- `README.md` / `CHANGELOG.md` — SAN and Setup.
- `docs/plans/feedback/report_handoff1_4.md` — final report.

## Non-goals

- No PGN parser/writer, no annotation glyphs (`!`, `?`, `!!`), no long
  algebraic notation as a separate format.
- No full SAN validation of move numbers or comments.
- No replacement of FEN; `Setup` is an additional construction path.
- No change to move order, perft, hash, FEN output, or `Outcome`.

## Final task

Write `docs/plans/feedback/report_handoff1_4.md` with the exact SAN grammar
implemented, the disambiguation/suffix rules, the Setup API, round-trip and
validation results, changed files, and any interoperability choices (e.g.
`0-0` accepted, `K` means commoner).
