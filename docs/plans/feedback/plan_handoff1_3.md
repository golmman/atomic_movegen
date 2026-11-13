# Plan 3 — Performance follow-ups (MG4, MG6, MG7)

## Scope

This plan is deliberately measurement-led. It covers:

- **MG4**: widen the trivial-legality ladder to captures and checks using the
  `EffectContext` from plan 2.
- **MG6**: profile and micro-optimise `generate_pseudo_legal` without changing
  generation order.
- **MG7**: the `from_fen`/`fen_with` performance fallback. It is a **non-goal**
  because MG1 removes the measured null-replay FEN round-trip. Keep it only as a
  documented fallback if MG1 is deferred.

Do not start this plan until plan 2's effect differential, perft, and order
gates are green.

## Current-tree assessment

- `TrivialLegality` accepts only moves that are legal without `Board::legal`:
  no checkers, not en passant, not castling, no capture, origin not a commoner,
  origin not pinned.
- The handoff measured `Board::legal` at ~9 % of the downstream wall, with most
  of the remaining cost in per-child board/legality/terminality work.
- `generate_pseudo_legal` is already table-driven for pawns and sliders and the
  move-order golden test pins its order.
- The handoff's own measurement shows `generate_pseudo_legal` folded into the
  caller after MG2; it remains the largest single movegen symbol in the
  control profile.
- `Board::from_fen` allocates `Vec<&str>` for fields and ranks. That is a
  fallback path only; the null-replay use case is removed by MG1.

## Decisions and pushback

1. **MG4 is a ladder widening, not a second rules implementation.**
   `EffectContext::is_legal` is the only new decision procedure. The existing
   quiet fast path stays first; captures/checks use the effect path; castling
   keeps `Board::legal` as the fallback. Do not add a separate “capture
   legality” function that can drift.
2. **Keep `Board::legal` as the oracle.** Every widened branch must be covered
   by the plan 2 differential and by a new ladder-equivalence test against
   `Board::legal`. A fast path that accepts an illegal move is a correctness
   bug, not a performance trade-off.
3. **Do not remove `TrivialLegality` speculatively.** It avoids effect-context
   work for the common quiet case. Measure both `TrivialLegality`-first and
   effect-only variants; keep the faster one only if the differential is green.
4. **MG6 is profile-led and order-preserving.** Do not change stage order,
   per-pawn ordering, promotion-piece order, or castling ordering. The
   `tests/move_order_golden.rs` test is the hard gate.
5. **MG7 is not implemented.** MG1 lands the null-move API without a FEN
   round-trip. Revisit `from_fen` only if a real remaining consumer benchmark
   shows it is material, and then as a separate allocation-reduction plan.
6. **No new feature flags for this plan.** Use the existing
   `unsafe-optimizations` only if a change genuinely needs it; prefer safe code.

## Implementation

### MG4 — widen the ladder

Start from the plan 2 `EffectContext` and `TrivialLegality`. Add an internal
`FastLegality` (or extend `TrivialLegality`) that holds:

- the existing `checkers` and `safe_origins`;
- an `EffectContext`;
- the populated `StateInfo` reference or the fields needed by the fallback.

`generate_legal_with_state` and `for_each_legal_with_state` use:

```text
if trivial_quiet.is_trivially_legal(board, m) {
    accept
} else if effect.is_legal(board, m) {
    accept
} else {
    reject
}
```

`EffectContext::is_legal` already falls back to `Board::legal` for castling.
For en passant and captures it must use the effect path; if the differential
shows any uncertainty, keep the explicit `board.legal` fallback for that move
type until the effect path is proven.

Add a reference ladder in tests that mirrors the accepted fast paths exactly,
so a future edit cannot silently accept a different set of moves. The existing
`is_move_trivially_legal_reference` test is the model.

Do not change `generate_legal`'s public order or `MoveList` capacity.

### MG6 — pseudo-legal generator profile

Only after MG4, run a release profile on:

- the handoff `deep-win` FEN;
- the two profiled positions in `TRIVIAL_LEGALITY_CORPUS`;
- the start position;
- a capture-heavy and a promotion-heavy position.

Candidate safe micro-optimisations, in order of preference:

1. Hoist repeated `board.pieces_color_pt(us, pt)` calls into local bitboards
   where the compiler does not already do so.
2. Use the already-hoisted `occupied`, `them_bb`, and `ep_square` for all pawns
   (already done) and avoid re-reading `board.ep_square()` inside the loop.
3. Specialise the castling path so the common non-castling stream does not pay
   for its checks.
4. Mark the public stream and its helpers `#[inline]` only where the profile
   shows call overhead; do not blanket-inline large bodies.

Do not add a `MoveList` preallocation or a dynamic buffer; the list is already
fixed-capacity and stack-allocated.

Any change must keep the order golden and perft unchanged. If the profile shows
no clear win, record that and make no code change.

### MG7 — FEN fallback

No implementation. Record in the report:

- which FEN allocations remain (`parts: Vec<&str>`, `rows: Vec<&str>`);
- that the measured null-replay path is removed by `null_for`;
- that `from_fen` remains the parser of record for interchange and fixtures;
- the condition under which a future allocation-reduction plan would be
  justified (a remaining consumer benchmark with `from_fen` as a material
  symbol).

## Tests

### MG4

- `fast_legality_matches_board_legal`: over `TRIVIAL_LEGALITY_CORPUS`, the
  order-golden positions, and seeded random play-outs, every pseudo-legal move
  accepted by the widened ladder must be accepted by `Board::legal`, and every
  move rejected by both the ladder and `Board::legal` must agree.
- `fast_legality_matches_reference_ladder`: a verbatim reference of the
  intended fast paths, as in the existing trivial-legality tests.
- `fast_legality_corpus_covers_captures_checks_pins`: guard against a vacuous
  corpus. Count accepted captures, accepted checks, pinned origins, en-passant
  and castling fallbacks, and no-commoner positions.
- `generate_legal_order_unchanged`: the existing golden test plus a comparison
  of `generate_legal` and `for_each_legal` on the corpus.

### MG6

- The full order-golden and perft suites are the correctness gate.
- Add no new semantic test unless a new helper is introduced; if one is, test
  it against the existing implementation.

### MG7

- No test changes. The null-for differential from plan 1 is the guard that the
  FEN round-trip is no longer needed for null replay.

## Measurement protocol

Use the repository's profiling guide (`docs/perf/perf_guide.md`) and the
handoff's provenance style:

- release build with the crate's existing release profile;
- three repeats, alternating control/candidate, one process at a time;
- report median wall time and ratios, not a single run;
- keep counters identical (nodes, perft, move counts);
- record the host, rustc version, and commands.

If the measurement is within noise, do not claim a speed-up. The plan may still
land if it simplifies the code and keeps all gates green, but the report must
say so.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (41/41; at least depth 5)
- `cargo test --test move_order_golden`
- `cargo test --test verify_moves`
- `cargo run --release --example perft` on the start position and the two
  profiled positions, before and after.
- Profile with `perf` on a release build; record the top symbols.

## Files to touch

- `src/board.rs` — widened fast-legality ladder and tests.
- `src/movegen.rs` — use the widened ladder; profile-led generator edits if
  any.
- `src/effects.rs` — only if a measured effect-path change is needed.
- `README.md` / `CHANGELOG.md` — performance note if a change lands.
- `docs/plans/feedback/report_handoff1_3.md` — final report.

## Non-goals

- No FEN parser rewrite (MG7 fallback not taken).
- No new feature flag, no new dependency, no `unsafe` beyond the existing
  opt-in magic lookup.
- No change to move order, perft, hash, FEN, or `Outcome`.
- No search/eval/threading/tablebase work.

## Final task

Write `docs/plans/feedback/report_handoff1_3.md` with the profile before/after,
the widened-ladder differential results, perft/order results, whether MG6
produced a code change, and why MG7 was not implemented.
