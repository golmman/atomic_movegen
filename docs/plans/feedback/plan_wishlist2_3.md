# Plan 3 — effect and child key from parent bitboards (W8)

## Scope

Add a lean child query for engines:

- `ChildEffect { occupied, key, rule50, capture }`;
- `Board::child_effect(m, counts, mode) -> Option<ChildEffect>`;
- `Board::child_effect_with_occupancy(m, occupied, counts, mode)` for a
  candidate loop that already has the parent occupancy (for example from
  plan 2's `LegalityContext::occupied()`);
- one shared incremental child-key implementation behind the new query and the
  existing `PostMove` path.

No `PostMove`, no full `PositionView` copy, no classification result unless the
caller asks for it, and no `ZOBRIST_VERSION` change.

Requirements served: wishlist 2 W8 (P1). Acceptance is field-for-field equality
with `clone + do_move + hash_with(mode)` and with `rule50_after` on the
differential corpus and play-outs, per-candidate instructions at ≤ 120 (today
~215 in the adapter, 394 batched), and no `ZOBRIST_VERSION` change.

## Current-tree assessment

- `EffectContext::post_move_and_key_with(board, m, mode)`:
  1. calls `analyze`, which validates the origin and calls
     `position::child_view(&self.view, m, board.rule50_after(m))`;
  2. `child_view` builds a full child `PositionView` (two color sets, six type
     sets, side, ep, rights, clock), computes `is_capture`, and computes blast
     casualties;
  3. `child_key_with` calls the incremental `child_key`, and for
     `EnPassantMode::Legal` may call `position::legal_ep_square_in_view` on the
     already-derived child view.
- `child_key` is already an incremental XOR delta from the parent's
  `board.hash()`: side key, emptied-square piece keys, filled-square piece
  keys, castling-rights key if changed, and old/new ep-file keys. It does not
  recompute the key from the child placement.
- The expensive part for a caller that only wants the engine facts is the
  `PostMove`/`PositionView` materialisation, not the key arithmetic. The
  wishlist's adapter cost is `hash_and_rule50_after` plus an occupancy query,
  and the batched query pays for a full summary.
- `Board::blast_casualties(m)` already derives the atomic removal set without a
  board write and without `PostMove`.
- `position::castling_rights_after(old_rights, from, to, is_capture,
  post_blast)` is the shared rights transition. `post_blast` is the piece on
  H1, A1, E1, H8, A8, E8 after the move.
- `position::legal_ep_square_in_view(view)` is the view-side legal-EP test
  added by wishlist 1. It needs a child view, so it is naturally only paid on
  a double push whose child stores an ep target.

## Decisions and pushback

1. **`ChildEffect` is a plain value, not a second `PostMove`.** It contains
   exactly the facts the wishlist names and nothing else:
   - `occupied`: the child's occupied squares;
   - `key`: the child key under the selected ep mode;
   - `rule50`: the child half-move clock, exactly `Board::rule50_after(m)`;
   - `capture`: whether the move captures (normal or en passant).
   It does not carry placement, casualties, castling rights, or terminality.
   A caller that needs those keeps using `PostMove`.

2. **The fields are public and the struct is `#[non_exhaustive]`.** The
   wishlist shows public fields and a hot-path caller should not pay for four
   getter calls. `#[non_exhaustive]` leaves room for a future field without
   breaking external pattern matches; external construction is not part of the
   contract. Derive `Debug, Clone, Copy, PartialEq, Eq`.

3. **Keep the requested `counts` parameter, but do not invent a precondition.**
   The child effect does not need the parent counts to compute the key or the
   occupancy. The plan keeps `counts` for candidate-loop symmetry and uses it
   only for a sound short-circuit in the `Legal` en-passant branch: if either
   parent count is zero, the child cannot have both commoners, so
   `legal_ep_square_in_view` is necessarily `None` and no child view is built.
   `counts` must be current if it is passed; stale counts can suppress a legal
   EP key, just as stale counts change `legal`. Malformed moves are `None`;
   well-formed illegal moves still return the child facts they would build,
   matching `post_move`'s documented behavior.

4. **`Always` and non-double-push `Legal` build no child view.** The child
   occupancy, rights and ep target are derived from the parent board and the
   move. For `Legal`, only a double push whose child stores an ep target may
   construct a temporary child view to ask the existing legal-EP helper. This
   preserves the current “do not re-derive the child” behavior for every
   non-double-push move. It is acceptable for the rare double-push case to
   reuse `position::child_view` rather than duplicate legal-EP logic.

5. **One key implementation.** `child_key` is refactored into a
   `child_key_from_parts` helper taking the parent, move, `is_capture`,
   casualties, child rights, and child ep. `child_key(board, parent, post, m)`
   becomes a thin adapter that supplies the same values from `PostMove`. The
   new lean path supplies them directly. There must not be a second copy of the
   XOR composition.

6. **One clock implementation.** The child clock is derived from the same
   facts as `Board::rule50_after`: castling increments; a pawn move or capture
   resets to zero; any other move saturating-increments. The lean path already
   classifies the moving piece and capture for the key, so it reuses those
   facts instead of calling `rule50_after` (which would classify again).

7. **Occupancy reuse is explicit.** `Board::child_effect` computes
   `self.occupied()` and delegates to
   `child_effect_with_occupancy`. A caller that already has the occupancy from
   a plan-2 `LegalityContext` can call the second method and avoid the
   re-read. The second method documents that `occupied` must equal
   `self.occupied()` at the time of the call; it is a hot-path contract, like
   a pre-populated `StateInfo`.

8. **No `PostMove` change and no effect-path extension.** `post_move`,
   `post_move_and_key_with`, `is_legal`, `EffectContext`, and the Zobrist table
   are untouched in their answers. The new child query is additive and lives
   beside them. `ZOBRIST_VERSION` remains `1`.

## Proposed API

```rust
// src/effects.rs (or src/board.rs, next to the other child helpers)

/// The engine-facing facts of a child position.
///
/// Equal, field for field, to a cloned child board after `do_move(m)` for the
/// same move: `occupied == child.occupied()`,
/// `key == child.hash_with(mode)`, `rule50 == child.rule50()`, and
/// `capture` is the move's capture flag.
///
/// This is intentionally smaller than [`PostMove`]: it carries no placement,
/// casualties, castling rights, or terminality. Use `PostMove` (or a built
/// child) for those.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChildEffect {
    /// The child's occupied squares.
    pub occupied: Bitboard,
    /// The child's Zobrist key under the requested en-passant policy.
    pub key: u64,
    /// The child's half-move clock.
    pub rule50: u16,
    /// Whether the move captures (normal capture or en passant).
    pub capture: bool,
}

impl Board {
    /// The child effect of `m`, without building a child board or a
    /// [`PostMove`].
    ///
    /// Returns `None` for malformed input only ([`Move::NONE`], an empty
    /// origin, or an origin without a piece of the side to move), like
    /// [`EffectContext::post_move`](crate::EffectContext::post_move). Defined
    /// for well-formed illegal moves: the returned facts describe the child
    /// `do_move` would build.
    ///
    /// `counts` must be current for this board. It is used only to skip the
    /// legal-EP child view when either side has no commoner. `mode` selects
    /// the same `Always`/`Legal` key policy as
    /// [`Board::hash_after_with`](Board::hash_after_with).
    #[must_use]
    pub fn child_effect(
        &self,
        m: Move,
        counts: Counts,
        mode: EnPassantMode,
    ) -> Option<ChildEffect>;

    /// [`child_effect`](Self::child_effect) with a caller-supplied parent
    /// occupancy.
    ///
    /// `occupied` must equal `self.occupied()` at the time of the call. The
    /// intended producer is
    /// [`LegalityContext::occupied`](crate::LegalityContext::occupied), so a
    /// candidate loop does not recompute the parent occupancy.
    #[must_use]
    pub fn child_effect_with_occupancy(
        &self,
        m: Move,
        occupied: Bitboard,
        counts: Counts,
        mode: EnPassantMode,
    ) -> Option<ChildEffect>;
}
```

`ChildEffect` is re-exported at the crate root. No `EffectContext` method is
added.

## Implementation

1. **Extract the key parts.** Replace the body of `child_key` with a helper:

   ```rust
   fn child_key_from_parts(
       board: &Board,
       parent: &PositionView,
       m: Move,
       is_capture: bool,
       casualties: BlastCasualties,
       child_castling_rights: u8,
       child_ep: Option<Square>,
   ) -> u64;
   ```

   `child_key(board, parent, post, m)` calls it with
   `post.is_capture()`, `post.casualties()`, `post.castling_rights()`, and
   `post.ep_square()`. The new path calls it with the values it derives below.
   The arithmetic, empty-key behavior, and castling/EP key handling are exactly
   the current body. Keep the comment explaining why `board.piece_on` is used
   instead of `parent.piece_on`.

2. **Validate and classify once.** In `child_effect_with_occupancy`:
   - reject `Move::NONE` and an origin that does not hold a piece of the side
     to move (the same malformed-input contract as `analyze`);
   - read `us`, `from`, `to`, `moving_type = board.piece_on(from).type_of()?`,
     `move_type = m.move_type()`;
   - `is_capture = move_type == EnPassant || (move_type != Castling &&
     occupied.contains(dest_index))`;
   - `is_capture_or_ep = is_capture || move_type == EnPassant`;
   - `casualties = board.blast_casualties(m)` (empty for a quiet move);
   - `castling = move_type == Castling`.

3. **Derive the child occupancy.**

   ```text
   castling:        (occupied \ {kfrom, rfrom}) | {kto, rto}
   capture / EP:    occupied \ ({from} | casualties.all())
   normal/promotion:(occupied \ {from}) | {to}
   ```

   The capturer at ground zero is already in `casualties`; the origin is not,
   so a capture explicitly removes both. This matches `child_view` and
   `legal_with_counts`.

4. **Derive the child ep target.** A double pawn push (`moving_type == Pawn &&
   abs(to - from) == 16`) stores the intermediate square, exactly as
   `child_view` does. Every other move stores `None`.

5. **Derive the child castling rights.**
   - Castling: clear the mover's two rights.
   - Non-castling: call `castling_rights_after(parent_rights, from, to,
     is_capture_or_ep, &post_blast)`. For a quiet move `post_blast` is ignored,
     so pass `[NO_PIECE; 6]`. For a capture, fill the six home squares with
     `if sq == from || casualties.all().contains(sq) { NO_PIECE } else {
     board.piece_on(sq) }`. This is the post-blast content `child_view` would
     read, without building the six type arrays.

6. **Derive the clock.** Exactly `Board::rule50_after`:

   ```text
   castling                         -> parent.rule50.saturating_add(1)
   pawn move or capture             -> 0
   otherwise                        -> parent.rule50.saturating_add(1)
   ```

   `moving_type == Pawn` covers promotions; `is_capture` covers normal and
   en-passant captures.

7. **Compute the key.** Call `child_key_from_parts` with the derived
   `is_capture`, casualties, child rights, and child ep. This is the `Always`
   key. For `EnPassantMode::Legal`, if `child_ep` is `Some` and both counts are
   nonzero, build the child view only then:

   ```rust
   let legal_ep = position::child_view(self.position_view(), m, rule50)
       .and_then(|child| position::legal_ep_square_in_view(&child.view));
   if legal_ep.is_none() {
       key ^= ZOBRIST.ep[file_of(ep) as usize];
   }
   ```

   The existing `legal_ep_square_in_view` already returns `None` when either
   child side has no commoner, so the counts short-circuit is a cost guard, not
   a different answer. Do not build the child view for `Always`, for a
   non-double-push, or for a `Legal` move with no child ep target. Reuse the
   existing implementation rather than adding a second legal-EP rule.

8. **Wire the public methods.** `Board::child_effect` calls
   `child_effect_with_occupancy(m, self.occupied(), counts, mode)`. Both are
   allocation-free and do not write the board.

9. **Document the contracts.** `ChildEffect`, both board methods, the hot-path
   section in `src/lib.rs`, and the README must state:
   - equality with the built child and with `PostMove` fields;
   - malformed vs. illegal behavior;
   - the `counts` and `occupied` preconditions;
   - the `Legal` double-push-only child view;
   - no key-table or `ZOBRIST_VERSION` change.

10. **Bench rows.** Extend `examples/hotpath_bench.rs`:
    - `child_effect(Always)`, `child_effect(Legal)`;
    - `child_effect_with_occupancy` with `ctx.occupied()`;
    - `post_move_and_key_with(Always/Legal)` as the current rows;
    - `clone + do_move + hash_with(mode)` as the oracle/cost comparison;
    - per class: quiet, capture, promotion, EP, castling, double-push, all.
    Print the double-push share, because that is the only class that can pay
    an extra legal-EP view under `Legal`.

## Tests

### Differential (the acceptance gate)

For every position of the 41-position perft corpus, the effect/conformance
fixtures, the child cases, and the seeded play-outs:

- populate a full `StateInfo`, build `ctx = board.effect_context_with_state`,
  and `counts = board.counts()`;
- for every pseudo-legal move and each mode in
  `[EnPassantMode::Always, EnPassantMode::Legal]`:
  - `let effect = board.child_effect(m, counts, mode).unwrap();`
  - clone the board, `do_move(m, &mut child_state)`;
  - assert `effect.occupied == child.occupied()`;
  - assert `effect.key == child.hash_with(mode)`;
  - assert `effect.key == board.hash_after_with(m, mode)`;
  - assert `effect.rule50 == child.rule50()`;
  - assert `effect.rule50 == board.rule50_after(m)`;
  - assert `effect.capture == ctx.post_move(&board, m).unwrap().is_capture()`;
  - assert `board.child_effect_with_occupancy(m, board.occupied(), counts,
    mode) == Some(effect)`;
- assert malformed input (`Move::NONE`, empty origin, enemy origin) is `None`;
- assert a well-formed but illegal move is `Some` and matches the built child
  (the summary/contract is defined for it);
- assert the parent board is unchanged (`fen()`, `hash()`, `ep_square()`,
  `rule50()`, `game_ply()`) after all queries.

### Named EP/key fixtures

Reuse the wishlist-1 fixtures and add the W8 cases:

- double push with a legal ep capture:
  `4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1`, `e2e4`; `Legal` and `Always` agree;
- double push with no adjacent enemy pawn: start position `e2e4`; `Legal`
  removes the ep-file key;
- double push whose ep capture is pinned or self-exploding: `Legal` is the
  removal case even though an ep move exists;
- an actual en-passant capture: the child stores no ep target; both modes
  agree;
- castling and promotion: no ep target; both modes agree;
- a terminal parent with a double push (either count zero): `Legal` still
  returns the same key as the built child; the counts short-circuit must not
  change the answer.

Every named row compares against a built child, not only against the other
internal helper.

### PostMove consistency and key stability

- For every pseudo-legal move, assert `child_effect` fields equal the
  corresponding `PostMove` fields (`occupied`, key from
  `post_move_and_key_with`, `rule50`, `capture`).
- Existing `tests/conformance.rs` and `tests/ep_key.rs` must stay green.
  `ZOBRIST_VERSION` stays `1`; no stored key or corpus value changes.
- Run the existing `hash_after`/`rule50_after` differential, since the key
  helper and clock derivation are refactored.

## Benchmark and gate

Rows on the same run:

| Row | Gate |
| --- | --- |
| `child_effect(Always)` vs `post_move_and_key_with(Always)` | must be cheaper; report ratio and ns |
| `child_effect(Legal)` vs `post_move_and_key_with(Legal)` | must be cheaper; report by double-push share |
| `child_effect_with_occupancy` vs `child_effect` | report whether the occupancy reuse is measurable |
| `child_effect` vs `clone + do_move + hash_with(mode)` | report; the lean path should be cheaper |
| `perf stat -e instructions:u` per child_effect | compare with ≤ 120 target; report command and call count |
| existing `effect_bench` child rows | no regression > 2 % |
| perft and effect_bench parent rows | no regression > 2 % |

If the lean query is not cheaper than the current `PostMove` path on this
crate's corpus, the report says so plainly and leaves adoption to the
downstream's wall measurement. The plan claims only the measured shape, not a
whole-binary saving.

## Files to touch

- `src/effects.rs` — `ChildEffect`, key-parts extraction, child-effect
  implementation, docs, unit tests.
- `src/board.rs` — public `child_effect`/`child_effect_with_occupancy` wrappers
  and docs, if they are placed beside the other board child helpers.
- `src/lib.rs` — root re-export and hot-path docs.
- `tests/effects.rs`, `tests/post_move.rs`, or a new
  `tests/child_effect.rs` — differential, named fixtures, malformed/illegal
  contract.
- `tests/public_api.rs` — `ChildEffect` and the two methods resolve.
- `examples/hotpath_bench.rs` — child rows and double-push share.
- `README.md`, `CHANGELOG.md` — additive API, contract, and measured status.
- `docs/plans/feedback/report_wishlist2_3.md` — final report.

## Non-goals

- No change to `PostMove`, `EffectContext`, `post_move_and_key_with`,
  `is_legal`, or the effect classification.
- No new child classification/terminality query; that is not W8.
- No change to the key table, key composition, or `ZOBRIST_VERSION`.
- No `Legal`-only new table or a second legal-EP implementation.
- No board write, allocation, `StateInfo` population, or full `PositionView`
  copy on the `Always`/non-double-push paths.
- No search/evaluation/threading API.

## Final task

Write `docs/plans/feedback/report_wishlist2_3.md` with:

- the exact API as implemented;
- differential counts: positions, moves, modes, double pushes, legal-EP hits,
  and removal cases;
- the benchmark table with absolute ns, ratios, optional instructions per call,
  and the double-push share;
- confirmation that `Legal` does not build a child view for non-double-pushes
  and that `Always` builds none;
- changed files, problems, deviations with measurements, unresolved parts,
  missing tests, and next steps (including whether
  `child_effect_with_occupancy` earned its surface).
