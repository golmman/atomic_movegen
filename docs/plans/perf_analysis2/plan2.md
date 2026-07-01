# Plan 2 — Inline Legal Filtering in `generate_legal()`

**Optimization:** Item 2 from `analysis.md` — Inline legal filtering into `generate_legal()`
**Estimated impact:** 5–10 % speedup (analysis.md, line 172)
**Effort:** ~60 lines across `src/movegen.rs`, `src/board.rs`, `src/types.rs`
**Risk:** Low (purely mechanical transformation; correctness checked by full perft regression)
**Baseline command:** `cargo run --release --example verify_perft`

---

## 1. Motivation

`generate_legal()` currently calls `generate_pseudo_legal()` to produce all
pseudo-legal moves, then filters them via a closure:

```rust
moves.retain(|m| board.legal(m, &state));
```

This closure call has three sources of overhead:

| Source | Evidence |
|--------|----------|
| **Closure call overhead** | `moves.retain()` calls the closure once per pseudo-legal move. The closure must capture `&Board` and `&StateInfo` and dispatch through the `FnMut` trait. |
| **`legal()` function call** | Every pseudo-legal move enters `Board::legal()` — even the ~80 % of moves that pass the early-out on line 696–709. The function prologue alone (saving/restoring callee-saved registers) is paid for every move. 23.2 % of all CPU samples fall in the first 70 bytes of `legal()`. |
| **Redundant computations** | `legal()` recomputes `piece_on(from)`, `type_of()`, `piece_on(to)`, and `move_type()` — all of which were already available (or cheaply derivable) during move generation. |

**Goal:** Fold the fast-path check directly into `generate_legal()` so that
trivially-safe moves are accepted with inline condition checks instead of a
function call. Only complex moves (captures, commoner moves, en-passant,
castling, pinned pieces, or when in check) fall through to `Board::legal()`.

---

## 2. Design

### 2.1 Extracted fast-path predicate

Extract the early-out check from `Board::legal()` (lines 696–709) into a
standalone `#[inline(always)]` helper function in `src/board.rs`:

```rust
/// Returns `true` when `m` is trivially legal — i.e. not a capture, not a
/// commoner move, not en-passant, the moving piece is unpinned, there are no
/// checkers, and at least one own commoner still exists.
///
/// When this returns `true` the move is guaranteed legal without needing the
/// full `legal()` check (blast, pseudo-royal, castling pass-through).
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

    // A non-Castling move that captures a piece (or EnPassant — already handled above).
    let is_capture = mt != MoveType::Castling && board.piece_on(m.to_sq()) != NO_PIECE;
    if is_capture {
        return false;
    }

    // Castling reaches here (not a capture, not en-passant, not a commoner move).
    // Castling still needs the full pass-through check, so reject fast-path.
    if mt == MoveType::Castling {
        return false;
    }

    // Check pin: a pinned piece might expose a commoner.
    if (state.pinned & Bitboard::square_bb(from)) != Bitboard::EMPTY {
        return false;
    }

    true
}
```

The `legal()` function's early-out block (lines 696–709) is replaced with:

```rust
if is_move_trivially_legal(self, m, state) {
    return true;
}
// ... rest of legal() unchanged
```

### 2.2 Inline filter in `generate_legal()`

Replace `generate_legal()` in `src/movegen.rs`:

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    generate_pseudo_legal(board, moves);

    // In-place compaction: fast-path accept without legal() call.
    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            if is_move_trivially_legal(board, m, &state)
                || board.legal(m, &state)
            {
                ms[write_idx] = m;
                write_idx += 1;
            }
        }
        write_idx
    };
    // SAFETY: new_len <= orig_len (we only copy, never create new entries).
    // The unchecked set_len avoids redundant zeroing; values at indices
    // >= new_len are stale but never accessed because as_slice()/len() use
    // the updated length.
    moves.set_len(new_len);
}
```

Key points:
- `is_move_trivially_legal()` is called first; if it returns `true`, the move
  is accepted without calling the heavyweight `legal()`.
- For the minority of moves (captures, commoner moves, en-passant, castling,
  pins, checks), we fall through to `board.legal()`.
- The `as_mut_slice()` read/write pattern works because `Move` is `Copy` — each
  iteration copies the value out of the array, checks it, and copies it back
  at the write position.
- The `orig_len` is captured before the mutable borrow to avoid borrowing
  `moves` for its `len()` while the mutable slice is alive.

### 2.3 `MoveList::set_len()` addition

Add a `pub(crate)` method to `MoveList` in `src/types.rs`:

```rust
impl MoveList {
    /// Sets the length directly (caller must ensure `len <= MAX_MOVES` and
    /// that elements beyond `len` are unused).
    #[inline]
    pub(crate) fn set_len(&mut self, len: usize) {
        debug_assert!(len <= MAX_MOVES, "MoveList::set_len overflow");
        self.len = len;
    }
}
```

Alternatively, make the `len` field `pub(crate)` directly. The `set_len` method
is preferred because it provides a natural place for the `debug_assert!`.

---

## 3. Changes Required

### 3.1 File: `src/board.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Add `is_move_trivially_legal()` | ~25 | New `pub(crate)` helper function with the fast-path check. |
| Replace early-out in `legal()` | ~2 | `if state.checkers.is_empty() && ...` → `if is_move_trivially_legal(self, m, state)` |

The `piece_on()` / `type_of()` / `move_type()` calls in `legal()` subsequent
to the early-out are unchanged — `legal()` still handles the full check for
non-fast-path moves that call it from `generate_legal()`.

### 3.2 File: `src/types.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Add `pub(crate) fn set_len()` | ~5 | New method on `MoveList` to allow `generate_legal()` to set the filtered length. |

### 3.3 File: `src/movegen.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Replace `moves.retain(...)` with inline filter | ~25 | New in-place compaction loop that checks fast-path first, falls through to `legal()`. |

### 3.4 Files not touched

- `src/lib.rs` — no changes (the `perft()` function uses `MoveList` and calls
  `generate_legal()` with the same API).
- `examples/*.rs` — no changes (the `MoveList` API is unchanged).
- `src/movegen.rs` (test code) — no changes needed.
- `src/board.rs` (test code) — no changes needed.

---

## 4. Implementation Order

| Step | Description | Files touched | Verification |
|------|-------------|---------------|--------------|
| **4.1** | Add `is_move_trivially_legal()` to `board.rs` and wire it into `legal()` | `board.rs` | `cargo build` |
| **4.2** | Add `MoveList::set_len()` to `types.rs` | `types.rs` | `cargo build` |
| **4.3** | Inline the filter loop in `generate_legal()` | `movegen.rs` | `cargo test` |
| **4.4** | Run `cargo clippy` and `cargo fmt` | (all) | `cargo clippy && cargo fmt` |
| **4.5** | Full perft regression | — | `cargo run --release --example verify_perft` |
| **4.6** | Performance measurement | — | `cargo run --release --example verify_perft` + record total time |

---

## 5. Testing Against Baseline

### Correctness

All 41 perft positions must pass at depths 1–6:

```sh
cargo run --release --example verify_perft
```

Expected:
```
  Test #1    PASS (6 depths) [X.XXX s]
  ...
  Result:    41/41 passed, 0/41 failed
```

The most likely correctness failure would be a logic error in
`is_move_trivially_legal()` — e.g., failing to reject a castling move or
accepting a move when pinned. Unit tests in `board.rs`
(`test_self_explosion_legal_with_surviving_commoner`,
`test_self_explosion_illegal_last_commoner`,
`test_pinned_piece_capture_explodes_pinner`) exercise these edge cases and
should pass first.

### Performance measurement

Run the baseline first (current `main` with Plan 1 applied):

```sh
# Baseline (Plan 1):
cargo run --release --example verify_perft
# Record "Total time:" from the summary line.
```

Then apply the Plan 2 changes and run again:

```sh
# Plan 2:
cargo run --release --example verify_perft
# Record "Total time:" from the summary line.
```

Compare the two totals to compute the speedup:

```
Speedup = (baseline_time - plan2_time) / baseline_time × 100 %
```

Both runs must be on the same machine under similar load. The current Plan 1
baseline from report1.md is **110.534 s** total wall-clock time.

### Expected improvement range

The analysis estimates **5–10 %** speedup. At 5 %, we would see ~105.0 s total;
at 10 %, ~99.5 s total.

---

## 6. Performance Model

### Why the inline filter is faster

The current `moves.retain(|m| board.legal(m, &state))` for a typical position
with ~40 pseudo-legal moves:

| Step | Cost |
|------|------|
| Closure call (×40) | 40 × (capture `&Board` + `&StateInfo` + trait dispatch) |
| `legal()` function call (×40) | 40 × (prologue: save callee-saved regs) |
| Early-out check in `legal()` (×40) | 40 × (piece_on, type_of, condition checks) |
| Full `legal()` body (×~8 captures) | ~8 × (blast, pseudo-royal loop) |

With the inline filter:

| Step | Cost |
|------|------|
| Inline fast-path check (×40) | 40 × (piece_on, type_of, condition checks) — same arithmetic, but **no function call** |
| `legal()` call (×~8 complex) | ~8 × (full function prologue + body) |

The savings come from:
1. **No closure dispatch** — the trait method `FnMut::call_mut` is eliminated.
2. **No `legal()` entry for fast-path moves** — ~32 out of 40 moves skip the
   function call entirely. Each function call costs ~10–20 cycles for the
   `bl`/`ret` pair plus prologue register saves.
3. **Better inlining** — the compiler can inline `is_move_trivially_legal()`
   into `generate_legal()`, potentially hoisting invariant loads
   (`state.checkers`, `state.pinned`, `state.commoners_count`) out of the loop.

### Worst-case positions

Positions with many captures, commoner moves, or checks (e.g., open tactical
positions) will see less benefit because a higher fraction of moves falls
through to `legal()`. Positions with few captures and no checks (e.g., the
starting position, closed positions) will see the most benefit.

---

## 7. Flow Comparison

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

---

## 8. Edge Cases & Risks

| Risk | Mitigation |
|------|-----------|
| **`is_move_trivially_legal()` logic divergence** from the original early-out in `legal()` | The extracted helper is checked against perft regression (41 positions, depths 1–6). The pre-existing unit tests for pins, self-explosion, and captures provide additional coverage. |
| **Borrow checker rejects the inline filter** | The pattern `let ms = moves.as_mut_slice(); ... for ... { ms[read_idx]; ... ms[write_idx] = m; }` works because `Move: Copy`. The `orig_len` is captured before `as_mut_slice()`. A block scope delimits the mutable borrow, after which `moves.set_len(new_len)` is called. |
| **`set_len` used incorrectly** | The `debug_assert!(len <= MAX_MOVES)` catches programming errors. The value `new_len` is always `<= orig_len` (monotonic), so it cannot exceed `MAX_MOVES`. |
| **Performance regression on positions with many complex moves** | In the worst case (every move is a capture/commoner/pin), every move falls through to `legal()` — but the inline filter still saves the closure dispatch overhead (~1–2 %), and the fast-path check's overhead is identical to what `legal()` did anyway. |
| **I-cache pressure from duplicating the fast-path check** | The fast-path check is small (~30 instructions) and called in a tight loop. On arm64 Firestorm with 192 KB L1I, this is negligible. |

---

## 9. Relationship to Other Items

- **Item 1 (MoveList ✅):** The inline filter builds directly on `MoveList`.
  Access to `as_mut_slice()` and the new `set_len()` are the only additions
  needed.
- **Items 3 + 4 (fused attackers, dedup sliders):** Independent of this change.
  After Item 2, the `legal()` function can be further optimized by fusing
  attacker computations in the pseudo-royal loop and castling checks. The
  compatibility is one-directional: Items 3+4 touch only `legal()`, which is
  still called for non-fast-path moves.
- **Item 5 (optimize `compute_pinned()`):** Independent — it changes
  `populate_state()`, not the legal filter.
- **Item 9 (split `legal()`):** If Items 3+4 don't provide enough gain,
  splitting `legal()` into smaller helpers would further benefit the
  non-fast-path callers.

### Cumulative speedup estimate

If Plan 1 delivered 11.1 % and Plan 2 delivers ~7.5 % (midpoint of the 5–10 %
range), the cumulative speedup from baseline is:

```
1 - (1 - 0.111) × (1 - 0.075) = 1 - 0.889 × 0.925 ≈ 17.8 %
```

Or equivalently, total time dropping from 124.380 s → ~102.3 s.

---

## 10. Summary

| Aspect | Detail |
|--------|--------|
| **What** | Replace `moves.retain(\|m\| board.legal(m, &state))` in `generate_legal()` with an inline filter that checks the fast-path condition without a function call. |
| **Why** | Eliminates closure dispatch overhead and `legal()` function-call cost for the ~80 % of pseudo-legal moves that are trivially safe. |
| **How** | Extract early-out logic from `legal()` into `is_move_trivially_legal()`; add inline compaction loop in `generate_legal()`; add `MoveList::set_len()`. |
| **Files** | `src/board.rs` (~25 lines), `src/types.rs` (~5 lines), `src/movegen.rs` (~25 lines) |
| **Verification** | `cargo test` + `cargo run --release --example verify_perft` (all 41 positions at depths 1–6) |
| **Expected speedup** | 5–10 % on `verify_perft` total time (from 110.534 s → ~99.5–105.0 s) |
