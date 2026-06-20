# Report: Incremental checkers/pinners in `StateInfo`

## Summary

Added `checkers`, `pinned`, `commoners_count`, `them_commoners_count` fields to
`StateInfo` and extracted `compute_checkers()` / `compute_pinned()` methods.
Changed `legal()` to accept `&StateInfo` and wired `generate_legal()` to pass a
state object. A conservative early-out was added and then removed after
benchmarking — see the Lessons Learned section.

**Status:** Infrastructure merged; the early-out optimisation was reverted
because it added more cost than it saved.

## Motivation

The perft hot path is:

```
perft(board, depth):
    generate_legal(board, &moves)      # calls legal() for each pseudo-legal move
    for each move:
        do_move(board, &state)
        total += perft(board, depth-1)
        undo_move(board, &state)
```

At depth 6, `generate_legal` calls `legal()` roughly 20–50 times per position.
Each `legal()` call scans slider attacks (`rook_attacks`, `bishop_attacks`,
`queen_attacks`), knight attacks, pawn attacks, *and* computes blast adjacency
from scratch. For positions with many pseudo-legal moves (40+), this adds up.

The plan: precompute `checkers` and `pinned` in `do_move()` (once) and reuse
them in `legal()` (many times), skipping the redundant attack scan for moves
that are trivially legal.

## Changes

### Files modified

| File | Lines changed | Description |
|------|---------------|-------------|
| `src/board.rs` | ~30 insertions, ~40 deletions | `StateInfo` fields, `compute_*` extraction, `legal()` signature, early-out |
| `src/movegen.rs` | ~5 insertions, ~3 deletions | `StateInfo` import, state creation in `generate_legal()` |

### `StateInfo` fields added

```rust
pub struct StateInfo {
    // … existing fields …
    pub checkers: Bitboard,
    pub pinned: Bitboard,
    pub commoners_count: u32,
    pub them_commoners_count: u32,
}
```

### `compute_checkers(us: Color)` and `compute_pinned(us: Color)`

Extracted as `pub(crate)` methods from the existing `checkers()` and `pinned()`.
The public methods now delegate:

```rust
pub fn checkers(&self) -> Bitboard {
    self.compute_checkers(self.side_to_move)
}
pub fn pinned(&self, c: Color) -> Bitboard {
    self.compute_pinned(c)
}
```

### Bugfix in pin-detection

The original `pinned()` contained an undefined-behaviour bug triggered when
`between` (the set of pieces between a commoner and an enemy slider) was empty.
The expression `!between.more_than_one()` called `self.0 & (self.0 - 1)` which
underflows in debug mode when `self.0 == 0`. Fixed by using
`between.count() == 1` instead. This bug was latent — it never fired before
because `pinned()` was rarely called. Our change calls it from
`generate_legal()`, which exercised the path.

### `legal()` signature change

```rust
// Before
pub fn legal(&self, m: Move) -> bool
// After
pub fn legal(&self, m: Move, _state: &StateInfo) -> bool
```

The `_state` parameter is currently unused (early-out removed). It exists as
infrastructure for future optimisations.

### `generate_legal()` change

```rust
pub fn generate_legal(board: &Board, moves: &mut Vec<Move>) {
    let state = StateInfo::new();
    generate_pseudo_legal(board, moves);
    moves.retain(|&m| board.legal(m, &state));
}
```

No signature change — all callers are unaffected. The `StateInfo` is created
once per position (not per move) and passed through to `legal()`.

## Early-out — attempted and reverted

### Attempt 1: All non-capture non-commoner moves

Condition: `no checkers && no pins && piece.type != Commoner && !is_capture`

Result: **perft mismatch** — overcounted nodes at depths ≥ 4. The blast from a
capture can destroy multiple enemy blockers simultaneously, clearing a line for
an enemy slider to attack our commoner even when no single piece was pinned.

### Attempt 2: Non-capture non-commoner + is_capture guard

Added `is_capture` check to exclude captures from the early-out. Still
overcounted by a smaller margin — some non-capture non-commoner moves with
exactly 1 pseudo-royal commoner were still being accepted incorrectly. The root
cause remains unclear; possible that some geometric configuration of the board
allows a non-capture move to create a discovered check through a mechanism not
captured by `pinned`.

### Attempt 3: Only fire when `commoners_count >= 2`

Restricted to positions with ≥2 commoners (where the pseudo-royal check is
already skipped by the original code). This passed all 41 perft tests at
depth 5, but saved practically nothing — it shortcuts a cheap `count()` and a
never-taken `if`-block, not the expensive attack scan.

### Removed entirely

Benchmarking showed the `compute_checkers()` + `compute_pinned()` calls in
`generate_legal()` cost more than the early-out saved. The `do_move()`
computation was also redundant (`generate_legal()` created a fresh state).
Both were removed.

## Performance

Benchmarks from `cargo run --release --example verify_perft 5` (all 41
positions to depth 5):

| Version | Total time | Notes |
|---------|------------|-------|
| Before any changes | ~4.4 s | Baseline (no StateInfo, no compute) |
| With checkers/pinned computed in both `do_move()` and `generate_legal()` + early-out | ~4.8 s | +9% regression |
| After removing redundant computations and early-out | ~4.45 s | +1% (within noise) |

The final code is within measurement noise of the baseline. The `StateInfo`
creation in `generate_legal()` is a small struct on the stack — negligible
overhead.

## Correctness

| Verification | Result |
|-------------|--------|
| `cargo test` | 34/34 passed |
| `cargo run --release --example verify_perft 5` | 41/41 positions passed |
| `cargo clippy` | Clean (no new warnings) |
| `cargo fmt` | Clean |

## Lessons learned

### Early-out pitfalls for atomic chess

1. **Blast can clear multiple blockers.** A single capture blast can destroy
   several non-pawn pieces on different squares simultaneously. Even though no
   single piece was the "sole blocker" (i.e., pinned), the blast can eliminate
   every piece on a ray at once, leaving the commoner exposed.

2. **The `commoners_count >= 2` guard is self-defeating.** The pseudo-royal
   attack scan only runs when `our_pr_count <= 1`. So an early-out that
   requires ≥2 commoners saves no significant work — it skips the cheap
   guard check, not the expensive scan.

3. **Non-capture non-commoner moves with 1 commoner are NOT trivially safe.**
   Perft testing revealed overcounts that cannot be explained by any mechanism
   we've identified (discovered check via pinned piece, blast clearing lines,
   or commoner adjacency changes). There is likely a subtle interaction with
   the extinction pseudo-royal rule that requires further study.

### Infrastructure is still valuable

Even though the early-out was removed, the infrastructure work is not wasted:

- `compute_checkers()` / `compute_pinned()` are cleanly extracted and reusable
- `legal()` accepting `&StateInfo` is ready for future optimisations
- The pin-detection bugfix (`between.count() == 1`) prevents future UB

### Measurement methodology

Early micro-benchmarks of `legal()` in isolation showed a 30–50% speedup from
the early-out. But the overall perft time *increased* because:

1. The `compute_checkers()`/`compute_pinned()` cost in `generate_legal()` was
   not accounted for
2. The early-out fired on a small fraction of moves in real positions
   (many positions have captures, promotions, or commoner moves)
3. The redundant `do_move()` computation doubled the overhead

Always measure end-to-end perft, not isolated micro-benchmarks.

## Future work

- **Full integration:** The original vision of replacing `legal()`'s attack scan
  with cached `state.checkers`/`state.pinned` is still sound. The challenge is
  that `legal()` needs *post-move* checkers (after blast/piece movement), while
  the cached values are *pre-move*. Computing post-move checkers incrementally
  (by adjusting the pre-move set for the specific piece that moved and the blast
  that occurred) could eliminate the full scan.

- **`check_squares[pt]` pattern:** Fairy-Stockfish stores, for each piece type,
  the set of squares that would give check to the king if a piece of that type
  were on them. This allows `legal()` to quickly test "does this square give
  check?" without scanning all rays.

- **Lazy computation:** Instead of computing checkers/pinned eagerly in
  `generate_legal()`, compute them lazily on first access and cache the result.
  If the early-out never fires for a particular position, the computation is
  never done.
