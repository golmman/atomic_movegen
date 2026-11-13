//! Pseudo-legal and legal move generation for atomic chess.

use crate::attacks;
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
/// `populate_state` itself. The output preserves the pseudo-legal order; see
/// the generation-order contract on [`for_each_pseudo_legal`].
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
/// not call `populate_state` itself.
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
/// does not call `populate_state` itself.
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
pub(crate) fn has_legal_move_in_view(view: &PositionView, state: &StateInfo) -> bool {
    // Game already over (extinction): no legal moves, matching
    // `generate_legal_with_state`.
    if state.commoners_count == 0 || state.them_commoners_count == 0 {
        return false;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;

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
}
