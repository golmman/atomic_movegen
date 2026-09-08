# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.2.0] - 2026-09-08

### Added

- `has_legal_move(board: &Board) -> bool` and
  `has_legal_move_with_state(board: &Board, state: &StateInfo) -> bool` in
  `movegen`: an early-exit legal-move existence query. Both return at the
  first legal move and are guaranteed equivalent to
  `!generate_legal_with_state(board, state, list).is_empty()` on every
  position (verified by an equivalence property test over ~8,600 randomly
  played positions plus targeted fixtures). Measured speedup versus
  `generate_legal` (`--release`, 2M iterations per position):

  | Position          | Legal moves | `generate_legal` | `has_legal_move` | Speedup |
  |-------------------|------------:|-----------------:|-----------------:|--------:|
  | Quiet middlegame  | 42          | 87.8 ns          | 8.9 ns           | 9.9×    |
  | Start position    | 20          | 49.6 ns          | 8.8 ns           | 5.6×    |
  | In check          | 4           | 38.0 ns          | 12.1 ns          | 3.1×    |
  | Captures only     | 2           | 40.6 ns          | 12.9 ns          | 3.2×    |
  | No legal move     | 0           | 125.6 ns         | 125.3 ns         | 1.0×    |

  (A no-legal-move position necessarily costs one full candidate-stream
  traversal: absence can only be proven by exhausting the stream.)

### Changed

- Internal, behavior-preserving: `generate_pseudo_legal` is now a thin
  wrapper over a new `for_each_pseudo_legal` candidate stream that walks the
  same stages in the same order (pawns → knights → bishops → rooks → queens →
  commoners → castling) and supports early exit. The exact generation order
  of both `generate_pseudo_legal` and `generate_legal` is pinned by the new
  `tests/move_order_golden.rs` golden test, generated from the
  pre-refactor implementation.

## [2.1.0] - 2026-08-24

### Changed

- `Board::fen()` and `Piece::ascii_char()` now serialize the commoner (the
  king-like pseudo-royal piece of atomic chess) with the standard FEN letter
  `K`/`k` instead of the non-standard `C`/`c`. This matches Fairy-Stockfish's
  `atomic` variant and every other standard atomic FEN.
- `Board::from_fen()` now accepts only the standard piece letters; FENs that
  label the commoner with `C`/`c` are rejected with `FenError::InvalidPlacement`.
  Previously both `C`/`c` and `K`/`k` were accepted.

### Added

- Regression tests: `test_fen_uses_standard_commoner_label`,
  `test_from_fen_rejects_nonstandard_commoner_label` (in `src/board.rs`), and
  `test_commoner_ascii_char_is_standard` (in `src/types.rs`).

## [2.0.0] - 2026-07-12

### Added

- Incremental Zobrist hashing for `Board` via a new `Board::hash()` method. The hash includes pieces, side to move, castling rights, and en-passant file; it does not include `rule50` or `game_ply`.
- `src/zobrist.rs` with compile-time generated Zobrist keys.
- `Board::rule50()`, `Board::game_ply()`, and `Board::fullmove_number()` getters.
- `Board::outcome()` and `Board::is_terminal()` for game-over detection (commoner extinction, 50-move rule, stalemate).
- `Board::is_capture(m: Move)` for capture detection.
- `generate_legal_with_state(board, state, moves)` in `movegen` for callers that want to reuse a pre-populated `StateInfo`.
- `MoveList::clear()` and `Move` helpers: `is_castling()`, `is_promotion()`, `is_en_passant()`, and `to_uci()`.
- `Outcome` enum (`Win`, `Loss`, `Draw`) re-exported from `src/lib.rs`.
- `Piece::raw()` for internal Zobrist indexing.

### Changed

- `Piece::color()` and `Piece::type_of()` now return `Option<Color>` and `Option<PieceType>` respectively, instead of panicking or returning incorrect values for `NO_PIECE`.
- `Board::fen()` now outputs the correct FEN full-move counter (`fullmove_number()`) instead of the raw ply counter.
- `Board::from_fen()` now converts the FEN full-move counter into the correct 1-based `game_ply` value and validates that the full-move counter is greater than zero.
- `Board::legal()` and top-level documentation now correctly state that touching an enemy commoner is allowed and does not count as an attack.

### Removed

- `attacks::init()` and `magic::init()` no longer exist; all attack tables are precomputed at compile time and require no runtime initialization.
- All example binaries and tests no longer call `attacks::init()`.

## [1.0.0] - 2026-07-05

### Added

- Initial release of `atomic-movegen`.
- Legal move generation for the standard atomic chess variant.
- FEN parsing and output with support for commoners (`C`/`c`/`K`/`k`).
- Perft implementation and `verify_perft` example.
- Pseudo-legal and legal move generation with atomic blast-on-capture rules.
- Commoner pseudo-royalty for the last commoner.
- Pure safe Rust, zero dependencies.
