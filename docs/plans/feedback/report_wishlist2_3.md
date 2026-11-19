# Implementation report — plan 3 (W8: effect and child key from parent bitboards)

Status: **landed** as additive API. Field-for-field exactness, the shared-key
requirement, and the plan's cheaper-than-`PostMove` benchmark gate pass. W8's
absolute instruction ceiling (≤ 120 per call) is **not reached** on this host:
the final probe is ~189.5 net instructions per `child_effect(Always)` call
(~211.3 under `Legal`), against a measured `Board::hash_after` cost of ~138.4
on the same probe. The item stays recorded as open for that absolute number;
the new API is kept because it is materially cheaper than the existing
`PostMove` key path in both instructions and wall time.

Plan: `docs/plans/feedback/plan_wishlist2_3.md`.
Wishlist item: W8 (P1). Plan 5 owns the release version and notes.

## Plan validation

The current-tree assessment matched the code:

- `EffectContext::post_move_and_key_with` derived a full child `PositionView`
  through `analyze`, then called the incremental `child_key_with`;
- `child_key` was already an XOR delta from the parent hash;
- `Board::blast_casualties` already produced the removal set without a board
  write;
- `position::castling_rights_after` and `position::legal_ep_square_in_view`
  were the shared rights and legal-EP helpers;
- no lean `ChildEffect` existed.

The plan's API shape, `counts` guard, `Always`/non-double-push no-view rule,
and one-key-implementation requirement were all implementable as written. The
only placement choice was to keep `ChildEffect` and the two `Board` methods in
`src/effects.rs` beside the existing child implementation rather than adding a
second block in `src/board.rs`; they are re-exported from the crate root.

## What landed

### Public API

```rust
// atomic_movegen::ChildEffect (also atomic_movegen::effects::ChildEffect)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChildEffect {
    pub occupied: Bitboard,
    pub key: u64,
    pub rule50: u16,
    pub capture: bool,
}

impl Board {
    pub fn child_effect(
        &self,
        m: Move,
        counts: Counts,
        mode: EnPassantMode,
    ) -> Option<ChildEffect>;

    pub fn child_effect_with_occupancy(
        &self,
        m: Move,
        occupied: Bitboard,
        counts: Counts,
        mode: EnPassantMode,
    ) -> Option<ChildEffect>;
}
```

`ChildEffect` is `Copy + Send + Sync`, has no private state, and is additive.
`child_effect` computes the parent occupancy and delegates to
`child_effect_with_occupancy`. `occupied` is documented as a hot-path
precondition equal to `self.occupied()`; the intended producer is
`LegalityContext::occupied()`.

### Shared key arithmetic

`src/effects.rs` now has one incremental key helper:

```rust
fn child_key_from_parts(
    board: &Board,
    parent: &PositionView,
    m: Move,
    is_capture: bool,
    casualties: BlastCasualties,
    child_castling_rights: u8,
    child_ep: Option<Square>,
) -> u64;
```

`child_key` is a thin adapter over it using the existing `PostMove` fields, and
`Board::child_effect` supplies the same values from the lean derivation. The XOR
composition, piece-key table, castling/EP transitions, and empty-key no-op are
unchanged. There is no second copy of the key arithmetic.

### Lean child derivation

- Validation rejects `Move::NONE`, an empty origin, and an origin without a
  piece of the side to move. Well-formed illegal moves are defined and describe
  the child `do_move` would build.
- The moving piece is read once from the board; capture and castling are
  classified once.
- Quiet moves do not call `blast_casualties`.
- Child occupancy is derived directly: castling relocations, capture removal
  (`from ∪ casualties`), or a normal relocation.
- Child clock follows `Board::rule50_after`: castling increments; a pawn move or
  capture resets; everything else saturating-increments.
- Child rights use the castling clear or the shared `castling_rights_after`
  with the six post-blast home-square pieces.
- A child ep target is stored only for a two-rank pawn move.

### Child-view cost rule

`Always` never builds a child `PositionView`. `Legal` enters the view/legal-EP
branch only when `child_ep` is `Some`, and that value is assigned only for a
two-rank pawn move. All non-double-pushes and all `Always` calls therefore stop
after the incremental derivation. When either parent count is zero, the
counts short-circuit also skips the view and still removes the ep-file key,
matching the child's `Legal` key.

## Differential counts

New suite: `tests/child_effect.rs`. For every pseudo-legal candidate and both
`EnPassantMode` values it checks:

```text
effect.occupied == { clone; do_move }.occupied()
effect.key      == child.hash_with(mode)
effect.key      == board.hash_after_with(m, mode)
effect.key      == ctx.post_move_and_key_with(board, m, mode).1
effect.rule50   == child.rule50()
effect.rule50   == board.rule50_after(m)
effect.rule50   == post_move(m).rule50()
effect.capture  == post_move(m).is_capture()
child_effect_with_occupancy(m, board.occupied(), ..) == child_effect(m, ..)
```

The parent `fen()`, `hash()`, ep target, clock, and game ply are re-checked after
all queries. Corpora are the 41-position perft corpus, `conformance::CASES`,
`conformance::CHILD_CASES`, the effect/board fixtures, edge fixtures, and the
seeded play-outs from `tests/common/mod.rs`.

| Corpus | Positions | Candidates | Modes | Captures | Double pushes | Stored ep | Legal ep | Removed ep |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| deduplicated corpora/fixtures | 102 | 2,155 | 2 | 205 | 133 | 133 | 11 | 122 |
| seeded play-outs | 14,960 | 283,924 | 2 | 20,566 | 10,310 | 10,310 | 962 | 9,348 |

That is 286,079 candidates and 572,158 mode rows in total, plus the named
fixtures, malformed-input test, well-formed-illegal test, and public API pin.

Named fixtures cover: double push with a legal ep capture; double push with no
adjacent enemy pawn; pinned ep; self-exploding ep; an actual ep capture;
castling; promotion; and a terminal parent (zero count) with a double push.
Every row compares against a cloned `do_move` child, not only the other helper.

## Benchmark

Harness: `examples/hotpath_bench.rs`, extended with child rows and the
`promotion`/`double-push` classes. Command:

```sh
cargo run --release --example hotpath_bench 15 16
```

Each row is the median of 15 batches. A call is one move. `effect-A`/`effect-L`
are `Board::child_effect`; `effect-occ` uses `LegalityContext::occupied()`;
`post-A`/`post-L` are the existing `post_move_and_key_with`; `clone-A`/`clone-L`
are `clone + do_move + hash_with(mode)`. Absolute ns are same-host only.

### Corpus (77 nodes, 1,724 moves, 117 double pushes — 6.79 %)

| class | calls | effect-A | effect-L | effect-occ | post-A | post-L | clone-A | clone-L |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| quiet | 1,115 | 10.32 | 10.45 | 10.34 | 26.64 | 39.65 | 14.75 | 14.86 |
| capture | 132 | 20.11 | 20.09 | 20.08 | 32.87 | 44.75 | 29.34 | 30.52 |
| promotion | 48 | 14.68 | 14.70 | 14.55 | 31.59 | 43.99 | 19.21 | 20.43 |
| en-passant | 1 | 19.07 | 19.59 | 18.85 | 29.95 | 40.19 | 27.12 | 26.42 |
| castling | 6 | 11.97 | 11.97 | 11.98 | 23.64 | 37.32 | 13.07 | 13.14 |
| double-push | 117 | 10.47 | 30.94 | 10.41 | 23.65 | 38.92 | 14.76 | 26.27 |
| commoner-move | 338 | 11.02 | 11.08 | 10.94 | 26.73 | 40.13 | 15.69 | 15.64 |
| all | 1,724 | 11.51 | 13.30 | 11.47 | 27.82 | 40.77 | 16.31 | 17.11 |

### Seeded play-outs (38,700 nodes, 835,794 moves, 27,006 double pushes — 3.23 %)

| class | calls | effect-A | effect-L | effect-occ | post-A | post-L | clone-A | clone-L |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| quiet | 539,154 | 10.65 | 10.85 | 10.48 | 26.89 | 39.53 | 15.32 | 15.49 |
| capture | 63,928 | 26.42 | 26.73 | 26.40 | 46.50 | 56.19 | 30.38 | 30.42 |
| promotion | 7,768 | 12.18 | 12.43 | 11.99 | 30.84 | 40.10 | 17.32 | 17.78 |
| en-passant | 116 | 22.81 | 22.63 | 22.68 | 31.82 | 42.40 | 30.19 | 29.39 |
| castling | 934 | 14.63 | 14.78 | 14.77 | 27.67 | 38.83 | 16.66 | 16.69 |
| double-push | 27,006 | 12.07 | 34.38 | 12.08 | 26.21 | 41.19 | 16.77 | 30.37 |
| commoner-move | 206,602 | 10.94 | 10.99 | 10.87 | 27.09 | 39.64 | 15.50 | 15.72 |
| all | 835,794 | 12.07 | 13.03 | 12.03 | 28.96 | 41.16 | 16.60 | 17.00 |

The all-row ratios in that run:

| Dataset | effect-A / post-A | effect-L / post-L | effect-A / clone-A | effect-occ / effect-A |
| --- | ---: | ---: | ---: | ---: |
| corpus | **0.414x** | **0.326x** | **0.706x** | 0.996x |
| play-outs | **0.417x** | **0.316x** | **0.727x** | 0.997x |

`Legal` is slower only on the double-push row, where it may construct the child
view for the legal-EP test; non-double-push `Legal` rows are effectively equal
to `Always`. Occupancy reuse from `LegalityContext` is **not** measurable on
this host (0.996–0.997x, within noise). The surface is retained because it is
the requested shape and avoids a re-read for callers that already hold the
occupancy, but the report does not claim a measurable win for it.

### Instruction diagnostic

`perf stat -e instructions:u` is available. A temporary example (removed after
the run) built 64 deduplicated perft/conformance positions with 1,382
pseudo-legal moves per pass, then ran 2,000 passes (2,764,000 calls per mode).
Commands were of the shape:

```sh
perf stat -e instructions:u target/release/examples/_instr_probe_child empty 2000
perf stat -e instructions:u target/release/examples/_instr_probe_child always 2000
perf stat -e instructions:u target/release/examples/_instr_probe_child legal 2000
perf stat -e instructions:u target/release/examples/_instr_probe_child hash_after 2000
```

| Row | Total instructions | Raw / call | Net of empty-loop overhead |
| --- | ---: | ---: | ---: |
| `empty` | 12,112,441 | 4.38 | — |
| `Board::hash_after` (context) | 394,409,087 | 142.69 | ~138.35 |
| `child_effect(Always)` | 535,940,779 | 193.90 | **~189.52** |
| `child_effect(Legal)` | 596,032,776 | 215.64 | **~211.26** |

The wishlist's ≤ 120 target is not met. The query is nevertheless below the
wishlist's quoted ~215 adapter cost and far below its ~394 batched cost, and it
is cheaper than `PostMove` in wall time. The report therefore claims the
measured shape, not the absolute ceiling. An early implementation that
computed the casualty OR for every non-castling move measured ~242 net; moving
that work into the capture branch is the final ~189.5 number above. A branch
that skipped the rights helper for quiet non-back-rank moves measured worse
(~209), so it was reverted.

## Existing-path regression checks

- No `Board::legal`, generator, `populate_state`, move-order, perft, FEN,
  terminality, `Always` key, or `ZOBRIST_VERSION` change.
- `child_key` was refactored to the shared parts helper; the existing
  `tests/ep_key.rs`, `tests/post_move.rs`, `tests/effects.rs`, and
  `tests/conformance.rs` suites stay green.
- The only changed hot-path-adjacent code is the new child query and the
  refactor of the existing `PostMove` key helper. `Board::hash_after` is
  untouched.
- The parent/perft rows of `hotpath_bench` show no regression beyond normal
  run-to-run noise; perft depth 5 is 41/41.

## Gate results

- `cargo test` — all suites pass; 191 unit tests, all integration suites,
  18 doctests.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo doc --no-deps` — no warnings; doctests pass.
- `cargo run --release --example verify_perft 5` — **41/41**.
- Targeted suites (`perft_tests`, `move_order_golden`, `verify_moves`,
  `conformance`, `effects`, `post_move`, `terminal_after`, `child_state`,
  `ep_key`, `null_for`, `public_api`, `interchange`, new `child_effect`) all
  pass through `cargo test`.
- `cargo test --features unsafe-optimizations` — all pass (the key helper is
  refactored, so this feature run was required).

## Changed files

- `src/effects.rs` — `ChildEffect`, `Board::child_effect`,
  `Board::child_effect_with_occupancy`, `child_key_from_parts`, docs.
- `src/lib.rs` — root re-export, hot-path docs, allocation-free list,
  `Send + Sync` assertion.
- `tests/child_effect.rs` — new differential, named EP/key, malformed/illegal,
  and capture-flag suite.
- `tests/public_api.rs` — public type/method pin and `Send + Sync`.
- `examples/hotpath_bench.rs` — child rows, promotion/double-push classes,
  double-push share, occupancy comparison.
- `README.md`, `CHANGELOG.md` — additive API, contract, measured status.
- `docs/plans/feedback/report_wishlist2_3.md` — this report.

## Problems encountered and deviations

1. **The zero-count `Legal` short-circuit initially skipped key removal.** The
   first implementation skipped the whole `Legal` adjustment when either count
   was zero, leaving the ep-file key in place. The child view's legal-EP helper
   returns `None` in that case, so the key must be removed. Fixed by using the
   same "no legal EP" removal branch with `legal_ep = None`; the named terminal
   fixture now pins it.
2. **Casualty work belongs inside the capture branch.** Computing
   `square_bb(from) | casualties.all()` for every non-castling move measured
   ~242 net instructions; moving it into the capture branch produced the final
   ~189.5. The shared key helper keeps its general shape.
3. **A quiet-rights skip was measured and reverted.** Skipping
   `castling_rights_after` for quiet moves away from both back ranks measured
   worse (~209 net) because of the extra branch/code shape. The simpler shared
   helper call is retained.
4. **Methods live in `src/effects.rs`.** The plan allowed either module; keeping
   the child query beside `PostMove`/`EffectContext` made the shared helper and
   docs local. No `src/board.rs` change was needed.
5. **The absolute instruction target is not portable and was not met.** The
   wall-time and relative gates are reported; the ≤ 120 number is recorded only
   as a host diagnostic and remains open.

## Unresolved parts and missing tests

- W8's ≤ 120 instruction acceptance is open. The API lands because it passes
  the plan's cheaper-than-`PostMove` gate and exactness gates.
- `child_effect_with_occupancy` shows no measurable win on this host. It is
  kept as the requested candidate-loop shape; a future caller with a cold
  occupancy load may still benefit.
- Stale `Counts` is documented but not separately tested by mutating the board
  after building a context/value. Ordinary stale-`StateInfo` behaviour is the
  model.
- The no-child-view rule for `Always`/non-double-push is established by the
  code path and documented; there is no instrumentation test proving the view
  is not constructed.
- `Move::NONE` and malformed-move behaviour matches `post_move`; broader
  malformed encodings remain outside the contract.

## Next steps

- Plan 4 (`null_for_advanced`) is independent and can proceed.
- Plan 5 should pin `ChildEffect`, `Board::child_effect`, and
  `Board::child_effect_with_occupancy` if they remain in the release, and should
  describe W8 as a measured additive improvement, not as having met the
  wishlist's absolute instruction ceiling.
- If a later consumer needs a fused occupancy/key/clock path below the current
  instruction count, that is a new measured experiment; this plan does not
  duplicate the board's existing fused `hash_after` blast traversal to chase it.
