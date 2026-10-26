# Implementation Report — Plan 2: Local Bitboard Cache in `Board::legal`

**Plan:** `docs/plans/perf_analysis7/plan2.md`
**Status:** Implemented, measured, and **reverted** because the fixed-FEN acceptance
criteria were not met on test #13.

---

## 1. Summary

The planned change was applied only inside `Board::legal` in `src/board.rs`. Three local
bitboards were hoisted to the top of the function and reused:

```rust
let base_occupied = self.occupied();
let our_commoners_base = self.commoners(us);
let them_commoners_base = self.commoners(them);
```

The exact substitutions were:

| Original | Replaced with |
|----------|---------------|
| `let occupied = self.occupied();` (castling block) | `let occupied = base_occupied;` |
| `self.commoners(them) & attacks::king_attacks(sq)` | `them_commoners_base & attacks::king_attacks(sq)` |
| `let mut occupied = self.occupied() ^ Bitboard::square_bb(from);` | `let mut occupied = base_occupied ^ Bitboard::square_bb(from);` |
| `let pre_non_pawns = self.occupied() ^ pre_pawns;` | `let pre_non_pawns = base_occupied ^ pre_pawns;` |
| `let mut our_commoners = self.commoners(us) & occupied;` | `let mut our_commoners = our_commoners_base & occupied;` |
| `them_pr_count <= 1 && (self.commoners(them) & occupied).is_empty()` | `them_pr_count <= 1 && (them_commoners_base & occupied).is_empty()` |
| `let them_commoners = self.commoners(them);` | `let them_commoners = them_commoners_base;` |

No other function, public signature, branch, early return, or call order was changed.
In particular, no `StateInfo` fields and no capture blast pre-filter were added.

The change passed all correctness checks, but repeated fixed-FEN measurements showed a
small stable regression on test #13 (about +0.2 % to +0.9 %). Test #14 improved by about
0.7 % to 1.3 %. Because the plan requires a #13 improvement of at least ~1 % and forbids
a regression on either fixed FEN, the change was reverted.

---

## 2. Measurements

The `perft` example prints only the node count; it does not print elapsed time. Timings
were therefore taken with `date +%s%N` around each invocation, using the same fixed FENs
from the plan. Node counts were unchanged throughout:
`2,160,817,389` for #13 and `245,931,633` for #14.

### 2.1 Initial baseline

FEN #13:
`rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5`

| Run | #13 time (s) |
|-----|-------------:|
| 1 | 11.644 |
| 2 | 11.738 |
| 3 | 11.780 |
| 4 | 11.698 |
| 5 | 11.578 |
| 6 | 11.634 |
| 7 | 11.682 |
| **Median** | **11.682** |

FEN #14:
`r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15`

| Run | #14 time (s) |
|-----|-------------:|
| 1 | 1.486 |
| 2 | 1.504 |
| 3 | 1.507 |
| 4 | 1.496 |
| 5 | 1.495 |
| 6 | 1.507 |
| 7 | 1.493 |
| **Median** | **1.496** |

### 2.2 First patched measurement

| Run | #13 time (s) | #14 time (s) |
|-----|-------------:|-------------:|
| 1 | 11.721 | 1.516 |
| 2 | 11.639 | 1.509 |
| 3 | 11.645 | 1.485 |
| 4 | 11.705 | 1.486 |
| 5 | 11.767 | 1.479 |
| 6 | 11.663 | 1.478 |
| 7 | 11.717 | 1.508 |
| **Median** | **11.705** | **1.486** |

Change versus initial baseline: **#13 +0.20 % (slower)**, **#14 −0.67 % (faster)**.

### 2.3 Same-session A/B recheck

Because the first result was close to the noise floor, the baseline was restored,
rebuilt, and remeasured in the same session. The patched tree was then restored,
rebuilt, and remeasured again.

| Measurement | Baseline median | Patched median | Change |
|-------------|----------------:|---------------:|-------:|
| Fixed FEN #13, depth 6 | 11.640 s | 11.742 s | **+0.88 %** (slower) |
| Fixed FEN #14, depth 6 | 1.500 s | 1.480 s | **−1.33 %** (faster) |

The recheck confirms the direction of the first measurement: #14 improves, while #13
regresses slightly. The #13 regression was not a one-off.

### 2.4 Full suite

| Tree | Total time | Result |
|------|-----------:|--------|
| Patched | 76.433 s | 41/41 passed |
| Reverted baseline | 77.495 s | 41/41 passed |

The patched full-suite run was about 1.4 % faster, but the suite is noisy on this host
(analysis §1 and `report1.md`). This is supporting evidence only and did not override the
fixed-FEN #13 result.

### 2.5 Did the expected gain reproduce?

Partially, but not in the required direction for test #13.

Analysis §4.2 measured **−2.1 % on #13** and **−1.1 % on #14**. On this host, #14 improved
by about 1.3 %, matching or slightly exceeding the expectation. Test #13 instead showed a
repeatable small regression of about 0.2–0.9 %. The expected #13 win did not reproduce.

---

## 3. Correctness

While the patch was applied:

- `cargo fmt` — passed.
- `cargo clippy --all-targets` — no new warnings; the only warnings were the pre-existing
  `manual_contains` suggestions in `examples/test_d5g8.rs`.
- `cargo test` — all tests passed:
  - 54 unit tests
  - `move_order_golden`: 1 passed
  - `perft_tests`: 4 passed
  - `verify_moves`: 1 passed
  - doc tests: 1 passed
- `cargo doc` — passed.
- `cargo run --release --example verify_perft` — **41/41 passed** at depths 1–6.

After reverting to the original `src/board.rs`, the same checks were run again on the final
tree:

- `cargo fmt --check` — passed.
- `cargo clippy --lib` — no warnings.
- `cargo test` — all 54 unit tests plus all integration and doc tests passed.
- `cargo doc` — passed.
- `cargo run --release --example verify_perft` — **41/41 passed**.

`git diff --stat` confirms that no source file remains modified; the only untracked file is
this report (and the plan file itself).

---

## 4. Decision

**Reverted.**

The plan's acceptance criteria were:

- Test #13 median improves by at least ~1 % — **not met** (it regressed slightly).
- Test #14 median improves by at least ~0.5 %, or is within noise but not a stable
  regression — **met** (improved).
- The change must not regress either fixed FEN in repeated runs — **not met** (#13 was
  slower in both measurement rounds).

The plan explicitly says to revert if the result is neutral or a stable regression rather
than keep a speculative optimization. The #14 gain is real but small, and it does not
justify a repeatable #13 regression.

---

## 5. Profile Confirmation

No profile was captured for the patched binary because the change was not retained. The
optional profile step was therefore skipped.

---

## 6. Notes and Constraints Observed

- Only the body of `Board::legal` was changed during the experiment.
- No `StateInfo` cache, no capture blast pre-filter, and no change to `populate_state`,
  `do_move`, or `undo_move` was made.
- The implementation followed the plan's exact substitution table; no helper function or
  new struct was introduced.
- The observed host timings differ materially from analysis §1 (for example, #13 baseline
  is about 11.6 s here rather than 8.457 s), so only same-session A/B results were used for
  the decision.

---

## 7. Next Steps / Recommendation

- Keep the original `Board::legal` implementation.
- Before attempting analysis §5.4 (boolean `square_attacked_by` early exit), re-baseline
  both fixed FENs again on this host, because Plan 1 and Plan 2 both produced results that
  differ from the original analysis host.
- The boolean attack probe is small and was measured as a modest win in analysis, but
  code-layout effects on this host have now twice turned an expected gain into a
  neutral/negative fixed-FEN result. Measure it with the same repeated #13/#14 A/B method.
- Consider whether to retry §5.6 (`StateInfo` commoner cache alone) only after the local
  cache is settled; this plan did not test that variant.
- Do not retry Plan 1's no-compaction `perft` shape without new profiling evidence.
