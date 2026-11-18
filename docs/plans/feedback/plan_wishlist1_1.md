# Plan 1 — legal-EP child key on the effect path (W1)

## Scope

One additive child-summary query and the crate-private helper it needs:

- **W1:** `EffectContext::post_move_and_key_with(board, m, mode)` returns the
  already-derived `PostMove` and the child's Zobrist key under the selected
  `EnPassantMode`, from one derivation and with no board clone.
- **Internal:** a view-side legal-EP test,
  `position::legal_ep_square_in_view`, so the `EnPassantMode::Legal` key can be
  derived from the child view without `populate_state` and without
  `Board::legal_ep_square`'s board write.
- `EffectContext::post_move_and_key` becomes the `EnPassantMode::Always`
  spelling of the new query. Its answer, and the `Always` key table and
  `ZOBRIST_VERSION`, do not change.

Requirements served: wishlist 1 W1 (P1). The wishlist's acceptance is a
differential against `clone + do_move + hash_with(mode)` on the conformance
corpus, the child cases and seeded random play-outs, with no clone for
non-double-push moves and one child derivation for double pushes.

## Current-tree assessment

- `src/effects.rs`:
  - `EffectContext::post_move_and_key` calls `analyze` once and then the private
    `child_key(board, parent_view, &post, m)`. `child_key` is an incremental
    XOR delta from `board.hash()` and has no mode parameter; it always applies
    the `EnPassantMode::Always` composition.
  - `PostMove` stores the child view, `is_capture`, and `casualties`. It has no
    parent hash and no `m`, so it cannot by itself produce an incremental key.
  - `EffectContext::analyze` derives the child through
    `position::child_view`, which already knows whether the move is a double
    pawn push and stores the child ep target.
- `src/board.rs`:
  - `Board::hash_after_with(m, EnPassantMode::Legal)` delegates to
    `hash_after(m)` for non-double-pushes. For a double push it clones the
    board, calls `do_move`, and then calls `hash_with(EnPassantMode::Legal)`.
    This is the clone the wishlist identifies.
  - `Board::legal_ep_square` is the exact board-side test, but it creates a
    `StateInfo`, calls `populate_state`, and validates candidate ep moves with
    `Board::legal`.
- `src/position.rs`:
  - `child_view` derives the child placement, side to move, ep square,
    castling rights and clock. It is the single child derivation used by the
    effect path.
  - `legal_with_counts(view, m)` is the body of `Board::legal` with the
    game-over guard supplied by the caller. It is exact for a view and does not
    need `StateInfo` or pins.
  - There is no view-side `legal_ep_square`. The existing `PositionView` API
    has `ep_square()`, `commoners`, `pieces_color_pt`, `is_capture`, and
    `blast_casualties`.
- `EnPassantMode` is `#[non_exhaustive]` and has `Always` and `Legal`. The
  `Legal` policy is a canonicalization layered on the same key table: it
  removes the stored ep-file key when no fully legal ep capture exists.

## Decisions and pushback

1. **The paired query is the API; a standalone `PostMove::key_with` is not
   added in this plan.** The wishlist offers `PostMove::key_with(mode)` as an
   equivalent shape, noting that it needs the parent or a stored base. Two
   candidate implementations are unattractive:
   - storing the parent hash (or the move) in `PostMove` increases the value
     that every `post_move`/`classification` call copies, including callers
     that never ask for a key;
   - computing the key from the child placement alone is a second Zobrist
     composition (~30 table loads), and it would sit next to the incremental
     `child_key` path with the same semantics but different cost.

   The supported consumer shape is therefore
   `post_move_and_key_with(board, m, mode)`: a consumer that wants summary +
   key + terminality calls it once and then `post.terminal()`. If the
   measurement shows a real consumer need for a summary-only key, that is a
   separate additive item with its own cost measurement; it is not smuggled
   into this plan.

2. **No clone.** `post_move_and_key_with` uses the existing `analyze`/`child_view`
   derivation exactly once. The `Legal` branch only asks whether the child's
   stored ep target has a legal capture; it never constructs a `Board`. The
   only double-push-specific work is that one legal-EP query.

3. **Legal EP is answered from the child view, with `legal_with_counts`.**
   `position::legal_ep_square_in_view(view)` mirrors `Board::legal_ep_square`:
   - return `None` without a stored target;
   - return `None` when either side has no commoner (the board method's
     terminal guard);
   - find the pawns of the side to move that attack the target;
   - for each candidate, require the enemy pawn on the ep-victim square and
     call `legal_with_counts(view, Move::make_enpassant(from, target))`.

   This is exact because `legal_with_counts` is the same body `Board::legal`
   uses; the board method differs only by its populated `StateInfo` and its
   game-over guard, both of which are supplied explicitly by the view test.
   `legal_with_counts` never needs `pinned`.

4. **The `Legal` key is the `Always` key with one optional XOR.** The existing
   incremental `child_key` already XORs the child ep-file key when the child
   `PostMove` stores an ep target. `EnPassantMode::Legal` removes that key
   exactly when the view-side legal-EP test says there is no legal capture:

   ```text
   always_key = child_key(board, parent_view, post, m)
   legal_key  = if post.ep_square().is_some()
                   && position::legal_ep_square_in_view(&post.view).is_none()
                { always_key ^ ZOBRIST.ep[file_of(ep)] }
                else { always_key }
   ```

   This is the same composition as `Board::hash_with(Legal)` in
   `src/board.rs`; no key table or version changes.

5. **`Board::hash_after_with` is not refactored in this plan.** Its clone is
   the convenience board path, and changing it is not required by W1. A later
   plan may delegate it to the same view helper if the measurement shows a
   useful win; this plan records that as a next step, not as scope.

6. **No new `PostMove` field and no public `PositionView`.** `PostMove` stays
   the same plain summary. `position::legal_ep_square_in_view` stays
   `pub(crate)`; the public surface is the paired query.

## Proposed API

```rust
// src/effects.rs

impl EffectContext {
    /// The child summary and the child's Zobrist key under `mode`, from one
    /// derivation.
    ///
    /// The key is exactly `board.hash_after_with(m, mode)` and exactly the
    /// `hash_with(mode)` of a cloned board after `do_move(m)`. With
    /// `EnPassantMode::Always` this is the existing `post_move_and_key`.
    ///
    /// `EnPassantMode::Legal` removes the child's en-passant file key when no
    /// fully legal en-passant capture exists in the child. The legal-EP
    /// question is answered from the already-derived child view, so a
    /// non-double-push move does no extra work and a double push is not
    /// cloned.
    ///
    /// Returns `None` for malformed input only (`Move::NONE`, no origin piece,
    /// or an origin that does not hold a piece of the side to move), like
    /// `post_move`.
    #[must_use]
    pub fn post_move_and_key_with(
        &self,
        board: &Board,
        m: Move,
        mode: EnPassantMode,
    ) -> Option<(PostMove, u64)>;
}
```

`post_move_and_key` keeps its signature and becomes:

```rust
pub fn post_move_and_key(&self, board: &Board, m: Move) -> Option<(PostMove, u64)> {
    self.post_move_and_key_with(board, m, EnPassantMode::Always)
}
```

Crate-private additions in `src/position.rs`:

```rust
/// The stored en-passant target, but only when the side to move has at least
/// one fully legal en-passant capture in this view.
///
/// Equal to `Board::legal_ep_square` for the equivalent board, without a
/// `StateInfo` and without a board write. Returns `None` when either side has
/// no commoner.
pub(crate) fn legal_ep_square_in_view(view: &PositionView) -> Option<Square>;
```

No change to `child_view`, `child_key`, `Board::hash_after_with`, the Zobrist
table, or `ZOBRIST_VERSION`.

## Implementation

1. **Add `position::legal_ep_square_in_view`.** Mirror
   `Board::legal_ep_square` branch for branch:

   ```rust
   pub(crate) fn legal_ep_square_in_view(view: &PositionView) -> Option<Square> {
       let ep = view.ep_square()?;
       let us = view.side_to_move();
       let them = us.flip();
       if view.commoners(us).is_empty() || view.commoners(them).is_empty() {
           return None;
       }
       let victim = match us {
           Color::White => Square::from_index(ep as i8 - 8),
           Color::Black => Square::from_index(ep as i8 + 8),
       };
       if view.piece_on(victim) != make_piece(them, PieceType::Pawn) {
           return None;
       }
       let mut origins =
           attacks::pawn_attacks(them, ep) & view.pieces_color_pt(us, PieceType::Pawn);
       while !origins.is_empty() {
           let from = origins.pop_lsb();
           let m = Move::make_enpassant(from, ep);
           if legal_with_counts(view, m) {
               return Some(ep);
           }
       }
       None
   }
   ```

   Use the existing attack-table symmetry (`pawn_attacks(them, ep)` is the set
   of origins from which a pawn of `us` attacks `ep`). The victim check is
   redundant for a well-formed child but makes the helper total and mirrors
   the board method.

2. **Add the mode-aware key helper in `src/effects.rs`.** Keep the existing
   `child_key` unchanged for `Always`, and wrap it:

   ```rust
   fn child_key_with(
       board: &Board,
       parent: &PositionView,
       post: &PostMove,
       m: Move,
       mode: EnPassantMode,
   ) -> u64 {
       let always = child_key(board, parent, post, m);
       match mode {
           EnPassantMode::Always => always,
           EnPassantMode::Legal => {
               if let Some(ep) = post.ep_square() {
                   if crate::position::legal_ep_square_in_view(&post.view).is_none() {
                       return always ^ crate::zobrist::ZOBRIST.ep[file_of(ep) as usize];
                   }
               }
               always
           }
       }
   }
   ```

   `EnPassantMode` is `#[non_exhaustive]` for downstream matches. Inside this
   crate, match both current variants explicitly so a future mode is a compile
   error at this key helper; external consumers keep the wildcard arm the
   enum's semver policy requires. Do not add a new mode.

3. **Wire the public methods.** Change `post_move_and_key` to delegate to
   `post_move_and_key_with(..., Always)`. Add
   `post_move_and_key_with` next to it with the doc comment above. Do not
   change `post_move`, `effect_and_post`, `terminal_after`, or
   `child_outcome`.

4. **Document the cost contract.** In the public docs and module docs state:
   - non-double-push moves never call the legal-EP helper;
   - double pushes derive the child once and may ask one view-side legal-EP
     question;
   - `Always` is the unchanged fast path;
   - `Legal` is a canonicalization, not a new key table, and
     `ZOBRIST_VERSION` is unchanged.

5. **Benchmark rows.** Add to `examples/effect_bench.rs`, in the per-child
   section:
   - `post_move_and_key_with(Always)` (must be indistinguishable from the
     existing `post_move_and_key` row);
   - `post_move_and_key_with(Legal)`;
   - `clone + do_move + hash_with(Legal)` as the comparison row;
   - the double-push share of the child corpus, and the Legal-sensitive share
     (double pushes whose child has a stored ep target and whose
     `legal_ep_square_in_view` is `None`).

   Add a class or small summary row for double pushes so the report can state
   the measured double-push share the wishlist asks for. Keep the existing
   rows and print labels; do not replace them.

## Tests

### Differential (the acceptance gate)

In `tests/post_move.rs` (or a new `tests/ep_key.rs` registered in
`Cargo.toml`'s explicit test list, matching the crate's existing style):

For every position of the 41-position perft corpus, the effect fixtures, the
conformance cases, the child cases, and seeded play-outs
(`tests/common/mod.rs`; at least 5 starts × 16 seeds × 300 plies):

- generate every pseudo-legal move;
- derive `ctx = board.effect_context_with_state(&state)` once per position;
- for each mode in `[EnPassantMode::Always, EnPassantMode::Legal]`:
  - `let (post, key) = ctx.post_move_and_key_with(&board, m, mode).unwrap();`
  - clone the board, `do_move(m, &mut child_state)`;
  - assert `key == child.hash_with(mode)`;
  - assert `key == board.hash_after_with(m, mode)` (the board-side convenience
    path; it is the current clone oracle and is deliberately not changed);
  - assert `post.rule50() == child.rule50()`;
- assert `ctx.post_move_and_key_with(&board, m, Always) ==
  ctx.post_move_and_key(&board, m)` for every move;
- assert malformed input (`Move::NONE`, an empty origin, an enemy origin) is
  `None` for both modes.

### Explicit EP fixtures

Add named rows in the test module for:

- a double push with a legal ep capture:
  `4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1`, move `e2e4`; the child stores `e3` and
  `legal_ep_square_in_view` is `Some(E3)`, so `Legal` and `Always` keys are
  equal;
- a double push with no adjacent enemy pawn: start position `e2e4`; the child
  stores `e3`, the legal EP is `None`, and `Legal` removes the ep-file key;
- a double push whose ep capture is pinned or self-exploding: use the existing
  `legal_ep_square_respects_pins_and_blast` fixtures from `src/board.rs`
  (re-express them as FEN/move pairs), so the `Legal` key is the removal case
  even though an ep *move* exists;
- an en-passant capture move itself: its child has no ep target, so `Always`
  and `Legal` agree and no legal-EP helper is called;
- castling and promotion moves: no ep target, both modes agree.

Every row asserts against `clone + do_move + hash_with(mode)`, not only against
the other internal helper.

### No board write

Record `board.fen()` and `board.hash()` before the loop and assert both are
unchanged after all queries, including the `Legal` double-push rows.

### Key stability

- Existing `tests/conformance.rs` asserts `ZOBRIST_VERSION == 1`; leave it
  unchanged. Regenerating conformance keys is not allowed by this plan.
- The test suite's `hash_after_with` differential must remain green. The plan
  does not edit any stored key or corpus value.

## Verification

All global gates of the index, plus:

```sh
cargo run --release --example effect_bench 9 2000   # three runs, medians
cargo test --test post_move
cargo test --test conformance
cargo test --features unsafe-optimizations
```

Acceptance, expressed as ratios measured in the same run:

| Row | Gate |
| --- | --- |
| `post_move_and_key_with(Always)` vs existing `post_move_and_key` | ≤ 1.02× (the change must not tax the existing path) |
| `post_move_and_key_with(Legal)` on non-double-push children vs `post_move_and_key(Always)` | ≤ 1.02×; no legal-EP query is allowed on this class |
| `post_move_and_key_with(Legal)` vs `clone + do_move + hash_with(Legal)` on double-push children | must be cheaper; report the ratio and absolute ns |
| double-push share of the child corpus | report as a count and percentage; the wishlist's decisive experiment asks for this share |
| existing child-path rows (`post_move`, `terminal`, `has_legal_move_after`) | no regression > 2 % |
| perft timing | no regression > 2 % (the plan does not touch the generator; this is a guard) |

If the `Legal` row is not cheaper than the clone path on this crate's corpus,
the report says so plainly and leaves adoption to the downstream's wall
measurement. The wishlist's rule is that a measured wall win decides adoption;
this plan only supplies the clone-free shape and the numbers.

## Files to touch

- `src/position.rs` — `legal_ep_square_in_view` and its unit test against
  `Board::legal_ep_square` on the corpus, fixtures, and random play-outs.
- `src/effects.rs` — `child_key_with`, `post_move_and_key_with`, delegation of
  `post_move_and_key`, docs, unit tests for the explicit EP fixtures.
- `tests/post_move.rs` (or new `tests/ep_key.rs`) — the differential and
  malformed-input tests.
- `examples/effect_bench.rs` — the new rows and double-push share.
- `CHANGELOG.md`, `README.md`, `src/lib.rs` — the additive API and its
  mode/`ZOBRIST_VERSION` contract.
- `docs/plans/feedback/report_wishlist1_1.md` — final report.

## Non-goals

- No `PostMove::key_with` in this plan (see decision 1).
- No change to `Board::hash_after_with`, `Board::hash_with`, the key table, or
  `ZOBRIST_VERSION`.
- No change to `Board::legal_ep_square`; the view helper is a parallel exact
  implementation used by the effect path.
- No change to `child_view`, move generation, FEN parsing, or terminality.
- No clone, no `StateInfo` population, no allocation on the keyed child path.
- No downstream-facing promise about a wall-clock win.

## Final task

Write `docs/plans/feedback/report_wishlist1_1.md` with item status, the exact
public API, differential counts (positions, pseudo-legal moves, double pushes,
legal-EP hits, removal cases), the benchmark table with absolute ns and the
ratios above, changed files, problems, deviations with the measurement that
motivated them, unresolved parts, missing tests, and next steps (whether
`Board::hash_after_with` should later delegate to the view helper, and whether
a standalone summary key is still requested).
