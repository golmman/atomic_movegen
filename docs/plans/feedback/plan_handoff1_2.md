# Plan 2 — Effect context and per-move effects (MG3, MG5)

## Scope

Implement the flagship request: a position-constant effect context plus
read-only per-move effect queries, so a consumer can classify a candidate
without writing a child board. This plan also subsumes **MG5** (node-constant
attack data) and adds the legal early-exit stream (`for_each_legal`) that the
handoff left open in MG2.

The plan keeps one implementation of the rules: the new effect path must be
differentially identical to [`Board::legal`] and to a cloned `do_move` child for
every pseudo-legal move. The existing `Board::legal` remains the oracle and the
fallback for any case the effect path does not decide.

## Current-tree assessment

- `Board::legal` is already read-only and takes a populated `StateInfo`, but it
  recomputes occupancy, the blast set, and the commoner attack test per move.
- `Board::blast_casualties` already computes the pieces a capture removes
  without mutating the board.
- `TrivialLegality` is a node-constant fast path for *quiet, non-capture,
  non-check, non-pinned* moves only.
- `has_legal_move_with_state` already drives the internal
  `for_each_pseudo_legal` stream and early-exits through `Board::legal`.
- There is no public classification of “checks the enemy commoner”, “explodes
  the enemy commoner”, or “leaves my commoner alive”, and no legal stream.
- Plan 1 supplies public `between`/x-ray helpers, the public pseudo-legal
  stream, and the `StateInfo` root re-export.

## Decisions and pushback

1. **Option A is the primary API; option B is layered on top.** Expose a public
   `EffectContext` and `MoveEffect` (A), then add a convenience
   `Board::outcome_after` / `EffectContext::outcome_after` (B). Do not expose a
   rules-only “sealed outcome” as the only API; the consumer needs the
   classification to build on.
2. **Every field is derivable from the rules alone.** No field may encode a
   search design (no “PV”, “threat”, “guide”, or “prover” concepts). The fields
   are exactly: capture, blast casualties, checks the enemy commoner, explodes
   the enemy commoner, leaves the mover with a commoner, and legal.
3. **`EffectContext` is immutable and copyable.** It holds a snapshot of the
   node-constant data (`StateInfo` fields plus the piece bitboards and commoner
   squares). Build it once per position; do not make it own a `Board`.
4. **`MoveEffect` is a value, not a mutator.** `EffectContext::effect(&board, m)`
   takes `&Board` and `Move`, returns `MoveEffect`, and never calls `do_move`.
   `m` must be pseudo-legal; malformed input is handled by
   `Board::is_legal`/`is_legal_with_state`, not by the effect path.
5. **The effect path is validated, not trusted.** `EffectContext::is_legal`
   uses the effect for normal/promotion/en-passant moves and falls back to
   `Board::legal` for castling (pass-through squares are a different rule
   surface). Any future fast path must keep the fallback until the differential
   proves it unnecessary.
6. **`outcome_after` has a documented fallback.** It uses the effect for
   immediate extinction and check/mate classification and falls back to a
   cloned `do_move` + `outcome()` where stalemate or a clock decision is
   possible. This is option B's “fall back only where stalemate is possible”
   shape; remove the fallback only if a later child-view generator makes the
   no-board-write path complete. The contract is exact equality with
   `clone.do_move(m).outcome()` for every legal `m`.
7. **`for_each_legal` is part of this plan.** It is the legal counterpart of
   the public pseudo stream and uses `EffectContext::is_legal` with the
   `TrivialLegality` quiet fast path retained as a first check.
8. **Do not change `Board::legal` semantics or `StateInfo` layout** in this
   plan. Effect code may share private helpers with `legal`, but the public
   oracle stays stable.

## Proposed API

### `src/effects.rs` (new module)

```rust
/// Node-constant data for classifying moves without mutating the board.
///
/// Build once per position with [`Board::effect_context`] or
/// [`Board::effect_context_with_state`], then call [`effect`](Self::effect) or
/// [`is_legal`](Self::is_legal) for each pseudo-legal candidate.
#[derive(Debug, Clone, Copy)]
pub struct EffectContext {
    us: Color,
    them: Color,
    occupied: Bitboard,
    by_color: [Bitboard; 2],
    by_type: [Bitboard; 6],
    our_commoner: Bitboard,
    their_commoner: Bitboard,
    checkers: Bitboard,
    pinned: Bitboard,
    commoners_count: u32,
    them_commoners_count: u32,
    /// Copy of the populated state, used only for the castling fallback to
    /// `Board::legal`.
    state: StateInfo,
}

/// The rules effects of one pseudo-legal move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveEffect {
    /// The move removes a piece (normal capture or en passant).
    pub is_capture: bool,
    /// Every piece removed by the move's blast, including the capturer at
    /// ground zero and the en-passant victim.
    pub casualties: BlastCasualties,
    /// The move leaves the enemy commoner attacked by the mover's pieces,
    /// after the move and blast. False when the enemy commoner is gone or the
    /// two commoners touch (touching grants immunity).
    pub checks_enemy: bool,
    /// The move's blast removes the enemy commoner.
    pub explodes_enemy_commoner: bool,
    /// The mover still has at least one commoner after the move and blast.
    pub leaves_own_commoner: bool,
    /// Equivalent to [`Board::legal`] for a pseudo-legal move.
    pub is_legal: bool,
}

impl EffectContext {
    /// Build from a populated [`StateInfo`].
    pub fn new(board: &Board, state: &StateInfo) -> Self;

    /// Classify `m` without mutating `board`.
    pub fn effect(&self, board: &Board, m: Move) -> MoveEffect;

    /// Fast legality query; equal to `board.legal(m, &state)` for a
    /// pseudo-legal `m` and the same populated state.
    pub fn is_legal(&self, board: &Board, m: Move) -> bool;

    /// Child outcome from the child side-to-move perspective.
    ///
    /// Exactly `{ let mut b = board.clone(); b.do_move(m, &mut st); b.outcome() }`
    /// for every legal `m`. Uses the effect for immediate extinction/check
    /// decisions and a documented cloned-board fallback for stalemate/clock
    /// cases.
    pub fn outcome_after(&self, board: &Board, m: Move) -> Option<Outcome>;
}

impl Board {
    /// Populate a temporary [`StateInfo`] and build an [`EffectContext`].
    pub fn effect_context(&self) -> EffectContext;

    /// Build an [`EffectContext`] from a caller-populated [`StateInfo`].
    pub fn effect_context_with_state(&self, state: &StateInfo) -> EffectContext;

    /// Convenience wrapper for [`EffectContext::outcome_after`].
    pub fn outcome_after(&self, m: Move, state: &StateInfo) -> Option<Outcome>;
}
```

Re-export `EffectContext` and `MoveEffect` from the crate root. Add
`pub mod effects;` to `lib.rs`.

### `src/movegen.rs` — legal stream

```rust
/// Walk the legal move stream for the side to move, stopping early if `f`
/// returns [`ControlFlow::Break`].
///
/// Visits candidates in the same order as [`generate_legal`] and
/// [`generate_pseudo_legal`]. Calls `populate_state` internally.
pub fn for_each_legal<B>(
    board: &Board,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B>;

/// Same as [`for_each_legal`] with a caller-populated [`StateInfo`].
pub fn for_each_legal_with_state<B>(
    board: &Board,
    state: &StateInfo,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B>;
```

`generate_legal_with_state` may be refactored to use the same stream with a
collecting closure, but its output order and contents must not change.

## Implementation

### 1. Post-move bitboards without a board write

Compute a private `PostMove` view inside `EffectContext::effect`:

1. Look up the moving piece and piece type. Debug-assert that `m` is
   pseudo-legal.
2. Start from `self.occupied`, `self.by_color`, `self.by_type`.
3. Compute `casualties = board.blast_casualties(m)` for captures; default for
   quiet moves, promotions, and castling.
4. Remove `casualties.all()` from every bitboard.
5. Apply the move to the post-blast bitboards:
   - **Castling**: move the commoner from its home square to the castled
     square and the rook from its home square to the castling-destination
     square. No capture, no blast.
   - **En passant**: the captured pawn and the capturer are both in
     `casualties`, so nothing is added back.
   - **Capture (normal or promotion)**: the capturer is at ground zero and is
     in `casualties`, so nothing is added back.
   - **Quiet normal move**: remove the mover from `from`, add it to `to`.
   - **Quiet promotion**: remove the pawn from `from`, add the promoted piece
     type to `to`.
6. Derive:
   - `our_commoner_after = our_commoner & !casualties.all()` plus the castled
     commoner square for castling, or the moved commoner square for a quiet
     commoner move.
   - `their_commoner_after = their_commoner & !casualties.all()`.
   - `leaves_own_commoner = !our_commoner_after.is_empty()`.
   - `explodes_enemy_commoner = !their_commoner.is_empty()
      && their_commoner_after.is_empty()`.
7. `checks_enemy`:
   - false if `their_commoner_after` is empty;
   - false if the two commoners touch (`our_commoner_after` is adjacent to
     `their_commoner_after`), because touching grants immunity;
   - otherwise true iff `attackers_to_sets(their_commoner_sq, post_occupied,
     post_by_type, post_by_color) & post_us_pieces` is non-empty.

Add a private `attackers_to_sets` helper in `effects.rs` that mirrors
`Board::attackers_to` but takes the post-move bitboards. Do not duplicate the
rule; factor the existing `Board::attackers_to` body through the same helper if
that keeps it readable.

### 2. Effect-based legality

`EffectContext::is_legal`:

- Debug-assert pseudo-legality; callers that need malformed-input safety use
  `Board::is_legal_with_state`.
- For castling, return `board.legal(m, &self.state)` (the context stores a
  copy of the populated `StateInfo` solely for this fallback).
- Otherwise compute the effect and decide:
  1. no own commoner after the move → false;
  2. enemy commoner exploded → true (the mover's commoner cannot be in check
     after the game has ended);
  3. if the post-move commoners touch → true (immunity);
  4. otherwise false iff the mover's commoner is attacked by an enemy piece on
     the post-move occupancy and piece sets;
  5. otherwise true.

This must be exactly the existing `Board::legal` ladder. Keep `Board::legal` as
the fallback for any branch the effect cannot decide, and as the test oracle.

### 3. `MoveEffect` construction

`effect()` fills `is_capture` from `board.is_capture(m)`, `casualties` from
`board.blast_casualties(m)`, and the derived fields above. It sets
`is_legal = self.is_legal(board, m)`.

### 4. `outcome_after`

Use the effect to answer the immediate cases:

- if `explodes_enemy_commoner` → `Some(Outcome::Loss)` from the child side to
  move (the child has no commoner);
- if the move is illegal → return `None` (documented; callers should only pass
  legal moves);
- if the child is in check and the effect can prove no child legal move
  exists, return `Some(Outcome::Loss)`;
- otherwise fall back to a cloned `do_move` + `outcome()`.

The fallback is deliberately conservative. Document that it is an
implementation detail and that the contract is equality with
`clone.do_move(m).outcome()`. If a later child-view generator makes the
no-board-write path complete, remove the fallback in a separate measured plan.

### 5. `for_each_legal`

Use the same flow as `generate_legal_with_state`:

1. early-out on `commoners_count == 0 || them_commoners_count == 0`;
2. build `TrivialLegality` and `EffectContext` once per node;
3. walk `for_each_pseudo_legal`, accepting `trivial.is_trivially_legal(...) ||
   effect.is_legal(...)` and breaking on `Break`.

Keep the `TrivialLegality` quiet fast path first so quiet moves do not pay for
effect computation.

## Tests

All tests must cover the handoff's gate list: every pseudo-legal move over a
fixture corpus, three bench positions, and long random play-outs.

### Differential tests

- `effect_is_legal_matches_board_legal`: for every pseudo-legal move over
  `TRIVIAL_LEGALITY_CORPUS`, the order-golden positions, and seeded random
  play-outs, `effect.is_legal` equals `board.legal(m, &state)`.
- `effect_fields_match_child`: for every pseudo-legal move, compare:
  - `is_capture` with `board.is_capture(m)`;
  - `casualties` with `board.blast_casualties(m)`;
  - `checks_enemy` with `!child.checkers().is_empty()`;
  - `explodes_enemy_commoner` with `child.commoners(them).is_empty()` (where
    `them` is the parent side not to move);
  - `leaves_own_commoner` with `!child.commoners(us).is_empty()`;
  - `is_legal` with membership in `generate_legal_with_state`.
- `effect_is_legal_never_accepts_illegal`: a second, independent assertion that
  every effect-accepted move is in the generated legal list.
- `outcome_after_matches_do_move`: for every legal move, compare with
  `{ clone; do_move; outcome() }`. Include extinction, checkmate, stalemate,
  material draw, and 50-move fixtures.
- `for_each_legal_matches_generate_legal`: same order and contents over the
  corpus and play-outs.
- `for_each_legal_early_exit`: `Break` after the first legal move visits exactly
  one legal candidate; `Continue` visits the full list.

### Deterministic fixtures

Add cases for:

- quiet move, capture, en-passant capture, promotion with and without capture,
  castling both sides;
- a capture whose blast removes the enemy commoner;
- a capture whose blast removes the mover's commoner (illegal);
- a capture whose blast removes a blocker and discovers an attack on the
  mover's commoner;
- a move that checks the enemy commoner;
- a move that explodes a blocker and discovers a check;
- touching commoners (immunity);
- side to move in check with a single escape;
- no legal moves while attacked (loss) and while safe (stalemate);
- a terminal position with no commoner;
- a 50-move position where mate outranks the clock.

### Move order

The existing `tests/move_order_golden.rs` remains the order gate. Add stream
order assertions for pseudo and legal streams.

### Performance

- Perft depth 5 must be identical to the current tree on all 14 `perft_tests`
  positions (depth 6 on the 41-position `verify_perft` corpus when time
  permits).
- Microbenchmark on the handoff's `deep-win` FEN and two middlegame positions:
  `EffectContext::effect` + `is_legal` versus `do_move` + `legal` + `outcome`;
  `for_each_legal` versus `generate_legal_with_state`; `outcome_after` versus
  clone + `do_move` + `outcome`. Report medians, not a single run.
- Do not claim a speed-up unless the measurement is outside noise. The
  correctness contract is the release gate; the performance claim is separate.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (41/41; at least depth 5)
- `cargo test --test move_order_golden`
- `cargo test --test verify_moves`
- An external differential harness for the downstream's three bench positions
  and long random play-outs, if available. Record the harness command and the
  visited-position count.

## Files to touch

- `src/effects.rs` — new `EffectContext`, `MoveEffect`, helpers, tests.
- `src/board.rs` — `effect_context*`, `outcome_after`, any private helper
  shared with `legal`, docs.
- `src/movegen.rs` — `for_each_legal*`, refactor `generate_legal_with_state`
  to share the stream, docs, tests.
- `src/lib.rs` — module/re-exports and crate docs.
- `README.md` / `CHANGELOG.md` — effect API, legal stream, contracts.
- `docs/plans/feedback/report_handoff1_2.md` — final report.

## Non-goals

- No change to `Board::legal` semantics, `StateInfo` layout, move order, perft,
  hashes, or FEN output.
- No full child-view move generator in this plan; `outcome_after` may fall back
  to a cloned board where stalemate/clock decisions require it.
- No MG4 capture/check fast-path widening beyond what `EffectContext::is_legal`
  naturally provides (plan 3 measures that).
- No SAN, `Setup`, terminal-reason enum, or conformance corpus (plans 4–6).
- No new dependencies.

## Final task

Write `docs/plans/feedback/report_handoff1_2.md` with:

- the exact public API as implemented;
- the effect-vs-`Board::legal` and effect-vs-child differential results,
  including visited-position counts;
- the `outcome_after` fallback cases and why they could not be decided by the
  effect alone;
- perft/order/verify results;
- benchmark numbers and whether the effect path is actually faster;
- any rule case that required a new helper or a change to the plan.
