# Implementation report — wishlist 1 plan 1 (W1: legal-EP child key on the effect path)

Status: **implemented.** The mode-aware child query and its view-side legal-EP
test are in the tree. `EnPassantMode::Always`, `post_move_and_key`, the key
table and `ZOBRIST_VERSION` are unchanged.

The measured verdict is deliberately conservative: the new `Legal` query is
correct and clone-free, but on this crate's double-push corpus it is **not
cheaper** than `clone + do_move + hash_with(Legal)` (20.3 ns vs 19.8 ns,
**1.025×** in the low-noise run). The plan's fallback applies: the report states
that plainly and leaves adoption to the downstream's measured wall win. No
promise is made about any downstream search tree.

## Item status

| Plan item | Status | Where |
| --- | --- | --- |
| W1 public query `EffectContext::post_move_and_key_with` | **done** | `src/effects.rs` |
| `post_move_and_key` becomes the `Always` spelling | **done** | `src/effects.rs` |
| Crate-private `position::legal_ep_square_in_view` | **done** | `src/position.rs` |
| Incremental mode-aware key (`child_key_with`) | **done** | `src/effects.rs` |
| No clone, no `StateInfo` population on the keyed child path | **done** | `src/effects.rs` |
| Differential against clone + `do_move` + `hash_with(mode)` | **done** | `tests/ep_key.rs`, unit tests in `src/effects.rs` |
| Explicit legal / impossible / pinned / self-exploding EP rows | **done** | `tests/ep_key.rs`, `src/effects.rs` |
| No board write | **done** | `tests/ep_key.rs`, `src/effects.rs` |
| Benchmark rows + double-push share | **done** | `examples/effect_bench.rs` |
| Docs / public API pin | **done** | `src/lib.rs`, `README.md`, `CHANGELOG.md`, `tests/public_api.rs` |
| Report | **done** | this file |

## Public API

Exact signature added:

```rust
impl EffectContext {
    /// The child summary and the child's Zobrist key under `mode`, from one
    /// derivation.
    #[must_use]
    pub fn post_move_and_key_with(
        &self,
        board: &Board,
        m: Move,
        mode: EnPassantMode,
    ) -> Option<(PostMove, u64)>;
}
```

`post_move_and_key` keeps its signature and is now exactly:

```rust
pub fn post_move_and_key(&self, board: &Board, m: Move) -> Option<(PostMove, u64)> {
    self.post_move_and_key_with(board, m, EnPassantMode::Always)
}
```

The crate-private helper is:

```rust
pub(crate) fn legal_ep_square_in_view(view: &PositionView) -> Option<Square>;
```

It mirrors `Board::legal_ep_square`: stored target present, both commoners
present, the expected enemy pawn on the victim square, and at least one
candidate accepted by `legal_with_counts`. The public surface stays the paired
query; `PositionView` does not become public and `PostMove::key_with` was not
added.

## Differential counts

`cargo test --test ep_key -- --nocapture` reports:

```text
ep_key: 15062 positions, 286079 pseudo-legal moves, 10443 double pushes,
        10443 stored ep children (973 legal, 9470 removed)
```

The walk covers the 41-position perft corpus, the effect fixtures, the frozen
`CONFORMANCE_CASES`, `CHILD_CASES`, and the shared seeded play-outs
(`tests/common/mod.rs`: 6 play-out starts × 16 seeds × 300 plies plus 2 seeds ×
60 plies from every corpus position).

For every candidate and both modes the test asserts:

```text
post_move_and_key_with(&b, m, mode).1
    == b.hash_after_with(m, mode)
    == { let mut c = b.clone(); c.do_move(m, &mut st); c.hash_with(mode) }
post_move_and_key_with(&b, m, Always) == b.post_move_and_key(m)
post_move_and_key_with(&b, m, mode).0.rule50() == child.rule50()
```

Malformed input (`Move::NONE`, empty origin, enemy origin) is `None` for both
modes. Every explicit fixture also asserts the clone oracle, not just the other
internal helper. The board's FEN and hash are recorded before each position and
asserted unchanged after all queries.

`src/position.rs` additionally checks `legal_ep_square_in_view` against
`Board::legal_ep_square` for the corpus, fixtures, every child, and seeded
play-outs.

## Benchmarks

Command, three runs, medians:

```sh
cargo run --release --example effect_bench 15 200000 20000
```

The higher iteration count was used after an initial `9 2000` set showed enough
run-to-run noise to move the aggregate `all` ratio by several percent; the
baseline was extracted from `HEAD` with `git archive` into `/tmp` and run with
the same command, so before/after rows are comparable. Corpus: **49,338 children
over 2,106 nodes** (41 perft positions + 3 bench positions, 2 seeds × 24 plies).

### Key rows, ns per child (ratio to the class's `build` row)

| class | children | build | `post_move_and_key` | `with(Always)` | `with(Legal)` | `clone+do_move+hash_with(Legal)` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| quiet | 35,412 | 10.4 | 17.7 (1.70×) | 17.9 (1.72×) | 17.8 (1.71×) | 11.3 (1.09×) |
| capture | 3,972 | 17.7 | 21.2 (1.20×) | 21.2 (1.20×) | 21.3 (1.20×) | 18.0 (1.02×) |
| promotion | 432 | 10.9 | 17.5 (1.61×) | 17.5 (1.61×) | 17.6 (1.61×) | 11.2 (1.03×) |
| en-passant | 8 | 13.7 | 17.8 (1.30×) | 17.8 (1.30×) | 17.9 (1.31×) | 14.2 (1.04×) |
| castling | 80 | 8.6 | 18.1 (2.10×) | 18.2 (2.12×) | 18.3 (2.13×) | 8.4 (0.98×) |
| commoner-move | 10,052 | 10.5 | 18.0 (1.71×) | 18.0 (1.71×) | 18.0 (1.71×) | 10.8 (1.03×) |
| **built-ish** | 13,926 | 12.9 | 19.8 (1.53×) | 19.8 (1.53×) | 19.8 (1.53×) | 13.3 (1.03×) |
| all | 49,338 | 11.3 | 19.0 (1.68×) | 18.8 (1.66×) | 18.9 (1.67×) | 12.1 (1.07×) |

### Double pushes

The dedicated section reports:

```text
double pushes: 2456 of 49338 children (4.98 %);
2456 store an ep target (164 legal, 2292 removed)
```

That 4.98 % is the wishlist's decisive share on this corpus; 6.68 % of the
stored targets have a fully legal capture, so **93.3 % of double pushes are
`Legal`-sensitive removal cases** (2,292 of 2,456).

| row | ns per double-push child | ratio to key | ratio to build |
| --- | ---: | ---: | ---: |
| build (`clone`+`do_move`) | 10.8 | 0.55× | 1.00× |
| `post_move_and_key` | 19.8 | 1.00× | 1.83× |
| `post_move_and_key_with(Always)` | 19.8 | 1.00× | 1.83× |
| `post_move_and_key_with(Legal)` | **20.3** | 1.03× | 1.88× |
| `clone+do_move+hash_with(Legal)` | **19.8** | 1.00× | 1.83× |

### Acceptance table

| Gate | Result |
| --- | --- |
| `with(Always)` vs existing `post_move_and_key` ≤ 1.02× | **pass**: `all` 18.8 / 19.0 = **0.99×**; double pushes 19.8 / 19.8 = **1.00×** |
| `with(Legal)` on non-double-push children vs `Always` ≤ 1.02× | **pass**: per-class ratios 0.99–1.006; `all` 18.9 / 18.8 = **1.006×** |
| `with(Legal)` vs `clone+do_move+hash_with(Legal)` on double pushes must be cheaper | **not met**: 20.3 vs 19.8 ns = **1.025× slower** (statistical parity / slightly worse in this run) |
| double-push share of the child corpus | **4.98 %** (2,456 / 49,338), 164 legal-EP hits, 2,292 removals |
| existing child-path rows no regression > 2 % | **pass**: see table below; largest change is `build` +0.8 % / +0.0 % |
| perft timing no regression > 2 % | **pass**: interleaved `verify_perft 5` medians 1.554 s (HEAD) → 1.559 s (after), **+0.3 %** |

The non-double-push row is measured through the class table: no non-double-push
child stores an ep target, so the `Legal` branch is a predicate check and an
untaken helper call. The per-class ratios above are all within the 1.02× gate.

### Existing child-path rows (before → after, medians)

| row | `built-ish` | ratio | `all` | ratio |
| --- | ---: | ---: | ---: | ---: |
| build (`clone`+`do_move`) | 12.8 → 12.9 | 1.008× | 11.3 → 11.3 | 1.000× |
| today (`clone`+`do_move`+outcome+key) | 33.5 → 33.1 | 0.988× | 30.9 → 30.6 | 0.990× |
| mg15 (`hash_and_rule50_after`+`terminal_after`) | 28.0 → 27.6 | 0.986× | 30.5 → 30.1 | 0.987× |
| summary (`post_move`) | 14.6 → 14.1 | 0.966× | 13.2 → 13.1 | 0.992× |
| mg19 (`post_move`+`terminal`) | 24.0 → 23.6 | 0.983× | 27.2 → 26.7 | 0.982× |
| stack (`post_move_and_key`+terminal) | 34.5 → 29.7 | 0.861× | 38.0 → 33.2 | 0.874× |
| exist (`has_legal_move_after`) | 23.7 → 23.0 | 0.970× | 26.7 → 26.5 | 0.993× |
| two-level (`child_outcome`) | 28.7 → 27.8 | 0.969× | 30.3 → 29.2 | 0.964× |

The existing row code is untouched; the measured changes are noise plus code
layout. No row regressed beyond 2 %.

### Perft

`cargo run --release --example verify_perft 5`, five interleaved HEAD/after
pairs: 41/41 each; medians **1.554 s → 1.559 s (+0.3 %)**. Depth 6 also ran
41/41 (46.1 s). The generator and legality ladder were not changed.

## Verification run

- `cargo test` — all unit, integration, and 17 doctests pass.
- `cargo test --test ep_key --test post_move --test conformance --test public_api`
  — pass.
- `cargo test --test move_order_golden --test verify_moves --test effects
  --test terminal_after --test child_state --test interchange` — pass.
- `cargo test --features unsafe-optimizations` — pass.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` — clean.
- `cargo run --release --example verify_perft 5` ×3 and `6` — 41/41.
- `cargo run --release --example effect_bench 15 200000 20000` ×3 — table above.

## Changed files

- `src/effects.rs`
  - `child_key_with` (mode-aware wrapper around the unchanged `child_key`);
  - `EffectContext::post_move_and_key_with`;
  - `post_move_and_key` delegates to the `Always` spelling;
  - docs and unit tests for the EP rows, malformed input, and the exact
    removed ep-file key.
- `src/position.rs`
  - `legal_ep_square_in_view`;
  - corpus/child/play-out differential against `Board::legal_ep_square`.
- `tests/ep_key.rs` (new)
  - clone-oracle differential over corpora, fixtures, conformance/child cases,
    and seeded play-outs;
  - explicit EP fixtures;
  - malformed-input and no-board-write checks.
- `examples/effect_bench.rs`
  - `Key`, `KeyAlways`, `KeyLegal`, `CloneHashLegal` rows;
  - the per-position mode rows;
  - the double-push share and dedicated double-push table.
- `src/lib.rs` — child-key docs and allocation-list pin.
- `README.md` — additive API and mode contract.
- `CHANGELOG.md` — `[Unreleased]` entry (no version bump; plan 5 owns the
  release version).
- `tests/public_api.rs` — public pin for `post_move_and_key_with`.
- `docs/plans/feedback/report_wishlist1_1.md` — this report.

## Problems encountered

1. **The clone-free shape does not produce a measurable double-push win on this
   corpus.** The legal-EP question is cheaper than `Board::legal_ep_square`
   (no `StateInfo` population), but the clone path's `do_move` is already
   cheap, and the summary derivation plus the legal-EP query together land at
   parity or slightly above it. This is reported rather than hidden; the plan
   explicitly permits it and leaves adoption downstream.
2. **Benchmark noise.** The first `9 2000` runs moved the aggregate ratios by
   several percent and made one existing child row look ~17 % slower. Running
   the baseline and the change with `15 200000 20000` and comparing medians
   removed that artifact; the existing-row table above is from the low-noise
   pair. The low-iteration numbers are not used for acceptance.
3. **Examples cannot see crate-private items.** The benchmark's Legal-sensitive
   share uses the cloned child's public `Board::legal_ep_square()` rather than
   `position::legal_ep_square_in_view`; the tests (inside the crate) pin the
   two to the same answer.

## Deviations from the plan

1. The differential was added as a new `tests/ep_key.rs` rather than appended to
   `tests/post_move.rs`; the plan allows either and the existing `Cargo.toml`
   uses automatic test discovery, not an explicit list.
2. The benchmark's double-push rows are a separate table and a separate share
   line, not a new `CHILD_CLASSES` member. A double push is also a `quiet` or
   `capture` child and often a pawn move, so adding it to the disjoint class
   table would double-count. The separate table keeps every existing row and
   label unchanged.
3. `CHANGELOG.md` uses `[Unreleased]` instead of taking the 2.9.0 version. The
   index makes plan 5 the only place that chooses the release version.
4. `tests/public_api.rs` gained the new method pin although the plan listed
   only docs/README. It is the crate's existing guard against release-note
   drift.
5. In `tests/ep_key.rs`, the exact XOR with the private Zobrist table is not
   asserted from the integration test; the unit test in `src/effects.rs` does
   assert `always ^ legal == ZOBRIST.ep[file]` on the removal rows. The
   integration test still asserts the clone and `hash_after_with` equalities,
   which are the acceptance contract.

## Unresolved parts / next steps

- **Adoption is not decided here.** The downstream must measure its own wall
  clock. On this corpus the double-push row is `Legal` 20.3 ns vs clone 19.8 ns,
  so the clone-removal argument is not supported by these numbers.
- **`Board::hash_after_with(Legal)` was not refactored.** It still clones for a
  double push. A later plan could delegate it to `legal_ep_square_in_view` and
  measure the board convenience path directly; this plan deliberately left it
  as the clone oracle. Given the parity above, that is a cleanup candidate, not
  an obvious win.
- **No standalone `PostMove::key_with`.** The paired query is the shipped API;
  no consumer need for a summary-only key was measured or requested.
- **No change to the key table or `ZOBRIST_VERSION`.** `conformance` still
  asserts version 1 and no stored key or corpus value was regenerated.
- **Defensive helper branches.** The missing-victim and missing-commoner
  branches of `legal_ep_square_in_view` mirror the board method but are not
  reachable from a valid `Board` with an ep target; no test constructs an
  intentionally inconsistent `PositionView` to hit them.
- **Double-push share is corpus-specific.** It is 4.98 % here (2,456 / 49,338);
  the downstream tree may differ, which is exactly why adoption is left to its
  wall measurement.
