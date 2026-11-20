# Implementation report — plan 1 (evasion-aware `LegalityContext`, W10)

Status: **landed** as an additive, opt-in constructor, with a **conditional**
measurement verdict. The full differential is exact and the in-check quiet/all
rows improve by far more than the plan gate. In this run two fall-through class
rows on the corpus exceed the plan's strict 2% blocker (commoner moves 1.048x,
out-of-check-all 1.035x), and the in-check capture/commoner split rows exceed it
on seeded play-outs (1.06–1.07x). The code path for those rows is the same old
path plus one destination test; the data and caveat are recorded in §7 for plan
2 rather than hidden. No version bump, public-API pin, release text, commit,
tag, or publish was made here.

Plan: `docs/plans/feedback/plan_wishlist3_1.md`.
Index: `docs/plans/feedback/plan_wishlist3_index.md`.
Baseline: released 2.10.0 (`5083647`).

## 1. Plan validation

The current-tree assessment in the plan was checked before editing and matched:
`LegalityContext` was check-blind, `LegalNode` already stored `our_commoners`
and `occupied`, `StateInfo::checkers` was the source of truth, `between_bb` and
`king_attacks` were available, and the corpus already counted in-check nodes.
The proposed constructor and the prefilter/body split fit the existing code
without touching `Board::legal`, the generator, keys, move order, FEN or perft.

Two plan corrections were necessary:

1. **Knight/pawn fixture FENs.** The table's `.../8/P3K3` places the white pawn
   on a1, not a2, so the expected `a2a3` candidate does not exist. The tests use
   `4k3/8/8/8/8/5n2/P7/4K3 w - - 0 1` and
   `4k3/8/8/8/8/8/P2p4/4K3 w - - 0 1`, which exercise the intended a2a3 push.
2. **Second-line fixture.** The listed `b1d2` fixture tests a destination
   outside the slider's ray. An extra fixture,
   `4r1k1/8/8/8/8/8/8/r1B1K3 w - - 0 1`, tests `c1e3`: the destination is on
   the e8-rook's block ray, but the move uncovers the a1-rook's first-rank
   line. The mask must let it reach the shared body, which rejects it.

The plan's cited downstream plan-9 artifacts are still absent from this tree.
No downstream tree, `.pvr`, proof acceptance, or whole-binary saving is claimed.

## 2. Landed API and internal shape

```rust
impl Board {
    pub fn legality_context_with_state(
        &self,
        counts: Counts,
        state: &StateInfo,
    ) -> LegalityContext;
}
```

The new constructor reads only `state.checkers`, builds the node snapshot once,
and applies the same `playable` guard as `Board::legality_context(counts)`.
`Board::legality_context(counts)`, `LegalNode`'s legality body,
`LegalityContext::legal/occupied`, `StateInfo`, `Counts`, keys, FEN, terminality
and perft are unchanged.

The crate-private implementation keeps **one** stored bitboard per node:

```rust
struct EvasionMask {
    // Destinations that may still answer the check or need the shared body:
    // the slider block ray, all occupied squares, the commoner's neighbourhood,
    // and the stored en-passant target.
    pass: Bitboard,
}
```

- Empty `checkers`, a missing commoner, or touching commoners → the sentinel
  `pass = !0`, so no move is rejected.
- Exactly one slider checker and no touching commoners →
  `pass = between(commoner, checker) | occupied | king_attacks(commoner)`.
- A leaper or multiple checkers → `pass = occupied | king_attacks(commoner)`.
- The stored en-passant target is always added to `pass`.
- `LegalNode::legal(m)` rejects only when `!pass.contains(m.destination)`;
  everything else executes the existing single `legal_in` body.

This is a rejection-only prefilter. Expanding the block ray by occupied squares,
the commoner neighbourhood and the en-passant target can only *lose* rejections,
never accept a move. In particular castling falls through because its encoded
destination (the rook origin) is occupied; the shared body still rejects it in
check. The wishlist's “commoner square” field is not stored; the existing
`our_commoners` set supplies the lowest commoner during construction.

## 3. Correctness proof summary

A quiet non-commoner move changes occupancy only at its origin and destination.

- It cannot remove a checker (no capture, no blast) and does not move the
  commoner.
- A leaper check cannot be blocked.
- One slider check can only be answered by a destination strictly between the
  checker and the commoner; removing a piece cannot close a ray.
- Two checker rays meet only at the commoner, so one quiet destination cannot
  block both; any leaper in the set cannot be blocked at all.
- Quiet promotions are treated like quiet moves; a capturing promotion is on an
  occupied destination and falls through.
- Castling, en passant, captures, commoner moves, quiet moves outside check and
  every destination in `pass` still use the shared body.

Therefore every rejection is a move the existing body rejects. Stale checker
data can only over-reject, exactly like a stale `Counts`; the game-over guard is
still evaluated first.

## 4. Differential results

`cargo test --test legality_context -- --nocapture`:

### Corpus and fixtures

- 87 positions: 6 terminal, 16 in check, 71 not in check, 8 single-slider block
  positions, 8 empty-block/leaper/double positions, 7 pinned, 4 ep, 19 with
  castling rights.
- 1,715 candidates: 8 castling, 2 en passant, 1,388 legal.
- In check: 232 candidates (204 quiet, 27 capture, 0 ep, 1 castling, 73
  commoner); 125 predicted mask rejections; **0 legal among them**.

### Seeded play-outs

- 14,960 positions: 82 terminal, 518 in check, 14,442 not in check, 252
  single-slider block positions, 266 empty-block/leaper/double, 344 pinned, 354
  ep, 1,756 with castling rights.
- 283,924 candidates: 350 castling, 38 ep, 261,430 legal.
- In check: 9,544 candidates (8,822 quiet, 716 capture, 2 ep, 4 castling, 3,326
  commoner); 4,928 predicted mask rejections; **0 legal among them**.

Every candidate asserted

```text
evasion_ctx.legal(m) == ctx.legal(m)
                     == board.legal(m, &full)
                     == board.legal(m, &counts_only)
                     == board.is_legal_with_state(m, &counts_only)
```

and `evasion_ctx.occupied() == board.occupied()`. The focused fixtures also
cover one slider check inside/outside the block ray, double check, knight and
pawn check, touching commoners, castling in check, en passant in check, all four
promotion encodings that block, the outside-ray second-line move, and the
inside-ray second-line move. The robustness suite adds a current-checkers /
zero-counts case: the guard rejects every candidate without panic.

## 5. Benchmark

Command:

```sh
cargo run --release --example hotpath_bench 9 8
```

Median ns per call; ratios are vs. `Board::legal` in the same run. Setup rows are
ns per node.

### Corpus (77 candidate nodes, 1,724 candidates)

| Row | Board::legal | old ctx | evasion ctx | evasion/ctx | evasion/board |
| --- | ---: | ---: | ---: | ---: | ---: |
| setup | — | 2.27 | 2.50 | 1.101x | — |
| quiet | 7.86 | 7.38 | 6.81 | 0.923x | 0.867x |
| capture | 7.13 | 6.82 | 7.06 | 1.035x | 0.991x |
| en passant | 10.36 | 9.89 | 9.68 | 0.979x | 0.935x |
| castling | 10.12 | 10.27 | 10.30 | 1.003x | 1.017x |
| commoner move | 8.05 | 7.54 | 7.90 | 1.048x | 0.980x |
| all | 8.24 | 7.73 | 7.47 | 0.966x | 0.906x |

In-check split:

| Row | calls | Board::legal | old ctx | evasion ctx | evasion/ctx |
| --- | ---: | ---: | ---: | ---: | ---: |
| in-check quiet | 167 | 7.56 | 7.11 | **1.60** | **0.225x** |
| in-check capture | 27 | 6.28 | 5.98 | 6.05 | 1.012x |
| in-check castling | 1 | 5.60 | 5.81 | 5.69 | 0.978x |
| in-check commoner | 65 | 7.55 | 7.20 | 7.29 | 1.012x |
| in-check all | 252 | 7.29 | 6.89 | **3.49** | **0.507x** |
| out-of-check quiet | 1,113 | 8.02 | 7.54 | 7.65 | 1.015x |
| out-of-check all | 1,472 | 8.43 | 7.92 | 8.20 | 1.035x |

### Seeded play-outs (38,700 candidate nodes, 835,794 candidates)

| Row | Board::legal | old ctx | evasion ctx | evasion/ctx | evasion/board |
| --- | ---: | ---: | ---: | ---: | ---: |
| setup | — | 3.71 | 4.08 | 1.100x | — |
| quiet | 8.76 | 7.99 | 7.92 | 0.991x | 0.904x |
| capture | 9.79 | 8.88 | 8.99 | 1.012x | 0.918x |
| en passant | 10.68 | 10.13 | 10.43 | 1.030x | 0.977x |
| castling | 16.15 | 15.44 | 15.64 | 1.013x | 0.968x |
| commoner move | 10.39 | 8.95 | 9.12 | 1.019x | 0.878x |
| all | 9.84 | 8.95 | 8.96 | 1.001x | 0.911x |

In-check split:

| Row | calls | Board::legal | old ctx | evasion ctx | evasion/ctx |
| --- | ---: | ---: | ---: | ---: | ---: |
| in-check quiet | 17,866 | 9.00 | 8.70 | **2.52** | **0.290x** |
| in-check capture | 2,314 | 8.23 | 7.59 | 8.12 | 1.071x |
| in-check en passant | 2 | 9.80 | 9.55 | 9.34 | 0.979x |
| in-check castling | 22 | 5.18 | 5.31 | 5.52 | 1.041x |
| in-check commoner | 10,572 | 10.89 | 10.42 | 11.11 | 1.066x |
| in-check all | 30,054 | 9.49 | 9.12 | **5.95** | **0.653x** |
| out-of-check quiet | 556,062 | 8.65 | 8.03 | 8.11 | 1.010x |
| out-of-check all | 805,740 | 9.76 | 8.94 | 9.04 | 1.012x |

Setup costs 0.23–0.37 ns more per node. On the corpus that is ~0.23 ns against a
quiet-candidate saving of ~5.5 ns, and on the play-outs ~0.37 ns against a
~6.2 ns in-check-quiet saving; the candidate loop amortises it.

### Instruction diagnostic

`perf stat -e instructions:u` was available. A temporary, uncommitted probe
built the 41 perft-corpus nodes, classified the in-check quiet candidates once
(116 calls), ran 2,000 rounds (232,000 calls) through either the old or the new
context, and printed the call count:

```sh
perf stat -e instructions:u target/release/examples/evasion_probe old 2000
perf stat -e instructions:u target/release/examples/evasion_probe new 2000
```

| Context | instructions | instructions/call |
| --- | ---: | ---: |
| old `ctx.legal` | 42,729,606 | 184.18 |
| new `evasion.legal` | 16,889,663 | 72.80 |

That is a 60.5% instruction reduction on this same-host in-check-quiet probe.
The probe was removed after the measurement; the committed harness is
`examples/hotpath_bench.rs`.

## 6. Perft, gates and regression evidence

All global gates from the index pass:

| Gate | Result |
| --- | --- |
| `cargo test` | 196 unit tests, all integration suites, 18 doctests |
| `cargo test --all-features` | pass |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | clean |
| `cargo test --doc` | 18/18 pass |
| targeted `perft_tests`, `move_order_golden`, `verify_moves`, `conformance`, `effects`, `post_move`, `terminal_after`, `child_state`, `child_effect`, `ep_key`, `null_for`, `legality_context`, `counts`, `public_api`, `interchange`, `robustness` | all pass |
| `cargo run --release --example verify_perft 5` | 41/41, 2.22 s |
| `cargo run --release --example verify_perft 6` | 41/41, 64.47 s |

Perft timing (`target/release/examples/perft`, three runs, median) against the
2.10.0 numbers recorded in `report_wishlist2_5.md` on this host:

| Case | 2.10.0 report | this tree | Ratio |
| --- | ---: | ---: | ---: |
| startpos depth 6 | 0.582 s | 0.568 s | 0.977x |
| position 2 depth 5 | 0.253 s | 0.248 s | 0.978x |
| position 13 depth 5 | 0.310 s | 0.310 s | 0.998x |

No perft, key, move-order, FEN, terminality or existing legality answer changed.
The effect and child rows remain unchanged because no effect source was edited.

## 7. Measurement-gate verdict

The plan's primary gate passes clearly:

- exactness: full differential, zero legal mask rejections;
- in-check all: 0.507x the old context on corpus and 0.653x on play-outs;
- in-check quiet: 0.225x on corpus and 0.290x on play-outs;
- all-candidate class rows: corpus 0.923x (quiet), 0.979x (ep), 1.003x
  (castling), 1.035x (capture) and **1.048x (commoner)**; play-outs 0.991–1.030x.
  Corpus commoner is outside the 2% class-row blocker; the other corpus class
  rows are inside it.
- out-of-check rows: corpus quiet 1.015x, all **1.035x**; play-outs quiet
  1.010x, all 1.012x. Corpus out-of-check-all is outside the 2% blocker;
  play-outs are inside.

Caveat for plan 2: the fall-through rows are the only places the old context
looks faster. They use the *same* legality body and the *same* one-destination
test; the only difference is the value of the pass bitboard. The in-check
capture/commoner rows on seeded play-outs are 1.071x and 1.066x, and the corpus
commoner class row is 1.048x, while the same rows are at or below
`Board::legal` in some runs. The likely causes are the extra test before the
unchanged body plus run-to-run/thermal ordering; the instruction probe shows a
large win on the class the mask actually rejects. Removing the test would
require specialising the body or accepting a second accepting path, which this
plan forbids. Plan 2 must make the release call with this caveat visible.

**Explicit verdict:** exactness and the primary in-check gate pass. The literal
“no class row above 2%” gate does **not** pass cleanly on this run because of
the fall-through rows above. Treat W10 as not automatically release-ready until
plan 2 decides whether those small fall-through movements are noise or a
blocker. No correctness claim depends on that decision.

## 8. Changed files

- `src/position.rs` — `EvasionMask`, `LegalNode` pass field, prefilter,
  constructors, unit tests.
- `src/board.rs` — `Board::legality_context_with_state`, docs, unit test.
- `src/lib.rs` — hot-path documentation.
- `README.md` — candidate-loop bullet.
- `tests/legality_context.rs` — differential, statistics, focused fixtures.
- `tests/robustness.rs` — current-checkers/zero-count guard case.
- `examples/hotpath_bench.rs` — state-aware setup, candidate rows and check split.
- `docs/plans/feedback/report_wishlist3_1.md` — this report.

No public re-export, dependency, `unsafe`, allocation, `StateInfo` layout,
`ZOBRIST_VERSION`, move order, FEN or terminality changed. `Cargo.toml`,
`CHANGELOG.md` and `tests/public_api.rs` are untouched; plan 2 owns the release
pin and version decision.

## 9. Problems, deviations and unresolved parts

1. **Permissive pass set instead of stored block + flags.** The plan proposed a
   block bitboard plus an in-check flag. The implementation stores one
   permissive destination set: block ray ∪ occupied ∪ commoner neighbourhood ∪
   en-passant target. This keeps the “one stored bitboard” shape, removes the
   second source-square and move-type tests from the rejection path, and is
   still rejection-only. The trade-off is that castling and en passant fall
   through instead of being rejected by the mask; the shared body still rejects
   incorrect castling and accepts legal en passant.
2. **This is why the fixture table changed.** The inside-ray second-line case
   (`c1e3`) is the important fall-through test; the original outside-ray
   `b1d2` case remains.
3. **Fall-through rows.** See §7. The corpus commoner class row and the
   in-check capture/commoner split rows exceed 2% in this run even though they
   execute the same body. There is no correctness regression; plan 2 must decide
   whether the literal gate blocks the release.
4. **No downstream evidence reproduced.** The wishlist's `82.0%`, `0.72e9`,
   `.pvr` and proof claims remain downstream-only.
5. No separate instruction-count gate was added to the crate; the `perf stat`
   probe was diagnostic and temporary.
6. No missing correctness test is known for the landed rule. The one gap is
   deliberate: there is no counter/gate proving the expanded pass set causes
   no future lost rejection if the generator adds a new move encoding; plan 2 or
   a later review should revisit that if `MoveType` gains variants.

## 10. Next steps for plan 2

1. Read this report's §7 verdict and decide whether W10 earns a release under
   the plan's class-row gate or fails the stricter split-row reading.
2. If released, pin `Board::legality_context_with_state` in
   `tests/public_api.rs`, add the compact wishlist-3 release summary to
   `examples/hotpath_bench.rs`, bump to 2.11.0 and update `CHANGELOG.md`/README
   without claiming a downstream/whole-binary save.
3. If not released, leave 2.10.0 and W10 open, and do not mention the
   constructor in release notes.
4. Re-run the combined differential, benchmark, package check and owner's
   manual publish checklist; this report does not commit, tag or publish.
