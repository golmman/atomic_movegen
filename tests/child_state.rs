//! Differential tests for the two-level child answer and the child summary:
//! `EffectContext::child_outcome` / `Board::child_outcome` (MG16) and
//! `EffectContext::post_move` / `EffectContext::effect_and_post` (MG17).
//!
//! Two independent contracts are checked against a cloned `do_move` child for
//! every pseudo-legal move:
//!
//! ```text
//! arm:      child_outcome(&b, m) == Illegal   iff !b.legal(m, &st)
//!           child_outcome(&b, m) == Terminal(t) iff child.terminal() == Some(t)
//!           child_outcome(&b, m) == Undecided  iff child.terminal() == None
//! content:  post_move(&b, m) describes exactly the child `do_move` builds
//! ```
//!
//! Positions come from the 41-position perft corpus in `tests/perft_values.md`,
//! from the deterministic fixtures below, and from seeded random play-outs.

mod common;

use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{
    AfterState, Board, Color, Move, MoveList, NO_PIECE, Outcome, PieceType, Square, StateInfo,
    Terminal, TerminalReason,
};
use common::{perft_corpus, walk_corpus_and_fixtures, walk_playouts};

/// Deterministic fixtures: one row per rule branch of the child view, plus the
/// illegal-move and content fixtures.
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

/// The child summary must equal the child `do_move` builds, field for field.
fn assert_content(board: &Board, m: Move, deep: bool) {
    let ctx = board.effect_context();
    let Some(post) = ctx.post_move(board, m) else {
        panic!(
            "{} {m}: post_move rejected a pseudo-legal move",
            board.fen()
        );
    };
    let (child_board, _) = child(board, m);

    assert_eq!(post.side_to_move(), child_board.side_to_move(), "{m}: side");
    assert_eq!(post.occupied(), child_board.occupied(), "{m}: occupied");
    for c in [Color::White, Color::Black] {
        assert_eq!(
            post.pieces_color(c),
            child_board.pieces_color(c),
            "{m}: pieces of {c:?}"
        );
    }
    for pt in [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::Commoner,
    ] {
        assert_eq!(
            post.pieces_pt(pt),
            child_board.pieces_pt(pt),
            "{m}: pieces of {pt:?}"
        );
        for c in [Color::White, Color::Black] {
            assert_eq!(
                post.pieces_color_pt(c, pt),
                child_board.pieces_color_pt(c, pt),
                "{m}: {c:?} {pt:?}"
            );
        }
    }

    if deep {
        for index in 0..64 {
            let sq = Square::from_index(index);
            assert_eq!(
                post.piece_on(sq),
                child_board.piece_on(sq),
                "{m}: piece on {sq:?}"
            );
        }
    } else {
        // Sample the squares a move can touch: its own squares, the castling
        // home squares, and the four corners.
        for sq in [
            m.from_sq(),
            m.to_sq(),
            Square::A1,
            Square::E1,
            Square::H1,
            Square::A8,
            Square::E8,
            Square::H8,
        ] {
            assert_eq!(
                post.piece_on(sq),
                child_board.piece_on(sq),
                "{m}: piece on {sq:?}"
            );
        }
    }

    assert_eq!(
        post.casualties(),
        board.blast_casualties(m),
        "{m}: casualties"
    );
    assert_eq!(post.is_capture(), board.is_capture(m), "{m}: is_capture");
    assert_eq!(post.ep_square(), child_board.ep_square(), "{m}: ep");
    assert_eq!(
        post.castling_rights(),
        child_board.castling_rights(),
        "{m}: castling rights"
    );
    assert_eq!(post.rule50(), child_board.rule50(), "{m}: rule50");

    // `effect_and_post` is one analysis pass for both halves.
    let ctx = board.effect_context();
    let (effect, post_again) = ctx.effect_and_post(board, m);
    assert_eq!(effect, ctx.effect(board, m), "{m}: effect_and_post/effect");
    assert_eq!(post_again, Some(post), "{m}: effect_and_post/post_move");
}

/// The arm must match `Board::legal` and the cloned child's `terminal()`.
fn assert_arm(board: &Board, state: &StateInfo, m: Move) {
    let ctx = board.effect_context_with_state(state);
    let (child_board, _) = child(board, m);

    let actual = ctx.child_outcome(board, m);
    let legal = board.legal(m, state);
    let expected = if legal {
        match child_board.terminal() {
            Some(t) => AfterState::Terminal(t),
            None => AfterState::Undecided,
        }
    } else {
        AfterState::Illegal
    };

    assert_eq!(
        actual,
        expected,
        "{} {m}: child_outcome (child terminal: {:?})",
        board.fen(),
        child_board.terminal()
    );

    // The helpers agree with the variant.
    assert_eq!(actual.is_illegal(), matches!(actual, AfterState::Illegal));
    assert_eq!(
        actual.is_undecided(),
        matches!(actual, AfterState::Undecided)
    );
    assert_eq!(actual.is_terminal(), expected.terminal().is_some());
    assert_eq!(actual.terminal(), expected.terminal());
    assert_eq!(actual.outcome(), expected.terminal().map(|t| t.outcome));

    // The board wrapper is the same query.
    assert_eq!(
        board.child_outcome(m, state),
        actual,
        "{} {m}: Board::child_outcome",
        board.fen()
    );

    // The documented relationship to `outcome_after`.
    assert_eq!(
        ctx.outcome_after(board, m),
        actual.outcome(),
        "{} {m}: outcome_after",
        board.fen()
    );
    assert_eq!(
        ctx.outcome_after(board, m),
        if actual.is_terminal() {
            actual.terminal().map(|t| t.outcome)
        } else {
            None
        },
        "{} {m}: documented outcome_after mapping",
        board.fen()
    );
    // `is_legal` is the arm's illegality flag.
    assert_eq!(
        ctx.is_legal(board, m),
        !actual.is_illegal(),
        "{} {m}",
        board.fen()
    );
}

#[test]
fn child_outcome_matches_legal_and_cloned_child() {
    let mut positions = 0u64;
    let mut candidates = 0u64;
    let mut arm = [0u64; 3];

    let mut check = |board: &Board, state: &StateInfo, deep: bool| {
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(board, &mut pseudo);
        let mut legal = MoveList::new();
        generate_legal_with_state(board, state, &mut legal);
        for &m in pseudo.as_slice() {
            candidates += 1;

            // The oracle, three ways: `Board::legal`, the legal move list, and
            // the cloned child.
            assert_eq!(
                board.legal(m, state),
                legal.as_slice().contains(&m),
                "{} {m}: legal/legal-list disagree",
                board.fen()
            );
            assert_arm(board, state, m);
            assert_content(board, m, deep);

            let ctx = board.effect_context_with_state(state);
            let after = ctx.child_outcome(board, m);
            match (after.is_illegal(), after.is_terminal()) {
                (true, _) => arm[0] += 1,
                (false, true) => arm[2] += 1,
                (false, false) => arm[1] += 1,
            }
        }
        positions += 1;
    };

    let mut check_deep = |board: &Board, state: &StateInfo| check(board, state, true);
    walk_corpus_and_fixtures(FIXTURES, &mut check_deep);

    let mut check_shallow = |board: &Board, state: &StateInfo| check(board, state, false);
    let played = walk_playouts(&mut check_shallow);

    assert!(
        played > 5_000,
        "expected thousands of played-out positions, got {played}"
    );
    assert!(
        candidates > 10_000,
        "expected many pseudo-legal candidates, got {candidates}"
    );
    assert!(arm[0] > 1_000, "expected illegal moves, got {}", arm[0]);
    assert!(
        arm[1] > 10_000,
        "expected undecided children, got {}",
        arm[1]
    );
    assert!(arm[2] > 1_000, "expected terminal children, got {}", arm[2]);
    eprintln!(
        "child_outcome: {positions} positions, {candidates} pseudo-legal moves \
         ({} illegal, {} undecided, {} terminal)",
        arm[0], arm[1], arm[2]
    );
}

/// One row per `TerminalReason`, one `Undecided` with a non-trivial child, and
/// the two `Illegal` fixtures.
#[test]
fn child_outcome_deterministic_fixtures() {
    struct Case {
        name: &'static str,
        fen: &'static str,
        uci: &'static str,
        expected: AfterState,
    }
    let terminal = |outcome, reason| AfterState::Terminal(Terminal { outcome, reason });
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
            name: "material draw from a capture",
            fen: FIXTURES[4],
            uci: "e4c5",
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
            name: "en-passant escape, child undecided",
            fen: FIXTURES[10],
            uci: "g7g5",
            expected: AfterState::Undecided,
        },
        Case {
            name: "castling rights kept, child undecided",
            fen: FIXTURES[11],
            uci: "a1a2",
            expected: AfterState::Undecided,
        },
        Case {
            name: "self-explosion is illegal",
            fen: FIXTURES[3],
            uci: "e1d1",
            expected: AfterState::Illegal,
        },
    ];

    for case in &cases {
        let (board, state) = fixture(case.fen);
        let ctx = board.effect_context_with_state(&state);
        let m = pseudo_move(&board, case.uci);
        let (child_board, _) = child(&board, m);

        assert_eq!(
            ctx.child_outcome(&board, m),
            case.expected,
            "{}: child_outcome",
            case.name
        );
        assert_eq!(
            board.child_outcome(m, &state),
            case.expected,
            "{}: Board::child_outcome",
            case.name
        );
        assert_eq!(
            ctx.child_outcome(&board, m),
            if board.legal(m, &state) {
                match child_board.terminal() {
                    Some(t) => AfterState::Terminal(t),
                    None => AfterState::Undecided,
                }
            } else {
                AfterState::Illegal
            },
            "{}: clone cross-check",
            case.name
        );
        // The summary is a content fact even for an illegal move.
        assert_content(&board, m, true);
    }
}

/// A position whose pseudo-legal moves are all illegal, and the en-passant
/// child: undecided, with an ep target and a single legal answer.
#[test]
fn illegal_parent_and_en_passant_child_details() {
    // A position where every candidate destroys the mover's own commoner.
    let (board, state) = fixture(FIXTURES[12]);
    let ctx = board.effect_context_with_state(&state);
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(&board, &mut pseudo);
    assert!(!pseudo.is_empty());
    let mut legal = MoveList::new();
    generate_legal_with_state(&board, &state, &mut legal);
    assert!(legal.is_empty());
    for &m in pseudo.as_slice() {
        assert_eq!(ctx.child_outcome(&board, m), AfterState::Illegal, "{m}");
        assert_eq!(ctx.outcome_after(&board, m), None, "{m}");
    }

    // The en-passant child: undecided, ep target `g6`, exactly one legal move.
    let (board, state) = fixture(FIXTURES[10]);
    let ctx = board.effect_context_with_state(&state);
    let m = pseudo_move(&board, "g7g5");
    assert_eq!(ctx.child_outcome(&board, m), AfterState::Undecided);
    let post = ctx.post_move(&board, m).expect("pseudo-legal move");
    assert_eq!(post.ep_square(), Some(Square::G6));
    assert_eq!(post.side_to_move(), Color::White);
    let (child_board, child_state) = child(&board, m);
    assert!(!child_board.checkers().is_empty());
    let mut legal = MoveList::new();
    generate_legal_with_state(&child_board, &child_state, &mut legal);
    let uci: Vec<String> = legal.as_slice().iter().map(|&m| m.to_uci()).collect();
    assert_eq!(uci, vec!["h5g6".to_string()]);
}

/// Castling rights, promotion and clock facts of the summary.
#[test]
fn post_move_position_facts() {
    // a1a2 keeps `Kkq`; the ep target stays empty.
    let (board, state) = fixture(FIXTURES[11]);
    let ctx = board.effect_context_with_state(&state);
    let m = pseudo_move(&board, "a1a2");
    let post = ctx.post_move(&board, m).expect("pseudo-legal move");
    assert_eq!(post.castling_rights(), 0b1101);
    assert_eq!(post.ep_square(), None);
    assert_eq!(post.rule50(), 1);
    assert!(!post.is_capture());
    assert!(post.casualties().is_empty());
    assert_eq!(post.piece_on(Square::A1), NO_PIECE);
    assert_eq!(
        post.piece_on(Square::A2),
        atomic_movegen::make_piece(Color::White, PieceType::Rook)
    );

    // A quiet commoner move drops both of its own rights but keeps black's.
    let m = pseudo_move(&board, "e1f1");
    let post = ctx.post_move(&board, m).expect("pseudo-legal move");
    assert_eq!(post.castling_rights(), 0b1100);

    // A double pawn push records the intermediate ep square.
    let board = Board::new();
    let ctx = board.effect_context();
    let m = board.move_from_uci("e2e4").expect("legal move");
    let post = ctx.post_move(&board, m).expect("pseudo-legal move");
    assert_eq!(post.ep_square(), Some(Square::E3));
    assert_eq!(post.side_to_move(), Color::Black);
    assert_eq!(post.rule50(), 0);

    // A quiet promotion: the pawn leaves `from`, the promoted piece appears on
    // `to`, and nothing is removed.
    let (board, state) = fixture("n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1");
    let ctx = board.effect_context_with_state(&state);
    let m = pseudo_move(&board, "b7b8q");
    let post = ctx.post_move(&board, m).expect("pseudo-legal move");
    assert_eq!(post.piece_on(Square::B7), NO_PIECE);
    assert_eq!(
        post.piece_on(Square::B8),
        atomic_movegen::make_piece(Color::White, PieceType::Queen)
    );
    assert!(!post.is_capture());
}

#[test]
fn child_queries_reject_malformed_moves() {
    let (board, state) = fixture(FIXTURES[11]);
    let ctx = board.effect_context_with_state(&state);
    for m in [
        Move::NONE,
        // Empty origin square.
        Move::make_move(Square::E4, Square::E5),
        // An enemy piece on the origin (the black rook on a8).
        Move::make_move(Square::A8, Square::A7),
    ] {
        assert_eq!(ctx.post_move(&board, m), None, "{m}");
        assert_eq!(ctx.effect_and_post(&board, m).1, None, "{m}");
        assert_eq!(ctx.child_outcome(&board, m), AfterState::Illegal, "{m}");
        assert_eq!(board.child_outcome(m, &state), AfterState::Illegal, "{m}");
        assert_eq!(ctx.outcome_after(&board, m), None, "{m}");
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
        let _ = ctx.post_move(&board, m);
        let _ = ctx.effect_and_post(&board, m);
        let _ = ctx.child_outcome(&board, m);
        let _ = ctx.outcome_after(&board, m);
    }
    assert_eq!(board.fen(), fen_before);
    assert_eq!(board.hash(), hash_before);
}

/// The fixtures used here are corpus positions too, so a rules change cannot
/// silently invalidate the table above.
#[test]
fn fixtures_parse() {
    for fen in FIXTURES {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("{fen}: {e}"));
        assert!(!board.fen().is_empty());
    }
    assert_eq!(perft_corpus().len(), 41);
}
