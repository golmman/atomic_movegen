# Implementation report — handoff 3 plan 2 (MG20: the specialised existential test)

Status: **implemented, cost gates missed**. The specialised, early-exit-by-
construction existential test is in the tree, wired into the child path, and
differentially exact against the legal list on every view the suites visit. The
existence share of a child query fell from **18.1 ns to 10.2 ns** (−44 %) and
`has_legal_move_after` from **32.5 ns to 24.3 ns** per child, but the plan's
ratio bars (≤ 0.40 × the build-only row for the existence question, ≤ 0.45 ×
for terminality) are **not** met: measured 0.73 × and 0.94 ×. The measurement,
the class it fails on and the mechanism are in "Benchmarks" and "Deviations";
the verdict belongs to plan 3.

No public API item was added (the plan's crate-private API only), no move order,
perft value, hash, FEN output or `Outcome`/`Terminal` semantics changed, and no
dependency or `unsafe` was added. Version in the tree: `2.7.0`.

## Item status

| Item | Status | Where |
| --- | --- | --- |
| MG20 positive screen (`quiet_legal_witness`, `quiet_legal_witness_unpopulated`) | **done** | `src/movegen.rs` |
| `first_quiet_move` (the witness search) | **done** | `src/movegen.rs` |
| Fact tier for a view without `StateInfo` (`checking_pieces_and_pinned`) | **done** (fused, one pass) | `src/movegen.rs` |
| `has_legal_move_in_view_unpopulated` (exact, no `StateInfo` on the hot path) | **done** | `src/movegen.rs` |
| `has_legal_move_in_view` (board path) gets the screen as its first step | **done** | `src/movegen.rs` |
| `has_legal_move_ladder` (the pre-screen walk, unchanged) | **done** | `src/movegen.rs` |
| Wiring: `terminal_in_view_light`, `is_terminal_after`, `has_legal_move_after` | **done** | `src/position.rs`, `src/effects.rs` |
| `terminal_tail` takes the check flag lazily | **done** (deviation 1) | `src/position.rs` |
| `has_legal_move_in_view_light(view, checkers)` as proposed | **not added** (deviation 1: the fused pass made it redundant) | — |
| Witness soundness / coverage / fixture tests | **done** | `src/movegen.rs` (3 new unit tests) |
| Bench rows for the cost bars (build-only row, existential/terminality shares) | **done** | `examples/effect_bench.rs` |

## Implemented API (crate-private)

```rust
// src/movegen.rs

/// One move the screen can prove legal, or `None` when it cannot prove any.
/// `checkers`/`pinned` come from the caller's populated `StateInfo`.
fn quiet_legal_witness(view: &PositionView, checkers: Bitboard, pinned: Bitboard) -> Option<Move>;

/// Same contract, no `StateInfo`: the two facts come from
/// `checking_pieces_and_pinned` (one pass over the enemy's aligned sliders).
fn quiet_legal_witness_unpopulated(view: &PositionView) -> Option<Move>;

/// `(checkers, pinned)` for the commoner of `us`, equal to
/// `view.checkers(us)` and `view.pinned(us)`.
fn checking_pieces_and_pinned(view: &PositionView, us: Color) -> (Bitboard, Bitboard);

/// The first quiet non-commoner move whose origin `accept` allows.
fn first_quiet_move(view: &PositionView, accept: impl Fn(Square) -> bool) -> Option<Move>;

/// The pre-screen existence walk (unchanged behaviour, now a helper).
fn has_legal_move_ladder(view: &PositionView, state: &StateInfo) -> bool;

/// Screen first, ladder second; out of line on purpose.
#[inline(never)]
fn has_legal_move_ladder_unpopulated(view: &PositionView) -> bool;

/// Guard → screen → ladder. Exactly `!generate_legal_with_state(..).is_empty()`.
pub(crate) fn has_legal_move_in_view(view: &PositionView, state: &StateInfo) -> bool;

/// The child-side existential: screen (deriving its own facts) → ladder.
/// Exactly `has_legal_move_in_view(view, &populated_state)`.
pub(crate) fn has_legal_move_in_view_unpopulated(view: &PositionView) -> bool;

// src/position.rs
fn terminal_tail(view: &PositionView,
                 in_check: impl Fn() -> bool,          // was `in_check: bool`
                 moves_exist: impl FnOnce() -> bool) -> Option<Terminal>;
pub(crate) fn terminal_in_view_light(view: &PositionView) -> Option<Terminal>;   // body simplified
```

Contract of the screen, verbatim from the code: `Some(m)` is a *proof* (`m` is a
legal move, encoded exactly as `generate_legal` encodes it, queen promotion
included); `None` concludes **nothing** and every such case is decided by the
ladder that ran there before. Exactness of the public answers is therefore
preserved by construction, and the differential tests assert both halves
(witness ∈ legal list, and `screen-first` == `legal list non-empty`).

The proof is the plan's decision 3, quoted from `WidenedLegality`: a quiet
non-commoner move empties exactly its origin and triggers no blast, so it is
legal whenever the mover's commoner is not attacked after it — which holds when
the commoner is not attacked now and the origin is not the sole blocker between
it and an aligned enemy slider. Nothing else is claimed: commoner moves,
captures, en passant, castling and in-check positions all return `None`.

Public contract changes are *measurements*, not answers:

| Public query | Answer | Change |
| --- | --- | --- |
| `EffectContext::has_legal_move_after` | unchanged | 38 → 28 ns per child |
| `EffectContext::terminal_after`, `PostMove::terminal` | unchanged | 44 → 31 ns per child |
| `EffectContext::is_terminal_after` | unchanged (no material shortcut) | faster, same code |
| `EffectContext::child_outcome` | unchanged | 46 → 35 ns |
| `movegen::has_legal_move*` (board side) | unchanged | 6 → 2 ns per node |

## Differential counts

`cargo test` (debug, `src/movegen.rs` unit tests), `-- --nocapture`:

```text
existence screen: 2043 views (800 play-out positions), 39 views without a legal move
  coverage (legal move, no checker): light 1899/1930 (98.4 %), state 1899/1930 (98.4 %)
  witnesses by piece type: [Pawn 1591, Knight 202, Bishop 1, Rook 99, Queen 6, Commoner 0]
checking/pinned split: 2486 (view, color) pairs, 245 with checkers, 218 with pins
```

Corpora: the 41 perft positions **and every child of every pseudo-legal move of
them**, the 13 `STREAM_CORPUS` fixtures and their children, plus 4 start FENs ×
4 seeded walks × up to 120 plies (`walk` positions 800).

Asserted per view (2043 views, 3798 witness hits over both screens):

- `has_legal_move_in_view_unpopulated(view) == has_legal_move_in_view(view, &populated_state)`
  and both `== !generate_legal_in_view(..).is_empty()`;
- for both screens: `Some(m)` ⇒ `m` is a member of that view's legal list
  (encoding-exact, so castling/en-passant/promotion encodings cannot mask a
  mismatch);
- `checking_pieces_and_pinned(view, us)` equals `view.checkers(us)` and (on
  single-commoner boards, i.e. all legal positions) `view.pinned(us)`.

The integration differentials that already covered the child answers were run
**unmodified** and stay green with their own counts:
`terminal_after` 15 013 positions / 285 064 pseudo-legal moves / 29 842 decided
children; `post_move` 15 018 / 285 150 / 29 874. All of their assertions
(`has_legal_move_after == has_legal_move_with_state(&cloned_child, ..)`,
`terminal_after == cloned child's terminal()`, `is_terminal_after == cloned
child's is_terminal()`) pass with the new implementation.

## Acceptance fixtures

Positions and children of the classes handoff 3 names. `screen` is whether
`quiet_legal_witness_unpopulated` fires (both screens agree on every row); the
`has`/`terminal` columns were also cross-checked against a cloned `do_move`
child in the same test run.

| Class | Position | Screen | `has_legal_move*` | `terminal*` |
| --- | --- | --- | --- | --- |
| only commoner and pawns | `8/8/8/4k3/8/4P3/8/4K3 w - - 0 1` | fires (pawn push `e3e4`) | `true` | `None` |
| blocked-pawn shield | `8/8/8/8/8/4k3/4P3/4K3 b - - 0 1` | declines (no non-commoner piece) | `true` (child `e3d4`) | `None` |
| touching commoners | `8/8/8/8/8/8/1k6/2K5 w - - 0 1` | declines (only the commoner moves) | `true` (child `c1d1`) | `Some(MaterialDraw)` |
| in check, one escape | `7k/5P2/8/8/8/8/8/K6R b - - 0 1` | declines (in check) | `true` (child `h8g7`) | `None` |
| captures only | `k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1` | declines (no quiet move) | `true` (child `e5d6`) | `None` |
| mate child | `7k/8/8/5N2/3K4/8/8/R6R w - - 99 60` + `a1g1` | declines | `false` | `Some(Loss, NoMovesInCheck)` |
| stalemate child | `7k/7p/pK5P/P7/8/8/2r5/8 b - - 24 55` + `c2c7` | declines | `false` | `Some(Draw, Stalemate)` |
| bare commoners (K vs K) | `k7/8/8/8/8/8/8/7K w - - 0 1` | declines | `true` (child `h1g1`) | `Some(MaterialDraw)` |
| knight / bishop / rook / queen mover | `7k/8/8/8/8/8/8/N3K3 w - - 0 1` (`…/2B1K3`, `…/R3K3`, `…/3QK3`) | fires | `true` | `None` |
| promotion push | `7k/4P3/8/8/8/8/8/4K3 w - - 0 1` | fires (queen promotion) | `true` | `None` |
| pinned pawn, knight available | `4r2k/8/8/8/8/8/4P3/N3K3 w - - 0 1` | fires, witness origin **a1** | `true` | `None` |
| checkmate (no moves) | `rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3` | declines | `false` | `Some(Loss, NoMovesInCheck)` |
| stalemate (no moves) | `7k/8/8/8/8/8/nn6/Kn6 w - - 0 1` | declines | `false` | `Some(Draw, Stalemate)` |

The pinned-pawn row is the plan's "sole blocker" case: the e2 pawn is the only
piece between the e8 rook and the e1 commoner, so its push is illegal and the
screen must not claim it — the witness is the a1 knight's move instead.

## Benchmarks (same run, before/after)

Two harnesses, because `examples/effect_bench` prints integer ns (±2 ns on the
child rows) and the plan's bars are ratios:

1. `cargo run --release --example effect_bench 9 2000`, 7 interleaved runs × 3
   bench positions, medians per row. The bench now prints the divider the bars
   need — a `clone+do_move` (**build only**) row and a `post_move.derivation`
   row (the derivation with nothing read from it; the existing `post_move` row
   also reads five facts off the summary) — plus one line with the two shares.
2. A throwaway sub-nanosecond probe (not committed; `deep-win` plus the two
   middlegame FENs and their pseudo-legal children, min of 15 batches ×
   300 000 calls, 3 interleaved runs, the machine-warm pair reported). Absolute
   numbers are only meaningful on the machine that produced them; the probe is
   what the ratios below are taken from.

`effect_bench` medians (ns per call, before → after):

| Row | before | after |
| --- | ---: | ---: |
| `post_move` (derivation + five facts) | 21.0 | 20.0 |
| `post_move.derivation` (new row) | — | 16.0 |
| `clone+do_move` (build only, new row) | — | 15.0 |
| `clone+do_move+facts` | 25.0 | 26.0 |
| `has_legal_move_after` | 38.0 | **28.0** |
| `clone+do_move+has_legal_move` | 30.0 | 26.0 |
| `PostMove::terminal` / `terminal_after` | 44.0 | **31.0** |
| `clone+do_move+terminal` | 31.0 | 28.0 |
| `child_outcome` | 46.0 | 35.0 |
| `outcome_after` (per legal move) | 47.0 | 36.0 |
| `post_move_and_key` | 25.0 | 25.0 |
| `has_legal_move` (board, per node) | 6.0 | **2.0** |
| `classification` / `effect.full` / `effect.is_legal` | 21.0 / 28.0 / 25.0 | 21.0 / 28.0 / 24.0 |
| `generate_legal` / `for_each_legal` (per node) | 107.0 / 70.0 | 112.0 / 76.0 |
| `board.legal` (per candidate) | 8.0 | 8.0 |

The two generator rows moved by more than 2 % in this printout; the same-run
sub-nanosecond probe says they did not move at all (see "Gates" for the numbers,
and "Problems" 6).

Sub-nanosecond probe (ns per call, before → after, and the ratio to the
build-only row of the same run):

| Row | before | after | after ÷ build-only |
| --- | ---: | ---: | ---: |
| `clone+do_move` (build only) | 13.9 | 13.9 | 1.00 |
| `post_move` | 14.5 | 14.4 | 1.04 |
| `has_legal_move_after` | 32.5 | **24.3** | 1.75 |
| — of which the existence question (`… − post_move`) | 18.1 | **10.2** | **0.73** |
| `post_move+terminal()` / `terminal_after` | 35.2 | **27.2** | 1.96 |
| — of which terminality (`… − post_move`) | 20.8 | **13.0** | **0.94** |
| `post_move_and_key` + `terminal()` | 37.4 | 29.7 | 2.14 |
| `clone+do_move+outcome()+hash()` | 32.5 | 26.4 | 1.90 |
| `clone+do_move+terminal()` | 31.3 | 26.5 | 1.91 |
| `clone+do_move+populate_state+has_legal_move` | 32.4 | 27.6 | 1.99 |
| `has_legal_move` (board path) | 6.73 | **2.75** | 0.20 |

Cost split of the existence question measured with the same probe (ns,
after): `checking_pieces_and_pinned` 2.7, `first_quiet_move` 3.2, whole witness
7.3, whole `has_legal_move_in_view_unpopulated` 7.4; the remaining ~3 ns of the
10.2 ns share is the in-situ distance between the standalone screen and the
same screen inlined behind `analyze` (register pressure, no separate work).

Screen hit rate on the bench children: **66/66** children of the three bench
positions are proven by the screen (deep-win 32/32, middlegame-13 34/34), none
in check, so the measured share is the screen's own cost, not a fallback.

## Gates

| Gate | Result |
| --- | --- |
| `cargo test` | 13 result blocks, all ok, 0 failures (173 lib — three new tests —, 5 `post_move`, 7 `child_state`, 6 `terminal_after`, 12 `effects`, 6 `conformance`, 4 `perft_tests`, 3 `public_api`, 1 `move_order_golden`, 1 `verify_moves`, 3 `interchange`, 2 `null_for`, 16 doctests) |
| `cargo test --features unsafe-optimizations` | same 13 blocks, all ok |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `cargo doc` (+ `RUSTDOCFLAGS=-D warnings`, `cargo test --doc`) | no warnings; 16 doctests ok |
| `verify_perft 5` (three interleaved runs) | 41/41 each; 2.204 / 2.209 / 2.215 s before, 2.219 / 2.222 / 2.216 s after → median **+0.4 %** |
| `verify_perft 6` | 41/41, 64.38 s (plan 1 reported 63.84 s before this plan) |
| `perft_tests`, `move_order_golden`, `verify_moves`, `conformance`, `effects`, `terminal_after`, `child_state`, `public_api`, `post_move` | all ok, **no expectation edited** |
| existential share ≤ 0.40 × build-only | **missed**: 0.73 × (10.2 / 13.9 ns) |
| `PostMove::terminal` / `terminal_after` ≤ `post_move` + 0.45 × build-only | **missed**: 0.94 × (13.0 / 13.9 ns) |
| composite `post_move_and_key` + `terminal()` ≤ 0.95 × `clone`+`do_move`+`outcome()`+key | **missed**: 29.7 vs 26.4 ns = 1.13 × (plan 3 owns the verdict) |
| `has_legal_move` (board path) no regression > 2 % | **met and much better**: 6.73 → 2.75 ns (0.41 ×) |
| `generate_legal`, `for_each_legal`, `classification`, `effect` no regression > 2 % | **met**: sub-ns probe identical within noise — `generate_legal` 90.3/96.2/101.1 → 89.7/94.7/101.3, `for_each_legal` 67.5/74.3/78.8 → 67.6/74.7/78.9, `classification` 21 → 21, `effect.full` 28 → 28; perft whole-tree +0.4 % |
| `effect_bench 9 2000` (three runs, medians) | table above (7 interleaved runs) |

## Changed files

- `src/movegen.rs` — `quiet_legal_witness`, `quiet_legal_witness_unpopulated`,
  `checking_pieces_and_pinned`, `first_quiet_move`, `has_legal_move_ladder`,
  `has_legal_move_ladder_unpopulated`, `has_legal_move_in_view` (guard → screen
  → ladder), `has_legal_move_in_view_unpopulated`, module doc for the two-step
  shape, three new unit tests (`existence_screen_matches_legal_lists`,
  `existence_screen_fixtures`,
  `checking_pieces_and_pinned_matches_view_queries`).
- `src/position.rs` — `terminal_tail` takes the check flag as a closure (the
  light path derives it only on the cold branches), `terminal_in_view_light`
  simplified, docs, `terminal_tail_branches` updated for the closure form,
  `populate_state_with_checkers` doc updated.
- `src/effects.rs` — `has_legal_move_after` and `is_terminal_after` use
  `has_legal_move_in_view_unpopulated` (no `StateInfo`, no double attacker
  query), module doc section "Child existence", `terminal_after` doc.
- `examples/effect_bench.rs` — new `clone+do_move` (build only) and
  `post_move.derivation` rows, and one `shares vs build only` line per position
  (existential and terminality share, both as ns and as a ratio to the
  build-only row).
- `CHANGELOG.md` (`[Unreleased]` → Changed), `README.md`, `src/lib.rs`,
  `docs/perf/perf_guide.md` — docs.
- `docs/plans/feedback/report_handoff3_2.md` — this report.

## Problems encountered

1. **The gates were not measurable with the harness that existed.** The plan's
   bars are ratios to the *build-only* row, and `effect_bench` had no
   `clone+do_move` row; its `post_move` row also reads five facts off the
   summary, which shrinks the apparent share by ~2 ns. Both rows were added
   (plus the derived share line) before any ratio was quoted.
2. **Integer-ns rows hid a factor of two.** The first implementation measured
   "34 ns" for `has_legal_move_after` in `effect_bench`, and successive within-
   noise edits moved that by ±3. A throwaway sub-nanosecond probe was needed to
   A/B variants reliably (min of 15 batches × 300 000, 3 interleaved runs). Two
   of the reported "wins" below were only visible that way.
3. **Naive design: verify the candidate with a post-move attacker query.** The
   first implementation searched a quiet move and then asked
   `is_attacked_by(ksq, occupied ^ from, ..)` per candidate. Measured share
   17 → 15 ns — *worse* than populating the facts once (`checkers` 4.0 +
   `pinned` 1.7 ns) and filtering origins with a bit test.
4. **The shared non-generic search helper cost 3 ns.** Passing a `Bitboard`
   mask into one shared `first_quiet_move` made LLVM stop inlining it at both
   call sites: the share went 15 → 17.5 ns. Keeping the `impl Fn(Square) -> bool`
   parameter (one monomorphised, inlined copy per screen) restored it. This is
   fragile and is called out as a maintenance risk below.
5. **Fusing `checkers` and `pinned` was the single real win.** Both answers read
   the same thing: with the *empty-occupancy* rays, an aligned enemy slider is
   a checker when nothing stands between it and the commoner, and a pinning
   piece is the single blocker when exactly one square is occupied there. One
   pass (2 magic lookups + a `between` lookup per sniper) replaced `checkers`
   (2 magic lookups on the real occupancy + 4 masks) *and* `pinned` (2 more
   magic lookups + the same sniper loop): share 15 → 10 ns. The two-function
   version is kept for the state-based screen, where both facts are free.
6. **A phantom generator regression.** `effect_bench` showed `for_each_legal`
   70 → 76 ns and `generate_legal` 107 → 112 ns on `middlegame-13` across three
   interleaved sessions, which would have been a 2 % blocker. The sub-ns probe
   in the same trees shows both rows identical to 0.1 ns (67.53 → 67.60,
   74.27 → 74.67, 78.76 → 78.90), and `verify_perft 5` is +0.4 % whole-tree, so
   the printout difference is integer rounding plus code-layout noise on one
   bench position. The probe is cited in the gate table rather than the bench
   row.
7. **Plan 1's "`pinned` is still read" note applies here too.** The child path
   no longer *populates* a `StateInfo`, but the screen still needs the
   sole-blocker set — derived directly, in the fused pass, not via
   `populate_state`.

## Deviations from the plan, and the measurements that motivated them

1. **No `has_legal_move_in_view_light(view, checkers)`; the check flag became
   lazy in `terminal_tail`.** The plan's shape ("one `checkers` value serving
   the terminal precedence and the existence query") assumes `checkers` is
   needed on the hot path. It is not: `terminal_tail` reads `in_check` only when
   the clock is up or when the existence question comes back `false`, i.e. on
   the cold branches. With the fused pass the screen needs *both* facts anyway,
   so a caller-supplied `checkers` would have been a second query. Measured: the
   checkers-first shape (checkers + pinned + bit-test search) is the 15 ns
   share; the fused shape is 10.2 ns. `terminal_tail` now takes
   `in_check: impl Fn() -> bool`, which costs the board path nothing (it passes
   `|| !state.checkers.is_empty()`, inlined) and lets the light path compute the
   attacker set only when it matters. One precedence implementation is kept
   (plan 1's lesson); the differential
   `terminal_light_matches_board_path(_playouts)` still pins it.
2. **The view-only screen uses the fused sniper pass, not `PositionView::pinned`
   plus a post-move query.** 10.2 ns vs 15 ns (problem 3/5). The pass is
   asserted equal to both board queries on 2486 sampled (view, color) pairs.
3. **`first_quiet_move` keeps the `impl Fn` parameter** (problem 4): a `Bitboard`
   mask parameter costs 2.5 ns at both call sites because LLVM stops inlining
   the shared instantiation.
4. **Pawn pushes are found with two shifts over the whole pawn set** rather than
   a per-pawn loop; the loop measured the same (the first pawn with a push
   usually answers), but the shift version has no per-pawn branch and is
   simpler to reason about.
5. **The out-of-line fallback (`#[inline(never)] has_legal_move_ladder_unpopulated`)**
   measured neutral (±0.5 ns) and is kept so the hot path does not carry the
   `StateInfo` construction.
6. **Two screens instead of one.** `quiet_legal_witness` (facts from
   `StateInfo`, no attacker query at all) exists for the board path, where it
   *improves* `has_legal_move` from 6.7 to 2.75 ns — a much larger win than
   anything available to the child path. The plan's `has_legal_move_in_view` was
   supposed to get the screen "as a first step (it has `checkers` already and
   loses nothing)"; with `pinned` also in hand it loses nothing *and* pays no
   attacker query.
7. **The cost gates are missed** (see "Gates"). The honest statement: the
   existence question costs **10.2 ns** per child here, which is under the
   handoff's 12 ns *absolute* target but **0.73 × this machine's build-only
   row** (13.9 ns) instead of ≤ 0.40 ×, because a board write is cheap on this
   machine while an attacker query is not. The failing class is not one class:
   the screen fires on 100 % of the bench children, so the miss is the screen's
   own cost (one sniper pass 2.7 ns + one quiet-move search 3.2 ns + ~4 ns of
   in-situ glue). The composite row (`post_move_and_key` + `terminal()` 29.7 ns
   vs `clone`+`do_move`+`outcome()`+key 26.4 ns) is likewise still 1.13 × the
   clone path; on this machine `analyze` (14.4 ns) costs what `clone`+`do_move`
   costs (13.9 ns), so the summary approach's advantage is API shape, not
   speed. Plan 3 owns the verdict and the wording.

## Unresolved parts

- **Ratio bars missed** (existential 0.73 ×, terminality 0.94 ×, composite
  1.13 ×). Plan 3 must publish this as the stack's verdict; nothing in this
  plan found a cheaper way to answer the existence question.
- **Classes the screen cannot serve** (all fall back to the ladder, so answers
  stay exact but those children pay the old cost):
  - children in check (245 of the 2486 sampled (view, color) pairs had
    checkers) — evading check is the ladder's question;
  - children with no quiet non-commoner move: captures-only, commoner-move-only
    (bare K vs K), and positions where every quiet non-commoner move is a
    pin-opening move;
  - en-passant-only and castling-only children (the screen never claims those
    move types);
  - decided children (mate, stalemate) — inherent: proving that *no* move is
    legal needs the walk. 61 of 1930 undecided, checker-free views (3.2 %)
    fell back.
- **`checking_pieces_and_pinned` uses the lowest commoner only.** That is
  exactly what `legal_with_counts` decides (a quiet move keeps the commoner
  set, and legality is judged on its lowest square), so the screen is sound on
  multi-commoner boards — but the function is *not* equal to
  `PositionView::pinned` there (which loops over all commoners), and the test
  only asserts the equality on single-commoner boards. Documented in the
  function's doc comment.
- **The `impl Fn` inlining is load-bearing.** A future refactor that turns
  `first_quiet_move` into one non-generic function silently costs ~2.5 ns at
  both call sites (problem 4). The coverage test does not catch that (it
  measures hits, not time); only the bench does.
- No measurement of the *decided* children (mate/stalemate) separately: they
  pay screen (7 ns, fails) + populate + full ladder, ~35–40 ns per decided
  child on the bench positions. Handoff 3's complaint about the cost of decided
  answers is therefore only partly addressed.

## Missing tests

- No test that the screen *declines* for an en-passant-only child; the class is
  covered indirectly by the corpus differential (the "en passant only" fixture
  of `has_legal_move_deterministic_fixtures` asserts the answer, not the
  screen's decline).
- No differential for multi-commoner boards (`Setup`-built, unreachable in
  legal play): `checking_pieces_and_pinned` is only asserted equal to
  `checkers`/`pinned` where one commoner per side exists.
- No coverage assertion per *bench position*; the coverage test walks the
  corpus, the stream fixtures and 4 seeded play-outs. The bench's 66/66 child
  hit rate is measured by hand (this report), not asserted in a test.
- No test pins the *cost* of the screen (the 2.5 ns inlining regression of
  problem 4 would go unnoticed by `cargo test`); the crate has no
  `cargo bench`-style regression gate by design.
- No test for a promotion witness on a *capture* square (the screen never claims
  captures) — covered by the ladder differentials only.

## Next steps

1. **Plan 3** — the composite measurement (`post_move_and_key` + `terminal()`
   against `clone`+`do_move`+`outcome()`+key: 29.7 vs 26.4 ns on the probe,
   1.13 ×), the docs, and the 2.8.0 checklist, including the honest verdict
   section: the stack is faster than 2.7.0 on every child row
   (`has_legal_move_after` 32.5 → 24.3 ns, `terminal_after` 35.2 → 27.2 ns,
   `child_outcome` 46 → 35 ns, board `has_legal_move` 6.7 → 2.75 ns) but the
   plan's *ratio* bars are missed, and the summary path is still slightly slower
   than the `clone` + `do_move` path on this machine.
2. If a later plan wants the last few ns of the existence question, the
   measured split is: fused sniper pass 2.7 ns, quiet-move search 3.2 ns, ~4 ns
   in-situ glue. The only shapes not yet tried are (a) a bitboard-only boolean
   screen that never builds the witness `Move` (saves the `Move` encoding and
   the promotion test, but splits the implementation the soundness test covers)
   and (b) a fused `analyze`-and-screen entry point that shares the child's
   occupancy with the screen (the `PostMove` is in hand by then, so the glue
   could shrink) — neither was built because both trade the single-implementation
   property for ~2–3 ns.
3. `effect_bench`'s integer rows are too coarse for 2 ns differences; a future
   harness plan may want the sub-nanosecond mode used here (plan 3 owns the
   harness).
