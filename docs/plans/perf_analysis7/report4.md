# Implementation Report — Plan 4: Category-Specialized Legality

**Plan:** `docs/plans/perf_analysis7/plan4.md`
**Status:** Implemented and measured, then **reverted**. The category split was functionally correct,
but the best retained configuration was only about **0.3–0.6 % faster on #13** and within noise on
#14, well below the plan’s ~1.5 % acceptance threshold on #13.

---

## 1. Summary

The full category split described in the plan was implemented and tested:

- `LegalMoveKind` and `Board::classify_legal_move`.
- `Board::legal_with_kind` plus the five category helpers:
  `legal_castling`, `legal_en_passant`, `legal_capture`, `legal_commoner_move`, `legal_quiet`.
- Shared `legal_commoner_survival` tail.
- A thin public `Board::legal` wrapper that delegates to the same helpers.

Incremental measurements were taken for the pure split, quiet fast path, commoner-move fast path,
castling fast path, and movegen category dispatch. The commoner and castling fast paths and the
movegen dispatch were rejected because they regressed one or both fixed FENs. The remaining
pure split plus quiet fast path was validated (41/41 perft, golden move order, all tests), but its
fixed-FEN gain was too small and inconsistent to justify the added code. The source was restored
to the kept Plan 3 tree.

No source change remains in the working tree. The only new file from this plan is this report;
`plan4.md` was already untracked before implementation.

---

## 2. Baseline and Profile

### 2.1 Fresh fixed-FEN baseline

Measured after `cargo build --release --examples`, using wall-clock time around
`target/release/examples/perft FEN 6`.

FEN #13:
`rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5`

FEN #14:
`r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15`

| Run | #13 (ms) | #14 (ms) |
|-----|---------:|---------:|
| 1 | 11316 | 1439 |
| 2 | 11328 | 1444 |
| 3 | 11388 | 1440 |
| 4 | 11347 | 1445 |
| 5 | 11363 | 1438 |
| 6 | 11354 | 1436 |
| 7 | 11302 | 1440 |
| **Median** | **11347** | **1440** |

Node counts were unchanged throughout the whole plan:

- #13: `2,160,817,389`
- #14: `245,931,633`

### 2.2 Fresh profile

A `profiling` build with frame pointers was recorded with `perf` for both FENs. The profile showed
legality-related work still prominent, but `Board::legal` no longer appeared as a standalone symbol
because LTO inlined it into `perft`. The visible hot inlined entries on #13 included:

- `is_move_trivially_legal` — 13.01 %
- pawn generation — 11.35 %
- `MoveList::push` and piece-generation paths — roughly 9.7 % combined

On #14 the corresponding `is_move_trivially_legal` entry was 10.77 %. This was treated as
confirmation that the legality/compaction path was still the right target, not as evidence that the
plan should stop.

---

## 3. Implemented Category Split

### 3.1 Classifier

A crate-private classifier was added and used only after `is_move_trivially_legal` failed:

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegalMoveKind {
    Castling,
    EnPassant,
    Capture,
    CommonerMove,
    Quiet,
}
```

`classify_legal_move` returned `None` for an empty origin square and otherwise classified castling,
en passant, captures, non-capturing commoner moves, and all remaining quiet moves. Promotions were
deliberately not a category: a quiet promotion behaved as `Quiet`, a capturing promotion as
`Capture`.

### 3.2 Public wrapper and dispatch

`Board::legal(m, state)` remained public with the same signature and became:

```rust
pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
    match self.classify_legal_move(m) {
        Some(kind) => self.legal_with_kind(m, state, kind),
        None => false,
    }
}
```

`legal_with_kind` dispatched to the five helpers. The helpers preserved the original blast logic,
en-passant capture direction, castling pass-through loop, adjacent-commoner immunity, and the
pre-move use of `state.commoners_count` / `state.them_commoners_count`.

### 3.3 Shared survival tail

The old tail of `Board::legal` was extracted into `legal_commoner_survival`. It:

- used pre-move `state.commoners_count` and `state.them_commoners_count`;
- preserved the `enemy_pr_destroyed` shortcut;
- kept the adjacent-enemy-commoner immunity;
- continued to use Plan 3’s `square_attacked_by` for enemy non-commoner attacks.

### 3.4 Generation and ordering

`generate_pseudo_legal`, pawn generation, piece generation, castling generation, move ordering, and
`is_move_trivially_legal` were not changed. `tests/move_order_golden.rs` stayed green. The only
movegen experiment was the planned fallback dispatch in `generate_legal_with_state`, described
below.

---

## 4. Incremental Measurements and Decisions

All fixed-FEN timings below are milliseconds from seven-run loops unless noted otherwise.

### 4.1 Pure split (no fast paths)

| Run | #13 | #14 |
|-----|----:|----:|
| 1 | 11260 | 1434 |
| 2 | 11428 | 1440 |
| 3 | 11311 | 1442 |
| 4 | 11281 | 1433 |
| 5 | 11196 | 1425 |
| 6 | 11293 | 1442 |
| 7 | 11297 | 1439 |
| **Median** | **11293** | **1439** |

Versus the fresh baseline this was about **−0.48 % on #13** and **−0.07 % on #14**. Correctness
checks passed at this stage, including `verify_perft` 41/41 in 73.852 s.

### 4.2 Quiet fast path — accepted for further testing

The quiet helper returned `false` for `commoners_count == 0` and `true` for `commoners_count > 1`.

| Run | #13 | #14 |
|-----|----:|----:|
| 1 | 11239 | 1460 |
| 2 | 11242 | 1439 |
| 3 | 11275 | 1435 |
| 4 | 11291 | 1440 |
| 5 | 11282 | 1439 |
| 6 | 11276 | 1437 |
| 7 | 11305 | 1462 |
| **Median** | **11276** | **1439** |

Relative to the pure split: roughly **−0.15 % on #13** and neutral on #14. This substage did not
show a stable regression, so it was kept for the subsequent combined comparison.

### 4.3 Commoner-move fast path — rejected

| Run | #13 | #14 |
|-----|----:|----:|
| 1 | 11351 | 1457 |
| 2 | 11280 | 1430 |
| 3 | 11369 | 1445 |
| 4 | 11336 | 1438 |
| 5 | 11272 | 1443 |
| 6 | 11612 | 1459 |
| 7 | 11365 | 1450 |
| **Median** | **11351** | **1445** |

A second #13-only round gave a median of **11374 ms**. Both rounds regressed relative to the quiet
checkpoint (about 0.7–0.9 % on #13 and about 0.4 % on #14). It was also inactive on both fixed
FENs, which each have exactly one commoner per side. The commoner fast path was removed.

### 4.4 Castling fast path — rejected

The pass-through loop remained first, followed by `commoners_count > 1` returning true.

| Run | #13 | #14 |
|-----|----:|----:|
| 1 | 11512 | 1481 |
| 2 | 11470 | 1471 |
| 3 | 12049 | 1433 |
| 4 | 11359 | 1445 |
| 5 | 11316 | 1468 |
| 6 | 11320 | 1454 |
| 7 | 11389 | 1505 |
| **Median** | **11389** | **1468** |

#14 regressed by roughly 1 %, #13 was neutral to slightly worse, and the fast path was likewise
inactive on both fixed FENs. It was removed.

### 4.5 Movegen category dispatch — rejected

The compaction loop was changed to classify only after `is_move_trivially_legal` failed, then call
`legal_with_kind`. Pseudo-legal order and compaction order were unchanged.

| Run | #13 | #14 |
|-----|----:|----:|
| 1 | 11632 | 1473 |
| 2 | 11596 | 1490 |
| 3 | 11519 | 1469 |
| 4 | 11526 | 1476 |
| 5 | 11539 | 1470 |
| 6 | 11570 | 1467 |
| 7 | 11582 | 1474 |
| **Median** | **11570** | **1473** |

This was clearly worse than the pure-split-plus-quiet tree and was removed. The likely cause is
code-size/layout growth from inlining the classifier and helper dispatch into the hot compaction
loop.

---

## 5. Same-Session A/B Verification

Because absolute timings drifted between builds, three release binaries were built once and then
run round-robin to reduce drift:

- `baseline` — kept Plan 3 tree.
- `quiet` — category split plus quiet fast path only.
- `dispatch` — quiet tree plus movegen category dispatch.

### 5.1 #13 three-way A/B

| Run | Baseline | Quiet | Dispatch |
|-----|---------:|------:|---------:|
| 1 | 11533 | 11438 | 11694 |
| 2 | 11528 | 11429 | 11672 |
| 3 | 11508 | 11454 | 11563 |
| 4 | 11453 | 11448 | 11621 |
| 5 | 11485 | 11410 | 11958 |
| 6 | 11469 | 11470 | 11657 |
| 7 | 11472 | 11449 | 11877 |
| **Median** | **11485** | **11448** | **11672** |

- Quiet versus baseline: **−0.32 %**
- Dispatch versus baseline: **+1.63 %**
- Dispatch versus quiet: **+1.96 %**

### 5.2 #14 three-way A/B

| Run | Baseline | Quiet | Dispatch |
|-----|---------:|------:|---------:|
| 1 | 1523 | 1544 | 1534 |
| 2 | 1541 | 1526 | 1552 |
| 3 | 1525 | 1549 | 1555 |
| 4 | 1549 | 1508 | 1543 |
| 5 | 1516 | 1515 | 1547 |
| 6 | 1549 | 1502 | 1522 |
| 7 | 1547 | 1517 | 1542 |
| **Median** | **1541** | **1517** | **1543** |

- Quiet versus baseline: **−1.56 %**
- Dispatch versus baseline: **+0.13 %**
- Dispatch versus quiet: **+1.71 %**

### 5.3 Baseline versus quiet recheck

A second nine-run round compared only baseline and quiet.

**#13 medians:** baseline **11390 ms**, quiet **11510 ms**. The medians were distorted by
host-drift outliers; pairwise, quiet was faster in 8 of 9 runs, with a median pairwise gain of
**63 ms (~0.55 %)** and a mean gain of **~51 ms (~0.45 %)**.

**#14 medians:** baseline **1440 ms**, quiet **1437 ms**. The median pairwise gain was **2 ms
(~0.14 %)**.

The recheck therefore supports only a small, noisy improvement for the quiet variant, not the
plan’s expected 2–4 %.

---

## 6. Correctness Verification

The category-split candidate passed:

- `cargo fmt` — clean.
- `cargo clippy --all-targets` — no new warnings; only the four pre-existing
  `manual_range_contains` / `manual_contains` warnings in `examples/test_d5g8.rs`.
- `cargo test` — all unit, integration, golden-order, and doc tests passed:
  - 54 unit tests
  - `move_order_golden`: 1 passed
  - `perft_tests`: 4 passed
  - `verify_moves`: 1 passed
  - doc tests: 1 passed
- `cargo doc --no-deps` — passed.
- `cargo run --release --example verify_perft` — **41/41 passed** at depths 1–6 in 74.237 s.
- `tests/move_order_golden.rs` — green, confirming generation and public move ordering were
  unchanged.

After reverting the source, the restored Plan 3 tree was rebuilt and rerun:

- `verify_perft` — **41/41 passed** in 74.910 s.
- Restored fixed-FEN medians: #13 **11358 ms**, #14 **1439 ms**, consistent with the original
  same-session baseline (#13 11347 ms, #14 1440 ms).

---

## 7. Decision

**Reverted.**

The best candidate (pure category split plus quiet fast path) was correct and did not show a
stable regression, but its measured gain was only about 0.3–0.6 % on #13 and roughly 0–1.6 % on
#14 across rounds. The plan’s acceptance threshold was at least ~1.5 % on #13, and the analysis
expected 2–4 % overall. The commoner fast path, castling fast path, and movegen dispatch all
regressed at least one fixed FEN and were removed before the decision.

Because the remaining result was effectively neutral on the profiled workload and would add roughly
100–140 lines plus a new dispatch layer, the plan’s instruction to revert on a neutral/insufficient
combined result was followed.

Current state:

- `src/board.rs` and `src/movegen.rs` are restored to the kept Plan 3 tree (`git diff` is empty).
- Only `docs/plans/perf_analysis7/plan4.md` (pre-existing untracked) and this report are new.

---

## 8. Expectation and Next Steps

The plan estimated ~2–4 % from analysis §5.3. That did **not** reproduce on this host. The pure
split alone was under 0.5 % on #13, and the fast paths that were expected to help had no effect on
the profiled positions because both have only one commoner per side.

The profile still points at move generation, trivial-legality handling, and `MoveList` growth as
major costs. The next candidates from analysis §8 remain:

- **§5.5:** bulk pawn generation, but only after an explicit move-order policy decision.
- **§5.6:** `StateInfo` commoner-bitboard cache.
- **§5.7 / §5.8:** pinned-state and occupancy-cache work, if a fresh profile still shows them.

Given the layout sensitivity observed here and in Plans 1–2, a fresh profile and same-session
fixed-FEN baseline should precede any next item. This plan’s negative result specifically argues
against retrying category dispatch without a new profile showing that the extra classification can
pay for itself.

No smaller fast path from this plan is recommended as a standalone retry: the commoner and castling
fast paths were inactive on the fixed FENs, and the quiet fast path only produced a sub-threshold
gain.
