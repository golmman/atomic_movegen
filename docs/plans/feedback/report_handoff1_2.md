# Report — Plan 2: Effect context and per-move effects (MG3, MG5)

## Status

All plan items are implemented. Nothing changed in move generation order, perft
values, hashes, FEN output, or `Outcome` semantics: the effect path is additive
and the existing board-writing path remains the oracle.

| Item | Status | Notes |
| --- | --- | --- |
| MG3 `EffectContext` + `MoveEffect` | Done | Node-constant context, one read-only classification per pseudo-legal move. |
| MG3 `EffectContext::is_legal` | Done | Exactly `Board::legal` for pseudo-legal input; castling delegates to `Board::legal`. |
| MG3 `outcome_after` (option B) | Done | `EffectContext::outcome_after` and `Board::outcome_after`; documented cloned-board fallback. |
| MG5 node-constant attack data | Done (merged) | The context snapshot is the node-constant data; no separate MG5 API. |
| MG2 legal early-exit stream | Done | `movegen::for_each_legal` / `for_each_legal_with_state`. |
| Differential gates | Done | 15,025 positions / 285,193 pseudo-legal moves, plus 23,560 decided child outcomes. |
| Performance measurement | Done | The effect path is **not** faster than the board-write path today; numbers below. |

## Public API as implemented

```rust
// atomic_movegen::effects (re-exported at the crate root)
#[derive(Debug, Clone, Copy)]
pub struct EffectContext { /* private snapshot */ }

impl EffectContext {
    pub fn new(board: &Board, state: &StateInfo) -> Self;
    pub fn side_to_move(&self) -> Color;
    pub fn effect(&self, board: &Board, m: Move) -> MoveEffect;
    pub fn is_legal(&self, board: &Board, m: Move) -> bool;
    pub fn outcome_after(&self, board: &Board, m: Move) -> Option<Outcome>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveEffect {
    pub is_capture: bool,
    pub casualties: BlastCasualties,
    pub checks_enemy: bool,
    pub explodes_enemy_commoner: bool,
    pub leaves_own_commoner: bool,
    pub is_legal: bool,
}

// Board
pub fn Board::effect_context(&self) -> EffectContext;
pub fn Board::effect_context_with_state(&self, state: &StateInfo) -> EffectContext;
pub fn Board::outcome_after(&self, m: Move, state: &StateInfo) -> Option<Outcome>;

// movegen
pub fn for_each_legal<B>(board: &Board, f: &mut impl FnMut(Move) -> ControlFlow<B>)
    -> ControlFlow<B>;
pub fn for_each_legal_with_state<B>(board: &Board, state: &StateInfo,
    f: &mut impl FnMut(Move) -> ControlFlow<B>) -> ControlFlow<B>;
```

Crate-root re-exports now include `EffectContext` and `MoveEffect`.

### Deviations from the plan's proposed API

1. **`EffectContext` stores no `checkers`/`pinned` fields.** The effect path
   derives post-move attacks directly from the child bitboards, so no query
   reads pins, and both fields would be dead weight (and a `dead_code`
   warning). They remain available through `StateInfo`. The stored snapshot is
   the piece placement, the side to move, the commoner bitboard, a `game_over`
   flag, and a copy of the populated `StateInfo` used only by the castling
   fallback.
2. **No private `attackers_to_sets` helper in `effects.rs`.** Instead the
   "is this square attacked?" rule became a single crate-private helper,
   `board::is_attacked_by(sq, occupied, by_type, attackers, sq_color)`, which
   `Board::legal`'s existing `square_attacked_by` now also calls. One
   implementation of the rule, used by the oracle and the effect path.
   `Board::attackers_to` was left alone: it returns the attacker *set* of both
   colors rather than a boolean, and refactoring it would have widened the diff
   for no gain.
3. **`EffectContext::side_to_move()` was added** (one line) so a consumer
   holding a context does not need the board for that query.
4. **`generate_legal_with_state` was deliberately not refactored** onto the
   stream: it still collects pseudo-legal moves and filters them with the
   trivial fast path and `Board::legal`. The plan allowed this refactor, but
   the measurement below shows the effect path is slower than `Board::legal`,
   so switching the perft hot path would have been a regression, and
   `generate_legal_with_state` also filters pre-existing entries when called
   with a non-empty `MoveList` — behaviour a stream refactor would have
   changed.

## Implementation notes

`EffectContext::effect` and `is_legal` share one analysis (`analyze`) that
derives the child placement without a board write: mask the context bitboards
by `BlastCasualties::all()`, then apply the move type (castling relocates the
commoner and rook, quiet moves and quiet promotions relocate one piece, a
capture leaves nothing behind). Two rule facts that the differential forced:

- **The capturer's origin square is not in `blast_casualties`.** Casualties
  record the destruction at ground zero (`to`), so a capture must also clear
  `from` from the post-move sets. Missing this made en passant and every
  "the capturer was the only blocker" case wrongly legal.
- **A quiet move changes occupancy squares, not just the piece list.** Clearing
  only the per-type sets left `from` occupied, so a quiet move that uncovers a
  slider attack on the mover's own commoner was wrongly accepted
  (`8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1`, `b5b6`).

`is_legal` reproduces the `Board::legal` ladder branch by branch: game over
when either side has no commoner, no commoner count change (self-explosion),
an exploded enemy commoner is an immediate yes, touching commoners are immune,
otherwise no attacker on the mover's commoner square.

`outcome_after` decides what the effect can decide and falls back otherwise:

| Case | Decided by the effect | Why |
| --- | --- | --- |
| Enemy commoner exploded | `Some(Loss)` | Blast casualties; the child has no commoner. |
| Commoner-only material draw | `Some(Draw)` | Child piece count from the casualties; exactly `is_material_draw`. |
| 50-move rule, child not in check | `Some(Draw)` | `rule50_after` + `checks_enemy` (mate outranks the clock). |
| Checkmate / stalemate (no child legal move) | **fallback** | “Does the child have a legal move?” still needs a board write. |
| 50-move rule with the child in check | **fallback** | Mate outranks the clock; needs the child's move existence. |
| Any other live position | **fallback** | Same reason: the child's outcome is `None` only if a move exists. |

The fallback is `board.clone(); do_move; outcome()`. It is documented as an
implementation detail in `EffectContext::outcome_after`.

`for_each_legal_with_state` walks `for_each_pseudo_legal` and accepts a
candidate when `TrivialLegality::is_trivially_legal` (quiet fast path, checked
first) or `EffectContext::is_legal` accepts it; `Break` propagates.

## Differential results

`cargo test --test effects` (all green). Every pseudo-legal move of every
visited position is compared with:

- `is_capture` vs `Board::is_capture`,
- `casualties` vs `Board::blast_casualties`,
- `is_legal` vs `Board::legal` **and** vs membership in
  `generate_legal_with_state`,
- `leaves_own_commoner` / `explodes_enemy_commoner` / `checks_enemy` vs a
  cloned `do_move` child (`commoners`, `checkers`),
- `outcome_after` vs cloned `do_move` + `outcome` for every legal move, and
  `None` for every pseudo-legal-but-illegal move,
- the `for_each_legal` stream vs the legal list, element for element.

Visited positions per gate:

| Gate | Positions | Moves / children |
| --- | --- | --- |
| `effect_is_legal_matches_board_legal` | 15,025 | 285,193 pseudo-legal moves |
| `effect_is_legal_never_accepts_illegal` | 15,025 | acceptance ⊆ and ⊇ the legal list |
| `effect_fields_match_child` | 15,025 | 285,193 candidates vs a cloned child |
| `outcome_after_matches_do_move` | 15,025 | 23,560 decided children |
| `for_each_legal_matches_generate_legal` | 15,025 | stream == list (order included) |

Position sources: the 41-position perft corpus (`tests/perft_values.md`), 24
deterministic fixtures (quiet move, capture, en passant, promotions with and
without capture, both castlings, exploding the enemy commoner, self-explosion,
a blasted blocker that uncovers an attack on the mover's commoner, "the
capturer was the blocker", a quiet check, a blast that discovers a check,
touching-commoner immunity, a single check escape, mate, stalemate,
extinction, the commoner-only draw, the clock draw, mate outranking the clock,
the `deep-win` handoff position), long random legal play-outs (6 starts ×
16 seeds × 300 plies) and short play-outs from every corpus position (41 × 2
seeds × 60 plies).

## Verification

| Gate | Result |
| --- | --- |
| `cargo test` | Pass: 124 unit (6 new), 10 `tests/effects.rs`, 1 move-order, 2 null-for, 4 perft, 1 verify-moves, 2 doctests. |
| `cargo test --features unsafe-optimizations` | Pass (same suite). |
| `cargo clippy --all-targets -- -D warnings` | Pass. |
| `cargo fmt --check` | Pass. |
| `cargo doc --no-deps` | Pass, no warnings (no private intra-doc links). |
| `cargo run --release --example verify_perft` | **41/41 passed at depths 1–6**, total 71.2 s. |
| `cargo run --release --example verify_perft 5` | 41/41, 2.4 s. |
| `cargo test --test move_order_golden` | Pass, unchanged. |
| `cargo test --test verify_moves` | Pass, unchanged. |
| `cargo test --test perft_tests` | Pass (depths 1–4 on the 14 shipped positions). |

## Measured performance

Harness: `cargo run --release --example effect_bench` (new, zero-dependency;
median of 9 batches × 2000 calls, three separate runs, numbers stable to a few
ns). Positions: the handoff's `deep-win` FEN and two perft-corpus middlegames.
All values are ns per call.

| Position | `effect().is_legal` | `effect()` (all fields) | `Board::legal` |
| --- | ---: | ---: | ---: |
| deep-win (32 moves) | 25–28 | 27–31 | 8 |
| middlegame-13 (34) | 24–33 | 26–36 | 7–10 |
| middlegame-21 (30) | 25–28 | 28–32 | 7–8 |

| Position | `outcome_after` | clone + `do_move` + `outcome` |
| --- | ---: | ---: |
| deep-win | 56–62 | 32–35 |
| middlegame-13 | 54–68 | 30–39 |
| middlegame-21 | 56–57 | 31–33 |

| Position | `generate_legal_with_state` | `for_each_legal` (all) | first legal only | `has_legal_move_with_state` |
| --- | ---: | ---: | ---: | ---: |
| deep-win | 96–108 | 122–133 | 4 | 6 |
| middlegame-13 | 103–131 | 145–169 | 4 | 6 |
| middlegame-21 | 129–146 | 212–225 | 4 | 6 |

**The effect path is not faster than the board-write path.** `Board::legal`
costs ~8 ns per move because it touches only the occupancy and exits at the
first attacked-square hit, and the generator already skips it for most moves
via `TrivialLegality` (`generate_legal` is ~100 ns for 30 moves, i.e. ~3 ns per
move). The effect path derives a whole child placement (`occupied`, 2 colors, 6
types) and, for `effect()`, also runs the check query; measurements of the
parts alone put the masks at ~7 ns and each attack query at ~4 ns. This is an
API result, not a speed result:

- `outcome_after` only wins where the effect actually decides (extinction,
  commoner-only draw, non-check clock); where the mate/stalemate question
  remains, the analysis is paid *on top of* the clone fallback.
- `for_each_legal` with early exit is a genuine win for "find one legal move"
  (4 ns vs ~100–140 ns for a full list), though the dedicated
  `has_legal_move_with_state` is still ~6 ns.
- The effect path was therefore **not** wired into `generate_legal_with_state`
  or perft.

## Changed files

- `src/effects.rs` — new module: `EffectContext`, `MoveEffect`, the private
  `PostMove` view, the ladder, six unit tests, module docs with a doctest.
- `src/board.rs` — `is_attacked_by` helper (now shared with
  `square_attacked_by`/`Board::legal`), `castling_squares` made
  `pub(crate)`, `Board::effect_context`, `effect_context_with_state`,
  `outcome_after`.
- `src/movegen.rs` — `for_each_legal`, `for_each_legal_with_state`, docs.
- `src/lib.rs` — `pub mod effects`, crate-root re-exports, a "Move effects"
  section in the crate docs.
- `tests/effects.rs` — new differential integration test (10 tests).
- `examples/effect_bench.rs` — new microbenchmark.
- `README.md` — feature bullet, effect-API usage example, bench command.
- `CHANGELOG.md` — Unreleased additions and the shared-attack-helper note.

## Problems encountered

- The first version of the post-move view was wrong in two independent ways
  that the differential caught within minutes (capturer origin not cleared;
  occupancy not relocated on quiet moves). Both are now pinned by fixtures in
  `tests/effects.rs` so a regression fails with a named case.
- `7k/8/6K1/8/8/8/8/R7 w - - 99 60` was intended as a mate-in-one; it is not a
  mate because the black commoner can step next to the white one and become
  immune to the rook. The fixture was rebuilt with two rooks and a knight
  (`7k/8/8/5N2/3K4/8/8/R6R w - - 99 60`) and verified to be mate with
  `rule50` crossing 100.
- `Move::from_uci` is a syntax-only parse, so castling and en passant compare
  unequal to the generator's encoding, and `Board::move_from_uci` returns
  `None` for illegal moves. The tests resolve legal moves through the board and
  fall back to a from/to lookup over the pseudo-legal list.
- The plan's `MoveEffect.casualties` shape assumption ("a same-square pair") is
  only true for plain captures; a blasted blocker adds third and fourth
  squares. The fixture cases assert `casualties == board.blast_casualties(m)`
  instead of a shape.
- `#[inline]` on the private analysis helpers made no measurable difference
  (27 ns before and after), so it was removed to keep the source plain.

## Unresolved parts / missing tests

- **No external veto.** The downstream's independent rules implementation (the
  handoff's adoption protocol step 3) is not available here, so the only rules
  oracle used is this crate's own `Board::legal` / `do_move` child. That is
  the strongest in-repo differential, but it cannot catch a rule the crate
  implements wrongly in both paths.
- **No child-view generator.** `outcome_after` still needs a board write for
  the mate/stalemate and in-check-clock decisions. Removing that fallback is a
  separate, measured plan.
- **Stale `StateInfo`.** The "populated state" precondition is documented and
  mirrored from `Board::legal`; passing a stale state is untested (it is also
  untested for `Board::legal`).
- **No allocation assertion.** The effect path allocates nothing by
  construction (no `Vec`, no `String`, no collected list), but there is no
  test that would catch a future hidden allocation.
- **The effect path is slower than `Board::legal`.** This is reported, not
  fixed; see next steps.
- Benchmarks are a plain example, not a statistical benchmarking harness (the
  crate is zero-dependency, so no criterion).

## Next steps

1. **Plan 3 (MG4/MG6) should start from these numbers.** The cheapest win is a
   legality-only derivation inside `EffectContext::is_legal`: post-move
   occupancy plus the two commoner bitboards only (~7 ns of masks), reusing the
   same ladder and the same differential gates, instead of the full nine-set
   `PostMove`. That is the only way `is_legal` can approach `Board::legal`'s
   8 ns.
2. **Do not adopt the effect path into move generation** until a measurement
   shows it is faster than `TrivialLegality::is_trivially_legal` +
   `Board::legal`. Widening `TrivialLegality` (MG4) is the better lever: the
   generator already runs at ~3 ns per move.
3. **Plan 4 (SAN) can use `MoveEffect`** for check/mate suffixes: `checks_enemy`
   plus `outcome_after` is exactly the SAN check/mate classifier, and
   `casualties` gives the capture and en-passant data.
4. Consider exposing the context's snapshot (`occupied`, per-color/per-type
   bitboards) if MG5's consumers actually need node-constant attack masks; it
   was left out per R3's "rules only" bound.
