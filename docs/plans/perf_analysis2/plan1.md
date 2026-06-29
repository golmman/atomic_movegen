# Plan 1 — Replace `Vec<Move>` with Stack-Allocated `MoveList`

**Optimization:** Item 1 from `analysis.md` — Stack-allocated `MoveList`
**Estimated impact:** 8–15 % speedup
**Effort:** ~50 lines of new code + signature changes across ~10 call sites
**Risk:** Low (bounded problem size, simple data structure)
**Baseline command:** `cargo run --release --example verify_perft`

---

## 1. Motivation

The performance analysis identified that `Vec<Move>` is used in every hot path:

| Problem | Evidence from profiling |
|---------|------------------------|
| Heap allocation (`malloc`/`free`) | 2.24 % of samples — `cfree`/`malloc` in libc |
| Bounds check on every `.push()` | Cannot be elided by compiler for `Vec` (dynamic capacity) |
| `Vec::retain()` closure overhead | Closure call + potential memory shift on every pseudo-legal move |
| Heap-backed iteration | Loop overhead in `perft()` (51.19 % self-time) includes Vec overhead |

A fixed-size `[Move; 256]` stack array eliminates all of these:
- Zero heap traffic: the `MoveList` lives entirely on the caller's stack
- The bounds check becomes a comparison against the constant `256`, which the compiler can often optimize away (especially given bounded loops in `generate_pseudo_legal`)
- In-place compaction in `retain` avoids reallocation
- Stack-allocated arrays have better cache locality

---

## 2. Design

### 2.1 New type: `MoveList`

```rust
/// Maximum number of moves that can be generated for any atomic chess position.
///
/// The absolute upper bound is well below 256:
/// - At most 64 squares with attackers
/// - Perft at depth 1 on the most complex legal positions yields < 150 moves
/// - 256 provides a comfortable safety margin.
pub const MAX_MOVES: usize = 256;

/// A fixed-capacity, stack-allocated list of `Move` values.
///
/// This is a drop-in replacement for `Vec<Move>` in move-generation hot paths.
/// It avoids heap allocation, eliminates dynamic capacity checks, and improves
/// cache locality by keeping the entire move list on the stack.
pub struct MoveList {
    moves: [Move; MAX_MOVES],
    len: usize,
}
```

### 2.2 Public API

```rust
impl MoveList {
    /// Creates an empty `MoveList`.
    #[inline]
    pub fn new() -> Self { ... }

    /// Returns the number of moves currently stored.
    #[inline]
    pub fn len(&self) -> usize { ... }

    /// Returns `true` if the list contains zero moves.
    #[inline]
    pub fn is_empty(&self) -> bool { ... }

    /// Appends a move to the end of the list.
    ///
    /// # Panics
    /// Panics in debug mode if the list is full (len == MAX_MOVES).
    /// In release mode, exceeding the capacity silently overwrites (unreachable
    /// in practice due to the move-count bound).
    #[inline]
    pub fn push(&mut self, m: Move) { ... }

    /// Removes all moves from the list (retains the allocated array).
    #[inline]
    pub fn clear(&mut self) { ... }

    /// Returns the stored moves as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[Move] { ... }

    /// Returns the stored moves as a mutable slice (for sorting, etc.).
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [Move] { ... }

    /// Retains only the moves satisfying the predicate, compacting in place.
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(Move) -> bool,
    { ... }
}

// Allow iteration with `for m in &list`
impl<'a> IntoIterator for &'a MoveList {
    type Item = Move;
    type IntoIter = MoveListIter<'a>;
    ...
}

// Allow direct indexing
impl Index<usize> for MoveList {
    type Output = Move;
    ...
}
impl IndexMut<usize> for MoveList { ... }
```

### 2.3 Module placement

Add `MoveList` to **`src/types.rs`** alongside the `Move` type it stores, so it is
accessible from `lib.rs`, `movegen.rs`, `board.rs`, and all examples without
circular dependencies.

---

## 3. Changes Required

### 3.1 File: `src/types.rs`

- Add `pub const MAX_MOVES: usize = 256;`
- Add `pub struct MoveList` with fields, methods, and iterator impls (~40 lines)

### 3.2 File: `src/movegen.rs`

Change signatures from `&mut Vec<Move>` to `&mut MoveList`:

| Function | Current signature | New signature |
|----------|------------------|---------------|
| `generate_pseudo_legal` | `(board: &Board, moves: &mut Vec<Move>)` | `(board: &Board, moves: &mut MoveList)` |
| `generate_pawn_moves_for` | `(board: &Board, us, them, from, moves: &mut Vec<Move>)` | `(board: &Board, us, them, from, moves: &mut MoveList)` |
| `generate_castling` | `(board: &Board, us, moves: &mut Vec<Move>)` | `(board: &Board, us, moves: &mut MoveList)` |
| `generate_legal` | `(board: &Board, moves: &mut Vec<Move>)` | `(board: &Board, moves: &mut MoveList)` |

Internal changes:
- Remove `moves.clear()` from `generate_pseudo_legal` (caller provides a fresh `MoveList`)
- Replace `moves.retain(|&m| ...)` closure call with `MoveList::retain`

### 3.3 File: `src/lib.rs`

- Replace `let mut moves = Vec::with_capacity(256);` with `let mut moves = MoveList::new();`
- Replace `for &m in &moves` with `for &m in moves.as_slice()` (or use `IntoIterator`)

### 3.4 File: `src/board.rs` (tests)

Three test functions use `crate::movegen::generate_legal`:
- `test_self_explosion_legal_with_surviving_commoner` (line 1216)
- `test_self_explosion_illegal_last_commoner` (line 1236)
- `test_pinned_piece_capture_explodes_pinner` (line 1280)

Each creates `let mut moves = Vec::new();` — replace with `let mut moves = MoveList::new();`
and update iteration to use `moves.as_slice()`.

### 3.5 File: `movegen.rs` (tests)

- `test_starting_position_move_count`: replace `Vec::with_capacity(256)` with `MoveList::new()`
- `test_knight_moves_start`: same

### 3.6 Examples (all 5 files)

| Example | Line(s) | Change |
|---------|---------|--------|
| `debug_moves.rs` | 16–17, 30–31 | `Vec::with_capacity(256)` → `MoveList::new()`, iteration via `.as_slice()` |
| `list_moves.rs` | 14–16 | Same, plus `moves.as_mut_slice().sort_by_key(...)` |
| `perft_divide.rs` | 17–19 | Same, plus sorting via `as_mut_slice()` |
| `fen_after.rs` | 20–21, 30–35 | Same |
| `pawn_debug.rs` | 37–38, 44–59 | Same |

---

## 4. Implementation Order

| Step | Description | Files touched | Verification |
|------|-------------|---------------|--------------|
| **4.1** | Add `MoveList` struct + methods to `types.rs` | `types.rs` | `cargo build` |
| **4.2** | Update `movegen.rs` signatures + internals | `movegen.rs`, `types.rs` | `cargo test` |
| **4.3** | Update `lib.rs` perft function | `lib.rs` | `cargo test` |
| **4.4** | Update `board.rs` test code | `board.rs` | `cargo test` |
| **4.5** | Update all 5 examples | `examples/*.rs` | `cargo build --examples` |
| **4.6** | Run `cargo clippy` and `cargo fmt` | (all) | `cargo clippy && cargo fmt` |
| **4.7** | **Baseline verification** | — | `cargo run --release --example verify_perft` |

---

## 5. Testing Against Baseline

After implementing, run:

```sh
cargo run --release --example verify_perft
```

Expected output (example):
```
  Test #1    PASS (6 depths) [X.XXX s]
  Test #2    PASS (6 depths) [X.XXX s]
  ...
  Result:    41/41 passed, 0/41 failed
```

If any test fails, the change introduced a correctness bug. The most likely
culprits would be:
1. `push` overflow (len >= 256) — add `debug_assert!` and investigate position
2. `retain` logic error — double-check the compaction loop
3. Iterator/indexing bug in `perft_divide` or `list_moves` sorting

### Performance measurement

Record the total wall-clock time from the summary line:

```
Total time:  XXX.XXX s
```

Compare against the baseline from `analysis.md` (Plan 1 baseline: **124.380 s** for
all 41 positions at depths 1–6). The `verify_perft` tool prints per-test timings
and a summary total time.

> **Note:** For a fair comparison, run both baseline and new code on the same
> machine under similar load. The previous baseline was measured with
> `lto = "fat"`, `codegen-units = 1`, `target-cpu = "native"`, and
> `overflow-checks = false` — all of which are already in `Cargo.toml`.

---

## 6. Edge Cases & Risks

| Risk | Mitigation |
|------|-----------|
| **MoveList overflow** (> 256 moves) | `debug_assert!` in `push`. In practice, max legal moves in atomic chess is < 150. Add a runtime assertion in `retain` that `write_idx <= MAX_MOVES`. |
| **Examples use `sort_by_key`** | Provide `as_mut_slice()` method that returns `&mut [Move]` so sorting works directly. |
| **Iteration pattern change** | All existing loops iterate `for &m in &moves` (over `&Vec<Move>`). With `MoveList`, loops become `for &m in moves.as_slice()` or we implement `IntoIterator` for `&MoveList`. Either approach compiles to the same machine code. |
| **Closure in retain** | The closure `\|&m\| board.legal(m, &state)` is the same as before. Item 2 (inline legal filtering) will remove this closure call. For now, just keep it. |
| **No `unsafe`** | All array accesses are safe. `push` uses `self.moves[self.len] = m` with a bounds check. The compiler may still elide it due to the constant capacity and bounded loop patterns. |

---

## 7. Future Work (Item 2)

After this plan is implemented and verified, **Item 2** (inline legal filtering)
can build directly on `MoveList` to eliminate the `retain` closure call entirely,
replacing it with a direct in-place filter inside `generate_legal`. This will
remove the per-move `legal()` function call overhead for trivially safe moves.
