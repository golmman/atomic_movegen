# Report — Plan 1: child-view terminality (MG15)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| crate-private `PositionView` (`src/position.rs`) | Done | Placement, side to move, ep target, castling rights, clock. Not exported. |
| One rules implementation over the view | Done | `legal_with_counts`/`legal_in_view`, `checkers`, `pinned`, `populate_state`, `is_material_draw`, `blast_casualties`, `is_capture`. |
| Generation over the view | Done | `generate_pseudo_legal_in_view`, `for_each_pseudo_legal_in_view`, `generate_legal_in_view`, `for_each_legal_in_view`, `has_legal_move_in_view`. |
| Ladder over the view | Done | `TrivialLegality`, `WidenedLegality`, `FastLegality` take `&PositionView`. |
| One terminal precedence | Done | `position::terminal_in_view`, used by `Board::terminal*` and `EffectContext::terminal_after`. |
| `EffectContext::terminal_after` | Done | No clone, no `MoveList`, no allocation. |
| `Board::terminal_after` | Done | Same signature shape as `Board::outcome_after`. |
| `outcome_after` clone fallback removed | Done | Now `is_legal` + `terminal_after(..).map(outcome)`. |
| `EffectContext::state` field removed | Done | Castling legality runs on the view. |
| Optional `is_terminal_after` | Done | Literal `is_terminal` mirror on the child. |
| Optional `has_legal_move_after` | Done | Child existence query. |
| Differential + fixtures (`tests/terminal_after.rs`) | Done | 15,013 positions / 285,064 pseudo-legal moves. |
| Shared test helpers (`tests/common/mod.rs`) | Done | Extracted from `tests/effects.rs`. |
| `PositionView` unit tests | Done | Accessor-for-accessor and child-view-vs-`do_move` over 49 positions. |
| Measurement (`examples/effect_bench.rs`) | Done | `terminal_after` cost, clone comparison, decided share. |
| Docs (`effects`, `lib.rs`, `README`, `CHANGELOG`) | Done | Doc bug fixed. |
| Perft and timing gates | Done | 41/41 depths 1–6; instruction ratios −1.9 % … +0.1 %. |
| Final report | Done | This file. |

## Public API as implemented

```rust
impl EffectContext {
    /// Terminal state of the child after `m`, from the child's side to move,
    /// without building the child. `None` = not decided. Exactly
    /// `{ clone; do_move; terminal() }` for every pseudo-legal `m`.
    /// Legality is NOT checked; malformed input returns `None`.
    pub fn terminal_after(&self, board: &Board, m: Move) -> Option<Terminal>;

    /// Exactly `{ clone; do_move; is_terminal() }` for every pseudo-legal `m`.
    pub fn is_terminal_after(&self, board: &Board, m: Move) -> Option<bool>;

    /// Exactly `{ clone; do_move; has_legal_move(&child) }`.
    pub fn has_legal_move_after(&self, board: &Board, m: Move) -> Option<bool>;

    /// Unchanged signature/behaviour, no clone: `is_legal(..)` then
    /// `terminal_after(..).map(|t| t.outcome)`.
    pub fn outcome_after(&self, board: &Board, m: Move) -> Option<Outcome>;
}

impl Board {
    /// Convenience wrapper for callers that keep no context.
    pub fn terminal_after(&self, m: Move, state: &StateInfo) -> Option<Terminal>;
}
```

Crate-internal additions (not exported): `PositionView`, `ChildPosition`,
`position::{child_view, legal_with_counts, legal_in_view, terminal_in_view}`,
`Board::position_view()`, and the `*_in_view` generator/ladder entry points.
No public type, variant, field, ordering, perft value, hash, FEN or
`Outcome`/`Terminal` semantic changed.

## Design as implemented

- **`Board` stores the view.** `Board { squares, position: PositionView,
  game_ply, hash }`; `Board::position_view()` borrows it. Every rule entry
  point on `Board` (`legal`, `populate_state`, `compute_checkers`,
  `compute_pinned`, `is_material_draw`, `is_capture`, `blast_casualties`,
  `terminal_impl`) delegates to the view, and the generator/ladder take
  `&PositionView`. There is exactly one implementation of each rule.
- **One terminal precedence.** `position::terminal_in_view(view, state,
  moves_exist)` is the body of the old `Board::terminal_impl` with
  `has_legal_move_with_state(self, ..)` replaced by
  `has_legal_move_in_view(view, ..)`. Branch order and values are unchanged.
- **Child derivation.** `position::child_view(parent, m, rule50)` returns
  `ChildPosition { view, is_capture, casualties }`, mirroring `do_move` field
  for field (placement incl. blast, castling relocation, promotion, capture;
  side to move; ep square after a double push; `castling_rights_after` with the
  child's post-blast home squares; `board.rule50_after(m)`). Malformed input —
  `Move::NONE`, `Square::NONE`, empty origin, wrong-color origin — returns
  `None`.
- **`terminal_after`** = `analyze` → `populate_state` on the child view →
  `terminal_in_view(.., None)`. No board write, no `MoveList`, no allocation.

## Differential counts (the gate)

`cargo test --test terminal_after -- --nocapture`:

```
terminal_after: 15013 positions, 285064 pseudo-legal moves, 29842 decided children
```

Every one of those candidates compared, for the *same* move:

| Assertion | Result |
| --- | --- |
| `ctx.terminal_after == clone.do_move.terminal()` | equal |
| `== clone.do_move.terminal_with_state(&populated_child_state)` | equal |
| `Board::terminal_after(m, &state) == ctx.terminal_after` | equal |
| `ctx.is_terminal_after == clone.do_move.is_terminal()` | equal |
| `ctx.has_legal_move_after == has_legal_move_with_state(child)` | equal |
| `ctx.outcome_after == Some(outcome)` iff legal **and** decided | equal |

Corpora: the 41-position perft table, 12 child-view fixtures, seeded
play-outs (6 start FENs × 16 seeds × depth 300, plus every perft position × 2
seeds × depth 60).

`tests/effects.rs` (unchanged assertions, shared helpers) still checks 15,025
positions / 285,193 candidates against `Board::legal` and a cloned child, and
`outcome_after` on 23,560 decided legal moves.

Unit test in `src/position.rs`: `view_matches_board` (all accessors, checkers,
pinned, populate_state, material draw, `piece_on`/`empty` for all 64 squares
over 49 positions) and `child_view_matches_do_move` (placement, per-type sets,
side, ep, rights, clock for every pseudo-legal move of those positions). A
view-construction defect therefore fails with a field name.

## Deterministic fixtures — observed values

| Case | Parent FEN | Move | Observed child `terminal()` |
| --- | --- | --- | --- |
| stalemate child | `7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55` | `c2c7` | `Draw` / `Stalemate` |
| stalemate child (2) | `8/8/k7/2p4p/7P/7K/5r2/8 b - - 1 59` | `f2g2` | `Draw` / `Stalemate` |
| extinction child | `k1n5/8/1K6/8/8/8/8/8 b - - 0 1` | `c8b6` | `Loss` / `Extinction` |
| illegal self-explosion child | `8/8/8/8/8/8/8/3qK3 w - - 0 1` | `e1d1` | `Loss` / `Extinction` |
| material draw from a capture | `8/8/8/2R5/4n3/8/7K/5k2 b - - 13 69` | `e4c5` | `Draw` / `MaterialDraw` |
| material draw (2) | `8/8/8/8/8/7k/8/2K3Rr b - - 19 74` | `h1g1` | `Draw` / `MaterialDraw` |
| material-draw parent, quiet child | `k7/8/8/8/8/8/8/7K w - - 0 1` | `h1g1` | `Draw` / `MaterialDraw` |
| mate outranks the clock | `7k/8/8/5N2/3K4/8/8/R6R w - - 99 60` | `a1g1` | `Loss` / `NoMovesInCheck`, child `rule50 = 100` |
| clock, child in check with an escape | `4k3/8/8/8/8/8/8/R3K3 w - - 99 60` | `a1a8` | `Draw` / `FiftyMoveRule` |
| clock, child not in check | `8/8/8/8/8/4k3/8/4K2q w - - 99 60` | `e1d2` | `Draw` / `FiftyMoveRule` |
| en-passant escape is the child's only legal move | `8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41` | `g7g5` | `None`; child `ep = g6`, in check, legal `["h5g6"]` |
| child keeps castling rights | `r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1` | `a1a2` | `None`; child rights `Kkq` (`0b1101`), black `e8g8`/`e8c8` legal |
| same position, commoner move | same | `e1f1` | `None`; child rights `kq` (`0b1100`) |
| K-vs-K divergence | `k7/8/8/8/8/8/8/7K w - - 0 1` | `h1g1` | `Some(MaterialDraw)` vs `is_terminal_after == Some(false)` |

All rows are asserted both against the documented value and against the cloned
child, so the table cannot drift from the engine.

## Perft before / after

Host: AMD Ryzen 9 5950X (4 visible cores), rustc 1.99.1, Linux. Release
profile as in `Cargo.toml` (`lto = "fat"`, `codegen-units = 1`).
`instructions:u` is deterministic per binary and is the primary metric; wall
time (median of 3, alternating binaries) is secondary.

### Instruction counts

| Position | Depth | Baseline | Candidate | Ratio |
| --- | ---: | ---: | ---: | ---: |
| start position | 5 | 423,321,625 | 419,733,533 | **0.9915** |
| kiwipete | 5 | 13,501,168,891 | 13,508,141,040 | 1.0005 |
| middlegame-13 (perft table #13) | 5 | 5,284,277,387 | 5,241,530,941 | **0.9919** |
| deep-win | 5 | 3,522,872,185 | 3,508,315,674 | **0.9959** |
| middlegame-21 (perft table #21) | 5 | 2,162,657,606 | 2,162,001,221 | 0.9997 |
| promotion-heavy | 5 | 289,581,316 | 285,383,615 | **0.9855** |
| castling-both (`r3k2r/8/8/8/8/8/8/R3K2R w KQkq -`) | 5 | 879,188,472 | 862,764,260 | **0.9813** |
| en passant (`8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -`) | 5 | 83,425,595 | 82,130,849 | **0.9845** |
| perft table #2 | 4 | 123,764,341 | 123,207,866 | 0.9955 |

No position regresses by more than 0.05 %, seven of nine improve by 0.4–1.9 %.

### Wall time (median of 3)

| Position | Nodes | Baseline | Candidate |
| --- | ---: | ---: | ---: |
| start position d6 | 118,926,425 | 575 ms | 571 ms |
| kiwipete d5 | 153,516,309 | 797 ms | 773 ms |
| middlegame-13 d5 | 53,833,718 | 312 ms | 309 ms |
| deep-win d5 | 41,091,330 | 200 ms | 196 ms |
| middlegame-21 d5 | 22,933,256 | 125 ms | 124 ms |
| promotion-heavy d6 | 35,364,502 | 264 ms | 261 ms |

`cargo run --release --example verify_perft 6` (41 positions, depths 1–6):
**41/41 passed**, 63–64 s total, versus 62.7 s for the 2.6.0 baseline binary
(run-to-run noise is ±1 s; the slowest single test, #13, is 9.7 s baseline and
10.0 s candidate). Depth 5 alone: 41/41, 2.2 s.

## `terminal_after` cost and decided share

`cargo run --release --example effect_bench 9 2000` (median of 9 batches ×
2000 calls, ns per call):

| Position | `terminal_after` | `clone+do_move+terminal` | `clone+do_move+generate_legal` |
| --- | ---: | ---: | ---: |
| deep-win | 39 | 31 | 156 |
| middlegame-13 | 37 | 29 | 187 |
| middlegame-21 | 38 | 31 | 159 |

| Position | `has_legal_move_after` | `clone+do_move+has_legal_move` |
| --- | ---: | ---: |
| deep-win | 34 | 29 |
| middlegame-13 | 35 | 28 |
| middlegame-21 | 36 | 29 |

| Position | `outcome_after` (2.6.0) | `outcome_after` (this plan) | `clone+do_move+outcome` |
| --- | ---: | ---: | ---: |
| deep-win | 67 | 64 | 32 |
| middlegame-13 | 67 | 54 | 29 |
| middlegame-21 | 65 | 58 | 31 |

| Position | `effect.is_legal` (2.6.0) | `effect.is_legal` (this plan) | `Board::legal` |
| --- | ---: | ---: | ---: |
| deep-win | 27 | 31 | 8 |
| middlegame-13 | 33 | 27 | 7 |
| middlegame-21 | 26 | 29 | 8 |

`effect.is_legal` moves within the ±15 % noise band of these 2000-call batches
with no consistent direction; it was not the target of this plan.

**Decided share** over seeded play-outs (4 start FENs incl. startpos × 8 seeds
× depth 60), children fed to `terminal_after`:

```
pseudo-legal children:  47884  decided    816  (1.70 %)
legal children:         45512  decided    434  (0.95 %)
```

This is *this crate's* tree, not the downstream prover's tree: it is not their
10 % gate, only the crate's own statement of what it decides.

Honest reading of the numbers: `terminal_after` is **23–28 % slower** than
`clone+do_move+terminal()` (37–39 ns vs 29–31 ns) and **4–5× faster** than
`clone+do_move+generate_legal_with_state()` (156–187 ns), which is the shape
the downstream measures for "does the child have a move?". The clone path is
cheap because `Board` keeps a square array and `do_move` is a tuned routine;
the derived child view costs about 6–8 ns, while the remaining work (state
population + the existence query) is identical in both shapes. Removing the
clone is a correctness/lifetime win (no board write for the caller), not yet a
speed win against cloning. See "Next steps".

## Deviations from the plan

1. **The view is nested in `Board`, not snapshotted per call.** The plan's
   `PositionView::of(board)` for every rule entry point measured at +4.3 %
   instructions on middlegame-13 before optimisation (over the 2 % gate).
   `Board` now owns the view and `Board::position_view()` lends it, so the
   board path copies nothing. `PositionView::of` still exists and copies, for
   the effect context's snapshot.
2. **Occupancy is not cached in the view.** A cached `occupied` had to be
   maintained by `move_piece`/`remove_piece`/`place_piece`; those extra writes
   in `do_move`/`undo_move` cost more than recomputing in capture-heavy trees
   (+3.8 % instructions on middlegame-13, measured). The view derives
   `occupied()` from the two color sets; node-constant consumers
   (`TrivialLegality`, `WidenedLegality`) cache it once per node. The invariant
   is structural, so a desync is impossible.
3. **`walk_corpus_and_fixtures` takes the fixture slice as a parameter**
   (`tests/common/mod.rs`): the effect fixtures stay in `tests/effects.rs` and
   `tests/terminal_after.rs` has its own child-view fixtures.
4. **Both optional queries landed** (`is_terminal_after`,
   `has_legal_move_after`). Neither adds a branch to `terminal_after`; both
   reuse `analyze` and are differentially tested.
5. **Wrong-color origin is malformed input** (`None`), in addition to the
   plan's `Move::NONE`/empty origin. Without it, `terminal_after` would either
   report a child built from the wrong mover color or trip a debug assertion;
   `EffectContext::effect` now documents the case.
6. `Board::is_capture` and `Board::blast_casualties` also delegate to the view
   (one implementation of each rule). `Move::dest_index` was kept because the
   ladder's capture test uses the raw index instead of the `Square` table load.
7. The `CHANGELOG` entry is under `## [Unreleased]`; naming 2.7.0 and publishing
   is plan 3's step.

## Problems encountered

1. **First implementation was over the performance gate.** Snapshotting the
   view at every entry point cost +4.3 % (middlegame-13) and +5.7 % (start
   position) instructions. Two rounds of measured fixes brought it to −1.9 % …
   +0.1 %: (a) one view per node in `generate_legal`/`for_each_legal`/
   `has_legal_move` instead of one per internal call, (b) nesting the view in
   `Board`, (c) caching occupancy in the node-constant ladder structs,
   (d) using `Move::dest_index` for the ladder's capture test.
2. **A cached `occupied` with incremental maintenance regressed capture-heavy
   perft trees** (+3.8 %); reverted (see deviation 2). This was found only
   because instruction counts were compared per position, not just wall time.
3. **Debug-assert panic on malformed input.** `analyze` computed
   `board.rule50_after(m)` before validating the origin; `classify_move`
   debug-asserts on an empty origin, and `tests/effects.rs`'s
   `malformed_moves_are_rejected` caught it. The origin is checked first now.
4. **A wrong-color origin would have tripped a debug assertion** in
   `child_view`; it is now rejected as malformed input (deviation 5).
5. **Attribution inside an inlined `perft`** was hard with `perf record`
   (everything is inlined). A temporary `examples/iso_bench.rs` measured
   isolated per-function instruction counts (removed before finishing);
   final numbers use whole-tree `instructions:u` on eight positions.

## Unresolved parts

- `terminal_after` is not yet faster than `clone+do_move+terminal()` (see
  above). Two measured candidates: (a) the plan's §6 lazy state population
  (skip `pinned` when it is not needed), (b) splitting the child derivation
  into placement-only (for `effect`/`is_legal`, which never read the child's
  ep/rights/clock) and the full view (for the child queries). Both change the
  shape of the effect path, so they belong to plan 3's MG18 decision.
- The decided share (1.7 % of pseudo-legal children in this crate's own trees)
  is published for reference only; whether the downstream gate is met can only
  be checked downstream after a release.

## Missing tests

- No test pins that a **stale** `StateInfo` handed to `EffectContext::new`
  yields the historical `game_over`-gated answers; game-over positions are
  covered, stale-state equivalence is not.
- No differential for `terminal_after` on well-formed but non-pseudo-legal
  moves (e.g. a rook move through a piece) — the documented contract is
  pseudo-legal input only, and malformed input is covered.
- The `castling_rights_after` home-square cascade is only covered indirectly
  (through the child-vs-`do_move` differential and the castling fixtures), not
  by a dedicated unit test of the bitboard path.

## Changed files

- `src/position.rs` — **new**: `PositionView`, `ChildPosition`, `child_view`,
  `legal_with_counts`, `legal_in_view`, `terminal_in_view`, unit tests.
- `src/board.rs` — `Board` stores the view; `legal`, `populate_state`,
  `compute_checkers`, `compute_pinned`, `is_material_draw`, `is_capture`,
  `blast_casualties`, `terminal_impl` delegate; `position_view()` added;
  `castling_rights_after` made `pub(crate)`; `Board::terminal_after` added;
  `square_attacked_by` moved to the view; ladder takes `&PositionView`; docs.
- `src/movegen.rs` — `*_in_view` entry points; public wrappers build/borrow one
  view per node; docs.
- `src/effects.rs` — `EffectContext` stores the view (no `StateInfo` copy),
  `terminal_after`, `is_terminal_after`, `has_legal_move_after`,
  clone-free `outcome_after`, `PostMove` wraps the child view, module docs and
  doc example.
- `src/lib.rs` — `pub(crate) mod position;`, crate docs (Move effects,
  allocation-free list).
- `src/types.rs` — `Move::dest_index` docs only (the ladder's raw-index capture
  test); no semantic change.
- `tests/common/mod.rs` — **new**: shared corpus/PRNG/play-out helpers.
- `tests/effects.rs` — uses the shared helpers.
- `tests/terminal_after.rs` — **new**: differential, fixtures, malformed
  input, read-only checks.
- `examples/effect_bench.rs` — `terminal_after`/`is_terminal_after`/
  `has_legal_move_after` rows and the decided-share walk.
- `README.md`, `CHANGELOG.md` — feature bullet, contract, doc-bug fix.
- `docs/plans/feedback/report_handoff2_1.md` — this report.

## Verification run

```sh
cargo test                                       # 158 unit tests + every integration test
cargo clippy --all-targets -- -D warnings        # clean
cargo fmt --check                                # clean
cargo doc                                        # no warnings
cargo test --features unsafe-optimizations       # all green
cargo test --test perft_tests --test move_order_golden --test verify_moves \
           --test conformance --test effects --test interchange --test null_for
cargo run --release --example verify_perft 6     # 41/41, 63–64 s
cargo run --release --example effect_bench 9 2000
```

## Next steps

1. Plan 2 (`plan_handoff2_2.md`): `AfterState { Illegal, Undecided, Terminal }`
   with `child_outcome` on top of `terminal_after`, and the public `PostMove`
   summary wrapping `ChildPosition`.
2. Plan 3 (`plan_handoff2_3.md`): `MoveClassification`/`classification`
   (MG18), the measured decision about the effect path (including the two
   candidates listed under "Unresolved parts"), and the 2.7.0 release with
   `cargo publish --dry-run`.
3. If a downstream reports the 10 % decided-share gate as critical, the next
   measured change is the lazy state population (plan §6), with its own
   differential and perft gates.
