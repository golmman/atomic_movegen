//! Shared differential-test helpers: the perft oracle corpus, a deterministic
//! PRNG, and the seeded play-out walkers.
//!
//! Every integration test that declares `mod common;` compiles its own copy, so
//! items a given test does not use are allowed to be dead there.
#![allow(dead_code)]

use atomic_movegen::movegen::generate_legal_with_state;
use atomic_movegen::{Board, MoveList, StateInfo};

/// Read the FEN column of the perft table (the 41-position oracle corpus).
pub fn perft_corpus() -> Vec<String> {
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

/// Playout start positions: capture-rich, promotion-heavy, en-passant, castling
/// and bare-endgame boards.
pub const PLAYOUT_FENS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
    "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
];

/// Deterministic xorshift64 PRNG (no dev-dependency, reproducible play-outs).
pub struct Xorshift(u64);

impl Xorshift {
    pub fn new(seed: u64) -> Self {
        Xorshift(seed | 1)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Visit every position of a random legal play-out from each start FEN.
pub fn walk_positions(
    start_fens: &[&str],
    seeds: u64,
    depth: u32,
    visit: &mut impl FnMut(&Board, &StateInfo),
) -> u64 {
    let mut visited = 0;
    let mut state = StateInfo::new();
    for (i, fen) in start_fens.iter().enumerate() {
        for seed in 0..seeds {
            let mut board = Board::from_fen(fen).expect("play-out FEN must parse");
            let mut rng = Xorshift::new(0x9E37_79B9_7F4A_7C15 ^ ((i as u64) << 32) ^ seed);
            for _ in 0..=depth {
                board.populate_state(&mut state);
                visit(&board, &state);
                visited += 1;

                let mut list = MoveList::new();
                generate_legal_with_state(&board, &state, &mut list);
                if list.is_empty() {
                    break;
                }
                let idx = (rng.next() % list.len() as u64) as usize;
                board.do_move(list.as_slice()[idx], &mut state);
            }
        }
    }
    visited
}

/// Visit random legal play-outs: long ones from the play-out starts and short
/// ones from every perft-corpus position.
pub fn walk_playouts(visit: &mut impl FnMut(&Board, &StateInfo)) -> u64 {
    let corpus = perft_corpus();
    let corpus_refs: Vec<&str> = corpus.iter().map(String::as_str).collect();
    walk_positions(PLAYOUT_FENS, 16, 300, visit) + walk_positions(&corpus_refs, 2, 60, visit)
}

/// Visit the perft corpus and `fixtures` once each.
pub fn walk_corpus_and_fixtures(fixtures: &[&str], visit: &mut impl FnMut(&Board, &StateInfo)) {
    let corpus = perft_corpus();
    let fens: Vec<&str> = corpus
        .iter()
        .map(String::as_str)
        .chain(fixtures.iter().copied())
        .collect();
    for fen in fens {
        let board = Board::from_fen(fen).expect("corpus FEN must parse");
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        visit(&board, &state);
    }
}
