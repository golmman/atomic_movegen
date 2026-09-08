//! Order-golden tests for move generation.
//!
//! These tests pin the *exact* move sequence (same moves, same order) produced
//! by [`generate_pseudo_legal`] and [`generate_legal`] for a fixed set of
//! positions. They guard the candidate-stream refactor behind
//! `for_each_pseudo_legal` against output drift: perft checks counts, not
//! order, and consumers depend on generation order for move ordering and
//! reproducible searches.
//!
//! The goldens were generated from the pre-refactor implementation and must
//! not change unless move ordering intentionally changes.

use atomic_movegen::movegen::{generate_legal, generate_pseudo_legal};
use atomic_movegen::{Board, MoveList};

/// `(name, fen, expected pseudo-legal UCIs, expected legal UCIs)` in order.
const GOLDENS: &[(&str, &str, &[&str], &[&str])] = &[
    (
        "startpos",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
        ],
        &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
        ],
    ),
    (
        "middlegame",
        "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
        &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "d2d3", "d2d4", "g2g3", "g2g4", "h2h3", "h2h4",
            "b1a3", "b1c3", "g1e2", "g1h3", "c4f1", "c4e2", "c4b3", "c4d3", "c4b5", "c4d5", "c4a6",
            "c4e6", "c4f7", "f3d1", "f3e2", "f3a3", "f3b3", "f3c3", "f3d3", "f3e3", "f3g3", "f3h3",
            "f3f4", "f3g4", "f3f5", "f3h5", "f3f6", "f3f7", "e1d1", "e1f1", "e1e2",
        ],
        &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "d2d3", "d2d4", "g2g3", "g2g4", "h2h3", "h2h4",
            "b1a3", "b1c3", "g1e2", "g1h3", "c4f1", "c4e2", "c4b3", "c4d3", "c4b5", "c4d5", "c4a6",
            "c4e6", "c4f7", "f3d1", "f3e2", "f3a3", "f3b3", "f3c3", "f3d3", "f3e3", "f3g3", "f3h3",
            "f3f4", "f3g4", "f3f5", "f3h5", "f3f6", "f3f7", "e1d1", "e1f1", "e1e2",
        ],
    ),
    (
        "endgame",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        &[
            "e2e3", "e2e4", "g2g3", "g2g4", "b5b6", "b4b1", "b4b2", "b4b3", "b4a4", "b4c4", "b4d4",
            "b4e4", "b4f4", "a5a4", "a5a6", "a5b6",
        ],
        &[
            "e2e3", "e2e4", "g2g3", "g2g4", "b4b1", "b4b2", "b4b3", "b4a4", "b4c4", "b4d4", "b4e4",
            "b4f4", "a5a4", "a5a6",
        ],
    ),
    (
        "promotion",
        "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        &[
            "b7b8q", "b7b8r", "b7b8b", "b7b8n", "b7a8q", "b7a8r", "b7a8b", "b7a8n", "b7c8q",
            "b7c8r", "b7c8b", "b7c8n", "f1d2", "f1h2", "f1e3", "f1g3", "h1f2", "h1g3", "e2d1",
            "e2e1", "e2d2", "e2f2", "e2d3", "e2e3", "e2f3",
        ],
        &[
            "b7b8q", "b7b8r", "b7b8b", "b7b8n", "b7a8q", "b7a8r", "b7a8b", "b7a8n", "b7c8q",
            "b7c8r", "b7c8b", "b7c8n", "f1d2", "f1h2", "f1e3", "f1g3", "h1g3", "e2d1", "e2d2",
            "e2d3", "e2e3", "e2f3",
        ],
    ),
    (
        "en_passant",
        "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
        &["e4e3", "e4d3", "a4a3", "a4b3", "a4b4", "a4a5", "a4b5"],
        &["e4e3", "a4a3", "a4b3", "a4b4", "a4a5", "a4b5"],
    ),
    (
        "checkmate",
        "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "h2h3",
            "f3f4", "g4g5", "b1a3", "b1c3", "g1h3", "f1g2", "f1h3", "e1f2",
        ],
        &[],
    ),
    (
        "in_check",
        "3k4/8/8/8/8/8/8/3QK3 b - - 0 1",
        &["d8c7", "d8d7", "d8e7", "d8c8", "d8e8"],
        &["d8c7", "d8e7", "d8c8", "d8e8"],
    ),
    (
        "castling",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        &[
            "a1b1", "a1c1", "a1d1", "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "h1f1",
            "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d1", "e1f1", "e1d2",
            "e1e2", "e1f2", "e1g1", "e1c1",
        ],
        &[
            "a1b1", "a1c1", "a1d1", "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "h1f1",
            "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d1", "e1f1", "e1d2",
            "e1e2", "e1f2", "e1g1", "e1c1",
        ],
    ),
    (
        "bare_commoners",
        "k7/8/8/8/8/8/8/7K w - - 0 1",
        &["h1g1", "h1g2", "h1h2"],
        &["h1g1", "h1g2", "h1h2"],
    ),
    ("extinction", "8/8/8/8/8/8/8/k7 w - - 0 1", &[], &[]),
    (
        "black_after_e4",
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        &[
            "a7a6", "a7a5", "b7b6", "b7b5", "c7c6", "c7c5", "d7d6", "d7d5", "e7e6", "e7e5", "f7f6",
            "f7f5", "g7g6", "g7g5", "h7h6", "h7h5", "b8a6", "b8c6", "g8f6", "g8h6",
        ],
        &[
            "a7a6", "a7a5", "b7b6", "b7b5", "c7c6", "c7c5", "d7d6", "d7d5", "e7e6", "e7e5", "f7f6",
            "f7f5", "g7g6", "g7g5", "h7h6", "h7h5", "b8a6", "b8c6", "g8f6", "g8h6",
        ],
    ),
    (
        "capture_heavy",
        "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
        &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "f2f3", "f2f4", "g2g3",
            "g2g4", "h2h3", "h2h4", "e4e5", "e4d5", "b1a3", "b1c3", "g1e2", "g1f3", "g1h3", "f1e2",
            "f1d3", "f1c4", "f1b5", "f1a6", "d1e2", "d1f3", "d1g4", "d1h5", "e1e2",
        ],
        &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "f2f3", "f2f4", "g2g3",
            "g2g4", "h2h3", "h2h4", "e4e5", "e4d5", "b1a3", "b1c3", "g1e2", "g1f3", "g1h3", "f1e2",
            "f1d3", "f1c4", "f1b5", "f1a6", "d1e2", "d1f3", "d1g4", "d1h5", "e1e2",
        ],
    ),
];

#[test]
fn move_order_matches_golden() {
    for (name, fen, expected_pseudo, expected_legal) in GOLDENS {
        let board = Board::from_fen(fen).expect("golden FEN must parse");

        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        let pseudo_uci: Vec<String> = pseudo.as_slice().iter().map(|m| m.to_uci()).collect();
        assert_eq!(
            pseudo_uci,
            expected_pseudo
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            "pseudo-legal order drift for position {name:?} ({fen})"
        );

        let mut legal = MoveList::new();
        generate_legal(&board, &mut legal);
        let legal_uci: Vec<String> = legal.as_slice().iter().map(|m| m.to_uci()).collect();
        assert_eq!(
            legal_uci,
            expected_legal
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            "legal order drift for position {name:?} ({fen})"
        );
    }
}
