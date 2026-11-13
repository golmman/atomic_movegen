//! Node-constant effect context and read-only per-move classification.
//!
//! [`Board::legal`] answers “is this move legal?” but recomputes the blast set
//! and the attacker query for every candidate. This module packages the
//! node-constant data into an [`EffectContext`] that is built once per position
//! and then classifies each pseudo-legal candidate into a [`MoveEffect`]:
//! capture, blast casualties, whether the move checks or explodes the enemy
//! commoner, whether the mover keeps a commoner, and legality — all from a
//! bitboard view of the child position, i.e. *without* writing a board.
//!
//! The rules live in one place only: the effect path is differentially
//! validated against [`Board::legal`] and against a cloned `do_move` child for
//! every pseudo-legal move of the fixture corpus and of seeded random
//! play-outs (`tests/effects.rs`). Castling keeps the [`Board::legal`] fallback
//! because its pass-through-square rule is a separate rule surface.
//!
//! ```
//! use atomic_movegen::{Board, Move};
//!
//! let board = Board::new();
//! let ctx = board.effect_context();
//!
//! let m = Move::from_uci("e2e4").unwrap();
//! let effect = ctx.effect(&board, m);
//! assert!(effect.is_legal);
//! assert!(!effect.is_capture);
//! assert!(effect.leaves_own_commoner);
//! assert!(!effect.checks_enemy);
//! assert_eq!(effect.is_legal, board.is_legal(m));
//! ```

use crate::attacks;
use crate::board::{BlastCasualties, Board, StateInfo, castling_squares, is_attacked_by};
use crate::types::*;

/// Node-constant data for classifying moves without mutating the board.
///
/// Build once per position with [`Board::effect_context`] or
/// [`Board::effect_context_with_state`], then call [`effect`](Self::effect) or
/// [`is_legal`](Self::is_legal) for each pseudo-legal candidate. The context is
/// a snapshot: it stays valid while `board` does not change and holds no
/// reference to it.
///
/// The context deliberately stores only data the rules need (piece
/// placement, side to move, and the populated [`StateInfo`]) — never a search
/// concept. The node-constant `checkers`/`pinned` data of [`StateInfo`] is not
/// part of the snapshot because the effect path derives post-move attacks
/// directly; callers that need pins read them from [`StateInfo`].
#[derive(Debug, Clone, Copy)]
pub struct EffectContext {
    /// The side to move in the context position.
    us: Color,
    /// The opponent of the side to move.
    them: Color,
    /// Occupied squares in the context position.
    occupied: Bitboard,
    /// Per-color piece bitboards in the context position.
    by_color: [Bitboard; Color::NB],
    /// Per-type piece bitboards in the context position.
    by_type: [Bitboard; PieceType::NB],
    /// The commoners of the side to move.
    our_commoners: Bitboard,
    /// `true` when either side has no commoner, i.e. the game is already over
    /// and [`Board::legal`] rejects every move.
    game_over: bool,
    /// Copy of the populated state, used only for the castling fallback.
    state: StateInfo,
}

/// The rules effects of one pseudo-legal move.
///
/// Returned by [`EffectContext::effect`]. Every field is derivable from the
/// atomic rules alone; nothing here encodes a search design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveEffect {
    /// The move removes a piece (normal capture or en passant).
    pub is_capture: bool,
    /// Every piece removed by the move's blast, including the capturer at
    /// ground zero and the en-passant victim. Empty for quiet moves, quiet
    /// promotions, and castling.
    pub casualties: BlastCasualties,
    /// The move leaves the enemy commoner attacked by the mover's pieces,
    /// after the move and the blast.
    ///
    /// `false` when the enemy commoner is gone, when the mover has no commoner
    /// left, or when the two commoners touch — touching makes the commoner
    /// immune to every attack, so it is not a check.
    pub checks_enemy: bool,
    /// The move's blast removes the enemy commoner. Such a move ends the game
    /// immediately; the blast may or may not also be the mover's own.
    pub explodes_enemy_commoner: bool,
    /// The mover still has at least one commoner after the move and the blast.
    ///
    /// A move with `false` here destroys the mover's own commoner and is
    /// always illegal.
    pub leaves_own_commoner: bool,
    /// Equivalent to [`Board::legal`] for a pseudo-legal move decorated by the
    /// same populated [`StateInfo`] as the context.
    pub is_legal: bool,
}

impl EffectContext {
    /// Build the context from a populated [`StateInfo`].
    ///
    /// `state` must have been populated by
    /// [`Board::populate_state`] for `board`; a stale state changes which
    /// positions count as already over, exactly as it does for
    /// [`Board::legal`].
    #[must_use]
    pub fn new(board: &Board, state: &StateInfo) -> Self {
        let us = board.side_to_move();
        let them = us.flip();
        Self {
            us,
            them,
            occupied: board.occupied(),
            by_color: [
                board.pieces_color(Color::White),
                board.pieces_color(Color::Black),
            ],
            by_type: PIECE_TYPES.map(|pt| board.pieces_pt(pt)),
            our_commoners: board.commoners(us),
            game_over: state.commoners_count == 0 || state.them_commoners_count == 0,
            state: *state,
        }
    }

    /// The side to move in the context position.
    #[inline]
    #[must_use]
    pub fn side_to_move(&self) -> Color {
        self.us
    }

    /// Classify `m` without mutating `board`.
    ///
    /// `m` must be a pseudo-legal move for this position (as produced by
    /// [`generate_pseudo_legal`](crate::movegen::generate_pseudo_legal) or
    /// accepted by [`Board::is_pseudo_legal`]). For malformed input — a
    /// [`Move::NONE`] or an empty origin square — every field except
    /// `is_capture` is `false`/empty and `is_legal` is `false`; use
    /// [`Board::is_legal`] for untrusted single moves.
    ///
    /// This never calls [`Board::do_move`] or [`Board::legal`], except for
    /// castling, which falls back to [`Board::legal`] to reuse the
    /// pass-through-square rule.
    #[must_use]
    pub fn effect(&self, board: &Board, m: Move) -> MoveEffect {
        let Some(post) = self.analyze(board, m) else {
            return MoveEffect {
                is_capture: board.is_capture(m),
                casualties: BlastCasualties::default(),
                checks_enemy: false,
                explodes_enemy_commoner: false,
                leaves_own_commoner: false,
                is_legal: false,
            };
        };

        let is_legal = self.is_legal_from(board, m, &post);
        MoveEffect {
            is_capture: post.is_capture,
            casualties: post.casualties,
            checks_enemy: self.checks_enemy(&post),
            explodes_enemy_commoner: !self.their_commoners().is_empty()
                && post.commoners(self.them).is_empty(),
            leaves_own_commoner: !post.commoners(self.us).is_empty(),
            is_legal,
        }
    }

    /// Fast legality query for the pseudo-legal move `m`.
    ///
    /// Equal to `board.legal(m, &state)` for a pseudo-legal `m` and the
    /// populated `state` the context was built from. Malformed input returns
    /// `false`; [`Board::is_legal_with_state`] remains the validating
    /// untrusted-input entry point.
    #[must_use]
    pub fn is_legal(&self, board: &Board, m: Move) -> bool {
        if self.game_over {
            return false;
        }
        match self.analyze(board, m) {
            None => false,
            Some(post) => self.is_legal_from(board, m, &post),
        }
    }

    /// The outcome of the child position after `m`, from the child side-to-move
    /// perspective, without building the child.
    ///
    /// Returns `None` when `m` is not legal on this board. For every legal `m`
    /// the result is exactly
    /// `{ let mut b = board.clone(); b.do_move(m, &mut st); b.outcome() }`.
    ///
    /// The immediate cases are answered from the effect: extinction of the
    /// enemy commoner is a loss for the child, the commoner-only material draw
    /// and the non-check 50-move draw are read from the blast casualties and
    /// [`Board::rule50_after`]. A child that still needs “does the child have a
    /// legal move?” — checkmate and stalemate — falls back to a cloned board
    /// and [`Board::outcome`]; see the module docs.
    #[must_use]
    pub fn outcome_after(&self, board: &Board, m: Move) -> Option<Outcome> {
        if self.game_over {
            return None;
        }
        let post = self.analyze(board, m)?;
        if !self.is_legal_from(board, m, &post) {
            return None;
        }

        // The child has no commoner: the side to move there (the opponent)
        // loses immediately. The mover's own commoner is guaranteed alive by
        // the legality check above.
        if post.commoners(self.them).is_empty() {
            return Some(Outcome::Loss);
        }

        // Commoner-only material draw, from the child's piece count.
        let child_pieces = (self.occupied & !post.casualties.all()).count();
        if child_pieces == 2
            && !post.commoners(Color::White).is_empty()
            && !post.commoners(Color::Black).is_empty()
        {
            return Some(Outcome::Draw);
        }

        // 50-move rule. When the child is in check, mate outranks the clock, so
        // the remaining question needs the full outcome path.
        if board.rule50_after(m) >= 100 && !self.checks_enemy(&post) {
            return Some(Outcome::Draw);
        }

        // Fallback: checkmate and stalemate both require the child's “has a
        // legal move?” answer, which needs a board write today.
        let mut child = board.clone();
        let mut state = StateInfo::new();
        child.do_move(m, &mut state);
        child.outcome()
    }

    /// The opponent's commoners in the context position.
    #[inline]
    fn their_commoners(&self) -> Bitboard {
        self.by_color[self.them as usize] & self.by_type[PieceType::Commoner as usize]
    }

    /// `true` when the move is legal, assuming the effect path decides it.
    ///
    /// Rejects everything when the game is over, and this is the ladder of
    /// [`Board::legal`]: no own commoner may be
    /// destroyed, exploding the enemy commoner wins at once, touching commoners
    /// are immune, and otherwise the mover's commoner may not stand on an
    /// attacked square after the move and blast.
    fn is_legal_from(&self, board: &Board, m: Move, post: &PostMove) -> bool {
        if self.game_over {
            return false;
        }
        if m.move_type() == MoveType::Castling {
            // Pass-through-square attacks are a separate rule surface; keep the
            // oracle until a differential proves the effect path sufficient.
            return board.legal(m, &self.state);
        }

        let our_after = post.commoners(self.us);
        // A move may not destroy any of our commoners. In a legal position
        // there is exactly one, so this is the usual self-explosion check.
        if our_after.count() != self.our_commoners.count() {
            return false;
        }
        let their_after = post.commoners(self.them);
        if their_after.is_empty() {
            return true;
        }
        if our_after.is_empty() {
            // Unreachable for a populated state (the count comparison above
            // rejects it); kept so the `lsb` below is total.
            return false;
        }

        let ksq = our_after.lsb();
        // Touching the enemy commoner grants immunity to every attack.
        if !(their_after & attacks::king_attacks(ksq)).is_empty() {
            return true;
        }

        !is_attacked_by(
            ksq,
            post.occupied,
            &post.by_type,
            post.by_color[self.them as usize],
            self.us,
        )
    }

    /// `true` when the move leaves the enemy commoner checked.
    ///
    /// Mirrors [`Board::checkers`] on the child: no check when either commoner
    /// is gone, no check when the commoners touch, and commoner attacks are
    /// excluded from the attacker query because an adjacent commoner touches.
    fn checks_enemy(&self, post: &PostMove) -> bool {
        let their_after = post.commoners(self.them);
        let our_after = post.commoners(self.us);
        if their_after.is_empty() || our_after.is_empty() {
            return false;
        }
        let ksq = their_after.lsb();
        if !(our_after & attacks::king_attacks(ksq)).is_empty() {
            return false;
        }
        is_attacked_by(
            ksq,
            post.occupied,
            &post.by_type,
            post.by_color[self.us as usize],
            self.them,
        )
    }

    /// Derive the post-move, post-blast piece placement of `m` without
    /// touching the board, or `None` for a move with no origin piece.
    fn analyze(&self, board: &Board, m: Move) -> Option<PostMove> {
        if m == Move::NONE {
            return None;
        }
        let from = m.from_sq();
        let to = m.to_sq();
        let moving = board.piece_on(from);
        if moving == NO_PIECE {
            return None;
        }

        let us = self.us;
        let is_capture = board.is_capture(m);
        let casualties = board.blast_casualties(m);
        let removed = casualties.all();
        let from_bb = Bitboard::square_bb(from);
        let to_bb = Bitboard::square_bb(to);

        let mut post = PostMove {
            occupied: self.occupied & !removed,
            by_color: [
                self.by_color[Color::White as usize] & !removed,
                self.by_color[Color::Black as usize] & !removed,
            ],
            by_type: self.by_type.map(|bb| bb & !removed),
            is_capture,
            casualties,
        };

        match m.move_type() {
            MoveType::Castling => {
                let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
                let kfrom_bb = Bitboard::square_bb(kfrom);
                let rfrom_bb = Bitboard::square_bb(rfrom);
                let kto_bb = Bitboard::square_bb(kto);
                let rto_bb = Bitboard::square_bb(rto);
                post.occupied = (post.occupied & !(kfrom_bb | rfrom_bb)) | kto_bb | rto_bb;
                post.by_color[us as usize] =
                    (post.by_color[us as usize] & !(kfrom_bb | rfrom_bb)) | kto_bb | rto_bb;
                post.by_type[PieceType::Commoner as usize] =
                    (post.by_type[PieceType::Commoner as usize] & !kfrom_bb) | kto_bb;
                post.by_type[PieceType::Rook as usize] =
                    (post.by_type[PieceType::Rook as usize] & !rfrom_bb) | rto_bb;
            }
            _ if post.is_capture => {
                // Every capture — normal, en passant, and capturing promotion —
                // destroys the capturer at ground zero: the origin square empties
                // and nothing appears on `to`. (The capturer's removal is
                // recorded in `casualties` at `to`, not at `from`.)
                post.occupied = post.occupied & !from_bb;
                post.by_color[us as usize] = post.by_color[us as usize] & !from_bb;
                if let Some(pt) = moving.type_of() {
                    post.by_type[pt as usize] = post.by_type[pt as usize] & !from_bb;
                }
            }
            MoveType::Promotion => {
                let promoted = m.promotion_type();
                post.occupied = (post.occupied & !from_bb) | to_bb;
                post.by_color[us as usize] = (post.by_color[us as usize] & !from_bb) | to_bb;
                post.by_type[PieceType::Pawn as usize] =
                    post.by_type[PieceType::Pawn as usize] & !from_bb;
                post.by_type[promoted as usize] = post.by_type[promoted as usize] | to_bb;
            }
            // En passant is always a capture, so the guarded arm above handles
            // it; this arm only satisfies the match exhaustiveness check.
            MoveType::EnPassant => {}
            MoveType::Normal => {
                if let Some(pt) = moving.type_of() {
                    post.occupied = (post.occupied & !from_bb) | to_bb;
                    post.by_color[us as usize] = (post.by_color[us as usize] & !from_bb) | to_bb;
                    post.by_type[pt as usize] = (post.by_type[pt as usize] & !from_bb) | to_bb;
                }
            }
        }

        Some(post)
    }
}

/// The piece placement of a child position, derived without a board write.
#[derive(Debug, Clone, Copy)]
struct PostMove {
    /// Occupied squares after the move and blast.
    occupied: Bitboard,
    /// Per-color piece bitboards after the move and blast.
    by_color: [Bitboard; Color::NB],
    /// Per-type piece bitboards after the move and blast.
    by_type: [Bitboard; PieceType::NB],
    /// The move removes a piece.
    is_capture: bool,
    /// The pieces the move's blast removes.
    casualties: BlastCasualties,
}

impl PostMove {
    /// The commoners of `c` in the child position.
    #[inline]
    fn commoners(&self, c: Color) -> Bitboard {
        self.by_color[c as usize] & self.by_type[PieceType::Commoner as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::{generate_legal_with_state, generate_pseudo_legal};
    use crate::types::MoveList;

    /// Positions that exercise every branch of the effect path: quiet moves,
    /// captures, en passant, promotions, castling, checks, pins, explosions,
    /// touching commoners, stalemate, checkmate, and game-over boards.
    pub(super) const CORPUS: &[&str] = &[
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
        "k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1",
        "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
        "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
        "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        "8/8/8/8/8/8/8/k7 w - - 0 1",
        "k7/8/8/8/8/8/8/7K w - - 0 1",
        "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15",
        "rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5",
        "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
        // 50-move clock with the side to move in check and not in check.
        "8/8/8/8/8/4k3/8/4K2q w - - 99 60",
        "8/8/8/8/8/4k3/8/4K2q w - - 100 60",
    ];

    #[test]
    fn effect_fields_match_child_and_legal() {
        for fen in CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut moves = MoveList::new();
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let ctx = EffectContext::new(&board, &state);

            generate_pseudo_legal(&board, &mut moves);
            let mut legal = MoveList::new();
            generate_legal_with_state(&board, &state, &mut legal);

            for &m in moves.as_slice() {
                let e = ctx.effect(&board, m);
                assert_eq!(e.is_capture, board.is_capture(m), "{fen} {m}");
                assert_eq!(e.casualties, board.blast_casualties(m), "{fen} {m}");
                assert_eq!(e.is_legal, board.legal(m, &state), "{fen} {m}");
                assert_eq!(e.is_legal, ctx.is_legal(&board, m), "{fen} {m}");
                assert_eq!(
                    e.is_legal,
                    legal.as_slice().contains(&m),
                    "{fen} {m}: legal list membership"
                );

                // Child comparison.
                let us = board.side_to_move();
                let them = us.flip();
                let mut child = board.clone();
                let mut child_state = StateInfo::new();
                child.do_move(m, &mut child_state);
                assert_eq!(
                    e.leaves_own_commoner,
                    !child.commoners(us).is_empty(),
                    "{fen} {m}: own commoner"
                );
                assert_eq!(
                    e.explodes_enemy_commoner,
                    !board.commoners(them).is_empty() && child.commoners(them).is_empty(),
                    "{fen} {m}: enemy commoner"
                );
                assert_eq!(
                    e.checks_enemy,
                    !child.checkers().is_empty(),
                    "{fen} {m}: checks"
                );
            }
        }
    }

    #[test]
    fn outcome_after_matches_do_move() {
        for fen in CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut legal = MoveList::new();
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let ctx = EffectContext::new(&board, &state);

            generate_legal_with_state(&board, &state, &mut legal);
            for &m in legal.as_slice() {
                let mut child = board.clone();
                let mut child_state = StateInfo::new();
                child.do_move(m, &mut child_state);
                assert_eq!(
                    ctx.outcome_after(&board, m),
                    child.outcome(),
                    "{fen} {m}: outcome_after"
                );
                assert_eq!(
                    board.outcome_after(m, &state),
                    child.outcome(),
                    "{fen} {m}: Board::outcome_after"
                );
            }
        }
    }

    #[test]
    fn outcome_after_rejects_illegal_moves() {
        // Every pseudo-legal-but-illegal move returns `None`.
        let board =
            Board::from_fen("k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1").expect("FEN must parse");
        let ctx = board.effect_context();
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        assert!(!pseudo.is_empty());
        for &m in pseudo.as_slice() {
            assert!(!ctx.is_legal(&board, m), "{m} should be illegal");
            assert_eq!(ctx.outcome_after(&board, m), None, "{m}");
        }
    }

    #[test]
    fn context_is_position_constant() {
        let board = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        let ctx = board.effect_context();
        assert_eq!(ctx.side_to_move(), Color::White);
        // The board is untouched by any query.
        let fen_before = board.fen();
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        for &m in pseudo.as_slice() {
            let _ = ctx.effect(&board, m);
            let _ = ctx.is_legal(&board, m);
        }
        assert_eq!(board.fen(), fen_before);
    }

    #[test]
    fn malformed_moves_are_rejected() {
        let board = Board::new();
        let ctx = board.effect_context();
        assert!(!ctx.is_legal(&board, Move::NONE));
        assert_eq!(ctx.outcome_after(&board, Move::NONE), None);
        let e = ctx.effect(&board, Move::NONE);
        assert!(!e.is_legal);
        assert!(e.casualties.is_empty());

        // Empty origin square.
        let m = Move::make_move(Square::E4, Square::E5);
        assert!(!ctx.is_legal(&board, m));
        assert!(!ctx.effect(&board, m).is_legal);
    }

    #[test]
    fn game_over_positions_are_never_legal() {
        for fen in ["8/8/8/8/8/8/8/k7 w - - 0 1", "k7/8/8/8/8/8/8/8 b - - 0 1"] {
            let board = Board::from_fen(fen).unwrap();
            let ctx = board.effect_context();
            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                assert!(!ctx.is_legal(&board, m), "{fen} {m}");
                assert!(!ctx.effect(&board, m).is_legal, "{fen} {m}");
                assert_eq!(ctx.outcome_after(&board, m), None, "{fen} {m}");
            }
        }
    }
}
