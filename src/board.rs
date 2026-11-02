//! Board representation, FEN parsing, move make/unmake, and legality checking
//! for atomic chess.

use crate::attacks;
use crate::bitboard::*;
use crate::types::*;
use std::error::Error;
use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

/// Errors that can occur when parsing a FEN string.
#[derive(Debug)]
pub enum FenError {
    /// Fewer than 4 space-separated fields.
    TooShort {
        /// Number of fields actually found.
        parts: usize,
    },
    /// The board section does not contain exactly 8 ranks.
    WrongRankCount {
        /// Expected number of ranks (always 8).
        expected: u8,
        /// Number of ranks found.
        got: usize,
    },
    /// Invalid side-to-move field (expected `"w"` or `"b"`).
    InvalidSideToMove(String),
    /// Invalid castling rights field.
    InvalidCastling(String),
    /// Invalid en-passant target square.
    InvalidEpSquare(String),
    /// Invalid piece placement in the FEN board section.
    InvalidPlacement(String),
    /// FEN does not have exactly 4 or 6 space-separated fields.
    InvalidFieldCount {
        /// Number of fields actually found.
        got: usize,
    },
    /// Integer parse failure in the move-clock or full-move fields.
    ParseInt(String),
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenError::TooShort { parts } => {
                write!(f, "FEN too short: expected at least 4 parts, got {parts}")
            }
            FenError::WrongRankCount { expected, got } => {
                write!(f, "Expected {expected} ranks in FEN, got {got}")
            }
            FenError::InvalidSideToMove(v) => write!(f, "Invalid side to move: {v}"),
            FenError::InvalidCastling(v) => write!(f, "Invalid castling rights: {v}"),
            FenError::InvalidEpSquare(v) => write!(f, "Invalid en-passant square: {v}"),
            FenError::InvalidPlacement(v) => write!(f, "Invalid piece placement: {v}"),
            FenError::InvalidFieldCount { got } => {
                write!(f, "FEN must have 4 or 6 fields, got {got}")
            }
            FenError::ParseInt(v) => write!(f, "Failed to parse integer: {v}"),
        }
    }
}

impl Error for FenError {}

/// Semantic validation failures for a position.
///
/// This is a small, dependency-free bitflags type. Multiple problems can be
/// reported at once; use [`contains`](Self::contains) to test for a specific
/// flag. The flags describe structural problems, not whether a position is
/// reachable from the starting position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValidationErrors(u32);

impl ValidationErrors {
    /// The board has no pieces at all.
    pub const EMPTY_BOARD: Self = Self(1 << 0);
    /// White has no commoner.
    pub const MISSING_COMMONER_WHITE: Self = Self(1 << 1);
    /// Black has no commoner.
    pub const MISSING_COMMONER_BLACK: Self = Self(1 << 2);
    /// White has more than one commoner.
    pub const TOO_MANY_COMMONERS_WHITE: Self = Self(1 << 3);
    /// Black has more than one commoner.
    pub const TOO_MANY_COMMONERS_BLACK: Self = Self(1 << 4);
    /// A pawn is on rank 1 or rank 8.
    pub const PAWNS_ON_BACKRANK: Self = Self(1 << 5);
    /// The side that just moved is in check.
    pub const OPPOSITE_CHECK: Self = Self(1 << 6);
    /// The stored en-passant target is not consistent with a double push.
    pub const INVALID_EP_SQUARE: Self = Self(1 << 7);
    /// A castling right is set without the required commoner and rook.
    pub const INVALID_CASTLING_RIGHTS: Self = Self(1 << 8);
    /// The material exceeds the conservative legal-start-plus-promotions bound.
    pub const TOO_MUCH_MATERIAL: Self = Self(1 << 9);

    /// Return an empty error set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Return the raw bitmask.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Return `true` when no flags are set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Return `true` when every flag in `other` is set in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Return the union of two error sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Return the intersection of two error sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl BitOr for ValidationErrors {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitAnd for ValidationErrors {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        self.intersection(rhs)
    }
}

impl BitOrAssign for ValidationErrors {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

impl BitAndAssign for ValidationErrors {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = self.intersection(rhs);
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [(ValidationErrors, &str); 10] = [
            (ValidationErrors::EMPTY_BOARD, "EMPTY_BOARD"),
            (
                ValidationErrors::MISSING_COMMONER_WHITE,
                "MISSING_COMMONER_WHITE",
            ),
            (
                ValidationErrors::MISSING_COMMONER_BLACK,
                "MISSING_COMMONER_BLACK",
            ),
            (
                ValidationErrors::TOO_MANY_COMMONERS_WHITE,
                "TOO_MANY_COMMONERS_WHITE",
            ),
            (
                ValidationErrors::TOO_MANY_COMMONERS_BLACK,
                "TOO_MANY_COMMONERS_BLACK",
            ),
            (ValidationErrors::PAWNS_ON_BACKRANK, "PAWNS_ON_BACKRANK"),
            (ValidationErrors::OPPOSITE_CHECK, "OPPOSITE_CHECK"),
            (ValidationErrors::INVALID_EP_SQUARE, "INVALID_EP_SQUARE"),
            (
                ValidationErrors::INVALID_CASTLING_RIGHTS,
                "INVALID_CASTLING_RIGHTS",
            ),
            (ValidationErrors::TOO_MUCH_MATERIAL, "TOO_MUCH_MATERIAL"),
        ];

        if self.is_empty() {
            return f.write_str("no validation errors");
        }
        let mut first = true;
        for (flag, name) in NAMES {
            if self.contains(flag) {
                if !first {
                    f.write_str(", ")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        Ok(())
    }
}

impl Error for ValidationErrors {}

/// Policy for semantic validation when loading a FEN with
/// [`Board::from_fen_with`].
///
/// The default [`Board::from_fen`] parser is syntax-only and remains
/// permissive. These policies add opt-in semantic checks for untrusted input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FenPolicy {
    /// All semantic checks; exactly one commoner per side.
    Strict,
    /// Permit zero or one commoner per side for artificial terminal fixtures.
    ///
    /// Multiple commoners are still rejected, and all other checks still run.
    AllowTerminal,
    /// Strict except for the side-not-to-move check.
    ///
    /// This permits positions where the side that just moved is in check, which
    /// can be useful for puzzle or analysis fixtures.
    AllowImpossibleCheck,
    /// Syntax only, exactly like [`Board::from_fen`].
    Permissive,
}

/// Error returned by [`Board::from_fen_with`].
///
/// Syntax errors keep the existing [`FenError`] type; semantic failures are
/// reported separately so downstream code can distinguish malformed FEN text
/// from a well-formed but invalid position.
#[derive(Debug)]
pub enum FenLoadError {
    /// The FEN text is malformed.
    Syntax(FenError),
    /// The FEN text is well formed, but the position failed validation.
    InvalidPosition(ValidationErrors),
}

impl fmt::Display for FenLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenLoadError::Syntax(e) => write!(f, "invalid FEN syntax: {e}"),
            FenLoadError::InvalidPosition(e) => write!(f, "invalid position: {e}"),
        }
    }
}

impl Error for FenLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FenLoadError::Syntax(e) => Some(e),
            FenLoadError::InvalidPosition(_) => None,
        }
    }
}

impl From<FenError> for FenLoadError {
    fn from(value: FenError) -> Self {
        FenLoadError::Syntax(value)
    }
}

fn char_to_piece(c: char) -> Option<Piece> {
    match c {
        'P' => Some(W_PAWN),
        'N' => Some(W_KNIGHT),
        'B' => Some(W_BISHOP),
        'R' => Some(W_ROOK),
        'Q' => Some(W_QUEEN),
        'K' => Some(W_COMMONER),
        'p' => Some(B_PAWN),
        'n' => Some(B_KNIGHT),
        'b' => Some(B_BISHOP),
        'r' => Some(B_ROOK),
        'q' => Some(B_QUEEN),
        'k' => Some(B_COMMONER),
        _ => None,
    }
}

/// Cached state for a position, used during move generation and legality checks.
///
/// Fields are populated by [`Board::populate_state`] and consumed by
/// [`Board::legal`] and [`generate_legal`](crate::movegen::generate_legal).
#[derive(Debug, Clone, Copy)]
pub struct StateInfo {
    /// Enemy pieces attacking the side to move's commoner (hot).
    pub checkers: Bitboard,
    /// Pieces of the side to move that are pinned to a commoner (hot).
    pub pinned: Bitboard,
    /// Number of commoners the side to move has on the board (hot). In a legal
    /// atomic position this is 0 (game over) or 1.
    pub commoners_count: u32,
    /// Number of commoners the opponent has on the board (hot). In a legal
    /// atomic position this is 0 (game over) or 1.
    pub them_commoners_count: u32,

    /// Castling rights before the move (undo data).
    pub castling_rights: u8,
    /// En-passant target before the move (undo data).
    pub ep_square: Option<Square>,
    /// Half-move clock before the move (undo data).
    pub rule50: u16,
    /// Number of pieces removed by a blast during `do_move` (undo data).
    pub captured_count: u8,
    /// Blast-removed pieces stored for `undo_move` (undo data).
    pub captured: [(Square, Piece); 9],
    /// The square of the main piece captured during `do_move`, if any.
    pub cap_sq: Option<Square>,
    /// The piece captured on `cap_sq`.
    pub cap_piece: Piece,
    /// The piece type of the piece captured on `cap_sq`.
    pub cap_pt: PieceType,
    /// Zobrist hash before the move (undo data).
    pub hash: u64,
}

impl Default for StateInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl StateInfo {
    /// Create a new `StateInfo` with all fields zeroed/empty.
    pub fn new() -> Self {
        StateInfo {
            checkers: Bitboard::EMPTY,
            pinned: Bitboard::EMPTY,
            commoners_count: 0,
            them_commoners_count: 0,
            castling_rights: 0,
            ep_square: None,
            rule50: 0,
            captured_count: 0,
            captured: [(Square::NONE, NO_PIECE); 9],
            cap_sq: None,
            cap_piece: NO_PIECE,
            cap_pt: PieceType::Pawn,
            hash: 0,
        }
    }
}

/// A chessboard with atomic chess rules.
///
/// Maintains piece placement, side-to-move, castling rights, en-passant
/// square, and the half-move clock. Supports FEN serialization/deserialization,
/// making and unmaking moves, and legality checking.
#[derive(Debug, Clone)]
pub struct Board {
    squares: [Piece; 64],
    by_color: [Bitboard; 2],
    by_type: [Bitboard; 6],
    side_to_move: Color,
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u16,
    game_ply: u16,
    hash: u64,
}

pub(crate) const WK_CASTLE: u8 = 1;
pub(crate) const WQ_CASTLE: u8 = 2;
pub(crate) const BK_CASTLE: u8 = 4;
pub(crate) const BQ_CASTLE: u8 = 8;

/// Squares on rank 1 or rank 8. A quiet move that touches neither rank can
/// never change the castling rights, which lets [`Board::hash_after`] skip
/// the rights computation entirely.
const BB_FIRST_LAST_RANK: Bitboard = Bitboard(0xFF00_0000_0000_00FF);

/// The six castling-relevant home squares A1, E1, H1, A8, E8, H8. A capture
/// whose blast set (plus origin) touches none of them leaves every piece the
/// post-blast castling cascade inspects in place, so the rights cannot change
/// beyond the origin/to-square rule, which the intersection already covers.
const BB_CASTLE_SQUARES: Bitboard = Bitboard(
    (1u64 << Square::A1 as u64)
        | (1u64 << Square::E1 as u64)
        | (1u64 << Square::H1 as u64)
        | (1u64 << Square::A8 as u64)
        | (1u64 << Square::E8 as u64)
        | (1u64 << Square::H8 as u64),
);

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    /// Create a board in the standard starting position.
    pub fn new() -> Self {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        Board::from_fen(fen).expect("Failed to create starting position")
    }

    /// Parse a board from a FEN string.
    ///
    /// Accepts standard FEN with 4 or 6 space-separated fields. The piece
    /// character set is the standard chess set (`P`, `N`, `B`, `R`, `Q`, `K`
    /// and their lower-case forms). The commoner — the king-like piece of
    /// atomic chess — uses the standard FEN letter `K`/`k`; the non-standard
    /// `C`/`c` labels are rejected.
    ///
    /// Parsing is permissive about game-state constraints: it does not reject
    /// missing commoners, pawns on the back rank, or invalid en-passant
    /// positions. In particular, atomic chess has at most one commoner per
    /// side, so FENs with more than one `K` or more than one `k` are not legal
    /// atomic positions even though this parser accepts them.
    ///
    /// For untrusted input, use [`Board::from_fen_with`] with
    /// [`FenPolicy::Strict`], or call [`Board::validate`] after parsing. This
    /// method's behavior is unchanged and remains permissive.
    ///
    /// # Errors
    ///
    /// Returns a [`FenError`] variant for malformed input:
    ///
    /// - `TooShort` — fewer than 4 fields.
    /// - `InvalidFieldCount` — not 4 or 6 fields.
    /// - `WrongRankCount` — the board section does not contain 8 ranks.
    /// - `InvalidPlacement` — a rank does not sum to 8 columns, an invalid
    ///   character appears, or a piece is placed outside the board.
    /// - `InvalidSideToMove` — the side-to-move field is not `w` or `b`.
    /// - `InvalidCastling` — the castling field is malformed or a castling
    ///   right is set without the required pieces on the board.
    /// - `InvalidEpSquare` — the en-passant target is not a valid square, is
    ///   on the wrong rank, or has no enemy pawn behind it.
    /// - `ParseInt` — the half-move clock or full-move number is not a valid
    ///   integer.
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(FenError::TooShort { parts: parts.len() });
        }
        if parts.len() != 4 && parts.len() != 6 {
            return Err(FenError::InvalidFieldCount { got: parts.len() });
        }

        let mut squares = [NO_PIECE; 64];
        let mut by_color = [Bitboard::EMPTY; 2];
        let mut by_type = [Bitboard::EMPTY; 6];

        // FEN piece placement: rank 8 to rank 1, left to right
        // Our board indices: rank 1 = 0-7, rank 2 = 8-15, ..., rank 8 = 56-63
        let rows: Vec<&str> = parts[0].split('/').collect();
        if rows.len() != 8 {
            return Err(FenError::WrongRankCount {
                expected: 8,
                got: rows.len(),
            });
        }
        for (ri, row) in rows.iter().enumerate() {
            let rank_idx = 7 - ri; // rank 0 (index 7) = rank 8 in FEN
            let fen_rank = 8 - ri; // FEN rank number for error messages
            let mut col = 0usize;
            for c in row.chars() {
                if c == '0' {
                    return Err(FenError::InvalidPlacement(format!(
                        "'0' is not a valid empty-square count at rank {fen_rank}, col {col}"
                    )));
                }
                if c.is_ascii_digit() {
                    let digit = c.to_digit(10).unwrap() as usize;
                    if col + digit > 8 {
                        return Err(FenError::InvalidPlacement(format!(
                            "digit {c} at rank {fen_rank}, col {col} would exceed 8 columns"
                        )));
                    }
                    col += digit;
                } else if let Some(piece) = char_to_piece(c) {
                    if col >= 8 {
                        return Err(FenError::InvalidPlacement(format!(
                            "piece '{c}' at rank {fen_rank} is placed past column 8"
                        )));
                    }
                    let sq_idx = rank_idx * 8 + col;
                    let sq = Square::from_u8(sq_idx as u8);
                    squares[sq_idx] = piece;
                    let bb = Bitboard::square_bb(sq);
                    by_color[piece.color().unwrap() as usize] =
                        by_color[piece.color().unwrap() as usize] | bb;
                    by_type[piece.type_of().unwrap() as usize] =
                        by_type[piece.type_of().unwrap() as usize] | bb;
                    col += 1;
                } else {
                    return Err(FenError::InvalidPlacement(format!(
                        "invalid character '{c}' at rank {fen_rank}, col {col}"
                    )));
                }
            }
            if col != 8 {
                return Err(FenError::InvalidPlacement(format!(
                    "rank {fen_rank} has {col} columns, expected 8"
                )));
            }
        }

        let side_to_move = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => {
                return Err(FenError::InvalidSideToMove(parts[1].to_string()));
            }
        };

        let mut castling_rights = 0u8;
        for c in parts[2].chars() {
            match c {
                'K' => castling_rights |= WK_CASTLE,
                'Q' => castling_rights |= WQ_CASTLE,
                'k' => castling_rights |= BK_CASTLE,
                'q' => castling_rights |= BQ_CASTLE,
                '-' => break,
                _ => {
                    return Err(FenError::InvalidCastling(c.to_string()));
                }
            }
        }

        // Validate castling rights against the actual king/commoner and rook squares.
        if castling_rights & WK_CASTLE != 0
            && (squares[Square::E1 as usize] != make_piece(Color::White, PieceType::Commoner)
                || squares[Square::H1 as usize] != make_piece(Color::White, PieceType::Rook))
        {
            return Err(FenError::InvalidCastling(
                "white king-side castling rights without required pieces".to_string(),
            ));
        }
        if castling_rights & WQ_CASTLE != 0
            && (squares[Square::E1 as usize] != make_piece(Color::White, PieceType::Commoner)
                || squares[Square::A1 as usize] != make_piece(Color::White, PieceType::Rook))
        {
            return Err(FenError::InvalidCastling(
                "white queen-side castling rights without required pieces".to_string(),
            ));
        }
        if castling_rights & BK_CASTLE != 0
            && (squares[Square::E8 as usize] != make_piece(Color::Black, PieceType::Commoner)
                || squares[Square::H8 as usize] != make_piece(Color::Black, PieceType::Rook))
        {
            return Err(FenError::InvalidCastling(
                "black king-side castling rights without required pieces".to_string(),
            ));
        }
        if castling_rights & BQ_CASTLE != 0
            && (squares[Square::E8 as usize] != make_piece(Color::Black, PieceType::Commoner)
                || squares[Square::A8 as usize] != make_piece(Color::Black, PieceType::Rook))
        {
            return Err(FenError::InvalidCastling(
                "black queen-side castling rights without required pieces".to_string(),
            ));
        }

        let ep_square = if parts[3] == "-" {
            None
        } else {
            let sq = crate::types::parse_sq(parts[3])
                .ok_or_else(|| FenError::InvalidEpSquare(parts[3].to_string()))?;
            let expected_rank = match side_to_move {
                Color::White => Rank::R6,
                Color::Black => Rank::R3,
            };
            if rank_of(sq) != expected_rank {
                return Err(FenError::InvalidEpSquare(format!(
                    "{} on rank {:?} is not a valid en-passant square for {:?}",
                    parts[3],
                    rank_of(sq),
                    side_to_move
                )));
            }
            let ep_cap_idx = match side_to_move {
                Color::White => sq as i8 - 8,
                Color::Black => sq as i8 + 8,
            };
            let ep_cap = Square::from_index(ep_cap_idx);
            let them = side_to_move.flip();
            if squares[ep_cap as usize] != make_piece(them, PieceType::Pawn) {
                return Err(FenError::InvalidEpSquare(format!(
                    "no {} pawn on the square behind {} for en-passant",
                    if them == Color::White {
                        "white"
                    } else {
                        "black"
                    },
                    parts[3]
                )));
            }
            Some(sq)
        };

        let rule50 = if parts.len() > 4 {
            parts[4]
                .parse::<u16>()
                .map_err(|e| FenError::ParseInt(e.to_string()))?
        } else {
            0
        };

        let game_ply = if parts.len() > 5 {
            let fm = parts[5]
                .parse::<u16>()
                .map_err(|e| FenError::ParseInt(e.to_string()))?;
            if fm == 0 {
                return Err(FenError::ParseInt(parts[5].to_string()));
            }
            2 * (fm - 1) + if side_to_move == Color::White { 1 } else { 2 }
        } else {
            if side_to_move == Color::White { 1 } else { 2 }
        };

        let mut board = Board {
            squares,
            by_color,
            by_type,
            side_to_move,
            castling_rights,
            ep_square,
            rule50,
            game_ply,
            hash: 0,
        };
        board.recompute_hash();
        Ok(board)
    }

    /// Parse a FEN string and apply the given semantic validation policy.
    ///
    /// This is the opt-in strict counterpart to [`Board::from_fen`]. The
    /// permissive parser is still used for syntax; after parsing, the position
    /// is checked according to `policy`. Use [`FenPolicy::Strict`] for
    /// untrusted input, [`FenPolicy::AllowTerminal`] for artificial terminal
    /// fixtures, [`FenPolicy::AllowImpossibleCheck`] to skip only the
    /// side-not-to-move check, or [`FenPolicy::Permissive`] for behavior
    /// identical to [`Board::from_fen`].
    ///
    /// # Errors
    ///
    /// Returns [`FenLoadError::Syntax`] for malformed FEN text and
    /// [`FenLoadError::InvalidPosition`] when the parsed position fails the
    /// selected policy.
    pub fn from_fen_with(fen: &str, policy: FenPolicy) -> Result<Self, FenLoadError> {
        let board = Board::from_fen(fen).map_err(FenLoadError::Syntax)?;
        let errors = board.validation_errors(policy);
        if errors.is_empty() {
            Ok(board)
        } else {
            Err(FenLoadError::InvalidPosition(errors))
        }
    }

    /// Validate this position with [`FenPolicy::Strict`].
    ///
    /// The checks are structural and conservative: exactly one commoner per
    /// side, no pawns on the back ranks, consistent castling rights and
    /// en-passant geometry, a conservative material bound, and no check against
    /// the side that just moved. They do **not** prove that the position is
    /// reachable from the starting position.
    ///
    /// # Errors
    ///
    /// Returns the accumulated [`ValidationErrors`] when any check fails.
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let errors = self.validation_errors(FenPolicy::Strict);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Accumulate semantic validation errors for `policy`.
    fn validation_errors(&self, policy: FenPolicy) -> ValidationErrors {
        if policy == FenPolicy::Permissive {
            return ValidationErrors::empty();
        }

        let mut errors = ValidationErrors::empty();

        // 1. Board presence and commoner counts.
        if self.occupied().is_empty() && policy != FenPolicy::AllowTerminal {
            errors |= ValidationErrors::EMPTY_BOARD;
        }
        let white_commoners = self.commoners(Color::White).count();
        let black_commoners = self.commoners(Color::Black).count();
        match policy {
            FenPolicy::Strict | FenPolicy::AllowImpossibleCheck => {
                if white_commoners == 0 {
                    errors |= ValidationErrors::MISSING_COMMONER_WHITE;
                } else if white_commoners > 1 {
                    errors |= ValidationErrors::TOO_MANY_COMMONERS_WHITE;
                }
                if black_commoners == 0 {
                    errors |= ValidationErrors::MISSING_COMMONER_BLACK;
                } else if black_commoners > 1 {
                    errors |= ValidationErrors::TOO_MANY_COMMONERS_BLACK;
                }
            }
            FenPolicy::AllowTerminal => {
                if white_commoners > 1 {
                    errors |= ValidationErrors::TOO_MANY_COMMONERS_WHITE;
                }
                if black_commoners > 1 {
                    errors |= ValidationErrors::TOO_MANY_COMMONERS_BLACK;
                }
            }
            FenPolicy::Permissive => unreachable!(),
        }

        // 2. Pawns on the back ranks.
        const RANK_1: Bitboard = Bitboard(0x0000_0000_0000_00FF);
        const RANK_8: Bitboard = Bitboard(0xFF00_0000_0000_0000);
        if !(self.pieces_pt(PieceType::Pawn) & (RANK_1 | RANK_8)).is_empty() {
            errors |= ValidationErrors::PAWNS_ON_BACKRANK;
        }

        // 3. Castling rights must have the required commoner and rook.
        let white_commoner = make_piece(Color::White, PieceType::Commoner);
        let white_rook = make_piece(Color::White, PieceType::Rook);
        let black_commoner = make_piece(Color::Black, PieceType::Commoner);
        let black_rook = make_piece(Color::Black, PieceType::Rook);
        if self.castling_rights & WK_CASTLE != 0
            && (self.squares[Square::E1 as usize] != white_commoner
                || self.squares[Square::H1 as usize] != white_rook)
        {
            errors |= ValidationErrors::INVALID_CASTLING_RIGHTS;
        }
        if self.castling_rights & WQ_CASTLE != 0
            && (self.squares[Square::E1 as usize] != white_commoner
                || self.squares[Square::A1 as usize] != white_rook)
        {
            errors |= ValidationErrors::INVALID_CASTLING_RIGHTS;
        }
        if self.castling_rights & BK_CASTLE != 0
            && (self.squares[Square::E8 as usize] != black_commoner
                || self.squares[Square::H8 as usize] != black_rook)
        {
            errors |= ValidationErrors::INVALID_CASTLING_RIGHTS;
        }
        if self.castling_rights & BQ_CASTLE != 0
            && (self.squares[Square::E8 as usize] != black_commoner
                || self.squares[Square::A8 as usize] != black_rook)
        {
            errors |= ValidationErrors::INVALID_CASTLING_RIGHTS;
        }

        // 4. Conservative material bounds: a legal start plus promotions.
        for color in [Color::White, Color::Black] {
            let pawns = self.pieces_color_pt(color, PieceType::Pawn).count();
            let total = self.pieces_color(color).count();
            if pawns > 8 || total > 16 {
                errors |= ValidationErrors::TOO_MUCH_MATERIAL;
            }
            let free = 8u32.saturating_sub(pawns);
            const INITIAL_COUNTS: [(PieceType, u32); 5] = [
                (PieceType::Knight, 2),
                (PieceType::Bishop, 2),
                (PieceType::Rook, 2),
                (PieceType::Queen, 1),
                (PieceType::Commoner, 1),
            ];
            for (pt, initial) in INITIAL_COUNTS {
                if self.pieces_color_pt(color, pt).count() > initial + free {
                    errors |= ValidationErrors::TOO_MUCH_MATERIAL;
                }
            }
        }

        // 5. En-passant geometry must be consistent with a double push. A
        // legal ep capture is deliberately not required: FSF records the
        // target after every double push.
        if let Some(sq) = self.ep_square {
            let mut valid = true;
            let expected_rank = match self.side_to_move {
                Color::White => Rank::R6,
                Color::Black => Rank::R3,
            };
            if rank_of(sq) != expected_rank {
                valid = false;
            } else {
                let them = self.side_to_move.flip();
                let (pawn_sq, origin_sq) = match self.side_to_move {
                    Color::White => (
                        Square::from_index(sq as i8 - 8),
                        Square::from_index(sq as i8 + 8),
                    ),
                    Color::Black => (
                        Square::from_index(sq as i8 + 8),
                        Square::from_index(sq as i8 - 8),
                    ),
                };
                if self.squares[pawn_sq as usize] != make_piece(them, PieceType::Pawn)
                    || self.squares[origin_sq as usize] != NO_PIECE
                    || self.squares[sq as usize] != NO_PIECE
                {
                    valid = false;
                }
            }
            if !valid {
                errors |= ValidationErrors::INVALID_EP_SQUARE;
            }
        }

        // 6. The side that just moved must not be in check.
        if policy != FenPolicy::AllowImpossibleCheck {
            let just_moved = self.side_to_move.flip();
            if !self.compute_checkers(just_moved).is_empty() {
                errors |= ValidationErrors::OPPOSITE_CHECK;
            }
        }

        errors
    }

    /// Serialize the board to a FEN string using the default
    /// [`EnPassantMode::Always`] policy.
    ///
    /// This matches Fairy-Stockfish and prints the stored en-passant target
    /// after every double push, even when no legal en-passant capture exists.
    /// Use [`fen_with`](Self::fen_with) for canonicalized output.
    #[must_use]
    pub fn fen(&self) -> String {
        self.write_fen(self.ep_square)
    }

    /// Serialize the board to a FEN string with the selected en-passant policy.
    ///
    /// [`EnPassantMode::Always`] is identical to [`fen`](Self::fen) and prints
    /// the stored target. [`EnPassantMode::Legal`] prints `-` unless at least
    /// one fully legal en-passant capture exists, which makes positions reached
    /// by different move orders serialize identically.
    #[must_use]
    pub fn fen_with(&self, mode: EnPassantMode) -> String {
        match mode {
            EnPassantMode::Always => self.write_fen(self.ep_square),
            EnPassantMode::Legal => self.write_fen(self.legal_ep_square()),
        }
    }

    /// Shared FEN writer; `ep_square` is the policy-resolved target field.
    fn write_fen(&self, ep_square: Option<Square>) -> String {
        let mut fen = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                let idx = rank * 8 + file;
                if self.squares[idx] == NO_PIECE {
                    empty += 1;
                } else {
                    if empty > 0 {
                        fen.push_str(&empty.to_string());
                        empty = 0;
                    }
                    fen.push(self.squares[idx].ascii_char());
                }
            }
            if empty > 0 {
                fen.push_str(&empty.to_string());
            }
            if rank > 0 {
                fen.push('/');
            }
        }

        fen.push(' ');
        fen.push(match self.side_to_move {
            Color::White => 'w',
            Color::Black => 'b',
        });

        fen.push(' ');
        let mut has_castling = false;
        if self.castling_rights & WK_CASTLE != 0 {
            fen.push('K');
            has_castling = true;
        }
        if self.castling_rights & WQ_CASTLE != 0 {
            fen.push('Q');
            has_castling = true;
        }
        if self.castling_rights & BK_CASTLE != 0 {
            fen.push('k');
            has_castling = true;
        }
        if self.castling_rights & BQ_CASTLE != 0 {
            fen.push('q');
            has_castling = true;
        }
        if !has_castling {
            fen.push('-');
        }

        fen.push(' ');
        match ep_square {
            Some(sq) => fen.push_str(crate::types::sq_str(sq).unwrap_or("??")),
            None => fen.push('-'),
        }

        fen.push(' ');
        fen.push_str(&self.rule50.to_string());
        fen.push(' ');
        fen.push_str(&self.fullmove_number().to_string());

        fen
    }

    /// Compute the Zobrist hash for the current board state without mutating `self`.
    pub(crate) fn recomputed_hash(&self) -> u64 {
        let mut h = 0u64;
        for sq_idx in 0..64 {
            let piece = self.squares[sq_idx];
            if piece != NO_PIECE {
                let raw = piece.raw() as usize;
                h ^= crate::zobrist::ZOBRIST.piece[sq_idx][raw];
            }
        }
        if self.side_to_move == Color::Black {
            h ^= crate::zobrist::ZOBRIST.side;
        }
        h ^= crate::zobrist::ZOBRIST.castling[self.castling_rights as usize];
        if let Some(sq) = self.ep_square {
            h ^= crate::zobrist::ZOBRIST.ep[crate::types::file_of(sq) as usize];
        }
        h
    }

    /// Recompute the Zobrist hash from the current board state.
    pub(crate) fn recompute_hash(&mut self) {
        self.hash = self.recomputed_hash();
    }

    /// Return the piece on a square (or [`NO_PIECE`] if empty or `Square::NONE`).
    #[inline(always)]
    #[must_use]
    pub fn piece_on(&self, sq: Square) -> Piece {
        if sq == Square::NONE {
            return NO_PIECE;
        }
        self.squares[sq as usize]
    }

    /// Return `true` if the given square is empty.
    ///
    /// Returns `true` for [`Square::NONE`].
    #[inline]
    #[must_use]
    pub fn empty(&self, sq: Square) -> bool {
        if sq == Square::NONE {
            return true;
        }
        self.squares[sq as usize] == NO_PIECE
    }

    /// Return the bitboard of all occupied squares.
    #[inline(always)]
    #[must_use]
    pub fn occupied(&self) -> Bitboard {
        self.by_color[0] | self.by_color[1]
    }

    /// Return all pieces of a given color.
    #[inline(always)]
    pub fn pieces_color(&self, c: Color) -> Bitboard {
        self.by_color[c as usize]
    }

    /// Return all pieces of a given type.
    #[inline(always)]
    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard {
        self.by_type[pt as usize]
    }

    /// Return all pieces of a given color and type.
    #[inline(always)]
    pub fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard {
        self.by_color[c as usize] & self.by_type[pt as usize]
    }

    /// Return the side to move.
    #[inline]
    #[must_use]
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// Return the castling rights bitmask.
    #[must_use]
    pub fn castling_rights(&self) -> u8 {
        self.castling_rights
    }

    /// Return the stored en-passant target square, if any.
    ///
    /// This is the Fairy-Stockfish-style target recorded after every double
    /// pawn push. It does not imply that an en-passant capture is legal; use
    /// [`legal_ep_square`](Self::legal_ep_square) for a target that at least
    /// one legal en-passant capture actually attacks.
    #[must_use]
    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// Return the stored en-passant target, but only when at least one fully
    /// legal en-passant capture exists.
    ///
    /// This checks the mover's own pawns that attack the stored target and
    /// validates each candidate with [`legal`](Self::legal), so pins,
    /// self-explosion, and commoner attacks are all respected. It does not
    /// build a [`MoveList`]. Terminal boards and positions
    /// without an en-passant target return `None`.
    #[must_use]
    pub fn legal_ep_square(&self) -> Option<Square> {
        let ep = self.ep_square?;
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        if state.commoners_count == 0 || state.them_commoners_count == 0 {
            return None;
        }

        let us = self.side_to_move;
        let them = us.flip();
        // Attack tables are symmetric, so this is the set of squares from which
        // one of our pawns attacks the ep target.
        let mut origins =
            attacks::pawn_attacks(them, ep) & self.pieces_color_pt(us, PieceType::Pawn);
        while !origins.is_empty() {
            let from = origins.pop_lsb();
            let victim = match us {
                Color::White => Square::from_index(ep as i8 - 8),
                Color::Black => Square::from_index(ep as i8 + 8),
            };
            if self.piece_on(victim) == make_piece(them, PieceType::Pawn)
                && self.legal(Move::make_enpassant(from, ep), &state)
            {
                return Some(ep);
            }
        }
        None
    }

    /// Return the half-move clock (plies since the last pawn move or capture).
    #[inline]
    #[must_use]
    pub fn rule50(&self) -> u16 {
        self.rule50
    }

    /// Return the raw per-ply move counter (1-based).
    #[inline]
    #[must_use]
    pub fn game_ply(&self) -> u16 {
        self.game_ply
    }

    /// Return the fullmove number as it appears in FEN output.
    #[inline]
    #[must_use]
    pub fn fullmove_number(&self) -> u16 {
        1 + (self.game_ply.saturating_sub(1)) / 2
    }

    /// Return the current Zobrist hash of the position.
    ///
    /// The hash includes pieces, side to move, castling rights, and the stored
    /// en-passant file, but not the half-move clock or fullmove counter. This
    /// is the [`EnPassantMode::Always`] policy, identical to
    /// [`hash_with(EnPassantMode::Always)`](Self::hash_with).
    #[inline]
    #[must_use]
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Return the Zobrist hash with the selected en-passant policy.
    ///
    /// [`EnPassantMode::Always`] is identical to [`hash`](Self::hash).
    /// [`EnPassantMode::Legal`] removes the stored en-passant file key unless
    /// [`legal_ep_square`](Self::legal_ep_square) is `Some`, so positions that
    /// differ only by a non-capturable ep target share a key. Evaluating the
    /// legal policy is more expensive because it validates the candidate
    /// captures; the default [`hash`](Self::hash) stays a field read.
    #[inline]
    #[must_use]
    pub fn hash_with(&self, mode: EnPassantMode) -> u64 {
        match mode {
            EnPassantMode::Always => self.hash,
            EnPassantMode::Legal => match self.ep_square {
                Some(sq) if self.legal_ep_square().is_none() => {
                    self.hash ^ crate::zobrist::ZOBRIST.ep[file_of(sq) as usize]
                }
                _ => self.hash,
            },
        }
    }

    /// Zobrist hash of the position after `m`, without modifying `self`.
    ///
    /// Exactly equal to
    /// `{ let mut b = self.clone(); b.do_move(m, &mut st); b.hash() }` for
    /// every move `m` that [`do_move`](Self::do_move) accepts in this position
    /// (every pseudo-legal move). Cheaper than a `do_move`/`undo_move`
    /// round-trip: no board, bitboard, or undo-record writes. This uses the
    /// default [`EnPassantMode::Always`] policy; use
    /// [`hash_after_with`](Self::hash_after_with) to select a policy.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `from` holds no piece (i.e. `m` is not a
    /// move `do_move` would accept).
    #[must_use]
    pub fn hash_after(&self, m: Move) -> u64 {
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();

        if m.move_type() == MoveType::Castling {
            let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
            let commoner = make_piece(us, PieceType::Commoner);
            let rook = make_piece(us, PieceType::Rook);
            let mut h = self.hash
                ^ crate::zobrist::ZOBRIST.side
                ^ piece_key(kfrom, commoner)
                ^ piece_key(kto, commoner)
                ^ piece_key(rfrom, rook)
                ^ piece_key(rto, rook);
            let new_rights = self.castling_rights
                & match us {
                    Color::White => !(WK_CASTLE | WQ_CASTLE),
                    Color::Black => !(BK_CASTLE | BQ_CASTLE),
                };
            h ^= crate::zobrist::ZOBRIST.castling[self.castling_rights as usize];
            h ^= crate::zobrist::ZOBRIST.castling[new_rights as usize];
            if let Some(sq) = self.ep_square {
                h ^= crate::zobrist::ZOBRIST.ep[file_of(sq) as usize];
            }
            return h;
        }

        let piece = self.squares[from as usize];
        debug_assert!(
            piece != NO_PIECE,
            "hash_after on a move from an empty square"
        );
        // Pawn iff the low three bits of the raw encoding are `Pawn + 1`
        // (piece types are encoded as `type + 1`, pawn is variant 0). Avoids
        // the `type_of()` option/table round-trip on the hot quiet path.
        let is_pawn = piece.raw() & 7 == PieceType::Pawn as u8 + 1;
        let is_capture = !self.empty(to);
        let is_capture_or_ep = is_capture || m.move_type() == MoveType::EnPassant;

        let mut h = self.hash ^ crate::zobrist::ZOBRIST.side;
        // Occupancy and pawn bitboards, mirrored to the exact intermediate
        // state `do_move` uses when it computes the blast zone: captured
        // piece removed, capturer relocated. Only capture moves need them.
        let mut occupied = Bitboard::EMPTY;
        let mut pawns = Bitboard::EMPTY;
        let mut ep_cap_sq = Square::NONE;

        if is_capture_or_ep {
            occupied = self.occupied();
            pawns = self.by_type[PieceType::Pawn as usize];
            if m.move_type() == MoveType::EnPassant {
                let ep_cap = match us {
                    Color::White => Square::from_index(to as i8 - 8),
                    Color::Black => Square::from_index(to as i8 + 8),
                };
                let cap = self.squares[ep_cap as usize];
                h ^= piece_key(ep_cap, cap);
                let cap_bb = Bitboard::square_bb(ep_cap);
                occupied = occupied ^ cap_bb;
                pawns = pawns ^ cap_bb;
                ep_cap_sq = ep_cap;
            } else {
                let cap = self.squares[to as usize];
                h ^= piece_key(to, cap);
                occupied = occupied ^ Bitboard::square_bb(to);
                if cap.raw() & 7 == PieceType::Pawn as u8 + 1 {
                    pawns = pawns ^ Bitboard::square_bb(to);
                }
            }
        }

        let from_bb = Bitboard::square_bb(from);
        let to_bb = Bitboard::square_bb(to);
        if m.move_type() == MoveType::Promotion {
            let prom_piece = make_piece(us, m.promotion_type());
            h = h ^ piece_key(from, make_piece(us, PieceType::Pawn)) ^ piece_key(to, prom_piece);
            if is_capture_or_ep {
                pawns = pawns ^ from_bb;
            }
        } else {
            h = h ^ piece_key(from, piece) ^ piece_key(to, piece);
            if is_pawn && is_capture_or_ep {
                pawns = pawns ^ from_bb ^ to_bb;
            }
        }

        // Blast zone, identical to `do_move`: pawns (other than at ground
        // zero) are immune, and empty squares in the ring stay empty. Quiet
        // moves trigger no blast at all.
        let mover_raw = if m.move_type() == MoveType::Promotion {
            make_piece(us, m.promotion_type()).raw()
        } else {
            piece.raw()
        };
        let mut removed_bb = Bitboard::EMPTY;
        if is_capture_or_ep {
            occupied = (occupied ^ from_bb) | to_bb;
            let blast = (attacks::king_attacks(to) & !pawns & occupied) | to_bb;

            // Pieces vaporized by the blast, capturer at ground zero
            // included. Squares in the blast zone but outside `occupied`
            // were already empty before the blast and contribute nothing.
            let blasted = blast & occupied;
            // At blast time `to` holds the mover (the captured piece is
            // already gone), everywhere else the original occupant.
            let mut b = blasted;
            while !b.is_empty() {
                let bsq = b.pop_lsb();
                let raw = if bsq == to {
                    mover_raw
                } else {
                    self.squares[bsq as usize].raw()
                };
                h ^= piece_key_raw(bsq, raw);
            }
            // Every square emptied by this capture move: origin, en-passant
            // victim square, blast set. Used for the post-blast castling
            // cascade below.
            removed_bb = blasted | from_bb;
            if ep_cap_sq != Square::NONE {
                removed_bb = removed_bb | Bitboard::square_bb(ep_cap_sq);
            }
        }

        // Castling rights: the shared pure helper consumes the post-blast
        // contents of H1/A1/E1/H8/A8/E8 (only needed for captures; every
        // square emptied by a capture is in `removed_bb` — origin,
        // en-passant victim square, or blast set — everything else keeps
        // its piece).
        let new_rights = if is_capture_or_ep && removed_bb & BB_CASTLE_SQUARES != Bitboard::EMPTY {
            let post_blast = [
                if removed_bb & Bitboard::square_bb(Square::H1) != Bitboard::EMPTY {
                    NO_PIECE
                } else {
                    self.squares[Square::H1 as usize]
                },
                if removed_bb & Bitboard::square_bb(Square::A1) != Bitboard::EMPTY {
                    NO_PIECE
                } else {
                    self.squares[Square::A1 as usize]
                },
                if removed_bb & Bitboard::square_bb(Square::E1) != Bitboard::EMPTY {
                    NO_PIECE
                } else {
                    self.squares[Square::E1 as usize]
                },
                if removed_bb & Bitboard::square_bb(Square::H8) != Bitboard::EMPTY {
                    NO_PIECE
                } else {
                    self.squares[Square::H8 as usize]
                },
                if removed_bb & Bitboard::square_bb(Square::A8) != Bitboard::EMPTY {
                    NO_PIECE
                } else {
                    self.squares[Square::A8 as usize]
                },
                if removed_bb & Bitboard::square_bb(Square::E8) != Bitboard::EMPTY {
                    NO_PIECE
                } else {
                    self.squares[Square::E8 as usize]
                },
            ];
            castling_rights_after(self.castling_rights, from, to, true, &post_blast)
        } else if is_capture_or_ep {
            // No castling-relevant square touched: rights unchanged.
            self.castling_rights
        } else if (from_bb | to_bb) & BB_FIRST_LAST_RANK == Bitboard::EMPTY {
            // A quiet move touching neither back rank cannot change rights.
            self.castling_rights
        } else {
            castling_rights_after(self.castling_rights, from, to, false, &[NO_PIECE; 6])
        };
        if new_rights != self.castling_rights {
            h ^= crate::zobrist::ZOBRIST.castling[self.castling_rights as usize];
            h ^= crate::zobrist::ZOBRIST.castling[new_rights as usize];
        }

        // En-passant square transition: old file out, new file in.
        if let Some(sq) = self.ep_square {
            h ^= crate::zobrist::ZOBRIST.ep[file_of(sq) as usize];
        }
        if is_pawn && (to as i8 - from as i8).abs() == 16 {
            let new_ep = match us {
                Color::White => Square::from_index(from as i8 + 8),
                Color::Black => Square::from_index(from as i8 - 8),
            };
            h ^= crate::zobrist::ZOBRIST.ep[file_of(new_ep) as usize];
        }

        h
    }

    /// Zobrist hash of the position after `m` with the selected en-passant
    /// policy, without modifying `self`.
    ///
    /// Exactly equal to
    /// `{ let mut b = self.clone(); b.do_move(m, &mut st); b.hash_with(mode) }`
    /// for every move `m` that [`do_move`](Self::do_move) accepts. With
    /// [`EnPassantMode::Always`] this delegates to [`hash_after`](Self::hash_after)
    /// and keeps its fast path. With [`EnPassantMode::Legal`] a double pawn
    /// push clones and plays the move to evaluate the child's legal ep square;
    /// this is a convenience, not a hot-loop substitute for the default pair.
    /// Consumers that need normalized keys in a tight loop should use
    /// [`legal_ep_square`](Self::legal_ep_square) directly.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `from` holds no piece (i.e. `m` is not a
    /// move `do_move` would accept).
    #[must_use]
    pub fn hash_after_with(&self, m: Move, mode: EnPassantMode) -> u64 {
        match mode {
            EnPassantMode::Always => self.hash_after(m),
            EnPassantMode::Legal => {
                // Only a double pawn push can create an ep target.
                let is_double_push = m.move_type() == MoveType::Normal
                    && (m.to_sq() as i8 - m.from_sq() as i8).abs() == 16
                    && self
                        .piece_on(m.from_sq())
                        .type_of()
                        .is_some_and(|pt| pt == PieceType::Pawn);
                if !is_double_push {
                    return self.hash_after(m);
                }
                let mut child = self.clone();
                let mut state = StateInfo::new();
                child.do_move(m, &mut state);
                child.hash_with(EnPassantMode::Legal)
            }
        }
    }

    /// Half-move clock after `m`, without modifying `self`.
    ///
    /// Exactly equal to the [`rule50()`](Self::rule50) value
    /// [`do_move`](Self::do_move)`(m, ..)` would leave: castling increments
    /// it; otherwise a pawn move or a move onto an occupied square resets it
    /// to 0, and any other move increments it.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `from` holds no piece (i.e. `m` is not a
    /// move `do_move` would accept).
    #[must_use]
    pub fn rule50_after(&self, m: Move) -> u16 {
        if m.move_type() == MoveType::Castling {
            return self.rule50 + 1;
        }
        let piece = self.squares[m.from_sq() as usize];
        debug_assert!(
            piece != NO_PIECE,
            "rule50_after on a move from an empty square"
        );
        let pt = piece.type_of().unwrap();
        if pt == PieceType::Pawn || !self.empty(m.to_sq()) {
            0
        } else {
            self.rule50 + 1
        }
    }

    /// Return the bitboard of commoners (king-like pieces) for a color.
    ///
    /// Legal atomic positions contain at most one commoner per color; a board
    /// with only one commoner in total is terminal.
    #[inline(always)]
    #[must_use]
    pub fn commoners(&self, c: Color) -> Bitboard {
        self.pieces_color_pt(c, PieceType::Commoner)
    }

    /// Return the game outcome from the side-to-move perspective, if the
    /// position is decided under the atomic rules/WDL view.
    ///
    /// Branches are evaluated in this order:
    ///
    /// 1. the side to move has no commoner → [`Outcome::Loss`];
    /// 2. the opponent has no commoner → [`Outcome::Win`];
    /// 3. only the two commoners remain → [`Outcome::Draw`] (the
    ///    [`is_material_draw`](Self::is_material_draw) WDL shortcut);
    /// 4. no legal moves: the commoner is attacked → `Loss`, otherwise
    ///    `Draw` (stalemate);
    /// 5. the 50-move rule (`rule50 >= 100`) → `Draw`.
    ///
    /// Mate outranks the clock: branch 4 is resolved before branch 5 when the
    /// side to move is in check, so checkmate on the clock's last ply is a
    /// loss, not a draw. This matches Fairy-Stockfish's
    /// `rule50 > 99 && (!checkers() || MoveList<LEGAL>(pos).size())` ordering.
    /// When the side to move is not in check, the 50-move branch is decided
    /// without running move generation.
    ///
    /// This method populates one temporary [`StateInfo`] and uses the
    /// existence query, but never builds a [`MoveList`].
    ///
    /// # WDL view versus literal game over
    ///
    /// [`Outcome`] is the WDL view. A commoner-only board is reported as a
    /// draw here even before the 50-move rule has elapsed, because no legal
    /// move can change the material. Use [`is_terminal`](Self::is_terminal)
    /// for the literal rules/game-over predicate, which does not include that
    /// shortcut.
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        self.outcome_with_state(&state)
    }

    /// Same as [`outcome`](Self::outcome), reusing a populated [`StateInfo`].
    ///
    /// `state` must have been populated by
    /// [`populate_state`](Self::populate_state) for this board. A stale
    /// `StateInfo` can change extinction, check, and stalemate results.
    ///
    /// This may run
    /// [`has_legal_move_with_state`](crate::movegen::has_legal_move_with_state)
    /// when the answer depends on whether a legal move exists.
    #[must_use]
    pub fn outcome_with_state(&self, state: &StateInfo) -> Option<Outcome> {
        self.outcome_impl(state, None)
    }

    /// Hot-path form of [`outcome_with_state`](Self::outcome_with_state) when
    /// the caller already knows whether a legal move exists.
    ///
    /// `moves_exist` must be the result of
    /// [`has_legal_move_with_state(self, state)`](crate::movegen::has_legal_move_with_state).
    /// It is authoritative: passing `false` for a non-terminal position
    /// reports `Draw` or `Loss` according to
    /// [`StateInfo::checkers`], even though no move generation was run. The
    /// value is not consulted when extinction, the material shortcut, or the
    /// non-check 50-move branch already decides the position.
    #[must_use]
    pub fn outcome_with(&self, state: &StateInfo, moves_exist: bool) -> Option<Outcome> {
        self.outcome_impl(state, Some(moves_exist))
    }

    /// One precedence implementation shared by every `outcome*` query.
    fn outcome_impl(&self, state: &StateInfo, moves_exist: Option<bool>) -> Option<Outcome> {
        if state.commoners_count == 0 {
            return Some(Outcome::Loss);
        }
        if state.them_commoners_count == 0 {
            return Some(Outcome::Win);
        }
        if self.is_material_draw() {
            return Some(Outcome::Draw);
        }

        // The clock is a draw without movegen when there is no check. When the
        // side to move is in check, mate outranks the clock, so resolve the
        // existence question first (Fairy-Stockfish's `!checkers() || legal`
        // guard).
        if self.rule50 >= 100 && state.checkers.is_empty() {
            return Some(Outcome::Draw);
        }

        let moves_exist =
            moves_exist.unwrap_or_else(|| crate::movegen::has_legal_move_with_state(self, state));

        if !moves_exist {
            return Some(if state.checkers.is_empty() {
                Outcome::Draw
            } else {
                Outcome::Loss
            });
        }
        if self.rule50 >= 100 {
            return Some(Outcome::Draw);
        }
        None
    }

    /// Return `true` if both colors have exactly one commoner and no other
    /// pieces (an atomic K-vs-K position).
    ///
    /// This is the only material draw in atomic chess. It is deliberately
    /// **not** a general “insufficient material” test: a lone bishop, knight,
    /// or rook can win immediately by capturing the enemy commoner when its own
    /// commoner is outside the blast, so endings such as K+B vs K, K+N vs K,
    /// and K+R vs K are not draws. There is no K+B/K+N/K+R table here.
    ///
    /// The color checks exclude artificial boards with two commoners of one
    /// color, which are not atomic positions. Adjacency is irrelevant: touching
    /// commoners alone satisfy this predicate, but touching commoners with any
    /// other piece on the board do not.
    #[must_use]
    pub fn is_material_draw(&self) -> bool {
        self.occupied().count() == 2
            && self.commoners(Color::White).count() == 1
            && self.commoners(Color::Black).count() == 1
    }

    /// Return `true` if the game is literally over.
    ///
    /// This is extinction (either side has no commoner), the 50-move rule
    /// (`rule50 >= 100`), or no legal moves. It deliberately does **not**
    /// include the commoner-only material shortcut from
    /// [`outcome`](Self::outcome): a K-vs-K position with `rule50 < 100` is a
    /// WDL draw, but the game has not ended and may still reach the 50-move
    /// rule.
    ///
    /// This method populates a temporary [`StateInfo`] and uses the
    /// move-existence query. It never calls [`outcome`](Self::outcome).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        self.commoners(self.side_to_move).is_empty()
            || self.commoners(self.side_to_move.flip()).is_empty()
            || self.rule50 >= 100
            || !crate::movegen::has_legal_move_with_state(self, &state)
    }

    /// Return `true` if `m` is a capture on this board.
    #[inline]
    #[must_use]
    pub fn is_capture(&self, m: Move) -> bool {
        m.move_type() == MoveType::EnPassant
            || (m.move_type() != MoveType::Castling && self.piece_on(m.to_sq()) != NO_PIECE)
    }

    /// Return all pieces that attack `sq` on the given occupancy.
    ///
    /// Includes pawns, knights, bishops, rooks, queens, and commoners of either
    /// color that attack `sq`.
    #[must_use]
    pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
        let mut attackers = Bitboard::EMPTY;

        // Pawn attacks: the attackers come from the opposite direction
        let white_pawn_attacks = attacks::pawn_attacks(Color::White, sq);
        attackers =
            attackers | (white_pawn_attacks & self.pieces_color_pt(Color::Black, PieceType::Pawn));
        let black_pawn_attacks = attacks::pawn_attacks(Color::Black, sq);
        attackers =
            attackers | (black_pawn_attacks & self.pieces_color_pt(Color::White, PieceType::Pawn));

        // Knight attacks
        let knight_atk = attacks::knight_attacks(sq);
        attackers = attackers | (knight_atk & (self.by_type[PieceType::Knight as usize]));

        // Bishop/Queen attacks
        let bishop_atk = attacks::bishop_attacks(sq, occupied);
        attackers = attackers
            | (bishop_atk
                & (self.by_type[PieceType::Bishop as usize]
                    | self.by_type[PieceType::Queen as usize]));

        // Rook/Queen attacks
        let rook_atk = attacks::rook_attacks(sq, occupied);
        attackers = attackers
            | (rook_atk
                & (self.by_type[PieceType::Rook as usize]
                    | self.by_type[PieceType::Queen as usize]));

        // Commoner (king) attacks
        let king_atk = attacks::king_attacks(sq);
        attackers = attackers | (king_atk & self.by_type[PieceType::Commoner as usize]);

        attackers
    }

    pub(crate) fn compute_checkers(&self, us: Color) -> Bitboard {
        let them = us.flip();
        let commoners = self.commoners(us);
        let them_commoners = self.commoners(them);

        // Legal atomic positions have at most one commoner per side. If either
        // side has none, the game is already over, so there is no check.
        if commoners.is_empty() || them_commoners.is_empty() {
            return Bitboard::EMPTY;
        }
        let ksq = commoners.lsb();

        // The two commoners are the sides' only commoners. If they touch,
        // neither may capture the other: the blast would destroy the
        // capturer's own commoner. No other enemy piece can capture ours
        // either, because that blast would also destroy the enemy's only
        // commoner. Touching therefore makes our commoner immune to *all*
        // attacks, so it is not in check.
        if !(them_commoners & attacks::king_attacks(ksq)).is_empty() {
            return Bitboard::EMPTY;
        }

        let occupied = self.occupied();
        let them_bb = self.pieces_color(them);
        let rook_atk = attacks::rook_attacks(ksq, occupied);
        let bishop_atk = attacks::bishop_attacks(ksq, occupied);
        let queen_atk = rook_atk | bishop_atk;

        (rook_atk & self.by_type[PieceType::Rook as usize] & them_bb)
            | (bishop_atk & self.by_type[PieceType::Bishop as usize] & them_bb)
            | (queen_atk & self.by_type[PieceType::Queen as usize] & them_bb)
            | (attacks::knight_attacks(ksq) & self.by_type[PieceType::Knight as usize] & them_bb)
            | (attacks::pawn_attacks(us, ksq) & self.by_type[PieceType::Pawn as usize] & them_bb)
    }

    /// Return the bitboard of enemy pieces checking the side to move.
    #[must_use]
    pub fn checkers(&self) -> Bitboard {
        self.compute_checkers(self.side_to_move)
    }

    pub(crate) fn compute_pinned(&self, us: Color) -> Bitboard {
        let mut pinned = Bitboard::EMPTY;
        let commoners = self.commoners(us);
        let them = us.flip();
        let occupied = self.occupied();

        let mut c_iter = commoners;
        while !c_iter.is_empty() {
            let ksq = c_iter.pop_lsb();

            let mut snipers = ((self.by_type[PieceType::Rook as usize]
                | self.by_type[PieceType::Queen as usize])
                & attacks::rook_attacks(ksq, Bitboard::EMPTY))
                | ((self.by_type[PieceType::Bishop as usize]
                    | self.by_type[PieceType::Queen as usize])
                    & attacks::bishop_attacks(ksq, Bitboard::EMPTY));
            snipers = snipers & self.pieces_color(them);

            let mut s = snipers;
            while !s.is_empty() {
                let sniper_sq = s.pop_lsb();
                let between = between_bb(ksq, sniper_sq) & occupied;
                if between.count() == 1 {
                    pinned = pinned | (between & self.pieces_color(us));
                }
            }
        }

        pinned
    }

    /// Return the bitboard of pieces of the given color that are pinned
    /// (cannot move without exposing a commoner to capture).
    #[must_use]
    pub fn pinned(&self, c: Color) -> Bitboard {
        self.compute_pinned(c)
    }

    /// Fill cached state fields (checkers, pinned, commoner counts) for the
    /// current position so that `legal()` can read them instead of recomputing.
    pub fn populate_state(&self, state: &mut StateInfo) {
        state.checkers = self.compute_checkers(self.side_to_move);
        state.pinned = self.compute_pinned(self.side_to_move);
        state.commoners_count = self.commoners(self.side_to_move).count();
        state.them_commoners_count = self.commoners(self.side_to_move.flip()).count();
    }

    /// Make `m` on the board, storing undo information in `state`.
    ///
    /// Handles all move types (normal, promotion, en-passant, castling) as well
    /// as atomic-blast removal on captures.
    ///
    /// `state` is used only for undo information; cached attack fields
    /// (`checkers`, `pinned`, `commoners_count`) are *not* populated here.
    /// Callers that need a legal move check must call [`populate_state`](Self::populate_state)
    /// themselves before calling [`legal`](Self::legal), or use
    /// [`generate_legal`](crate::movegen::generate_legal) which creates its own
    /// populated `StateInfo`.
    pub fn do_move(&mut self, m: Move, state: &mut StateInfo) {
        state.hash = self.hash;
        state.castling_rights = self.castling_rights;
        state.ep_square = self.ep_square;
        state.rule50 = self.rule50;
        state.captured_count = 0;
        state.cap_sq = None;
        state.cap_piece = NO_PIECE;

        let us = self.side_to_move;
        let them = us.flip();
        let from = m.from_sq();
        let to = m.to_sq();
        let piece = self.squares[from as usize];
        let pt = piece.type_of().unwrap();
        let is_capture = !self.empty(to);
        let is_capture_or_ep = is_capture || m.move_type() == MoveType::EnPassant;

        if m.move_type() == MoveType::Castling {
            let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
            self.move_piece(
                kfrom,
                kto,
                make_piece(us, PieceType::Commoner),
                us,
                PieceType::Commoner,
            );
            self.move_piece(
                rfrom,
                rto,
                make_piece(us, PieceType::Rook),
                us,
                PieceType::Rook,
            );
            self.castling_rights &= match us {
                Color::White => !(WK_CASTLE | WQ_CASTLE),
                Color::Black => !(BK_CASTLE | BQ_CASTLE),
            };
            self.ep_square = None;
            self.rule50 += 1;
            self.side_to_move = them;
            self.game_ply += 1;
            self.update_hash_state_transition(state);
            return;
        }

        if m.move_type() == MoveType::EnPassant {
            let ep_cap = match us {
                Color::White => Square::from_index(to as i8 - 8),
                Color::Black => Square::from_index(to as i8 + 8),
            };
            let cap_piece = self.squares[ep_cap as usize];
            let cap_pt = cap_piece.type_of().unwrap();
            state.captured[state.captured_count as usize] = (ep_cap, cap_piece);
            state.captured_count += 1;
            self.remove_piece(ep_cap, them, cap_pt);
        } else if is_capture {
            let cap_piece = self.squares[to as usize];
            let cap_pt = cap_piece.type_of().unwrap();
            state.cap_sq = Some(to);
            state.cap_piece = cap_piece;
            state.cap_pt = cap_pt;
            self.remove_piece(to, them, cap_pt);
        }

        if m.move_type() == MoveType::Promotion {
            let prom_pt = m.promotion_type();
            let prom_piece = make_piece(us, prom_pt);
            self.remove_piece(from, us, PieceType::Pawn);
            self.place_piece(prom_piece, to, us, prom_pt);
        } else {
            self.move_piece(from, to, piece, us, pt);
        }

        // Blast on capture
        if is_capture_or_ep {
            // Blast zone = king attacks from `to`, minus pawn squares
            let blast_zone = attacks::king_attacks(to) & !self.by_type[PieceType::Pawn as usize];
            let mut to_blast = blast_zone & self.occupied();

            // Always blast the capturer at ground zero (pawns are NOT immune at `to`).
            to_blast = to_blast | Bitboard::square_bb(to);

            let mut b = to_blast;
            while !b.is_empty() {
                let bsq = b.pop_lsb();
                let bpiece = self.squares[bsq as usize];
                if bpiece != NO_PIECE {
                    let bc = bpiece.color().unwrap();
                    let bpt = bpiece.type_of().unwrap();
                    state.captured[state.captured_count as usize] = (bsq, bpiece);
                    state.captured_count += 1;
                    self.remove_piece(bsq, bc, bpt);
                }
            }
        }

        // Post-blast contents of the castling-relevant squares; the cascade
        // that drops rights whose rook/commoner vanished lives in the same
        // pure helper that [`Board::hash_after`] uses, so the two paths
        // cannot drift.
        let post_blast = [
            self.squares[Square::H1 as usize],
            self.squares[Square::A1 as usize],
            self.squares[Square::E1 as usize],
            self.squares[Square::H8 as usize],
            self.squares[Square::A8 as usize],
            self.squares[Square::E8 as usize],
        ];
        self.castling_rights = castling_rights_after(
            self.castling_rights,
            from,
            to,
            is_capture_or_ep,
            &post_blast,
        );

        if pt == PieceType::Pawn && (to as i8 - from as i8).abs() == 16 {
            self.ep_square = Some(match us {
                Color::White => Square::from_index(from as i8 + 8),
                Color::Black => Square::from_index(from as i8 - 8),
            });
        } else {
            self.ep_square = None;
        }

        if pt == PieceType::Pawn || is_capture {
            self.rule50 = 0;
        } else {
            self.rule50 += 1;
        }

        self.side_to_move = them;
        self.game_ply += 1;
        self.update_hash_state_transition(state);
    }

    /// Unmake `m`, restoring the board to its state before [`do_move`](Self::do_move).
    ///
    /// `state` must be the same [`StateInfo`] that was passed to `do_move`. It
    /// is used only for undo data; cached attack fields are not restored.
    pub fn undo_move(&mut self, m: Move, state: &StateInfo) {
        self.castling_rights = state.castling_rights;
        self.ep_square = state.ep_square;
        self.rule50 = state.rule50;

        self.side_to_move = self.side_to_move.flip();
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();

        if m.move_type() == MoveType::Castling {
            let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
            self.move_piece(
                kto,
                kfrom,
                make_piece(us, PieceType::Commoner),
                us,
                PieceType::Commoner,
            );
            self.move_piece(
                rto,
                rfrom,
                make_piece(us, PieceType::Rook),
                us,
                PieceType::Rook,
            );
            self.game_ply -= 1;
            self.hash = state.hash;
            return;
        }

        let mut i = state.captured_count;
        while i > 0 {
            i -= 1;
            let (sq, piece) = state.captured[i as usize];
            let c = piece.color().unwrap();
            let pt = piece.type_of().unwrap();
            self.place_piece(piece, sq, c, pt);
        }

        if m.move_type() == MoveType::Promotion {
            let pawn = make_piece(us, PieceType::Pawn);
            self.remove_piece(to, us, m.promotion_type());
            self.place_piece(pawn, from, us, PieceType::Pawn);
        } else {
            let piece = self.squares[to as usize];
            let pt = piece.type_of().unwrap();
            self.move_piece(to, from, piece, us, pt);
        }

        if let Some(sq) = state.cap_sq {
            let cap_piece = state.cap_piece;
            self.place_piece(cap_piece, sq, us.flip(), state.cap_pt);
        }

        self.game_ply -= 1;
        self.hash = state.hash;
    }

    #[inline]
    fn move_piece(&mut self, from: Square, to: Square, piece: Piece, c: Color, pt: PieceType) {
        debug_assert!(piece != NO_PIECE);
        let raw = piece.raw() as usize;
        self.hash ^= crate::zobrist::ZOBRIST.piece[from as usize][raw];
        self.hash ^= crate::zobrist::ZOBRIST.piece[to as usize][raw];

        self.squares[to as usize] = piece;
        self.squares[from as usize] = NO_PIECE;

        let from_bb = Bitboard::square_bb(from);
        let to_bb = Bitboard::square_bb(to);

        self.by_color[c as usize] = (self.by_color[c as usize] ^ from_bb) | to_bb;
        self.by_type[pt as usize] = (self.by_type[pt as usize] ^ from_bb) | to_bb;
    }

    #[inline]
    fn remove_piece(&mut self, sq: Square, c: Color, pt: PieceType) {
        debug_assert!(self.squares[sq as usize] != NO_PIECE);
        let piece = self.squares[sq as usize];
        let raw = piece.raw() as usize;
        self.hash ^= crate::zobrist::ZOBRIST.piece[sq as usize][raw];

        self.squares[sq as usize] = NO_PIECE;
        let sq_bb = Bitboard::square_bb(sq);
        self.by_color[c as usize] = self.by_color[c as usize] ^ sq_bb;
        self.by_type[pt as usize] = self.by_type[pt as usize] ^ sq_bb;
    }

    #[inline]
    fn place_piece(&mut self, piece: Piece, sq: Square, c: Color, pt: PieceType) {
        debug_assert!(self.squares[sq as usize] == NO_PIECE);
        let raw = piece.raw() as usize;
        self.hash ^= crate::zobrist::ZOBRIST.piece[sq as usize][raw];

        self.squares[sq as usize] = piece;
        let sq_bb = Bitboard::square_bb(sq);
        self.by_color[c as usize] = self.by_color[c as usize] | sq_bb;
        self.by_type[pt as usize] = self.by_type[pt as usize] | sq_bb;
    }

    /// Toggle the Zobrist hash for a change in side to move, castling rights,
    /// and en-passant square.
    ///
    /// `state` must hold the pre-move values for castling rights and ep square.
    /// The hash is updated from the state in `state` to the current state in `self`.
    #[inline]
    fn update_hash_state_transition(&mut self, state: &StateInfo) {
        self.hash ^= crate::zobrist::ZOBRIST.side;
        self.hash ^= crate::zobrist::ZOBRIST.castling[state.castling_rights as usize];
        self.hash ^= crate::zobrist::ZOBRIST.castling[self.castling_rights as usize];
        if let Some(sq) = state.ep_square {
            self.hash ^= crate::zobrist::ZOBRIST.ep[crate::types::file_of(sq) as usize];
        }
        if let Some(sq) = self.ep_square {
            self.hash ^= crate::zobrist::ZOBRIST.ep[crate::types::file_of(sq) as usize];
        }
    }

    /// Check whether `m` is legal under atomic chess rules.
    ///
    /// Considers blast-zone effects (self-explosion), castling pass-through
    /// safety, and commoner extinction. A move that destroys any of the mover's
    /// commoners is illegal; in a legal atomic position there is exactly one, so
    /// there is no spare-commoner exception. Touching the enemy commoner is
    /// allowed and does not count as an attack: any capture would also blast
    /// the enemy's only commoner away.
    ///
    /// `state` must contain up-to-date cached fields for the current position.
    /// A terminal board (either side has no commoner) has no legal moves.
    ///
    /// # Preconditions
    ///
    /// This is the low-level predicate used by the move generator. It assumes
    /// that `m` is pseudo-legal and that `state` has been populated with
    /// [`populate_state`](Self::populate_state). For untrusted move input use
    /// [`is_legal`](Self::is_legal) or
    /// [`is_legal_with_state`](Self::is_legal_with_state), which validate the
    /// move shape first.
    #[must_use]
    pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
        // A board with only one commoner is terminal.
        if state.commoners_count == 0 || state.them_commoners_count == 0 {
            return false;
        }

        let from = m.from_sq();
        let to = m.to_sq();
        let us = self.side_to_move;
        let them = us.flip();

        let piece = self.piece_on(from);
        if piece == NO_PIECE {
            return false;
        }

        let is_capture = m.move_type() != MoveType::Castling
            && (m.move_type() == MoveType::EnPassant || self.piece_on(to) != NO_PIECE);

        if m.move_type() == MoveType::Castling {
            let ksq = from;
            let occupied = self.occupied();
            let pass_through = if to > ksq {
                [ksq, Square::from_index(ksq as i8 + 1)]
            } else {
                [Square::from_index(ksq as i8 - 1), ksq]
            };
            let enemy_commoners = self.commoners(them);
            for &sq in &pass_through {
                // A commoner adjacent to this square makes the commoner
                // immune to every attack while it stands there.
                if !(enemy_commoners & attacks::king_attacks(sq)).is_empty() {
                    continue;
                }
                if square_attacked_by(self, sq, occupied, self.by_color[them as usize]) {
                    return false;
                }
            }
        }

        let mut occupied = self.occupied() ^ Bitboard::square_bb(from);
        let mut kto = to;

        if m.move_type() == MoveType::Castling {
            let (_, kto_actual, rfrom, rto) = castling_squares(us, to > from);
            kto = kto_actual;
            occupied = occupied ^ Bitboard::square_bb(rfrom);
            occupied = occupied | Bitboard::square_bb(rto);
        } else if m.move_type() == MoveType::EnPassant {
            let capsq = match us {
                Color::White => Square::from_index(to as i8 - 8),
                Color::Black => Square::from_index(to as i8 + 8),
            };
            occupied = occupied & !Bitboard::square_bb(capsq);
        }

        occupied = occupied | Bitboard::square_bb(kto);

        if is_capture {
            let pre_pawns = self.by_type[PieceType::Pawn as usize];
            let pre_non_pawns = self.occupied() ^ pre_pawns;
            let blast_adjacent = attacks::king_attacks(kto) & pre_non_pawns;
            occupied = occupied & !(blast_adjacent | Bitboard::square_bb(kto));
        }

        let pre_commoners = self.commoners(us);
        let mut our_commoners = pre_commoners & occupied;

        if !is_capture && piece == make_piece(us, PieceType::Commoner) {
            our_commoners = our_commoners | Bitboard::square_bb(kto);
        }

        // A move may not destroy any of our commoners. In a legal position
        // there is exactly one, so this is the usual self-explosion check.
        if our_commoners.count() != pre_commoners.count() {
            return false;
        }

        // Capturing the enemy's only commoner wins immediately; the mover's
        // own commoner cannot be in check after the game has ended.
        let enemy_commoners = self.commoners(them) & occupied;
        if enemy_commoners.is_empty() {
            return true;
        }

        let ksq = our_commoners.lsb();
        // Touching the enemy commoner grants immunity to every attack.
        if !(enemy_commoners & attacks::king_attacks(ksq)).is_empty() {
            return true;
        }

        !square_attacked_by(self, ksq, occupied, self.by_color[them as usize] & occupied)
    }

    /// Check whether `m` is fully legal on this board.
    ///
    /// Populates a temporary [`StateInfo`] internally, then applies
    /// [`is_legal_with_state`](Self::is_legal_with_state). This is the
    /// untrusted-input single-move predicate; it never builds or clones a
    /// [`MoveList`].
    #[must_use]
    pub fn is_legal(&self, m: Move) -> bool {
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        self.is_legal_with_state(m, &state)
    }

    /// Check whether `m` is fully legal using a caller-populated [`StateInfo`].
    ///
    /// The result is exactly membership in
    /// `generate_legal_with_state(self, state, ..)`: the move must be
    /// pseudo-legal *and* pass [`legal`](Self::legal). The shape check is what
    /// makes arbitrary 16-bit input safe; for example, an encoded castling
    /// move whose origin is not a commoner home square is rejected even if
    /// [`legal`](Self::legal) would otherwise accept it. The hot move generator
    /// keeps calling [`legal`](Self::legal) directly.
    ///
    /// The caller is responsible for ensuring `state` is up to date; stale
    /// state can change the answer, exactly as it does for
    /// [`legal`](Self::legal).
    #[must_use]
    pub fn is_legal_with_state(&self, m: Move, state: &StateInfo) -> bool {
        self.is_pseudo_legal(m) && self.legal(m, state)
    }

    /// Check whether `m` is pseudo-legal on this board.
    ///
    /// The result is exactly membership in
    /// [`generate_pseudo_legal`](crate::movegen::generate_pseudo_legal): the
    /// origin holds a piece of the side to move, the destination is not
    /// occupied by that side (except for the internal castling encoding), and
    /// the move shape matches the moving piece. Castling rights, the rook and
    /// commoner home squares, and the empty path are checked, but attacked
    /// pass-through squares are not; those require [`legal`](Self::legal).
    ///
    /// This is **not** a legality guarantee and does not check self-explosion
    /// or check. No [`MoveList`] is built.
    #[must_use]
    pub fn is_pseudo_legal(&self, m: Move) -> bool {
        if m == Move::NONE {
            return false;
        }

        let from = m.from_sq();
        let to = m.to_sq();
        if from == to {
            return false;
        }
        // `from_bits` accepts non-canonical promotion-type bits on non-promotion
        // moves; the generator never emits them.
        if !m.has_canonical_promotion_bits() {
            return false;
        }

        let us = self.side_to_move;
        let piece = self.piece_on(from);
        if piece == NO_PIECE || piece.color() != Some(us) {
            return false;
        }

        if m.move_type() == MoveType::Castling {
            return self.is_pseudo_legal_castling(from, to);
        }

        let to_piece = self.piece_on(to);
        if to_piece != NO_PIECE && to_piece.color() == Some(us) {
            return false;
        }

        let Some(pt) = piece.type_of() else {
            return false;
        };
        // Only pawns have non-normal encodings. A knight/rook/etc. bit pattern
        // with promotion, en-passant, or castling type bits is malformed; the
        // generator emits ordinary moves for these pieces.
        if pt != PieceType::Pawn && m.move_type() != MoveType::Normal {
            return false;
        }
        match pt {
            PieceType::Pawn => self.is_pseudo_legal_pawn(m),
            PieceType::Knight => {
                !(attacks::knight_attacks(from) & Bitboard::square_bb(to)).is_empty()
            }
            PieceType::Bishop => !(attacks::bishop_attacks(from, self.occupied())
                & Bitboard::square_bb(to))
            .is_empty(),
            PieceType::Rook => {
                !(attacks::rook_attacks(from, self.occupied()) & Bitboard::square_bb(to)).is_empty()
            }
            PieceType::Queen => !(attacks::queen_attacks(from, self.occupied())
                & Bitboard::square_bb(to))
            .is_empty(),
            PieceType::Commoner => {
                !(attacks::king_attacks(from) & Bitboard::square_bb(to)).is_empty()
            }
        }
    }

    /// Pawn branch of [`is_pseudo_legal`](Self::is_pseudo_legal), mirroring
    /// the pawn generator exactly.
    fn is_pseudo_legal_pawn(&self, m: Move) -> bool {
        let us = self.side_to_move;
        let them = us.flip();
        let from = m.from_sq();
        let to = m.to_sq();
        let (start_rank, promotion_from_rank, promotion_rank) = match us {
            Color::White => (Rank::R2, Rank::R7, Rank::R8),
            Color::Black => (Rank::R7, Rank::R2, Rank::R1),
        };
        let from_rank = rank_of(from);
        let to_rank = rank_of(to);
        let them_bb = self.pieces_color(them);
        let attacks_bb = attacks::pawn_attacks(us, from);
        let captures_to = attacks_bb & them_bb & Bitboard::square_bb(to);

        match m.move_type() {
            MoveType::Promotion => {
                if from_rank != promotion_from_rank || to_rank != promotion_rank {
                    return false;
                }
                let single = attacks::pawn_pushes(us, from);
                let is_push = !single.is_empty() && to == single.lsb() && self.empty(to);
                is_push || !captures_to.is_empty()
            }
            MoveType::EnPassant => {
                let expected_rank = match us {
                    Color::White => Rank::R6,
                    Color::Black => Rank::R3,
                };
                if to_rank != expected_rank || self.ep_square != Some(to) {
                    return false;
                }
                if (attacks_bb & Bitboard::square_bb(to)).is_empty() {
                    return false;
                }
                let captured_sq = match us {
                    Color::White => Square::from_index(to as i8 - 8),
                    Color::Black => Square::from_index(to as i8 + 8),
                };
                captured_sq != Square::NONE
                    && self.piece_on(captured_sq) == make_piece(them, PieceType::Pawn)
            }
            MoveType::Normal => {
                // The generator emits a promotion, never a normal move, when
                // a pawn reaches the last rank.
                if to_rank == promotion_rank {
                    return false;
                }

                let single = attacks::pawn_pushes(us, from);
                if !single.is_empty() && to == single.lsb() && self.empty(to) {
                    return true;
                }

                if from_rank == start_rank {
                    let double = attacks::pawn_double_pushes(us, from);
                    if !double.is_empty() && to == double.lsb() && self.empty(to) {
                        let intermediate = match us {
                            Color::White => Square::from_index(to as i8 - 8),
                            Color::Black => Square::from_index(to as i8 + 8),
                        };
                        return intermediate != Square::NONE && self.empty(intermediate);
                    }
                }

                !captures_to.is_empty()
            }
            MoveType::Castling => false,
        }
    }

    /// Castling branch of [`is_pseudo_legal`](Self::is_pseudo_legal). The
    /// encoded destination is the rook home square; attacked pass-through
    /// squares are deliberately left to [`legal`](Self::legal).
    fn is_pseudo_legal_castling(&self, from: Square, to: Square) -> bool {
        let us = self.side_to_move;
        let (
            king_right,
            queen_right,
            king_sq,
            king_rook,
            queen_rook,
            king_side_clear,
            queen_side_clear,
        ) = match us {
            Color::White => (
                WK_CASTLE,
                WQ_CASTLE,
                Square::E1,
                Square::H1,
                Square::A1,
                self.empty(Square::F1) && self.empty(Square::G1),
                self.empty(Square::B1) && self.empty(Square::C1) && self.empty(Square::D1),
            ),
            Color::Black => (
                BK_CASTLE,
                BQ_CASTLE,
                Square::E8,
                Square::H8,
                Square::A8,
                self.empty(Square::F8) && self.empty(Square::G8),
                self.empty(Square::B8) && self.empty(Square::C8) && self.empty(Square::D8),
            ),
        };

        if from != king_sq {
            return false;
        }
        let (right, rook_sq, path_clear) = if to == king_rook {
            (king_right, king_rook, king_side_clear)
        } else if to == queen_rook {
            (queen_right, queen_rook, queen_side_clear)
        } else {
            return false;
        };

        self.castling_rights & right != 0
            && self.piece_on(king_sq) == make_piece(us, PieceType::Commoner)
            && self.piece_on(rook_sq) == make_piece(us, PieceType::Rook)
            && path_clear
    }

    /// Board-aware UCI lookup.
    ///
    /// Parses `s` with [`Move::from_uci`](crate::Move::from_uci), then tries
    /// the board-dependent encodings that plain UCI text cannot distinguish:
    /// the four castling destinations (`e1g1`, `e1c1`, `e8g8`, `e8c8`) map to
    /// the internal king-takes-rook encoding, and a normal-looking move onto
    /// the current en-passant target maps to an en-passant capture. Only a
    /// fully legal candidate is returned. If no special encoding is legal, the
    /// parsed ordinary move is checked last, so a rook move such as `e8g8`
    /// still resolves correctly when castling is unavailable.
    ///
    /// `"0000"`, malformed text, and illegal moves return `None`.
    /// [`Move::from_uci`](crate::Move::from_uci) is syntax-only and does not
    /// resolve castling or en passant; this method is the safe replacement for
    /// generating all legal moves and comparing UCI strings.
    #[must_use]
    pub fn move_from_uci(&self, s: &str) -> Option<Move> {
        let parsed = Move::from_uci(s)?;
        if parsed == Move::NONE {
            return None;
        }

        let mut state = StateInfo::new();
        self.populate_state(&mut state);

        for candidate in [
            self.resolve_castling_uci(parsed),
            self.resolve_en_passant_uci(parsed),
        ]
        .into_iter()
        .flatten()
        {
            if self.is_legal_with_state(candidate, &state) {
                return Some(candidate);
            }
        }

        self.is_legal_with_state(parsed, &state).then_some(parsed)
    }

    /// Map UCI castling destinations to the internal king-takes-rook encoding.
    fn resolve_castling_uci(&self, m: Move) -> Option<Move> {
        match (m.from_sq(), m.to_sq()) {
            (Square::E1, Square::G1) => Some(Move::make_castling(Square::E1, Square::H1)),
            (Square::E1, Square::C1) => Some(Move::make_castling(Square::E1, Square::A1)),
            (Square::E8, Square::G8) => Some(Move::make_castling(Square::E8, Square::H8)),
            (Square::E8, Square::C8) => Some(Move::make_castling(Square::E8, Square::A8)),
            _ => None,
        }
    }

    /// Map a normal-looking move onto the current en-passant target to the
    /// internal en-passant encoding.
    fn resolve_en_passant_uci(&self, m: Move) -> Option<Move> {
        if m.move_type() != MoveType::Normal || self.ep_square != Some(m.to_sq()) {
            return None;
        }
        Some(Move::make_enpassant(m.from_sq(), m.to_sq()))
    }
}

/// Return `true` if an enemy non-commoner piece attacks `sq` on `occupied`.
///
/// Mirrors the attacker categories of the previous private `attackers_to`
/// helper, but exits at the first matching category. Commoner attacks are
/// intentionally excluded; callers handle adjacent enemy commoners separately.
#[inline(always)]
fn square_attacked_by(board: &Board, sq: Square, occupied: Bitboard, enemy_bb: Bitboard) -> bool {
    // Pawns. `board.side_to_move` is the side whose commoner is being tested,
    // matching the previous helper's pawn direction.
    if !(attacks::pawn_attacks(board.side_to_move, sq)
        & board.by_type[PieceType::Pawn as usize]
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Knights.
    if !(attacks::knight_attacks(sq) & board.by_type[PieceType::Knight as usize] & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Bishop/queen sliders.
    if !(attacks::bishop_attacks(sq, occupied)
        & (board.by_type[PieceType::Bishop as usize] | board.by_type[PieceType::Queen as usize])
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Rook/queen sliders. This is skipped when a pawn, knight, bishop, or
    // queen already hit.
    if !(attacks::rook_attacks(sq, occupied)
        & (board.by_type[PieceType::Rook as usize] | board.by_type[PieceType::Queen as usize])
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    false
}

/// Return the Zobrist piece key for the piece with raw encoding `raw`
/// standing on `sq`, skipping the bounds checks the compiler cannot elide.
///
/// `ZOBRIST.piece` is `[[-u64; 16]; 64]`. Every `Square` used as a board
/// index is `< 64` and every piece raw encoding is `< 16` by construction
/// (see `types.rs`); debug builds assert both, release builds rely on the
/// invariants. This is confined to `hash_after`, where the extra compare-
/// and-panic plumbing per table load was a measurable share of the runtime.
#[inline(always)]
fn piece_key_raw(sq: Square, raw: u8) -> u64 {
    debug_assert!((sq as usize) < 64);
    debug_assert!((raw as usize) < 16);
    // SAFETY: in-bounds per the invariants above (debug-asserted).
    unsafe {
        *crate::zobrist::ZOBRIST
            .piece
            .get_unchecked(sq as usize)
            .get_unchecked(raw as usize)
    }
}

/// Same as [`piece_key_raw`] for a full [`Piece`].
#[inline(always)]
fn piece_key(sq: Square, piece: Piece) -> u64 {
    piece_key_raw(sq, piece.raw())
}

/// Pure castling-rights transition for one move, shared by
/// [`Board::do_move`] and [`Board::hash_after`] so the mutating and the
/// speculative paths cannot drift.
///
/// `old_rights` are the rights before the move. `is_capture` must be set for
/// normal *and* en-passant captures (anything that triggers a blast).
/// `post_blast` holds the piece on H1, A1, E1, H8, A8, E8 *after* the move
/// (blast removals included); when `is_capture` is set, rights whose rook or
/// commoner no longer occupies its home square are dropped.
fn castling_rights_after(
    old_rights: u8,
    from: Square,
    to: Square,
    is_capture: bool,
    post_blast: &[Piece; 6],
) -> u8 {
    let mut rights = old_rights;

    // King side
    if from == Square::E1
        || from == Square::H1
        || (is_capture && (to == Square::H1 || to == Square::E1))
    {
        rights &= !WK_CASTLE;
    }
    if from == Square::E1
        || from == Square::A1
        || (is_capture && (to == Square::A1 || to == Square::E1))
    {
        rights &= !WQ_CASTLE;
    }
    if from == Square::E8
        || from == Square::H8
        || (is_capture && (to == Square::H8 || to == Square::E8))
    {
        rights &= !BK_CASTLE;
    }
    if from == Square::E8
        || from == Square::A8
        || (is_capture && (to == Square::A8 || to == Square::E8))
    {
        rights &= !BQ_CASTLE;
    }

    if is_capture {
        let [h1, a1, e1, h8, a8, e8] = *post_blast;
        // Clear white king-side right if the H1 rook is gone.
        if rights & WK_CASTLE != 0 && h1 != make_piece(Color::White, PieceType::Rook) {
            rights &= !WK_CASTLE;
        }
        // Clear white queen-side right if the A1 rook is gone.
        if rights & WQ_CASTLE != 0 && a1 != make_piece(Color::White, PieceType::Rook) {
            rights &= !WQ_CASTLE;
        }
        // Clear black king-side right if the H8 rook is gone.
        if rights & BK_CASTLE != 0 && h8 != make_piece(Color::Black, PieceType::Rook) {
            rights &= !BK_CASTLE;
        }
        // Clear black queen-side right if the A8 rook is gone.
        if rights & BQ_CASTLE != 0 && a8 != make_piece(Color::Black, PieceType::Rook) {
            rights &= !BQ_CASTLE;
        }
        // Clear all white castling rights if the commoner on E1 is gone.
        if rights & (WK_CASTLE | WQ_CASTLE) != 0
            && e1 != make_piece(Color::White, PieceType::Commoner)
        {
            rights &= !(WK_CASTLE | WQ_CASTLE);
        }
        // Clear all black castling rights if the commoner on E8 is gone.
        if rights & (BK_CASTLE | BQ_CASTLE) != 0
            && e8 != make_piece(Color::Black, PieceType::Commoner)
        {
            rights &= !(BK_CASTLE | BQ_CASTLE);
        }
    }

    rights
}

/// Return the castling square data for a given color and side.
fn castling_squares(us: Color, kingside: bool) -> (Square, Square, Square, Square) {
    match (us, kingside) {
        (Color::White, true) => (Square::E1, Square::G1, Square::H1, Square::F1),
        (Color::White, false) => (Square::E1, Square::C1, Square::A1, Square::D1),
        (Color::Black, true) => (Square::E8, Square::G8, Square::H8, Square::F8),
        (Color::Black, false) => (Square::E8, Square::C8, Square::A8, Square::D8),
    }
}

/// Node-constant part of the trivial-legality fast path.
///
/// Build once per node from a populated [`StateInfo`], then call
/// [`is_trivially_legal`](Self::is_trivially_legal) for every pseudo-legal
/// candidate. The fast path accepts only moves that are legal without the full
/// [`Board::legal`] check: no checkers, not en passant, not castling, not a
/// capture, the origin is not a commoner, the moving piece is unpinned, and at
/// least one own commoner still exists.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrivialLegality {
    /// Enemy pieces giving check to the side to move (`state.checkers`).
    checkers: Bitboard,
    /// Squares that are neither own commoners nor pinned, or [`Bitboard::EMPTY`]
    /// when the side to move has no commoner left.
    safe_origins: Bitboard,
}

impl TrivialLegality {
    /// Derive the node-constant fast-path data from a populated [`StateInfo`].
    ///
    /// All inputs are only read, so the resulting value stays valid for every
    /// candidate move of the node.
    #[inline(always)]
    pub(crate) fn new(board: &Board, state: &StateInfo) -> Self {
        debug_assert!(
            state.commoners_count == 0 || !board.commoners(board.side_to_move()).is_empty(),
            "StateInfo.commoners_count disagrees with the board"
        );
        let safe_origins = if state.commoners_count == 0 {
            // No own commoner: every candidate must go through `legal`, which
            // rejects all of them (matching the old `commoners_count == 0` guard).
            Bitboard::EMPTY
        } else {
            !(board.commoners(board.side_to_move()) | state.pinned)
        };
        Self {
            checkers: state.checkers,
            safe_origins,
        }
    }

    /// Returns `true` when `m` is trivially legal.
    ///
    /// `m` must come from the pseudo-legal generator (valid origin and
    /// destination squares); this is a crate-private precondition, not an input
    /// validation point.
    #[inline(always)]
    pub(crate) fn is_trivially_legal(&self, board: &Board, m: Move) -> bool {
        debug_assert!(m != Move::NONE);
        if !self.checkers.is_empty() {
            return false; // in check: every move needs the full check
        }
        if m.needs_full_legal_check() {
            return false; // en passant and castling
        }
        if board.squares[m.dest_index()] != NO_PIECE {
            return false; // capture
        }
        // Origin must be neither a commoner nor pinned.
        self.safe_origins.contains(m.origin_index())
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        for rank in (0..8).rev() {
            write!(f, "{} ", rank + 1)?;
            for file in 0..8 {
                let wrapped_sq = make_square(
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
                    match rank {
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
                let wrapped_idx = wrapped_sq as usize;
                if self.squares[wrapped_idx] == NO_PIECE {
                    write!(f, " .")?;
                } else {
                    write!(f, " {}", self.squares[wrapped_idx].ascii_char())?;
                }
            }
            writeln!(f)?;
        }
        writeln!(f, "   a b c d e f g h")?;
        writeln!(f, "Side to move: {:?}", self.side_to_move)?;
        writeln!(f, "Castling: {:b}", self.castling_rights)?;
        writeln!(f, "EP: {:?}", self.ep_square)?;
        writeln!(f, "FEN: {}", self.fen())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starting_position() {
        let board = Board::new();
        assert_eq!(board.piece_on(Square::E1), W_COMMONER);
        assert_eq!(board.piece_on(Square::D1), W_QUEEN);
        assert_eq!(board.piece_on(Square::E8), B_COMMONER);
        assert_eq!(board.side_to_move(), Color::White);
        assert_eq!(
            board.castling_rights,
            WK_CASTLE | WQ_CASTLE | BK_CASTLE | BQ_CASTLE
        );
    }

    #[test]
    fn test_fen_roundtrip() {
        let board = Board::new();
        let fen = board.fen();
        let board2 = Board::from_fen(&fen).unwrap();
        assert_eq!(board.fen(), board2.fen());
    }

    #[test]
    fn test_custom_fen() {
        let fen = "8/8/8/8/8/8/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.piece_on(Square::E1), W_COMMONER);
    }

    #[test]
    fn test_fen_uses_standard_commoner_label() {
        // Standard atomic FEN labels the commoner (king-like piece) as K/k.
        let fen = Board::new().fen();
        assert!(
            fen.contains("K") && fen.contains("k"),
            "standard FEN should contain 'K' and 'k', got: {fen}"
        );
        assert!(
            !fen.contains('C') && !fen.contains('c'),
            "non-standard 'C'/'c' labels must not be emitted, got: {fen}"
        );
    }

    #[test]
    fn test_from_fen_rejects_nonstandard_commoner_label() {
        // C/c are not part of standard FEN; only K/k label the commoner.
        for fen in ["8/8/8/8/8/8/8/4C3 w - - 0 1", "8/8/8/8/8/8/8/4c3 w - - 0 1"] {
            let result = Board::from_fen(fen);
            assert!(
                result.is_err(),
                "expected non-standard FEN {fen:?} to be rejected"
            );
        }
    }

    #[test]
    fn validate_accepts_legal_positions() {
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            // FSF-style ep target with no legal ep capture.
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            // Promotion-heavy, with the side to move in check (allowed).
            "4k3/PPPPPPPP/8/8/8/8/8/4K3 b - - 0 1",
        ];
        for fen in fens {
            let board = Board::from_fen(fen).expect("test FEN must parse");
            assert_eq!(board.validate(), Ok(()), "unexpected errors for {fen}");
        }
    }

    #[test]
    fn validate_rejects_structural_errors() {
        let cases: &[(&str, ValidationErrors)] = &[
            (
                "8/8/8/8/8/8/8/8 w - - 0 1",
                ValidationErrors::EMPTY_BOARD
                    .union(ValidationErrors::MISSING_COMMONER_WHITE)
                    .union(ValidationErrors::MISSING_COMMONER_BLACK),
            ),
            (
                "4k3/8/8/8/8/8/8/8 w - - 0 1",
                ValidationErrors::MISSING_COMMONER_WHITE,
            ),
            (
                "8/8/8/8/8/8/8/4K3 w - - 0 1",
                ValidationErrors::MISSING_COMMONER_BLACK,
            ),
            (
                "4k3/8/8/8/8/8/8/3KK3 w - - 0 1",
                ValidationErrors::TOO_MANY_COMMONERS_WHITE,
            ),
            (
                "3kk3/8/8/8/8/8/8/4K3 w - - 0 1",
                ValidationErrors::TOO_MANY_COMMONERS_BLACK,
            ),
            (
                "4k2P/8/8/8/8/8/8/p3K3 w - - 0 1",
                ValidationErrors::PAWNS_ON_BACKRANK,
            ),
            (
                "4k3/4p3/8/4p3/8/8/8/4K3 w - e6 0 1",
                ValidationErrors::INVALID_EP_SQUARE,
            ),
            (
                "4k3/8/4n3/4p3/8/8/8/4K3 w - e6 0 1",
                ValidationErrors::INVALID_EP_SQUARE,
            ),
            (
                "4k3/QQQQQQQQ/QQ2QQQQ/8/8/qq2qqqq/qqqqqqqq/4K3 w - - 0 1",
                ValidationErrors::TOO_MUCH_MATERIAL,
            ),
            (
                "4k3/4R3/8/8/8/8/8/4K3 w - - 0 1",
                ValidationErrors::OPPOSITE_CHECK,
            ),
        ];
        for &(fen, expected) in cases {
            let board = Board::from_fen(fen).expect("test FEN must parse");
            let errors = board.validate().expect_err("position should be rejected");
            assert!(
                errors.contains(expected),
                "expected {expected} for {fen}, got {errors}"
            );
        }
    }

    #[test]
    fn validate_is_opt_in() {
        let fens = [
            "8/8/8/8/8/8/8/8 w - - 0 1",
            "4k3/8/8/8/8/8/8/8 w - - 0 1",
            "8/8/8/8/8/8/8/4K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/3KK3 w - - 0 1",
            "3kk3/8/8/8/8/8/8/4K3 w - - 0 1",
            "4k2P/8/8/8/8/8/8/p3K3 w - - 0 1",
            "4k3/4p3/8/4p3/8/8/8/4K3 w - e6 0 1",
            "4k3/8/4n3/4p3/8/8/8/4K3 w - e6 0 1",
            "4k3/QQQQQQQQ/QQ2QQQQ/8/8/qq2qqqq/qqqqqqqq/4K3 w - - 0 1",
            "4k3/4R3/8/8/8/8/8/4K3 w - - 0 1",
        ];
        for fen in fens {
            assert!(
                Board::from_fen(fen).is_ok(),
                "permissive parser must still accept {fen}"
            );
        }
    }

    #[test]
    fn fen_policy_behavior() {
        let legal = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        assert!(Board::from_fen_with(legal, FenPolicy::Strict).is_ok());

        let missing = "4k3/8/8/8/8/8/8/8 w - - 0 1";
        match Board::from_fen_with(missing, FenPolicy::Strict) {
            Err(FenLoadError::InvalidPosition(errors)) => {
                assert!(errors.contains(ValidationErrors::MISSING_COMMONER_WHITE));
            }
            other => panic!("expected InvalidPosition, got {other:?}"),
        }

        let terminal = "8/8/8/8/8/8/8/K7 w - - 0 1";
        assert!(Board::from_fen_with(terminal, FenPolicy::AllowTerminal).is_ok());
        let empty = "8/8/8/8/8/8/8/8 w - - 0 1";
        assert!(Board::from_fen_with(empty, FenPolicy::AllowTerminal).is_ok());

        let two_commoners = "4k3/8/8/8/8/8/8/3KK3 w - - 0 1";
        match Board::from_fen_with(two_commoners, FenPolicy::AllowTerminal) {
            Err(FenLoadError::InvalidPosition(errors)) => {
                assert!(errors.contains(ValidationErrors::TOO_MANY_COMMONERS_WHITE));
            }
            other => panic!("expected InvalidPosition, got {other:?}"),
        }

        let opposite_check = "4k3/4R3/8/8/8/8/8/4K3 w - - 0 1";
        assert!(Board::from_fen_with(opposite_check, FenPolicy::Strict).is_err());
        assert!(Board::from_fen_with(opposite_check, FenPolicy::AllowImpossibleCheck).is_ok());

        let all_fens = [
            legal,
            missing,
            terminal,
            empty,
            two_commoners,
            opposite_check,
            "4k3/4p3/8/4p3/8/8/8/4K3 w - e6 0 1",
            "4k3/QQQQQQQQ/QQ2QQQQ/8/8/qq2qqqq/qqqqqqqq/4K3 w - - 0 1",
        ];
        for fen in all_fens {
            match (
                Board::from_fen(fen),
                Board::from_fen_with(fen, FenPolicy::Permissive),
            ) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(a.fen(), b.fen(), "permissive FEN mismatch for {fen}");
                    assert_eq!(a.hash(), b.hash(), "permissive hash mismatch for {fen}");
                }
                (Err(a), Err(FenLoadError::Syntax(b))) => {
                    assert_eq!(a.to_string(), b.to_string(), "syntax mismatch for {fen}");
                }
                (a, b) => panic!("permissive mismatch for {fen}: {a:?} vs {b:?}"),
            }
        }

        assert!(matches!(
            Board::from_fen_with("not a fen", FenPolicy::Strict),
            Err(FenLoadError::Syntax(FenError::TooShort { .. }))
        ));
    }

    #[test]
    fn proof_db_workflow() {
        // A proof-DB style ingestion path: the permissive parser still accepts
        // artificial fixtures, while the strict loader rejects them unless the
        // caller explicitly opts into a terminal policy.
        let malformed = "4k3/8/8/8/8/8/8/3KK3 w - - 0 1";
        assert!(Board::from_fen(malformed).is_ok());
        match Board::from_fen_with(malformed, FenPolicy::Strict) {
            Err(FenLoadError::InvalidPosition(errors)) => {
                assert!(errors.contains(ValidationErrors::TOO_MANY_COMMONERS_WHITE));
            }
            other => panic!("expected InvalidPosition, got {other:?}"),
        }

        let artificial = "8/8/8/8/8/8/8/K7 w - - 0 1";
        assert!(Board::from_fen(artificial).is_ok());
        assert!(Board::from_fen_with(artificial, FenPolicy::AllowTerminal).is_ok());
    }

    #[test]
    fn validation_errors_display_and_bits() {
        let errors = ValidationErrors::EMPTY_BOARD
            .union(ValidationErrors::MISSING_COMMONER_WHITE)
            .union(ValidationErrors::PAWNS_ON_BACKRANK);
        assert!(errors.contains(ValidationErrors::EMPTY_BOARD));
        assert!(errors.contains(ValidationErrors::MISSING_COMMONER_WHITE));
        assert!(!errors.contains(ValidationErrors::MISSING_COMMONER_BLACK));
        assert_eq!(
            errors.intersection(ValidationErrors::EMPTY_BOARD),
            ValidationErrors::EMPTY_BOARD
        );
        assert_eq!(
            errors.bits(),
            ValidationErrors::EMPTY_BOARD.bits()
                | ValidationErrors::MISSING_COMMONER_WHITE.bits()
                | ValidationErrors::PAWNS_ON_BACKRANK.bits()
        );
        assert!(ValidationErrors::empty().is_empty());
        assert!(!errors.is_empty());

        let text = errors.to_string();
        assert!(text.contains("EMPTY_BOARD"), "{text}");
        assert!(text.contains("MISSING_COMMONER_WHITE"), "{text}");
        assert!(text.contains("PAWNS_ON_BACKRANK"), "{text}");
        assert!(!text.contains("MISSING_COMMONER_BLACK"), "{text}");
    }

    #[test]
    fn test_checkers() {
        // Position where white queen gives check to black commoner
        let fen = "4k3/8/8/8/8/8/8/4Q2K b - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let checkers = board.checkers();
        assert!(!checkers.is_empty(), "Expected checkers, got empty");
    }

    #[test]
    fn test_touching_commoners_are_immune() {
        // Exactly one commoner each, adjacent: neither can capture the other
        // (the blast would destroy the capturer as well), so adjacency is not
        // a check and the commoner is immune to *every* attacker.
        let board = Board::from_fen("8/8/8/8/8/8/4k3/4K3 w - - 0 1").unwrap();
        assert!(board.checkers().is_empty());

        // The black rook attacks e1 along the first rank, but the white
        // commoner touches the black one, so it cannot be captured.
        let board = Board::from_fen("8/8/8/8/8/8/4k3/r3K3 w - - 0 1").unwrap();
        assert!(board.checkers().is_empty());
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        // d1, f1, d2, f2: every destination is still adjacent to the black
        // commoner, so the rook's attack does not matter.
        assert_eq!(moves.len(), 4);
    }

    #[test]
    fn test_castling_through_square_adjacent_to_commoner() {
        // White may castle through f1, which the black commoner on f2 attacks.
        // Touching the enemy commoner makes the white commoner immune there.
        let board = Board::from_fen("8/8/8/8/8/8/5k2/4K2R w K - 0 1").unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        assert!(moves.as_slice().iter().any(|&m| m.to_uci() == "e1g1"));
    }

    #[test]
    fn test_pinned() {
        // Black rook on e4, white pawn on e3, white commoner on e2 - pawn is pinned
        let fen = "4k3/8/8/8/4r3/4P3/4K3/8 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let pinned = board.pinned(Color::White);
        assert!(!pinned.is_empty(), "Expected pinned pieces, got empty");
    }

    #[test]
    fn test_do_undo_restores_state() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let mut board = Board::from_fen(fen).unwrap();
        let orig_fen = board.fen();
        let mut state = StateInfo::new();

        // e4
        let m = Move::make_move(Square::E2, Square::E4);
        board.do_move(m, &mut state);
        board.undo_move(m, &state);

        assert_eq!(board.fen(), orig_fen);
    }

    #[test]
    fn test_do_undo_capture_restores() {
        let fen2 = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
        let mut board2 = Board::from_fen(fen2).unwrap();
        let orig_fen = board2.fen();
        let mut state2 = StateInfo::new();

        let m = Move::make_move(Square::E4, Square::D5);
        board2.do_move(m, &mut state2);
        board2.undo_move(m, &state2);

        assert_eq!(board2.fen(), orig_fen);
    }

    #[test]
    fn test_legal_rejects_moves_on_terminal_board() {
        // Only one commoner remains on the board, so the game is over even
        // though the surviving side has pseudo-legal moves.
        let board = Board::from_fen("8/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let mut moves = MoveList::new();
        crate::movegen::generate_pseudo_legal(&board, &mut moves);
        assert!(!moves.is_empty());
        for &m in moves.as_slice() {
            assert!(!board.legal(m, &state), "move {m:?} must not be legal");
        }
    }

    #[test]
    fn test_self_explosion_illegal() {
        // White's only commoner is on d3; white rook on d5, black pawn on d4.
        // Rook takes pawn on d4 — blast zone includes d3, destroying the
        // commoner → self-explosion, illegal. There is no "spare commoner"
        // exception in atomic chess.
        let fen = "4k3/8/8/3R4/3p4/3K4/8/8 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        for &m in moves.as_slice() {
            assert!(
                m.from_sq() != Square::D5 || m.to_sq() != Square::D4,
                "rook capture on d4 should be illegal (commoner destroyed)"
            );
        }
    }

    #[test]
    fn test_blast_zone_removes_pieces() {
        // White rook on e4, black knight on e5, black pawn on f5
        // Rook captures knight — blast zone around e5 (d4-f4, d5-f5, d6-f6)
        // removes: rook (non-pawn capturer), knight, but NOT the pawn on f5
        let fen = "4k3/8/8/4np2/4R3/8/8/4K3 w - - 0 1";
        let mut board = Board::from_fen(fen).unwrap();
        let mut state = StateInfo::new();
        let m = Move::make_move(Square::E4, Square::E5);
        board.do_move(m, &mut state);
        // The rook and knight should be gone; the black pawn on f5 should remain
        assert!(
            board.piece_on(Square::E4) == NO_PIECE,
            "rook at e4 should be gone"
        );
        assert!(
            board.piece_on(Square::E5) == NO_PIECE,
            "knight at e5 should be gone"
        );
        assert!(
            board.piece_on(Square::F5) == B_PAWN,
            "pawn at f5 should survive"
        );
    }

    #[test]
    fn test_pinned_piece_capture_explodes_pinner() {
        // Black rook on e5 (pinning), white rook on e3, black pawn on e4,
        // white commoner on e1. The rook on e3 is pinned by the rook on e5
        // (both on e-file, commoner on e1 behind).
        // But rook captures pawn on e4 — blast zone (d3-f5) destroys the
        // rook on e5, so the pin is removed and the move is legal.
        let fen = "4k3/8/8/4r3/4p3/4R3/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        let has_rook_e4 = moves
            .as_slice()
            .iter()
            .any(|&m| m.from_sq() == Square::E3 && m.to_sq() == Square::E4);
        assert!(
            has_rook_e4,
            "rook capture on e4 should be legal (blast removes pinning rook)"
        );
    }

    #[test]
    fn test_en_passant_blast() {
        // White pawn on d5, black pawn on c5 (just double-pushed), black knight on d4
        // White plays dxc6 en passant — blast at c6
        let fen2 = "4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2";
        let mut board2 = Board::from_fen(fen2).unwrap();
        let mut state2 = StateInfo::new();
        let m = Move::make_enpassant(Square::C5, Square::D6);
        board2.do_move(m, &mut state2);
        // After EP capture + blast: pawns on c5 and d5 are gone,
        // commoners should remain (out of blast zone)
        assert!(board2.piece_on(Square::C5) == NO_PIECE);
        assert!(board2.piece_on(Square::D5) == NO_PIECE);
    }

    #[test]
    fn test_do_undo_restores_hash() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let mut board = Board::from_fen(fen).unwrap();
        let orig_hash = board.hash();
        let mut state = StateInfo::new();

        let m = Move::make_move(Square::E2, Square::E4);
        board.do_move(m, &mut state);
        assert_eq!(board.hash(), board.recomputed_hash());
        board.undo_move(m, &state);
        assert_eq!(board.hash(), orig_hash);
        assert_eq!(board.hash(), board.recomputed_hash());
    }

    #[test]
    fn test_hash_after_moves() {
        let mut board = Board::new();
        let mut states = [StateInfo::new(); 4];

        let moves = [
            Move::make_move(Square::E2, Square::E4),
            Move::make_move(Square::E7, Square::E5),
            Move::make_move(Square::G1, Square::F3),
            Move::make_move(Square::B8, Square::C6),
        ];
        for (i, &m) in moves.iter().enumerate() {
            board.do_move(m, &mut states[i]);
            assert_eq!(board.hash(), board.recomputed_hash());
        }
        for (i, &m) in moves.iter().enumerate().rev() {
            board.undo_move(m, &states[i]);
            assert_eq!(board.hash(), board.recomputed_hash());
        }
    }

    #[test]
    fn test_fen_fullmove_counter() {
        let mut board = Board::new();
        // Starting position FEN ends with fullmove 1.
        assert!(board.fen().ends_with("0 1"));
        assert_eq!(board.fullmove_number(), 1);

        let mut state = StateInfo::new();
        let m = Move::make_move(Square::E2, Square::E4);
        board.do_move(m, &mut state);
        // After 1. e4 it is still fullmove 1 (Black to move).
        assert!(board.fen().ends_with("0 1"));
        assert_eq!(board.fullmove_number(), 1);
    }

    #[test]
    fn test_from_fen_fullmove_conversion() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.game_ply(), 2);
        assert_eq!(board.fullmove_number(), 1);

        let fen2 = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 5";
        let board2 = Board::from_fen(fen2).unwrap();
        assert_eq!(board2.game_ply(), 9);
        assert_eq!(board2.fullmove_number(), 5);
    }

    #[test]
    fn test_rule50_game_ply_fullmove_getters() {
        let board = Board::new();
        assert_eq!(board.rule50(), 0);
        assert_eq!(board.game_ply(), 1);
        assert_eq!(board.fullmove_number(), 1);
    }

    #[test]
    fn test_outcome_loss() {
        // White has no commoners: loss for White.
        let fen = "4k3/8/8/8/8/8/8/8 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.outcome(), Some(Outcome::Loss));
    }

    #[test]
    fn test_outcome_win() {
        // Black has no commoners: win for White.
        let fen = "8/8/8/8/8/8/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.outcome(), Some(Outcome::Win));
    }

    #[test]
    fn test_outcome_draw_rule50() {
        // Not commoner-only: the clock, not the material shortcut, decides.
        let fen = "4k3/8/8/8/8/8/8/4K2R w - - 100 1";
        let board = Board::from_fen(fen).unwrap();
        assert!(!board.is_material_draw());
        assert_eq!(board.outcome(), Some(Outcome::Draw));
        assert!(board.is_terminal());
    }

    #[test]
    fn test_outcome_draw_stalemate() {
        // White's only commoner is enclosed by black knights and not attacked:
        // every "escape" is a capture that self-destructs it. Stalemate.
        let fen = "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert!(board.checkers().is_empty());
        assert_eq!(board.outcome(), Some(Outcome::Draw));
    }

    #[test]
    fn test_outcome_loss_no_moves_in_check() {
        // Black's commoner on h8 is attacked by the g7 pawn and has no legal
        // moves (g8 is covered by h7, capturing either pawn self-destructs).
        // Unlike a true stalemate, this is a loss for the side to move.
        let fen = "7k/6PP/8/8/8/8/8/7K b - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert!(!board.checkers().is_empty());
        assert_eq!(board.outcome(), Some(Outcome::Loss));
    }

    #[test]
    fn terminal_precedence_table() {
        // One table pins every feedback FEN and the full precedence order.
        // (fen, outcome, legal moves, checkers, is_terminal)
        let cases: &[(&str, Option<Outcome>, usize, u32, bool)] = &[
            // Touching commoners: no check, ordinary moves remain. This is
            // also K vs K, so the WDL shortcut applies; the game is not over
            // before the 50-move rule.
            (
                "8/8/8/8/8/8/4k3/4K3 w - - 0 1",
                Some(Outcome::Draw),
                4,
                0,
                false,
            ),
            // Checkmate, mate must outrank the material/clock branches.
            (
                "7K/8/8/8/8/8/1Q6/k7 b - - 0 1",
                Some(Outcome::Loss),
                0,
                1,
                true,
            ),
            // Checkmate on the clock's last ply is still a loss.
            (
                "7K/8/8/8/8/8/1Q6/k7 b - - 100 1",
                Some(Outcome::Loss),
                0,
                1,
                true,
            ),
            // Opponent already extinct: win and terminal, no legal moves.
            ("8/8/8/8/8/8/8/K7 w - - 0 1", Some(Outcome::Win), 0, 0, true),
            // K vs K is a WDL draw, but the game has not ended yet.
            (
                "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
                Some(Outcome::Draw),
                5,
                0,
                false,
            ),
            // K vs K on the clock is both a WDL draw and literally over.
            (
                "4k3/8/8/8/8/8/8/4K3 w - - 100 1",
                Some(Outcome::Draw),
                5,
                0,
                true,
            ),
            // Artificial both-extinct board: own extinction wins the tie.
            ("8/8/8/8/8/8/8/8 w - - 0 1", Some(Outcome::Loss), 0, 0, true),
        ];

        for &(fen, expected_outcome, legal, checkers, terminal) in cases {
            let board = Board::from_fen(fen).expect("precedence FEN must parse");
            let mut moves = MoveList::new();
            crate::movegen::generate_legal(&board, &mut moves);

            assert_eq!(board.outcome(), expected_outcome, "outcome for {fen}");
            assert_eq!(moves.len(), legal, "legal move count for {fen}");
            assert_eq!(
                board.checkers().count(),
                checkers,
                "checker count for {fen}"
            );
            assert_eq!(
                crate::movegen::has_legal_move(&board),
                legal != 0,
                "has_legal_move for {fen}"
            );
            assert_eq!(board.is_terminal(), terminal, "is_terminal for {fen}");
        }
    }

    fn assert_outcome_equivalence(board: &Board) {
        let mut state = StateInfo::new();
        board.populate_state(&mut state);

        let expected = board.outcome();
        assert_eq!(
            expected,
            board.outcome_with_state(&state),
            "outcome_with_state mismatch at {}",
            board.fen()
        );

        let moves_exist = crate::movegen::has_legal_move_with_state(board, &state);
        assert_eq!(
            expected,
            board.outcome_with(&state, moves_exist),
            "outcome_with mismatch at {}",
            board.fen()
        );
    }

    fn check_outcome_and_playout(
        board: &mut Board,
        rng: &mut Xorshift2,
        depth: u32,
        visited: &mut u64,
    ) {
        *visited += 1;
        assert_outcome_equivalence(board);

        let mut state = StateInfo::new();
        board.populate_state(&mut state);

        let mut list = MoveList::new();
        crate::movegen::generate_legal_with_state(board, &state, &mut list);
        if depth == 0 || list.is_empty() {
            return;
        }

        let idx = (rng.next() % list.len() as u64) as usize;
        let m = list[idx];
        board.do_move(m, &mut state);
        check_outcome_and_playout(board, rng, depth - 1, visited);
        board.undo_move(m, &state);
    }

    #[test]
    fn outcome_state_aware_equivalence() {
        const FIXTURES: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "3k4/8/8/8/8/8/8/3QK3 b - - 0 1",
            "7k/6PP/8/8/8/8/8/7K b - - 0 1",
            "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/4K2R w - - 100 1",
            "8/8/8/8/8/8/8/8 w - - 0 1",
        ];
        for fen in FIXTURES {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            assert_outcome_equivalence(&board);
        }

        const START_FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "3k4/8/8/8/8/8/8/3QK3 b - - 0 1",
        ];

        let mut visited = 0u64;
        for (i, fen) in START_FENS.iter().enumerate() {
            for seed in 0..8u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0xA076_1D64_78BD_642F ^ ((i as u64) << 32) ^ seed);
                check_outcome_and_playout(&mut board, &mut rng, 200, &mut visited);
            }
        }
        assert!(
            visited > 2_000,
            "expected a few thousand positions, got {visited}"
        );
    }

    #[test]
    fn outcome_with_known_move() {
        // Not in check: a caller-supplied `false` is the stalemate branch.
        let board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        assert!(crate::movegen::has_legal_move_with_state(&board, &state));
        assert_eq!(board.outcome_with(&state, true), None);
        assert_eq!(board.outcome_with(&state, false), Some(Outcome::Draw));

        // In check with escapes: a caller-supplied `false` is the mate branch.
        let board = Board::from_fen("3k4/8/8/8/8/8/8/3QK3 b - - 0 1").unwrap();
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        assert!(!state.checkers.is_empty());
        assert!(crate::movegen::has_legal_move_with_state(&board, &state));
        assert_eq!(board.outcome_with(&state, true), None);
        assert_eq!(board.outcome_with(&state, false), Some(Outcome::Loss));
    }

    #[test]
    fn material_draw_is_commoner_only() {
        for fen in [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            // Touching commoners are still bare K vs K.
            "8/8/8/8/8/8/4k3/4K3 w - - 0 1",
        ] {
            let board = Board::from_fen(fen).unwrap();
            assert!(board.is_material_draw(), "{fen}");
            assert_eq!(board.outcome(), Some(Outcome::Draw), "{fen}");
        }

        // Touching commoners are *not* automatically a draw: the shortcut is
        // bare K vs K, not adjacency. Here a black rook remains, so the
        // position is WDL-undecided even though the commoners touch.
        let touching_with_rook = Board::from_fen("8/8/8/8/8/8/4k3/r3K3 w - - 0 1").unwrap();
        assert!(!touching_with_rook.is_material_draw());
        assert_eq!(touching_with_rook.outcome(), None);
        assert!(!touching_with_rook.is_terminal());

        // There is deliberately no insufficient-material table: every one of
        // these positions is an immediate win by capturing the enemy commoner,
        // so `outcome` must stay `None` (WDL undecided) before the capture.
        let counterexamples: &[(&str, Move)] = &[
            (
                "8/8/8/8/8/3B4/4k3/K7 w - - 0 1",
                Move::make_move(Square::D3, Square::E2),
            ),
            (
                "8/8/8/8/3N4/8/4k3/K7 w - - 0 1",
                Move::make_move(Square::D4, Square::E2),
            ),
            (
                "8/8/8/8/8/8/4k3/K3R3 w - - 0 1",
                Move::make_move(Square::E1, Square::E2),
            ),
        ];

        for &(fen, capture) in counterexamples {
            let mut board = Board::from_fen(fen).expect("counterexample FEN must parse");
            assert!(!board.is_material_draw(), "{fen}");
            assert_eq!(board.outcome(), None, "{fen}");

            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            assert!(board.legal(capture, &state), "{fen}: {capture:?}");

            board.do_move(capture, &mut state);
            assert!(board.commoners(Color::Black).is_empty(), "{fen}");
            assert_eq!(board.outcome(), Some(Outcome::Loss), "{fen}");

            let mut moves = MoveList::new();
            crate::movegen::generate_legal(&board, &mut moves);
            assert!(moves.is_empty(), "{fen}");
        }
    }

    #[test]
    fn test_is_capture() {
        let fen = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
        let board = Board::from_fen(fen).unwrap();
        let capture = Move::make_move(Square::E4, Square::D5);
        let non_capture = Move::make_move(Square::E4, Square::E5);
        assert!(board.is_capture(capture));
        assert!(!board.is_capture(non_capture));
    }

    #[test]
    fn legal_ep_square_detects_legal_capture() {
        let board = Board::from_fen("8/8/8/PP6/kP1Pp3/PN2P3/8/6K1 b - d3 0 1").unwrap();
        assert_eq!(board.ep_square(), Some(Square::D3));
        assert_eq!(board.legal_ep_square(), Some(Square::D3));
        assert!(board.is_legal(Move::make_enpassant(Square::E4, Square::D3)));

        // A double push records an ep target even when no legal capture exists.
        let board =
            Board::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2")
                .unwrap();
        assert_eq!(board.ep_square(), Some(Square::D6));
        assert_eq!(board.legal_ep_square(), None);

        let board = Board::from_fen("8/8/8/8/8/8/8/4K2k w - - 0 1").unwrap();
        assert_eq!(board.ep_square(), None);
        assert_eq!(board.legal_ep_square(), None);
    }

    #[test]
    fn ep_transposition_hash() {
        let ep = Board::from_fen("r1bqkbnr/pppppppp/2n5/8/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq e3 0 3")
            .unwrap();
        let plain =
            Board::from_fen("r1bqkbnr/pppppppp/2n5/8/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 0 3")
                .unwrap();

        for idx in 0..64 {
            let sq = Square::from_u8(idx);
            assert_eq!(
                ep.piece_on(sq),
                plain.piece_on(sq),
                "placement differs at {sq:?}"
            );
        }
        assert_eq!(ep.side_to_move(), plain.side_to_move());
        assert_eq!(ep.castling_rights(), plain.castling_rights());

        assert_ne!(ep.hash(), plain.hash());
        assert_ne!(ep.fen(), plain.fen());
        assert!(ep.fen().ends_with("b KQkq e3 0 3"));

        assert_eq!(ep.legal_ep_square(), None);
        assert_eq!(
            ep.hash_with(EnPassantMode::Legal),
            plain.hash_with(EnPassantMode::Legal)
        );
        assert_eq!(
            ep.fen_with(EnPassantMode::Legal),
            plain.fen_with(EnPassantMode::Legal)
        );
        assert!(ep.fen_with(EnPassantMode::Legal).ends_with("b KQkq - 0 3"));
    }

    #[test]
    fn legal_ep_square_respects_pins_and_blast() {
        // e5xd6 would open the e-file from the rook on e8 to the commoner on
        // e1. The blast at d6 does not reach e8, so the pinned pawn cannot
        // capture even though the target and victim pawn are present.
        let pinned = Board::from_fen("4r2k/8/8/3pP3/8/8/8/4K3 w - d6 0 2").unwrap();
        // e5xd6 blasts e7, which holds the mover's own commoner.
        let self_blast = Board::from_fen("7k/4K3/8/3pP3/8/8/8/8 w - d6 0 2").unwrap();

        for board in [pinned, self_blast] {
            assert_eq!(board.ep_square(), Some(Square::D6));
            assert_eq!(board.legal_ep_square(), None);
            assert!(!board.is_legal(Move::make_enpassant(Square::E5, Square::D6)));
        }
    }

    #[test]
    fn ep_default_behavior_unchanged() {
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            "r1bqkbnr/pppppppp/2n5/8/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq e3 0 3",
            "8/8/8/PP6/kP1Pp3/PN2P3/8/6K1 b - d3 0 1",
        ];
        for fen in fens {
            let board = Board::from_fen(fen).unwrap();
            assert_eq!(board.hash(), board.hash_with(EnPassantMode::Always));
            assert_eq!(board.fen(), board.fen_with(EnPassantMode::Always));
        }
    }

    // --- hash_after / rule50_after equivalence tests ---

    /// Assert the `hash_after`/`rule50_after` contract for a single move:
    /// both must match a cloned board's `do_move` result exactly.
    fn assert_after_equivalence(board: &Board, m: Move) {
        let expected_hash = board.hash_after(m);
        let expected_rule50 = board.rule50_after(m);

        let mut played = board.clone();
        let mut state = StateInfo::new();
        played.do_move(m, &mut state);

        assert_eq!(
            expected_hash,
            played.hash(),
            "hash_after mismatch for {} at {}",
            m.to_uci(),
            board.fen()
        );
        assert_eq!(
            expected_rule50,
            played.rule50(),
            "rule50_after mismatch for {} at {}",
            m.to_uci(),
            board.fen()
        );
        // The speculative path must not touch `self`.
        assert_eq!(board.fen(), board.fen()); // sanity; `board` is `&self`
    }

    /// Assert the `hash_after_with` contract for both en-passant policies.
    fn assert_after_with_equivalence(board: &Board, m: Move) {
        for mode in [EnPassantMode::Always, EnPassantMode::Legal] {
            let expected = board.hash_after_with(m, mode);
            let mut played = board.clone();
            let mut state = StateInfo::new();
            played.do_move(m, &mut state);
            assert_eq!(
                expected,
                played.hash_with(mode),
                "hash_after_with({mode:?}) mismatch for {} at {}",
                m.to_uci(),
                board.fen()
            );
        }
    }

    /// Deterministic xorshift64 PRNG (mirrors the movegen harness).
    struct Xorshift2(u64);

    impl Xorshift2 {
        fn new(seed: u64) -> Self {
            Xorshift2(seed | 1)
        }

        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    /// At every visited position, check `hash_after`/`rule50_after` for every
    /// pseudo-legal move against a cloned board's `do_move`, then recurse
    /// along a random legal move (same walk as the movegen harness).
    fn check_after_and_playout(
        board: &mut Board,
        rng: &mut Xorshift2,
        depth: u32,
        visited: &mut u64,
    ) {
        *visited += 1;

        let mut pseudo = MoveList::new();
        crate::movegen::generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            assert_after_equivalence(board, m);
            assert_after_with_equivalence(board, m);
        }

        let mut state = StateInfo::new();
        board.populate_state(&mut state);

        let mut list = MoveList::new();
        crate::movegen::generate_legal_with_state(board, &state, &mut list);

        if depth == 0 || list.is_empty() {
            return;
        }

        let idx = (rng.next() % list.len() as u64) as usize;
        let m = list[idx];
        board.do_move(m, &mut state);
        check_after_and_playout(board, rng, depth - 1, visited);
        board.undo_move(m, &state);
    }

    #[test]
    fn hash_after_equivalence_random_playouts() {
        const START_FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            // Capture-rich middlegame: explosions change occupancy mid-move.
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            // Promotion-heavy position with black pawns about to promote.
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        ];

        let mut visited = 0u64;
        for (i, fen) in START_FENS.iter().enumerate() {
            for seed in 0..16u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0x9E37_79B9_7F4A_7C15 ^ ((i as u64) << 32) ^ seed);
                check_after_and_playout(&mut board, &mut rng, 300, &mut visited);
            }
        }
        assert!(
            visited > 5000,
            "expected several thousand positions, got {visited}"
        );
        assert_eq!(visited, 8_592);
    }

    #[test]
    fn hash_after_deterministic_fixtures() {
        // (name, fen, moves)
        let fixtures: &[(&str, &str, &[Move])] = &[
            (
                "castling_white_kingside",
                "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
                &[Move::make_castling(Square::E1, Square::H1)],
            ),
            (
                "castling_white_queenside",
                "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
                &[Move::make_castling(Square::E1, Square::A1)],
            ),
            (
                "castling_black_kingside",
                "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1",
                &[Move::make_castling(Square::E8, Square::H8)],
            ),
            (
                "castling_black_queenside",
                "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1",
                &[Move::make_castling(Square::E8, Square::A8)],
            ),
            (
                // dxc6 e.p.: the blast at d6 vaporizes the knight on e7.
                "en_passant_blast_removes_piece",
                "4k3/4n3/8/2Pp4/8/8/8/4K3 w - d6 0 2",
                &[Move::make_enpassant(Square::C5, Square::D6)],
            ),
            (
                // Double pawn push b2b4 replaces the existing ep square d6.
                "double_push_replaces_ep",
                "4k3/8/8/3p4/8/8/PP6/4K3 w - d6 0 2",
                &[Move::make_move(Square::B2, Square::B4)],
            ),
            (
                // Quiet promotions b7b8 and capture promotions b7xa8/b7xc8,
                // all four promotion pieces.
                "promotions",
                "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
                &[
                    Move::make_promotion(Square::B7, Square::B8, PieceType::Queen),
                    Move::make_promotion(Square::B7, Square::B8, PieceType::Rook),
                    Move::make_promotion(Square::B7, Square::B8, PieceType::Bishop),
                    Move::make_promotion(Square::B7, Square::B8, PieceType::Knight),
                    Move::make_promotion(Square::B7, Square::A8, PieceType::Queen),
                    Move::make_promotion(Square::B7, Square::A8, PieceType::Rook),
                    Move::make_promotion(Square::B7, Square::A8, PieceType::Bishop),
                    Move::make_promotion(Square::B7, Square::A8, PieceType::Knight),
                    Move::make_promotion(Square::B7, Square::C8, PieceType::Queen),
                    Move::make_promotion(Square::B7, Square::C8, PieceType::Rook),
                    Move::make_promotion(Square::B7, Square::C8, PieceType::Bishop),
                    Move::make_promotion(Square::B7, Square::C8, PieceType::Knight),
                ],
            ),
            (
                // qxb2: the blast at b2 removes the corner rook on a1, so the
                // white queen-side right is lost via the presence cascade.
                "blast_removes_corner_rook",
                "r3k2r/8/8/8/8/q7/1Q6/R3K3 b Qkq - 0 1",
                &[Move::make_move(Square::A3, Square::B2)],
            ),
            (
                // qxe2: the blast at e2 removes the commoner on e1, losing
                // both white rights via the presence cascade.
                "blast_removes_e1_commoner",
                "4k3/8/8/8/8/3q4/4P3/4K2R b K - 0 1",
                &[Move::make_move(Square::D3, Square::E2)],
            ),
            (
                // Qxe7: the blast at e7 removes the commoner on e8, losing
                // both black rights via the presence cascade.
                "blast_removes_e8_commoner",
                "4k3/3Qp3/8/8/8/8/8/4K3 w - - 0 1",
                &[Move::make_move(Square::D7, Square::E7)],
            ),
        ];

        for (name, fen, moves) in fixtures {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            for &m in *moves {
                assert_after_equivalence(&board, m);
            }
            let _ = name;
        }
    }

    #[test]
    fn rule50_after_fixtures() {
        // Reset by a pawn move.
        let board = Board::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 23 40").unwrap();
        assert_eq!(
            board.rule50_after(Move::make_move(Square::E2, Square::E4)),
            0
        );

        // Reset by a capture.
        let board = Board::from_fen("4k3/8/8/8/8/3q4/4P3/4K2R b K - 41 60").unwrap();
        assert_eq!(
            board.rule50_after(Move::make_move(Square::D3, Square::E2)),
            0
        );

        // Reset by an en-passant capture (a pawn move).
        let board = Board::from_fen("4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2").unwrap();
        assert_eq!(
            board.rule50_after(Move::make_enpassant(Square::C5, Square::D6)),
            0
        );

        // Incremented by castling.
        let board = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 7 30").unwrap();
        assert_eq!(
            board.rule50_after(Move::make_castling(Square::E1, Square::G1)),
            8
        );

        // Incremented by a quiet piece move.
        let board = Board::from_fen("4k3/8/8/8/8/8/8/4K2R w - - 7 30").unwrap();
        assert_eq!(
            board.rule50_after(Move::make_move(Square::H1, Square::H2)),
            8
        );
    }

    /// Positions covering every branch of the trivial-legality ladder.
    const TRIVIAL_LEGALITY_CORPUS: &[&str] = &[
        // Startpos and the `tests/move_order_golden.rs` positions.
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        // En passant available for the side to move.
        "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        "4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2",
        // Castling available.
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        // Side to move in check.
        "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        "3k4/8/8/8/8/8/8/3QK3 b - - 0 1",
        // Pinned pieces (rook and bishop pins).
        "4k3/8/8/8/8/4r3/4N3/4K3 w - - 0 1",
        "4k3/8/b7/8/8/3N4/8/5K2 w - - 0 1",
        // Side to move has no commoner (empty candidate stream).
        "8/8/8/8/8/8/8/k7 w - - 0 1",
        // Bare commoners only.
        "k7/8/8/8/8/8/8/7K w - - 0 1",
        // The two profiled positions.
        "rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5",
        "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15",
    ];

    /// Verbatim copy of the pre-refactor `is_move_trivially_legal` ladder, used
    /// as the reference implementation for the equivalence test below.
    fn is_move_trivially_legal_reference(board: &Board, m: Move, state: &StateInfo) -> bool {
        if !state.checkers.is_empty() {
            return false;
        }
        if state.commoners_count == 0 {
            return false;
        }

        let from = m.from_sq();
        let us = board.side_to_move();
        if board.piece_on(from) == make_piece(us, PieceType::Commoner) {
            return false;
        }

        let mt = m.move_type();
        if mt == MoveType::EnPassant {
            return false;
        }

        // A non-Castling move that captures a piece (EnPassant already handled above).
        let is_capture = mt != MoveType::Castling && board.piece_on(m.to_sq()) != NO_PIECE;
        if is_capture {
            return false;
        }

        // Castling reaches here (not a capture, not en-passant, not a commoner move).
        // Castling still needs the full pass-through check, so reject fast-path.
        if mt == MoveType::Castling {
            return false;
        }

        // Check pin: a pinned piece might expose a commoner.
        if (state.pinned & Bitboard::square_bb(from)) != Bitboard::EMPTY {
            return false;
        }

        true
    }

    #[test]
    fn test_trivial_legality_matches_reference_ladder() {
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).unwrap();
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let trivial = TrivialLegality::new(&board, &state);

            let mut moves = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut moves);

            for &m in moves.as_slice() {
                assert_eq!(
                    trivial.is_trivially_legal(&board, m),
                    is_move_trivially_legal_reference(&board, m, &state),
                    "fast path diverged on FEN {fen}, move {m:?}"
                );
            }
        }
    }

    #[test]
    fn test_trivial_legality_accepts_only_legal_moves() {
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).unwrap();
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let trivial = TrivialLegality::new(&board, &state);

            let mut moves = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut moves);

            for &m in moves.as_slice() {
                if trivial.is_trivially_legal(&board, m) {
                    assert!(
                        board.legal(m, &state),
                        "fast path accepted an illegal move on FEN {fen}, move {m:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_trivial_legality_corpus_covers_accept_and_reject() {
        // Guard against a vacuous equivalence test: the corpus must exercise
        // both fast-path acceptance and rejection.
        let mut accepted = 0;
        let mut rejected = 0;
        let mut with_checkers = 0;
        let mut with_pins = 0;
        let mut with_special_moves = 0;
        let mut without_commoner = 0;
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).unwrap();
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let trivial = TrivialLegality::new(&board, &state);
            if !state.checkers.is_empty() {
                with_checkers += 1;
            }
            if !state.pinned.is_empty() {
                with_pins += 1;
            }
            if state.commoners_count == 0 {
                without_commoner += 1;
            }
            let mut moves = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut moves);
            if moves.as_slice().iter().any(|m| m.needs_full_legal_check()) {
                with_special_moves += 1;
            }
            for &m in moves.as_slice() {
                if trivial.is_trivially_legal(&board, m) {
                    accepted += 1;
                } else {
                    rejected += 1;
                }
            }
        }
        assert!(accepted > 0, "corpus never accepts a move");
        assert!(rejected > 0, "corpus never rejects a move");
        assert!(
            with_checkers > 0,
            "corpus never exercises the checker guard"
        );
        assert!(with_pins > 0, "corpus never exercises the pin guard");
        assert!(
            with_special_moves > 0,
            "corpus never exercises en passant or castling"
        );
        assert!(
            without_commoner > 0,
            "corpus never exercises the no-commoner guard"
        );
    }

    // --- single-move predicates and UCI lookup ---

    /// Assert both single-move predicates against the generated lists for every
    /// pseudo-legal move of `board`.
    fn assert_move_predicates(board: &Board) {
        let mut state = StateInfo::new();
        board.populate_state(&mut state);

        let mut legal = MoveList::new();
        crate::movegen::generate_legal_with_state(board, &state, &mut legal);

        let mut pseudo = MoveList::new();
        crate::movegen::generate_pseudo_legal(board, &mut pseudo);

        for &m in pseudo.as_slice() {
            assert!(
                board.is_pseudo_legal(m),
                "pseudo generator emitted {} which the predicate rejects at {}",
                m.to_uci(),
                board.fen()
            );
            let expected = legal.as_slice().contains(&m);
            assert_eq!(
                board.is_legal_with_state(m, &state),
                expected,
                "is_legal_with_state mismatch for {} at {}",
                m.to_uci(),
                board.fen()
            );
            assert_eq!(
                board.is_legal(m),
                expected,
                "is_legal mismatch for {} at {}",
                m.to_uci(),
                board.fen()
            );
        }
    }

    fn check_move_predicate_playout(
        board: &mut Board,
        rng: &mut Xorshift2,
        depth: u32,
        visited: &mut u64,
    ) {
        *visited += 1;
        assert_move_predicates(board);

        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let mut list = MoveList::new();
        crate::movegen::generate_legal_with_state(board, &state, &mut list);
        if depth == 0 || list.is_empty() {
            return;
        }

        let idx = (rng.next() % list.len() as u64) as usize;
        let m = list[idx];
        board.do_move(m, &mut state);
        check_move_predicate_playout(board, rng, depth - 1, visited);
        board.undo_move(m, &state);
    }

    const PREDICATE_START_FENS: &[&str] = &[
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
        "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    ];

    #[test]
    fn is_pseudo_legal_matches_generator() {
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            assert_move_predicates(&board);
        }

        let mut visited = 0u64;
        for (i, fen) in PREDICATE_START_FENS.iter().enumerate() {
            for seed in 0..4u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0xD1B5_4A32_D192_ED03 ^ ((i as u64) << 32) ^ seed);
                check_move_predicate_playout(&mut board, &mut rng, 80, &mut visited);
            }
        }
        assert!(visited > 500, "only visited {visited} positions");
    }

    #[test]
    fn is_pseudo_legal_rejects_malformed_moves() {
        let start = Board::new();
        let malformed = [
            Move::NONE,
            Move::make_move(Square::E2, Square::E2),
            Move::make_move(Square::E7, Square::E5),
            Move::make_move(Square::E4, Square::E5),
            Move::make_move(Square::E2, Square::E5),
            Move::make_move(Square::E2, Square::D3),
            Move::make_move(Square::G1, Square::E2),
            Move::make_move(Square::A1, Square::A3),
            Move::make_promotion(Square::E2, Square::E4, PieceType::Queen),
            Move::make_promotion(Square::A2, Square::A4, PieceType::Queen),
            Move::make_enpassant(Square::E2, Square::E4),
            // Castling is blocked in the start position.
            Move::make_castling(Square::E1, Square::H1),
        ];
        for m in malformed {
            assert!(
                !start.is_pseudo_legal(m),
                "start position accepted {}",
                m.to_uci()
            );
        }

        // Rights missing: the internal castling encoding is not pseudo-legal.
        let no_rights = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").unwrap();
        assert!(!no_rights.is_pseudo_legal(Move::make_castling(Square::E1, Square::H1)));
        assert!(!no_rights.is_pseudo_legal(Move::make_castling(Square::E1, Square::A1)));

        // Rights present but the path is occupied.
        let blocked = Board::from_fen("r3k2r/8/8/8/8/8/8/R3KB1R w KQkq - 0 1").unwrap();
        assert!(!blocked.is_pseudo_legal(Move::make_castling(Square::E1, Square::H1)));

        // Castling destinations use the internal rook-square encoding.
        let castling = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        assert!(castling.is_pseudo_legal(Move::make_castling(Square::E1, Square::H1)));
        assert!(castling.is_pseudo_legal(Move::make_castling(Square::E1, Square::A1)));
        assert!(!castling.is_pseudo_legal(Move::make_castling(Square::E1, Square::G1)));

        // En passant must target the actual ep square and have a captured pawn.
        let ep = Board::from_fen("4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2").unwrap();
        assert!(ep.is_pseudo_legal(Move::make_enpassant(Square::C5, Square::D6)));
        assert!(!ep.is_pseudo_legal(Move::make_enpassant(Square::C5, Square::E6)));
        assert!(!ep.is_pseudo_legal(Move::make_enpassant(Square::C5, Square::D5)));

        // A normal move may not replace a promotion, and promotions need the
        // correct origin and destination ranks.
        let promotion = Board::from_fen("k7/4P3/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(!promotion.is_pseudo_legal(Move::make_move(Square::E7, Square::E8)));
        assert!(promotion.is_pseudo_legal(Move::make_promotion(
            Square::E7,
            Square::E8,
            PieceType::Queen
        )));
        assert!(!promotion.is_pseudo_legal(Move::make_promotion(
            Square::E6,
            Square::E7,
            PieceType::Queen
        )));
    }

    #[test]
    fn is_legal_matches_generated_list() {
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            assert_move_predicates(&board);
        }

        let mut visited = 0u64;
        for (i, fen) in PREDICATE_START_FENS.iter().enumerate() {
            for seed in 0..4u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0xA076_1D64_78BD_642F ^ ((i as u64) << 32) ^ seed);
                check_move_predicate_playout(&mut board, &mut rng, 80, &mut visited);
            }
        }
        assert!(visited > 500, "only visited {visited} positions");
    }

    #[test]
    fn is_legal_classifies_all_wire_codes() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            "4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2",
            "k7/4P3/8/8/8/8/8/4K3 w - - 0 1",
            "8/8/8/8/8/8/8/k7 w - - 0 1",
        ] {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let mut legal = MoveList::new();
            crate::movegen::generate_legal_with_state(&board, &state, &mut legal);

            for bits in 0..=u16::MAX {
                let Some(m) = Move::from_bits(bits) else {
                    continue;
                };
                let expected = legal.as_slice().contains(&m);
                assert_eq!(
                    board.is_legal_with_state(m, &state),
                    expected,
                    "wire code {bits:#06x} ({}) at {fen}",
                    m.to_uci()
                );
            }
        }
    }

    #[test]
    fn is_legal_covers_every_move_type() {
        let cases = [
            (
                "quiet and double push",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                Move::make_move(Square::E2, Square::E4),
            ),
            (
                "capture",
                "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1",
                Move::make_move(Square::E4, Square::D5),
            ),
            (
                "en passant",
                "4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2",
                Move::make_enpassant(Square::C5, Square::D6),
            ),
            (
                "promotion",
                "k7/4P3/8/8/8/8/8/4K3 w - - 0 1",
                Move::make_promotion(Square::E7, Square::E8, PieceType::Knight),
            ),
            (
                "castling",
                "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
                Move::make_castling(Square::E1, Square::H1),
            ),
        ];

        for (name, fen, m) in cases {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let mut legal = MoveList::new();
            crate::movegen::generate_legal(&board, &mut legal);
            assert!(board.is_pseudo_legal(m), "{name} should be pseudo-legal");
            assert!(board.is_legal(m), "{name} should be legal");
            assert!(legal.as_slice().contains(&m), "{name} should be generated");
            assert_eq!(
                board.move_from_uci(&m.to_uci()),
                Some(m),
                "{name} should round-trip through UCI"
            );
        }
    }

    #[test]
    fn move_from_uci_resolves_castling() {
        let white = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        assert_eq!(
            white.move_from_uci("e1g1"),
            Some(Move::make_castling(Square::E1, Square::H1))
        );
        assert_eq!(
            white.move_from_uci("e1c1"),
            Some(Move::make_castling(Square::E1, Square::A1))
        );

        let black = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1").unwrap();
        assert_eq!(
            black.move_from_uci("e8g8"),
            Some(Move::make_castling(Square::E8, Square::H8))
        );
        assert_eq!(
            black.move_from_uci("e8c8"),
            Some(Move::make_castling(Square::E8, Square::A8))
        );

        // The resolved move must be the internal king-takes-rook encoding,
        // and its UCI text maps the rook square back to the castled square.
        let resolved = white.move_from_uci("e1g1").unwrap();
        assert_eq!(resolved.to_sq(), Square::H1);
        assert_eq!(resolved.to_uci(), "e1g1");
    }

    #[test]
    fn move_from_uci_rejects_illegal() {
        let start = Board::new();
        for text in ["e7e5", "0000", "e2e2", "e2e4x", "a0a1", "e2e"] {
            assert_eq!(start.move_from_uci(text), None, "accepted {text:?}");
        }
        assert_eq!(
            start.move_from_uci("e2e4"),
            Some(Move::make_move(Square::E2, Square::E4))
        );

        let no_rights = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").unwrap();
        assert_eq!(no_rights.move_from_uci("e1g1"), None);
        assert_eq!(no_rights.move_from_uci("e1c1"), None);

        let blocked = Board::from_fen("r3k2r/8/8/8/8/8/8/R3KB1R w KQkq - 0 1").unwrap();
        assert_eq!(blocked.move_from_uci("e1g1"), None);

        // Pseudo-legal castling through an attacked square is still illegal.
        let through_check = Board::from_fen("4kr2/8/8/8/8/8/8/4K2R w K - 0 1").unwrap();
        assert!(through_check.is_pseudo_legal(Move::make_castling(Square::E1, Square::H1)));
        assert_eq!(through_check.move_from_uci("e1g1"), None);

        // Illegal promotion: the square is not reachable by a pawn.
        let no_pawn = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(no_pawn.move_from_uci("e7e8q"), None);
    }

    #[test]
    fn move_from_uci_promotion_and_ep() {
        let promotion = Board::from_fen("k7/4P3/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(
            promotion.move_from_uci("e7e8q"),
            Some(Move::make_promotion(
                Square::E7,
                Square::E8,
                PieceType::Queen
            ))
        );
        assert_eq!(
            promotion.move_from_uci("e7e8n"),
            Some(Move::make_promotion(
                Square::E7,
                Square::E8,
                PieceType::Knight
            ))
        );
        assert_eq!(promotion.move_from_uci("e7e8k"), None);

        let ep = Board::from_fen("4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2").unwrap();
        assert_eq!(
            ep.move_from_uci("c5d6"),
            Some(Move::make_enpassant(Square::C5, Square::D6))
        );
        assert_eq!(ep.move_from_uci("c5e6"), None);
    }

    fn check_uci_lookup_playout(
        board: &mut Board,
        rng: &mut Xorshift2,
        depth: u32,
        visited: &mut u64,
    ) {
        *visited += 1;

        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let mut list = MoveList::new();
        crate::movegen::generate_legal_with_state(board, &state, &mut list);

        for &m in list.as_slice() {
            let text = m.to_uci();
            assert_eq!(
                board.move_from_uci(&text),
                Some(m),
                "UCI lookup for {text} at {}",
                board.fen()
            );
        }

        if depth == 0 || list.is_empty() {
            return;
        }
        let idx = (rng.next() % list.len() as u64) as usize;
        let m = list[idx];
        board.do_move(m, &mut state);
        check_uci_lookup_playout(board, rng, depth - 1, visited);
        board.undo_move(m, &state);
    }

    #[test]
    fn move_from_uci_matches_full_list_lookup() {
        let mut visited = 0u64;
        for (i, fen) in PREDICATE_START_FENS.iter().enumerate() {
            for seed in 0..4u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0x9E37_79B9_7F4A_7C15 ^ ((i as u64) << 32) ^ seed);
                check_uci_lookup_playout(&mut board, &mut rng, 100, &mut visited);
            }
        }
        assert!(visited > 500, "only visited {visited} positions");
    }
}
