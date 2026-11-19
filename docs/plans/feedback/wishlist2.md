# Wishlist 2 — hot-path cost asks for `atomic-movegen`

**Status:** a request list, not a plan. **Baseline:** `atomic-movegen` 2.9.0
(`af3faf0`, MIT). **Evidence:** [`hot-path-audit.md`](hot-path-audit.md);
per-call instruction counts on the ten-FEN probe, and whole-binary runs of the
`core` cell. **Supersedes nothing:** [`wishlist1.md`](wishlist1.md) stays
open (`W4` watchlist, `W5` only on a key-driven regression).

**Protocol.** We own `golmman/atomic_movegen`, so an item is a request we may
implement upstream, not a commitment by this repository. A release enters
`470M1` only by an owner-triggered [`plan4.md`](plan4.md) / `report4.md`, a pin
and `PROVENANCE.md` update, the differential, the gates and a bench row. No
fork, path dependency or implicit sync.

**Why this list is narrower than the earlier `W6`.** The earlier ask was for "a public
view-based rules API, a batch `legal + effect + key`, and null with an advanced
clock". The first two already exist in 2.9.0 (`EffectContext`,
`post_move_and_key_with`) and were measured **slower** than the current path
(394 vs ~200 instructions per candidate; 566 for a 3-in-1 pass; +41.7% on the
whole `core` run when used for legality too). Asking for them again would send
the crate in the wrong direction. The asks below are for cheaper shapes of the
same facts.

## Priority

| # | Ask | Expected saving | Priority |
| --- | --- | ---: | --- |
| `W6` | counts-only state, or documented `legal` precondition | ~0.05e9 | P3 |
| `W7` | legality entry point at plan-2 occupancy cost | 0.3–0.4e9 | P1 |
| `W8` | effect + child key from parent bitboards, no view materialisation | ~0.4e9 | P1 |
| `W9` | null move with advanced clock, no `Setup` rebuild | ~0.1e9 | P2 |

Combined optimistic ceiling: ~0.95e9 of the 2.46e9 gap. With `plan4`'s
prover-side native-move storage and the rest of the audit's list the channel
ceiling is ~1.50e9, which lands near **1.12× the plan 2 binary** — still short
of the bar. Stated up front so no report claims more.

## W6 — counts-only state

**Why.** `Board::legal` reads only `commoners_count` and
`them_commoners_count`; `checkers` and `pinned` are used only by
`has_legal_move_with_state` and the existence screen. The prover populates the
full `StateInfo` 1.74 times per expanded node (166 instructions) where 18 would
do unless a `checkers` query follows.

**Proposal.** Either a `Board::populate_counts(&mut StateInfo)` that fills only
the two counts, or a documented statement that `legal`/`is_legal_with_state`
require only those fields, so callers can set them directly. `populate_state`
keeps its current contract.

**Acceptance.** For every candidate of the conformance corpus and the seeded
play-outs, `legal(m, &counts_only)` equals `legal(m, &full)`; the prover's
differential stays at zero; bench counters unchanged.

## W7 — legality entry point at occupancy cost

**Why.** 8.4M legality calls per `core` run at 144 instructions each is 1.21e9
— the largest single rules cost. Plan 2's occupancy check is 83. The crate's
ladder recomputes `occupied`, the capture test, the castling pass-through pair,
the en-passant victim and two commoner counts for every candidate, and reaches
the parent through a `PositionView`.

**Proposal.** One entry point with the same answer, shaped for a candidate
loop: it takes the parent's own bitboards and the two counts, handles
non-castling moves first (the common case), and exposes the occupancy it
computed so an effect query can reuse it instead of recomputing. A
`legal_is_quiet_cheap`-style fast path for "quiet, non-commoner, not pinned, no
checkers" is welcome only if it is measured and stays exact.

**Acceptance.** Exact equality with today's `legal` on the differential
corpus, fixture positions and play-outs; per-candidate instructions on the
ten-FEN probe at **≤ 100** (today 144) and no whole-binary regression. If the
ladder cannot go below ~120, say so and keep the item open.

## W8 — effect and child key from the parent bitboards

**Why.** The current adapter pays 164 instructions for
`hash_and_rule50_after` + `is_double_push` and ~50 for the occupancy, ~215 per
legal candidate. `post_move_and_key_with(Legal)` costs 394 because it
materialises a `PositionView` (72 bytes) and classifies the child before
deriving the key; a 3-in-1 variant costs 566. Plan 2's shape is 50–75:
occupancy from the parent's bitboards plus a Zobrist delta from a per-node
base.

**Proposal.** A lean child query returning only what an engine needs:

```rust
pub struct ChildEffect { pub occupied: Bitboard, pub key: u64,
                         pub rule50: u16, pub capture: bool }
impl Board {
    pub fn child_effect(&self, m: Move, counts: Counts,
                        mode: EnPassantMode) -> Option<ChildEffect>;
}
```

No `PostMove`, no view copy, no classification unless asked for; the `Legal`
en-passant canonicalisation must not re-derive the child (it already does not
for non-double pushes — keep that). A separate entry point may return
classification in the same pass for callers that want it.

**Acceptance.** Field-for-field equal to `clone + do_move + hash_with(mode)` and
to `rule50_after` on the differential corpus and play-outs; per-candidate
instructions at **≤ 120** (today ~215 in the adapter, 394 batched); no
`ZOBRIST_VERSION` change.

## W9 — null move with an advanced clock

**Why.** The prover needs a passed position whose `rule50` is one higher, so the
50-move limit is reached at the same point as after a real move. `null_for`
flips the side and clears en passant but keeps the clock, so the adapter
rebuilds a `Setup` and a `Board` for every null probe.

**Proposal.** `Board::null_for_advanced(&self, attacker: Color) -> Board` with
`rule50 + 1` and `game_ply + 1`, or a `set_rule50` that makes this a field
update. No `Setup` rebuild.

**Acceptance.** Equal to the adapter's current `null()` (side flipped, no ep,
clock + 1, same key discipline) on the fixtures; allocation-free; the prover's
`null_advances_the_clock` test keeps passing.

## Not asking for

- `EffectContext`, `PostMove`, `post_move_and_key_with`, `is_legal`: shipped,
  measured, and the wrong shape for a candidate loop. Do not extend them on the
  hot path.
- A native `u16` move encoding: `Move::to_bits`/`from_bits` already are that.
  Storing it is a prover change (`M2`), not an upstream one.
- A search, evaluation or threading API.
- An unreleased dependency: an item is usable only in a released version
  recorded by a plan and a provenance pin.
