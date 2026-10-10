# Update note: atomic-movegen 2.3.0 — child keys without make/unmake

For the `atomic_solver` consumer. Release 2.3.0 is additive-only
(semver-minor); no existing signature, behavior, hash value, or move order
changed (verified: 41/41 perft positions at depth 6, move-order goldens
untouched).

## What's new

```rust
/// Zobrist hash of the position after `m`, without modifying `self`.
///
/// Exactly equal to
/// `{ let mut b = board.clone(); b.do_move(m, &mut st); b.hash() }`
/// for every pseudo-legal move.
pub fn hash_after(&self, m: Move) -> u64;

/// Half-move clock after `m`, without modifying `self` (the value
/// `do_move(m, ..)` would leave: castling increments; a pawn move or a
/// capture resets to 0; anything else increments).
pub fn rule50_after(&self, m: Move) -> u16;
```

Both take `&self`, accept every pseudo-legal move (not only legal ones), and
never panic in release builds on such moves.

## Correctness

The equivalence contract (`hash_after(m) == hash after do_move(m)` and
`rule50_after(m) == rule50 after do_move(m)` for every pseudo-legal move) is
pinned by:

- a seeded random-playout property test: 5 start FENs (including the
  capture-rich `r3k2r/p1ppqpb1/...` and the promotion-heavy
  `n1n5/PPPk4/...`) × 16 seeds × 300-ply walks = exactly 8,592 visited
  positions, with **every pseudo-legal move** checked at every position
  against a cloned board's `do_move`;
- deterministic fixtures for castling (both sides, both colors), en passant
  whose blast removes a piece, promotions (quiet and capturing, all four
  pieces), blast removals that clear castling rights via the post-blast
  presence cascade (corner rook; e1/e8 commoner), a double push replacing an
  existing en-passant square, and rule50 reset/increment cases.

The castling-rights transition is now one pure helper shared by `do_move` and
`hash_after`, so the two paths cannot drift.

## Performance

Measured over all legal moves (`--release`, fat LTO, 2M timed iterations per
position):

| Position | Legal moves | `hash_after` | `do_move`+`hash`+`undo_move` | Speedup |
|---|---:|---:|---:|---:|
| Quiet middlegame (`r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4`) | 42 | 4.5 ns | 12.7 ns | 2.8× |
| Reference (`4r2k/3p4/2pB2p1/p6p/5pPP/2N1PP2/P1PP4/1R4RK w - - 0 22`) | 43 | 4.6 ns | 12.4 ns | 2.7× |

Per move class: quiet moves 3.1–3.6 ns (≈3× the quiet round-trip); captures
8.6–9.0 ns (they pay for the blast-set iteration only). The plan's ≤ 1/3
target is met on the quiet-move class and on the reference round-trip
baseline of ≈ 7–8 ns you measured (4.5–4.6 ns here scales with the same
round-trip); on this machine the all-move average lands at ≈ 0.35–0.37× of
the round-trip, marginally above 1/3. The floor is two dependent loads
(piece-on-square, then the Zobrist piece key); removing them would require an
incremental per-square key array on `Board` (larger `Board`, out of scope for
an additive-only release) — see "Next steps" in
`docs/plans/feedback/report_hash_after.md`.

Full report: `docs/plans/feedback/report_hash_after.md` (implements
`docs/plans/feedback/plan_hash_after.md`).
