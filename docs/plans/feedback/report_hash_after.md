# Implementation Report: `Board::hash_after` / `Board::rule50_after` (release 2.3.0)

Implements `docs/plans/feedback/plan_hash_after.md`.

## 1. Version / publication

- `Cargo.toml` bumped **2.2.0 → 2.3.0** (semver-minor, additive only).
- README dependency snippet updated to `atomic-movegen = "2.3.0"`.
- **Not committed/tagged/published**: the repo's AGENTS.md forbids `git`
  write operations for the implementing agent, so the change is delivered as
  a working-tree diff for the maintainer to commit, tag (`v2.3.0`), and
  publish.

## 2. What was implemented

- `Board::hash_after(&self, m: Move) -> u64` — non-mutating Zobrist hash of
  the child position. Mirrors `do_move` exactly: side key; castling path
  (commoner/rook from-to keys, rights transition, ep cleared); captured-piece
  removal (normal capture on `to`, en-passant victim on its square); mover
  relocation or promotion (pawn out at `from`, promoted piece in at `to`);
  blast iteration over the exact intermediate occupancy/pawn state `do_move`
  uses (captured piece removed, capturer relocated — pinned by the property
  test); castling-rights transition; en-passant old-out/new-in.
- `Board::rule50_after(&self, m: Move) -> u16` — castling increments, pawn
  move or move onto an occupied square resets to 0, anything else increments.
- Refactor: `do_move`'s castling-rights update (origin/destination clearing +
  post-blast rook/commoner-presence cascade on A1/E1/H1/A8/E8/H8) is now the
  shared pure helper `castling_rights_after(old_rights, from, to, is_capture,
  post_blast)`; `do_move` feeds it the post-blast board squares, `hash_after`
  derives the same six squares from the origin/en-passant-victim/blast-set
  bitboard without materializing a board. Behavior-preserving.
- Tests in `src/board.rs`: `hash_after_equivalence_random_playouts` (property
  test, 5 start FENs × 16 seeds × 300-ply playouts, every pseudo-legal move
  checked at every visited position against a cloned board's `do_move`;
  visited count pinned at 8,592) and `hash_after_deterministic_fixtures` /
  `rule50_after_fixtures` (castling both colors/sides, ep blast, double push
  replacing an ep square, all 12 quiet/capture promotions, blast clearing a
  corner rook right, blast removing the e1/e8 commoner, rule50
  reset/increment cases).

## 3. Gate results

| Gate | Result |
|---|---|
| Property test (`hash_after_equivalence_random_playouts`, exactly 8,592 positions, all pseudo-legal moves each) | pass |
| Deterministic fixtures + rule50 fixtures | pass |
| Unit + integration tests (`cargo test`: 54 lib, move-order golden, perft tests, verify_moves) | pass |
| Perft (`cargo run --release --example verify_perft 6`) | **41/41 positions pass at depth 6** (confirms the `do_move`/`castling_rights_after` refactor is behavior-preserving) |
| `cargo clippy --lib --tests --examples` | clean for lib/tests (only the pre-existing warnings in `examples/test_d5g8.rs`, untouched) |
| `cargo fmt --check` | clean |
| `cargo doc` | warning-free |

## 4. Benchmark

Temporary bench example (deleted after recording, per the 2.2.0 convention;
no committed bench infrastructure, no new dev-dependencies). `--release`,
fat LTO, 2M timed iterations per position, all legal moves, per-call
averages:

| Position | Legal moves | `hash_after` | `do_move`+`hash`+`undo_move` | Speedup |
|---|---:|---:|---:|---:|
| Quiet middlegame | 42 | 4.5 ns | 12.7 ns | 2.8× |
| Reference position | 43 | 4.6 ns | 12.4 ns | 2.7× |

Per move class (same code, per-class loops — removes quiet/capture branch
misprediction from the average): quiet 3.1–3.6 ns vs 10.4–10.8 ns (≈3×);
captures 8.6–9.0 ns vs 21.6–31.0 ns (2.5–3.4×).

Two deliberate micro-optimizations were needed to get there:

1. The castling-rights computation is skipped entirely when it cannot change:
   quiet moves touching neither back rank, and captures whose
   origin/en-passant-victim/blast set intersects none of A1/E1/H1/A8/E8/H8.
2. `piece_key`/`piece_key_raw` read the Zobrist piece table via
   `get_unchecked` — the only `unsafe` in this change. The table is
   `[u64; 16] × 64`; squares are `< 64` and raw piece encodings `< 16` by
   type invariant, debug builds assert both. Without it the compiler emits
   compare-and-panic plumbing per table load (measured: quiet path
   4.2 ns → 3.0 ns).

## 5. Problems encountered

- Two correctness bugs caught by the equivalence tests before they could
  ship: the quiet path initially computed a blast set unconditionally (the
  plan's step 6 only applies to captures), and the castling path initially
  missed the side-key XOR. The pinned 8,592-position property test and the
  castling fixtures pinpointed both immediately.
- The plan's step 7 wording ("post-blast occupancy of those six squares") is
  subtly under-specified: what the cascade actually consumes is the
  post-blast *piece identity* on those squares (it compares against
  rook/commoner pieces), and a square counts as "emptied" if it is the
  origin, the en-passant victim square, or in the blast set. Implemented
  accordingly; the shared helper takes the six post-blast `Piece` values.
- Meeting the ≤ 1/3 target on the all-move average required the two
  micro-optimizations above; see §6 for the honest residual.

## 6. Unresolved parts / deviations

- The all-move average on this machine lands at ≈ 0.35–0.37× of the
  `do_move`+`hash`+`undo_move` round-trip (2.7–2.8× speedup), marginally
  above the plan's ≤ 1/3 target; the quiet-move class meets ≈ 1/3 and
  captures are at 2.5–3.4×. The residual cost is structural: the quiet path
  needs two *dependent* loads (piece-on-`from` feeds the piece-key table
  index). Eliminating that dependency requires an incremental per-square key
  array (e.g. `sq_hash: [u64; 64]`) maintained by `do_move`, which would grow
  `Board` from ≈ 200 to ≈ 700 bytes — a change to the crate's memory layout
  the plan's additive-only scope does not sanction (and the consumer's search
  clones/stack-allocates boards). Recorded as next steps instead.
- Benchmarks are machine-relative; the consumer's own round-trip baseline was
  ≈ 7–8 ns vs 12.4–12.7 ns here, so absolute `hash_after` cost on their
  hardware should scale down similarly.

## 7. Missing tests / next steps

- No test pins `hash_after`/`rule50_after` for *illegal* pseudo-legal moves
  outside random playouts (self-explosion captures, castling through check);
  the property test covers these implicitly (it checks every pseudo-legal
  move, and playouts visit such positions), but a dedicated fixture would
  make the intent explicit.
- If the consumer needs the last ≈ 1.5–2 ns on the quiet path, implement the
  incremental per-square key array (`sq_hash: [u64; 64]` maintained in
  `move_piece`/`remove_piece`/`place_piece`/`recompute_hash`) behind the same
  equivalence property test; it also accelerates `do_move`/`undo_move`.
- Consider exposing the temporary micro-benchmark permanently if a
  zero-dependency bench convention is ever introduced; for now the numbers
  live in CHANGELOG.md and this report.

## 8. Deliverables checklist

- [x] `hash_after` / `rule50_after` on `Board` (documented, `#[must_use]`)
- [x] Property test (pinned 8,592 positions) + fixtures
- [x] `do_move` refactored onto the shared castling helper; all goldens
      unchanged
- [x] Benchmark numbers in CHANGELOG.md / this report
- [x] Consumer update note at `docs/plans/movegen/report_update_2.3.0.md`
- [x] Version 2.3.0 (Cargo.toml, README, CHANGELOG)
