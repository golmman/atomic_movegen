//! Differential tests for `LegalityContext`, the node-local candidate legality
//! loop (wishlist 2 W7).
//!
//! For every pseudo-legal move of the perft corpus, the conformance and child
//! corpora, the effect/board fixtures, and the seeded play-outs:
//!
//! ```text
//! ctx.occupied() == board.occupied()
//! ctx.legal(m)   == board.legal(m, &full)
//!                == board.legal(m, &counts_only)
//!                == board.is_legal_with_state(m, &counts_only)
//! ```
//!
//! The corpus deliberately includes checks, pins, en passant, castling, and
//! terminal boards; the test counts those branches so a report can show that
//! the expensive rules were exercised.

mod common;

use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{Board, Counts, LegalityContext, MoveList, StateInfo};

/// The effect fixtures (`src/effects.rs::tests::CORPUS`), copied here because
/// that corpus is crate-private. They exercise quiet moves, captures, en
/// passant, promotions, castling, checks, pins, explosions and terminal boards.
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

/// Branch fixtures: terminal boards, touching commoners, an ep pin, a slider
/// double attack, and a bare commoner endgame.
const EDGE_FIXTURES: &[&str] = &[
    "8/8/8/8/8/8/8/R6K w - - 0 1",
    "r6k/8/8/8/8/8/8/8 b - - 0 1",
    "8/8/8/8/8/8/8/k7 w - - 0 1",
    "k7/8/8/8/8/8/8/7K w - - 0 1",
    "k7/8/8/8/8/8/1K6/8 w - - 0 1",
    "8/8/8/8/8/8/3kK3/8 w - - 0 1",
    "8/8/8/8/1k6/8/2P5/4K3 w - - 0 1",
    "4k3/8/8/8/8/8/4q3/4K3 w - - 0 1",
];

#[derive(Default)]
struct Stats {
    positions: usize,
    terminal_positions: usize,
    in_check_positions: usize,
    pinned_positions: usize,
    ep_positions: usize,
    castling_right_positions: usize,
    castling_candidates: usize,
    ep_candidates: usize,
    candidates: usize,
    legal_candidates: usize,
}

fn check_position(board: &Board, stats: &mut Stats) {
    let mut full = StateInfo::new();
    board.populate_state(&mut full);
    let counts = board.counts();
    assert_eq!(counts, Counts::from(&full), "{board:?}");
    let ctx: LegalityContext = board.legality_context(counts);
    assert_eq!(ctx.occupied(), board.occupied(), "{board:?}");

    let mut counts_only = StateInfo::new();
    board.populate_counts(&mut counts_only);

    let fen_before = board.fen();
    let hash_before = board.hash();

    stats.positions += 1;
    if counts.commoners() == 0 || counts.them_commoners() == 0 {
        stats.terminal_positions += 1;
    }
    if !full.checkers.is_empty() {
        stats.in_check_positions += 1;
    }
    if !full.pinned.is_empty() {
        stats.pinned_positions += 1;
    }
    if board.ep_square().is_some() {
        stats.ep_positions += 1;
    }
    if board.castling_rights() != 0 {
        stats.castling_right_positions += 1;
    }

    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);
    for &m in pseudo.as_slice() {
        stats.candidates += 1;
        if m.is_castling() {
            stats.castling_candidates += 1;
        }
        if m.is_en_passant() {
            stats.ep_candidates += 1;
        }
        let full_legal = board.legal(m, &full);
        assert_eq!(ctx.legal(m), full_legal, "{} {m}: full", board.fen());
        assert_eq!(
            ctx.legal(m),
            board.legal(m, &counts_only),
            "{} {m}: counts only",
            board.fen()
        );
        assert_eq!(
            ctx.legal(m),
            board.is_legal_with_state(m, &counts_only),
            "{} {m}: is_legal_with_state",
            board.fen()
        );
        if full_legal {
            stats.legal_candidates += 1;
        }
    }

    let mut legal = MoveList::new();
    generate_legal_with_state(board, &full, &mut legal);
    for &m in legal.as_slice() {
        assert!(ctx.legal(m), "{} {m}: legal member", board.fen());
    }

    // The context is a snapshot, not a board write.
    assert_eq!(board.fen(), fen_before);
    assert_eq!(board.hash(), hash_before);
}

#[test]
fn corpus_and_fixtures_match_board_legal() {
    let mut stats = Stats::default();
    let mut fens: Vec<String> = common::perft_corpus();
    fens.extend(
        atomic_movegen::CONFORMANCE_CASES
            .iter()
            .map(|c| c.fen.to_owned()),
    );
    fens.extend(
        atomic_movegen::conformance::CHILD_CASES
            .iter()
            .map(|c| c.fen.to_owned()),
    );
    fens.extend(EFFECT_CORPUS.iter().map(|s| (*s).to_owned()));
    fens.extend(BOARD_FIXTURES.iter().map(|s| (*s).to_owned()));
    fens.extend(EDGE_FIXTURES.iter().map(|s| (*s).to_owned()));
    fens.sort();
    fens.dedup();

    for fen in &fens {
        let board = Board::from_fen(fen).expect("legality corpus FEN must parse");
        check_position(&board, &mut stats);
    }

    assert_eq!(stats.positions, fens.len());
    assert!(stats.positions >= 80, "expected the full corpora");
    assert!(
        stats.terminal_positions > 0,
        "corpus must include terminals"
    );
    assert!(stats.in_check_positions > 0, "corpus must include checks");
    assert!(stats.pinned_positions > 0, "corpus must include pins");
    assert!(stats.ep_positions > 0, "corpus must include ep targets");
    assert!(stats.ep_candidates > 0, "corpus must include ep candidates");
    assert!(
        stats.castling_right_positions > 0,
        "corpus must include castling rights"
    );
    assert!(
        stats.ep_positions > 0,
        "corpus must include en-passant positions"
    );
    assert!(stats.candidates > 1_000, "expected real candidate traffic");
    assert!(stats.legal_candidates > 0);

    eprintln!(
        "legality context corpus: {} positions ({} terminal, {} in check, {} pinned, {} ep, {} with rights), {} candidates ({} castling, {} ep), {} legal",
        stats.positions,
        stats.terminal_positions,
        stats.in_check_positions,
        stats.pinned_positions,
        stats.ep_positions,
        stats.castling_right_positions,
        stats.candidates,
        stats.castling_candidates,
        stats.ep_candidates,
        stats.legal_candidates
    );
}

#[test]
fn seeded_playouts_match_board_legal() {
    let mut stats = Stats::default();
    let visited = common::walk_playouts(&mut |board, _state| {
        check_position(board, &mut stats);
    });

    assert_eq!(visited as usize, stats.positions);
    assert!(stats.positions > 14_000, "expected a long walk");
    assert!(stats.terminal_positions > 0, "walk must reach terminals");
    assert!(stats.in_check_positions > 0, "walk must reach checks");
    assert!(stats.pinned_positions > 0, "walk must reach pins");
    assert!(stats.ep_positions > 0, "walk must reach ep targets");
    assert!(
        stats.castling_right_positions > 0,
        "walk must reach castling rights"
    );
    assert!(
        stats.castling_candidates > 0,
        "walk must reach castling moves"
    );
    assert!(
        stats.candidates > 100_000,
        "expected real candidate traffic"
    );
    assert!(stats.legal_candidates > 50_000);

    eprintln!(
        "legality context play-outs: {} positions ({} terminal, {} in check, {} pinned, {} ep, {} with rights), {} candidates ({} castling, {} ep), {} legal",
        stats.positions,
        stats.terminal_positions,
        stats.in_check_positions,
        stats.pinned_positions,
        stats.ep_positions,
        stats.castling_right_positions,
        stats.candidates,
        stats.castling_candidates,
        stats.ep_candidates,
        stats.legal_candidates
    );
}

#[test]
fn terminal_context_rejects_every_candidate() {
    for fen in EDGE_FIXTURES {
        let board = Board::from_fen(fen).expect("edge fixture must parse");
        let counts = board.counts();
        let ctx = board.legality_context(counts);
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        if counts.commoners() == 0 || counts.them_commoners() == 0 {
            for &m in pseudo.as_slice() {
                assert!(!ctx.legal(m), "{fen} {m}: terminal context");
            }
        }
        // Rebuilding the context from a stale zero pair is the documented
        // game-over guard, exactly like `Board::legal` with such a state.
        let stale = board.legality_context(Counts::new(0, counts.them_commoners()));
        for &m in pseudo.as_slice() {
            assert!(!stale.legal(m), "{fen} {m}: zero count");
        }
    }
}
