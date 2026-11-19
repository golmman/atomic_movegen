# wishlist 2 — implementation plan index and sequencing

## Purpose

This index turns `docs/plans/feedback/wishlist2.md` into independently
reviewable implementation plans. It is a plan, not an implementation. Each
numbered plan follows `AGENTS.md` and ends with an implementation report
(`docs/plans/feedback/report_wishlist2_N.md`).

The wishlist is a hot-path cost request written against released
`atomic-movegen` 2.9.0 (`af3faf0`). This tree is that baseline. Unlike wishlist
1, every item here is about the cost and shape of existing facts, not about a
new rules answer. Therefore every plan has the same top-level contract:

- exact equality with the current oracle on the differential corpus and seeded
  play-outs;
- no change to perft values, move order, FEN output, terminality, the `Always`
  and `Legal` key tables, or `ZOBRIST_VERSION`;
- no new dependency, no new required `unsafe`, and no allocation on a claimed
  hot path;
- the existing `EffectContext`, `PostMove`, `post_move_and_key_with`, and
  `is_legal` are not extended on the hot path, as the wishlist requests.

The wishlist's own numbers are downstream-machine instruction counts. This
repository cannot make those numbers a portable release gate; the plans
therefore gate on exactness plus same-machine ns/ratio measurements, and report
retired instructions only when `perf stat` is available and stable. The
wishlist ceilings remain useful targets, not promises.

| # | Ask | Type | Priority | Plan |
| --- | --- | --- | --- | --- |
| W6 | Counts-only state, or a documented `legal` precondition | API + documentation | P3 | 1 |
| W7 | Legality entry point at occupancy cost | candidate-loop API + measurement | P1 | 2 |
| W8 | Effect + child key from parent bitboards | child API + measurement | P1 | 3 |
| W9 | Null move with advanced clock, no `Setup` rebuild | additive board method | P2 | 4 |
| — | Integration, combined measurement and release gate | release gate | final | 5 |

## Current-tree assessment (2.9.0)

| Item | Current state | Plan |
| --- | --- | --- |
| W6 | `Board::legal(m, state)` delegates to `position::legal_in_view`, whose only `StateInfo` reads are `commoners_count` and `them_commoners_count`; `legal_with_counts` likewise needs only placement. `Board::populate_state` computes the two counts plus `checkers` and `pinned`. There is no `Board::populate_counts`, no public `Counts` value, and the `legal`/`is_legal_with_state` docs still say only “state must be populated”. | 1 |
| W7 | There is no candidate-loop legality context. `Board::legal` borrows the internal view per call and `position::legal_with_counts` recomputes occupancy, the capture test, the commoner sets, and the castle/EP branches for each candidate. `FastLegality`/`WidenedLegality` are crate-private, require `checkers`/`pinned`, and are used by move generation, not as a public legality entry point. | 2 |
| W8 | `EffectContext::post_move_and_key_with` always calls `analyze`, which derives a full `PostMove` (a `PositionView`: two color sets, six type sets, side, ep, rights, clock), then `child_key_with`. The key delta itself is already incremental. There is no lean `ChildEffect`; a caller that only wants `occupied + key + rule50 + capture` pays for the view copy and the summary. | 3 |
| W9 | `Board::null_for(attacker)` clones, flips the side, clears ep, recomputes the game ply from the preserved fullmove number, keeps `rule50`, and recomputes the hash. There is no advanced-clock variant and no public clock setter. | 4 |

## What each plan does

1. **`plan_wishlist2_1.md` — W6: counts-only state.**
   Add a small public `Counts` value (`commoners`, `them_commoners`), a
   `Board::counts()` reader and a `Board::populate_counts(&mut StateInfo)` that
   writes only the two count fields. Document precisely which existing entry
   points need only those fields (`Board::legal`,
   `Board::is_legal_with_state`, `EffectContext::new`'s game-over flag) and which
   still need a full `StateInfo` (`generate_legal_with_state`,
   `has_legal_move_with_state`, and friends). `populate_state` keeps its present
   contract. This is the shared value type used by plans 2 and 3.

2. **`plan_wishlist2_2.md` — W7: legality entry point at occupancy cost.**
   Add a node-local `LegalityContext` built once from `&Board` and `Counts`,
   with `legal(m)` and an exposed `occupied()`. The rules body is the existing
   `position::legal_with_counts`, refactored into one shared parts/`LegalNode`
   implementation rather than copied. The plan does **not** retry the
   category-specialised legality split that was implemented and reverted in
   `docs/plans/perf_analysis7/report4.md`. A quiet-move fast path is optional
   and measurement-gated. The public context ships only if it is exact and
   measurably useful; the report says plainly if the ≤ 100-instruction target is
   not reached.

3. **`plan_wishlist2_3.md` — W8: effect and child key from parent bitboards.**
   Add `ChildEffect { occupied, key, rule50, capture }` and
   `Board::child_effect(m, counts, mode)`. The implementation derives the child
   occupancy and key from the parent's placement and the existing incremental
   key delta, without constructing a `PostMove` or copying a full
   `PositionView`. `Legal` en-passant canonicalisation is unchanged in
   substance: non-double-pushes do no extra work, and only a double push whose
   child stores an ep target asks the existing view-side legal-EP helper. Plan
   3's `child_effect_with_occupancy` can consume the occupancy exposed by plan
   2's `LegalityContext`; the plain `Board::child_effect` remains the
   convenience shape requested by the wishlist.

4. **`plan_wishlist2_4.md` — W9: null move with an advanced clock.**
   Add `Board::null_for_advanced(attacker)`: same pieces and castling rights,
   `attacker` to move, no ep, `rule50 + 1`, `game_ply + 1`, and an incremental
   hash update (side key toggled when the side changes, old ep key removed).
   `null_for` is unchanged. No public `set_rule50` is added in this plan; the
   dedicated method is the smaller, harder-to-misuse surface. Saturation and
   the difference from the FEN-preserving `null_for` are documented and tested.

5. **`plan_wishlist2_5.md` — integration, combined measurement and release.**
   No new feature. It runs the combined differential and benchmark, pins the
   new public API, updates `README.md`/`CHANGELOG.md`, chooses the release
   version from what actually landed, and performs the release checks. It does
   not publish; the owner commits, tags and publishes.

## Dependency order

```text
plan 1 (W6: Counts + populate_counts) ---+
                                          |
plan 2 (W7: LegalityContext) ------------+--> plan 3 (W8: ChildEffect)
                                          |       |
plan 4 (W9: null_for_advanced) -----------+-------+
                                          |
plan 5 (integration, measurement, release)|
```

- Plan 1 is self-contained and lands first. Plans 2 and 3 use its `Counts`.
- Plan 2 can land without plan 3, and plan 3 can land without the public
  `LegalityContext`: its `Board::child_effect` shape has no hard dependency on
  plan 2. The `child_effect_with_occupancy` reuse is useful only when plan 2's
  context survives its measurement gate, but the method itself is independent.
- Plan 4 is independent of plans 1–3; it does not need `Counts` and can land
  at any point in the release train.
- Plan 5 runs last and is the only plan that changes the release version. It
  must not start until the reports for the landed plans exist.

## Release strategy

- **2.10.0** for the additive API that passes its measurement gate. This is a
  minor release: no existing answer changes, no key changes, no move-order or
  perft change. `Counts`, `LegalityContext`, `ChildEffect` and
  `null_for_advanced` may all be named in the release notes only when their
  plans' reports say they landed.
- **No release for a reverted experiment.** If plan 2 cannot beat the current
  `Board::legal` path in a way that justifies the public surface, the report
  records the measurement, the item stays open, and plan 5 does not mention it
  in the release notes. The same rule applies to any optional fast path in plan
  2 or 3.
- **Plan 5 finalises the release.** It picks the version from what actually
  landed, runs the gates, updates `Cargo.toml`, `CHANGELOG.md`, the README
  dependency/API snippets, and the public-API pin. It does not publish.
- **No promise.** The plans publish measured costs for the new shapes. They make
  no claim about a downstream search tree, a whole-binary `core` run, or wall
  clock on another machine.

## Global gates for every plan

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc` (no warnings; `cargo test --doc` for new doctests)
- `cargo run --release --example verify_perft` (41/41; depth 5 minimum, depth 6
  when time permits)
- `cargo test --test perft_tests`, `--test move_order_golden`,
  `--test verify_moves`, `--test conformance`, `--test effects`,
  `--test post_move`, `--test terminal_after`, `--test child_state`,
  `--test ep_key`, `--test null_for`, `--test public_api`,
  `--test interchange`
- `cargo test --features unsafe-optimizations` when touching the key/attack
  helpers (plans 3 and 4; plan 2 when it changes attack geometry).
- Perft timing (`--release`, three runs, median) for any change that could
  affect `Board::legal`, `populate_state`, the generator, or the legality
  ladder. A regression above ~2 % is a blocker, not a note.
- No new dependency, no new GPL interop, no search/evaluation/tablebase/
  threading API, no second public position representation, and no API that
  exists only to serve one downstream's search design.
- No change to `Board::terminal`/`outcome` precedence, `ZOBRIST_VERSION`, the
  `Always` key, the default FEN parser, or the generation order while
  implementing any of these plans.

## Differential corpus and measurement protocol

Every exactness gate uses the same inputs as the recent child/effect plans:

- the 41 positions in `tests/perft_values.md`;
- `conformance::CASES` and `conformance::CHILD_CASES`;
- the effect fixtures in `src/effects.rs` (`CORPUS`) and the board fixtures in
  `src/board.rs`;
- the seeded play-outs from `tests/common/mod.rs` (at least the six play-out
  starts, 16 seeds, 300 plies, plus the corpus walk);
- the `tests/null_for.rs` fixtures for plan 4.

Every cost gate uses a new `examples/hotpath_bench.rs` (introduced by plan 1 and
extended by plans 2–4), so the rows are built once and the candidate loops are
measured without construction noise. Each row reports the median of repeated
batches, absolute ns, and the ratio to the current 2.9.0 row in the same run.
When `perf stat -e instructions:u` is available and the event counts, the
report also prints instructions per call and says how it was obtained. The
wishlist's instruction ceilings are reported against that diagnostic; they are
not treated as portable upstream gates.

## Pushback recorded against the wishlist

- **W6 is correct and cheap.** `Board::legal` genuinely reads only the two
  counts. The plan still adds a small `Counts` type rather than only changing
  prose because W7 and W8 need a value that can be passed across a node. The
  documented precondition is part of the deliverable.
- **W7's ≤ 100-instruction target is not guaranteed.** The request's number is
  a downstream measurement on a downstream compiler. This plan can hoist
  node-constant data and expose occupancy, but it cannot promise a portable
  instruction count. If the measured result is not below ~120 instructions and
  does not beat the current path, the report keeps W7 open rather than claiming
  it.
- **W8's `counts` parameter is not needed to compute the child effect.** The
  plan retains the requested signature for candidate-loop symmetry and uses
  `counts` only for a sound terminal-parent short-circuit in the `Legal`
  en-passant branch. It is not a legality precondition: malformed input is
  `None`, and illegal but well-formed moves still describe the child they would
  build.
- **W9 does not add a public `set_rule50`.** The dedicated
  `null_for_advanced` method is the requested shape, keeps clock mutation in one
  place, and avoids a second way to construct a board whose clock and side do
  not describe a real move. If a later consumer needs a general clock setter,
  that is a separate additive request.
- **No `EffectContext`/`PostMove` extension.** The shipped view-based shape was
  measured slower by the requester; these plans add candidate/child values
  beside it, not more methods on it.

## Final task for each plan

Write `docs/plans/feedback/report_wishlist2_N.md` with item-by-item status, the
public (or crate-private) API as implemented, differential counts (positions,
moves, checked modes), the benchmark table with absolute ns and the ratios the
plan defines, changed files, problems encountered, deviations from the plan
with the measurement that motivated them, unresolved parts, missing tests, and
next steps. Plan 2's report must state the measurement-gate verdict and either
the shipped API or the open-item decision. Plan 3's report must state
field-for-field equality with `clone + do_move + hash_with(mode)` and with
`rule50_after`. Plan 4's report must state that `null_for` is unchanged and that
the advanced variant is allocation-free. Plan 5's report must state the final
release version, the landed plans, the package diff and the owner's manual
publish checklist.
