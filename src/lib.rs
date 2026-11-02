//! `atomic-movegen` — atomic chess move generation in Rust.
//!
//! This crate implements legal move generation, FEN parsing, and perft for
//! the standard `atomic` variant, validated against the
//! [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish)
//! reference implementation.
//!
//! # Atomic chess rules implemented
//!
//! - **Blast on capture:** capturing (or en passant) destroys all non-pawn
//!   pieces in a 3×3 king-move blast zone centered on the capture square,
//!   including the capturer itself.
//! - **Pawns and the blast:** pawns are not removed by the blast except when
//!   a pawn is the capturer at the blast square (`to`). In that case the
//!   capturing pawn is also destroyed.
//! - **COMMONER replaces KING:** each side has exactly one commoner, which
//!   moves like a king. There is no way to gain another one: pawns promote only
//!   to knight, bishop, rook, or queen, and there are no drops. A board
//!   therefore has one or two commoners, and a board with only one commoner is
//!   terminal.
//! - **Commoner royalty:** the commoner may not be left under attack and may
//!   not be destroyed; doing so loses the game. Touching the enemy commoner is
//!   allowed and does not count as an attack: any capture would also blast the
//!   enemy's only commoner away. This is `extinctionPseudoRoyal` with the
//!   default `extinctionPieceCount = 0` in Fairy-Stockfish's `atomic` variant.
//! - **Not `atomar`:** commoners are not mutually immune and there is no
//!   `mutuallyImmuneTypes` / `blastImmuneTypes` rule.
//! - **No check/mate in the usual sense:** the game ends when a side has no
//!   commoner left. With no legal moves, an attacked commoner loses and a safe
//!   commoner is stalemated.
//! - **Terminal precedence:** commoner extinction, then the commoner-only WDL
//!   draw, then no legal moves (attacked → loss, safe → draw), then the
//!   50-move rule. Mate outranks the clock: a checkmate on the 100th half-move
//!   is a loss, not a draw.
//! - **No insufficient-material draw:** there is no FIDE-style material table.
//!   A board with only the two commoners is a WDL draw and runs into the
//!   50-move rule. Endings such as K+B vs K, K+N vs K, and K+R vs K are not
//!   draws: the piece can capture the enemy commoner when its own commoner is
//!   outside the blast. Touching commoners are not automatically a draw when
//!   other pieces remain.
//!
//! # Move API
//!
//! [`Move`] has a stable 16-bit wire encoding via [`Move::to_bits`] and
//! [`Move::from_bits`], and implements `Hash` and `Ord`, so consumers can key
//! collections by move. [`Move::from_uci`] is a syntax-only parser (castling
//! text becomes a normal two-square move); [`Board::move_from_uci`] is the
//! board-aware lookup that resolves castling and en-passant encodings and
//! requires full legality. [`Move::write_uci`] formats without allocating, and
//! `Display` delegates to it.
//!
//! For untrusted single moves, use [`Board::is_legal`] or
//! [`Board::is_legal_with_state`]; [`Board::is_pseudo_legal`] mirrors the
//! pseudo-legal generator. None of these builds a [`MoveList`].
//!
//! # Example
//!
//! ```rust
//! use atomic_movegen::Board;
//! use atomic_movegen::perft;
//!
//! let mut board = Board::new();
//! let nodes = perft(&mut board, 3);
//! assert_eq!(nodes, 8902);
//! ```

#![warn(missing_docs)]

pub mod attacks;
pub(crate) mod bitboard;
pub mod board;
pub(crate) mod magic;
pub mod movegen;
pub mod types;
pub(crate) mod util;
pub(crate) mod zobrist;

pub use board::Board;
pub use types::{Bitboard, Color, Move, MoveList, Outcome, PieceType, Square};

/// Count the number of legal moves at each node to the given depth (perft).
///
/// This is the standard perft (performance test) function used to verify
/// move-generator correctness against a reference engine.
#[must_use]
pub fn perft(board: &mut board::Board, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut moves = MoveList::new();
    movegen::generate_legal(board, &mut moves);

    if depth == 1 {
        return moves.len() as u64;
    }

    let mut total = 0u64;
    let mut state = board::StateInfo::new();
    for &m in moves.as_slice() {
        board.do_move(m, &mut state);
        total += perft(board, depth - 1);
        board.undo_move(m, &state);
    }
    total
}
