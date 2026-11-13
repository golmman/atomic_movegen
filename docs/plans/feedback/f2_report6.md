# Report 6 — Optional performance work and non-goals (P2-10)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| `unsafe-optimizations` feature | Done | Opt-in, default off; only the magic table/entry load is unchecked. |
| `Board::hash_and_rule50_after()` | Done | Shared classification; exactly `(hash_after, rule50_after)`. |
| Lazy `populate_state` | Deliberately deferred | No code change; reasoning below. |
| Non-goals from feedback §5 | Recorded | Nothing from that list was added. |
| Documentation | Done | Rustdoc, `README.md`, `CHANGELOG.md`, this report. |
| Verification | Done | Tests, clippy, fmt, docs, and depth-6 perft with and without the feature. |

## API decisions

- The feature is named `unsafe-optimizations`, declared in `Cargo.toml` with
  `default = []`. It gates exactly one thing: the bounds check on
  `BISHOP_TABLE` / `ROOK_TABLE` loads in `src/magic.rs`. `queen_attacks`
  remains the OR of the two gated functions. No other `unsafe` code was added;
  the pre-existing `piece_key_raw` helper (from the `hash_after` plan) is
  untouched.
- The safety argument is written down in the module header of `src/magic.rs`
  and repeated at each `get_unchecked`: the magic index is
  `((occupied & mask) * magic) >> shift` with `shift = 64 - index_bits`, so it
  is `< 1 << index_bits`; the table builders allocate exactly `1 << index_bits`
  entries at `entry.offset` using the same formula (and assert the subset
  count); `sq` is a `Square` and therefore `< 64`. Square indexing
  (`ENTRIES[sq]`) stays bounds-checked in both modes.
- `Board::hash_and_rule50_after(m)` is additive. It does **not** replace
  `hash_after` or `rule50_after`; both keep their public contracts and their
  separate timings. Its contract is
  `hash_and_rule50_after(m) == (hash_after(m), rule50_after(m))`, which in turn
  equals a cloned board's `(hash(), rule50())` after `do_move`.
- To share work without forcing branches into the two existing functions,
  `hash_after` was split into `hash_after_castling` + `classify_move` +
  `hash_after_classified`, and `rule50_after` into `classify_move` +
  `rule50_after_classified`. `MoveClass` holds the moving piece, `is_pawn`, and
  `is_capture`; the combined accessor computes it once. This is a pure
  refactor: the public paths execute the same operations as before (one
  origin lookup and one destination-occupancy test either way), so no existing
  timing changes shape. `is_pawn` is still derived from the raw encoding
  (`raw & 7 == Pawn + 1`), which is bit-for-bit equivalent to the
  `type_of() == Pawn` test `rule50_after` used before; the random-playout
  equivalence test confirms it.

## Feature verification matrix

All commands were run on the available x86_64 host.

| Command | Result |
| --- | --- |
| `cargo build` | clean |
| `cargo build --features unsafe-optimizations` | clean |
| `cargo test` | 108 unit + 1 move-order + 4 perft + 1 verify_moves + 1 doctest, all pass |
| `cargo test --features unsafe-optimizations` | same suites, all pass |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `cargo doc --no-deps` | clean |
| `cargo doc --no-deps --features unsafe-optimizations` | clean |
| `cargo run --release --example verify_perft` (depth 6) | 41/41 passed, 74.616 s |
| `cargo run --release --example verify_perft --features unsafe-optimizations` (depth 6) | 41/41 passed, 72.836 s |

The exhaustive magic tests (`test_magic_vs_loop_bishop`,
`test_magic_vs_loop_rook`) run in both modes and compare every square × every
occupancy subset against the ray-casting reference; they are the main
correctness proof for the unchecked lookup. Depth-6 perft over all 41 corpus
positions passing in both modes is the end-to-end check.

## Benchmark: `unsafe-optimizations`

`hyperfine` and `/usr/bin/time` are not installed on this host, so timings use
the bash `date +%s%N` wall-clock around a pre-built example binary
(`target/release/examples/perft` copied per feature mode). Runs were
interleaved (default, feature, default, …) and nothing else was running.

### Starting position, depth 6 (118,926,425 nodes), 7 runs each

| Mode | Mean | Min | Max | CV |
| --- | ---: | ---: | ---: | ---: |
| default | 589.3 ms | 573 ms | 612 ms | 2.7 % |
| `unsafe-optimizations` | 573.3 ms | 556 ms | 580 ms | 1.4 % |

The 2.7 % difference is **not** conclusive here: the default mode's slowest
runs (612/601/600 ms) look like scheduler noise, and the ranges overlap.

### Perft position #13, depth 5 (53,833,718 nodes), 5 runs each

| Mode | Mean | Min | Max | CV |
| --- | ---: | ---: | ---: | ---: |
| default | 368.4 ms | 352 ms | 397 ms | 4.3 % |
| `unsafe-optimizations` | 365.8 ms | 361 ms | 370 ms | 0.9 % |

Within noise (0.7 %); the default mode again has a single 397 ms outlier.

### Perft position #13, depth 6 (2,160,817,389 nodes), 3 runs each

Position: `rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5`.

| Mode | Mean | Min | Max | CV |
| --- | ---: | ---: | ---: | ---: |
| default | 11,058.3 ms | 11,051 ms | 11,065 ms | 0.06 % |
| `unsafe-optimizations` | 10,855.7 ms | 10,840 ms | 10,885 ms | 0.2 % |

This run is long enough to be dominated by the work rather than startup or
scheduling noise: the feature is consistently **1.8 % faster** (≈203 ms over
2.16 G nodes). That is the same order as the ≈1.5 % the feedback measured on
aarch64.

**Verdict:** kept as opt-in, not default. It is a measurable win on long
search-like workloads on this host (1.8 %, CV < 0.25 %) and indistinguishable
from noise on short perft runs, which is exactly why the crate leaves it off
by default and documents the audit argument.

## Combined child key

### Equivalence

- `assert_after_equivalence` (used by the random-playout harness and the
  deterministic fixtures) now also asserts
  `board.hash_and_rule50_after(m) == (hash_after(m), rule50_after(m))` for
  every pseudo-legal move, and the existing assertions tie that pair to a
  cloned board's `do_move` + `hash()`/`rule50()`.
- New test `board::tests::hash_and_rule50_after_matches_separate`: 4 start
  positions × 8 seeds × random walks of depth 100 (>500 visited positions, all
  pseudo-legal moves at each), comparing the tuple with both separate
  functions. Passes in both feature modes.
- The deterministic fixtures cover castling (all four), en passant with a
  blast, double pushes that replace the ep target, quiet and capture
  promotions, and quiet/capture plain moves.

### Microbenchmark

A temporary example (`examples/_bench_combined.rs`, created, run, then
removed — git status shows no leftover) timed, in release mode, all
pseudo-legal moves of three positions, 5 × 200,000 iterations each, taking the
minimum of 5 measurements per variant. Separate-vs-combined was measured in
the same process, alternated, with `std::hint::black_box` on the board and
move so nothing is hoisted.

| Position | Moves | Separate | Combined | Delta |
| --- | ---: | ---: | ---: | ---: |
| `r3k2r/p1ppqpb1/...` (capture-rich middlegame) | 48 | 6.36 ns/child | 6.01 ns/child | −0.35 ns (−5.5 %) |
| `r1b2rk1/ppp3p1/...` (quiet middlegame) | 31 | 6.24 ns/child | 5.90 ns/child | −0.34 ns (−5.5 %) |
| start position | 20 | 6.24 ns/child | 5.97 ns/child | −0.27 ns (−4.4 %) |

Results reproduced across three separate process runs with < 0.1 ns variation.
The absolute numbers are dominated by this microbenchmark's own overhead (a
constant board must be `black_box`ed each iteration), so they are larger than
the consumer's ~1–2 ns/child total. The relative gain is stable and slightly
better than the ~1–2 ns/child the feedback expected: the combined accessor
saves the second origin lookup and destination test in every case.

Both of the plan's options were considered: the shared-locals version was
straightforward once `hash_after` was split into named private helpers, and it
is what shipped. There is no claim that `hash_after` itself got faster; its
code path is the same as before.

## Lazy `populate_state` — intentionally deferred

No code change. Reasons, restated from the plan and the feedback:

- `TrivialLegality::new` consumes `pinned` to build `safe_origins`; deferring
  `checkers`/`pinned` would require a different trivial-legality design, not a
  local tweak.
- The feedback is explicit that this is a measured observation, not a request:
  `has_legal_move_with_state` already runs at 5.4–14.8 ns on their fixtures
  while `populate_state` costs 8.7–31.9 ns, and their P0 work targeted
  `outcome()`/`generate_legal`, not this split.
- Nothing in the local benchmarks here isolates `populate_state` on a
  consumer-relevant workload, so there is no evidence to justify the risk.

Revisit only if a profiling pass shows `populate_state` dominating a workload
that matters.

## Non-goals (feedback §5) — confirmed not added

None of the following were added in this plan: `generate_captures` /
quiescence generators, SAN parsing/formatting, Chess960 castling or stacking
variants, `Arbitrary`/`serde`/`bincode` support, board symmetry transforms or
Miri hooks, a hyperbola-quintessence fallback, a random perft file, an
insufficient-material table, or a rules-model rewrite (packed moves,
`&mut MoveList`, `StateInfo`, and incremental hashing stay as they are).

## Problems encountered

1. **No `hyperfine`, no `/usr/bin/time`** on this host. Timings use bash
   `date +%s%N` around pre-built binaries, with interleaved runs and repeated
   measurements. The depth-6 position-#13 run was added exactly because the
   short runs were noise-dominated.
2. **Short benchmarks are noisy.** Starting-position depth 6 (~0.6 s) and
   position #13 depth 5 (~0.37 s) show overlapping ranges and occasional
   600 ms/397 ms outliers in the default mode. The report does not treat
   those as a feature win.
3. **Microbenchmark overhead.** The combined-key benchmark must re-`black_box`
   the immutable board every iteration, which inflates absolute ns/child; only
   the delta is meaningful and it is reproducible.
4. **`hash_after` had to be split to share classification.** A naive
   combined body would have called `hash_after` (which re-looks-up the piece)
   plus a second classification, saving nothing. Splitting castling out and
   threading `MoveClass` through private helpers kept both public functions
   behaviourally identical while making the single classification possible.
   The existing random-playout and fixture tests guard the refactor.

## Missing tests / remaining gaps

- The unchecked-index safety argument rests on the magic-table construction
  and the exhaustive magic tests, not on Miri (Miri cannot run meaningful
  release-codegen performance paths, and the plan lists Miri hooks as a
  non-goal). `cargo miri test --features unsafe-optimizations` was not run.
- The feature's benefit is host-specific. Only one x86_64 machine was
  available; the aarch64 figure in the feedback was not reproduced here.
- The combined-key microbenchmark was a throwaway example; there is no
  committed benchmark harness (the crate stays zero-dependency, so no
  criterion).
- `cargo tarpaulin`/coverage was not run; the new test is exercised in both
  feature modes by `cargo test`.

## Changed files

| File | Change |
| --- | --- |
| `Cargo.toml` | `[features]` with `default = []` and `unsafe-optimizations = []`. |
| `src/magic.rs` | Module safety note, cfg-gated `get_unchecked` in `bishop_attacks`/`rook_attacks`. |
| `src/board.rs` | `MoveClass`, `classify_move`, `hash_after_castling`, `hash_after_classified`, `rule50_after_classified`, `Board::hash_and_rule50_after`, combined assertions in the harness, new `hash_and_rule50_after_matches_separate` test. |
| `src/lib.rs` | Crate-doc “Feature flags” section. |
| `README.md` | Feature bullet, “Feature flags” section, combined accessor mention. |
| `CHANGELOG.md` | `Unreleased / Added` entries for the accessor and the feature. |
| `docs/plans/feedback/f2_report6.md` | This report. |
