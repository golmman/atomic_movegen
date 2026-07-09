# Plan 5 Report: Documentation / rules

This report documents the changes made in `docs/plans/review1/plan5.md` to align
public documentation and project rules with the final implementation and close
out the review cycle.

## Changes

### 1. `src/lib.rs` — corrected module-level docs

- Updated the crate-level documentation to describe the exact variant
  implemented: the standard `atomic` variant, validated against Fairy-Stockfish.
- Removed the incorrect "Adjacent COMMONERs (even own) are illegal" statement.
- Clarified the pseudo-royal rule: a commoner is only pseudo-royal when it is
  the side's last commoner.
- Clarified that the crate does not implement `atomar` rules
  (`mutuallyImmuneTypes` / `blastImmuneTypes` for commoners).
- Documented the pawn-blast nuance: pawns are immune to the blast except when a
  pawn is the capturer at the blast square (`to`), in which case it is also
  destroyed.
- Added `#![warn(missing_docs)]` to enforce documentation coverage.

### 2. `AGENTS.md` — aligned conventions with the implementation

- Removed the stale `thiserror`/`strum` convention (the crate remains
  zero-dependency).
- Updated the `unsafe` convention to "avoid `unsafe` by default; `unsafe` is
  acceptable if it is needed for a measurable performance win and is clearly
  documented" (PEXT was removed in plan 2).
- Added `cargo doc` and `cargo test --test verify_moves` to the command list.
- Added conventions to document `# Panics`/`# Errors` sections and keep the
  `missing_docs` build warning-free.

### 3. Public API documentation

- Added `# Panics` and `# Errors` sections to public methods:
  - `Board::from_fen` — lists all `FenError` variants and error cases.
  - `Piece::type_of` / `Piece::color` — note they panic on `NO_PIECE`.
  - `Board::piece_on` / `Board::empty` — note they are safe for `Square::NONE`.
  - `parse_sq` / `sq_str` — document the `Option` return and `None` cases.
  - `Move::make_move`, `make_promotion`, `make_enpassant`, `make_castling` —
    document the panic on `Square::NONE` or invalid promotion piece.
  - `Board::legal` — documented `state` requirements.
  - `Board::do_move` / `Board::undo_move` — documented `state` usage and that
    `do_move` no longer populates checkers/pinned.
- Documented all `StateInfo` public fields.
- Added module-level docs to `src/attacks.rs`, `src/board.rs`, `src/movegen.rs`,
  and `src/types.rs`.

### 4. `README.md` — updated project description

- Changed the header to note that the variant is validated against Fairy-Stockfish.
- Updated the feature list to note the pseudo-royal commoner rule and the pawn
  blast exception.
- Added "pure safe Rust, zero dependencies".
- Fixed the `verify_perft` `--release` command order.

### 5. `missing_docs` lint

- Added `#![warn(missing_docs)]` to `src/lib.rs`.
- Resolved all 191 warnings by adding module docs, struct/field docs, method
  docs, and by allowing `missing_docs` on the repetitive `Square`/`File`/
  `Rank`/`Color`/`PieceType`/`MoveType` enum variants and `SQ_*` constants
  (which are self-explanatory aliases).

## Public API changes

No functional API changes were made in plan 5. The only changes are additions
and corrections to documentation and project rules.

## Performance impact

Plan 5 did not touch the move generation or board logic. The release
`verify_perft 6` run on the same Apple M4 machine used for previous plans:

```
Total time:  57.357 s
Result:      41/41 passed, 0/41 failed
```

This is within the normal run-to-run variance of the previous `verify_perft 6`
results (plan 2: 56.739 s, plan 3: 56.832 s, plan 4: 56.328 s). The starting
position perft depth 6 still returns `118926425` and completes in under a
second.

## Most important problems and surprises across the review cycle

- **`Piece::type_of` UB on `NO_PIECE`:** the original code computed
  `(self.0 & 7) - 1` for `NO_PIECE` and `transmute`d the result. With
  `overflow-checks = false`, this underflowed to `255` and was UB. Fixed in
  plan 1 by using a `PIECE_TYPES` table lookup and `debug_assert`.
- **`Board::piece_on` / `Board::empty` out of bounds on `Square::NONE`:**
  `Square::NONE` has discriminant `64`, causing `squares[64]` access. Fixed in
  plan 1 by returning `NO_PIECE` / `true` for `Square::NONE`.
- **`parse_sq` silently returned `A1` for invalid input and `sq_str` returned
  `"a9"` for `Square::NONE`:** fixed in plan 1 by changing both to return
  `Option`.
- **`do_move` called `populate_state` redundantly:** `do_move` populated cached
  attack fields, then `generate_legal` immediately populated them again on a
  fresh `StateInfo`. Fixed in plan 2 by removing the redundant call.
- **PEXT forced on every BMI2 CPU:** the original PEXT inline-assembly path was
  slow on AMD Zen and used `unsafe`. Fixed in plan 2 by removing PEXT from the
  default build and always using magic bitboards.
- **`rule50` was `u8` and overflowed after 255 moves:** fixed in plan 3 by
  migrating to `u16`.
- **`from_fen` accepted malformed FEN:** 5-field FEN, stale castling rights,
  stale en-passant, and invalid placement were all accepted. Fixed in plan 3.
- **`update_castling_rights` over-cleared and under-cleared:** cleared rights on
  non-capture moves to corner squares and missed captures of the commoner on
  `E1`/`E8`. Fixed in plan 3.
- **`AGENTS.md` conflicts:** `AGENTS.md` originally demanded `thiserror`/`strum`
  and zero `unsafe` while the code used neither and contained `unsafe`. The
  conflicts were resolved across plans 2 and 5: PEXT removed, dependencies not
  added, and `AGENTS.md` updated.

## Workarounds chosen

- `Piece::type_of` and `Piece::color` use `debug_assert!` on `NO_PIECE` and a
  safe `PIECE_TYPES` lookup, avoiding `unsafe` transmute.
- PEXT was removed entirely from the default build rather than feature-gated,
  keeping the default build free of `unsafe` blocks.
- `rule50` was widened to `u16` to avoid overflow without changing `fen()`
  formatting.
- `parse_sq` and `sq_str` now return `Option`, with call sites using
  `unwrap_or("??")` for FEN output.
- `FenError` remains a hand-written `Display`/`Error` implementation, keeping
  the crate zero-dependency.

## Performance numbers before/after

| Plan | `verify_perft 6` (41 positions) | Source |
| ---- | ------------------------------ | ------ |
| Baseline (before plan 2) | 59.751 s | `docs/perf/m4/2026-07-09_plan2_baseline.txt` |
| After plan 2 | 56.739 s | `report2.md` |
| After plan 3 | 56.832 s | `report3.md` |
| After plan 4 | 56.328 s | `report4.md` |
| After plan 5 | 57.357 s | this report |

The perft numbers remain stable. The 57.357 s run is within normal variance of
previous results. Plan 5 added no runtime code, so no performance regression is
expected.

## Deferred or unresolved decisions

- **PEXT re-addition:** A future plan could re-introduce a PEXT path for Intel
  CPUs behind a `pext` feature or `target_feature` guard, using
  `core::arch::x86_64::_pext_u64`. This would require a fresh `AGENTS.md` review.
- **`strum`/`thiserror`:** The project remains zero-dependency. If the public
  API grows much larger, a future review could re-evaluate these crates.
- **Commoner capture-check rule:** The current implementation matches
  Fairy-Stockfish's `extinctionPseudoRoyal` with `extinctionPieceCount = 0`.
  This is the standard `atomic` variant; `atomar` rules are not implemented.

## Verification

- `cargo build` — passed, no warnings.
- `cargo clippy` — passed.
- `cargo fmt --check` — passed.
- `cargo test` — passed (32 unit tests, 4 perft tests, 1 move verification test).
- `cargo test --test verify_moves` — passed.
- `cargo run --release --example verify_perft 6` — passed (41/41 positions).
- `cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6` — passed, returned `118926425`.
- `cargo doc` — passed with no warnings.

## Recommendations for future work

- Keep the `missing_docs` lint enabled. Any new public item must be documented
  before merging.
- If performance work is revisited, do it as a new plan and update `AGENTS.md`
  accordingly (e.g., for a `pext` feature).
- Consider adding `cargo doc` and `cargo test --test verify_moves` to any CI
  pipeline based on `AGENTS.md`.
- `report5.md` should be the first file read by anyone continuing from this
  review.
