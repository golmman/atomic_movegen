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
//! [`EffectContext::terminal_after`](crate::EffectContext::terminal_after).
//!
//! The view is crate-private on purpose. [`Board`] remains the public position
//! representation.

use crate::attacks;
use crate::bitboard::between_bb;
use crate::board::{
    BK_CASTLE, BQ_CASTLE, BlastCasualties, Board, StateInfo, WK_CASTLE, WQ_CASTLE,
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
#[derive(Debug, Clone, Copy)]
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
        let us = self.side_to_move;
        state.checkers = self.checkers(us);
        state.pinned = self.pinned(us);
        state.commoners_count = self.commoners(us).count();
        state.them_commoners_count = self.commoners(us.flip()).count();
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

/// The single terminal-precedence implementation.
///
/// Shared by [`Board::terminal`] and
/// [`EffectContext::terminal_after`](crate::EffectContext::terminal_after), so a
/// position on a board and a position derived from bitboards are decided by the
/// same code in the same order: extinction → opponent extinction → material
/// draw → clock without check → no legal moves → clock with check.
pub(crate) fn terminal_in_view(
    view: &PositionView,
    state: &StateInfo,
    moves_exist: Option<bool>,
) -> Option<Terminal> {
    if state.commoners_count == 0 {
        return Some(Terminal {
            outcome: Outcome::Loss,
            reason: TerminalReason::Extinction,
        });
    }
    if state.them_commoners_count == 0 {
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

    // The clock is a draw without movegen when there is no check. When the
    // side to move is in check, mate outranks the clock, so resolve the
    // existence question first (Fairy-Stockfish's `!checkers() || legal`
    // guard).
    if view.rule50 >= 100 && state.checkers.is_empty() {
        return Some(Terminal {
            outcome: Outcome::Draw,
            reason: TerminalReason::FiftyMoveRule,
        });
    }

    let moves_exist =
        moves_exist.unwrap_or_else(|| crate::movegen::has_legal_move_in_view(view, state));

    if !moves_exist {
        return Some(if state.checkers.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::generate_pseudo_legal;

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
}
