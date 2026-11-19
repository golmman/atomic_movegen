//! Board representation, FEN parsing, move make/unmake, and legality checking
//! for atomic chess.

use crate::attacks;
use crate::bitboard::*;
use crate::position::{LegalNode, PositionView};
use crate::types::*;
use std::cell::{Cell, OnceCell};
use std::error::Error;
use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

/// Errors that can occur when parsing a FEN string.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The side to move is in a check that the last move could not have given.
    ///
    /// This mirrors the shakmaty/`Setup` impossible-check class used by
    /// [`FenPolicy::Lichess`]. The atomic rules make most configurations that
    /// would be impossible in standard chess reachable after an explosion, so
    /// this flag is only raised when a stored en-passant target makes the
    /// double-push history explicit.
    pub const IMPOSSIBLE_CHECK: Self = Self(1 << 10);

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
        const NAMES: [(ValidationErrors, &str); 11] = [
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
            (ValidationErrors::IMPOSSIBLE_CHECK, "IMPOSSIBLE_CHECK"),
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
    /// Mirror the shakmaty/`Setup` acceptance used by the downstream's
    /// `parity` predicate.
    ///
    /// Unlike the other policies, this policy both **drops** and **rejects**:
    ///
    /// - a castling right not backed by the required commoner and rook on its
    ///   home square is dropped, keeping every other valid right;
    /// - an en-passant target not consistent with a double push is dropped;
    /// - the conservative material bound is not a rejection;
    /// - an empty board, a missing or extra commoner that the atomic
    ///   `Setup` constructor rejects, a pawn on the back ranks, the side that
    ///   just moved being in check, and the atomic impossible-check class are
    ///   rejected.
    ///
    /// The name refers to the downstream parity predicate, not to every
    /// behavior of the Lichess site. Syntax errors are still reported as
    /// [`FenError`], and the parser keeps the crate's standard FEN grammar
    /// (4 or 6 fields, `KQkq` castling letters, `K`/`k` commoners). Use
    /// [`Board::from_fen`] for the permissive parser.
    Lichess,
}

/// Error returned by [`Board::from_fen_with`].
///
/// Syntax errors keep the existing [`FenError`] type; semantic failures are
/// reported separately so downstream code can distinguish malformed FEN text
/// from a well-formed but invalid position.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// The syntax-only result of parsing a FEN string.
///
/// Castling rights and the en passant target are stored raw: the parser does
/// not require them to be backed by pieces on the board or by a double push.
/// [`Board::from_fen`] applies those structural checks itself, and
/// [`FenPolicy::Lichess`] sanitizes the raw values instead.
struct ParsedFen {
    squares: [Piece; 64],
    side_to_move: Color,
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u16,
    game_ply: u32,
}

/// Parse the field count, placement, side, castling characters, and the
/// en-passant token syntax.
///
/// Clocks are left at their defaults; call [`parse_fen_clocks`] once the
/// caller has finished its structural checks so that the existing
/// [`Board::from_fen`] error precedence is preserved.
fn parse_fen_prefix(fen: &str) -> Result<ParsedFen, FenError> {
    let parts: Vec<&str> = fen.split_whitespace().collect();
    if parts.len() < 4 {
        return Err(FenError::TooShort { parts: parts.len() });
    }
    if parts.len() != 4 && parts.len() != 6 {
        return Err(FenError::InvalidFieldCount { got: parts.len() });
    }

    let mut parsed = ParsedFen {
        squares: [NO_PIECE; 64],
        side_to_move: Color::White,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
    };

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
                parsed.squares[rank_idx * 8 + col] = piece;
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

    parsed.side_to_move = match parts[1] {
        "w" => Color::White,
        "b" => Color::Black,
        _ => {
            return Err(FenError::InvalidSideToMove(parts[1].to_string()));
        }
    };

    for c in parts[2].chars() {
        match c {
            'K' => parsed.castling_rights |= WK_CASTLE,
            'Q' => parsed.castling_rights |= WQ_CASTLE,
            'k' => parsed.castling_rights |= BK_CASTLE,
            'q' => parsed.castling_rights |= BQ_CASTLE,
            '-' => break,
            _ => {
                return Err(FenError::InvalidCastling(c.to_string()));
            }
        }
    }

    parsed.ep_square = if parts[3] == "-" {
        None
    } else {
        Some(
            crate::types::parse_sq(parts[3])
                .ok_or_else(|| FenError::InvalidEpSquare(parts[3].to_string()))?,
        )
    };

    Ok(parsed)
}

/// Parse the half-move clock and full-move number.
///
/// The field count is expected to have been validated by
/// [`parse_fen_prefix`]. Keeping the clocks out of that function preserves
/// [`Board::from_fen`]'s existing error order: castling and en-passant
/// geometry errors are reported before clock errors.
fn parse_fen_clocks(fen: &str, side_to_move: Color) -> Result<(u16, u32), FenError> {
    let parts: Vec<&str> = fen.split_whitespace().collect();

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
        2 * u32::from(fm - 1) + if side_to_move == Color::White { 1 } else { 2 }
    } else if side_to_move == Color::White {
        1
    } else {
        2
    };

    Ok((rule50, game_ply))
}

/// Parse a FEN string including its clocks, without applying castling-backing
/// or en-passant geometry checks.
fn parse_fen_syntax(fen: &str) -> Result<ParsedFen, FenError> {
    let mut parsed = parse_fen_prefix(fen)?;
    let (rule50, game_ply) = parse_fen_clocks(fen, parsed.side_to_move)?;
    parsed.rule50 = rule50;
    parsed.game_ply = game_ply;
    Ok(parsed)
}

/// Return `true` when every castling right in `rights` is backed by the
/// required commoner and rook on their home squares.
///
/// This is the single source of truth for the structural castling predicate
/// shared by [`Board::from_fen`], [`Board::from_setup`](crate::Board::from_setup)
/// and [`Board::validation_errors`]. It says nothing about whether the side
/// may actually castle (the path must also be clear and safe).
pub(crate) fn castling_rights_backed_by_pieces(rights: u8, squares: &[Piece; 64]) -> bool {
    let white_commoner = make_piece(Color::White, PieceType::Commoner);
    let white_rook = make_piece(Color::White, PieceType::Rook);
    let black_commoner = make_piece(Color::Black, PieceType::Commoner);
    let black_rook = make_piece(Color::Black, PieceType::Rook);

    (rights & WK_CASTLE == 0
        || (squares[Square::E1 as usize] == white_commoner
            && squares[Square::H1 as usize] == white_rook))
        && (rights & WQ_CASTLE == 0
            || (squares[Square::E1 as usize] == white_commoner
                && squares[Square::A1 as usize] == white_rook))
        && (rights & BK_CASTLE == 0
            || (squares[Square::E8 as usize] == black_commoner
                && squares[Square::H8 as usize] == black_rook))
        && (rights & BQ_CASTLE == 0
            || (squares[Square::E8 as usize] == black_commoner
                && squares[Square::A8 as usize] == black_rook))
}

/// Return `true` when the side that just moved has a pawn on the en-passant
/// victim square for the stored target.
///
/// This is the permissive parser/setup predicate. It deliberately does not
/// require the target or the double-push origin to be empty; the stricter
/// [`ep_geometry_is_consistent`] is used by `validate()` and by the Lichess
/// sanitizer.
pub(crate) fn ep_victim_pawn_present(
    squares: &[Piece; 64],
    side_to_move: Color,
    sq: Square,
) -> bool {
    let victim = match side_to_move {
        Color::White => Square::from_index(sq as i8 - 8),
        Color::Black => Square::from_index(sq as i8 + 8),
    };
    squares[victim as usize] == make_piece(side_to_move.flip(), PieceType::Pawn)
}

/// Return `true` when `sq` is consistent with a double push by the side that
/// just moved.
///
/// This is the full structural predicate: target on rank 6/3, target empty,
/// the enemy pawn on the victim square behind it, and the double-push origin
/// empty.
fn ep_geometry_is_consistent(squares: &[Piece; 64], side_to_move: Color, sq: Square) -> bool {
    let expected_rank = match side_to_move {
        Color::White => Rank::R6,
        Color::Black => Rank::R3,
    };
    if rank_of(sq) != expected_rank {
        return false;
    }
    let origin_sq = match side_to_move {
        Color::White => Square::from_index(sq as i8 + 8),
        Color::Black => Square::from_index(sq as i8 - 8),
    };
    ep_victim_pawn_present(squares, side_to_move, sq)
        && squares[origin_sq as usize] == NO_PIECE
        && squares[sq as usize] == NO_PIECE
}

/// Drop castling rights and an en-passant target that the Lichess/`parity`
/// predicate would discard instead of rejecting.
fn sanitize_lichess(parsed: &mut ParsedFen) {
    let white_commoner =
        parsed.squares[Square::E1 as usize] == make_piece(Color::White, PieceType::Commoner);
    let white_rook = make_piece(Color::White, PieceType::Rook);
    if parsed.castling_rights & WK_CASTLE != 0
        && !(white_commoner && parsed.squares[Square::H1 as usize] == white_rook)
    {
        parsed.castling_rights &= !WK_CASTLE;
    }
    if parsed.castling_rights & WQ_CASTLE != 0
        && !(white_commoner && parsed.squares[Square::A1 as usize] == white_rook)
    {
        parsed.castling_rights &= !WQ_CASTLE;
    }

    let black_commoner =
        parsed.squares[Square::E8 as usize] == make_piece(Color::Black, PieceType::Commoner);
    let black_rook = make_piece(Color::Black, PieceType::Rook);
    if parsed.castling_rights & BK_CASTLE != 0
        && !(black_commoner && parsed.squares[Square::H8 as usize] == black_rook)
    {
        parsed.castling_rights &= !BK_CASTLE;
    }
    if parsed.castling_rights & BQ_CASTLE != 0
        && !(black_commoner && parsed.squares[Square::A8 as usize] == black_rook)
    {
        parsed.castling_rights &= !BQ_CASTLE;
    }

    if let Some(sq) = parsed.ep_square
        && !ep_geometry_is_consistent(&parsed.squares, parsed.side_to_move, sq)
    {
        parsed.ep_square = None;
    }
}

/// Return the attackers of `square` under the atomic validation rule.
///
/// This mirrors shakmaty's atomic `king_attackers`: a side without a commoner
/// gives no check, and two touching commoners make the target immune to every
/// attack from that side.
fn atomic_validation_attackers(
    board: &Board,
    square: Square,
    attacker: Color,
    occupied: Bitboard,
) -> Bitboard {
    let attacker_commoners = board.commoners(attacker);
    if attacker_commoners.is_empty()
        || !(attacks::king_attacks(square) & attacker_commoners).is_empty()
    {
        Bitboard::EMPTY
    } else {
        board.attackers_to_color(square, attacker, occupied)
    }
}

/// Return the validation flags for the Lichess/`parity` predicate on an
/// already-sanitized board.
///
/// This mirrors the atomic `Setup` constructor with the recoverable
/// `INVALID_CASTLING_RIGHTS`, `INVALID_EP_SQUARE`, and `TOO_MUCH_MATERIAL`
/// classes discarded. Castling and en passant have already been sanitized by
/// [`sanitize_lichess`], so this function only reports the classes that the
/// constructor rejects.
fn lichess_validation_errors(board: &Board) -> ValidationErrors {
    let mut errors = ValidationErrors::empty();

    if board.occupied().is_empty() {
        errors |= ValidationErrors::EMPTY_BOARD;
    }

    const RANK_1: Bitboard = Bitboard(0x0000_0000_0000_00FF);
    const RANK_8: Bitboard = Bitboard(0xFF00_0000_0000_0000);
    if !(board.pieces_pt(PieceType::Pawn) & (RANK_1 | RANK_8)).is_empty() {
        errors |= ValidationErrors::PAWNS_ON_BACKRANK;
    }

    let us = board.side_to_move();
    let them = us.flip();
    let us_commoners = board.commoners(us);
    let them_commoners = board.commoners(them);

    // The atomic constructor removes MISSING_KING when the opponent still has
    // a commoner: the side to move may legitimately have just exploded. Only
    // the opponent's missing commoner is fatal, and both missing is fatal too.
    if them_commoners.is_empty() {
        errors |= match them {
            Color::White => ValidationErrors::MISSING_COMMONER_WHITE,
            Color::Black => ValidationErrors::MISSING_COMMONER_BLACK,
        };
        if us_commoners.is_empty() {
            errors |= match us {
                Color::White => ValidationErrors::MISSING_COMMONER_WHITE,
                Color::Black => ValidationErrors::MISSING_COMMONER_BLACK,
            };
        }
    }
    if us_commoners.count() > 1 {
        errors |= match us {
            Color::White => ValidationErrors::TOO_MANY_COMMONERS_WHITE,
            Color::Black => ValidationErrors::TOO_MANY_COMMONERS_BLACK,
        };
    }
    if them_commoners.count() > 1 {
        errors |= match them {
            Color::White => ValidationErrors::TOO_MANY_COMMONERS_WHITE,
            Color::Black => ValidationErrors::TOO_MANY_COMMONERS_BLACK,
        };
    }

    // The side that just moved must not be attacked by the side to move.
    if them_commoners.count() == 1 {
        let their_commoner = them_commoners.lsb();
        if !atomic_validation_attackers(board, their_commoner, us, board.occupied()).is_empty() {
            errors |= ValidationErrors::OPPOSITE_CHECK;
        }
    }

    // Sanitization makes this unreachable on the FEN path. It matters for
    // `from_setup_with(Lichess)`, where `from_setup` only checks the ep rank
    // and the victim pawn, not whether the target or the double-push origin is
    // empty.
    let stored_ep = board.ep_square();
    let valid_ep = stored_ep.filter(|&sq| ep_geometry_is_consistent(&board.squares, us, sq));
    if stored_ep.is_some() && valid_ep.is_none() {
        errors |= ValidationErrors::INVALID_EP_SQUARE;
    }

    // The atomic constructor only retains the impossible-check class when a
    // valid en-passant target makes the last double push explicit.
    if let Some(ep) = valid_ep
        && us_commoners.count() == 1
    {
        let our_commoner = us_commoners.lsb();
        let checkers = atomic_validation_attackers(board, our_commoner, them, board.occupied());
        if !checkers.is_empty() {
            let (pushed_to, pushed_from) = match us {
                Color::White => (
                    Square::from_index(ep as i8 - 8),
                    Square::from_index(ep as i8 + 8),
                ),
                Color::Black => (
                    Square::from_index(ep as i8 + 8),
                    Square::from_index(ep as i8 - 8),
                ),
            };

            let mut impossible = checkers.count() > 1;
            if !impossible {
                let checker = checkers.lsb();
                if checker != pushed_to {
                    // Rebuild occupancy as it was before the double push.
                    let occupied = (board.occupied() & !Bitboard::square_bb(pushed_to))
                        | Bitboard::square_bb(pushed_from);
                    if !atomic_validation_attackers(board, our_commoner, them, occupied).is_empty()
                    {
                        impossible = true;
                    }
                }
            }

            let steppers = board.pieces_pt(PieceType::Pawn)
                | board.pieces_pt(PieceType::Knight)
                | board.pieces_pt(PieceType::Commoner);
            if (checkers & steppers).count() > 1 {
                impossible = true;
            }

            if impossible {
                errors |= ValidationErrors::IMPOSSIBLE_CHECK;
            }
        }
    }

    errors
}

/// Cached state for a position, used during move generation and legality checks.
///
/// Fields are populated by [`Board::populate_state`] and consumed by
/// [`Board::legal`] and [`generate_legal`](crate::movegen::generate_legal).
/// A caller that only needs [`Board::legal`] or
/// [`Board::is_legal_with_state`] can fill the two commoner counts with
/// [`Board::populate_counts`] instead; move generation still requires the full
/// cached state.
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

/// The two commoner counts a legality check reads.
///
/// The first count belongs to the side to move; the second to its opponent.
/// A legal atomic position has zero (game over) or one commoner per side.
///
/// This value is a snapshot of the board's placement. A stale `Counts` changes
/// the legality answer in exactly the same way a stale [`StateInfo`] does: a
/// zero count rejects every candidate, and a nonzero count no longer matches
/// the board if a commoner has been captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    commoners: u32,
    them_commoners: u32,
}

impl Counts {
    /// Build a count pair for a side to move and its opponent.
    #[must_use]
    pub const fn new(commoners: u32, them_commoners: u32) -> Self {
        Self {
            commoners,
            them_commoners,
        }
    }

    /// The number of commoners of the side to move.
    #[must_use]
    pub const fn commoners(self) -> u32 {
        self.commoners
    }

    /// The number of commoners of the opponent.
    #[must_use]
    pub const fn them_commoners(self) -> u32 {
        self.them_commoners
    }
}

impl From<&StateInfo> for Counts {
    fn from(state: &StateInfo) -> Self {
        Self::new(state.commoners_count, state.them_commoners_count)
    }
}

/// Node-local cached data for a candidate legality loop.
///
/// Build once per position with [`Board::legality_context`], then ask
/// [`legal`](Self::legal) for every pseudo-legal candidate. The context is a
/// snapshot: it stays valid while the board does not change and holds no
/// reference to it. A stale context answers for the old placement, exactly as a
/// stale [`StateInfo`] does.
///
/// This is not a position representation and does not generate moves. It needs
/// only the placement and the two [`Counts`]; it reads no `checkers`/`pinned`
/// fields and populates no [`StateInfo`]. The occupancy computed for the node
/// is exposed by [`occupied`](Self::occupied) so a child/effect query can reuse
/// it.
#[derive(Debug, Clone, Copy)]
pub struct LegalityContext {
    /// The node-constant placement the rule body reads.
    node: LegalNode,
    /// Both sides still have a commoner; `false` is the game-over guard.
    playable: bool,
}

impl LegalityContext {
    /// The node's occupied squares.
    #[inline]
    #[must_use]
    pub fn occupied(&self) -> Bitboard {
        self.node.occupied()
    }

    /// Whether the pseudo-legal move `m` is legal in this node.
    ///
    /// Exactly [`Board::legal`] for the board and populated state this context
    /// was built from. A terminal board (either count zero) rejects every
    /// candidate, the same as [`Board::legal`].
    ///
    /// `m` must be pseudo-legal; malformed input is outside the contract, like
    /// [`Board::legal`].
    #[inline]
    #[must_use]
    pub fn legal(&self, m: Move) -> bool {
        self.playable && self.node.legal(m)
    }
}

/// A chessboard with atomic chess rules.
///
/// Maintains piece placement, side-to-move, castling rights, en-passant
/// square, and the half-move clock. Supports FEN serialization/deserialization,
/// making and unmaking moves, and legality checking.
#[derive(Debug, Clone)]
pub struct Board {
    /// Piece placement as a square array, for O(1) lookups by square.
    squares: [Piece; 64],
    /// Placement bitboards, side to move, ep target, castling rights, clock.
    ///
    /// Nested so that every rule query can borrow the view instead of copying
    /// it; the fields are kept in lock-step with `squares` by the crate's own
    /// mutators.
    position: PositionView,
    game_ply: u32,
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

/// Pieces removed by an atomic capture move.
///
/// Returned by [`Board::blast_casualties`]. Unlike the raw 3×3 geometry from
/// [`Board::blast_zone`], this value already applies the atomic removal rules:
/// pawns in the ring are immune, while the capturer at ground zero is always
/// destroyed. The normal capture victim or en-passant victim is included as
/// well, split by victim color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlastCasualties {
    /// Squares whose white pieces are removed by the move.
    pub white: Bitboard,
    /// Squares whose black pieces are removed by the move.
    pub black: Bitboard,
    /// The en-passant victim's square, or `None` for a normal capture.
    pub en_passant_victim: Option<Square>,
}

impl Default for BlastCasualties {
    fn default() -> Self {
        Self {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
            en_passant_victim: None,
        }
    }
}

impl BlastCasualties {
    /// Return `true` when the move removes no pieces.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.white.is_empty() && self.black.is_empty()
    }

    /// Return all removed squares, regardless of victim color.
    #[must_use]
    pub fn all(self) -> Bitboard {
        self.white | self.black
    }
}

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
    /// [`FenPolicy::Strict`], or call [`Board::validate`] after parsing. For
    /// the sanitizing Lichess/`parity` acceptance predicate use
    /// `from_fen_with(..., FenPolicy::Lichess)`; the default parser's answer is
    /// unchanged.
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
        let mut parsed = parse_fen_prefix(fen)?;

        // Validate castling rights against the actual commoner and rook
        // squares. The shared predicate is the one `from_setup` and
        // `validate()` use; checking one right at a time preserves the
        // existing per-side error text.
        for (bit, message) in [
            (
                WK_CASTLE,
                "white king-side castling rights without required pieces",
            ),
            (
                WQ_CASTLE,
                "white queen-side castling rights without required pieces",
            ),
            (
                BK_CASTLE,
                "black king-side castling rights without required pieces",
            ),
            (
                BQ_CASTLE,
                "black queen-side castling rights without required pieces",
            ),
        ] {
            if parsed.castling_rights & bit != 0
                && !castling_rights_backed_by_pieces(bit, &parsed.squares)
            {
                return Err(FenError::InvalidCastling(message.to_string()));
            }
        }

        if let Some(sq) = parsed.ep_square {
            let expected_rank = match parsed.side_to_move {
                Color::White => Rank::R6,
                Color::Black => Rank::R3,
            };
            if rank_of(sq) != expected_rank {
                return Err(FenError::InvalidEpSquare(format!(
                    "{} on rank {:?} is not a valid en-passant square for {:?}",
                    sq_str(sq).unwrap_or("??"),
                    rank_of(sq),
                    parsed.side_to_move
                )));
            }
            let them = parsed.side_to_move.flip();
            if !ep_victim_pawn_present(&parsed.squares, parsed.side_to_move, sq) {
                return Err(FenError::InvalidEpSquare(format!(
                    "no {} pawn on the square behind {} for en-passant",
                    if them == Color::White {
                        "white"
                    } else {
                        "black"
                    },
                    sq_str(sq).unwrap_or("??")
                )));
            }
        }

        let (rule50, game_ply) = parse_fen_clocks(fen, parsed.side_to_move)?;
        parsed.rule50 = rule50;
        parsed.game_ply = game_ply;

        Ok(Board::from_setup_parts(
            parsed.squares,
            parsed.side_to_move,
            parsed.castling_rights,
            parsed.ep_square,
            parsed.rule50,
            parsed.game_ply,
        ))
    }

    /// Parse a FEN string and apply the given semantic validation policy.
    ///
    /// This is the opt-in strict counterpart to [`Board::from_fen`]. The
    /// permissive parser is still used for syntax; after parsing, the position
    /// is checked according to `policy`. Use [`FenPolicy::Strict`] for
    /// untrusted input, [`FenPolicy::AllowTerminal`] for artificial terminal
    /// fixtures, [`FenPolicy::AllowImpossibleCheck`] to skip only the
    /// side-not-to-move check, [`FenPolicy::Permissive`] for behavior
    /// identical to [`Board::from_fen`], or [`FenPolicy::Lichess`] for the
    /// sanitizing Lichess/`parity` acceptance predicate.
    ///
    /// [`FenPolicy::Lichess`] is the only policy that drops data: it discards
    /// castling rights that are not backed by the required commoner and rook,
    /// and an en-passant target that is not consistent with a double push.
    /// Every other policy keeps the behavior it had before this variant was
    /// added.
    ///
    /// # Errors
    ///
    /// Returns [`FenLoadError::Syntax`] for malformed FEN text and
    /// [`FenLoadError::InvalidPosition`] when the parsed position fails the
    /// selected policy.
    pub fn from_fen_with(fen: &str, policy: FenPolicy) -> Result<Self, FenLoadError> {
        let board = if policy == FenPolicy::Lichess {
            let mut parsed = parse_fen_syntax(fen).map_err(FenLoadError::Syntax)?;
            sanitize_lichess(&mut parsed);
            Board::from_setup_parts(
                parsed.squares,
                parsed.side_to_move,
                parsed.castling_rights,
                parsed.ep_square,
                parsed.rule50,
                parsed.game_ply,
            )
        } else {
            Board::from_fen(fen).map_err(FenLoadError::Syntax)?
        };
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
    ///
    /// Crate-internal so that [`Board::from_setup_with`](crate::Board::from_setup_with)
    /// can reuse the FEN validation vocabulary for piece-set construction.
    pub(crate) fn validation_errors(&self, policy: FenPolicy) -> ValidationErrors {
        if policy == FenPolicy::Lichess {
            return lichess_validation_errors(self);
        }
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
            FenPolicy::Permissive | FenPolicy::Lichess => unreachable!(),
        }

        // 2. Pawns on the back ranks.
        const RANK_1: Bitboard = Bitboard(0x0000_0000_0000_00FF);
        const RANK_8: Bitboard = Bitboard(0xFF00_0000_0000_0000);
        if !(self.pieces_pt(PieceType::Pawn) & (RANK_1 | RANK_8)).is_empty() {
            errors |= ValidationErrors::PAWNS_ON_BACKRANK;
        }

        // 3. Castling rights must have the required commoner and rook.
        if !castling_rights_backed_by_pieces(self.position.castling_rights, &self.squares) {
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
        if let Some(sq) = self.position.ep_square
            && !ep_geometry_is_consistent(&self.squares, self.position.side_to_move, sq)
        {
            errors |= ValidationErrors::INVALID_EP_SQUARE;
        }

        // 6. The side that just moved must not be in check.
        if policy != FenPolicy::AllowImpossibleCheck {
            let just_moved = self.position.side_to_move.flip();
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
    ///
    /// For a null-move replay, [`null_for`](Self::null_for) is the
    /// allocation-free equivalent of serializing and re-parsing this FEN.
    #[must_use]
    pub fn fen(&self) -> String {
        self.write_fen(self.position.ep_square)
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
            EnPassantMode::Always => self.write_fen(self.position.ep_square),
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
        fen.push(match self.position.side_to_move {
            Color::White => 'w',
            Color::Black => 'b',
        });

        fen.push(' ');
        let mut has_castling = false;
        if self.position.castling_rights & WK_CASTLE != 0 {
            fen.push('K');
            has_castling = true;
        }
        if self.position.castling_rights & WQ_CASTLE != 0 {
            fen.push('Q');
            has_castling = true;
        }
        if self.position.castling_rights & BK_CASTLE != 0 {
            fen.push('k');
            has_castling = true;
        }
        if self.position.castling_rights & BQ_CASTLE != 0 {
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
        fen.push_str(&self.position.rule50.to_string());
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
        if self.position.side_to_move == Color::Black {
            h ^= crate::zobrist::ZOBRIST.side;
        }
        h ^= crate::zobrist::ZOBRIST.castling[self.position.castling_rights as usize];
        if let Some(sq) = self.position.ep_square {
            h ^= crate::zobrist::ZOBRIST.ep[crate::types::file_of(sq) as usize];
        }
        h
    }

    /// Recompute the Zobrist hash from the current board state.
    pub(crate) fn recompute_hash(&mut self) {
        self.hash = self.recomputed_hash();
    }

    /// Build a board from a raw square array and position state, deriving the
    /// color/type bitboards and the Zobrist hash.
    ///
    /// This is the crate-private constructor behind
    /// [`Board::from_setup`](crate::Board::from_setup); it performs no
    /// validation and allocates nothing.
    pub(crate) fn from_setup_parts(
        squares: [Piece; 64],
        side_to_move: Color,
        castling_rights: u8,
        ep_square: Option<Square>,
        rule50: u16,
        game_ply: u32,
    ) -> Board {
        let mut by_color = [Bitboard::EMPTY; 2];
        let mut by_type = [Bitboard::EMPTY; 6];
        for (idx, piece) in squares.iter().enumerate() {
            let Some(color) = piece.color() else { continue };
            let Some(pt) = piece.type_of() else { continue };
            let bb = Bitboard::square_bb(SQUARES[idx]);
            by_color[color as usize] = by_color[color as usize] | bb;
            by_type[pt as usize] = by_type[pt as usize] | bb;
        }

        let mut board = Board {
            squares,
            position: PositionView::from_parts(
                by_color,
                by_type,
                side_to_move,
                ep_square,
                castling_rights,
                rule50,
            ),
            game_ply,
            hash: 0,
        };
        board.recompute_hash();
        board
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

    /// Iterate over the occupied squares in ascending square order.
    ///
    /// Yields `(square, piece)` pairs and skips empty squares, so the sequence
    /// is exactly the piece placement of [`fen`](Self::fen) read from `a1`
    /// upwards. The iterator borrows the board and allocates nothing.
    ///
    /// ```
    /// use atomic_movegen::{Board, Color, PieceType, Square, make_piece, Piece};
    ///
    /// let board = Board::new();
    /// assert_eq!(board.pieces().count(), 32);
    /// assert_eq!(
    ///     board.pieces().next(),
    ///     Some((Square::A1, make_piece(Color::White, PieceType::Rook)))
    /// );
    /// assert!(board.pieces().any(|(sq, piece): (Square, Piece)| {
    ///     sq == Square::E1 && piece == make_piece(Color::White, PieceType::Commoner)
    /// }));
    /// ```
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_ {
        SQUARES.iter().copied().filter_map(move |sq| {
            Some((sq, self.squares[sq as usize])).filter(|&(_, p)| p != NO_PIECE)
        })
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
        self.position.by_color[0] | self.position.by_color[1]
    }

    /// Return all pieces of a given color.
    #[inline(always)]
    pub fn pieces_color(&self, c: Color) -> Bitboard {
        self.position.by_color[c as usize]
    }

    /// Return all pieces of a given type.
    #[inline(always)]
    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard {
        self.position.by_type[pt as usize]
    }

    /// Return all pieces of a given color and type.
    #[inline(always)]
    pub fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard {
        self.position.by_color[c as usize] & self.position.by_type[pt as usize]
    }

    /// Return the side to move.
    #[inline]
    #[must_use]
    pub fn side_to_move(&self) -> Color {
        self.position.side_to_move
    }

    /// Borrow the rule view of this position.
    ///
    /// This is how the board, the move generator, and the effect path share one
    /// rules implementation without copying the placement.
    #[inline(always)]
    pub(crate) fn position_view(&self) -> &PositionView {
        &self.position
    }

    /// Return the castling rights bitmask.
    #[must_use]
    pub fn castling_rights(&self) -> u8 {
        self.position.castling_rights
    }

    /// Return `true` when `color` still has the castling right for `side`.
    #[must_use]
    pub fn has_castling_right(&self, color: Color, side: CastlingSide) -> bool {
        let bit = match (color, side) {
            (Color::White, CastlingSide::KingSide) => WK_CASTLE,
            (Color::White, CastlingSide::QueenSide) => WQ_CASTLE,
            (Color::Black, CastlingSide::KingSide) => BK_CASTLE,
            (Color::Black, CastlingSide::QueenSide) => BQ_CASTLE,
        };
        self.position.castling_rights & bit != 0
    }

    /// Return the stored en-passant target square, if any.
    ///
    /// This is the Fairy-Stockfish-style target recorded after every double
    /// pawn push. It does not imply that an en-passant capture is legal; use
    /// [`legal_ep_square`](Self::legal_ep_square) for a target that at least
    /// one legal en-passant capture actually attacks.
    #[must_use]
    pub fn ep_square(&self) -> Option<Square> {
        self.position.ep_square
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
        let ep = self.position.ep_square?;
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        if state.commoners_count == 0 || state.them_commoners_count == 0 {
            return None;
        }

        let us = self.position.side_to_move;
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
        self.position.rule50
    }

    /// Return the raw per-ply move counter (1-based), saturating at
    /// [`u16::MAX`].
    ///
    /// The internal counter is wider so that a FEN with a large (but
    /// `u16`-representable) fullmove number cannot overflow on
    /// [`do_move`](Self::do_move) or [`undo_move`](Self::undo_move).
    #[inline]
    #[must_use]
    pub fn game_ply(&self) -> u16 {
        u16::try_from(self.game_ply).unwrap_or(u16::MAX)
    }

    /// Return the fullmove number as it appears in FEN output.
    ///
    /// The value saturates at [`u16::MAX`], which only happens for an internal
    /// game ply beyond `2 * u16::MAX`; a parsed FEN cannot reach it because its
    /// fullmove field is itself a `u16`.
    #[inline]
    #[must_use]
    pub fn fullmove_number(&self) -> u16 {
        let fullmove = 1 + self.game_ply.saturating_sub(1) / 2;
        u16::try_from(fullmove).unwrap_or(u16::MAX)
    }

    /// The null counterpart of this position: same pieces and castling rights,
    /// `attacker` to move, no en-passant target.
    ///
    /// This is the cheap in-place equivalent of the FEN round-trip used by
    /// null-move searches. The half-move clock and fullmove number are
    /// preserved, so the derived game ply follows the new side exactly as the
    /// FEN path does. The Zobrist hash is recomputed for the new side and
    /// cleared ep target.
    ///
    /// The result is an ordinary [`Board`]. A caller that needs an ephemeral
    /// null position must still keep it out of any keyed or stored state.
    ///
    /// For the search-clock variant that advances the half-move clock, see
    /// [`null_for_advanced`](Self::null_for_advanced).
    #[must_use]
    pub fn null_for(&self, attacker: Color) -> Board {
        let mut board = self.clone();
        board.position.side_to_move = attacker;
        board.position.ep_square = None;
        // Match the FEN round-trip: the fullmove number is preserved and the
        // game ply is derived from the new side to move. The internal counter
        // is wide enough for every parsed fullmove number, so the same
        // saturating arithmetic as the FEN path is exact here.
        let fullmove = self.fullmove_number();
        board.game_ply = u32::from(fullmove.saturating_sub(1))
            .saturating_mul(2)
            .saturating_add(if attacker == Color::White { 1 } else { 2 });
        board.recompute_hash();
        board
    }

    /// The null counterpart of this position with an advanced half-move clock.
    ///
    /// Same pieces and castling rights, `attacker` to move, no en-passant
    /// target, `rule50 + 1`, and `game_ply + 1`. The Zobrist key is updated for
    /// the new side and cleared ep target; it is exactly the key of a board
    /// with the same placement and state fields.
    ///
    /// Unlike [`null_for`](Self::null_for), this does **not** derive the game
    /// ply from the preserved fullmove number. It advances the clock by one
    /// ply unconditionally, as a 50-move-rule null probe requires. For the
    /// usual `attacker = self.side_to_move().flip()`, the two methods agree on
    /// the side and ep target but differ by the advanced clock. When the side
    /// does not flip, the advanced result is not necessarily FEN-equivalent to
    /// a same-fullmove round-trip, because its game ply no longer has the
    /// parity implied by the stored side to move.
    ///
    /// Both clocks saturate: `rule50` at [`u16::MAX`] and the internal game
    /// ply at its counter limit. The result is an ordinary [`Board`]. A caller
    /// that uses it as an ephemeral null position must keep it out of keyed or
    /// stored state.
    #[must_use]
    pub fn null_for_advanced(&self, attacker: Color) -> Board {
        let mut board = self.clone();
        let old_side = self.position.side_to_move;
        let old_ep = self.position.ep_square;

        board.position.side_to_move = attacker;
        board.position.ep_square = None;
        board.position.rule50 = self.position.rule50.saturating_add(1);
        board.game_ply = self.game_ply.saturating_add(1);

        // The key covers pieces, side, castling rights, and the stored ep
        // file, but not either clock. Only the side and ep transitions need a
        // delta; castling rights and every piece key are unchanged.
        if attacker != old_side {
            board.hash ^= crate::zobrist::ZOBRIST.side;
        }
        if let Some(sq) = old_ep {
            board.hash ^= crate::zobrist::ZOBRIST.ep[file_of(sq) as usize];
        }
        board
    }

    /// Return the current Zobrist hash of the position.
    ///
    /// The hash includes pieces, side to move, castling rights, and the stored
    /// en-passant file, but not the half-move clock or fullmove counter. This
    /// is the [`EnPassantMode::Always`] policy, identical to
    /// [`hash_with(EnPassantMode::Always)`](Self::hash_with).
    ///
    /// # Key contract
    ///
    /// The key table and its XOR composition are compatible with
    /// Fairy-Stockfish under [`EnPassantMode::Always`]. They are stable across
    /// the 2.x line: any change to the table or to how pieces, side, castling,
    /// or the ep file are combined is a major-version change and bumps
    /// [`ZOBRIST_VERSION`](crate::ZOBRIST_VERSION). Persisted transposition
    /// tables should store that version with their data and invalidate on a
    /// mismatch.
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
    ///
    /// [`EnPassantMode::Legal`] is a canonicalization layered on the stable
    /// base keys (it XORs the same ep-file key in and out); it is not a second
    /// key table. See [`hash`](Self::hash) for the stability contract and
    /// [`ZOBRIST_VERSION`](crate::ZOBRIST_VERSION).
    #[inline]
    #[must_use]
    pub fn hash_with(&self, mode: EnPassantMode) -> u64 {
        match mode {
            EnPassantMode::Always => self.hash,
            EnPassantMode::Legal => match self.position.ep_square {
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
        if m.move_type() == MoveType::Castling {
            return self.hash_after_castling(m);
        }
        self.hash_after_classified(m, self.classify_move(m))
    }

    /// Zobrist hash and half-move clock of the position after `m`, computed
    /// together, without modifying `self`.
    ///
    /// Exactly equal to `(self.hash_after(m), self.rule50_after(m))`, and to
    /// the `(hash(), rule50())` pair of a cloned board after
    /// [`do_move`](Self::do_move). It exists so that a consumer building child
    /// keys can classify `m` once instead of twice; it does **not** replace
    /// [`hash_after`](Self::hash_after) or
    /// [`rule50_after`](Self::rule50_after). The hash uses the default
    /// [`EnPassantMode::Always`] policy, like `hash_after`.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `from` holds no piece (i.e. `m` is not a
    /// move `do_move` would accept).
    #[must_use]
    pub fn hash_and_rule50_after(&self, m: Move) -> (u64, u16) {
        if m.move_type() == MoveType::Castling {
            return (
                self.hash_after_castling(m),
                self.position.rule50.saturating_add(1),
            );
        }
        let class = self.classify_move(m);
        (
            self.hash_after_classified(m, class),
            self.rule50_after_classified(class),
        )
    }

    /// Castling branch of [`hash_after`](Self::hash_after); castling never
    /// captures and never changes the en-passant file.
    fn hash_after_castling(&self, m: Move) -> u64 {
        let us = self.position.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();

        let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
        let commoner = make_piece(us, PieceType::Commoner);
        let rook = make_piece(us, PieceType::Rook);
        let mut h = self.hash
            ^ crate::zobrist::ZOBRIST.side
            ^ piece_key(kfrom, commoner)
            ^ piece_key(kto, commoner)
            ^ piece_key(rfrom, rook)
            ^ piece_key(rto, rook);
        let new_rights = self.position.castling_rights
            & match us {
                Color::White => !(WK_CASTLE | WQ_CASTLE),
                Color::Black => !(BK_CASTLE | BQ_CASTLE),
            };
        h ^= crate::zobrist::ZOBRIST.castling[self.position.castling_rights as usize];
        h ^= crate::zobrist::ZOBRIST.castling[new_rights as usize];
        if let Some(sq) = self.position.ep_square {
            h ^= crate::zobrist::ZOBRIST.ep[file_of(sq) as usize];
        }
        h
    }

    /// Classify the non-castling move `m` once for the child-key helpers.
    ///
    /// Panics in debug builds when the origin square is empty.
    fn classify_move(&self, m: Move) -> MoveClass {
        let piece = self.squares[m.from_sq() as usize];
        debug_assert!(
            piece != NO_PIECE,
            "hash_after/rule50_after on a move from an empty square"
        );
        MoveClass {
            piece,
            // Pawn iff the low three bits of the raw encoding are `Pawn + 1`
            // (piece types are encoded as `type + 1`, pawn is variant 0).
            // Avoids the `type_of()` option/table round-trip on the hot
            // quiet path.
            is_pawn: piece.raw() & 7 == PieceType::Pawn as u8 + 1,
            is_capture: !self.empty(m.to_sq()),
        }
    }

    /// Hash branch of [`hash_after`](Self::hash_after) for an already
    /// classified non-castling move.
    fn hash_after_classified(&self, m: Move, class: MoveClass) -> u64 {
        let us = self.position.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();
        let piece = class.piece;
        let is_pawn = class.is_pawn;
        let is_capture = class.is_capture;
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
            pawns = self.position.by_type[PieceType::Pawn as usize];
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
            castling_rights_after(self.position.castling_rights, from, to, true, &post_blast)
        } else if is_capture_or_ep {
            // No castling-relevant square touched: rights unchanged.
            self.position.castling_rights
        } else if (from_bb | to_bb) & BB_FIRST_LAST_RANK == Bitboard::EMPTY {
            // A quiet move touching neither back rank cannot change rights.
            self.position.castling_rights
        } else {
            castling_rights_after(
                self.position.castling_rights,
                from,
                to,
                false,
                &[NO_PIECE; 6],
            )
        };
        if new_rights != self.position.castling_rights {
            h ^= crate::zobrist::ZOBRIST.castling[self.position.castling_rights as usize];
            h ^= crate::zobrist::ZOBRIST.castling[new_rights as usize];
        }

        // En-passant square transition: old file out, new file in.
        if let Some(sq) = self.position.ep_square {
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
            return self.position.rule50.saturating_add(1);
        }
        self.rule50_after_classified(self.classify_move(m))
    }

    /// Rule50 branch of [`rule50_after`](Self::rule50_after) for an already
    /// classified non-castling move.
    fn rule50_after_classified(&self, class: MoveClass) -> u16 {
        if class.is_pawn || class.is_capture {
            0
        } else {
            self.position.rule50.saturating_add(1)
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
        self.terminal_impl(state, moves_exist).map(|t| t.outcome)
    }

    /// Return the WDL outcome and its reason, if the position is decided.
    ///
    /// Uses the same precedence as [`outcome`](Self::outcome):
    ///
    /// 1. side-to-move commoner extinct → `Loss`/`Extinction`;
    /// 2. opponent commoner extinct → `Win`/`Extinction`;
    /// 3. only the two commoners remain → `Draw`/`MaterialDraw`;
    /// 4. no legal moves: attacked → `Loss`/`NoMovesInCheck`, safe →
    ///    `Draw`/`Stalemate`;
    /// 5. `rule50 >= 100` → `Draw`/`FiftyMoveRule`.
    ///
    /// Mate outranks the clock: the no-move branch is resolved before the
    /// 50-move branch when the side to move is in check.
    ///
    /// This mirrors [`outcome`](Self::outcome), including the commoner-only
    /// material draw, and is therefore **not** the same as
    /// [`is_terminal`](Self::is_terminal): a K-vs-K position with
    /// `rule50 < 100` returns `Some(Terminal { outcome: Draw, reason:
    /// MaterialDraw })` while `is_terminal()` is `false`.
    ///
    /// This method populates one temporary [`StateInfo`] and uses the
    /// existence query, but never builds a [`MoveList`].
    #[must_use]
    pub fn terminal(&self) -> Option<Terminal> {
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        self.terminal_with_state(&state)
    }

    /// Same as [`terminal`](Self::terminal), reusing a populated [`StateInfo`].
    ///
    /// `state` must have been populated by
    /// [`populate_state`](Self::populate_state) for this board. A stale
    /// `StateInfo` can change extinction, check, and stalemate results.
    #[must_use]
    pub fn terminal_with_state(&self, state: &StateInfo) -> Option<Terminal> {
        self.terminal_impl(state, None)
    }

    /// Hot-path form of [`terminal_with_state`](Self::terminal_with_state) when
    /// the caller already knows whether a legal move exists.
    ///
    /// `moves_exist` must be the result of
    /// [`has_legal_move_with_state(self, state)`](crate::movegen::has_legal_move_with_state).
    /// It is authoritative: passing `false` for a non-terminal position reports
    /// `NoMovesInCheck` or `Stalemate` according to [`StateInfo::checkers`],
    /// even though no move generation was run. The value is not consulted when
    /// extinction, the material shortcut, or the non-check 50-move branch
    /// already decides the position.
    #[must_use]
    pub fn terminal_with(&self, state: &StateInfo, moves_exist: bool) -> Option<Terminal> {
        self.terminal_impl(state, Some(moves_exist))
    }

    /// One precedence implementation shared by every `terminal*` query and,
    /// through [`outcome_impl`](Self::outcome_impl), by every `outcome*` query.
    fn terminal_impl(&self, state: &StateInfo, moves_exist: Option<bool>) -> Option<Terminal> {
        crate::position::terminal_in_view(self.position_view(), state, moves_exist)
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
        self.position_view().is_material_draw()
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
        self.commoners(self.position.side_to_move).is_empty()
            || self.commoners(self.position.side_to_move.flip()).is_empty()
            || self.position.rule50 >= 100
            || !crate::movegen::has_legal_move_with_state(self, &state)
    }

    /// Build an [`EffectContext`](crate::effects::EffectContext) for this
    /// position, populating one temporary [`StateInfo`].
    ///
    /// The context snapshots the node-constant data the effect queries need, so
    /// it is built once per position and then reused for every pseudo-legal
    /// candidate, exactly like [`populate_state`](Self::populate_state) for
    /// [`legal`](Self::legal). It does not own the board.
    #[must_use]
    pub fn effect_context(&self) -> crate::effects::EffectContext {
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        crate::effects::EffectContext::new(self, &state)
    }

    /// Build an [`EffectContext`](crate::effects::EffectContext) from a
    /// caller-populated [`StateInfo`].
    ///
    /// `state` must have been populated by
    /// [`populate_state`](Self::populate_state) for this board; a stale state
    /// changes which positions are reported as already over.
    #[must_use]
    pub fn effect_context_with_state(&self, state: &StateInfo) -> crate::effects::EffectContext {
        crate::effects::EffectContext::new(self, state)
    }

    /// Return the outcome of the child position after `m` **without building
    /// the child**, from the child side-to-move perspective.
    ///
    /// # Contract
    ///
    /// For every legal `m` this returns exactly
    /// `{ let mut b = self.clone(); b.do_move(m, &mut st); b.outcome() }`, and
    /// for every illegal `m` it returns `None`.
    ///
    /// `None` is therefore ambiguous: it means *illegal* **or** *undecided*
    /// (the child is not decided, so the game continues). Use
    /// [`child_outcome`](Self::child_outcome) for the two-level answer, or
    /// [`EffectContext::is_legal`](crate::effects::EffectContext::is_legal) and
    /// [`terminal_after`](Self::terminal_after) to ask the two questions
    /// separately.
    ///
    /// `state` must be populated for this board. This is a convenience wrapper
    /// for [`EffectContext::outcome_after`](crate::effects::EffectContext::outcome_after)
    /// for callers that do not keep a context.
    #[must_use]
    pub fn outcome_after(&self, m: Move, state: &StateInfo) -> Option<Outcome> {
        self.effect_context_with_state(state).outcome_after(self, m)
    }

    /// Return the two-level child answer after `m` **without building the
    /// child**: [`AfterState::Illegal`](crate::effects::AfterState::Illegal),
    /// [`AfterState::Undecided`](crate::effects::AfterState::Undecided), or
    /// [`AfterState::Terminal`](crate::effects::AfterState::Terminal) with the
    /// child's [`Terminal`].
    ///
    /// This is the unambiguous counterpart of
    /// [`outcome_after`](Self::outcome_after), which returns `None` both for an
    /// illegal move and for an undecided child. For every **pseudo-legal** `m`:
    /// `Illegal` iff `!self.legal(m, state)`, and otherwise `Terminal(t)` iff
    /// `{ let mut b = self.clone(); b.do_move(m, &mut st); b.terminal() }` is
    /// `Some(t)`, `Undecided` iff it is `None`.
    ///
    /// `state` must be populated for this board. This is a convenience wrapper
    /// for [`EffectContext::child_outcome`](crate::effects::EffectContext::child_outcome)
    /// for callers that do not keep a context.
    #[must_use]
    pub fn child_outcome(&self, m: Move, state: &StateInfo) -> crate::effects::AfterState {
        self.effect_context_with_state(state).child_outcome(self, m)
    }

    /// Return the terminal state of the child position after `m` **without
    /// building the child**, from the child side-to-move perspective.
    ///
    /// `None` means the child is not decided. For every **pseudo-legal** `m`
    /// this returns exactly
    /// `{ let mut b = self.clone(); b.do_move(m, &mut st); b.terminal() }`, with
    /// the same precedence as [`terminal`](Self::terminal) (extinction →
    /// material draw → no legal moves → clock). Legality is **not** checked: a
    /// pseudo-legal move that explodes the mover's commoner has an extinct
    /// child and is reported as such.
    ///
    /// `state` must be populated for this board. This is a convenience wrapper
    /// for
    /// [`EffectContext::terminal_after`](crate::effects::EffectContext::terminal_after)
    /// for callers that do not keep a context.
    #[must_use]
    pub fn terminal_after(&self, m: Move, state: &StateInfo) -> Option<Terminal> {
        self.effect_context_with_state(state)
            .terminal_after(self, m)
    }

    /// Return `true` if `m` is a capture on this board.
    #[inline]
    #[must_use]
    pub fn is_capture(&self, m: Move) -> bool {
        self.position_view().is_capture(m)
    }

    /// Return the full 3×3 atomic blast area centered on `sq`: the square
    /// itself plus its eight neighbours.
    ///
    /// This is pure geometry and deliberately includes the center and any
    /// pawn squares. Atomic rules make pawns immune to the blast except at
    /// ground zero, so [`blast_casualties`](Self::blast_casualties) and
    /// [`do_move`](Self::do_move) mask out pawns before removing pieces.
    /// [`Square::NONE`] is total here and returns [`Bitboard::EMPTY`].
    #[inline]
    #[must_use]
    pub fn blast_zone(sq: Square) -> Bitboard {
        if sq == Square::NONE {
            return Bitboard::EMPTY;
        }
        attacks::king_attacks(sq) | Bitboard::square_bb(sq)
    }

    /// Return every piece `m` would remove, without modifying the board.
    ///
    /// `m` is expected to be a pseudo-legal capture (normal capture or en
    /// passant). Castling and non-capturing moves return
    /// [`BlastCasualties::default`], since quiet moves have no blast.
    ///
    /// The result includes:
    /// - the normal capture victim, or the en-passant victim (also identified
    ///   by [`BlastCasualties::en_passant_victim`]);
    /// - every non-pawn in the blast ring around the destination;
    /// - the capturer at ground zero (even when it is a pawn or a promoted
    ///   piece).
    ///
    /// This mirrors [`do_move`](Self::do_move) without mutation: it neither
    /// calls `do_move` nor allocates.
    #[must_use]
    pub fn blast_casualties(&self, m: Move) -> BlastCasualties {
        self.position_view().blast_casualties(m)
    }

    /// Return `true` when `m` resets the half-move clock.
    ///
    /// A move is zeroing when it is a pawn move, a promotion, an en-passant
    /// capture, or a capture of any kind. Castling increments the clock, so it
    /// is not zeroing. Returns `false` for [`Move::NONE`] and moves whose
    /// origin is empty.
    ///
    /// This is the safe, board-aware counterpart to
    /// [`rule50_after`](Self::rule50_after): it answers the same question and
    /// never panics on malformed input.
    #[must_use]
    pub fn is_zeroing(&self, m: Move) -> bool {
        let from = m.from_sq();
        if m == Move::NONE || from == Square::NONE || self.piece_on(from) == NO_PIECE {
            return false;
        }
        if m.is_en_passant() || m.is_promotion() {
            return true;
        }
        if m.is_castling() {
            return false;
        }
        self.piece_on(from)
            .type_of()
            .is_some_and(|pt| pt == PieceType::Pawn)
            || !self.empty(m.to_sq())
    }

    /// Return `true` when `m` is a normal two-square pawn push from the start
    /// rank.
    ///
    /// Checks the move type, the moving piece (a pawn of the side to move), the
    /// file, the two-rank distance, and that the intermediate square is empty.
    /// A rook or other slider moving two ranks, a one-square pawn move, and a
    /// promotion all return `false`.
    #[must_use]
    pub fn is_double_push(&self, m: Move) -> bool {
        if m.move_type() != MoveType::Normal {
            return false;
        }
        let from = m.from_sq();
        let to = m.to_sq();
        let piece = self.piece_on(from);
        if piece.color() != Some(self.position.side_to_move)
            || piece.type_of() != Some(PieceType::Pawn)
            || file_of(from) != file_of(to)
            || (to as i8 - from as i8).abs() != 16
        {
            return false;
        }
        let middle = Square::from_index((from as i8 + to as i8) / 2);
        self.empty(middle)
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
        attackers = attackers | (knight_atk & (self.position.by_type[PieceType::Knight as usize]));

        // Bishop/Queen attacks
        let bishop_atk = attacks::bishop_attacks(sq, occupied);
        attackers = attackers
            | (bishop_atk
                & (self.position.by_type[PieceType::Bishop as usize]
                    | self.position.by_type[PieceType::Queen as usize]));

        // Rook/Queen attacks
        let rook_atk = attacks::rook_attacks(sq, occupied);
        attackers = attackers
            | (rook_atk
                & (self.position.by_type[PieceType::Rook as usize]
                    | self.position.by_type[PieceType::Queen as usize]));

        // Commoner (king) attacks
        let king_atk = attacks::king_attacks(sq);
        attackers = attackers | (king_atk & self.position.by_type[PieceType::Commoner as usize]);

        attackers
    }

    /// Return the attackers of `sq` restricted to pieces of one color.
    ///
    /// Equivalent to `self.attackers_to(sq, occupied) & self.pieces_color(color)`.
    #[must_use]
    pub fn attackers_to_color(&self, sq: Square, color: Color, occupied: Bitboard) -> Bitboard {
        self.attackers_to(sq, occupied) & self.pieces_color(color)
    }

    pub(crate) fn compute_checkers(&self, us: Color) -> Bitboard {
        self.position_view().checkers(us)
    }

    /// Return the bitboard of enemy pieces checking the side to move.
    #[must_use]
    pub fn checkers(&self) -> Bitboard {
        self.compute_checkers(self.position.side_to_move)
    }

    pub(crate) fn compute_pinned(&self, us: Color) -> Bitboard {
        self.position_view().pinned(us)
    }

    /// Return the bitboard of pieces of the given color that are pinned
    /// (cannot move without exposing a commoner to capture).
    #[must_use]
    pub fn pinned(&self, c: Color) -> Bitboard {
        self.compute_pinned(c)
    }

    /// Fill cached state fields (checkers, pinned, commoner counts) for the
    /// current position so that `legal()` can read them instead of recomputing.
    ///
    /// The computation lives on the crate-private position view, so the same
    /// code populates the state of a child position that is derived from
    /// bitboards. This is still required for the state-aware move-generation
    /// entry points; a caller that only checks [`legal`](Self::legal) or
    /// [`is_legal_with_state`](Self::is_legal_with_state) can use
    /// [`populate_counts`](Self::populate_counts) instead.
    pub fn populate_state(&self, state: &mut StateInfo) {
        self.position_view().populate_state(state);
    }

    /// The two commoner counts of this position.
    ///
    /// This is the whole cached state [`legal`](Self::legal) reads: a call
    /// with a [`StateInfo`] whose two count fields equal this value answers
    /// exactly like one populated by [`populate_state`](Self::populate_state).
    #[must_use]
    pub fn counts(&self) -> Counts {
        self.position_view().counts()
    }

    /// Build a node-local [`LegalityContext`] for a candidate loop.
    ///
    /// `counts` must be current for this board; use [`counts`](Self::counts) or
    /// [`Counts::from`] over a populated [`StateInfo`]. The context reads no
    /// `checkers`/`pinned` fields and populates no state. Its
    /// [`legal`](LegalityContext::legal) agrees with [`legal`](Self::legal) for
    /// every pseudo-legal move, and its `occupied()` is this board's
    /// occupancy.
    #[must_use]
    pub fn legality_context(&self, counts: Counts) -> LegalityContext {
        LegalityContext {
            node: LegalNode::from_view(self.position_view()),
            playable: counts.commoners() != 0 && counts.them_commoners() != 0,
        }
    }

    /// Fill only `commoners_count` and `them_commoners_count` in `state`.
    ///
    /// `checkers`, `pinned`, and the undo fields are left untouched. This is
    /// enough for [`legal`](Self::legal) and
    /// [`is_legal_with_state`](Self::is_legal_with_state), but **not** for
    /// [`generate_legal_with_state`](crate::movegen::generate_legal_with_state),
    /// [`has_legal_move_with_state`](crate::movegen::has_legal_move_with_state),
    /// or any other state-aware move-generation query; those still require
    /// [`populate_state`](Self::populate_state).
    pub fn populate_counts(&self, state: &mut StateInfo) {
        let counts = self.position_view().counts();
        state.commoners_count = counts.commoners();
        state.them_commoners_count = counts.them_commoners();
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
    ///
    /// # Panics
    ///
    /// Panics if `m` is malformed for this board: a [`Move::NONE`] sentinel,
    /// an empty origin square, a move whose origin holds no piece of the side
    /// to move, or a move type that does not match the placement (for example
    /// a castling encoding without the commoner). These are caller
    /// preconditions; untrusted move input must be checked first with
    /// [`is_legal`](Self::is_legal), [`is_pseudo_legal`](Self::is_pseudo_legal)
    /// or [`move_from_uci`](Self::move_from_uci).
    pub fn do_move(&mut self, m: Move, state: &mut StateInfo) {
        state.hash = self.hash;
        state.castling_rights = self.position.castling_rights;
        state.ep_square = self.position.ep_square;
        state.rule50 = self.position.rule50;
        state.captured_count = 0;
        state.cap_sq = None;
        state.cap_piece = NO_PIECE;

        let us = self.position.side_to_move;
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
            self.position.castling_rights &= match us {
                Color::White => !(WK_CASTLE | WQ_CASTLE),
                Color::Black => !(BK_CASTLE | BQ_CASTLE),
            };
            self.position.ep_square = None;
            self.position.rule50 = self.position.rule50.saturating_add(1);
            self.position.side_to_move = them;
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
            let blast_zone =
                attacks::king_attacks(to) & !self.position.by_type[PieceType::Pawn as usize];
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
        self.position.castling_rights = castling_rights_after(
            self.position.castling_rights,
            from,
            to,
            is_capture_or_ep,
            &post_blast,
        );

        if pt == PieceType::Pawn && (to as i8 - from as i8).abs() == 16 {
            self.position.ep_square = Some(match us {
                Color::White => Square::from_index(from as i8 + 8),
                Color::Black => Square::from_index(from as i8 - 8),
            });
        } else {
            self.position.ep_square = None;
        }

        if pt == PieceType::Pawn || is_capture {
            self.position.rule50 = 0;
        } else {
            self.position.rule50 = self.position.rule50.saturating_add(1);
        }

        self.position.side_to_move = them;
        self.game_ply += 1;
        self.update_hash_state_transition(state);
    }

    /// Unmake `m`, restoring the board to its state before [`do_move`](Self::do_move).
    ///
    /// `state` must be the same [`StateInfo`] that was passed to `do_move`. It
    /// is used only for undo data; cached attack fields are not restored.
    ///
    /// # Panics
    ///
    /// Panics if `m` is not the move that produced `state` on this board, or
    /// if `state` is stale or was populated by a different board. The board is
    /// left inconsistent in that case; this is a caller precondition, not a
    /// recoverable error.
    pub fn undo_move(&mut self, m: Move, state: &StateInfo) {
        self.position.castling_rights = state.castling_rights;
        self.position.ep_square = state.ep_square;
        self.position.rule50 = state.rule50;

        self.position.side_to_move = self.position.side_to_move.flip();
        let us = self.position.side_to_move;
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

        self.position.by_color[c as usize] = (self.position.by_color[c as usize] ^ from_bb) | to_bb;
        self.position.by_type[pt as usize] = (self.position.by_type[pt as usize] ^ from_bb) | to_bb;
    }

    #[inline]
    fn remove_piece(&mut self, sq: Square, c: Color, pt: PieceType) {
        debug_assert!(self.squares[sq as usize] != NO_PIECE);
        let piece = self.squares[sq as usize];
        let raw = piece.raw() as usize;
        self.hash ^= crate::zobrist::ZOBRIST.piece[sq as usize][raw];

        self.squares[sq as usize] = NO_PIECE;
        let sq_bb = Bitboard::square_bb(sq);
        self.position.by_color[c as usize] = self.position.by_color[c as usize] ^ sq_bb;
        self.position.by_type[pt as usize] = self.position.by_type[pt as usize] ^ sq_bb;
    }

    #[inline]
    fn place_piece(&mut self, piece: Piece, sq: Square, c: Color, pt: PieceType) {
        debug_assert!(self.squares[sq as usize] == NO_PIECE);
        let raw = piece.raw() as usize;
        self.hash ^= crate::zobrist::ZOBRIST.piece[sq as usize][raw];

        self.squares[sq as usize] = piece;
        let sq_bb = Bitboard::square_bb(sq);
        self.position.by_color[c as usize] = self.position.by_color[c as usize] | sq_bb;
        self.position.by_type[pt as usize] = self.position.by_type[pt as usize] | sq_bb;
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
        self.hash ^= crate::zobrist::ZOBRIST.castling[self.position.castling_rights as usize];
        if let Some(sq) = state.ep_square {
            self.hash ^= crate::zobrist::ZOBRIST.ep[crate::types::file_of(sq) as usize];
        }
        if let Some(sq) = self.position.ep_square {
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
    /// Only the two commoner counts are read: `state.commoners_count` and
    /// `state.them_commoners_count` (written by
    /// [`populate_counts`](Self::populate_counts)). `checkers` and `pinned`
    /// are not consulted. A terminal board (either side has no commoner) has
    /// no legal moves.
    ///
    /// The rule body lives on the crate-private position view, shared with the
    /// move generator, the fast-legality ladder, the effect path, and the
    /// effect context's child queries (which feed it a bitboard view of the
    /// child instead of a board).
    ///
    /// # Preconditions
    ///
    /// This is the low-level predicate used by the move generator. It assumes
    /// that `m` is pseudo-legal and that the two commoner counts in `state`
    /// are current for this board; [`populate_state`](Self::populate_state) and
    /// [`populate_counts`](Self::populate_counts) both satisfy that. For
    /// untrusted move input use [`is_legal`](Self::is_legal) or
    /// [`is_legal_with_state`](Self::is_legal_with_state), which validate the
    /// move shape first.
    #[must_use]
    pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
        crate::position::legal_in_view(self.position_view(), m, state)
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
    /// Only `state.commoners_count` and `state.them_commoners_count` are read,
    /// so [`populate_counts`](Self::populate_counts) is sufficient; `checkers`
    /// and `pinned` are not consulted. The caller is responsible for ensuring
    /// those two counts are up to date; stale counts can change the answer,
    /// exactly as they do for [`legal`](Self::legal).
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

        let us = self.position.side_to_move;
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
        let us = self.position.side_to_move;
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
                if to_rank != expected_rank || self.position.ep_square != Some(to) {
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
        let us = self.position.side_to_move;
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

        self.position.castling_rights & right != 0
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
        if m.move_type() != MoveType::Normal || self.position.ep_square != Some(m.to_sq()) {
            return None;
        }
        Some(Move::make_enpassant(m.from_sq(), m.to_sq()))
    }

    /// Return the SAN (standard algebraic notation) of `m`.
    ///
    /// Returns `None` unless `m` is one of this position's legal moves, so the
    /// text is always unambiguous and always parses back to `m` with
    /// [`move_from_san`](Self::move_from_san).
    ///
    /// # Grammar
    ///
    /// - Castling is written `O-O` / `O-O-O` (the internal commoner-takes-rook
    ///   encoding is translated, exactly like [`Move::write_uci`]).
    /// - Pawn pushes are the destination square (`e4`); pawn captures prefix the
    ///   origin file (`exd5`, including en passant); promotions append
    ///   `=Q`, `=R`, `=B`, or `=N` (`e8=Q`, `exd8=R`).
    /// - Piece moves write the piece letter (`N`, `B`, `R`, `Q`, `K` — `K` is
    ///   the atomic **commoner**, not a FIDE king), then standard
    ///   disambiguation by origin file, else origin rank, else both
    ///   (`Nbd2`, `R1a3`, `Qh4e1`), then `x` for a capture, then the
    ///   destination.
    /// - A trailing `#` is written when the move leaves the opponent with
    ///   [`Outcome::Loss`] (the enemy commoner exploded away, or the opponent
    ///   has no legal move while their commoner is attacked), and `+` when the
    ///   opponent's commoner is attacked but the game goes on.
    ///
    /// The suffix is computed with the same rules as [`Board::outcome`] and
    /// [`Board::checkers`], via [`EffectContext`](crate::EffectContext); the
    /// `#`/`+` decision never changes legality.
    ///
    /// This allocates one `String`; use [`write_san`](Self::write_san) to
    /// format without allocating.
    ///
    /// ```
    /// use atomic_movegen::{Board, Move};
    ///
    /// let board = Board::new();
    /// let e4 = Move::from_uci("e2e4").unwrap();
    /// assert_eq!(board.san(e4).as_deref(), Some("e4"));
    /// assert_eq!(board.move_from_san("e4"), Some(e4));
    /// ```
    #[must_use]
    pub fn san(&self, m: Move) -> Option<String> {
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        if !self.is_legal_with_state(m, &state) {
            return None;
        }
        let mut text = String::with_capacity(8);
        self.write_san_legal(m, &state, &mut text)
            .expect("writing to a String cannot fail");
        Some(text)
    }

    /// Write the SAN of `m` without allocating.
    ///
    /// `m` must be one of this position's legal moves; an illegal move makes
    /// this return [`fmt::Error`] (the same error a failing writer produces, so
    /// there is no separate SAN error type). [`san`](Self::san) is the
    /// validating, allocating counterpart.
    ///
    /// # Errors
    ///
    /// Returns [`fmt::Error`] when `m` is not legal or when the writer fails.
    ///
    /// ```
    /// use atomic_movegen::{Board, Move};
    ///
    /// let board = Board::new();
    /// let mut text = String::new();
    /// board
    ///     .write_san(Move::from_uci("g1f3").unwrap(), &mut text)
    ///     .unwrap();
    /// assert_eq!(text, "Nf3");
    /// ```
    pub fn write_san(&self, m: Move, out: &mut impl fmt::Write) -> fmt::Result {
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        if !self.is_legal_with_state(m, &state) {
            return Err(fmt::Error);
        }
        self.write_san_legal(m, &state, out)
    }

    /// SAN writer for a move that is known to be legal and for a populated
    /// `state`; the single implementation shared by [`san`](Self::san) and
    /// [`write_san`](Self::write_san).
    fn write_san_legal(
        &self,
        m: Move,
        state: &StateInfo,
        out: &mut impl fmt::Write,
    ) -> fmt::Result {
        debug_assert!(self.is_legal_with_state(m, state), "SAN needs a legal move");

        if m.is_castling() {
            // The internal encoding is commoner-origin -> rook square.
            return out.write_str(if file_of(m.to_sq()) == File::H {
                "O-O"
            } else {
                "O-O-O"
            });
        }

        let from = m.from_sq();
        let to = m.to_sq();
        let Some(pt) = self.piece_on(from).type_of() else {
            debug_assert!(false, "legal move with an empty origin");
            return Err(fmt::Error);
        };
        let capture = self.is_capture(m);

        if pt == PieceType::Pawn {
            if capture {
                out.write_char(file_char(from))?;
                out.write_char('x')?;
            }
            write_sq(out, to)?;
            if m.is_promotion() {
                out.write_char('=')?;
                out.write_char(san_piece_char(m.promotion_type()).expect("promotion piece"))?;
            }
        } else {
            out.write_char(san_piece_char(pt).expect("non-pawn piece"))?;
            self.write_san_disambiguation(m, state, out)?;
            if capture {
                out.write_char('x')?;
            }
            write_sq(out, to)?;
        }

        // The suffix is decided by the child position. `checks_enemy` is
        // exactly "the child is in check" (differentially tested in
        // `tests/effects.rs`), and a child `Outcome::Loss` is checkmate,
        // stalemate while attacked, or commoner extinction.
        let ctx = crate::effects::EffectContext::new(self, state);
        let effect = ctx.effect(self, m);
        match ctx.outcome_after(self, m) {
            Some(crate::types::Outcome::Loss) => out.write_char('#'),
            _ if effect.checks_enemy => out.write_char('+'),
            _ => Ok(()),
        }
    }

    /// Write the origin disambiguator for a non-pawn, non-castling move.
    ///
    /// Standard SAN: nothing when `m` is the only legal move of that piece type
    /// to the destination, otherwise the origin file when that is unique,
    /// else the origin rank, else the whole origin square.
    fn write_san_disambiguation(
        &self,
        m: Move,
        state: &StateInfo,
        out: &mut impl fmt::Write,
    ) -> fmt::Result {
        let from = m.from_sq();
        let to = m.to_sq();
        let pt = self.piece_on(from).type_of();
        let mut rivals = 0u32;
        let mut same_file = false;
        let mut same_rank = false;
        let _ = crate::movegen::for_each_legal_with_state::<()>(self, state, &mut |cand| {
            if cand.move_type() != MoveType::Castling
                && cand.to_sq() == to
                && cand.from_sq() != from
                && self.piece_on(cand.from_sq()).type_of() == pt
            {
                rivals += 1;
                same_file |= file_of(cand.from_sq()) == file_of(from);
                same_rank |= rank_of(cand.from_sq()) == rank_of(from);
            }
            std::ops::ControlFlow::Continue(())
        });

        if rivals == 0 {
            Ok(())
        } else if !same_file {
            out.write_char(file_char(from))
        } else if !same_rank {
            out.write_char(rank_char(from))
        } else {
            write_sq(out, from)
        }
    }

    /// Resolve SAN text to one of this position's legal moves.
    ///
    /// The accepted grammar is the one [`san`](Self::san) emits, plus the
    /// interoperability variants below. Returns `None` for malformed text,
    /// illegal moves, and ambiguity — a text that matches more than one legal
    /// move is never resolved by picking one.
    ///
    /// - Castling accepts `O-O` / `O-O-O` and `0-0` / `0-0-0`.
    /// - One trailing `+`, `#`, or `++` is stripped and otherwise ignored.
    /// - Pawn pushes are the destination square, pawn captures prefix the origin
    ///   file; the `x` of a capture is optional.
    /// - Promotions accept `=Q`, `=R`, `=B`, `=N` and the suffix-less `Q`, `R`,
    ///   `B`, `N` forms (either case). Promotion to the commoner (`K`) is
    ///   rejected: it is not an atomic move.
    /// - Piece moves accept the piece letters `N`, `B`, `R`, `Q`, `K` and
    ///   optional origin disambiguation by file, by rank, by both, or by a full
    ///   origin square (`Nb1d2`).
    ///
    /// ```
    /// use atomic_movegen::{Board, Move};
    ///
    /// let board = Board::new();
    /// assert_eq!(board.move_from_san("e4"), board.move_from_uci("e2e4"));
    /// assert_eq!(board.move_from_san("Nf3+"), board.move_from_uci("g1f3"));
    /// assert_eq!(board.move_from_san("Qf3"), None);
    /// ```
    #[must_use]
    pub fn move_from_san(&self, text: &str) -> Option<Move> {
        if text.is_empty() || !text.is_ascii() {
            return None;
        }

        // 1. Strip the check/mate suffix. Any other trailing punctuation stays
        //    in the text and makes the destination parse fail.
        let mut body = text;
        while let Some(stripped) = body.strip_suffix(['+', '#']) {
            body = stripped;
        }
        if body.is_empty() {
            return None;
        }

        // 2. Castling, in both spelling variants (and only if it is legal).
        let castling_uci = match body {
            "O-O" | "0-0" => Some(match self.position.side_to_move {
                Color::White => "e1g1",
                Color::Black => "e8g8",
            }),
            "O-O-O" | "0-0-0" => Some(match self.position.side_to_move {
                Color::White => "e1c1",
                Color::Black => "e8c8",
            }),
            _ => None,
        };
        if let Some(uci) = castling_uci {
            return self.move_from_uci(uci);
        }

        // 3. Strip the promotion suffix in its `=Q` and bare `Q` forms.
        let mut promotion = None;
        let mut rest = body;
        for candidate in ["=Q", "=R", "=B", "=N", "Q", "R", "B", "N"] {
            if let Some(stripped) = rest.strip_suffix(candidate) {
                promotion = san_piece_type(candidate.as_bytes()[candidate.len() - 1]);
                rest = stripped;
                break;
            }
        }
        if promotion.is_none() {
            // Lower-case promotion letters are accepted too, but only behind an
            // `=`, so a destination square such as `b4` is never misread.
            for candidate in ["=q", "=r", "=b", "=n"] {
                if let Some(stripped) = rest.strip_suffix(candidate) {
                    promotion = san_piece_type(candidate.as_bytes()[1]);
                    rest = stripped;
                    break;
                }
            }
        }
        // A promotion letter may not be mistaken for the destination of a
        // piece move: only a pawn destination is ever followed by one.
        if promotion.is_some() && rest.len() < 2 {
            return None;
        }

        // 4. Destination, then the optional capture marker.
        if rest.len() < 2 {
            return None;
        }
        let split = rest.len() - 2;
        let to = crate::types::parse_sq(&rest[split..])?;
        let mut prefix = &rest[..split];
        let mut capture = None;
        if let Some(stripped) = prefix.strip_suffix('x') {
            capture = Some(true);
            prefix = stripped;
        }

        // 5. Origin: a piece letter with optional disambiguation, a capture file
        //    letter, or nothing (a pawn push).
        let mut piece_type = Some(PieceType::Pawn);
        let mut from_file = None;
        let mut from_rank = None;
        let mut from_sq = None;
        let first = prefix.as_bytes().first().copied();
        match first {
            None => {}
            Some(c @ (b'N' | b'B' | b'R' | b'Q' | b'K')) => {
                piece_type = san_piece_type(c);
                if promotion.is_some() {
                    // Only a pawn promotes.
                    return None;
                }
                let disambiguation = &prefix[1..];
                match disambiguation.len() {
                    0 => {}
                    1 => {
                        let c = disambiguation.as_bytes()[0];
                        if let Some(file) = parse_file_char(c) {
                            from_file = Some(file);
                        } else {
                            from_rank = Some(parse_rank_char(c)?);
                        }
                    }
                    2 => {
                        let sq = crate::types::parse_sq(disambiguation)?;
                        from_sq = Some(sq);
                    }
                    _ => return None,
                }
            }
            Some(c) => {
                // A pawn capture: exactly one file letter.
                if prefix.len() != 1 {
                    return None;
                }
                from_file = Some(parse_file_char(c)?);
            }
        }

        // 6. Filter the legal moves; exactly one candidate must survive.
        let mut state = StateInfo::new();
        self.populate_state(&mut state);
        let mut found = None;
        let _ = crate::movegen::for_each_legal_with_state::<()>(self, &state, &mut |candidate| {
            let matches = candidate.to_sq() == to
                && self.piece_on(candidate.from_sq()).type_of() == piece_type
                && match promotion {
                    Some(pt) => candidate.is_promotion() && candidate.promotion_type() == pt,
                    None => !candidate.is_promotion(),
                }
                && capture.is_none_or(|_| self.is_capture(candidate))
                && from_file.is_none_or(|f| file_of(candidate.from_sq()) == f)
                && from_rank.is_none_or(|r| rank_of(candidate.from_sq()) == r)
                && from_sq.is_none_or(|sq| candidate.from_sq() == sq);
            if matches {
                if found.is_some() {
                    found = Some(Move::NONE);
                    return std::ops::ControlFlow::Break(());
                }
                found = Some(candidate);
            }
            std::ops::ControlFlow::Continue(())
        });

        found.filter(|&m| m != Move::NONE)
    }
}

/// Return the SAN piece letter for a non-pawn piece type.
///
/// The commoner uses `K`, the same letter as the FEN label.
fn san_piece_char(pt: PieceType) -> Option<char> {
    match pt {
        PieceType::Knight => Some('N'),
        PieceType::Bishop => Some('B'),
        PieceType::Rook => Some('R'),
        PieceType::Queen => Some('Q'),
        PieceType::Commoner => Some('K'),
        PieceType::Pawn => None,
    }
}

/// Inverse of [`san_piece_char`].
fn san_piece_type(c: u8) -> Option<PieceType> {
    match c.to_ascii_uppercase() {
        b'N' => Some(PieceType::Knight),
        b'B' => Some(PieceType::Bishop),
        b'R' => Some(PieceType::Rook),
        b'Q' => Some(PieceType::Queen),
        b'K' => Some(PieceType::Commoner),
        _ => None,
    }
}

/// Parse a SAN file letter (`a`–`h`).
fn parse_file_char(c: u8) -> Option<File> {
    match c {
        b'a'..=b'h' => Some(file_of(Square::from_u8(c - b'a'))),
        _ => None,
    }
}

/// Parse a SAN rank digit (`1`–`8`).
fn parse_rank_char(c: u8) -> Option<Rank> {
    match c {
        b'1'..=b'8' => Some(rank_of(Square::from_u8((c - b'1') << 3))),
        _ => None,
    }
}

/// Write the file letter of a square.
fn file_char(sq: Square) -> char {
    (b'a' + file_of(sq) as u8) as char
}

/// Write the rank digit of a square.
fn rank_char(sq: Square) -> char {
    (b'1' + rank_of(sq) as u8) as char
}

/// Write a square in algebraic notation without allocating.
fn write_sq(out: &mut impl fmt::Write, sq: Square) -> fmt::Result {
    out.write_char(file_char(sq))?;
    out.write_char(rank_char(sq))
}

/// Return `true` when the piece standing on `sq` is attacked by one of
/// `attackers`.
///
/// `sq_color` is the color of the piece on `sq`, so the pawn term looks for
/// opposing pawns; `attackers` is the set of opposing pieces on `occupied` and
/// `by_type` describes the opposing piece placement on `occupied`. Commoner
/// attacks are intentionally excluded: an adjacent commoner grants immunity
/// instead of a check, so callers handle that case separately.
///
/// This is the single implementation of the “is this square attacked” rule
/// shared by [`Board::legal`], [`is_attacked_by`]-based effect queries, and
/// [`Board::attackers_to`]'s per-color companion.
#[inline(always)]
pub(crate) fn is_attacked_by(
    sq: Square,
    occupied: Bitboard,
    by_type: &[Bitboard; PieceType::NB],
    attackers: Bitboard,
    sq_color: Color,
) -> bool {
    // Pawns.
    if !(attacks::pawn_attacks(sq_color, sq) & by_type[PieceType::Pawn as usize] & attackers)
        .is_empty()
    {
        return true;
    }

    // Knights.
    if !(attacks::knight_attacks(sq) & by_type[PieceType::Knight as usize] & attackers).is_empty() {
        return true;
    }

    // Bishop/queen sliders.
    if !(attacks::bishop_attacks(sq, occupied)
        & (by_type[PieceType::Bishop as usize] | by_type[PieceType::Queen as usize])
        & attackers)
        .is_empty()
    {
        return true;
    }

    // Rook/queen sliders. This is skipped when a pawn, knight, bishop, or
    // queen already hit.
    !(attacks::rook_attacks(sq, occupied)
        & (by_type[PieceType::Rook as usize] | by_type[PieceType::Queen as usize])
        & attackers)
        .is_empty()
}

/// One-shot classification of a non-castling move for the child-key helpers
/// ([`Board::hash_after`], [`Board::rule50_after`], and
/// [`Board::hash_and_rule50_after`]).
///
/// Both child-key quantities need the moving piece and whether the move is a
/// pawn move or a capture; computing them once lets
/// [`Board::hash_and_rule50_after`] avoid a second origin lookup.
#[derive(Debug, Clone, Copy)]
struct MoveClass {
    /// Piece standing on the move's origin square (never [`NO_PIECE`] for a
    /// move `do_move` accepts).
    piece: Piece,
    /// The moving piece is a pawn (a promotion still moves a pawn).
    is_pawn: bool,
    /// The destination square is occupied (not true for en passant).
    is_capture: bool,
}

/// Return the Zobrist piece key for the piece with raw encoding `raw`
/// standing on `sq`.
///
/// `ZOBRIST.piece` is `[[u64; 16]; 64]`. Every `Square` used as a board index
/// is `< 64` and every piece raw encoding is `< 16` by construction (see
/// `types.rs`); debug builds assert both. The default build performs ordinary
/// indexed loads (safe Rust). With `unsafe-optimizations` the loads skip their
/// bounds checks, which was a measurable share of `hash_after`'s runtime.
#[inline(always)]
fn piece_key_raw(sq: Square, raw: u8) -> u64 {
    debug_assert!((sq as usize) < 64);
    debug_assert!((raw as usize) < 16);
    #[cfg(feature = "unsafe-optimizations")]
    {
        // SAFETY: in-bounds per the invariants above (debug-asserted).
        unsafe {
            *crate::zobrist::ZOBRIST
                .piece
                .get_unchecked(sq as usize)
                .get_unchecked(raw as usize)
        }
    }
    #[cfg(not(feature = "unsafe-optimizations"))]
    {
        crate::zobrist::ZOBRIST.piece[sq as usize][raw as usize]
    }
}

/// Same as [`piece_key_raw`] for a full [`Piece`].
#[inline(always)]
pub(crate) fn piece_key(sq: Square, piece: Piece) -> u64 {
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
pub(crate) fn castling_rights_after(
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
///
/// The tuple is `(commoner origin, commoner destination, rook origin, rook
/// destination)`. Shared with [`crate::effects`], which mirrors the castling
/// relocation on bitboards without a board write.
pub(crate) fn castling_squares(us: Color, kingside: bool) -> (Square, Square, Square, Square) {
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
    /// The node's occupancy, hoisted out of the per-candidate capture test.
    occupied: Bitboard,
}

impl TrivialLegality {
    /// Derive the node-constant fast-path data from a populated [`StateInfo`].
    ///
    /// All inputs are only read, so the resulting value stays valid for every
    /// candidate move of the node.
    #[inline(always)]
    pub(crate) fn new(view: &PositionView, state: &StateInfo) -> Self {
        let us = view.side_to_move();
        debug_assert!(
            state.commoners_count == 0 || !view.commoners(us).is_empty(),
            "StateInfo.commoners_count disagrees with the board"
        );
        let safe_origins = if state.commoners_count == 0 {
            // No own commoner: every candidate must go through `legal`, which
            // rejects all of them (matching the old `commoners_count == 0` guard).
            Bitboard::EMPTY
        } else {
            !(view.commoners(us) | state.pinned)
        };
        Self {
            checkers: state.checkers,
            safe_origins,
            occupied: view.occupied(),
        }
    }

    /// Returns `true` when `m` is trivially legal.
    ///
    /// `m` must come from the pseudo-legal generator (valid origin and
    /// destination squares); this is a crate-private precondition, not an input
    /// validation point.
    #[inline(always)]
    pub(crate) fn is_trivially_legal(&self, m: Move) -> bool {
        debug_assert!(m != Move::NONE);
        if !self.checkers.is_empty() {
            return false; // in check: every move needs the full check
        }
        if m.needs_full_legal_check() {
            return false; // en passant and castling
        }
        if self.occupied.contains(m.dest_index()) {
            return false; // capture
        }
        // Origin must be neither a commoner nor pinned.
        self.safe_origins.contains(m.origin_index())
    }
}

/// Widened part of the fast-legality ladder: node-constant data plus
/// accept/reject rules for the moves [`TrivialLegality`] cannot decide.
///
/// The rules are sufficient conditions, never a second rules implementation:
/// anything undecided falls back to [`Board::legal`], which stays the oracle.
/// They rest on two facts about atomic chess.
///
/// 1. **Only removals matter for a commoner that stays put.** Enemy pieces
///    never move inside one move, so the enemy's attacks on our commoner can
///    only grow when the move *removes* a piece that blocked a slider's ray.
///    Only the pieces between our commoner and an aligned enemy slider, of
///    either colour (`sensitive` below), can do that: if the move empties none
///    of them, our commoner cannot become attacked — provided it is not
///    attacked now (`state.checkers` empty) and it does not move itself. When
///    a blocker is emptied, [`WidenedLegality::rays_stay_closed`] refines the
///    answer per sniper.
/// 2. **A quiet commoner move is a one-square relocation.** The post-move
///    occupancy is the pre-move occupancy with the origin emptied and the
///    destination filled. Whether the *destination* is attacked does not depend
///    on the destination being occupied (a piece cannot block an attack on its
///    own square), so the shared attack query with the origin emptied answers
///    it exactly.
///
/// The move's *emptied* set is `{from}` for quiet moves and promotions, and
/// `{from, to} ∪ (non-pawns adjacent to to) ∪ {en passant victim}` for
/// captures, which is exactly the [`Board::blast_casualties`] set plus the
/// capturer's origin square (the capturer dies at ground zero).
///
/// A move that leaves the mover with fewer commoners is illegal (atomic
/// self-explosion). A capture that removes every enemy commoner ends the game
/// at once, so it is legal regardless of the mover's own exposure — the same
/// branch order as [`Board::legal`] (own-commoner count first, then enemy
/// extinction, then the attack query).
///
/// Multi-commoner boards (not reachable in legal play) fall back: the rules
/// above reason about one commoner square, while [`Board::legal`] uses the
/// lowest commoner of the post-move set.
pub(crate) struct WidenedLegality {
    /// The commoners of the side to move (`Board::commoners(us)`).
    our_commoners: Bitboard,
    /// The commoners of the opponent (`Board::commoners(them)`).
    their_commoners: Bitboard,
    /// Every occupied non-pawn square: the pieces a blast can destroy.
    non_pawns: Bitboard,
    /// The node's occupancy.
    occupied: Bitboard,
    /// Lazily computed aligned enemy sliders and their blockers.
    snipers: Cell<Option<(Bitboard, Bitboard)>>,
}

impl WidenedLegality {
    /// Snapshot the node-constant placement data.
    ///
    /// `view` must not change while the value is in use. Occupancy, the
    /// per-type sets, and the pawn set of a node are fixed, so every field is
    /// valid for all candidates.
    #[inline(always)]
    pub(crate) fn new(view: &PositionView) -> Self {
        let us = view.side_to_move();
        let them = us.flip();
        Self {
            our_commoners: view.commoners(us),
            their_commoners: view.commoners(them),
            non_pawns: view.occupied() & !view.pieces_pt(PieceType::Pawn),
            occupied: view.occupied(),
            snipers: Cell::new(None),
        }
    }

    /// Decide a non-castling pseudo-legal move below the oracle, or return
    /// `None` when the ladder cannot decide it.
    ///
    /// `state.checkers` must be empty: in check, every move needs the oracle.
    /// `m` must come from the pseudo-legal generator (crate-private
    /// precondition, not an input validation point).
    #[inline(always)]
    pub(crate) fn decide(&self, view: &PositionView, m: Move) -> Option<bool> {
        debug_assert!(m != Move::NONE);
        let move_type = m.move_type();
        if move_type == MoveType::Castling {
            // Pass-through-square attacks are a separate rule surface.
            return None;
        }

        let from = m.from_sq();
        let to = m.to_sq();
        let from_bb = Bitboard::square_bb(from);
        let to_bb = Bitboard::square_bb(to);
        let is_capture = move_type == MoveType::EnPassant || self.occupied.contains(m.dest_index());

        if !(self.our_commoners & from_bb).is_empty() {
            return self.decide_commoner_move(view, from, to, is_capture);
        }

        // Every square the move empties.
        let mut emptied = from_bb;
        if is_capture {
            emptied = emptied | to_bb | (attacks::king_attacks(to) & self.non_pawns);
            if move_type == MoveType::EnPassant {
                let victim = match view.side_to_move() {
                    Color::White => Square::from_index(to as i8 - 8),
                    Color::Black => Square::from_index(to as i8 + 8),
                };
                emptied = emptied | Bitboard::square_bb(victim);
            }
        }

        // Losing a commoner is self-explosion (or, on a multi-commoner board,
        // an undecided count change).
        let lost_commoners = self.our_commoners & emptied;
        if !lost_commoners.is_empty() {
            return if lost_commoners.count() == self.our_commoners.count() {
                Some(false)
            } else {
                None
            };
        }

        // The blast removes every enemy commoner: the game ends immediately, so
        // the mover's own exposure no longer matters.
        if is_capture && (self.their_commoners & !emptied).is_empty() {
            return Some(true);
        }

        // No piece blocking an enemy slider's ray to our commoner is emptied, so
        // the commoner stays as safe as it is now (checked by the caller).
        let (snipers, sensitive) = self.sniper_data(view);
        if (emptied & sensitive).is_empty() {
            return Some(true);
        }
        // A blocker on a sniper ray is emptied; the ray is still closed when the
        // sniper itself is removed or another blocker remains.
        if self.rays_stay_closed(snipers, emptied) {
            return Some(true);
        }
        None
    }

    /// Decide a move of the mover's commoner (`from` is in `our_commoners`).
    #[inline]
    fn decide_commoner_move(
        &self,
        view: &PositionView,
        from: Square,
        to: Square,
        is_capture: bool,
    ) -> Option<bool> {
        if self.our_commoners.count() != 1 {
            // `Board::legal` reasons about the lowest commoner of the post-move
            // set; a single commoner keeps this branch exact.
            return None;
        }
        if is_capture {
            // The capturing commoner is destroyed at ground zero.
            return Some(false);
        }
        if !(self.their_commoners & attacks::king_attacks(to)).is_empty() {
            // Touching the enemy commoner grants immunity to every attack.
            return Some(true);
        }
        // The one-square relocation empties `from` and fills `to`; the arrival
        // cannot block an attack on `to`, so only the origin matters.
        let occupied_after = (self.occupied & !Bitboard::square_bb(from)) | Bitboard::square_bb(to);
        let us = view.side_to_move();
        Some(!is_attacked_by(
            to,
            occupied_after,
            view.by_type(),
            view.pieces_color(us.flip()),
            us,
        ))
    }

    /// The node's aligned enemy sliders and the pieces blocking their rays,
    /// computed on first use.
    #[inline]
    fn sniper_data(&self, view: &PositionView) -> (Bitboard, Bitboard) {
        if let Some(data) = self.snipers.get() {
            return data;
        }
        debug_assert_eq!(self.our_commoners.count(), 1);
        let data = compute_sniper_data(view, self.our_commoners.lsb());
        self.snipers.set(Some(data));
        data
    }

    /// Returns `true` when every sniper in `snipers` that survives `emptied`
    /// still has a blocker on its ray after `emptied` is removed.
    ///
    /// Blocker squares are read from the node's occupancy, so a piece arriving
    /// on the move's destination is not counted as a blocker. That is sound:
    /// the arrival can only close a ray, so ignoring it can only make a ray look
    /// more open, and an open ray falls back to [`Board::legal`].
    fn rays_stay_closed(&self, snipers: Bitboard, emptied: Bitboard) -> bool {
        let occupied = self.occupied;
        let ksq = self.our_commoners.lsb();
        let mut remaining = snipers & !emptied;
        while !remaining.is_empty() {
            let sniper = remaining.pop_lsb();
            if (between_bb(ksq, sniper) & occupied & !emptied).is_empty() {
                return false;
            }
        }
        true
    }
}

/// The aligned enemy sliders of the commoner on `ksq` and the squares that
/// block their rays, as `(snipers, blockers)`.
///
/// A sniper is an enemy rook, bishop, or queen on a `ksq` ray of its own line
/// type. `blockers` holds every occupied square strictly between `ksq` and a
/// sniper, both colours: only removals can open a slider's ray, and a capture's
/// blast empties squares of either colour. A blocker square may block two
/// snipers at once; [`WidenedLegality::rays_stay_closed`] resolves those cases
/// per sniper.
fn compute_sniper_data(view: &PositionView, ksq: Square) -> (Bitboard, Bitboard) {
    let occupied = view.occupied();
    let them_bb = view.pieces_color(view.side_to_move().flip());
    // Rook-lines and bishop-lines are disjoint for a single square, so a slider
    // is a sniper exactly when its own line type reaches `ksq`.
    let orthogonal = (view.pieces_pt(PieceType::Rook) | view.pieces_pt(PieceType::Queen))
        & attacks::rook_attacks(ksq, Bitboard::EMPTY);
    let diagonal = (view.pieces_pt(PieceType::Bishop) | view.pieces_pt(PieceType::Queen))
        & attacks::bishop_attacks(ksq, Bitboard::EMPTY);
    let snipers = (orthogonal | diagonal) & them_bb;

    let mut remaining = snipers;
    let mut blockers = Bitboard::EMPTY;
    while !remaining.is_empty() {
        let sniper = remaining.pop_lsb();
        blockers = blockers | (between_bb(ksq, sniper) & occupied);
    }
    (snipers, blockers)
}

/// The fast-legality ladder of a node: the trivial quiet fast path first, then
/// the widened rules, then [`Board::legal`] as the oracle.
///
/// Build once per node and ask [`is_legal`](Self::is_legal) for every
/// pseudo-legal candidate. The ladder never overrides the oracle: every branch
/// either reproduces a [`Board::legal`] outcome for a proven reason (see
/// [`WidenedLegality`]) or delegates to it. The widened state is created on
/// first use, so nodes whose candidates are all trivially legal pay nothing for
/// it.
pub(crate) struct FastLegality {
    /// Quiet-move fast path (checked first: it is the cheapest rule).
    trivial: TrivialLegality,
    /// Widened rules, created on first non-trivial candidate.
    widened: OnceCell<WidenedLegality>,
    /// `state.checkers` is non-empty: every move needs the oracle.
    in_check: bool,
    /// Both sides still have a commoner. `false` means the game is already over
    /// and no move is legal.
    playable: bool,
}

impl FastLegality {
    /// Derive the node-constant ladder data from a populated [`StateInfo`].
    #[inline(always)]
    pub(crate) fn new(view: &PositionView, state: &StateInfo) -> Self {
        Self {
            trivial: TrivialLegality::new(view, state),
            widened: OnceCell::new(),
            in_check: !state.checkers.is_empty(),
            playable: state.commoners_count != 0 && state.them_commoners_count != 0,
        }
    }

    /// Returns `true` when the pseudo-legal move `m` is legal.
    ///
    /// Exactly membership in `generate_legal_with_state(board, state, ..)` for
    /// the same populated `state`. `m` must come from the pseudo-legal generator
    /// (crate-private precondition, not an input validation point).
    #[inline(always)]
    pub(crate) fn is_legal(&self, view: &PositionView, m: Move, state: &StateInfo) -> bool {
        // The trivial quiet fast path stays a leaf: it accepts most candidates
        // without touching the widened state or the oracle.
        self.trivial.is_trivially_legal(m) || self.is_legal_widened(view, m, state)
    }

    /// The widened rules plus the oracle fallback.
    fn is_legal_widened(&self, view: &PositionView, m: Move, state: &StateInfo) -> bool {
        if !self.playable {
            // Terminal position: `Board::legal` rejects every move.
            return false;
        }
        if self.in_check {
            return crate::position::legal_in_view(view, m, state);
        }
        let widened = self.widened.get_or_init(|| WidenedLegality::new(view));
        match widened.decide(view, m) {
            Some(verdict) => verdict,
            None => crate::position::legal_in_view(view, m, state),
        }
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
        writeln!(f, "Side to move: {:?}", self.position.side_to_move)?;
        writeln!(f, "Castling: {:b}", self.position.castling_rights)?;
        writeln!(f, "EP: {:?}", self.position.ep_square)?;
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
            board.castling_rights(),
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
    fn test_lichess_sanitizes_castling_rights_individually() {
        let mut parsed =
            parse_fen_syntax("4k3/8/8/8/8/8/8/4K2R w KQkq - 0 1").expect("valid syntax");
        sanitize_lichess(&mut parsed);
        assert_eq!(parsed.castling_rights, WK_CASTLE);

        let mut parsed = parse_fen_syntax("4k3/8/8/8/8/8/8/R3K3 w KQ - 0 1").expect("valid syntax");
        sanitize_lichess(&mut parsed);
        assert_eq!(parsed.castling_rights, WQ_CASTLE);
    }

    #[test]
    fn test_lichess_sanitizes_ep_geometry() {
        let mut parsed =
            parse_fen_syntax("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").expect("valid syntax");
        sanitize_lichess(&mut parsed);
        assert_eq!(parsed.ep_square, Some(Square::D6));

        for fen in [
            "4k3/8/8/3pP3/8/8/8/4K3 w - d3 0 1",
            "4k3/8/8/3pP3/8/8/8/4K3 w - c6 0 1",
            "4k3/8/3R4/3pP3/8/8/8/4K3 w - d6 0 1",
            "4k3/3R4/8/3pP3/8/8/8/4K3 w - d6 0 1",
        ] {
            let mut parsed = parse_fen_syntax(fen).expect("valid syntax");
            sanitize_lichess(&mut parsed);
            assert_eq!(parsed.ep_square, None, "{fen}");
        }
    }

    #[test]
    fn test_lichess_terminal_missing_side_to_move_is_accepted() {
        assert!(Board::from_fen_with("4k3/8/8/8/8/8/8/8 w - - 0 1", FenPolicy::Lichess).is_ok());
        assert!(Board::from_fen_with("8/8/8/8/8/8/8/4K3 w - - 0 1", FenPolicy::Lichess).is_err());
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

    #[test]
    fn terminal_reason_table() {
        // One row per branch of the WDL precedence, including the two
        // clock/mate interactions and the material shortcut that
        // `is_terminal` deliberately excludes.
        let cases: &[(&str, Outcome, TerminalReason, bool)] = &[
            // Side to move extinct.
            (
                "4k3/8/8/8/8/8/8/8 w - - 0 1",
                Outcome::Loss,
                TerminalReason::Extinction,
                true,
            ),
            // Opponent extinct.
            (
                "8/8/8/8/8/8/8/4K3 w - - 0 1",
                Outcome::Win,
                TerminalReason::Extinction,
                true,
            ),
            // K vs K: WDL draw, but the game is not literally over.
            (
                "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
                Outcome::Draw,
                TerminalReason::MaterialDraw,
                false,
            ),
            // Checkmate.
            (
                "7K/8/8/8/8/8/1Q6/k7 b - - 0 1",
                Outcome::Loss,
                TerminalReason::NoMovesInCheck,
                true,
            ),
            // Checkmate on the clock's last ply: mate outranks the clock.
            (
                "7K/8/8/8/8/8/1Q6/k7 b - - 100 1",
                Outcome::Loss,
                TerminalReason::NoMovesInCheck,
                true,
            ),
            // Stalemate: no legal moves, commoner safe.
            (
                "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
                Outcome::Draw,
                TerminalReason::Stalemate,
                true,
            ),
            // Clock reached 100 with legal moves available.
            (
                "4k3/8/8/8/8/8/8/4K2R w - - 100 1",
                Outcome::Draw,
                TerminalReason::FiftyMoveRule,
                true,
            ),
        ];

        for &(fen, outcome, reason, terminal) in cases {
            let board = Board::from_fen(fen).expect("terminal FEN must parse");
            let expected = Terminal { outcome, reason };

            assert_eq!(board.outcome(), Some(outcome), "outcome for {fen}");
            assert_eq!(board.terminal(), Some(expected), "terminal for {fen}");
            assert_eq!(
                board.terminal().map(|t| t.outcome),
                board.outcome(),
                "terminal/outcome agreement for {fen}"
            );
            assert_eq!(board.is_terminal(), terminal, "is_terminal for {fen}");

            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            assert_eq!(
                board.terminal_with_state(&state),
                Some(expected),
                "terminal_with_state for {fen}"
            );
            let moves_exist = crate::movegen::has_legal_move_with_state(&board, &state);
            assert_eq!(
                board.terminal_with(&state, moves_exist),
                Some(expected),
                "terminal_with for {fen}"
            );
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

        // The terminal API must agree with `outcome` on the WDL result and
        // agree across its three forms.
        let terminal = board.terminal();
        assert_eq!(
            terminal.map(|t| t.outcome),
            expected,
            "terminal outcome mismatch at {}",
            board.fen()
        );
        assert_eq!(
            terminal,
            board.terminal_with_state(&state),
            "terminal_with_state mismatch at {}",
            board.fen()
        );
        assert_eq!(
            terminal,
            board.terminal_with(&state, moves_exist),
            "terminal_with mismatch at {}",
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

        // The combined accessor must agree with both separate functions.
        assert_eq!(
            board.hash_and_rule50_after(m),
            (expected_hash, expected_rule50),
            "hash_and_rule50_after mismatch for {} at {}",
            m.to_uci(),
            board.fen()
        );

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

    /// Assert that `blast_casualties` reproduces the removals `do_move`
    /// records for a single move.
    fn assert_blast_casualties_equivalence(board: &Board, m: Move) {
        let expected = board.blast_casualties(m);

        let mut played = board.clone();
        let mut state = StateInfo::new();
        played.do_move(m, &mut state);

        let mut white = Bitboard::EMPTY;
        let mut black = Bitboard::EMPTY;
        // `do_move` records blast removals in `captured`, but the ordinary
        // capture victim lives in `cap_sq`/`cap_piece`.
        let removed = state
            .captured
            .iter()
            .take(state.captured_count as usize)
            .copied()
            .chain(state.cap_sq.map(|sq| (sq, state.cap_piece)));
        for (sq, piece) in removed {
            let sq_bb = Bitboard::square_bb(sq);
            match piece.color().expect("captured pieces have a color") {
                Color::White => white = white | sq_bb,
                Color::Black => black = black | sq_bb,
            }
        }

        assert_eq!(
            expected.white,
            white,
            "blast white casualties mismatch for {} at {}",
            m.to_uci(),
            board.fen()
        );
        assert_eq!(
            expected.black,
            black,
            "blast black casualties mismatch for {} at {}",
            m.to_uci(),
            board.fen()
        );

        let ep_victim = if m.move_type() == MoveType::EnPassant {
            let to = m.to_sq();
            Some(match board.side_to_move() {
                Color::White => Square::from_index(to as i8 - 8),
                Color::Black => Square::from_index(to as i8 + 8),
            })
        } else {
            None
        };
        assert_eq!(
            expected.en_passant_victim,
            ep_victim,
            "blast en-passant victim mismatch for {} at {}",
            m.to_uci(),
            board.fen()
        );
        assert_eq!(expected.is_empty(), expected.all().is_empty());
    }

    fn check_blast_and_playout(
        board: &mut Board,
        rng: &mut Xorshift2,
        depth: u32,
        visited: &mut u64,
    ) {
        *visited += 1;

        let mut pseudo = MoveList::new();
        crate::movegen::generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            assert_blast_casualties_equivalence(board, m);
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
        check_blast_and_playout(board, rng, depth - 1, visited);
        board.undo_move(m, &state);
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

    /// Hard-coded keys are the compatibility guard for persisted tables. Any
    /// change to these values needs a major-version decision and a
    /// `ZOBRIST_VERSION` bump.
    #[test]
    fn zobrist_golden_keys() {
        let cases: &[(&str, u64, u64, u64)] = &[
            (
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                0x5ab8_4d1c_fc77_f79e,
                0x5ab8_4d1c_fc77_f79e,
                0x5ab8_4d1c_fc77_f79e,
            ),
            (
                "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
                0x4c47_301a_ad9e_4ede,
                0x4c47_301a_ad9e_4ede,
                0x0196_5184_bb13_1d9a,
            ),
            (
                "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
                0x6c55_68b9_2aee_6ad4,
                0x6c55_68b9_2aee_6ad4,
                0x6c55_68b9_2aee_6ad4,
            ),
            (
                "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
                0xebf4_6b70_88e7_25bf,
                0xebf4_6b70_88e7_25bf,
                0xebf4_6b70_88e7_25bf,
            ),
        ];

        for &(fen, hash, always, legal) in cases {
            let board = Board::from_fen(fen).expect("golden FEN must parse");
            assert_eq!(board.hash(), hash, "hash for {fen}");
            assert_eq!(
                board.hash_with(EnPassantMode::Always),
                always,
                "hash_with(Always) for {fen}"
            );
            assert_eq!(
                board.hash_with(EnPassantMode::Legal),
                legal,
                "hash_with(Legal) for {fen}"
            );
        }
    }

    #[test]
    fn zobrist_version_is_stable() {
        // Update only together with an announced key-schema change.
        assert_eq!(crate::ZOBRIST_VERSION, 1);
    }

    #[test]
    fn hash_ep_modes() {
        // Same pieces, same side to move, castling-rights-free; only the ep
        // target differs, and it is not capturable in either position.
        let with_ep = Board::from_fen("4k3/8/8/3p4/8/8/8/4K3 w - d6 0 2").unwrap();
        let without_ep = Board::from_fen("4k3/8/8/3p4/8/8/8/4K3 w - - 0 2").unwrap();

        assert_eq!(
            with_ep.hash_with(EnPassantMode::Legal),
            without_ep.hash_with(EnPassantMode::Legal),
            "Legal must canonicalize a non-capturable ep target away"
        );
        assert_ne!(
            with_ep.hash(),
            without_ep.hash(),
            "Always keeps the stored ep target"
        );
        assert_ne!(
            with_ep.hash_with(EnPassantMode::Always),
            without_ep.hash_with(EnPassantMode::Always)
        );
    }

    /// Dedicated check for the combined child-key accessor: over seeded random
    /// playouts it must equal `(hash_after, rule50_after)` for every
    /// pseudo-legal move (the helper also compares that pair with a cloned
    /// board's `do_move` result).
    #[test]
    fn hash_and_rule50_after_matches_separate() {
        const START_FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        ];

        let mut visited = 0u64;
        for (i, fen) in START_FENS.iter().enumerate() {
            for seed in 0..8u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0xB502_6F5A_A2C3_5C01 ^ ((i as u64) << 32) ^ seed);
                check_after_and_playout(&mut board, &mut rng, 100, &mut visited);
            }
        }
        assert!(
            visited > 500,
            "expected several hundred positions, got {visited}"
        );
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
    fn blast_casualties_matches_do_move() {
        const START_FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            "4k3/4n3/8/2Pp4/8/8/8/4K3 w - d6 0 2",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        ];

        let mut visited = 0u64;
        for (i, fen) in START_FENS.iter().enumerate() {
            for seed in 0..8u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0xC2B2_AE3D_27D4_EB4F ^ ((i as u64) << 32) ^ seed);
                check_blast_and_playout(&mut board, &mut rng, 200, &mut visited);
            }
        }
        assert!(
            visited > 2_000,
            "expected a few thousand positions, got {visited}"
        );
    }

    #[test]
    fn blast_zone_geometry() {
        let center = Board::blast_zone(Square::D4);
        assert_eq!(center.count(), 9);
        assert!(center & Bitboard::square_bb(Square::D4) != Bitboard::EMPTY);
        for sq in [
            Square::C3,
            Square::C4,
            Square::C5,
            Square::D3,
            Square::D5,
            Square::E3,
            Square::E4,
            Square::E5,
        ] {
            assert!(
                center & Bitboard::square_bb(sq) != Bitboard::EMPTY,
                "missing {sq:?}"
            );
        }

        assert_eq!(Board::blast_zone(Square::A1).count(), 4);
        assert_eq!(Board::blast_zone(Square::H8).count(), 4);
        assert!(Board::blast_zone(Square::A1) & Bitboard::square_bb(Square::A1) != Bitboard::EMPTY);
    }

    #[test]
    fn blast_casualties_normal_capture() {
        // Qd4xd5: the black knight on d5 is captured, the black knight on e6
        // and the white knight on c4 are in the ring, and the queen dies at
        // ground zero. The origin d4 is not part of the removal set.
        let board = Board::from_fen("4k3/8/4n3/3n4/2NQ4/8/8/4K3 w - - 0 1").unwrap();
        let m = Move::make_move(Square::D4, Square::D5);
        let casualties = board.blast_casualties(m);

        let whites = Bitboard::square_bb(Square::C4) | Bitboard::square_bb(Square::D5);
        let blacks = Bitboard::square_bb(Square::D5) | Bitboard::square_bb(Square::E6);
        assert_eq!(casualties.white, whites);
        assert_eq!(casualties.black, blacks);
        assert_eq!(casualties.all(), whites | blacks);
        assert_eq!(casualties.en_passant_victim, None);
        assert!(!casualties.is_empty());
    }

    #[test]
    fn blast_casualties_pawn_immunity() {
        // exd5: the black pawn on d5 is the victim and the white pawn at
        // ground zero dies. The white pawn on c5 and the black pawn on e5 are
        // in the ring but immune. Nothing else is removed.
        let board = Board::from_fen("4k3/8/8/2Ppp3/4P3/8/8/4K3 w - - 0 1").unwrap();
        let m = Move::make_move(Square::E4, Square::D5);
        let casualties = board.blast_casualties(m);

        assert_eq!(casualties.white, Bitboard::square_bb(Square::D5));
        assert_eq!(casualties.black, Bitboard::square_bb(Square::D5));
        assert_eq!(casualties.en_passant_victim, None);

        // The ring pawns survive on the actual board too.
        let mut played = board.clone();
        let mut state = StateInfo::new();
        played.do_move(m, &mut state);
        assert_eq!(
            played.piece_on(Square::C5),
            make_piece(Color::White, PieceType::Pawn)
        );
        assert_eq!(
            played.piece_on(Square::E5),
            make_piece(Color::Black, PieceType::Pawn)
        );
    }

    #[test]
    fn blast_casualties_en_passant() {
        // cxd6 e.p.: the victim is the black pawn on d5 behind the target; the
        // black knight on e7 is in the ring; the capturing pawn dies at d6.
        let board = Board::from_fen("4k3/4n3/8/2Pp4/8/8/8/4K3 w - d6 0 2").unwrap();
        let m = Move::make_enpassant(Square::C5, Square::D6);
        let casualties = board.blast_casualties(m);

        let blacks = Bitboard::square_bb(Square::D5) | Bitboard::square_bb(Square::E7);
        assert_eq!(casualties.white, Bitboard::square_bb(Square::D6));
        assert_eq!(casualties.black, blacks);
        assert_eq!(casualties.en_passant_victim, Some(Square::D5));
        assert_eq!(casualties.all(), blacks | Bitboard::square_bb(Square::D6));
    }

    #[test]
    fn blast_casualties_quiet_castle_promotion() {
        let start = Board::new();
        assert!(
            start
                .blast_casualties(Move::make_move(Square::E2, Square::E4))
                .is_empty()
        );

        let castling = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        assert!(
            castling
                .blast_casualties(Move::make_castling(Square::E1, Square::H1))
                .is_empty()
        );

        let promotion = Board::from_fen("k7/4P3/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(
            promotion
                .blast_casualties(Move::make_promotion(
                    Square::E7,
                    Square::E8,
                    PieceType::Queen
                ))
                .is_empty()
        );

        // A capture promotion blasts: b7xa8=Q removes the knight on a8 and
        // the promoted queen at ground zero, while the white pawn on a7 is
        // immune in the ring.
        let capture_promotion = Board::from_fen("n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1").unwrap();
        let casualties = capture_promotion.blast_casualties(Move::make_promotion(
            Square::B7,
            Square::A8,
            PieceType::Queen,
        ));
        assert_eq!(casualties.white, Bitboard::square_bb(Square::A8));
        assert_eq!(casualties.black, Bitboard::square_bb(Square::A8));
        assert_eq!(casualties.en_passant_victim, None);
        assert!(!casualties.is_empty());
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
            let trivial = TrivialLegality::new(board.position_view(), &state);

            let mut moves = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut moves);

            for &m in moves.as_slice() {
                assert_eq!(
                    trivial.is_trivially_legal(m),
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
            let trivial = TrivialLegality::new(board.position_view(), &state);

            let mut moves = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut moves);

            for &m in moves.as_slice() {
                if trivial.is_trivially_legal(m) {
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
            let trivial = TrivialLegality::new(board.position_view(), &state);
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
                if trivial.is_trivially_legal(m) {
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

    // --- widened fast-legality ladder (MG4) ---

    /// Positions that exercise every branch of the widened ladder in addition
    /// to the trivial-legality corpus: captures that open a slider ray, a
    /// capture that removes an enemy blocker of an enemy slider, en passant
    /// that empties two blockers on one ray, exploding the enemy commoner,
    /// self-explosion, pins, and a bare-commoner endgame.
    const WIDENED_LEGALITY_CORPUS: &[&str] = &[
        // Enemy blocker of an enemy slider: Rb4 removes the a4 knight, which
        // alone blocks the a8 rook's ray to the a1 commoner. Illegal.
        "r6k/8/8/8/nb5R/8/8/K7 w - - 0 1",
        // En passant empties *two* blockers (d4 victim, e4 capturer) on the
        // h4 queen's rank-4 ray to the a4 commoner. Illegal.
        "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        // Exploding the enemy commoner wins regardless of own exposure.
        "k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1",
        // Self-exploding commoner capture is illegal.
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        // Single-check position: every move needs the oracle.
        "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
        // Pinned knight: its quiet moves all fall back.
        "4k3/8/8/8/8/4r3/4N3/4K3 w - - 0 1",
        // Bare commoners: every quiet commoner move is decided by the map.
        "k7/8/8/8/8/8/8/7K w - - 0 1",
        // Castling: always the oracle.
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    ];

    /// Deterministic play-out starts for the widened-ladder differential.
    const WIDENED_PLAYOUT_FENS: &[&str] = &[
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
        "r6k/8/8/8/nb5R/8/8/K7 w - - 0 1",
        "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    ];

    /// Reference implementation of the *node-constant* data of the widened
    /// ladder: the aligned enemy sliders of the commoner on `ksq` and the
    /// squares that block their rays.
    ///
    /// Written by scanning the board, deliberately unlike the bitboard-driven
    /// [`compute_sniper_data`], so a mistake in either finds the other.
    fn sniper_data_reference(board: &Board, ksq: Square) -> (Bitboard, Bitboard) {
        let them = board.side_to_move().flip();
        let mut snipers = Bitboard::EMPTY;
        let mut blockers = Bitboard::EMPTY;
        for idx in 0..64u8 {
            let sq = Square::from_u8(idx);
            let piece = board.piece_on(sq);
            if piece.color() != Some(them) {
                continue;
            }
            let sq_bb = Bitboard::square_bb(sq);
            let on_rook_line = !(attacks::rook_attacks(ksq, Bitboard::EMPTY) & sq_bb).is_empty();
            let on_bishop_line =
                !(attacks::bishop_attacks(ksq, Bitboard::EMPTY) & sq_bb).is_empty();
            let aligned = match piece.type_of() {
                Some(PieceType::Rook) => on_rook_line,
                Some(PieceType::Bishop) => on_bishop_line,
                Some(PieceType::Queen) => on_rook_line || on_bishop_line,
                _ => false,
            };
            if aligned {
                snipers = snipers | Bitboard::square_bb(sq);
                blockers = blockers | (between_bb(ksq, sq) & board.occupied());
            }
        }
        (snipers, blockers)
    }

    /// Verbatim reference of the intended widened rules, written against the
    /// public board API only (`blast_casualties` instead of a hand-built blast
    /// set, `attackers_to_color` instead of `is_attacked_by`).
    ///
    /// Mirrors [`WidenedLegality::decide`] decision for decision, so a future
    /// edit cannot silently change which moves the ladder accepts.
    fn widened_legality_reference(board: &Board, m: Move) -> Option<bool> {
        let us = board.side_to_move();
        let them = us.flip();
        let our_commoners = board.commoners(us);
        let their_commoners = board.commoners(them);
        if our_commoners.is_empty() || their_commoners.is_empty() {
            return None;
        }
        let ksq = our_commoners.lsb();
        let move_type = m.move_type();
        if move_type == MoveType::Castling {
            return None;
        }
        let from = m.from_sq();
        let to = m.to_sq();
        let is_capture = board.is_capture(m);

        if board.piece_on(from) == make_piece(us, PieceType::Commoner) {
            if our_commoners.count() != 1 {
                return None;
            }
            if is_capture {
                return Some(false);
            }
            if !(their_commoners & attacks::king_attacks(to)).is_empty() {
                return Some(true);
            }
            // The origin empties; the arrival cannot block an attack on itself.
            let occupied_after =
                (board.occupied() & !Bitboard::square_bb(from)) | Bitboard::square_bb(to);
            let attackers = board.attackers_to_color(to, them, occupied_after) & !their_commoners;
            return Some(attackers.is_empty());
        }

        // Every square the move empties: the origin plus every blast casualty.
        let emptied = Bitboard::square_bb(from) | board.blast_casualties(m).all();

        let lost = our_commoners & emptied;
        if !lost.is_empty() {
            return if lost.count() == our_commoners.count() {
                Some(false)
            } else {
                None
            };
        }
        if is_capture && (their_commoners & !emptied).is_empty() {
            return Some(true);
        }

        let (snipers, sensitive) = sniper_data_reference(board, ksq);
        if (emptied & sensitive).is_empty() {
            return Some(true);
        }
        // Some blocker on a sniper ray is emptied: the ray stays closed when the
        // sniper itself is removed or another blocker remains.
        let mut remaining = snipers & !emptied;
        while !remaining.is_empty() {
            let sniper = remaining.pop_lsb();
            if (between_bb(ksq, sniper) & board.occupied() & !emptied).is_empty() {
                return None;
            }
        }
        Some(true)
    }

    /// Assert that the widened ladder and its reference agree move for move and
    /// that every decision matches [`Board::legal`].
    fn assert_widened_legality(board: &Board, state: &StateInfo) {
        if state.commoners_count == 0 || state.them_commoners_count == 0 {
            // Terminal positions are filtered by the callers; `Board::legal`
            // rejects everything here.
            return;
        }
        let widened = WidenedLegality::new(board.position_view());
        let mut pseudo = MoveList::new();
        crate::movegen::generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            // `decide` is only well-defined when the side to move is not in
            // check; the reference mirrors that precondition.
            let decided = if state.checkers.is_empty() {
                widened.decide(board.position_view(), m)
            } else {
                None
            };
            let reference = if state.checkers.is_empty() {
                widened_legality_reference(board, m)
            } else {
                None
            };
            assert_eq!(
                decided,
                reference,
                "widened ladder diverged from its reference on {} at {}",
                m.to_uci(),
                board.fen()
            );
            if let Some(verdict) = decided {
                assert_eq!(
                    verdict,
                    board.legal(m, state),
                    "widened ladder decided {} as {verdict} but Board::legal disagrees at {}",
                    m.to_uci(),
                    board.fen()
                );
            }
        }

        // The ladder as used by the generator must reproduce the legal list.
        let fast = FastLegality::new(board.position_view(), state);
        let mut legal = MoveList::new();
        crate::movegen::generate_legal_with_state(board, state, &mut legal);
        let expected = |m: Move| legal.as_slice().contains(&m);
        for &m in pseudo.as_slice() {
            assert_eq!(
                fast.is_legal(board.position_view(), m, state),
                expected(m),
                "FastLegality disagrees with the legal list on {} at {}",
                m.to_uci(),
                board.fen()
            );
        }
    }

    fn check_widened_legality_playout(
        board: &mut Board,
        rng: &mut Xorshift2,
        depth: u32,
        visited: &mut u64,
    ) {
        *visited += 1;
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        assert_widened_legality(board, &state);

        let mut list = MoveList::new();
        crate::movegen::generate_legal_with_state(board, &state, &mut list);
        if depth == 0 || list.is_empty() {
            return;
        }
        let idx = (rng.next() % list.len() as u64) as usize;
        let m = list[idx];
        board.do_move(m, &mut state);
        check_widened_legality_playout(board, rng, depth - 1, visited);
        board.undo_move(m, &state);
    }

    #[test]
    fn widened_legality_matches_reference_and_board_legal() {
        for fen in TRIVIAL_LEGALITY_CORPUS
            .iter()
            .chain(WIDENED_LEGALITY_CORPUS.iter())
        {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            assert_widened_legality(&board, &state);
        }

        let mut visited = 0u64;
        for (i, fen) in WIDENED_PLAYOUT_FENS.iter().enumerate() {
            for seed in 0..4u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0x2F53_9C1B_7AE4_55D1 ^ ((i as u64) << 32) ^ seed);
                check_widened_legality_playout(&mut board, &mut rng, 120, &mut visited);
            }
        }
        assert!(visited > 1000, "only visited {visited} positions");
    }

    /// The generator's own encoding of the pseudo-legal move `from -> to`.
    fn pseudo_move(board: &Board, from: Square, to: Square) -> Move {
        let mut pseudo = MoveList::new();
        crate::movegen::generate_pseudo_legal(board, &mut pseudo);
        *pseudo
            .as_slice()
            .iter()
            .find(|m| m.from_sq() == from && m.to_sq() == to)
            .unwrap_or_else(|| panic!("no pseudo-legal move {from:?}{to:?} on {}", board.fen()))
    }

    #[test]
    fn widened_legality_blast_that_opens_a_ray_is_undecided() {
        // Rb4 blasts the a4 knight, the only piece between the a1 commoner and
        // the a8 rook: the ladder must hand this to `Board::legal`, which
        // rejects it.
        let board = Board::from_fen("r6k/8/8/8/nb5R/8/8/K7 w - - 0 1").expect("FEN must parse");
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let m = pseudo_move(&board, Square::H4, Square::B4);
        assert!(board.is_capture(m));
        assert!(
            WidenedLegality::new(board.position_view())
                .decide(board.position_view(), m)
                .is_none()
        );
        assert!(!board.legal(m, &state));

        // En passant empties two blockers on the same ray (d4 victim, e4
        // capturer), which the single-blocker rule must not accept either.
        let board = Board::from_fen("8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1").expect("FEN must parse");
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let m = pseudo_move(&board, Square::E4, Square::D3);
        assert!(
            WidenedLegality::new(board.position_view())
                .decide(board.position_view(), m)
                .is_none()
        );
        assert!(!board.legal(m, &state));
    }

    #[test]
    fn widened_legality_corpus_covers_every_rule() {
        // Guard against a vacuous equivalence test: every rule of the ladder
        // must be reached by the corpus, and every undecided path exercised.
        #[derive(Default)]
        struct Coverage {
            accepted_quiet: u64,
            accepted_capture: u64,
            accepted_explosion: u64,
            accepted_commoner: u64,
            accepted_check: u64,
            rejected_self_explosion: u64,
            rejected_commoner_capture: u64,
            undecided_pinned_origin: u64,
            undecided_castling: u64,
            undecided_en_passant: u64,
            undecided_quiet: u64,
            in_check_positions: u64,
            no_commoner_positions: u64,
        }

        let mut coverage = Coverage::default();
        for fen in TRIVIAL_LEGALITY_CORPUS
            .iter()
            .chain(WIDENED_LEGALITY_CORPUS.iter())
            .chain(WIDENED_PLAYOUT_FENS.iter())
        {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            if !state.checkers.is_empty() {
                coverage.in_check_positions += 1;
                continue;
            }
            if state.commoners_count == 0 || state.them_commoners_count == 0 {
                coverage.no_commoner_positions += 1;
                continue;
            }

            let us = board.side_to_move();
            let widened = WidenedLegality::new(board.position_view());
            let trivial = TrivialLegality::new(board.position_view(), &state);
            let mut pseudo = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                if trivial.is_trivially_legal(m) {
                    coverage.accepted_quiet += 1;
                    continue;
                }
                match widened.decide(board.position_view(), m) {
                    Some(true) => {
                        let is_capture = board.is_capture(m);
                        let emptied =
                            Bitboard::square_bb(m.from_sq()) | board.blast_casualties(m).all();
                        if board.piece_on(m.from_sq()) == make_piece(us, PieceType::Commoner) {
                            coverage.accepted_commoner += 1;
                        } else if is_capture && (board.commoners(us.flip()) & !emptied).is_empty() {
                            coverage.accepted_explosion += 1;
                        } else if is_capture {
                            coverage.accepted_capture += 1;
                        } else {
                            coverage.accepted_quiet += 1;
                        }
                        let mut child = board.clone();
                        let mut child_state = StateInfo::new();
                        child.do_move(m, &mut child_state);
                        if !child.checkers().is_empty() {
                            coverage.accepted_check += 1;
                        }
                    }
                    Some(false) => {
                        if board.is_capture(m)
                            && board.piece_on(m.from_sq()) == make_piece(us, PieceType::Commoner)
                        {
                            coverage.rejected_commoner_capture += 1;
                        } else {
                            coverage.rejected_self_explosion += 1;
                        }
                    }
                    None => match m.move_type() {
                        MoveType::Castling => coverage.undecided_castling += 1,
                        MoveType::EnPassant => coverage.undecided_en_passant += 1,
                        _ => {
                            if !(state.pinned & Bitboard::square_bb(m.from_sq())).is_empty() {
                                coverage.undecided_pinned_origin += 1;
                            } else {
                                coverage.undecided_quiet += 1;
                            }
                        }
                    },
                }
            }
        }

        assert!(coverage.accepted_quiet > 0, "no accepted quiet move");
        assert!(coverage.accepted_capture > 0, "no accepted capture");
        assert!(
            coverage.accepted_explosion > 0,
            "no accepted enemy-commoner explosion"
        );
        assert!(coverage.accepted_commoner > 0, "no accepted commoner move");
        assert!(coverage.accepted_check > 0, "no accepted checking move");
        assert!(
            coverage.rejected_self_explosion > 0,
            "no rejected self-explosion"
        );
        assert!(
            coverage.rejected_commoner_capture > 0,
            "no rejected commoner capture"
        );
        assert!(
            coverage.undecided_pinned_origin > 0,
            "no undecided pinned-origin move"
        );
        assert!(coverage.undecided_castling > 0, "no undecided castling");
        assert!(coverage.undecided_en_passant > 0, "no undecided en passant");
        assert!(coverage.undecided_quiet > 0, "no undecided quiet move");
        assert!(
            coverage.in_check_positions > 0,
            "no position with the side to move in check"
        );
        assert!(
            coverage.no_commoner_positions > 0,
            "no position without a commoner"
        );
    }

    #[test]
    fn fast_legality_handles_terminal_positions() {
        // `FastLegality` rejects everything when the game is already over, even
        // when called directly (not through the generator's early return).
        let mut checked = 0;
        for fen in ["8/8/8/8/8/8/8/k7 w - - 0 1", "k7/8/8/8/8/8/8/8 b - - 0 1"] {
            let board = Board::from_fen(fen).expect("FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let fast = FastLegality::new(board.position_view(), &state);
            let mut pseudo = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                checked += 1;
                assert!(
                    !fast.is_legal(board.position_view(), m, &state),
                    "{fen} {m}"
                );
            }
        }
        assert!(checked > 0, "no pseudo-legal move reached the ladder");
    }

    #[test]
    fn attackers_to_color_matches_intersection() {
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let occupied = board.occupied();
            for idx in 0..64u8 {
                let sq = Square::from_u8(idx);
                for color in [Color::White, Color::Black] {
                    assert_eq!(
                        board.attackers_to_color(sq, color, occupied),
                        board.attackers_to(sq, occupied) & board.pieces_color(color),
                        "attackers_to_color mismatch at {fen}, {sq:?}, {color:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn is_zeroing_matches_rule50_after() {
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut moves = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut moves);
            for &m in moves.as_slice() {
                assert_eq!(
                    board.is_zeroing(m),
                    board.rule50_after(m) == 0,
                    "is_zeroing mismatch for {} at {fen}",
                    m.to_uci()
                );
            }
        }

        let start = Board::new();
        assert!(!start.is_zeroing(Move::NONE));
        assert!(!start.is_zeroing(Move::make_move(Square::E4, Square::E5)));
        assert!(!start.is_zeroing(Move::make_castling(Square::E1, Square::H1)));
    }

    #[test]
    fn is_double_push_matches_move() {
        let start = Board::new();
        assert!(start.is_double_push(Move::make_move(Square::E2, Square::E4)));
        assert!(!start.is_double_push(Move::make_move(Square::E2, Square::E3)));

        let black =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1").unwrap();
        assert!(black.is_double_push(Move::make_move(Square::E7, Square::E5)));

        // A rook sliding two ranks is not a double pawn push.
        let rook = Board::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 0 1").unwrap();
        assert!(!rook.is_double_push(Move::make_move(Square::A1, Square::A3)));

        // Promotions are never double pushes.
        let promotion = Board::from_fen("k7/4P3/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(!promotion.is_double_push(Move::make_promotion(
            Square::E7,
            Square::E8,
            PieceType::Queen
        )));

        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut moves = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut moves);
            for &m in moves.as_slice() {
                let from = m.from_sq();
                let to = m.to_sq();
                let piece = board.piece_on(from);
                let expected = m.move_type() == MoveType::Normal
                    && piece.color() == Some(board.side_to_move())
                    && piece.type_of() == Some(PieceType::Pawn)
                    && file_of(from) == file_of(to)
                    && (to as i8 - from as i8).abs() == 16
                    && board.empty(Square::from_index((from as i8 + to as i8) / 2));
                assert_eq!(
                    board.is_double_push(m),
                    expected,
                    "is_double_push mismatch for {} at {fen}",
                    m.to_uci()
                );
            }
        }
    }

    #[test]
    fn corpus_legal_move_count_below_max_moves() {
        let mut max_seen = 0;
        let mut moves = MoveList::new();
        for fen in TRIVIAL_LEGALITY_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            moves.clear();
            crate::movegen::generate_legal(&board, &mut moves);
            max_seen = max_seen.max(moves.len());
        }
        assert!(
            max_seen < MAX_MOVES,
            "corpus maximum {max_seen} reached MAX_MOVES"
        );
    }

    #[test]
    fn observed_maximum_legal_move_count() {
        // Largest count found by scanning all 41 perft positions through depth
        // 4 (14,003,267 positions). This pins the value quoted by the
        // `MAX_MOVES` documentation.
        let board = Board::from_fen("8/8/5q2/3N3p/1r5P/4q3/6Kk/8 b - - 25 63")
            .expect("largest-count FEN must parse");
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        assert_eq!(moves.len(), 68);
        assert!(moves.len() < MAX_MOVES);
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

    const NULL_FOR_FIXTURES: &[&str] = &[
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2",
        "4k3/8/8/8/8/8/8/4K3 w - - 99 50",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    ];

    fn null_for_fen_roundtrip(board: &Board, attacker: Color) -> Board {
        let mut fields: Vec<String> = board.fen().split_whitespace().map(str::to_owned).collect();
        fields[1] = match attacker {
            Color::White => "w".to_owned(),
            Color::Black => "b".to_owned(),
        };
        fields[3] = "-".to_owned();
        Board::from_fen(&fields.join(" ")).expect("round-trip FEN must parse")
    }

    #[test]
    fn null_for_matches_fen_roundtrip() {
        for fen in NULL_FOR_FIXTURES {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            for attacker in [Color::White, Color::Black] {
                let null = board.null_for(attacker);
                let expected = null_for_fen_roundtrip(&board, attacker);
                assert_eq!(null.fen(), expected.fen(), "{fen} / {attacker:?}");
                assert_eq!(null.hash(), expected.hash(), "{fen} / {attacker:?}");
                assert_eq!(
                    null.hash_with(EnPassantMode::Legal),
                    expected.hash_with(EnPassantMode::Legal),
                    "{fen} / {attacker:?}"
                );
                assert_eq!(null.side_to_move(), attacker);
                assert_eq!(null.ep_square(), None);
                assert_eq!(null.rule50(), board.rule50());
                assert_eq!(null.game_ply(), expected.game_ply());
                assert_eq!(null.castling_rights(), board.castling_rights());
                // The source board is untouched.
                assert_eq!(board.fen(), *fen);
            }
        }
    }

    #[test]
    fn null_for_preserves_fullmove_and_rule50() {
        let board = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 99 50").unwrap();
        let null = board.null_for(Color::Black);
        assert_eq!(null.rule50(), 99);
        assert_eq!(null.fullmove_number(), 50);
        assert_eq!(null.game_ply(), 100);
        assert_eq!(null.side_to_move(), Color::Black);
    }

    #[test]
    fn null_for_clears_ep_and_recomputes_hash() {
        let board = Board::from_fen("4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2").unwrap();
        assert!(board.ep_square().is_some());
        let null = board.null_for(Color::Black);
        assert_eq!(null.ep_square(), None);
        assert_eq!(null.hash(), null.recomputed_hash());
        assert_ne!(null.hash(), board.hash());
    }

    #[test]
    fn null_for_advanced_updates_hash_and_clocks_incrementally() {
        // The high-fullmove and saturated-clock fixtures pin the paths that the
        // public getters cannot represent exactly.
        let extra = [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 65535",
            "4k3/8/8/8/8/8/8/4K3 b - - 0 65535",
            "4k3/8/8/8/8/8/8/4K3 w - - 65535 50",
        ];
        for fen in NULL_FOR_FIXTURES.iter().copied().chain(extra) {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            for attacker in [Color::White, Color::Black] {
                let advanced = board.null_for_advanced(attacker);

                assert_eq!(advanced.squares, board.squares, "{fen} / {attacker:?}");
                assert_eq!(
                    advanced.position.castling_rights, board.position.castling_rights,
                    "{fen} / {attacker:?}"
                );
                assert_eq!(advanced.position.side_to_move, attacker);
                assert_eq!(advanced.position.ep_square, None);
                assert_eq!(
                    advanced.position.rule50,
                    board.position.rule50.saturating_add(1),
                    "{fen} / {attacker:?}"
                );
                assert_eq!(
                    advanced.game_ply,
                    board.game_ply.saturating_add(1),
                    "{fen} / {attacker:?}"
                );
                // The incremental side/ep delta must equal a full recompute.
                assert_eq!(
                    advanced.hash,
                    advanced.recomputed_hash(),
                    "{fen} / {attacker:?}"
                );

                // The source board is untouched.
                assert_eq!(board.fen(), fen, "{fen} / {attacker:?}");
            }
        }
    }

    #[test]
    fn has_castling_right_matches_raw_bits() {
        for fen in NULL_FOR_FIXTURES {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let raw = board.castling_rights();
            for (color, side, bit) in [
                (Color::White, CastlingSide::KingSide, WK_CASTLE),
                (Color::White, CastlingSide::QueenSide, WQ_CASTLE),
                (Color::Black, CastlingSide::KingSide, BK_CASTLE),
                (Color::Black, CastlingSide::QueenSide, BQ_CASTLE),
            ] {
                assert_eq!(
                    board.has_castling_right(color, side),
                    raw & bit != 0,
                    "{fen} / {color:?} / {side:?}"
                );
            }
        }
    }

    // --- SAN: formatting, parsing, and the round-trip contract ---------------

    /// A deterministic xorshift64 PRNG for the SAN round-trip play-outs.
    struct SanRng(u64);

    impl SanRng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    /// Resolve a UCI string on `board`, panicking with the FEN on failure.
    fn uci(board: &Board, text: &str) -> Move {
        board
            .move_from_uci(text)
            .unwrap_or_else(|| panic!("{}: {text} must be legal", board.fen()))
    }

    /// Assert that `m` formats to exactly `expected` SAN.
    fn assert_san(fen: &str, m: &str, expected: &str) {
        let board = Board::from_fen(fen).expect("fixture FEN must parse");
        let mv = uci(&board, m);
        assert_eq!(board.san(mv).as_deref(), Some(expected), "{fen} / {m}");
        assert_eq!(
            board.move_from_san(expected),
            Some(mv),
            "{fen} / {expected}: parse"
        );
    }

    /// Assert the SAN of `m` with any trailing `+`/`#` removed.
    fn assert_san_body(fen: &str, m: &str, expected: &str) {
        let board = Board::from_fen(fen).expect("fixture FEN must parse");
        let mv = uci(&board, m);
        let san = board
            .san(mv)
            .unwrap_or_else(|| panic!("{fen} / {m}: no SAN"));
        let body = san.trim_end_matches(['+', '#']);
        assert_eq!(body, expected, "{fen} / {m}");
        assert_eq!(
            board.move_from_san(expected),
            Some(mv),
            "{fen} / {expected}"
        );
    }

    #[test]
    fn san_start_position() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        assert_san(fen, "e2e4", "e4");
        assert_san(fen, "e2e3", "e3");
        assert_san(fen, "g1f3", "Nf3");
        assert_san(fen, "b1c3", "Nc3");
        assert_san(fen, "b1a3", "Na3");
        assert_san(fen, "d2d4", "d4");
        let board = Board::from_fen(fen).unwrap();
        // No SAN text for a non-move or for the wrong side to move.
        assert_eq!(board.san(Move::NONE), None);
        assert_eq!(board.san(Move::from_uci("e7e5").unwrap()), None);
        assert_eq!(board.move_from_san("e5"), None);
    }

    #[test]
    fn san_disambiguation() {
        // Knights reaching d2 from b1 and f3: the origin file decides.
        let fen = "4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1";
        assert_san_body(fen, "b1d2", "Nbd2");
        assert_san_body(fen, "f3d2", "Nfd2");
        // A lone knight needs no disambiguation.
        let fen = "4k3/8/8/8/8/8/8/1N2K3 w - - 0 1";
        assert_san_body(fen, "b1d2", "Nd2");

        // Rooks sharing the a-file: the origin rank decides.
        let fen = "4k3/8/8/8/8/R7/8/R3K3 w - - 0 1";
        assert_san_body(fen, "a1a2", "R1a2");
        assert_san_body(fen, "a3a2", "R3a2");
        // Rooks on rank 1 with distinct destinations: nothing to disambiguate.
        let fen = "4k3/8/8/8/8/8/8/R3K2R w - - 0 1";
        assert_san_body(fen, "a1d1", "Rd1");
        assert_san_body(fen, "h1h5", "Rh5");

        // Queens on the a-file and on the fourth rank: one rival shares the
        // file, another the rank, so both coordinates are written.
        let fen = "4k3/8/8/8/Q6Q/8/Q7/4K3 w - - 0 1";
        assert_san_body(fen, "a4c4", "Qa4c4");
        assert_san_body(fen, "a2c2", "Q2c2");
        assert_san_body(fen, "h4c4", "Qhc4");

        // Only one of the two knights can reach d4: `e2` is pinned to the
        // commoner by the rook on e8, so `f3` needs no disambiguation.
        let fen = "4r1k1/8/8/8/8/5N2/4N3/4K3 w - - 0 1";
        assert_san_body(fen, "f3d4", "Nd4");
        let board = Board::from_fen(fen).unwrap();
        assert!(!board.pinned(Color::White).is_empty());
        // ... the pinned knight's text is rejected, and `Nd4` still resolves
        // (unambiguously, to the one legal knight move) instead of failing.
        assert_eq!(board.move_from_san("Nc3"), None);
        assert_eq!(board.move_from_san("Nd4"), Some(uci(&board, "f3d4")));
    }

    #[test]
    fn san_castling() {
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        assert_san(fen, "e1g1", "O-O");
        assert_san(fen, "e1c1", "O-O-O");
        let board = Board::from_fen(fen).unwrap();
        // The zero variants are accepted on input and never emitted.
        assert_eq!(board.move_from_san("0-0"), board.move_from_uci("e1g1"));
        assert_eq!(board.move_from_san("0-0-0"), board.move_from_uci("e1c1"));
        assert_eq!(board.move_from_san("O-O+"), board.move_from_uci("e1g1"));

        let fen = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1";
        assert_san(fen, "e8g8", "O-O");
        assert_san(fen, "e8c8", "O-O-O");
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.move_from_san("0-0"), board.move_from_uci("e8g8"));
        assert_eq!(board.move_from_san("0-0-0"), board.move_from_uci("e8c8"));

        // Without the right, castling text resolves to nothing.
        let board = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").unwrap();
        assert_eq!(board.move_from_san("O-O"), None);
        assert_eq!(board.move_from_san("0-0-0"), None);
        // A rook move that shares the castling square is still resolved.
        let board = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").unwrap();
        assert_eq!(board.move_from_san("Rh1g1"), board.move_from_uci("h1g1"));
    }

    #[test]
    fn san_pawn_capture_en_passant() {
        // Normal pawn capture.
        assert_san("4k3/8/8/2p5/3P4/8/8/4K3 w - - 0 1", "d4c5", "dxc5");
        let fen = "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.move_from_san("exd6"), board.move_from_uci("e5d6"));
        // The capture marker is optional on input for pawn captures too.
        assert_eq!(board.move_from_san("ed6"), board.move_from_uci("e5d6"));

        // En passant, both colours, including a checking one.
        let fen = "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3";
        assert_san(fen, "e5d6", "exd6");
        let fen = "rnbqkbnr/pppp1ppp/8/8/3Pp3/8/PPP1PPPP/RNBQKBNR b KQkq d3 0 3";
        assert_san(fen, "e4d3", "exd3");
    }

    #[test]
    fn san_promotion() {
        let fen = "8/4P3/8/8/8/8/k7/4K3 w - - 0 1";
        assert_san(fen, "e7e8q", "e8=Q");
        assert_san(fen, "e7e8r", "e8=R");
        assert_san(fen, "e7e8b", "e8=B");
        assert_san(fen, "e7e8n", "e8=N");
        let board = Board::from_fen(fen).unwrap();
        // Suffix-less and lower-case input forms.
        assert_eq!(board.move_from_san("e8Q"), board.move_from_uci("e7e8q"));
        assert_eq!(board.move_from_san("e8=q"), board.move_from_uci("e7e8q"));
        // Promotion to the commoner is not an atomic move.
        assert_eq!(board.move_from_san("e8=K"), None);
        assert_eq!(board.move_from_san("e8K"), None);

        // Capture promotions keep the origin file.
        let fen = "3r4/4P3/8/8/8/8/k7/4K3 w - - 0 1";
        assert_san(fen, "e7d8q", "exd8=Q");
        assert_san(fen, "e7d8n", "exd8=N");
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.move_from_san("exd8R"), board.move_from_uci("e7d8r"));
    }

    #[test]
    fn san_check_and_mate_suffix() {
        // A quiet check that is not mate.
        assert_san("4k3/8/8/8/8/8/8/R3K3 w - - 0 1", "a1a8", "Ra8+");
        // A mate: the opponent has no legal move while attacked. It also needs
        // disambiguation (two rooks reach g1), so the suffix follows the move.
        assert_san("7k/8/8/5N2/3K4/8/8/R6R w - - 0 1", "a1g1", "Rag1#");
        // An explosion of the enemy commoner is `#` even though the child is
        // not in check: the blast on f7 also removes the commoner on g8.
        assert_san("6k1/5n2/8/8/8/8/8/K4R2 w - - 0 1", "f1f7", "Rxf7#");
        // No suffix for a quiet move into a drawn, unprotected position.
        assert_san("4k3/8/8/8/8/8/8/4K2R w - - 0 1", "h1h2", "Rh2");
    }

    #[test]
    fn san_rejects_ambiguous_and_illegal() {
        let fen = "4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        // `Nd2` is ambiguous between b1 and f3.
        assert_eq!(board.move_from_san("Nd2"), None);
        // Wrong side, blocked, impossible, malformed, trailing junk.
        assert_eq!(board.move_from_san("e5"), None);
        assert_eq!(board.move_from_san("Nf7"), None);
        assert_eq!(board.move_from_san("Nz3"), None);
        assert_eq!(board.move_from_san(""), None);
        assert_eq!(board.move_from_san("e4!"), None);
        assert_eq!(board.move_from_san("e4 e5"), None);
        assert_eq!(board.move_from_san("§"), None);
        assert_eq!(board.move_from_san("#"), None);
        assert_eq!(board.move_from_san("Ke3"), None);
        // Two queens can reach c3: even the rank-suffixed text is not enough.
        let board = Board::from_fen("4k3/8/8/8/Q6Q/8/Q7/4K3 w - - 0 1").unwrap();
        assert_eq!(board.move_from_san("Qc3"), None);
        // A pinned piece cannot move, so its text is rejected.
        let board = Board::from_fen("4r1k1/8/8/8/8/8/4N3/4K3 w - - 0 1").unwrap();
        assert_eq!(board.move_from_san("Nc3"), None);
    }

    #[test]
    fn write_san_matches_san() {
        const FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
            "4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1",
        ];
        for fen in FENS {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let mut legal = MoveList::new();
            crate::movegen::generate_legal(&board, &mut legal);
            assert!(!legal.is_empty(), "{fen}");
            for &m in legal.as_slice() {
                let mut written = String::new();
                board.write_san(m, &mut written).expect("legal move writes");
                assert_eq!(Some(written.clone()), board.san(m), "{fen} / {m}");
                assert_eq!(board.move_from_san(&written), Some(m), "{fen} / {m}");
            }
            // An illegal move is an error, not a silent empty write.
            let mut written = String::new();
            assert!(board.write_san(Move::NONE, &mut written).is_err());
            assert!(written.is_empty());
        }
    }

    #[test]
    fn san_roundtrips_every_legal_move() {
        const FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "r4b1r/p1N1k1p1/2pNp2p/5p2/1n1P1P2/6PP/PP6/R1B1K3 b Q - 4 17",
            "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15",
            "7k/pp6/8/1PP5/6pp/7P/P3b3/4R1K1 b - - 0 28",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
        ];
        let mut checked = 0u64;
        let mut with_suffix = 0u64;
        let mut with_disambiguation = 0u64;
        for fen in FENS {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let mut legal = MoveList::new();
            crate::movegen::generate_legal(&board, &mut legal);
            for &m in legal.as_slice() {
                let san = board
                    .san(m)
                    .unwrap_or_else(|| panic!("{fen} / {m}: no SAN"));
                assert_eq!(board.move_from_san(&san), Some(m), "{fen} / {san}");
                checked += 1;
                if san.ends_with('+') || san.ends_with('#') {
                    with_suffix += 1;
                }
                if san.len() > 4
                    && !san.starts_with("O-O")
                    && !san.contains('=')
                    && san.starts_with(['N', 'B', 'R', 'Q', 'K'])
                {
                    with_disambiguation += 1;
                }
            }
        }
        assert!(checked > 150, "expected a real corpus, got {checked}");
        // Non-vacuity: the corpus exercises both the suffix and the
        // disambiguation paths.
        assert!(with_suffix > 0, "no checking or mating SAN");
        assert!(with_disambiguation > 0, "no disambiguated SAN");

        // Seeded play-outs: the round-trip must survive moves that are only
        // reachable after other moves.
        let mut played = 0u64;
        for seed in 0..4u64 {
            let mut board = Board::new();
            let mut state = StateInfo::new();
            let mut rng = SanRng(0x5DEE_CE66_D1CE_4E5B ^ seed);
            for _ in 0..120 {
                board.populate_state(&mut state);
                let mut legal = MoveList::new();
                crate::movegen::generate_legal_with_state(&board, &state, &mut legal);
                if legal.is_empty() {
                    break;
                }
                for &m in legal.as_slice() {
                    let san = board.san(m).expect("legal move has SAN");
                    assert_eq!(
                        board.move_from_san(&san),
                        Some(m),
                        "{} / {san} / {m}",
                        board.fen()
                    );
                }
                let pick = (rng.next() % legal.len() as u64) as usize;
                board.do_move(legal.as_slice()[pick], &mut state);
                played += 1;
            }
        }
        assert!(played > 100, "expected a real play-out, got {played}");
    }

    #[test]
    fn san_promotion_with_mate_suffix() {
        // The capture promotion on c8 blasts the commoner on d7 away, so it is
        // a mate and gets the `#` suffix after the promotion piece.
        let fen = "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1";
        assert_san(fen, "b7c8q", "bxc8=Q#");
        assert_san(fen, "b7c8n", "bxc8=N#");
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.move_from_san("bxc8=Q#"), board.move_from_uci("b7c8q"));
        assert_eq!(board.move_from_san("bxc8=N"), board.move_from_uci("b7c8n"));
    }

    #[test]
    fn san_keeps_commoner_letter() {
        // The commoner is `K` in SAN as in FEN.
        assert_san("4k3/8/8/8/8/8/8/4K3 w - - 0 1", "e1e2", "Ke2");
        assert_san("4k3/8/8/8/8/8/8/4K3 b - - 0 1", "e8e7", "Ke7");
    }

    #[test]
    fn clock_boundaries_saturate_and_undo_exactly() {
        // fullmove 32768 makes the old u16 game-ply boundary (65535) reachable
        // on the next move. The internal counter is wider, so do/undo is exact;
        // the public getter saturates.
        let fen = "4k3/8/8/8/8/8/8/4K2R w - - 65535 32768";
        let mut board = Board::from_fen(fen).unwrap();
        assert_eq!(board.position.rule50, u16::MAX);
        assert_eq!(board.game_ply, 65535);
        assert_eq!(board.game_ply(), u16::MAX);
        assert_eq!(board.fullmove_number(), 32768);

        let before_fen = board.fen();
        let before_hash = board.hash();
        let m = Move::make_move(Square::H1, Square::H2);
        assert_eq!(board.rule50_after(m), u16::MAX);
        assert_eq!(
            board.hash_and_rule50_after(m),
            (board.hash_after(m), u16::MAX)
        );

        let mut state = StateInfo::new();
        board.do_move(m, &mut state);
        assert_eq!(board.rule50(), u16::MAX);
        assert_eq!(board.game_ply, 65536);
        assert_eq!(board.game_ply(), u16::MAX);
        assert_eq!(board.fullmove_number(), 32768);

        board.undo_move(m, &state);
        assert_eq!(board.rule50(), u16::MAX);
        assert_eq!(board.game_ply, 65535);
        assert_eq!(board.game_ply(), u16::MAX);
        assert_eq!(board.fullmove_number(), 32768);
        assert_eq!(board.hash(), before_hash);
        assert_eq!(board.fen(), before_fen);

        // The largest representable fullmove number parses and does not
        // overflow, while a field that does not fit u16 is still an error.
        let max = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 65535").unwrap();
        assert_eq!(max.game_ply, 131_069);
        assert_eq!(max.game_ply(), u16::MAX);
        assert_eq!(max.fullmove_number(), 65535);
        let max_black = Board::from_fen("4k3/8/8/8/8/8/8/4K3 b - - 0 65535").unwrap();
        assert_eq!(max_black.game_ply, 131_070);
        assert_eq!(max_black.fullmove_number(), 65535);
        assert!(matches!(
            Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 65536"),
            Err(FenError::ParseInt(_))
        ));
    }

    #[test]
    fn long_do_undo_crosses_the_old_clock_boundary_exactly() {
        let mut board = Board::from_fen("4k3/8/8/8/8/8/8/4K2R w - - 0 32768").unwrap();
        let start_fen = board.fen();
        let start_hash = board.hash();
        let start_ply = board.game_ply;
        let mut state_out = StateInfo::new();
        let mut state_back = StateInfo::new();
        let out = Move::make_move(Square::H1, Square::H2);
        let back = Move::make_move(Square::H2, Square::H1);

        for _ in 0..8 {
            board.do_move(out, &mut state_out);
            board.do_move(back, &mut state_back);
            board.undo_move(back, &state_back);
            board.undo_move(out, &state_out);
            assert_eq!(board.game_ply, start_ply);
            assert_eq!(board.hash(), start_hash);
            assert_eq!(board.fen(), start_fen);
        }
    }

    fn castling_token(rights: u8) -> String {
        if rights == 0 {
            return "-".to_owned();
        }
        let mut token = String::new();
        for (bit, ch) in [
            (WK_CASTLE, 'K'),
            (WQ_CASTLE, 'Q'),
            (BK_CASTLE, 'k'),
            (BQ_CASTLE, 'q'),
        ] {
            if rights & bit != 0 {
                token.push(ch);
            }
        }
        token
    }

    #[test]
    fn structural_castling_predicate_agrees_across_constructors() {
        // Each placement exercises a different subset of the backing pieces.
        const PLACEMENTS: &[&str] = &[
            "4k2r/8/8/8/8/8/8/R3K2R", // all four rights backed
            "4k3/8/8/8/8/8/8/4K3",    // no rooks
            "4k2r/8/8/8/8/8/8/4K2R",  // king-side rooks only
            "4k3/8/8/8/8/8/8/R3K3",   // queen-side rooks only
            "3k4/8/8/8/8/8/8/3K4",    // commoners off their home squares
        ];

        for placement in PLACEMENTS {
            let base = Board::from_fen(&format!("{placement} w - - 0 1")).unwrap();
            let mut setup = crate::setup::Setup::new();
            for (sq, piece) in base.pieces() {
                setup.place(sq, piece);
            }
            setup.set_side_to_move(Color::White);

            for rights in 0u8..16 {
                let expected = castling_rights_backed_by_pieces(rights, &base.squares);
                setup.set_castling_rights(rights);
                assert_eq!(
                    Board::from_setup(&setup).is_ok(),
                    expected,
                    "{placement}: from_setup rights {rights:#x}"
                );

                let fen = format!("{placement} w {} - 0 1", castling_token(rights));
                let parsed = Board::from_fen(&fen);
                assert_eq!(
                    parsed.is_ok(),
                    expected,
                    "{placement}: from_fen rights {rights:#x} ({fen})"
                );
                if !expected {
                    assert!(matches!(parsed, Err(FenError::InvalidCastling(_))));
                }

                let built = Board::from_setup_parts(base.squares, Color::White, rights, None, 0, 1);
                assert_eq!(
                    built
                        .validation_errors(FenPolicy::Strict)
                        .contains(ValidationErrors::INVALID_CASTLING_RIGHTS),
                    !expected,
                    "{placement}: validate rights {rights:#x}"
                );
            }
        }
    }

    #[test]
    fn structural_ep_predicates_preserve_each_caller_contract() {
        struct Case {
            name: &'static str,
            placement: &'static str,
            ep: Square,
            parser_ok: bool,
            setup_ok: bool,
            strict_ok: bool,
        }

        let cases = [
            Case {
                name: "valid",
                placement: "4k3/8/8/3p4/8/8/8/4K3",
                ep: Square::D6,
                parser_ok: true,
                setup_ok: true,
                strict_ok: true,
            },
            Case {
                name: "missing victim",
                placement: "4k3/8/8/8/8/8/8/4K3",
                ep: Square::D6,
                parser_ok: false,
                setup_ok: false,
                strict_ok: false,
            },
            Case {
                name: "occupied origin",
                placement: "4k3/3R4/8/3p4/8/8/8/4K3",
                ep: Square::D6,
                parser_ok: true,
                setup_ok: true,
                strict_ok: false,
            },
            Case {
                name: "occupied target",
                placement: "4k3/8/3R4/3p4/8/8/8/4K3",
                ep: Square::D6,
                parser_ok: true,
                setup_ok: true,
                strict_ok: false,
            },
            Case {
                name: "wrong rank",
                placement: "4k3/8/8/8/3p4/8/8/4K3",
                ep: Square::D4,
                parser_ok: false,
                setup_ok: false,
                strict_ok: false,
            },
        ];

        for case in cases {
            let fen = format!("{} w - {} 0 1", case.placement, sq_str(case.ep).unwrap());
            let parsed = Board::from_fen(&fen);
            assert_eq!(parsed.is_ok(), case.parser_ok, "{}: from_fen", case.name);
            if !case.parser_ok {
                assert!(matches!(parsed, Err(FenError::InvalidEpSquare(_))));
            }

            let base = Board::from_fen(&format!("{} w - - 0 1", case.placement)).unwrap();
            let mut setup = crate::setup::Setup::new();
            for (sq, piece) in base.pieces() {
                setup.place(sq, piece);
            }
            setup
                .set_side_to_move(Color::White)
                .set_ep_square(Some(case.ep));
            assert_eq!(
                Board::from_setup(&setup).is_ok(),
                case.setup_ok,
                "{}: from_setup",
                case.name
            );

            assert_eq!(
                Board::from_fen_with(&fen, FenPolicy::Strict).is_ok(),
                case.strict_ok,
                "{}: Strict",
                case.name
            );

            let built = Board::from_setup_parts(base.squares, Color::White, 0, Some(case.ep), 0, 1);
            assert_eq!(
                built
                    .validation_errors(FenPolicy::Strict)
                    .contains(ValidationErrors::INVALID_EP_SQUARE),
                !case.strict_ok,
                "{}: validate",
                case.name
            );
        }
    }

    #[test]
    fn blast_zone_none_is_total() {
        assert_eq!(Board::blast_zone(Square::NONE), Bitboard::EMPTY);
        assert_eq!(
            Board::blast_zone(Square::E4),
            attacks::king_attacks(Square::E4) | Bitboard::square_bb(Square::E4)
        );
    }

    /// Read the FEN column of the perft table for the counts tests.
    fn perft_corpus_fens() -> Vec<String> {
        let content = include_str!("../tests/perft_values.md");
        let mut fens = Vec::new();
        for line in content.lines() {
            if !line.starts_with('|') {
                continue;
            }
            let Some(start) = line.find('`') else {
                continue;
            };
            let Some(len) = line[start + 1..].find('`') else {
                continue;
            };
            fens.push(line[start + 1..start + 1 + len].to_owned());
        }
        assert_eq!(fens.len(), 41, "expected 41 perft corpus positions");
        fens
    }

    #[test]
    fn counts_constructor_getters() {
        let counts = Counts::new(1, 0);
        assert_eq!(counts.commoners(), 1);
        assert_eq!(counts.them_commoners(), 0);
        assert_eq!(counts, Counts::new(1, 0));
        assert_ne!(counts, Counts::new(0, 1));

        let mut state = StateInfo::new();
        state.commoners_count = 1;
        state.them_commoners_count = 1;
        assert_eq!(Counts::from(&state), Counts::new(1, 1));
    }

    #[test]
    fn counts_match_full_populate() {
        const FIXTURES: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/8/8/8/8/8/8/k7 w - - 0 1",
            "k7/8/8/8/8/8/8/7K w - - 0 1",
            "8/8/8/8/8/8/8/8 w - - 0 1",
        ];

        let corpus = perft_corpus_fens();
        let fens: Vec<&str> = corpus
            .iter()
            .map(String::as_str)
            .chain(FIXTURES.iter().copied())
            .collect();
        let mut checked = 0usize;
        let mut zero_count = 0usize;
        for fen in fens {
            let board = Board::from_fen(fen).expect("counts FEN must parse");
            let mut full = StateInfo::new();
            board.populate_state(&mut full);
            let counts = board.counts();
            assert_eq!(counts, Counts::from(&full), "{fen}");
            assert_eq!(counts.commoners(), full.commoners_count, "{fen}");
            assert_eq!(counts.them_commoners(), full.them_commoners_count, "{fen}");
            if full.commoners_count == 0 || full.them_commoners_count == 0 {
                zero_count += 1;
            }
            checked += 1;
        }
        assert!(checked > 45, "expected the corpus, got {checked}");
        assert!(zero_count > 0, "corpus should include terminal boards");

        // A seeded walk keeps the same relation while pieces disappear.
        const START_FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        ];
        let mut visited = 0usize;
        for (i, fen) in START_FENS.iter().enumerate() {
            for seed in 0..4u64 {
                let mut board = Board::from_fen(fen).expect("start FEN must parse");
                let mut rng = Xorshift2::new(0xC2B2_AE3D_27D4_EB4F ^ ((i as u64) << 32) ^ seed);
                let mut state = StateInfo::new();
                for _ in 0..80 {
                    board.populate_state(&mut state);
                    assert_eq!(board.counts(), Counts::from(&state), "{fen} seed {seed}");
                    visited += 1;
                    let mut list = MoveList::new();
                    crate::movegen::generate_legal_with_state(&board, &state, &mut list);
                    if list.is_empty() {
                        break;
                    }
                    let idx = (rng.next() % list.len() as u64) as usize;
                    board.do_move(list.as_slice()[idx], &mut state);
                }
            }
        }
        assert!(visited > 300, "expected a real walk, got {visited}");
    }

    #[test]
    fn legality_context_matches_legal_on_corpus() {
        let corpus = perft_corpus_fens();
        let mut checked = 0usize;
        for fen in &corpus {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let counts = board.counts();
            let ctx = board.legality_context(counts);
            assert_eq!(ctx.occupied(), board.occupied(), "{fen}");
            let mut pseudo = MoveList::new();
            crate::movegen::generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                assert_eq!(ctx.legal(m), board.legal(m, &state), "{fen} {m}");
                checked += 1;
            }
        }
        assert!(checked > 500, "expected real candidates, got {checked}");
    }

    #[test]
    fn populate_counts_preserves_other_fields() {
        let board = Board::from_fen("4k3/4r3/8/8/8/8/4R3/4K3 w - - 7 23").expect("FEN");
        let mut state = StateInfo::new();
        state.checkers = Bitboard::square_bb(Square::A1);
        state.pinned = Bitboard::square_bb(Square::H8);
        state.captured_count = 9;
        state.cap_sq = Some(Square::D4);
        state.hash = 0xDEAD_BEEF;

        board.populate_counts(&mut state);
        assert_eq!(state.checkers, Bitboard::square_bb(Square::A1));
        assert_eq!(state.pinned, Bitboard::square_bb(Square::H8));
        assert_eq!(state.captured_count, 9);
        assert_eq!(state.cap_sq, Some(Square::D4));
        assert_eq!(state.hash, 0xDEAD_BEEF);
        assert_eq!(state.commoners_count, 1);
        assert_eq!(state.them_commoners_count, 1);

        // The full writer still overwrites every cached field.
        board.populate_state(&mut state);
        assert_eq!(state.checkers, board.checkers());
        assert_eq!(state.pinned, board.pinned(Color::White));
        assert_eq!(state.captured_count, 9, "undo fields are not cleared");
        assert_eq!(state.hash, 0xDEAD_BEEF);
    }

    #[test]
    fn counts_only_state_is_not_sufficient_for_move_generation() {
        // The white rook on e2 is pinned by the black rook on e7. A counts-only
        // state has correct counts but an empty `pinned` set, so the generator's
        // fast path would accept Re2-d2. This documents the boundary.
        let board = Board::from_fen("4k3/4r3/8/8/8/8/4R3/4K3 w - - 0 1").expect("FEN");
        let mut full = StateInfo::new();
        board.populate_state(&mut full);
        let mut counts = StateInfo::new();
        board.populate_counts(&mut counts);

        let mut full_moves = MoveList::new();
        crate::movegen::generate_legal_with_state(&board, &full, &mut full_moves);
        let mut counts_moves = MoveList::new();
        crate::movegen::generate_legal_with_state(&board, &counts, &mut counts_moves);
        assert_ne!(
            full_moves.as_slice(),
            counts_moves.as_slice(),
            "a counts-only state must not be treated as a full one"
        );
        assert!(
            !full_moves
                .as_slice()
                .contains(&Move::from_uci("e2d2").unwrap()),
            "the pinned rook move is illegal"
        );

        // And a checkmate: with no checkers cached, the fast path can claim a
        // legal move where the full state has none.
        let board =
            Board::from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
                .expect("FEN");
        board.populate_state(&mut full);
        board.populate_counts(&mut counts);
        assert!(!crate::movegen::has_legal_move_with_state(&board, &full));
        assert!(
            crate::movegen::has_legal_move_with_state(&board, &counts),
            "the checkmate fixture should expose the missing checkers"
        );
    }

    #[test]
    fn unfilled_state_rejects_every_move() {
        let board = Board::new();
        let state = StateInfo::new();
        let mut list = MoveList::new();
        crate::movegen::generate_pseudo_legal(&board, &mut list);
        assert!(!list.is_empty());
        for &m in list.as_slice() {
            assert!(!board.legal(m, &state), "{m}");
            assert!(!board.is_legal_with_state(m, &state), "{m}");
        }
    }
}
