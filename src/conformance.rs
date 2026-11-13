//! Frozen conformance corpus for downstream differential testing.
//!
//! The corpus is inert data: each [`ConformanceCase`] records a FEN, the
//! pseudo-legal and legal moves in generation order, the WDL outcome and
//! terminal reason, the literal terminal flag, both en-passant key modes, and
//! the clocks. The crate's own test suite verifies every field against the
//! engine, so the published data cannot drift silently.
//!
//! The expected values were generated from this crate's engine, whose move
//! generation and perft values are verified against Fairy-Stockfish. No
//! GPL-derived data is included. Treat any difference not announced as a rules
//! fix as a veto in a downstream differential.
//!
//! [`CHILD_CASES`] is the child-state counterpart: a frozen table of
//! (position, move, expected [`AfterState`], child legal move count) rows
//! covering every branch of the two-level child answer, so a downstream can
//! veto a child-terminality disagreement without trusting the implementation.
//!
//! ```
//! use atomic_movegen::conformance::CASES;
//!
//! for case in CASES {
//!     assert!(!case.name.is_empty());
//!     assert!(case.legal.len() <= case.pseudo_legal.len());
//! }
//! ```

use crate::effects::AfterState;
use crate::types::{
    Outcome,
    Outcome::{Draw, Loss, Win},
    Terminal, TerminalReason,
    TerminalReason::{Extinction, FiftyMoveRule, MaterialDraw, NoMovesInCheck, Stalemate},
};

/// One frozen conformance case.
///
/// The fields are public data. The corpus is intentionally small and curated;
/// it is not a perft tree. Moves are UCI strings in generation order.
#[derive(Debug, Clone, Copy)]
pub struct ConformanceCase {
    /// Short human-readable name, unique within [`CASES`].
    pub name: &'static str,
    /// Position FEN, including clocks.
    pub fen: &'static str,
    /// Pseudo-legal moves in generation order, as UCI strings.
    pub pseudo_legal: &'static [&'static str],
    /// Legal moves in generation order, as UCI strings.
    pub legal: &'static [&'static str],
    /// WDL outcome from the side-to-move perspective, if decided.
    pub outcome: Option<Outcome>,
    /// Terminal reason mirroring [`Board::terminal`](crate::Board::terminal), if decided.
    pub terminal_reason: Option<TerminalReason>,
    /// Literal [`Board::is_terminal`](crate::Board::is_terminal).
    pub is_terminal: bool,
    /// [`Board::hash`](crate::Board::hash) / `hash_with(EnPassantMode::Always)`.
    pub hash_always: u64,
    /// [`Board::hash_with`](crate::Board::hash_with) with `EnPassantMode::Legal`.
    pub hash_legal: u64,
    /// Half-move clock of the position.
    pub rule50: u16,
    /// Raw game ply of the position.
    pub game_ply: u16,
}

/// The frozen conformance corpus.
///
/// The cases cover the start position, quiet and capture-heavy middlegames,
/// legal and non-capturable en passant, castling (including an attacked path),
/// promotions, touching commoners, checks, mate, stalemate, extinction,
/// material and 50-move draws, mate on the clock, pins, blast wins,
/// self-explosions, the largest observed legal move count, and all 14
/// `tests/perft_tests.rs` positions.
pub static CASES: &[ConformanceCase] = &[
    // Start position (perft position 1).
    ConformanceCase {
        name: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        pseudo_legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
        ],
        legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x5ab84d1cfc77f79e,
        hash_legal: 0x5ab84d1cfc77f79e,
        rule50: 0,
        game_ply: 1,
    },
    // Quiet middlegame (move_order_golden `middlegame`).
    ConformanceCase {
        name: "quiet-middlegame",
        fen: "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
        pseudo_legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "d2d3", "d2d4", "g2g3", "g2g4", "h2h3", "h2h4",
            "b1a3", "b1c3", "g1e2", "g1h3", "c4f1", "c4e2", "c4b3", "c4d3", "c4b5", "c4d5", "c4a6",
            "c4e6", "c4f7", "f3d1", "f3e2", "f3a3", "f3b3", "f3c3", "f3d3", "f3e3", "f3g3", "f3h3",
            "f3f4", "f3g4", "f3f5", "f3h5", "f3f6", "f3f7", "e1d1", "e1f1", "e1e2",
        ],
        legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "d2d3", "d2d4", "g2g3", "g2g4", "h2h3", "h2h4",
            "b1a3", "b1c3", "g1e2", "g1h3", "c4f1", "c4e2", "c4b3", "c4d3", "c4b5", "c4d5", "c4a6",
            "c4e6", "c4f7", "f3d1", "f3e2", "f3a3", "f3b3", "f3c3", "f3d3", "f3e3", "f3g3", "f3h3",
            "f3f4", "f3g4", "f3f5", "f3h5", "f3f6", "f3f7", "e1d1", "e1f1", "e1e2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x6b45728e09410b1a,
        hash_legal: 0x6b45728e09410b1a,
        rule50: 4,
        game_ply: 7,
    },
    // Capture available after 1.e4 d5 (move_order_golden `capture_heavy`).
    ConformanceCase {
        name: "capture-middlegame",
        fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
        pseudo_legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "f2f3", "f2f4", "g2g3",
            "g2g4", "h2h3", "h2h4", "e4e5", "e4d5", "b1a3", "b1c3", "g1e2", "g1f3", "g1h3", "f1e2",
            "f1d3", "f1c4", "f1b5", "f1a6", "d1e2", "d1f3", "d1g4", "d1h5", "e1e2",
        ],
        legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "f2f3", "f2f4", "g2g3",
            "g2g4", "h2h3", "h2h4", "e4e5", "e4d5", "b1a3", "b1c3", "g1e2", "g1f3", "g1h3", "f1e2",
            "f1d3", "f1c4", "f1b5", "f1a6", "d1e2", "d1f3", "d1g4", "d1h5", "e1e2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x4c47301aad9e4ede,
        hash_legal: 0x01965184bb131d9a,
        rule50: 0,
        game_ply: 3,
    },
    // Capture-heavy middlegame (perft position 33).
    ConformanceCase {
        name: "capture-heavy-middlegame",
        fen: "r4rk1/1p4pp/p1p1b3/5p2/PbB2P2/BPN1P1PP/3p1n2/R4RK1 b - - 3 18",
        pseudo_legal: &[
            "d2d1q", "d2d1r", "d2d1b", "d2d1n", "a6a5", "c6c5", "b7b6", "b7b5", "g7g6", "g7g5",
            "h7h6", "h7h5", "f2d1", "f2h1", "f2d3", "f2h3", "f2e4", "f2g4", "b4a3", "b4c3", "b4a5",
            "b4c5", "b4d6", "b4e7", "e6c4", "e6d5", "e6d7", "e6f7", "e6c8", "a8a7", "a8b8", "a8c8",
            "a8d8", "a8e8", "f8f6", "f8f7", "f8b8", "f8c8", "f8d8", "f8e8", "g8f7", "g8h8",
        ],
        legal: &[
            "d2d1q", "d2d1r", "d2d1b", "d2d1n", "a6a5", "c6c5", "b7b6", "b7b5", "g7g6", "g7g5",
            "h7h6", "h7h5", "f2d1", "f2h1", "f2d3", "f2h3", "f2e4", "f2g4", "b4a3", "b4c3", "b4a5",
            "b4c5", "b4d6", "b4e7", "e6c4", "e6d5", "e6f7", "a8a7", "a8b8", "a8c8", "a8d8", "a8e8",
            "f8f6", "f8f7", "f8b8", "f8c8", "f8d8", "f8e8", "g8f7", "g8h8",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x9500611af48c080f,
        hash_legal: 0x9500611af48c080f,
        rule50: 3,
        game_ply: 36,
    },
    // Legal en-passant capture (move_order_golden `en_passant_white`).
    ConformanceCase {
        name: "en-passant-legal",
        fen: "8/8/8/1pP5/8/8/1k6/4K3 w - b6 0 1",
        pseudo_legal: &["c5c6", "c5b6", "e1d1", "e1f1", "e1d2", "e1e2", "e1f2"],
        legal: &["c5c6", "c5b6", "e1d1", "e1f1", "e1d2", "e1e2", "e1f2"],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x13641c2390b75641,
        hash_legal: 0x13641c2390b75641,
        rule50: 0,
        game_ply: 1,
    },
    // Stored ep target with no legal ep capture; `hash_with(Legal)` canonicalizes it away.
    ConformanceCase {
        name: "en-passant-non-capturable",
        fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        pseudo_legal: &[
            "a7a6", "a7a5", "b7b6", "b7b5", "c7c6", "c7c5", "d7d6", "d7d5", "e7e6", "e7e5", "f7f6",
            "f7f5", "g7g6", "g7g5", "h7h6", "h7h5", "b8a6", "b8c6", "g8f6", "g8h6",
        ],
        legal: &[
            "a7a6", "a7a5", "b7b6", "b7b5", "c7c6", "c7c5", "d7d6", "d7d5", "e7e6", "e7e5", "f7f6",
            "f7f5", "g7g6", "g7g5", "h7h6", "h7h5", "b8a6", "b8c6", "g8f6", "g8h6",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x7174e524cfc7e991,
        hash_legal: 0x8e583d343b49affb,
        rule50: 0,
        game_ply: 2,
    },
    // Both sides may castle (move_order_golden `castling`).
    ConformanceCase {
        name: "castling-both",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        pseudo_legal: &[
            "a1b1", "a1c1", "a1d1", "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "h1f1",
            "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d1", "e1f1", "e1d2",
            "e1e2", "e1f2", "e1g1", "e1c1",
        ],
        legal: &[
            "a1b1", "a1c1", "a1d1", "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "h1f1",
            "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d1", "e1f1", "e1d2",
            "e1e2", "e1f2", "e1g1", "e1c1",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x9df4882cba5e0818,
        hash_legal: 0x9df4882cba5e0818,
        rule50: 0,
        game_ply: 1,
    },
    // Both castling paths are attacked; the castling moves are pseudo-legal but illegal.
    ConformanceCase {
        name: "castling-through-check",
        fen: "r3k2r/8/8/8/8/5q2/8/R3K2R w KQkq - 0 1",
        pseudo_legal: &[
            "a1b1", "a1c1", "a1d1", "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "h1f1",
            "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d1", "e1f1", "e1d2",
            "e1e2", "e1f2", "e1g1", "e1c1",
        ],
        legal: &[
            "a1b1", "a1c1", "a1d1", "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "h1f1",
            "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x7158ea352a7edd49,
        hash_legal: 0x7158ea352a7edd49,
        rule50: 0,
        game_ply: 1,
    },
    // Promotion pushes and captures, all four pieces (move_order_golden `promotion`).
    ConformanceCase {
        name: "promotion-push-and-capture",
        fen: "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        pseudo_legal: &[
            "b7b8q", "b7b8r", "b7b8b", "b7b8n", "b7a8q", "b7a8r", "b7a8b", "b7a8n", "b7c8q",
            "b7c8r", "b7c8b", "b7c8n", "f1d2", "f1h2", "f1e3", "f1g3", "h1f2", "h1g3", "e2d1",
            "e2e1", "e2d2", "e2f2", "e2d3", "e2e3", "e2f3",
        ],
        legal: &[
            "b7b8q", "b7b8r", "b7b8b", "b7b8n", "b7a8q", "b7a8r", "b7a8b", "b7a8n", "b7c8q",
            "b7c8r", "b7c8b", "b7c8n", "f1d2", "f1h2", "f1e3", "f1g3", "h1g3", "e2d1", "e2d2",
            "e2d3", "e2e3", "e2f3",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0xebf46b7088e725bf,
        hash_legal: 0xebf46b7088e725bf,
        rule50: 0,
        game_ply: 1,
    },
    // Black promotion capture (move_order_golden `black_promotion_capture`).
    ConformanceCase {
        name: "promotion-capture-black",
        fen: "4k3/8/8/8/8/8/1p6/R1N1K3 b - - 0 1",
        pseudo_legal: &[
            "b2b1q", "b2b1r", "b2b1b", "b2b1n", "b2a1q", "b2a1r", "b2a1b", "b2a1n", "b2c1q",
            "b2c1r", "b2c1b", "b2c1n", "e8d7", "e8e7", "e8f7", "e8d8", "e8f8",
        ],
        legal: &[
            "b2b1q", "b2b1r", "b2b1b", "b2b1n", "b2a1q", "b2a1r", "b2a1b", "b2a1n", "b2c1q",
            "b2c1r", "b2c1b", "b2c1n", "e8d7", "e8e7", "e8f7", "e8d8", "e8f8",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x2162a7b1fca0cb1f,
        hash_legal: 0x2162a7b1fca0cb1f,
        rule50: 0,
        game_ply: 2,
    },
    // Touching commoners: the enemy commoner is immune to capture.
    ConformanceCase {
        name: "touching-commoners",
        fen: "8/8/8/8/3kK2r/8/7b/8 w - - 0 1",
        pseudo_legal: &[
            "e4d3", "e4e3", "e4f3", "e4d4", "e4f4", "e4d5", "e4e5", "e4f5",
        ],
        legal: &["e4d3", "e4e3", "e4f3", "e4d5", "e4e5", "e4f5"],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0xf4d0fe7e6cc3cb4b,
        hash_legal: 0xf4d0fe7e6cc3cb4b,
        rule50: 0,
        game_ply: 1,
    },
    // Side to move is in check with exactly one legal escape.
    ConformanceCase {
        name: "check-single-escape",
        fen: "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
        pseudo_legal: &["h8g7", "h8h7", "h8g8"],
        legal: &["h8g7"],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x4b68721c30290905,
        hash_legal: 0x4b68721c30290905,
        rule50: 0,
        game_ply: 2,
    },
    // Checkmate: no legal moves while attacked (move_order_golden `checkmate`).
    ConformanceCase {
        name: "checkmate",
        fen: "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        pseudo_legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "h2h3",
            "f3f4", "g4g5", "b1a3", "b1c3", "g1h3", "f1g2", "f1h3", "e1f2",
        ],
        legal: &[],
        outcome: Some(Loss),
        terminal_reason: Some(NoMovesInCheck),
        is_terminal: true,
        hash_always: 0x2c9bf8f51dc15756,
        hash_legal: 0x2c9bf8f51dc15756,
        rule50: 1,
        game_ply: 5,
    },
    // Stalemate: no legal moves while safe.
    ConformanceCase {
        name: "stalemate",
        fen: "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
        pseudo_legal: &["a1b1", "a1a2", "a1b2"],
        legal: &[],
        outcome: Some(Draw),
        terminal_reason: Some(Stalemate),
        is_terminal: true,
        hash_always: 0x45e3ca210ab88241,
        hash_legal: 0x45e3ca210ab88241,
        rule50: 0,
        game_ply: 1,
    },
    // Side to move has no commoner (extinction loss).
    ConformanceCase {
        name: "extinction-side-to-move",
        fen: "8/8/8/8/8/8/8/k7 w - - 0 1",
        pseudo_legal: &[],
        legal: &[],
        outcome: Some(Loss),
        terminal_reason: Some(Extinction),
        is_terminal: true,
        hash_always: 0xbde7aeb915ad0e40,
        hash_legal: 0xbde7aeb915ad0e40,
        rule50: 0,
        game_ply: 1,
    },
    // Opponent has no commoner (extinction win).
    ConformanceCase {
        name: "extinction-opponent",
        fen: "8/8/8/8/8/8/8/4K3 w - - 0 1",
        pseudo_legal: &["e1d1", "e1f1", "e1d2", "e1e2", "e1f2"],
        legal: &[],
        outcome: Some(Win),
        terminal_reason: Some(Extinction),
        is_terminal: true,
        hash_always: 0x7fc558877bd94945,
        hash_legal: 0x7fc558877bd94945,
        rule50: 0,
        game_ply: 1,
    },
    // Only the two commoners remain (WDL material draw, not literal game over).
    ConformanceCase {
        name: "material-draw-k-vs-k",
        fen: "k7/8/8/8/8/8/8/7K w - - 0 1",
        pseudo_legal: &["h1g1", "h1g2", "h1h2"],
        legal: &["h1g1", "h1g2", "h1h2"],
        outcome: Some(Draw),
        terminal_reason: Some(MaterialDraw),
        is_terminal: false,
        hash_always: 0x93ce83108740d7f0,
        hash_legal: 0x93ce83108740d7f0,
        rule50: 0,
        game_ply: 1,
    },
    // 50-move draw with legal moves still available.
    ConformanceCase {
        name: "fifty-move-draw",
        fen: "4k3/8/8/8/8/8/8/4K2R w - - 100 1",
        pseudo_legal: &[
            "h1f1", "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d1", "e1f1",
            "e1d2", "e1e2", "e1f2",
        ],
        legal: &[
            "h1f1", "h1g1", "h1h2", "h1h3", "h1h4", "h1h5", "h1h6", "h1h7", "h1h8", "e1d1", "e1f1",
            "e1d2", "e1e2", "e1f2",
        ],
        outcome: Some(Draw),
        terminal_reason: Some(FiftyMoveRule),
        is_terminal: true,
        hash_always: 0xa784399e23111abd,
        hash_legal: 0xa784399e23111abd,
        rule50: 100,
        game_ply: 1,
    },
    // Checkmate on the 100th half-move: mate outranks the clock.
    ConformanceCase {
        name: "checkmate-on-clock",
        fen: "7K/8/8/8/8/8/1Q6/k7 b - - 100 1",
        pseudo_legal: &["a1b1", "a1a2", "a1b2"],
        legal: &[],
        outcome: Some(Loss),
        terminal_reason: Some(NoMovesInCheck),
        is_terminal: true,
        hash_always: 0x4778b47716ffbdad,
        hash_legal: 0x4778b47716ffbdad,
        rule50: 100,
        game_ply: 2,
    },
    // A pinned knight cannot move; only commoner moves are legal.
    ConformanceCase {
        name: "pinned-piece",
        fen: "4k3/8/8/8/4n3/8/8/4RK2 b - - 0 1",
        pseudo_legal: &[
            "e4d2", "e4f2", "e4c3", "e4g3", "e4c5", "e4g5", "e4d6", "e4f6", "e8d7", "e8e7", "e8f7",
            "e8d8", "e8f8",
        ],
        legal: &["e8d7", "e8e7", "e8f7", "e8d8", "e8f8"],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x138689cd9939b54a,
        hash_legal: 0x138689cd9939b54a,
        rule50: 0,
        game_ply: 2,
    },
    // A capture whose blast removes the enemy commoner.
    ConformanceCase {
        name: "blast-removes-enemy-commoner",
        fen: "4R1k1/8/8/8/8/8/8/4K3 w - - 0 1",
        pseudo_legal: &[
            "e8e2", "e8e3", "e8e4", "e8e5", "e8e6", "e8e7", "e8a8", "e8b8", "e8c8", "e8d8", "e8f8",
            "e8g8", "e1d1", "e1f1", "e1d2", "e1e2", "e1f2",
        ],
        legal: &[
            "e8e2", "e8e3", "e8e4", "e8e5", "e8e6", "e8e7", "e8a8", "e8b8", "e8c8", "e8d8", "e8f8",
            "e8g8", "e1d1", "e1f1", "e1d2", "e1e2", "e1f2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x8cdc3442a71d957c,
        hash_legal: 0x8cdc3442a71d957c,
        rule50: 0,
        game_ply: 1,
    },
    // The commoner capture is a self-explosion and is illegal.
    ConformanceCase {
        name: "self-explosion-capture",
        fen: "4k3/8/8/8/8/8/8/3qK3 w - - 0 1",
        pseudo_legal: &["e1d1", "e1f1", "e1d2", "e1e2", "e1f2"],
        legal: &["e1f2"],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x60cf1a1b35e4a53a,
        hash_legal: 0x60cf1a1b35e4a53a,
        rule50: 0,
        game_ply: 1,
    },
    // Largest observed legal move count (68) in the perft corpus through depth 4.
    ConformanceCase {
        name: "max-legal-68",
        fen: "8/8/5q2/3N3p/1r5P/4q3/6Kk/8 b - - 25 63",
        pseudo_legal: &[
            "b4b1", "b4b2", "b4b3", "b4a4", "b4c4", "b4d4", "b4e4", "b4f4", "b4g4", "b4h4", "b4b5",
            "b4b6", "b4b7", "b4b8", "e3c1", "e3e1", "e3g1", "e3d2", "e3e2", "e3f2", "e3a3", "e3b3",
            "e3c3", "e3d3", "e3f3", "e3g3", "e3h3", "e3d4", "e3e4", "e3f4", "e3c5", "e3e5", "e3g5",
            "e3b6", "e3e6", "e3h6", "e3a7", "e3e7", "e3e8", "f6a1", "f6f1", "f6b2", "f6f2", "f6c3",
            "f6f3", "f6d4", "f6f4", "f6h4", "f6e5", "f6f5", "f6g5", "f6a6", "f6b6", "f6c6", "f6d6",
            "f6e6", "f6g6", "f6h6", "f6e7", "f6f7", "f6g7", "f6d8", "f6f8", "f6h8", "h2g1", "h2h1",
            "h2g2", "h2g3", "h2h3",
        ],
        legal: &[
            "b4b1", "b4b2", "b4b3", "b4a4", "b4c4", "b4d4", "b4e4", "b4f4", "b4g4", "b4h4", "b4b5",
            "b4b6", "b4b7", "b4b8", "e3c1", "e3e1", "e3g1", "e3d2", "e3e2", "e3f2", "e3a3", "e3b3",
            "e3c3", "e3d3", "e3f3", "e3g3", "e3h3", "e3d4", "e3e4", "e3f4", "e3c5", "e3e5", "e3g5",
            "e3b6", "e3e6", "e3h6", "e3a7", "e3e7", "e3e8", "f6a1", "f6f1", "f6b2", "f6f2", "f6c3",
            "f6f3", "f6d4", "f6f4", "f6h4", "f6e5", "f6f5", "f6g5", "f6a6", "f6b6", "f6c6", "f6d6",
            "f6e6", "f6g6", "f6h6", "f6e7", "f6f7", "f6g7", "f6d8", "f6f8", "f6h8", "h2g1", "h2h1",
            "h2g3", "h2h3",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0xac6f8f8c4f6302ac,
        hash_legal: 0xac6f8f8c4f6302ac,
        rule50: 25,
        game_ply: 126,
    },
    // Perft position 1 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-1",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        pseudo_legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
        ],
        legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4", "b1a3", "b1c3", "g1f3", "g1h3",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x5ab84d1cfc77f79e,
        hash_legal: 0x5ab84d1cfc77f79e,
        rule50: 0,
        game_ply: 1,
    },
    // Perft position 2 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-2",
        fen: "rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4",
        pseudo_legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "f2f3", "f2f4", "g2g3", "g2g4", "e4e5",
            "e4d5", "b1d2", "b1a3", "b1c3", "h3g1", "h3f4", "h3g5", "c1d2", "c1e3", "c1f4", "c1g5",
            "c1h6", "f1e2", "f1d3", "f1c4", "f1b5", "f1a6", "h1g1", "d1d2", "d1e2", "d1d3", "d1f3",
            "d1g4", "d1h5", "e1d2", "e1e2",
        ],
        legal: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "f2f3", "f2f4", "g2g3", "g2g4", "e4e5",
            "e4d5", "b1d2", "b1a3", "b1c3", "h3g1", "h3f4", "h3g5", "c1d2", "c1e3", "c1f4", "c1g5",
            "c1h6", "f1e2", "f1d3", "f1c4", "f1b5", "f1a6", "h1g1", "d1d2", "d1e2", "d1d3", "d1f3",
            "d1g4", "d1h5", "e1d2", "e1e2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x83afe1582698c89b,
        hash_legal: 0x83afe1582698c89b,
        rule50: 0,
        game_ply: 7,
    },
    // Perft position 3 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-3",
        fen: "rnb1kb1r/p5p1/2pNp2p/1p1q1p2/3P1Pn1/N5P1/PPP4P/R1BQKB1R b KQkq - 6 10",
        pseudo_legal: &[
            "b5b4", "c6c5", "e6e5", "h6h5", "a7a6", "a7a5", "g7g6", "g7g5", "g4f2", "g4h2", "g4e3",
            "g4e5", "g4f6", "b8a6", "b8d7", "c8a6", "c8b7", "c8d7", "f8d6", "f8e7", "h8h7", "h8g8",
            "d5h1", "d5a2", "d5g2", "d5b3", "d5f3", "d5c4", "d5d4", "d5e4", "d5c5", "d5e5", "d5d6",
            "e8d7", "e8e7", "e8f7", "e8d8",
        ],
        legal: &["f8d6", "d5d6", "e8d7", "e8e7", "e8d8"],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x7ea2bcc4da30aec2,
        hash_legal: 0x7ea2bcc4da30aec2,
        rule50: 6,
        game_ply: 20,
    },
    // Perft position 4 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-4",
        fen: "r4b1r/p1N1k1p1/2pNp2p/5p2/1n1P1P2/6PP/PP6/R1B1K3 b Q - 4 17",
        pseudo_legal: &[
            "c6c5", "e6e5", "h6h5", "a7a6", "a7a5", "g7g6", "g7g5", "b4a2", "b4c2", "b4d3", "b4d5",
            "b4a6", "a8b8", "a8c8", "a8d8", "a8e8", "h8h7", "h8g8", "e7d6", "e7f6", "e7d7", "e7f7",
            "e7d8", "e7e8",
        ],
        legal: &[
            "c6c5", "e6e5", "h6h5", "a7a6", "a7a5", "g7g6", "g7g5", "b4a2", "b4c2", "b4d3", "b4d5",
            "b4a6", "a8b8", "a8c8", "a8d8", "a8e8", "h8h7", "h8g8", "e7f6", "e7d7", "e7d8",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x6e4ca2473a3c51d0,
        hash_legal: 0x6e4ca2473a3c51d0,
        rule50: 4,
        game_ply: 34,
    },
    // Perft position 5 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-5",
        fen: "r1k4r/p4Np1/N1p1p2p/5p2/3P1P2/6PP/8/5K2 w - - 0 22",
        pseudo_legal: &[
            "g3g4", "h3h4", "d4d5", "a6b4", "a6c5", "a6c7", "a6b8", "f7e5", "f7g5", "f7d6", "f7h6",
            "f7d8", "f7h8", "f1e1", "f1g1", "f1e2", "f1f2", "f1g2",
        ],
        legal: &[
            "g3g4", "h3h4", "d4d5", "a6b4", "a6c5", "a6c7", "a6b8", "f7e5", "f7g5", "f7d6", "f7h6",
            "f7d8", "f7h8", "f1e1", "f1g1", "f1e2", "f1f2", "f1g2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x87e2f99a42d04a04,
        hash_legal: 0x87e2f99a42d04a04,
        rule50: 0,
        game_ply: 43,
    },
    // Perft position 6 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-6",
        fen: "r1k1r3/p5p1/N1p4p/2N2p2/5P2/6PP/8/5K2 w - - 2 25",
        pseudo_legal: &[
            "g3g4", "h3h4", "c5b3", "c5d3", "c5a4", "c5e4", "c5e6", "c5b7", "c5d7", "a6b4", "a6c7",
            "a6b8", "f1e1", "f1g1", "f1e2", "f1f2", "f1g2",
        ],
        legal: &[
            "g3g4", "h3h4", "c5b3", "c5d3", "c5a4", "c5e4", "c5e6", "c5b7", "c5d7", "a6b4", "a6c7",
            "a6b8", "f1g1", "f1f2", "f1g2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0xfff21f1656f9218d,
        hash_legal: 0xfff21f1656f9218d,
        rule50: 2,
        game_ply: 49,
    },
    // Perft position 7 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-7",
        fen: "r1k5/p7/N1p5/2N4p/7P/8/5r2/4K3 w - - 0 31",
        pseudo_legal: &[
            "c5b3", "c5d3", "c5a4", "c5e4", "c5e6", "c5b7", "c5d7", "a6b4", "a6c7", "a6b8", "e1d1",
            "e1f1", "e1d2", "e1e2", "e1f2",
        ],
        legal: &[
            "c5b3", "c5d3", "c5a4", "c5e4", "c5e6", "c5b7", "c5d7", "a6b4", "a6c7", "a6b8", "e1d1",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x529b346d2c347350,
        hash_legal: 0x529b346d2c347350,
        rule50: 0,
        game_ply: 61,
    },
    // Perft position 8 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-8",
        fen: "r2k4/p7/N1p5/7p/7P/8/2K5/8 w - - 2 34",
        pseudo_legal: &[
            "a6b4", "a6c5", "a6c7", "a6b8", "c2b1", "c2c1", "c2d1", "c2b2", "c2d2", "c2b3", "c2c3",
            "c2d3",
        ],
        legal: &[
            "a6b4", "a6c5", "a6c7", "a6b8", "c2b1", "c2c1", "c2d1", "c2b2", "c2d2", "c2b3", "c2c3",
            "c2d3",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x71db99896d772c0a,
        hash_legal: 0x71db99896d772c0a,
        rule50: 2,
        game_ply: 67,
    },
    // Perft position 9 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-9",
        fen: "3r4/8/8/5k1p/4KN1P/p1p5/8/8 w - - 6 44",
        pseudo_legal: &[
            "f4e2", "f4g2", "f4d3", "f4h3", "f4d5", "f4h5", "f4e6", "f4g6", "e4d3", "e4e3", "e4f3",
            "e4d4", "e4d5", "e4e5", "e4f5",
        ],
        legal: &[
            "f4e2", "f4g2", "f4d3", "f4h3", "f4d5", "f4h5", "f4e6", "f4g6", "e4e3", "e4f3", "e4e5",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x108efe6a1d59f215,
        hash_legal: 0x108efe6a1d59f215,
        rule50: 6,
        game_ply: 87,
    },
    // Perft position 10 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-10",
        fen: "5r2/8/4k1N1/4K2p/7P/p7/8/2q5 w - - 0 48",
        pseudo_legal: &[
            "g6f4", "g6e7", "g6f8", "g6h8", "e5d4", "e5e4", "e5f4", "e5d5", "e5f5", "e5d6", "e5e6",
            "e5f6",
        ],
        legal: &[
            "g6f4", "g6e7", "g6f8", "g6h8", "e5d4", "e5e4", "e5d5", "e5f5", "e5d6", "e5f6",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x8d5edafb754623b7,
        hash_legal: 0x8d5edafb754623b7,
        rule50: 0,
        game_ply: 95,
    },
    // Perft position 11 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-11",
        fen: "8/4N3/8/4K2p/1r3qkP/8/8/q7 w - - 16 59",
        pseudo_legal: &[
            "e7d5", "e7f5", "e7c6", "e7g6", "e7c8", "e7g8", "e5d4", "e5e4", "e5f4", "e5d5", "e5f5",
            "e5d6", "e5e6", "e5f6",
        ],
        legal: &["e5d5", "e5f5", "e5e6"],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x84eb8f5521149355,
        hash_legal: 0x84eb8f5521149355,
        rule50: 16,
        game_ply: 117,
    },
    // Perft position 12 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-12",
        fen: "8/4N3/8/7p/1r3q1P/6K1/7k/q7 b - - 21 61",
        pseudo_legal: &[
            "b4b1", "b4b2", "b4b3", "b4a4", "b4c4", "b4d4", "b4e4", "b4b5", "b4b6", "b4b7", "b4b8",
            "a1b1", "a1c1", "a1d1", "a1e1", "a1f1", "a1g1", "a1h1", "a1a2", "a1b2", "a1a3", "a1c3",
            "a1a4", "a1d4", "a1a5", "a1e5", "a1a6", "a1f6", "a1a7", "a1g7", "a1a8", "a1h8", "f4c1",
            "f4f1", "f4d2", "f4f2", "f4e3", "f4f3", "f4g3", "f4c4", "f4d4", "f4e4", "f4g4", "f4h4",
            "f4e5", "f4f5", "f4g5", "f4d6", "f4f6", "f4h6", "f4c7", "f4f7", "f4b8", "f4f8", "h2g1",
            "h2h1", "h2g2", "h2g3", "h2h3",
        ],
        legal: &[
            "b4b1", "b4b2", "b4b3", "b4a4", "b4c4", "b4d4", "b4e4", "b4b5", "b4b6", "b4b7", "b4b8",
            "a1b1", "a1c1", "a1d1", "a1e1", "a1f1", "a1g1", "a1h1", "a1a2", "a1b2", "a1a3", "a1c3",
            "a1a4", "a1d4", "a1a5", "a1e5", "a1a6", "a1f6", "a1a7", "a1g7", "a1a8", "a1h8", "f4c1",
            "f4f1", "f4d2", "f4f2", "f4e3", "f4f3", "f4c4", "f4d4", "f4e4", "f4g4", "f4h4", "f4e5",
            "f4f5", "f4g5", "f4d6", "f4f6", "f4h6", "f4c7", "f4f7", "f4b8", "f4f8", "h2g1", "h2h1",
            "h2g2", "h2h3",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x84a21a282b1d7cb3,
        hash_legal: 0x84a21a282b1d7cb3,
        rule50: 21,
        game_ply: 122,
    },
    // Perft position 13 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-13",
        fen: "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1",
        pseudo_legal: &[
            "a6a5", "f6f5", "g6g5", "b7b6", "b7b5", "c7c6", "c7c5", "e7e6", "e7e5", "h7h6", "h7h5",
            "b8c6", "b8d7", "g4f3", "g4h3", "g4f5", "g4h5", "g4e6", "g4d7", "g4c8", "f8h6", "f8g7",
            "a8a7", "h8g8", "d6h2", "d6a3", "d6g3", "d6b4", "d6f4", "d6c5", "d6d5", "d6e5", "d6b6",
            "d6c6", "d6e6", "d6d7", "d6d8", "e8d7", "e8f7", "e8d8",
        ],
        legal: &[
            "a6a5", "f6f5", "g6g5", "b7b6", "b7b5", "c7c6", "c7c5", "e7e6", "e7e5", "h7h6", "h7h5",
            "b8c6", "b8d7", "g4f3", "g4h3", "g4f5", "g4h5", "g4e6", "g4d7", "g4c8", "f8h6", "f8g7",
            "a8a7", "h8g8", "d6h2", "d6a3", "d6g3", "d6b4", "d6f4", "d6c5", "d6d5", "d6e5", "d6b6",
            "d6c6", "d6e6", "d6d7", "d6d8", "e8d7", "e8f7", "e8d8",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0x4a4f6b22eb068fc2,
        hash_legal: 0x4a4f6b22eb068fc2,
        rule50: 0,
        game_ply: 2,
    },
    // Perft position 14 from tests/perft_tests.rs.
    ConformanceCase {
        name: "perft-14",
        fen: "rn1qkb1r/p5pp/2p5/3p4/N3P3/5P2/PPP4P/R1BQK3 w Qkq - 0 1",
        pseudo_legal: &[
            "a2a3", "b2b3", "b2b4", "c2c3", "c2c4", "h2h3", "h2h4", "f3f4", "e4e5", "e4d5", "a4c3",
            "a4c5", "a4b6", "c1d2", "c1e3", "c1f4", "c1g5", "c1h6", "a1b1", "d1d2", "d1e2", "d1d3",
            "d1d4", "d1d5", "e1f1", "e1d2", "e1e2", "e1f2",
        ],
        legal: &[
            "a2a3", "b2b3", "b2b4", "c2c3", "c2c4", "h2h3", "h2h4", "f3f4", "e4e5", "e4d5", "a4c3",
            "a4c5", "a4b6", "c1d2", "c1e3", "c1f4", "c1g5", "c1h6", "a1b1", "d1d2", "d1e2", "d1d3",
            "d1d4", "d1d5", "e1f1", "e1d2", "e1e2", "e1f2",
        ],
        outcome: None,
        terminal_reason: None,
        is_terminal: false,
        hash_always: 0xd8cf3754c6e90b44,
        hash_legal: 0xd8cf3754c6e90b44,
        rule50: 0,
        game_ply: 1,
    },
];

/// One frozen child-state case: a position, one candidate move, and the
/// two-level answer the engine gives for the child after that move.
///
/// The fields are public data. `after_state` is the child's state from the
/// **child's** side to move, exactly as
/// [`EffectContext::child_outcome`](crate::EffectContext::child_outcome)
/// reports it: an extinction capture by the mover is therefore a
/// `Terminal(Loss/Extinction)` here, because the child is lost for the side
/// that has to move in it.
#[derive(Debug, Clone, Copy)]
pub struct ChildCase {
    /// Short human-readable name, unique within [`CHILD_CASES`].
    pub name: &'static str,
    /// Position FEN, including clocks.
    pub fen: &'static str,
    /// The candidate move in UCI. It is pseudo-legal on `fen`; castling and
    /// en-passant moves use the board-aware encoding.
    pub uci: &'static str,
    /// [`EffectContext::child_outcome`](crate::EffectContext::child_outcome)
    /// for that move.
    pub after_state: AfterState,
    /// Number of legal moves in the child position the move produces (the
    /// cloned `do_move` child), which is `0` for an unreachable or move-less
    /// child.
    pub child_legal_count: usize,
}

/// The frozen child-state corpus: one row per `AfterState` branch and per
/// [`TerminalReason`].
///
/// The rows are inert data for a downstream that wants to veto a
/// child-terminality disagreement without trusting this crate's
/// implementation: each row names the position, the move, the expected
/// two-level answer, and the child's legal move count. `tests/conformance.rs`
/// verifies every field against the engine, so the table cannot drift
/// silently.
///
/// The `Illegal` rows are pseudo-legal moves the rules reject (a self-explosion
/// and a commoner move into an attacked square), and their
/// `child_legal_count` still describes the (unreachable) child that `do_move`
/// would build.
pub static CHILD_CASES: &[ChildCase] = &[
    ChildCase {
        name: "stalemate-child",
        fen: "7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55",
        uci: "c2c7",
        after_state: AfterState::Terminal(Terminal {
            outcome: Draw,
            reason: Stalemate,
        }),
        child_legal_count: 0,
    },
    ChildCase {
        name: "extinction-child",
        fen: "k1n5/8/1K6/8/8/8/8/8 b - - 0 1",
        uci: "c8b6",
        after_state: AfterState::Terminal(Terminal {
            outcome: Loss,
            reason: Extinction,
        }),
        child_legal_count: 0,
    },
    // The mover captures the enemy commoner: the child is extinct for the
    // side that has to move there, so the mover wins.
    ChildCase {
        name: "extinction-capture-child",
        fen: "4R1k1/8/8/8/8/8/8/4K3 w - - 0 1",
        uci: "e8g8",
        after_state: AfterState::Terminal(Terminal {
            outcome: Loss,
            reason: Extinction,
        }),
        child_legal_count: 0,
    },
    // The material shortcut decides the child although moves remain.
    ChildCase {
        name: "material-draw-child",
        fen: "8/8/8/2R5/4n3/8/7K/5k2 b - - 13 69",
        uci: "e4c5",
        after_state: AfterState::Terminal(Terminal {
            outcome: Draw,
            reason: MaterialDraw,
        }),
        child_legal_count: 5,
    },
    // Mate outranks the clock: the child's clock reads 100.
    ChildCase {
        name: "checkmate-child",
        fen: "7k/8/8/5N2/3K4/8/8/R6R w - - 99 60",
        uci: "a1g1",
        after_state: AfterState::Terminal(Terminal {
            outcome: Loss,
            reason: NoMovesInCheck,
        }),
        child_legal_count: 0,
    },
    ChildCase {
        name: "fifty-move-child",
        fen: "4k3/8/8/8/8/8/8/R3K3 w - - 99 60",
        uci: "a1a8",
        after_state: AfterState::Terminal(Terminal {
            outcome: Draw,
            reason: FiftyMoveRule,
        }),
        child_legal_count: 3,
    },
    // Undecided children with a non-trivial answer: the en-passant child is in
    // check with exactly one escape, the castling child kept black's rights.
    ChildCase {
        name: "undecided-en-passant-child",
        fen: "8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41",
        uci: "g7g5",
        after_state: AfterState::Undecided,
        child_legal_count: 1,
    },
    ChildCase {
        name: "undecided-castling-rights-child",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        uci: "a1a2",
        after_state: AfterState::Undecided,
        child_legal_count: 25,
    },
    ChildCase {
        name: "undecided-promotion-child",
        fen: "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
        uci: "b7b8q",
        after_state: AfterState::Undecided,
        child_legal_count: 22,
    },
    // Illegal pseudo-legal moves: a self-explosion, and a commoner move into
    // an attacked square.
    ChildCase {
        name: "illegal-self-explosion",
        fen: "8/8/8/8/8/8/8/3qK3 w - - 0 1",
        uci: "e1d1",
        after_state: AfterState::Illegal,
        child_legal_count: 0,
    },
    ChildCase {
        name: "illegal-move-into-check",
        fen: "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
        uci: "e1d1",
        after_state: AfterState::Illegal,
        child_legal_count: 64,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_is_non_empty_and_names_are_unique() {
        assert!(!CASES.is_empty());
        for (i, case) in CASES.iter().enumerate() {
            assert!(!case.name.is_empty());
            for other in &CASES[i + 1..] {
                assert_ne!(case.name, other.name, "duplicate conformance name");
            }
        }
    }

    #[test]
    fn child_corpus_names_are_unique_and_cover_every_branch() {
        assert!(!CHILD_CASES.is_empty());
        for (i, case) in CHILD_CASES.iter().enumerate() {
            assert!(!case.name.is_empty());
            assert!(!case.fen.is_empty());
            assert!(!case.uci.is_empty());
            for other in &CHILD_CASES[i + 1..] {
                assert_ne!(case.name, other.name, "duplicate child case name");
            }
        }
        assert!(
            CHILD_CASES.iter().any(|case| case.after_state.is_illegal()),
            "child corpus lost its Illegal branch"
        );
        assert!(
            CHILD_CASES
                .iter()
                .any(|case| case.after_state.is_undecided()),
            "child corpus lost its Undecided branch"
        );
        for reason in [
            Extinction,
            NoMovesInCheck,
            Stalemate,
            MaterialDraw,
            FiftyMoveRule,
        ] {
            assert!(
                CHILD_CASES
                    .iter()
                    .any(|case| case.after_state.terminal().map(|t| t.reason) == Some(reason)),
                "child corpus lost the {reason:?} reason"
            );
        }
    }
}
