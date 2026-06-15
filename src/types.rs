use std::fmt;
use std::ops;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Square {
    A1,
    B1,
    C1,
    D1,
    E1,
    F1,
    G1,
    H1,
    A2,
    B2,
    C2,
    D2,
    E2,
    F2,
    G2,
    H2,
    A3,
    B3,
    C3,
    D3,
    E3,
    F3,
    G3,
    H3,
    A4,
    B4,
    C4,
    D4,
    E4,
    F4,
    G4,
    H4,
    A5,
    B5,
    C5,
    D5,
    E5,
    F5,
    G5,
    H5,
    A6,
    B6,
    C6,
    D6,
    E6,
    F6,
    G6,
    H6,
    A7,
    B7,
    C7,
    D7,
    E7,
    F7,
    G7,
    H7,
    A8,
    B8,
    C8,
    D8,
    E8,
    F8,
    G8,
    H8,
    NONE,
}

pub const SQ_A1: Square = Square::A1;
pub const SQ_B1: Square = Square::B1;
pub const SQ_C1: Square = Square::C1;
pub const SQ_D1: Square = Square::D1;
pub const SQ_E1: Square = Square::E1;
pub const SQ_F1: Square = Square::F1;
pub const SQ_G1: Square = Square::G1;
pub const SQ_H1: Square = Square::H1;
pub const SQ_A2: Square = Square::A2;
pub const SQ_B2: Square = Square::B2;
pub const SQ_C2: Square = Square::C2;
pub const SQ_D2: Square = Square::D2;
pub const SQ_E2: Square = Square::E2;
pub const SQ_F2: Square = Square::F2;
pub const SQ_G2: Square = Square::G2;
pub const SQ_H2: Square = Square::H2;
pub const SQ_A3: Square = Square::A3;
pub const SQ_B3: Square = Square::B3;
pub const SQ_C3: Square = Square::C3;
pub const SQ_D3: Square = Square::D3;
pub const SQ_E3: Square = Square::E3;
pub const SQ_F3: Square = Square::F3;
pub const SQ_G3: Square = Square::G3;
pub const SQ_H3: Square = Square::H3;
pub const SQ_A4: Square = Square::A4;
pub const SQ_B4: Square = Square::B4;
pub const SQ_C4: Square = Square::C4;
pub const SQ_D4: Square = Square::D4;
pub const SQ_E4: Square = Square::E4;
pub const SQ_F4: Square = Square::F4;
pub const SQ_G4: Square = Square::G4;
pub const SQ_H4: Square = Square::H4;
pub const SQ_A5: Square = Square::A5;
pub const SQ_B5: Square = Square::B5;
pub const SQ_C5: Square = Square::C5;
pub const SQ_D5: Square = Square::D5;
pub const SQ_E5: Square = Square::E5;
pub const SQ_F5: Square = Square::F5;
pub const SQ_G5: Square = Square::G5;
pub const SQ_H5: Square = Square::H5;
pub const SQ_A6: Square = Square::A6;
pub const SQ_B6: Square = Square::B6;
pub const SQ_C6: Square = Square::C6;
pub const SQ_D6: Square = Square::D6;
pub const SQ_E6: Square = Square::E6;
pub const SQ_F6: Square = Square::F6;
pub const SQ_G6: Square = Square::G6;
pub const SQ_H6: Square = Square::H6;
pub const SQ_A7: Square = Square::A7;
pub const SQ_B7: Square = Square::B7;
pub const SQ_C7: Square = Square::C7;
pub const SQ_D7: Square = Square::D7;
pub const SQ_E7: Square = Square::E7;
pub const SQ_F7: Square = Square::F7;
pub const SQ_G7: Square = Square::G7;
pub const SQ_H7: Square = Square::H7;
pub const SQ_A8: Square = Square::A8;
pub const SQ_B8: Square = Square::B8;
pub const SQ_C8: Square = Square::C8;
pub const SQ_D8: Square = Square::D8;
pub const SQ_E8: Square = Square::E8;
pub const SQ_F8: Square = Square::F8;
pub const SQ_G8: Square = Square::G8;
pub const SQ_H8: Square = Square::H8;
pub const SQ_NONE: Square = Square::NONE;

pub const SQUARE_NB: usize = 64;
pub const FILE_NB: usize = 8;
pub const RANK_NB: usize = 8;

pub fn is_ok(s: Square) -> bool {
    (s as usize) < SQUARE_NB
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum File {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
}

impl File {
    pub const NB: usize = 8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
}

impl Rank {
    pub const NB: usize = 8;
}

pub fn file_of(s: Square) -> File {
    let idx = s as usize;
    let files = [
        File::A,
        File::B,
        File::C,
        File::D,
        File::E,
        File::F,
        File::G,
        File::H,
    ];
    files[idx % 8]
}

pub fn rank_of(s: Square) -> Rank {
    let idx = s as usize;
    let ranks = [
        Rank::R1,
        Rank::R2,
        Rank::R3,
        Rank::R4,
        Rank::R5,
        Rank::R6,
        Rank::R7,
        Rank::R8,
    ];
    ranks[idx / 8]
}

pub fn make_square(f: File, r: Rank) -> Square {
    let idx = (r as usize) * 8 + (f as usize);
    static SQUARES: [Square; 64] = [
        Square::A1,
        Square::B1,
        Square::C1,
        Square::D1,
        Square::E1,
        Square::F1,
        Square::G1,
        Square::H1,
        Square::A2,
        Square::B2,
        Square::C2,
        Square::D2,
        Square::E2,
        Square::F2,
        Square::G2,
        Square::H2,
        Square::A3,
        Square::B3,
        Square::C3,
        Square::D3,
        Square::E3,
        Square::F3,
        Square::G3,
        Square::H3,
        Square::A4,
        Square::B4,
        Square::C4,
        Square::D4,
        Square::E4,
        Square::F4,
        Square::G4,
        Square::H4,
        Square::A5,
        Square::B5,
        Square::C5,
        Square::D5,
        Square::E5,
        Square::F5,
        Square::G5,
        Square::H5,
        Square::A6,
        Square::B6,
        Square::C6,
        Square::D6,
        Square::E6,
        Square::F6,
        Square::G6,
        Square::H6,
        Square::A7,
        Square::B7,
        Square::C7,
        Square::D7,
        Square::E7,
        Square::F7,
        Square::G7,
        Square::H7,
        Square::A8,
        Square::B8,
        Square::C8,
        Square::D8,
        Square::E8,
        Square::F8,
        Square::G8,
        Square::H8,
    ];
    SQUARES[idx]
}

pub fn relative_rank(c: Color, r: Rank) -> Rank {
    match c {
        Color::White => r,
        Color::Black => {
            let ranks = [
                Rank::R8,
                Rank::R7,
                Rank::R6,
                Rank::R5,
                Rank::R4,
                Rank::R3,
                Rank::R2,
                Rank::R1,
            ];
            ranks[r as usize]
        }
    }
}

pub fn relative_rank_sq(c: Color, s: Square) -> Rank {
    relative_rank(c, rank_of(s))
}

pub fn pawn_push(c: Color) -> Direction {
    match c {
        Color::White => Direction::North,
        Color::Black => Direction::South,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bitboard(pub u64);

impl Bitboard {
    pub const EMPTY: Bitboard = Bitboard(0);
    pub const ALL: Bitboard = Bitboard(!0u64);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    pub fn lsb(self) -> Square {
        debug_assert!(!self.is_empty());
        let idx = self.0.trailing_zeros() as usize;
        static SQUARES: [Square; 64] = [
            Square::A1,
            Square::B1,
            Square::C1,
            Square::D1,
            Square::E1,
            Square::F1,
            Square::G1,
            Square::H1,
            Square::A2,
            Square::B2,
            Square::C2,
            Square::D2,
            Square::E2,
            Square::F2,
            Square::G2,
            Square::H2,
            Square::A3,
            Square::B3,
            Square::C3,
            Square::D3,
            Square::E3,
            Square::F3,
            Square::G3,
            Square::H3,
            Square::A4,
            Square::B4,
            Square::C4,
            Square::D4,
            Square::E4,
            Square::F4,
            Square::G4,
            Square::H4,
            Square::A5,
            Square::B5,
            Square::C5,
            Square::D5,
            Square::E5,
            Square::F5,
            Square::G5,
            Square::H5,
            Square::A6,
            Square::B6,
            Square::C6,
            Square::D6,
            Square::E6,
            Square::F6,
            Square::G6,
            Square::H6,
            Square::A7,
            Square::B7,
            Square::C7,
            Square::D7,
            Square::E7,
            Square::F7,
            Square::G7,
            Square::H7,
            Square::A8,
            Square::B8,
            Square::C8,
            Square::D8,
            Square::E8,
            Square::F8,
            Square::G8,
            Square::H8,
        ];
        SQUARES[idx]
    }

    pub fn msb(self) -> Square {
        debug_assert!(!self.is_empty());
        let idx = 63 - self.0.leading_zeros() as usize;
        static SQUARES: [Square; 64] = [
            Square::A1,
            Square::B1,
            Square::C1,
            Square::D1,
            Square::E1,
            Square::F1,
            Square::G1,
            Square::H1,
            Square::A2,
            Square::B2,
            Square::C2,
            Square::D2,
            Square::E2,
            Square::F2,
            Square::G2,
            Square::H2,
            Square::A3,
            Square::B3,
            Square::C3,
            Square::D3,
            Square::E3,
            Square::F3,
            Square::G3,
            Square::H3,
            Square::A4,
            Square::B4,
            Square::C4,
            Square::D4,
            Square::E4,
            Square::F4,
            Square::G4,
            Square::H4,
            Square::A5,
            Square::B5,
            Square::C5,
            Square::D5,
            Square::E5,
            Square::F5,
            Square::G5,
            Square::H5,
            Square::A6,
            Square::B6,
            Square::C6,
            Square::D6,
            Square::E6,
            Square::F6,
            Square::G6,
            Square::H6,
            Square::A7,
            Square::B7,
            Square::C7,
            Square::D7,
            Square::E7,
            Square::F7,
            Square::G7,
            Square::H7,
            Square::A8,
            Square::B8,
            Square::C8,
            Square::D8,
            Square::E8,
            Square::F8,
            Square::G8,
            Square::H8,
        ];
        SQUARES[idx]
    }

    pub fn pop_lsb(&mut self) -> Square {
        let sq = self.lsb();
        self.0 &= self.0 - 1;
        sq
    }

    pub fn more_than_one(self) -> bool {
        self.0 & (self.0 - 1) != 0
    }

    pub fn square_bb(sq: Square) -> Bitboard {
        if sq == Square::NONE {
            return Bitboard::EMPTY;
        }
        Bitboard(1u64 << (sq as u8))
    }
}

impl ops::BitAnd for Bitboard {
    type Output = Bitboard;
    fn bitand(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 & rhs.0)
    }
}

impl ops::BitOr for Bitboard {
    type Output = Bitboard;
    fn bitor(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 | rhs.0)
    }
}

impl ops::BitXor for Bitboard {
    type Output = Bitboard;
    fn bitxor(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 ^ rhs.0)
    }
}

impl ops::Not for Bitboard {
    type Output = Bitboard;
    fn not(self) -> Bitboard {
        Bitboard(!self.0)
    }
}

impl ops::Shl<usize> for Bitboard {
    type Output = Bitboard;
    fn shl(self, rhs: usize) -> Bitboard {
        Bitboard(self.0 << rhs)
    }
}

impl ops::Shr<usize> for Bitboard {
    type Output = Bitboard;
    fn shr(self, rhs: usize) -> Bitboard {
        Bitboard(self.0 >> rhs)
    }
}

impl ops::BitAnd<Square> for Bitboard {
    type Output = Bitboard;
    fn bitand(self, rhs: Square) -> Bitboard {
        self & Bitboard::square_bb(rhs)
    }
}

impl ops::BitOr<Square> for Bitboard {
    type Output = Bitboard;
    fn bitor(self, rhs: Square) -> Bitboard {
        self | Bitboard::square_bb(rhs)
    }
}

impl ops::BitXor<Square> for Bitboard {
    type Output = Bitboard;
    fn bitxor(self, rhs: Square) -> Bitboard {
        self ^ Bitboard::square_bb(rhs)
    }
}

impl ops::Sub<Square> for Bitboard {
    type Output = Bitboard;
    fn sub(self, rhs: Square) -> Bitboard {
        self & !Bitboard::square_bb(rhs)
    }
}

impl ops::BitAnd<Square> for Square {
    type Output = Bitboard;
    fn bitand(self, rhs: Square) -> Bitboard {
        Bitboard::square_bb(self) & rhs
    }
}

impl ops::BitOr<Square> for Square {
    type Output = Bitboard;
    fn bitor(self, rhs: Square) -> Bitboard {
        Bitboard::square_bb(self) | rhs
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    pub const NB: usize = 2;

    pub fn flip(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    pub fn to_usize(self) -> usize {
        self as usize
    }
}

impl ops::Not for Color {
    type Output = Color;
    fn not(self) -> Color {
        self.flip()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PieceType {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    Commoner = 5,
}

impl PieceType {
    pub const NB: usize = 6;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece(u8);

impl Piece {
    // Encode as (color << 3) | (pt as u8 + 1) to avoid NO_PIECE = 0 conflict
    pub const fn from_parts(color: Color, pt: PieceType) -> Piece {
        Piece(((color as u8) << 3) | ((pt as u8) + 1))
    }

    pub fn color(self) -> Color {
        if self.0 & 8 == 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    pub fn type_of(self) -> PieceType {
        let inner = (self.0 & 7).wrapping_sub(1);
        static TYPES: [PieceType; 6] = [
            PieceType::Pawn,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
            PieceType::Commoner,
        ];
        if inner < 6 {
            TYPES[inner as usize]
        } else {
            PieceType::Pawn
        }
    }

    pub fn is_ok(self) -> bool {
        self.0 != 0
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return write!(f, "--");
        }
        let c = match self.color() {
            Color::White => "W",
            Color::Black => "B",
        };
        let t = match self.type_of() {
            PieceType::Pawn => "P",
            PieceType::Knight => "N",
            PieceType::Bishop => "B",
            PieceType::Rook => "R",
            PieceType::Queen => "Q",
            PieceType::Commoner => "C",
        };
        write!(f, "{}{}", c, t)
    }
}

pub const NO_PIECE: Piece = Piece(0);
pub const W_PAWN: Piece = Piece::from_parts(Color::White, PieceType::Pawn);
pub const W_KNIGHT: Piece = Piece::from_parts(Color::White, PieceType::Knight);
pub const W_BISHOP: Piece = Piece::from_parts(Color::White, PieceType::Bishop);
pub const W_ROOK: Piece = Piece::from_parts(Color::White, PieceType::Rook);
pub const W_QUEEN: Piece = Piece::from_parts(Color::White, PieceType::Queen);
pub const W_COMMONER: Piece = Piece::from_parts(Color::White, PieceType::Commoner);
pub const B_PAWN: Piece = Piece::from_parts(Color::Black, PieceType::Pawn);
pub const B_KNIGHT: Piece = Piece::from_parts(Color::Black, PieceType::Knight);
pub const B_BISHOP: Piece = Piece::from_parts(Color::Black, PieceType::Bishop);
pub const B_ROOK: Piece = Piece::from_parts(Color::Black, PieceType::Rook);
pub const B_QUEEN: Piece = Piece::from_parts(Color::Black, PieceType::Queen);
pub const B_COMMONER: Piece = Piece::from_parts(Color::Black, PieceType::Commoner);

pub fn make_piece(color: Color, pt: PieceType) -> Piece {
    Piece::from_parts(color, pt)
}

pub fn color_of(p: Piece) -> Color {
    p.color()
}

pub fn type_of(p: Piece) -> PieceType {
    p.type_of()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveType {
    Normal = 0,
    Promotion = 1,
    EnPassant = 2,
    Castling = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move(u16);

impl Move {
    pub const NONE: Move = Move(0);
    pub const NULL: Move = Move(1 + (1 << 6));

    pub fn from_sq(self) -> Square {
        let idx = ((self.0 >> 6) & 0x3f) as usize;
        static SQUARES: [Square; 64] = [
            Square::A1,
            Square::B1,
            Square::C1,
            Square::D1,
            Square::E1,
            Square::F1,
            Square::G1,
            Square::H1,
            Square::A2,
            Square::B2,
            Square::C2,
            Square::D2,
            Square::E2,
            Square::F2,
            Square::G2,
            Square::H2,
            Square::A3,
            Square::B3,
            Square::C3,
            Square::D3,
            Square::E3,
            Square::F3,
            Square::G3,
            Square::H3,
            Square::A4,
            Square::B4,
            Square::C4,
            Square::D4,
            Square::E4,
            Square::F4,
            Square::G4,
            Square::H4,
            Square::A5,
            Square::B5,
            Square::C5,
            Square::D5,
            Square::E5,
            Square::F5,
            Square::G5,
            Square::H5,
            Square::A6,
            Square::B6,
            Square::C6,
            Square::D6,
            Square::E6,
            Square::F6,
            Square::G6,
            Square::H6,
            Square::A7,
            Square::B7,
            Square::C7,
            Square::D7,
            Square::E7,
            Square::F7,
            Square::G7,
            Square::H7,
            Square::A8,
            Square::B8,
            Square::C8,
            Square::D8,
            Square::E8,
            Square::F8,
            Square::G8,
            Square::H8,
        ];
        SQUARES[idx]
    }

    pub fn to_sq(self) -> Square {
        let idx = (self.0 & 0x3f) as usize;
        static SQUARES: [Square; 64] = [
            Square::A1,
            Square::B1,
            Square::C1,
            Square::D1,
            Square::E1,
            Square::F1,
            Square::G1,
            Square::H1,
            Square::A2,
            Square::B2,
            Square::C2,
            Square::D2,
            Square::E2,
            Square::F2,
            Square::G2,
            Square::H2,
            Square::A3,
            Square::B3,
            Square::C3,
            Square::D3,
            Square::E3,
            Square::F3,
            Square::G3,
            Square::H3,
            Square::A4,
            Square::B4,
            Square::C4,
            Square::D4,
            Square::E4,
            Square::F4,
            Square::G4,
            Square::H4,
            Square::A5,
            Square::B5,
            Square::C5,
            Square::D5,
            Square::E5,
            Square::F5,
            Square::G5,
            Square::H5,
            Square::A6,
            Square::B6,
            Square::C6,
            Square::D6,
            Square::E6,
            Square::F6,
            Square::G6,
            Square::H6,
            Square::A7,
            Square::B7,
            Square::C7,
            Square::D7,
            Square::E7,
            Square::F7,
            Square::G7,
            Square::H7,
            Square::A8,
            Square::B8,
            Square::C8,
            Square::D8,
            Square::E8,
            Square::F8,
            Square::G8,
            Square::H8,
        ];
        SQUARES[idx]
    }

    pub fn move_type(self) -> MoveType {
        match (self.0 >> 12) & 3 {
            0 => MoveType::Normal,
            1 => MoveType::Promotion,
            2 => MoveType::EnPassant,
            _ => MoveType::Castling,
        }
    }

    pub fn promotion_type(self) -> PieceType {
        static TYPES: [PieceType; 4] = [
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
        ];
        TYPES[((self.0 >> 14) & 3) as usize]
    }

    pub fn make_move(from: Square, to: Square) -> Move {
        Move(((from as u16) << 6) | (to as u16))
    }

    pub fn make_promotion(from: Square, to: Square, pt: PieceType) -> Move {
        let pt_bits = match pt {
            PieceType::Knight => 0u16,
            PieceType::Bishop => 1u16,
            PieceType::Rook => 2u16,
            PieceType::Queen => 3u16,
            _ => 0u16,
        };
        Move((pt_bits << 14) | (1 << 12) | ((from as u16) << 6) | (to as u16))
    }

    pub fn make_enpassant(from: Square, to: Square) -> Move {
        Move((2 << 12) | ((from as u16) << 6) | (to as u16))
    }

    pub fn make_castling(from: Square, to: Square) -> Move {
        Move((3 << 12) | ((from as u16) << 6) | (to as u16))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    North = 8,
    East = 1,
    South = -8,
    West = -1,
    NorthEast = 9,
    NorthWest = 7,
    SouthEast = -7,
    SouthWest = -9,
}

impl ops::Add<Direction> for Square {
    type Output = Square;
    fn add(self, rhs: Direction) -> Square {
        let idx = (self as i16) + (rhs as i16);
        if idx >= 0 && idx < 64 {
            static SQUARES: [Square; 64] = [
                Square::A1,
                Square::B1,
                Square::C1,
                Square::D1,
                Square::E1,
                Square::F1,
                Square::G1,
                Square::H1,
                Square::A2,
                Square::B2,
                Square::C2,
                Square::D2,
                Square::E2,
                Square::F2,
                Square::G2,
                Square::H2,
                Square::A3,
                Square::B3,
                Square::C3,
                Square::D3,
                Square::E3,
                Square::F3,
                Square::G3,
                Square::H3,
                Square::A4,
                Square::B4,
                Square::C4,
                Square::D4,
                Square::E4,
                Square::F4,
                Square::G4,
                Square::H4,
                Square::A5,
                Square::B5,
                Square::C5,
                Square::D5,
                Square::E5,
                Square::F5,
                Square::G5,
                Square::H5,
                Square::A6,
                Square::B6,
                Square::C6,
                Square::D6,
                Square::E6,
                Square::F6,
                Square::G6,
                Square::H6,
                Square::A7,
                Square::B7,
                Square::C7,
                Square::D7,
                Square::E7,
                Square::F7,
                Square::G7,
                Square::H7,
                Square::A8,
                Square::B8,
                Square::C8,
                Square::D8,
                Square::E8,
                Square::F8,
                Square::G8,
                Square::H8,
            ];
            SQUARES[idx as usize]
        } else {
            Square::NONE
        }
    }
}

impl ops::Sub<Direction> for Square {
    type Output = Square;
    fn sub(self, rhs: Direction) -> Square {
        self + Direction::from(-(rhs as i16))
    }
}

impl Direction {
    fn from(val: i16) -> Direction {
        match val {
            8 => Direction::North,
            1 => Direction::East,
            -8 => Direction::South,
            -1 => Direction::West,
            9 => Direction::NorthEast,
            7 => Direction::NorthWest,
            -7 => Direction::SouthEast,
            -9 => Direction::SouthWest,
            _ => panic!("Invalid direction value"),
        }
    }
}

impl ops::Add for Direction {
    type Output = Direction;
    fn add(self, rhs: Direction) -> Direction {
        let val = (self as i16) + (rhs as i16);
        match val {
            16 => Direction::North,
            2 => Direction::East,
            -16 => Direction::South,
            -2 => Direction::West,
            8 => Direction::North,
            1 => Direction::East,
            -8 => Direction::South,
            -1 => Direction::West,
            9 => Direction::NorthEast,
            7 => Direction::NorthWest,
            -7 => Direction::SouthEast,
            -9 => Direction::SouthWest,
            17 => Direction::NorthEast,
            15 => Direction::NorthWest,
            -15 => Direction::SouthEast,
            -17 => Direction::SouthWest,
            _ => panic!("Invalid combined direction"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_square_conversion() {
        let sq = make_square(File::A, Rank::R1);
        assert_eq!(sq, Square::A1);
        assert_eq!(file_of(sq), File::A);
        assert_eq!(rank_of(sq), Rank::R1);

        let sq = make_square(File::H, Rank::R8);
        assert_eq!(sq, Square::H8);
        assert_eq!(file_of(sq), File::H);
        assert_eq!(rank_of(sq), Rank::R8);
    }

    #[test]
    fn test_bitboard_ops() {
        let b1 = Bitboard(0xFF);
        let b2 = Bitboard(0x0F);
        assert_eq!((b1 & b2).0, 0x0F);
        assert_eq!((b1 | b2).0, 0xFF);
        assert_eq!((!b1).0, !0xFFu64);
        assert_eq!(Bitboard::square_bb(Square::A1).0, 1);
        assert_eq!(Bitboard::square_bb(Square::H8).0, 1u64 << 63);
    }

    #[test]
    fn test_move_encoding() {
        let m = Move::make_move(Square::A2, Square::A4);
        assert_eq!(m.from_sq(), Square::A2);
        assert_eq!(m.to_sq(), Square::A4);
        assert_eq!(m.move_type(), MoveType::Normal);

        let m = Move::make_enpassant(Square::C5, Square::D6);
        assert_eq!(m.move_type(), MoveType::EnPassant);

        let m = Move::make_castling(Square::E1, Square::H1);
        assert_eq!(m.move_type(), MoveType::Castling);

        let m = Move::make_promotion(Square::A7, Square::A8, PieceType::Queen);
        assert_eq!(m.move_type(), MoveType::Promotion);
        assert_eq!(m.promotion_type(), PieceType::Queen);
    }

    #[test]
    fn test_piece() {
        let wp = make_piece(Color::White, PieceType::Pawn);
        assert_eq!(color_of(wp), Color::White);
        assert_eq!(type_of(wp), PieceType::Pawn);

        let bk = make_piece(Color::Black, PieceType::Commoner);
        assert_eq!(color_of(bk), Color::Black);
        assert_eq!(type_of(bk), PieceType::Commoner);
    }
}
