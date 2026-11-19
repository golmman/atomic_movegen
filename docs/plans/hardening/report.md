# Hardening report — wishlist 2 follow-up

Status: **complete**. Two panic paths were found in the newly shipped
wishlist-2 surface, both are fixed, and the new predicates now survive an
exhaustive 16-bit move-encoding sweep. No oracle value, perft node count, key,
move order, FEN output, or public API shape changed.

Plan: `docs/plans/hardening/plan.md`.
Related reports: `docs/plans/feedback/report_wishlist2_1.md` … `_5.md`.

## Findings

### 1. `child_effect(..., Legal)` panicked on an impossible double push

Reproducer:

```rust
let board = Board::from_fen("p3k3/8/8/8/8/8/8/p6K b - - 0 1")?;
let m = Move::make_move(Square::A1, Square::A3);
board.child_effect(m, board.counts(), EnPassantMode::Legal);
// panic: index out of bounds: the len is 64 but the index is 64
// at src/attacks.rs pawn_attacks
```

`a1a3` is not pseudo-legal, but it is well-formed by `child_effect`'s stated
contract: the origin holds a piece of the side to move. The move is a two-rank
pawn push, so the child view records `ep_square = Some(Square::NONE)`
(`from_index(0 - 8)`), and the `Legal`-mode legal-EP helper fed that `NONE`
target to `attacks::pawn_attacks`. This is exactly the class of input the
"well-formed illegal moves still describe the child" paragraph promises to
handle.

### 2. `Board::legal` panicked on an inconsistent `StateInfo` (pre-existing)

```rust
let board = Board::from_fen("4k3/8/8/8/8/8/8/R7 w - - 0 1")?; // no white commoner
let mut state = board.counts_only_state_or_hand_written();
state.commoners_count = 1; // not current for this board
board.legal(Move::make_move(Square::A1, Square::A2), &state);
// panic: king_attacks(Square::NONE)
```

This is not a 2.10.0 regression (2.9.0 has the same body shape), but the new
`legal_in` is now shared by `Board::legal`, `is_legal_with_state`,
`LegalityContext::legal`, and the effect path. The documented contract still
requires current counts; hardening makes the body total anyway.

## Fixes

All in safe Rust, no new API, no dependency.

| Location | Change |
| --- | --- |
| `src/position.rs` | New `is_valid_ep_target(us, ep)`: rejects `Square::NONE` and any target not on rank 6 (White to move) / rank 3 (Black to move). |
| `src/position.rs` | `legal_ep_square_in_view` validates the stored target before `attacks::pawn_attacks`; it now returns `None` for a structurally impossible target. |
| `src/position.rs` | `legal_in` returns `false` when the moving side has no commoner, before `our_commoners.lsb()` is used as an attack-table index. This is the same totality guard the effect path already carried. |
| `src/board.rs` | `Board::legal_ep_square` applies `is_valid_ep_target` before its own attack lookup. |
| docs | `legal_in`, `legal_with_counts`, and `LegalNode::legal` state that they do not apply the callers' count guard but still reject a commoner-less side. |

The `counts`-based `playable` flag in `LegalityContext` is unchanged; the new
guard is a second line of defence for a stale/hand-written `Counts`, not a
replacement for the documented precondition.

## Tests added

- `src/board.rs::tests::inconsistent_counts_do_not_panic_and_reject_commoner_less_side`
  pins the second finding with real and faked counts, `is_legal_with_state`,
  and a `Counts::new(1, 1)` context.
- `tests/child_effect.rs::child_effect_survives_an_impossible_double_push`
  pins the first finding and compares `occupied`, `key` (both modes), `rule50`,
  and `capture` field for field with a cloned `do_move` child.
- `tests/robustness.rs::wishlist2_predicates_are_total_for_arbitrary_move_bits`
  sweeps all 2^16 encodings over eight curated boards (both colors, castling, a
  live ep target, terminal boards, back-rank pawns) calling
  `child_effect` (both modes), `child_effect_with_occupancy`,
  `LegalityContext::legal`, `null_for`, and `null_for_advanced`.

The temporary exhaustive probe used during development also called
`Board::legal`/`is_legal_with_state` with a faked `StateInfo`; it reported
**0 panics** after the fix (it reported 24 before, all the same ep path). The
probe was deleted; the public regression is the robustness test above.

## Oracle and timing gates

- `cargo test` — 193 unit tests, every integration suite, 18 doctests; all
  pass.
- `cargo test --features unsafe-optimizations` — all pass.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` — no
  warnings.
- `cargo test --doc` — 18/18.
- `cargo run --release --example verify_perft 6` — **41/41**, 65.6 s.
- Differential suites `counts`, `legality_context`, `child_effect`, `null_for`
  remain exact.

Perft A/B against a clean `HEAD` archive (`git archive HEAD` in `/tmp`, identical
release profile), alternating runs on the same host:

| Case | HEAD median | hardened median | Ratio |
| --- | ---: | ---: | ---: |
| Kiwipete depth 5 (153,516,309 nodes) | 0.7949 s | 0.7972 s | 1.003x |
| startpos depth 6 (118,926,425 nodes) | 0.5822 s | 0.5812 s | 0.998x |

Identical node counts; the largest movement is inside run-to-run spread and far
below the ~2 % blocker.

Hot-path benchmark (`cargo run --release --example hotpath_bench 15 16`) keeps
the wishlist-2 ratios: `ctx.legal` 0.918x/0.905x `Board::legal`,
`child_effect` 0.405x/0.314x the `PostMove` key path, `null_for_advanced`
0.119x/0.076x `null_for`.

## Changed files

- `src/position.rs` — `is_valid_ep_target`, `legal_in` totality guard, EP
  validation, doc clarifications.
- `src/board.rs` — `Board::legal_ep_square` validation, mixed-count unit test.
- `tests/child_effect.rs` — impossible-double-push regression.
- `tests/robustness.rs` — exhaustive wishlist-2 predicate sweep.
- `CHANGELOG.md` — `### Fixed` entries under 2.10.0.
- `docs/plans/hardening/plan.md`, this report.

## Problems encountered

- The release profile uses `panic = "abort"`, so the development probe used a
  `catch_unwind` sweep had to run in the dev profile. The shipped regression
  tests use ordinary assertions, not `catch_unwind`.
- `do_move` on the same impossible double push stores `Some(Square::NONE)` and
  `fen()` prints `??` for that target. This is pre-existing and only reachable
  for an input `do_move` already documents as malformed; the hardening test
  compares against that child so the new query is exact rather than silently
  diverging.

## Unresolved parts and missing tests

- `do_move` itself still accepts a geometrically impossible two-rank pawn move
  instead of panicking or normalizing its ep target. Legal play cannot reach it;
  changing it would touch the hot move path and is a separate decision.
- Stale `Counts`/`LegalityContext` snapshots are documented but still not
  tested by mutating a board after construction.
- No allocation instrumentation; the allocation-free claims remain shape-based
  as recorded by the wishlist reports.
- W7's ≤ 100 and W8's ≤ 120 instruction targets remain open measured items,
  unchanged by this session.

## Next steps

No correctness follow-up is required for the 2.10.0 release. Optional later
work: a measured fused occupancy/key/clock path for W8, and a decision on
normalizing impossible double pushes inside `do_move`.
