# Plan 2 Report — Cache Pseudo-Royal Commoner Bitboards and Add a Capture Blast Pre-Filter

## Summary

Plan 2 was implemented and then reverted.

The changes added `our_commoners` and `them_commoners` bitboards to `StateInfo`, reused them in `compute_checkers`, `compute_pinned`, and `Board::legal`, and added a capture blast-illegal pre-filter in `generate_legal`. A small refinement was also made to pass the pre-computed `is_capture` flag into `is_move_trivially_legal` to avoid redundant `piece_on(to)` reads.

Performance testing showed a **5.17 % regression** on `cargo run --release --example verify_perft` (from 53.246 s to 55.999 s), so the library changes were reverted. The codebase is back to the Plan 1 state and `report2.md` now records the attempt and the reversion.

## Baseline

`main` before the change (Plan 1):

```sh
cargo run --release --example verify_perft
```

```
Total time:  53.246 s
Result:      41/41 passed, 0/41 failed
```

## Result After Implementation

After applying the Plan 2 changes:

```sh
cargo run --release --example verify_perft
```

```
Total time:  55.999 s
Result:      41/41 passed, 0/41 failed
```

Change: `(53.246 - 55.999) / 53.246 ≈ -5.17 %`.

The profiled FEN `r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15` at depth 6 still produced `245931633` nodes, matching the reference value.

## Reversion

Because the implementation did not meet the expected 3–7 % speedup and instead regressed `verify_perft` by about 5 %, the library changes were reverted with:

```sh
git checkout -- src/board.rs src/movegen.rs
```

After the reversion:

```sh
cargo build          # ok
cargo test           # ok
cargo clippy         # clean
cargo fmt --check    # clean
cargo doc            # clean
cargo run --release --example verify_perft  # 41/41 passed, 53.542 s
```

`git status` shows only `docs/plans/perf_analysis6/report2.md` as a new file; the library files are back to the Plan 1 state.

## Implementation Notes (From the Attempted Change)

- `StateInfo` was extended with `our_commoners: Bitboard` and `them_commoners: Bitboard`.
- `Board::populate_state` computed both bitboards once and derived the counts from them, then passed them to `compute_checkers` and `compute_pinned`.
- `compute_checkers` and `compute_pinned` signatures were updated to accept cached commoner bitboards.
- `Board::checkers()` and `Board::pinned(c)` recomputed the required bitboards and called the new helpers.
- `Board::legal` used `state.our_commoners` and `state.them_commoners` instead of `self.commoners(us)`/`self.commoners(them)`.
- `generate_legal` added a fast reject for captures whose blast zone destroyed all own commoners:

  ```rust
  survivors = (state.our_commoners & !Bitboard::square_bb(from)) & !attacks::king_attacks(to);
  ```

- `is_move_trivially_legal` was parameterized to accept `is_capture: bool` so the capture flag was not recomputed inside the hot loop.

## Problems and Surprises

1. **No measured speedup; actual regression.** The expected 3–7 % speedup did not materialize; `verify_perft` became ~5 % slower.
2. **Pre-filter overhead.** The capture pre-filter computes `is_capture` and, for every capture, evaluates `survivors` using `attacks::king_attacks(to)`. For legal captures, `Board::legal` still runs and recomputes both `is_capture` and `attacks::king_attacks(kto)`, so the pre-filter added work without skipping `legal()`.
3. **Public API blocked reuse.** Because `Board::legal` is a public method, the pre-computed `is_capture` flag could not be passed through it; only `is_move_trivially_legal` could reuse the flag.
4. **Suite-wide effect.** The 41 perft positions apparently contain enough legal captures that the pre-filter cost dominates the savings from the `commoners` cache.

## Files Changed (During Implementation; Reverted)

| File | Change (reverted) |
|------|-------------------|
| `src/board.rs` | Added `our_commoners`/`them_commoners` to `StateInfo`; updated `populate_state`; changed `compute_checkers`/`compute_pinned` signatures; updated `checkers()`/`pinned()`; used cached bitboards in `legal`; parameterized `is_move_trivially_legal`. |
| `src/movegen.rs` | Added capture blast pre-filter and passed `is_capture` to `is_move_trivially_legal`. |

## Files Changed (Current)

| File | Change |
|------|--------|
| `docs/plans/perf_analysis6/report2.md` | This report (added). |

## Verification Results

After reversion:

```sh
cargo build       # ok
cargo test        # 32 unit tests, 4 perft tests, 1 verify_moves test, 1 doctest - all ok
cargo clippy      # clean
cargo fmt --check # clean
cargo doc         # clean
```

The working tree is back to the Plan 1 baseline.

## Notes for Plan 3

- The `commoners` bitboard cache and blast pre-filter remain valid ideas for isolated single-commoner positions, but the current `verify_perft` suite does not benefit from them as a whole.
- The `EVASIONS`/`NON_EVASIONS` split (Item 3) and `is_square_attacked` bool early-exit (Item 4) can still be explored, but they should be measured against the full `verify_perft` suite and the profile FEN before being accepted.
- Any future attempt to cache `commoners` bitboards should also consider passing the `is_capture` flag into `Board::legal` (e.g., via an internal `legal_with_capture` helper) to avoid redundant `piece_on(to)` and `king_attacks` work.
