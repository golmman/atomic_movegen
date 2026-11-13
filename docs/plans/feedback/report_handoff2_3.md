# Report — Plan 3: legality-ladder bypass, measurements, documentation, release 2.7.0 (MG18)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| `MoveClassification` (five `MoveEffect` fields, no `is_legal`) | Done | `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`, all fields `pub`, documented; **not** `#[non_exhaustive]` (see deviation 1). |
| `MoveEffect::classification()` | Done | `#[inline]`, copies the five fields. |
| `EffectContext::classification()` | Done | `analyze` + `classify`; no `is_attacked_by` for the mover's commoner. |
| `effect` and `classification` share one derivation | Done | `effect` = `effect_and_post(..).0`; both build the fields in `classify` / `classify_malformed`. No duplicated derivation. |
| `MoveEffect` field set, size and results unchanged | Done | Untouched; `tests/effects.rs` effect differential unchanged and green. |
| Crate-root re-export + `Send + Sync` assertion | Done | `atomic_movegen::MoveClassification`; `send_sync_assertions` extended. |
| `classification_matches_effect` differential | Done | 285,079 candidates (262,387 legal) over perft corpus + plan-1/2 fixtures + seeded play-outs; every field compared to `effect`, to `Board`, and to a cloned `do_move` child. |
| `classification_malformed_input` | Done | `Move::NONE`, empty-origin move, malformed-on-start-position (`is_capture` from the parent view), game-over parent. |
| Existing effect differential unchanged | Done | `cargo test --test effects` 12/12, including the two new tests. |
| Public-API-surface test | Done | `tests/public_api.rs` (3 tests): every 2.7.0 name resolves from the crate root, `AfterState`/`PostMove`/`MoveClassification`/`MoveEffect`/`EffectContext`/`Terminal` `Send + Sync`. |
| Benchmarks: `classification` rows + legal-list loop | Done | `examples/effect_bench.rs`. |
| README documentation pass | Done | Features, a `terminal_after` + `child_outcome` + `classification` example set, and the honest "what the effect path does and does not decide" paragraph. |
| `src/lib.rs` crate docs | Done | Effect section rewritten around the child view + classification; allocation-free list extended. |
| `CHANGELOG.md` `[2.7.0]` | Done | Added/Changed/Fixed; the `outcome_after` doc bug is named **Fixed (docs only, no behaviour change)**. |
| `docs/perf/perf_guide.md` | Done | New "Where to look in this crate" section (view code, child path, measured starting points). |
| Version bump 2.7.0 | Done | `Cargo.toml` + `Cargo.lock` (`Cargo.lock` is gitignored for this library crate). |
| Release checklist (`test`, `unsafe-optimizations`, `clippy`, `fmt`, `doc`, `package --list`, `verify_perft`) | Done | See “Verification run”. |
| `cargo publish --dry-run` | Done | 44 files, 902.1 KiB (206.3 KiB compressed), compiles from the packed crate; upload aborted by the dry run (expected). |
| Final report | Done | This file. |

## Public API as implemented

```rust
// src/effects.rs, re-exported at the crate root.

/// The rules classification of one pseudo-legal move, without a legality
/// verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveClassification {
    pub is_capture: bool,
    pub casualties: BlastCasualties,
    pub checks_enemy: bool,
    pub explodes_enemy_commoner: bool,
    pub leaves_own_commoner: bool,
}

impl MoveEffect {
    pub fn classification(&self) -> MoveClassification;
}

impl EffectContext {
    /// Classify `m` without running the legality ladder.
    pub fn classification(&self, board: &Board, m: Move) -> MoveClassification;
    // unchanged: new, side_to_move, effect, effect_and_post, post_move,
    //            is_legal, outcome_after, child_outcome, terminal_after,
    //            is_terminal_after, has_legal_move_after
}
```

There is deliberately **no** `effect_trusted`/`effect_assume_legal`/`effect_unchecked`
and no `MoveEffect` with a documented-but-not-evaluated `is_legal`: a public
boolean set to a value the crate did not compute would be a silent correctness
hole in a rules crate. `MoveEffect` keeps all six public fields (no break) and
gains only the `classification()` accessor.

Internal shape (one derivation, two entry points):

```rust
fn classify(&self, post: &PostMove) -> MoveClassification;          // the five fields
fn classify_malformed(&self, m: Move) -> MoveClassification;        // capture flag from the parent view
pub fn classification(..) -> MoveClassification { analyze -> classify | classify_malformed }
pub fn effect_and_post(..) -> (MoveEffect, Option<PostMove>) { (classify, is_legal_from) -> MoveEffect }
pub fn effect(..) -> MoveEffect { effect_and_post(..).0 }
```

`classification` runs exactly one child derivation (`analyze`) and one
`is_attacked_by` for the *enemy* commoner (`checks_enemy`); it skips the
`is_attacked_by` for the mover's commoner and the commoner-count comparison that
`is_legal_from` performs.

## Classification differential

`cargo test --release --test effects -- --nocapture`:

```
classification checked on 285079 candidates (262387 legal)
visited 15025 positions, 285193 pseudo-legal moves
checked 285193 candidates against a cloned child
```

Corpora: the 41-position perft table, the classification fixtures (plan-1
child-view fixtures: stalemate ×2, extinction, material draws ×3, mate-on-the-clock,
clock draw, en-passant-escape child, castling, touching commoners, explosion
win), the plan-2 effect fixtures, and seeded play-outs (6 start FENs × 16 seeds ×
300 plies, plus every perft position × 2 seeds × 60 plies).

For every candidate:

| Assertion | Result |
| --- | --- |
| `ctx.classification(b, m) == ctx.effect(b, m).classification()` | equal |
| each of the five fields equals the `effect` field (field-by-field compare) | equal |
| `is_capture == board.is_capture(m)` | equal |
| `casualties == board.blast_casualties(m)` | equal |
| `leaves_own_commoner == !child.commoners(us).is_empty()` | equal |
| `explodes_enemy_commoner == !board.commoners(them).is_empty() && child.commoners(them).is_empty()` | equal |
| `checks_enemy == !child.checkers().is_empty()` | equal |
| for legal candidates: `classification == effect.classification()` | equal (262,387 moves) |

The `src/effects.rs` unit test `classification_matches_effect` repeats the
`classification == effect.classification()` (and capture/casualties) identity on
the 18-position module corpus, including the malformed branch of both entry
points. `tests/public_api.rs::classification_is_usable_from_a_legal_list` pins
the consuming shape (legal list → classify → no second verdict) and the
"classification is not legality" contract on the documented self-explosion
fixture.

## Benchmark table

`cargo run --release --example effect_bench 9 2000`, ns per call; each cell is
the median of three runs (each run already a median of 9 batches × 2000 calls).

Per pseudo-legal move:

| Position | `classification` | `effect` (full) | `effect.is_legal` | `Board::legal` |
| --- | ---: | ---: | ---: | ---: |
| deep-win | 20 | 27 | 24 | 8 |
| middlegame-13 | 19 | 26 | 23 | 8 |
| middlegame-21 | 20 | 26 | 23 | 8 |

The legal-list consumer loop (`generate_legal_with_state` + classify every move):

| Position | `classification` loop | `effect` loop | generate only |
| --- | ---: | ---: | ---: |
| deep-win (32 legal) | 810 | 960 | 110 |
| middlegame-13 (31 legal) | 812 | 966 | 110 |
| middlegame-21 (29 legal) | 825 | 975 | 116 |

Interpretation, reported as measured rather than promised: `classification` is
**~6–8 ns cheaper per candidate** (~25 %) than `effect`, which is the duplicated
ladder query plus the commoner-count comparison; over a 30-move legal list that
is **~150–165 ns per node** (~16 % of the classify-after-generate loop). It is
still strictly slower than `Board::legal` (8 ns) because the effect path derives
the child placement — the same honest note as in 2.5.0/2.6.0. So MG18 is
"removes one duplicated rule query from a path the consumer chose to use", not
a speed claim for the generator, and the effect path is **not** adopted into
`generate_legal_with_state` or perft.

Child-path rows on the same runs (unchanged from plan 2, within noise):
`terminal_after` 39–47 ns vs `clone+do_move+terminal` 30–34 ns;
`child_outcome` 42–44 ns vs the split `is_legal`+`terminal_after` 56–62 ns;
`post_move` 21–22 ns vs `clone+do_move+facts` 23–25 ns.

Decided share over this crate's seeded play-outs (8 seeds × depth 60 × 4 start
FENs): pseudo-legal children 47,884 / decided 816 (**1.70 %**); legal children
45,512 / decided 434 (0.95 %). This crate's own tree, not a downstream search
tree — the downstream gate (decided share > 10 % of built children) is theirs to
re-run.

## Documentation

- `README.md` — new feature bullet for `MoveClassification`; new example set
  (`classification` next to `effect().classification()` over a legal list,
  `terminal_after` + `child_outcome`), a "a classification is not a legality
  claim" warning, and the "what the effect path does and does not decide"
  paragraph with the measured 8–9 ns vs 26–29 ns numbers. Dependency snippet
  bumped to `2.7.0`.
- `src/lib.rs` — the effect section now covers the child view, `terminal_after`,
  `AfterState`/`child_outcome`, `PostMove`/`effect_and_post` and
  `MoveClassification`/`classification`; the allocation-free list gained
  `EffectContext::classification` and `MoveEffect::classification`.
- `CHANGELOG.md` — `[2.7.0] - 2026-11-13` with an Added/Changed/Fixed split,
  explicitly naming the `outcome_after` documentation bug as **Fixed
  (documentation only, no behaviour change)**, plus the classification saving as
  measured.
- `docs/perf/perf_guide.md` — new "Where to look in this crate" section before
  the tool guidance: `board.rs` / `position.rs` (view, `child_view`,
  `terminal_in_view`) / `movegen.rs` (ladder) / `effects.rs` (child path), with
  the measured starting points and the "> 2 % is a regression" rule.

## Release record

- Version: **2.7.0** in `Cargo.toml`; `Cargo.lock` regenerated locally (it is
  gitignored: library crate).
- `cargo test` — 163 unit tests + 12 integration binaries, all green.
- `cargo test --features unsafe-optimizations` — all green.
- `cargo clippy --all-targets -- -D warnings` and
  `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings` —
  clean.
- `cargo fmt --check` — clean.
- `cargo doc` / `cargo test --doc` — no warnings, 12 doctests green.
- `cargo run --release --example verify_perft 5` — **41/41 passed**, 2.219 s.
- `cargo run --release --example verify_perft 6` — **41/41 passed**, 64.718 s
  (plan-2 baseline 64.4 s; no hot-path code changed in this plan, so this is
  run-to-run noise).
- `cargo run --release --example verify_san` — 41/41 corpus positions,
  507,346 positions, 14,003,226 moves, 6.078 s.
- `cargo run --release --example effect_bench 9 2000` — table above.
- `cargo package --list` — 44 files; no `docs/`, no `Fairy-Stockfish/`, no
  scratch example, includes `tests/public_api.rs`.
- `cargo publish --dry-run --allow-dirty` — packaged 44 files (902.1 KiB /
  206.3 KiB compressed), verified by compiling the packed crate, upload aborted
  by the dry run as expected. (`--allow-dirty` because this repository's writing
  git commands are out of scope for the agent; a real publish needs the change
  committed first.)

### What the downstream should re-run

1. Re-point the dependency at the **published** `atomic-movegen = "2.7.0"`
   (no path dependency; handoff 2 §3).
2. Re-run `measurements/plan29/probe.py`: the gate is a decided share **> 10 %
   of built children** on *their* hand stack (this crate's own play-out share is
   1.70 % pseudo-legal / 0.95 % legal and is not a predictor of theirs).
3. Replace `is_legal` + `terminal_after` with `child_outcome`, and
   clone-and-read with `post_move`/`effect_and_post` where the child facts are
   wanted.
4. Replace `effect(..).is_capture/casualties/…` with `classification(..)` for
   moves that came out of `generate_legal_with_state`.
5. Expect `EffectContext` to still be slower than `Board::legal` per candidate;
   the win is the removed child write, not the ladder.

## Deviations from the plan

1. **`MoveClassification` is not `#[non_exhaustive]`.** A struct literal is the
   natural downstream use (compare against a stored value, pattern-match the
   fields), and the plan's own API sketch has all fields public. Adding a field
   later is a breaking change; that is accepted and can be avoided by keeping
   any future field on a *new* type. No plan text forbade it, but the `AfterState`
   rationale is different, so it is recorded here.
2. **No `classify_and_decide` function.** The plan's step 1 named it; the
   implementation splits it into `classify`/`classify_malformed` (the five fields
   for a derived child / for malformed input) plus the existing `is_legal_from`,
   because `effect_and_post` needs the `PostMove` and the verdict *and*
   `classification` needs only the fields without deriving a `PostMove` return
   value. Same property the plan asked for: one derivation, no duplicated field
   logic, no oracle for `classification`.
3. **`terminal_after_bench` does not exist** and was not created: the
   `terminal_after`/`child_outcome`/`post_move` rows live in
   `examples/effect_bench.rs` (added in plan 2) and were re-run there. Creating a
   second bench binary for the same measurement would duplicate it.
4. **README snippets are not doctests.** `README.md` is not `include_str!`-ed
   into the crate docs (it would duplicate the crate-level docs), so
   `cargo test --doc` cannot compile its examples. They were verified by
   extracting every ```rust``` block into a temporary integration test and
   compiling/running it (all 9 blocks green); see "Problems encountered" 2.
5. **`tests/public_api.rs` is a new integration binary** rather than an addition
   to `tests/conformance.rs`: it is about the *import surface*, not the corpus.
6. **`docs/perf/perf_guide.md` had no "where to look" section.** One was added
   before the tool guidance; the existing generic content is untouched.

## Problems encountered

1. **`Move::NONE` is not "no capture".** `Move::NONE` encodes `to_sq == A1`, and
   the documented malformed branch of `effect` takes `is_capture` from the parent
   view, so on the start position `effect(Move::NONE).is_capture == true`. The
   first version of `classification_malformed_input` asserted an all-`false`
   value and failed immediately (the differential, not a reviewer, caught it).
   The test now pins both halves: all-`false` on a board with `a1` empty, and
   `is_capture == true` on the start position, plus the identity with `effect`'s
   malformed branch.
2. **A pre-existing README bug was found by compiling the snippets** (plan 2
   legacy): the "not decided" example used `h1h2` in the 99-ply clock position
   `7k/8/8/5N2/3K4/8/8/R6R w - - 99 60`, whose child is a 50-move **draw**
   (`Some(Terminal { Draw, FiftyMoveRule })`), not `None`. Replaced with the
   start position's `e2e4`. The `lib.rs`/module doctest already used the correct
   example, which is why `cargo test --doc` had never caught this — README code
   was never compiled. Fixed as part of this documentation pass.
3. **`cargo package --list` / `cargo publish --dry-run` refuse a dirty tree.**
   The agent must not run writing git commands (`AGENTS.md`), so both were run
   with `--allow-dirty`; the packed file list and the packed-crate compile are
   identical either way.
4. `examples/effect_bench.rs`'s `println!` lines were long enough that
   `cargo fmt` rewrapped them twice; the row format was adjusted (one row per
   measurement family) so the source stays formatted and the output stays
   aligned.

## Unresolved parts

- The effect path is still ~3× the cost of `Board::legal` per candidate. Nothing
  in this plan was supposed to change that (the 2.5.0 measurement says the child
  derivation, not the ladder, is the cost), and the plan's non-goals forbid
  adopting the effect path into the generator. A future change there must start
  from a profile.
- `MoveClassification` has no `Default` and no constructor; a downstream that
  wants a "no effect" sentinel must write the struct literal. Not added because
  no consumer has asked and `MoveEffect` has no such constructor either.
- `CHILD_CASES` is still not re-exported at the crate root (plan 2 deviation 5);
  `tests/public_api.rs` therefore imports it via `conformance::CHILD_CASES`.
  If a downstream asks for symmetry with `CONFORMANCE_CASES`, that is a
  one-line additive change.
- The stale-`StateInfo` contract (plan 1/2 gap) is still not pinned by a test.
- The 2.7.0 release is prepared but **not published**: the version bump is
  uncommitted and this agent does not run `git commit`/`cargo publish`.

## Missing tests

- No differential for `classification` on well-formed but non-pseudo-legal moves
  (a rook moving through a piece): the documented contract is pseudo-legal input,
  and malformed input is covered. Same gap as `effect`.
- No test pins the *timing* claim. `examples/effect_bench.rs` reports it; a
  benchmark-based assertion would be flaky and is out of scope (no new
  dependency).
- No test asserts that `classification` skips the mover's-commoner query (an
  implementation detail, not a contract); only the field-by-field equality is
  pinned.

## Changed files

- `src/effects.rs` — `MoveClassification`, `MoveEffect::classification`,
  `EffectContext::classification`, `classify`/`classify_malformed`,
  `effect_and_post` rebuilt on them, module docs (classification section +
  doctest), unit test `classification_matches_effect`.
- `src/lib.rs` — re-export `MoveClassification`; crate docs (effect section,
  allocation-free list); `send_sync` assertion.
- `tests/effects.rs` — `classification_matches_effect_and_child`,
  `classification_malformed_input`, `CLASSIFICATION_FIXTURES`.
- `tests/public_api.rs` — **new**: 2.7.0 import surface, legal-list consumer
  shape, `Send + Sync`.
- `examples/effect_bench.rs` — `classification` row, legal-list loop row
  (`classification` vs `effect` vs generate-only).
- `README.md` — classification feature + examples, honest effect-path paragraph,
  2.7.0 dependency snippet, fixed the `terminal_after` "not decided" example.
- `CHANGELOG.md` — `[2.7.0]`.
- `Cargo.toml` (+ local `Cargo.lock`) — 2.7.0.
- `docs/perf/perf_guide.md` — "Where to look in this crate".
- `docs/plans/feedback/report_handoff2_3.md` — this report.

## Verification run

```sh
cargo test                                                # 163 unit + 12 binaries, green
cargo test --features unsafe-optimizations                # green
cargo clippy --all-targets -- -D warnings                 # clean
cargo clippy --all-targets --features unsafe-optimizations -- -D warnings  # clean
cargo fmt --check                                         # clean
cargo doc && cargo test --doc                             # no warnings, 12 doctests
cargo test --test perft_tests --test move_order_golden --test verify_moves \
           --test conformance --test effects --test terminal_after \
           --test child_state --test public_api
cargo test --release --test effects -- --nocapture        # classification differential
cargo run --release --example verify_perft 5              # 41/41, 2.219 s
cargo run --release --example verify_perft 6              # 41/41, 64.718 s
cargo run --release --example verify_san                  # 41/41, 507k positions / 14.0M moves
cargo run --release --example effect_bench 9 2000         # three runs, medians above
cargo package --list --allow-dirty                        # 44 files, no docs/, no Fairy-Stockfish/
cargo publish --dry-run --allow-dirty                      # packaged + verified, upload aborted (dry run)
```

Integration suites after the change: `child_state` 7, `conformance` 6,
`effects` 12, `interchange` 3, `move_order_golden` 1, `null_for` 2,
`perft_tests` 4, `public_api` 3, `terminal_after` 6, `verify_moves` 1,
doctests 12.

## Next steps (possible `handoff3`)

1. **Lazy-state child path.** `terminal_after`/`child_outcome` populate a full
   `StateInfo` for the child (checkers, pins, commoner counts). `child_outcome`
   only needs the commoner counts and the legal-move existence query, and
   `has_legal_move_in_view` does not need pins. A lazy `populate_state` mode (or
   splitting `StateInfo` population into a cheap tier and a full tier) is the
   obvious next measurement, and it is the only remaining path to closing the
   ~40 ns vs ~30 ns gap to `clone+do_move`.
2. **Effect-path adoption (or not).** Re-measure whether the ladder's
   `is_attacked_by` is worth replacing by the effect path's derived attacks once
   the child derivation is cheaper; today the answer is no and the plan says so
   explicitly. Start from a profile.
3. **Downstream feedback on naming**: `AfterState` (`Illegal`/`Undecided`/
   `Terminal`), `PostMove`, `MoveClassification`. If a consumer asks for a single
   "effect + summary + arm" query, add it as a third entry point rather than
   growing `MoveEffect`.
4. **Re-export `CHILD_CASES`** at the crate root if the downstream protocol wants
   the same import shape as `CONFORMANCE_CASES`.
5. **Pin the stale-`StateInfo` contract** for the child queries (plan 1/2/3
   shared gap).
6. **Publish 2.7.0** once the tree is committed (out of scope for the agent).
