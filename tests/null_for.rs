//! Differential tests for `Board::null_for` against the FEN round-trip.

use atomic_movegen::{Board, Color, EnPassantMode, StateInfo};

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
