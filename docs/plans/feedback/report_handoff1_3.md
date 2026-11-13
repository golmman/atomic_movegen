# Report — Plan 3: Performance follow-ups (MG4, MG6, MG7)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| MG4 widened trivial-legality ladder | Done | New crate-private ladder decides quiet moves, captures, explosions, self-explosions, en-passant blasts, and quiet commoner relocations; `Board::legal` stays the oracle for the rest. |
| MG4 ladder-equivalence + differential tests | Done | Reference ladder, decision differential against `Board::legal`, seeded play-outs, rule coverage, and two explicit "blast opens a ray" regressions. |
| MG6 `generate_pseudo_legal` micro-optimisation | Partly done | One measured, order-preserving change (castling early-out, ~1.6 % of corpus instructions). Two other candidates were measured and rejected; the profile shows no single dominant cost in the generator. |
| MG7 FEN performance fallback | Not implemented (as planned) | Documented below: the allocations that remain, why the null-replay round-trip is gone, and the condition that would justify a future plan. |
| Performance measurement | Done | 41-position corpus, depth 5: **10.6 % fewer instructions, 5.6 % less wall time**; 40 of 41 positions improve. |
| Correctness gates | Done | Full suite, perft 41/41 at depths 1–6, order golden, `verify_moves`, both feature settings. |

Nothing changed in move order, perft values, Zobrist keys, FEN output, or
`Outcome`. No public API was added or removed; `FastLegality` /
`WidenedLegality` are crate-private. The switch of `for_each_legal*` from the
effect path to the ladder is a behaviour-preserving internal change (same
visited sequence, verified by tests).

## Deviation from the plan (decided by measurement)

The plan prescribed implementing MG4 as

```text
if trivial_quiet.is_trivially_legal(board, m) { accept }
else if effect.is_legal(board, m) { accept }
else { reject }
```

i.e. widening the ladder with `EffectContext::is_legal` from plan 2. The plan
also required measuring "both `TrivialLegality`-first and effect-only variants;
keep the faster one only if the differential is green". The effect path is not
the faster one — not even close:

| Variant (41 positions, perft depth 5, `instructions:u`) | Instructions | vs. baseline |
| --- | ---: | ---: |
| baseline: `trivial \|\| Board::legal` | 42,821,098,903 | 1.000 |
| `trivial \|\| EffectContext::is_legal` (plan's variant) | 55,703,541,172 | **0.769** (23 % slower) |
| kept: `trivial \|\| widened ladder \|\| Board::legal` | 38,260,668,653 | **1.119** |

The effect path derives a whole child placement (occupancy, two colours, six
type sets) and re-uses none of the node-constant data that the generator already
holds, so it costs ~25 ns per move against `Board::legal`'s ~8 ns (plan 2
measured the same). The widened ladder therefore adds *cheap* node-constant
rules and keeps `Board::legal` as the only fallback. This satisfies the plan's
decision 2 ("Keep `Board::legal` as the oracle") and decision 3 (measure both,
keep the faster); it deviates from decision 1 (`EffectContext::is_legal` as the
one new decision procedure), which the measurement makes untenable.

## MG4 as implemented

### The ladder

`FastLegality` (crate-private, `src/board.rs`) is built once per node:

```rust
if !playable { false }                  // terminal position
else if in_check { board.legal(...) }   // every move needs the full check
else { trivial.is_trivially_legal(...) || widened.decide(...) || board.legal(...) }
```

`WidenedLegality` holds only node-constant data (own/enemy commoners, the
non-pawn occupancy) plus one lazily computed item (the aligned enemy sliders and
their blockers, `OnceCell`), so nodes whose candidates are all trivially legal
pay nothing for it.

The rules, each a *sufficient* condition with a stated reason (see the module
docs for the proofs):

1. **Self-explosion / count change** — the move empties an own commoner:
   `Some(false)` when every own commoner is emptied, otherwise undecided
   (multi-commoner boards, unreachable in legal play).
2. **Enemy extinction** — a capture whose blast removes *every* enemy commoner
   ends the game, so it is `Some(true)` even if the mover's commoner is exposed.
   Same branch order as `Board::legal` (own count first, then enemy extinction,
   then the attack query).
3. **No blocker removed** — if the move empties no square between the mover's
   commoner and an aligned enemy slider (of either colour), no slider ray can
   open, so a commoner that is currently safe stays safe: `Some(true)`.
4. **Ray refinement** — when a blocker *is* emptied, the per-sniper check
   `between(ksq, sniper) ∩ occupied \ emptied == ∅` decides whether that ray
   really opens. This is what makes multi-blocker blasts (en passant emptying
   the victim *and* the capturer on one rank, a blast removing an enemy blocker
   of its own slider) correct instead of optimistically accepted.
5. **Quiet commoner relocation** — a capturing commoner self-destructs
   (`Some(false)`); otherwise touching the enemy commoner is immunity
   (`Some(true)`), and the destination's attack status is one shared
   `is_attacked_by` query on the post-move occupancy. The arrival on the
   destination cannot block an attack on that square, so only the origin's
   emptying matters — the query is exact for quiet commoner moves.

The *emptied* set is `{from}` for quiet moves and promotions, and
`{from, to} ∪ (non-pawns adjacent to to) ∪ {en passant victim}` for captures
(exactly `blast_casualties().all()` plus the capturer's origin). Castling keeps
`Board::legal` (pass-through-square attacks are a separate rule surface), as
does anything in check and every pinned-origin move.

### Hit rates (perft tree, depth 4)

| Position | pseudo-legal | trivial accept | widened accept | fallback to `Board::legal` |
| --- | ---: | ---: | ---: | ---: |
| middlegame-13 (`#13`) | 1,848,109 | 1,578,938 | 255,590 | 13,581 |
| middlegame-21 (`#21`) | 830,731 | 726,859 | 101,720 | 2,152 |
| startpos | 207,101 | 202,249 | 4,379 | 473 |

Before the change every non-trivial candidate called `Board::legal`; now ~93 %
of them are decided by the ladder. Profiling confirms the shift: `Board::legal`
fell from 30 % of a perft run (plan 2's number) to **4.5 %** on the deep-win
position.

### Tests (all new, `src/board.rs` + `src/movegen.rs`)

- `widened_legality_matches_reference_and_board_legal` — for the
  `TRIVIAL_LEGALITY_CORPUS`, the new `WIDENED_LEGALITY_CORPUS`, and 24 seeded
  play-outs (6 starts × 4 seeds × 120 plies, > 1000 positions), every
  pseudo-legal move is checked three ways: (a) `WidenedLegality::decide` equals
  the verbatim reference ladder, (b) every decision equals `Board::legal`,
  (c) `FastLegality::is_legal` equals membership in `generate_legal_with_state`.
  The reference is written against the public board API with deliberately
  different building blocks (`blast_casualties`, `attackers_to_color`, a
  square-scan sniper search) so a mistake in either side finds the other.
- `widened_legality_blast_that_opens_a_ray_is_undecided` — the two bugs the
  differential found during development are pinned as named cases:
  `r6k/8/8/8/nb5R/8/8/K7 w - - 0 1` (`h4b4` removes an *enemy* blocker of an
  enemy slider) and `8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1` (`e4d3` empties two
  blockers on the h4 queen's rank). Both must fall back and are illegal.
- `widened_legality_corpus_covers_every_rule` — non-vacuity guard: asserts
  non-zero counts for accepted quiet moves, accepted captures, accepted
  enemy-commoner explosions, accepted commoner moves, accepted *checking*
  moves, rejected self-explosions, rejected commoner captures, undecided
  pinned-origin moves, undecided castling, undecided en passant, undecided
  quiet moves, positions in check, and positions without a commoner.
- `fast_legality_handles_terminal_positions` — `FastLegality` rejects every
  candidate when either side has no commoner, even when called without the
  generator's early return.
- `legal_stream_matches_legal_list` (`src/movegen.rs`) — `for_each_legal` and
  `has_legal_move` agree with `generate_legal` on `STREAM_CORPUS` (the plan's
  "`generate_legal_order_unchanged`"; the order golden test and
  `tests/effects.rs::for_each_legal_matches_generate_legal` remain as the hard
  gates).

## MG6 as implemented

### Profile (release + debug info, `perf record -e cpu-clock`, deep-win, perft 5)

| Symbol / source line | Share |
| --- | ---: |
| `perft` (inlined generator + make/unmake) | 92.8 % |
| `Board::legal` | 4.5 % |
| `OnceCell<WidenedLegality>::try_init` | 1.5 % |
| `__memset_avx2` (`StateInfo::new` per node) | 1.2 % |
| `types.rs:978` (`MoveList::push`) | 7.1 % |
| `types.rs:755` (`Move::make_move`) | 6.8 % |
| `board.rs:2953` (trivial-legality test) | 5.5 % |
| `movegen.rs:305` (compaction loop) | 4.6 % |

`generate_pseudo_legal` no longer exists as a separate symbol: it is folded into
`perft`, and its cost is spread over `MoveList::push`, `Move::make_move`, and
bitboard operators. There is no single dominant cost left to attack, which is
the plan's "if the profile shows no clear win, record that".

### Changes measured

| Candidate | Corpus depth 5 instructions | Verdict |
| --- | ---: | --- |
| `#[cold]` on the widened path | 40,622,606,108 (+6 % vs kept) | rejected |
| `#[inline(never)]` on the widened path | 38,902,576,320 (~equal) | rejected (no gain) |
| hoist pawn-loop constants/arguments out of `generate_pawn_moves_for` | 38,902,578,176 (+1.7 %) | rejected (register pressure) |
| **skip the castling path when the side to move has no rights** | **38,260,668,653 (kept)** | **kept, −1.6 %** |
| node-constant enemy attack map for the commoner rule | 39,597,645,341 (+3.4 %) | rejected (per-move query is cheaper overall) |

The kept change is in `for_each_pseudo_legal`: one rights test per node instead
of walking both castling branches. Order is untouched (castling is still the
last stage, and the golden test passes).

The plan's candidate 1 (hoist repeated `pieces_color_pt`) needs no change —
every type set is read exactly once per stage. Candidate 2 (`occupied`,
`them_bb`, `ep_square`) was already hoisted. Candidate 4 (blanket `#[inline]`)
was measured above and rejected.

## MG7 — not implemented (as planned)

- FEN allocations that remain in `Board::from_fen`: `parts: Vec<&str>`
  (`src/board.rs:484`) and `rows: Vec<&str>` (`src/board.rs:498`) — two small
  heap allocations per parse.
- The measured null-replay round-trip is gone: `Board::null_for()` (plan 1)
  builds the null counterpart without parsing or formatting, and its
  equivalence to the FEN round-trip is pinned by `tests/null_for.rs`.
- `from_fen` remains the parser of record for interchange and fixtures
  (`tests/*`, examples), where one parse per fixture is irrelevant.
- A future allocation-reduction plan would only be justified if a remaining
  consumer benchmark shows `Board::from_fen` as a material symbol. The profile
  above shows no FEN code in the hot path at all (it does not appear in the
  perft profile), so that condition is not met.

## Measured performance

Host: AMD Ryzen 9 5950X, rustc 1.99.1 (58cf8f9c6 2026-10-13), Linux.
Kernel counters: `perf stat -e instructions:u` (deterministic per binary, so
instruction counts are reported as the primary metric; wall time as the
secondary). `perf record` with hardware events is unavailable
(`perf_event_paranoid=2`), so profiling used `perf record -e cpu-clock`.
Release profile as in `Cargo.toml` (`lto = "fat"`, `codegen-units = 1`,
`opt-level = 3`). Control and candidate were run alternately, one process at a
time; medians reported.

### Whole corpus (41 positions of `tests/perft_values.md`)

| Metric | Depth 4 (13,495,921 nodes) | Depth 5 (399,109,774 nodes) |
| --- | ---: | ---: |
| baseline instructions | 1,454,940,318 | 42,821,098,903 |
| candidate instructions | 1,289,040,024 | 38,260,668,653 |
| instructions ratio | **1.129×** | **1.119×** |
| baseline wall (median of 5) | — | 2,336 ms |
| candidate wall (median of 5) | — | 2,205 ms |
| wall ratio | — | **1.059×** |

Per-position (depth 4, instruction ratio base/candidate): 40 of 41 positions
improve (1.028× … 1.246×). The single regression is the start position
(`#1`, 0.954×): its tree contains almost no non-trivial candidates, so the
extra branch to the widened path is pure overhead there.

### Selected positions (perft wall time, median of 3)

| Position | Nodes | Baseline | Candidate | Ratio |
| --- | ---: | ---: | ---: | ---: |
| startpos d6 | 118,926,425 | 552 ms | 570 ms | 0.968× |
| middlegame-13 d5 | 53,833,718 | 319 ms | 308 ms | 1.036× |
| middlegame-21 d5 | 22,933,256 | 138 ms | 128 ms | 1.078× |
| deep-win d5 | 41,091,330 | 202 ms | 200 ms | 1.010× |
| promotion-heavy d6 | 35,364,502 | 323 ms | 264 ms | 1.223× |

### Isolated move generation (one position, 100 k iterations, instructions)

Ratio candidate/baseline for `generate_pseudo_legal` + `generate_legal`
(1.000 = unchanged; lower is better):

| startpos | deep-win | middlegame-13 | middlegame-21 | kiwipete | promotion-heavy | castling |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1.051 | 0.985 | 0.979 | 0.890 | 0.921 | 0.750 | 0.920 |

### Stream API (`for_each_legal`, `effect_bench`, ns per node)

`for_each_legal` now uses the ladder instead of the effect path:

| Position | before | after |
| --- | ---: | ---: |
| deep-win | 125 | 72 |
| middlegame-13 | 144 | 75 |
| middlegame-21 | 221 | 90 |

`generate_legal` in the same microbenchmark is ~107/111/134 ns, i.e. the
allocation-free stream is now also the cheapest way to collect/count all legal
moves (~35 % cheaper than building the list).

`cargo run --release --example verify_perft` (depths 1–6, 41 positions) went
from 71.2 s (plan 2 report) to 65.8 s in this run.

## Verification

| Gate | Result |
| --- | --- |
| `cargo test` | Pass: 129 unit (5 new), 10 `tests/effects.rs`, 1 move-order, 2 null-for, 4 perft, 1 verify-moves, 2 doctests. |
| `cargo test --release` | Pass (same suite). |
| `cargo test --features unsafe-optimizations` | Pass (same suite; no magic/attack code was touched). |
| `cargo clippy --all-targets -- -D warnings` | Pass. |
| `cargo fmt --check` | Pass. |
| `cargo doc --no-deps` | Pass, no warnings. |
| `cargo run --release --example verify_perft` | **41/41 passed at depths 1–6**, 65.8 s. |
| `cargo run --release --example verify_perft 5` | 41/41, 2.27 s. |
| `cargo test --test move_order_golden` | Pass, unchanged. |
| `cargo test --test verify_moves` | Pass, unchanged. |
| `cargo test --test perft_tests` | Pass. |
| `cargo test --test effects` | Pass (effect API unchanged and still differentially tested). |

## Changed files

- `src/board.rs` — `WidenedLegality`, `compute_sniper_data`, `FastLegality`
  (new, crate-private), module/rule docs with the proofs, and the new tests
  listed above. `TrivialLegality`, `Board::legal`, and `is_attacked_by` are
  unchanged.
- `src/movegen.rs` — `generate_legal_with_state`, `for_each_legal_with_state`,
  and `has_legal_move_with_state` now share `FastLegality`; the castling
  early-out in `for_each_pseudo_legal` (MG6); docs updated; new
  `legal_stream_matches_legal_list` test.
- `README.md` — feature bullets for the fast-legality ladder and for the stream
  API sharing it.
- `CHANGELOG.md` — Unreleased "Changed" entries (performance, stream path,
  castling early-out) with the measuring note that no observable behaviour
  changed.
- `docs/plans/feedback/report_handoff1_3.md` — this report.

Nothing else: `src/effects.rs` was not touched (the measured effect-path change
was rejected), and no new dependency, feature flag, or `unsafe` was added.

## Problems encountered

1. **The first ladder accepted an illegal en-passant capture.** The initial
   rule accepted a move when it emptied no *sole* blocker. `e4d3` in
   `8/8/8/8/k2Pp2Q/8/8/3K4 b - d3 0 1` empties *two* blockers (the d4 victim and
   the e4 capturer) on the h4 queen's rank-4 ray to the a4 commoner, so the
   "sole blocker" set did not contain them and the capture was accepted while
   `Board::legal` rejects it. The differential play-out found it within seconds.
   The fix is the per-sniper refinement (`rays_stay_closed`) plus keeping the
   union of blockers (`sensitive`) as the cheap test.
2. **The first version of the commoner rule ignored enemy pieces blocking their
   own sliders.** `r6k/8/8/8/nb5R/8/8/K7 w - - 0 1` (`h4b4`) blasts the a4
   knight, which alone blocks the a8 rook's ray to the a1 commoner. The
   blocker set now includes both colours, and `state.pinned` (own pieces only)
   is deliberately not reused as the blocker set.
3. **Multi-commoner boards.** The rules reason about one commoner square while
   `Board::legal` uses the lowest commoner of the *post-move* set, so every
   branch that would differ (a commoner move or capture on a board with more
   than one commoner, a partially lost commoner set) returns "undecided" and
   falls back. This is unreachable in legal play but keeps the ladder total.
4. **The effect path is a dead end for move generation** (see the deviation
   section). Recorded, not fixed: the effect API remains public and correct, it
   is simply not the cheapest way to decide legality.
5. **`perf record` with hardware events is blocked in this environment**
   (`perf_event_paranoid=2`), so the profile used `-e cpu-clock` and the
   quantitative comparison used `perf stat -e instructions:u`. Instruction
   counts fluctuate by ~2–3 % between builds/runs from process startup and code
   layout, which is why every variant comparison was run in the same session
   against the same baseline binary.
6. **Wall-clock micro-benchmarks are noisy here** (turbo and shared host), so
   the report leads with instruction counts and states wall time only as
   medians of interleaved runs.

## Unresolved parts / missing tests

- **The start position regresses** by 2.4 % (depth 6) to 4.6 % (depth 4)
  instructions. The cause is structural: the compaction loop now contains a
  branch to a function that the compiler does not inline, and an all-trivial
  node pays that branch per candidate. `#[cold]` and `#[inline(never)]` were
  measured (both make the corpus worse) and reverted. A node-level
  "all candidates are trivial" pre-test would need its own attack computation
  and is not obviously cheaper; it is left for a future profile-led plan.
- **No external veto.** The only rules oracle is this crate's `Board::legal`
  and the perft corpus; the handoff's independent downstream implementation is
  not available here.
- **No allocation assertion** that the ladder stays allocation-free (it does by
  construction: `Bitboard`/`Cell` only).
- **Multi-commoner boards are untested** beyond falling back (no fixture
  constructs one; `Board::from_fen` would accept such a FEN, but the rules of
  the crate treat them as terminal/illegal).
- **The blocked/sniper data is recomputed per node** even when
  `populate_state` already walks the same snipers for `state.pinned`
  (`compute_pinned`). Sharing that walk would need either a new public
  `StateInfo` field (a semver question for plan 5's contract) or a crate-private
  channel; the measured cost is small (the `snipers` `OnceCell` does not show up
  in the profile separately), so it was not done here.
- **No coverage of `unsafe-optimizations`-specific code paths** (none changed).

## Next steps

1. **Plan 4 (SAN + piece-set setup)** can build on `MoveEffect` for
   check/mate suffixes as planned; nothing in this plan changes the effect API.
2. A follow-up micro-plan could try to remove the start-position overhead with
   a node-level "no non-trivial candidate" pre-test (needs an attack-set
   computation and a measurement; likely a wash).
3. If a future consumer profiles `Board::from_fen` as a hot symbol, take MG7 as
   a separate allocation-reduction plan (`parts`/`rows` `Vec` → fixed arrays).
4. Consider documenting the fast-legality contract (which move kinds are
   decided below the oracle) in plan 5's public contract section, since
   `for_each_legal`/`has_legal_move` behaviour now depends on it only through
   the equivalence tests.
