# Code Cleanup Report 6

## Summary

Behaviour-neutral cleanup of the wishlist-3 evasion mask. Mask construction now
reuses the occupancy and commoner sets already snapshotted by `LegalNode`
instead of recomputing them from the view. No public signature, rule, key,
move order, benchmark row, or generated answer changed.

Plan: `docs/plans/cleanup/plan-code-cleanup6.md`.
Hardening report: `docs/plans/hardening/report_wishlist3.md`.

## Items completed

### 1. One mask-building path for production

`EvasionMask::from_parts(occupied, our_commoners, their_commoners, by_type,
ep_square, checkers)` now holds the rule. `LegalNode::from_view_with_checkers`
computes those node-constant parts once, stores them, and passes them to the
mask. The previous `from_view` recomputed `occupied()`, both `commoners()` sets,
and re-read the slider tables.

The old `from_view` name remains only as a `#[cfg(test)]` adapter for the
existing unit tests; its doc says production uses the snapshot parts.

### 2. Test seam

The test-only adapter keeps the mask unit tests readable without adding a
second production constructor.

## Verification

- `cargo test` — 311 passed, 0 failed.
- `cargo test --all-features` — 311 passed, 0 failed.
- `cargo test --release` — 311 passed, 0 failed.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings` —
  clean.
- `cargo fmt --check` — clean.
- rustdoc with `-D warnings` — clean.
- `cargo test --doc` — 18/18.
- all integration suites — pass.
- `verify_perft 6` — 41/41, 63.849 s.

Hot-path benchmark (`hotpath_bench 9 8`, three runs): candidate rows unchanged
within noise (corpus all 0.938–0.950x old context, in-check quiet
0.223–0.227x; play-outs all 0.986x, in-check quiet 0.282–0.286x). Corpus setup
dropped from about 2.53 ns to 2.37–2.40 ns per node (old context 2.22–2.27 ns);
the play-out setup row is flat within run noise. Perft medians:

| Case | Previous plan-2 run | this tree |
| --- | ---: | ---: |
| startpos depth 6 | 0.571 s | 0.559 s |
| position 2 depth 5 | 0.244 s | 0.242 s |
| position 13 depth 5 | 0.306 s | 0.303 s |

No regression.

## Changed files

- `src/position.rs` — `EvasionMask::from_parts`, snapshot reuse,
  test-only `from_view`.
- `docs/plans/cleanup/plan-code-cleanup6.md`, this report.

## Unresolved parts

None for this cleanup. The wishlist-3 measurement caveat (small in-check
capture/commoner fall-through rows) and the future-`MoveType` review remain as
recorded in the wishlist-3 reports.
