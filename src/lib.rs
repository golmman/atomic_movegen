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
//! - **COMMONER replaces KING:** a commoner moves like a king. Losing all of
//!   your commoners loses the game.
//! - **Pseudo-royal only for the last commoner:** a commoner is treated as
//!   pseudo-royal (it cannot be left under attack and its loss ends the game)
//!   only when it is the side's last commoner. Touching an enemy commoner is
//!   allowed and does not count as an attack. This is `extinctionPseudoRoyal`
//!   with the default `extinctionPieceCount = 0` in Fairy-Stockfish's `atomic`
//!   variant.
//! - **Not `atomar`:** commoners are not mutually immune and there is no
//!   `mutuallyImmuneTypes` / `blastImmuneTypes` rule.
//! - **No check/mate in the usual sense:** the game ends when a side has no
//!   commoners left.
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
