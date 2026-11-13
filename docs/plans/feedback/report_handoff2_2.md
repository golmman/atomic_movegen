# Report — Plan 2: child information (MG16, MG17)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| `AfterState { Illegal, Undecided, Terminal(Terminal) }` (`src/effects.rs`) | Done | `#[non_exhaustive]`, `Hash`, with `is_illegal`/`is_undecided`/`is_terminal`/`terminal`/`outcome`. |
| `EffectContext::child_outcome` | Done | Exact for pseudo-legal input; one analysis pass (legality ladder + terminal precedence). |
| `Board::child_outcome` | Done | Thin wrapper for callers without a context. |
| Shared `terminal_from` (used by `terminal_after` *and* `child_outcome`) | Done | Private; the two queries cannot drift. |
| `outcome_after` documentation fix + re-point | Done | Signature/behaviour unchanged; body is `child_outcome(..).outcome()` (one analysis pass instead of two). |
| `Board::outcome_after` doc | Done | Already unambiguous from plan 1; now also points at `Board::child_outcome`. |
| Public `PostMove` with the planned accessors | Done | Fields private, `#[inline]` on every accessor, `Debug` mirrors the data, `PartialEq`/`Eq` added (`PositionView` gained the same derives). |
| `EffectContext::post_move` | Done | Thin wrapper over `analyze`; `None` only for malformed input. |
| `EffectContext::effect_and_post` | Done | `effect` = `effect_and_post(..).0`; one child derivation for both halves. |
| `MoveEffect` unchanged (fields and size) | Done | No placement field; `casualties` remains the effect-path summary. |
| `conformance::CHILD_CASES` (optional) | Done | 11 frozen rows: one per `TerminalReason`, three `Undecided`, two `Illegal`; each with the child's legal move count. |
| Arm differential (`tests/child_state.rs`) | Done | 15,016 positions / 285,112 pseudo-legal moves. |
| Content differential (`tests/child_state.rs`, `src/effects.rs`) | Done | Every accessor vs a cloned `do_move` child, all 64 squares on the deep corpus. |
| Deterministic fixtures | Done | Table below, each row also cross-checked against the cloned child. |
| Unit tests in `src/effects.rs` | Done | Arms, content, `effect_and_post` identity, malformed input, game-over parents. |
| Docs (`effects`, `lib.rs`, `README`, `CHANGELOG`, `conformance`) | Done | |
| Global gates | Done | See “Verification run”. |
| Final report | Done | This file. |

## Public API as implemented

```rust
// src/effects.rs, both re-exported at the crate root.

/// What an `EffectContext` knows about the child after a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AfterState {
    Illegal,
    Undecided,
    Terminal(Terminal),
}

impl AfterState {
    pub fn is_illegal(&self) -> bool;
    pub fn is_undecided(&self) -> bool;
    pub fn is_terminal(&self) -> bool;
    pub fn terminal(&self) -> Option<Terminal>;
    pub fn outcome(&self) -> Option<Outcome>;
}

/// The piece placement and position data of the child after `m`, derived
/// without a board write. Facts only: no legality, no terminality, no
/// material-draw shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostMove { /* private: view + casualties + is_capture */ }

impl PostMove {
    pub fn side_to_move(&self) -> Color;
    pub fn occupied(&self) -> Bitboard;
    pub fn pieces_color(&self, c: Color) -> Bitboard;
    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard;
    pub fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard;
    pub fn piece_on(&self, sq: Square) -> Piece;
    pub fn casualties(&self) -> BlastCasualties;
    pub fn is_capture(&self) -> bool;
    pub fn ep_square(&self) -> Option<Square>;
    pub fn castling_rights(&self) -> u8;
    pub fn rule50(&self) -> u16;
}

impl EffectContext {
    /// Two-level child answer; `Illegal` iff `!board.legal(m, &state)`.
    pub fn child_outcome(&self, board: &Board, m: Move) -> AfterState;
    /// Child summary; `None` only for malformed input.
    pub fn post_move(&self, board: &Board, m: Move) -> Option<PostMove>;
    /// `effect` and `post_move` from one analysis pass.
    pub fn effect_and_post(&self, board: &Board, m: Move) -> (MoveEffect, Option<PostMove>);
    /// Unchanged signature/behaviour, now `child_outcome(..).outcome()`.
    pub fn outcome_after(&self, board: &Board, m: Move) -> Option<Outcome>;
}

impl Board {
    pub fn child_outcome(&self, m: Move, state: &StateInfo) -> AfterState;
}

// src/conformance.rs
pub struct ChildCase {
    pub name: &'static str,
    pub fen: &'static str,
    pub uci: &'static str,
    pub after_state: AfterState,
    pub child_legal_count: usize,
}
pub static CHILD_CASES: &[ChildCase]; // 11 rows
```

`MoveEffect` is byte-for-byte the same six fields. No public constructor for
`PostMove` exists; only an `EffectContext` builds one. No `PositionView` became
public. No `outcome_after` rename, removal or deprecation.

## Differential counts (the gates)

`cargo test --test child_state -- --nocapture`:

```
child_outcome: 15016 positions, 285112 pseudo-legal moves
               (22745 illegal, 238802 undecided, 23565 terminal)
```

Every one of those candidates was compared, for the *same* move:

| Assertion | Result |
| --- | --- |
| `AfterState::Illegal` iff `!board.legal(m, &state)` (and iff the legal list lacks `m`) | equal |
| `Terminal(t)` iff `clone.do_move.terminal() == Some(t)` (exact reason) | equal |
| `Undecided` iff `clone.do_move.terminal() == None` | equal |
| `board.child_outcome(m, &state) == ctx.child_outcome(&board, m)` | equal |
| `is_illegal`/`is_undecided`/`is_terminal`/`terminal`/`outcome` consistent with the variant | equal |
| `ctx.outcome_after` == `child_outcome(..).outcome()` (the documented mapping) | equal |
| `ctx.is_legal` == `!child_outcome(..).is_illegal()` | equal |

Content differential (same corpus, same moves, `child = clone; do_move;
populate_state`):

| Assertion | Result |
| --- | --- |
| `post.occupied()` / `pieces_color(c)` / `pieces_pt(pt)` / `pieces_color_pt(c, pt)` | equal |
| `post.piece_on(sq)` for all 64 squares (deep corpus; 8 sampled squares on play-outs) | equal |
| `post.casualties() == board.blast_casualties(m)` | equal |
| `post.is_capture() == board.is_capture(m)` | equal |
| `post.side_to_move()` / `ep_square()` / `castling_rights()` / `rule50()` | equal |
| `effect_and_post(..) == (effect(..), Some(post_move(..)))` | equal |

Corpora: the 41-position perft table, the 15 fixtures below, seeded play-outs
(6 start FENs × 16 seeds × depth 300, plus every perft position × 2 seeds ×
depth 60). `tests/effects.rs` (unchanged assertions) still checks 15,025
positions / 285,193 candidates against `Board::legal` and a cloned child.

Unit tests in `src/effects.rs` repeat both differentials on the 18-position
module corpus (`child_outcome_arms_match_legal_and_cloned_child`,
`post_move_matches_do_move`) and pin malformed input
(`post_move_rejects_malformed_moves`) and game-over parents
(`game_over_positions_are_never_legal` now also asserts the `Illegal` arm).

`tests/conformance.rs::child_conformance_corpus_verifies` re-derives all 11
`CHILD_CASES` rows from their FEN + UCI: arm, `Board::child_outcome`,
`outcome_after`, `is_legal`, the child's legal move count, and
`has_legal_move_after`.

## Deterministic fixtures — observed values

| Case | Parent FEN | Move | Observed `AfterState` |
| --- | --- | --- | --- |
| stalemate child | `7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55` | `c2c7` | `Terminal(Draw/Stalemate)` |
| stalemate child (2) | `8/8/k7/2p4p/7P/7K/5r2/8 b - - 1 59` | `f2g2` | `Terminal(Draw/Stalemate)` |
| extinction child | `k1n5/8/1K6/8/8/8/8/8 b - - 0 1` | `c8b6` | `Terminal(Loss/Extinction)` |
| extinction from a capture (mover wins) | `4R1k1/8/8/8/8/8/8/4K3 w - - 0 1` | `e8g8` | `Terminal(Loss/Extinction)`, child side to move is black |
| material draw from a capture | `8/8/8/2R5/4n3/8/7K/5k2 b - - 13 69` | `e4c5` | `Terminal(Draw/MaterialDraw)`, child legal count 5 |
| material draw (2) | `8/8/8/8/8/7k/8/2K3Rr b - - 19 74` | `h1g1` | `Terminal(Draw/MaterialDraw)` |
| mate outranks the clock | `7k/8/8/5N2/3K4/8/8/R6R w - - 99 60` | `a1g1` | `Terminal(Loss/NoMovesInCheck)`, child `rule50 = 100` |
| clock, child in check with an escape | `4k3/8/8/8/8/8/8/R3K3 w - - 99 60` | `a1a8` | `Terminal(Draw/FiftyMoveRule)`, child legal count 3 |
| clock, child not in check | `8/8/8/8/8/4k3/8/4K2q w - - 99 60` | `e1d2` | `Terminal(Draw/FiftyMoveRule)` |
| en-passant escape is the child's only legal move | `8/5kp1/1p5p/p1p4P/PP2RPPK/6r1/8/7n b - - 17 41` | `g7g5` | `Undecided`; `post.ep_square() == Some(G6)`, child legal `1` |
| child keeps castling rights | `r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1` | `a1a2` | `Undecided`; `castling_rights() == 0b1101`, `ep_square() == None` |
| same position, commoner move | same | `e1f1` | `Undecided`; `castling_rights() == 0b1100` |
| quiet promotion | `n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1` | `b7b8q` | `Undecided`; `b8` holds a white queen, `b7` empty, no casualties |
| double pawn push | start position | `e2e4` | `Undecided`; `ep_square() == Some(E3)`, `rule50() == 0` |
| self-explosion is illegal | `8/8/8/8/8/8/8/3qK3 w - - 0 1` | `e1d1` | `Illegal`; the `PostMove` still equals the `do_move` child |
| every pseudo-legal move illegal | `k3qr2/8/8/7q/1bb1R3/8/8/4K3 w - - 0 1` | all | `Illegal` (`legal` list is empty) |
| game-over parents | `8/8/8/8/8/8/8/R6K w - - 0 1`, `r6k/8/8/8/8/8/8/8 b - - 0 1` | all | `Illegal` |

Every row is asserted both against the documented value and against the cloned
child, so the table cannot drift from the engine.

## Measurement (`effect_bench`, added rows)

`cargo run --release --example effect_bench 9 2000`, ns per call, median of 9
batches × 2000 calls:

| Position | `child_outcome` | `is_legal` + `terminal_after` (2.6.0 shape) |
| --- | ---: | ---: |
| deep-win | 42 | 79 |
| middlegame-13 | 42 | 58 |
| middlegame-21 | 42 | 63 |

| Position | `post_move` | `clone+do_move` + read the same facts |
| --- | ---: | ---: |
| deep-win | 26 | 27 |
| middlegame-13 | 20 | 23 |
| middlegame-21 | 21 | 25 |

| Position | `outcome_after` (2.6.0) | `outcome_after` (this plan) | `clone+do_move+outcome` |
| --- | ---: | ---: | ---: |
| deep-win | 67 | 53 | 39 |
| middlegame-13 | 67 | 44 | 30 |
| middlegame-21 | 65 | 43 | 30 |

`effect.is_legal` is unchanged within noise (29–38 ns). `post_move` is at
clone parity; `child_outcome` is 26–47 % cheaper than asking the two questions
separately (one child derivation, one state population, one precedence pass),
and `outcome_after` inherits that because it is now the reduction of one
`child_outcome` call.

No hot-path code changed in this plan (`Board::legal`, `populate_state`, the
generator and the ladder are untouched), so no perft timing comparison was run;
perft totals are unchanged (see below).

## Perft

`cargo run --release --example verify_perft 5`: **41/41 passed**, 2.2 s.
`cargo run --release --example verify_perft 6`: **41/41 passed**, 64.4 s
(plan 1 baseline: 63–64 s; run-to-run noise).

## Deviations from the plan

1. **`child_outcome` is not literally `analyze`-then-`is_legal_from`-then-
   `terminal_from` on a *cloned* child** — it reuses the same `PostMove` value
   for both steps (the plan's snippet rebuilt it). Same result, one derivation.
2. **`PositionView` gained `PartialEq`/`Eq`** so that `PostMove` can derive
   them as the plan's API sketch shows. The view is crate-private; no public
   change.
3. **`effect_and_post` is the body of `effect`**, not a second implementation:
   `effect` = `effect_and_post(..).0`. The malformed-input arm (`is_capture`
   from the parent view, everything else empty) is preserved inside it, so
   `effect` is bit-for-bit the old behaviour and measured identical.
4. **`Board::child_outcome` lives on `board.rs` next to `Board::outcome_after`**
   and builds a throw-away context from the caller's `StateInfo`, exactly like
   the existing `Board::*_after` wrappers.
5. **`CHILD_CASES` landed** with 11 rows instead of the plan's minimum (“each
   `TerminalReason`, `Undecided`, `Illegal”): it also carries an extinction
   *capture* row (mover wins → child `Loss`), a promotion child, an
   ep-capture child and a “commoner moves into an attacked square” illegal row,
   because those are the rows a downstream is most likely to disagree about.
   The table is hand-reviewed against the fixtures above and machine-verified
   in `tests/conformance.rs`; it is **not** re-exported at the crate root
   (plan 3 may add `CHILD_CONFORMANCE_CASES` if a downstream asks for
   symmetry with `CONFORMANCE_CASES`).
6. **`tests/child_state.rs` samples `piece_on` on play-outs** (8 squares) and
   checks all 64 squares on the perft corpus and fixtures, as the plan allows.
7. `examples/effect_bench.rs` gained two rows (`child_outcome` vs the split
   pair, `post_move` vs clone-and-read) so the claim on the API docs is
   measured rather than asserted.

## Problems encountered

1. `do_move` does **not** populate the child `StateInfo` (only undo data), so
   the first draft of the fixture generator reported `child_legal_count: 0` for
   every row. The generator had to `populate_state` the child before
   `generate_legal_with_state` — the same mistake the crate documents on
   `Board::do_move`, and the reason the conformance test asserts the child
   count against a *populated* child.
2. `AfterState` is `#[non_exhaustive]`, so even the crate's own integration
   tests cannot `match` it exhaustively. The tests use the accessor helpers
   (`is_illegal`/`is_terminal`) and, to pin the documented mapping, the
   `terminal()`-derived reduction — which is exactly the downstream experience
   the enum is designed for.
3. A README draft used `Move::from_uci("e2e5")` as the “illegal” example; it is
   not pseudo-legal, so the effect path accepts it (`Undecided`) — the contract
   is pseudo-legal input. Replaced with the `8/8/8/8/8/8/8/3qK3` self-explosion.
4. `cargo doc` initially failed on three intra-doc links across modules
   (`AfterState::*` referenced from `board.rs`) and one redundant explicit link;
   fixed with explicit crate paths.
5. One wrong test expectation while writing `tests/child_state.rs` (assuming
   every move of the self-explosion fixture was undecided); caught immediately
   by the differential, removed.

## Unresolved parts

- `PostMove` deliberately exposes no `Debug`-only “is this child reachable?”
  bit; reachability is `child_outcome(..).is_illegal()`. A downstream that
  wants both in one pass currently calls `effect_and_post` **and**
  `child_outcome` (two derivations) — a combined
  `(MoveEffect, Option<PostMove>, AfterState)` query was not added because no
  consumer has asked and it would triple the hot-path return size.
- `CHILD_CASES` is not re-exported at the crate root (deviation 5).
- The stale-`StateInfo` contract is still covered only for the readers plan 1
  documented: `child_outcome` on a stale `StateInfo` changes the `game_over`
  flag and therefore the arm, exactly as `Board::legal` does; no dedicated test
  pins it (same gap as plan 1).

## Missing tests

- No differential for `child_outcome`/`post_move` on well-formed but
  non-pseudo-legal moves (a rook moving through a piece): the documented
  contract is pseudo-legal input only, and malformed input is covered.
- No test pins that a **stale** `StateInfo` yields the historical
  `game_over`-gated arm for `child_outcome` (see above).
- `PostMove::piece_on` on `Square::NONE` is exercised only through the view's
  own unit test, not through `PostMove`.

## Changed files

- `src/effects.rs` — `AfterState` (+ helpers), `child_outcome`, `terminal_from`,
  public `PostMove` (+ 11 accessors), `post_move`, `effect_and_post`,
  `outcome_after` reimplementation and corrected docs, module docs (“child
  summary”, `child_outcome` doctest), 4 unit tests.
- `src/position.rs` — `PositionView` derives `PartialEq`/`Eq` (needed by
  `PostMove`); no behaviour change.
- `src/board.rs` — `Board::child_outcome`; `Board::outcome_after` doc points at
  it.
- `src/lib.rs` — re-export `AfterState`, `PostMove`; crate docs (effect section,
  conformance section, allocation-free list); `send_sync_assertions` extended
  with `AfterState`, `MoveEffect`, `PostMove`.
- `src/conformance.rs` — `ChildCase`, `CHILD_CASES` (11 rows), module docs,
  branch-coverage unit test.
- `tests/conformance.rs` — `child_conformance_corpus_verifies`.
- `tests/child_state.rs` — **new**: arm differential, content differential,
  deterministic fixtures, malformed input, read-only checks.
- `examples/effect_bench.rs` — `child_outcome`/split-pair and
  `post_move`/clone rows.
- `README.md`, `CHANGELOG.md`.
- `docs/plans/feedback/report_handoff2_2.md` — this report.

## Verification run

```sh
cargo test                                       # 162 unit + 40 integration tests, all green
cargo clippy --all-targets -- -D warnings        # clean
cargo fmt --check                                # clean
cargo doc                                        # no warnings
cargo test --features unsafe-optimizations       # all green
cargo test --test perft_tests --test move_order_golden --test verify_moves \
           --test conformance --test effects --test terminal_after \
           --test child_state
cargo run --release --example verify_perft 5     # 41/41, 2.2 s
cargo run --release --example verify_perft 6     # 41/41, 64.4 s
cargo run --release --example effect_bench 9 2000
```

Integration suites: `child_state` 7, `conformance` 6, `effects` 10,
`interchange` 3, `move_order_golden` 1, `null_for` 2, `perft_tests` 4,
`terminal_after` 6, `verify_moves` 1, doctests 11.

## Next steps

1. Plan 3 (`plan_handoff2_3.md`): `MoveClassification` /
   `EffectContext::classification` (MG18) on top of `effect_and_post`, the
   measured decision about the effect path, and the 2.7.0 release with
   `cargo publish --dry-run`.
2. If a downstream asks for one call that answers “effect + summary + arm”, add
   it as a third query (`analyze_and_classify`) rather than growing
   `MoveEffect`; the measurement above (child derivation ≈ 20–26 ns) bounds what
   such a call can save.
3. Re-export `CHILD_CASES` at the crate root if the downstream protocol wants
   the same import shape as `CONFORMANCE_CASES`.
