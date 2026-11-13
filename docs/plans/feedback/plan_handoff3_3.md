# Plan 3 — the measured stack, documentation, release 2.8.0

## Scope

Plan 3 is the plan that decides whether handoff 3 was answered. Plans 1 and 2
add the shapes; this plan measures them the way the downstream's own table is
made, publishes the numbers, and releases.

1. **A measurement harness that reproduces handoff 3's table.** Extend
   `examples/effect_bench.rs` with a per-child-class breakdown and the composite
   rows, so "MG19 + MG20 as one stack" can be compared to "today's path" on the
   same corpus, in the same run, on the same machine.
2. **The stack verdict.** A rules-based statement in `CHANGELOG.md` and in the
   report of what the stack measured and whether it cleared handoff 3 §2's cost
   bars — including the honest "it did not" case.
3. **Documentation.** README, `src/lib.rs`, `CHANGELOG.md`,
   `docs/perf/perf_guide.md`.
4. **API expectations.** `tests/public_api.rs`, `tests/conformance.rs` and the
   doc examples updated for the 2.8.0 surface.
5. **Release 2.8.0**, with the checklist this repository used for 2.7.0.

**Depends on plans 1 and 2.** If plan 2 slips, this plan still runs with plan
1 alone and reports the smaller stack (see "Verdict rules", branch C).

## Current-tree assessment

- `examples/effect_bench.rs` reports, for three positions only (`deep-win`,
  `middlegame-13`, `middlegame-21`), aggregate per-pseudo-legal-move and
  per-child rows: `terminal_after`, `child_outcome`, `is_legal` +
  `terminal_after`, `post_move`, `clone`+`do_move`+`terminal`, `clone`+`do_move`
  +`has`, legal-list loops, and a decided-share walk. It has **no** per-child-
  class split, no `post_move` + `terminal()` composite row, no keyed row, and no
  capture/promotion/en-passant/castling/commoner breakdown — i.e. it cannot
  produce the table handoff 3 §1 is made of.
- 2.7.0's measured numbers in this repository's own report
  (`report_handoff2_3.md`) are from the same three positions and the same
  aggregation; they are the baseline this plan extends rather than replaces.
- The public surface after plans 1–2: `PostMove::terminal`,
  `EffectContext::post_move_and_key`, plus the re-pointed (unchanged-contract)
  child queries. No new type, so `tests/public_api.rs`'s name list grows by two
  entries.
- Release machinery exists and is proven: `Cargo.toml` version bump,
  `CHANGELOG.md` section, `README.md` dependency snippet, `cargo package
  --list`, `cargo publish --dry-run`, `verify_perft`, `verify_san`.

## Decisions and pushback

1. **Measure classes, not averages.** Handoff 3's demand is explicitly about
   the *built-ish* class and about captures. A single "per child" number over
   mixed children cannot answer it. The classes are:
   - `quiet` — no casualties, not castling, not en passant, not a promotion,
     origin is not a commoner;
   - `capture` — casualties non-empty and not en passant;
   - `promotion` — `MoveType::Promotion` (quiet or capturing);
   - `en-passant` — `MoveType::EnPassant`;
   - `castling` — `MoveType::Castling`;
   - `commoner-move` — origin square holds a commoner of the side to move;
   - **`built-ish` = `capture` ∪ `promotion` ∪ `en-passant` ∪ `castling` ∪
     `commoner-move`** (handoff 3 §1's definition), reported as its own row.

   A move may belong to several classes (a capturing promotion is in both
   `capture` and `promotion`); every move of the corpus is in exactly one of
   `quiet` / the five others, and in `built-ish` iff it is not `quiet`.
2. **Reproduce their methodology, not their tree.** One `populate_state` and
   one `EffectContext` per *node*, then all rows over that node's pseudo-legal
   children; children collected from seeded play-outs over the 41-position perft
   corpus (and the bench positions), best-of-`repeats` batches, medians. Report
   the number of children per class (the handoff's 7,864 built-ish children over
   39,665 moves is the order of magnitude to expect). This crate's tree is
   **not** their tree; the report must say so, as `report_handoff2_3.md` already
   does for the decided share.
3. **Ratios in the same run are the acceptance currency.** Absolute ns differ
   by ~2× between this machine and theirs (index, "cost split"). Every
   acceptance row is therefore published as *both* ns and a ratio to the
   build-only row (`clone` + `do_move`) measured in the same run;
   handoff 3's 12/32/57.5 ns become 0.375×/1.0×/1.80× reference points.
4. **The benchmark is not a test.** No timing assertion lands in the test suite
   (flaky, and the crate has no benchmark dependency). The harness is an
   example; the verdict is prose in the report and the `CHANGELOG`.
5. **No claim about the downstream's wall clock.** Handoff 3 §3's integration
   gate (5 M-node `deep-win` run) is theirs. The crate publishes the pair's
   cost; it does not promise a win, and the `CHANGELOG` must not read as a
   performance guarantee for a consumer.
6. **Nothing is deprecated in this plan.** MG15–MG18 keep their names, so a
   downstream that already integrated them gains the speed-up only, and one that
   did not gains the shapes.

## The measurement harness

`examples/effect_bench.rs` gains one section (keep every existing row and its
format; the report diffs against them):

```text
per child, by class (<class>: children <n>):
  today:   clone+do_move+outcome+key        <ns>   (<ratio>× build)
  build:   clone+do_move                    <ns>   (1.00×)
  mg15:    hash_and_rule50_after+terminal_after  <ns>   (<ratio>×)
  summary: post_move                        <ns>   (<ratio>×)
  mg19:    post_move + PostMove::terminal()  <ns>   (<ratio>×)
  stack:   post_move_and_key + terminal()    <ns>   (<ratio>×)
  exist:   has_legal_move_after             <ns>   (<ratio>×)
  two-level: child_outcome                  <ns>   (<ratio>×)
```

Class rows in the order `quiet`, `capture`, `promotion`, `en-passant`,
`castling`, `commoner-move`, `built-ish`, `all`. Implementation notes:

- The child list per node is materialised once (`Vec<(Move, Class)>`) so the
  benchmark loop does not classify per iteration.
- "key" in the `today` row is `board.hash_and_rule50_after(m)`, i.e. the same
  work the downstream's baseline pays for the child key.
- "`terminal()`" in the `mg19`/`stack` rows is `post.terminal()` on the summary
  the row already derived (one derivation, no second `analyze`).
- `repeats`/`iterations` stay CLI arguments; document in the file header that
  class rows use fewer iterations for the smaller classes (bounded by a total
  time budget that is printed).
- Keep the decided-share walk; add the number of children per class to its
  output.

## Verdict rules

The report and `CHANGELOG.md` must state one of these, with the measured
numbers, and nothing softer:

| Branch | Condition (same run, `built-ish` class and `capture`) | What the report/`CHANGELOG` says |
| --- | --- | --- |
| A | stack ≤ 0.95× `today` on `built-ish` **and** on `capture` | handoff 3's bars are met *on this crate's corpus and machine*; the numbers and the ratios quoted; the downstream's gate stays theirs to run |
| B | stack < `today` but > 0.95× on either class | "measurably cheaper, short of the handoff's target", with the ratio; no claim of clearing the bar |
| C | plan 2 not in the release (plan 1 only) | "MG19 only: removes the second derivation; terminality alone is not cheaper than the build" — the index's patch escape hatch |
| D | stack ≥ `today` | "MG19 + MG20 do not clear the cost bars on this corpus"; name the class and the dominating cost; no re-proposal without numbers (handoff 3 §3) |

In every branch: no API is removed, no item is withdrawn, and the report names
what a future plan would have to attack (e.g. `analyze` itself).

## Documentation pass

- `README.md` — feature bullets for `PostMove::terminal` and
  `post_move_and_key`; one example showing the downstream's composite
  (`let (post, key) = ctx.post_move_and_key(&b, m)?; let terminal =
  post.terminal();`); the honest paragraph updated with this release's measured
  numbers **and** the ratios (the paragraph already says the effect path is not
  a replacement for `Board::legal` in the generator — keep that sentence);
  dependency snippet `atomic-movegen = "2.8.0"`.
- `src/lib.rs` — effect section: the summary now carries the terminal state and
  can be taken with the child key; the allocation-free list gains both items.
- `CHANGELOG.md` — `[2.8.0]` with Added (`PostMove::terminal`,
  `EffectContext::post_move_and_key`), Changed ("the child existence query no
  longer populates a `StateInfo`; no answer changed") and a "Measured" block
  quoting the class table and the branch of "Verdict rules". The
  `[Unreleased]` entries from plans 1–2 are folded in.
- `docs/perf/perf_guide.md` — update "Where to look in this crate" with the
  child path's new shape (`terminal_in_view_light`, `quiet_legal_witness`) and
  the new measured starting points.

## Tests and conformance

- `tests/public_api.rs` — the 2.8.0 import surface: `PostMove::terminal`,
  `EffectContext::post_move_and_key`, `Send + Sync` assertions for the affected
  types, and a doctest-shaped usage of the composite (the shape the README
  advertises must compile).
- `tests/conformance.rs` / `conformance::CHILD_CASES` — unchanged rows must
  still verify; optionally add one row per branch whose child key is recorded
  once (frozen `(fen, uci, AfterState, child legal count, key)`), so a future
  key regression is a conformance failure rather than a silent drift. Optional:
  only if the values are generated and reviewed.
- `cargo test --doc` — the two new doctests (`PostMove::terminal`,
  `post_move_and_key`) must be green; README snippets are not doctests in this
  crate (see `report_handoff2_3.md`, deviation 4) — re-verify them by extracting
  the ```rust``` blocks into a temporary integration test, as that report did.
- No timing assertion anywhere.

## Release checklist (2.8.0)

In the order that keeps a failure cheap:

1. `cargo test`, `cargo test --features unsafe-optimizations`,
   `cargo test --release --test effects --test terminal_after --test
   child_state --test post_move`.
2. `cargo clippy --all-targets -- -D warnings` (and with
   `--features unsafe-optimizations`), `cargo fmt --check`, `cargo doc`.
3. `cargo run --release --example verify_perft 5` and `6`; `verify_san`;
   perft timing three runs, median.
4. `cargo run --release --example effect_bench 9 2000` three runs → the class
   table and the verdict branch.
5. `Cargo.toml` → `2.8.0`; `CHANGELOG.md` `[2.8.0]` dated; README snippet;
   `[Unreleased]` empty.
6. `cargo package --list` (no `docs/`, no `Fairy-Stockfish/`, no scratch
   examples) and `cargo publish --dry-run`.
7. Report `docs/plans/feedback/report_handoff3_3.md`.

This agent does not run writing git commands (`AGENTS.md`): commit, tag and
`cargo publish` stay with the maintainer, and the report says so.

## What the downstream should re-run (report section)

- Re-point at the **published** 2.8.0 (no path dependency).
- Replace `post_move` + `terminal_after` with
  `post_move_and_key` + `PostMove::terminal()` (one derivation), and
  `clone`+`do_move`+`outcome()`+key with that composite where the child board is
  not needed.
- Re-run their own 1,248-position / 39,665-move measurement and the
  `deep-win`-gated stack test; this crate's numbers are not a predictor of
  theirs.
- Expect no API break: `terminal_after`, `child_outcome`, `post_move`,
  `is_terminal_after`, `has_legal_move_after` are unchanged in name and answer.

## Files to touch

- `examples/effect_bench.rs` — class breakdown, composite/keyed rows, methodology
  notes in the file header.
- `README.md`, `src/lib.rs`, `CHANGELOG.md`, `docs/perf/perf_guide.md`.
- `Cargo.toml` (version), `tests/public_api.rs`, optionally
  `src/conformance.rs` + `tests/conformance.rs`.
- `docs/plans/feedback/report_handoff3_3.md` — final report.

## Non-goals

- No new API item beyond plans 1–2; no deprecation; no rename.
- No timing test, no benchmark dependency (no `criterion`), no CI/perf harness
  scaffolding beyond the existing example.
- No claim about any downstream wall clock, tree or gate; no "prover" concept in
  any name or field.
- No re-litigation of MG15–MG18 (handoff 3 §4) and no plan for the
  handoff 1/2 residue (MG6).
- No change to move order, perft values, hash values, FEN output or
  `Outcome`/`Terminal` semantics.

## Final task

Write `docs/plans/feedback/report_handoff3_3.md`: the class table (children per
class, ns and ratios per row), the verdict branch with its numbers, the
documentation and release record (every command and result), the public API as
shipped, what the downstream should re-run, deviations, problems, unresolved
parts, missing tests and next steps.
