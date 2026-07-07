# Code Cleanup Report 3

## Summary

All 5 phases of the cleanup plan completed, plus 2 stretch goals. Zero behavioral changes — all tests and perft values unchanged.

---

## Items Completed

### 1. Module Visibility Shrunk

| # | File | Change |
|---|------|--------|
| 1a | `src/lib.rs:34` | `pub mod pext;` → `pub(crate) mod pext;` |
| 1b | `src/lib.rs:32` | `pub mod magic;` → `pub(crate) mod magic;` |
| 1c | `src/lib.rs:30` | `pub mod bitboard;` → `pub(crate) mod bitboard;` |

**Why:** These modules are implementation details. `pext` is empty on non-x86_64; `magic` is re-exported through `attacks`; `bitboard` constants are internal to `board.rs`. Exposing them polluted the public API surface.

**Impact:** All internal consumers use `crate::module::...` paths which still work with `pub(crate)`. `pub use crate::magic::...` in `attacks.rs` re-exports `pub(crate)` items at higher visibility — valid in Rust.

### 2. Platform-Consistent Sliding-Attack Docs

| File | Change |
|------|--------|
| `src/attacks.rs:23, 33, 42` | Added `///` doc comments to `sliding_dispatch::bishop_attacks`, `rook_attacks`, `queen_attacks` |

**Why:** On x86_64 the re-exported names lacked doc comments (the `sliding_dispatch` functions had none). On non-x86_64 the same names inherited docs from `magic`. This created a platform-dependent documentation gap. The new docs use the exact wording from `magic.rs`.

### 3. Example File Doc Comments

| File | Change |
|------|--------|
| `examples/perft.rs` | Added `//!` doc comment |
| `examples/perft_divide.rs` | Added `//!` doc comment |
| `examples/list_moves.rs` | Added `//!` doc comment |
| `examples/fen_after.rs` | Added `//!` doc comment |
| `examples/debug_moves.rs` | Added `//!` doc comment |
| `examples/pawn_debug.rs` | Added `//!` doc comment |

Each doc comment describes the example's purpose and CLI usage. `verify_perft.rs` already had one — no change.

### 4. Crate-Root Re-exports (Stretch 4a)

Added at `src/lib.rs:38`:

```rust
pub use board::Board;
pub use types::{Bitboard, Color, Move, MoveList, PieceType, Square};
```

Consumers can now write `atomic_movegen::Board` instead of `atomic_movegen::board::Board`. The crate-level doc example was updated to use the shorter path.

### 5. Removed `#[doc(hidden)]` from Sq Helpers (Stretch 4c)

Removed `#[doc(hidden)]` from `sq_str` and `parse_sq` in `src/types.rs:777, 789`. Both already had proper doc comments — the attribute was unnecessarily hiding them from API docs.

---

## Items Intentionally Unchanged

| # | Reason |
|---|--------|
| 4b (auto-init via `Board::new()`) | Larger behavioral change; deferred to a separate plan |

---

## Correctness Verification

```sh
cargo test --lib                  # 40/40 unit tests pass
cargo test                        # 45/45 (40 unit + 4 perft + 1 doc)
```

All perft values unchanged.

## Final Build

```
cargo build     # clean
cargo clippy    # no warnings
cargo fmt       # clean
```

## Deviations from Plan

1. **Dead-code suppression** — `bitboard.rs` and `attacks.rs` items only used in tests (FILE/RANK constants, `line_bb`, `LINE_BB`, `compute_line_bb`) were given `#[allow(dead_code)]` to silence post-visibility warnings. The plan didn't anticipate these but zero-warning clippy is a requirement.

2. **Clippy `needless_range_loop`** — Fixed two `for sq_idx in 0..64` patterns in `pext.rs` test code that indexed arrays by position instead of iterating. Converted to `.iter().enumerate()`.
