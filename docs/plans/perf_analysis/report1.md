# Report 1: Precompute State in `do_move()` (with Early-Out)

## Summary

Plan 1 has been fully implemented. The changes introduce a
`Board::populate_state()` method that fills `StateInfo` caching fields at
the end of `do_move()`, combined with an early-out in `legal()` that skips
the expensive pseudo-royal attack scan for trivially safe moves.

**Total:** 42 lines added, 8 removed across 2 files (`src/board.rs`,
`src/movegen.rs`).

**Measured speedup:** 33.9 % reduction in `verify_perft` total wall-clock
time (188.246 s → 124.380 s), exceeding the estimated 30–50 % range (lower
bound). See [`docs/perf/m1/2026-06-30.txt`](../../perf/m1/2026-06-30.txt)

---

## Changes Implemented

### 1. `Board::populate_state()` method (Step 1)

Added after `pinned()` and before `do_move()`:

```rust
pub fn populate_state(&self, state: &mut StateInfo) {
    state.checkers = self.compute_checkers(self.side_to_move);
    state.pinned = self.compute_pinned(self.side_to_move);
    state.commoners_count = self.commoners(self.side_to_move).count();
    state.them_commoners_count = self.commoners(self.side_to_move.flip()).count();
}
```

Wraps the existing `compute_checkers`/`compute_pinned` methods and stores
results into `StateInfo`.

### 2. `populate_state()` called at end of `do_move()` (Step 2)

Inserted just before the closing brace of `do_move()`, after
`self.game_ply += 1`. At this point `side_to_move` has been flipped to the
opponent, so the state is populated from the *new* side's perspective —
ready for the next half-move's `legal()` call.

### 3. `generate_legal()` populates state (Step 3)

`src/movegen.rs`: changed from `let state = StateInfo::new()` to
`let mut state = StateInfo::new(); board.populate_state(&mut state);`.
This ensures root positions (not reached via `do_move`) also have their
state populated.

### 4. Early-out in `legal()` (Step 4b)

Inserted after the `NO_PIECE` check and before the castling block:

```rust
if state.checkers.is_empty()
    && !is_capture
    && m.move_type() != MoveType::EnPassant
    && pt != PieceType::Commoner
    && (state.pinned & Bitboard::square_bb(from)).is_empty()
    && state.commoners_count > 0
{
    return true;
}
```

Returns `true` early when a move cannot possibly change the attack status
of any commoner. Requires computing `pt` and `is_capture` at the top of
`legal()` (they were previously computed later).

### 5. Cached counts replace `.count()` calls (Step 4a)

Lines 815-816 changed from:

```rust
let our_pr_count = self.commoners(us).count();
let them_pr_count = self.commoners(them).count();
```

to:

```rust
let our_pr_count = state.commoners_count as usize;
let them_pr_count = state.them_commoners_count as usize;
```

### 6. Removed `_state` prefix, removed duplicate `is_capture` (Step 5)

- Renamed `_state` to `state` in the `legal()` signature.
- Removed the later `let is_capture = ...` declaration (lines 767-768 in
  the original file) since it is now computed early.

---

## Bugs Discovered and Fixed

Two correctness issues were found during implementation that are not
mentioned in the original plan. Both were exposed by the new early-out
code path.

### Bug 1: Early-out fires for side with zero commoners

**Root cause:** The previous move may have destroyed the side-to-move's
last commoner via blast (on a capture). With no commoners remaining, no
move can be legal, but the early-out was returning `true` for safe-looking
non-capture moves.

**Manifestation:** After `Qf3xf7+blast` in position 2, Black's last
commoner (on e8) was destroyed. The old code correctly returned `false`
for every Black move (self-explosion check caught `our_commoners.is_empty()`).
The early-out skipped this check.

**Fix:** Added `state.commoners_count > 0` to the early-out condition.

### Bug 2: `compute_checkers()` missing pawn attack detection

**Root cause:** The original `compute_checkers()` function only checked
for checks from rooks, bishops, queens, and knights — but NOT pawns.
This was a pre-existing bug that had no effect when `StateInfo.checkers`
was a dead field (never read). The early-out exposed it by depending on
correct `state.checkers` data.

**Manifestation:** In some positions, a pawn was giving check but
`state.checkers.is_empty()` returned `true`, causing the early-out to
incorrectly accept moves.

**Fix:** Added the missing pawn attacks to `compute_checkers()`:

```rust
| (attacks::pawn_attacks(us, ksq)
    & self.by_type[PieceType::Pawn as usize]
    & self.pieces_color(them))
```

This was inserted after the knight attack check, in the per-commoner
checker-computation loop.

---

## Verification Results

### Unit tests (`cargo test`)

All 46 tests pass:
- 41 unit tests (attacks, bitboard, board, movegen, magic, pext, types)
- 4 perft regression tests (depth 1-4)
- 1 doc-test

### Full perft regression (`cargo run --release --example verify_perft 6`)

All 41 test positions pass at depths 1-6, producing exact counts matching
`perft_values.md`.

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Total time | 188.246 s | 124.380 s | **−33.9 %** |
| Average per test | 4.591 s | 3.034 s | **−33.9 %** |
| Fastest test | 0.002 s | 0.002 s | — |
| Slowest test | 31.722 s | 19.419 s | **−38.8 %** |
| All 41 positions | PASS | PASS | ✓ |

---

## Deviations from Plan

| Item | Planned | Actual | Reason |
|------|---------|--------|--------|
| `populate_state()` | `count() as u32` | `count()` (no cast) | Clippy warning — `count()` returns `u32` |
| Early-out condition | No `commoners_count > 0` | Added `commoners_count > 0` | Bugfix: previous move may have destroyed last commoner |
| `compute_checkers()` | No change | Added pawn attacks | Bugfix: missing pawn check detection exposed by early-out |
| Plan doc update | N/A | This report added | Document deviations and bugs found |

---

## File Changes

| File | Lines | Change |
|------|-------|--------|
| `src/board.rs:362-364` | +3 | Add pawn attacks to `compute_checkers()` |
| `src/board.rs:422-431` | +9 | Add `populate_state()` method |
| `src/board.rs:570-572` | +3 | Call `self.populate_state(state)` at end of `do_move()` |
| `src/board.rs:676-707` | +22 | Rename `_state` → `state`; compute `pt`, `is_capture` early; add early-out |
| `src/board.rs:788-790` | -3 | Remove duplicate `let is_capture = ...` (now early) |
| `src/board.rs:833-834` | +2 / -2 | Use `state.commoners_count`, `state.them_commoners_count` |
| `src/movegen.rs:230-231` | +2 | Make `state` mutable, call `board.populate_state(&mut state)` |
| **Total** | **+42 -8** | across 2 files |

---

## Debugging Journey

1. **Initial run:** depth-4 perft failed for positions 2-12. Depth 1-3
   passed. The `got` values were consistently higher than `expected`,
   meaning the early-out was accepting illegal moves.

2. **Cause isolation:** Disabling the early-out made all tests pass,
   confirming the bug was in the early-out condition, not in the
   `populate_state` / cached-counts changes.

3. **Bug 1 (zero commoners):** Using `perft_divide` to compare per-root-move
   counts between old and new code for position 2, we found that after
   `d1f3` (Queen moves to f3) followed by `f3f7` (Queen captures f7), the
   resulting position had Black with no commoners (destroyed by the blast).
   The early-out was incorrectly returning `true` for Black's moves even
   though Black had no commoners and the game was over.

4. **Bug 2 (pawn checks):** After fixing Bug 1, positions 9 and 11 still
   failed. Inspection of `compute_checkers()` revealed it never checked
   for pawn attacks — a pre-existing bug in a formerly-dead code path.

5. **Fix verification:** After both fixes, all 46 tests pass and all 41
   perft positions match at depths 1-6.

---

## Risk Assessment

| Risk | Outcome | Notes |
|------|---------|-------|
| `populate_state()` cost > savings | **Mitigated** — 33.9 % net speedup | The early-out savings far outweigh the per-move cost of `populate_state()`. |
| Early-out fires incorrectly | Two bugs found and fixed | Both documented above; all 41 perft positions verified at depths 1–6. |
| Regressions in existing tests | None | All 46 tests pass. |

---

## Performance Measurement

Measured using `cargo run --release --example verify_perft 6` on Apple
Firestorm (arm64). The `verify_perft` runner runs all 41 test positions
at depths 1–6 and reports wall-clock time per test.

### Before (commit 9e9d1be)

```
--- Summary ---
  Average:     4.591 s per test
  Total time:  188.246 s
```

### After (this implementation)

```
--- Summary ---
  Average:     3.034 s per test
  Total time:  124.380 s
```

### Speedup by test weight

The heaviest tests (positions with many moves) showed the largest absolute
gains:

| Test | Before | After | Δ |
|------|--------|-------|---|
| #2  (depth-6 heavy) | 25.325 s | 16.177 s | −9.148 s |
| #13 (depth-6 heavy) | 31.722 s | 19.419 s | −12.303 s |
| #16 | 11.426 s | 6.874 s | −4.552 s |
| #22 | 13.926 s | 8.639 s | −5.287 s |
| #24 | 12.230 s | 8.503 s | −3.727 s |
| #33 | 25.024 s | 16.969 s | −8.055 s |

The speedup scales with position complexity (more nodes → more `legal()`
calls → greater early-out benefit), matching expectations.

### Interpretation

The 33.9 % total reduction falls within the plan's estimated 30–50 %
range. The lower end of the estimate was hit, likely because:

1. **Two bugfixes added overhead:** The `compute_checkers()` pawn-attack
   fix adds a bitboard operation per commoner, and the
   `commoners_count > 0` check is a no-op branch.
2. **`populate_state()` is called in `generate_legal()` too:** Every root
   position now pays the cost of `compute_checkers` + `compute_pinned`,
   which was not previously accounted for in the estimate.
3. **Non-capture fraction varies:** Positions with many captures or king
   moves get less early-out benefit.

Full measurement data in
[`docs/perf/m1/2026-06-30.txt`](../../perf/m1/2026-06-30.txt).

---

## Relationship to Other Items

- **Item 2 (early-out):** Implemented simultaneously in this plan.
- **Item 3 (cache commoner bitboards):** Independent follow-up.
- **Item 5 (precomputed `between_bb`):** Would speed up `compute_pinned()`
  itself, reducing the cost of `populate_state()`.
