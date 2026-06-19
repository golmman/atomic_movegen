# Plan: Fixed-size `StateInfo` (No-heap `captured_pieces`)

## Overview

Replace `StateInfo::captured_pieces: Vec<(Square, Piece)>` with a fixed-size array `[(Square, Piece); 9]` plus a `u8` length counter. This eliminates a heap allocation per `do_move` call, which is a major bottleneck in recursive perft — every single move in the search tree incurs an allocation and later a deallocation.

## Current state

```rust
pub struct StateInfo {
    pub castling_rights: u8,
    pub ep_square: Option<Square>,
    pub rule50: u8,
    pub captured_pieces: Vec<(Square, Piece)>,  // <-- heap alloc per move
    pub cap_sq: Option<Square>,
    pub cap_piece: Piece,
}
```

The blast can destroy at most the capturer + the 8 surrounding squares, but pawns are never removed. In the worst case, a capture on a central square with all 9 squares (capturer + 8 neighbours) occupied by non-pawn pieces means **at most 9 entries** in `captured_pieces`. For edge/corner captures it's even fewer.

## Proposed change

Replace with:

```rust
pub struct StateInfo {
    pub castling_rights: u8,
    pub ep_square: Option<Square>,
    pub rule50: u8,
    pub captured_count: u8,
    pub captured: [(Square, Piece); 9],
    pub cap_sq: Option<Square>,
    pub cap_piece: Piece,
}
```

### Changes in `StateInfo::new()`

```rust
pub fn new() -> Self {
    StateInfo {
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        captured_count: 0,
        captured: [(Square::NONE, NO_PIECE); 9],
        cap_sq: None,
        cap_piece: NO_PIECE,
    }
}
```

### Changes in `Board::do_move()`

Replace every `state.captured_pieces.push((sq, piece))` with:

```rust
state.captured[state.captured_count as usize] = (sq, piece);
state.captured_count += 1;
```

And replace `state.captured_pieces.clear()` with `state.captured_count = 0;`.

### Changes in `Board::undo_move()`

Replace the iteration:

```rust
for &(sq, piece) in state.captured_pieces.iter().rev() {
    self.place_piece(piece, sq);
}
```

with:

```rust
// Iterate in reverse order of insertion (blast victims restored last-in-first-out)
let mut i = state.captured_count;
while i > 0 {
    i -= 1;
    let (sq, piece) = state.captured[i as usize];
    self.place_piece(piece, sq);
}
```

## File changes

| Action | File | Description |
|--------|------|-------------|
| Edit | `src/board.rs` | Change `StateInfo` struct, `new()`, `do_move()`, `undo_move()` |

## Performance impact

- **No heap allocation** per `do_move` — `StateInfo` is entirely stack-allocated (currently ~120 bytes with the array vs ~56 bytes with a `Vec` + heap, but with zero alloc/free cost).
- In perft at depth 6 on the starting position (~119M nodes), this saves ~119 million `Vec::push` allocations and corresponding `Vec` drop deallocations.
- The stack-memory trade-off (9 elements × 12 bytes = 108 bytes for the array vs 24 bytes for the `Vec` on the stack + heap alloc) is negligible; the elimination of `malloc`/`free` dominates.

## Verification

1. `cargo test` — all existing tests pass.
2. `cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 4` yields 197326.
3. `cargo run --example verify_perft 3` passes all 41 positions.
4. Assert no `Vec::push` or `Vec::clear` remains in `do_move`/`undo_move` (grep check).

## Reference implementation: Fairy-Stockfish

Fairy-Stockfish serves as the correctness oracle and performance reference for this optimisation.

### Location
- **`Fairy-Stockfish/src/position.h`** (lines 43–90): `struct StateInfo` — entirely stack-allocated, no heap usage.
- **`Fairy-Stockfish/src/position.cpp`** (lines 1991–2069): Blast storage in `do_move()` — writes to fixed-size arrays.
- **`Fairy-Stockfish/src/position.cpp`** (lines 2142–2184): Blast restoration in `undo_move()` — reads from fixed-size arrays.

### Key design points from Fairy-Stockfish

1. **No heap allocation.** Fairy-Stockfish's `StateInfo` is a plain struct with no `std::vector`, `std::unique_ptr`, or any dynamic allocation. Captured pieces and blast victims are stored inline.

2. **Per-square blast storage:** Instead of a list, Fairy-Stockfish uses a fixed-size `Piece` array indexed by square:
   ```cpp
   Piece unpromotedBycatch[SQUARE_NB];  // per-square blast victim pieces
   Bitboard promotedBycatch;            // which squares had promoted pieces
   Bitboard demotedBycatch;             // which squares had demoted pieces
   ```
   This avoids any list/vector overhead — the square index itself encodes position, and the bitboard tracks which squares were affected.

3. **Single captured piece:** For non-blast captures, Fairy-Stockfish uses a single `Piece capturedPiece` field (not a list), since standard chess never has multiple captures in one move.

4. **Linked-list stacking:** `StateInfo* previous` links successive states into an implicit stack, with all `StateInfo` objects allocated on the search function's call stack (not heap). This is possible because the search depth is bounded.

### Applicability to this plan
- Our `captured: [(Square, Piece); 9]` is a Rust-idiomatic simplification of Fairy-Stockfish's per-square array. The constant 9 matches the maximum blast zone size (3×3 king-neighbourhood).
- Fairy-Stockfish's approach of zero heap allocation per `do_move()` is the gold standard we are matching.
- The `StateInfo* previous` linked-list pattern is not needed in Rust since perft recursion already passes `&mut StateInfo` by reference — our single reusable `StateInfo` in the perft loop (see `src/lib.rs:48`) achieves the same effect with less complexity.

## Edge cases considered

- **No capture**: `captured_count` stays 0, `undo_move` loop is skipped entirely.
- **En passant + blast**: In the worst case, EP removes the captured pawn (1 entry) + the blast up to 8 neighbouring non-pawn squares = 9 entries total. Perfectly fits.
- **Promotion capture + blast**: Same analysis — at most 9 non-pawn entries.
- **Non-capture**: `captured_count` is 0, no entries written.

The constant 9 is verified by proof: a 3×3 King-move blast zone has exactly 9 squares. Pawns in those squares are never added (guard: `if bpiece != NO_PIECE` in the blast loop). So 9 is the absolute upper bound.
