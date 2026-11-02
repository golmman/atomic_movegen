# Report 5 — Canonical blast helper and conveniences (P2-8/P2-9)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| `Board::blast_zone()` | Done | Pure 3×3 geometry; includes the center and pawn squares. |
| `Board::blast_casualties()` / `BlastCasualties` | Done | Mirrors `do_move`; per-color removal bitboards plus the ep victim square. |
| `attacks::attacks_from()` | Done | Public dispatch over all six piece types. |
| `Board::attackers_to_color()` | Done | Intersection of `attackers_to` and `pieces_color`. |
| `Board::is_zeroing()` / `Board::is_double_push()` | Done | Board-aware; no `Move`-only versions were added. |
| `MoveList::retain()` | Done | Order-preserving in-place compaction. |
| `MAX_MOVES` / `MoveList::push` docs | Done | Corrected release behavior and observed maximum. |
| Documentation | Done | Rustdoc, `README.md`, `CHANGELOG.md`, and this report. |
| Verification | Done | Tests, clippy, fmt, docs, and depth-6 perft all pass. |

## API decisions

- `blast_zone` returns the full 3×3 area. It cannot know which squares hold
  pawns, so pawn immunity is applied by `blast_casualties` and `do_move`.
- `blast_casualties` returns *all* removed pieces: the ordinary capture victim
  (or the en-passant victim), every non-pawn in the ring, and the capturer at
  ground zero. The normal victim and the capturer can therefore both appear at
  the destination square, one in each color's bitboard.
- `BlastCasualties` implements `Default` manually instead of deriving it,
  because `Bitboard` does not implement `Default` and adding that trait impl was
  unnecessary surface area for this plan.
- `Move::is_zeroing` / `Move::is_double_push` were not added. Both need board
  context (the piece on the origin and, for zeroing, destination occupancy), so
  they live on `Board`.
- `MoveList::retain` preserves order and uses the existing in-place list
  representation. `generate_legal_with_state` was deliberately **not** changed
  to call it; the hot path keeps its manual write-index compaction to avoid any
  closure-call/codegen risk. No benchmark was needed because the hot path was
  untouched.

## Blast equivalence

`blast_casualties_matches_do_move` runs seeded random playouts from six start
positions covering ordinary captures, en passant, promotions, and castling.
At every visited position it compares, for every pseudo-legal move:

- the helper's white/black removal bitboards, and
- the en-passant victim square,

against a cloned board's `do_move` result. The expected removals are assembled
from `StateInfo::captured`, `cap_sq`, and `cap_piece`. The walk visits more
than 2,000 positions and all pseudo-legal moves at each position.

Result: every generated move matched `do_move`; no divergences were observed.
Specific fixtures also pin normal captures, pawn immunity in the ring, the
en-passant victim behind the target, quiet moves/castling, quiet promotion, and
capture promotion.

## Observed legal-move maximum

A temporary example (`examples/_max_moves.rs`, created, run, then removed)
scanned all 41 FENs in `tests/perft_values.md` and every position reachable
through the selected depth:

| Scan | Positions visited | Maximum legal moves | Position |
| --- | ---: | ---: | --- |
| Perft FENs, depth ≤ 3 | 507,346 | 63 | `8/8/2N5/3K3p/1r4kP/4q3/8/q7 b - - 19 60` |
| Perft FENs, depth ≤ 4 | 14,003,267 | 68 | `8/8/5q2/3N3p/1r5P/4q3/6Kk/8 b - - 25 63` |

The plan's “consumer-observed maximum is 62” was therefore stale. The
documentation now states the measured 68 and explicitly calls 256 a
conservative safety margin, not a proven tight bound. A regression test pins
the 68-move fixture and checks it is below `MAX_MOVES`.

Depths 5–6 were not exhaustively scanned for move-count maxima, so 68 is a
lower bound on the true maximum, not a claimed global maximum.

## Problems encountered

1. **Normal capture victim recording.** The first equivalence-test version read
   only `StateInfo::captured`, which made ordinary captures look like they were
   missing the victim. `do_move` stores the main victim in
   `cap_sq`/`cap_piece`, while `captured` holds the en-passant victim and blast
   removals. A temporary `_tmp_blast.rs` example confirmed this; the test now
   unions both sources.
2. **Observed maximum differs from the plan.** The plan quoted 62, but the
   perft-corpus scan found 63 at depth 3 and 68 at depth 4. The implementation
   follows the measurement rather than the stale number.
3. **No `Bitboard::Default`.** Rather than widening the `Bitboard` API, the new
   `BlastCasualties` type has a small manual `Default` implementation.

## Verification

```text
cargo test
  106 unit tests passed
  all integration test targets and doctests passed

cargo clippy --all-targets -- -D warnings
  clean

cargo fmt --check
  clean

cargo doc --no-deps
  generated with no warnings

cargo run --release --example verify_perft
  Test cases: 41
  Result: 41/41 passed, 0/41 failed (total 74.808 s)
```

## Missing tests / remaining gaps

- `blast_casualties` is a pure mirror of `do_move`, not shared code. The
  random-playout equivalence test is the drift guard. A future refactor could
  share the removal computation if perft stays neutral.
- `MAX_MOVES` still has no formal proof for every artificial board. The scan
  only covers the 41 corpus positions through depth 4.
- `MoveList::push` release behavior is documented but intentionally not tested;
  forcing a release overflow would require a non-atomic artificial condition
  and the plan explicitly kept the debug assertion only.
- No dedicated benchmark was run because `generate_legal_with_state` was left
  unchanged and the new helpers are not on the hot path.

## Changed files

| File | Change |
| --- | --- |
| `src/attacks.rs` | `attacks_from()` and its dispatch test. |
| `src/board.rs` | `BlastCasualties`, `Board::blast_zone`, `blast_casualties`, `attackers_to_color`, `is_zeroing`, `is_double_push`, blast/convenience tests, and the observed-maximum regression test. |
| `src/types.rs` | `MoveList::retain`, corrected `MAX_MOVES`/`push` docs, and the retain test. |
| `src/lib.rs` | Re-export `BlastCasualties`. |
| `README.md` | Feature bullets for the new blast and convenience APIs. |
| `CHANGELOG.md` | Unreleased Added entries and the corrected capacity note. |
| `docs/plans/feedback/f2_report5.md` | This report. |
