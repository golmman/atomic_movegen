use atomic_movegen::board::Board;
use atomic_movegen::movegen;
use atomic_movegen::types::*;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: pawn_debug <fen>");
        return;
    }
    let fen = &args[1];
    let board = Board::from_fen(fen).expect("Invalid FEN");

    println!("Board side to move: {:?}", board.side_to_move());

    // Manual check: iterate all pawns
    let us = board.side_to_move();
    let _them = us.flip();
    let _occupied = board.occupied();

    let pawns = board.pieces_color_pt(us, PieceType::Pawn);
    println!("Pawns ({:?}): {}", us, pawns.count());
    let mut p = pawns;
    while !p.is_empty() {
        let sq = p.pop_lsb();
        println!(
            "  Pawn at {} (idx={}): file={} rank={}",
            sq_str(sq),
            sq as u8,
            file_of(sq) as u8,
            rank_of(sq) as u8
        );
    }

    // Check what generate_legal produces
    let mut moves = MoveList::new();
    movegen::generate_legal(&board, &mut moves);

    // Find all h3g2 moves
    let h3 = Square::from_index(23);
    let g2 = Square::from_index(14);
    println!("\nAll h3g2 moves:");
    for (i, &m) in moves.as_slice().iter().enumerate() {
        if m.from_sq() == h3 && m.to_sq() == g2 {
            println!(
                "  Move #{}: from={} to={} type={:?}",
                i,
                sq_str(m.from_sq()),
                sq_str(m.to_sq()),
                m.move_type()
            );
        }
    }

    // Also check how many times each move appears
    use std::collections::HashMap;
    let mut counts: HashMap<(u16, u16, u16), usize> = HashMap::new();
    for &m in moves.as_slice() {
        *counts
            .entry((m.from_sq() as u16, m.to_sq() as u16, m.move_type() as u16))
            .or_insert(0) += 1;
    }
    for ((from, to, mt), count) in &counts {
        if *count > 1 {
            println!(
                "DUPLICATE: {}{} type={:?} appears {} times",
                sq_str(Square::from_u8(*from as u8)),
                sq_str(Square::from_u8(*to as u8)),
                match mt {
                    0 => "Normal",
                    1 => "Promo",
                    2 => "EP",
                    3 => "Castle",
                    _ => "?",
                },
                count
            );
        }
    }
}

fn sq_str(sq: Square) -> String {
    let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    let idx = sq as usize;
    format!("{}{}", files[idx % 8], (idx / 8 + 1))
}
