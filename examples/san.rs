//! Print the legal moves of a position with their SAN and UCI spelling.
//!
//! ```sh
//! cargo run --example san "FEN"
//! cargo run --example san "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
//! ```
//!
//! Each line is `SAN<TAB>UCI`, in generation order. Feeding the SAN column back
//! through [`Board::move_from_san`] must reproduce the UCI column.

use atomic_movegen::movegen::generate_legal;
use atomic_movegen::{Board, MoveList};

fn main() {
    let fen = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned());
    let board = Board::from_fen(&fen).unwrap_or_else(|e| {
        eprintln!("invalid FEN: {e}");
        std::process::exit(1);
    });

    let mut moves = MoveList::new();
    generate_legal(&board, &mut moves);
    for &m in moves.as_slice() {
        let san = board
            .san(m)
            .expect("generated moves are legal, so SAN always exists");
        let roundtrip = board.move_from_san(&san);
        let marker = if roundtrip == Some(m) { "" } else { "  (!)" };
        println!("{san}\t{m}{marker}");
    }
    println!("# {} legal moves", moves.len());
}
