# Plan 6 — Node-Level Commoner-Bitboard Cache in `StateInfo`

**Corresponds to:** Item 7 of `docs/plans/perf_analysis7/analysis.md` — §5.6, *Retry
commoner-bitboard caching alone, not with a pre-filter* (and the first entry of `report5.md` §9)
**Estimated speedup:** ~0.5–1.5 % on the profiled positions (analysis §5.6 estimates ~1–2 %); the
upper bound is deliberately treated as unverified after four layout-sensitive results in this series
**Risk:** Low–medium — adds two public `StateInfo` fields that must be populated together with the
existing hot fields, and relies on the already-documented "`state` must be populated" contract
**Effort:** Small–medium — roughly 60–90 changed lines in `src/board.rs`, ~10 in `src/movegen.rs`

---

## 1. Context

Plans 1–5 covered analysis §8 items 1–6:

| Plan | Item | Result |
|------|------|--------|
| 1 (`report1.md`) | §5.1 perft fast path without compaction | Reverted — regressed ~3 % on #14 |
| 2 (`report2.md`) | §5.2 local bitboard cache in `Board::legal` | Reverted — stable ~+0.2–0.9 % on #13 |
| 3 (`report3.md`) | §5.4 boolean `square_attacked_by` early exit | **Kept** |
| 4 (`report4.md`) | §5.3 category-specialized legality | Reverted — ≤0.6 % on #13, tree restored |
| 5 (`report5.md`) | §5.5 order-preserving table-driven pawn generation | **Kept** — −6.9 % on both #13 and #14 |

The tree therefore contains Plan 3 and Plan 5 only; `git status` is clean.

Analysis §8's remaining items are exactly item 7: *"Polish with `compute_pinned`/empty-board
attacks/`occupied` cache if the profile still shows those costs."* §5.6, §5.7/§5.9 and §5.8 are the
concrete candidates. `report5.md` §9 orders them:

1. **§5.6 — `StateInfo` commoner-bitboard cache**, because `is_move_trivially_legal`,
   `compute_checkers` and `compute_pinned` are now the largest remaining movegen-side blocks
   (~14 %, ~3.3 %, ~1 % of the post-Plan-5 profile) and all of them recompute
   `by_color & by_type` commoner bitboards.
2. §5.7/§5.9 — `compute_pinned` with the `slider_blockers` pattern and precomputed empty-board
   slider attacks (smaller, ~1 %).
3. A fresh profile before choosing between §5.6 and a castling fast path, because `generate_castling`
   owned the top remaining `movegen.rs` source lines after Plan 5 and that may be attribution noise.

This plan takes item 1 and folds the "fresh profile first" precondition into its Step 1, so the
decision to spend effort on §5.6 is itself evidence-based. §5.7/§5.9/§5.8 stay out of scope (§9).

Two earlier data points constrain the design:

- **Plan 2 (`report2.md`) already tried a local cache inside `Board::legal` and reverted.** That
  version recomputed the bitboards at every `legal` call, i.e. up to several times per node, and
  changed nothing outside one function. Its neutral/slightly negative result is the reason §5.6 must
  cache at the **node** level, once per `populate_state`, not per candidate move.
- **`perf_analysis6/report2.md` tried the same cache bundled with a capture blast pre-filter and
  measured a 5.17 % regression.** Analysis §5.6 explicitly says to retry the cache *alone*. This
  plan therefore adds no pre-filter, no `Board` field, and no new `legal` branch.

Post-Plan-5 the pawn stage is ~10.3 % and no longer dominant, so the remaining profile is
legality-dominated (`Board::legal` self time, `is_move_trivially_legal`, `compute_checkers`,
`compute_pinned`). That is exactly what §5.6 touches.

---

## 2. Problem

`Board::commoners` (`src/board.rs:847`) is two array loads and an `and`:

```rust
pub fn commoners(&self, c: Color) -> Bitboard {
    self.pieces_color_pt(c, PieceType::Commoner)
}
```

and it is called repeatedly for the *same* position inside a single node:

| Site | Calls per node |
|------|----------------|
| `populate_state` (`board.rs:1023`) → `compute_checkers` (own + enemy) → `compute_pinned` (own) → two `commoners_count` fields | 5 |
| `Board::legal` (`board.rs:1310`): castling adjacent-commoner probes (up to 2, enemy), `our_commoners` base (own), `enemy_pr_destroyed` probe (enemy), survival-loop `them_commoners` (enemy) | up to 5 per **call** |

`Board::legal` is not called once per node: it is called for every candidate that is not
trivially legal — every capture, every en-passant, every commoner move, every pinned-piece move,
every castling move, and (because `state.checkers` is non-empty) **every** candidate in check. On
the check-heavy test #13 that is most of the pseudo-legal stream, which is why analysis §3.2/§3.3
report `Board::legal` at ~20 % self time on both profiled positions and `pieces_color_pt`
(`board.rs:556`) at 3.4–4.2 % of source-line samples.

The information never changes within a node: `populate_state` computes the whole legality state once
and `Board::legal` only reads it. `StateInfo` already caches `checkers` and `pinned` from the same
computation; the two commoner bitboards are the only hot inputs still re-derived per candidate.

The `commoners_count` / `them_commoners_count` fields (`board.rs:94`, `board.rs:96`) already prove
the intent — they exist so `legal` does not have to recompute the commoner sets — but they only cache
the *size*, not the *set*, so every emptiness or intersection test still pays for
`pieces_color_pt`.

---

## 3. Goal

1. Cache the two commoner bitboards of the *current* position in `StateInfo`, next to the existing
   hot fields:

   ```rust
   pub commoners: Bitboard,       // side to move's commoners, pre-move
   pub them_commoners: Bitboard,  // opponent's commoners, pre-move
   ```

2. Populate them **exactly once per node** in `Board::populate_state`, and thread them into
   `compute_checkers` / `compute_pinned` so that the state population itself no longer re-derives
   them.

3. Consume the cached bitboards in every hot reader: `Board::legal`, `is_move_trivially_legal`, and
   `movegen::has_legal_move_with_state`. After the change no hot path calls
   `Board::commoners()` for the side to move / opponent of the current node.

4. Preserve exact semantics: identical legal-move sets, identical move order, identical perft node
   counts, identical public API behaviour. The existing public fields `commoners_count` and
   `them_commoners_count` stay (no field removal, no rename), and are derived from the same cached
   bitboards in one place so they cannot disagree with them.

5. Add no work to any path: no new branch in `Board::legal`, no capture pre-filter, no `Board`
   field, no extra `commoners()` call anywhere.

---

## 4. Design

### 4.1 `StateInfo` fields (`src/board.rs:88`)

Add the two bitboards at the **top** of the struct, immediately after `checkers`/`pinned` and
before the counts, so the four hot 8-byte fields stay contiguous:

```rust
#[derive(Debug, Clone, Copy)]
pub struct StateInfo {
    /// Enemy pieces attacking the side to move's commoners (hot).
    pub checkers: Bitboard,
    /// Pieces of the side to move that are pinned to a commoner (hot).
    pub pinned: Bitboard,
    /// Commoners of the side to move before the move (hot).
    pub commoners: Bitboard,
    /// Commoners of the opponent before the move (hot).
    pub them_commoners: Bitboard,
    /// Number of commoners the side to move has on the board (hot).
    pub commoners_count: u32,
    /// Number of commoners the opponent has on the board (hot).
    pub them_commoners_count: u32,
    ...
}
```

`StateInfo::new()` (`board.rs:126`) initialises both to `Bitboard::EMPTY`.

Notes:

- Keeping the two `*_count` fields rather than deriving them with `count()` at use sites avoids
  adding popcounts to `Board::legal`'s two `<= 1` guards, and keeps the public surface additive
  instead of removing documented fields. The counts are assigned from the cached bitboards in
  `populate_state`, so the redundancy is confined to one writer.
- The struct grows by 16 bytes (~56 → ~72 bytes). `StateInfo` is `Copy` and `StateInfo::new()` runs
  once per `generate_legal` call, so this adds two `u64` stores to node setup. §4.5 gives the fallback
  if that turns out to matter.
- The new fields are `pub`, so they need doc comments (`#![warn(missing_docs)]`), and the struct's
  own doc comment must mention that they are populated by `populate_state` like the other hot
  fields.
- Adding public fields is technically a breaking change for downstream code that builds
  `StateInfo` with a struct literal. The crate is at `2.3.0`; the plan requires a note in
  `report6.md` (a future release note should mention it), but no version bump is performed here.

### 4.2 Single population point (`board.rs:1023`)

`populate_state` becomes the only writer of the four hot fields and derives everything from one
pair of bitboard loads:

```rust
pub fn populate_state(&self, state: &mut StateInfo) {
    let us = self.side_to_move;
    state.commoners = self.commoners(us);
    state.them_commoners = self.commoners(us.flip());
    state.commoners_count = state.commoners.count();
    state.them_commoners_count = state.them_commoners.count();
    state.checkers = self.compute_checkers_with(us, state.commoners, state.them_commoners);
    state.pinned = self.compute_pinned_with(us, state.commoners);
}
```

Ordering note: `checkers`/`pinned` are computed *after* the bitboards are stored, so the existing
consumers see the same values as before (they are independent of the bitboards' storage).

### 4.3 `compute_checkers` / `compute_pinned` take the cached bitboards

Split each into a thin bitboard-loading wrapper plus a `_with` variant, keeping the existing
`pub(crate)` names working for the public `checkers()` / `pinned()` accessors:

```rust
pub(crate) fn compute_checkers(&self, us: Color) -> Bitboard {
    self.compute_checkers_with(us, self.commoners(us), self.commoners(us.flip()))
}

pub(crate) fn compute_checkers_with(
    &self,
    us: Color,
    commoners: Bitboard,
    them_commoners: Bitboard,
) -> Bitboard {
    // body unchanged, except:
    //   let commoners = self.commoners(us);        // deleted
    //   let them_commoners = self.commoners(them); // deleted
    ...
}

pub(crate) fn compute_pinned(&self, us: Color) -> Bitboard {
    self.compute_pinned_with(us, self.commoners(us))
}

pub(crate) fn compute_pinned_with(&self, us: Color, commoners: Bitboard) -> Bitboard {
    // body unchanged, except: let commoners = self.commoners(us); deleted
    // (`us` is still needed for the `us.flip()` sniper colour filter.)
    ...
}
```

`Board::checkers()` and `Board::pinned()` (the public conveniences) are unchanged in behaviour and
still compute fresh values.

### 4.4 Consumers switch to the cached bitboards

**`Board::legal` (`board.rs:1310`)** — four substitutions, all semantically identical because
`legal` does not mutate the board and `state` is contractually populated for the current position:

| Line | Before | After |
|------|--------|-------|
| `1333` | `self.commoners(them) & attacks::king_attacks(sq)` | `state.them_commoners & attacks::king_attacks(sq)` |
| `1372` | `self.commoners(us) & occupied` | `state.commoners & occupied` |
| `1387` | `(self.commoners(them) & occupied).is_empty()` | `(state.them_commoners & occupied).is_empty()` |
| `1390` | `let them_commoners = self.commoners(them);` | `let them_commoners = state.them_commoners;` |

Add one debug-only consistency guard at the top of `legal`, alongside the existing `piece ==
NO_PIECE` early return:

```rust
debug_assert_eq!(state.commoners, self.commoners(us), "StateInfo not populated");
debug_assert_eq!(state.them_commoners, self.commoners(them), "StateInfo not populated");
```

This is compiled out in release and makes the new "state must be populated" requirement explicit
for tests and for `cargo test` builds.

**`is_move_trivially_legal` (`board.rs:1579`)** — replace the count test with the equivalent
bitboard test so the fast path never touches `by_type`:

```rust
if state.commoners.is_empty() {
    return false;
}
```

(`commoners_count == 0` ⇔ `commoners.is_empty()`, and both are written together in
`populate_state`. `commoners_count` remains part of the public surface and is still read by
`legal`.)

**`movegen::has_legal_move_with_state` (`movegen.rs:324`)** — replace the direct board query with
the cached value:

```rust
if state.commoners.is_empty() {
    return false;
}
```

This is a strict improvement (it removes a `commoners()` call per `has_legal_move*` invocation) and
respects the documented contract that `state` is already populated.

**Unchanged:** `Board::outcome()` (`board.rs:860`) keeps using `self.commoners(...)`; it is a
user-facing game-outcome query, not a node-hot path, and must stay correct for a board with no
`StateInfo` at all.

### 4.5 Fallback design if `StateInfo` growth hurts

If Step 5 shows the change is neutral or negative, do **not** start tweaking the struct. Instead
report the result and stop (analysis §6 discipline: no retries without a new design). Only if the
profile shows the bitboards are *still* hot after the cache lands (i.e. the cache was right but the
struct change was wrong) should the fallback be considered in a **separate** plan:

- thread the two bitboards as explicit parameters through a new crate-private
  `Board::legal_with(&self, m, state, commoners, them_commoners)` called from
  `generate_legal_with_state`, with the public `Board::legal` computing them and delegating;
- leave `StateInfo` untouched.

This alternative keeps `StateInfo` at its current size and has no public-field change, but needs
extra parameters at two call sites and does not help external callers of
`generate_legal_with_state`. It is documented here only so that a future plan does not have to
re-derive the trade-off; it is **not** part of this plan's implementation.

### 4.6 What not to change

- No capture pre-filter, no `EVASIONS`/`NON_EVASIONS` split (analysis §6).
- No `Board::occupied` field (§5.8), no `slider_blockers`/`more_than_one` rewrite of
  `compute_pinned` (§5.7), no precomputed empty-board slider tables (§5.9) — those are separate
  items and must stay separately attributable.
- No change to `src/lib.rs::perft` (Plan 1 stays reverted) and no change to `generate_legal`,
  `generate_legal_with_state`, `has_legal_move`, `for_each_pseudo_legal`, `MoveList`, or move order.
- No change to `do_move` / `undo_move`: the new fields are *derived* state like `checkers` and
  `pinned`, and `do_move`'s documentation already says those are not populated there. State that
  the plan extends that sentence to cover the commoner bitboards.
- No removal or rename of `commoners_count` / `them_commoners_count`.
- No `unsafe`, no new dependencies.

---

## 5. Implementation Steps

### Step 1: Re-baseline and re-profile (decision gate)

Absolute timings drift between sessions (analysis baseline 8.46 s for #13, `report4.md` 11.3 s,
`report5.md` 11.77 s). Everything below must be same-session.

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

Expected node counts: #13 `2 160 817 389`, #14 `245 931 633` (`report5.md` §2). Record every run,
not only the median.

Then take one fresh profile of the **current** tree (this is `report5.md` §9 item 3 — settle whether
the commoner/legality block or castling is the real remaining cost before editing):

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
perf record -F 997 -g --call-graph dwarf -o /tmp/perf7-plan6-baseline.data \
  target/profiling/examples/perft "$F13" 6
perf report -i /tmp/perf7-plan6-baseline.data --stdio --no-children --sort=symbol
perf report -i /tmp/perf7-plan6-baseline.data --stdio --children --sort=symbol
perf report -i /tmp/perf7-plan6-baseline.data --stdio --no-children --sort=srcline | head -40
```

**Decision gate.** Proceed with Steps 2–5 only if the fresh profile still shows the
commoner/legality block dominating — concretely, `Board::legal` self ≥ ~10 %, `pieces_color_pt`
(`board.rs:556`) ≥ ~2 % of source-line samples, or `is_move_trivially_legal` + `compute_checkers` +
`compute_pinned` ≥ ~15 % combined. If instead `generate_castling` or the `MoveList` push dominate and
the commoner lines are below ~1 %, **stop**, write `report6.md` with the negative/redirected result
and recommend re-ordering §8 (castling fast path or `MoveList` work) instead of implementing this
plan.

This gate is not a formality: Plan 2 failed while the analysis still showed the same target as hot,
and Plan 5 succeeded because the target really was ~17 % of runtime.

### Step 2: Add the cached fields (behaviour no-op)

1. Add `commoners` / `them_commoners` to `StateInfo` (`board.rs:88`) with doc comments, and to
   `StateInfo::new()` (`board.rs:126`).
2. Add `compute_checkers_with` / `compute_pinned_with` (§4.3) and make the existing
   `compute_checkers` / `compute_pinned` thin wrappers.
3. Rewrite `populate_state` (§4.2) to write all four hot fields.

At this point every consumer still reads `self.commoners(...)`/counts, so behaviour is identical by
construction. Verify with:

```sh
cargo build
cargo test
```

### Step 3: Switch the consumers

Apply §4.4: four substitutions plus two `debug_assert_eq!`s in `Board::legal`, the bitboard
emptiness test in `is_move_trivially_legal`, and the cached test in
`has_legal_move_with_state`. Update the `populate_state`/`do_move`/`StateInfo` doc comments to list
the commoner bitboards among the populated (non-undo) fields.

Grep to confirm no hot-site `commoners()` remains:

```sh
grep -n "commoners(" src/board.rs src/movegen.rs
```

Expected survivors only: the `Board::commoners` definition, `Board::outcome`,
`compute_checkers`/`compute_pinned` wrappers, and `populate_state`.

### Step 4: Correctness checks

```sh
cargo fmt
cargo clippy --all-targets
cargo test
cargo doc
cargo run --release --example verify_perft
cargo test --test move_order_golden
cargo test --test verify_moves
```

Required results:

- `cargo clippy --all-targets` shows no new warnings (the four pre-existing
  `manual_range_contains`/`manual_contains` warnings in `examples/test_d5g8.rs` are unrelated).
- All lib/unit/integration tests pass. **Specifically the `#[cfg(test)]` module in `board.rs`** must
  still pass, so add/extend a unit test that builds a two-commoner board, calls `populate_state`,
  and asserts `state.commoners == board.commoners(us)`, `state.them_commoners ==
  board.commoners(us.flip())`, and that the count fields match `.count()` — this is the
  single-writer contract from §4.2.
- `verify_perft` **41/41** at depths 1–6.
- `move_order_golden` passes with **unchanged goldens** (the fields cannot affect order; this is the
  guard proving it).
- `cargo doc` is warning-free (new public fields are documented).

Optional but cheap: a test that `StateInfo::new()` (unpopulated state) still yields the same
`Board::legal` answers as a populated one for the empty-commoner edge case, i.e. that the
`debug_assert` is not reachable from the library's own `StateInfo::new()`-then-`legal` sequences.
If any internal caller sequences `StateInfo::new()` → `legal` without `populate_state`, the debug
assert will fire under `cargo test`; treat that as a **bug in the caller** and fix the caller, or
document in the plan report why the assert had to be narrowed.

### Step 5: Performance verification

Rebuild and repeat the Step 1 fixed-FEN loops in the same session, then run the A/B round
(`git stash` the patch → baseline → `git stash pop` → candidate), as in `report5.md` §3.2.

Acceptance criteria (deliberately tighter than Plan 5 in relative terms because the expected gain is
small):

- **#13 median improves by ≥ 0.5 %** versus the same-session baseline, **and** the pairwise A/B runs
  are faster in at least 6 of 7 pairs.
- **#14 does not regress by more than 0.3 %** (an improvement is a bonus).
- Full suite stays 41/41; the suite total is supporting evidence only.
- If the gain is below 0.5 %, mixed, or #14 regresses beyond the threshold, **revert** `src/board.rs`
  and `src/movegen.rs` to the Plan 5 tree, rebuild, confirm the baseline is restored, and record the
  negative result. Do not stack §5.7/§5.9 on top to "rescue" the measurement.

If kept, re-profile and check that `pieces_color_pt` (`board.rs:556`) and `Board::legal` self time
dropped, and that `compute_checkers`/`compute_pinned` shares did not grow.

### Step 6: Write the implementation report

Create `docs/plans/perf_analysis7/report6.md` per `AGENTS.md`. It must contain:

- The Step 1 decision-gate profile result (why §5.6 was or was not the right target at this point).
- `git diff --stat` proving only `src/board.rs` and `src/movegen.rs` changed.
- All baseline, post-change and A/B fixed-FEN runs with medians and pairwise deltas.
- Full-suite result and perft node counts.
- Correctness table: `fmt`, `clippy --all-targets`, `test`, `doc`, `verify_perft` 41/41,
  `verify_moves`, `move_order_golden` (unchanged goldens), plus the new `StateInfo` consistency test.
- The measured `StateInfo` size change and whether the extra node-setup work was visible.
- Explicit keep/revert decision with the acceptance numbers it was judged against.
- The public-API note: two new `StateInfo` fields (additive; struct-literal construction breaks) and
  whether `commoners_count`/`them_commoners_count` should be deprecated in a later cleanup.
- If kept: profile evidence that the commoner/legality lines dropped.
- If reverted: what the profile says should be done instead (castling fast path, `MoveList`
  push cost, or §5.7/§5.9).
- Next candidate from analysis §8: §5.7/§5.9 (`slider_blockers` + precomputed empty-board slider
  attacks), then §5.8, or whatever the fresh profile redirects to.

---

## 6. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/board.rs` | Two new `StateInfo` fields + `new()` init; `populate_state` rewrite; `compute_checkers_with` / `compute_pinned_with` split; four `legal` substitutions + debug asserts; cached-bitboard test in `is_move_trivially_legal`; doc updates; one unit test. | ~60–90 |
| `src/movegen.rs` | `has_legal_move_with_state` uses `state.commoners.is_empty()`. | ~5–10 |
| `docs/plans/perf_analysis7/report6.md` | New implementation report. | New file |

No changes expected in `src/lib.rs`, `src/types.rs`, `src/attacks.rs`, `src/bitboard.rs`, `src/magic.rs`,
`examples/`, or `tests/`.

---

## 7. Correctness Contract

For every position and every populated `StateInfo`:

1. **Field consistency.**
   `state.commoners == board.commoners(board.side_to_move())`,
   `state.them_commoners == board.commoners(board.side_to_move().flip())`,
   `state.commoners_count == state.commoners.count()`,
   `state.them_commoners_count == state.them_commoners.count()`.
   Guaranteed by the single writer in `populate_state` and debug-asserted in `Board::legal`.
2. **Legality equivalence.** The four `legal` substitutions only replace `self.commoners(c)` by the
   same bitboard read from `state`. `legal` takes `&self`, so the board cannot change between
   population and use within a node; `generate_legal`, `generate_legal_with_state`, `perft` and the
   tests all populate through `Board::populate_state`.
3. **Fast-path equivalence.** `commoners_count == 0` ⇔ `state.commoners.is_empty()` (rule 1).
4. **Unpopulated state.** `StateInfo::new()` leaves the bitboards empty, so the
   `is_move_trivially_legal` fast path declines and `legal` returns `false` on any candidate for a
   state that was never populated — the same outcome as the previous `commoners_count == 0` path.
   No behaviour change for that misuse case; the debug assert turns it into a loud failure in tests.
5. **No mutation surface.** `do_move`/`undo_move` neither read nor write the new fields; they are
   derived state exactly like `checkers`/`pinned`.
6. **Pseudo-royal reasoning unchanged.** `legal`'s pre-move vs post-blast distinction is preserved:
   the cached bitboards are the *pre-move* sets (used for the `pr_count <= 1` guards and as the base
   of `our_commoners`/`enemy_pr_destroyed`), and `& occupied` still applies the post-move/post-blast
   filter.

Verification set: `verify_perft` (41 positions, depths 1–6), `verify_moves`,
`tests/move_order_golden.rs` (unchanged goldens), the `board.rs` unit tests including the new
consistency test, and `cargo test` in debug (which enables the new `debug_assert_eq!`s).

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Layout/register-pressure effect cancels the gain (precedent: Plans 1, 2, 4) | High | Step 1 decision gate, fixed-FEN medians, pairwise A/B round, explicit revert threshold (§5 Step 5). |
| `StateInfo` growth (two extra `u64`) makes node setup slower | Medium | `StateInfo::new()` runs once per `generate_legal`; measure end-to-end. §4.5 documents the parameter-threading fallback but it is **not** implemented in this plan. |
| Stale/incorrect cached bitboards if some caller mutates the board between populate and `legal` | Low | `legal` takes `&self`; all internal paths populate per node; `debug_assert_eq!` catches mistakes under `cargo test`. |
| New public fields break downstream struct-literal construction | Low (crate has no known external literal users; `StateInfo::new()`/`Default` exist) | Additive change; documented in `report6.md` for release notes. No field removal or rename. |
| `commoners_count` and the bitboards drift apart | Low | Single writer in `populate_state`; counts derived from the bitboards in the same statement block; unit test pins the relation. |
| Debug asserts change `cargo test` outcomes for a misusing internal caller | Low | Treat a firing assert as a caller bug and fix the caller (documented in Step 4). |
| Temptation to bundle §5.7/§5.9/§5.8 or the capture pre-filter | Medium | §4.6 and §9 make them explicitly out of scope; each must be measured alone (analysis §6, `perf_analysis6/report2.md`). |
| Full-suite noise hides a small regression | High | Decide on fixed-FEN medians and pairwise runs; suite total is context only. |
| Expected gain (1–2 %) does not materialise because the removed work was ~5 loads/`and` per node | Medium | The plan's own estimate is 0.5–1.5 %; a sub-threshold result is an accepted outcome with a documented revert, not a failure. |

---

## 9. Out of Scope / Follow-ups

- **§5.7 / §5.9 — `compute_pinned` `slider_blockers` + precomputed empty-board slider attacks.**
  The natural Plan 7. Requires a `more_than_one()` helper (the crate has none; `count() == 1`
  works but a `u64` trick is cheaper) and preferably the same fresh-profile evidence. Keep it
  separate so the `compute_pinned` share (~1 %) can be measured alone.
- **§5.8 — incrementally maintained `occupied` field on `Board`.** Needs writes in
  place/remove/move and FEN setup; only with a measurement against the post-Plan-6 tree.
- **Castling fast path.** `report5.md` §4 noted `generate_castling` owning the top remaining
  `movegen.rs` source lines (~0.4 % self), probably attribution noise. Step 1's profile should
  settle it; if it is real, it is a better next target than §5.8.
- **`MoveList` push cost** (~2.9–4.2 % of source-line samples, all attempts so far failed:
  `MaybeUninit`, local-count batching). Do not retry without a new design (analysis §6).
- **§5.1 / §5.2 / §5.3 / §5.5 options 1–2:** do not retry without new profiling evidence, per
  `report1.md`, `report2.md`, `report4.md`, `report5.md` §9.
- **Public API cleanup:** deprecating or removing `commoners_count`/`them_commoners_count` in favour
  of the bitboards would be a breaking change for a later `3.0`-style plan; this plan keeps them.
