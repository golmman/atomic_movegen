# Iterative Performance Plan

## Process

Each iteration follows 6 stages, documented in a timestamped directory:

```
mkdir -p docs/perf/iter/"$(date '+%Y-%m-%dT%H-%M-%S')"
```

| Stage     | File          | Task                                                              |
|-----------|---------------|-------------------------------------------------------------------|
| Setup     | N/A           | Create the iteration directory                                    |
| Recap     | `1-recap.md`  | Summarize prior iterations' results and lessons                   |
| Analysis  | `2-perf.md`   | `hyperfine` baseline, flamegraph or `perf stat` to locate hot spots |
| Planning  | `3-plan.md`   | Document proposed changes with rationale, risks, and verification |
| Implement | `4-impl.md`   | Code changes, `cargo test`, `cargo clippy`, `cargo fmt`           |
| Report    | `5-report.md` | Re-run `hyperfine`, compare to baseline, summarize delta          |

## Tools

### Baseline measurement

```sh
cargo build --release && hyperfine \
  --warmup 3 \
  --min-runs 10 \
  --export-markdown docs/perf/iter/TIMESTAMP/hyperfine.md \
  'cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 5'
```

### Correctness check

```sh
cargo test && cargo run --release --example verify_perft 5
```

For deeper verification, run depth 6 (takes longer):

```sh
cargo run --release --example verify_perft 6
```

### Hot-spot analysis

```sh
perf record --call-graph dwarf \
  cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 5
perf report
```

### Flamegraph

```sh
perf record --call-graph dwarf --freq 1000 \
  cargo run --release --example perft "..." 5
perf script | inferno-fold | inferno-flamegraph > flame.svg
```

## Suggested iteration order

### Iteration 0 — Release profile tuning

| Risk | Impact | Effort |
|------|--------|--------|
| None | 10–25 % | 1 line |

**Problem:** `Cargo.toml` has no `[profile.release]` section. Rust defaults to
`opt-level = 3` but `lto = "thin"` and `codegen-units = 16`, which leaves
sizable performance on the table.

**Fix:** Add:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
```

`lto = "fat"` enables full cross-crate inlining even within this single-crate
project. `codegen-units = 1` prevents the thin-LTO/codegen-units heuristic
from splitting functions across codegen units, letting LLVM see the entire
module at once. Both are standard for Rust perf work.

---

### Iteration 1 — Populate `StateInfo` in `do_move()`, use in `generate_legal()`

| Risk | Impact | Effort |
|------|--------|--------|
| Low | 5–15 % | ~50 lines |

**Problem:** `StateInfo` already has fields for `checkers`, `pinned`,
`commoners_count`, and `them_commoners_count` but they are never populated.
`do_move()` only fills `castling_rights`, `ep_square`, `rule50`, and the
capture array.

`generate_legal()` creates a fresh `StateInfo::new()` and passes it to
`legal()`, so every call to `legal()` recomputes checkers/pinners from
scratch.

**Fix (4 sub-steps):**

1. At the end of `do_move()` (after `self.side_to_move` is flipped), compute
   and store checkers/pinners/counts in `state`.

2. Refactor `generate_legal()` to pre-compute checkers/pinners for the current
   position and store them in a `StateInfo`, then pass that populated
   `StateInfo` to `legal()`.

3. In `legal()`, use `state.checkers`, `state.pinned`,
   `state.commoners_count`, `state.them_commoners_count` to avoid redundant
   `attacks::*` and `self.commoners()` calls.

4. Similarly, `perft()` creates a `StateInfo` but never populates
   checkers/pinners — do so after construction so the recursive call to
   `generate_legal()` inside `perft()` gets a populated state.

**Key insight:** `legal()` does not call `Board::checkers()` or
`Board::pinned()` directly — it recomputes the same information inline.
The value of caching is avoiding 4× `self.commoners()` bitboard queries,
2× bitboard counts, and duplicated commoner-iteration logic inside the
`legal()` hot loop (lines 783–847 of `board.rs`).

---

### Iteration 2 — `blockersForKing` + safe early-out for non-capture moves

| Risk | Impact | Effort |
|------|--------|--------|
| Medium | 20–50 % | ~100 lines |

**Problem:** `legal()` performs the full attack scan for every
pseudo-legal move, even when the move is trivially safe. In standard chess,
Stockfish uses the early-out: "no checkers + not a blocker → non-king move is
legal". Atomic chess complicates this with the blast mechanic (captures, en
passant), but the early-out is safe for **non-capture, non-commoner** moves.

The current `compute_pinned()` only returns pieces that are the **sole**
blocker on a ray between a commoner and an enemy slider (the traditional
"pinned" definition). But a piece can also be **one of several** blockers
on a ray — moving it still exposes the commoner to attack. Stockfish calls
this `blockersForKing[them]` and it is a superset of `pinned`.

**Fix:**

1. Rename `compute_pinned()` → `compute_blockers_for_king()` and change it to
   return ALL pieces that lie between any commoner and any enemy slider (not
   just the sole blockers). Store as `state.blockers_for_king[us]` instead of
   `state.pinned`.

2. In `legal()`, before the full attack scan, check:

   ```
   if state.checkers.is_empty()
      && m.move_type() != Capture
      && m.move_type() != EnPassant
      && piece_on(from).type_of() != Commoner
      && !(state.blockers_for_king & Bitboard::square_bb(from))
   {
       // Self-explosion check already passed above.
       // No checkers + not a capture + not a commoner + not a blocker
       // → cannot create a discovered attack → move is legal.
       return true;
   }
   ```

   This early-out fires for the majority of moves at typical positions
   (non-captures are 60–80 % of moves, and only a few pieces are blockers).

3. Also store `state.pinned` separately (sole blockers) for future
   optimisations, but don't use it for the early-out (pinned ⊆ blockers).

**Why this failed before:** The previous attempt likely tried the early-out
for ALL moves including captures. Atomic blast can destroy attackers and
blockers simultaneously during a capture, making pre-move checkers/pins an
unreliable post-move predictor for captures.

**Why this is safe now:** By restricting to non-capture, non-commoner moves,
the blast never fires, so the post-move board state changes at most by the
moving piece. A non-capture, non-commoner piece that is not a blocker for any
commoner cannot create a new attack by moving (it was not the sole piece
obstructing any enemy slider).

---

### Iteration 3 — Dedicated queen attack table (PEXT path only)

| Risk | Impact | Effort |
|------|--------|--------|
| Low | 5–15 % (on PEXT) | ~80 lines |

**Problem:** `queen_attacks()` is defined as `bishop_attacks(sq, occ) |
rook_attacks(sq, occ)`. On the PEXT path, each of these computes a separate
PEXT instruction + table lookup. The queen's attack set is the union of both,
which can be precomputed as a single combined table.

**Fix:** Generate a dedicated queen attack table (combining bishop and rook
masks) for the PEXT path only. On the magic path, keep the `|` formulation
(smaller tables, no real bottleneck since queen attacks are already fast).

---

### Iteration 4 — Profile-guided optimisation (PGO)

| Risk | Impact | Effort |
|------|--------|--------|
| Low | 10–20 % | CI config |

**Problem:** Without PGO, LLVM's branch prediction and inlining heuristics are
based purely on static analysis, which can mispredict hot/cold paths in the
recursive perft search.

**Fix:** Add a PGO pipeline:

```sh
# Step 1: Instrument
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" \
  cargo build --release --example perft

# Step 2: Profile
./target/release/examples/perft \
  "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 5

# Step 3: Optimize
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data -Cllvm-args=-pgo-warn-missing"
  cargo build --release --example perft
```

---

### Iteration 5 — Precompute `check_squares[pt][sq]` for faster attack detection

| Risk | Impact | Effort |
|------|--------|--------|
| Medium | 5–10 % | ~60 lines |

**Problem:** `legal()` recomputes `attacks::rook_attacks(ksq, occupied)`,
`attacks::bishop_attacks(ksq, occupied)`, etc. for each pseudo-royal
commoner. These are O(1) but still involve a full magic/PEXT lookup.

**Fix:** Store `check_squares[PieceType][Square]` in `StateInfo` — the set of
squares from which each piece type would attack a commoner at `sq`. If the
attack mask is empty for a given piece type, skip the lookup entirely.

Fairy-Stockfish uses this pattern extensively (see `position.cpp` `set_check_info()`).

---

## What NOT to do (pushback notes)

| Idea | Why to skip |
|------|-------------|
| `unsafe` for `Vec::set_len` in move list | The move list is `Vec<Move>` with capacity 256. The `retain()` closure is fast enough; `unsafe` adds risk for <1 % gain. |
| Arena allocator for `StateInfo` | `StateInfo` is ~100 bytes on the stack, created once per position in the perft recursion. Stack allocation is already free (just a `sub rsp`). |
| SIMD for blast-zone computation | The blast zone is at most 9 squares. SIMD setup overhead dwarfs the loop. |
| Threading / parallel perft | Perft trees are trivially parallel, but that's a different benchmark (parallel speedup). Not relevant to the single-threaded perft benchmark. |
| Early-out for ALL moves (including captures) | The previous attempt failed because atomic blast invalidates pre-move invariants. Even if it could be made correct, the logic complexity isn't worth the risk. |
| Remove magic tables unconditionally | Magic tables are the ARM/non-BMI2 fallback. They can only be removed if we mandate BMI2 (not practical for a portable library). |

## Verification checklist (every iteration)

- [ ] `cargo test` — all unit tests pass
- [ ] `cargo clippy` — no new warnings
- [ ] `cargo run --release --example verify_perft 5` — all 41 positions match
- [ ] If the change is risk, also: `cargo run --release --example verify_perft 6`
- [ ] Baseline and final `hyperfine` runs use identical command-line args
- [ ] Results saved to the iteration directory before making changes
