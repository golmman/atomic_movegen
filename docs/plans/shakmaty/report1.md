# Report 1: Correct the one-commoner rule model

Implements `docs/plans/shakmaty/plan1.md`.

## Summary

The one-commoner misconception in `analysis.md` has been corrected, and the
uncommitted implementation changes have been reduced to the rules that are
actually reachable in atomic chess.

The key invariant is now stated consistently in code, tests, and docs:

* each side has at most one commoner;
* pawns cannot promote to commoner;
* a board has one or two commoners total;
* a board with one commoner is terminal;
* no move may destroy the mover's own commoner.

No public API signatures changed and no dependencies were added.

## Plan item status

| Plan item | Status | Result |
| --- | --- | --- |
| 1. Correct the analysis | ✅ Done | Correction notice added; multi-commoner evidence removed; reachable findings retained |
| 2. Correct public rule docs | ✅ Done | Updated crate docs, types, README, comparison, and board/movegen rustdoc |
| 3. Simplify implementation | ✅ Done | `compute_checkers`, `legal`, `outcome`, and terminal move generation now follow the one-commoner invariant |
| 4. Update tests | ✅ Done | Invalid multi-commoner tests removed; reachable regression tests added/updated |
| 5. Update changelog | ✅ Done | Unreleased section now describes the real fixes and documentation correction |

## Detailed changes

### `docs/plans/shakmaty/analysis.md`

* Added an explicit correction at the top explaining why the earlier
  multi-commoner conclusions were invalid.
* Removed the 1-vs-2, 2-vs-1, and multi-commoner self-destruction material.
* Rewrote the reachable correctness findings:
  * touching-commoner immunity in `checkers`;
  * loss vs. stalemate in `outcome`;
  * terminal-board move generation.
* Removed multi-commoner FENs from evidence tables and Appendix B.
* Updated testing and follow-up sections, including strict validation of “at
  most one commoner per side.”

### `src/board.rs`

* `compute_checkers`:
  * returns empty if either side has no commoner;
  * uses the side-to-move commoner directly;
  * returns empty when the commoners touch, since touching makes the commoner
    immune to every attack.
* `legal`:
  * returns `false` on a terminal board;
  * rejects any move that reduces the mover's commoner count;
  * skips the enemy-attack check when the enemy's only commoner is captured;
  * uses the post-blast surviving commoners for the final attack check.
* `outcome`:
  * no moves + attacked commoner → `Loss`;
  * no moves + safe commoner → `Draw`.
* `from_fen`, `commoners`, `outcome`, and `legal` rustdoc now state the
  one-commoner invariant and the permissive-parser caveat.
* Removed invalid multi-commoner tests; added terminal-`legal` and updated
  touching/castling tests.

### `src/movegen.rs`

* `generate_legal_with_state` and `has_legal_move_with_state` return no moves
  when either side has no commoner.
* Pseudo-legal/self-explosion wording now refers to “the commoner,” not a
  “last commoner.”
* Added a regression test for terminal boards.

### `src/types.rs`

* Documented that each side has at most one commoner and that promotion to
  commoner is impossible.
* Documented `PROMOTION_PIECES`.
* Added `test_commoner_is_not_a_promotion_piece`.

### Public docs

* `src/lib.rs`: replaced “pseudo-royal only for the last commoner” with the
  one-commoner rule, touching immunity, and terminal semantics.
* `README.md`: feature list now says “one commoner per side, no promotion to
  commoner.”
* `docs/shakmaty_comparison.md`: corrected the rule summary.
* `CHANGELOG.md`: replaced the old multi-commoner entries with the real fixes
  and documentation correction.

## Problems encountered and resolutions

### 1. The previous analysis modeled impossible positions

**Problem:** The earlier draft treated 1-vs-2 and 2-vs-1 commoner FENs as
meaningful oracle tests, then derived “spare commoner” and multi-commoner
self-destruction conclusions from them.

**Resolution:** Removed those positions from the analysis and tests. The
generalized FSF `extinctionPseudoRoyal` implementation is documented as serving
other variants/artificial FENs, not atomic play.

### 2. Old implementation still assumed multi-commoner semantics

**Problem:** `compute_checkers`, `legal`, and several tests contained branches
and fixtures for sides with multiple commoners.

**Resolution:** Simplified the code to the one-commoner invariant. The
strongest remaining self-destruction check rejects any move that reduces the
mover's commoner count, which also prevents artificial multi-commoner FENs from
silently enabling partial self-destruction.

### 3. `StateInfo` became unused in `legal`

**Problem:** After removing the pseudo-royal-count logic, the `state` argument
was no longer read, producing an unused-variable warning.

**Resolution:** `legal` now uses the populated commoner counts to reject all
moves on a terminal board. This keeps the public signature, removes the warning,
and makes direct `Board::legal` calls consistent with `generate_legal`.

### 4. Invalid tests contradicted the new rule model

**Problem:** The old `test_self_explosion_legal_with_surviving_commoner` test
asserted that destroying one of two own commoners was legal.

**Resolution:** Removed the test. The replacement `test_self_explosion_illegal`
uses a single commoner and asserts that destroying it is illegal. The analysis
states explicitly that no spare-commoner exception exists.

## Verification

| Check | Result |
| --- | --- |
| `cargo test` | ✅ 66 unit tests + 1 move-order + 4 perft + 1 verify_moves + 1 doc test pass |
| `cargo fmt --check` | ✅ Clean |
| `cargo clippy --all-targets -- -D warnings` | ✅ Clean |
| `cargo doc --no-deps` | ✅ No warnings |
| `cargo run --release --example verify_perft 6` | ✅ **41/41 positions pass at depths 1–6** (74.4 s total) |
| FSF oracle, touching commoners `8/8/8/8/8/8/4k3/4K3 w - - 0 1` | ✅ 4 moves, ours 4 |
| FSF oracle, touching commoners + rook `8/8/8/8/8/8/4k3/r3K3 w - - 0 1` | ✅ 4 moves, ours 4 |
| FSF oracle, castling through adjacent commoner `8/8/8/8/8/8/5k2/4K2R w K - 0 1` | ✅ 14 moves incl. `e1g1`, ours 14 |
| FSF oracle, terminal board `8/8/8/8/8/8/8/K7 w - - 0 1` | ✅ 0 moves, ours 0 |
| FSF oracle, terminal board `8/8/8/8/8/8/8/k7 w - - 0 1` | ✅ 0 moves, ours 0 |

The original differential runs against FSF (300 random reachable positions at
depth 2, 200 at depth 3, one commoner per side) were performed in the preceding
analysis session. This pass re-ran the complete perft suite and the selected
corner-case oracle checks above; the reachable move-generation behavior was not
changed by the simplification.

## Tools/examples used

* `examples/verify_perft` for the full 41-position depth-6 regression.
* `examples/perft` for quick corner-case move counts.
* Fairy-Stockfish's `go perft 1` for the selected corner-case FENs.
* `cargo test`, `cargo fmt`, `cargo clippy`, and `cargo doc`.

## Missing tests / intentionally not covered

* No test asserts behavior for artificial multi-commoner FENs. Those positions
  are outside the atomic rule set; `from_fen` still accepts them because it is
  a permissive parser.
* No full differential harness was rerun in this pass. The perft suite and
  selected oracle cases cover the changed legality paths.
* No material-draw tests were added; insufficient material remains a known gap
  in `Board::outcome`.

## Follow-up work

* Add `Board::validate()` / strict FEN loading that rejects more than one
  commoner per side and other impossible game-state constraints while keeping
  `from_fen` permissive.
* Add material draws to `Board::outcome` using FSF's conservative semantics.
* Add a generated random reachable-perft file and optionally rerun the full
  differential harness against FSF.
