# Plan 1 — Check Evasion Generation

## Summary

When the side to move is **in check**, the current code generates **all**
pseudo-legal moves (pawns, knights, bishops, rooks, queens, commoners,
castling — typically 40–50 moves), then filters out those that are not
legal evasions. The fast-path `is_move_trivially_legal()` immediately
returns `false` for every move because `state.checkers` is non-empty,
so every move incurs the full `legal()` check. Most moves (especially
non-commoner, non-capturing, non-blocking) are rejected.

This plan replaces the all-move generation with **check-evasion generation**
that produces only moves that can possibly resolve the check. This is
the single largest remaining optimization according to the analysis.

| Metric | Value |
|--------|-------|
| **Estimated speedup** | **8–20 %** |
| **Effort** | ~120 lines across 1 file (`src/movegen.rs`) |
| **Risk** | Medium — correctness-critical for atomic chess blast interactions |
| **Tests most affected** | #13, #33, #2, #31 (tactical positions with many checks) |

### Reference: Fairy-Stockfish approach

Fairy-Stockfish `generate<LEGAL>()` (movegen.cpp:509–526) dispatches:
```cpp
moveList = pos.checkers() ? generate<EVASIONS>(pos, moveList)
                          : generate<NON_EVASIONS>(pos, moveList);
```

The `generate<EVASIONS>()` path (movegen.cpp:375–474) computes a single
`target` bitboard based on check type and generates only moves to those
squares:

| Check scenario | Target bitboard | Generated moves |
|---------------|-----------------|-----------------|
| Single check, sliding checker (bishop/rook/queen) | `between_bb(ksq, checksq) & ~own_pieces` | Blocking + capture + commoner |
| Single check, leaper checker (knight/pawn/adjacent-commoner) | `checkers` (capture only) | Capture + commoner |
| Double check (any) | `~own_pieces` (commoner-only target) | Only commoner moves |

Our implementation follows the same structure but adapted for atomic chess
(blast on capture, extinction pseudo-royal, pawn blast-immunity).

---

## Current Behavior (Baseline)

In `src/movegen.rs:231–259`:
```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    generate_pseudo_legal(board, moves);   // ← ALWAYS generates ALL moves

    // In-place compaction with trivially-legal fast-path
    let orig_len = moves.len();
    // ...
    for read_idx in 0..orig_len {
        let m = ms[read_idx];
        if is_move_trivially_legal(board, m, &state) || board.legal(m, &state) {
            // ...
        }
    }
}
```

When in check:
- `generate_pseudo_legal` produces ~40–50 moves (all piece types + castling)
- `is_move_trivially_legal` returns `false` for every move (line 913: `state.checkers` non-empty)
- `legal()` is called for every single move (~40–50 calls)
- Most `legal()` calls return `false` after computing blast, self-explosion, and attacker checks
- **Result:** >80 % of the generated moves are wasted work

---

## Proposed Changes

### File changed: `src/movegen.rs`

#### Change 1: Add `generate_evasions()` function

A new function that generates only pseudo-legal evasions when the side to
move is in check:

```rust
/// Generates pseudo-legal evasion moves for a position where the side to
/// move is in check. The caller must guarantee `!state.checkers.is_empty()`.
///
/// The generated moves still need to pass `board.legal(m, state)` because
/// atomic blast/capture interactions may affect legality. However, the
/// number of generated moves is much smaller than `generate_pseudo_legal`
/// (typically 5–15 vs. 40–50).
fn generate_evasions(board: &Board, state: &StateInfo, moves: &mut MoveList) {
    let us = board.side_to_move();
    let them = us.flip();
    let occupied = board.occupied();
    let checkers = state.checkers;
    let commoners = board.commoners(us);

    if commoners.is_empty() {
        return;
    }

    // The commoner that is being checked (pick the first one).
    // In practice there is at most 1 commoner in check positions
    // (if there are 2+, only the last commoner is pseudo-royal).
    let ksq = commoners.lsb();

    if checkers.more_than_one() {
        // DOUBLE CHECK: only commoner moves can resolve.
        // Generate commoner moves to all non-occupied squares.
        generate_commoner_moves(board, us, ksq, !occupied, moves);
        return;
    }

    // SINGLE CHECK
    let checksq = checkers.lsb();
    let checker_pt = board.piece_on(checksq).type_of();

    // Determine target squares for blocking and capturing.
    // Default: squares between commoner and checker (blocking squares).
    let mut target = between_bb(ksq, checksq) & !occupied;

    // Leaper checks (knight, pawn, adjacent commoner) cannot be blocked.
    if matches!(
        checker_pt,
        PieceType::Knight | PieceType::Pawn | PieceType::Commoner
    ) {
        // Only captures of the checking piece are possible.
        target = checkers;
    }

    // 1. Generate pawn moves to target (captures of checker + pushes to blocking squares).
    generate_pawn_moves_to_target(board, us, them, target, moves);

    // 2. Generate piece moves (knights, bishops, rooks, queens) to target.
    generate_piece_moves_to_target(board, us, them, target, occupied, moves);

    // 3. Generate commoner moves (to any non-occupied square).
    generate_commoner_moves(board, us, ksq, !occupied, moves);
}
```

#### Change 2: Add `generate_commoner_moves()` helper

```rust
fn generate_commoner_moves(
    board: &Board,
    us: Color,
    ksq: Square,
    target: Bitboard,
    moves: &mut MoveList,
) {
    let attacks = attacks::king_attacks(ksq) & target;
    let mut a = attacks;
    while !a.is_empty() {
        let to = a.pop_lsb();
        moves.push(Move::make_move(ksq, to));
    }
}
```

#### Change 3: Add `generate_pawn_moves_to_target()` helper

A variant of the existing `generate_pawn_moves_for` that filters both
pushes and captures to only those destinations in `target`:

```rust
fn generate_pawn_moves_to_target(
    board: &Board,
    us: Color,
    them: Color,
    target: Bitboard,
    moves: &mut MoveList,
) {
    let pawns = board.pieces_color_pt(us, PieceType::Pawn);
    let (push_dir, start_rank, promo_rank) = match us {
        Color::White => (8i8, Rank::R2, Rank::R8),
        Color::Black => (-8i8, Rank::R7, Rank::R1),
    };

    let mut p = pawns;
    while !p.is_empty() {
        let from = p.pop_lsb();
        let from_idx = from as i8;
        let from_rank = rank_of(from);

        // Single push (for blocking)
        let push_to = Square::from_index(from_idx + push_dir);
        if push_to != Square::NONE
            && board.empty(push_to)
            && (Bitboard::square_bb(push_to) & target) != Bitboard::EMPTY
        {
            if rank_of(push_to) == promo_rank {
                for &pt in &[
                    PieceType::Queen,
                    PieceType::Rook,
                    PieceType::Bishop,
                    PieceType::Knight,
                ] {
                    moves.push(Move::make_promotion(from, push_to, pt));
                }
            } else {
                moves.push(Move::make_move(from, push_to));
            }
        }

        // Captures (for capturing the checker)
        for df in &[-1i8, 1i8] {
            let target_f = file_of(from) as i8 + df;
            if !(0..=7).contains(&target_f) {
                continue;
            }
            let cap_to = Square::from_index(from_idx + push_dir + df);
            if cap_to == Square::NONE {
                continue;
            }
            if (Bitboard::square_bb(cap_to) & target) == Bitboard::EMPTY {
                continue;
            }
            // Only generate if the target square has an enemy piece
            // (for evasions, the only capture target is the checking piece)
            if (board.pieces_color(them) & Bitboard::square_bb(cap_to)) != Bitboard::EMPTY {
                if rank_of(cap_to) == promo_rank {
                    for &pt in &[
                        PieceType::Queen,
                        PieceType::Rook,
                        PieceType::Bishop,
                        PieceType::Knight,
                    ] {
                        moves.push(Move::make_promotion(from, cap_to, pt));
                    }
                } else {
                    moves.push(Move::make_move(from, cap_to));
                }
            }
        }
    }
}
```

#### Change 4: Add `generate_piece_moves_to_target()` helper

Generates knight, bishop, rook, queen moves that land on target squares:

```rust
fn generate_piece_moves_to_target(
    board: &Board,
    us: Color,
    them: Color,
    target: Bitboard,
    occupied: Bitboard,
    moves: &mut MoveList,
) {
    let piece_types = [
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
    ];

    for &pt in &piece_types {
        let mut pieces = board.pieces_color_pt(us, pt);
        while !pieces.is_empty() {
            let from = pieces.pop_lsb();
            let attacks = match pt {
                PieceType::Knight => attacks::knight_attacks(from),
                PieceType::Bishop => attacks::bishop_attacks(from, occupied),
                PieceType::Rook => attacks::rook_attacks(from, occupied),
                PieceType::Queen => attacks::queen_attacks(from, occupied),
                _ => unreachable!(),
            };
            // Only moves to target squares (capturing checker or blocking)
            let b = attacks & target;
            let mut a = b;
            while !a.is_empty() {
                let to = a.pop_lsb();
                moves.push(Move::make_move(from, to));
            }
        }
    }
}
```

#### Change 5: Modify `generate_legal()` to dispatch

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);

    if !state.checkers.is_empty() {
        // Generate only evasions (typically 5–15 moves instead of 40–50).
        generate_evasions(board, &state, moves);
    } else {
        // Not in check: generate all pseudo-legal moves as before.
        generate_pseudo_legal(board, moves);
    }

    // In-place legal filtering (same as before, but without fast-path
    // since trivially-legal is always false when in check, and when not
    // in check the existing fast-path is used above).
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

### What stays the same

- `src/board.rs` — no changes to `legal()`, `populate_state()`, `do_move()`,
  `StateInfo`, or any other board logic
- `src/types.rs` — no changes
- `src/attacks.rs` — no changes
- `src/bitboard.rs` — no changes
- `src/magic.rs` — no changes
- `src/lib.rs` — `perft()` stays the same

---

## Edge Cases and Correctness Considerations

### 1. En passant captures of the checking piece

A check can be delivered by a pawn that just double-pushed, making it
capturable en passant. The evasion generator must handle this:

- When the checker is a pawn on its starting rank, `target = checkers`
  sets the target to the checking pawn's square (NOT the e.p. square).
- Standard pawn capture generation targets the checking pawn's square,
  which works correctly.
- En passant capture of the checking pawn: the checker's square is
  the e.p. target square. Our pawn capture generation targets
  `checkers` (the checker's square), so an en passant capture would
  target `epSquare + push_dir` (which is the checker's square). This
  is a normal pawn capture in terms of the target square check.
- However, the en passant special move type is not generated in the
  evasion path. Instead, the pawn captures the checker via a normal
  capture move. **This is correct** because in atomic chess, en passant
  is a capture type — if the checking pawn is the e.p. target, a pawn
  capturing it via the e.p. mechanism would produce a different move
  encoding but the same board result after `do_move`.

**But wait** — the `legal()` function checks whether the move is en-passant
to compute blast correctly. If we generate a regular `Move::make_move`
instead of `Move::make_enpassant` for an en-passant capture of the
checking pawn, the blast computation would be wrong.

**Resolution:** The `is_capture` check in `legal()` (line 704–705) checks
`self.piece_on(to) != NO_PIECE`, which is true for the e.p. capture of
the checker (the checking pawn is on `to`). However, line 780 handles the
en-passant case differently (removing the captured pawn at `capsq` rather
than at `to`). If we don't generate an en-passant move type, the blast
removal of the captured pawn won't happen correctly.

**This means we MUST still generate proper en-passant moves when the
checker can be captured en passant.**

**Fix in `generate_evasions()`:** Add en passant capture generation when
the checker is a pawn and can be captured en passant:

```rust
// After pawn capture generation, check for en passant of the checker
if checker_pt == PieceType::Pawn, let Some(ep_sq) = board.ep_square() {
    if ep_sq == checksq {
        // The checking pawn can be captured en passant.
        // Generate en passant captures from adjacent pawns.
        let ep_attackers = board.pieces_color_pt(us, PieceType::Pawn)
            & attacks::pawn_attacks(them, checksq);
        let mut ea = ep_attackers;
        while !ea.is_empty() {
            let from = ea.pop_lsb();
            moves.push(Move::make_enpassant(from, checksq));
        }
    }
}
```

### 2. Double-check with multiple checkers

When the side to move has 2+ commoners and BOTH are in check, we have a
"double check" situation. Only commoner moves can resolve this (moving
a commoner out of check). If only one commoner remains (the common case),
double check means a game-ending move (cannot move and save the last
commoner). The evasion generator correctly handles this by generating
only commoner moves for double checks.

### 3. Adjacent commoner immunity

In atomic chess, adjacent commoners (even enemy) grant mutual blast
immunity. When a commoner moves in evasion, it may move away from an
adjacent enemy commoner and lose immunity, exposing it to attacks. The
`legal()` function handles this correctly (adjacency check at line 864).
Our evasion generation can ignore this nuance — `legal()` will filter
out unsafe moves.

### 4. Blast destroys the checking piece

When capturing the checking piece, the blast may destroy additional
pieces (including the capturer itself if non-pawn). The `legal()`
function handles this correctly. Evasion generation does not need to
account for blast — it just generates candidate moves.

### 5. Pinned pieces blocking checks

A pinned piece can block a check (by moving onto the checking line)
but only if the pinning piece is the same as the checking piece. The
`legal()` function will correctly validate or reject such moves. Our
blocking move generation produces moves to `between_bb(ksq, checksq)`
regardless of pin status, leaving the final check to `legal()`.

### 6. Promotion as evasion

If a pawn capture of the checking piece reaches a promotion rank,
promotion moves should be generated. Our `generate_pawn_moves_to_target`
already handles this (promotion rank check and all 4 promotion types).

### 7. Castling when in check

Castling is illegal when in check in atomic chess (same as standard
chess). Our evasion generator correctly does not generate castling
moves. (The current `generate_pseudo_legal` generates castling moves
even when in check, which are then rejected by `legal()`. We avoid
this waste.)

---

## Implementation Steps

### Step 1: Add new helper functions in `src/movegen.rs`

All new functions go in `src/movegen.rs`, before the existing
`generate_legal`:

1. `generate_commoner_moves()` — ~8 lines
2. `generate_pawn_moves_to_target()` — ~45 lines
3. `generate_piece_moves_to_target()` — ~30 lines
4. `generate_evasions()` — ~55 lines

### Step 2: Modify `generate_legal()`

Replace the unconditional `generate_pseudo_legal(board, moves)` call
with the dispatch:

```rust
if !state.checkers.is_empty() {
    generate_evasions(board, &state, moves);
} else {
    generate_pseudo_legal(board, moves);
}
```

### Step 3: Run correctness tests

```sh
cargo test
cargo run --release --example verify_perft
```

All 41 positions, depths 1–6, must produce identical node counts to
the baseline.

### Step 4: Measure performance

```sh
# Before (baseline):
cargo run --release --example verify_perft
# After:
cargo run --release --example verify_perft
```

Compare total wall-clock time and individual timings for tests #2, #13, #33.

---

## Testing Strategy

| Test | Description | Expected |
|------|-------------|----------|
| `cargo test` | All unit tests | Pass |
| `cargo run --release --example verify_perft` | Full 41-position perft | All pass, counts match |
| Test #13 (slowest, tactical with checks) | `r1b1Brk1/ppp5/...` depth 6 | Matches expected 2,160,817,389 |
| Test #33 (heavy checking) | Depth 6 | Matches expected |
| Test #2 (tactical) | Depth 6 | Matches expected |

**Additional manual verification:**

```sh
# Quick spot-check on a position with double check
# (craft a FEN with 2 checks, verify only commoner moves are generated)
cargo run --example pertt_divide "FEN" 1

# Spot check a single-check position
# (verify blocking + capture + commoner moves are generated)
```

---

## Performance Targets

| Benchmark | Current (93.940 s) | Target | Speedup |
|-----------|-------------------|--------|---------|
| Total verify_perft | 93.940 s | 75–86 s | 8–20 % |
| Test #13 (slowest) | 14.098 s | 11.3–13.0 s | 8–20 % |
| Test #33 | 13.208 s | 10.6–12.1 s | 8–20 % |
| Test #2 | 11.827 s | 9.5–10.9 s | 8–20 % |

Cumulative speedup from original baseline (124.380 s):

```
1 - (1 - 0.111)(1 - 0.029)(1 - 0.093)(1 - 0.033)(1 - 0.14)
≈ 1 - 0.889 × 0.971 × 0.907 × 0.967 × 0.860
≈ 1 - 0.645 (optimistic) to 1 - 0.702 (conservative)
≈ 30–35 % cumulative
```

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| **En passant capture of checking pawn** | Incorrect blast computation | Add explicit e.p. generation in `generate_evasions()` when the checker is a pawn capturable en passant |
| **Missing promotion moves** | Missing evasions | `generate_pawn_moves_to_target` already handles promotions; verify with test positions |
| **Double-check edge cases** | Missing commoner moves | Only commoner moves generated for double check; verified by perft |
| **Between_bb for non-aligned squares** | Incorrect blocking targets | `between_bb` returns empty for non-aligned; safe |
| **Blast destroys blockading piece** | Blockade considered illegal by legal() | Correct — `legal()` validates blast effects; we only generate candidates |
| **Check by adjacent commoner** | Non-slider check not blockable | Handled by the leaper branch (Commoner → only capture) |
| **LazyLock in between_bb** | Slow blocking computation | `between_bb` is loop-based (not LazyLock-bound); performance impact negligible since it's called once per `generate_evasions` call |
| **Regression on non-check positions** | Slower when not in check | Non-check path is unchanged (same `generate_pseudo_legal` call); no regression |

---

## Relationship to Optimization Pipeline

| Item | Status | Notes |
|------|--------|-------|
| **Item 2** (cache pseudoRoyals) | ❌ Not yet | Phase 1 — estimated 4–8 %, planned separately |
| **Item 1** (evasion generation) | ⏳ **This plan** | Phase 1 — highest impact item |
| **Item 6** (fused attackers_to) | ❌ Not yet | Phase 1 — can be done independently |
| **Item 3** (BetweenBB table) | ❌ Not yet | Phase 2 — depends on avoiding LazyLock |
| **Item 4** (compute_pinned opt) | ❌ Not yet | Phase 2 — depends on Item 3 for between_bb |
| **Item 5** (LazyLock elimination) | ❌ Not yet | Phase 2 |

After this plan, the next highest-impact items are **Item 2** (cache
pseudoRoyals in StateInfo, 4–8 %) and **Item 6** (fused attackers_to,
2–3 %), both of which are independent and can be implemented in any
order.
