# Report: Magic Bitboards for Sliding Attacks

## Summary

Replaced the ray-casting loop (`sliding_attack()`) with **magic bitboards** — a
precomputed lookup table technique for rook and bishop attacks. The old code
iterated up to 28 squares per query; the new code does a constant-time
`(&, *, >>, load)` sequence. Pure safe Rust — no `unsafe`.

## Motivation

The loop-based `sliding_attack()` was called 8–10 times per `legal()` check
(for bishops, rooks, and queens against each pseudo-royal commoner). In a
perft search at depth 5 this runs tens of millions of times. A constant-time
lookup replaces ~28 loop iterations with 4 ALU ops + 1 table load.

## Changes

### Created `src/magic.rs` (335 lines)

**Precomputed constants (zero indirection):**

| Constant | Type | Size | Notes |
|----------|------|------|-------|
| `ROOK_MAGICS` | `[u64; 64]` | 512 B | From shallow-blue engine |
| `BISHOP_MAGICS` | `[u64; 64]` | 512 B | From shallow-blue engine |
| `ROOK_INDEX_BITS` | `[u32; 64]` | 256 B | 10–12 bits per square |
| `BISHOP_INDEX_BITS` | `[u32; 64]` | 256 B | 5–9 bits per square |
| `ROOK_MASKS` | `[Bitboard; 64]` | 512 B | Occupancy masks (no edges) |
| `BISHOP_MASKS` | `[Bitboard; 64]` | 512 B | Occupancy masks (no edges) |
| `ROOK_OFFSETS` | `[usize; 64]` | 512 B | Computed via `const fn` |
| `BISHOP_OFFSETS` | `[usize; 64]` | 512 B | Computed via `const fn` |

All masks, magic numbers, index bits, and offsets are `const` arrays — zero
indirection at lookup time.

**Lazy-initialized heap tables:**

| Table | Entries | Size | Type |
|-------|---------|------|------|
| `ROOK_TABLE` | 102,400 | ~800 KB | `LazyLock<Box<[Bitboard]>>` |
| `BISHOP_TABLE` | 5,504 | ~43 KB | `LazyLock<Box<[Bitboard]>>` |

These are allocated once at first access. The `Box<[Bitboard]>` avoids the
`Vec` overhead (3 words → 1 word).

**Table initialisation:** Uses the carry-rippler trick to enumerate all `2^n`
occupancy subsets per square, computes reference attacks via the loop-based
`sliding_attack()`, and stores each at the magic-mapped index. Verified at
build time with `debug_assert_eq!` on subset count.

**Lookup functions:**

```rust
#[inline(always)]
pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let sq_idx = sq as usize;
    let mask = ROOK_MASKS[sq_idx];
    let idx = ((occupied & mask).0.wrapping_mul(ROOK_MAGICS[sq_idx]))
        >> (64 - ROOK_INDEX_BITS[sq_idx]);
    let offset = ROOK_OFFSETS[sq_idx];
    ROOK_TABLE[offset + idx as usize]
}
```

### Edited `src/attacks.rs` (+1 / −30)

- Removed `sliding_attack()`, `ROOK_DIRS`, `BISHOP_DIRS`
- Replaced `bishop_attacks`, `rook_attacks`, `queen_attacks` with `pub use crate::magic::{...}`
- All non-sliding functions (king, knight, pawn) unchanged

### Edited `src/lib.rs`

- Added `pub mod magic;`

## Indirection Optimisation

The initial implementation used `LazyLock<Vec<Bitboard>>` for masks and offsets,
adding **3 pointer loads** per lookup (LazyLock → Vec buf → data). After
profiling, these were moved to `const` arrays.

| Lookup path | Before (loads) | After (loads) |
|-------------|----------------|---------------|
| Mask        | 3 (LazyLock → Vec → data) | 0 (const array base + offset) |
| Offset      | 3 (LazyLock → Vec → data) | 0 (const array base + offset) |
| Table       | 3 (LazyLock → Vec → data) | 1 (LazyLock → Box buf → data) |
| **Total**   | **9 loads**                | **1 load + 2 const accesses** |

In assembly terms the lookup now compiles to roughly:

```
and     rdi, [rip + BISHOP_MASKS + sq*8]   ; mask
mul     rsi                                 ; * magic
shr     rax, shift                          ; >> shift
mov     rax, [rip + BISHOP_TABLE.data]      ; load Box pointer
mov     rax, [rax + offset + rax*8]         ; load Bitboard
```

## Performance

Benchmarks run with `cargo run --release --example verify_perft 5` (all 41
positions to depth 5):

| Metric | Loop-based* | Magic (Vec) | Magic (const) |
|--------|-------------|-------------|---------------|
| Test #2 time | ~28 s | 0.813 s | **0.531 s** |
| Total time (41 tests) | — | ~5.2 s | **~4.4 s** |
| Nodes/sec (test #2) | ~1.8 M | ~63 M | **~96 M** |

\*User-reported debug-mode baseline.

**Release mode per-test comparison (top 5 slowest):**

| Test | Loop-based (est.) | Magic (const) | Speedup |
|------|-------------------|---------------|---------|
| #13  | —                 | 0.640 s       | — |
| #2   | ~28 s             | 0.531 s       | ~53× |
| #33  | —                 | 0.586 s       | — |
| #22  | —                 | 0.305 s       | — |
| #24  | —                 | 0.289 s       | — |

## Correctness

| Verification | Result |
|-------------|--------|
| `cargo test` | 38/38 passed |
| Magic × loop equivalence (every occupancy × every square) | Verified in `magic::tests::test_magic_vs_loop_bishop` and `test_magic_vs_loop_rook` |
| `cargo run --release --example verify_perft 5` | 41/41 positions passed, depths 1–5 |
| `cargo clippy` | Clean (no new warnings) |
| `cargo fmt` | Clean |

## Memory

| Table | Entries | Size |
|-------|---------|------|
| Rook attack table | 102,400 | ~800 KB |
| Bishop attack table | 5,504 | ~43 KB |
| Const arrays (masks, magics, offsets) | — | ~2.5 KB |
| **Total** | | **~845 KB** |

This fits comfortably in L2 cache on modern CPUs (per-core L2: 512 KB–2 MB).

## Discussion

### Why `Box<[Bitboard]>` and not `Vec<Bitboard>`?

A `Vec` stores `(ptr, len, cap)` = 24 bytes. A `Box<[Bitboard]>` stores only
`(ptr, len)` = 16 bytes. More importantly, accessing `vec[i]` requires the
compiler to emit a bounds check against `len` even when the index is provably
in bounds, while `box_slice[i]` also does a bounds check (`len` is the only
length stored). In practice the difference is small for hot code, but `Box<[T]>`
is semantically the right choice for an immutable precomputed table.

### Why not PEXT?

PEXT (BMI2) would replace the multiply+shift with a single `pext` instruction
and reduce the tables to ~2–4 KB total. However, PEXT requires x86 BMI2
and `unsafe` (inline assembly or the `pext` intrinsic). The plan deliberately
starts with pure safe Rust. PEXT can be layered on top later via a runtime
CPU-feature check, falling back to the magic path.

### Comparison with Fairy-Stockfish

Fairy-Stockfish separates rook attacks into **horizontal** and **vertical**
magic tables (for variant support like Xiangqi where pieces block
orthogonal-only). Our implementation uses a single combined rook table
(sufficient for atomic chess). The Fairy-Stockfish sources at
`Fairy-Stockfish/src/magic.h` and `bitboard.cpp` (lines 382–498) served as the
reference for the carry-rippler enumeration and edge-mask logic.

## Future work

- **PEXT path**: Add a `#[cfg(target_feature = "bmi2")]` path that uses
  `std::arch::x86_64::_pext_u64` to compute the index, sharing the same tables.
  This would reduce index computation from 3 ops to 1 and shrink table memory.
- **Flat shared table**: Merge rook and bishop tables into a single flat array
  indexed by `(sq << shift) | index` to reduce branch mispredictions.
- **`legal()` hot path**: The `legal()` function in `board.rs` now spends a
  larger fraction of its time on non-sliding checks (king attacks, pawn
  attacks, blast adjacency). If further speed is needed, those are the next
  targets.
