# Cleanup Plan

## Goal

Tighten the public API surface, fix documentation inconsistencies, and add
example-level doc comments — without changing any behavior.

---

## 1. Shrink module visibility

### 1a. `pext` → `pub(crate) mod pext`

**Why:** Every item in `pext` is already `pub(crate)`. Exposing an empty public
module is noise in the public API. On non-x86_64 the module is dead code.

**Impact:**
- Only `attacks.rs` references it (`crate::pext::has_bmi2`, `crate::pext::init`,
  `crate::pext::bishop_attacks_pext`, `crate::pext::rook_attacks_pext`).
- No examples or tests reference `pext` directly.

**Changes needed:**
- `src/lib.rs:34`: `pub mod pext;` → `pub(crate) mod pext;`

---

### 1b. `magic` → `pub(crate) mod magic`

**Why:** `magic` exposes `bishop_attacks`, `rook_attacks`, `queen_attacks`
directly, even though they are already re-exported through the `attacks` module.
On x86_64 this lets users bypass the PEXT runtime dispatch. `magic` is an
implementation detail.

**Impact:**
- Internal consumers (`attacks.rs`, `pext.rs`) use `crate::magic::...` — still
  works because `pub(crate)` is visible within the crate.
- On non-x86_64, `attacks.rs:51` does `pub use crate::magic::{...}` — in Rust,
  `pub use` can re-export `pub(crate)` items at a higher visibility. This
  keeps working.
- No examples or tests reference `magic` directly.

**Changes needed:**
- `src/lib.rs:32`: `pub mod magic;` → `pub(crate) mod magic;`

---

### 1c. `bitboard` → `pub(crate) mod bitboard`

**Why:** The bitboard constants (`FILE_ABB` … `FILE_HBB`, `RANK_1BB` … `RANK_8BB`,
`ALL_SQUARES`) and helpers (`between_bb`, `line_bb`) are only needed internally
by `board.rs`. They are general-purpose chess utilities, but the crate's public
API should be atomic-movegen-specific.

**Impact:**
- `board.rs:2` uses `use crate::bitboard::*;` — still works with `pub(crate)`.
- `attacks.rs` also imports `BETWEEN_BB` / `LINE_BB` but those are
  `pub(crate)` in `attacks`, not via `bitboard`.
- No examples or tests reference `bitboard` directly.
- `bitboard.rs:45` has a `#[cfg(test)] pub fn aligned()` — this is only public
  in test builds; it can stay as-is or be made `pub(crate)` for consistency.

**Changes needed:**
- `src/lib.rs:30`: `pub mod bitboard;` → `pub(crate) mod bitboard;`

---

## 2. Document the `sliding_dispatch` re-exports in `attacks`

**Problem:** On x86_64, `attacks` re-exports `bishop_attacks`, `rook_attacks`,
`queen_attacks` from the `sliding_dispatch` submodule (`attacks.rs:48`).
The `sliding_dispatch` functions (lines 23–44) have **no doc comments**, so the
re-exported names lack documentation on x86_64. On non-x86_64 the same names
inherit doc comments from `magic`, creating a platform-dependent documentation
gap.

**Fix:** Add doc comments to the three dispatch functions in `sliding_dispatch`
(`attacks.rs:23–44`). Use the exact same wording as the `magic` module's
versions so the docs are identical regardless of platform.

---

## 3. Add `//!` doc comments to examples

6 of 7 example files have no file-level doc comment. Only `verify_perft.rs`
has one. Add a `//!` doc comment to each describing its purpose and CLI usage.

| File | Current | Action |
|------|---------|--------|
| `examples/perft.rs` | bare | Add doc comment |
| `examples/perft_divide.rs` | bare | Add doc comment |
| `examples/list_moves.rs` | bare | Add doc comment |
| `examples/fen_after.rs` | bare | Add doc comment |
| `examples/debug_moves.rs` | bare | Add doc comment |
| `examples/pawn_debug.rs` | bare | Add doc comment |
| `examples/verify_perft.rs` | has one | No change |

---

## 4. Optional / stretch goals

### 4a. Re-export key types at crate root

Currently consumers write `atomic_movegen::board::Board`,
`atomic_movegen::types::MoveList`, etc. Consider adding selective re-exports
at the crate root for the most common types:

```rust
pub use board::Board;
pub use types::{Move, MoveList, Bitboard, Square, Color, PieceType};
```

This would give a cleaner `atomic_movegen::Board` vs. `atomic_movegen::board::Board`.

### 4b. Auto-init attacks from `Board::new()`

Every example calls `atomic_movegen::attacks::init()` before using the library.
If `Board::new()` (or a lazy `OnceLock` inside the attack functions) handled
this automatically, the `init()` call could be removed from the examples and
the `attacks` module could become `pub(crate)`. This is a larger behavioral
change and should be considered separately.

### 4c. Remove `sq_str` / `parse_sq` `#[doc(hidden)]` or make `pub(crate)`

These are convenience helpers for the examples. They are `pub` but
`#[doc(hidden)]`. Either:
- Remove `#[doc(hidden)]` and properly document them (they are genuinely useful
  for consumers), or
- Make them `pub(crate)` and have examples inline or duplicate the logic.
  The first option is less disruptive.

---

## Order of implementation

1. `pext` → `pub(crate)` (trivial, no ripple)
2. `magic` → `pub(crate)` (needs doc-comment fix in step 4 first or concurrently)
3. `bitboard` → `pub(crate)` (trivial, no ripple)
4. Add doc comments to `sliding_dispatch` functions in `attacks.rs`
5. Add doc comments to example files
6. (Optional) Evaluate 4a–4c

After each change, verify with `cargo build`, `cargo test`, `cargo clippy`,
and `cargo doc --no-deps`.
