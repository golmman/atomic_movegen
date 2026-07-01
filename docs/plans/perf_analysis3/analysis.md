# Performance Analysis 3 — `atomic-movegen`

## Current Baseline

**System:** Linux on x86_64 (Docker), CPU unknown.

| Metric | Value |
|--------|-------|
| Total time (41 positions, depths 1–6) | **107.305 s** |
| Average per test | 2.617 s |
| Starting position depth 6 | 118,926,425 nodes in ~1.84 s |
| Slowest test | Test #13 (16.093 s, 2.16B nodes) |

### Previous optimizations applied (✓)

| Plan | Change | Speedup | Cumulative |
|------|--------|---------|------------|
| Plan 1 | Stack-allocated `MoveList` replacing `Vec<Move>` | 11.1 % | 11.1 % |
| Plan 2 | Inline legal filtering in `generate_legal()` | 2.9 % | 13.7 % |

### Cumulative speedup from original baseline

Original baseline (pre-MoveList): **124.380 s** → Current: **107.305 s** = **13.7 %**

---

## Methodology

This analysis examines the **current codebase** (with Plans 1 & 2 applied) against the
**Fairy-Stockfish reference implementation** (C++17). We identify remaining architectural
gaps and novel optimization opportunities that were not considered in Plans 1–2.

Sources consulted:
- Fairy-Stockfish `src/position.cpp` (legal, attackers_to, set_check_info, slider_blockers, do_move)
- Fairy-Stockfish `src/movegen.cpp` (generate<LEGAL>, generate_all)
- Fairy-Stockfish `src/bitboard.h` (magic table layout, between_bb, attacks_bb)
- Previous analysis documents in `docs/plans/perf_analysis/` and `docs/plans/perf_analysis2/`
- Previous perf iteration reports in `docs/perf/iter/`

---

## Recurring Theme: The `legal()` Function is Still the Hotspot

Even after Plan 2's inline filter, the `legal()` function is called for all non-trivial
moves (captures, commoner moves, castling, en-passant, pinned pieces, checks). For deep
tactical positions (Test #2, #13, #33), the inline filter catches fewer moves because
most moves are captures or involve commoners, meaning `legal()` is called for a large
fraction of the move tree.

The `legal()` function contains three duplicated attacker-computation blocks:

| Block | Lines | What it does | Cost |
|-------|-------|-------------|------|
| Castling pass-through | 702–744 | For each of 2 squares, computes 6 attacker types separately | 12 magic lookups total |
| Pseudo-royal attack loop | 842–879 | For each commoner, computes 5 attacker types separately | 5+ magic lookups per commoner |
| (shared) | 854–856, 860–862 | Uses `queen_attacks()` which internally calls `bishop_attacks()` + `rook_attacks()` redundantly with the adjacent direct calls | ~2× magic work |

Each magic lookup (rook/bishop) involves: 2 loads (mask, magic), 1 multiply, 1 shift,
1 table load, 1 AND with piece-type bitboard, 1 AND with color bitboard. When duplicated
3× per attacker check, this adds up quickly.

---

## Comparison: Remaining Architectural Gaps

| # | Aspect | `atomic-movegen` (Rust) | Fairy-Stockfish (C++) | Impact |
|---|--------|------------------------|----------------------|--------|
| A | **Magic data layout** | 5 separate arrays: `MASKS[]`, `MAGICS[]`, `INDEX_BITS[]`, `OFFSETS[]`, `TABLE[]` | Single `Magic` struct per square: `{mask, magic, attacks*, shift}` → `m.index(occ)` | High — 5 cache lines vs 1 |
| B | **`attackers_to()` expression** | 6 separate `|= ... & ...` blocks, each reloading `by_type[]` and `by_color[]` | Single fused expression: `(pawn_atk & pieces(PAWN)) | (knight_atk & pieces(KNIGHT)) | ...` | Medium |
| C | **Sliding attack reuse** | Each caller computes rook/bishop/queen attacks independently | Compiler CSEs shared `rider_attacks_bb` calls within the fused expression | High |
| D | **`between_bb()` implementation** | Loop-based with 2 `match` statements per iteration | `BetweenBB[s1][s2]` — single table lookup | Medium |
| E | **`state.pseudoRoyals` caching** | Only stores `commoners_count` and `them_commoners_count` (u32) | Stores full `pseudoRoyals` bitboard for BOTH sides | High |
| F | **Move generation in check** | Always generates all pseudo-legal moves, then filters | Generates `EVASIONS` vs `NON_EVASIONS` based on check status | Medium |
| G | **`compute_pinned()` algorithm** | Loops per commoner → per sniper → `between_bb()` + `count() == 1` | `slider_blockers()` with occupancy-delta: removes snipers from occupancy, `between_bb()` + `!more_than_one(b)` | Medium |
| H | **Bitboard-to-square conversion** | Static 64-element array lookup for `lsb()`, `msb()`, `from_sq()`, `to_sq()` | Truncated enum + `transmute` or direct cast | Low–Medium |
| I | **`Piece::type_of()` dispatch** | `(self.0 & 7).wrapping_sub(1)` + bounds check + table lookup | Bit field extraction + `transmute`-equivalent | Low |
| J | **StateInfo field layout** | Arbitrary ordering; hot fields (checkers, pinned) interleaved with cold fields | Hot fields grouped at front; `memcpy` bulk-copies up to `offsetof(key)` | Low |

---

## Novel Optimization Opportunities

### 1. Co-locate magic data into struct-of-arrays → array-of-structs (estimated: 5–12 %)

**Problem:** The magic bitboard lookup currently loads from 5 separate arrays per call:

```rust
let mask = BISHOP_MASKS[sq_idx];           // load #1
let idx = ((occupied & mask).0.wrapping_mul(BISHOP_MAGICS[sq_idx]))  // load #2
    >> (64 - BISHOP_INDEX_BITS[sq_idx]);   // load #3
let offset = BISHOP_OFFSETS[sq_idx];       // load #4
BISHOP_TABLE[offset + idx as usize]        // load #5 (table data)
```

These arrays are scattered across the binary's data section, potentially in different
cache lines. The multiply and shift are fast, but the scattered loads create
instruction-level dependencies and cache pressure.

**Fix:** Define a single struct per square and a single flat array:

```rust
pub struct MagicEntry {
    pub mask: Bitboard,
    pub magic: u64,
    pub shift: u32,
    pub offset: u32,   // smaller than usize = smaller struct
}

pub(crate) const BISHOP_ENTRIES: [MagicEntry; 64] = [ /* compile-time constants */ ];

#[inline(always)]
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let e = &BISHOP_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    BISHOP_TABLE[e.offset as usize + idx as usize]
}
```

This reduces 4 separate loads into 1 struct load (plus the table data load). The struct
is 24 bytes (Bitboard(8) + u64(8) + u32(4) + u32(4) = 24), fitting in one cache line
with room to spare for the adjacent entry.

**Effort:** ~20 lines per table (rook + bishop = 40 lines total). All the data is already
`const`; the new struct just needs to be defined and the arrays transformed.

**Challenge:** Rust's const fn evaluation must be able to compute `offset` values at
compile time. The `compute_offsets()` function is already `const fn`, so this should work.

**Reference:** Fairy-Stockfish `Magic` struct at `bitboard.h:122–145`.

---

### 2. Eliminate redundant `queen_attacks()` magic lookups (estimated: 3–8 %)

**Problem:** In `legal()`'s pseudo-royal loop and castling check, the pattern:

```rust
let rook_attackers = attacks::rook_attacks(ksq, occupied) & by_type[Rook] & enemy_survivors;
let bishop_attackers = attacks::bishop_attacks(ksq, occupied) & by_type[Bishop] & enemy_survivors;
let queen_attackers = attacks::queen_attacks(ksq, occupied) & by_type[Queen] & enemy_survivors;
```

`queen_attacks()` is defined as `bishop_attacks(sq, occ) | rook_attacks(sq, occ)`.
This means `bishop_attacks()` and `rook_attacks()` are called **twice** — once directly
and once inside `queen_attacks()`. That's 6 magic lookups instead of 3, plus 2
unnecessary OR operations.

**Fix:** Compute slider attacks once and reuse:

```rust
let rook_atk = attacks::rook_attacks(ksq, occupied);
let bishop_atk = attacks::bishop_attacks(ksq, occupied);

let rook_attackers = rook_atk & by_type[Rook] & enemy_survivors;
let bishop_attackers = bishop_atk & by_type[Bishop] & enemy_survivors;
let queen_attackers = (rook_atk | bishop_atk) & by_type[Queen] & enemy_survivors;
```

This eliminates 3 magic lookups per attacker check (from 6 to 3). For the pseudo-royal
loop (1 commoner × 1 attacker check), that's 3 magic lookups saved per `legal()` call.
For the castling check (2 squares × 1 attacker check), that's 6 magic lookups saved.

Combined with item 1, each saved magic lookup is: 1 struct load + 1 AND + 1 multiply +
1 shift + 1 table load = relatively cheap but not zero.

**Effort:** ~10 lines. Both the castling block (lines 715–732) and pseudo-royal loop
(lines 854–866) need the same transformation.

**Precedent:** This was identified in the original `perf_analysis/analysis.md` (item 8)
but never implemented because Plans 1 and 2 took priority.

---

### 3. Cache `pseudoRoyals` bitboard in `StateInfo` (estimated: 3–6 %)

**Problem:** `StateInfo` currently stores `commoners_count: u32` and
`them_commoners_count: u32` — just the counts. In `legal()`, the pseudo-royal check
must recompute `self.commoners(us)` and `self.commoners(them)` from bitboard ANDs,
and then check `self.commoners(us) & occupied` for the self-explosion test.

**Fix:** Add `our_pseudo_royals: Bitboard` and `them_pseudo_royals: Bitboard` to
`StateInfo`. Compute them once in `populate_state()`:

```rust
pub fn populate_state(&self, state: &mut StateInfo) {
    state.checkers = self.compute_checkers(self.side_to_move);
    state.pinned = self.compute_pinned(self.side_to_move);
    let our_commoners = self.commoners(self.side_to_move);
    let them_commoners = self.commoners(self.side_to_move.flip());
    state.commoners_count = our_commoners.count();
    state.them_commoners_count = them_commoners.count();
    state.our_pseudo_royals = our_commoners;     // NEW
    state.them_pseudo_royals = them_commoners;    // NEW
}
```

Then in `legal()`, replace:
- `self.commoners(us) & occupied` → `state.our_pseudo_royals & occupied` (1 AND instead of 3 ops)
- `self.commoners(them)` → `state.them_pseudo_royals` (0 ops instead of 3)
- `self.commoners(them) & occupied` → `state.them_pseudo_royals & occupied`

**Savings per `legal()` call:** 2 bitboard ANDs + 2 `by_color[]` loads + 2
`by_type[]` loads. The bitboard AND is cheap (1 cycle), but the memory loads to
`by_color[us]`, `by_color[them]`, `by_type[Commoner]` add latency (3 cache accesses).

**Effort:** ~15 lines in `board.rs`.

**Reference:** Fairy-Stockfish `set_check_info()` at `position.cpp:600–613` computes
`st->pseudoRoyals` which includes both sides' pseudo-royal pieces.

---

### 4. Precomputed `between_bb` table (estimated: 3–6 %)

**Problem:** `between_bb()` in `bitboard.rs:92–143` is a loop-based computation with
`make_square()` calls, each requiring 2 `match` statements (one for file, one for rank).
This is called from `compute_pinned()` for every sniper pair.

**Fix:** Precompute a `BETWEEN_BB: [[Bitboard; 64]; 64]` table. The table is
64 × 64 × 8 = 32,768 bytes, which fits in L1 cache on all modern CPUs.

Use `const` initialization to avoid `LazyLock` overhead:

```rust
const BETWEEN_BB: [[Bitboard; 64]; 64] = compute_between_table();

const fn compute_between_table() -> [[Bitboard; 64]; 64] {
    // const fn version of the loop-based computation
    // Must be able to compute Bitboard values at compile time
}
```

**Challenge:** Rust's `const fn` cannot call `match` on enums with non-integer
discriminants in all contexts, and cannot easily construct `Square` enum values from
indices. However, we can compute the underlying `u64` values at compile time and wrap
them in `Bitboard`:

```rust
const fn compute_between_bb(s1: u8, s2: u8) -> u64 { ... }
const fn compute_between_table() -> [[u64; 64]; 64] { ... }
// Then wrap in Bitboard at access time or use a union/transmute
```

**Alternative:** Use `LazyLock` but force initialization at program start by calling
`LazyLock::force()` once. This adds a one-time init cost but eliminates the atomic
check on each access (after the first call, `LazyLock` is "hot" and the check is a
predictable branch). This was tried in Plan 2 and showed no benefit, likely because
the hot path (compute_pinned) called the function rarely enough that the LazyLock
cost wasn't dominant.

**Better alternative:** Initialize with `std::sync::OnceLock` + manual init, or simply
use `static` with `unsafe` mutable initialization at program start. The simplest safe
approach:

```rust
use std::sync::LazyLock;
static BETWEEN_BB: LazyLock<Box<[[Bitboard; 64]; 64]>> = LazyLock::new(|| {
    Box::new(compute_between_table_rt())
});
```

And ensure `LazyLock::force()` is called during binary startup so the once-flag check
is always predicted-taken.

**Effort:** ~30 lines.

---

### 5. Fused `attackers_to()` with shared slider attacks (estimated: 2–5 %)

**Problem:** `Board::attackers_to()` currently computes each attacker type in
separate blocks (lines 304–337), reloading `by_type[]` arrays each time. The
same pattern is repeated inline in `legal()`'s castling check and pseudo-royal loop.

**Fix:** Rewrite `attackers_to()` as a single fused expression that computes shared
slider attacks once:

```rust
pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
    let rook_atk = attacks::rook_attacks(sq, occupied);
    let bishop_atk = attacks::bishop_attacks(sq, occupied);
    let slider_atk = rook_atk | bishop_atk;

    (attacks::pawn_attacks(Color::White, sq) & self.by_type[PieceType::Pawn as usize] & self.by_color[Color::Black as usize])
    | (attacks::pawn_attacks(Color::Black, sq) & self.by_type[PieceType::Pawn as usize] & self.by_color[Color::White as usize])
    | (attacks::knight_attacks(sq) & self.by_type[PieceType::Knight as usize])
    | (bishop_atk & (self.by_type[PieceType::Bishop as usize] | self.by_type[PieceType::Queen as usize]))
    | (rook_atk & (self.by_type[PieceType::Rook as usize] | self.by_type[PieceType::Queen as usize]))
    | (slider_atk & self.by_type[PieceType::Queen as usize])
    | (attacks::king_attacks(sq) & self.by_type[PieceType::Commoner as usize])
}
```

Then use `attackers_to()` in `legal()`'s pseudo-royal loop and castling check
instead of the inline per-type pattern. This eliminates the duplicated code and
lets the compiler CSE the `rook_atk` and `bishop_atk` across callers.

**Effort:** ~20 lines. Touch `attackers_to()`, the castling block, and the
pseudo-royal loop in `legal()`.

**Reference:** Fairy-Stockfish `attackers_to()` at `position.cpp:932–1004`.

---

### 6. Generate evasions when in check (estimated: 2–5 %)

**Problem:** `generate_legal()` always calls `generate_pseudo_legal()` which
generates ALL pseudo-legal moves. When the side to move is in check, many of
these moves cannot possibly be legal (they don't block the check, capture the
checking piece, or move the commoner). Each of these moves then goes through the
`is_move_trivially_legal()` check (which returns `false` because checkers is
non-empty) and then `legal()` (which returns `false`), wasting time.

**Fix:** In `generate_legal()`, check if `state.checkers` is non-empty. If so,
only generate evasions:

1. If single check: generate only moves that capture the checking piece, block
   the check (interpose between checker and commoner), or move the commoner.
2. If double check: only commoner moves are legal.

This requires implementing a `generate_evasions()` function that generates a much
smaller set of pseudo-legal moves when in check.

**Pseudo-code:**

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);

    if !state.checkers.is_empty() {
        generate_evasions(board, &state, moves);
        // Evasions are already filtered; no legal() call needed
    } else {
        generate_pseudo_legal(board, moves);
        // Normal inline filter
        ...
    }
}
```

**How Fairy-Stockfish does it** (`movegen.cpp:509–526`):
```cpp
moveList = pos.checkers()
    ? generate<EVASIONS>(pos, moveList)
    : generate<NON_EVASIONS>(pos, moveList);
while (cur != moveList)
    if (!pos.legal(*cur))
        *cur = (--moveList)->move;
    else
        ++cur;
```

**Effort:** ~80 lines for `generate_evasions()`. Needs to understand single vs
double check, blocking squares between checker and king.

**Risk:** Correctness-critical (check evasion logic). Must be verified against
all 41 perft positions.

---

### 7. Optimize `compute_pinned()` with occupancy-delta and `more_than_one()` (estimated: 2–4 %)

**Problem:** `compute_pinned()` currently loops over each commoner, then each sniper,
and checks `between.count() == 1`. This has two inefficiencies:

1. **`count() == 1`** calls `popcount` (a full popcount instruction) when only a
   "is exactly one bit set" check is needed. `(b & (b - 1)) == 0 && b != 0` is faster
   (no popcount).
2. **No occupancy-delta:** When multiple snipers share the same ray, the between-set
   includes the other snipers as "blockers", which can produce incorrect results.
   Fairy-Stockfish removes snipers from occupancy before the between check.

**Fix:**

```rust
pub(crate) fn compute_pinned(&self, us: Color) -> Bitboard {
    let mut pinned = Bitboard::EMPTY;
    let them = us.flip();
    let occupied = self.occupied();
    let mut commoners = self.commoners(us);

    while !commoners.is_empty() {
        let ksq = commoners.pop_lsb();

        // Find snipers using empty-board attacks (pseudo-attacks)
        let rook_snipers = attacks::rook_attacks(ksq, Bitboard::EMPTY)
            & (self.by_type[PieceType::Rook as usize] | self.by_type[PieceType::Queen as usize])
            & self.pieces_color(them);
        let bishop_snipers = attacks::bishop_attacks(ksq, Bitboard::EMPTY)
            & (self.by_type[PieceType::Bishop as usize] | self.by_type[PieceType::Queen as usize])
            & self.pieces_color(them);
        let snipers = rook_snipers | bishop_snipers;

        // Remove snipers from occupancy to avoid self-blocking (occupancy-delta)
        let occ = occupied ^ snipers;

        let mut s = snipers;
        while !s.is_empty() {
            let sniper_sq = s.pop_lsb();
            let between = between_bb(ksq, sniper_sq) & occ;
            // Use is_one() instead of count() == 1
            if between.is_one() {
                pinned = pinned | between;
            }
        }
    }
    pinned
}
```

Also add `is_one()` to `Bitboard`:

```rust
impl Bitboard {
    #[inline(always)]
    pub fn is_one(self) -> bool {
        self.0 != 0 && (self.0 & (self.0 - 1)) == 0
    }
}
```

**Effort:** ~20 lines for the function rewrite, ~3 lines for `is_one()`.

**Reference:** Fairy-Stockfish `slider_blockers()` at `position.cpp:849–926`,
especially lines 907 (occupancy-delta: `Bitboard occupancy = pieces() ^ slidingSnipers;`)
and line 915 (`!more_than_one(b)` instead of `count() == 1`).

---

### 8. Bit-manipulation accessors via `#[repr(u8)]` transmute (estimated: 1–4 %)

**Problem:** The following accessors use static 64/6-element table lookups instead
of simple bit manipulation:

| Function | Current approach | Replacement |
|----------|-----------------|-------------|
| `Piece::type_of()` | `(self.0 & 7).wrapping_sub(1)` + bounds check + table lookup | `transmute(((self.0 as u8) & 7).wrapping_sub(1))` |
| `Bitboard::lsb()` | `trailing_zeros()` + 64-element `SQUARES[]` table lookup | `transmute(self.0.trailing_zeros() as u8)` |
| `Bitboard::msb()` | `63 - leading_zeros()` + 64-element `SQUARES[]` table lookup | `transmute((63 - self.0.leading_zeros()) as u8)` |
| `file_of()` | `idx % 8` + 8-element `FILES[]` table lookup | `transmute((s as u8) & 7)` |
| `rank_of()` | `idx / 8` + 8-element `RANKS[]` table lookup | `transmute(((s as u8) >> 3) & 7)` |
| `Move::from_sq()` | Shift + mask + 64-element `SQUARES[]` table lookup | `transmute(((self.0 >> 6) & 0x3f) as u8)` |
| `Move::to_sq()` | Mask + 64-element `SQUARES[]` table lookup | `transmute((self.0 & 0x3f) as u8)` |

**Requirements:** Add `#[repr(u8)]` to `Square`, `PieceType`, `File`, `Rank`.

Since `Square` has 65 variants (A1..H8 + NONE = 65), `#[repr(u8)]` is valid (256
possible values). The range 0–64 covers all valid squares plus NONE=64. The
`from_sq()`/`to_sq()` accessors only produce values 0–63 for valid moves, so
`transmute` is safe.

**Safety justification:** The `transmute` in each case maps a known-range integer
to an enum with a valid discriminant. Bounds checks are replaced by exclusive-range
guarantees at the call site (e.g., `debug_assert!(idx < 64)` in `lsb()`).

**Effort:** ~30 lines across `types.rs`. Requires adding `#[repr(u8)]` to 4 enums
and replacing 7 accessor bodies.

**Performance impact:** Each table lookup is a load from a static array. On x86_64,
a load from a static address is ~4–5 cycles for L1 hit, plus the array base address
must be materialized (RIP-relative lea). Replacing with `transmute` eliminates the
load entirely, replacing it with a single `and`/`shr`/`sub` ALU instruction.

---

### 9. Magic table initialization as `static` instead of `LazyLock` (estimated: 1–3 %)

**Problem:** The magic tables `ROOK_TABLE` and `BISHOP_TABLE` are initialized via
`LazyLock<Box<[Bitboard]>>`. While the atomic check is cheap after initialization,
it still adds a branch that must be predicted correctly. On x86_64, `LazyLock`
compiles to:

```asm
mov    rax, QWORD PTR [rip + ROOK_TABLE+8]  ; load atomic state
test   al, 3                                  ; check initialized?
jne    .L_initialized                         ; branch (usually taken)
call   init_fn                                ; cold path
.L_initialized:
```

This is ~3 instructions (load, test, branch) on every call to `rook_attacks()` or
`bishop_attacks()`. With millions of calls per second, these add up.

**Fix:** Use `static` with manual initialization in `main()`/`init()`, or better,
make the tables `Box<[Bitboard]>` stored in a `static` that is initialized at
compile time using `const` evaluation if possible.

**Alternative:** The tables could be `&'static [Bitboard]` by embedding the data
directly in the binary's `.rodata` section using `include_bytes!` or by generating
the tables with a build script.

**Simplest fix:** Ensure `LazyLock::force()` is called once at startup, so the
branch is always predicted-taken. But this is already happening — the first call
forces initialization.

**More effective fix:** Combine with item 1 (co-located magic data). If the tables
are smaller and accessed through a single struct per square, the number of
`LazyLock` checks drops from 2 per lookup to... well, still 2 (rook + bishop).

**Long-term fix:** Generate the tables at compile time and embed them. Rust's const
fn can compute the magic table entries if the `sliding_attack()` reference function
is made const.

**Effort:** ~50 lines for build-script-based precomputation.

---

### 10. Reduce `StateInfo` memory footprint (estimated: 0.5–2 %)

**Problem:** `StateInfo` has several fields that are only written and never read
during the hot `perft()` loop:

- `state.captured` — `[(Square, Piece); 9]` = 18 bytes, only read in `undo_move()`
- `state.cap_sq`, `state.cap_piece` — only read in `undo_move()`
- `state.castling_rights`, `state.ep_square`, `state.rule50` — only read in `undo_move()`

These cold fields are interleaved with hot fields (checkers, pinned, commoners_count),
causing the hot fields to be in different cache lines than they would be if co-located.

**Fix:** Reorder `StateInfo` to group hot fields at the beginning:

```rust
pub struct StateInfo {
    // Hot fields (read in legal() and populate_state())
    pub checkers: Bitboard,
    pub pinned: Bitboard,
    pub commoners_count: u32,
    pub them_commoners_count: u32,
    pub our_pseudo_royals: Bitboard,     // NEW (item 3)
    pub them_pseudo_royals: Bitboard,    // NEW (item 3)

    // Warm fields (read in do_move/undo_move perft loop)
    pub castling_rights: u8,
    pub ep_square: Option<Square>,
    pub rule50: u8,
    pub captured_count: u8,
    pub captured: [(Square, Piece); 9],
    pub cap_sq: Option<Square>,
    pub cap_piece: Piece,
}
```

This ensures the hot path touches fewer cache lines per `legal()` call.

**Effort:** ~5 lines (field reordering only).

---

### 11. Inline hot bitboard accessors explicitly (estimated: 0.5–1.5 %)

**Problem:** Several hot accessor functions lack explicit `#[inline]` annotations:

- `Board::piece_on()` (line 260)
- `Board::pieces_color()` (line 272)
- `Board::pieces_pt()` (line 276)
- `Board::pieces_color_pt()` (line 280)
- `Board::occupied()` (line 300)
- `Board::commoners()` (line 296)

While the compiler often inlines these automatically, explicit `#[inline(always)]`
ensures consistent behavior across optimization levels and helps with cross-crate
inlining.

**Effort:** ~6 lines of annotations.

---

## Summary: Sorted by Estimated Impact

| # | Optimization | Est. Speedup | Effort | Risk | Dependencies |
|---|-------------|-------------|--------|------|-------------|
| 1 | **Co-locate magic data** (array-of-structs) | 5–12 % | ~40 lines | Low | None |
| 2 | **Eliminate redundant `queen_attacks()` magic lookups** | 3–8 % | ~10 lines | None | None |
| 3 | **Cache `pseudoRoyals` bitboard in `StateInfo`** | 3–6 % | ~15 lines | Low | None |
| 4 | **Precomputed `between_bb` table** | 3–6 % | ~30 lines | Low (repeat) | None |
| 5 | **Fused `attackers_to()` with shared sliders** | 2–5 % | ~20 lines | None | Items 1, 2 (synergistic) |
| 6 | **Generate evasions when in check** | 2–5 % | ~80 lines | Medium | None |
| 7 | **Optimize `compute_pinned()` with occupancy-delta** | 2–4 % | ~20 lines | Low | Item 4 (for between_bb) |
| 8 | **Bit-manipulation accessors via `#[repr(u8)]` transmute** | 1–4 % | ~30 lines | Low | None |
| 9 | **Eliminate `LazyLock` for magic tables** | 1–3 % | ~50 lines | Low | Item 1 (can combine) |
| 10 | **`StateInfo` field reordering for cache** | 0.5–2 % | ~5 lines | None | Item 3 (field additions) |
| 11 | **Explicit `#[inline(always)]` on hot accessors** | 0.5–1.5 % | ~6 lines | None | None |

### Cumulative potential

If all items were implemented and their impacts stacked multiplicatively:

```
1 - (1-0.08)(1-0.05)(1-0.04)(1-0.04)(1-0.03)(1-0.03)(1-0.03)(1-0.02)(1-0.02)(1-0.01)(1-0.01)
≈ 1 - 0.92×0.95×0.96×0.96×0.97×0.97×0.97×0.98×0.98×0.99×0.99
≈ 1 - 0.68
≈ 32 % total speedup from current baseline
```

This would bring total `verify_perft` time from **107.3 s → ~73 s**, and starting
position depth 6 from ~1.84 s → ~1.25 s.

In reality, impacts are not additive (Amdahl's Law applies), but the high-impact
items (1–4) alone could deliver 10–20 % speedup.

---

## Recommended Implementation Order

### Phase 1: High-impact, low-risk (items 1–4)

1. **Item 8** — Bit-manipulation accessors (`#[repr(u8)]` transmute). Mechanical
   change with no correctness risk; enables the struct shrink in item 1.
2. **Item 1** — Co-locate magic data. Eliminates scattered loads, reduces latency
   per magic lookup. Combined with item 8, this is the single biggest gain.
3. **Item 3** — Cache `pseudoRoyals` in `StateInfo`. Adds new fields, wires them
   into `populate_state()` and `legal()`. Straightforward and safe.
4. **Item 2** — Eliminate redundant `queen_attacks()` lookups. After item 1, every
   saved magic lookup is pure profit. ~10 line change.

### Phase 2: Medium impact (items 5–7)

5. **Item 5** — Fused `attackers_to()`. After items 1–3, this consolidates the
   attacker computation pattern and enables further CSE.
6. **Item 7** — Optimize `compute_pinned()`. After the `between_bb` table (item 4)
   is available, this is simpler and safer.
7. **Item 6** — Generate evasions when in check. Largest effort, highest risk,
   but opens the door for the biggest remaining gain.

### Phase 3: Polish (items 9–11)

8. **Item 9** — Eliminate `LazyLock` for tables. Requires either build script or
   manual precomputation.
9. **Item 10** — `StateInfo` field reordering.
10. **Item 11** — Explicit `#[inline]` annotations.

---

## Items Already Completed (Plans 1–2)

| Plan | Change | Speedup |
|------|--------|---------|
| ✅ Plan 1 | Stack-allocated `MoveList` replacing `Vec<Move>` | 11.1 % |
| ✅ Plan 2 | Inline legal filtering in `generate_legal()` | 2.9 % |
| ✅ Early-out | Fast-path in `legal()` for safe moves | (part of Plan 2) |
| ✅ State caching | `checkers`, `pinned`, `commoners_count` in `StateInfo` | (enabled Plan 2) |

## Items Attempted and Reverted

| Item | Why reverted |
|------|-------------|
| Precomputed `between_bb` table | `LazyLock` overhead offset gains (Plan 2). Re-evaluating in item 4 with better table layout. |
| Manual caching of `commoners()` calls | Compiler already CSE'd these. The `pseudoRoyals` bitboard caching (item 3) goes further. |

---

## Lessons Learned

1. **Magic data layout matters.** The Fairy-Stockfish approach of a single `Magic`
   struct per square is more cache-friendly than 5 parallel arrays. Rust's const
   evaluation can support this.

2. **Fairy-Stockfish's `legal()` is not inherently faster** for atomic chess — the
   algorithm is essentially the same. The difference is in data layout, caching,
   and eliminating redundant work.

3. **The Rust compiler is good but not magical.** It will CSE within a basic block
   but cannot merge `queen_attacks()` + `bishop_attacks()` + `rook_attacks()` into
   2 calls when they are separate function invocations.

4. **Every magic lookup saved in `legal()` compounds.** The remaining `legal()`
   calls (after inline filter) are for the most complex positions, which dominate
   total runtime.

5. **Precomputed tables are worth the complexity** if they replace loop-based
   computation in hot paths. The key is using `const` or `static` initialization
   rather than `LazyLock`, which adds per-access overhead.

6. **Evasions generation is the next frontier.** Fairy-Stockfish generates only
   evasions when in check, reducing the pseudo-legal move count by 50–80 % in
   check positions. This is the single biggest algorithmic improvement still
   available.
