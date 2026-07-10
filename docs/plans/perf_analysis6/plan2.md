# Plan 2 — Cache Pseudo-Royal Commoner Bitboards and Add a Capture Blast Pre-Filter

**Corresponds to:** Item 2 of `docs/plans/perf_analysis6/analysis.md` — *Cache `pseudoRoyals` bitboards and add a capture blast-illegal pre-filter*
**Estimated speedup:** 3–7 % on `cargo run --release --example verify_perft`
**Risk:** Medium
**Effort:** ~60–90 lines changed across `src/board.rs`, `src/movegen.rs`, `src/types.rs`

---

## 1. Problem

`Board::legal` is the second hottest symbol in the profile (21.7 % of sampled time). Inside it, the most expensive work is rebuilding the post-move `our_commoners` and `them_commoners` bitboards on every call:

- `legal` calls `self.commoners(us)` and `self.commoners(them)` multiple times.
- `self.commoners(c)` is `pieces_color_pt(c, PieceType::Commoner)`, which does two `Bitboard` loads and an `&` every time.
- `generate_legal` calls `Board::legal` for every capture, and in atomic chess many captures are illegal because the 3×3 blast zone destroys the side's last commoner.

`StateInfo` currently only stores the *counts* of commoners (`commoners_count`, `them_commoners_count`), not the actual bitboards. Recomputing the bitboards repeatedly in `compute_checkers`, `compute_pinned`, and `legal` is wasted work.

---

## 2. Goal

1. Store full `our_commoners` and `them_commoners` bitboards in `StateInfo`.
2. Compute them once in `Board::populate_state` and reuse them in `compute_checkers`, `compute_pinned`, and `Board::legal`.
3. Add a cheap, correct *capture blast pre-filter* in `generate_legal` that skips the full `Board::legal` call for captures that obviously destroy all of the side's own commoners.
4. Keep the exact atomic-chess semantics unchanged (pseudo-royal last-commoner rules, adjacency immunity, self-explosion, castling pass-through).

---

## 3. Design

### 3.1 Extend `StateInfo`

Add two `Bitboard` fields next to the existing counts:

```rust
pub struct StateInfo {
    /// Enemy pieces attacking the side to move's commoners (hot).
    pub checkers: Bitboard,
    /// Pieces of the side to move that are pinned to a commoner (hot).
    pub pinned: Bitboard,
    /// Commoners of the side to move (pre-move position).
    pub our_commoners: Bitboard,
    /// Commoners of the opponent (pre-move position).
    pub them_commoners: Bitboard,
    /// Number of commoners the side to move has on the board (hot).
    pub commoners_count: u32,
    /// Number of commoners the opponent has on the board (hot).
    pub them_commoners_count: u32,

    // ... undo data unchanged
}
```

`StateInfo::new()` and `Default` initialize the new fields to `Bitboard::EMPTY`.

### 3.2 `populate_state` computes the bitboards once

```rust
pub fn populate_state(&self, state: &mut StateInfo) {
    let us = self.side_to_move;
    let them = us.flip();

    state.our_commoners = self.commoners(us);
    state.them_commoners = self.commoners(them);
    state.commoners_count = state.our_commoners.count();
    state.them_commoners_count = state.them_commoners.count();

    state.checkers = self.compute_checkers(us, state.our_commoners, state.them_commoners);
    state.pinned = self.compute_pinned(us, state.our_commoners);
}
```

### 3.3 `compute_checkers` and `compute_pinned` take cached commoners

Change the signatures to accept the commoner bitboards so they do not recompute `self.commoners()` internally:

```rust
pub(crate) fn compute_checkers(
    &self,
    us: Color,
    our_commoners: Bitboard,
    them_commoners: Bitboard,
) -> Bitboard;

pub(crate) fn compute_pinned(&self, us: Color, our_commoners: Bitboard) -> Bitboard;
```

The bodies stay the same except:

- `compute_checkers` uses the passed `our_commoners` for the main loop and `them_commoners` for the adjacency check.
- `compute_pinned` uses the passed `our_commoners` for the sniper discovery loop.

The public wrappers `Board::checkers()` and `Board::pinned(c)` compute the required bitboards with `self.commoners(...)` and call the new internal helpers.

### 3.4 `Board::legal` uses `StateInfo` commoner bitboards

In `Board::legal` (`src/board.rs` lines 962–1058), replace every `self.commoners(us)` / `self.commoners(them)` with the pre-computed `state.our_commoners` / `state.them_commoners`:

```rust
// Castling pass-through check
let adjacent_enemy_commoners = state.them_commoners & attacks::king_attacks(sq);
// ...

// Post-blast commoners
let mut our_commoners = state.our_commoners & occupied;
if !is_capture && piece == make_piece(us, PieceType::Commoner) {
    our_commoners = our_commoners | Bitboard::square_bb(kto);
}

// ...
let them_commoners = state.them_commoners;
let enemy_survivors = self.by_color[them as usize] & occupied;

let mut c = our_commoners;
while !c.is_empty() {
    let ksq = c.pop_lsb();
    let adjacent_enemy = them_commoners & attacks::king_attacks(ksq);
    if adjacent_enemy.is_empty()
        && attackers_to(self, ksq, occupied, enemy_survivors) != Bitboard::EMPTY
    {
        return false;
    }
}

// Enemy pseudo-royal destroyed
let enemy_pr_destroyed =
    state.them_commoners_count <= 1 && (state.them_commoners & occupied).is_empty();
```

`state.our_commoners` and `state.them_commoners` are the *pre-move* bitboards. `& occupied` gives the post-move survivors. The non-capture commoner-move case still adds `kto` explicitly because the commoner moved from an empty target square.

### 3.5 Capture blast-illegal pre-filter in `generate_legal`

Before the `is_move_trivially_legal || board.legal` check, add a fast reject for capture moves.

A capture in atomic chess is a blast centered on `to`. Any of our commoners inside `attacks::king_attacks(to)` — including the `to` square itself — is destroyed. If the moving piece is itself a commoner, it also leaves `from`. Therefore the surviving commoners are:

```rust
survivors = (state.our_commoners & !Bitboard::square_bb(from)) & !attacks::king_attacks(to)
```

If `survivors` is empty, the move is self-explosion and is illegal. We can skip the full `legal()` call.

In `generate_legal`:

```rust
pub fn generate_legal(board: &Board, moves: &mut MoveList) {
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    generate_pseudo_legal(board, moves);

    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            let from = m.from_sq();
            let to = m.to_sq();
            let mt = m.move_type();
            let is_capture =
                mt != MoveType::Castling && (mt == MoveType::EnPassant || board.piece_on(to) != NO_PIECE);

            // Fast reject: capture blast destroys all our commoners.
            if is_capture {
                let from_bb = Bitboard::square_bb(from);
                let survivors = (state.our_commoners & !from_bb) & !attacks::king_attacks(to);
                if survivors.is_empty() {
                    continue;
                }
            }

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

This mirrors the `is_capture` logic used in `Board::legal` and is valid for normal captures, en-passant, and promotion captures.

---

## 4. Implementation Steps

### Step 1: Record baseline

Run the current `main` and record the exact `verify_perft` time:

```sh
cargo run --release --example verify_perft
```

Reference (after Plan 1): `report1.md` reports `53.926 s` with `41/41` passed. Record the exact number on the current machine.

### Step 2: Add `our_commoners` / `them_commoners` to `StateInfo`

In `src/board.rs` lines 90–116 (where `StateInfo` is defined):

- Add `pub our_commoners: Bitboard` and `pub them_commoners: Bitboard`.
- Update `StateInfo::new()` and `Default` to initialize them to `Bitboard::EMPTY`.
- Add doc comments to satisfy `#![warn(missing_docs)]`.

### Step 3: Update `populate_state`

In `src/board.rs` `populate_state`:

- Compute `our_commoners` and `them_commoners` first.
- Derive the counts from the bitboards.
- Pass the bitboards to `compute_checkers` and `compute_pinned`.

### Step 4: Update `compute_checkers` and `compute_pinned` signatures

In `src/board.rs`:

- `compute_checkers(&self, us: Color, our_commoners: Bitboard, them_commoners: Bitboard) -> Bitboard`
- `compute_pinned(&self, us: Color, our_commoners: Bitboard) -> Bitboard`

Replace internal `let commoners = self.commoners(...)` loads with the parameters.

### Step 5: Update public `checkers()` and `pinned()` wrappers

In `src/board.rs`:

- `checkers()` computes `our_commoners`/`them_commoners` and calls `compute_checkers`.
- `pinned(c)` computes `self.commoners(c)` and calls `compute_pinned`.

### Step 6: Update `Board::legal` to use cached bitboards

In `src/board.rs` `legal`:

- Replace `self.commoners(us)` with `state.our_commoners`.
- Replace `self.commoners(them)` with `state.them_commoners`.
- Keep the `& occupied` and `| kto` adjustments exactly as they are today.
- Keep `adjacent_enemy` using pre-move `them_commoners` (do not filter by `occupied` here).
- Use `state.them_commoners & occupied` for `enemy_pr_destroyed`.

### Step 7: Add the capture blast pre-filter to `generate_legal`

In `src/movegen.rs` `generate_legal`:

- Compute `is_capture` per move the same way as `Board::legal`.
- Add the `survivors` fast reject.
- Keep the existing `is_move_trivially_legal || board.legal` filter.

### Step 8: Build, test, lint, document

Run in order:

```sh
cargo build
cargo test
cargo clippy
cargo fmt
cargo doc
```

All must pass without warnings.

### Step 9: Performance verification

```sh
cargo run --release --example verify_perft
```

Record:

- Total time and per-test times.
- Compare to baseline.
- Optionally run the profiled FEN:
  ```sh
  cargo run --release --example perft "r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15" 6
  ```

Ensure 41/41 positions pass.

### Step 10: Write `docs/plans/perf_analysis6/report2.md`

Document the implementation and create a hand-off report for Plan 3. Required sections:

- **Summary** — what was changed and the measured effect.
- **Baseline** — exact `verify_perft` time before the change.
- **Result** — exact time after and speedup/deg percentage.
- **Implementation notes** — why the bitboards were added, how the pre-filter works, and why the `adjacent_enemy`/`enemy_pr_destroyed` split was kept.
- **Problems, surprises, and workarounds** — e.g. `const`/bitboard operator issues, `!Bitboard` precedence, interaction with `MoveType::Promotion`, `EnPassant`, castling, or any correctness test failures.
- **Files changed** — list of files and the nature of the change.
- **Verification results** — `cargo test`, `cargo clippy`, `cargo run --release --example verify_perft` outcomes.
- **Notes for Plan 3** — state of `StateInfo`, whether the `EVASIONS`/`NON_EVASIONS` split (Item 3) is now easier, `is_square_attacked` bool (Item 4), bulk pawn generation (Item 5), etc.

---

## 5. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/board.rs` | Add `our_commoners`/`them_commoners` to `StateInfo`; update `populate_state`; change `compute_checkers`/`compute_pinned` signatures; update `checkers`/`pinned` wrappers; use `state.our_commoners`/`state.them_commoners` in `legal`. | ~50 |
| `src/movegen.rs` | Add capture blast pre-filter in `generate_legal`. | ~15 |
| `src/types.rs` | No change unless `StateInfo` is moved there; `Bitboard` already supports `Copy`/`Clone`. | ~0 |

---

## 6. Correctness Verification

- `cargo test` — all unit tests, including `board.rs` self-explosion / blast tests, `movegen.rs` tests, and `magic.rs` tests.
- `cargo run --release --example verify_perft` — 41/41 positions at depths 1–6 must pass.
- `cargo clippy` must be clean.
- `cargo fmt` and `cargo doc` must be clean.

---

## 7. Expected Impact

- `Board::legal` no longer recomputes `commoners` bitboards from scratch on every call.
- `compute_checkers` and `compute_pinned` no longer recompute `commoners`.
- A significant fraction of capture moves will be rejected by the cheap `survivors` pre-filter, avoiding the expensive `attackers_to`/`legal` path.
- Estimated 3–7 % total `verify_perft` speedup, with the biggest gains on capture-heavy and single-commoner positions.

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| `adjacent_enemy` semantics accidentally changed by filtering `them_commoners` with `occupied` | Medium | Keep `adjacent_enemy` using pre-move `state.them_commoners`; only use `& occupied` for `enemy_pr_destroyed` and `our_commoners` post-blast filtering. |
| Pre-filter rejects a capture that is actually legal | Medium | The `survivors` formula is exactly the same bitboard expression used inside `legal` for post-blast `our_commoners`; verify with the self-explosion tests in `board.rs` and `verify_perft`. |
| `StateInfo` size growth hurts cache / stack | Low | `StateInfo` is already two `u64`s larger than before; it is tiny and stack-allocated per `perft` recursion frame. |
| Measured speedup is less than expected | Medium | The pre-filter wins depend on how many capture moves single-commoner positions generate. If lower, the bitboard cache still removes repeated `commoners()` calls. |

---

## 9. Notes for Plan 3

After this plan:

- `StateInfo` contains full `our_commoners`/`them_commoners` bitboards, which will also be needed for the `EVASIONS`/`NON_EVASIONS` split (Item 3) and for a future `is_square_attacked` bool early-exit (Item 4).
- The capture pre-filter may make some capture-heavy positions faster but does not change the overall `generate_legal` structure, so the `EVASIONS` split remains a clean next step.
- `compute_checkers` and `compute_pinned` already accept `our_commoners`/`them_commoners`; any further fusion or occupancy-delta optimization (Items 7/8) can build on these signatures.
