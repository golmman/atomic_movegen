# Iteration 2 Implementation

## Changes

### `src/board.rs`

1. **Added `blockers_for_king` to `StateInfo`** (line ~36):
   - New field `pub blockers_for_king: Bitboard` — all pieces (both colors) between
     any own commoner and any enemy slider (rook/bishop/queen), not just sole blockers.
   - Initialized to `Bitboard::EMPTY` in `StateInfo::new()`.

2. **Replaced `compute_pinned()` with `compute_blockers_and_pinned()`** (line ~377):
   ```rust
   pub(crate) fn compute_blockers_and_pinned(&self, us: Color) -> (Bitboard, Bitboard) {
       // For each own commoner, find enemy snipers on empty-board attack rays.
       // All pieces on `between_bb(ksq, sniper_sq) & occupied` are blockers.
       // If exactly one piece is between, it's also pinned.
       // Returns (blockers_for_king, pinned).
   }
   ```
   - `blockers_for_king` = ALL pieces on the between ray (any count)
   - `pinned` = only pieces that are the SOLE blocker on their ray (unchanged semantics)
   - `pub fn pinned(&self, c: Color)` updated to return `.1` of new function.

3. **Added safe early-out in `legal()`** (line ~797):
   ```rust
   // When commoners_count > 1 (no pseudo-royal) and no checkers and
   // moving a non-commoner non-blocker non-capture, the move cannot
   // create discovered attacks or affect extinction. Return true.
   if state.checkers.is_empty()
       && !is_capture
       && piece_on(from).type_of() != Commoner
       && (blockers_for_king & from).is_empty()
       && state.commoners_count > 1
   {
       return true;
   }
   ```
   - `commoners_count > 1` is required: empirical evidence shows that the
     pseudo-royal check can reject moves even when all early-out conditions
     are met. Root cause still under investigation.

4. **Conditional `populate_state()`** (line ~416):
   - `state.commoners_count` and `state.them_commoners_count` always computed
   - `state.checkers`, `state.blockers_for_king`, `state.pinned` computed
     only when `commoners_count > 1` (they are only read by the early-out,
     which fires only in that case)
   - This avoids the cost of `compute_checkers()` for the common
     single-commoner case, keeping the starting-position overhead to ~1.6%.

### `src/movegen.rs`

5. `generate_legal_with_state()` unchanged from iteration 1.

### `src/lib.rs`

6. `perft()` unchanged from iteration 1.

### `tests/perft_tests.rs`

7. Added `perft_all_positions_depth_4()` — quick regression check for 12
   positions at depth 4 (total ~0.08 s).

## Debugging

The early-out without `commoners_count > 1` caused ~20/41 tests to fail at
depth 5, each with +16 to +468 extra nodes. The condition was narrowed to
`commoners_count > 1` which isolates the failure to the pseudo-royal
extinction path. The root cause remains a puzzle: theoretically, a
non-blocking, non-commoner, non-capture move with no checkers should not
affect the safety of a single remaining commoner.

## Verification results

- `cargo build` — compiles clean
- `cargo test` — 46/46 tests pass
- `cargo clippy` — no new warnings (17 pre-existing)
- `cargo fmt` — no changes needed
- `verify_perft 5` — 41/41 PASS
- `verify_perft 6` — 41/41 PASS
