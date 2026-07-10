# Plan 3 — EVASIONS / NON_EVASIONS Split (Second Attempt)

**Corresponds to:** Item 3 of `docs/plans/perf_analysis6/analysis.md` — *EVASIONS / NON_EVASIONS split (second attempt with a different layout)*  
**Estimated speedup:** 4–10 % on `cargo run --release --example verify_perft` (up to 20 % on the check-heavy tests)  
**Risk:** High  
**Effort:** ~120–180 lines changed in `src/movegen.rs` (plus possible `src/board.rs` helpers)  
**Fairy-Stockfish reference:** `movegen.cpp:510–526` `pos.checkers() ? generate<EVASIONS>(...) : generate<NON_EVASIONS>(...)`

---

## 1. Background

- **Plan 1** (Item 1) removed the `OnceLock` from magic tables and produced a ~4.58 % `verify_perft` speedup.
- **Plan 2** (Item 2) cached `commoners` bitboards and added a capture blast pre-filter, but regressed ~5.17 % and was reverted. The codebase is back to the Plan 1 state.
- A previous attempt at the same EVASIONS split (`docs/plans/perf_analysis5/report1.md`) also regressed ~5–6 %. The root cause was that the new branch and the new `generate_evasions` code changed the inlining / code layout of the hot non-check path inside `generate_legal`, even with `#[inline(never)]` on the evasions function and an early-return structure.
- This plan follows the different layout from `analysis.md`: keep the non-check source path untouched, mark `generate_legal` `#[inline(never)]` so `perft` does not inline the whole generator, and move the in-check logic into a separate `#[inline(never)]` / `#[cold]` `generate_legal_in_check`.

---

## 2. Problem

`generate_legal` in `src/movegen.rs` currently:

1. Always calls `generate_pseudo_legal`, which builds *all* pseudo-legal moves regardless of check state.
2. Filters every move through `is_move_trivially_legal` and `Board::legal`.
3. When `state.checkers` is non-empty, `is_move_trivially_legal` returns `false` immediately, so every pseudo-legal move pays the full `Board::legal` cost.
4. In double-check positions, no non-commoner move can ever be legal (except a single capture that blasts all checkers), yet every non-commoner pseudo-legal move is still generated and filtered.

The slowest `verify_perft` tests (#2, #13, #33) are check-heavy, so avoiding this waste is the most direct way to reduce total time.

---

## 3. Goal

1. Add a dedicated, cold `generate_legal_in_check` path that generates only moves that can resolve a check.
2. Keep the non-check `generate_legal` path source-identical to the current `main`.
3. Use `#[inline(never)]` and `#[cold]` to isolate the cold check path from the hot non-check path.
4. Preserve all atomic-chess semantics: pseudo-royal commoners, blast destruction, en-passant, castling, commoner adjacency immunity.
5. Measure against the full `verify_perft` suite and the profiled FEN; accept only if it is a clear improvement.

---

## 4. Design

### 4.1 `generate_legal` as a thin branch

Current `generate_legal` is:

```text
populate_state -> generate_pseudo_legal -> in-place compaction (is_move_trivially_legal || legal)
```

The new `generate_legal` will be:

```rust
#[inline(never)]
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);

    if state.checkers.is_empty() {
        generate_pseudo_legal(board, moves);
    } else {
        generate_legal_in_check(board, &state, moves);
    }

    // In-place compaction: identical to today.
    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            if is_move_trivially_legal(board, m, &state) || board.legal(m, &state) {
                ms[write_idx] = m;
                write_idx += 1;
            }
        }
        write_idx
    };
    moves.set_len(new_len);
}
```

Key points:

- The **non-check branch** is the exact same code as today.
- The **compaction loop** is unchanged; for in-check moves `is_move_trivially_legal` returns `false` immediately and falls through to `Board::legal`.
- `generate_legal` is `#[inline(never)]` so `perft` does not inline the whole generator and the hot non-check path remains a compact function body.
- If `#[inline(never)]` on `generate_legal` regresses because of call overhead, the fallback is to remove the attribute and only mark `generate_legal_in_check` `#[inline(never)]` / `#[cold]`.

### 4.2 `generate_legal_in_check` (evasions path)

`generate_legal_in_check` is a new `#[inline(never)]` function (and `#[cold]` if the compiler accepts it) in `src/movegen.rs`. It generates only pseudo-legal moves that can resolve a check. The generated moves are still filtered by `Board::legal` in `generate_legal`.

```rust
fn generate_legal_in_check(board: &Board, state: &StateInfo, moves: &mut MoveList) {
    let us = board.side_to_move();
    let them = us.flip();
    let occupied = board.occupied();
    let our_pieces = board.pieces_color(us);
    let enemy_pieces = board.pieces_color(them);
    let our_commoners = board.commoners(us);

    // 1. Commoner moves are always possible (escapes, captures, blasts).
    let commoner_target = !our_pieces;
    generate_piece_moves(
        board,
        us,
        commoner_target,
        PieceType::Commoner,
        attacks::king_attacks,
        moves,
    );

    // 2. Commoner-adjacency check: if any checker is an enemy commoner,
    //    legal() treats adjacency as safety, so non-commoner moves are not
    //    restricted to the evasion target. Fall back to full pseudo-legal.
    if any_checker_is_commoner(board, state.checkers) {
        generate_pseudo_legal(board, moves);
        return;
    }

    // 3. Single vs double check.
    let mut checkers = state.checkers;
    let first_checker = checkers.pop_lsb();
    let double_check = !checkers.is_empty();

    // 4. Build the restricted target for non-commoner moves.
    //    resolve_set(checker) = { checker_sq }                          // direct capture
    //      ∪ (if non-pawn) king_attacks(checker_sq) & enemy_pieces      // blast-destruction
    //      ∪ (if slider) between_bb(ksq, checker_sq) for each ksq in our_commoners
    let target = if double_check {
        intersection_of_resolve_sets(board, state, our_commoners, enemy_pieces)
    } else {
        resolve_set(board, first_checker, our_commoners, enemy_pieces)
    };

    if target.is_empty() {
        return; // only commoner moves can save the position
    }

    // 5. Generate non-commoner moves restricted to the target.
    generate_pawn_evasion_moves(board, us, them, target, moves);

    for pt in [
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
    ] {
        let attacks = |sq: Square| match pt {
            PieceType::Knight => attacks::knight_attacks(sq),
            PieceType::Bishop => attacks::bishop_attacks(sq, occupied),
            PieceType::Rook => attacks::rook_attacks(sq, occupied),
            PieceType::Queen => attacks::queen_attacks(sq, occupied),
            _ => unreachable!(),
        };
        generate_piece_moves(board, us, target, pt, attacks, moves);
    }

    // No castling when in check.
}
```

Helper functions (all private to `src/movegen.rs`):

- `resolve_set(board, checker_sq, our_commoners, enemy_pieces) -> Bitboard`
  - `checker_sq` always.
  - `attacks::king_attacks(checker_sq) & enemy_pieces` if the checker is **not** a pawn (pawns are not destroyed by the blast).
  - `between_bb(ksq, checker_sq)` for every `ksq` in `our_commoners` if the checker is a slider (Rook/Bishop/Queen).
- `intersection_of_resolve_sets(board, state, our_commoners, enemy_pieces) -> Bitboard`
  - Start with `Bitboard::EMPTY` or `!Bitboard::EMPTY`? Use `resolve_set` for the first checker, then intersect with `resolve_set` for each remaining checker.
  - If any checker is a pawn, its resolve set is just `checker_sq` (plus the EP square, handled separately by `generate_pawn_evasion_moves`).
- `any_checker_is_commoner(board, checkers) -> bool`
  - Iterate over `checkers` and check `board.piece_on(sq).type_of() == PieceType::Commoner`.

### 4.3 `generate_pawn_evasion_moves`

A new helper in `src/movegen.rs` similar to `generate_pawn_moves_for`, but:

- Single and double pushes are only generated if the destination is in `target` (i.e., the push blocks the check).
- Normal captures are only generated if the destination is in `target` (i.e., captures the checker or a piece adjacent to a non-pawn checker).
- Promotion handling is the same as `generate_pawn_moves_for`.
- En-passant is always generated when available (at most 2 moves). `Board::legal` filters illegal EP captures and EP blast interactions.

### 4.4 Commoner-adjacency fallback

`compute_checkers` adds an enemy commoner to `checkers` when it is king-adjacent to one of our commoners. `Board::legal` treats such adjacent commoners as immune and does not reject non-commoner moves that leave the adjacency unresolved. Therefore, if any checker is a commoner, the restricted target approach would miss legal moves. In that case, fall back to `generate_pseudo_legal` and let `Board::legal` filter. This is the same safe behavior as today for those positions.

### 4.5 Castling

Castling is never legal when in check because the commoner would pass through or end on an attacked square. `generate_legal_in_check` does not generate castling. The non-check path continues to call `generate_castling` as before.

### 4.6 Inlining and cold-path hints

- `generate_legal`: `#[inline(never)]` (experiment; remove if call overhead dominates).
- `generate_legal_in_check`: `#[inline(never)]` and `#[cold]` if the compiler accepts it.
- `generate_pawn_evasion_moves`: `#[inline(always)]` to match the existing `generate_pawn_moves_for`.
- `generate_piece_moves`: unchanged.
- `resolve_set` / `intersection_of_resolve_sets` / `any_checker_is_commoner`: `#[inline(always)]` or `#[inline]`.

---

## 5. Implementation Steps

### Step 1: Record baseline

Run the current `main` and record the exact `verify_perft` time:

```sh
cargo build
cargo test
cargo clippy
cargo fmt
cargo doc
cargo run --release --example verify_perft
```

Record:

- Total time and `41/41` pass/fail result.
- Per-test times, especially the slowest ones (#2, #13, #33).
- Optionally the profiled FEN:
  ```sh
  cargo run --release --example perft \
    "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15" 6
  ```

### Step 2: Implement helper functions

In `src/movegen.rs`:

- `resolve_set`
- `intersection_of_resolve_sets`
- `any_checker_is_commoner`
- `generate_pawn_evasion_moves`

Keep them private and add doc comments to satisfy `#![warn(missing_docs)]`.

### Step 3: Implement `generate_legal_in_check`

Add `generate_legal_in_check` in `src/movegen.rs`:

- Generate commoner moves first.
- Handle the commoner-adjacency fallback.
- Build the `target` bitboard for single or double check.
- Generate pawn and piece evasion moves restricted to `target`.

### Step 4: Refactor `generate_legal`

- Add `#[inline(never)]` to `generate_legal`.
- Add the `if state.checkers.is_empty() { generate_pseudo_legal } else { generate_legal_in_check }` branch before the compaction loop.
- Keep the compaction loop exactly as it is.

### Step 5: Build and correctness test

```sh
cargo build
cargo test
cargo clippy
cargo fmt
cargo doc
cargo run --release --example verify_perft
```

All 41 positions must pass. Fix any failures or warnings.

### Step 6: Performance measurement

- Run `cargo run --release --example verify_perft` at least three times and average.
- Compare total time and per-test times to the baseline.
- Run the profiled FEN.
- If `#[inline(never)]` on `generate_legal` regresses, try the alternative: remove `#[inline(never)]` from `generate_legal` and only mark `generate_legal_in_check` `#[inline(never)]` / `#[cold]`.
- If all variants regress, revert the changes and report.

### Step 7: Write `docs/plans/perf_analysis6/report3.md`

Document the implementation and create a hand-off report for Plan 4. Required sections:

- **Summary** — what was changed and the measured effect.
- **Baseline** — exact `verify_perft` time before the change.
- **Result** — exact time after and speedup/deg percentage.
- **Implementation notes** — why the new layout was chosen, how `resolve_set` works, the commoner-adjacency fallback, double-check intersection, and EP handling.
- **Problems, surprises, and workarounds** — e.g. `#[inline(never)]` effects, `#[cold]` support, over-generated targets, `Bitboard` intersections, `commoners` fallback frequency, or any correctness test failures.
- **Files changed** — list of files and the nature of the change.
- **Verification results** — `cargo test`, `cargo clippy`, `cargo fmt`, `cargo doc`, and `cargo run --release --example verify_perft` outcomes.
- **Notes for Plan 4** — state of `generate_legal`/`generate_legal_in_check`, whether `is_square_attacked` bool (Item 4) or bulk pawn generation (Item 5) would be the next logical step.

---

## 6. Files Changed

| File | Change | Approx. Lines |
|---|---|---|
| `src/movegen.rs` | Add `generate_legal_in_check`, `generate_pawn_evasion_moves`, `resolve_set`, `intersection_of_resolve_sets`, `any_checker_is_commoner`; refactor `generate_legal` branch. | ~120–180 |
| `src/board.rs` | Possibly no changes; if helpers are placed in `board.rs`, add `pub(crate)` helpers. | ~0–20 |

---

## 7. Correctness Verification

- `cargo test` — all unit tests pass.
- `cargo run --release --example verify_perft` — 41/41 positions pass at depths 1–6.
- `cargo clippy` — clean.
- `cargo fmt` — clean.
- `cargo doc` — clean.

---

## 8. Expected Impact

- **Non-check positions:** no change; the non-check branch in `generate_legal` is the same source.
- **Single-check positions:** generate far fewer pseudo-legal moves, reducing `Board::legal` calls.
- **Double-check positions:** only commoner moves (plus rare capture-blast captures) are generated.
- **Check-heavy tests (#2, #13, #33):** should improve the most.
- **Estimated `verify_perft` speedup:** 4–10 %.

---

## 9. Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| `#[inline(never)]` on `generate_legal` adds call overhead that dominates the savings | Medium | Measure and try alternative (inline `generate_legal`, only `#[inline(never)]` the check path). |
| Commoner-adjacency fallback removes the benefit for positions where a commoner is the only checker | Medium | Accept; correctness comes first. Future optimization can split commoner adjacency from real check. |
| Double-check capture-blast intersection missed or over-generated | Low | Start with full `resolve_set` intersection; rely on `Board::legal` to filter over-generated moves. |
| Target computation misses a legal evasion | Low | `verify_perft` catches all; if a test fails, add the missing target. |
| Layout regression on the non-check path despite isolation | Medium | If `verify_perft` regresses, revert and document. |

---

## 10. Notes for Plan 4

- If this plan succeeds, `Board::legal` is still called for every generated evasion. The next logical item from `analysis.md` is **Item 4** (`is_square_attacked` boolean early-exit in `Board::legal`), which can reduce the cost of each `legal` call.
- If `generate_legal_in_check` shows that `commoners` bitboards are fetched repeatedly, a future attempt at **Item 2** could reintroduce the `our_commoners` / `them_commoners` cache in `StateInfo` but only inside `generate_legal_in_check` / `Board::legal` to avoid the Plan 2 regression.
- **Item 5** (bulk bitboard pawn generation) and **Item 7** (`compute_pinned` occupancy-delta) can be layered on top later.
