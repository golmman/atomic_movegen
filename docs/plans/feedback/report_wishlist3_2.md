# Implementation report — plan 2 (integration, combined measurement, release gate)

Status: **released as 2.11.0** in the working tree, not committed, tagged or
published. Plan 1 landed W10 as an additive constructor; this plan verified it,
pinned the public surface, corrected a benchmark specialization artifact, ran
the combined differential, all global gates, the package checks and the
same-host measurements, and prepared the release text. W10 is closed for this
tree. The owner still performs the commit/tag/publish steps in §11.

Plan: `docs/plans/feedback/plan_wishlist3_2.md`.
Index: `docs/plans/feedback/plan_wishlist3_index.md`.
Input: `docs/plans/feedback/report_wishlist3_1.md`.
Baseline: released 2.10.0 (`5083647`).

## 1. Plan validation

The expected plan-1 shape was confirmed from the report and the tree before
editing:

| Item | Landed state | Verified |
| --- | --- | --- |
| `W10` | `Board::legality_context_with_state`, rejection-only `EvasionMask`, focused fixtures, bench rows | yes |
| Existing context | `Board::legality_context(counts)` and check-blind `legal` unchanged | yes |
| Board path | `Board::legal`, `legal_in`, generator, effects unchanged | yes |
| Keys/perft | no key, move-order, FEN, terminality or `ZOBRIST_VERSION` change | yes |

Two preconditions differed from the plan's literal wording:

1. Plan 1 left the version at 2.10.0 and did not pin the constructor, exactly as
   its report said. This plan owns both.
2. Plan 1's measurement verdict was conditional because its literal per-class
   reading had fall-through rows above 2%. The first combined re-run reproduced
   an even larger apparent regression, which led to the measurement correction
   in §3. No release was made on the uncorrected numbers.

The absent downstream plan-9 artifacts (`report9.md`, `measurements/plan9/`)
remain absent. Nothing here reproduces a downstream tree, `.pvr`, proof
acceptance, or whole-binary saving.

## 2. Landed set from plan 1

- Public constructor: `Board::legality_context_with_state(counts, &state)`.
- It reads only `state.checkers`, keeps the `Counts` game-over guard, and
  stores one rejection-only destination set per node.
- `pass = !0` when not in check, a missing commoner, or touching commoners.
- One slider checker: `between(commoner, checker) | occupied |
  king_attacks(commoner)`, plus the stored en-passant target.
- A leaper or multiple checkers: `occupied | king_attacks(commoner)`, plus the
  stored en-passant target.
- `LegalNode::legal` rejects only destinations outside `pass`; every other move
  still runs the shared `legal_in` body.
- The check-blind constructor, `Board::legal`, generator, `StateInfo` layout,
  keys, move order, FEN and terminality are unchanged.

## 3. Release decision and measurement correction

The first combined `hotpath_bench` run after plan 1 showed the new context at
roughly 1.43–1.52x the old context on out-of-check rows, which would have been
a blocker. Diagnosis:

- outside check both contexts store the same `pass = !0` value, so they should
  be identical;
- swapping which context the benchmark rows called made the regression
  disappear;
- black-boxing both stored contexts made the comparison reproduce report 1.

The cause is constant propagation: the check-blind constructor's `pass` is
statically known to be `!0` at the call site, while the state-aware
constructor's `pass` is data-dependent. The harness was comparing a specialized
old path with a generic new path. `CandidateEntry` now stores both contexts
through `black_box`, so both are equally opaque and the timed rows measure the
runtime call cost a caller sees. This is a benchmark-only correction; no
release-code path changed.

Verdict: **release 2.11.0**. Exactness passes, the in-check rows win by
0.22–0.64x, all-candidate rows are 0.94–0.99x, and out-of-check rows are
0.99–1.01x. The small in-check capture/commoner fall-through subsets are
1.03–1.06x the old check-blind context, but they stay at or below
`Board::legal` and run the same shared body plus one destination test. They are
recorded rather than hidden; they are not a blocker for the additive opt-in
surface.

## 4. Public API pin

`tests/public_api.rs` gained
`state_aware_legality_context_resolves_and_answers`:

- builds a populated `StateInfo` for the in-check fixture
  `4r1k1/8/8/8/8/8/8/r1B1K3 w - - 0 1`;
- asserts the fixture has checkers;
- calls `board.legality_context_with_state(counts, &state)`;
- asserts `occupied()` equals `board.occupied()`;
- compares `legal(m)` with `board.legal(m, &state)` for every pseudo-legal
  move;
- checks the zero-count game-over guard rejects every candidate.

The existing `legality_context_resolves_and_answers` pin is unchanged.
`cargo test --test public_api`: 8/8 pass.

## 5. Combined differential

`cargo test --test legality_context -- --nocapture`:

### Corpus and fixtures

- 90 positions: 6 terminal, 17 in check, 73 not in check, 9 slider-block,
  8 empty-block, 7 pinned, 4 ep, 19 with castling rights. (The plan-2 run
  before the follow-up hardening session had 87 positions; three permissive
  multiple-commoner fixtures were added afterwards.)
- 1,750 candidates: 8 castling, 2 ep, 1,417 legal.
- In check: 242 candidates (214 quiet, 27 capture, 0 ep, 1 castling,
  83 commoner), 130 predicted mask rejections, **0 legal among them**.

### Seeded play-outs

- 14,960 positions: 82 terminal, 518 in check, 14,442 not in check,
  252 slider-block, 266 empty-block, 344 pinned, 354 ep, 1,756 with rights.
- 283,924 candidates: 350 castling, 38 ep, 261,430 legal.
- In check: 9,544 candidates (8,822 quiet, 716 capture, 2 ep, 4 castling,
  3,326 commoner), 4,928 predicted mask rejections, **0 legal among them**.

Every candidate asserted

```text
evasion_ctx.legal(m) == ctx.legal(m)
                     == board.legal(m, &full)
                     == board.legal(m, &counts_only)
                     == board.is_legal_with_state(m, &counts_only)
```

and `evasion_ctx.occupied() == board.occupied()`. The focused fixtures still
cover one slider inside/outside the block ray, double check, knight/pawn check,
touching commoners, castling in check, ep in check, promotion pushes that
block, and the inside/outside second-line moves.

## 6. Combined benchmark

Command (three runs, `hotpath_bench 9 8`; values below are the median of the
three runs; setup rows are ns per node):

```sh
cargo run --release --example hotpath_bench 9 8
```

### Corpus (77 candidate nodes, 1,724 candidates)

| Row | Board::legal | old ctx | evasion ctx | evasion/ctx | evasion/board |
| --- | ---: | ---: | ---: | ---: | ---: |
| all candidates | 8.30 | 7.86 | 7.41 | **0.943x** | **0.893x** |
| in-check all | 7.32 | 6.94 | 3.47 | **0.500x** | **0.474x** |
| in-check quiet | 7.50 | 7.12 | 1.61 | **0.226x** | **0.215x** |
| out-of-check quiet | 8.13 | 7.63 | 7.55 | 0.990x | 0.929x |
| out-of-check all | 8.44 | 7.98 | 8.09 | 1.014x | 0.959x |
| setup | — | 2.37 | 2.53 | 1.068x | — |

### Seeded play-outs (38,700 candidate nodes, 835,794 candidates)

| Row | Board::legal | old ctx | evasion ctx | evasion/ctx | evasion/board |
| --- | ---: | ---: | ---: | ---: | ---: |
| all candidates | 9.70 | 8.98 | 8.87 | **0.988x** | **0.914x** |
| in-check all | 9.51 | 9.08 | 5.78 | **0.637x** | **0.608x** |
| in-check quiet | 8.97 | 8.69 | 2.50 | **0.288x** | **0.279x** |
| out-of-check quiet | 8.66 | 8.09 | 8.03 | 0.993x | 0.927x |
| out-of-check all | 9.53 | 8.99 | 8.90 | 0.990x | 0.934x |
| setup | — | 3.76 | 4.06 | 1.080x | — |

The full per-class rows are in `report_wishlist3_1.md` and in the harness
output. The stable wishlist-3 summary is now printed by `hotpath_bench` after
the wishlist-2 summary.

In-check capture/commoner fall-through rows (representative full runs):

| Dataset row | old ctx | evasion ctx | evasion/ctx | evasion/board |
| --- | ---: | ---: | ---: | ---: |
| corpus in-check capture | 6.16 | 6.12 | 0.994x | 0.950x |
| corpus in-check commoner | 7.25 | 7.30 | 1.008x | 0.952x |
| play-outs in-check capture | 7.60–7.97 | 7.83–8.11 | 1.018–1.031x | 0.958–0.971x |
| play-outs in-check commoner | 10.30–10.32 | 10.82–10.86 | 1.048–1.055x | 0.988–0.991x |

Instruction diagnostics were not repeated. Plan 1's temporary `perf stat`
probe found 184.18 instructions/call for the old context and 72.80 for the new
context on in-check quiet candidates; that probe was not committed, and this
plan keeps the instruction count diagnostic rather than a portable gate.

## 7. Release gates

All gates pass on the final tree:

| Gate | Result |
| --- | --- |
| `cargo test` | 310 passed, 0 failed |
| `cargo test --all-features` | 310 passed, 0 failed |
| `cargo test --release` | 310 passed, 0 failed |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | clean |
| `cargo test --doc` | 18/18 pass |
| `perft_tests`, `move_order_golden`, `verify_moves`, `conformance`, `effects`, `post_move`, `terminal_after`, `child_state`, `child_effect`, `ep_key`, `null_for`, `legality_context`, `counts`, `public_api`, `interchange`, `robustness` | all pass |
| `cargo run --release --example verify_perft 5` | 41/41, 2.227 s |
| `cargo run --release --example verify_perft 6` | 41/41, 64.898 s |

Perft timing (`target/release/examples/perft`, three runs, median) against the
2.10.0 numbers recorded in `report_wishlist2_5.md` on this host:

| Case | 2.10.0 report | this tree | Ratio |
| --- | ---: | ---: | ---: |
| startpos depth 6 | 0.582 s | 0.571 s | 0.981x |
| position 2 depth 5 | 0.253 s | 0.244 s | 0.964x |
| position 13 depth 5 | 0.310 s | 0.306 s | 0.987x |

No perft, key, move-order, FEN, terminality or existing legality answer
changed.

## 8. Package check and publish dry-run

`cargo package --list --allow-dirty` lists 42 packaged files: the 11 source
modules, all integration tests, markdown/data fixtures, `Cargo.toml`,
`Cargo.toml.orig`, `Cargo.lock`, `README.md`, `CHANGELOG.md`, `LICENSE`, and the
generated `.cargo_vcs_info.json`. No `examples/` file is packaged, and the new
public item is inside the packaged `src/board.rs`.

`cargo publish --dry-run --allow-dirty`:

- packages `atomic-movegen v2.11.0` (42 files, 1.2 MiB, 270.8 KiB compressed);
- verifies the packaged crate by compiling it;
- prints the expected “ignoring example …” warnings for every example;
- aborts the upload because of the dry run.

No dependency was added; `Cargo.lock` changed only its own package version
2.10.0 → 2.11.0.

## 9. Changed files and package diff

- `Cargo.toml` — version 2.11.0.
- `Cargo.lock` — generated package version 2.11.0 (git-ignored, packaged).
- `CHANGELOG.md` — `[2.11.0]` section with the landed contract and same-host
  measurements.
- `README.md` — dependency snippet 2.11.0 and the state-aware candidate-loop
  bullet with the same-host ratios.
- `examples/hotpath_bench.rs` — both contexts stored through `black_box`; new
  stable-row `wishlist 3 summary`.
- `tests/public_api.rs` — release pin for
  `Board::legality_context_with_state`.
- `docs/plans/feedback/report_wishlist3_2.md` — this report.
- `docs/plans/hardening/plan_wishlist3.md`, `report_wishlist3.md` — follow-up
  hardening session (tests only).
- `docs/plans/cleanup/plan-code-cleanup6.md`, `report-code-cleanup6.md` —
  follow-up cleanup (mask construction reuses the node snapshot).

No source module, dependency, `unsafe`, allocation, `StateInfo` layout,
`ZOBRIST_VERSION`, move order, FEN or terminality changed. Excluding the new
report, the package diff is five files and no new packaged paths.

## 10. Problems, deviations and unresolved parts

1. **Benchmark specialization correction (deviation).** The plan did not say
   to change the existing harness rows. The initial combined run produced a
   false fall-through regression because LLVM specialized the check-blind
   sentinel. Both contexts are now black-boxed before storage; the report
   records both the uncorrected observation and the corrected measurement.
   No release-code behavior changed.
2. **Small in-check fall-through rows remain above 2%.** In-check capture and
   commoner rows are 1.018–1.055x the old context on play-outs and ~1.00x on
   the corpus; all remain at or below `Board::legal`. They are the same shared
   body plus one destination test. The primary in-check row and every
   out-of-check row are inside the blocker.
3. **The literal plan-1 “no class row above 2%” reading is still not met.**
   The release decision uses the plan-2 gate: exactness, a repeatable in-check
   win, additive-only surface, no existing-path or perft regression. That gate
   passes.
4. **No downstream artifacts were reproduced.** The wishlist's `82.0%`,
   `0.72e9`, `.pvr` and 9/9 proof claims are downstream-only.
5. **Instruction diagnostics were not repeated.** Plan 1's temporary probe is
   not in the tree; the committed harness remains `examples/hotpath_bench.rs`.
6. **No missing correctness test is known.** The deliberate gap from plan 1
   remains: a future `MoveType` variant could add an encoding the permissive
   pass set does not anticipate. Any such change must revisit the mask and the
   differential.

## 11. Owner's manual publish checklist

Not performed by this plan:

- [ ] Review the diff, this report, the public API pin and the changelog.
- [ ] Commit the five changed files and this report.
- [ ] Re-run `cargo package --list` without `--allow-dirty`.
- [ ] Re-run `cargo publish --dry-run` without `--allow-dirty`.
- [ ] Tag the release and publish `atomic-movegen 2.11.0` from the owner's
      account.
- [ ] Confirm the published crate contains `src/board.rs` with
      `legality_context_with_state` and no `examples/`.

## 12. Downstream handoff checklist

Outside this repository and not modified here:

- [ ] Trigger the downstream owner-side plan/report for the pin update.
- [ ] Update the downstream pin to 2.11.0 and `PROVENANCE.md` with the exact
      commit/tag.
- [ ] Run the downstream differential and gates against this release.
- [ ] Record the same-session `MD6` row.
- [ ] Treat the downstream `core`/`easy-win`/`deep-win` runs, `.pvr` and proof
      acceptance as separate evidence. This report makes no claim about them.
