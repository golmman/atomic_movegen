# Implementation Report

> Report generated after implementing all items from `fix_plan.md`.

## Summary

All **6 items** from the fix plan have been implemented and verified. Most items were already applied to the codebase before this session; **4 unit tests** in `src/board.rs` had incorrect FEN positions and required fixes to pass.

---

## Item-by-item status

### 1. `between_bb` returns wrong value for non-aligned squares

| Aspect | Detail |
|--------|--------|
| **Status** | ✅ Already fixed in codebase |
| **File** | `src/bitboard.rs:106` |
| **Change** | `return square_bb(s2);` → `return Bitboard::EMPTY;` |
| **Verification** | `test_between_bb` (aligned squares) passes; `test_between_bb_non_aligned` passes |

### 2. Delete stale `src/main.rs`

| Aspect | Detail |
|--------|--------|
| **Status** | ✅ Already deleted |
| **Change** | `src/main.rs` removed from filesystem |
| **Verification** | `cargo build` succeeds; `cargo run --example perft "..." 1` outputs `20` |

### 3. Add crate-level documentation

| Aspect | Detail |
|--------|--------|
| **Status** | ✅ Already present |
| **File** | `src/lib.rs` |
| **Change** | `//!` doc comment added at crate root with description, atomic chess rules, and usage example |
| **Verification** | `cargo doc --no-deps` produces documentation without warnings; doc-test runs and passes |

### 4. Expand `README.md`

| Aspect | Detail |
|--------|--------|
| **Status** | ✅ Already expanded |
| **File** | `README.md` |
| **Change** | Full library description, features list, library+CLI usage examples, tested positions table, license |
| **Verification** | Renders correctly; code examples are accurate |

### 5. Add integration tests

| Aspect | Detail |
|--------|--------|
| **Status** | ✅ Already created |
| **File** | `tests/perft_tests.rs` |
| **Change** | 5 tests: perft starting depth 1–4, and position 2 depth 2 (reference values from Fairy-Stockfish) |
| **Verification** | `cargo test --test perft_tests` — all 5 pass |

### 6. Add edge case unit tests

| Aspect | Detail |
|--------|--------|
| **Status** | ✅ Fixed and passing |
| **File** | `src/board.rs` (`#[cfg(test)] mod tests` block) |
| **Change** | 6 tests added; **4 required FEN corrections** (see below) |
| **Verification** | `cargo test --lib` — all 31 unit tests pass |

---

## Issues encountered and resolved

During verification, **4 of the 6 new unit tests failed** due to incorrect FEN positions in the original plan. Each was diagnosed and fixed.

### Issue 1: FEN commoner serialization mismatch

**Tests affected:** `test_do_undo_restores_state`, `test_do_undo_capture_restores`

**Root cause:** The plan's tests compared `board.fen()` against the input FEN string (`"rnbqkbnr/..."`), which uses `k`/`K` for commoners. However, the `Board::fen()` method serializes commoners as `c`/`C`, not `k`/`K`. After a do/undo cycle the board state is identical, but the FEN string differs.

**Fix:** Capture the reference FEN via `board.fen()` *before* making the move, then compare against that after undo:

```rust
let orig_fen = board.fen();
// ... do_move, undo_move ...
assert_eq!(board.fen(), orig_fen);
```

### Issue 2: Self-explosion test — commoner out of blast zone

**Test:** `test_self_explosion_illegal`

**Root cause:** The original position placed the white commoner on **e1** (rank 1) and the capture on **e3** (rank 3). The 3×3 blast zone around e3 covers ranks 2–4, so e1 (rank 1) was **not** in the blast zone. The rook capture was correctly generated as legal, causing the assertion to fail.

**Fix:** Moved pieces to the d-file so the commoner is within blast range:

- Commoner on **d3** (rank 3), rook on **d5** (rank 5), black pawn on **d4** (rank 4)
- Blast zone around d4 (c3–e5) includes d3 ✓

**FEN:** `4k3/8/8/3R4/3p4/3C4/8/4K3 w - - 0 1`

### Issue 3: Pinned piece capture — bishop cannot move vertically

**Test:** `test_pinned_piece_capture_explodes_pinner`

**Root cause (part 1):** The original position had a white **bishop** on e4 pinned by a black rook on **h8** (FEN `4k2r`). The rook was nowhere near the bishop, and they were not on the same line — no pin existed.

**Root cause (part 2):** The revised position (bishop on e3, rook on e5) was also wrong because **bishops cannot move vertically**. A bishop on e3 cannot capture a pawn on e4 (same file, different rank).

**Fix:** Changed the pinned piece to a **rook** (which can move vertically):

- White rook on **e3**, white commoner on **e1**, black rook on **e5**, black pawn on **e4**
- The white rook on e3 is pinned by the black rook on e5 (both on the e-file, commoner behind)
- White rook captures e4 — blast zone around e4 (d3–f5) destroys the black rook on e5, removing the pin

**FEN:** `4k3/8/8/4r3/4p3/4R3/8/4K3 w - - 0 1`

---

## Final verification

| Check | Result |
|-------|--------|
| `cargo build` | ✅ Passes |
| `cargo test --lib` (31 unit tests) | ✅ All pass |
| `cargo test --test perft_tests` (5 integration tests) | ✅ All pass |
| `cargo doc --no-deps` | ✅ No warnings |
| `cargo run --example perft "..." 1` → output `20` | ✅ Correct |

**Total tests:** 36 passing (31 unit + 5 integration).

---

## Files modified

| Action | File | Description |
|--------|------|-------------|
| Edit (pre-existing) | `src/bitboard.rs:106` | `square_bb(s2)` → `Bitboard::EMPTY` |
| Delete (pre-existing) | `src/main.rs` | Removed stale stub |
| Edit (pre-existing) | `src/lib.rs` | Added `//!` crate-level doc comment |
| Edit (pre-existing) | `README.md` | Full library description and examples |
| Create (pre-existing) | `tests/perft_tests.rs` | Integration tests for perft reference values |
| Edit (fixed in session) | `src/board.rs` | Fixed 4 test FENs in the `#[cfg(test)] mod tests` block |
