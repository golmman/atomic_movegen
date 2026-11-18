//! Microbenchmark for the effect context (`EffectContext`) against the
//! board-writing path, plus the legal move stream against the legal list.
//!
//! Three groups are reported per bench position:
//!
//! 1. per pseudo-legal move: `effect`/`is_legal` against `Board::legal`,
//!    `classification` (the effect path without the legality ladder) against
//!    `effect` and against the legal-list loop it is meant for, and
//!    `child_outcome` (the two-level answer) against the split
//!    `is_legal` + `terminal_after` pair it replaces;
//! 2. per child: `terminal_after` (and `outcome_after`) against cloning the
//!    board and playing the move — both against `terminal()` and against
//!    `generate_legal_with_state`, the two shapes a downstream
//!    "is this child over?" question takes, plus the `PostMove` summary against
//!    cloning and reading the same facts off the child board, plus the MG19
//!    composites: `post_move` + `PostMove::terminal()` (summary + terminality
//!    from one derivation) and `post_move_and_key` (summary + key from one
//!    derivation);
//! 3. the **decided share**: the fraction of children answered `Some` over a
//!    seeded play-out walk, with the number of children per child class. This
//!    is this crate's own tree, not a downstream search tree; it exists so the
//!    crate publishes what `terminal_after` decides, not as a claim about any
//!    consumer;
//! 4. the **per-class child table** (handoff 3's table): the same child rows,
//!    split by child class — `quiet`, `capture`, `promotion`, `en-passant`,
//!    `castling`, `commoner-move`, the aggregate `built-ish` (everything that
//!    is not `quiet`) and `all` — over the pseudo-legal children of seeded
//!    play-outs from the 41-position perft corpus (`tests/perft_values.md`)
//!    and the three bench positions. Every row is reported in ns per child
//!    *and* as a ratio to that class's build-only row of the same run, because
//!    the acceptance bars the table exists for are ratios: absolute ns are
//!    only comparable on one machine.
//!
//! Usage:
//! ```sh
//! cargo run --release --example effect_bench [repeats] [iterations] [budget-ms]
//! ```
//!
//! Each row reports the *median* of `repeats` batches of `iterations` calls, so
//! a single scheduling hiccup cannot be read as a result. Class rows use fewer
//! iterations for classes whose children are few, so that the whole section
//! stays inside `budget-ms` (default 4000); the iterations used and the time
//! spent are printed with the table. A small class also cycles a shorter child
//! list, which keeps its boards hot in cache — class rows are comparable to
//! each other on the same class, less so across classes.

use atomic_movegen::movegen::{
    for_each_legal_with_state, generate_legal_with_state, generate_pseudo_legal,
    has_legal_move_with_state,
};
use atomic_movegen::types::MoveType;
use atomic_movegen::{Board, EffectContext, EnPassantMode, Move, MoveList, PieceType, StateInfo};
use std::fs;
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

// --- child classes (the handoff 3 table) -------------------------------------

/// `quiet`: no casualties, not castling, not en passant, not a promotion, and
/// the origin is not a commoner.
const CLASS_QUIET: u8 = 1 << 0;
/// `capture`: casualties non-empty and not an en-passant capture.
const CLASS_CAPTURE: u8 = 1 << 1;
/// `promotion`: [`MoveType::Promotion`], quiet or capturing.
const CLASS_PROMOTION: u8 = 1 << 2;
/// `en-passant`: [`MoveType::EnPassant`].
const CLASS_EN_PASSANT: u8 = 1 << 3;
/// `castling`: [`MoveType::Castling`].
const CLASS_CASTLING: u8 = 1 << 4;
/// `commoner-move`: the origin square holds a commoner of the side to move.
const CLASS_COMMONER: u8 = 1 << 5;

/// `built-ish`, handoff 3's definition: every class that is not `quiet`.
const CLASS_BUILT_ISH: u8 =
    CLASS_CAPTURE | CLASS_PROMOTION | CLASS_EN_PASSANT | CLASS_CASTLING | CLASS_COMMONER;

/// Every child, quiet or not.
const CLASS_ALL: u8 = u8::MAX;

/// The primitive classes, in reported order, with their labels. A move is in
/// none of these exactly when it is `quiet`; otherwise it is in one or more
/// (a capture by a commoner is in `capture` *and* `commoner-move`).
const PRIMITIVE_CLASSES: &[(&str, u8)] = &[
    ("quiet", CLASS_QUIET),
    ("capture", CLASS_CAPTURE),
    ("promotion", CLASS_PROMOTION),
    ("en-passant", CLASS_EN_PASSANT),
    ("castling", CLASS_CASTLING),
    ("commoner-move", CLASS_COMMONER),
];

/// The classes of the per-class table, in reported order. A move can be in
/// several of the first six (a capturing promotion is in `capture` *and*
/// `promotion`); `built-ish` is their union and `all` is everything.
const CHILD_CLASSES: &[(&str, u8)] = &[
    ("quiet", CLASS_QUIET),
    ("capture", CLASS_CAPTURE),
    ("promotion", CLASS_PROMOTION),
    ("en-passant", CLASS_EN_PASSANT),
    ("castling", CLASS_CASTLING),
    ("commoner-move", CLASS_COMMONER),
    ("built-ish", CLASS_BUILT_ISH),
    ("all", CLASS_ALL),
];

/// The class membership of the child `m` of `board`.
///
/// Classified once per child while the child lists are materialised, never in a
/// benchmark loop.
fn class_mask(board: &Board, m: Move) -> u8 {
    let mut mask = 0u8;
    match m.move_type() {
        MoveType::Promotion => mask |= CLASS_PROMOTION,
        MoveType::EnPassant => mask |= CLASS_EN_PASSANT,
        MoveType::Castling => mask |= CLASS_CASTLING,
        _ => {}
    }
    if !board.blast_casualties(m).is_empty() && m.move_type() != MoveType::EnPassant {
        mask |= CLASS_CAPTURE;
    }
    let piece = board.piece_on(m.from_sq());
    if piece.type_of() == Some(PieceType::Commoner) && piece.color() == Some(board.side_to_move()) {
        mask |= CLASS_COMMONER;
    }
    if mask == 0 { CLASS_QUIET } else { mask }
}

/// `true` when `m` is the normal double pawn push that stores an ep target.
fn is_double_push(board: &Board, m: Move) -> bool {
    m.move_type() == MoveType::Normal
        && (m.to_sq() as i8 - m.from_sq() as i8).abs() == 16
        && board
            .piece_on(m.from_sq())
            .type_of()
            .is_some_and(|pt| pt == PieceType::Pawn)
}

/// One visited position with its effect context and every pseudo-legal child,
/// classified once so the benchmark loop does no classification work.
struct NodeEntry {
    board: Board,
    ctx: EffectContext,
    children: Vec<(Move, u8)>,
}

/// Visit the nodes of seeded play-outs from `start_fens` and collect their
/// pseudo-legal children.
fn collect_nodes(start_fens: &[String], seeds: u64, plies: u32) -> Vec<NodeEntry> {
    let mut nodes = Vec::new();
    let mut state = StateInfo::new();
    let mut pseudo = MoveList::new();
    let mut legal = MoveList::new();
    for (i, fen) in start_fens.iter().enumerate() {
        for seed in 0..seeds {
            let mut board = Board::from_fen(fen).expect("play-out FEN must parse");
            let mut rng = Xorshift::new(0x9E37_79B9_7F4A_7C15 ^ ((i as u64) << 32) ^ seed);
            for _ in 0..=plies {
                board.populate_state(&mut state);
                let ctx = board.effect_context_with_state(&state);
                pseudo.clear();
                generate_pseudo_legal(&board, &mut pseudo);
                if pseudo.is_empty() {
                    break;
                }
                let children = pseudo
                    .as_slice()
                    .iter()
                    .map(|&m| (m, class_mask(&board, m)))
                    .collect();
                nodes.push(NodeEntry {
                    board: board.clone(),
                    ctx,
                    children,
                });
                legal.clear();
                generate_legal_with_state(&board, &state, &mut legal);
                if legal.is_empty() {
                    break;
                }
                let m = legal.as_slice()[(rng.next() % legal.len() as u64) as usize];
                board.do_move(m, &mut state);
            }
        }
    }
    nodes
}

/// The FENs of the `tests/perft_values.md` corpus (every row's last backticked
/// field). A missing file is reported and yields the bench positions only.
fn perft_corpus_fens(path: &str) -> Vec<String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!(
                "note: could not read `{path}` ({e}); class rows use the bench positions only"
            );
            return Vec::new();
        }
    };
    let mut fens = Vec::new();
    for line in content.lines() {
        if let Some(close) = line.rfind('`') {
            if let Some(open) = line[..close].rfind('`') {
                let fen = &line[open + 1..close];
                if fen.contains('/') && fen.contains(' ') {
                    fens.push(fen.to_string());
                }
            }
        }
    }
    fens
}

/// The child rows of the handoff 3 table, in reported order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Today,
    Build,
    Mg15,
    Summary,
    Mg19,
    Stack,
    Key,
    KeyAlways,
    KeyLegal,
    CloneHashLegal,
    Exist,
    TwoLevel,
}

impl Row {
    const ALL: [Row; 12] = [
        Row::Today,
        Row::Build,
        Row::Mg15,
        Row::Summary,
        Row::Mg19,
        Row::Stack,
        Row::Key,
        Row::KeyAlways,
        Row::KeyLegal,
        Row::CloneHashLegal,
        Row::Exist,
        Row::TwoLevel,
    ];

    /// The label the table prints.
    fn label(self) -> &'static str {
        match self {
            Row::Today => "today",
            Row::Build => "build",
            Row::Mg15 => "mg15",
            Row::Summary => "summary",
            Row::Mg19 => "mg19",
            Row::Stack => "stack",
            Row::Key => "key",
            Row::KeyAlways => "key-always",
            Row::KeyLegal => "key-legal",
            Row::CloneHashLegal => "clone-hash-legal",
            Row::Exist => "exist",
            Row::TwoLevel => "two-level",
        }
    }

    /// The work the row does for one child.
    fn run(self, node: &NodeEntry, m: Move) {
        let board = &node.board;
        let ctx = &node.ctx;
        match self {
            // The downstream's baseline: build the child, read its outcome and
            // its key.
            Row::Today => {
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box((child.outcome(), board.hash_and_rule50_after(m)));
            }
            // The divisor of every ratio: the child build with nothing read.
            Row::Build => {
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box(&child);
            }
            // MG15: the key and the terminal state from two effect queries.
            Row::Mg15 => {
                black_box((board.hash_and_rule50_after(m), ctx.terminal_after(board, m)));
            }
            // The summary alone.
            Row::Summary => {
                black_box(ctx.post_move(board, m));
            }
            // MG19: the terminal state of the summary the row just derived.
            Row::Mg19 => {
                if let Some(post) = ctx.post_move(board, m) {
                    black_box(post.terminal());
                }
            }
            // MG19 + MG20 as the downstream would use them: summary + key from
            // one derivation, terminality from the summary.
            Row::Stack => {
                if let Some((post, key)) = ctx.post_move_and_key(board, m) {
                    black_box((post.terminal(), key));
                }
            }
            // The existing keyed summary alone, as the baseline for the
            // mode-aware rows.
            Row::Key => {
                if let Some((post, key)) = ctx.post_move_and_key(board, m) {
                    black_box((post.rule50(), key));
                }
            }
            // The mode-aware query's `Always` spelling: the same key and
            // summary from the same one derivation.
            Row::KeyAlways => {
                if let Some((post, key)) =
                    ctx.post_move_and_key_with(board, m, EnPassantMode::Always)
                {
                    black_box((post.rule50(), key));
                }
            }
            // The canonicalized child key from the child view.
            Row::KeyLegal => {
                if let Some((post, key)) =
                    ctx.post_move_and_key_with(board, m, EnPassantMode::Legal)
                {
                    black_box((post.rule50(), key));
                }
            }
            // The clone oracle the Legal row is compared against on double
            // pushes.
            Row::CloneHashLegal => {
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box((child.rule50(), child.hash_with(EnPassantMode::Legal)));
            }
            // MG20: the child's existence question alone.
            Row::Exist => {
                black_box(ctx.has_legal_move_after(board, m));
            }
            // The two-level child answer.
            Row::TwoLevel => {
                black_box(ctx.child_outcome(board, m));
            }
        }
    }
}

/// Run `row` over `entries` in one batch of `iterations` calls and return the
/// ns per call with sub-nanosecond resolution.
fn batch_row(row: Row, nodes: &[NodeEntry], entries: &[(usize, Move)], iterations: usize) -> f64 {
    let start = Instant::now();
    for i in 0..iterations {
        let (ni, m) = entries[i % entries.len()];
        row.run(&nodes[ni], m);
    }
    start.elapsed().as_nanos() as f64 / iterations as f64
}

/// Print the per-class child table: every row in ns per child and as a ratio to
/// the class's build-only row of the same run.
fn per_class_table(nodes: &[NodeEntry], repeats: usize, iterations: usize, budget_ms: f64) {
    let total_children: usize = nodes.iter().map(|n| n.children.len()).sum();
    println!(
        "per child, by class ({} children over {} nodes; empty classes are skipped):",
        total_children,
        nodes.len()
    );
    let rows = Row::ALL.len() as f64;
    let per_class_budget_ns = budget_ms * 1e6 / CHILD_CLASSES.len() as f64;
    let mut spent = 0.0f64;
    let mut used_iterations = Vec::new();
    for (name, class) in CHILD_CLASSES {
        let mut entries: Vec<(usize, Move)> = Vec::new();
        for (ni, node) in nodes.iter().enumerate() {
            for &(m, mask) in &node.children {
                if mask & class != 0 {
                    entries.push((ni, m));
                }
            }
        }
        if entries.is_empty() {
            println!("  {name}: no children");
            continue;
        }
        // Size the batches to the budget: calibrate on the cheapest row, then
        // give every class the same share of the budget.
        let calibration = batch_row(Row::Build, nodes, &entries, 50).max(0.1);
        let iters = ((per_class_budget_ns / (repeats as f64 * rows * calibration)) as usize)
            .clamp(100, iterations.min(200_000));
        used_iterations.push(iters);
        let start = Instant::now();
        // Interleave the rows within a round so a slow patch of the machine
        // biases every row of the class alike; the median across rounds is the
        // reported sample.
        let mut samples: Vec<Vec<f64>> = vec![Vec::with_capacity(repeats); Row::ALL.len()];
        for _ in 0..repeats {
            for (ri, row) in Row::ALL.iter().enumerate() {
                samples[ri].push(batch_row(*row, nodes, &entries, iters));
            }
        }
        spent += start.elapsed().as_secs_f64() * 1e3;
        let times: Vec<f64> = samples
            .iter_mut()
            .map(|s| {
                s.sort_by(f64::total_cmp);
                s[s.len() / 2]
            })
            .collect();
        let build = times[Row::ALL.iter().position(|&r| r == Row::Build).unwrap()];
        println!("  {name}: children {}", entries.len());
        for (row, ns) in Row::ALL.iter().zip(&times) {
            let ratio = ns / build;
            println!(
                "    {:<9} {:<42} {ns:>7.1}  ({ratio:>4.2}x build)",
                row.label(),
                row.description()
            );
        }
    }
    let (min_iters, max_iters) = used_iterations
        .iter()
        .fold((usize::MAX, 0usize), |(lo, hi), &i| (lo.min(i), hi.max(i)));
    println!(
        "  class rows: {repeats} rounds x {min_iters}..={max_iters} iterations per row, \
         {spent:.0} ms spent (budget {budget_ms:.0} ms)\n"
    );
}

impl Row {
    /// The work description the table prints.
    fn description(self) -> &'static str {
        match self {
            Row::Today => "clone+do_move+outcome+key",
            Row::Build => "clone+do_move",
            Row::Mg15 => "hash_and_rule50_after+terminal_after",
            Row::Summary => "post_move",
            Row::Mg19 => "post_move+PostMove::terminal()",
            Row::Stack => "post_move_and_key+terminal()",
            Row::Key => "post_move_and_key",
            Row::KeyAlways => "post_move_and_key_with(Always)",
            Row::KeyLegal => "post_move_and_key_with(Legal)",
            Row::CloneHashLegal => "clone+do_move+hash_with(Legal)",
            Row::Exist => "has_legal_move_after",
            Row::TwoLevel => "child_outcome",
        }
    }
}

/// The double-push share of the child corpus and the same rows restricted to
/// double pushes.
///
/// The counts are the wishlist's decisive experiment: how often a child can
/// carry an ep target, and how often `EnPassantMode::Legal` actually removes
/// the ep-file key because no fully legal capture exists.
fn double_push_table(nodes: &[NodeEntry], repeats: usize, iterations: usize) {
    let total_children: usize = nodes.iter().map(|n| n.children.len()).sum();
    let mut entries: Vec<(usize, Move)> = Vec::new();
    let mut stored = 0u64;
    let mut legal = 0u64;
    let mut removed = 0u64;
    for (ni, node) in nodes.iter().enumerate() {
        for &(m, _) in &node.children {
            if !is_double_push(&node.board, m) {
                continue;
            }
            entries.push((ni, m));
            let mut child = node.board.clone();
            let mut st = StateInfo::new();
            child.do_move(m, &mut st);
            if child.ep_square().is_some() {
                stored += 1;
                if child.legal_ep_square().is_some() {
                    legal += 1;
                } else {
                    removed += 1;
                }
            }
        }
    }

    let pct = |part: u64| {
        if total_children == 0 {
            0.0
        } else {
            100.0 * part as f64 / total_children as f64
        }
    };
    println!(
        "double pushes: {} of {} children ({:.2} %); {} store an ep target \
         ({} legal, {} removed)",
        entries.len(),
        total_children,
        pct(entries.len() as u64),
        stored,
        legal,
        removed
    );
    if entries.is_empty() {
        println!();
        return;
    }

    let rows = [
        Row::Build,
        Row::Key,
        Row::KeyAlways,
        Row::KeyLegal,
        Row::CloneHashLegal,
    ];
    let iters = iterations.clamp(200, 200_000);
    let mut samples: Vec<Vec<f64>> = vec![Vec::with_capacity(repeats); rows.len()];
    for _ in 0..repeats {
        for (ri, row) in rows.iter().enumerate() {
            samples[ri].push(batch_row(*row, nodes, &entries, iters));
        }
    }
    let times: Vec<f64> = samples
        .iter_mut()
        .map(|s| {
            s.sort_by(f64::total_cmp);
            s[s.len() / 2]
        })
        .collect();
    let key = times[rows.iter().position(|&r| r == Row::Key).unwrap()];
    let build = times[rows.iter().position(|&r| r == Row::Build).unwrap()];
    println!(
        "double-push children only: {} entries, {repeats} rounds x {iters} iterations per row",
        entries.len()
    );
    for (row, ns) in rows.iter().zip(&times) {
        println!(
            "    {:<18} {:<38} {ns:>7.1} ns  ({:.2}x key, {:.2}x build)",
            row.label(),
            row.description(),
            ns / key,
            ns / build
        );
    }
    println!();
}

/// The share of children answered `Some` by `terminal_after` over seeded random
/// play-outs, for pseudo-legal and for legal candidates separately, plus the
/// number of pseudo-legal children of each class.
///
/// # Panics
///
/// Panics when a start FEN does not parse.
#[allow(clippy::type_complexity)]
fn decided_share(start_fens: &[&str], seeds: u64, depth: u32) -> (u64, u64, u64, u64, [u64; 6]) {
    let mut pseudo_children = 0u64;
    let mut pseudo_decided = 0u64;
    let mut legal_children = 0u64;
    let mut legal_decided = 0u64;
    let mut class_children = [0u64; 6];
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
                    let mask = class_mask(&board, m);
                    for (i, (_, class)) in PRIMITIVE_CLASSES.iter().enumerate() {
                        if mask & class != 0 {
                            class_children[i] += 1;
                        }
                    }
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
        class_children,
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let repeats: usize = args.get(1).map_or(9, |s| s.parse().expect("repeats"));
    let iterations: usize = args.get(2).map_or(2000, |s| s.parse().expect("iterations"));
    let budget_ms: f64 = args
        .get(3)
        .map_or(4000.0, |s| s.parse().expect("budget-ms"));

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
        let classification_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(ctx.classification(&board, m));
            },
            repeats,
            iterations,
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
        let mut i = 0usize;
        let child_outcome_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(ctx.child_outcome(&board, m));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let split_answer_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                if ctx.is_legal(&board, m) {
                    black_box(ctx.terminal_after(&board, m));
                }
            },
            repeats,
            iterations,
        );
        // The divisor of the MG20 cost bars: the child build alone, with no
        // fact read at all.
        let mut i = 0usize;
        let build_only_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box(&child);
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let post_move_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                if let Some(post) = ctx.post_move(&board, m) {
                    black_box((
                        post.occupied(),
                        post.rule50(),
                        post.ep_square(),
                        post.castling_rights(),
                        post.casualties(),
                    ));
                }
            },
            repeats,
            iterations,
        );
        // The same derivation with nothing read from it: the row the MG20
        // shares are measured against (the `post_move` row above also reads
        // five facts off the summary).
        let mut i = 0usize;
        let post_derivation_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(ctx.post_move(&board, m));
            },
            repeats,
            iterations,
        );
        // MG19a: the terminal state of an already derived summary.
        let mut i = 0usize;
        let post_terminal_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                if let Some(post) = ctx.post_move(&board, m) {
                    black_box(post.terminal());
                }
            },
            repeats,
            iterations,
        );
        // MG19b: summary + key from one derivation, against the key alone.
        let mut i = 0usize;
        let post_key_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                if let Some((post, key)) = ctx.post_move_and_key(&board, m) {
                    black_box((post.rule50(), key));
                }
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let post_key_always_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                if let Some((post, key)) =
                    ctx.post_move_and_key_with(&board, m, EnPassantMode::Always)
                {
                    black_box((post.rule50(), key));
                }
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let post_key_legal_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                if let Some((post, key)) =
                    ctx.post_move_and_key_with(&board, m, EnPassantMode::Legal)
                {
                    black_box((post.rule50(), key));
                }
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let clone_hash_legal_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box((child.rule50(), child.hash_with(EnPassantMode::Legal)));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let post_and_key_separate_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                if let Some(post) = ctx.post_move(&board, m) {
                    black_box((post.rule50(), board.hash_after(m)));
                }
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let hash_after_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                black_box(board.hash_after(m));
            },
            repeats,
            iterations,
        );
        let mut i = 0usize;
        let post_clone_ns = bench(
            || {
                let m = pseudo[i % pseudo.len()];
                i += 1;
                let mut child = board.clone();
                let mut st = StateInfo::new();
                child.do_move(m, &mut st);
                black_box((
                    child.occupied(),
                    child.rule50(),
                    child.ep_square(),
                    child.castling_rights(),
                    child.blast_casualties(m),
                ));
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
        // The legal-list consumer loop MG18 is about: generate the legal list,
        // then classify every move — once with `effect` (the ladder runs
        // again) and once with `classification` (it does not).
        let classify_after_generate_ns = bench(
            || {
                let mut list = MoveList::new();
                generate_legal_with_state(&board, &state, &mut list);
                let mut captures = 0usize;
                for &m in list.as_slice() {
                    captures += usize::from(ctx.classification(&board, m).is_capture);
                }
                black_box(captures);
            },
            repeats,
            iterations,
        );
        let effect_after_generate_ns = bench(
            || {
                let mut list = MoveList::new();
                generate_legal_with_state(&board, &state, &mut list);
                let mut captures = 0usize;
                for &m in list.as_slice() {
                    captures += usize::from(ctx.effect(&board, m).is_capture);
                }
                black_box(captures);
            },
            repeats,
            iterations,
        );
        let generate_only_ns = bench(
            || {
                let mut list = MoveList::new();
                generate_legal_with_state(&board, &state, &mut list);
                black_box(list.len());
            },
            repeats,
            iterations,
        );

        let existential_share = has_move_after_ns - post_derivation_ns;
        let terminality_share = post_terminal_ns - post_derivation_ns;
        let existential_ratio = existential_share / build_only_ns;
        let terminality_ratio = terminality_share / build_only_ns;
        println!(
            "  per pseudo-legal move: classification {classification_ns:>9.1}  \
             effect.full {effect_full:>9.1}  effect.is_legal {effect_legal:>9.1}  \
             board.legal {board_legal:>9.1}"
        );
        println!(
            "  legal list + classify: classify {classify_after_generate_ns:>9.1}  \
             effect {effect_after_generate_ns:>9.1}  generate only {generate_only_ns:>9.1}"
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
            "  two-level child:       child_outcome {child_outcome_ns:>9.1}  \
             is_legal+terminal_after {split_answer_ns:>9.1}"
        );
        println!(
            "  child summary:         post_move {post_move_ns:>9.1}  \
             post_move.derivation {post_derivation_ns:>9.1}  \
             clone+do_move+facts {post_clone_ns:>9.1}  \
             clone+do_move {build_only_ns:>9.1}"
        );
        println!(
            "  summary + terminal:    post_move+terminal {post_terminal_ns:>9.1}  \
             terminal_after {terminal_after_ns:>9.1}"
        );
        // The MG20 bars, as ratios of this run: the existence question alone
        // (`has_legal_move_after - post_move.derivation`) and the terminality
        // alone (`post_move+terminal - post_move.derivation`) against the
        // build-only row.
        println!(
            "  shares vs build only:  existential {existential_share:>9.1} \
             ({existential_ratio:>4.2}x)  terminality {terminality_share:>9.1} \
             ({terminality_ratio:>4.2}x)"
        );
        println!(
            "  summary + key:         post_move_and_key {post_key_ns:>9.1}  \
             post_move+hash_after {post_and_key_separate_ns:>9.1}  \
             hash_after {hash_after_ns:>9.1}"
        );
        println!(
            "  key by ep mode:        with(Always) {post_key_always_ns:>9.1}  \
             with(Legal) {post_key_legal_ns:>9.1}  \
             clone+do_move+hash_with(Legal) {clone_hash_legal_ns:>9.1}"
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
    let (pseudo_children, pseudo_decided, legal_children, legal_decided, class_children) =
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
    println!("  pseudo-legal children by class:");
    for (i, (name, _)) in PRIMITIVE_CLASSES.iter().enumerate() {
        println!(
            "    {:<14} {:>8}  ({:.1} %)",
            name,
            class_children[i],
            pct(class_children[i], pseudo_children)
        );
    }
    println!(
        "    {:<14} {:>8}  ({:.1} %)",
        "built-ish",
        pseudo_children - class_children[0],
        pct(pseudo_children - class_children[0], pseudo_children)
    );
    println!();

    // Handoff 3's table: children of seeded play-outs over the perft corpus and
    // the bench positions, split by class, every row against its own class's
    // build-only row.
    let mut class_fens = perft_corpus_fens("tests/perft_values.md");
    println!(
        "class corpus: {} perft positions + {} bench positions, 2 seeds x 24 plies",
        class_fens.len(),
        POSITIONS.len()
    );
    class_fens.extend(POSITIONS.iter().map(|(_, fen)| (*fen).to_string()));
    let nodes = collect_nodes(&class_fens, 2, 24);
    per_class_table(&nodes, repeats, iterations, budget_ms);
    double_push_table(&nodes, repeats, iterations);
}
