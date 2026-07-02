use atomic_movegen::board::Board;
use atomic_movegen::movegen;
use atomic_movegen::types::*;
use std::collections::HashSet;
use std::env;

fn main() {
    atomic_movegen::attacks::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: debug_moves <fen>");
        return;
    }
    let fen = &args[1];
    let board = Board::from_fen(fen).expect("Invalid FEN");

    let mut pseudo = MoveList::new();
    movegen::generate_pseudo_legal(&board, &mut pseudo);

    let mut pseudo_set = HashSet::new();
    for &m in pseudo.as_slice() {
        if !pseudo_set.insert((m.from_sq(), m.to_sq(), m.move_type())) {
            println!(
                "DUPLICATE PSEUDO-LEGAL: {}{}",
                sq_str(m.from_sq()),
                sq_str(m.to_sq())
            );
        }
    }

    let mut legal = MoveList::new();
    movegen::generate_legal(&board, &mut legal);

    let mut legal_set = HashSet::new();
    for &m in legal.as_slice() {
        if !legal_set.insert((m.from_sq(), m.to_sq(), m.move_type())) {
            println!(
                "DUPLICATE LEGAL: {}{}",
                sq_str(m.from_sq()),
                sq_str(m.to_sq())
            );
        }
    }

    println!(
        "Pseudo-legal moves: {} (unique: {})",
        pseudo.len(),
        pseudo_set.len()
    );
    println!("Legal moves: {} (unique: {})", legal.len(), legal_set.len());
}

fn sq_str(sq: Square) -> String {
    let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    let idx = sq as usize;
    format!("{}{}", files[idx % 8], (idx / 8 + 1))
}
