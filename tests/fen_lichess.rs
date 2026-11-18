//! Frozen Lichess/`parity` FEN acceptance fixtures.
//!
//! The dispositions and expected boards in this file were produced with an
//! external shakmaty 0.30.2 oracle (`Atomic::from_setup`, recovering
//! `INVALID_CASTLING_RIGHTS`, `INVALID_EP_SQUARE`, and `TOO_MUCH_MATERIAL`)
//! and this crate's post-sanitization board. The data is committed as plain
//! tables; no shakmaty code or dependency is used at build or test time.

use atomic_movegen::board::FenError;
use atomic_movegen::movegen;
use atomic_movegen::{
    Board, Color, EnPassantMode, FenLoadError, FenPolicy, MoveList, Square, ValidationErrors,
};

struct Accepted {
    name: &'static str,
    fen: &'static str,
    expected_fen: &'static str,
    legal_fen: &'static str,
    hash_always: u64,
    hash_legal: u64,
    legal_ep: Option<Square>,
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u16,
    game_ply: u16,
    side_to_move: Color,
    legal_moves: &'static [&'static str],
}

const ACCEPTED: &[Accepted] = &[
    Accepted {
        name: "start position",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        expected_fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        legal_fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        hash_always: 6537059646018418590,
        hash_legal: 6537059646018418590,
        legal_ep: None,
        castling_rights: 15,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "a2a3", "a2a4", "b1a3", "b1c3", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3",
            "e2e4", "f2f3", "f2f4", "g1f3", "g1h3", "g2g3", "g2g4", "h2h3", "h2h4",
        ], // 20 moves
    },
    Accepted {
        name: "quiet middlegame (perft corpus 2)",
        fen: "rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4",
        expected_fen: "rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4",
        legal_fen: "rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4",
        hash_always: 9489050708613843099,
        hash_legal: 9489050708613843099,
        legal_ep: None,
        castling_rights: 15,
        ep_square: None,
        rule50: 0,
        game_ply: 7,
        side_to_move: Color::White,
        legal_moves: &[
            "a2a3", "a2a4", "b1a3", "b1c3", "b1d2", "b2b3", "b2b4", "c1d2", "c1e3", "c1f4", "c1g5",
            "c1h6", "c2c3", "c2c4", "d1d2", "d1d3", "d1e2", "d1f3", "d1g4", "d1h5", "e1d2", "e1e2",
            "e4d5", "e4e5", "f1a6", "f1b5", "f1c4", "f1d3", "f1e2", "f2f3", "f2f4", "g2g3", "g2g4",
            "h1g1", "h3f4", "h3g1", "h3g5",
        ], // 37 moves
    },
    Accepted {
        name: "legal ep capture",
        fen: "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
        expected_fen: "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
        legal_fen: "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
        hash_always: 2958377731689463399,
        hash_legal: 2958377731689463399,
        legal_ep: Some(Square::D6),
        castling_rights: 0,
        ep_square: Some(Square::D6),
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &["e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "e5d6", "e5e6"], // 7 moves
    },
    Accepted {
        name: "valid castling both sides",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        expected_fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        legal_fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        hash_always: 11381871883958749208,
        hash_legal: 11381871883958749208,
        legal_ep: None,
        castling_rights: 15,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "a1b1", "a1c1", "a1d1", "e1c1",
            "e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "e1g1", "h1f1", "h1g1", "h1h2", "h1h3", "h1h4",
            "h1h5", "h1h6", "h1h7", "h1h8",
        ], // 26 moves
    },
    Accepted {
        name: "mixed white castling (K valid, Q and black invalid)",
        fen: "4k3/8/8/8/8/8/8/4K2R w KQkq - 0 1",
        expected_fen: "4k3/8/8/8/8/8/8/4K2R w K - 0 1",
        legal_fen: "4k3/8/8/8/8/8/8/4K2R w K - 0 1",
        hash_always: 7636566133860783598,
        hash_legal: 7636566133860783598,
        legal_ep: None,
        castling_rights: 1,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "e1g1", "h1f1", "h1g1", "h1h2", "h1h3", "h1h4",
            "h1h5", "h1h6", "h1h7", "h1h8",
        ], // 15 moves
    },
    Accepted {
        name: "white Q right without the a1 rook is dropped",
        fen: "4k3/8/8/8/8/8/8/4K2R w Q - 0 1",
        expected_fen: "4k3/8/8/8/8/8/8/4K2R w - - 0 1",
        legal_fen: "4k3/8/8/8/8/8/8/4K2R w - - 0 1",
        hash_always: 12070836252615711421,
        hash_legal: 12070836252615711421,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "h1f1", "h1g1", "h1h2", "h1h3", "h1h4", "h1h5",
            "h1h6", "h1h7", "h1h8",
        ], // 14 moves
    },
    Accepted {
        name: "white K right without the h1 rook is dropped",
        fen: "4k3/8/8/8/8/8/8/R3K3 w K - 0 1",
        expected_fen: "4k3/8/8/8/8/8/8/R3K3 w - - 0 1",
        legal_fen: "4k3/8/8/8/8/8/8/R3K3 w - - 0 1",
        hash_always: 14748369001686273208,
        hash_legal: 14748369001686273208,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "a1b1", "a1c1", "a1d1", "e1d1",
            "e1d2", "e1e2", "e1f1", "e1f2",
        ], // 15 moves
    },
    Accepted {
        name: "white Q right with the a1 rook is kept",
        fen: "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
        expected_fen: "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
        legal_fen: "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
        hash_always: 3895212227895636575,
        hash_legal: 3895212227895636575,
        legal_ep: None,
        castling_rights: 2,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "a1b1", "a1c1", "a1d1", "e1c1",
            "e1d1", "e1d2", "e1e2", "e1f1", "e1f2",
        ], // 16 moves
    },
    Accepted {
        name: "black mix (k valid, q invalid)",
        fen: "4k2r/8/8/8/8/8/8/4K3 w kq - 0 1",
        expected_fen: "4k2r/8/8/8/8/8/8/4K3 w k - 0 1",
        legal_fen: "4k2r/8/8/8/8/8/8/4K3 w k - 0 1",
        hash_always: 5408655372968565109,
        hash_legal: 5408655372968565109,
        legal_ep: None,
        castling_rights: 4,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &["e1d1", "e1d2", "e1e2", "e1f1", "e1f2"], // 5 moves
    },
    Accepted {
        name: "enemy rook on the h1 home square drops K",
        fen: "4k3/8/8/8/8/8/8/4K2r w K - 0 1",
        expected_fen: "4k3/8/8/8/8/8/8/4K2r w - - 0 1",
        legal_fen: "4k3/8/8/8/8/8/8/4K2r w - - 0 1",
        hash_always: 5380764187090223360,
        hash_legal: 5380764187090223360,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &["e1d2", "e1e2", "e1f2"], // 3 moves
    },
    Accepted {
        name: "ep target on the wrong rank is dropped",
        fen: "4k3/8/8/3pP3/8/8/8/4K3 w - d3 0 1",
        expected_fen: "4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1",
        legal_fen: "4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1",
        hash_always: 7268569286418623779,
        hash_legal: 7268569286418623779,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &["e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "e5e6"], // 6 moves
    },
    Accepted {
        name: "ep target without an enemy pawn behind is dropped",
        fen: "4k3/8/8/3pP3/8/8/8/4K3 w - c6 0 1",
        expected_fen: "4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1",
        legal_fen: "4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1",
        hash_always: 7268569286418623779,
        hash_legal: 7268569286418623779,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &["e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "e5e6"], // 6 moves
    },
    Accepted {
        name: "ep target with a non-pawn victim is dropped",
        fen: "4k3/8/8/3nP3/8/8/8/4K3 w - d6 0 1",
        expected_fen: "4k3/8/8/3nP3/8/8/8/4K3 w - - 0 1",
        legal_fen: "4k3/8/8/3nP3/8/8/8/4K3 w - - 0 1",
        hash_always: 16317482571546529517,
        hash_legal: 16317482571546529517,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &["e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "e5e6"], // 6 moves
    },
    Accepted {
        name: "ep target on an occupied square is dropped",
        fen: "4k3/8/3R4/3pP3/8/8/8/4K3 w - d6 0 1",
        expected_fen: "4k3/8/3R4/3pP3/8/8/8/4K3 w - - 0 1",
        legal_fen: "4k3/8/3R4/3pP3/8/8/8/4K3 w - - 0 1",
        hash_always: 9347072776022341767,
        hash_legal: 9347072776022341767,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "d6a6", "d6b6", "d6c6", "d6d5", "d6d7", "d6d8", "d6e6", "d6f6", "d6g6", "d6h6", "e1d1",
            "e1d2", "e1e2", "e1f1", "e1f2", "e5e6",
        ], // 16 moves
    },
    Accepted {
        name: "ep target with an occupied double-push origin is dropped",
        fen: "4k3/3R4/8/3pP3/8/8/8/4K3 w - d6 0 1",
        expected_fen: "4k3/3R4/8/3pP3/8/8/8/4K3 w - - 0 1",
        legal_fen: "4k3/3R4/8/3pP3/8/8/8/4K3 w - - 0 1",
        hash_always: 16610476159837098817,
        hash_legal: 16610476159837098817,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "d7a7", "d7b7", "d7c7", "d7d5", "d7d6", "d7d8", "d7e7", "d7f7", "d7g7", "d7h7", "e1d1",
            "e1d2", "e1e2", "e1f1", "e1f2", "e5e6",
        ], // 16 moves
    },
    Accepted {
        name: "black legal ep target",
        fen: "4k3/8/8/8/3Pp3/8/8/4K3 b - d3 0 1",
        expected_fen: "4k3/8/8/8/3Pp3/8/8/4K3 b - d3 0 1",
        legal_fen: "4k3/8/8/8/3Pp3/8/8/4K3 b - d3 0 1",
        hash_always: 8385227815484728406,
        hash_legal: 8385227815484728406,
        legal_ep: Some(Square::D3),
        castling_rights: 0,
        ep_square: Some(Square::D3),
        rule50: 0,
        game_ply: 2,
        side_to_move: Color::Black,
        legal_moves: &["e4d3", "e4e3", "e8d7", "e8d8", "e8e7", "e8f7", "e8f8"], // 7 moves
    },
    Accepted {
        name: "nine white pawns are accepted (material bound ignored)",
        fen: "4k3/PPP1P1P1/8/8/8/8/PPP5/4K3 w - - 0 1",
        expected_fen: "4k3/PPP1P1P1/8/8/8/8/PPP5/4K3 w - - 0 1",
        legal_fen: "4k3/PPP1P1P1/8/8/8/8/PPP5/4K3 w - - 0 1",
        hash_always: 5755900718503868174,
        hash_legal: 5755900718503868174,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "a2a3", "a2a4", "a7a8b", "a7a8n", "a7a8q", "a7a8r", "b2b3", "b2b4", "b7b8b", "b7b8n",
            "b7b8q", "b7b8r", "c2c3", "c2c4", "c7c8b", "c7c8n", "c7c8q", "c7c8r", "e1d1", "e1d2",
            "e1e2", "e1f1", "e1f2", "g7g8b", "g7g8n", "g7g8q", "g7g8r",
        ], // 27 moves
    },
    Accepted {
        name: "seven queens with all eight pawns are accepted (material bound ignored)",
        fen: "4k3/8/8/8/8/8/PPPPPPPP/QQQQKQQQ w - - 0 1",
        expected_fen: "4k3/8/8/8/8/8/PPPPPPPP/QQQQKQQQ w - - 0 1",
        legal_fen: "4k3/8/8/8/8/8/PPPPPPPP/QQQQKQQQ w - - 0 1",
        hash_always: 16628347953608531390,
        hash_legal: 16628347953608531390,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "a2a3", "a2a4", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3", "e2e4", "f2f3",
            "f2f4", "g2g3", "g2g4", "h2h3", "h2h4",
        ], // 16 moves
    },
    Accepted {
        name: "terminal board: side to move has no commoner",
        fen: "4k3/8/8/8/8/8/8/8 w - - 0 1",
        expected_fen: "4k3/8/8/8/8/8/8/8 w - - 0 1",
        legal_fen: "4k3/8/8/8/8/8/8/8 w - - 0 1",
        hash_always: 296777328187326797,
        hash_legal: 296777328187326797,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[], // 0 moves
    },
    Accepted {
        name: "terminal board: side to move (Black) has no commoner",
        fen: "8/8/8/8/8/8/8/4K3 b - - 0 1",
        expected_fen: "8/8/8/8/8/8/8/4K3 b - - 0 1",
        legal_fen: "8/8/8/8/8/8/8/4K3 b - - 0 1",
        hash_always: 7378407734676953531,
        hash_legal: 7378407734676953531,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        game_ply: 2,
        side_to_move: Color::Black,
        legal_moves: &[], // 0 moves
    },
    Accepted {
        name: "touching commoners do not count as opposite check (perft corpus 9)",
        fen: "3r4/8/8/5k1p/4KN1P/p1p5/8/8 w - - 6 44",
        expected_fen: "3r4/8/8/5k1p/4KN1P/p1p5/8/8 w - - 6 44",
        legal_fen: "3r4/8/8/5k1p/4KN1P/p1p5/8/8 w - - 6 44",
        hash_always: 1193170683012182549,
        hash_legal: 1193170683012182549,
        legal_ep: None,
        castling_rights: 0,
        ep_square: None,
        rule50: 6,
        game_ply: 87,
        side_to_move: Color::White,
        legal_moves: &[
            "e4e3", "e4e5", "e4f3", "f4d3", "f4d5", "f4e2", "f4e6", "f4g2", "f4g6", "f4h3", "f4h5",
        ], // 11 moves
    },
    Accepted {
        name: "FSF-style stored ep target with no legal ep capture",
        fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
        expected_fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
        legal_fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
        hash_always: 5496414761369095902,
        hash_legal: 114368471060651418,
        legal_ep: None,
        castling_rights: 15,
        ep_square: Some(Square::D6),
        rule50: 0,
        game_ply: 3,
        side_to_move: Color::White,
        legal_moves: &[
            "a2a3", "a2a4", "b1a3", "b1c3", "b2b3", "b2b4", "c2c3", "c2c4", "d1e2", "d1f3", "d1g4",
            "d1h5", "d2d3", "d2d4", "e1e2", "e4d5", "e4e5", "f1a6", "f1b5", "f1c4", "f1d3", "f1e2",
            "f2f3", "f2f4", "g1e2", "g1f3", "g1h3", "g2g3", "g2g4", "h2h3", "h2h4",
        ], // 31 moves
    },
    Accepted {
        name: "white castling both sides",
        fen: "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
        expected_fen: "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
        legal_fen: "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
        hash_always: 6248352153838572182,
        hash_legal: 6248352153838572182,
        legal_ep: None,
        castling_rights: 3,
        ep_square: None,
        rule50: 0,
        game_ply: 1,
        side_to_move: Color::White,
        legal_moves: &[
            "a1a2", "a1a3", "a1a4", "a1a5", "a1a6", "a1a7", "a1a8", "a1b1", "a1c1", "a1d1", "e1c1",
            "e1d1", "e1d2", "e1e2", "e1f1", "e1f2", "e1g1", "h1f1", "h1g1", "h1h2", "h1h3", "h1h4",
            "h1h5", "h1h6", "h1h7", "h1h8",
        ], // 26 moves
    },
    Accepted {
        name: "black castling both sides",
        fen: "r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1",
        expected_fen: "r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1",
        legal_fen: "r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1",
        hash_always: 13382229547532594949,
        hash_legal: 13382229547532594949,
        legal_ep: None,
        castling_rights: 12,
        ep_square: None,
        rule50: 0,
        game_ply: 2,
        side_to_move: Color::Black,
        legal_moves: &[
            "a8a1", "a8a2", "a8a3", "a8a4", "a8a5", "a8a6", "a8a7", "a8b8", "a8c8", "a8d8", "e8c8",
            "e8d7", "e8d8", "e8e7", "e8f7", "e8f8", "e8g8", "h8f8", "h8g8", "h8h1", "h8h2", "h8h3",
            "h8h4", "h8h5", "h8h6", "h8h7",
        ], // 26 moves
    },
];

struct Rejected {
    name: &'static str,
    fen: &'static str,
    expected: ValidationErrors,
}

const REJECTED: &[Rejected] = &[
    Rejected {
        name: "empty board",
        fen: "8/8/8/8/8/8/8/8 w - - 0 1",
        expected: ValidationErrors::EMPTY_BOARD
            .union(ValidationErrors::MISSING_COMMONER_WHITE)
            .union(ValidationErrors::MISSING_COMMONER_BLACK),
    },
    Rejected {
        name: "missing opponent commoner",
        fen: "8/8/8/8/8/8/8/4K3 w - - 0 1",
        expected: ValidationErrors::MISSING_COMMONER_BLACK,
    },
    Rejected {
        name: "missing opponent commoner (Black to move)",
        fen: "4k3/8/8/8/8/8/8/8 b - - 0 1",
        expected: ValidationErrors::MISSING_COMMONER_WHITE,
    },
    Rejected {
        name: "two white commoners",
        fen: "4k3/8/8/8/8/8/8/3KK3 w - - 0 1",
        expected: ValidationErrors::TOO_MANY_COMMONERS_WHITE,
    },
    Rejected {
        name: "two black commoners",
        fen: "3kk3/8/8/8/8/8/8/4K3 w - - 0 1",
        expected: ValidationErrors::TOO_MANY_COMMONERS_BLACK,
    },
    Rejected {
        name: "pawn on rank 1",
        fen: "4k3/8/8/8/8/8/8/P3K3 w - - 0 1",
        expected: ValidationErrors::PAWNS_ON_BACKRANK,
    },
    Rejected {
        name: "pawn on rank 8",
        fen: "p3k3/8/8/8/8/8/8/4K3 w - - 0 1",
        expected: ValidationErrors::PAWNS_ON_BACKRANK,
    },
    Rejected {
        name: "side that just moved is in check",
        fen: "4k3/4R3/8/8/8/8/8/4K3 w - - 0 1",
        expected: ValidationErrors::OPPOSITE_CHECK,
    },
    Rejected {
        name: "impossible check with a valid ep target",
        fen: "4k3/8/8/3pP3/8/8/8/4K2r w - d6 0 1",
        expected: ValidationErrors::IMPOSSIBLE_CHECK,
    },
];

struct SyntaxCase {
    name: &'static str,
    fen: &'static str,
    expected: &'static str,
}

const SYNTAX: &[SyntaxCase] = &[
    SyntaxCase {
        name: "too few fields",
        fen: "4k3/8/8/8/8/8/8/4K3 w -",
        expected: "TooShort",
    },
    SyntaxCase {
        name: "too many fields",
        fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 1 x",
        expected: "InvalidFieldCount",
    },
    SyntaxCase {
        name: "seven ranks",
        fen: "4k3/8/8/8/8/8/4K3 w - - 0 1",
        expected: "WrongRankCount",
    },
    SyntaxCase {
        name: "nine ranks",
        fen: "4k3/8/8/8/8/8/8/8/4K3 w - - 0 1",
        expected: "WrongRankCount",
    },
    SyntaxCase {
        name: "invalid piece character",
        fen: "4k3/8/8/8/8/8/8/4K2X w - - 0 1",
        expected: "InvalidPlacement",
    },
    SyntaxCase {
        name: "invalid side to move",
        fen: "4k3/8/8/8/8/8/8/4K3 x - - 0 1",
        expected: "InvalidSideToMove",
    },
    SyntaxCase {
        name: "invalid castling character",
        fen: "4k3/8/8/8/8/8/8/4K3 w X - 0 1",
        expected: "InvalidCastling",
    },
    SyntaxCase {
        name: "malformed ep token",
        fen: "4k3/8/8/8/8/8/8/4K3 w - x9 0 1",
        expected: "InvalidEpSquare",
    },
    SyntaxCase {
        name: "non-integer halfmove clock",
        fen: "4k3/8/8/8/8/8/8/4K3 w - - x 1",
        expected: "ParseInt",
    },
    SyntaxCase {
        name: "non-integer fullmove number",
        fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 x",
        expected: "ParseInt",
    },
    SyntaxCase {
        name: "zero fullmove number",
        fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 0",
        expected: "ParseInt",
    },
];

struct PolicyCase {
    fen: &'static str,
    strict: bool,
    allow_terminal: bool,
    allow_impossible_check: bool,
    permissive: bool,
}

const POLICY_CASES: &[PolicyCase] = &[
    PolicyCase {
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        strict: true,
        allow_terminal: true,
        allow_impossible_check: true,
        permissive: true,
    },
    PolicyCase {
        fen: "8/8/8/8/8/8/8/8 w - - 0 1",
        strict: false,
        allow_terminal: true,
        allow_impossible_check: false,
        permissive: true,
    },
    PolicyCase {
        fen: "4k3/8/8/8/8/8/8/8 w - - 0 1",
        strict: false,
        allow_terminal: true,
        allow_impossible_check: false,
        permissive: true,
    },
    PolicyCase {
        fen: "4k3/8/8/8/8/8/8/3KK3 w - - 0 1",
        strict: false,
        allow_terminal: false,
        allow_impossible_check: false,
        permissive: true,
    },
    PolicyCase {
        fen: "4k3/8/8/8/8/8/8/P3K3 w - - 0 1",
        strict: false,
        allow_terminal: false,
        allow_impossible_check: false,
        permissive: true,
    },
    PolicyCase {
        fen: "4k3/4R3/8/8/8/8/8/4K3 w - - 0 1",
        strict: false,
        allow_terminal: false,
        allow_impossible_check: true,
        permissive: true,
    },
    PolicyCase {
        fen: "4k3/4p3/8/4p3/8/8/8/4K3 w - e6 0 1",
        strict: false,
        allow_terminal: false,
        allow_impossible_check: false,
        permissive: true,
    },
    PolicyCase {
        fen: "4k3/8/8/8/8/8/PPPPPPPP/QQQQKQQQ w - - 0 1",
        strict: false,
        allow_terminal: false,
        allow_impossible_check: false,
        permissive: true,
    },
    PolicyCase {
        fen: "4k3/8/8/8/8/8/8/4K2R w KQ - 0 1",
        strict: false,
        allow_terminal: false,
        allow_impossible_check: false,
        permissive: false,
    },
    PolicyCase {
        fen: "8/8/8/8/8/8/8/4K3 b - - 0 1",
        strict: false,
        allow_terminal: true,
        allow_impossible_check: false,
        permissive: true,
    },
];

fn fen_error_name(error: &FenError) -> &'static str {
    match error {
        FenError::TooShort { .. } => "TooShort",
        FenError::WrongRankCount { .. } => "WrongRankCount",
        FenError::InvalidSideToMove(_) => "InvalidSideToMove",
        FenError::InvalidCastling(_) => "InvalidCastling",
        FenError::InvalidEpSquare(_) => "InvalidEpSquare",
        FenError::InvalidPlacement(_) => "InvalidPlacement",
        FenError::InvalidFieldCount { .. } => "InvalidFieldCount",
        FenError::ParseInt(_) => "ParseInt",
    }
}

fn legal_ucis(board: &Board) -> Vec<String> {
    let mut list = MoveList::new();
    movegen::generate_legal(board, &mut list);
    list.as_slice().iter().map(|m| m.to_uci()).collect()
}

fn sorted_legal_ucis(board: &Board) -> Vec<String> {
    let mut moves = legal_ucis(board);
    moves.sort();
    moves
}

fn perft_corpus() -> Vec<String> {
    let content = std::fs::read_to_string("tests/perft_values.md")
        .expect("tests/perft_values.md must be readable");
    let mut fens = Vec::new();
    for line in content.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let Some(start) = line.find('`') else {
            continue;
        };
        let Some(len) = line[start + 1..].find('`') else {
            continue;
        };
        fens.push(line[start + 1..start + 1 + len].to_owned());
    }
    assert_eq!(fens.len(), 41, "expected 41 perft corpus positions");
    fens
}

#[test]
fn accepted_fixtures_have_the_frozen_board() {
    for case in ACCEPTED {
        let board = Board::from_fen_with(case.fen, FenPolicy::Lichess)
            .unwrap_or_else(|e| panic!("{}: expected accept, got {e:?}", case.name));

        assert_eq!(board.fen(), case.expected_fen, "{}", case.name);
        assert_eq!(
            board.fen_with(EnPassantMode::Legal),
            case.legal_fen,
            "{}",
            case.name
        );
        assert_eq!(board.hash(), case.hash_always, "{}", case.name);
        assert_eq!(
            board.hash_with(EnPassantMode::Legal),
            case.hash_legal,
            "{}",
            case.name
        );
        assert_eq!(board.legal_ep_square(), case.legal_ep, "{}", case.name);
        assert_eq!(
            board.castling_rights(),
            case.castling_rights,
            "{}",
            case.name
        );
        assert_eq!(board.ep_square(), case.ep_square, "{}", case.name);
        assert_eq!(board.rule50(), case.rule50, "{}", case.name);
        assert_eq!(board.game_ply(), case.game_ply, "{}", case.name);
        assert_eq!(board.side_to_move(), case.side_to_move, "{}", case.name);
        assert_eq!(sorted_legal_ucis(&board), case.legal_moves, "{}", case.name);

        // The sanitized board must be the board of its expected FEN, including
        // generation order, so the defaults and Lichess cannot drift apart.
        let rebuilt = Board::from_fen(case.expected_fen)
            .unwrap_or_else(|e| panic!("{}: expected FEN must parse: {e:?}", case.name));
        assert_eq!(legal_ucis(&board), legal_ucis(&rebuilt), "{}", case.name);
    }
}

#[test]
fn rejected_fixtures_have_the_frozen_errors() {
    for case in REJECTED {
        match Board::from_fen_with(case.fen, FenPolicy::Lichess) {
            Err(FenLoadError::InvalidPosition(errors)) => {
                assert_eq!(errors, case.expected, "{}", case.name);
                assert!(!errors.is_empty(), "{}", case.name);
                assert!(
                    !errors.contains(ValidationErrors::INVALID_CASTLING_RIGHTS),
                    "{}: sanitization should have removed castling errors",
                    case.name
                );
                assert!(
                    !errors.contains(ValidationErrors::INVALID_EP_SQUARE),
                    "{}: sanitization should have removed ep errors",
                    case.name
                );
                assert!(
                    !errors.contains(ValidationErrors::TOO_MUCH_MATERIAL),
                    "{}: material is not a Lichess rejection",
                    case.name
                );
            }
            other => panic!("{}: expected InvalidPosition, got {other:?}", case.name),
        }

        // Semantic rejects are syntactically valid, so the default parser
        // still accepts them and only the Lichess policy rejects them.
        assert!(
            Board::from_fen(case.fen).is_ok(),
            "{}: default parser should still accept this FEN",
            case.name
        );
    }
}

#[test]
fn syntax_fixtures_keep_the_default_parser_answer() {
    for case in SYNTAX {
        match Board::from_fen_with(case.fen, FenPolicy::Lichess) {
            Err(FenLoadError::Syntax(error)) => {
                assert_eq!(fen_error_name(&error), case.expected, "{}", case.name);
            }
            other => panic!("{}: expected syntax error, got {other:?}", case.name),
        }

        let default = Board::from_fen(case.fen).expect_err("default parser must reject");
        assert_eq!(fen_error_name(&default), case.expected, "{}", case.name);
    }
}

#[test]
fn default_parser_still_rejects_invalid_castling_and_ep() {
    let cases = [
        ("4k3/8/8/8/8/8/8/4K2R w KQ - 0 1", "InvalidCastling"),
        ("4k3/8/8/8/8/8/8/4K2R w Q - 0 1", "InvalidCastling"),
        ("4k3/8/8/3pP3/8/8/8/4K3 w - d3 0 1", "InvalidEpSquare"),
        ("4k3/8/8/3pP3/8/8/8/4K3 w - c6 0 1", "InvalidEpSquare"),
    ];
    for (fen, expected) in cases {
        let error = Board::from_fen(fen).expect_err("default parser must reject");
        assert_eq!(fen_error_name(&error), expected, "{fen}");
        assert!(
            Board::from_fen_with(fen, FenPolicy::Lichess).is_ok(),
            "{fen}: Lichess policy should sanitize instead of rejecting"
        );
    }
}

#[test]
fn default_parser_error_precedence_is_unchanged() {
    // Castling backing is reported before clocks.
    let castling = "4k3/8/8/8/8/8/8/4K2R w KQ - x 1";
    assert_eq!(
        fen_error_name(&Board::from_fen(castling).unwrap_err()),
        "InvalidCastling"
    );

    // Ep geometry is reported before clocks.
    let ep = "4k3/8/8/3pP3/8/8/8/4K3 w - d3 x 1";
    assert_eq!(
        fen_error_name(&Board::from_fen(ep).unwrap_err()),
        "InvalidEpSquare"
    );

    // A valid prefix reaches the clock error.
    let clock = "4k3/8/8/8/8/8/8/4K3 w - - x 1";
    assert_eq!(
        fen_error_name(&Board::from_fen(clock).unwrap_err()),
        "ParseInt"
    );
}

#[test]
fn corpus_positions_are_accepted_unchanged() {
    for fen in perft_corpus() {
        let default = Board::from_fen(&fen).unwrap_or_else(|e| panic!("{fen}: {e:?}"));
        let lichess = Board::from_fen_with(&fen, FenPolicy::Lichess)
            .unwrap_or_else(|e| panic!("{fen}: {e:?}"));
        assert_eq!(default.fen(), lichess.fen(), "{fen}");
        assert_eq!(default.hash(), lichess.hash(), "{fen}");
        assert_eq!(
            default.hash_with(EnPassantMode::Legal),
            lichess.hash_with(EnPassantMode::Legal),
            "{fen}"
        );
        assert_eq!(
            default.castling_rights(),
            lichess.castling_rights(),
            "{fen}"
        );
        assert_eq!(default.ep_square(), lichess.ep_square(), "{fen}");
        assert_eq!(
            default.legal_ep_square(),
            lichess.legal_ep_square(),
            "{fen}"
        );
        assert_eq!(legal_ucis(&default), legal_ucis(&lichess), "{fen}");
    }
}

#[test]
fn existing_policies_keep_their_answers() {
    for case in POLICY_CASES {
        assert_eq!(
            Board::from_fen_with(case.fen, FenPolicy::Strict).is_ok(),
            case.strict,
            "Strict changed for {}",
            case.fen
        );
        assert_eq!(
            Board::from_fen_with(case.fen, FenPolicy::AllowTerminal).is_ok(),
            case.allow_terminal,
            "AllowTerminal changed for {}",
            case.fen
        );
        assert_eq!(
            Board::from_fen_with(case.fen, FenPolicy::AllowImpossibleCheck).is_ok(),
            case.allow_impossible_check,
            "AllowImpossibleCheck changed for {}",
            case.fen
        );
        assert_eq!(
            Board::from_fen_with(case.fen, FenPolicy::Permissive).is_ok(),
            case.permissive,
            "Permissive changed for {}",
            case.fen
        );
    }
}

#[test]
fn mutation_differential_matches_the_frozen_oracle_table() {
    let data = std::fs::read_to_string("tests/data/fen_lichess_mutations.tsv")
        .expect("tests/data/fen_lichess_mutations.tsv must be readable");
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut syntax = 0usize;

    for (line_no, line) in data.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        let fen = columns.next().expect("fen column");
        let disposition = columns.next().expect("disposition column");
        let expected_fen = columns.next().unwrap_or("-");
        assert_eq!(columns.next(), None, "line {}: extra columns", line_no + 1);

        match disposition {
            "ACCEPT" => {
                accepted += 1;
                let board = Board::from_fen_with(fen, FenPolicy::Lichess)
                    .unwrap_or_else(|e| panic!("line {}: {fen}: {e:?}", line_no + 1));
                assert_eq!(board.fen(), expected_fen, "line {}: {fen}", line_no + 1);
                assert_eq!(
                    fen.split(' ').next(),
                    expected_fen.split(' ').next(),
                    "line {}: Lichess policy must not remove pieces from {fen}",
                    line_no + 1
                );
            }
            "REJECT" => {
                rejected += 1;
                match Board::from_fen_with(fen, FenPolicy::Lichess) {
                    Err(FenLoadError::InvalidPosition(errors)) => {
                        assert!(!errors.is_empty(), "line {}: {fen}", line_no + 1);
                        assert!(
                            !errors.contains(ValidationErrors::INVALID_CASTLING_RIGHTS)
                                && !errors.contains(ValidationErrors::INVALID_EP_SQUARE)
                                && !errors.contains(ValidationErrors::TOO_MUCH_MATERIAL),
                            "line {}: {fen}: post-sanitization error {errors}",
                            line_no + 1
                        );
                    }
                    other => panic!(
                        "line {}: {fen}: expected InvalidPosition, got {other:?}",
                        line_no + 1
                    ),
                }
            }
            "SYNTAX" => {
                syntax += 1;
                assert!(
                    matches!(
                        Board::from_fen_with(fen, FenPolicy::Lichess),
                        Err(FenLoadError::Syntax(_))
                    ),
                    "line {}: {fen}: expected Syntax",
                    line_no + 1
                );
            }
            other => panic!("line {}: unknown disposition {other}", line_no + 1),
        }
    }

    assert_eq!(accepted + rejected + syntax, 1074, "frozen table size");
    assert_eq!(accepted, 483, "frozen accepted count");
    assert_eq!(rejected, 591, "frozen rejected count");
    assert_eq!(syntax, 0, "frozen syntax count");
}

#[test]
fn from_setup_with_lichess_applies_only_validation() {
    use atomic_movegen::{Setup, make_piece};

    // The validation half accepts the material bound being exceeded.
    let mut setup = Setup::new();
    setup
        .place(
            Square::E1,
            make_piece(Color::White, atomic_movegen::PieceType::Commoner),
        )
        .place(
            Square::E8,
            make_piece(Color::Black, atomic_movegen::PieceType::Commoner),
        );
    for sq in [Square::A1, Square::B1, Square::C1, Square::D1] {
        setup.place(
            sq,
            make_piece(Color::White, atomic_movegen::PieceType::Queen),
        );
    }
    for sq in [
        Square::A2,
        Square::B2,
        Square::C2,
        Square::D2,
        Square::E2,
        Square::F2,
        Square::G2,
        Square::H2,
    ] {
        setup.place(
            sq,
            make_piece(Color::White, atomic_movegen::PieceType::Pawn),
        );
    }
    assert!(Board::from_setup_with(&setup, FenPolicy::Lichess).is_ok());

    // Missing commoners follow the same terminal rule as the FEN path.
    let mut setup = Setup::new();
    setup.place(
        Square::E8,
        make_piece(Color::Black, atomic_movegen::PieceType::Commoner),
    );
    assert!(Board::from_setup_with(&setup, FenPolicy::Lichess).is_ok());

    let mut setup = Setup::new();
    setup.place(
        Square::E1,
        make_piece(Color::White, atomic_movegen::PieceType::Commoner),
    );
    assert!(matches!(
        Board::from_setup_with(&setup, FenPolicy::Lichess),
        Err(FenLoadError::InvalidPosition(errors))
            if errors.contains(ValidationErrors::MISSING_COMMONER_BLACK)
    ));

    // `from_setup` only checks the ep rank and victim, so the Lichess
    // validation half rejects the remaining impossible-target cases that the
    // FEN path would sanitize away.
    let mut setup = Setup::new();
    setup
        .place(
            Square::E1,
            make_piece(Color::White, atomic_movegen::PieceType::Commoner),
        )
        .place(
            Square::E8,
            make_piece(Color::Black, atomic_movegen::PieceType::Commoner),
        )
        .place(
            Square::E5,
            make_piece(Color::White, atomic_movegen::PieceType::Pawn),
        )
        .place(
            Square::D5,
            make_piece(Color::Black, atomic_movegen::PieceType::Pawn),
        )
        .place(
            Square::D6,
            make_piece(Color::White, atomic_movegen::PieceType::Rook),
        )
        .set_side_to_move(Color::White)
        .set_ep_square(Some(Square::D6));
    assert!(Board::from_setup(&setup).is_ok());
    assert!(matches!(
        Board::from_setup_with(&setup, FenPolicy::Lichess),
        Err(FenLoadError::InvalidPosition(errors))
            if errors.contains(ValidationErrors::INVALID_EP_SQUARE)
    ));
}
