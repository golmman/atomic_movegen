# Implementation Report: `has_legal_move` (release 2.2.0)

Implements `docs/plans/feedback/plan_has_legal_move.md`.

## 1. Version / publication

- `Cargo.toml` bumped **2.1.0 → 2.2.0** (semver-minor, additive only).
- README dependency snippet updated to `atomic-movegen = "2.2.0"`.
- **Not committed/tagged/published**: the repo's AGENTS.md forbids `git`
  write operations for the implementing agent, so the change is delivered as
  a working-tree diff for the maintainer to commit, tag (`v2.2.0`), and
  publish.

## 2. Gate results

| Gate | Result |
|---|---|
| Order-golden (`tests/move_order_golden.rs`, 12 positions, pseudo + legal UCI sequences in order) | pass — unchanged through the stream refactor; goldens were dumped from the pre-refactor code via a temporary example (deleted afterwards) |
| Equivalence property test (`has_legal_move_equivalence_random_playouts`) | pass — deterministic xorshift playouts, 5 start FENs × 16 seeds, depth-300 random walks, exactly **8,592** positions visited (count pinned in the test); asserts the contract `has_legal_move_with_state(b, s) == !generate_legal_with_state(b, s, list).is_empty()` plus order-agreement of two independently generated lists and wrapper agreement |
| Deterministic fixtures (`has_legal_move_deterministic_fixtures`, 10 fixtures) | pass |
| Unit + integration tests (`cargo test`) | pass — 51 lib tests, golden test, perft tests, verify_moves |
| Perft (`cargo run --release --example verify_perft 6`) | **41/41 positions pass at depth 6** (billions of nodes; confirms the refactor is behavior-preserving) |
| `cargo clippy --lib --tests` | clean (only pre-existing warnings in `examples/test_d5g8.rs`, untouched by this change) |
| `cargo fmt --check` | clean |
| `cargo doc` | warning-free |

## 3. Benchmark (Step 3)

`--release`, fat LTO, 2M timed iterations per position after warmup
(temporary bench example; deleted after recording — the repo has no
committed benchmark infrastructure and the plan required no new
dev-dependencies):

| Position | FEN | Legal moves | `generate_legal` | `has_legal_move` | Speedup |
|---|---|---:|---:|---:|---:|
| Quiet middlegame | `r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4` | 42 | 87.8 ns | 8.9 ns | **9.9×** |
| Start position | `rnbqkbnr/.../RNBQKBNR w KQkq - 0 1` | 20 | 49.6 ns | 8.8 ns | **5.6×** |
| In check | `3k4/8/8/8/8/8/8/3QK3 b - - 0 1` | 4 | 38.0 ns | 12.1 ns | **3.1×** |
| Captures only | `k7/8/3npn2/4P3/8/8/3nnn2/3nKn2 w - - 0 1` | 2 | 40.6 ns | 12.9 ns | **3.2×** |
| No legal move | fool's-mate checkmate FEN | 0 | 125.6 ns | 125.3 ns | 1.0× |

The expected shape from the plan holds: quiet middlegame ≪ generation, and a
no-legal-move position costs one full stream traversal (absence can only be
proven by exhausting the stream). For the consumer's profile (~95% of
children have moves and are never searched), the dominant case improves by
~6–10×.

## 4. Deviations from the plan

1. **"Castling as only legal move" fixture — replaced, because it is
   provably impossible in this ruleset.** Castling requires the king not to
   be in check (otherwise the E1 pass-through check rejects it), and with no
   checkers a quiet rook move along the back rank (`Rg1`/`Rb1`) is always
   trivially legal (unpinned, non-capture, non-commoner). So castling can
   never be the *only* legal move. The fixture instead uses the castling
   position and asserts both castling moves appear in the legal list (the
   fixture remains self-validating via `generate_legal`).
   **"En passant as only legal move" — implemented exactly**, with a
   hand-built position (`8/8/8/PP6/kP1Pp3/PN2P3/8/6K1 b - d3 0 1`) where the
   side to move's only pseudo-legal move is `e4d3` e.p. and it is legal (the
   commoner is walled in by enemy pieces whose capture would self-destruct
   it, and the pawn's push is blocked).
2. **Optional fast pre-pass (design item 3) — not added.** Measured: the
   closure-driven stream already reaches 8.9 ns on the quiet middlegame
   (~10× vs. generation). A specialized quiet-move pre-pass would introduce
   a second, independently maintained acceptance rule (exactly the kind of
   re-derived legality the plan forbids) for a marginal gain over a cost
   that is already dominated by `populate_state`. Per the plan's "do not add
   it speculatively", it was skipped.
3. **No git commit/tag/publish** (repo policy: read-only git for the agent).
   See §1.
4. Minor: goldens were generated with a temporary `examples/golden_dump.rs`
   that was removed after `tests/move_order_golden.rs` was authored, keeping
   the committed tree free of one-off tooling.

## 5. Public signatures as implemented

```rust
// src/movegen.rs
pub fn has_legal_move(board: &Board) -> bool;

pub fn has_legal_move_with_state(board: &Board, state: &StateInfo) -> bool;
```

Internal (crate-visible) addition:

```rust
pub(crate) fn for_each_pseudo_legal<B>(
    board: &Board,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B>;
```

`generate_pseudo_legal` is now a thin wrapper collecting the stream into a
`MoveList`; its output (moves and order) is bit-identical to the previous
implementation, verified by the golden test and the depth-6 perft suite.

## Notes / findings for the consumer

- `has_legal_move_with_state` applies exactly the filter
  `is_move_trivially_legal(board, m, state) || board.legal(m, state)` per
  candidate — correctness is inherited unchanged from
  `generate_legal_with_state`; the only new logic is the early exit plus the
  explicit `commoners(us).is_empty() ⇒ false` pre-check.
- Discovery while building fixtures: when the *opponent* has zero commoners
  (`them_commoners_count == 0`), `Board::legal` skips the pseudo-royal
  attack check entirely (`enemy_pr_destroyed` is trivially true), so the
  side to move's last commoner may move anywhere. Fixture FENs in the test
  suite account for this (e.g. the `all_pinned` fixture includes a distant
  black commoner).

## Next steps

- Maintainer: commit, tag `v2.2.0`, `cargo publish`.
- Consumer (`atomic_solver`): switch evaluate-only children from
  `generate_legal_with_state` + `populate_state` to
  `has_legal_move_with_state` with a reused `StateInfo`; expect the ~58%
  movegen-related runtime share to drop substantially.
