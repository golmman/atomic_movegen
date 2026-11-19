//! Hot-path cost rows for the wishlist-2 shapes.
//!
//! Plan 1 (W6) introduced this harness with the setup writers and the counts
//! reader; later plans add candidate/child rows with stable names. Each row is
//! the median of repeated passes over a fixed board list, reported in absolute
//! ns per call and as a ratio to the current full-state row of the same run.
//! Absolute ns are only comparable on one machine; the ratio is the gate.
//!
//! Usage:
//! ```sh
//! cargo run --release --example hotpath_bench [repeats] [passes]
//! ```
//!
//! Two datasets are reported:
//!
//! - `corpus`: the 41 positions of `tests/perft_values.md` plus the frozen
//!   conformance FENs (the positions the differential suites use);
//! - `playouts`: every position visited by the seeded play-out walk from
//!   `tests/common/mod.rs` (16 seeds, 300 plies from the six play-out starts,
//!   plus a shorter walk from each corpus position).

use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::types::MoveType;
use atomic_movegen::{
    Board, CONFORMANCE_CASES, Color, Counts, EffectContext, EnPassantMode, LegalityContext, Move,
    MoveList, PieceType, Setup, StateInfo,
};
use std::hint::black_box;
use std::time::Instant;

/// Deterministic xorshift64 PRNG, matching `tests/common/mod.rs`.
struct Xorshift(u64);

impl Xorshift {
    fn new(seed: u64) -> Self {
        Xorshift(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

const PLAYOUT_FENS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1",
    "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
];

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

fn corpus_boards() -> Vec<Board> {
    let mut fens = perft_corpus();
    fens.extend(CONFORMANCE_CASES.iter().map(|case| case.fen.to_owned()));
    fens.iter()
        .map(|fen| Board::from_fen(fen).expect("corpus FEN must parse"))
        .collect()
}

fn playout_boards() -> Vec<Board> {
    let mut fens: Vec<String> = perft_corpus();
    fens.extend(PLAYOUT_FENS.iter().map(|s| (*s).to_owned()));
    let mut boards = Vec::new();
    let mut state = StateInfo::new();
    for fen in &fens {
        for seed in 0..16u64 {
            let mut board = Board::from_fen(fen).expect("play-out FEN must parse");
            let mut rng = Xorshift::new(0x9E37_79B9_7F4A_7C15 ^ seed);
            for _ in 0..=60 {
                boards.push(board.clone());
                board.populate_state(&mut state);
                let mut legal = MoveList::new();
                generate_legal_with_state(&board, &state, &mut legal);
                if legal.is_empty() {
                    break;
                }
                let idx = (rng.next() % legal.len() as u64) as usize;
                board.do_move(legal.as_slice()[idx], &mut state);
            }
        }
    }
    boards
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// Run `f` in `repeats` batches of `passes` full board-list passes and return
/// the median nano-seconds per call (one call is one board in the list).
fn bench(mut f: impl FnMut(), calls: usize, repeats: usize, passes: usize) -> f64 {
    assert!(calls > 0);
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        for _ in 0..passes {
            f();
        }
        let ns = start.elapsed().as_nanos() as f64 / (passes * calls) as f64;
        samples.push(ns);
    }
    median(&mut samples)
}

struct Rows {
    populate_state: f64,
    populate_counts: f64,
    counts: f64,
    new_state: f64,
}

fn measure(boards: &[Board], repeats: usize, passes: usize) -> Rows {
    let mut states = vec![StateInfo::new(); boards.len()];

    let populate_state = bench(
        || {
            for (board, state) in boards.iter().zip(states.iter_mut()) {
                board.populate_state(black_box(state));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let populate_counts = bench(
        || {
            for (board, state) in boards.iter().zip(states.iter_mut()) {
                board.populate_counts(black_box(state));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let counts = bench(
        || {
            for board in boards {
                black_box(board.counts());
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let new_state = bench(
        || {
            for _ in 0..boards.len() {
                black_box(StateInfo::new());
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    Rows {
        populate_state,
        populate_counts,
        counts,
        new_state,
    }
}

fn print_table(name: &str, boards: usize, rows: &Rows) {
    println!();
    println!("{name}: {boards} boards; ns/call (median), ratio to populate_state");
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "populate_state", rows.populate_state, 1.0
    );
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "populate_counts",
        rows.populate_counts,
        rows.populate_counts / rows.populate_state
    );
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "Board::counts",
        rows.counts,
        rows.counts / rows.populate_state
    );
    println!(
        "  {:<20} {:>10.2} ns {:>8.3}x",
        "StateInfo::new",
        rows.new_state,
        rows.new_state / rows.populate_state
    );
}

// --- candidate legality rows (plan 2, W7) -----------------------------------

/// A quiet move: no capture, no castling or en passant, not a promotion, and
/// the origin is not a commoner.
const CLASS_QUIET: u8 = 1 << 0;
/// A capture, including en passant.
const CLASS_CAPTURE: u8 = 1 << 1;
/// An en-passant capture.
const CLASS_EN_PASSANT: u8 = 1 << 2;
/// A castling move.
const CLASS_CASTLING: u8 = 1 << 3;
/// The origin square holds a commoner of the side to move.
const CLASS_COMMONER: u8 = 1 << 4;
/// Every candidate.
const CLASS_ALL: u8 = u8::MAX;
/// A promotion, including a capturing promotion (child rows only).
const CLASS_PROMOTION: u8 = 1 << 5;
/// A normal two-square pawn push (child rows only).
const CLASS_DOUBLE_PUSH: u8 = 1 << 6;

/// One node with its populated state, legality context, and all pseudo-legal
/// candidates classified once. Nothing in a timed candidate row constructs a
/// context or classifies a move.
struct CandidateEntry {
    board: Board,
    state: StateInfo,
    counts: Counts,
    ctx: LegalityContext,
    effect: EffectContext,
    moves: Vec<(u8, Move)>,
    child_moves: Vec<(u8, Move)>,
}

fn class_mask(board: &Board, m: Move) -> u8 {
    let mut mask = 0u8;
    match m.move_type() {
        MoveType::EnPassant => mask |= CLASS_CAPTURE | CLASS_EN_PASSANT,
        MoveType::Castling => mask |= CLASS_CASTLING,
        MoveType::Promotion => {}
        MoveType::Normal if board.is_capture(m) => mask |= CLASS_CAPTURE,
        _ => {}
    }
    let piece = board.piece_on(m.from_sq());
    if piece.type_of() == Some(PieceType::Commoner) && piece.color() == Some(board.side_to_move()) {
        mask |= CLASS_COMMONER;
    }
    if mask == 0 { CLASS_QUIET } else { mask }
}

fn child_class_mask(board: &Board, m: Move) -> u8 {
    let mut mask = class_mask(board, m);
    if m.move_type() == MoveType::Promotion {
        mask &= !CLASS_QUIET;
        mask |= CLASS_PROMOTION;
    }
    if board.is_double_push(m) {
        mask &= !CLASS_QUIET;
        mask |= CLASS_DOUBLE_PUSH;
    }
    mask
}

fn candidate_entries(boards: &[Board]) -> Vec<CandidateEntry> {
    let mut entries = Vec::new();
    let mut state = StateInfo::new();
    let mut pseudo = MoveList::new();
    for board in boards {
        board.populate_state(&mut state);
        let counts = board.counts();
        let ctx = board.legality_context(counts);
        let effect = board.effect_context_with_state(&state);
        pseudo.clear();
        generate_pseudo_legal(board, &mut pseudo);
        if pseudo.is_empty() {
            continue;
        }
        let moves: Vec<_> = pseudo
            .as_slice()
            .iter()
            .map(|&m| (class_mask(board, m), m))
            .collect();
        let child_moves = pseudo
            .as_slice()
            .iter()
            .map(|&m| (child_class_mask(board, m), m))
            .collect();
        entries.push(CandidateEntry {
            board: board.clone(),
            state,
            counts,
            ctx,
            effect,
            moves,
            child_moves,
        });
    }
    entries
}

struct CandidateRows {
    context: f64,
    classes: [usize; 6],
    board_legal: [f64; 6],
    context_legal: [f64; 6],
}

const CANDIDATE_CLASSES: [(&str, u8); 6] = [
    ("quiet", CLASS_QUIET),
    ("capture", CLASS_CAPTURE),
    ("en-passant", CLASS_EN_PASSANT),
    ("castling", CLASS_CASTLING),
    ("commoner-move", CLASS_COMMONER),
    ("all", CLASS_ALL),
];

fn class_calls(entries: &[CandidateEntry], mask: u8) -> Vec<(usize, Move)> {
    entries
        .iter()
        .enumerate()
        .flat_map(|(index, entry)| {
            entry
                .moves
                .iter()
                .filter(|(class, _)| *class & mask != 0)
                .map(move |&(_, m)| (index, m))
        })
        .collect()
}

/// Raise the pass count for a rare class so its row measures at least roughly
/// 100k calls instead of a handful. The all-candidate row keeps the caller's
/// passes.
fn class_passes(calls: usize, repeats: usize, passes: usize) -> usize {
    let target = 100_000 / repeats.max(1) / calls.max(1);
    passes.max(target.max(1))
}

fn bench_class(
    entries: &[CandidateEntry],
    calls: &[(usize, Move)],
    repeats: usize,
    passes: usize,
    context: bool,
) -> f64 {
    let passes = class_passes(calls.len(), repeats, passes);
    bench(
        || {
            for &(index, m) in calls {
                let entry = &entries[index];
                let verdict = if context {
                    entry.ctx.legal(black_box(m))
                } else {
                    entry.board.legal(black_box(m), &entry.state)
                };
                black_box(verdict);
            }
        },
        calls.len(),
        repeats,
        passes,
    )
}

fn measure_candidates(entries: &[CandidateEntry], repeats: usize, passes: usize) -> CandidateRows {
    for entry in entries {
        assert_eq!(entry.ctx.occupied(), entry.board.occupied());
        assert_eq!(entry.ctx.legal(entry.moves[0].1), {
            let m = entry.moves[0].1;
            entry.board.legal(m, &entry.state)
        });
    }

    let context = bench(
        || {
            for entry in entries {
                black_box(entry.board.legality_context(black_box(entry.counts)));
            }
        },
        entries.len(),
        repeats,
        passes,
    );

    let mut classes = [0usize; 6];
    let mut board_legal = [0.0; 6];
    let mut context_legal = [0.0; 6];
    for (index, (_, mask)) in CANDIDATE_CLASSES.iter().enumerate() {
        let calls = class_calls(entries, *mask);
        classes[index] = calls.len();
        if calls.is_empty() {
            continue;
        }
        board_legal[index] = bench_class(entries, &calls, repeats, passes, false);
        context_legal[index] = bench_class(entries, &calls, repeats, passes, true);
    }

    CandidateRows {
        context,
        classes,
        board_legal,
        context_legal,
    }
}

fn print_candidates(name: &str, entries: &[CandidateEntry], rows: &CandidateRows) {
    let candidates: usize = entries.iter().map(|entry| entry.moves.len()).sum();
    println!();
    println!(
        "{name} candidates: {} nodes, {candidates} pseudo-legal candidates",
        entries.len()
    );
    println!(
        "  {:<20} {:>10.2} ns/node",
        "legality_context", rows.context
    );
    println!(
        "  {:<20} {:>10} {:>12} {:>12} {:>9}",
        "class", "calls", "Board::legal", "ctx.legal", "ratio"
    );
    for (index, (label, _)) in CANDIDATE_CLASSES.iter().enumerate() {
        let calls = rows.classes[index];
        if calls == 0 {
            println!("  {label:<20} {calls:>10}");
            continue;
        }
        println!(
            "  {:<20} {:>10} {:>10.2} ns {:>10.2} ns {:>8.3}x",
            label,
            calls,
            rows.board_legal[index],
            rows.context_legal[index],
            rows.context_legal[index] / rows.board_legal[index]
        );
    }
}

// --- child-effect rows (plan 3, W8) ----------------------------------------

const CHILD_CLASSES: [(&str, u8); 8] = [
    ("quiet", CLASS_QUIET),
    ("capture", CLASS_CAPTURE),
    ("promotion", CLASS_PROMOTION),
    ("en-passant", CLASS_EN_PASSANT),
    ("castling", CLASS_CASTLING),
    ("double-push", CLASS_DOUBLE_PUSH),
    ("commoner-move", CLASS_COMMONER),
    ("all", CLASS_ALL),
];

fn child_class_calls(entries: &[CandidateEntry], mask: u8) -> Vec<(usize, Move)> {
    entries
        .iter()
        .enumerate()
        .flat_map(|(index, entry)| {
            entry
                .child_moves
                .iter()
                .filter(|(class, _)| *class & mask != 0)
                .map(move |&(_, m)| (index, m))
        })
        .collect()
}

fn bench_child_calls(
    entries: &[CandidateEntry],
    calls: &[(usize, Move)],
    repeats: usize,
    passes: usize,
    mut f: impl FnMut(&CandidateEntry, Move),
) -> f64 {
    let passes = class_passes(calls.len(), repeats, passes);
    bench(
        || {
            for &(index, m) in calls {
                f(&entries[index], black_box(m));
            }
        },
        calls.len(),
        repeats,
        passes,
    )
}

struct ChildRows {
    classes: [usize; 8],
    effect_always: [f64; 8],
    effect_legal: [f64; 8],
    effect_occupancy: [f64; 8],
    post_always: [f64; 8],
    post_legal: [f64; 8],
    clone_always: [f64; 8],
    clone_legal: [f64; 8],
}

fn measure_children(entries: &[CandidateEntry], repeats: usize, passes: usize) -> ChildRows {
    for entry in entries {
        for &(_, m) in &entry.moves {
            assert_eq!(
                entry
                    .board
                    .child_effect(m, entry.counts, atomic_movegen::EnPassantMode::Always)
                    .map(|effect| effect.key),
                entry
                    .effect
                    .post_move_and_key(&entry.board, m)
                    .map(|(_, key)| key)
            );
        }
    }

    let mut classes = [0usize; 8];
    let mut effect_always = [0.0; 8];
    let mut effect_legal = [0.0; 8];
    let mut effect_occupancy = [0.0; 8];
    let mut post_always = [0.0; 8];
    let mut post_legal = [0.0; 8];
    let mut clone_always = [0.0; 8];
    let mut clone_legal = [0.0; 8];

    for (index, (_, mask)) in CHILD_CLASSES.iter().enumerate() {
        let calls = child_class_calls(entries, *mask);
        classes[index] = calls.len();
        if calls.is_empty() {
            continue;
        }

        effect_always[index] = bench_child_calls(entries, &calls, repeats, passes, |entry, m| {
            black_box(entry.board.child_effect(
                m,
                entry.counts,
                atomic_movegen::EnPassantMode::Always,
            ));
        });
        effect_legal[index] = bench_child_calls(entries, &calls, repeats, passes, |entry, m| {
            black_box(entry.board.child_effect(
                m,
                entry.counts,
                atomic_movegen::EnPassantMode::Legal,
            ));
        });
        effect_occupancy[index] =
            bench_child_calls(entries, &calls, repeats, passes, |entry, m| {
                black_box(entry.board.child_effect_with_occupancy(
                    m,
                    entry.ctx.occupied(),
                    entry.counts,
                    atomic_movegen::EnPassantMode::Always,
                ));
            });
        post_always[index] = bench_child_calls(entries, &calls, repeats, passes, |entry, m| {
            black_box(entry.effect.post_move_and_key_with(
                &entry.board,
                m,
                atomic_movegen::EnPassantMode::Always,
            ));
        });
        post_legal[index] = bench_child_calls(entries, &calls, repeats, passes, |entry, m| {
            black_box(entry.effect.post_move_and_key_with(
                &entry.board,
                m,
                atomic_movegen::EnPassantMode::Legal,
            ));
        });

        let mut state = StateInfo::new();
        clone_always[index] = bench_child_calls(entries, &calls, repeats, passes, |entry, m| {
            let mut child = entry.board.clone();
            child.do_move(m, &mut state);
            black_box(child.hash_with(atomic_movegen::EnPassantMode::Always));
        });
        clone_legal[index] = bench_child_calls(entries, &calls, repeats, passes, |entry, m| {
            let mut child = entry.board.clone();
            child.do_move(m, &mut state);
            black_box(child.hash_with(atomic_movegen::EnPassantMode::Legal));
        });
    }

    ChildRows {
        classes,
        effect_always,
        effect_legal,
        effect_occupancy,
        post_always,
        post_legal,
        clone_always,
        clone_legal,
    }
}

fn print_children(name: &str, entries: &[CandidateEntry], rows: &ChildRows) {
    let candidates: usize = entries.iter().map(|entry| entry.moves.len()).sum();
    let double_pushes = rows.classes[5];
    println!();
    println!(
        "{name} child effects: {} nodes, {candidates} moves, {double_pushes} double pushes ({:.2}%)",
        entries.len(),
        double_pushes as f64 * 100.0 / candidates.max(1) as f64
    );
    println!(
        "  {:<20} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "class",
        "calls",
        "effect-A",
        "effect-L",
        "effect-occ",
        "post-A",
        "post-L",
        "clone-A",
        "clone-L"
    );
    for (index, (label, _)) in CHILD_CLASSES.iter().enumerate() {
        let calls = rows.classes[index];
        if calls == 0 {
            println!("  {label:<20} {calls:>10}");
            continue;
        }
        println!(
            "  {:<20} {:>10} {:>7.2} ns {:>7.2} ns {:>7.2} ns {:>7.2} ns {:>7.2} ns {:>7.2} ns {:>7.2} ns",
            label,
            calls,
            rows.effect_always[index],
            rows.effect_legal[index],
            rows.effect_occupancy[index],
            rows.post_always[index],
            rows.post_legal[index],
            rows.clone_always[index],
            rows.clone_legal[index],
        );
    }
    if rows.classes[7] != 0 {
        println!(
            "  all ratios: effect-A {:.3}x post-A, effect-L {:.3}x post-L, effect-A {:.3}x clone-A, occ {:.3}x effect-A",
            rows.effect_always[7] / rows.post_always[7],
            rows.effect_legal[7] / rows.post_legal[7],
            rows.effect_always[7] / rows.clone_always[7],
            rows.effect_occupancy[7] / rows.effect_always[7],
        );
    }
}

// --- null-move rows (plan 4, W9) -------------------------------------------

/// The downstream adapter shape: copy the pieces into a fresh `Setup`, set the
/// side/rights/ep/clocks, then build. A fresh `Setup` is constructed per call,
/// so this row includes the piece-copy cost and is not a reused-`Setup` lower
/// bound.
fn setup_rebuild_null(board: &Board, attacker: Color) -> Board {
    let mut setup = Setup::new();
    for (square, piece) in board.pieces() {
        setup.place(square, piece);
    }
    setup
        .set_side_to_move(attacker)
        .set_castling_rights(board.castling_rights())
        .set_ep_square(None)
        .set_rule50(board.rule50().saturating_add(1))
        .set_game_ply(board.game_ply().saturating_add(1));
    Board::from_setup(&setup).expect("null setup must build")
}

struct NullRows {
    clone: f64,
    null_for: f64,
    advanced: f64,
    advanced_same_side: f64,
    setup_rebuild: f64,
}

fn measure_nulls(boards: &[Board], repeats: usize, passes: usize) -> NullRows {
    for board in boards {
        let same_side = board.side_to_move();
        let flipped = same_side.flip();
        for attacker in [flipped, same_side] {
            let advanced = board.null_for_advanced(attacker);
            let rebuilt = setup_rebuild_null(board, attacker);
            assert_eq!(advanced.hash(), rebuilt.hash());
            assert_eq!(
                advanced.hash_with(EnPassantMode::Legal),
                rebuilt.hash_with(EnPassantMode::Legal)
            );
            assert_eq!(advanced.rule50(), rebuilt.rule50());
            assert_eq!(advanced.game_ply(), rebuilt.game_ply());
            if board.game_ply() < u16::MAX {
                assert_eq!(advanced.fen(), rebuilt.fen());
            }
        }
    }

    let clone = bench(
        || {
            for board in boards {
                black_box(board.clone());
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let null_for = bench(
        || {
            for board in boards {
                black_box(board.null_for(black_box(board.side_to_move().flip())));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let advanced = bench(
        || {
            for board in boards {
                black_box(board.null_for_advanced(black_box(board.side_to_move().flip())));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let advanced_same_side = bench(
        || {
            for board in boards {
                black_box(board.null_for_advanced(black_box(board.side_to_move())));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    let setup_rebuild = bench(
        || {
            for board in boards {
                black_box(setup_rebuild_null(
                    board,
                    black_box(board.side_to_move().flip()),
                ));
            }
        },
        boards.len(),
        repeats,
        passes,
    );

    NullRows {
        clone,
        null_for,
        advanced,
        advanced_same_side,
        setup_rebuild,
    }
}

fn print_nulls(name: &str, boards: usize, rows: &NullRows) {
    println!();
    println!("{name} null moves: {boards} boards; ns/call (median), ratio to null_for_advanced");
    for (label, value) in [
        ("clone", rows.clone),
        ("null_for", rows.null_for),
        ("null_for_advanced", rows.advanced),
        ("advanced (same side)", rows.advanced_same_side),
        ("Setup rebuild", rows.setup_rebuild),
    ] {
        println!(
            "  {label:<20} {value:>10.2} ns {:>8.3}x",
            value / rows.advanced
        );
    }
    println!(
        "  ratios: advanced/null_for {:.3}x, advanced/Setup {:.3}x, advanced/clone {:.3}x",
        rows.advanced / rows.null_for,
        rows.advanced / rows.setup_rebuild,
        rows.advanced / rows.clone,
    );
}

// --- wishlist 2 release summary (plan 5) -----------------------------------

/// One compact release row built from the per-class tables above. `old` is the
/// 2.9.0 shape in the same run; the ratio is the release gate.
fn print_summary(
    name: &str,
    setup: &Rows,
    candidates: &CandidateRows,
    children: &ChildRows,
    nulls: &NullRows,
) {
    println!();
    println!("wishlist 2 summary — {name}: ns/call (median), ratio to the current 2.9.0 shape");
    println!(
        "  {:<42} {:>10} {:>10} {:>8}",
        "row (new / old)", "new", "old", "ratio"
    );
    let rows = [
        (
            "setup: populate_counts / populate_state",
            setup.populate_counts,
            setup.populate_state,
        ),
        (
            "legality: ctx.legal / Board::legal",
            candidates.context_legal[5],
            candidates.board_legal[5],
        ),
        (
            "child-A: child_effect / post_move_and_key",
            children.effect_always[7],
            children.post_always[7],
        ),
        (
            "child-L: child_effect / post_move_and_key",
            children.effect_legal[7],
            children.post_legal[7],
        ),
        (
            "null: null_for_advanced / null_for",
            nulls.advanced,
            nulls.null_for,
        ),
        (
            "null: null_for_advanced / Setup rebuild",
            nulls.advanced,
            nulls.setup_rebuild,
        ),
    ];
    for (label, new, old) in rows {
        println!(
            "  {label:<42} {new:>7.2} ns {old:>7.2} ns {:>7.3}x",
            new / old
        );
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let repeats: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(9).max(3);
    let passes: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(8).max(1);

    println!("hotpath_bench: repeats={repeats}, passes={passes} (median per call)");

    let corpus = corpus_boards();
    let corpus_rows = measure(&corpus, repeats, passes);
    print_table("corpus", corpus.len(), &corpus_rows);

    let playouts = playout_boards();
    let playout_rows = measure(&playouts, repeats, passes);
    print_table("playouts", playouts.len(), &playout_rows);

    let corpus_candidates = candidate_entries(&corpus);
    let corpus_candidate_rows = measure_candidates(&corpus_candidates, repeats, passes);
    print_candidates("corpus", &corpus_candidates, &corpus_candidate_rows);

    let playout_candidates = candidate_entries(&playouts);
    let playout_candidate_rows = measure_candidates(&playout_candidates, repeats, passes);
    print_candidates("playouts", &playout_candidates, &playout_candidate_rows);

    let corpus_child_rows = measure_children(&corpus_candidates, repeats, passes);
    print_children("corpus", &corpus_candidates, &corpus_child_rows);

    let playout_child_rows = measure_children(&playout_candidates, repeats, passes);
    print_children("playouts", &playout_candidates, &playout_child_rows);

    let corpus_null_rows = measure_nulls(&corpus, repeats, passes);
    print_nulls("corpus", corpus.len(), &corpus_null_rows);

    let playout_null_rows = measure_nulls(&playouts, repeats, passes);
    print_nulls("playouts", playouts.len(), &playout_null_rows);

    print_summary(
        "corpus",
        &corpus_rows,
        &corpus_candidate_rows,
        &corpus_child_rows,
        &corpus_null_rows,
    );
    print_summary(
        "playouts",
        &playout_rows,
        &playout_candidate_rows,
        &playout_child_rows,
        &playout_null_rows,
    );
}
