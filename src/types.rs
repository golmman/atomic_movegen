//! Core types and constants for atomic chess move generation.
//!
//! This module defines squares, bitboards, pieces, colors, moves, and the
//! move list container used by the rest of the crate.

use std::fmt;
use std::ops;

/// A square on a chessboard indexed `A1` (0) through `H8` (63), plus `NONE`.
///
/// Layout: `A1` = 0, `B1` = 1, …, `H1` = 7, `A2` = 8, …, `H8` = 63.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[allow(missing_docs)]
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

macro_rules! define_sq_consts {
    ($($name:ident, $variant:ident);*) => {
        $(
            #[allow(missing_docs)]
            pub const $name: Square = Square::$variant;
        )*
    };
}

define_sq_consts!(
    SQ_A1, A1; SQ_B1, B1; SQ_C1, C1; SQ_D1, D1; SQ_E1, E1; SQ_F1, F1; SQ_G1, G1; SQ_H1, H1;
    SQ_A2, A2; SQ_B2, B2; SQ_C2, C2; SQ_D2, D2; SQ_E2, E2; SQ_F2, F2; SQ_G2, G2; SQ_H2, H2;
    SQ_A3, A3; SQ_B3, B3; SQ_C3, C3; SQ_D3, D3; SQ_E3, E3; SQ_F3, F3; SQ_G3, G3; SQ_H3, H3;
    SQ_A4, A4; SQ_B4, B4; SQ_C4, C4; SQ_D4, D4; SQ_E4, E4; SQ_F4, F4; SQ_G4, G4; SQ_H4, H4;
    SQ_A5, A5; SQ_B5, B5; SQ_C5, C5; SQ_D5, D5; SQ_E5, E5; SQ_F5, F5; SQ_G5, G5; SQ_H5, H5;
    SQ_A6, A6; SQ_B6, B6; SQ_C6, C6; SQ_D6, D6; SQ_E6, E6; SQ_F6, F6; SQ_G6, G6; SQ_H6, H6;
    SQ_A7, A7; SQ_B7, B7; SQ_C7, C7; SQ_D7, D7; SQ_E7, E7; SQ_F7, F7; SQ_G7, G7; SQ_H7, H7;
    SQ_A8, A8; SQ_B8, B8; SQ_C8, C8; SQ_D8, D8; SQ_E8, E8; SQ_F8, F8; SQ_G8, G8; SQ_H8, H8
);
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

impl Square {
    /// Construct a [`Square`] from its 0–63 index. Returns [`Square::NONE`]
    /// for out-of-range values.
    #[inline]
    pub fn from_index(idx: i8) -> Square {
        Square::from_u8(idx as u8)
    }

    /// Construct a [`Square`] from its 0–63 index as a `u8`.
    #[inline]
    pub fn from_u8(idx: u8) -> Square {
        if (0..64).contains(&idx) {
            SQUARES[idx as usize]
        } else {
            Square::NONE
        }
    }
}

/// A file (column) on a chessboard.
#[allow(missing_docs)]
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
    /// Number of files on a chessboard.
    pub const NB: usize = 8;
}

/// A rank (row) on a chessboard.
#[allow(missing_docs)]
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
    /// Number of ranks on a chessboard.
    pub const NB: usize = 8;
}

/// Return the file of a square.
#[inline]
pub fn file_of(s: Square) -> File {
    const FILES: [File; 8] = [
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
#[inline]
pub fn rank_of(s: Square) -> Rank {
    const RANKS: [Rank; 8] = [
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
#[inline]
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
    #[inline]
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
    /// Returns [`Square::NONE`] if the bitboard is empty.
    #[inline]
    #[must_use]
    pub fn lsb(self) -> Square {
        if self.is_empty() {
            return Square::NONE;
        }
        let idx = self.0.trailing_zeros() as usize;
        SQUARES[idx]
    }

    /// Extract and remove the least-significant set square.
    #[inline]
    pub fn pop_lsb(&mut self) -> Square {
        let sq = self.lsb();
        self.0 &= self.0 - 1;
        sq
    }

    /// Return a bitboard with only the given square set.
    /// Returns [`EMPTY`](Self::EMPTY) for [`Square::NONE`].
    #[inline]
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

/// A side in a chess game.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    /// Number of colors in a chess game.
    pub const NB: usize = 2;

    /// Return the opposite color.
    #[inline]
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
#[allow(missing_docs)]
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
    /// Number of piece types.
    pub const NB: usize = 6;
}

pub(crate) const PIECE_TYPES: [PieceType; 6] = [
    PieceType::Pawn,
    PieceType::Knight,
    PieceType::Bishop,
    PieceType::Rook,
    PieceType::Queen,
    PieceType::Commoner,
];

/// A colored piece, packed into a single byte.
///
/// Encoding: `(color << 3) | (type + 1)` so that `0` can represent
/// [`NO_PIECE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece(u8);

impl Piece {
    /// Construct a [`Piece`] from a color and piece type.
    ///
    /// The internal encoding is `(color << 3) | (piece_type + 1)` so that
    /// [`NO_PIECE`] can be represented by the value `0`.
    #[inline]
    pub const fn from_parts(color: Color, pt: PieceType) -> Piece {
        Piece(((color as u8) << 3) | ((pt as u8) + 1))
    }

    /// Return the color of this piece.
    ///
    /// # Panics
    /// Panics in debug builds if called on [`NO_PIECE`].
    #[inline]
    #[must_use]
    pub fn color(self) -> Color {
        debug_assert!(self.0 != 0, "Piece::color called on NO_PIECE");
        if self.0 & 8 == 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    /// Return the piece type.
    ///
    /// # Panics
    /// Panics in debug builds if called on [`NO_PIECE`]. In release builds, an
    /// out-of-bounds `PIECE_TYPES` lookup panics for [`NO_PIECE`].
    #[inline]
    #[must_use]
    pub fn type_of(self) -> PieceType {
        debug_assert!(self.0 != 0, "Piece::type_of called on NO_PIECE");
        let inner = (self.0 & 7) - 1;
        PIECE_TYPES[inner as usize]
    }

    /// Return the ASCII character for this piece.
    ///
    /// Upper-case for white (`P`, `N`, `B`, `R`, `Q`, `C`), lower-case for black.
    /// Returns `'.'` for [`NO_PIECE`].
    #[must_use]
    pub fn ascii_char(self) -> char {
        if self.0 == 0 {
            return '.';
        }
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

/// An empty square (no piece).
pub const NO_PIECE: Piece = Piece(0);
/// White pawn.
pub const W_PAWN: Piece = Piece::from_parts(Color::White, PieceType::Pawn);
/// White knight.
pub const W_KNIGHT: Piece = Piece::from_parts(Color::White, PieceType::Knight);
/// White bishop.
pub const W_BISHOP: Piece = Piece::from_parts(Color::White, PieceType::Bishop);
/// White rook.
pub const W_ROOK: Piece = Piece::from_parts(Color::White, PieceType::Rook);
/// White queen.
pub const W_QUEEN: Piece = Piece::from_parts(Color::White, PieceType::Queen);
/// White commoner.
pub const W_COMMONER: Piece = Piece::from_parts(Color::White, PieceType::Commoner);
/// Black pawn.
pub const B_PAWN: Piece = Piece::from_parts(Color::Black, PieceType::Pawn);
/// Black knight.
pub const B_KNIGHT: Piece = Piece::from_parts(Color::Black, PieceType::Knight);
/// Black bishop.
pub const B_BISHOP: Piece = Piece::from_parts(Color::Black, PieceType::Bishop);
/// Black rook.
pub const B_ROOK: Piece = Piece::from_parts(Color::Black, PieceType::Rook);
/// Black queen.
pub const B_QUEEN: Piece = Piece::from_parts(Color::Black, PieceType::Queen);
/// Black commoner.
pub const B_COMMONER: Piece = Piece::from_parts(Color::Black, PieceType::Commoner);

/// Construct a [`Piece`] from a color and piece type.
#[inline]
pub const fn make_piece(color: Color, pt: PieceType) -> Piece {
    Piece::from_parts(color, pt)
}

/// The piece types a pawn may promote to, ordered by promotion bit encoding.
pub const PROMOTION_PIECES: [PieceType; 4] = [
    PieceType::Queen,
    PieceType::Rook,
    PieceType::Bishop,
    PieceType::Knight,
];

/// The type of a chess move (normal, promotion, en-passant, castling).
#[allow(missing_docs)]
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

    /// Return the origin square of this move.
    #[inline]
    #[must_use]
    pub fn from_sq(self) -> Square {
        let idx = ((self.0 >> 6) & 0x3f) as usize;
        SQUARES[idx]
    }

    /// Return the destination square of this move.
    #[inline]
    #[must_use]
    pub fn to_sq(self) -> Square {
        let idx = (self.0 & 0x3f) as usize;
        SQUARES[idx]
    }

    /// Return the move type (normal, promotion, en-passant, castling).
    #[inline]
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
    #[inline]
    #[must_use]
    pub fn promotion_type(self) -> PieceType {
        PROMOTION_PIECES[((self.0 >> 14) & 3) as usize]
    }

    /// Construct a normal (non-promotion) move.
    ///
    /// # Panics
    /// Panics in debug builds if `from` or `to` is [`Square::NONE`].
    #[inline]
    pub fn make_move(from: Square, to: Square) -> Move {
        debug_assert!(from != Square::NONE && to != Square::NONE);
        Move(((from as u16) << 6) | (to as u16))
    }

    /// Construct a promotion move. `pt` must be a non-pawn piece type.
    ///
    /// # Panics
    /// Panics if `pt` is not a valid promotion piece (`Knight`, `Bishop`, `Rook`,
    /// or `Queen`). In debug builds, also panics if `from` or `to` is
    /// [`Square::NONE`].
    #[inline]
    pub fn make_promotion(from: Square, to: Square, pt: PieceType) -> Move {
        debug_assert!(from != Square::NONE && to != Square::NONE);
        let pt_bits = PROMOTION_PIECES
            .iter()
            .position(|&p| p == pt)
            .unwrap_or_else(|| panic!("invalid promotion piece: {:?}", pt))
            as u16;
        Move((pt_bits << 14) | (1 << 12) | ((from as u16) << 6) | (to as u16))
    }

    /// Construct an en-passant capture move.
    ///
    /// # Panics
    /// Panics in debug builds if `from` or `to` is [`Square::NONE`].
    #[inline]
    pub fn make_enpassant(from: Square, to: Square) -> Move {
        debug_assert!(from != Square::NONE && to != Square::NONE);
        Move((2 << 12) | ((from as u16) << 6) | (to as u16))
    }

    /// Construct a castling move.
    ///
    /// # Panics
    /// Panics in debug builds if `from` or `to` is [`Square::NONE`].
    #[inline]
    pub fn make_castling(from: Square, to: Square) -> Move {
        debug_assert!(from != Square::NONE && to != Square::NONE);
        Move((3 << 12) | ((from as u16) << 6) | (to as u16))
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
/// Returns `None` for [`Square::NONE`].
/// This is a convenience helper exposed for the crate's own example binaries;
/// downstream consumers should prefer formatting squares via their own display
/// logic. Not covered by semantic versioning guarantees.
pub fn sq_str(sq: Square) -> Option<&'static str> {
    const STRS: [&str; 64] = [
        "a1", "b1", "c1", "d1", "e1", "f1", "g1", "h1", "a2", "b2", "c2", "d2", "e2", "f2", "g2",
        "h2", "a3", "b3", "c3", "d3", "e3", "f3", "g3", "h3", "a4", "b4", "c4", "d4", "e4", "f4",
        "g4", "h4", "a5", "b5", "c5", "d5", "e5", "f5", "g5", "h5", "a6", "b6", "c6", "d6", "e6",
        "f6", "g6", "h6", "a7", "b7", "c7", "d7", "e7", "f7", "g7", "h7", "a8", "b8", "c8", "d8",
        "e8", "f8", "g8", "h8",
    ];
    if sq == Square::NONE {
        return None;
    }
    Some(STRS[sq as usize])
}

/// Parse a square in algebraic notation (e.g. `"e2"`) into a `Square`.
///
/// Returns `None` for malformed input.
/// This is a convenience helper exposed for the crate's own example binaries;
/// downstream consumers should prefer more robust parsers. Not covered by
/// semantic versioning guarantees.
pub fn parse_sq(s: &str) -> Option<Square> {
    if s.len() != 2 || !s.is_ascii() {
        return None;
    }
    let bytes = s.as_bytes();
    let file = match bytes[0] {
        b'a' => 0,
        b'b' => 1,
        b'c' => 2,
        b'd' => 3,
        b'e' => 4,
        b'f' => 5,
        b'g' => 6,
        b'h' => 7,
        _ => return None,
    };
    let rank = match bytes[1] {
        b'1' => Rank::R1,
        b'2' => Rank::R2,
        b'3' => Rank::R3,
        b'4' => Rank::R4,
        b'5' => Rank::R5,
        b'6' => Rank::R6,
        b'7' => Rank::R7,
        b'8' => Rank::R8,
        _ => return None,
    };
    Some(make_square(
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
    ))
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
