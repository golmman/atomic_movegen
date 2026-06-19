# Plan: Incremental checkers/pinners in `StateInfo`

## Overview

Currently `Board::checkers()` and `Board::pinned()` recompute from scratch every time `.legal(m)` is called. Since `legal()` is called for every pseudo-legal move (20–50+ per position), and `checkers()`/`pinned()` each loop over commoners and scan rays, this is a significant repeated cost.

The fix: pre-compute `checkers` and `pinned` **once** in `do_move()`, store them in `StateInfo`, and reuse them in `legal()`.

## Current state

### `checkers()` (in `board.rs:318–360`)

Iterates over all own commoners, computes rook/bishop/queen/knight attacks from each using the current occupied bitboard.

### `pinned()` (in `board.rs:362–391`)

Iterates over all own commoners, finds enemy sliders on the same ray, and checks whether exactly one piece lies between the commoner and the sniper (the pinned piece).

### Usage in `legal()` (in `board.rs:630–828`)

`legal()` does **not** call `checkers()` or `pinned()` directly — instead it duplicates the same ray-scanning and attack logic inline. This means the expensive computations happen twice: once if `checkers()`/`pinned()` were called by the move generator, and again inside `legal()`.

## Proposed change

### Step 1: Add fields to `StateInfo`

```rust
pub struct StateInfo {
    pub castling_rights: u8,
    pub ep_square: Option<Square>,
    pub rule50: u8,
    pub captured_count: u8,
    pub captured: [(Square, Piece); 9],
    pub cap_sq: Option<Square>,
    pub cap_piece: Piece,
    // NEW:
    pub checkers: Bitboard,
    pub pinned: Bitboard,
    pub commoners_count: u32,     // number of own commoners (for pseudo-royal check)
    pub them_commoners_count: u32, // number of enemy commoners (for pseudo-royal check)
}
```

### Step 2: Compute incrementally in `do_move()`

After the board state has been updated (pieces moved, blast applied, all state changes done), compute `checkers` and `pinned`:

```rust
// At end of do_move(), after self.side_to_move has been flipped to "them":
let us = self.side_to_move; // the side that will move next
let them = us.flip();

// Recompute checkers for the new side to move (us)
state.checkers = self.compute_checkers(us);
state.pinned = self.compute_pinned(us);

// Count commoners for pseudo-royal rule
state.commoners_count = self.commoners(us).count();
state.them_commoners_count = self.commoners(them).count();
```

### Step 3: Factor out the computation logic

Extract the body of `checkers()` into `pub(crate) fn compute_checkers(&self, us: Color) -> Bitboard` and the body of `pinned()` into `pub(crate) fn compute_pinned(&self, us: Color) -> Bitboard`.

These are **not** public API — they exist only for incremental computation during `do_move()`.

### Step 4: Simplify `legal()` to use stored values

Remove the duplicated attack-scanning loops in `legal()` and replace with the stored `StateInfo` fields:

- **Blast adjacency immunity:** still needs to check adjacency of enemy commoners (pre-blast positions), which is available via `self.commoners(them)`.
- **Pin check:** Instead of recomputing pinned pieces, check whether the moving piece is in `state.pinned`. If pinned, the move is only legal if it is along the ray between the commoner and the pinner.

Actually, `legal()` does not currently use `pinned()` either — it reconstructs the post-move board and checks if any commoner is under attack. This is a fundamentally different approach. Let me re-analyse:

#### How `legal()` works today

`legal()` simulates the move on a bitboard level (without mutating the board) and then checks whether any surviving commoner of the moving side is attacked by an enemy piece (given the post-blast occupied bitboard). This is effectively a "check detection" after the move.

#### Proposed simplification

Since `state.checkers` gives the pre-move checkers, we can **short-circuit**: if there are no checkers and the moving piece is not pinned, and it is not a commoner move, then most moves are trivially legal and we can skip the expensive post-move attack analysis.

The detailed logic:

1. **Self-explosion check** (new commoner count after move) — still needed.
2. **If no checkers and no pins** (state.checkers.is_empty() && state.pinned.is_empty()):
   - For any non-commoner move: legal (provided self-explosion passes).
   - For commoner moves: still need to verify no enemy commoner is adjacent after the move (extinction rule).
3. **If checkers exist:**
   - Must verify the move blocks or captures the checking piece.
   - This is a simpler check than a full attack scan.
4. **If pinned:**
   - Must verify the move stays on the pin ray.

For atomic chess specifically, the blast complicates things (the pinned piece can capture the pinner if the blast destroys it), so the simplified logic needs careful handling. However, the stored `checkers`/`pinned` still lets us skip the full recomputation in many cases.

### Minimal first step

The simplest safe change: add `checkers` and `pinned` to `StateInfo`, compute them at the end of `do_move()`, and then in `legal()` use these stored values as an **early-out** optimisation:

```rust
pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
    // ... existing checks for occupancy, self-explosion, etc ...
    
    // EARLY OUT: if the position had no checkers and no pins before the move,
    // and the move is not a commoner move, and doesn't create self-explosion,
    // then it's legal (the blast cannot create new attackers out of nothing
    // for the opponent).
    if state.checkers.is_empty() && state.pinned.is_empty() {
        let moving_piece = self.piece_on(m.from_sq());
        if moving_piece.type_of() != PieceType::Commoner {
            // Verify self-explosion handled above, then return true.
            // ... (with careful handling for the specific move type)
        }
    }
    
    // Fall through to full check for commoner moves and positions with pins/checkers.
}
```

## Reference implementation: Fairy-Stockfish

Fairy-Stockfish serves as the correctness oracle and performance reference for this optimisation.

### Location
- **`Fairy-Stockfish/src/position.h`** (lines 62–69, 266–270, 1350–1364): `StateInfo` fields (`checkersBB`, `blockersForKing[COLOR_NB]`, `pinners[COLOR_NB]`, `checkSquares[PIECE_TYPE_NB]`) and their accessors.
- **`Fairy-Stockfish/src/position.cpp`** (lines 576–614): `set_check_info()` — recomputes `blockersForKing`, `pinners`, and `checkSquares` after each `do_move()`.
- **`Fairy-Stockfish/src/position.cpp`** (lines 849–926): `slider_blockers()` — core pinned-piece detection used by `set_check_info()`.
- **`Fairy-Stockfish/src/position.cpp`** (lines 2092–2114): `do_move()` — calls `set_check_info()` and sets `checkersBB`.

### Key design points from Fairy-Stockfish

1. **Stored in StateInfo:** `checkersBB`, `blockersForKing[2]`, `pinners[2]`, and `checkSquares[PIECE_TYPE_NB]` are fields of `StateInfo`, computed once after the board state is updated and reused many times during move generation.

2. **Full recompute, not truly incremental:** Fairy-Stockfish does **not** incrementally update these fields — it calls `set_check_info(st)` which does a full `slider_blockers()` scan and recomputes `checkSquares` for every piece type. The optimisation is that this happens **once per `do_move()`** rather than once per pseudo-legal move.

3. **`checkersBB` from `givesCheck`:** The call site (`search.cpp`) computes `bool givesCheck = pos.gives_check(m)` and passes it to `do_move()`. This saves the full `attackers_to()` call when the move trivially cannot give check (e.g., non-capture pawn push). In our perft-focused context, we can compute `checkersBB` unconditionally at the end of `do_move()`.

4. **`checkSquares[pt]` for fast legality:** For each piece type, Fairy-Stockfish precomputes the squares from which that piece type would give check to the king. This allows `legal()` to quickly test "could this piece type, moving to this square, check the king?" — a critical optimisation for evasion move generation.

5. **Extinction royal handling** (`set_check_info` lines 578–614): For atomic chess, `pseudoRoyalCandidates` and `pseudoRoyals` bitboards are computed alongside the standard blockers/pinners. This tracks which commoners are pseudo-royal (those with count ≤ threshold+1) for the move legality check.

### Applicability to this plan
- The `checkersBB`, `blockersForKing[2]`, `pinners[2]` fields in Fairy-Stockfish's `StateInfo` directly map to our proposed `checkers` and `pinned` fields.
- Fairy-Stockfish's `set_check_info()` called at the end of `do_move()` is the pattern for our incremental computation.
- The extinction-royal-specific logic in Fairy-Stockfish's `set_check_info()` is the reference for our `commoners_count` / `them_commoners_count` tracking.
- Fairy-Stockfish's `check_squares[pt]` optimisation could be added in a future iteration to further speed up `legal()` for checking-piece blocking.

## Files to modify

| Action | File | Description |
|--------|------|-------------|
| Edit | `src/board.rs` | Add fields to `StateInfo`, compute in `do_move()`, factor out helpers, use in `legal()` |
| Edit | `src/movegen.rs` | Pass `&StateInfo` to `legal()` if signature changes |

Note: `legal()` currently takes `&self` (no `StateInfo`). If we want `legal()` to use the stored `checkers`/`pinned`, it needs access to `&StateInfo`. The call in `movegen.rs` becomes:

```rust
pub fn generate_legal(board: &Board, state: &StateInfo, moves: &mut Vec<Move>) {
    generate_pseudo_legal(board, moves);
    moves.retain(|&m| board.legal(m, state));
}
```

This is a breaking change but `generate_legal` is internal API (not `pub` to end users beyond the perft example).

## Performance impact

- In positions with zero or one checkers and few pins (which is the vast majority of positions in the search tree), the expensive attack-scanning in `legal()` can be skipped entirely for non-commoner moves.
- The pre-computation cost in `do_move()` is incurred once per move instead of once per pseudo-legal move, which is typically a 20–50× reduction.
- The memory cost is small: `Bitboard + Bitboard + u32 + u32` = 20 bytes extra per `StateInfo`.

## Verification

1. `cargo test` — all tests pass.
2. `cargo run --example verify_perft 5` — matches all reference values (depth 5 is sufficient to exercise check/pin positions heavily).
3. Profile before/after: compare `perf stat -e cycles:u` for `cargo run --example perft "FEN" 5` and verify cycle count drops.

## Future work

- Full incremental update (instead of recompute) could further optimise `do_move()` itself, but the recompute-once approach is simpler and already a major win.
- The early-out in `legal()` can be progressively strengthened as more cases are proven safe.
