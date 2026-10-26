# Implementation Report — Plan 3: Boolean Attack Probe with Early Exit

**Plan:** `docs/plans/perf_analysis7/plan3.md`
**Status:** Implemented, measured, and **kept**. The fixed-FEN acceptance criteria were met
in both measurement rounds.

---

## 1. Summary

The planned change was applied only in `src/board.rs`:

1. The private bitboard helper `attackers_to(board, sq, occupied, enemy_bb) -> Bitboard` was
   replaced by the private boolean helper
   `square_attacked_by(board, sq, occupied, enemy_bb) -> bool`.
2. The helper tests pawns, knights, bishop/queen sliders, and rook/queen sliders in that
   order and returns `true` at the first matching category.
3. The castling pass-through safety check now calls `square_attacked_by` and the separate
   enemy-commoner attack test, preserving the existing short-circuit structure.
4. The pseudo-royal survival check now calls `square_attacked_by` instead of comparing the
   old bitboard result to `Bitboard::EMPTY`.

Commoner attacks remain excluded from the helper, exactly as before. Adjacent enemy
commoners are still handled separately at both call sites. No public API, move generation,
move ordering, state population, or perft code was changed.

`git diff --stat` confirms that the only modified source file is `src/board.rs`
(`50 insertions, 22 deletions`). The only new file from this plan is this report; `plan3.md`
was already untracked before implementation.

---

## 2. Implementation

### 2.1 New private helper

```rust
/// Return `true` if an enemy non-commoner piece attacks `sq` on `occupied`.
///
/// Mirrors the attacker categories of the previous private `attackers_to`
/// helper, but exits at the first matching category. Commoner attacks are
/// intentionally excluded; callers handle adjacent enemy commoners separately.
#[inline(always)]
fn square_attacked_by(board: &Board, sq: Square, occupied: Bitboard, enemy_bb: Bitboard) -> bool {
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
    if !(attacks::knight_attacks(sq) & board.by_type[PieceType::Knight as usize] & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Bishop/queen sliders.
    if !(attacks::bishop_attacks(sq, occupied)
        & (board.by_type[PieceType::Bishop as usize] | board.by_type[PieceType::Queen as usize])
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    // Rook/queen sliders. This is skipped when a pawn, knight, bishop, or
    // queen already hit.
    if !(attacks::rook_attacks(sq, occupied)
        & (board.by_type[PieceType::Rook as usize] | board.by_type[PieceType::Queen as usize])
        & enemy_bb)
        .is_empty()
    {
        return true;
    }

    false
}
```

### 2.2 Castling call site

The private `attackers_to(...) | commoner_attacks` expression was replaced by:

```rust
let enemy_commoner_attack = attacks::king_attacks(sq)
    & self.by_type[PieceType::Commoner as usize]
    & self.by_color[them as usize];
if square_attacked_by(self, sq, occupied, self.by_color[them as usize])
    || !enemy_commoner_attack.is_empty()
{
    return false;
}
```

The outer `adjacent_enemy_commoners.is_empty()` guard and the pass-through loop are
unchanged.

### 2.3 Pseudo-royal call site

```rust
if adjacent_enemy.is_empty()
    && square_attacked_by(self, ksq, occupied, enemy_survivors)
{
    return false;
}
```

The surrounding commoner loop, `enemy_survivors` construction, and `adjacent_enemy` guard
are unchanged.

### 2.4 Unchanged items

- Public `Board::attackers_to` was **not** changed.
- `Board::legal` remains the same public function; only the two predicate substitutions
  above changed.
- `is_move_trivially_legal`, `compute_checkers`, `compute_pinned`, `populate_state`,
  `do_move`, and `undo_move` were not changed.
- `src/movegen.rs`, `src/lib.rs`, `src/types.rs`, tests, and examples were not changed.
- No `StateInfo` fields, capture pre-filter, or local `Board::legal` cache was added.

---

## 3. Measurements

All fixed-FEN timings are wall-clock measurements around
`target/release/examples/perft FEN 6`, taken with `date +%s%N`. Node counts were unchanged
throughout:

- Test #13: `2,160,817,389`
- Test #14: `245,931,633`

FEN #13:
`rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5`

FEN #14:
`r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15`

### 3.1 Round 1 — initial baseline versus patched tree

| Run | Baseline #13 (ms) | Patched #13 (ms) | Baseline #14 (ms) | Patched #14 (ms) |
|-----|------------------:|-----------------:|------------------:|-----------------:|
| 1 | 11950 | 11445 | 1456 | 1466 |
| 2 | 11903 | 11514 | 1469 | 1458 |
| 3 | 11788 | 11458 | 1457 | 1459 |
| 4 | 11630 | 11509 | 1469 | 1461 |
| 5 | 11609 | 11494 | 1472 | 1481 |
| 6 | 11519 | 11489 | 1482 | 1457 |
| 7 | 11639 | 11571 | 1474 | 1455 |
| **Median** | **11639** | **11494** | **1469** | **1459** |

Round 1 change:

- #13: **−1.25 %**
- #14: **−0.68 %**

### 3.2 Round 2 — same-session baseline/patched A/B recheck

Because absolute timings drifted between sessions, the baseline source was restored,
rebuilt, and remeasured. The patched source was then restored, rebuilt, and remeasured.

Baseline recheck (original tree):

| Run | #13 (ms) | #14 (ms) |
|-----|---------:|---------:|
| 1 | 11459 | 1458 |
| 2 | 11446 | 1463 |
| 3 | 11441 | 1479 |
| 4 | 11517 | 1463 |
| 5 | 11485 | 1468 |
| 6 | 11518 | 1464 |
| 7 | 11484 | 1463 |
| **Median** | **11484** | **1463** |

Patched recheck:

| Run | #13 (ms) | #14 (ms) |
|-----|---------:|---------:|
| 1 | 11275 | 1439 |
| 2 | 11294 | 1438 |
| 3 | 11437 | 1449 |
| 4 | 11300 | 1437 |
| 5 | 11280 | 1438 |
| 6 | 11348 | 1430 |
| 7 | 11302 | 1431 |
| **Median** | **11300** | **1438** |

Round 2 change versus same-session baseline:

- #13: **−1.60 %**
- #14: **−1.71 %**

Both rounds show an improvement on both fixed FENs. No fixed FEN shows a stable
regression.

### 3.3 Full suite

| Tree | `verify_perft` total | Result |
|------|---------------------:|--------|
| Patched | 75.270 s | 41/41 passed |
| Baseline (after restore/recheck) | 76.381 s | 41/41 passed |

The patched suite was about **1.45 %** faster. The full suite is noisy on this host, as
noted in the analysis and earlier reports, so this is supporting evidence only. The
fixed-FEN medians above were the acceptance basis.

### 3.4 Did the expected gain reproduce?

Yes, and slightly better than the plan’s expectation.

The analysis standalone probe measurement was about **−0.9 % on #13** and **−0.6 % on
#14**. On this host the same-session A/B recheck measured **−1.60 % on #13** and
**−1.71 % on #14**. Unlike Plans 1 and 2, the expected direction reproduced for both
profiled positions.

---

## 4. Correctness

All required checks passed with the patched tree:

- `cargo fmt` — passed.
- `cargo clippy --all-targets` — no new warnings. The only warnings were the pre-existing
  `manual_range_contains` / `manual_contains` suggestions in `examples/test_d5g8.rs`.
- `cargo test` — all tests passed:
  - 54 unit tests
  - `move_order_golden`: 1 passed
  - `perft_tests`: 4 passed
  - `verify_moves`: 1 passed
  - doc tests: 1 passed
- `cargo doc` — passed.
- `cargo run --release --example verify_perft` — **41/41 passed** at depths 1–6.
- `tests/move_order_golden.rs` stayed green, confirming move generation and public move
  ordering are unchanged.

After the temporary baseline A/B restore, the patched file was copied back and its
checksum confirmed identical to the already-verified patched version before rebuilding.
No additional source changes were made.

---

## 5. Decision

**Kept.**

The acceptance criteria from the plan were met:

- Test #13 median improved by more than ~0.5 %: **−1.25 %** in round 1 and **−1.60 %** in
  the same-session recheck.
- Test #14 did not regress: **−0.68 %** in round 1 and **−1.71 %** in round 2.
- Neither fixed FEN showed a stable regression across the two rounds.
- The full suite passed 41/41 and was about 1.45 % faster in the supporting measurement.

The change is small, private, and does not alter public behavior.

---

## 6. Profile Evidence and Next Candidate

No new `perf` profile was captured after the change. Performance was accepted on repeated
fixed-FEN wall-clock measurements, as required by the plan; profiling was optional and was
not needed to resolve the decision.

The next item in analysis §8 is **§5.3, category-specialized legality**. However, Plan 3
was the first of the three perf-analysis-7 plans to reproduce its expected gain on this
host. Before attempting §5.3, re-baseline both fixed FENs and capture a fresh profile of the
current tree, because absolute host timings drifted noticeably between measurement sessions
and earlier code-layout surprises remain a risk.

---

## 7. Notes and Constraints Observed

- The helper ordering follows analysis §5.4: pawns, knights, bishop/queen, rook/queen.
- The queen is included in both slider categories, matching the prior helper.
- The pawn direction still uses `board.side_to_move`, matching the prior private helper.
- Commoner attacks are still excluded from the probe; they are handled separately at the
  call sites.
- The public `Board::attackers_to`, public `Board::legal` signature, `generate_legal`,
  `perft`, and move ordering are unchanged.
- No other optimization from the analysis was bundled into this change.
