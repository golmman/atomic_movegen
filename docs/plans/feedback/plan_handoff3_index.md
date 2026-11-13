# Handoff 3 — implementation plan index and sequencing

## Purpose

This index turns `docs/plans/feedback/handoff3.md` into a small set of
independently reviewable implementation plans. It is a plan, not an
implementation. Each numbered plan follows `AGENTS.md` and ends with an
implementation report (`docs/plans/feedback/report_handoff3_N.md`).

Handoff 3 supersedes `handoff2.md` as the live wishlist and contains **two**
new items, both P2, both purely additive:

| MG | Ask | Priority |
| --- | --- | --- |
| MG19 | the child's terminal state on the already-derived summary (`PostMove::terminal()`), ideally with the post-move key next to it, so one derivation yields key + terminality | P2 |
| MG20 | a *specialised*, early-exit-by-construction existential-legality test for the child ("does this side have any legal move?"), target ≤ 12 ns per child on the *built-ish* class | P2 |

Every MG1–MG14/MG15–MG18 item that has not landed stays requested; nothing in
handoff 3 re-opens a landed one. §4 of handoff 3 (non-asks) and handoff 1 §4 /
R9 are honoured verbatim: no search, evaluation, tablebase, threading or GPL
interop, and no API that exists only to serve one downstream's search design.
MG19 and MG20 are statements about *positions and moves* and are testable
inside this crate, which is the reason they are accepted here.

## What handoff 3 actually says

Handoff 2's MG15–MG18 shipped sound and are not questioned. The downstream
measured the *cost* and declined to integrate:

| Path, ns per child (their machine) | built-ish | captures |
| --- | ---: | ---: |
| today: `clone` + `do_move` + `outcome()` + key | 57.5 | 64.8 |
| build only (`clone` + `do_move`) | 32.0 | 42.4 |
| MG15: `hash_and_rule50_after` + `terminal_after` | 65.8 | 83.3 |
| `terminal_after` alone | 62.2 | — |
| `post_move` (`analyze`) alone | 36.2 | — |
| `has_legal_move_after` | 61.8 | — |

Mechanism, verbatim from the handoff: `terminal_after` re-derives the post-move
view and then runs the view movegen, so it pays the derivation *in addition to*
the build it competes with; the view movegen is not cheaper than
`do_move` + the board movegen; `terminal_from(&PostMove)` is private, so
`post_move` + terminality pays `analyze` twice; and `has_legal_move_after`
costs the same as `terminal_after`, i.e. it does not behave like an existential
query.

The whole ask is therefore: **no second derivation and no double/linear-time
movegen**. Both acceptance bars in handoff 3 §2 are *cost* bars (on top of the
soundness bars, which are already met by the shared rules code).

## Current-tree assessment (2.7.0)

| Item | Current state | Plan |
| --- | --- | --- |
| MG19 | absent. `EffectContext::terminal_from(&self, child: &PostMove)` (`src/effects.rs`) is **private** and is the single implementation behind `terminal_after`/`child_outcome`, so a consumer holding a `PostMove` cannot ask it for the child's terminal state; `post_move` + `terminal_after` pays `analyze` twice. `terminal_from` also populates a full `StateInfo` for the child (`checkers`, `pinned`, counts) although terminality needs neither pins nor a populated `StateInfo`. There is no post-move key on the summary: `Board::hash_after`/`hash_and_rule50_after` live on `&Board` only. | 1 |
| MG20 | absent. `movegen::has_legal_move_in_view` walks the pseudo-legal stream through `FastLegality` and therefore behaves like a linear candidate filter with a first-candidate exit that only pays off after the full child `StateInfo` has been populated; the child path (`EffectContext::terminal_after`, `::has_legal_move_after`, `::is_terminal_after`) always calls `view.populate_state(&mut state)` first. Nothing states the existence question as a positive, cheaply-decidable claim ("here is one demonstrably legal move"). | 2 |
| MG15–MG18 | landed in 2.7.0; handoff 3 §4 says they need nothing further at their current cost. | — |
| MG1–MG14 | landed by 2.6.0 except MG6, which the 2.5/2.6 report closed as "partly done: profile-led, no dominant cost found". No new plan: handoff 3 adds no new measurable target there. | — |
| Handoff 2 next steps that overlap | `report_handoff2_3.md` "Next steps" 1 (lazy-state child path) and 3 (a consumer-facing summary query) are exactly MG19/MG20; the plans below supersede them item by item. Items 2, 4, 5, 6 of that list are not in handoff 3 and are not planned. | — |

## Indicative cost split measured in this repository

An indicative probe (not a benchmark entry; `--release`, one `deep-win` child
set of 32 pseudo-legal children, median of 9 batches; absolute numbers are only
meaningful on the machine that produced them):

| Row | ns |
| --- | ---: |
| `clone` + `do_move` | 21.0 |
| `clone` + `do_move` + `populate_state` | 25.1 |
| `populate_state` alone | 7.4 |
| — of which `checkers` | 6.0 |
| — of which commoner counts | 1.5 |
| `has_legal_move_with_state` (state given) | 10.4 |
| `has_legal_move` (populates internally) | 21.9 |
| `ctx.post_move` | 15.1 |
| `ctx.terminal_after` | 36.2 |
| `ctx.has_legal_move_after` | 33.8 |
| `post_move` + `terminal_after` | 52.9 |
| `clone` + `do_move` + `terminal()` | 31.5 |
| `board.hash_and_rule50_after` | 6.2–14.0 (noisy) |

Consequences that shape the plans:

1. `terminal_after` ≈ `post_move` + `populate_state` + existence. Making the
   terminal path `PostMove`-local (plan 1) removes one `analyze`; **dropping the
   full `StateInfo` population** from the terminal path removes most of the
   rest. Both are needed for a win — exactly what handoff 3 §2 predicts.
2. The existential half of the child path is `populate_state` (~7 ns) +
   ladder walk (~10 ns). Plan 2 targets the *sum*: a fact tier that computes
   only commoner counts + `checkers` (~7 ns together) plus a positive
   "one demonstrably legal quiet move exists" screen that exits on the first
   hit.
3. `hash_and_rule50_after` is a per-move classification plus the delta; an
   incremental delta computed *from the already-derived child* is a small
   handful of XORs. That is the only shape in which a post-move key can be
   "free" (MG19's ceiling argument depends on it).

The handoff's absolute numbers (12 ns, 36.2 ns, 57.5 ns) are downstream-machine
numbers; this repository's machine is ~2× faster on `post_move`. The plans
therefore state acceptance as **ratios measured in the same run** (per-child
cost ÷ the build-only row of the same run), and report absolute ns only as
information.

## Plan set

1. **`plan_handoff3_1.md` — the child's terminal state on the summary (MG19).**
   `PostMove::terminal()` as the single implementation (replacing the private
   `terminal_from`), a light terminal path that needs no populated `StateInfo`
   (`position::terminal_from_facts` + `terminal_in_view_light`), and
   `EffectContext::post_move_and_key` giving the post-move key together with
   the summary from one derivation. `terminal_after`, `child_outcome`,
   `is_terminal_after` and `has_legal_move_after` are re-pointed at these
   without behaviour change. Medium risk (terminal precedence refactor; the
   board-side `terminal`/`outcome` precedence must stay byte-identical).
2. **`plan_handoff3_2.md` — the specialised existential test (MG20).**
   `movegen::has_legal_move_in_view_light(view, checkers)`: commoner counts +
   the caller's `checkers`, a positive quiet-move screen with an early exit,
   and the existing exact ladder as the fallback. Wired into
   `terminal_in_view_light`, `has_legal_move_after` and `is_terminal_after`.
   The exactness bar (`!has_legal_move_in_view(..)` on every corpus child) is
   kept by construction: the screen is a *sufficient* condition only.
   Medium risk (a new "is this move legal" argument in the crate; it must be
   justified from the existing `WidenedLegality` facts, not re-derived).
3. **`plan_handoff3_3.md` — the measured stack: benchmark harness, docs,
   release 2.8.0.** The downstream gates MG19+MG20 as *one stack* and only
   against measured numbers, so this plan builds the measurement: the
   `effect_bench` child-class breakdown (quiet / capture / promotion / en
   passant / castling / commoner move, plus the aggregated *built-ish* class),
   the composite rows the handoff's table is made of, the documentation
   (README / `lib.rs` / `CHANGELOG` / `docs/perf/perf_guide.md`), the
   public-API and conformance tests, and the 2.8.0 release checklist — including
   the **honest verdict** section: what the stack measured, whether it cleared
   the handoff's bars, and no claim about any downstream tree.

Handoff 3 has no item that justifies more than these three plans, and none of
the three may grow a fourth item "while we are here".

## Dependency order

```text
plan 1 (MG19: PostMove::terminal, light terminal path, keyed summary)
   |
   +--> plan 2 (MG20: has_legal_move_in_view_light; rewire plan 1's path)
           |
           +--> plan 3 (stack measurement, docs, release 2.8.0)
```

- Plan 1 lands first: it is the API the handoff asks for and it is
  self-contained (the light terminal path uses today's exact existence query
  until plan 2 rewires it).
- Plan 2 depends on plan 1's `terminal_from_facts`/fact tier: the existential
  test must *reuse* the `checkers` value the terminal path already computed
  rather than compute it twice.
- Plan 3 is last: it can only measure the pair once both are in the tree, and
  its release step names the final API in the CHANGELOG.
- Nothing else may be in flight while `position.rs` or `board.rs` fixtures are
  red.

## Release strategy

All items are additive → a single **2.8.0** after plans 1–3. No plan may change
move order, perft values, hash values, FEN output, or `Outcome`/`Terminal`
semantics.

- **No deprecation, no removal.** MG15–MG18 stay as they are; the new paths are
  cheaper implementations behind the same contracts, plus two additive entry
  points (`PostMove::terminal`, `EffectContext::post_move_and_key`).
- **No promise.** The 2.8.0 report publishes the measured composite for the
  *built-ish* and *captures* classes and states plainly whether it clears
  handoff 3's bars. If it does not, the report says so and handoff 3 §3's
  measured no-go stands untouched: no re-proposal from the crate side without
  numbers.
- **Patch escape hatch.** If plan 1 lands and plan 2 slips, plan 1's API may
  ship as 2.8.0 alone (it is additive and its contract is unchanged); the
  measured stack verdict then moves to the 2.9.0 report. The index's reports
  must name which of the two shapes shipped.

## Global gates for every plan

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc` (no warnings; `cargo test --doc` for new doctests)
- `cargo run --release --example verify_perft` (41/41; depth 5 minimum, depth 6
  when time permits)
- `cargo test --test perft_tests`, `--test move_order_golden`,
  `--test verify_moves`, `--test conformance`, `--test effects`,
  `--test terminal_after`, `--test child_state`, `--test public_api`
- `cargo test --features unsafe-optimizations` when touching magic/attack or
  the Zobrist-key helpers
- `cargo run --release --example effect_bench 9 2000` (three runs, medians)
  **before and after** any change to the child path
- **Perft timing** (`--release`, three runs, median) for any change that
  touches `Board::legal`, `populate_state`, the generator, the ladder, or the
  `has_legal_move`/`terminal` paths: a regression above ~2 % is a blocker, not
  a note.
- No new dependency, no new `unsafe` (the existing `unsafe` stays confined to
  `unsafe-optimizations` and the documented `piece_key_raw`), no GPL interop, no
  variant framework, no search/eval/threads/network, no API that only one
  downstream's search design needs.

## Non-goals (handoff 3 §4, handoff 2 §4, handoff 1 R9)

Do not add: a search/eval/tablebase/threading API, a general
insufficient-material rule, a second public position representation, a generic
"variant rules" framework, GPL interop, a "prover" concept in any name or field,
"child board replacement" claims of any kind, or a public constructor for
`PostMove`/`PositionView`.

Also out of scope for these plans (explicitly): re-deriving or re-documenting
MG16–MG18, changing `Board::terminal`/`Board::outcome` to the light path
(measured separately later, if ever), adding `PostMove::key()` by storing the
key in every summary without a measurement that says it is free, and any
change to the downstream's protocol.

## Final task for each plan

Write `docs/plans/feedback/report_handoff3_N.md` with item-by-item status, the
public (or crate-private) API as implemented, differential counts (visited
positions and moves), the benchmark table measured in the same run (absolute ns
**and** ratio to the build-only row), changed files, problems encountered,
deviations from the plan with the measurement that motivated them, unresolved
parts, missing tests and next steps.
