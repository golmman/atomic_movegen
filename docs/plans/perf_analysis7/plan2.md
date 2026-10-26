# Plan 2 — Local Bitboard Cache in `Board::legal`

**Corresponds to:** Item 2 of `docs/plans/perf_analysis7/analysis.md` — §5.2 / §4.2,
*Cache base bitboards in `Board::legal`*
**Estimated speedup:** ~1–2 % on both profiled positions (measured in analysis §4)
**Risk:** Very low — pure local hoisting, no semantic change
**Effort:** Small — approximately 10 changed lines in `src/board.rs`

---

## 1. Context

Plan 1 (§5.1, no-compaction `perft`) was implemented, verified, and **reverted** because it
regressed test #14 and did not improve test #13 on this host (`report1.md`). That result is
host-specific and does not invalidate the analysis, but it means the implementation order in
analysis §8 now starts here, with item 2.

This plan therefore only implements the **local bitboard cache** in `Board::legal`. It is
independent of the rejected `perft` change: `perft` continues to call `generate_legal`, which
calls `Board::legal`, so this optimization affects every path that uses the library.

Do not stack any other change onto this plan. Plan 1 already showed that code-layout effects on
this target can turn an expected gain into a regression, so each candidate must be attributable.

---

## 2. Problem

`Board::legal` (`src/board.rs:1310`) recomputes the same base bitboards repeatedly within a
single call:

- `self.occupied()` is computed in the castling block and again for the generic path, and a
  third time for `pre_non_pawns` on captures.
- `self.commoners(us)` is computed up to twice (`our_commoners` and, in the one-commoner path,
  again inside the pseudo-royal loop setup).
- `self.commoners(them)` is computed up to twice (adjacent-commoner check and the `enemy_pr_destroyed`
  calculation).

`commoners(c)` and `occupied()` are cheap individually (`by_color & by_type`, and ORs over the
piece arrays respectively — see analysis §3.3), but they appear at roughly 3–4 % of samples each
and are recomputed several times per `legal` call. Hoisting them to the top of the function is
semantically identical because `legal` takes `&self` and never mutates the board.

Analysis §4.2 measured this exact cache at **−2.1 % on #13** and **−1.1 % on #14**.

### 2.1 This is not the failed `pseudoRoyals` experiment

`docs/plans/perf_analysis6/report2.md` bundled a `StateInfo` commoner-bitboard cache with a
capture blast pre-filter and regressed by 5.17 %. This plan is deliberately different:

- It adds **no** new `StateInfo` fields.
- It adds **no** capture pre-filter and no extra blast computation.
- The cached values are local to one `legal` call and cannot go stale.
- It does not touch `populate_state` or `do_move`/`undo_move`.

If this local cache alone does not beat the current code on fixed-FEN A/B, stop and revert; do not
“rescue” it by adding the `StateInfo` cache (§5.6) in the same change.

---

## 3. Goal

Compute `occupied`, `commoners(us)`, and `commoners(them)` once at the top of `Board::legal` and
reuse those locals everywhere the function currently recomputes them, with no other behavior
change.

Specifically:

- Preserve `Board::legal`'s public signature, semantics, and return values exactly.
- Preserve call ordering of `attackers_to`, `king_attacks`, `pop_lsb`, and all early returns.
- Do not change any other function.
- Keep the change local to the body of `legal`.

---

## 4. Design

### 4.1 Hoisted locals

At the start of the body, after `us`/`them` are established, add:

```rust
let base_occupied = self.occupied();
let our_commoners_base = self.commoners(us);
let them_commoners_base = self.commoners(them);
```

Use them as follows:

| Current expression | Replace with |
|--------------------|--------------|
| `let occupied = self.occupied();` (castling block) | `let occupied = base_occupied;` |
| `let mut occupied = self.occupied() ^ Bitboard::square_bb(from);` | `let mut occupied = base_occupied ^ Bitboard::square_bb(from);` |
| `let pre_non_pawns = self.occupied() ^ pre_pawns;` | `let pre_non_pawns = base_occupied ^ pre_pawns;` |
| `let mut our_commoners = self.commoners(us) & occupied;` | `let mut our_commoners = our_commoners_base & occupied;` |
| `self.commoners(them) & attacks::king_attacks(sq)` (castling pass-through) | `them_commoners_base & attacks::king_attacks(sq)` |
| `let enemy_pr_destroyed = them_pr_count <= 1 && (self.commoners(them) & occupied).is_empty();` | `let enemy_pr_destroyed = them_pr_count <= 1 && (them_commoners_base & occupied).is_empty();` |
| `let them_commoners = self.commoners(them);` | `let them_commoners = them_commoners_base;` |

Notes:

- `base_occupied` is the **pre-move** occupancy. The castling block and the generic path both
  start from pre-move occupancy, so a single value is correct for both. The later mutation into
  a local `occupied` variable stays exactly as it is.
- `our_commoners_base` is masked with the post-blast `occupied` for the survival test, so the
  early return when `our_commoners.is_empty()` keeps its current meaning.
- The `!is_capture && piece == Commoner` branch still ORs the destination square into
  `our_commoners`; only the base source changes.
- `them_commoners_base` is the pre-move enemy commoner set and is used only after being masked
  with an occupancy where appropriate. This matches the current code.

### 4.2 What not to change

- Do **not** reorder the castling pass-through check relative to the generic path.
- Do **not** merge `our_pr_count`/`them_pr_count` reads or remove the `state` fields.
- Do **not** introduce a helper function, trait, or new struct. A plain set of locals is the
  measured shape.
- Do **not** add `occupied` as a `Board` field (§5.8); that is a different, higher-risk change.
- Do **not** revisit Plan 1's `perft` loop.

### 4.3 Expected interaction with `generate_legal`

No API changes are needed. `generate_legal_with_state` still calls `is_move_trivially_legal`
first and only falls back to `board.legal`, so the cache benefits the non-trivial path
(captures, commoner moves, castling, en-passant, pinned/check positions) — which is exactly the
path that dominates test #14 and a large share of #13.

---

## 5. Implementation Steps

### Step 1: Establish a fresh baseline on this host

The timings in analysis §1 (8.457 s / 1.053 s) were recorded on a different loaded session than
report1's baseline (11.529 s / 1.466 s). Re-measure on the current machine and use only
same-session comparisons.

```sh
cargo build --release --examples

# Fixed-FEN depth 6, 7 runs each; compute the median.
F13='rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5'
F14='r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15'
for i in $(seq 1 7); do
  target/release/examples/perft "$F13" 6
done
for i in $(seq 1 7); do
  target/release/examples/perft "$F14" 6
done
```

`perft` already prints its own elapsed time, so wrapping each invocation with `date` is not
required; capture the printed timing or use the shell `time` builtin if the environment lacks
`/usr/bin/time`. Record all seven runs, not just the median, so noise is visible.

Also record the full-suite baseline once for context:

```sh
cargo run --release --example verify_perft
```

The full suite is noisy (analysis §1); it is supporting evidence only.

### Step 2: Apply the cache

Edit only the body of `Board::legal` in `src/board.rs` per §4.1. Keep the diff minimal: add three
locals and substitute the table above. Do not reformat unrelated code.

### Step 3: Correctness checks

```sh
cargo fmt
cargo clippy
cargo test
cargo doc
cargo run --release --example verify_perft
```

All existing tests must stay green, especially:

- `tests/move_order_golden.rs` — proves move order is untouched.
- `tests/perft_tests.rs` — exercises `legal` across all positions and depths.
- `tests/verify_moves.rs` — moves against `tests/moves.md`.

`verify_perft` must report **41/41** at depths 1–6. A single mismatch means the hoisted bitboards
are not equivalent to the recomputed ones; stop and diff the substitutions.

### Step 4: Performance verification

Rebuild (`cargo build --release --examples`) and repeat the Step 1 fixed-FEN runs in the same
session.

Acceptance criteria:

- Test #13 median improves by at least ~1 %.
- Test #14 median improves by at least ~0.5 %, or is within noise but not a stable regression.
- Combined, the change must not regress either fixed FEN in repeated runs.
- Full-suite result is supporting evidence only; do not accept or reject solely on it.

If the result is neutral or a stable regression:

1. Revert `src/board.rs` to the original content.
2. Rebuild and re-run the same A/B to confirm the baseline is restored.
3. Record the negative result in `report2.md` and stop; do not proceed to §5.4 automatically
   without deciding whether host-specific layout effects make further micro-optimizations
   worthwhile.

### Step 5: Optional profile confirmation

If the win is retained, optionally verify that repeated `commoners`/`occupied` work dropped out
of `Board::legal`:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
perf record -F 997 -g --call-graph dwarf -o /tmp/perf7-plan2.data \
  target/profiling/examples/perft "$F13" 6
perf report -i /tmp/perf7-plan2.data --stdio --no-children --sort=srcline \
  | grep -A20 'board.rs'
```

This is confirmatory only; `Board::legal` may be small enough that the change is below the
sampling resolution.

### Step 6: Write the implementation report

Create `docs/plans/perf_analysis7/report2.md` as required by `AGENTS.md`. Include:

- Summary of the exact substitutions made and confirmation that only `Board::legal` changed.
- Baseline and post-change fixed-FEN #13/#14 timings (all runs plus medians) and the full-suite
  result.
- Whether the measured ~1–2 % from analysis §4.2 reproduced, and if not, by how much it differed.
- Correctness: `cargo fmt`, `cargo clippy`, `cargo test`, `cargo doc`, and `verify_perft` 41/41.
- Explicit statement of whether the change was kept or reverted, with the decision reason.
- Note that this plan did **not** add a `StateInfo` cache or any capture pre-filter.
- If kept: whether the profile shows the expected reduction.
- If reverted: the negative result, and a recommendation for whether to continue with §5.4 on
  this host or to re-baseline the whole analysis first.

---

## 6. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/board.rs` | Hoist `occupied`, own-commoner, and enemy-commoner bitboards in `Board::legal` and reuse them. | ~10 |
| `docs/plans/perf_analysis7/report2.md` | New implementation report. | New file |

No changes expected in `src/lib.rs`, `src/movegen.rs`, tests, or examples.

---

## 7. Correctness Contract

For any board, move, and `StateInfo`, the patched `Board::legal` must return exactly the same
boolean as before, because:

- `self.occupied()` and `self.commoners(c)` are pure functions of `&self`.
- `legal` does not mutate `self`, so the values cannot change during the call.
- The code only substitutes identical expressions; no branch, comparison, or early return is
  reordered.

The 41-position `verify_perft` run plus `move_order_golden` and `verify_moves` are the strongest
verification.

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| A substitution uses pre-move occupancy where post-move was intended | Low | Only replace the *sources* of `base_occupied`; leave the local `occupied` mutation sequence untouched. |
| Commoner base is masked incorrectly | Low | Keep every existing `& occupied` mask after substitution; review the diff line by line. |
| Code layout cancels the expected gain | Medium | Fixed-FEN A/B with all runs recorded; revert if not a stable win. |
| Host differs from the analysis host | High | Re-baseline in the same session; do not compare against analysis §1 times. |
| Temptation to bundle the `StateInfo` cache or pre-filter | Medium | Explicitly out of scope (analysis §5.6); a separate plan if ever retried. |
| Full-suite noise hides a regression | High | Base the decision on repeated fixed-FEN medians, not the full suite. |

---

## 9. Out of Scope / Follow-ups

- **Plan 3 candidate:** re-run Plan 1's no-compaction `perft` on a freshly re-baselined host only
  if new profiling evidence justifies it; otherwise do not retry it (`report1.md`).
- **§5.4:** boolean `square_attacked_by` early exit. Only after this plan is accepted and
  re-baselined both FENs.
- **§5.3:** category-specialized legality, with before/after measurements on both FENs and the
  full suite.
- **§5.6:** retry the `StateInfo` commoner cache **alone**, after this local cache is settled.
- **§5.5:** bulk pawn generation only after a move-order policy decision.
- Do not retry the rejected experiments in analysis §6 as part of this plan.
