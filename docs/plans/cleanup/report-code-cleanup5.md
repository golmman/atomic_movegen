# Code Cleanup Report 5

## Summary

Behaviour-neutral cleanup of the wishlist-2 code after the hardening session.
The only structural change is a single crate-private helper for the two-rank
pawn ep target; the rest is contract wording. All tests, perft values, keys,
move order, and public signatures are unchanged.

Plan: `docs/plans/cleanup/plan-code-cleanup5.md`.
Hardening report: `docs/plans/hardening/report.md`.

## Items completed

### 1. Shared double-push ep helper

Added in `src/position.rs`:

```rust
#[inline]
pub(crate) fn double_push_ep(us: Color, from: Square) -> Square
```

It is now the only place that derives the crossed square. The three previous
copies were replaced:

| File | Previously | Now |
| --- | --- | --- |
| `src/board.rs` (`do_move`) | inline `match us { from ± 8 }` | `position::double_push_ep(us, from)` |
| `src/position.rs` (`child_view`) | inline `match us { from ± 8 }` | `double_push_ep(us, from)` |
| `src/effects.rs` (`child_effect`) | inline `match us { from ± 8 }` | `position::double_push_ep(us, from)` |

The helper deliberately does not validate the start rank: the callers already
test the pawn type and the two-rank delta, and the hardening helper
`is_valid_ep_target` owns the structural check at the consumer. This keeps the
hot `do_move` codegen a single inlined expression.

### 2. Totality wording

`legal_in`, `legal_with_counts`, and `LegalNode::legal` now state that the
shared body answers `false` for a side with no commoner, while
`Board::legal`/`LegalityContext::legal` still apply the caller-side
`Counts`/`StateInfo` game-over guard. No code semantics changed in this step.

## Verification

- `cargo test` — 193 unit tests, all integration suites, 18 doctests; pass.
- `cargo test --features unsafe-optimizations` — pass.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` — no
  warnings.
- `cargo run --release --example verify_perft 6` — 41/41.
- Perft A/B against a clean `HEAD` archive, alternating runs:
  Kiwipete depth 5 median 0.7949 s (HEAD) vs 0.7972 s (current), ratio 1.003x;
  startpos depth 6 median 0.5822 s vs 0.5812 s, ratio 0.998x. Identical node
  counts. No regression above the ~2 % blocker.
- Differential suites `counts`, `legality_context`, `child_effect`, `null_for`
  are exact.

## Items intentionally unchanged

| Item | Reason |
| --- | --- |
| `LegalData` trait shape / board-path view borrow | A `LegalNode` construction on the board path was a measured regression in wishlist-2 plan 2. |
| `child_effect` key/rights body vs `child_view` | The two traverse different representations and were measured; merging them risks the hot path for no readability win. |
| Examples, benchmark rows, package metadata | Out of scope for a cleanup session; the release gate already pinned them. |
| Pre-existing long markdown lines | Not introduced by wishlist 2; reflowing would add diff noise. |

## Changed files

- `src/position.rs` — `double_push_ep`, three call sites reduced to it, doc
  wording.
- `src/effects.rs` — one call site uses the helper.
- `src/board.rs` — `do_move` uses the helper.
- `docs/plans/cleanup/plan-code-cleanup5.md`, this report.
