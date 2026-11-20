# Plan 1 — evasion-aware `LegalityContext` (W10)

## Scope

Add an opt-in, state-aware constructor for the existing candidate-loop legality
context:

- `Board::legality_context_with_state(counts, &StateInfo)`;
- a node-local evasion mask built once from the supplied `StateInfo::checkers`;
- a rejection-only prefilter in `LegalityContext::legal(m)` for quiet
  non-commoner moves and castling while in check;
- the existing `Board::legality_context(counts)` and every existing legality
  answer unchanged;
- differential tests covering all branches named by the wishlist;
- a same-host benchmark gate for the candidate loop.

Requirements served: wishlist 3 `W10` (P1). The wishlist acceptance is exact
equality with the crate's existing `legal_in_view` oracle for every
pseudo-legal move of an in-check position, with branch coverage for one slider
check (inside and outside the block mask), double check, knight and pawn check,
touching commoners, castling in check, en passant in check, a promotion push
that blocks, and a quiet move that would open a second line to the commoner.
The wishlist's `0.72e9` whole-binary target is downstream evidence; this plan
does not promise or measure it.

## Current-tree assessment

- `LegalityContext` stores a `LegalNode` snapshot and a `playable` flag. It is
  built by `Board::legality_context(counts)`, which reads only placement and the
  two counts. `LegalityContext::legal(m)` returns
  `playable && node.legal(m)`.
- `LegalNode` stores the per-type sets, side to move, occupancy, opponent
  pieces, and the two commoner sets. It already has everything the evasion rule
  needs except the checker set.
- `LegalNode::legal` calls the shared `legal_in` body. `position::legal_in_view`
  keeps the game-over guard from `StateInfo`; the board path uses a borrowed
  `PositionView`, while the context uses `LegalNode`. There is one rules
  implementation.
- `StateInfo::checkers` is populated by `Board::populate_state`. `PositionView`
  computes it as the enemy pawns, knights and sliders attacking the lowest
  commoner of the side to move. It returns empty when either side has no
  commoner, and when the two commoners touch (touching grants immunity to all
  attacks).
- `attacks::between` / `bitboard::between_bb` provide the strictly-between
  squares; `attacks::king_attacks` provides the touching test. `LegalNode`
  already exposes the node occupancy through `occupied()` and stores
  `our_commoners`.
- The generator's crate-private `TrivialLegality` rejects every in-check move
  to the oracle; `WidenedLegality` requires empty `StateInfo::checkers`. Neither
  is the candidate-loop API and neither is changed here.
- `tests/legality_context.rs` already checks the check-blind context against
  `Board::legal(m, &full)` over the corpus, fixtures and seeded play-outs, and
  counts in-check positions. It does not build the new context or exercise a
  mask rejection.
- `examples/hotpath_bench.rs` already has a candidate-entry structure with
  `state`, `counts`, `ctx`, move classes, and per-class `Board::legal` versus
  `ctx.legal` rows. It has no in-check/out-of-check split and no state-aware
  context setup row.

## Why the rule is sound

The prefilter is a rejection rule, not a second legality implementation. Its
soundness rests on the fact that a quiet non-commoner move changes occupancy in
exactly two places: it empties the origin and fills the destination.

1. **A quiet non-commoner move cannot remove a checker.** It is not a capture
   and does not trigger a blast, so the enemy checker remains on the board.
   It does not move our commoner, so the attacked square remains the same.
2. **A leaper check cannot be blocked.** A knight or pawn attack does not pass
   through intervening squares. A quiet move can therefore never answer it.
3. **A slider check can only be answered by interposing.** With one slider
   checker, the attack on the commoner exists exactly when the squares strictly
   between the slider and the commoner are empty. A quiet destination outside
   that between-set cannot close the ray. Removing a blocker can only open or
   preserve a ray, never close one.
4. **Double check cannot be answered by one quiet move.** Two distinct slider
   rays meet at the commoner, not at a square strictly between both; two
   checkers on the same ray would be occluded by the nearer one. A single
   destination cannot block both. Any leaper in a multi-checker set cannot be
   blocked at all.
5. **Castling while in check is illegal.** The existing body tests the
   commoner's origin as a pass-through square. With a nonempty checker set, that
   origin is attacked; touching commoners would have made the checker set empty.
   The early rejection matches the body.
6. **Everything else falls through.** Captures (including en passant),
   commoner moves, castling outside check, and quiet moves outside check are
   not decided by the mask. A quiet move whose destination is inside the block
   mask is also not accepted by the mask; the existing body still decides it,
   including self-explosion and the post-move attack query.

Therefore every mask rejection is a move the existing body rejects. The mask
never returns `true` for a move the oracle rejects. A wrong or stale `checkers`
input can only make the context reject too much; that is a state-contract
violation, not a new accepting path.

The build uses the node's lowest commoner, matching `PositionView::checkers`
and the existing body's `our_commoners.lsb()` after a quiet non-commoner move
(which leaves the commoner set unchanged). Multi-commoner artificial boards are
therefore not silently given a different attacked square. If either side has no
commoner or the commoners touch, the mask is disabled; the normal game-over
guard and rules body remain authoritative.

## Proposed API

```rust
impl Board {
    /// Build a check-aware node-local [`LegalityContext`].
    ///
    /// `counts` and `state` must describe this board and its side to move.
    /// Only `state.checkers` is read; `pinned`, undo fields and the remaining
    /// `StateInfo` fields are ignored, and no state is populated or
    /// recomputed. Use [`populate_state`](Self::populate_state) before this
    /// call. A stale state changes answers exactly as a stale `Counts` does.
    ///
    /// The context rejects quiet non-commoner moves that cannot answer a check
    /// before running the shared legality body. Captures, en passant,
    /// commoner moves, castling outside check, quiet moves outside check, and
    /// quiet moves onto the block ray still use the full shared body.
    #[must_use]
    pub fn legality_context_with_state(
        &self,
        counts: Counts,
        state: &StateInfo,
    ) -> LegalityContext;
}
```

`Board::legality_context(counts)`, `LegalityContext`, its `occupied()` and
`legal()` signatures, `Counts`, `StateInfo`, and all existing re-exports stay
unchanged. The new constructor is additive.

## Internal shape

```rust
// src/position.rs

/// Rejection-only node data for an in-check candidate loop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EvasionMask {
    /// Squares strictly between our commoner and the sole slider checker.
    /// Empty when no quiet move can answer the check.
    block: Bitboard,
    /// Whether the side to move is in check.
    in_check: bool,
}

impl EvasionMask {
    pub(crate) const NONE: Self;

    /// Derive the mask from a view and the caller-supplied `StateInfo::checkers`.
    pub(crate) fn from_view(view: &PositionView, checkers: Bitboard) -> Self;

    /// Return `true` only for a quiet non-commoner move that cannot answer
    /// the check, or for castling while in check.
    #[inline(always)]
    pub(crate) fn rejects(
        &self,
        m: Move,
        our_commoners: Bitboard,
        occupied: Bitboard,
    ) -> bool;
}

pub(crate) struct LegalNode {
    // existing fields ...
    evasion: EvasionMask,
}

impl LegalNode {
    pub(crate) fn from_view(view: &PositionView) -> Self;
    pub(crate) fn from_view_with_checkers(view: &PositionView, checkers: Bitboard) -> Self;

    pub(crate) fn legal(&self, m: Move) -> bool {
        if self.evasion.rejects(m, self.our_commoners, self.occupied) {
            return false;
        }
        legal_in(self, m)
    }
}
```

`EvasionMask` is private to `src/position.rs`; no new public type is needed.
`Board::legality_context_with_state` constructs `LegalNode` with
`from_view_with_checkers`; the old constructor keeps `from_view`, which stores
`EvasionMask::NONE`. The wishlist's “commoner square” field is deliberately
omitted: `LegalNode` already stores `our_commoners`, so a separate square would
be redundant and one more field to keep in sync.

## Implementation

1. **Build the mask from `checkers`, never recompute the attacker set.**
   Add `EvasionMask::from_view(view, checkers)`:
   - `checkers` empty → `NONE`;
   - either side has no commoner → `NONE` (the game-over guard handles it);
   - the commoners touch → `NONE` (matching `PositionView::checkers` immunity);
   - exactly one checker, that checker is an enemy bishop/rook/queen →
     `block = between_bb(our_commoner, checker)`, `in_check = true`;
   - otherwise → empty `block`, `in_check = true`.

   Identify the slider from the node's per-type sets and the opponent set, not
   by a second attack query. Use the lowest commoner from `our_commoners`.
   `between_bb` is a table lookup and must not be replaced with a fresh attack
   calculation.

2. **Add the rejection predicate.** `EvasionMask::rejects` returns `false`
   immediately when `!in_check`. It then handles the move shape in this order:
   - `MoveType::Castling` → reject;
   - `MoveType::EnPassant` → fall through;
   - `Normal | Promotion` → if the origin is one of our commoners, fall
     through; if the destination is occupied, fall through (capture); otherwise
     reject exactly when `block` does not contain the destination.

   The order matters: castling is rejected before the commoner-origin test
   because the castling move's origin is the commoner; en passant is excluded
   before the empty-destination test because its destination is empty.

3. **Wire it into `LegalNode`.** Add the `evasion` field and make `from_view`
   call the existing snapshot path with `Bitboard::EMPTY`. Add
   `from_view_with_checkers` for the new constructor. Put the prefilter immediately
   before `legal_in(self, m)` in `LegalNode::legal`. Do not modify `legal_in`,
   `legal_in_view`, `legal_with_counts`, or the `LegalData` trait. The board
   path never constructs a `LegalNode` and therefore pays no new branch.

4. **Add the board constructor.** In `src/board.rs`, make
   `Board::legality_context_with_state` call
   `LegalNode::from_view_with_checkers(self.position_view(), state.checkers)` and
   reuse the same `playable` expression from `legality_context`. It must not
   call `populate_state`, `checkers`, `pinned`, or any attack query.

5. **Document the split.** Update `LegalityContext`'s docs to name both
   constructors: `Board::legality_context(counts)` is check-blind and unchanged;
   `Board::legality_context_with_state(counts, state)` uses `state.checkers`.
   Document the stale-state contract and the fact that `pinned` is ignored. Add
   one paragraph to the hot-path section of `src/lib.rs` and the candidate-loop
   bullet in `README.md`.

6. **Keep the risk concentrated.** No change to `StateInfo` layout,
   `populate_state`, the generator, `Board::legal`, the effect path, keys,
   FEN, castling/EP validation, or terminality. If the added field/branch
   regresses the existing check-blind context or `Board::legal` by more than
   the global ~2 % gate, keep the prefilter in a separate internal path used
   only by the state-aware context; do not trade the existing hot path for
   W10.

7. **Extend the benchmark.** In `examples/hotpath_bench.rs`, add
   `evasion_ctx: LegalityContext` to each candidate entry, built once from
   `board.legality_context_with_state(entry.counts, &entry.state)`. Add rows:
   - setup: `legality_context` versus `legality_context_with_state` ns/node;
   - all candidates: `Board::legal`, current `ctx.legal`, new
     `evasion_ctx.legal`;
   - in-check candidates split by quiet, capture, en passant, castling,
     commoner move and all;
   - out-of-check candidates split by quiet and all, to prove the extra branch
     did not regress the common case.

   The timed loops must not rebuild contexts or classify moves. Keep the
   existing row names stable and add new suffixes for the state-aware rows.

## Tests

### Differential tests in `tests/legality_context.rs`

Extend `check_position` so that, after `full` is populated:

```text
ctx           = board.legality_context(counts)
evasion_ctx   = board.legality_context_with_state(counts, &full)
```

and for every pseudo-legal move:

```text
evasion_ctx.occupied() == board.occupied()
evasion_ctx.legal(m)   == ctx.legal(m)
                       == board.legal(m, &full)
                       == board.legal(m, &counts_only)
                       == board.is_legal_with_state(m, &counts_only)
```

The existing corpus, conformance/child cases, effect/board fixtures, edge
fixtures and seeded play-outs are the acceptance set. Record at least:

- positions, terminal positions, in-check positions, pinned positions, EP
  targets, castling-right positions;
- candidates, legal candidates, castling candidates, EP candidates;
- in-check quiet/capture/EP/castling/commoner candidates;
- predicted mask rejections, and legal moves among them (must be zero);
- branches that reached the “single slider with a block mask”, “empty block in
  check”, and “not in check” cases.

Recompute the *predicted* rejection in the test from public data
(`full.checkers`, `board.commoners`, `attacks::between`, `occupied`, move
shape) so the report can state how many candidates the mask rejected without
adding production counters. Assert every predicted rejection is `false` under
`Board::legal`; the equality assertions then prove the production prefilter
agrees.

### Focused fixture tests

Add a fixture table with these positions (FENs already parse under the default
parser; locate moves from the pseudo-legal generator, not from
`Board::move_from_uci`, which requires legality):

| Branch | FEN | Expected |
| --- | --- | --- |
| one slider check, block inside/outside | `4r1k1/8/8/8/8/8/8/4K1N1 w - - 0 1` | `g1e2` (blocks) is legal; `g1f3` (does not block) is illegal; `e1d1` (commoner move) is legal |
| double check | `4k3/8/8/8/1b6/8/4r3/4K1N1 w - - 0 1` | `g1f3` quiet move is illegal; `e1d1` commoner move is legal |
| knight check | `4k3/8/8/8/8/5n2/8/P3K3 w - - 0 1` | `a2a3` quiet move is illegal; `e1d1` is legal |
| pawn check | `4k3/8/8/8/8/8/3p4/P3K3 w - - 0 1` | `a2a3` quiet move is illegal; `e1d1` is legal |
| touching commoners | `8/8/8/8/8/8/1k6/2K5 w - - 0 1` | `full.checkers` is empty; the context uses no evasion rejection |
| castling in check | `r3k2r/8/8/8/8/8/4r3/R3K2R w KQkq - 0 1` | `e1g1` and `e1c1` pseudo-legal castling candidates are illegal |
| en passant in check | `4k3/8/8/3pP3/4K3/8/P7/8 w - d6 0 1` | `e5d6` is legal; the mask does not reject EP |
| promotion push that blocks | `K6r/6P1/4k3/8/8/8/8/8 w - - 0 1` | the four `g7g8` promotion encodings are legal; the mask does not reject the push |
| quiet move opening a second line | `4r1k1/8/8/8/8/8/8/rN2K3 w - - 0 1` | `b1d2` is illegal; `e1d1` is legal |

For the castling row, assert the pseudo-legal stream contains both castling
candidates before checking them; if a fixture cannot produce a candidate under
the current encoder, replace the fixture with one that does rather than
silently dropping the branch. For the promotion row, check every promotion
piece encoding generated for `g7g8`, not just one.

### Unit tests in `src/position.rs`

Add small value-level tests for `EvasionMask`:

- empty checkers and touching commoners produce `NONE`;
- one slider produces a nonempty `block` between the commoner and checker;
- one leaper, multiple checkers, or a slider with no intervening square produce
  `in_check` with an empty `block`;
- `rejects` returns false for en passant, captures, commoner origins, and quiet
  moves outside check; it returns true for castling in check and for quiet
  non-commoner moves whose destination is outside a nonempty block or when the
  block is empty.

These are predicate tests; the corpus differential remains the correctness
oracle.

### Regression tests

- The existing `tests/legality_context.rs` assertions for the check-blind
  context must remain unchanged and green.
- `tests/public_api.rs` gains the new constructor only in plan 2 when the
  release is cut; plan 1 may add a local compile/use assertion in
  `tests/legality_context.rs`.
- The `robustness` suite keeps its stale-count and arbitrary-move checks; add
  one case with a hand-written `StateInfo` whose `checkers` is current and
  whose counts are zero, confirming `playable` still wins and no panic occurs.
- `fen()`, `hash()`, `ep_square()`, `rule50()`, and `game_ply()` are unchanged
  across construction and queries.

## Benchmark and shipping gate

Harness: `examples/hotpath_bench.rs`, run with a stable release build. Report
median ns and same-run ratios for:

| Row | Purpose |
| --- | --- |
| `legality_context` setup | existing context construction |
| `legality_context_with_state` setup | added mask construction (no attack query) |
| all candidates, current `ctx.legal` | existing shipped shape |
| all candidates, new `evasion_ctx.legal` | whole-candidate-loop effect |
| in-check quiet candidates, new vs old vs `Board::legal` | primary target |
| in-check all/capture/EP/castling/commoner | no hidden class regression |
| out-of-check quiet/all, new vs old | common-case branch cost |

Shipping gate:

- exactness on the full differential is mandatory;
- the new context must be repeatably faster than the current context on the
  in-check all-candidate row, and on the in-check quiet row where the wishlist
  expects the gain — plan gate: at least 2 % lower median ns in repeated runs;
- no candidate class and no out-of-check row may regress by more than 2 %
  against the current context in the same run;
- the added setup cost must be amortised by the candidate loop: report the
  setup delta and the per-node candidate count, and do not ship if the added
  construction cost exceeds the measured candidate saving on both datasets;
- `Board::legal`, `populate_state`, perft timing and the existing effect rows
  must stay inside the global 2 % blocker.

If the gate fails, the report records the result, W10 stays open, and the
public constructor is not shipped. If the gate passes, the report states that
the same-host candidate improvement is real; it must still not claim the
wishlist's whole-binary `0.72e9` or a downstream `.pvr` result. When
`perf stat -e instructions:u` is available and stable, report the command,
batch size, and instructions per call as a diagnostic against the prototype.

## Files to touch

- `src/position.rs` — `EvasionMask`, `LegalNode` field/constructors, prefilter,
  unit tests.
- `src/board.rs` — `Board::legality_context_with_state`, docs, unit tests.
- `src/lib.rs` — hot-path documentation; no root re-export change.
- `tests/legality_context.rs` — differential, statistics, focused fixtures.
- `tests/robustness.rs` — one current-checkers/zero-count case if not already
  covered by the legality-context suite.
- `examples/hotpath_bench.rs` — state-aware context setup and candidate rows.
- `README.md` — candidate-loop hot-path bullet.
- `docs/plans/feedback/report_wishlist3_1.md` — final report.

## Non-goals

- No second legality implementation, no prover-side legality, and no copied
  castling/EP/blast logic.
- No change to `Board::legal`, the generator, move order, perft, FEN,
  terminality, keys, `ZOBRIST_VERSION`, or `StateInfo` layout.
- No change to the existing check-blind `Board::legality_context(counts)`.
- No mask acceptance of captures, en passant, commoner moves, or any move
  outside check.
- No attempt to settle the 21 legal castling/EP calls in check mentioned by the
  wishlist; EP falls through to the oracle by design.
- No new dependency, no new `unsafe`, no allocation, and no downstream
  pin/provenance work.

## Final task

Write `docs/plans/feedback/report_wishlist3_1.md` with:

- the exact public API that landed (or the open-item decision if the gate
  failed);
- the proof summary and the exact mask conditions;
- differential counts: positions, in-check branches, candidates, mask
  rejections, legal moves among rejections (expected zero), and focused fixture
  results;
- the benchmark table with absolute ns, ratios, setup cost, and optional
  instructions per call;
- perft/effect/legality regression results and the old-context comparison;
- changed files, problems, deviations with the measurement that motivated
  them, unresolved parts, missing tests, and next steps for plan 2.
