# Plan 2 — the specialised existential test (MG20)

## Scope

One crate-private rule query and its wiring:

- **MG20:** a child-side existential-legality test that is *early-exit by
  construction*: "does this side, with these pieces, have any legal move?"
  answered by a positive, cheaply-decidable claim ("here is one demonstrably
  legal quiet move") first, and by the existing exact ladder only when the
  claim cannot be made.
- **Wiring:** `terminal_in_view_light` (plan 1), `EffectContext::has_legal_move_after`
  and `EffectContext::is_terminal_after` use the new entry point instead of
  populating a full `StateInfo` and walking the pseudo-legal stream; the
  board-side `has_legal_move_in_view` gets the same positive screen as a first
  step (it has `checkers` already and loses nothing).

Requirements served: handoff 3 MG20 (P2). **Depends on plan 1** (the light
terminal path and the fact split that let one `checkers` value serve both the
terminal precedence and the existence question).

## Current-tree assessment

- `src/movegen.rs::has_legal_move_in_view(view, state)` = extinction guard, then
  `FastLegality::new` + a `for_each_pseudo_legal_in_view` walk that breaks at
  the first accepted candidate. It is an *existential* in name only: it needs a
  populated `StateInfo` (`checkers`, `pinned`, counts) up front, it builds the
  generator's per-node setup (piece-type sets, target masks, ep, castling
  check) before the first candidate, and it asks the per-candidate filter
  rather than "does any move of this kind exist".
- The child path always pays for that population: `terminal_from` (2.7.0),
  `is_terminal_after` and `has_legal_move_after` each do
  `let mut state = StateInfo::new(); view.populate_state(&mut state);` before
  the query, although terminality and existence need neither `pinned` nor a
  `StateInfo`.
- Indicative split (index): `populate_state` 7.4 ns of which `checkers` 6.0 ns,
  `has_legal_move_with_state` 10.4 ns, `has_legal_move` 21.9 ns,
  `ctx.has_legal_move_after` 33.8 ns, `ctx.terminal_after` 36.2 ns on this
  machine. Handoff 3 measures 61.8 ns for `has_legal_move_after` on theirs and
  asks for **≤ 12 ns** for the existential alone (i.e. ≈ 0.375× of the 32 ns
  build).
- The rules reasoning the screen needs already exists in one place:
  `WidenedLegality`'s documentation in `src/board.rs` states the two facts this
  plan reuses (see below). Nothing new is derived.

## Decisions and pushback

1. **A positive witness query, not a faster filter.**
   `fn quiet_legal_witness(view: &PositionView, checkers: Bitboard) -> Option<Move>`
   returns *one move it can prove legal*, or `None` (meaning "not proven", never
   "no legal move"). Existence becomes `is_some()` and the "demonstrably legal"
   half is directly testable: a test can assert the witness is in the legal
   list. A per-candidate `Fn(Move) -> bool` would be the existing design under a
   new name.
2. **The screen is a sufficient condition, the fallback is exact.** Returning
   `false`-as-in-"no legal move" from the screen would make the cost of the
   *decided* answers (mate, stalemate) the cost of a full walk — which is
   exactly the shape handoff 3 rejects. So the contract is:

   ```text
   witness = Some(m)  =>  m is legal and `has_legal_move` is true
   witness = None     =>  nothing is concluded; the exact ladder decides
   ```

   Exactness of the whole query is therefore preserved by construction: an
   accepted witness is a *proof*, and every other case falls back to the code
   that runs today.
3. **Where the proof comes from (the only rule argument this plan adds).**
   Reuse the two facts already documented on `WidenedLegality`, quoted rather
   than re-derived:
   - *Only removals matter for a commoner that stays put.* Enemy pieces never
     move inside one move, so the enemy's attacks on our commoner can only grow
     when the move removes a piece blocking a slider's ray. A **quiet,
     non-commoner** move empties exactly `{from}`; if `from` is not a sole
     blocker between our commoner and an aligned enemy slider, our commoner is
     as safe after the move as before it.
   - *A quiet non-commoner move triggers no blast*, so it can neither explode
     the mover's own commoner nor the opponent's, and it cannot be a
     self-explosion.
   Together with "our commoner is **not** attacked now" (`checkers` empty), a
   quiet non-commoner move from an origin that is not a sole blocker **is**
   legal. That is the whole proof; the screen searches for one such move.
   The "sole blocker" set is `PositionView::pinned(us)` — the same set
   `TrivialLegality::safe_origins` uses, with the same definition. When the two
   commoners touch, the commoner is immune to every attack
   (`PositionView::checkers` documents this), so the blocker exclusion is not
   needed and the engine reads `checkers.is_empty()` anyway (touching implies no
   check).
4. **No commoner origins in the screen.** A commoner move is legal only if its
   destination is not attacked after the move (or touches the enemy commoner) —
   a different, more expensive question. Those positions fall back. Handoff 3
   names both rules ("a move that explodes the mover's own commoner is not a
   legal move; a commoner move that leaves it attacked is not legal"), and both
   are handled by never claiming a commoner move and by the exact fallback.
5. **No castling in the screen.** Castling has its own pass-through rule
   surface; the fallback decides it. (The crate already notes that castling can
   never be the only legal move in this ruleset, which is why this costs
   nothing in practice — but the plan does not rely on that note.)
6. **One `checkers` value, computed once.** The terminal path (plan 1) needs
   `checkers` for the mate/stalemate reason and the clock branch; the screen
   needs it as its precondition. The light entry point therefore *takes*
   `checkers` (`has_legal_move_in_view_light(view, checkers)`) and a
   convenience wrapper computes it. Computing it twice would eat the budget on
   its own (6.0 ns of the ~12 ns target on this machine, before rescaling).
7. **`TrivialLegality`/`WidenedLegality`/`FastLegality` are not touched.** The
   screen is a separate, positive query with its own early exits; the ladder
   stays the oracle path for the generator. Duplicated *reasoning* is avoided
   by documenting both against the same two facts (decision 3) and by the
   witness-soundness test; duplicated *code* is not created.

## Proposed API (crate-private)

```rust
// src/movegen.rs

/// One move the screen can prove legal, or `None` when it cannot prove any.
///
/// `checkers` is `view.checkers(view.side_to_move())`, supplied by the caller
/// so one attacker query serves the terminal precedence and this test.
/// `Some(m)` is a *proof*: `m` is legal in `view`. `None` means "not proven".
fn quiet_legal_witness(view: &PositionView, checkers: Bitboard) -> Option<Move>;

/// The per-node fast-legality walk, without the "game is already over" guard.
fn has_legal_move_ladder(view: &PositionView, state: &StateInfo) -> bool;

/// Unchanged contract: exactly `!generate_legal_with_state(..).is_empty()`.
pub(crate) fn has_legal_move_in_view(view: &PositionView, state: &StateInfo) -> bool;

/// The child-side existential, without a `StateInfo`.
///
/// Equal to `has_legal_move_in_view` on a populated state for this view, and to
/// `!generate_legal_with_state(..).is_empty()`; populations happen only on the
/// fallback path.
pub(crate) fn has_legal_move_in_view_light(view: &PositionView, checkers: Bitboard) -> bool;

/// Convenience: `has_legal_move_in_view_light(view, view.checkers(us))`.
pub(crate) fn has_legal_move_in_view_unpopulated(view: &PositionView) -> bool;
```

No public API item is added by this plan. The public contract changes are
*measurements*, not answers:

| Public query | Answer | Change |
| --- | --- | --- |
| `EffectContext::terminal_after`, `PostMove::terminal` | unchanged | cheaper (uses `has_legal_move_in_view_light`) |
| `EffectContext::has_legal_move_after` | unchanged | cheaper, no `StateInfo` |
| `EffectContext::is_terminal_after` | unchanged (literal mirror, no material shortcut) | cheaper, no `StateInfo` |
| `EffectContext::child_outcome` | unchanged | cheaper |
| `movegen::has_legal_move*` (board side) | unchanged | first step is the screen |

## Implementation

1. **The witness** (`src/movegen.rs`):

   ```rust
   fn quiet_legal_witness(view: &PositionView, checkers: Bitboard) -> Option<Move> {
       let us = view.side_to_move();
       let them = us.flip();
       let our_commoners = view.commoners(us);
       let their_commoners = view.commoners(them);
       // Extinction: no move is legal at all. (The callers guard this too; the
       // screen stays total on its own.)
       if our_commoners.is_empty() || their_commoners.is_empty() {
           return None;
       }
       // In check: a quiet move must resolve the attack (block or capture) —
       // that is the ladder's question, not this screen's.
       if !checkers.is_empty() {
           return None;
       }
       // Touching commoners: the commoner is immune to every attack, so no
       // origin exclusion is needed.
       let immune = !(their_commoners & attacks::king_attacks(our_commoners.lsb())).is_empty();
       let unsafe_origins = if immune {
           our_commoners
       } else {
           our_commoners | view.pinned(us)   // sole blockers of a slider ray
       };
       let origins = !unsafe_origins;
       let empty = !view.occupied();

       // Pawn single pushes (and promotions — still quiet, still no blast).
       let mut pawns = view.pieces_color_pt(us, PieceType::Pawn) & origins;
       while !pawns.is_empty() {
           let from = pawns.pop_lsb();
           let push = attacks::pawn_pushes(us, from) & empty;
           if !push.is_empty() {
               return Some(Move::make_move(from, push.lsb()));
           }
       }
       // Knights.
       let mut knights = view.pieces_color_pt(us, PieceType::Knight) & origins;
       while !knights.is_empty() {
           let from = knights.pop_lsb();
           let to = attacks::knight_attacks(from) & empty;
           if !to.is_empty() {
               return Some(Move::make_move(from, to.lsb()));
           }
       }
       // Sliders, one magic lookup per piece, early exit at the first hit.
       // (Bishops, rooks, queens in that order; reuse `attacks::bishop_attacks`,
       // `rook_attacks`, `queen_attacks` with `view.occupied()`.)
       ...
       None
   }
   ```

   Notes that must survive implementation: a promotion push is encoded as
   `Move::make_promotion(from, to, pt)` for the witness (any
   `PROMOTION_PIECES` member is legal — use the first), or the plain push is
   accepted and the witness is documented as "a move that exists", not
   necessarily the generator's encoding; **prefer the generator's encoding** so
   the witness is directly comparable with the legal list in tests. Double
   pushes add nothing (a legal single push exists whenever a double push does).
   Pawn captures are not quiet and are not claimed.

2. **Restructure the existence queries** around the witness (code sketch in
   "Proposed API"). Two rules:
   - the extinction guard of `has_legal_move_in_view` stays **first**, so a
     stale `StateInfo` can still only produce `false` (the witness must never
     flip a guarded `false` into `true`);
   - `has_legal_move_in_view_light` takes its counts from the view (that is the
     documented difference from the state-based entry point, same as plan 1's
     light terminal path) and populates a `StateInfo` only on the fallback.

3. **Wire the child path** (`src/effects.rs`):
   - `position::terminal_in_view_light` (plan 1) replaces its local
     population with `movegen::has_legal_move_in_view_light(view, checkers)`,
     reusing the `checkers` value it already computed.
   - `EffectContext::has_legal_move_after` and
     `EffectContext::is_terminal_after` use
     `has_legal_move_in_view_unpopulated(&child.view)` instead of populating a
     `StateInfo` and calling the state-based form. Their answers must not
     change (`is_terminal_after` keeps excluding the material draw; it may reuse
     the extinction/clock prefix of the light terminal path but must not gain
     the material branch).
   - `PostMove::terminal` and `EffectContext::child_outcome` inherit the win
     automatically.

4. **Docs.** `src/movegen.rs` docs for `has_legal_move_in_view*` describe the
   two-step shape (positive screen → exact ladder) and the "witness is a proof,
   absence is not" contract; `src/effects.rs` module docs and
   `docs/perf/perf_guide.md` note that the child existence path no longer
   populates a `StateInfo`. `CHANGELOG.md` `[Unreleased]` gains the measured
   sentence (numbers filled in by plan 3's run).

## Tests

### Exactness (public contract)

For every position of the 41-position perft corpus, the child/effect fixtures,
and seeded play-outs (`tests/common/mod.rs`; ≥ 5 start FENs × 16 seeds × 300
plies):

- for every pseudo-legal move: `ctx.has_legal_move_after(&b, m) ==
  !generate_legal_with_state(&child, &child_state, ..).is_empty()`;
- the same for `ctx.is_terminal_after` and `ctx.terminal_after` against the
  cloned child (these diffs already exist — they must stay green, unchanged);
- `movegen::has_legal_move_in_view_light(view, view.checkers(us)) ==
  movegen::has_legal_move_in_view(view, &populated_state)` for **every child
  view** produced by a pseudo-legal move of the corpora (this is the direct
  differential of the new entry point, and it covers in-check children, only-
  commoner-and-pawns children, and touching-commoner children);
- the same differential over every corpus position itself (view-level, unit
  test in `src/movegen.rs`).

### Witness soundness and coverage (unit test in `src/movegen.rs`)

- **Soundness:** for every position of the corpus + play-outs and every child
  view of every pseudo-legal move, if `quiet_legal_witness(view, checkers)` is
  `Some(m)` then `m` is a member of that view's legal list (compare after
  encoding both through the generator, so castling/ep encodings cannot mask a
  mismatch). Report the witness count in the report.
- **Coverage:** the screen must actually fire. Assert it fires for ≥ 95 % of the
  visited positions that have a legal move and no checker, and assert it fires
  for each named fixture below. A regression that makes the screen useless
  (e.g. an over-wide `unsafe_origins`) fails this test instead of silently
  costing 20 ns.

### Handoff 3's named acceptance fixtures

Children of, and positions of, these shapes (write down expected
`has_legal_move_after`/`terminal_after` values, each verified against the cloned
child in the same test):

- **only-commoner-and-pawns:** e.g. `8/8/8/4k3/8/4P3/8/4K3 w - - 0 1` and the
  child after `e3e4`; a blocked-pawn variant
  `8/8/8/8/8/4k3/4P3/4K3 b - - 0 1` where the pawn is the sole blocker of a
  ray;
- **touching commoners:** e.g. `8/8/8/8/8/8/1k6/2K5 w - - 0 1` (b2/c1) and a
  child of a commoner move that ends `k`-adjacent;
- **in check, one escape:** `7k/5P2/8/8/8/8/8/K6R b - - 0 1` (child in check,
  witness must decline, ladder must decide);
- **captures only:** `k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1` (witness must
  decline — quiet moves are all illegal there — and the ladder must find the
  pawn captures);
- **no legal move:** checkmate and stalemate children from the existing
  `tests/terminal_after.rs` fixtures (witness declines, ladder returns `false`,
  `terminal_after` still reports `NoMovesInCheck`/`Stalemate`);
- **bare commoners (K vs K):** `k7/8/8/8/8/8/8/7K w - - 0 1` (material-draw
  branch decides before the existence question).

### No behaviour change elsewhere

- `cargo test` (all suites), `perft_tests`, `move_order_golden`,
  `verify_moves`, `conformance`: green, no expectation edited.
- `tests/terminal_after.rs`, `tests/child_state.rs`, `tests/effects.rs`: green
  **unmodified** (this plan changes cost, not answers).

## Verification

All global gates of the index, plus `cargo run --release --example
effect_bench 9 2000` (three runs, medians). Acceptance is expressed as ratios
measured **in the same run**, because handoff 3's absolute ns are
downstream-machine numbers:

| Row | Gate |
| --- | --- |
| existential share = `has_legal_move_after` − `post_move` | **≤ 0.40×** the build-only row (`clone`+`do_move`) of the same run; handoff 3's 12 ns / 32 ns = 0.375 is the reference point |
| `PostMove::terminal()` / `terminal_after` | ≤ `post_move` + 0.45× build |
| composite `post_move_and_key` + `terminal()` on the *built-ish* class | report the ratio to `clone`+`do_move`+`outcome()`+key; ≤ 0.95× is the handoff's realisable target, and plan 3 owns the verdict |
| `has_legal_move` (board path) | no regression > 2 % vs the pre-change three-run median |
| `generate_legal`, `for_each_legal`, `classification`, `effect` | no regression > 2 % (unchanged code paths) |

If the ≤ 0.40× share is **not** reached, do not paper over it: report the
measured ratio, the class it fails on (in-check children, only-commoner
positions, …) and hand the verdict to plan 3. The handoff's rule is that only
measured numbers may re-open the question.

Also run `cargo run --release --example verify_perft 5` (41/41) and perft
timing (three runs, median): this plan touches `has_legal_move_in_view`, which
is on the `terminal`/`outcome` path, not on the generator — a perft change is a
blocker.

## Files to touch

- `src/movegen.rs` — `quiet_legal_witness`, `has_legal_move_ladder`,
  `has_legal_move_in_view` (guard, screen, ladder),
  `has_legal_move_in_view_light`, `has_legal_move_in_view_unpopulated`, docs,
  unit tests (soundness, coverage, fixtures).
- `src/position.rs` — `terminal_in_view_light` closure now calls the light
  existential (plan 1's function body, one call site).
- `src/effects.rs` — `has_legal_move_after`, `is_terminal_after` use the
  unpopulated entry; module docs.
- `src/board.rs` — only if a `pub(crate)` visibility helper is needed for the
  witness (none expected).
- `tests/post_move.rs` (plan 1) — additive child-existence differential if it
  is not already covered; `tests/terminal_after.rs`,
  `tests/child_state.rs`, `tests/effects.rs` — unmodified.
- `examples/effect_bench.rs` — existential-share row (the plan-3 harness may
  land first; coordinate, do not duplicate).
- `CHANGELOG.md` (`[Unreleased]`), `src/lib.rs`, `docs/perf/perf_guide.md`.
- `docs/plans/feedback/report_handoff3_2.md` — final report.

## Non-goals

- No change to `TrivialLegality`, `WidenedLegality`, `FastLegality`, the
  pseudo-legal generator, or the generation-order contract.
- No "sufficient-only" public query: the public answers stay exact (`has_legal_move*`,
  `terminal*`, `is_terminal_after`); the screen is internal and a proof, never a
  filter that can change an answer.
- No commoner-move, castling, capture or en-passant reasoning in the screen; no
  in-check evasion reasoning (the ladder owns it).
- No change to the extinction guard semantics of `has_legal_move_in_view`
  (stale `StateInfo` may still only produce `false`).
- No new public API, no new dependency, no `unsafe`.
- No downstream-facing claim about search trees; the numbers are this crate's.

## Final task

Write `docs/plans/feedback/report_handoff3_2.md`: item status, the implemented
(crate-private) API, the witness-soundness and coverage counts (positions,
moves, witness hits), the acceptance fixture table, the benchmark table with the
ratios above (and absolute ns for information), changed files, problems,
deviations with their measurements, unresolved parts (which classes the screen
cannot serve), missing tests, and next steps (plan 3's composite verdict and
release).
