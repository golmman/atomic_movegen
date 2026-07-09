//! Perft with per-move breakdown from a given FEN position.
//!
//! Usage: `perft_divide <FEN> <DEPTH>`

use atomic_movegen::board::Board;
use atomic_movegen::movegen;
use atomic_movegen::perft;
use atomic_movegen::types::MoveList;
use atomic_movegen::types::sq_str;
use std::env;

fn main() {
    atomic_movegen::attacks::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: perft_divide <fen> <depth>");
        return;
    }
    let fen = &args[1];
    let depth: u32 = args[2].parse().unwrap_or(1);

    let mut board = Board::from_fen(fen).expect("Invalid FEN");

    let mut moves = MoveList::new();
    movegen::generate_legal(&board, &mut moves);
    moves
        .as_mut_slice()
        .sort_by_key(|m| (m.from_sq() as u16, m.to_sq() as u16));

    let mut total = 0u64;
    for &m in moves.as_slice() {
        let mut state = atomic_movegen::board::StateInfo::new();
        board.do_move(m, &mut state);
        let cnt = if depth <= 1 {
            1
        } else {
            perft(&mut board, depth - 1)
        };
        board.undo_move(m, &state);
        total += cnt;
        println!(
            "{}{}: {}",
            sq_str(m.from_sq()).unwrap_or("??"),
            sq_str(m.to_sq()).unwrap_or("??"),
            cnt
        );
    }
    println!("\nNodes searched: {}", total);
}
