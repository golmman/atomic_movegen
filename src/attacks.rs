use crate::types::*;

// ---------------------------------------------------------------------------
// Sliding-attack dispatch
//
// On x86_64: runtime dispatch between PEXT (BMI2) and magic-multiply.
// On other architectures (ARM etc.): direct re-export of the magic fallback
// — zero overhead, since BMI2 is never available.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
mod sliding_dispatch {
    use crate::bitboard::square_bb;
    use crate::types::*;
    use core::sync::atomic::{AtomicU8, Ordering};

    // 0 = uninit, 1 = Magic, 2 = Pext
    static IMPL: AtomicU8 = AtomicU8::new(0);

    pub(crate) fn force_magic() {
        IMPL.store(1, Ordering::Relaxed);
    }

    pub(crate) fn force_pext() {
        IMPL.store(2, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
        // After init(), IMPL is stable and threadsafe to read.
        if IMPL.load(Ordering::Relaxed) == 2 {
            unsafe { crate::pext::bishop_attacks_pext(sq, occupied) }
        } else {
            crate::magic::bishop_attacks(sq, occupied)
        }
    }

    #[inline(always)]
    pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
        if IMPL.load(Ordering::Relaxed) == 2 {
            unsafe { crate::pext::rook_attacks_pext(sq, occupied) }
        } else {
            crate::magic::rook_attacks(sq, occupied)
        }
    }

    #[inline(always)]
    pub fn queen_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
        bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)
    }
}

#[cfg(target_arch = "x86_64")]
use sliding_dispatch::{bishop_attacks, queen_attacks, rook_attacks};

#[cfg(not(target_arch = "x86_64"))]
pub use crate::magic::{bishop_attacks, queen_attacks, rook_attacks};

// ---------------------------------------------------------------------------
// Leaper attack tables computed at compile time (no lazy init, no overhead)
// ---------------------------------------------------------------------------

/// Compute king attacks for all 64 squares at compile time.
const fn compute_king_attacks() -> [Bitboard; 64] {
    let mut attacks = [Bitboard(0); 64];
    let mut sq: u8 = 0;
    while sq < 64 {
        let f = sq % 8;
        let r = sq / 8;
        let mut atk = 0u64;

        // North
        if r < 7 {
            atk |= 1u64 << ((r + 1) * 8 + f);
        }
        // South
        if r > 0 {
            atk |= 1u64 << ((r - 1) * 8 + f);
        }
        // East
        if f < 7 {
            atk |= 1u64 << (r * 8 + f + 1);
        }
        // West
        if f > 0 {
            atk |= 1u64 << (r * 8 + f - 1);
        }
        // North-East
        if r < 7 && f < 7 {
            atk |= 1u64 << ((r + 1) * 8 + f + 1);
        }
        // North-West
        if r < 7 && f > 0 {
            atk |= 1u64 << ((r + 1) * 8 + f - 1);
        }
        // South-East
        if r > 0 && f < 7 {
            atk |= 1u64 << ((r - 1) * 8 + f + 1);
        }
        // South-West
        if r > 0 && f > 0 {
            atk |= 1u64 << ((r - 1) * 8 + f - 1);
        }

        attacks[sq as usize] = Bitboard(atk);
        sq += 1;
    }
    attacks
}

/// Compute knight attacks for all 64 squares at compile time.
const fn compute_knight_attacks() -> [Bitboard; 64] {
    let mut attacks = [Bitboard(0); 64];
    let mut sq: u8 = 0;
    while sq < 64 {
        let f = sq as i8 % 8;
        let r = sq as i8 / 8;
        let mut atk = 0u64;
        // Knight offsets: (df,dr) pairs where |df|+|dr|=3 and min(|df|,|dr|)=1
        let offsets: [i8; 8] = [17, 15, 10, 6, -6, -10, -15, -17];
        let mut i: u8 = 0;
        while i < 8 {
            let to = sq as i8 + offsets[i as usize];
            if to >= 0 && to < 64 {
                let tf = to % 8;
                let tr = to / 8;
                let df = tf - f;
                let dr = tr - r;
                // Valid knight move: (|df|, |dr|) is (1,2) or (2,1).
                // Squared-distance == 5 avoids abs() dependency.
                if (df == 1 || df == -1) && (dr == 2 || dr == -2)
                    || (df == 2 || df == -2) && (dr == 1 || dr == -1)
                {
                    atk |= 1u64 << to;
                }
            }
            i += 1;
        }
        attacks[sq as usize] = Bitboard(atk);
        sq += 1;
    }
    attacks
}

/// Compute pawn attacks for all 64 squares at compile time.
/// Returns a 2D array indexed by [square][color].
const fn compute_pawn_attacks() -> [[Bitboard; 2]; 64] {
    let mut attacks = [[Bitboard(0); 2]; 64];
    let mut sq: u8 = 0;
    while sq < 64 {
        let f = sq % 8;
        let r = sq / 8;
        let mut white_atk = 0u64;
        let mut black_atk = 0u64;

        // White pawns attack north-west and north-east
        if r < 7 {
            if f > 0 {
                white_atk |= 1u64 << ((r + 1) * 8 + f - 1);
            }
            if f < 7 {
                white_atk |= 1u64 << ((r + 1) * 8 + f + 1);
            }
        }
        // Black pawns attack south-west and south-east
        if r > 0 {
            if f > 0 {
                black_atk |= 1u64 << ((r - 1) * 8 + f - 1);
            }
            if f < 7 {
                black_atk |= 1u64 << ((r - 1) * 8 + f + 1);
            }
        }

        attacks[sq as usize] = [Bitboard(white_atk), Bitboard(black_atk)];
        sq += 1;
    }
    attacks
}

/// Precomputed king attacks for all 64 squares (compile-time constant).
const KING_ATTACKS: [Bitboard; 64] = compute_king_attacks();

/// Precomputed knight attacks for all 64 squares (compile-time constant).
const KNIGHT_ATTACKS: [Bitboard; 64] = compute_knight_attacks();

/// Precomputed pawn attacks for all 64 squares (compile-time constant).
const PAWN_ATTACKS: [[Bitboard; 2]; 64] = compute_pawn_attacks();

/// Return the attack bitboard for a king on the given square.
#[inline(always)]
pub fn king_attacks(sq: Square) -> Bitboard {
    KING_ATTACKS[sq as usize]
}

/// Return the attack bitboard for a knight on the given square.
#[inline(always)]
pub fn knight_attacks(sq: Square) -> Bitboard {
    KNIGHT_ATTACKS[sq as usize]
}

/// Return the attack bitboard for a pawn of the given color on the given square.
#[inline(always)]
pub fn pawn_attacks(c: Color, sq: Square) -> Bitboard {
    PAWN_ATTACKS[sq as usize][c as usize]
}

// ---------------------------------------------------------------------------
// Initialization — must be called before any sliding-attack lookups
// ---------------------------------------------------------------------------

/// Initialize all attack tables (magic and PEXT).
///
/// Must be called before any call to `bishop_attacks`, `rook_attacks`, or
/// `queen_attacks`. Safe to call multiple times — subsequent calls are no-ops.
pub fn init() {
    crate::magic::init();
    #[cfg(target_arch = "x86_64")]
    {
        if crate::pext::has_bmi2() {
            crate::pext::init();
            sliding_dispatch::force_pext();
        } else {
            sliding_dispatch::force_magic();
        }
    }
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
    use crate::bitboard::square_bb;

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
        crate::attacks::init();
        let atk = bishop_attacks(Square::D4, Bitboard::EMPTY);
        assert!(atk & square_bb(Square::A1) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::G7) != Bitboard::EMPTY);
        assert!((atk & square_bb(Square::D4)).is_empty());
    }

    #[test]
    fn test_rook_attacks() {
        crate::attacks::init();
        let atk = rook_attacks(Square::D4, Bitboard::EMPTY);
        assert!(atk & square_bb(Square::D1) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::D8) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::A4) != Bitboard::EMPTY);
        assert!(atk & square_bb(Square::H4) != Bitboard::EMPTY);
    }

    #[test]
    fn test_bishop_blocked() {
        crate::attacks::init();
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
        crate::attacks::init();
        let blocker = square_bb(Square::D5);
        let atk = rook_attacks(Square::D4, blocker);
        assert!(atk & square_bb(Square::D5) != Bitboard::EMPTY);
        assert!((atk & square_bb(Square::D6)).is_empty());
    }

    #[test]
    fn test_queen_equals_bishop_plus_rook() {
        crate::attacks::init();
        let occ = square_bb(Square::E5);
        let queen = queen_attacks(Square::D4, occ);
        let bishop = bishop_attacks(Square::D4, occ);
        let rook = rook_attacks(Square::D4, occ);
        assert_eq!(queen, bishop | rook);
    }
}
