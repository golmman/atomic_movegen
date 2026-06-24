# Iteration 1 Implementation

## Changes

### `src/board.rs`

1. **Added `populate_state()` method** (line ~408):
   ```rust
   pub(crate) fn populate_state(&self, state: &mut StateInfo) {
       let us = self.side_to_move;
       state.checkers = self.compute_checkers(us);
       state.pinned = self.compute_pinned(us);
       state.commoners_count = self.commoners(us).count();
       state.them_commoners_count = self.commoners(!us).count();
   }
   ```

2. **`do_move()` calls `populate_state()`** at the end, after
   `self.side_to_move = them` and `self.game_ply += 1`.

3. **`legal()` uses state fields** — Renamed `_state` → `state` and
   replaced:
   - `self.commoners(us).count()` → `state.commoners_count`
   - `self.commoners(them).count()` → `state.them_commoners_count`

### `src/movegen.rs`

4. **Refactored `generate_legal()`** — Split into:
   - `generate_legal()` (public) — creates `StateInfo`, delegates to
     `generate_legal_with_state()`
   - `generate_legal_with_state()` (pub(crate)) — calls
     `board.populate_state(state)`, then generates pseudo-legal moves
     and filters with `board.legal(m, state)`

### `src/lib.rs`

5. **`perft()` uses `generate_legal_with_state()`** — Creates
   `StateInfo` at the top of the function, passes it to both
   `generate_legal_with_state()` and the `do_move`/`undo_move` loop.

## Verification results

- `cargo build` — compiles clean
- `cargo test` — 45/45 tests pass
- `cargo clippy` — no new warnings (only pre-existing ones)
- `cargo fmt` — no changes needed (already formatted)
- `verify_perft 5` — 41/41 PASS
- `verify_perft 6` — 41/41 PASS
