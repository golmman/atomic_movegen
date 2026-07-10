# Plan 1 Report — Remove `OnceLock` from Magic Bitboard Tables

## Summary

Replaced the lazy `std::sync::OnceLock<&[Bitboard]>` magic tables in `src/magic.rs` with compile-time `static [Bitboard; N]` arrays. `rook_attacks`, `bishop_attacks`, and `queen_attacks` now do a direct array lookup with no atomic load, no branch, and no runtime initialization. The `std::sync::Once` guard in `src/lib.rs::perft()` was removed.

Measured on the current machine (`cargo run --release --example verify_perft`):

- Baseline: 56.512 s
- After:    53.926 s
- Speedup:  4.58 %

41/41 perft positions passed.

## Baseline

`main` before the change:

```sh
cargo run --release --example verify_perft
```

```
Total time:  56.512 s
Result:      41/41 passed, 0/41 failed
```

## Result

After the change:

```sh
cargo run --release --example verify_perft
```

```
Total time:  53.926 s
Result:      41/41 passed, 0/41 failed
```

Speedup: `(56.512 - 53.926) / 56.512 ≈ 4.58 %`.

The profiled FEN `r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15` depth 6 produces `245931633` nodes, matching the reference value.

## Implementation Notes

- `sliding_attack` was converted to a `pub(crate) const fn` by replacing its `for` loop with a `while` loop over the `&[(i8, i8)]` slice.
- `build_rook_table` and `build_bishop_table` are new `const fn` builders that enumerate every occupancy subset at compile time and fill the flat table using the same carry-rippler trick as the old runtime builder.
- `ROOK_TABLE` and `BISHOP_TABLE` are now `static [Bitboard; _]` arrays initialized by those builders. The total table size is still determined by `crate::util::total_table_size`.
- `magic::init()` and `attacks::init()` were turned into no-ops and kept for backwards compatibility so existing examples and tests that call them still compile.
- `perft()` no longer contains a `std::sync::Once` guard.
- No `unsafe` code was introduced.

## Problems, Surprises, and Workarounds

1. **`const` evaluator loop lint.** The first build hit `#[deny(long_running_const_eval)]` inside `sliding_attack` because the table build is genuinely large. Added `#![allow(long_running_const_eval)]` at the top of `src/magic.rs`. This is the expected lint for precomputing ~107k table entries at compile time.

2. **`const fn` control flow.** `for` loops and `Range::contains` are not allowed in `const fn`, so the loop was rewritten as a `while` and the boundary check was expanded to explicit `f >= 0 && f < 8 && r >= 0 && r < 8` comparisons.

3. **Assertion formatting.** `const fn` cannot use formatted `assert_eq!` or `panic!` messages, so the builder uses `assert!` with static messages (`"index out of bounds"`, `"wrong number of subsets"`).

4. **Compile-time cost.** The table build adds a small, one-time compile-time cost (release build took ~6 s including examples; normal builds finish in ~5 s). It is well within acceptable limits.

## Files Changed

| File | Change |
|------|--------|
| `src/magic.rs` | Convert `sliding_attack` to `const fn`; add `build_rook_table`/`build_bishop_table`; replace `OnceLock` with `static` arrays; simplify `rook_attacks`/`bishop_attacks`; make `init()` a no-op; add `#![allow(long_running_const_eval)]`. |
| `src/attacks.rs` | Update `init()` doc comment to state it is a no-op. |
| `src/lib.rs` | Remove `std::sync::Once` guard from `perft()`. |

## Verification Results

```sh
cargo build       # ok
cargo test        # 32 unit tests, 4 perft tests, 1 verify_moves test, 1 doctest - all ok
cargo clippy      # clean
cargo fmt         # clean
cargo doc         # clean
cargo run --release --example verify_perft  # 41/41 passed, 53.926 s
```

## Notes for Plan 2

- Magic tables are now zero-overhead static arrays.
- `StateInfo` still computes `commoners_count` and `them_commoners_count` every call, so the next logical item from `analysis.md` is **Item 2** (cache `pseudoRoyals` bitboards and add a capture blast-illegal pre-filter).
- `perft()` no longer initializes tables, so the `EVASIONS`/`NON_EVASIONS` split (Item 3) can be attempted without worrying about `init` ordering.
- `is_square_attacked` bool early-exit (Item 4) and bulk pawn generation (Item 5) can be layered on top.
