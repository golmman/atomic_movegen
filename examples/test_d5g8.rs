use atomic_movegen::attacks::{bishop_attacks, queen_attacks};
use atomic_movegen::board::Board;
use atomic_movegen::types::{Bitboard, Move, MoveList, Square};

fn main() {
    let board = Board::from_fen("rnbqkbnr/ppp1p2p/3p1pp1/3Q4/8/4P3/PPPP1PPP/RNB1KBNR w KQkq - 0 4").unwrap();
    let occupied = board.occupied();
    println!("occupied = 0x{:016x}", occupied.0);
    let b = bishop_attacks(Square::D5, occupied);
    println!("bishop_attacks(d5, occ) = 0x{:016x}", b.0);
    println!("contains g8: {}", (b & Bitboard::square_bb(Square::G8)) != Bitboard::EMPTY);
    println!("contains f7: {}", (b & Bitboard::square_bb(Square::F7)) != Bitboard::EMPTY);
    let q = queen_attacks(Square::D5, occupied);
    println!("queen_attacks(d5, occ) = 0x{:016x}", q.0);
    println!("queen contains g8: {}", (q & Bitboard::square_bb(Square::G8)) != Bitboard::EMPTY);

    // manual ray-cast
    let mut manual = 0u64;
    let sq = Square::D5;
    let s_idx = sq as i8;
    let sf = s_idx % 8;
    let sr = s_idx / 8;
    for (df, dr) in [(1i8, 1i8), (1i8, -1i8), (-1i8, 1i8), (-1i8, -1i8)].iter() {
        let mut f = sf + *df;
        let mut r = sr + *dr;
        while f >= 0 && f < 8 && r >= 0 && r < 8 {
            let idx = (r * 8 + f) as usize;
            manual |= 1u64 << idx;
            if occupied.0 & (1u64 << idx) != 0 {
                break;
            }
            f += *df;
            r += *dr;
        }
    }
    println!("manual bishop_attacks(d5, occ) = 0x{:016x}", manual);
    println!("manual contains g8: {}", (manual & (1u64 << (Square::G8 as usize))) != 0);
    println!("manual contains f7: {}", (manual & (1u64 << (Square::F7 as usize))) != 0);

    let mut moves = MoveList::new();
    atomic_movegen::movegen::generate_pseudo_legal(&board, &mut moves);
    let v: Vec<_> = moves.as_slice().iter().map(|m| m.to_uci()).collect();
    println!("pseudo-legal count: {}", moves.len());
    for m in &v {
        if m.starts_with("d5") {
            println!("{}", m);
        }
    }
    let d5g8 = Move::make_move(Square::D5, Square::G8);
    println!("has d5g8: {}", moves.as_slice().iter().any(|&m| m == d5g8));
    let d5g7 = Move::make_move(Square::D5, Square::G7);
    println!("has d5g7: {}", moves.as_slice().iter().any(|&m| m == d5g7));
}
