# Implementation Order

## Recommended sequence

| Order | Plan                                                        | Rationale                                                                                                                                                           |
| ----- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1     | **Fixed-size `StateInfo`** (plan_state_info.md)             | Pure mechanical change, zero semantic risk. Cleans up `StateInfo` layout so adding more fields in plan 3 is natural.                                                |
| 2     | **Magic bitboards** (plan_magic.md)                         | Safe Rust, easy to verify exhaustively, biggest independent performance win. Replaces the `sliding_attack()` loop everywhere.                                       |
| 3     | **Incremental checkers/pinners** (plan_checkers_pinners.md) | Medium complexity — changes `legal()` signature, touches `do_move()`, `movegen.rs`, `lib.rs`. Builds on the clean `StateInfo` from plan 1.                          |
| 4     | **PEXT** (plan_pext.md)                                     | Refinement on top of magic bitboards. Introduces `unsafe`, CPU feature gating, dynamic dispatch. Replaces magic lookup with single `pext` instruction on BMI2 CPUs. |

## Dependency graph

```
plan_state_info (1) ────→ plan_checkers_pinners (3)
                               (StateInfo layout ready)

plan_magic (2) ─────────→ plan_pext (4)   [optional refinement]
                               (replaces magic lookup with pext
                                when BMI2 available)
```

Plans 1→3 and 2→4 form two largely independent tracks (StateInfo/legal and sliding attacks). Within each track the order is fixed; between tracks the work can overlap.

## Risk profile

| Plan                         | Safety                              | Verification complexity              |
| ---------------------------- | ----------------------------------- | ------------------------------------ |
| Fixed-size StateInfo         | Trivial (no semantic change)        | `cargo test` + `verify_perft 5`      |
| Magic bitboards              | High (exhaustive test against loop) | Per-square exhaustive occupancy test |
| Incremental checkers/pinners | Medium (early-out logic)            | `verify_perft 6` + cycle profiling   |
| PEXT                         | Medium (unsafe isolation)           | Same tests on BMI2 and non-BMI2 CPUs |
