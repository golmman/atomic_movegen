use crate::attacks;
use crate::board::{BK_CASTLE, BQ_CASTLE, Board, StateInfo, WK_CASTLE, WQ_CASTLE};
use crate::types::*;

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

    // Knight moves
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

    // Bishop moves
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

    // Rook moves
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

    // Queen moves
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

    // Commoner (king) moves
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
    let from_file = file_of(from) as i8;

    let (push_dir, push_double, start_rank, promo_rank, _to_rank, _to_file_inc) = match us {
        Color::White => (8i8, 16i8, Rank::R2, Rank::R8, Rank::R3, 1i8),
        Color::Black => (-8i8, -16i8, Rank::R7, Rank::R1, Rank::R6, -1i8),
    };

    let from_idx = from as i8;

    // Single push
    let to_idx = from_idx + push_dir;
    let to_sq = Square::from_index(to_idx);
    if to_sq != Square::NONE && board.empty(to_sq) {
        if rank_of(to_sq) == promo_rank {
            for &pt in &[
                PieceType::Queen,
                PieceType::Rook,
                PieceType::Bishop,
                PieceType::Knight,
            ] {
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
        let target_f = from_file + df;
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
                for &pt in &[
                    PieceType::Queen,
                    PieceType::Rook,
                    PieceType::Bishop,
                    PieceType::Knight,
                ] {
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
        if ep_f == from_file - 1 || ep_f == from_file + 1 {
            let df = ep_f - from_file;
            let to_idx = from_idx + push_dir + df;
            let to_sq = Square::from_index(to_idx);
            if to_sq == ep_sq {
                moves.push(Move::make_enpassant(from, ep_sq));
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
    if board.castling_rights() & king_side_right != 0 {
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
    if board.castling_rights() & queen_side_right != 0 {
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

pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    generate_pseudo_legal(board, moves);
    moves.retain(|m| board.legal(m, &state));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;

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
}
