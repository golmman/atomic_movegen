# Hardening report — wishlist 3 follow-up

Status: **complete, no new defect found.** The state-aware `LegalityContext` is
total for every move encoding and checker pattern probed, and it never accepts a
move the shared legality body rejects. No source behaviour, oracle value, key,
move order, FEN output or public signature changed.

Plan: `docs/plans/hardening/plan_wishlist3.md`.
Related reports: `report_wishlist3_1.md`, `report_wishlist3_2.md`.

## Probes

A temporary, uncommitted probe compared the state-aware context with
`Board::legal` over 21 curated FENs (multiple commoners, missing commoners,
touching commoners, slider/leaper/double checks, castling rights with and
without rooks, live and impossible en-passant targets, back-rank pawns, and
terminal boards). It swept every valid 16-bit move encoding and five checker
patterns per board: current checkers, empty, full, all occupied squares, and an
unrelated square.

Result: **195 pseudo-legal candidates, 0 verdict differences**, and no panic
across the exhaustive move/checker sweep. The probe was deleted after the run.

The existing wishlist-2 robustness sweep now also calls
`Board::legality_context_with_state` for every move encoding on its eight
curated boards.

## Tests added

- `tests/robustness.rs::state_aware_context_is_rejection_only_for_arbitrary_checker_data`:
  for eight permissive boards and five checker patterns, every valid move
  encoding is total, and `ctx.legal(m)` implies `board.legal(m, &state)`.
- `tests/robustness.rs::wishlist2_predicates_are_total_for_arbitrary_move_bits`
  now also exercises the state-aware context with a freshly populated state.
- `tests/legality_context.rs`: three permissive multiple-commoner fixtures added
  to the edge corpus (`4r1k1/8/8/8/8/K7/8/4K3 w`, the same with Black to move,
  and `4k3/8/8/8/8/8/8/K3K3 w`).

Updated differential counts after the added fixtures:

- corpus: 90 positions, 1,750 candidates, 1,417 legal;
- in check: 242 candidates, 130 predicted mask rejections, **0 legal**;
- seeded play-outs unchanged: 14,960 positions, 283,924 candidates,
  4,928 predicted mask rejections, **0 legal**.

## Gates

All pass on the post-cleanup tree:

| Gate | Result |
| --- | --- |
| `cargo test` | 311 passed, 0 failed |
| `cargo test --all-features` | 311 passed, 0 failed |
| `cargo test --release` | 311 passed, 0 failed |
| both clippy configurations with `-D warnings` | clean |
| `cargo fmt --check` | clean |
| rustdoc with `-D warnings` | clean |
| `cargo test --doc` | 18/18 |
| all integration suites | pass |
| `verify_perft 6` | 41/41, 63.849 s |

No perft, key, move-order, FEN or terminality change.

## Changed files

- `tests/robustness.rs` — two state-aware total/rejection-only checks.
- `tests/legality_context.rs` — three permissive multiple-commoner fixtures.
- `docs/plans/hardening/plan_wishlist3.md`, this report.

No `src/` change was needed.

## Unresolved parts

- A future `MoveType` variant could add a quiet encoding the permissive pass set
  does not anticipate; the differential and focused fixtures must be revisited
  if the enum gains variants.
- `do_move` still accepts a geometrically impossible two-rank pawn move on
  malformed input, as recorded by the wishlist-2 hardening report. Legal play
  cannot reach it and changing it would touch the hot move path.
