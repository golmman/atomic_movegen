# Code Cleanup Report 2

## Summary

All 3 phases of the second cleanup plan completed. 22 items addressed (18 from plan + 4 incidental). Zero behavioral changes — all perft values verified identical to the reference.

---

## Items Completed

### Phase 1 — Dead Code Removal (14 items + 1 incidental)

| # | File(s) | Change |
|---|---------|--------|
| Y2-1 | `attacks.rs:311` | Removed `attacks_bb` dispatch function (never called) |
| Y2-2 | `bitboard.rs:64-73` | Removed `pawn_attacks_bb` / `pawn_attacks_from` (dead pawn helpers) |
| Y2-3 | `bitboard.rs:24-62` | Removed 8 shift functions (`shift_north`, `shift_south`, `shift_east`, `shift_west`, `shift_ne`, `shift_nw`, `shift_se`, `shift_sw`); removed `test_shifts` |
| Y2-4 | `types.rs:314` | Removed `Bitboard::ALL` constant |
| Y2-5 | `types.rs:332` | Removed `Bitboard::msb` method |
| Y2-6 | `types.rs:512` | Removed `Piece::is_ok` method |
| Y2-7 | `types.rs:138` | Removed `SQ_NONE` const alias |
| Y2-8 | `types.rs:458` | Removed `Color::to_usize` method |
| Y2-9 | `types.rs:279-300` | Removed `relative_rank` / `relative_rank_sq` functions |
| Y2-10 | `types.rs:302` | Removed `pawn_push` function |
| Y2-11 | `types.rs:774` | Removed `MoveList::clear` method |
| Y2-12 | `types.rs:800` | Removed `MoveList::retain` method |
| Y2-13 | `types.rs:685-709` | Removed `impl Add for Direction` (dead trait impl) |
| Y2-14 | `bitboard.rs:85` | Gated `aligned` with `#[cfg(test)]` |
| — | `types.rs` | Removed `Direction::from_i16` (became dead after `Sub` simplification) |

### Phase 2 — Naming & Complexity (3 items + 1 incidental)

| # | File(s) | Change |
|---|---------|--------|
| P2-2 / I2-1 | `types.rs` | Simplified `Sub<Direction> for Square` to direct arithmetic (matching `Add`); removed `Direction::from` entirely |
| K2-2 | `movegen.rs` | Renamed `from_file` → `from_f` to avoid shadowing `File` type |
| K2-3 | `types.rs` | Removed `impl Not for Color` (callers use `.flip()`; the `!` operator was never applied to `Color`) |
| — | `movegen.rs` | Reverted `them` parameter removal (see Deviations below) |

### Phase 3 — Comments (3 items)

| # | File(s) | Change |
|---|---------|--------|
| K2-1 | `movegen.rs:246-250` | Trimmed misleading SAFETY comment on `set_len` to a bare call |
| C2-1 | `magic.rs:5-7` | Removed "Pure safe Rust" line; updated `LazyLock` → `OnceLock` |
| C2-3 | `types.rs:711-713` | Removed decorative separator comment |

### Items Audited and NOOP (WONTFIX)

| # | Reason |
|---|--------|
| P2-1 | `generate_pseudo_legal` loop patterns — kept for inlining friendliness |
| K2-4 | `Square::from_u8` — useful convenience, 9 call sites |
| C2-4 | `attacks.rs` dispatch comment — useful for maintainers |
| C2-5 | `board.rs` `populate_state` doc — accurate and useful |

---

## Correctness Verification

```sh
cargo test --lib                  # 40/40 unit tests pass
cargo test                        # 45/45 (40 unit + 4 perft + 1 doc)
cargo run --release --example verify_perft    # 41/41 positions, all depths
```

All perft values match the C++ reference in `Fairy-Stockfish`.

## Final Build

```
cargo build     # clean
cargo clippy    # no warnings
cargo fmt       # clean
```

## Deviations from Plan

1. **K2-2** (`them` parameter removal) — Implemented as specified, then reverted after the user reported a timing regression (~55s vs ~53s baseline). Investigation showed the difference was within system noise (~1%), but the principled fix was to restore the parameter: `us.flip()` should be computed once per `generate_pseudo_legal` call, not once per pawn in the hot loop. The rename `from_file` → `from_f` (I2-3) was kept.

2. **P2-2 / I2-1** (`Direction::from` → `from_i16`) — Implemented the rename and `Sub` simplification. After simplifying `Sub` to direct arithmetic (no longer calling `Direction::from_i16`), the `from_i16` method became dead and was removed entirely. Net effect: `Direction` has no conversion-from-i16 method; `Sub` uses the same pattern as `Add`.
