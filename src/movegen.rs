use crate::attacks;
use crate::board::{
    BK_CASTLE, BQ_CASTLE, Board, StateInfo, WK_CASTLE, WQ_CASTLE, is_move_trivially_legal,
};
use crate::types::*;

const PROMOTION_PIECES: [PieceType; 4] = [
    PieceType::Queen,
    PieceType::Rook,
    PieceType::Bishop,
    PieceType::Knight,
];

/// Generate all pseudo-legal moves for the side to move.
///
/// Pseudo-legal means every move that is legal *except* moves that would
/// result in self-explosion (losing the last commoner), castling through
/// check, or leaving a commoner under attack. Use [`generate_legal`] for
/// fully legal moves.
pub fn generate_pseudo_legal(board: &Board, moves: &mut MoveList) {
    let us = board.side_to_move();
    let them = us.flip();
    let occupied = board.occupied();
    let target = !occupied | board.pieces_color(them);

    let mut p = board.pieces_color_pt(us, PieceType::Pawn);
    while !p.is_empty() {
        let from = p.pop_lsb();
        generate_pawn_moves_for(board, us, them, from, moves);
    }

    let mut knights = board.pieces_color_pt(us, PieceType::Knight);
    while !knights.is_empty() {
        let from = knights.pop_lsb();
        let attacks = attacks::knight_attacks(from) & target;
        let mut a = attacks;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }

    let mut bishops = board.pieces_color_pt(us, PieceType::Bishop);
    while !bishops.is_empty() {
        let from = bishops.pop_lsb();
        let attacks = attacks::bishop_attacks(from, occupied) & target;
        let mut a = attacks;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }

    let mut rooks = board.pieces_color_pt(us, PieceType::Rook);
    while !rooks.is_empty() {
        let from = rooks.pop_lsb();
        let attacks = attacks::rook_attacks(from, occupied) & target;
        let mut a = attacks;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }

    let mut queens = board.pieces_color_pt(us, PieceType::Queen);
    while !queens.is_empty() {
        let from = queens.pop_lsb();
        let attacks = attacks::queen_attacks(from, occupied) & target;
        let mut a = attacks;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }

    let mut commoners = board.pieces_color_pt(us, PieceType::Commoner);
    while !commoners.is_empty() {
        let from = commoners.pop_lsb();
        let attacks = attacks::king_attacks(from) & target;
        let mut a = attacks;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }

    // Castling moves
    generate_castling(board, us, moves);
}

fn generate_pawn_moves_for(
    board: &Board,
    us: Color,
    them: Color,
    from: Square,
    moves: &mut MoveList,
) {
    let from_rank = rank_of(from);
    let from_f = file_of(from) as i8;

    let (push_dir, push_double, start_rank, promo_rank) = match us {
        Color::White => (8i8, 16i8, Rank::R2, Rank::R8),
        Color::Black => (-8i8, -16i8, Rank::R7, Rank::R1),
    };

    let from_idx = from as i8;

    // Single push
    let to_idx = from_idx + push_dir;
    let to_sq = Square::from_index(to_idx);
    if to_sq != Square::NONE && board.empty(to_sq) {
        if rank_of(to_sq) == promo_rank {
            for &pt in &PROMOTION_PIECES {
                moves.push(Move::make_promotion(from, to_sq, pt));
            }
        } else {
            moves.push(Move::make_move(from, to_sq));
        }

        // Double push (only from starting rank)
        if from_rank == start_rank {
            let to_idx2 = from_idx + push_double;
            let to_sq2 = Square::from_index(to_idx2);
            if to_sq2 != Square::NONE && board.empty(to_sq2) {
                moves.push(Move::make_move(from, to_sq2));
            }
        }
    }

    // Captures - adjacent files only
    for df in &[-1i8, 1i8] {
        let target_f = from_f + df;
        if !(0..=7).contains(&target_f) {
            continue;
        }
        let to_idx = from_idx + push_dir + df;
        let to_sq = Square::from_index(to_idx);
        if to_sq == Square::NONE {
            continue;
        }
        // Verify correct file (guard against wrapping)
        if file_of(to_sq) as i8 != target_f {
            continue;
        }
        if board.pieces_color(them) & Bitboard::square_bb(to_sq) != Bitboard::EMPTY {
            if rank_of(to_sq) == promo_rank {
                for &pt in &PROMOTION_PIECES {
                    moves.push(Move::make_promotion(from, to_sq, pt));
                }
            } else {
                moves.push(Move::make_move(from, to_sq));
            }
        }
    }

    // En passant
    if let Some(ep_sq) = board.ep_square() {
        let ep_f = file_of(ep_sq) as i8;
        if ep_f == from_f - 1 || ep_f == from_f + 1 {
            let df = ep_f - from_f;
            let to_idx = from_idx + push_dir + df;
            let to_sq = Square::from_index(to_idx);
            if to_sq == ep_sq {
                let ep_cap_idx = match us {
                    Color::White => ep_sq as i8 - 8,
                    Color::Black => ep_sq as i8 + 8,
                };
                let ep_cap = Square::from_index(ep_cap_idx);
                if board.piece_on(ep_cap) == make_piece(them, PieceType::Pawn) {
                    moves.push(Move::make_enpassant(from, ep_sq));
                }
            }
        }
    }
}

fn generate_castling(board: &Board, us: Color, moves: &mut MoveList) {
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

    // King-side castling
    if board.castling_rights() & king_side_right != 0
        && board.piece_on(king_sq) == make_piece(us, PieceType::Commoner)
        && board.piece_on(king_side_rook_sq) == make_piece(us, PieceType::Rook)
    {
        let mut clear = true;
        for &sq in &king_side_squares {
            if !board.empty(sq) {
                clear = false;
                break;
            }
        }
        if clear {
            moves.push(Move::make_castling(king_sq, king_side_rook_sq));
        }
    }

    // Queen-side castling
    if board.castling_rights() & queen_side_right != 0
        && board.piece_on(king_sq) == make_piece(us, PieceType::Commoner)
        && board.piece_on(queen_side_rook_sq) == make_piece(us, PieceType::Rook)
    {
        let mut clear = true;
        for &sq in &queen_side_squares {
            if !board.empty(sq) {
                clear = false;
                break;
            }
        }
        if clear {
            moves.push(Move::make_castling(king_sq, queen_side_rook_sq));
        }
    }
}

/// Generate all fully legal moves for the side to move.
///
/// Wraps [`generate_pseudo_legal`] and filters out illegal moves using
/// [`Board::legal`] and a fast-path trivial-legality check.
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    generate_pseudo_legal(board, moves);

    // In-place compaction: fast-path accept without legal() call.
    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            if is_move_trivially_legal(board, m, &state) || board.legal(m, &state) {
                ms[write_idx] = m;
                write_idx += 1;
            }
        }
        write_idx
    };
    moves.set_len(new_len);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;

    #[test]
    fn test_starting_position_move_count() {
        crate::attacks::init();
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
        crate::attacks::init();
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
}
