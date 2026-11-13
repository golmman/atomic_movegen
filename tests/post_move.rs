//! Differential tests for the two MG19 additions on the child summary:
//!
//! - `PostMove::terminal` — the child's terminal state, asked about an already
//!   derived summary;
//! - `EffectContext::post_move_and_key` — the summary and the child's Zobrist
//!   key from one derivation.
//!
//! Both contracts are equalities with a cloned `do_move` child, checked for
//! **every pseudo-legal** move (legal or not) of the 41-position perft corpus,
//! the deterministic fixtures below, and seeded random play-outs:
//!
//! ```text
//! post_move(&b, m).unwrap().terminal()
//!     == { let mut c = b.clone(); c.do_move(m, &mut st); c.terminal() }
//! terminal_after(&b, m) == post_move(&b, m).and_then(|p| p.terminal())
//! post_move_and_key(&b, m).1 == b.hash_after(m) == child.hash()
//! ```
//!
//! The precedence split itself (`position::terminal_in_view_light` against the
//! board-side `position::terminal_in_view`) is pinned by unit tests in
//! `src/position.rs`, because both functions are crate-private.

mod common;

use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{
    AfterState, Board, Move, MoveList, Outcome, Square, StateInfo, Terminal, TerminalReason,
};
use common::{walk_corpus_and_fixtures, walk_playouts};

/// Deterministic fixtures: one row per rule branch of the child precedence
/// (the same positions `tests/terminal_after.rs` and `tests/child_state.rs`
/// use), plus the key fixtures at the bottom.
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
    // A child whose only legal move is an en-passant escape.
    "8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41",
    // A child that keeps (some) castling rights.
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    // Every pseudo-legal move here explodes the mover's own commoner.
    "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
    // Game-over parents (one commoner is already extinct).
    "8/8/8/8/8/8/8/R6K w - - 0 1",
    "r6k/8/8/8/8/8/8/8 b - - 0 1",
    // Key fixtures: en passant available, a pawn about to double-push, and a
    // castling-board with both rights.
    "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
    "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
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

/// The two additions, checked against the cloned child on every pseudo-legal
/// move of the corpora and the play-outs.
#[test]
fn post_move_terminal_and_key_match_cloned_child() {
    let mut positions = 0u64;
    let mut candidates = 0u64;
    let mut decided = 0u64;
    let mut check = |board: &Board, state: &StateInfo| {
        let ctx = board.effect_context_with_state(state);
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);
        for &m in pseudo.as_slice() {
            candidates += 1;
            let (child, child_state) = child(board, m);
            let fen = board.fen();

            // MG19a: terminality on the already derived summary.
            let post = ctx.post_move(board, m).expect("pseudo-legal move");
            let expected = child.terminal();
            assert_eq!(
                post.terminal(),
                expected,
                "{fen} {m}: PostMove::terminal (legal: {:?})",
                ctx.is_legal(board, m)
            );
            assert_eq!(
                post.terminal(),
                child.terminal_with_state(&child_state),
                "{fen} {m}: PostMove::terminal vs populated child state"
            );
            if post.terminal().is_some() {
                decided += 1;
            }

            // "Zero behaviour change, they call it": `terminal_after` is the
            // summary query plus the summary's terminality.
            assert_eq!(
                ctx.terminal_after(board, m),
                ctx.post_move(board, m).and_then(|p| p.terminal()),
                "{fen} {m}: terminal_after"
            );
            // ... and `child_outcome` is the same terminality behind the
            // legality gate.
            let arm = ctx.child_outcome(board, m);
            if ctx.is_legal(board, m) {
                assert_eq!(
                    arm.terminal(),
                    post.terminal(),
                    "{fen} {m}: child_outcome terminality"
                );
                match post.terminal() {
                    Some(t) => assert_eq!(arm, AfterState::Terminal(t), "{fen} {m}"),
                    None => assert_eq!(arm, AfterState::Undecided, "{fen} {m}"),
                }
            } else {
                assert_eq!(arm, AfterState::Illegal, "{fen} {m}: Illegal wins");
            }

            // MG19b: the key next to the summary.
            let (post_again, key) = ctx.post_move_and_key(board, m).expect("pseudo-legal move");
            assert_eq!(post_again, post, "{fen} {m}: summary from the keyed query");
            assert_eq!(key, child.hash(), "{fen} {m}: key vs cloned child");
            assert_eq!(key, board.hash_after(m), "{fen} {m}: key vs hash_after");
            assert_eq!(
                (key, post.rule50()),
                board.hash_and_rule50_after(m),
                "{fen} {m}: key+clock vs hash_and_rule50_after"
            );
            // The keyed queries are consistent with each other and with the
            // board's own facts.
            assert_eq!(key, board.hash_after(m));
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
        "post_move: {positions} positions, {candidates} pseudo-legal moves, \
         {decided} decided children"
    );
}

/// One row per branch of the precedence, on the summary itself.
#[test]
fn post_move_terminal_deterministic_fixtures() {
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
            name: "undecided child",
            fen: FIXTURES[11],
            uci: "a1a2",
            expected: None,
        },
    ];

    for case in &cases {
        let (board, state) = fixture(case.fen);
        let ctx = board.effect_context_with_state(&state);
        let m = pseudo_move(&board, case.uci);
        let (child, _) = child(&board, m);

        let post = ctx.post_move(&board, m).expect("pseudo-legal move");
        assert_eq!(post.terminal(), case.expected, "{}", case.name);
        assert_eq!(
            post.terminal(),
            child.terminal(),
            "{}: clone cross-check",
            case.name
        );
        assert_eq!(
            ctx.terminal_after(&board, m),
            case.expected,
            "{}: terminal_after",
            case.name
        );
    }
}

/// Malformed input is rejected by both additions, exactly like `post_move`.
#[test]
fn post_move_additions_reject_malformed_moves() {
    let (board, state) = fixture(FIXTURES[11]);
    let ctx = board.effect_context_with_state(&state);
    for m in [
        Move::NONE,
        // Empty origin square.
        Move::make_move(Square::E4, Square::E5),
        // An enemy piece on the origin: the mover must be the side to move.
        Move::make_move(Square::A8, Square::A7),
    ] {
        assert_eq!(ctx.post_move_and_key(&board, m), None, "{m}");
    }
    assert_eq!(
        ctx.post_move_and_key(&board, Move::NONE).map(|(p, _)| p),
        None
    );
}

/// The key rows the plan calls out, each also asserted against the cloned child
/// and against `Board::hash_after`: quiet double push (ep key in), capture with
/// a blast (several pieces out), en passant (victim out, ep in), quiet
/// promotion, capturing promotion, castling (commoner + rook, rights out),
/// a capture that removes a rook on its home square (rights out), a quiet
/// back-rank move (rights unchanged), a capture that drops a stored ep square,
/// and a capture of the last enemy commoner.
#[test]
fn child_key_deterministic_fixtures() {
    struct Case {
        name: &'static str,
        fen: &'static str,
        uci: &'static str,
    }
    let cases = [
        Case {
            name: "quiet double push (ep key in)",
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            uci: "e2e4",
        },
        Case {
            name: "pawn capture with a blast",
            fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            uci: "e4d5",
        },
        Case {
            name: "en passant capture",
            fen: "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
            uci: "e4d3",
        },
        Case {
            name: "quiet promotion",
            fen: "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            uci: "b7b8q",
        },
        Case {
            name: "capturing promotion",
            fen: "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            uci: "b7a8q",
        },
        Case {
            name: "castling (commoner and rook, rights out)",
            fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            uci: "e1g1",
        },
        Case {
            name: "queen-side castling",
            fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            uci: "e1c1",
        },
        Case {
            name: "capture removes a home rook (rights out)",
            fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            uci: "a1a8",
        },
        Case {
            name: "quiet back-rank move (rights unchanged)",
            fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            uci: "a1a2",
        },
        Case {
            name: "quiet move after a double push (stored ep dropped)",
            fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            uci: "f1b5",
        },
        Case {
            name: "capture of the last enemy commoner",
            fen: "k7/8/8/8/8/8/8/RK6 w - - 0 1",
            uci: "a1a8",
        },
        Case {
            name: "quiet pawn push keeping a stored ep",
            fen: "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
            uci: "e4e3",
        },
        Case {
            name: "ep capture from a castling-rights board",
            fen: "r3k2r/8/8/3pP3/8/8/8/R3K2R w KQkq d6 0 1",
            uci: "e5d6",
        },
    ];

    for case in &cases {
        let (board, state) = fixture(case.fen);
        let ctx = board.effect_context_with_state(&state);
        let m = pseudo_move(&board, case.uci);
        let (child, _) = child(&board, m);

        let (post, key) = ctx
            .post_move_and_key(&board, m)
            .unwrap_or_else(|| panic!("{}: keyed summary", case.name));
        assert_eq!(key, child.hash(), "{}: clone cross-check", case.name);
        assert_eq!(key, board.hash_after(m), "{}: hash_after", case.name);
        assert_eq!(
            (key, post.rule50()),
            board.hash_and_rule50_after(m),
            "{}: hash_and_rule50_after",
            case.name
        );
        assert_eq!(
            post,
            ctx.post_move(&board, m).expect("pseudo-legal move"),
            "{}: summary",
            case.name
        );
    }
}

/// Every query is read-only: the board keeps its FEN and hash, and asking for
/// the key twice yields the same key.
#[test]
fn post_move_additions_do_not_touch_the_board() {
    let (board, state) = fixture(FIXTURES[11]);
    let ctx = board.effect_context_with_state(&state);
    let fen_before = board.fen();
    let hash_before = board.hash();
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(&board, &mut pseudo);
    for &m in pseudo.as_slice() {
        let first = ctx.post_move_and_key(&board, m).expect("pseudo-legal").1;
        if let Some(post) = ctx.post_move(&board, m) {
            let _ = post.terminal();
        }
        let second = ctx.post_move_and_key(&board, m).expect("pseudo-legal").1;
        assert_eq!(first, second, "{m}");
    }
    assert_eq!(board.fen(), fen_before);
    assert_eq!(board.hash(), hash_before);

    // The board still plays the moves it generated.
    let mut legal = MoveList::new();
    generate_legal_with_state(&board, &state, &mut legal);
    assert!(!legal.is_empty());
}
