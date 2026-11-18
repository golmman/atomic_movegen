# Plan 2 — Lichess-style FEN acceptance (W2)

## Scope

One opt-in FEN policy and the parser split it needs:

- **W2:** `FenPolicy::Lichess`, used through the existing
  `Board::from_fen_with(fen, policy)`, mirroring the acceptance predicate the
  downstream calls `parity` (shakmaty's `Setup`-based atomic position
  construction):
  - invalid castling rights are **dropped**;
  - an impossible en-passant target is **dropped**;
  - the conservative "too much material" bound is **not** a rejection (unless
    the frozen oracle table says the selected shakmaty predicate rejects it);
  - the fixed structural class (empty board, missing/extra commoners, pawns on
    the back ranks, opposite/impossible check, syntax errors) is **rejected**.
- **Internal:** split the current FEN parser into a syntax-only part and the
  structural validation that `Board::from_fen` already performs, so the new
  policy can sanitize before building the board. `Board::from_fen` and every
  existing policy keep byte-for-byte behavior.

Requirements served: wishlist 1 W2 (P2). The wishlist also permits the
downstream to keep shakmaty for parsing; this plan is therefore independent and
does not block plans 1 or 3.

## Current-tree assessment

- `src/board.rs::Board::from_fen` parses 4 or 6 fields and currently rejects,
  with `FenError`:
  - castling rights not backed by a commoner on `e1`/`e8` and the
    corresponding rook on `h1`/`a1`/`h8`/`a8` (`InvalidCastling`);
  - an en-passant target not on rank 6/3 for the side to move, with no enemy
    pawn behind it, a non-empty victim square, or a non-empty double-push
    origin (`InvalidEpSquare`);
  - placement, side-to-move, field-count, and integer syntax errors.
- `Board::from_fen_with(fen, policy)` first calls `from_fen`, then
  `validation_errors(policy)`. `FenPolicy::Strict` checks exactly one commoner
  per side, pawns off the back ranks, castling backing (again), a conservative
  material bound, ep geometry (again), and that the side that just moved is not
  in check. `Permissive` returns no semantic errors. There is no policy that
  *drops* a bad castling right or ep target.
- `Board::validation_errors` is a small bitflags vocabulary:
  `EMPTY_BOARD`, `MISSING_COMMONER_WHITE/BLACK`,
  `TOO_MANY_COMMONERS_WHITE/BLACK`, `PAWNS_ON_BACKRANK`, `OPPOSITE_CHECK`,
  `INVALID_EP_SQUARE`, `INVALID_CASTLING_RIGHTS`, `TOO_MUCH_MATERIAL`.
- `Board::from_setup` and `Board::from_setup_with` reuse the same structural
  and policy vocabulary. A `Setup` cannot express bad castling backing or an
  impossible ep square: `from_setup` rejects them with `SetupError`. Therefore
  only the FEN entry point can perform the "drop" half of W2.
- The downstream's `parity` accepts FENs the way shakmaty's `Setup` does: bad
  castling rights and an impossible ep target are dropped, and a fixed class of
  illegal positions is rejected. `atomic-movegen`'s permissive parser is not
  that predicate, and `FenPolicy::Strict` is not either.

## Parity contract (to be frozen before implementation)

The exact contract is data, not prose. Before changing code, produce a frozen
disposition table from the selected shakmaty version used by the downstream's
`parity` (or from the downstream's own `parity` binary). Do not add shakmaty as
a dependency or dev-dependency: handoff 1 R9 forbids GPL interop, including
test-only dependencies. Run the oracle externally, record its version in the
report, and commit only the FEN → disposition data.

The table must answer these questions for each row:

| Class | Expected W2 behavior |
| --- | --- |
| Valid start position, quiet middlegame, all perft corpus positions | accept unchanged |
| Castling right without the required commoner or rook on the home square | drop that right, keep the other valid rights, accept |
| Castling right with the right pieces but the wrong side's rook | drop that right |
| Castling field containing an unknown character | syntax error (same class as `from_fen`), not a drop |
| Ep target on the wrong rank, no enemy pawn behind it, victim square occupied, or double-push origin occupied | drop the ep target, accept |
| Ep target token malformed (not a square name) | syntax error |
| Excess material (for example 9 pawns, or a count above the conservative start-plus-promotions bound) | accept unless the oracle table says the selected predicate rejects it; never silently delete pieces |
| Empty board | reject |
| Missing or more than one commoner per side | reject |
| Pawn on rank 1 or rank 8 | reject |
| Side that just moved is in check | reject |
| Impossible check (both sides in check, a geometrically impossible checker set, etc.) | reject if the oracle rejects it; otherwise accept |
| Fewer than 4 fields, more than 6, wrong rank count, invalid placement, invalid side, invalid clocks | syntax error |

The plan's default mapping is the table above. If the oracle table contradicts a
row, the table wins and the implementation documents the difference. A silent
widening of either the accepted set or the dropped set is a finding; it must not
be absorbed into "close enough".

## Decisions and pushback

1. **One new policy, not a second constructor.** `FenPolicy::Lichess` keeps the
   existing `from_fen_with` entry point and the existing
   `FenLoadError`/`ValidationErrors` vocabulary. The policy is named for the
   downstream's parity predicate; its docs state that it mirrors shakmaty's
   `Setup` acceptance, not every Lichess-site behavior. The enum is
   `#[non_exhaustive]`, so the variant is an additive minor change.

2. **`Board::from_fen` does not change.** It must keep rejecting invalid
   castling rights and an impossible ep target exactly as today, with the same
   `FenError` variants and the same error precedence. The Lichess policy is the
   only entry point that sanitizes.

3. **Parse syntax once, then choose the policy path.** Extract a crate-private
   `parse_fen_syntax` (or `ParsedFen`) that:
   - validates field count, rank count, placement, side, castling characters,
     ep token syntax, and clocks;
   - returns raw castling rights and a raw optional ep square *without* checking
     that they are backed by pieces or a double push;
   - builds no `Board` and computes no hash.

   `Board::from_fen` then calls `parse_fen_syntax` followed by the existing
   structural checks and `Board::from_setup_parts`; this keeps its behavior and
   lets the Lichess path run `sanitize_lichess` on the raw parts before
   building.

4. **Sanitize individual rights, not the whole field.** A FEN `KQkq` with only
   the white king-side rook and commoner on their home squares becomes `K`, not
   `-`. A right is kept only when the standard home-square predicate holds:
   - `WK` → white commoner on `e1`, white rook on `h1`;
   - `WQ` → white commoner on `e1`, white rook on `a1`;
   - `BK` → black commoner on `e8`, black rook on `h8`;
   - `BQ` → black commoner on `e8`, black rook on `a8`.

5. **Drop the ep target unless it is a geometrically possible double-push
   target.** The existing structural predicate is the right one: target on rank
   6 (White to move) or rank 3 (Black to move), target empty, enemy pawn on the
   victim square behind it, and the double-push origin empty. Dropping is not a
   rejection and does not log an error.

6. **Do not truncate material.** "Excess material are dropped" in the wishlist
   means the material *bound* is not part of the rejection predicate if that is
   what parity does; it cannot mean that pieces are removed from the board.
   Removing pieces would be a rules change, not FEN acceptance. The plan pins
   whichever behavior the oracle table shows, but the implementation must never
   silently alter placement to satisfy a material count.

7. **Rejection is still structural.** After sanitization, run the Lichess
   validation predicate. It should reuse `ValidationErrors`; if the oracle
   distinguishes an impossible-check reason that `OPPOSITE_CHECK` cannot
   express, add an additive `ValidationErrors::IMPOSSIBLE_CHECK` flag (and its
   `Display` name) rather than mislabeling the rejection. If the oracle only
   reports accept/reject, keep the existing flags and document the mapping.

8. **`Board::from_setup_with(..., FenPolicy::Lichess)` applies only the
   validation half.** `from_setup` already rejects unbacked castling rights and
   impossible ep targets structurally, so there is nothing to drop. Document
   that `Setup` cannot express the sanitization half; use `from_fen_with` for
   parser parity.

9. **The oracle is external and optional at build time.** No shakmaty
   dependency, no generated build script, no build-time network. The frozen
   table lives in the test suite as plain data. The report names the oracle
   version and how the table was produced.

## Proposed API

```rust
// src/board.rs

/// Policy for semantic validation when loading a FEN with
/// `Board::from_fen_with`.
#[non_exhaustive]
pub enum FenPolicy {
    Strict,
    AllowTerminal,
    AllowImpossibleCheck,
    Permissive,
    /// Mirror the shakmaty/`Setup` acceptance used by the downstream's
    /// `parity` predicate.
    ///
    /// Unlike the other policies, this policy both **drops** and **rejects**:
    ///
    /// - a castling right not backed by the required commoner and rook on its
    ///   home square is dropped, keeping every other valid right;
    /// - an en-passant target not consistent with a double push is dropped;
    /// - the conservative material bound is not a rejection (unless the frozen
    ///   parity table says otherwise);
    /// - an empty board, a missing or extra commoner, a pawn on the back
    ///   ranks, the side that just moved being in check, and the parity
    ///   predicate's impossible-check class are rejected.
    ///
    /// Syntax errors are still reported as [`FenError`]. Use
    /// [`Board::from_fen`] for the permissive parser.
    Lichess,
}
```

No new public constructor is required; the documented call is:

```rust
let board = Board::from_fen_with(fen, FenPolicy::Lichess)?;
```

If the implementation finds `from_fen_with`'s two error channels inconvenient
for the drop path, it may add `Board::from_fen_lichess(fen)` as a thin alias;
that is an implementation choice, not a second acceptance predicate. Prefer the
policy-only shape unless a concrete consumer need appears.

Crate-private additions in `src/board.rs`:

```rust
/// The syntax-only parse result: placement plus the raw state fields.
struct ParsedFen {
    squares: [Piece; 64],
    side_to_move: Color,
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u16,
    game_ply: u16,
}

/// Parse FEN syntax without checking castling backing or double-push geometry.
fn parse_fen_syntax(fen: &str) -> Result<ParsedFen, FenError>;

/// Drop unbacked castling rights and impossible ep targets.
fn sanitize_lichess(parsed: &mut ParsedFen);

/// The Lichess/post-sanitization rejection predicate.
fn lichess_validation_errors(board: &Board) -> ValidationErrors;
```

## Implementation

1. **Extract `parse_fen_syntax`.**
   - Move the current placement, side, castling-character, ep-token, and clock
     parsing into the new function. Keep the exact `FenError` variants and
     messages, including the current rejection of `'0'` and non-standard
     commoner labels (`C`/`c`).
   - Do **not** perform the current castling backing checks or ep geometry
     checks inside `parse_fen_syntax`; return the raw bits/square.
   - Keep the current 4-or-6 field rule unless the frozen parity table shows
     that the oracle requires six fields. If it does, the Lichess policy
     rejects 4-field FENs while `from_fen` keeps accepting them; document that
     difference explicitly.

2. **Keep `from_fen` behavior identical.**
   - `Board::from_fen` calls `parse_fen_syntax`, then the existing
     `validate_castling_parts` and `validate_ep_parts` (extracted as private
     helpers if useful), then `Board::from_setup_parts`.
   - Error precedence stays: placement/side/field-count errors, then castling
     backing, then ep geometry, then clocks as today. Existing `board.rs`
     tests for FEN errors must pass without edits.

3. **Implement `sanitize_lichess`.**
   - Apply the four home-square predicates from decision 4 and clear the
     unbacked bits.
   - Apply the double-push predicate from decision 5 and set `ep_square` to
     `None` when it fails.
   - Do not touch placement, side, clocks, or game ply.
   - No allocation; `ParsedFen` is plain data.

4. **Implement the Lichess rejection predicate.**
   - Build the sanitized `Board` with `Board::from_setup_parts`, then run:
     - `EMPTY_BOARD`;
     - missing/extra commoners (same strict counts; `AllowTerminal` does not
       apply here);
     - `PAWNS_ON_BACKRANK`;
     - the side that just moved being in check (`OPPOSITE_CHECK`);
     - the oracle's impossible-check class. Add
       `ValidationErrors::IMPOSSIBLE_CHECK` only if the frozen table requires a
       distinct flag; otherwise document that `OPPOSITE_CHECK` represents both
       in this vocabulary.
   - Do **not** run `INVALID_CASTLING_RIGHTS`, `INVALID_EP_SQUARE`, or
     `TOO_MUCH_MATERIAL` after sanitization unless the table says a
     post-sanitization case still rejects. Sanitization is expected to make the
     first two impossible; the material bound is policy-dependent.

5. **Wire `from_fen_with`.**
   - For `FenPolicy::Lichess`: `parse_fen_syntax` → `sanitize_lichess` →
     `from_setup_parts` → `lichess_validation_errors`.
   - For every existing policy: keep the current `from_fen` → `validation_errors`
     path untouched.
   - `validation_errors(policy)` gets a `FenPolicy::Lichess` arm that calls the
     Lichess predicate; this keeps `from_setup_with(..., Lichess)` coherent
     (validation only, no sanitization).
   - `Permissive` stays a no-op. `Strict`, `AllowTerminal`, and
     `AllowImpossibleCheck` keep their current branches.

6. **Docs.**
   - `Board::from_fen_with` docs list the new policy and state that it is the
     only policy that drops data.
   - `FenPolicy` docs state the exact accept/drop/reject contract, including
     that "Lichess" names the parity predicate and not every behavior of the
     Lichess site.
   - `Board::from_fen` docs add one sentence: for the sanitizing predicate use
     `from_fen_with(..., FenPolicy::Lichess)`; the default parser is unchanged.
   - `README.md` and `CHANGELOG.md` add the opt-in policy and the no-dependency
     oracle note.

7. **Frozen fixture table.** Add `tests/fen_lichess.rs` with a Rust table of
   `(name, fen, disposition, expected_fen, expected_hash_always,
   expected_legal_ep, expected_legal_moves)` rows. The expected values are
   produced by the external oracle and this crate's post-sanitization board;
   the table is committed, not regenerated at test time. If a row's expected
   legal move list is long, compare a sorted list and record the count in the
   row comment.

## Tests

### Disposition table

`tests/fen_lichess.rs` must cover every row type:

- **Accept unchanged:** start position; every perft corpus FEN; a quiet
  middlegame; a position with a legal ep capture; a position with valid
  castling for both sides.
- **Drop castling:** each of the four rights removed individually; all four
  invalid; a mix where only one is valid; a valid right with an enemy rook on
  the home square (must drop).
- **Drop ep:** wrong rank; no enemy pawn behind; victim square occupied;
  double-push origin occupied; ep target on an otherwise valid board.
- **Accept excess material:** at least one FEN with 9 pawns and one with an
  extra queen/rook beyond the conservative bound, if the oracle accepts them;
  if the oracle rejects them, move the row to the reject class and record the
  exact flag.
- **Reject:** empty board; missing white commoner; missing black commoner; two
  white commoners; two black commoners; pawn on rank 1; pawn on rank 8;
  opposite check; impossible check if the table separates it.
- **Syntax errors:** too few fields; too many fields; 7 ranks; 9 ranks;
  invalid piece character; invalid side; invalid castling character; malformed
  ep token; non-integer halfmove/fullmove; fullmove zero.
- **Default compatibility:** every malformed FEN row also asserts that
  `Board::from_fen` returns the same `FenError` variant as before. The Lichess
  policy must not change the default parser's answer.

### Accepted-board equality

For every accepted row:

- compare `board.fen()` with `expected_fen` (which includes the sanitized
  castling field and `-` ep);
- compare `board.hash()` and `board.hash_with(EnPassantMode::Legal)` with the
  frozen values;
- compare `board.legal_ep_square()` with `expected_legal_ep`;
- compare the sorted legal UCI list with `expected_legal_moves` and assert the
  pseudo/legal generation order on a sample of rows;
- assert `board.side_to_move()`, `board.castling_rights()`,
  `board.ep_square()`, `board.rule50()`, and `board.game_ply()` are the
  sanitized/expected values.

### Mutations

Generate mutations of the corpus:
- clear each castling bit in turn;
- move/set each castling rook or commoner away and re-add it;
- replace each ep target with a wrong-rank, wrong-victim, and occupied-target
  square;
- add/remove one pawn or commoner;
- set both sides in check (where constructible).

For each mutation, compare the W2 accept/drop/reject verdict with the frozen
oracle verdict. Record the mutation count in the report. A mutation the oracle
does not cover is excluded from the comparison and listed as a missing test,
not silently accepted.

### Existing behavior

- `cargo test --test conformance` and the existing `board.rs` FEN tests stay
  green without edits.
- `cargo test --test interchange` stays green: FEN round-trips of accepted
  boards still reproduce the same board/key.
- `Board::from_fen_with` with `Strict`, `AllowTerminal`,
  `AllowImpossibleCheck`, and `Permissive` must return exactly what it returned
  before. Add a small test comparing each policy against the previous expected
  result on a shared fixture list.

## Verification

All global gates of the index, plus:

```sh
cargo test --test fen_lichess
cargo test --test interchange
cargo test --test conformance
cargo run --release --example verify_perft 5
```

There is no perft or movegen change expected. If the parser refactor touches
`Board::from_setup_parts` or `recompute_hash`, run the perft timing guard
anyway; a >2 % change is a blocker.

Acceptance:

- every frozen row and every covered mutation has the same accept/drop/reject
  answer as the oracle;
- every accepted position has the same board (sanitized FEN), hash, legal-ep
  key, and legal move list;
- `from_fen` and all existing policies are byte-for-byte unchanged;
- no new dependency, no GPL code/data, no build-time network.

## Files to touch

- `src/board.rs` — `FenPolicy::Lichess`, `parse_fen_syntax`, `sanitize_lichess`,
  `lichess_validation_errors`, `from_fen_with` wiring, docs, unit tests.
- `src/setup.rs` — docs only for `from_setup_with` + `Lichess` (validation
  half), unless a shared private helper needs visibility.
- `tests/fen_lichess.rs` — new frozen table, mutation differential, accepted
  board equality.
- `tests/interchange.rs` — no expectation changes; add a round-trip row only if
  the table needs it.
- `README.md`, `CHANGELOG.md`, `src/lib.rs` — policy documentation.
- `docs/plans/feedback/report_wishlist1_2.md` — final report.

## Non-goals

- No change to `Board::from_fen`, its `FenError` variants, or its error
  precedence.
- No new FEN feature beyond the opt-in policy: no Chess960 castling rights, no
  variant pockets, no SAN, no JSON.
- No component of shakmaty or any GPL crate, at build time or test time.
- No material truncation, no silent piece removal.
- No change to `FenPolicy::Strict`/`AllowTerminal`/`AllowImpossibleCheck`/
  `Permissive` answers.
- No promise that W2 enters the downstream's critical path; the wishlist's
  "keep the parser outside" option remains valid.

## Final task

Write `docs/plans/feedback/report_wishlist1_2.md` with item status, the exact
policy contract as implemented, the oracle name/version and how the frozen
table was produced, the disposition counts (accepted, dropped rights, dropped
ep targets, rejected, syntax errors), the mutation counts, the accepted-board
key/move equality results, changed files, problems, deviations with the oracle
evidence that motivated them, unresolved rows, missing tests, and next steps
(whether the downstream adopts `FenPolicy::Lichess` or keeps shakmaty for
parsing).
