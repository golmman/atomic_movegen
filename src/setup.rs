//! Piece-set construction of a [`Board`] without a FEN string.
//!
//! [`Setup`] is a data builder: it stores a square array plus the six
//! FEN-style state fields and nothing else. [`Board::from_setup`] turns it into
//! a board (computing the bitboards and the Zobrist key once), and
//! [`Board::from_setup_with`] applies the same semantic validation policy as
//! [`Board::from_fen_with`]. There is no second board representation, no
//! duplicated move generation, and no legality logic here.
//!
//! ```
//! use atomic_movegen::{Board, Color, PieceType, Setup, Square, make_piece};
//!
//! let mut setup = Setup::new();
//! setup
//!     .place(Square::E1, make_piece(Color::White, PieceType::Commoner))
//!     .place(Square::E8, make_piece(Color::Black, PieceType::Commoner))
//!     .place(Square::H1, make_piece(Color::White, PieceType::Rook));
//!
//! let board = Board::from_setup(&setup).unwrap();
//! assert_eq!(board.fen(), "4k3/8/8/8/8/8/8/4K2R w - - 0 1");
//! ```

use crate::board::{
    BK_CASTLE, BQ_CASTLE, Board, FenError, FenLoadError, FenPolicy, WK_CASTLE, WQ_CASTLE,
};
use crate::types::*;
use std::error::Error;
use std::fmt;

/// A piece-set description used to build a [`Board`] without a FEN string.
///
/// The builder is deliberately plain data: a 64-square array plus the position
/// state (side to move, castling rights, en-passant target, half-move clock, and
/// game ply). It performs no validation while it is being filled, so a partial
/// or artificial position can be assembled in any order; the structural checks
/// run once in [`Board::from_setup`].
///
/// Use [`Board::pieces`] to read the placement of an existing board:
///
/// ```
/// use atomic_movegen::{Board, Setup};
///
/// let board = Board::new();
/// let mut setup = Setup::new();
/// for (square, piece) in board.pieces() {
///     setup.place(square, piece);
/// }
/// setup
///     .set_side_to_move(board.side_to_move())
///     .set_castling_rights(board.castling_rights())
///     .set_ep_square(board.ep_square())
///     .set_rule50(board.rule50())
///     .set_game_ply(board.game_ply());
///
/// let rebuilt = Board::from_setup(&setup).unwrap();
/// assert_eq!(rebuilt.fen(), board.fen());
/// assert_eq!(rebuilt.hash(), board.hash());
/// ```
#[derive(Debug, Clone)]
pub struct Setup {
    squares: [Piece; 64],
    side_to_move: Color,
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u16,
    game_ply: u16,
}

impl Default for Setup {
    fn default() -> Self {
        Self::new()
    }
}

impl Setup {
    /// Create an empty setup: no pieces, White to move, no castling rights, no
    /// en-passant target, half-move clock `0`, and game ply `1`.
    #[must_use]
    pub fn new() -> Self {
        Setup {
            squares: [NO_PIECE; 64],
            side_to_move: Color::White,
            castling_rights: 0,
            ep_square: None,
            rule50: 0,
            game_ply: 1,
        }
    }

    /// Place a piece on a square, replacing whatever stood there.
    ///
    /// [`Square::NONE`] and [`NO_PIECE`] are ignored.
    pub fn place(&mut self, sq: Square, piece: Piece) -> &mut Self {
        if sq != Square::NONE && piece != NO_PIECE {
            self.squares[sq as usize] = piece;
        }
        self
    }

    /// Remove the piece on a square, if any.
    pub fn remove(&mut self, sq: Square) -> &mut Self {
        if sq != Square::NONE {
            self.squares[sq as usize] = NO_PIECE;
        }
        self
    }

    /// Set the side to move.
    pub fn set_side_to_move(&mut self, c: Color) -> &mut Self {
        self.side_to_move = c;
        self
    }

    /// Set the raw castling-rights bits.
    ///
    /// The bits are the same ones [`Board::castling_rights`] returns:
    /// `1` = White `O-O`, `2` = White `O-O-O`, `4` = Black `O-O`,
    /// `8` = Black `O-O-O`. Out-of-range bits are ignored by
    /// [`Board::from_setup`] (they never produce a castling move), but any set
    /// right must be backed by a commoner on `e1`/`e8` and a rook on
    /// `h1`/`a1`/`h8`/`a8`.
    pub fn set_castling_rights(&mut self, rights: u8) -> &mut Self {
        self.castling_rights = rights;
        self
    }

    /// Set the en-passant target square.
    ///
    /// The target must be on rank 6 (White to move) or rank 3 (Black to move)
    /// with the enemy pawn that just double-pushed behind it, exactly as
    /// [`Board::from_fen`] requires.
    pub fn set_ep_square(&mut self, sq: Option<Square>) -> &mut Self {
        self.ep_square = sq;
        self
    }

    /// Set the half-move clock (plies since the last pawn move or capture).
    pub fn set_rule50(&mut self, n: u16) -> &mut Self {
        self.rule50 = n;
        self
    }

    /// Set the game ply (1-based, the same counter [`Board::game_ply`] returns).
    ///
    /// [`Board::from_setup`] rejects the value `0`, mirroring the 1-based FEN
    /// full-move number.
    pub fn set_game_ply(&mut self, n: u16) -> &mut Self {
        self.game_ply = n;
        self
    }

    /// Return the piece on a square, or [`NO_PIECE`].
    #[must_use]
    pub fn piece_on(&self, sq: Square) -> Piece {
        if sq == Square::NONE {
            return NO_PIECE;
        }
        self.squares[sq as usize]
    }

    /// Iterate over the occupied squares in ascending square order.
    ///
    /// Like [`Board::pieces`], empty squares are skipped and nothing is
    /// allocated.
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_ {
        SQUARES.iter().copied().filter_map(move |sq| {
            let piece = self.squares[sq as usize];
            (piece != NO_PIECE).then_some((sq, piece))
        })
    }

    /// Return the side to move.
    #[must_use]
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// Return the raw castling-rights bits.
    #[must_use]
    pub fn castling_rights(&self) -> u8 {
        self.castling_rights
    }

    /// Return the en-passant target square, if any.
    #[must_use]
    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// Return the half-move clock.
    #[must_use]
    pub fn rule50(&self) -> u16 {
        self.rule50
    }

    /// Return the game ply (1-based).
    #[must_use]
    pub fn game_ply(&self) -> u16 {
        self.game_ply
    }
}

/// Structural errors returned by [`Board::from_setup`].
///
/// These mirror the syntax-level checks of [`Board::from_fen`]; semantic
/// position checks are available separately through [`Board::from_setup_with`]
/// and [`ValidationErrors`](crate::ValidationErrors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SetupError {
    /// A castling right is set without the required commoner and rook.
    InvalidCastlingRight,
    /// The en-passant target is not on the expected rank or has no enemy pawn
    /// behind it.
    InvalidEnPassant,
    /// The game ply is zero (the game ply is 1-based).
    InvalidGamePly,
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetupError::InvalidCastlingRight => {
                f.write_str("castling right set without the required commoner and rook")
            }
            SetupError::InvalidEnPassant => f.write_str(
                "en-passant target is not on the expected rank or has no enemy pawn behind it",
            ),
            SetupError::InvalidGamePly => f.write_str("game ply must be at least 1"),
        }
    }
}

impl Error for SetupError {}

impl From<SetupError> for FenLoadError {
    /// Map a structural setup error onto the closest [`FenError`] variant so
    /// that [`Board::from_setup_with`] keeps the FEN error vocabulary.
    fn from(value: SetupError) -> Self {
        FenLoadError::Syntax(match value {
            SetupError::InvalidCastlingRight => FenError::InvalidCastling(value.to_string()),
            SetupError::InvalidEnPassant => FenError::InvalidEpSquare(value.to_string()),
            SetupError::InvalidGamePly => FenError::ParseInt(value.to_string()),
        })
    }
}

impl Board {
    /// Build a board from a piece-set description.
    ///
    /// This mirrors the permissiveness of [`Board::from_fen`]: only the
    /// structural checks run (castling rights backed by the required commoner
    /// and rook, en-passant target on the right rank with the enemy pawn behind
    /// it, and a 1-based game ply). Missing commoners, pawns on the back ranks,
    /// and other game-state oddities are accepted, so terminal and artificial
    /// fixtures can be built. Use [`from_setup_with`](Self::from_setup_with) for
    /// the opt-in semantic checks.
    ///
    /// The piece placement, side to move, castling rights, en-passant target,
    /// and clocks are taken from `setup`; the bitboards and the Zobrist key are
    /// derived, so the result is identical to parsing the equivalent FEN.
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::InvalidCastlingRight`],
    /// [`SetupError::InvalidEnPassant`], or [`SetupError::InvalidGamePly`].
    pub fn from_setup(setup: &Setup) -> Result<Board, SetupError> {
        if setup.game_ply == 0 {
            return Err(SetupError::InvalidGamePly);
        }

        let squares = setup.squares;
        let rights = setup.castling_rights;
        let white_commoner = make_piece(Color::White, PieceType::Commoner);
        let white_rook = make_piece(Color::White, PieceType::Rook);
        let black_commoner = make_piece(Color::Black, PieceType::Commoner);
        let black_rook = make_piece(Color::Black, PieceType::Rook);
        if rights & WK_CASTLE != 0
            && (squares[Square::E1 as usize] != white_commoner
                || squares[Square::H1 as usize] != white_rook)
        {
            return Err(SetupError::InvalidCastlingRight);
        }
        if rights & WQ_CASTLE != 0
            && (squares[Square::E1 as usize] != white_commoner
                || squares[Square::A1 as usize] != white_rook)
        {
            return Err(SetupError::InvalidCastlingRight);
        }
        if rights & BK_CASTLE != 0
            && (squares[Square::E8 as usize] != black_commoner
                || squares[Square::H8 as usize] != black_rook)
        {
            return Err(SetupError::InvalidCastlingRight);
        }
        if rights & BQ_CASTLE != 0
            && (squares[Square::E8 as usize] != black_commoner
                || squares[Square::A8 as usize] != black_rook)
        {
            return Err(SetupError::InvalidCastlingRight);
        }

        if let Some(ep) = setup.ep_square {
            let expected_rank = match setup.side_to_move {
                Color::White => Rank::R6,
                Color::Black => Rank::R3,
            };
            if rank_of(ep) != expected_rank {
                return Err(SetupError::InvalidEnPassant);
            }
            let victim = match setup.side_to_move {
                Color::White => Square::from_index(ep as i8 - 8),
                Color::Black => Square::from_index(ep as i8 + 8),
            };
            if squares[victim as usize] != make_piece(setup.side_to_move.flip(), PieceType::Pawn) {
                return Err(SetupError::InvalidEnPassant);
            }
        }

        Ok(Board::from_setup_parts(
            squares,
            setup.side_to_move,
            setup.castling_rights,
            setup.ep_square,
            setup.rule50,
            setup.game_ply,
        ))
    }

    /// Build a board from a piece-set description and apply a semantic
    /// validation policy.
    ///
    /// The opt-in counterpart to [`from_setup`](Self::from_setup), using the
    /// same [`FenPolicy`] and [`ValidationErrors`](crate::ValidationErrors)
    /// machinery as [`Board::from_fen_with`]: structural setup errors are
    /// reported as [`FenLoadError::Syntax`] and policy failures as
    /// [`FenLoadError::InvalidPosition`].
    ///
    /// # Errors
    ///
    /// Returns [`FenLoadError::Syntax`] for a structurally invalid setup and
    /// [`FenLoadError::InvalidPosition`] when the position fails `policy`.
    pub fn from_setup_with(setup: &Setup, policy: FenPolicy) -> Result<Board, FenLoadError> {
        let board = Board::from_setup(setup).map_err(FenLoadError::from)?;
        let errors = board.validation_errors(policy);
        if errors.is_empty() {
            Ok(board)
        } else {
            Err(FenLoadError::InvalidPosition(errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{FenPolicy, StateInfo};

    fn place(setup: &mut Setup, sq: Square, piece: Piece) {
        setup.place(sq, piece);
    }

    #[test]
    fn setup_defaults() {
        let setup = Setup::default();
        assert_eq!(setup.pieces().count(), 0);
        assert_eq!(setup.side_to_move(), Color::White);
        assert_eq!(setup.castling_rights(), 0);
        assert_eq!(setup.ep_square(), None);
        assert_eq!(setup.rule50(), 0);
        assert_eq!(setup.game_ply(), 1);
        // The defaults build the empty board that `from_fen` also accepts.
        let board = Board::from_setup(&setup).expect("empty setup is structurally valid");
        assert_eq!(board.fen(), "8/8/8/8/8/8/8/8 w - - 0 1");
        assert_eq!(board.pieces().count(), 0);
    }

    /// Every corpus position rebuilt from a piece set must be the same board.
    #[test]
    fn setup_matches_fen() {
        const FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
            "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
            "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "r1b1kbnr/pppp1Np1/2n5/5p1p/1P1Pp3/4P3/P1P2PPP/RN1QKB1R b KQkq - 1 7",
            "4k3/8/8/8/8/8/8/4K3 w - - 99 60",
            "8/8/8/8/8/8/8/k7 w - - 0 1",
            "4R1k1/8/8/8/8/8/8/4K3 w - - 3 42",
        ];

        for fen in FENS {
            let board = Board::from_fen(fen).expect("fixture must parse");
            let mut setup = Setup::new();
            for (sq, piece) in board.pieces() {
                place(&mut setup, sq, piece);
            }
            setup
                .set_side_to_move(board.side_to_move())
                .set_castling_rights(board.castling_rights())
                .set_ep_square(board.ep_square())
                .set_rule50(board.rule50())
                .set_game_ply(board.game_ply());

            let rebuilt = Board::from_setup(&setup).expect("setup must be structurally valid");
            assert_eq!(rebuilt.fen(), board.fen(), "{fen}: FEN");
            assert_eq!(rebuilt.hash(), board.hash(), "{fen}: hash");
            assert_eq!(
                rebuilt.hash_with(EnPassantMode::Legal),
                board.hash_with(EnPassantMode::Legal),
                "{fen}: legal-ep hash"
            );
            assert_eq!(rebuilt.side_to_move(), board.side_to_move(), "{fen}: stm");
            assert_eq!(rebuilt.ep_square(), board.ep_square(), "{fen}: ep");
            assert_eq!(rebuilt.rule50(), board.rule50(), "{fen}: rule50");
            assert_eq!(rebuilt.game_ply(), board.game_ply(), "{fen}: game ply");
            assert_eq!(
                rebuilt.castling_rights(),
                board.castling_rights(),
                "{fen}: castling"
            );
            assert_eq!(rebuilt.occupied(), board.occupied(), "{fen}: occupancy");
            for pt in crate::types::PIECE_TYPES {
                for color in [Color::White, Color::Black] {
                    assert_eq!(
                        rebuilt.pieces_color_pt(color, pt),
                        board.pieces_color_pt(color, pt),
                        "{fen}: {color:?} {pt:?}"
                    );
                }
            }

            // The move generators must agree, not just the serialized data.
            let mut a = StateInfo::new();
            let mut b = StateInfo::new();
            rebuilt.populate_state(&mut a);
            board.populate_state(&mut b);
            assert_eq!(a.checkers, b.checkers, "{fen}: checkers");
            assert_eq!(a.pinned, b.pinned, "{fen}: pinned");
            assert_eq!(rebuilt.outcome(), board.outcome(), "{fen}: outcome");
        }
    }

    #[test]
    fn setup_midgame_matches_fen_after_moves() {
        // A play-out, not a fixture: any drift between the two construction
        // paths shows up in the hashes as soon as the boards diverge.
        let mut board = Board::new();
        let mut state = StateInfo::new();
        let mut played = 0u32;
        for i in 0..40u64 {
            let mut setup = Setup::new();
            for (sq, piece) in board.pieces() {
                setup.place(sq, piece);
            }
            setup
                .set_side_to_move(board.side_to_move())
                .set_castling_rights(board.castling_rights())
                .set_ep_square(board.ep_square())
                .set_rule50(board.rule50())
                .set_game_ply(board.game_ply());
            let rebuilt = Board::from_setup(&setup).expect("play-out setup must be valid");
            assert_eq!(rebuilt.fen(), board.fen());
            assert_eq!(rebuilt.hash(), board.hash());

            let mut moves = MoveList::new();
            board.populate_state(&mut state);
            crate::movegen::generate_legal_with_state(&board, &state, &mut moves);
            if moves.is_empty() {
                break;
            }
            // Deterministic pseudo-random pick.
            let pick = ((i * 7 + 3) % moves.len() as u64) as usize;
            board.do_move(moves.as_slice()[pick], &mut state);
            played += 1;
        }
        assert!(played > 20, "expected a real play-out, got {played}");
    }

    #[test]
    fn setup_place_remove() {
        let mut setup = Setup::new();
        setup
            .place(Square::E2, make_piece(Color::White, PieceType::Pawn))
            .place(Square::E4, make_piece(Color::White, PieceType::Pawn))
            .place(Square::A1, make_piece(Color::White, PieceType::Rook));

        assert_eq!(
            setup.piece_on(Square::E2),
            make_piece(Color::White, PieceType::Pawn)
        );
        // Placing replaces the previous piece.
        setup.place(Square::E2, make_piece(Color::Black, PieceType::Knight));
        assert_eq!(
            setup.piece_on(Square::E2),
            make_piece(Color::Black, PieceType::Knight)
        );
        setup.remove(Square::E2);
        assert_eq!(setup.piece_on(Square::E2), NO_PIECE);

        // Out-of-board input is ignored.
        setup.place(Square::NONE, make_piece(Color::White, PieceType::Queen));
        setup.remove(Square::NONE);
        setup.place(Square::A1, NO_PIECE);
        assert_eq!(setup.pieces().count(), 2);

        let board = Board::from_setup(&setup).unwrap();
        assert_eq!(board.fen(), "8/8/8/8/4P3/8/8/R7 w - - 0 1");
    }

    #[test]
    fn setup_castling_and_ep_validation() {
        let mut setup = Setup::new();
        setup
            .place(Square::E1, make_piece(Color::White, PieceType::Commoner))
            .set_castling_rights(1);
        assert_eq!(
            Board::from_setup(&setup).unwrap_err(),
            SetupError::InvalidCastlingRight
        );
        setup.place(Square::H1, make_piece(Color::White, PieceType::Rook));
        assert!(Board::from_setup(&setup).is_ok());

        // Queen side: the a-file rook is required, not the h-file one.
        let mut setup = Setup::new();
        setup
            .place(Square::E1, make_piece(Color::White, PieceType::Commoner))
            .place(Square::H1, make_piece(Color::White, PieceType::Rook))
            .set_castling_rights(2);
        assert_eq!(
            Board::from_setup(&setup).unwrap_err(),
            SetupError::InvalidCastlingRight
        );

        // En passant: the wrong rank is rejected.
        let mut setup = Setup::new();
        setup
            .place(Square::E4, make_piece(Color::Black, PieceType::Pawn))
            .set_ep_square(Some(Square::D5));
        assert_eq!(
            Board::from_setup(&setup).unwrap_err(),
            SetupError::InvalidEnPassant
        );

        // En passant: no enemy pawn behind the target is rejected.
        let mut setup = Setup::new();
        setup.set_ep_square(Some(Square::D6));
        assert_eq!(
            Board::from_setup(&setup).unwrap_err(),
            SetupError::InvalidEnPassant
        );

        // En passant: the double-pushed pawn behind the target is accepted.
        let mut setup = Setup::new();
        setup
            .place(Square::D5, make_piece(Color::Black, PieceType::Pawn))
            .set_ep_square(Some(Square::D6));
        assert_eq!(
            Board::from_setup(&setup).unwrap().fen(),
            "8/8/8/3p4/8/8/8/8 w - d6 0 1"
        );

        // Black to move: rank 3 with a white pawn on rank 4.
        let mut setup = Setup::new();
        setup
            .place(Square::D4, make_piece(Color::White, PieceType::Pawn))
            .set_ep_square(Some(Square::D3))
            .set_side_to_move(Color::Black);
        assert_eq!(
            Board::from_setup(&setup).unwrap().fen(),
            "8/8/8/8/3P4/8/8/8 b - d3 0 1"
        );

        // Zero game ply is rejected.
        let mut setup = Setup::new();
        setup.set_game_ply(0);
        assert_eq!(
            Board::from_setup(&setup).unwrap_err(),
            SetupError::InvalidGamePly
        );
    }

    #[test]
    fn setup_with_policy() {
        // The empty board is structurally valid but fails the strict policy.
        let setup = Setup::new();
        assert!(Board::from_setup(&setup).is_ok());
        assert!(matches!(
            Board::from_setup_with(&setup, FenPolicy::Strict),
            Err(FenLoadError::InvalidPosition(e))
                if e.contains(crate::ValidationErrors::EMPTY_BOARD)
        ));
        assert!(Board::from_setup_with(&setup, FenPolicy::Permissive).is_ok());
        assert!(Board::from_setup_with(&setup, FenPolicy::AllowTerminal).is_ok());

        // A start position passes the strict policy.
        let board = Board::new();
        let mut setup = Setup::new();
        for (sq, piece) in board.pieces() {
            setup.place(sq, piece);
        }
        setup
            .set_castling_rights(board.castling_rights())
            .set_side_to_move(board.side_to_move());
        assert!(Board::from_setup_with(&setup, FenPolicy::Strict).is_ok());

        // A one-commoner board is terminal but not strict.
        let mut setup = Setup::new();
        setup.place(Square::E1, make_piece(Color::White, PieceType::Commoner));
        assert!(matches!(
            Board::from_setup_with(&setup, FenPolicy::Strict),
            Err(FenLoadError::InvalidPosition(e))
                if e.contains(crate::ValidationErrors::MISSING_COMMONER_BLACK)
        ));
        assert!(Board::from_setup_with(&setup, FenPolicy::AllowTerminal).is_ok());

        // Structural errors map to the FEN syntax vocabulary either way.
        let mut setup = Setup::new();
        setup.set_castling_rights(4);
        assert!(matches!(
            Board::from_setup_with(&setup, FenPolicy::Permissive),
            Err(FenLoadError::Syntax(FenError::InvalidCastling(_)))
        ));
    }

    #[test]
    fn setup_permissive_accepts_artificial_boards() {
        // A pawn on the back rank and a missing commoner are semantic
        // problems, not structural ones: `from_setup` stays permissive, the
        // policies reject them.
        let mut setup = Setup::new();
        setup.place(Square::A1, make_piece(Color::White, PieceType::Pawn));
        let board = Board::from_setup(&setup).expect("structurally valid");
        assert_eq!(board.fen(), "8/8/8/8/8/8/8/P7 w - - 0 1");
        assert!(Board::from_setup_with(&setup, FenPolicy::Permissive).is_ok());
        assert!(matches!(
            Board::from_setup_with(&setup, FenPolicy::AllowTerminal),
            Err(FenLoadError::InvalidPosition(e)) if e.contains(crate::ValidationErrors::PAWNS_ON_BACKRANK)
        ));
    }

    #[test]
    fn board_pieces_order() {
        let board = Board::from_fen("4k3/8/8/8/8/8/8/R3K2R w - - 0 1").unwrap();
        let squares: Vec<Square> = board.pieces().map(|(sq, _)| sq).collect();
        assert_eq!(
            squares,
            vec![Square::A1, Square::E1, Square::H1, Square::E8],
            "ascending square order, no empty squares"
        );

        // The iterator yields exactly the non-empty squares.
        let occupied = board.occupied();
        assert_eq!(board.pieces().count() as u32, occupied.count());
        for (sq, piece) in board.pieces() {
            assert_eq!(piece, board.piece_on(sq));
            assert_ne!(piece, NO_PIECE);
        }

        // Setup exposes the same traversal.
        let mut setup = Setup::new();
        for (sq, piece) in board.pieces() {
            setup.place(sq, piece);
        }
        let setup_squares: Vec<Square> = setup.pieces().map(|(sq, _)| sq).collect();
        assert_eq!(setup_squares, squares);
    }

    #[test]
    fn setup_error_display() {
        assert!(!SetupError::InvalidCastlingRight.to_string().is_empty());
        assert!(!SetupError::InvalidEnPassant.to_string().is_empty());
        assert!(!SetupError::InvalidGamePly.to_string().is_empty());
    }
}
