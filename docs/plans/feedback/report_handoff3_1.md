# Implementation report — handoff 3 plan 1 (MG19: terminal state on the summary, key next to it)

Status: **complete**. MG19a (`PostMove::terminal`) and MG19b
(`EffectContext::post_move_and_key`) are in the tree with the differential
suites, the docs and the bench rows the plan asked for. The internal refactor
(the light terminal path) landed as `position::terminal_cheap` /
`terminal_tail` / `terminal_in_view_light`. One gate is **missed** (the
`post_move_and_key` cost gate, by 1–4 ns); the measurement and its cause are in
"Deviations" below. No `PostMove`-carried key field was added (its measurement
gate fails by an order of magnitude — see decision 4 in "Deviations").

Version in the tree: `2.7.0` (`Cargo.toml` untouched; the release to 2.8.0 is
plan 3's step).

## Item status

| Item | Status | Where |
| --- | --- | --- |
| MG19a `PostMove::terminal()` | **done** | `src/effects.rs` (`impl PostMove`), re-pointed `terminal_after`, `child_outcome` |
| MG19b `EffectContext::post_move_and_key()` | **done** (with a cost gate miss) | `src/effects.rs`, plus the private `child_key` |
| Split precedence (`terminal_cheap` / `terminal_tail` / `terminal_in_view_light`) | **done** | `src/position.rs` |
| `piece_key` crate-visible | **done** | `src/board.rs` |
| `PostMove`-carried key (`PostMove::key()`) | **not added**, gate fails | see "Deviations" |
| `PostMove::is_terminal()` | **not added** (decision 2) | — |
| Board path switched to the light precedence | **not done** (non-goal) | — |

## Implemented API

Public (both additive, neither allocates, neither writes a board):

```rust
impl PostMove {
    /// The terminal state of this child, from the child's side to move.
    #[must_use]
    pub fn terminal(&self) -> Option<Terminal>;
}

impl EffectContext {
    /// The child summary and the child's Zobrist key, from one derivation.
    #[must_use]
    pub fn post_move_and_key(&self, board: &Board, m: Move) -> Option<(PostMove, u64)>;
}
```

Crate-private:

```rust
// src/position.rs
fn terminal_cheap(view: &PositionView, our_commoners: u32, their_commoners: u32)
    -> Option<Terminal>;
fn terminal_tail(view: &PositionView, in_check: bool, moves_exist: impl FnOnce() -> bool)
    -> Option<Terminal>;
pub(crate) fn terminal_in_view(view: &PositionView, state: &StateInfo,
    moves_exist: Option<bool>) -> Option<Terminal>;                 // re-composed, same branches
pub(crate) fn terminal_in_view_light(view: &PositionView) -> Option<Terminal>;
impl PositionView {
    pub(crate) fn populate_state(&self, state: &mut StateInfo);      // unchanged behaviour
    pub(crate) fn populate_state_with_checkers(&self, state: &mut StateInfo, checkers: Bitboard);
}

// src/effects.rs
fn child_key(board: &Board, parent: &PositionView, post: &PostMove, m: Move) -> u64;

// src/board.rs
pub(crate) fn piece_key(sq: Square, piece: Piece) -> u64;            // was `fn`
```

Unchanged signatures, re-pointed bodies: `EffectContext::terminal_after`
(now literally `self.analyze(board, m)?.terminal()`),
`EffectContext::child_outcome` (now `child.terminal()`), and the board wrappers
`Board::terminal_after` / `Board::child_outcome`. `terminal_from` was deleted,
not wrapped, so the "zero behaviour change, they call it" identity is true by
construction.

`child_key` composition (documented on the function): parent key XOR the side
key XOR the parent piece keys of `{from} ∪ casualties` (plus the rook's origin
for castling) XOR the child piece keys of the filled squares XOR the castling
key delta XOR the old/new en-passant file keys. Two details worth naming:

- `ZOBRIST.ep[8]` is **not** an all-zero "no ep" key — `zobrist::generate()`
  fills indices 0..9 with PRNG values even though `Board::hash` never uses index
  8. XORing `ep[8]` for "no target" is wrong (it was my first bug); the
  function XORs an ep key only for a stored square.
- A capture's ground zero is already part of `casualties`, and for a quiet move
  nothing sits on `to`, so `{from} ∪ casualties` is the exact emptied set;
  `to` is only added for castling (where it is the rook's origin).

## Differential counts

`cargo test --release --test post_move -- --nocapture`:

```text
post_move: 15018 positions, 285150 pseudo-legal moves, 29874 decided children
```

(`terminal_after`'s own suite re-reports its counts for the same walk:
15013 positions, 285064 pseudo-legal moves, 29842 decided children.)

Corpora: the 41 perft positions, the 18 fixtures of `tests/post_move.rs` (the
`terminal_after`/`child_state` branch fixtures plus three key fixtures), and
`common::walk_playouts` (6 start FENs × 16 seeds × 300 plies + 41 corpus
positions × 2 seeds × 60 plies).

Asserted per pseudo-legal move, legal or not:

- `post.terminal() == child.terminal() == child.terminal_with_state(&child_state)`;
- `ctx.terminal_after(b, m) == ctx.post_move(b, m).and_then(|p| p.terminal())`;
- `ctx.child_outcome(b, m).terminal() == post.terminal()` for legal moves, and
  `AfterState::Illegal` still wins for illegal ones;
- `(post, key) == (ctx.post_move(b, m), board.hash_after(m)) == (post, child.hash())`
  and `(key, post.rule50()) == board.hash_and_rule50_after(m)`;
- `None` for `Move::NONE`, an empty origin, and an origin holding an enemy piece.

Precedence split equivalence (crate-private, so a unit test in
`src/position.rs`): `terminal_in_view_light(view) == terminal_in_view(view,
&populated_state, None) == board.terminal()` for every corpus position and every
child of every corpus position (1000+ positions, 34 of them decided) and at
every position of a seeded walk of up to 200 plies over three start FENs.

## Benchmarks (same run, three runs, medians)

`cargo run --release --example effect_bench 9 2000`, three runs, nine
position-rows per column (3 bench positions × 3 runs), ns per call. "ratio" is
against the build-only row of the same run (`clone` + `do_move`, which the bench
already reports).

| Row | before | after | after ÷ build-only |
| --- | ---: | ---: | ---: |
| `classification` (per candidate) | 20.0 | 20.0 | 1.06 |
| `effect.full` (per candidate) | 26.0 | 26.0 | 1.38 |
| `board.legal` (per candidate) | 8.0 | 8.0 | 0.43 |
| `EffectContext::post_move` | 21.0 | **20.0** | 1.06 |
| `clone` + `do_move` (build only) | — | 18.8 | 1.00 |
| `clone` + `do_move` + facts | 24.0 | 25.0 | 1.33 |
| `EffectContext::post_move` + `PostMove::terminal()` | — | **40.0** | 2.13 |
| `EffectContext::terminal_after` | 42.0 | 44.0 | 2.34 |
| `clone` + `do_move` + `terminal()` | 31.0 | 31.0 | 1.65 |
| `EffectContext::post_move_and_key` | — | **24.0** | 1.28 |
| `post_move` + `Board::hash_after` (separate) | — | 21.0 | 1.12 |
| `Board::hash_after` (alone) | — | 5.0 | 0.27 |
| `has_legal_move_after` | 38.0 | 37.0 | 1.97 |
| `clone` + `do_move` + `has_legal_move` | 31.0 | 30.0 | 1.60 |

The "build only" divisor (18.8 ns) is from a temporary sub-nanosecond probe (see
below), not from an `effect_bench` row: the bench's `clone+do_move+facts` row
includes reading the facts.

Sub-nanosecond cross-check. The bench prints integer ns, so I also measured with
a throwaway probe (not committed; `deep-win`, its 32 pseudo-legal children, min
of 9 batches × 200 000, three runs):

| Row | ns (three runs) |
| --- | --- |
| `post_move` | 15.75 / 15.75 / 16.34 |
| `post_move` + `terminal()` | 39.56 / 41.19 / 39.45 |
| `terminal_after` | 43.78 / 43.18 / 39.61 |
| `clone` + `do_move` + `terminal()` | 34.38 / 34.26 / 33.87 |
| `post_move_and_key` | 23.58 / 24.80 / 23.96 |
| `post_move` + `hash_after` | 21.23 / 21.30 / 21.21 |
| `hash_after` | 7.20 / 7.11 / 7.15 |
| `clone` + `do_move` | 18.90 / 19.07 / 18.48 |
| `clone` + `do_move` + `outcome()` + key | 34.85 / 34.93 / 34.33 |
| `classification` | 21.22 / 21.54 / 21.22 |
| `has_legal_move_after` | 37.83 / 37.99 / 39.82 |

Pre-change probe numbers for the rows that existed then: `post_move`
14.79–15.01, `hash_after` 6.72–6.85, `clone+do_move` 17.49–17.84,
`classification` 20.22–20.35, `post_move`+`terminal()` 42.44–45.69 (that run
still populated the child state twice, see "Deviations").

## Gates

| Gate | Result |
| --- | --- |
| `cargo test` | 13 result blocks (12 unit/integration binaries + doctests), all ok, 0 failures: 170 lib, 5 `post_move`, 7 `child_state`, 6 `terminal_after`, 12 `effects`, 6 `conformance`, 4 `perft_tests`, 3 `public_api`, 1 `move_order_golden`, 1 `verify_moves`, 3 `interchange`, 2 `null_for`, 16 doctests |
| `cargo test --features unsafe-optimizations` | same 13 result blocks, all ok |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `cargo doc` (+ `cargo test --doc`) | no warnings, 16 doctests ok |
| `verify_perft 5` (three runs) | 41/41 each; 2.198 s before / 2.200 s after (median, **+0.1 %**) |
| `verify_perft 6` | 41/41, 63.84 s (63.98 s before the last edit) |
| `perft_tests`, `move_order_golden`, `verify_moves`, `conformance`, `effects`, `terminal_after`, `child_state`, `public_api`, `post_move` | all ok, no expectation changed |
| `effect_bench 9 2000` × 3 | table above |
| `post_move` + `terminal()` ≤ 1.05 × `terminal_after` | **met** (0.91 × in the bench, 0.92–0.96 × in the probe) |
| `ctx.post_move` no regression | **met** (21.0 → 20.0 ns; the key is not paid here) |
| `post_move_and_key` ≤ `post_move` + 3 ns | **missed**: +4 ns in the bench, +7.5 ns in the same-box probe (≈ the cost of `hash_after` itself, 7.15 ns) |
| `classification`, `effect`, `effect_and_post` no regression | **met** (20.0 → 20.0 ns, 26.0 → 26.0 ns) |

## Changed files

- `src/position.rs` — `terminal_cheap`, `terminal_tail`, `terminal_in_view`
  (re-composed), `terminal_in_view_light`, `populate_state_with_checkers`, module
  doc, four unit tests (`terminal_cheap_branches`, `terminal_tail_branches`,
  `terminal_light_matches_board_path`,
  `terminal_light_matches_board_path_playouts`).
- `src/effects.rs` — `PostMove::terminal`, `child_key`,
  `EffectContext::post_move_and_key`, `terminal_from` deleted, `terminal_after`/
  `child_outcome` re-pointed, module docs (two sections + doctests), `PostMove`
  and `post_move` docs, three unit tests
  (`post_move_terminal_matches_terminal_after`, `child_key_matches_hash_after`,
  `post_move_and_key_rejects_malformed_moves`).
- `src/board.rs` — `piece_key` is `pub(crate)` (one word; no behaviour change).
- `tests/post_move.rs` — **new**, 5 differential tests, 18 fixtures.
- `tests/child_state.rs`, `tests/terminal_after.rs`, `tests/public_api.rs` —
  additive assertions only.
- `examples/effect_bench.rs` — two new row groups (`summary + terminal`,
  `summary + key`), module doc.
- `README.md`, `CHANGELOG.md` (`[Unreleased]`), `src/lib.rs` — docs.
- `docs/plans/feedback/report_handoff3_1.md` — this report.

## Problems encountered

1. **`terminal_cheap(...)?` in an `Option`-returning function.** My first
   version propagated the *prefix decision* as `None` instead of returning it, so
   every position the cheap exits decided fell through and every position they
   did not decide returned `None` immediately. Caught by the differential tests
   (a stalemate child came back `None`); it changed `Board::terminal` too, i.e.
   it would have been a semantics change, not just a slowdown. Fixed by
   `if let Some(terminal) = … { return Some(terminal); }` in both entry points.
   Lesson for plan 2: the split makes the two exit shapes easy to confuse; the
   `terminal_light_matches_board_path` unit test is what pins it.
2. **`ZOBRIST.ep[8]` is not zero.** `zobrist::generate()` fills all nine ep keys
   with PRNG values while `Board::hash` only ever applies a key for a stored
   square. The first `child_key` XORed `ep[8]` for "no target" and disagreed with
   `hash_after` on every move without an ep target. Documented in `child_key`;
   `tests/post_move.rs` now asserts the key against `hash_after` and against a
   cloned `do_move` child for every pseudo-legal move.
3. **`pinned` *is* read by the legality ladder.** Decision 3 reads as if the
   child path could drop the populated `StateInfo` entirely ("`pinned` is never
   read by the precedence"), which is true of the precedence but not of
   `movegen::has_legal_move_in_view`: `TrivialLegality::new` builds
   `safe_origins = !(commoners | state.pinned)`, so an empty `pinned` would let
   the fast path accept quiet moves of pinned pieces. The light path therefore
   populates the state for the existence query — but reuses the `checkers` value
   it already computed (`populate_state_with_checkers`), which was the only
   avoidable duplicate.
4. **Fusing the key is not free, and the gate's premise does not hold on this
   machine.** See "Deviations".

## Deviations from the plan, and the measurements that motivated them

1. **`child_key` signature: `(board: &Board, parent: &PositionView, …)` instead
   of `(board_hash: u64, parent: &PositionView, …)`**, and the parent's piece
   contents are read with `board.piece_on(sq)` (an array read) instead of
   `parent.piece_on(sq)` (a scan over the six per-type bitboards plus a color
   test). Reason: measured. With `parent.piece_on` the key delta was
   ~11 ns per child (bench rows boxing the same `rule50` + terminal +
   key/hash_after work: `post_move_and_key` 53–54 ns against `post_move` +
   `hash_after` 47–49 ns, with `hash_after` alone at 5 ns); with the board lookup
   and the branch-light quiet path it is ~7.5 ns (probe) / +4 ns (bench rows). The context's view is a snapshot of the board it
   was built from (`Board::effect_context`), which is the same assumption every
   other `EffectContext` query already makes (`rule50_after`, `hash_after`, …).
2. **`populate_state_with_checkers` added** (crate-private) instead of leaving
   `terminal_in_view_light` to call `view.populate_state` as the plan's snippet
   shows. Reason: the light path must compute `checkers` for the precedence, and
   `populate_state` computes it again; the probe's `post_move`+`terminal()` was
   42–46 ns with the duplicate, 39.6–41.2 ns without. The helper is three lines
   and `populate_state` is now its first caller.
3. **The `PostMove`-carried key (decision 4's measurement gate) was measured and
   rejected.** A field computed inside `analyze` would cost the key delta on
   every `classification`/`effect`/`effect_and_post` call, which today measure
   21.2 ns (`classification`) and 16–20 ns (`post_move`). The cheapest key delta
   this crate has — `Board::hash_after` — is 7.15 ns on this machine; my
   `child_key` is 7.5 ns. That is 7× the plan's 1 ns gate on `classification`, so
   the field was not added, and `PostMove::key()` / `PostMove::hash_rule50()` do
   not exist. (Even the other shape — a from-scratch key over the child
   placement — was estimated at ~30 table loads in the plan and was not built.)
4. **The `post_move_and_key` cost gate is missed**: `post_move_and_key` (24 ns
   bench median, 23.6–24.8 ns probe) is `post_move` (20 ns / 15.8–16.3 ns) +
   **4–7.5 ns**, not +3 ns. Worse, the fused query is *slower* than the two
   separate calls: `post_move` + `hash_after` is 21.0–21.3 ns. The gate assumed an
   incremental child key is "a small handful of XORs"; on this machine the
   already-optimised `Board::hash_after` is 7.15 ns, so no fused implementation
   can be ≤ 3 ns. The honest statement is: **the key is not free, and the paired
   query buys an API shape, not speed.** I tried two variants that did not help:
   computing the key from `position::child_view` before building the `PostMove`
   (same numbers, and it made `post_move` itself 26 ns because `analyze` stopped
   being inlined) and a generic bitboard loop over the filled squares (11 ns).
   Both were reverted; the retained shape is `analyze` + `child_key(&PostMove)`.
5. **MG19a's own value is only visible on the composite, as the plan predicted.**
   `post_move` + `terminal()` (40 ns bench / ~40 ns probe) is now *the same code*
   as `terminal_after` (44 ns bench / 40–43 ns probe), i.e. the duplicate
   `analyze` is gone; but it is still slower than `clone` + `do_move` +
   `terminal()` (31 ns bench / 34 ns probe), because the existence question still
   pays a full `populate_state` plus the ladder. That is exactly plan 2's
   target; the MG19 target in handoff 3 §2 (`post_move` + terminality <
   `terminal_after`) is met by construction, but the *useful* comparison is
   plan 3's composite.
6. **Docs wording.** `PostMove::terminal`'s and `terminal_after`'s first drafts
   claimed the child path "does not populate a `StateInfo`"; after problem 3 that
   is wrong, so both now say the attacker query is run once and the state is
   populated only when the existence question is reached.

## Unresolved parts

- `post_move` + terminality is still ~1.2 × the cost of `clone` + `do_move` +
  `terminal()` on this machine (plan 2's target).
- The fused key is not cheaper than `post_move` + `hash_after`. Whether to keep
  `post_move_and_key` as an API convenience or to document
  `post_move` + `hash_after` as the faster shape is a documentation/policy
  question for plan 3 (this plan may not deprecate or remove anything).
- `EffectContext::is_terminal_after` and `has_legal_move_after` still build their
  own `StateInfo` (`is_terminal_after` unchanged; `has_legal_move_after`
  unchanged). Plan 2 re-points them; plan 1 deliberately did not change what they
  answer.
- No measurement of `EnPassantMode::Legal` keys on the summary (explicit
  non-goal).

## Missing tests

- No test drives `post_move_and_key` from `Setup`-built positions with more than
  one commoner per side (artificial boards); `blast_casualties` already asserts
  throw-away invariants there, and the key would follow the same recipe.
- No differential test for a *pseudo-legal but unreachable* `EnPassant` encoding
  whose victim square is empty (the generator never produces one); `hash_after`
  has the same gap.
- The play-out walk uses `generate_legal_with_state`, so illegal-move coverage
  comes from the corpora's self-explosion fixtures rather than from depth. That
  mirrors `tests/terminal_after.rs`; a walk over *pseudo-legal* moves would be a
  stronger (and slower) variant nobody has asked for.
- No `cargo bench`-style regression gate: the numbers in this report are manual
  runs, as the plan requires.

## Next steps

1. **Plan 2 (MG20)**: `movegen::has_legal_move_in_view_light(view, checkers)` —
   reuse `terminal_in_view_light`'s `checkers` (already threaded through
   `populate_state_with_checkers`), add the positive quiet-move screen with an
   early exit, and wire it into `terminal_in_view_light`,
   `has_legal_move_after` and `is_terminal_after`. The measured head-room is the
   rest of the gap: `post_move` + terminality 40 ns vs `clone` + `do_move` +
   `terminal()` 34 ns, and the existence query alone is 37–40 ns
   (`has_legal_move_after`) against `clone` + `do_move` + `has_legal_move` 30 ns.
2. **Plan 3**: the composite measurement (`post_move_and_key(...) →
   (post.terminal(), key)` against `clone` + `do_move` + `outcome()` + `hash()`:
   today 50.4 ns vs 34.6 ns on the probe — plan 2 has to move that), the docs,
   and the 2.8.0 checklist. Report the key cost honestly: the fused key is not
   free (+4 ns bench / +7.5 ns same-box) and does not beat
   `post_move` + `hash_after`.
3. If plan 3 wants a key that *is* free, the only remaining shape is a
   `PostMove` field computed by a keyed variant of `analyze` that only the keyed
   call sites use; the measurement to beat is 7.5 ns on `post_move` and ≤ 1 ns on
   `classification`. Nothing in this plan found a cheaper key.
