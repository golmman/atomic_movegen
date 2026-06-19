# Plan: Magic Bitboards for Sliding Attacks

## Overview

Replace the current ray-casting loop (`sliding_attack()`) with **magic bitboards** — a pre-computed lookup table technique for rook and bishop attacks. This replaces a variable-length loop (up to 28 iterations) with a constant-time table lookup, at the cost of a 64–128 KiB table per piece type.

Unlike the PEXT approach (which requires x86 BMI2 and `unsafe`), magic bitboards are pure safe Rust using only multiplication and shift.

## Current state

Sliding attacks use `sliding_attack()` in `src/attacks.rs`:

```rust
fn sliding_attack(directions: &[(i8, i8)], sq: Square, occupied: Bitboard) -> Bitboard {
    let mut result = 0u64;
    let s_idx = sq as i8;
    let sf = s_idx % 8;
    let sr = s_idx / 8;
    for &(df, dr) in directions {
        let mut f = sf + df;
        let mut r = sr + dr;
        while f >= 0 && f < 8 && r >= 0 && r < 8 {
            let idx = (r * 8 + f) as usize;
            result |= 1u64 << idx;
            if occupied.0 & (1u64 << idx) != 0 { break; }
            f += df;
            r += dr;
        }
    }
    Bitboard(result)
}
```

This is called from `bishop_attacks()`, `rook_attacks()`, and indirectly from `queen_attacks()` every time sliding-piece attacks are needed.

## Magic bitboard theory

For a given square `sq` and occupancy bitboard `occupied`, the attack set for a slider is:

```
index = ((occupied & mask[sq]) * magic[sq]) >> (64 - shift[sq])
attacks = table[sq][index]
```

Where:
- `mask[sq]` is the relevant occupancy mask (squares on the rays from `sq` that can block).
- `magic[sq]` is a carefully chosen multiplier that maps each distinct occupancy pattern to a unique index.
- `shift[sq]` = 64 - popcount(mask[sq]) ensures a minimal perfect hash.
- `table[sq][index]` is the pre-computed attack set for that occupancy pattern.

## Proposed change

### Step 1: Create `src/magic.rs`

A new module that:

1. Defines the magic bitboard tables (rooks and bishops) as `static` or `LazyLock` data.
2. Provides `bishop_attacks_magic(sq: Square, occupied: Bitboard) -> Bitboard` and `rook_attacks_magic(...)` functions.
3. Generates the tables at program start (or uses compile-time constants).

### Step 2: Pre-compute magic numbers

Use **fixed magic numbers** that are known to work (rather than searching for them at runtime). Many well-known magic numbers for standard chess are available (e.g., from Stockfish, Crafty, or the chess programming wiki). Since atomic chess uses the same board geometry, standard chess magic numbers work directly.

The magics need to be verified — for each square, for every possible occupancy pattern, the magic must map it to a unique index within `1 << popcount(mask)` entries.

Since we are in a controlled environment with a fixed set of magics, we can embed them as lookup tables and validate at test time.

### Step 3: Table initialisation

```rust
use std::sync::LazyLock;

const ROOK_MAGICS: [u64; 64] = [
    // ... from known table
];
const ROOK_SHIFTS: [u32; 64] = [
    // ... popcount(rook_mask[sq])
];
const BISHOP_MAGICS: [u64; 64] = [
    // ...
];
const BISHOP_SHIFTS: [u32; 64] = [
    // ...
];

static ROOK_TABLE: LazyLock<Vec<[Bitboard; 4096]>> = ...;
static BISHOP_TABLE: LazyLock<Vec<[Bitboard; 512]>> = ...;
```

(The max table size for rooks is 4096 entries per square, bishops is 512 per square.)

### Step 4: Expose functions

```rust
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let mask = BISHOP_MASKS[sq as usize];
    let idx = ((occupied & mask).0.wrapping_mul(BISHOP_MAGICS[sq as usize])) >> (64 - BISHOP_SHIFTS[sq as usize]);
    BISHOP_TABLE[sq as usize][idx as usize]
}

pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let mask = ROOK_MASKS[sq as usize];
    let idx = ((occupied & mask).0.wrapping_mul(ROOK_MAGICS[sq as usize])) >> (64 - ROOK_SHIFTS[sq as usize]);
    ROOK_TABLE[sq as usize][idx as usize]
}
```

### Step 5: Remove `sliding_attack()` and the direction constants

Once magic bitboards are verified and the old loop is no longer called, remove the `sliding_attack()` function and the `ROOK_DIRS`/`BISHOP_DIRS` constants from `src/attacks.rs`.

## Files to create/modify

| Action | File | Description |
|--------|------|-------------|
| Create | `src/magic.rs` | Magic number tables, masks, table generation, lookup functions |
| Edit | `src/attacks.rs` | Replace `sliding_attack` calls with magic lookups; remove loop-based functions |
| Edit | `src/lib.rs` | Add `pub mod magic;` |

## Memory overhead

- Rook tables: 64 squares × 4096 entries × 8 bytes = 2 MiB (if all max-size). In practice, many squares at edges have far fewer entries, so typically ~800 KiB.
- Bishop tables: 64 squares × 512 entries × 8 bytes = 256 KiB.
- Total: ~1–2 MiB. This is acceptable for a movegen crate. Can be further reduced with "fancy" magic bitboards that use a single shared table.

If memory is a concern, `Box<[Bitboard]>` per square instead of `[[Bitboard; 4096]; 64]` saves space for edge squares.

## Performance impact

- Sliding attacks become 2–4 CPU instructions (and, mul, shr, load) instead of a loop.
- In `legal()`, which currently calls `bishop_attacks`/`rook_attacks`/`queen_attacks` 20+ times per position (once per attacker type per commoner), this is a significant win.
- No `unsafe` code — pure safe Rust with only `u64` arithmetic.

## Verification

1. **Correctness test**: For every square and every possible relevant occupancy pattern, assert that the magic lookup matches the loop-based `sliding_attack()`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_magic_vs_loop_bishop() {
        for sq in 0..64 {
            let sq = Square::from_u8(sq);
            // Test a representative set of occupancy patterns
            for occ in 0..(1u64 << BISHOP_SHIFTS[sq as usize]) {
                let occ_bb = Bitboard(occ); // simplified; need to properly expand
                assert_eq!(
                    bishop_attacks_magic(sq, occ_bb),
                    bishop_attacks_loop(sq, occ_bb),
                    "Mismatch at sq={:?}, occ={:x}", sq, occ
                );
            }
        }
    }
}
```

2. `cargo test` — all existing tests pass.
3. `cargo run --example verify_perft 5` — matches all reference values.

## Alternative: PEXT vs Magic

| Aspect | Magic (this plan) | PEXT (separate plan) |
|--------|-------------------|----------------------|
| `unsafe` | No | Yes |
| CPU requirement | Any x86/ARM | x86 with BMI2 |
| Table size | ~1–2 MiB | ~2–4 KiB (tiny) |
| Speed | ~3–4 ops | ~1 instruction |
| Complexity | Moderate | Low (once tables built) |

Both can coexist: use PEXT when available, fall back to magic otherwise. However, the prompt asks for a *dedicated* magic plan; start with magic since it's pure safe Rust and immediately beneficial. PEXT can be layered on top later.

## Reference implementation: Fairy-Stockfish

Fairy-Stockfish serves as the correctness oracle and performance reference for this optimisation.

### Location
- **`Fairy-Stockfish/src/bitboard.h`** (lines 121–145): `struct Magic` — the core data structure holding `mask`, `magic`, `attacks` pointer, and `shift`.
- **`Fairy-Stockfish/src/bitboard.h`** (lines 403–431): `rider_attacks_bb()` — the template-based dispatch that selects the right magic table for a given rider type and calls `Magic::index()`.
- **`Fairy-Stockfish/src/bitboard.cpp`** (lines 43–60): Declaration of 14 separate magic table arrays (RookMagicsH, RookMagicsV, BishopMagics, etc.).
- **`Fairy-Stockfish/src/bitboard.cpp`** (lines 382–498): `init_magics()` — full table initialisation: for each square, enumerate all occupancy subsets via carry-rippler, compute reference attacks via `sliding_attack<>()`, and populate the magic table.
- **`Fairy-Stockfish/src/magic.h`**: Precomputed magic multipliers (for the non-PEXT path).

### Key design points from Fairy-Stockfish

1. **14 separate magic tables** for different rider types: RookH (horizontal), RookV (vertical), Bishop, CannonH/V, LameDabbaba, Horse, Elephant, JanggiElephant, CannonDiag, Nightrider, GrasshopperH/V/D. For atomic chess we only need RookH, RookV, and Bishop.

2. **Two-path index computation** (`Magic::index()`):
   ```cpp
   unsigned index(Bitboard occupied) const {
       if (HasPext)
           return unsigned(pext(occupied, mask));
       return unsigned(((occupied & mask) * magic) >> shift);
   }
   ```
   The multiply+shift path (traditional magic) and the PEXT path share the same struct and tables — the only difference is how the index is computed.

3. **Template-based piece dispatch** (`rider_attacks_bb<RiderType R>()`, bitboard.h:426–431):
   ```cpp
   inline Bitboard rider_attacks_bb(RiderType R, Square s, Bitboard occupied) {
       const Magic& m = magics[lsb(R)][s];
       return m.attacks[m.index(occupied)];
   }
   ```
   And `attacks_bb()` (bitboard.h:473–488) ORs together results from multiple rider types for compound pieces (e.g., queen = bishop + rook hor + rook ver).

4. **Table generation via carry-rippler** (bitboard.cpp:382–498): The standard algorithm — for a mask with `b` bits, iterate `subset = (subset - mask) & mask` to enumerate all `2^b` occupancy patterns, compute reference attack via `sliding_attack<>()` loop, and store at `magics[sq].attacks[index]`.

5. **Precomputed magics** (`magic.h`): For non-PEXT builds, Fairy-Stockfish ships precomputed magic multipliers so the expensive magic-finding search is not needed at runtime. The `init_magics()` function skips the search loop and directly constructs tables using the known-good magics.

### Applicability to this plan
- Our `src/magic.rs` will implement the same `Magic` struct (without the PEXT branch for now), using precomputed magics for standard chess (RookH, RookV, Bishop).
- The carry-rippler loop from `init_magics()` is the reference algorithm for building our attack tables at init time.
- Fairy-Stockfish's `sliding_attack<>()` template (bitboard.cpp, used only for table generation) is the correctness baseline that our magic lookups must match.
- The 64-bit only implementation (no LARGEBOARDS) simplifies things — we only need standard chess square indexing (0–63).

## Future work

- Optimise `queen_attacks()` as `bishop_attacks(sq, occ) | rook_attacks(sq, occ)` (already done).
- Consider collapsing the table into a single flat array indexed by `(sq << shift) | index` to reduce branch mispredictions.
