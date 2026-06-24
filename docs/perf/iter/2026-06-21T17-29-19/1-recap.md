# Iteration 2 Recap

## Prior iteration results

### Iteration 0 — Release profile tuning
Applied `lto = "fat"` and `codegen-units = 1` in `Cargo.toml`.

### Iteration 1 — Populate `StateInfo` in `do_move()` / `generate_legal()`

Added `populate_state()` helper, refactored `generate_legal()` into
`generate_legal_with_state()` (pub(crate)), and modified `legal()` to use
`state.commoners_count` / `state.them_commoners_count` instead of
recomputing them.

**Performance:** ~3 % regression (1.871 s → 1.927 s at depth 6). The
`populate_state()` overhead slightly exceeds savings from caching 2
counts. Accepted as architectural prerequisite for this iteration.

## What `generate_legal_with_state()` currently does

1. `board.populate_state(state)` — computes `checkers`, `pinned`,
   `commoners_count`, `them_commoners_count`.
2. `generate_pseudo_legal(board, moves)` — generates all pseudo-legal
   moves.
3. `moves.retain(|&m| board.legal(m, state))` — validates each move.

## What `legal()` currently does (hot path)

- Self-explosion check (6 bitboard ops)
- Pseudo-royal attack check (5 attack table lookups per surviving commoner)
