# Report — Plan 1: Hot-path API and position helpers (MG1, MG2, R2)

## Status

All plan items are implemented. The change is additive: no move order, perft
value, hash value, FEN output, or `Outcome` semantics changed.

| Item | Status | Notes |
| --- | --- | --- |
| MG1 `Board::null_for` | Done | Allocation-free null counterpart; differential-tested against the FEN round-trip. |
| MG2 public `for_each_pseudo_legal` | Done | Visibility changed from `pub(crate)` to `pub`; body and stage order unchanged. |
| R2 `StateInfo` root re-export | Done | `atomic_movegen::StateInfo` now resolves. |
| R2 typed castling side | Done | Added `CastlingSide` and `Board::has_castling_right`; raw accessor unchanged. |
| R2 public between/x-ray helpers | Done | Added `attacks::between` and the three `xray_*_attacks` helpers. |

## Public signatures added

```rust
// board
pub fn Board::null_for(&self, attacker: Color) -> Board;
pub fn Board::has_castling_right(&self, color: Color, side: CastlingSide) -> bool;

// types
pub enum CastlingSide {
    KingSide,
    QueenSide,
}

// movegen
pub fn for_each_pseudo_legal<B>(
    board: &Board,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B>;

// attacks
pub fn between(s1: Square, s2: Square) -> Bitboard;
pub fn xray_rook_attacks(sq: Square, occupied: Bitboard, blockers: Bitboard) -> Bitboard;
pub fn xray_bishop_attacks(sq: Square, occupied: Bitboard, blockers: Bitboard) -> Bitboard;
pub fn xray_queen_attacks(sq: Square, occupied: Bitboard, blockers: Bitboard) -> Bitboard;
```

Crate-root re-exports now include `StateInfo` and `CastlingSide`.

## `null_for` behavior and differential results

`null_for` clones the board, sets the requested side to move, clears the stored
en-passant target, derives `game_ply` from the preserved fullmove number using
saturating arithmetic, and recomputes the Zobrist hash. It does not retain or
populate a `StateInfo`; `Board` has no such field.

The new `tests/null_for.rs` compares `null_for` with the FEN round-trip for six
fixtures (startpos, middlegame, endgame, legal ep target, 99-ply clock, all four
castling rights) and both attackers. All comparisons pass for `fen()`, `hash()`,
`hash_with(EnPassantMode::Legal)`, side to move, ep square, rule50, game ply,
and castling rights. The source board is asserted unchanged. A second test
confirms the result behaves as an ordinary board when `populate_state` is
called.

## Stream and order results

- `for_each_pseudo_legal_matches_list` compares the stream element-for-element
  with `generate_pseudo_legal` over 13 positions, including the order-golden
  positions, en passant, castling, check, pins, extinction, and the two
  profiled positions.
- `for_each_pseudo_legal_early_exit` verifies `Break` stops after exactly one
  callback and `Continue` visits all 20 startpos candidates.
- `for_each_pseudo_legal_order_matches_golden` pins the startpos UCI order.
- `tests/move_order_golden.rs` and `tests/verify_moves.rs` both pass unchanged.

## Verification

| Gate | Result |
| --- | --- |
| `cargo test` | Pass: 118 unit, 1 move-order, 2 null-for, 4 perft, 1 verify-moves, 1 doctest. |
| `cargo test --features unsafe-optimizations` | Pass (same suite). |
| `cargo clippy --all-targets -- -D warnings` | Pass. |
| `cargo fmt --check` | Pass. |
| `cargo doc --no-deps` | Pass. |
| `cargo run --release --example verify_perft 5` | 41/41 passed. |
| `cargo run --release --example verify_perft` | 41/41 passed at depths 1–6; total 71.7 s. |
| `cargo test --test move_order_golden` | Pass. |
| `cargo test --test verify_moves` | Pass. |

No external null-replay benchmark harness was available, so no allocation or
timing delta is reported. The implementation removes the FEN string
construction, split/join, and parse from the null-replay path by construction.

## Changed files

- `src/board.rs` — `null_for`, `has_castling_right`, FEN doc note, unit tests.
- `src/movegen.rs` — public stream, contract docs, stream tests.
- `src/types.rs` — `CastlingSide`.
- `src/attacks.rs` — between/x-ray helpers and tests.
- `src/lib.rs` — re-exports and hot-path docs.
- `tests/null_for.rs` — new differential integration test.
- `README.md` — feature list.
- `CHANGELOG.md` — Unreleased additions.

## Problems encountered

- `Color` is `#[non_exhaustive]`, so the integration test's side-to-move match
  needs a wildcard arm even though only two values exist.
- `StateInfo::populate_state` intentionally fills only the hot legality fields,
  not the undo fields. The integration test therefore checks `checkers`,
  `pinned`, and commoner counts rather than `castling_rights`/`rule50`/`hash`.
- `game_ply` is a `u16`; an extreme fullmove number can overflow the FEN
  parser's `2 * (fm - 1)` expression. `null_for` uses saturating arithmetic as
  the plan requested. The pre-existing FEN parser behavior was not changed.

## Unresolved parts / missing tests

- No benchmark proves the allocation/timing improvement of `null_for`; this
  needs an external null-replay harness.
- The FEN parser's extreme-fullmove overflow remains a separate pre-existing
  issue and is outside this plan's scope.
- `for_each_legal` and the effect context remain for plan 2.

## Next steps

Proceed to `plan_handoff1_2.md` (effect context and per-move effects), which
can now use the public stream and the public between/x-ray helpers.
