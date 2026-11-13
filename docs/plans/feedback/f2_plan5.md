# Plan 5 — Canonical blast helper and small conveniences (P2-8/P2-9)

## Scope

Address:

- **P2-8**: one canonical atomic blast API:
  - `Board::blast_zone(sq) -> Bitboard`
  - `Board::blast_casualties(m) -> BlastCasualties` with per-color removal
    bitboards and the ep victim square.
- **P2-9**:
  - `attacks::attacks_from(pt, color, sq, occupied)`;
  - `Board::attackers_to_color(sq, color, occupied)`;
  - board-aware `Board::is_zeroing(m)` / `Board::is_double_push(m)`;
  - `MoveList::retain`;
  - corrected/document `MAX_MOVES` facts and the `MoveList::push` release
    behavior.

These are small, independent additions but share one goal: remove duplicated
atomic rules from consumers. They are deliberately kept after the correctness
work in plans 1–4.

## Current-tree assessment

The blast rules are currently re-derived in at least:

- `Board::do_move` / `Board::legal`;
- `Board::hash_after` (including ep and castling interactions);
- consumer modules (ordering, static tactics, quiet groups, move classification)
  outside this crate.

The 3×3 geometry is simple, but the subtle rules are easy to get wrong:
pawns are immune except at ground zero; the capturer is always destroyed;
en-passant ground zero is the target square and the victim is behind it;
castling and quiet moves have no blast.

`MoveList` already has `clear`, slices, and in-place compaction inside
`generate_legal_with_state`, but no public `retain`. `MAX_MOVES` is 256 while
consumers observe at most 62 legal moves. `push` currently uses a debug
assertion and silently drops a move in release if the cap were ever reached
(the doc incorrectly says “overwrites”).

## Decisions and pushback

1. **`blast_zone` is pure geometry.** A static `fn blast_zone(sq)` cannot know
   which squares hold pawns, so it returns the full 3×3 area including the
   center. The feedback’s “3×3 minus pawns” is applied by
   `blast_casualties`/`do_move`, not by the static geometry helper. Document
   this explicitly.
2. **`blast_casualties` returns all pieces the move removes, not only the
   blast wave.** That is what material-delta/ordering consumers need. The normal
   capture victim, the en-passant victim, the capturer at ground zero, and every
   non-pawn in the zone are all included, split by color. `en_passant_victim`
   identifies the special behind-the-target square.
3. **Do not add `Move::is_zeroing` or `Move::is_double_push`.** Both depend on
   the piece on the origin and, for zeroing, on whether the destination is
   occupied. A `Move` alone cannot answer them correctly (a rook can also move
   two ranks). Add `Board::is_zeroing`/`Board::is_double_push` instead and keep
   the error impossible to make.
4. **Keep `MAX_MOVES = 256` and the debug assertion.** The feedback observes at
   most 62 legal moves; 256 leaves a large margin. We cannot formally prove the
   bound for every artificial board from a short review, so do not claim a proof.
   Document the observed maximum and the release behavior accurately. Changing
   `push` to `assert!` in release is a separate performance decision; if done,
   benchmark it and keep it out of this plan.
5. **`MoveList::retain` preserves order** and uses the existing
   `as_mut_slice`/`set_len` primitives. Refactoring `generate_legal_with_state`
   to use it is optional; do it only if the perft benchmark stays neutral.

## Implementation

### `src/magic`/`src/attacks.rs` — `attacks_from`

Add to `src/attacks.rs`:

```rust
/// Attack set for a piece type of the given color on `sq`.
///
/// Pawns use `pawn_attacks(color, sq)`; sliders use the supplied occupancy;
/// commoners use king attacks.
pub fn attacks_from(
    pt: PieceType,
    color: Color,
    sq: Square,
    occupied: Bitboard,
) -> Bitboard {
    match pt {
        PieceType::Pawn => pawn_attacks(color, sq),
        PieceType::Knight => knight_attacks(sq),
        PieceType::Bishop => bishop_attacks(sq, occupied),
        PieceType::Rook => rook_attacks(sq, occupied),
        PieceType::Queen => queen_attacks(sq, occupied),
        PieceType::Commoner => king_attacks(sq),
    }
}
```

`PieceType` currently has exactly six variants and is `#[non_exhaustive]`
elsewhere; keep the match exhaustive and update it if the enum grows.
Document that this is the public dispatch helper the consumer keeps duplicating.

### `src/board.rs` — blast API

Add:

```rust
/// One square plus its eight neighbours: the full atomic blast area.
pub fn blast_zone(sq: Square) -> Bitboard {
    attacks::king_attacks(sq) | Bitboard::square_bb(sq)
}

/// Pieces removed by `do_move(m)`: capture victim, blast victims, and the
/// en-passant victim. `white`/`black` are the removed squares by victim color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlastCasualties {
    pub white: Bitboard,
    pub black: Bitboard,
    pub en_passant_victim: Option<Square>,
}

impl BlastCasualties {
    pub fn is_empty(self) -> bool;
    pub fn all(self) -> Bitboard; // white | black
}

impl Board {
    pub fn blast_casualties(&self, m: Move) -> BlastCasualties;
}
```

`blast_casualties` mirrors `do_move` without mutation:

1. Castling or any non-capturing normal move → `Default` (no blast).
2. Determine `to`, `us`, and the moving piece.
3. En passant:
   - victim square is behind `to`;
   - add the victim pawn to the opposite color’s bitboard;
   - set `en_passant_victim`;
   - simulate occupancy/pawns with the victim removed and the capturer moved.
4. Normal capture:
   - add the destination victim to its color’s bitboard;
   - remove it from simulated occupancy/pawns;
   - move the capturer (or promote it) into `to`.
5. Blast:
   - `area = blast_zone(to) & !simulated_pawns`;
   - `removed = (area & simulated_occupied) | Bitboard::square_bb(to)`;
   - for every removed square, add the piece standing there (after promotion,
     the promoted piece at `to`) to its color’s bitboard.
6. Quiet promotion/capture promotion use the same path: a quiet promotion has
   no blast; a capture promotion blasts normally.

The helper must not read/write `self` mutably, allocate, or call `do_move`.
Because it is a pure mirror, add an equivalence test against `do_move` (below).
If a later refactor can share code with `do_move` without a measured perft
regression, do so; otherwise the equivalence test is the drift guard.

### `src/board.rs` — attackers and board-aware move predicates

```rust
/// Attackers to `sq` restricted to one color.
pub fn attackers_to_color(
    &self,
    sq: Square,
    color: Color,
    occupied: Bitboard,
) -> Bitboard {
    self.attackers_to(sq, occupied) & self.pieces_color(color)
}

/// True when `m` resets the half-move clock: pawn move, promotion, ep, or
/// capture. Castling does not.
pub fn is_zeroing(&self, m: Move) -> bool;

/// True when `m` is a normal two-square pawn move from the start rank.
pub fn is_double_push(&self, m: Move) -> bool;
```

`is_zeroing`: `m.is_en_passant() || m.is_promotion()
|| (!m.is_castling() && (piece_on(from).type_of() == Some(Pawn)
                          || !self.empty(m.to_sq())))`.
Guard `Move::NONE`/empty origin with `false`. This matches `rule50_after`
without the debug-panic precondition.

`is_double_push`: `m.move_type() == MoveType::Normal`, origin holds a pawn of
the side to move, same file, `abs(to - from) == 16`, and the intermediate square
is empty. (The emptiness is an extra safety condition; the normal generator
already guarantees it.)

### `src/types.rs` — `MoveList::retain`

```rust
pub fn retain(&mut self, mut f: impl FnMut(Move) -> bool) {
    let mut write = 0;
    for read in 0..self.len {
        let m = self.moves[read];
        if f(m) {
            self.moves[write] = m;
            write += 1;
        }
    }
    self.len = write;
}
```

Document that order is preserved and the closure receives moves by value.
Optionally replace the manual compaction loop in
`generate_legal_with_state` with `retain` if the benchmark is neutral.

### `MAX_MOVES` / `MoveList::push` documentation

- Update `MAX_MOVES` docs: the consumer-observed maximum legal move count is 62
  (standard chess tops out at 218); 256 is a safety margin, not a proven tight
  bound.
- Fix `push` docs: release builds currently ignore a push at capacity (they do
  not overwrite an existing move); the debug assertion remains.
- Do not add a release panic in this plan. Mention in the report that the
  observed 62 and the generator’s per-piece target masks make 256
  intentionally conservative, but a formal proof is still absent.

### Documentation

- `src/attacks.rs`, `src/board.rs`, `src/types.rs` rustdoc.
- `CHANGELOG.md`, `README.md`.

## Tests

### Blast

1. `blast_zone_geometry`: center has 9 squares; corner A1 has 4; H8 has 4;
   squares in the zone include the center.
2. `blast_casualties_normal_capture`: use an existing blast fixture (rook/knight
   capture) and assert which colors/squares are removed.
3. `blast_casualties_pawn_immunity`: a pawn in the ring survives; a pawn
   capturer at ground zero is removed; an enemy pawn victim at the destination
   is removed.
4. `blast_casualties_en_passant`: victim is the pawn behind the target; ep
   victim is reported; capturer and non-pawn ring pieces are removed.
5. `blast_casualties_quiet_castle_promotion`: quiet moves/castling return
   empty; quiet promotion returns empty; capture promotion blasts.
6. `blast_casualties_matches_do_move`: seeded random playouts. For every
   pseudo-legal move, compute `BlastCasualties`, clone the board, `do_move`, then
   compare (`state.captured` plus `state.cap_sq` plus ep victim) with the
   helper’s per-color bitboards. Include all move types and check the ep victim
   field.

### Conveniences

7. `attacks_from_matches_individual_functions`: all six piece types, several
   occupancies/squares.
8. `attackers_to_color_matches_intersection`: compare with
   `attackers_to(...) & pieces_color(...)` on the corpus.
9. `is_zeroing_matches_rule50_after`: over pseudo-legal moves, `is_zeroing(m)`
   iff `rule50_after(m) == 0` (with a populated state where needed).
10. `is_double_push_matches_move`: true only for actual double pushes; false
    for a rook two-rank move, a one-square pawn move, and a promotion.
11. `move_list_retain_preserves_order`: retain evens from a known list, check
    length/order and that `as_slice()` is compact.
12. `move_list_push_capacity_docs`: a debug-only test (or doc assertion) that
    the corpus maximum is below `MAX_MOVES`; do not rely on release behavior.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft`
- Optional: use the blast helper in one example (e.g. `debug_moves`) to show the
  API; do not change move-generation behavior.

## Files to touch

- `src/attacks.rs` — `attacks_from`.
- `src/board.rs` — blast geometry/casualties, `attackers_to_color`,
  `is_zeroing`, `is_double_push`, tests.
- `src/types.rs` — `MoveList::retain`, `MAX_MOVES`/`push` docs, tests.
- `src/lib.rs` — re-export `BlastCasualties` if placed in `board`.
- `CHANGELOG.md`, `README.md`.

## Non-goals

- No capture-only generator or quiescence generator.
- No board symmetry transforms.
- No release-mode panic change for `MoveList::push`.
- No refactor of `do_move` unless it is benchmark-neutral.

## Final task

Write `docs/plans/feedback/f2_report5.md` with the blast helper equivalence
results, any observed move-count maxima, retain refactor decision, and
verification output.
