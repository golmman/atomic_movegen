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
//! # Hot-path helpers
//!
//! [`StateInfo`] is re-exported at the crate root for callers that cache
//! legality state. [`movegen::for_each_pseudo_legal`] and
//! [`movegen::for_each_legal`] walk the candidate and legal streams without
//! allocating and support early exit, while [`Board::null_for`] builds a
//! null-move position without a FEN round-trip. [`attacks::between`] and the
//! `attacks::xray_*_attacks` helpers expose the geometry an engine needs for
//! its own legality shortcuts.
//!
//! # Move effects
//!
//! [`EffectContext`] snapshots the node-constant data of a position once, so
//! [`EffectContext::effect`] can classify every pseudo-legal candidate as a
//! [`MoveEffect`] — capture, blast casualties, checks or explodes the enemy
//! commoner, keeps the mover's commoner, and legality — without writing a
//! board. [`EffectContext::is_legal`] is exactly [`Board::legal`] for
//! pseudo-legal input, and [`EffectContext::outcome_after`] (also reachable as
//! [`Board::outcome_after`]) is exactly the child's [`Board::outcome`] for
//! legal input.
//!
//! # Interchange: SAN and piece sets
//!
//! [`Board::san`] / [`Board::write_san`] format a **legal** move in standard
//! algebraic notation (castling as `O-O`/`O-O-O`, standard disambiguation, and a
//! `#`/`+` suffix taken from the child outcome), and [`Board::move_from_san`]
//! resolves SAN text back to exactly one legal move. The commoner's SAN letter
//! is `K`, like its FEN label. [`Setup`] builds a position piece by piece
//! instead of parsing a FEN string, and [`Board::pieces`] iterates the
//! placement in ascending square order. `Setup` is a data builder only: it
//! duplicates no move generation or legality logic.
//!
//! # Position keys and en passant
//!
//! [`Board::hash`] and [`Board::fen`] follow Fairy-Stockfish and include the
//! stored en-passant target after every double pawn push, even when no legal
//! en-passant capture exists. For canonicalized keys, use
//! [`Board::legal_ep_square`] with [`EnPassantMode::Legal`] via
//! [`Board::hash_with`], [`Board::fen_with`], or [`Board::hash_after_with`].
//! Positions that differ only by a non-capturable ep target then share a key
//! and a FEN while the default `Always` behavior stays unchanged.
//!
//! The key table and its XOR composition are FSF-compatible under
//! [`EnPassantMode::Always`] and stable across the 2.x line. Any change to the
//! table or its composition is a major-version change and bumps
//! [`ZOBRIST_VERSION`]. [`EnPassantMode::Legal`] is a canonicalization on top
//! of the stable base keys, not a separate table. Persisted transposition
//! tables should store [`ZOBRIST_VERSION`] with their data and invalidate on a
//! mismatch.
//!
//! # Terminal reasons
//!
//! [`Board::outcome`] answers *whether* a position is decided; [`Board::terminal`]
//! (and its `_with_state`/`_with` forms) also answers *why*, returning a
//! [`Terminal`] with an [`Outcome`] and a [`TerminalReason`]. The reason mirrors
//! `outcome`'s precedence, including the commoner-only material draw, so it is
//! not the same as [`Board::is_terminal`].
//!
//! # Generation order contract
//!
//! Generation order is a public compatibility contract: pawn moves per pawn in
//! ascending origin-square order (single push, four promotions in
//! [`PROMOTION_PIECES`](types::PROMOTION_PIECES) order, double push, captures in ascending destination
//! order, en passant), then knights, bishops, rooks, queens, commoners, then
//! castling (king side before queen side). [`movegen::generate_legal`]
//! preserves the pseudo-legal order after filtering. The order may only change
//! in a major release and is enforced by `tests/move_order_golden.rs`.
//!
//! # Engineering contract
//!
//! - [`Board`], [`StateInfo`], [`MoveList`], [`Move`], [`EffectContext`],
//!   [`Setup`], and the public enums are `Send + Sync`.
//! - The move path does not allocate: [`movegen::generate_pseudo_legal`],
//!   [`movegen::generate_legal`], [`movegen::generate_legal_with_state`],
//!   [`movegen::has_legal_move`], [`movegen::has_legal_move_with_state`],
//!   [`movegen::for_each_pseudo_legal`], [`movegen::for_each_legal`],
//!   [`movegen::for_each_legal_with_state`], [`Board::legal`],
//!   [`Board::is_legal`], [`Board::is_legal_with_state`],
//!   [`Board::is_pseudo_legal`], [`Board::do_move`], [`Board::undo_move`],
//!   [`Board::hash`], [`Board::hash_with`], [`Board::hash_after`],
//!   [`Board::hash_after_with`], [`Board::hash_and_rule50_after`],
//!   [`Board::rule50_after`], [`Board::outcome`], [`Board::outcome_with_state`],
//!   [`Board::outcome_with`], [`Board::terminal`], [`Board::terminal_with_state`],
//!   [`Board::terminal_with`], [`Board::is_terminal`],
//!   [`Board::blast_casualties`], [`Board::attackers_to`],
//!   [`Board::attackers_to_color`], and [`Move::write_uci`].
//! - Allocations are allowed in interchange ([`Board::fen`], [`Move::to_uci`],
//!   [`Board::san`], [`Setup`] construction) and in user-facing convenience
//!   methods that return `String`.
//! - `#[non_exhaustive]` enums may gain variants in a minor release; consumers
//!   must use a wildcard arm. Removing or renaming a variant is a major
//!   release.
//! - The crate is zero-dependency and safe-Rust-by-default; the only opt-in
//!   `unsafe` is the `unsafe-optimizations` magic-table lookup.
//!
//! # Feature flags
//!
//! - `unsafe-optimizations` (off by default): removes the bounds check from
//!   the magic-bitboard table lookup. Every index stays provably in bounds by
//!   the magic-table construction invariants (see the `magic` module safety
//!   note); enable it only if you are willing to audit that argument. It
//!   changes no public API and no move-generation semantics.
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
pub mod effects;
pub(crate) mod magic;
pub mod movegen;
pub mod setup;
pub mod types;
pub(crate) mod util;
pub(crate) mod zobrist;

pub use board::{BlastCasualties, Board, FenLoadError, FenPolicy, StateInfo, ValidationErrors};
pub use effects::{EffectContext, MoveEffect};
pub use setup::{Setup, SetupError};
pub use types::{
    Bitboard, CastlingSide, Color, EnPassantMode, Move, MoveList, NO_PIECE, Outcome, Piece,
    PieceType, Square, Terminal, TerminalReason, make_piece,
};

/// Version of the Zobrist key schema.
///
/// This is `1` for the current FSF-compatible key table. It is incremented
/// only when the table or its XOR composition changes; such a change is a
/// major-version change. Persisted transposition tables should store this
/// value with their data and invalidate on a mismatch.
pub const ZOBRIST_VERSION: u32 = 1;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn send_sync_assertions() {
        assert_send_sync::<Board>();
        assert_send_sync::<StateInfo>();
        assert_send_sync::<Move>();
        assert_send_sync::<MoveList>();
        assert_send_sync::<Bitboard>();
        assert_send_sync::<Color>();
        assert_send_sync::<Piece>();
        assert_send_sync::<PieceType>();
        assert_send_sync::<Outcome>();
        assert_send_sync::<Terminal>();
        assert_send_sync::<TerminalReason>();
        assert_send_sync::<EffectContext>();
        assert_send_sync::<Setup>();
        assert_send_sync::<FenLoadError>();
        assert_send_sync::<SetupError>();
        assert_send_sync::<board::FenError>();
        assert_send_sync::<board::ValidationErrors>();
    }

    #[test]
    fn clone_roundtrip() {
        const FENS: &[&str] = &[
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
            "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
            "4k3/8/8/8/8/8/8/4K2R w - - 100 1",
        ];

        for fen in FENS {
            let board = Board::from_fen(fen).expect("fixture FEN must parse");
            let clone = board.clone();

            assert_eq!(clone.fen(), board.fen(), "fen for {fen}");
            assert_eq!(clone.hash(), board.hash(), "hash for {fen}");
            assert_eq!(
                clone.hash_with(EnPassantMode::Legal),
                board.hash_with(EnPassantMode::Legal),
                "legal hash for {fen}"
            );
            assert_eq!(clone.side_to_move(), board.side_to_move(), "side for {fen}");
            assert_eq!(clone.ep_square(), board.ep_square(), "ep for {fen}");
            assert_eq!(clone.rule50(), board.rule50(), "rule50 for {fen}");
            assert_eq!(clone.game_ply(), board.game_ply(), "game_ply for {fen}");
            assert_eq!(
                clone.castling_rights(),
                board.castling_rights(),
                "castling for {fen}"
            );
            assert_eq!(clone.outcome(), board.outcome(), "outcome for {fen}");
        }
    }
}
