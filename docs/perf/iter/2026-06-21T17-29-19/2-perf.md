# Iteration 2 — Baseline measurement

Baseline: iteration 1 result (1.927 s on starting position depth 6).

## Analysis

`legal()` runs for every pseudo-legal move (~35 per position). The
majority of these moves are non-captures (60–80 %), non-commoner, and
non-blocker. For these moves, the full pseudo-royal attack scan is
wasted — the move is trivially safe because:

- No checkers exist (checked via `state.checkers`).
- No blast occurs (non-capture).
- The piece is not a blocker for any commoner (cannot create a
  discovered attack).
- The piece is not a commoner itself (does not change pseudo-royal
  status).

Currently `legal()` does not distinguish these safe moves and runs the
full attack scan for every pseudo-legal move.

**Opportunity:** A 5-field check at the beginning of `legal()` can
short-circuit ~60 % of moves, avoiding the expensive pseudo-royal attack
scan entirely for those moves. The `state.checkers` and
`state.blockers_for_king` fields are already populated by
`populate_state()` from iteration 1, so the check is very cheap.

## Current hot spots in `legal()`

- `compute_pinned()` / `compute_checkers()` — called once per position
  via `populate_state()` (iteration 1).
- Pseudo-royal attack check (lines 789–855 of `board.rs`) — called for
  every move where `our_pr_count <= 1`. With the early-out, this is only
  needed for captures, commoner moves, and moves by blockers.
