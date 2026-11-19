# Plan 2 — legality entry point at occupancy cost (W7)

## Scope

Add a node-local legality context for a candidate loop:

- `LegalityContext`, built once per node from `&Board` and `Counts`;
- `LegalityContext::legal(m)`, exactly equal to the existing
  `Board::legal(m, &state)` for pseudo-legal `m`;
- `LegalityContext::occupied()`, the occupancy computed once for the node, so
  a child/effect query can reuse it;
- one shared legality body behind both the board and the context, not a second
  rules implementation;
- an optional quiet-move fast path only if it is measured and remains exact.

Requirements served: wishlist 2 W7 (P1). The wishlist acceptance is exact
equality with today's `legal` over the differential corpus, fixture positions
and play-outs, plus a per-candidate instruction target of ≤ 100 (today 144) and
no whole-binary regression. The wishlist itself says to keep the item open if
the ladder cannot go below about 120; this plan makes that an explicit
measurement gate rather than a promised number.

## Current-tree assessment

- `Board::legal(m, state)` is a thin wrapper over
  `position::legal_in_view(self.position_view(), m, state)`.
- `legal_in_view` applies the game-over guard (`commoners_count == 0 ||
  them_commoners_count == 0`) and calls `legal_with_counts`.
- `legal_with_counts` is the actual rule body. It:
  - checks the origin is non-empty;
  - computes `is_capture` from the destination and move type;
  - handles castling pass-through squares;
  - derives post-move occupancy, including the castling rook, the en-passant
    victim, and the capture blast;
  - compares own-commoner survival;
  - returns `true` immediately when the enemy commoner is extinct, handles
    touching commoners, and otherwise runs one attack query.
- `Board::legal` reaches the view through a borrowed `&PositionView` per call.
  The view is already stored on the board, so this is not a 72-byte copy, but
  the body still re-derives occupancy and the commoner sets for every
  candidate.
- `FastLegality`, `TrivialLegality`, and `WidenedLegality` are crate-private
  move-generation ladders. They need `state.checkers` and `state.pinned` and
  are not a counts-only legality API.
- A previous attempt to split `Board::legal` into move categories
  (`docs/plans/perf_analysis7/plan4.md`) was implemented, measured at roughly
  0.3–0.6 % on one FEN and within noise on another, and reverted
  (`report4.md`). This plan does not repeat that split.

## Decisions and pushback

1. **The public shape is a context, not a second `Board::legal` wrapper.**
   A `Board::legal_with_counts(m, counts)` would still recompute the
   node-constant data on every call. W7 asks for a candidate-loop shape and an
   exposed occupancy. `LegalityContext` is built once per node and then answers
   every candidate from the same snapshot. The board remains the owner of the
   position; the context does not borrow it and stores no lifetime.

2. **The context is not an `EffectContext` and does not produce effects.**
   The wishlist explicitly rejects extending the effect path. `LegalityContext`
   has one rules question (`legal`) and one data accessor (`occupied`). Plan 3
   consumes that occupancy through `Board::child_effect_with_occupancy`; it does
   not add a second effect API to the context itself.

3. **One rule body.** `position::legal_with_counts` is refactored into a
   crate-private `LegalNode` (or an equivalent parts function) that both the
   existing board path and the new context call. The public context must not
   contain a copy of the castling/EP/blast logic. If the refactor makes the
   existing `Board::legal` path slower, the plan keeps the board path's current
   argument computation and shares the body through a free function rather than
   constructing a node per call; the public context stores the same arguments.
   Either way, there is one implementation of the rule.

4. **No category-specialised legality split.** The prior experiment is
   documented as a revert. This plan may branch on “castling first” versus
   “non-castling first” to satisfy the wishlist's common-case ordering, but it
   must not introduce the five helper categories that were measured and
   rejected. The full body remains the oracle.

5. **A quiet-move fast path is optional and measured.** The wishlist welcomes a
   `legal_is_quiet_cheap`-style path only if it is exact. The plan permits one
   only when it is built from node-constant facts and falls back to the shared
   body for anything undecided. It must not use stale `checkers`/`pinned`: the
   context is constructed from `Counts`, so any fast path has to derive its own
   sound node facts (for example a single commoner-attacked query and the
   blocker set for enemy sliders) or be omitted. If the measurement does not
   show a win, the report records that and the path is not kept.

6. **Ship the public context only if it earns its surface.** W7 is P1, but a
   new public type that is no faster than `Board::legal` would be API debt. The
   plan's measurement gate is:
   - exactness on the differential corpus (mandatory);
   - no regression above 2 % on the existing `Board::legal`/perft paths;
   - a measured per-candidate improvement over `Board::legal` with a populated
     state on the hotpath bench.
   If the last condition fails, the report keeps W7 open and does not claim the
   ≤ 100/≤ 120 instruction target. The internal `LegalNode` refactor may remain
   if it is neutral and improves maintainability, or be reverted with the
   public context.

7. **Instruction counts are diagnostic.** The plan uses `perf stat` when
   available and reports the command, batch size, and per-call result. The
   same-machine ns/ratio is the portable gate; the wishlist's absolute
   instruction numbers are compared in the report, not treated as a
   cross-machine promise.

## Proposed API

```rust
// src/board.rs

/// Node-local cached data for legality checks.
///
/// Build once per position from [`Board::legality_context`] and ask
/// [`legal`](Self::legal) for every pseudo-legal candidate. The snapshot is
/// valid until the board changes; a stale context answers for the old
/// placement, exactly as a stale [`StateInfo`] does.
///
/// This is not a position representation and does not generate moves. It
/// exposes the occupancy it computed so a child/effect query can reuse it.
#[derive(Debug, Clone, Copy)]
pub struct LegalityContext {
    node: LegalNode, // crate-private
    counts: Counts,
}

impl LegalityContext {
    /// The node's occupied squares.
    #[inline]
    #[must_use]
    pub fn occupied(&self) -> Bitboard;

    /// Whether the pseudo-legal move `m` is legal.
    ///
    /// Exactly `board.legal(m, &populated_state)` for the board and state this
    /// context was built from. `m` must be pseudo-legal; malformed input is
    /// outside the contract, like [`Board::legal`].
    #[must_use]
    pub fn legal(&self, m: Move) -> bool;
}

impl Board {
    /// Build a legality context from the board and its commoner counts.
    ///
    /// `counts` must be current for this board; use [`Board::counts`] or
    /// [`Board::populate_counts`] plus [`Counts::from`]. The context reads no
    /// `checkers`/`pinned` fields and populates no [`StateInfo`].
    #[must_use]
    pub fn legality_context(&self, counts: Counts) -> LegalityContext;
}
```

Crate-private shape:

```rust
// src/position.rs

pub(crate) struct LegalNode {
    by_type: [Bitboard; PieceType::NB],
    side_to_move: Color,
    occupied: Bitboard,
    our_pieces: Bitboard,
    their_pieces: Bitboard,
    our_commoners: Bitboard,
    their_commoners: Bitboard,
}

impl LegalNode {
    pub(crate) fn from_view(view: &PositionView) -> Self;
    pub(crate) fn legal(&self, m: Move) -> bool;
}

pub(crate) fn legal_with_counts(view: &PositionView, m: Move) -> bool {
    LegalNode::from_view(view).legal(m)
}
```

The exact internal split may differ if the benchmark shows that
`from_view`-per-call is not free on the existing `Board::legal` path; the
public API and the single-body requirement do not change.

## Implementation

1. **Extract the node-constant data.** Move the body of
   `position::legal_with_counts` into `LegalNode::legal`, replacing
   `view.pieces_pt`, `view.pieces_color`, `view.commoners`, and `view.occupied`
   calls with the stored fields. Keep every branch and evaluation order that
   affects the answer. The existing `legal_with_counts` becomes a
   `LegalNode::from_view(view).legal(m)` call, or a free-function call with the
   same arguments if that avoids a board-path regression.

2. **Handle non-castling first, castling last.** The current body checks
   castling early and then has a second castling branch. Restructure only if it
   keeps one body and passes the differential:
   - compute `from`, `to`, `is_capture` for all moves;
   - if `m.move_type() != MoveType::Castling`, run the common path (EP victim,
     blast, commoner survival, attack query);
   - otherwise run the pass-through and rook relocation branch.
   Do not add the reverted category helpers.

3. **Add `Board::legality_context`.** Snapshot
   `LegalNode::from_view(self.position_view())`, store `counts`, and return the
   context. `occupied()` returns `node.occupied`. The constructor must not
   call `populate_state`, `checkers`, or `pinned`.

4. **Wire the game-over guard.** `LegalityContext::legal` returns `false` when
   either count is zero, exactly like `legal_in_view`. A zero count is the only
   state field beyond placement that the context needs.

5. **Optional fast path.** Only after the baseline context is measured:
   - add one node-constant quiet test that is proven equivalent, or
   - stop. The report must say which.
   If added, it must be a leaf that accepts only moves the full body would
   accept, and every other move must fall through to the shared body. The fast
   path cannot read stale `checkers`/`pinned`; it must derive the facts it uses
   at context construction. Any extra per-node query is included in the setup
   row, not hidden in the per-candidate row.

6. **Document the contract.** In `LegalityContext`, `Board::legality_context`,
   the hot-path section of `src/lib.rs`, and the README:
   - the context is a snapshot;
   - `counts` must be current;
   - `legal` assumes pseudo-legal input, like `Board::legal`;
   - the context is `Send + Sync`, `Copy`, and allocation-free;
   - `EffectContext`, `PostMove`, `is_legal`, and `post_move_and_key_with` are
     unchanged.

7. **Bench harness.** Extend `examples/hotpath_bench.rs` with a per-candidate
   section:
   - build once: `state`/`populate_state`, `counts`, `legality_context`;
   - per candidate: `Board::legal(m, &state)` versus `ctx.legal(m)`;
   - per class: quiet, capture, EP, castling, commoner move, and all;
   - `ctx.occupied()` is checked against `board.occupied()` outside the timed
     loop.
   The harness prints the number of calls so a `perf stat -e instructions:u`
   run can divide retired instructions by calls. Report the exact command.

## Tests

### Differential (the acceptance gate)

For every position of the 41-position perft corpus, the effect fixtures, the
conformance cases, the child cases, and the seeded play-outs:

- populate `full = StateInfo::new()` with `board.populate_state`;
- build `counts = board.counts()`;
- build `ctx = board.legality_context(counts)`;
- assert `ctx.occupied() == board.occupied()`;
- for every pseudo-legal move:
  - assert `ctx.legal(m) == board.legal(m, &full)`;
  - assert `ctx.legal(m) == board.legal(m, &counts_state)` where
    `counts_state` is the plan-1 counts-only state;
  - assert `ctx.legal(m) == board.is_legal_with_state(m, &counts_state)` for
    pseudo-legal `m`;
- count and report positions, candidates, legal candidates, and positions with
  checks/pins/EP/castling so the report shows the expensive branches were
  exercised.

### Edge cases and contract

- Terminal positions (either count zero): every candidate is `false`.
- A board with a commoner-only endgame, a touching-commoners position, a
  double-check-like slider position, and an EP pin fixture.
- `Move::NONE` and an empty origin: document that the context, like
  `Board::legal`, is for pseudo-legal moves; assert the result is `false` if
  the existing body already has that behavior, or leave it out of the contract
  and do not test undefined input.
- The board is unchanged after building and querying the context: compare
  `fen()`, `hash()`, `ep_square()`, and `rule50()` before and after.
- `assert_send_sync::<LegalityContext>()` in `src/lib.rs`'s test module.

### Generator and key regression

- `generate_legal_with_state` and the legal list still equal a filter of the
  pseudo-legal stream by `board.legal(m, &full)`. The existing tests already
  cover this; add a direct test that `LegalityContext::legal` is not used by
  the generator (it takes a state path and must stay independent).
- Perft values are unchanged. A refactor in `position::legal_with_counts` is
  on the generator path, so run the full perft verifier, not just the unit
  differential.

## Benchmark and gate

Rows, all on the same run and same host:

| Row | Gate |
| --- | --- |
| `ctx.legal` vs `Board::legal` on quiet candidates | report ratio; expected win |
| `ctx.legal` vs `Board::legal` on all candidates | report ratio and absolute ns |
| `ctx.legal` vs `Board::legal` on captures/EP/castling | report separately; no class may silently regress > 2 % |
| context construction | report ns per node; must be amortised by the candidate loop |
| `perf stat -e instructions:u` per `ctx.legal` | compare with the wishlist's 144 → ≤ 100 target; report if ≤ 120 or not |
| perft (`--release`, three medians) | no regression > 2 % |
| `effect_bench` existing rows | no regression > 2 % |

If the context is slower or neutral on the all-candidate row, the report says
so and does not ship the public type. If the quiet row wins but the all row
does not, the report may keep only an internal node type and leave W7 open.
The plan does not claim a whole-binary `core` saving.

## Files to touch

- `src/position.rs` — `LegalNode` extraction, shared body, unit tests.
- `src/board.rs` — `LegalityContext`, `Board::legality_context`, docs, unit
  tests.
- `src/lib.rs` — root re-export, hot-path docs, `Send + Sync` assertion.
- `tests/public_api.rs` — `LegalityContext` and `Board::legality_context`
  resolve when the plan ships.
- `tests/effects.rs` or a new `tests/legality_context.rs` — differential.
- `examples/hotpath_bench.rs` — per-candidate rows and setup row.
- `README.md`, `CHANGELOG.md` — the context, its snapshot/`Counts` contract,
  and the measured status.
- `docs/plans/feedback/report_wishlist2_2.md` — final report.

## Non-goals

- No category-specialised legality split; it was measured and reverted.
- No change to `Board::legal`, the generator, move order, perft, FEN,
  terminality, keys, or `ZOBRIST_VERSION`.
- No `StateInfo` layout change and no lazy `populate_state`.
- No extension of `EffectContext`, `PostMove`, `post_move_and_key_with`, or
  `is_legal`.
- No public `PositionView`, no lifetime-carrying context, and no allocation.
- No promise of the wishlist's instruction ceiling on another compiler.

## Final task

Write `docs/plans/feedback/report_wishlist2_2.md` with:

- the measurement-gate verdict and whether the public `LegalityContext`
  shipped, stayed internal, or was reverted;
- the exact API as implemented;
- differential counts and the branch coverage counts (checks, pins, EP,
  castling, terminal);
- the benchmark table (absolute ns, ratios, optional instructions per call),
  including the `perf stat` command;
- perft/effect_bench regression results;
- changed files, problems, deviations with measurements, unresolved parts,
  missing tests, and next steps for W7 and plan 3.
