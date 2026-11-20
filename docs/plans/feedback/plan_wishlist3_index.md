# wishlist 3 — implementation plan index and sequencing

## Purpose

This index turns `docs/plans/feedback/wishlist3.md` into independently
reviewable implementation plans. It is a plan, not an implementation. Each
numbered plan follows `AGENTS.md` and ends with an implementation report
(`docs/plans/feedback/report_wishlist3_N.md`).

Wishlist 3 contains one item, `W10`, written against released
`atomic-movegen` 2.10.0 (`5083647`, default features). The baseline in this
tree is that release. `W10` asks for an evasion-aware `LegalityContext`: an
in-check node should reject quiet moves that cannot possibly answer the check
before the full legality body runs. The only other wishlist items remain as
stated there: `wishlist1.md` stays open (`W4` watchlist, `W5` only on a
key-driven regression), and `wishlist2.md` is closed into 2.10.0.

The wishlist cites `report9.md` and `measurements/plan9/`; those artifacts are
not present in this tree. Their counts and instruction totals are therefore
treated as supplied downstream evidence, not as facts this repository can
reproduce. This repository can prove exactness on its own corpus and measure
same-host candidate costs; it cannot measure a downstream `core`/`easy-win`/
`deep-win` run, a `.pvr`, or a prover acceptance rate.

| # | Ask | Type | Priority | Plan |
| --- | --- | --- | --- | --- |
| `W10` | evasion-aware `LegalityContext` | additive API + candidate-loop cost | P1 | 1 |
| — | integration, combined measurement and release gate | release gate | final | 2 |

## Current-tree assessment (2.10.0)

| Item | Current state | Plan |
| --- | --- | --- |
| `LegalityContext` | `Board::legality_context(counts)` builds `LegalNode::from_view` once and stores `playable`. `LegalNode` snapshots per-type sets, occupancy, the two commoner sets, and the side to move. `LegalityContext::legal(m)` applies the game-over guard and calls the shared `legal_in` body. It has no checker information and must run the full body for every in-check candidate. | 1 |
| `LegalNode` | Crate-private, `Copy`, allocation-free. It already stores `our_commoners` and `occupied`, which the evasion rule needs. The board path (`Board::legal` → `legal_in_view` → `legal_in`) uses a borrowed `PositionView`; the context uses the snapshot. | 1 |
| `StateInfo::checkers` | Populated by `Board::populate_state` / `PositionView::populate_state`; it is the enemy pawn, knight, bishop, rook and queen set attacking the side to move's lowest commoner. `PositionView::checkers` returns empty when either side has no commoner or when the two commoners touch (touching grants immunity). | 1 |
| Check geometry | `attacks::between` / `bitboard::between_bb` already expose the strictly-between squares, and `attacks::king_attacks` exposes the touching test. The generator's crate-private `TrivialLegality` rejects all in-check moves to the oracle, while `WidenedLegality` requires empty `checkers`; neither is a candidate-loop context. | 1 |
| Bench | `examples/hotpath_bench.rs` compares `Board::legal`, the current context, and child/null rows, but only the current check-blind context is measured. There is no in-check class split and no setup row for a state-aware context. | 1, 2 |
| Differential | `tests/legality_context.rs` compares the current context with `Board::legal` over the perft corpus, conformance/child cases, effect and board fixtures, edge fixtures, and the seeded play-outs. It already counts in-check positions, but the new context and its mask-rejection branch are not exercised. | 1 |

The wishlist's premise is correct: `Board::legal` has the full `StateInfo` at
the candidate loop in the prover, while `LegalityContext` is deliberately
check-blind. Adding a second constructor that *takes* `state.checkers` gives
the context the one node fact it lacks without putting an attacker query on
either the existing board path or the existing check-blind context.

## What each plan does

1. **`plan_wishlist3_1.md` — W10: evasion-aware `LegalityContext`.**
   Add `Board::legality_context_with_state(counts, &StateInfo)`. It stores a
   small node-local evasion mask derived from the supplied `checkers`: either
   “not in check”, “in check and no quiet move can answer”, or “in check by one
   slider, and only the squares strictly between the commoner and that slider
   can block”. The existing constructor keeps its exact current behavior. The
   new context rejects only the quiet non-commoner moves that cannot answer a
   check, and only before the shared `legal_in` body; captures, en passant,
   commoner moves, everything outside check, and every mask-accepted quiet move
   still go through the existing rules body unchanged. No board, generator,
   `StateInfo`, key, FEN, terminality or perft path changes. The plan includes a
   proof of the rejection rule, the full differential, focused fixtures for
   every branch the wishlist names, and a same-host measurement gate.

2. **`plan_wishlist3_2.md` — integration, combined measurement and release
   gate.**
   This plan adds no feature. It starts only after plan 1 has a report. It
   re-runs the combined differential and benchmark, pins the new API in
   `tests/public_api.rs`, updates the README/CHANGELOG, chooses whether W10
   earned a release, and prepares 2.11.0 if it did. It does not publish, tag,
   commit, or update a downstream pin/`PROVENANCE.md`/`MD6` row.

## Dependency order

```text
plan 1 (W10: state-aware evasion context)
                 |
                 +--> plan 2 (combined differential, measurement, release gate)
```

Plan 2 must not start until `report_wishlist3_1.md` exists and its verdict is
known. If plan 1's measurement gate fails, plan 2 still writes a short report,
does not bump the version, and leaves W10 open.

## Release strategy

- **2.11.0** for the additive `Board::legality_context_with_state` surface, but
  only when plan 1 reports exactness and a repeatable same-host win on the
  in-check candidate row without a material regression elsewhere. No existing
  answer, key, move order, perft value, FEN, terminality result or
  `ZOBRIST_VERSION` changes.
- **No release for a reverted experiment.** If the new context is exact but
  does not beat the current context on the in-check row, or regresses the
  existing path, plan 1's report says so and the public constructor is not
  shipped. W10 stays open; plan 2 does not mention it in the release notes.
- **No downstream claim.** The plan report may compare its same-host
  instructions/ns with the wishlist's prototype, but it must not state a
  downstream tree, `.pvr`, proof acceptance, or whole-binary instruction
  saving. The downstream owner's plan/report, pin, `PROVENANCE.md`, differential
  and same-session `MD6` row are outside this repository.

## Global gates for both plans

- `cargo test`
- `cargo test --all-features`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings`
- `cargo fmt --check`
- `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`
- `cargo test --doc`
- `cargo run --release --example verify_perft` (41/41; depth 5 minimum, depth
  6 when time permits)
- `cargo test --test perft_tests`, `--test move_order_golden`,
  `--test verify_moves`, `--test conformance`, `--test effects`,
  `--test post_move`, `--test terminal_after`, `--test child_state`,
  `--test child_effect`, `--test ep_key`, `--test null_for`,
  `--test legality_context`, `--test counts`, `--test public_api`,
  `--test interchange`, `--test robustness`
- Perft timing (`--release`, three runs, median) for any change that could
  affect `Board::legal`, `populate_state`, the generator or the legality
  ladder. A regression above ~2 % is a blocker, not a note.
- No new dependency, no new required `unsafe`, no allocation on the claimed
  hot path, no change to the `Always`/`Legal` key tables, move order, FEN
  parsing defaults, `ZOBRIST_VERSION`, or terminal/outcome precedence.

## Differential corpus and measurement protocol

Every exactness gate uses the same inputs as the wishlist-2 legality plan:

- the 41 positions in `tests/perft_values.md`;
- `conformance::CASES` and `conformance::CHILD_CASES`;
- the effect fixtures in `src/effects.rs` (`CORPUS`) and the board fixtures in
  `src/board.rs`;
- the edge fixtures already listed in `tests/legality_context.rs`;
- the seeded play-outs from `tests/common/mod.rs` (the six play-out starts,
  16 seeds, 300 plies, plus the corpus walk).

Every cost gate extends `examples/hotpath_bench.rs`; rows are built once per
dataset and the timed loops do not construct contexts or classify moves. Each
row reports the median of repeated batches, absolute ns, and the ratio to
`Board::legal` and to the current `LegalityContext` in the same run. When
`perf stat -e instructions:u` is available and stable, the report also prints
instructions per call and the exact command. The wishlist's instruction totals
remain diagnostics, not portable release gates.

## Pushback recorded against the wishlist

- **The cited plan-9 artifacts are absent.** The plan can implement the rule and
  measure this crate's candidate loop; it cannot confirm the `82.0%`, `0.72e9`,
  `.pvr`, or 9/9 proof claims from this tree. The report must say which numbers
  were reproduced and which are downstream-only.
- **No “commoner square” field is needed on `LegalNode`.** It already stores
  `our_commoners`; the mask build uses `lsb()` once and the rejection test uses
  the existing bitboard to exclude every commoner origin. The plan adds only
  the block mask and an in-check discriminator, not a redundant square field.
- **The new constructor takes both `Counts` and `&StateInfo`.** This preserves
  the requested candidate-loop shape and keeps the game-over guard separate
  from the checker data. The documentation requires both to be current. Only
  `state.checkers` is read; `pinned`, undo fields and the rest of `StateInfo`
  are ignored.
- **Existing `Board::legality_context(counts)` stays check-blind.** It remains
  exact and retains its measured shape. A caller that has a populated
  `StateInfo` opts in to the new constructor; the crate does not silently
  attach checker data to a context whose caller supplied only counts.
- **The prototype's whole-binary saving is not promised.** The shipping gate is
  a same-host candidate-row improvement with exactness and no material
  regression. If that gate fails, W10 stays open even though the rule is sound.
- **No rules change outside the prefilter.** Captures, en passant, commoner
  moves, castling outside check, and every quiet move outside check still use
  the existing shared body. The mask is a rejection-only predicate, never a
  second legality answer.

## Final task for each plan

Write `docs/plans/feedback/report_wishlist3_N.md` with item-by-item status,
the public (or crate-private) API as implemented, differential counts
(positions, candidates, in-check branches, mask rejections and legal verdicts),
the benchmark table with absolute ns, ratios and optional instruction counts,
changed files, problems encountered, deviations from the plan with the
measurement that motivated them, unresolved parts, missing tests, and next
steps. Plan 1's report must state the measurement-gate verdict explicitly.
Plan 2's report must state the final release version (or “no release”), the
landed plans, the package diff, and the owner's manual publish checklist.
