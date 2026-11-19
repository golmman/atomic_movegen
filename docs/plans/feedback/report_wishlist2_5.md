# Implementation report — plan 5 (integration, combined measurement and release)

Status: **landed** as the wishlist-2 integration and release gate. The release
version is **2.10.0**, naming only the four plans that landed measured, exact
additive API. No existing answer, perft value, move order, FEN output,
terminality answer, `Always`/`Legal` key, or `ZOBRIST_VERSION` changed. This
report does not publish, tag, or commit; the owner checklist is at the end.

Plan: `docs/plans/feedback/plan_wishlist2_5.md`.
Index: `docs/plans/feedback/plan_wishlist2_index.md`.

## 1. Landed set collected from reports 1–4

| Plan | Item | Report status | Public surface landed |
| --- | --- | --- | --- |
| 1 | W6, counts-only state | landed | `Counts`, `Board::counts`, `Board::populate_counts`, precondition docs |
| 2 | W7, occupancy-cost legality | landed; exact and cheaper, absolute ≤ 100-instruction target open | `LegalityContext`, `Board::legality_context` |
| 3 | W8, effect + child key | landed; exact and cheaper than `PostMove`, absolute ≤ 120-instruction target open | `ChildEffect`, `Board::child_effect`, `Board::child_effect_with_occupancy` |
| 4 | W9, advanced-clock null | landed | `Board::null_for_advanced` |

Plan 2's report states the shipping verdict: the context is kept because it is
exact and measurably cheaper than `Board::legal`, while the wishlist's numeric
target is not reached. Plan 3's report states field-for-field equality and the
same cheaper-than-the-existing-path verdict. Both open numeric items remain
open in the wishlist response; the release notes claim the API, not the missing
instruction ceilings.

## 2. Release metadata and public API pin

- `Cargo.toml`: version `2.9.0` → `2.10.0`.
- `CHANGELOG.md`: the `Unreleased` section is now
  `## [2.10.0] - 2026-11-19`, with the release contract paragraph and the six
  additive entries already written by plans 1–4. No reverted or open item is
  claimed.
- `README.md`: dependency snippet updated to `atomic-movegen = "2.10.0"`; the
  hot-path/API descriptions already name all four landed shapes.
- `src/lib.rs`: crate-root exports and the engineering-contract allocation list
  already include `Counts`, `LegalityContext`, `ChildEffect`,
  `Board::child_effect`, `Board::child_effect_with_occupancy`, and
  `Board::null_for_advanced`; verified complete.
- `tests/public_api.rs`: the header now names the 2.10.0 notes. The existing
  release pin already calls every landed item from the crate root and asserts
  the cheap contracts (`Counts::from(&state) == board.counts()`,
  `ctx.occupied() == board.occupied()`, `ChildEffect::rule50 ==
  board.rule50_after`, `ChildEffect::key == board.hash_after`, and
  `null_for_advanced`/`null_for` resolution).

Public-API pin result: **7/7 pass**.

## 3. Combined differential

The landed plans' integration suites were run in one invocation:

```sh
cargo test --test counts --test legality_context --test child_effect \
  --test null_for -- --nocapture
```

Because the plan's files-to-touch list does not include a new combined test and
no landed plan left a correctness gap, this uses the four existing differential
suites together rather than inventing a fifth copy of the corpora. Each suite
walks the same perft/conformance/effect/board fixtures and seeded play-outs:

| Check | Corpus / fixtures | Seeded play-outs |
| --- | ---: | ---: |
| counts-only legality (`tests/counts.rs`) | 100 positions, 4 zero-count, 2,078 candidates, 1,706 legal | 14,960 positions, 82 zero-count, 283,924 candidates, 261,430 legal |
| context legality (`tests/legality_context.rs`) | 87 positions (6 terminal, 16 check, 7 pinned, 4 ep, 19 with rights), 1,715 candidates (8 castling, 2 ep), 1,388 legal | 14,960 positions (82 terminal, 518 check, 344 pinned, 354 ep, 1,756 with rights), 283,924 candidates (350 castling, 38 ep), 261,430 legal |
| child effect, both modes (`tests/child_effect.rs`) | 102 positions, 2,155 candidates, 205 captures, 133 double pushes, 133 stored ep (11 legal, 122 removed) | 14,960 positions, 283,924 candidates, 20,566 captures, 10,310 double pushes, 10,310 stored ep (962 legal, 9,348 removed) |
| advanced null (`tests/null_for.rs`) | six fixtures × both attackers; saturation, 50-move branch, large fullmove, and `null_for` distinction rows | — |

Every assertion is an equality with the existing oracle:
`Board::legal` with a full `StateInfo`, a cloned `do_move` child, and the
`Setup`-built expected board. The differential result is exact.

## 4. Combined benchmark

Command:

```sh
cargo run --release --example hotpath_bench 15 16
```

Each row is the median of 15 batches; absolute ns are same-host only. New rows
are built from the same per-class tables plans 1–4 added; this summary is the
release view.

### Corpus (78 boards / 77 candidate nodes)

| Wishlist-2 row | New ns/call | 2.9.0-shape ns/call | Ratio |
| --- | ---: | ---: | ---: |
| setup: `populate_counts` / `populate_state` | 1.27 | 8.13 | 0.156x |
| legality: `ctx.legal` / `Board::legal` (all) | 8.12 | 8.57 | 0.948x |
| child-A: `child_effect` / `post_move_and_key` (all) | 11.62 | 27.79 | 0.418x |
| child-L: `child_effect` / `post_move_and_key` (all) | 13.26 | 40.33 | 0.329x |
| null: `null_for_advanced` / `null_for` | 3.44 | 33.57 | 0.102x |
| null: `null_for_advanced` / `Setup` rebuild | 3.44 | 95.42 | 0.036x |

`LegalityContext` construction costs 2.17 ns/node on this corpus.

### Seeded play-outs (38,706 boards / 38,700 candidate nodes)

| Wishlist-2 row | New ns/call | 2.9.0-shape ns/call | Ratio |
| --- | ---: | ---: | ---: |
| setup: `populate_counts` / `populate_state` | 1.85 | 9.57 | 0.194x |
| legality: `ctx.legal` / `Board::legal` (all) | 9.02 | 9.66 | 0.934x |
| child-A: `child_effect` / `post_move_and_key` (all) | 12.08 | 28.72 | 0.420x |
| child-L: `child_effect` / `post_move_and_key` (all) | 12.98 | 40.95 | 0.317x |
| null: `null_for_advanced` / `null_for` | 3.90 | 56.29 | 0.069x |
| null: `null_for_advanced` / `Setup` rebuild | 3.90 | 175.02 | 0.022x |

`LegalityContext` construction costs 3.62 ns/node on the play-outs. The full
per-class tables remain in reports 2–4 and are printed by the harness. No
instruction-count probe was rerun for the integration; the per-plan diagnostics
(`perf stat -e instructions:u`) are recorded in reports 2–4 and are explicitly
host diagnostics, not portable gates.

## 5. Perft and timing

- `cargo run --release --example verify_perft 5` — **41/41** passed, 2.27 s.
- `cargo run --release --example verify_perft 6` — **41/41** passed, 65.45 s.
- Perft timing against the `v2.9.0` tag (`af3faf0`), three runs each, median:

| Case | v2.9.0 median | v2.10.0 median | Ratio |
| --- | ---: | ---: | ---: |
| startpos depth 6 (118,926,425 nodes) | 0.590 s | 0.582 s | 0.986x |
| position 2 depth 5 (51,225,398 nodes) | 0.250 s | 0.253 s | 1.011x |
| position 13 depth 5 (53,833,718 nodes) | 0.311 s | 0.310 s | 0.998x |

No regression above the ~2 % blocker; the largest movement is +1.1 % on one
midgame case, inside the observed run-to-run spread.

## 6. Release gates

| Gate | Result |
| --- | --- |
| `cargo test` | pass: 192 unit tests, all integration suites, 18 doctests |
| `cargo test --all-features` | pass |
| `cargo test --release` | pass |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | no warnings |
| `cargo test --doc` | 18/18 pass |
| targeted `perft_tests`, `move_order_golden`, `verify_moves`, `conformance`, `effects`, `post_move`, `terminal_after`, `child_state`, `ep_key`, `null_for`, `public_api`, `interchange`, `robustness` | all pass |
| `cargo package --list --allow-dirty` | 42 files, no examples, source/tests/data present |
| `cargo publish --dry-run --allow-dirty` | packaged + verified, upload aborted by dry run |

The package is **42 files, 1.2 MiB (262.8 KiB compressed)**, version
`2.10.0`. It contains `src/**`, `tests/**`, the markdown/data fixtures,
`Cargo.toml`, generated `Cargo.lock`, `README.md`, `CHANGELOG.md`, and
`LICENSE`. Examples are intentionally outside the package. No dependency was
added and no `unsafe` was introduced.

`cargo package --list` and `cargo publish --dry-run` required `--allow-dirty`
because the plan-5 files are not committed yet. The clean, owner-committed run
is the remaining release check.

## 7. Package diff / changed files

Plan-5 changes:

- `Cargo.toml` — version `2.10.0`.
- `CHANGELOG.md` — `[2.10.0] - 2026-11-19` release section.
- `README.md` — dependency snippet `2.10.0`.
- `tests/public_api.rs` — release-pin header names 2.10.0.
- `examples/hotpath_bench.rs` — compact wishlist-2 summary (corpus and
  play-outs) built from the existing rows, plus the release row names.
- `docs/plans/feedback/report_wishlist2_5.md` — this report.

Plans 1–4 supplied the source, integration tests, and per-class benchmark rows.
No source module changed in plan 5.

## 8. Problems and deviations

1. **Dirty-tree package checks.** `cargo package --list` and
   `cargo publish --dry-run` refuse a tree with uncommitted changes. They were
   rerun with `--allow-dirty`; both then packaged and verified `2.10.0`, and the
   dry run aborted the upload as expected. The final clean run belongs to the
   owner after committing.
2. **No separate combined test file.** The combined differential is the four
   landed suites run together, with the shared corpora and oracle. The plan's
   files-to-touch list excluded a new test and no landed plan left a gap; adding
   a fifth copy would duplicate the same walk rather than increase coverage.
3. **No new instruction diagnostic.** The integration report reuses the
   per-plan `perf stat` rows as host diagnostics. The plan made those optional;
   the ns/ratio gate is the release measurement.
4. **`Cargo.lock` is generated and ignored by git.** It is nevertheless present
   in the package with the `2.10.0` root package version, matching
   `Cargo.toml`. No dependency entry appeared.

## 9. Unresolved parts and missing tests

- W7's ≤ 100-instruction and W8's ≤ 120-instruction absolute targets remain
  open as recorded in reports 2 and 3. The shipped API is a measured
  improvement, not a promise of those ceilings.
- No public general `Board` clock setter was added; `null_for_advanced` is the
  only new clock-mutating shape (report 4).
- No downstream search-tree, node-count, or whole-binary measurement was made
  or claimed.
- No allocation-instrumentation test was added; the allocation-free claims rest
  on the implementation shapes and the existing engineering contract.

## 10. Owner's manual publish checklist

1. Review the combined benchmark in this report and the per-class tables in
   reports 1–4.
2. Review the diff and the `[2.10.0]` changelog section; confirm no unintended
   item is named.
3. Commit the plan-5 changes (including the ignored generated `Cargo.lock` only
   if the project's release process wants it).
4. Re-run `cargo package --list` and `cargo publish --dry-run` without
   `--allow-dirty` on the committed tree.
5. Tag `v2.10.0` and publish from the owner account; this repository does not
   publish or tag.
6. Update any downstream pin/provenance file outside this repository.
