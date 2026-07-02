use atomic_movegen::board::Board;
use atomic_movegen::movegen;
use atomic_movegen::types::MoveList;
use std::env;

fn main() {
    atomic_movegen::attacks::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: list_moves <fen>");
        return;
    }
    let fen = &args[1];
    let board = Board::from_fen(fen).expect("Invalid FEN");

    let mut moves = MoveList::new();
    movegen::generate_legal(&board, &mut moves);
    moves
        .as_mut_slice()
        .sort_by_key(|m| (m.from_sq() as u16, m.to_sq() as u16));

    println!("Legal moves ({} total):", moves.len());
    for &m in moves.as_slice() {
        println!("  {}{}", sq_str(m.from_sq()), sq_str(m.to_sq()));
    }
}

fn sq_str(sq: atomic_movegen::types::Square) -> String {
    let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    let idx = sq as usize;
    format!("{}{}", files[idx % 8], (idx / 8 + 1))
}
