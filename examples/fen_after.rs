//! Show the FEN string and legal moves after playing a move from a position.
//!
//! Usage: `fen_after <FEN> <MOVE>` where MOVE is a 4-character string (e.g. e2e4).

use atomic_movegen::board::Board;
use atomic_movegen::movegen;
use atomic_movegen::types::parse_sq;
use atomic_movegen::types::sq_str;
use atomic_movegen::types::*;
use std::env;

fn main() {
    atomic_movegen::attacks::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: fen_after <fen> <move>");
        return;
    }
    let fen = &args[1];
    let movestr = &args[2];
    let mut board = Board::from_fen(fen).expect("Invalid FEN");

    let from_sq = parse_sq(&movestr[0..2]).unwrap_or_else(|| {
        eprintln!("Invalid from square: {}", &movestr[0..2]);
        std::process::exit(1);
    });
    let to_sq = parse_sq(&movestr[2..4]).unwrap_or_else(|| {
        eprintln!("Invalid to square: {}", &movestr[2..4]);
        std::process::exit(1);
    });

    let mut moves = MoveList::new();
    movegen::generate_legal(&board, &mut moves);

    for &m in moves.as_slice() {
        if m.from_sq() == from_sq && m.to_sq() == to_sq {
            let mut state = atomic_movegen::board::StateInfo::new();
            board.do_move(m, &mut state);
            println!("{}", board.fen());

            let mut moves2 = MoveList::new();
            movegen::generate_legal(&board, &mut moves2);
            moves2
                .as_mut_slice()
                .sort_by_key(|m| (m.from_sq() as u16, m.to_sq() as u16));
            println!("Legal moves ({} total):", moves2.len());
            for &m2 in moves2.as_slice() {
                println!(
                    "  {}{}",
                    sq_str(m2.from_sq()).unwrap_or("??"),
                    sq_str(m2.to_sq()).unwrap_or("??")
                );
            }
            return;
        }
    }
    println!("Move not found");
}
