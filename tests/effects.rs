//! Differential tests for [`EffectContext`], [`MoveEffect`], and the legal
//! move stream.
//!
//! The effect path is a second implementation of the legality rule surface, so
//! every test here is a differential against an independent oracle:
//!
//! - [`Board::legal`] and the [`generate_legal_with_state`] list for legality,
//! - a cloned [`Board::do_move`] child for the effect fields,
//! - a cloned `do_move` + [`Board::outcome`] child for the child outcome.
//!
//! Positions come from the 41-position perft corpus in
//! `tests/perft_values.md`, from the deterministic fixtures below, and from
//! seeded random play-outs.

use atomic_movegen::movegen::{
    for_each_legal, for_each_legal_with_state, generate_legal_with_state, generate_pseudo_legal,
};
use atomic_movegen::{Board, Move, MoveList, Outcome, StateInfo};
use std::ops::ControlFlow;

/// Deterministic fixtures covering each rule branch the effect path has: quiet
/// moves, captures, en passant, promotions with and without capture, both
/// castlings, explosions, self-explosion, blasted blockers, checks, touching
/// commoners, check escapes, stalemate, mate, extinction, and the clock.
const FIXTURES: &[&str] = &[
    // Quiet move and both castlings.
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    // Capture, capture-and-promotion, quiet promotion, en passant.
    "4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1",
    "3r4/4P3/8/8/8/8/k7/4K3 w - - 0 1",
    "8/4P3/8/8/8/8/k7/4K3 w - - 0 1",
    "8/8/8/PP6/kP1Pp3/PN2P3/8/6K1 b - d3 0 1",
    // The blast removes the enemy commoner (an immediate win).
    "4R1k1/8/8/8/8/8/8/4K3 w - - 0 1",
    // The blast removes the mover's own commoner (always illegal).
    "8/8/8/8/8/8/8/3qK3 w - - 0 1",
    // A capture whose blast removes a blocker and uncovers an attack on the
    // mover's commoner, plus the simpler "the capturer was the blocker" case.
    "4r2k/8/8/8/4N3/4n3/5B2/4K3 w - - 0 1",
    "4r3/8/8/8/3p4/8/4N3/4K3 w - - 0 1",
    // Checks: a quiet check, and a capture whose blast discovers a check.
    "4k3/8/8/8/8/8/8/R3K3 w - - 0 1",
    "4k3/8/4n3/8/8/7B/8/K3R3 w - - 0 1",
    // Touching commoners: the attacker is immune, and the commoner captures of
    // the enemy commoner are self-explosions.
    "8/8/8/8/3kK2r/8/7b/8 w - - 0 1",
    "k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1",
    "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
    // In check with a single escape; no legal moves while attacked (mate) and
    // while safe (stalemate); extinction.
    "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
    "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
    "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
    "8/8/8/8/8/8/8/k7 w - - 0 1",
    "k7/8/8/8/8/8/8/8 b - - 0 1",
    // Clock: a non-check quiet move at 99 plies draws by the 50-move rule, and a
    // mate on the same ply is a loss because mate outranks the clock.
    "4k3/8/8/8/8/8/8/4K2R w - - 99 60",
    "7k/8/8/5N2/3K4/8/8/R6R w - - 99 60",
    // Material draw: both commoners and nothing else.
    "k7/8/8/8/8/8/8/7K w - - 0 1",
    // The effects-driven handoff `deep-win` position.
    "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
];

mod common;

use common::{walk_corpus_and_fixtures, walk_playouts};

#[test]
fn effect_is_legal_matches_board_legal() {
    let mut positions = 0u64;
    let mut moves = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            moves += 1;
            assert_eq!(
                ctx.is_legal(board, m),
                board.legal(m, state),
                "{} {m}: effect legality",
                board.fen()
            );
            assert_eq!(
                ctx.effect(board, m).is_legal,
                board.legal(m, state),
                "{} {m}: effect field",
                board.fen()
            );
        }
        positions += 1;
    };

    walk_corpus_and_fixtures(FIXTURES, &mut check);
    let played = walk_playouts(&mut check);
    assert!(
        played > 5_000,
        "expected thousands of positions, got {played}"
    );
    eprintln!("visited {positions} positions, {moves} pseudo-legal moves");
}

#[test]
fn effect_is_legal_never_accepts_illegal() {
    // Independent of `Board::legal`: acceptance must equal membership in the
    // generated legal list, in both directions.
    let mut accepted = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let mut legal = MoveList::new();
        generate_legal_with_state(board, state, &mut legal);
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            let in_list = legal.as_slice().contains(&m);
            let by_effect = ctx.is_legal(board, m);
            assert_eq!(by_effect, in_list, "{} {m}: acceptance", board.fen());
            if by_effect {
                accepted += 1;
            }
        }
        // Nothing the effect accepts is missing from the list.
        for &m in legal.as_slice() {
            assert!(ctx.is_legal(board, m), "{} {m}: missing", board.fen());
        }
    };

    walk_corpus_and_fixtures(FIXTURES, &mut check);
    walk_playouts(&mut check);
    assert!(
        accepted > 10_000,
        "expected many legal moves, got {accepted}"
    );
}

#[test]
fn effect_fields_match_child() {
    let mut checked = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let us = board.side_to_move();
        let them = us.flip();
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);

        for &m in pseudo.as_slice() {
            let e = ctx.effect(board, m);
            assert_eq!(e.is_capture, board.is_capture(m), "{} {m}", board.fen());
            assert_eq!(
                e.casualties,
                board.blast_casualties(m),
                "{} {m}: casualties",
                board.fen()
            );

            // Every pseudo-legal move can be made legally as a board operation,
            // even when the rules reject it, so the child is the oracle for the
            // fields.
            let mut child = board.clone();
            let mut child_state = StateInfo::new();
            child.do_move(m, &mut child_state);

            assert_eq!(
                e.leaves_own_commoner,
                !child.commoners(us).is_empty(),
                "{} {m}: own commoner",
                board.fen()
            );
            assert_eq!(
                e.explodes_enemy_commoner,
                !board.commoners(them).is_empty() && child.commoners(them).is_empty(),
                "{} {m}: enemy commoner",
                board.fen()
            );
            assert_eq!(
                e.checks_enemy,
                !child.checkers().is_empty(),
                "{} {m}: check",
                board.fen()
            );
            if e.explodes_enemy_commoner {
                // The mover's own commoner survives an explosion it wins with,
                // except when both explode, which is illegal.
                assert_eq!(
                    e.is_legal,
                    e.leaves_own_commoner,
                    "{} {m}: explosion legality",
                    board.fen()
                );
            }
            if !e.leaves_own_commoner {
                assert!(!e.is_legal, "{} {m}: self-explosion", board.fen());
            }
            checked += 1;
        }
    };

    walk_corpus_and_fixtures(FIXTURES, &mut check);
    walk_playouts(&mut check);
    assert!(checked > 50_000, "expected many candidates, got {checked}");
    eprintln!("checked {checked} candidates against a cloned child");
}

#[test]
fn outcome_after_matches_do_move() {
    let mut positions = 0u64;
    let mut decided = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let mut legal = MoveList::new();
        generate_legal_with_state(board, state, &mut legal);
        for &m in legal.as_slice() {
            let mut child = board.clone();
            let mut child_state = StateInfo::new();
            child.do_move(m, &mut child_state);
            let expected = child.outcome();
            assert_eq!(
                ctx.outcome_after(board, m),
                expected,
                "{} {m}: effect outcome",
                board.fen()
            );
            assert_eq!(
                board.outcome_after(m, state),
                expected,
                "{} {m}: board outcome_after",
                board.fen()
            );
            if expected.is_some() {
                decided += 1;
            }
        }
        // Every pseudo-legal but illegal move is rejected.
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            if !legal.as_slice().contains(&m) {
                assert_eq!(
                    ctx.outcome_after(board, m),
                    None,
                    "{} {m}: illegal move",
                    board.fen()
                );
            }
        }
        positions += 1;
    };

    walk_corpus_and_fixtures(FIXTURES, &mut check);
    walk_playouts(&mut check);
    assert!(decided > 500, "expected decided children, got {decided}");
    eprintln!("outcome_after checked at {positions} positions, {decided} decided");
}

#[test]
fn for_each_legal_matches_generate_legal() {
    let mut total = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let mut expected = MoveList::new();
        generate_legal_with_state(board, state, &mut expected);

        let mut streamed = Vec::new();
        let flow = for_each_legal_with_state::<()>(board, state, &mut |m| {
            streamed.push(m);
            ControlFlow::Continue(())
        });
        assert!(flow.is_continue());
        assert_eq!(streamed, expected.as_slice(), "{}", board.fen());

        // The convenience wrapper agrees and is not affected by state reuse.
        let mut wrapper = Vec::new();
        let _ = for_each_legal::<()>(board, &mut |m| {
            wrapper.push(m);
            ControlFlow::Continue(())
        });
        assert_eq!(wrapper, expected.as_slice(), "{}", board.fen());

        // The stream must never emit a move the effect path rejects.
        let ctx = board.effect_context_with_state(state);
        for &m in expected.as_slice() {
            assert!(ctx.is_legal(board, m), "{} {m}", board.fen());
        }
        total += expected.len() as u64;
    };

    walk_corpus_and_fixtures(FIXTURES, &mut check);
    walk_playouts(&mut check);
    assert!(total > 50_000, "expected many legal moves, got {total}");
}

#[test]
fn for_each_legal_early_exit() {
    // Break after the first legal move visits exactly one.
    let board = Board::new();
    let mut seen = 0usize;
    let flow = for_each_legal::<()>(&board, &mut |_| {
        seen += 1;
        ControlFlow::Break(())
    });
    assert!(flow.is_break());
    assert_eq!(seen, 1);

    // Continue visits the whole legal list.
    let mut all = Vec::new();
    let flow = for_each_legal::<()>(&board, &mut |m| {
        all.push(m);
        ControlFlow::Continue(())
    });
    assert!(flow.is_continue());
    assert_eq!(all.len(), 20);

    // A position whose candidates are all illegal emits nothing at all.
    let board = Board::from_fen("k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1").unwrap();
    let mut count = 0usize;
    let flow = for_each_legal::<()>(&board, &mut |_| {
        count += 1;
        ControlFlow::Continue(())
    });
    assert!(flow.is_continue());
    assert_eq!(count, 0);
}

/// Find the pseudo-legal encoding of `uci` (the generator's castling and
/// en-passant encodings differ from the syntax-only parse).
fn pseudo_move(board: &Board, uci: &str) -> Move {
    // Legal moves — including castling and en passant — resolve to the exact
    // board encoding.
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

/// Find the legal encoding of `uci`.
fn legal_move(board: &Board, uci: &str) -> Move {
    board
        .move_from_uci(uci)
        .unwrap_or_else(|| panic!("{uci} is not legal in {}", board.fen()))
}

fn fixture(fen: &str) -> (Board, StateInfo) {
    let board = Board::from_fen(fen).expect("fixture FEN must parse");
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    (board, state)
}

#[test]
fn effect_deterministic_fixtures() {
    // (name, fen, uci, capture, casualties, checks, explodes, keeps commoner,
    // legal)
    struct Case {
        name: &'static str,
        fen: &'static str,
        uci: &'static str,
        is_capture: bool,
        checks_enemy: bool,
        explodes: bool,
        keeps_commoner: bool,
        is_legal: bool,
    }
    let cases = [
        Case {
            name: "quiet move",
            fen: FIXTURES[0],
            uci: "e2e4",
            is_capture: false,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "castle king side",
            fen: FIXTURES[1],
            uci: "e1g1",
            is_capture: false,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "castle queen side",
            fen: FIXTURES[1],
            uci: "e1c1",
            is_capture: false,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "capture",
            fen: FIXTURES[2],
            uci: "e4d5",
            is_capture: true,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "capturing promotion",
            fen: FIXTURES[3],
            uci: "e7d8q",
            is_capture: true,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "quiet promotion",
            fen: FIXTURES[4],
            uci: "e7e8q",
            is_capture: false,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "en passant",
            fen: FIXTURES[5],
            uci: "e4d3",
            is_capture: true,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "explodes enemy commoner",
            fen: FIXTURES[6],
            uci: "e8g8",
            is_capture: true,
            checks_enemy: false,
            explodes: true,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "own commoner explodes",
            fen: FIXTURES[7],
            uci: "e1d1",
            is_capture: true,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: false,
            is_legal: false,
        },
        Case {
            name: "blasted blocker uncovers attack",
            fen: FIXTURES[8],
            uci: "f2e3",
            is_capture: true,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: false,
        },
        Case {
            name: "capturer was the blocker",
            fen: FIXTURES[9],
            uci: "e2d4",
            is_capture: true,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: false,
        },
        Case {
            name: "quiet check",
            fen: FIXTURES[10],
            uci: "a1a8",
            is_capture: false,
            checks_enemy: true,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "blast discovers check",
            fen: FIXTURES[11],
            uci: "h3e6",
            is_capture: true,
            checks_enemy: true,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "touching commoners are immune",
            fen: FIXTURES[12],
            uci: "e4e5",
            is_capture: false,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "single check escape",
            fen: FIXTURES[15],
            uci: "h8g7",
            is_capture: false,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
        Case {
            name: "material draw move",
            fen: FIXTURES[22],
            uci: "h1g2",
            is_capture: false,
            checks_enemy: false,
            explodes: false,
            keeps_commoner: true,
            is_legal: true,
        },
    ];

    for case in &cases {
        let (board, state) = fixture(case.fen);
        let ctx = board.effect_context_with_state(&state);
        let m = pseudo_move(&board, case.uci);
        let e = ctx.effect(&board, m);

        assert_eq!(e.is_capture, case.is_capture, "{}: is_capture", case.name);
        // Captures always remove the capturer; quiet moves, quiet promotions,
        // and castling remove nothing. The exact bits are the oracle's:
        assert_eq!(
            e.casualties,
            board.blast_casualties(m),
            "{}: casualties",
            case.name
        );
        assert_eq!(
            !e.casualties.is_empty(),
            case.is_capture,
            "{}: casualties presence",
            case.name
        );
        assert_eq!(
            e.checks_enemy, case.checks_enemy,
            "{}: checks_enemy",
            case.name
        );
        assert_eq!(
            e.explodes_enemy_commoner, case.explodes,
            "{}: explodes_enemy_commoner",
            case.name
        );
        assert_eq!(
            e.leaves_own_commoner, case.keeps_commoner,
            "{}: leaves_own_commoner",
            case.name
        );
        assert_eq!(e.is_legal, case.is_legal, "{}: is_legal", case.name);

        // Cross-check against the oracle for the legal ones.
        if case.is_legal {
            assert_eq!(legal_move(&board, case.uci), m, "{}: encoding", case.name);
        }
        assert_eq!(e.is_legal, board.legal(m, &state), "{}", case.name);
    }
}

#[test]
fn en_passant_casualties_identify_the_victim() {
    let (board, state) = fixture(FIXTURES[5]);
    let ctx = board.effect_context_with_state(&state);
    let m = pseudo_move(&board, "e4d3");
    let e = ctx.effect(&board, m);
    assert_eq!(
        e.casualties.white,
        atomic_movegen::Bitboard::square_bb(atomic_movegen::Square::D4)
    );
    assert_eq!(
        e.casualties.black,
        atomic_movegen::Bitboard::square_bb(atomic_movegen::Square::D3)
    );
    assert_eq!(
        e.casualties.en_passant_victim,
        Some(atomic_movegen::Square::D4)
    );
    assert!(e.is_legal);
}

#[test]
fn outcome_after_deterministic_fixtures() {
    // Mate outranks the clock: the quiet mating move crosses the 100-ply clock.
    let (board, state) = fixture(FIXTURES[21]);
    assert_eq!(board.rule50(), 99);
    let m = legal_move(&board, "a1a8");
    assert_eq!(board.outcome_after(m, &state), Some(Outcome::Loss));

    // The same clock without a check is a 50-move draw.
    let (board, state) = fixture(FIXTURES[20]);
    let m = legal_move(&board, "h1h2");
    assert_eq!(board.outcome_after(m, &state), Some(Outcome::Draw));

    // The commoner-only material draw is visible from the effect.
    let (board, state) = fixture(FIXTURES[22]);
    let m = legal_move(&board, "h1g2");
    assert_eq!(board.outcome_after(m, &state), Some(Outcome::Draw));

    // Extinction of the enemy commoner is an immediate loss for the child.
    let (board, state) = fixture(FIXTURES[6]);
    let m = legal_move(&board, "e8g8");
    assert_eq!(board.outcome_after(m, &state), Some(Outcome::Loss));

    // Terminal parents: extinction (no commoner) and checkmate.
    for fen in [FIXTURES[18], FIXTURES[16], FIXTURES[17]] {
        let (board, state) = fixture(fen);
        let mut legal = MoveList::new();
        generate_legal_with_state(&board, &state, &mut legal);
        assert!(legal.is_empty(), "{fen}");
        assert!(board.outcome().is_some(), "{fen}");
    }
    let (board, _) = fixture(FIXTURES[18]);
    assert_eq!(board.outcome(), Some(Outcome::Loss));
    let (board, _) = fixture(FIXTURES[17]);
    assert_eq!(board.outcome(), Some(Outcome::Draw));
}

#[test]
fn legal_stream_order_matches_golden_startpos() {
    let board = Board::new();
    let mut uci = Vec::new();
    let _ = for_each_legal::<()>(&board, &mut |m| {
        uci.push(m.to_uci());
        ControlFlow::Continue(())
    });
    assert_eq!(
        uci,
        [
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
        ]
    );
}

/// Fixtures for the classification differential: the plan-1 child-view
/// fixtures (stalemate, extinction, material draw, clock, en-passant escape,
/// castling) plus the effect fixtures, so every rule branch of the analysis
/// pass is visited at least once.
const CLASSIFICATION_FIXTURES: &[&str] = &[
    // Stalemate children.
    "7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55",
    "8/8/k7/2p4p/7P/7K/5r2/8 b - - 1 59",
    // Extinction child.
    "k1n5/8/1K6/8/8/8/8/8 b - - 0 1",
    // Material-draw children, one from a capture.
    "8/8/8/2R5/4n3/8/7K/5k2 b - - 13 69",
    "8/8/8/8/8/7k/8/2K3Rr b - - 19 74",
    "k7/8/8/8/8/8/8/7K w - - 0 1",
    // Clock: a mate on the 100th ply and a quiet 50-move draw.
    "7k/8/8/5N2/3K4/8/8/R6R w - - 99 60",
    "4k3/8/8/8/8/8/8/R3K3 w - - 99 60",
    // A child whose only legal move is an en-passant escape.
    "8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41",
    // A child that keeps (some) castling rights.
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    // Touching commoners: immune, and the capture of the enemy commoner
    // explodes the mover's own.
    "8/8/8/8/3kK2r/8/7b/8 w - - 0 1",
    // Explosion wins for the mover.
    "4R1k1/8/8/8/8/8/8/4K3 w - - 0 1",
];

#[test]
fn classification_matches_effect_and_child() {
    let mut candidates = 0u64;
    let mut legal_candidates = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let us = board.side_to_move();
        let them = us.flip();
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);
        let mut legal = MoveList::new();
        generate_legal_with_state(board, state, &mut legal);

        for &m in pseudo.as_slice() {
            let classification = ctx.classification(board, m);
            let effect = ctx.effect(board, m);

            // The two entry points derive the same five fields.
            assert_eq!(
                classification,
                effect.classification(),
                "{} {m}: classification vs effect",
                board.fen()
            );
            assert_eq!(classification.is_capture, effect.is_capture, "{m}");
            assert_eq!(classification.casualties, effect.casualties, "{m}");
            assert_eq!(classification.checks_enemy, effect.checks_enemy, "{m}");
            assert_eq!(
                classification.explodes_enemy_commoner, effect.explodes_enemy_commoner,
                "{m}"
            );
            assert_eq!(
                classification.leaves_own_commoner, effect.leaves_own_commoner,
                "{m}"
            );

            // Every field equals the independent board/child oracle.
            assert_eq!(
                classification.is_capture,
                board.is_capture(m),
                "{} {m}: capture",
                board.fen()
            );
            assert_eq!(
                classification.casualties,
                board.blast_casualties(m),
                "{} {m}: casualties",
                board.fen()
            );

            let mut child = board.clone();
            let mut child_state = StateInfo::new();
            child.do_move(m, &mut child_state);
            assert_eq!(
                classification.leaves_own_commoner,
                !child.commoners(us).is_empty(),
                "{} {m}: own commoner",
                board.fen()
            );
            assert_eq!(
                classification.explodes_enemy_commoner,
                !board.commoners(them).is_empty() && child.commoners(them).is_empty(),
                "{} {m}: enemy commoner",
                board.fen()
            );
            assert_eq!(
                classification.checks_enemy,
                !child.checkers().is_empty(),
                "{} {m}: check",
                board.fen()
            );

            // A classification of a legal move is exactly the effect of it.
            if legal.as_slice().contains(&m) {
                legal_candidates += 1;
                assert_eq!(classification, effect.classification(), "{m}");
            }
            candidates += 1;
        }
    };

    walk_corpus_and_fixtures(CLASSIFICATION_FIXTURES, &mut check);
    walk_playouts(&mut check);
    assert!(
        candidates > 50_000,
        "expected many candidates, got {candidates}"
    );
    assert!(
        legal_candidates > 10_000,
        "expected many legal candidates, got {legal_candidates}"
    );
    eprintln!("classification checked on {candidates} candidates ({legal_candidates} legal)");
}

#[test]
fn classification_malformed_input() {
    use atomic_movegen::{BlastCasualties, MoveClassification, Square};

    let empty = MoveClassification {
        is_capture: false,
        casualties: BlastCasualties::default(),
        checks_enemy: false,
        explodes_enemy_commoner: false,
        leaves_own_commoner: false,
    };

    // A board whose a1, e4 and e5 are empty, so the malformed moves are
    // unambiguous.
    let board = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    let ctx = board.effect_context();
    assert_eq!(ctx.classification(&board, Move::NONE), empty);
    assert_eq!(
        ctx.classification(&board, Move::NONE),
        ctx.effect(&board, Move::NONE).classification()
    );

    // An empty origin square is malformed too.
    let m = Move::make_move(Square::E4, Square::E5);
    assert_eq!(ctx.classification(&board, m), empty);
    assert_eq!(
        ctx.classification(&board, m),
        ctx.effect(&board, m).classification()
    );

    // The capture flag of malformed input comes from the *parent* view (as
    // `effect` documents): `Move::NONE` has `a1` as its destination, which is
    // occupied on the start position.
    let board = Board::new();
    let ctx = board.effect_context();
    let e = ctx.classification(&board, Move::NONE);
    assert_eq!(Move::NONE.to_sq(), Square::A1);
    assert!(e.is_capture);
    assert_eq!(e, ctx.effect(&board, Move::NONE).classification());
    assert_eq!(e.casualties, BlastCasualties::default());
    assert!(!e.checks_enemy);
    assert!(!e.explodes_enemy_commoner);
    assert!(!e.leaves_own_commoner);

    // A game-over parent still classifies: the classification is not a
    // legality verdict, so the quiet rook moves report `leaves_own_commoner`
    // even though every one of them is illegal.
    let board = Board::from_fen("8/8/8/8/8/8/8/R6K w - - 0 1").unwrap();
    let ctx = board.effect_context();
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(&board, &mut pseudo);
    assert!(!pseudo.is_empty());
    for &m in pseudo.as_slice() {
        assert_eq!(
            ctx.classification(&board, m),
            ctx.effect(&board, m).classification(),
            "{m}"
        );
        assert!(
            ctx.classification(&board, m).leaves_own_commoner,
            "{m}: a commoner-only move"
        );
        assert!(!ctx.is_legal(&board, m), "{m}: game over parent");
    }
}
