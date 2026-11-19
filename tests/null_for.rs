//! Differential tests for `Board::null_for` and `Board::null_for_advanced`.

use atomic_movegen::{Board, Color, EnPassantMode, Setup, StateInfo, TerminalReason};

const FIXTURES: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "4k3/8/8/2Pp4/8/8/8/4K3 w - d6 0 2",
    "4k3/8/8/8/8/8/8/4K3 w - - 99 50",
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
];

fn fen_roundtrip(board: &Board, attacker: Color) -> Board {
    let mut fields: Vec<String> = board.fen().split_whitespace().map(str::to_owned).collect();
    fields[1] = match attacker {
        Color::White => "w".to_owned(),
        Color::Black => "b".to_owned(),
        _ => unreachable!("Color is one of two values"),
    };
    fields[3] = "-".to_owned();
    Board::from_fen(&fields.join(" ")).expect("round-trip FEN must parse")
}

#[test]
fn null_for_matches_fen_roundtrip() {
    for fen in FIXTURES {
        let board = Board::from_fen(fen).expect("fixture FEN must parse");
        for attacker in [Color::White, Color::Black] {
            let null = board.null_for(attacker);
            let expected = fen_roundtrip(&board, attacker);

            assert_eq!(null.fen(), expected.fen(), "{fen} / {attacker:?}");
            assert_eq!(null.hash(), expected.hash(), "{fen} / {attacker:?}");
            assert_eq!(
                null.hash_with(EnPassantMode::Legal),
                expected.hash_with(EnPassantMode::Legal),
                "{fen} / {attacker:?}"
            );
            assert_eq!(null.side_to_move(), attacker);
            assert_eq!(null.ep_square(), None);
            assert_eq!(null.rule50(), board.rule50());
            assert_eq!(null.game_ply(), expected.game_ply());
            assert_eq!(null.castling_rights(), board.castling_rights());

            // The source board is unchanged.
            assert_eq!(board.fen(), *fen);
        }
    }
}

fn advanced_expected(board: &Board, attacker: Color) -> Board {
    let mut setup = Setup::new();
    for (square, piece) in board.pieces() {
        setup.place(square, piece);
    }
    setup
        .set_side_to_move(attacker)
        .set_castling_rights(board.castling_rights())
        .set_ep_square(None)
        .set_rule50(board.rule50().saturating_add(1))
        .set_game_ply(board.game_ply().saturating_add(1));
    Board::from_setup(&setup).expect("advanced null setup must build")
}

fn assert_advanced_matches_setup(board: &Board, attacker: Color) {
    let advanced = board.null_for_advanced(attacker);
    let expected = advanced_expected(board, attacker);
    let context = format!("{} / {attacker:?}", board.fen());

    assert_eq!(advanced.fen(), expected.fen(), "{context}: FEN");
    assert_eq!(advanced.hash(), expected.hash(), "{context}: hash");
    assert_eq!(
        advanced.hash_with(EnPassantMode::Legal),
        expected.hash_with(EnPassantMode::Legal),
        "{context}: legal hash"
    );
    assert_eq!(advanced.side_to_move(), attacker, "{context}: side");
    assert_eq!(advanced.ep_square(), None, "{context}: ep");
    assert_eq!(
        advanced.rule50(),
        board.rule50().saturating_add(1),
        "{context}: rule50"
    );
    assert_eq!(
        advanced.game_ply(),
        board.game_ply().saturating_add(1),
        "{context}: game ply"
    );
    assert_eq!(advanced.fullmove_number(), expected.fullmove_number());
    assert_eq!(
        advanced.castling_rights(),
        board.castling_rights(),
        "{context}: castling rights"
    );
}

#[test]
fn null_for_advanced_matches_setup() {
    for fen in FIXTURES {
        let board = Board::from_fen(fen).expect("fixture FEN must parse");
        for attacker in [Color::White, Color::Black] {
            assert_advanced_matches_setup(&board, attacker);
            // Both colors of attacker include the same-side case, where the
            // advanced ply deliberately has no FEN-round-trip parity.
            assert_eq!(board.fen(), *fen);
        }
    }
}

#[test]
fn null_for_advanced_saturates_rule50() {
    let board = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 65535 50").unwrap();
    for attacker in [Color::White, Color::Black] {
        let advanced = board.null_for_advanced(attacker);
        assert_eq!(advanced.rule50(), u16::MAX);
        assert_eq!(
            advanced.rule50(),
            advanced_expected(&board, attacker).rule50()
        );
    }
}

#[test]
fn null_for_advanced_reaches_the_fifty_move_branch() {
    let board = Board::from_fen("4k3/8/8/8/8/8/8/4KN2 w - - 99 50").unwrap();
    let advanced = board.null_for_advanced(Color::Black);
    let expected = advanced_expected(&board, Color::Black);

    assert_eq!(advanced.rule50(), 100);
    assert_eq!(advanced.terminal(), expected.terminal());
    assert_eq!(
        advanced.terminal().map(|terminal| terminal.reason),
        Some(TerminalReason::FiftyMoveRule)
    );
    assert!(advanced.is_terminal());
}

#[test]
fn null_for_advanced_handles_large_fullmove() {
    for fen in [
        "4k3/8/8/8/8/8/8/4K3 w - - 0 65535",
        "4k3/8/8/8/8/8/8/4K3 b - - 0 65535",
    ] {
        let board = Board::from_fen(fen).expect("fixture FEN must parse");
        assert_eq!(board.game_ply(), u16::MAX, "{fen}: fixture is saturated");
        for attacker in [Color::White, Color::Black] {
            let advanced = board.null_for_advanced(attacker);
            let expected = advanced_expected(&board, attacker);
            let context = format!("{fen} / {attacker:?}");

            // The internal counter is beyond the public u16 getter, so compare
            // the representable fields and the key. The unit test in
            // `src/board.rs` pins the exact internal `game_ply + 1`.
            assert_eq!(advanced.side_to_move(), attacker, "{context}: side");
            assert_eq!(advanced.ep_square(), None, "{context}: ep");
            assert_eq!(advanced.rule50(), 1, "{context}: rule50");
            assert_eq!(advanced.game_ply(), u16::MAX, "{context}: game ply");
            assert_eq!(advanced.fullmove_number(), u16::MAX, "{context}: fullmove");
            assert_eq!(advanced.hash(), expected.hash(), "{context}: hash");
            assert_eq!(
                advanced.hash_with(EnPassantMode::Legal),
                expected.hash_with(EnPassantMode::Legal),
                "{context}: legal hash"
            );
            assert_eq!(
                advanced.castling_rights(),
                board.castling_rights(),
                "{context}: castling rights"
            );
        }
    }
}

#[test]
fn null_for_advanced_differs_from_null_for_by_the_advanced_clock() {
    let board = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 99 50").unwrap();

    // Same side: both clear ep and keep the side, but `null_for` preserves the
    // clock while the advanced variant increments both clocks.
    let null = board.null_for(Color::White);
    let advanced = board.null_for_advanced(Color::White);
    assert_eq!(null.side_to_move(), advanced.side_to_move());
    assert_eq!(null.ep_square(), advanced.ep_square());
    assert_eq!(null.rule50(), 99);
    assert_eq!(advanced.rule50(), 100);
    assert_eq!(null.game_ply(), 99);
    assert_eq!(advanced.game_ply(), 100);
    assert_ne!(null.fen(), advanced.fen());

    // Flipped side: `null_for` derives the already-advanced ply from the FEN
    // fullmove, so the ply matches here even though the clock still differs.
    let null = board.null_for(Color::Black);
    let advanced = board.null_for_advanced(Color::Black);
    assert_eq!(null.side_to_move(), advanced.side_to_move());
    assert_eq!(null.ep_square(), advanced.ep_square());
    assert_eq!(null.rule50(), 99);
    assert_eq!(advanced.rule50(), 100);
    assert_eq!(null.game_ply(), advanced.game_ply());
    assert_ne!(null.fen(), advanced.fen());
}

#[test]
fn null_for_result_is_an_ordinary_board() {
    let board = Board::from_fen(FIXTURES[0]).unwrap();
    let null = board.null_for(Color::Black);

    // No stale StateInfo is retained: populating a fresh state must agree with
    // the board's own queries.
    let mut state = StateInfo::new();
    null.populate_state(&mut state);
    assert_eq!(state.checkers, null.checkers());
    assert_eq!(state.pinned, null.pinned(null.side_to_move()));
    assert_eq!(
        state.commoners_count,
        null.commoners(null.side_to_move()).count()
    );
    assert_eq!(
        state.them_commoners_count,
        null.commoners(null.side_to_move().flip()).count()
    );
}
