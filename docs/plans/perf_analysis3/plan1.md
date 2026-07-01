# Plan 1 — Co-locate Magic Data into Array-of-Structs

**Optimization:** Item 1 from `analysis.md` — Co-locate magic bitboard data from 5 parallel arrays into a single array-of-structs.

**Prerequisite optimization:** Item 8 from `analysis.md` — Bit-manipulation accessors via `#[repr(u8)]` transmute (enables struct field shrink and reduces instruction count in hot paths).

**Estimated impact (Item 1 alone):** 5–12 % speedup
**Estimated impact (Item 8 alone):** 1–4 % speedup
**Combined estimated impact:** 6–15 % speedup
**Effort:** ~80 lines across `src/magic.rs`, `src/types.rs`, `src/bitboard.rs`
**Risk:** Low (mechanical transformation; all data already const; verified by full perft regression)
**Baseline command:** `cargo run --release --example verify_perft`

---

## 1. Motivation

### 1.1 The scattered-load problem (Item 1)

Every magic bitboard lookup currently loads from **5 separate arrays** in the binary's data section:

```rust
// In magic.rs — bishop_attacks():
let sq_idx = sq as usize;
let mask = BISHOP_MASKS[sq_idx];           // load #1
let idx = ((occupied & mask).0.wrapping_mul(BISHOP_MAGICS[sq_idx]))  // load #2
    >> (64 - BISHOP_INDEX_BITS[sq_idx]);   // load #3
let offset = BISHOP_OFFSETS[sq_idx];       // load #4
BISHOP_TABLE[offset + idx as usize]        // load #5 (table data)
```

These arrays (`ROOK_MASKS`, `ROOK_MAGICS`, `ROOK_INDEX_BITS`, `ROOK_OFFSETS`, `BISHOP_MASKS`, `BISHOP_MAGICS`, `BISHOP_INDEX_BITS`, `BISHOP_OFFSETS`) are independent `const` items placed in different locations in the binary's `.rodata`/`.data.rel.ro` section. On each call to `rook_attacks()` or `bishop_attacks()`:

- The CPU must load from **4 separate cache lines** (mask, magic, index_bits, offset) plus the table itself.
- Each load is a RIP-relative address with no spatial locality between accesses.
- The multiply/shift is fast (3–4 cycles), but the scattered loads create instruction-level dependencies: each load must complete before the next addressing mode can be resolved.

**Solution:** Pack the per-square constants into a single struct:

```rust
pub struct MagicEntry {
    pub mask: Bitboard,    // 8 bytes
    pub magic: u64,        // 8 bytes
    pub shift: u32,        // 4 bytes (was: index_bits → shift = 64 - index_bits)
    pub offset: u32,       // 4 bytes (was: usize)
}
// Total: 24 bytes, fits in one 64-byte cache line with room for 1 more entry.
```

Then `bishop_attacks()` becomes:

```rust
let e = &BISHOP_ENTRIES[sq as usize];      // 1 struct load (24 bytes, 1 cache line)
let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
BISHOP_TABLE[e.offset as usize + idx as usize]  // load #2 (table data)
```

This reduces 4 separate array loads → **1 struct load** (the struct is 24 bytes and fits in 1 cache line; adjacent entries are on the same or next cache line for excellent prefetch).

### 1.2 The table-lookup overhead problem (Item 8, prerequisite)

Several frequently-called accessors in `types.rs` use 64-element `SQUARES` static arrays to convert between integer indices and enum values:

| Function | Current approach | Machine cost |
|----------|-----------------|--------------|
| `Bitboard::lsb()` | `trailing_zeros()` + `SQUARES[idx]` table load | 1 ALU op + 1 load (~4 cycles) |
| `Bitboard::msb()` | `leading_zeros()` + `SQUARES[idx]` table load | 2 ALU ops + 1 load |
| `Move::from_sq()` | shift + mask + `SQUARES[idx]` table load | 2 ALU ops + 1 load |
| `Move::to_sq()` | mask + `SQUARES[idx]` table load | 1 ALU op + 1 load |
| `file_of()` | `idx % 8` + `FILES[idx % 8]` table load | 1 ALU op + 1 load |
| `rank_of()` | `idx / 8` + `RANKS[idx / 8]` table load | 1 ALU op + 1 load |
| `Piece::type_of()` | `wrapping_sub(1)` + bounds check + `TYPES[inner]` table load | 3 ALU ops + 1 load |

With `#[repr(u8)]` on the target enums, each table-lookup can be replaced by a single `transmute` (zero-cost cast of the integer discriminant):

```rust
// Before:
pub fn lsb(self) -> Square {
    let idx = self.0.trailing_zeros() as usize;
    SQUARES[idx]  // load from static array
}

// After:
pub fn lsb(self) -> Square {
    let idx = self.0.trailing_zeros() as u8;
    // Safety: trailing_zeros() returns 0..63, and Square with #[repr(u8)]
    // has valid discriminants for 0..63. Square::NONE = 64 is unreachable.
    unsafe { std::mem::transmute::<u8, Square>(idx) }
}
```

This eliminates **7 static table loads** from the hot path. While each individual load is cheap (L1 hit ~4 cycles), these functions are called millions of times per second (every `lsb()` in every pseudo-legal move, every `from_sq()`/`to_sq()` in every `legal()` check). The cumulative savings can be significant.

**Prerequisite relationship:** Adding `#[repr(u8)]` to `Square`, `PieceType`, `File`, `Rank` is a mechanical change. It also makes `sq as u8` a zero-extend rather than a sign-extend, which is slightly cheaper. The `MagicEntry` struct uses `u32` for `shift` and `offset` (smaller than `usize`), and the `sq as usize` indexing into `MagicEntry` arrays is unchanged — the main synergy is that Item 8 is quick and easy to verify, so it makes sense to land it first before tackling the more impactful Item 1.

---

## 2. Design

### 2.1 Item 8: `#[repr(u8)]` enum annotations and transmute accessors

#### 2.1.1 Enum annotations

Add `#[repr(u8)]` to four enums in `src/types.rs`:

| Enum | Current | New | Discriminant range |
|------|---------|-----|-------------------|
| `Square` | (no repr) | `#[repr(u8)]` | 0–64 (65 variants) |
| `PieceType` | (no repr, explicit =0..=5) | `#[repr(u8)]` | 0–5 |
| `File` | (no repr) | `#[repr(u8)]` | 0–7 |
| `Rank` | (no repr) | `#[repr(u8)]` | 0–7 |

`#[repr(u8)]` is valid for enums with ≤ 256 variants. All four enums satisfy this.

#### 2.1.2 Replace `SQUARES` table lookups with `transmute`

Replace the 64-element `SQUARES` static arrays in the following functions:

**`Bitboard::lsb()`** (lines 325–395):
```rust
pub fn lsb(self) -> Square {
    debug_assert!(!self.is_empty());
    let idx = self.0.trailing_zeros() as u8;
    // SAFETY: trailing_zeros() returns 0..63 when self is non-empty.
    // All discriminants 0..63 are valid Square values.
    unsafe { std::mem::transmute(idx) }
}
```

**`Bitboard::msb()`** (lines 397–467):
```rust
pub fn msb(self) -> Square {
    debug_assert!(!self.is_empty());
    let idx = (63 - self.0.leading_zeros()) as u8;
    unsafe { std::mem::transmute(idx) }
}
```

**`Move::from_sq()`** (lines 714–783):
```rust
pub fn from_sq(self) -> Square {
    let idx = ((self.0 >> 6) & 0x3f) as u8;
    unsafe { std::mem::transmute(idx) }
}
```

**`Move::to_sq()`** (lines 785–854):
```rust
pub fn to_sq(self) -> Square {
    let idx = (self.0 & 0x3f) as u8;
    unsafe { std::mem::transmute(idx) }
}
```

#### 2.1.3 Replace `file_of()` and `rank_of()` with direct arithmetic

**`file_of()`** (lines 179–192):
```rust
pub fn file_of(s: Square) -> File {
    let idx = s as u8;
    unsafe { std::mem::transmute(idx & 7) }
}
```

**`rank_of()`** (lines 194–207):
```rust
pub fn rank_of(s: Square) -> Rank {
    let idx = s as u8;
    unsafe { std::mem::transmute((idx >> 3) & 7) }
}
```

#### 2.1.4 Replace `Piece::type_of()` table lookup

**`Piece::type_of()`** (lines 630–645):
```rust
pub fn type_of(self) -> PieceType {
    let inner = (self.0 & 7).wrapping_sub(1);
    debug_assert!(inner < 6, "Piece::type_of called with invalid Piece encoding");
    unsafe { std::mem::transmute(inner) }
}
```

**Safety note:** The encoding `Piece(u8)` guarantees `(self.0 & 7)` is in 1..=6 for valid pieces (bit 0 is always set for valid pieces, wrapping_sub(1) maps to 0..=5). The `debug_assert!` catches invalid callers. `NO_PIECE` (Piece(0)) should never reach `type_of()` in the hot path — all callers either work with `piece_on(from)` (which is known to be non-empty) or guard with an `is_ok()` check.

#### 2.1.5 Remove redundant `SQUARES` / `FILES` / `RANKS` / `TYPES` arrays

After the above changes, the following static arrays are no longer used and should be removed:

| Array | Used by |
|-------|---------|
| `SQUARES` (×5 instances) | `lsb`, `msb`, `from_sq`, `to_sq`, `from_index` |
| `FILES` (in `file_of`) | `file_of` |
| `RANKS` (in `rank_of`) | `rank_of` |
| `TYPES` (in `Piece::type_of`) | `type_of` |

The `SQUARES` array inside `Square::from_index()` and `make_square()` should remain (they are used for safe access where the input index may be out of range, e.g., `from_index(i8)` is called with `to as i8 + 8` which could be ≥ 64).

#### 2.1.6 No other changes needed

The `make_square()`, `Square::from_index()`, `Square::from_u8()` functions remain unchanged — they are called from non-hot paths (FEN parsing, display) and do not need the transmute optimization. The `Piece::type_of()` return value feeds into `match pt { PieceType::Pawn => ..., PieceType::Knight => ... }` branches; the enum representation change is transparent to pattern matching.

### 2.2 Item 1: `MagicEntry` struct and array-of-structs

#### 2.2.1 Define `MagicEntry` struct

Add to `src/magic.rs`:

```rust
/// Per-square constant data for a magic bitboard lookup.
///
/// Packed into 24 bytes (Bitboard + u64 + u32 + u32) to fit in one
/// 64-byte cache line, with room for a second entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MagicEntry {
    pub mask: Bitboard,
    pub magic: u64,
    /// `64 - index_bits` — the right-shift amount for the magic index.
    pub shift: u32,
    /// Offset into the flat attack table for this square.
    pub offset: u32,
}
```

#### 2.2.2 Compute `MagicEntry` arrays at compile time

Add `const fn` to compute the arrays:

```rust
const fn compute_rook_entries() -> [MagicEntry; 64] {
    let mut entries = [MagicEntry {
        mask: Bitboard(0),
        magic: 0,
        shift: 0,
        offset: 0,
    }; 64];
    let mut total: u32 = 0;
    let mut i = 0;
    while i < 64 {
        entries[i] = MagicEntry {
            mask: ROOK_MASKS[i],
            magic: ROOK_MAGICS[i],
            shift: 64 - ROOK_INDEX_BITS[i],
            offset: total,
        };
        total += 1u32 << ROOK_INDEX_BITS[i];
        i += 1;
    }
    entries
}

const fn compute_bishop_entries() -> [MagicEntry; 64] {
    // Same pattern as rook, using BISHOP_MASKS/MAGICS/INDEX_BITS
}

pub(crate) const ROOK_ENTRIES: [MagicEntry; 64] = compute_rook_entries();
pub(crate) const BISHOP_ENTRIES: [MagicEntry; 64] = compute_bishop_entries();
```

**Constraint:** Rust `const fn` cannot call methods on `Bitboard` in const contexts in all editions. The `Bitboard(0)` literal is fine. The assignment to `entries[i]` must work in `const` context — this is supported as of Rust 1.64+ (inline const expressions).

#### 2.2.3 Update lookup functions

Replace `bishop_attacks()` and `rook_attacks()`:

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

#### 2.2.4 Keep old arrays for backward compatibility

The old `ROOK_MASKS`, `BISHOP_MASKS` arrays are still needed by:
- `pext.rs` — imports `BISHOP_MASKS`, `ROOK_MASKS`, uses them for PEXT table layout computation and hot-path mask extraction.
- `build_magic_table()` — uses masks to enumerate occupancy subsets.
- `magic.rs` tests — enumerate subsets using masks.

Keep the old `ROOK_MASKS`, `BISHOP_MASKS` arrays as-is. They are `pub(crate)` const and used by `pext.rs`. They will now be **alongside** the new `ROOK_ENTRIES`, `BISHOP_ENTRIES` arrays, not replacing them.

The old `ROOK_MAGICS`, `BISHOP_MAGICS`, `ROOK_INDEX_BITS`, `BISHOP_INDEX_BITS` arrays become **unused after the lookup functions are updated** (they were only used in the lookup functions and indirectly through `ROOK_OFFSETS`/`BISHOP_OFFSETS`). They can be kept for reference or removed. Safe to keep them — the compiler won't emit them if they're unused (`dead_code`).

The `ROOK_OFFSETS`, `BISHOP_OFFSETS` arrays are still needed by `build_magic_table()` (the table builder uses offsets to determine where to write each square's data). Keep them for now.

#### 2.2.5 Summary of array status after changes

| Array | Status | Still used by |
|-------|--------|---------------|
| `ROOK_MASKS` | **Keep** | `compute_rook_entries()`, `pext.rs`, `build_magic_table()`, tests |
| `BISHOP_MASKS` | **Keep** | `compute_bishop_entries()`, `pext.rs`, `build_magic_table()`, tests |
| `ROOK_MAGICS` | **Keep** (could remove) | `compute_rook_entries()` |
| `BISHOP_MAGICS` | **Keep** (could remove) | `compute_bishop_entries()` |
| `ROOK_INDEX_BITS` | **Keep** (could remove) | `compute_rook_entries()`, `compute_offsets()` |
| `BISHOP_INDEX_BITS` | **Keep** (could remove) | `compute_bishop_entries()`, `compute_offsets()` |
| `ROOK_OFFSETS` | **Keep** | `build_magic_table()` |
| `BISHOP_OFFSETS` | **Keep** | `build_magic_table()` |
| `ROOK_ENTRIES` | **NEW** | `rook_attacks()` |
| `BISHOP_ENTRIES` | **NEW** | `bishop_attacks()` |

---

## 3. Changes Required

### 3.1 File: `src/types.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Add `#[repr(u8)]` to `Square` | ~5 | Before `pub enum Square { ... }` |
| Add `#[repr(u8)]` to `PieceType` | ~5 | Before `pub enum PieceType { ... }` |
| Add `#[repr(u8)]` to `File` | ~5 | Before `pub enum File { ... }` |
| Add `#[repr(u8)]` to `Rank` | ~5 | Before `pub enum Rank { ... }` |
| Replace `Bitboard::lsb()` body | ~8 | Remove `SQUARES` table, use `transmute` |
| Replace `Bitboard::msb()` body | ~8 | Same pattern |
| Replace `Move::from_sq()` body | ~4 | `transmute(((self.0 >> 6) & 0x3f) as u8)` |
| Replace `Move::to_sq()` body | ~4 | `transmute((self.0 & 0x3f) as u8)` |
| Replace `file_of()` body | ~4 | `transmute(s as u8 & 7)` |
| Replace `rank_of()` body | ~4 | `transmute((s as u8 >> 3) & 7)` |
| Replace `Piece::type_of()` body | ~6 | `transmute((self.0 & 7).wrapping_sub(1))` |
| Remove unused `SQUARES`, `FILES`, `RANKS`, `TYPES` arrays | ~varies | Remove the static arrays (but keep `from_index`'s SQUARES; keep `make_square`'s SQUARES) |

### 3.2 File: `src/bitboard.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Replace `between_bb()` `make_square()` calls | ~20 | The `match f { ... }` / `match r { ... }` inside `between_bb()` produces 64 `match` cases × 2 per iteration. While `between_bb()` is not as hot as the lsb/msb/from_sq callers, we can optimize it by using the now-cheap `file_of`/`rank_of` arithmetic or `Square::from_u8()`. **Optional** — not required for Item 1 but synergizes with Item 8. |

### 3.3 File: `src/magic.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Define `MagicEntry` struct | ~12 | New struct with `mask: Bitboard`, `magic: u64`, `shift: u32`, `offset: u32` |
| Add `compute_rook_entries()` `const fn` | ~18 | Loop over 64 squares, compute offset as cumulative sum of `1 << index_bits[i]` |
| Add `compute_bishop_entries()` `const fn` | ~18 | Same pattern for bishop |
| Add `ROOK_ENTRIES` const array | ~1 | `pub(crate) const ROOK_ENTRIES: [MagicEntry; 64] = compute_rook_entries();` |
| Add `BISHOP_ENTRIES` const array | ~1 | Same |
| Replace `bishop_attacks()` body | ~5 | Use `&BISHOP_ENTRIES[sq as usize]` pattern |
| Replace `rook_attacks()` body | ~5 | Use `&ROOK_ENTRIES[sq as usize]` pattern |

### 3.4 Files not touched

- `src/attacks.rs` — unchanged (re-exports `magic::bishop_attacks` etc.)
- `src/pext.rs` — unchanged (still uses `ROOK_MASKS`, `BISHOP_MASKS` directly)
- `src/board.rs` — unchanged
- `src/movegen.rs` — unchanged
- `src/lib.rs` — unchanged
- `examples/*.rs` — unchanged

---

## 4. Implementation Order

| Step | Description | Files touched | Verification |
|------|-------------|---------------|--------------|
| **4.1** | Add `#[repr(u8)]` to `Square`, `PieceType`, `File`, `Rank` | `types.rs` | `cargo build` |
| **4.2** | Replace `lsb()`, `msb()`, `from_sq()`, `to_sq()`, `file_of()`, `rank_of()`, `type_of()` with transmute; remove unused arrays | `types.rs` | `cargo build && cargo test` |
| **4.3** | Run `cargo clippy` and `cargo fmt` | (all) | `cargo clippy && cargo fmt` |
| **4.4** | Define `MagicEntry` struct + const computation functions | `magic.rs` | `cargo build` |
| **4.5** | Add `ROOK_ENTRIES` and `BISHOP_ENTRIES` const arrays | `magic.rs` | `cargo build` |
| **4.6** | Replace `bishop_attacks()` and `rook_attacks()` to use new entries | `magic.rs` | `cargo build && cargo test` |
| **4.7** | Run `cargo clippy` and `cargo fmt` | (all) | `cargo clippy && cargo fmt` |
| **4.8** | **Baseline verification** | — | `cargo run --release --example verify_perft` |

**Why Item 8 first:** The `#[repr(u8)]` change is purely additive (no logic change, no data layout change), trivially verifiable, and has zero risk of correctness regression. Landing it first isolates the mechanical `types.rs` changes from the more impactful `magic.rs` restructuring, making bisection easier if a regression occurs.

---

## 5. Testing Against Baseline

### Correctness verification

After each step, run the unit tests:

```sh
cargo test
```

After all steps are complete, run the full perft regression:

```sh
cargo run --release --example verify_perft
```

Expected output (unchanged from baseline):
```
  Test #1    PASS (6 depths) [X.XXX s]
  Test #2    PASS (6 depths) [X.XXX s]
  ...
  Result:    41/41 passed, 0/41 failed
```

### Performance measurement

Record the total wall-clock time from the summary line:

```
Total time:  XXX.XXX s
```

Compare against the baseline from `analysis.md` (Plan 2 cumulative baseline: **107.305 s** for all 41 positions at depths 1–6).

Run the baseline first on the same machine:

```sh
# Baseline (current main with Plans 1+2):
cargo run --release --example verify_perft
# Record "Total time:" from summary line.
```

Then apply this plan's changes and run again:

```sh
# Plan 1 (this plan):
cargo run --release --example verify_perft
# Record "Total time:" from summary line.
```

Compute speedup:

```
Speedup = (baseline_time - plan1_time) / baseline_time × 100 %
```

### Expected improvement range

If Item 8 delivers 2 % (midpoint of 1–4 %) and Item 1 delivers 8 % (midpoint of 5–12 %), the combined speedup is approximately:

```
1 - (1 - 0.02) × (1 - 0.08) = 1 - 0.98 × 0.92 ≈ 9.8 %
```

This would bring total time from **107.3 s → ~96.8 s**.

**Important:** The improvements are structural (better cache utilization, fewer loads) and may show higher gains on actual hardware with real cache hierarchies vs. the Docker/cloud environment where the baseline was measured. The `verify_perft` tool includes 41 diverse positions, so the gain should be representative.

---

## 6. Performance Model

### 6.1 Why the struct load is faster

Each magic lookup currently performs 4 loads from static data:

```
Load BISHOP_MASKS[sq]    → RIP-relative address (different offset from base)
Load BISHOP_MAGICS[sq]   → RIP-relative address (different offset)
Load BISHOP_INDEX_BITS[sq] → RIP-relative address (smaller, could share page)
Load BISHOP_OFFSETS[sq]  → RIP-relative address (different offset)
```

The four arrays are at different addresses in the binary. Each load touches a potentially different cache line. The CPU must:

1. Issue load #1 (mask) — L1 hit or miss
2. Wait for load #2 (magic) — may be in-flight if prefetcher caught it, but the arrays are adjacent
3. Issue load #3 (index_bits) — small array, likely same page
4. Issue load #4 (offset) — different array, potentially different cache line

With the struct approach:

```
Load BISHOP_ENTRIES[sq]  → struct load (24 bytes, 1 cache line)
  - mask (offset 0)         → available after 1 cache-line fill
  - magic (offset 8)        → available after same cache-line fill
  - shift (offset 16)       → available after same cache-line fill
  - offset (offset 20)      → available after same cache-line fill
```

All four per-square values come from a **single cache-line fill** (64 bytes). The struct is packed as 24 bytes, so two entries (sq and sq+1) fit in one cache line — giving spatial locality for sequential square accesses (as occur in compute_checkers, compute_pinned, and some attack loops).

### 6.2 Why transmute is faster than table lookup

A static table lookup in Rust compiles to something like:

```asm
lea   rax, [rip + SQUARES]     ; materialize base address
movzx eax, byte [rdi]          ; or similar, load index
mov   eax, dword [rax + rdi*4] ; load Square from table (4 byte enum)
```

With `#[repr(u8)]` and transmute:

```asm
movzx eax, BYTE [rdi]          ; load the u8 value
; Square value is now in eax as the correct discriminant
```

The load instruction is replaced by a register move or eliminated entirely. For `lsb()` and `msb()`, the `trailing_zeros()` result is already in a register — the transmute is free (the value is already the correct discriminant).

For `from_sq()`/`to_sq()`, the shift-and-mask operations produce the index in a register — the transmute just reinterprets the bits.

### 6.3 Hot-path frequency

The approximate call frequency of these functions in a `perft(6)` run on the starting position (~119M nodes) and tactical positions (~2B nodes):

| Function | Calls per legal() call | Approx total per verify_perft run |
|----------|-----------------------|-----------------------------------|
| `lsb()` | ~2–10 (move generation loops) | 10¹¹ – 10¹² |
| `from_sq()` | 1 per move generated + 1 per legal() check | >10¹⁰ |
| `to_sq()` | 1 per move generated + 1 per legal() check | >10¹⁰ |
| `file_of()` / `rank_of()` | Called indirectly through `between_bb()`, `make_square()` | 10⁸ – 10⁹ |
| `Piece::type_of()` | ~2–3 per legal() call (piece_on, promotion type) | 10¹⁰ – 10¹¹ |
| `BISHOP_ATTACKS` / `ROOK_ATTACKS` | 3–6 per legal() call (bishop + rook + queen per attacker check) | >10¹⁰ |

Every magic lookup saved per legal() call compounds across the entire move tree.

---

## 7. Safety Considerations

### 7.1 `transmute` safety justification

All `transmute` calls in Item 8 map a **known-range integer** to an **enum with a valid discriminant**:

| Call site | Input range | Valid Square/PieceType/File/Rank range | Gap? |
|-----------|-------------|---------------------------------------|------|
| `Bitboard::lsb()` | 0..63 (guaranteed by `trailing_zeros()` on non-zero u64) | Square 0..63 | `Square::NONE` = 64 is not mapped → OK |
| `Bitboard::msb()` | 0..63 (guaranteed by `63 - leading_zeros()` on non-zero u64) | Square 0..63 | Same |
| `Move::from_sq()` | 0..63 (`(self.0 >> 6) & 0x3f` = 6-bit field) | Square 0..63 | Same |
| `Move::to_sq()` | 0..63 (`self.0 & 0x3f` = 6-bit field) | Square 0..63 | Same |
| `file_of()` | 0..7 (`s as u8 & 7`) | File 0..7 | Complete |
| `rank_of()` | 0..7 (`(s as u8 >> 3) & 7`) | Rank 0..7 | Complete |
| `Piece::type_of()` | 0..5 (wrapping_sub(1) on (self.0 & 7) where bit 0 is always set for valid pieces) | PieceType 0..5 | Complete |

**Risk of UB from invalid discriminant:** The only way to produce an out-of-range discriminant is a programming error (e.g., calling `lsb()` on an empty Bitboard, which is caught by `debug_assert!`; or calling `type_of()` on `NO_PIECE` which never happens in the hot path because the callers always check `piece_on(from)` is non-empty before calling `type_of()`).

### 7.2 Alignment of `MagicEntry`

`MagicEntry` has `Bitboard(u64)` (8 bytes), `u64` (8 bytes), `u32` (4 bytes), `u32` (4 bytes). Total = 24 bytes. The struct has alignment 8 (from the `u64` fields). In a `[MagicEntry; 64]` array, entries are at offsets 0, 24, 48, 72, ... Each entry is 8-byte aligned, and all fields are at their natural alignment. No UB.

### 7.3 `offset` field fit in `u32`

The largest table (rook) has index bits up to 12 per square. Total rook table size:

```
∑(1 << ROOK_INDEX_BITS[i]) for i in 0..64
```

The max offset is `(total_size - 1)`, which is well under 2³² (~4 billion). The actual rook table size is ~120,000 entries. `u32` is sufficient.

---

## 8. Edge Cases & Risks

| Risk | Mitigation |
|------|-----------|
| **`#[repr(u8)]` changes enum ABI** | In Rust, enums without `#[repr]` use `#[repr(C)]`-like layout only when FFI-bound. Pure Rust code treats all enum variants as opaque values — the ABI change is transparent within the crate. The `debug`/`Display` implementations are unaffected. |
| **`transmute` of invalid discriminant causes UB** | Each call site has a `debug_assert!` or invariant guarantee that the input is in range. The `Piece::type_of()` function will panic in debug builds if called on `NO_PIECE`; in release, the `transmute` would produce an invalid value, but this path is never reached in practice because `legal()` checks `piece_on(from) != NO_PIECE` before calling `type_of()`. |
| **`const fn` array assignment fails in older Rust** | The `compute_rook_entries()` function uses `while i < 64 { entries[i] = ...; i += 1; }` inside a `const fn`. This requires Rust ≥ 1.64 (for inline const expressions). The project's `Cargo.toml` should specify `edition = "2021"` which implies Rust ≥ 1.56, but for const mutation we need Rust ≥ 1.64. Check the MSRV in `Cargo.toml`. |
| **`Bitboard` constructor in const context** | `Bitboard(0)` is a tuple struct constructor, valid in const contexts. `Bitboard::EMPTY` is also const. |
| **`pext.rs` breaks if old arrays removed** | Not removing `ROOK_MASKS`/`BISHOP_MASKS` — they are kept for backward compatibility with `pext.rs`. |
| **`build_magic_table()` still needs offsets** | The old `ROOK_OFFSETS`/`BISHOP_OFFSETS` arrays are kept for `build_magic_table()`. Alternatively, we could make `build_magic_table()` iterate over `MagicEntry` arrays, but that adds unnecessary risk. The current plan keeps both old and new arrays in parallel. |
| **No perf regression from larger `.rodata` footprint** | The new `ROOK_ENTRIES` and `BISHOP_ENTRIES` arrays add 2 × 64 × 24 = 3,072 bytes of `.rodata`. The removed `SQUARES` arrays save about 5 × 64 × 1 (or 4) = 320–1280 bytes. Net increase ~2 KB — negligible. |
| **`LazyLock` atomic check still present** | The magic tables (`ROOK_TABLE`, `BISHOP_TABLE`) are still `LazyLock<Box<[Bitboard]>>`. The atomic check is per-table, not per-square, so the `MagicEntry` restructuring doesn't affect the `LazyLock` overhead. Item 9 (eliminate `LazyLock`) can be applied separately later. |

---

## 9. Relationship to Other Items

| Item | Relationship | Status |
|------|-------------|--------|
| **Item 1** (this plan) | Co-locate magic data | **This plan** |
| **Item 8** (prerequisite) | Bit-manipulation accessors | Included in this plan (Step 1) |
| **Item 2** (redundant `queen_attacks()`) | Independent; can apply after this | Not yet planned |
| **Item 3** (cache `pseudoRoyals`) | Independent; touches `board.rs` | Not yet planned |
| **Item 4** (precomputed `between_bb`) | Independent; touches `bitboard.rs` | Not yet planned |
| **Item 9** (eliminate `LazyLock`) | Synergistic (combines with struct co-location for maximum magic perf) | Not yet planned |
| **Item 10** (StateInfo field reordering) | Independent | Not yet planned |

After this plan, the next highest-impact item is **Item 2** (eliminate redundant `queen_attacks()` magic lookups), estimated at 3–8 %. It is purely mechanical (~10 lines) and directly saves 3 magic lookups per attacker check in the pseudo-royal loop and castling check.

---

## 10. Summary

| Aspect | Detail |
|--------|--------|
| **What** | (a) Add `#[repr(u8)]` to enums and replace 7 table-lookup accessors with `transmute`. (b) Define `MagicEntry` struct and replace 4 separate per-square arrays with a single array-of-structs. |
| **Why** | Reduces cache-line footprint of each magic lookup from 4 loads → 1 struct load; eliminates 7 static-table loads from hot-path accessors. |
| **Item 8 scope** | `src/types.rs`: 4 `#[repr(u8)]` annotations, 7 function bodies replaced, 5 static arrays removed. |
| **Item 1 scope** | `src/magic.rs`: 1 struct definition, 2 `const fn` computations, 2 const arrays, 2 lookup function bodies replaced. |
| **Files** | `src/types.rs` (~60 lines), `src/magic.rs` (~60 lines) |
| **Verification** | `cargo test` + `cargo run --release --example verify_perft` (all 41 positions at depths 1–6) |
| **Expected speedup** | 6–15 % combined (`verify_perft` total time from 107.3 s → ~91–101 s) |
| **Risk** | Low — mechanical transformation, all data already const, full perft regression catches regressions |
