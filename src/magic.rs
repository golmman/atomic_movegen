//! Magic bitboards for sliding piece attacks.
//!
//! Replaces the ray-casting loop with a constant-time table lookup
//! using precomputed magic multipliers.
//!
//! Tables are precomputed as compile-time `static` arrays; masks,
//! magic numbers, and index bits are `const` arrays (zero indirection).

#![allow(long_running_const_eval)]

use crate::types::*;

const ROOK_MAGICS: [u64; 64] = [
    0xa8002c000108020,
    0x6c00049b0002001,
    0x100200010090040,
    0x2480041000800801,
    0x280028004000800,
    0x900410008040022,
    0x280020001001080,
    0x2880002041000080,
    0xa000800080400034,
    0x4808020004000,
    0x2290802004801000,
    0x411000d00100020,
    0x402800800040080,
    0xb000401004208,
    0x2409000100040200,
    0x1002100004082,
    0x22878001e24000,
    0x1090810021004010,
    0x801030040200012,
    0x500808008001000,
    0xa08018014000880,
    0x8000808004000200,
    0x201008080010200,
    0x801020000441091,
    0x800080204005,
    0x1040200040100048,
    0x120200402082,
    0xd14880480100080,
    0x12040280080080,
    0x100040080020080,
    0x9020010080800200,
    0x813241200148449,
    0x491604001800080,
    0x100401000402001,
    0x4820010021001040,
    0x400402202000812,
    0x209009005000802,
    0x810800601800400,
    0x4301083214000150,
    0x204026458e001401,
    0x40204000808000,
    0x8001008040010020,
    0x8410820820420010,
    0x1003001000090020,
    0x804040008008080,
    0x12000810020004,
    0x1000100200040208,
    0x430000a044020001,
    0x280009023410300,
    0xe0100040002240,
    0x200100401700,
    0x2244100408008080,
    0x8000400801980,
    0x2000810040200,
    0x8010100228810400,
    0x2000009044210200,
    0x4080008040102101,
    0x40002080411d01,
    0x2005524060000901,
    0x502001008400422,
    0x489a000810200402,
    0x1004400080a13,
    0x4000011008020084,
    0x26002114058042,
];

const BISHOP_MAGICS: [u64; 64] = [
    0x89a1121896040240,
    0x2004844802002010,
    0x2068080051921000,
    0x62880a0220200808,
    0x4042004000000,
    0x100822020200011,
    0xc00444222012000a,
    0x28808801216001,
    0x400492088408100,
    0x201c401040c0084,
    0x840800910a0010,
    0x82080240060,
    0x2000840504006000,
    0x30010c4108405004,
    0x1008005410080802,
    0x8144042209100900,
    0x208081020014400,
    0x4800201208ca00,
    0xf18140408012008,
    0x1004002802102001,
    0x841000820080811,
    0x40200200a42008,
    0x800054042000,
    0x88010400410c9000,
    0x520040470104290,
    0x1004040051500081,
    0x2002081833080021,
    0x400c00c010142,
    0x941408200c002000,
    0x658810000806011,
    0x188071040440a00,
    0x4800404002011c00,
    0x104442040404200,
    0x511080202091021,
    0x4022401120400,
    0x80c0040400080120,
    0x8040010040820802,
    0x480810700020090,
    0x102008e00040242,
    0x809005202050100,
    0x8002024220104080,
    0x431008804142000,
    0x19001802081400,
    0x200014208040080,
    0x3308082008200100,
    0x41010500040c020,
    0x4012020c04210308,
    0x208220a202004080,
    0x111040120082000,
    0x6803040141280a00,
    0x2101004202410000,
    0x8200000041108022,
    0x21082088000,
    0x2410204010040,
    0x40100400809000,
    0x822088220820214,
    0x40808090012004,
    0x910224040218c9,
    0x402814422015008,
    0x90014004842410,
    0x1000042304105,
    0x10008830412a00,
    0x2520081090008908,
    0x40102000a0a60140,
];

const ROOK_INDEX_BITS: [u32; 64] = [
    12, 11, 11, 11, 11, 11, 11, 12, 11, 10, 10, 10, 10, 10, 10, 11, 11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11, 11, 10, 10, 10, 10, 10, 10, 11, 11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11, 12, 11, 11, 11, 11, 11, 11, 12,
];

const BISHOP_INDEX_BITS: [u32; 64] = [
    6, 5, 5, 5, 5, 5, 5, 6, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 7, 7, 7, 7, 5, 5, 5, 5, 7, 9, 9, 7, 5, 5,
    5, 5, 7, 9, 9, 7, 5, 5, 5, 5, 7, 7, 7, 7, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 6, 5, 5, 5, 5, 5, 5, 6,
];

pub(crate) const ROOK_MASKS: [Bitboard; 64] = [
    Bitboard(0x000101010101017e),
    Bitboard(0x000202020202027c),
    Bitboard(0x000404040404047a),
    Bitboard(0x0008080808080876),
    Bitboard(0x001010101010106e),
    Bitboard(0x002020202020205e),
    Bitboard(0x004040404040403e),
    Bitboard(0x008080808080807e),
    Bitboard(0x0001010101017e00),
    Bitboard(0x0002020202027c00),
    Bitboard(0x0004040404047a00),
    Bitboard(0x0008080808087600),
    Bitboard(0x0010101010106e00),
    Bitboard(0x0020202020205e00),
    Bitboard(0x0040404040403e00),
    Bitboard(0x0080808080807e00),
    Bitboard(0x00010101017e0100),
    Bitboard(0x00020202027c0200),
    Bitboard(0x00040404047a0400),
    Bitboard(0x0008080808760800),
    Bitboard(0x00101010106e1000),
    Bitboard(0x00202020205e2000),
    Bitboard(0x00404040403e4000),
    Bitboard(0x00808080807e8000),
    Bitboard(0x000101017e010100),
    Bitboard(0x000202027c020200),
    Bitboard(0x000404047a040400),
    Bitboard(0x0008080876080800),
    Bitboard(0x001010106e101000),
    Bitboard(0x002020205e202000),
    Bitboard(0x004040403e404000),
    Bitboard(0x008080807e808000),
    Bitboard(0x0001017e01010100),
    Bitboard(0x0002027c02020200),
    Bitboard(0x0004047a04040400),
    Bitboard(0x0008087608080800),
    Bitboard(0x0010106e10101000),
    Bitboard(0x0020205e20202000),
    Bitboard(0x0040403e40404000),
    Bitboard(0x0080807e80808000),
    Bitboard(0x00017e0101010100),
    Bitboard(0x00027c0202020200),
    Bitboard(0x00047a0404040400),
    Bitboard(0x0008760808080800),
    Bitboard(0x00106e1010101000),
    Bitboard(0x00205e2020202000),
    Bitboard(0x00403e4040404000),
    Bitboard(0x00807e8080808000),
    Bitboard(0x007e010101010100),
    Bitboard(0x007c020202020200),
    Bitboard(0x007a040404040400),
    Bitboard(0x0076080808080800),
    Bitboard(0x006e101010101000),
    Bitboard(0x005e202020202000),
    Bitboard(0x003e404040404000),
    Bitboard(0x007e808080808000),
    Bitboard(0x7e01010101010100),
    Bitboard(0x7c02020202020200),
    Bitboard(0x7a04040404040400),
    Bitboard(0x7608080808080800),
    Bitboard(0x6e10101010101000),
    Bitboard(0x5e20202020202000),
    Bitboard(0x3e40404040404000),
    Bitboard(0x7e80808080808000),
];

pub(crate) const BISHOP_MASKS: [Bitboard; 64] = [
    Bitboard(0x0040201008040200),
    Bitboard(0x0000402010080400),
    Bitboard(0x0000004020100a00),
    Bitboard(0x0000000040221400),
    Bitboard(0x0000000002442800),
    Bitboard(0x0000000204085000),
    Bitboard(0x0000020408102000),
    Bitboard(0x0002040810204000),
    Bitboard(0x0020100804020000),
    Bitboard(0x0040201008040000),
    Bitboard(0x00004020100a0000),
    Bitboard(0x0000004022140000),
    Bitboard(0x0000000244280000),
    Bitboard(0x0000020408500000),
    Bitboard(0x0002040810200000),
    Bitboard(0x0004081020400000),
    Bitboard(0x0010080402000200),
    Bitboard(0x0020100804000400),
    Bitboard(0x004020100a000a00),
    Bitboard(0x0000402214001400),
    Bitboard(0x0000024428002800),
    Bitboard(0x0002040850005000),
    Bitboard(0x0004081020002000),
    Bitboard(0x0008102040004000),
    Bitboard(0x0008040200020400),
    Bitboard(0x0010080400040800),
    Bitboard(0x0020100a000a1000),
    Bitboard(0x0040221400142200),
    Bitboard(0x0002442800284400),
    Bitboard(0x0004085000500800),
    Bitboard(0x0008102000201000),
    Bitboard(0x0010204000402000),
    Bitboard(0x0004020002040800),
    Bitboard(0x0008040004081000),
    Bitboard(0x00100a000a102000),
    Bitboard(0x0022140014224000),
    Bitboard(0x0044280028440200),
    Bitboard(0x0008500050080400),
    Bitboard(0x0010200020100800),
    Bitboard(0x0020400040201000),
    Bitboard(0x0002000204081000),
    Bitboard(0x0004000408102000),
    Bitboard(0x000a000a10204000),
    Bitboard(0x0014001422400000),
    Bitboard(0x0028002844020000),
    Bitboard(0x0050005008040200),
    Bitboard(0x0020002010080400),
    Bitboard(0x0040004020100800),
    Bitboard(0x0000020408102000),
    Bitboard(0x0000040810204000),
    Bitboard(0x00000a1020400000),
    Bitboard(0x0000142240000000),
    Bitboard(0x0000284402000000),
    Bitboard(0x0000500804020000),
    Bitboard(0x0000201008040200),
    Bitboard(0x0000402010080400),
    Bitboard(0x0002040810204000),
    Bitboard(0x0004081020400000),
    Bitboard(0x000a102040000000),
    Bitboard(0x0014224000000000),
    Bitboard(0x0028440200000000),
    Bitboard(0x0050080402000000),
    Bitboard(0x0020100804020000),
    Bitboard(0x0040201008040200),
];

const ROOK_TABLE_SIZE: usize = crate::util::total_table_size(&ROOK_INDEX_BITS);
const BISHOP_TABLE_SIZE: usize = crate::util::total_table_size(&BISHOP_INDEX_BITS);

pub(crate) const ROOK_DIRS: [(i8, i8); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
pub(crate) const BISHOP_DIRS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

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

/// Compute `ROOK_ENTRIES` at compile time from the raw parallel arrays.
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

/// Compute `BISHOP_ENTRIES` at compile time from the raw parallel arrays.
const fn compute_bishop_entries() -> [MagicEntry; 64] {
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
            mask: BISHOP_MASKS[i],
            magic: BISHOP_MAGICS[i],
            shift: 64 - BISHOP_INDEX_BITS[i],
            offset: total,
        };
        total += 1u32 << BISHOP_INDEX_BITS[i];
        i += 1;
    }
    entries
}

pub(crate) const ROOK_ENTRIES: [MagicEntry; 64] = compute_rook_entries();
pub(crate) const BISHOP_ENTRIES: [MagicEntry; 64] = compute_bishop_entries();

/// Compute sliding-piece attacks by ray-casting in the given directions.
///
/// Walks along each direction from `sq` until hitting the edge of the board
/// or an occupied square (inclusive). Used as the reference implementation
/// for building magic bitboard tables.
pub(crate) const fn sliding_attack(
    directions: &[(i8, i8)],
    sq: Square,
    occupied: Bitboard,
) -> Bitboard {
    let mut result = 0u64;
    let s_idx = sq as i8;
    let sf = s_idx % 8;
    let sr = s_idx / 8;

    let mut i = 0;
    while i < directions.len() {
        let (df, dr) = directions[i];
        let mut f = sf + df;
        let mut r = sr + dr;
        while f >= 0 && f < 8 && r >= 0 && r < 8 {
            let idx = (r * 8 + f) as usize;
            result |= 1u64 << idx;
            if occupied.0 & (1u64 << idx) != 0 {
                break;
            }
            f += df;
            r += dr;
        }
        i += 1;
    }
    Bitboard(result)
}

/// Build the precomputed rook attack table at compile time.
const fn build_rook_table() -> [Bitboard; ROOK_TABLE_SIZE] {
    let mut table = [Bitboard::EMPTY; ROOK_TABLE_SIZE];
    let mut sq: usize = 0;
    while sq < 64 {
        let e = ROOK_ENTRIES[sq];
        let mask = e.mask.0;
        let magic = e.magic;
        let shift = e.shift;
        let offset = e.offset as usize;
        let size = 1usize << (64 - shift);
        let sq_enum = SQUARES[sq];

        let mut subset: u64 = 0;
        let mut count: usize = 0;
        loop {
            let idx = (subset.wrapping_mul(magic) >> shift) as usize;
            assert!(idx < size, "index out of bounds");
            table[offset + idx] = sliding_attack(&ROOK_DIRS, sq_enum, Bitboard(subset));
            count += 1;

            subset = subset.wrapping_sub(mask) & mask;
            if subset == 0 {
                break;
            }
        }
        assert!(count == size, "wrong number of subsets");
        sq += 1;
    }
    table
}

/// Build the precomputed bishop attack table at compile time.
const fn build_bishop_table() -> [Bitboard; BISHOP_TABLE_SIZE] {
    let mut table = [Bitboard::EMPTY; BISHOP_TABLE_SIZE];
    let mut sq: usize = 0;
    while sq < 64 {
        let e = BISHOP_ENTRIES[sq];
        let mask = e.mask.0;
        let magic = e.magic;
        let shift = e.shift;
        let offset = e.offset as usize;
        let size = 1usize << (64 - shift);
        let sq_enum = SQUARES[sq];

        let mut subset: u64 = 0;
        let mut count: usize = 0;
        loop {
            let idx = (subset.wrapping_mul(magic) >> shift) as usize;
            assert!(idx < size, "index out of bounds");
            table[offset + idx] = sliding_attack(&BISHOP_DIRS, sq_enum, Bitboard(subset));
            count += 1;

            subset = subset.wrapping_sub(mask) & mask;
            if subset == 0 {
                break;
            }
        }
        assert!(count == size, "wrong number of subsets");
        sq += 1;
    }
    table
}

/// Precomputed rook attack table. Indexed by `ROOK_ENTRIES[sq].offset + magic_index`.
static ROOK_TABLE: [Bitboard; ROOK_TABLE_SIZE] = build_rook_table();

/// Precomputed bishop attack table. Indexed by `BISHOP_ENTRIES[sq].offset + magic_index`.
static BISHOP_TABLE: [Bitboard; BISHOP_TABLE_SIZE] = build_bishop_table();

/// No-op; magic tables are precomputed at compile time.
pub(crate) fn init() {}

/// Return the attack set for a bishop on `sq` given the `occupied` board.
#[inline(always)]
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let e = &BISHOP_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    BISHOP_TABLE[e.offset as usize + idx as usize]
}

/// Return the attack set for a rook on `sq` given the `occupied` board.
#[inline(always)]
pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let e = &ROOK_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    ROOK_TABLE[e.offset as usize + idx as usize]
}

/// Return the attack set for a queen (bishop + rook).
#[inline(always)]
pub fn queen_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)
}

#[cfg(test)]
pub fn bishop_attacks_loop(sq: Square, occupied: Bitboard) -> Bitboard {
    sliding_attack(&BISHOP_DIRS, sq, occupied)
}

#[cfg(test)]
pub fn rook_attacks_loop(sq: Square, occupied: Bitboard) -> Bitboard {
    sliding_attack(&ROOK_DIRS, sq, occupied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that magic lookups match the loop-based reference for every
    /// square and every possible occupancy pattern.
    #[test]
    fn test_magic_vs_loop_bishop() {
        super::init();
        for sq_idx in 0..64 {
            let sq = Square::from_u8(sq_idx as u8);
            let mask = BISHOP_MASKS[sq_idx];
            let size = 1 << BISHOP_INDEX_BITS[sq_idx];
            let mut count = 0;
            let mut subset = 0u64;
            loop {
                let occ = Bitboard(subset);
                let magic_atk = bishop_attacks(sq, occ);
                let loop_atk = bishop_attacks_loop(sq, occ);
                assert_eq!(
                    magic_atk, loop_atk,
                    "Bishop mismatch at sq={:?}, occ=0x{:x}",
                    sq, subset
                );
                count += 1;
                subset = subset.wrapping_sub(mask.0) & mask.0;
                if subset == 0 {
                    break;
                }
            }
            assert_eq!(count, size, "Bishop count mismatch at sq={:?}", sq);
        }
    }

    #[test]
    fn test_magic_vs_loop_rook() {
        super::init();
        for sq_idx in 0..64 {
            let sq = Square::from_u8(sq_idx as u8);
            let mask = ROOK_MASKS[sq_idx];
            let size = 1 << ROOK_INDEX_BITS[sq_idx];
            let mut count = 0;
            let mut subset = 0u64;
            loop {
                let occ = Bitboard(subset);
                let magic_atk = rook_attacks(sq, occ);
                let loop_atk = rook_attacks_loop(sq, occ);
                assert_eq!(
                    magic_atk, loop_atk,
                    "Rook mismatch at sq={:?}, occ=0x{:x}",
                    sq, subset
                );
                count += 1;
                subset = subset.wrapping_sub(mask.0) & mask.0;
                if subset == 0 {
                    break;
                }
            }
            assert_eq!(count, size, "Rook count mismatch at sq={:?}", sq);
        }
    }
}
