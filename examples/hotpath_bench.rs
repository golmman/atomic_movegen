//! Hot-path cost rows for the wishlist-2 shapes.
//!
//! Plan 1 (W6) introduced this harness with the setup writers and the counts
//! reader; later plans add candidate/child rows with stable names. Each row is
//! the median of repeated passes over a fixed board list, reported in absolute
//! ns per call and as a ratio to the current full-state row of the same run.
//! Absolute ns are only comparable on one machine; the ratio is the gate.
//!
//! Usage:
//! ```sh
//! cargo run --release --example hotpath_bench [repeats] [passes]
//! ```
//!
//! Two datasets are reported:
//!
//! - `corpus`: the 41 positions of `tests/perft_values.md` plus the frozen
//!   conformance FENs (the positions the differential suites use);
//! - `playouts`: every position visited by the seeded play-out walk from
//!   `tests/common/mod.rs` (16 seeds, 300 plies from the six play-out starts,
//!   plus a shorter walk from each corpus position).

use atomic_movegen::movegen::generate_legal_with_state;
use atomic_movegen::{Board, CONFORMANCE_CASES, MoveList, StateInfo};
use std::hint::black_box;
use std::time::Instant;

/// Deterministic xorshift64 PRNG, matching `tests/common/mod.rs`.
struct Xorshift(u64);

impl Xorshift {
    fn new(seed: u64) -> Self {
        Xorshift(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

const PLAYOUT_FENS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
    "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
];

fn perft_corpus() -> Vec<String> {
    let content = std::fs::read_to_string("tests/perft_values.md")
        .expect("tests/perft_values.md must be readable");
    let mut fens = Vec::new();
    for line in content.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let Some(start) = line.find('`') else {
            continue;
        };
        let Some(len) = line[start + 1..].find('`') else {
            continue;
        };
        fens.push(line[start + 1..start + 1 + len].to_owned());
    }
    assert_eq!(fens.len(), 41, "expected 41 perft corpus positions");
    fens
}

fn corpus_boards() -> Vec<Board> {
    let mut fens = perft_corpus();
    fens.extend(CONFORMANCE_CASES.iter().map(|case| case.fen.to_owned()));
    fens.iter()
        .map(|fen| Board::from_fen(fen).expect("corpus FEN must parse"))
        .collect()
}

fn playout_boards() -> Vec<Board> {
    let mut fens: Vec<String> = perft_corpus();
    fens.extend(PLAYOUT_FENS.iter().map(|s| (*s).to_owned()));
    let mut boards = Vec::new();
    let mut state = StateInfo::new();
    for fen in &fens {
        for seed in 0..16u64 {
            let mut board = Board::from_fen(fen).expect("play-out FEN must parse");
            let mut rng = Xorshift::new(0x9E37_79B9_7F4A_7C15 ^ seed);
            for _ in 0..=60 {
                boards.push(board.clone());
                board.populate_state(&mut state);
                let mut legal = MoveList::new();
                generate_legal_with_state(&board, &state, &mut legal);
                if legal.is_empty() {
                    break;
                }
                let idx = (rng.next() % legal.len() as u64) as usize;
                board.do_move(legal.as_slice()[idx], &mut state);
            }
        }
    }
    boards
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// Run `f` in `repeats` batches of `passes` full board-list passes and return
/// the median nano-seconds per call (one call is one board in the list).
fn bench(mut f: impl FnMut(), calls: usize, repeats: usize, passes: usize) -> f64 {
    assert!(calls > 0);
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        for _ in 0..passes {
            f();
        }
        let ns = start.elapsed().as_nanos() as f64 / (passes * calls) as f64;
        samples.push(ns);
    }
    median(&mut samples)
}

struct Rows {
    populate_state: f64,
    populate_counts: f64,
    counts: f64,
    new_state: f64,
}

fn measure(boards: &[Board], repeats: usize, passes: usize) -> Rows {
    let mut states = vec![StateInfo::new(); boards.len()];

    let populate_state = bench(
        || {
            for (board, state) in boards.iter().zip(states.iter_mut()) {
                board.populate_state(black_box(state));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let populate_counts = bench(
        || {
            for (board, state) in boards.iter().zip(states.iter_mut()) {
                board.populate_counts(black_box(state));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let counts = bench(
        || {
            for board in boards {
                black_box(board.counts());
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let new_state = bench(
        || {
            for _ in 0..boards.len() {
                black_box(StateInfo::new());
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    Rows {
        populate_state,
        populate_counts,
        counts,
        new_state,
    }
}

fn print_table(name: &str, boards: usize, rows: &Rows) {
    println!();
    println!("{name}: {boards} boards; ns/call (median), ratio to populate_state");
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "populate_state", rows.populate_state, 1.0
    );
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "populate_counts",
        rows.populate_counts,
        rows.populate_counts / rows.populate_state
    );
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "Board::counts",
        rows.counts,
        rows.counts / rows.populate_state
    );
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "StateInfo::new",
        rows.new_state,
        rows.new_state / rows.populate_state
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let repeats: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(9).max(3);
    let passes: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(8).max(1);

    println!("hotpath_bench: repeats={repeats}, passes={passes} (median per call)");

    let corpus = corpus_boards();
    let corpus_rows = measure(&corpus, repeats, passes);
    print_table("corpus", corpus.len(), &corpus_rows);

    let playouts = playout_boards();
    let playout_rows = measure(&playouts, repeats, passes);
    print_table("playouts", playouts.len(), &playout_rows);
}
