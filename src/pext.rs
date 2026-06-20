//! PEXT-based sliding piece attacks (BMI2 instruction).
//!
//! When BMI2 is available, the `pext` instruction replaces the magic
//! multiplication + shift with a single instruction, giving ~2× speedup
//! for sliding piece attacks.
//!
//! Tables are built at init time using a software PEXT emulation, so
//! they work regardless of CPU support. The hot-path lookup uses hardware
//! PEXT when available via `#[target_feature(enable = "bmi2")]`.
//!
//! # Dead-code note
//!
//! The compiler cannot trace usage across `#[target_feature]` boundaries,
//! so items like the table statics appear unused. They are in fact used
//! in the hot-path `bishop_attacks_pext` / `rook_attacks_pext` functions
//! called from `attacks.rs`.

#![allow(dead_code)]

use crate::magic::{self, BISHOP_DIRS, BISHOP_MASKS, ROOK_DIRS, ROOK_MASKS};
use crate::types::*;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// CPU feature detection
// ---------------------------------------------------------------------------

/// Returns `true` if the CPU supports the BMI2 instruction set (PEXT).
///
/// On non-x86_64 architectures this always returns `false`.
pub fn has_bmi2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("bmi2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Software PEXT (for table generation without BMI2)
// ---------------------------------------------------------------------------

/// Software emulation of the `pext` instruction.
///
/// Extracts bits from `val` at positions where `mask` has 1 bits, and
/// compacts them into a contiguous index starting from bit 0.
fn pext_soft(val: u64, mask: u64) -> u64 {
    let mut result = 0u64;
    let mut bit = 0u64;
    let mut m = mask;
    while m != 0 {
        // Isolate lowest set bit.
        let lsb = m & m.wrapping_neg();
        if val & lsb != 0 {
            result |= 1 << bit;
        }
        bit += 1;
        // Clear the lowest set bit.
        m ^= lsb;
    }
    result
}

// ---------------------------------------------------------------------------
// Compile-time layout computation (popcounts, offsets, total size)
// ---------------------------------------------------------------------------

/// Compiled-time layout for a PEXT-indexed table.
struct PextLayout {
    popcounts: [u32; 64],
    offsets: [usize; 64],
    total: usize,
}

/// Compute popcounts and offsets from masks at compile time.
const fn compute_pext_layout(masks: &[Bitboard; 64]) -> PextLayout {
    let mut popcounts = [0u32; 64];
    let mut offsets = [0usize; 64];
    let mut total = 0usize;
    let mut i = 0;
    while i < 64 {
        let pc = masks[i].0.count_ones();
        popcounts[i] = pc;
        offsets[i] = total;
        total += 1usize << pc;
        i += 1;
    }
    PextLayout {
        popcounts,
        offsets,
        total,
    }
}

const ROOK_LAYOUT: PextLayout = compute_pext_layout(&ROOK_MASKS);
const BISHOP_LAYOUT: PextLayout = compute_pext_layout(&BISHOP_MASKS);

// ---------------------------------------------------------------------------
// Build PEXT-indexed attack tables
// ---------------------------------------------------------------------------

/// Build a PEXT-indexed attack table for a given piece type.
fn build_pext_table(
    directions: &[(i8, i8)],
    masks: &[Bitboard; 64],
    offsets: &[usize; 64],
    total_size: usize,
) -> Box<[Bitboard]> {
    let mut table = vec![Bitboard::EMPTY; total_size].into_boxed_slice();

    for sq in 0..64 {
        let mask = masks[sq].0;
        let offset = offsets[sq];
        let sq_enum = Square::from_u8(sq as u8);

        // Enumerate all subsets of the mask using the carry-rippler trick.
        let mut subset = 0u64;
        loop {
            let attacks = magic::sliding_attack(directions, sq_enum, Bitboard(subset));
            let idx = offset + pext_soft(subset, mask) as usize;
            table[idx] = attacks;

            // Carry-rippler: compute next subset
            subset = subset.wrapping_sub(mask) & mask;
            if subset == 0 {
                break;
            }
        }
    }

    table
}

// ---------------------------------------------------------------------------
// Lazy-initialized PEXT tables
// ---------------------------------------------------------------------------

static ROOK_PEXT_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| {
    build_pext_table(
        &ROOK_DIRS,
        &ROOK_MASKS,
        &ROOK_LAYOUT.offsets,
        ROOK_LAYOUT.total,
    )
});

static BISHOP_PEXT_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| {
    build_pext_table(
        &BISHOP_DIRS,
        &BISHOP_MASKS,
        &BISHOP_LAYOUT.offsets,
        BISHOP_LAYOUT.total,
    )
});

// ---------------------------------------------------------------------------
// Hot-path lookup functions (require BMI2)
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "bmi2")]
unsafe fn bishop_attacks_pext_impl(sq: Square, occupied: Bitboard) -> Bitboard {
    let sq_idx = sq as usize;
    let mask = BISHOP_MASKS[sq_idx];
    let occ = occupied & mask;
    let idx = core::arch::x86_64::_pext_u64(occ.0, mask.0) as usize;
    BISHOP_PEXT_TABLE[BISHOP_LAYOUT.offsets[sq_idx] + idx]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "bmi2")]
unsafe fn rook_attacks_pext_impl(sq: Square, occupied: Bitboard) -> Bitboard {
    let sq_idx = sq as usize;
    let mask = ROOK_MASKS[sq_idx];
    let occ = occupied & mask;
    let idx = core::arch::x86_64::_pext_u64(occ.0, mask.0) as usize;
    ROOK_PEXT_TABLE[ROOK_LAYOUT.offsets[sq_idx] + idx]
}

// Non-x86_64 stubs that should never be called (has_bmi2() returns false).
#[cfg(not(target_arch = "x86_64"))]
unsafe fn bishop_attacks_pext_impl(_sq: Square, _occupied: Bitboard) -> Bitboard {
    unreachable!("PEXT not available on non-x86_64")
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn rook_attacks_pext_impl(_sq: Square, _occupied: Bitboard) -> Bitboard {
    unreachable!("PEXT not available on non-x86_64")
}

/// Return the attack set for a bishop on `sq` given the `occupied` board,
/// using the BMI2 `pext` instruction.
///
/// # Safety
///
/// Caller must ensure the CPU supports BMI2 (e.g., by calling `has_bmi2()`).
/// On non-x86_64 platforms, calling this function panics.
#[inline(always)]
pub unsafe fn bishop_attacks_pext(sq: Square, occupied: Bitboard) -> Bitboard {
    // SAFETY: The caller has verified BMI2 support via has_bmi2().
    unsafe { bishop_attacks_pext_impl(sq, occupied) }
}

/// Return the attack set for a rook on `sq` given the `occupied` board,
/// using the BMI2 `pext` instruction.
///
/// # Safety
///
/// Caller must ensure the CPU supports BMI2 (e.g., by calling `has_bmi2()`).
/// On non-x86_64 platforms, calling this function panics.
#[inline(always)]
pub unsafe fn rook_attacks_pext(sq: Square, occupied: Bitboard) -> Bitboard {
    // SAFETY: The caller has verified BMI2 support via has_bmi2().
    unsafe { rook_attacks_pext_impl(sq, occupied) }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pext_soft_identity() {
        // PEXT with all-ones mask should return the original value.
        assert_eq!(pext_soft(0xDEADBEEF, 0xFFFFFFFF), 0xDEADBEEF);
    }

    #[test]
    fn test_pext_soft_compact() {
        // Extract bits 0, 2, 4 from 0b10101 = 21 -> should give 0b111 = 7
        assert_eq!(pext_soft(0b10101, 0b10101), 0b111);
    }

    #[test]
    fn test_pext_soft_sparse() {
        // val = 0xFF, mask = 0x0101010101010101 (bits 0,8,16,24,32,40,48,56)
        // Only bit 0 of val overlaps with mask bit 0 -> result = 1
        assert_eq!(pext_soft(0xFF, 0x0101010101010101), 1);
    }

    #[test]
    fn test_pext_soft_zero() {
        assert_eq!(pext_soft(0, 0xFFFFFFFF), 0);
        assert_eq!(pext_soft(0xFFFFFFFF, 0), 0);
    }

    /// Verify that the PEXT tables produce the same results as the
    /// loop-based reference for every square and every occupancy.
    #[test]
    fn test_pext_vs_loop_bishop() {
        // Force table initialization.
        LazyLock::force(&BISHOP_PEXT_TABLE);
        for sq_idx in 0..64 {
            let sq = Square::from_u8(sq_idx as u8);
            let mask = BISHOP_MASKS[sq_idx].0;
            let size = 1usize << BISHOP_LAYOUT.popcounts[sq_idx];
            let mut count = 0;
            let mut subset = 0u64;
            loop {
                let occ = Bitboard(subset);
                // Compute using PEXT table (software index)
                let pext_idx = BISHOP_LAYOUT.offsets[sq_idx] + pext_soft(subset, mask) as usize;
                let pext_atk = BISHOP_PEXT_TABLE[pext_idx];
                let loop_atk = magic::sliding_attack(&BISHOP_DIRS, sq, occ);
                assert_eq!(
                    pext_atk, loop_atk,
                    "Bishop PEXT mismatch at sq={:?}, occ=0x{:x}",
                    sq, subset
                );
                count += 1;
                subset = subset.wrapping_sub(mask) & mask;
                if subset == 0 {
                    break;
                }
            }
            assert_eq!(count, size, "Bishop PEXT count mismatch at sq={:?}", sq);
        }
    }

    #[test]
    fn test_pext_vs_loop_rook() {
        LazyLock::force(&ROOK_PEXT_TABLE);
        for sq_idx in 0..64 {
            let sq = Square::from_u8(sq_idx as u8);
            let mask = ROOK_MASKS[sq_idx].0;
            let size = 1usize << ROOK_LAYOUT.popcounts[sq_idx];
            let mut count = 0;
            let mut subset = 0u64;
            loop {
                let occ = Bitboard(subset);
                let pext_idx = ROOK_LAYOUT.offsets[sq_idx] + pext_soft(subset, mask) as usize;
                let pext_atk = ROOK_PEXT_TABLE[pext_idx];
                let loop_atk = magic::sliding_attack(&ROOK_DIRS, sq, occ);
                assert_eq!(
                    pext_atk, loop_atk,
                    "Rook PEXT mismatch at sq={:?}, occ=0x{:x}",
                    sq, subset
                );
                count += 1;
                subset = subset.wrapping_sub(mask) & mask;
                if subset == 0 {
                    break;
                }
            }
            assert_eq!(count, size, "Rook PEXT count mismatch at sq={:?}", sq);
        }
    }

    /// If BMI2 is available, verify the hardware PEXT path matches the
    /// loop-based reference on a subset of positions.
    #[test]
    fn test_pext_hardware_vs_loop() {
        if !has_bmi2() {
            return;
        }
        // Force table initialization.
        LazyLock::force(&BISHOP_PEXT_TABLE);
        LazyLock::force(&ROOK_PEXT_TABLE);

        // Test a representative set of squares and occupancies.
        for sq_idx in [0, 7, 9, 27, 36, 56, 63] {
            let sq = Square::from_u8(sq_idx as u8);
            for occ_val in [0u64, 0xFF, 0xFFFF, 0xDEADBEEF, 0xFFFFFFFFFFFFFFFF] {
                let occ = Bitboard(occ_val);
                unsafe {
                    let pext_atk = bishop_attacks_pext(sq, occ);
                    let loop_atk = magic::sliding_attack(&BISHOP_DIRS, sq, occ);
                    assert_eq!(
                        pext_atk, loop_atk,
                        "Bishop HW PEXT mismatch at sq={:?}, occ=0x{:x}",
                        sq, occ_val
                    );

                    let pext_atk = rook_attacks_pext(sq, occ);
                    let loop_atk = magic::sliding_attack(&ROOK_DIRS, sq, occ);
                    assert_eq!(
                        pext_atk, loop_atk,
                        "Rook HW PEXT mismatch at sq={:?}, occ=0x{:x}",
                        sq, occ_val
                    );
                }
            }
        }
    }
}
