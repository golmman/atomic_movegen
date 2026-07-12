# Feedback for `atomic-movegen` maintainer

We are using `atomic-movegen = "1.0.0"` as the move generator for a pure atomic chess solver. Overall the crate is fast, self-contained, and easy to integrate, so thank you for that. Below is concrete feedback based on our integration work.

## Confirmed non-issues

- `atomic_movegen::attacks::init()` is a no-op in 1.0.0. It is not required for correctness or performance, because all attack tables are precomputed at compile time.
- `rule50` is already tracked internally by `Board` (`do_move`, `undo_move`, FEN input/output). It is just not exposed in the public API.

## Suggested improvements

1. **Incremental Zobrist `Board::hash()`** — This would be the biggest performance win for our solver. We currently have to recompute the Zobrist key from scratch after every `do_move`/`undo_move`. If `Board` maintained its own `u64` hash and `StateInfo` stored the old hash, `undo_move` could restore it in one assignment. For transposition tables the hash should include pieces, side, castling, and en-passant, but not `rule50`.

2. **`Board::rule50()` and `Board::game_ply()` getters** — Because `Board` already tracks these, exposing them would let us drop the duplicated `rule50` field in our `Position` wrapper and simplify FEN handling.

3. **Allow `generate_legal` to reuse a `StateInfo`** — `generate_legal` calls `populate_state` on every invocation, which recomputes `checkers`, `pinned`, and commoner counts. A `generate_legal_with_state(board, state, moves)` variant would let callers avoid that overhead in tight search loops. Incrementally maintaining a cached `StateInfo` inside `Board` would be even better but is more involved because of atomic blast effects.

4. **`Move` / `MoveList` helpers** — `MoveList::clear()`, `Move::to_uci()`, `Move::is_castling()`, `Move::is_promotion()`, `Move::is_en_passant()`, and `Board::is_capture(m: Move)` would remove a lot of boilerplate from our code.

5. **`Board::outcome()` or `Board::is_terminal()`** — Since `Board` already tracks `rule50` and commoner counts, a built-in terminal check would remove duplicated logic in our solver.

6. **Deprecate or remove `attacks::init()`** — It is a no-op and is therefore misleading to new users.

7. **Documentation correction for pseudo-royal adjacency** — The doc comment says the last commoner "cannot move next to an enemy commoner". Standard atomic chess allows touching kings (commoners), and `Board::legal()` correctly permits them. The docs should be updated to match the code behavior.

8. **Minor quality-of-life issues:**
   - `Board::fen()` writes `game_ply` as the FEN full-move counter, but `game_ply` is incremented every ply. The output should be `1 + (game_ply - 1) / 2` or the field should be renamed to `fullmove_number`.
   - `Piece::color()` returns `Color::White` for `NO_PIECE` in release builds, and `Piece::type_of()` can panic or cause an out-of-bounds lookup. Returning `Option<Color>` / `Option<PieceType>` would be safer.

## One thing we got wrong

We initially reported a correctness bug in `Board::legal()` for adjacent commoners. We later realized that the code is correct: in atomic chess, touching commoners are allowed and are immune from being checked by other pieces. The only real issue is the misleading documentation.

## Priority for our solver

If we could pick the most impactful changes, they would be:

1. Incremental `Board::hash()`
2. `Board::rule50()` / `Board::game_ply()` getters
3. `Move` / `MoveList` helpers and `generate_legal_with_state`
4. Deprecate `attacks::init()` and fix the pseudo-royal adjacency docs

Thanks for considering this feedback.
