# Plan 1 — EVASIONS / NON_EVASIONS Move-Generation Split

**Corresponds to:** Item 1 of `analysis.md` — *"Add EVASIONS/NON_EVASIONS move generation split"*
**Estimated speedup:** 8–20 %
**Effort:** ~120 lines changed across `src/movegen.rs`, `src/board.rs` (moderate)
**Fairy-Stockfish reference:** `movegen.cpp:376–474` (`generate_all<EVASIONS>`) and `movegen.cpp:509–526` (`generate<LEGAL>`)

---

## 1. Problem

`generate_legal()` (`src/movegen.rs:233–257`) currently calls `generate_pseudo_legal()` unconditionally, which generates **all** pseudo-legal moves (30–80+ in typical positions) regardless of whether the side to move is in check. When in check:

1. `is_move_trivially_legal()` immediately returns `false` (because `state.checkers` is non-empty at `board.rs:901`).
2. Every pseudo-legal move falls through to the full `legal()` check — blast simulation, commoner adjacency, `attackers_to` — even for non-commoner moves that can't possibly stop the check.
3. In double-check positions (≥2 checkers), **no** non-commoner move can ever be legal, yet all are still generated and filtered.

perf shows this waste is significant: tactical positions with checks (Tests #2, #13, #33 — the 3 slowest) spend the majority of time filtering irrelevant non-commoner moves.

---

## 2. Design

### 2.1 New entry-point: `generate_legal()`

Refactor `generate_legal()` to branch on check state **before** generating any moves:

```
generate_legal(board, moves):
    state = StateInfo
    board.populate_state(&state)

    if state.checkers.is_empty():
        generate_non_evasions(board, moves)         // same as current generate_pseudo_legal
    else:
        generate_evasions(board, &state, moves)     // restricted target

    // Compaction — still needed because evasions-generated moves can be
    // illegal for non-check reasons (self-explosion, adjacency, pin).
    for each move m:
        if board.legal(m, &state):
            keep(m)
```

**Key differences from current:**
- The non-evasions path keeps `is_move_trivially_legal()` as a fast-path filter (same as today).
- The evasions path skips `is_move_trivially_legal()` (it always returns `false` when `checkers != EMPTY`) and calls `legal()` directly.

### 2.2 Non-evasions path: `generate_non_evasions()`

Identical to the current `generate_pseudo_legal()` function body (`movegen.rs:20–89`). All piece types generated normally, castling included. The compaction pass applies `is_move_trivially_legal()` + `legal()` as before.

### 2.3 Evasions path: `generate_evasions()`

New function that generates a restricted set of pseudo-legal moves based on the check configuration.

#### 2.3.1 Double-check shortcut

When `state.checkers.count() > 1`:
- Only generate **commoner moves** (unrestricted). Non-commoner moves cannot possibly resolve a double check.
- Skip all pawn/leaper/slider/castling generation entirely.
- The `legal()` filter still applies (self-explosion, adjacency, pin).

This mirrors FS `movegen.cpp:385`: `if (Type != EVASIONS || !more_than_one(pos.checkers()))`.

#### 2.3.2 Single-check logic

When exactly one checker:

1. **Identify the checker** — `checker_sq = state.checkers.lsb()`, `checker_type = board.piece_on(checker_sq).type_of()`.
2. **Determine whether the checker is a slider** (Rook, Bishop, Queen) or a leaper (Knight, Pawn).
3. **Compute the restricted `target`** for non-commoner moves:
   - **Slider check:** `target = between_bb(attacked_commoner_sq, checker_sq) | state.checkers`
     - The `between_bb` covers blocking squares; `state.checkers` covers captures of the checker.
     - If multiple commoners are attacked by the same slider (same ray), take the union of `between_bb` for all attacked commoners. In practice this is rare; the first attacked commoner suffices for correctness (extra squares just generate more moves that `legal()` will filter).
   - **Leaper check (Knight/Pawn):** `target = state.checkers` — can't block a leaper, must capture the checker.
4. **Generate non-commoner moves restricted to `target`:**
   - Pawn: only pushes/captures landing on `target` squares (including promotions).
   - Knight, Bishop, Rook, Queen: only attacks intersecting `target`.
   - Skip castling entirely (never legal when in check).
5. **Generate commoner moves** unrestricted (all king-attack squares, excluding own pieces). The legality filter will catch any that don't resolve the check.

#### 2.3.3 FS equivalence

| FS concept | atomic_movegen equivalent |
|---|---|
| `pos.checkers()` | `state.checkers` |
| `ksq` (king square) | Each commoner in `commoners(us)` |
| `between_bb(ksq, lsb(pos.checkers()))` | `between_bb(commoner_sq, checker_sq)` |
| `more_than_one(pos.checkers())` | `state.checkers.more_than_one()` |
| Leaper vs slider check | `checker_type` dispatch |
| King moves unrestricted | Commoner moves unrestricted |

### 2.4 Evasions-specific helpers

**`generate_evasion_pawn_moves()`** — Like `generate_pawn_moves_for` but accepts a `target: Bitboard` parameter. For each pawn, only generate pushes/captures whose destination is in `target`. Promotions are included if the dest is a promotion rank and in `target`. En-passant is included if the dest is in `target`.

**`generate_evasion_piece_moves()`** — Generic helper taking `PieceType`, generates moves for all pieces of that type, filtering attacks by `target`. Replaces the per-type loops in `generate_pseudo_legal` when in the evasion path.

**`generate_evasion_commoner_moves()`** — Generates commoner moves to all non-own squares (same as the commoner loop in `generate_pseudo_legal` but without the target restriction). Called in both single and double check.

---

## 3. Implementation Steps

### Step 1: Rename existing `generate_pseudo_legal` to `generate_non_evasions`

- `movegen.rs:20` — rename function, keep body unchanged.
- Update the doc comment: "Generate all pseudo-legal non-evasive moves..."

### Step 2: Add `generate_evasions()` function (~80 lines)

Pseudo-code:

```rust
fn generate_evasions(board: &Board, state: &StateInfo, moves: &mut MoveList) {
    let us = board.side_to_move();
    let them = us.flip();
    let occupied = board.occupied();

    // Double check — only commoner moves
    if state.checkers.more_than_one() {
        generate_commoner_moves(board, us, them, moves);
        return;
    }

    // Single check
    let checker_sq = state.checkers.lsb();
    let checker_type = board.piece_on(checker_sq).type_of();
    let is_slider = matches!(checker_type,
        PieceType::Rook | PieceType::Bishop | PieceType::Queen);

    // Build target for non-commoner moves
    let mut target = state.checkers;  // captures always allowed
    if is_slider {
        // Find attacked commoner(s) and add between_bb
        let mut commoners = board.commoners(us);
        while !commoners.is_empty() {
            let ksq = commoners.pop_lsb();
            let slider_atk = attacks::rook_attacks(checker_sq, occupied)
                           | attacks::bishop_attacks(checker_sq, occupied);
            if slider_atk & Bitboard::square_bb(ksq) != Bitboard::EMPTY {
                target = target | between_bb(ksq, checker_sq);
            }
        }
    }
    // else: leaper — target = checkers (already set above)

    // Non-commoner evasions restricted to target
    generate_pawn_moves_to_target(board, us, them, target, moves);
    generate_piece_moves_to_target(board, us, them, PieceType::Knight, target, moves);
    generate_piece_moves_to_target(board, us, them, PieceType::Bishop, target, moves);
    generate_piece_moves_to_target(board, us, them, PieceType::Rook, target, moves);
    generate_piece_moves_to_target(board, us, them, PieceType::Queen, target, moves);

    // Commoner moves unrestricted
    generate_commoner_moves(board, us, them, moves);
}
```

### Step 3: Refactor `generate_legal()` (~25 lines)

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);

    if state.checkers.is_empty() {
        generate_non_evasions(board, moves);
    } else {
        generate_evasions(board, &state, moves);
    }

    // Compaction pass
    let orig_len = moves.len();
    if orig_len == 0 { return; }

    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            if state.checkers.is_empty() {
                // Non-evasions: use fast-path + legal
                if is_move_trivially_legal(board, m, &state) || board.legal(m, &state) {
                    ms[write_idx] = m;
                    write_idx += 1;
                }
            } else {
                // Evasions: skip trivially-legal (always false), go straight to legal
                if board.legal(m, &state) {
                    ms[write_idx] = m;
                    write_idx += 1;
                }
            }
        }
        write_idx
    };
    moves.set_len(new_len);
}
```

### Step 4: Extract `generate_commoner_moves()` from `generate_non_evasions` (~12 lines)

Pull the commoner loop body into a shared helper called by both `generate_non_evasions` and `generate_evasions`:

```rust
fn generate_commoner_moves(board: &Board, us: Color, them: Color, moves: &mut MoveList) {
    let target = !board.pieces_color(us);
    let mut commoners = board.pieces_color_pt(us, PieceType::Commoner);
    while !commoners.is_empty() {
        let from = commoners.pop_lsb();
        let attacks = attacks::king_attacks(from) & target;
        let mut a = attacks;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }
}
```

### Step 5: Add `generate_pawn_moves_to_target()` (~50 lines)

A modified version of `generate_pawn_moves_for` that takes a `target: Bitboard` and only generates moves landing on `target`. For the evasions path, the existing per-pawn loop is fine — we just restrict destination filtering. The simplest approach is to add a `target` parameter to `generate_pawn_moves_for` (defaulting to `!occupied | board.pieces_color(them)` for the non-evasions path), or create a separate helper.

Given the existing per-pawn iteration in `generate_pawn_moves_for`, the simplest change is to add a `target` parameter:

```rust
fn generate_pawn_moves_for(board: &Board, us: Color, them: Color,
    from: Square, moves: &mut MoveList, target: Bitboard)
```

In the single-push path: `if to_sq != Square::NONE && board.empty(to_sq) && (Bitboard::square_bb(to_sq) & target) != EMPTY`.
Same for captures, ep, promotions.

### Step 6: Add `generate_piece_moves_to_target()` (~15 lines)

A generic helper that iterates pieces of a given type, computes attacks, and filters by target:

```rust
fn generate_piece_moves_to_target(board: &Board, us: Color, them: Color,
    pt: PieceType, target: Bitboard, moves: &mut MoveList)
{
    let occupied = board.occupied();
    let mut pieces = board.pieces_color_pt(us, pt);
    while !pieces.is_empty() {
        let from = pieces.pop_lsb();
        let attacks = attacks_for(pt, from, occupied) & target;
        let mut a = attacks;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }
}
```

Where `attacks_for()` dispatches to `knight_attacks`, `bishop_attacks`, `rook_attacks`, `queen_attacks` based on `pt`.

### Step 7: Update `generate_non_evasions` to call helpers

Replace the inlined loops with calls to `generate_commoner_moves`, `generate_piece_moves_to_target`, and `generate_pawn_moves_for(..., full_target)`.

---

## 4. Correctness Verification

### 4.1 Unit tests in `movegen.rs`

Add `#[cfg(test)]` tests for the evasions path:

- **Test A**: Position with single commoner in check by a rook. Only generate 1 blocking move + N captures + commoner moves. Verify via known perft values.
- **Test B**: Position with double check. Verify no non-commoner moves generated.
- **Test C**: Position with leaper check (knight fork). Verify no blocking moves generated.
- **Test D**: Position with multiple commoners where only one is in check. Verify non-attacked commoner moves are still generated.

### 4.2 Full regression: `cargo run --release --example verify_perft`

Run the full 41-position perft suite at depths 1–6. All positions must match the expected values in `perft_values.md`. This is the definitive correctness check.

### 4.3 Test commands

```sh
cargo test             # unit tests
cargo clippy           # no warnings
cargo fmt              # formatting
cargo run --release --example verify_perft   # 41 positions, depths 1–6
```

---

## 5. Expected Impact

- **Check positions (Tests #2, #13, #33):** Non-commoner pseudo-legal moves reduced by 50–80 % in single-check, 90–100 % in double-check.
- **Non-check positions (all others):** Zero overhead — the `state.checkers.is_empty()` branch resolves at compile time to the same non-evasions path as today, with the same `is_move_trivially_legal()` fast-path.
- **Total:** 8–20 % overall speedup on the `verify_perft` benchmark.

---

## 6. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Incorrect between_bb for multi-commoner check | Low | `legal()` filter catches any false positives; tests verify no false negatives |
| Compiler fails to inline helpers | Low | `#[inline(always)]` on all evasion helpers |
| Pawn evasions miss en-passant that captures checker | Low | EP destination is always checked against target before generation |
| Performance regression on non-check path | None | `generate_non_evasions` path is identical to current `generate_pseudo_legal` |
