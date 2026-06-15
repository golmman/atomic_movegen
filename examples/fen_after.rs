use atomic_movegen::board::Board;
use atomic_movegen::movegen;
use atomic_movegen::types::*;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: fen_after <fen> <move>");
        return;
    }
    let fen = &args[1];
    let movestr = &args[2];
    let mut board = Board::from_fen(fen).expect("Invalid FEN");

    // Find the move
    let from_sq = parse_sq(&movestr[0..2]);
    let to_sq = parse_sq(&movestr[2..4]);

    let mut moves = Vec::with_capacity(256);
    movegen::generate_legal(&board, &mut moves);

    for &m in &moves {
        if m.from_sq() == from_sq && m.to_sq() == to_sq {
            let mut state = atomic_movegen::board::StateInfo::new();
            board.do_move(m, &mut state);
            println!("{}", board.fen());

            // Now list legal moves for the other side
            let mut moves2 = Vec::with_capacity(256);
            movegen::generate_legal(&board, &mut moves2);
            moves2.sort_by_key(|m| (m.from_sq() as u16, m.to_sq() as u16));
            println!("Legal moves ({} total):", moves2.len());
            for &m2 in &moves2 {
                println!("  {}{}", sq_str(m2.from_sq()), sq_str(m2.to_sq()));
            }
            return;
        }
    }
    println!("Move not found");
}

fn parse_sq(s: &str) -> Square {
    let file = match s.chars().nth(0).unwrap() {
        'a' => 0,
        'b' => 1,
        'c' => 2,
        'd' => 3,
        'e' => 4,
        'f' => 5,
        'g' => 6,
        'h' => 7,
        _ => 0,
    };
    let rank = match s.chars().nth(1).unwrap() {
        '1' => 0,
        '2' => 1,
        '3' => 2,
        '4' => 3,
        '5' => 4,
        '6' => 5,
        '7' => 6,
        '8' => 7,
        _ => 0,
    };
    make_square(
        match file {
            0 => File::A,
            1 => File::B,
            2 => File::C,
            3 => File::D,
            4 => File::E,
            5 => File::F,
            6 => File::G,
            7 => File::H,
            _ => unreachable!(),
        },
        match rank {
            0 => Rank::R1,
            1 => Rank::R2,
            2 => Rank::R3,
            3 => Rank::R4,
            4 => Rank::R5,
            5 => Rank::R6,
            6 => Rank::R7,
            7 => Rank::R8,
            _ => unreachable!(),
        },
    )
}

fn sq_str(sq: Square) -> String {
    let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    let idx = sq as usize;
    format!("{}{}", files[idx % 8], (idx / 8 + 1))
}
