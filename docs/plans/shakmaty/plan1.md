# Plan 1: Correct the one-commoner rule model

## Background

`docs/plans/shakmaty/analysis.md` was written from a comparison with
`shakmaty`. The first version of that analysis contained a fundamental rule
error: it treated positions with several commoners per side as reachable and
derived correctness conclusions, tests, and differential results from them.

That is not atomic chess. The actual invariant is:

* Each side has exactly **one** commoner.
* Pawns promote only to knight, bishop, rook, or queen; promotion to commoner
  is impossible.
* A board therefore has one or two commoners in total.
* A board with only one commoner is terminal.
* A move may not destroy any of the mover's own commoners. There is no
  “spare commoner” exception.

The analysis therefore needs a correction, and the uncommitted implementation
changes from the previous session need to be reduced to the rules that are
actually reachable.

## Goals

1. Make the analysis consistent with the one-commoner invariant.
2. Remove multi-commoner assumptions from public documentation.
3. Simplify legality and checker computation to the real rule set.
4. Remove invalid regression tests and add tests for the real invariant.
5. Keep the public API unchanged and add no dependencies.
6. Re-run the full correctness gates.

## Non-goals

* Adding `Board::validate()` or making `from_fen` reject impossible positions.
  The parser remains permissive by design; strict validation is a follow-up.
* Material/insufficient-material draws.
* Changing FEN, hashing, move generation order, or the public signatures.
* Matching Fairy-Stockfish on artificial multi-commoner FENs. Those are not
  atomic positions.

## Implementation steps

### Step 1 — Correct the analysis

Edit `docs/plans/shakmaty/analysis.md`:

* Add a correction notice explaining the one-commoner invariant.
* Remove the earlier claims that multi-commoner adjacency and
  self-destruction were reachable move-generation divergences.
* Keep the real, reachable findings:
  * `Board::checkers` disagreed with legal move generation when the two
    commoners touch.
  * `Board::outcome` treated a lost no-move position as a draw.
  * Terminal one-commoner boards still generated moves.
* Remove all 1-vs-2 and 2-vs-1 FENs from evidence tables and appendices.
* Remove differential results based on random 1–3-commoner positions.
* Update follow-ups so strict validation explicitly includes “at most one
  commoner per side.”

### Step 2 — Correct public rule documentation

Update the rule descriptions in:

* `src/lib.rs`
* `src/types.rs`
* `README.md`
* `docs/shakmaty_comparison.md`
* `Board::from_fen`, `Board::commoners`, `Board::outcome`, and `Board::legal`
  rustdoc in `src/board.rs`
* `src/movegen.rs` pseudo-legal/self-explosion wording

The documentation must not describe pseudo-royalty as depending on a “last”
or “spare” commoner. It should state the one-commoner invariant directly.

### Step 3 — Simplify implementation

In `src/board.rs`:

* `compute_checkers`:
  * return empty if either side has no commoner (terminal);
  * use the side-to-move commoner directly;
  * return empty when the two commoners touch, because touching makes the
    commoner immune to every attack.
* `legal`:
  * return `false` for every move on a terminal board;
  * keep the existing blast-occupancy calculation;
  * reject any move that reduces the mover's commoner count;
  * skip the enemy-attack check if the move captures the enemy's only
    commoner;
  * keep touching-commoner immunity for castling pass-through squares and the
    final square.
* `outcome`:
  * no legal moves + commoner attacked → loss;
  * no legal moves + commoner safe → draw.

In `src/movegen.rs`:

* `generate_legal_with_state` and `has_legal_move_with_state` return no moves
  when either commoner count is zero.

Do not change public function signatures. `StateInfo` fields stay as counts so
terminal detection and existing callers remain compatible.

### Step 4 — Update tests

Remove tests that rely on impossible multi-commoner FENs, including the old
“surviving spare commoner” self-explosion test.

Keep/add reachable regression tests:

* touching commoners are immune and not in check;
* touching commoners are immune to a rook;
* castling through a square adjacent to the enemy commoner is legal;
* self-explosion of the only commoner is illegal;
* terminal boards have no legal moves (`generate_legal`, `has_legal_move`, and
  direct `Board::legal`);
* no moves + attacked commoner is a loss;
* no moves + safe commoner is a draw;
* `PROMOTION_PIECES` does not contain `Commoner`.

### Step 5 — Update the changelog

Replace the multi-commoner entries in the unreleased section with entries that
describe the real fixes and the documentation correction.

## Acceptance criteria

* No reachable-rule documentation claims that extra commoners can exist.
* No regression test uses a FEN with more than one `K` or more than one `k`.
* `Board::checkers` is empty when the commoners touch.
* `Board::legal` rejects any move destroying an own commoner.
* Terminal boards produce no legal moves.
* `Board::outcome` distinguishes loss from stalemate correctly.
* `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt
  --check`, and `cargo doc` pass.
* `verify_perft` passes all 41 reference positions at depths 1–6.

## Report

After implementation, write `report1.md` with:

* item-by-item status;
* changed files;
* problems encountered and how they were resolved;
* verification commands and results;
* intentionally missing tests/unresolved items;
* follow-up work.
