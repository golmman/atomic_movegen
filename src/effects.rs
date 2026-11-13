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
//! play-outs (`tests/effects.rs`). Castling is decided by the same
//! [`Board::legal`] body, fed the context's view.
//!
//! # Child terminality
//!
//! [`EffectContext::terminal_after`] answers "is the position reached by this
//! move decided, and why?" from the same bitboard view of the child, with no
//! clone and no board write. It is exactly
//! `{ let mut c = board.clone(); c.do_move(m, &mut st); c.terminal() }` for
//! every pseudo-legal move, and `None` when the child is not decided:
//!
//! ```
//! use atomic_movegen::{Board, Move, Outcome, TerminalReason};
//!
//! let board = Board::from_fen("7k/8/8/5N2/3K4/8/8/R6R w - - 99 60").unwrap();
//! let ctx = board.effect_context();
//!
//! let m = Move::from_uci("a1g1").unwrap();
//! let terminal = ctx.terminal_after(&board, m).expect("mate on the clock");
//! assert_eq!(terminal.outcome, Outcome::Loss);
//! assert_eq!(terminal.reason, TerminalReason::NoMovesInCheck);
//!
//! // A quiet move that leaves the game running is not decided.
//! let board = Board::new();
//! let ctx = board.effect_context();
//! assert_eq!(ctx.terminal_after(&board, Move::from_uci("e2e4").unwrap()), None);
//! ```
//!
//! [`EffectContext::child_outcome`] is the two-level answer over the same
//! child: an illegal move, a legal move with an undecided child, or a legal
//! move with a decided child:
//!
//! ```
//! use atomic_movegen::{AfterState, Board, Move, Outcome, TerminalReason};
//!
//! let board = Board::from_fen("7k/8/8/5N2/3K4/8/8/R6R w - - 99 60").unwrap();
//! let ctx = board.effect_context();
//! match ctx.child_outcome(&board, Move::from_uci("a1g1").unwrap()) {
//!     AfterState::Terminal(terminal) => {
//!         assert_eq!(terminal.outcome, Outcome::Loss);
//!         assert_eq!(terminal.reason, TerminalReason::NoMovesInCheck);
//!     }
//!     other => panic!("expected a decided child, got {other:?}"),
//! }
//! ```
//!
//! # Child summary
//!
//! [`EffectContext::post_move`] returns a [`PostMove`]: the child's placement,
//! casualties, side to move, en-passant target, castling rights and clock,
//! derived without a board write and without a rules verdict. Schedule,
//! tablebase and “insufficient material” opinions stay with the caller;
//! [`MoveEffect::casualties`] remains the effect-path summary of the same
//! fact for consumers that only need the removed pieces.
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
use crate::board::{BlastCasualties, Board, StateInfo, is_attacked_by};
use crate::position::{self, PositionView};
use crate::types::*;

/// Node-constant data for classifying moves without mutating the board.
///
/// Build once per position with [`Board::effect_context`] or
/// [`Board::effect_context_with_state`], then call [`effect`](Self::effect) or
/// [`is_legal`](Self::is_legal) for each pseudo-legal candidate. The context is
/// a snapshot: it stays valid while `board` does not change and holds no
/// reference to it.
///
/// The context deliberately stores only data the rules need — a snapshot of the
/// position's placement, side to move, en-passant target, castling rights and
/// clock, plus the “game is already over” flag read from the [`StateInfo`] it
/// was built from — never a search concept. The node-constant
/// `checkers`/`pinned` data of [`StateInfo`] is not part of the snapshot because
/// the effect path derives post-move attacks directly; callers that need pins
/// read them from [`StateInfo`].
#[derive(Debug, Clone, Copy)]
pub struct EffectContext {
    /// Placement, side to move, castling rights and clock of the context
    /// position.
    view: PositionView,
    /// `true` when either side has no commoner, i.e. the game is already over
    /// and [`Board::legal`] rejects every move.
    game_over: bool,
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

/// What an [`EffectContext`] knows about the child position after a move.
///
/// Returned by [`EffectContext::child_outcome`] and
/// [`Board::child_outcome`]. It separates the two questions that
/// [`outcome_after`](EffectContext::outcome_after) folds into one `None`:
/// *“is the move legal?”* and *“is the child decided?”*.
///
/// The terminal values are exactly the ones [`Board::terminal`] would report
/// for the child, so `Terminal(t)` is a rules verdict about the child, not about
/// the mover.
///
/// # Semver
///
/// This enum is `#[non_exhaustive]`: a new variant is a minor release, so
/// downstream `match` expressions must include a wildcard arm. Removing or
/// renaming a variant is a major release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AfterState {
    /// `m` is not legal: the child is unreachable.
    ///
    /// This covers every illegal pseudo-legal move (a self-explosion, a move
    /// that leaves the mover's commoner attacked, a castling path through an
    /// attacked square, a pinned mover, an illegal en-passant capture) as well
    /// as a board that is already over, where every move is illegal.
    Illegal,
    /// `m` is legal and the child is not decided: the game continues.
    Undecided,
    /// `m` is legal and the child is over, from the child's perspective.
    Terminal(Terminal),
}

impl AfterState {
    /// Return `true` for [`AfterState::Illegal`].
    #[inline]
    #[must_use]
    pub fn is_illegal(&self) -> bool {
        matches!(self, AfterState::Illegal)
    }

    /// Return `true` for [`AfterState::Undecided`].
    #[inline]
    #[must_use]
    pub fn is_undecided(&self) -> bool {
        matches!(self, AfterState::Undecided)
    }

    /// Return `true` for [`AfterState::Terminal`].
    #[inline]
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, AfterState::Terminal(_))
    }

    /// The child's [`Terminal`] when it is decided.
    #[inline]
    #[must_use]
    pub fn terminal(&self) -> Option<Terminal> {
        match self {
            AfterState::Terminal(t) => Some(*t),
            AfterState::Illegal | AfterState::Undecided => None,
        }
    }

    /// The child's WDL [`Outcome`] when it is decided.
    #[inline]
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        self.terminal().map(|t| t.outcome)
    }
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
        Self {
            view: PositionView::of(board),
            game_over: state.commoners_count == 0 || state.them_commoners_count == 0,
        }
    }

    /// The side to move in the context position.
    #[inline]
    #[must_use]
    pub fn side_to_move(&self) -> Color {
        self.view.side_to_move()
    }

    /// The side to move in the context position.
    #[inline(always)]
    fn us(&self) -> Color {
        self.view.side_to_move()
    }

    /// The opponent of the side to move in the context position.
    #[inline(always)]
    fn them(&self) -> Color {
        self.view.side_to_move().flip()
    }

    /// The commoners of the side to move in the context position.
    #[inline(always)]
    fn our_commoners(&self) -> Bitboard {
        self.view.commoners(self.us())
    }

    /// Classify `m` without mutating `board`.
    ///
    /// `m` must be a pseudo-legal move for this position (as produced by
    /// [`generate_pseudo_legal`](crate::movegen::generate_pseudo_legal) or
    /// accepted by [`Board::is_pseudo_legal`]). For malformed input — a
    /// [`Move::NONE`], an empty origin square, or an origin that does not hold
    /// a piece of the side to move — every field except `is_capture` is
    /// `false`/empty and `is_legal` is `false`; use [`Board::is_legal`] for
    /// untrusted single moves.
    ///
    /// This never calls [`Board::do_move`], and it calls [`Board::legal`] for
    /// no move type: castling uses the shared pass-through-square rule on the
    /// context's view.
    #[must_use]
    pub fn effect(&self, board: &Board, m: Move) -> MoveEffect {
        self.effect_and_post(board, m).0
    }

    /// [`effect`](Self::effect) and [`post_move`](Self::post_move) from one
    /// analysis pass.
    ///
    /// The two halves equal the separate queries field for field, so a consumer
    /// that wants both (for example a search that classifies a capture and then
    /// reads the child's clock) pays for the child derivation once.
    #[must_use]
    pub fn effect_and_post(&self, board: &Board, m: Move) -> (MoveEffect, Option<PostMove>) {
        let post = self.analyze(board, m);
        let effect = match &post {
            None => MoveEffect {
                is_capture: self.view.is_capture(m),
                casualties: BlastCasualties::default(),
                checks_enemy: false,
                explodes_enemy_commoner: false,
                leaves_own_commoner: false,
                is_legal: false,
            },
            Some(post) => MoveEffect {
                is_capture: post.is_capture,
                casualties: post.casualties,
                checks_enemy: self.checks_enemy(post),
                explodes_enemy_commoner: !self.their_commoners().is_empty()
                    && post.view.commoners(self.them()).is_empty(),
                leaves_own_commoner: !post.view.commoners(self.us()).is_empty(),
                is_legal: self.is_legal_from(m, post),
            },
        };
        (effect, post)
    }

    /// The child summary after `m`, derived without a board write.
    ///
    /// The returned [`PostMove`] describes the position `board.do_move(m, ..)`
    /// would build: placement, removed pieces, side to move, en-passant target,
    /// castling rights and half-move clock. It carries no rules opinion — no
    /// legality, no terminality, no material-draw shortcut; combine it with
    /// [`child_outcome`](Self::child_outcome) when the verdict is wanted too.
    ///
    /// Returns `None` for malformed input only — a [`Move::NONE`], an empty
    /// origin square, or an origin that does not hold a piece of the side to
    /// move. Legality is deliberately **not** involved: the summary of an
    /// illegal move describes the (unreachable) child exactly as `do_move`
    /// would build it.
    #[must_use]
    pub fn post_move(&self, board: &Board, m: Move) -> Option<PostMove> {
        self.analyze(board, m)
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
            Some(post) => self.is_legal_from(m, &post),
        }
    }

    /// The outcome of the child position after `m`, from the child side-to-move
    /// perspective, without building the child.
    ///
    /// # Contract
    ///
    /// `None` means **either** that `m` is not legal on this board **or** that
    /// the child is not decided (the game continues). The two cases are
    /// indistinguishable here; use [`child_outcome`](Self::child_outcome) (or
    /// [`is_legal`](Self::is_legal)) when the difference matters. For every
    /// legal `m` with a decided child the result is exactly
    /// `{ let mut b = board.clone(); b.do_move(m, &mut st); b.outcome() }`.
    ///
    /// This is `child_outcome(..).outcome()`; it never clones the board and
    /// never builds a [`MoveList`].
    #[must_use]
    pub fn outcome_after(&self, board: &Board, m: Move) -> Option<Outcome> {
        self.child_outcome(board, m).outcome()
    }

    /// The two-level child answer after `m`: illegal, undecided, or terminal.
    ///
    /// This separates the questions that
    /// [`outcome_after`](Self::outcome_after) folds into one `None`:
    ///
    /// - [`AfterState::Illegal`] — `m` is not legal here (`Board::legal` says
    ///   `false`), so the child is unreachable;
    /// - [`AfterState::Undecided`] — `m` is legal and the child continues;
    /// - [`AfterState::Terminal`] with `t` — `m` is legal and `t` is exactly what
    ///   [`Board::terminal`] reports for the child.
    ///
    /// For every **pseudo-legal** `m` the arm is exact: `Illegal` iff
    /// `!board.legal(m, &state)`, and otherwise `Terminal(t)` iff the cloned
    /// child's `terminal()` is `Some(t)`. No board is written and nothing is
    /// allocated.
    ///
    /// Malformed input ([`Move::NONE`], an empty origin square, an origin
    /// without a piece of the side to move) is [`AfterState::Illegal`].
    #[must_use]
    pub fn child_outcome(&self, board: &Board, m: Move) -> AfterState {
        if self.game_over {
            return AfterState::Illegal;
        }
        let Some(child) = self.analyze(board, m) else {
            return AfterState::Illegal;
        };
        if !self.is_legal_from(m, &child) {
            return AfterState::Illegal;
        }
        match self.terminal_from(&child) {
            Some(t) => AfterState::Terminal(t),
            None => AfterState::Undecided,
        }
    }

    /// The terminal state of the child position after `m`, from the child
    /// side-to-move perspective, without building the child.
    ///
    /// `None` means the child is not decided. For every **pseudo-legal** `m`
    /// this is exactly
    /// `{ let mut c = board.clone(); c.do_move(m, &mut st); c.terminal() }` —
    /// the same precedence (extinction → material draw → no legal moves →
    /// clock) and the same child clock, which comes from
    /// [`Board::rule50_after`].
    ///
    /// # Legality is not checked
    ///
    /// This does not reject illegal moves: a pseudo-legal move that explodes
    /// the mover's own commoner has an extinct child and is reported as such.
    /// When the move's legality is the question, use [`is_legal`](Self::is_legal)
    /// (or [`Board::is_legal`]), or [`child_outcome`](Self::child_outcome) for
    /// the two-level answer that separates *illegal* from *undecided*.
    ///
    /// Malformed input — a [`Move::NONE`] or an empty origin square — returns
    /// `None`.
    ///
    /// The child's [`StateInfo`] is populated internally (checkers, pins,
    /// commoner counts); no board is written and nothing is allocated.
    #[must_use]
    pub fn terminal_after(&self, board: &Board, m: Move) -> Option<Terminal> {
        let child = self.analyze(board, m)?;
        self.terminal_from(&child)
    }

    /// The terminal state of an already derived child.
    ///
    /// This is the single implementation behind
    /// [`terminal_after`](Self::terminal_after) and
    /// [`child_outcome`](Self::child_outcome), so the two cannot drift.
    fn terminal_from(&self, child: &PostMove) -> Option<Terminal> {
        let mut state = StateInfo::new();
        child.view.populate_state(&mut state);
        position::terminal_in_view(&child.view, &state, None)
    }

    /// The literal [`Board::is_terminal`] mirror on the child position after
    /// `m`, without building the child.
    ///
    /// For every pseudo-legal `m` this is exactly
    /// `{ let mut c = board.clone(); c.do_move(m, &mut st); c.is_terminal() }`:
    /// extinction, the 100-ply clock, or no legal move. Unlike
    /// [`terminal_after`](Self::terminal_after) it does **not** include the
    /// commoner-only material shortcut, so a K-vs-K child that
    /// [`terminal_after`](Self::terminal_after) reports as
    /// `Some(MaterialDraw)` is `false` here.
    ///
    /// Malformed input returns `None`.
    #[must_use]
    pub fn is_terminal_after(&self, board: &Board, m: Move) -> Option<bool> {
        let child = self.analyze(board, m)?;
        let view = &child.view;
        let us = view.side_to_move();
        if view.commoners(us).is_empty()
            || view.commoners(us.flip()).is_empty()
            || view.rule50() >= 100
        {
            return Some(true);
        }
        let mut state = StateInfo::new();
        view.populate_state(&mut state);
        Some(!crate::movegen::has_legal_move_in_view(view, &state))
    }

    /// Whether the child position after `m` has at least one legal move,
    /// without building the child.
    ///
    /// For every pseudo-legal `m` this is exactly
    /// `{ let mut c = board.clone(); c.do_move(m, &mut st);
    /// crate::movegen::has_legal_move(&c) }`. Malformed input returns `None`.
    #[must_use]
    pub fn has_legal_move_after(&self, board: &Board, m: Move) -> Option<bool> {
        let child = self.analyze(board, m)?;
        let mut state = StateInfo::new();
        child.view.populate_state(&mut state);
        Some(crate::movegen::has_legal_move_in_view(&child.view, &state))
    }

    /// The opponent's commoners in the context position.
    #[inline]
    fn their_commoners(&self) -> Bitboard {
        self.view.commoners(self.them())
    }

    /// `true` when the move is legal, assuming the effect path decides it.
    ///
    /// Rejects everything when the game is over, and this is the ladder of
    /// [`Board::legal`]: no own commoner may be
    /// destroyed, exploding the enemy commoner wins at once, touching commoners
    /// are immune, and otherwise the mover's commoner may not stand on an
    /// attacked square after the move and blast.
    fn is_legal_from(&self, m: Move, post: &PostMove) -> bool {
        if self.game_over {
            return false;
        }
        if m.move_type() == MoveType::Castling {
            // Pass-through-square attacks are their own rule surface; the
            // shared view implementation already answers them from bitboards.
            return position::legal_with_counts(&self.view, m);
        }

        let our_after = post.view.commoners(self.us());
        // A move may not destroy any of our commoners. In a legal position
        // there is exactly one, so this is the usual self-explosion check.
        if our_after.count() != self.our_commoners().count() {
            return false;
        }
        let their_after = post.view.commoners(self.them());
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

        let view = &post.view;
        !is_attacked_by(
            ksq,
            view.occupied(),
            view.by_type(),
            view.pieces_color(self.them()),
            self.us(),
        )
    }

    /// `true` when the move leaves the enemy commoner checked.
    ///
    /// Mirrors [`Board::checkers`] on the child: no check when either commoner
    /// is gone, no check when the commoners touch, and commoner attacks are
    /// excluded from the attacker query because an adjacent commoner touches.
    fn checks_enemy(&self, post: &PostMove) -> bool {
        let view = &post.view;
        let their_after = view.commoners(self.them());
        let our_after = view.commoners(self.us());
        if their_after.is_empty() || our_after.is_empty() {
            return false;
        }
        let ksq = their_after.lsb();
        if !(our_after & attacks::king_attacks(ksq)).is_empty() {
            return false;
        }
        is_attacked_by(
            ksq,
            view.occupied(),
            view.by_type(),
            view.pieces_color(self.us()),
            self.them(),
        )
    }

    /// Derive the child position of `m` without touching the board, or `None`
    /// for a move with no origin piece.
    fn analyze(&self, board: &Board, m: Move) -> Option<PostMove> {
        // Malformed input: no move and no origin piece. Checked before the
        // child clock, which is only defined for a real origin piece.
        if m == Move::NONE || self.view.piece_on(m.from_sq()) == NO_PIECE {
            return None;
        }
        let child = position::child_view(&self.view, m, board.rule50_after(m))?;
        Some(PostMove {
            view: child.view,
            is_capture: child.is_capture,
            casualties: child.casualties,
        })
    }
}

/// The child position of one move: placement, side to move, clock, and the
/// pieces the move removed.
///
/// Returned by [`EffectContext::post_move`] and the second half of
/// [`EffectContext::effect_and_post`]. It is a plain data value — the facts
/// `board.do_move(m, ..)` would produce — and carries no rules verdict: no
/// legality, no terminality, and no material or “insufficient material”
/// shortcut. Those stay rules questions and are answered by
/// [`EffectContext::child_outcome`], [`Board::terminal`] and the caller's own
/// policy.
///
/// Only an [`EffectContext`] builds one; a malformed move yields `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostMove {
    /// The child position, derived without a board write.
    view: PositionView,
    /// The move removes a piece (normal capture or en passant).
    is_capture: bool,
    /// The pieces the move's blast removes.
    casualties: BlastCasualties,
}

impl PostMove {
    /// The child's side to move (the opponent of the parent's).
    #[inline]
    #[must_use]
    pub fn side_to_move(&self) -> Color {
        self.view.side_to_move()
    }

    /// The child's occupied squares.
    #[inline]
    #[must_use]
    pub fn occupied(&self) -> Bitboard {
        self.view.occupied()
    }

    /// The child's pieces of color `c`.
    #[inline]
    #[must_use]
    pub fn pieces_color(&self, c: Color) -> Bitboard {
        self.view.pieces_color(c)
    }

    /// The child's pieces of type `pt`.
    #[inline]
    #[must_use]
    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard {
        self.view.pieces_pt(pt)
    }

    /// The child's pieces of color `c` and type `pt`.
    #[inline]
    #[must_use]
    pub fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard {
        self.view.pieces_color_pt(c, pt)
    }

    /// The piece standing on `sq` in the child, [`NO_PIECE`] when empty.
    #[inline]
    #[must_use]
    pub fn piece_on(&self, sq: Square) -> Piece {
        self.view.piece_on(sq)
    }

    /// Every piece the move's blast removes, including the capturer at ground
    /// zero and the en-passant victim.
    ///
    /// Identical to [`Board::blast_casualties`] for the same move.
    #[inline]
    #[must_use]
    pub fn casualties(&self) -> BlastCasualties {
        self.casualties
    }

    /// Whether the move removes a piece (normal capture or en passant).
    #[inline]
    #[must_use]
    pub fn is_capture(&self) -> bool {
        self.is_capture
    }

    /// The child's stored en-passant target, exactly as
    /// [`Board::ep_square`] reports it after `do_move(m)`.
    #[inline]
    #[must_use]
    pub fn ep_square(&self) -> Option<Square> {
        self.view.ep_square()
    }

    /// The child's castling-right bitmask.
    #[inline]
    #[must_use]
    pub fn castling_rights(&self) -> u8 {
        self.view.castling_rights()
    }

    /// The child's half-move clock.
    #[inline]
    #[must_use]
    pub fn rule50(&self) -> u16 {
        self.view.rule50()
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
                assert_eq!(
                    ctx.child_outcome(&board, m),
                    AfterState::Illegal,
                    "{fen} {m}"
                );
            }
        }
    }

    #[test]
    fn child_outcome_arms_match_legal_and_cloned_child() {
        for fen in CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let ctx = EffectContext::new(&board, &state);

            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let mut child = board.clone();
                let mut child_state = StateInfo::new();
                child.do_move(m, &mut child_state);

                let expected = if board.legal(m, &state) {
                    match child.terminal() {
                        Some(t) => AfterState::Terminal(t),
                        None => AfterState::Undecided,
                    }
                } else {
                    AfterState::Illegal
                };
                let actual = ctx.child_outcome(&board, m);
                assert_eq!(actual, expected, "{fen} {m}");
                assert_eq!(board.child_outcome(m, &state), actual, "{fen} {m}");

                // Helpers are consistent with the variant.
                assert_eq!(actual.is_illegal(), expected == AfterState::Illegal);
                assert_eq!(actual.is_undecided(), expected == AfterState::Undecided);
                assert_eq!(actual.is_terminal(), expected.is_terminal());
                assert_eq!(actual.terminal(), expected.terminal());
                assert_eq!(actual.outcome(), expected.terminal().map(|t| t.outcome));

                // `outcome_after` is the documented reduction of the arm.
                assert_eq!(ctx.outcome_after(&board, m), actual.outcome(), "{fen} {m}");
            }
        }
    }

    #[test]
    fn post_move_matches_do_move() {
        for fen in CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let ctx = EffectContext::new(&board, &state);

            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let post = ctx.post_move(&board, m).expect("pseudo-legal move");
                let mut child = board.clone();
                let mut child_state = StateInfo::new();
                child.do_move(m, &mut child_state);

                assert_eq!(post.side_to_move(), child.side_to_move(), "{fen} {m}");
                assert_eq!(post.occupied(), child.occupied(), "{fen} {m}");
                for c in [Color::White, Color::Black] {
                    assert_eq!(post.pieces_color(c), child.pieces_color(c), "{fen} {m}");
                }
                for pt in [
                    PieceType::Pawn,
                    PieceType::Knight,
                    PieceType::Bishop,
                    PieceType::Rook,
                    PieceType::Queen,
                    PieceType::Commoner,
                ] {
                    assert_eq!(post.pieces_pt(pt), child.pieces_pt(pt), "{fen} {m}");
                }
                for index in 0..64 {
                    let sq = Square::from_index(index);
                    assert_eq!(post.piece_on(sq), child.piece_on(sq), "{fen} {m} {sq:?}");
                }
                assert_eq!(post.casualties(), board.blast_casualties(m), "{fen} {m}");
                assert_eq!(post.is_capture(), board.is_capture(m), "{fen} {m}");
                assert_eq!(post.ep_square(), child.ep_square(), "{fen} {m}");
                assert_eq!(post.castling_rights(), child.castling_rights(), "{fen} {m}");
                assert_eq!(post.rule50(), child.rule50(), "{fen} {m}");

                let (effect, post_again) = ctx.effect_and_post(&board, m);
                assert_eq!(effect, ctx.effect(&board, m), "{fen} {m}");
                assert_eq!(post_again, Some(post), "{fen} {m}");
            }
        }
    }

    #[test]
    fn post_move_rejects_malformed_moves() {
        let board = Board::new();
        let ctx = board.effect_context();
        assert_eq!(ctx.post_move(&board, Move::NONE), None);
        assert_eq!(
            ctx.post_move(&board, Move::make_move(Square::E4, Square::E5)),
            None
        );
        // An enemy piece on the origin is malformed input for a child query.
        assert_eq!(
            ctx.post_move(&board, Move::make_move(Square::A7, Square::A6)),
            None
        );
        assert_eq!(ctx.effect_and_post(&board, Move::NONE).1, None);
        assert_eq!(ctx.child_outcome(&board, Move::NONE), AfterState::Illegal);
    }
}
