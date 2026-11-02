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

    /// Returns `true` if the square at `index` (`0..64`) is set.
    ///
    /// This is the raw-index variant of a membership test, avoiding the
    /// [`SQUARES`] and [`square_bb`](Self::square_bb) table lookups.
    #[inline(always)]
    pub(crate) fn contains(self, index: usize) -> bool {
        (self.0 >> index) & 1 != 0
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

/// The result of a finished game, from the side-to-move perspective.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    Win,
    Loss,
    Draw,
}

/// How an en-passant target participates in a position key or FEN.
///
/// The stored en-passant target follows Fairy-Stockfish: a double pawn push
/// always records one, even when no legal en-passant capture exists. The
/// default [`Board::hash`](crate::Board::hash) and
/// [`Board::fen`](crate::Board::fen) use [`Always`](Self::Always); the
/// `_with` variants accept this mode explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EnPassantMode {
    /// Include the stored ep square after every double push (FSF-compatible,
    /// the default for `hash()` and `fen()`).
    Always,
    /// Include the ep square only when at least one fully legal ep capture
    /// exists.
    Legal,
}

/// A piece type (pawn, knight, bishop, rook, queen, or commoner).
///
/// In atomic chess, a *commoner* moves like a king and is royal: losing it
/// loses the game. Each side has at most one commoner, and pawns cannot
/// promote to a commoner.
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

    /// Return the raw internal encoding.
    ///
    /// The value is `(color << 3) | (piece_type + 1)`, or `0` for [`NO_PIECE`].
    #[inline]
    #[must_use]
    pub(crate) const fn raw(self) -> u8 {
        self.0
    }

    /// Return the color of this piece, or `None` for [`NO_PIECE`].
    #[inline]
    #[must_use]
    pub fn color(self) -> Option<Color> {
        if self.0 == 0 {
            return None;
        }
        Some(if self.0 & 8 == 0 {
            Color::White
        } else {
            Color::Black
        })
    }

    /// Return the piece type, or `None` for [`NO_PIECE`].
    #[inline]
    #[must_use]
    pub fn type_of(self) -> Option<PieceType> {
        if self.0 == 0 {
            return None;
        }
        Some(PIECE_TYPES[((self.0 & 7) - 1) as usize])
    }

    /// Return the ASCII character for this piece.
    ///
    /// Upper-case for white (`P`, `N`, `B`, `R`, `Q`, `K`), lower-case for black.
    /// The commoner is serialized with the standard FEN letter `K`/`k` (it is
    /// the king-like piece of atomic chess). Returns `'.'` for [`NO_PIECE`].
    #[must_use]
    pub fn ascii_char(self) -> char {
        if self.0 == 0 {
            return '.';
        }
        let t = match self.type_of().unwrap() {
            PieceType::Pawn => 'P',
            PieceType::Knight => 'N',
            PieceType::Bishop => 'B',
            PieceType::Rook => 'R',
            PieceType::Queen => 'Q',
            PieceType::Commoner => 'K',
        };
        match self.color().unwrap() {
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
        let c = match self.color().unwrap() {
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
///
/// Atomic chess does not allow promotion to a commoner.
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
/// # Stable wire encoding
///
/// Bit layout (0-indexed LSB):
/// `to_sq:6 | from_sq:6 | type:2 | promotion_type:2`.
/// - `to_sq` and `from_sq` are square indices `0..64` (`a1` = 0, `h8` = 63).
/// - `type` is the [`MoveType`] discriminant (`Normal` = 0, `Promotion` = 1,
///   `EnPassant` = 2, `Castling` = 3).
/// - `promotion_type` is an index into [`PROMOTION_PIECES`] and is meaningful
///   only when `type == Promotion`.
///
/// The layout, the sentinel [`Move::NONE`] value `0`, and the order of
/// [`PROMOTION_PIECES`] (queen, rook, bishop, knight) are semver-stable: they
/// can only change in a major release, because [`to_bits`](Self::to_bits) is
/// intended to be persisted. Castling and en-passant moves use the board's
/// internal encoding, which [`Display`](fmt::Display) and
/// [`Board::move_from_uci`](crate::Board::move_from_uci) translate to and from
/// ordinary UCI text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Move(u16);

impl Move {
    /// A sentinel value representing "no move".
    pub const NONE: Move = Move(0);

    /// Return the stable 16-bit wire encoding of this move.
    ///
    /// This is the exact inverse of [`from_bits`](Self::from_bits) for every
    /// structurally valid value. The bit layout is documented on [`Move`] and
    /// is semver-stable.
    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> u16 {
        self.0
    }

    /// Decode a stable 16-bit wire code.
    ///
    /// - `0` decodes to `Some(Move::NONE)`.
    /// - Any other code whose origin equals its destination decodes to `None`.
    /// - Every remaining code decodes to `Some(move)`; no board legality is
    ///   implied. In particular, a castling or en-passant bit pattern is only
    ///   meaningful together with a board.
    #[must_use]
    pub fn from_bits(bits: u16) -> Option<Move> {
        if bits == 0 {
            return Some(Move::NONE);
        }
        let m = Move(bits);
        if m.from_sq() == m.to_sq() {
            None
        } else {
            Some(m)
        }
    }

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

    /// Raw origin-square index (`0..64`) without the [`SQUARES`] conversion.
    ///
    /// Only valid for moves produced by the move generator; [`Move::NONE`]
    /// would map to index `0` (a1) rather than [`Square::NONE`].
    #[inline(always)]
    pub(crate) fn origin_index(self) -> usize {
        ((self.0 >> 6) & 0x3f) as usize
    }

    /// Raw destination-square index (`0..64`) without the [`SQUARES`] conversion.
    ///
    /// Only valid for moves produced by the move generator; see
    /// [`origin_index`](Self::origin_index).
    #[inline(always)]
    pub(crate) fn dest_index(self) -> usize {
        (self.0 & 0x3f) as usize
    }

    /// Returns `true` for en-passant and castling moves, i.e. the two move
    /// types that always need the full legality check.
    #[inline(always)]
    pub(crate) fn needs_full_legal_check(self) -> bool {
        ((self.0 >> 12) & 3) >= 2
    }

    /// Returns `true` when the promotion-type bits are canonical for this move
    /// type: they are free for promotions and zero for every other type.
    ///
    /// [`from_bits`](Self::from_bits) deliberately decodes non-canonical
    /// patterns; the board-aware pseudo-legality predicate rejects them because
    /// the generator never emits them.
    #[inline(always)]
    pub(crate) fn has_canonical_promotion_bits(self) -> bool {
        self.move_type() == MoveType::Promotion || ((self.0 >> 14) & 3) == 0
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

    /// Returns `true` for castling moves.
    #[inline]
    #[must_use]
    pub fn is_castling(self) -> bool {
        self.move_type() == MoveType::Castling
    }

    /// Returns `true` for promotion moves.
    #[inline]
    #[must_use]
    pub fn is_promotion(self) -> bool {
        self.move_type() == MoveType::Promotion
    }

    /// Returns `true` for en-passant moves.
    #[inline]
    #[must_use]
    pub fn is_en_passant(self) -> bool {
        self.move_type() == MoveType::EnPassant
    }

    /// Parse a move in UCI coordinate notation without consulting a board.
    ///
    /// This is a *syntax-only* parser: `e1g1` yields an ordinary
    /// `e1 -> g1` move, not a castling move. Use
    /// [`Board::move_from_uci`](crate::Board::move_from_uci) to resolve
    /// castling destinations and to require legality. `"0000"` maps to
    /// [`Move::NONE`]. Promotion suffixes are `n`, `b`, `r`, and `q`; anything
    /// else, a malformed length, or equal origin and destination returns
    /// `None`.
    #[must_use]
    pub fn from_uci(s: &str) -> Option<Move> {
        if s == "0000" {
            return Some(Move::NONE);
        }

        let bytes = s.as_bytes();
        if !(bytes.len() == 4 || bytes.len() == 5) || !s.is_ascii() {
            return None;
        }

        let from = parse_sq(&s[..2])?;
        let to = parse_sq(&s[2..4])?;
        if from == to {
            return None;
        }

        if bytes.len() == 4 {
            return Some(Move::make_move(from, to));
        }

        let promotion = match bytes[4] {
            b'n' => PieceType::Knight,
            b'b' => PieceType::Bishop,
            b'r' => PieceType::Rook,
            b'q' => PieceType::Queen,
            _ => return None,
        };
        Some(Move::make_promotion(from, to, promotion))
    }

    /// Write this move in UCI coordinate notation without allocating.
    ///
    /// [`Move::NONE`] writes `0000`. Castling destinations are mapped to
    /// `g1`/`c1`/`g8`/`c8`, and promotion moves append `n`/`b`/`r`/`q`.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`fmt::Error`] if the writer fails.
    pub fn write_uci(self, out: &mut impl fmt::Write) -> fmt::Result {
        if self.0 == 0 {
            return out.write_str("0000");
        }

        out.write_str(sq_str(self.from_sq()).unwrap_or("??"))?;
        let to = if self.is_castling() {
            match (self.from_sq(), self.to_sq()) {
                (Square::E1, Square::H1) => "g1",
                (Square::E1, Square::A1) => "c1",
                (Square::E8, Square::H8) => "g8",
                (Square::E8, Square::A8) => "c8",
                _ => sq_str(self.to_sq()).unwrap_or("??"),
            }
        } else {
            sq_str(self.to_sq()).unwrap_or("??")
        };
        out.write_str(to)?;

        if self.is_promotion() {
            let ch = match self.promotion_type() {
                PieceType::Knight => 'n',
                PieceType::Bishop => 'b',
                PieceType::Rook => 'r',
                PieceType::Queen => 'q',
                _ => unreachable!("promotion_type is one of PROMOTION_PIECES"),
            };
            out.write_char(ch)?;
        }
        Ok(())
    }

    /// Returns the UCI representation of this move.
    ///
    /// Returns `"0000"` for `Move::NONE`. Castling destinations are mapped to
    /// `g1`/`c1`/`g8`/`c8`, and promotion moves append `n`/`b`/`r`/`q`. This
    /// allocates exactly once; use [`write_uci`](Self::write_uci) to avoid the
    /// allocation.
    #[must_use]
    pub fn to_uci(self) -> String {
        let mut s = String::with_capacity(5);
        self.write_uci(&mut s)
            .expect("writing to a String cannot fail");
        s
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self).write_uci(f)
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

    /// Clears the list, resetting the length to zero.
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
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
        assert_eq!(wp.color(), Some(Color::White));
        assert_eq!(wp.type_of(), Some(PieceType::Pawn));

        let bk = make_piece(Color::Black, PieceType::Commoner);
        assert_eq!(bk.color(), Some(Color::Black));
        assert_eq!(bk.type_of(), Some(PieceType::Commoner));
    }

    #[test]
    fn test_piece_no_piece() {
        assert_eq!(NO_PIECE.color(), None);
        assert_eq!(NO_PIECE.type_of(), None);
    }

    #[test]
    fn test_commoner_is_not_a_promotion_piece() {
        // Atomic chess has no promotion to commoner; each side keeps its
        // original commoner for the whole game.
        assert!(!PROMOTION_PIECES.contains(&PieceType::Commoner));
    }

    #[test]
    fn test_commoner_ascii_char_is_standard() {
        // The commoner is the king-like piece of atomic chess and must use the
        // standard FEN letter K/k (not the non-standard C/c).
        assert_eq!(W_COMMONER.ascii_char(), 'K');
        assert_eq!(B_COMMONER.ascii_char(), 'k');
    }

    #[test]
    fn test_move_helpers() {
        let m = Move::make_move(Square::A2, Square::A4);
        assert!(!m.is_castling());
        assert!(!m.is_promotion());
        assert!(!m.is_en_passant());

        let m = Move::make_castling(Square::E1, Square::H1);
        assert!(m.is_castling());

        let m = Move::make_enpassant(Square::C5, Square::D6);
        assert!(m.is_en_passant());

        let m = Move::make_promotion(Square::A7, Square::A8, PieceType::Queen);
        assert!(m.is_promotion());
    }

    #[test]
    fn test_move_to_uci() {
        assert_eq!(Move::NONE.to_uci(), "0000");

        let m = Move::make_move(Square::E2, Square::E4);
        assert_eq!(m.to_uci(), "e2e4");

        let m = Move::make_castling(Square::E1, Square::H1);
        assert_eq!(m.to_uci(), "e1g1");

        let m = Move::make_castling(Square::E8, Square::A8);
        assert_eq!(m.to_uci(), "e8c8");

        let m = Move::make_promotion(Square::A7, Square::A8, PieceType::Queen);
        assert_eq!(m.to_uci(), "a7a8q");

        let m = Move::make_promotion(Square::H2, Square::H1, PieceType::Knight);
        assert_eq!(m.to_uci(), "h2h1n");
    }

    #[test]
    fn move_bits_roundtrip_exhaustive() {
        assert_eq!(Move::from_bits(0), Some(Move::NONE));

        for bits in 0..=u16::MAX {
            let from = (bits >> 6) & 0x3f;
            let to = bits & 0x3f;
            let structurally_valid = bits == 0 || from != to;
            let decoded = Move::from_bits(bits);
            assert_eq!(
                decoded.is_some(),
                structurally_valid,
                "from_bits({bits:#06x}) validity"
            );
            if let Some(m) = decoded {
                assert_eq!(m.to_bits(), bits, "roundtrip for {bits:#06x}");
            }
        }

        // `a1a1` and `a2a2`-style codes are rejected; zero is the only
        // special sentinel.
        assert_eq!(Move::from_bits(0), Some(Move::NONE));
        assert_eq!(Move::from_bits(8 | (8 << 6)), None);
        assert_eq!(Move::from_bits((1 << 12) | (8 | (8 << 6))), None);
    }

    #[test]
    fn move_bits_stable_golden() {
        // Hard-coded wire values pin the layout and PROMOTION_PIECES order.
        let cases = [
            (Move::make_move(Square::E2, Square::E4), 796),
            (Move::make_castling(Square::E1, Square::H1), 12_551),
            (Move::make_castling(Square::E1, Square::A1), 12_544),
            (Move::make_castling(Square::E8, Square::H8), 16_191),
            (Move::make_castling(Square::E8, Square::A8), 16_184),
            (Move::make_enpassant(Square::C5, Square::D6), 10_411),
            (
                Move::make_promotion(Square::A7, Square::A8, PieceType::Queen),
                7_224,
            ),
            (
                Move::make_promotion(Square::A7, Square::A8, PieceType::Rook),
                23_608,
            ),
            (
                Move::make_promotion(Square::A7, Square::A8, PieceType::Bishop),
                39_992,
            ),
            (
                Move::make_promotion(Square::A7, Square::A8, PieceType::Knight),
                56_376,
            ),
        ];

        for (m, expected_bits) in cases {
            assert_eq!(m.to_bits(), expected_bits, "golden bits for {m}");
            assert_eq!(Move::from_bits(expected_bits), Some(m));
        }
        assert_eq!(Move::NONE.to_bits(), 0);
    }

    #[test]
    fn move_uci_parse_format() {
        assert_eq!(
            Move::from_uci("e2e4"),
            Some(Move::make_move(Square::E2, Square::E4))
        );
        assert_eq!(Move::from_uci("0000"), Some(Move::NONE));

        // Syntax-only: castling text is an ordinary two-square move here.
        assert_eq!(
            Move::from_uci("e1g1"),
            Some(Move::make_move(Square::E1, Square::G1))
        );
        assert_eq!(
            Move::from_uci("e8c8"),
            Some(Move::make_move(Square::E8, Square::C8))
        );

        let promotions = [
            ("a7a8q", PieceType::Queen),
            ("a7a8r", PieceType::Rook),
            ("a7a8b", PieceType::Bishop),
            ("a7a8n", PieceType::Knight),
        ];
        for (text, pt) in promotions {
            let parsed = Move::from_uci(text).expect("promotion must parse");
            assert_eq!(parsed.promotion_type(), pt);
            assert_eq!(parsed.to_uci(), text);
        }

        for malformed in [
            "", "e2e", "e2e44", "e2e9", "i2e4", "e2e2", "a1a1", "00000", "e2e4 ", "e7e8Q", "a7a8k",
            "a7a8p",
        ] {
            assert_eq!(Move::from_uci(malformed), None, "parsed {malformed:?}");
        }
    }

    #[test]
    fn move_display_alloc_free() {
        let moves = [
            Move::NONE,
            Move::make_move(Square::E2, Square::E4),
            Move::make_castling(Square::E1, Square::H1),
            Move::make_castling(Square::E8, Square::A8),
            Move::make_enpassant(Square::C5, Square::D6),
            Move::make_promotion(Square::A7, Square::A8, PieceType::Queen),
            Move::make_promotion(Square::H2, Square::H1, PieceType::Knight),
        ];

        for m in moves {
            let mut written = String::new();
            m.write_uci(&mut written)
                .expect("String writer cannot fail");
            assert_eq!(written, m.to_uci());
            assert_eq!(format!("{m}"), m.to_uci());
        }
        assert_eq!(format!("{}", Move::NONE), "0000");
    }

    #[test]
    fn move_ord_hash() {
        let moves = [
            Move::make_promotion(Square::A7, Square::A8, PieceType::Knight),
            Move::make_move(Square::E2, Square::E4),
            Move::make_castling(Square::E1, Square::H1),
            Move::make_move(Square::A2, Square::A4),
            Move::NONE,
        ];

        for &a in &moves {
            for &b in &moves {
                assert_eq!(
                    a.cmp(&b),
                    a.to_bits().cmp(&b.to_bits()),
                    "ordering must follow the wire encoding"
                );
            }
        }

        let mut set = std::collections::HashSet::new();
        set.insert(Move::make_move(Square::E2, Square::E4));
        set.insert(Move::make_move(Square::E2, Square::E4));
        set.insert(Move::make_move(Square::A2, Square::A4));
        assert_eq!(set.len(), 2);

        let mut sorted = moves;
        sorted.sort();
        assert!(sorted.windows(2).all(|w| w[0].to_bits() <= w[1].to_bits()));
    }
}
