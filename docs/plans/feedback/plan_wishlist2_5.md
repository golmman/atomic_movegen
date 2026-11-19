# Plan 5 — integration, combined measurement and release gate

## Scope

This plan adds no wishlist feature. It integrates whatever of plans 1–4
actually landed, runs the combined differential and cost measurements, pins the
new public surface, and prepares the next release. It is the only plan that
changes the release version.

Requirements served: the closing/release step of wishlist 2. The wishlist's
protocol says a release is only usable downstream when it is recorded by a plan
and a report, with the differential, gates and a bench row. This plan produces
that record for the upstream crate. It does not publish; the owner commits,
tags and publishes.

## Inputs and preconditions

Do not start until:

- `docs/plans/feedback/report_wishlist2_1.md` exists;
- each of plans 2–4 has either an implementation report saying it landed a
  measured, exact change, or a report saying it was reverted / kept open;
- the tree is green under the global gates in
  `docs/plans/feedback/plan_wishlist2_index.md`;
- the owner has confirmed that the additive API is acceptable for a minor
  release.

If no plan landed a public change, this plan still produces a short report and
does not bump the version.

## Current-tree assessment (synthesis of plans 1–4)

The expected shape before this plan runs:

| Item | Expected landed state | Expected public surface |
| --- | --- | --- |
| W6 | `Counts`, `Board::counts`, `Board::populate_counts`, docs split | additive |
| W7 | `LegalityContext` + `Board::legality_context` if the cost gate passed; otherwise internal/absent | additive or none |
| W8 | `ChildEffect`, `Board::child_effect`, `Board::child_effect_with_occupancy` | additive |
| W9 | `Board::null_for_advanced` | additive |
| Existing | `Board::legal`, generator, effect path, keys, perft, FEN, terminality | unchanged |

The integration must confirm this table from the reports, not assume it.

## Decisions and pushback

1. **A minor release only.** Every plan is additive and exactness-preserving.
   If any change altered a perft value, move order, FEN, terminality answer,
   `Always`/`Legal` key, or `ZOBRIST_VERSION`, it is not a candidate for this
   release; it must be reverted or moved to a major-version plan. There is no
   “mostly unchanged” exception.

2. **Reverted items are not mentioned in the release notes.** If plan 2's
   public context did not earn its surface, or an optional fast path did not
   measure, the report says so and the CHANGELOG omits it. The item remains
   open in the wishlist response; it is not silently claimed.

3. **No whole-binary claim.** The combined benchmark is this crate's
   `hotpath_bench`/`effect_bench` on this host. It cannot reproduce the
   downstream's ten-FEN probe or `core` run. The report may show that the new
   rows are cheaper and that perft did not regress; it must not state a
   downstream tree, node count, or wall-clock multiplier.

4. **The public-API test is a release pin, not a behavior test.** Add the new
   names to `tests/public_api.rs` so the changelog/README cannot drift from the
   code. The differential suites remain the behavior proof.

5. **No new dependency, no `unsafe`, no search API.** This plan does not relax
   the crate's engineering constraints to make a benchmark look better. If a
   measurement needs a profiler, it is an external command, not a dependency.

## Implementation

1. **Collect the landed set.** From reports 1–4, list:
   - the exact public items that landed;
   - the measured row for each item;
   - any plan that was reverted or kept open;
   - any test or benchmark file the plans added.
   Do not infer a landing from a plan that has no report.

2. **Update the public API pin.** Add the landed types/functions to
   `tests/public_api.rs`, including:
   - `Counts` construction/getters and `Board::counts`/`populate_counts`;
   - `LegalityContext`/`Board::legality_context` only if plan 2 shipped;
   - `ChildEffect` and the child-effect methods;
   - `Board::null_for_advanced`.
   The test should call each name and assert the documented cheap contract
   (for example `Counts::from(&state)` equals `board.counts()`, and
   `LegalContext::occupied()` equals `board.occupied()`), but it must not
   duplicate the differential suites.

3. **Add the combined benchmark row.** Extend `examples/hotpath_bench.rs` with
   one compact “wishlist 2 summary” section, built from the existing rows:
   - setup: `populate_state` vs `populate_counts`;
   - legality: `Board::legal` vs `LegalityContext::legal` (if landed);
   - child: `post_move_and_key_with` vs `ChildEffect` for `Always` and `Legal`;
   - null: `null_for` vs `null_for_advanced` vs the `Setup` rebuild.
   Keep the full per-class tables in the individual plans' reports; this
   summary is for the release report and must link the row names.

4. **Run the combined differential.**
   - one run over the 41-position corpus, conformance cases, effect fixtures,
     child cases and seeded play-outs, checking all landed plans simultaneously
     (counts-only legality, context equality, child-effect equality, advanced
     null equality);
   - one `cargo run --release --example verify_perft` at depth 5 minimum and
     depth 6 if time permits;
   - the existing `move_order_golden`, `verify_moves`, `conformance`,
     `effects`, `post_move`, `terminal_after`, `child_state`, `ep_key`,
     `null_for`, `interchange`, `public_api`, `robustness` suites.

5. **Run the release gates.**
   - `cargo test`
   - `cargo test --all-features`
   - `cargo test --release`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings`
   - `cargo fmt --check`
   - `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`
   - `cargo test --doc`
   - `cargo package --list`
   - `cargo publish --dry-run` (no network publish; this is the owner-approved
     dry run)
   - perft timing (`--release`, three medians) against the 2.9.0 baseline. A
     regression above ~2 % is a blocker, not a note.

6. **Update the release text.**
   - `Cargo.toml`: version `2.10.0` if at least one public item landed;
     otherwise leave the version unchanged.
   - `CHANGELOG.md`: an `Unreleased`/`2.10.0` section naming only landed items,
     with the exact contracts and the measurement summary. State explicitly
     that no existing answer, key, move order, perft value, FEN or terminality
     changed, and that `ZOBRIST_VERSION` is unchanged.
   - `README.md`: the dependency snippet (if versioned), the hot-path section,
     and any new API examples. Keep the documented 2.x key-stability contract.
   - `src/lib.rs`: the root exports and the engineering-contract allocation
     list already updated by the individual plans; verify they are complete.

7. **Package check.** Confirm the `include` list still contains the source,
   tests, markdown/data and no examples; confirm the new public items are in
   the packaged source; confirm no new dependency entered `Cargo.lock`.

8. **Produce the owner checklist.** It is not a publish action:
   - review the combined benchmark table;
   - review the diff and the changelog;
   - tag and publish from the owner's account;
   - update any downstream pin/provenance file outside this repository.

## Tests and measurement summary

The combined differential is the release proof. It must report:

- number of positions and candidate moves checked;
- number of positions with checks, pins, EP, castling, and a zero count;
- number of double pushes and legal-EP hits for the child-effect `Legal` row;
- number of advanced-null fixtures and the saturation cases;
- the combined benchmark table with absolute ns, same-machine ratios, and
  optional `perf stat` instruction counts;
- perft node counts and timings.

No new correctness test is invented here unless a landed plan left a gap; in
that case the plan's report, not this one, is amended first.

## Files to touch

- `tests/public_api.rs` — release pin for the landed API.
- `examples/hotpath_bench.rs` — the compact combined summary.
- `Cargo.toml` — version only, if a release is warranted.
- `CHANGELOG.md`, `README.md`, `src/lib.rs` — release text and exports.
- `docs/plans/feedback/report_wishlist2_5.md` — final report.
- No new dependency, no new source module, no new `unsafe`.

## Non-goals

- No feature work, no re-litigating a plan that was reverted, and no claiming a
  wishlist item that its report kept open.
- No publish, tag, commit, or downstream pin update from this repository.
- No change to the Zobrist table, `ZOBRIST_VERSION`, move order, perft, FEN,
  terminality, or the existing effect/legality answers.
- No search/evaluation/tablebase/threading API.
- No claim about a downstream search tree or another machine's wall clock.

## Final task

Write `docs/plans/feedback/report_wishlist2_5.md` with:

- the final release version (or “no release”) and the exact landed plans;
- the public API pin results;
- combined differential counts and benchmark table (absolute ns, ratios,
  optional instructions);
- the full gate results, `cargo package --list`, and
  `cargo publish --dry-run` result;
- the package diff, any problems/deviations, unresolved parts, missing tests,
  and the owner's manual publish checklist.
