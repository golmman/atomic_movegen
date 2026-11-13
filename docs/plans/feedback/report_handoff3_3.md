# Implementation report — handoff 3 plan 3 (the measured stack, docs, release 2.8.0)

Status: **implemented and released as 2.8.0 (in-tree; the git tag and
`cargo publish` stay with the maintainer). Verdict branch D: the stack does not
clear handoff 3's cost bars.**

The full stack shipped — plan 1 (`PostMove::terminal`,
`EffectContext::post_move_and_key`, the light terminal path) **and** plan 2 (the
specialised existence screen) — so this is the index's "both plans" shape, not
the patch-escape-hatch (branch C) shape. Measured with the extended harness, in
the same run, on this crate's corpus:

| Row (per child, medians of three runs) | `built-ish` | `capture` |
| --- | ---: | ---: |
| today: `clone`+`do_move`+`outcome()`+key | 42.5 ns (2.11× build) | 54.3 ns (1.86×) |
| stack: `post_move_and_key` + `PostMove::terminal()` | **52.8 ns (2.63×)** | **63.0 ns (2.16×)** |
| stack ÷ today | **1.24×** | **1.16×** |

Branch D applies (`stack ≥ today` on both classes). The report states plainly,
in the "Verdict" section and in `CHANGELOG.md`, that MG19 + MG20 are **not**
cheaper than the build path on this corpus, and names the dominating costs. No
API was withdrawn, no item re-proposed, no claim about any downstream made.

No move order, perft value, hash, FEN output or `Outcome`/`Terminal` semantics
changed by this plan: it adds a benchmark section, documentation, tests, and the
version bump. Version in the tree: `2.8.0`.

## Item status

| Plan item | Status | Where |
| --- | --- | --- |
| Measurement harness: per-child-class breakdown + the composite rows | **done** | `examples/effect_bench.rs` |
| Class definitions (`quiet`, `capture`, `promotion`, `en-passant`, `castling`, `commoner-move`, `built-ish`, `all`) | **done** | `examples/effect_bench.rs` (`class_mask`, `CHILD_CLASSES`) |
| Corpus: seeded play-outs over the 41-position perft corpus + the 3 bench positions | **done** (49,338 children / 2,106 nodes) | `examples/effect_bench.rs` (`collect_nodes`, `perft_corpus_fens`) |
| Children-per-class in the decided-share output | **done** | `examples/effect_bench.rs` (`decided_share`) |
| Stack verdict with measured numbers | **done — branch D** | `CHANGELOG.md` "Measured", this report |
| `README.md` (bullets, composite example, honest paragraph + ratios, `2.8.0` snippet) | **done** | `README.md` |
| `src/lib.rs` effect section (measured-cost paragraph) | **done** | `src/lib.rs` |
| `CHANGELOG.md` `[2.8.0]` (Added / Changed / Measured; `[Unreleased]` folded in and empty) | **done** | `CHANGELOG.md` |
| `docs/perf/perf_guide.md` (child path shape + measured starting points + class table) | **done** | `docs/perf/perf_guide.md` |
| `tests/public_api.rs` 2.8.0 surface + the README composite shape | **done** (4 tests) | `tests/public_api.rs` |
| `conformance::CHILD_CASES` key column | **not done** (optional; see deviations 5) | — |
| Release 2.8.0 checklist | **done** (recorded below) | `Cargo.toml`, `CHANGELOG.md` |

## The measurement harness

`examples/effect_bench.rs` keeps every existing row and its format and gains one
section (`per_class_table`) at the end:

- The corpus is materialised once per run: for each start FEN (41 from
  `tests/perft_values.md` + the 3 bench FENs) and each of 2 seeds, a seeded
  play-out of up to 24 plies. Every visited node keeps its `Board`, its
  `EffectContext`, and its **pseudo-legal children with a pre-computed class
  mask** (`Vec<(Move, u8)>`), so the timing loops classify nothing.
- Per class, the eight rows of the plan's table are timed over that class's
  children:
  `today` (`clone`+`do_move`+`outcome()`+`hash_and_rule50_after`), `build`
  (`clone`+`do_move`, the divisor), `mg15`
  (`hash_and_rule50_after`+`terminal_after`), `summary` (`post_move`), `mg19`
  (`post_move`+`PostMove::terminal()`), `stack` (`post_move_and_key`+
  `terminal()`), `exist` (`has_legal_move_after`), `two-level`
  (`child_outcome`).
- Each class gets `rounds` rounds; each round times every row once (one batch of
  `iterations` calls), so a slow patch of the machine biases all rows of the
  class alike. The reported value is the median across rounds, with
  sub-nanosecond resolution (the pre-existing per-position rows keep their
  integer-ns `bench`).
- Batches are sized to a printed time budget (CLI arg 3, default 4000 ms, capped
  by the `iterations` argument so the class rows use the same batch size as the
  rest of the file). Every class also prints its child count; the section prints
  the iterations used and the time spent.

Usage is unchanged plus the optional budget:

```sh
cargo run --release --example effect_bench [repeats] [iterations] [budget-ms]
```

## The class table

`cargo run --release --example effect_bench 9 2000`, **three runs, medians**.
Corpus: **49,338 children over 2,106 nodes**; 13,926 children (**28.2 %**) are
`built-ish` (handoff 3's downstream counts 7,864 of 39,665 = 19.8 %, so the
order of magnitude matches; the share differs, see deviations 6).

ns per child, and the ratio to the class's `build` row of the same run
(`build` in the second column, unratioed):

| class | children | today | build | mg15 | summary | mg19 | stack | exist | two-level |
| --- | ---: | --- | --- | --- | --- | --- | --- | --- | --- |
| quiet | 35,412 | 37.9 (2.38×) | 15.9 | 38.3 (2.41×) | 17.8 (1.12×) | 32.1 (2.02×) | 43.5 (2.74×) | 28.3 (1.78×) | 34.8 (2.19×) |
| capture | 3,972 | 54.3 (1.86×) | 29.2 | 61.5 (2.11×) | 36.4 (1.25×) | 49.7 (1.70×) | 63.0 (2.16×) | 47.4 (1.62×) | 53.3 (1.83×) |
| promotion | 432 | 60.5 (3.56×) | 17.0 | 65.4 (3.85×) | 21.2 (1.25×) | 58.4 (3.44×) | 69.9 (4.11×) | 55.6 (3.27×) | 61.0 (3.59×) |
| en-passant | 8 | 46.0 (1.83×) | 25.1 | 53.4 (2.13×) | 21.1 (0.84×) | 45.2 (1.80×) | 57.8 (2.30×) | 36.7 (1.46×) | 49.1 (1.96×) |
| castling | 80 | 33.5 (2.15×) | 15.6 | 32.9 (2.11×) | 15.2 (0.97×) | 29.7 (1.90×) | 42.3 (2.71×) | 28.0 (1.79×) | 35.2 (2.26×) |
| commoner-move | 10,052 | 37.6 (2.38×) | 15.8 | 37.1 (2.35×) | 18.6 (1.18×) | 31.2 (1.97×) | 41.6 (2.63×) | 28.8 (1.82×) | 39.8 (2.52×) |
| **built-ish** | 13,926 | **42.5 (2.11×)** | 20.1 | 44.6 (2.22×) | 23.6 (1.17×) | **39.7 (1.98×)** | **52.8 (2.63×)** | 33.4 (1.66×) | 43.1 (2.14×) |
| all | 49,338 | 39.5 (2.34×) | 16.9 | 39.4 (2.33×) | 19.4 (1.15×) | 32.8 (1.94×) | 44.6 (2.64×) | 30.5 (1.80×) | 38.0 (2.25×) |

Ratios of the acceptance rows to `today`, per run (so the spread is visible):

| class | stack ÷ today (run 1 / 2 / 3) | mg19 ÷ today |
| --- | --- | --- |
| quiet | 1.24 / 1.15 / 1.14 | 0.85 |
| capture | 1.16 / 1.15 / 1.17 | 0.92 |
| built-ish | 1.25 / 1.23 / 1.24 | 0.93 |
| all | 1.12 / 1.14 / 1.13 | 0.83 |

Reference points from the index (handoff 3's numbers divided by its 32 ns
build-only row): 12 ns = 0.375×, 32 ns = 1.00×, 57.5 ns = 1.80×. On this
machine the clone path is **2.11×** the build and the stack **2.63×**.

Children per class in the *decided-share* walk (8 seeds × depth 60 × 4 start
FENs, now printed by the harness): 47,884 pseudo-legal children — quiet 37,700
(78.7 %), capture 4,416 (9.2 %), promotion 0, en-passant 4, castling 104,
commoner-move 5,872 (12.3 %), `built-ish` 10,184 (21.3 %).

Per-position context rows (medians over the three positions and three runs;
these are the 2.7.0 rows, unchanged in shape): `post_move` 21, `post_move`
derivation only 17, `clone`+`do_move` 15, `terminal_after` 30,
`clone`+`do_move`+`terminal` 28, `post_move`+`terminal` 30, `post_move_and_key`
25, `hash_after` 6, `child_outcome` 35, `is_legal`+`terminal_after` 51,
`generate_legal` 111, `for_each_legal` 69, `has_legal_move` 2 ns. Shares vs
build only: existential 9–12 ns (0.50–0.86×), terminality 11–15 ns
(0.61–1.08×).

## Verdict

**Branch D — "MG19 + MG20 do not clear the cost bars on this corpus."**

The stack (`post_move_and_key` + `PostMove::terminal()`, one derivation yielding
summary, key and terminality) costs **1.24× the clone path on `built-ish`**
(52.8 vs 42.5 ns) and **1.16× on `capture`** (63.0 vs 54.3 ns), and 1.11–1.26×
on every other class, in three consecutive runs. Branch A needed ≤ 0.95× on both
classes; branch B needed `< today`; neither holds.

The dominating costs, from the same run's marginal differences (differences of
medians, so they include any interference between the two pieces of work):

| Component | `built-ish` | `capture` | `all` |
| --- | ---: | ---: | ---: |
| `analyze` (the derivation, `post_move`) | 23.6 | 36.4 | 19.4 |
| child-side existence (`exist` − `summary`) | 9.8 (0.49× build) | 11.0 (0.38×) | 11.1 (0.66×) |
| terminality on the summary (`mg19` − `summary`) | 16.1 (0.80×) | 13.3 (0.46×) | 13.4 (0.79×) |
| key + terminality inside the stack (`stack` − `summary`) | 29.2 | 26.6 | 25.2 |
| the clone path's facts (`today` − `build`: `outcome()` + key) | 22.4 | 25.1 | 22.6 |

1. **The derivation is not cheaper than the board write.** `analyze` costs
   23.6 ns on `built-ish` where `clone` + `do_move` costs 20.1 ns, so no
   summary-based composite can start from an advantage — the summary API's
   value is shape (one derivation, no clone), not speed.
2. **The child-side existence question is ~5× the board-side one.** 9.8 ns from
   the placement view against `has_legal_move`'s 2–3 ns on a populated
   `StateInfo`: the board path obtains `checkers`/`pinned` from
   `populate_state`, which it needed anyway, while the child path re-derives the
   fused fact pass and then searches for a witness. This is plan 2's finding,
   now quantified per class (MG20's own 0.40× bar is met on `capture` at 0.38×
   and missed on `built-ish` at 0.49×).
3. **The key inside the composite is not the "small handful of XORs" the index
   priced it as.** `stack` − `mg19` ≈ 13 ns on `built-ish`, while
   `Board::hash_and_rule50_after` costs ≈ 11 ns standalone — the key's marginal
   cost in the composite exceeds a plain re-derivation on the parent board.

`MG19 alone` is a different statement and is *cheaper* than the clone path:
`post_move` + `terminal()` is 0.85–0.97× `today` per class (0.93× on
`built-ish`, 0.92× on `capture`). It is not what the downstream gates (their
composite needs the key), so it does not change the branch — but it locates the
failure in the key plus the child-side facts, not in `PostMove::terminal()`.

Per handoff 3 §3 and the index's release strategy: this is a measured no-go, the
crate does not re-propose anything, and a future proposal needs numbers.

## Documentation record

- `README.md` — dependency snippet `atomic-movegen = "2.8.0"`; the effect-path
  bullet list already named the 2.8.0 surface (added by plan 1) and keeps it;
  a new composite snippet (a `child_key_and_terminal(&board, m)` helper, a mate
  child and an undecided child, plus "the key is exactly `Board::hash_after`");
  the honest paragraph replaced with this release's measured numbers **and
  ratios** (`built-ish` 42.5 ns clone path / 52.8 ns stack = 1.24×; summary
  without the key 39.7 ns = 0.93×), keeping the sentence that the effect path is
  not a replacement for `Board::legal` in the generator; the `effect_bench` CLI
  line mentions the class table.
- `src/lib.rs` — effect section: a measured-cost paragraph (what
  `effect_bench` prints, that `analyze` costs about what `clone` + `do_move`
  costs, and that the summary is the right query when the child's *facts* are
  the question, not as a faster `Board::do_move`). `PostMove::terminal` and
  `EffectContext::post_move_and_key` were already in the allocation-free list.
- `CHANGELOG.md` — `[2.8.0] - 2026-11-13`, `[Unreleased]` folded in and left
  empty: Added (both items), Changed (existence screen, light terminal
  precedence — contracts unchanged), and a **Measured** block with the class
  table, the ratios, the verdict sentence and the explicit
  no-downstream-claim paragraph.
- `docs/perf/perf_guide.md` — "Where to look in this crate" names
  `position::terminal_in_view_light`, `child_key`, and the two screens; the
  measured starting points are updated (`post_move` 18–25 ns,
  `terminal_after`/`child_outcome` 30–40 ns, `has_legal_move_after` 25–34 ns),
  and the class table plus the "a derivation still has to beat the board write"
  paragraph are added.

## Public API as shipped (2.8.0)

Nothing was added or removed by this plan; the surface is plan 1's:

```rust
// on PostMove
pub fn terminal(&self) -> Option<Terminal>;
// on EffectContext
pub fn post_move_and_key(&self, board: &Board, m: Move) -> Option<(PostMove, u64)>;
```

Everything else (`terminal_after`, `is_terminal_after`, `has_legal_move_after`,
`child_outcome`, `post_move`, `effect_and_post`, `is_legal`, `effect`,
`classification`) is unchanged in name, signature and answer.

`tests/public_api.rs` (4 tests) now pins:

- `crate_root_reexports_resolve` — every 2.8.0 type/function resolves (now
  labelled 2.8.0; `PostMove::terminal` and `post_move_and_key` were added to it
  by plan 1 and stay);
- `readme_composite_shape_compiles_and_answers` — **new**: the exact shape the
  README advertises (`post_move_and_key` then `post.terminal()`), asserting the
  key equals `Board::hash_after`, a mate child (`Outcome::Loss`) and an
  undecided child (`None`), agreement with `terminal_after`, and `None` for
  `Move::NONE`;
- `classification_is_usable_from_a_legal_list`, `public_types_are_send_and_sync`
  — unchanged (`PostMove`, `EffectContext`, `AfterState`, `Terminal`,
  `Outcome`, `MoveEffect`, `MoveClassification` are `Send + Sync`).

README snippets were re-verified by extracting all ten ```rust``` blocks into a
temporary integration test (the procedure `report_handoff2_3.md` used) and
running them: 10/10 passed; the temporary test was deleted afterwards.

## Release record

Every command below was run in the workspace, in this order.

| # | Command | Result |
| --- | --- | --- |
| 1 | `cargo test` | 13 result blocks, all ok: lib 173, `child_state` 7, `conformance` 6, `effects` 12, `interchange` 3, `move_order_golden` 1, `null_for` 2, `perft_tests` 4, `post_move` 5, `public_api` 4, `terminal_after` 6, `verify_moves` 1, doctests 16 |
| 2 | `cargo test --features unsafe-optimizations` | same 13 blocks, all ok |
| 3 | `cargo test --release --test effects --test terminal_after --test child_state --test post_move` | 12 / 6 / 7 / 5 passed, 0 failed |
| 4 | `cargo clippy --all-targets -- -D warnings` and with `--features unsafe-optimizations` | clean (both) |
| 5 | `cargo fmt --check` | clean |
| 6 | `RUSTDOCFLAGS="-D warnings" cargo doc` | no warnings |
| 7 | `cargo test --doc` | 16 passed |
| 8 | `cargo run --release --example verify_perft 5` ×3 | 41/41 each; total 2.170 / 2.194 / 2.161 s → median **2.170 s** (2.7.0 after plan 2: 2.219 s median — no regression) |
| 9 | `cargo run --release --example verify_perft 6` | 41/41, 63.279 s |
| 10 | `cargo run --release --example verify_san 3` | 507,346 positions, 14,003,226 moves, 41/41, 6.010 s |
| 11 | `cargo run --release --example effect_bench 9 2000` ×3 | the class table above; 44–87 ms of measured batches per run |
| 12 | `Cargo.toml` → `2.8.0`, `CHANGELOG.md` `[2.8.0]` dated, README snippet, `[Unreleased]` empty | done |
| 13 | `cargo package --list --allow-dirty` | 45 files, no `docs/`, no `Fairy-Stockfish/`, no scratch examples (the temporary `examples/scratch_probe.rs` was deleted) |
| 14 | `cargo publish --dry-run --allow-dirty` | packaged 45 files, 998.0 KiB (227.4 KiB compressed), verified compile, "aborting upload due to dry run" |

`--allow-dirty` was needed because this agent does not run writing git commands
(`AGENTS.md`): **commit, tag `v2.8.0` and `cargo publish` are the maintainer's
steps.**

## What the downstream should re-run

- Re-point at the **published** 2.8.0 (no path dependency).
- Replace `post_move()` + `terminal_after()` with `post_move_and_key()` +
  `PostMove::terminal()` — one derivation instead of two — and replace
  `clone` + `do_move` + `outcome()` + key with that composite **where the child
  board is not needed for anything else**. On this crate's machine the composite
  is 1.13–1.26× the clone path, so this is an API-shape change, not a promised
  win: measure their own tree before switching.
- Re-run their 1,248-position / 39,665-move table and the `deep-win`-gated
  stack test (handoff 3 §3). This crate's numbers are not a predictor of theirs:
  different corpus (28.2 % `built-ish` here vs 19.8 % there) and a different
  machine.
- Expect **no API break**: `terminal_after`, `child_outcome`, `post_move`,
  `is_terminal_after`, `has_legal_move_after`, `effect`, `is_legal`,
  `classification` keep their names and answers.

## Changed files

- `examples/effect_bench.rs` — the per-class corpus and table (`class_mask`,
  `PRIMITIVE_CLASSES`, `CHILD_CLASSES`, `NodeEntry`, `collect_nodes`,
  `perft_corpus_fens`, `Row`, `batch_row`, `per_class_table`), children-per-class
  in the decided-share output, the optional `budget-ms` CLI argument, and a
  rewritten file header (methodology, class definitions, how to read the ratios,
  the cache caveat for small classes).
- `README.md` — `2.8.0` snippet, composite example, measured honest paragraph
  with ratios, `effect_bench` CLI line.
- `src/lib.rs` — measured-cost paragraph in the effect section.
- `CHANGELOG.md` — `[2.8.0]` with Added / Changed / Measured; `[Unreleased]`
  emptied.
- `docs/perf/perf_guide.md` — child-path shapes, updated measured starting
  points, class table, "beat the board write" paragraph.
- `Cargo.toml` — version `2.8.0`.
- `tests/public_api.rs` — 2.8.0 header, `readme_composite_shape_compiles_and_answers`.
- `docs/plans/feedback/report_handoff3_3.md` — this report.

## Deviations from the plan, and the measurements that motivated them

1. **An optional third CLI argument (`budget-ms`) and a budget bounded by
   `iterations`.** The plan says repeats/iterations stay CLI arguments and asks
   for a printed budget; a budget without a knob would be untunable, and without
   the `iterations` cap the class rows would be measured at a different batch
   size than the rest of the file. In practice the plan's default invocation
   (`9 2000`) spends 44–87 ms on the table, far below the 4 s budget, because
   `iterations` is the cap.
2. **Rows are interleaved within a class, not run one row after another.** The
   first version timed each row with all its rounds back-to-back (the shape of
   the existing per-position rows) and produced run-to-run swings of ±30 % on
   the larger classes: `built-ish` `stack` measured 54.0 ns in one run, 71.0 in
   the next, and `built-ish` `exist` 36 → 56. Interleaving the eight rows per
   round and taking the median across rounds gave 52.8 / 53.8 / 52.2 ns across
   three runs (±1 %), which is what makes branch D readable at all. Rows also
   report one decimal (the class rows use `f64` ns) instead of the integer ns of
   the pre-existing rows, which cannot resolve 2 ns differences.
3. **The `today` row's key is `Board::hash_and_rule50_after(m)`,** as the plan
   specifies; `Row::Today` therefore does `clone` + `do_move` + `outcome()` +
   `hash_and_rule50_after`, i.e. exactly the downstream's baseline (a cloned
   child keyed on the parent board).
4. **No branch-A optimism in the wording.** `mg19` (`post_move` +
   `terminal()`) is cheaper than `today` on every class (0.85–0.97×), which
   reads like branch B; the plan's branch table is defined on the *stack*
   (summary + key + terminality), which is what the downstream gates, so the
   verdict is D. Both numbers are published so the distinction cannot be lost.
5. **No key column in `conformance::CHILD_CASES`** (the plan's optional item).
   `ChildCase` is a public struct with public fields, so a new `child_key` field
   is a schema change in a measurement/doc release; the contract it would pin is
   already asserted differentially over the whole corpus by `tests/post_move.rs`
   (`key == hash_after == hash_and_rule50_after.0 == child.hash()`) and
   `tests/child_state.rs`. Recorded as a missing test rather than silently
   skipped.
6. **The class corpus is this crate's tree, not the downstream's.** Children
   come from 2 seeds × 24 plies over 44 start FENs; a different seed/depth mix
   shifts the `built-ish` share (28.2 % here, 19.8 % downstream) and the
   promotion/en-passant rows are small (432 / 8 children). The report and the
   `CHANGELOG` say so; no number here is presented as the downstream's.
7. **A pre-existing README defect was fixed:** the classification snippet
   imported `StateInfo` without using it. It surfaced only because the snippets
   were compiled as part of this plan's re-verification.

## Problems encountered

1. **The existing harness could not produce the table.** `effect_bench` had no
   class split, no composite row and no `built-ish` aggregate; all of it had to
   be written, and the corpus had to be materialised (the per-position rows
   benchmark one node at a time, which says nothing about a class).
2. **Sequential timing of rows is not usable for 5 % decisions** (deviation 2).
   The *first* reading of the stack made it look like branch B
   (49 vs 42 ns, 1.13×) on one run and like 1.3× on another; only the
   interleaved-round design gave a stable answer.
3. **The composite is superadditive, so marginal costs are approximate.** On
   `built-ish`, `summary` 23.6 + (key 13.1 + terminality 16.1) = 52.8: `stack` −
   `mg19` = 13.1 ns for the key, while `post_move_and_key` alone is only ≈ 8.6 ns
   above `post_move` in the probe (33.2 vs 24.6 ns, same run). A throwaway probe (not committed) measured four shapes of
   the composite on the same corpus — `post_move_and_key` + `terminal()`,
   the same with two separate `black_box` calls, `.map(|(p, k)| (p.terminal(),
   k, p.rule50()))`, and `post_move` + `terminal()` +
   `Board::hash_and_rule50_after` — at 53.8 / 53.3 / 52.2 / 48.0 ns against
   `today` 40.7 ns in the same run. Every shape is at or above the clone path,
   so the verdict does not depend on which composite spelling is used; the
   separate-hash spelling is ~10 % cheaper than `post_move_and_key`, which is
   called out as a next step.
4. **The "free post-move key" premise does not hold.** The index expected the
   incremental key from the already-derived child to be "a small handful of
   XORs"; measured, its marginal cost inside the composite (≈ 13 ns on
   `built-ish`) is *more* than `Board::hash_and_rule50_after`'s standalone 11 ns.
   `child_key` walks `{from} ∪ casualties` with `pop_lsb` and reads
   `board.piece_on(..)` per square; the plausible explanation is register
   pressure/code layout when it sits next to `terminal()` rather than the XOR
   count itself. Nothing was changed on that basis: plan 3's scope is
   measurement.
5. **Perft timing had to be re-taken, not remembered.** `verify_perft 5` was run
   three times interleaved with the bench; the whole-tree time is unchanged
   (2.170 s median vs 2.219 s in `report_handoff2_3.md`), which is the evidence
   that this plan touched no hot path.

## Unresolved parts

- **The cost bars are not cleared (branch D).** The stack is 1.11–1.26× the
  clone path. Per handoff 3 §3 and the index, the crate does not re-propose
  anything without numbers; a future plan would have to attack, in this order:
  1. `analyze` itself (23.6 ns on `built-ish` ≈ `clone` + `do_move`'s 20.1 ns) —
     while the derivation costs what the board write costs, no summary-based
     composite can beat the clone path;
  2. the child-side existence question (9.8 ns vs the board-side 2–3 ns) — the
     board path gets `checkers`/`pinned` from `populate_state`, the child path
     re-derives them;
  3. the key's marginal cost in the composite (≈ 13 ns) — the
     `post_move` + `terminal()` + `hash_and_rule50_after` spelling measured
     ~10 % cheaper than `post_move_and_key` + `terminal()` in the probe.
- **Decided children are still the expensive ones** (screen fails, then the
  ladder): not measured per class here, because the class split is by move type,
  not by child state. Plan 2 flagged the same gap.
- **`Board::terminal`/`outcome` were not moved to the light path** (index
  non-goal); the board path remains the reference implementation.
- **The `built-ish` share depends on the corpus recipe** (2 seeds × 24 plies);
  a different recipe shifts every row a few ns and is not published as a
  sensitivity study.

## Missing tests

- **No timing assertion anywhere** (by design: no benchmark dependency, no
  flaky CI gate). `cargo test` cannot detect a 2.5 ns inlining regression;
  `effect_bench` can, but only when someone runs it.
- **`class_mask` (the class definition itself) is untested.** A wrong class
  boundary would silently move rows between classes; the example is the only
  place the definition exists and `cargo test` does not run example unit tests.
  A future harness could assert, for the corpus, that `quiet` ⟺ no casualties
  and no special move type and no commoner origin, and that
  `built-ish` = `all` − `quiet`.
- **No per-class coverage assertion for the existence screen** (plan 2's gap):
  how often the screen fires per class is not printed, so the `exist` row cannot
  be split into "screen" and "fallback".
- **No frozen child-key conformance column** (deviation 5).
- **No assertion that the README's numbers match a fresh run.** The
  `CHANGELOG`'s table is prose; only a human diff against `effect_bench` output
  keeps it honest.

## Next steps

1. **Treat handoff 3 as measured-no-go** (branch D). Publish nothing further
   about MG19/MG20 as a performance win; the API shape is the deliverable.
2. If a future plan wants the last few ns, start from the three costs named
   above, and only with a measurement that beats `today` — not merely the
   previous effect query.
3. Extend the harness rather than the API: print the screen hit rate per class
   and a `decided` row per class, and consider the sub-nanosecond mode plan 2
   asked for (the class rows now have it; the per-position rows are still
   integer ns).
4. Downstream (handoff 3 §3): re-run their own table against the published
   2.8.0 and their `deep-win`-gated stack test; this crate's numbers are not
   portable.
5. Maintainer: `git commit`, `git tag v2.8.0`, `cargo publish` (this agent ran
   no writing git commands and used `--allow-dirty` for packaging only).
