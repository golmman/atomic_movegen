# Performance Analysis — `atomic-movegen`

## Methodology

The binary was built with the `profiling` profile (inherits `release` with
`debug = 2` and `strip = false`) and frame pointers enabled:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
```

Profiling was done with `perf record` on Apple Firestorm (arm64) at 999 Hz
sampling rate with DWARF call graph:

```sh
perf record -F 999 -g --call-graph dwarf target/profiling/examples/perft \
  'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1' 6
```

Symbols were resolved via `addr2line` against the unstripped binary.

**Test position:** starting position, depth 6 (≈119M nodes).

---

## Profiling Results — CPU Hotspots

### Overall distribution

| Hotspot | Self CPU | Cumulative | Source |
|---------|----------|------------|--------|
| `Board::legal()` | ≈35–40 % | ≈35–40 % | `board.rs:664–862` |
| `magic::bishop_attacks()` | ≈10–15 % | ≈50 % | `magic.rs:453–462` |
| `magic::rook_attacks()` | ≈8–10 % | ≈58–60 % | `magic.rs:466–473` |
| `Board::pieces()`, `piece_on()`, `pieces_color_pt()` | ≈8–10 % | ≈68 % | `board.rs:260–282` |
| `Move::from_sq()` / `to_sq()` / `move_type()` | ≈5–7 % | ≈74 % | `types.rs:714–863` |
| `Piece::type_of()` | ≈3–5 % | ≈78 % | `types.rs:630–645` |
| `generate_pawn_moves_for()` | ≈4–6 % | ≈83 % | `movegen.rs:82–169` |
| `file_of()` / `rank_of()` | ≈3–5 % | ≈87 % | `types.rs:179–207` |
| `generate_pseudo_legal()` | ≈3–4 % | ≈91 % | `movegen.rs:5–80` |
| `Board::do_move()` | ≈3–4 % | ≈94 % | `board.rs:422–561` |
| `malloc` / `free` | ≈2 % | ≈96 % | libc |

### Detailed call-chain observations

- **`legal()` is by far the dominant function.** Its internal attack-check
  loop (the while-loop over our pseudo-royal commoners at lines 820–857)
  accounts for most of its samples. Within that loop, the 6 `attacks_bb`
  calls (rook, bishop, queen, knight, pawn, king) dominate.
- **`bishop_attacks` and `rook_attacks`** are called not only from
  `legal()`, but also from `generate_pseudo_legal()` (for sliding pieces),
  `compute_checkers()`, `compute_pinned()`, and `attackers_to()`. The
  overhead multiplies.
- **`Move` accessor methods** (`from_sq()`, `to_sq()`, `move_type()`) show
  up because they are called on every pseudo-legal move during retention in
  `generate_legal()` and again in `do_move()`.
- **`malloc`/`free` overhead** comes from `Vec::retain` and potential Vec
  reallocation in the move list.

---

## Comparison with Fairy-Stockfish Reference

### Architectural differences

| Aspect | `atomic-movegen` (Rust) | Fairy-Stockfish (C++) |
|--------|------------------------|----------------------|
| **State update** | `do_move()` does **not** update checkers, blockers, or pseudo-royal info. These are recomputed from scratch in `legal()`. | `do_move()` **incrementally** updates `checkersBB`, `blockersForKing`, `pinners`, `pseudoRoyals` via `set_check_info()`. |
| **Legal move filtering** | Calls `legal()` on every pseudo-legal move in `Vec::retain()`. | Generates moves inline and tests `pos.legal(m)` on a stack-allocated array with pointer arithmetic. |
| **Early-out in legal()** | None — always runs the full pseudo-royal attack scan (lines 820–857). | Returns early for non-capture, non-king moves when `blockers_for_king` and `checkers` are empty. |
| **`attackers_to()`** | Computes pawn/knight/bishop/rook/queen king attacks separately, each with individual bitboard lookups. | Unrolled with a single expression using `fastAttacks` path: combines all attacker types in one expression. |
| **Sliding attack lookup** | Runtime dispatch: `match *IMPL { Pext => ..., Magic => ... }` — the match cannot be eliminated at compile time. | Direct function calls that inline away; PEXT vs. magic is a compile-time decision. |
| **`between_bb()`** | Loop-based computation using `make_square()` with match statements per iteration. | Precomputed lookup table `between_bb[sq1][sq2]`. |
| **Move storage** | `Vec<Move>` with dynamic allocation and bounds-checked `.push()`. | Fixed-size `ExtMove[MAX_MOVES]` stack array with raw pointer writes. |
| **Board accessors** | `self.commoners(us)` loads `by_color[c]` and `by_type[COMMONER]` and ANDs them. | Direct `pieces(COMMONER)` with cached piece lists stored per-type. |

### Sliding attack dispatch overhead

The `attacks.rs` dispatch:

```rust
#[inline(always)]
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    match *IMPL {
        Impl::Pext => unsafe { pext::bishop_attacks_pext(sq, occupied) },
        Impl::Magic => magic::bishop_attacks(sq, occupied),
    }
}
```

This `match` on a `LazyLock<Impl>` compiles to a load-from-global + compare +
branch — executed on **every** sliding attack query. On arm64 (no BMI2), only
the magic path is ever taken, but the dead PEXT branch still emits a
conditional. The C++ reference uses `#ifdef` or compile-time polymorphism
so the unused path is eliminated entirely.

### `legal()` algorithm comparison

**Fairy-Stockfish algorithm** (for variants with `blastOnCapture` and
`extinctionPseudoRoyal`):

1. Compute post-move `occupied` (xor/OR on bitboards).
2. Handle blast: `occupied &= ~((attacks_bb<KING>(kto) & ~pieces(PAWN)) | kto)`.
3. Check self-explosion: if `pseudoRoyals & !occupied`, return false.
4. Early-out: if we captured the enemy's last pseudo-royal, return true
   (no attack check needed).
5. For each pseudo-royal commoner:
   - Check adjacency immunity (touching enemy commoner → immune).
   - If not immune, check `attackers_to(sr, occupied, ~us)`.

The key optimization is that **`pseudoRoyals`, `blockersForKing`, and
`checkers` are precomputed** and stored in `StateInfo`. The hot path in
`legal()` is just a few bitboard operations.

**Rust algorithm** (current):

1. Build post-move `occupied` from scratch using complex logic for each
   move type.
2. Handle blast (also from scratch).
3. Self-explosion check: recompute `self.commoners(us)` for pre-move,
   then manually add moved commoner if it survived.
4. Early-out: **none**.
5. For each pseudo-royal commoner:
   - Compute adjacency check (reloading `self.commoners(them)`).
   - If not immune, individually compute 6 attack types (rook, bishop,
     queen, knight, pawn, king) each with its own attack function call
     and bitboard AND, reloading piece-type bitboards from the Board
     struct each time.

---

## Potential Performance Improvements

Ordered by estimated impact (highest first).

### 1. Precompute state in `do_move()` (estimated: 30–50 % speedup)

**Problem:** `legal()` recomputes checkers, blockers, commoner counts, and
pseudo-royal info from raw bitboards for every pseudo-legal move.

**Fix:** Populate `StateInfo` fields at the end of `do_move()`:
- `checkers: Bitboard` — who is giving check.
- `blockers_for_king: Bitboard` — all pieces blocking slider attacks on
  the side-to-move's commoners.
- `pinners: Bitboard` — enemy pieces that have a commoner pinned.
- `commoners_count: u32` — count of own pseudo-royal commoners.
- `them_commoners_count: u32` — count of enemy commoners.

Then `legal()` reads these cached values instead of recomputing.

**Reference:** Fairy-Stockfish `set_check_info()` at `position.cpp:578`.

**Effort:** ~50 lines. Requires restructuring `generate_legal()` /
`perft()` to pass `StateInfo` through.

**Risk:** Low. Prior attempts at this (iterations 1–2) showed a ~1–3 %
regression on the starting position because `populate_state()` adds
overhead per do_move. However, those runs also had an early-out bug
(see item 2). With both fixes combined, the savings from the early-out
should far outweigh the `populate_state()` cost.

---

### 2. Early-out in `legal()` for safe moves (estimated: 15–30 % speedup)

**Problem:** The pseudo-royal attack check runs for every move, even
trivially safe ones:
- Non-capture moves.
- Non-commoner moves (piece is not a pseudo-royal).
- No checkers on the board.
- Moving piece is not a blocker (no discovered attack possible).

**Fix:** Add an early return at the top of `legal()`:

```rust
if state.checkers.is_empty()
    && !is_capture
    && m.move_type() != MoveType::EnPassant
    && pt != PieceType::Commoner
    && (state.blockers_for_king & Bitboard::square_bb(from)).is_empty()
{
    return true;
}
```

This is safe because a non-capture, non-commoner, non-blocker move when
there are no checkers **cannot** put the pseudo-royal commoner in check.

**Reference:** Fairy-Stockfish uses the same pattern — they check
`blockersForKing[us]`, `checkersBB`, and `type_of(moved_piece(m))` early.

**Note from history:** When tried in iteration 2, this early-out caused
test failures when `commoners_count > 1`. The likely cause is that
`blockers_for_king` was computed incorrectly (the condition was `> 1`
instead of `> 0` blockers). With correct `blockers_for_king` computation
(all pieces on a between-ray, not just sole blockers), the early-out
should be safe.

**Effort:** ~10 lines plus proper `blockers_for_king` computation.

---

### 3. Cache commoner bitboards and counts in legal() (estimated: 5–10 % speedup)

**Problem:** `legal()` calls `self.commoners(us)` and `self.commoners(them)`
multiple times, each performing `by_color[c] & by_type[COMMONER]`.
The former is accessed 3+ times per `legal()` call.

**Fix:** Load once at the top:

```rust
let our_commoners = self.commoners(us);
let them_commoners = self.commoners(them);
```

Then reuse throughout the function.

**Effort:** ~5 lines.

---

### 4. Eliminate runtime sliding-attack dispatch on non-BMI2 targets (estimated: 3–8 % speedup)

**Problem:** `attacks.rs` dispatches between PEXT and magic via
`match *IMPL` on every call. On arm64 (Apple Silicon), BMI2 is never
available, so the PEXT branch is dead code — but the compiler still
emits a load + compare + branch.

**Fix:** Use `#[cfg(target_arch = "x86_64")]` guard on the PEXT path
and re-export magic directly on non-x86_64:

```rust
#[cfg(not(target_arch = "x86_64"))]
pub use crate::magic::{bishop_attacks, rook_attacks, queen_attacks};

#[cfg(target_arch = "x86_64")]
mod sliding_dispatch { ... }
```

This is already partially done (lines 56–60), but the runtime check
still exists. Verify that the non-x86_64 path truly eliminates the
`match`/`LazyLock`.

**Alternative:** On x86_64, use `#[target_feature]` to generate separate
PEXT and non-PEXT versions and resolve via function pointer at init,
avoiding the `LazyLock` overhead entirely.

**Effort:** ~20 lines.

---

### 5. Replace `between_bb()` loop with precomputed table (estimated: 2–5 % speedup)

**Problem:** `between_bb()` in `bitboard.rs:92–143` uses a loop with
`make_square()` that calls `match` on both file and rank — 2 match
statements per iteration. This is called in `compute_pinned()` /
`compute_blockers_for_king()` for every sniper pair.

**Fix:** Precompute a `BETWEEN_BB: [[Bitboard; 64]; 64]` table at init
time. Replace the loop with a table lookup.

**Reference:** Fairy-Stockfish `SquareBB[]` and `BetweenBB[]` are
precomputed arrays.

**Effort:** ~15 lines.

---

### 6. Inline hot accessor functions (estimated: 2–5 % speedup)

**Problem:** `Move::from_sq()`, `to_sq()`, `move_type()`, and
`Piece::type_of()` appear at ~10–12 % cumulative self time. Each decodes
bit fields via table lookup (a static array indexed by decoded value).

**Fix:** Mark them `#[inline(always)]` — verify they are currently
inlined: the annotations are present in the source, so the issue may be
that LLVM on arm64 chooses not to inline due to the large static arrays.
Consider rewriting `from_sq()` and `to_sq()` to use bit manipulation
directly:

```rust
pub fn from_sq(self) -> Square {
    // Bit extraction without table lookup
    unsafe { core::mem::transmute(((self.0 >> 6) & 0x3f) as u8) }
}
```

Similarly for `Move::to_sq()` (transmute from u8 to Square requires
Square to be `#[repr(u8)]`).

**Reference:** Fairy-Stockfish uses C++ bit-fields and inline functions
that compile to single extract instructions.

**Effort:** ~15 lines, depends on `#[repr(u8)]` for `Square`.

---

### 7. Replace Vec with stack-array for move generation (estimated: 1–3 % speedup)

**Problem:** `generate_pseudo_legal()` writes into a `Vec<Move>`, which
incurs bounds-checked `.push()` and may reallocate. `generate_legal()`
uses `Vec::retain()` which creates a closure and may cause memmove.

**Fix:** Use a fixed-size array `[Move; MAX_MOVES]` with a length counter.
This eliminates allocation, bounds checks (with debug_assert), and
simplifies the filtering.

**Reference:** Fairy-Stockfish uses `ExtMove moveList[MAX_MOVES]` and
returns a pointer to the end.

**Effort:** ~30 lines. Changes `generate_pseudo_legal` and
`generate_legal` signatures.

---

### 8. Deduplicate attack computations in `legal()` (estimated: 1–3 % speedup)

**Problem:** The pseudo-royal check loop (lines 820–857) computes
`rook_attacks(ksq, occupied)`, `bishop_attacks(ksq, occupied)`, and
`queen_attacks(ksq, occupied)` for each commoner. Since
`queen_attacks = bishop_attacks | rook_attacks`, we can compute
`slider_attacks = rook_attacks | bishop_attacks` once and reuse:

```rust
let rook_atk = attacks::rook_attacks(ksq, occupied);
let bishop_atk = attacks::bishop_attacks(ksq, occupied);
let slider_atk = rook_atk | bishop_atk;
// rook_attackers = slider_atk & rooks;
// bishop_attackers = slider_atk & bishops;
// queen_attackers = slider_atk & queens;
```

**Effort:** ~5 lines.

---

### 9. `file_of()` and `rank_of()` table lookup optimization (estimated: 1–2 % speedup)

**Problem:** `file_of()` and `rank_of()` each go through a static array
(indexed by the square value). These are called from `generate_pawn_moves_for`
for every pawn push/capture — which is a hot path.

**Fix:** Use bit manipulation:
```rust
pub fn file_of(s: Square) -> File {
    unsafe { core::mem::transmute((s as u8 & 0b111) as u8) }
}
pub fn rank_of(s: Square) -> Rank {
    unsafe { core::mem::transmute(((s as u8 >> 3) & 0b111) as u8) }
}
```
Requires `#[repr(u8)]` on both `File` and `Rank`.

**Effort:** ~15 lines.

---

### 10. Use `static` instead of `LazyLock` for read-only tables (estimated: 0–1 % speedup)

**Problem:** Attack tables (king, knight, pawn) use `LazyLock<Vec<Bitboard>>`,
which has a once-flag check on every access. The king/move tables are
read-only hot data.

**Fix:** Initialize at compile time using `const` arrays where possible,
or use `static` with `LazyLock` but ensure the inline path is optimal.

**Note:** This is already optimal for king/knight/pawn attacks which
are simple static arrays. Verify that `LazyLock::force` is called at
startup to avoid runtime checks.

**Effort:** ~10 lines.

---

## Summary

| # | Optimization | Est. Impact | Effort | Risk |
|---|-------------|------------|--------|------|
| 1 | Precompute state in `do_move()` | 30–50 % | ~50 lines | Low |
| 2 | Early-out in `legal()` | 15–30 % | ~10 lines | Medium |
| 3 | Cache commoner bitboards in `legal()` | 5–10 % | ~5 lines | None |
| 4 | Eliminate runtime sliding dispatch | 3–8 % | ~20 lines | None |
| 5 | Precompute `between_bb` table | 2–5 % | ~15 lines | None |
| 6 | Inline hot accessor functions | 2–5 % | ~15 lines | Low |
| 7 | Stack-array for move generation | 1–3 % | ~30 lines | Low |
| 8 | Deduplicate slider attacks | 1–3 % | ~5 lines | None |
| 9 | Optimize `file_of`/`rank_of` | 1–2 % | ~15 lines | Low |
| 10 | Static-init hot tables | 0–1 % | ~10 lines | None |

### Recommended implementation order

1. **Items 1 + 2 together** — Precompute state and add early-out. These
   are interdependent and their combined effect is multiplicative.
2. **Item 3** — Trivial win while touching `legal()`.
3. **Item 5** — Precomputed `between_bb` table speeds up
   `blockers_for_king` computation used by item 1.
4. **Items 4, 6, 9** — Micro-optimizations to inline/debloat hot paths.
5. **Item 7** — Structural change to move generation.
6. **Item 8** — Minor deduplication in the hot loop.
7. **Item 10** — Cleanup.
