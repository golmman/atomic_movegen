# Report 1 — Terminal rules, state-aware `outcome`, and material draws

## Status

| Item | Status | Notes |
| --- | --- | --- |
| P0-1 terminal precedence | Done | Mate now outranks the 50-move clock; commoner-only is a WDL draw; the full precedence is pinned by one table. |
| P0-2 state-aware outcome API | Done | Added `outcome_with_state`, `outcome_with`, `is_material_draw`; `outcome()` reuses the state-aware path and no longer builds a `MoveList` or calls `generate_legal`. |
| P0-3 no material tables | Done | Only `occupied == 2` with one commoner per color is a material draw. K+B/K+N/K+R capture counterexamples are tested; no insufficiency table was added. |
| Docs / changelog | Done | Updated `src/board.rs`, `src/lib.rs`, `README.md`, `CHANGELOG.md`, and the superseded note in `docs/plans/shakmaty/analysis.md`. |
| Verification | Done | Full tests, clippy, fmt check, docs, perft 41/41, probe, and timing check all pass. |

## Final precedence

Implemented in one private classifier (`Board::outcome_impl`):

1. side-to-move commoner extinct → `Loss`;
2. opponent commoner extinct → `Win`;
3. only the two commoners remain (`is_material_draw`) → `Draw` (WDL shortcut);
4. no legal moves: commoner attacked → `Loss`, otherwise `Draw`;
5. `rule50 >= 100` → `Draw`.

Branch 4 is resolved before branch 5 when the side to move is in check, so a
mate on the clock's final ply is a loss. When the side is not in check, the
50-move branch is decided without move generation. The only existence query is
the one that can change the answer.

`is_terminal()` is deliberately stricter than `outcome().is_some()`: it is true
only for extinction, no legal moves, or `rule50 >= 100`. A fresh K-vs-K board is
`outcome() == Some(Draw)` but `is_terminal() == false`.

## Probe output

Temporary example (created, run, then removed):

```text
8/8/8/8/8/8/4k3/4K3 w - - 0 1: outcome=Some(Draw) legal=4 checkers=0 occupied=2 material=true terminal=false
7K/8/8/8/8/8/1Q6/k7 b - - 0 1: outcome=Some(Loss) legal=0 checkers=1 occupied=3 material=false terminal=true
7K/8/8/8/8/8/1Q6/k7 b - - 100 1: outcome=Some(Loss) legal=0 checkers=1 occupied=3 material=false terminal=true
8/8/8/8/8/8/8/K7 w - - 0 1: outcome=Some(Win) legal=0 checkers=0 occupied=1 material=false terminal=true
4k3/8/8/8/8/8/8/4K3 w - - 0 1: outcome=Some(Draw) legal=5 checkers=0 occupied=2 material=true terminal=false
4k3/8/8/8/8/8/8/4K3 w - - 100 1: outcome=Some(Draw) legal=5 checkers=0 occupied=2 material=true terminal=true
8/8/8/8/8/8/8/8 w - - 0 1: outcome=Some(Loss) legal=0 checkers=0 occupied=0 material=false terminal=true
```

Final regression table in `terminal_precedence_table` matches this, including
the empty-board own-extinction tie-break.

### Plan inconsistency resolved

The plan's precedence list and its test table disagree for the touching-commoner
FEN `8/8/8/8/8/8/4k3/4K3 w - - 0 1`:

* the precedence says “only the two commoners remain → `Draw`”, and test 4 says
  `is_material_draw` is true for K vs K *including touching commoners*;
* the precedence test table lists that FEN as `outcome == None`.

Both cannot hold with one classifier. The implementation follows the explicit
precedence, so the touching-commoner position is a WDL `Draw`, not `None`. Its
`checkers == 0`, `legal == 4`, and `is_terminal == false` are unchanged, so the
touching-commoner regression is still fully pinned. The table row was updated to
the coherent value.

## Timing check

Same temporary probe, `--release`, 2M iterations per operation, after warm-up:

| FEN | `outcome()` | `populate_state + has_legal_move_with_state` | `generate_legal` |
| --- | ---: | ---: | ---: |
| Start position | 16.9 ns | 14.0 ns | 60.1 ns |
| `4r2k/3p4/2pB2p1/p6p/5pPP/2N1PP2/P1PP4/1R4RK w - - 0 22` | 13.6 ns | 13.9 ns | 131.1 ns |
| `3r3k/2rB3P/p7/P4p2/1p3Pp1/1P4P1/2p1p3/2R1R2K b - - 5 41` | 23.0 ns | 22.8 ns | 78.0 ns |

The first cold run showed a 43.9 ns start-position outlier; an immediate rerun
gave 16.9 ns, matching the state-aware path. `outcome()` no longer pays the
`generate_legal`/`MoveList` cost.

## Changed files

| File | Change |
| --- | --- |
| `src/board.rs` | `outcome_impl`, `outcome`, `outcome_with_state`, `outcome_with`, `is_material_draw`, strict `is_terminal`, docs, and tests. |
| `src/lib.rs` | Rules summary: terminal precedence, no insufficiency table, commoner-only WDL draw. |
| `README.md` | Feature list: state-aware outcome API, strict terminal predicate, no material table. |
| `CHANGELOG.md` | Unreleased fixed/changed/added entries for mate-vs-clock, commoner-only draw, new API, and rejection of insufficiency tables. |
| `docs/plans/shakmaty/analysis.md` | §3.5 and §8 note that follow-up 1 (port FSF insufficient material) is superseded by `feedback2.md` P0-3. |

## Tests added / updated

* `terminal_precedence_table` — all feedback FENs plus K-vs-K on the clock,
  asserting outcome, legal count, checker count, `has_legal_move`, and
  `is_terminal`.
* `outcome_state_aware_equivalence` — ten deterministic fixtures plus five
  seeded random-playout starts (8 seeds, 200 plies); `outcome()`,
  `outcome_with_state`, and `outcome_with(has_legal_move_with_state)` must
  agree at every visited position.
* `outcome_with_known_move` — caller-supplied `false` produces the stalemate
  draw (not in check) and the mate loss (in check), while `true` stays `None`.
* `material_draw_is_commoner_only` — K vs K and touching K vs K are true;
  K+B/K+N/K+R vs K are false and remain WDL-undecided until the tested capture
  removes the enemy commoner. A touching-commoners-plus-black-rook position is
  also false and remains `outcome() == None`, pinning that adjacency alone is
  not a draw when other material remains.
* Existing `test_outcome_draw_rule50` now uses a non-commoner-only position, so
  it tests the clock rather than the material shortcut.

## Problems and caveats

* **Stale `StateInfo` cannot be detected.** `outcome_with_state` and
  `outcome_with` trust the caller-populated state exactly as
  `generate_legal_with_state` and `has_legal_move_with_state` already do. The
  docs now explicitly warn that stale state changes extinction/check/stalemate
  results.
* **`outcome_with`'s `moves_exist` is authoritative.** Passing `false` for a
  live position intentionally returns a draw/loss according to `checkers`; this
  is documented and tested so the hot-path contract is unambiguous.
* **Artificial FENs.** The empty board is `Loss` because own extinction is
  checked first. Multi-commoner boards do not satisfy `is_material_draw`
  because both color counts must be exactly one. The only artificial position
  with new semantics is touching K vs K (now WDL `Draw`), as explained above.
  Touching commoners with any other piece on the board are not a material draw;
  adjacency is not the predicate.
* **No `is_terminal_with_state` form.** `is_terminal()` always creates and
  populates a fresh `StateInfo`, then runs the existence query. Consumers that
  already have populated state cannot reuse it yet.

## Verification commands and results

```sh
cargo test
# 70 lib tests + move_order_golden + perft_tests + verify_moves + doctest: all pass

cargo clippy --all-targets -- -D warnings
# clean

cargo fmt --check
# clean

cargo doc
# clean

cargo run --release --example verify_perft
# 41/41 passed, depths 1–6, total 74.585 s
```

## Remaining gaps / next steps

* Add `is_terminal_with_state` / `is_terminal_with(state, moves_exist)` if a
  consumer needs the strict predicate on the hot path; it can reuse the same
  state contract as `outcome_with`.
* Optionally expose the precedence as a crate-level doc example once the API
  stabilises.
* No perft or move-generation behavior changed; the 41/41 depth-6 run confirms
  this.
