# Handoff 1 — implementation plan index and sequencing

## Purpose

This index turns `docs/plans/feedback/handoff1.md` into a small set of
independently reviewable implementation plans. It is a plan, not an
implementation. Each numbered plan follows the repository conventions in
`AGENTS.md` and ends with an implementation report.

The handoff is a wishlist, not a specification. The plans below accept the
requirements that serve R1–R8, reject the explicit non-requirements in R9, and
keep the crate zero-dependency, safe-Rust-by-default, and atomic-only.

## Current-tree assessment (2.4.0 working tree)

| Item | Current state | Plan |
| --- | --- | --- |
| MG1 `Board::null_for` | absent; consumers use a FEN round-trip | 1 |
| MG2 public `for_each_pseudo_legal` | internal `pub(crate)` stream exists; order golden exists | 1 |
| R2 helpers (`StateInfo` root re-export, typed castling side, public between/x-ray) | absent or private | 1 |
| MG3 effect context + per-move effect query | absent; `Board::legal` is read-only but recomputes per move | 2 |
| MG5 node-constant attack data | merge into MG3 | 2 |
| MG4 widen trivial legality to captures/checks | current `TrivialLegality` accepts only quiet, non-capture, non-check, non-pinned moves | 3 |
| MG6 `generate_pseudo_legal` micro-optimisation | already table-driven; only profile-led changes | 3 |
| MG7 FEN performance fallback | not needed if MG1 lands | 3 (non-goal) |
| MG8 SAN parse/format | absent | 4 |
| MG9 piece-set construction/inspection | absent | 4 |
| MG10 terminal reason | absent; `Outcome` is WDL only | 5 |
| MG11 key contract | FSF-compatible keys exist; stability is unstated | 5 |
| MG12 generation order contract | golden test exists; order is not stated as public contract | 5 |
| MG13 conformance corpus | absent | 6 |
| MG14 engineering contract | `Send + Sync`, allocation, and semver policy unstated | 5 |

## Plan set

1. **`plan_handoff1_1.md` — hot-path API and position helpers (MG1, MG2, R2).**
   Add `Board::null_for`, make the pseudo-legal stream public, re-export
   `StateInfo`, add typed castling-side queries, and expose between/x-ray
   helpers. Low risk, additive, independently releasable.
2. **`plan_handoff1_2.md` — effect context and per-move effects (MG3, MG5).**
   The flagship. Add a node-constant `EffectContext`, a `MoveEffect` value, a
   fast legality path, a legal early-exit stream, and a child-outcome helper
   with a documented fallback. High risk; differential gates are mandatory.
3. **`plan_handoff1_3.md` — performance follow-ups (MG4, MG6, MG7).**
   Use the effect context to widen the trivial-legality ladder, profile the
   pseudo-legal generator, and explicitly skip the FEN fallback because MG1
   removes the measured round-trip.
4. **`plan_handoff1_4.md` — interchange: SAN and piece-set setup (MG8, MG9).**
   Add board-aware SAN parse/format and a `Setup` builder plus piece iteration.
   Depends on plan 2 for check/mate suffixes.
5. **`plan_handoff1_5.md` — terminal reasons and public contracts (MG10, MG11,
   MG12, MG14).** Add `TerminalReason`/`Terminal`, pin the Zobrist key contract,
   publish the generation-order contract, and state the engineering/semver
   contract.
6. **`plan_handoff1_6.md` — conformance corpus as data (MG13).**
   Export a frozen, public corpus of FEN → legal moves in order, outcome,
   terminal reason, keys, and clocks, with a self-checking test.

## Dependency order

```text
plan 1 (MG1, MG2, R2)
   |
   v
plan 2 (MG3, MG5) -----> plan 4 (MG8, MG9)
   |
   v
plan 3 (MG4, MG6)      plan 5 (MG10, MG11, MG12, MG14)
                              |
                              v
                         plan 6 (MG13)
```

- Plan 1 is independent and should land first because plan 2 uses the public
  between/x-ray helpers and the public stream.
- Plan 2 is the correctness-critical centre. Do not start plan 3 until plan 2's
  differential gates are green.
- Plan 4 can start after plan 2; SAN suffixes use the effect/outcome path.
- Plan 5 is mostly additive API and documentation. It can run in parallel with
  plan 3, but plan 6 should follow plan 5 so the corpus can include terminal
  reasons.
- Plan 6 is data-only and should be last so it freezes the final observable
  behaviour.

## Release strategy

All plans are additive and target semver-minor releases. A reasonable sequence
is:

- **2.5.0** after plans 1 and 2 (hot-path API and effect context).
- **2.6.0** after plans 3–6 (performance follow-ups, interchange, contracts,
  conformance corpus).

If a plan is deferred, the earlier release can still ship. No plan may change
existing move order, perft values, hash values, FEN output, or `Outcome`
semantics without an explicit rules-fix note and a major-version decision.

## Global gates for every plan

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (41/41, depths 1–6 when time
  permits; at least depth 5 for every change)
- `cargo test --test move_order_golden`
- `cargo test --test verify_moves`
- `cargo test --features unsafe-optimizations` when touching magic/attack code
- No new dependencies, no GPL interop, no tablebase probing, no variant
  framework, no search/eval/threads/network.

## Non-goals (from handoff R9)

Do not add:

- GPL interop, a GPL dependency, a GPL dev-dependency, or a
  “shakmaty compatibility” feature;
- tablebase probing;
- a variant framework or non-atomic rules;
- evaluation, NNUE, search, threads, a UCI loop, `async`, or networking;
- an API that only one downstream lifecycle needs.

## Final task for each plan

Write the corresponding `report_handoff1_N.md` with item-by-item status,
changed files, test/verification results, measured numbers where applicable,
problems encountered, unresolved parts, missing tests, and next steps.
