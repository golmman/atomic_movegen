use crate::bitboard::*;
use crate::types::*;
use std::sync::LazyLock;

const SQUARE_NB: usize = 64;
const ROOK_DIRS: [(i8, i8); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
const BISHOP_DIRS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

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
            if occupied.0 & (1u64 << idx) != 0 {
                break;
            }
            f += df;
            r += dr;
        }
    }
    Bitboard(result)
}

static KING_ATTACKS: LazyLock<Vec<Bitboard>> = LazyLock::new(|| {
    let mut attacks = vec![Bitboard::EMPTY; SQUARE_NB];
    for sq_idx in 0..SQUARE_NB {
        let sq = Square::from_u8(sq_idx as u8);
        let b = Bitboard::square_bb(sq);
        let mut atk = Bitboard::EMPTY;
        if sq as i8 % 8 > 0 {
            atk = atk | shift_west(b) | shift_sw(b) | shift_nw(b);
        }
        if sq as i8 % 8 < 7 {
            atk = atk | shift_east(b) | shift_se(b) | shift_ne(b);
        }
        atk = atk | shift_north(b) | shift_south(b);
        attacks[sq_idx] = atk;
    }
    attacks
});

static KNIGHT_ATTACKS: LazyLock<Vec<Bitboard>> = LazyLock::new(|| {
    let mut attacks = vec![Bitboard::EMPTY; SQUARE_NB];
    let knight_offsets: &[i8] = &[6, 10, 15, 17, -6, -10, -15, -17];
    for sq_idx in 0..SQUARE_NB {
        let sq = sq_idx as i8;
        let f = sq % 8;
        let r = sq / 8;
        let mut atk = 0u64;
        for &off in knight_offsets {
            let to = sq + off;
            if to < 0 || to >= 64 {
                continue;
            }
            let tf = to % 8;
            let tr = to / 8;
            let df = (tf - f).abs();
            let dr = (tr - r).abs();
            if (df == 1 && dr == 2) || (df == 2 && dr == 1) {
                atk |= 1u64 << to;
            }
        }
        attacks[sq_idx] = Bitboard(atk);
    }
    attacks
});

static PAWN_ATTACKS: LazyLock<Vec<[Bitboard; 2]>> = LazyLock::new(|| {
    let mut attacks = vec![[Bitboard::EMPTY, Bitboard::EMPTY]; SQUARE_NB];
    for sq_idx in 0..SQUARE_NB {
        let sq = Square::from_u8(sq_idx as u8);
        let b = Bitboard::square_bb(sq);
        attacks[sq_idx][Color::White as usize] = shift_nw(b) | shift_ne(b);
        attacks[sq_idx][Color::Black as usize] = shift_sw(b) | shift_se(b);
    }
    attacks
});

pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    sliding_attack(&BISHOP_DIRS, sq, occupied)
}

pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    sliding_attack(&ROOK_DIRS, sq, occupied)
}

pub fn queen_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)
}

pub fn king_attacks(sq: Square) -> Bitboard {
    KING_ATTACKS[sq as usize]
}

pub fn knight_attacks(sq: Square) -> Bitboard {
    KNIGHT_ATTACKS[sq as usize]
}

pub fn pawn_attacks(c: Color, sq: Square) -> Bitboard {
    PAWN_ATTACKS[sq as usize][c as usize]
}

pub fn attacks_bb(pt: PieceType, sq: Square, occupied: Bitboard) -> Bitboard {
    match pt {
        PieceType::Pawn => Bitboard::EMPTY,
        PieceType::Knight => knight_attacks(sq),
        PieceType::Bishop => bishop_attacks(sq, occupied),
        PieceType::Rook => rook_attacks(sq, occupied),
        PieceType::Queen => queen_attacks(sq, occupied),
        PieceType::Commoner => king_attacks(sq),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knight_attacks_center() {
        let atk = knight_attacks(Square::D4);
        assert!(atk & square_bb(Square::C2) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::E2) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::B3) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::F3) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::B5) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::F5) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::C6) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::E6) != Bitboard::EMPTY);
        assert_eq!(atk.count(), 8);
    }

    #[test]
    fn test_knight_attacks_corner() {
        let atk = knight_attacks(Square::A1);
        assert_eq!(atk.count(), 2);
        assert!(atk & square_bb(Square::B3) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::C2) != Bitboard::EMPTY);
    }

    #[test]
    fn test_king_attacks_center() {
        let atk = king_attacks(Square::D4);
        assert_eq!(atk.count(), 8);
    }

    #[test]
    fn test_king_attacks_corner() {
        let atk = king_attacks(Square::A1);
        assert_eq!(atk.count(), 3);
    }

    #[test]
    fn test_bishop_attacks() {
        let atk = bishop_attacks(Square::D4, Bitboard::EMPTY);
        assert!(atk & square_bb(Square::A1) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::G7) != Bitboard::EMPTY);
        assert!((atk & square_bb(Square::D4)).is_empty());
    }

    #[test]
    fn test_rook_attacks() {
        let atk = rook_attacks(Square::D4, Bitboard::EMPTY);
        assert!(atk & square_bb(Square::D1) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::D8) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::A4) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::H4) != Bitboard::EMPTY);
    }

    #[test]
    fn test_bishop_blocked() {
        let blocker = square_bb(Square::E5);
        let atk = bishop_attacks(Square::D4, blocker);
        assert!(
            atk & square_bb(Square::E5) != Bitboard::EMPTY,
            "blocker square should be attackable"
        );
        assert!(
            (atk & square_bb(Square::F6)).is_empty(),
            "beyond blocker should be blocked"
        );
    }

    #[test]
    fn test_rook_blocked() {
        let blocker = square_bb(Square::D5);
        let atk = rook_attacks(Square::D4, blocker);
        assert!(atk & square_bb(Square::D5) != Bitboard::EMPTY);
        assert!((atk & square_bb(Square::D6)).is_empty());
    }

    #[test]
    fn test_queen_equals_bishop_plus_rook() {
        let occ = square_bb(Square::E5);
        let queen = queen_attacks(Square::D4, occ);
        let bishop = bishop_attacks(Square::D4, occ);
        let rook = rook_attacks(Square::D4, occ);
        assert_eq!(queen, bishop | rook);
    }
}
