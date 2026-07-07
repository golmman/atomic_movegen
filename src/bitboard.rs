#![allow(dead_code)]

use crate::attacks::{BETWEEN_BB, LINE_BB};
use crate::types::*;

/// Bitboard with all squares on file A set.
pub const FILE_ABB: Bitboard = Bitboard(0x0101010101010101);
pub const FILE_BBB: Bitboard = Bitboard(FILE_ABB.0 << 1);
pub const FILE_CBB: Bitboard = Bitboard(FILE_ABB.0 << 2);
pub const FILE_DBB: Bitboard = Bitboard(FILE_ABB.0 << 3);
pub const FILE_EBB: Bitboard = Bitboard(FILE_ABB.0 << 4);
pub const FILE_FBB: Bitboard = Bitboard(FILE_ABB.0 << 5);
pub const FILE_GBB: Bitboard = Bitboard(FILE_ABB.0 << 6);
pub const FILE_HBB: Bitboard = Bitboard(FILE_ABB.0 << 7);

pub const RANK_1BB: Bitboard = Bitboard(0xFF);
pub const RANK_2BB: Bitboard = Bitboard(RANK_1BB.0 << FILE_NB);
pub const RANK_3BB: Bitboard = Bitboard(RANK_1BB.0 << (FILE_NB * 2));
pub const RANK_4BB: Bitboard = Bitboard(RANK_1BB.0 << (FILE_NB * 3));
pub const RANK_5BB: Bitboard = Bitboard(RANK_1BB.0 << (FILE_NB * 4));
pub const RANK_6BB: Bitboard = Bitboard(RANK_1BB.0 << (FILE_NB * 5));
pub const RANK_7BB: Bitboard = Bitboard(RANK_1BB.0 << (FILE_NB * 6));
pub const RANK_8BB: Bitboard = Bitboard(RANK_1BB.0 << (FILE_NB * 7));

/// Bitboard with all 64 squares set.
pub const ALL_SQUARES: Bitboard = Bitboard(!0u64);

/// Return the bitboard of squares strictly between `s1` and `s2`.
///
/// Returns [`Bitboard::EMPTY`] if the squares are not on the same rank,
/// file, or diagonal.
#[inline(always)]
pub fn between_bb(s1: Square, s2: Square) -> Bitboard {
    BETWEEN_BB[s1 as usize][s2 as usize]
}

/// Return the bitboard of all squares on the same rank, file, or diagonal
/// as `s1` and `s2` (including both endpoints).
///
/// Returns [`Bitboard::EMPTY`] if the squares are not aligned.
#[inline(always)]
pub fn line_bb(s1: Square, s2: Square) -> Bitboard {
    LINE_BB[s1 as usize][s2 as usize]
}

#[cfg(test)]
pub fn aligned(s1: Square, s2: Square, s3: Square) -> bool {
    line_bb(s1, s2) & Bitboard::square_bb(s3) != Bitboard::EMPTY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_rank_constants() {
        assert_eq!(FILE_ABB.0, 0x0101010101010101);
        assert_eq!(FILE_HBB.0, 0x8080808080808080);
        assert_eq!(RANK_1BB.0, 0xFF);
        assert_eq!(RANK_8BB.0, 0xFF00000000000000);
    }

    #[test]
    fn test_between_bb() {
        let between = between_bb(Square::C1, Square::F4);
        assert!(between & Bitboard::square_bb(Square::D2) != Bitboard::EMPTY);
        assert!(between & Bitboard::square_bb(Square::E3) != Bitboard::EMPTY);
        // Non-aligned squares return empty
        assert!((between_bb(Square::A1, Square::B3)).is_empty());
        // Same-square returns empty
        assert!((between_bb(Square::D4, Square::D4)).is_empty());
    }

    #[test]
    fn test_line_bb() {
        let line = line_bb(Square::A1, Square::H8);
        assert!(line & Bitboard::square_bb(Square::B2) != Bitboard::EMPTY);
        assert!(line & Bitboard::square_bb(Square::C3) != Bitboard::EMPTY);
        // Line includes both endpoints
        assert!(line & Bitboard::square_bb(Square::A1) != Bitboard::EMPTY);
        assert!(line & Bitboard::square_bb(Square::H8) != Bitboard::EMPTY);
    }

    #[test]
    fn test_aligned() {
        assert!(aligned(Square::A1, Square::C3, Square::E5));
        assert!(aligned(Square::A1, Square::C3, Square::E5));
    }
}
