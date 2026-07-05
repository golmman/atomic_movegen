//! `atomic-movegen` — atomic chess move generation in Rust.
//!
//! This crate implements legal move generation, FEN parsing, and perft for
//! [atomic chess](https://en.wikipedia.org/wiki/Atomic_chess).
//!
//! # Atomic chess rules implemented
//!
//! - **Blast on capture:** capturing (or en passant) destroys all non-pawn
//!   pieces in a 3×3 king-move blast zone centered on the capture square,
//!   including the capturer itself if it is not a pawn.
//! - **Pawns are blast-immune:** pawns are never removed by a blast.
//! - **COMMONER replaces KING:** pieces move like kings but are pseudo-royal.
//!   Losing all COMMONERs means loss. Adjacent COMMONERs (even own) are illegal
//!   (extinction pseudo-royal rule).
//! - **No check/mate in the usual sense:** the game ends when a side has no
//!   COMMONERs left.
//!
//! # Example
//!
//! ```rust
//! use atomic_movegen::board::Board;
//! use atomic_movegen::perft;
//!
//! let mut board = Board::new();
//! let nodes = perft(&mut board, 3);
//! assert_eq!(nodes, 8902);
//! ```

pub mod attacks;
pub mod bitboard;
pub mod board;
pub mod magic;
pub mod movegen;
pub mod pext;
pub mod types;

use crate::types::MoveList;

/// Count the number of legal moves at each node to the given depth (perft).
///
/// This is the standard perft (performance test) function used to verify
/// move-generator correctness against a reference engine.
pub fn perft(board: &mut board::Board, depth: u32) -> u64 {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(attacks::init);
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
