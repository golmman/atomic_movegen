# Plan 4 — null move with an advanced clock (W9)

## Scope

Add one additive board method:

- `Board::null_for_advanced(&self, attacker: Color) -> Board`: the same pieces
  and castling rights, `attacker` to move, no en-passant target, `rule50 + 1`,
  `game_ply + 1`, and a correctly updated Zobrist key;
- no `Setup`/FEN round-trip, no allocation, no board write on `self`;
- leave `Board::null_for` exactly as it is.

Requirements served: wishlist 2 W9 (P2). Acceptance is equality with the
adapter's current advanced null position (side flipped, no ep, clock + 1, same
key discipline) on the fixtures, allocation-free behaviour, and the downstream
`null_advances_the_clock` test still passing.

## Current-tree assessment

- `Board::null_for(attacker)`:
  - clones the board;
  - sets `side_to_move = attacker` and `ep_square = None`;
  - recomputes `game_ply` from the **preserved fullmove number**
    (`fullmove = self.fullmove_number()`), so the derived ply follows the new
    side exactly as a FEN round-trip with the same fullmove number does;
  - keeps `rule50`;
  - recomputes the full hash from the 64-square array.
- This is already allocation-free (the board is a stack value), but it is the
  FEN-preserving null, not the “advance the 50-move clock” null the prover
  needs. In particular, flipping Black to White at fullmove 1 yields
  `game_ply == 1`, not `game_ply + 1`.
- `Setup` already has `set_rule50` and `set_game_ply`, so a downstream adapter
  can rebuild the advanced null through `Board::from_setup`; that is the path
  this plan removes from the hot probe.
- The hash includes pieces, side, castling rights, and the stored ep file, but
  not `rule50` or `game_ply`. A null that clears ep and changes side therefore
  needs only two XOR deltas relative to the parent key: toggle the side key
  when the side changes, and remove the old ep-file key if an ep target was
  stored. The castling-rights key and every piece key stay unchanged.

## Decisions and pushback

1. **Add `null_for_advanced`, not a public `set_rule50`.** The wishlist allows
   either shape. A dedicated method keeps the null clock transition in one
   place, documents that it also advances `game_ply`, and avoids a second way
   to construct a board whose clock, side, and ply do not describe a real move.
   `Setup` already exposes a builder-level `set_rule50` for artificial
   positions; no board-level setter is needed for W9.

2. **Do not change `null_for`.** Its FEN-preserving `game_ply` derivation is a
   documented contract and is covered by `tests/null_for.rs`. Changing it to
   advance the clock would silently change an existing public answer. The new
   method is explicitly the search-clock variant; both are documented next to
   each other.

3. **“Advanced” means unconditional `rule50 + 1` and `game_ply + 1`.** This is
   what the wishlist asks for and what the downstream's 50-move probe needs.
   It is deliberately not the same as “FEN round-trip with the same fullmove
   number”: when the null flips Black to White, a same-fullmove FEN round-trip
   would not advance the ply, while this method does. The documentation says
   so; tests compare against a `Setup` rebuild with explicit fields.

4. **Clock saturation is explicit.** `rule50` saturates at `u16::MAX`;
   `game_ply` saturates at the internal counter's limit (and its public getter
   saturates at `u16::MAX`, matching the existing getter contract). The plan
   does not introduce a new overflow path. A FEN with `rule50 == 65535` stays
   at `65535`; a FEN with a large fullmove number cannot overflow the internal
   `u32` game ply.

5. **Hash update is incremental, not a full recomputation.** The result is
   exactly a recomputed hash for the modified position:
   - if `attacker != self.side_to_move()`, XOR `ZOBRIST.side`;
   - if `self.ep_square` is `Some`, XOR out the ep-file key;
   - pieces, castling rights, and the side key of the new position are
     otherwise unchanged.
   The plan keeps `null_for`'s recomputation as-is; only the new method uses
   the delta. The key table and `ZOBRIST_VERSION` are unchanged.

6. **No search API.** The method builds a `Board`, nothing more. It does not
   set a “null move” flag, does not mark a transposition-table entry, and does
   not decide whether a null is legal in a search. The caller owns that policy.

## Proposed API

```rust
impl Board {
    /// The null counterpart of this position with an advanced half-move clock.
    ///
    /// Same pieces and castling rights, `attacker` to move, no en-passant
    /// target, `rule50 + 1`, and `game_ply + 1`. The Zobrist key is updated
    /// for the new side and cleared ep target; it is exactly the key of a
    /// board with the same placement and state fields.
    ///
    /// Unlike [`null_for`](Self::null_for), this does **not** derive the game
    /// ply from the preserved fullmove number. It advances the clock by one
    /// ply unconditionally, as a 50-move-rule null probe requires. For the
    /// usual `attacker = self.side_to_move().flip()`, the two methods agree on
    /// the side and ep target but differ by the advanced clock.
    ///
    /// The result is an ordinary [`Board`]. A caller that uses it as an
    /// ephemeral null position must keep it out of keyed or stored state.
    #[must_use]
    pub fn null_for_advanced(&self, attacker: Color) -> Board;
}
```

No `set_rule50` on `Board` in this plan.

## Implementation

1. **Add the method beside `null_for`.** Use a clone and field updates:

   ```rust
   pub fn null_for_advanced(&self, attacker: Color) -> Board {
       let mut board = self.clone();
       let old_side = self.side_to_move();
       let old_ep = self.ep_square();

       board.position.side_to_move = attacker;
       board.position.ep_square = None;
       board.position.rule50 = self.position.rule50.saturating_add(1);
       board.game_ply = self.game_ply.saturating_add(1);

       // The hash does not include rule50 or game_ply.
       if attacker != old_side {
           board.hash ^= crate::zobrist::ZOBRIST.side;
       }
       if let Some(sq) = old_ep {
           board.hash ^= crate::zobrist::ZOBRIST.ep[file_of(sq) as usize];
       }
       board
   }
   ```

   `file_of` is already imported/used in `board.rs`; keep the use site local if
   needed. Do not call `recompute_hash`; the delta is exactly the recomputed
   value because only side and ep change.

2. **Do not refactor `null_for` to share the hash delta in this plan.** Its
   current `recompute_hash` is correct and measured; changing it would add a
   second behavior under the same name and make the existing tests less useful.
   If a later plan wants to optimize `null_for`, it must keep the FEN-preserving
   `game_ply` semantics and have its own report.

3. **Document the clock/ply distinction.** In the method docs and in the
   hot-path section of `src/lib.rs`/README:
   - `null_for` preserves the fullmove number and derives the ply as a FEN
     round-trip would;
   - `null_for_advanced` advances both clocks unconditionally;
   - neither method changes castling rights, pieces, or the stored ep beyond
     clearing it;
   - the advanced result is not necessarily FEN-equivalent to a same-fullmove
     round-trip when the side does not flip.

4. **Bench rows.** Extend `examples/hotpath_bench.rs`:
   - `null_for` (current FEN-preserving null);
   - `null_for_advanced`;
   - a `Setup` rebuild matching the downstream adapter (copy pieces, side,
     castling rights, ep cleared, `rule50 + 1`, `game_ply + 1`, then
     `Board::from_setup`).
   Report absolute ns and the ratios. The `Setup` row must build a fresh
   `Setup` per call or clearly state if it reuses one; the report should not
   compare a reused `Setup` against a clone without saying so.

## Tests

Extend `tests/null_for.rs` (do not replace the existing `null_for` tests).

### Advanced clock and fields

For each fixture and for the usual `attacker = board.side_to_move().flip()`:

- build the expected board with `Setup`:
  - copy every `(square, piece)` from `board.pieces()`;
  - `set_side_to_move(attacker)`;
  - `set_castling_rights(board.castling_rights())`;
  - `set_ep_square(None)`;
  - `set_rule50(board.rule50().saturating_add(1))`;
  - `set_game_ply(board.game_ply().saturating_add(1))`;
  - `Board::from_setup(&setup)`.
- assert equality of:
  - `fen()`, `hash()`, `hash_with(EnPassantMode::Legal)`;
  - `side_to_move()`, `ep_square()`, `rule50()`, `game_ply()`,
    `fullmove_number()`, `castling_rights()`;
  - `outcome()`/`is_terminal()`? Only if the clock change crosses a terminal
    boundary; a direct equality of the boards plus the existing terminal logic
    is enough. Add one fixture at `rule50 == 99` and assert the advanced board
    reaches the 50-move branch at the same point as the built expected board.
- assert the source board is unchanged: `fen()`, `hash()`, `rule50()`,
  `game_ply()`.

### Saturation

- `rule50 == u16::MAX`: result stays `u16::MAX`; no panic in debug or release.
- `rule50 == 99`: result `100`, and the advanced board's terminal/outcome
  matches the `Setup` expected board.
- Large fullmove (`65535`) with both colors: `game_ply` advances by one and the
  public getters saturate consistently; compare with the `Setup` expected
  board where representable.
- `attacker == board.side_to_move()`: the side does not change, `rule50` and
  `game_ply` still advance, the hash toggles no side key, and the ep key is
  removed. Compare with a `Setup` rebuild with the same side and advanced
  clocks.

### Hash discipline

- For every fixture, assert `null.hash() == expected.hash()` for the `Setup`
  expected board. This proves the incremental side/ep delta is complete.
- `null.hash_with(EnPassantMode::Legal)` equals the expected board's legal
  hash; a null has no stored ep target, so the legal policy should not change
  the key.
- No Zobrist table or version assertion changes.

### Existing contract

- Keep `null_for_matches_fen_roundtrip` and
  `null_for_result_is_an_ordinary_board` unchanged.
- Add a test that `null_for_advanced` and `null_for` have the same side (when
  given the same attacker) and both clear ep, but differ exactly in `rule50`
  and `game_ply` for the documented cases. This pins the distinction rather
  than relying on prose.

## Benchmark and gate

Rows on the same run:

| Row | Gate |
| --- | --- |
| `null_for_advanced` vs `null_for` | report ratio and absolute ns; advanced should not be slower |
| `null_for_advanced` vs `Setup` rebuild | report ratio; expected win |
| `null_for_advanced` vs `clone` only | show the clock/hash overhead |
| `perf stat -e instructions:u` | optional diagnostic; report command and calls |
| perft/effect_bench | no regression (the method is additive, this is a guard) |

The method is additive; no whole-binary regression is expected. If the measured
`null_for_advanced` row is not cheaper than the `Setup` rebuild on the bench
corpus, the report says so and leaves adoption to the downstream's wall
measurement.

## Files to touch

- `src/board.rs` — `null_for_advanced`, docs, unit tests.
- `src/lib.rs` — hot-path docs/export list.
- `tests/null_for.rs` — advanced-clock, saturation, hash-discipline, and
  unchanged-`null_for` tests.
- `tests/public_api.rs` — `Board::null_for_advanced` resolves.
- `examples/hotpath_bench.rs` — null rows.
- `README.md`, `CHANGELOG.md` — additive API and clock distinction.
- `docs/plans/feedback/report_wishlist2_4.md` — final report.

## Non-goals

- No change to `Board::null_for`, its FEN-round-trip semantics, or its tests.
- No public board-level `set_rule50`/`set_game_ply`.
- No change to the hash table, key composition, or `ZOBRIST_VERSION`.
- No search/evaluation/tablebase/threading behavior and no null-move legality
  policy.
- No allocation and no FEN string construction in the new method.

## Final task

Write `docs/plans/feedback/report_wishlist2_4.md` with:

- the exact API and the documented clock/ply distinction;
- field-by-field equality results against the `Setup` expected boards on all
  fixtures, including saturation and same-side cases;
- confirmation that `null_for` and its tests are unchanged;
- the benchmark table (absolute ns, ratios, optional instructions) with the
  `Setup` row's construction disclosed;
- changed files, problems, deviations, unresolved parts, missing tests, and
  next steps.
