# Plan 3 — immediate-explosion query on a child (W3)

## Scope

One read-only per-move query on `EffectContext`:

- **W3:** `EffectContext::exploding_capture_after(board, m) -> Option<Move>`:
  after the pseudo-legal move `m`, return a legal capture by the child's side to
  move whose blast removes the commoner of the side that played `m`.
- **Internal:** a crate-private view-side search over the already-derived child
  view, built from `movegen::for_each_pseudo_legal_in_view`,
  `PositionView::blast_casualties`, and `position::legal_with_counts`.
- **Measurement:** the new query is benchmarked against an exhaustive search
  over the built child's legal moves. No guide integration or hard cost bar is
  claimed; the wishlist asks for the fact and its cost before any swap.

Requirements served: wishlist 1 W3 (P2/P3).

## Current-tree assessment

- `src/effects.rs`:
  - `EffectContext::analyze` derives a child `PostMove` from the parent view.
  - `EffectContext::post_move` exposes the child placement, casualties, side to
    move, ep target, rights and clock without a board write.
  - `EffectContext::effect`/`classification` answer the move's own rules fields;
    `terminal_after`/`child_outcome` answer the child's state.
  - There is no query for a *one-ply threat in the child*: "can the other side
    immediately capture a piece beside the mover's commoner and explode it?"
- `src/position.rs`:
  - `PositionView` exposes `commoners`, `pieces_color`, `is_capture`,
    `blast_casualties`, `checkers`, and the placement queries.
  - `legal_with_counts(view, m)` is the exact legality body for a view, with
    the game-over guard supplied by the caller. It needs no `StateInfo`.
- `src/movegen.rs`:
  - `for_each_pseudo_legal_in_view(view, f)` walks the child's pseudo-legal
    candidates without a board write and supports an early `Break`.
  - `generate_legal_in_view(view, state, moves)` and
    `has_legal_move_in_view` exist, but the plan only needs the pseudo-legal
    walk plus `legal_with_counts`; this avoids populating a `StateInfo`.
- The downstream's `Mover::exposes`, `exploding_capture` and `threats` already
  answer this question in its own code. Until the crate answers it, the adapter
  needs a second rules implementation for the same one-ply fact.

## Semantics

Let `us` be the side that played `m` and `them = us.flip()` the child's side to
move.

`exploding_capture_after(board, m)` returns a move `c` such that:

1. `c` is a legal move for `them` in the child position;
2. `c` is a capture (`PositionView::is_capture` is true; en passant included);
3. the blast of `c` removes at least one of `us`'s commoners in the child.

The query is about the **mover's** commoner, i.e. the commoner exposed by `m`.
The wishlist's proposal line says "the immediate capture that removes the child
side-to-move's commoner"; read together with the motivation ("after this move,
can the other side capture a piece beside the mover's commoner and explode it at
once?") the intended target is the commoner of the side that played `m`. The
plan pins that interpretation in the public docs and in the differential
fixtures. If the downstream intended the opposite target, its own differential
will expose the mismatch before integration; the upstream definition must not
be left ambiguous.

Naming/return rules:

- The result is the **first** such capture in the child's pseudo-legal
  generation order that passes legality. Existence is `is_some()`. The plan does
  not promise a "best" capture or a unique one.
- `None` when:
  - `m` is malformed (`Move::NONE`, no origin piece, or an origin that does not
    hold a piece of the parent side to move);
  - the child has no commoner for `us` (for example `m` self-exploded or
    already removed it);
  - the child has no commoner for `them` (the child is already decided by
    extinction; there are no legal moves);
  - no legal capture satisfies conditions 1–3.
- The query is defined for pseudo-legal `m`; for an illegal `m` it describes
  the unreachable child exactly as `post_move` does. It does not require `m` to
  be legal, and it does not return an error for an illegal child.
- Touching commoners do not suppress the query by themselves. The child-side
  capture must still be legal: a capture that destroys `them`'s own commoner is
  illegal and must be rejected by `legal_with_counts`; a capture that destroys
  only `us`'s commoner and leaves `them`'s commoner alive is a win and may be
  returned. This is the same branch order as `Board::legal`.

## Decisions and pushback

1. **The query is a child query, not a `MoveClassification` flag.**
   `MoveClassification` describes the move that was just played. The useful
   question is about the *next* player's captures. Adding a boolean to the
   classification would force every consumer of `effect`/`classification` to
   pay for a child scan it did not ask for. The separate query keeps the hot
   path unchanged.

2. **Use the already-derived child view.** `analyze`/`child_view` already
   produced the placement, side to move, ep target, and casualties. The query
   starts from `self.analyze(board, m)?` and never clones `Board`, never calls
   `do_move`, and never calls `populate_state`.

3. **Exactness first, specialization later.** The first implementation walks
   the child's pseudo-legal stream and filters captures. This is exact by
   construction and shares the generator's move encodings (including en passant
   and promotions). If the measured cost is too high, a later crate-private
   optimization may add a capture-only / destination-mask walk; it must remain
   behind the same differential. Do not add a second rules implementation now.

4. **Destination prefilter is exact.** To remove a commoner by blast, a capture
   must land on the commoner's square or on one of its eight neighbours (the
   blast is the 3×3 zone around the destination; pawns are immune but a
   commoner is not a pawn). The plan uses
   `target_zone = target | attacks::king_attacks(target_sq)` as a cheap
   prefilter before `blast_casualties`, but it still confirms the removal with
   `blast_casualties(c).all() & target != EMPTY` so the prefilter cannot become
   the rule.

5. **Legality from the child view.** `position::legal_with_counts(&child.view, c)`
   is the exact `Board::legal` body for the child. The query must pre-check that
   both commoners exist in the child before calling it, because the board
   method's game-over guard is not in `legal_with_counts` and a commoner-less
   side would otherwise reach an `lsb` on an empty bitboard. The pre-check also
   makes the documented `None` cases total.

6. **No board write.** The child view is a value; `for_each_pseudo_legal_in_view`
   and `legal_with_counts` only read it. The test suite asserts the parent
   board's FEN and hash are unchanged after every query.

7. **No new public type.** The method lives on the existing `EffectContext`, as
   the wishlist proposes. Do not expose a `ChildAnalysis` type or a public
   `PositionView`.

## Proposed API

```rust
// src/effects.rs

impl EffectContext {
    /// The first legal capture in the child after `m` whose blast removes the
    /// commoner of the side that played `m`.
    ///
    /// The returned move belongs to the child's side to move. It is the first
    /// such capture in the child's pseudo-legal generation order; the query is
    /// an existence/one-witness query, not a "best capture" query.
    ///
    /// `None` when `m` is malformed, when the child lacks the target commoner
    /// or the child side's commoner (a decided child), or when no legal capture
    /// removes the target commoner.
    ///
    /// Defined for **pseudo-legal** `m`, including illegal `m`: the unreachable
    /// child is described exactly as `do_move` would build it. The parent board
    /// is never written and no `StateInfo` is populated.
    ///
    /// Touching commoners do not make the child immune to a blast: the capture
    /// must be legal, so a blast that destroys the child side's own commoner is
    /// rejected; a blast that removes only the target commoner is a win and may
    /// be returned.
    #[must_use]
    pub fn exploding_capture_after(&self, board: &Board, m: Move) -> Option<Move>;
}
```

No Board wrapper is added in this plan. If a future consumer wants
`Board::exploding_capture_after(m, &state)`, it is an additive wrapper with the
same body and can be considered separately.

## Implementation

1. **Add a crate-private helper in `src/effects.rs`.**

   ```rust
   fn exploding_capture_in_view(view: &PositionView, target: Bitboard) -> Option<Move> {
       let us = view.side_to_move();
       let them = us.flip();
       // The child must have both commoners; otherwise there are no legal moves
       // (and `legal_with_counts` expects a live commoner for the side to move).
       if view.commoners(us).is_empty() || view.commoners(them).is_empty() {
           return None;
       }
       if target.is_empty() {
           return None;
       }
       let target_sq = target.lsb();
       let target_zone = target | attacks::king_attacks(target_sq);

       let mut found = None;
       let _ = crate::movegen::for_each_pseudo_legal_in_view::<()>(view, &mut |c| {
           if !view.is_capture(c) {
               return std::ops::ControlFlow::Continue(());
           }
           if !target_zone.contains(c.to_sq() as usize) {
               return std::ops::ControlFlow::Continue(());
           }
           if (view.blast_casualties(c).all() & target).is_empty() {
               return std::ops::ControlFlow::Continue(());
           }
           if crate::position::legal_with_counts(view, c) {
               found = Some(c);
               return std::ops::ControlFlow::Break(());
           }
           std::ops::ControlFlow::Continue(())
       });
       found
   }
   ```

   Notes:
   - `view.is_capture` is the same predicate `child_view` uses; it covers en
     passant.
   - `blast_casualties` is exact for normal, promotion, en-passant and castling
     encodings (castling is not a capture, so it is skipped).
   - `target_zone.contains(c.to_sq() as usize)` is a prefilter only; the
     `blast_casualties` test is the decisive one.
   - `for_each_pseudo_legal_in_view` may produce a pseudo-legal move that is
     malformed for `legal_with_counts` only if the generator is broken; the
     generator's precondition is documented and existing tests cover it.

2. **Implement the public method.**

   ```rust
   pub fn exploding_capture_after(&self, board: &Board, m: Move) -> Option<Move> {
       let child = self.analyze(board, m)?;
       let target = child.view.commoners(self.us());
       exploding_capture_in_view(&child.view, target)
   }
   ```

   `self.us()` is the parent's side to move, i.e. the side that played `m`.
   The child view's side to move is `self.them()`, as `child_view` sets.

3. **Docs.** Add the method to the `EffectContext` docs and the module-level
   "child queries" section. State the target side and the first-in-generation
   order explicitly. Add a doctest with a small position where a capture beside
   the commoner is an immediate explosion, and a negative case with no such
   capture. Keep the doctest positions legal and stable under the crate's rules.

4. **Optional measurement-driven optimization.** Only after the exact first
   implementation passes the differential, consider a crate-private
   `for_each_pseudo_legal_capture_to_in_view(view, target_mask, f)` that skips
   non-captures before the callback. It must produce the same order for the
   captures it visits and the same first hit. If it does not measurably improve
   the row, leave the generic walk in place; do not carry a second generator
   without a measured reason.

## Tests

### Oracle

Add a test-only oracle in `tests/post_move.rs` (or a new `tests/explosion.rs`):

```text
oracle(board, m) -> Option<Move>:
    target = parent mover's commoner in the child (if any)
    if target is empty: return None
    child = clone + do_move(m)
    legal = generate_legal_with_state(child, state)
    if child has no commoner for the child side: return None (no legal moves)
    for c in legal in generation order:
        if child.is_capture(c)
           && (child.blast_casualties(c).all() & target) != EMPTY:
            return Some(c)
    None
```

The oracle deliberately uses the built child and the legal list, the exact
shape the wishlist names. It must use the parent mover's commoner square in the
child (the target square is unchanged by the move unless `m` moved the commoner;
read it from the child). If `m` removed the target commoner, return `None`.

### Differential

For every position of the 41-position perft corpus, the effect fixtures, the
conformance cases, the child cases, and seeded play-outs
(`tests/common/mod.rs`; at least 5 starts × 16 seeds × 300 plies):

- derive `ctx = board.effect_context_with_state(&state)` once per position;
- for every pseudo-legal move `m`:
  - `expected = oracle(&board, m)`;
  - `actual = ctx.exploding_capture_after(&board, m)`;
  - assert `actual.is_some() == expected.is_some()`;
  - when both are `Some`, assert `actual` is a member of the expected legal
    list and has the same `from`/`to`/`MoveType`/promotion encoding as one
    legal capture that removes the target. Because the oracle and the query both
    return the first hit in generation order, they should normally be equal;
    assert strict equality and fall back to membership only if a documented
    generator-order difference is found. Record the count of both-hit rows;
  - assert the parent board's FEN and hash are unchanged after the query.

Also check `m = Move::NONE`, an empty-origin move, and an enemy-origin move
return `None`.

### Named fixtures

Add explicit rows (each asserts the query and the oracle in the same test):

- **direct commoner capture:** `4k3/4r3/8/8/8/8/8/4K3 b - - 0 1` (in a
  child-view unit test) or the equivalent parent/move pair; the black rook
  captures `e1` and the blast does not reach the black commoner on `e8`; must
  return `e7e1`.
- **capture beside the commoner:** the child
  `5r2/8/8/8/3kK3/5R2/8/8 b - - 0 1`; `f8f3` captures the white rook on `f3`,
  whose blast removes the white commoner on `e4` but does not reach the black
  commoner on `d4`. This is the shape `Mover::exposes` is about; the commoners
  touch, which makes the position a useful legal-blast check.
- **en-passant explosion:** the parent
  `k7/8/8/8/3p1K2/8/4P3/8 w - - 0 1`, move `e2e4`; the child has ep target
  `e3`, and `d4e3` is an ep capture whose blast removes the white commoner on
  `f4` while the black commoner on `a8` survives. The returned move must be
  `MoveType::EnPassant`.
- **pawn immunity:** a capture beside a pawn where the target square holds a
  pawn, not a commoner; the query must not treat a pawn as the target. (The
  target is always a commoner, so this is a negative fixture: no explosion of a
  pawn.)
- **touching commoners, only self-exploding candidate:** the child
  `8/8/8/8/3kK3/3q4/8/8 b - - 0 1` and a variant where the only capture beside
  the white commoner would also destroy the black commoner because the
  commoners touch; the query must return `None`, proving it respects the
  self-explosion branch.
- **touching commoners, blast from a non-touching square:** the child
  `5r2/8/8/8/3kK3/5R2/8/8 b - - 0 1`; `f8f3` is legal and removes only the
  white commoner. This pins the distinction between "touching makes direct
  attacks/check immune" and "a legal blast can still win".
- **self-explosion of the mover:** a parent move that removes the mover's own
  commoner; the child target is absent and the query returns `None`.
- **already-extinct child:** a parent move that removes the child side's
  commoner (the child side has no commoner); the query returns `None` and the
  oracle has no legal moves.
- **no candidate:** a quiet child with no capture near the commoner; `None`.

### Cost measurement

Extend `examples/effect_bench.rs` with rows in the per-child section:

- `ctx.exploding_capture_after` (ns per child, all pseudo-legal children);
- the same query on the *built-ish* class and on the quiet class;
- the oracle cost: `clone + do_move + generate_legal_with_state` + filter, in
  the same run;
- the hit rate: how many children return `Some`, split by class.

There is no hard cost acceptance bar in W3; the wishlist asks for the cost
before any guide integration and says to keep the prover-side helper until the
measurement justifies the swap. The report must state the measured ratio and
the hit rate, and must not claim a downstream win.

## Verification

All global gates of the index, plus:

```sh
cargo run --release --example effect_bench 9 2000   # three runs, medians
cargo test --test post_move
cargo test --test effects
cargo test --test child_state
cargo run --release --example verify_perft 5
```

Acceptance:

- zero disagreements with the exhaustive oracle on the corpus, fixtures and
  random play-outs (existence and the returned move);
- no board write in any test;
- no regression > 2 % in the existing child-path rows or perft timing;
- the new query's cost and hit rate are published, even if it is expensive.

## Files to touch

- `src/effects.rs` — `exploding_capture_in_view`,
  `EffectContext::exploding_capture_after`, docs, unit tests.
- `src/position.rs` — visibility only if `legal_with_counts` needs to be
  `pub(crate)` for `effects.rs` (it already is crate-private; confirm).
- `tests/post_move.rs` or new `tests/explosion.rs` — oracle differential,
  named fixtures, no-board-write test.
- `examples/effect_bench.rs` — query/oracle rows and hit-rate counts.
- `CHANGELOG.md`, `README.md`, `src/lib.rs` — public API documentation.
- `docs/plans/feedback/report_wishlist1_3.md` — final report.

## Non-goals

- No change to `MoveClassification`, `MoveEffect`, `post_move`, terminality, or
  legality answers.
- No all-captures iterator, no "best capture" policy, no search/evaluation
  concept.
- No mutation of `Board` or `StateInfo`; no public `PositionView`.
- No second move generator unless a measured optimization needs one.
- No claim that the downstream should replace `Mover::exposes` before its own
  wall measurement.

## Final task

Write `docs/plans/feedback/report_wishlist1_3.md` with item status, the exact
public semantics (target side, first-hit order, `None` cases), differential
counts (positions, pseudo-legal moves, hits, oracle rows, fixture rows), the
benchmark table with absolute ns and ratio to the clone+legal-list oracle, the
hit rate by class, changed files, problems, deviations with measurements,
unresolved parts, missing tests, and next steps (whether a specialized capture
walk is worth adding and whether the downstream will run its own
`Mover::exposes` differential).
