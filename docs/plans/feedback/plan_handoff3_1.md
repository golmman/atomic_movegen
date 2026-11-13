# Plan 1 — the child's terminal state on the summary, and the key next to it (MG19)

## Scope

Two additive API items and one internal refactor that they need:

- **MG19a:** `PostMove::terminal()` — the terminal state of an already derived
  child, without re-analyzing. This makes the private
  `EffectContext::terminal_from(&PostMove)` public in the only shape that can
  never drift: the method *is* the implementation, and `terminal_after` /
  `child_outcome` call it.
- **MG19b:** the post-move Zobrist key next to the summary, so one derivation
  yields key + summary (and, through MG19a, terminality):
  `EffectContext::post_move_and_key(&board, m) -> Option<(PostMove, u64)>`.
- **Internal:** a light terminal path that does not populate a `StateInfo` for
  the child (`position::terminal_cheap` / `position::terminal_tail` /
  `position::terminal_in_view_light`), because terminality needs neither pins
  nor a populated `StateInfo` — only commoner counts, "is the side to move in
  check", and the existence answer.

Requirements served: handoff 3 MG19 (P2). Acceptance bars from handoff 3 §2,
restated as this plan's gates in "Verification".

**Depends on nothing.** Plan 2 rewires the existence query used here; plan 3
measures the pair and releases.

## Current-tree assessment

- `src/effects.rs`:
  - `fn terminal_from(&self, child: &PostMove) -> Option<Terminal>` — private,
    ignores `self` entirely except for the call, populates a fresh
    `StateInfo` (`checkers`, `pinned`, counts) and calls
    `position::terminal_in_view(&child.view, &state, None)`.
  - `terminal_after`, `child_outcome` both funnel into it;
    `is_terminal_after` and `has_legal_move_after` populate their own
    `StateInfo` and re-implement the existence question on top.
  - `PostMove` exposes placement, casualties, side to move, ep square,
    castling rights and `rule50()` — no key, no terminality.
- `src/position.rs`: `terminal_in_view(view, state, moves_exist: Option<bool>)`
  is the single terminal precedence (extinction → opponent extinction →
  material draw → clock without check → existence → stalemate/mate → clock).
  It reads `state.commoners_count` / `state.them_commoners_count` /
  `state.checkers` and falls back to `movegen::has_legal_move_in_view`.
- `src/board.rs`: `hash_after` / `hash_and_rule50_after` are `&Board`-only and
  work incrementally from `self.hash`; the pieces a key builder needs
  (`piece_key`, `castling_squares`, `ZOBRIST`) exist but `piece_key` is
  module-private.
- Indicative measurement (index, "cost split"): `ctx.post_move` 15.1 ns,
  `ctx.terminal_after` 36.2 ns, `post_move` + `terminal_after` 52.9 ns,
  `populate_state` 7.4 ns, `has_legal_move_with_state` 10.4 ns,
  `board.hash_and_rule50_after` 6.2–14.0 ns on this machine (one `deep-win`
  child set). So today `post_move` + terminality pays `analyze` twice, and the
  terminal half pays a full population.

## Decisions and pushback

1. **`PostMove::terminal()` is the single implementation.** `terminal_from` is
   deleted, not wrapped: `terminal_after` becomes
   `self.analyze(board, m)?.terminal()` and `child_outcome` uses the same
   method. Handoff 3's acceptance ("`post_move(b, m).unwrap().terminal()`
   equals the cloned child's `terminal()`, same precedence as
   `terminal_after`; zero behaviour change to `terminal_after`/`child_outcome`
   (they call it)") is then true by construction rather than by parallel
   implementations.
2. **No `PostMove::is_terminal()`.** `Board::is_terminal` deliberately
   *excludes* the commoner-only material shortcut while `terminal_after`
   includes it; a `PostMove::is_terminal()` would sit next to
   `PostMove::terminal()` with the opposite semantics of the `Board` method of
   the same name. `EffectContext::is_terminal_after` stays the literal mirror
   (re-pointed at the light path by plan 2) and is the only "literal" query.
3. **The terminal path stops populating `StateInfo`.** What terminality needs
   is three facts about the child view: the two commoner counts, "is the side
   to move in check", and the existence answer. `pinned` is never read by the
   precedence, and the counts come from the view in one popcount each. The
   precedence itself is not rewritten: it is split into a cheap prefix
   (`terminal_cheap`) and a tail (`terminal_tail`) so the board path and the
   light path call the same branch code in the same order.
4. **The key is a paired query, not a field on `PostMove`.**
   `EffectContext::post_move_and_key(board, m)` returns `(PostMove, u64)`.
   Rationale, and the alternative that was considered:
   - A from-scratch key over the child placement (`PostMove::key()` computed on
     demand from the view) is ~30 XORs of table loads: not "free", and it
     duplicates the Zobrist composition in a second place.
   - Carrying `key: u64` inside `PostMove` (computed in `analyze`) is the
     shape handoff 3 suggests, but `analyze` is also the body of
     `effect`/`effect_and_post`/`classification`, which consumers run per
     legal move (~20 ns on this machine); an unconditional key makes the
     classification path pay for a value it did not ask for. `AGENTS.md` ranks
     correctness then performance, and a per-candidate regression on an
     existing path is not acceptable for a speculative convenience.
     **Measurement gate:** if the implementer measures `PostMove`-carried keys
     at ≤ 1 ns on `classification` and ≤ 2 ns on `post_move` (i.e. genuinely
     free, `analyze` split into a lean and a keyed variant so only the keyed
     call sites pay), the field may be added *in addition*, with
     `PostMove::key()` and `PostMove::hash_and_rule50()` as thin accessors.
     Record the measurements in the report either way.
   - The incremental delta (`board.hash()` XOR the squares this move changed)
     is the only shape that is cheap for the *consumer* that wants it, and it
     needs the parent hash — which is a `Board` fact, hence the paired query.
5. **Naming.** `post_move_and_key` follows the existing `effect_and_post`
   ("X and Y from one derivation"). Handoff 3's suggested spellings
   (`PostMove::key()` / `PostMove::hash_and_rule50()`) are satisfied in
   substance: one derivation yields summary, key and terminality.
6. **No public `PositionView`, no public constructor for `PostMove`.** Unchanged
   from 2.7.0.
7. **The board-side `terminal`/`outcome` precedence is not switched to the
   light path in this plan.** `Board::terminal`/`outcome` keep their populated
   `StateInfo` inputs (they are on the caller's node-constant path already) and
   their exact behaviour, including the documented stale-`StateInfo`
   contract. A board-side switch is a separate, measured decision.

## Proposed API

```rust
// src/effects.rs

impl PostMove {
    /// The terminal state of this child, from the child's side to move.
    ///
    /// `None` when the child is not decided. Equal to
    /// `{ let mut c = board.clone(); c.do_move(m, &mut st); c.terminal() }`
    /// for the move that produced this summary, with the same precedence as
    /// `EffectContext::terminal_after` (extinction → commoner-only material
    /// draw → no legal moves → clock; mate outranks the clock).
    ///
    /// Legality is deliberately not involved: the summary of a pseudo-legal
    /// move that explodes the mover's own commoner describes the (unreachable)
    /// extinct child and reports it as such, exactly like `terminal_after`.
    #[must_use]
    pub fn terminal(&self) -> Option<Terminal>;
}

impl EffectContext {
    /// The child summary and the child's Zobrist key, from one derivation.
    ///
    /// The key is exactly `board.hash_after(m)` and exactly the `hash()` of a
    /// cloned board after `do_move(m)`: same key table and composition as
    /// `Board::hash` (`EnPassantMode::Always`), so the stability contract and
    /// `ZOBRIST_VERSION` apply unchanged. Combined with
    /// `PostMove::terminal`/`PostMove::rule50`, one derivation answers
    /// "summary + key + terminality".
    ///
    /// Returns `None` for malformed input only (`Move::NONE`, no origin piece),
    /// like `post_move`. Defined for illegal moves: the key of the child
    /// `do_move` would build.
    #[must_use]
    pub fn post_move_and_key(&self, board: &Board, m: Move) -> Option<(PostMove, u64)>;
}
```

Unchanged signatures, re-pointed bodies: `EffectContext::terminal_after`,
`EffectContext::child_outcome`, `EffectContext::is_terminal_after`,
`EffectContext::has_legal_move_after`, `Board::terminal_after`,
`Board::child_outcome`.

Crate-private additions in `src/position.rs`:

```rust
/// The two cheap terminal exits: extinction and the commoner-only draw.
fn terminal_cheap(view: &PositionView, our_commoners: u32, their_commoners: u32)
    -> Option<Terminal>;

/// The terminal precedence tail: the clock, the existence question, mate or
/// stalemate, and the clock again.
fn terminal_tail(view: &PositionView, in_check: bool, moves_exist: impl FnOnce() -> bool)
    -> Option<Terminal>;

/// Kept: the board-side entry point, unchanged behaviour.
pub(crate) fn terminal_in_view(view: &PositionView, state: &StateInfo,
    moves_exist: Option<bool>) -> Option<Terminal>;

/// The child-side entry point: no `StateInfo`, facts read from the view.
pub(crate) fn terminal_in_view_light(view: &PositionView) -> Option<Terminal>;
```

## Implementation

1. **Split the precedence** in `src/position.rs`. Move the first three branches
   of `terminal_in_view` into `terminal_cheap(view, our, their)` and the rest
   into `terminal_tail(view, in_check, moves_exist)`, keeping the branch text
   verbatim (extinction → opponent extinction → material draw → clock without
   check → `moves_exist()` → stalemate/mate → clock). `terminal_in_view`
   becomes

   ```rust
   pub(crate) fn terminal_in_view(view: &PositionView, state: &StateInfo,
                                  moves_exist: Option<bool>) -> Option<Terminal> {
       terminal_cheap(view, state.commoners_count, state.them_commoners_count)?;
       terminal_tail(view, !state.checkers.is_empty(), || {
           moves_exist.unwrap_or_else(|| crate::movegen::has_legal_move_in_view(view, state))
       })
   }
   ```

   Note the laziness contract that must survive: the existence question is only
   asked when the cheap exits and the non-check clock branch did not decide.

2. **Add the light entry point.** `terminal_in_view_light(view)`:

   ```rust
   let us = view.side_to_move();
   let our = view.commoners(us).count();
   let their = view.commoners(us.flip()).count();
   terminal_cheap(view, our, their)?;
   let checkers = view.checkers(us);              // one query, shared below
   terminal_tail(view, !checkers.is_empty(), || {
       // Plan 1: the exact, populated query. Plan 2 replaces this with the
       // specialised existential that reuses `checkers`.
       let mut state = StateInfo::new();
       view.populate_state(&mut state);
       crate::movegen::has_legal_move_in_view(view, &state)
   })
   ```

   `checkers` is intentionally computed *after* `terminal_cheap`: extinction and
   the material draw must not pay for an attacker query.

3. **`PostMove::terminal()`** in `src/effects.rs`:

   ```rust
   pub fn terminal(&self) -> Option<Terminal> {
       crate::position::terminal_in_view_light(&self.view)
   }
   ```

   Delete `EffectContext::terminal_from`. `terminal_after` becomes
   `self.analyze(board, m)?.terminal()`; `child_outcome` replaces its
   `terminal_from` call with `child.terminal()`; `is_terminal_after` and
   `has_legal_move_after` keep their semantics but may already reuse
   `terminal_in_view_light` for the extinction/clock prefix (do **not** change
   what they answer: `is_terminal_after` still excludes the material draw).

4. **The key.** In `src/board.rs`, make the existing `piece_key` (and, if
   needed, `piece_key_raw`) `pub(crate)` — no behaviour change. In
   `src/effects.rs` add a private free function:

   ```rust
   /// The Zobrist key of `post`, `board_hash` being the parent's key.
   ///
   /// `Board::hash` is the XOR of the piece keys, the side key, the castling
   /// key of the current rights, and the ep-file key. A move changes the piece
   /// on the origin, on the destination, and on every square its blast (or its
   /// en-passant victim) empties — nothing else — plus side, rights, and ep, so
   /// the child key is the parent key XOR those deltas.
   fn child_key(board_hash: u64, parent: &PositionView, post: &PostMove, m: Move) -> u64;
   ```

   Exact composition to implement and to document in the function:
   - `h = board_hash ^ ZOBRIST.side` (the side key is always flipped).
   - Squares emptied by the move:
     `out = {from} ∪ {to} ∪ post.casualties().all()`. XOR out
     `piece_key(sq, parent.piece_on(sq))` for each square in `out`
     (`ZOBRIST.piece[sq][0] == 0`, so empty squares are a no-op).
   - Squares filled by the move: for castling, the king and rook destinations
     from `board::castling_squares(us, to > from)`; otherwise `{to}`. XOR in
     `piece_key(sq, post.piece_on(sq))` (empty at `to` for a capture, because
     the capturer dies at ground zero).
   - Castling rights: if `parent.castling_rights() != post.castling_rights()`,
     XOR `ZOBRIST.castling[parent] ^ ZOBRIST.castling[post]`.
   - En passant: if the target changed, XOR the old and the new ep-file keys
     (`ZOBRIST.ep[file]`, index 8 for "none"; check how `Board::hash`
     represents "no ep" and mirror it exactly).
   - Promotions need no special case: `from` loses the pawn, `to` gains
     `post.piece_on(to)`.

   `post_move_and_key` = `self.analyze(board, m)` then apply `child_key` with
   `board.hash()`.

5. **Docs.** `src/effects.rs` module docs: a "terminal state on the summary"
   paragraph and a "child key" paragraph with one doctest each; `PostMove`
   struct docs gain the terminality note (`terminal()` is a rules verdict, the
   summary itself still carries none). `README.md` and `CHANGELOG.md`
   (`[Unreleased]`) get the two additions. `src/lib.rs`'s effect section and
   the allocation-free list gain `PostMove::terminal` and
   `post_move_and_key` (neither allocates, neither writes a board).

## Tests

New integration test file `tests/post_move.rs` (uses `tests/common/mod.rs`),
plus unit tests at the bottom of `src/effects.rs` and `src/position.rs`.

### MG19a — terminality on the summary

For every pseudo-legal move of the 41-position perft corpus, of the plan-1/2/3
child-state fixtures (`tests/child_state.rs`, `tests/terminal_after.rs`), and of
seeded play-outs (`common::walk_playouts`, ≥ 5 start FENs × 16 seeds × 300
plies; `common::walk_corpus_and_fixtures`):

- `ctx.post_move(&b, m).unwrap().terminal() == {let mut c = b.clone();
  c.do_move(m, &mut st); c.terminal()}` — exact `Terminal { outcome, reason }`,
  for **illegal** pseudo-legal moves too (self-explosion → `Loss`/`Extinction`).
- `ctx.terminal_after(&b, m) == ctx.post_move(&b, m).and_then(|p| p.terminal())`
  — the "zero behaviour change, they call it" identity.
- `ctx.child_outcome(&b, m).terminal() == ctx.post_move(&b, m).unwrap().terminal()`
  for legal moves; `AfterState::Illegal` still wins for illegal ones.
- Malformed input: `post_move(Move::NONE)`/empty origin are `None` (unchanged).
- `position::terminal_in_view_light(&view) == position::terminal_in_view(&view,
  &populated_state, None)` for every position of the view corpus (41 perft FENs
  + the effect/child fixtures + the seeded play-outs) — the precedence is the
  same code, so this pins the split.

Deterministic fixtures (all must be listed in the test with their expected
`Terminal`, one row per branch, reusing the fixture FENs already in
`tests/terminal_after.rs`): stalemate child, mate child (`NoMovesInCheck`), mate
on the 100th half-move, extinction child, commoner-only material draw child,
`FiftyMoveRule` child, an undecided child, and the self-explosion
`8/8/8/8/8/8/8/3qK3 w - - 0 1` `e1d1` (extinct child from an illegal move).

### MG19b — the key

For every pseudo-legal move of the same corpora (and, in the play-out walk, at
every visited position):

- `let (post, key) = ctx.post_move_and_key(&b, m).unwrap();`
  `key == {clone; do_move; hash()}`;
- `key == b.hash_after(m)` and `(key, post.rule50()) == b.hash_and_rule50_after(m)`;
- `ctx.post_move_and_key(&b, m).map(|(p, _)| p) == ctx.post_move(&b, m)`;
- `None` for `Move::NONE` and for a move with an empty origin square.

Explicit fixture rows (each also asserted against the cloned child): a quiet
pawn double push (ep key in), a capture with a blast (multiple pieces out), an
en-passant capture (victim square + new ep = none), a quiet promotion, a
capturing promotion, a castling move (king + rook, rights out), a capture that
removes a rook on a home square (rights out), a move from/to the back ranks
(rights unchanged, quiet), a capture after which the stored ep square is
dropped, and a capture of the last enemy commoner.

### Unit tests

- `src/position.rs`: `terminal_cheap`/`terminal_tail` precedence unit test on
  hand-built views (branch order, mate-outranks-clock, clock-without-check does
  not ask the existence question — assert via a `moves_exist` closure that
  panics if called).
- `src/effects.rs`: `post_move_terminal_matches_terminal_after` and
  `child_key_matches_hash_after` on the module `CORPUS`.

## Verification

All global gates of the index, plus:

- `cargo test --test terminal_after --test child_state --test effects
  --test conformance --test public_api` (unchanged expectations: no behaviour
  change).
- `cargo test --release --test post_move -- --nocapture` (report the visited
  position/move counts in the report).
- `cargo run --release --example effect_bench 9 2000`, three runs, medians,
  reporting at least:

  | Row | Gate |
  | --- | --- |
  | `post_move` + `PostMove::terminal()` | **≤ 1.05×** `ctx.terminal_after` of the same run (it *is* the same code path; this pins that the public composite does not cost more than the existing query) |
  | `ctx.post_move` | **no regression** vs the pre-change three-run median (the key must not be paid here) |
  | `post_move_and_key` | **≤ `post_move` + 3 ns** on this machine (and report the delta as a ratio to `clone`+`do_move` so the downstream can rescale) |
  | `classification`, `effect`, `effect_and_post` | no regression (unchanged code path — the split in `analyze` must not add work) |

  Handoff 3's own MG19 target (`post_move` + terminality < `terminal_after`) is
  met by construction; the *useful* comparison (`post_move` + terminality +
  key against `clone`+`do_move`+`outcome()` on captures) is plan 3's composite
  measurement and is expected to need plan 2. State that plainly in the report.
- `cargo run --release --example verify_perft 5` (41/41) and, if time permits,
  depth 6; perft timing within noise (no generator/legality change).

## Files to touch

- `src/position.rs` — `terminal_cheap`, `terminal_tail`, `terminal_in_view`
  (re-composed, same behaviour), `terminal_in_view_light`, unit tests.
- `src/effects.rs` — `PostMove::terminal`, delete `terminal_from`, re-point
  `terminal_after`/`child_outcome`/`is_terminal_after`/`has_legal_move_after`,
  `child_key`, `post_move_and_key`, module docs, unit tests.
- `src/board.rs` — `piece_key`/`piece_key_raw` become `pub(crate)` (no
  behaviour change).
- `tests/post_move.rs` — **new** differential tests.
- `tests/child_state.rs`, `tests/terminal_after.rs` — additive assertions only;
  no expectation may change.
- `examples/effect_bench.rs` — the two new row groups.
- `README.md`, `CHANGELOG.md` (`[Unreleased]`), `src/lib.rs` — docs.
- `docs/plans/feedback/report_handoff3_1.md` — final report.

## Non-goals

- No `PostMove::key()` / `hash_and_rule50()` field unless the measurement gate
  of decision 4 passes; never at the cost of `classification`/`effect`.
- No `PostMove::is_terminal()` (decision 2), no new public view type, no public
  `PostMove` constructor.
- No change to `Board::hash`, `hash_with`, `hash_after`, `hash_after_with`,
  `rule50_after`, the Zobrist table or `ZOBRIST_VERSION`. The key must be equal
  to what the existing implementation produces, not a new composition.
- No change to `Board::terminal`/`outcome`/`is_terminal` behaviour; no switch of
  the board path to the light precedence.
- No `EnPassantMode::Legal` key, no keyed `PostMove` for `EnPassantMode::Legal`.
- No deprecation or removal of `terminal_after`, `child_outcome`,
  `outcome_after`, `is_terminal_after`, `has_legal_move_after`.
- No movegen change (plan 2 owns that).

## Final task

Write `docs/plans/feedback/report_handoff3_1.md`: item status for MG19a/MG19b,
the implemented API, differential counts, whether the `PostMove`-carried key was
tried and what it cost, the benchmark table (absolute ns and the ratios above),
changed files, problems, deviations with their measurements, unresolved parts,
missing tests, and next steps (plan 2's rewiring and plan 3's composite
measurement).
