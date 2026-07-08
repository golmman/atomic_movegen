use atomic_movegen::board::Board;
use atomic_movegen::movegen;
use atomic_movegen::types::{MoveList, MoveType, PieceType, Square, sq_str};

fn uci(m: atomic_movegen::types::Move) -> String {
    let from = sq_str(m.from_sq());
    let to = if m.move_type() == MoveType::Castling {
        match (m.from_sq(), m.to_sq()) {
            (Square::E1, Square::H1) => "g1".to_string(),
            (Square::E1, Square::A1) => "c1".to_string(),
            (Square::E8, Square::H8) => "g8".to_string(),
            (Square::E8, Square::A8) => "c8".to_string(),
            _ => sq_str(m.to_sq()),
        }
    } else {
        sq_str(m.to_sq())
    };
    let mut s = from + &to;
    if m.move_type() == MoveType::Promotion {
        let ch = match m.promotion_type() {
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            _ => unreachable!(),
        };
        s.push(ch);
    }
    s
}

fn fix_halfmove_clock(fen: &str) -> String {
    let fields: Vec<&str> = fen.splitn(6, ' ').collect();
    if fields.len() >= 5 {
        let hm: u16 = fields[4].parse().unwrap_or(0);
        if hm > 255 {
            let mut new_fen = fields[..4].join(" ");
            new_fen.push_str(" 0 ");
            new_fen.push_str(fields.get(5).unwrap_or(&"1"));
            return new_fen;
        }
    }
    fen.to_string()
}

#[test]
fn verify_moves_md() {
    atomic_movegen::attacks::init();

    let data = include_str!("moves.md");
    let mut failed = 0;
    let mut total = 0;

    for line in data.lines() {
        let line = line.trim();
        if !line.starts_with('|') || line.starts_with("|---") {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        let fen = parts[2].trim().trim_matches('`');
        let moves_str = parts[3].trim();

        if !fen.contains('/') {
            continue;
        }

        total += 1;

        let mut expected: Vec<&str> = moves_str.split_whitespace().collect();
        expected.sort();

        let fen_fixed = fix_halfmove_clock(fen);
        let board = Board::from_fen(&fen_fixed).expect("Invalid FEN");
        let mut moves = MoveList::new();
        movegen::generate_legal(&board, &mut moves);

        let mut got: Vec<String> = moves.as_slice().iter().map(|&m| uci(m)).collect();
        got.sort();

        if expected.len() != got.len() || expected.iter().zip(&got).any(|(e, g)| *e != g.as_str()) {
            failed += 1;
            eprintln!("FAIL [{}] {}", total, fen);
            eprintln!("  expected ({}): {}", expected.len(), expected.join(" "));
            eprintln!("  got      ({}): {}", got.len(), got.join(" "));
        }
    }

    eprintln!("Passed {}/{} positions", total - failed, total);
    assert_eq!(failed, 0, "{} positions failed", failed);
}
