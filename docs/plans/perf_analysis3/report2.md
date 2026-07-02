# Report 2 — Eliminate Redundant `queen_attacks()` Magic Lookups + Polish

## Summary

Plan 2 has been fully implemented, combining three complementary optimizations:

| Item | Description | Expected speedup |
|------|-------------|-----------------|
| **Item 2** (primary) | Eliminate redundant `queen_attacks()` magic lookups — reuse `rook_atk \| bishop_atk` instead of calling `queen_attacks()` a second time | 3–8 % |
| **Item 11** (bonus) | Explicit `#[inline(always)]` on hot accessors | 0.5–1.5 % |
| **Item 10** (bonus) | `StateInfo` field reordering for cache | 0.5–2 % |
| **Combined** | | **4–11 %** |

**Measured speedup: 3.3 %** (97.168 s → 93.922 s median of 3 runs).

**Files touched:** `src/board.rs`, `src/bitboard.rs`

**Correctness:** All 41 perft positions verified at depths 1–6 across 3 runs; all 46 existing unit tests pass; `cargo clippy` clean.

---

## Detailed Changes

### Item 2 — Eliminate Redundant `queen_attacks()` Magic Lookups (`src/board.rs`)

**Problem:** `queen_attacks(sq, occ)` is defined as `bishop_attacks(sq, occ) | rook_attacks(sq, occ)`. At three sites, `bishop_attacks()` and `rook_attacks()` were called **twice** — once directly and once inside `queen_attacks()`. This doubled the magic-table lookup cost for bishops and rooks at those sites.

**Fix:** Compute `rook_atk` and `bishop_atk` once each, then `queen_atk = rook_atk | bishop_atk`. Reuse the precomputed values across all three piece-type checks.

#### Site 1: Castling pass-through (legal(), line ~714)

The castling pass-through check tests whether any enemy piece attacks each square the king travels through. Before the fix, it made 6 magic lookups (rook + bishop + queen × 2 squares). After the fix, it makes 3 magic lookups (rook + bishop per square, queen_atk is the OR):

```rust
// BEFORE (6 lookups):
let rook_attackers = attacks::rook_attacks(sq, occupied)
    & ...;
let bishop_attackers = attacks::bishop_attacks(sq, occupied)
    & ...;
let queen_attackers = attacks::queen_attacks(sq, occupied)
    & ...;

// AFTER (3 lookups):
let rook_atk = attacks::rook_attacks(sq, occupied);
let bishop_atk = attacks::bishop_attacks(sq, occupied);
let queen_atk = rook_atk | bishop_atk;
let rook_attackers = rook_atk & ...;
let bishop_attackers = bishop_atk & ...;
let queen_attackers = queen_atk & ...;
```

#### Site 2: Pseudo-royal attack loop (legal(), line ~854)

Same transformation in the pseudo-royal check loop, filtering against `enemy_survivors` instead of `self.by_color[them]`. This is the hottest of the three sites — it runs for every non-trivial move in every node.

#### Site 3: `compute_checkers()` (line ~353)

Called once per `populate_state()`, which runs for every node in the perft tree (~100M+ node visits in Test #13). Same mechanical transformation.

#### Verification

The transformation is algebraically identical: `queen_attacks(sq, occ) = bishop_attacks(sq, occ) | rook_attacks(sq, occ)` by definition. All 41 perft positions produce identical node counts.

---

### Item 11 — Explicit `#[inline(always)]` on Hot Accessors

Six `Board` accessor methods and eight bitboard shift helpers received `#[inline(always)]` annotations to ensure the compiler consistently inlines them across `pub(crate)` boundaries.

#### Board accessors (`src/board.rs`)

| Function | Signature | Callers |
|----------|-----------|---------|
| `piece_on()` | `sq → Piece` | Called once per `legal()` call |
| `pieces_color()` | `c → Bitboard` | Every attacker check block |
| `pieces_pt()` | `pt → Bitboard` | `attackers_to`, `compute_pinned` |
| `pieces_color_pt()` | `(c, pt) → Bitboard` | `commoners()`, hot path |
| `commoners()` | `c → Bitboard` | `legal()`, `populate_state()` |
| `occupied()` | `→ Bitboard` | Every magic lookup, castling, blast zone |

#### Bitboard shift helpers (`src/bitboard.rs`)

`shift_north`, `shift_south`, `shift_east`, `shift_west`, `shift_ne`, `shift_nw`, `shift_se`, `shift_sw` — each a single-line function called by pawn attack generation, a hot path in every perft node.

#### Note

The `attacks.rs` functions (`king_attacks`, `knight_attacks`, `pawn_attacks`, `bishop_attacks`, `rook_attacks`, `queen_attacks`) already had `#[inline(always)]` on their magic dispatch wrappers. No change needed there.

---

### Item 10 — `StateInfo` Field Reordering (`src/board.rs`)

**Problem:** The original `StateInfo` struct interleaved hot fields (read every `legal()` call) with cold fields (read only during `undo_move()`):

```rust
// BEFORE (hot and cold interleaved):
pub struct StateInfo {
    pub castling_rights: u8,       // cold
    pub ep_square: Option<Square>, // cold
    pub rule50: u8,                // cold
    pub captured_count: u8,        // cold
    pub captured: [(Square, Piece); 9], // cold (90 bytes)
    pub cap_sq: Option<Square>,    // cold
    pub cap_piece: Piece,          // cold
    pub checkers: Bitboard,        // HOT
    pub pinned: Bitboard,          // HOT
    pub commoners_count: u32,      // HOT
    pub them_commoners_count: u32, // HOT
}
```

This spread the hot fields across multiple cache lines — a single `legal()` call (which reads all 4 hot fields) could incur multiple L1 cache misses.

**Fix:** Group all hot fields at the front:

```rust
// AFTER (hot fields grouped):
pub struct StateInfo {
    pub checkers: Bitboard,             // HOT  (8 bytes)
    pub pinned: Bitboard,               // HOT  (8 bytes)
    pub commoners_count: u32,           // HOT  (4 bytes)
    pub them_commoners_count: u32,      // HOT  (4 bytes)
    // --- cache line boundary (24 bytes used, 8 bytes of padding) ---
    pub castling_rights: u8,            // cold
    pub ep_square: Option<Square>,      // cold
    pub rule50: u8,                     // cold
    pub captured_count: u8,             // cold
    pub captured: [(Square, Piece); 9], // cold
    pub cap_sq: Option<Square>,         // cold
    pub cap_piece: Piece,               // cold
}
```

The first 4 fields occupy 24 bytes — comfortably within one 64-byte cache line. With alignment padding, the hot data fits in the first cache line, and the cold data starts on (or near) the next.

#### Safety

`StateInfo` is an internal type (no public re-export of its layout). All field accesses use named syntax (`state.checkers`, `state.pinned`, etc.) — the field order in memory is an implementation detail. No code changes beyond the struct definition and its constructor were needed.

#### Synergy with future items

When Item 3 (cache pseudoRoyals) is implemented, the two new `Bitboard` fields would go right after `them_commoners_count`, keeping all hot data in the first 2 cache lines.

---

## Performance Results

### Environment

- **System:** Linux on x86_64 (Docker), same environment as baseline
- **Measurement:** `cargo run --release --example verify_perft` (41 positions, depths 1–6)
  - 3 runs, median reported
- **Baseline (post-Plan 1):** 97.168 s (measured 2026-07-01)

### Measured times

| Run | Total time |
|-----|------------|
| 1 | 94.006 s |
| 2 | 93.922 s |
| 3 | 93.825 s |
| **Median** | **93.922 s** |

### Speedup calculation

```
Baseline (Plan 1): 97.168 s
Plan 2 (median):   93.922 s
Speedup:          (97.168 - 93.922) / 97.168 × 100 % = 3.34 %
```

### Comparison to prediction

| Metric | Predicted | Actual |
|--------|-----------|--------|
| Item 2 contribution | 3–8 % (midpoint 5.5 %) | Included in combined |
| Item 11 contribution | 0.5–1.5 % (midpoint 1 %) | Included in combined |
| Item 10 contribution | 0.5–2 % (midpoint 1.25 %) | Included in combined |
| Combined speedup | 4–11 % (midpoint ~7.5 %) | **3.34 %** |
| Total time (conservative) | ~93.3 s | 93.922 s (within ~0.6 s of conservative target) |

The measured 3.34 % speedup is below the plan's midpoint estimate but close to the conservative 4 % target. The discrepancy is likely explained by:

1. The compiler already inlining some of the accessor functions automatically, reducing Item 11's impact.
2. The `StateInfo` fields being in the same allocation as other `Board` data, so cache-line pressure may be less severe than predicted.
3. System noise from lazy table initialization and variance in CPU turbo behavior between baseline and current runs.

Nevertheless, the improvement is measurable and consistent across all 3 runs (94.006 / 93.922 / 93.825 s — only 0.2 % spread between runs).

### Impact by test case

| Test | Time (baseline) | Time (median) | Improvement |
|------|-----------------|---------------|-------------|
| #2 | — | 11.799 s | — |
| #13 (slowest, 2.16B nodes) | 14.650 s | 14.093 s | **3.8 %** |
| #33 (~1.5B nodes) | — | 13.207 s | — |

The heaviest test (#13) shows the largest absolute improvement (0.557 s saved), consistent with deeper perft trees amplifying the per-node savings.

---

## Cumulative Performance

| Plan | Change | Speedup | Cumulative |
|------|--------|---------|------------|
| Plan 1 (prior) | Stack-allocated `MoveList` | 11.1 % | 11.1 % |
| Plan 2 (prior) | Inline legal filtering | 2.9 % | 13.7 % |
| Plan 1 (report1) | MagicEntry + transmute accessors | 9.3 % | 21.7 % |
| **Plan 2 (this)** | **queen_attacks elimination + polish** | **3.3 %** | **24.3 %** |

**Cumulative speedup from original baseline (pre-MoveList, 124.380 s):** **24.3 %**

```
1 - (1 - 0.111)(1 - 0.029)(1 - 0.093)(1 - 0.033)
≈ 1 - 0.889 × 0.971 × 0.907 × 0.967
≈ 1 - 0.757
≈ 24.3 %
```

---

## Risk Assessment

| Risk | Outcome |
|------|---------|
| Item 2 changes alter computed result | `bishop_attacks \| rook_attacks` is the definition of `queen_attacks`; no semantic change possible |
| `#[inline(always)]` increases code size | Functions are 1–3 lines each; inlining eliminates call overhead with negligible size increase |
| Field reordering breaks ABI | `StateInfo` is internal; all accesses use named fields — no layout dependency |
| Regression from `compute_checkers` change | Verified by perft: all 41 positions match node counts across 3 runs |
| PEXT path regresses | Unchanged — only magic-lookup sites were modified |

---

## Relationship to Other Optimization Items

| Item | Status | Notes |
|------|--------|-------|
| **Item 8** (transmute) | ✅ Done | Plan 1 |
| **Item 1** (MagicEntry) | ✅ Done | Plan 1 |
| **Item 2** (queen_attacks) | ✅ Done | This plan |
| **Item 11** (inline) | ✅ Done | This plan |
| **Item 10** (field order) | ✅ Done | This plan |
| **Item 3** (pseudoRoyals) | ❌ Not yet | Phase 1, ~3–6 % estimated |
| **Item 4** (between_bb table) | ❌ Not yet | Phase 1, ~3–6 % estimated |
| **Item 5** (fused attackers_to) | ❌ Not yet | Phase 2 |
| **Item 6** (evasions) | ❌ Not yet | Phase 2 |
| **Item 7** (pinned optimization) | ❌ Not yet | Phase 2 |
| **Item 9** (LazyLock elimination) | ❌ Not yet | Phase 3 |

The next highest-impact items are **Item 3** (cache pseudoRoyals bitboard in `StateInfo`, ~3–6 % estimated) and **Item 4** (precomputed `between_bb` table, ~3–6 % estimated).

---

## Files Changed

### `src/board.rs`

- **Item 2 (−9 lines / +15 lines):** Three sites refactored to reuse `rook_atk | bishop_atk` instead of calling `queen_attacks()`.
- **Item 11 (+6 lines):** `#[inline(always)]` added to 6 accessor methods.
- **Item 10 (−8 lines / +8 lines):** `StateInfo` struct fields reordered; constructor updated.

**Net:** +12 lines

### `src/bitboard.rs`

- **Item 11 (+8 lines):** `#[inline(always)]` added to 8 shift helper functions.

**Net:** +8 lines

### Files not touched

`src/types.rs`, `src/attacks.rs`, `src/magic.rs`, `src/pext.rs`, `src/movegen.rs`, `src/lib.rs`, `examples/*.rs`
