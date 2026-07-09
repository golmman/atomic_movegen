# Plan 5: Documentation / rules

## Goal

Align public documentation and project rules with the actual implementation; add missing doc comments and panic/error sections; close out the review cycle.

## Scope

- `src/lib.rs` (module-level docs and rules description)
- `AGENTS.md` (project rules)
- `Cargo.toml` (dependency / feature policy alignment if needed)
- `README.md` (optional, if needed)
- All public API doc comments
- `docs/plans/review1/report5.md` (final summary)

## Current state (from review and previous plans)

- `lib.rs` doc says “Adjacent COMMONERs (even own) are illegal,” which does not match the implemented `Fairy-Stockfish` `atomic` rules (`extinctionPseudoRoyal` only for the last commoner, no own-adjacency restriction, no `mutuallyImmuneTypes` for commoners).
- `AGENTS.md` says zero `unsafe` and use `thiserror`/`strum`, but the code may not match depending on decisions in plans 2 and 4.
- Many public items lack `# Panics`/`# Errors` sections (e.g., `from_fen`, `type_of`, `parse_sq`, `make_move`).
- There is no `missing_docs` lint.

## Prerequisites

- Read `report4.md` to know the final state of the code: which dependencies were added, whether PEXT was removed or feature-gated, and which public API items moved or were removed.

## Tasks

1. **Update `src/lib.rs` module-level docs**
   - Describe the exact variant implemented: `Fairy-Stockfish` `atomic` (blast on capture, commoner is pseudo-royal only when it is the last one, pawns are not removed by the blast unless the blast square is `to` for a pawn capture, no check/mate in the usual sense).
   - Remove the incorrect “Adjacent COMMONERs (even own) are illegal” statement.
   - Clarify that the crate is not implementing `atomar` (`mutuallyImmuneTypes`/`blastImmuneTypes` for commoners).

2. **Update `AGENTS.md`**
   - Update the `Conventions` section to reflect the actual dependency policy:
     - If `thiserror`/`strum` were added in plan 4, keep them and mention they are used.
     - If not, remove or soften the `thiserror`/`strum` rule.
   - Update the `unsafe` rule:
     - If PEXT was removed, keep “zero unsafe by default.”
     - If PEXT was kept behind a feature using `core::arch`, state “zero unsafe by default; the `pext` feature uses `unsafe` for target-specific intrinsics.”
   - Add any new commands added during implementation (e.g., `--features pext` test command).

3. **Add `# Panics` and `# Errors` sections to public methods**
   - `Board::from_fen` — list error cases and `FenError` variants.
   - `Piece::type_of` / `Piece::color` — note they panic on `NO_PIECE`.
   - `Board::piece_on` / `Board::empty` — note they are safe for `Square::NONE` (return `NO_PIECE`/`true`).
   - `parse_sq` — document `Option<Square>` return and `None` for invalid input.
   - `sq_str` — document `Option<String>` return and `None` for `Square::NONE`.
   - `Move::make_move`, `make_promotion`, `make_enpassant`, `make_castling` — document `panic` on `Square::NONE` or invalid promotion piece.
   - `Board::legal` — document `state` requirements.
   - `Board::do_move` / `undo_move` — document `state` usage and that `do_move` no longer populates checkers/pinned.

4. **Enable `missing_docs` lint (optional but recommended)**
   - Add `#![warn(missing_docs)]` to `src/lib.rs` and clean up any new warnings.
   - This may surface other missing docs; fix them in this plan.

5. **Update `README.md` if needed**
   - If the FEN piece characters changed, or if the `pext` feature was added, or if `rule50` behavior changed, update the README examples.
   - Add a note about the `atomic` variant implemented and reference Fairy-Stockfish.

6. **Run full verification suite**
   - `cargo build`
   - `cargo clippy`
   - `cargo test`
   - `cargo run --release --example verify_perft 6` (or the highest depth the machine can handle in a reasonable time)
   - `cargo test --test verify_moves`
   - `cargo doc` (check for warnings)
   - If `pext` feature exists: `cargo build --features pext` and `cargo test --features pext` on x86-64.

7. **Final sanity check on `AGENTS.md` conflicts**
   - Verify the crate is zero-unsafe by default (if that was the goal).
   - Verify the dependency list matches the convention.
   - Verify no remaining `#[allow(dead_code)]` or `#![allow(dead_code)]` hiding real unused code.

8. **Write `report5.md`**
   - This is the final report. Summarize:
     - What was implemented across all 5 plans.
     - The most important problems and surprises (e.g., `Piece::type_of` UB, `do_move` double `populate_state`, PEXT AMD issue, `rule50` overflow, `AGENTS.md` conflicts).
     - Workarounds chosen (e.g., `assert` on `NO_PIECE`, removing PEXT, `u16` `rule50`, `Option` returns for `parse_sq`/`sq_str`).
     - Performance numbers before/after.
     - Any deferred or unresolved decisions (e.g., whether to re-add PEXT later, whether to use `strum`).
     - Recommendations for future work or the next review cycle.

## Notes for future work

- Plan 5 closes the review cycle. The final code state should be safe, tested, and documented.
- Any future performance work (e.g., re-adding a fast PEXT path for Intel) should be done as a new plan after this one, with a fresh `AGENTS.md` review if needed.
- `report5.md` should be the first file read by anyone continuing from this review.
