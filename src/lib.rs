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
//! # FEN loading policies
//!
//! [`Board::from_fen`] is the permissive FEN parser. For untrusted input use
//! [`Board::from_fen_with`] with [`FenPolicy::Strict`] (or
//! [`FenPolicy::AllowTerminal`] / [`FenPolicy::AllowImpossibleCheck`] for
//! artificial fixtures). [`FenPolicy::Lichess`] is the opt-in sanitizing
//! predicate that mirrors shakmaty's atomic `Setup` acceptance: it drops
//! unbacked castling rights and an impossible en-passant target, accepts
//! excess material, and rejects the structural illegal-position class. It is
//! the only policy that drops data; the default parser is unchanged.
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
//! [`EffectContext::terminal_after`] (also reachable as
//! [`Board::terminal_after`]) answers the child's *terminal state* — outcome
//! **and** reason — without building the child, for every pseudo-legal move.
//! The child is described by the same crate-private placement view the rules
//! read, so “does the child have a legal move?” needs no board write.
//! [`EffectContext::is_terminal_after`] mirrors [`Board::is_terminal`] on the
//! child and [`EffectContext::has_legal_move_after`] answers the existence
//! query alone. That existence question is two-stepped: a positive screen
//! proves one quiet non-commoner move legal from the placement view (a quiet
//! move empties only its origin and triggers no blast, so it is legal whenever
//! the mover's commoner is not attacked after it), and the exact
//! fast-legality ladder decides everything the screen cannot prove. The
//! answers are unchanged; the child path populates no `StateInfo` unless the
//! screen fails.
//!
//! [`EffectContext::child_outcome`] (also reachable as
//! [`Board::child_outcome`]) is the unambiguous two-level answer over the same
//! child: an [`AfterState`] of `Illegal` (the move is not legal), `Undecided`
//! (legal, the game continues) or `Terminal(t)` (legal, and `t` is the child's
//! [`Terminal`]). [`EffectContext::outcome_after`] is `child_outcome(..)`
//! reduced to the WDL result, so its `None` means *illegal* **or**
//! *undecided* — exactly as documented.
//!
//! [`PostMove`] is the child summary the effect path already derives:
//! [`EffectContext::post_move`] returns the child's placement, its removed
//! pieces, its side to move, en-passant target, castling rights and half-move
//! clock without a board write and without a rules verdict.
//! [`EffectContext::effect_and_post`] returns both values from one analysis
//! pass, and [`PostMove::terminal`] asks a summary that is already in hand for
//! its child's [`Terminal`] — it is the implementation behind
//! [`EffectContext::terminal_after`], so a consumer that kept the summary pays
//! for the child derivation once. [`EffectContext::post_move_and_key`] returns
//! the summary together with the child's Zobrist key (exactly
//! [`Board::hash_after`]) from that same derivation.
//! [`EffectContext::post_move_and_key_with`] selects the en-passant policy:
//! [`EnPassantMode::Always`] is the existing query unchanged, and
//! [`EnPassantMode::Legal`] canonically drops the child's ep-file key when no
//! fully legal en-passant capture exists. The legal-EP question is answered
//! from the already-derived child view, so a non-double-push move does no extra
//! work and a double push is not cloned. This is the same composition as
//! [`Board::hash_with`] and [`Board::hash_after_with`]: it is a
//! canonicalization on the stable key table, not a new table and not a
//! [`ZOBRIST_VERSION`] change.
//!
//! [`EffectContext::exploding_capture_after`] answers a one-ply threat in the
//! child: after the pseudo-legal move `m`, it returns the first legal capture by
//! the child's side to move whose blast removes the commoner of the side that
//! played `m`, or `None`. It is a child query, not a [`MoveClassification`]
//! field, and it reuses the already-derived child view: no clone, no board
//! write, and no `StateInfo`.
//!
//! Cost is measured, not promised: `cargo run --release --example effect_bench`
//! prints the child rows and, per child class (`quiet`, `capture`, `promotion`,
//! `en-passant`, `castling`, `commoner-move`, the aggregate `built-ish` and
//! `all`), the same questions asked of a built child. On the machine the 2.8.0
//! notes were measured on, deriving the summary costs about what
//! `clone` + `do_move` costs, and the summary answers terminality plus the key
//! more cheaply than two effect queries but not more cheaply than a child board
//! the caller would build anyway — take the summary when the child's *facts*
//! are the question, not as a faster [`Board::do_move`].
//!
//! [`MoveClassification`] is [`MoveEffect`] without the legality verdict: the
//! same five rules fields, derived by
//! [`EffectContext::classification`] without running the legality ladder, for
//! callers that already know the move is legal (for example because it came
//! out of [`movegen::generate_legal_with_state`]).
//! [`MoveEffect::classification`] returns the same value from a full effect.
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
//! # Conformance corpus
//!
//! [`conformance::CASES`] is a small frozen corpus of positions with their
//! ordered pseudo-legal and legal moves, outcome and terminal reason, literal
//! terminal flag, both en-passant key modes, and clocks. It is inert data for
//! downstream differential testing; `tests/conformance.rs` verifies every case
//! against the engine. [`CONFORMANCE_CASES`] is the same slice re-exported at
//! the crate root. [`conformance::CHILD_CASES`] is the child-state counterpart:
//! (position, move, expected [`AfterState`], child legal move count) rows that a
//! downstream can use to veto a child-terminality disagreement without trusting
//! the implementation.
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
//!   [`Board::outcome_after`], [`Board::terminal_after`],
//!   [`Board::child_outcome`],
//!   [`EffectContext::effect`], [`EffectContext::effect_and_post`],
//!   [`EffectContext::classification`], [`MoveEffect::classification`],
//!   [`EffectContext::post_move`], [`PostMove::terminal`],
//!   [`EffectContext::post_move_and_key`],
//!   [`EffectContext::post_move_and_key_with`],
//!   [`EffectContext::exploding_capture_after`],
//!   [`EffectContext::is_legal`],
//!   [`EffectContext::terminal_after`], [`EffectContext::child_outcome`],
//!   [`EffectContext::outcome_after`],
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
pub mod conformance;
pub mod effects;
pub(crate) mod magic;
pub mod movegen;
pub(crate) mod position;
pub mod setup;
pub mod types;
pub(crate) mod util;
pub(crate) mod zobrist;

pub use board::{BlastCasualties, Board, FenLoadError, FenPolicy, StateInfo, ValidationErrors};
pub use conformance::{CASES as CONFORMANCE_CASES, ConformanceCase};
pub use effects::{AfterState, EffectContext, MoveClassification, MoveEffect, PostMove};
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
        assert_send_sync::<AfterState>();
        assert_send_sync::<MoveEffect>();
        assert_send_sync::<MoveClassification>();
        assert_send_sync::<PostMove>();
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
