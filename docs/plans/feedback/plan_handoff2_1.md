# Plan 1 — child-view terminality (MG15)

## Scope

Implement the flagship of handoff 2: **one query that answers the child's
terminal state without a board write**, and that never lies.

```
EffectContext::terminal_after(&self, board: &Board, m: Move) -> Option<Terminal>
```

`None` means "the child is not decided"; `Some(Terminal { outcome, reason })`
is exactly `{ let mut c = board.clone(); c.do_move(m, &mut st); c.terminal() }`.
There is **no clone fallback**: the question "does the child have a legal
move?" is answered on a bitboard view of the child.

The plan also removes `outcome_after`'s clone fallback (it becomes
`terminal_after(..).map(|t| t.outcome)`), because the fallback is the entire
0.00 %-decided share handoff 2 measured.

Requirement served: R6 (outcome and terminality, child side). Priority P1.

## Current-tree assessment

- `Board` is the only position representation. Everything the rules read is
  behind `&Board`:
  `Board::legal(&self, m, state)`, `Board::populate_state`,
  `Board::compute_checkers`, `Board::compute_pinned`,
  `movegen::{for_each_pseudo_legal, generate_pseudo_legal,
  generate_legal_with_state, has_legal_move_with_state}`, `FastLegality`,
  `TrivialLegality`, `WidenedLegality`.
- `src/effects.rs` already derives the child **placement** without a board
  write (`analyze` → private `PostMove`: `occupied`, `by_color`, `by_type`,
  `casualties`, `is_capture`). It does *not* derive the child's side to move,
  en-passant square, castling rights or clock, so it cannot run a move
  generator on the child.
- `Board::do_move` is the only place that computes the child's
  ep/rights/clock. Two of those computations already exist as pure helpers
  reuse-ready: `castling_squares` (already `pub(crate)`),
  `castling_rights_after` (private) and `Board::rule50_after` (public).
- `Board::is_attacked_by(sq, occupied, by_type, attackers, sq_color)` is
  already placement-only (`pub(crate)`), and `Board::legal`'s
  `square_attacked_by` delegates to it, so the *attack* rule is not the
  blocker. The blocker is that generation and the ladder are `&Board`-typed.
- `Outcome`/`Terminal`/`TerminalReason` are public and frozen; the precedence
  lives in one place today (`Board::terminal_impl`).

## Decisions and pushback

1. **Introduce a crate-private `PositionView`, not a public one.** The view is
   the set of facts the rules read: `by_color`, `by_type`, `side_to_move`,
   `ep_square`, `castling_rights`, `rule50` (plus a cached `occupied`). It is
   `Copy`, lives in a new `src/position.rs` (`pub(crate) mod position;`), and
   is **not** exported: handoff 2 explicitly does not ask for a second public
   position representation, and `Board` stays the public one. MG17's public
   `PostMove` (plan 2) wraps it.
2. **One implementation of the rules, over the view.** `Board::legal`,
   `Board::populate_state`, `Board::compute_checkers`, `Board::compute_pinned`,
   `Board::is_material_draw` and every generator entry point become thin
   wrappers over view functions. No rule is written twice; the view is not a
   second rules engine, it is the same engine with a narrower input. This is
   the only shape that keeps the crate's "rules live in one place" invariant
   while answering questions about a position that is not on a board.
3. **Exactness for pseudo-legal input, all input.** `terminal_after` is
   defined by equality with the cloned child's `terminal()` for every
   *pseudo-legal* move, including pseudo-legal-but-illegal ones (a
   self-explosion child is terminal, and the query says so). It does not
   validate legality; the discriminating two-level answer is MG16 (plan 2), and
   the doc points there. Malformed input (`Move::NONE`, empty origin) returns
   `None`, matching `effect`'s documented malformed handling.
4. **Signature deviation (documented).** Handoff 2 proposes
   `terminal_after(&self, board, m, state)`. The context already owns the
   populated `StateInfo` it was built from (`state`), so the extra parameter is
   redundant; the implemented shape is
   `EffectContext::terminal_after(&self, board: &Board, m: Move)` — exactly the
   shape of `effect`/`is_legal`. For API symmetry with
   `Board::outcome_after(&self, m, state)` a `Board::terminal_after(&self, m,
   state)` wrapper is added too. Both are documented.
5. **The `EffectContext::state` copy can go.** Castling legality moves onto the
   view (`legal_in_view` already implements the pass-through rule from
   bitboards), so `is_legal_from` no longer needs `Board::legal`. The private
   field is deleted; `new(board, state)` keeps its signature and still uses
   `state.commoners_count`/`them_commoners_count` for the `game_over` flag so
   stale-state semantics are unchanged.
6. **No lazy-state micro-optimisation before measurement.** The child's
   `StateInfo` (checkers, pinned, counts) is populated once per query and fed to
   the shared precedence function. That keeps one precedence implementation.
   If the benchmark shows state population dominating, a lazy variant is a
   *separate*, measured change with its own differential — not part of this
   plan.
7. **Optional, only if free:** `is_terminal_after` (the literal
   `Board::is_terminal` mirror on the child, i.e. without the commoner-only
   material shortcut) and `EffectContext::has_legal_move_after(&self, board, m)
   -> Option<bool>`. Both are statements about positions and both fall out of
   the machinery; include them only if they add no branch to the flagship path
   and are differentially tested. Otherwise list them as next steps.

## Proposed API

```rust
// src/effects.rs (crate root re-exports already cover Terminal)
impl EffectContext {
    /// Terminal state of the position reached by `m`, from the child's
    /// side-to-move perspective, without building the child.
    ///
    /// `None` = the child is not decided (the game continues). For every
    /// pseudo-legal `m` this is exactly
    /// `{ let mut c = board.clone(); c.do_move(m, &mut st); c.terminal() }`,
    /// including the precedence extinction → material → no-moves → clock and
    /// the child's `rule50`. It does *not* reject illegal moves: a
    /// self-explosion child is terminal and is reported as such. Use
    /// `child_outcome` (2.7) or `is_legal` when the move's legality is the
    /// question. Malformed input returns `None`.
    pub fn terminal_after(&self, board: &Board, m: Move) -> Option<Terminal>;

    /// (optional) literal `Board::is_terminal` mirror on the child.
    pub fn is_terminal_after(&self, board: &Board, m: Move) -> Option<bool>;

    /// (optional) "does the child have a legal move?" without building it.
    pub fn has_legal_move_after(&self, board: &Board, m: Move) -> Option<bool>;
}

impl Board {
    /// Convenience wrapper: build an `EffectContext` for this position and ask
    /// `terminal_after`. Mirrors `Board::outcome_after(&self, m, state)`.
    pub fn terminal_after(&self, m: Move, state: &StateInfo) -> Option<Terminal>;
}
```

`outcome_after` keeps its exact signature and behaviour (documented `None` for
illegal *and* undecided) but loses the clone:

```rust
pub fn outcome_after(&self, board: &Board, m: Move) -> Option<Outcome> {
    // legality first (unchanged), then the terminality answer
    if !self.is_legal(board, m) { return None; }
    self.terminal_after(board, m).map(|t| t.outcome)
}
```

## Implementation

### 1. `src/position.rs` — the view

```rust
/// The parts of a position the rules read.
///
/// A `PositionView` is a *snapshot* of placement, side to move, en passant,
/// castling rights and the half-move clock. Hash and ply bookkeeping stay on
/// `Board`: no rule reads them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PositionView {
    by_color: [Bitboard; Color::NB],
    by_type: [Bitboard; PieceType::NB],
    occupied: Bitboard,
    side_to_move: Color,
    ep_square: Option<Square>,
    castling_rights: u8,
    rule50: u16,
}

impl PositionView {
    pub(crate) fn of(board: &Board) -> Self;
    // Same accessor names and semantics as `Board` (all `#[inline]`):
    pub(crate) fn occupied(&self) -> Bitboard;
    pub(crate) fn pieces_color(&self, c: Color) -> Bitboard;
    pub(crate) fn pieces_pt(&self, pt: PieceType) -> Bitboard;
    pub(crate) fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard;
    pub(crate) fn piece_on(&self, sq: Square) -> Piece;   // 12-set lookup
    pub(crate) fn empty(&self, sq: Square) -> bool;
    pub(crate) fn side_to_move(&self) -> Color;
    pub(crate) fn ep_square(&self) -> Option<Square>;
    pub(crate) fn castling_rights(&self) -> u8;
    pub(crate) fn rule50(&self) -> u16;
    pub(crate) fn commoners(&self, c: Color) -> Bitboard;

    // Rules that depend on placement only:
    pub(crate) fn checkers(&self, us: Color) -> Bitboard;      // = Board::compute_checkers
    pub(crate) fn pinned(&self, us: Color) -> Bitboard;        // = Board::compute_pinned
    pub(crate) fn populate_state(&self, state: &mut StateInfo);
    pub(crate) fn is_material_draw(&self) -> bool;
}
```

`PositionView::of` is the only "conversion"; `piece_on` scans the 12
color×type sets (no `[Piece; 64]` array — that would be a board write in
disguise).

### 2. Legality over the view

- Move the body of `Board::legal` to
  `position::legal_in_view(view: &PositionView, m: Move, state: &StateInfo) -> bool`
  verbatim (it reads only `commoners_count`/`them_commoners_count` from
  `state`). `Board::legal` becomes
  `legal_in_view(&PositionView::of(self), m, state)`.
- `Board::compute_checkers`/`compute_pinned`/`populate_state`/
  `is_material_draw` delegate to the view.
- `board::castling_rights_after` becomes `pub(crate)` and is the single
  implementation of the child's rights, used by both `do_move` and the view
  path.

### 3. Generation over the view

- `movegen::{for_each_pseudo_legal, generate_pseudo_legal, generate_legal*,
  for_each_legal*, has_legal_move*}` build one view and call `pub(crate)`
  siblings taking `&PositionView`:
  `for_each_pseudo_legal_in_view`, `generate_legal_in_view`,
  `has_legal_move_in_view`. The public functions keep their signatures and
  their documented contracts (order, early exit, game-over behaviour).
- `TrivialLegality`, `WidenedLegality`, `FastLegality` take `&PositionView`;
  their oracle fallback becomes `position::legal_in_view(view, m, state)`
  instead of `board.legal`. `TrivialLegality`'s `squares[dest_index()]` and
  `WidenedLegality`'s `squares[…]` become `view.piece_on(..)`;
  `compute_sniper_data` reads `view`.
- Nothing about generation order, prune order or the ladder's branch order
  changes. `tests/move_order_golden.rs` is the gate.

### 4. One terminal precedence

```rust
/// Shared by `Board::terminal_impl` and `EffectContext::terminal_after`.
pub(crate) fn terminal_in_view(
    view: &PositionView,
    state: &StateInfo,        // populated for `view`, from the view's side to move
    moves_exist: Option<bool>,
) -> Option<Terminal>;
```

`Board::terminal_impl` becomes a call with `PositionView::of(self)`. Branch
order and `Terminal` values are unchanged:
extinction (`commoners_count == 0` → `Loss`) → opponent extinction
(`them_commoners_count == 0` → `Win`) → `is_material_draw` → clock with empty
`checkers` → `has_legal_move_in_view` (false → `Stalemate`/`NoMovesInCheck`) →
`rule50 >= 100` → `None`.

### 5. The child view

Extend `effects::analyze` to return the child view plus its casualties and
capture flag (today's private `PostMove`, now built as a `PositionView` +
`BlastCasualties` + `is_capture`). Every field must mirror `do_move`
exactly — the checklist the differential must pin:

| Child field | Source |
| --- | --- |
| placement | existing `analyze` masks (`blast_casualties` + move type: castling relocation, capture clears `from`, promotion swaps pawn/piece, quiet move relocates) |
| `side_to_move` | `self.us.flip()` |
| `ep_square` | `pt == Pawn && (to as i8 - from as i8).abs() == 16` → the intermediate square, else `None` (mirrors `do_move`, which sets it unconditionally for a double push) |
| `castling_rights` | `castling_rights_after(parent_rights, from, to, is_capture_or_ep, &post_blast_home_pieces)` where the six home-square pieces are read from the **child** view |
| `rule50` | `board.rule50_after(m)` (public, already differentially tested) |
| `occupied` | OR of the child's color sets |

`EffectContext::terminal_after`:

```rust
pub fn terminal_after(&self, board: &Board, m: Move) -> Option<Terminal> {
    let child = self.analyze(board, m)?;          // None for malformed input
    let mut state = StateInfo::new();
    child.view.populate_state(&mut state);        // child's checkers/pinned/counts
    position::terminal_in_view(&child.view, &state, None)
}
```

No legality gate, no clone, no `MoveList`, no allocation.

### 6. Documentation

- `effects` module docs: the fallback paragraph is deleted and replaced by the
  `terminal_after` contract and one worked example.
- `EffectContext::outcome_after` doc: `None` = illegal **or** undecided (the
  MG16 doc bug), pointing at `terminal_after` and at `child_outcome` (2.7).
- `lib.rs` crate docs: the "Move effects" section gains the child-terminality
  query; `Board::terminal_after` is listed next to `Board::outcome_after`.
- README feature bullet + a two-line usage example.

## Tests

New `tests/terminal_after.rs`; move the corpus/play-out helpers of
`tests/effects.rs` into `tests/common/mod.rs` (`perft_corpus()`, `Xorshift`,
`walk_positions`, `walk_playouts`, `walk_corpus_and_fixtures`) and use it from
both files. Integration-test only, no dev-dependency.

### Differential (the gate)

For every position of the 41-position perft corpus, every fixture below, and
seeded random play-outs (≥ 5,000 positions, ≥ 10,000 pseudo-legal moves,
depth 80, several start FENs incl. the start position, casting, en-passant and
promotion-heavy boards):

- `ctx.terminal_after(&b, m) == { clone; do_move; terminal() }` for **every
  pseudo-legal** move (legal and illegal).
- `== { clone; do_move; terminal_with_state(&populated_child_state) }`.
- `ctx.outcome_after(&b, m) == clone.do_move(m).outcome()` for every legal
  move, unchanged.
- `b.terminal_after(m, &state) == ctx.terminal_after(&b, m)`.
- if `is_terminal_after` lands: `== clone.do_move(m).is_terminal()`. Note the
  documented divergence: `terminal_after` is `Some(MaterialDraw)` where
  `is_terminal_after` is `false` for a K-vs-K child.
- if `has_legal_move_after` lands: `== has_legal_move_with_state(&child, ..)`.

Also, a `PositionView` unit test (in `src/position.rs`) over the corpus: every
accessor and `checkers`/`pinned`/`populate_state` result equals the `Board`
counterpart, so a view-construction defect fails with a field name rather than
a perft number.

### Deterministic fixtures (all verified against 2.6.0 while writing this plan)

| Case | Parent FEN | Move | Child `terminal()` |
| --- | --- | --- | --- |
| stalemate child | `7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55` | `c2c7` | `Draw` / `Stalemate` |
| stalemate child (2) | `8/8/k7/2p4p/7P/7K/5r2/8 b - - 1 59` | `f2g2` | `Draw` / `Stalemate` |
| extinction child | `k1n5/8/1K6/8/8/8/8/8 b - - 0 1` | `c8b6` | `Loss` / `Extinction` |
| illegal self-explosion child (parity) | `8/8/8/8/8/8/8/3qK3 w - - 0 1` | `e1d1` | `Loss` / `Extinction` on an empty board |
| material-draw child from a capture | `8/8/8/2R5/4n3/8/7K/5k2 b - - 13 69` | `e4c5` | `Draw` / `MaterialDraw` |
| material-draw child (2) | `8/8/8/8/8/7k/8/2K3Rr b - - 19 74` | `h1g1` | `Draw` / `MaterialDraw` |
| mate outranks the clock | `7k/8/8/5N2/3K4/8/8/R6R w - - 99 60` | `a1g1` | `Loss` / `NoMovesInCheck`, `rule50 = 100` |
| clock with the child in check and moves | `4k3/8/8/8/8/8/8/R3K3 w - - 99 60` | `a1a8` | `Draw` / `FiftyMoveRule` |
| clock, child not in check | `8/8/8/8/8/4k3/8/4K2q w - - 99 60` | `e1d2` | `Draw` / `FiftyMoveRule` |
| **en-passant escape is the child's only legal move** | `8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41` | `g7g5` | `None`; child `ep = g6`, in check, legal `["h5g6"]` |
| child keeps castling rights | `r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1` | `a1a2` (and `e1f1`) | `None`; child rights `Kkq` (`kq`), castling in the child's legal list |
| children of a material-draw position | `k7/8/8/8/8/8/8/7K w - - 0 1` | `h1g1` | `Draw` / `MaterialDraw` |

The en-passant and castling rows are the ones that fail if the child view
forgets `ep_square` or `castling_rights` (the child would look move-less and be
reported as mate/stalemate). The self-explosion row pins the exactness decision
of §3.

### Regression

- `tests/effects.rs` must pass unchanged (effect fields, `is_legal`,
  `outcome_after`, legal stream).
- `tests/conformance.rs`, `tests/move_order_golden.rs`, `tests/verify_moves.rs`,
  `tests/perft_tests.rs`, `tests/interchange.rs` unchanged.
- `verify_perft` 41/41 (depth 5 minimum), and `perft` timing within 2 % of the
  pre-refactor baseline.

## Verification

All global gates of the index, plus perft timing before/after (three runs,
median) and `cargo run --release --example effect_bench` before/after.

## Measurement (part of the deliverable)

Extend `examples/effect_bench.rs` (or add `examples/terminal_after_bench.rs`)
with, per bench position:

1. `terminal_after` vs `clone + do_move + terminal()`, ns per call, median of
   several runs.
2. `terminal_after` vs `clone + do_move + generate_legal_with_state`, the shape
   the downstream measures.
3. the **share of children answered `Some`** over a seeded play-out walk, and
   the share answered `None`, so the crate publishes a decided share of its own
   tree. Note in the report that this number is *not* the downstream's 10 %
   gate (their tree is the prover's); upstream can only ship the complete
   answer and tell them what to re-run.

## Files to touch

- `src/position.rs` — new crate-private view, `legal_in_view`,
  `terminal_in_view`, view unit tests.
- `src/board.rs` — legality/state/`is_material_draw` delegate to the view,
  `castling_rights_after` made `pub(crate)`, `Board::terminal_after`, docs.
- `src/movegen.rs` — view-based generator/ladder entry points, docs.
- `src/effects.rs` — child view construction, `terminal_after` (and optional
  `is_terminal_after`/`has_legal_move_after`), `state` field removal,
  `outcome_after` without the clone, module docs.
- `src/lib.rs` — `mod position;`, crate docs.
- `tests/common/mod.rs` — shared corpus/play-out helpers (extracted).
- `tests/effects.rs` — use the shared helpers.
- `tests/terminal_after.rs` — new differential + fixtures.
- `examples/effect_bench.rs` (or new bench example) — measurements.
- `README.md`, `CHANGELOG.md` — feature bullet, contract, doc-bug fix.
- `docs/plans/feedback/report_handoff2_1.md` — final report.

## Non-goals

- No public position representation; `PositionView` stays `pub(crate)`.
- No change to `Board::legal`'s result, `StateInfo`'s layout, generation order,
  perft values, hashes, FEN output, or `Outcome`/`Terminal` semantics.
- No search-oriented API ("prove this child", "guide", "PV"), no tablebase, no
  threading.
- No second rules implementation for the child: the child is answered by the
  same `legal` / `has_legal_move` / `terminal` code, fed a view.
- No performance claim about the effect path as a whole; this plan removes a
  clone and reports numbers.

## Final task

Write `docs/plans/feedback/report_handoff2_1.md`: the exact API as implemented,
differential counts, the fixture table with observed values, perft numbers
(nodes and timing) before/after, `terminal_after` cost and decided share,
deviations (signature, optional items), problems encountered, unresolved parts,
missing tests, and next steps.
