# Report 2 — Inline Legal Filtering in `generate_legal()`

## Summary

Plan 2 has been fully implemented. The `moves.retain(|m| board.legal(m, &state))`
closure-based filter in `generate_legal()` has been replaced with an inline
compaction loop that calls the lightweight `is_move_trivially_legal()` fast-path
check first, falling through to the full `Board::legal()` only for complex moves
(captures, commoner moves, en-passant, castling, pins, or when in check).

**Total:** ~55 lines added across 3 library files (`src/board.rs`, `src/types.rs`,
`src/movegen.rs`).

**Measured speedup:** 2.9 % reduction in `verify_perft` total wall-clock time
(110.534 s → 107.310 s), below the estimated 5–10 % range.

**Cumulative speedup from original baseline (before Plan 1):** 13.7 %
(124.380 s → 107.310 s).

---

## Changes Implemented

### 1. `is_move_trivially_legal()` helper (`src/board.rs`)

The early-out condition from `Board::legal()` (previously lines 696–709) was
extracted into a standalone `#[inline(always)]` free function:

```rust
#[inline(always)]
pub(crate) fn is_move_trivially_legal(board: &Board, m: Move, state: &StateInfo) -> bool {
    if !state.checkers.is_empty() {
        return false;
    }
    if state.commoners_count == 0 {
        return false;
    }
    let from = m.from_sq();
    let pt = board.piece_on(from).type_of();
    if pt == PieceType::Commoner {
        return false;
    }
    let mt = m.move_type();
    if mt == MoveType::EnPassant {
        return false;
    }
    let is_capture = mt != MoveType::Castling && board.piece_on(m.to_sq()) != NO_PIECE;
    if is_capture {
        return false;
    }
    if mt == MoveType::Castling {
        return false;
    }
    if (state.pinned & Bitboard::square_bb(from)) != Bitboard::EMPTY {
        return false;
    }
    true
}
```

The early-out block in `legal()` was replaced with a single call:

```rust
if is_move_trivially_legal(self, m, state) {
    return true;
}
```

The now-unused `let pt = piece.type_of();` variable was also removed from
`legal()` to eliminate the compiler warning.

### 2. `MoveList::set_len()` (`src/types.rs`)

Added a `pub(crate)` method to support the in-place compaction loop:

```rust
#[inline]
pub(crate) fn set_len(&mut self, len: usize) {
    debug_assert!(len <= MAX_MOVES, "MoveList::set_len overflow");
    self.len = len;
}
```

### 3. Inline filter in `generate_legal()` (`src/movegen.rs`)

Replaced the `moves.retain()` closure with an explicit compaction loop:

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    generate_pseudo_legal(board, moves);

    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            if is_move_trivially_legal(board, m, &state) || board.legal(m, &state) {
                ms[write_idx] = m;
                write_idx += 1;
            }
        }
        write_idx
    };
    moves.set_len(new_len);
}
```

Key design points:
- `is_move_trivially_legal()` is called first; if it returns `true`, the move
  is accepted without calling the heavyweight `legal()`.
- Only the minority of moves (captures, commoner moves, en-passant, castling,
  pins, checks) fall through to `board.legal()`.
- The `as_mut_slice()` read/write pattern works because `Move` is `Copy` —
  each iteration reads the value, checks it, and writes it back at the
  write position.
- `orig_len` is captured before the mutable borrow to avoid borrowing `moves`
  for its `len()` while the mutable slice is alive.
- The block scope delimits the mutable borrow, after which `moves.set_len()` is called.

---

## Verification Results

### Unit tests (`cargo test`)

All 46 tests pass:
- 41 unit tests (attacks, bitboard, board, movegen, magic, pext, types)
- 4 perft regression tests (depth 1–4)
- 1 doc-test

### Full perft regression (`cargo run --release --example verify_perft`)

All 41 test positions pass at depths 1–6, producing exact counts matching
`perft_values.md`.

| Metric | Plan 1 baseline | Plan 2 | Change |
|--------|-----------------|--------|--------|
| Total time | 110.534 s | 107.310 s | **−2.9 %** |
| Average per test | 2.696 s | 2.617 s | **−2.9 %** |
| Fastest test | 0.001 s | 0.002 s | — |
| Slowest test | 17.115 s | 16.045 s | **−6.3 %** |
| All 41 positions | PASS | PASS | ✓ |

### Cumulative improvement from original baseline

| Baseline | Total time | vs Original | vs Plan 1 |
|----------|-----------|-------------|-----------|
| Original (pre-Plan 1) | 124.380 s | — | — |
| After Plan 1 (`MoveList`) | 110.534 s | **−11.1 %** | — |
| After Plan 2 (inline filter) | 107.310 s | **−13.7 %** | **−2.9 %** |

---

## Deviations from Plan

| Item | Planned | Actual | Reason |
|------|---------|--------|--------|
| `is_move_trivially_legal()` location | Free function in `board.rs` | Free function in `board.rs` | Matches plan exactly |
| `set_len()` signature | `pub(crate) fn set_len(&mut self, len: usize)` | `pub(crate) fn set_len(&mut self, len: usize)` | Matches plan exactly |
| `generate_legal()` structure | Block scope with `as_mut_slice()` | Block scope with `as_mut_slice()` | Matches plan exactly |
| Block scope | Delimits mutable borrow, `moves.set_len()` after | Same pattern | Matches plan exactly |
| Removed unused `pt` variable | Not mentioned in plan | Removed from `legal()` | Necessary to avoid compiler warning after early-out removal |
| Speedup achieved | 5–10 % | 2.9 % | Below expected range; see interpretation below |

---

## File Changes

| File | Δ Lines | Change |
|------|---------|--------|
| `src/board.rs` | +46 / −16 | Added `is_move_trivially_legal()` free function; replaced early-out block in `legal()` with call to it; removed unused `pt` variable |
| `src/types.rs` | +8 | Added `MoveList::set_len()` with debug_assert |
| `src/movegen.rs` | +27 / −3 | Replaced `moves.retain(...)` with inline compaction loop; added `is_move_trivially_legal` import |

### Files not touched

- `src/lib.rs` — no changes (the `perft()` function uses `MoveList` and calls
  `generate_legal()` with the same API).
- `examples/*.rs` — no changes (the `MoveList` API is unchanged).
- `src/movegen.rs` (test code) / `src/board.rs` (test code) — no changes needed.

---

## Risk Assessment

| Risk | Outcome | Notes |
|------|---------|-------|
| **`is_move_trivially_legal()` logic divergence** from original early-out | **Did not occur** | All 41 perft positions pass at depths 1–6; all edge-case unit tests pass |
| **Borrow checker rejects inline filter** | **Did not occur** | Block-scoped mutable borrow + captured `orig_len` compiles cleanly |
| **`set_len` used incorrectly** | **Did not occur** | `debug_assert!` guards overflow; compaction loop guarantees `new_len <= orig_len` |
| **Performance regression on complex positions** | **Minor** | Positions with many captures/checks see less benefit, but no regression observed |
| **I-cache pressure from duplicated fast-path check** | **Negligible** | The function is ~30 instructions; Firestorm's 192 KB L1I is barely affected |
| **Unused variable warning** | **Resolved** | `let pt` in `legal()` was removed after the early-out was replaced |

---

## Performance Measurement

Measured using `cargo run --release --example verify_perft` on Apple Firestorm
(arm64). The runner processes all 41 test positions at depths 1–6 and reports
wall-clock time per test.

### Baseline (Plan 1, from `report1.md`)

```
--- Summary ---
  Average:     2.696 s per test
  Total time:  110.534 s
```

### Plan 2 (this implementation)

```
--- Summary ---
  Average:     2.617 s per test
  Total time:  107.310 s
```

### Test-by-test comparison of selected positions

| Test | Plan 1 | Plan 2 | Δ | Δ% |
|------|--------|--------|---|----|
| #1   | 0.917 s | 0.913 s | −0.004 s | −0.4 % |
| #2   | 14.309 s | 13.382 s | −0.927 s | −6.5 % |
| #13  | 17.115 s | 16.045 s | −1.070 s | −6.3 % |
| #16  | 6.037 s | 5.802 s | −0.235 s | −3.9 % |
| #22  | 7.682 s | 7.340 s | −0.342 s | −4.5 % |
| #24  | 7.578 s | 7.365 s | −0.213 s | −2.8 % |
| #33  | 15.110 s | 15.097 s | −0.013 s | −0.1 % |

The heaviest positions (depth-6, many nodes) show the most benefit because they
spend proportionally more time in `generate_legal()` vs. FEN parsing/IO.

### Interpretation of below-expected speedup

The measured 2.9 % is below the estimated 5–10 % range. Several factors
contribute:

1. **The early-out was already cheap.** The profiling showed 23.2 % of samples
   in the first 70 bytes of `legal()`, but in Plan 1 the majority of that time
   was already just the condition checks themselves (loads, compares, branches)
   — not the function call overhead. The function call overhead (prologue:
   saving/restoring callee-saved registers, `bl`/`ret`) accounts for a smaller
   fraction than estimated.

2. **Closure dispatch was already optimized.** The compiler may have already
   inlined or devirtualized the `FnMut` closure call in `moves.retain()` for
   release builds, reducing the expected savings from eliminating the closure.

3. **The `is_move_trivially_legal()` check redoes work.** Although the function
   is `#[inline(always)]` and hoists invariant loads, it still performs the
   same `piece_on(from)`, `type_of()`, `move_type()` calls that the early-out
   in `legal()` performed. The savings come only from:
   - Eliminating the `legal()` function call prologue for fast-path moves
   - Eliminating the closure dispatch
   - Potentially better inlining (hoisting invariant state loads)

4. **The overhead was already small.** The original profile showed `legal()` at
   33.38 % self-time, but Plan 1's early-out already eliminated the body of
   `legal()` for most calls. The remaining overhead in `legal()` is dominated
   by the condition-check arithmetic (~23 % of total samples), which the inline
   filter must still perform.

5. **Amdahl's Law.** With Plan 1's 11.1 % speedup already reducing the total
   time, the absolute headroom for further improvements is smaller. The ~3 %
   gain is still real but represents diminishing returns on this particular
   optimization.

### When would the gain be larger?

The inline filter would show larger gains on:
- **Positions with fewer captures/checks** — the fast-path fires for a higher
  fraction of moves.
- **Hardware with larger function-call overhead** — deeper pipelines, more
  expensive `bl`/`ret`, or slower L1I cache.
- **Compilation with debug info disabled** — `--release` already optimizes
  heavily; a `--profile profiling` build (which has `debug = 2`) might show
  larger relative gains.

---

## Relationship to Other Items

- **Item 1 (`MoveList` ✅):** Plan 2 builds directly on `MoveList`, using
  `as_mut_slice()` and `set_len()` for the in-place compaction loop.
- **Items 3 + 4 (fused attackers, dedup sliders):** Independent of this change.
  After Plan 2, the `legal()` function can still be further optimized by fusing
  attacker computations. The compatibility is one-directional: Items 3+4 touch
  only `legal()`, which is still called for non-fast-path moves.
- **Item 5 (optimize `compute_pinned()`):** Independent — it changes
  `populate_state()`, not the legal filter.
- **Item 9 (split `legal()`):** If Items 3+4 don't provide enough gain,
  splitting `legal()` into smaller helpers would benefit the non-fast-path
  callers.

---

## Flow Comparison

```
Before (Plan 1):
  generate_legal()
    ├─ generate_pseudo_legal()          // collects ~40 pseudo-legal moves
    └─ moves.retain(|m| board.legal())
         ├─ legal() [fast path]  ← ~32 moves — early-out returns true
         └─ legal() [full]       ← ~8 moves — blast + pseudo-royal

After (Plan 2):
  generate_legal()
    ├─ generate_pseudo_legal()          // collects ~40 pseudo-legal moves
    └─ inline filter loop
         ├─ is_move_trivially_legal()   // ~32 moves — accept, no function call
         └─ board.legal()              // ~8 moves — full check
```

The key difference: ~32 out of ~40 moves now skip the `legal()` function entry
entirely, avoiding the function prologue/epilogue and closure dispatch for those
moves. Only the non-trivial moves pay the full `legal()` call cost.
