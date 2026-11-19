//! W6 differential: the two commoner counts are the whole cached state
//! [`Board::legal`] and [`Board::is_legal_with_state`] read.
//!
//! Every position of the perft corpus, the conformance corpus, the effect
//! fixtures, and the seeded play-outs is checked twice: once with a full
//! `StateInfo` from `populate_state`, once with a counts-only state from
//! `populate_counts`. Every pseudo-legal candidate must get the same answer
//! from both. The boundary — a counts-only state is *not* sufficient for move
//! generation — is a unit test in `src/board.rs`.

mod common;

use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{Board, Counts, MoveList, StateInfo};

/// The effect fixtures (`src/effects.rs::tests::CORPUS`), copied here because
/// the unit-test corpus is crate-private. They exercise every branch of the
/// legality body: quiet moves, captures, en passant, promotions, castling,
/// checks, pins, explosions and terminal boards.
const EFFECT_CORPUS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
    "k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1",
    "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
    "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
    "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
    "8/8/8/8/8/8/8/k7 w - - 0 1",
    "k7/8/8/8/8/8/8/7K w - - 0 1",
    "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15",
    "rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5",
    "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
    "8/8/8/8/8/4k3/8/4K2q w - - 99 60",
    "8/8/8/8/8/4k3/8/4K2q w - - 100 60",
];

/// The board fixtures used by the state-aware outcome tests.
const BOARD_FIXTURES: &[&str] = &[
    "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
    "4k3/4r3/8/8/8/8/4R3/4K3 w - - 7 23",
    "4k3/8/8/8/8/8/8/4K2R w - - 100 1",
    "8/8/8/8/8/8/8/8 w - - 0 1",
];

#[derive(Default)]
struct Stats {
    positions: usize,
    zero_count_positions: usize,
    candidates: usize,
    legal_candidates: usize,
}

fn check_position(board: &Board, stats: &mut Stats) {
    let mut full = StateInfo::new();
    board.populate_state(&mut full);
    let mut counts_only = StateInfo::new();
    board.populate_counts(&mut counts_only);

    assert_eq!(board.counts(), Counts::from(&full), "{board:?}");
    assert_eq!(counts_only.commoners_count, full.commoners_count);
    assert_eq!(
        counts_only.them_commoners_count, full.them_commoners_count,
        "{board:?}"
    );
    assert_eq!(
        counts_only.checkers,
        atomic_movegen::Bitboard::EMPTY,
        "populate_counts must not write checkers"
    );
    assert_eq!(
        counts_only.pinned,
        atomic_movegen::Bitboard::EMPTY,
        "populate_counts must not write pinned"
    );

    stats.positions += 1;
    if full.commoners_count == 0 || full.them_commoners_count == 0 {
        stats.zero_count_positions += 1;
    }

    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);
    for &m in pseudo.as_slice() {
        stats.candidates += 1;
        let full_legal = board.legal(m, &full);
        assert_eq!(
            board.legal(m, &counts_only),
            full_legal,
            "{} {m}: legal",
            board.fen()
        );
        assert_eq!(
            board.is_legal_with_state(m, &counts_only),
            board.is_legal_with_state(m, &full),
            "{} {m}: is_legal_with_state",
            board.fen()
        );
        if full_legal {
            stats.legal_candidates += 1;
        }
    }

    // The shape a downstream caller uses: generate once with the full state,
    // then ask the cheap predicate for every member.
    let mut legal = MoveList::new();
    generate_legal_with_state(board, &full, &mut legal);
    for &m in legal.as_slice() {
        assert!(
            board.legal(m, &counts_only),
            "{} {m}: legal member",
            board.fen()
        );
        assert!(
            board.is_legal_with_state(m, &counts_only),
            "{} {m}",
            board.fen()
        );
    }

    // A fresh, unfilled state is the documented game-over guard.
    if !pseudo.is_empty() {
        let unfilled = StateInfo::new();
        for &m in pseudo.as_slice() {
            assert!(!board.legal(m, &unfilled), "{} {m}: unfilled", board.fen());
            assert!(
                !board.is_legal_with_state(m, &unfilled),
                "{} {m}: unfilled validating",
                board.fen()
            );
        }
    }

    // The move-generation entry points are deliberately not compared here:
    // a counts-only state is not a full state (see the boundary unit test).
}

#[test]
fn corpus_and_fixtures_agree_on_counts_only() {
    let mut stats = Stats::default();
    let mut fens: Vec<String> = common::perft_corpus();
    fens.extend(
        atomic_movegen::CONFORMANCE_CASES
            .iter()
            .map(|c| c.fen.to_owned()),
    );
    fens.extend(EFFECT_CORPUS.iter().map(|s| (*s).to_owned()));
    fens.extend(BOARD_FIXTURES.iter().map(|s| (*s).to_owned()));

    for fen in &fens {
        let board = Board::from_fen(fen).expect("counts corpus FEN must parse");
        check_position(&board, &mut stats);
    }

    assert_eq!(stats.positions, fens.len());
    assert!(
        stats.positions >= 80,
        "expected the full corpus, got {}",
        stats.positions
    );
    assert!(
        stats.zero_count_positions > 0,
        "corpus must include terminal boards"
    );
    assert!(stats.candidates > 1_000, "expected real candidate traffic");
    assert!(stats.legal_candidates > 0, "expected legal candidates");

    eprintln!(
        "counts differential: {} positions ({} zero-count), {} pseudo-legal candidates, {} legal",
        stats.positions, stats.zero_count_positions, stats.candidates, stats.legal_candidates
    );
}

#[test]
fn seeded_playouts_agree_on_counts_only() {
    let mut stats = Stats::default();
    let visited = common::walk_playouts(&mut |board, _state| {
        check_position(board, &mut stats);
    });

    assert_eq!(visited as usize, stats.positions);
    assert!(
        stats.positions > 14_000,
        "expected a long walk, got {}",
        stats.positions
    );
    assert!(
        stats.zero_count_positions > 0,
        "walk must reach terminal boards"
    );
    assert!(
        stats.candidates > 100_000,
        "expected real candidate traffic"
    );
    assert!(stats.legal_candidates > 50_000, "expected legal candidates");

    eprintln!(
        "counts play-outs: {} positions ({} zero-count), {} pseudo-legal candidates, {} legal",
        stats.positions, stats.zero_count_positions, stats.candidates, stats.legal_candidates
    );
}
