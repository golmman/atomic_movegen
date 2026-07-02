use std::env;

fn main() {
    atomic_movegen::attacks::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: perft <fen> <depth>");
        std::process::exit(1);
    }

    let fen = &args[1];
    let depth: u32 = args[2]
        .parse()
        .expect("Depth must be a non-negative integer");

    match atomic_movegen::board::Board::from_fen(fen) {
        Ok(mut board) => {
            let nodes = atomic_movegen::perft(&mut board, depth);
            println!("{}", nodes);
        }
        Err(e) => {
            eprintln!("Error parsing FEN: {}", e);
            std::process::exit(1);
        }
    }
}
