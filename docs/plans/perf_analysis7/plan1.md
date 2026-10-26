# Plan 1 — Perft Fast Path Without Legal-List Compaction

**Corresponds to:** Item 1 of `docs/plans/perf_analysis7/analysis.md` — §5.1 / §4.1, *Add a perft fast path that avoids legal-list compaction*
**Estimated speedup:** ~3 % on check-heavy test #13; neutral on test #14; ~2–4 % on the full `verify_perft` suite
**Risk:** Low — perft-only, public move generation and move order unchanged
**Effort:** Small — primarily `src/lib.rs`, approximately 20–30 changed lines

---

## 1. Problem

`perft` currently calls `movegen::generate_legal` at every node:

```rust
let mut moves = MoveList::new();
movegen::generate_legal(board, &mut moves);

if depth == 1 {
    return moves.len() as u64;
}
```

`generate_legal` performs three jobs:

1. Generate a pseudo-legal `MoveList`.
2. Rewrite that list in place to keep only legal moves.
3. Return the compacted list to the caller.

For perft, step 2 is unnecessary. The recursion only needs to visit each legal move; it does not
need a compact legal list. The in-place compaction loop in `generate_legal_with_state`
(`src/movegen.rs:288–310`) accounts for a large share of sampled time: roughly 20 % of source-line
samples on test #13 and 14 % on test #14. Skipping the writes and list-length management in the
perft path is a measured **~3 % win on #13** and neutral on #14 (see analysis §4).

This optimization must stay inside `perft`. Public `generate_legal` behavior and move ordering are
pinned by `tests/move_order_golden.rs` and must not change.

---

## 2. Goal

Make `perft` walk the pseudo-legal list directly and recurse on moves accepted by the existing
legality predicate, without building a compact legal list first.

Specifically:

- Do not call `generate_legal` from `perft`.
- Call `generate_pseudo_legal` once per node.
- Reuse the same pre-filter already used by `generate_legal_with_state`:
  `is_move_trivially_legal(board, m, &state) || board.legal(m, &state)`.
- At `depth == 1`, count accepted moves with that predicate instead of compacting a list.
- Preserve `perft`'s public signature, doc comment, `#[must_use]`, and exact node counts.
- Leave `generate_legal`, `generate_legal_with_state`, `has_legal_move`, and all public move
  ordering untouched.

---

## 3. Design

### 3.1 New `perft` flow

Replace the body of `src/lib.rs::perft` with the measured no-compaction shape:

```rust
pub fn perft(board: &mut board::Board, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut moves = MoveList::new();
    movegen::generate_pseudo_legal(board, &mut moves);

    let mut state = board::StateInfo::new();
    board.populate_state(&mut state);

    // At the leaf level, count legal moves without writing a compacted list.
    if depth == 1 {
        return moves
            .as_slice()
            .iter()
            .filter(|&&m| {
                board::is_move_trivially_legal(board, m, &state) || board.legal(m, &state)
            })
            .count() as u64;
    }

    let mut total = 0u64;
    let mut child_state = board::StateInfo::new();
    for &m in moves.as_slice() {
        if board::is_move_trivially_legal(board, m, &state) || board.legal(m, &state) {
            board.do_move(m, &mut child_state);
            total += perft(board, depth - 1);
            board.undo_move(m, &child_state);
        }
    }
    total
}
```

Notes:

- `board::is_move_trivially_legal` is already `pub(crate)` in `src/board.rs`, so no visibility
  change is required.
- `StateInfo` must be populated before either predicate is used. `generate_legal` currently does
  this internally; the new `perft` path must do it explicitly.
- `child_state` is still allocated only for `depth >= 2` and reused across siblings, matching the
  current allocation pattern.
- The recursive legal-move order is unchanged: pseudo-legal candidates are visited in the same
  order, and rejected candidates are simply skipped rather than removed. Therefore `do_move` /
  `undo_move` are called in the same sequence as before.

### 3.2 Keep the predicate duplicated rather than borrowing across mutation

Do **not** create a closure such as:

```rust
let is_legal = |m| board::is_move_trivially_legal(board, m, &state) || board.legal(m, &state);
```

and hold it across the recursion loop: it borrows `board` immutably while `board.do_move` needs a
mutable borrow. Either write the predicate at each use site as above, or add a small
`#[inline(always)]` private helper taking `&Board`, `Move`, and `&StateInfo`. The inline duplicate
matches the measured experiment and avoids any risk of changed borrow/codegen behavior.

### 3.3 What is deliberately not included

This plan does not include any of the following; they belong to later items or were explicitly
rejected by the analysis:

- No changes to public `generate_legal` or move order.
- No `StateInfo` allocation/reuse redesign.
- No `MaybeUninit` move list or batched `MoveList` counter.
- No Zobrist/hash changes.
- No pawn-generation rewrite.
- No local `occupied`/commoner cache in `Board::legal` (that is Plan 2 / §5.2).

Keeping the change isolated is important because the analysis measured this exact variant and
because prior attempts have shown layout sensitivity on this target.

---

## 4. Implementation Steps

### Step 1: Record the baseline

Build the release examples and record reference timings before editing:

```sh
cargo build --release --examples

# Full suite, 3 runs (noisy; record all of them)
for i in 1 2 3; do
  /usr/bin/time -f "suite run $i: %e s" \
    target/release/examples/verify_perft
done

# Fixed-FEN depth 6, 7 runs each, record median
F13='rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5'
F14='r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15'
for i in $(seq 1 7); do
  /usr/bin/time -f "F13 run $i: %e s" target/release/examples/perft "$F13" 6
done
for i in $(seq 1 7); do
  /usr/bin/time -f "F14 run $i: %e s" target/release/examples/perft "$F14" 6
done
```

For comparison, `docs/plans/perf_analysis7/analysis.md` reports medians of **8.457 s** for #13 and
**1.053 s** for #14. Re-record on the actual machine; do not compare across differently loaded
sessions.

### Step 2: Rewrite `perft`

In `src/lib.rs`, replace the body of `perft` as shown in §3.1.

Keep:

- The `if depth == 0 { return 1; }` base case.
- The existing public doc comment, or extend it slightly to note that perft uses a direct
  pseudo-legal walk without materializing a compact legal list.
- `#[must_use]`.

Make sure `board::is_move_trivially_legal` is reachable from the crate root. It already is through
`pub mod board;`; no new public API is needed.

### Step 3: Keep public legal generation untouched

Verify with a diff that `src/movegen.rs`, `src/board.rs`, and all public APIs are unchanged. In
particular, `generate_legal` must still populate its own `StateInfo` and compact in place.

### Step 4: Correctness checks

Run the focused and full test suites:

```sh
cargo fmt
cargo clippy
cargo test
```

The relevant existing coverage already exercises the changed path:

- `tests/perft_tests.rs::perft_all_positions_depth_1` exercises the new `depth == 1` counting path.
- `tests/perft_tests.rs::perft_starting_depth_2/3` and `perft_all_positions_depth_4` exercise the
  no-compaction recursion.
- `tests/move_order_golden.rs` must remain green, proving public move order is unchanged.

Then verify every oracle count:

```sh
cargo run --release --example verify_perft
```

All **41/41** positions must match at depths 1–6. If any mismatch appears, do not proceed to
performance measurement; the predicate order or state setup differs from `generate_legal`.

### Step 5: Performance verification

Rebuild release examples and repeat the Step 1 measurements on the patched tree.

Acceptance criteria:

- Test #13 median improves by at least ~2 % (analysis measured ~3 %).
- Test #14 median is within measurement noise (no more than ~1 % regression).
- Full-suite runs pass and show no material regression; full-suite totals remain noisy.
- If the fixed-FEN result contradicts the analysis (stable regression), revert the change and
  document the result rather than keeping a speculative optimization.

Optionally profile the patched binary with the existing `profiling` profile to confirm that the
compaction source lines (`movegen.rs:301–303`) disappear from the `perft` path:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" \
  cargo build --profile profiling --example perft
perf record -g target/profiling/examples/perft "$F13" 6
perf report
```

### Step 6: Write the implementation report

Create `docs/plans/perf_analysis7/report1.md` as required by `AGENTS.md`. Include:

- Summary of the change and that it is confined to `src/lib.rs::perft`.
- Baseline and post-change timings: full suite (all runs) and fixed-FEN #13/#14 medians.
- Speedup or regression percentages.
- Correctness results: `cargo test`, `cargo clippy`, `cargo fmt`, `cargo doc`, and
  `verify_perft` 41/41.
- Any surprises, including whether the measured gain matched ~3 % on #13.
- Confirmation that public move ordering and `generate_legal` are unchanged.
- Next-step notes: Plan 2 is the local bitboard cache in `Board::legal` (§5.2), followed by
  re-baselining both #13 and #14 before stacking further changes.

---

## 5. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/lib.rs` | Replace `perft` body with pseudo-legal walk plus direct legality filtering and depth-1 counting. | ~20–30 |
| `docs/plans/perf_analysis7/report1.md` | New implementation report. | New file |

No changes expected in `src/movegen.rs`, `src/board.rs`, tests, or examples.

---

## 6. Correctness Contract

The patched `perft` must satisfy, for any board and any depth:

- `perft(board, 0) == 1`.
- `perft(board, 1)` equals the number of moves accepted by
  `is_move_trivially_legal(...) || board.legal(...)` over the pseudo-legal list, which equals
  `generate_legal(board).len()`.
- `perft(board, d)` for `d >= 2` equals the previous implementation's count, because it visits
  exactly the legal moves in the same order and makes/unmakes each one identically.
- Board state is fully restored after each recursive call via the existing `undo_move`.
- No public API, move order, or `generate_legal` semantics change.

The strongest verification is the 41-position `verify_perft` run at depths 1–6.

---

## 7. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Missing `populate_state` causes incorrect trivial-legality or `legal` decisions | Low | Populate explicitly before any predicate; `verify_perft` catches mismatches immediately. |
| Depth-1 counting differs from compacted legal-list length | Low | Use the exact same `is_move_trivially_legal || legal` predicate as `generate_legal_with_state`; existing depth-1 tests plus full verification. |
| Public move ordering changes | Very low | No public generator is edited; `tests/move_order_golden.rs` remains authoritative. |
| Measured win disappears due to code layout | Medium | Accept only after fixed-FEN A/B; revert and report if it regresses. |
| Borrow-checker friction from trying to share a predicate closure | Low | Inline the predicate or use an `#[inline(always)]` helper that does not hold a borrow across `do_move`. |
| Full-suite timings are noisy and misleading | High | Base the accept/reject decision on repeated fixed-FEN #13/#14 medians; treat full-suite totals as supporting evidence only. |

---

## 8. Out of Scope / Follow-ups

- **Plan 2:** cache `occupied`, `our_commoners`, and `them_commoners` locally in `Board::legal`
  (analysis §5.2). Keep it separate from this plan so each measured win can be attributed.
- After Plans 1 and 2, re-baseline and re-profile both #13 and #14 before stacking more changes.
- Later candidates: boolean `square_attacked_by` early exit (§5.4), category-specialized legality
  (§5.3), bulk pawn generation only after a move-order policy decision (§5.5).
- Do not retry the rejected experiments listed in analysis §6 as part of this work.
