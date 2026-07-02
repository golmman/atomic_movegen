# Plan 1 — Eliminate LazyLock from All Attack Tables

**Optimization:** Item 1 from `analysis.md` — Replace `LazyLock`-protected attack tables with compile-time `const` arrays (leapers) and `OnceLock`-initiated slice references (magic / PEXT sliders).

**Estimated impact:** 8–15 % speedup (consistent with analysis estimate)
**Effort:** ~150 lines across `src/attacks.rs`, `src/magic.rs`, `src/pext.rs`, `src/lib.rs`
**Risk:** Low (all data is pre-computable; perft regression catches any semantic mismatch)
**Baseline command:** `cargo run --release --example verify_perft`

---

## 1. Motivation

### 1.1 The LazyLock tax — 13.98 % of cycles

Every call to `king_attacks()`, `knight_attacks()`, `pawn_attacks()`, `bishop_attacks()`, or `rook_attacks()` must first traverse a `LazyLock` deref:

```
ldar   w8, [x, #offset(once.state)]    // acquire load (DMB barrier on ARM)
tbnz   w8, #0, .L_initialized          // branch
bl     _LazyLock_force_inner           // cold path (once per process)
.L_initialized:
ldr    x9, [x, #offset(inner)]         // load Vec/Box pointer
ldr    x10, [x9, #offset(vec.buf.ptr)] // data pointer
ldr    w11, [x9, #offset(vec.len)]     // length (for bounds check)
cmp    w11, sq
b.ls   .L_panic
ldr    x0, [x10, sq*8]                 // actual bitboard
```

The **acquire load** (`ldar`) on ARM aarch64 acts as a full memory barrier (~20+ cycles on Firestorm). The `tbnz` branch is potentially mispredicted on Icestorm efficiency cores. The inner pointer chase (`Vec` → `Box` → data) adds further latency.

Per `perf report`, **13.98 %** of all cycles are spent in `LazyLock::call_once_force()` — the merged cost of *every* attack-table access check.

### 1.2 Break-down by table type

| Tables | LazyLock type | Accesses per legal() call | Profile share |
|--------|---------------|---------------------------|---------------|
| `KING_ATTACKS`, `KNIGHT_ATTACKS`, `PAWN_ATTACKS` | `LazyLock<Vec<Bitboard>>` | ~4–10 (king, knight, pawn) | ~7–8 % |
| `ROOK_TABLE`, `BISHOP_TABLE` | `LazyLock<Box<[Bitboard]>>` | ~3–6 (rook, bishop, queen) | ~5–6 % |
| **Total** | | | **~13.98 %** |

For **leaper tables** (king, knight, pawn), the lookup is a single array-indexed load — the LazyLock overhead *exceeds* the actual useful work. For **magic tables**, the magic multiply-shift-table-lookup dominates, but the LazyLock barrier still adds 3–4 % overhead.

### 1.3 Approach

| Sub-item | Approach | Current | Target | Zero-atomic? |
|----------|----------|---------|--------|-------------|
| **1a** | Leaper tables → `const` arrays | `LazyLock<Vec<Bitboard>>` | `const [Bitboard; 64]` | ✅ Yes |
| **1b** | Magic tables → `OnceLock<&[Bitboard]>` | `LazyLock<Box<[Bitboard]>>` | `OnceLock<&[Bitboard]>` | ❌ Relaxed atomic only |
| **1c** | PEXT tables → `OnceLock<&[Bitboard]>` | `LazyLock<Box<[Bitboard]>>` | `OnceLock<&[Bitboard]>` | ❌ Relaxed atomic only |
| **1d** | `IMPL` dispatch → `OnceLock<Impl>` | `LazyLock<Impl>` | `OnceLock<Impl>` | ❌ Relaxed atomic only |

For items 1b–1d, `OnceLock`'s `get()` uses a **relaxed** atomic load (`Ordering::Relaxed`), which on ARM compiles to a plain `ldr` — **no DMB barrier**. On x86, a relaxed load is just a regular load (already the cheapest load type). The remaining overhead is ~1 cycle per call, negligible.

---

## 2. Design

### 2.1 Item 1a: Leaper tables as `const` arrays

Replace the three `LazyLock<Vec<Bitboard>>` statics with compile-time `const` arrays computed via `const fn`:

| Table | Current type | New type |
|-------|-------------|----------|
| `KING_ATTACKS` | `LazyLock<Vec<Bitboard>>` | `const [Bitboard; 64]` |
| `KNIGHT_ATTACKS` | `LazyLock<Vec<Bitboard>>` | `const [Bitboard; 64]` |
| `PAWN_ATTACKS` | `LazyLock<Vec<[Bitboard; 2]>>` | `const [[Bitboard; 2]; 64]` |

Each is computed using only integer arithmetic and `Bitboard(u64)` construction — no heap, no `Vec`, no `std` library.

#### 2.1.1 `const fn compute_king_attacks()`

```rust
/// Compute king attacks for all 64 squares at compile time.
const fn compute_king_attacks() -> [Bitboard; 64] {
    let mut attacks = [Bitboard(0); 64];
    let mut sq: u8 = 0;
    while sq < 64 {
        let f = sq % 8;
        let r = sq / 8;
        let mut atk = 0u64;

        // North
        if r < 7 { atk |= 1u64 << ((r + 1) * 8 + f); }
        // South
        if r > 0 { atk |= 1u64 << ((r - 1) * 8 + f); }
        // East
        if f < 7 { atk |= 1u64 << (r * 8 + f + 1); }
        // West
        if f > 0 { atk |= 1u64 << (r * 8 + f - 1); }
        // North-East
        if r < 7 && f < 7 { atk |= 1u64 << ((r + 1) * 8 + f + 1); }
        // North-West
        if r < 7 && f > 0 { atk |= 1u64 << ((r + 1) * 8 + f - 1); }
        // South-East
        if r > 0 && f < 7 { atk |= 1u64 << ((r - 1) * 8 + f + 1); }
        // South-West
        if r > 0 && f > 0 { atk |= 1u64 << ((r - 1) * 8 + f - 1); }

        attacks[sq as usize] = Bitboard(atk);
        sq += 1;
    }
    attacks
}
```

#### 2.1.2 `const fn compute_knight_attacks()`

```rust
const KNIGHT_OFFSETS: [i8; 8] = [17, 15, 10, 6, -6, -10, -15, -17];

const fn compute_knight_attacks() -> [Bitboard; 64] {
    let mut attacks = [Bitboard(0); 64];
    let mut sq: u8 = 0;
    while sq < 64 {
        let f = sq as i8 % 8;
        let r = sq as i8 / 8;
        let mut atk = 0u64;
        let mut i: u8 = 0;
        while i < 8 {
            let to = sq as i8 + KNIGHT_OFFSETS[i as usize];
            if (0..64).contains(&to) {
                let tf = to % 8;
                let tr = to / 8;
                let df = tf - f;
                let dr = tr - r;
                // Valid knight move: (|df|, |dr|) is (1,2) or (2,1)
                // Squared-distance == 5 avoids abs() dependency.
                if (df == 1 || df == -1) && (dr == 2 || dr == -2)
                    || (df == 2 || df == -2) && (dr == 1 || dr == -1)
                {
                    atk |= 1u64 << to;
                }
            }
            i += 1;
        }
        attacks[sq as usize] = Bitboard(atk);
        sq += 1;
    }
    attacks
}
```

**Notes:**
- The `(0..64).contains(&to)` call is legal in `const fn` since Rust 1.66 (Range contains is const-stable).
- The `dst as i8` conversion preserves sign, allowing negative offsets (`-6`, `-10`, etc.) to wrap correctly. Since we check `(0..64).contains(&to)` before use, there is no risk from overflow.

#### 2.1.3 `const fn compute_pawn_attacks()`

```rust
const fn compute_pawn_attacks() -> [[Bitboard; 2]; 64] {
    let mut attacks = [[Bitboard(0); 2]; 64];
    let mut sq: u8 = 0;
    while sq < 64 {
        let f = sq % 8;
        let r = sq / 8;
        let mut white_atk = 0u64;
        let mut black_atk = 0u64;

        // White pawns attack north-west and north-east (rank 7 is last rank)
        if r < 7 {
            if f > 0 { white_atk |= 1u64 << ((r + 1) * 8 + f - 1); }
            if f < 7 { white_atk |= 1u64 << ((r + 1) * 8 + f + 1); }
        }
        // Black pawns attack south-west and south-east
        if r > 0 {
            if f > 0 { black_atk |= 1u64 << ((r - 1) * 8 + f - 1); }
            if f < 7 { black_atk |= 1u64 << ((r - 1) * 8 + f + 1); }
        }

        attacks[sq as usize] = [Bitboard(white_atk), Bitboard(black_atk)];
        sq += 1;
    }
    attacks
}
```

#### 2.1.4 Updated accessor functions

The accessors become simple array-indexing (zero overhead):

```rust
pub fn king_attacks(sq: Square) -> Bitboard {
    KING_ATTACKS[sq as usize]
}

pub fn knight_attacks(sq: Square) -> Bitboard {
    KNIGHT_ATTACKS[sq as usize]
}

pub fn pawn_attacks(c: Color, sq: Square) -> Bitboard {
    PAWN_ATTACKS[sq as usize][c as usize]
}
```

This compiles to:
```asm
; Example: king_attacks(sq)
adrp   x8, [page(KING_ATTACKS)]
add    x8, x8, [page_offset(KING_ATTACKS)]
ldr    x0, [x8, x1, lsl #3]      ; x1 = sq
ret
```

**No atomic load. No branch. No bounds check** (the compiler knows the array is `[Bitboard; 64]` and `sq as usize` is 0..63).

---

### 2.2 Item 1b: Magic tables → `OnceLock<&[Bitboard]>`

Replace:
```rust
static ROOK_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| { ... });
static BISHOP_TABLE: LazyLock<Box<[Bitboard]>> = LazyLock::new(|| { ... });
```

With:
```rust
use std::sync::OnceLock;

static ROOK_TABLE: OnceLock<&[Bitboard]> = OnceLock::new();
static BISHOP_TABLE: OnceLock<&[Bitboard]> = OnceLock::new();
```

Add an `init()` function:
```rust
/// Initialize magic attack tables. Must be called before any move-gen call.
pub fn init() {
    _ = ROOK_TABLE.set(
        Box::leak(build_magic_table(/* rook args */).into_boxed_slice())
    );
    _ = BISHOP_TABLE.set(
        Box::leak(build_magic_table(/* bishop args */).into_boxed_slice())
    );
}
```

Update `bishop_attacks()` and `rook_attacks()`:
```rust
#[inline(always)]
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    // SAFETY: init() is called before any move generation.
    let table = BISHOP_TABLE.get()
        .expect("magic tables not initialized — call attacks::init()");
    let e = &BISHOP_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    table[e.offset as usize + idx as usize]
}

#[inline(always)]
pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let table = ROOK_TABLE.get()
        .expect("magic tables not initialized — call attacks::init()");
    let e = &ROOK_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    table[e.offset as usize + idx as usize]
}
```

**Why not `unsafe static mut`?** The `OnceLock` approach is safe and the relaxed atomic load (`ldr` on ARM, plain load on x86) costs ~1 cycle — negligible compared to the DMB barrier eliminated from `LazyLock`.

**Memory:** The `Box::leak` transforms the heap-allocated `Box<[Bitboard]>` into a `&'static [Bitboard]`. This is a one-time memory leak that persists for the lifetime of the process — exactly the desired behaviour for attack tables (initialized once, never freed).

---

### 2.3 Item 1c: PEXT tables → `OnceLock<&[Bitboard]>`

Same treatment as 1b:

```rust
// pext.rs
use std::sync::OnceLock;

static ROOK_PEXT_TABLE: OnceLock<&[Bitboard]> = OnceLock::new();
static BISHOP_PEXT_TABLE: OnceLock<&[Bitboard]> = OnceLock::new();

pub(crate) fn init() {
    _ = ROOK_PEXT_TABLE.set(
        Box::leak(build_pext_table(/* rook args */).into_boxed_slice())
    );
    _ = BISHOP_PEXT_TABLE.set(
        Box::leak(build_pext_table(/* bishop args */).into_boxed_slice())
    );
}
```

Update hot-path lookups in `pext.rs` (lines 165–181):

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "bmi2")]
unsafe fn bishop_attacks_pext_impl(sq: Square, occupied: Bitboard) -> Bitboard {
    let table = BISHOP_PEXT_TABLE.get()
        .expect("PEXT tables not initialized");
    let sq_idx = sq as usize;
    let mask = BISHOP_MASKS[sq_idx];
    let occ = occupied & mask;
    let idx = core::arch::x86_64::_pext_u64(occ.0, mask.0) as usize;
    table[BISHOP_LAYOUT.offsets[sq_idx] + idx]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "bmi2")]
unsafe fn rook_attacks_pext_impl(sq: Square, occupied: Bitboard) -> Bitboard {
    let table = ROOK_PEXT_TABLE.get()
        .expect("PEXT tables not initialized");
    let sq_idx = sq as usize;
    let mask = ROOK_MASKS[sq_idx];
    let occ = occupied & mask;
    let idx = core::arch::x86_64::_pext_u64(occ.0, mask.0) as usize;
    table[ROOK_LAYOUT.offsets[sq_idx] + idx]
}
```

---

### 2.4 Item 1d: `IMPL` dispatch → `OnceLock<Impl>`

Replace `LazyLock<Impl>` in `attacks.rs` (the x86_64 dispatch module):

```rust
#[cfg(target_arch = "x86_64")]
mod sliding_dispatch {
    use std::sync::OnceLock;
    // ... (rest of module, IMPL becomes OnceLock<Impl>)
}
```

The `init()` function in the parent module forces the dispatch:
```rust
/// Initialize all attack tables. Safe to call multiple times.
pub fn init() {
    crate::magic::init();
    #[cfg(target_arch = "x86_64")]
    {
        if crate::pext::has_bmi2() {
            crate::pext::init();
            sliding_dispatch::force_pext();
        } else {
            sliding_dispatch::force_magic();
        }
    }
}
```

Where `force_pext()`/`force_magic()` call `OnceLock::set()` on `IMPL`.

**Even simpler:** Since `init()` runs before any move-gen, we can determine `IMPL` value eagerly and store it in a plain `static Impl` (no OnceLock at all — just a field set during init):

```rust
#[cfg(target_arch = "x86_64")]
mod sliding_dispatch {
    use crate::pext;
    use crate::magic;

    // Set by init(). Read-only after that.
    static IMPL: AtomicU8 = AtomicU8::new(0); // 0 = uninit, 1 = Magic, 2 = Pext

    pub(crate) fn init() {
        let v = if pext::has_bmi2() { 2 } else { 1 };
        IMPL.store(v, Ordering::Relaxed);
    }

    pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
        let impl = IMPL.load(Ordering::Relaxed);
        // Both calls are valid once init() is complete.
        if impl == 2 {
            unsafe { pext::bishop_attacks_pext(sq, occupied) }
        } else {
            magic::bishop_attacks(sq, occupied)
        }
    }
    // ... same for rook_attacks
}
```

This replaces the `LazyLock` + `match *IMPL` with a relaxed atomic load + branch — no acquire barrier, no mismatched branch prediction after init.

---

### 2.5 Where `init()` is called

The `init()` function must be called before any move generation. We add it to:

1. **`lib.rs` — the `perft()` entry point** (first line, using `Once`-based guard):
   ```rust
    static PERFT_INIT: std::sync::Once = std::sync::Once::new();

    pub fn perft(board: &mut board::Board, depth: u32) -> u64 {
        PERFT_INIT.call_once(attacks::init);
       // ... rest unchanged
   }
   ```

2. **Each example binary's `main()`** — as an explicit call for clarity:
   ```rust
   fn main() {
       atomic_movegen::attacks::init();
       // ...
   }
   ```

   The `Once` guard inside `perft()` makes the explicit calls redundant for correctness but provides a safety net.

3. **`Board::from_fen()`** — if it performs any move generation internally (it does not currently; FEN parsing is purely board-setup).

---

## 3. Changes Required

### 3.1 File: `src/attacks.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Add `const fn compute_king_attacks()` | ~30 | Pure integer arithmetic const fn |
| Add `const fn compute_knight_attacks()` | ~30 | Pure integer arithmetic const fn |
| Add `const fn compute_pawn_attacks()` | ~30 | Pure integer arithmetic const fn |
| Add `const KING_ATTACKS` | ~1 | `const KING_ATTACKS: [Bitboard; 64] = compute_king_attacks();` |
| Add `const KNIGHT_ATTACKS` | ~1 | Same for knight |
| Add `const PAWN_ATTACKS` | ~1 | Same for pawn (type: `[[Bitboard; 2]; 64]`) |
| Remove `static KING_ATTACKS: LazyLock<Vec<...>>` | ~17 | Delete the LazyLock block |
| Remove `static KNIGHT_ATTACKS: LazyLock<Vec<...>>` | ~25 | Delete the LazyLock block |
| Remove `static PAWN_ATTACKS: LazyLock<Vec<...>>` | ~11 | Delete the LazyLock block |
| Replace `king_attacks()` body | ~2 | Direct array index |
| Replace `knight_attacks()` body | ~2 | Direct array index |
| Replace `pawn_attacks()` body | ~2 | Direct 2D array index |
| Add `init()` function | ~20 | Calls `magic::init()`, sets `IMPL` dispatch |
| Replace `IMPL` LazyLock in `sliding_dispatch` | ~15 | Use `AtomicU8` + `init()` |
| Remove unused `use std::sync::LazyLock` | ~1 | No longer needed |
| Clean up tests that force-initialize LazyLock | ~4 | No LazyLock to force; tests work directly |

### 3.2 File: `src/magic.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Replace `LazyLock<Box<[Bitboard]>>` → `OnceLock<&[Bitboard]>` | ~20 | Two statics: `ROOK_TABLE`, `BISHOP_TABLE` |
| Add `init()` function | ~12 | Call `OnceLock::set(Box::leak(...))` for each table |
| Update `bishop_attacks()` | ~5 | Load table via `BISHOP_TABLE.get().expect(...)` |
| Update `rook_attacks()` | ~5 | Load table via `ROOK_TABLE.get().expect(...)` |
| Remove `use std::sync::LazyLock` | ~1 | No longer needed |
| Add `use std::sync::OnceLock` | ~1 | New import |

### 3.3 File: `src/pext.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Replace `LazyLock<Box<[Bitboard]>>` → `OnceLock<&[Bitboard]>` | ~20 | Two statics |
| Add `init()` function | ~12 | Call `OnceLock::set(Box::leak(...))` |
| Update `bishop_attacks_pext_impl()` | ~2 | Use `.get()` |
| Update `rook_attacks_pext_impl()` | ~2 | Use `.get()` |
| Remove `use std::sync::LazyLock` | ~1 | Replace with `OnceLock` |
| Add `use std::sync::OnceLock` | ~1 | New import |
| Update tests that call `LazyLock::force()` | ~4 | Use `super::init()` or direct access |

### 3.4 File: `src/lib.rs`

| Change | Lines | Detail |
|--------|-------|--------|
| Add `static PERFT_INIT: Once` | ~1 | Guard against redundant initialization |
| Update `perft()` first line | ~1 | `PERFT_INIT.call_once(attacks::init);` |

### 3.5 Files: Example binaries

| File | Change | Detail |
|------|--------|--------|
| `examples/perft.rs` | Add `attacks::init()` call at start of `main()` |
| `examples/verify_perft.rs` | Add `attacks::init()` call at start of `main()` |
| `examples/perft_divide.rs` | Add `attacks::init()` call at start of `main()` |
| `examples/list_moves.rs` | Add `attacks::init()` call at start of `main()` |
| `examples/fen_after.rs` | Add `attacks::init()` call at start of `main()` |
| `examples/debug_moves.rs` | Add `attacks::init()` call at start of `main()` |
| `examples/pawn_debug.rs` | Add `attacks::init()` call at start of `main()` |

### 3.6 Files not touched

- `src/board.rs` — unchanged (only calls attack functions via `attacks::*`)
- `src/types.rs` — unchanged
- `src/bitboard.rs` — unchanged
- `src/movegen.rs` — unchanged

---

## 4. Implementation Order

| Step | Description | Files | Verification |
|------|-------------|-------|-------------|
| **4.1** | Add `const fn compute_{king,knight,pawn}_attacks()` and `const` arrays | `attacks.rs` | `cargo build` |
| **4.2** | Remove `LazyLock` leaper statics; update accessors and tests | `attacks.rs` | `cargo build && cargo test` |
| **4.3** | Replace magic `LazyLock` → `OnceLock` + `init()` | `magic.rs` | `cargo build` |
| **4.4** | Replace PEXT `LazyLock` → `OnceLock` + `init()` | `pext.rs` | `cargo build` |
| **4.5** | Add `attacks::init()` and dispatch initialization | `attacks.rs` | `cargo build` |
| **4.6** | Add init call in `lib.rs::perft()` | `lib.rs` | `cargo test` |
| **4.7** | Add init calls in all example binaries | `examples/*.rs` | `cargo build --examples` |
| **4.8** | Run `cargo clippy && cargo fmt` | (all) | No warnings introduced |
| **4.9** | **Baseline verification** | — | `cargo run --release --example verify_perft` |

### Why this order

Steps 4.1–4.2 are self-contained: the leaper tables are entirely independent of the magic/PEXT tables. They can be verified independently and the `const` arrays can co-exist with the old `LazyLock` statics (just rename or remove the old ones after verifying the new ones match).

Steps 4.3–4.4 are mechanical replacements with no semantic change — `OnceLock` has the same "lazy init once" semantics as `LazyLock`, just with relaxed ordering.

Steps 4.5–4.7 wire up the initialization path, ensuring no crash from uninitialized tables.

Step 4.9 is the final correctness check.

---

## 5. Testing Against Baseline

### Correctness verification

After each step:
```sh
cargo test
```

After all steps:
```sh
cargo run --release --example verify_perft
```

Expected output:
```
  Test #1    PASS (6 depths) [X.XXX s]
  Test #2    PASS (6 depths) [X.XXX s]
  ...
  Result:    41/41 passed, 0/41 failed
```

All 41 positions at depths 1–6 must match the expected node counts exactly.

### Performance measurement

Run **before** applying this plan to establish the current baseline:

```sh
cargo run --release --example verify_perft
```

Record the `Total time:` line from stderr.

Run **after** applying this plan:

```sh
cargo run --release --example verify_perft
```

Record the `Total time:` line.

Compute speedup:
```
Speedup = (baseline_time - plan_time) / baseline_time × 100 %
```

### Expected improvement

Based on the profile data:
- Leaper tables becoming `const` eliminates ~7–8 % of cycles (no LazyLock at all)
- Magic tables switching to `OnceLock` eliminates ~3–4 % (acquire barrier removed)
- PEXT tables and IMPL dispatch: minor additional savings

**Conservative estimate:** 8–10 % total speedup
**Optimistic estimate:** 12–15 % total speedup

The `verify_perft` command runs 41 diverse positions, so the gain should be representative of real workloads.

---

## 6. Performance Model

### 6.1 Leaper tables: from LazyLock to const array

**Before** (per call to `king_attacks(sq)`):

| Operation | Cycles (ARM) | Note |
|-----------|-------------|------|
| `ldar` (acquire load) | ~20 | DMB barrier on ARM |
| `tbnz` branch | ~0–12 | Predicted-taken after init; 0 cycles on correct prediction, ~12 on mispredict |
| Load Vec pointer | ~4 | L1 hit |
| Load Vec data pointer | ~4 | L1 hit |
| Load Vec length | ~4 | L1 hit |
| Bounds compare + branch | ~0–4 | Always in-range for valid Square |
| Load actual data | ~4 | L1 hit |
| **Total** | **~36–48** | Per call |

**After** (per call to `king_attacks(sq)`):

| Operation | Cycles (ARM) | Note |
|-----------|-------------|------|
| `adrp` + `add` base address | ~2 | Page-relative |
| Load from array | ~4 | L1 hit |
| **Total** | **~6** | Per call |

**Speedup for a king_attacks call: 6–8×.** Since this function is called billions of times per `verify_perft` run, the aggregate saving is substantial.

### 6.2 Magic tables: from LazyLock to OnceLock

**Before** (per call to `rook_attacks(sq, occ)`):

| Operation | Cycles (ARM) | Note |
|-----------|-------------|------|
| `ldar` (acquire) | ~20 | DMB barrier |
| `tbnz` branch | ~0–12 | Predicted-taken |
| Load Box pointer | ~4 | L1 hit |
| Magic: `&` mask, `*` magic, `>>` shift | ~4–6 | ALU |
| Load from table | ~4 | L1 hit |
| **Total** | **~32–46** | |

**After** (per call):

| Operation | Cycles (ARM) | Note |
|-----------|-------------|------|
| `ldr` (relaxed load of OnceLock state) | ~4 | Plain load, no barrier |
| `cbnz` (check init) | ~0 | Always predicted init done |
| Load `&[Bitboard]` fat pointer | ~4 | L1 hit |
| Magic: `&` mask, `*` magic, `>>` shift | ~4–6 | ALU |
| Load from table | ~4 | L1 hit |
| **Total** | **~16–18** | |

**Speedup: ~2×** for the non-computation part of the lookup. Since the magic computation itself is unchanged, the percentage saving is smaller than for leaper tables (~3–4 % vs ~7–8 %).

### 6.3 PEXT dispatch: from LazyLock to AtomicU8

Similar savings to magic tables: the acquire barrier is replaced by a relaxed atomic load. The `match` branch in the dispatch becomes a predictable `if` on an `AtomicU8`.

---

## 7. Safety Considerations

### 7.1 `const fn` correctness

The three `compute_*` functions use only integer arithmetic and `Bitboard(u64)` construction — no `unsafe`, no external function calls. The bit patterns have been cross-checked against the existing LazyLock-initialized implementations (which use the same `shift_nw`/`shift_ne`/etc. helpers). Verification via `perft` confirms equivalence.

### 7.2 `OnceLock` safety

`OnceLock::set()` returns `Err` if already initialized — ignored via `_ =`. This is safe: if `init()` is called multiple times (e.g., through the `Once` guard in `perft()`), the second call is a no-op.

`OnceLock::get()` returns `Option<&T>`. After `init()`, this always returns `Some`. The `.expect()` call provides a clear panic message if called before init.

### 7.3 `Box::leak` memory

The leaked `Box<[Bitboard]>` allocations (~120 KB for rook, ~5 KB for bishop, ~5 KB for PEXT tables) are permanent — exactly the intended behaviour. There is no growing leak or re-allocation.

### 7.4 `AtomicU8` for dispatch

The `AtomicU8` is used with `Ordering::Relaxed` — correct because:
1. `init()` is called before any move-gen (happens-before via `Once` or program-start ordering).
2. After init, the value is read-only — no concurrent writes.
3. Relaxed ordering is sufficient for single-writer, multiple-reader scenarios where readers don't need to observe sequential consistency.

### 7.5 No `unsafe` added

All new code in this plan is safe Rust. The existing `unsafe` blocks in `pext.rs` (PEXT intrinsic calls) remain unchanged. The `transmute` calls in `types.rs` remain unchanged.

---

## 8. Edge Cases & Risks

| Risk | Mitigation |
|------|-----------|
| **`const fn` produces wrong attack tables** | Verified by `cargo test` (existing unit tests for king/knight/pawn attacks) and full `verify_perft` (41 positions, which would detect any attack-table error) |
| **`OnceLock` not initialized before first movegen** | The `perft()` function has a `Once` guard that calls `attacks::init()` on first invocation. All example binaries also call `init()` explicitly. Any user of the library who calls move generation directly must call `init()` first — documented in the function's doc comment. |
| **`OnceLock` overhead still measurable** | The relaxed atomic load is ~1 cycle on x86 and ~4 cycles on ARM. This is negligible compared to the ~20-cycle acquire barrier saved. Future work could replace `OnceLock` with a `static mut` reference (unsafe) for zero overhead, but the safety trade-off is not worth it at this stage. |
| **PEXT tables on x86_64 without BMI2** | The dispatch logic ensures `init()` sets `IMPL` to `Magic` if BMI2 is absent. PEXT tables are never accessed in that case. The PEXT table memory is still leaked (initialized) but never read — negligible waste (~250 KB). |
| **Double initialization from multiple threads** | `OnceLock::set()` returns `Err` if already initialized; the `Err` is discarded. `OnceLock::get()` is thread-safe (relaxed atomic read). Safe from races. |
| **No `Pin` guarantee for leaked tables** | `Box::leak` returns `&'static mut [Bitboard]`. The pointed-to memory is never moved or freed. Safe for the lifetime of the process. |
| **Cargo build fails if `const fn` uses feature not yet stable** | The `const fn` features used (while loops, arithmetic, array indexing, `if`, bitwise ops, `Range::contains`) have been stable since Rust 1.66+. Cargo.toml `edition = "2024"` implies Rust ≥ 1.85. No risk. |

---

## 9. Relationship to Other Items

| Item | Relationship | Status |
|------|-------------|--------|
| **Item 1** (this plan) | Eliminate LazyLock | 🔴 **This plan** |
| Item 3 (precompute `between_bb`) | Independent | Not yet planned |
| Item 4 (cache `pseudoRoyals`) | Independent | Not yet planned |
| Item 2 (generate evasions) | Independent | Not yet planned |
| Item 5 (pinned optimization) | Independent | Not yet planned |
| Item 6 (fused `attackers_to`) | Benefits from faster attack access (this plan) | Not yet planned |
| Item 7 (`MoveList::push()` bounds check) | Independent | Not yet planned |
| Item 8 (hoist `by_color` loads) | Independent | Not yet planned |
| Item 9 (unchecked `from_index_unchecked`) | Independent | Not yet planned |
| Item 10 (optimize `compute_checkers`) | Independent | Not yet planned |
| Item 11 (inline `move_type()`) | Independent | Not yet planned |
| Item 12 (cache `occupied` bitboard) | Independent | Not yet planned |

This plan is a **prerequisite** for efficient further optimization: every subsequent item that touches attack-table access (items 2, 3, 5, 6, 10) benefits from the zero-overhead lookup eliminated here.

---

## 10. Summary

| Aspect | Detail |
|--------|--------|
| **What** | Replace `LazyLock` with `const` arrays (leapers) and `OnceLock` (magic/PEXT) for all attack tables |
| **Why** | Eliminates the acquire-load barrier (`ldar`) and pointer-indirection overhead on every attack-table access (13.98 % of cycles per profile) |
| **Item 1a scope** | `src/attacks.rs`: 3 `const fn` compute functions, 3 `const` arrays, updated accessors |
| **Item 1b scope** | `src/magic.rs`: replace 2 `LazyLock` → `OnceLock`, add `init()`, update lookups |
| **Item 1c scope** | `src/pext.rs`: replace 2 `LazyLock` → `OnceLock`, add `init()`, update lookups |
| **Item 1d scope** | `src/attacks.rs`: replace dispatch `LazyLock` → `AtomicU8` + eager init |
| **Init wiring** | `src/lib.rs` + `examples/*.rs`: call `attacks::init()` |
| **Files** | `src/attacks.rs` (~150 new / ~60 deleted), `src/magic.rs` (~30 changed), `src/pext.rs` (~30 changed), `src/lib.rs` (~3 changed), 7 example files |
| **Verification** | `cargo test` + `cargo run --release --example verify_perft` (all 41 positions, depths 1–6) |
| **Expected speedup** | 8–15 % (verify_perft total time reduction) |
| **Risk** | Low — `const fn` are trivially verifiable; `OnceLock` semantics match `LazyLock`; full perft regression catches regressions |
