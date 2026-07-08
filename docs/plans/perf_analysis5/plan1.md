# Plan 1 — EVASIONS / NON_EVASIONS Move-Generation Split

**Corresponds to:** Item 1 of `analysis.md` — *"Add EVASIONS/NON_EVASIONS move generation split"*
**Estimated speedup:** 8–20 %
**Effort:** ~130 lines changed across `src/movegen.rs`, `src/board.rs` (moderate)
**Fairy-Stockfish reference:** `movegen.cpp:376–474` (`generate_all<EVASIONS>`) and `movegen.cpp:509–526` (`generate<LEGAL>`)

---

## 1. Problem

`generate_legal()` (`src/movegen.rs:233–257`) currently calls `generate_pseudo_legal()` unconditionally, generating **all** pseudo-legal moves (30–80+ in typical positions) regardless of whether the side is in check. When in check:

1. `is_move_trivially_legal()` immediately returns `false` (because `state.checkers` is non-empty at `board.rs:901`).
2. Every pseudo-legal move falls through to the full `legal()` check — blast simulation, commoner adjacency, `attackers_to` — even for non-commoner moves that can't possibly resolve the check.
3. In double-check positions (≥2 checkers), **no** non-commoner move can ever be legal, yet all are still generated and filtered.

`perf` shows this waste is significant: tactical positions with checks (Tests #2, #13, #33 — the 3 slowest) spend the majority of time filtering irrelevant non-commoner moves.

---

## 2. Atomic Chess Check Semantics

Before designing the evasions path, it's important to understand all the ways a position can be "in check" in atomic chess. `compute_checkers()` (`board.rs:429–470`) identifies two kinds of checks:

### 2.1 Standard checks (sliding + leaper)

Rook, Bishop, Queen, Knight, and Pawn attacks on our commoner squares. These use the standard `attacks::*_attacks(ksq, occupied)` and `attacks::pawn_attacks(us, ksq)` lookups. Computed in the slider/leaper loop at `board.rs:442–455`.

### 2.2 Commoner adjacency checks

Enemy commoners adjacent to any of our commoners also appear in `checkers` (`board.rs:458–468`). This is the extinction pseudo-royal rule: adjacent commoners threaten each other. These are **leaper checks** (king-step distance) and **cannot be blocked** — only captured or evaded.

**Key insight:** A commoner adjacency check means the checker is an enemy commoner that is king-adjacent to one of our commoners. The only non-commoner way to resolve it is capturing the adjacent enemy commoner.

---

## 3. Design

### 3.1 New entry-point: `generate_legal()`

Refactor `generate_legal()` to branch on check state **before** generating any moves:

```
generate_legal(board, moves):
    state = StateInfo
    board.populate_state(&state)

    if state.checkers.is_empty():
        generate_pseudo_legal(board, moves)       // unchanged — public API preserved
    else:
        generate_evasions(board, &state, moves)   // restricted target

    // Compaction — still needed because evasion-generated moves can be
    // illegal for non-check reasons (self-explosion, adjacency, pin).
    compact(moves, board, &state)
```

**Key decisions:**
- **`generate_pseudo_legal` keeps its name and public visibility.** It's the existing public API. Only `generate_legal()` is refactored internally.
- The compaction pass is unchanged: `is_move_trivially_legal()` + `legal()` for non-evasions, `legal()` only for evasions (since `is_move_trivially_legal` always returns false when checkers is non-empty — no point calling it).

### 3.2 Evasions path: `generate_evasions()`

New `pub(crate)` function that generates a restricted set of pseudo-legal moves based on the check configuration.

#### 3.2.1 Double-check shortcut

When `state.checkers.more_than_one()`:
- Only generate **commoner moves** (to non-own squares). No non-commoner move can resolve a double check.
- Skip all pawn/knight/bishop/rook/queen/castling generation entirely.
- The `legal()` filter still applies (self-explosion, adjacency, commoner safety).

This mirrors FS `movegen.cpp:385`: `if (Type != EVASIONS || !more_than_one(pos.checkers()))`.

**Note on commoner adjacency double-checks:** If `checkers` contains two entries and both are enemy commoner adjacency checks, the above shortcut is still correct — only our commoner moves can resolve it (move our commoner away, or capture one of the adjacent enemy commoners).

#### 3.2.2 Single-check logic

When exactly one checker (`!state.checkers.more_than_one()` and `!state.checkers.is_empty()`):

1. **Identify the checker:**
   ```rust
   let checker_sq = state.checkers.lsb();
   let checker_piece = board.piece_on(checker_sq);
   let checker_type = checker_piece.type_of();
   ```

2. **Classify the check type:**
   - **Slider check** (Rook, Bishop, Queen): Can be blocked or captured.
   - **Leaper check** (Knight, Pawn, Commoner-adjacency): Can only be captured (no blocking squares).

3. **Compute the restricted `target`** for non-commoner moves:

   ```rust
   let mut target = state.checkers;  // captures of checker always in target

   let is_slider = matches!(checker_type,
       PieceType::Rook | PieceType::Bishop | PieceType::Queen);

   if is_slider {
       // Find which commoner(s) this slider is attacking, add blocking squares
       let mut commoners = board.commoners(us);
       while !commoners.is_empty() {
           let ksq = commoners.pop_lsb();
           // Check if the checker actually attacks this commoner
           let between = between_bb(ksq, checker_sq);
           // The checker attacks ksq if they're on the same line and
           // there are no pieces between them (except possibly the commoner itself)
           if line_bb(ksq, checker_sq) != Bitboard::EMPTY {
               // Verify no blockers between checker and commoner
               let blockers = between & board.occupied();
               if blockers.is_empty() {
                   target = target | between;
               }
           }
       }
   }
   ```

   **Simplification:** We can use a simpler approach — compute the checker's attack set from the checker square and test if it includes our commoner:

   ```rust
   if is_slider {
       let mut commoners = board.commoners(us);
       let occupied = board.occupied();
       while !commoners.is_empty() {
           let ksq = commoners.pop_lsb();
           // Use the appropriate slider attack from the checker's square
           let checker_attacks = match checker_type {
               PieceType::Rook => attacks::rook_attacks(checker_sq, occupied),
               PieceType::Bishop => attacks::bishop_attacks(checker_sq, occupied),
               PieceType::Queen => attacks::queen_attacks(checker_sq, occupied),
               _ => unreachable!(),
           };
           if (checker_attacks & Bitboard::square_bb(ksq)) != Bitboard::EMPTY {
               target = target | between_bb(ksq, checker_sq);
           }
       }
   }
   ```

   **Even simpler (recommended):** Since `compute_checkers` already verified the checker attacks a commoner, and we only have one checker, we know at least one commoner is attacked. For the typical single-commoner case (one King), there's exactly one between_bb to add. For multi-commoner, we iterate and add all relevant between_bb sets. Over-generating (adding all commoners' between_bb even if not attacked) is safe — `legal()` filters false positives.

   **Recommended final approach:**
   ```rust
   if is_slider {
       let mut commoners = board.commoners(us);
       while !commoners.is_empty() {
           let ksq = commoners.pop_lsb();
           target = target | between_bb(ksq, checker_sq);
       }
   }
   ```
   This over-generates slightly for multi-commoner positions where only one commoner is attacked, but `legal()` handles it. The simplicity and branch-free nature is more important for performance.

4. **Generate non-commoner moves restricted to `target`.**
5. **Generate commoner moves unrestricted** (all king-attack squares intersected with `!own_pieces`). The `legal()` filter catches any that don't resolve the check.
6. **Skip castling** (never legal when in check).

#### 3.2.3 En-passant edge case

En-passant can resolve a check in two ways:
1. **The checking pawn can be captured en-passant.** The EP destination square differs from the checker square (e.g., checker is on d4, EP destination is d3). The EP target square is in `target` only if `target` includes `checkers` (which it always does). But the *EP destination* square is not the checker square — it's one rank behind. So we need special handling.
2. **The EP capture blast zone destroys the checker.** This is handled by `legal()`, not by movegen.

**Solution:** When generating pawn evasion moves, always include en-passant captures of the checking piece. Check if the checker is a pawn and `board.ep_square()` is set:

```rust
// In generate_evasion_pawn_moves:
if let Some(ep_sq) = board.ep_square() {
    // The captured pawn in EP is one rank behind the EP square
    let ep_captured_sq = match us {
        Color::White => Square::from_index(ep_sq as i8 - 8),
        Color::Black => Square::from_index(ep_sq as i8 + 8),
    };
    // If the captured pawn is the checker, include this EP
    if Bitboard::square_bb(ep_captured_sq) & state.checkers != Bitboard::EMPTY {
        // Generate EP moves for adjacent pawns
        // (same logic as current generate_pawn_moves_for EP section)
    }
}
```

Alternatively, simply **always generate EP moves** in the evasions path (there are at most 2 EP capture moves per position). The `legal()` filter handles illegality. This is simpler and the performance cost is negligible.

**Recommended:** Always generate EP moves in evasions — simplest, safest.

#### 3.2.4 FS equivalence table

| FS concept | atomic_movegen equivalent |
|---|---|
| `pos.checkers()` | `state.checkers` |
| `ksq` (king square) | Each commoner in `board.commoners(us)` |
| `between_bb(ksq, lsb(pos.checkers()))` | `between_bb(commoner_sq, checker_sq)` for each commoner |
| `more_than_one(pos.checkers())` | `state.checkers.more_than_one()` |
| `non_sliding_riders()` | Commoner adjacency checkers (leaper type) |
| `LeaperAttacks[type][sq] & ksq` | Knight/Pawn check → target = checkers only |
| King moves unrestricted target | Commoner moves with target `!own_pieces` |

---

## 4. Implementation Steps

### Step 1: Add helper `attacks_for_pt()` (~15 lines) in `movegen.rs`

A dispatch function returning the attack set for a given piece type on a square:

```rust
#[inline(always)]
fn attacks_for_pt(pt: PieceType, sq: Square, occupied: Bitboard) -> Bitboard {
    match pt {
        PieceType::Knight => attacks::knight_attacks(sq),
        PieceType::Bishop => attacks::bishop_attacks(sq, occupied),
        PieceType::Rook => attacks::rook_attacks(sq, occupied),
        PieceType::Queen => attacks::queen_attacks(sq, occupied),
        PieceType::Commoner => attacks::king_attacks(sq),
        _ => Bitboard::EMPTY,
    }
}
```

### Step 2: Add `generate_evasions()` (~60 lines) in `movegen.rs`

```rust
/// Generate pseudo-legal evasion moves when the side to move is in check.
///
/// Restricts non-commoner moves to the check evasion target (blocking and
/// capturing the checker). Commoner moves are unrestricted.
fn generate_evasions(board: &Board, state: &StateInfo, moves: &mut MoveList) {
    let us = board.side_to_move();
    let them = us.flip();
    let occupied = board.occupied();
    let our_pieces = board.pieces_color(us);

    // --- Commoner moves (always generated, unrestricted) ---
    let commoner_target = !our_pieces;
    let mut commoners = board.pieces_color_pt(us, PieceType::Commoner);
    while !commoners.is_empty() {
        let from = commoners.pop_lsb();
        let mut a = attacks::king_attacks(from) & commoner_target;
        while !a.is_empty() {
            let to = a.pop_lsb();
            moves.push(Move::make_move(from, to));
        }
    }

    // --- Double check: only commoner moves ---
    if state.checkers.more_than_one() {
        return;
    }

    // --- Single check: non-commoner moves restricted to target ---
    let checker_sq = state.checkers.lsb();
    let checker_type = board.piece_on(checker_sq).type_of();
    let is_slider = matches!(checker_type,
        PieceType::Rook | PieceType::Bishop | PieceType::Queen);

    let mut target = state.checkers; // always allow capturing the checker
    if is_slider {
        // Add blocking squares between each commoner and the checker
        let mut c = board.commoners(us);
        while !c.is_empty() {
            let ksq = c.pop_lsb();
            target = target | between_bb(ksq, checker_sq);
        }
    }

    // Pawns
    let mut p = board.pieces_color_pt(us, PieceType::Pawn);
    while !p.is_empty() {
        let from = p.pop_lsb();
        generate_pawn_evasion_moves(board, us, them, from, target, moves);
    }

    // Knights, Bishops, Rooks, Queens
    for &pt in &[PieceType::Knight, PieceType::Bishop, PieceType::Rook, PieceType::Queen] {
        let mut pieces = board.pieces_color_pt(us, pt);
        while !pieces.is_empty() {
            let from = pieces.pop_lsb();
            let mut a = attacks_for_pt(pt, from, occupied) & target;
            while !a.is_empty() {
                let to = a.pop_lsb();
                moves.push(Move::make_move(from, to));
            }
        }
    }

    // No castling when in check
}
```

### Step 3: Add `generate_pawn_evasion_moves()` (~55 lines) in `movegen.rs`

A variant of `generate_pawn_moves_for` that restricts push/capture destinations to `target`, but always includes en-passant (at most 1 EP move per pawn, `legal()` filters):

```rust
fn generate_pawn_evasion_moves(
    board: &Board, us: Color, them: Color,
    from: Square, target: Bitboard, moves: &mut MoveList,
) {
    let from_rank = rank_of(from);
    let from_f = file_of(from) as i8;
    let (push_dir, push_double, start_rank, promo_rank) = match us {
        Color::White => (8i8, 16i8, Rank::R2, Rank::R8),
        Color::Black => (-8i8, -16i8, Rank::R7, Rank::R1),
    };
    let from_idx = from as i8;

    // Single push — only if destination is in target
    let to_idx = from_idx + push_dir;
    let to_sq = Square::from_index(to_idx);
    if to_sq != Square::NONE && board.empty(to_sq) {
        if (Bitboard::square_bb(to_sq) & target) != Bitboard::EMPTY {
            if rank_of(to_sq) == promo_rank {
                for &pt in &PROMOTION_PIECES {
                    moves.push(Move::make_promotion(from, to_sq, pt));
                }
            } else {
                moves.push(Move::make_move(from, to_sq));
            }
        }

        // Double push — only if destination is in target
        if from_rank == start_rank {
            let to_idx2 = from_idx + push_double;
            let to_sq2 = Square::from_index(to_idx2);
            if to_sq2 != Square::NONE && board.empty(to_sq2)
                && (Bitboard::square_bb(to_sq2) & target) != Bitboard::EMPTY
            {
                moves.push(Move::make_move(from, to_sq2));
            }
        }
    }

    // Captures — only if destination is in target
    for df in &[-1i8, 1i8] {
        let target_f = from_f + df;
        if !(0..=7).contains(&target_f) { continue; }
        let to_idx = from_idx + push_dir + df;
        let to_sq = Square::from_index(to_idx);
        if to_sq == Square::NONE { continue; }
        if file_of(to_sq) as i8 != target_f { continue; }
        if (board.pieces_color(them) & Bitboard::square_bb(to_sq)) != Bitboard::EMPTY
            && (Bitboard::square_bb(to_sq) & target) != Bitboard::EMPTY
        {
            if rank_of(to_sq) == promo_rank {
                for &pt in &PROMOTION_PIECES {
                    moves.push(Move::make_promotion(from, to_sq, pt));
                }
            } else {
                moves.push(Move::make_move(from, to_sq));
            }
        }
    }

    // En passant — always generate (at most 1 per pawn, legal() filters)
    if let Some(ep_sq) = board.ep_square() {
        let ep_f = file_of(ep_sq) as i8;
        if ep_f == from_f - 1 || ep_f == from_f + 1 {
            let df = ep_f - from_f;
            let to_idx = from_idx + push_dir + df;
            let to_sq = Square::from_index(to_idx);
            if to_sq == ep_sq {
                moves.push(Move::make_enpassant(from, ep_sq));
            }
        }
    }
}
```

### Step 4: Refactor `generate_legal()` (~30 lines) in `movegen.rs`

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);

    if state.checkers.is_empty() {
        generate_pseudo_legal(board, moves);
    } else {
        generate_evasions(board, &state, moves);
    }

    // In-place compaction: filter out illegal moves.
    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    let in_check = !state.checkers.is_empty();
    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            // In check: skip trivially-legal (always false), go straight to legal().
            // Not in check: try trivially-legal fast-path first.
            let keep = if in_check {
                board.legal(m, &state)
            } else {
                is_move_trivially_legal(board, m, &state) || board.legal(m, &state)
            };
            if keep {
                ms[write_idx] = m;
                write_idx += 1;
            }
        }
        write_idx
    };
    moves.set_len(new_len);
}
```

### Step 5: Import `between_bb` in `movegen.rs`

Add `use crate::bitboard::between_bb;` to the imports at the top of `movegen.rs`.

---

## 5. Files Changed

| File | Change | Lines |
|------|--------|-------|
| `src/movegen.rs` | Add `generate_evasions()`, `generate_pawn_evasion_moves()`, `attacks_for_pt()`. Refactor `generate_legal()`. Add import. | ~130 new/modified |
| `src/board.rs` | No changes needed. | 0 |

---

## 6. Correctness Verification

### 6.1 Unit tests in `movegen.rs`

Add `#[cfg(test)]` tests for the evasions path:

- **Test A: Single slider check.** Position with one commoner in check by a rook on an open file. Verify that the evasion-generated move list is a subset of the full pseudo-legal list filtered by `legal()`.
- **Test B: Double check.** Position with a rook and a knight giving check. Verify only commoner moves are generated.
- **Test C: Leaper check (knight).** Verify no blocking moves generated, only capture of the knight or commoner moves.
- **Test D: Commoner adjacency check.** Position with adjacent enemy commoner. Verify the evasion path handles this correctly.
- **Test E: En-passant captures checker.** A checking pawn that can be captured en-passant. Verify the EP move is generated.
- **Test F: Multi-commoner, only one attacked.** Verify non-attacked commoner can still move.

### 6.2 Full regression

```sh
cargo test                                          # unit tests
cargo clippy                                        # no warnings
cargo fmt                                           # formatting
cargo run --release --example verify_perft           # 41 positions, depths 1–6
```

All 41 perft positions must match the expected values in `perft_values.md`. This is the definitive correctness check.

---

## 7. Expected Impact

- **Check positions (Tests #2, #13, #33):** Non-commoner pseudo-legal moves reduced by 50–80 % in single-check, 90–100 % in double-check.
- **Non-check positions (all others):** Zero overhead — the `state.checkers.is_empty()` branch goes to the unchanged `generate_pseudo_legal` path with the same `is_move_trivially_legal()` fast-path.
- **Total:** 8–20 % overall speedup on the `verify_perft` benchmark.

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Incorrect between_bb target for multi-commoner check | Low | Over-generating (union of all commoners' between_bb) is safe; `legal()` catches false positives. Under-generating would be caught by verify_perft. |
| En-passant evasion missed | Low | Always generating EP in evasions path eliminates this risk entirely. |
| Commoner adjacency check not handled | None | Adjacency checkers are in `state.checkers`, classified as leaper → target = checkers → must capture. |
| Compiler fails to inline helpers | Low | `#[inline(always)]` on all evasion helpers. |
| Performance regression on non-check path | None | `generate_pseudo_legal` path is unchanged. |
| `generate_pseudo_legal` public API breakage | None | Function name and signature preserved. |
