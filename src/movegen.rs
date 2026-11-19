//! Pseudo-legal and legal move generation for atomic chess.

use crate::attacks;
use crate::bitboard::between_bb;
use crate::board::{BK_CASTLE, BQ_CASTLE, Board, FastLegality, StateInfo, WK_CASTLE, WQ_CASTLE};
use crate::position::PositionView;
use crate::types::*;
use std::ops::ControlFlow;

/// Generate all pseudo-legal moves for the side to move.
///
/// Pseudo-legal means every move that is legal *except* moves that would
/// result in self-explosion (destroying the commoner), castling through
/// check, or leaving the commoner under attack. Use [`generate_legal`] for
/// fully legal moves.
///
/// The output order is the generation-order contract documented on
/// [`for_each_pseudo_legal`].
pub fn generate_pseudo_legal(board: &Board, moves: &mut MoveList) {
    generate_pseudo_legal_in_view(board.position_view(), moves);
}

/// [`generate_pseudo_legal`] over a [`PositionView`]: the single
/// implementation both entry points use.
pub(crate) fn generate_pseudo_legal_in_view(view: &PositionView, moves: &mut MoveList) {
    let _ = for_each_pseudo_legal_in_view::<core::convert::Infallible>(view, &mut |m| {
        moves.push(m);
        ControlFlow::Continue(())
    });
}

/// Walk the pseudo-legal candidate stream for the side to move, invoking `f`
/// on each candidate in generation order and stopping early if `f` returns
/// [`ControlFlow::Break`].
///
/// The walk visits the same stages in the same order as
/// [`generate_pseudo_legal`], which is a thin wrapper collecting the whole
/// stream into a [`MoveList`]: pawn moves (single/double pushes, captures,
/// promotions, en passant, per pawn) → knights → bishops → rooks → queens →
/// commoners → castling. `f` must not mutate the board.
///
/// # Generation-order contract
///
/// Generation order is a public compatibility contract: pawn moves per pawn in
/// ascending origin-square order (single push, four promotions in
/// [`PROMOTION_PIECES`] order, double push, captures in ascending destination
/// order, en passant), then knights, bishops, rooks, queens, commoners, then
/// castling (king side before queen side). [`generate_legal`] preserves the
/// pseudo-legal order after filtering. The order may only change in a major
/// release and is enforced by `tests/move_order_golden.rs`.
pub fn for_each_pseudo_legal<B>(
    board: &Board,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    for_each_pseudo_legal_in_view(board.position_view(), f)
}

/// [`for_each_pseudo_legal`] over a [`PositionView`]: the single
/// implementation both entry points use.
pub(crate) fn for_each_pseudo_legal_in_view<B>(
    view: &PositionView,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    let us = view.side_to_move();
    let them = us.flip();
    let occupied = view.occupied();
    let them_bb = view.pieces_color(them);
    let ep_square = view.ep_square();
    let target = !occupied | them_bb;

    let mut p = view.pieces_color_pt(us, PieceType::Pawn);
    while !p.is_empty() {
        let from = p.pop_lsb();
        generate_pawn_moves_for(view, us, occupied, them_bb, ep_square, from, f)?;
    }

    generate_piece_moves(
        view,
        us,
        target,
        PieceType::Knight,
        attacks::knight_attacks,
        f,
    )?;
    generate_piece_moves(
        view,
        us,
        target,
        PieceType::Bishop,
        |sq| attacks::bishop_attacks(sq, occupied),
        f,
    )?;
    generate_piece_moves(
        view,
        us,
        target,
        PieceType::Rook,
        |sq| attacks::rook_attacks(sq, occupied),
        f,
    )?;
    generate_piece_moves(
        view,
        us,
        target,
        PieceType::Queen,
        |sq| attacks::queen_attacks(sq, occupied),
        f,
    )?;
    generate_piece_moves(
        view,
        us,
        target,
        PieceType::Commoner,
        attacks::king_attacks,
        f,
    )?;

    // Castling moves. Most nodes have no castling rights for the side to move,
    // so skip the whole path with one test.
    if view.castling_rights() & castling_rights_mask(us) != 0 {
        generate_castling(view, us, f)?;
    }
    ControlFlow::Continue(())
}

#[inline(always)]
fn generate_piece_moves<B>(
    view: &PositionView,
    us: Color,
    target: Bitboard,
    pt: PieceType,
    attacks: impl Fn(Square) -> Bitboard,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    let mut pieces = view.pieces_color_pt(us, pt);
    while !pieces.is_empty() {
        let from = pieces.pop_lsb();
        let mut a = attacks(from) & target;
        while !a.is_empty() {
            let to = a.pop_lsb();
            f(Move::make_move(from, to))?;
        }
    }
    ControlFlow::Continue(())
}

/// Emit the pseudo-legal moves of one pawn in generation order: single push (or
/// its four promotions), double push, captures (ascending target square, i.e.
/// west before east), en passant.
///
/// `occupied`, `them_bb` and `ep_square` are per-node values hoisted by the
/// caller; the pawn targets come from the compile-time attack tables.
fn generate_pawn_moves_for<B>(
    view: &PositionView,
    us: Color,
    occupied: Bitboard,
    them_bb: Bitboard,
    ep_square: Option<Square>,
    from: Square,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    let them = us.flip();
    let (start_rank, promo_from_rank) = match us {
        Color::White => (Rank::R2, Rank::R7),
        Color::Black => (Rank::R7, Rank::R2),
    };
    let from_rank = rank_of(from);
    let promoting = from_rank == promo_from_rank;
    let attacks_bb = attacks::pawn_attacks(us, from);

    // Single push. A promotion push expands to the four promotion moves.
    let single = attacks::pawn_pushes(us, from) & !occupied;
    if !single.is_empty() {
        let to_sq = single.lsb();
        if promoting {
            for &pt in &PROMOTION_PIECES {
                f(Move::make_promotion(from, to_sq, pt))?;
            }
        } else {
            f(Move::make_move(from, to_sq))?;
        }

        // Double push: only from the start rank, and only when the
        // intermediate square is empty (implied by the single push being
        // non-empty) and the destination is empty.
        if from_rank == start_rank {
            let double = attacks::pawn_double_pushes(us, from) & !occupied;
            if !double.is_empty() {
                f(Move::make_move(from, double.lsb()))?;
            }
        }
    }

    // Captures. `pop_lsb` visits ascending target squares, which is the
    // required west-before-east order for both colours.
    let mut captures = attacks_bb & them_bb;
    while !captures.is_empty() {
        let to_sq = captures.pop_lsb();
        if promoting {
            for &pt in &PROMOTION_PIECES {
                f(Move::make_promotion(from, to_sq, pt))?;
            }
        } else {
            f(Move::make_move(from, to_sq))?;
        }
    }

    // En passant
    if let Some(ep_sq) = ep_square {
        if attacks_bb & Bitboard::square_bb(ep_sq) != Bitboard::EMPTY {
            let ep_cap = match us {
                Color::White => Square::from_index(ep_sq as i8 - 8),
                Color::Black => Square::from_index(ep_sq as i8 + 8),
            };
            if view
                .pieces_color_pt(them, PieceType::Pawn)
                .contains(ep_cap as usize)
            {
                f(Move::make_enpassant(from, ep_sq))?;
            }
        }
    }

    ControlFlow::Continue(())
}

/// The castling-right bits that belong to `us`.
#[inline(always)]
fn castling_rights_mask(us: Color) -> u8 {
    match us {
        Color::White => WK_CASTLE | WQ_CASTLE,
        Color::Black => BK_CASTLE | BQ_CASTLE,
    }
}

fn generate_castling<B>(
    view: &PositionView,
    us: Color,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    let (
        king_side_right,
        queen_side_right,
        king_sq,
        king_side_rook_sq,
        queen_side_rook_sq,
        king_side_squares,
        queen_side_squares,
    ) = match us {
        Color::White => (
            WK_CASTLE,
            WQ_CASTLE,
            Square::E1,
            Square::H1,
            Square::A1,
            [Square::F1, Square::G1],
            [Square::B1, Square::C1, Square::D1],
        ),
        Color::Black => (
            BK_CASTLE,
            BQ_CASTLE,
            Square::E8,
            Square::H8,
            Square::A8,
            [Square::F8, Square::G8],
            [Square::B8, Square::C8, Square::D8],
        ),
    };

    let commoners = view.pieces_color_pt(us, PieceType::Commoner);
    let rooks = view.pieces_color_pt(us, PieceType::Rook);

    // King-side castling
    if view.castling_rights() & king_side_right != 0
        && commoners.contains(king_sq as usize)
        && rooks.contains(king_side_rook_sq as usize)
    {
        let mut clear = true;
        for &sq in &king_side_squares {
            if !view.empty(sq) {
                clear = false;
                break;
            }
        }
        if clear {
            f(Move::make_castling(king_sq, king_side_rook_sq))?;
        }
    }

    // Queen-side castling
    if view.castling_rights() & queen_side_right != 0
        && commoners.contains(king_sq as usize)
        && rooks.contains(queen_side_rook_sq as usize)
    {
        let mut clear = true;
        for &sq in &queen_side_squares {
            if !view.empty(sq) {
                clear = false;
                break;
            }
        }
        if clear {
            f(Move::make_castling(king_sq, queen_side_rook_sq))?;
        }
    }

    ControlFlow::Continue(())
}

/// Generate all fully legal moves for the side to move.
///
/// Wraps [`generate_pseudo_legal`] and filters out illegal moves with the
/// per-node fast-legality ladder: quiet moves, captures, explosions,
/// self-explosions, en-passant blasts, and quiet commoner relocations are
/// decided from node-constant bitboards, and only the remaining candidates
/// reach [`Board::legal`].
///
/// The output preserves the pseudo-legal order; see the generation-order
/// contract on [`for_each_pseudo_legal`].
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    // One view per node: the state and the generator both read from it, so the
    // snapshot is built once here instead of once per entry point.
    let view = board.position_view();
    let mut state = StateInfo::new();
    view.populate_state(&mut state);
    generate_legal_in_view(view, &state, moves);
}

/// Generate all fully legal moves for the side to move using a pre-populated
/// [`StateInfo`].
///
/// The caller is responsible for ensuring `state` is up to date (e.g. by calling
/// [`Board::populate_state`]) before invoking this function. It does not call
/// `populate_state` itself. Unlike [`Board::legal`], this entry point reads
/// `checkers` and `pinned`, so a counts-only state from
/// [`Board::populate_counts`] is **not** sufficient. The output preserves the
/// pseudo-legal order; see the generation-order contract on
/// [`for_each_pseudo_legal`].
pub fn generate_legal_with_state(board: &Board, state: &StateInfo, moves: &mut MoveList) {
    generate_legal_in_view(board.position_view(), state, moves);
}

/// [`generate_legal_with_state`] over a [`PositionView`]: the single
/// implementation both entry points use.
pub(crate) fn generate_legal_in_view(view: &PositionView, state: &StateInfo, moves: &mut MoveList) {
    // The game is already over when either side has no commoner left
    // (extinction). Fairy-Stockfish's `generate<LEGAL>` returns no moves in
    // that case, so neither do we.
    if state.commoners_count == 0 || state.them_commoners_count == 0 {
        return;
    }

    generate_pseudo_legal_in_view(view, moves);

    // In-place compaction: the ladder accepts most moves without a `legal()`
    // call.
    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    // Node-constant fast-legality data, hoisted out of the compaction loop.
    let fast = FastLegality::new(view, state);

    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            if fast.is_legal(view, m, state) {
                ms[write_idx] = m;
                write_idx += 1;
            }
        }
        write_idx
    };
    moves.set_len(new_len);
}

/// Walk the legal move stream for the side to move, invoking `f` on each
/// legal move in generation order and stopping early if `f` returns
/// [`ControlFlow::Break`].
///
/// The stream visits exactly the moves that [`generate_legal`] collects, in the
/// same order: it walks [`for_each_pseudo_legal`] and keeps the candidates that
/// the shared fast-legality ladder accepts, falling back to [`Board::legal`] for
/// the rest. No [`MoveList`] is built and nothing is allocated. `f` must not
/// mutate the board.
///
/// Positions where the game is already over (either side has no commoner) emit
/// nothing, matching [`generate_legal`]. This populates one temporary
/// [`StateInfo`] internally; use [`for_each_legal_with_state`] to reuse a
/// caller-populated one.
pub fn for_each_legal<B>(
    board: &Board,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    let view = board.position_view();
    let mut state = StateInfo::new();
    view.populate_state(&mut state);
    for_each_legal_in_view(view, &state, f)
}

/// Same as [`for_each_legal`], using a caller-populated [`StateInfo`].
///
/// The caller is responsible for ensuring `state` is up to date (e.g. by
/// calling [`Board::populate_state`]) before invoking this function. It does
/// not call `populate_state` itself. This entry point reads `checkers` and
/// `pinned`, so a counts-only state from [`Board::populate_counts`] is **not**
/// sufficient.
///
/// # Equivalence contract
///
/// The visited sequence is exactly `generate_legal_with_state(board, state,
/// ..)`, element for element and in order.
pub fn for_each_legal_with_state<B>(
    board: &Board,
    state: &StateInfo,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    for_each_legal_in_view(board.position_view(), state, f)
}

/// [`for_each_legal_with_state`] over a [`PositionView`]: the single
/// implementation both entry points use.
pub(crate) fn for_each_legal_in_view<B>(
    view: &PositionView,
    state: &StateInfo,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    // The game is already over when either side has no commoner left
    // (extinction), matching `generate_legal_with_state`.
    if state.commoners_count == 0 || state.them_commoners_count == 0 {
        return ControlFlow::Continue(());
    }

    // Node-constant data, built once per node. The same ladder as
    // `generate_legal_with_state`: the trivial quiet fast path first, then the
    // widened rules, then `Board::legal`.
    let fast = FastLegality::new(view, state);

    for_each_pseudo_legal_in_view(view, &mut |m| {
        if fast.is_legal(view, m, state) {
            f(m)
        } else {
            ControlFlow::Continue(())
        }
    })
}

/// Returns `true` if the side to move has at least one legal move.
///
/// Early-exits at the first legal move; substantially cheaper than
/// [`generate_legal`] when moves exist. Calls [`Board::populate_state`]
/// internally.
pub fn has_legal_move(board: &Board) -> bool {
    let view = board.position_view();
    let mut state = StateInfo::new();
    view.populate_state(&mut state);
    has_legal_move_in_view(view, &state)
}

/// Same as [`has_legal_move`], using a caller-populated [`StateInfo`].
///
/// The caller is responsible for ensuring `state` is up to date (e.g. by
/// calling [`Board::populate_state`]) before invoking this function. It
/// does not call `populate_state` itself. This entry point reads `checkers` and
/// `pinned`, so a counts-only state from [`Board::populate_counts`] is **not**
/// sufficient.
///
/// # Equivalence contract
///
/// For any position and any populated `StateInfo` this returns exactly
/// `!generate_legal_with_state(board, state, list).is_empty()`.
pub fn has_legal_move_with_state(board: &Board, state: &StateInfo) -> bool {
    has_legal_move_in_view(board.position_view(), state)
}

/// [`has_legal_move_with_state`] over a [`PositionView`]: the single
/// implementation both entry points use.
///
/// Two steps: the positive screen ([`quiet_legal_witness`]) first, which answers
/// `true` as soon as it can *prove* one legal move, then the exact ladder
/// ([`has_legal_move_ladder`]). The screen can never change an answer: a witness
/// is a proof of existence and "no witness" concludes nothing, so every other
/// case is decided by the ladder that ran here before. The screen reads the two
/// facts a populated [`StateInfo`] already holds (`checkers`, `pinned`), so it
/// costs no attacker query.
///
/// The extinction guard stays first, so a stale [`StateInfo`] can still only
/// produce `false`.
pub(crate) fn has_legal_move_in_view(view: &PositionView, state: &StateInfo) -> bool {
    // Game already over (extinction): no legal moves, matching
    // `generate_legal_with_state`.
    if state.commoners_count == 0 || state.them_commoners_count == 0 {
        return false;
    }

    if quiet_legal_witness(view, state.checkers, state.pinned).is_some() {
        return true;
    }
    has_legal_move_ladder(view, state)
}

/// The child-side existential-legality query: a view, no [`StateInfo`].
///
/// Exactly `has_legal_move_in_view(view, &populated_state)` for the same view,
/// and exactly `!generate_legal_with_state(..).is_empty()`. The screen derives
/// the two facts a [`StateInfo`] would have supplied
/// ([`checking_pieces_and_pinned`], one pass, no board write), and a
/// [`StateInfo`] is populated only when the screen cannot prove a move.
pub(crate) fn has_legal_move_in_view_unpopulated(view: &PositionView) -> bool {
    quiet_legal_witness_unpopulated(view).is_some() || has_legal_move_ladder_unpopulated(view)
}

/// The exact fallback of [`has_legal_move_in_view_unpopulated`]: the full ladder
/// on a freshly populated [`StateInfo`].
///
/// Out of line on purpose: it is the cold path, and keeping it away from the
/// screen leaves the hot path free of the population it never needs.
#[inline(never)]
fn has_legal_move_ladder_unpopulated(view: &PositionView) -> bool {
    let mut state = StateInfo::new();
    view.populate_state(&mut state);
    if state.commoners_count == 0 || state.them_commoners_count == 0 {
        return false;
    }
    has_legal_move_ladder(view, &state)
}

/// The exact existence walk: the pseudo-legal stream through the fast-legality
/// ladder, stopping at the first accepted candidate.
///
/// This is the pre-screen implementation of [`has_legal_move_in_view`], without
/// the "game is already over" guard (the callers apply it, which also keeps a
/// stale [`StateInfo`] from turning into a `true`).
fn has_legal_move_ladder(view: &PositionView, state: &StateInfo) -> bool {
    let fast = FastLegality::new(view, state);
    for_each_pseudo_legal_in_view::<()>(view, &mut |m| {
        if fast.is_legal(view, m, state) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })
    .is_break()
}

/// One move the screen can prove legal, or `None` when it cannot prove any.
///
/// `checkers` and `pinned` are `view.checkers(view.side_to_move())` and
/// `view.pinned(view.side_to_move())`, taken from the caller's populated
/// [`StateInfo`] so the screen costs no attacker query.
///
/// `Some(m)` is a *proof*: `m` is a legal move of `view`, encoded exactly like
/// [`generate_legal`] encodes it (a promotion push is the queen promotion).
/// `None` means "not proven", never "no legal move": the callers fall back to
/// [`has_legal_move_ladder`], which stays exact.
///
/// The proof rests on the two facts documented on
/// [`WidenedLegality`](crate::board::WidenedLegality), quoted rather than
/// re-derived:
///
/// 1. Only removals matter for a commoner that stays put. A quiet non-commoner
///    move empties exactly its origin square, so our commoner can only become
///    attacked if that origin is the sole piece between it and an aligned enemy
///    slider — exactly `pinned`. A quiet move from any other origin leaves the
///    commoner as safe as it is now.
/// 2. A quiet non-commoner move triggers no blast, so it can neither explode the
///    mover's own commoner nor the opponent's, and it cannot be a
///    self-explosion.
///
/// With our commoner *not* attacked (`checkers` empty) — or immune because the
/// two commoners touch, which makes `checkers` empty as well — such a move is
/// legal. Nothing else is claimed: commoner moves, captures, en passant,
/// castling, and in-check positions all return `None` here, and the ladder
/// decides them.
fn quiet_legal_witness(view: &PositionView, checkers: Bitboard, pinned: Bitboard) -> Option<Move> {
    let us = view.side_to_move();
    let our_commoners = view.commoners(us);
    // Extinction: no move is legal at all. (The callers guard this too; the
    // screen stays total on its own so it can never contradict them.)
    if our_commoners.is_empty() || view.commoners(us.flip()).is_empty() {
        return None;
    }
    // In check: a quiet move must also resolve the attack. That is the ladder's
    // question, not this screen's.
    if !checkers.is_empty() {
        return None;
    }
    // The only origins a quiet move may not use are the sole blockers of an
    // enemy slider's ray to our commoner (`pinned` holds our pieces only); the
    // mover's commoner is not a non-commoner piece and is never iterated.
    first_quiet_move(view, |from| !pinned.contains(from as usize))
}

/// [`quiet_legal_witness`] for a view that has no populated [`StateInfo`].
///
/// Same contract, same proof. The two facts a [`StateInfo`] would have supplied
/// come from [`checking_pieces_and_pinned`]: one pass over the enemy's aligned
/// sliders yields both the checkers and the sole blockers. A touching enemy
/// commoner makes our commoner immune to every attack, in which case that pass
/// returns two empty sets and any quiet non-commoner move is legal.
fn quiet_legal_witness_unpopulated(view: &PositionView) -> Option<Move> {
    let us = view.side_to_move();
    if view.commoners(us).is_empty() || view.commoners(us.flip()).is_empty() {
        return None;
    }
    let (checkers, pinned) = checking_pieces_and_pinned(view, us);
    if !checkers.is_empty() {
        return None;
    }
    first_quiet_move(view, |from| !pinned.contains(from as usize))
}

/// The enemy pieces checking the commoner of `us`, and the pieces of `us` that
/// are the sole blocker between that commoner and an aligned enemy slider.
///
/// Equal to `view.checkers(us)` / `view.pinned(us)` (see
/// [`PositionView::checkers`] for the immunity and extinction cases), computed
/// in one pass because both answers read the same thing: with the empty-
/// occupancy rays, an aligned enemy slider is a checker when nothing stands
/// between it and the commoner and a pinning piece is the single blocker when
/// exactly one square is occupied there.
///
/// Returns two empty sets when either side has no commoner or the two commoners
/// touch; both facts are then "no check, no pin" as documented on
/// [`PositionView::checkers`], and immunity means no origin has to be excluded.
fn checking_pieces_and_pinned(view: &PositionView, us: Color) -> (Bitboard, Bitboard) {
    let them = us.flip();
    let commoners = view.commoners(us);
    let them_commoners = view.commoners(them);
    if commoners.is_empty() || them_commoners.is_empty() {
        return (Bitboard::EMPTY, Bitboard::EMPTY);
    }
    let ksq = commoners.lsb();
    if !(them_commoners & attacks::king_attacks(ksq)).is_empty() {
        return (Bitboard::EMPTY, Bitboard::EMPTY);
    }

    let occupied = view.occupied();
    let enemy = view.pieces_color(them);
    let mut checkers = (attacks::knight_attacks(ksq) & view.pieces_pt(PieceType::Knight) & enemy)
        | (attacks::pawn_attacks(us, ksq) & view.pieces_pt(PieceType::Pawn) & enemy);
    let mut snipers = (((view.pieces_pt(PieceType::Rook) | view.pieces_pt(PieceType::Queen))
        & attacks::rook_attacks(ksq, Bitboard::EMPTY))
        | ((view.pieces_pt(PieceType::Bishop) | view.pieces_pt(PieceType::Queen))
            & attacks::bishop_attacks(ksq, Bitboard::EMPTY)))
        & enemy;

    let mut pinned = Bitboard::EMPTY;
    while !snipers.is_empty() {
        let sniper = snipers.pop_lsb();
        let between = between_bb(ksq, sniper) & occupied;
        match between.count() {
            // Nothing between: the slider's ray reaches the commoner.
            0 => checkers = checkers | Bitboard::square_bb(sniper),
            // Exactly one piece between: it is the sole blocker (`pinned` keeps
            // our own pieces, as `PositionView::pinned` documents).
            1 => pinned = pinned | (between & view.pieces_color(us)),
            _ => {}
        }
    }
    (checkers, pinned)
}

/// The first quiet non-commoner move whose origin `accept` allows, in
/// [`for_each_pseudo_legal`]'s piece order (pawns, knights, bishops, rooks,
/// queens).
///
/// Only quiet moves are produced: the destination is always empty, so no blast
/// is triggered and the move's only effect on the placement is emptying
/// `origin`. Pawn moves are the single push (a promotion push is encoded as the
/// generator's queen promotion); double pushes are never needed, because a legal
/// double push implies a legal single push.
///
/// Pawn pushes are computed for the whole pawn set with two shifts, so the
/// common case (some pawn can push from an allowed origin) costs no per-piece
/// loop and no branching per pawn.
fn first_quiet_move(view: &PositionView, accept: impl Fn(Square) -> bool) -> Option<Move> {
    let us = view.side_to_move();
    let occupied = view.occupied();
    let empty = !occupied;
    let promo_from_rank = match us {
        Color::White => Rank::R7,
        Color::Black => Rank::R2,
    };
    let push = |bb: Bitboard| match us {
        Color::White => bb << 8,
        Color::Black => bb >> 8,
    };

    let pawns = view.pieces_color_pt(us, PieceType::Pawn);
    let mut pushes = push(pawns) & empty;
    while !pushes.is_empty() {
        let to = pushes.pop_lsb();
        // A push is vertical, so the origin follows from the destination.
        let from = match us {
            Color::White => Square::from_index(to as i8 - 8),
            Color::Black => Square::from_index(to as i8 + 8),
        };
        if !accept(from) {
            continue;
        }
        return Some(if rank_of(from) == promo_from_rank {
            Move::make_promotion(from, to, PROMOTION_PIECES[0])
        } else {
            Move::make_move(from, to)
        });
    }

    let mut knights = view.pieces_color_pt(us, PieceType::Knight);
    while !knights.is_empty() {
        let from = knights.pop_lsb();
        let to = attacks::knight_attacks(from) & empty;
        if to.is_empty() || !accept(from) {
            continue;
        }
        return Some(Move::make_move(from, to.lsb()));
    }

    // Sliders: one attack lookup per piece, the first quiet destination wins.
    let mut bishops = view.pieces_color_pt(us, PieceType::Bishop);
    while !bishops.is_empty() {
        let from = bishops.pop_lsb();
        let to = attacks::bishop_attacks(from, occupied) & empty;
        if to.is_empty() || !accept(from) {
            continue;
        }
        return Some(Move::make_move(from, to.lsb()));
    }
    let mut rooks = view.pieces_color_pt(us, PieceType::Rook);
    while !rooks.is_empty() {
        let from = rooks.pop_lsb();
        let to = attacks::rook_attacks(from, occupied) & empty;
        if to.is_empty() || !accept(from) {
            continue;
        }
        return Some(Move::make_move(from, to.lsb()));
    }
    let mut queens = view.pieces_color_pt(us, PieceType::Queen);
    while !queens.is_empty() {
        let from = queens.pop_lsb();
        let to = attacks::queen_attacks(from, occupied) & empty;
        if to.is_empty() || !accept(from) {
            continue;
        }
        return Some(Move::make_move(from, to.lsb()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::position::child_view;

    #[test]
    fn test_no_legal_moves_when_game_is_over() {
        // The opponent has no commoner left, so the game is already over and
        // no moves are generated. This matches Fairy-Stockfish's
        // `generate<LEGAL>`, which returns an empty list for an immediate game
        // end.
        for fen in ["8/8/8/8/8/8/8/K7 w - - 0 1", "8/8/8/8/8/8/8/KQ6 w - - 0 1"] {
            let board = Board::from_fen(fen).unwrap();
            let mut moves = MoveList::new();
            generate_legal(&board, &mut moves);
            assert!(moves.is_empty(), "{fen}");
            assert!(!has_legal_move(&board), "{fen}");
        }
    }

    #[test]
    fn test_starting_position_move_count() {
        let board = Board::new();
        let mut moves = MoveList::new();
        generate_pseudo_legal(&board, &mut moves);
        let mut legal_moves = MoveList::new();
        generate_legal(&board, &mut legal_moves);
        // Standard starting position: 20 legal moves
        assert_eq!(legal_moves.len(), 20);
    }

    #[test]
    fn test_knight_moves_start() {
        let board = Board::new();
        let mut moves = MoveList::new();
        generate_pseudo_legal(&board, &mut moves);
        let knight_moves: Vec<Move> = moves
            .as_slice()
            .iter()
            .filter(|&&m| {
                let from = m.from_sq();
                from == Square::B1 || from == Square::G1
            })
            .copied()
            .collect();
        // Each knight has 2 moves from starting position
        assert_eq!(knight_moves.len(), 4);
    }

    #[test]
    fn test_generate_legal_with_state() {
        let board = Board::new();
        let mut state = StateInfo::new();
        board.populate_state(&mut state);

        let mut moves1 = MoveList::new();
        generate_legal(&board, &mut moves1);

        let mut moves2 = MoveList::new();
        generate_legal_with_state(&board, &state, &mut moves2);

        assert_eq!(moves1.len(), moves2.len());
        let mut v1: Vec<String> = moves1.as_slice().iter().map(|&m| m.to_uci()).collect();
        let mut v2: Vec<String> = moves2.as_slice().iter().map(|&m| m.to_uci()).collect();
        v1.sort();
        v2.sort();
        assert_eq!(v1, v2);
    }

    /// Deterministic xorshift64 PRNG for the equivalence playout test.
    struct Xorshift(u64);

    impl Xorshift {
        fn new(seed: u64) -> Self {
            Xorshift(seed | 1)
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

    /// Assert the equivalence contract at `board` and recurse along a random
    /// legal move, checking every position visited during the playout.
    fn check_and_playout(board: &mut Board, rng: &mut Xorshift, depth: u32, visited: &mut u64) {
        *visited += 1;

        let mut state = StateInfo::new();
        board.populate_state(&mut state);

        let mut list = MoveList::new();
        generate_legal_with_state(board, &state, &mut list);

        // Contract: has_legal_move_with_state == (legal list non-empty).
        assert_eq!(
            has_legal_move_with_state(board, &state),
            !list.is_empty(),
            "equivalence violated at {} (depth {depth})",
            board.fen()
        );

        // Two independently generated legal lists must agree (order included).
        let mut list2 = MoveList::new();
        generate_legal(board, &mut list2);
        assert_eq!(
            list.as_slice(),
            list2.as_slice(),
            "legal list mismatch at {}",
            board.fen()
        );

        // The convenience wrapper must agree with the `_with_state` variant.
        assert_eq!(
            has_legal_move(board),
            !list.is_empty(),
            "wrapper mismatch at {}",
            board.fen()
        );

        if depth == 0 || list.is_empty() {
            return;
        }

        let idx = (rng.next() % list.len() as u64) as usize;
        let m = list[idx];
        board.do_move(m, &mut state);
        check_and_playout(board, rng, depth - 1, visited);
        board.undo_move(m, &state);
    }

    #[test]
    fn has_legal_move_equivalence_random_playouts() {
        const START_FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            // Capture-rich middlegame: explosions change occupancy mid-legal().
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
                let mut rng = Xorshift::new(0x9E37_79B9_7F4A_7C15 ^ ((i as u64) << 32) ^ seed);
                check_and_playout(&mut board, &mut rng, 300, &mut visited);
            }
        }
        assert!(
            visited > 5000,
            "expected several thousand positions, got {visited}"
        );
        assert_eq!(visited, 8_592);
    }

    #[test]
    fn has_legal_move_deterministic_fixtures() {
        // (name, fen, expected has_legal_move, extra assertions closure)
        struct Fixture {
            name: &'static str,
            fen: &'static str,
            expected: bool,
        }

        let fixtures = [
            // Bare checkmate (fool's mate): no legal moves.
            Fixture {
                name: "checkmate",
                fen: "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
                expected: false,
            },
            // Stalemate: white's only commoner is enclosed by knights; every
            // "escape" is a capture that self-destructs it. No check.
            Fixture {
                name: "stalemate",
                fen: "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
                expected: false,
            },
            // In check with exactly one escape (h8g7): g8 covered by the f7
            // pawn, h7 by the rook.
            Fixture {
                name: "in_check_one_escape",
                fen: "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
                expected: true,
            },
            // En passant is the only pseudo-legal move, and it is legal: the
            // king is walled in by enemy pieces whose capture would
            // self-destruct it, and the pawn's push is blocked.
            Fixture {
                name: "en_passant_only",
                fen: "8/8/8/PP6/kP1Pp3/PN2P3/8/6K1 b - d3 0 1",
                expected: true,
            },
            // Castling is legal (though it can never be the *only* legal move
            // in this ruleset: the rook's quiet move along the back rank is
            // always available).
            Fixture {
                name: "castling_available",
                fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
                expected: true,
            },
            // Promotion-heavy position.
            Fixture {
                name: "promotion_heavy",
                fen: "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
                expected: true,
            },
            // Extinction: side to move has zero commoners.
            Fixture {
                name: "extinction",
                fen: "8/8/8/8/8/8/8/k7 w - - 0 1",
                expected: false,
            },
            // Bare commoners, K vs K: quiet commoner moves are legal.
            Fixture {
                name: "bare_commoners",
                fen: "k7/8/8/8/8/8/8/7K w - - 0 1",
                expected: true,
            },
            // Every pseudo-legal move is by the pinned e4 rook (pinned by the
            // e8 queen) or the in-check commoner; nothing is legal. The rook
            // is also pinned against moving at all: any e-file move leaves
            // the b4 bishop checking, and Rxc4 opens the e8 queen's file.
            Fixture {
                name: "all_pinned",
                fen: "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
                expected: false,
            },
            // Captures-only: the only legal moves are the e5 pawn captures;
            // the commoner's "moves" are self-destructing captures.
            Fixture {
                name: "captures_only",
                fen: "k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1",
                expected: true,
            },
        ];

        for f in &fixtures {
            let board = Board::from_fen(f.fen).expect("fixture FEN must parse");
            let mut state = StateInfo::new();
            board.populate_state(&mut state);

            let mut legal = MoveList::new();
            generate_legal_with_state(&board, &state, &mut legal);

            // Self-validating: the expected value is derived from the legal
            // move list, and additionally pinned to the documented value.
            assert_eq!(
                has_legal_move_with_state(&board, &state),
                !legal.is_empty(),
                "fixture {}: contract violated",
                f.name
            );
            assert_eq!(
                has_legal_move_with_state(&board, &state),
                f.expected,
                "fixture {}: unexpected result",
                f.name
            );
            assert_eq!(
                has_legal_move(&board),
                f.expected,
                "fixture {}: wrapper mismatch",
                f.name
            );
            let uci: Vec<String> = legal.as_slice().iter().map(|&m| m.to_uci()).collect();

            match f.name {
                "in_check_one_escape" => {
                    assert_eq!(uci, vec!["h8g7".to_string()]);
                }
                "en_passant_only" => {
                    assert_eq!(uci, vec!["e4d3".to_string()]);
                }
                "castling_available" => {
                    assert!(uci.contains(&"e1g1".to_string()));
                    assert!(uci.contains(&"e1c1".to_string()));
                }
                "promotion_heavy" => {
                    let promos = uci.iter().filter(|s| s.len() == 5).count();
                    assert_eq!(promos, 12, "expected 12 promotion moves");
                }
                "captures_only" => {
                    assert!(!uci.is_empty());
                    assert!(
                        uci.iter()
                            .all(|s| s == &"e5d6".to_string() || s == &"e5f6".to_string()),
                        "expected only e5 pawn captures, got {uci:?}"
                    );
                }
                _ => {}
            }
        }
    }

    const STREAM_CORPUS: &[&str] = &[
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        "3k4/8/8/8/8/8/8/3QK3 b - - 0 1",
        "4k3/8/8/8/8/4r3/4N3/4K3 w - - 0 1",
        "8/8/8/8/8/8/8/k7 w - - 0 1",
        "k7/8/8/8/8/8/8/7K w - - 0 1",
        "rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5",
        "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15",
    ];

    #[test]
    fn for_each_pseudo_legal_matches_list() {
        for fen in STREAM_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut expected = MoveList::new();
            generate_pseudo_legal(&board, &mut expected);

            let mut streamed = Vec::new();
            let flow = for_each_pseudo_legal::<()>(&board, &mut |m| {
                streamed.push(m);
                ControlFlow::Continue(())
            });
            assert!(flow.is_continue());
            assert_eq!(streamed, expected.as_slice(), "{fen}");
        }
    }

    #[test]
    fn legal_stream_matches_legal_list() {
        // The legal stream must visit exactly the legal list, in order: both go
        // through the same fast-legality ladder.
        for fen in STREAM_CORPUS {
            let board = Board::from_fen(fen).expect("corpus FEN must parse");
            let mut expected = MoveList::new();
            generate_legal(&board, &mut expected);

            let mut streamed = Vec::new();
            let flow = for_each_legal::<()>(&board, &mut |m| {
                streamed.push(m);
                ControlFlow::Continue(())
            });
            assert!(flow.is_continue());
            assert_eq!(streamed, expected.as_slice(), "{fen}");
            assert_eq!(has_legal_move(&board), !expected.is_empty(), "{fen}");
        }
    }

    #[test]
    fn for_each_pseudo_legal_early_exit() {
        let board = Board::new();
        let mut count = 0usize;
        let flow = for_each_pseudo_legal::<()>(&board, &mut |_| {
            count += 1;
            ControlFlow::Break(())
        });
        assert!(flow.is_break());
        assert_eq!(count, 1);

        let mut full = 0usize;
        let flow = for_each_pseudo_legal::<()>(&board, &mut |_| {
            full += 1;
            ControlFlow::Continue(())
        });
        assert!(flow.is_continue());
        assert_eq!(full, 20);
    }

    #[test]
    fn for_each_pseudo_legal_order_matches_golden() {
        let board = Board::new();
        let mut uci = Vec::new();
        let _ = for_each_pseudo_legal::<()>(&board, &mut |m| {
            uci.push(m.to_uci());
            ControlFlow::Continue(())
        });
        assert_eq!(
            uci,
            [
                "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4",
                "f2f3", "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
            ]
        );
    }

    /// The 41-position perft oracle corpus plus the stream fixtures, so the
    /// screen is exercised on real and on artificial positions.
    fn oracle_corpus() -> Vec<String> {
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
        for fen in STREAM_CORPUS {
            fens.push((*fen).to_owned());
        }
        fens
    }

    fn populated(board: &Board) -> StateInfo {
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        state
    }

    /// Visit `view` as a position the child path would see: the differential
    /// against the populated state, the witness soundness, and the counters.
    ///
    /// `coverage` is `(positions with a legal move and no checker, of which the
    /// screen proved one)`.
    fn check_view(
        view: &PositionView,
        checked: &mut u64,
        coverage: &mut (u64, u64),
        state_coverage: &mut (u64, u64),
        witness_hits: &mut [u64; PieceType::NB],
        decided: &mut u64,
    ) {
        let mut state = StateInfo::new();
        view.populate_state(&mut state);
        let mut legal = MoveList::new();
        generate_legal_in_view(view, &state, &mut legal);

        let expected = !legal.is_empty();
        let us = view.side_to_move();
        let witness = quiet_legal_witness_unpopulated(view);
        let witness_state = quiet_legal_witness(view, state.checkers, state.pinned);

        // The direct differential of the light entry point.
        assert_eq!(
            has_legal_move_in_view_unpopulated(view),
            has_legal_move_in_view(view, &state),
            "light vs state existence"
        );
        assert_eq!(
            has_legal_move_in_view_unpopulated(view),
            expected,
            "light existence vs legal list"
        );
        assert_eq!(
            has_legal_move_in_view(view, &state),
            expected,
            "state existence vs legal list"
        );

        // Soundness: a witness is a proof, so it must be a member of the legal
        // list (the encodings are the generator's, so equality is exact).
        for (name, candidate) in [("light", witness), ("state", witness_state)] {
            if let Some(m) = candidate {
                assert!(
                    legal.as_slice().contains(&m),
                    "{name} witness {m} is not in the legal list"
                );
            }
        }
        if let Some(m) = witness {
            let pt = view
                .type_on(m.from_sq())
                .expect("a witness origin holds a piece");
            witness_hits[pt as usize] += 1;
        }

        *checked += 1;
        if expected && view.checkers(us).is_empty() {
            coverage.0 += 1;
            if witness.is_some() {
                coverage.1 += 1;
            }
            state_coverage.0 += 1;
            if witness_state.is_some() {
                state_coverage.1 += 1;
            }
        }
        if !expected {
            *decided += 1;
        }
    }

    /// Every corpus position, every child of every corpus position, and a
    /// seeded play-out walk: the light existence query equals the state-based
    /// one, every witness is a legal move, and the screen actually fires.
    #[test]
    fn existence_screen_matches_legal_lists() {
        let mut checked = 0u64;
        let mut coverage = (0u64, 0u64);
        let mut state_coverage = (0u64, 0u64);
        let mut witness_hits = [0u64; PieceType::NB];
        let mut decided = 0u64;

        for fen in oracle_corpus() {
            let board = Board::from_fen(&fen).expect("corpus FEN must parse");
            let view = board.position_view();
            check_view(
                view,
                &mut checked,
                &mut coverage,
                &mut state_coverage,
                &mut witness_hits,
                &mut decided,
            );

            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                let child = child_view(view, m, board.rule50_after(m))
                    .expect("child of a pseudo-legal move")
                    .view;
                check_view(
                    &child,
                    &mut checked,
                    &mut coverage,
                    &mut state_coverage,
                    &mut witness_hits,
                    &mut decided,
                );
            }
        }

        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut visited = 0u64;
        let start_fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        ];
        for (i, fen) in start_fens.iter().enumerate() {
            for walk in 0..4u64 {
                let mut board = Board::from_fen(fen).expect("play-out FEN must parse");
                let mut state = StateInfo::new();
                seed ^= (i as u64) << 32 ^ walk;
                for _ in 0..120 {
                    board.populate_state(&mut state);
                    check_view(
                        board.position_view(),
                        &mut checked,
                        &mut coverage,
                        &mut state_coverage,
                        &mut witness_hits,
                        &mut decided,
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
        }

        eprintln!(
            "existence screen: {checked} views ({visited} play-out positions), \
             {decided} views without a legal move",
            checked = checked,
            visited = visited,
            decided = decided,
        );
        eprintln!(
            "  coverage (legal move, no checker): light {}/{} ({:.1} %), state {}/{} ({:.1} %)",
            coverage.1,
            coverage.0,
            100.0 * coverage.1 as f64 / coverage.0 as f64,
            state_coverage.1,
            state_coverage.0,
            100.0 * state_coverage.1 as f64 / state_coverage.0 as f64,
        );
        eprintln!(
            "  witnesses by piece type: {:?}",
            &witness_hits[..PieceType::NB]
        );
        // Every piece loop of the screen must be reachable; a bishop witness is
        // rare only because the pawn screen answers first.
        for pt in [
            PieceType::Pawn,
            PieceType::Knight,
            PieceType::Rook,
            PieceType::Queen,
        ] {
            assert!(
                witness_hits[pt as usize] > 5,
                "the screen fired only {} times for {pt:?}",
                witness_hits[pt as usize]
            );
        }
        assert!(
            witness_hits[PieceType::Bishop as usize] > 0,
            "no bishop witness"
        );
        assert!(checked > 2_000, "expected many views, got {checked}");
        assert!(decided > 20, "expected decided views, got {decided}");
        assert!(
            coverage.0 > 1_000,
            "expected many undecided, in-check-free views, got {}",
            coverage.0
        );
        assert!(
            10 * coverage.1 >= 95 * coverage.0 / 10,
            "the screen fires too rarely: {}/{}",
            coverage.1,
            coverage.0
        );
    }

    /// The classes handoff 3 names, with the documented witness behaviour: the
    /// screen proves only quiet non-commoner moves, and every other class is
    /// decided by the exact ladder.
    #[test]
    fn existence_screen_fixtures() {
        use crate::movegen::generate_legal_with_state;

        // (name, fen, expected existence, the screen fires)
        let fixtures = [
            (
                "only commoner and pawns",
                "8/8/8/4k3/8/4P3/8/4K3 w - - 0 1",
                true,
                true,
            ),
            (
                "blocked-pawn ray shield",
                "8/8/8/8/8/4k3/4P3/4K3 b - - 0 1",
                true,
                false,
            ),
            (
                "touching commoners",
                "8/8/8/8/8/8/1k6/2K5 w - - 0 1",
                true,
                false,
            ),
            (
                "in check, one escape",
                "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
                true,
                false,
            ),
            (
                "captures only",
                "k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1",
                true,
                false,
            ),
            (
                "checkmate",
                "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
                false,
                false,
            ),
            ("stalemate", "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1", false, false),
            (
                "bare commoners (K vs K)",
                "k7/8/8/8/8/8/8/7K w - - 0 1",
                true,
                false,
            ),
            // The screen's piece loops, one fixture per mover type: a lone
            // piece is the only quiet mover, so the witness must come from it.
            ("knight", "7k/8/8/8/8/8/8/N3K3 w - - 0 1", true, true),
            ("bishop", "7k/8/8/8/8/8/8/2B1K3 w - - 0 1", true, true),
            ("rook", "7k/8/8/8/8/8/8/R3K3 w - - 0 1", true, true),
            ("queen", "7k/8/8/8/8/8/8/3QK3 w - - 0 1", true, true),
            // A promotion push is quiet, so the witness is the queen promotion.
            (
                "promotion push",
                "7k/4P3/8/8/8/8/8/4K3 w - - 0 1",
                true,
                true,
            ),
            // The e2 pawn is the sole blocker between the e8 rook and the e1
            // commoner, so its push is illegal and the witness must be the
            // a1 knight's move instead.
            (
                "pinned pawn, knight available",
                "4r2k/8/8/8/8/8/4P3/N3K3 w - - 0 1",
                true,
                true,
            ),
        ];

        for (name, fen, expected, screen_fires) in fixtures {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let state = populated(&board);
            let mut legal = MoveList::new();
            generate_legal_with_state(&board, &state, &mut legal);
            assert_eq!(legal.is_empty(), !expected, "{name}: legal list");

            // Both entry points, and the direct differential.
            assert_eq!(
                has_legal_move_in_view_unpopulated(board.position_view()),
                expected,
                "{name}: light existence"
            );
            assert_eq!(
                has_legal_move_in_view(board.position_view(), &state),
                expected,
                "{name}: state existence"
            );
            assert_eq!(
                quiet_legal_witness_unpopulated(board.position_view()).is_some(),
                screen_fires,
                "{name}: screen fires"
            );
            assert_eq!(
                quiet_legal_witness(board.position_view(), state.checkers, state.pinned).is_some(),
                screen_fires,
                "{name}: state screen fires"
            );

            // A witness is a proof: it must be a member of the legal list, in
            // both screens. Encoding equality is exact (the screen uses the
            // generator's encodings, queen promotion included).
            for (screen, witness) in [
                (
                    "light",
                    quiet_legal_witness_unpopulated(board.position_view()),
                ),
                (
                    "state",
                    quiet_legal_witness(board.position_view(), state.checkers, state.pinned),
                ),
            ] {
                if let Some(m) = witness {
                    assert!(
                        legal.as_slice().contains(&m),
                        "{name}: {screen} witness {m} is not a legal move"
                    );
                }
            }

            if name == "pinned pawn, knight available" {
                let witness = quiet_legal_witness_unpopulated(board.position_view())
                    .expect("the knight has quiet moves");
                assert_eq!(
                    witness.from_sq(),
                    Square::A1,
                    "the pinned pawn may not be claimed"
                );
            }
        }
    }

    /// The checking/pinned split is exactly the two board queries it replaces,
    /// including the immunity and extinction branches.
    #[test]
    fn checking_pieces_and_pinned_matches_view_queries() {
        let mut checked = 0u64;
        let mut with_checkers = 0u64;
        let mut with_pins = 0u64;
        for fen in oracle_corpus() {
            let board = Board::from_fen(&fen).expect("corpus FEN must parse");
            let view = board.position_view();
            let mut views = vec![*view];
            let mut pseudo = MoveList::new();
            generate_pseudo_legal(&board, &mut pseudo);
            for &m in pseudo.as_slice() {
                views.push(
                    child_view(view, m, board.rule50_after(m))
                        .expect("child of a pseudo-legal move")
                        .view,
                );
            }
            for view in views {
                for us in [Color::White, Color::Black] {
                    let (checkers, pinned) = checking_pieces_and_pinned(&view, us);
                    if us == view.side_to_move() {
                        // The single-commoner case is the one the screen uses.
                        assert_eq!(checkers, view.checkers(us), "{fen}: checkers");
                        if view.commoners(Color::White).count() == 1
                            && view.commoners(Color::Black).count() == 1
                        {
                            assert_eq!(pinned, view.pinned(us), "{fen}: pinned");
                        }
                    }
                    if !checkers.is_empty() {
                        with_checkers += 1;
                    }
                    if !pinned.is_empty() {
                        with_pins += 1;
                    }
                    checked += 1;
                }
            }
        }
        eprintln!(
            "checking/pinned split: {checked} (view, color) pairs, \
             {with_checkers} with checkers, {with_pins} with pins"
        );
        assert!(with_checkers > 10, "expected checked positions");
        assert!(with_pins > 10, "expected pinned pieces");
    }
}
