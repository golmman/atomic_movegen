# Plan 1 — Remove `OnceLock` from Magic Bitboard Tables

**Corresponds to:** Item 1 of `docs/plans/perf_analysis6/analysis.md` — *Remove `OnceLock` from magic tables*
**Estimated speedup:** 3–8 % on `cargo run --release --example verify_perft`
**Risk:** Low
**Effort:** ~80–120 lines changed across `src/magic.rs`, `src/attacks.rs`, `src/lib.rs`

---

## 1. Problem

`src/magic.rs` stores `ROOK_TABLE` and `BISHOP_TABLE` as `std::sync::OnceLock<&[Bitboard]>`. Every `rook_attacks`, `bishop_attacks`, and `queen_attacks` call does `ROOK_TABLE.get().expect(...)` — an `Acquire` atomic load and a branch. `sample` on ARM64 hides this because the functions are inlined, but the previous `perf` run put `OnceLock` overhead at ~4.4 % of samples, and `Board::legal` / `generate_pseudo_legal` make 4–6 magic lookups per `legal` call.

The `std::sync::Once` guard in `lib.rs::perft()` is also unnecessary because the tables can be built before `perft` ever runs.

---

## 2. Goal

Replace the lazy `OnceLock` tables with compile-time static arrays so that:

- `rook_attacks`/`bishop_attacks` are one multiplication, one shift, and one direct array load.
- No runtime initialization of magic tables is required.
- `perft()` no longer contains a `std::sync::Once` guard.
- No `unsafe` code is introduced.

---

## 3. Design

### 3.1 Compile-time table generation

The magic numbers, masks, and offsets are already `const` (`ROOK_MAGICS`, `BISHOP_MAGICS`, `ROOK_MASKS`, `BISHOP_MASKS`, `ROOK_ENTRIES`, `BISHOP_ENTRIES`). The existing `build_magic_table` function already enumerates every occupancy subset and computes the reference attack with `sliding_attack`. Move that logic into two `const fn` builders:

```rust
const fn build_rook_table() -> [Bitboard; ROOK_TABLE_SIZE] { ... }
const fn build_bishop_table() -> [Bitboard; BISHOP_TABLE_SIZE] { ... }
```

The result is a `static` array:

```rust
static ROOK_TABLE: [Bitboard; ROOK_TABLE_SIZE] = build_rook_table();
static BISHOP_TABLE: [Bitboard; BISHOP_TABLE_SIZE] = build_bishop_table();
```

This is the compile-time equivalent of the `build.rs` option described in `analysis.md`, but it keeps the table logic in `magic.rs` and avoids a separate build script or code duplication.

### 3.2 Make `sliding_attack` `const` friendly

`sliding_attack` is needed by the builders and by the `#[cfg(test)]` `*_loop` reference functions. Convert it to a `pub(crate) const fn` and replace its `for` loop with a `while` loop over the `&[(i8, i8)]` slice so it can be used in `const` context.

### 3.3 Update `rook_attacks` and `bishop_attacks`

```rust
#[inline(always)]
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let e = &BISHOP_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    BISHOP_TABLE[e.offset as usize + idx as usize]
}

#[inline(always)]
pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let e = &ROOK_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    ROOK_TABLE[e.offset as usize + idx as usize]
}
```

No `.get().expect(...)`, no `OnceLock`, no runtime initialization.

### 3.4 Retire `init()`

`magic::init()` and `attacks::init()` become no-ops (or are removed if every call site can be cleaned up). The `std::sync::Once` guard in `lib.rs::perft()` is removed. Because the arrays are static, the first call to `rook_attacks`/`bishop_attacks` needs no initialization guard.

If `init()` is kept for compatibility, document it as a no-op and remove at least the `perft()` guard because it is the only one on the hot recursive path.

---

## 4. Implementation Steps

### Step 1: Record baseline

Run the baseline on the current `main` and record the result:

```sh
cargo run --release --example verify_perft
```

Reference values:
- `docs/perf/m4/2026-07-09.txt` reports `59.194 s`.
- `analysis.md` reports the current `review1` cleanup baseline as `55.941 s`.

Record the exact total time on the current machine.

### Step 2: Convert `sliding_attack` to `const fn`

In `src/magic.rs`:

- Change `pub(crate) fn sliding_attack(...)` to `pub(crate) const fn sliding_attack(...)`.
- Replace `for &(df, dr) in directions` with:

```rust
let mut i = 0;
while i < directions.len() {
    let (df, dr) = directions[i];
    // existing body
    i += 1;
}
```

- No other logic changes.

### Step 3: Add `const` table builders

Add two `const fn` builders before the table definitions:

```rust
const fn build_rook_table() -> [Bitboard; ROOK_TABLE_SIZE] { ... }
const fn build_bishop_table() -> [Bitboard; BISHOP_TABLE_SIZE] { ... }
```

Both:

- Start with `let mut table = [Bitboard::EMPTY; ...]`.
- For each square `0..64`:
  - Get `e = ROOK_ENTRIES[sq]` / `BISHOP_ENTRIES[sq]`.
  - `let mask = e.mask.0; let magic = e.magic; let shift = e.shift; let offset = e.offset as usize; let size = 1usize << (64 - shift);`
  - Enumerate `subset` from `0` through all relevant subsets using `subset = subset.wrapping_sub(mask) & mask; break when subset == 0`.
  - Compute `idx = (subset.wrapping_mul(magic) >> shift) as usize`.
  - `table[offset + idx] = sliding_attack(&ROOK_DIRS, SQUARES[sq], Bitboard(subset));`
  - Keep a `count` and `assert!(count == size, "wrong number of subsets")` at the end of each square.
- Use `assert!(idx < size, "index out of bounds")` for each generated index.

**Notes:**
- `debug_assert!` with formatted values (`"index {} ..."`) is not allowed in `const fn`; use `assert!` with a static message.
- `SQUARES` is `pub(crate) const [Square; 64]` in `src/types.rs`.
- `&ROOK_DIRS` / `&BISHOP_DIRS` coerce to `&[(i8, i8)]` in `const` context.

### Step 4: Replace `OnceLock` tables with `static` arrays

Remove:

```rust
use std::sync::OnceLock;
static ROOK_TABLE: OnceLock<&[Bitboard]> = OnceLock::new();
static BISHOP_TABLE: OnceLock<&[Bitboard]> = OnceLock::new();
fn build_magic_table(...) -> Box<[Bitboard]> { ... }
pub(crate) fn init() { ... }
```

Add:

```rust
/// Precomputed rook attack table. Indexed by `ROOK_ENTRIES[sq].offset + magic_index`.
static ROOK_TABLE: [Bitboard; ROOK_TABLE_SIZE] = build_rook_table();

/// Precomputed bishop attack table. Indexed by `BISHOP_ENTRIES[sq].offset + magic_index`.
static BISHOP_TABLE: [Bitboard; BISHOP_TABLE_SIZE] = build_bishop_table();
```

### Step 5: Update `rook_attacks`/`bishop_attacks`

Replace `.get().expect(...)` with direct indexing as in §3.3.

### Step 6: Update `init()` and `perft()`

- In `src/magic.rs`, keep `pub(crate) fn init()` only if tests/examples still call it. If kept, make it a no-op and update the doc comment.
- In `src/attacks.rs`, update `pub fn init()` to a no-op or remove it and update all call sites. Update the doc comment to state tables are precomputed.
- In `src/lib.rs`, remove the `std::sync::Once` guard from `perft()`:

```rust
pub fn perft(board: &mut board::Board, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    ...
}
```

### Step 7: Clean up redundant `init()` calls

Remove `init()` calls from:
- `src/magic.rs` `#[cfg(test)]` module.
- `src/lib.rs` `perft()` (already done in Step 6).
- `src/attacks.rs` tests.
- `src/board.rs` tests.
- `src/movegen.rs` tests.
- `tests/verify_moves.rs`.
- `examples/*.rs`.

If `init()` is kept as a compatibility no-op, this step is optional; at minimum remove the `perft()` guard.

### Step 8: Fix `const` diagnostics

- Ensure no `assert_eq!` with formatting is used in `const fn`.
- Ensure no `for` loops in `const fn` table builders.
- Ensure `debug_assert!` messages are static or use `assert!` instead.
- Run `cargo build` and fix any `const` errors.

### Step 9: Build, test, lint, document

Run in order:

```sh
cargo build
cargo test
cargo clippy
cargo fmt
cargo doc
```

All must pass without warnings.

### Step 10: Performance verification

```sh
cargo run --release --example verify_perft
```

Record:
- Total time and per-test times.
- Compare to baseline.
- Optionally run `cargo run --release --example perft "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15" 6` for the profiled FEN.

Ensure 41/41 positions pass.

### Step 11: Write `docs/plans/perf_analysis6/report1.md`

Document the implementation and create a hand-off report for plan 2. Required sections:

- **Summary** — what was changed and the measured effect.
- **Baseline** — the exact `verify_perft` time before the change.
- **Result** — exact time after and speedup/deg percentage.
- **Implementation notes** — why `const fn` was chosen over `build.rs` or `unsafe`.
- **Problems, surprises, and workarounds** — e.g. `const fn` limitations (`assert!`, `for` loops, `panic!` formatting), compile-time cost, `init()` cleanup, `clippy` nudges.
- **Files changed** — list of files and the nature of the change.
- **Verification results** — `cargo test`, `cargo clippy`, `cargo run --release --example verify_perft` outcomes.
- **Notes for plan 2** — state of `StateInfo`, `pseudoRoyals`, `EVASIONS`, etc.

---

## 5. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/magic.rs` | Add `const fn` builders, replace `OnceLock` with `static` arrays, simplify or remove `init()`. | ~80 |
| `src/attacks.rs` | Make `init()` a no-op or remove it; update doc comment. | ~5 |
| `src/lib.rs` | Remove `std::sync::Once` guard from `perft()`. | ~3 |
| `examples/*.rs` | Remove `attacks::init()` calls if `init()` is removed. | ~7 |
| `tests/verify_moves.rs` | Remove `attacks::init()` call if `init()` is removed. | ~1 |
| `src/board.rs` / `src/movegen.rs` tests | Remove `crate::attacks::init()` calls if `init()` is removed. | ~11 |

---

## 6. Correctness Verification

- `cargo test` — all unit tests, including `magic.rs` `test_magic_vs_loop_*` and `board.rs`/`movegen.rs` tests.
- `cargo run --release --example verify_perft` — 41/41 positions at depths 1–6 must pass.
- `cargo clippy` must be clean.
- `cargo fmt` and `cargo doc` must be clean.

---

## 7. Expected Impact

- `OnceLock::get` atomic load + branch is removed from every `rook_attacks`/`bishop_attacks`/`queen_attacks` call.
- `perft()` no longer pays the `std::sync::Once` guard on every recursive node.
- `generate_pseudo_legal` and `Board::legal` both benefit because they both call sliding-piece attacks.
- Estimated 3–8 % total `verify_perft` speedup, with the biggest gains on sliding-piece-rich positions (profiled FEN, tests #14, #16, etc.).

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| `const fn` cannot compile `build_rook_table`/`build_bishop_table` due to `const` evaluator limits or `panic!` formatting | Low | Replace `assert_eq!` and formatted `panic!` with simple `assert!`/`debug_assert!` static messages; test on the full `magic.rs` table. If all else fails, fall back to a `build.rs` generating `include!` files. |
| Compile-time regression from `const` table builders | Low | `const` table generation for ~107k entries is well under a second in standalone tests; if it becomes a problem, switch to `build.rs`. |
| `init()` removal breaks examples/tests | Low | Run `cargo test` and `cargo build --examples`; if `init()` is removed, update all call sites. If kept as no-op, nothing breaks. |
| Measured speedup is less than expected | Medium | `sample` hides inlined `OnceLock` overhead; the real win may be larger than the profile suggests. If no improvement, the cost may be hidden elsewhere, and we move to item 2. |

---

## 9. Notes for Plan 2

After this plan:
- Magic tables are zero-overhead static arrays.
- `StateInfo` still computes `commoners_count` and `them_commoners_count` every call; the next logical item from `analysis.md` is **Item 2** (cache `pseudoRoyals` bitboards and add a capture blast-illegal pre-filter).
- `perft()` no longer initializes tables; the `EVASIONS`/`NON_EVASIONS` split (Item 3) can be attempted without worrying about `init` ordering.
- `is_square_attacked` bool early-exit (Item 4) and bulk pawn generation (Item 5) can be layered on top.
