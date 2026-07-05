use std::fmt;
use std::ops;

/// A square on a chessboard indexed `A1` (0) through `H8` (63), plus `NONE`.
///
/// Layout: `A1` = 0, `B1` = 1, …, `H1` = 7, `A2` = 8, …, `H8` = 63.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
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
/// Number of squares on a chessboard.
pub const SQUARE_NB: usize = 64;
/// Number of files on a chessboard.
pub const FILE_NB: usize = 8;
/// Number of ranks on a chessboard.
pub const RANK_NB: usize = 8;

// Consolidation point for Square-by-index lookup.
pub(crate) const SQUARES: [Square; 64] = [
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

/// A file (column) on a chessboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
#[non_exhaustive]
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

/// A rank (row) on a chessboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
#[non_exhaustive]
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

/// Return the file of a square.
pub fn file_of(s: Square) -> File {
    static FILES: [File; 8] = [
        File::A,
        File::B,
        File::C,
        File::D,
        File::E,
        File::F,
        File::G,
        File::H,
    ];
    FILES[(s as u8 & 7) as usize]
}

/// Return the rank of a square.
pub fn rank_of(s: Square) -> Rank {
    static RANKS: [Rank; 8] = [
        Rank::R1,
        Rank::R2,
        Rank::R3,
        Rank::R4,
        Rank::R5,
        Rank::R6,
        Rank::R7,
        Rank::R8,
    ];
    RANKS[((s as u8 >> 3) & 7) as usize]
}

/// Construct a square from a file and rank.
pub fn make_square(f: File, r: Rank) -> Square {
    let idx = (r as usize) * 8 + (f as usize);
    SQUARES[idx]
}

/// A set of squares represented as a 64-bit bitboard.
///
/// Bit `n` corresponds to [`Square`] with discriminant `n`. Supports standard
/// bitwise operators (`&`, `|`, `^`, `!`, `<<`, `>>`) as well as set-wise
/// operations with [`Square`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bitboard(pub u64);

impl Bitboard {
    /// The empty bitboard (no squares set).
    pub const EMPTY: Bitboard = Bitboard(0);

    /// Returns `true` if no squares are set.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Return the number of squares set in the bitboard.
    #[must_use]
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Return the least-significant (lowest-index) set square.
    ///
    /// # Panics
    /// Panics in debug mode if the bitboard is empty.
    #[must_use]
    pub fn lsb(self) -> Square {
        debug_assert!(!self.is_empty());
        let idx = self.0.trailing_zeros() as u8;
        // SAFETY: trailing_zeros() returns 0..63 when self is non-empty.
        // All discriminants 0..63 are valid Square values.
        unsafe { std::mem::transmute(idx) }
    }

    /// Extract and remove the least-significant set square.
    pub fn pop_lsb(&mut self) -> Square {
        let sq = self.lsb();
        self.0 &= self.0 - 1;
        sq
    }

    /// Returns `true` if more than one square is set.
    #[must_use]
    pub fn more_than_one(self) -> bool {
        self.0 & (self.0 - 1) != 0
    }

    /// Return a bitboard with only the given square set.
    /// Returns [`EMPTY`](Self::EMPTY) for [`Square::NONE`].
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

/// A side in a chess game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    pub const NB: usize = 2;

    /// Return the opposite color.
    pub fn flip(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

/// A piece type (pawn, knight, bishop, rook, queen, or commoner).
///
/// In atomic chess, a *commoner* moves like a king and is pseudo-royal:
/// losing all commoners loses the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
#[non_exhaustive]
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

/// A colored piece, packed into a single byte.
///
/// Encoding: `(color << 3) | (type + 1)` so that `0` can represent
/// [`NO_PIECE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece(u8);

impl Piece {
    // Encode as (color << 3) | (pt as u8 + 1) to avoid NO_PIECE = 0 conflict
    pub const fn from_parts(color: Color, pt: PieceType) -> Piece {
        Piece(((color as u8) << 3) | ((pt as u8) + 1))
    }

    /// Return the color of this piece.
    #[must_use]
    pub fn color(self) -> Color {
        if self.0 & 8 == 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    /// Return the piece type.
    #[must_use]
    pub fn type_of(self) -> PieceType {
        let inner = (self.0 & 7) - 1;
        debug_assert!(
            inner < 6,
            "Piece::type_of called with invalid Piece encoding: inner={}",
            inner
        );
        // SAFETY: inner is 0..5 after masking and decrementing a valid Piece.
        // All discriminants 0..5 are valid PieceType values.
        unsafe { std::mem::transmute(inner) }
    }

    /// Return the ASCII character for this piece.
    ///
    /// Upper-case for white (`P`, `N`, `B`, `R`, `Q`, `C`), lower-case for black.
    #[must_use]
    pub fn ascii_char(self) -> char {
        let t = match self.type_of() {
            PieceType::Pawn => 'P',
            PieceType::Knight => 'N',
            PieceType::Bishop => 'B',
            PieceType::Rook => 'R',
            PieceType::Queen => 'Q',
            PieceType::Commoner => 'C',
        };
        match self.color() {
            Color::White => t,
            Color::Black => t.to_ascii_lowercase(),
        }
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
        write!(f, "{}{}", c, self.ascii_char().to_ascii_uppercase())
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

/// Construct a [`Piece`] from a color and piece type.
pub fn make_piece(color: Color, pt: PieceType) -> Piece {
    Piece::from_parts(color, pt)
}

/// The type of a chess move (normal, promotion, en-passant, castling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MoveType {
    Normal = 0,
    Promotion = 1,
    EnPassant = 2,
    Castling = 3,
}

/// A chess move packed into a 16-bit integer.
///
/// Bit layout (0-indexed LSB): `to_sq:6 | from_sq:6 | type:2 | promotion_type:2`.
/// - `promotion_type` is valid only when `type == Promotion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move(u16);

impl Move {
    /// A sentinel value representing "no move".
    pub const NONE: Move = Move(0);
    /// A sentinel null move (from_sq=1, to_sq=1).
    pub const NULL: Move = Move(1 + (1 << 6));

    /// Return the origin square of this move.
    #[must_use]
    pub fn from_sq(self) -> Square {
        let idx = ((self.0 >> 6) & 0x3f) as u8;
        // SAFETY: (self.0 >> 6) & 0x3f extracts a 6-bit field (0..63).
        // All discriminants 0..63 are valid Square values.
        unsafe { std::mem::transmute(idx) }
    }

    /// Return the destination square of this move.
    #[must_use]
    pub fn to_sq(self) -> Square {
        let idx = (self.0 & 0x3f) as u8;
        // SAFETY: self.0 & 0x3f extracts a 6-bit field (0..63).
        // All discriminants 0..63 are valid Square values.
        unsafe { std::mem::transmute(idx) }
    }

    /// Return the move type (normal, promotion, en-passant, castling).
    #[must_use]
    pub fn move_type(self) -> MoveType {
        match (self.0 >> 12) & 3 {
            0 => MoveType::Normal,
            1 => MoveType::Promotion,
            2 => MoveType::EnPassant,
            _ => MoveType::Castling,
        }
    }

    /// Return the promotion piece type (valid only for promotion moves).
    #[must_use]
    pub fn promotion_type(self) -> PieceType {
        static TYPES: [PieceType; 4] = [
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
        ];
        TYPES[((self.0 >> 14) & 3) as usize]
    }

    /// Construct a normal (non-promotion) move.
    pub fn make_move(from: Square, to: Square) -> Move {
        Move(((from as u16) << 6) | (to as u16))
    }

    /// Construct a promotion move. `pt` should be a non-pawn piece type.
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

    /// Construct an en-passant capture move.
    pub fn make_enpassant(from: Square, to: Square) -> Move {
        Move((2 << 12) | ((from as u16) << 6) | (to as u16))
    }

    /// Construct a castling move.
    pub fn make_castling(from: Square, to: Square) -> Move {
        Move((3 << 12) | ((from as u16) << 6) | (to as u16))
    }
}

/// A direction on the chessboard (used for stepping computations).
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
        if (0..64).contains(&idx) {
            SQUARES[idx as usize]
        } else {
            Square::NONE
        }
    }
}

impl ops::Sub<Direction> for Square {
    type Output = Square;
    fn sub(self, rhs: Direction) -> Square {
        let idx = (self as i16) - (rhs as i16);
        if (0..64).contains(&idx) {
            SQUARES[idx as usize]
        } else {
            Square::NONE
        }
    }
}

/// Maximum number of moves that can be generated for any atomic chess position.
///
/// The absolute upper bound is well below 256:
/// - At most 64 squares with attackers
/// - Perft at depth 1 on the most complex legal positions yields < 150 moves
/// - 256 provides a comfortable safety margin.
pub const MAX_MOVES: usize = 256;

/// A fixed-capacity, stack-allocated list of `Move` values.
///
/// This is a drop-in replacement for `Vec<Move>` in move-generation hot paths.
/// It avoids heap allocation, eliminates dynamic capacity checks, and improves
/// cache locality by keeping the entire move list on the stack.
#[derive(Debug, Clone)]
pub struct MoveList {
    moves: [Move; MAX_MOVES],
    len: usize,
}

impl MoveList {
    /// Creates an empty `MoveList`.
    #[inline]
    pub fn new() -> Self {
        MoveList {
            moves: [Move::NONE; MAX_MOVES],
            len: 0,
        }
    }

    /// Returns the number of moves currently stored.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the list contains zero moves.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Appends a move to the end of the list.
    ///
    /// # Panics
    /// Panics in debug mode if the list is full (`len == MAX_MOVES`).
    /// In release mode, exceeding the capacity silently overwrites (unreachable
    /// in practice due to the move-count bound).
    #[inline]
    pub fn push(&mut self, m: Move) {
        debug_assert!(self.len < MAX_MOVES, "MoveList overflow");
        // Safe: unreachable in practice due to move-count bound.
        if self.len < MAX_MOVES {
            self.moves[self.len] = m;
            self.len += 1;
        }
    }

    /// Sets the length directly (caller must ensure `len <= MAX_MOVES` and
    /// that elements beyond `len` are unused).
    #[inline]
    pub(crate) fn set_len(&mut self, len: usize) {
        debug_assert!(len <= MAX_MOVES, "MoveList::set_len overflow");
        self.len = len;
    }

    /// Returns the stored moves as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[Move] {
        &self.moves[..self.len]
    }

    /// Returns the stored moves as a mutable slice (for sorting, etc.).
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [Move] {
        let len = self.len;
        &mut self.moves[..len]
    }
}

/// Convert a `Square` to its algebraic notation string (e.g. `Square::E2` -> `"e2"`).
///
/// This is a convenience helper exposed for the crate's own example binaries;
/// downstream consumers should prefer formatting squares via their own display
/// logic. Not covered by semantic versioning guarantees.
#[doc(hidden)]
pub fn sq_str(sq: Square) -> String {
    let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    let idx = sq as usize;
    format!("{}{}", files[idx % 8], (idx / 8 + 1))
}

/// Parse a square in algebraic notation (e.g. `"e2"`) into a `Square`.
///
/// This is a convenience helper exposed for the crate's own example binaries;
/// downstream consumers should prefer more robust parsers. Not covered by
/// semantic versioning guarantees.
#[doc(hidden)]
pub fn parse_sq(s: &str) -> Square {
    if s.len() < 2 {
        return Square::A1;
    }
    let file = match s.chars().next().unwrap() {
        'a' => 0,
        'b' => 1,
        'c' => 2,
        'd' => 3,
        'e' => 4,
        'f' => 5,
        'g' => 6,
        'h' => 7,
        _ => 0,
    };
    let rank = match s.chars().nth(1).unwrap() {
        '1' => Rank::R1,
        '2' => Rank::R2,
        '3' => Rank::R3,
        '4' => Rank::R4,
        '5' => Rank::R5,
        '6' => Rank::R6,
        '7' => Rank::R7,
        '8' => Rank::R8,
        _ => Rank::R1,
    };
    make_square(
        match file {
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
        rank,
    )
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

/// Iteration over `&MoveList` yields `Move` by value (cheap copy).
impl<'a> IntoIterator for &'a MoveList {
    type Item = Move;
    type IntoIter = MoveListIter<'a>;

    fn into_iter(self) -> MoveListIter<'a> {
        MoveListIter {
            iter: self.as_slice().iter(),
        }
    }
}

/// An iterator over the moves in a [`MoveList`].
///
/// Obtained via [`IntoIterator`] on `&MoveList`. Yields [`Move`] by value.
pub struct MoveListIter<'a> {
    iter: std::slice::Iter<'a, Move>,
}

impl<'a> Iterator for MoveListIter<'a> {
    type Item = Move;

    #[inline]
    fn next(&mut self) -> Option<Move> {
        self.iter.next().copied()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a> ExactSizeIterator for MoveListIter<'a> {}

impl ops::Index<usize> for MoveList {
    type Output = Move;

    #[inline]
    fn index(&self, index: usize) -> &Move {
        &self.moves[index]
    }
}

impl ops::IndexMut<usize> for MoveList {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Move {
        &mut self.moves[index]
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
        assert_eq!(wp.color(), Color::White);
        assert_eq!(wp.type_of(), PieceType::Pawn);

        let bk = make_piece(Color::Black, PieceType::Commoner);
        assert_eq!(bk.color(), Color::Black);
        assert_eq!(bk.type_of(), PieceType::Commoner);
    }
}
