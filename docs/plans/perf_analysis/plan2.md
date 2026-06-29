# Plan 2: Cache Commoner Bitboards in `legal()` + Precompute `between_bb` Table

## Summary

Implement Items 3 and 5 from `analysis.md`:

| Item | Description | Est. Impact | Effort | Risk |
|------|-------------|-------------|--------|------|
| 3 | Cache commoner bitboards in `legal()` | 5–10 % | ~5 lines | None |
| 5 | Replace `between_bb()` loop with precomputed table | 2–5 % | ~15 lines | None |

**Total estimated speedup:** 7–15 % additive; may compound with Plan 1's 33.9 %.

These are independent of each other and can be implemented in either order. They
share no code conflicts.

---

## Item 3: Cache commoner bitboards in `legal()`

### Problem

`legal()` calls `self.commoners(us)` 1× and `self.commoners(them)` 3×. Each
call dereferences the Board's `by_color[c]` and `by_type[COMMONER]` arrays and
performs a bitwise AND:

```rust
// commoners(c) -> pieces_color_pt(c, Commoner) -> by_color[c] & by_type[Commoner]
```

The four call sites in the current code (board.rs):

| Line | Expression | Context |
|------|-----------|---------|
| 724 | `self.commoners(them)` | Castling pass-through check |
| 805 | `self.commoners(us)` | Self-explosion: `our_commoners` init |
| 844 | `self.commoners(them) & occupied` | Enemy-PR-destroyed check |
| 849 | `self.commoners(them)` | Pseudo-royal loop adjacency immunity |

### Fix

Load both bitboards once at the top of `legal()` and reuse:

```rust
let our_commoners_bb = self.commoners(us);
let their_commoners_bb = self.commoners(them);
```

Replace each call site:

| Line | Before | After |
|------|--------|-------|
| 724 | `self.commoners(them)` | `their_commoners_bb` |
| 805 | `self.commoners(us) & occupied` | `our_commoners_bb & occupied` |
| 844 | `self.commoners(them) & occupied` | `their_commoners_bb & occupied` |
| 849 | `self.commoners(them)` | `their_commoners_bb` |

### Safety

No change in behavior — both bitboards are read-only during `legal()`. The
board state does not mutate within `legal()`. Trivially correct.

### Edge cases

- Castling block (lines 711–757) uses `their_commoners_bb` for adjacency
  immunity. If the early-out fires (line 701–708) the castling block is
  skipped entirely, so no risk of out-of-date caching.
- The pseudo-royal loop uses `our_commoners_bb` (line 805) which is then
  mutated locally (`our_commoners = our_commoners | Bitboard::square_bb(kto)`)
  — this is fine because we assign to a `let mut` local, not to the cached
  binding.

### Effort

~5 lines changed in `src/board.rs`, 0 new imports.

---

## Item 5: Replace `between_bb()` loop with precomputed table

### Problem

`between_bb()` in `src/bitboard.rs:92–143` computes the set of squares between
two squares using a while-loop with `make_square()` + match-on-file + match-on-
rank per iteration:

```rust
pub fn between_bb(s1: Square, s2: Square) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    let f1 = s1 as i8 % 8;
    // ... delta computation ...
    let mut f = f1 + f_step;
    let mut r = r1 + r_step;
    while f != f2 || r != r2 {
        let sq = make_square(
            match f { 0 => File::A, 1 => File::B, ... },
            match r { 0 => Rank::R1, 1 => Rank::R2, ... },
        );
        b = b | square_bb(sq);
        f += f_step;
        r += r_step;
    }
    b
}
```

This is called from `compute_pinned()` (via `populate_state()` → every
`do_move()`) for every (commoner, sniper) pair. The profile shows
`compute_pinned` inside the ~35–40 % `legal()` hotspot (indirectly via
`populate_state`), and `between_bb`'s match-heavy loop contributes to that.

### Fix

Precompute a `BETWEEN_BB: [[Bitboard; 64]; 64]` lookup table at init time.
Replace the loop body with a single table read.

#### Step 1: Add the table

In `src/bitboard.rs`:

```rust
use std::sync::LazyLock;

static BETWEEN_BB: LazyLock<[[Bitboard; 64]; 64]> = LazyLock::new(|| {
    // Only compute upper triangle; mirror on access
    let mut table = [[Bitboard::EMPTY; 64]; 64];
    for s1 in 0..64u8 {
        for s2 in 0..64u8 {
            if s1 == s2 { continue; }
            let b = compute_between_bb(
                Square::from_index(s1 as i8),
                Square::from_index(s2 as i8),
            );
            table[s1 as usize][s2 as usize] = b;
            table[s2 as usize][s1 as usize] = b; // symmetric
        }
    }
    table
});

/// The old loop-based body, renamed for use during init only.
fn compute_between_bb(s1: Square, s2: Square) -> Bitboard { ... }
```

#### Step 2: Replace public function

```rust
pub fn between_bb(s1: Square, s2: Square) -> Bitboard {
    BETWEEN_BB[s1 as usize][s2 as usize]
}
```

#### Step 3: Ensure `LazyLock` is forced at startup

Add a call to `LazyLock::force(&BETWEEN_BB)` in the existing `init()` function
(if one exists) or at the start of `perft()` / `generate_legal()`. Since the
table is small (64×64×8 = 32 KiB — fits in L1 cache), the one-time init cost
(~4096 iterations of the old loop) is negligible.

Alternatively: if the table is cheap enough to compute, use a `static` with a
`const` initializer using a compile-time loop. However, `const` loops are not
stable in Rust 2024 without `adt_const_params`, so `LazyLock` is the pragmatic
choice.

### Safety

Pure table lookup — no unsafe, no mutation after init. The result is
bit-identical to the loop-based version.

### Performance

- **Before:** O(n) loop with 2 match statements per iteration (n = distance
  between squares, up to 7).
- **After:** Single array load (`ldr` on arm64, ~4 cycles L1 hit).
- **Worst-case init cost:** 64×64÷2 = 2048 unique pairs × ~7 iterations ×
  2 matches each ≈ 28k match evaluations, done once.

### Benchmarking note

The speedup will be most visible in positions with many commoners (more
`compute_pinned` calls) and deep search trees (where `populate_state` is called
frequently). The starting position (2 commoners per side) may show less benefit
than positions with 5–7 commoners.

### Effort

~15–20 lines added in `src/bitboard.rs`. No changes to callers (API unchanged).

---

## Implementation Order

```
Item 3 ──── src/board.rs  (~5 lines)
   ↓
Item 5 ──── src/bitboard.rs  (~15-20 lines)
```

Either can be done first. Item 3 is smaller and can serve as a warm-up. Item 5
is standalone but touches a different file so there is zero merge risk.

---

## Verification

### Method
1. `cargo test` — all unit tests must pass.
2. `cargo run --release --example verify_perft 6` — all 41 positions at
   depths 1–6 must match `perft_values.md`.
3. Timing comparison: run `verify_perft 6` before and after, report total
   wall-clock time.

### Performance target

| Item | Expected reduction | Measurement |
|------|-------------------|-------------|
| Item 3 only | 5–10 % | `verify_perft 6` total time |
| Item 5 only | 2–5 % | `verify_perft 6` total time |
| Combined | 7–15 % | `verify_perft 6` total time |

Measure three times and report the median as in Report 1.

---

## File Changes (estimated)

| File | Lines | Change |
|------|-------|--------|
| `src/board.rs` | ~+5 / ~−0 | Cache commoner bitboards in `legal()` |
| `src/bitboard.rs` | ~+20 / ~−50 | Add `BETWEEN_BB` table, replace `between_bb()` body |
| **Total** | **~+25 / ~−50** | Net −25 lines (shorter `between_bb`) |

---

## Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Item 3: bitboard not up-to-date in a code path | Very low | Both bitboards are read-only; all 4 call sites verified |
| Item 5: `LazyLock` init cost at startup | Trivial | 32 KiB table, ~2048 pairs, ~28k match evals — sub-millisecond |
| Item 5: `LazyLock` deref overhead per call | Low | After init, `LazyLock` is a single atomic load + branch (already paid by other `LazyLock` tables) |
| Item 3 + 5 combined regress | None | Orthogonal, no shared state |

---

## Relationship to Remaining Items

Once Plan 2 is complete, the priority queue from `analysis.md` becomes:

| Priority | Item | Est. Impact | Status |
|----------|------|-------------|--------|
| 1 | Precompute state in `do_move()` | 30–50 % | ✅ Plan 1 |
| 2 | Early-out in `legal()` | 15–30 % | ✅ Plan 1 |
| **3** | **Cache commoner bitboards** | **5–10 %** | **← Plan 2** |
| **4** | **Precompute `between_bb`** | **2–5 %** | **← Plan 2** |
| 5 | Eliminate runtime sliding dispatch | 3–8 % | Next |
| 6 | Inline hot accessor functions | 2–5 % | Next |
| 7 | Stack-array for move generation | 1–3 % | Later |
| 8 | Deduplicate slider attacks | 1–3 % | Later |
| 9 | Optimize `file_of`/`rank_of` | 1–2 % | Next |
| 10 | Static-init hot tables | 0–1 % | Cleanup |

Items 5–6 and 9 can be bundled into a "Micro-optimizations" plan (Plan 3).
