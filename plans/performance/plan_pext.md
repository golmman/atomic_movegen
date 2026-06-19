# Plan: PEXT for Sliding Piece Attacks

## Overview

Replace the current `sliding_attack()` loop-based ray-casting with the x86 BMI2 `pext` (parallel bits extract) instruction. This collapses the generalised `& mask * magic >> shift` two-step lookup into a single `_pext_u64` call, giving a ~2× speedup on sliding-piece attack generation.

## Current state

Sliding attacks (`bishop_attacks`, `rook_attacks`, `queen_attacks`) use `sliding_attack()` in `src/attacks.rs`:

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

This iterates up to 4 directions × 7 squares = 28 iterations for every single sliding-attack query. Since legal-move generation calls `bishop_attacks` / `rook_attacks` / `queen_attacks` for every sliding piece of both sides on every pseudo-legal move (and many times in `legal()`), the loop overhead is significant.

## Proposed change

Use **PEXT-based magic bitboards** (also called "fancy magic bitboards" in the chess programming literature). The key insight is that only the occupied bits **along the relevant rays** determine the attack set. PEXT extracts exactly those bits into a compact index, which is used to index a pre-computed attack table.

### Step 1: Add CPU feature detection

Create a `src/pext.rs` module that:

1. Defines a function `has_bmi2() -> bool` using `std::arch::is_x86_feature_detected!("bmi2")`.
2. Provides two implementations of `sliding_attacks(sq, occupied) -> Bitboard` — one fast (PEXT), one fallback (current loop).
3. Exposes them via function pointers or a trait, set once at program start via `std::sync::Once`.

### Step 2: Pre-compute PEXT tables (init-time)

For each square (64) and each piece type (rook, bishop):

1. Build the **relevant occupancy mask**: all squares on the rays from `sq` (excluding board edges, since edge bits don't block).
2. For each relevant occupancy mask value (2^popcount(mask) entries — typically 64–4096), use PEXT to compute the index and store the resulting attack bitboard.

This can be done at compile-time via `LazyLock` or at first-use.

### Step 3: Implement `attacks_pext`

```rust
#[target_feature(enable = "bmi2")]
unsafe fn bishop_attacks_pext(sq: Square, occupied: u64) -> u64 {
    let idx = __pext_u64(occupied, BISHOP_MASKS[sq as usize]);
    BISHOP_TABLE[sq as usize][idx as usize]
}
```

(Similarly for `rook_attacks_pext`.)

### Step 4: Glue at module level

`src/attacks.rs` calls through function pointers that are initialised once:

```rust
static SLIDING_IMPL: LazyLock<SlidingImpl> = LazyLock::new(|| {
    if has_bmi2() { SlidingImpl::Pext }
    else { SlidingImpl::Loop }
});

pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    match *SLIDING_IMPL {
        SlidingImpl::Pext => unsafe { bishop_attacks_pext(sq, occupied.0).into() },
        SlidingImpl::Loop => sliding_attack(&BISHOP_DIRS, sq, occupied),
    }
}
```

## Files to create/modify

| Action | File | Description |
|--------|------|-------------|
| Create | `src/pext.rs` | PEXT tables, `has_bmi2()`, `#[target_feature]` functions |
| Edit | `src/attacks.rs` | Integrate PEXT path; keep loop fallback |
| Edit | `src/lib.rs` | Add `pub mod pext;` |

## Dependencies

- `std::arch::is_x86_feature_detected!("bmi2")` — standard library, no extra deps.
- The `#[target_feature(enable = "bmi2")]` attribute is stable Rust as of 1.69+.

## Safety

- `#[target_feature]` functions are `unsafe`. The calling code must dynamic-dispatch via `has_bmi2()` check before calling them, satisfying the safety contract.
- The rest of the crate remains `#![forbid(unsafe_code)]` — isolate `unsafe` to the PEXT module alone.

## Performance impact

- Rook/bishop/queen attacks become a single PEXT instruction + lookup (~3 CPU cycles) instead of a loop (tens of cycles for typical positions).
- The fallback path for non-BMI2 CPUs (older x86, ARM) remains the current loop, so no regression.

## Verification

1. `cargo test` — all attack-table tests pass for both PEXT and fallback paths.
2. `cargo run --example perft "FEN" 4` matches reference values for both paths.
3. `cargo run --example verify_perft 3` passes all 41 positions.

## Reference implementation: Fairy-Stockfish

Fairy-Stockfish serves as the correctness oracle and performance reference for this optimisation.

### Location
- **`Fairy-Stockfish/src/types.h`** (lines 81–104): `USE_PEXT` macro, `pext()` definition, and `HasPext` constexpr bool.
- **`Fairy-Stockfish/src/bitboard.h`** (lines 128–144): `Magic::index()` method that dispatches between PEXT and multiply+shift.
- **`Fairy-Stockfish/src/bitboard.cpp`** (lines 426–451): `init_magics()` — when `HasPext` is true, the attack table is indexed directly via `pext(b, m.mask)` and the slow magic-finding loop is skipped entirely.
- **`Fairy-Stockfish/src/Makefile`** (lines 76, 202–210, 641–647): Build system — `ARCH=x86-64-bmi2` sets `-DUSE_PEXT -mbmi2`.

### Key design points from Fairy-Stockfish

1. **Unified dispatch:** `Magic::index()` uses `if constexpr (HasPext)`, so both PEXT and non-PEXT paths share the same `Magic` struct and lookup tables. The only difference is how the table index is computed.

2. **Build-time selection:** PEXT is selected at compile time (not runtime) via `-DUSE_PEXT`. This avoids any dynamic dispatch overhead in the hot path. For a Rust crate, the equivalent is `#[cfg(target_feature = "bmi2")]` or runtime detection with `#[target_feature(enable = "bmi2")]`.

3. **LARGEBOARDS support:** For 128-bit boards (fairy variants with >64 squares), `pext(b, m) = _pext_u64(b, m) ^ (_pext_u64(b >> 64, m >> 64) << popcount(m >> 64))` — handles the upper 64-bit half.

4. **14 rider magic tables** share this same PEXT mechanism: RookH, RookV, Bishop, CannonH/V, LameDabbaba, Horse, Elephant, JanggiElephant, CannonDiag, Nightrider, GrasshopperH/V/D.

### Applicability to this plan
- For atomic chess we only need RookH, RookV, and Bishop (queen = bishop | rook, commoner uses precomputed king attacks, not magic).
- The `Magic::index()` two-path pattern (PEXT vs multiply+shift) is the model for our runtime dispatch.
- The Fairy-Stockfish magic table population loop (enumerate all occupancies, compute reference attacks, store at `pext(occ, mask)`) is the reference algorithm for building our PEXT tables.
- Fairy-Stockfish's `sliding_attack()` template function (used only for table generation, not hot path) is our correctness baseline.

## Future work

- Once PEXT is stable, the loop fallback can be removed if the project decides to require BMI2.
- The same tables can also be used for an SSE/AVX2 path if desired.
