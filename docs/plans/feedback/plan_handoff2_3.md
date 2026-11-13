# Plan 3 — legality-ladder bypass, measurements, documentation and the 2.7.0 release (MG18)

## Scope

- **MG18 (P3):** give a caller that already knows a move is legal (it came from
  `generate_legal_with_state`) the move's rules classification **without**
  re-running the legality ladder.
- Then close the release: measurements that back the claims of plans 1–3, the
  documentation pass (README, `lib.rs`, `CHANGELOG`), the version bump to
  **2.7.0**, and the checklist the downstream protocol of handoff 2 §3 needs.

Requirements served: R3 (MG18). Priority P3, and deliberately the last item:
it is an optimisation of an API that 2.6.0 already has.

**Depends on plans 1 and 2.**

## Current-tree assessment

- `EffectContext::effect` = `analyze` + `is_legal_from` + `checks_enemy`. A
  caller that classified moves taken from `generate_legal_with_state` has
  already paid for `is_legal_from` (one `is_attacked_by`, the commoner-count
  comparison, and the touching-commoner branch) and discards the result.
- The 2.5.0 report measured `effect()` at 27–31 ns and `effect().is_legal` at
  25–28 ns against `Board::legal` at 7–8 ns; the effect path is **not** faster
  than the board-writing path today, and the report says so. MG18 is therefore
  not a speed claim for the crate: it removes one duplicated rule query from a
  path a consumer chose to use.
- `MoveEffect`'s six public fields are read by consumers (`effect.is_legal`),
  so the field set cannot be narrowed without a major release.

## Decisions and pushback

1. **Do not ship `effect_trusted(..) -> MoveEffect`.** The handoff offers two
   shapes; the one that keeps `MoveEffect` and documents `is_legal` as "not
   evaluated" would set a public boolean to a value the crate did not compute.
   A one-word misread (`e.is_legal`) would then be a silent correctness bug in
   a *rules* crate whose selling point is "never lies". The implemented shape
   is the other option the handoff lists: a value **without** the field.
2. **`MoveClassification` is a new type; `MoveEffect` keeps its shape.** The
   five rules fields move into `MoveClassification`; `MoveEffect` keeps all six
   public fields (no break) and gains `classification()`, which returns the
   five-field value. `effect` = `classification` + one lowercased legality
   verdict — one implementation, two entry points.
3. **No `is_legal`-shaped shortcut by another name.** Names like
   `effect_assume_legal`, `effect_unchecked`, `unsafe_effect` are rejected:
   "checked" in Rust implies memory safety, and "assume" implies trust in an
   input precondition the crate cannot verify. `classification` states what it
   returns.
4. **Measured, then accepted as an API win.** The acceptance criterion is "the
   five fields are byte-identical to `effect` for every pseudo-legal move, and
   the legality differential is unaffected". The speed number is reported, not
   promised: if `classification` and `effect` are within noise on the bench
   positions, the item still ships (it removes a duplicated query for
   legality-aware callers) and the report says the delta was within noise.
5. **Release is a plan step, not a promise.** Nothing is published until the
   full gate list is green and the plans 1–2 reports exist.

## Proposed API

```rust
// src/effects.rs
/// The rules classification of one pseudo-legal move, without a legality
/// verdict.
///
/// Equal field for field to the corresponding [`MoveEffect`] fields for every
/// pseudo-legal move. Returned by [`EffectContext::classification`] for callers
/// that already know the move is legal (e.g. it came from
/// [`generate_legal_with_state`](crate::movegen::generate_legal_with_state)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveClassification {
    pub is_capture: bool,
    pub casualties: BlastCasualties,
    pub checks_enemy: bool,
    pub explodes_enemy_commoner: bool,
    pub leaves_own_commoner: bool,
}

impl MoveEffect {
    /// The classification part of this effect (everything except `is_legal`).
    pub fn classification(&self) -> MoveClassification;
}

impl EffectContext {
    /// Classify `m` without running the legality ladder.
    ///
    /// Same fields as [`effect`](Self::effect) minus `is_legal`; no
    /// `is_attacked_by` for the mover's commoner is run. Use
    /// [`effect`](Self::effect) or [`is_legal`](Self::is_legal) when the
    /// verdict itself is needed. Malformed input yields all-`false`/empty
    /// fields, exactly like `effect`'s malformed branch.
    pub fn classification(&self, board: &Board, m: Move) -> MoveClassification;
}
```

## Implementation

1. Add `MoveClassification`; make `effect` build
   `MoveEffect { ..classification, is_legal }` from it (one code path, no
   duplicated derivation):
   ```rust
   pub fn effect(&self, board: &Board, m: Move) -> MoveEffect {
       let (classification, legality) = self.classify_and_decide(board, m);
       MoveEffect { is_capture: classification.is_capture, .., is_legal: legality }
   }
   ```
   `classify_and_decide` runs `analyze` once and skips the oracle for
   `classification`.
2. `#![warn(missing_docs)]` stays satisfied: every new field, method and type is
   documented; the crate root re-exports `MoveClassification`.
3. Benchmarks (`examples/effect_bench.rs`): add a row per position for
   `classification` next to `effect`, `effect().is_legal` and `Board::legal`,
   and one combined row for the "legal-list consumer" loop:
   `generate_legal_with_state` + `effect` versus
   `generate_legal_with_state` + `classification`.
4. Documentation pass across all three plans:
   - `README.md`: features (child terminality without a board write, two-level
     child state, post-move summary, classification), a short example using
     `terminal_after` + `child_outcome`, and a "what the effect path does and
     does not decide" paragraph that is honest about the measured numbers.
   - `src/lib.rs`: crate docs' effect section rewritten around the child view;
     contracts section unchanged.
   - `CHANGELOG.md`: `[2.7.0]` with Added/Changed/Fixed, explicitly naming the
     `outcome_after` documentation fix as **Fixed (docs only, no behaviour
     change)**.
   - `docs/perf/perf_guide.md`: point the "where to look" section at the new
     view code and the child path.
5. Release checklist:
   - bump `Cargo.toml` to `2.7.0` (and `Cargo.lock`);
   - `cargo test`, `cargo test --features unsafe-optimizations`,
     `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
     `cargo doc`, `cargo package --list`;
   - `cargo run --release --example verify_perft` (41/41, depth 5 minimum);
   - `cargo run --release --example verify_san`, `effect_bench`,
     `terminal_after_bench` (record medians in the report);
   - `cargo publish --dry-run`;
   - a short "what the downstream should re-run" note in the report
     (`measurements/plan29/probe.py`; decided share > 10 % of built children on
     their hand stack; the 2.7.0 API they now have).
6. Record open items for a possible `handoff3`: the lazy-state child path, the
   effect path's adoption (or not) into `generate_legal_with_state`, and any
   downstream feedback on `AfterState`/`PostMove` naming.

## Tests

- `classification_matches_effect`: for every pseudo-legal move over the perft
  corpus, plan 1's fixtures and the seeded play-outs,
  `ctx.classification(&b, m) == ctx.effect(&b, m).classification()` and every
  field equals the directly comparable board/child value
  (`is_capture`/`casualties` vs `Board`, `checks_enemy` /
  `explodes_enemy_commoner` / `leaves_own_commoner` vs the `do_move` child).
- `classification_malformed_input`: `Move::NONE` and an empty-origin move yield
  the all-`false`/empty value, identical to `effect`'s malformed branch.
- The existing effect differential (`tests/effects.rs`, arm/handoff1) must pass
  unchanged — `effect`'s implementation moved, its result may not.
- A `#[test]` that the 2.7.0 public API surface named in the plans exists and
  re-exports resolve from the crate root (`AfterState`, `PostMove`,
  `MoveClassification`, `MoveEffect`, `EffectContext`, `Terminal`), so the
  release notes cannot drift from the code.
- Doc tests: the README and `lib.rs` examples compile (`cargo test --doc`).

## Verification

All global gates of the index, plus the release checklist above and
`cargo package --list` (no `docs/`, no `Fairy-Stockfish/`, no scratch example).

## Files to touch

- `src/effects.rs` — `MoveClassification`, `classify_and_decide`,
  `classification`, `MoveEffect::classification`, unit tests, docs.
- `src/lib.rs` — re-export + crate docs.
- `examples/effect_bench.rs` — new rows/loops.
- `tests/effects.rs` (or `tests/child_state.rs`) — classification differential.
- `README.md`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`,
  `docs/perf/perf_guide.md`.
- `docs/plans/feedback/report_handoff2_3.md` — final report (includes the
  release record).

## Non-goals

- No `unsafe`, no "assume legal" / "unchecked" API name or shape, no
  `debug_assert!`-only legality.
- No change to `MoveEffect`'s field set or to any legality result.
- No adoption of the classification/effect path into `generate_legal_with_state`
  or perft: the 2.5.0 measurement says the ladder plus `Board::legal` is faster
  on the generator's own hot path. A future change there starts from a profile.
- No new dependency, no benchmarking framework.
- No publish without the plans 1 and 2 reports being written.

## Final task

Write `docs/plans/feedback/report_handoff2_3.md`: the implemented API, the
classification differential, the benchmark table (`classification` vs `effect`
vs `Board::legal`, and the legal-list loop), the documentation and release
record (version, `cargo` commands run, `verify_perft` output), open items for
`handoff3`, problems, unresolved parts, missing tests and next steps.
