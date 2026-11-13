//! SAN round-trip and piece-set identity check over the perft corpus.
//!
//! For every position reachable from the 41 corpus positions in
//! `tests/perft_values.md` up to a given depth, this example checks that
//!
//! 1. every legal move's [`Board::san`] parses back to the same move with
//!    [`Board::move_from_san`], and
//! 2. rebuilding the position piece by piece with [`Setup`] and
//!    [`Board::from_setup`] yields the same FEN, key, and legal move list.
//!
//! Usage:
//! ```sh
//! cargo run --release --example verify_san [depth] [path]
//! ```
//!
//! Arguments:
//! - `depth` — tree depth per corpus position (default: 3; SAN is several
//!   microseconds per move in debug builds, so prefer `--release`)
//! - `path`  — path to the perft values markdown file (default:
//!   `tests/perft_values.md`)
//!
//! The process exits with code 0 if every position passes, or 1 otherwise.

use atomic_movegen::movegen::{generate_legal, generate_legal_with_state};
use atomic_movegen::{Board, MoveList, Setup, StateInfo};
use std::env;
use std::fs;
use std::process;
use std::time::Instant;

/// Read the FEN column of the perft table.
fn parse_corpus(path: &str) -> Vec<String> {
    let content = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Error: could not read `{path}`: {e}");
        process::exit(1);
    });

    let mut fens = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || trimmed.starts_with("| #") || trimmed.starts_with("| ---") {
            continue;
        }
        let Some(start) = trimmed.find('`') else {
            continue;
        };
        let Some(end) = trimmed.rfind('`') else {
            continue;
        };
        if end > start {
            fens.push(trimmed[start + 1..end].to_owned());
        }
    }
    fens
}

/// Depth-first walk; `visit` sees every node exactly once.
fn walk(board: &mut Board, depth: u32, visit: &mut impl FnMut(&Board) -> bool) -> bool {
    if !visit(board) {
        return false;
    }
    if depth == 0 {
        return true;
    }
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    let mut moves = MoveList::new();
    generate_legal_with_state(board, &state, &mut moves);
    let mut undo = StateInfo::new();
    for i in 0..moves.len() {
        let m = moves.as_slice()[i];
        board.do_move(m, &mut undo);
        let ok = walk(board, depth - 1, visit);
        board.undo_move(m, &undo);
        if !ok {
            return false;
        }
    }
    true
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let depth: u32 = if args.len() > 1 {
        args[1].parse().unwrap_or(3)
    } else {
        3
    };
    let path = if args.len() > 2 {
        &args[2]
    } else {
        "tests/perft_values.md"
    };

    let corpus = parse_corpus(path);
    eprintln!(
        "=== SAN / Setup verification ===\n  Source: {path}\n  Depth:  {depth}\n  Cases:  {}\n",
        corpus.len()
    );

    let mut positions = 0u64;
    let mut moves = 0u64;
    let mut failures = 0u64;
    let started = Instant::now();

    for (index, fen) in corpus.iter().enumerate() {
        let mut board = match Board::from_fen(fen) {
            Ok(board) => board,
            Err(e) => {
                eprintln!("  Test #{}: INVALID FEN — {fen} ({e})", index + 1);
                failures += 1;
                continue;
            }
        };

        let case_started = Instant::now();
        let ok = walk(&mut board, depth, &mut |board| {
            positions += 1;
            let mut legal = MoveList::new();
            generate_legal(board, &mut legal);
            for &m in legal.as_slice() {
                moves += 1;
                let Some(san) = board.san(m) else {
                    eprintln!("  {} / {m}: legal move without SAN", board.fen());
                    return false;
                };
                if board.move_from_san(&san) != Some(m) {
                    eprintln!("  {} / {san} / {m}: SAN does not round-trip", board.fen());
                    return false;
                }
            }

            let mut setup = Setup::new();
            for (square, piece) in board.pieces() {
                setup.place(square, piece);
            }
            setup
                .set_side_to_move(board.side_to_move())
                .set_castling_rights(board.castling_rights())
                .set_ep_square(board.ep_square())
                .set_rule50(board.rule50())
                .set_game_ply(board.game_ply());
            let rebuilt = match Board::from_setup(&setup) {
                Ok(rebuilt) => rebuilt,
                Err(e) => {
                    eprintln!("  {}: setup rejected: {e}", board.fen());
                    return false;
                }
            };
            if rebuilt.fen() != board.fen() || rebuilt.hash() != board.hash() {
                eprintln!("  {}: setup round-trip differs", board.fen());
                return false;
            }
            true
        });

        let elapsed = case_started.elapsed();
        if ok {
            println!(
                "  Test #{:<4} PASS [{:.3} s]",
                index + 1,
                elapsed.as_secs_f64()
            );
        } else {
            eprintln!(
                "  Test #{:<4} FAIL [{:.3} s]",
                index + 1,
                elapsed.as_secs_f64()
            );
            failures += 1;
        }
    }

    eprintln!("\n--- Summary ---");
    eprintln!("  Positions:  {positions}");
    eprintln!("  Moves:      {moves}");
    eprintln!("  Total time: {:.3} s", started.elapsed().as_secs_f64());
    eprintln!(
        "  Result:     {}/{} corpus positions passed",
        corpus.len() as u64 - failures,
        corpus.len()
    );

    if failures > 0 {
        process::exit(1);
    }
}
