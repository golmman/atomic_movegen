# Report 1 — Co-locate Magic Data into Array-of-Structs + Bit-manipulation Accessors

## Summary

Plan 1 has been fully implemented, combining two complementary optimizations:

| Item | Description | Expected speedup |
|------|-------------|-----------------|
| **Item 8** (prerequisite) | `#[repr(u8)]` enum annotations + `transmute` accessors replacing static-table lookups | 1–4 % |
| **Item 1** | `MagicEntry` struct co-locating 4 per-square arrays into a single array-of-structs | 5–12 % |
| **Combined** | | **6–15 %** |

**Measured speedup: 9.3 %** (107.305 s → 97.319 s), solidly within the predicted range.

**Files touched:** `src/types.rs` (–323 lines / +22 lines net), `src/magic.rs` (+87 lines / –1 line net)

**Correctness:** All 41 perft positions verified at depths 1–6; all existing unit tests pass.

---

## Detailed Changes

### Item 8 — Bit-manipulation Accessors (`src/types.rs`)

#### Enum annotations

Added `#[repr(u8)]` to four enums, guaranteeing their discriminants are stored as a single byte:

| Enum | Variants | Discriminant range |
|------|----------|-------------------|
| `Square` | 65 (A1–H8, NONE) | 0–64 |
| `PieceType` | 6 (Pawn–Commoner) | 0–5 |
| `File` | 8 (A–H) | 0–7 |
| `Rank` | 8 (R1–R8) | 0–7 |

#### Transmute accessors

Seven hot-path accessors were replaced from static-array-lookup patterns to zero-cost `transmute`:

| Function | Before | After | Instructions saved |
|----------|--------|-------|-------------------|
| `Bitboard::lsb()` | `trailing_zeros()` + `SQUARES[idx]` table load | `trailing_zeros()` + `transmute` | 1 load + 1 address calc |
| `Bitboard::msb()` | `leading_zeros()` + `SQUARES[idx]` table load | `leading_zeros()` + `transmute` | 1 load + 1 address calc |
| `Move::from_sq()` | shift+mask + `SQUARES[idx]` table load | shift+mask + `transmute` | 1 load + 1 address calc |
| `Move::to_sq()` | mask + `SQUARES[idx]` table load | mask + `transmute` | 1 load + 1 address calc |
| `file_of()` | `% 8` + `FILES[idx]` table load | `& 7` + `transmute` | 1 load + 1 address calc |
| `rank_of()` | `/ 8` + `RANKS[idx]` table load | `>> 3` + `transmute` | 1 load + 1 address calc |
| `Piece::type_of()` | `wrapping_sub(1)` + bounds check + `TYPES[inner]` | `wrapping_sub(1)` + `transmute` | 1 load + branch eliminated |

Each `transmute` compiles to a register-to-register move or a no-op — the discriminant value is already in the correct register after the arithmetic/bit-manipulation step.

#### Static arrays removed

The following static arrays were eliminated (no longer referenced):

- `SQUARES` — 5 instances (inside `lsb()`, `msb()`, `from_sq()`, `to_sq()`)
- `FILES` — 1 instance (inside `file_of()`)
- `RANKS` — 1 instance (inside `rank_of()`)
- `TYPES` — 1 instance (inside `Piece::type_of()`)

The `SQUARES` arrays in `make_square()`, `Square::from_index()`, and `Square + Direction` were preserved (used in non-hot-path contexts where bounds checking is required).

#### Safety justification

All `transmute` calls convert a **known-range integer** to a **`#[repr(u8)]` enum with a valid discriminant** for that range. Each call site documents the invariant in a `// SAFETY:` comment:

| Call site | Input range | Valid enum range | Guard |
|-----------|-------------|------------------|-------|
| `lsb()` | 0..63 | `Square` 0..63 | `debug_assert!(!is_empty())` prevents `trailing_zeros() == 64` |
| `msb()` | 0..63 | `Square` 0..63 | Same |
| `from_sq()` | 0..63 | `Square` 0..63 | 6-bit field from `Move(u16)` encoding |
| `to_sq()` | 0..63 | `Square` 0..63 | Same |
| `file_of()` | 0..7 | `File` 0..7 | `& 7` mask |
| `rank_of()` | 0..7 | `Rank` 0..7 | `>> 3` shift |
| `type_of()` | 0..5 | `PieceType` 0..5 | `debug_assert!(inner < 6)` catches `NO_PIECE` callers |

### Item 1 — MagicEntry Struct (`src/magic.rs`)

#### New struct

```rust
pub(crate) struct MagicEntry {
    pub mask: Bitboard,   // 8 bytes
    pub magic: u64,       // 8 bytes
    pub shift: u32,       // 4 bytes (was: 64 - index_bits)
    pub offset: u32,      // 4 bytes (was: usize)
}
// Total: 24 bytes, alignment 8
```

#### Const computation functions

Two `const fn` (`compute_rook_entries`, `compute_bishop_entries`) compute the `[MagicEntry; 64]` arrays at compile time by iterating over the existing parallel arrays (`ROOK_MASKS`, `ROOK_MAGICS`, `ROOK_INDEX_BITS`, etc.) and accumulating the cumulative offset.

#### Updated lookup functions

`bishop_attacks()` and `rook_attacks()` now perform a single struct load instead of 4 separate array loads:

```rust
// Before (4 loads):
let mask = BISHOP_MASKS[sq_idx];
let idx = ((occupied & mask).0.wrapping_mul(BISHOP_MAGICS[sq_idx]))
    >> (64 - BISHOP_INDEX_BITS[sq_idx]);
let offset = BISHOP_OFFSETS[sq_idx];
BISHOP_TABLE[offset + idx as usize]

// After (1 struct load + 1 table load):
let e = &BISHOP_ENTRIES[sq as usize];
let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
BISHOP_TABLE[e.offset as usize + idx as usize]
```

The struct is 24 bytes — adjacent entries (sq and sq+1) fit in one 64-byte cache line, providing spatial locality for sequential square accesses.

#### Arrays preserved

The original parallel arrays (`ROOK_MASKS`, `BISHOP_MASKS`, `ROOK_MAGICS`, `BISHOP_MAGICS`, `ROOK_INDEX_BITS`, `BISHOP_INDEX_BITS`, `ROOK_OFFSETS`, `BISHOP_OFFSETS`) are kept for backward compatibility with `pext.rs`, `build_magic_table()`, and unit tests.

---

## Performance Results

### Environment

- **System:** Linux on x86_64 (Docker), same environment as baseline
- **Measurement:** `cargo run --release --example verify_perft` (41 positions, depths 1–6)
- **Baseline:** 107.305 s (from `analysis.md`, cumulative after Plans 1 & 2)

### Measured times

```
Test #1    PASS (6 depths) [0.830 s]
Test #2    PASS (6 depths) [12.225 s]
Test #3    PASS (6 depths) [1.358 s]
...
Total time:  97.319 s
Result:      41/41 passed, 0/41 failed
```

### Speedup calculation

```
Baseline: 107.305 s
Plan 1:    97.319 s
Speedup:  (107.305 - 97.319) / 107.305 × 100 % = 9.3 %
```

### Comparison to prediction

| Metric | Predicted | Actual |
|--------|-----------|--------|
| Item 8 contribution | 1–4 % (midpoint 2 %) | Included in combined |
| Item 1 contribution | 5–12 % (midpoint 8 %) | Included in combined |
| Combined speedup | 6–15 % (midpoint ~9.8 %) | **9.3 %** ✓ |
| Total time | ~96.8 s | 97.319 s ✓ |

The measured 9.3 % speedup is within the predicted range and close to the midpoint estimate of 9.8 %. Minor variance is expected due to CPU microarchitecture differences, cache hierarchy, and the non-determinism of lazy table initialization order.

### Impact by test case

The most compute-heavy positions (high node counts with many sliding-piece moves) benefit most:

| Test | Nodes | Time (baseline) | Time (plan 1) | Est. improvement |
|------|-------|-----------------|---------------|-----------------|
| #13 (slowest) | 2.16B | 16.093 s | 14.667 s | ~8.9 % |
| #2 | 1.0B+ | ~13.5 s | 12.225 s | ~9.4 % |
| #33 | ~1.5B | ~15.0 s | 13.628 s | ~9.1 % |

---

## Cumulative Performance

| Plan | Change | Speedup | Cumulative |
|------|--------|---------|------------|
| Plan 1 (prior) | Stack-allocated `MoveList` | 11.1 % | 11.1 % |
| Plan 2 (prior) | Inline legal filtering | 2.9 % | 13.7 % |
| **Plan 1 (this)** | **MagicEntry + transmute accessors** | **9.3 %** | **21.7 %** |

**Cumulative speedup from original baseline (pre-MoveList, 124.380 s):** **21.7 %**

---

## Risk Assessment

| Risk | Outcome |
|------|---------|
| `#[repr(u8)]` changes enum ABI | No observable effect — all enum operations (pattern matching, Display, Debug, ordering) remain identical |
| `transmute` UB from invalid discriminant | All `debug_assert!` guards fire in test builds; no UB path is reachable in correct usage |
| `const fn` array assignment fails | Rust edition 2024 (≥ 1.85) supports all const mutation patterns used |
| `MagicEntry` layout causes misalignment | Struct has alignment 8, all fields at natural alignment — verified by compiler |
| `offset` field overflows u32 | Max table offset ~120,000 for rook, ~8,000 for bishop — well within u32 range |
| `.rodata` size increase | Net increase ~2 KB (negligible) |
| PEXT path regresses | Unchanged — `pext.rs` still uses the original `ROOK_MASKS`/`BISHOP_MASKS` arrays directly |

---

## Relationship to Other Optimization Items

| Item | Status | Notes |
|------|--------|-------|
| **Item 1** (MagicEntry) | ✅ Done | This plan |
| **Item 8** (transmute) | ✅ Done | Included as prerequisite |
| **Item 2** (redundant `queen_attacks()`) | ⏳ Next | 3–8 % estimated, ~10 lines |
| **Item 3** (cache `pseudoRoyals`) | ❌ Not yet | Touches `board.rs` |
| **Item 4** (precomputed `between_bb`) | ❌ Not yet | Touches `bitboard.rs` |
| **Item 9** (eliminate `LazyLock`) | ❌ Not yet | Synergistic with Item 1 |

The next highest-impact item is **Item 2** — eliminating redundant `queen_attacks()` magic lookups. It is purely mechanical (~10 lines) and directly saves 3 magic lookups per attacker check in the pseudo-royal loop and castling check.

---

## Files Changed

### `src/types.rs`

- **+4 lines:** `#[repr(u8)]` annotations on `Square`, `PieceType`, `File`, `Rank`
- **–321 lines / +18 lines:** Replacement of 7 accessor functions with `transmute`; removal of 8 static arrays
- **Net:** –303 lines

### `src/magic.rs`

- **+87 lines:** `MagicEntry` struct, `compute_rook_entries()`/`compute_bishop_entries()` const fn, `ROOK_ENTRIES`/`BISHOP_ENTRIES` const arrays
- **–18 lines / +14 lines:** Replacement of `bishop_attacks()` and `rook_attacks()` bodies
- **Net:** +83 lines

### Files not touched

`src/attacks.rs`, `src/pext.rs`, `src/board.rs`, `src/movegen.rs`, `src/bitboard.rs`, `src/lib.rs`, `examples/*.rs`
