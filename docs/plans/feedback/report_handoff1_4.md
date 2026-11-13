# Report — Plan 4: Interchange (MG8 SAN, MG9 piece-set setup)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| MG8 SAN formatting (`Board::san`, `Board::write_san`) | Done | Board-aware, legality-checked; `#`/`+` suffix from the child outcome via `EffectContext` (plan 2), no board writes except `outcome_after`'s mate/stalemate fallback. |
| MG8 SAN parsing (`Board::move_from_san`) | Done | Exactly one legal candidate or `None`; castling, zero-variant castling, optional `x`, `=Q`/`Q` promotions (either case), file/rank/square disambiguation, trailing `+`/`#` stripped. |
| MG8 SAN tests | Done | 11 unit tests plus two corpus-wide differentials (round-trip and suffix-vs-child) in `tests/interchange.rs`. |
| MG9 `Setup` builder | Done | New `src/setup.rs`: square array + the six state fields, fluent setters, allocation-free `pieces()`. No movegen or legality logic duplicated. |
| MG9 `Board::from_setup` / `from_setup_with` | Done | Structural `SetupError` (castling, en-passant geometry, game ply) and reuse of `FenPolicy`/`ValidationErrors` for the semantic layer. |
| MG9 `Board::pieces()` | Done | Ascending square order, no allocation, `impl Iterator<Item = (Square, Piece)>`. |
| Plan-2 dependency (suffix) | Used as planned | `EffectContext::effect(...).checks_enemy` and `EffectContext::outcome_after(...)`; no fallback path was needed. |
| Deep SAN round-trip over the perft corpus | Done | `examples/verify_san.rs`: depth 3 (507 k positions, 14.0 M moves) and depth 4 (14.0 M positions, 413 M moves) both 41/41. |
| Correctness gates | Done | `cargo test` (149 unit, 10 effects, 3 interchange, 1 order golden, 2 null-for, 4 perft, 1 verify-moves, 8 doctests), clippy `-D warnings`, `fmt --check`, `doc`, perft 41/41 at depths 1–6, `unsafe-optimizations` and `--release` runs. |

No existing behaviour changed: move order, perft values, Zobrist keys, FEN
output, and `Outcome` are untouched. Everything added is additive public API
(san/write_san/move_from_san/pieces, `Setup`, `SetupError`, and the root
re-exports of `Piece`, `NO_PIECE`, `make_piece`).

## SAN grammar as implemented

### Formatting (`Board::san` → `Option<String>`, `Board::write_san` → `fmt::Result`)

1. Legality gate: `is_legal_with_state` with one populated `StateInfo`. `san`
   returns `None`, `write_san` returns `Err(fmt::Error)` (no separate SAN error
   type, matching the plan's decision 5) for an illegal move.
2. Castling: `O-O` when the internal target is the h-file rook, `O-O-O` for the
   a-file rook. The internal commoner-takes-rook encoding is translated, exactly
   like `Move::write_uci`.
3. Pawn:
   - push → destination square (`e4`),
   - capture → origin file, `x`, destination (`exd5`, en passant included),
   - promotion → `=Q`/`=R`/`=B`/`=N` appended (`e8=Q`, `exd8=R`).
4. Piece: piece letter (`N`, `B`, `R`, `Q`, `K`; `K` is the atomic **commoner**),
   then disambiguation, then `x` for a capture, then the destination.
5. Suffix: `#` when the child's `Outcome` is `Loss` for the opponent
   (extinction/explosion, checkmate, or stalemate while attacked), else `+` when
   `MoveEffect::checks_enemy` (proved equal to `!child.checkers().is_empty()` by
   `tests/effects.rs`), else nothing.

### Disambiguation rule (emitted by the formatter, counted over *legal* moves)

Among the legal moves of the same piece type to the same destination (castling
candidates excluded), if the mover is the only one, no disambiguator is written.
Otherwise: origin file when no rival shares the file, else origin rank when no
rival shares the rank, else the full origin square (`Nbd2`, `R1a2`, `Qa4c4`).
Only legal rivals count, so a pinned twin never forces a disambiguator
(`4r1k1/8/8/8/8/5N2/4N3/4K3 w` writes `Nd4` for the unpinned knight).

### Parsing (`Board::move_from_san`)

1. Reject empty and non-ASCII input.
2. Strip all trailing `+`/`#` (so `++` and `e4+` are accepted); any other
   trailing character makes the destination parse fail.
3. `O-O`/`O-O-O` and `0-0`/`0-0-0` (case-sensitive `O`/`0`) → the side's
   `e1g1`/`e1c1`/`e8g8`/`e8c8` through `move_from_uci`, i.e. legal castling only.
4. Promotion suffix: `=Q`/`=R`/`=B`/`=N`, `Q`/`R`/`B`/`N`, and the lower-case
   `=q`/`=r`/`=b`/`=n` forms. `K` is never a promotion piece, so `e8=K`/`e8K`
   return `None`.
5. Destination = the last two characters (`parse_sq`); a `x` immediately before
   it sets the capture constraint.
6. Prefix: empty → pawn push; one file letter → pawn capture; a piece letter
   followed by nothing, one file letter, one rank digit, or a full square →
   piece move with that disambiguation; anything else is rejected.
7. Filter the legal stream (`for_each_legal_with_state`, allocation-free) by
   piece type, destination, promotion piece (absent promotion text rejects
   promotion moves), the capture flag when present, and the origin
   file/rank/square when present. Exactly one survivor is returned; two
   survivors (e.g. `Nd2` with two knights, `Qc3` with two queens) return `None`.

Resolution compares `Move` fields and `Board::is_capture`; no UCI string
comparison is used.

## `Setup` API as implemented

```rust
pub struct Setup { /* squares: [Piece; 64] + 6 state fields */ }

impl Setup {
    pub fn new() -> Self;                          // + Default
    pub fn place(&mut self, sq: Square, piece: Piece) -> &mut Self;
    pub fn remove(&mut self, sq: Square) -> &mut Self;
    pub fn set_side_to_move(&mut self, c: Color) -> &mut Self;
    pub fn set_castling_rights(&mut self, rights: u8) -> &mut Self;
    pub fn set_ep_square(&mut self, sq: Option<Square>) -> &mut Self;
    pub fn set_rule50(&mut self, n: u16) -> &mut Self;
    pub fn set_game_ply(&mut self, n: u16) -> &mut Self;
    pub fn piece_on(&self, sq: Square) -> Piece;
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_;
    // getters: side_to_move, castling_rights, ep_square, rule50, game_ply
}

impl Board {
    pub fn from_setup(setup: &Setup) -> Result<Board, SetupError>;
    pub fn from_setup_with(setup: &Setup, policy: FenPolicy) -> Result<Board, FenLoadError>;
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_;
}

pub enum SetupError { InvalidCastlingRight, InvalidEnPassant, InvalidGamePly }
```

`Board::from_setup` mirrors `from_fen` permissiveness: only the structural
checks run (a set castling right needs the commoner on `e1`/`e8` and the rook on
`h1`/`a1`/`h8`/`a8`; an en-passant target needs the right rank and the enemy pawn
behind it; the game ply must be ≥ 1). Missing commoners, pawns on the back rank,
and other game-state oddities are accepted, so terminal fixtures still build.
`from_setup_with` runs the same `FenPolicy`/`ValidationErrors` layer as
`from_fen_with`; the structural errors map onto the closest existing `FenError`
variant (`InvalidCastling`, `InvalidEpSquare`, `ParseInt`) so the validation
layer keeps one vocabulary.

`Board::from_setup` builds the bitboards and the Zobrist key directly through a
new crate-private `Board::from_setup_parts`; it never routes through a FEN
string and allocates nothing.

## Verification

### Round-trip and differential results

| Check | Result |
| --- | --- |
| `tests/interchange.rs::san_roundtrips_the_perft_corpus` (41 corpus roots, depth 2, debug) | 19,034 positions / 507,305 moves, every SAN parses back to its move. |
| Same test, coverage counters (non-vacuity) | 836 castles, 7,184 promotions, 44,464 captures, 24,020 disambiguated, 10,593 `+`, 2,165 `#`. |
| `tests/interchange.rs::san_suffix_matches_the_child_position` | For the same 507,305 moves, the suffix equals a cloned `do_move` child's `outcome() == Loss` / `checkers()` verdict — identical counts (2,165 `#`, 10,593 `+`, 494,547 none). |
| `tests/interchange.rs::setup_reproduces_the_perft_corpus` | 19,034 corpus positions rebuilt from `Board::pieces()` + state: identical FEN, `hash`, `hash_with(Legal)`, `outcome`, and legal move list **in order**. |
| `examples/verify_san 3` (release) | 41/41 corpus positions, 507,346 positions, 14,003,226 moves, 6.26 s. |
| `examples/verify_san 4` (release) | 41/41 corpus positions, 14,003,267 positions, 413,113,000 moves, 194.1 s. |
| `examples/san "FEN"` | Prints `SAN<TAB>UCI` per legal move and marks any round-trip failure with `(!)`. |

### Functional tests added (20 unit tests, 3 integration tests, 6 doctests)

- `src/board.rs`: `san_start_position`, `san_disambiguation`,
  `san_castling`, `san_pawn_capture_en_passant`, `san_promotion`,
  `san_promotion_with_mate_suffix`, `san_check_and_mate_suffix`,
  `san_rejects_ambiguous_and_illegal`, `write_san_matches_san`,
  `san_keeps_commoner_letter`, `san_roundtrips_every_legal_move` (fixtures plus
  four seeded 120-ply play-outs).
- `src/setup.rs`: `setup_defaults`, `setup_matches_fen` (nine fixtures:
  startpos, middlegame, en passant, castling, promotion, clock, terminal,
  explosion, bare commoner – comparing FEN, hashes, state, per-type bitboards,
  checkers/pins, and outcome), `setup_midgame_matches_fen_after_moves` (40-ply
  play-out), `setup_place_remove`, `setup_castling_and_ep_validation`,
  `setup_with_policy`, `setup_permissive_accepts_artificial_boards`,
  `board_pieces_order`, `setup_error_display`.
- `tests/interchange.rs`: the three corpus tests above.

### Gates

| Gate | Result |
| --- | --- |
| `cargo test` | Pass: 149 unit (20 new), 10 `effects`, 3 `interchange` (new), 1 `move_order_golden`, 2 `null_for`, 4 `perft_tests`, 1 `verify_moves`, 8 doctests (6 new). |
| `cargo test --release` | Pass (same suite). |
| `cargo test --features unsafe-optimizations` | Pass (same suite; no magic/attack code touched). |
| `cargo clippy --all-targets -- -D warnings` | Pass. |
| `cargo fmt --check` | Pass. |
| `cargo doc --no-deps` | Pass, no warnings. |
| `cargo run --release --example verify_perft 6` | **41/41 at depths 1–6**, 66.07 s (plan 3 measured 65.8 s — unchanged, as expected for additive code). |
| `cargo run --release --example verify_perft 5` | 41/41, 2.27 s. |
| `cargo test --test move_order_golden` | Pass, unchanged. |
| `cargo test --test verify_moves` | Pass, unchanged. |

### Cost (release, 200 k iterations over one position's move list)

| Position | `san` | `write_san` | `move_from_san` |
| --- | ---: | ---: | ---: |
| startpos (20 moves) | 125 ns/move | 116 ns/move | 89 ns/move |
| middlegame-13 (31 moves) | 187 ns/move | 179 ns/move | 150 ns/move |
| promotion-heavy (22 moves) | 174 ns/move | 167 ns/move | 159 ns/move |

Interchange cost only: no generator, attack, or hashing code was touched, so
perft is unaffected.

Host: AMD Ryzen 9 5950X (4 vCPU visible), rustc 1.99.1 (58cf8f9c3 2026-10-13),
Linux. Release profile as in `Cargo.toml`.

## Changed files

- `src/board.rs` — `Board::san`, `Board::write_san`, `Board::move_from_san`,
  `write_san_legal`, `write_san_disambiguation`, the SAN helper functions
  (`san_piece_char`, `san_piece_type`, `parse_file_char`, `parse_rank_char`,
  `file_char`, `rank_char`, `write_sq`), `Board::pieces`, the crate-private
  `Board::from_setup_parts`, `validation_errors` promoted to `pub(crate)`, and
  the 11 new SAN tests.
- `src/setup.rs` — new module: `Setup`, `SetupError`, `From<SetupError> for
  FenLoadError`, `Board::from_setup`, `Board::from_setup_with`, and 9 tests.
- `src/lib.rs` — `pub mod setup`, root re-exports (`Setup`, `SetupError`,
  `Piece`, `NO_PIECE`, `make_piece`), and the new "Interchange: SAN and piece
  sets" module-doc section.
- `tests/interchange.rs` — new: SAN round-trip, SAN suffix differential, and
  `Setup` identity over the perft corpus.
- `examples/san.rs` — new: SAN + UCI listing per legal move.
- `examples/verify_san.rs` — new: corpus-wide SAN/`Setup` verification.
- `README.md` — feature bullets, a SAN/`Setup` usage section, and the two new
  example binaries.
- `CHANGELOG.md` — Unreleased "Added" entries for SAN, `Setup`, the root
  re-exports, and the examples.
- `docs/plans/feedback/report_handoff1_4.md` — this report.

No dependency, feature flag, or `unsafe` was added; no `Fairy-Stockfish` code
was consulted or linked.

## Interoperability choices

- **`K` means the atomic commoner**, not a FIDE king, in FEN and in SAN.
  Promotion to `K` is rejected in both directions.
- **`0-0`/`0-0-0` are accepted on input** and never emitted.
- **The capture `x` is optional on input** (`ed6`), mandatory on output.
- **The `+`/`#` suffix is ignored on input** and generated on output; the
  emitting rule is the child position, not a separate check heuristic, so SAN
  agrees with `Board::outcome()` and `Board::checkers()` by construction.
- **Promotion with and without `=`** is accepted; the canonical output is
  `=Q`-style, which is what PGN uses in practice.
- **Ambiguity is never resolved by order**: exactly one legal candidate or
  `None`. Long algebraic text (`e2e4`) is not accepted, except in the
  full-origin disambiguation form that SAN itself defines (`Nb1d2`).

## Problems encountered

1. **`checks_enemy` versus `child.checkers()`.** The suffix needs the child's
   check state, and reaching it with a board write would cost a
   `clone` + `do_move` per formatted move. Plan 2's `MoveEffect::checks_enemy`
   is exactly `!child.checkers().is_empty()` (already differentially tested in
   `tests/effects.rs`), so `san` builds one `EffectContext` per call and reuses
   it for both the suffix decision and `outcome_after`, eliminating all board
   writes except `outcome_after`'s mate/stalemate fallback. The new corpus-wide
   differential (`san_suffix_matches_the_child_position`) re-proves the
   equivalence for 507 k moves through the SAN path.
2. **First test fixtures were artificial, not merely unusual.**
   `4R1k1/8/8/8/8/8/8/4K3 w` has the *black* commoner already attacked with
   White to move, so `Kd1+` and `Rxg8#` both appeared. The extinction suffix
   case was rewritten to the realistic blast form (`6k1/5n2/8/8/8/8/8/K4R2 w`,
   `Rxf7#`, child not in check), and the mate case to `7k/8/8/5N2/3K4/8/8/R6R w`,
   `Rag1#` (disambiguation *and* mate in one string).
3. **Disambiguation fixtures kept acquiring suffixes.** A queen/rook test
   position where every candidate checks the enemy commoner would have made the
   literal expectations `Q2c2+`-shaped, which is brittle. The tests use an
   `assert_san_body` helper that strips the suffix for the disambiguation
   cases, while the suffix itself is pinned separately (and differentially, at
   corpus scale).
4. **The `Setup` module cannot see `Board`'s private fields.** It builds through
   a crate-private `Board::from_setup_parts` constructor (bitboards + hash
   derivation, no FEN round-trip); `validation_errors` became `pub(crate)` so
   `from_setup_with` shares the FEN policy layer instead of re-implementing it.
5. **`SetupError` has no `FenLoadError` variant.** Adding a variant to a
   non-`#[non_exhaustive]` public enum would be a breaking change, so
   `from_setup_with` maps structural errors through `From<SetupError> for
   FenLoadError` onto the closest existing `FenError` variants and leaves
   semantic failures as `FenLoadError::InvalidPosition`.
6. **Two test expectations were simply wrong and were corrected by the code**
   (`Nd4` does resolve when only one knight can legally reach `d4`; a rook on
   `h1` cannot reach `d1` through the commoner on `e1`). Both cases are now
   explicit assertions of the correct behaviour.

## Deviations from the plan

- **`Setup::empty()` is not implemented.** The plan listed it with the note
  "alias for `new`; keep one name"; only `new()` (plus `Default`) exists.
- **`write_san` validates instead of assuming.** The plan allowed documenting
  that `write_san` "expects a legal move"; the implementation checks legality
  and returns `Err(fmt::Error)`, which is strictly safer and keeps
  `write_san == san` for every legal move (`write_san_matches_san`).
- **SAN tests live in `src/board.rs` and `src/setup.rs`** (module-level
  `#[cfg(test)] mod tests`, per `AGENTS.md`), with the corpus-scale checks in
  `tests/interchange.rs` and the deep walk in `examples/verify_san.rs` instead
  of a single slow test.

## Unresolved parts / missing tests

- **No PGN layer** (non-goal): no move numbers, comments, annotations (`!`,
  `?`), variations, or result tokens. SAN strings are single moves only.
- **No long algebraic notation parser** (non-goal); `e2e4` returns `None`.
- **`Setup` accepts any square order and any state combination** (by design),
  so it is not itself a validator; callers that need guarantees must use
  `from_setup_with`. There is no `Setup::from_board` convenience — the
  documented pattern is `for (sq, piece) in board.pieces() { setup.place(..) }`
  plus the five setters.
- **Multi-commoner boards** (artificial only) are accepted structurally by
  `Setup` and by SAN (the disambiguation logic is piece-type generic), but no
  test constructs a legal two-commoner SAN case; under `FenPolicy::Strict`
  they are rejected.
- **`san` is not benchmarked against a downstream consumer**; the measured
  ~0.1–0.2 µs per move is an upper bound that includes `outcome_after`'s
  mate/stalemate fallback. A future optimisation could cache the child "has a
  legal move?" answer, which plan 2 left as a documented fallback.
- **No differential against an external SAN implementation**
  (Fairy-Stockfish's PGN parser or a PGN corpus) — the oracle here is the
  round-trip plus the crate's own outcome/check rules. This is the same
  external-veto gap plan 3 recorded.
- **`tests/interchange.rs` takes ~3.9 s in a debug build** (507 k SAN
  round-trips). If CI time becomes an issue, cut the SAN walk to depth 1 or move
  it behind an ignored test; the release example covers depth 4.

## Next steps

1. **Plan 5 (terminal reasons and public contracts)** can now state that SAN's
   `#` is exactly `Outcome::Loss` and, once `TerminalReason` exists, optionally
   enrich the suffix decision (e.g. distinguishing extinction from mate without
   changing the emitted text).
2. Plan 6 (conformance corpus) can freeze SAN strings alongside FEN → moves,
   since the round-trip and suffix rules are now differentially pinned at corpus
   scale.
3. If a downstream consumer needs PGN, that is a separate additive plan: SAN is
   the only piece of PGN this crate should own.
4. If `Board::from_setup_with` gains more structural error kinds, revisit the
   `From<SetupError> for FenLoadError` mapping (or add a `#[non_exhaustive]`
   `FenLoadError::Setup` variant in a major release).
