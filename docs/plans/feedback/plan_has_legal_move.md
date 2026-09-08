# Plan: `has_legal_move` — early-exit legal-move existence query

Standalone plan for the `atomic-movegen` repository
(https://github.com/golmman/atomic_movegen, current release 2.1.0).
Written by the `atomic_solver` project, which consumes this crate and is
blocked on this feature; the background section explains why. Follow the
movegen repo's own AGENTS.md and conventions wherever they conflict with
anything suggested here.

## Background (why the consumer needs this)

The consuming solver evaluates ~19× more child positions than it searches.
For every evaluated child it runs a full
`generate_legal_with_state` (plus `populate_state`) **only to learn the
boolean "does this position have any legal move?"**; the move list itself
is discarded for the ~95% of children that are never searched. Profiling
the solver shows `generate_legal_with_state` + `Board::legal` at ~58% of
runtime samples. An existence query that stops at the *first* legal move
removes most of that cost on the consumer side.

The query must be implemented inside this crate because legality
(`Board::legal`, blast zones, pins, pseudo-royal adjacency) lives here.

## Scope

Additive only. Target release: **2.2.0** (semver-minor; no existing
signature, behavior, or output-order change).

New public API in `movegen.rs`, mirroring the existing
`generate_legal` / `generate_legal_with_state` convention:

```rust
/// Returns `true` if the side to move has at least one legal move.
///
/// Early-exits at the first legal move; substantially cheaper than
/// `generate_legal` when moves exist. Calls `populate_state` internally.
pub fn has_legal_move(board: &Board) -> bool;

/// Same as [`has_legal_move`], using a caller-populated [`StateInfo`].
///
/// The caller is responsible for ensuring `state` is up to date (e.g. by
/// calling [`Board::populate_state`]) before invoking this function. It
/// does not call `populate_state` itself.
pub fn has_legal_move_with_state(board: &Board, state: &StateInfo) -> bool;
```

### Equivalence contract (the correctness gate)

For any position and any populated `StateInfo`:

```text
has_legal_move_with_state(b, s) == { generate_legal_with_state(b, s, &mut list); list.len() > 0 }
```

The two must agree on **every** position, including all edge cases that
fall out of `Board::legal`:

- `commoners(us).is_empty()` ⇒ `false` (every candidate is rejected by
  the `our_commoners.is_empty()` guard in `Board::legal`).
- Positions where the only legal move is a capture, en passant,
  promotion, or castling ⇒ the fast acceptance path must not fire for
  them; the full `Board::legal` check decides.
- Positions in check ⇒ quiet moves are not automatically legal
  (`is_move_trivially_legal` returns `false` when `checkers` is
  non-empty); the check decides.
- Last-commoner adjacency immunity and self-explosion rules are
  inherited from `Board::legal` — the new function must not re-derive
  any legality rule itself.

## Design

1. **Shared candidate stream.** Factor the body of
   `generate_pseudo_legal` (src/movegen.rs) into an internal
   `for_each_pseudo_legal(board, f: impl FnMut(Move) -> ControlFlow<B>)
   -> ControlFlow<B>` that walks the same stages in the same order:
   pawn moves (single/double/captures/promotions/en passant, per pawn)
   → knights → bishops → rooks → queens → commoners → castling.
   `generate_pseudo_legal` becomes a thin wrapper collecting into the
   `MoveList`.
   **Hard requirement: the collected move sequence must be
   bit-identical to today's output** (same moves, same order). Consumers
   depend on generation order for move ordering and reproducible
   searches. Guarded by the order-golden test below.
2. **`has_legal_move_with_state` drives the same stream** with a closure
   that, per candidate, applies exactly the filter
   `generate_legal_with_state` applies —
   `is_move_trivially_legal(board, m, state) || board.legal(m, state)` —
   and returns `ControlFlow::Break` on the first accepted candidate.
   Correctness is inherited from the unchanged filter; the only new
   logic is the early exit.
   Explicit early-out before the stream: if
   `board.commoners(board.side_to_move()).is_empty()`, return `false`.
3. **Optional fast pre-pass (measure first, add only if it pays).** A
   dedicated scan that accepts the first *quiet* pseudo-legal move by an
   unpinned non-commoner piece without calling `is_move_trivially_legal`
   per candidate can save the closure-call overhead. If added, it must
   accept only moves `is_move_trivially_legal` would accept, and the
   equivalence test must cover positions where it short-circuits
   (quiet-move-rich middlegames) and where it must not (in check,
   captures-only, all-pinned). Do not add it speculatively; benchmark
   with and without.
4. **`Board::outcome` / `Board::is_terminal` are out of scope**, as is a
   resumable generator API. Keep the diff minimal and additive.
5. **No new dependencies**, including dev-dependencies (the crate is
   currently dependency-free; keep it that way). The property-test
   playout generator must be hand-rolled and deterministic (e.g. a
   seeded xorshift/LCG in test code), walking random legal moves from a
   set of start FENs via `do_move`/`undo_move`.

## Implementation steps

### Step 0 — Order golden (before any refactor)

Add `tests/move_order_golden.rs` (or extend an existing test file per
repo conventions): for a fixed list of positions — the start position,
several middlegame/endgame FENs, promotion and en-passant positions,
positions in check, castling-available positions — record the exact
`generate_pseudo_legal` and `generate_legal` output as UCI strings in
order, and assert future output against it. Generate the goldens from
the current code and commit them *before* the refactor. This test is
the guard against stream-refactor drift; perft checks counts, not
order.

### Step 1 — Candidate-stream refactor

Perform the `for_each_pseudo_legal` refactor. Gates:

- All existing tests pass (`cargo test`, perft suite, examples still
  compile/run).
- The order-golden test passes unchanged.
- `cargo clippy` / `cargo fmt` clean per repo conventions.

### Step 2 — The query

Implement `has_legal_move` / `has_legal_move_with_state` per the design.
New tests:

- **Equivalence property test:** deterministic random playouts (multiple
  seeds, several thousand positions total, varied start FENs including
  capture-heavy lines — explosions change occupancy mid-`legal()`) — at
  each position, `populate_state`, then assert the contract above using
  two independently generated move lists.
- **Deterministic fixtures** (each defined by what `generate_legal`
  returns, so the test remains self-validating): bare checkmate,
  stalemate, in-check with exactly one escape, castling as only legal
  move, en passant as only legal move, promotion-heavy position,
  extinction (zero own commoners), bare-commoners K vs K, a position
  where every pseudo-legal move is by a pinned piece.
- Assert `has_legal_move(b)` (convenience wrapper) agrees with the
  `_with_state` variant on the same fixtures.

### Step 3 — Benchmark

A small `--release` timing loop (or criterion if the repo already uses
it) over: a quiet middlegame, an in-check position, a captures-only
position, a no-legal-move position, vs. `generate_legal` on the same
positions. Expected shape: quiet middlegame ≪ generation; no-legal-move
≈ one full generation (unavoidable — the stream must be exhausted to
prove absence). Record numbers for the CHANGELOG / release notes.

### Step 4 — Release

- `CHANGELOG.md` entry under a 2.2.0 heading describing the two new
  functions and the internal `for_each_pseudo_legal` refactor
  (behavior-preserving, order-verified).
- Version bump to 2.2.0, README API list update if it enumerates
  functions.
- Publish (or open the PR / hand the diff to the maintainer, per however
  this repo ships changes), and report back: the tag/version, test
  results, and the benchmark numbers from step 3.

## Report-back checklist for the implementing agent

When done, report: (1) version/tag published, (2) results of the
order-golden, equivalence, and perft gates, (3) benchmark table from
step 3, (4) any deviations from this plan with rationale, (5) the exact
public signatures as implemented.
