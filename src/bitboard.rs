use crate::types::*;

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

pub const DARK_SQUARES: Bitboard = Bitboard(0xAA55AA55AA55AA55);

pub const ALL_SQUARES: Bitboard = Bitboard(!0u64);

pub fn square_bb(sq: Square) -> Bitboard {
    Bitboard::square_bb(sq)
}

pub fn file_bb(f: File) -> Bitboard {
    FILE_ABB << (f as usize)
}

pub fn rank_bb(r: Rank) -> Bitboard {
    RANK_1BB << (FILE_NB * (r as usize))
}

pub fn adjacent_files_bb(f: File) -> Bitboard {
    let f_idx = f as usize;
    let mut b = Bitboard::EMPTY;
    if f_idx > 0 {
        b = b | (FILE_ABB << (f_idx - 1));
    }
    if f_idx < 7 {
        b = b | (FILE_ABB << (f_idx + 1));
    }
    b
}

pub fn shift_north(b: Bitboard) -> Bitboard {
    b << 8
}

pub fn shift_south(b: Bitboard) -> Bitboard {
    b >> 8
}

pub fn shift_east(b: Bitboard) -> Bitboard {
    (b & !FILE_HBB) << 1
}

pub fn shift_west(b: Bitboard) -> Bitboard {
    (b & !FILE_ABB) >> 1
}

pub fn shift_ne(b: Bitboard) -> Bitboard {
    (b & !FILE_HBB) << 9
}

pub fn shift_nw(b: Bitboard) -> Bitboard {
    (b & !FILE_ABB) << 7
}

pub fn shift_se(b: Bitboard) -> Bitboard {
    (b & !FILE_HBB) >> 7
}

pub fn shift_sw(b: Bitboard) -> Bitboard {
    (b & !FILE_ABB) >> 9
}

pub fn pawn_attacks_bb(c: Color, pawns: Bitboard) -> Bitboard {
    match c {
        Color::White => shift_nw(pawns) | shift_ne(pawns),
        Color::Black => shift_sw(pawns) | shift_se(pawns),
    }
}

pub fn pawn_attacks_from(c: Color, sq: Square) -> Bitboard {
    pawn_attacks_bb(c, square_bb(sq))
}

pub fn between_bb(s1: Square, s2: Square) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    let f1 = s1 as i8 % 8;
    let r1 = s1 as i8 / 8;
    let f2 = s2 as i8 % 8;
    let r2 = s2 as i8 / 8;
    let df = f2 - f1;
    let dr = r2 - r1;

    if df == 0 && dr == 0 {
        return b;
    }

    if df != 0 && dr != 0 && df.abs() != dr.abs() {
        return Bitboard::EMPTY;
    }

    let f_step = df.signum();
    let r_step = dr.signum();
    let mut f = f1 + f_step;
    let mut r = r1 + r_step;
    while f != f2 || r != r2 {
        let sq = make_square(
            match f {
                0 => File::A,
                1 => File::B,
                2 => File::C,
                3 => File::D,
                4 => File::E,
                5 => File::F,
                6 => File::G,
                7 => File::H,
                _ => unreachable!(),
            },
            match r {
                0 => Rank::R1,
                1 => Rank::R2,
                2 => Rank::R3,
                3 => Rank::R4,
                4 => Rank::R5,
                5 => Rank::R6,
                6 => Rank::R7,
                7 => Rank::R8,
                _ => unreachable!(),
            },
        );
        b = b | square_bb(sq);
        f += f_step;
        r += r_step;
    }
    b
}

pub fn line_bb(s1: Square, s2: Square) -> Bitboard {
    let f1 = s1 as i8 % 8;
    let r1 = s1 as i8 / 8;
    let f2 = s2 as i8 % 8;
    let r2 = s2 as i8 / 8;
    let df = f2 - f1;
    let dr = r2 - r1;

    if df != 0 && dr != 0 && df.abs() != dr.abs() {
        return Bitboard::EMPTY;
    }
    if df == 0 && dr == 0 {
        return Bitboard::EMPTY;
    }

    let f_step = if df == 0 { 0 } else { df.signum() };
    let r_step = if dr == 0 { 0 } else { dr.signum() };

    let mut b = Bitboard::EMPTY;
    let mut f = f1;
    let mut r = r1;
    while (0..8).contains(&f) && (0..8).contains(&r) {
        let sq = make_square(
            match f {
                0 => File::A,
                1 => File::B,
                2 => File::C,
                3 => File::D,
                4 => File::E,
                5 => File::F,
                6 => File::G,
                7 => File::H,
                _ => unreachable!(),
            },
            match r {
                0 => Rank::R1,
                1 => Rank::R2,
                2 => Rank::R3,
                3 => Rank::R4,
                4 => Rank::R5,
                5 => Rank::R6,
                6 => Rank::R7,
                7 => Rank::R8,
                _ => unreachable!(),
            },
        );
        b = b | square_bb(sq);
        f += f_step;
        r += r_step;
    }
    b
}

pub fn aligned(s1: Square, s2: Square, s3: Square) -> bool {
    line_bb(s1, s2) & Bitboard::square_bb(s3) != Bitboard::EMPTY
}

pub fn popcount(bb: Bitboard) -> u32 {
    bb.count()
}

pub fn lsb(bb: Bitboard) -> Square {
    bb.lsb()
}

pub fn more_than_one(bb: Bitboard) -> bool {
    bb.more_than_one()
}

pub fn pop_lsb(bb: &mut Bitboard) -> Square {
    bb.pop_lsb()
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
    fn test_shifts() {
        let b = square_bb(Square::A2);
        assert_eq!(shift_north(b), square_bb(Square::A3));
        assert_eq!(shift_south(b), square_bb(Square::A1));

        let b = square_bb(Square::B2);
        assert_eq!(shift_east(b), square_bb(Square::C2));
        assert_eq!(shift_west(b), square_bb(Square::A2));
    }

    #[test]
    fn test_between_bb() {
        let between = between_bb(Square::C1, Square::F4);
        assert!(between & square_bb(Square::D2) != Bitboard::EMPTY);
        assert!(between & square_bb(Square::E3) != Bitboard::EMPTY);
    }

    #[test]
    fn test_line_bb() {
        let line = line_bb(Square::A1, Square::H8);
        assert!(line & square_bb(Square::B2) != Bitboard::EMPTY);
        assert!(line & square_bb(Square::C3) != Bitboard::EMPTY);
    }

    #[test]
    fn test_aligned() {
        assert!(aligned(Square::A1, Square::C3, Square::E5));
        assert!(aligned(Square::A1, Square::C3, Square::E5));
    }
}
