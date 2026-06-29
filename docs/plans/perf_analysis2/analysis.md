# Performance Analysis 2 — `atomic-movegen`

## Methodology

The binary was built with the `profiling` profile (inherits `release` with `debug =
2` and `strip = false`):

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
```

Profiling was done with `perf record` on Apple Firestorm (arm64) at 999 Hz sampling
rate with precise event sampling (`precise_ip = 3`):

```sh
perf record -F 999 -g --call-graph dwarf target/profiling/examples/perft \
  'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1' 6
```

Symbols were resolved via `addr2line` against the unstripped profiling binary.

**Test position:** Starting position, depth 6 (≈119M nodes).

---

## Profiling Results — CPU Hotspots

### Self-time breakdown (by function)

| Self CPU | Samples | Function | Source |
|----------|---------|----------|--------|
| 51.19 % | 134 | `perft()` | `lib.rs:37–57` |
| 33.38 % | 49 | `Board::legal()` | `board.rs:679–895` |
| 8.30 % | 35 | `Board::compute_checkers()` | `board.rs:340–383` |
| 2.86 % | 12 | `Board::compute_pinned()` | `board.rs:390–418` |
| 2.24 % | 23 | `cfree`/`malloc` | libc (Vec alloc/free) |
| 0.46 % | 3 | `??` (unresolved) | — |

### `legal()` hot-spot analysis (disassembly)

The hottest addresses within `legal()` all lie in the first ~70 bytes of the
function body (offsets 0x00–0x46 from function entry at 0xb82c):

| Offset | Samples | Instruction | Operation |
|--------|---------|-------------|-----------|
| 0x048 (0xb874) | 4.08 % | `ldrb` | Load `piece_on(to)` via piece-type table |
| 0x030 (0xb85c) | 3.81 % | `ldrb` | Load `piece_on(from)` from squares array |
| 0x0a8 (0xb8d4) | 3.34 % | `cmp`/`b.eq` | Check `pt != PieceType::Commoner` |
| 0x0c4 (0xb8f0) | 3.29 % | `cmp`/`cset` | Check `move_type != Castling` |
| 0x068 (0xb894) | 3.25 % | `ldr` | Load `state.checkers` (Bitboard load) |
| 0x0bc (0xb8e8) | 2.85 % | `ldr` | Load `state.commoners_count` |
| 0x02c (0xb858) | 1.83 % | `ldrb` | Piece-type table lookup in `piece_on(from)` |
| 0x028 (0xb854) | 0.65 % | `add` | Address computation for piece-type table |

**Key insight:** 23.2 % of all CPU time is spent in the first 70 bytes of
`legal()` — the prologue, `piece_on()` calls, and early-out condition checks.
The function is 4684 bytes total, but the backend (pseudo-royal attack loop,
castling checks) accounts for only ~10 % of `legal()`'s self-time because the
early-out (Plan 1) filters the majority of calls early.

### `perft()` self-time breakdown

The `perft()` function at 51.19 % self-time is the recursive driver. Its samples
come from:
- Loop overhead (iterating over move list, recursive calls)
- State management (`do_move`/`undo_move` call overhead)
- `generate_legal()` dispatch overhead
- Vec operations (push, retain, bounds checks)

---

## Current Performance Baseline

```
cargo run --release --example perft 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1' 6
= 118926425 nodes
```

Previous total for all 41 positions at depths 1–6 (Plan 1 baseline): **124.380 s**

---

## Comparison with Fairy-Stockfish Reference

### Architectural gaps still remaining

| Aspect | `atomic-movegen` (Rust) | Fairy-Stockfish (C++) | Impact |
|--------|------------------------|----------------------|--------|
| **Move storage** | `Vec<Move>` with heap allocation + bounds checks | `ExtMove[MAX_MOVES]` stack array with raw pointer writes | High — malloc/cfree is ~3.6 % |
| **Legal filtering** | `Vec::retain()` closure + per-move `legal()` call | Inline filter in `generate<LEGAL>` with pointer compaction | High — extra closure + call overhead |
| **`attackers_to()`** | Disjoint per-type: rook, bishop, queen, knight, pawn, king — each with separate bitboard ANDs | Single expression: `(pawn_attacks & pieces(PAWN)) | (attacks_bb<KNIGHT> & pieces(KNIGHT)) | ...` | Medium — 6 separate ANDs vs. 1 fused |
| **Sliding attack dispatch** | `match *IMPL` on arm64 resolves to magic path at compile time | `#ifdef` or template — zero-overhead at compile time | Low — already `#[cfg(not(x86_64))]` path exists |
| **`compute_pinned()`** | Loop over each sniper, `between_bb()` per pair. Uses `Bitboard::count() == 1` | `between_bb(s, sniperSq) & occupancy`, checks `!more_than_one(b)` | Medium — `count()` is slower than bit test |
| **`compute_checkers()`** | Dedicated function called from `populate_state()` | `set_check_info()` at end of `do_move()` — same approach | Already aligned |
| **Move accessors** | `from_sq()`/`to_sq()` use 64-element static arrays (table lookup) | C++ bit-fields — single shift + mask instruction | Low — already fast, but table lookups add cache pressure |
| **`Piece::type_of()`** | Static array lookup with bounds check | Bit field extraction | Low |
| **`file_of()`/`rank_of()`** | Static array lookup with modulo | Shift/mask inline | Low — 1–2 % |
| **Legal function structure** | Single huge function (4684 bytes on arm64) | Modular: variants dispatched via `var->extinctionPseudoRoyal`, templates | Medium — I-cache pressure |
| **`between_bb()`** | Loop-based with `make_square()` + 2 match statements | Precomputed `BetweenBB[]` table (tried and reverted in Plan 2) | Low — LazyLock overhead offset gains |
| **`do_move()` state copy** | Manual field-by-field copy of StateInfo | `std::memcpy` up to `offsetof(StateInfo, key)` for bulk copy | Low — Rust can't memcpy due to Drop/move semantics |

### Fairy-Stockfish key architectural advantages

1. **Pseudo-royal info is cached in StateInfo** — `st->pseudoRoyals` is computed
   once in `set_check_info()` and reused. The Rust code still recomputes
   `self.commoners(us)` and `self.commoners(them)` within `legal()` (around
   the self-explosion and pseudo-royal loop checks).

2. **Attackers-to is a single fused expression** — The C++ `attackers_to()`
   combines all piece types in one expression with template-parameterized
   `attacks_bb<pt>()` calls. The Rust `attackers_to()` and the inline checks
   in `legal()` call each attack type separately, reloading piece-type bitboards
   each time.

3. **Move generation uses template specialization** — Fairy-Stockfish generates
   legal moves via `generate<LEGAL>` which calls `generate<EVASIONS>` or
   `generate<NON_EVASIONS>` and filters inline. No closure, no Vec.

4. **`slider_blockers()` uses a more efficient algorithm** — It identifies
   snipers using pseudo-attacks (empty-board attacks), then checks if there's
   exactly one blocker between sniper and king. The Rust `compute_pinned()`
   loops over all snipers and calls `between_bb()` with `count() == 1`, which
   is slower.

---

## Potential Performance Improvements

Sorted by estimated impact (highest first).

### 1. Replace `Vec<Move>` with stack-allocated array (estimated: 8–15 % speedup)

**Problem:** `generate_pseudo_legal()` and `generate_legal()` use `Vec<Move>`,
which:
- Allocates on the heap (`malloc`/`free` — 3.6 % of samples)
- Bounds-checks every `.push()` (compiler cannot elide for Vec)
- `Vec::retain()` creates a closure and may shift memory
- The `for &m in &moves` loop in `perft()` iterates over a heap-backed slice

**Fix:** Implement a `MoveList` type backed by a fixed-size `[Move; MAX_MOVES]`
array with a length counter:

```rust
pub struct MoveList {
    moves: [Move; 256],
    len: usize,
}

impl MoveList {
    pub fn push(&mut self, m: Move) { ... }
    pub fn len(&self) -> usize { self.len }
    pub fn iter(&self) -> impl Iterator<Item = Move> { ... }
    pub fn clear(&mut self) { self.len = 0; }
    pub fn as_slice(&self) -> &[Move] { &self.moves[..self.len] }
    pub fn retain<F>(&mut self, f: F) { ... }
}
```

This eliminates heap allocation entirely. The `retain` method can compact
in-place without a closure by using a raw pointer loop.

**Reference:** Fairy-Stockfish `ExtMove moveList[MAX_MOVES]` + pointer return.

**Effort:** ~50 lines. Changes `generate_pseudo_legal`, `generate_legal`,
and `perft()` signatures.

**Risk:** Low. The maximum number of legal moves in atomic chess is bounded
by the number of squares (64) × piece types — 256 is safe.

---

### 2. Inline legal filtering into `generate_legal()` (estimated: 5–10 % speedup)

**Problem:** `generate_legal()` calls `Vec::retain()` with a closure that
invokes `board.legal(m, &state)` for every pseudo-legal move. This adds:
- Closure call overhead
- Per-move `legal()` function call overhead (23.2 % of samples are in the
  first 70 bytes of `legal()`)
- Redundant accessor calls (`from_sq()`, `to_sq()`, `move_type()`) inside
  `legal()` that duplicate work already done during generation

**Fix:** Fold the early-out check and legal verification into
`generate_legal()` so that trivially-safe moves are accepted without a
function call:

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let state = /* precomputed state */;
    generate_pseudo_legal(board, moves);

    // In-place filter
    let mut write_idx = 0;
    for read_idx in 0..moves.len() {
        let m = moves.moves[read_idx];
        if quick_legal_check(board, m, &state) {
            moves.moves[write_idx] = m;
            write_idx += 1;
        }
    }
    moves.len = write_idx;
}
```

Where `quick_legal_check()` is a `#[inline(always)]` version of the early-out
path that avoids the function call overhead for the fast path.

**Alternative:** Generate non-commoner, non-capture moves directly into a
separate "accepted" section of the array, skipping the legal check entirely
when checkers are empty and the piece is unpinned. Only non-trivial moves
(pins, captures, commoner moves, en-passant, castling) go through the full
`legal()` check.

**Reference:** Fairy-Stockfish's `generate<LEGAL>` generates into a flat array
and filters with pointer manipulation — no function call per move.

**Effort:** ~40 lines.

---

### 3. Deduplicate attacker computations in `legal()` (estimated: 3–8 % speedup)

**Problem:** The pseudo-royal attack loop (lines 865–879) computes 5 separate
attack bitboards per commoner:
- `rook_attacks(ksq, occupied) & by_type[Rook] & enemy_survivors`
- `bishop_attacks(ksq, occupied) & by_type[Bishop] & enemy_survivors`
- `queen_attacks(ksq, occupied) & by_type[Queen] & enemy_survivors`
- `knight_attacks(ksq) & by_type[Knight] & enemy_survivors`
- `pawn_attacks(us, ksq) & by_type[Pawn] & enemy_survivors`

Since `queen_attacks = bishop_attacks | rook_attacks`, we can compute both
slider types once and reuse:

```rust
let rook_atk = attacks::rook_attacks(ksq, occupied);
let bishop_atk = attacks::bishop_attacks(ksq, occupied);
let slider_atk = rook_atk | bishop_atk;

let rook_attackers = rook_atk & by_type[Rook] & enemy_survivors;
let bishop_attackers = bishop_atk & by_type[Bishop] & enemy_survivors;
let queen_attackers = slider_atk & by_type[Queen] & enemy_survivors;
```

Similarly, the castling pass-through check (lines 726–743) computes the same
5 attack types per square — and the `commoner_attackers` check can use the
already-computed `king_attacks(sq)`.

**Fix:** Refactor the attacker-check blocks to compute shared slider attacks
once. Extract a helper `attackers_to_filtered(sq, occupied, candidate_mask)`
that computes all attacker types in a single fused expression (like
Fairy-Stockfish's `attackers_to()`).

**Reference:** Fairy-Stockfish `Position::attackers_to()` lines 937–941.

**Effort:** ~20 lines.

---

### 4. Fuse `attackers_to()` into a single expression (estimated: 2–5 % speedup)

**Problem:** The current `Board::attackers_to()` (lines 304–337) computes each
attacker type separately, loading `by_type` arrays multiple times. This
duplicated pattern also appears inline in `legal()` (castling check and
pseudo-royal loop) and in `compute_checkers()`.

**Fix:** Rewrite `attackers_to()` as a single expression:

```rust
pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
    (attacks::pawn_attacks(Color::White, sq) & self.pieces_color_pt(Color::Black, PieceType::Pawn))
    | (attacks::pawn_attacks(Color::Black, sq) & self.pieces_color_pt(Color::White, PieceType::Pawn))
    | (attacks::knight_attacks(sq) & self.by_type[PieceType::Knight as usize])
    | (attacks::bishop_attacks(sq, occupied) & (self.by_type[PieceType::Bishop as usize] | self.by_type[PieceType::Queen as usize]))
    | (attacks::rook_attacks(sq, occupied) & (self.by_type[PieceType::Rook as usize] | self.by_type[PieceType::Queen as usize]))
    | (attacks::king_attacks(sq) & self.by_type[PieceType::Commoner as usize])
}
```

And use `attackers_to()` in the pseudo-royal loop, `compute_checkers()`, and
the castling check instead of repeating the per-type pattern. The compiler can
then CSE the shared slider attacks across the single expression.

**Effort:** ~15 lines.

---

### 5. Optimize `compute_pinned()` with two-pass algorithm (estimated: 2–5 % speedup)

**Problem:** `compute_pinned()` iterates over each commoner, then each sniper,
calling `between_bb()` per pair and checking `between.count() == 1`. This is
O(commoners × snipers). The Fairy-Stockfish `slider_blockers()` uses a
two-pass approach:
1. Find snipers using pseudo-attacks (empty-board attack masks)
2. Compute `between_bb(s, sniperSq) & occupancy` and check
   `!more_than_one(b)` (a single bit-test)

**Fix:** Use `Bitboard::is_single()` instead of `count() == 1`:

```rust
// Instead of:
let between = between_bb(ksq, sniper_sq) & occupied;
if between.count() == 1 { pinned = pinned | between; }

// Use:
let between = between_bb(ksq, sniper_sq) & occupied;
if (between & (between - 1)) == 0 && between != Bitboard::EMPTY {
    pinned = pinned | between;
}
```

Also pre-compute sniper masks using empty-board attacks to avoid the
queen-doubling:

```rust
let rook_snipers = attacks::rook_attacks(ksq, Bitboard::EMPTY)
    & (self.by_type[PieceType::Rook as usize] | self.by_type[PieceType::Queen as usize]);
let bishop_snipers = attacks::bishop_attacks(ksq, Bitboard::EMPTY)
    & (self.by_type[PieceType::Bishop as usize] | self.by_type[PieceType::Queen as usize]);
let snipers = (rook_snipers | bishop_snipers) & self.pieces_color(them);
```

**Effort:** ~10 lines.

---

### 6. Eliminate `Piece::type_of()` table lookup (estimated: 1–3 % speedup)

**Problem:** `Piece::type_of()` (lines 630–645) uses a static 6-element array
lookup with a bounds check. The encoding `Piece((color << 3) | (pt + 1))`
means `type_of = (self.0 & 7) - 1`, which maps to array index. The array is
only needed because `PieceType` is an enum and we need to convert the numeric
value.

**Fix:** Add `#[repr(u8)]` to `PieceType` and use `core::mem::transmute`:

```rust
#[repr(u8)]
pub enum PieceType {
    Pawn = 0, Knight = 1, Bishop = 2,
    Rook = 3, Queen = 4, Commoner = 5,
}

impl Piece {
    pub fn type_of(self) -> PieceType {
        // Safety: self.0 & 7 gives 1..=6 for valid pieces (0 = NO_PIECE).
        // Subtracting 1 gives 0..=5 which maps directly to PieceType.
        // NO_PIECE (0) maps to Pawn via wrapping_sub, but is caught by NO_PIECE check.
        unsafe { core::mem::transmute(((self.0 as u8) & 7).wrapping_sub(1)) }
    }
}
```

This replaces a load from a static array + bounds check with a single `and`
+ `sub` instruction.

**Similarly for `file_of()`/`rank_of()`:** Replace static array lookups with
bit manipulation:

```rust
pub fn file_of(s: Square) -> File {
    unsafe { core::mem::transmute((s as u8) & 7) }
}

pub fn rank_of(s: Square) -> Rank {
    unsafe { core::mem::transmute(((s as u8) >> 3) & 7) }
}
```

Requires `#[repr(u8)]` on `File` and `Rank` (they are simple fieldless enums
with sequential discriminants, so `#[repr(u8)]` is safe).

**Similarly for `Move::from_sq()`/`to_sq()`:** Replace 64-element static arrays
with `transmute` from `u8`:

```rust
pub fn from_sq(self) -> Square {
    unsafe { core::mem::transmute(((self.0 >> 6) & 0x3f) as u8) }
}
```

Requires `#[repr(u8)]` on `Square`. Note: `Square::NONE` (value 64) must not
be reachable from valid moves; `from_sq` and `to_sq` only produce values
0–63 by construction.

**Effort:** ~25 lines across `types.rs`.

---

### 7. Add `fn is_one(&self) -> bool` to Bitboard (estimated: 1–2 % speedup)

**Problem:** The pattern `x.count() == 1` appears in `compute_pinned()` and
the pseudo-royal check. `Bitboard::count()` on arm64 uses the intrinsic
`__builtin_popcountll` which is a single instruction, but `count()` returns
`u32`, and comparing with `== 1` requires the comparison.

**Fix:** Add an `is_one()` method:

```rust
pub fn is_one(self) -> bool {
    self.0 != 0 && (self.0 & (self.0 - 1)) == 0
}
```

This is often faster than `count() == 1` because it avoids the popcount
instruction (which can have latency on some microarchitectures) in favor of
a simple bit-twiddle. On arm64 Firestorm, popcount is fast (single cycle),
but the branch pattern can be better with the bit-twiddle.

**Effort:** ~5 lines.

---

### 8. Precompute `captured_count` upper bound to avoid dynamic StateInfo arrays (estimated: 1–2 % speedup)

**Problem:** `StateInfo.captured` is a fixed-size `[(Square, Piece); 9]` array
to hold blast victims. The maximum blast zone is king attacks (8 squares) + 1
(ground zero) = 9, so 9 is the safe upper bound. However, the code iterates
with `state.captured_count` which is incremented per blast victim. The
`while i > 0 { i -= 1; ... }` loop in `undo_move` can be replaced with
a fixed upper bound if the count is known.

**Low impact** — only affects `undo_move`, which is not a hot path (only
called during perft search tree traversal).

**Effort:** ~5 lines.

---

### 9. Reduce `legal()` function size via splitting (estimated: 1–2 % speedup)

**Problem:** The `legal()` function is 4684 bytes on arm64, which is large
for a hot function. This causes:
- I-cache pressure
- The compiler may not inline as aggressively
- The prologue saves/restores 10 callee-saved registers (x19–x28, x29, x30)

**Fix:** Split into specialized helper functions:
- `legal_commoner_move()` — handles both capture and non-capture commoner moves
- `legal_castling()` — the castling pass-through and destination check
- `legal_capture()` — handles blast, self-explosion, pseudo-royal attacks
- `legal_non_capture()` — primarily the early-out + simple checks

Each helper is smaller and can be better optimized by the compiler.

**Effort:** ~50 lines.

---

### 10. Inline `Bitboard::pop_lsb()` into loop structure (estimated: 0–2 % speedup)

**Problem:** The `while !c.is_empty() { let ksq = c.pop_lsb(); ... }` pattern
has two calls per iteration: `is_empty()` (a `cmp x0, #0`) and `pop_lsb()`
(which uses `Bitboard::lsb()` calling the `__builtin_ctzll` intrinsic, then
clears the bit).

**Fix:** Use `Bitboard::next()` or iterator pattern:

```rust
impl Bitboard {
    pub fn iter(self) -> BitboardIter {
        BitboardIter(self)
    }
}

pub struct BitboardIter(Bitboard);

impl Iterator for BitboardIter {
    type Item = Square;
    fn next(&mut self) -> Option<Square> {
        if self.0 .0 == 0 { None } else {
            let sq = self.0.lsb();
            self.0 .0 &= self.0 .0 - 1;
            Some(sq)
        }
    }
}
```

The iterator pattern is idiomatic and enables the compiler to use loop
optimizations. However, in practice, the compiler already does a good job
with the `while + pop_lsb` pattern, so this is low impact.

**Effort:** ~15 lines.

---

### 11. Cache `by_type` and `by_color` references in hot functions (estimated: 0–1 % speedup)

**Problem:** Hot functions like `legal()`, `compute_checkers()`, and
`compute_pinned()` repeatedly dereference `self.by_type[pt as usize]` and
`self.pieces_color(c)`. Each access is an array load from memory, and while
the compiler can CSE some of these within basic blocks, it cannot always
hoist them out of loops.

**Fix:** Load the frequently-used arrays once:

```rust
let by_type = &self.by_type;
let by_color = &self.by_color;
let them_bb = by_color[them as usize];
```

**Note:** Plan 2 tried this for `commoners()` and it showed no benefit (the
compiler already CSEs these). However, caching the full `by_type` and
`by_color` slices may help in functions where multiple piece types are
queried in a loop.

**Effort:** ~5 lines.

---

## Summary

| # | Optimization | Est. Impact | Effort | Risk | Note |
|---|-------------|------------|--------|------|------|
| 1 | Stack-allocated `MoveList` | 8–15 % | ~50 lines | Low | Replaces `Vec<Move>`, eliminates malloc/free |
| 2 | Inline legal filtering | 5–10 % | ~40 lines | Low | Skips `legal()` call for fast-path moves |
| 3 | Deduplicate slider attacks in `legal()` | 3–8 % | ~20 lines | None | Shared `rook_atk | bishop_atk` |
| 4 | Fused `attackers_to()` | 2–5 % | ~15 lines | None | Single expression, better CSE |
| 5 | Optimize `compute_pinned()` | 2–5 % | ~10 lines | None | Bit-twiddle `is_one()`, precomputed snipers |
| 6 | Bit-manipulation in accessors | 1–3 % | ~25 lines | Low | Replace table lookups with `transmute` |
| 7 | `Bitboard::is_one()` method | 1–2 % | ~5 lines | None | Avoids popcount in hot path |
| 8 | Precompute `captured_count` bound | 1–2 % | ~5 lines | None | Minor undo_move optimization |
| 9 | Split `legal()` into helpers | 1–2 % | ~50 lines | Low | Better I-cache, smaller prologue |
| 10 | Bitboard iterator pattern | 0–2 % | ~15 lines | None | More idiomatic, minor optimization |
| 11 | Cache `by_type`/`by_color` refs | 0–1 % | ~5 lines | None | Compiler likely already handles this |

### Recommended implementation order

1. **Item 1** — Stack-allocated `MoveList`: highest impact, foundation for Item 2.
2. **Item 2** — Inline legal filtering: builds on Item 1, eliminates `legal()` call overhead.
3. **Items 3 + 4 together** — Fused `attackers_to()` + deduplication in `legal()`: the two are
   interdependent and maximize CSE benefit.
4. **Item 5** — Optimize `compute_pinned()`: smaller change, measurable benefit.
5. **Item 6** — Bit-manipulation accessors: mechanical change, enables `#[repr(u8)]` enums.
6. **Items 7, 10** — Bitboard helper methods: small additive improvements.
7. **Items 9, 11** — Structural cleanup: `legal()` splitting, caching.

### Items already completed (Plan 1, previous work)

- ✅ Early-out in `legal()` for safe moves
- ✅ Cached `checkers`, `pinned`, `commoners_count` in `StateInfo`
- ✅ `populate_state()` at end of `do_move()`
- ✅ Pawn checker detection fix

### Items attempted and reverted (Plan 2)

- ❌ Manual caching of `commoners()` calls — compiler already CSEs
- ❌ Precomputed `between_bb` table — `LazyLock` overhead offset gains

---

## Lessons from Previous Attempts

1. **Profile first, measure after.** Plan 2's estimated 7–15 % turned into 0 %
   because the compiler already optimized the "hot" code paths.

2. **Don't add cost to the early-out path.** The early-out in `legal()` fires
   for the majority of calls (non-capture, non-commoner moves). Any code added
   before the early-out is paid on every invocation. Always place caching after
   the early-out.

3. **`LazyLock` has non-zero cost.** For hot paths called millions of times per
   second, the atomic load + branch of `LazyLock::deref` adds up. Prefer
   compile-time initialization where possible.

4. **The compiler sees through simple accessors.** `self.commoners(c)` is two
   array loads and an AND — the compiler CSEs repeated calls automatically.
   Manual caching adds register pressure.
