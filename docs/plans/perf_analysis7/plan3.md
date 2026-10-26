# Plan 3 — Boolean Attack Probe with Early Exit

**Corresponds to:** Item 4 of `docs/plans/perf_analysis7/analysis.md` — §5.4,
*Boolean attack probe with early exit*
**Estimated speedup:** ~0.5–1 % when combined with the local legality cache; analysis measured
~0.9 % on test #13 and ~0.6 % on test #14 for the probe alone
**Risk:** Low — private helper only, exact boolean equivalence to the current `attackers_to`
call sites
**Effort:** Small — one private helper and two call-site substitutions in `src/board.rs`

---

## 1. Context

Plan 1 (§5.1, no-compaction `perft`) and Plan 2 (§5.2, local bitboard cache in `Board::legal`)
were both implemented, measured, and **reverted** because they did not meet their fixed-FEN
acceptance criteria on this host (`report1.md`, `report2.md`). The source tree still contains the
original `perft` and `Board::legal` implementations.

The next item in analysis §8 is therefore item 4, the boolean attack probe. It was measured
separately during the analysis as a modest win: about **0.9 % on #13** and **0.6 % on #14**
(analysis §4). Those numbers were obtained with the probe alone, so it is not necessary to land
Plan 2 first. The expected gain is small, and this host has already shown that small expected
gains can invert under code-layout changes; the plan is therefore deliberately narrow and must be
accepted only on repeated fixed-FEN measurements.

Do **not** bundle this with any other optimization.

---

## 2. Problem

`Board::legal` calls the private helper `attackers_to` in two places and then only tests whether
the result is empty:

1. **Castling pass-through safety** (`src/board.rs:1335`):

   ```rust
   let atk = attackers_to(self, sq, occupied, self.by_color[them as usize])
       | (attacks::king_attacks(sq)
           & self.by_type[PieceType::Commoner as usize]
           & self.by_color[them as usize]);
   if atk != Bitboard::EMPTY {
       return false;
   }
   ```

2. **Pseudo-royal survival check** (`src/board.rs:1397`):

   ```rust
   if adjacent_enemy.is_empty()
       && attackers_to(self, ksq, occupied, enemy_survivors) != Bitboard::EMPTY
   {
       return false;
   }
   ```

The private helper (`src/board.rs:1415`) always computes:

- both bishop/queen sliding attacks and rook/queen sliding attacks, even if a pawn or knight
  already attacks the square;
- all attacker categories and repeatedly intersects them with `enemy_bb`;
- a `Bitboard`, although the callers only need a single predicate.

On positions where an early category hits, computing the remaining categories is wasted work.
`attackers_to` is only reached after the trivial-legality fast path has already rejected the move,
so this is on the hot non-trivial legality path targeted by both profiled positions.

### 2.1 This is not the failed Plan 2 cache

This plan does **not**:

- add locals for `occupied` or commoner bitboards in `Board::legal`;
- add fields to `StateInfo`;
- add a capture pre-filter;
- change `generate_legal`, `perft`, move generation, or move ordering;
- change the public `Board::attackers_to` method.

It only replaces two emptiness tests with a boolean probe that is logically identical to the
existing private helper.

---

## 3. Goal

Add a private `#[inline(always)]` function:

```rust
fn square_attacked_by(
    board: &Board,
    sq: Square,
    occupied: Bitboard,
    enemy_bb: Bitboard,
) -> bool
```

that returns exactly the same truth value as:

```rust
attackers_to(board, sq, occupied, enemy_bb) != Bitboard::EMPTY
```

but returns as soon as one attacker category matches. Use it at the two call sites above and
remove the now-unused private bitboard helper. Public APIs, semantics, and move ordering must not
change.

The helper must **not** include commoner attacks. The current private `attackers_to` excludes
commoners deliberately: the castling path and pseudo-royal loop handle enemy-commoner adjacency
separately because of the pseudo-royal immunity rules. Preserve that separation exactly.

---

## 4. Design

### 4.1 Private boolean helper

Replace the private `attackers_to` helper with:

```rust
/// Return `true` if an enemy non-commoner piece attacks `sq` on `occupied`.
///
/// Mirrors the attacker categories of the previous private `attackers_to`
/// helper, but exits at the first matching category. Commoner attacks are
/// intentionally excluded; callers handle adjacent enemy commoners separately.
#[inline(always)]
fn square_attacked_by(
    board: &Board,
    sq: Square,
    occupied: Bitboard,
    enemy_bb: Bitboard,
) -> bool {
    // Pawns. `board.side_to_move` is the side whose commoner is being tested,
    // matching the previous helper's pawn direction.
    if !(attacks::pawn_attacks(board.side_to_move, sq)
        & board.by_type[PieceType::Pawn as usize]
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Knights.
    if !(attacks::knight_attacks(sq)
        & board.by_type[PieceType::Knight as usize]
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Bishop/queen sliders.
    if !(attacks::bishop_attacks(sq, occupied)
        & (board.by_type[PieceType::Bishop as usize]
            | board.by_type[PieceType::Queen as usize])
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Rook/queen sliders. This is skipped when a pawn, knight, bishop, or
    // queen already hit.
    if !(attacks::rook_attacks(sq, occupied)
        & (board.by_type[PieceType::Rook as usize]
            | board.by_type[PieceType::Queen as usize])
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    false
}
```

Notes:

- The ordering follows analysis §5.4: pawns, knights, bishops/queens, rooks/queens.
- The queen appears in both slider tests, exactly as in the old helper.
- The pawn direction uses `board.side_to_move`, matching the old helper. Both call sites are in
  `Board::legal`, where the side to move is the side whose commoners are being tested.
- The helper uses `is_empty()` rather than comparing to `Bitboard::EMPTY`; either form is fine,
  but use one consistently.
- If the compiler decides to compute both slider attack sets before the branches, that is still
  no worse than the current helper. The early exits are the only intended behavioral difference.

### 4.2 Castling call site

Keep the existing adjacent-commoner condition and commoner-attack test; only replace the
`attackers_to(...) | commoner_attacks` emptiness test:

```rust
if adjacent_enemy_commoners.is_empty() {
    let enemy_commoner_attack = attacks::king_attacks(sq)
        & self.by_type[PieceType::Commoner as usize]
        & self.by_color[them as usize];
    if square_attacked_by(self, sq, occupied, self.by_color[them as usize])
        || !enemy_commoner_attack.is_empty()
    {
        return false;
    }
}
```

Preserve short-circuiting: if `square_attacked_by` returns `true`, the commoner test need not be
evaluated, and vice versa. Do not reorder the outer `adjacent_enemy_commoners` check.

### 4.3 Pseudo-royal call site

Replace only the predicate:

```rust
if adjacent_enemy.is_empty()
    && square_attacked_by(self, ksq, occupied, enemy_survivors)
{
    return false;
}
```

The surrounding loop, `our_commoners` iteration, `enemy_survivors` construction, and
`adjacent_enemy` check stay exactly as they are.

### 4.4 Remove the old private helper

After both call sites are converted, the private `attackers_to` helper has no callers. Delete it
to avoid a `dead_code` warning and to ensure there is only one implementation of the predicate.

Do **not** delete or change the public `Board::attackers_to` method at `src/board.rs:897`; it is
public API and may be used by consumers and tests.

### 4.5 What not to change

- No changes to `Board::legal`'s cached bitboards, branch order, or early returns beyond the two
  substitutions above.
- No changes to `is_move_trivially_legal`, `compute_checkers`, `compute_pinned`, `populate_state`,
  or `do_move`/`undo_move`.
- No changes to `src/movegen.rs`, `src/lib.rs`, tests, or examples.
- No `StateInfo` fields, no capture pre-filter, no compaction changes.

---

## 5. Implementation Steps

### Step 1: Re-baseline on this host

Plans 1 and 2 recorded materially different absolute timings from the original analysis, so
re-measure before editing and use only same-session comparisons.

```sh
cargo build --release --examples

F13='rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5'
F14='r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15'

# Seven runs each. Capture wall time around each invocation.
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

Record all seven runs, not just the median. Confirm the node counts are unchanged from the
analysis:

- #13: `2,160,817,389`
- #14: `245,931,633`

Also record a full-suite baseline for context, but treat it as supporting evidence only:

```sh
cargo run --release --example verify_perft
```

If `perf` is available, optionally capture a fresh profile of the current tree for both FENs so
the decision to continue with later items is based on this host rather than the original analysis
host:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
perf record -F 997 -g --call-graph dwarf -o /tmp/perf7-plan3-baseline.data \
  target/profiling/examples/perft "$F13" 6
perf report -i /tmp/perf7-plan3-baseline.data --stdio --no-children --sort=symbol
```

This profile is not an acceptance criterion; it is context for whether the remaining ideas are
worth pursuing after this plan.

### Step 2: Add the boolean probe

In `src/board.rs`, add `square_attacked_by` near the old private `attackers_to` helper and convert
the two call sites per §4.2 and §4.3. Delete the old private helper. Keep the diff confined to
those three locations.

### Step 3: Correctness checks

```sh
cargo fmt
cargo clippy --all-targets
cargo test
cargo doc
cargo run --release --example verify_perft
```

Required results:

- `cargo clippy --all-targets` has no new warnings. The pre-existing `manual_contains` warnings in
  `examples/test_d5g8.rs`, if still present, are unrelated.
- All unit, integration, golden-order, and doc tests pass.
- `verify_perft` reports **41/41** at depths 1–6.
- `tests/move_order_golden.rs` stays green, proving move generation and ordering are unchanged.

Because the helper is private, the existing `legal` coverage plus `verify_perft` is the primary
verification. Optionally add a focused unit test in the `board.rs` test module comparing
`square_attacked_by` with the public `Board::attackers_to` filtered to enemy non-commoner pieces
for a few FENs and every square. This is useful if the helper is tricky to review, but it is not
required to accept the change.

### Step 4: Performance verification

Rebuild and repeat the Step 1 fixed-FEN measurements in the same session.

```sh
cargo build --release --examples
# Re-run the #13 and #14 loops from Step 1.
```

Use all seven runs to compute medians. If the first round is close to the noise floor, restore the
baseline, rebuild, remeasure, then reapply the patch and remeasure again, exactly as
`report2.md` did.

Acceptance criteria:

- Test #13 median improves by at least ~0.5 %, consistent with the analysis's ~0.9 % standalone
  measurement.
- Test #14 does not regress beyond normal noise; if it improves, that matches the analysis.
- Neither fixed FEN shows a stable regression across both measurement rounds.
- The full suite passes 41/41 and does not show a material slowdown, but do not accept or reject
  solely on the noisy full-suite total.

If the change is neutral or mixed:

1. Revert `src/board.rs` to its original content.
2. Rebuild and re-run the same A/B to confirm the baseline is restored.
3. Record the negative result in `report3.md`.
4. Do not attempt to "rescue" the result by adding the local legality cache from Plan 2 or the
   `StateInfo` commoner cache from §5.6 in the same change.

### Step 5: Write the implementation report

Create `docs/plans/perf_analysis7/report3.md` as required by `AGENTS.md`. Include:

- Summary of the exact helper and call-site substitutions and confirmation that only
  `src/board.rs` changed.
- Baseline and post-change fixed-FEN #13/#14 timings, all runs plus medians, and the full-suite
  result.
- Whether the analysis's ~0.9 % / ~0.6 % probe gains reproduced on this host.
- Correctness: `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo doc`, and
  `verify_perft` 41/41.
- Explicit statement of whether the change was kept or reverted, with the decision reason.
- Confirmation that the public `Board::attackers_to`, `Board::legal`,
  `is_move_trivially_legal`, `generate_legal`, `perft`, and move ordering are unchanged.
- If reverted: the negative result and whether further host-specific micro-optimization is
  worthwhile, or whether the next step should be a fresh profile/rebaseline rather than another
  small legal-path change.
- If kept: any profile evidence that the attack-probe work dropped, and the next candidate from
  analysis §8.

---

## 6. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/board.rs` | Add boolean `square_attacked_by`; convert the castling and pseudo-royal call sites; remove the private bitboard `attackers_to` helper. | ~30–40 |
| `docs/plans/perf_analysis7/report3.md` | New implementation report. | New file |

No changes expected in `src/lib.rs`, `src/movegen.rs`, `src/types.rs`, tests, or examples.

---

## 7. Correctness Contract

For any board, square, occupancy, and enemy bitboard, the new helper must satisfy:

```text
square_attacked_by(board, sq, occupied, enemy_bb)
    == (old private attackers_to(board, sq, occupied, enemy_bb) != Bitboard::EMPTY)
```

This holds because:

- The helper evaluates the same five categories as the old private helper: pawns, knights,
  bishops, rooks, and queens, all filtered by `enemy_bb`.
- Each category is tested for non-emptiness; the boolean result is the OR of those tests.
- Returning early on the first non-empty category cannot change an OR result that is already
  true.
- Commoner attacks remain excluded from both the old helper and the new helper; callers continue
  to handle adjacent enemy commoners separately.
- The pawn direction still uses `board.side_to_move`, matching the old helper.

The 41-position `verify_perft` run plus `move_order_golden` and `verify_moves` are the strongest
verification. If an optional direct helper-equivalence test is added, it must compare against the
public `Board::attackers_to` result filtered to `enemy_bb` and excluding commoners.

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Boolean helper accidentally includes or excludes commoners differently | Low | Keep commoner handling at the call sites exactly as before; do not route commoner adjacency through the new helper. |
| Pawn direction is reversed or uses the wrong color | Low | Use `board.side_to_move`, exactly as the old private helper did; full perft catches any mismatch. |
| Call-site rewrite changes outer branch/short-circuit order | Low | Substitute only the emptiness predicate; keep `adjacent_enemy`/`adjacent_enemy_commoners` guards in place. |
| Removing the old helper leaves an unused function or import | Low | Delete it and run `cargo clippy --all-targets`. |
| Expected small gain is lost to code layout | Medium | Repeated fixed-FEN A/B with all runs recorded; revert on a stable regression or neutral result. |
| Full-suite noise hides a regression | High | Base the decision on fixed-FEN #13/#14 medians, not the full-suite total. |
| Temptation to stack Plan 2's cache or §5.6 on top | Medium | Explicitly out of scope; each candidate must remain attributable. |

---

## 9. Out of Scope / Follow-ups

- **§5.3:** category-specialized legality. Only after this plan is settled and both fixed FENs are
  re-baselined.
- **§5.6:** `StateInfo` commoner-bitboard cache alone. Do not combine it with this plan.
- **§5.5:** bulk pawn generation only after a move-order policy decision; the golden-order test
  currently pins public ordering.
- **Plan 1 / Plan 2:** do not retry the no-compaction `perft` shape or the local `Board::legal`
  cache without new profiling evidence.
- Do not retry the rejected experiments listed in analysis §6 as part of this work.
