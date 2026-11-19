//! The placement-only position snapshot the rules read.
//!
//! A [`PositionView`] holds everything the atomic rules depend on — piece
//! placement, side to move, en passant target, castling rights, and the
//! half-move clock — and nothing else. Hash and ply bookkeeping stay on
//! [`Board`].
//!
//! The view is *not* a second rules engine: [`Board::legal`], the move
//! generator, and the effect path all delegate to the functions here, so a
//! rule is written once and can be asked about a position that is not on a
//! board (for example the child of a move, derived from bitboards).
//!
//! One terminal precedence lives here too ([`terminal_in_view`]), shared by
//! [`Board::terminal`] and
//! [`EffectContext::terminal_after`](crate::EffectContext::terminal_after), plus
//! the child-side entry point that feeds the same branches from the placement
//! view ([`terminal_in_view_light`]).
//!
//! The view is crate-private on purpose. [`Board`] remains the public position
//! representation.

use crate::attacks;
use crate::bitboard::between_bb;
use crate::board::{
    BK_CASTLE, BQ_CASTLE, BlastCasualties, Board, Counts, StateInfo, WK_CASTLE, WQ_CASTLE,
    castling_rights_after, castling_squares, is_attacked_by,
};
use crate::types::*;

/// The parts of a position the rules read.
///
/// This is a snapshot: it holds no reference to a [`Board`] and stays valid
/// while the values it was built from do not change. [`Board`] stores one of
/// these as its placement, so the board's own rule queries borrow the view
/// instead of copying it; only positions that are *not* on a board (the child
/// of a move) need a stand-alone copy.
///
/// Occupancy is derived from the two color sets rather than cached, so a view
/// can never disagree with its own placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PositionView {
    /// Per-color piece bitboards.
    pub(crate) by_color: [Bitboard; Color::NB],
    /// Per-type piece bitboards.
    pub(crate) by_type: [Bitboard; PieceType::NB],
    /// The side to move.
    pub(crate) side_to_move: Color,
    /// The stored en-passant target, if any.
    pub(crate) ep_square: Option<Square>,
    /// The castling-rights bitmask.
    pub(crate) castling_rights: u8,
    /// The half-move clock.
    pub(crate) rule50: u16,
}

impl PositionView {
    /// Snapshot the position of `board`.
    ///
    /// This copies the view out of the board; the board's own rule queries use
    /// [`Board::position_view`] instead, which borrows it.
    #[inline]
    pub(crate) fn of(board: &Board) -> Self {
        *board.position_view()
    }

    /// Build a view from explicit parts.
    ///
    /// The caller owns the placement and must keep the per-color and per-type
    /// sets consistent with each other (a square holds at most one piece).
    #[inline]
    pub(crate) fn from_parts(
        by_color: [Bitboard; Color::NB],
        by_type: [Bitboard; PieceType::NB],
        side_to_move: Color,
        ep_square: Option<Square>,
        castling_rights: u8,
        rule50: u16,
    ) -> Self {
        Self {
            by_color,
            by_type,
            side_to_move,
            ep_square,
            castling_rights,
            rule50,
        }
    }

    /// The occupied squares.
    ///
    /// Computed from the two color sets rather than cached, so a view can never
    /// disagree with its own placement; node-constant consumers such as
    /// [`TrivialLegality`](crate::board::TrivialLegality) cache it once per
    /// node instead.
    #[inline(always)]
    pub(crate) fn occupied(&self) -> Bitboard {
        self.by_color[Color::White as usize] | self.by_color[Color::Black as usize]
    }

    /// The pieces of `c`.
    #[inline(always)]
    pub(crate) fn pieces_color(&self, c: Color) -> Bitboard {
        self.by_color[c as usize]
    }

    /// The pieces of type `pt`.
    #[inline(always)]
    pub(crate) fn pieces_pt(&self, pt: PieceType) -> Bitboard {
        self.by_type[pt as usize]
    }

    /// The pieces of color `c` and type `pt`.
    #[inline(always)]
    pub(crate) fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard {
        self.by_color[c as usize] & self.by_type[pt as usize]
    }

    /// The per-type piece sets, for the shared attacker query.
    #[inline(always)]
    pub(crate) fn by_type(&self) -> &[Bitboard; PieceType::NB] {
        &self.by_type
    }

    /// The commoners of `c`.
    #[inline(always)]
    pub(crate) fn commoners(&self, c: Color) -> Bitboard {
        self.pieces_color_pt(c, PieceType::Commoner)
    }

    /// The side to move.
    #[inline(always)]
    pub(crate) fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// The stored en-passant target.
    #[inline(always)]
    pub(crate) fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// The castling-rights bitmask.
    #[inline(always)]
    pub(crate) fn castling_rights(&self) -> u8 {
        self.castling_rights
    }

    /// The half-move clock.
    #[inline(always)]
    pub(crate) fn rule50(&self) -> u16 {
        self.rule50
    }

    /// The piece on `sq`, or [`NO_PIECE`].
    ///
    /// This scans the per-type sets instead of keeping a square array: the view
    /// is derived from bitboards and must not need a board write.
    #[inline]
    pub(crate) fn piece_on(&self, sq: Square) -> Piece {
        let (Some(color), Some(pt)) = (self.color_on(sq), self.type_on(sq)) else {
            return NO_PIECE;
        };
        make_piece(color, pt)
    }

    /// The color of the piece on `sq`, or `None` when the square is empty.
    #[inline]
    pub(crate) fn color_on(&self, sq: Square) -> Option<Color> {
        if sq == Square::NONE {
            return None;
        }
        if self.by_color[Color::White as usize].contains(sq as usize) {
            Some(Color::White)
        } else if self.by_color[Color::Black as usize].contains(sq as usize) {
            Some(Color::Black)
        } else {
            None
        }
    }

    /// The type of the piece on `sq`, or `None` when the square is empty.
    #[inline]
    pub(crate) fn type_on(&self, sq: Square) -> Option<PieceType> {
        if sq == Square::NONE {
            return None;
        }
        PIECE_TYPES
            .into_iter()
            .find(|&pt| self.by_type[pt as usize].contains(sq as usize))
    }

    /// Returns `true` when `sq` is empty (or [`Square::NONE`]).
    #[inline(always)]
    pub(crate) fn empty(&self, sq: Square) -> bool {
        sq == Square::NONE || !self.occupied().contains(sq as usize)
    }

    /// Return the enemy pieces checking the commoner of `us`.
    ///
    /// Mirrors [`Board::checkers`] exactly: no check when either side has no
    /// commoner, no check when the two commoners touch (touching makes the
    /// commoner immune to every attack), and otherwise the enemy pawns,
    /// knights, and sliders attacking the commoner's square.
    pub(crate) fn checkers(&self, us: Color) -> Bitboard {
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

    /// Return the pieces of `us` pinned to one of their commoners.
    ///
    /// Mirrors [`Board::pinned`] exactly. The atomic blast makes this a
    /// heuristic in the generator, never a legality decision: a pinned piece
    /// may still legally move when the moved piece is not what shields the
    /// commoner.
    pub(crate) fn pinned(&self, us: Color) -> Bitboard {
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

    /// Fill the cached state fields (checkers, pinned, commoner counts) of
    /// `state` for this view's side to move.
    #[inline]
    pub(crate) fn populate_state(&self, state: &mut StateInfo) {
        let checkers = self.checkers(self.side_to_move);
        self.populate_state_with_checkers(state, checkers);
    }

    /// [`populate_state`](Self::populate_state) with a `checkers` value the
    /// caller has already computed for this view's side to move.
    ///
    /// For callers that derive the attacker set for another purpose (the
    /// terminal precedence, the existence screen); the query then runs once
    /// instead of twice.
    #[inline]
    pub(crate) fn populate_state_with_checkers(&self, state: &mut StateInfo, checkers: Bitboard) {
        let us = self.side_to_move;
        state.checkers = checkers;
        state.pinned = self.pinned(us);
        state.commoners_count = self.commoners(us).count();
        state.them_commoners_count = self.commoners(us.flip()).count();
    }

    /// The two commoner counts of this view's side to move.
    ///
    /// Shared by the board's counts-only writer and reader so the pair has one
    /// expression. [`populate_state`](Self::populate_state) keeps its original
    /// inline writes; it computes pins and checkers anyway.
    #[inline]
    pub(crate) fn counts(&self) -> Counts {
        let us = self.side_to_move;
        Counts::new(
            self.commoners(us).count(),
            self.commoners(us.flip()).count(),
        )
    }

    /// Return `true` when both colors have exactly one commoner and no other
    /// pieces (an atomic K-vs-K position).
    ///
    /// See [`Board::is_material_draw`] for why this is the only atomic
    /// material draw.
    #[inline]
    pub(crate) fn is_material_draw(&self) -> bool {
        self.occupied().count() == 2
            && self.commoners(Color::White).count() == 1
            && self.commoners(Color::Black).count() == 1
    }

    /// Return `true` when `m` is a capture in this view.
    #[inline]
    pub(crate) fn is_capture(&self, m: Move) -> bool {
        m.move_type() == MoveType::EnPassant
            || (m.move_type() != MoveType::Castling && !self.empty(m.to_sq()))
    }

    /// Return every piece `m` removes, without modifying anything.
    ///
    /// Mirrors [`Board::blast_casualties`] exactly.
    pub(crate) fn blast_casualties(&self, m: Move) -> BlastCasualties {
        let mut casualties = BlastCasualties::default();
        if m.move_type() == MoveType::Castling || !self.is_capture(m) {
            return casualties;
        }

        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();
        let Some(pt) = self.type_on(from) else {
            return casualties;
        };

        let to_bb = Bitboard::square_bb(to);
        let from_bb = Bitboard::square_bb(from);
        // Simulated post-capture, pre-blast occupancy and pawns, matching the
        // state `do_move` uses when it builds the blast set.
        let mut occupied = self.occupied();
        let mut pawns = self.by_type[PieceType::Pawn as usize];

        if m.move_type() == MoveType::EnPassant {
            let victim = match us {
                Color::White => Square::from_index(to as i8 - 8),
                Color::Black => Square::from_index(to as i8 + 8),
            };
            let victim_bb = Bitboard::square_bb(victim);
            match us.flip() {
                Color::White => casualties.white = casualties.white | victim_bb,
                Color::Black => casualties.black = casualties.black | victim_bb,
            }
            casualties.en_passant_victim = Some(victim);
            occupied = occupied & !victim_bb;
            pawns = pawns & !victim_bb;
        } else {
            // A normal capture always has a piece on the destination.
            let victim_color = self
                .color_on(to)
                .expect("a capture has a victim on its destination");
            match victim_color {
                Color::White => casualties.white = casualties.white | to_bb,
                Color::Black => casualties.black = casualties.black | to_bb,
            }
            occupied = occupied & !to_bb;
            pawns = pawns & !to_bb;
        }

        // Relocate the capturer, applying a promotion if any.
        occupied = (occupied & !from_bb) | to_bb;
        if m.move_type() == MoveType::Promotion {
            pawns = pawns & !from_bb;
        } else if pt == PieceType::Pawn {
            pawns = (pawns & !from_bb) | to_bb;
        }

        let area = Board::blast_zone(to) & !pawns;
        let mut removed = (area & occupied) | to_bb;
        while !removed.is_empty() {
            let sq = removed.pop_lsb();
            let victim_color = if sq == to {
                us
            } else {
                self.color_on(sq).expect("a blasted square holds a piece")
            };
            match victim_color {
                Color::White => casualties.white = casualties.white | Bitboard::square_bb(sq),
                Color::Black => casualties.black = casualties.black | Bitboard::square_bb(sq),
            }
        }

        casualties
    }
}

/// The position reached by one move, derived from a [`PositionView`] without a
/// board write.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChildPosition {
    /// The child position itself.
    pub(crate) view: PositionView,
    /// The move removes a piece (normal capture or en passant).
    pub(crate) is_capture: bool,
    /// Every piece the move's blast removes, including the capturer at ground
    /// zero and the en-passant victim.
    pub(crate) casualties: BlastCasualties,
}

/// Derive the position reached by `m`, without a board write.
///
/// The child's side to move is the mover's opponent. `rule50` is the child's
/// half-move clock (see [`Board::rule50_after`]). `ep_square` and the castling
/// rights are derived exactly like [`Board::do_move`] derives them: a double
/// pawn push records the intermediate square, a castling move drops the mover's
/// rights, and otherwise [`castling_rights_after`] applies with the post-blast
/// contents of the six home squares read from the child placement.
///
/// Returns `None` for malformed input: [`Move::NONE`], an empty origin square
/// ([`Square::NONE`] included), or an origin that holds a piece of the wrong
/// color. Well-formed but illegal moves are *not* rejected here; the caller
/// decides what to do with them.
pub(crate) fn child_view(view: &PositionView, m: Move, rule50: u16) -> Option<ChildPosition> {
    if m == Move::NONE {
        return None;
    }
    let from = m.from_sq();
    let to = m.to_sq();
    let us = view.side_to_move;
    if !view.pieces_color(us).contains(from as usize) {
        return None;
    }
    let moving_type = view.type_on(from)?;
    let move_type = m.move_type();
    let is_capture = view.is_capture(m);
    let is_capture_or_ep = is_capture || move_type == MoveType::EnPassant;
    let casualties = view.blast_casualties(m);
    let removed = casualties.all();
    let from_bb = Bitboard::square_bb(from);
    let to_bb = Bitboard::square_bb(to);

    let mut by_color = [
        view.pieces_color(Color::White) & !removed,
        view.pieces_color(Color::Black) & !removed,
    ];
    let mut by_type = [
        view.pieces_pt(PieceType::Pawn) & !removed,
        view.pieces_pt(PieceType::Knight) & !removed,
        view.pieces_pt(PieceType::Bishop) & !removed,
        view.pieces_pt(PieceType::Rook) & !removed,
        view.pieces_pt(PieceType::Queen) & !removed,
        view.pieces_pt(PieceType::Commoner) & !removed,
    ];

    match move_type {
        MoveType::Castling => {
            let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
            let kfrom_bb = Bitboard::square_bb(kfrom);
            let rfrom_bb = Bitboard::square_bb(rfrom);
            let kto_bb = Bitboard::square_bb(kto);
            let rto_bb = Bitboard::square_bb(rto);
            by_color[us as usize] =
                (by_color[us as usize] & !(kfrom_bb | rfrom_bb)) | kto_bb | rto_bb;
            by_type[PieceType::Commoner as usize] =
                (by_type[PieceType::Commoner as usize] & !kfrom_bb) | kto_bb;
            by_type[PieceType::Rook as usize] =
                (by_type[PieceType::Rook as usize] & !rfrom_bb) | rto_bb;
        }
        _ if is_capture => {
            // Every capture — normal, en passant, and capturing promotion —
            // destroys the capturer at ground zero: the origin square empties
            // and nothing appears on `to`. (The capturer's removal is recorded
            // in `casualties` at `to`, not at `from`.)
            by_color[us as usize] = by_color[us as usize] & !from_bb;
            by_type[moving_type as usize] = by_type[moving_type as usize] & !from_bb;
        }
        MoveType::Promotion => {
            let promoted = m.promotion_type();
            by_color[us as usize] = (by_color[us as usize] & !from_bb) | to_bb;
            by_type[PieceType::Pawn as usize] = by_type[PieceType::Pawn as usize] & !from_bb;
            by_type[promoted as usize] = by_type[promoted as usize] | to_bb;
        }
        // En passant is always a capture, so the guarded arm above handles it;
        // this arm only satisfies the match exhaustiveness check.
        MoveType::EnPassant => {}
        MoveType::Normal => {
            by_color[us as usize] = (by_color[us as usize] & !from_bb) | to_bb;
            by_type[moving_type as usize] = (by_type[moving_type as usize] & !from_bb) | to_bb;
        }
    }

    let mut child = PositionView::from_parts(
        by_color,
        by_type,
        us.flip(),
        None,
        view.castling_rights,
        rule50,
    );

    if move_type == MoveType::Castling {
        child.castling_rights &= match us {
            Color::White => !(WK_CASTLE | WQ_CASTLE),
            Color::Black => !(BK_CASTLE | BQ_CASTLE),
        };
    } else {
        // The home-square cascade only runs for a capture.
        let post_blast = if is_capture_or_ep {
            [
                child.piece_on(Square::H1),
                child.piece_on(Square::A1),
                child.piece_on(Square::E1),
                child.piece_on(Square::H8),
                child.piece_on(Square::A8),
                child.piece_on(Square::E8),
            ]
        } else {
            // Ignored by `castling_rights_after` when there is no capture.
            [NO_PIECE; 6]
        };
        child.castling_rights = castling_rights_after(
            view.castling_rights,
            from,
            to,
            is_capture_or_ep,
            &post_blast,
        );

        if moving_type == PieceType::Pawn && (to as i8 - from as i8).abs() == 16 {
            child.ep_square = Some(match us {
                Color::White => Square::from_index(from as i8 + 8),
                Color::Black => Square::from_index(from as i8 - 8),
            });
        }
    }

    Some(ChildPosition {
        view: child,
        is_capture,
        casualties,
    })
}

/// Return `true` when `m` is legal in `view`, ignoring the "game is already
/// over" guard.
///
/// This is the body of [`Board::legal`] with the two commoner-count fields of
/// [`StateInfo`] replaced by an explicit caller check:
/// [`legal_in_view`] applies the guard from `state`, the effect path applies
/// its own snapshot of the same counts.
///
/// `m` must be pseudo-legal (the generator's precondition); a move whose origin
/// is empty returns `false`.
pub(crate) fn legal_with_counts(view: &PositionView, m: Move) -> bool {
    let from = m.from_sq();
    let to = m.to_sq();
    let us = view.side_to_move;
    let them = us.flip();

    if view.empty(from) {
        return false;
    }

    let is_capture = m.move_type() != MoveType::Castling
        && (m.move_type() == MoveType::EnPassant || !view.empty(to));

    if m.move_type() == MoveType::Castling {
        let ksq = from;
        let occupied = view.occupied();
        let pass_through = if to > ksq {
            [ksq, Square::from_index(ksq as i8 + 1)]
        } else {
            [Square::from_index(ksq as i8 - 1), ksq]
        };
        let enemy_commoners = view.commoners(them);
        for &sq in &pass_through {
            // A commoner adjacent to this square makes the commoner
            // immune to every attack while it stands there.
            if !(enemy_commoners & attacks::king_attacks(sq)).is_empty() {
                continue;
            }
            if square_attacked_by(view, sq, occupied, view.pieces_color(them)) {
                return false;
            }
        }
    }

    let mut occupied = view.occupied() ^ Bitboard::square_bb(from);
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
        let pre_pawns = view.pieces_pt(PieceType::Pawn);
        let pre_non_pawns = view.occupied() ^ pre_pawns;
        let blast_adjacent = attacks::king_attacks(kto) & pre_non_pawns;
        occupied = occupied & !(blast_adjacent | Bitboard::square_bb(kto));
    }

    let pre_commoners = view.commoners(us);
    let mut our_commoners = pre_commoners & occupied;

    if !is_capture && !(pre_commoners & Bitboard::square_bb(from)).is_empty() {
        our_commoners = our_commoners | Bitboard::square_bb(kto);
    }

    // A move may not destroy any of our commoners. In a legal position
    // there is exactly one, so this is the usual self-explosion check.
    if our_commoners.count() != pre_commoners.count() {
        return false;
    }

    // Capturing the enemy's only commoner wins immediately; the mover's
    // own commoner cannot be in check after the game has ended.
    let enemy_commoners = view.commoners(them) & occupied;
    if enemy_commoners.is_empty() {
        return true;
    }

    let ksq = our_commoners.lsb();
    // Touching the enemy commoner grants immunity to every attack.
    if !(enemy_commoners & attacks::king_attacks(ksq)).is_empty() {
        return true;
    }

    !square_attacked_by(view, ksq, occupied, view.pieces_color(them) & occupied)
}

/// The stored en-passant target, but only when the side to move has at least
/// one fully legal en-passant capture in this view.
///
/// Equal to [`Board::legal_ep_square`] for the equivalent board, without a
/// [`StateInfo`] and without a board write. Returns `None` when either side has
/// no commoner.
pub(crate) fn legal_ep_square_in_view(view: &PositionView) -> Option<Square> {
    let ep = view.ep_square()?;
    let us = view.side_to_move();
    let them = us.flip();
    if view.commoners(us).is_empty() || view.commoners(them).is_empty() {
        return None;
    }

    // Attack tables are symmetric, so this is the set of squares from which
    // one of our pawns attacks the ep target.
    let victim = match us {
        Color::White => Square::from_index(ep as i8 - 8),
        Color::Black => Square::from_index(ep as i8 + 8),
    };
    if view.piece_on(victim) != make_piece(them, PieceType::Pawn) {
        return None;
    }

    let mut origins = attacks::pawn_attacks(them, ep) & view.pieces_color_pt(us, PieceType::Pawn);
    while !origins.is_empty() {
        let from = origins.pop_lsb();
        if legal_with_counts(view, Move::make_enpassant(from, ep)) {
            return Some(ep);
        }
    }
    None
}

/// [`Board::legal`] over a view: rejects every move when the game is over.
pub(crate) fn legal_in_view(view: &PositionView, m: Move, state: &StateInfo) -> bool {
    // A board with only one commoner is terminal.
    if state.commoners_count == 0 || state.them_commoners_count == 0 {
        return false;
    }
    legal_with_counts(view, m)
}

/// Return `true` if an enemy non-commoner piece attacks `sq` on `occupied`.
///
/// Commoner attacks are intentionally excluded; callers handle adjacent enemy
/// commoners separately.
#[inline(always)]
fn square_attacked_by(
    view: &PositionView,
    sq: Square,
    occupied: Bitboard,
    enemy_bb: Bitboard,
) -> bool {
    is_attacked_by(sq, occupied, view.by_type(), enemy_bb, view.side_to_move)
}

/// The terminal-precedence prefix: the two cheap exits.
///
/// Extinction of the side to move, extinction of its opponent, and the
/// commoner-only material draw. These need the two commoner counts and nothing
/// else — no attacker query, no pins, no movegen — so a caller that has the
/// counts from anywhere (a populated [`StateInfo`] or two popcounts on a view)
/// can start here.
#[inline]
fn terminal_cheap(
    view: &PositionView,
    our_commoners: u32,
    their_commoners: u32,
) -> Option<Terminal> {
    if our_commoners == 0 {
        return Some(Terminal {
            outcome: Outcome::Loss,
            reason: TerminalReason::Extinction,
        });
    }
    if their_commoners == 0 {
        return Some(Terminal {
            outcome: Outcome::Win,
            reason: TerminalReason::Extinction,
        });
    }
    if view.is_material_draw() {
        return Some(Terminal {
            outcome: Outcome::Draw,
            reason: TerminalReason::MaterialDraw,
        });
    }
    None
}

/// The terminal-precedence tail: the clock, the existence question, and mate or
/// stalemate.
///
/// `in_check` says whether the side to move is in check; it is a closure so a
/// caller that cannot cheaply know it pays for the attacker query only when the
/// answer matters: a position whose clock is up and whose side to move is *not*
/// in check is decided without asking it, and so is a position whose clock is
/// down and which has legal moves. `moves_exist` answers the existence question
/// and is only called when the answer matters.
#[inline]
fn terminal_tail(
    view: &PositionView,
    in_check: impl Fn() -> bool,
    moves_exist: impl FnOnce() -> bool,
) -> Option<Terminal> {
    // The clock is a draw without movegen when there is no check. When the
    // side to move is in check, mate outranks the clock, so resolve the
    // existence question first (Fairy-Stockfish's `!checkers() || legal`
    // guard).
    if view.rule50 >= 100 && !in_check() {
        return Some(Terminal {
            outcome: Outcome::Draw,
            reason: TerminalReason::FiftyMoveRule,
        });
    }

    if !moves_exist() {
        return Some(if !in_check() {
            Terminal {
                outcome: Outcome::Draw,
                reason: TerminalReason::Stalemate,
            }
        } else {
            Terminal {
                outcome: Outcome::Loss,
                reason: TerminalReason::NoMovesInCheck,
            }
        });
    }
    if view.rule50 >= 100 {
        return Some(Terminal {
            outcome: Outcome::Draw,
            reason: TerminalReason::FiftyMoveRule,
        });
    }
    None
}

/// The single terminal-precedence implementation.
///
/// Shared by [`Board::terminal`] and
/// [`EffectContext::terminal_after`](crate::EffectContext::terminal_after), so a
/// position on a board and a position derived from bitboards are decided by the
/// same code in the same order: extinction → opponent extinction → material
/// draw → clock without check → no legal moves → clock with check.
///
/// The branches themselves live in [`terminal_cheap`] and [`terminal_tail`],
/// which the child-side [`terminal_in_view_light`] calls in the same order.
pub(crate) fn terminal_in_view(
    view: &PositionView,
    state: &StateInfo,
    moves_exist: Option<bool>,
) -> Option<Terminal> {
    if let Some(terminal) = terminal_cheap(view, state.commoners_count, state.them_commoners_count)
    {
        return Some(terminal);
    }
    terminal_tail(
        view,
        || !state.checkers.is_empty(),
        || moves_exist.unwrap_or_else(|| crate::movegen::has_legal_move_in_view(view, state)),
    )
}

/// The child-side terminal-precedence implementation: a view, no [`StateInfo`].
///
/// Same precedence as [`terminal_in_view`] and the same branch code, but the
/// facts it reads come from the view: the two commoner counts (one popcount
/// each) and “is the side to move in check”, whose attacker query runs only when
/// the precedence actually needs it. The existence question itself is
/// [`has_legal_move_in_view_unpopulated`](crate::movegen::has_legal_move_in_view_unpopulated),
/// which answers `true` from a proven quiet legal move and falls back to the
/// exact ladder (populating a [`StateInfo`]) otherwise.
///
/// The attacker query is deliberately run only after [`terminal_cheap`]:
/// extinction and the material draw must not pay for it.
pub(crate) fn terminal_in_view_light(view: &PositionView) -> Option<Terminal> {
    let us = view.side_to_move();
    let our_commoners = view.commoners(us).count();
    let their_commoners = view.commoners(us.flip()).count();
    if let Some(terminal) = terminal_cheap(view, our_commoners, their_commoners) {
        return Some(terminal);
    }

    terminal_tail(
        view,
        || !view.checkers(us).is_empty(),
        || crate::movegen::has_legal_move_in_view_unpopulated(view),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::{generate_legal_with_state, generate_pseudo_legal};

    /// The 41-position perft oracle corpus plus the deterministic fixtures the
    /// effect tests use, so view construction is checked on real and on
    /// artificial positions.
    fn view_corpus() -> Vec<String> {
        let table = include_str!("../tests/perft_values.md");
        let mut fens = Vec::new();
        for line in table.lines() {
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
        for fen in [
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
            "8/8/8/8/8/8/8/k7 w - - 0 1",
            "k7/8/8/8/8/8/8/7K w - - 0 1",
            "7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55",
            "8/8/8/8/8/4k3/8/4K2q w - - 99 60",
        ] {
            fens.push(fen.to_owned());
        }
        fens
    }

    /// Every field of the view must equal the [`Board`] counterpart; a
    /// view-construction defect then fails with a field name instead of a
    /// perft number.
    #[test]
    fn view_matches_board() {
        for fen in view_corpus() {
            let fen = fen.as_str();
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let view = PositionView::of(&board);

            assert_eq!(view.occupied(), board.occupied(), "{fen}: occupied");
            assert_eq!(view.side_to_move(), board.side_to_move(), "{fen}: side");
            assert_eq!(view.ep_square(), board.ep_square(), "{fen}: ep");
            assert_eq!(
                view.castling_rights(),
                board.castling_rights(),
                "{fen}: rights"
            );
            assert_eq!(view.rule50(), board.rule50(), "{fen}: rule50");
            for c in [Color::White, Color::Black] {
                assert_eq!(view.pieces_color(c), board.pieces_color(c), "{fen}: {c:?}");
                assert_eq!(view.commoners(c), board.commoners(c), "{fen}: {c:?}");
                assert_eq!(
                    view.checkers(c),
                    board.compute_checkers(c),
                    "{fen}: checkers {c:?}"
                );
                assert_eq!(
                    view.pinned(c),
                    board.compute_pinned(c),
                    "{fen}: pinned {c:?}"
                );
            }
            for pt in PIECE_TYPES {
                assert_eq!(view.pieces_pt(pt), board.pieces_pt(pt), "{fen}: {pt:?}");
            }
            for sq in SQUARES.iter().copied() {
                assert_eq!(view.piece_on(sq), board.piece_on(sq), "{fen}: {sq:?}");
                assert_eq!(view.empty(sq), board.empty(sq), "{fen}: {sq:?} empty");
            }
            assert_eq!(
                view.is_material_draw(),
                board.is_material_draw(),
                "{fen}: material draw"
            );

            let mut from_view = StateInfo::new();
            view.populate_state(&mut from_view);
            let mut from_board = StateInfo::new();
            board.populate_state(&mut from_board);
            assert_eq!(from_view.checkers, from_board.checkers, "{fen}: checkers");
            assert_eq!(from_view.pinned, from_board.pinned, "{fen}: pinned");
            assert_eq!(
                from_view.commoners_count, from_board.commoners_count,
                "{fen}: counts"
            );
            assert_eq!(
                from_view.them_commoners_count, from_board.them_commoners_count,
                "{fen}: counts"
            );

            // The counts-only shape is one expression shared by both writers.
            assert_eq!(view.counts(), Counts::from(&from_board), "{fen}: counts");
            let mut counts_only = StateInfo::new();
            view.populate_state(&mut counts_only);
            let mut full = StateInfo::new();
            board.populate_state(&mut full);
            board.populate_counts(&mut counts_only);
            assert_eq!(counts_only.commoners_count, full.commoners_count);
            assert_eq!(
                counts_only.them_commoners_count, full.them_commoners_count,
                "{fen}: counts-only writer"
            );
        }
    }

    /// The child view must mirror `do_move` field for field.
    #[test]
    fn child_view_matches_do_move() {
        for fen in view_corpus() {
            let fen = fen.as_str();
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let view = PositionView::of(&board);
            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let child = child_view(&view, m, board.rule50_after(m))
                    .unwrap_or_else(|| panic!("{fen} {m}: child"))
                    .view;
                let mut played = board.clone();
                let mut state = StateInfo::new();
                played.do_move(m, &mut state);

                assert_eq!(
                    child.pieces_color(Color::White),
                    played.pieces_color(Color::White),
                    "{fen} {m}: white"
                );
                assert_eq!(
                    child.pieces_color(Color::Black),
                    played.pieces_color(Color::Black),
                    "{fen} {m}: black"
                );
                for pt in PIECE_TYPES {
                    assert_eq!(
                        child.pieces_pt(pt),
                        played.pieces_pt(pt),
                        "{fen} {m}: {pt:?}"
                    );
                }
                assert_eq!(
                    child.side_to_move(),
                    played.side_to_move(),
                    "{fen} {m}: side"
                );
                assert_eq!(child.ep_square(), played.ep_square(), "{fen} {m}: ep");
                assert_eq!(
                    child.castling_rights(),
                    played.castling_rights(),
                    "{fen} {m}: rights"
                );
                assert_eq!(child.rule50(), played.rule50(), "{fen} {m}: rule50");
            }
        }
    }

    /// The view-side legal-EP answer is the board-side answer, for the parent
    /// and for every child.
    #[test]
    fn legal_ep_square_in_view_matches_board() {
        let mut stored = 0u64;
        let mut legal = 0u64;
        let mut removed = 0u64;
        let fixtures = [
            "4r2k/3p4/8/4P3/8/8/8/4K3 b - - 0 1",
            "7k/3pK3/8/4P3/8/8/8/8 b - - 0 1",
            "4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1",
            "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        ];
        for fen in view_corpus().iter().map(String::as_str).chain(fixtures) {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let view = PositionView::of(&board);
            let expected = board.legal_ep_square();
            assert_eq!(
                legal_ep_square_in_view(&view),
                expected,
                "{fen}: parent legal ep"
            );
            if view.ep_square().is_some() {
                stored += 1;
                if expected.is_some() {
                    legal += 1;
                } else {
                    removed += 1;
                }
            }

            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let child = child_view(&view, m, board.rule50_after(m))
                    .unwrap_or_else(|| panic!("{fen} {m}: child"))
                    .view;
                let mut played = board.clone();
                let mut state = StateInfo::new();
                played.do_move(m, &mut state);
                let expected = played.legal_ep_square();
                assert_eq!(
                    legal_ep_square_in_view(&child),
                    expected,
                    "{fen} {m}: child legal ep"
                );
                if child.ep_square().is_some() {
                    stored += 1;
                    if expected.is_some() {
                        legal += 1;
                    } else {
                        removed += 1;
                    }
                }
            }
        }
        assert!(stored > 20, "expected stored ep targets, got {stored}");
        assert!(legal > 0, "expected a legal ep capture");
        assert!(removed > 0, "expected non-capturable stored targets");
    }

    /// Seeded play-outs, including the random clock: the view answer stays the
    /// board answer at every node.
    #[test]
    fn legal_ep_square_in_view_matches_board_playouts() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut visited = 0u64;
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        ] {
            let mut board = Board::from_fen(fen).expect("FEN must parse");
            let mut state = StateInfo::new();
            for _ in 0..200 {
                board.populate_state(&mut state);
                let view = board.position_view();
                assert_eq!(
                    legal_ep_square_in_view(view),
                    board.legal_ep_square(),
                    "{}: legal ep",
                    board.fen()
                );
                visited += 1;

                let mut legal = MoveList::new();
                generate_legal_with_state(&board, &state, &mut legal);
                if legal.is_empty() {
                    break;
                }
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let m = legal.as_slice()[(seed % legal.len() as u64) as usize];
                board.do_move(m, &mut state);
            }
        }
        assert!(visited > 100, "expected a real walk, got {visited}");
    }

    fn view_of(fen: &str) -> PositionView {
        PositionView::of(&Board::from_fen(fen).expect("FEN must parse"))
    }

    fn state_of(view: &PositionView) -> StateInfo {
        let mut state = StateInfo::new();
        view.populate_state(&mut state);
        state
    }

    /// The prefix decides extinction in both directions and the commoner-only
    /// material draw, and says nothing about any other position.
    #[test]
    fn terminal_cheap_branches() {
        let loss = Terminal {
            outcome: Outcome::Loss,
            reason: TerminalReason::Extinction,
        };
        let win = Terminal {
            outcome: Outcome::Win,
            reason: TerminalReason::Extinction,
        };
        let draw = Terminal {
            outcome: Outcome::Draw,
            reason: TerminalReason::MaterialDraw,
        };

        // The side to move has no commoner.
        assert_eq!(
            terminal_cheap(&view_of("8/8/8/8/8/8/8/k7 w - - 0 1"), 0, 1),
            Some(loss)
        );
        // The opponent has no commoner.
        assert_eq!(
            terminal_cheap(&view_of("8/8/8/8/8/8/8/7K w - - 0 1"), 1, 0),
            Some(win)
        );
        // Both counts are checked in that order: both extinct is a loss.
        assert_eq!(
            terminal_cheap(&view_of("8/8/8/8/8/8/8/8 w - - 0 1"), 0, 0),
            Some(loss)
        );
        // K vs K is the only atomic material draw.
        assert_eq!(
            terminal_cheap(&view_of("k7/8/8/8/8/8/8/7K w - - 0 1"), 1, 1),
            Some(draw)
        );
        // Two commoners and any other piece is not a material draw.
        assert_eq!(
            terminal_cheap(&view_of("k7/8/8/8/8/8/8/R6K w - - 0 1"), 1, 1),
            None
        );
        // Vanilla positions are undecided by the prefix.
        assert_eq!(terminal_cheap(&PositionView::of(&Board::new()), 1, 1), None);
    }

    /// The tail: the clock without check, the existence question, mate or
    /// stalemate, and the clock again.
    #[test]
    fn terminal_tail_branches() {
        let clock = Terminal {
            outcome: Outcome::Draw,
            reason: TerminalReason::FiftyMoveRule,
        };
        let loaded = view_of("k7/8/8/8/8/8/8/7K w - - 100 60");
        assert_eq!(loaded.rule50(), 100);
        let fresh = view_of("k7/8/8/8/8/8/8/7K w - - 0 1");
        assert_eq!(fresh.rule50(), 0);

        let never = || -> bool { panic!("the existence question must not be asked") };
        let in_check = || true;
        let not_in_check = || false;
        let yes = || true;
        let no = || false;

        // The clock is a draw without movegen when there is no check.
        assert_eq!(terminal_tail(&loaded, not_in_check, never), Some(clock));
        // A clock with the side to move in check is resolved by the existence
        // question first: mate outranks the clock, and so does the clock when
        // moves exist.
        assert_eq!(terminal_tail(&loaded, in_check, yes), Some(clock));
        assert_eq!(
            terminal_tail(&loaded, in_check, no),
            Some(Terminal {
                outcome: Outcome::Loss,
                reason: TerminalReason::NoMovesInCheck,
            })
        );
        // Without the clock: no moves and no check is stalemate.
        assert_eq!(
            terminal_tail(&fresh, not_in_check, no),
            Some(Terminal {
                outcome: Outcome::Draw,
                reason: TerminalReason::Stalemate,
            })
        );
        // Undecided: moves exist and the clock is not up. The check flag is
        // never read here, which is what lets the light path derive it lazily.
        assert_eq!(terminal_tail(&fresh, not_in_check, yes), None);
        assert_eq!(terminal_tail(&fresh, in_check, yes), None);
    }

    /// The light path and the board path are the same precedence: same answer
    /// for every position, not just for decided ones.
    #[test]
    fn terminal_light_matches_board_path() {
        let mut checked = 0u64;
        let mut decided = 0u64;
        for fen in view_corpus() {
            let board = Board::from_fen(fen.as_str()).expect("corpus FEN must parse");
            let view = PositionView::of(&board);
            let state = state_of(&view);

            let light = terminal_in_view_light(&view);
            let heavy = terminal_in_view(&view, &state, None);
            assert_eq!(light, heavy, "{fen}: light vs board path");
            assert_eq!(light, board.terminal(), "{fen}: light vs Board::terminal");
            if light.is_some() {
                decided += 1;
            }
            checked += 1;

            // Every child of every corpus position, decided or not.
            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let child = child_view(&view, m, board.rule50_after(m))
                    .expect("child")
                    .view;
                let child_state = state_of(&child);
                assert_eq!(
                    terminal_in_view_light(&child),
                    terminal_in_view(&child, &child_state, None),
                    "{fen} {m}: child light vs board path"
                );

                let mut played = board.clone();
                let mut played_state = StateInfo::new();
                played.do_move(m, &mut played_state);
                assert_eq!(
                    terminal_in_view_light(&child),
                    played.terminal(),
                    "{fen} {m}: child light vs clone"
                );
                checked += 1;
                if terminal_in_view_light(&child).is_some() {
                    decided += 1;
                }
            }
        }
        assert!(checked > 1000, "expected many positions, got {checked}");
        assert!(decided > 20, "expected decided positions, got {decided}");
    }

    /// A seeded play-out walk: the light path must agree with the board path at
    /// every visited position, including clock and check branches.
    #[test]
    fn terminal_light_matches_board_path_playouts() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut visited = 0u64;
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/4k3/8/8/8/8/4P3/4K3 w - - 96 60",
        ] {
            let mut board = Board::from_fen(fen).expect("FEN must parse");
            let mut state = StateInfo::new();
            for _ in 0..200 {
                board.populate_state(&mut state);
                let view = board.position_view();
                assert_eq!(
                    terminal_in_view_light(view),
                    terminal_in_view(view, &state, None),
                    "{}: light vs board path",
                    board.fen()
                );
                assert_eq!(
                    terminal_in_view_light(view),
                    board.terminal_with_state(&state),
                    "{}: light vs board path (state)",
                    board.fen()
                );
                visited += 1;

                let mut legal = MoveList::new();
                generate_legal_with_state(&board, &state, &mut legal);
                if legal.is_empty() {
                    break;
                }
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let m = legal.as_slice()[(seed % legal.len() as u64) as usize];
                board.do_move(m, &mut state);
            }
        }
        assert!(visited > 100, "expected a real walk, got {visited}");
    }
}
