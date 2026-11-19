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
//! # Terminal state on the summary
//!
//! [`PostMove::terminal`] asks an already derived summary for the child's
//! terminal state, with the same precedence as
//! [`EffectContext::terminal_after`] — and it *is* that implementation, so a
//! consumer that kept the summary does not derive the child twice:
//!
//! ```
//! use atomic_movegen::{Board, Move, Outcome, TerminalReason};
//!
//! let board = Board::from_fen("7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55").unwrap();
//! let ctx = board.effect_context();
//! let m = Move::from_uci("c2c7").unwrap();
//!
//! let post = ctx.post_move(&board, m).unwrap();
//! assert_eq!(ctx.terminal_after(&board, m), post.terminal());
//! let terminal = post.terminal().expect("stalemate child");
//! assert_eq!(terminal.outcome, Outcome::Draw);
//! assert_eq!(terminal.reason, TerminalReason::Stalemate);
//! ```
//!
//! # Child key
//!
//! [`EffectContext::post_move_and_key`] returns the summary together with the
//! child's Zobrist key from one derivation, so “summary + key + terminality”
//! costs one analysis pass and one incremental key delta:
//!
//! ```
//! use atomic_movegen::{Board, Move};
//!
//! let board = Board::new();
//! let ctx = board.effect_context();
//! let m = Move::from_uci("e2e4").unwrap();
//!
//! let (post, key) = ctx.post_move_and_key(&board, m).unwrap();
//! assert_eq!(key, board.hash_after(m));
//! assert_eq!(post.ep_square(), Some(atomic_movegen::Square::E3));
//! assert_eq!(post.terminal(), None);
//! ```
//!
//! # Immediate explosion in the child
//!
//! [`EffectContext::exploding_capture_after`] answers one one-ply threat: after
//! the pseudo-legal move `m`, is there a legal capture by the child's side to
//! move whose blast removes the **commoner of the side that played `m`**? The
//! result is the first such capture in the child's pseudo-legal generation
//! order, or `None` when no such capture exists (including a decided child, a
//! malformed `m`, or an exploded target commoner). It is a child query, not a
//! field of [`MoveClassification`]: the move that was just played is not the
//! move being described.
//!
//! ```
//! use atomic_movegen::types::MoveType;
//! use atomic_movegen::{Board, Move, Square};
//!
//! // After the double push, Black's en-passant capture on e3 explodes the
//! // white commoner on f4; the black commoner on a8 is far away.
//! let board = Board::from_fen("k7/8/8/8/3p1K2/8/4P3/8 w - - 0 1").unwrap();
//! let ctx = board.effect_context();
//! let capture = ctx
//!     .exploding_capture_after(&board, Move::from_uci("e2e4").unwrap())
//!     .expect("the child can explode the mover's commoner");
//! assert_eq!((capture.from_sq(), capture.to_sq()), (Square::D4, Square::E3));
//! assert_eq!(capture.move_type(), MoveType::EnPassant);
//!
//! // From the initial position no black capture comes near e1.
//! let board = Board::new();
//! let ctx = board.effect_context();
//! assert_eq!(
//!     ctx.exploding_capture_after(&board, Move::from_uci("e2e4").unwrap()),
//!     None
//! );
//! ```
//!
//! # Effect
//!
//! [`EffectContext::effect`] is the full answer: the classification plus the
//! legality verdict.
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
//!
//! # Classification without the legality ladder
//!
//! [`EffectContext::classification`] returns the rules fields of a move as a
//! [`MoveClassification`] and stops there: no `is_attacked_by` query for the
//! mover's commoner is run. A consumer that classified the moves it got from
//! [`generate_legal_with_state`](crate::movegen::generate_legal_with_state) has
//! already paid for the verdict, so [`effect`](EffectContext::effect) would
//! repeat it.
//!
//! ```
//! use atomic_movegen::{Board, MoveList};
//! use atomic_movegen::movegen::generate_legal;
//!
//! let board = Board::from_fen("4R1k1/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
//! let ctx = board.effect_context();
//! let mut legal = MoveList::new();
//! generate_legal(&board, &mut legal);
//!
//! for &m in legal.as_slice() {
//!     assert_eq!(ctx.classification(&board, m), ctx.effect(&board, m).classification());
//! }
//! ```
//!

use crate::attacks;
use crate::board::{
    BK_CASTLE, BQ_CASTLE, BlastCasualties, Board, Counts, StateInfo, WK_CASTLE, WQ_CASTLE,
    castling_rights_after, castling_squares, is_attacked_by, piece_key,
};
use crate::position::{self, PositionView};
use crate::types::*;

/// Node-constant data for classifying moves without mutating the board.
///
/// Build once per position with [`Board::effect_context`] or
/// [`Board::effect_context_with_state`], then call [`effect`](Self::effect),
/// [`classification`](Self::classification) or [`is_legal`](Self::is_legal)
/// for each pseudo-legal candidate. The context is
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

/// The rules classification of one pseudo-legal move, without a legality
/// verdict.
///
/// Equal field for field to the corresponding [`MoveEffect`] fields for every
/// pseudo-legal move. Returned by [`EffectContext::classification`] for callers
/// that already know the move is legal (for example because it came out of
/// [`generate_legal_with_state`](crate::movegen::generate_legal_with_state)) and
/// therefore have no use for a second legality query.
///
/// # Semantics
///
/// A classification is a statement about the *rules*, not about reachability:
/// it says what the move and its blast would do, not whether the move is
/// allowed. A move with `leaves_own_commoner == false` is a self-explosion and
/// can never be legal, but every other field can be true or false for both
/// legal and illegal moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveClassification {
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
}

/// The rules effects of one pseudo-legal move.
///
/// Returned by [`EffectContext::effect`]. Every field is derivable from the
/// atomic rules alone; nothing here encodes a search design.
///
/// The five rules fields are also available on their own as a
/// [`MoveClassification`], either with
/// [`EffectContext::classification`] (which does not run the legality ladder)
/// or with [`MoveEffect::classification`].
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

impl MoveEffect {
    /// The classification part of this effect: everything except `is_legal`.
    #[inline]
    #[must_use]
    pub fn classification(&self) -> MoveClassification {
        MoveClassification {
            is_capture: self.is_capture,
            casualties: self.casualties,
            checks_enemy: self.checks_enemy,
            explodes_enemy_commoner: self.explodes_enemy_commoner,
            leaves_own_commoner: self.leaves_own_commoner,
        }
    }
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
    /// Only the two commoner counts are read, for the `game_over` snapshot, so
    /// [`Board::populate_counts`](crate::Board::populate_counts) is sufficient
    /// for this constructor. The child/effect queries derive their own child
    /// placement and do not read `checkers` or `pinned`. A stale count pair
    /// changes which positions count as already over, exactly as it does for
    /// [`Board::legal`](crate::Board::legal).
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
        let (classification, is_legal) = match &post {
            None => (self.classify_malformed(m), false),
            Some(post) => (self.classify(post), self.is_legal_from(m, post)),
        };
        let effect = MoveEffect {
            is_capture: classification.is_capture,
            casualties: classification.casualties,
            checks_enemy: classification.checks_enemy,
            explodes_enemy_commoner: classification.explodes_enemy_commoner,
            leaves_own_commoner: classification.leaves_own_commoner,
            is_legal,
        };
        (effect, post)
    }

    /// Classify `m` without running the legality ladder.
    ///
    /// Same fields as [`effect`](Self::effect) minus `is_legal`, derived in the
    /// same analysis pass: no `is_attacked_by` query for the *mover's*
    /// commoner is run. Use [`effect`](Self::effect) or
    /// [`is_legal`](Self::is_legal) when the verdict itself is needed.
    ///
    /// Malformed input ([`Move::NONE`], an empty origin square, an origin
    /// without a piece of the side to move) yields all-`false`/empty fields,
    /// exactly like the malformed branch of [`effect`](Self::effect).
    ///
    /// The classification does not imply legality: a pseudo-legal move that
    /// loses the mover's commoner is classified (`leaves_own_commoner ==
    /// false`) and is illegal. Call this only for moves whose legality is
    /// already known, e.g. moves taken from
    /// [`generate_legal_with_state`](crate::movegen::generate_legal_with_state).
    #[must_use]
    pub fn classification(&self, board: &Board, m: Move) -> MoveClassification {
        match self.analyze(board, m) {
            None => self.classify_malformed(m),
            Some(post) => self.classify(&post),
        }
    }

    /// The rules fields of `m` for an already derived child.
    ///
    /// The single derivation behind [`classification`](Self::classification)
    /// and [`effect`](Self::effect), so the two can never disagree.
    fn classify(&self, post: &PostMove) -> MoveClassification {
        MoveClassification {
            is_capture: post.is_capture,
            casualties: post.casualties,
            checks_enemy: self.checks_enemy(post),
            explodes_enemy_commoner: !self.their_commoners().is_empty()
                && post.view.commoners(self.them()).is_empty(),
            leaves_own_commoner: !post.view.commoners(self.us()).is_empty(),
        }
    }

    /// The documented malformed-input classification: capture flag from the
    /// parent view, everything else empty.
    fn classify_malformed(&self, m: Move) -> MoveClassification {
        MoveClassification {
            is_capture: self.view.is_capture(m),
            casualties: BlastCasualties::default(),
            checks_enemy: false,
            explodes_enemy_commoner: false,
            leaves_own_commoner: false,
        }
    }

    /// The child summary after `m`, derived without a board write.
    ///
    /// The returned [`PostMove`] describes the position `board.do_move(m, ..)`
    /// would build: placement, removed pieces, side to move, en-passant target,
    /// castling rights and half-move clock. It carries no rules opinion — no
    /// legality, no terminality, no material-draw shortcut; ask the summary
    /// itself with [`PostMove::terminal`], or ask for the key with
    /// [`post_move_and_key`](Self::post_move_and_key).
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

    /// The child summary and the child's Zobrist key, from one derivation.
    ///
    /// This is [`post_move_and_key_with`](Self::post_move_and_key_with) with
    /// [`EnPassantMode::Always`]; the key is exactly [`Board::hash_after`] and
    /// exactly the [`hash`](Board::hash) of a cloned board after `do_move(m)`.
    /// The stability contract on [`Board::hash`] and
    /// [`ZOBRIST_VERSION`](crate::ZOBRIST_VERSION) applies unchanged. Combined
    /// with [`PostMove::terminal`] and [`PostMove::rule50`], one derivation
    /// answers "summary + key + terminality".
    ///
    /// Returns `None` for malformed input only ([`Move::NONE`], an empty origin
    /// square, an origin without a piece of the side to move), like
    /// [`post_move`](Self::post_move). Defined for illegal moves: the key is the
    /// one a `do_move` child would carry.
    ///
    /// ```
    /// use atomic_movegen::{Board, Move};
    ///
    /// let board = Board::from_fen("8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1").unwrap();
    /// let ctx = board.effect_context();
    /// let m = Move::from_uci("e4e3").unwrap();
    ///
    /// let (post, key) = ctx.post_move_and_key(&board, m).unwrap();
    /// assert_eq!(key, board.hash_after(m));
    /// assert_eq!(post.rule50(), 0);
    /// assert_eq!(post.terminal(), None);
    /// ```
    #[must_use]
    pub fn post_move_and_key(&self, board: &Board, m: Move) -> Option<(PostMove, u64)> {
        self.post_move_and_key_with(board, m, EnPassantMode::Always)
    }

    /// The child summary and the child's Zobrist key under `mode`, from one
    /// derivation.
    ///
    /// The key is exactly [`Board::hash_after_with`] and exactly the
    /// [`hash_with`](Board::hash_with) of a cloned board after `do_move(m)`.
    /// With [`EnPassantMode::Always`] this is the existing
    /// [`post_move_and_key`](Self::post_move_and_key), unchanged.
    ///
    /// [`EnPassantMode::Legal`] removes the child's en-passant file key when no
    /// fully legal en-passant capture exists in the child. The legal-EP
    /// question is answered from the already-derived child view, so a
    /// non-double-push move does no extra work and a double push is not cloned.
    /// The `Legal` policy is a canonicalization layered on the stable base
    /// keys: no key table or [`ZOBRIST_VERSION`](crate::ZOBRIST_VERSION)
    /// change. The cost contract is: non-double pushes never call the legal-EP
    /// helper; double pushes derive the child once and may ask one view-side
    /// legal-EP question; `Always` is the unchanged fast path.
    ///
    /// Returns `None` for malformed input only ([`Move::NONE`], an empty origin
    /// square, an origin without a piece of the side to move), like
    /// [`post_move`](Self::post_move). Defined for illegal moves: the key is the
    /// one a `do_move` child would carry.
    ///
    /// ```
    /// use atomic_movegen::{Board, EnPassantMode, Move, Square};
    ///
    /// let board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
    ///     .unwrap();
    /// let ctx = board.effect_context();
    /// let m = Move::from_uci("e2e4").unwrap();
    ///
    /// let (post, always) = ctx
    ///     .post_move_and_key_with(&board, m, EnPassantMode::Always)
    ///     .unwrap();
    /// let (_, legal) = ctx
    ///     .post_move_and_key_with(&board, m, EnPassantMode::Legal)
    ///     .unwrap();
    /// assert_eq!(post.ep_square(), Some(Square::E3));
    /// assert_ne!(always, legal); // no black pawn can capture on e3
    /// assert_eq!(always, board.hash_after_with(m, EnPassantMode::Always));
    /// assert_eq!(legal, board.hash_after_with(m, EnPassantMode::Legal));
    /// ```
    #[must_use]
    pub fn post_move_and_key_with(
        &self,
        board: &Board,
        m: Move,
        mode: EnPassantMode,
    ) -> Option<(PostMove, u64)> {
        let post = self.analyze(board, m)?;
        Some((post, child_key_with(board, &self.view, &post, m, mode)))
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
        match child.terminal() {
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
    /// This is `post_move(board, m).and_then(|post| post.terminal())`: the
    /// child's commoner counts are read from the placement view, and the
    /// existence question is answered by the positive screen whenever that
    /// screen can prove a legal move — so the child's attacker query and its
    /// pins are derived only when the precedence actually needs them. No board
    /// is written, nothing is allocated, and no [`StateInfo`] is populated on
    /// that path.
    #[must_use]
    pub fn terminal_after(&self, board: &Board, m: Move) -> Option<Terminal> {
        self.analyze(board, m)?.terminal()
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
        Some(!crate::movegen::has_legal_move_in_view_unpopulated(view))
    }

    /// Whether the child position after `m` has at least one legal move,
    /// without building the child.
    ///
    /// For every pseudo-legal `m` this is exactly
    /// `{ let mut c = board.clone(); c.do_move(m, &mut st);
    /// crate::movegen::has_legal_move(&c) }`. Malformed input returns `None`.
    /// Like the board query, this is a two-step answer: a quiet move the
    /// existence screen can prove legal decides `true` immediately, and only
    /// the remaining children (in check, captures only, commoner moves only)
    /// populate a [`StateInfo`] for the exact ladder.
    #[must_use]
    pub fn has_legal_move_after(&self, board: &Board, m: Move) -> Option<bool> {
        let child = self.analyze(board, m)?;
        Some(crate::movegen::has_legal_move_in_view_unpopulated(
            &child.view,
        ))
    }

    /// The first legal capture in the child after `m` whose blast removes the
    /// commoner of the side that played `m`.
    ///
    /// The returned move belongs to the child's side to move. It is the first
    /// such capture in the child's pseudo-legal generation order; this is an
    /// existence/one-witness query, not a "best capture" query.
    ///
    /// Returns `None` when:
    ///
    /// - `m` is malformed ([`Move::NONE`], an empty origin square, or an origin
    ///   that does not hold a piece of the parent side to move);
    /// - the child has no commoner for the side that played `m` (for example
    ///   `m` exploded it);
    /// - the child has no commoner for the child side to move (the child is
    ///   already decided);
    /// - no legal capture removes the target commoner.
    ///
    /// Defined for **pseudo-legal** `m`, including illegal `m`: the unreachable
    /// child is described exactly as `do_move` would build it. The parent board
    /// is never written and no [`StateInfo`] is populated.
    ///
    /// Touching commoners do not make the child immune to a blast: the capture
    /// must be legal, so a blast that destroys the child side's own commoner is
    /// rejected; a blast that removes only the target commoner is a win and may
    /// be returned.
    #[must_use]
    pub fn exploding_capture_after(&self, board: &Board, m: Move) -> Option<Move> {
        let child = self.analyze(board, m)?;
        let target = child.view.commoners(self.us());
        exploding_capture_in_view(&child.view, target)
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
/// shortcut. Those stay rules questions: terminality is asked *about* the
/// summary with [`PostMove::terminal`], legality with
/// [`EffectContext::is_legal`], and the caller's own policy
/// (tablebase, schedule) is the caller's.
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

    /// The terminal state of this child, from the child's side to move.
    ///
    /// `None` when the child is not decided. Equal to
    /// `{ let mut c = board.clone(); c.do_move(m, &mut st); c.terminal() }` for
    /// the move that produced this summary, with the same precedence as
    /// [`EffectContext::terminal_after`] (extinction → commoner-only material
    /// draw → no legal moves → clock; mate outranks the clock).
    ///
    /// This is a rules verdict about the child, not a field of the summary: the
    /// summary itself still carries no opinion. Nothing is written and nothing
    /// is allocated: the two commoner counts and the attacker query come from
    /// the placement view, and the existence question (asked only when the
    /// precedence reaches it) reuses that same attacker query.
    ///
    /// # Legality is not involved
    ///
    /// The summary of a pseudo-legal move that explodes the mover's own
    /// commoner describes the (unreachable) extinct child and is reported as
    /// such, exactly like [`EffectContext::terminal_after`]. Use
    /// [`EffectContext::child_outcome`] when *illegal* and *undecided* must be
    /// distinguishable.
    ///
    /// ```
    /// use atomic_movegen::{Board, Move, Outcome, TerminalReason};
    ///
    /// let board = Board::from_fen("8/8/8/8/8/8/8/3qK3 w - - 0 1").unwrap();
    /// let ctx = board.effect_context();
    /// // `e1d1` walks into the blast of the queen: the child is extinct, and the
    /// // move is illegal — the summary still reports the child it would build.
    /// let post = ctx.post_move(&board, Move::from_uci("e1d1").unwrap()).unwrap();
    /// let terminal = post.terminal().expect("extinct child");
    /// assert_eq!(terminal.outcome, Outcome::Loss);
    /// assert_eq!(terminal.reason, TerminalReason::Extinction);
    /// ```
    #[must_use]
    pub fn terminal(&self) -> Option<Terminal> {
        crate::position::terminal_in_view_light(&self.view)
    }
}

/// The first legal capture in `view` whose blast removes one of `target`.
///
/// `view` is the child position and `target` the commoners of the side that
/// played the move that produced it. Both commoners must exist in `view`: a
/// side without a commoner has no legal moves, and [`legal_with_counts`](crate::position::legal_with_counts)
/// expects the side to move's commoner to be live.
///
/// The destination-zone test is a cheap prefilter only; the decisive test is
/// the blast-casualty intersection with `target`.
fn exploding_capture_in_view(view: &PositionView, target: Bitboard) -> Option<Move> {
    let us = view.side_to_move();
    let them = us.flip();
    if view.commoners(us).is_empty() || view.commoners(them).is_empty() {
        return None;
    }
    if target.is_empty() {
        return None;
    }
    let target_sq = target.lsb();
    let target_zone = target | attacks::king_attacks(target_sq);

    let mut found = None;
    let _ = crate::movegen::for_each_pseudo_legal_in_view::<()>(view, &mut |c| {
        if !view.is_capture(c) {
            return std::ops::ControlFlow::Continue(());
        }
        if !target_zone.contains(c.to_sq() as usize) {
            return std::ops::ControlFlow::Continue(());
        }
        if (view.blast_casualties(c).all() & target).is_empty() {
            return std::ops::ControlFlow::Continue(());
        }
        if crate::position::legal_with_counts(view, c) {
            found = Some(c);
            return std::ops::ControlFlow::Break(());
        }
        std::ops::ControlFlow::Continue(())
    });
    found
}

/// The engine-facing facts of a child position.
///
/// Equal, field for field, to a cloned child board after `do_move(m)` for the
/// same move: `occupied == child.occupied()`, `key == child.hash_with(mode)`,
/// `rule50 == child.rule50()`, and `capture` is the move's capture flag.
///
/// This is intentionally smaller than [`PostMove`]: it carries no placement,
/// casualties, castling rights, or terminality. Use [`PostMove`] (or a built
/// child) for those.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChildEffect {
    /// The child's occupied squares.
    pub occupied: Bitboard,
    /// The child's Zobrist key under the requested en-passant policy.
    pub key: u64,
    /// The child's half-move clock.
    pub rule50: u16,
    /// Whether the move captures (normal capture or en passant).
    pub capture: bool,
}

impl Board {
    /// The child effect of `m`, without building a child board or a
    /// [`PostMove`].
    ///
    /// Returns `None` for malformed input only ([`Move::NONE`], an empty
    /// origin, or an origin without a piece of the side to move), like
    /// [`EffectContext::post_move`]. Defined for well-formed illegal moves:
    /// the returned facts describe the child `do_move` would build.
    ///
    /// `counts` must be current for this board. It is used only to skip the
    /// legal-EP child view when either side has no commoner. `mode` selects
    /// the same `Always`/`Legal` key policy as
    /// [`Board::hash_after_with`](Board::hash_after_with).
    ///
    /// This is allocation-free and does not write the board.
    #[must_use]
    pub fn child_effect(
        &self,
        m: Move,
        counts: Counts,
        mode: EnPassantMode,
    ) -> Option<ChildEffect> {
        self.child_effect_with_occupancy(m, self.occupied(), counts, mode)
    }

    /// [`child_effect`](Self::child_effect) with a caller-supplied parent
    /// occupancy.
    ///
    /// `occupied` must equal `self.occupied()` at the time of the call. The
    /// intended producer is
    /// [`LegalityContext::occupied`](crate::LegalityContext::occupied), so a
    /// candidate loop does not recompute the parent occupancy.
    ///
    /// This is allocation-free and does not write the board.
    #[must_use]
    pub fn child_effect_with_occupancy(
        &self,
        m: Move,
        occupied: Bitboard,
        counts: Counts,
        mode: EnPassantMode,
    ) -> Option<ChildEffect> {
        if m == Move::NONE {
            return None;
        }
        let view = self.position_view();
        let us = view.side_to_move();
        let from = m.from_sq();
        let to = m.to_sq();
        let piece = self.piece_on(from);
        if piece == NO_PIECE || piece.color() != Some(us) {
            return None;
        }
        let moving_type = piece.type_of()?;
        let move_type = m.move_type();
        let castling = move_type == MoveType::Castling;
        let is_capture = move_type == MoveType::EnPassant
            || (move_type != MoveType::Castling && occupied.contains(to as usize));

        // Quiet moves have no blast; only a capture pays for the casualty set.
        let casualties = if is_capture {
            self.blast_casualties(m)
        } else {
            BlastCasualties::default()
        };

        let child_ep =
            if !castling && moving_type == PieceType::Pawn && (to as i8 - from as i8).abs() == 16 {
                Some(position::double_push_ep(us, from))
            } else {
                None
            };

        let child_rights = if castling {
            view.castling_rights()
                & match us {
                    Color::White => !(WK_CASTLE | WQ_CASTLE),
                    Color::Black => !(BK_CASTLE | BQ_CASTLE),
                }
        } else {
            let post_blast = if is_capture {
                let removed = Bitboard::square_bb(from) | casualties.all();
                let after = |sq: Square| {
                    if removed.contains(sq as usize) {
                        NO_PIECE
                    } else {
                        self.piece_on(sq)
                    }
                };
                [
                    after(Square::H1),
                    after(Square::A1),
                    after(Square::E1),
                    after(Square::H8),
                    after(Square::A8),
                    after(Square::E8),
                ]
            } else {
                // Ignored by the helper when there is no capture.
                [NO_PIECE; 6]
            };
            castling_rights_after(view.castling_rights(), from, to, is_capture, &post_blast)
        };

        let rule50 = if castling || (moving_type != PieceType::Pawn && !is_capture) {
            view.rule50().saturating_add(1)
        } else {
            0
        };

        let child_occupied = if castling {
            let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
            (occupied & !(Bitboard::square_bb(kfrom) | Bitboard::square_bb(rfrom)))
                | Bitboard::square_bb(kto)
                | Bitboard::square_bb(rto)
        } else if is_capture {
            occupied & !(Bitboard::square_bb(from) | casualties.all())
        } else {
            (occupied & !Bitboard::square_bb(from)) | Bitboard::square_bb(to)
        };

        let mut key = child_key_from_parts(
            self,
            view,
            m,
            is_capture,
            casualties,
            child_rights,
            child_ep,
        );

        if mode == EnPassantMode::Legal {
            if let Some(ep) = child_ep {
                // `legal_ep_square_in_view` is necessarily `None` when either
                // side has no commoner; the counts short-circuit avoids the
                // child view in that case and still removes the ep-file key.
                let legal_ep = if counts.commoners() != 0 && counts.them_commoners() != 0 {
                    position::child_view(view, m, rule50)
                        .and_then(|child| position::legal_ep_square_in_view(&child.view))
                } else {
                    None
                };
                if legal_ep.is_none() {
                    key ^= crate::zobrist::ZOBRIST.ep[file_of(ep) as usize];
                }
            }
        }

        Some(ChildEffect {
            occupied: child_occupied,
            key,
            rule50,
            capture: is_capture,
        })
    }
}

/// The Zobrist key of `post`, `board` being the parent position.
///
/// [`Board::hash`] is the XOR of the piece keys, the side key, the castling key
/// of the current rights, and the en-passant file key. A move changes the piece
/// on the origin, on the destination, and on every square its blast (or its
/// en-passant victim) empties — nothing else — plus side, rights, and ep, so the
/// child key is the parent key XOR those deltas:
///
/// - the side key: always flipped, so always XORed;
/// - every square in `{from} ∪ casualties` (plus the rook's origin for
///   castling): XOR out the parent's piece key ([`ZOBRIST`](crate::zobrist::ZOBRIST)
///   index 0 is zero, so an empty square is a no-op);
/// - every square the move fills — `{to}`, or the commoner's and rook's castling
///   destinations — XOR in the child's piece key (empty at a ground-zero
///   square, where the capturer dies);
/// - the castling-rights key, when the rights changed;
/// - the en-passant file key of the old target out and of the new target in
///   (no target, no key: `Board::hash` only adds an ep key for a stored
///   square, so `ZOBRIST.ep[8]` — which the generator fills but the hash never
///   uses — must not be XORed).
///
/// Promotions need no special case: `from` loses the pawn and `to` gains
/// `post.piece_on(to)`.
///
/// The *parent's* contents are read from the board (an array read), not from
/// `parent.piece_on` (a scan over the per-type bitboards): the emptied-square
/// lookup is on the hot path and the two are equal by construction — the
/// context's view is a snapshot of this board.
fn child_key(board: &Board, parent: &PositionView, post: &PostMove, m: Move) -> u64 {
    child_key_from_parts(
        board,
        parent,
        m,
        post.is_capture(),
        post.casualties(),
        post.castling_rights(),
        post.ep_square(),
    )
}

/// The incremental child-key arithmetic shared by the [`PostMove`] path and
/// [`Board::child_effect`].
///
/// `parent` is the parent placement, `is_capture` includes en passant,
/// `casualties` is the move's blast-removal set (empty for quiet moves), and
/// the child rights/ep values are the transition results. The parent's
/// contents are read from the board (an array read), not from
/// `parent.piece_on` (a scan over the per-type bitboards): the emptied-square
/// lookup is on the hot path and the two are equal by construction — the
/// parent view is a snapshot of this board.
fn child_key_from_parts(
    board: &Board,
    parent: &PositionView,
    m: Move,
    is_capture: bool,
    casualties: BlastCasualties,
    child_castling_rights: u8,
    child_ep: Option<Square>,
) -> u64 {
    let us = parent.side_to_move();
    let from = m.from_sq();
    let to = m.to_sq();

    let mut h = board.hash() ^ crate::zobrist::ZOBRIST.side;

    // Every square the move empties. `to` needs no separate mention for a
    // capture: ground zero is part of the casualties. For a quiet move nothing
    // sits on `to` (its key is the all-zero `NO_PIECE` key), and for castling
    // `to` is the rook's origin square — the only non-capturing case where
    // `to` holds a piece.
    let castling = m.move_type() == MoveType::Castling;
    if is_capture || castling {
        let mut emptied = Bitboard::square_bb(from) | casualties.all();
        if castling {
            emptied = emptied | Bitboard::square_bb(to);
        }
        while !emptied.is_empty() {
            let sq = emptied.pop_lsb();
            h ^= piece_key(sq, board.piece_on(sq));
        }
    } else {
        // The common case: a quiet move empties exactly its origin square.
        h ^= piece_key(from, board.piece_on(from));
    }

    // The squares the move fills, and the piece it puts there: a castling move
    // moves a commoner and a rook, a capture leaves ground zero empty (the
    // capturer dies there), a promotion places the promoted piece, and any
    // other move relocates the mover.
    if castling {
        let (_, kto, _, rto) = castling_squares(us, to > from);
        h ^= piece_key(kto, make_piece(us, PieceType::Commoner));
        h ^= piece_key(rto, make_piece(us, PieceType::Rook));
    } else {
        let placed = if is_capture {
            NO_PIECE
        } else if m.move_type() == MoveType::Promotion {
            make_piece(us, m.promotion_type())
        } else {
            board.piece_on(from)
        };
        h ^= piece_key(to, placed);
    }

    let rights = parent.castling_rights();
    if rights != child_castling_rights {
        h ^= crate::zobrist::ZOBRIST.castling[rights as usize];
        h ^= crate::zobrist::ZOBRIST.castling[child_castling_rights as usize];
    }

    let ep_key = |sq: Option<Square>| match sq {
        Some(sq) => crate::zobrist::ZOBRIST.ep[file_of(sq) as usize],
        None => 0,
    };
    h ^= ep_key(parent.ep_square()) ^ ep_key(child_ep);

    h
}

/// The child key under `mode`: the incremental [`child_key`] plus the optional
/// `Legal` en-passant canonicalization.
///
/// The existing incremental delta already XORs the child ep-file key when the
/// child stores an ep target. `EnPassantMode::Legal` removes that same key when
/// the view-side legal-EP test says there is no legal capture in the child.
fn child_key_with(
    board: &Board,
    parent: &PositionView,
    post: &PostMove,
    m: Move,
    mode: EnPassantMode,
) -> u64 {
    let always = child_key(board, parent, post, m);
    match mode {
        EnPassantMode::Always => always,
        EnPassantMode::Legal => match post.ep_square() {
            Some(ep) if crate::position::legal_ep_square_in_view(&post.view).is_none() => {
                always ^ crate::zobrist::ZOBRIST.ep[file_of(ep) as usize]
            }
            _ => always,
        },
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
    fn classification_matches_effect() {
        for fen in CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let ctx = EffectContext::new(&board, &state);

            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let classification = ctx.classification(&board, m);
                let effect = ctx.effect(&board, m);
                assert_eq!(classification, effect.classification(), "{fen} {m}");
                assert_eq!(classification.is_capture, effect.is_capture, "{fen} {m}");
                assert_eq!(classification.casualties, effect.casualties, "{fen} {m}");
                assert_eq!(
                    classification.checks_enemy, effect.checks_enemy,
                    "{fen} {m}"
                );
                assert_eq!(
                    classification.explodes_enemy_commoner, effect.explodes_enemy_commoner,
                    "{fen} {m}"
                );
                assert_eq!(
                    classification.leaves_own_commoner, effect.leaves_own_commoner,
                    "{fen} {m}"
                );
                assert_eq!(classification.is_capture, board.is_capture(m), "{fen} {m}");
                assert_eq!(
                    classification.casualties,
                    board.blast_casualties(m),
                    "{fen} {m}"
                );
            }

            // The malformed branch of both entry points agrees as well.
            assert_eq!(
                ctx.classification(&board, Move::NONE),
                ctx.effect(&board, Move::NONE).classification(),
                "{fen}"
            );
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

    #[test]
    fn post_move_terminal_matches_terminal_after() {
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

                assert_eq!(post.terminal(), child.terminal(), "{fen} {m}");
                assert_eq!(post.terminal(), ctx.terminal_after(&board, m), "{fen} {m}");
                assert_eq!(
                    ctx.terminal_after(&board, m),
                    ctx.post_move(&board, m).and_then(|p| p.terminal()),
                    "{fen} {m}"
                );
                assert_eq!(
                    ctx.child_outcome(&board, m).terminal(),
                    if ctx.is_legal(&board, m) {
                        post.terminal()
                    } else {
                        None
                    },
                    "{fen} {m}"
                );
            }
        }
    }

    #[test]
    fn child_key_matches_hash_after() {
        for fen in CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let ctx = EffectContext::new(&board, &state);

            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let (post, key) = ctx.post_move_and_key(&board, m).expect("pseudo-legal move");
                let mut child = board.clone();
                let mut child_state = StateInfo::new();
                child.do_move(m, &mut child_state);

                assert_eq!(key, board.hash_after(m), "{fen} {m}");
                assert_eq!(key, child.hash(), "{fen} {m}");
                assert_eq!(
                    (key, post.rule50()),
                    board.hash_and_rule50_after(m),
                    "{fen} {m}"
                );
                assert_eq!(Some(post), ctx.post_move(&board, m), "{fen} {m}");
            }
        }
    }

    #[test]
    fn post_move_and_key_rejects_malformed_moves() {
        let board = Board::new();
        let ctx = board.effect_context();
        for mode in [EnPassantMode::Always, EnPassantMode::Legal] {
            assert_eq!(ctx.post_move_and_key_with(&board, Move::NONE, mode), None);
            assert_eq!(
                ctx.post_move_and_key_with(&board, Move::make_move(Square::E4, Square::E5), mode),
                None
            );
            assert_eq!(
                ctx.post_move_and_key_with(&board, Move::make_move(Square::A7, Square::A6), mode),
                None
            );
        }
    }

    /// The explicit en-passant key rows: a double push that leaves a legal
    /// capture, one whose capture is impossible, two whose capture is illegal
    /// because of a pin or the mover's own blast, an ep capture, castling and a
    /// promotion.
    #[test]
    fn post_move_and_key_with_matches_cloned_child() {
        let pseudo = |board: &Board, uci: &str| {
            board.move_from_uci(uci).unwrap_or_else(|| {
                let parsed = Move::from_uci(uci).expect("UCI must parse");
                let mut pseudo = MoveList::new();
                generate_pseudo_legal(board, &mut pseudo);
                pseudo
                    .as_slice()
                    .iter()
                    .copied()
                    .find(|m| m.from_sq() == parsed.from_sq() && m.to_sq() == parsed.to_sq())
                    .unwrap_or_else(|| panic!("{uci} is not pseudo-legal"))
            })
        };
        let cases: &[(&str, &str, &str)] = &[
            (
                "double push with a legal ep capture",
                "4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1",
                "e2e4",
            ),
            (
                "double push without an adjacent enemy pawn",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                "e2e4",
            ),
            (
                "double push whose ep capture is pinned",
                "4r2k/3p4/8/4P3/8/8/8/4K3 b - - 0 1",
                "d7d5",
            ),
            (
                "double push whose ep capture self-explodes",
                "7k/3pK3/8/4P3/8/8/8/8 b - - 0 1",
                "d7d5",
            ),
            (
                "en-passant capture",
                "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
                "e4d3",
            ),
            ("castling", "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1g1"),
            (
                "promotion",
                "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
                "b7b8q",
            ),
        ];

        for &(name, fen, uci) in cases {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let ctx = board.effect_context();
            let m = pseudo(&board, uci);
            let mut child = board.clone();
            let mut state = StateInfo::new();
            child.do_move(m, &mut state);

            let (always_post, always) = ctx
                .post_move_and_key_with(&board, m, EnPassantMode::Always)
                .unwrap_or_else(|| panic!("{name}: keyed summary"));
            let (legal_post, legal) = ctx
                .post_move_and_key_with(&board, m, EnPassantMode::Legal)
                .unwrap_or_else(|| panic!("{name}: keyed summary"));
            assert_eq!(always_post, legal_post, "{name}: summary");
            assert_eq!(always_post.rule50(), child.rule50(), "{name}: clock");
            assert_eq!(always, child.hash_with(EnPassantMode::Always), "{name}");
            assert_eq!(legal, child.hash_with(EnPassantMode::Legal), "{name}");
            assert_eq!(
                always,
                board.hash_after_with(m, EnPassantMode::Always),
                "{name}"
            );
            assert_eq!(
                legal,
                board.hash_after_with(m, EnPassantMode::Legal),
                "{name}"
            );
            assert_eq!(
                ctx.post_move_and_key_with(&board, m, EnPassantMode::Always),
                ctx.post_move_and_key(&board, m),
                "{name}: Always is the existing query"
            );

            let view_ep = crate::position::legal_ep_square_in_view(&always_post.view);
            assert_eq!(
                view_ep,
                child.legal_ep_square(),
                "{name}: view-side legal ep"
            );
            if let Some(ep) = always_post.ep_square().filter(|_| view_ep.is_none()) {
                assert_eq!(
                    always ^ legal,
                    crate::zobrist::ZOBRIST.ep[file_of(ep) as usize],
                    "{name}: Legal removes exactly the ep file key"
                );
            } else {
                assert_eq!(always, legal, "{name}: modes agree");
            }
        }
    }

    /// The child-view fixtures of the explosion query: each row builds the
    /// *child* position, so the target is the commoner of the side that is not
    /// to move.
    #[test]
    fn exploding_capture_in_view_fixtures() {
        let cases: &[(&str, &str, Option<&str>)] = &[
            (
                "direct commoner capture",
                "4k3/4r3/8/8/8/8/8/4K3 b - - 0 1",
                Some("e7e1"),
            ),
            (
                "capture beside the commoner",
                "5r2/8/8/8/3kK3/5R2/8/8 b - - 0 1",
                Some("f8f3"),
            ),
            (
                "touching commoners, only a self-exploding candidate",
                "8/8/8/8/3kK3/3q4/8/8 b - - 0 1",
                None,
            ),
            (
                "a pawn is not a target",
                "5r2/8/8/8/3k4/5P1K/8/8 b - - 0 1",
                None,
            ),
        ];

        for &(name, fen, expected) in cases {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let view = PositionView::of(&board);
            let target = view.commoners(view.side_to_move().flip());
            let actual = exploding_capture_in_view(&view, target);

            let mut state = StateInfo::new();
            board.populate_state(&mut state);
            let mut legal = MoveList::new();
            generate_legal_with_state(&board, &state, &mut legal);
            let oracle = legal.as_slice().iter().copied().find(|&c| {
                board.is_capture(c) && !(board.blast_casualties(c).all() & target).is_empty()
            });

            assert_eq!(actual, oracle, "{name}: first legal hit");
            assert_eq!(
                actual,
                expected.map(|uci| Move::from_uci(uci).expect("UCI must parse")),
                "{name}"
            );
        }
    }
}
