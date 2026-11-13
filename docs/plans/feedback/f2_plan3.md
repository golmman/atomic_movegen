# Plan 3 — En-passant normalization (P1-6)

## Scope

Add canonical en-passant handling for hashing and FEN output:

- `Board::legal_ep_square() -> Option<Square>`
- `pub enum EnPassantMode { Always, Legal }`
- `Board::hash_with(mode)` / `Board::fen_with(mode)`
- `Board::hash_after_with(m, mode)` for consistency with `hash_after`

The default `hash()` and `fen()` stay FSF-compatible (`Always`): `do_move` still
sets `ep_square` after every double push and the default hash/FEN still include
it. The new mode is opt-in.

## Current-tree assessment

`do_move` already follows FSF: after a double push it stores the ep target, and
`recompute_hash`/`update_hash_state_transition` always XOR the ep-file key when
`ep_square` is `Some`. `hash_after` mirrors that transition. `Board::fen()`
always prints the stored ep target.

This creates the transposition mismatch reported in feedback §P1-6:

```
1.Nf3 Nc6 2.e4   -> same placement as 1.e4 Nc6 2.Nf3, but e3 ep is set
1.e4 Nc6 2.Nf3   -> no ep square
```

The positions are identical for play but hash differently. This is not a
repetition-soundness bug (an ep-set position cannot recur in a repetition), but
it loses TT transpositions and makes FEN-derived keys inconsistent.

## Decisions and pushback

1. **Do not change the default.** `hash()` and `fen()` remain `Always`.
   Changing them would break every existing consumer’s stored keys and would
   diverge from FSF’s FEN behavior. The feedback explicitly allows an explicit
   major-version decision; this plan keeps the additive route instead.
2. **`Legal` means at least one fully legal ep capture exists.** It is not
   enough to have an enemy pawn adjacent to the double-pushed pawn. The ep
   capture must survive pins, self-explosion, and the commoner attack check.
   `legal_ep_square` therefore uses the normal legality predicate for at most
   two candidate moves.
3. **`legal_ep_square` does not build a `MoveList`.** It computes the at most
   two pawn origins from the attack table and tests `Move::make_enpassant` with
   the existing `legal` predicate. It works on terminal boards (returns
   `None`).
4. **`hash_after_with(Legal)` is a convenience, not the hot path.** It is
   implemented only for the mode requested; for a double push it may clone and
   make the child board to evaluate the child’s legal ep square. Document that
   `hash_after`/`hash` remain the fast, `Always`-mode pair. Consumers that want
   normalized keys in a hot loop should use `legal_ep_square` and adjust their
   own key, or accept the optional cost.
5. **No `EnPassantMode::PseudoLegal` yet.** The feedback asks only `Always` and
   `Legal`; adding a third semantic now would expand the API without a consumer.

## Implementation

### `src/types.rs` (or `src/board.rs`)

Add and re-export from `src/lib.rs`:

```rust
/// How an en-passant target participates in a position key or FEN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EnPassantMode {
    /// Include the stored ep square after every double push (FSF-compatible,
    /// the default for `hash()` and `fen()`).
    Always,
    /// Include the ep square only when at least one fully legal ep capture
    /// exists.
    Legal,
}
```

### `src/board.rs`

Add:

```rust
impl Board {
    /// The stored en-passant target, but only when at least one legal
    /// en-passant capture exists.
    pub fn legal_ep_square(&self) -> Option<Square>;

    /// Hash with the selected ep policy.
    pub fn hash_with(&self, mode: EnPassantMode) -> u64;

    /// FEN with the selected ep policy.
    pub fn fen_with(&self, mode: EnPassantMode) -> String;

    /// Hash after `m` with the selected ep policy.
    pub fn hash_after_with(&self, m: Move, mode: EnPassantMode) -> u64;
}
```

`legal_ep_square`:

```rust
pub fn legal_ep_square(&self) -> Option<Square> {
    let ep = self.ep_square?;
    let mut state = StateInfo::new();
    self.populate_state(&mut state);
    if state.commoners_count == 0 || state.them_commoners_count == 0 {
        return None;
    }

    let us = self.side_to_move;
    let them = us.flip();
    // Squares from which one of our pawns attacks `ep`.
    let mut origins =
        attacks::pawn_attacks(them, ep) & self.pieces_color_pt(us, PieceType::Pawn);
    while !origins.is_empty() {
        let from = origins.pop_lsb();
        let victim = match us {
            Color::White => Square::from_index(ep as i8 - 8),
            Color::Black => Square::from_index(ep as i8 + 8),
        };
        if self.piece_on(victim) == make_piece(them, PieceType::Pawn)
            && self.legal(Move::make_enpassant(from, ep), &state)
        {
            return Some(ep);
        }
    }
    None
}
```

Notes:

- Use `pawn_attacks(them, ep)` because attack tables are symmetric; this gives
  the squares from which an `us` pawn attacks `ep`.
- `populate_state` also supplies `checkers`/`pinned` for `legal`.
- Do not call `generate_legal`; there is no `MoveList`.

`hash_with`:

```rust
match mode {
    EnPassantMode::Always => self.hash,
    EnPassantMode::Legal => match self.ep_square {
        Some(sq) if self.legal_ep_square().is_none() => {
            self.hash ^ crate::zobrist::ZOBRIST.ep[file_of(sq) as usize]
        }
        _ => self.hash,
    },
}
```

`fen_with`: factor the existing body of `fen()` into a private
`write_fen(&self, ep: EnPassantMode, out: &mut String)` (or return the ep field
as a small helper). `fen()` calls it with `Always`; `fen_with(Legal)` prints `-`
when `legal_ep_square()` is `None`. Do not duplicate the board/castling/clocks
formatting.

`hash_after_with`:

```rust
pub fn hash_after_with(&self, m: Move, mode: EnPassantMode) -> u64 {
    match mode {
        EnPassantMode::Always => self.hash_after(m),
        EnPassantMode::Legal => {
            // The ep square can only be created by a double push.
            let is_double_push = m.move_type() == MoveType::Normal
                && (m.to_sq() as i8 - m.from_sq() as i8).abs() == 16
                && {
                    let p = self.piece_on(m.from_sq());
                    p.type_of() == Some(PieceType::Pawn)
                };
            if !is_double_push {
                return self.hash_after(m);
            }
            let mut child = self.clone();
            let mut state = StateInfo::new();
            child.do_move(m, &mut state);
            child.hash_with(EnPassantMode::Legal)
        }
    }
}
```

The double-push check is a guard, not a substitute for `hash_after`’s
precondition. `hash_after_with` keeps `hash_after` semantics for `Always`.
If the clone cost is unacceptable in review, keep `legal_ep_square` as the
primary API and document `hash_after_with(Legal)` as a convenience; do not
silently make `Always` slower.

### Defaults and consistency

- `hash()` = `hash_with(EnPassantMode::Always)`.
- `fen()` = `fen_with(EnPassantMode::Always)`.
- `hash_after()` = `hash_after_with(m, EnPassantMode::Always)`.
- `do_move(m)` followed by `hash()` must continue to equal `hash_after(m)`.
- `do_move(m)` followed by `hash_with(Legal)` must equal
  `hash_after_with(m, Legal)`.

### Documentation

Update:

- `Board::hash`, `Board::fen`, `Board::hash_after` — explain the default ep
  policy and point to the `_with` variants.
- `Board::ep_square` — state that it is the stored FSF-style target, not
  necessarily a capturable target; use `legal_ep_square` for the latter.
- `CHANGELOG.md` under `Unreleased`.
- `README.md` / `lib.rs` feature list.

## Tests

Add to `src/board.rs` tests:

1. `legal_ep_square_detects_legal_capture`:
   - `8/8/8/PP6/kP1Pp3/PN2P3/8/6K1 b - d3 0 1` → `Some(D3)` (`e4d3` is the
     known legal ep capture).
   - The existing ep fixture
     `rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2` has no
     legal ep capture → `None` even though `ep_square()` is `Some(D6)`.
   - `8/8/8/8/8/8/8/4K2k w - - 0 1` → `None` (no ep).

2. `ep_transposition_hash`:
   - Build or parse the two positions:
     `r1bqkbnr/pppppppp/2n5/8/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq e3 0 3`
     and
     `r1bqkbnr/pppppppp/2n5/8/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 0 3`.
   - Assert placement, side, and castling rights are identical.
   - `hash()` and `fen()` differ (the first prints `e3`).
   - `hash_with(Legal)` is equal and `fen_with(Legal)` is equal (`-`).
   - Assert `legal_ep_square()` is `None` for the ep-set position.

3. `legal_ep_square_respects_pins_and_blast`: construct/collect at least one
   position where an adjacent enemy pawn exists and the ep target is set but
   the ep capture is illegal because it exposes the commoner or self-explodes.
   Assert `legal_ep_square() == None` while `ep_square() == Some(...)`.

4. `hash_after_with_equivalence`: reuse the existing random-playout harness.
   For every pseudo-legal move and both modes, compare
   `hash_after_with(m, mode)` with clone + `do_move` + `hash_with(mode)`.
   Keep the existing `hash_after` equivalence test unchanged.

5. `ep_default_behavior_unchanged`: `hash()`/`fen()` still equal `_with(Always)`
   for ep-set and non-ep positions, and perft/move-order goldens are unchanged.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft`
- Re-run the feedback’s random-playout ep prevalence probe if convenient and
  report `legal_ep_square` counts (the existing 4.66 % of ep-set positions is
  the expected order of magnitude).

## Files to touch

- `src/types.rs` — `EnPassantMode` (or place it in `src/board.rs`).
- `src/lib.rs` — re-export `EnPassantMode`.
- `src/board.rs` — `legal_ep_square`, `hash_with`, `fen_with`,
  `hash_after_with`, refactored FEN writer, tests.
- `CHANGELOG.md`, `README.md`, `src/lib.rs` docs.

## Non-goals

- No change to the default hash or FEN.
- No `PseudoLegal` ep mode.
- No repetition-detection implementation; this only normalizes keys.

## Final task

Write `docs/plans/feedback/f2_report3.md` with the measured ep prevalence,
transposition before/after hashes, `legal_ep_square` correctness cases,
`hash_after_with` equivalence result, and any performance cost of the Legal
mode.
