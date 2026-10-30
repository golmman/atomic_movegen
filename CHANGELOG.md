# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `Board::checkers` no longer reports a check when the two commoners touch.
  Each side has only one commoner, so a capture of either would blast the
  capturer's own commoner away. Touching therefore makes the commoner immune
  to every attack. Previously the legal-move generator already applied this
  immunity, but `checkers()` disagreed with it.
- `Board::legal` rejects every move that destroys one of the mover's
  commoners, not just moves that leave the mover with none. In legal atomic
  positions there is only one, so this is the same self-explosion rule; it also
  removes the artificial “spare commoner” exception.
- `generate_legal` and `has_legal_move` return no moves when the game is already
  over because either side has no commoner left. A board with only one commoner
  is terminal, matching Fairy-Stockfish's `generate<LEGAL>`.
- `Board::outcome` now reports a loss (instead of a draw) when the side to move
  has no legal moves *and* its commoner is attacked. Atomic chess has no
  checkmate in the usual sense, but an attacked commoner with no escape is lost
  (Fairy-Stockfish's `stalemate_value` for `extinctionPseudoRoyal`). True
  stalemates (no legal moves, commoner not attacked) remain draws.

### Changed

- Documented the atomic invariant that each side has at most one commoner:
  there is no promotion to commoner, a board has one or two commoners, and a
  board with only one commoner is terminal. Removed regression tests that used
  impossible multi-commoner FENs.

### Added

- Regression tests for touching-commoner immunity (including castling through
  an adjacent enemy commoner), terminal positions, and the absence of
  commoner promotion.
- Two atomic perft positions from shakmaty's `tests/atomic.perft` suite
  ("programfox"), verified against Fairy-Stockfish to depth 6.

## [2.3.1] - 2026-10-27

### Changed

- Internal, behavior-preserving performance work; the public API is unchanged.
- Pawn generation is now table-driven and order-preserving: push, double-push,
  capture, and en-passant targets come from precomputed attack/push tables,
  removing per-target square arithmetic, file/wrap guards, and repeated board
  lookups. Measured about 6.9% faster on both fixed depth-6 perft positions.
- Attack detection for commoner safety and castling now uses a boolean
  early-exit probe instead of building an attacker bitboard. Measured about
  1.3-1.6% faster on the fixed depth-6 positions.
- The trivial-legality predicate is hoisted per node and classifies candidates
  from precomputed state, avoiding repeated piece lookups and work that is
  unnecessary for quiet moves. Measured about 2.2-2.3% faster on the fixed
  depth-6 positions.

### Added

- Additional move-order golden positions covering pawn-rich and promotion
  cases, plus unit tests for the new pawn push/double-push tables.

## [2.3.0] - 2026-09-10

### Added

- `Board::hash_after(m: Move) -> u64`: the Zobrist hash of the position after
  `m`, computed without modifying the board. Exactly equal to
  `{ let mut b = board.clone(); b.do_move(m, &mut st); b.hash() }` for every
  pseudo-legal move, but with no board, bitboard, or undo-record writes.
  Intended for consumers that prefetch transposition-table lines before
  evaluating children.
- `Board::rule50_after(m: Move) -> u16`: the half-move clock `do_move(m, ..)`
  would leave, without modifying the board (castling increments; a pawn move
  or a move onto an occupied square resets to 0; anything else increments).
- Equivalence tests pinning both functions against `do_move`:
  a seeded random-playout property test (5 start FENs × 16 seeds × 300-ply
  walks, exactly 8,592 positions, every pseudo-legal move checked at every
  position) and deterministic fixtures covering castling (both sides, both
  colors), en-passant with a blasting capture, promotions (quiet and
  capturing, all four pieces), blast removals that clear castling rights via
  the presence cascade (corner rook; e1/e8 commoner), a double push that
  replaces an existing en-passant square, and rule50 reset/increment cases.

### Changed

- Internal, behavior-preserving: the castling-rights transition (origin/
  destination clearing plus the post-blast rook/commoner-presence cascade) is
  now a single pure helper shared by `do_move` and `hash_after`, so the two
  paths cannot drift. Verified unchanged by the full gate suite, including
  41/41 perft positions at depth 6 and the move-order goldens.

### Performance

`hash_after` vs a `do_move`/`hash`/`undo_move` round-trip over all legal
moves (`--release`, fat LTO, 2M timed iterations per position):

| Position | Legal moves | `hash_after` | round-trip | Speedup |
|---|---:|---:|---:|---:|
| Quiet middlegame | 42 | 4.5 ns | 12.7 ns | 2.8× |
| Reference position | 43 | 4.6 ns | 12.4 ns | 2.7× |

Per move class (same code, per-class loops): quiet moves 3.1–3.6 ns
(≈ 3× the quiet round-trip), captures 8.6–9.0 ns (captures pay for the
blast-set iteration). The remaining cost on the quiet path is two dependent
memory loads (piece-on-square, then Zobrist piece key); removing them would
require an incremental per-square key array on `Board`, which was out of
scope for this additive-only release.

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
