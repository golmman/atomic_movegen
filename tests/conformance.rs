//! Verification of the frozen conformance corpus in `src/conformance.rs`.
//!
//! Every published field is compared against the engine, so regenerating the
//! corpus after a rules, key, or generation-order change is a deliberate act:
//! the data and `ZOBRIST_VERSION` must be updated together.

use atomic_movegen::conformance::{CASES, CHILD_CASES, ConformanceCase};
use atomic_movegen::movegen::{generate_legal, generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{
    Board, CONFORMANCE_CASES, EnPassantMode, Move, MoveList, StateInfo, ZOBRIST_VERSION,
};

fn uci_list(case: &ConformanceCase, legal: bool) -> Vec<String> {
    let board = Board::from_fen(case.fen).expect("conformance FEN must parse");
    let mut moves = MoveList::new();
    if legal {
        generate_legal(&board, &mut moves);
    } else {
        generate_pseudo_legal(&board, &mut moves);
    }
    moves.as_slice().iter().map(|m| m.to_uci()).collect()
}

fn is_subsequence(needle: &[&str], haystack: &[String]) -> bool {
    let mut expected = needle.iter();
    let mut wanted = expected.next();
    for item in haystack {
        if wanted == Some(&item.as_str()) {
            wanted = expected.next();
        }
    }
    wanted.is_none()
}

#[test]
fn conformance_corpus_verifies() {
    // The corpus was generated with this key schema. A key-schema change must
    // regenerate the corpus and bump both this assertion and ZOBRIST_VERSION.
    assert_eq!(
        ZOBRIST_VERSION, 1,
        "regenerate the conformance corpus when the Zobrist schema changes"
    );

    for case in CASES {
        let board = Board::from_fen(case.fen)
            .unwrap_or_else(|e| panic!("case {:?}: FEN does not parse: {e}", case.name));

        assert_eq!(
            uci_list(case, false),
            case.pseudo_legal,
            "case {:?}: pseudo-legal order drift",
            case.name
        );
        assert_eq!(
            uci_list(case, true),
            case.legal,
            "case {:?}: legal order drift",
            case.name
        );
        assert_eq!(
            board.outcome(),
            case.outcome,
            "case {:?}: outcome drift",
            case.name
        );
        assert_eq!(
            board.terminal().map(|t| t.reason),
            case.terminal_reason,
            "case {:?}: terminal reason drift",
            case.name
        );
        assert_eq!(
            board.is_terminal(),
            case.is_terminal,
            "case {:?}: is_terminal drift",
            case.name
        );
        assert_eq!(
            board.hash(),
            case.hash_always,
            "case {:?}: Always key drift",
            case.name
        );
        assert_eq!(
            board.hash_with(EnPassantMode::Legal),
            case.hash_legal,
            "case {:?}: Legal key drift",
            case.name
        );
        assert_eq!(
            board.rule50(),
            case.rule50,
            "case {:?}: rule50 drift",
            case.name
        );
        assert_eq!(
            board.game_ply(),
            case.game_ply,
            "case {:?}: game ply drift",
            case.name
        );
    }
}

#[test]
fn conformance_corpus_covers_rules() {
    const REQUIRED_PREFIXES: &[&str] = &[
        "startpos",
        "quiet-",
        "capture-",
        "en-passant-",
        "castling-",
        "promotion-",
        "touching-",
        "check-",
        "checkmate",
        "stalemate",
        "extinction-",
        "material-",
        "fifty-move-",
        "checkmate-on-clock",
        "pinned-",
        "blast-",
        "self-explosion-",
        "max-legal-",
        "perft-",
    ];
    for prefix in REQUIRED_PREFIXES {
        assert!(
            CASES.iter().any(|case| case.name.starts_with(prefix)),
            "conformance corpus lost its {prefix:?} category"
        );
    }
    for i in 1..=14 {
        let name = format!("perft-{i}");
        assert!(
            CASES.iter().any(|case| case.name == name),
            "conformance corpus lost {name:?}"
        );
    }
}

#[test]
fn conformance_names_are_unique() {
    for (i, case) in CASES.iter().enumerate() {
        assert!(!case.name.is_empty());
        assert!(!case.fen.is_empty());
        for other in &CASES[i + 1..] {
            assert_ne!(
                case.name, other.name,
                "duplicate conformance name {:?}",
                case.name
            );
        }
    }
}

#[test]
fn conformance_legal_is_subset_of_pseudo() {
    for case in CASES {
        assert!(
            is_subsequence(case.legal, &uci_list(case, false)),
            "case {:?}: legal moves are not an ordered subset of the pseudo-legal stream",
            case.name
        );
    }
}

#[test]
fn conformance_root_reexport_matches_module() {
    assert_eq!(CONFORMANCE_CASES.len(), CASES.len());
    for (a, b) in CONFORMANCE_CASES.iter().zip(CASES) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.hash_always, b.hash_always);
    }
}

/// Resolve the board-aware encoding of a child case's UCI move.
fn child_case_move(board: &Board, uci: &str) -> Move {
    if let Some(m) = board.move_from_uci(uci) {
        return m;
    }
    let parsed = Move::from_uci(uci).expect("case UCI must parse");
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);
    pseudo
        .as_slice()
        .iter()
        .copied()
        .find(|m| m.from_sq() == parsed.from_sq() && m.to_sq() == parsed.to_sq())
        .unwrap_or_else(|| panic!("{uci} is not pseudo-legal in {}", board.fen()))
}

#[test]
fn child_conformance_corpus_verifies() {
    for case in CHILD_CASES {
        let board = Board::from_fen(case.fen)
            .unwrap_or_else(|e| panic!("child case {:?}: FEN does not parse: {e}", case.name));
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let m = child_case_move(&board, case.uci);
        let ctx = board.effect_context_with_state(&state);

        // The move is pseudo-legal by contract.
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        assert!(
            pseudo.as_slice().contains(&m),
            "child case {:?}: {} is not pseudo-legal",
            case.name,
            case.uci
        );

        assert_eq!(
            ctx.child_outcome(&board, m),
            case.after_state,
            "child case {:?}: child_outcome drift",
            case.name
        );
        assert_eq!(
            board.child_outcome(m, &state),
            case.after_state,
            "child case {:?}: Board::child_outcome drift",
            case.name
        );
        assert_eq!(
            ctx.outcome_after(&board, m),
            case.after_state.outcome(),
            "child case {:?}: outcome_after drift",
            case.name
        );
        assert_eq!(
            ctx.is_legal(&board, m),
            !case.after_state.is_illegal(),
            "child case {:?}: illegality drift",
            case.name
        );

        // The child legal-move count is what a cloned child generates.
        let mut child = board.clone();
        let mut child_state = StateInfo::new();
        child.do_move(m, &mut child_state);
        child.populate_state(&mut child_state);
        let mut legal = MoveList::new();
        generate_legal_with_state(&child, &child_state, &mut legal);
        assert_eq!(
            legal.len(),
            case.child_legal_count,
            "child case {:?}: child legal move count drift",
            case.name
        );
        assert_eq!(
            ctx.has_legal_move_after(&board, m),
            Some(case.child_legal_count > 0),
            "child case {:?}: has_legal_move_after drift",
            case.name
        );
    }
}
