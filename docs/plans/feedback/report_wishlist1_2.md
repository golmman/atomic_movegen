# Implementation report — wishlist 1 plan 2 (W2: Lichess-style FEN acceptance)

Status: **implemented.** `FenPolicy::Lichess` is in the tree through the
existing `Board::from_fen_with(fen, policy)` entry point. `Board::from_fen`,
the four existing policies, their error variants, and their error precedence
are unchanged. The parser is split into a syntax phase and the existing
structural checks, and the new policy sanitizes individual castling rights and
an impossible en-passant target before building the board.

No dependency was added. The oracle was run externally with shakmaty 0.30.2;
no shakmaty source, GPL code, or build-time network access is in the repository.

## Item status

| Plan item | Status | Where |
| --- | --- | --- |
| `FenPolicy::Lichess` variant | **done** | `src/board.rs` |
| Syntax-only `ParsedFen` / `parse_fen_syntax` | **done** | `src/board.rs` |
| `Board::from_fen` keeps its exact behavior | **done** | `src/board.rs` |
| Individual castling-right sanitization | **done** | `sanitize_lichess` |
| Impossible-ep sanitization | **done** | `sanitize_lichess`, `ep_geometry_is_consistent` |
| Lichess rejection predicate | **done** | `lichess_validation_errors` |
| Additive `ValidationErrors::IMPOSSIBLE_CHECK` | **done** | `src/board.rs` |
| `from_setup_with(..., Lichess)` validation half | **done** | `src/setup.rs`, dispatch in `validation_errors` |
| Docs (policy, `from_fen`, README, CHANGELOG, lib) | **done** | `src/board.rs`, `src/setup.rs`, `src/lib.rs`, `README.md`, `CHANGELOG.md` |
| Frozen accepted/rejected/syntax fixtures | **done** | `tests/fen_lichess.rs` |
| Frozen mutation differential | **done** | `tests/data/fen_lichess_mutations.tsv`, `tests/fen_lichess.rs` |
| Existing-policy compatibility test | **done** | `tests/fen_lichess.rs` |
| Unit tests for sanitization | **done** | `src/board.rs` |
| Report | **done** | this file |

## Policy contract as implemented

`Board::from_fen_with(fen, FenPolicy::Lichess)` follows this order:

1. **Syntax**, using the crate's existing grammar:
   - 4 or 6 whitespace-separated fields;
   - exactly 8 ranks, standard placement characters, no `'0'` empty count;
   - side `w` or `b`;
   - castling field `K`, `Q`, `k`, `q`, or `-` (a `-` ends the field, as in
     `Board::from_fen`);
   - an ep token that is `-` or a lowercase square name;
   - `u16` half-move and full-move numbers, full move at least 1.
   Failures are the existing `FenError` variants.
2. **Sanitize**, without touching placement, side, or clocks:
   - keep `WK` only with a white commoner on `e1` and a white rook on `h1`;
   - keep `WQ` only with a white commoner on `e1` and a white rook on `a1`;
   - keep `BK` only with a black commoner on `e8` and a black rook on `h8`;
   - keep `BQ` only with a black commoner on `e8` and a black rook on `a8`;
   - keep the ep target only on rank 6 (White to move) or rank 3 (Black to
     move), with an empty target, an enemy pawn on the victim square, and an
     empty double-push origin.
   Each right is discarded individually; valid rights are kept.
3. **Validate** the sanitized board with the atomic `Setup` rejection class:
   - `EMPTY_BOARD`;
   - a missing opponent commoner is rejected; a missing side-to-move commoner
     is accepted when the opponent still has one (the atomic terminal-board
     rule of shakmaty's `FromSetup for Atomic`); both missing is rejected;
   - more than one commoner per side is rejected;
   - `PAWNS_ON_BACKRANK`;
   - the side that just moved is in check (`OPPOSITE_CHECK`);
   - the atomic impossible-check class (`IMPOSSIBLE_CHECK`), which is only
     retained when a valid ep target makes the double-push history explicit;
   - the conservative material bound is **not** a rejection, and no piece is
     ever removed.

`Board::from_fen` still rejects an unbacked castling right and a wrong-rank or
victimless ep target as `FenError::InvalidCastling` / `FenError::InvalidEpSquare`
with the same messages and the same order (placement/side, castling backing,
ep geometry, clocks). The existing policies still delegate to that parser and
the unchanged `validation_errors` branches.

`Board::from_setup_with(..., FenPolicy::Lichess)` applies only the validation
half. `from_setup` already rejects unbacked castling rights. Its ep structural
check is the same weaker rank+victim check as `Board::from_fen`, so the Lichess
validation half additionally reports `INVALID_EP_SQUARE` when a setup retains
a target whose square or double-push origin is occupied. The FEN path never
sees that check because sanitization has already dropped the target.

## Oracle, version, and frozen data

The oracle is **shakmaty 0.30.2** with the `variant` feature (the version named
in `docs/shakmaty_comparison.md`; 0.30.2 is also the current crates.io
release). It was built in an external `/tmp` crate and never added to
`Cargo.toml` or `Cargo.lock`. The predicate used for the table is:

```text
Fen -> into_setup
    -> Atomic::from_setup(setup, CastlingMode::Standard)
    -> PositionError::ignore_invalid_castling_rights()
    -> PositionError::ignore_invalid_ep_square()
    -> PositionError::ignore_too_much_material()
```

This is the reconstruction of the wishlist's `parity` description: castling,
ep, and the material bound are recoverable; every other `PositionErrorKinds`
class is a rejection. The raw `kinds()` value and the recovered position's
`Fen::from_position(..., EnPassantMode::Always)` are what the frozen tables
record.

The oracle harness was a small Rust binary in `/tmp`; it was not committed.
The generated data is:

- `tests/fen_lichess.rs` — 24 accepted rows with the sanitized FEN, legal-mode
  FEN, both hashes, legal-ep square, stored state, and sorted legal UCI list;
  9 rejected rows with the exact `ValidationErrors` flags; 11 syntax rows.
- `tests/data/fen_lichess_mutations.tsv` — 1,074 generated mutation rows. Each
  row is `FEN<TAB>ACCEPT|REJECT|SYNTAX<TAB>sanitized FEN` (the third column is
  `-` for reject/syntax). The mutation set is derived from the 41 perft corpus
  positions plus 11 plan fixtures, de-duplicated, and covers castling-bit
  clearing/addition, home-piece removal/replacement, ep wrong-rank/wrong-victim/
  occupied-target/occupied-origin mutations, pawn/commoner add/remove, and
  both-sides-in-check fixtures.

The accepted fixture values were produced by the external oracle plus this
crate's post-sanitization board; they are committed, not regenerated at test
time.

## Disposition counts

Frozen fixture table:

| Class | Count |
| --- | --- |
| Accepted | 24 |
| — rows with at least one castling right dropped | 5 (7 individual rights) |
| — rows with an ep target dropped | 5 |
| Rejected | 9 |
| Syntax errors | 11 |

Mutation table (1,074 rows):

| Class | Count |
| --- | --- |
| Accepted | 483 |
| — rows with at least one castling right dropped | 248 (302 individual rights) |
| — rows with an ep target dropped | 9 |
| — rows whose oracle material bound was exceeded | 39 |
| Rejected | 591 |
| Syntax errors | 0 |

The 39 material-bound rows are accepted by `FenPolicy::Lichess`; the oracle's
`TOO_MUCH_MATERIAL` is the recoverable class, not a rejection. No pieces are
removed.

Extra differential run (not committed): 31,219 unique positions from the 41
perft positions plus two castling/ep-rich starts, 12 seeds × 80 plies. Oracle
and `FenPolicy::Lichess` agreed on every disposition and every sanitized FEN
(0 mismatches). The 1,074-row mutation run also compared sorted legal move
lists for accepted rows; 0 mismatches.

## Accepted-board equality

For every one of the 24 accepted frozen rows the test compares:

- `board.fen()` and `board.fen_with(EnPassantMode::Legal)`;
- `board.hash()` and `board.hash_with(EnPassantMode::Legal)`;
- `board.legal_ep_square()`, stored `ep_square()`, `castling_rights()`,
  `rule50()`, `game_ply()`, and `side_to_move()`;
- the sorted legal UCI list;
- the unsorted legal generation order against
  `Board::from_fen(expected_fen)`.

All pass. For accepted mutation rows the test also checks that the placement
part of the input FEN equals the sanitized FEN placement, which proves the
policy never truncates material.

The existing suites are green without expectation edits:

- `cargo test --test conformance`
- `cargo test --test interchange`
- all `src/board.rs` FEN/validation tests
- `cargo run --release --example verify_perft 5` (41/41 positions)

The full `cargo test` run is also green, including the new
`cargo test --test fen_lichess` (9 tests).

## Changed files

| File | Change |
| --- | --- |
| `src/board.rs` | `FenPolicy::Lichess`, `ValidationErrors::IMPOSSIBLE_CHECK`, `ParsedFen`, `parse_fen_prefix`, `parse_fen_clocks`, `parse_fen_syntax`, `ep_geometry_is_consistent`, `sanitize_lichess`, `atomic_validation_attackers`, `lichess_validation_errors`, `from_fen_with`/`validation_errors` wiring, docs, unit tests |
| `src/setup.rs` | `from_setup_with` docs for the validation-only Lichess half |
| `src/lib.rs` | crate-level FEN policy documentation |
| `README.md` | opt-in Lichess policy example and no-dependency oracle note |
| `CHANGELOG.md` | Unreleased entry for the policy, the flag, and the unchanged defaults |
| `tests/fen_lichess.rs` | frozen accepted/rejected/syntax tables, corpus equality, policy compatibility, mutation differential, setup-half tests |
| `tests/data/fen_lichess_mutations.tsv` | 1,074 frozen oracle mutation dispositions and sanitized FENs |
| `docs/plans/feedback/report_wishlist1_2.md` | this report |

## Problems and deviations from the plan's default table

1. **Missing side-to-move commoner is accepted.** The plan's default table
   says missing commoners are rejected. The oracle's atomic `FromSetup` removes
   `MISSING_KING` when the opponent still has a commoner, so a terminal board
   where the side to move has just exploded is accepted. The implementation
   follows the oracle and the plan's "table wins" rule. Tests cover both the
   accepted terminal cases and the rejected missing-opponent cases.
2. **`IMPOSSIBLE_CHECK` is conditional.** The plan allowed an additive flag if
   the oracle distinguishes the class. Atomic removes `IMPOSSIBLE_CHECK` when
   the recovered ep square is `None`, because an explosion can make otherwise
   impossible checker sets reachable. The flag is therefore raised only when a
   valid ep target remains, which is exactly the oracle behavior. The flag
   (`1 << 10`), its `Display` name, and its docs were added additively.
3. **Material bound is ignored, as the plan preferred.** The conservative
   `TOO_MUCH_MATERIAL` check is not part of the Lichess rejection predicate.
   Tests include 9 pawns and 7 queens with all 8 pawns; neither removes pieces.
4. **The syntax grammar is not widened to shakmaty's full parser.** Shakmaty
   accepts fewer than 4 fields, fullmove `0`, Chess960 castling file letters,
   and `u32` clocks; the crate keeps its existing 4/6-field grammar, `KQkq`
   letters, fullmove ≥ 1, and `u16` clocks. These are syntax classes, not
   accept/drop/reject parity. The plan explicitly requires fullmove zero and
   the other malformed rows to stay syntax errors. The syntax fixture table
   pins the crate's answers, and these rows are excluded from the oracle
   comparison.
5. **`from_setup_with(Lichess)` rejects the remaining invalid-ep cases.**
   `from_setup` only checks the ep rank and the victim pawn, unlike the full
   geometry used by sanitization. The Lichess validation half now reports
   `INVALID_EP_SQUARE` for an occupied target or origin; otherwise a setup
   could silently retain a target the FEN path would drop. The FEN path is
   unaffected because sanitization has already removed such targets.
6. **Error precedence was preserved by splitting prefix and clocks.** The plan
   sketched `parse_fen_syntax` followed by structural checks. Doing that
   literally would report a clock error before a castling-backing or ep error,
   changing `Board::from_fen`. `from_fen` therefore uses `parse_fen_prefix`
   (no clocks), runs the structural checks, then `parse_fen_clocks`.
   `parse_fen_syntax` is the same prefix plus clocks for the Lichess path.

## Unresolved rows and missing tests

- **No downstream `parity` binary was available.** The oracle predicate is the
  reconstruction above from the wishlist text and shakmaty's documented
  recovery methods. If the downstream predicate ignores a different subset
  (for example, does not call `ignore_too_much_material`, or uses strict
  `Fen::into_position` without recovery), the tables must be regenerated. Any
  difference is a finding, not silently absorbed.
- **Mutation generator is external.** The Python mutation generator and the
  oracle harness were run in `/tmp` and are not committed; only the FEN →
  disposition/sanitized-FEN data is committed. Regenerating the table requires
  the same generator and shakmaty 0.30.2.
- **No Chess960 castling syntax.** That is an explicit non-goal.
- **No `Setup` castling-sanitization test.** A `Setup` cannot express an
  unbacked castling right because `from_setup` rejects it first; the setup
  tests cover the validation half, not parser parity.
- **No per-policy mutation differential.** Existing-policy behavior is pinned
  by the small shared fixture list in `existing_policies_keep_their_answers`,
  not by the 1,074 mutation rows.

## Verification commands run

```sh
cargo test
cargo test --test fen_lichess
cargo test --test interchange
cargo test --test conformance
cargo clippy --all-targets
cargo fmt --check
cargo doc --no-deps
cargo run --release --example verify_perft 5
```

All passed. The release perft run reported 41/41 positions at depth 5. No
perft or movegen hot path was changed: `from_setup_parts` and `recompute_hash`
are untouched. `Board::from_fen` now calls the existing constructor instead of
duplicating its bitboard build; that is a FEN/setup-time cost, so no perft
timing guard was needed.

## Next steps

- The downstream can now choose `FenPolicy::Lichess` for the `parity` path, or
  keep shakmaty for parsing; W2 does not force the parser into the critical
  path.
- If the downstream owns a `parity` binary, run the same corpus/mutation data
  through it and replace the frozen table with that output. Treat any
  disposition difference as a bug report against this policy.
- No `Board::from_fen_lichess` alias was added; the policy-only shape is the
  documented API.
