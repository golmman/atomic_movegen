# Report 1: EVASIONS / NON_EVASIONS Move-Generation Split

## Goal

Reduce pseudo-legal move generation in check positions by implementing a
targeted `generate_evasions()` function, avoiding the cost of generating and
filtering all pseudo-legal moves when the side to move is in check.

## Approach

1.  Add `generate_evasions()` to `src/movegen.rs` that replaces
    `generate_pseudo_legal()` in the check path of `generate_legal()`.
2.  Generate only:
    - King (commoner) escapes (always).
    - Non-commoner moves to target squares:
      - The checker's square (direct capture).
      - `between_bb(king, checker)` for slider checks (interposition).
      - `king_attacks(checker)` ∩ enemy pieces (blast-destruction via adjacent
        capture).
      - Extinction-win captures that destroy the last enemy commoner.
3.  When **any checker is a commoner** (adjacency check), fall back to full
    pseudo-legal generation because `legal()` treats commoner-adjacency as
    safety and does *not* reject moves that leave the adjacency unresolved.
4.  For **double checks** (non-commoner only), the intersection zone
    `⋂ᵢ ({sqᵢ} ∪ king_attacks(sqᵢ))` — squares where a single capture
    destroys *all* checkers through direct capture and/or blast — is added
    to the target set.

## Result

- **All 41 perft positions pass at depths 1–6** (verified `--release`).
- **~5–6 % performance regression** (76 s → 80 s on `verify_perft 6`).

## Analysis of the Regression

The regression is uniform across all tests and consistent across multiple
runs, so it is not from double-check positions specifically but from the
overall code structure change in `generate_legal()`.

| Version | Total time | vs baseline |
|---------|------------|-------------|
| Original (commit `5a63140`) | ~76 s | — |
| EVASIONS + early-return + `#[inline(never)]` | ~80 s | +5.6 % |

### Root cause hypothesis

The original `generate_legal()` is a compact, single-path function:

```
populate_state → generate_pseudo_legal → is_trivially_legal || legal
```

The compiler inlines `generate_pseudo_legal` into it (or at least optimises
the two together as one unit). The replacement adds a branch — the hot
non-check path goes through the `else` branch, while the cold check path
takes the early return through `generate_evasions`.  Even with
`#[inline(never)]` on `generate_evasions` and an early-return structure that
isolates the check path, the compiler still treats `generate_legal` as a
larger function, which changes code-layout and inlining decisions for the
hot non-check path.

Attempted mitigations that did *not* close the gap:
- `#[inline(never)]` on `generate_evasions` (recovered ~3 s of the gap).
- Early-return structure that keeps the non-check path verbatim from the
  original.
- Moving `attacks_for_pt` and helper functions after `generate_legal`.
- `#[inline(always)]` on `generate_pseudo_legal` (made it worse).

### Possible future improvements

- Extract the check path into a `#[inline(never)] fn generate_legal_in_check`
  inside `board.rs` so the hot path in `movegen.rs` stays identical to the
  original source.
- Mark the check-path body with `#[cold]` to hint the compiler to optimise
  for the non-check layout.
- Profile the original to find the real bottleneck (pseudo-legal generation,
  `legal()` filtering, or `populate_state` / `compute_checkers`) and target
  the optimisation there instead.

## Key findings

1. **Double-check fix.**  The original EVASIONS code (without the
   zone-intersection squares) missed captures that resolve a double check by
   directly capturing one checker while the blast destroys the other (facing
   two queens adjacent to each other).  The fix adds
   `(⋂ᵢ {sqᵢ} ∪ king_attacks(sqᵢ)) ∩ enemy_pieces` to the target set for
   double checks.

2. **Commoner-checker fallback.**  When *any* checker is an enemy commoner
   (king adjacency), `legal()` does *not* reject non-king moves that leave
   the adjacency unresolved (it treats commoner adjacency as safety).  This
   means the restricted target approach would miss legal moves, so the code
   falls back to full pseudo-legal generation in that case.

3. **`find_discrepancy` bug.**  The debug tool returned `0` both when it
   found a mismatch *and* when a checkmate position had zero legal moves,
   causing false positives.  Fixed by using a separate sentinel value.

## Files changed (reverted)

- `src/movegen.rs` — added `attacks_for_pt`, `generate_pawn_evasion_moves`,
  `generate_evasions`, modified `generate_legal`.
- Stale debug examples removed.
