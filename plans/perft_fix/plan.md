# Plan: Fix Atomic Move Generator

## Root Cause

The perft mismatches are caused by **missing blast adjacency immunity** in the `legal()` function — a core rule of atomic chess with extinction pseudo-royal pieces.

---

## Bug #1 (Critical): Missing Blast Adjacency Immunity in `legal()`

**File:** `src/board.rs`, method `Board::legal()`, approx. lines 717–754

**What:** When checking whether any own commoner is under attack after a move, the reference implementation (Fairy-Stockfish) *skips* the attack check if at least one **enemy commoner is adjacent** (within king-move radius) to the own commoner being checked. The rationale: capturing the own commoner would also destroy the adjacent enemy commoner in the blast (mutual destruction), so the own commoner is not meaningfully "in check."

**Reference code** (`position.cpp` lines 1184–1187):
```cpp
// Touching pseudo-royal pieces are immune
if (  !(blast_on_capture() && (pseudoRoyalsTheirs & attacks_bb<KING>(sr)))
    && (attackers_to(sr, occupied, ~us) & attackerCandidatesTheirs))
    return false;
```

**Current Rust code** (`board.rs` lines 725–751) unconditionally checks attacks:
```rust
while !c.is_empty() {
    let ksq = c.pop_lsb();
    // ... compute rook/bishop/queen/knight/commoner attackers ...
    if (...) != Bitboard::EMPTY {
        return false;  // incorrectly rejects legal moves
    }
}
```

**Fix:** Before rejecting a move because a commoner is attacked, check if any enemy commoner is adjacent (within `king_attacks(ksq)`). If so, skip the rejection for that commoner.

**Positions affected:** All positions where commoners are adjacent, which is positions #9–#12 in `perft_values.md`:
| Pos | Current depth-1 | Expected depth-1 | Delta |
|-----|-----------------|------------------|-------|
| 9   | 2               | 11               | −9    |
| 10  | 2               | 10               | −8    |
| 11  | 2               | 3                | −1    |
| 12  | 3               | 57               | −54   |

---

## Bug #2 (Related): Missing Adjacency Immunity in Castling Legality

**File:** `src/board.rs`, method `Board::legal()`, approx. lines 638–681

**What:** The pre-move castling pass-through square check doesn't apply blast adjacency immunity. The reference applies the same `!(blast_on_capture() && enemy_commoner_adjacent)` check to each pass-through square during castling.

**Reference code** (`position.cpp` lines 1138–1144):
```cpp
if (st->pseudoRoyals & from)
    for (Square s = from; from != kto ? s != kto : s == from; s += step)
        if (  !(blast_on_capture() && (attacks_bb<KING>(s) & st->pseudoRoyals & pieces(~sideToMove)))
            && attackers_to(s, occupied, ~us))
            return false;
```

**Fix:** Apply the same adjacency immunity logic when checking pass-through squares during castling.

**Positions affected:** Positions where a side has castling rights and both sides have adjacent commoners. Likely not triggered by current `perft_values.md` positions, but needed for correctness.

---

## Bug #3 (Minor): Castling Pass-Through Square Set is Wrong

**File:** `src/board.rs`, method `Board::legal()`, lines 643–653

**What:** The Rust code checks squares `[F1, G1]` for K-side castling, but the reference checks `[E1, F1]` (the starting square through the intermediate square). The king's destination square (G1) is checked later via the post-move commoner safety check, so this discrepancy is partially mitigated. However, the starting square (E1) should be checked for completeness.

**Fix:** Change the castling pass-through check to include the starting square and exclude the destination square (which is checked post-move).

**Positions affected:** Positions with castling rights where the king's starting square is under attack. Unlikely to affect current perft values, but needed for correctness.

---

## Bug #4: Tests Use Wrong FENs and Expected Values

**File:** `tests/perft_tests.rs`

**What:** The current test uses:
```rust
const POS2_FEN: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
// expects 1939 at depth 2
```

This is a *standard chess* position with *standard chess* perft values (which happen to be 1939 at depth 2). It does NOT match any of the 12 positions in `perft_values.md`. The expected values are standard chess values, not atomic chess values.

**Fix:** Replace the test FENs and expected values with the 12 positions and depths from `perft_values.md`. Add tests for all 12 positions at multiple depths where reference values are available.

---

## Bug #5 (Build Error): `Hash` Not Implemented for `Square` and `MoveType`

**File:** `src/types.rs`

**What:** The example `examples/debug_moves.rs` uses `HashSet<(Square, Square, MoveType)>`, but `Square` and `MoveType` don't implement `Hash`. This causes a build failure for that example.

**Fix:** Derive `Hash` for `Square` and `MoveType` in `src/types.rs`.

---

## Implementation Order

1. **Fix Bug #1** — Add blast adjacency immunity to the `legal()` attack check. This is the single change that fixes all perft mismatches.
2. **Fix Bug #2** — Apply adjacency immunity to castling check for correctness.
3. **Fix Bug #3** — Correct the castling pass-through squares.
4. **Fix Bug #4** — Update `tests/perft_tests.rs` with the correct FENs and expected values from `perft_values.md`.
5. **Fix Bug #5** — Derive `Hash` for `Square` and `MoveType` to fix build.

---

## Verification

After each fix, verify by running:

```bash
cargo test --lib && cargo test --test perft_tests
```

For full verification, run the example check:

```bash
cargo build --example check_perft && cargo run --example check_perft
```

Compare depth-1 values for all 12 positions. Then confirm deeper perft values match for positions where the table has values (depth 2–6).

To cross-check against the reference:

```bash
echo -e "setoption name UCI_Variant value atomic\nposition fen '<FEN>'\ngo perft <depth>" \
  | /Users/d.kretschmann/projects/dirk/golmman/Fairy-Stockfish/src/stockfish
```
