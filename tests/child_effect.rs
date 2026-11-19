//! Differential tests for `ChildEffect` and `Board::child_effect` (wishlist 2
//! W8).
//!
//! The acceptance gate is field-for-field equality with the clone oracle for
//! every pseudo-legal move (legal or not) of the perft corpus, the conformance
//! and child corpora, the effect/board fixtures, and the seeded play-outs:
//!
//! ```text
//! effect.occupied == { clone; do_move }.occupied()
//! effect.key      == child.hash_with(mode) == board.hash_after_with(m, mode)
//! effect.rule50   == child.rule50()        == board.rule50_after(m)
//! effect.capture  == post_move(m).is_capture()
//! child_effect_with_occupancy(m, board.occupied(), ..) == child_effect(m, ..)
//! ```
//!
//! Both en-passant key modes are checked. `Legal` may build a child view only
//! for a double push whose child stores an ep target; `Always` never needs one.

mod common;

use atomic_movegen::conformance::{CASES, CHILD_CASES};
use atomic_movegen::movegen::generate_pseudo_legal;
use atomic_movegen::{Board, EnPassantMode, Move, MoveList, PieceType, Square, StateInfo};
use common::{walk_corpus_and_fixtures, walk_playouts};

const MODES: [EnPassantMode; 2] = [EnPassantMode::Always, EnPassantMode::Legal];

/// Effect-path fixtures that exercise quiet moves, captures, en passant,
/// promotions, castling, checks, pins, explosions and terminal boards.
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

#[derive(Default)]
struct Stats {
    positions: usize,
    candidates: usize,
    double_pushes: usize,
    stored_ep: usize,
    legal_ep_hits: usize,
    removals: usize,
    captures: usize,
}

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

fn check_position(board: &Board, state: &StateInfo, stats: &mut Stats) {
    let ctx = board.effect_context_with_state(state);
    let counts = board.counts();
    let fen_before = board.fen();
    let hash_before = board.hash();
    let ep_before = board.ep_square();
    let rule50_before = board.rule50();
    let game_ply_before = board.game_ply();

    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);
    stats.positions += 1;

    for &m in pseudo.as_slice() {
        stats.candidates += 1;
        let mut child = board.clone();
        let mut child_state = StateInfo::new();
        child.do_move(m, &mut child_state);
        if child.ep_square().is_some() {
            stats.stored_ep += 1;
            if child.legal_ep_square().is_some() {
                stats.legal_ep_hits += 1;
            } else {
                stats.removals += 1;
            }
        }
        if board.is_double_push(m) {
            stats.double_pushes += 1;
        }

        let post = ctx
            .post_move(board, m)
            .expect("pseudo-legal move must have a summary");

        for mode in MODES {
            let effect = board
                .child_effect(m, counts, mode)
                .unwrap_or_else(|| panic!("{fen_before} {m} {mode:?}: child effect"));
            assert_eq!(
                effect.occupied,
                child.occupied(),
                "{fen_before} {m} {mode:?}: occupied"
            );
            assert_eq!(
                effect.key,
                child.hash_with(mode),
                "{fen_before} {m} {mode:?}: clone key"
            );
            assert_eq!(
                effect.key,
                board.hash_after_with(m, mode),
                "{fen_before} {m} {mode:?}: hash_after_with"
            );
            assert_eq!(
                effect.rule50,
                child.rule50(),
                "{fen_before} {m} {mode:?}: child clock"
            );
            assert_eq!(
                effect.rule50,
                board.rule50_after(m),
                "{fen_before} {m} {mode:?}: rule50_after"
            );
            assert_eq!(
                effect.capture,
                post.is_capture(),
                "{fen_before} {m} {mode:?}: capture"
            );
            assert_eq!(
                effect,
                board
                    .child_effect_with_occupancy(m, board.occupied(), counts, mode)
                    .expect("occupancy variant"),
                "{fen_before} {m} {mode:?}: occupancy variant"
            );
            assert_eq!(
                effect.key,
                ctx.post_move_and_key_with(board, m, mode)
                    .expect("keyed summary")
                    .1,
                "{fen_before} {m} {mode:?}: PostMove key"
            );
        }

        let effect = board
            .child_effect(m, counts, EnPassantMode::Always)
            .expect("child effect");
        assert_eq!(
            effect.occupied,
            post.occupied(),
            "{fen_before} {m}: PostMove occupied"
        );
        assert_eq!(
            effect.rule50,
            post.rule50(),
            "{fen_before} {m}: PostMove clock"
        );
        if effect.capture {
            stats.captures += 1;
        }
    }

    assert_eq!(board.fen(), fen_before, "child query wrote the board");
    assert_eq!(board.hash(), hash_before, "child query wrote the board");
    assert_eq!(board.ep_square(), ep_before, "child query wrote the board");
    assert_eq!(board.rule50(), rule50_before, "child query wrote the board");
    assert_eq!(
        board.game_ply(),
        game_ply_before,
        "child query wrote the board"
    );
}

#[test]
fn child_effect_matches_cloned_child_on_corpora() {
    let mut stats = Stats::default();
    let mut fixture_fens: Vec<&str> = FIXTURES.to_vec();
    fixture_fens.extend(CASES.iter().map(|case| case.fen));
    fixture_fens.extend(CHILD_CASES.iter().map(|case| case.fen));

    walk_corpus_and_fixtures(&fixture_fens, &mut |board, state| {
        check_position(board, state, &mut stats);
    });

    assert!(stats.positions >= 80, "expected the full corpora");
    assert!(stats.candidates > 1_000, "expected real candidate traffic");
    assert!(stats.double_pushes > 0, "expected double pushes");
    assert!(stats.stored_ep > 0, "expected children storing ep");
    assert!(
        stats.legal_ep_hits > 0,
        "expected legal ep captures in children"
    );
    assert!(
        stats.removals > 0,
        "expected non-capturable stored ep targets"
    );
    assert!(stats.captures > 0, "expected captures");
    eprintln!(
        "child_effect corpus: {} positions, {} candidates, {} captures, \
         {} double pushes, {} stored ep ({} legal, {} removed)",
        stats.positions,
        stats.candidates,
        stats.captures,
        stats.double_pushes,
        stats.stored_ep,
        stats.legal_ep_hits,
        stats.removals
    );
}

#[test]
fn child_effect_matches_cloned_child_on_seeded_playouts() {
    let mut stats = Stats::default();
    let visited = walk_playouts(&mut |board, state| {
        check_position(board, state, &mut stats);
    });

    assert_eq!(visited as usize, stats.positions);
    assert!(stats.positions > 14_000, "expected a long walk");
    assert!(
        stats.candidates > 100_000,
        "expected real candidate traffic"
    );
    assert!(stats.double_pushes > 0, "walk must reach double pushes");
    assert!(stats.stored_ep > 0, "walk must reach stored ep targets");
    assert!(stats.legal_ep_hits > 0, "walk must reach legal ep captures");
    assert!(stats.removals > 0, "walk must reach removed ep targets");
    assert!(stats.captures > 10_000, "walk must reach captures");
    eprintln!(
        "child_effect play-outs: {} positions, {} candidates, {} captures, \
         {} double pushes, {} stored ep ({} legal, {} removed)",
        stats.positions,
        stats.candidates,
        stats.captures,
        stats.double_pushes,
        stats.stored_ep,
        stats.legal_ep_hits,
        stats.removals
    );
}

#[test]
fn child_effect_named_ep_and_key_fixtures() {
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
            name: "terminal parent, double push",
            fen: "4k3/8/8/8/3p4/8/4P3/8 w - - 0 1",
            uci: "e2e4",
            child_ep: Some(Square::E3),
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
        let board = Board::from_fen(case.fen).expect("fixture FEN must parse");
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let counts = board.counts();
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

        let always = board
            .child_effect(m, counts, EnPassantMode::Always)
            .unwrap_or_else(|| panic!("{}: Always", case.name));
        let legal = board
            .child_effect(m, counts, EnPassantMode::Legal)
            .unwrap_or_else(|| panic!("{}: Legal", case.name));
        for (mode, effect) in [
            (EnPassantMode::Always, always),
            (EnPassantMode::Legal, legal),
        ] {
            assert_eq!(
                effect.occupied,
                child.occupied(),
                "{}: occupied ({mode:?})",
                case.name
            );
            assert_eq!(
                effect.key,
                child.hash_with(mode),
                "{}: key ({mode:?})",
                case.name
            );
            assert_eq!(
                effect.rule50,
                child.rule50(),
                "{}: clock ({mode:?})",
                case.name
            );
        }

        if case.modes_agree {
            assert_eq!(always.key, legal.key, "{}: modes agree", case.name);
        } else {
            assert_ne!(always.key, legal.key, "{}: Legal removes ep key", case.name);
            assert_eq!(
                always.key ^ legal.key,
                child.hash() ^ child.hash_with(EnPassantMode::Legal),
                "{}: exactly the board's ep canonicalization",
                case.name
            );
        }
    }
}

#[test]
fn child_effect_rejects_malformed_and_describes_illegal() {
    let board = Board::new();
    let counts = board.counts();
    for mode in MODES {
        for m in [
            Move::NONE,
            // Empty origin square.
            Move::make_move(Square::E4, Square::E5),
            // An enemy piece on the origin.
            Move::make_move(Square::A8, Square::A7),
        ] {
            assert_eq!(board.child_effect(m, counts, mode), None, "{m} ({mode:?})");
        }
    }

    // A well-formed but illegal self-explosion still describes the child that
    // `do_move` builds.
    let board = Board::from_fen("8/8/8/8/8/8/8/3qK3 w - - 0 1").expect("fixture parses");
    let counts = board.counts();
    let m = Move::make_move(Square::E1, Square::D1);
    assert!(!board.is_legal(m), "fixture move must be illegal");
    let mut child = board.clone();
    let mut state = StateInfo::new();
    child.do_move(m, &mut state);
    for mode in MODES {
        let effect = board
            .child_effect(m, counts, mode)
            .expect("well-formed illegal move has a child effect");
        assert_eq!(effect.occupied, child.occupied(), "{mode:?}");
        assert_eq!(effect.key, child.hash_with(mode), "{mode:?}");
        assert_eq!(effect.rule50, child.rule50(), "{mode:?}");
        assert!(effect.capture, "{mode:?}");
    }
}

#[test]
fn child_effect_survives_an_impossible_double_push() {
    // A pawn two ranks from its own back rank is not reachable in play, but the
    // encoding is well-formed (the origin holds a piece of the side to move),
    // so `child_effect` must describe the child `do_move` builds rather than
    // feed `Square::NONE` to the attack tables of the legal-EP helper.
    let board = Board::from_fen("p3k3/8/8/8/8/8/8/p6K b - - 0 1").expect("fixture parses");
    let counts = board.counts();
    assert_eq!((counts.commoners(), counts.them_commoners()), (1, 1));
    let m = Move::make_move(Square::A1, Square::A3);

    let mut child = board.clone();
    let mut state = StateInfo::new();
    child.do_move(m, &mut state);
    assert_eq!(child.ep_square(), Some(Square::NONE));
    assert_eq!(child.legal_ep_square(), None);

    for mode in MODES {
        let effect = board
            .child_effect(m, counts, mode)
            .expect("well-formed move has a child effect");
        assert_eq!(effect.occupied, child.occupied(), "{mode:?}: occupied");
        assert_eq!(effect.key, child.hash_with(mode), "{mode:?}: key");
        assert_eq!(effect.rule50, child.rule50(), "{mode:?}: clock");
        assert!(!effect.capture, "{mode:?}: capture");
    }
}

#[test]
fn child_effect_capture_flag_matches_move_type_and_victim() {
    let board = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").expect("fixture parses");
    let counts = board.counts();
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(&board, &mut pseudo);
    for &m in pseudo.as_slice() {
        let effect = board
            .child_effect(m, counts, EnPassantMode::Always)
            .expect("pseudo-legal move");
        let expected = m.move_type() == atomic_movegen::types::MoveType::EnPassant
            || (m.move_type() != atomic_movegen::types::MoveType::Castling
                && board.piece_on(m.to_sq()).type_of().is_some());
        assert_eq!(effect.capture, expected, "{m}");
        if board.piece_on(m.from_sq()).type_of() == Some(PieceType::Commoner) {
            assert!(
                !effect.capture || m.is_en_passant(),
                "{m}: commoner capture"
            );
        }
    }
}
