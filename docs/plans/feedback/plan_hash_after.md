# Plan: `Board::hash_after` / `Board::rule50_after` — child keys without make/unmake

Standalone plan for the `atomic-movegen` repository
(https://github.com/golmman/atomic_movegen, current release 2.2.0).
Written by the `atomic_solver` project, which consumes this crate. Follow
the movegen repo's own AGENTS.md and conventions wherever they conflict
with anything suggested here.

## Background (why the consumer needs this)

The solver evaluates every child of a searched node, and each child
evaluation starts with a transposition-table lookup keyed by the child's
Zobrist hash. The table is far larger than the caches, so each lookup is
a DRAM miss. The consumer now overlaps those misses by computing every
child's key up front and issuing cache prefetches before evaluating any
child. Today the only way to get a child's key is `do_move` + `hash()` +
`undo_move`, and that pre-pass costs ≈12% of the consumer's wall time —
it performs the full board mutation (squares, bitboards, blast loop,
undo record) twice per move just to read 8 bytes.

A pure, non-mutating key computation removes most of that cost. It must
live in this crate because the hash scheme (`zobrist.rs`, piece/side/
castling/en-passant keys) and the move semantics (blast zone, castling
rights cascade, en-passant square) are this crate's.

## Scope

Additive only. Target release: **2.3.0** (semver-minor; no existing
signature, behavior, hash value, or move-order change).

New public API on `Board` (`board.rs`):

```rust
/// Zobrist hash of the position after `m`, without modifying `self`.
///
/// Exactly equal to `{ let mut b = self.clone(); b.do_move(m, &mut st); b.hash() }`
/// for every move `m` that `do_move` accepts in this position (every
/// pseudo-legal move). Cheaper than a `do_move`/`undo_move` round-trip:
/// no board, bitboard, or undo-record writes.
#[must_use]
pub fn hash_after(&self, m: Move) -> u64;

/// Half-move clock after `m`, without modifying `self`.
///
/// Exactly equal to the `rule50()` value `do_move(m, ..)` would leave:
/// castling increments it; otherwise a pawn move or a move onto an
/// occupied square resets it to 0, and any other move increments it.
#[must_use]
pub fn rule50_after(&self, m: Move) -> u16;
```

### Equivalence contract (the correctness gate)

For every position reachable in the tests and every pseudo-legal move
`m` (not only legal ones — the consumer computes keys for its legal list,
but the stronger contract is cheap to test):

- `board.hash_after(m) == hash after board.do_move(m, ..)`
- `board.rule50_after(m) == rule50 after board.do_move(m, ..)`
- `board` is unchanged (it takes `&self`; nothing to check at runtime).

### Hash components to reproduce (mirror `do_move` exactly)

Starting from `self.hash`:

1. Side key: always XOR `ZOBRIST.side`.
2. Castling move: XOR the commoner and rook from/to piece keys
   (`castling_squares`), then the castling-rights transition old → new
   (the side's both rights cleared), then the en-passant transition
   old → none. (`do_move` returns early for castling; no blast.)
3. En passant: XOR out the captured pawn on its square.
4. Normal capture: XOR out the captured piece on `to`.
5. Mover: promotion → XOR out the pawn on `from`, XOR in the promoted
   piece on `to`; otherwise XOR the mover from `from` and to `to`.
6. Blast (captures incl. en passant): for every square in
   `(king_attacks(to) & !pawns_after_capture & occupied_after_capture) | to`
   that is occupied after steps 3–5, XOR out the piece there (this
   includes the capturer at ground zero). **Pin the occupancy/pawn
   bitboards to the exact intermediate state `do_move` uses** (it
   computes the blast zone *after* removing the captured piece and moving
   the capturer) — the property test catches any deviation.
7. Castling rights: the new rights are `update_castling_rights(from, to,
   ..)` followed, on captures, by the rook/commoner-presence cascade on
   A1/H1/E1/A8/H8/E8 **evaluated on the post-blast board**. Implement the
   cascade as a pure function of (old rights, from, to, is_capture,
   post-blast occupancy of those six squares) and make `do_move` call the
   same function, so the two paths cannot drift. XOR
   `castling[old] ^ castling[new]`.
8. En passant: XOR out the old ep file key (if any) and XOR in the new
   one (double pawn push → square behind the pawn).

The post-blast piece presence needed by step 7 can be derived from the
blast set without materializing a board: a corner rook / E-file commoner
survives iff its square is not in the blast set and was not `from`
(moved away) — but follow whatever is simplest to prove equal; the
shared pure helper is the requirement, not a particular formula.

### Required tests

- **Property test** over seeded random playouts (reuse the
  `has_legal_move` equivalence harness: same start FENs incl. the
  capture-rich and promotion-heavy ones, 16 seeds × 300 plies): at every
  visited position, for **every pseudo-legal move**, compare
  `hash_after`/`rule50_after` against a cloned board's `do_move`. Pin the
  visited-position count like the existing test does.
- **Fixtures** (each asserting both functions against `do_move`):
  kingside and queenside castling for both colors; en-passant capture
  whose blast removes a piece; promotion with and without capture
  (all four promotion pieces); a capture whose blast removes a corner
  rook (castling-right loss via the cascade); a capture whose blast
  removes the e1/e8 commoner (both rights lost); a double pawn push
  creating an ep square next to an existing one being cleared; a rule50
  reset by a pawn move, by a capture, and an increment by castling.
- Existing perft/order goldens unchanged (`do_move` refactor for the
  shared castling helper must not change any output).

### Performance target

Micro-benchmark (the crate's bench convention) on a middlegame and the
consumer's reference position
`4r2k/3p4/2pB2p1/p6p/5pPP/2N1PP2/P1PP4/1R4RK w - - 0 22`, all legal
moves: `hash_after` ≤ **1/3** of a `do_move`+`undo_move` round-trip
(the consumer measured ≈7–8 ns per round-trip on this position). Quiet
moves should be a handful of XORs and table loads; captures pay for the
blast-set iteration only.

## Out of scope

- Any change to `do_move`/`undo_move` behavior or `StateInfo`.
- A combined "key + terminal classification" API; the consumer still
  plays the move for children it actually evaluates.

## Deliverables

Release 2.3.0 with the two functions, the property test, fixtures, the
micro-benchmark numbers in the changelog/PR description, and an update
note the consumer can link from `docs/plans/movegen/report_update_2.3.0.md`.
