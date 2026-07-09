# Code review of `atomic-movegen`

## Test results

- `cargo build` OK
- `cargo clippy` OK (default warnings)
- `cargo test` OK (39 unit tests + 4 perft tests + 1 `verify_moves` test + 1 doctest)
- `cargo run --release --example verify_perft 5` OK — all 41 perft positions pass
- `tests/verify_moves.rs` passes against `tests/moves.md` (1000 positions)

Core move generation and blast logic are verified against the Fairy-Stockfish oracle for the tested positions.

---

## 1. Safety / correctness — critical

### `Piece::type_of` is UB on `NO_PIECE`

`src/types.rs` lines 497–509:

```rust
pub fn type_of(self) -> PieceType {
    let inner = (self.0 & 7) - 1;
    debug_assert!(
        inner < 6,
        "Piece::type_of called with invalid Piece encoding: inner={}",
        inner
    );
    unsafe { std::mem::transmute(inner) }
}
```

For `NO_PIECE` (encoding `0`) `inner` underflows to `255` in release (because `Cargo.toml` sets `overflow-checks = false`), and `transmute(255)` to a 6-variant enum is undefined behavior. `NO_PIECE` is a public constant, so a user can trigger UB with `NO_PIECE.type_of()` or `NO_PIECE.ascii_char()`.

**Recommendation:** return `Option<PieceType>` (or at least panic deterministically with `assert!`) and update call sites.

### `Board::piece_on` / `Board::empty` are UB on `Square::NONE`

`src/board.rs` lines 325–336:

```rust
pub fn piece_on(&self, sq: Square) -> Piece {
    self.squares[sq as usize]
}

pub fn empty(&self, sq: Square) -> bool {
    self.squares[sq as usize] == NO_PIECE
}
```

`Square::NONE` has discriminant `64`. `squares` is `[Piece; 64]`, so `squares[64]` is out of bounds. `Square::NONE` is a public sentinel used by `Square::from_index` for invalid indices.

**Recommendation:** guard `NONE` and return `NO_PIECE` / `true`.

### `parse_sq` silently returns `A1` for invalid input

`src/types.rs` (`parse_sq`) and `src/board.rs` lines 219–227:

`from_fen` works around this by checking `sq == A1 && parts[3] != "a1"`, but `parse_sq` is public and callers like `examples/fen_after.rs` silently get a wrong square for malformed input.

**Recommendation:** make `parse_sq` return `Option<Square>` or `Result<Square, _>`.

### `sq_str` is wrong for `Square::NONE`

For `NONE` (index `64`), `idx / 8 + 1` gives `9`, so it returns `"a9"` instead of indicating an invalid square.

### `Piece::color` returns `Color::White` for `NO_PIECE`

This is misleading and can corrupt `by_color` if `move_piece`/`remove_piece`/`place_piece` are called with a `NO_PIECE` value (the `debug_assert!` catches it in debug, not release).

### `Move` constructors silently truncate `Square::NONE`

`Move::make_move(NONE, NONE)` shifts the `64` discriminant into 6-bit `from`/`to` fields and truncates to a different square. The constructors should `assert`/`debug_assert` that `from` and `to` are not `NONE`.

### `unsafe` violates `AGENTS.md`

`AGENTS.md` states: **Zero `unsafe` — keep the crate entirely safe Rust.** The crate uses `unsafe` in:

- `Bitboard::lsb` / `Move::from_sq` / `Move::to_sq` — safe for valid inputs but still `unsafe`
- `Piece::type_of` — UB on `NO_PIECE`
- `src/pext.rs` lines 157–165 — inline assembly for `pext`

**Recommendation:** remove the `unsafe` blocks or update `AGENTS.md`.

---

## 2. Performance

### `do_move` calls `populate_state` redundantly

`src/board.rs` lines 656–658:

```rust
self.side_to_move = them;
self.game_ply += 1;

self.populate_state(state);
}
```

`do_move` ends with `populate_state`. Then `perft`/`generate_legal` immediately creates a fresh `StateInfo` and calls `populate_state` again. This means `populate_state` (which computes checkers, pinned pieces, and commoner counts) runs twice per node. Additionally, the castling branch of `do_move` does **not** call `populate_state`, so behavior is inconsistent.

**Recommendation:** remove `populate_state` from `do_move` (or move it consistently after every branch). `undo_move` only needs the undo fields.

### PEXT is forced on every BMI2 CPU, which hurts on AMD Zen

`src/attacks.rs` lines 292–302:

```rust
if crate::pext::has_bmi2() {
    crate::pext::init();
    sliding_dispatch::force_pext();
} else {
    sliding_dispatch::force_magic();
}
```

`has_bmi2()` is the only gate. AMD Zen implements `pext` in microcode and it is much slower than magic; `docs/perf/5950X` shows depth-6 perft at ~80s while `docs/perf/m4` (which uses magic) reports ~53s. The inline assembly also makes the PEXT path non-portable and `unsafe`.

**Recommendation:** make PEXT opt-in or disable it by default on Zen-class CPUs. Also consider using `core::arch::x86_64::_pext_u64` (still `unsafe`) if `unsafe` is allowed.

### Both magic and PEXT tables are built on x86-64

`attacks::init()` always calls `magic::init()`, then `pext::init()` if BMI2 is present. If PEXT is used, the magic tables are never used, so the build time and memory are wasted. `pext` only needs `magic::ROOK_MASKS`/`BISHOP_MASKS` and `sliding_attack`, not the magic tables.

**Recommendation:** reorder `init()` to check BMI2 first and build only the needed tables.

### `magic.rs` and `pext.rs` recompute offsets/totals

- `src/magic.rs` lines 313–316 compute `ROOK_OFFSETS`/`BISHOP_OFFSETS`, then `compute_rook_entries` (lines 335–379) recomputes the same totals as `MagicEntry.offset`.
- `src/pext.rs` `compute_pext_layout` repeats the same total/offset logic.

**Recommendation:** reuse `ROOK_OFFSETS`/`BISHOP_OFFSETS` in `compute_rook_entries`/`compute_bishop_entries` and share a helper for total table size.

---

## 3. FEN parsing and board-state correctness

### `rule50` is `u8`

`src/board.rs` lines 229–235:

```rust
let rule50 = if parts.len() > 4 {
    parts[4]
        .parse::<u8>()
        .map_err(|e| FenError::ParseInt(e.to_string()))?
} else {
    0
};
```

`do_move` increments `rule50` with `+= 1`. This overflows after 255 moves and the `overflow-checks = false` release profile makes it wrap silently. `tests/verify_moves.rs` has a `fix_halfmove_clock` hack that clamps `>255` to `0` to work around this.

**Recommendation:** change `rule50` to `u16` and parse `u16` in `from_fen`.

### `from_fen` does not validate the placement field

`src/board.rs` lines 177–195:

It checks `rows.len() == 8` but does not validate that each row sums to 8, that digits do not push `col` past 7, or that pieces are placed on valid squares. An extra `col` silently causes `sq_idx >= 64` and the piece is skipped, but `col` keeps advancing, so subsequent pieces land in wrong squares.

**Recommendation:** add `col` bounds checking and row-sum validation.

### `from_fen` accepts 5-field FEN and unvalidated castling/EP rights

Standard FEN is 6 fields. A 5-field FEN is silently accepted. Castling rights are not checked against the actual king/rook squares, and EP is not checked against an actual pawn that just moved.

### `update_castling_rights` over-clears and under-clears

`src/board.rs` lines 737–751:

```rust
fn update_castling_rights(&mut self, from: Square, to: Square, _us: Color) {
    if from == Square::E1 || from == Square::H1 || to == Square::H1 {
        self.castling_rights &= !WK_CASTLE;
    }
    if from == Square::E1 || from == Square::A1 || to == Square::A1 {
        self.castling_rights &= !WQ_CASTLE;
    }
    ...
}
```

It checks `to == H1`/`A1`/`H8`/`A8` unconditionally, so a non-capture move landing on an empty `A1`/`H1` (in an inconsistent FEN) would clear castling rights incorrectly. Conversely, it does not clear rights when the commoner on `E1`/`E8` is captured (`to == E1`/`E8`), which can leave stale castling rights after a multi-commoner blast.

**Recommendation:** only consider `to` for captures, and add `to == E1`/`E8` checks.

### `generate_castling` does not verify the king/commoner is present

`src/movegen.rs` lines 170–227:

It only checks `castling_rights()` and `board.empty()` for the intermediate squares. It does not check `board.piece_on(king_sq) == make_piece(us, Commoner)`. If castling rights are stale, it generates a castling move whose `from` square is empty, which `Board::legal()` later rejects, but `generate_pseudo_legal` is public and returns an impossible move.

### `generate_pawn_moves_for` does not validate the EP victim

`src/movegen.rs` lines 156–167:

It generates an en-passant move if the file is adjacent, but does not check that the captured pawn square actually contains an enemy pawn. With a malformed `ep_square` this can produce an illegal move.

### `Board::pinned` does not filter by color

`compute_pinned` returns every piece between a commoner and an enemy slider, regardless of color. `is_move_trivially_legal` only uses it with `from` (our own piece), so the effect is benign there, but `Board::pinned(c)` is a public method and returns incorrect color results.

---

## 4. Consistency / DRY / YAGNI

### `bitboard.rs` hides dead code with `#![allow(dead_code)]`

`src/bitboard.rs` lines 1–49:

`FILE_*BB`, `RANK_*BB`, `ALL_SQUARES`, `line_bb`, `aligned`, and `more_than_one` are not used in production. `line_bb` is only used in tests; `BETWEEN_BB` is used, but `LINE_BB` is not.

**Recommendation:** remove or gate `#[cfg(test)]` the unused items and remove `#![allow(dead_code)]`.

### `LINE_BB` is 32 KB of dead data

`src/attacks.rs` lines 263–268:

Only `BETWEEN_BB` is used (in `compute_pinned`). `LINE_BB` and `compute_line_bb` should be gated to `#[cfg(test)]` or removed.

### `Direction` enum and `Square` `Add`/`Sub` impls are unused

`src/types.rs` lines 655–700:

The code uses raw `i8` arithmetic and `Square::from_index` everywhere. `Direction` is dead public API.

### `Move::NULL` and `Bitboard::more_than_one` are unused

`Move::NULL` is defined but never used; `more_than_one` is defined but never used.

### `Square` helpers live in `board.rs` instead of `types.rs`

`Square::from_index` and `from_u8` are implemented in `src/board.rs` lines 990–1002, which is odd for a type defined in `types.rs`. `PROMOTION_PIECES` is in `src/movegen.rs` while `Move::make_promotion` is in `src/types.rs`.

### `movegen.rs` repetitive slider/leaper loops

The loops for knights, bishops, rooks, queens, and commoners are nearly identical and could be collapsed into a small helper or macro.

### `Cargo.toml` vs `AGENTS.md` conventions

`AGENTS.md` says to use `thiserror` and `strum`, but `Cargo.toml` has zero dependencies and `FenError` is hand-written. `AGENTS.md` also says zero `unsafe`, but the crate uses `unsafe`. One of the two should be updated.

---

## 5. Documentation / rules

### `lib.rs` docstring does not match the code

The doc says “Adjacent COMMONERs (even own) are illegal (extinction pseudo-royal rule)”. The implementation only checks whether the **last** commoner is adjacent to an **enemy** commoner, which matches Fairy-Stockfish's `atomic` variant (`extinctionPseudoRoyal` with `extinctionPieceCount = 0`). It does not implement `atomar`'s `mutuallyImmuneTypes`/`blastImmuneTypes` for commoners, nor does it forbid own adjacent commoners.

**Recommendation:** align the README/lib docs with the implemented ruleset.

---

## Priority order

1. **Fix UB / OOB:** `Piece::type_of`, `piece_on`/`empty` on `Square::NONE`, `parse_sq`/`sq_str` behavior.
2. **Remove `unsafe` or update `AGENTS.md`:** the `unsafe` blocks are directly contrary to the project rule.
3. **Performance:** remove `populate_state` from `do_move`, and make PEXT optional / not the default on all BMI2 CPUs.
4. **FEN robustness:** `u16` `rule50`, placement validation, stale castling rights.
5. **YAGNI/DRY:** gate/remove `LINE_BB`, `Direction`, file/rank constants, dead helpers, and duplicate offset computations.
