# Plan 4: Consistency / DRY / YAGNI

## Goal

Remove dead code, reduce duplication, move type helpers to the modules that own the types, and align the dependency set with `AGENTS.md`.

## Scope

- `src/bitboard.rs` (dead code and `#![allow(dead_code)]`)
- `src/attacks.rs` (`LINE_BB`)
- `src/types.rs` (`Direction`, `Move::NULL`, `more_than_one`, `Square` helpers)
- `src/movegen.rs` (repetitive loops, `PROMOTION_PIECES` location)
- `src/magic.rs` and `src/pext.rs` (offset duplication, shared helpers)
- `Cargo.toml` (dependencies for `thiserror`/`strum` if adopted)

## Current state (from review and previous plans)

- `bitboard.rs` has `#![allow(dead_code)]` hiding `FILE_*`, `RANK_*`, `ALL_SQUARES`, `line_bb`, `aligned`, and `more_than_one`.
- `LINE_BB` in `attacks.rs` is 32 KB of dead data (only `BETWEEN_BB` is used).
- `Direction` enum and `Square` `Add`/`Sub` impls are unused.
- `Move::NULL` is unused.
- `Square::from_index`/`from_u8` are in `board.rs` instead of `types.rs`.
- `PROMOTION_PIECES` is in `movegen.rs` while `Move::make_promotion` is in `types.rs`.
- `movegen.rs` has nearly identical loops for knights, bishops, rooks, queens, and commoners.
- `magic.rs` has both `ROOK_OFFSETS`/`BISHOP_OFFSETS` and `MagicEntry.offset`.
- `Cargo.toml` has no dependencies, but `AGENTS.md` says to use `thiserror` and `strum`.

## Prerequisites

- Read `report3.md` for any `from_fen`/`fen`/`rule50` changes that may affect `FenError` or error handling.
- Decide whether to add `thiserror`/`strum` dependencies. If the project wants to remain zero-dependency, defer to plan 5 to update `AGENTS.md` instead.

## Tasks

1. **Clean up `bitboard.rs`**
   - Remove `#![allow(dead_code)]`.
   - Remove `FILE_*BB`, `RANK_*BB`, `ALL_SQUARES` if they are not used anywhere.
   - Move `line_bb` and `aligned` to `#[cfg(test)]` modules (or remove if not used outside tests).
   - Verify `between_bb` is still used and keep it.

2. **Remove or gate `LINE_BB` in `attacks.rs`**
   - Move `compute_line_bb` and `LINE_BB` to `#[cfg(test)]` or delete them.
   - Remove the `#[allow(dead_code)]` on `compute_line_bb`.

3. **Remove `Direction` and unused `Square`/`Bitboard` operator impls**
   - Delete the `Direction` enum.
   - Delete `impl ops::Add<Direction> for Square` and `impl ops::Sub<Direction> for Square` if they are unused.
   - `Bitboard` `BitAnd`/`BitOr`/`BitXor`/`Sub` for `Square` — verify usage; if only used in tests, gate them to `#[cfg(test)]`.

4. **Remove `Move::NULL` and `Bitboard::more_than_one` if still unused**
   - Search the codebase; if no references, delete.

5. **Move `Square` helpers to `types.rs`**
   - Move `impl` block containing `from_index` and `from_u8` from `src/board.rs` to `src/types.rs`.
   - Ensure `Square::from_u8` is still accessible from `magic.rs`/`pext.rs`.

6. **Move `PROMOTION_PIECES` to `types.rs`**
   - Define `pub const PROMOTION_PIECES: [PieceType; 4] = [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight];` in `types.rs`.
   - Update `movegen.rs` and `Move::make_promotion` to use it.

7. **Refactor `movegen.rs` repetitive loops**
   - Create a small helper (or macro) that takes:
     - the piece bitboard
     - the attack function (`knight_attacks`, `bishop_attacks`, etc.)
     - the move constructor
   - Use it for knights, bishops, rooks, queens, and commoners.
   - Keep the logic identical to avoid functional changes.

8. **DRY in `magic.rs` and `pext.rs`**
   - `build_magic_table` should use `ROOK_ENTRIES`/`BISHOP_ENTRIES` `.offset` instead of a separate `offsets` slice.
   - Delete `ROOK_OFFSETS` and `BISHOP_OFFSETS`.
   - Move `total_table_size` to a shared module (e.g., `src/util.rs`) and use it in both `magic.rs` and `pext.rs` (if `pext` still exists).

9. **Optional: add `thiserror` and `strum` dependencies**
   - If the project adopts dependencies:
     - Add `thiserror = "2"` and `strum = { version = "0.26", features = ["derive"] }` to `Cargo.toml`.
     - Convert `FenError` to `#[derive(Error, thiserror::Error)]`.
     - Add `FromRepr` derives to `Square`, `PieceType`, `Color`, `MoveType`, `Rank`, `File` where appropriate.
   - If dependencies are not adopted, note in `report4.md` that `AGENTS.md` should be updated in plan 5.

10. **Run tests and linting**
    - `cargo build`
    - `cargo clippy`
    - `cargo test`
    - `cargo run --release --example verify_perft 5`
    - `cargo doc` (check for missing docs)

11. **Write `report4.md`**
    - Document what was removed and why.
    - Note any public API changes (e.g., `Direction` gone, `Square` helpers moved).
    - Record the decision on `thiserror`/`strum` and any dependency additions.
    - Mention any unexpected test breakage from dead-code removal.

## Notes for plan 5

- `bitboard.rs` and `attacks.rs` are cleaned up; `LINE_BB` is gone.
- `Square`/`Move` helpers are centralized in `types.rs`.
- `movegen.rs` is refactored.
- Any remaining `AGENTS.md` conflicts (dependencies, `unsafe`) should be resolved in plan 5.
- Public API is now cleaner; plan 5 should document it and add `missing_docs` lint if appropriate.
