# Plan 4 — Category-Specialized Legality in `Board::legal`

**Corresponds to:** Item 5 of `docs/plans/perf_analysis7/analysis.md` — §5.3,
*Specialize the legality path by move category*
**Estimated speedup:** ~2–4 % on the profiled positions if implemented cleanly; category fast
paths may exceed this on positions with several non-royal commoners
**Risk:** Medium — the refactor must preserve exact atomic-chess legality, including pre-move
pseudo-royal decisions and blast destruction
**Effort:** Medium — approximately 100–160 changed lines in `src/board.rs` plus a small,
classification-only change in `src/movegen.rs`

---

## 1. Context

Plan 3 (§5.4, boolean attack probe) was implemented, measured, and **kept** (`report3.md`). It is
the only perf-analysis-7 change currently in the tree. The next item in analysis §8 is item 5,
§5.3 category-specialized legality.

Two earlier attempts at a broader check-state split (`perf_analysis5` and
`perf_analysis6/plan3.md`, the `EVASIONS`/`NON_EVASIONS` rewrite) regressed. Those attempts changed
the **generator** and added a branch on `state.checkers` around pseudo-legal generation. This plan
does **not** do that. It keeps generation and the compaction loop in their current order and only
specializes the body of the legality predicate by move category.

Analysis §3 and §5.3 note:

- `Board::legal` is ~20 % self time on both profiled positions.
- The compaction loop is ~20 % / ~14 % of source-line samples.
- `Board::legal` currently executes castling, en-passant, capture-blast, commoner-move, pin, and
  pseudo-royal logic in one large function even though most calls need only one category.

Because Plan 3 was accepted on repeated fixed-FEN measurements, start this plan from a fresh
same-session baseline and profile of the kept tree. Do not compare against analysis §1 timings or
against the pre-Plan-3 tree.

---

## 2. Problem

`Board::legal` (`src/board.rs:1310`) classifies nothing. Every call that reaches it recomputes
`from`, `to`, the moving piece, and `is_capture`, then walks through:

1. the castling pass-through block,
2. occupancy adjustment for normal moves, en passant, and castling,
3. the capture blast,
4. own-commoner survival,
5. the pseudo-royal attack loop.

Most of that work is irrelevant for a given move:

- A quiet non-capturing non-commoner move cannot destroy a commoner. Today it still builds the
  capture/blast state and the commoner-survival state.
- A non-capturing commoner move cannot destroy a commoner either. If the side has more than one
  commoner, the move cannot make the commoner pseudo-royal and cannot reduce the commoner count.
- Castling has a unique pass-through check and then a non-capturing commoner move.
- En passant and captures need the blast path, but they do not need castling logic.

The pseudo-royal condition is based on the **pre-move** commoner count (`state.commoners_count`),
not the post-blast count. That must be preserved exactly.

---

## 3. Goal

Split `Board::legal` into a small public wrapper plus private category helpers:

- `legal_castling`
- `legal_en_passant`
- `legal_capture`
- `legal_commoner_move`
- `legal_quiet`

Add a private classifier so the compaction loop can compute the category once and call the matching
helper directly. Use category-specific early exits where they are exactly equivalent to the current
tail, especially:

- non-capturing commoner move with `state.commoners_count > 1` is legal without a blast simulation;
- quiet non-capturing non-commoner move with `state.commoners_count > 1` is legal without an
  occupancy simulation;
- `state.commoners_count == 0` is immediately illegal for quiet and commoner-move categories.

Keep the public `Board::legal(m, state) -> bool` signature and documented semantics. It must remain
the authoritative entry point for external callers and must delegate to the same helpers used by
`generate_legal_with_state`, not to a divergent copy.

### 3.1 Explicit non-goals

This plan does **not**:

- add an `EVASIONS` / `NON_EVASIONS` generator or branch on `state.checkers` around generation;
- change `generate_pseudo_legal`, pawn generation, piece generation, castling generation, or move
  ordering;
- change `tests/move_order_golden.rs` expectations;
- add `StateInfo` commoner bitboards or a capture blast pre-filter;
- retry the no-compaction `perft` path from Plan 1 or the local bitboard cache from Plan 2;
- change the public `Board::attackers_to` method or Plan 3's `square_attacked_by` helper.

---

## 4. Design

### 4.1 Category type and classifier

Add a crate-private enum in `src/board.rs`:

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegalMoveKind {
    Castling,
    EnPassant,
    Capture,
    CommonerMove,
    Quiet,
}
```

Add a classifier on `Board`:

```rust
impl Board {
    /// Classify a pseudo-legal move for legality dispatch.
    ///
    /// Returns `None` for a move whose origin square is empty.
    #[inline]
    pub(crate) fn classify_legal_move(&self, m: Move) -> Option<LegalMoveKind> {
        let from = m.from_sq();
        let piece = self.piece_on(from);
        if piece == NO_PIECE {
            return None;
        }

        match m.move_type() {
            MoveType::Castling => Some(LegalMoveKind::Castling),
            MoveType::EnPassant => Some(LegalMoveKind::EnPassant),
            _ if self.piece_on(m.to_sq()) != NO_PIECE => Some(LegalMoveKind::Capture),
            _ if piece.type_of() == Some(PieceType::Commoner) => {
                Some(LegalMoveKind::CommonerMove)
            }
            _ => Some(LegalMoveKind::Quiet),
        }
    }
}
```

Notes:

- `MoveType::Promotion` is intentionally not its own legality category. A non-capturing promotion
  behaves like a quiet move for atomic legality; a capturing promotion is a capture.
- En passant is classified before the destination-piece check because its destination square is
  empty.
- The classifier does not inspect `StateInfo`; it is a pure function of the board and move.
- Keep this classifier `pub(crate)`; it is not public API.

### 4.2 Public wrapper

`Board::legal` becomes a thin wrapper:

```rust
#[must_use]
pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
    match self.classify_legal_move(m) {
        Some(kind) => self.legal_with_kind(m, state, kind),
        None => false,
    }
}
```

Add the crate-private dispatcher:

```rust
#[inline]
pub(crate) fn legal_with_kind(
    &self,
    m: Move,
    state: &StateInfo,
    kind: LegalMoveKind,
) -> bool {
    match kind {
        LegalMoveKind::Castling => self.legal_castling(m, state),
        LegalMoveKind::EnPassant => self.legal_en_passant(m, state),
        LegalMoveKind::Capture => self.legal_capture(m, state),
        LegalMoveKind::CommonerMove => self.legal_commoner_move(m, state),
        LegalMoveKind::Quiet => self.legal_quiet(m, state),
    }
}
```

The existing doc comment on `Board::legal` stays on the public wrapper. The helpers are private and
documented only as needed for maintainability.

### 4.3 Shared pseudo-royal tail

Extract the current tail of `Board::legal` into one helper so all categories use identical
pseudo-royal semantics:

```rust
#[inline(always)]
fn legal_commoner_survival(
    &self,
    state: &StateInfo,
    occupied: Bitboard,
    our_commoners: Bitboard,
    them_commoners: Bitboard,
) -> bool {
    if our_commoners.is_empty() {
        return false;
    }

    let our_pr_count = state.commoners_count as usize;
    let them_pr_count = state.them_commoners_count as usize;

    if our_pr_count <= 1 {
        let enemy_pr_destroyed =
            them_pr_count <= 1 && (them_commoners & occupied).is_empty();

        if !enemy_pr_destroyed {
            let enemy_survivors = self.by_color[them as usize] & occupied;

            let mut c = our_commoners;
            while !c.is_empty() {
                let ksq = c.pop_lsb();
                let adjacent_enemy = them_commoners & attacks::king_attacks(ksq);
                if adjacent_enemy.is_empty()
                    && square_attacked_by(self, ksq, occupied, enemy_survivors)
                {
                    return false;
                }
            }
        }
    }

    true
}
```

Rules for this helper:

- Use the **pre-move** `state.commoners_count` and `state.them_commoners_count`, exactly as the
  current code does. Do not substitute a post-blast survivor count.
- Keep the `enemy_pr_destroyed` condition, the adjacent-commoner immunity, and the order of the
  checks unchanged.
- Keep `square_attacked_by` from Plan 3; do not route commoner adjacency through it.
- `them` is derived from `self.side_to_move.flip()` inside the helper or passed explicitly; either
  is fine, but keep it consistent across helpers.

### 4.4 Category helpers

Each helper may assume `classify_legal_move(m)` returned its category. They must still be correct
for the public `legal` wrapper, which calls the classifier first.

#### `legal_castling`

Preserve the current castling pass-through loop exactly:

```rust
fn legal_castling(&self, m: Move, state: &StateInfo) -> bool {
    let from = m.from_sq();
    let to = m.to_sq();
    let us = self.side_to_move;
    let them = us.flip();

    let ksq = from;
    let base_occupied = self.occupied();
    let pass_through = if to > ksq {
        [ksq, Square::from_index(ksq as i8 + 1)]
    } else {
        [Square::from_index(ksq as i8 - 1), ksq]
    };

    for &sq in &pass_through {
        let adjacent_enemy_commoners = self.commoners(them) & attacks::king_attacks(sq);
        if adjacent_enemy_commoners.is_empty() {
            let enemy_commoner_attack = attacks::king_attacks(sq)
                & self.by_type[PieceType::Commoner as usize]
                & self.by_color[them as usize];
            if square_attacked_by(self, sq, base_occupied, self.by_color[them as usize])
                || !enemy_commoner_attack.is_empty()
            {
                return false;
            }
        }
    }

    // Castling is a non-capturing commoner move. If the side has more than one
    // commoner, the current pre-move pseudo-royal condition never runs.
    if state.commoners_count > 1 {
        return true;
    }

    let (_, kto_actual, rfrom, rto) = castling_squares(us, to > from);
    let mut occupied = base_occupied ^ Bitboard::square_bb(from);
    occupied = occupied ^ Bitboard::square_bb(rfrom);
    occupied = occupied | Bitboard::square_bb(rto);
    occupied = occupied | Bitboard::square_bb(kto_actual);

    let our_commoners =
        (self.commoners(us) & occupied) | Bitboard::square_bb(kto_actual);
    let them_commoners = self.commoners(them);
    self.legal_commoner_survival(state, occupied, our_commoners, them_commoners)
}
```

The `state.commoners_count > 1` early return is equivalent to the old tail because a non-capturing
castling move leaves at least one own commoner on the board, and the old tail skips its pseudo-royal
block whenever `our_pr_count > 1`. The pass-through loop still runs first, because castling through
attack is rejected regardless of commoner count in the current implementation.

#### `legal_en_passant`

```rust
fn legal_en_passant(&self, m: Move, state: &StateInfo) -> bool {
    let from = m.from_sq();
    let to = m.to_sq();
    let us = self.side_to_move;
    let them = us.flip();

    let capsq = match us {
        Color::White => Square::from_index(to as i8 - 8),
        Color::Black => Square::from_index(to as i8 + 8),
    };

    let base_occupied = self.occupied();
    let mut occupied = base_occupied ^ Bitboard::square_bb(from);
    occupied = occupied & !Bitboard::square_bb(capsq);
    occupied = occupied | Bitboard::square_bb(to);

    let pre_pawns = self.by_type[PieceType::Pawn as usize];
    let pre_non_pawns = base_occupied ^ pre_pawns;
    let blast_adjacent = attacks::king_attacks(to) & pre_non_pawns;
    occupied = occupied & !(blast_adjacent | Bitboard::square_bb(to));

    let our_commoners = self.commoners(us) & occupied;
    let them_commoners = self.commoners(them);
    self.legal_commoner_survival(state, occupied, our_commoners, them_commoners)
}
```

Keep the en-passant capture-square direction exactly as in the current function.

#### `legal_capture`

```rust
fn legal_capture(&self, m: Move, state: &StateInfo) -> bool {
    let from = m.from_sq();
    let to = m.to_sq();
    let us = self.side_to_move;
    let them = us.flip();

    let base_occupied = self.occupied();
    let mut occupied = base_occupied ^ Bitboard::square_bb(from);
    occupied = occupied | Bitboard::square_bb(to);

    let pre_pawns = self.by_type[PieceType::Pawn as usize];
    let pre_non_pawns = base_occupied ^ pre_pawns;
    let blast_adjacent = attacks::king_attacks(to) & pre_non_pawns;
    occupied = occupied & !(blast_adjacent | Bitboard::square_bb(to));

    let our_commoners = self.commoners(us) & occupied;
    let them_commoners = self.commoners(them);
    self.legal_commoner_survival(state, occupied, our_commoners, them_commoners)
}
```

Do **not** add a capture pre-filter or early rejection based on `state` counts. The blast path is
where the legality decision is made. The shared tail already returns `true` quickly when
`our_commoners` is non-empty and `state.commoners_count > 1`.

#### `legal_commoner_move`

```rust
fn legal_commoner_move(&self, m: Move, state: &StateInfo) -> bool {
    let from = m.from_sq();
    let to = m.to_sq();
    let us = self.side_to_move;
    let them = us.flip();

    if state.commoners_count == 0 {
        return false;
    }

    // Non-capturing commoner move with multiple commoners: no commoner can be
    // destroyed, and the pre-move pseudo-royal condition is not active.
    if state.commoners_count > 1 {
        return true;
    }

    let base_occupied = self.occupied();
    let mut occupied = base_occupied ^ Bitboard::square_bb(from);
    occupied = occupied | Bitboard::square_bb(to);

    let our_commoners =
        (self.commoners(us) & occupied) | Bitboard::square_bb(to);
    let them_commoners = self.commoners(them);
    self.legal_commoner_survival(state, occupied, our_commoners, them_commoners)
}
```

The `commoners_count == 0` case is unreachable for a real commoner move but keeps the helper
defensive and equivalent if called through the public wrapper with unusual state.

#### `legal_quiet`

```rust
fn legal_quiet(&self, m: Move, state: &StateInfo) -> bool {
    let from = m.from_sq();
    let to = m.to_sq();
    let us = self.side_to_move;
    let them = us.flip();

    if state.commoners_count == 0 {
        return false;
    }

    // A non-capturing, non-commoner move cannot destroy a commoner. With more
    // than one commoner, the old tail returns true without entering the
    // pseudo-royal loop.
    if state.commoners_count > 1 {
        return true;
    }

    let base_occupied = self.occupied();
    let mut occupied = base_occupied ^ Bitboard::square_bb(from);
    occupied = occupied | Bitboard::square_bb(to);

    let our_commoners = self.commoners(us) & occupied;
    let them_commoners = self.commoners(them);
    self.legal_commoner_survival(state, occupied, our_commoners, them_commoners)
}
```

This category covers quiet pawn pushes, quiet promotions, pinned non-commoner moves, and quiet
non-commoner moves while in check. It intentionally does not inspect `state.checkers`; the final
attack check in `legal_commoner_survival` handles positions with exactly one commoner.

### 4.5 Dispatch from `generate_legal_with_state`

In `src/movegen.rs`, keep the compaction loop structure and pseudo-legal order unchanged. Change
only the fallback from `board.legal` to the pre-classified helper:

```rust
for read_idx in 0..orig_len {
    let m = ms[read_idx];
    let legal = if is_move_trivially_legal(board, m, state) {
        true
    } else {
        match board.classify_legal_move(m) {
            Some(kind) => board.legal_with_kind(m, state, kind),
            None => false,
        }
    };
    if legal {
        ms[write_idx] = m;
        write_idx += 1;
    }
}
```

`has_legal_move_with_state` may keep calling `board.legal(m, state)`; it is not hot enough to
require a second dispatch path. If a later measurement shows otherwise, change it separately.

Do not move `is_move_trivially_legal` into the helpers and do not change its definition. It remains
the fast-accept path for quiet moves, castling-free moves, etc.

### 4.6 Inlining policy

- Mark `classify_legal_move` and `legal_with_kind` `#[inline]`.
- Mark `legal_commoner_survival` `#[inline(always)]`, matching the measured hot-tail style.
- Mark the five category helpers `#[inline]` initially. Do **not** mark them `#[inline(never)]` or
  `#[cold]`; the EVASIONS attempts showed that isolating the hot path can change layout
  unfavorably.
- If the compiler's layout produces a regression, measure changes to these attributes one at a
  time. Do not combine attribute experiments with semantic changes.

---

## 5. Implementation Steps

### Step 1: Re-baseline and profile the kept Plan 3 tree

`report3.md` recorded materially different absolute timings from analysis §1, and Plan 3 itself is
now part of the baseline. Re-measure before editing and use only same-session comparisons.

```sh
cargo build --release --examples

F13='rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5'
F14='r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15'

# Seven runs each; capture wall time around each invocation.
for i in $(seq 1 7); do
  start=$(date +%s%N)
  target/release/examples/perft "$F13" 6
  end=$(date +%s%N)
  echo "#13 run $i: $(( (end - start) / 1000000 )) ms"
done

for i in $(seq 1 7); do
  start=$(date +%s%N)
  target/release/examples/perft "$F14" 6
  end=$(date +%s%N)
  echo "#14 run $i: $(( (end - start) / 1000000 )) ms"
done
```

Confirm the node counts are unchanged:

- #13: `2,160,817,389`
- #14: `245,931,633`

Record a full-suite baseline once for context; it is supporting evidence only:

```sh
cargo run --release --example verify_perft
```

Capture a fresh profile of the current kept tree for both FENs:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft

perf record -F 997 -g --call-graph dwarf -o /tmp/perf7-plan4-13.data \
  target/profiling/examples/perft "$F13" 6
perf record -F 997 -g --call-graph dwarf -o /tmp/perf7-plan4-14.data \
  target/profiling/examples/perft "$F14" 6

perf report -i /tmp/perf7-plan4-13.data --stdio --no-children --sort=symbol
perf report -i /tmp/perf7-plan4-13.data --stdio --no-children --sort=srcline
perf report -i /tmp/perf7-plan4-14.data --stdio --no-children --sort=symbol
```

Use this profile to confirm that `Board::legal` and its blast/commoner work are still the dominant
target on the current host. If the profile no longer shows `Board::legal` as hot, stop and write a
report recommending a new analysis rather than proceeding with this plan.

### Step 2: Add the classifier and split without fast paths

1. Add `LegalMoveKind` and `classify_legal_move`.
2. Add `legal_with_kind` and the five category helpers by moving the current logic into them.
3. Extract `legal_commoner_survival` exactly as in §4.3.
4. Make `Board::legal` delegate to `legal_with_kind`.

At this substage, do **not** add the `commoners_count > 1` early returns or the `commoners_count ==
0` early returns. The goal is a pure refactor with identical work, to verify correctness and to
measure whether dispatch/layout alone helps or hurts.

Run the full correctness checks from Step 4 before adding fast paths.

If this pure split is already a stable regression on both FENs, revert and record the negative
result. Do not try to rescue it with fast paths in the same measurement.

### Step 3: Add category fast paths incrementally

Add the fast paths in this order, measuring after each:

1. **Quiet fast path** — `commoners_count == 0` returns false; `commoners_count > 1` returns true.
2. **Commoner-move fast path** — `commoners_count == 0` returns false; `commoners_count > 1`
   returns true.
3. **Castling fast path** — after the pass-through loop, `commoners_count > 1` returns true.
4. **Movegen dispatch** — compute the category once in `generate_legal_with_state` only after
   `is_move_trivially_legal` fails, and call `legal_with_kind`.

Accept a substage only if it does not regress either fixed FEN. If a substage is neutral, keep or
drop it based on the combined result; do not stack a speculative third substage on top of a stable
regression.

### Step 4: Correctness checks

After each accepted implementation substage:

```sh
cargo fmt
cargo clippy --all-targets
cargo test
cargo doc
cargo run --release --example verify_perft
```

Required results:

- No new `clippy` warnings. The pre-existing `manual_contains` / `manual_range_contains` warnings in
  `examples/test_d5g8.rs`, if still present, are unrelated.
- All unit, integration, golden-order, and doc tests pass.
- `verify_perft` reports **41/41** at depths 1–6.
- `tests/move_order_golden.rs` stays green. This plan does not change generation or ordering.

The strongest correctness checks are the 41-position perft suite and the golden move-order test. If
the pure split in Step 2 fails any perft position, compare the category helpers against the old
`Board::legal` body line by line; do not proceed to fast paths until the split is exactly
equivalent.

### Step 5: Performance verification

Rebuild release examples and repeat the Step 1 fixed-FEN loops in the same session.

Acceptance criteria:

- Test #13 median improves by at least ~1.5 %, consistent with the low end of the analysis's
  ~2–4 % estimate after code-layout risk.
- Test #14 median is neutral or improves; it must not show a stable regression across repeated
  rounds.
- If the result is close to the noise floor, perform the same restore/rebuild/remeasure A/B used by
  `report2.md` and `report3.md`.
- The full suite must remain 41/41. Do not accept or reject solely on the noisy full-suite total.

If the combined result is neutral or a stable regression:

1. Revert `src/board.rs` and `src/movegen.rs` to the kept Plan 3 tree.
2. Rebuild and re-run the same A/B to confirm the baseline is restored.
3. Record the negative result in `report4.md`.
4. Do not automatically proceed to §5.5 or §5.6 in the same tree without deciding whether another
   fresh profile is warranted.

### Step 6: Write the implementation report

Create `docs/plans/perf_analysis7/report4.md` as required by `AGENTS.md`. Include:

- The exact category split, classifier, dispatch changes, and which fast paths were retained.
- Confirmation that generation, pseudo-legal order, `is_move_trivially_legal`, and public move
  ordering are unchanged.
- Baseline and post-change fixed-FEN #13/#14 timings, all runs plus medians, and the full-suite
  result.
- Results for each incremental substage (pure split, quiet fast path, commoner fast path, castling
  fast path, movegen dispatch) so the report explains which part helped or hurt.
- Whether the ~2–4 % expectation from analysis §5.3 reproduced on this host.
- Correctness: `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo doc`, and
  `verify_perft` 41/41.
- Explicit statement of whether the change was kept or reverted, with the reason.
- If kept: the next candidate from analysis §8 and whether a fresh profile is needed.
- If reverted: the negative result and whether any smaller category fast path is worth retrying
  alone.

---

## 6. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/board.rs` | Add `LegalMoveKind`, classifier, dispatcher, category helpers, shared survival helper; make `Board::legal` a wrapper. | ~100–140 |
| `src/movegen.rs` | Classify only after the trivial-legality fast path fails and call `legal_with_kind`; compaction order unchanged. | ~10–20 |
| `docs/plans/perf_analysis7/report4.md` | New implementation report. | New file |

No changes expected in `src/lib.rs`, `src/types.rs`, `src/attacks.rs`, tests, examples, or FEN
parsing.

---

## 7. Correctness Contract

For every board, `Move`, and populated `StateInfo`, the refactored `Board::legal` must return exactly
the same boolean as the kept Plan 3 `Board::legal`.

This holds if the category helpers preserve these rules:

1. **Classification is exhaustive for pseudo-legal moves.** Castling, en passant, captures,
   non-capturing commoner moves, and all remaining quiet moves are disjoint categories.
2. **Pre-move pseudo-royal decision.** `legal_commoner_survival` uses
   `state.commoners_count` and `state.them_commoners_count` from before the move, exactly like the
   current tail.
3. **Fast paths are equivalent.**
   - A non-capturing move cannot destroy a commoner.
   - A non-capturing commoner move with `state.commoners_count > 1` leaves at least one commoner,
     so the old tail returns true without entering its pseudo-royal loop.
   - A quiet non-capturing non-commoner move with `state.commoners_count > 1` leaves the commoner
     set non-empty, so the old tail returns true without entering its pseudo-royal loop.
   - With `state.commoners_count == 0`, the old `our_commoners.is_empty()` guard returns false.
4. **Blast path is unchanged.** Captures and en passant still compute `pre_non_pawns` from the
   pre-move occupancy minus pawns, mask the blast zone, and then evaluate commoner survival.
5. **Castling pass-through is unchanged.** It runs before any category fast path and uses the same
   `square_attacked_by` and adjacent-commoner logic as Plan 3.
6. **Public behavior is unchanged.** `Board::legal` remains public with the same signature and
   delegates to the same helpers used by generation.

The 41-position `verify_perft` suite at depths 1–6, plus `move_order_golden` and `verify_moves`, are
the strongest verification.

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Fast path uses post-move commoner count instead of pre-move | Low–medium | Use `state.commoners_count` only; never derive the pseudo-royal decision from survivors. |
| Quiet/commoner fast path is wrong during check | Low | The fast path only applies when `commoners_count > 1`; verify against perft on check-heavy positions, especially #13. |
| Castling fast path skips the pass-through check | Low | Keep the existing pass-through loop before the early return. |
| Category helpers drift from the original blast logic | Medium | Extract the blast logic verbatim; validate the pure split before adding fast paths. |
| Movegen dispatch adds more overhead than it saves | Medium | Measure the pure split and dispatch separately; skip movegen dispatch if it regresses. |
| Code layout cancels gains, as in Plans 1 and 2 | Medium | Incremental substage measurements; revert stable regressions; do not combine with other optimizations. |
| Full-suite noise hides a regression | High | Base acceptance on repeated fixed-FEN #13/#14 medians, including a same-session A/B recheck. |
| Duplicating `Board::legal` leaves two divergent implementations | Medium | Make the public wrapper delegate to the helpers; do not keep a second full copy. |
| Temptation to add a check-state generator or capture pre-filter | Medium | Explicitly out of scope; preserve the current generator and compaction loop. |

---

## 9. Out of Scope / Follow-ups

- **§5.5:** bulk pawn generation only after a move-order policy decision. Do not change
  `tests/move_order_golden.rs` as part of this plan.
- **§5.6:** `StateInfo` commoner-bitboard cache alone. It may be considered after this plan is
  settled, but it must not be bundled into this change.
- **§5.7:** `compute_pinned` occupancy-delta and empty-board slider attacks, if the new profile
  still shows `compute_pinned`.
- **§5.8:** maintained `Board::occupied` cache, only if the profile still shows occupancy cost
  after this plan.
- **Plan 1 / Plan 2:** do not retry the no-compaction `perft` shape or the local `Board::legal`
  bitboard cache without new profiling evidence.
- Do not retry the `EVASIONS` / `NON_EVASIONS` generator split as part of this plan; this plan is a
  legality-predicate refactor, not a check-state generator.
