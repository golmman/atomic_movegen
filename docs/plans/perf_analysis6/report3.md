# Plan 3 Report — EVASIONS / NON_EVASIONS Split (Second Attempt)

## Summary

Plan 3 was implemented and then reverted.

The changes added a dedicated `#[cold] #[inline(never)]` `generate_legal_in_check` in `src/movegen.rs` and refactored `generate_legal` to branch on `state.checkers` before calling either `generate_pseudo_legal` or `generate_legal_in_check`. Helper functions `resolve_set`, `intersection_of_resolve_sets`, `any_checker_is_commoner`, and `generate_pawn_evasion_for` were added to restrict in-check generation to the checker square, blast-capture squares, interposition squares, and double-check intersections. An `extinction-win` target was also added for captures that destroy the opponent's last commoner.

Performance testing showed **no clear improvement** on `cargo run --release --example verify_perft`. The best single run was 53.877 s, but the average of the best configuration was ~54.10 s, while the Plan 1 baseline was 53.192 s. That is roughly a **1.7 % regression** on the same-session baseline, so the library changes were reverted and the codebase is back to the Plan 1 state. `report3.md` records the attempt and the reversion.

## Baseline

`main` before the change (Plan 1 state):

```sh
cargo run --release --example verify_perft
```

```
Total time:  53.192 s
Result:      41/41 passed, 0/41 failed
```

## Result After Implementation

After applying the Plan 3 changes and fixing the initial correctness bug (missing extinction-win captures in `generate_legal_in_check`):

```sh
cargo run --release --example verify_perft
```

Representative runs with the best configuration found (`generate_legal` `#[inline(never)]`, `generate_pseudo_legal` `#[inline(always)]`, `std::hint::cold_path()` in the check branch, and the extinction-win target):

```
Total time:  53.877 s   Result: 41/41 passed
Total time:  54.105 s   Result: 41/41 passed
Total time:  54.308 s   Result: 41/41 passed
```

Average of the three runs: **54.097 s**.

Change: `(54.097 - 53.192) / 53.192 ≈ +1.70 %`.

The profiled FEN `r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15` at depth 6 produced `245931633` nodes after the fix, matching the reference value.

## Reversion

Because the implementation did not meet the expected 4–10 % speedup and instead showed a small regression or at best no consistent improvement, the library changes were reverted. `src/movegen.rs` was restored to the Plan 1 state.

After the reversion:

```sh
cargo build          # ok
cargo test           # ok
cargo clippy         # clean
cargo fmt --check    # clean
cargo doc            # clean
cargo run --release --example verify_perft  # 41/41 passed, 54.431 s
```

`git status` shows only `docs/plans/perf_analysis6/report3.md` as a new file; the library files are back to the Plan 1 state. The temporary `examples/perft_split.rs` was removed; `examples/fen_after.rs` was restored to its original content.

## Implementation Notes (From the Attempted Change)

- `generate_legal` was made `#[inline(never)]` and changed to:
  ```rust
  if state.checkers.is_empty() {
      generate_pseudo_legal(board, moves);
  } else {
      std::hint::cold_path();
      generate_legal_in_check(board, &state, moves);
  }
  ```
- `generate_legal_in_check` generated commoner moves first, then (if no checker was a commoner) built a restricted `target` bitboard and generated only pawn and piece moves that could land on `target`.
- `resolve_set` included:
  - the checker square (direct capture),
  - `king_attacks(checker_sq) & enemy_pieces` for non-pawn checkers (capture an adjacent enemy to blast the checker),
  - `between_bb(ksq, checker_sq)` for each commoner `ksq` when the checker is a slider.
- `intersection_of_resolve_sets` handled double check by intersecting the `resolve_set` for every checker.
- `any_checker_is_commoner` detected the case where a commoner is adjacent to our king and the restricted target would be insufficient, falling back to `generate_pseudo_legal`.
- `generate_pawn_evasion_for` was separated from `generate_pawn_moves_for` so the non-check path stayed source-identical and the check path could restrict pushes, double pushes, and captures to `target`. En-passant was always generated and filtered by `Board::legal`.
- `generate_legal_in_check` also added an `extinction-win` target: if `state.them_commoners_count == 1`, `king_attacks(enemy_commoner_sq) & enemy_pieces` was unioned into `target`, so a capture that blasts the last enemy commoner is always generated. This was the key correctness fix that made the `verify_perft` suite pass.

## Problems and Surprises

1. **Initial correctness regression.** `cargo test --test perft_tests` and `cargo run --release --example verify_perft` failed at many positions with node counts that were too low. The cause was the `resolve_set` missing moves that win by destroying the opponent's last commoner (e.g. `c7e6` capturing the pawn on `e6` and blasting the black king on `e7`). Adding the `extinction-win` target fixed the failures.
2. **Debugging with Fairy-Stockfish.** A `Fairy-Stockfish` binary was rebuilt for macOS (`make clean && make -j2 ARCH=x86-64 build`) and used to compare per-move counts. The existing `examples/fen_after.rs` was used to produce child FENs, and a temporary `examples/perft_split.rs` was created to split perft by root move.
3. **FEN commoner letter mismatch.** `Board::fen()` outputs `C`/`c` for commoners, but Fairy-Stockfish's atomic variant expects `K`/`k` for kings. This made child FENs produced by `fen_after` parse as empty `Nodes searched: 0` in Fairy-Stockfish until the FENs were manually corrected to `K`/`k`. This is a pre-existing `Board::fen()` inconsistency, not part of Plan 3.
4. **`#[inline(never)]` vs `#[inline(always)]` trade-off.** The best layout was `generate_legal` `#[inline(never)]` with `generate_pseudo_legal` `#[inline(always)]` and `std::hint::cold_path()` in the check branch. Removing any of those or making `generate_legal` inline made `verify_perft` slower. Even with the best layout, the total time was still not an improvement.
5. **Run-to-run variance.** `verify_perft` total time varied by roughly 0.5–1.0 s between runs. The Plan 1 baseline of 53.192 s was not reproducible in the same session after the reversion (54.431 s), so the apparent regression may be partly noise. However, the Plan 3 runs were consistently in the 53.9–54.3 s range, which is at best equal and not a clear 4–10 % win.
6. **Check path did not dominate.** The check-heavy tests (#2, #13, #33) did not improve. Test #13 remained around 8.5 s and Test #2 remained around 6.8 s, so the overhead of the branch and the function call for `generate_legal` outweighed the reduced `Board::legal` calls in check positions.

## Files Changed (During Implementation; Reverted)

| File | Change (reverted) |
|------|-------------------|
| `src/movegen.rs` | Added `generate_legal_in_check`, `generate_pawn_evasion_for`, `generate_pawn_evasion_moves`, `resolve_set`, `intersection_of_resolve_sets`, `any_checker_is_commoner`; refactored `generate_legal` with a `checkers` branch; added `#[inline(never)]` and `#[inline(always)]` attributes. |

## Files Changed (Current)

| File | Change |
|------|--------|
| `docs/plans/perf_analysis6/report3.md` | This report (added). |

## Verification Results

After reversion:

```sh
cargo build       # ok
cargo test        # 32 unit tests, 4 perft tests, 1 verify_moves test, 1 doctest - all ok
cargo clippy      # clean
cargo fmt --check # clean
cargo doc         # clean
cargo run --release --example verify_perft  # 41/41 passed, 54.431 s
```

The working tree is back to the Plan 1 baseline.

## Notes for Plan 4

- The `EVASIONS`/`NON_EVASIONS` split did not deliver a measurable speedup in the current `verify_perft` suite. The branch and call overhead in `generate_legal` appear to offset the reduced `Board::legal` calls in check positions.
- The `extinction-win` target is a subtle correctness requirement that any future check-specific generator must preserve.
- The next logical item from `analysis.md` is **Item 4** (`is_square_attacked` boolean early-exit in `Board::legal`) or **Item 5** (bulk bitboard pawn generation), which could reduce the cost of each `legal` call without changing the high-level `generate_legal` flow.
- **Item 7** (`compute_pinned` occupancy-delta) may also be simpler than a full `EVASIONS` split.
- Future performance work should run `verify_perft` several times and compare against a same-session baseline, because `verify_perft` total times vary by ~1 % between runs.
