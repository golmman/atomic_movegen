//! Differential tests for `LegalityContext`, the node-local candidate legality
//! loop (wishlist 2 W7).
//!
//! For every pseudo-legal move of the perft corpus, the conformance and child
//! corpora, the effect/board fixtures, and the seeded play-outs:
//!
//! ```text
//! ctx.occupied() == board.occupied()
//! ctx.legal(m)   == board.legal(m, &full)
//!                == board.legal(m, &counts_only)
//!                == board.is_legal_with_state(m, &counts_only)
//! ```
//!
//! The corpus deliberately includes checks, pins, en passant, castling, and
//! terminal boards; the test counts those branches so a report can show that
//! the expensive rules were exercised.

mod common;

use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{
    Bitboard, Board, Counts, LegalityContext, Move, MoveList, PieceType, StateInfo, attacks,
};

/// The effect fixtures (`src/effects.rs::tests::CORPUS`), copied here because
/// that corpus is crate-private. They exercise quiet moves, captures, en
/// passant, promotions, castling, checks, pins, explosions and terminal boards.
const EFFECT_CORPUS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
    "8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "7k/5P2/8/8/8/8/8/K6R b - - 0 1",
    "k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1",
    "k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1",
    "7k/8/8/8/8/8/nn6/Kn6 w - - 0 1",
    "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
    "8/8/8/8/8/8/8/k7 w - - 0 1",
    "k7/8/8/8/8/8/8/7K w - - 0 1",
    "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15",
    "rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5",
    "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
    "8/8/8/8/8/4k3/8/4K2q w - - 99 60",
    "8/8/8/8/8/4k3/8/4K2q w - - 100 60",
];

/// The board fixtures used by the state-aware outcome tests.
const BOARD_FIXTURES: &[&str] = &[
    "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
    "4k3/4r3/8/8/8/8/4R3/4K3 w - - 7 23",
    "4k3/8/8/8/8/8/8/4K2R w - - 100 1",
    "8/8/8/8/8/8/8/8 w - - 0 1",
];

/// Branch fixtures: terminal boards, touching commoners, an ep pin, a slider
/// double attack, and a bare commoner endgame.
const EDGE_FIXTURES: &[&str] = &[
    "8/8/8/8/8/8/8/R6K w - - 0 1",
    "r6k/8/8/8/8/8/8/8 b - - 0 1",
    "8/8/8/8/8/8/8/k7 w - - 0 1",
    "k7/8/8/8/8/8/8/7K w - - 0 1",
    "k7/8/8/8/8/8/1K6/8 w - - 0 1",
    "8/8/8/8/8/8/3kK3/8 w - - 0 1",
    "8/8/8/8/1k6/8/2P5/4K3 w - - 0 1",
    "4k3/8/8/8/8/8/4q3/4K3 w - - 0 1",
    // Permissive multiple-commoner boards: the mask must stay exact even
    // though legal atomic play has at most one commoner per side.
    "4r1k1/8/8/8/8/K7/8/4K3 w - - 0 1",
    "4r1k1/8/8/8/8/K7/8/4K3 b - - 0 1",
    "4k3/8/8/8/8/8/8/K3K3 w - - 0 1",
];

#[derive(Default)]
struct Stats {
    positions: usize,
    terminal_positions: usize,
    in_check_positions: usize,
    not_in_check_positions: usize,
    slider_block_positions: usize,
    empty_block_positions: usize,
    pinned_positions: usize,
    ep_positions: usize,
    castling_right_positions: usize,
    castling_candidates: usize,
    ep_candidates: usize,
    candidates: usize,
    legal_candidates: usize,
    in_check_candidates: usize,
    in_check_quiet_candidates: usize,
    in_check_capture_candidates: usize,
    in_check_ep_candidates: usize,
    in_check_castling_candidates: usize,
    in_check_commoner_candidates: usize,
    mask_rejections: usize,
    legal_mask_rejections: usize,
}

/// True when the move's origin holds a commoner of the side to move.
fn is_our_commoner(board: &Board, m: Move) -> bool {
    let piece = board.piece_on(m.from_sq());
    piece.type_of() == Some(PieceType::Commoner) && piece.color() == Some(board.side_to_move())
}

/// The block mask the production context derives from `state.checkers`, using
/// only public data. `None` means "not in check" (no rejection is predicted).
fn predicted_block(board: &Board, state: &StateInfo) -> Option<Bitboard> {
    if state.checkers.is_empty() {
        return None;
    }
    if state.checkers.count() == 1 {
        let sliders = board.pieces_pt(PieceType::Bishop)
            | board.pieces_pt(PieceType::Rook)
            | board.pieces_pt(PieceType::Queen);
        if state.checkers & sliders != Bitboard::EMPTY {
            let commoner = board.commoners(board.side_to_move()).lsb();
            return Some(attacks::between(commoner, state.checkers.lsb()));
        }
    }
    Some(Bitboard::EMPTY)
}

/// Predict whether the evasion prefilter rejects `m`, from public data.
/// `None` means the position is not in check.
fn predicted_rejection(board: &Board, state: &StateInfo, m: Move) -> Option<bool> {
    let block = predicted_block(board, state)?;
    let commoner = board.commoners(board.side_to_move()).lsb();
    // The production mask lets captures (occupied), commoner moves (the
    // commoner's neighbourhood) and the stored en-passant target fall through
    // before testing the destination.
    let mut pass = block | board.occupied() | attacks::king_attacks(commoner);
    if let Some(ep) = board.ep_square() {
        pass = pass | Bitboard::square_bb(ep);
    }
    Some(pass & Bitboard::square_bb(m.to_sq()) == Bitboard::EMPTY)
}

fn check_position(board: &Board, stats: &mut Stats) {
    let mut full = StateInfo::new();
    board.populate_state(&mut full);
    let counts = board.counts();
    assert_eq!(counts, Counts::from(&full), "{board:?}");
    let ctx: LegalityContext = board.legality_context(counts);
    let evasion_ctx = board.legality_context_with_state(counts, &full);
    assert_eq!(ctx.occupied(), board.occupied(), "{board:?}");
    assert_eq!(evasion_ctx.occupied(), board.occupied(), "{board:?}");

    let mut counts_only = StateInfo::new();
    board.populate_counts(&mut counts_only);

    let fen_before = board.fen();
    let hash_before = board.hash();

    stats.positions += 1;
    if counts.commoners() == 0 || counts.them_commoners() == 0 {
        stats.terminal_positions += 1;
    }
    if !full.checkers.is_empty() {
        stats.in_check_positions += 1;
        if predicted_block(board, &full).is_some_and(|block| block.is_empty()) {
            stats.empty_block_positions += 1;
        } else {
            stats.slider_block_positions += 1;
        }
    } else {
        stats.not_in_check_positions += 1;
    }
    if !full.pinned.is_empty() {
        stats.pinned_positions += 1;
    }
    if board.ep_square().is_some() {
        stats.ep_positions += 1;
    }
    if board.castling_rights() != 0 {
        stats.castling_right_positions += 1;
    }

    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);
    for &m in pseudo.as_slice() {
        stats.candidates += 1;
        if m.is_castling() {
            stats.castling_candidates += 1;
        }
        if m.is_en_passant() {
            stats.ep_candidates += 1;
        }
        let full_legal = board.legal(m, &full);
        let evasion_legal = evasion_ctx.legal(m);
        assert_eq!(
            evasion_legal,
            full_legal,
            "{} {m}: evasion context",
            board.fen()
        );
        assert_eq!(ctx.legal(m), full_legal, "{} {m}: full", board.fen());

        if let Some(rejected) = predicted_rejection(board, &full, m) {
            stats.in_check_candidates += 1;
            let capture = board.is_capture(m);
            if is_our_commoner(board, m) {
                stats.in_check_commoner_candidates += 1;
            }
            if m.is_castling() {
                stats.in_check_castling_candidates += 1;
            } else if m.is_en_passant() {
                stats.in_check_ep_candidates += 1;
            } else if capture {
                stats.in_check_capture_candidates += 1;
            } else {
                stats.in_check_quiet_candidates += 1;
            }
            if rejected {
                stats.mask_rejections += 1;
                assert!(
                    !full_legal,
                    "{} {m}: mask rejection must be illegal under Board::legal",
                    board.fen()
                );
                assert!(
                    !evasion_legal,
                    "{} {m}: production prefilter must reject",
                    board.fen()
                );
                if full_legal {
                    stats.legal_mask_rejections += 1;
                }
            }
        }
        assert_eq!(
            ctx.legal(m),
            board.legal(m, &counts_only),
            "{} {m}: counts only",
            board.fen()
        );
        assert_eq!(
            ctx.legal(m),
            board.is_legal_with_state(m, &counts_only),
            "{} {m}: is_legal_with_state",
            board.fen()
        );
        if full_legal {
            stats.legal_candidates += 1;
        }
    }

    let mut legal = MoveList::new();
    generate_legal_with_state(board, &full, &mut legal);
    for &m in legal.as_slice() {
        assert!(ctx.legal(m), "{} {m}: legal member", board.fen());
        assert!(
            evasion_ctx.legal(m),
            "{} {m}: evasion legal member",
            board.fen()
        );
    }

    // The context is a snapshot, not a board write.
    assert_eq!(board.fen(), fen_before);
    assert_eq!(board.hash(), hash_before);
}

#[test]
fn corpus_and_fixtures_match_board_legal() {
    let mut stats = Stats::default();
    let mut fens: Vec<String> = common::perft_corpus();
    fens.extend(
        atomic_movegen::CONFORMANCE_CASES
            .iter()
            .map(|c| c.fen.to_owned()),
    );
    fens.extend(
        atomic_movegen::conformance::CHILD_CASES
            .iter()
            .map(|c| c.fen.to_owned()),
    );
    fens.extend(EFFECT_CORPUS.iter().map(|s| (*s).to_owned()));
    fens.extend(BOARD_FIXTURES.iter().map(|s| (*s).to_owned()));
    fens.extend(EDGE_FIXTURES.iter().map(|s| (*s).to_owned()));
    fens.sort();
    fens.dedup();

    for fen in &fens {
        let board = Board::from_fen(fen).expect("legality corpus FEN must parse");
        check_position(&board, &mut stats);
    }

    assert_eq!(stats.positions, fens.len());
    assert!(stats.positions >= 80, "expected the full corpora");
    assert!(
        stats.terminal_positions > 0,
        "corpus must include terminals"
    );
    assert!(stats.in_check_positions > 0, "corpus must include checks");
    assert!(
        stats.slider_block_positions > 0,
        "corpus must include a single slider check with a block ray"
    );
    assert!(
        stats.empty_block_positions > 0,
        "corpus must include an unblockable or double check"
    );
    assert!(
        stats.in_check_quiet_candidates > 0,
        "corpus must include quiet candidates in check"
    );
    assert!(
        stats.mask_rejections > 0,
        "corpus must exercise mask rejections"
    );
    assert_eq!(
        stats.legal_mask_rejections, 0,
        "the corpus must reject no legal move"
    );
    assert!(stats.pinned_positions > 0, "corpus must include pins");
    assert!(stats.ep_positions > 0, "corpus must include ep targets");
    assert!(stats.ep_candidates > 0, "corpus must include ep candidates");
    assert!(
        stats.castling_right_positions > 0,
        "corpus must include castling rights"
    );
    assert!(
        stats.ep_positions > 0,
        "corpus must include en-passant positions"
    );
    assert!(stats.candidates > 1_000, "expected real candidate traffic");
    assert!(stats.legal_candidates > 0);

    eprintln!(
        "legality context corpus: {} positions ({} terminal, {} in check, {} not in check, {} slider-block, {} empty-block, {} pinned, {} ep, {} with rights), {} candidates ({} castling, {} ep), {} legal",
        stats.positions,
        stats.terminal_positions,
        stats.in_check_positions,
        stats.not_in_check_positions,
        stats.slider_block_positions,
        stats.empty_block_positions,
        stats.pinned_positions,
        stats.ep_positions,
        stats.castling_right_positions,
        stats.candidates,
        stats.castling_candidates,
        stats.ep_candidates,
        stats.legal_candidates
    );
    eprintln!(
        "legality context corpus in check: {} candidates ({} quiet, {} capture, {} ep, {} castling, {} commoner), {} predicted mask rejections, {} legal among them",
        stats.in_check_candidates,
        stats.in_check_quiet_candidates,
        stats.in_check_capture_candidates,
        stats.in_check_ep_candidates,
        stats.in_check_castling_candidates,
        stats.in_check_commoner_candidates,
        stats.mask_rejections,
        stats.legal_mask_rejections,
    );
}

#[test]
fn seeded_playouts_match_board_legal() {
    let mut stats = Stats::default();
    let visited = common::walk_playouts(&mut |board, _state| {
        check_position(board, &mut stats);
    });

    assert_eq!(visited as usize, stats.positions);
    assert!(stats.positions > 14_000, "expected a long walk");
    assert!(stats.terminal_positions > 0, "walk must reach terminals");
    assert!(stats.in_check_positions > 0, "walk must reach checks");
    assert!(
        stats.slider_block_positions > 0,
        "walk must reach a single slider check with a block ray"
    );
    assert!(
        stats.empty_block_positions > 0,
        "walk must reach an unblockable or double check"
    );
    assert!(
        stats.in_check_quiet_candidates > 0,
        "walk must reach quiet candidates in check"
    );
    assert!(
        stats.mask_rejections > 0,
        "walk must exercise mask rejections"
    );
    assert_eq!(
        stats.legal_mask_rejections, 0,
        "the walk must reject no legal move"
    );
    assert!(stats.pinned_positions > 0, "walk must reach pins");
    assert!(stats.ep_positions > 0, "walk must reach ep targets");
    assert!(
        stats.castling_right_positions > 0,
        "walk must reach castling rights"
    );
    assert!(
        stats.castling_candidates > 0,
        "walk must reach castling moves"
    );
    assert!(
        stats.candidates > 100_000,
        "expected real candidate traffic"
    );
    assert!(stats.legal_candidates > 50_000);

    eprintln!(
        "legality context play-outs: {} positions ({} terminal, {} in check, {} not in check, {} slider-block, {} empty-block, {} pinned, {} ep, {} with rights), {} candidates ({} castling, {} ep), {} legal",
        stats.positions,
        stats.terminal_positions,
        stats.in_check_positions,
        stats.not_in_check_positions,
        stats.slider_block_positions,
        stats.empty_block_positions,
        stats.pinned_positions,
        stats.ep_positions,
        stats.castling_right_positions,
        stats.candidates,
        stats.castling_candidates,
        stats.ep_candidates,
        stats.legal_candidates
    );
    eprintln!(
        "legality context play-outs in check: {} candidates ({} quiet, {} capture, {} ep, {} castling, {} commoner), {} predicted mask rejections, {} legal among them",
        stats.in_check_candidates,
        stats.in_check_quiet_candidates,
        stats.in_check_capture_candidates,
        stats.in_check_ep_candidates,
        stats.in_check_castling_candidates,
        stats.in_check_commoner_candidates,
        stats.mask_rejections,
        stats.legal_mask_rejections,
    );
}

fn pseudo_moves(board: &Board) -> MoveList {
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(board, &mut pseudo);
    pseudo
}

fn normal_move(board: &Board, from: atomic_movegen::Square, to: atomic_movegen::Square) -> Move {
    *pseudo_moves(board)
        .as_slice()
        .iter()
        .find(|m| {
            m.move_type() == atomic_movegen::types::MoveType::Normal
                && m.from_sq() == from
                && m.to_sq() == to
        })
        .unwrap_or_else(|| panic!("{board:?}: missing normal move {from:?}{to:?}"))
}

fn assert_verdict(board: &Board, state: &StateInfo, m: Move, expected: bool) {
    let ctx = board.legality_context_with_state(board.counts(), state);
    assert_eq!(ctx.occupied(), board.occupied(), "{board:?}: occupancy");
    assert_eq!(ctx.legal(m), expected, "{board:?} {m}: context verdict");
    assert_eq!(
        board.legal(m, state),
        expected,
        "{board:?} {m}: board verdict"
    );
}

#[test]
fn focused_evasion_fixtures() {
    use atomic_movegen::Square;

    // One slider check: g1e2 is inside the e-file block ray, g1f3 is outside
    // it, and the commoner move e1d1 falls through.
    let board = Board::from_fen("4r1k1/8/8/8/8/8/8/4K1N1 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert_eq!(state.checkers.count(), 1);
    assert_eq!(
        predicted_block(&board, &state),
        Some(attacks::between(Square::E1, Square::E8))
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::G1, Square::E2),
        true,
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::G1, Square::F3),
        false,
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::E1, Square::D1),
        true,
    );

    // Double check: no quiet move can answer both checkers.
    let board = Board::from_fen("4k3/8/8/8/1b6/8/4r3/4K1N1 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert_eq!(state.checkers.count(), 2);
    assert_eq!(predicted_block(&board, &state), Some(Bitboard::EMPTY));
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::G1, Square::F3),
        false,
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::E1, Square::D1),
        true,
    );

    // Knight check: a2a3 cannot block a leaper.
    let board = Board::from_fen("4k3/8/8/8/8/5n2/P7/4K3 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert_eq!(state.checkers.count(), 1);
    assert_eq!(predicted_block(&board, &state), Some(Bitboard::EMPTY));
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::A2, Square::A3),
        false,
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::E1, Square::D1),
        true,
    );

    // Pawn check: same leaper reasoning.
    let board = Board::from_fen("4k3/8/8/8/8/8/P2p4/4K3 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert_eq!(state.checkers.count(), 1);
    assert_eq!(predicted_block(&board, &state), Some(Bitboard::EMPTY));
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::A2, Square::A3),
        false,
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::E1, Square::D1),
        true,
    );

    // Touching commoners grant immunity: no checker data and no rejection.
    let board = Board::from_fen("8/8/8/8/8/8/1k6/2K5 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert!(state.checkers.is_empty());
    assert_eq!(predicted_block(&board, &state), None);
    let ctx = board.legality_context_with_state(board.counts(), &state);
    for &m in pseudo_moves(&board).as_slice() {
        assert_eq!(ctx.legal(m), board.legal(m, &state), "{board:?} {m}");
        assert_eq!(predicted_rejection(&board, &state, m), None);
    }

    // Castling while in check: both pseudo-legal castling candidates are
    // rejected by the mask and by the shared body.
    let board = Board::from_fen("r3k2r/8/8/8/8/8/4r3/R3K2R w KQkq - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert!(!state.checkers.is_empty());
    let pseudo = pseudo_moves(&board);
    let castles: Vec<Move> = pseudo
        .as_slice()
        .iter()
        .copied()
        .filter(|m| m.is_castling() && m.from_sq() == Square::E1)
        .collect();
    assert_eq!(castles.len(), 2, "both castling candidates must be present");
    for m in castles {
        assert_eq!(
            predicted_rejection(&board, &state, m),
            Some(false),
            "castling falls through to the shared body"
        );
        assert_verdict(&board, &state, m, false);
    }

    // En passant in check: the mask must let it fall through, and this one is
    // legal because the ep capture removes the checking pawn.
    let board = Board::from_fen("4k3/8/8/3pP3/4K3/8/P7/8 w - d6 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert!(!state.checkers.is_empty());
    let ep = *pseudo_moves(&board)
        .as_slice()
        .iter()
        .find(|m| m.is_en_passant() && m.from_sq() == Square::E5 && m.to_sq() == Square::D6)
        .expect("ep candidate");
    assert_eq!(predicted_rejection(&board, &state, ep), Some(false));
    assert_verdict(&board, &state, ep, true);

    // A promotion push that blocks the check: all four encodings fall through
    // the mask and are legal.
    let board = Board::from_fen("K6r/6P1/4k3/8/8/8/8/8 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert_eq!(state.checkers.count(), 1);
    let promotions: Vec<Move> = pseudo_moves(&board)
        .as_slice()
        .iter()
        .copied()
        .filter(|m| {
            m.move_type() == atomic_movegen::types::MoveType::Promotion
                && m.from_sq() == Square::G7
                && m.to_sq() == Square::G8
        })
        .collect();
    assert_eq!(promotions.len(), 4, "all four promotions must be present");
    for m in promotions {
        assert_eq!(predicted_rejection(&board, &state, m), Some(false));
        assert_verdict(&board, &state, m, true);
    }

    // A quiet move that opens a second line to the commoner. The destination
    // is outside the sole slider's block ray, so the mask rejects it directly.
    let board = Board::from_fen("4r1k1/8/8/8/8/8/8/rN2K3 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::B1, Square::D2),
        false,
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::E1, Square::D1),
        true,
    );

    // The same idea with the destination *inside* the block ray: c1e3 blocks
    // the e8 rook but uncovers the a1 rook along the first rank. The mask
    // accepts it into the shared body, which must reject it.
    let board = Board::from_fen("4r1k1/8/8/8/8/8/8/r1B1K3 w - - 0 1").unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    assert_eq!(state.checkers.count(), 1);
    let block = predicted_block(&board, &state).expect("in check");
    assert!(
        block & Bitboard::square_bb(Square::E3) != Bitboard::EMPTY,
        "c1e3 must land on the block ray"
    );
    assert_eq!(
        predicted_rejection(&board, &state, normal_move(&board, Square::C1, Square::E3)),
        Some(false)
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::C1, Square::E3),
        false,
    );
    assert_verdict(
        &board,
        &state,
        normal_move(&board, Square::E1, Square::D1),
        true,
    );
}

#[test]
fn terminal_context_rejects_every_candidate() {
    for fen in EDGE_FIXTURES {
        let board = Board::from_fen(fen).expect("edge fixture must parse");
        let counts = board.counts();
        let ctx = board.legality_context(counts);
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        if counts.commoners() == 0 || counts.them_commoners() == 0 {
            for &m in pseudo.as_slice() {
                assert!(!ctx.legal(m), "{fen} {m}: terminal context");
            }
        }
        // Rebuilding the context from a stale zero pair is the documented
        // game-over guard, exactly like `Board::legal` with such a state.
        let stale = board.legality_context(Counts::new(0, counts.them_commoners()));
        for &m in pseudo.as_slice() {
            assert!(!stale.legal(m), "{fen} {m}: zero count");
        }
    }
}
