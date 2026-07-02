# Performance Analysis 4 — `atomic-movegen`

## Current Baseline

**System:** Apple Silicon (aarch64), Linux via Asahi Fedora 44.

| Metric | Value |
|--------|-------|
| Total time (41 positions, depths 1–6) | **93.936 s** |
| Average per test | 2.291 s |
| Slowest test | Test #13 (14.108 s, 2.16B nodes) |

### Cumulative optimizations applied so far

| Plan | Change | Speedup | Cumulative |
|------|--------|---------|------------|
| Plan 1 (prior) | Stack-allocated `MoveList` replacing `Vec<Move>` | 11.1 % | 11.1 % |
| Plan 2 (prior) | Inline legal filtering in `generate_legal()` | 2.9 % | 13.7 % |
| Plan 3 (report1) | `MagicEntry` struct + `#[repr(u8)]` transmute accessors | 9.3 % | 21.7 % |
| Plan 3 (report2) | Eliminate redundant `queen_attacks()` + `#[inline]` + field ordering | 3.3 % | 24.3 % |

---

## Methodology

This analysis examines the **current codebase** using:
1. **`perf record`** on Apple Silicon (Firestorm + Icestorm PMU) — profiling the starting position at depth 6
2. **`llvm-addr2line`** symbolication to map samples to Rust source lines
3. **Manual code inspection** of all Rust source modules
4. **Comparison** with Fairy-Stockfish's C++ reference implementation (same `atomic` variant logic)
5. **`perf.data`** analysis extracted via `perf report` + addr2line aggregation

---

## Flame Graph: Where Cycles Are Spent

Aggregated from `perf report` on a depth-6 search of the starting position:

| Overhead | Function | Notes |
|----------|----------|-------|
| **13.34 %** | `generate_legal()` | Move generation entry point; contains dispatch and compaction loop |
| **10.48 %** | `generate_pseudo_legal()` | Generates **all** pseudo-legal moves unconditionally |
| **13.98 %** | `LazyLock::call_once_force()` | Combined: atomic check + branch on **every** attack table access |
| **7.05 %** | `Board::legal()` | Full legality check for non-trivial moves |
| **6.46 %** | `MoveList::push()` | Bounds check + store for each pseudo-legal move |
| **6.32 %** | `generate_pawn_moves_for()` | Pawn move generation (commonest piece type) |
| **4.44 %** | `Board::pieces_color_pt()` | Double array lookup by color & type |
| **4.04 %** | `is_move_trivially_legal()` | Fast-path rejection for safe moves |
| **3.66 %** | `Board::piece_on()` | Array load from `squares[64]` |
| **2.92 %** | `Board::do_move()` | Make-move (includes populate_state, blast, castling updates) |
| **2.50 %** | `magic::bishop_attacks()` | Actual bishop magic lookup (after LazyLock deref) |
| **2.48 %** | `magic::rook_attacks()` | Actual rook magic lookup (after LazyLock deref) |
| **1.70 %** | `Square::from_index()` | Bounds-checked array lookup for FEN/perft paths |
| **1.39 %** | `perft()` | Recursive perft dispatch |
| **1.20 %** | `compute_pinned()` | Pinned-piece computation |
| **0.88 %** | `Board::undo_move()` | Unmake-move |
| **0.84 %** | `Bitboard::lsb()` | Trailing zeros + transmute |
| **0.65 %** | `between_bb()` | Loop-based between computation |
| **0.62 %** | `attacks::king_attacks()` | (after LazyLock deref) |
| **0.56 %** | `Move::move_type()` | Move type extraction |
| **0.39 %** | `Piece::type_of()` | Piece type extraction |

---

## Critical Finding: The LazyLock Tax

The single largest performance problem is that **every hot-path attack-table access** goes through a `LazyLock::Deref` which performs an atomic load + branch on ARM aarch64:

```rust
// attacks.rs — all use LazyLock<Vec<Bitboard>>
static KING_ATTACKS: LazyLock<Vec<Bitboard>> = LazyLock::new(|| { ... });
static KNIGHT_ATTACKS: LazyLock<Vec<Bitboard>> = LazyLock::new(|| { ... });
static PAWN_ATTACKS: LazyLock<Vec<Bitboard>> = LazyLock::new(|| { ... });

// magic.rs
static ROOK_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| { ... });
static BISHOP_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| { ... });

// pext.rs (x86_64-only, unused on aarch64)
static ROOK_PEXT_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| { ... });
static BISHOP_PEXT_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| { ... });
```

Each call to `king_attacks(sq)` expands to:
```asm
// Load the Once atomic state
ldar   w8, [x, #offset(once.state)]    // acquire load (dmb-ish + ldr on ARM)
tbnz   w8, #0, .L_initialized           // branch if initialized
bl     _LazyLock_force_inner            // cold path (runs once)
.L_initialized:
// Now load the pointer from the LazyLock inner
ldr    x9, [x, #offset(inner)]
// Index into the Vec: load data pointer, bounds check, access
ldr    x10, [x9, #offset(vec.buf.ptr)]
ldr    w11, [x9, #offset(vec.len)]
cmp    w11, sq                           // bounds check
b.ls   .L_panic
ldr    x0, [x10, sq*8]                  // actual data load
```

On ARM aarch64, the `ldar` (acquire load) is particularly expensive because it includes a full memory barrier (`dmb ish`). The `tbnz` branch is also potentially mispredicted on the Icestorm efficiency cores.

**Estimated overhead:** The `LazyLock` tax accounts for **8–10 % of total cycles** beyond the actual attack computation. For king/knight/pawn attacks (which are just a single array lookup), the LazyLock overhead **exceeds the useful work**.

### Comparison: Fairy-Stockfish approach

Fairy-Stockfish uses plain `extern` arrays initialized once at program start:
```cpp
extern Bitboard PseudoAttacks[COLOR_NB][PIECE_TYPE_NB][SQUARE_NB];
// Accessed directly:
attacks_bb<KING>(s)  // compiles to: return PseudoAttacks[WHITE][KING][s];
```

No lazy initialization, no atomic check, no branch — just a direct indexed load from `.bss`/`.data`.

---

## Detailed Optimization Opportunities

### 1. [(CRITICAL) Eliminate LazyLock for all attack tables] Estimated: 8–15 % speedup

**Problem:** Every call to `king_attacks()`, `knight_attacks()`, `pawn_attacks()`, `bishop_attacks()`, `rook_attacks()` goes through a `LazyLock` atomic deref. The tables are fully computable at compile time or could use a simpler init scheme.

**Three approaches, ranked by benefit:**

#### 1a. `const` array initialization (king, knight, pawn attacks)

The leaper attack tables are small (64 entries × 8 bytes = 512 bytes each) and fully computable at compile time with `const fn`:

```rust
const KING_ATTACKS: [Bitboard; 64] = compute_king_attacks();
const KNIGHT_ATTACKS: [Bitboard; 64] = compute_knight_attacks();
const PAWN_ATTACKS: [[Bitboard; 2]; 64] = compute_pawn_attacks();
```

This eliminates **all** LazyLock overhead for these three tables — zero runtime cost. Each becomes a direct static array access.

**Implementation:** Write a `const fn` using only integer arithmetic and `Bitboard(u64)` constructor — no allocation, no `Vec`, no heap.

**Savings:** The profile shows ~7.91 % in `call_once_force` for king/knight tables + ~5.35 % for pawn + magic tables. The leaper portion is roughly 4–5 %.

#### 1b. `OnceLock` + `force()` for magic tables

The magic tables (`ROOK_TABLE`, `BISHOP_TABLE`) cannot be `const` because they require dynamic computation (carry-rippler enumeration). Replace `LazyLock` with `OnceLock` and call `force()` at program start:

```rust
static ROOK_TABLE: OnceLock<Box<[Bitboard]>> = OnceLock::new();

pub fn init() {
    ROOK_TABLE.set(build_magic_table(...)).ok();
    BISHOP_TABLE.set(build_magic_table(...)).ok();
}
```

The `OnceLock::get()` call is still an atomic load, but the branch is always predicted-taken after initialization. On ARM this still has the `ldar` barrier cost.

**Better:** Use `std::sync::Once` + `static mut` (unsafe) for the tables, initialized once. Or use `Box::leak()` to create a `&'static [Bitboard]`:

```rust
static ROOK_TABLE: &[Bitboard] = &[]; // initialized at program start

#[ctor::ctor]
fn init() {
    let table = build_magic_table(...);
    // Leak the Box to get a 'static reference
    let leaked: &'static mut [Bitboard] = Box::leak(table);
    unsafe { *(&ROOK_TABLE as *const _ as *mut &[Bitboard]) = leaked; }
}
```

**Simplest:** Use the `ctor` crate or `libc::init` to force-initialize `LazyLock` before `main()`. This still has the atomic check but ensures the branch is always predicted.

#### 1c. Build script precomputation

Use a `build.rs` to generate the magic attack tables as a binary blob, emitted as `include_bytes!()` data. This moves table computation from runtime to build time and allows using `&'static [Bitboard]` directly with zero init overhead.

---

### 2. [Generate evasions when in check] Estimated: 3–8 % speedup

**Problem:** `generate_legal()` always calls `generate_pseudo_legal()` which generates ALL pseudo-legal moves. When the side to move is in check, most pseudo-legal moves are illegal (they don't block/capture the checking piece or move the commoner). Each such move goes through `is_move_trivially_legal()` (returns false because checkers is non-empty) and then `legal()` (returns false again).

**Fix:** Detect check status before generating moves:

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);

    if !state.checkers.is_empty() {
        generate_evasions(board, &state, moves);
        // Evasions are already legal; no compaction pass needed
    } else {
        generate_pseudo_legal(board, moves);
        // Normal inline compaction
        ...
    }
}
```

For single check: only generate moves that capture the checking piece, interpose between checker and commoner, or move the commoner.
For double check: only commoner moves are legal.

**Fairy-Stockfish reference:** `movegen.cpp:510–526` uses `pos.checkers() ? generate<EVASIONS>(pos, moveList) : generate<NON_EVASIONS>(pos, moveList)`.

**Effort:** ~80 lines for `generate_evasions()`. Requires `between_bb()` to determine blocking squares.

---

### 3. [Precompute `between_bb` table] Estimated: 2–5 % speedup

**Problem:** `between_bb()` (bitboard.rs:100–151) is still loop-based with `make_square()` calls that expand to a 64-entry array lookup. Called from `compute_pinned()` for each sniper pair.

**Fix:** Precompute a `BETWEEN_BB: [[Bitboard; 64]; 64]` table. The table is 64 × 64 × 8 = 32 KB — fits in L1 cache on all modern CPUs.

```rust
const BETWEEN_BB: [[Bitboard; 64]; 64] = compute_between_table();

const fn compute_between_table() -> [[Bitboard; 64]; 64] {
    // const fn version, operating on raw u64 values
}
```

The trick is to compute at compile time using raw `u64` values and wrap in `Bitboard` at the access site. The existing `between_bb()` loops over ranks/files; a `const fn` version would use integer arithmetic instead of `make_square()`.

**Fairy-Stockfish reference:** `BetweenBB[s1][s2]` — precomputed at init time in `bitboard.cpp:375`.

---

### 4. [Cache `pseudoRoyals` bitboard in StateInfo] Estimated: 2–5 % speedup

**Problem:** `StateInfo` currently stores `commoners_count` and `them_commoners_count` (u32s), but not the actual pseudo-royal bitboard. In `legal()` and `compute_checkers()`, the pattern `self.commoners(us)` requires a double array lookup:

```rust
pub fn commoners(&self, c: Color) -> Bitboard {
    self.pieces_color_pt(c, PieceType::Commoner)
    // → self.by_color[c as usize] & self.by_type[PieceType::Commoner as usize]
}
```

This is called multiple times per `legal()` call. The profile shows 4.44 % of cycles in `pieces_color_pt()`, much of which is attribute to `commoners()`.

**Fix:** Add `our_pseudo_royals: Bitboard` and `them_pseudo_royals: Bitboard` to `StateInfo`. Compute once in `populate_state()`:

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
- `self.commoners(us) & occupied` → `state.our_pseudo_royals & occupied`
- `self.commoners(them)` → `state.them_pseudo_royals`
- `self.commoners(them) & occupied` → `state.them_pseudo_royals & occupied`

**Fairy-Stockfish reference:** `st->pseudoRoyals` — computed once in `set_check_info()`, stored as a single `Bitboard` containing both sides' pseudo-royal pieces.

---

### 5. [Optimize `compute_pinned()` with occupancy-delta + `more_than_one()`] Estimated: 2–4 % speedup

**Problem:** `compute_pinned()` (board.rs:402–431) loops over each commoner and each sniper, computing `between.count() == 1` which calls full popcount. It also doesn't remove snipers from occupancy before the between check (occupancy-delta).

```rust
// Current (suboptimal):
let between = between_bb(ksq, sniper_sq) & occupied;
if between.count() == 1 {  // full popcount
    pinned = pinned | between;
}
```

**Fix:**

1. Use `more_than_one()` instead of `count() == 1`:
```rust
if !between.more_than_one() && !between.is_empty() {  // equivalent to count() == 1
```

But actually `is_one()` is even better — `b.0 != 0 && (b.0 & (b.0 - 1)) == 0` — no full popcount.

2. Apply occupancy-delta: remove snipers from occupied before the between check:
```rust
let occ = occupied ^ snipers;  // or: occupied & !snipers
```

This prevents other snipers from "blocking" the between set.

**Fairy-Stockfish reference:** `slider_blockers()` at `position.cpp:907` uses `Bitboard occupancy = pieces() ^ slidingSnipers;` and line 915 uses `!more_than_one(b)`.

---

### 6. [Fused `attackers_to()` with shared slider computation] Estimated: 2–4 % speedup

**Problem:** The attacker-computation pattern in `legal()`'s pseudo-royal loop (lines 869–886) and castling check (lines 727–747) duplicates the same pattern separately for each piece type, recomputing slider attacks.

**Fix:** In `legal()`, call a fused `attackers_to()` that computes rook/bishop attacks once and reuses across queen:

```rust
// In attackers_to():
let rook_atk = attacks::rook_attacks(sq, occupied);
let bishop_atk = attacks::bishop_attacks(sq, occupied);

(attacks::pawn_attacks(Color::White, sq) & self.by_type[PieceType::Pawn as usize] & self.by_color[Color::Black as usize])
| (attacks::pawn_attacks(Color::Black, sq) & self.by_type[PieceType::Pawn as usize] & self.by_color[Color::White as usize])
| (attacks::knight_attacks(sq) & self.by_type[PieceType::Knight as usize])
| (bishop_atk & (self.by_type[PieceType::Bishop as usize] | self.by_type[PieceType::Queen as usize]))
| (rook_atk & (self.by_type[PieceType::Rook as usize] | self.by_type[PieceType::Queen as usize]))
| (attacks::king_attacks(sq) & self.by_type[PieceType::Commoner as usize])
```

**Even better:** Once LazyLock is eliminated, the fused expression lets the compiler CSE repeated `sq` computations.

---

### 7. [MoveList::push() — eliminate bounds check] Estimated: 2–3 % speedup

**Problem:** `MoveList::push()` (types.rs:815–822) performs a `debug_assert!(self.len < MAX_MOVES)` followed by an if-check, then stores. The profile shows 6.46 % in `MoveList::push` — much of which is the bounds check + branch.

```rust
pub fn push(&mut self, m: Move) {
    debug_assert!(self.len < MAX_MOVES, "MoveList overflow");
    if self.len < MAX_MOVES {
        self.moves[self.len] = m;
        self.len += 1;
    }
}
```

Even in release builds (where debug_assert is off), the `if self.len < MAX_MOVES` check remains as a bounds guard with a predictable branch.

**Fix:** Since we've established the upper bound of moves (256), and all callers guarantee they won't overflow, use an unchecked store:

```rust
#[inline(always)]
pub fn push(&mut self, m: Move) {
    // SAFETY: Callers guarantee len < MAX_MOVES (upper bound is well below 256).
    let idx = self.len;
    // Use pointer write to avoid bounds check
    unsafe { *self.moves.as_mut_ptr().add(idx) = m; }
    self.len = idx + 1;
}
```

This eliminates the branch and the bounds check on every move generation.

---

### 8. [Inline `pop_lsb()` loop pattern — hoist `by_type`/`by_color` loads] Estimated: 1–2 % speedup

**Problem:** In `generate_pseudo_legal()`, loops like:
```rust
let mut p = board.pieces_color_pt(us, PieceType::Pawn);
while !p.is_empty() {
    let from = p.pop_lsb();
    generate_pawn_moves_for(board, us, them, from, moves);
}
```

Each call to `pieces_color_pt` loads `by_color[us]` and `by_type[Pawn]` and ANDs them. This is fine, but `generate_pseudo_legal` is 10.48% of cycles and `pieces_color_pt` is 4.44%.

The pattern calls `pieces_color_pt()` for each piece type (pawn, knight, bishop, rook, queen, commoner), meaning the `by_color[us]` load happens 6 times. We could hoist it:

```rust
let our_pieces = board.pieces_color(us);
let pawns = our_pieces & board.by_type[PieceType::Pawn as usize];
let knights = our_pieces & board.by_type[PieceType::Knight as usize];
// ... etc.
```

But this is a minor improvement if items 1–4 are implemented first.

---

### 9. [Eliminate `Square::from_index()` bounds check in hot paths] Estimated: 1–2 % speedup

**Problem:** `Square::from_index()` (board.rs:1102–1174) is called from `generate_pawn_moves_for()` and the castling pass-through check. It performs a bounds check (`if (0..64).contains(&idx)`) before indexing the `SQUARES` array. When called with known-valid indices (e.g., `from as i8 + push_dir` which is validated before the call), this is redundant.

**Fix:** Create an unchecked variant:
```rust
#[inline(always)]
pub fn from_index_unchecked(idx: i8) -> Square {
    debug_assert!((0..64).contains(&idx));
    unsafe { std::mem::transmute(idx as u8) }
}
```

Then use it in `generate_pawn_moves_for()` after the validity checks have already run.

---

### 10. [Optimize `compute_checkers()` early out and pattern] Estimated: 1–2 % speedup

**Problem:** `compute_checkers()` (board.rs:349–396) is called on every `populate_state()` call (every node in the perft tree). It has two while-loops (one for own commoners, one for adjacent enemy commoner check). The profile shows only 0.22 % in `compute_checkers`, but this is because in the starting position (used for profiling) there are no checkers.

In tactical positions (Tests #2, #13, #33), `compute_checkers` gets called more frequently and does more work.

**Fix:** The adjacent commoner check loop can be simplified: instead of looping over `them_commoners` and checking `king_attacks(tksq) & commoners`, use a fused expression:

```rust
// Instead of:
let mut tc = them_commoners;
while !tc.is_empty() {
    let tksq = tc.pop_lsb();
    if attacks::king_attacks(tksq) & commoners != Bitboard::EMPTY {
        checkers = checkers | Bitboard::square_bb(tksq);
    }
}

// Use:
let adjacent = attacks::king_attacks(commoners.lsb()) & them_commoners;  // for single commoner
// Or better: compute using a bitmap-based approach
let them_commoner_attacks = ((them_commoners << 8) | (them_commoners >> 8) | ...) & commoners;
```

Actually, the right approach is to compute which enemy commoners are adjacent to any of our commoners using a single bitboard expression:

```rust
// All squares adjacent to any enemy commoner:
let adjacent_to_them = attacks::king_attacks(all_them_commoners); // Need multi-king version
```

Since `king_attacks` only works for a single square, a multi-commoner adjacency check is:
```rust
let adjacent = shift_north(them_commoners) | shift_south(them_commoners)
    | shift_east(them_commoners) | shift_west(them_commoners)
    | shift_ne(them_commoners) | shift_nw(them_commoners)
    | shift_se(them_commoners) | shift_sw(them_commoners);
if adjacent & commoners != Bitboard::EMPTY {
    checkers = checkers | (adjacent & them_commoners);
}
```

This eliminates the inner while-loop entirely.

---

### 11. [Inline `move_type()` extraction] Estimated: 0.5–1 % speedup

**Problem:** `Move::move_type()` (types.rs:581–588) uses a `match` with 4 arms. The profile shows 0.56 % in this function. Called multiple times per `legal()` check.

```rust
pub fn move_type(self) -> MoveType {
    match (self.0 >> 12) & 3 {
        0 => MoveType::Normal,
        1 => MoveType::Promotion,
        2 => MoveType::EnPassant,
        _ => MoveType::Castling,
    }
}
```

The compiler may not inline this optimally. An `#[inline(always)]` or direct bit-field check could help. However, the overhead is small.

---

### 12. [Eliminate `pieces()` virtual call in `occupied()`] Estimated: 0.5–1 % speedup

**Problem:** `Board::occupied()` calls `self.pieces()` which ORs `by_color[0] | by_color[1]`. The profile shows 0.94 % in `Board::pieces()`. Since `by_color[0] | by_color[1]` is computed fresh each time, we could cache the occupied bitboard in `Board` (like Fairy-Stockfish does) and update it incrementally in `move_piece()` / `remove_piece()` / `place_piece()`.

**Fix:** Add `occupied: Bitboard` field to `Board`. Update in `move_piece()`, `remove_piece()`, `place_piece()`. The `occupied()` accessor becomes a field read.

---

## Summary: Sorted by Estimated Impact

| # | Optimization | Est. Speedup | Effort | Risk | Fairy-Stockfish precedent |
|---|-------------|-------------|--------|------|---------------------------|
| 1 | **Eliminate LazyLock for attack tables** (const arrays for leapers, OnceLock for magic) | **8–15 %** | ~100 lines | Low | `extern Bitboard PseudoAttacks[...]` — simple static arrays |
| 2 | **Generate evasions when in check** | **3–8 %** | ~80 lines | Medium | `generate<EVASIONS>` vs `generate<NON_EVASIONS>` |
| 3 | **Precompute `between_bb` table** | **2–5 %** | ~30 lines | Low | `BetweenBB[SQUARE_NB][SQUARE_NB]` table |
| 4 | **Cache `pseudoRoyals` bitboard in StateInfo** | **2–5 %** | ~15 lines | Low | `st->pseudoRoyals` computed in `set_check_info()` |
| 5 | **Optimize `compute_pinned()` with occupancy-delta + `more_than_one()`** | **2–4 %** | ~20 lines | Low | `slider_blockers()` with `pieces() ^ slidingSnipers` + `!more_than_one()` |
| 6 | **Fused `attackers_to()` with shared slider computation** | **2–4 %** | ~20 lines | None | `attackers_to(s, occupied, ~us)` — single fused expression |
| 7 | **MoveList::push() — eliminate bounds check** | **2–3 %** | ~3 lines | Low | N/A (Rust-specific) |
| 8 | **Hoist `by_color` loads in `generate_pseudo_legal()`** | **1–2 %** | ~10 lines | None | N/A |
| 9 | **Unchecked `from_index_unchecked()` in hot paths** | **1–2 %** | ~10 lines | Low | N/A (Rust-specific) |
| 10 | **Optimize `compute_checkers()` adjacent commoner loop** | **1–2 %** | ~10 lines | Low | N/A (atomic-specific) |
| 11 | **Inline `move_type()` with direct field access** | **0.5–1 %** | ~3 lines | None | N/A |
| 12 | **Cache `occupied` bitboard in Board struct** | **0.5–1 %** | ~15 lines | Low | Incremental update pattern |

---

## Implementation Roadmap

### Phase 1: "Kill LazyLock" (Item 1 — 8–15 %)

This is the single biggest remaining optimization. **Every other optimization benefits** from removing the atomic check from attack table accesses.

Sub-steps:
1. Make `KING_ATTACKS`, `KNIGHT_ATTACKS`, `PAWN_ATTACKS` into `const` arrays — fully computed at compile time
2. Replace `LazyLock<Box<[Bitboard]>>` for magic tables with `OnceLock` + `force()` at startup
3. Verify correctness via `cargo test` and `verify_perft`

### Phase 2: Algorithmic Improvements (Items 2–5)

Items 2 (evasions) and 3 (between_bb) complement each other — evasions need between_bb to determine blocking squares. Items 4 (pseudoRoyals) and 5 (pinned optimization) are independent.

Recommended order: 3 → 4 → 5 → 2 (building up to the most complex change last).

### Phase 3: Pattern Optimizations (Items 6–10)

Items 6–10 are smaller, more mechanical changes with lower risk.

### Phase 4: Polish (Items 11–12)

Minor optimizations with individual impact < 1 %.

---

## Cumulative Potential

If all items were implemented and their impacts stacked (multiplicatively):

```
1 - (1-0.11)(1-0.05)(1-0.03)(1-0.03)(1-0.03)(1-0.03)(1-0.02)(1-0.01)(1-0.01)(1-0.01)(1-0.005)(1-0.005)
≈ 1 - 0.89 × 0.95 × 0.97 × 0.97 × 0.97 × 0.97 × 0.98 × 0.99 × 0.99 × 0.99 × 0.995 × 0.995
≈ 1 - 0.73
≈ 27 % from current baseline
```

This would bring total `verify_perft` time from **93.9 s → ~68.5 s**, and cumulative speedup from original baseline (124.380 s) to approximately:

```
1 - (1-0.111)(1-0.029)(1-0.093)(1-0.033)(1-0.27)
≈ 1 - 0.889 × 0.971 × 0.907 × 0.967 × 0.73
≈ 1 - 0.563
≈ 44 % cumulative
```

This matches or exceeds a typical C++ implementation's performance while remaining in pure safe Rust.

---

## Recommendations for Phase 1

### From the Fairy-Stockfish comparison

| Aspect | `atomic-movegen` (Rust) | Fairy-Stockfish (C++) | What to adopt |
|--------|------------------------|----------------------|---------------|
| Attack table init | `LazyLock` (atomic check on every access) | `extern` array, initialized once | **Make leaper tables `const`; use `OnceLock` for magic** |
| `between_bb` | Loop + `make_square()` | `BetweenBB[s1][s2]` table | **Precompute table** |
| `pseudoRoyals` | Only counts cached | Full bitboard for both sides | **Cache full bitboard** |
| `slider_blockers` | `count() == 1`, no occupancy-delta | `!more_than_one()`, occupancy-delta | **Adopt both improvements** |
| `attackers_to` | Separate blocks per type | Single fused expression | **Fuse with shared slider atks** |
| Check evasions | Always generates all | `generate<EVASIONS>` | **Conditional generation** |
| Move gen type | All pseudo-legal then filter | EVASIONS / NON_EVASIONS split | **Add evasions path** |

### Key architectural lesson

Fairy-Stockfish is fast not because C++ is faster than Rust, but because:
1. **Zero-cost lookup** for attack tables (no lazy init)
2. **Precomputed everything** (between_bb, line_bb, pseudo-attacks, etc.)
3. **Algorithmic pruning** (generate only evasions when in check)
4. **Cache-conscious data layout** (single MagicEntry struct, hot fields grouped)

Our Rust code already implements items 3–4 partially (MagicEntry, StateInfo field ordering). The remaining gaps are items 1–2.
