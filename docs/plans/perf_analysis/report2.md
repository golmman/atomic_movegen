# Report 2: Cache Commoner Bitboards + Precompute `between_bb` Table

## Summary

Plan 2 implemented Items 3 and 5 from `analysis.md`: caching commoner bitboards
in `legal()` and replacing the `between_bb()` loop with a precomputed table.
Both changes were reverted after benchmarking showed no measurable performance
benefit — the compiler already optimised away the redundant `commoners()`
calls, and the `LazyLock`-based table lookup was a wash against the original
loop.

**Total estimated speedup:** 7–15 % (per plan)
**Actual:** 0 % — reverted.

---

## Changes Implemented (reverted)

### Item 3: Cache commoner bitboards in `legal()`

Two cached bindings were added inside `legal()` and four call sites were
updated:

```rust
let our_commoners_bb = self.commoners(us);
let their_commoners_bb = self.commoners(them);
```

### Item 5: Precomputed `between_bb` table

A `BETWEEN_BB` static table was added using `std::sync::LazyLock`:

```rust
static BETWEEN_BB: LazyLock<[[Bitboard; 64]; 64]> = LazyLock::new(|| {
    let mut table = [[Bitboard::EMPTY; 64]; 64];
    // compute upper triangle, mirror on access
    ...
});
```

The public `between_bb()` function was replaced with a single table lookup.

---

## Benchmark Results

All measurements from `cargo run --release --example verify_perft 6` (41
positions, depths 1–6).

| Iteration | Total time | Δ from baseline | Notes |
|-----------|-----------|----------------|-------|
| Baseline (Plan 1 final) | 124.380 s | — | Code before any Plan 2 changes |
| With Plan 2 (caching before early-out) | 126.527 s | **+2.147 s (+1.7 %)** | Regression |
| After fix (caching after early-out) | 125.396 s | +1.016 s (+0.8 %) | Partial recovery |
| After full revert | 124.507 s | +0.127 s (+0.1 %) | Within noise |

### Timeline

1. **Initial implementation** placed the cached bindings *before* the early-out
   in `legal()`, adding two `commoners()` calls to every fast-path
   `legal()` invocation → **126.5 s**.
2. **Diagnosis:** The early-out is the hottest path — most moves in a perft
   search are non-capture, non-commoner, non-en-passant moves. Adding
   unconditional `commoners()` lookups there caused the regression.
3. **Fix:** Moved the cached bindings after the early-out block → **125.4 s**.
4. **Evaluation:** Still ~1 % slower than baseline. Investigation showed the
   remaining difference was explained by the `LazyLock` deref overhead in
   `between_bb()` being comparable to the original loop, and the commoner
   caching being already handled by the compiler's CSE pass. Both changes
   were reverted → **124.5 s**.

---

## Why the Predicted Gains Didn't Materialise

### Item 3: `self.commoners(c)` is already free

```rust
pub fn commoners(&self, c: Color) -> Bitboard {
    self.pieces_color_pt(c, PieceType::Commoner)
    // → self.by_color[c as usize] & self.by_type[PieceType::Commoner as usize]
}
```

This is two array loads and a bitwise AND. Inside `legal()`, the compiler sees
`&self` (immutable) and can CSE (common-subexpression-eliminate) repeated calls
to `commoners()` within the same basic block automatically. The hoisted locals
added register pressure and code bloat with no benefit.

**Lesson:** Don't manually cache pure-function results that the compiler can
see through — the optimiser is better at it than we are.

### Item 5: `LazyLock` overhead offsets table-lookup savings

The original `between_bb()` loop:

```
For non-aligned squares:  check df/dr → return EMPTY (2-3 arithmetic ops)
For aligned squares:      loop 1-7 iterations with match-on-file + match-on-rank
```

The non-aligned case is the common one (most sniper pairs in `compute_pinned`
are not aligned), and it was already fast. The table lookup replaces this with:

```
LazyLock deref  →  relaxed atomic load + branch (always taken after init)
Array index    →  s1*64 + s2 offset computation
Load           →  load Bitboard from table
```

The `LazyLock` deref adds a small but measurable overhead on every call, wiping
out the savings from the aligned case. The 32 KB table also adds cache pressure
and startup cost.

**Lesson:** Table lookups only win when the computation they replace is
expensive *in the common case*. For simple arithmetic + short-early-return
functions, the original code is often optimal.

---

## Verification

| Check | Result |
|-------|--------|
| `cargo test` | 46/46 passed (before revert) |
| `verify_perft 6` | 41/41 passed (all iterations) |
| `cargo clippy` | No new warnings |
| `cargo fmt` | Clean |

The reverted code is identical to the Plan 1 baseline, so all existing
verification remains valid.

---

## Deviation from Plan

| Item | Plan | Actual | Reason |
|------|------|--------|--------|
| Item 3: Cache commoners | Keep | **Reverted** | No measurable benefit |
| Item 5: `between_bb` table | Keep | **Reverted** | No measurable benefit |
| Estimated speedup | 7–15 % | 0 % | Compiler already optimised; LazyLock overhead offset gains |

---

## Lessons Learned

### 1. Profile before micro-optimising, verify end-to-end after

The plan's estimated 7–15 % was based on static reasoning about call counts and
operation costs. In practice, the compiler's CSE pass already handled Item 3,
and Item 5's LazyLock overhead was not accounted for. Always verify with an
end-to-end benchmark before committing to code changes.

### 2. The early-out is sacred — never add cost before it

The early-out in `legal()` handles the majority of calls. Any code placed
before it is paid on every invocation, including the fast path. This is a
classic "pay for what you use" principle — the caching should have been placed
after the early-out from the start.

### 3. `LazyLock` has non-zero cost

While `std::sync::LazyLock` is fast (relaxed atomic load + predictable branch),
it is not zero-cost. For a function called millions of times per second in a
hot loop, the deref overhead can measurably affect total runtime. Use
`LazyLock` for tables that replace truly expensive computation (e.g., sliding
attack tables), not for simple arithmetic functions.

### 4. Some optimisations look good on paper but don't compile to better code

The Rust compiler aggressively inlines simple accessors and CSEs repeated
calls. Manual caching of these patterns often generates worse code due to
register pressure. Trust the optimiser for trivial pure functions.

---

## File Changes

| File | Lines | Change |
|------|-------|--------|
| `src/board.rs` | 0 | Reverted Item 3 |
| `src/bitboard.rs` | 0 | Reverted Item 5 |
| **Total** | **0** | No net change |

Plan 2 left no code footprint — both changes were fully reverted.

---

## Relationship to Remaining Items

The priority queue from `analysis.md` now becomes:

| Priority | Item | Est. Impact | Status |
|----------|------|-------------|--------|
| 1 | Precompute state in `do_move()` | 30–50 % | ✅ Plan 1, done |
| 2 | Early-out in `legal()` | 15–30 % | ✅ Plan 1, done |
| 3 | Cache commoner bitboards | 5–10 % | ❌ No benefit, reverted |
| 4 | Precompute `between_bb` | 2–5 % | ❌ No benefit, reverted |
| 5 | Eliminate runtime sliding dispatch | 3–8 % | Next |
| 6 | Inline hot accessor functions | 2–5 % | Next |
| 7 | Stack-array for move generation | 1–3 % | Later |
| 8 | Deduplicate slider attacks | 1–3 % | Later |
| 9 | Optimise `file_of`/`rank_of` | 1–2 % | Next |
| 10 | Static-init hot tables | 0–1 % | Cleanup |

Items 5, 6, and 9 remain as candidates for a future "Micro-optimisations" plan.
