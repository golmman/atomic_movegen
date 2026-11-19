# Plan 1 — counts-only state (W6)

## Scope

Make the two commoner counts a first-class, cheap node value and make the
existing legality precondition explicit:

- add a public `Counts` value holding the side-to-move and opponent commoner
  counts;
- add `Board::counts()` and `Board::populate_counts(&mut StateInfo)`;
- document exactly which entry points need only those fields and which still
  need a fully populated `StateInfo`;
- leave `Board::populate_state`, `StateInfo` layout, the generator, and every
  legality answer unchanged.

Requirements served: wishlist 2 W6 (P3). The wishlist acceptance is equality of
`legal(m, &counts_only)` and `legal(m, &full)` over the conformance corpus and
seeded play-outs, with the downstream differential and bench counters
unchanged.

## Current-tree assessment

- `src/board.rs` defines `StateInfo` with four cached fields:
  `checkers`, `pinned`, `commoners_count`, `them_commoners_count`. The first
  two are “hot” for move generation; the last two are the game-over/count
  fields.
- `Board::legal(m, state)` delegates to `position::legal_in_view`, which:
  - rejects every move when either count is zero;
  - calls `legal_with_counts`, which reads placement and `Move` shape only.
  It never reads `state.checkers` or `state.pinned`.
- `Board::is_legal_with_state` calls `is_pseudo_legal` (a board query) and then
  `legal`; it therefore also needs only the two counts.
- `EffectContext::new` reads only the two counts for its `game_over` snapshot.
  Its child/effect queries do not read `checkers`/`pinned` either.
- `movegen::generate_legal_with_state`, `for_each_legal_with_state`,
  `has_legal_move_with_state`, and the terminal/existence paths *do* read
  `checkers` and `pinned`. They must keep requiring a full `populate_state`.
- `Board::populate_state` computes `checkers` (an attack query), `pinned` (a
  slider-ray pass), and the two counts. There is no counts-only writer.
- There is no public value type for the counts; callers construct a
  `StateInfo`, populate it fully, and then pass it to `legal`.

The wishlist's premise is therefore correct: `Board::legal` needs two
`popcount`s, not the full cached state. This plan makes that usable without
making a stale or partially populated `StateInfo` look safe to the generator.

## Decisions and pushback

1. **Add a value type, not only documentation.** The wishlist permits either
   “a `populate_counts`” or “a documented statement that the counts suffice”.
   A docs-only change would leave W7/W8 passing two bare `u32`s through their
   APIs. The plan adds a small `Counts` value so the count pair has one name,
   one constructor, and one documentation site. The precondition documentation
   is still part of the deliverable, not a replacement for it.

2. **`Counts` fields are private, with a constructor and getters.** The counts
   are a pair that must correspond to a particular board and side to move. A
   public struct literal invites mismatched or stale pairs with no place for
   the contract. The type is `Copy`, `Eq`, and small (two `u32`s); construction
   is `Counts::new(commoners, them_commoners)`. `Board::counts()` and
   `From<&StateInfo>` are the supported ways to obtain the current pair.

3. **`populate_counts` writes only the two count fields.** It must not clear or
   compute `checkers`/`pinned`; a caller may already have those values and may
   be filling a reused `StateInfo`. The method is not a substitute for
   `populate_state`. Its documentation says so, and the method name is distinct
   from `populate_state`.

4. **Document the precondition in the right places.**
   - `Board::legal`: the *only* required cached fields are
     `commoners_count` and `them_commoners_count`; `m` must remain
     pseudo-legal.
   - `Board::is_legal_with_state`: same state requirement; the pseudo-legal
     shape check is unchanged.
   - `EffectContext::new`: reads only the two counts for its game-over flag.
   - `generate_legal_with_state`, `for_each_legal_with_state`,
     `has_legal_move_with_state`, `Board::is_terminal`/`outcome` state paths:
     explicitly still require the full cached state.
   This split is the point of W6; a vague “state must be populated” sentence on
   every method would not address the request.

5. **Do not add `Board::legal_with_counts` in this plan.** Plan 2 owns the
   candidate-loop entry point. W6 only makes the counts cheap; adding a second
   legality wrapper here would create two public shapes before the measurement
   in plan 2 decides which one is justified.

6. **No `StateInfo` layout change.** The struct is public and its fields are
   public; rearranging or nesting it would be a breaking change for no
   correctness reason. `Counts` is a separate value. Existing code continues to
   read `state.commoners_count` etc.

## Proposed API

```rust
// src/board.rs

/// The two commoner counts a legality check reads.
///
/// The first count belongs to the side to move; the second to its opponent.
/// A legal atomic position has zero (game over) or one commoner per side.
///
/// This value is a snapshot of the board's placement. A stale `Counts` changes
/// the legality answer in exactly the same way a stale [`StateInfo`] does: a
/// zero count rejects every candidate, and a nonzero count no longer matches
/// the board if a commoner has been captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    commoners: u32,
    them_commoners: u32,
}

impl Counts {
    /// Build a count pair for a side to move and its opponent.
    #[must_use]
    pub const fn new(commoners: u32, them_commoners: u32) -> Self;

    /// The number of commoners of the side to move.
    #[must_use]
    pub const fn commoners(self) -> u32;

    /// The number of commoners of the opponent.
    #[must_use]
    pub const fn them_commoners(self) -> u32;
}

impl From<&StateInfo> for Counts {
    fn from(state: &StateInfo) -> Self;
}

impl Board {
    /// The two commoner counts of this position.
    #[must_use]
    pub fn counts(&self) -> Counts;

    /// Fill only `commoners_count` and `them_commoners_count` in `state`.
    ///
    /// `checkers` and `pinned` are left untouched. This is enough for
    /// [`legal`](Self::legal) and [`is_legal_with_state`](Self::is_legal_with_state),
    /// but **not** for `generate_legal_with_state`,
    /// `has_legal_move_with_state`, or any other state-aware move-generation
    /// query; those still require [`populate_state`](Self::populate_state).
    pub fn populate_counts(&self, state: &mut StateInfo);
}
```

Re-export `Counts` from `src/lib.rs` and from `src/board.rs` alongside
`StateInfo`.

## Implementation

1. **Add `Counts` after `StateInfo` in `src/board.rs`.** Keep the fields
   private. Add `new`, the two getters, and `From<&StateInfo>`. Add a
   crate-private `PositionView::counts()` helper next to
   `populate_state_with_checkers`, so the board method and the view share one
   expression:

   ```rust
   pub(crate) fn counts(&self) -> Counts {
       let us = self.side_to_move();
       Counts::new(
           self.commoners(us).count(),
           self.commoners(us.flip()).count(),
       )
   }
   ```

2. **Add the board methods.** `Board::counts()` delegates to
   `self.position_view().counts()`. `Board::populate_counts` writes only the
   two fields and leaves everything else unchanged:

   ```rust
   pub fn populate_counts(&self, state: &mut StateInfo) {
       let counts = self.position_view().counts();
       state.commoners_count = counts.commoners();
       state.them_commoners_count = counts.them_commoners();
   }
   ```

   Do not call `self.counts()` and then re-read it; either shape is fine, but
   keep the two writes visibly adjacent so a future edit cannot accidentally
   start computing pins.

3. **Document the split.** Update the docs on:
   - `Board::legal` (remove the blanket “populated” wording; name the two
     fields);
   - `Board::is_legal_with_state`;
   - `EffectContext::new` (counts-only game-over flag);
   - `generate_legal_with_state`, `for_each_legal_with_state`,
     `has_legal_move_with_state`, and the internal `FastLegality` users, which
     gain one sentence saying they still need `checkers` and `pinned`.
   Keep `Board::populate_state`'s contract byte-for-byte in behavior; only add
   a cross-reference to `populate_counts`.

4. **Export the type.** Add `Counts` to `pub use board::{...}` in `src/lib.rs`
   and to the public-API test in plan 5. Update the module-level hot-path
   paragraph in `src/lib.rs` and the corresponding README hot-path section:
   the cheap count pair is the W6 shape; a full state is still the generator
   shape.

5. **No new benchmark harness yet unless one already exists.** Plan 1 introduces
   `examples/hotpath_bench.rs` with two setup rows (`populate_state`,
   `populate_counts`) and one reader row (`Board::counts`). Later plans extend
   this file; the row names stay stable.

## Tests

### Unit tests in `src/board.rs`

- `counts_match_full_populate`: for the board fixtures, the perft corpus, and a
  seeded walk, `board.counts()` equals
  `Counts::from(&full_state)` and the two fields of a full `populate_state`.
- `populate_counts_preserves_other_fields`: fill a `StateInfo` with sentinel
  `checkers`, `pinned`, and undo fields; call `populate_counts`; assert only
  the two counts changed. `populate_state` must still overwrite all four cached
  fields as before.
- `counts_constructor_getters`: a tiny value-level test for `new`, getters, and
  `From<&StateInfo>`.

### Differential tests in `tests/effects.rs` (or a new `tests/counts.rs`)

For every position of the 41-position perft corpus, the effect fixtures, the
conformance cases and the seeded play-outs:

- populate `full = StateInfo::new(); board.populate_state(&mut full);`
- build `counts_state = StateInfo::new(); board.populate_counts(&mut counts_state);`
- for every pseudo-legal move:
  - assert `board.legal(m, &counts_state) == board.legal(m, &full)`;
  - assert `board.is_legal_with_state(m, &counts_state) ==
    board.is_legal_with_state(m, &full)`;
- for every legal move from the generator, assert `legal` is `true` in both
  states (the differential already covers this, but it pins the consuming
  shape);
- record how many positions and candidates were checked in the report.

Also assert that a fresh `StateInfo::new()` (counts zero) rejects every move,
which is the documented game-over guard and prevents a caller from mistaking
“unfilled” for “safe”.

### Regression tests

- Existing `src/position.rs` tests compare `PositionView::populate_state` with
  the board; keep them green. Add the counts-only comparison next to them.
- Existing `generate_legal_with_state`/`has_legal_move_with_state` tests must
  not be changed to counts-only states; add an explicit test that a counts-only
  state is *not* sufficient for `has_legal_move_with_state` by using a fixture
  with a pin/check. This documents the boundary instead of leaving it to prose.

## Benchmark

Extend `examples/hotpath_bench.rs` with:

| Row | What it measures |
| --- | --- |
| `populate_state` | current full cached-state population |
| `populate_counts` | the new two-popcount writer |
| `StateInfo::new` | the baseline construction cost, for context |

Run on the perft corpus and the play-out walk, median of at least 9 batches.
The report gives absolute ns and `populate_counts / populate_state`. If
`perf stat -e instructions:u` is stable on the host, also report instructions
per call; the wishlist's “166 → 18” figure is a diagnostic comparison, not a
portable gate.

No whole-binary perft gate is required for this plan by itself, but the global
gates still run, because `populate_state` is on the generator path and must not
change.

## Files to touch

- `src/board.rs` — `Counts`, `Board::counts`, `Board::populate_counts`,
  precondition docs, unit tests.
- `src/position.rs` — crate-private `PositionView::counts`, counts-only tests.
- `src/lib.rs` — root re-export and hot-path docs.
- `tests/public_api.rs` — `Counts` resolves; `Board::counts`/
  `populate_counts` resolve.
- `tests/effects.rs` or new `tests/counts.rs` — differential and boundary
  tests.
- `examples/hotpath_bench.rs` — new harness with the setup rows.
- `README.md`, `CHANGELOG.md` — additive value and precondition docs.
- `docs/plans/feedback/report_wishlist2_1.md` — final report.

## Non-goals

- No lazy or partial `populate_state`; `checkers`/`pinned` remain required for
  move generation.
- No change to `StateInfo` fields, layout, or `Default`.
- No `Board::legal_with_counts` or candidate context here; plan 2 owns it.
- No change to `Board::legal`'s answer, `EffectContext`, the generator, perft,
  move order, FEN, terminality, or the Zobrist tables.
- No new dependency and no new `unsafe`.

## Final task

Write `docs/plans/feedback/report_wishlist2_1.md` with:

- the exact public API that landed;
- differential counts: positions, pseudo-legal candidates, legal candidates,
  and the number of positions with a zero count;
- the setup benchmark table (absolute ns, `populate_counts / populate_state`,
  optional instructions per call);
- confirmation that `populate_state` behavior and the generator's state
  requirements are unchanged;
- changed files, problems, deviations with the measurement that motivated
  them, unresolved parts, missing tests, and next steps for plans 2 and 3.
