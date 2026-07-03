# Code Cleanup Plan

## Summary

Audit findings organized by category. Each item has a priority (P0 = highest) and an estimated effort.

---

## 1. Performance

| # | Issue | Priority | Effort | Description |
|---|-------|----------|--------|-------------|
| P1 | `legal()` re-checks `is_move_trivially_legal()` | P0 | 5 min | `generate_legal` (movegen.rs:247) calls `is_move_trivially_legal` first, then `board.legal()` (board.rs:702) calls it again. The second call is pure waste. Either remove the check from `legal()` since the caller already checked, or use a flag parameter. |
| P2 | Triple-computed sliding attacks in `compute_checkers` | P0 | 5 min | board.rs:362-364 calls `rook_attacks`, `bishop_attacks`, then `queen_attacks(rook \| bishop)` which re-computes both. The queen variable is only used for the queen piece check — replace with reuse of existing variables. |
| P3 | Quadruple-computed sliding attacks in castling check | P0 | 5 min | board.rs:721-748 computes `rook_atk`, `bishop_atk`, and `queen_atk` (= rook\|bishop), then uses all three separately. Merge into shared variables and eliminate redundancy. |
| P4 | Unused destructure in `generate_pawn_moves_for` | P1 | 2 min | movegen.rs:93 unpacks `_to_rank` and `_to_file_inc` which are never used. Remove them. |
| P5 | Unused variables `_idx` and `_sq` in `Board::fmt` | P1 | 2 min | board.rs:948-959 declares `_idx` and `_sq` that are never read. Remove them. |
| P6 | `adjacent_files_bb` defined but never called | P2 | 2 min | bitboard.rs:38 — dead function, remove it. |
| P7 | `Direction::from()` panics on invalid input | P2 | 10 min | types.rs:663-669 uses `panic!` for invalid direction values. Make it return `Option<Direction>` or use safe arithmetic. |

---

## 2. DRY (Don't Repeat Yourself)

| # | Issue | Priority | Effort | Description |
|---|-------|----------|--------|-------------|
| D1 | `SQUARES` lookup table defined **4 times** verbatim | **P0** | 20 min | The 64-element `SQUARES` array exists in: `types.rs` in `make_square()`, `types.rs` in `Add<Direction>`, `board.rs` in `Square::from_index()`. Consolidate into a single `const SQUARES: [Square; 64]` in `types.rs` (or the `Square` impl) and reference it everywhere. |
| D2 | Pawn promotion list repeated twice | P1 | 5 min | `[Queen, Rook, Bishop, Knight]` appears in movegen.rs:105-110 and 143-149. Extract to a const `PROMOTION_PIECES`. |
| D3 | Piece-to-character mapping repeated 4× | P1 | 15 min | The `color -> char, type -> char` mapping appears in: `Piece::fmt` (types.rs), `Board::fen` (board.rs:198-209), `Board::fmt` (board.rs:989-1001), and `char_to_piece` (board.rs:6-23). Add a `fn ascii_char(self) -> char` method to `Piece` (or `fn to_uci_char(self) -> char`) and use it everywhere. `char_to_piece` can stay as the inverse. |
| D4 | Castling square mapping repeated 4× | P1 | 15 min | The `(kfrom, kto, rfrom, rto)` pattern for white/black, king-side/queen-side appears in `do_move`, `undo_move`, `legal`, and `generate_castling`. Extract to a helper function or const table indexed by `(Color, bool is_kingside)`. |
| D5 | Attack-check pattern repeated 3× | P0 | 15 min | The block checking rook+bishop+queen+knight+pawn attackers against a square, filtered by enemy survivors, appears in `legal()` for castling (board.rs:724-748), `legal()` for pseudo-royal check (board.rs:862-883), and `compute_checkers()` (board.rs:362-374). Extract into `fn is_square_attacked_by(board, sq, occupied, enemy_bb) -> bool`. |
| D6 | `sq_str` and `parse_sq` duplicated across 4 example files | P1 | 10 min | Every example has its own `fn sq_str(Square) -> String` and `parse_sq` (list_moves, debug_moves, fen_after, pawn_debug, perft_divide). Add public helper functions to the library (e.g. in `types.rs` or a new `util` module) or use a shared mini module. |
| D7 | `Square::from_u8` is a thin delegator to `from_index` | P2 | 2 min | board.rs:1167-1169 just calls `from_index(idx as i8)`. Either inline it or remove one. |
| D8 | `color_of` / `type_of` free functions in `types.rs` | P2 | 5 min | types.rs:544-549 are trivial wrappers around `Piece::color()` / `Piece::type_of()`. They're used in the test module only (2 calls each). Remove them and use the method syntax directly. |
| D9 | `bitboard.rs` free functions are wrappers around `Bitboard` methods | P2 | 5 min | `square_bb`, `file_bb`, `rank_bb`, `popcount`, `lsb`, `more_than_one`, `pop_lsb` (bitboard.rs:26-127) just delegate to `Bitboard::` methods. The only external user is `attacks.rs` which uses `square_bb`. Remove the free functions and use `Bitboard::` methods directly. |

---

## 3. YAGNI / Dead Code

| # | Issue | Priority | Effort | Description |
|---|-------|----------|--------|-------------|
| Y1 | `DARK_SQUARES` constant defined but never used | P2 | 2 min | bitboard.rs:22 — remove it. |
| Y2 | `adjacent_files_bb` defined but never used | P2 | 2 min | bitboard.rs:38 — remove it. |
| Y3 | `bishop_attacks_loop` / `rook_attacks_loop` public but only used in magic tests | P2 | 5 min | magic.rs:552-557 — make them `#[cfg(test)]` only or move to the test module. |
| Y4 | `#[allow(dead_code)]` on entire `pext.rs` module | P2 | 5 min | pext.rs:18. The tables/functions are indeed used via `unsafe { pext::bishop_attacks_pext(...) }` in `attacks.rs`, but the `dead_code` lint triggers because the dispatch indirection confuses the compiler. Restrict the allow to the minimum scope instead of the whole file. |
| Y5 | `PERFT_INIT` Once in `lib.rs` | P2 | 10 min | lib.rs:12 uses a `Once` to ensure `attacks::init()` runs once. But `attacks::init()` already uses `OnceLock` internally — this outer guard is redundant. Remove it and let each perft call assume init already happened (or move init to the public `perft` function). |

---

## 4. Inconsistencies

| # | Issue | Priority | Effort | Description |
|---|-------|----------|--------|-------------|
| I1 | `Board::occupied()` is a trivial alias for `Board::pieces()` | P1 | 5 min | board.rs:309-311. Both are used inconsistently across the codebase (sometimes `self.occupied()`, sometimes `self.pieces()`). Choose one and stick with it. `occupied()` is more standard in chess engines. |
| I2 | "COMMONER" spelling vs "COMMONER" — actually consistent | — | — | No action needed (I checked and it's consistently "Commoner"). |
| I3 | `StateInfo` mixed hot/cold field ordering | P2 | 10 min | The struct in board.rs:27-42 has `checkers`, `pinned`, `commoners_count`, `them_commoners_count` followed by colder fields. The current layout is already reasonable, but verify cache-line alignment using `#[repr(C)]` and reorder hot fields first for potential perf gains. |
| I4 | `is_ok(sq)` free function vs `(0..64).contains(&idx)` pattern | P2 | 2 min | types.rs:64 has a `pub fn is_ok(Square) -> bool` that's never used after type safety improvements. Remove it. |

---

## 5. Unnecessary / Outdated Comments

| # | Location | Priority | Effort | Issue |
|---|----------|----------|--------|-------|
| C1 | movegen.rs:19, 31, 43, 55, 67 | P1 | 3 min | `// Knight moves`, `// Bishop moves`, etc. — these comments are redundant with the code (function names and variable names are self-explanatory). Remove them. |
| C2 | board.rs:698 | P2 | 1 min | `// Pre-compute is_capture (needed for early-out and later logic)` — obvious. Trim to just `let is_capture = ...`. |
| C3 | board.rs:707, 754, 760, 773, 781 | P2 | 3 min | Overly verbose "step-by-step" comments for castling, en-passant, blast logic. Keep only the non-obvious invariants. |
| C4 | pext.rs:1-17 | P2 | 2 min | The module-level doc comment is informative — keep it (it's not unnecessary). But the "Dead-code note" (lines 11-16) can be removed once the `#[allow]` is scoped properly. |
| C5 | board.rs:824-835 (extinction pseudo-royal reference) | — | — | This is valuable — keep it as-is. It documents the Fairy-Stockfish reference behavior. |
| C6 | movegen.rs:254-257 (SAFETY comment on set_len) | — | — | This is appropriate due to unsoundness risk — keep it. |

---

## 6. Structural / Miscellaneous

| # | Issue | Priority | Effort | Description |
|---|-------|----------|--------|-------------|
| M1 | Remove `_`-prefixed unused variables | P1 | 5 min | `_to_rank`, `_to_file_inc` (movegen.rs:93), `_idx`, `_sq` (board.rs:948-949). Either remove them or prefix properly — they're already prefixed but the match arms still compute them. |
| M2 | `unsafe` in `file_of` / `rank_of` | P2 | 15 min | types.rs:122, 128 use `unsafe { std::mem::transmute }` for File/Rank enum conversion. Since `File` and `Rank` are `#[repr(u8)]` with exactly 8 variants, this is sound but ugly. Could be replaced with a lookup table or `From<u8>` impl. |
| M3 | `Piece::type_of` uses `wrapping_sub(1)` for safety net | P2 | 5 min | types.rs:488-492. The `debug_assert!` is fine, but the `wrapping_sub` is a code smell — if the `Piece` encoding is wrong, it silently produces wrong results. Replace with non-wrapping and let the debug_assert catch it. |
| M4 | No `Copy` on `StateInfo` | P1 | 5 min | `StateInfo` is passed by reference everywhere but only contains trivial `Copy` types. Derive `Copy` or replace with `Copy` + pass-by-value for tiny structs. |

---

## Recommended Execution Order

### Phase 1 — Safety & Perf (P0 items): ~1 hour
1. P1 — Remove redundant `is_move_trivially_legal` call in `legal()`
2. P2, P3 — Deduplicate sliding attack computations
3. D5 — Extract `is_square_attacked_by()` helper (unblocks P2/P3)
4. D1 — Consolidate `SQUARES` lookup table (single source of truth)

### Phase 2 — DRY cleanup: ~1 hour
5. D3 — Add `Piece::to_uci_char()` and use everywhere
6. D4 — Extract castling square table
7. D2 — Extract `PROMOTION_PIECES` const
8. D6 — Add public `sq_str` / `parse_sq` to the library

### Phase 3 — Dead code & YAGNI: ~30 min
9. Y1–Y4 — Remove dead code
10. I1 — Normalize `occupied()` vs `pieces()`
11. I4 — Remove `is_ok()`
12. D7–D9 — Remove thin wrappers

### Phase 4 — Comments & cleanup: ~30 min
13. C1–C4 — Trim verbose/obvious comments
14. P4–P7 — Remove unused variables, fix `Direction::from()`
15. M2–M4 — Fix `transmute`, `wrapping_sub`, `Copy` derives

---

## Verification

After each phase, run:
```sh
cargo build && cargo test && cargo clippy && cargo fmt
```

For comprehensive correctness verification:
```sh
cargo run --example verify_perft 4
```

For deeper checks (may need `--release`):
```sh
cargo run --example verify_perft 6
```
