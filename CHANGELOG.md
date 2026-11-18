# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.9.0] - 2026-11-18

### Added

- `EffectContext::exploding_capture_after(board, m)`, a read-only one-ply child
  query: the first legal capture by the child's side to move whose blast
  removes the commoner of the side that played `m`, or `None`. It is defined
  for pseudo-legal `m` (including illegal `m`), reuses the already-derived
  child view, and never clones or writes the parent board or populates a
  `StateInfo`. The returned move is the first hit in the child's pseudo-legal
  generation order; this is an existence/one-witness query, not a
  "best capture" query. No existing answer, key, move order, perft value, FEN
  output or `Outcome`/`Terminal` semantics change.
- `FenPolicy::Lichess`, an opt-in sanitizing FEN acceptance policy for the
  downstream `parity` check. Through the existing
  `Board::from_fen_with(fen, policy)` entry point it drops castling rights that
  are not backed by the required commoner and rook, drops an en-passant target
  that is not consistent with a double push, accepts excess material, and
  rejects the atomic `Setup` illegal-position class (empty board, the rejected
  missing/extra commoner configurations, back-rank pawns, opposite check, and
  the impossible-check class). `ValidationErrors::IMPOSSIBLE_CHECK` is added
  for the last class. `Board::from_fen` and every existing `FenPolicy` variant
  keep their exact previous answers and error precedence. The parser was split
  into a syntax-only phase and the existing structural checks; the frozen
  disposition data comes from an external shakmaty 0.30.2 run, with no new
  dependency, GPL code/data, or build-time network access.
- The child's Zobrist key under the selected en-passant policy.
  `EffectContext::post_move_and_key_with(board, m, mode)` returns the
  `PostMove` summary and the child key from one derivation.
  `EnPassantMode::Always` is exactly the existing
  `EffectContext::post_move_and_key()` and exactly `Board::hash_after()`.
  `EnPassantMode::Legal` is the same key as a cloned child's
  `hash_with(EnPassantMode::Legal)` and as `Board::hash_after_with(m, Legal)`:
  it removes the child's ep-file key when no fully legal en-passant capture
  exists. The legal-EP test runs on the already-derived child view, so a
  non-double-push move does no extra work and a double push is not cloned or
  re-derived. This is a canonicalization on the stable key table: no key,
  `ZOBRIST_VERSION`, move order, perft value, FEN output or terminality answer
  changes.

### Changed

- The default build is now genuinely safe Rust: `piece_key_raw` uses an
  ordinary indexed Zobrist load and the crate declares
  `forbid(unsafe_code)` unless `unsafe-optimizations` is enabled. The feature
  now removes the bounds checks from the Zobrist piece-key lookup and the
  magic-bitboard lookups; no public API or move-generation semantics change.
- Clock arithmetic is hardened without changing accepted positions: the
  internal game-ply counter is wider, `do_move`/`undo_move` restore it exactly
  across the old `u16` boundary, and the half-move clock saturates at
  `u16::MAX` instead of panicking in debug builds. The public `game_ply()` and
  `fullmove_number()` getters document their saturation.
- The structural castling and en-passant checks have one source of truth per
  predicate, shared by `from_fen`, `from_setup`, and `validate()`. Each caller
  keeps its documented permissiveness, so parser acceptance is unchanged for
  every previously valid input.
- The package is explicit: `Cargo.toml` uses an `include` list (no example
  binaries, docs, or repository artifacts), adds `categories`, `documentation`
  and `[package.metadata.docs.rs]`, and the published README says the examples
  live in the repository.
- `FenError` and `FenLoadError` now implement `Clone`, `PartialEq`, and `Eq`.
  They remain exhaustive (not `#[non_exhaustive]`) in this minor release.
- Raw attack-table helpers document their `Square::NONE` panic;
  `Board::blast_zone(Square::NONE)` is total and returns an empty bitboard.

### Fixed

- `Board::from_fen` no longer overflows in debug builds for a legal FEN whose
  fullmove number is large but fits in `u16` (for example `65535`). A
  fullmove field that does not fit `u16` remains `FenError::ParseInt`.
- `do_move`/`undo_move` no longer overflow at the old `u16` game-ply boundary,
  and quiet moves at `rule50 == u16::MAX` saturate instead of panicking in
  debug builds. Hashes and FEN output for ordinary positions are unchanged.

## [2.8.0] - 2026-11-13

The child's terminal state on an already derived summary, the child key next to
it, and a cheaper child-side existence question. Both additions are additive
API; every existing query answers exactly what it answered in 2.7.0. No move
order, perft value, hash, FEN output, or `Outcome`/`Terminal` semantics changed.

Measured, this stack is **cheaper than 2.7.0 on every child query but not
cheaper than building the child board** — see “Measured” below for the class
table and the verdict; the numbers are not a performance promise for any
consumer.

### Added

- The child's terminal state on an already derived summary.
  `PostMove::terminal()` returns the child's `Terminal { outcome, reason }` for
  every **pseudo-legal** move, equal to a cloned `do_move` child's
  `Board::terminal()` with the same precedence as
  `EffectContext::terminal_after()`. It *is* the implementation behind
  `EffectContext::terminal_after()` and `EffectContext::child_outcome()`, so a
  consumer that kept the summary pays for the child derivation once instead of
  calling `post_move()` and `terminal_after()` separately.
- The child's Zobrist key next to the summary. `EffectContext::post_move_and_key()`
  returns `(PostMove, u64)` from one derivation; the key is exactly
  `Board::hash_after()` and exactly the `hash()` of a cloned child (same key
  table, `EnPassantMode::Always`, so `ZOBRIST_VERSION` and its stability
  contract apply unchanged). Together with `PostMove::terminal()` and
  `PostMove::rule50()` one derivation answers "summary + key + terminality".

### Changed

- The child's existence question ("does the side to move have a legal move?")
  is answered by a positive screen before the exact ladder runs, and it no
  longer populates a `StateInfo` on the way. The screen proves one quiet
  non-commoner move legal from the placement view alone — a quiet move triggers
  no blast and empties only its origin, so it is legal when the mover's
  commoner is not attacked after it — and falls back to the unchanged
  fast-legality ladder for everything it cannot prove (in check, captures only,
  commoner moves only, decided positions).
  `EffectContext::has_legal_move_after`, `EffectContext::is_terminal_after`,
  `EffectContext::terminal_after`, `PostMove::terminal`,
  `EffectContext::child_outcome`, `movegen::has_legal_move` and
  `movegen::has_legal_move_with_state` all answer exactly what they answered
  before; only the cost changes. Measured on this crate's bench positions:
  `has_legal_move_after` 36 → 25–30 ns per child, `terminal_after` 43 → 30–34 ns,
  `has_legal_move` 6 → 2–3 ns per node.
- The child-side terminal precedence no longer populates a `StateInfo` for the
  child: the two commoner counts and "is the side to move in check" are read
  from the placement view. The precedence itself is unchanged and now lives in
  two helpers (`position::terminal_cheap` / `position::terminal_tail`) that the
  board path and the child path call in the same order; no `Outcome` or
  `Terminal` semantics changed, and `Board::terminal`/`outcome` behaviour is
  byte-identical (their inputs are the caller's populated `StateInfo`, as
  before).

### Measured

`cargo run --release --example effect_bench 9 2000`, three runs, medians, over
the pseudo-legal children of seeded play-outs from the 41-position perft corpus
and the three bench positions (49,338 children over 2,106 nodes; 13,926 of them
in handoff 3's *built-ish* class). ns per child, ratio to the class's build-only
row (`clone` + `do_move`) in the same run:

| class | children | `clone`+`do_move`+`outcome()`+key | build | `post_move` | `post_move`+`terminal()` | `post_move_and_key`+`terminal()` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| quiet | 35,412 | 37.9 (2.38×) | 15.9 | 17.8 (1.12×) | 32.1 (2.02×) | 43.5 (2.74×) |
| capture | 3,972 | 54.3 (1.86×) | 29.2 | 36.4 (1.25×) | 49.7 (1.70×) | 63.0 (2.16×) |
| promotion | 432 | 60.5 (3.56×) | 17.0 | 21.2 (1.25×) | 58.4 (3.44×) | 69.9 (4.11×) |
| en-passant | 8 | 46.0 (1.83×) | 25.1 | 21.1 (0.84×) | 45.2 (1.80×) | 57.8 (2.30×) |
| castling | 80 | 33.5 (2.15×) | 15.6 | 15.2 (0.97×) | 29.7 (1.90×) | 42.3 (2.71×) |
| commoner-move | 10,052 | 37.6 (2.38×) | 15.8 | 18.6 (1.18×) | 31.2 (1.97×) | 41.6 (2.63×) |
| **built-ish** | 13,926 | **42.5 (2.11×)** | 20.1 | 23.6 (1.17×) | 39.7 (1.98×) | **52.8 (2.63×)** |
| all | 49,338 | 39.5 (2.34×) | 16.9 | 19.4 (1.15×) | 32.8 (1.94×) | 44.6 (2.64×) |

**Verdict: the composite does not clear handoff 3's cost bars on this corpus.**
The summary stack (`post_move_and_key` + `PostMove::terminal()`) costs
**1.24× the clone path on `built-ish`** and **1.16× on `capture`** (1.11–1.26×
on every class, three runs), so the honest statement is that it is *not*
cheaper than `clone` + `do_move` + `outcome()` + key on this machine. The
terminal state on the summary without the key (`post_move` + `terminal()`) *is*
cheaper — 0.85–0.97× the clone path per class, 0.93× on `built-ish` — so the
shape is sound and the remaining cost is the child key plus the child-side
attack/existence work that the clone path gets from `populate_state`.

No claim is made about any downstream's wall clock, tree or gate; the numbers
above are this crate's corpus on this crate's machine, in the same run.

## [2.7.0] - 2026-11-13

Child terminality without a board write, a two-level child answer, the
post-move summary, and a legality-free classification of a move. The crate's
hot path (`Board::legal`, `populate_state`, the generator and the fast-legality
ladder) is unchanged since 2.6.0: no move order, perft value, hash, FEN output,
or `Outcome`/`Terminal` semantics changed.

### Added

- Move classification without the legality ladder. `MoveClassification`
  (`is_capture`, `casualties`, `checks_enemy`, `explodes_enemy_commoner`,
  `leaves_own_commoner`) is `MoveEffect` without `is_legal`, and is returned by
  `EffectContext::classification()`, which skips the `is_attacked_by` query for
  the mover's commoner. It is the query for a caller that already holds a legal
  move (for example one taken from `movegen::generate_legal_with_state()`) and
  therefore does not need the verdict a second time.
  `MoveEffect::classification()` returns the same value from a full effect, and
  both entry points share one derivation, so they cannot disagree. Measured
  saving on this crate's bench positions: ~5–7 ns per pseudo-legal move
  (`examples/effect_bench.rs`).
- Child terminality without a board write. `EffectContext::terminal_after()`
  (and the `Board::terminal_after()` wrapper) return the child position's
  `Terminal { outcome, reason }` for every **pseudo-legal** move, equal to
  `{ let mut c = board.clone(); c.do_move(m, &mut st); c.terminal() }`, with no
  clone, no `MoveList`, and no allocation. The child is described by a
  crate-private bitboard view (placement, side to move, en-passant target,
  castling rights, and the child clock), so “does the child have a legal
  move?” is answered by the same rules code the board uses. Illegal moves are
  not rejected — a pseudo-legal move whose blast removes the mover's own
  commoner has an extinct child and is reported as such.
- `EffectContext::is_terminal_after()` (the literal `Board::is_terminal()`
  mirror on the child, i.e. without the commoner-only material shortcut) and
  `EffectContext::has_legal_move_after()`.
- Two-level child answer. `AfterState` (`Illegal`, `Undecided`,
  `Terminal(Terminal)`, with `is_illegal`/`is_undecided`/`is_terminal`/
  `terminal`/`outcome` helpers) is returned by
  `EffectContext::child_outcome()` and the `Board::child_outcome()` wrapper, so
  “the move is illegal” and “the child is not decided” are separable without a
  second query. It is exact for pseudo-legal input: `Illegal` iff
  `Board::legal()` is `false`, `Terminal(t)` iff the cloned child's
  `terminal()` is `Some(t)`.
- Child summary. `PostMove` (placement, casualties, side to move, en-passant
  target, castling rights, half-move clock — facts only, no rules verdict) is
  returned by `EffectContext::post_move()`, and
  `EffectContext::effect_and_post()` returns `(MoveEffect, Option<PostMove>)`
  from one analysis pass. Both are derived without a board write; only an
  `EffectContext` can build a `PostMove`.
- `conformance::CHILD_CASES`: a frozen child-state corpus with one row per
  `AfterState` branch and per `TerminalReason` (position, move, expected
  `AfterState` from the child's side to move, child legal move count), verified
  field by field in `tests/conformance.rs`.

### Changed

- `EffectContext::effect()` is now `effect_and_post(..).0` with the five rules
  fields coming from `classification()`; its result is unchanged field for
  field (the effect differential in `tests/effects.rs` is untouched).
- `EffectContext::outcome_after()` lost its clone fallback: it is now
  `child_outcome(..).outcome()` — one analysis pass, with no board written,
  cloned or moved for any child. Signature and behaviour are unchanged
  (`None` = illegal **or** undecided).
- The rules live on one crate-private `PositionView`. `Board` stores that view
  as its placement, and `Board::legal`, `populate_state`, `compute_checkers`,
  `compute_pinned`, `is_material_draw`, `blast_casualties`, `is_capture`, the
  move generator, and the fast-legality ladder all read it. No public API,
  ordering, perft value, hash, FEN output, or `Outcome`/`Terminal` semantics
  changed.

### Fixed

- **Documentation only, no behaviour change:** `EffectContext::outcome_after()`
  and `Board::outcome_after()` now document that `None` means *illegal* **or**
  *undecided*, and point at the queries that separate the two. The previous
  text mentioned only the illegal case, which was a correctness trap for a
  consumer that read `None` as “the child is decided”.

## [2.6.0] - 2026-11-13

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
