# Implementation plan for `feedback.md`

## Validity summary

All feedback points are valid or mostly valid.

| # | Feedback | Validity |
|---|----------|----------|
| 1 | Incremental Zobrist `Board::hash()` | Valid. `Board` currently has no hash at all. Adding it is the biggest performance win. |
| 2 | `Board::rule50()` and `Board::game_ply()` getters | Valid. The fields are already tracked. |
| 3 | `generate_legal` should reuse a `StateInfo` | Valid. Add `generate_legal_with_state(board, state, moves)`. |
| 4 | `Move`/`MoveList` helpers | Valid. All requested helpers are useful and trivial. |
| 5 | `Board::outcome()`/`is_terminal()` | Valid. `Board` already tracks `rule50` and commoner counts. |
| 6 | Deprecate `attacks::init()` | Valid. It is a no-op. Should be deprecated, not removed, for backward compatibility. |
| 7 | Fix pseudo-royal adjacency docs | Valid. The code already allows touching commoners; the docs are misleading. |
| 8 | `Board::fen()` writes `game_ply` as fullmove counter | Valid bug, confirmed: after `1. e4` the FEN currently ends with `0 2` instead of `0 1`. |
| 8 | `Piece::color()`/`type_of()` are unsafe on `NO_PIECE` | Valid. `color()` returns `White` for `NO_PIECE` in release, and `type_of()` can OOB-index. |

### Feedback the user already corrected themselves

The adjacent-commoner "bug" is not a bug; the code is correct. Only the docs need to change.

---

## Pushback

1. **`game_ply` is a raw per-ply counter, not the FEN fullmove counter.**
   - We will expose `game_ply()` as the raw counter, and add `fullmove_number()` for FEN output.
   - `Board::fen()` will use `fullmove_number()`, and `from_fen()` will convert the FEN fullmove into the raw ply.
2. **`attacks::init()` and `magic::init()` will be removed entirely.**
   - Since we are releasing a breaking `2.0.0`, there is no reason to keep a no-op `init` in the public API.
3. **`Piece::color()`/`type_of()` returning `Option` is a breaking API change.**
   - This plan implements that change and bumps the crate to `2.0.0`.
   - If a non-breaking `1.1.0` is preferred, an alternative is to add `Piece::color_opt()`/`type_opt()` and keep the existing methods as panicking accessors.
4. **`Board::outcome()` will call `generate_legal()` for stalemate detection.**
   - It is a convenience, not a zero-cost pre-check; it first checks commoners/`rule50` and only generates moves when those are not decisive.

---

## Implementation plan

### 1. Add incremental Zobrist hash

New file: `src/zobrist.rs`.

- Add a `pub(crate) static ZOBRIST: ZobristKeys` with keys generated at compile time by a small `const fn` PRNG (e.g. splitmix64).
- Components:
  - `piece[64][16]` — keyed by `Piece::raw()` (`0` for `NO_PIECE` is all zeros).
  - `side: u64` — XORed when `side_to_move == Black`.
  - `castling[16]` — keyed by the full castling-rights bitmask.
  - `ep[9]` — keyed by ep file; index `8` is the "no ep" key (all zeros).
- Add `pub(crate) const fn Piece::raw(self) -> u8` to `src/types.rs` so `Board` can index the table.

Changes to `src/board.rs`:

- Add `hash: u64` to `Board`.
- Add `pub(crate) fn recompute_hash(&mut self)`.
- Add `pub fn hash(&self) -> u64`.
- Add `state.hash: u64` to `StateInfo` (store the pre-move hash).
- In `do_move`, set `state.hash = self.hash` at the very start, then update `self.hash` incrementally as pieces are moved/removed/placed and as side/castling/ep change.
- In `undo_move`, restore `self.hash = state.hash` at the end (after all pieces are restored).
- In `from_fen`, call `recompute_hash()` before returning the board.
- Hash must include **pieces, side to move, castling rights, en-passant file**. It must **not** include `rule50` or `game_ply`.

### 2. Expose `rule50`, `game_ply`, `fullmove_number`

In `src/board.rs`:

- `pub fn rule50(&self) -> u16`
- `pub fn game_ply(&self) -> u16` — raw per-ply counter, 1-based.
- `pub fn fullmove_number(&self) -> u16` — `1 + (self.game_ply - 1) / 2`.

Fix `Board::from_fen()`:

- Parse FEN fullmove as `fm`.
- If 6 fields, set `game_ply = 2 * (fm - 1 + (if side_to_move == White { 0 } else { 1 }))`.
- If 4 fields, default `game_ply` to `1` for White-to-move, `2` for Black-to-move.
- Validate `fm > 0` (return `FenError::ParseInt` otherwise).

Fix `Board::fen()`:

- Output `self.fullmove_number()` instead of `self.game_ply`.

### 3. `generate_legal_with_state`

In `src/movegen.rs`:

- Add `pub fn generate_legal_with_state(board: &Board, state: &StateInfo, moves: &mut MoveList)`.
- It does **not** call `populate_state`; it assumes `state` is already populated.
- It calls `generate_pseudo_legal(board, moves)` and filters using `is_move_trivially_legal` and `board.legal(m, state)`.
- Refactor `generate_legal` to:
  ```rust
  pub fn generate_legal(board: &Board, moves: &mut MoveList) {
      let mut state = StateInfo::new();
      board.populate_state(&mut state);
      generate_legal_with_state(board, &state, moves);
  }
  ```

### 4. `Move`/`MoveList` helpers

In `src/types.rs`:

- `MoveList::clear(&mut self)` -> `self.len = 0`.
- `Move::is_castling(self) -> bool`
- `Move::is_promotion(self) -> bool`
- `Move::is_en_passant(self) -> bool`
- `Move::to_uci(self) -> String`:
  - Returns `"0000"` for `Move::NONE`.
  - Maps castling `to_sq` to `g1`/`c1`/`g8`/`c8`.
  - Appends promotion char (`n`/`b`/`r`/`q`) for promotions.

In `src/board.rs`:

- `pub fn is_capture(&self, m: Move) -> bool`:
  ```rust
  m.move_type() == MoveType::EnPassant
      || (m.move_type() != MoveType::Castling
          && self.piece_on(m.to_sq()) != NO_PIECE)
  ```

### 5. `Board::outcome()` / `is_terminal()`

In `src/types.rs`:

- Add `pub enum Outcome { Win, Loss, Draw }` (from the side-to-move perspective).
- Re-export it in `src/lib.rs` (`pub use types::{..., Outcome};`).

In `src/board.rs`:

```rust
pub fn outcome(&self) -> Option<Outcome> {
    let us = self.side_to_move;
    if self.commoners(us).is_empty() {
        return Some(Outcome::Loss);
    }
    if self.commoners(us.flip()).is_empty() {
        return Some(Outcome::Win);
    }
    if self.rule50 >= 100 {
        return Some(Outcome::Draw);
    }
    let mut moves = MoveList::new();
    crate::movegen::generate_legal(self, &mut moves);
    if moves.is_empty() {
        return Some(Outcome::Draw); // stalemate
    }
    None
}

pub fn is_terminal(&self) -> bool {
    self.outcome().is_some()
}
```

This matches the Fairy-Stockfish atomic defaults: commoner extinction = `-VALUE_MATE`, stalemate = `VALUE_DRAW`, `nMoveRule = 50` (so `rule50 >= 100` plies = draw).

### 6. Remove `attacks::init()` and `magic::init()`

In `src/attacks.rs`:

- Delete `pub fn init()`.

In `src/magic.rs`:

- Delete `pub(crate) fn init()`.

Remove all internal `init()` calls from:

- `src/attacks.rs` tests
- `src/magic.rs` tests
- `src/board.rs` tests
- `src/movegen.rs` tests
- `tests/verify_moves.rs`
- `examples/*.rs`

### 7. Fix documentation for pseudo-royal adjacency

- `src/lib.rs` top-level doc: remove "cannot move next to an enemy commoner"; state that touching enemy commoners is allowed and does not count as an attack.
- `src/board.rs` `Board::legal()` doc: remove "pseudo-royal adjacency" as a restriction; mention that the last commoner is immune from adjacent commoner checks.

### 8. Fix `Piece::color()`/`type_of()` safety

In `src/types.rs`:

```rust
pub fn color(self) -> Option<Color> {
    if self.0 == 0 { return None; }
    Some(if self.0 & 8 == 0 { Color::White } else { Color::Black })
}

pub fn type_of(self) -> Option<PieceType> {
    if self.0 == 0 { return None; }
    Some(PIECE_TYPES[(self.0 & 7) - 1 as usize])
}
```

Update all call sites in `src/board.rs`, `src/types.rs` `Display`/`ascii_char`, and tests to `unwrap()` where the piece is known to be non-empty.

### 9. Version bump

- `Cargo.toml`: `version = "2.0.0"` (because of the `Piece` API change).
- `README.md`: update the dependency example to `atomic-movegen = "2.0.0"`.

---

## Verification

- `cargo build` must be warning-free under `#![warn(missing_docs)]`.
- `cargo clippy` must pass.
- `cargo fmt` must pass.
- `cargo test` must pass.
- `cargo run --example verify_perft` must still pass all 41 perft positions.
- `cargo test --test verify_moves` must pass.
- Add new unit tests:
  - `board::tests::test_do_undo_restores_hash` and `test_hash_after_moves`.
  - `board::tests::test_fen_fullmove_counter` and `test_from_fen_fullmove_conversion`.
  - `board::tests::test_outcome_*` for loss/win/draw/stalemate.
  - `board::tests::test_is_capture`.
  - `board::tests::test_rule50_game_ply_fullmove_getters`.
  - `types::tests::test_move_helpers` and `test_move_to_uci`.
  - `types::tests::test_piece_no_piece`.
  - `movegen::tests::test_generate_legal_with_state`.
- For `hash` correctness, compare `self.hash` against `recompute_hash()` in the test after every `do_move`/`undo_move`.

---

## Files to touch

- `src/lib.rs` — docs and `pub use types::Outcome;`
- `src/types.rs` — `Move`/`MoveList` helpers, `Piece` `Option` return, `Outcome`, `Piece::raw`
- `src/zobrist.rs` — new module with `ZobristKeys` and `ZOBRIST`
- `src/board.rs` — `hash`, getters, `outcome`/`is_terminal`, `is_capture`, FEN fix, `do_move`/`undo_move` hash updates, doc fixes
- `src/movegen.rs` — `generate_legal_with_state`
- `src/attacks.rs` — remove `init()`
- `src/magic.rs` — remove `init()` and test calls
- `tests/verify_moves.rs` — remove `attacks::init()` call
- `examples/*.rs` — remove `attacks::init()` calls
- `Cargo.toml` — version bump to `2.0.0`
- `README.md` — update dependency version string
