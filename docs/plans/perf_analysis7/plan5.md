# Plan 5 — Order-Preserving, Table-Driven Pawn Generation

**Corresponds to:** Item 6 of `docs/plans/perf_analysis7/analysis.md` — §5.5,
*Bulk pawn generation, but only with an ordering plan* (specifically **option 3** of that section)
**Estimated speedup:** ~1–2.5 % on the profiled positions (analysis §5.5 estimates ~2–4 % for
order-breaking bulk generation; the order-preserving variant captures part of that)
**Risk:** Low–medium — pawn generation is the only move-generation stage touched, move order is
pinned by `tests/move_order_golden.rs`, and the change is confined to `src/movegen.rs` plus two
crate-private tables in `src/attacks.rs`
**Effort:** Small–medium — roughly 60–90 changed lines in `src/movegen.rs`, ~50 lines of
compile-time tables plus two accessors in `src/attacks.rs`

---

## 1. Context

Plans 1–4 covered analysis §8 items 1–5:

| Plan | Item | Result |
|------|------|--------|
| 1 (`report1.md`) | §5.1 perft fast path without compaction | Reverted — regressed ~3 % on #14 |
| 2 (`report2.md`) | §5.2 local bitboard cache in `Board::legal` | Reverted — stable ~+0.2–0.9 % on #13 |
| 3 (`report3.md`) | §5.4 boolean `square_attacked_by` early exit | **Kept** |
| 4 (`report4.md`) | §5.3 category-specialized legality | Reverted — ≤0.6 % on #13, tree restored |

Only Plan 3 is in the tree. `git log` confirms the current tree is the post-Plan-4 state and
`git diff` is clean.

The next item in analysis §8 is item 6, §5.5 *bulk pawn generation, but only with an ordering
plan*. That section lists three options:

1. a perft-only bulk pawn generator, leaving public move generation unchanged;
2. changing public move order intentionally and updating the goldens;
3. keeping per-pawn iteration but making the per-pawn target computation cheaper.

This plan chooses **option 3** and explicitly defers options 1 and 2. Reasons:

- **Option 2 needs a product decision, not an optimization decision.** `tests/move_order_golden.rs`
  documents that "consumers depend on generation order for move ordering and reproducible
  searches". Changing public order requires an explicit maintainer decision that ordering is not
  part of the API contract. That decision is not made here, and this plan must not make it
  implicitly.
- **Option 1 depends on a reverted change.** `perft` currently uses `generate_legal`
  (`src/lib.rs:65`). A perft-only bulk generator would require `perft` to run its own
  pseudo-legal stream, i.e. the same perft shape that Plan 1 measured and reverted (§5.1,
  `report1.md`: neutral on #13, ~3 % regression on #14). Building option 1 on top of a reverted
  baseline would confound two effects and violate the attribution rules used in Plans 1–4.
- **Option 3 is the only variant that is safe, attributable, and does not touch any pinned
  behavior.** It keeps the exact candidate stream and attacks the measured cost directly.

The measured target is large: `generate_pawn_moves_for` is ~15.8 % of child time on test #13
(analysis §3.2), pawn/piece generation shows as 15.0 % / 16.2 % of source-line samples (§3.3), and
the pawn-specific source lines `movegen.rs:179` (en-passant square check) and `movegen.rs:155`
(pawn capture file check) alone account for ~2.1 % / ~2.3 % of samples. Analysis §3.4 calls pawn
generation out explicitly as the check-heavy cost centre.

`report4.md` closes with the recommendation that a fresh profile and same-session fixed-FEN
baseline precede any next item, and that layout sensitivity has repeatedly inverted small expected
gains on this host. This plan therefore starts with a re-baseline (Step 1) and treats the
expected gain as an upper bound.

---

## 2. Problem

`generate_pawn_moves_for` (`src/movegen.rs:119`) is called once per pawn from
`for_each_pseudo_legal`. For each pawn it does:

```rust
let from_rank = rank_of(from);           // rank table lookup
let from_f = file_of(from) as i8;        // file table lookup
let (push_dir, push_double, start_rank, promo_rank) = match us { ... };
let from_idx = from as i8;

// Single push
let to_idx = from_idx + push_dir;
let to_sq = Square::from_index(to_idx);  // bounds-checked square table lookup
if to_sq != Square::NONE && board.empty(to_sq) {
    ...
    // Double push
    if from_rank == start_rank {
        let to_idx2 = from_idx + push_double;
        let to_sq2 = Square::from_index(to_idx2);
        if to_sq2 != Square::NONE && board.empty(to_sq2) { ... }
    }
}

// Captures - adjacent files only
for df in &[-1i8, 1i8] {
    let target_f = from_f + df;
    if !(0..=7).contains(&target_f) { continue; }
    let to_idx = from_idx + push_dir + df;
    let to_sq = Square::from_index(to_idx);
    if to_sq == Square::NONE { continue; }
    if file_of(to_sq) as i8 != target_f { continue; }   // wrap guard
    if board.pieces_color(them) & Bitboard::square_bb(to_sq) != Bitboard::EMPTY {
        ...
    }
}

// En passant
if let Some(ep_sq) = board.ep_square() { ... file/rank arithmetic ... }
```

Every per-pawn call pays for:

- two `file_of` / `rank_of` table loads plus a bounds-checked `Square::from_index`;
- a per-pawn `match us` for the direction tuple;
- **two** freshly allocated capture-direction iterations with range and wrap guards, even though
  the crate already has a `PAWN_ATTACKS` table (`src/attacks.rs:139`, exposed as
  `attacks::pawn_attacks`) that encodes exactly those targets without wrap hazards;
- a `board.empty(...)` scalar array probe per push instead of a bitboard intersection, and a
  `board.pieces_color(them) & Bitboard::square_bb(to)` intersection rebuilt per capture target;
- a repeated `board.ep_square()` read per pawn, and an arithmetic re-derivation of the en-passant
  capture square.

The capture loop is the worst offender: analysis §3.3 attributes ~0.85–1.07 % of samples to
`movegen.rs:155` (the `file_of(to_sq) == target_f` wrap guard) and ~1.26 % to `movegen.rs:179`
(the en-passant square check), both of which are pure bookkeeping that a table lookup removes.

This plan replaces that arithmetic with compile-time bitboard tables, **without changing the
emitted move sequence in any way**.

### 2.1 Why a full bitboard bulk generator is not part of this plan

Fairy-Stockfish generates all pawn pushes and captures with whole-set bitboard shifts
(`movegen.cpp:135–198`). A direct port produces targets grouped by target set (all single
pushes, then all double pushes, then all left captures, …), which is **not** the current
`atomic-movegen` order. The current order is per pawn, in ascending origin-square order, and per
pawn: push (or 4 promotions) → double push → west capture → east capture → en passant:

```text
a2a3 a2a4 b2b3 b2b4 ... (startpos golden)
b7b8q b7b8r b7b8b b7b8n b7a8q b7a8r b7a8b b7a8n b7c8q b7c8r b7c8b b7c8n (promotion golden)
e4e3 e4d3 ... (en_passant golden)
```

Matching that order from a bulk stream would require re-sorting by origin square and category,
which gives back most of the saving. Option 3 (table-driven, per-pawn iteration) keeps the order
byte-for-byte and still removes the table lookups, the wrap guards, and the per-target bitboard
rebuilding.

---

## 3. Goal

1. Add two crate-private compile-time tables to `src/attacks.rs`, next to the existing
   `PAWN_ATTACKS` table:

   - `PAWN_PUSHES[color][sq] -> Bitboard`: the single-push target square, or empty when the pawn
     cannot push (last rank).
   - `PAWN_DOUBLE_PUSHES[color][sq] -> Bitboard`: the double-push target, or empty when the pawn
     is not on its start rank.

   Expose them as `pub(crate)` accessors `pawn_pushes(c, sq)` and `pawn_double_pushes(c, sq)`,
   mirroring `pawn_attacks`.

2. Rewrite the body of `generate_pawn_moves_for` to use
   `pawn_pushes` / `pawn_double_pushes` / `pawn_attacks` with bitboard intersections against
   occupancy and enemy occupancy, instead of file/rank arithmetic and wrap guards.

3. Pass the per-node bitboards (`occupied`, enemy occupancy) and the en-passant square from
   `for_each_pseudo_legal` into `generate_pawn_moves_for`, so they are computed once per node
   rather than once per pawn.

4. Emit exactly the same moves in exactly the same order. `tests/move_order_golden.rs` must pass
   **without changes to its existing goldens**, and `perft` node counts must be identical.

No public API change, no change to `Board`, no change to `perft`, no change to move ordering.

---

## 4. Design

### 4.1 New tables in `src/attacks.rs`

Follow the existing compile-time-table style (`compute_king_attacks`, `compute_pawn_attacks`),
with the same square/color indexing convention as `PAWN_ATTACKS` (indexed by
`[square][color]`):

```rust
/// Compute single-push targets for all 64 squares at compile time.
/// Returns a 2D array indexed by `[square][color]`; entries are empty for
/// pawns that cannot push (a pawn on its last rank).
const fn compute_pawn_pushes() -> [[Bitboard; 2]; 64] { ... }

/// Compute double-push targets for all 64 squares at compile time.
/// Returns a 2D array indexed by `[square][color]`; only start-rank squares
/// have a target, all other entries are empty.
const fn compute_pawn_double_pushes() -> [[Bitboard; 2]; 64] { ... }
```

Accessors, placed directly after `pawn_attacks`:

```rust
/// Return the single-push target bitboard for a pawn of the given color.
#[inline(always)]
pub(crate) fn pawn_pushes(c: Color, sq: Square) -> Bitboard { ... }

/// Return the double-push target bitboard for a pawn of the given color.
///
/// Only start-rank squares have a target; all other squares return
/// [`Bitboard::EMPTY`].
#[inline(always)]
pub(crate) fn pawn_double_pushes(c: Color, sq: Square) -> Bitboard { ... }
```

Implementation notes:

- `pawn_pushes`: white target is `sq + 8` when `rank < 7`; black target is `sq - 8` when
  `rank > 0`. `pawn_double_pushes`: white `sq + 16` when `rank == 1`; black `sq - 16` when
  `rank == 6`.
- Use `#[inline(always)]` accessors like the existing ones so the tables inline into the
  movegen loop.
- Add unit tests in `attacks.rs`'s `mod tests` (there is already a pawn-attack test there)
  checking: a start-rank white pawn has exactly one push (`d2d3`) and one double push (`d2d4`);
  a black start-rank pawn likewise (`e7e6`, `e7e5`); a pawn on the seventh rank has a push but no
  double push; a pawn on the second-to-last rank for its color (rank 7 white, rank 2 black) has a
  push; and `pawn_pushes`/`pawn_double_pushes` are empty on the promotion/impossible ranks.

### 4.2 Rewritten `generate_pawn_moves_for`

Signature change (the function is private to `movegen.rs`; no `pub` surface changes):

```rust
fn generate_pawn_moves_for<B>(
    board: &Board,
    us: Color,
    them: Color,
    occupied: Bitboard,
    them_bb: Bitboard,
    ep_square: Option<Square>,
    from: Square,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> {
    let from_rank = rank_of(from);
    let (start_rank, promo_from_rank) = match us {
        Color::White => (Rank::R2, Rank::R7),
        Color::Black => (Rank::R7, Rank::R2),
    };
    let promoting = from_rank == promo_from_rank;

    // Single push (a promotion push expands to four promotion moves).
    let single = attacks::pawn_pushes(us, from) & !occupied;
    if !single.is_empty() {
        let to_sq = single.lsb();
        if promoting {
            for &pt in &PROMOTION_PIECES {
                f(Move::make_promotion(from, to_sq, pt))?;
            }
        } else {
            f(Move::make_move(from, to_sq))?;
        }

        // Double push: only from the start rank, and only when the
        // intermediate square is empty (implied by `!single.is_empty()`).
        if from_rank == start_rank {
            let double = attacks::pawn_double_pushes(us, from) & !occupied;
            if !double.is_empty() {
                f(Move::make_move(from, double.lsb()))?;
            }
        }
    }

    // Captures. Pop order is ascending target-square index, which is the
    // current order: west capture (file - 1) before east capture (file + 1).
    let mut captures = attacks::pawn_attacks(us, from) & them_bb;
    while !captures.is_empty() {
        let to_sq = captures.pop_lsb();
        if promoting {
            for &pt in &PROMOTION_PIECES {
                f(Move::make_promotion(from, to_sq, pt))?;
            }
        } else {
            f(Move::make_move(from, to_sq))?;
        }
    }

    // En passant.
    if let Some(ep_sq) = ep_square {
        if !(attacks::pawn_attacks(us, from) & Bitboard::square_bb(ep_sq)).is_empty() {
            let ep_cap = match us {
                Color::White => Square::from_index(ep_sq as i8 - 8),
                Color::Black => Square::from_index(ep_sq as i8 + 8),
            };
            if board.piece_on(ep_cap) == make_piece(them, PieceType::Pawn) {
                f(Move::make_enpassant(from, ep_sq))?;
            }
        }
    }

    ControlFlow::Continue(())
}
```

Implementation constraints:

- Keep the stage order **single/double push → captures → en passant** and the inner order
  **promotion quartet in `PROMOTION_PIECES` order**.
- Keep using `PROMOTION_PIECES`; do not inline or reorder it.
- Do not add a `board.empty(...)` fallback: the occupancy bitboard is authoritative. `by_color`
  maintenance is already exercised by `verify_perft` and `verify_moves`.
- The en-passant square comparison changes from "compute `to_sq` and test equality with `ep_sq`"
  to "test `pawn_attacks(us, from)` membership of `ep_sq`". These are equivalent: a pawn can
  capture en passant exactly when the en-passant square is one of its attack targets. Keep the
  existing enemy-pawn-on-the-capture-square check unchanged; it is part of the contract.
- The `promoting` flag is computed once per pawn from the origin rank, replacing the per-target
  `rank_of(to_sq) == promo_rank` tests. This is equivalent because every pawn target of a pawn on
  its seventh rank is on the promotion rank, and a pawn on any other rank never promotes.

### 4.3 Driver change in `for_each_pseudo_legal`

`for_each_pseudo_legal` already computes `occupied` and `target` once per node. Hoist the enemy
occupancy and en-passant square once and pass them down:

```rust
let us = board.side_to_move();
let them = us.flip();
let occupied = board.occupied();
let empty = !occupied;
let them_bb = board.pieces_color(them);
let ep_square = board.ep_square();
let target = empty | them_bb;

let mut p = board.pieces_color_pt(us, PieceType::Pawn);
while !p.is_empty() {
    let from = p.pop_lsb();
    generate_pawn_moves_for(board, us, them, occupied, them_bb, ep_square, from, f)?;
}
```

The rest of `for_each_pseudo_legal` (knights → bishops → rooks → queens → commoners → castling)
and `generate_piece_moves` stay exactly as they are.

`has_legal_move_with_state` uses `for_each_pseudo_legal` and therefore picks up the same path; do
not special-case it.

### 4.4 Order-preservation argument (the acceptance contract)

The emitted order is unchanged because:

1. The outer loop still iterates the side's pawns with `pop_lsb` on
   `pieces_color_pt(us, Pawn)`, i.e. ascending origin square. Unchanged.
2. For each pawn the stages are emitted in the same sequence: single push (or its four promotions),
   then double push, then captures, then en passant. Unchanged.
3. `attacks::pawn_attacks` sets exactly the two capture targets that the old `for df in [-1, 1]`
   loop produced, and `pop_lsb` visits them in ascending square index, which is `df = -1` (west)
   then `df = +1` (east) for **both** colours. This is the reason the table version preserves the
   old `df` ordering without an explicit direction loop; it must be verified by the golden test
   for both colours (the `startpos` golden covers white, the `en_passant` and `promotion` goldens
   cover black, `middlegame` covers both).
4. The double push is still emitted only when the single push succeeds, and only from the start
   rank.
5. En passant is still emitted last, only for an adjacent-file pawn with an enemy pawn on the
   captured square.

Phrased as a contract: for every position,

```text
for_each_pseudo_legal(board, f) visits exactly the same move sequence as before this change
```

and, as a corollary, `generate_legal` returns the same list in the same order.

### 4.5 What not to change

- No change to `Board`, `Board::legal`, `is_move_trivially_legal`, `populate_state`,
  `compute_checkers`, `compute_pinned`, `do_move`/`undo_move`, or `StateInfo`.
- No change to `src/lib.rs::perft` (Plan 1 / §5.1 stays reverted until there is new evidence).
- No change to `generate_piece_moves`, `generate_castling`, `generate_legal_with_state`,
  `has_legal_move_with_state`, or `MoveList`.
- No change to the existing goldens in `tests/move_order_golden.rs`.
- No `unsafe`, no new dependencies, no lazy initialization.
- No `StateInfo` fields and no capture pre-filter (that combination regressed in
  `perf_analysis6/report2.md` and is explicitly out of scope here).
- Do not bundle the §5.6 commoner cache, §5.7 pinned rewrite, or §5.8 occupied cache into this
  change.

---

## 5. Implementation Steps

### Step 1: Re-baseline on this host and lock the pawn-stream goldens

Absolute timings drifted materially between Plans 1–4 (analysis baseline ~8.5 s for #13 vs
~11.3 s in `report4.md`). Re-measure before editing and use only same-session comparisons.

```sh
cargo build --release --examples

F13='rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5'
F14='r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15'

for i in $(seq 1 7); do
  start=$(date +%s%N); target/release/examples/perft "$F13" 6; end=$(date +%s%N)
  echo "#13 run $i: $(( (end - start) / 1000000 )) ms"
done
for i in $(seq 1 7); do
  start=$(date +%s%N); target/release/examples/perft "$F14" 6; end=$(date +%s%N)
  echo "#14 run $i: $(( (end - start) / 1000000 )) ms"
done
```

Record all runs, not only the median. Confirm node counts are unchanged from earlier plans:

- #13: `2,160,817,389`
- #14: `245,931,633`

Also record a full-suite baseline for context only:

```sh
cargo run --release --example verify_perft
```

**Before touching `src/movegen.rs`**, extend `tests/move_order_golden.rs` with pawn-rich positions
whose expected streams are captured from the *current, unmodified* implementation, then confirm
the test passes on the baseline tree. Suggested additions:

- the #13 FEN (`rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5`) — black to
  move, several pawns, captures and pushes;
- the #14 FEN (`r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15`) — black pawns with
  mixed capture options;
- one hand-built position with a promotion capture and a black en passant, e.g.
  `8/8/8/1pP5/8/8/1k6/4K3 w - b6 0 1` (it parses and produces `c5b6` en passant plus `c5c6`).

Capture the expected pseudo-legal and legal UCI strings from the **unmodified** tree — e.g. with a
throwaway `examples/` binary or by temporarily printing the streams in the golden test itself
(`list_moves` prints legal moves only) — paste them into `GOLDENS`, and confirm
`cargo test --test move_order_golden` is green **before** the pawn-generation rewrite. Those
goldens are then the primary guard for this plan. Do not regenerate them after the change; if the
post-change run differs, the change is wrong.

Optionally capture a fresh profile of the current tree so the pawn share can be compared after
the change:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
perf record -F 997 -g --call-graph dwarf -o /tmp/perf7-plan5-baseline.data \
  target/profiling/examples/perft "$F13" 6
perf report -i /tmp/perf7-plan5-baseline.data --stdio --children --sort=symbol
```

This is context, not an acceptance criterion.

### Step 2: Add the pawn push tables

Implement `compute_pawn_pushes`, `compute_pawn_double_pushes`, and the `pub(crate)` accessors in
`src/attacks.rs` per §4.1, plus the unit tests. Build and run `cargo test` — nothing else depends
on the tables yet, so this step must be a no-op for behaviour.

### Step 3: Rewrite pawn generation

Apply §4.2 and §4.3 in `src/movegen.rs`:

- change `generate_pawn_moves_for`'s signature and body;
- update the single call site in `for_each_pseudo_legal` and hoist `them_bb` / `ep_square`;
- delete any now-unused imports or locals (`file_of` may become unused in `movegen.rs`; if
  `rank_of` survives only for the start-rank/promotion checks, keep it — do **not** silence an
  unused-import warning with `#[allow]`, remove the import).

Keep the diff confined to `src/attacks.rs` and `src/movegen.rs`.

### Step 4: Correctness checks

```sh
cargo fmt
cargo clippy --all-targets
cargo test
cargo doc
cargo run --release --example verify_perft
```

Required results:

- `cargo clippy --all-targets` has no new warnings (the pre-existing `manual_range_contains` /
  `manual_contains` warnings in `examples/test_d5g8.rs` are unrelated).
- All unit, integration, golden-order, and doc tests pass.
- **`tests/move_order_golden.rs` passes with the goldens captured in Step 1, unchanged.** This is
  the strongest single check in this plan.
- `verify_perft` reports **41/41** at depths 1–6.
- `cargo run --example verify_moves` / `cargo test --test verify_moves` stays green.

If any pawn move is emitted in a different order, stop and fix the ordering rather than updating
goldens.

### Step 5: Performance verification

Rebuild and repeat the Step 1 fixed-FEN loops in the same session.

```sh
cargo build --release --examples
# re-run the #13 and #14 loops
```

Use all seven runs. If the result is close to the noise floor, do a second round with an A/B
restore: re-measure baseline, reapply the patch, re-measure, and compare pairwise per run (this is
what `report2.md` and `report4.md` did to get a usable signal).

Acceptance criteria:

- Test #13 median improves by **at least ~1 %**, and the improvement is consistent across runs
  (no stable regression on #14 beyond noise; an improvement on #14 is a bonus).
- The full suite passes 41/41 and does not show a material slowdown; the full-suite total is
  supporting evidence only.
- If the gain is below ~0.5 % or mixed, revert `src/movegen.rs` and `src/attacks.rs` to the Plan 3
  tree, rebuild, confirm the baseline is restored, and record the negative result.

If the change is kept, optionally re-profile to confirm the pawn share dropped and that
`movegen.rs:155` / `movegen.rs:179` no longer appear as hot source lines.

### Step 6: Write the implementation report

Create `docs/plans/perf_analysis7/report5.md` as required by `AGENTS.md`. Include:

- Summary of the new tables, the rewritten `generate_pawn_moves_for`, and the driver change;
  confirmation that only `src/attacks.rs` and `src/movegen.rs` changed (`git diff --stat`).
- Baseline and post-change fixed-FEN #13/#14 timings, all runs plus medians, pairwise comparison
  of the A/B round, and the full-suite result.
- Whether the expected ~1–2.5 % reproduced, and how the measured gain compares with analysis §5.5's
  ~2–4 % estimate for order-breaking bulk generation.
- Correctness: `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo doc`,
  `verify_perft` 41/41, `verify_moves`, and the new `move_order_golden` positions.
- Explicit keep/revert decision and reason.
- If kept: profile evidence for the pawn-cost drop and confirmation that the rollback of Plan 1 was
  not disturbed (`perft` unchanged).
- If reverted: the negative result, whether the remaining pawn cost justifies revisiting §5.5
  options 1/2, and a recommendation for the maintainer's move-order policy decision.
- Next candidate from analysis §8, namely §5.6 (`StateInfo` commoner cache) or
  §5.7/§5.9 (`compute_pinned` with `slider_blockers` and precomputed empty-board attacks), plus
  whether a fresh profile says otherwise.

---

## 6. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/attacks.rs` | Add `PAWN_PUSHES` / `PAWN_DOUBLE_PUSHES` compile-time tables, `pawn_pushes` / `pawn_double_pushes` accessors, and unit tests. | ~50–70 |
| `src/movegen.rs` | Rewrite `generate_pawn_moves_for` with table-driven bitboard targets; hoist `them_bb` / `ep_square` in `for_each_pseudo_legal`; remove dead imports. | ~60–90 |
| `tests/move_order_golden.rs` | Add pawn-rich golden positions captured from the **pre-change** tree; existing goldens untouched. | ~20–40 |
| `docs/plans/perf_analysis7/report5.md` | New implementation report. | New file |

No changes expected in `src/lib.rs`, `src/board.rs`, `src/types.rs`, `src/bitboard.rs`,
`examples/`, or the other tests.

---

## 7. Correctness Contract

For every position and every pawn:

1. **Membership.** The set of emitted moves is unchanged.
   - Single push ↔ `pawn_pushes(us, from) & !occupied`.
   - Double push ↔ start-rank pawn, single push square empty, and
     `pawn_double_pushes(us, from) & !occupied` non-empty.
   - Captures ↔ `pawn_attacks(us, from) & enemy_occupancy`, promoted on the attacking pawn's
     seventh rank.
   - En passant ↔ `ep_square` is a member of `pawn_attacks(us, from)` and an enemy pawn stands on
     the captured square.
2. **Order.** The sequence is unchanged (§4.4): ascending origin square, and within a pawn
   single push, double push, ascending capture target (west then east), en passant.
3. **Promotions.** A pawn on its seventh rank emits its four promotions in `PROMOTION_PIECES`
   order for both pushes and captures; no other pawn emits promotions.
4. **Impossible squares.** Pawns only ever stand on ranks 2–7, but the tables must still be total
   and safe for all 64 squares (empty entries) so that the accessors cannot index out of range.

The verification set is `verify_perft` (41 positions, depths 1–6), `verify_moves`,
`tests/move_order_golden.rs` including the newly locked pawn-rich positions, and the unit tests
for the new tables.

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Capture order flips (west/east) for one colour, changing the golden stream | Medium | The `pawn_attacks` pop order is ascending square index; verify with the `startpos` (white) and `en_passant`/`promotion` (black) goldens, plus the new dual-colour goldens from Step 1. |
| En-passant membership test diverges from the old `to_sq == ep_sq` test | Low | `pawn_attacks(us, from)` contains exactly the two squares that the old arithmetic produced; the enemy-pawn-on-capture-square check is preserved verbatim. Any divergence is caught by the `en_passant` golden and by perft. |
| Promotion detection moves from the target rank to the origin rank and misses a case | Low | Equivalent for every reachable pawn square; the `promotion` golden (push and capture promotions) plus the new promotion-capture golden cover it. |
| Occupancy bitboard disagrees with the `squares` array | Very low | `by_color` is maintained by every move/place/remove path and is already trusted by piece generation and `Board::legal`; `verify_perft` 41/41 is the check. |
| Expected gain lost to code layout on this host | High (precedent: Plans 1, 2, 4) | Two measurement rounds with pairwise comparison; revert on a sub-threshold or mixed result instead of stacking further changes. |
| Full-suite noise hides a regression | High | Decide on fixed-FEN #13/#14 medians and pairwise runs, not the full-suite total. |
| Goldens regenerated after the change, hiding an order regression | Medium | Step 1 requires the goldens to be generated and green *before* the rewrite; Step 4 forbids updating them afterwards. |
| Temptation to also apply §5.6/§5.7/§5.8 or the perft fast path | Medium | Explicitly out of scope; each candidate must remain attributable, as in Plans 1–4. |

---

## 9. Out of Scope / Follow-ups

- **§5.5 options 1 and 2 (true bulk bitboard pawn generation).** These need an explicit
  maintainer decision on the move-order contract, and option 1 also needs the reverted perft shape
  from Plan 1. If this plan's option-3 gain is real but small, the report should quantify the
  remaining pawn cost and ask for that decision rather than deciding unilaterally.
- **§5.6:** `StateInfo` commoner-bitboard cache, attempted alone (it previously regressed only
  when bundled with a capture pre-filter).
- **§5.7 / §5.9:** `compute_pinned` with the Fairy-Stockfish `slider_blockers` pattern and
  precomputed empty-board slider attacks.
- **§5.8:** incrementally maintained `occupied` field on `Board` — only with a measurement
  against the current tree.
- **§5.1 / §5.2:** do not retry the no-compaction `perft` shape or the local `Board::legal`
  bitboard cache without new profiling evidence.
- Do not retry any experiment listed in analysis §6.
