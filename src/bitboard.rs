use crate::attacks::BETWEEN_BB;
use crate::types::*;

/// Return the bitboard of squares strictly between `s1` and `s2`.
///
/// Returns [`Bitboard::EMPTY`] if the squares are not on the same rank,
/// file, or diagonal.
#[inline(always)]
pub fn between_bb(s1: Square, s2: Square) -> Bitboard {
    BETWEEN_BB[s1 as usize][s2 as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attacks::LINE_BB;

    fn line_bb(s1: Square, s2: Square) -> Bitboard {
        LINE_BB[s1 as usize][s2 as usize]
    }

    fn aligned(s1: Square, s2: Square, s3: Square) -> bool {
        line_bb(s1, s2) & Bitboard::square_bb(s3) != Bitboard::EMPTY
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
