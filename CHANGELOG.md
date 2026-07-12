# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
