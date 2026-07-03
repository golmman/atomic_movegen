# Plan: Between/Line Lookup Tables

## Goal

Replace the runtime loop-and-match implementations of `between_bb()` and
`line_bb()` with compile-time precomputed lookup tables (`const` arrays),
matching the established pattern used by `KING_ATTACKS`, `KNIGHT_ATTACKS`,
`PAWN_ATTACKS` in `attacks.rs`.

**Estimated speedup:** 5–12 % on `perft` (hotter positions with many pinned
pieces benefit more). Largest relative win inside `compute_pinned()` where
`between_bb` is called for every sniper–king pair.

---

## Background

### Current implementation

`between_bb` (`src/bitboard.rs:100`) computes squares between two squares by:

1. Decomposing both `Square` values into `(file, rank)` via `% 8` and `/ 8`.
2. Walking a loop from `s1` toward `s2`, converting each `(f, r)` pair back
   into a `Square` via a nine-arm `match` — generating an `unreachable!()`
   guard per arm.
3. Setting one bit at a time in an accumulator, then returning the bitboard.

`line_bb` (`src/bitboard.rs:153`) follows the same pattern, walking the
entire ray instead of just the between squares, with additional
`(0..8).contains(&f)` bounds checks.

### Usage

| Call site | File : line | Frequency |
|-----------|-------------|-----------|
| `compute_pinned` — `between_bb(ksq, sniper_sq) & occupied` | `board.rs:417` | Once per (commoner, sniper) pair per `populate_state` call — called *every* non-leaf perft node |
| `aligned` — `line_bb(s1, s2) & square_bb(s3)` | `bitboard.rs:207` | Only in test — not in hot path |
| Tests | `bitboard.rs:250–267` | — |

The critical hot path is `compute_pinned` → `between_bb`. Each call to
`populate_state` iterates over (commoners × snipers), typically 1–8 pairs,
with a loop inside `between_bb` of 1–6 iterations. The `make_square` match
and `unreachable!` compile to a jump table + panic branch, adding overhead.

### Memory

| Table | Element | Entries | Size |
|-------|---------|---------|------|
| `BETWEEN_BB` | `Bitboard` (u64) | 64 × 64 = 4096 | 32 KiB |
| `LINE_BB` | `Bitboard` (u64) | 64 × 64 = 4096 | 32 KiB |
| **Total** | | | **64 KiB** |

64 KiB of `.rodata` — negligible even on embedded targets, trivially
cache-resident on any modern CPU.

---

## Implementation

### Step 1 — Add `const fn compute_between_bb()` (in `attacks.rs`)

Following the pattern of `compute_king_attacks`, add:

```rust
const fn compute_between_bb() -> [[Bitboard; 64]; 64] {
    let mut table = [[Bitboard(0); 64]; 64];
    let mut s1: u8 = 0;
    while s1 < 64 {
        let mut s2: u8 = 0;
        while s2 < 64 {
            let f1 = s1 % 8;
            let r1 = s1 / 8;
            let f2 = s2 % 8;
            let r2 = s2 / 8;

            if s1 != s2 && (f1 == f2 || r1 == r2 || (f1 as i8 - f2 as i8).abs() == (r1 as i8 - r2 as i8).abs()) {
                let mut b = 0u64;
                let df = (f2 as i8 - f1 as i8).signum();
                let dr = (r2 as i8 - r1 as i8).signum();
                let mut f = f1 as i8 + df;
                let mut r = r1 as i8 + dr;
                while f != f2 as i8 || r != r2 as i8 {
                    b |= 1u64 << ((r as u8) * 8 + (f as u8));
                    f += df;
                    r += dr;
                }
                table[s1 as usize][s2 as usize] = Bitboard(b);
            }
            s2 += 1;
        }
        s1 += 1;
    }
    table
}
```

Key points:
- Uses `const fn` with `while` loops (no `for`, no `match` — those are
  allowed in stable `const fn` since Rust 1.46+).
- Straight `u8`/`i8` arithmetic on 0..63 indices — avoids the `Square` → `(file, rank)` → `Square` round-trip entirely within the const function.
- `s1 == s2` yields empty (zero bitboard) — no need for special-casing.
- Non-aligned pairs also yield zero implicitly.

### Step 2 — Add `const fn compute_line_bb()` (in `attacks.rs`)

Same pattern, but walks the full ray instead of stopping one short:

```rust
const fn compute_line_bb() -> [[Bitboard; 64]; 64] {
    let mut table = [[Bitboard(0); 64]; 64];
    let mut s1: u8 = 0;
    while s1 < 64 {
        let mut s2: u8 = 0;
        while s2 < 64 {
            let f1 = s1 % 8;
            let r1 = s1 / 8;
            let f2 = s2 % 8;
            let r2 = s2 / 8;

            if s1 != s2 && (f1 == f2 || r1 == r2 || (f1 as i8 - f2 as i8).abs() == (r1 as i8 - r2 as i8).abs()) {
                let mut b = 0u64;
                let df = (f2 as i8 - f1 as i8).signum();
                let dr = (r2 as i8 - r1 as i8).signum();
                let mut f = f1 as i8;
                let mut r = r1 as i8;
                while (0..8).contains(&f) && (0..8).contains(&r) {
                    b |= 1u64 << ((r as u8) * 8 + (f as u8));
                    f += df;
                    r += dr;
                }
                table[s1 as usize][s2 as usize] = Bitboard(b);
            }
            s2 += 1;
        }
        s1 += 1;
    }
    table
}
```

### Step 3 — Declare static tables in `attacks.rs`

```rust
/// Precomputed between-squares table: `BETWEEN_BB[s1][s2]` gives the
/// bitboard of squares strictly between `s1` and `s2`, or `Bitboard::EMPTY`
/// when `s1` and `s2` are not on the same rank, file, or diagonal.
const BETWEEN_BB: [[Bitboard; 64]; 64] = compute_between_bb();

/// Precomputed line-squares table: `LINE_BB[s1][s2]` gives the bitboard
/// of all squares on the same rank, file, or diagonal as `s1` and `s2`
/// (including `s1` and `s2` themselves), or `Bitboard::EMPTY` when the
/// two squares are not aligned.
const LINE_BB: [[Bitboard; 64]; 64] = compute_line_bb();
```

### Step 4 — Update `bitboard.rs` functions

Replace the bodies of `between_bb`, `line_bb`, and `aligned` with table
lookups:

```rust
#[inline(always)]
pub fn between_bb(s1: Square, s2: Square) -> Bitboard {
    BETWEEN_BB[s1 as usize][s2 as usize]
}

#[inline(always)]
pub fn line_bb(s1: Square, s2: Square) -> Bitboard {
    LINE_BB[s1 as usize][s2 as usize]
}
```

`aligned` already delegates to `line_bb`, so it improves automatically:

```rust
#[inline(always)]
pub fn aligned(s1: Square, s2: Square, s3: Square) -> bool {
    line_bb(s1, s2) & Bitboard::square_bb(s3) != Bitboard::EMPTY
}
```

The re-export via `pub use` in `bitboard.rs` stays unchanged — `between_bb`,
`line_bb`, and `aligned` remain public functions in the `bitboard` module.

### Step 5 — Decide where to put the tables

Option A (recommended): Put `BETWEEN_BB` and `LINE_BB` in `attacks.rs`
alongside `KING_ATTACKS` etc., then make `attacks` module `pub(crate)` or
re-export in `bitboard.rs`.

Option B: Put the `const` tables directly in `bitboard.rs`. Slightly more
self-contained but clutters the bitboard module with >60 lines of table
declarations.

We recommend Option A because:
- `attacks.rs` already serves as the single module for precomputed attack
  tables. Between/line tables are conceptually the same category.
- The `const fn` compute functions sit naturally next to
  `compute_king_attacks` etc.
- `bitboard.rs` stays clean (just the `#[inline]` wrappers).

The `bitboard.rs` functions then import the tables:

```rust
use crate::attacks::{BETWEEN_BB, LINE_BB};
```

### Step 6 — Update tests

The existing tests (`test_between_bb`, `test_line_bb`, `test_aligned` in
`bitboard.rs:249–267`) already assert correct behaviour. After the change:

```rust
#[test]
fn test_between_bb() {
    let between = between_bb(Square::C1, Square::F4);
    assert!(between & square_bb(Square::D2) != Bitboard::EMPTY);
    assert!(between & square_bb(Square::E3) != Bitboard::EMPTY);
    // LUT-specific: test non-aligned squares return empty
    assert!((between_bb(Square::A1, Square::B3)).is_empty());
    // LUT-specific: test same-square returns empty
    assert!((between_bb(Square::D4, Square::D4)).is_empty());
}

#[test]
fn test_line_bb() {
    let line = line_bb(Square::A1, Square::H8);
    assert!(line & square_bb(Square::B2) != Bitboard::EMPTY);
    assert!(line & square_bb(Square::C3) != Bitboard::EMPTY);
    // LUT-specific: line includes both endpoints
    assert!(line & square_bb(Square::A1) != Bitboard::EMPTY);
    assert!(line & square_bb(Square::H8) != Bitboard::EMPTY);
}
```

---

## Verification

### Correctness

Run `cargo run --release --example verify_perft` — all 41 positions at
depth 1–6 must match `perft_values.md`.

The existing `#[cfg(test)]` unit tests in `bitboard.rs` (`test_between_bb`,
`test_line_bb`, `test_aligned`) and `board.rs` (`test_pinned`) must also pass
(`cargo test`), but the authoritative correctness gate is `verify_perft`.

The tables are pure functions of `(s1, s2)` — no state, no init, no
`OnceLock`. If the `const fn` logic is correct, the tables are correct for
every call.

### Performance

Run `cargo run --release --example verify_perft` before and after the
change, and compare total wall time. Since the change is purely a
drop-in replacement of runtime loops with table lookups, a measurable
speedup (5–12 % on the overall `verify_perft` suite) confirms the win.

If the `const fn` computation is too heavy for the compiler, it can be
moved to a `lazy_static` or `OnceLock` initializer — but 64 KiB is well
within `const` evaluation limits. Stockfish uses the same approach for its
`SquareDistance` and `BetweenBb` tables.

### Risk

None. The tables are pure, small (64 KiB), and drop-in replacements. The
only diff is a compile-time vs. runtime computation, and the removal of the
`unreachable!()` match arms from the hot path.
