# Plan: Fix Atomic Move Generator Correctness for Depths 5–6

## Current Status

| Depth | 1 | 2 | 3 | 4 | 5 | 6 |
|-------|---|---|---|---|---|---|
| P1–P12 all ✓ | ✓ | 10/12 | 11/12 | 8/12 | — | — |

See `plans/perft_fix/report.md` for the full per-position breakdown. The root causes are systematic errors in the blast model and pseudo-royal legality check that compound at depth 4+.

---

## Bug #1 (Critical) — Capturing Pawns Blasted at Ground Zero

**Observations:**
- Perft fixed at D1 by the previous round of fixes, but D4+ values still diverge.
- Positions where pawns do most of the capturing (e.g., starting position) show systematic negative deltas at D4+.
- Positions with many non-pawn captures show mixed or positive deltas.

**Root cause in reference (Fairy-Stockfish `position.cpp` lines 2000–2001):**

```cpp
Bitboard blast = blast_on_capture()
    ? ((attacks_bb<KING>(to) & ((pieces(WHITE) | pieces(BLACK)) ^ pieces(PAWN))) | to)
      & (pieces() ^ blastImmune)
    : …;
```

The blast formula always includes `| to` (the capture square) unconditionally. The adjacent-pawn exclusion (`^ pieces(PAWN)`) applies only to the **adjacent** squares (`attacks_bb<KING>(to)`), not to `to` itself. So the capturer at `to` is **always destroyed**, even when it is a pawn.

The **ICC rules** confirm this interpretation:

> *"When a capture is made, the capturing piece is removed from the board. All pieces within a king's move of the square where the capture occurs, except pawns, are also removed."*

The first sentence unconditionally removes the capturer; the second sentence's "except pawns" exemption applies only to the **adjacent** blast wave.

**Current Rust code (`src/board.rs` lines 466–470):**

```rust
let capturer = self.squares[to as usize];
if capturer == NO_PIECE || capturer.type_of() != PieceType::Pawn {
    to_blast = to_blast | Bitboard::square_bb(to);
}
```

This was Bug #6 from the previous fix — but it **incorrectly** exempts the capturer at `to` when it is a pawn. The fix is to **always** include `to` in the blast zone, matching the reference.

**Impact:** This bug reduces the number of surviving pawns, which increases the number of future positions where sliders are unblocked → more attacks found → **some** moves correctly rejected that were previously allowed (reducing the positive bias). It also changes follow-up positions (one fewer pawn) which can add or subtract moves depending on the position.

**Fix:**
```rust
// Always blast the capturer at ground zero (pawns are NOT immune at `to`).
to_blast = to_blast | Bitboard::square_bb(to);
// Remove the special-case `if capturer == NO_PIECE || …` block entirely.
```

---

## Bug #2 (Critical) — Pseudo-Royal Immunity Uses Pre-Blast Opponent State

**Observations:**
- Several positions show a NEGATIVE bias (Rust finds *fewer* legal moves than the reference). This cannot be explained by Bug #1 alone (which would reduce moves further).
- The effect appears in positions where commoners are near each other and blasts destroy one of them.

**Root cause in reference (`position.cpp` lines 1156, 1185):**

The reference computes `pseudoRoyalsTheirs` from the **current** (pre-move, pre-blast) state:

```cpp
// line 1156 (BEFORE the pseudo-royal position update at line 1161–1162):
Bitboard pseudoRoyalsTheirs = st->pseudoRoyals & pieces(~sideToMove);
```

This bitboard includes enemy commoners that are about to be **destroyed by the blast**. The immunity check at line 1185:

```cpp
// Touching pseudo-royal pieces are immune
if (  !(blast_on_capture() && (pseudoRoyalsTheirs & attacks_bb<KING>(sr)))
    && (attackers_to(sr, occupied, ~us) & attackerCandidatesTheirs))
    return false;
```

If an enemy commoner was adjacent to our commoner before the blast (even if it gets destroyed by the blast), the reference **still grants immunity** and skips the attack check.

**Current Rust code (`src/board.rs` lines 749–751):**

```rust
let adjacent_enemy_commoners =
    board.commoners(them) & attacks::king_attacks(ksq);
if adjacent_enemy_commoners.is_empty() {
    // check attackers
}
```

Here `board` is a clone of the original board after `do_move()` (which already applied the blast). So `board.commoners(them)` uses the **post-blast** state. If the enemy commoner was destroyed, it is not considered adjacent and immunity is **not** granted.

**Impact:** The Rust code rejects moves where:
1. An enemy commoner adjacent to our commoner is destroyed by the blast
2. Another enemy piece attacks our commoner

The reference considers these moves legal (immunity granted). The Rust code rejects them (no immunity → attack found). This explains the **negative bias** seen in positions 1, 3, 4, 10.

**Fix:** Match the reference behavior by using pre-blast enemy commoner positions for the immunity check. In the `legal()` function, before cloning and `do_move()`, record the enemy commoner positions from the current board:

```rust
// Before do_move(), capture enemy commoner positions for immunity check
let pre_blast_enemy_commoners = self.commoners(self.side_to_move().flip());

// … clone and do_move() …

// Use pre-blast positions for the immunity check:
let adjacent_enemy_commoners =
    pre_blast_enemy_commoners & attacks::king_attacks(ksq);
```

---

## Bug #3 (Critical) — Legal Check Uses Post-Move `occupied` But Reference Pre-Computes It

**Observations:**
- The refactored legal check (clone + `do_move`) is functionally correct for most cases but has a subtle difference in how `occupied` is computed for the pseudo-royal attack check.
- The reference uses the **current board's piece positions** to compute the blast removal, not the after-move positions.

**Root cause in reference (`position.cpp` lines 1151–1152):**

```cpp
if (capture(m) && blast_on_capture())
    occupied &= ~((attacks_bb<KING>(kto) & ((pieces(WHITE) | pieces(BLACK)) ^ pieces(PAWN))) | kto);
```

The `pieces()` in the formula refer to the **current board**, not the `occupied` bitboard being built. This means:
- The moving piece is still at `from` (its origin) in `pieces()`
- The captured piece is still at `to` in `pieces()`
- `pieces(PAWN)` is the pre-move pawn set

**Current Rust code (`src/board.rs` lines 462–463):**

```rust
let blast_zone = attacks::king_attacks(to) & !self.by_type[PieceType::Pawn as usize];
let mut to_blast = blast_zone & self.pieces();
```

After `do_move()`, `self.pieces()` has the moving piece at `to`, not at `from`. For most cases this gives the same result because:
- `from` is empty after the move and `& self.pieces()` filters it out
- Pawns at `from` before the move are now at `to`

However, there is a **subtle difference**: the reference's `(all_pieces ^ pieces(PAWN))` uses the pre-move pawn set. If the capturing piece was a pawn and `from` was adjacent to `to`, the reference's formula sees a pawn at `from` (excluded from adjacency blast) while the Rust code sees an empty `from` (included in adjacency blast but filtered by `& self.pieces()`).

**Impact:** This is likely a small effect but could explain some of the remaining discrepancies after fixing Bugs #1 and #2.

**Fix:** Restructure the legal check to use pre-move board state for the blast computation, similar to the reference. The cleanest way is to compute `occupied` before the move (as the reference does) rather than relying on `do_move` on a clone.

However, the clone+do_move approach is simpler and less error-prone. An alternative is to keep the clone+do_move approach but pass the pre-move board's piece set to the blast computation. But this is complex.

**Preferred approach:** Switch to the reference's pre-computation model for the legal check. Instead of cloning and calling do_move(), compute the resulting `occupied` bitboard directly:

```rust
pub fn legal(&self, m: Move) -> bool {
    let from = m.from_sq();
    let to = m.to_sq();
    let us = self.side_to_move;
    let them = us.flip();

    // Castling pass-through check (same as current, uses self)
    if m.move_type() == MoveType::Castling {
        // … (keep current castling check) …
    }

    // Pre-compute occupied after the move (without cloning/do_move)
    let mut occupied = self.pieces() ^ from; // remove moving piece from origin
    let mut kto = to;

    if m.move_type() == MoveType::Castling {
        // … adjust kto and occupied for castling …
    }

    occupied |= kto; // add piece at destination

    if m.move_type() == MoveType::EnPassant {
        let capsq = match us {
            Color::White => Square::from_index(to as i8 - 8),
            Color::Black => Square::from_index(to as i8 + 8),
        };
        occupied &= !Bitboard::square_bb(capsq);
    }

    let is_capture = m.move_type() == MoveType::EnPassant
        || self.piece_on(to) != NO_PIECE;

    if is_capture {
        // Blast: remove adjacent non-pawn pieces AND destination
        // IMPORTANT: use self.* (pre-move board state), not occupied
        let pre_pawns = self.by_type[PieceType::Pawn as usize];
        let pre_non_pawns = self.pieces() ^ pre_pawns;
        let blast_adjacent = attacks::king_attacks(kto) & pre_non_pawns;
        occupied &= !(blast_adjacent | Bitboard::square_bb(kto));
    }

    // Self-explosion check
    let our_commoners = self.commoners(us) & occupied;
    if our_commoners.is_empty() {
        return false;
    }

    // Check each own commoner for attacks
    let them_commoners = self.commoners(them);
    let mut c = our_commoners;
    while !c.is_empty() {
        let ksq = c.pop_lsb();
        // Immunity: use pre-blast enemy commoner positions
        let adjacent_enemy = them_commoners & attacks::king_attacks(ksq);
        if adjacent_enemy.is_empty() {
            // Check attackers using the computed occupied
            let attackers = self.attackers_to(ksq, occupied)
                & self.by_color[them as usize];
            if attackers != Bitboard::EMPTY {
                return false;
            }
        }
    }

    true
}
```

This approach:
1. Matches the reference's blast computation (uses pre-move `pieces()`)
2. Uses pre-blast enemy commoners for immunity (matches Bug #2 fix)
3. Eliminates the clone+do_move overhead
4. Still handles castling (do the pass-through check before the main logic)

---

## Verification Strategy

### Step 1: Apply Bug #1 fix (always blast `to`)

Change `src/board.rs` `do_move()`:
- Remove the `if capturer == NO_PIECE || capturer.type_of() != PieceType::Pawn` guard
- Always add `to` to the blast set

Run quick unit tests and a shallow perft spot-check (debug is fine here):
```bash
cargo test --lib
cargo test --test perft_tests
cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 4
```

Expected: D4 value moves from 197314 toward 197326 (should reduce the −12 gap).

### Step 2: Apply Bug #2 and #3 fixes together

Since Bug #3 completely restructures the `legal()` function, Bug #2 is naturally fixed as part of it (pre-blast enemy commoners). 

Do both in one refactoring:
1. Rewrite `legal()` to use the pre-computation model
2. Remove the clone+do_move approach

Run tests (release for depth≥4 perft, debug for unit tests):
```bash
cargo test --lib
cargo test --test perft_tests
cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 4
cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 5
cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6
```

### Step 3: Full perft validation

Test all 12 positions at depths 1–6. Run the integration tests under `--release` — debug builds would take **hours** at depths 5–6:

```bash
# Depths 1–4 are fast enough in debug for quick iteration:
cargo test --test perft_tests

# Full validation including depths 5–6 MUST use release:
cargo test --test perft_tests --release
```

Update the test expectations to include depths 4–6 for all positions.  
*Note: when iterating on fixes, test depths 5–6 only after depths 1–4 pass, and always use `--release` for the deep checks.*

### Step 4: Debug remaining differences (if any)

If discrepancies remain after Steps 1–3, use `perft_divide` on specific positions to find the exact moves that differ. Use `--release` for depth≥3:

```bash
# Depths 1–2 work in debug:
cargo run --example perft_divide "FEN" 2

# Deeper divides need release:
cargo run --release --example perft_divide "FEN" 3
```

Compare the divide output between Rust and the reference (Fairy-Stockfish).

Check for these additional edge cases:
1. **En passant blast zone**: Verify that en passant blast adjacency uses the correct square (the destination, not the captured pawn's square)
2. **Castling through check**: Verify that the castling pass-through check correctly applies adjacency immunity for each square
3. **Promotion + blast**: Verify that a promotion on a capture correctly blasts the promoted piece

---

## Summary

| Bug | File | Impact | Fix |
|-----|------|--------|-----|
| #1 — Capturing pawn not blasted at `to` | `board.rs:466-470` | Systematic positive bias; changes subsequent positions | Always include `to` in blast zone |
| #2 — Immunity uses post-blast enemy commoners | `board.rs:749-751` | Systematic negative bias | Use pre-blast enemy commoner positions |
| #3 — Legal check uses after-move `pieces()` for blast | `board.rs:698-783` | Subtle compound effect | Switch to pre-computation model matching reference |

All three fixes should bring perft into parity with `perft_values.md` through depth 6.
