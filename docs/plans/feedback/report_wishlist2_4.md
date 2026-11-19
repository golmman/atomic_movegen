# Implementation report — plan 4 (W9: null move with an advanced clock)

Status: **landed** as additive API. `Board::null_for_advanced` matches a
`Setup`-built expected board field for field on the fixtures, saturates both
clocks as documented, updates the Zobrist key incrementally, and is
allocation-free. `Board::null_for` and its tests are unchanged. The measured
row is substantially cheaper than both the existing null method and the
`Setup` rebuild adapter.

Plan: `docs/plans/feedback/plan_wishlist2_4.md`.
Wishlist item: W9 (P2). Plan 5 owns the release version and notes.

## Plan validation

The current-tree assessment matched the code:

- `Board::null_for` clones, sets the requested side, clears the ep target,
  derives `game_ply` from the preserved fullmove number, keeps `rule50`, and
  recomputes the hash from the 64-square array;
- the Zobrist key covers pieces, side, castling rights, and the stored ep file,
  but neither clock;
- `Setup` already exposes `set_side_to_move`, `set_castling_rights`,
  `set_ep_square`, `set_rule50`, and `set_game_ply`, and `Board::from_setup`
  derives the key;
- `game_ply` is an internal `u32`; its public getter saturates at `u16::MAX`,
  and `rule50` is a `u16`.

The proposed method shape, incremental side/ep key delta, and no-`set_rule50`
decision were implementable as written. One representability nuance was found
during test construction: a `Setup` carries `game_ply` as a `u16`, so it cannot
express an internal ply beyond `u16::MAX`. The plan already scoped the
large-fullmove comparison to “where representable”; the tests therefore compare
the public fields and key for that fixture, while a unit test pins the exact
internal `game_ply + 1`.

## What landed

### Public API

```rust
impl Board {
    /// The null counterpart of this position with an advanced half-move clock.
    #[must_use]
    pub fn null_for_advanced(&self, attacker: Color) -> Board;
}
```

The method returns the same pieces and castling rights, `attacker` to move, no
en-passant target, `rule50 + 1`, and `game_ply + 1`. Both clocks use saturating
arithmetic. It clones the stack-resident `Board` and writes fields; it does not
build a `Setup`, construct a FEN string, or allocate.

The documentation makes the clock/ply distinction explicit:

- `null_for` preserves the half-move clock and fullmove number, deriving the
  game ply as a FEN round-trip with the new side would;
- `null_for_advanced` advances `rule50` and `game_ply` unconditionally, as a
  50-move-rule null probe requires;
- for the usual `attacker = side_to_move.flip()`, both methods agree on the side
  and ep target, but the advanced variant also advances `rule50`;
- when the side does not flip, the advanced result is not necessarily
  FEN-equivalent to a same-fullmove round-trip, because its ply no longer has
  the parity implied by the stored side-to-move.

`Board::null_for` now links to the advanced variant; its behaviour and tests
are untouched.

### Hash update

The method updates the key incrementally:

- XOR `ZOBRIST.side` only when `attacker` differs from the previous side;
- XOR out the old ep-file key when the parent stored an ep target;
- leave every piece key and the castling-rights key unchanged.

The unit test compares `advanced.hash` with `advanced.recomputed_hash()` for the
six fixtures plus large-fullmove and saturated-clock fixtures, both attackers,
and both same-side and flipped-side cases. There is no second key-composition
implementation in the method.

## Differential results

New tests in `tests/null_for.rs` use a `Setup` expected board built by copying
every `(square, piece)`, setting the requested side, the parent castling rights,
`ep = None`, `rule50 + 1`, and `game_ply + 1`, then calling
`Board::from_setup`. For every fixture and both attackers they compare:

```text
advanced.fen()                      == expected.fen()
advanced.hash()                     == expected.hash()
advanced.hash_with(Legal)           == expected.hash_with(Legal)
advanced.side_to_move()             == attacker
advanced.ep_square()                == None
advanced.rule50()                   == parent.rule50().saturating_add(1)
advanced.game_ply()                 == parent.game_ply().saturating_add(1)
advanced.fullmove_number()          == expected.fullmove_number()
advanced.castling_rights()          == parent.castling_rights()
parent.fen()                        == the original fixture FEN
```

| Case | Checked pairs / rows | Result |
| --- | ---: | --- |
| six `FIXTURES`, both attackers | 12 | all equal |
| `rule50 == u16::MAX` saturation, both attackers | 2 | all equal |
| `rule50 == 99` → 100, terminal branch | 1 | `Draw` / `FiftyMoveRule`, equal |
| large fullmove `65535`, both starting sides and attackers | 4 | public fields and keys equal; internal ply pinned by unit test |
| incremental key vs `recomputed_hash`, unit test | 18 | all equal |
| `null_for` vs advanced distinction | 2 attackers | intended clock difference pinned |

The `rule50 == 99` fixture is `4k3/8/8/8/8/8/8/4KN2 w - - 99 50`; its advanced
child reaches `rule50 == 100` and reports `TerminalReason::FiftyMoveRule` exactly
like the `Setup` board. The large-fullmove fixtures start from both colors at
fullmove `65535`; the public `game_ply()` and `fullmove_number()` both saturate
consistently, while `hash`, `rule50`, side, ep, and castling rights match the
expected board. The source board is re-checked after every query.

### `null_for` unchanged

The existing `tests/null_for.rs::null_for_matches_fen_roundtrip` and
`null_for_result_is_an_ordinary_board`, and the existing `src/board.rs` null
tests, are unchanged and still pass. A new distinction test shows that with the
same attacker both methods clear ep and choose the same side, while
`null_for_advanced` always increments `rule50`. In the same-side fixture the
advanced game ply also differs; for the flipped-side fixture at fullmove 50
`null_for` already derives the advanced ply, so only the clock differs. That is
the documented FEN-preserving distinction, not a change to `null_for`.

## Benchmark

Harness: `examples/hotpath_bench.rs`, extended with the null rows. Command:

```sh
cargo run --release --example hotpath_bench 15 16
```

Each row is the median of 15 batches. Absolute ns are same-host only; the ratio
is the gate. `clone` is the bare `Board` clone; `null_for` is the existing
FEN-preserving null; `null_for_advanced` and `advanced (same side)` use flipped
and same-side attackers respectively.

The `Setup rebuild` row constructs a **fresh** `Setup` per call, copies every
piece, sets side/rights/`ep = None`/`rule50 + 1`/`game_ply + 1`, and calls
`Board::from_setup`; it is not a reused-`Setup` lower bound.

### Corpus (78 boards)

| Row | ns/call | ratio to `null_for_advanced` |
| --- | ---: | ---: |
| `clone` | 2.78 | 0.861x |
| `null_for` | 28.91 | 8.953x |
| `null_for_advanced` | 3.23 | 1.000x |
| `advanced (same side)` | 3.21 | 0.995x |
| `Setup rebuild` | 110.13 | 34.104x |

Ratios: advanced/null_for **0.112x**, advanced/Setup **0.029x**,
advanced/clone **1.161x**.

### Seeded play-outs (38,706 boards)

| Row | ns/call | ratio to `null_for_advanced` |
| --- | ---: | ---: |
| `clone` | 3.43 | 0.888x |
| `null_for` | 50.32 | 13.042x |
| `null_for_advanced` | 3.86 | 1.000x |
| `advanced (same side)` | 3.86 | 1.002x |
| `Setup rebuild` | 187.32 | 48.546x |

Ratios: advanced/null_for **0.077x**, advanced/Setup **0.021x**,
advanced/clone **1.126x**.

The advanced method is ~0.4 ns above a bare clone, while `null_for` pays a full
hash recompute and the `Setup` rebuild pays piece copying plus construction and
validation. This is the requested adapter cost removed from the hot probe.

### Instruction diagnostic

`perf stat -e instructions:u` was available. A temporary example (removed after
the run) built 64 deduplicated perft/conformance boards, ran 2,000–20,000 passes
depending on the row, and was driven by commands of the shape:

```sh
perf stat -e instructions:u target/release/examples/_instr_probe_null empty 20000
perf stat -e instructions:u target/release/examples/_instr_probe_null clone 10000
perf stat -e instructions:u target/release/examples/_instr_probe_null null_for 4000
perf stat -e instructions:u target/release/examples/_instr_probe_null advanced 10000
perf stat -e instructions:u target/release/examples/_instr_probe_null advanced_same 10000
perf stat -e instructions:u target/release/examples/_instr_probe_null setup 1000
```

A second identical run was within a few hundred instructions per row; the table
uses the second run.

| Row | Calls | Total instructions | Raw / call | Net of empty-loop overhead |
| --- | ---: | ---: | ---: | ---: |
| `empty` | 1,280,000 | 7,352,216 | 5.74 | — |
| `clone` | 640,000 | 24,592,191 | 38.43 | ~32.68 |
| `null_for` | 256,000 | 121,172,056 | 473.33 | ~467.58 |
| `null_for_advanced` | 640,000 | 35,022,372 | 54.72 | **~48.98** |
| `advanced (same side)` | 640,000 | 33,742,617 | 52.72 | ~46.98 |
| `Setup rebuild` | 64,000 | 123,410,216 | 1,928.28 | ~1,922.54 |

The same-side row saves the side-key toggle; both advanced rows remain well
below the existing null and the adapter. These are host diagnostics, not a
portable promise.

## Gate results

- `cargo test` — all suites pass; 192 unit tests, all integration suites,
  18 doctests.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo doc --no-deps` — no warnings.
- `cargo run --release --example verify_perft 5` — **41/41**.
- Targeted suites including `perft_tests`, `move_order_golden`, `verify_moves`,
  `conformance`, `effects`, `post_move`, `terminal_after`, `child_state`,
  `ep_key`, `null_for`, `public_api`, and `interchange` all pass.
- `cargo test --features unsafe-optimizations` — all pass.
- The change is additive and touches no existing legality, generation,
  `populate_state`, FEN, move-order, terminality, key-table, or
  `ZOBRIST_VERSION` path, so no perft timing regression run was required; the
  complete perft suite and the depth-5 release verifier are green.

## Changed files

- `src/board.rs` — `Board::null_for_advanced`, cross-link from `null_for`, unit
  test for incremental hash/clock exactness.
- `tests/null_for.rs` — `Setup` differential, saturation, 50-move branch,
  large-fullmove, and `null_for`-distinction tests.
- `tests/public_api.rs` — pins `Board::null_for` and
  `Board::null_for_advanced`.
- `examples/hotpath_bench.rs` — fresh-`Setup` disclosure plus null rows and
  ratios.
- `src/lib.rs` — hot-path docs.
- `README.md`, `CHANGELOG.md` — additive API and clock distinction.
- `docs/plans/feedback/report_wishlist2_4.md` — this report.

No `Cargo.toml` version change; plan 5 owns the release.

## Problems encountered and deviations

1. **`Setup` cannot represent an internal ply beyond `u16::MAX`.** The
   large-fullmove test compares the public fields and key against a
   `Setup`-built board, and the `src/board.rs` unit test asserts the exact
   internal `game_ply + 1`. This follows the plan's “where representable”
   wording rather than inventing a wider setup surface.
2. **The plan's “differ in rule50 and game_ply” prose is too strong for every
   flipped-side fixture.** `null_for` already derives the advanced ply from the
   preserved fullmove number at fullmove 50, so only `rule50` differs there. The
   test pins the actual semantics: `rule50` always differs, and `game_ply`
   differs when the parent already had the advanced parity (the same-side case).
3. **The benchmark `Setup` row builds a fresh `Setup` per call.** This includes
   the piece-copy cost the downstream adapter pays. The table and harness
   documentation say so explicitly.
4. **The method lives beside `null_for` in `src/board.rs`.** The plan allowed
   this and it keeps the two clock contracts adjacent. No new module or
   dependency was added.

## Unresolved parts and missing tests

- No public `set_rule50`/`set_game_ply` on `Board` was added, as planned. The
  dedicated method is the only new clock-mutating surface.
- The method is not wired into any internal search or legality path; W9 is an
  additive building block. Adoption remains a downstream measurement.
- `null_for` still recomputes the full hash. Changing it would alter an existing
  public method's implementation and is outside this plan.
- Internal `u32` saturation is not reachable through a parsed FEN or ordinary
  play in the tested range; the public `u16` saturation and the large-fullmove
  boundary are tested, but a literal `u32::MAX` game ply is not constructed.
- “Allocation-free” is established by the implementation (clone plus field
  writes, no `Setup`, no FEN string) and is not instrumented with a custom
  allocator test.

## Next steps

- Plan 5 (`integration, combined measurement and release`) should pin
  `Board::null_for_advanced` as part of the additive API if it remains in the
  release, mention the FEN-preserving vs advanced-clock distinction in the
  release notes, and choose the release version from the landed plans.
- If a downstream needs a general board clock setter rather than the null
  method, that is a separate additive request; this plan deliberately does not
  add one.
- Keep `null_for` as the FEN-preserving null. Any future optimisation of its
  hash recomputation must preserve its documented fullmove/ply contract and
  carry its own measurement.
