# Plan 4 Report: Consistency / DRY / YAGNI

This report documents the changes made in `docs/plans/review1/plan4.md` to remove
dead code, reduce duplication, centralize helpers on the types that own them, and
keep the crate zero-dependency.

## Changes

### 1. `Cargo.toml` and `AGENTS.md` — stay zero-dependency

- `Cargo.toml` remains empty under `[dependencies]`.
- `AGENTS.md` no longer mentions `thiserror` or `strum`.
- The project stays a pure, zero-dependency Rust crate.

### 2. `src/types.rs` — centralize helpers, remove dead API

- Added `pub const PROMOTION_PIECES: [PieceType; 4]` in `types.rs` (the natural
  home because `Move::make_promotion` lives there).
- `Move::make_promotion` now looks up the promotion bit encoding from
  `PROMOTION_PIECES` instead of a hard-coded `match`.
- `Move::promotion_type` now returns `PROMOTION_PIECES[bits]` instead of a local
  `TYPES` array, keeping the encode/decode tables in one place.
- Removed the unused `Move::NULL` constant.
- Removed the unused `Direction` enum and the `impl ops::Add/Sub<Direction>` for
  `Square` operators.
- Removed the unused `Bitboard::more_than_one` method.
- Removed the unused `Bitboard`/`Square` operator impls
  (`BitAnd`, `BitOr`, `BitXor`, `Sub` for `Bitboard` with `Square` and
  `BitAnd`/`BitOr` for `Square` with `Square`).
- The one production call site that used `Bitboard ^ Square`
  (`Board::legal` `self.occupied() ^ from`) was updated to use
  `Bitboard::square_bb(from)`.
- Moved `Square::from_index` and `Square::from_u8` from `src/board.rs` to an
  `impl Square` block in `src/types.rs`.

### 3. `src/board.rs` — `Square` helpers moved out

- Removed the `Square` impl block that lived in `board.rs` (moved to `types.rs`).
- `FenError` remains a hand-written `Debug`/`Display`/`std::error::Error` type.

### 4. `src/bitboard.rs` — remove dead constants and `#![allow(dead_code)]`

- Removed `#![allow(dead_code)]`.
- Removed the unused `FILE_*BB`, `RANK_*BB`, and `ALL_SQUARES` constants.
- Removed the `test_file_rank_constants` test that only exercised those constants.
- Removed the public `line_bb` module function.
- Moved `line_bb` and `aligned` helpers into the `#[cfg(test)]` module, where they
  read the `LINE_BB` table from `attacks.rs`.

### 5. `src/attacks.rs` — gate `LINE_BB` to tests

- `compute_line_bb` and the `LINE_BB` static table are now `#[cfg(test)]`.
- Only `BETWEEN_BB` remains in the production build.

### 6. `src/movegen.rs` — DRY the piece-move loops

- Removed the local `PROMOTION_PIECES` constant (now imported from `types`).
- Added a generic `generate_piece_moves` helper that takes a piece type and an
  attack closure.
- Used it for knights, bishops, rooks, queens, and commoners.
- Pawn and castling generation remain separate because they have special logic.

### 7. `src/magic.rs` — offset duplication was already resolved

- `build_magic_table` already reads `entries[sq].offset` from `ROOK_ENTRIES`/
  `BISHOP_ENTRIES`.
- `ROOK_OFFSETS`/`BISHOP_OFFSETS` no longer exist (removed in plan 2).
- `total_table_size` lives in `src/util.rs` and is used for `ROOK_TABLE_SIZE` and
  `BISHOP_TABLE_SIZE`.

## Public API changes

- `Direction` and `Move::NULL` are gone.
- `Bitboard::more_than_one` is gone.
- `Bitboard`/`Square` binary operator impls (`BitAnd`, `BitOr`, `BitXor`, `Sub`)
  are gone.
- `Square::from_index`/`from_u8` are now defined in `types.rs` (same public API,
  better location).
- `PROMOTION_PIECES` is now public from `types.rs`.
- `FenError` remains a hand-written `std::error::Error`.
- The crate remains zero-dependency.

## Performance impact

- `cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6` —
  returned `118926425` in ~0.79 s.
- `cargo run --release --example verify_perft 6` — passed 41/41 positions,
  total time 56.328 s.

The `movegen.rs` refactor introduces a generic helper with closure dispatch in the
hottest path; the helper is marked `#[inline(always)]` so the compiler can still
monomorphize and inline it. `verify_perft 6` after the dependency reversion is
roughly on par with plan 3.

## Unexpected observations

- No test code broke after removing `Direction`, `Move::NULL`, `more_than_one`, or
  the `Bitboard`/`Square` operators.
- `PROMOTION_PIECES` was reordered from the previous `[Queen, Rook, Bishop, Knight]`
  (already the order used in `movegen.rs`) and is now used consistently for both
  move generation and `Move` promotion encoding. The change in promotion bit
  encoding is internal-only; `Move::promotion_type` still returns the correct
  `PieceType`.
- Keeping `FenError` hand-written and not adding `thiserror`/`strum` keeps the
  crate zero-dependency while preserving identical behavior.

## Verification

- `cargo build` — passed.
- `cargo clippy` — passed.
- `cargo fmt` — passed.
- `cargo test` — passed (32 unit tests, 4 perft tests, 1 move verification test).
- `cargo test --test verify_moves` — passed.
- `cargo run --release --example verify_perft 5` — passed (41/41 positions).
- `cargo run --release --example verify_perft 6` — passed (41/41 positions).
- `cargo doc` — passed.

## Notes for plan 5

- `bitboard.rs` and `attacks.rs` are cleaned up; `LINE_BB` is test-only.
- `Square`/`Move` helpers are centralized in `types.rs`.
- `movegen.rs` is refactored.
- `Cargo.toml` remains zero-dependency; `AGENTS.md` no longer recommends external crates.
- The public API is smaller; plan 5 should document it and consider adding the
  `missing_docs` lint if appropriate.
