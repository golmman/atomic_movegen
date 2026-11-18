# Implementation report — wishlist 1 plan 4 (W4: node-constant analysis view)

## Gate verdict first

**No implementation was performed. W4 remains a watchlist entry.**

The plan's first task is a measurement gate owned by the downstream. The gate
requires, before any code is written:

1. `Mover::new`'s absolute cost and share of per-expansion wall on the same
   bench corpus used for handoff 3;
2. the field-by-field mapping from the prover's `Mover` fields to the proposed
   `AnalysisContext` accessors, including whether each field is placement- or
   `StateInfo`-derived;
3. representative positions and a play-out seed set for a reproducible
   upstream differential.

None of that evidence is present in this repository. A repository-wide search
for `Mover::new`/`Mover` (excluding `target/` and `Fairy-Stockfish/`) finds only
the wishlist text, the handoff commentary, and this plan; no downstream
measurements, field mapping, corpus, or seed set was supplied. The upstream
cannot measure the downstream's private `Mover::new`, and the plan explicitly
forbids building the API first and looking for a consumer later.

| Gate input | Required | Available |
| --- | --- | --- |
| `Mover::new` absolute ns | yes | **not supplied** |
| `Mover::new` share of per-expansion wall | yes | **not supplied** |
| Field-by-field `Mover` → accessor mapping | yes | **not supplied** |
| Representative positions and play-out seeds | yes | **not supplied** |

The default materiality threshold was recorded before any implementation work,
as required by the plan: **implement only if `Mover::new` is at least 5 % of
per-expansion wall and the projected removal is at least 1 % of total wall after
the adapter drops `Mover`.** No number was available to apply it to. Because the
gate is not merely below threshold but unevaluable, the plan closes without
code rather than guessing a share or substituting an upstream proxy.

Gate verdict: **blocked / not passed → watchlist, no implementation.**

## Item status

| Plan item | Status | Where |
| --- | --- | --- |
| W4 measurement gate (`Mover::new` share) | **blocked: no downstream data** | this report |
| Field-by-field `Mover` → accessor mapping | **not supplied** | this report |
| Materiality threshold recorded | **done** (default 5 % / 1 %) | this report |
| `AnalysisContext`, `src/analysis.rs` | **not started** | — |
| `examples/analysis_bench.rs` | **not started** | — |
| Corpus/play-out differential and public-API pin | **not started** | — |
| Watchlist decision | **done** | this report |

## What was checked

- `docs/plans/feedback/wishlist1.md` W4 confirms the intended outcome: if
  `Mover::new`'s share is small, the item "stays a watchlist entry and is not
  implemented".
- `docs/plans/feedback/handoff3.md` discusses downstream costs for other items
  (MG15–MG20) but contains no `Mover::new` profile share and no `Mover` field
  mapping.
- `docs/plans/feedback/plan_wishlist1_3.md` and its report note that no
  downstream `Mover::exposes` differential was available; the same absence
  applies here for the broader `Mover` view.
- The tree currently has no `src/analysis.rs`, no `AnalysisContext` export, and
  no `analysis_bench` example. Nothing was added.

## Watchlist decision

W4 is recorded as a watchlist item. The proposed API in the plan remains a
proposal, not a contract. In particular, these questions are still open and
must be answered by the downstream before the plan can be reopened:

| Proposed accessor | Downstream `Mover` field(s) | Mapping status |
| --- | --- | --- |
| `checkers()` | unknown | unresolved |
| `pinned(color)` | unknown | unresolved |
| `check_squares(attacker, role)` | unknown | unresolved |
| `blockers_between(target, slider_color)` | unknown | unresolved |
| `pieces_beside_commoner(commoner_color)` | unknown | unresolved |
| `attack_map(color)` | unknown | unresolved |
| `uncovering_pieces(target, piece_color)` | unknown | unresolved |

No accessor was added, renamed, or narrowed on the basis of a guessed mapping.
The plan explicitly says unresolved fields must be listed, not invented.

## Code changes

None beyond this report. No source file, test, example, public-API pin, README,
CHANGELOG, `Cargo.toml`, key table, move order, or hot path was changed.

This is the plan's stated non-goal-preserving outcome: the analysis view must
be possible to delete without changing an existing answer, and no consumer
exists in-tree to exercise it.

## Verification

No code changed, so no new compilation or differential gate applies. The
repository was only inspected read-only:

```sh
git status --short                      # clean before this report
grep -R "Mover::new\|Mover" ...          # no downstream data in-tree
ls src/analysis.rs examples/analysis_bench.rs  # absent
```

## Problems encountered

1. **The gate owner and the repository owner are the same project, but the
   measurement lives in the downstream.** `wishlist1.md` identifies the prover
   as the gate owner; this tree has no access to its private `Mover` type,
   benchmark harness, or play-out seeds. The plan correctly forbids an upstream
   substitute.
2. **No field mapping means the API shape is not yet decidable.** Even the
   provisional parameter order (`check_squares(attacker, role)` versus a
   combined helper) depends on the downstream's field layout. Implementing
   now would risk baking in the wrong shape and then exposing both spellings
   permanently, which the plan forbids.
3. **A proxy benchmark would be misleading.** Re-deriving similar masks from
   `Board`/`PositionView` inside this crate would measure an invented
   `Mover`, not the prover's, and would not satisfy the 5 % / 1 % gate. It was
   not performed.

## Deviations from the plan

None. The plan's gate closed the item before implementation, exactly as
written. No threshold other than the recorded default was chosen because no
measurement existed to compare against it.

## Unresolved parts and missing tests

- **`Mover::new` share:** unmeasured; this is the blocking item.
- **`Mover` field mapping:** unavailable; every proposed accessor is
  unresolved.
- **Representative positions and seeds:** unavailable; no upstream differential
  can be reproduced.
- **API exactness/cost:** not tested, because no API was shipped.
- **`AnalysisContext::new` vs `populate_state` cost:** not measured; the plan
  makes even that measurement conditional on the gate passing.

## Next steps

1. The downstream records, for the same handoff-3 bench corpus:
   - `Mover::new` absolute ns and its share of per-expansion wall;
   - the projected wall after the adapter drops `Mover`, so the removal share
     can be checked against the 1 % bar;
   - a table mapping every `Mover` field to a proposed accessor, marking
     placement/`StateInfo` provenance;
   - representative FENs and play-out seeds.
2. If the 5 % / 1 % threshold is met, reopen the plan and implement the API
   shape that the mapping supports, then run the corpus/play-out differential,
   the no-board-write checks, and `examples/analysis_bench.rs` as specified.
3. If the share is below threshold, keep this watchlist verdict and do not add
   the type. The downstream can continue to derive these masks in its own
   `Mover`; the crate already exposes the individual facts (`checkers`,
   `pinned`, `attackers_to`, attack tables, `between`).
