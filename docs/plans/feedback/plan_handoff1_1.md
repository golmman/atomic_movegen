# Plan 1 — Hot-path API and position helpers (MG1, MG2, R2)

## Scope

Address the low-risk, additive hot-path requests:

- **MG1**: `Board::null_for(attacker)` — the null counterpart of a position
  without a FEN round-trip.
- **MG2**: make the existing pseudo-legal callback stream public as
  `movegen::for_each_pseudo_legal`.
- **R2 partial**: re-export `StateInfo` at the crate root, add a typed
  castling-side query, and expose the between/x-ray helpers an engine needs for
  its own legality shortcuts.

This plan deliberately does **not** add `for_each_legal`; that belongs with the
effect context in plan 2, where it can use the per-move effect path.

## Current-tree assessment

- `src/movegen.rs` already has the internal
  `pub(crate) fn for_each_pseudo_legal<B>(...)` and
  `generate_pseudo_legal` is a thin collecting wrapper. The order-golden test
  `tests/move_order_golden.rs` already pins the output order.
- `Board` has no null-move constructor. The handoff's measured workaround is
  `fen_with(EnPassantMode::Legal)` + string split/join + `from_fen`.
- `StateInfo` is public in `board` but not re-exported at the crate root.
- `src/bitboard.rs` has `between_bb`, but the module is `pub(crate)`.
- `src/attacks.rs` has `BETWEEN_BB` and a test-only `LINE_BB`; there is no
  public x-ray helper.
- Castling rights are exposed only as a raw `u8`; there is no typed
  `(color, side)` query.

## Decisions and pushback

1. **Keep the name `null_for`.** It is short, domain-specific, and matches the
   handoff's measured prototype. Document that it is a null-move search helper,
   not a legal move. Do not add aliases; renaming later is breaking.
2. **`null_for` preserves the clocks and clears en passant.** It clones the
   board, sets `side_to_move = attacker`, clears `ep_square`, recomputes
   `game_ply` from the existing fullmove number exactly as the FEN round-trip
   does, and recomputes the hash. It does not populate or retain `StateInfo`.
   The result is an ordinary `Board`; callers must keep ephemeral null positions
   out of keyed/stored state.
3. **The public stream keeps the exact callback shape.**
   `FnMut(Move) -> ControlFlow<B>` is allocation-free and supports early exit.
   Do not replace it with an iterator in this plan.
4. **Do not re-export the stream at the crate root.** The existing module style
   exposes `movegen::generate_*`; keep `movegen::for_each_pseudo_legal`.
   Re-export `StateInfo` at the root because it is the hot-path type.
5. **Between/x-ray helpers live in `attacks`.** `bitboard` stays crate-private.
   Add `attacks::between(s1, s2)` and x-ray helpers with a precise definition:
   the squares newly attacked when the pieces on `blockers` are removed from
   `occupied`.
6. **Typed castling query is additive.** Add `CastlingSide` and
   `Board::has_castling_right(color, side)`. Keep `castling_rights() -> u8`
   unchanged for existing consumers.

## Implementation

### `src/board.rs` — `null_for`

Add next to the clock accessors:

```rust
/// The null counterpart of this position: same pieces and castling rights,
/// `attacker` to move, no en-passant target.
///
/// This is the cheap in-place equivalent of the FEN round-trip used by
/// null-move searches. The half-move clock and fullmove number are preserved,
/// so the derived game ply follows the new side exactly as the FEN path does.
/// The Zobrist hash is recomputed for the new side and cleared ep target.
///
/// The result is an ordinary [`Board`]. A caller that needs an ephemeral null
/// position must still keep it out of any keyed or stored state.
#[must_use]
pub fn null_for(&self, attacker: Color) -> Board {
    let mut board = self.clone();
    board.side_to_move = attacker;
    board.ep_square = None;
    // Match the FEN round-trip: fullmove number is preserved, and game_ply is
    // derived from the new side to move.
    let fullmove = self.fullmove_number();
    board.game_ply = 2 * (fullmove - 1)
        + if attacker == Color::White { 1 } else { 2 };
    board.recompute_hash();
    board
}
```

Use saturating arithmetic if `fullmove_number()` can be `u16::MAX`; the FEN
path already parses `u16`, so avoid a debug overflow. Add a short note to
`Board::fen`/`from_fen` docs that `null_for` is the allocation-free equivalent
for null-move replay.

### `src/movegen.rs` — public stream

Change the visibility of the existing function and keep the body unchanged:

```rust
/// Walk the pseudo-legal candidate stream for the side to move, invoking `f`
/// on each candidate in generation order, stopping early if it returns
/// [`ControlFlow::Break`].
///
/// Visits the same stages in the same order as [`generate_pseudo_legal`]:
/// pawn pushes and captures, promotions and en passant per pawn → knights →
/// bishops → rooks → queens → commoners → castling. `f` must not mutate the
/// board.
///
/// The order is a public compatibility contract; see the crate-level
/// documentation and `tests/move_order_golden.rs`.
pub fn for_each_pseudo_legal<B>(
    board: &Board,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> { /* existing body */ }
```

`generate_pseudo_legal` remains the collecting wrapper. Do not change the
stage order or the per-pawn order.

### `src/lib.rs` — re-export

```rust
pub use board::{
    BlastCasualties, Board, FenLoadError, FenPolicy, StateInfo, ValidationErrors,
};
```

Keep `movegen` as a module. Add `StateInfo` to the crate-level docs where the
hot-path types are listed.

### `src/types.rs` — typed castling side

Add:

```rust
/// The two castling sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CastlingSide {
    /// The h-file side (`O-O`).
    KingSide,
    /// The a-file side (`O-O-O`).
    QueenSide,
}
```

Re-export it from `lib.rs`. Add to `Board`:

```rust
/// Return `true` when `color` still has the castling right for `side`.
#[must_use]
pub fn has_castling_right(&self, color: Color, side: CastlingSide) -> bool {
    let bit = match (color, side) {
        (Color::White, CastlingSide::KingSide) => WK_CASTLE,
        (Color::White, CastlingSide::QueenSide) => WQ_CASTLE,
        (Color::Black, CastlingSide::KingSide) => BK_CASTLE,
        (Color::Black, CastlingSide::QueenSide) => BQ_CASTLE,
    };
    self.castling_rights & bit != 0
}
```

Do not change the raw `castling_rights()` accessor.

### `src/attacks.rs` — between and x-ray helpers

Add public wrappers:

```rust
/// Return the squares strictly between `s1` and `s2`, or empty when the
/// squares are not on the same rank, file, or diagonal.
#[inline(always)]
pub fn between(s1: Square, s2: Square) -> Bitboard {
    crate::bitboard::between_bb(s1, s2)
}

/// Rook attacks from `sq` that are revealed when the pieces on `blockers`
/// are removed from `occupied`.
///
/// Equals `rook_attacks(sq, occupied ^ blockers) & !rook_attacks(sq, occupied)`.
#[inline(always)]
pub fn xray_rook_attacks(
    sq: Square,
    occupied: Bitboard,
    blockers: Bitboard,
) -> Bitboard {
    rook_attacks(sq, occupied ^ blockers) & !rook_attacks(sq, occupied)
}

/// Bishop counterpart of [`xray_rook_attacks`].
#[inline(always)]
pub fn xray_bishop_attacks(
    sq: Square,
    occupied: Bitboard,
    blockers: Bitboard,
) -> Bitboard {
    bishop_attacks(sq, occupied ^ blockers) & !bishop_attacks(sq, occupied)
}

/// Queen counterpart of [`xray_rook_attacks`].
#[inline(always)]
pub fn xray_queen_attacks(
    sq: Square,
    occupied: Bitboard,
    blockers: Bitboard,
) -> Bitboard {
    xray_rook_attacks(sq, occupied, blockers)
        | xray_bishop_attacks(sq, occupied, blockers)
}
```

Document that `blockers` is expected to be a subset of `occupied`; the formula
is still well-defined otherwise, but the “revealed” interpretation assumes it.
Add tests for aligned and non-aligned squares, empty blockers, and blockers
that are not on the line.

## Tests

### `tests/null_for.rs` (new integration test)

Register it with `[[test]]` if the crate keeps `autotests = false`; otherwise a
normal integration test is enough. Compare `null_for` against the FEN
round-trip on six fixtures for both attackers:

- three middlegame FENs;
- one with a legal en-passant target;
- one with a 99-ply clock;
- one with castling rights on all four sides.

For each fixture and attacker, assert equality on:

- `fen()`;
- `hash()`;
- `hash_with(EnPassantMode::Legal)`;
- `side_to_move()`;
- `ep_square()`;
- `rule50()`;
- `game_ply()`;
- `castling_rights()`.

Also assert the original board is unchanged and that `null_for` does not
populate or retain a `StateInfo`.

### `src/movegen.rs` tests

- `for_each_pseudo_legal_matches_list`: over the order-golden positions and
  `TRIVIAL_LEGALITY_CORPUS`, collect the stream and compare it element-for-
  element with `generate_pseudo_legal`.
- `for_each_pseudo_legal_early_exit`: return `Break` after the first candidate
  and assert exactly one callback; return `Continue` and assert the full count.
- `for_each_pseudo_legal_order_matches_golden`: compare the stream's UCI order
  with the existing golden pseudo-legal lists.

### `src/board.rs` tests

- `null_for_matches_fen_roundtrip` (unit-level version of the integration test).
- `null_for_preserves_fullmove_and_rule50`.
- `null_for_clears_ep_and_recomputes_hash`.
- `has_castling_right_matches_raw_bits` over the castling fixtures and random
  playouts.

### `src/attacks.rs` tests

- `between_matches_bitboard_table` for all 64×64 pairs.
- `xray_helpers_match_definition` for a set of blockers on and off the line.
- `xray_empty_blockers_is_empty`.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (41/41; at least depth 5)
- `cargo test --test move_order_golden`
- `cargo test --test verify_moves`
- A quick external check that `null_for` removes the FEN allocation/parse from
  the null-replay path; report the measured delta if a harness is available.

## Files to touch

- `src/board.rs` — `null_for`, `has_castling_right`, docs, tests.
- `src/movegen.rs` — public stream, docs, tests.
- `src/types.rs` — `CastlingSide`.
- `src/attacks.rs` — between/x-ray helpers, tests.
- `src/lib.rs` — re-exports and docs.
- `tests/null_for.rs` — new differential test.
- `README.md` / `CHANGELOG.md` — new APIs and the null-move note.

## Non-goals

- No `for_each_legal` (plan 2).
- No effect context or per-move effect query (plan 2).
- No SAN or `Setup` (plan 4).
- No change to move generation order, perft values, hash values, or FEN output.
- No new dependencies.

## Final task

Write `docs/plans/feedback/report_handoff1_1.md` with item status, the exact
public signatures, the `null_for` differential results, stream/order results,
changed files, and any naming or invariant concerns.
