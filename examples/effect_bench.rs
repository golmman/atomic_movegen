//! Microbenchmark for the effect context (`EffectContext`) against the
//! board-writing path, plus the legal move stream against the legal list.
//!
//! Three groups are reported per bench position:
//!
//! 1. per pseudo-legal move: `effect`/`is_legal` against `Board::legal`;
//! 2. per child: `terminal_after` (and `outcome_after`) against cloning the
//!    board and playing the move — both against `terminal()` and against
//!    `generate_legal_with_state`, the two shapes a downstream
//!    "is this child over?" question takes;
//! 3. the **decided share**: the fraction of children answered `Some` over a
//!    seeded play-out walk. This is this crate's own tree, not a downstream
//!    search tree; it exists so the crate publishes what `terminal_after`
//!    decides, not as a claim about any consumer.
//!
//! Usage:
//! ```sh
//! cargo run --release --example effect_bench [repeats] [iterations]
//! ```
//!
//! Each row reports the *median* of `repeats` batches of `iterations` calls, so
//! a single scheduling hiccup cannot be read as a result. Absolute numbers are
//! only meaningful on the machine that produced them; compare rows within one
//! run.

use atomic_movegen::movegen::{
    for_each_legal_with_state, generate_legal_with_state, generate_pseudo_legal,
    has_legal_move_with_state,
};
use atomic_movegen::{Board, MoveList, StateInfo};
use std::hint::black_box;
use std::time::Instant;

/// The downstream `deep-win` position from the handoff plus two perft-corpus
/// middlegames.
const POSITIONS: &[(&str, &str)] = &[
    (
        "deep-win",
        "rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1",
    ),
    (
        "middlegame-13",
        "rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5",
    ),
    (
        "middlegame-21",
        "r1bqkbnr/pppp2pp/2n5/4ppB1/1P1P4/7N/P1P1PPPP/RN1QKB1R b KQkq - 1 4",
    ),
];

fn median(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Run `f` in `repeats` batches of `iterations` calls and return the median
/// nano-seconds per call.
fn bench(mut f: impl FnMut(), repeats: usize, iterations: usize) -> f64 {
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        for _ in 0..iterations {
            f();
        }
        samples.push(start.elapsed().as_nanos() / iterations as u128);
    }
    median(&mut samples) as f64
}

/// Deterministic xorshift64 PRNG for the play-out walk.
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

/// The share of children answered `Some` by `terminal_after` over seeded random
/// play-outs, for pseudo-legal and for legal candidates separately.
///
/// # Panics
///
/// Panics when a start FEN does not parse.
fn decided_share(start_fens: &[&str], seeds: u64, depth: u32) -> (u64, u64, u64, u64) {
    let mut pseudo_children = 0u64;
    let mut pseudo_decided = 0u64;
    let mut legal_children = 0u64;
    let mut legal_decided = 0u64;
    let mut state = StateInfo::new();
    for (i, fen) in start_fens.iter().enumerate() {
        for seed in 0..seeds {
            let mut board = Board::from_fen(fen).expect("play-out FEN must parse");
            let mut rng = Xorshift::new(0x9E37_79B9_7F4A_7C15 ^ ((i as u64) << 32) ^ seed);
            for _ in 0..=depth {
                board.populate_state(&mut state);
                let ctx = board.effect_context_with_state(&state);
                let mut pseudo = MoveList::new();
                generate_pseudo_legal(&board, &mut pseudo);
                for &m in pseudo.as_slice() {
                    let decided = ctx.terminal_after(&board, m).is_some();
                    pseudo_children += 1;
                    pseudo_decided += u64::from(decided);
                    if ctx.is_legal(&board, m) {
                        legal_children += 1;
                        legal_decided += u64::from(decided);
                    }
                }

                let mut legal = MoveList::new();
                generate_legal_with_state(&board, &state, &mut legal);
                if legal.is_empty() {
                    break;
                }
                let m = legal.as_slice()[(rng.next() % legal.len() as u64) as usize];
                board.do_move(m, &mut state);
            }
        }
    }
    (
        pseudo_children,
        pseudo_decided,
        legal_children,
        legal_decided,
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let repeats: usize = args.get(1).map_or(9, |s| s.parse().expect("repeats"));
    let iterations: usize = args.get(2).map_or(2000, |s| s.parse().expect("iterations"));

    println!("effect_bench: median of {repeats} batches x {iterations} calls, ns per call\n");

    for (name, fen) in POSITIONS {
        let board = Board::from_fen(fen).expect("bench FEN must parse");
        let mut state = StateInfo::new();
        board.populate_state(&mut state);
        let ctx = board.effect_context_with_state(&state);

        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&board, &mut pseudo);
        let mut legal = MoveList::new();
        generate_legal_with_state(&board, &state, &mut legal);
        let pseudo = pseudo.as_slice().to_vec();
        let legal_moves = legal.as_slice().to_vec();

        println!(
            "{name}: {fen}\n  {} pseudo-legal, {} legal moves",
            pseudo.len(),
            legal_moves.len()
        );

        let mut i = 0usize;
        let effect_legal = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(ctx.effect(&board, m).is_legal);
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let effect_full = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(ctx.effect(&board, m));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let board_legal = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(board.legal(m, &state));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let outcome_effect = bench(
            || {
                let m = legal_moves[i % legal_moves.len()];
                i += 1;
                black_box(ctx.outcome_after(&board, m));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let outcome_clone = bench(
            || {
                let m = legal_moves[i % legal_moves.len()];
                i += 1;
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box(child.outcome());
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let terminal_after_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(ctx.terminal_after(&board, m));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let terminal_clone_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box(child.terminal());
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let terminal_clone_legal_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                child.populate_state(&mut st);
                let mut list = MoveList::new();
                generate_legal_with_state(&child, &st, &mut list);
                black_box(list.len());
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let has_move_after_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(ctx.has_legal_move_after(&board, m));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let has_move_clone_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                child.populate_state(&mut st);
                black_box(has_legal_move_with_state(&child, &st));
            },
            repeats,
            iterations,
        );
        let generate_legal_ns = bench(
            || {
                let mut list = MoveList::new();
                generate_legal_with_state(&board, &state, &mut list);
                black_box(list.len());
            },
            repeats,
            iterations,
        );
        let stream_legal_ns = bench(
            || {
                let mut count = 0usize;
                let _ = for_each_legal_with_state::<()>(&board, &state, &mut |_| {
                    count += 1;
                    std::ops::ControlFlow::Continue(())
                });
                black_box(count);
            },
            repeats,
            iterations,
        );
        let first_legal_ns = bench(
            || {
                let flow = for_each_legal_with_state::<()>(&board, &state, &mut |_| {
                    std::ops::ControlFlow::Break(())
                });
                black_box(flow.is_break());
            },
            repeats,
            iterations,
        );
        let has_legal_ns = bench(
            || {
                black_box(has_legal_move_with_state(&board, &state));
            },
            repeats,
            iterations,
        );

        println!(
            "  per pseudo-legal move: effect.is_legal {effect_legal:>9.1}  \
             effect.full {effect_full:>9.1}  board.legal {board_legal:>9.1}"
        );
        println!(
            "  per legal move:        outcome_after {outcome_effect:>9.1}  \
             clone+do_move+outcome {outcome_clone:>9.1}"
        );
        println!(
            "  per child (pseudo):    terminal_after {terminal_after_ns:>9.1}  \
             clone+do_move+terminal {terminal_clone_ns:>9.1}  \
             clone+do_move+legal {terminal_clone_legal_ns:>9.1}"
        );
        println!(
            "  has legal move after:  {has_move_after_ns:>9.1}  \
             clone+do_move+has {has_move_clone_ns:>9.1}"
        );
        println!(
            "  per node:              generate_legal {generate_legal_ns:>9.1}  \
             for_each_legal {stream_legal_ns:>9.1}  has_legal_move {has_legal_ns:>9.1}"
        );
        println!(
            "  first legal move only: for_each_legal {first_legal_ns:>4.1}  \
             vs generate_legal {generate_legal_ns:>4.1}"
        );
        println!();
    }

    let mut start_fens: Vec<&str> = POSITIONS.iter().map(|(_, fen)| *fen).collect();
    start_fens.push("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    let (pseudo_children, pseudo_decided, legal_children, legal_decided) =
        decided_share(&start_fens, 8, 60);
    let pct = |decided: u64, total: u64| {
        if total == 0 {
            0.0
        } else {
            100.0 * decided as f64 / total as f64
        }
    };
    println!("decided share over seeded play-outs (8 seeds x depth 60 x 4 start FENs):");
    println!(
        "  pseudo-legal children: {pseudo_children:>8}  decided {pseudo_decided:>8}  \
         ({:.2} %)",
        pct(pseudo_decided, pseudo_children)
    );
    println!(
        "  legal children:        {legal_children:>8}  decided {legal_decided:>8}  \
         ({:.2} %)",
        pct(legal_decided, legal_children)
    );
}
