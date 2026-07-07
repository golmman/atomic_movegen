# Performance Analysis 5 — `atomic-movegen`

## Current Baseline

**System:** AMD Ryzen 5950X, 4 vCPUs (Docker), x86_64.

| Metric | Value |
|--------|-------|
| Total time (41 positions, depths 1–6) | **75.554 s** |
| Average per test | 1.843 s |
| Slowest test | Test #13 (11.565 s, 2.16B nodes) |

### Cumulative optimizations applied so far

| Plan | Change | Speedup | Cumulative |
|------|--------|---------|------------|
| Plan 1 (prior) | Stack-allocated `MoveList` replacing `Vec<Move>` | 11.1 % | 11.1 % |
| Plan 2 (prior) | Inline legal filtering in `generate_legal()` | 2.9 % | 13.7 % |
| Plan 3 (prior) | `MagicEntry` struct + `#[repr(u8)]` transmute accessors | 9.3 % | 21.7 % |
| Plan 3 (prior) | Eliminate redundant `queen_attacks()` + `#[inline]` + field ordering | 3.3 % | 24.3 % |
| Plan 4 (report1) | LazyLock → `const` arrays (leapers) + `OnceLock` (magic/PEXT) | 6.3 % | 29.1 % |
| Cleanup 3–4 | Precomputed `BETWEEN_BB`/`LINE_BB`, PEXT dispatch, AtomicU8 | ~7.5 % | 34.5 % |

Original baseline (perf_analysis1): ~124.4 s. Current: **75.554 s** → **39.3 % cumulative speedup**.

---

## Methodology

1. **`perf record`** on x86_64 AMD Ryzen 5950X — profiling starting position at depth 6
2. **`addr2line`** symbolication to map hot addresses to source lines
3. **Manual code inspection** of all 8 Rust source modules
4. **Comparison** with Fairy-Stockfish's C++ reference implementation (accessed via `Fairy-Stockfish/src/`)
5. **Fairy-Stockfish patterns identified**: movegen.cpp (EVASIONS split), position.cpp (slider_blockers, attackers_to, set_check_info), bitboard.cpp (BETWEEN_BB, magic init, bulk pawn shift)

---

## Flame Graph: Where Cycles Are Spent

Aggregated from `perf report` on a depth-6 search of the starting position (x86_64, profiling build):

| Overhead | Function / Module | Context |
|----------|-------------------|---------|
| **4.07 %** | `generate_pseudo_legal` dispatch (board.rs:479+) | Move generation entry — 6 per-type loops |
| **3.00 %** | `compute_pinned` inner loop (board.rs:479–508) | Between-bb + popcount per sniper pair |
| **2.62 %** | `OnceLock::get` (magic table deref) | Ongoing overhead from lazy-init tables |
| **1.72 %** | Bitboard `BitAnd` / `BitXor` (types.rs:353, 367) | Ubiquitous bitwise ops in hot paths |
| **1.72 %** | `Board::legal` blast computation (board.rs:460–462) | Post-blast occupancy builder |
| **1.66 %** | `parser_builder` / str hot path | FEN parsing during perft |
| **1.34 %** | `legal()` pseudo-royal loop (board.rs:849–856) | Commoner adjacency + attackers_to |
| **1.29 %** | `magic::bishop_attacks` index + table load (magic.rs:497) | Magic lookup (after OnceLock) |
| **1.28 %** | Bitboard `BitAnd` (types.rs:353) in generate_pseudo_legal | per-piece-type filtering |
| **1.08 %** | `compute_checkers` slider loop (board.rs:443–455) | Rook/bishop/queen/knight/pawn per commoner |
| **1.08 %** | `legal()` capture-path (board.rs:460) | Non-pawn blast zone computation |
| **1.02 %** | `magic::rook_attacks` index + table load (magic.rs:497) | Magic lookup |
| **1.02 %** | `pext::bishop_attacks_pext` (pext.rs:169) | PEXT table lookup |
| **1.02 %** | `__memchr` hot path (str iteration) | FEN parsing during perft |
| **1.02 %** | `pext::rook_attacks_pext` (pext.rs:169) | PEXT table lookup |
| **0.97 %** | `OnceLock::get` (magic table deref) | Bishop table access |
| **0.96 %** | Bitboard `BitXor` (types.rs:367) in legal() | Occupancy construction |
| **0.96 %** | `pext::rook_attacks_pext` table load | PEXT table access |
| **0.96 %** | `attacks::king_attacks` (attacks.rs:273) | const array load |
| **0.96 %** | `attacks::king_attacks` (attacks.rs:273) in legal() | Commoner adjacency check |
| **0.89 %** | `legal()` capture blast (board.rs:460) | Occupancy update |
| **0.89 %** | `ptr::write` (ptr/mod.rs) in pawn gen | MoveList store |
| **0.84 %** | `OnceLock::get` (magic table deref) | Bishop table |

Note: With only 1K samples, entries below ~0.5 % are within noise. The 5 hottest functions account for **~13 %** of samples; the top 15 for **~30 %**. No single bottleneck dominates (unlike the 13.98 % LazyLock in Plan 4).

---

## Comparison: Fairy-Stockfish vs Current State

| Aspect | Fairy-Stockfish (C++) | atomic_movegen (Rust) | Gap |
|--------|----------------------|----------------------|-----|
| **EVASIONS split** | `generate<EVASIONS>` when in check, `generate<NON_EVASIONS>` otherwise | Always `generate_pseudo_legal()` → filter | **Major** — waste in check positions |
| **Double check shortcut** | Only king moves when 2+ checkers | No shortcut | **Major** |
| **Target restriction** | `between_bb + checkers` for blocking/capture | Full `!occupied | enemy` target | **Major** |
| **Table init** | Eager global arrays (0 runtime overhead) | `OnceLock` relaxed atomic + check on each lookup | **Minor** — ~1-2 cycles per lookup |
| **Bulk pawn moves** | Shift-based bitboard generation (all pawns at once) | Per-pawn `pop_lsb` iteration | **Moderate** |
| **Pinned computation** | `slider_blockers()` with occupancy-delta + `!more_than_one()` | `count() == 1`, no occupancy-delta | **Moderate** |
| **pseudoRoyal caching** | `st->pseudoRoyals` full bitboard cached | Only `commoners_count` (u32) cached | **Moderate** |
| **Checkers caching** | `st->checkersBB` cached in StateInfo | Recomputes in `checkers()` (public API) | **Moderate** |
| **Pinned caching** | `st->blockersForKing[c]` cached | Recomputes in `pinned()` (public API) | **Moderate** |
| **attackers_to fusion** | Shared slider computation (rook_atk reused for queen) | Already fused in `attackers_to()` | **None** |
| **MoveList bounds** | Raw array + counter (no bounds check) | `if self.len < MAX_MOVES` guard | **Minor** |
| **Square construction** | `make_square(f, r)` = `(r << 3) | f` | `from_index(i8)` with bounds check | **Minor** |

---

## Optimization Opportunities

### 1. [(CRITICAL) Add EVASIONS/NON_EVASIONS move generation split] Estimated: 8–20 % speedup

**Problem:** `generate_legal()` always generates all pseudo-legal moves, even when the side to move is in check. In atomic chess, checks are common due to explosive capture chains exposing commoners. When in check, most non-commoner moves are illegal and the commoner moves are heavily restricted.

The full `is_move_trivially_legal()` fast-path returns false (because `checkers` is non-empty), so every pseudo-legal move falls through to the full `legal()` check — which then does blast simulation, commoner adjacency, and attackers_to computation for moves that are irrelevant.

**Fix:** Add an evasions path as Fairy-Stockfish does in `movegen.cpp:376–474`:

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);

    if !state.checkers.is_empty() {
        generate_evasions(board, &state, moves);
        // No compaction needed — evasions are already legal.
    } else {
        generate_pseudo_legal(board, moves);
        // Inline compaction pass
        let orig_len = moves.len();
        // ... existing compaction loop
    }
}
```

Within `generate_evasions()`:
- **Double check** (`checkers.count() > 1`): Only generate commoner moves. Non-commoner moves are never legal.
- **Single check by slider**: Target = `between_bb(ksq, checker) | checkers` — must block or capture.
- **Single check by leaper (knight/pawn)**: Target = `checkers` — must capture the checking piece.
- **Commoner moves**: Always generate (may move out of check or capture the checker).

**Fairy-Stockfish reference:** `movegen.cpp:376-474` — `generate_all()` template specialization for `EVASIONS`.

**Effort:** ~100 lines. Requires `between_bb()` (already precomputed).

**Impact:** In tactical positions with checks (Tests #2, #13, #33, which are the 3 slowest), the evasion path eliminates 50–90 % of pseudo-legal moves. Total speedup estimated at 8–20 % depending on position frequency.

---

### 2. [(HIGH) Eliminate OnceLock from magic/PEXT tables via build.rs precomputation] Estimated: 3–6 % speedup

**Problem:** Every call to `bishop_attacks()`, `rook_attacks()` goes through `OnceLock::get()` which performs a relaxed atomic load + branch. The `perf.data` shows 2.62 % + 0.97 % + 0.84 % = ~4.4 % of samples in `OnceLock::get` for magic/PEXT tables.

The `OnceLock` is initialized once before any move generation via `attacks::init()`, but the atomic check remains on every access. On x86_64, a relaxed atomic load is a regular load (1–2 cycles), but it still involves loading from memory and a comparison.

**Fix:** Use a `build.rs` script to generate the magic/PEXT attack tables as a binary blob. Emit the blob as a `.rs` file with an `include_bytes!()` directive, or embed as `const` data:

```rust
// In magic.rs, generated by build.rs:
pub(crate) static ROOK_TABLE: &[Bitboard] = &include_bytes!(concat!(env!("OUT_DIR"), "/rook_table.bin"));
```

The `build.rs` would:
1. Use the existing `build_magic_table()` function compiled for the build host
2. Serialize the resulting `[Bitboard; TABLE_SIZE]` as raw bytes
3. The Rust binary includes this as a `static` reference — **zero runtime init, zero atomic checks**

**Alternative (simpler):** Use `Box::leak` + `OnceLock::get_or_init` with a flag, or use a `pub static ROOK_TABLE: &[Bitboard] = &[]` that gets overwritten via unsafe pointer write in `init()`.

**Simplest fix:** Replace `OnceLock` with a `static ROOK_SLICE: &[Bitboard] = &[]` and initialize it with `unsafe` pointer write (safe because init happens before any access):

```rust
static ROOK_TABLE: &[Bitboard] = &[];

pub fn init() {
    let table = build_magic_table(...).into_boxed_slice();
    // SAFETY: called once before any access.
    unsafe { *(&ROOK_TABLE as *const _ as *mut &[Bitboard]) = Box::leak(table); }
}
```

This eliminates all atomic operations — just direct static dereference.

**Effort:** ~20 lines for the `build.rs` approach, ~5 lines for the unsafe pointer write.

---

### 3. [(HIGH) Bulk-bitboard pawn move generation] Estimated: 3–5 % speedup

**Problem:** The current `generate_pawn_moves_for()` iterates over each pawn individually using `pop_lsb()`. Each iteration calls `Square::from_index()` multiple times with bounds checks, does file/rank extraction, and pushes individual moves. With up to 8 pawns per side, this adds up.

**Fix:** Use bitboard-wide shift operations to generate all pawn moves at once (the standard chess programming pattern):

```rust
fn generate_pawn_moves(board: &Board, us: Color, them: Color, moves: &mut MoveList) {
    let pawns = board.pieces_color_pt(us, PieceType::Pawn);
    let occupied = board.occupied();
    let empty = !occupied;

    let (push, double, promo_rank, start_rank) = match us {
        Color::White => (North, North + North, Rank8, Rank2),
        Color::Black => (South, South + South, Rank1, Rank7),
    };

    // Single pushes
    let single_targets = shift(pawns, push) & empty;
    // Promotions
    let promo_targets = single_targets & rank_bb(promo_rank);
    let non_promo_targets = single_targets & !rank_bb(promo_rank);
    // Double pushes
    let double_targets = shift(shift(pawns & rank_bb(start_rank), push) & empty, push) & empty;

    // Captures
    let captures = (shift(pawns, NorthWest) | shift(pawns, NorthEast)) & board.pieces_color(them);
    // ... iterate over target bits and generate moves
}
```

**Fairy-Stockfish reference:** `movegen.cpp:104-165` — uses `shift<Up>(pawns)` templates for bulk generation.

**Effort:** ~60 lines. Replaces the per-pawn `generate_pawn_moves_for()` function.

---

### 4. [(HIGH) Optimize compute_pinned with occupancy-delta + is_one()] Estimated: 2–5 % speedup

**Problem:** In `compute_pinned()` (board.rs:479–508):
1. `between.count() == 1` uses a full 64-bit popcount, which is expensive (~3 cycles on Zen 3). Using `!between.more_than_one() && !between.is_empty()` (which is `!(b.0 & (b.0 - 1)) && b.0 != 0`) avoids the popcount.
2. The occupancy includes the snipers themselves, meaning one sniper can "block" another. FS removes snipers from occupancy: `occupied = pieces() ^ slidingSnipers`.
3. The `snipers` computation calls `attacks::rook_attacks(ksq, Bitboard::EMPTY)` and `attacks::bishop_attacks(ksq, Bitboard::EMPTY)` for each commoner — these are empty-board attacks and could be precomputed (they're just `pseudo_attacks[ROOK][sq]`, already available as `attacks::rook_attacks(sq, EMPTY)` but still goes through magic/PEXT dispatch).

**Fix:**

```rust
pub fn compute_pinned(&self, us: Color) -> Bitboard {
    let commoners = self.commoners(us);
    let them = us.flip();
    let them_bb = self.pieces_color(them);
    let occupied = self.occupied();

    let mut pinned = Bitboard::EMPTY;
    let mut c = commoners;
    while !c.is_empty() {
        let ksq = c.pop_lsb();
        let rook_att = attacks::rook_attacks(ksq, Bitboard::EMPTY);
        let bishop_att = attacks::bishop_attacks(ksq, Bitboard::EMPTY);

        let snipers = ((self.by_type[PieceType::Rook as usize] | self.by_type[PieceType::Queen as usize]) & rook_att)
                    | ((self.by_type[PieceType::Bishop as usize] | self.by_type[PieceType::Queen as usize]) & bishop_att);
        let snipers = snipers & them_bb;

        let occ = occupied ^ snipers;  // occupancy-delta
        let mut s = snipers;
        while !s.is_empty() {
            let sniper_sq = s.pop_lsb();
            let between = between_bb(ksq, sniper_sq) & occ;
            // is_one() = !more_than_one() && !is_empty()
            if !between.more_than_one() && !between.is_empty() {
                pinned = pinned | between;
            }
        }
    }
    pinned
}
```

**Fairy-Stockfish reference:** `position.cpp:907-915` — `slider_blockers()` with `pieces() ^ slidingSnipers` and `!more_than_one(b)`.

**Effort:** ~20 lines.

---

### 5. [(MEDIUM) Cache pseudoRoyal bitboards in StateInfo] Estimated: 2–4 % speedup

**Problem:** `StateInfo` caches `commoners_count` and `them_commoners_count` but NOT the full pseudoRoyal bitboard. Every call to `board.commoners(us)` or `board.commoners(them)` in `legal()`, `compute_checkers()`, and `compute_pinned()` recomputes `self.by_color[c] & self.by_type[Commoner]` — a double array load + and. This is called 3–5 times per `legal()` call.

**Fix:** Add `our_pseudo_royals: Bitboard` and `them_pseudo_royals: Bitboard` to `StateInfo`. Compute once in `populate_state()`:

```rust
pub fn populate_state(&self, state: &mut StateInfo) {
    state.checkers = self.compute_checkers(self.side_to_move);
    state.pinned = self.compute_pinned(self.side_to_move);
    let our_commoners = self.commoners(self.side_to_move);
    let them_commoners = self.commoners(self.side_to_move.flip());
    state.commoners_count = our_commoners.count();
    state.them_commoners_count = them_commoners.count();
    state.our_pseudo_royals = our_commoners;
    state.them_pseudo_royals = them_commoners;
}
```

Then in `legal()` and `compute_checkers()`, use `state.our_pseudo_royals` / `state.them_pseudo_royals` instead of `self.commoners(us)` / `self.commoners(them)`.

**Fairy-Stockfish reference:** `st->pseudoRoyals` — a single `Bitboard` for both sides' pseudo-royal pieces, computed in `set_check_info()`.

**Effort:** ~15 lines.

---

### 6. [(MEDIUM) Fused attackers_to with shared slider computation] Estimated: 1–3 % speedup

**Problem:** The private `attackers_to()` function (board.rs:870–881) in `legal()`'s pseudo-royal loop already fuses slider computation (computes rook_atk once, bishop_atk once, reuses for queen). However, `compute_checkers()` (board.rs:429–471) recomputes `rook_attacks`, `bishop_attacks`, and `queen_attacks` separately for each commoner and each attack type. This is redundant work.

**Fix:** In `compute_checkers()`, fuse the slider computation:

```rust
let rook_atk = attacks::rook_attacks(ksq, occupied);
let bishop_atk = attacks::bishop_attacks(ksq, occupied);
let queen_atk = rook_atk | bishop_atk;
checkers = checkers
    | (rook_atk & self.by_type[PieceType::Rook as usize] & them_bb)
    | (bishop_atk & self.by_type[PieceType::Bishop as usize] & them_bb)
    | (queen_atk & self.by_type[PieceType::Queen as usize] & them_bb);
```

The current code at `board.rs:443-455` computes these separately, though the compiler might CSE them. Explicit fusion guarantees it.

**Fairy-Stockfish reference:** `attackers_to()` in `position.cpp` — single fused expression.

**Effort:** ~10 lines.

---

### 7. [(MEDIUM) MoveList::push() — eliminate bounds check] Estimated: 2–3 % speedup

**Problem:** `MoveList::push()` (types.rs:740–747) performs `if self.len < MAX_MOVES` on every single push. The 6.46 % in the Plan 4 ARM profile for MoveList::push shows this is measurable, though the x86 profile shows it lower (subsumed into surrounding functions due to inlining).

**Fix:** Use `unsafe` to skip the bounds check:

```rust
#[inline(always)]
pub fn push(&mut self, m: Move) {
    // SAFETY: All callers guarantee len < MAX_MOVES (max perft at depth 1 is < 150 moves).
    unsafe {
        *self.moves.as_mut_ptr().add(self.len) = m;
    }
    self.len += 1;
}
```

This is safe in practice because the worst-case atomic chess position generates well under 256 moves. Every call site has guaranteed room.

**Effort:** ~3 lines.

---

### 8. [(MEDIUM) Optimize compute_checkers() bitboard-level adjacency check] Estimated: 1–2 % speedup

**Problem:** The adjacent-commoner check in `compute_checkers()` (board.rs:459–468) iterates over `them_commoners` individually:

```rust
let mut tc = them_commoners;
while !tc.is_empty() {
    let tksq = tc.pop_lsb();
    if attacks::king_attacks(tksq) & commoners != Bitboard::EMPTY {
        checkers = checkers | Bitboard::square_bb(tksq);
    }
}
```

**Fix:** Replace the while-loop with a bitboard-level expression:

```rust
// All squares adjacent to any enemy commoner:
let king_atk_all = king_attacks(them_commoners);  // would need multi-target version
// Or manually compute using shifts:
let adj = (shift_north(them_commoners) | shift_south(them_commoners)
    | shift_east(them_commoners) | shift_west(them_commoners)
    | shift_ne(them_commoners) | shift_nw(them_commoners)
    | shift_se(them_commoners) | shift_sw(them_commoners))
    & commoners;
if adj != Bitboard::EMPTY {
    checkers = checkers | (adj & them_commoners);
}
```

The directional shifts expand the commoner set by one square in each direction, capturing all squares adjacent to any enemy commoner. If any overlaps with our commoners, the adjacent enemy commoners are checkers.

**Effort:** ~15 lines. Requires defining shift helpers or inline u64 operations.

---

### 9. [(LOW) Hoist by_color loads in generate_pseudo_legal()] Estimated: 1–2 % speedup

**Problem:** `generate_pseudo_legal()` calls `board.pieces_color_pt(us, PieceType::*)` for each of 6 piece types. Each call loads `by_color[us]` and `by_type[pt]` and ANDs them. The `by_color[us]` load is repeated 6 times.

**Fix:**

```rust
pub fn generate_pseudo_legal(board: &Board, moves: &mut MoveList) {
    let us = board.side_to_move();
    let them = us.flip();
    let our_pieces = board.pieces_color(us);
    let occupied = board.occupied();
    let target = !occupied | board.pieces_color(them);

    let pawns = our_pieces & board.by_type[PieceType::Pawn as usize];
    // ... iterate pawns
    let knights = our_pieces & board.by_type[PieceType::Knight as usize];
    // ... iterate knights
    // etc.
}
```

**Effort:** ~10 lines.

---

### 10. [(LOW) Unchecked from_index_unchecked() in hot paths] Estimated: 1–2 % speedup

**Problem:** `Square::from_index(idx)` (board.rs:990–996) checks `(0..64).contains(&idx)` before indexing. In hot paths like `generate_pawn_moves_for()`, the index is validated before the call (e.g., `to_idx = from_idx + 8` which is already checked via `to_sq != NONE`).

**Fix:** Add an unchecked variant:

```rust
#[inline(always)]
pub fn from_index_unchecked(idx: i8) -> Square {
    debug_assert!((0..64).contains(&idx));
    // SAFETY: Caller guarantees idx is in 0..63.
    unsafe { std::mem::transmute(idx as u8) }
}
```

Then in `generate_pawn_moves_for()`:
```rust
let to_sq = if (0..64).contains(&to_idx) {
    Square::from_index_unchecked(to_idx)
} else {
    Square::NONE
};
```

**Effort:** ~10 lines.

---

### 11. [(LOW) Cache occupied bitboard in Board struct] Estimated: 0.5–1 % speedup

**Problem:** `Board::occupied()` computes `by_color[0] | by_color[1]` each call. This is called in `generate_pseudo_legal`, `compute_checkers`, `compute_pinned`, and `legal()`.

**Fix:** Add `occupied: Bitboard` field to `Board`. Update it in `move_piece()`, `remove_piece()`, `place_piece()` using XOR/OR:

```rust
fn move_piece(&mut self, from: Square, to: Square) {
    // ... existing logic
    self.occupied = (self.occupied ^ from_bb) | to_bb;
}
```

**Fairy-Stockfish reference:** `byTypeBB[ALL_PIECES]` maintained incrementally.

**Effort:** ~20 lines across `Board` struct, constructor, and all piece-manipulation functions.

---

### 12. [(LOW) Inline move_type() extraction] Estimated: 0.5–1 % speedup

**Problem:** `Move::move_type()` uses a `match` on `(self.0 >> 12) & 3`. The compiler may not inline this perfectly across all call sites.

**Fix:** Add `#[inline(always)]` (already present implicitly in many cases) and/or use direct comparison:

```rust
pub fn is_normal(self) -> bool { (self.0 >> 12) & 3 == 0 }
pub fn is_promotion(self) -> bool { (self.0 >> 12) & 3 == 1 }
pub fn is_en_passant(self) -> bool { (self.0 >> 12) & 3 == 2 }
pub fn is_castling(self) -> bool { (self.0 >> 12) & 3 == 3 }
```

**Effort:** ~10 lines.

---

### 13. [(LOW) Fused compute_checkers + compute_pinned] Estimated: 1–3 % speedup

**Problem:** `populate_state()` calls `compute_checkers()` and `compute_pinned()` separately. Both iterate over commoners and snipers with overlapping work. Each does a complete traversal of commoners.

**Fix:** Fuse into a single traversal:

```rust
pub fn populate_state(&self, state: &mut StateInfo) {
    let mut checkers = Bitboard::EMPTY;
    let mut pinned = Bitboard::EMPTY;
    let us = self.side_to_move;
    let them = us.flip();
    let commoners = self.commoners(us);
    let occupied = self.occupied();

    let mut c = commoners;
    while !c.is_empty() {
        let ksq = c.pop_lsb();
        // Compute checkers for this commoner
        let rook_atk = attacks::rook_attacks(ksq, occupied);
        let bishop_atk = attacks::bishop_attacks(ksq, occupied);
        let queen_atk = rook_atk | bishop_atk;
        let them_bb = self.pieces_color(them);
        // ... accumulate checkers ...

        // Compute pinners for this commoner
        let empty_rook = attacks::rook_attacks(ksq, Bitboard::EMPTY);
        let empty_bishop = attacks::bishop_attacks(ksq, Bitboard::EMPTY);
        // ... accumulate pinned ...
    }

    state.checkers = checkers;
    state.pinned = pinned;
    // ... rest of populate_state
}
```

This eliminates one complete commoner-iteration overhead.

**Effort:** ~40 lines.

---

### 14. [(LOW) Branch hints via core::hint::likely/unlikely] Estimated: 0.5–1 % speedup

**Problem:** Critical branches in the hot path lack branch prediction hints. On x86_64, `likely!`/`unlikely!` hints tell the compiler to lay out code optimally.

**Fix:** Annotate key branches:
- `if is_capture` → `unlikely` (most moves are non-captures in atomic chess due to blast)
- `if checkers != EMPTY` → `unlikely` (most positions are not in check)
- `if move_type == Castling` → `unlikely`
- `if move_type == EnPassant` → `unlikely`

```rust
use core::hint::{likely, unlikely};

if unlikely(is_capture) { ... }
```

**Effort:** ~20 annotations across hot functions.

---

### 15. [(LOW) StateInfo reuse in perft recursion] Estimated: 0.5–1 % speedup

**Problem:** The perft loop in `lib.rs:61-65` creates a new `StateInfo` per position. The `StateInfo` is 81+ bytes of zero-initialization. While stack allocation is cheap, the memset of StateInfo's 9-element `captured` array adds up.

**Fix:** Use a persistent `StateInfo` that gets properly reset in `do_move()` rather than zeroed. The `do_move()` already sets `captured_count = 0`, so the captured array entries after `captured_count` are never read. We could avoid zeroing the entire struct by making `StateInfo::new()` a no-op and trusting `populate_state`/`do_move` to set all fields.

```rust
pub fn new() -> Self {
    // Safe: all fields are overwritten by do_move() or populate_state() before reading.
    StateInfo {
        checkers: Bitboard::EMPTY,
        pinned: Bitboard::EMPTY,
        commoners_count: 0,
        them_commoners_count: 0,
        castling_rights: 0,
        ep_square: None,
        rule50: 0,
        captured_count: 0,
        captured: [(Square::NONE, NO_PIECE); 9],  // only memset 2 * 9 = 18 bytes
        cap_sq: None,
        cap_piece: NO_PIECE,
    }
}
```

Or better: move `StateInfo::new()` to a const, eliminating the runtime initialization entirely.

**Effort:** ~5 lines.

---

## Out-of-the-Box Ideas

### O1. Generator/callback pattern for legal move processing

Instead of storing all pseudo-legal moves in `MoveList` and then filtering, use a callback that processes each non-trivially-legal move immediately. For perft, this would directly recurse without the intermediate store:

```rust
pub fn perft(board: &mut Board, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    if depth == 1 { return generate_legal_count(board); }  // fast count, no move storage

    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    let mut total = 0u64;

    for_each_legal_move(board, &state, |m, state| {
        board.do_move(m, state);
        total += perft(board, depth - 1);
        board.undo_move(m, state);
    });

    total
}
```

This eliminates the `MoveList` entirely from the perft path. For the public API, a `MoveList`-producing version wraps the callback.

**Impact:** Removes the store-and-compact pattern, saving 6–10 % of cycles currently spent in `generate_pseudo_legal` → `MoveList::push` → compaction loop → `do_move` reading from MoveList.

### O2. Check-aware legal() fast path

The current `legal()` function always computes the post-blast occupancy, even when not in check and the move is not a capture. The `is_move_trivially_legal()` fast path handles this, but when it fails (commoner move, capture, pin, castling, check), the full `legal()` recomputes everything from scratch.

For non-capture moves not involving the last commoner, `legal()` could return early without the blast computation:

```rust
pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
    // ... early checks ...

    if !is_capture && state.commoners_count > 1 {
        // Non-capture, not last commoner, can't self-explode
        // Just check pin and commoner safety
        let ksq = self.commoners(us).lsb();
        if !(state.pinned & from_bb).is_empty() && !aligned(from, to, ksq) {
            return false;  // pin-violating non-capture
        }
        return true;  // safe non-capture
    }

    // Full blast/pseudo-royal check for captures and last-commoner moves
    // ...
}
```

**Impact:** Saves the blast-zone computation and attackers_to call for most moves.

### O3. Build-time precomputation of all attack tables

The magic/PEXT tables are 100+K of deterministic data. A `build.rs` script can generate them as `include_bytes!()` arrays, eliminating:
- The `OnceLock` atomic check (saves ~4 % from profile)
- The 4.55 % memset overhead for table initialization
- The runtime init function and its `Once` guard
- The `Box::leak` memory pattern (wasteful, confuses leak sanitizers)

**Impact:** Cleaner code + 3–6 % speedup.

### O4. Parallel perft via subtree splitting

The perft at each node splits into independent subtrees. Each move's subtree can be computed independently. While this doesn't help single-threaded latency, it would allow utilizing the 4 vCPUs available:

```rust
pub fn perft_parallel(board: &Board, depth: u32) -> u64 {
    // Generate moves at root, then dispatch each subtree to a thread
}
```

**Impact:** Up to 4× throughput on multi-core systems. Not a "speed" improvement per se but improves wall-clock time.

---

## Summary: Sorted by Estimated Impact

| # | Optimization | Est. Speedup | Effort | Risk | Fairy-Stockfish precedent |
|---|-------------|-------------|--------|------|---------------------------|
| **1** | **EVASIONS/NON_EVASIONS split** | **8–20 %** | ~100 lines | Medium | `generate<EVASIONS>` vs `generate<NON_EVASIONS>` |
| **2** | **Eliminate OnceLock via build.rs / unsafe static** | **3–6 %** | ~20 lines | Low | Eager global arrays |
| **3** | **Bulk-bitboard pawn move generation** | **3–5 %** | ~60 lines | Low | `shift<Up>(pawns)` templates |
| **4** | **Optimize compute_pinned (occupancy-delta + is_one)** | **2–5 %** | ~20 lines | Low | `slider_blockers()` with `pieces() ^ slidingSnipers` + `!more_than_one()` |
| **5** | **Cache pseudoRoyal bitboards in StateInfo** | **2–4 %** | ~15 lines | Low | `st->pseudoRoyals` |
| **6** | **Fused slider computation in compute_checkers** | **1–3 %** | ~10 lines | None | Single fused `attackers_to` expression |
| **7** | **MoveList::push() — eliminate bounds check** | **2–3 %** | ~3 lines | Low | Raw array + counter |
| **8** | **Optimize compute_checkers adjacency loop** | **1–2 %** | ~15 lines | Low | N/A (atomic-specific) |
| **9** | **Hoist by_color loads in generate_pseudo_legal** | **1–2 %** | ~10 lines | None | N/A |
| **10** | **Unchecked from_index_unchecked in hot paths** | **1–2 %** | ~10 lines | Low | N/A |
| **11** | **Cache occupied bitboard** | **0.5–1 %** | ~20 lines | Low | Incremental `byTypeBB` |
| **12** | **Inline move_type() direct field access** | **0.5–1 %** | ~5 lines | None | N/A |
| **13** | **Fused compute_checkers + compute_pinned** | **1–3 %** | ~40 lines | Low | `set_check_info()` computes both |
| **14** | **Branch hints (likely/unlikely)** | **0.5–1 %** | ~20 annotations | None | N/A |
| **15** | **StateInfo zero-init avoidance** | **0.5–1 %** | ~5 lines | None | N/A |
| **O1** | **Generator/callback pattern** | **6–10 %** | ~80 lines | High | N/A (C++ uses templates) |
| **O2** | **Check-aware legal() fast path** | **2–4 %** | ~30 lines | Medium | N/A (atomic-specific) |
| **O3** | **Build-time table precomputation** | **3–6 %** | ~50 lines | Low | `PRECOMPUTED_MAGICS` |
| **O4** | **Parallel perft** | **4× wall** | ~50 lines | Low | N/A |

---

## Cumulative Potential

If all core items (1–10) were implemented:

```
1 - (1-0.12)(1-0.04)(1-0.04)(1-0.03)(1-0.03)(1-0.02)(1-0.02)(1-0.01)(1-0.01)(1-0.01)
≈ 1 - 0.88 × 0.96 × 0.96 × 0.97 × 0.97 × 0.98 × 0.98 × 0.99 × 0.99 × 0.99
≈ 1 - 0.72
≈ 28 % from current baseline
```

This would bring `verify_perft` from **75.6 s → ~54.4 s**, representing a total **56 % cumulative speedup** from the original baseline.

Items O1 + O2 + O3 could push this further to ~45 s (64 % cumulative).

---

## Recommended Implementation Order

### Phase 1: "Kill OnceLock" (Item 2 — 3–6 %)
Use a `build.rs` script or unsafe `static mut` pointer write to eliminate the relaxed atomic load on every magic/PEXT call. This is a prerequisite for efficient slider-dependent optimizations.

### Phase 2: "Evasions" (Item 1 — 8–20 %)
Implement `generate_evasions()` with the double-check shortcut and between-based target restriction. This is the single largest remaining optimization.

### Phase 3: Algorithmic (Items 3–5)
- Bulk pawn moves
- compute_pinned optimization
- pseudoRoyal caching

### Phase 4: Micro-optimizations (Items 6–10)
- Fused sliders in checkers
- MoveList bounds elimination
- compute_checkers adjacency loop
- Hoisted by_color loads
- Unchecked from_index

### Phase 5: Polish (Items 11–15)
- Occupied caching
- Inline move_type
- Fused checkers+pinned
- Branch hints
- StateInfo zero-init

### Phase 6: Out-of-the-box (Items O1–O4)
- Generator/callback pattern
- Check-aware fast path
- Build-time tables
- Parallel perft
