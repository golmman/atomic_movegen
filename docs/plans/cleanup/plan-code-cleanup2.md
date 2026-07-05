# Code Cleanup Plan 2

## Context

This is the second round of cleanup. The first round (see `plan-code-cleanup.md` / `report-code-cleanup.md`) addressed 41 items across the codebase. This round focuses on what remains: dead code exposed as `pub` surface, unused methods, and a few structural inconsistencies.

Build is clean (`cargo build`, `cargo test` 41/41 + 4/4 + 1/1, `cargo clippy` clean, `cargo fmt --check` clean).

---

## 1. Dead Code — Public API Surface (YAGNI)

Functions/types exported `pub` but never called anywhere in the crate (no internal or example usage). Removing them shrinks the API and reduces maintenance burden.

### Y2-1. `attacks_bb` — dead dispatch function

**File:** `src/attacks.rs:311`  
**Issue:** `pub fn attacks_bb(pt, sq, occupied) -> Bitboard` matches on piece type and delegates to the correct attack function. Never called anywhere in the crate.  
**Action:** Remove it (or gate behind `#[cfg(test)]` if tests want it).  
**Effort:** 2 min.

### Y2-2. `pawn_attacks_bb` / `pawn_attacks_from` — dead pawn helpers

**File:** `src/bitboard.rs:64-73`  
**Issue:** Two `pub` functions compute pawn attack bitboards from a set of pawns or a single square. They use the shift helpers (also dead). The crate already uses `attacks::pawn_attacks()` (compile-time table) instead.  
**Action:** Remove both functions.  
**Effort:** 3 min.

### Y2-3. `shift_north` / `shift_south` / `shift_east` / `shift_west` / `shift_ne` / `shift_nw` / `shift_se` / `shift_sw` — dead shift helpers

**File:** `src/bitboard.rs:24-62`  
**Issue:** Eight `pub` shift functions. Only used by `pawn_attacks_bb` (also dead). The crate uses `Bitboard` `<<` / `>>` ops and file-masked shifts inline elsewhere.  
**Action:** Remove all eight functions.  
**Effort:** 5 min.

### Y2-4. `Bitboard::ALL` — dead constant

**File:** `src/types.rs:314`  
**Issue:** `pub const ALL: Bitboard = Bitboard(!0u64)`. Never referenced outside its definition.  
**Action:** Remove.  
**Effort:** 1 min.

### Y2-5. `Bitboard::msb` — dead method

**File:** `src/types.rs:332`  
**Issue:** `pub fn msb(self) -> Square`. Never called.  
**Action:** Remove.  
**Effort:** 1 min.

### Y2-6. `Piece::is_ok` — dead method

**File:** `src/types.rs:512`  
**Issue:** `pub fn is_ok(self) -> bool`. Never called.  
**Action:** Remove.  
**Effort:** 1 min.

### Y2-7. `SQ_NONE` — dead const alias

**File:** `src/types.rs:138`  
**Issue:** `pub const SQ_NONE: Square = Square::NONE`. Every usage site uses `Square::NONE` directly.  
**Action:** Remove the const.  
**Effort:** 1 min.

### Y2-8. `Color::to_usize` — dead method

**File:** `src/types.rs:458`  
**Issue:** `pub fn to_usize(self) -> usize`. Never called (callers use `c as usize` directly).  
**Action:** Remove.  
**Effort:** 1 min.

### Y2-9. `relative_rank` / `relative_rank_sq` — dead functions

**File:** `src/types.rs:279-300`  
**Issue:** Two `pub` functions for computing relative rank. `relative_rank_sq` calls `relative_rank`, but neither is called anywhere.  
**Action:** Remove both.  
**Effort:** 2 min.

### Y2-10. `pawn_push` — dead function

**File:** `src/types.rs:302`  
**Issue:** `pub fn pawn_push(c: Color) -> Direction`. Never called.  
**Action:** Remove.  
**Effort:** 1 min.

### Y2-11. `MoveList::clear` — dead method

**File:** `src/types.rs:774`  
**Issue:** `pub fn clear(&mut self)`. Never called (new MoveLists are created per generation).  
**Action:** Remove.  
**Effort:** 1 min.

### Y2-12. `MoveList::retain` — dead method

**File:** `src/types.rs:800`  
**Issue:** `pub fn retain<F>(&mut self, mut f: F)`. Never called. Adds 13 lines + generic complexity.  
**Action:** Remove.  
**Effort:** 1 min.

### Y2-13. `impl Add for Direction` — dead trait impl

**File:** `src/types.rs:685-709`  
**Issue:** `impl ops::Add for Direction` adds two directions and panics on invalid combinations. Never used anywhere.  
**Action:** Remove the entire impl block.  
**Effort:** 2 min.

### Y2-14. `aligned` — dead function (test-only)

**File:** `src/bitboard.rs:85`  
**Issue:** `pub fn aligned(s1, s2, s3) -> bool`. Only used in `#[cfg(test)]`. Make it `#[cfg(test)]` or remove it (move the test inline).  
**Action:** Gate with `#[cfg(test)]`.  
**Effort:** 1 min.

---

**Subtotal Y2: ~14 items, ~25 min**

---

## 2. Performance

### P2-1. `generate_pseudo_legal` — duplicated loop patterns

**File:** `src/movegen.rs:26-79`  
**Issue:** Five near-identical `while !pieces.is_empty()` loops for knights, bishops, rooks, queens, commoners. The only difference is the attack function (`knight_attacks`, `bishop_attacks`, `rook_attacks`, `queen_attacks`, `king_attacks`).  
**Options:**
  - (a) Extract a `generate_sliding_moves(board, pieces, attack_fn, moves)` helper
  - (b) Use a macro to stamp out the loops
  - (c) Keep as-is (inlining-friendly, clear)
**Recommendation:** (c) — the current pattern is maximally inline-friendly, and a helper/macro adds abstraction overhead that the optimizer may not fully eliminate. Close as WONTFIX after audit.

### P2-2. `Direction::from` — Name collision with `From` trait

**File:** `src/types.rs:669`  
**Issue:** `fn from(val: i16) -> Option<Direction>` shadows the `From` trait convention. Every call site uses `Direction::from(val)` rather than `<Direction as TryFrom<i16>>::try_from(val)`. The name `from` suggests infallible conversion.  
**Action:** Rename to `from_i16` for clarity.  
**Effort:** 2 min.

---

## 3. KISS — Unnecessary Complexity

### K2-1. `MoveList::set_len` — misleading safety comment

**File:** `src/movegen.rs:246-250` + `src/types.rs:781-784`  
**Issue:** The call site in `generate_legal` has a verbose SAFETY comment explaining why `set_len` is safe. But `set_len` just sets `self.len = len` — there's no unsafe code. The comment is misleading (implies an unsafe operation).  
**Action:** Replace with plain `moves.set_len(new_len)` call (keep the method, it's used). Trim the comment to a one-liner.  
**Effort:** 2 min.

### K2-2. `generate_pawn_moves_for` — redundant `them` parameter

**File:** `src/movegen.rs:85-91`  
**Issue:** `fn generate_pawn_moves_for(board, us, them, from, moves)` takes `them: Color` as a parameter, but it's only used once (line 139: `board.pieces_color(them)`) and can be trivially derived as `us.flip()`.  
**Action:** Remove the `them` parameter; use `us.flip()` at the single call site.  
**Effort:** 2 min.

### K2-3. `Color::flip()` and `impl Not for Color` — redundant duality

**File:** `src/types.rs:451-468`  
**Issue:** Both `Color::flip()` and the `Not` trait do the same thing (`!c` calls `c.flip()`). Having both is fine for ergonomics (convention in chess engines), but the `Not` impl is used in zero places in the crate. Remove it (callers can use `.flip()`).  
**Action:** Remove `impl Not for Color`.  
**Effort:** 1 min.

### K2-4. `Square::from_u8` — thin delegator

**File:** `src/board.rs:894`  
**Issue:** `pub fn from_u8(idx: u8) -> Square { Square::from_index(idx as i8) }`. Thin wrapper. Used in 9 places (`pext.rs`, `magic.rs`, `board.rs`, examples). Add a `From<u8>` impl instead, or keep as-is.  
**Recommendation:** Keep — it's used enough to justify the convenience. Close as WONTFIX.

---

## 4. Inconsistencies

### I2-1. `Direction::from` vs `From` trait convention

See P2-2 above. Same issue.

### I2-2. `impl Sub<Direction> for Square` — verbose pattern

**File:** `src/types.rs:659-667`  
**Issue:** Uses `match Direction::from(-(rhs as i16)) { Some(d) => self + d, None => Square::NONE }`. After renaming `from` to `from_i16`, this becomes `Direction::from_i16(-rhs).map_or(Square::NONE, |d| self + d)`.  
**Action:** Simplify with `map_or` after renaming P2-2.  
**Effort:** 1 min.

### I2-3. `generate_pawn_moves_for` — `from_file` shadowing

**File:** `src/movegen.rs:93`  
**Issue:** `let from_file = file_of(from) as i8` shadows the `File` type. Not a bug, but slightly confusing for readability.  
**Action:** Rename to `from_f` for clarity.  
**Effort:** 1 min.

---

## 5. Unnecessary / Outdated Comments

### C2-1. `magic.rs:5-7` — outdated policy comment

**File:** `src/magic.rs:5-7`  
**Content:** `//! Pure safe Rust.` and `//! Tables are initialized once at first use via LazyLock...`  
**Issue:** The crate policy is "zero unsafe" (doc in AGENTS.md). The comment states the already-known. The "LazyLock" reference is also outdated — the code uses `OnceLock`, not `LazyLock`.  
**Action:** Remove the "Pure safe Rust" line; fix the second line to say `OnceLock` instead of `LazyLock`.  
**Effort:** 2 min.

### C2-2. `movegen.rs:246-250` — misleading safety comment

See K2-1 above.

### C2-3. `types.rs:711` — decorative separator comment

**File:** `src/types.rs:711-713`  
**Content:** `// ---- MoveList...`  
**Issue:** Decorative section separator. Not harmful but adds noise.  
**Action:** Remove the separator and merge the doc comment directly above `pub const MAX_MOVES`.  
**Effort:** 1 min.

### C2-4. `attacks.rs:3-9` — module-level architecture comment

**File:** `src/attacks.rs:3-9`  
**Content:** Explains the sliding-dispatch architecture.  
**Issue:** The comment is accurate but the `// On ARM etc.: zero overhead` line is slightly misleading — it's not zero overhead, the functions simply aren't compiled on ARM.  
**Action:** No change — the comment is useful for maintainers. Keep as-is.

### C2-5. `board.rs:415-416` — `populate_state` doc comment

**File:** `src/board.rs:415-416`  
**Content:** `/// Fill cached state fields...`  
**Issue:** Accurate and useful. Keep.

---

## Summary

| Category | Items | Est. Effort |
|----------|-------|-------------|
| YAGNI / Dead Code | 14 | ~25 min |
| Performance | 1 (P2-2 rename) | ~2 min |
| KISS / Complexity | 2 (K2-1, K2-2, K2-3) | ~5 min |
| Inconsistencies | 2 (I2-1, I2-3) | ~2 min |
| Comments | 2 (C2-1, C2-3) | ~3 min |
| **Total** | **~18 items** | **~35 min** |

Items audited and NOOP (close as WONTFIX):
- P2-1: `generate_pseudo_legal` loop patterns — kept for inlining
- K2-4: `Square::from_u8` — useful convenience, kept
- C2-4: `attacks.rs` dispatch comment — useful, kept
- C2-5: `populate_state` doc — useful, kept

---

## Execution Order

### Phase 1 — Dead code removal (~25 min)

1. Y2-1: `attacks_bb`
2. Y2-2: `pawn_attacks_bb` + `pawn_attacks_from`
3. Y2-3: 8 shift functions
4. Y2-4: `Bitboard::ALL`
5. Y2-5: `Bitboard::msb`
6. Y2-6: `Piece::is_ok`
7. Y2-7: `SQ_NONE`
8. Y2-8: `Color::to_usize`
9. Y2-9: `relative_rank` + `relative_rank_sq`
10. Y2-10: `pawn_push`
11. Y2-11: `MoveList::clear`
12. Y2-12: `MoveList::retain`
13. Y2-13: `impl Add for Direction`
14. Y2-14: `aligned` → `#[cfg(test)]`

### Phase 2 — Naming & complexity (~10 min)

15. P2-2 + I2-1: Rename `Direction::from` → `from_i16`; simplify `Sub` with `map_or`
16. K2-2: Remove `them` param from `generate_pawn_moves_for`
17. K2-3: Remove `impl Not for Color`
18. I2-3: Rename `from_file` → `from_f`

### Phase 3 — Comments (~5 min)

19. K2-1: Trim `set_len` safety comment
20. C2-1: Fix `magic.rs` outdated module doc
21. C2-3: Remove separator comment

---

## Verification

```sh
cargo build && cargo test && cargo clippy && cargo fmt --check
```

For perft correctness:
```sh
cargo run --example verify_perft 4
```
