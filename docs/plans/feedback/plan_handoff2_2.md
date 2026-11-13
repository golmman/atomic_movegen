# Plan 2 — child information: two-level child outcome and the post-move summary (MG16, MG17)

## Scope

Two additive API items that read the child view plan 1 introduced:

- **MG16 (P2):** stop conflating "illegal move" with "legal, undecided child".
  Add `AfterState { Illegal, Undecided, Terminal(Terminal) }` and
  `EffectContext::child_outcome` / `Board::child_outcome`, and fix the
  `outcome_after` documentation that today claims the only `None` is an
  illegal move.
- **MG17 (P2):** publish the post-move piece summary the effect path already
  computes: a public `PostMove` value (occupancy, per-color, per-type,
  casualties, child side to move, ep square, castling rights, clock) reachable
  from an `EffectContext` without a board write and without a rules opinion.

Requirements served: R6 (MG16) and R3 (MG17).

**Depends on plan 1** (child view, shared `terminal_in_view`). May land in
parallel with plan 3.

## Current-tree assessment

- `EffectContext::outcome_after` returns `Option<Outcome>`; its doc says
  "Returns `None` when `m` is not legal on this board" and then describes the
  extinction/material/clock shortcuts and a cloned-board fallback for the rest.
  So the doc under-describes `None` (the undecided case is only implied by the
  fallback paragraph) — precisely MG16's complaint.
- `effects::PostMove` exists, is private, and holds `occupied`, `by_color`,
  `by_type`, `is_capture`, `casualties`. After plan 1 it also becomes (or
  holds) the child `PositionView` with side/ep/rights/rule50.
- `MoveEffect` has six public fields; its layout is public API in practice
  (consumers read `effect.is_capture` etc.), so it must not change shape.

## Decisions and pushback

1. **A named enum, not `Option<Result<..>>`.** `AfterState` is
   `#[non_exhaustive]` (crate policy for enums) with three variants; the crate
   prefers named variants over nested `Option`/`Result` — `Outcome`,
   `Terminal`, `FenLoadError` and `SetupError` all follow that shape.
2. **`child_outcome` is the new name, `outcome_after` stays.** `outcome_after`
   keeps its signature and its exact behaviour (`None` for illegal *and*
   undecided); only its documentation is corrected and its body re-pointed at
   `child_outcome`. A rename or a deprecation would break 2.6.0 consumers for
   no gain.
3. **The illegal test reuses the legality ladder, it is not re-derived.**
   `child_outcome` starts with `is_legal_from` (the same code `is_legal` and
   `effect` use). The acceptance bar "the illegal test is exact for
   pseudo-legal input (equal to `Board::legal`)" is therefore satisfied by
   construction and pinned by the existing effect differential plus a new arm
   differential.
4. **`PostMove` is the public face of the child view, and it is a plain data
   value.** It exposes placement, casualties, the child's side to move, its ep
   square, its castling rights and its clock — facts, no rules verdicts, no
   "insufficient material", no Lichess rule. It stays `pub(crate)`
   constructible: only `EffectContext` builds one.
5. **`MoveEffect` does not grow a placement field.** Copying ~100 bytes of
   view into every `MoveEffect` would make the common path pay for data most
   callers never read. Instead `PostMove` is a sibling query, plus one
   combined call (`effect_and_post`) for consumers that want both in a single
   analysis pass. This is the "zero cost when unused" requirement read
   literally.
6. **No new rules.** `AfterState` and `PostMove` add no branch to the terminal
   precedence; the reason values are exactly `TerminalReason`'s.

## Proposed API

```rust
// src/effects.rs (both re-exported at the crate root)

/// What an `EffectContext` knows about the child after a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AfterState {
    /// `m` is pseudo-legal but not legal: the child is unreachable.
    Illegal,
    /// `m` is legal and the child is not decided: the game continues.
    Undecided,
    /// `m` is legal and the child is over, from the child's perspective.
    Terminal(Terminal),
}

impl AfterState {
    pub fn is_illegal(&self) -> bool;
    pub fn is_undecided(&self) -> bool;
    pub fn is_terminal(&self) -> bool;
    /// The child's `Terminal` when decided.
    pub fn terminal(&self) -> Option<Terminal>;
    /// The child's WDL `Outcome` when decided.
    pub fn outcome(&self) -> Option<Outcome>;
}

/// The piece placement and position data of the child after `m`, derived
/// without a board write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostMove { /* private: child view + casualties + is_capture */ }

impl PostMove {
    /// The child's side to move (the opponent of the parent's).
    pub fn side_to_move(&self) -> Color;
    pub fn occupied(&self) -> Bitboard;
    pub fn pieces_color(&self, c: Color) -> Bitboard;
    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard;
    pub fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard;
    /// The piece standing on `sq` in the child, `NO_PIECE` when empty.
    pub fn piece_on(&self, sq: Square) -> Piece;
    /// Every piece the move's blast removes (identical to
    /// `Board::blast_casualties`).
    pub fn casualties(&self) -> BlastCasualties;
    /// The move removes a piece (normal capture or en passant).
    pub fn is_capture(&self) -> bool;
    /// The child's raw en-passant target, as `Board::ep_square()` reports it
    /// after `do_move(m)`.
    pub fn ep_square(&self) -> Option<Square>;
    /// The child's castling-right bitmask.
    pub fn castling_rights(&self) -> u8;
    /// The child's half-move clock.
    pub fn rule50(&self) -> u16;
}

impl EffectContext {
    pub fn child_outcome(&self, board: &Board, m: Move) -> AfterState;
    /// The child summary, or `None` for malformed input (`Move::NONE`, empty
    /// origin). No legality is involved: the summary of an illegal move
    /// describes the (unreachable) child exactly as `do_move` would build it.
    pub fn post_move(&self, board: &Board, m: Move) -> Option<PostMove>;
    /// `effect` and `post_move` from one analysis pass.
    pub fn effect_and_post(&self, board: &Board, m: Move) -> (MoveEffect, Option<PostMove>);
}

impl Board {
    pub fn child_outcome(&self, m: Move, state: &StateInfo) -> AfterState;
}
```

`MoveEffect` is unchanged. `outcome_after` behaviour is unchanged; its body
becomes a `match` over `child_outcome`.

## Implementation

1. `AfterState` in `src/effects.rs`, re-exported in `src/lib.rs`; add it to the
   `send_sync_assertions` list and to the crate docs' effect section.
2. `child_outcome`:
   ```rust
   pub fn child_outcome(&self, board: &Board, m: Move) -> AfterState {
       if self.game_over { return AfterState::Illegal; }
       let Some(child) = self.analyze(board, m) else { return AfterState::Illegal };
       if !self.is_legal_from(board, m, &child) { return AfterState::Illegal; }
       match self.terminal_from(&child) {          // plan 1's shared path
           Some(t) => AfterState::Terminal(t),
           None => AfterState::Undecided,
       }
   }
   ```
   `terminal_after` and `child_outcome` share one private
   `terminal_from(&self, child: &ChildView) -> Option<Terminal>` so the two
   cannot drift.
3. `outcome_after` — keep the signature, fix the doc:
   > Returns `None` when `m` is illegal **or when the child is not decided**.
   > The two cases are indistinguishable here; use `child_outcome` (or
   > `is_legal`) when the difference matters.
   and implement it through `child_outcome` (this also keeps the plan 1 win:
   no clone).
4. `PostMove` — make the private `PostMove` public with the accessor list
   above, keep the fields private, add `#[inline]` on every accessor, and
   `debug_assert!` nothing (a malformed move is `None`, not a panic).
   `EffectContext::post_move` is a thin wrapper over `analyze`. `effect` and
   `effect_and_post` share one analysis pass:
   `effect` = `effect_and_post(..).0`.
5. Docs: `effects` module docs get a "child summary" section; `lib.rs` and
   README get the two-item bullet; `CHANGELOG` gets the additions and the
   `outcome_after` documentation fix.
6. Optional (include only if the data can be generated and reviewed):
   - `conformance::CHILD_CASES: &[ChildCase]` — a frozen list of
     `(name, fen, uci, expected AfterState, child legal-move count)` covering
     each `TerminalReason`, `Undecided` and `Illegal`, verified field by field
     in `tests/conformance.rs`. This is the data a downstream uses to veto a
     child-terminality disagreement without trusting our implementation.
   - `PostMove`'s accessors are also mirrored by `Debug`.

## Tests

New `tests/child_state.rs` (uses `tests/common/mod.rs` from plan 1), plus unit
tests in `src/effects.rs`.

### Arm differential (MG16 acceptance)

For every pseudo-legal move of the perft corpus, the fixtures of plan 1, and
the seeded play-outs:

- `child_outcome(..) == Illegal` **iff** `!board.legal(m, &state)` (and iff
  `!generate_legal_with_state(..).as_slice().contains(&m)`).
- For legal moves: `Terminal(t)` iff the cloned child's `terminal()` is
  `Some(t)` (exact reason), and `Undecided` iff the cloned child's
  `terminal()` is `None`.
- `AfterState::terminal()`/`outcome()`/`is_*` helpers are consistent with the
  variant.
- `board.child_outcome(m, &state) == ctx.child_outcome(&board, m)`.
- `ctx.outcome_after(..) == match ctx.child_outcome(..) { Terminal(t) =>
  Some(t.outcome), Illegal | Undecided => None }` — the documented
  relationship, over every pseudo-legal move.
- Optional `CHILD_CASES`: every case reproduces from its FEN + UCI.

### Content differential (MG17 acceptance)

For every pseudo-legal move of the corpus and play-outs, with
`child = clone; do_move(m)`:

- `post.occupied() == child.occupied()`
- `post.pieces_color(c) == child.pieces_color(c)` for both colors
- `post.pieces_pt(pt) == child.pieces_pt(pt)` for all six types
- `post.piece_on(sq) == child.piece_on(sq)` for all 64 squares (sampled for the
  play-outs)
- `post.casualties() == board.blast_casualties(m)`, `post.is_capture() ==
  board.is_capture(m)`
- `post.side_to_move() == child.side_to_move()`
- `post.ep_square() == child.ep_square()`
- `post.castling_rights() == child.castling_rights()`
- `post.rule50() == child.rule50()`
- `post_move(..).is_none()` for `Move::NONE` and for a move with an empty
  origin square.
- `effect_and_post(..)` equals `(effect(..), post_move(..))` field by field,
  for every pseudo-legal move.

Also assert that the illegal-move summary is still a pure content fact: for a
self-explosion move the `PostMove` equals the `do_move` child (which is how the
plan 1 parity fixture `8/8/8/8/8/8/8/3qK3 w - - 0 1`, `e1d1`, is expressed).

### Deterministic fixtures

Reuse plan 1's table for `Undecided`/`Terminal`, plus:

- `Illegal`: `k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1` (every pseudo-legal move
  is illegal) and `8/8/8/8/8/8/8/3qK3 w - - 0 1` `e1d1`.
- `Undecided` with a non-trivial child: the en-passant fixture
  `8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41` `g7g5` (child in check with
  one legal move).
- `PostMove` with castling rights changing: `r3k2r/8/8/8/8/8/8/R3K2R w KQkq -
  0 1` `a1a2` → `castling_rights() == 0b1101` (`Kkq`), `ep_square()` stays
  `None`.
- `PostMove` with a child ep square: `8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - -
  17 41` `g7g5` → `ep_square() == Some(G6)`.

## Verification

All global gates of the index; `cargo test --test effects`,
`cargo test --test terminal_after`, `cargo test --test conformance`.

## Files to touch

- `src/effects.rs` — `AfterState`, `child_outcome`, `terminal_from` refactor,
  public `PostMove`, `post_move`, `effect_and_post`, `outcome_after` doc +
  re-point, unit tests, module docs.
- `src/board.rs` — `Board::child_outcome`; `Board::outcome_after` doc.
- `src/lib.rs` — re-export `AfterState`, `PostMove`; crate docs; Send+Sync
  assertions.
- `src/conformance.rs` + `tests/conformance.rs` — optional `CHILD_CASES`.
- `tests/common/mod.rs`, `tests/child_state.rs` — new differential tests.
- `README.md`, `CHANGELOG.md`.
- `docs/plans/feedback/report_handoff2_2.md` — final report.

## Non-goals

- No rename, removal or deprecation of `outcome_after`; no behaviour change.
- No change to `MoveEffect`'s fields or size.
- No insufficient-material rule, no draw rule of any kind added to `PostMove`.
- No public constructor for `PostMove`/`PositionView`; no public
  `PositionView`.
- No search-facing convenience ("is this child winning for me").

## Final task

Write `docs/plans/feedback/report_handoff2_2.md` with the implemented API, the
arm and content differential counts, the fixture table, whether `CHILD_CASES`
landed, deviations, problems, unresolved parts, missing tests and next steps.
