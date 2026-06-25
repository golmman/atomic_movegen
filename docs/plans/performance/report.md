# Performance Optimisation: Summary and Future Directions

## Overview

This directory documents four performance-optimisation efforts for the atomic
chess move generator. Three were implemented successfully; one
(incremental checkers/pinners) was merged as infrastructure only, with the
intended speedup deferred to future work.

**Implementation order** (per `notes.md`):

1. Fixed-size `StateInfo` (no-heap captured pieces)
2. Magic bitboards (constant-time sliding attacks)
3. Incremental checkers/pinners (infrastructure merged; no speedup yet)
4. PEXT (BMI2) sliding attacks (refinement on magic)

---

## Report Summaries

### 1. Fixed-size `StateInfo` — `report_state_info.md`

**Problem:** `StateInfo::captured_pieces: Vec<(Square, Piece)>` allocated and
deallocated heap memory for every `do_move()` / `undo_move()` pair (~119M
times at perft depth 6).

**Solution:** Replaced `Vec` with `[(Square, Piece); 9]` (a fixed-size array)
plus `u8` count. The constant `9` is the proven upper bound: a 3×3 king-move
blast zone covers at most 9 squares, and pawns are immune to blast.

**Result:** Eliminated ~119M heap allocations at depth 6. Perft time on the
starting position at depth 6: **~1.79 s** (~66 M nodes/s).

---

### 2. Magic Bitboards — `report_magic.md`

**Problem:** Sliding attacks used a loop-based `sliding_attack()` that
iterated up to 28 squares per query, called 8–12 times per `legal()` check.

**Solution:** Replaced the loop with magic bitboards — a constant-time
`(&, *, >>, load)` sequence using precomputed lookup tables (~845 KB total).

**Result:** ~53× speedup over the debug-mode loop baseline on the heaviest
test. Total perft(5) across all 41 positions: **~4.4 s** (vs ~5.2 s with an
intermediate `Vec`-backed implementation). Pure safe Rust, zero `unsafe`.

---

### 3. Incremental Checkers/Pinners — `report_checkers_pinners.md`

**Problem:** `legal()` recomputed checkers and pinned pieces from scratch
for every pseudo-legal move (20–50× per position).

**Solution (infrastructure):** Added `checkers`, `pinned`, `commoners_count`,
`them_commoners_count` fields to `StateInfo`. Extracted `compute_checkers()`
and `compute_pinned()` methods. Changed `legal()` signature to accept
`&StateInfo`. Bugfix in pin-detection (`between.count() == 1` instead of
`!between.more_than_one()`).

**Attempted early-out reverted:** Three attempts at a correctness-preserving
early-out failed — the blast mechanic can clear multiple blockers
simultaneously in ways that a simple "no checkers, no pins" guard cannot
capture for atomic chess. See Lessons Learned in the full report.

**Final status:** Infrastructure merged with negligible performance change
(~4.45 s total perft(5), within noise of ~4.4 s baseline). Ready for future
optimisations that can leverage the cached state.

---

### 4. PEXT (BMI2) — `report_pext.md`

**Problem:** The magic bitboard index computation `(occ & mask) * magic >> shift`
uses 3 ALU ops. PEXT collapses this into one CPU instruction.

**Solution:** Added a PEXT-based lookup path using `_pext_u64` via
`#[target_feature(enable = "bmi2")]`. Runtime dispatch on x86_64;
`#[cfg]`-gated to zero overhead on ARM.

**Result:** On AMD Zen 4, perft(6) starting position improved from **1.79 s
(~66 M nodes/s) to 0.91 s (~131 M nodes/s)** — a 2× throughput gain. Zero
regression on ARM after fixing a `LazyLock` dispatch overhead issue. The
`unsafe` surface is fully contained in `pext.rs`.

---

## Current Performance Baseline

Starting position, perft(6), 118,926,425 nodes:

| Configuration | Time | Nodes/s | Relative |
|---|---|---|---|
| Magic (no PEXT) | ~1.79 s | ~66 M | 1× |
| PEXT (BMI2, Zen 4) | ~0.91 s | ~131 M | ~2× |
| ARM (Apple M3, magic) | ~0.85 s | ~140 M | — |

---

## Future Optimisations

Listed roughly in order of estimated impact. Rough impact is given as a
factor relative to the current baseline (~1.79 s / ~66 M nodes/s on Zen 4
with magic, or ~0.91 s / ~131 M on Zen 4 with PEXT).

### High Impact (≥1.5× speedup)

| # | Optimisation | Description | Estimated impact | Prerequisites |
|---|-------------|-------------|------------------|---------------|
| 1 | **Correct early-out for `legal()`** | A sound early-out using `state.checkers` and `state.pinned` to skip the full attack scan for trivially legal moves. Previous attempts failed due to atomic blast subtleties. A correct implementation would skip 60–80% of `legal()` calls. | **1.5–2×** on perft | Deep understanding of atomic-blast legality invariants |
| 2 | **Incremental post-move checkers in `legal()`** | Instead of recomputing post-move checkers from scratch, compute them incrementally by adjusting the cached pre-move `state.checkers` for: (a) the piece that moved, (b) the blast victims removed, and (c) any newly opened lines. | **1.5–2.5×** on perft | The cached `state.checkers` infrastructure from report #3 |

### Medium Impact (1.1–1.5× speedup)

| # | Optimisation | Description | Estimated impact | Prerequisites |
|---|-------------|-------------|------------------|---------------|
| 3 | **`check_squares[pt]` pattern** | Precompute, for each piece type, the set of squares from which that piece would give check to the pseudo-royal commoner. Allows `legal()` to test "does this square give check?" in O(1) instead of scanning rays. Fairy-Stockfish uses this extensively. | **1.2–1.5×** | StateInfo infrastructure |
| 4 | **Dedicated PEXT queen table** | Currently `queen_attacks = bishop_attacks | rook_attacks`, which computes two PEXT indices. A dedicated queen table (combined bishop+rook mask) computes one PEXT index and one lookup instead. | **1.1–1.3×** (PEXT path only) | PEXT tables |
| 5 | **Optimise non-sliding hot paths** | With sliding attacks now ~2–3 cycles, the remaining bottlenecks in `legal()` are king attacks, pawn attacks, and blast-adjacency checks. Micro-optimising these (precomputed tables, bitboard tricks) would further reduce per-`legal()` cost. | **1.1–1.3×** | Profiling data to identify the hottest remaining loops |

### Low Impact (<1.1× speedup, but worthwhile)

| # | Optimisation | Description | Estimated impact | Prerequisites |
|---|-------------|-------------|------------------|---------------|
| 6 | **Lazy checkers/pinners computation** | Instead of computing `checkers`/`pinned` eagerly in `generate_legal()`, compute them lazily on first access and cache in `StateInfo`. If the early-out never fires for a position, the computation is never done. | **1.02–1.05×** (avoided work) | StateInfo infrastructure |
| 7 | **Flat shared attack table** | Merge the per-square rook and bishop tables into a single flat array indexed by `(sq << shift) | index`. Reduces branch mispredictions and icache pressure from separate table accesses. | **1.01–1.05×** | — |
| 8 | **Reduce `StateInfo` size** | The current `[(Square, Piece); 9]` array occupies ~108 bytes on the stack. A more compact encoding (e.g., two `Bitboard`s + a piece list) could shrink it, reducing stack pressure in deep searches. | **1.01–1.03×** | — |
| 9 | **SSE/AVX2 parallel bits extract** | Software-emulated PEXT using SIMD, providing a speedup on ARM NEON or x86 without BMI2. | ~1.05× on non-BMI2 | PEXT tables + SIMD intrinsics |

### Non-performance (correctness / maintenance)

| # | Optimisation | Description | Impact |
|---|-------------|-------------|--------|
| 10 | **Remove magic tables on BMI2-only builds** | Once BMI2 is universally required (or for BMI2-only binary builds), the ~843 KB magic tables can be removed, leaving only the PEXT tables. | Zero performance change; ~843 KB memory saving |
| 11 | **Full incremental checkers update** | Instead of recomputing `state.checkers` from scratch in `generate_legal()`, update it incrementally for the single changed piece and blast zone. | Small-to-medium; reduces a fixed O(n) cost to O(1) |

---

## Cross-cutting Lessons

1. **Measure end-to-end perft, not micro-benchmarks.** Early micro-benchmarks
   of the `legal()` early-out showed 30–50% improvement, but integrated perft
   was *slower* because the pre-computation overhead was not accounted for.

2. **Platform dispatch must be zero-cost on non-target architectures.** The
   PEXT `LazyLock` dispatch caused an 11% regression on ARM because
   `Once::call_once` uses atomic acquire barriers. Fixed with `#[cfg]` to
   elide the dispatch entirely on ARM.

3. **Atomic chess breaks standard chess invariants.** The blast can clear
   multiple blockers simultaneously, making the simple "no checkers + no pins
   ⇒ all non-commoner moves are legal" rule incorrect. Optimisations from
   standard chess need careful re-validation.

4. **`unsafe` can be well-contained.** The PEXT implementation is the only
   `unsafe` code in the crate (<30 lines), all inside `pext.rs`. On ARM these
   functions are never reachable because `#[cfg]` elides the calling code.

---

*Generated from reports in `plans/performance/`. Full detail for each
implementation in the respective `report_*.md` file.*
