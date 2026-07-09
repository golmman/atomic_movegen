# Plan 2 Report: Performance

This report documents the changes made in `docs/plans/review1/plan2.md` to reduce
per-node work, avoid building unused attack tables, and remove the unsafe,
AMD-unfriendly PEXT code path.

## Changes

### 1. `Board::do_move` no longer populates `StateInfo`

- Removed `self.populate_state(state);` from the end of the non-castling branch
  in `src/board.rs`.
- The castling branch already did not call `populate_state`, so the behavior is
  now consistent.
- Updated `do_move`/`undo_move` doc comments to clarify that `state` is used
  only for undo information and that callers must call `populate_state`
  themselves before `legal()` if they need cached attack data.
- `generate_legal` already creates its own `StateInfo` and populates it, so no
  other callers needed updates.
- This removes the redundant `compute_checkers`/`compute_pinned` work on every
  perft node, since `generate_legal` recomputes those values anyway.

### 2. `attacks::init` only builds the magic tables

- Removed the x86_64 runtime dispatch between PEXT and magic.
- `attacks::init` now simply calls `crate::magic::init()`.
- The PEXT attack tables are no longer built, so startup time and memory are
  reduced.

### 3. Removed duplicate offset arrays in `magic.rs`

- Deleted `ROOK_OFFSETS` and `BISHOP_OFFSETS` (and the `compute_offsets` helper).
- `build_magic_table` now reads `entries[sq].offset` from `ROOK_ENTRIES`/
  `BISHOP_ENTRIES` and casts it to `usize`.
- `compute_rook_entries`/`compute_bishop_entries` still produce the same offsets
  as `MagicEntry.offset`, so the table layout is unchanged.

### 4. Shared `total_table_size` helper

- Created a new `src/util.rs` module with `total_table_size`.
- `magic.rs` now uses `crate::util::total_table_size` for `ROOK_TABLE_SIZE` and
  `BISHOP_TABLE_SIZE`.
- The `pext` module was removed in this plan, so the PEXT reuse described in
  step 4 of the plan is no longer applicable (see below).

### 5. Removed PEXT from the default build (Option A)

- Deleted `src/pext.rs`.
- Removed `pub(crate) mod pext;` from `src/lib.rs`.
- Removed the `sliding_dispatch` module and `force_magic`/`force_pext` helpers
  from `src/attacks.rs`.
- `src/magic.rs` no longer hides `queen_attacks` behind `allow(dead_code)` on
  x86_64; it is now the single `queen_attacks` implementation for all targets.

This leaves the default build with zero `unsafe` blocks and no PEXT path, so the
AGENTS.md `unsafe` rule does not need to change in plan 5.

## PEXT decision

Plan 2 offered two options:

- **Option A:** Delete the PEXT module and always use magic.
- **Option B:** Keep PEXT behind a non-default `pext` feature using
  `core::arch::x86_64::_pext_u64`.

**Option A was chosen** because:

- The project rules strongly prefer safe Rust.
- The inline-assembly PEXT path was forced on every BMI2 CPU, which is
  significantly slower on AMD Zen processors.
- The benchmark impact of removing PEXT was not a regression (see below).

If the PEXT path is revisited later, it should be added in a new plan with a
fresh AGENTS.md review and an Intel-only `target_feature` guard.

## Performance impact

All tests were run on an Apple M4 Pro (`arm64`).

### `verify_perft 6` (41 test positions)

| Run | Total time | Source |
| --- | ---------- | ------ |
| Before plan 2 | 59.751 s | `docs/perf/m4/2026-07-09_plan2_baseline.txt` |
| After plan 2  | 56.739 s | `docs/perf/m4/2026-07-09_plan2.txt` |

**Improvement: ~3.0 s (~5.0%)**.

### Starting position perft depth 6

```
# Before plan 2
$ time cargo run --release --example perft "rnbqkbnr/..." 6
118926425
real    0m0.782s

# After plan 2
$ time cargo run --release --example perft "rnbqkbnr/..." 6
118926425
real    0m0.724s
```

**Improvement: ~0.058 s (~7.4%)**.

### Notes

- Removing the redundant `populate_state` call was the largest contributor. The
  perft node count is high, and `compute_checkers`/`compute_pinned` do a lot of
  sliding-piece attack lookups.
- Removing PEXT did not hurt performance because the M4 build already used the
  magic fallback. The `pext` tables were never consulted at runtime, but they
  were still being built at initialization, which is now also saved.

## Unexpected observations

- No test code relied on `state.checkers`/`state.pinned` after `do_move`. The
  `do_undo` tests compare the board FEN, not the state fields.
- The `magic.rs` table layout is byte-for-byte identical because the
  `MagicEntry.offset` values were always the same as `ROOK_OFFSETS`/
  `BISHOP_OFFSETS`.
- Removing `pext.rs` also removed its unit tests (`test_pext_*`). The magic
  tests (`test_magic_vs_loop_*`) still cover every square and occupancy
  pattern, so sliding-piece correctness is still fully verified.

## Verification

- `cargo build` — passed.
- `cargo clippy` — passed.
- `cargo fmt` — passed.
- `cargo test` — passed (33 unit tests, 4 perft tests, 1 move verification test).
- `cargo run --release --example verify_perft 6` — passed (41/41 positions),
  total time 56.739 s.
- `cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6` —
  passed, returned `118926425`.

## Notes for plan 3

- `do_move` no longer populates `state.checkers`/`state.pinned`/
  `state.commoners_count`; `generate_legal` must continue to call
  `populate_state` itself.
- `rule50` is still `u8` at this point; plan 3 will change it.
- `Board::piece_on`/`empty` are safe for `Square::NONE` (plan 1).
- PEXT is no longer in the default build path, so plan 3 should see no
  PEXT-related behavior.
