# What can `shakmaty` still teach us about *speed*? (2.7.0 re-assessment)

**Question:** `docs/plans/shakmaty/analysis.md` concluded that `shakmaty` is a
correctness/API oracle, not a speed oracle. The tree has moved on since then
(2.3.1 → 2.7.0: `PositionView`, the fast-legality ladder, `EffectContext`,
`unsafe-optimizations`), and handoff 3 has been planned. Can we still take
inspiration from `shakmaty` to increase performance?

**Short answer: almost not.** One remaining `shakmaty` idea is real and is worth
a small, feature-gated change: leave the *unused tail* of `MoveList`
uninitialized, like `ArrayVec`/`shakmaty`'s `build_move_list` path does.
Measured in this repository it is worth **≈4 % perft**, needs `unsafe` and is
therefore confined to the existing `unsafe-optimizations` feature. A second
idea — `shakmaty`'s bitboard-shift pawn generation — is also worth **≈4 %**,
but it changes the public move order; the order-preserving port of it is
**≈9 % slower**, so it is rejected. Everything else in `shakmaty` is either
already used, not applicable, or a pessimisation on this machine.

This document is the measurement record behind `plan2.md`. No crate code was
changed for it; all experiments ran in throwaway copies of the 2.7.0 tree.

---

## 1. What was re-read

`shakmaty` 0.30.2 (`/tmp/shakmaty`, 17,569 lines under `src/`), with the focus
on the parts that touch execution speed:

- `src/attacks.rs` — magics (`Magic` table, unchecked `ATTACKS[idx]`),
  hyperbola-quintessence fallback, `attacks()` dispatch.
- `src/board.rs` — `by_color` / `by_role` representation, `attacks_from`,
  `attacks_to`.
- `src/m.rs` — `Move`, `MoveList = ArrayVec<Move, 270+>`, `build_move_list`
  (`MaybeUninit` + `#[inline(never)]` for RVO).
- `src/position.rs:3792-3990` — the generic `Stepper`/`Slider` movegen and
  `gen_pawn_moves` (bitboard shifts), plus the atomic `legal_moves` filter
  (`clone` + `play_unchecked` + `king_attackers`).
- `src/perft.rs` — clone-per-node recursion.
- `src/bitboard.rs`, `src/square.rs`, `src/zobrist.rs`, `benches/`.

The earlier analysis's structural findings still hold: the 2.3–2.8× perft gap
comes from `shakmaty`'s clone-per-candidate legality filter, not from its data
structures. The re-read added no new *algorithmic* candidate.

---

## 2. Measurement setup (and a trap worth recording)

- Throwaway copies of `atomic-movegen` 2.7.0, `[profile.release]` as shipped
  (`lto = "fat"`, `codegen-units = 1`, `opt-level = 3`).
- A local `perf_probe` example runs perft on four positions, best of 7:

  | Position | Depth | Nodes |
  | --- | ---: | ---: |
  | start position | 6 | 118,926,425 |
  | `tests/perft_values.md` #2 | 5 | 51,225,398 |
  | `tests/perft_values.md` #13 | 5 | 53,833,718 |
  | `tests/perft_values.md` #33 | 5 | 54,748,199 |
  | **total** | | **278,733,740** |

  All variants produced these exact counts (`verify_perft`-equivalent
  equality), so no time was measured on a wrong answer.
- A/B runs are **ABBA-interleaved** (A, B, B, A per round, 3–4 rounds) and
  reported as medians of the per-run total. Absolute ms are informational;
  the ratio is the result.

> **Trap:** the first experiments ran A then B in every round. With the CPU
> governor on `powersave`, that produced a *stable-looking* 4 % “win” for
> control builds whose source was identical to A. Re-running the same binaries
> ABBA made the difference disappear. Every number below is ABBA; anything
> measured before that is discarded. `docs/perf/perf_guide.md` should say this
> explicitly (plan 2 does).

No `perf`/`callgrind` instruction counts were available in this container
(`perf_event_paranoid=2`, no `valgrind`), so wall time is all we have. Wall
time at this resolution cannot resolve <1 % differences.

---

## 3. Results

Baseline (2.7.0, default features) medians: **≈1,385–1,400 ms** for the
278.7 M-node probe in the runs below.

| # | `shakmaty` idea / shape | Result vs baseline | Verdict |
| --- | --- | ---: | --- |
| 1 | **Uninitialized move-list tail** (`MoveList` stores `MaybeUninit<Move>` and only writes the first `len` slots; `shakmaty` gets this from `ArrayVec`) | **1,331–1,338 ms (≈0.96×)** | **Adopt**, feature-gated |
| 2 | #1 together with the existing `unsafe-optimizations` magic index | 1,309–1,350 ms (≈0.95×) | Ships with #1; see #3 |
| 3 | **Unchecked magic index** (already in `unsafe-optimizations`, `shakmaty` claims ≈2 %) | 1,408–1,415 ms (**≈1.02× slower**) | Keep the feature, but do not claim a win here |
| 4 | **Bitboard-shift pawn generation** (`shakmaty`'s `gen_pawn_moves`: shift the pawn set, iterate target squares) | 1,339–1,350 ms (**≈0.965×**) | Real, but changes move order → rejected for now |
| 5 | Order-preserving variant of #4 (per-pawn walk, node-constant shifted target sets) | ≈1,509 ms (**≈1.09× slower**) | Rejected |
| 6 | `#[inline(always)]` on all `Bitboard` operators/`count`/`is_empty` | 1,368–1,403 ms (no change) | Already inlined |
| 7 | Reuse one `MoveList` per depth in `perft` (safe, avoids `new` per node) | 1,408–1,433 ms (**≈1.03× slower**) | Rejected; the live lists cost more than the zeroing |

Notes per row:

1. **The one win.** `MoveList` is `[Move; 256]` + `len` = 520 bytes; `new()`
   writes all 520 bytes even though a node writes ~30 and reads `len`. Storing
   `[MaybeUninit<Move>; MAX_MOVES]` (a sound representation — see plan 2,
   `assume_init` on an uninitialized integer array is *not* sound) and reading
   only the initialized prefix removes that. The gain shows on all four
   positions (start +5 %, #2 +3 %, #13 +3 %, #33 +2 %) and survives ABBA. It
   is `unsafe`, so it belongs behind `unsafe-optimizations`, exactly like the
   magic lookup.
2. Combining with the magic feature does not lose the gain on this machine,
   but row 3 says the magic change alone is slightly negative; the combined
   number is not a reason to enable the feature for magic.
3. `shakmaty`'s comment (`attacks.rs`) says the unchecked index is “worth about
   2 % in move generation and perft”. On this container it is consistently
   ~1 % *slower*; this matches the earlier analysis's ARM note (magics gain
   ~5 % on ARM, the bounds check is well predicted). Feature stays for users
   on other targets; no change recommended.
4. `shakmaty`'s pawn generator shifts the whole pawn set and iterates the
   *target* bitboard for pushes, and shifted captures per direction. That is
   ~4 % faster here — but it emits moves in target-square order, which breaks
   this crate's documented generation-order contract (per pawn, ascending
   origin; see `for_each_pseudo_legal` and `tests/move_order_golden.rs`).
   The contract may only change in a major release, and 4 % is not worth a
   major release. An opt-in “unordered” generator could exist later; it is not
   in this plan.
5. Keeping the contract forces a per-pawn walk. Precomputing the shifted
   target sets per node and indexing them per pawn adds branches and square
   arithmetic that cost more than the three attack-table loads it replaces.
6. LLVM already inlines these; forcing it changes nothing.
7. Even though `new()` is the only per-node zeroing, reusing lists makes the
   whole per-depth stack live across recursion (6 × 520 bytes), which costs
   more in cache/stack traffic than it saves. This also explains why the win
   in row 1 is *not* simply “any way of skipping the zeroing”.

---

## 4. What is *not* worth copying (confirmed)

Unchanged from `analysis.md`, re-confirmed against 2.7.0:

- **Clone-per-candidate legality** (`shakmaty`'s atomic `legal_moves` and
  perft). This is the bulk of its 2.3–2.8× deficit. Nothing to learn.
- **`Move` as an 8-byte enum** (role/capture/promotion). Ours is 2 bytes.
- **`MoveList` capacity 450** (3,608 bytes) vs our 256 (520 bytes).
- **No undo** (`play_unchecked` + clone). We have `do_move`/`undo_move`.
- **`build_move_list`'s `MaybeUninit` + RVO trick** is about returning a huge
  list by value; our API takes `&mut MoveList`, so there is no return copy.
  The only reusable part is the *content* of row 1 (do not initialize the
  tail), not the RVO scaffolding.
- **Hyperbola quintessence** is a footprint/no-`unsafe` fallback, not faster
  than magics where magics are good.
- **Fixed 12/9-bit magic indices with an 88,772-entry table** (≈710 KiB) vs our
  per-square index widths (≈344 KiB). Bigger table, no speed argument.
- **`attacks_to` via attacks-from-the-square**: we already do exactly that in
  `PositionView::checkers` and `is_attacked_by`.
- **Generic `Stepper`/`Slider` traits** disappear after monomorphisation; our
  explicit per-piece calls are equivalent.

The remaining `shakmaty` ideas that are genuinely good are feature-level, not
speed: `capture_moves()`, `legal_ep_square()`/`EnPassantMode`, `Move::from_uci`
and the `validate()` error bitflags (already partially adopted). None of them
is in this plan; they do not move perft.

---

## 5. What this means for the planned work

- **Handoff 3 plans 1–3 are unaffected.** `MG19`/`MG20` are about the child
  derivation and the existential query; `shakmaty` has no equivalent API (it
  clones to ask any child question). Nothing here should be folded into those
  plans — the index forbids growing them, and the measured costs there
  (20–40 ns per child) are an order of magnitude above the ≈4 % perft effects
  here.
- **One separate, small plan is justified:** `plan2.md` — the feature-gated
  uninitialized tail in `MoveList`, plus the ABBA methodology note in
  `docs/perf/perf_guide.md` that this session needed. Expected effect: ≈4 %
  perft with `--features unsafe-optimizations`, no change to the default build
  (the default path keeps `[Move; MAX_MOVES]` and the identical zeroing).
- **Rejected, recorded so it is not re-proposed without new numbers:** the
  unordered bitboard pawn generator (≈4 % but a move-order break) and any
  “make the magic lookup faster” work on this target (it is not faster here).

---

## 6. Suggested next step

Implement `plan2.md`, then re-run the same ABBA probe **with the feature on and
off** and publish both numbers in the report. If the feature-on gain is not
reproducible in the committed tree, the plan says to drop the change rather
than keep an `unsafe` that buys nothing. The default build must be measured to
be unchanged (same median, same perft values, same move order, same hashes).
