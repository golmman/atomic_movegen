//! Differential tests for the mode-aware child key:
//! `EffectContext::post_move_and_key_with(board, m, mode)`.
//!
//! The acceptance gate is an equality with the clone oracle for **every
//! pseudo-legal** move (legal or not) of the 41-position perft corpus, the
//! effect and child fixtures, the frozen conformance corpus, and seeded random
//! play-outs:
//!
//! ```text
//! post_move_and_key_with(&b, m, mode).1
//!     == b.hash_after_with(m, mode)
//!     == { let mut c = b.clone(); c.do_move(m, &mut st); c.hash_with(mode) }
//! post_move_and_key_with(&b, m, Always) == post_move_and_key(&b, m)
//! ```
//!
//! The en-passant rows pin the two modes against a legal, an impossible, a
//! pinned and a self-exploding ep capture. No stored key or corpus value is
//! changed: `EnPassantMode::Legal` is the same ep-file XOR as
//! `Board::hash_with(Legal)`, so `ZOBRIST_VERSION` stays `1`.

mod common;

use atomic_movegen::conformance::{CASES, CHILD_CASES};
use atomic_movegen::movegen::generate_pseudo_legal;
use atomic_movegen::{Board, EnPassantMode, Move, MoveList, Square, StateInfo};
use common::{walk_corpus_and_fixtures, walk_playouts};

const MODES: [EnPassantMode; 2] = [EnPassantMode::Always, EnPassantMode::Legal];

/// The effect path's own corpus and the fixtures that exercise every child
/// branch. The conformance and child cases are appended below.
const FIXTURES: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55",
    "k1n5/8/1K6/8/8/8/8/8 b - - 0 1",
    "8/8/8/8/8/8/8/3qK3 w - - 0 1",
    "8/8/8/2R5/4n3/8/7K/5k2 b - - 13 69",
    "8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41",
    "4r2k/3p4/8/4P3/8/8/8/4K3 b - - 0 1",
    "7k/3pK3/8/4P3/8/8/8/8 b - - 0 1",
];

/// Find the pseudo-legal encoding of `uci` (castling and en-passant encodings
/// differ from the syntax-only parse).
fn pseudo_move(board: &Board, uci: &str) -> Move {
    if let Some(m) = board.move_from_uci(uci) {
        return m;
    }
    let parsed = Move::from_uci(uci).expect("UCI must parse");
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);
    pseudo
        .as_slice()
        .iter()
        .copied()
        .find(|m| m.from_sq() == parsed.from_sq() && m.to_sq() == parsed.to_sq())
        .unwrap_or_else(|| panic!("{uci} is not pseudo-legal in {}", board.fen()))
}

fn fixture(fen: &str) -> (Board, StateInfo) {
    let board = Board::from_fen(fen).expect("fixture FEN must parse");
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    (board, state)
}

fn is_double_push(board: &Board, m: Move) -> bool {
    m.move_type() == atomic_movegen::types::MoveType::Normal
        && (m.to_sq() as i8 - m.from_sq() as i8).abs() == 16
        && board
            .piece_on(m.from_sq())
            .type_of()
            .is_some_and(|pt| pt == atomic_movegen::PieceType::Pawn)
}

/// The mode-aware key equals the clone oracle for every pseudo-legal move of
/// the corpora and the play-outs, and `Always` is the existing query.
#[test]
fn key_with_modes_matches_cloned_child() {
    let mut positions = 0u64;
    let mut candidates = 0u64;
    let mut double_pushes = 0u64;
    let mut stored_ep = 0u64;
    let mut legal_ep_hits = 0u64;
    let mut removals = 0u64;

    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let fen = board.fen();
        let fen_before = fen.clone();
        let hash_before = board.hash();
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);

        for &m in pseudo.as_slice() {
            candidates += 1;
            let mut child = board.clone();
            let mut child_state = StateInfo::new();
            child.do_move(m, &mut child_state);

            let mut always_post = None;
            for mode in MODES {
                let (post, key) = ctx
                    .post_move_and_key_with(board, m, mode)
                    .unwrap_or_else(|| panic!("{fen} {m}: keyed summary"));
                assert_eq!(
                    key,
                    child.hash_with(mode),
                    "{fen} {m}: key vs cloned child ({mode:?})"
                );
                assert_eq!(
                    key,
                    board.hash_after_with(m, mode),
                    "{fen} {m}: key vs hash_after_with ({mode:?})"
                );
                assert_eq!(post.rule50(), child.rule50(), "{fen} {m}: clock ({mode:?})");
                if mode == EnPassantMode::Always {
                    always_post = Some((post, key));
                }
            }

            let (post, key) = always_post.expect("Always row");
            assert_eq!(
                (post, key),
                ctx.post_move_and_key(board, m).expect("existing query"),
                "{fen} {m}: Always spelling"
            );

            if is_double_push(board, m) {
                double_pushes += 1;
            }
            if post.ep_square().is_some() {
                stored_ep += 1;
                match child.legal_ep_square() {
                    Some(_) => legal_ep_hits += 1,
                    None => removals += 1,
                }
            }
        }

        assert_eq!(board.fen(), fen_before, "the keyed query wrote the board");
        assert_eq!(board.hash(), hash_before, "the keyed query wrote the board");
        positions += 1;
    };

    let mut fixture_fens: Vec<&str> = FIXTURES.to_vec();
    fixture_fens.extend(CASES.iter().map(|case| case.fen));
    fixture_fens.extend(CHILD_CASES.iter().map(|case| case.fen));
    walk_corpus_and_fixtures(&fixture_fens, &mut check);
    let played = walk_playouts(&mut check);

    assert!(
        played > 5_000,
        "expected thousands of played-out positions, got {played}"
    );
    assert!(
        candidates > 10_000,
        "expected many pseudo-legal candidates, got {candidates}"
    );
    assert!(double_pushes > 0, "expected double pushes");
    assert!(stored_ep > 0, "expected children with a stored ep target");
    assert!(legal_ep_hits > 0, "expected a legal ep capture in a child");
    assert!(removals > 0, "expected a non-capturable stored ep target");
    eprintln!(
        "ep_key: {positions} positions, {candidates} pseudo-legal moves, \
         {double_pushes} double pushes, {stored_ep} stored ep children \
         ({legal_ep_hits} legal, {removals} removed)"
    );
}

/// One row per en-passant key branch. Every row also asserts against
/// `clone + do_move + hash_with(mode)`, never only against the other internal
/// helper.
#[test]
fn key_with_deterministic_ep_fixtures() {
    struct Case {
        name: &'static str,
        fen: &'static str,
        uci: &'static str,
        child_ep: Option<Square>,
        child_legal_ep: Option<Square>,
        modes_agree: bool,
    }
    let cases = [
        Case {
            name: "double push with a legal ep capture",
            fen: "4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1",
            uci: "e2e4",
            child_ep: Some(Square::E3),
            child_legal_ep: Some(Square::E3),
            modes_agree: true,
        },
        Case {
            name: "double push without an adjacent enemy pawn",
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            uci: "e2e4",
            child_ep: Some(Square::E3),
            child_legal_ep: None,
            modes_agree: false,
        },
        Case {
            name: "double push whose ep capture is pinned",
            fen: "4r2k/3p4/8/4P3/8/8/8/4K3 b - - 0 1",
            uci: "d7d5",
            child_ep: Some(Square::D6),
            child_legal_ep: None,
            modes_agree: false,
        },
        Case {
            name: "double push whose ep capture self-explodes",
            fen: "7k/3pK3/8/4P3/8/8/8/8 b - - 0 1",
            uci: "d7d5",
            child_ep: Some(Square::D6),
            child_legal_ep: None,
            modes_agree: false,
        },
        Case {
            name: "en-passant capture",
            fen: "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
            uci: "e4d3",
            child_ep: None,
            child_legal_ep: None,
            modes_agree: true,
        },
        Case {
            name: "castling",
            fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            uci: "e1g1",
            child_ep: None,
            child_legal_ep: None,
            modes_agree: true,
        },
        Case {
            name: "promotion",
            fen: "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            uci: "b7b8q",
            child_ep: None,
            child_legal_ep: None,
            modes_agree: true,
        },
    ];

    for case in &cases {
        let (board, state) = fixture(case.fen);
        let ctx = board.effect_context_with_state(&state);
        let m = pseudo_move(&board, case.uci);
        let mut child = board.clone();
        let mut child_state = StateInfo::new();
        child.do_move(m, &mut child_state);

        assert_eq!(child.ep_square(), case.child_ep, "{}: child ep", case.name);
        assert_eq!(
            child.legal_ep_square(),
            case.child_legal_ep,
            "{}: child legal ep",
            case.name
        );

        let (post, always) = ctx
            .post_move_and_key_with(&board, m, EnPassantMode::Always)
            .unwrap_or_else(|| panic!("{}: keyed summary", case.name));
        let (legal_post, legal) = ctx
            .post_move_and_key_with(&board, m, EnPassantMode::Legal)
            .unwrap_or_else(|| panic!("{}: keyed summary", case.name));

        assert_eq!(post.ep_square(), case.child_ep, "{}: summary ep", case.name);
        assert_eq!(post, legal_post, "{}: summary", case.name);
        assert_eq!(
            always,
            child.hash_with(EnPassantMode::Always),
            "{}",
            case.name
        );
        assert_eq!(
            legal,
            child.hash_with(EnPassantMode::Legal),
            "{}",
            case.name
        );
        assert_eq!(
            always,
            board.hash_after_with(m, EnPassantMode::Always),
            "{}",
            case.name
        );
        assert_eq!(
            legal,
            board.hash_after_with(m, EnPassantMode::Legal),
            "{}",
            case.name
        );
        assert_eq!(post.rule50(), child.rule50(), "{}: clock", case.name);
        assert_eq!(
            ctx.post_move_and_key_with(&board, m, EnPassantMode::Always),
            ctx.post_move_and_key(&board, m),
            "{}: Always spelling",
            case.name
        );

        if case.modes_agree {
            assert_eq!(always, legal, "{}: modes must agree", case.name);
        } else {
            assert_ne!(always, legal, "{}: Legal removes the ep key", case.name);
            assert_eq!(
                always ^ legal,
                child.hash() ^ child.hash_with(EnPassantMode::Legal),
                "{}: exactly the board's ep canonicalization",
                case.name
            );
        }
    }
}

/// Malformed input is `None` for both modes.
#[test]
fn key_with_rejects_malformed_moves_for_both_modes() {
    let (board, state) = fixture("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let ctx = board.effect_context_with_state(&state);
    for mode in MODES {
        for m in [
            Move::NONE,
            // Empty origin square.
            Move::make_move(Square::E4, Square::E5),
            // An enemy piece on the origin.
            Move::make_move(Square::A8, Square::A7),
        ] {
            assert_eq!(ctx.post_move_and_key_with(&board, m, mode), None, "{m}");
        }
    }
}
