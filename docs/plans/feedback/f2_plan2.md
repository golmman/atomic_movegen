# Plan 2 — Stable `Move` encoding, UCI parsing, and single-move queries (P1-4/P1-5)

## Scope

Address:

- **P1-4**: stable `Move` wire encoding (`to_bits`/`from_bits`), UCI
  parse/format (`Move::from_uci`, `Board::move_from_uci`, `Display`,
  `write_uci`), and `Hash`/`Ord` on `Move`.
- **P1-5**: `Board::is_legal`, `Board::is_legal_with_state`, and
  `Board::is_pseudo_legal` without building or cloning a `MoveList`.

They are one plan because `Board::move_from_uci` needs the single-move
validation primitives. Implement this plan before plan 3 (en-passant
normalization) and plan 6 (blast helpers), which can use the new predicates in
tests.

## Current-tree assessment

- `Move` is a private `u16` with a documented layout but no stable accessor.
  `Move::to_uci` already exists and allocates a `String`; `parse_sq` and
  `sq_str` exist but are explicitly not semver-covered.
- `Move` currently derives only `Debug, Clone, Copy, PartialEq, Eq`. There is
  no `Hash`/`Ord`, so consumers cannot key collections by `Move`.
- `Board::legal(m, &StateInfo)` exists but requires a populated state and
  assumes the move is pseudo-legal. Validation call sites therefore still build
  full legal lists (or clone them) to test one move.
- There is no UCI parser. Consumers generate all legal moves and compare
  `to_uci()` strings, which the feedback measured at 1.1–1.5 µs per lookup.

The requested APIs are all additive except adding traits to `Move`; no existing
signature changes.

## Decisions and pushback

1. **`Move::from_bits` validates structure, not legality.** `0` is the
   documented sentinel and maps to `Some(Move::NONE)`. Every other encoding
   whose origin equals its destination is rejected (`None`). All remaining
   16-bit patterns decode; they may still be illegal on a board. Castling and
   en-passant decoding are only meaningful on a board, but the bit pattern is
   stable.
2. **The layout becomes semver-stable.** Document in `Move`, `to_bits`,
   `from_bits`, and the changelog that the following cannot change without a
   major release:
   - bit layout `to:6 | from:6 | type:2 | promotion_type:2`;
   - `Move::NONE == 0`;
   - `PROMOTION_PIECES` order (queen, rook, bishop, knight).
3. **`Move::from_uci` is syntax-only.** `e1g1` produces a normal move
   `e1 -> g1`; castling is resolved by `Board::move_from_uci`. `0000` maps to
   `Move::NONE`. It validates square syntax and promotion letters
   (`n/b/r/q`), nothing else.
4. **`Board::move_from_uci` is board-aware and fully legal.** It maps UCI
   castling destinations to the internal king-takes-rook encoding, then checks
   pseudo-legality and legality. It is the safe replacement for the consumer’s
   “generate all moves and compare strings” path.
5. **`Board::legal` stays the low-level precondition-based predicate.**
   `is_legal_with_state` is the untrusted-input form and is equivalent to
   membership in `generate_legal_with_state(...)`, not merely
   `is_pseudo_legal`-free `legal()`. Concretely:

   ```rust
   is_legal_with_state(m, state)
       == is_pseudo_legal(m) && legal(m, state)
   ```

   This differs from `legal()` only for malformed moves (bad castling encoding,
   wrong promotion rank, unreachable target). It prevents arbitrary 16-bit input
   from being interpreted as a legal castling move. The hot move generator keeps
   calling `legal()` directly and does not pay for the shape checks.
6. **Do not put board-dependent predicates on `Move`.** `Move::is_zeroing`
   cannot be correct without the board (a normal `Move` does not say whether the
   mover is a pawn or whether the destination is occupied), so P1-5/P2-9 add
   `Board::is_zeroing` instead. This plan only adds `Board::is_pseudo_legal`;
   the convenience predicates are in plan 5.

## Implementation

### `src/types.rs` — wire encoding and UCI

Add to `Move`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Move(u16);

impl Move {
    /// Stable wire encoding. Exact inverse of `from_bits` for every
    /// structurally valid value.
    pub const fn to_bits(self) -> u16 { self.0 }

    /// Decode a stable wire code.
    /// - `0` -> `Some(Move::NONE)`
    /// - origin == destination (other than 0) -> `None`
    /// - otherwise `Some(move)`; no board legality is implied.
    pub fn from_bits(bits: u16) -> Option<Move>;

    /// Syntax-only UCI parse. `0000` -> `Move::NONE`.
    /// Castling destinations are not resolved here; use
    /// `Board::move_from_uci`.
    pub fn from_uci(s: &str) -> Option<Move>;

    /// Write UCI without allocating. `Move::NONE` writes `0000`.
    pub fn write_uci(self, out: &mut impl fmt::Write) -> fmt::Result;
}

impl fmt::Display for Move { /* delegates to write_uci */ }
```

Implementation details:

- `from_bits`: do not use `Square::NONE`; all 6-bit values are real squares.
  Use `Move::from_sq`/`to_sq` to detect `from == to`. Keep `Move::NONE` special
  because its bits are `0`, which would otherwise look like `a1a1`.
- `from_uci`:
  - reject strings that are not 4 or 5 ASCII bytes;
  - use the existing `parse_sq` for the first two and next two bytes;
  - reject `from == to`;
  - 5th byte maps `n/b/r/q` through `PROMOTION_PIECES` and builds
    `Move::make_promotion`; any other promotion letter is rejected.
- Refactor `to_uci` to allocate once (`String::with_capacity(5)`) and call
  `write_uci`. Keep the existing castling destination mapping
  (`e1h1 -> e1g1`, etc.) and promotion suffixes in `write_uci`.
- `write_uci` uses `sq_str`/direct file-rank characters; it must never allocate
  and must produce exactly the same text as the old `to_uci`.
- Derive `Hash`, `PartialOrd`, `Ord`. Ordering is the `u16` wire ordering,
  which is stable because the layout is now stable.

### `src/board.rs` — single-move predicates

Add:

```rust
impl Board {
    /// Populates a StateInfo internally, then checks full legality.
    pub fn is_legal(&self, m: Move) -> bool;

    /// Shape check first, then `legal` using the caller-populated state.
    /// Equivalent to membership in `generate_legal_with_state`.
    pub fn is_legal_with_state(&self, m: Move, state: &StateInfo) -> bool;

    /// Cheap pseudo-legality shape check. No self-check/blast guarantee.
    /// Equivalent to membership in `generate_pseudo_legal`.
    pub fn is_pseudo_legal(&self, m: Move) -> bool;

    /// Board-aware UCI lookup: resolves castling and requires legality.
    pub fn move_from_uci(&self, s: &str) -> Option<Move>;
}
```

`is_legal` / `is_legal_with_state`:

```rust
pub fn is_legal(&self, m: Move) -> bool {
    let mut state = StateInfo::new();
    self.populate_state(&mut state);
    self.is_legal_with_state(m, &state)
}

pub fn is_legal_with_state(&self, m: Move, state: &StateInfo) -> bool {
    self.is_pseudo_legal(m) && self.legal(m, state)
}
```

`is_pseudo_legal` mirrors `for_each_pseudo_legal` exactly:

1. Reject `Move::NONE`, `from == to`, and any move whose origin does not hold a
   piece of the side to move. Also reject a destination occupied by the side to
   move (except castling, whose encoded destination is the rook square).
2. Switch on the moving piece type:
   - **Pawn**
     - `EnPassant`: `board.ep_square() == Some(to)`, `to` is on the expected
       rank, the square behind `to` contains an enemy pawn, and `to` is attacked
       by this pawn. Mirror the generator: do not add extra “ep square empty”
       checks here; validation handles impossible FENs.
     - `Promotion`: pawn starts on its promotion-from rank, `to` is on the last
       rank, and `to` is either a single push or a diagonal enemy capture.
     - `Normal`: reject destinations on the last rank (the generator would emit
       promotion moves). Accept:
       - single push if `to` is the pawn-push target and empty;
       - double push if from the start rank, the double target is empty, and
         the intermediate square is empty;
       - diagonal capture if `to` holds an enemy piece and is attacked by the
         pawn. A normal move must not claim en passant.
   - **Knight / Commoner**: `to` is in the corresponding attack set and is not
     occupied by the mover.
   - **Bishop / Rook / Queen**: `to` is in the slider attack set computed on
     `self.occupied()` and is not occupied by the mover.
   - **Any other/no piece**: false.
3. **Castling** (`m.move_type() == Castling`):
   - `from` is the side’s commoner home square (`E1`/`E8`);
   - `to` is the corresponding rook home square (`H1`/`A1`/`H8`/`A8`), because
     that is the internal encoding;
   - the castling right is set, the commoner and rook are on their home
     squares;
   - the same emptiness conditions as `generate_castling` (F/G or B/C/D).
   - Do **not** test attacked pass-through squares here; `legal` does that.

`move_from_uci`:

```rust
pub fn move_from_uci(&self, s: &str) -> Option<Move> {
    let parsed = Move::from_uci(s)?;
    if parsed == Move::NONE { return None; }
    let candidate = self.resolve_castling_uci(parsed).unwrap_or(parsed);
    let mut state = StateInfo::new();
    self.populate_state(&mut state);
    self.is_legal_with_state(candidate, &state).then_some(candidate)
}
```

`resolve_castling_uci` maps:

- `e1g1` → `Move::make_castling(E1, H1)`
- `e1c1` → `Move::make_castling(E1, A1)`
- `e8g8` → `Move::make_castling(E8, H8)`
- `e8c8` → `Move::make_castling(E8, A8)`

The mapping is applied before legality checking. In atomic/standard UCI, a
commoner never has a normal two-square move, so there is no fallback to a normal
`e1g1` move. If the rights are missing, `is_pseudo_legal` rejects the castling
encoding.

### Documentation

- `Move`: call the layout and `PROMOTION_PIECES` order semver-stable.
- `Board::legal`: state that it assumes a pseudo-legal move and a populated
  state; point untrusted callers to `is_legal`.
- `Board::is_legal_with_state`: explicitly state equivalence to membership in
  `generate_legal_with_state`.
- `Board::is_pseudo_legal`: explicitly state equivalence to membership in
  `generate_pseudo_legal`, and that it is not a legality guarantee.
- `Board::move_from_uci`: state that it resolves castling and rejects illegal
  moves; `Move::from_uci` is syntax-only.
- `CHANGELOG.md`, `README.md`, and `lib.rs` feature list.

## Tests

### `src/types.rs`

- `move_bits_roundtrip_exhaustive`: for every `u16` with `from != to`, assert
  `Move::from_bits(bits).unwrap().to_bits() == bits`; assert
  `Move::from_bits(0) == Some(Move::NONE)`; assert `from_bits` rejects
  `a1a1`-style encodings.
- `move_bits_stable_golden`: hard-code a few wire values (normal, castling,
  each promotion) so an accidental layout/order change fails even if
  `to_bits`/`from_bits` remain self-consistent.
- `move_uci_parse_format`: `e2e4`, castling text, all four promotions,
  `0000`, malformed strings, invalid promotion letters, `from == to`.
- `move_display_alloc_free`: `format!("{}", m)` equals `to_uci()` and
  `write_uci` into a `String`; `Move::NONE` prints `0000`.
- `move_ord_hash`: ordering follows `to_bits`; equal moves hash equal.

### `src/board.rs`

- `is_pseudo_legal_matches_generator`: over `TRIVIAL_LEGALITY_CORPUS`, random
  playouts, and all pseudo-legal moves, `is_pseudo_legal(m)` is true; for a
  curated set of malformed moves it is false.
- `is_legal_matches_generated_list`: over the same corpus/playouts,
  `generate_legal(...).contains(m)` iff `is_legal(m)` and
  `is_legal_with_state(m, &state)`. Include at least one move of each type:
  quiet, capture, double push, en passant, promotion, castling.
- `is_legal_rejects_malformed_moves`: wrong side piece, empty origin,
  unreachable slider/knight/pawn target, normal move onto promotion rank,
  promotion from wrong rank, en-passant encoding when `ep_square` is elsewhere,
  castling encoding without rights/pieces, `Move::NONE`.
- `move_from_uci_resolves_castling`: `e1g1`/`e1c1`/`e8g8`/`e8c8` on the
  `r3k2r/8/8/8/8/8/8/R3K2R` fixture; also verify the returned move is the
  internal king-takes-rook encoding.
- `move_from_uci_rejects_illegal`: missing rights, blocked castling path,
  illegal promotion, wrong-side move, `0000`, malformed text.
- `move_from_uci_promotion_and_ep`: parse `e7e8q` / `e7e8n` and a legal ep
  capture; reject `e7e8k` (commoner promotion is not representable).
- `move_from_uci_matches_full_list_lookup`: seeded random playouts; for every
  legal move, its `to_uci()` parses back to the same move via
  `move_from_uci` (castling included).

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (move generation must be
  unchanged)
- A quick external benchmark comparing one `move_from_uci` lookup with the old
  generate-and-compare fallback; report the measured value. Expected tens of
  ns, not microseconds.

## Files to touch

- `src/types.rs` — `Move` derives/accessors, UCI parse/format, tests.
- `src/board.rs` — `is_legal*`, `is_pseudo_legal`, `move_from_uci`, tests.
- `src/lib.rs` / `README.md` — document the Move wire/API surface.
- `CHANGELOG.md` — added APIs and the stable-layout guarantee.

## Non-goals

- No SAN parsing or formatting.
- No board-dependent `Move::is_zeroing`/`Move::is_double_push`.
- No change to move generation order or the `MoveList` capacity.

## Final task

Write `docs/plans/feedback/f2_report2.md` with item status, the stable wire
golden values, parser/predicate equivalence results, measured UCI lookup cost,
changed files, and any unsupported semantics (e.g. castling only via
`Board::move_from_uci`).
