# Performance Analysis 4 — `atomic-movegen`

## Current Baseline

**System:** Linux on arm64 (Apple Firestorm), perf sampling at 999 Hz.

| Metric | Value |
|--------|-------|
| **Total time (41 positions, depths 1–6)** | **93.940 s** |
| Average per test | 2.291 s |
| Slowest test | Test #13 (14.098 s, 2.16B nodes) |
| Fastest test | Test #30 (0.001 s) |

### Cumulative optimizations applied to date

| Plan | Change | Speedup | Cumulative |
|------|--------|---------|------------|
| Orig baseline | — | — | 124.380 s |
| Plan 1 (perf_analysis) | Stack-allocated `MoveList` replacing `Vec<Move>` | 11.1 % | 11.1 % |
| Plan 2 (perf_analysis) | Inline legal filtering in `generate_legal()` | 2.9 % | 13.7 % |
| Plan 1 (perf_analysis3) | MagicEntry struct + transmute accessors | 9.3 % | 21.7 % |
| Plan 2 (perf_analysis3) | Eliminate redundant `queen_attacks()` + inline + field reorder | 3.3 % | 24.3 % |
| **Current** | | **24.3 % cumulative** | **93.940 s** |

---

## Methodology

1. **perf profiling** of the current codebase at 999 Hz on a tactical position
   (`r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15`, depth 6).
2. **Address resolution** via `addr2line` against the unstripped profiling binary
   (LTO merges CGUs, so line numbers reference CGU boundaries).
3. **Source comparison** with the Fairy-Stockfish C++ reference at
   `./Fairy-Stockfish/src/` for `legal()`, `slider_blockers()`,
   `set_check_info()`, `attackers_to()`, magic lookup, and check evasion.
4. **Manual code review** of all 8 source files in `src/` for redundant work,
   cache-inefficient patterns, and compiler-frustrating constructs.

---

## Profiling Results — CPU Hotspots

### Self-time breakdown by function

| Self CPU | Function | Source |
|----------|----------|--------|
| **57.48 %** | `atomic_movegen::perft` | `lib.rs:39–58` (recursive driver) |
| **27.16 %** | `Board::legal` | `board.rs:691–902` (legality check) |
| **8.89 %** | `Board::compute_checkers` | `board.rs:349–396` (check computation) |
| **4.74 %** | `Board::compute_pinned` | `board.rs:402–431` (pin computation) |
| 1.88 % | (unresolved) | — |

### Key observations

1. **perft() at 57.48 %** is the recursive driver: it calls `generate_legal()` → `do_move()` → recurse → `undo_move()`. Within it the cost is split between:
   - `generate_legal` (~30 %): `StateInfo::new()` + `populate_state()` + `generate_pseudo_legal()` + inline filter
   - Loop + recurse (~15 %): move iteration, function call overhead
   - `do_move` + `undo_move` (~12 %): state copy, board update, blast computation

2. **legal() at 27.16 %** is the second hotspot. The `is_move_trivially_legal()` fast-path filters most simple moves, but for tactical positions (captures, pins, checks, commoner moves) the full `legal()` still runs for a significant fraction of moves.

3. **compute_checkers() at 8.89 %** and **compute_pinned() at 4.74 %** are called from `populate_state()`, which runs once per `do_move()` call — i.e., once for every node in the perft tree.

4. **The perft() self-time is unusually high** (57.48 %) because this profile was taken on the `apple_firestorm_pmu` PMU event (actual cycles). On a simpler cycle-counter, the function-call-heavy `perft()` driver registers a large number of samples for its loop/recurse overhead. The actual "useful work" (legal checking, attack computation) within that time is captured in the child samples.

---

## Comparison with Fairy-Stockfish Reference

### Architectural gaps remaining

| # | Aspect | `atomic-movegen` (Rust) | Fairy-Stockfish (C++) | Impact |
|---|--------|------------------------|----------------------|--------|
| **A** | **Check evasion generation** | Always generates ALL pseudo-legal moves, then filters | Generates `EVASIONS` when in check (~⅓ of move count) | **Very High** |
| **B** | **Pseudo-royal bitboard caching** | Only caches `commoners_count` (u32), recomputes `self.commoners(c)` bitboard in `legal()` | Caches `st->pseudoRoyals` for both sides as full bitboards | **High** |
| **C** | **`between_bb()` implementation** | Loop-based with `make_square()` match statements per iteration | `BetweenBB[s1][s2]` — single table lookup | **Medium** |
| **D** | **`compute_pinned()` algorithm** | Counts blockers with `count() == 1` | Uses occupancy-delta (`occupancy ^ snipers`) + `!more_than_one(b)` | **Medium** |
| **E** | **Magic table init** | `LazyLock<Box<[Bitboard]>>` — atomic check on every lookup | Static `Magic` struct array, no per-access init check | **Medium** |
| **F** | **`attackers_to()` structure** | Separate per-type blocks reloading `by_type` arrays | Single fused expression with shared slider attacks | **Medium** |
| **G** | **`StateInfo` reuse** | Creates new `StateInfo` per `generate_legal()` call | StateInfo is passed through the call chain | **Medium** |
| **H** | **`legal()` function size** | Single huge ~900-line function | Variant dispatch via `var->extinctionPseudoRoyal` block | **Low-Medium** |
| **I** | **`compute_checkers()` loop** | Loops over ALL commoners, recomputing attacks for each | Only checks the king square (single king) | **Low-Medium** |
| **J** | **`check_squares[pt]` caching** | Not implemented | Precomputed in `set_check_info()` for fast detection | **Low** |
| **K** | **Blast computation in legal()** | Re-derives post-blast `occupied` from scratch | Same algorithm but uses `attacks_bb<KING>` inline | **Low** |

### What Fairy-Stockfish does differently

The FS `legal()` function (position.cpp:1049–1296) is structured with clear early-return blocks:

1. **Pre-check:** non-standard rules (sittuyin, must-capture, must-drop, etc.)
2. **Extinction pseudo-royal block** (lines ~1099–1188):
   - Uses `st->pseudoRoyals` (cached bitboard for both sides)
   - Computes `pseudoRoyals` post-move with bitboard operations on the cached value
   - **Check-evasions-like early-out:** if all enemy pseudo-royals are destroyed, returns true immediately
   - Pseudo-royal immunity: checks `pseudoRoyalsTheirs & attacks_bb<KING>(sr)`
   - Attack check: `attackers_to(sr, occupied, ~us) & attackerCandidatesTheirs` — single fused call
3. **Standard legal block** for king moves, en-passant, castling, non-king moves

The key optimizations in FS that we haven't applied:

- **Check evasion generation** (A): When `checkers()` is non-empty, FS generates only `EVASIONS` instead of `NON_EVASIONS`. Evasion generation computes a much smaller target bitboard (`between_bb(ksq, checksq)` for blocking, plus `~pieces(Us)` for king moves, plus `checkers` for captures). This reduces the pseudo-legal count by 50–80 % in check positions.

- **Pseudo-royal bitboard caching** (B): FS computes `st->pseudoRoyals` once in `set_check_info()` as:
  ```cpp
  for (PieceSet ps = extinction_piece_types(); ps;) {
      PieceType pt = pop_lsb(ps);
      if (count(sideToMove, pt) <= var->extinctionPieceCount + 1)
          si->pseudoRoyals |= pieces(sideToMove, pt);
      if (count(~sideToMove, pt) <= var->extinctionPieceCount + 1)
          si->pseudoRoyals |= pieces(~sideToMove, pt);
  }
  ```
  This is computed once per node and stored as a bitboard. Our `legal()` recomputes `self.commoners(us)` and `self.commoners(them)` multiple times — each requiring two array loads and a bitwise AND.

- **Occupancy-delta in slider_blockers** (D): FS removes snipers from occupancy before checking:
  ```cpp
  Bitboard occupancy = pieces() ^ slidingSnipers;
  // ...
  Bitboard b = between_bb(s, sniperSq, ...) & occupancy;
  if (b && !more_than_one(b)) {
      blockers |= b;
  }
  ```
  This avoids self-interference when multiple snipers share a ray. Also, `!more_than_one(b)` is a simple bit-twiddle (`b & (b-1)`), not a full popcount.

- **Check-squares precomputation** (J): FS's `set_check_info()` precomputes `checkSquares[pt]` for each piece type:
  ```cpp
  si->checkSquares[pt] = ksq != SQ_NONE
      ? attacks_bb(~sideToMove, movePt, ksq, pieces())
      : Bitboard(0);
  ```
  This gives a fast "does this move give check?" test that doesn't need to recompute king attacks.

---

## New Optimization Opportunities

### 1. Check Evasion Generation (estimated: 8–20 %)

**Problem:** `generate_legal()` calls `generate_pseudo_legal()` which generates ALL
pseudo-legal moves regardless of check status. When the side to move is in check,
most generated moves are immediately filtered by `is_move_trivially_legal()` (returns
false) and then by `legal()` (returns false for non-blocking, non-capturing, non-king
moves).

**Impact:** For tactical positions with many checks (Test #13, #33, #2), this is the
single largest remaining optimization. Test #13 at 14.098 s contains many positions
where the side to move is in check. Generating only evasions would:
- Reduce pseudo-legal move count from ~40–50 down to ~5–15 when in single check
- Reduce pseudo-legal move count from ~40–50 down to ~3–8 commoner moves when in double check
- Save the full `legal()` call for every non-evasive pseudo-legal move

**How Fairy-Stockfish does it (movegen.cpp:375–497):**
```cpp
template<Color Us, GenType Type>
ExtMove* generate_all(const Position& pos, ExtMove* moveList) {
    // For EVASIONS:
    target = Type == EVASIONS ? between_bb(ksq, lsb(pos.checkers()))
           : Type == NON_EVASIONS ? ~pos.pieces(Us)
           : ...;
    // For double check: only king moves
    if (Type == EVASIONS && !more_than_one(pos.checkers()
        & ~pos.non_sliding_riders())) {
        target = Type == EVASIONS ? between_bb(ksq, lsb(pos.checkers())) : ...;
    }
    // For leaper checks that cannot be blocked:
    if (pos.checkers() & pos.non_sliding_riders())
        target = ~pos.pieces(Us);
    // Leaper attacks cannot be blocked
    Square checksq = lsb(pos.checkers());
    if (LeaperAttacks[~Us][type_of(pos.piece_on(checksq))][checksq]
        & pos.square<KING>(Us))
        target = pos.checkers();
}
```

**Effort:** ~100 lines. A new `generate_evasions()` function that:
1. Detects single vs. double check
2. For single check: generates moves that capture the checking piece,
   block the check (between squares), or move the commoner
3. For double check: only generates commoner moves
4. Bypasses `is_move_trivially_legal()` and `legal()` entirely (evasions
   are always legal by construction — but each must still pass the full
   legal check for blast/capture effects)

**Risk:** Medium — correctness-critical. Must handle atomic chess blast
interactions (capturing the checking piece may destroy it via blast,
which can be advantageous).

**Tests most affected:** #13, #33, #2, #31 — these are tactical positions
with heavy checking.

---

### 2. Cache pseudoRoyals Bitboard in StateInfo (estimated: 4–8 %)

**Problem:** `legal()` calls `self.commoners(us)` and `self.commoners(them)`
up to 4 times each, plus the self-explosion check at line 809 and the
`enemy_pr_destroyed` check at line 848. Each `commoners(c)` call is:
```rust
self.by_color[c as usize] & self.by_type[PieceType::Commoner as usize]
```
This is 2 array loads + 1 AND — but with memory latency, it can be
4–6 cycles per call. With 4 calls per `legal()` invocation, that's
~20 cycles per move that survives the fast-path filter.

For Test #13 (2.16B nodes), if even 10 % of moves reach the full `legal()`
check, that's 216M × 20 cycles = 4.3B cycles wasted on redundant
bitboard recomputation.

**Fix:** Add two fields to `StateInfo`:
```rust
pub our_pseudo_royals: Bitboard,
pub them_pseudo_royals: Bitboard,
```
Compute them in `populate_state()` and use them in `legal()`:
```rust
// Instead of:
let mut our_commoners = self.commoners(us) & occupied;
// Use:
let mut our_commoners = state.our_pseudo_royals & occupied;
```

This saves:
- 2 array loads (`by_color[c]`, `by_type[Commoner]`) + 1 AND per access
- 2 accesses in the self-explosion check (line 809)
- 1 access in the enemy_is_destroyed check (line 848)
- 1 access in the pseudo-royal loop (line 853)

**Synergy with item 1:** After evasion generation is implemented, the
proportion of calls bypassing the fast-path filter drops, so the absolute
savings from caching pseudo-royals also drops slightly. However, for
positions where `legal()` still runs (captures, commoner moves, castling),
the savings remain.

**Effort:** ~15 lines across `board.rs` (`StateInfo`, `populate_state()`,
`legal()`).

---

### 3. Precomputed BetweenBB and LineBB Tables (estimated: 3–6 %)

**Problem:** `between_bb()` in `src/bitboard.rs:100–151` is a loop-based
computation that calls `make_square()` with two match statements per
iteration (64 cases each). `compute_pinned()` calls `between_bb()` for
every (commoner, sniper) pair. `compute_checkers()` also uses
`between_bb()` indirectly through pin detection.

The Fairy-Stockfish approach is a precomputed `BetweenBB[s1][s2]` table
(64 × 64 × 8 bytes = 32 KB, fitting in L1 cache on modern CPUs).

**Why the previous attempt was reverted:** In Plan 2 (perf_analysis2),
a `LazyLock`-backed table was tried and showed no benefit because the
`LazyLock` atomic check overhead offset the gains from the table lookup.

**New approach:** Use `std::sync::OnceLock` and force-initialize at
program start via a one-time initialization function. Or better:
compute the table at compile time using `const fn` and embed it in
`.rodata`:

```rust
const BETWEEN_BB: [[Bitboard; 64]; 64] = compute_between_table();

const fn compute_between_table() -> [[Bitboard; 64]; 64] {
    let mut table = [[Bitboard(0); 64]; 64];
    let mut s1 = 0;
    while s1 < 64 {
        let mut s2 = 0;
        while s2 < 64 {
            table[s1][s2] = between_bb_const(s1 as u8, s2 as u8);
            s2 += 1;
        }
        s1 += 1;
    }
    table
}
```

The challenge is `const fn` limitations — but since we just need the
underlying `u64` values, we can compute a `[[u64; 64]; 64]` table
at compile time and wrap individual entries in `Bitboard` at access time.

**Alternative:** Use `static` with a raw slice and `include_bytes!` from
a build script that precomputes the data.

**Effort:** ~40 lines (compute function + table + updated accessor +
removal of loop-based implementation).

---

### 4. Optimize `compute_pinned()` with Occupancy-Delta (estimated: 2–4 %)

**Problem:** The current `compute_pinned()` (lines 402–431):
1. Computes snipers using `attacks::rook_attacks(ksq, Bitboard::EMPTY)`
2. For each sniper, computes `between_bb(ksq, sniper_sq) & occupied`
3. Checks `between.count() == 1` (full popcount)

Two issues:
- **Self-interference:** When multiple snipers share the same ray,
  `between_bb(ksq, sniper_sq) & occupied` includes the OTHER sniper as
  a "blocker", which can cause incorrect (or missed) pin detection.
  FS avoids this by `occupancy ^ snipers` (removing snipers).
- **`count() == 1` vs. `is_one()`:** `between.count() == 1` does a
  popcount instruction, while `between.is_one()` is the bit-twiddle
  `between != 0 && (between & (between - 1)) == 0` — which is faster
  on most microarchitectures.

**Fix:**
```rust
pub(crate) fn compute_pinned(&self, us: Color) -> Bitboard {
    let mut pinned = Bitboard::EMPTY;
    let them = us.flip();
    let occupied = self.occupied();

    let mut c_iter = self.commoners(us);
    while !c_iter.is_empty() {
        let ksq = c_iter.pop_lsb();

        // Snipers using pseudo-attacks (empty board)
        let rook_snipers = attacks::rook_attacks(ksq, Bitboard::EMPTY)
            & (self.by_type[PieceType::Rook as usize]
               | self.by_type[PieceType::Queen as usize])
            & self.pieces_color(them);
        let bishop_snipers = attacks::bishop_attacks(ksq, Bitboard::EMPTY)
            & (self.by_type[PieceType::Bishop as usize]
               | self.by_type[PieceType::Queen as usize])
            & self.pieces_color(them);
        let snipers = rook_snipers | bishop_snipers;

        // Remove snipers from occupancy to avoid self-blocking
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

Also add `is_one()` to `Bitboard` (if not already present — checking):

The method `more_than_one()` exists (returns `self.0 & (self.0 - 1) != 0`), but `is_one()` does not. Add:
```rust
pub fn is_one(self) -> bool {
    self.0 != 0 && (self.0 & (self.0 - 1)) == 0
}
```

**Effort:** ~25 lines (function rewrite + `is_one()` method).

---

### 5. Eliminate `LazyLock` for Magic Attack Tables (estimated: 2–4 %)

**Problem:** The magic attack tables `ROOK_TABLE` and `BISHOP_TABLE` are
initialized via `LazyLock<Box<[Bitboard]>>`. Every call to
`rook_attacks()` or `bishop_attacks()` compiles to:

```asm
; Load atomic state (first 8 bytes of LazyLock)
mov   rax, QWORD PTR [rip + ROOK_TABLE+8]
; Check if initialized
test  al, 3
; Branch to cold init path
jne   .L_initialized
; Init path (cold, rarely taken)
...
.L_initialized:
; Normal lookup continues...
```

On arm64 Apple Firestorm, this is ~3 instructions + 1 branch. With
billions of attack lookups per `verify_perft` run, even 3 instructions
per call adds up.

**Fix:** Precompute the magic tables at compile time using a build script
(`build.rs`) and embed them directly in the binary's `.rodata` section.
The build script would:
1. Call `build_magic_table()` during `cargo build`
2. Serialize the resulting `[Bitboard; N]` arrays to a binary file
3. Use `include_bytes!()` in `magic.rs` to embed the data
4. Cast the byte slice to `&'static [Bitboard]`

```rust
static ROOK_TABLE: &[Bitboard] = {
    const DATA: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/rook_table.bin"
    ));
    // Safety: build script produces properly aligned data
    unsafe {
        std::slice::from_raw_parts(
            DATA.as_ptr() as *const Bitboard,
            DATA.len() / std::mem::size_of::<Bitboard>(),
        )
    }
};
```

**Alternative (simpler):** Use `std::sync::OnceLock` with manual init:
```rust
static ROOK_TABLE: OnceLock<Box<[Bitboard]>> = OnceLock::new();

pub fn init_tables() {
    ROOK_TABLE.set(build_magic_table(...)).unwrap();
}
```
Then call `init_tables()` once at `main()` or library init. This
eliminates the per-access atomic check but adds a one-time init cost.
The `OnceLock::get()` call still does an atomic load, but it's a
predictable read-only load with no branch.

**Best fix:** Const-generic table with `const` initialization. The
magic tables are purely a function of the masks, magics, and index_bits
(which are all `const`). If we can make `build_magic_table()` a `const fn`,
the tables become `const` data in `.rodata` — zero access overhead.

**Effort:** ~60 lines for build-script approach; ~20 lines for
OnceLock approach.

---

### 6. Fused `attackers_to()` with Shared Slider Attacks (estimated: 2–3 %)

**Problem:** The current `attackers_to()` (lines 313–347) computes 6
separate attack blocks, each with its own function call and bitboard
loads. The same pattern is replicated inline in `legal()` (both
castling check at lines 727–757 and pseudo-royal loop at lines
869–896).

While item 2 from Plan 2 (perf_analysis3) eliminated the redundant
`queen_attacks()` calls (which internally called both `rook_attacks`
and `bishop_attacks` again), the `attackers_to()` itself still
computes `bishop_atk` and `rook_atk` separately:

```rust
let bishop_atk = attacks::bishop_attacks(sq, occupied);
attackers = attackers | (bishop_atk & (by_type[Bishop] | by_type[Queen]));

let rook_atk = attacks::rook_attacks(sq, occupied);
attackers = attackers | (rook_atk & (by_type[Rook] | by_type[Queen]));
```

These could share the slider attacks:

```rust
let rook_atk = attacks::rook_attacks(sq, occupied);
let bishop_atk = attacks::bishop_attacks(sq, occupied);
let slider_atk = rook_atk | bishop_atk;

attackers = (pawn_attacks(Color::White, sq)
             & self.pieces_color_pt(Color::Black, PieceType::Pawn))
          | (pawn_attacks(Color::Black, sq)
             & self.pieces_color_pt(Color::White, PieceType::Pawn))
          | (knight_attacks(sq) & self.by_type[PieceType::Knight as usize])
          | (bishop_atk & (self.by_type[PieceType::Bishop as usize]
                           | self.by_type[PieceType::Queen as usize]))
          | (rook_atk & (self.by_type[PieceType::Rook as usize]
                         | self.by_type[PieceType::Queen as usize]))
          | (king_attacks(sq) & self.by_type[PieceType::Commoner as usize]);
```

This is structurally identical to the FS `fastAttacks` path:
```cpp
return  (pawn_attacks_bb(~c, s)          & pieces(c, PAWN))
      | (attacks_bb<KNIGHT>(s)           & pieces(c, KNIGHT, ARCHBISHOP, CHANCELLOR))
      | (attacks_bb<  ROOK>(s, occupied) & pieces(c, ROOK, QUEEN, CHANCELLOR))
      | (attacks_bb<BISHOP>(s, occupied) & pieces(c, BISHOP, QUEEN, ARCHBISHOP))
      | (attacks_bb<KING>(s)             & pieces(c, KING, COMMONER));
```

**Synergy:** After implementing fused `attackers_to()`, we can use it
in the pseudo-royal loop and castling check instead of the inline
per-type code, eliminating ~30 lines of duplicated logic.

**Effort:** ~20 lines in `board.rs` for the fused rewrite + call-site
refactoring.

---

### 7. Perft Loop Optimization: StateInfo Reuse (estimated: 2–3 %)

**Problem:** Each `generate_legal()` call creates a fresh `StateInfo`
on the stack (line 232 of `movegen.rs`):
```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();  // ALLOCATED each call
    board.populate_state(&mut state);
    // ...
}
```

This `StateInfo::new()` zeros out a ~130-byte struct (including the
`[(Square, Piece); 9]` captured array). Zeroing 130 bytes per perft
node is a significant overhead — `__memset_zva64` shows up at 1.66 %
in the perf data.

**Fix:** Allow `generate_legal()` to accept a pre-allocated `StateInfo`
from the caller, which is the parent's `StateInfo` that's already on
the stack:

```rust
pub fn generate_legal_with_state(
    board: &Board,
    moves: &mut MoveList,
    state: &mut StateInfo,
) {
    board.populate_state(state);
    // ... rest same as generate_legal
}
```

Then in `perft()`:
```rust
pub fn perft(board: &mut Board, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    let mut moves = MoveList::new();
    let mut state = StateInfo::new();
    generate_legal_with_state(board, &mut moves, &mut state);
    if depth == 1 { return moves.len() as u64; }

    let mut total = 0u64;
    let mut child_state = StateInfo::new();
    for &m in moves.as_slice() {
        board.do_move(m, &mut state);
        total += perft(board, depth - 1);
        board.undo_move(m, &state);
    }
    total
}
```

Wait — the `StateInfo` must be saved per sibling because `undo_move()`
restores from it. But for the recursive call, we can reuse the
parent's `StateInfo` IF we save/restore it. Actually, in the perft
loop, the parent `state` is needed for `undo_move()`, and the child
needs its own state. So we need one extra `StateInfo` allocated once
at each depth level.

Better approach: pass a `state_buffer: &mut StateInfo` to `perft()`
that is reused for the recursive call:

```rust
pub fn perft(board: &mut Board, depth: u32, state_buffer: &mut StateInfo) -> u64 {
    if depth == 0 { return 1; }
    let mut moves = MoveList::new();
    generate_legal_with_state(board, &mut moves, state_buffer);
    if depth == 1 { return moves.len() as u64; }

    let mut total = 0u64;
    let mut saved_state = state_buffer.clone();  // shallow copy
    for &m in moves.as_slice() {
        board.do_move(m, state_buffer);
        total += perft(board, depth - 1, state_buffer);
        board.undo_move(m, &saved_state);
    }
    total
}
```

But `StateInfo` contains the 9-element captured array (18 bytes +
overhead = ~90 bytes). A clone every iteration is worse than creating fresh.

**Alternative:** The biggest win is just avoiding the `StateInfo::new()`
zeroing and re-allocation. We can do this by making `generate_legal`
write to a caller-provided `StateInfo` instead of its own stack allocation:

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList, state: &mut StateInfo) {
    board.populate_state(state);
    // ... filter using state
}
```

In the perft loop, the `StateInfo` is reused for the recursive call.
The key change: `StateInfo::new()` is called once at the top of perft,
not once per node.

**Effort:** ~10 lines (change `generate_legal` signature + update
caller).

---

### 8. Optimize `compute_checkers()` (estimated: 1–2 %)

**Problem:** `compute_checkers()` (lines 349–396) loops over ALL
commoners and for each one:
1. Computes `rook_attacks`, `bishop_attacks`, `queen_atk = rook | bishop`
2. ORs together 5 attacker type checks
3. Separately checks adjacent commoners for extinction pseudo-royal

For positions with exactly 1 commoner (the common case), the loop
runs once and is fairly efficient. For positions with >1 commoner
(rare, but happens with promotions), the cost doubles.

**Optimizations:**
- Only check pseudo-royal commoners (those whose count ≤ 1). If we
  have 2 commoners, only 1 is pseudo-royal. The other commoners cannot
  be "in check" because losing them doesn't end the game.
- Use the fused `attackers_to()` pattern (item 6) instead of separate
  per-type blocks.
- Move the adjacent commoner check into the main loop (after computing
  `king_attacks(ksq)`) to reuse the `king_attacks` result.

**Effort:** ~15 lines.

---

### 9. Legal Function Splitting (estimated: 1–2 %)

**Problem:** The `legal()` function is ~211 lines (lines 691–902)
and likely compiles to several thousand bytes of machine code. This
causes I-cache pressure: when `legal()` is called, it evicts other
hot code (like `do_move()`, `populate_state()`) from the L1 I-cache.

The Fairy-Stockfish `legal()` is also large (~250 lines), but it
benefits from function splitting via variant-specific blocks that
are conditionally compiled or have early returns. Our `legal()` has
no early return after the fast-path check — it always goes through
castling, blast computation, self-explosion, and pseudo-royal attack
check (but many of these are skipped by condition checks).

**Fix:** Split `legal()` into:
- `legal_castling()` — the castling pass-through check (lines 714–761)
- `legal_capture_post_blast()` — blast + self-explosion + pseudo-royal
  check (lines 763–898)
- Keep the main body small: fast-path → castling dispatch → capture
  dispatch → commoner/fallback

The compiler can then inline each helper more aggressively and
generate more compact code for the main `legal()` body, reducing
I-cache pressure.

**Effort:** ~40 lines (refactoring).

**Risk:** The function is complex; splitting could confuse the
compiler's ability to optimize across the split boundary. Must
verify with `perf` that the split doesn't regress.

---

### 10. Precomputed `check_squares[pt]` in StateInfo (estimated: 1–2 %)

**Problem:** To determine if a move gives check, the full attack
computation must be done. Fairy-Stockfish precomputes
`check_squares[pt]` = the attack mask of each piece type from the
king's perspective (for the side to move). This allows quick checking
of whether a piece of type `pt` at a given square would be giving
check to the opponent king.

In atomic chess, check detection is used in:
- `compute_pinned()` — determining if a piece blocks a check
- `legal()` — though atomic chess doesn't use the traditional
  "gives_check" mechanism, it's used internally for some edge cases

**Implementation:** Add `check_squares: [Bitboard; 6]` to `StateInfo`.
Compute in `populate_state()`:
```rust
for pt in 0..6 {
    state.check_squares[pt] = attacks_bb(pt, enemy_king, occupied);
}
```

Then in `compute_pinned()` and the pseudo-royal attack check, instead
of computing 5 separate attack functions per commoner, we can use
the precomputed check squares to narrow the candidate attackers.

**Effort:** ~20 lines.

---

### 11. Bitboard `is_one()` Method (estimated: 0.5–1 %)

**Problem:** The pattern `between.count() == 1` appears in
`compute_pinned()` and potentially in other places. This does a
full popcount (`u64.count_ones()` = 1 instruction on arm64) and
a comparison. The bit-twiddle `b != 0 && (b & (b - 1)) == 0` is
often faster because it avoids the popcount instruction (which has
3-cycle latency on Apple Firestorm, vs. 1 cycle for bit-twiddle).

**Fix:** Already identified — add `is_one()` to `Bitboard` and use
it in `compute_pinned()`.

```rust
pub fn is_one(self) -> bool {
    self.0 != 0 && (self.0 & (self.0 - 1)) == 0
}
```

**Note:** Actually checking: `more_than_one()` already exists but
`is_one()` does not. The check `!b.more_than_one() && !b.is_empty()`
is equivalent to `b.is_one()` but uses 2 method calls instead of 1.

**Effort:** ~3 lines.

---

### 12. Move Type Dispatch Optimization (estimated: 0.5 %)

**Problem:** `Move::move_type()` (types.rs:581–588) does:
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

This compiles to a shift, AND, and a table lookup or series of
comparisons. Since `MoveType` is a small enum with 4 variants, a
lookup table indexed by the 2-bit field could be faster:

```rust
static MOVE_TYPES: [MoveType; 4] = [
    MoveType::Normal,
    MoveType::Promotion,
    MoveType::EnPassant,
    MoveType::Castling,
];

pub fn move_type(self) -> MoveType {
    MOVE_TYPES[((self.0 >> 12) & 3) as usize]
}
```

But the `match` likely already compiles to a jump table, so this
optimization may be marginal.

**Effort:** ~10 lines.

---

## Summary: Sorted by Estimated Impact

| # | Optimization | Est. Speedup | Effort | Risk | Dependencies |
|---|-------------|-------------|--------|------|-------------|
| 1 | **Check evasion generation** | **8–20 %** | ~100 lines | Medium | None |
| 2 | **Cache `pseudoRoyals` bitboard in `StateInfo`** | **4–8 %** | ~15 lines | Low | None |
| 3 | **Precomputed `BetweenBB` + `LineBB` tables** | **3–6 %** | ~40 lines | Low (repeat) | None |
| 4 | **Optimize `compute_pinned()` with occupancy-delta** | **2–4 %** | ~25 lines | Low | Item 3 (for between_bb) |
| 5 | **Eliminate `LazyLock` for magic tables** | **2–4 %** | ~60 lines | Low | None |
| 6 | **Fused `attackers_to()` with shared sliders** | **2–3 %** | ~20 lines | None | None |
| 7 | **Perft loop: StateInfo reuse** | **2–3 %** | ~10 lines | None | None |
| 8 | **Optimize `compute_checkers()`** | **1–2 %** | ~15 lines | Low | Item 6 (fused attackers) |
| 9 | **Legal function splitting** | **1–2 %** | ~40 lines | Low | None |
| 10 | **Precomputed `check_squares[pt]`** | **1–2 %** | ~20 lines | Low | None |
| 11 | **Bitboard `is_one()` method** | **0.5–1 %** | ~3 lines | None | None |
| 12 | **Move type dispatch optimization** | **0.5 %** | ~10 lines | None | None |

### Cumulative potential

If all items were implemented and their impacts stacked multiplicatively
(optimistic scenario):

```
1 - (1-0.12)(1-0.06)(1-0.04)(1-0.03)(1-0.03)(1-0.02)(1-0.02)(1-0.01)(1-0.01)(1-0.01)(1-0.005)(1-0.005)
≈ 1 - 0.88×0.94×0.96×0.97×0.97×0.98×0.98×0.99×0.99×0.99×0.995×0.995
≈ 1 - 0.66
≈ 34 % total speedup from current baseline
```

This would bring total `verify_perft` time from **93.9 s → ~62 s**,
approaching the performance of the Fairy-Stockfish reference.

### Realistic scenario

In practice, Amdahl's Law applies, and items interact. A more realistic
estimate:

| Phase | Items | Est. speedup | Cumulative time |
|-------|-------|-------------|-----------------|
| Phase 1 (high) | 1, 2 | 10–20 % | ~75–85 s |
| Phase 2 (medium) | 3, 4, 5, 6 | 6–12 % | ~67–78 s |
| Phase 3 (polish) | 7, 8, 9, 10, 11, 12 | 3–6 % | ~63–75 s |

---

## Recommended Implementation Order

### Phase 1: Highest impact, strategic

1. **Item 2 — Cache pseudoRoyals bitboard.** This is a small, safe change
   (~15 lines) with 4–8 % estimated impact. It directly reduces redundant
   work in `legal()` without changing any algorithms.

2. **Item 6 — Fused `attackers_to()`. A straightforward refactoring
   (~20 lines) that consolidates the attack computation pattern. After
   this, `attackers_to` can be reused in `compute_checkers()` and the
   pseudo-royal loop.

3. **Item 1 — Check evasion generation.** The single biggest remaining
   optimization (~100 lines). This changes the move generation algorithm
   to generate only evasions when in check. Must be carefully verified
   against all 41 perft positions.

### Phase 2: Medium impact, moderate effort

4. **Item 4 — Optimize `compute_pinned()`.** After BetweenBB (item 3)
   is available, this is straightforward. The occupancy-delta fix also
   improves correctness.

5. **Item 3 — Precomputed BetweenBB.** 64×64 lookup table. The key is
   avoiding `LazyLock` overhead. Use `const` initialization or a build
   script.

6. **Item 5 — Eliminate LazyLock for magic tables.** Requires either
   a build script or `OnceLock` with manual init. Worth combining with
   item 3 if similar techniques are used.

### Phase 3: Polish

7. **Item 7 — StateInfo reuse.** Small signature change to
   `generate_legal()` that eliminates per-node allocation.

8. **Item 8 — Optimize `compute_checkers()`.** After items 2 and 6,
   this reduces further.

9. **Item 9 — Legal function splitting.** Low priority but helps I-cache.

10. **Items 10–12 — Low-hanging micro-optimizations.**

---

## Out-of-the-Box Ideas

### A. Parallel perft
The perft tree is embarrassingly parallel at the root level. Each
root move's subtree can be computed independently. For `verify_perft`,
we could parallelize the top N moves using `rayon` or `std::thread`.
This would give a ~N× speedup on multicore machines — but only for
the perft benchmark, not for the actual move generator performance.

**Relevance:** Not applicable for movegen optimization. Only for
benchmarking.

### B. Compile-time magic table generation
Rust's `const fn` can now be quite sophisticated. If we can make the
`sliding_attack()` function and the carry-rippler subset enumeration
`const fn`, the entire magic tables (ROOK_TABLE and BISHOP_TABLE)
could be computed at compile time and embedded directly in the binary's
`.rodata` section. This would:
- Eliminate the `LazyLock` check entirely (item 5)
- Eliminate all initialization cost
- Allow the compiler to optimize table access patterns

**Challenge:** The magic tables are large (~120K entries for rook,
~8K for bishop). Compile-time computation of 128K table entries is
possible but may significantly increase compile time.

### C. PEXT on x86_64
The PEXT (Bit Manipulation Extension) path already exists in
`src/pext.rs`, gated behind `#[cfg(target_arch = "x86_64")]`. On
x86_64 with BMI2, PEXT replaces `(occ & mask) * magic >> shift`
with a single `pext` instruction + table lookup. This is faster
than magic multiplication on CPUs with BMI2 (Intel Haswell+, AMD
Excavator+).

Currently, the code compiles PEXT support on any x86_64 but
runtime-dispatch based on CPUID. This adds a `LazyLock<Impl>` check
on every call. If we could make the PEXT/magic decision at compile
time (e.g., via cargo features), the runtime dispatch overhead would
be eliminated on x86_64 as well.

**Relevance:** Not applicable on arm64 (current test platform), but
important for x86_64 deployments.

### D. Hybrid BetweenBB / LineBB / Square distance table
Instead of just BetweenBB, a combined table of `(between, line, distance,
aligned)` could serve as a hot lookup cache for multiple computations
(between_bb, line_bb, aligned, more_than_one on between sets). A
single 64×64 table of a compact struct (3×Bitboard + u8 = 25 bytes ×
4096 = 102 KB) would be ~L2-cache resident and serve all geometry queries.

**Relevance:** Beyond the current scope, but could eliminate 3 separate
loop-based functions.

### E. Negative refactoring: Restructure `legal()` as two-phase check
The current `legal()` first computes the post-blast `occupied` and then
checks pseudo-royal safety. These are independent phases. If we split
`legal()` into:
1. `compute_post_move_state(m, &post)` — compute post-blast occupied,
   surviving pieces, pseudo-royal candidates
2. `check_pseudo_royal_safety(post) & attackers_to(...)` — check safety

Then for commoner moves (which are rare — ≤1 per position), the costly
phase 2 can be specialized for "last commoner check", while for
non-commoner moves, only enemy pseudo-royal safety matters.

This is essentially what we already do (the "if our_pr_count <= 1" check
at line 844), but the structure is implicit rather than explicit.

**Relevance:** Marginal improvement; mainly for clarity.

---

## Items Already Completed (Plans 1–4)

| Plan | Change | Speedup |
|------|--------|---------|
| ✅ Plan 1 (perf_analysis) | Stack-allocated `MoveList` | 11.1 % |
| ✅ Plan 2 (perf_analysis) | Inline legal filtering | 2.9 % |
| ✅ Plan 1 (perf_analysis3) | MagicEntry + transmute accessors | 9.3 % |
| ✅ Plan 2 (perf_analysis3) | queen_attacks elimination + inline + field order | 3.3 % |

## Items Attempted and Worth Revisiting

| Item | Why reverted or deferred | Why revisit now |
|------|--------------------------|----------------|
| Precomputed `between_bb` table | `LazyLock` overhead offset gains (Plan 2, perf_analysis) | Use `const` or `OnceLock` instead of `LazyLock` to avoid per-access overhead |
| `commoners()` caching | Compiler already CSE'd repeated calls (Plan 2) | We propose caching the full `pseudoRoyals` bitboard (not individual calls), which adds new state to `StateInfo` |

## Lessons Learned

1. **The 80/20 rule holds.** Check evasion generation (item 1) is the
   single biggest remaining optimization — it changes the algorithm so
   that 50–80 % fewer moves are generated in check positions. Combined
   with pseudo-royal caching (item 2), these two changes could deliver
   10–20 % total speedup.

2. **LazyLock is the enemy of hot paths.** Every `LazyLock` check adds
   an atomic load and a predictable branch. On modern CPUs, the branch
   is almost always correctly predicted, but the load adds latency.
   Prefer `const` initialization where possible, `OnceLock::get()` as
   a fallback.

3. **Fairy-Stockfish's advantage is structural, not algorithmic.**
   The legal() logic is essentially the same. The difference is in
   data structure design (packed `Magic` struct, cached bitboards in
   `StateInfo`, fused attacker expressions) and move generation
   strategy (evasions vs. non-evasions dispatch).

4. **The Rust compiler is already excellent at inlining and CSE.**
   Manual caching of individual values (like `commoners(c)` in local
   variables) showed no benefit in Plan 2. The wins come from:
   (a) Algorithmic changes (evasion generation)
   (b) Cache-friendly data layout (MagicEntry, StateInfo ordering)
   (c) Eliminating repeated computation entirely (pseudoRoyals caching)

5. **perft() overhead is significant but expected.** The recursive
   driver function naturally takes a large share of samples because
   it contains the loop, function calls, and branching overhead of
   the perft search. Optimizing `generate_legal()`, `do_move()`, and
   `undo_move()` directly improves perft performance by reducing the
   work done per iteration.

6. **The tactical positions tell the story.** Test #13 (14.098 s),
   #33 (13.208 s), and #2 (11.827 s) together account for ~42 % of
   total runtime. Any optimization that disproportionately improves
   these positions (evasion generation, pseudo-royal caching) will
   show large overall speedups.
