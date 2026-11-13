# Handoff 2 — implementation plan index and sequencing

## Purpose

This index turns `docs/plans/feedback/handoff2.md` into a small set of
independently reviewable implementation plans. It is a plan, not an
implementation. Each numbered plan follows `AGENTS.md` and ends with an
implementation report (`docs/plans/feedback/report_handoff2_N.md`).

Handoff 2 is a wishlist with an acceptance bar, not a specification. The plans
below accept MG15–MG18 because each one is a statement about *positions and
moves* that is testable inside this crate, and reject nothing that handoff 2
did not already reject (§4, non-asks). The crate stays zero-dependency,
safe-Rust-by-default and atomic-only.

## What handoff 2 actually asks for

The downstream measured 2.6.0's `EffectContext`: of the children it builds,
0.00 % are decided by the effect shortcuts, and the remaining 100 % of the
"is this child over?" question is answered by cloning the board and playing the
move. R6's "the child's terminal state, and why" is therefore answered only in
the cases where extinction, the commoner-only draw, or the non-check clock
short-circuit — everything else needs the child's *"does the side to move have
a legal move?"* answer, which 2.6.0 cannot give without a board write.

So the flagship is one question, and the rest is API hygiene around it:

| MG | Ask | Priority |
| --- | --- | --- |
| MG15 | `terminal_after`: child terminality without a board write | P1 |
| MG16 | a two-level child result that separates *illegal* from *undecided* | P2 |
| MG17 | the post-move piece summary the effect path already computes | P2 |
| MG18 | `effect` without the legality ladder | P3 |

## Current-tree assessment (2.6.0 working tree)

| Item | Current state | Plan |
| --- | --- | --- |
| MG15 | absent. `EffectContext::outcome_after` decides extinction, the commoner-only draw and the non-check clock from the effect, and **falls back to `board.clone(); do_move; outcome()`** for mate, stalemate and every live position. There is no way to ask "does the child have a legal move" without a board write: `movegen::has_legal_move_with_state` takes `&Board`. | 1 |
| MG16 | `EffectContext::outcome_after` returns `Option<Outcome>` and documents only the illegal `None`; the undecided `None` is undocumented. | 2 |
| MG17 | `effects::PostMove` exists but is a **private** struct; only its derived commoner bitboards leak out through `MoveEffect`. `MoveEffect` carries no placement. | 2 |
| MG18 | `EffectContext::effect` always runs `is_legal_from` (one `is_attacked_by` for the mover's commoner, plus the castling oracle). No legality-free variant exists. | 3 |
| handoff1 residue | All MG1–MG14 landed by 2.6.0 except MG6, which the 2.5/2.6 report closed as "partly done: profile-led, no dominant cost found". No new plan is created for it: handoff 1 has no new measurable target in it, and any further generator work must start from a profile, not from a wish. | — |

The one piece of *new* machinery all four items need is the same: the rules
must be able to answer questions about a position that is described by
bitboards + side + en passant + castling rights + clock, **not only by a
`Board`**. That is plan 1's real work; MG16–MG18 then read from it.

## Plan set

1. **`plan_handoff2_1.md` — child-view terminality (MG15).**
   The flagship. Introduce a crate-private `PositionView` (the part of a
   position the rules read), move legality, `populate_state`, the pseudo-legal
   generator and the fast-legality ladder onto that view, share one terminal
   precedence function between `Board::terminal` and the new
   `EffectContext::terminal_after`, and delete `outcome_after`'s clone
   fallback. High risk (hot-path refactor); differential and perft gates are
   mandatory.
2. **`plan_handoff2_2.md` — child information (MG16, MG17).**
   `AfterState { Illegal, Undecided, Terminal }` +
   `EffectContext::child_outcome` / `Board::child_outcome`, the corrected
   `outcome_after` documentation (and its reimplementation on `child_outcome`),
   and the public `PostMove` summary that MG17 asks for. Additive, cheap,
   depends on plan 1.
3. **`plan_handoff2_3.md` — legality-ladder bypass, measurement, release
   (MG18).** `MoveClassification` (the `MoveEffect` fields without `is_legal`)
   + `EffectContext::classification`, the decode of 2.6.0's honest "the effect
   path is not faster" note into a decision about the effect path, README /
   `lib.rs` / `CHANGELOG` / conformance updates, benchmarks, and the 2.7.0
   release checklist including the downstream handoff protocol (§3 of handoff
   2: published version only, no path dependency).

## Dependency order

```text
plan 1 (MG15: PositionView + terminal_after)
   |
   +--> plan 2 (MG16 AfterState, MG17 PostMove)
   |
   +--> plan 3 (MG18 classification, docs, release 2.7.0)
```

- Plan 1 lands first and alone: it changes the hottest code in the crate
  (`Board::legal`, `populate_state`, the pseudo-legal generator, perft). Nothing
  else may be in flight while its differential and perft gates are red.
- Plan 2 and plan 3 are independent of each other and may land in either
  order; both are pure API and docs and must not touch the view.
- Plan 3's release step is last so the CHANGELOG can name the final API.

## Release strategy

All four items are additive, so a single **2.7.0** release after plans 1–3 is
enough. If plan 1 slips, **2.6.1** may ship MG16's documentation fix alone
(the `outcome_after` doc bug is a correctness trap, and fixing the doc without
changing behaviour is a patch). No plan may change move order, perft values,
hash values, FEN output, or `Outcome`/`Terminal` semantics.

The downstream protocol (handoff 2 §3) needs a *published* version: plan 3 ends
with `cargo publish --dry-run`, a `CHANGELOG` entry and a short note in the
report naming what the downstream must re-run (`measurements/plan29/probe.py`
gate: decided share > 10 % of built children).

## Global gates for every plan

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (41/41; depths 1–5 minimum, 6
  when time permits)
- `cargo test --test perft_tests`, `cargo test --test move_order_golden`,
  `cargo test --test verify_moves`, `cargo test --test conformance`,
  `cargo test --test effects`
- `cargo test --features unsafe-optimizations` when touching magic/attack code
- **Perft timing** (`--release`, three runs, median) for every change that
  touches `Board::legal`, `populate_state`, the generator or the ladder: a
  regression above ~2 % is a blocker, not a note.
- No new dependency, no `unsafe`, no GPL interop, no variant framework, no
  search/eval/threads/network, no API that only one downstream's search design
  needs.

## Non-goals (handoff 2 §4 and handoff 1 R9)

Do not add: a search/eval/tablebase/threading API, a general insufficient
material rule (the downstream's Lichess-style rule stays the downstream's, MG17
exposes data only), a second public position representation, a generic
"variant rules" framework, GPL interop, or a "prover" concept in any name or
field.

## Final task for each plan

Write `docs/plans/feedback/report_handoff2_N.md` with item-by-item status,
the public API as implemented, differential counts (visited positions and
moves), benchmark numbers, changed files, problems encountered, deviations from
the plan, unresolved parts, missing tests, and next steps.
