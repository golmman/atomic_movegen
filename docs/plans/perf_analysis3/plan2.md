# Plan 2 — Eliminate Redundant `queen_attacks()` Magic Lookups + Polish

## Summary

Building on Plan 1 (Item 8 + Item 1), this plan targets the remaining
redundant magic lookups in `legal()` and related hot paths, along with
two trivial polish items.

| Item | Description | Est. Speedup | Effort |
|------|-------------|-------------|--------|
| **Item 2** (primary) | Eliminate redundant `queen_attacks()` magic lookups | 3–8 % | ~15 lines |
| **Item 11** (bonus) | Explicit `#[inline(always)]` on hot accessors | 0.5–1.5 % | ~6 lines |
| **Item 10** (bonus) | `StateInfo` field reordering for cache | 0.5–2 % | ~5 lines |
| **Combined** | | **4–11 %** | ~26 lines |

**Baseline** (post-Plan 1): **97.168 s** (measured 2026-07-01, 41 positions,
depths 1–6).

**Measured time target:** 97.168 s × (1 − 0.04) ≈ **93.3 s** (conservative),
down to 97.168 × (1 − 0.11) ≈ **86.5 s** (optimistic).

**Files touched:** `src/board.rs` only (all three items).

---

## Detailed Changes

### Item 2 — Eliminate Redundant `queen_attacks()` Magic Lookups

**Problem:** `queen_attacks(sq, occ)` is defined as
`bishop_attacks(sq, occ) | rook_attacks(sq, occ)`. In three locations,
`bishop_attacks()` and `rook_attacks()` are called **twice** — once
directly and once inside `queen_attacks()`.

#### Three fix sites in `src/board.rs`:

##### Site 1: Castling pass-through (lines 714–744)

```rust
// BEFORE (6 magic lookups + OR):
let rook_attackers = attacks::rook_attacks(sq, occupied)
    & self.by_type[PieceType::Rook as usize] & self.by_color[them as usize];
let bishop_attackers = attacks::bishop_attacks(sq, occupied)
    & self.by_type[PieceType::Bishop as usize] & self.by_color[them as usize];
let queen_attackers = attacks::queen_attacks(sq, occupied)
    & self.by_type[PieceType::Queen as usize] & self.by_color[them as usize];

// AFTER (3 magic lookups + OR):
let rook_atk = attacks::rook_attacks(sq, occupied);
let bishop_atk = attacks::bishop_attacks(sq, occupied);
let queen_atk = rook_atk | bishop_atk;
let rook_attackers = rook_atk & self.by_type[PieceType::Rook as usize] & self.by_color[them as usize];
let bishop_attackers = bishop_atk & self.by_type[PieceType::Bishop as usize] & self.by_color[them as usize];
let queen_attackers = queen_atk & self.by_type[PieceType::Queen as usize] & self.by_color[them as usize];
```

##### Site 2: Pseudo-royal attack loop (lines 854–868)

Same transformation as Site 1 but using `enemy_survivors` instead of
`self.by_color[them]`.

##### Site 3: `compute_checkers()` (lines 353–368)

Same transformation again, using `self.pieces_color(them)`.

#### Why not `compute_checkers`?

Lines 353–368 are called once per `populate_state()` call, not per
`legal()` call. However, `populate_state()` is called for every node
in the perft tree (via `do_move()` at line 575), so the savings
accumulate across ~100M+ node visits in Test #13 alone.

#### Safety

The transformation is purely mechanical and does not change the
computed result. `queen_attacks(sq, occ) = bishop_attacks(sq, occ) | rook_attacks(sq, occ)`
by definition; computing `bishop_atk | rook_atk` once and reusing it
is algebraically identical.

---

### Item 11 — Explicit `#[inline(always)]` on Hot Accessors

**Problem:** Several hot accessor functions in `Board` lack explicit
`#[inline]` annotations. While the compiler often inlines them
automatically, explicit `#[inline(always)]` ensures consistent
behavior, especially across the `pub(crate)` boundary.

#### Functions to annotate in `src/board.rs`:

| Function | Line | Notes |
|----------|------|-------|
| `Board::piece_on()` | 260 | Called once per `legal()` call (piece existence check) |
| `Board::pieces_color()` | 272 | Called in every attacker check block |
| `Board::pieces_pt()` | 276 | Used in attackers_to and compute_pinned |
| `Board::pieces_color_pt()` | 280 | Used in commoners(), which is called in legal() |
| `Board::occupied()` | 300 | Called in every magic lookup, castling, blast zone |
| `Board::commoners()` | 296 | Called in legal() and populate_state() |

#### Additional functions in `src/bitboard.rs`:

| Function | Notes |
|----------|-------|
| `bitboard_ops.rs` shift helpers (north, south, etc.) | Called by pawn attack generation |

**Note:** The `attacks.rs` functions (`king_attacks`, `knight_attacks`,
`pawn_attacks`) already have `#[inline(always)]` on the magic dispatch
wrappers. No change needed there.

---

### Item 10 — `StateInfo` Field Reordering

**Problem:** `StateInfo` currently interleaves hot fields (`checkers`,
`pinned`, `commoners_count`, `them_commoners_count`) with cold fields
(`castling_rights`, `ep_square`, `rule50`, `captured`, `cap_sq`,
`cap_piece`). This spreads the hot fields across multiple cache lines.

**Fix:** Group all hot fields at the front of the struct:

```rust
pub struct StateInfo {
    // Hot fields (read in legal() and generate_legal())
    pub checkers: Bitboard,
    pub pinned: Bitboard,
    pub commoners_count: u32,
    pub them_commoners_count: u32,

    // Cold fields (read in undo_move, write in do_move / populate_state)
    pub castling_rights: u8,
    pub ep_square: Option<Square>,
    pub rule50: u8,
    pub captured_count: u8,
    pub captured: [(Square, Piece); 9],
    pub cap_sq: Option<Square>,
    pub cap_piece: Piece,
}
```

This ensures the first 4 fields (28 bytes) fit in one cache line,
reducing L1 misses during `legal()`.

**Synergy with future items:** When Item 3 (cache pseudoRoyals) is
implemented, the two new `Bitboard` fields would go right after
`them_commoners_count`, keeping all hot data in the first 2 cache
lines.

---

## Performance Prediction

| Benchmark | Baseline (Plan 1) | Predicted (Plan 2) | Speedup |
|-----------|------------------|-------------------|---------|
| Total verify_perft | 97.168 s | 86.5–93.3 s | 4–11 % |
| Test #13 (slowest) | 14.650 s | ~13.0–14.1 s | 4–11 % |

Cumulative speedup from original baseline (pre-MoveList, 124.380 s):

```
1 - (1 - 0.111)(1 - 0.029)(1 - 0.093)(1 - 0.075)
≈ 1 - 0.889 × 0.971 × 0.907 × 0.925
≈ 1 - 0.724
≈ 27.6 % cumulative
```

---

## Verification

### Correctness

1. `cargo test` — All existing unit tests must pass.
2. `cargo run --release --example verify_perft` — All 41 positions,
   depths 1–6, must produce identical node counts to the baseline.

### Performance

1. Run `cargo run --release --example verify_perft` before and after.
2. Compare total wall-clock time.
3. Report individual test times for the 3 slowest positions (#2, #13, #33).

### Environment

- **System:** Linux on x86_64 (Docker), same environment as baseline.
- **Measurement:** `cargo run --release --example verify_perft`
  (3 runs, median reported to account for system noise).

---

## Implementation Order

1. **Item 2** — Remove redundant queen_attacks() (3 mechanical edits in `board.rs`)
2. **Item 11** — Add `#[inline(always)]` annotations (~6 lines)
3. **Item 10** — Reorder StateInfo fields (move 4 field declarations)
4. Run full test suite and verify_perft
5. Measure and document results

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Item 2 changes are algebraically identical | `bishop_attacks \| rook_attacks` is definition of `queen_attacks`; no semantic change |
| `#[inline(always)]` can increase code size | Functions are 1–3 lines each; inlining is always beneficial in hot paths |
| Field reordering breaks ABI | `StateInfo` is internal (no pub re-export of layout); only accessed through methods |
| No regression from `compute_checkers` change | Verified by perft: all 41 positions match node counts |

---

## Relationship to Other Optimization Items

| Item | Status | Notes |
|------|--------|-------|
| **Item 8** (transmute) | ✅ Done | Plan 1 |
| **Item 1** (MagicEntry) | ✅ Done | Plan 1 |
| **Item 2** (queen_attacks) | ⏳ This plan | Primary |
| **Item 11** (inline) | ⏳ This plan | Bonus |
| **Item 10** (field order) | ⏳ This plan | Bonus |
| **Item 3** (pseudoRoyals) | ❌ Not yet | Phase 1, next after this plan |
| **Item 4** (between_bb table) | ❌ Not yet | Phase 1 |
| **Item 5** (fused attackers_to) | ❌ Not yet | Phase 2 |
| **Item 6** (evasions) | ❌ Not yet | Phase 2 |
| **Item 7** (pinned optimization) | ❌ Not yet | Phase 2 |
| **Item 9** (LazyLock elimination) | ❌ Not yet | Phase 3 |

The next items after Plan 2 would be **Item 3** (cache pseudoRoyals
bitboard in StateInfo, ~3–6 % estimate) and **Item 4** (precomputed
between_bb table, ~3–6 % estimate).
