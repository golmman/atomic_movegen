# Iteration 1 Plan — Populate `StateInfo` in `do_move()`, use in `generate_legal()`

| Risk | Impact | Effort |
|------|--------|--------|
| Low  | 5–15 % | ~50 lines |

## Problem

`StateInfo` already has fields for `checkers`, `pinned`,
`commoners_count`, and `them_commoners_count` but they are never
populated. `do_move()` only fills `castling_rights`, `ep_square`,
`rule50`, and the capture array.

`generate_legal()` creates a fresh `StateInfo::new()` and passes it to
`legal()`, so every call to `legal()` recomputes checkers/pinners from
scratch.

## Fix (4 sub-steps)

1. **`populate_state()` helper + call in `do_move()`** — Add a
   `populate_state()` method on `Board` that computes checkers, pinned,
   commoners_count, them_commoners_count and stores them in a
   `StateInfo`. Call it at the end of `do_move()` (after
   `side_to_move` is flipped).

2. **Refactor `generate_legal()`** — Split into `generate_legal()`
   (public, creates its own state) and
   `generate_legal_with_state()` (pub(crate), accepts a pre-populated
   state). The latter calls `board.populate_state()` and passes the
   populated state to `legal()`.

3. **Use state fields in `legal()`** — Replace
   `self.commoners(us).count()` with `state.commoners_count` and
   `self.commoners(them).count()` with `state.them_commoners_count`.

4. **Update `perft()`** — Have `perft()` create a `StateInfo` at the
   top of the function and pass it to
   `generate_legal_with_state()`, so the same state handles both the
   legal generation and the do/undo cycle.

## Verification

- `cargo test` — all unit tests pass
- `cargo clippy` — no new warnings
- `cargo fmt` — formatting clean
- `cargo run --release --example verify_perft 5` — all 41 positions
- `cargo run --release --example verify_perft 6` — deeper verification
