# Plan 1: Precompute State in `do_move()` (with Early-Out)

## Goal

Eliminate redundant recomputation within `Board::legal()` by populating
`StateInfo` fields at the end of `do_move()`. Combine with the early-out
from analysis Item 2 (they are interdependent — the savings come from the
early-out, which depends on precomputed state).

**Estimated speedup:** 30–50 % (Items 1 + 2 combined, per analysis §Summary).

---

## Background

### Current situation

| Aspect | Current behaviour |
|--------|------------------|
| `StateInfo.checkers` | Declared, never written, never read (dead field). |
| `StateInfo.pinned` | Declared, never written, never read (dead field). |
| `StateInfo.commoners_count` | Declared, never written, never read (dead field). |
| `StateInfo.them_commoners_count` | Declared, never written, never read (dead field). |
| `legal(&self, m, _state)` | Computes `self.commoners(us).count()` and `self.commoners(them).count()` from raw bitboards every call. |
| `legal()` attack loop | Runs full pseudo-royal scan for every move, no early exit for safe moves. |
| `generate_legal()` | Creates throwaway `StateInfo::new()` (all-empty) just to satisfy `legal()` signature. |

### Target state

| Aspect | Target behaviour |
|--------|-----------------|
| `StateInfo.checkers` | Populated by `do_move()` at end, read by `legal()` early-out. |
| `StateInfo.pinned` | Populated by `do_move()` at end, read by `legal()` early-out. |
| `StateInfo.commoners_count` | Populated by `do_move()` at end, used by `legal()` to skip `self.commoners(us).count()`. |
| `StateInfo.them_commoners_count` | Populated by `do_move()` at end, used by `legal()` to skip `self.commoners(them).count()`. |
| `generate_legal()` | Populates state at entry (root position) via `populate_state()`. |
| `legal()` early-out | Returns `true` early for safe moves using `state.checkers`, `state.pinned`. |

---

## Implementation Steps

### Step 1 — Add `Board::populate_state(&self, state: &mut StateInfo)` method

```rust
/// Fill cached state fields (checkers, pinned, commoner counts) for the
/// current position so that `legal()` can read them instead of recomputing.
pub fn populate_state(&self, state: &mut StateInfo) {
    state.checkers = self.compute_checkers(self.side_to_move);
    state.pinned = self.compute_pinned(self.side_to_move);
    state.commoners_count = self.commoners(self.side_to_move).count() as u32;
    state.them_commoners_count = self.commoners(self.side_to_move.flip()).count() as u32;
}
```

**File:** `src/board.rs`, inserted before `do_move()` (after `pinned()` method, ~line 420).

**Note:** `compute_checkers` and `compute_pinned` already exist as `pub(crate)` methods on `Board`. This method simply wraps them and stores the results into `StateInfo`.

---

### Step 2 — Call `populate_state()` at the end of `do_move()`

Insert just before the closing brace of `do_move()` (after `self.game_ply += 1`):

```rust
// Populate cached state for the new position
self.populate_state(state);
```

**File:** `src/board.rs`, at end of `do_move()` (after line 560).

**Why here:** By this point `self.side_to_move` has been flipped to the
opponent, so `populate_state` computes checkers/pinned/counts from the
*new* side-to-move's perspective — exactly what `legal()` will need when
it is called for the next half-move.

**No change to `undo_move()`:** The undo path does not read these fields;
uses only `captured[]`, `cap_sq`, `cap_piece`, `castling_rights`,
`ep_square`, `rule50`. The stale checkers/pinned values left in `state`
after undo are harmless — they will be overwritten by the next `do_move()`.

---

### Step 3 — Populate state in `generate_legal()`

The root-position (and any position that hasn't been reached via `do_move`)
has no pre-populated `StateInfo`. Fix `generate_legal()` to populate one:

```rust
pub fn generate_legal(board: &Board, moves: &mut Vec<Move>) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);                     // NEW
    generate_pseudo_legal(board, moves);
    moves.retain(|&m| board.legal(m, &state));
}
```

**File:** `src/movegen.rs`, around line 230–234.

This adds the cost of computing checkers/pinned/counts once per position.
Without the early-out (Step 4) this would be a net regression (~1–3 %).
With the early-out, `legal()` becomes much cheaper on average (especially
for non-capture moves, which are the majority).

---

### Step 4 — Use cached counts and add early-out in `legal()`

Two sub-steps:

#### 4a. Replace `commoners(us).count()` calls with state fields

```rust
// Replace lines 803-804:
//   let our_pr_count = self.commoners(us).count();
//   let them_pr_count = self.commoners(them).count();
let our_pr_count = state.commoners_count as usize;
let them_pr_count = state.them_commoners_count as usize;
```

**File:** `src/board.rs`, lines 803–804.

#### 4b. Add early-out for safe moves (Item 2)

Insert at the top of `legal()`, after the `NO_PIECE` check (after line 672)
and before the castling block (line 677). This requires computing `pt` and
`is_capture` early (they were previously computed later in the function):

```rust
let piece = self.piece_on(from);
if piece == NO_PIECE {
    return false;  // existing check
}
let pt = piece.type_of();

// Pre-compute is_capture (needed for both early-out and later logic)
let is_capture = m.move_type() != MoveType::Castling
    && (m.move_type() == MoveType::EnPassant || self.piece_on(to) != NO_PIECE);

// Early-out for trivially safe moves:
// A non-capture, non-commoner, non-en-passant move by a non-blocker when
// there are no checkers cannot possibly expose a commoner to attack.
if state.checkers.is_empty()
    && !is_capture
    && m.move_type() != MoveType::EnPassant
    && pt != PieceType::Commoner
    && (state.pinned & Bitboard::square_bb(from)).is_empty()
{
    return true;
}
```

**Safety argument:** A non-capture, non-commoner, non-ep move when no
checkers exist and the moving piece is not pinned **cannot** change the
attack status of any commoner. The commoners' positions are unchanged, no
new enemy attacks are uncovered, and no blast occurs. Therefore the
pre-move legal state (which is legal by construction — the position was
reached via legal play) remains valid.

**File:** `src/board.rs`, inserted before the castling block (~line 677).

---

### Step 5 — Remove `_state` prefix from `legal()` signature

Since `legal()` now actually uses `state`, remove the underscore:

```rust
// Before:
pub fn legal(&self, m: Move, _state: &StateInfo) -> bool {

// After:
pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
```

---

### Step 6 — Compile and clippy

```sh
cargo build && cargo clippy && cargo fmt
```

No new dependencies. All changes are in `src/board.rs` and `src/movegen.rs`.

---

### Step 7 — Run perft regression

```sh
cargo test
cargo run --example verify_perft 6 --release
```

All 41 test positions at depths 1–6 must match the expected values in
`perft_values.md`. If any fail, the early-out condition or the state
population is incorrect.

---

## Files Changed

| File | Lines | Change |
|------|-------|--------|
| `src/board.rs` | ~420 | Add `populate_state()` method |
| `src/board.rs` | ~560 | Add `self.populate_state(state)` at end of `do_move()` |
| `src/board.rs` | ~664 | Remove `_` prefix; compute `pt`, `is_capture` early |
| `src/board.rs` | ~675 | Insert early-out block (Item 2) |
| `src/board.rs` | ~803–804 | Replace `.count()` calls with `state.*_count` |
| `src/movegen.rs` | ~231 | Add `board.populate_state(&mut state)` in `generate_legal()` |

**Total:** ~50 lines of new/changed code (matching analysis estimate).

---

## Verification

1. **Unit tests:** `cargo test` — existing board tests (do/undo, checkers,
   pinned, self-explosion, blast, en-passant) must all pass.

2. **Perft regression:** `cargo run --example verify_perft 6 --release`
   — all 41 test positions, depths 1–6, must match `perft_values.md`.

3. **Performance measurement** (optional, post-merge):
   ```sh
   RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
   hyperfine 'target/profiling/examples/perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6'
   ```
   Compare wall-clock time against the previous commit.

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| `populate_state()` cost > early-out savings on positions with few pieces | Low | The overhead is one `compute_checkers` + `compute_pinned` per position. Even in endgames with few moves, this is cheaper than computing checkers/pinned per move in `legal()`. |
| Early-out fires incorrectly for multi-commoner positions | Medium | The condition `state.pinned & square_bb(from)` catches the case where a non-blocker non-commoner move is safe even with multiple commoners. If `blockers_for_king` is computed correctly (all pieces on a between-ray, not just sole blockers), this is sound. Test thoroughly. |
| Early-out blocks incorrectly for en passant | Low | En passant is explicitly excluded from the early-out (`m.move_type() != MoveType::EnPassant`). EP is a capture so it would be excluded anyway via `!is_capture`, but the explicit check is a safety net. |
| Regressions in existing tests | Low | All existing tests must pass before merge. The perft regression suite covers 41 positions across all depths. |

---

## Relationship to Other Items

- **Item 2 (early-out):** Implemented simultaneously — the early-out is
  the primary source of speedup and depends on `state.checkers` and
  `state.pinned`.
- **Item 3 (cache commoner bitboards):** Independent follow-up that can be
  done after this plan merges. It will further reduce bitboard AND
  operations in `legal()`.
- **Item 5 (precomputed `between_bb`):** Would speed up `compute_pinned()`
  itself, but is not a prerequisite for this plan.
