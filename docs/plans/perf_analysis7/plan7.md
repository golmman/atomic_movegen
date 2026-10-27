# Plan 7 — Node-Hoisted Trivial-Legality Fast Path

**Corresponds to:** `report6.md` §6 item 4 and §9 item 2 — *attack `is_move_trivially_legal`'s
per-candidate branch ladder*. This supersedes the remaining (sub-1 %) polish items of
`analysis.md` §8 item 7 and the plan queue in `report6.md` §9.
**Estimated speedup:** **~1–3 %** on both profiled positions; the single hottest line in the
function (`if board.piece_on(from) == make_piece(us, PieceType::Commoner)`) is 5.6 % of #13's
runtime on its own (`report6.md` §6 item 4)
**Risk:** Low–medium — a crate-private predicate is rewritten and two call sites are switched, but
both predicates must return the *same boolean for every generated candidate*, which is pinned by
perft and by a new equivalence unit test
**Effort:** Small–medium — roughly 60–100 changed lines in `src/board.rs`, ~15 in `src/movegen.rs`,
~10 in `src/types.rs`, plus tests

---

## 1. Context

Plans 1–6 covered `analysis.md` §8 items 1–7:

| Plan | Item | Result |
|------|------|--------|
| 1 (`report1.md`) | §5.1 perft fast path without compaction | Reverted — regressed ~3 % on #14 |
| 2 (`report2.md`) | §5.2 local bitboard cache in `Board::legal` | Reverted — stable ~+0.2–0.9 % on #13 |
| 3 (`report3.md`) | §5.4 boolean `square_attacked_by` early exit | **Kept** |
| 4 (`report4.md`) | §5.3 category-specialized legality | Reverted — ≤0.6 % on #13, tree restored |
| 5 (`report5.md`) | §5.5 order-preserving table-driven pawn generation | **Kept** — −6.9 % on both #13 and #14 |
| 6 (`report6.md`) | §5.6 `StateInfo` commoner-bitboard cache | Reverted — #13 +4.12 %, #14 +1.89 % |

The tree therefore contains Plan 3 and Plan 5 only; `git status` shows only untracked and changed
documentation files.

**Analysis §8 is now exhausted.** Its final item ("polish with `compute_pinned`/empty-board
attacks/`occupied` cache") was measured by `report6.md` §6 and is an order of magnitude too small to
pay for itself:

| §8 remainder | Post-Plan-5 share (#13 / #14) | Verdict |
|--------------|-------------------------------|---------|
| §5.7 / §5.9 `compute_pinned`, empty-board sliders | 0.26 % / 0.14 % | drop |
| §5.8 `occupied` field | 0.08 % / 0.00 % | drop |
| castling fast path | 0.25 % / 0.07 % | drop |

`report6.md` §6 instead redirects to the two blocks that still dominate:

1. `MoveList::push` + `MoveList::new` (≈13–16 % combined) — **no plan yet**, because two designs
   already failed (`analysis.md` §6) and a new design, not a retry, is required.
2. **`is_move_trivially_legal` (7.84 % / 7.23 %)**, whose single commoner test line costs 5.6 % of
   total runtime on #13 and whose surrounding capture/en-passant/castling tests add another ~2 pp.

This plan takes item 2. It is the cheapest remaining block that can be attacked **without touching
the move-order contract** and **without repeating any failed design**:

- It is not §5.1/§5.2/§5.3/§5.5 (all reverted) — it changes neither `perft`, nor `Board::legal`, nor
  the generator, nor the move list.
- It is not §5.6 — it adds no `StateInfo` field and does not grow any per-node struct. The two
  hoisted bitboards live in registers across the filter loop.
- It is not the failed `EVASIONS`/`NON_EVASIONS` split (`analysis.md` §6) — there is no new
  generator and no duplicated loop; the *predicate body* is made cheaper.

The post-Plan-5 profile also explains why the predicate is hot: after bulk pawn generation, the
filter loop runs `is_move_trivially_legal(board, m, state)` once per pseudo-legal candidate, and
`generate_legal` calls `Board::legal` only for the candidates the predicate rejects. Making the
predicate reject/accept cheaper therefore affects **every** candidate in every node.

---

## 2. Problem

`is_move_trivially_legal` (`src/board.rs:1579–1616`) is `#[inline(always)]` and is called from the
two hot filter sites:

- `generate_legal_with_state` (`src/movegen.rs:293`), once per pseudo-legal candidate;
- `has_legal_move_with_state` (`src/movegen.rs:332`), once per candidate until the first legal move.

The current body:

```rust
if !state.checkers.is_empty() { return false; }
if state.commoners_count == 0 { return false; }

let from = m.from_sq();
let us = board.side_to_move();
if board.piece_on(from) == make_piece(us, PieceType::Commoner) { return false; }

let mt = m.move_type();
if mt == MoveType::EnPassant { return false; }

let is_capture = mt != MoveType::Castling && board.piece_on(m.to_sq()) != NO_PIECE;
if is_capture { return false; }

if mt == MoveType::Castling { return false; }

if (state.pinned & Bitboard::square_bb(from)) != Bitboard::EMPTY { return false; }

true
```

Per candidate this re-derives work that is **constant for the node**:

| Per-candidate work | Kind | Constant per node? |
|--------------------|------|--------------------|
| `state.checkers.is_empty()` | register test | **yes** |
| `state.commoners_count == 0` | field load + compare | **yes** |
| `m.from_sq()` → `SQUARES[from]` table load | load | no (move-dependent) |
| `board.side_to_move()` | field load | **yes** |
| `make_piece(us, Commoner)` | arithmetic | **yes** |
| `board.piece_on(from)` → `squares[from]` load, compare against commoner piece | **dependent load** | no |
| `m.move_type()` | shift + match (twice, via `mt`) | no |
| two `MoveType` comparisons that both reject (`EnPassant`, `Castling`) | 2 branches | no |
| `m.to_sq()` + `board.piece_on(to)` load | load | no |
| `mt != MoveType::Castling` conjunct in the capture test | compare | no |
| `Bitboard::square_bb(from)` (table load + shift) + `state.pinned` load + `and` | load + ops | partly |

`report6.md` §6 attributes the entrance `perf` profile as follows (self time, #13 run 2 / #14):

| Symbol | #13 | #14 |
|--------|-----|-----|
| `is_move_trivially_legal` | 7.84 % | 7.23 % |
| single line `board.rs:1589` (`piece_on(from) == make_piece(us, Commoner)`) | **5.6 %** | — |
| capture/en-passant/castling tests around it | ≈2 pp | — |

The commoner line is the most expensive *dependent load* in the filter loop: `from` comes out of the
move, the `squares[]` load needs that index, and the compare can only start afterwards. Nothing
about the comparison changes within a node — the set of commoner squares is fixed — so the load is
pure per-candidate overhead.

Two further observations make the body reducible without any new data structure:

1. **En-passant and castling are rejected unconditionally** (the latter after the capture test, but
   the capture test already excludes castling with `mt != MoveType::Castling`). Both can be rejected
   with a single 2-bit test, eliminating one branch and one `mt` comparison.
2. **"origin is not a commoner" and "origin is not pinned" are both bitboard membership tests against
   node-constant sets.** They can be merged once per node into
   `safe_origins = !(our_commoners | pinned)`, leaving a single register shift+test per candidate
   and no `squares[from]` load at all.

This is not a semantics change and not a generator change; it is hoisting node-constant work out of
the candidate loop, the same idea as §5.2/§5.6 but **without growing any struct**, which was the
measured reason §5.6 regressed (`report6.md` §3.4: +16 bytes of `StateInfo` → doubled
`populate_state` and ~1.7 pp of spill code).

---

## 3. Goal

1. Introduce a small crate-private, `Copy` helper that captures the node-constant part of the
   predicate exactly once per node:

   ```rust
   pub(crate) struct TrivialLegality {
       checkers: Bitboard,      // state.checkers
       safe_origins: Bitboard,  // !(our_commoners | state.pinned), or EMPTY if no own commoners
   }
   ```

2. Rewrite the per-candidate predicate as a method on that helper so that a candidate costs at most
   **two register tests, one shift+test, one `squares[to]` load, and one 2-bit move-type test** — no
   `side_to_move()` load, no `make_piece`, no `squares[from]` load, no `square_bb(from)` table load.

3. Hoist the helper construction out of the two filter loops in `src/movegen.rs`; keep both loops
   single (no per-category or per-check-state duplication — that is what sank Plan 4).

4. Preserve **exact** semantics: the predicate must return the same boolean for every generated
   candidate (proved in §7 and pinned by a new unit test comparing against a verbatim copy of the
   current ladder), identical legal-move sets, identical move order, identical perft counts, no
   public API change.

5. Add no `StateInfo` field, no `Board` field, no new `unsafe`, no new dependency, and no change to
   generation, `perft`, `Board::legal`, `MoveList`, `do_move`, or `undo_move`.

---

## 4. Design

### 4.1 Extra crate-private `Move` accessors (`src/types.rs`)

Add three `#[inline(always)]` crate-private helpers next to `from_sq`/`to_sq`/`move_type`:

```rust
impl Move {
    /// Raw origin-square index (0..64) without the `Square`/`SQUARES` conversion.
    #[inline(always)]
    pub(crate) fn from_index(self) -> usize {
        ((self.0 >> 6) & 0x3f) as usize
    }

    /// Raw destination-square index (0..64) without the `SQUARES` conversion.
    #[inline(always)]
    pub(crate) fn to_index(self) -> usize {
        (self.0 & 0x3f) as usize
    }

    /// `true` for en-passant and castling moves, which always need `Board::legal`.
    #[inline(always)]
    pub(crate) fn needs_full_legal_check(self) -> bool {
        ((self.0 >> 12) & 3) >= 2
    }
}
```

Rationale: the predicate no longer needs `Square` values at all — the origin test is a bit test and
the destination test is an array index — so two 64-byte `SQUARES` table loads per candidate disappear.

**Precondition:** `from_index`/`to_index` are only valid for moves produced by the generator. For
`Move::NONE` the raw index is `0` (a1), whereas `Square::NONE` maps to 64. The rewritten predicate is
therefore crate-private and only called on candidates from the pseudo-legal stream; it must
`debug_assert!(m != Move::NONE)` so misuse fails loudly in tests. Do not expose these helpers
publicly and do not change `from_sq`/`to_sq` semantics.

### 4.2 `TrivialLegality` in `src/board.rs`

Replace the free function with a `Copy` struct and a method. Move the existing doc comment to the
method and state the generator precondition. Suggested shape:

```rust
/// Node-constant part of the trivial-legality fast path.
///
/// Build once per node from a populated [`StateInfo`], then call
/// [`is_trivially_legal`](Self::is_trivially_legal) for every pseudo-legal
/// candidate. The fast path returns `true` only for moves that are legal
/// without the full [`Board::legal`] check (no capture, not a commoner move,
/// not en passant, not castling, no checkers, moving piece unpinned, at least
/// one own commoner).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrivialLegality {
    checkers: Bitboard,
    safe_origins: Bitboard,
}

impl TrivialLegality {
    /// Derive the node-constant fast-path data.
    #[inline(always)]
    pub(crate) fn new(board: &Board, state: &StateInfo) -> Self {
        let safe_origins = if state.commoners_count == 0 {
            // No own commoner: every candidate must go through `legal`, which
            // rejects them (matching the old `commoners_count == 0` guard).
            Bitboard::EMPTY
        } else {
            !(board.commoners(board.side_to_move()) | state.pinned)
        };
        Self { checkers: state.checkers, safe_origins }
    }

    /// Returns `true` when `m` is trivially legal.
    ///
    /// `m` must come from the pseudo-legal generator (valid from/to squares);
    /// this is a crate-private precondition, not an input validation point.
    #[inline(always)]
    pub(crate) fn is_trivially_legal(&self, board: &Board, m: Move) -> bool {
        debug_assert!(m != Move::NONE);
        if !self.checkers.is_empty() {
            return false; // in check: everything needs the full check
        }
        if m.needs_full_legal_check() {
            return false; // en passant and castling
        }
        // Quiet promotion and quiet normal moves are the only survivors so far;
        // both need the destination to be empty.
        if board.squares[m.to_index()] != NO_PIECE {
            return false; // capture
        }
        // Origin must be neither a commoner nor pinned.
        ((self.safe_origins.0 >> m.from_index()) & 1) != 0
    }
}
```

Notes:

- `board.squares[...]` is directly accessible inside `src/board.rs`; using it (rather than
  `piece_on`) avoids the `Square::NONE` branch, which is dead under the precondition. If maintainers
  prefer keeping the `piece_on` abstraction, keep `piece_on(m.to_sq())`; that retains one `SQUARES`
  load but is still a large net win. Choose one and document the choice in `report7.md`.
- Bitboard internals: `Bitboard` is `pub struct Bitboard(pub u64)` in `src/types.rs:283`, so
  `(self.safe_origins.0 >> m.from_index()) & 1` compiles from `src/board.rs`. For readability, prefer
  adding a `#[inline(always)] pub(crate) fn contains(self, index: usize) -> bool` helper to
  `Bitboard` (in `src/types.rs`) and writing `self.safe_origins.contains(m.from_index())`; do not add
  it to the public API.
- `safe_origins` is `EMPTY` exactly when `commoners_count == 0`, so the old second guard is folded
  into the origin test. Add a debug assertion in `new` documenting the count/board invariant
  instead:

  ```rust
  debug_assert!(
      state.commoners_count == 0 || !board.commoners(board.side_to_move()).is_empty(),
      "StateInfo.commoners_count disagrees with the board"
  );
  ```

  (This is cheap, debug-only, and catches a stale `StateInfo`.)
- The method must stay `#[inline(always)]` and the struct `Copy` so the loop body keeps the two
  bitboards in registers.

### 4.3 Call sites: hoist once per node (`src/movegen.rs`)

**`generate_legal_with_state` (`src/movegen.rs:279`)**: 

```rust
pub fn generate_legal_with_state(board: &Board, state: &StateInfo, moves: &mut MoveList) {
    generate_pseudo_legal(board, moves);

    let orig_len = moves.len();
    if orig_len == 0 {
        return;
    }

    let trivial = TrivialLegality::new(board, state);
    let new_len = {
        let ms = moves.as_mut_slice();
        let mut write_idx = 0;
        for read_idx in 0..orig_len {
            let m = ms[read_idx];
            if trivial.is_trivially_legal(board, m) || board.legal(m, state) {
                ms[write_idx] = m;
                write_idx += 1;
            }
        }
        write_idx
    };
    moves.set_len(new_len);
}
```

**`has_legal_move_with_state` (`src/movegen.rs:324`)**: build `trivial` after the existing
empty-commoner early return and capture it in the closure.

Both loops remain single loops. Do **not** split "in check" from "not in check", and do not split by
move category — that is the Plan 4 failure mode.

### 4.4 Remove the old free function; keep one canonical predicate

Delete `is_move_trivially_legal` and update the import in `src/movegen.rs` to `TrivialLegality`. There
must be exactly **one** implementation of the predicate in the crate, otherwise the equivalence test
in §5 Step 3 would only compare the new code against itself.

### 4.5 Fallback if the gain does not materialise

If Step 5 shows a regression or a sub-threshold gain, **revert** — do not start layering:

- Do not add the `occupied` hoist (§5.8), the `square_attacked_by` rewrite, or the `MoveList`
  changes on top to rescue the number (analysis §6 discipline, `report6.md` §5).
- The one documented alternative shape, if the profile shows the origin test was *not* the cost, is
  to pass `checkers`/`safe_origins` as two explicit `Bitboard` parameters instead of a struct. That is
  a one-line signature difference and is **not** part of this plan; it should only be attempted if a
  report documents that the struct itself (not the predicate) is the problem.
- If the gain is real but smaller than expected, keep the measured change (it removes work), but
  record that `is_move_trivially_legal` was already partly optimised and re-rank the follow-ups in
  `report7.md` §9.

### 4.6 What not to change

- No change to `for_each_pseudo_legal`, `generate_pseudo_legal`, `generate_pawn_moves_for`,
  `generate_piece_moves`, or `generate_castling`; move order stays byte-identical.
- No change to `Board::legal`, `Board::populate_state`, `StateInfo` (no fields, same size),
  `perft`, `do_move`, `undo_move`, or `MoveList`.
- No change to `Move::from_sq`/`to_sq`/`move_type` behaviour; only additive crate-private helpers.
- No `unsafe`, no new dependency, no public API change.
- No retry of `MaybeUninit` move lists or batched `MoveList` counters (`analysis.md` §6).

---

## 5. Implementation Steps

### Step 0: Record the §8 status (documentation only)

`analysis.md` §8's implementation order is fully consumed and partially invalidated by measurement.
In the new report (`report7.md`), state explicitly that §5.7/§5.9/§5.8 and the castling fast path are
**closed** (targets < 0.3 %, `report6.md` §6) and that this plan is a *profile-derived* item, not an
`analysis.md` §5 item. Do not rewrite `analysis.md` itself in this plan; the report is the record.

### Step 1: Re-baseline and re-profile (decision gate)

Absolute timings drift between sessions (`analysis.md` 8.46 s for #13, `report4.md` 11.3 s,
`report5.md` 11.77 s, `report6.md` 10.6–11.1 s). Everything below must be same-session.

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

Expected node counts: #13 `2 160 817 389`, #14 `245 931 633`. Record every run, not only the median.

Then confirm the target is still hot in a fresh profile of the **current** tree:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
perf record -F 997 -g --call-graph dwarf -o /tmp/perf7-plan7-baseline.data \
  target/profiling/examples/perft "$F13" 6
perf report -i /tmp/perf7-plan7-baseline.data --stdio --no-children --sort=symbol
perf report -i /tmp/perf7-plan7-baseline.data --stdio --no-children --sort=srcline | head -40
```

**Decision gate.** Proceed only if `is_move_trivially_legal` is still ≥ ~5 % self time on #13 and its
commoner/`piece_on(from)` line is ≥ ~3 %. If `MoveList::push` has grown and the predicate has fallen
below ~4 %, **stop** and write `report7.md` with the redirected result, recommending the
`MoveList`/generation-time-filter design instead (`report6.md` §6 item 3).

Before timing, also resolve the line-less attribution gap that `report6.md` §8 flags: run
`perf annotate` on the inlined `perft`/`generate_legal_with_state` body at least once, so that the
`board.rs:0` / `movegen.rs:0` bins (12–14 %) can be checked to still belong to move construction and
the filter loop rather than to the predicate.

### Step 2: Add `Move`/`Bitboard` helpers and `TrivialLegality` (behaviour no-op)

1. Add `from_index`, `to_index`, `needs_full_legal_check` to `src/types.rs` (§4.1) with doc comments.
2. Add `Bitboard::contains(index)` if the tuple field is not accessible from `src/board.rs`.
3. Add `TrivialLegality` with `new` and `is_trivially_legal` (§4.2).
4. Keep `is_move_trivially_legal` temporarily as a thin wrapper
   (`TrivialLegality::new(board, state).is_trivially_legal(board, m)`) so the tree still compiles and
   the tests can compare the two implementations.

Verify the no-op state:

```sh
cargo build
cargo test
```

### Step 3: Switch the call sites, add the equivalence test, remove the wrapper

1. Add the `trivial` hoist in both `src/movegen.rs` functions (§4.3).
2. In `src/board.rs`'s `#[cfg(test)] mod tests`, add a **reference implementation** that is a verbatim
   copy of the current ladder (pre-Step 2), and a test that, for every FEN in a fixed corpus, calls
   `populate_state` and iterates `generate_pseudo_legal`, asserting for every candidate:

   ```rust
   assert_eq!(
       TrivialLegality::new(&board, &state).is_trivially_legal(&board, m),
       is_move_trivially_legal_reference(&board, m, &state),
       "fast path diverged on FEN {fen}, move {m:?}"
   );
   ```

   Corpus: startpos, the four `tests/move_order_golden.rs` positions (startpos, middlegame, endgame,
   promotion), #13 and #14, plus a hand-built position with an en-passant move available, a castling
   move available, a pinned piece, a side with no commoner, and a side in check. The golden and perft
   FENs are copied as literals because the test module cannot read files.
3. Also assert soundness against the full checker for every candidate: if the fast path returns
   `true`, then `board.legal(m, &state)` must be `true`. (Completeness is already pinned by perft;
   this catches over-eager fast paths at the unit level.)
4. Delete the wrapper free function and update the `src/movegen.rs` import (§4.4).

```sh
grep -n "is_move_trivially_legal" src/
```

Expected: only the reference implementation and test code (or nothing if the reference is named
differently).

### Step 4: Correctness checks (before any timing)

`report6.md` §4 records the process mistake of timing before `verify_perft`; this plan fixes the
order:

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

- `clippy --all-targets` shows no new warnings (the four pre-existing warnings in
  `examples/test_d5g8.rs` are unrelated).
- All tests pass, including the new equivalence test.
- `verify_perft` **41/41** at depths 1–6.
- `move_order_golden` passes with **unchanged goldens** (the predicate cannot reorder the list; this
  proves it).
- `cargo doc` warning-free (no public items changed).

**Only after all of the above pass** continue to Step 5.

### Step 5: Performance verification

Rebuild and repeat the Step 1 fixed-FEN loops in the same session, then run the A/B round
(`git stash` the patch → baseline → `git stash pop` → candidate) as in `report5.md` §3.2.

Acceptance criteria:

- **#13 median improves by ≥ 1.0 %** versus the same-session baseline, **and** the pairwise A/B runs
  are faster in at least 6 of 7 pairs.
- **#14 does not regress by more than 0.3 %** (an improvement is a bonus).
- Full suite stays 41/41; the suite total is supporting evidence only.
- **Marginal band (0.5–1.0 % on #13):** keep only if both (a) the post-change profile shows the
  `is_move_trivially_legal` block and its commoner line roughly halved, and (b) the same-session
  full-suite total did not regress. Record the decision rationale in `report7.md`.
- **Below 0.5 % or any #14 regression beyond 0.3 %:** revert `src/board.rs`, `src/movegen.rs`,
  `src/types.rs` to the Plan 5 tree, rebuild, confirm baseline restoration, and record the negative
  result. Do not stack other optimizations (§4.5).

If kept, re-profile and check that:

- `is_move_trivially_legal`'s share (or its inlined attribution inside the filter loop) dropped;
- `Board::legal` / `square_attacked_by` shares did not grow (the fast path must still accept the same
  candidates — if `Board::legal` grew, the predicate became over-eager and is wrong, not slow);
- `MoveList::push` did not grow (no extra stores introduced).

### Step 6: Write the implementation report

Create `docs/plans/perf_analysis7/report7.md` per `AGENTS.md`. It must contain:

- The Step 0 statement closing §5.7/§5.9/§5.8/castling and explaining why this plan exists.
- The Step 1 gate profile (symbol and source-line shares) and the `perf annotate` result for the
  line-less bins.
- `git diff --stat` proving only `src/board.rs`, `src/movegen.rs`, `src/types.rs` changed (plus
  `docs/`).
- All baseline, candidate and A/B fixed-FEN runs with medians and pairwise deltas.
- Full-suite result and perft node counts.
- Correctness table: `fmt`, `clippy --all-targets`, `test`, `doc`, `verify_perft` 41/41,
  `verify_moves`, `move_order_golden` (unchanged goldens), the equivalence test, and the
  fast-path ⇒ `legal` soundness test.
- Whether `board.squares[...]` or `piece_on(...)` was used for the capture test, and the measured
  difference if both were tried.
- Explicit keep/revert decision with the acceptance numbers it was judged against.
- If kept: profile evidence that the predicate's share dropped.
- If reverted: what the profile says should be done instead — the expected redirect is the
  `MoveList`/generation-time-filter design (`report6.md` §6 item 3), which needs a *new* design, not
  a retry of `MaybeUninit`/batching.
- Next steps for the plan queue (see §9).

---

## 6. Files Changed

| File | Change | Approx. Lines |
|------|--------|---------------|
| `src/board.rs` | `TrivialLegality` struct + `new` + `is_trivially_legal`; delete `is_move_trivially_legal`; doc updates; equivalence/soundness unit tests. | ~60–100 |
| `src/movegen.rs` | Hoist `TrivialLegality::new` in `generate_legal_with_state` and `has_legal_move_with_state`; import update. | ~10–20 |
| `src/types.rs` | Three crate-private `Move` accessors; optional crate-private `Bitboard::contains(index)`. | ~10–20 |
| `docs/plans/perf_analysis7/report7.md` | New implementation report. | New file |

No changes expected in `src/lib.rs`, `src/attacks.rs`, `src/magic.rs`, `src/zobrist.rs`, `src/util.rs`,
`examples/`, or `tests/`.

---

## 7. Correctness Contract

For every generated pseudo-legal candidate `m` in a node with populated `StateInfo`, the new
predicate must equal the old one. Case analysis (old order → new order):

| # | Case | Old result | New result | Why equal |
|---|------|-----------|-----------|-----------|
| 1 | `checkers` non-empty | `false` (first guard) | `false` (first guard) | identical |
| 2 | no own commoners (`commoners_count == 0`) | `false` (second guard) | `false` (`safe_origins == EMPTY`) | `new` sets `EMPTY` iff count is 0 |
| 3 | origin is a commoner | `false` | `false` | commoner squares cleared from `safe_origins` |
| 4 | en passant | `false` | `false` | type bits 2 |
| 5 | castling | `false` | `false` | type bits 3 |
| 6 | capture (normal) | `false` | `false` | `squares[to] != NO_PIECE` |
| 7 | capture (promotion) | `false` | `false` | same destination test; promotions are type 1 |
| 8 | quiet promotion | `true` iff unpinned | `true` iff unpinned | origin is a pawn: not commoner, not pinned ⇔ safe |
| 9 | quiet normal, unpinned, non-commoner origin | `true` | `true` | origin test passes |
| 10 | quiet normal, pinned origin | `false` | `false` | pinned squares cleared from `safe_origins` |
| 11 | quiet normal, commoner origin | `false` | `false` | case 3 |

Ordering differences (case 4/5 before 6, case 3 after 6) only reorder **rejections**; the boolean is
the same. The old capture test's `mt != MoveType::Castling` conjunct is implied by case 5 rejecting
castling first.

Additional invariants:

1. **Node-constant values.** `checkers`, `our_commoners` and `pinned` are written by
   `populate_state` and `Board::legal`/the generator only read the board (`&Board`), so they cannot
   change between `TrivialLegality::new` and the last candidate of the node.
2. **Unpopulated `StateInfo`.** `StateInfo::new()` has empty `checkers`, `pinned` and
   `commoners_count == 0`, so `safe_origins == EMPTY` and the predicate declines every candidate —
   the same outcome as the old `commoners_count == 0` guard. No behaviour change for that misuse
   case; the debug assertion in `new` documents the count/board relation.
3. **Precondition.** Only generator-produced moves are passed; `from_index`/`to_index` are not valid
   for `Move::NONE` and the `debug_assert!(m != Move::NONE)` enforces that in tests.
4. **No move-order effect.** The predicate only decides whether `board.legal` is called; accepted
   candidates are written in the same read order as before (§4.3 leaves the compaction loop's shape
   unchanged). `tests/move_order_golden.rs` must pass unchanged.
5. **No public API change.** `TrivialLegality` and the `Move` accessors are `pub(crate)`; `Move`,
   `Bitboard` and `StateInfo` keep their layouts and documented semantics.

Verification set: `verify_perft` (41 positions, depths 1–6), `verify_moves`,
`tests/move_order_golden.rs` (unchanged goldens), the new equivalence test over a fixed corpus
(including en passant, castling, pins, check, no-commoner positions), the fast-path ⇒ `legal`
soundness test, and `cargo test` in debug (which enables the `debug_assert!`s).

---

## 8. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Layout/register-pressure effect cancels the gain (precedent: Plans 1, 2, 4, 6) | Medium–high | Step 1 gate, fixed-FEN medians, pairwise A/B round, explicit revert threshold; the change **adds no struct field and no node store**, which was the measured cost mechanism in Plan 6. |
| `MoveList::push` grows / the filter loop becomes spill-bound | Low–medium | The predicate uses only two extra registers held across the loop; measure via profile in Step 5 and revert if the block does not shrink. |
| Semantics drift in the reordered ladder | Low | §7 case table, verbatim-reference equivalence test over a corpus covering every case, perft 41/41, move-order goldens. |
| Raw `Move` indices misused on `Move::NONE` | Low | Crate-private only; `debug_assert!(m != Move::NONE)`; no public API change; generator precondition documented on the method. |
| `Bitboard` internals inaccessible from `board.rs` | Low | `contains(index)` helper in `src/bitboard.rs` (§4.2); if neither is clean, keep one `piece_on(from)` load and accept a smaller gain. |
| Temptation to split the filter loop by check state or move category | Medium | §4.3/§4.6 explicitly forbid it (Plan 4 precedent). |
| Full-suite noise hides a small regression | High | Decide on fixed-FEN medians and pairwise runs; suite total is context only. |
| Expected gain overestimated (the 5.6 % line may be mostly the branch, not the load) | Medium | A measured sub-threshold result is an accepted outcome with a documented revert, not a failure; `report7.md` must say which part of the line dominated. |
| Re-profiling is ambiguous because of line-less bins | Medium | Step 1 requires one `perf annotate` pass and records the result before editing. |

---

## 9. Out of Scope / Follow-ups

- **`MoveList` push/zero-init (13–16 % combined)** — the largest remaining block. A plan requires a
  genuinely new design, e.g. filtering the pseudo-legal stream **during generation** instead of
  writing candidates to a list and compacting afterwards (`report6.md` §6 item 3), or a
  proven-monotonic length without a per-push bounds branch. `analysis.md` §6 forbids retrying
  `MaybeUninit` and local-count batching. `TrivialLegality` is deliberately a standalone `Copy`
  predicate so it can be reused by a future generation-time filter without rewriting it.
- **§5.7 / §5.9 (`slider_blockers`, precomputed empty-board sliders), §5.8 (`occupied` field),
  castling fast path** — closed; targets are 0.07–0.26 % (`report6.md` §6). Do not schedule.
- **§5.1 / §5.2 / §5.3 / §5.5 options 1–2 / §5.6** — closed by `report1.md`–`report6.md`; do not
  retry without new profiling evidence.
- **`EVASIONS`/`NON_EVASIONS` split and category-specialized generation/legality** — two reverts
  (`analysis.md` §6, `report4.md`); not in scope.
- **Wall-clock/build-level items (`analysis.md` §5.10)** — parallel root perft for `verify_perft`,
  `target-cpu=native`, PGO — unchanged and unrelated to per-node cost.
