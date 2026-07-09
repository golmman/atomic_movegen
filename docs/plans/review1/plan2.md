# Plan 2: Performance

## Goal

Eliminate redundant per-node work in `do_move`, avoid building unused attack tables, and remove or feature-gate the unsafe and AMD-unfriendly PEXT inline assembly.

## Scope

- `src/board.rs` (`do_move` / `undo_move`)
- `src/attacks.rs` (`init` and dispatch)
- `src/magic.rs` (offset/duplication cleanup)
- `src/pext.rs` (inline assembly replacement or removal)
- `Cargo.toml` (feature flags for PEXT)
- `docs/perf/` (benchmarks)

## Current state (from review and plan 1)

- `do_move` calls `populate_state` at the end, then `generate_legal` calls it again on a fresh `StateInfo`.
- `do_move` for castling does **not** call `populate_state`, so the behavior is inconsistent.
- `attacks::init` always builds magic tables, then builds PEXT tables on x86-64, even if only one is used.
- `magic.rs` computes `ROOK_OFFSETS`/`BISHOP_OFFSETS` and then `compute_rook_entries` recomputes the same offsets as `MagicEntry.offset`.
- `pext.rs` uses `unsafe` inline assembly and forces PEXT on every BMI2 CPU, which is slow on AMD Zen.
- `Piece::type_of` and `color` no longer use `unsafe` (from plan 1).

## Prerequisites

- Read `report1.md` for any API changes that affect `do_move` callers.
- Decide whether to keep PEXT. Since `AGENTS.md` currently says "zero unsafe", the default plan is to remove PEXT from the default build and rely on magic; a `pext` feature can be re-added later if desired. Plan 5 will update `AGENTS.md` accordingly.

## Tasks

1. **Remove redundant `populate_state` from `do_move`**
   - Delete `self.populate_state(state);` at the end of the non-castling branch in `src/board.rs`.
   - Confirm that `do_move` for castling does not call it either (it does not already).
   - Update `do_move`/`undo_move` doc comments to clarify that `state` is only for undo information and that callers must call `populate_state` themselves before `legal()` if they need it.
   - `generate_legal` already creates a new `StateInfo` and populates it; `perft` already uses `generate_legal`, so no other code changes are needed.

2. **Reorder `attacks::init` to build only the needed table**
   - Restructure `init` so that:
     - On x86-64, if the `pext` feature is enabled and `has_bmi2()` returns true, only `pext::init()` is called.
     - Otherwise, only `magic::init()` is called.
   - `pext::init` only needs `magic::ROOK_MASKS`, `BISHOP_MASKS`, and `sliding_attack`, which are `const`/`pub(crate)` and do not require `magic::init`.

3. **Remove duplicate offset arrays in `magic.rs`**
   - Delete the `ROOK_OFFSETS` and `BISHOP_OFFSETS` constants.
   - Change `build_magic_table` to read `entries[sq].offset` from `ROOK_ENTRIES`/`BISHOP_ENTRIES` and cast to `usize`:
     ```rust
     let offset = entries[sq].offset as usize;
     ```
   - Ensure `compute_rook_entries`/`compute_bishop_entries` still produce the same offsets.

4. **Share the total table size helper**
   - Move `total_table_size` from `magic.rs` to a shared location (e.g., a new `src/util.rs` or re-export from `magic.rs`) and reuse it in `pext.rs` `compute_pext_layout`.

5. **PEXT: remove from default build or replace inline assembly**
   - **Option A (recommended for zero-unsafe default):** delete the `pext` module and the `cfg(target_arch = "x86_64")` dispatch in `attacks.rs`. Always use magic.
   - **Option B (if PEXT must be kept):** move the `pext` module behind a non-default `pext` feature, replace inline assembly with `core::arch::x86_64::_pext_u64` inside an `unsafe` block and `#[target_feature(enable = "bmi2")]` function, and add `#[cfg(feature = "pext")]` guards. The `AGENTS.md` update must be done in plan 5.
   - This plan defaults to **Option A** for safety. Record in `report2.md` if the benchmark impact is significant and PEXT should be reconsidered.

6. **Benchmarks**
   - Run `cargo run --release --example verify_perft 6` before and after the changes.
   - Capture `docs/perf/m4/`, `docs/perf/5950X/` style timing (or `docs/perf/$(uname -m)-$(date +%F).txt` on the current machine).
   - Compare with the existing `docs/perf/` numbers.

7. **Run tests and linting**
   - `cargo build`
   - `cargo clippy`
   - `cargo test`
   - `cargo run --release --example verify_perft 5`
   - `cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6`

8. **Write `report2.md`**
   - Document the measured speed-up (or slow-down) from removing redundant `populate_state` and from removing/replacing PEXT.
   - Note any unexpected dependency between `do_move` and `populate_state` (e.g., tests that were relying on `state.checkers` after `do_move`).
   - State whether PEXT was removed or feature-gated, and whether the `AGENTS.md` zero-unsafe rule needs to be updated in plan 5.
   - Record any `magic.rs` offset changes that affected tests.

## Notes for plan 3

- `do_move` no longer populates `state.checkers`/`state.pinned`/`state.commoners_count`; `generate_legal` must continue to call `populate_state` itself.
- `rule50` is still `u8` at this point; plan 3 will change it.
- `Board::piece_on`/`empty` are safe for `Square::NONE` (plan 1).
- `pext` is no longer in the default build path, so plan 3 should see no PEXT-related behavior.
