# Plan 2 — integration, combined measurement and release gate

## Scope

This plan adds no wishlist feature. It integrates whatever plan 1 actually
landed, re-runs the combined differential and cost measurements, pins the new
public surface, and prepares the next release. It is the only plan that changes
the release version.

Requirements served: the closing/release step of wishlist 3 `W10`. The
wishlist's protocol says a release becomes usable downstream only through an
owner-triggered plan/report, a pin and `PROVENANCE.md` update, the
differential, the gates and a same-session `MD6` row. This plan produces the
upstream crate record only. It does not publish, tag, commit, or touch the
downstream repository or its pin/provenance files.

## Inputs and preconditions

Do not start until:

- `docs/plans/feedback/report_wishlist3_1.md` exists and states the
  measurement-gate verdict;
- if plan 1 shipped the new API, the tree is green under the global gates in
  `docs/plans/feedback/plan_wishlist3_index.md`;
- the owner has confirmed that an additive minor release is acceptable.

If plan 1 kept W10 open, this plan still produces a short report and does not
bump the version or add release notes for the item.

## Current-tree assessment (synthesis)

The expected shape after plan 1, if the gate passed:

| Item | Expected landed state | Expected public surface |
| --- | --- | --- |
| `W10` | `EvasionMask` in `LegalNode`, `Board::legality_context_with_state`, focused fixtures, bench rows | additive |
| Existing context | `Board::legality_context(counts)` and check-blind `LegalityContext::legal` unchanged | unchanged |
| Board path | `Board::legal`, `legal_in`, `legal_in_view`, generator, effects | unchanged |
| Keys/perft | `Always`/`Legal` keys, move order, FEN, terminality, `ZOBRIST_VERSION`, 41/41 perft | unchanged |

The integration must confirm this from `report_wishlist3_1.md`, not assume it.

## Decisions and pushback

1. **A minor release only.** The change is additive and exactness-preserving.
   If any existing perft value, move order, FEN, terminality answer,
   `Always`/`Legal` key or `ZOBRIST_VERSION` changed, the change is not a
   candidate for this release and must be reverted before this plan runs. There
   is no “mostly unchanged” exception.

2. **No release for an unmeasured or reverted W10.** If the state-aware context
   is exact but does not beat the current context on the in-check candidate row,
   or regresses the existing path beyond the global blocker, plan 1 keeps W10
   open. This plan then does not bump `Cargo.toml` and does not mention the
   constructor in `CHANGELOG.md`/`README.md`.

3. **The release pin is not a behavior proof.** Add the new constructor to
   `tests/public_api.rs` so the release notes cannot drift from the code. The
   differential in `tests/legality_context.rs` remains the behavior proof.

4. **No whole-binary or downstream claim.** The combined benchmark is this
   crate's `hotpath_bench` on this host. It cannot reproduce the downstream
   `core`/`easy-win`/`deep-win` runs, the `.pvr`, or the proof checker. The
   report may compare same-host instruction diagnostics with the wishlist's
   prototype; it must not state a downstream tree, node count, wall-clock
   multiplier, or acceptance rate.

5. **No new dependency, no new `unsafe`, no search API.** Nothing in the
   release gate relaxes the crate's engineering constraints. Profiling remains
   an external command, not a dependency.

6. **The absence of the cited plan-9 artifacts is recorded.** The wishlist cites
   `report9.md` and `measurements/plan9/`, which are not in this tree. The
   release notes may name W10 as the requested item, but they must not present
   those external totals as locally verified.

## Implementation

1. **Collect the landed set.** From `report_wishlist3_1.md`, list:
   - whether `Board::legality_context_with_state` shipped or W10 stayed open;
   - the exact internal mask shape and public contract;
   - the measured setup and candidate rows;
   - any test or benchmark file added;
   - any deviation from plan 1 and the measurement that motivated it.

   Do not infer a landing from a plan that has no report.

2. **Update the public API pin.** If the constructor shipped, add it to
   `tests/public_api.rs`:
   - build a populated `StateInfo` and a `Counts` value;
   - call `board.legality_context_with_state(counts, &state)`;
   - assert `occupied()` equals `board.occupied()`;
   - compare `legal(m)` with `board.legal(m, &state)` for every pseudo-legal
     move of a small in-check fixture;
   - keep the existing `legality_context` pin unchanged.
   The pin must not duplicate the full differential suite.

3. **Add the combined benchmark summary.** Extend
   `examples/hotpath_bench.rs` with one compact wishlist-3 summary built from
   the plan-1 rows:
   - `legality_context` setup versus `legality_context_with_state` setup;
   - `Board::legal` versus old `ctx.legal` versus new `evasion_ctx.legal` on
     all candidates;
   - the same three on in-check candidates, with the quiet subset called out;
   - one out-of-check quiet/all row to show the branch cost.

   Keep the full per-class tables in `report_wishlist3_1.md`; the summary is for
   the release report and must use stable row names. The timed loops must not
   construct contexts or classify moves.

4. **Run the combined differential.**
   - One run over the 41-position perft corpus, conformance cases,
     `CHILD_CASES`, effect/board fixtures, edge fixtures and seeded play-outs,
     checking both the check-blind and state-aware contexts against
     `Board::legal`. Confirm the predicted mask rejections are all illegal.
   - Confirm the focused fixtures from plan 1 still cover slider block
     inside/outside, double check, knight/pawn check, touching commoners,
     castling in check, EP in check, promotion push that blocks, and the
     second-line opening move.
   - Run the old context and board path comparisons to show no behavior drift.

5. **Run the release gates.**
   - `cargo test`
   - `cargo test --all-features`
   - `cargo test --release`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings`
   - `cargo fmt --check`
   - `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`
   - `cargo test --doc`
   - `cargo run --release --example verify_perft` (41/41; depth 5 minimum,
     depth 6 if time permits)
   - all integration suites listed in
     `docs/plans/feedback/plan_wishlist3_index.md`
   - `cargo package --list`
   - `cargo publish --dry-run` (owner-approved dry run; no network publish)
   - perft timing (`--release`, three medians) against the 2.10.0 baseline. A
     regression above ~2 % is a blocker, not a note.
   - `perf stat -e instructions:u` on the candidate probe when available and
     stable; report the command and per-call diagnostic. Do not turn the
     wishlist's instruction totals into a portable gate.

6. **Update the release text.**
   - `Cargo.toml`: version `2.11.0` only if the new constructor shipped and
     passed its gate. Otherwise leave the version unchanged.
   - `CHANGELOG.md`: a `2.11.0` section naming only landed items, with the exact
     contract and the same-host measurement summary. State explicitly that no
     existing answer, key, move order, perft value, FEN or terminality changed,
     and that `ZOBRIST_VERSION` is unchanged. Do not claim a whole-binary
     saving. If W10 stayed open, add no `2.11.0` entry for it.
   - `README.md`: the hot-path candidate-loop bullet, the snapshot/precondition
     wording, and the dependency snippet if versioned.
   - `src/lib.rs`: hot-path docs and root exports already updated by plan 1;
     verify they are complete.
   - `docs/plans/feedback/plan_wishlist3_index.md`: if the release decision
     differs from the planned 2.11.0, record the verdict in the final report
     rather than rewriting the plan retroactively.

7. **Package check.** Confirm the `include` list still contains the source,
   tests, markdown/data and no examples; confirm the new public item is in the
   packaged source; confirm no new dependency entered `Cargo.lock`.

8. **Produce the handoff and owner checklist.** It is not a publish action:
   - review the combined differential and benchmark tables;
   - review the diff, changelog and public API pin;
   - tag and publish from the owner's account;
   - downstream: trigger the owner-side `planN.md`/`reportN.md`, update the pin
     and `PROVENANCE.md`, run the differential/gates and record a same-session
     `MD6` row. None of those files live in this repository or are modified by
     this plan.

## Tests and measurement summary

The combined differential is the release proof. The report must include:

- number of positions and candidate moves checked;
- number of positions with checks, pins, EP, castling and zero counts;
- the in-check branch counts from plan 1: single slider with block, empty-block
  in check, double/leaper check, castling in check, EP in check;
- predicted mask rejections and legal moves among them (expected zero);
- the combined benchmark table with absolute ns, same-machine ratios and
  optional instruction counts;
- perft node counts and timings;
- the public API pin result.

No new correctness test is invented here unless plan 1 left a gap; in that
case `report_wishlist3_1.md` and its test suite are amended first.

## Files to touch

- `tests/public_api.rs` — release pin for the new constructor, only if shipped.
- `examples/hotpath_bench.rs` — compact wishlist-3 summary.
- `Cargo.toml` — version only, if a release is warranted.
- `CHANGELOG.md`, `README.md`, `src/lib.rs` — release text and exports.
- `docs/plans/feedback/report_wishlist3_2.md` — final report.
- No new dependency, no new source module, no new `unsafe`.

## Non-goals

- No feature work, no re-litigating a plan that was reverted or kept open, and
  no claiming W10 if its report kept it open.
- No publish, tag, commit, or downstream pin/`PROVENANCE.md`/`MD6` update from
  this repository.
- No change to the Zobrist table, `ZOBRIST_VERSION`, move order, perft, FEN,
  terminality, or the existing effect/legality answers.
- No search/evaluation/tablebase/threading API.
- No claim about a downstream search tree, `.pvr`, proof acceptance, or another
  machine's wall clock.

## Final task

Write `docs/plans/feedback/report_wishlist3_2.md` with:

- the final release version (or “no release”) and the exact landed plans;
- the public API pin results;
- combined differential counts and benchmark table (absolute ns, ratios,
  optional instructions);
- the full gate results, `cargo package --list`, and
  `cargo publish --dry-run` result;
- the package diff, any problems/deviations, unresolved parts, missing tests,
  the owner's manual publish checklist, and the downstream handoff checklist.
