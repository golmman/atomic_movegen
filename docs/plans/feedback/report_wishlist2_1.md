# Implementation report — plan 1 (W6: counts-only state)

Status: **landed** (additive API, no answer changes).

Plan: `docs/plans/feedback/plan_wishlist2_1.md`.
Wishlist item: W6 (P3), counts-only state or a documented legality
precondition. This report is for plan 1 only; plan 5 owns the release.

## What landed

### Public API

```rust
// atomic_movegen::Counts (also atomic_movegen::board::Counts)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts { /* private fields */ }

impl Counts {
    pub const fn new(commoners: u32, them_commoners: u32) -> Self;
    pub const fn commoners(self) -> u32;
    pub const fn them_commoners(self) -> u32;
}

impl From<&StateInfo> for Counts;

impl Board {
    pub fn counts(&self) -> Counts;
    pub fn populate_counts(&self, state: &mut StateInfo);
}
```

`Counts` is re-exported from the crate root and from `board`. It is `Copy`,
`Eq`, `Send + Sync`, and allocation-free. Fields stay private so a pair cannot
be built without naming the side-to-move/opponent convention.

`populate_counts` writes only `commoners_count` and `them_commoners_count`. It
does not clear or compute `checkers`, `pinned`, or the undo fields. A full
`populate_state` keeps its exact previous behavior.

### Precondition documentation

The distinction is now stated at the entry points, not only in prose elsewhere:

- `Board::legal` — reads only `state.commoners_count` and
  `state.them_commoners_count`; `checkers`/`pinned` are not consulted.
- `Board::is_legal_with_state` — same two fields; the pseudo-legal shape check
  is unchanged.
- `EffectContext::new` — reads only the two counts for its `game_over` snapshot.
  Its child/effect queries derive the child placement themselves.
- `generate_legal_with_state`, `for_each_legal_with_state`,
  `has_legal_move_with_state` — explicitly still require the full cached state
  (`checkers` and `pinned`); a counts-only state is not sufficient.

`Board::populate_state` and `StateInfo`'s layout/default are unchanged.

## Differential counts

All runs are exact-equality runs of the two count writers. There is no oracle
change: the full-state answer is the reference and the counts-only state must
match it.

| Corpus | Positions | Zero-count positions | Pseudo-legal candidates | Legal candidates |
| --- | ---: | ---: | ---: | ---: |
| perft + conformance + effect fixtures + board fixtures | 100 | 4 | 2,078 | 1,706 |
| seeded play-outs (`tests/common::walk_playouts`) | 14,960 | 82 | 283,924 | 261,430 |

For every candidate:

- `board.legal(m, &counts_only) == board.legal(m, &full)`;
- `board.is_legal_with_state(m, &counts_only) ==
  board.is_legal_with_state(m, &full)`;
- every generator-produced legal move answers `true` through the counts-only
  state;
- a fresh `StateInfo::new()` (zero counts) rejects every pseudo-legal candidate,
  the documented game-over guard.

The differential is `tests/counts.rs` (new). Unit tests in `src/board.rs` also
check the complete perft corpus, five board fixtures, and a 316-position seeded
walk directly against `Counts::from(&full_state)`.

### Boundary test

`counts_only_state_is_not_sufficient_for_move_generation` pins the negative
side of the contract:

- the pinned-rook fixture `4k3/4r3/8/8/8/8/4R3/4K3 w - - 0 1` produces a
  different legal list from a counts-only state (an empty `pinned` set accepts
  `e2d2`);
- the checkmate fixture
  `rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3` makes
  `has_legal_move_with_state` report `true` from a counts-only state and
  `false` from the full state.

This was found while writing the differential: the first version asserted that
`has_legal_move_with_state` agreed for counts-only and full states. It does not,
by design. The assertion was removed from the differential and replaced with
the explicit boundary test, so the limitation is visible rather than hidden.

## Benchmark

Harness: `examples/hotpath_bench.rs` (new). Row names are stable for plans 2–4.
Command:

```sh
cargo run --release --example hotpath_bench 15 32
```

Each row is the median of 15 batches of 32 full board-list passes, reported in
ns per call. Ratios are within the same run.

| Dataset | Row | ns/call | Ratio to `populate_state` |
| --- | --- | ---: | ---: |
| corpus (78 boards) | `populate_state` | 8.09 | 1.000x |
| corpus | `populate_counts` | 1.26 | 0.155x |
| corpus | `Board::counts` | 1.23 | 0.151x |
| corpus | `StateInfo::new` | 1.17 | 0.145x |
| play-outs (38,706 boards) | `populate_state` | 9.34 | 1.000x |
| play-outs | `populate_counts` | 1.89 | 0.202x |
| play-outs | `Board::counts` | 1.62 | 0.173x |
| play-outs | `StateInfo::new` | 1.05 | 0.112x |

The setup writer is roughly 5–6.5× cheaper than the full writer; the reader is
in the same range. This is a same-machine comparison only.

### Instruction diagnostic

`perf stat -e instructions:u` counted on this host. A temporary example (not
shipped; deleted after measurement) ran each row over the 78-board corpus for
200,000 passes (15.6M calls). The totals include the board-loop and
`black_box` overhead, so an `empty` row (`side_to_move` + `black_box`) was
measured and subtracted for the net column:

| Row | Total instructions | Raw / call | Net of loop overhead |
| --- | ---: | ---: | ---: |
| `empty` (loop overhead) | 80,104,050 | 5 | — |
| `populate_state` | 2,006,504,480 | 128 | ~123 |
| `populate_counts` | 454,504,381 | 29 | ~24 |
| `Board::counts` | 360,704,169 | 23 | ~18 |
| `StateInfo::new` | 64,303,986 | 4 | ~0 |

The wishlist's “166 → 18” figure is a downstream diagnostic. Here the reader
lands at ~18 net instructions; the full writer at ~123. They are reported, not
treated as a portable gate, per the index.

## Gate results

- `cargo test` — all suites pass; unit tests 190 passed, 18 doctests passed.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo doc --no-deps` — no warnings; `cargo test --doc` passes.
- `cargo run --release --example verify_perft 5` — **41/41** positions pass at
  depths 1–5.
- Targeted integration suites: `perft_tests` (4), `move_order_golden` (1),
  `verify_moves` (1), `conformance` (6), `effects` (12), `post_move` (5),
  `terminal_after` (6), `child_state` (7), `ep_key` (3), `null_for` (2),
  `public_api` (5), `interchange` (3) — all pass.
- `cargo test --features unsafe-optimizations` — not run: plan 1 touches no key
  or attack helper and no geometry, so the feature cannot change this diff.
- Perft timing (`--release`, Kiwipete depth 5, 9 alternating runs against an
  `af3faf0` archive build): baseline median 0.7870 s, current median 0.7827 s,
  ratio **0.9945**. Both engines returned 153,516,309 nodes. The difference is
  inside run-to-run noise and well below the ~2% blocker.

## Changed files

- `src/board.rs` — `Counts`, `From<&StateInfo>`, `Board::counts`,
  `Board::populate_counts`, precondition docs, five unit tests.
- `src/position.rs` — crate-private `PositionView::counts`, counts assertions in
  the existing view/board comparison.
- `src/lib.rs` — root re-export, hot-path and allocation-free docs,
  `Counts` added to the `Send + Sync` assertion.
- `src/effects.rs` — `EffectContext::new` counts-only documentation.
- `src/movegen.rs` — the three state-aware entry points now state that
  `checkers`/`pinned` are required.
- `tests/public_api.rs` — `Counts`, `Board::counts`, `Board::populate_counts`
  resolve; counts-only answers match the full state; `Counts: Send + Sync`.
- `tests/counts.rs` — new differential and play-out suite.
- `examples/hotpath_bench.rs` — new harness with the four plan-1 rows.
- `README.md`, `CHANGELOG.md` — additive API and precondition docs
  (`CHANGELOG` has an `Unreleased` section; plan 5 picks the version).
- `docs/plans/feedback/report_wishlist2_1.md` — this report.

## Problems encountered and deviations

1. **`has_legal_move_with_state` is not counts-only.** The plan's differential
   sentence could be read as asking for existence equality; it is not true and
   must not be. The differential now checks only `legal` and
   `is_legal_with_state`, and the boundary is a unit test (see above).
2. **`populate_state` was left byte-for-byte in codegen.** The first
   implementation made `populate_state_with_checkers` call the new
   `PositionView::counts()` helper. An alternating perft comparison showed a
   ~1.4% slowdown (still inside the gate, but avoidable), so the helper stayed
   next to `populate_state_with_checkers` and the full writer kept its original
   two adjacent `popcount` writes. The counts-only path and the board reader
   share the helper. The re-measured ratio was 0.9945 with identical node
   counts.
3. **The instruction diagnostic needed an empty-loop subtraction.** Without it
   the raw counts include the benchmark loop and `black_box`. The temporary
   measuring example was removed; only the numbers are recorded here.
4. **`tests/counts.rs` counts more positions than the plan named.** It includes
   all 41 perft positions, 37 conformance FENs, 18 effect fixtures, and 4 board
   fixtures (100 total), plus the seeded walk. The effect fixtures are copied
   because the unit-test `CORPUS` is crate-private; if it changes, the copy is
   the one thing to update.

## Unresolved parts and missing tests

- The wishlist's instruction ceiling is not a gate. The reader reaches ~18 net
  instructions; `populate_counts` is ~24 net because it writes two private
  fields and is called through the `Counts` value. No attempt was made to
  micro-tune below that, because plan 2 measures the candidate-loop shape that
  actually matters.
- No public `Board::legal_with_counts` was added, intentionally. Plan 2 owns
  the candidate-loop entry point and its measurement gate.
- No `unsafe-optimizations` run (not applicable). No test asserts
  `populate_counts` on a child view, because the crate-private child path
  populates no `StateInfo`.
- Plan 5 still needs to pin `Counts` in the release notes and choose 2.10.0 (or
  the version that matches what later plans land).

## Next steps

- Plan 2 can build `LegalityContext` on `Counts` and expose `occupied()`.
- Plan 3 can use `ChildEffect` with the same `Counts`; its remaining
  dependency on plan 2 is only the optional occupancy reuse.
- Plan 4 (`null_for_advanced`) is independent of this plan.
