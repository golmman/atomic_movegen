# Implementation report — plan 2 (W7: legality entry point at occupancy cost)

Status: **landed** as additive API; exactness and the plan's shipping gate pass.
W7's absolute instruction target (≤ 100 per candidate) is **not reached**; the
measured context is ~124.5 net instructions per candidate against ~138.6 for
`Board::legal`, so the wishlist's numeric item stays open even though the new
entry point is measurably cheaper.

Plan: `docs/plans/feedback/plan_wishlist2_2.md`.
Wishlist item: W7 (P1), legality entry point at occupancy cost. Plan 5 owns the
release version and notes.

## Plan validation

The plan's current-tree assessment matched the code: `Board::legal` delegated
to `position::legal_in_view`, the rule body was `position::legal_with_counts`,
`FastLegality`/`WidenedLegality` were crate-private state-aware ladders, and no
public candidate-loop legality context existed. The prior
category-specialised split was not retried.

One implementation choice changed from the plan's sketch. The first version
made `LegalNode::from_view` the board path as well. That cost ~7.5 extra
instructions per `Board::legal` call (~5.4 % on the candidate probe), which the
plan forbids. The final shape keeps the borrowed view on the board path and
shares the body through a crate-private `LegalData` trait:

```rust
trait LegalData { /* placement accessors */ }
impl LegalData for PositionView { /* borrows the board view */ }
impl LegalData for LegalNode { /* reads the cached snapshot */ }

fn legal_in<D: LegalData + ?Sized>(data: &D, m: Move) -> bool;
```

`Board::legal` -> `legal_in(&PositionView, m)`; `LegalityContext::legal` ->
`legal_in(&LegalNode, m)`. This is one rules implementation without copying a
node on the existing path. The final board-path instruction count is
statistically identical to the baseline.

## What landed

### Public API

```rust
// atomic_movegen::LegalityContext (also atomic_movegen::board::LegalityContext)

#[derive(Debug, Clone, Copy)]
pub struct LegalityContext { /* private */ }

impl LegalityContext {
    pub fn occupied(&self) -> Bitboard;
    pub fn legal(&self, m: Move) -> bool;
}

impl Board {
    pub fn legality_context(&self, counts: Counts) -> LegalityContext;
}
```

`LegalityContext` stores a snapshot of the node constants (`LegalNode`) and one
`playable` flag derived from the two `Counts`; it does not store a `Counts`
value or borrow the board. It is `Copy + Send + Sync`, allocation-free, and
does not populate or read a `StateInfo`. The `playable` flag is a smaller
game-over guard than two count getters per candidate.

`Board::legality_context(counts)` is the only constructor. `counts` must be
current for the board, exactly like a populated `StateInfo`; `Board::counts` or
`Counts::from(&state)` provide it. The context is a snapshot: a stale context
answers for the old placement, as documented.

### Shared rules body

`src/position.rs` now has:

- `LegalData`, implemented by `PositionView` and `LegalNode`;
- `legal_in`, the one castling/en-passant/blast/commoner body;
- `LegalNode::legal`, a thin call to `legal_in(self, m)`;
- `legal_with_counts(view, m)`, a thin call to `legal_in(view, m)`.

There is no copied castling/EP/blast logic. `legal_in_view` keeps the game-over
guard from `StateInfo`. The effect path's castling fallback and
`legal_ep_square_in_view` still call the same `legal_with_counts` body.

The plan's optional quiet-move fast path was **not added**. It would have needed
new node-derived checker/pin facts beyond `Counts`, and the context already
passes the measured-improvement gate. No category-specialised split was
reintroduced.

## Differential counts

New suite: `tests/legality_context.rs`. Every pseudo-legal candidate of the
perft corpus, conformance cases, `CHILD_CASES` positions, effect/board fixtures,
edge fixtures, and the seeded play-outs is checked for:

```text
ctx.occupied() == board.occupied()
ctx.legal(m)   == board.legal(m, &full)
               == board.legal(m, &counts_only)
               == board.is_legal_with_state(m, &counts_only)
```

`generate_legal_with_state` output is also checked to be a subset of the
context's accepted candidates, and `fen()`/`hash()` are unchanged across the
context build/query.

| Corpus | Positions | Terminal | In check | Pinned | EP target | Castle rights | Candidates | Castling | EP | Legal |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| deduplicated corpora/fixtures | 87 | 6 | 16 | 7 | 4 | 19 | 1,715 | 8 | 2 | 1,388 |
| seeded play-outs | 14,960 | 82 | 518 | 344 | 354 | 1,756 | 283,924 | 350 | 38 | 261,430 |

The extra unit test `board::tests::legality_context_matches_legal_on_corpus`
walks the 41-position perft corpus directly. `tests/public_api.rs` pins the
public type, constructor, `occupied`, `legal`, and `Send + Sync`. `src/lib.rs`
adds the `Send + Sync` assertion.

## Benchmark

Harness: `examples/hotpath_bench.rs`, extended with the per-candidate section.
Command:

```sh
cargo run --release --example hotpath_bench 15 16
```

Each row is the median of 15 batches; a call is one candidate. Context
construction is measured per node. Class rows use their own candidate index
list so a rare class is not dominated by a branch over all candidates; a class
with fewer calls uses extra passes to reach roughly 100k calls. Absolute ns are
same-host only.

| Dataset | Row | ns/call | Ratio to `Board::legal` |
| --- | --- | ---: | ---: |
| corpus (77 nodes, 1,724 candidates) | `legality_context` setup | 2.32 ns/node | — |
| corpus | quiet (1,280) | 8.09 vs 7.64 | 0.944x |
| corpus | capture (132) | 7.88 vs 7.52 | 0.954x |
| corpus | en-passant (1) | 10.85 vs 10.14 | 0.934x |
| corpus | castling (6) | 10.38 vs 10.27 | 0.989x |
| corpus | commoner move (338) | 8.48 vs 7.94 | 0.936x |
| corpus | all (1,724) | 8.61 vs 8.10 | **0.941x** |
| play-outs (38,700 nodes, 835,794 candidates) | `legality_context` setup | 3.11 ns/node | — |
| play-outs | quiet (573,928) | 8.77 vs 8.33 | 0.950x |
| play-outs | capture (63,928) | 10.00 vs 9.24 | 0.924x |
| play-outs | en-passant (116) | 10.98 vs 10.67 | 0.972x |
| play-outs | castling (934) | 15.92 vs 15.13 | 0.950x |
| play-outs | commoner move (206,602) | 10.21 vs 9.46 | 0.927x |
| play-outs | all (835,794) | 9.68 vs 9.15 | **0.946x** |

The `Board::legal` column is the first ns figure in the paired row; the context
figure is the second. All classes improve; no class regresses. Setup is
amortised by the candidate list (~22 candidates per node here).

### Instruction diagnostic

`perf stat -e instructions:u` is available and stable on this host. A temporary
example (removed after the run) built the deduplicated perft/conformance corpus
(63 nodes, 1,382 pseudo-legal candidates per pass) and ran 2,000 passes
(2,764,000 calls per mode). Three repetitions gave the same counts. Command
shape:

```sh
perf stat -e instructions:u target/release/examples/_instr_probe empty 2000
perf stat -e instructions:u target/release/examples/_instr_probe board 2000
perf stat -e instructions:u target/release/examples/_instr_probe ctx   2000
```

| Row | Total instructions | Raw / call | Net of empty-loop overhead |
| --- | ---: | ---: | ---: |
| `empty` (loop + `black_box`) | 21,713,632 | 7.86 | — |
| `Board::legal` | 405,014,019 | 146.53 | ~138.68 |
| `ctx.legal` | 365,936,002 | 132.39 | **~124.54** |

So the context saves ~14.1 instructions per candidate (~10.2 % of the net board
cost) on this probe. The wishlist's 144 -> ≤ 100 target is not met, and the
net context cost is just above the plan's ~120 “keep it open” line. The report
therefore claims the measured improvement, not the absolute ceiling.

## Existing-path regression checks

- **`Board::legal` instruction count**: baseline `af3faf0` net ~138.62;
  current net ~138.68 on the same probe. The `LegalData` design removed the
  node-construction regression found in the first implementation.
- **Perft timing**: Kiwipete depth 5, nine alternating release runs against an
  `af3faf0` archive build. Baseline median **782 ms**, current median **780 ms**,
  ratio **0.9974**; both returned 153,516,309 nodes. No regression.
- **`effect_bench`**: `effect_bench 15 5000 0`, five alternating runs each,
  medians. `board.legal` is 9/8/8 ns in both trees across the three bench
  positions; classification/effect rows are equal or within one ns. The largest
  observed difference was `generate_legal` +1.7 % on one position, inside timer
  granularity and below the 2 % blocker.
- No key, move order, FEN, terminality, `ZOBRIST_VERSION`, `Always` table, or
  generation-order change.

## Gate results

- `cargo test` — all suites pass; 191 unit tests plus all integration suites and
  18 doctests.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo doc --no-deps` — no warnings; `cargo test --doc` passes.
- `cargo run --release --example verify_perft 5` — **41/41**.
- Targeted suites (`perft_tests`, `move_order_golden`, `verify_moves`,
  `conformance`, `effects`, `post_move`, `terminal_after`, `child_state`,
  `ep_key`, `null_for`, `public_api`, `interchange`, new `legality_context`) all
  pass through `cargo test`.
- `cargo test --features unsafe-optimizations` — all pass. This plan does not
  change attack geometry, but the feature run was done anyway.

## Changed files

- `src/position.rs` — `LegalData`, `legal_in`, `LegalNode`, shared body
  wiring; the old body moved without rule changes.
- `src/board.rs` — public `LegalityContext`, `Board::legality_context`, docs,
  and `legality_context_matches_legal_on_corpus`.
- `src/lib.rs` — root re-export, hot-path docs, `Send + Sync` assertion.
- `tests/legality_context.rs` — new differential and edge suite.
- `tests/public_api.rs` — public API pin and zero-count guard test.
- `examples/hotpath_bench.rs` — candidate rows, class lists, setup row.
- `README.md`, `CHANGELOG.md` — additive API, contract, and measured status.
- `docs/plans/feedback/report_wishlist2_2.md` — this report.

## Problems encountered and deviations

1. **Node construction on the board path was a real regression.** The first
   implementation made `legal_with_counts` build a `LegalNode`; the board-path
   instruction probe rose from ~138.6 to ~146.1 net per call. The final
   `LegalData` generic keeps the view path allocation/copy-free and restores the
   baseline count. This is the main deviation from the plan's code sketch; the
   single-body requirement is unchanged.
2. **The context stores a `playable` flag instead of `Counts`.** Only the guard
   matters per candidate, and a bool avoids two getters/comparisons. The public
   constructor still takes `Counts`; no public contract changes.
3. **The per-class harness needed per-class candidate lists.** Filtering all
   candidates inside each class row made rare EP/castling rows measure the
   filter loop rather than the legality call. The harness now builds an index
   list per class and scales passes for rare classes.
4. **The absolute instruction target is not portable.** The ≤ 100 figure is a
   downstream-compiler measurement. This host reports ~124.5 net, so the
   report does not claim it. No micro-tuning was attempted beyond the final
   shared-body design; the context already wins against the current board path.

## Unresolved parts and missing tests

- W7's ≤ 100-instruction acceptance is open. The public context is shipped
  because it passes the plan's explicit shipping gate, but plan 5 should not
  describe W7 as having met the wishlist's absolute number.
- No quiet-move fast path. It is still a possible later experiment, but any such
  path must derive sound node facts rather than reading stale
  `checkers`/`pinned`; none was added here.
- `Move::NONE` and other malformed moves remain outside the context contract,
  exactly as they are for `Board::legal`. Only the terminal zero-count guard is
  tested directly.
- The context's snapshot semantics are documented but not separately tested by
  mutating a board after context construction; ordinary stale-`StateInfo`
  behaviour is the model.
- `unsafe-optimizations` was run although this plan does not touch geometry.

## Next steps

- Plan 3 can use `LegalityContext::occupied()` as the optional occupancy reuse
  for `Board::child_effect_with_occupancy`; `Board::child_effect` remains
  independent of the context.
- Plan 4 (`null_for_advanced`) is still independent.
- Plan 5 should choose 2.10.0 for the landed additive API, pin
  `LegalityContext` and `Board::legality_context`, and keep W7's absolute
  instruction target listed as open rather than met.
