//! Differential tests for `EffectContext::exploding_capture_after`.
//!
//! The query is checked against an exhaustive oracle over the built child:
//!
//! ```text
//! child = clone + do_move(m)
//! target = the commoner of the side that played m, in the child
//! for c in generate_legal_with_state(child) in generation order:
//!     if child.is_capture(c) && child.blast_casualties(c).all() & target != EMPTY:
//!         return Some(c)
//! None
//! ```
//!
//! The oracle deliberately uses the built child and its legal list, the shape
//! the wishlist names. Positions come from the 41-position perft corpus, the
//! effect/child/conformance fixtures, and seeded random play-outs from
//! `tests/common/mod.rs`.
//!
//! The query and the oracle both walk the child's pseudo-legal generation
//! order, so they must return exactly the same first hit, not merely the same
//! existence answer.

mod common;

use atomic_movegen::conformance::CHILD_CASES;
use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::types::MoveType;
use atomic_movegen::{Board, CONFORMANCE_CASES, Move, MoveList, Square, StateInfo};
use common::{walk_corpus_and_fixtures, walk_playouts};

/// The exhaustive oracle over the built child.
fn oracle(board: &Board, m: Move) -> Option<Move> {
    let mover = board.side_to_move();
    let mut child = board.clone();
    let mut state = StateInfo::new();
    child.do_move(m, &mut state);

    // The target is read from the child: `m` may have moved the commoner.
    let target = child.commoners(mover);
    if target.is_empty() {
        return None;
    }
    // A child side without a commoner is already decided and has no legal move.
    if child.commoners(child.side_to_move()).is_empty() {
        return None;
    }

    child.populate_state(&mut state);
    let mut legal = MoveList::new();
    generate_legal_with_state(&child, &state, &mut legal);
    legal
        .as_slice()
        .iter()
        .copied()
        .find(|&c| child.is_capture(c) && !(child.blast_casualties(c).all() & target).is_empty())
}

/// `true` when `candidate` is in the built child's legal list.
fn is_legal_in_child(board: &Board, m: Move, candidate: Move) -> bool {
    let mut child = board.clone();
    let mut state = StateInfo::new();
    child.do_move(m, &mut state);
    child.populate_state(&mut state);
    let mut legal = MoveList::new();
    generate_legal_with_state(&child, &state, &mut legal);
    legal.as_slice().contains(&candidate)
}

#[derive(Default)]
struct Counts {
    positions: u64,
    moves: u64,
    hits: u64,
}

/// Check every pseudo-legal move of one position.
fn check_position(board: &Board, state: &StateInfo, counts: &mut Counts) {
    let ctx = board.effect_context_with_state(state);
    let fen = board.fen();
    let hash = board.hash();
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);

    for &m in pseudo.as_slice() {
        counts.moves += 1;
        let expected = oracle(board, m);
        let actual = ctx.exploding_capture_after(board, m);
        assert_eq!(
            actual, expected,
            "{fen} {m}: query and oracle must return the same first hit"
        );
        if let Some(capture) = actual {
            counts.hits += 1;
            assert!(
                is_legal_in_child(board, m, capture),
                "{fen} {m}: {capture} must be legal in the child"
            );
        }
        // The parent board is untouched by the query.
        assert_eq!(board.fen(), fen, "{fen} {m}: FEN must not change");
        assert_eq!(board.hash(), hash, "{fen} {m}: hash must not change");
    }
    counts.positions += 1;
}

/// Effect, child, conformance and named fixtures.
const FIXTURES: &[&str] = &[
    // Effect-path fixtures.
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
    "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
    // Child-state fixtures.
    "7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55",
    "8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41",
    "8/8/8/8/8/4k3/8/4K2q w - - 99 60",
    "k1n5/8/1K6/8/8/8/8/8 b - - 0 1",
    // The child-view fixtures of the crate-private unit tests.
    "4k3/4r3/8/8/8/8/8/4K3 b - - 0 1",
    "5r2/8/8/8/3kK3/5R2/8/8 b - - 0 1",
    "8/8/8/8/3kK3/3q4/8/8 b - - 0 1",
    "5r2/8/8/8/3k4/5P1K/8/8 b - - 0 1",
    // Named parent/move fixtures: capture beside the commoner, the en-passant
    // explosion, a self-exploding mover, and an already-extinct child.
    "5r2/8/8/8/3kK3/8/5R2/8 w - - 0 1",
    "k7/8/8/8/3p1K2/8/4P3/8 w - - 0 1",
    "k7/8/8/8/8/8/8/3qK3 w - - 0 1",
    "4k3/8/8/8/8/8/4R3/4K3 w - - 0 1",
];

#[test]
fn exploding_capture_matches_built_child_oracle() {
    let mut counts = Counts::default();
    let mut fixtures: Vec<&str> = FIXTURES.to_vec();
    fixtures.extend(CONFORMANCE_CASES.iter().map(|case| case.fen));
    fixtures.extend(CHILD_CASES.iter().map(|case| case.fen));
    fixtures.sort_unstable();
    fixtures.dedup();

    walk_corpus_and_fixtures(&fixtures, &mut |board, state| {
        check_position(board, state, &mut counts)
    });
    walk_playouts(&mut |board, state| check_position(board, state, &mut counts));

    eprintln!(
        "explosion differential: {} positions, {} named/conformance/child fixtures, \
         {} pseudo-legal moves, {} hits",
        counts.positions,
        fixtures.len(),
        counts.moves,
        counts.hits
    );
    assert!(counts.positions > 10_000, "the play-out corpus must run");
    assert!(counts.moves > counts.positions);
    assert!(counts.hits > 0, "the corpus must contain explosions");
}

#[test]
fn malformed_moves_return_none() {
    let board = Board::new();
    let ctx = board.effect_context();
    assert_eq!(ctx.exploding_capture_after(&board, Move::NONE), None);
    // An empty origin square.
    assert_eq!(
        ctx.exploding_capture_after(&board, Move::make_move(Square::E4, Square::E5)),
        None
    );
    // An enemy piece on the origin.
    assert_eq!(
        ctx.exploding_capture_after(&board, Move::make_move(Square::A7, Square::A6)),
        None
    );
}

/// The named public-API rows. The child-view rows are unit tests in
/// `src/effects.rs` because they start from an already-derived child; these
/// rows start from a parent and a move.
struct NamedCase {
    name: &'static str,
    fen: &'static str,
    uci: &'static str,
    expected: Option<(&'static str, MoveType)>,
}

#[test]
fn named_fixtures() {
    let cases = &[
        NamedCase {
            name: "capture beside the commoner",
            fen: "5r2/8/8/8/3kK3/8/5R2/8 w - - 0 1",
            uci: "f2f3",
            expected: Some(("f8f3", MoveType::Normal)),
        },
        NamedCase {
            name: "en-passant explosion",
            fen: "k7/8/8/8/3p1K2/8/4P3/8 w - - 0 1",
            uci: "e2e4",
            expected: Some(("d4e3", MoveType::EnPassant)),
        },
        NamedCase {
            name: "mover self-explodes",
            fen: "k7/8/8/8/8/8/8/3qK3 w - - 0 1",
            uci: "e1d1",
            expected: None,
        },
        NamedCase {
            name: "child side already extinct",
            fen: "4k3/8/8/8/8/8/4R3/4K3 w - - 0 1",
            uci: "e2e8",
            expected: None,
        },
        NamedCase {
            name: "no capture near the commoner",
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            uci: "e2e4",
            expected: None,
        },
    ];

    for case in cases {
        let board = Board::from_fen(case.fen).expect("fixture FEN must parse");
        let ctx = board.effect_context();
        let m = Move::from_uci(case.uci).expect("UCI must parse");
        let fen_before = board.fen();
        let hash_before = board.hash();

        let actual = ctx.exploding_capture_after(&board, m);
        let expected_move = case.expected.map(|(uci, move_type)| {
            let parsed = Move::from_uci(uci).expect("expected UCI must parse");
            (parsed.from_sq(), parsed.to_sq(), move_type)
        });
        assert_eq!(
            actual.map(|c| (c.from_sq(), c.to_sq(), c.move_type())),
            expected_move,
            "{}",
            case.name
        );
        assert_eq!(actual, oracle(&board, m), "{}: oracle", case.name);
        assert_eq!(
            board.fen(),
            fen_before,
            "{}: FEN must not change",
            case.name
        );
        assert_eq!(
            board.hash(),
            hash_before,
            "{}: hash must not change",
            case.name
        );
    }
}

/// The parent board is not written by any query of the corpus/fixtures.
#[test]
fn queries_do_not_write_the_board() {
    let fixtures = [
        "k7/8/8/8/3p1K2/8/4P3/8 w - - 0 1",
        "5r2/8/8/8/3kK3/8/5R2/8 w - - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
    ];
    for fen in fixtures {
        let board = Board::from_fen(fen).expect("fixture FEN must parse");
        let ctx = board.effect_context();
        let before = board.fen();
        let hash = board.hash();
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        for &m in pseudo.as_slice() {
            let _ = ctx.exploding_capture_after(&board, m);
            assert_eq!(board.fen(), before, "{fen} {m}: FEN must not change");
            assert_eq!(board.hash(), hash, "{fen} {m}: hash must not change");
        }
    }
}
