# Feedback Plan Report

## Summary

All feedback points from `docs/plans/feedback/feedback.md` were implemented.

The library now provides an incremental Zobrist hash, exposes `rule50` / `game_ply` / `fullmove_number`, supports `generate_legal_with_state`, adds `Move`/`MoveList` helpers and `Board::is_capture`, exposes `Board::outcome`/`is_terminal`, fixes the FEN full-move counter, makes `Piece::color`/`type_of` return `Option`, removes the no-op `attacks::init()` and `magic::init()`, and corrects the pseudo-royal adjacency documentation.

The crate version was bumped to `2.0.0` because of the breaking `Piece` API change.

## Implementation

| # | Feedback | Change |
|---|----------|--------|
| 1 | Incremental Zobrist `Board::hash()` | Added `src/zobrist.rs` with compile-time generated keys. `Board` now carries a `hash: u64` and `StateInfo` stores the pre-move hash. `do_move`/`undo_move` update the hash incrementally via `move_piece`/`remove_piece`/`place_piece` and `update_hash_state_transition`. `from_fen` calls `recompute_hash()`. |
| 2 | `Board::rule50()` / `Board::game_ply()` getters | Added `rule50()`, `game_ply()`, and `fullmove_number()` getters. `game_ply` is a 1-based raw ply counter; `fullmove_number()` is `1 + (game_ply - 1) / 2`. |
| 3 | `generate_legal` should reuse `StateInfo` | Added `generate_legal_with_state(board, state, moves)` in `src/movegen.rs`. `generate_legal` now creates a `StateInfo`, populates it, and calls `generate_legal_with_state`. |
| 4 | `Move`/`MoveList` helpers | Added `MoveList::clear`, `Move::is_castling`, `Move::is_promotion`, `Move::is_en_passant`, `Move::to_uci`, and `Board::is_capture`. |
| 5 | `Board::outcome()` / `is_terminal()` | Added `Outcome` enum and `Board::outcome()` / `Board::is_terminal()`. Checks commoner extinction, 50-move rule (100 plies), and stalemate. |
| 6 | Deprecate/remove `attacks::init()` | Removed `pub fn init()` from `src/attacks.rs`, `pub(crate) fn init()` from `src/magic.rs`, and all call sites in examples, tests, and `src` tests. |
| 7 | Fix pseudo-royal adjacency docs | Updated top-level `src/lib.rs` docs and `Board::legal` doc comment to clarify that touching enemy commoners is allowed and does not count as an attack. |
| 8 | `Board::fen()` full-move counter bug | `fen()` now outputs `fullmove_number()`. `from_fen()` converts the FEN fullmove counter into the correct `game_ply` value and validates `fullmove > 0`. |
| 8 | `Piece::color()`/`type_of()` safety | `Piece::color()` and `Piece::type_of()` now return `Option<Color>` / `Option<PieceType>`. `Piece::raw()` was added for internal Zobrist indexing. All call sites were updated. |

## Verification

```sh
cargo build       # ok, no warnings
cargo test        # ok
cargo clippy --all-targets  # ok
cargo fmt --check # ok
cargo doc         # ok
cargo test --test verify_moves  # ok
```

Full perft verification:

```sh
cargo run --release --example verify_perft 6
```

```
Result: 41/41 passed, 0/41 failed
Total time: 53.865 s
```

## Files Changed

| File | Change |
|------|--------|
| `Cargo.toml` | Version bumped to `2.0.0`. |
| `README.md` | Dependency example updated to `2.0.0`. |
| `src/lib.rs` | Top-level docs updated; `Outcome` re-exported. |
| `src/types.rs` | Added `Outcome`, `Move`/`MoveList` helpers, `Piece::raw`, and `Piece::color`/`type_of` `Option` API. |
| `src/zobrist.rs` | New module: `ZobristKeys` and `ZOBRIST` static. |
| `src/board.rs` | Added `hash`, `StateInfo.hash`, `recompute_hash`/`recomputed_hash`, `hash`, `rule50`, `game_ply`, `fullmove_number`, `outcome`, `is_terminal`, `is_capture`; FEN `game_ply`/`fullmove_number` fix; `do_move`/`undo_move` incremental hash updates; doc fixes. |
| `src/movegen.rs` | Added `generate_legal_with_state`; refactored `generate_legal`; removed `attacks::init()` test calls. |
| `src/attacks.rs` | Removed `init()`; removed test `init()` calls. |
| `src/magic.rs` | Removed `init()`; removed test `init()` calls. |
| `examples/*.rs` | Removed `attacks::init()` calls. |
| `tests/verify_moves.rs` | Removed `attacks::init()` call. |
| `docs/plans/feedback/report.md` | This report. |

## Notes

- `Piece::color()` and `Piece::type_of()` returning `Option` is the breaking change that required the `2.0.0` bump.
- `Board::game_ply()` is the raw 1-based ply counter; `Board::fullmove_number()` is the FEN-style full-move counter. `from_fen` converts the FEN fullmove to `game_ply` and `fen()` outputs `fullmove_number()`.
- `attacks::init()` and `magic::init()` were removed entirely rather than deprecated because the `2.0.0` bump already allows a breaking change.
- The Zobrist hash includes pieces, side to move, castling rights, and en-passant file; it does not include `rule50` or `game_ply`.
