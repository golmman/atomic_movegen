# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Frozen conformance corpus. `atomic_movegen::conformance::CASES` (also
  re-exported as `CONFORMANCE_CASES`) is a curated, public array of
  `ConformanceCase` values covering the start position, middlegames, en
  passant, castling (including an attacked path), promotions, checks, mate,
  stalemate, extinction, material and 50-move draws, pins, blast wins,
  self-explosions, the largest observed legal move count (68), and the 14
  `tests/perft_tests.rs` positions. Each case records the FEN, ordered
  pseudo-legal and legal UCI moves, outcome, terminal reason, `is_terminal`,
  both en-passant key modes, `rule50`, and `game_ply`. `tests/conformance.rs`
  verifies every field against the engine.
- Terminal reasons. `TerminalReason` (`Extinction`, `NoMovesInCheck`,
  `Stalemate`, `MaterialDraw`, `FiftyMoveRule`) and `Terminal { outcome,
  reason }` are re-exported at the crate root. `Board::terminal()`,
  `terminal_with_state()`, and `terminal_with()` mirror `Board::outcome()`'s
  precedence through one shared classifier, so a consumer can explain every
  `Some(Outcome)`. The reason includes the commoner-only material draw and is
  therefore not the same as `Board::is_terminal()`; this is documented.
- `ZOBRIST_VERSION` (`1`): a public version for the stable, FSF-compatible
  Zobrist key schema, for persisted transposition tables.
- Published public contracts: generation order (pinned by
  `tests/move_order_golden.rs`), Zobrist key stability and `EnPassantMode::Legal`
  as a canonicalization, `Send + Sync` for the public types, an allocation-free
  move path, and the `#[non_exhaustive]` semver policy. Documented on the crate,
  `Board::hash`/`hash_with`, the generators, every non-exhaustive enum, and in
  the README.
- `Outcome` now also derives `Hash` (additive; no variant or precedence change).
- SAN parsing and formatting. `Board::san()` (allocating) and
  `Board::write_san()` (allocation-free) format a **legal** move in standard
  algebraic notation: `O-O`/`O-O-O`, standard disambiguation by origin file,
  then rank, then both, `=Q`/`=R`/`=B`/`=N` promotions, pawn-capture origin
  files, and a trailing `#`/`+` taken from the child outcome (extinction and
  mate/stalemate loss are `#`, a check that continues is `+`). The commoner's
  letter is `K`, as in FEN. `Board::move_from_san()` resolves SAN text to
  exactly one legal move and returns `None` for ambiguous, malformed, or
  illegal input; it accepts `O-O`/`O-O-O` and `0-0`/`0-0-0`, one trailing
  `+`/`#`, an optional capture `x`, `=Q` and bare `Q` promotion forms (either
  case, `K` rejected), and disambiguation by file, rank, both, or a full
  origin square. Both directions are round-trip tested over the 41-position
  perft corpus (`tests/interchange.rs`, `examples/verify_san.rs`).
- Piece-set construction and inspection. `Setup` (`atomic_movegen::setup`)
  builds a position from `(Square, Piece)` pairs plus the side to move,
  castling rights, en-passant target, half-move clock, and game ply.
  `Board::from_setup()` mirrors the permissiveness of `Board::from_fen()` and
  reports structural problems as `SetupError` (`InvalidCastlingRight`,
  `InvalidEnPassant`, `InvalidGamePly`); `Board::from_setup_with()` applies the
  existing `FenPolicy`/`ValidationErrors` layer. `Board::pieces()` and
  `Setup::pieces()` iterate the placement in ascending square order without
  allocating. `Setup` is a data builder only: it duplicates no move generation
  or legality logic.
- `Piece`, `NO_PIECE`, and `make_piece` are re-exported at the crate root, so a
  piece set can be built without naming the `types` module.
- `examples/san.rs` prints the legal moves of a position as SAN + UCI and
  verifies the SAN round-trip; `examples/verify_san.rs` runs the SAN and
  `Setup` round-trips over the whole perft corpus (depth 3 in ~6 s release).
- `EffectContext` and `MoveEffect` (`atomic_movegen::effects`): a
  position-constant context plus a read-only per-move classification — capture,
  blast casualties, checks the enemy commoner, explodes the enemy commoner,
  keeps the mover's commoner, and legality. Build it with
  `Board::effect_context()` / `Board::effect_context_with_state()`.
  `EffectContext::is_legal` equals `Board::legal` for pseudo-legal input and
  `EffectContext::outcome_after` (also `Board::outcome_after()`) equals the
  child's `Board::outcome()` for legal input; both are differentially tested
  against the board-writing path.
- `movegen::for_each_legal()` and `movegen::for_each_legal_with_state()`: an
  allocation-free, early-exit stream over the legal moves in generation order,
  the legal sibling of `for_each_pseudo_legal()`.
- `examples/effect_bench.rs`: a dependency-free microbenchmark comparing the
  effect path with `Board::legal`, `do_move` + `outcome`, and `for_each_legal`
  with `generate_legal_with_state`.
- `Board::null_for(attacker)` returns the null counterpart of a position
  without a FEN round-trip: same pieces and castling rights, `attacker` to
  move, no en-passant target, preserved clocks, and a recomputed hash.
- `movegen::for_each_pseudo_legal()` is now public. It walks the pseudo-legal
  candidate stream in generation order and supports early exit via
  `ControlFlow`; `generate_pseudo_legal()` remains the collecting wrapper.
- `StateInfo` is re-exported at the crate root.
- `CastlingSide` and `Board::has_castling_right(color, side)` provide a typed
  castling-rights query. The raw `Board::castling_rights()` accessor is
  unchanged.
- `attacks::between()`, `attacks::xray_rook_attacks()`,
  `attacks::xray_bishop_attacks()`, and `attacks::xray_queen_attacks()` expose
  the between-squares and x-ray helpers needed for engine-side legality
  shortcuts.

### Changed

- Faster legal move generation. `generate_legal`, `generate_legal_with_state`,
  `for_each_legal`, `for_each_legal_with_state`, and `has_legal_move_with_state`
  share one per-node fast-legality ladder that decides more candidates from
  node-constant bitboards instead of calling `Board::legal`: captures that empty
  no blocker of an enemy slider's ray to the mover's commoner, blasts that
  remove every enemy commoner, self-explosions, quiet commoner relocations, and
  commoner captures are now decided directly. `Board::legal` remains the oracle
  for everything the ladder cannot decide (checks, castling, pinned origins,
  blasts that open a slider ray). No public API, move order, perft value,
  Zobrist key, FEN output, or `Outcome` changed. Measured on the 41-position
  perft corpus at depth 5: 10–12 % fewer instructions and ~4 % less wall time;
  the legal stream (`for_each_legal`) is ~40 % cheaper per node than before.
- `movegen::for_each_legal` / `for_each_legal_with_state` now filter with the
  shared ladder instead of `EffectContext::is_legal`. The visited sequence is
  unchanged (still exactly the `generate_legal` list, in order).
- The pseudo-legal generator skips the castling path when the side to move has
  no castling rights, instead of walking the castling checks per node. Order is
  unchanged.
- The “is this square attacked?” rule is now one shared helper
  (`board::is_attacked_by`) used by `Board::legal` and the effect path. No
  move-generation, perft, hash, FEN, or `Outcome` behaviour changed.

## [2.4.0] - 2026-11-02

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
- Mate now outranks the 50-move clock in `Board::outcome`: a checkmate on the
  100th half-move is a loss, not a draw (Fairy-Stockfish's
  `rule50 > 99 && (!checkers() || MoveList<LEGAL>(pos).size())` ordering).
- `Board::outcome` now reports the commoner-only K-vs-K position as a WDL draw,
  even before the 50-move rule expires. Such a board with a fresh clock is
  still not literal game over (`is_terminal() == false`).

### Changed

- `Board::is_terminal` is now the strict, literal game-over predicate:
  extinction, no legal moves, or `rule50 >= 100`. It deliberately excludes the
  commoner-only WDL shortcut used by `outcome()`.

- Documented the atomic invariant that each side has at most one commoner:
  there is no promotion to commoner, a board has one or two commoners, and a
  board with only one commoner is terminal. Removed regression tests that used
  impossible multi-commoner FENs.

### Added

- Combined child-key accessor `Board::hash_and_rule50_after(m)`: returns
  `(hash_after(m), rule50_after(m))` from a single move classification. It is
  a consumer convenience for child-key prefetch and does not replace either
  separate function; `hash_after` and `rule50_after` are unchanged.
- Opt-in `unsafe-optimizations` cargo feature (off by default) that removes the
  bounds check from the magic-bitboard table lookup. Indices remain provably
  in bounds by the magic-table construction invariants; the feature changes no
  public API and no move-generation semantics.
- Canonical atomic blast API: `Board::blast_zone(sq)` returns the full 3×3
  blast geometry (including the center and pawn squares), and
  `Board::blast_casualties(m)` returns a `BlastCasualties` value with the
  removed white and black pieces plus the en-passant victim square. The helper
  mirrors `do_move` without mutation and is covered by a random-playout
  equivalence test.
- Convenience helpers for consumers that previously re-derived rules:
  `attacks::attacks_from(pt, color, sq, occupied)`,
  `Board::attackers_to_color(sq, color, occupied)`, board-aware
  `Board::is_zeroing(m)` and `Board::is_double_push(m)`, and
  `MoveList::retain(f)` with order preservation.
- Corrected the `MAX_MOVES` and `MoveList::push` documentation: the largest
  observed count is 68 legal moves (all 41 perft positions through depth 4),
  256 is a conservative safety margin rather than a proven tight bound, and a
  release-mode push at capacity is ignored (not an overwrite).
- Opt-in semantic FEN validation: `Board::validate()` and
  `Board::from_fen_with(fen, policy)` with `FenPolicy::{Strict, AllowTerminal,
  AllowImpossibleCheck, Permissive}` and a `ValidationErrors` bitflags type.
  `FenLoadError` keeps syntax failures (`FenError`) separate from
  `InvalidPosition` failures. The permissive `Board::from_fen()` is unchanged;
  strict validation checks commoner counts, back-rank pawns, castling and
  en-passant geometry, a conservative material bound, and opposite check. It
  deliberately does not reject FSF-style ep targets that have no legal ep
  capture, and it is not a reachability proof.
- Canonical en-passant handling for position keys and FEN output.
  `Board::legal_ep_square()` returns the stored target only when at least one
  fully legal en-passant capture exists (pins, self-explosion, and commoner
  attacks are respected). The new `EnPassantMode` enum selects between
  `Always` (the FSF-compatible default) and `Legal` in `Board::hash_with()`,
  `Board::fen_with()`, and `Board::hash_after_with()`. `hash()`, `fen()`, and
  `hash_after()` are unchanged and remain `Always`, so existing stored keys
  and FEN output keep their current values. With `Legal`, positions that
  differ only by a non-capturable ep target share a key and a FEN.
- Stable `Move` wire encoding: `Move::to_bits()` and `Move::from_bits()`. The
  packing (`to:6 | from:6 | type:2 | promotion_type:2`), `Move::NONE == 0`,
  and the `PROMOTION_PIECES` order (queen, rook, bishop, knight) are now
  semver-stable and can only change in a major release. `from_bits` validates
  structure only: `0` is `Move::NONE`, equal origin/destination is rejected,
  and all other 16-bit patterns decode. `Move` now also derives `Hash`,
  `PartialOrd`, and `Ord`; ordering follows the wire encoding.
- UCI move helpers: syntax-only `Move::from_uci()` (no board needed, `0000` is
  `Move::NONE`, castling text decodes as a normal two-square move),
  allocation-free `Move::write_uci()`, and `Display` for `Move`.
  `to_uci()` now allocates once and delegates to `write_uci`.
- `Board::move_from_uci()`: board-aware UCI lookup that resolves the four
  castling destinations and en-passant targets to the internal encodings and
  returns the move only if it is fully legal. If no special encoding is legal,
  the ordinary parsed move is checked last, so a rook move such as `e8g8`
  still resolves when castling is unavailable.
- Single-move legality queries without building or cloning a `MoveList`:
  `Board::is_pseudo_legal()` (equivalent to membership in
  `generate_pseudo_legal`), `Board::is_legal()`, and
  `Board::is_legal_with_state()` (equivalent to membership in
  `generate_legal_with_state`).
- `Board::outcome_with_state(&StateInfo)` and
  `Board::outcome_with(&StateInfo, moves_exist: bool)`: the same precedence as
  `outcome()`, reusing a populated state and optionally a caller-known
  legal-move-existence answer. `outcome()` now uses this path and no longer
  builds a `MoveList`.
- `Board::is_material_draw()`: true only when both colors have exactly one
  commoner and nothing else. This is intentionally the only material draw;
  there is no insufficient-material table. K+B/K+N/K+R vs K counterexamples
  pin that a lone piece can win immediately by capturing the enemy commoner.
- A single `terminal_precedence_table` regression test covering touching
  commoners, checkmate, mate on the 50-move clock, extinction, K vs K (fresh
  and expired clock), and the artificial empty board; plus state-aware outcome
  equivalence tests over fixtures and seeded random playouts.
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
