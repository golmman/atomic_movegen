# Plan 4 — node-constant analysis view (W4)

## Scope

A measurement-gated plan for a small, read-only `AnalysisContext`:

- **W4:** expose one stable node-constant view of the facts a guide/prover
  derives once per node: checkers, pins, check squares by role, blockers between
  a square and a colour's sliders, pieces beside a commoner, a colour's attack
  map, and uncovering pieces for a target.
- **Gate first:** the wishlist is explicit that `Mover::new`'s profile share must
  be measured before this is requested. Upstream cannot measure the
  downstream's `Mover::new`. This plan therefore starts with a gate owned by the
  downstream and **does not start implementation until the gate passes**.
- If the gate passes, the implementation is additive and public:
  `AnalysisContext` in a new `src/analysis.rs` (or a carefully named module) with
  exact, documented masks. If the gate fails, the plan closes as a watchlist
  entry and no code is written.

Requirements served: wishlist 1 W4 (P3). The wishlist's acceptance is equality
with the prover's `Mover` fields on random play-outs; if `Mover::new` is small,
the item stays a watchlist entry.

## Current-tree assessment

- `src/board.rs` already exposes the individual facts:
  - `Board::checkers()` → enemy pieces checking the side to move;
  - `Board::pinned(c)` → pieces of `c` pinned to their commoner;
  - `Board::attackers_to(sq, occupied)` and
    `Board::attackers_to_color(sq, color, occupied)`;
  - `Board::populate_state(&mut StateInfo)` stores `checkers`, `pinned`, and the
    two commoner counts;
  - `Board::commoners(c)`, `piece_on`, per-colour/per-type bitboards.
- `src/position.rs` has the view-side implementations:
  - `PositionView::checkers(us)` and `PositionView::pinned(us)`;
  - `PositionView::populate_state`/`populate_state_with_checkers`;
  - the shared attack tables.
- `src/movegen.rs`/`src/board.rs` already have node-constant legality ladders
  (`TrivialLegality`, `WidenedLegality`, `FastLegality`) that derive some of
  these masks per node, but they are crate-private and shaped for the legal
  move generator, not for a consumer's guide.
- There is no public type that groups the masks, no `check_squares` by role, no
  generic `blockers_between(target, colour)`, no `pieces_beside_commoner`, no
  public `attack_map(colour)`, and no generic `uncovering_pieces(target,
  colour)`.
- The downstream's `Mover::new` derives pins, check squares, uncovering pieces,
  pieces beside the enemy commoner, attack maps, and exposure targets once per
  node. Reimplementing those facts in the adapter is the work this ask would
  remove; the profile share decides whether that work is material.

## Measurement gate (first task, no code until it passes)

The upstream cannot answer "how much of the prover's wall is `Mover::new`?"
This plan therefore has an explicit gate:

1. The downstream provides, for the same bench corpus it used for handoff 3:
   - `Mover::new`'s share of per-expansion wall (absolute ns and percentage),
     measured against the same run's expansion total;
   - the field-by-field mapping from `Mover` fields to the proposed
     `AnalysisContext` accessors, including whether each field is derived from
     placement, `StateInfo`, or both;
   - one or more representative positions and a random play-out seed set so the
     upstream differential can be reproduced.
2. The owner records a materiality threshold in the plan report **before**
   implementation. Default recommendation: implement only if `Mover::new` is at
   least 5 % of per-expansion wall **and** the projected removal is at least 1 %
   of total wall after the adapter drops `Mover`. If the downstream chooses a
   different threshold, record it and use it.
3. If the share is below the threshold, close the plan: write
   `report_wishlist1_4.md` with the number, the watchlist verdict, and no code.
   This is the wishlist's stated outcome, not a failure.

Do not build an `AnalysisContext` first and look for a consumer later. The API
shape below is a proposal for the gate-passing case; it is not an authorization
to implement.

## Proposed API (only after the gate passes)

```rust
// src/analysis.rs (new module, re-exported from the crate root)

/// A read-only, node-constant view of the placement facts a guide derives once
/// per position.
///
/// The context stores a snapshot of the position's placement, side to move,
/// castling rights, en-passant target and clock, plus the node-constant
/// `checkers`/`pinned` masks. It holds no reference to a `Board`, no `StateInfo`
/// internals, and no search concept. Build it once per node and ask it for
/// masks; it never mutates the board.
#[derive(Debug, Clone, Copy)]
pub struct AnalysisContext {
    view: PositionView,
    checkers: Bitboard,
    pinned: [Bitboard; Color::NB],
    commoners: [Bitboard; Color::NB],
}

impl AnalysisContext {
    /// Snapshot `board`, computing the node-constant masks once.
    #[must_use]
    pub fn new(board: &Board) -> Self;

    /// Snapshot `board`, reusing the `checkers`/`pinned`/counts already in a
    /// populated `StateInfo`.
    ///
    /// `state` must have been populated for `board`; a stale state yields the
    /// same facts a stale `Board::legal`/`checkers` path would yield, exactly
    /// like the rest of the crate.
    #[must_use]
    pub fn with_state(board: &Board, state: &StateInfo) -> Self;

    /// The enemy pieces checking the side to move (empty when either side has
    /// no commoner or the commoners touch).
    #[must_use]
    pub fn checkers(&self) -> Bitboard;

    /// The pieces of `color` pinned to one of their commoners.
    #[must_use]
    pub fn pinned(&self, color: Color) -> Bitboard;

    /// The squares from which a piece of `role` and `attacker` would check the
    /// opposing commoner in this placement.
    ///
    /// This is a geometry mask, not a promise that such a piece exists. Empty
    /// when either side has no commoner, and empty for every role when the two
    /// commoners touch (touching makes the commoner immune to every attack).
    #[must_use]
    pub fn check_squares(&self, attacker: Color, role: PieceType) -> Bitboard;

    /// The occupied squares between `target` and the sliders of `slider_color`
    /// that are aligned with `target`.
    ///
    /// This is the set a move from `target`'s perspective may uncover; it does
    /// not require a single blocker. Empty when `target == Square::NONE`.
    #[must_use]
    pub fn blockers_between(&self, target: Square, slider_color: Color) -> Bitboard;

    /// The occupied squares adjacent to the commoner of `commoner_color`.
    ///
    /// Empty when that side has no commoner. Intersect with
    /// `pieces_color(piece_color)` for the attacker-side subset; no second
    /// colour parameter is needed for the common "pieces beside the enemy
    /// commoner" query.
    #[must_use]
    pub fn pieces_beside_commoner(&self, commoner_color: Color) -> Bitboard;

    /// The union of the squares attacked by every piece of `color` in this
    /// placement.
    ///
    /// Sliders stop at the first occupied square (inclusive), pawns use their
    /// forward attack set, and commoners use one-square attacks. The map is a
    /// geometry fact; it is not filtered for own pieces or for commoner
    /// immunity.
    #[must_use]
    pub fn attack_map(&self, color: Color) -> Bitboard;

    /// The pieces of `piece_color` that are a sole blocker between `target` and
    /// an enemy slider of the opposite colour, i.e. the pieces whose departure
    /// would uncover an attack on `target`.
    ///
    /// Empty when `target == Square::NONE`. Multi-commoner positions are
    /// treated by geometry; no rule verdict is attached.
    #[must_use]
    pub fn uncovering_pieces(&self, target: Square, piece_color: Color) -> Bitboard;
}
```

The exact method names are provisional until the downstream provides its
`Mover` mapping. If a mapping shows that a different signature is needed (for
example `check_squares(&self, role, attacker)`, or a combined
`beside_enemy_commoner(&self, color)`), adjust names before implementation and
record the mapping in the report. Do not expose both spellings permanently.

## Definitions to pin before coding

The following definitions make the masks testable. They are stated now so an
implementation cannot drift into a search-specific interpretation.

1. **Commoner square.** Legal atomic positions have at most one commoner per
   side. `AnalysisContext` stores the commoner bitboard per colour. Accessors
   that need a single square use `commoners(color).lsb()` and return `EMPTY`
   when the bitboard is empty. Multi-commoner boards (unreachable) use the same
   lowest-square rule as `Board::legal`; no new policy is introduced.

2. **Check squares by role.** Return `EMPTY` when either side has no commoner
   or when the two commoners currently touch (touching makes the commoner
   immune to every attack, so no square checks it). Otherwise, for
   `target = commoners(attacker.flip()).lsb()`:
   - `Pawn`: `attacks::pawn_attacks(attacker, target)` (the origins from which a
     pawn of `attacker` attacks `target`);
   - `Knight`: `attacks::knight_attacks(target)`;
   - `Bishop`: `attacks::bishop_attacks(target, occupied)`;
   - `Rook`: `attacks::rook_attacks(target, occupied)`;
   - `Queen`: `attacks::bishop_attacks(target, occupied)
     | attacks::rook_attacks(target, occupied)`;
   - `Commoner`: empty. A commoner that stands on a square attacking the
     opposing commoner touches it, and touching commoners do not check; there
     is no "check square" for the commoner role.

   The non-commoner mask includes squares currently occupied by any piece; it
   is a "would attack from here" mask. The commoner mask is empty by rule, not
   by a missing table.

3. **Blockers between.** For `target` and `slider_color`, iterate the colour's
   rooks, bishops and queens that are aligned with `target` on a rook/bishop
   ray. Return the union of `between_bb(target, slider_sq) & occupied` over
   those sliders. This includes every blocker, not just a sole blocker; the
   caller can test `count() == 1` (or use `uncovering_pieces`). Return `EMPTY`
   for `Square::NONE` or when the target is not a real square.

4. **Pieces beside a commoner.** `attacks::king_attacks(commoner_sq) &
   occupied`. The result includes both colours and excludes the commoner's own
   square. Empty when that colour has no commoner.

5. **Attack map.** For each piece of `color`:
   - pawn: `attacks::pawn_attacks(color, from)`;
   - knight: `attacks::knight_attacks(from)`;
   - bishop: `attacks::bishop_attacks(from, occupied)`;
   - rook: `attacks::rook_attacks(from, occupied)`;
   - queen: the union of the bishop and rook sets;
   - commoner: `attacks::king_attacks(from)`.
   Union them. Do not remove squares occupied by the same colour. Empty for a
   colour with no pieces.

6. **Uncovering pieces.** For each enemy slider of `piece_color.flip()` aligned
   with `target`, compute `between = between_bb(target, slider_sq) & occupied`.
   If `between.count() == 1`, union `between & pieces_color(piece_color)`.
   Return only pieces of `piece_color`. This is the generic "the move of this
   piece opens a line to `target`" set; it is not a legality verdict and does
   not handle commoner immunity.

7. **No `StateInfo` mutation and no board write.** `AnalysisContext::with_state`
   reads the state; it does not modify it. Every query only reads the snapshot.
   The test suite compares the board FEN/hash before and after.

## Implementation (if the gate passes)

1. **Create the context.** Add `src/analysis.rs`; re-export `AnalysisContext`
   at the crate root. Store the `PositionView` snapshot, `checkers`,
   `pinned[2]`, and `commoners[2]`. `new` uses
   `PositionView::of(board)` and `view.populate_state`-equivalent masks;
   `with_state` copies the three state fields after a `debug_assert` or a
   documented precondition that `state` belongs to `board`. Do not expose
   `StateInfo` fields that are not part of the documented snapshot.

2. **Implement the masks from the definitions above.** Prefer the existing
   `PositionView`, `attacks`, and `between_bb` helpers. Do not duplicate the
   `pinned` or `checkers` algorithms; call the view methods. `check_squares`
   and `blockers_between` are new geometry, not new legality.

3. **Keep the hot paths untouched.** Do not change `Board::legal`,
   `populate_state`, `WidenedLegality`, `TrivialLegality`, `FastLegality`, the
   generator, or the effect path to call `AnalysisContext`. This type is an
   additive consumer view; it must be possible to delete it without changing any
   existing answer.

4. **Docs.** Each method documents:
   - whether it is a geometry mask or a rules verdict;
   - the commoner-touching rule where applicable;
   - `EMPTY`/`None` conditions;
   - that the context is a snapshot, not a live board;
   - that it is not a search or evaluation type.
   Add a doctest that builds a context from a small board and checks a known
   mask.

5. **Benchmark.** Add `examples/analysis_bench.rs` (or a section in
   `effect_bench`) that measures, on the perft corpus and seeded play-outs:
   - `AnalysisContext::new` vs `Board::populate_state`;
   - `with_state` vs `populate_state` (the reuse case);
   - each accessor vs its recomputation from `Board`/`PositionView`;
   - the projected `Mover`-equivalent composite (all masks) vs the sum of the
     individual current queries.

   The point is to tell the downstream whether the view is cheaper than
   recomputing and whether `Mover::new`'s share is plausibly removable. No
   claim about the downstream's tree.

## Tests (if the gate passes)

### Differential against existing facts

For every position of the 41-position perft corpus, the effect fixtures, and
seeded play-outs:

- `ctx.checkers() == board.checkers()` and equals the populated
  `StateInfo.checkers`;
- `ctx.pinned(c) == board.pinned(c)` for both colours;
- `ctx.pieces_beside_commoner(c)` equals
  `attacks::king_attacks(board.commoners(c).lsb()) & board.occupied()` (and is
  empty when the commoner is absent);
- `ctx.check_squares(attacker, role)` intersected with the actual pieces of
  that colour/role equals `board.checkers()` intersected with that set for the
  commoner that is actually checking (for each role); the full mask is checked
  against a brute-force geometric reference;
- `ctx.blockers_between(target, color)` equals a brute-force scan of aligned
  sliders and `between_bb`;
- `ctx.attack_map(color)` equals a brute-force union over the colour's pieces
  using the same attack functions;
- `ctx.uncovering_pieces(target, color)` equals a brute-force "remove the piece
  from occupancy and see whether an enemy slider newly attacks `target`" scan;
- all queries leave the board FEN/hash unchanged.

### Downstream field mapping

The plan cannot assert equality with the prover's private `Mover` fields from
inside this repository. The report must include a table mapping each `Mover`
field to the corresponding `AnalysisContext` accessor and the differential
result the downstream ran on its own random play-outs. If a field has no exact
counterpart, list it as unresolved; do not widen an accessor silently to make
the table look complete.

### Cost/differential gates

- `AnalysisContext::new` must not be more expensive than
  `populate_state` + the individual queries it replaces on the same corpus; if
  it is, the API is not worth shipping in this shape. Record the ratio.
- Accessors must be exact; cost is reported per accessor and for the composite.
- No existing path may call the new type, so perft and movegen timings must be
  unchanged (guard >2 %).
- `cargo test --features unsafe-optimizations` if the implementation reads the
  attack tables in a new way.

## Verification

All global gates of the index, plus:

```sh
cargo test --test public_api
cargo test --features unsafe-optimizations
cargo run --release --example effect_bench 9 2000   # regression guard
cargo run --release --example verify_perft 5
```

Acceptance for the gate-passing case:

- every mask equals the existing public fact or its brute-force definition on
  the corpus and random play-outs;
- the downstream's `Mover` mapping is green on its random play-outs;
- `AnalysisContext::new` is materially cheaper than the current per-node
  derivation, or the report says it is not and the API is not shipped;
- no existing answer, key, move order, perft value, or hot-path timing changes.

## Files to touch (if the gate passes)

- `src/analysis.rs` — new `AnalysisContext`, docs, unit tests.
- `src/lib.rs` — module and re-export.
- `examples/analysis_bench.rs` — measurement harness (or an
  `effect_bench` section, coordinated with plans 1 and 3).
- `tests/public_api.rs` — pin the new public type only if it ships.
- `tests/analysis.rs` (or unit tests in `analysis.rs`) — corpus/play-out
  differential.
- `CHANGELOG.md`, `README.md` — API and snapshot contract.
- `docs/plans/feedback/report_wishlist1_4.md` — final report, including the
  gate verdict.

## Non-goals

- No implementation before the downstream measurement gate passes.
- No change to `StateInfo`, `PositionView`, `WidenedLegality`,
  `TrivialLegality`, `FastLegality`, `Board::legal`, `populate_state`, the
  generator, or the effect path.
- No mutable state, no search/evaluation concept, no tablebase or guide
  integration inside the crate.
- No guarantee that the API matches every private `Mover` field; the report
  lists unresolved fields instead of inventing semantics.
- No new dependency, no `unsafe`, no second position representation.

## Final task

Write `docs/plans/feedback/report_wishlist1_4.md` with:

1. the gate verdict first: the measured `Mover::new` share, the threshold used,
   and whether implementation proceeded;
2. if it proceeded: the exact public API, the field-mapping table, differential
   counts (positions, masks, mismatches), the benchmark table (absolute ns and
   ratios), changed files, problems, deviations with measurements, unresolved
   fields, missing tests and next steps;
3. if it did not proceed: the measurement, the watchlist decision, and no code
   changes beyond the report.
