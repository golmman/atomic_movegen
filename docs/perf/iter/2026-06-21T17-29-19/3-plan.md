# Iteration 2 Plan — `blockersForKing` + safe early-out for non-capture moves

| Risk | Impact | Effort |
|------|--------|--------|
| Medium | 20–50 % | ~100 lines |

## Problem

`legal()` performs the full pseudo-royal attack scan for every
pseudo-legal move, even when the move is trivially safe. For
non-capture, non-commoner moves where there are no checkers and the
piece is not a blocker, the move cannot create a discovered attack and
is always legal.

The current `compute_pinned()` returns only pieces that are the **sole**
blocker on a ray. But a piece that is one of several blockers also
cannot be moved without exposing the commoner — this is called
`blockers_for_king` in Stockfish and is a superset of `pinned`.

## Changes

### Step 1 — StateInfo: add `blockers_for_king` field

In `types.rs`, add `blockers_for_king: Bitboard` to `StateInfo`.

### Step 2 — `compute_blockers_for_king()` replaces `compute_pinned()`

In `board.rs`:
- Rename `compute_pinned()` → `compute_blockers_and_pinned()` that
  returns a tuple `(blockers_for_king, pinned)`.
- `blockers_for_king` = ALL pieces between any commoner and any enemy
  slider (change `between.count() == 1` → `!between.is_empty()`).
- `pinned` = sole blockers (keep the `between.count() == 1` check).

### Step 3 — Update `populate_state()`

Store both `blockers_for_king` and `pinned` in the state info.

### Step 4 — Early-out in `legal()`

After the self-explosion check, add:

```rust
if state.checkers.is_empty()
    && !is_capture
    && m.move_type() != MoveType::EnPassant
    && self.piece_on(from).type_of() != PieceType::Commoner
    && (state.blockers_for_king & Bitboard::square_bb(from)).is_empty()
{
    return true;
}
```

### Step 5 — Remove stale `pinned` method

The `Board::pinned(c)` method calls `compute_pinned(c)` which no longer
exists. Replace it to use the new `compute_blockers_and_pinned`.

## Verification

- `cargo test` — all unit tests pass
- `cargo clippy` — no new warnings
- `cargo fmt` — formatting clean
- `cargo run --release --example verify_perft 5` — all 41 positions
- `cargo run --release --example verify_perft 6` — deeper verification
