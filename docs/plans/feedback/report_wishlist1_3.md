# Implementation report — wishlist 1 plan 3 (W3: immediate-explosion query)

Status: **implemented.** `EffectContext::exploding_capture_after(board, m)`
returns the first legal capture by the child's side to move whose blast removes
the commoner of the side that played `m`. The query is read-only, reuses the
already-derived child view, and is differentially checked against an exhaustive
search over the built child's legal moves. The cost and hit rate are measured
and published below; no downstream guide integration or wall-clock win is
claimed.

No dependency was added, no `unsafe` was added, and no hot path
(`Board::legal`, `populate_state`, the generator, or the legality ladder) was
changed.

## Item status

| Plan item | Status | Where |
| --- | --- | --- |
| `EffectContext::exploding_capture_after` | **done** | `src/effects.rs` |
| Crate-private `exploding_capture_in_view` | **done** | `src/effects.rs` |
| Both-commoners pre-check and total `None` cases | **done** | `exploding_capture_in_view` |
| Destination-zone prefilter plus decisive blast test | **done** | `exploding_capture_in_view` |
| Public docs, module "child queries" docs, doctests | **done** | `src/effects.rs`, `src/lib.rs` |
| README / CHANGELOG / public-API pin | **done** | `README.md`, `CHANGELOG.md`, `tests/public_api.rs` |
| Exhaustive child oracle | **done** | `tests/explosion.rs` |
| Differential over corpus, fixtures, conformance and play-outs | **done** | `tests/explosion.rs` |
| Named fixtures | **done** | `tests/explosion.rs`, `src/effects.rs` unit tests |
| No-board-write checks | **done** | `tests/explosion.rs` |
| Benchmark rows: query, oracle, hit rate by class | **done** | `examples/effect_bench.rs` |
| Report | **done** | this file |

`src/position.rs` was not changed: `legal_with_counts`, `PositionView`, and the
view-side generator were already crate-private and sufficient.

## Public semantics as implemented

Let `us` be the parent's side to move (the side that played `m`), and let
`them = us.flip()` be the child's side to move. The method returns a move `c`
such that:

1. `c` is a legal move for `them` in the child position;
2. `c` is a capture (`PositionView::is_capture`, en passant included);
3. the blast of `c` removes at least one of `us`'s commoners in the child.

The target is therefore the **mover's** commoner, read from the child after the
move. If `m` moved the commoner, the target is its new square.

The result is the first such capture in the child's **pseudo-legal generation
order** that passes legality. Both the implementation and the oracle use that
order, so the differential asserts strict equality of the returned move, not
only equal existence. The query is an existence/one-witness query, not a "best
capture" query.

`None` is returned when:

- `m` is malformed (`Move::NONE`, an empty origin square, or an origin that does
  not hold a piece of the parent side to move);
- the child has no commoner for the side that played `m` (for example `m`
  exploded it);
- the child has no commoner for the child side to move (the child is already
  decided);
- no legal capture satisfies conditions 1–3.

The query is defined for **pseudo-legal** `m`, including illegal `m`. The
unreachable child is described exactly as `do_move` would build it. Touching
commoners do not suppress the query by themselves: the capture must still be
legal, so a blast that destroys the child side's own commoner is rejected, while
a blast that removes only the target commoner is a win and may be returned.

## Implementation notes

`exploding_capture_in_view(view, target)` first rejects a child where either side
has no commoner, then rejects an empty target. That pre-check is required by
`legal_with_counts`, which expects a live commoner for the side to move and has
no game-over guard of its own.

The walk is the existing
`movegen::for_each_pseudo_legal_in_view` with an early `Break`. Each candidate
is filtered by `view.is_capture`, then by
`target_zone = target | attacks::king_attacks(target.lsb())`, then decisively by
`(view.blast_casualties(c).all() & target).is_empty()`. Only a capture that
survives those filters pays for `position::legal_with_counts(view, c)`. The
destination-zone test is a prefilter only; it cannot become the rule.

No board is cloned, no `do_move` is called, no `StateInfo` is populated, and no
public `PositionView` or new public type is exposed.

## Differential results

`tests/explosion.rs` compares the query against this oracle:

```text
child = clone + do_move(m)
target = child.commoners(parent mover)
if target empty or child.commoners(child side) empty: None
for c in generate_legal_with_state(child) in generation order:
    if child.is_capture(c) and child.blast_casualties(c).all() & target != EMPTY:
        return Some(c)
None
```

The committed run covered:

| Quantity | Count |
| --- | --- |
| Positions visited | 15,056 |
| Named/conformance/child fixture FENs | 55 (the 17 named fixtures plus the conformance and child-case FENs, de-duplicated) |
| Perft corpus positions | 41 (visited by `walk_corpus_and_fixtures`) |
| Pseudo-legal moves checked | 285,960 |
| Oracle rows | 285,960 |
| Query hits (`Some`) | 51,509 |
| Both-hit rows (`Some` on both sides) | 51,509 |
| Disagreements in existence or returned move | **0** |

The play-outs use `tests/common/mod.rs` (`walk_playouts`): six start positions ×
16 seeds × up to 300 plies, plus two seeds × up to 60 plies from each perft
corpus position. Atomic games terminate early, so the actual number of visited
positions is 15,056, not the maximum ply budget.

For every pseudo-legal move the differential checks:

- strict equality `actual == expected` (same first hit, not merely both `Some`);
- when `Some`, membership of the returned move in the built child's legal list;
- the parent board's FEN and hash are unchanged after the query.

`queries_do_not_write_the_board` additionally checks FEN and hash after every
query over four move-rich fixtures, including en passant, castling, and the
explosion fixture.

## Named fixtures

All rows assert the query/helper and the oracle in the same test.

| Fixture | Expected | Where |
| --- | --- | --- |
| Direct commoner capture: child `4k3/4r3/8/8/8/8/8/4K3 b - - 0 1` | `e7e1` | `src/effects.rs` child-view unit test |
| Capture beside the commoner: child `5r2/8/8/8/3kK3/5R2/8/8 b - - 0 1` | `f8f3` | `src/effects.rs` child-view unit test |
| Same shape from a parent: `5r2/8/8/8/3kK3/8/5R2/8 w - - 0 1`, `f2f3` | `f8f3` | `tests/explosion.rs` |
| En-passant explosion: `k7/8/8/8/3p1K2/8/4P3/8 w - - 0 1`, `e2e4` | `d4e3`, `MoveType::EnPassant` | `tests/explosion.rs` |
| Touching commoners, only a self-exploding candidate: child `8/8/8/8/3kK3/3q4/8/8 b - - 0 1` | `None` | `src/effects.rs` child-view unit test |
| Pawn is not a target: child `5r2/8/8/8/3k4/5P1K/8/8 b - - 0 1` | `None` | `src/effects.rs` child-view unit test |
| Mover self-explodes: `k7/8/8/8/8/8/8/3qK3 w - - 0 1`, `e1d1` | `None` | `tests/explosion.rs` |
| Child side already extinct: `4k3/8/8/8/8/8/4R3/4K3 w - - 0 1`, `e2e8` | `None` | `tests/explosion.rs` |
| No capture near the commoner: initial position, `e2e4` | `None` | `tests/explosion.rs` |
| Malformed: `Move::NONE`, empty origin, enemy origin | `None` | `tests/explosion.rs` |

## Benchmark

`cargo run --release --example effect_bench 9 2000` was run three times; the
table reports the median of the three per-class medians. The child corpus is
seeded play-outs from the 41-position perft corpus plus the three bench
positions (2 seeds × 24 plies): 2,106 nodes and 49,338 pseudo-legal children.
The W3 rows are a dedicated table so the established per-class rows keep their
exact previous code and measurement order.

| Class | Children | Hits | Hit % | Query ns | Oracle ns | Query/build | Query/oracle |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| quiet | 35,412 | 4,982 | 14.1 | 79.5 | 190.0 | 4.96× | 0.42× |
| capture | 3,972 | 536 | 13.5 | 85.0 | 173.6 | 2.87× | 0.49× |
| promotion | 432 | 56 | 13.0 | 53.6 | 165.5 | 3.18× | 0.32× |
| en-passant | 8 | 2 | 25.0 | 50.1 | 148.5 | 1.92× | 0.34× |
| castling | 80 | 32 | 40.0 | 69.9 | 213.3 | 4.61× | 0.33× |
| commoner-move | 10,052 | 2,812 | 28.0 | 62.9 | 156.0 | 3.87× | 0.40× |
| built-ish | 13,926 | 3,390 | 24.3 | 72.2 | 172.0 | 3.50× | 0.42× |
| all | 49,338 | 8,372 | 17.0 | 80.6 | 186.4 | 4.68× | 0.43× |

The "oracle" is the plan's exhaustive shape: `clone` + `do_move` + populate +
`generate_legal_with_state` + capture/blast filter. On this machine the query is
about 2.0–3.0× cheaper than that oracle, but still 1.9–5.0× more expensive than
building the child with nothing read (`build` = `clone` + `do_move`, 15–30 ns
depending on class).

The measured hit rate is real, not a synthetic property: quiet moves have a
14.1 % chance that the child side can immediately explode the mover, and
commoner moves 28.0 %. The aggregate is 17.0 % over all pseudo-legal children.
Small classes (8 en-passant, 80 castling, 432 promotion children) have noisy
absolute ns; the aggregate classes are the stable rows.

The oracle is faster than expected relative to older child queries because it is
one legal generation over a small child, but it is still roughly twice the
query cost. A capture-only/destination-mask generator could remove the quiet
scan, but this plan did not add one because the measured query already beats the
oracle and the plan's gate is "measure first, specialize only with a measured
reason".

## Regression check

The existing child-path rows were compared against a clean `HEAD` build
(`git archive HEAD` into `/tmp`, same command, three runs each). No existing row
implementation changed: `diff -u` shows only the new oracle helper, the new
dedicated table, and its call.

| Class | Mean absolute delta of existing rows | Worst row |
| --- | ---: | --- |
| quiet | 1.54 % | `key-always` −4.62 % |
| built-ish | 1.52 % | `two-level` −3.77 % |
| all | 2.13 % | `summary` −4.95 % (1.0 ns absolute) |

These are run-to-run noise on rows of a few nanoseconds to a few tens of
nanoseconds; the largest absolute difference is 1.0 ns. The new explosion table
runs after the established per-class table and does not interleave with it.
No perft or movegen code was changed; `verify_perft 5` remains 41/41 and the
hot-path source diff is empty.

## Changed files

| File | Change |
| --- | --- |
| `src/effects.rs` | `exploding_capture_in_view`, `EffectContext::exploding_capture_after`, module docs and doctest, child-view fixture unit test |
| `src/lib.rs` | crate-level docs for the new child query and its API index entry |
| `README.md` | feature bullet for the one-ply child threat query |
| `CHANGELOG.md` | Unreleased `Added` entry |
| `tests/public_api.rs` | public-API pin for `exploding_capture_after` |
| `tests/explosion.rs` | exhaustive oracle, differential, named fixtures, malformed cases, no-board-write tests |
| `examples/effect_bench.rs` | dedicated per-class explosion table: query, oracle, hit rate |
| `docs/plans/feedback/report_wishlist1_3.md` | this report |

## Problems and deviations

1. **The plan's helper used one target square; the implementation keeps that
   shape.** It is exact for the crate's positions because `Board::from_fen`
   rejects more than one commoner per side. A multi-commoner `PositionView`
   cannot be produced through the public board API. If a future non-board view
   could have several targets, the prefilter should loop over all target bits;
   the decisive blast test already intersects the full `target` bitboard.
2. **The direct-commoner and touching-commoner fixtures are tested as child
   views.** The plan permits this: those rows start from an already-built child,
   so the crate-private helper is the query and the same test runs the
   exhaustive legal-list oracle on that child. The public parent/move shape is
   covered separately by the capture-beside-the-commoner and en-passant rows.
3. **The benchmark table is separate, not added to the existing per-class
   `Row` enum.** Adding two heavy rows to the existing interleaved table changed
   the measured cache behaviour of unrelated rows by up to ~13 % on small
   classes even though no library code changed. Keeping the W3 rows in their own
   enum and table leaves the established child-path measurements byte-for-byte
   unchanged. The plan asks for the fact and its cost; it does not require the
   rows to share the same table.
4. **The query is faster than the oracle on every class, but not cheaper than
   building the child.** This is the honest measurement, not a claim of a
   downstream win. W3 only asks for the fact and its cost before any swap.
5. **No specialized capture walk was added.** The exact generic pseudo-legal
   walk passes the differential and already beats the oracle. The optional
   `for_each_pseudo_legal_capture_to_in_view` optimization is left for a
   follow-up if a downstream wall measurement says the remaining ~63–80 ns per
   child matters.

## Unresolved parts and missing tests

- **No downstream `Mover::exposes` differential was available.** The public
  semantics pin the target as the mover's commoner, matching the plan's
  interpretation of the wishlist. If the downstream intended the opposite
  target, its own differential must expose that before integration. The
  upstream definition is not ambiguous in the docs or tests.
- **No multi-commoner test.** Public FEN parsing rejects such positions, so the
  case is not reachable through the documented board API; the code documents
  the single-target assumption.
- **No mutation/fuzz harness beyond the play-out differential.** The corpus is
  the same one the other effect-path tests use; it is not an exhaustive table of
  every atomic capture geometry.
- **No `cargo test --release` run for this report.** The debug differential and
  the release benchmark/perft runs cover the tree; the plan did not require a
  release test run.

## Verification commands run

```sh
cargo test
cargo test --test explosion -- --nocapture
cargo test --test post_move --test effects --test child_state --test explosion --test public_api
cargo clippy --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo test --doc
cargo run --release --example verify_perft 5        # 41/41, depth 5
cargo run --release --example effect_bench 9 2000  # three runs, medians above
```

The baseline comparison used `git archive HEAD` into `/tmp/baseline_atomic` and
the same three-run `effect_bench 9 2000` command.

## Next steps

- The downstream can now use `EffectContext::exploding_capture_after` for the
  one-ply "does the mover expose its commoner?" question and remove its second
  rules implementation, or keep `Mover::exposes` until its own wall measurement
  says the swap is profitable.
- If the downstream measures the remaining query share as material, add the
  planned capture-only/destination-mask walk behind the same differential. It
  must preserve the first-hit order for captures and must not become a second
  rules implementation.
- If a future consumer wants `Board::exploding_capture_after(m, &state)`, it is
  an additive wrapper with the same body; this plan deliberately did not add
  one.
