//! Public-API robustness: malformed input must not panic, and do/undo must
//! restore every observable field exactly.
//!
//! The predicates exercised here are the documented total ones. Methods whose
//! docs name a malformed-move or stale-`StateInfo` panic are intentionally not
//! fed garbage.

mod common;

use atomic_movegen::movegen::generate_legal_with_state;
use atomic_movegen::{
    Board, CONFORMANCE_CASES, Color, EnPassantMode, FenPolicy, Move, MoveList, StateInfo,
};

fn robustness_boards() -> Vec<Board> {
    let mut fens = common::perft_corpus();
    fens.extend(CONFORMANCE_CASES.iter().map(|case| case.fen.to_owned()));
    fens.extend(common::PLAYOUT_FENS.iter().map(|fen| (*fen).to_owned()));
    fens.into_iter()
        .map(|fen| Board::from_fen(&fen).expect("fixture FEN must parse"))
        .collect()
}

/// Boards spanning the wishlist-2 edge shapes: both colors, castling, a live
/// en-passant target, terminal boards, and deliberately impossible placements
/// (pawns on the back ranks) that the permissive parser still accepts.
fn wishlist2_robustness_boards() -> Vec<Board> {
    [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
        "8/8/8/8/8/8/8/8 w - - 0 1",
        "p3k3/8/8/8/8/8/8/p6K b - - 0 1",
        "P3K3/8/8/8/8/8/8/P6k w - - 0 1",
        "4k3/8/8/8/8/8/8/R7 w - - 0 1",
    ]
    .into_iter()
    .map(|fen| Board::from_fen(fen).expect("robustness FEN must parse"))
    .collect()
}

#[test]
fn wishlist2_predicates_are_total_for_arbitrary_move_bits() {
    for board in wishlist2_robustness_boards() {
        let counts = board.counts();
        let ctx = board.legality_context(counts);
        std::hint::black_box(ctx.occupied());
        for bits in 0..=u16::MAX {
            let Some(m) = Move::from_bits(bits) else {
                continue;
            };
            for mode in [EnPassantMode::Always, EnPassantMode::Legal] {
                std::hint::black_box(board.child_effect(m, counts, mode));
            }
            std::hint::black_box(board.child_effect_with_occupancy(
                m,
                board.occupied(),
                counts,
                EnPassantMode::Legal,
            ));
            std::hint::black_box(ctx.legal(m));
        }
        for attacker in [Color::White, Color::Black] {
            std::hint::black_box(board.null_for(attacker));
            std::hint::black_box(board.null_for_advanced(attacker));
        }
    }
}

#[test]
fn arbitrary_move_bits_never_panic_in_total_predicates() {
    for board in robustness_boards() {
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let mut text = String::with_capacity(8);
        for bits in 0..=u16::MAX {
            let Some(m) = Move::from_bits(bits) else {
                continue;
            };
            std::hint::black_box(board.is_pseudo_legal(m));
            std::hint::black_box(board.is_legal(m));
            std::hint::black_box(board.is_legal_with_state(m, &state));
            std::hint::black_box(board.is_capture(m));
            std::hint::black_box(board.blast_casualties(m));
            std::hint::black_box(board.is_zeroing(m));
            std::hint::black_box(board.is_double_push(m));
            text.clear();
            m.write_uci(&mut text).expect("String writer cannot fail");
        }
    }
}

#[test]
fn malformed_fen_and_random_ascii_never_panic() {
    let curated = [
        "",
        " ",
        "        ",
        "not a fen",
        "♞♞♞♞♞♞♞♞/8/8/8/8/8/8/8 w - - 0 1",
        "8/8/8/8/8/8/8/8 w - - 0 1",
        "0/8/8/8/8/8/8/8 w - - 0 1",
        "8/8/8/8/8/8/8/8/8 w - - 0 1",
        "8/8/8/8/8/8/8 w - - 0 1",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x - - 0 1",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w Z - 0 1",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e9 0 1",
        "4k3/8/8/8/8/8/8/4K3 w - - 65536 1",
        "4k3/8/8/8/8/8/8/4K3 w - - 0 65536",
        "4k3/8/8/8/8/8/8/4K3 w - - -1 1",
        "4k3/8/8/8/8/8/8/4K3 w - - 0 0",
        "4k3/8/8/8/8/8/8/4K3 w KQkq - 0 1",
    ];
    for fen in curated {
        let _ = Board::from_fen(fen);
        let _ = Board::from_fen_with(fen, FenPolicy::Strict);
        let _ = Board::from_fen_with(fen, FenPolicy::Lichess);
        let _ = Board::from_fen_with(fen, FenPolicy::Permissive);
    }

    let mut rng = common::Xorshift::new(0xF3A9_5C21_7B4D_0E86);
    for len in 0..=96usize {
        for _ in 0..8 {
            let mut text = String::with_capacity(len);
            for _ in 0..len {
                text.push((0x20 + (rng.next() % 95) as u8) as char);
            }
            let _ = Board::from_fen(&text);
            for policy in [
                FenPolicy::Strict,
                FenPolicy::AllowTerminal,
                FenPolicy::AllowImpossibleCheck,
                FenPolicy::Permissive,
                FenPolicy::Lichess,
            ] {
                let _ = Board::from_fen_with(&text, policy);
            }
        }
    }
}

#[test]
fn interchange_random_text_never_panics() {
    let boards = [
        Board::new(),
        Board::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2").unwrap(),
        Board::from_fen("n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1").unwrap(),
        Board::from_fen("8/5k2/p7/8/P7/1P1n1K2/8/2q5 b - - 1 40").unwrap(),
    ];
    let mut rng = common::Xorshift::new(0x5EED_1234_ABCD_9876);
    for len in 0..=48usize {
        for _ in 0..12 {
            let mut text = String::with_capacity(len);
            for _ in 0..len {
                text.push((0x20 + (rng.next() % 95) as u8) as char);
            }
            let _ = Move::from_uci(&text);
            for board in &boards {
                let _ = board.move_from_uci(&text);
                let _ = board.move_from_san(&text);
                let _ =
                    board.san(Move::from_bits((rng.next() & 0xffff) as u16).unwrap_or(Move::NONE));
                let mut out = String::new();
                let _ = board.write_san(
                    Move::from_bits((rng.next() & 0xffff) as u16).unwrap_or(Move::NONE),
                    &mut out,
                );
            }
        }
    }
}

fn assert_restored(before: &Board, after: &Board, context: &str) {
    assert_eq!(after.fen(), before.fen(), "{context}: fen");
    assert_eq!(after.hash(), before.hash(), "{context}: hash");
    assert_eq!(
        after.hash_with(EnPassantMode::Legal),
        before.hash_with(EnPassantMode::Legal),
        "{context}: legal hash"
    );
    assert_eq!(
        after.side_to_move(),
        before.side_to_move(),
        "{context}: side"
    );
    assert_eq!(
        after.castling_rights(),
        before.castling_rights(),
        "{context}: castling"
    );
    assert_eq!(after.ep_square(), before.ep_square(), "{context}: ep");
    assert_eq!(after.rule50(), before.rule50(), "{context}: rule50");
    assert_eq!(after.game_ply(), before.game_ply(), "{context}: game ply");
    assert_eq!(
        after.fullmove_number(),
        before.fullmove_number(),
        "{context}: fullmove"
    );
    let before_pieces: Vec<_> = before.pieces().collect();
    let after_pieces: Vec<_> = after.pieces().collect();
    assert_eq!(after_pieces, before_pieces, "{context}: pieces");
}

fn check_do_undo(board: &Board) {
    let before = board.clone();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    let mut moves = MoveList::new();
    generate_legal_with_state(board, &state, &mut moves);
    for &m in moves.as_slice() {
        let mut child = board.clone();
        let mut child_state = state;
        child.do_move(m, &mut child_state);
        child.undo_move(m, &child_state);
        assert_restored(&before, &child, &format!("{} / {m}", before.fen()));
    }
}

#[test]
fn do_undo_restores_every_observable_field() {
    for board in robustness_boards() {
        check_do_undo(&board);
    }
    common::walk_positions(common::PLAYOUT_FENS, 4, 40, &mut |board, _| {
        check_do_undo(board);
    });
}

#[test]
fn do_undo_boundary_clocks_restore_exactly() {
    for fen in [
        "4k3/8/8/8/8/8/8/4K2R w - - 65535 32768",
        "4k3/8/8/8/8/8/8/R3K3 b - - 65535 65535",
        "4k3/8/8/8/8/8/8/4K3 w - - 0 65535",
        "4k3/8/8/8/8/8/8/4K3 b - - 0 65535",
    ] {
        let board = Board::from_fen(fen).expect("boundary FEN must parse");
        check_do_undo(&board);
    }
}

#[test]
fn boundary_fixtures_cover_both_colors() {
    // The boundary boards above are both colors; this pins the enumeration so
    // a future fixture edit cannot quietly drop a side.
    let sides: Vec<Color> = [
        "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
        "4k3/8/8/8/8/8/8/4K3 b - - 0 1",
    ]
    .into_iter()
    .map(|fen| Board::from_fen(fen).unwrap().side_to_move())
    .collect();
    assert_eq!(sides, vec![Color::White, Color::Black]);
}
