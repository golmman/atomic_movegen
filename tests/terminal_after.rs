//! Differential tests for `EffectContext::terminal_after` and the child queries
//! built on it (`is_terminal_after`, `has_legal_move_after`,
//! `Board::terminal_after`).
//!
//! The contract under test is an equality with a cloned child:
//!
//! ```text
//! ctx.terminal_after(&b, m) == { let mut c = b.clone(); c.do_move(m, &mut st); c.terminal() }
//! ```
//!
//! for **every pseudo-legal** move, legal or not — the query answers "is the
//! child decided?", not "is the move legal?". Positions come from the
//! 41-position perft corpus in `tests/perft_values.md`, from the deterministic
//! fixtures below, and from seeded random play-outs.

mod common;

use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{Board, Move, MoveList, Outcome, StateInfo, Terminal, TerminalReason};
use common::{walk_corpus_and_fixtures, walk_playouts};

/// Deterministic fixtures: one row per rule branch of the child view. Every row
/// was verified against a cloned `do_move` child while writing the plan; the
/// test asserts both the documented value and that equality.
const FIXTURES: &[&str] = &[
    // Stalemate children.
    "7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55",
    "8/8/k7/2p4p/7P/7K/5r2/8 b - - 1 59",
    // Extinction child, and an illegal self-explosion child.
    "k1n5/8/1K6/8/8/8/8/8 b - - 0 1",
    "8/8/8/8/8/8/8/3qK3 w - - 0 1",
    // Material-draw children, one from a capture.
    "8/8/8/2R5/4n3/8/7K/5k2 b - - 13 69",
    "8/8/8/8/8/7k/8/2K3Rr b - - 19 74",
    "k7/8/8/8/8/8/8/7K w - - 0 1",
    // Clock: mate outranks it, a check with an escape and a quiet child do not.
    "7k/8/8/5N2/3K4/8/8/R6R w - - 99 60",
    "4k3/8/8/8/8/8/8/R3K3 w - - 99 60",
    "8/8/8/8/8/4k3/8/4K2q w - - 99 60",
    // A child whose only legal move is an en passant escape.
    "8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41",
    // A child that keeps (some) castling rights.
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
];

/// Find the pseudo-legal encoding of `uci` (the generator's castling and
/// en-passant encodings differ from the syntax-only parse).
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

/// Clone, play, and return the child with a populated `StateInfo`.
fn child(board: &Board, m: Move) -> (Board, StateInfo) {
    let mut child = board.clone();
    let mut state = StateInfo::new();
    child.do_move(m, &mut state);
    child.populate_state(&mut state);
    (child, state)
}

#[test]
fn terminal_after_matches_cloned_child() {
    let mut positions = 0u64;
    let mut candidates = 0u64;
    let mut decided = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            candidates += 1;

            // The oracle: a cloned child, both with a fresh state and with a
            // populated one.
            let (child_board, child_state) = child(board, m);
            let expected = child_board.terminal();
            assert_eq!(
                expected,
                child_board.terminal_with_state(&child_state),
                "{} {m}: terminal/terminal_with_state disagree",
                board.fen()
            );

            let actual = ctx.terminal_after(board, m);
            assert_eq!(
                actual,
                expected,
                "{} {m}: terminal_after (legal: {:?})",
                board.fen(),
                ctx.is_legal(board, m)
            );

            // The board convenience wrapper must agree.
            assert_eq!(
                board.terminal_after(m, state),
                actual,
                "{} {m}: Board::terminal_after",
                board.fen()
            );

            // The same answer on the already derived summary (MG19a).
            assert_eq!(
                ctx.post_move(board, m).and_then(|post| post.terminal()),
                actual,
                "{} {m}: PostMove::terminal",
                board.fen()
            );

            // `is_terminal_after` is the literal `is_terminal` mirror.
            assert_eq!(
                ctx.is_terminal_after(board, m),
                Some(child_board.is_terminal()),
                "{} {m}: is_terminal_after",
                board.fen()
            );

            // `has_legal_move_after` is the child's existence query.
            assert_eq!(
                ctx.has_legal_move_after(board, m),
                Some(atomic_movegen::movegen::has_legal_move_with_state(
                    &child_board,
                    &child_state
                )),
                "{} {m}: has_legal_move_after",
                board.fen()
            );

            // The outcome query is unchanged: `Some` exactly for a legal move
            // with a decided child.
            let expected_outcome = if ctx.is_legal(board, m) {
                expected.map(|t| t.outcome)
            } else {
                None
            };
            assert_eq!(
                ctx.outcome_after(board, m),
                expected_outcome,
                "{} {m}: outcome_after",
                board.fen()
            );

            if actual.is_some() {
                decided += 1;
            }
        }
        positions += 1;
    };

    walk_corpus_and_fixtures(FIXTURES, &mut check);
    let played = walk_playouts(&mut check);

    assert!(
        played > 5_000,
        "expected thousands of played-out positions, got {played}"
    );
    assert!(
        candidates > 10_000,
        "expected many pseudo-legal candidates, got {candidates}"
    );
    assert!(decided > 1_000, "expected decided children, got {decided}");
    eprintln!(
        "terminal_after: {positions} positions, {candidates} pseudo-legal moves, \
         {decided} decided children"
    );
}

/// The documented divergence between the WDL query and the literal flag: a
/// K-vs-K child is a material draw but not literally over.
#[test]
fn is_terminal_after_excludes_the_material_shortcut() {
    let (board, state) = fixture("k7/8/8/8/8/8/8/7K w - - 0 1");
    let ctx = board.effect_context_with_state(&state);
    let m = pseudo_move(&board, "h1g1");
    assert_eq!(
        ctx.terminal_after(&board, m),
        Some(Terminal {
            outcome: Outcome::Draw,
            reason: TerminalReason::MaterialDraw,
        })
    );
    assert_eq!(ctx.is_terminal_after(&board, m), Some(false));
    assert_eq!(ctx.has_legal_move_after(&board, m), Some(true));
}

/// One row per branch: the documented child terminality, the child view fields
/// that make it right, and the clone cross-check.
#[test]
fn terminal_after_deterministic_fixtures() {
    struct Case {
        name: &'static str,
        fen: &'static str,
        uci: &'static str,
        expected: Option<Terminal>,
    }
    let terminal = |outcome, reason| Some(Terminal { outcome, reason });
    let cases = [
        Case {
            name: "stalemate child",
            fen: FIXTURES[0],
            uci: "c2c7",
            expected: terminal(Outcome::Draw, TerminalReason::Stalemate),
        },
        Case {
            name: "stalemate child (2)",
            fen: FIXTURES[1],
            uci: "f2g2",
            expected: terminal(Outcome::Draw, TerminalReason::Stalemate),
        },
        Case {
            name: "extinction child",
            fen: FIXTURES[2],
            uci: "c8b6",
            expected: terminal(Outcome::Loss, TerminalReason::Extinction),
        },
        Case {
            name: "illegal self-explosion child",
            fen: FIXTURES[3],
            uci: "e1d1",
            expected: terminal(Outcome::Loss, TerminalReason::Extinction),
        },
        Case {
            name: "material draw from a capture",
            fen: FIXTURES[4],
            uci: "e4c5",
            expected: terminal(Outcome::Draw, TerminalReason::MaterialDraw),
        },
        Case {
            name: "material draw (2)",
            fen: FIXTURES[5],
            uci: "h1g1",
            expected: terminal(Outcome::Draw, TerminalReason::MaterialDraw),
        },
        Case {
            name: "material draw position, quiet child",
            fen: FIXTURES[6],
            uci: "h1g1",
            expected: terminal(Outcome::Draw, TerminalReason::MaterialDraw),
        },
        Case {
            name: "mate outranks the clock",
            fen: FIXTURES[7],
            uci: "a1g1",
            expected: terminal(Outcome::Loss, TerminalReason::NoMovesInCheck),
        },
        Case {
            name: "clock with the child in check and moves",
            fen: FIXTURES[8],
            uci: "a1a8",
            expected: terminal(Outcome::Draw, TerminalReason::FiftyMoveRule),
        },
        Case {
            name: "clock, child not in check",
            fen: FIXTURES[9],
            uci: "e1d2",
            expected: terminal(Outcome::Draw, TerminalReason::FiftyMoveRule),
        },
        Case {
            name: "en-passant escape is the child's only legal move",
            fen: FIXTURES[10],
            uci: "g7g5",
            expected: None,
        },
        Case {
            name: "child keeps castling rights",
            fen: FIXTURES[11],
            uci: "a1a2",
            expected: None,
        },
    ];

    for case in &cases {
        let (board, state) = fixture(case.fen);
        let ctx = board.effect_context_with_state(&state);
        let m = pseudo_move(&board, case.uci);
        let (child_board, child_state) = child(&board, m);

        assert_eq!(
            ctx.terminal_after(&board, m),
            case.expected,
            "{}: terminal_after",
            case.name
        );
        assert_eq!(
            ctx.terminal_after(&board, m),
            child_board.terminal(),
            "{}: clone cross-check",
            case.name
        );
        // The clock the child query uses is the child's, not the parent's.
        assert_eq!(
            ctx.is_terminal_after(&board, m),
            Some(child_board.is_terminal()),
            "{}: is_terminal_after",
            case.name
        );

        match case.name {
            "mate outranks the clock" => {
                // Mate outranks the clock: the child's clock reads 100, yet the
                // reason is the missing move, not the 50-move rule.
                assert_eq!(child_board.rule50(), 100);
            }
            "en-passant escape is the child's only legal move" => {
                use atomic_movegen::Square;
                assert_eq!(child_board.ep_square(), Some(Square::G6));
                assert!(!child_board.checkers().is_empty());
                let mut legal = MoveList::new();
                generate_legal_with_state(&child_board, &child_state, &mut legal);
                let uci: Vec<String> = legal.as_slice().iter().map(|&m| m.to_uci()).collect();
                assert_eq!(uci, vec!["h5g6".to_string()]);
            }
            "child keeps castling rights" => {
                // a1a2 keeps `Kkq`; the castling rook and commoner stay home.
                assert_eq!(child_board.castling_rights(), 0b1101);
                let mut legal = MoveList::new();
                generate_legal_with_state(&child_board, &child_state, &mut legal);
                // The child's side to move is black, which kept both rights.
                let uci: Vec<String> = legal.as_slice().iter().map(|&m| m.to_uci()).collect();
                assert!(uci.contains(&"e8g8".to_string()), "{uci:?}");
                assert!(uci.contains(&"e8c8".to_string()), "{uci:?}");
            }
            _ => {}
        }
    }

    // Black's castling child from the same position: e1f1 keeps `kq` only.
    let (board, state) = fixture(FIXTURES[11]);
    let ctx = board.effect_context_with_state(&state);
    let m = pseudo_move(&board, "e1f1");
    let (child_board, child_state) = child(&board, m);
    assert_eq!(child_board.castling_rights(), 0b1100); // `kq`
    assert_eq!(ctx.terminal_after(&board, m), None);
    let mut legal = MoveList::new();
    generate_legal_with_state(&child_board, &child_state, &mut legal);
    let uci: Vec<String> = legal.as_slice().iter().map(|&m| m.to_uci()).collect();
    assert!(uci.contains(&"e8g8".to_string()), "{uci:?}");
}

#[test]
fn terminal_after_rejects_malformed_moves() {
    let (board, state) = fixture(FIXTURES[11]);
    let ctx = board.effect_context_with_state(&state);
    for m in [
        Move::NONE,
        // Empty origin square.
        Move::make_move(atomic_movegen::Square::E4, atomic_movegen::Square::E5),
        // An enemy piece on the origin is malformed input for a child query:
        // the mover must be the side to move (the black rook on a8).
        Move::make_move(atomic_movegen::Square::A8, atomic_movegen::Square::A7),
    ] {
        assert_eq!(ctx.terminal_after(&board, m), None, "{m}");
        assert_eq!(ctx.is_terminal_after(&board, m), None, "{m}");
        assert_eq!(ctx.has_legal_move_after(&board, m), None, "{m}");
        assert_eq!(ctx.outcome_after(&board, m), None, "{m}");
        assert_eq!(board.terminal_after(m, &state), None, "{m}");
    }
}

/// Every query is read-only: the board keeps its FEN and hash.
#[test]
fn child_queries_do_not_touch_the_board() {
    let (board, state) = fixture(FIXTURES[11]);
    let ctx = board.effect_context_with_state(&state);
    let fen_before = board.fen();
    let hash_before = board.hash();
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(&board, &mut pseudo);
    for &m in pseudo.as_slice() {
        let _ = ctx.terminal_after(&board, m);
        let _ = ctx.is_terminal_after(&board, m);
        let _ = ctx.has_legal_move_after(&board, m);
        let _ = ctx.outcome_after(&board, m);
    }
    assert_eq!(board.fen(), fen_before);
    assert_eq!(board.hash(), hash_before);
}

/// Game-over parents: every pseudo-legal move of a board with one commoner is
/// answered from the child, which is extinct.
#[test]
fn terminal_after_on_game_over_parents() {
    // The side to move has a commoner and pieces, but the opponent's commoner
    // is extinct, so every move of the parent is pseudo-legal yet rejected.
    for fen in ["8/8/8/8/8/8/8/R6K w - - 0 1", "r6k/8/8/8/8/8/8/8 b - - 0 1"] {
        let (board, state) = fixture(fen);
        let ctx = board.effect_context_with_state(&state);
        let mut checked = 0u64;
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        for &m in pseudo.as_slice() {
            checked += 1;
            let (child_board, _) = child(&board, m);
            assert_eq!(
                ctx.terminal_after(&board, m),
                child_board.terminal(),
                "{fen} {m}"
            );
        }
        assert!(checked > 0, "{fen}: no pseudo-legal move");
    }
}
