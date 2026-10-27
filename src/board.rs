//! Board representation, FEN parsing, move make/unmake, and legality checking
//! for atomic chess.

use crate::attacks;
use crate::bitboard::*;
use crate::types::*;
use std::error::Error;
use std::fmt;

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
    /// Enemy pieces attacking the side to move's commoners (hot).
    pub checkers: Bitboard,
    /// Pieces of the side to move that are pinned to a commoner (hot).
    pub pinned: Bitboard,
    /// Number of commoners the side to move has on the board (hot).
    pub commoners_count: u32,
    /// Number of commoners the opponent has on the board (hot).
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
    /// and their lower-case forms). The commoner — the king-like pseudo-royal
    /// piece of atomic chess — uses the standard FEN letter `K`/`k`; the
    /// non-standard `C`/`c` labels are rejected.
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

    /// Serialize the board to a FEN string.
    #[must_use]
    pub fn fen(&self) -> String {
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
        match self.ep_square {
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

    /// Return the en-passant target square, if any.
    #[must_use]
    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
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
    /// The hash includes pieces, side to move, castling rights, and en-passant
    /// file, but not the half-move clock or fullmove counter.
    #[inline]
    #[must_use]
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Zobrist hash of the position after `m`, without modifying `self`.
    ///
    /// Exactly equal to
    /// `{ let mut b = self.clone(); b.do_move(m, &mut st); b.hash() }` for
    /// every move `m` that [`do_move`](Self::do_move) accepts in this position
    /// (every pseudo-legal move). Cheaper than a `do_move`/`undo_move`
    /// round-trip: no board, bitboard, or undo-record writes.
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

    /// Return the bitboard of commoners (king-like pseudo-royal pieces) for a color.
    #[inline(always)]
    #[must_use]
    pub fn commoners(&self, c: Color) -> Bitboard {
        self.pieces_color_pt(c, PieceType::Commoner)
    }

    /// Return the game outcome from the side-to-move perspective, if the game
    /// is over.
    ///
    /// Commoner extinction for the side to move is a loss, extinction of the
    /// opponent is a win, the 50-move rule (100 plies) is a draw, and a position
    /// with no legal moves is a draw by stalemate.
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        let us = self.side_to_move;
        if self.commoners(us).is_empty() {
            return Some(Outcome::Loss);
        }
        if self.commoners(us.flip()).is_empty() {
            return Some(Outcome::Win);
        }
        if self.rule50 >= 100 {
            return Some(Outcome::Draw);
        }
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(self, &mut moves);
        if moves.is_empty() {
            return Some(Outcome::Draw);
        }
        None
    }

    /// Returns `true` if the game is over.
    #[inline]
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.outcome().is_some()
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
        if commoners.is_empty() {
            return Bitboard::EMPTY;
        }

        let mut checkers = Bitboard::EMPTY;
        let occupied = self.occupied();

        let them_bb = self.pieces_color(them);
        let mut c = commoners;
        while !c.is_empty() {
            let ksq = c.pop_lsb();
            let rook_atk = attacks::rook_attacks(ksq, occupied);
            let bishop_atk = attacks::bishop_attacks(ksq, occupied);
            let queen_atk = rook_atk | bishop_atk;
            checkers = checkers
                | (rook_atk & self.by_type[PieceType::Rook as usize] & them_bb)
                | (bishop_atk & self.by_type[PieceType::Bishop as usize] & them_bb)
                | (queen_atk & self.by_type[PieceType::Queen as usize] & them_bb)
                | (attacks::knight_attacks(ksq)
                    & self.by_type[PieceType::Knight as usize]
                    & them_bb)
                | (attacks::pawn_attacks(us, ksq)
                    & self.by_type[PieceType::Pawn as usize]
                    & them_bb);
        }

        // Adjacent commoner check (extinction pseudo-royal)
        let them_commoners = self.commoners(them);
        if !them_commoners.is_empty() && !commoners.is_empty() {
            let mut tc = them_commoners;
            while !tc.is_empty() {
                let tksq = tc.pop_lsb();
                if attacks::king_attacks(tksq) & commoners != Bitboard::EMPTY {
                    checkers = checkers | Bitboard::square_bb(tksq);
                }
            }
        }

        checkers
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
    /// safety, and commoner extinction. The last commoner is immune from
    /// adjacent-commoner checks; touching an enemy commoner is allowed and does
    /// not count as an attack.
    ///
    /// `state` must contain up-to-date cached fields for the current position.
    #[must_use]
    pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
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
            for &sq in &pass_through {
                let adjacent_enemy_commoners = self.commoners(them) & attacks::king_attacks(sq);
                if adjacent_enemy_commoners.is_empty() {
                    let enemy_commoner_attack = attacks::king_attacks(sq)
                        & self.by_type[PieceType::Commoner as usize]
                        & self.by_color[them as usize];
                    if square_attacked_by(self, sq, occupied, self.by_color[them as usize])
                        || !enemy_commoner_attack.is_empty()
                    {
                        return false;
                    }
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

        let mut our_commoners = self.commoners(us) & occupied;

        if !is_capture && piece == make_piece(us, PieceType::Commoner) {
            our_commoners = our_commoners | Bitboard::square_bb(kto);
        }

        if our_commoners.is_empty() {
            return false;
        }

        let our_pr_count = state.commoners_count as usize;
        let them_pr_count = state.them_commoners_count as usize;

        if our_pr_count <= 1 {
            let enemy_pr_destroyed =
                them_pr_count <= 1 && (self.commoners(them) & occupied).is_empty();

            if !enemy_pr_destroyed {
                let them_commoners = self.commoners(them);
                let enemy_survivors = self.by_color[them as usize] & occupied;

                let mut c = our_commoners;
                while !c.is_empty() {
                    let ksq = c.pop_lsb();
                    let adjacent_enemy = them_commoners & attacks::king_attacks(ksq);
                    if adjacent_enemy.is_empty()
                        && square_attacked_by(self, ksq, occupied, enemy_survivors)
                    {
                        return false;
                    }
                }
            }
        }

        true
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
    fn test_checkers() {
        // Position where white queen gives check to black commoner
        let fen = "4k3/8/8/8/8/8/8/4Q2K b - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let checkers = board.checkers();
        assert!(!checkers.is_empty(), "Expected checkers, got empty");
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
    fn test_self_explosion_legal_with_surviving_commoner() {
        // White commoners on d3 AND e1; white rook on d5, black pawn on d4.
        // Rook takes pawn on d4 — blast zone (c3-e5) includes d3, destroying
        // the commoner on d3, but the commoner on e1 survives.
        // This is NOT self-explosion — only the LAST commoner being destroyed
        // makes a move illegal.
        let fen = "4k3/8/8/3R4/3p4/3K4/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        // The capture should be LEGAL because e1 survives and is not under attack.
        let has_rook_d4 = moves
            .as_slice()
            .iter()
            .any(|&m| m.from_sq() == Square::D5 && m.to_sq() == Square::D4);
        assert!(
            has_rook_d4,
            "rook capture on d4 should be legal (e1 commoner survives)"
        );
    }

    #[test]
    fn test_self_explosion_illegal_last_commoner() {
        // White commoner ONLY on d3; white rook on d5, black pawn on d4.
        // Rook takes pawn on d4 — blast zone includes d3, destroying the
        // LAST (only) commoner → self-explosion, illegal.
        let fen = "4k3/8/8/3R4/3p4/3K4/8/8 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        for &m in moves.as_slice() {
            assert!(
                m.from_sq() != Square::D5 || m.to_sq() != Square::D4,
                "rook capture on d4 should be illegal (last commoner destroyed)"
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
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 100 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.outcome(), Some(Outcome::Draw));
    }

    #[test]
    fn test_outcome_draw_stalemate() {
        // Black commoner on h8 is boxed in by white pawns on g7 and h7;
        // g8 is attacked by h7, and capturing either pawn destroys the last commoner.
        let fen = "7k/6PP/8/8/8/8/8/7K b - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.outcome(), Some(Outcome::Draw));
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
}
