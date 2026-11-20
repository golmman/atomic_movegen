# Wishlist 3 — a legality ask for `atomic-movegen`

**Status:** a request list, not a plan. `W10` is the only item, raised by the
plan 9 measurement ([`report9.md`](report9.md),
[`measurements/plan9/`](measurements/plan9/README.md)). **Baseline:** 2.10.0
(`5083647`, MIT), default features. **Supersedes nothing:**
[`wishlist1.md`](wishlist1.md) stays open (`W4` watchlist, `W5` only on a
key-driven regression) and [`wishlist2.md`](wishlist2.md) is closed into 2.10.0.
**Protocol** is unchanged: we own `golmman/atomic_movegen`, so an item is a
request we may implement upstream. A release enters `470M1` only by an
owner-triggered `planN.md`/`reportN.md`, a pin and `PROVENANCE.md` update, the
differential, the gates and a same-session `MD6` row. No fork, path dependency
or implicit sync.

## Priority

| # | Ask | Expected saving | Priority | 2.10.0 |
| --- | --- | ---: | --- | --- |
| `W10` | evasion-aware `LegalityContext` | 0.72e9 (measured prototype) | P1 | `legality_context` |

## W10 — evasion-aware `LegalityContext`

**Why.** The prover's candidate loop falls back to `Board::legal` 10.01M times
per `core` run, and **82.0%** of those candidates are in check, where
`LegalityContext` can only defer to the full check. The plan 9 census
(instrumented `Mover::legal`, seed 0; call counts do not depend on the
architecture) counts what an evasion mask settles:

| Cell | in-check oracle calls | quiet non-king | rejected by mask (legal) | castle/EP (legal) |
| --- | ---: | ---: | ---: | ---: |
| `core` | 8,220,516 | 6,128,196 | 5,882,026 (0) | 116,128 (0) |
| `easy-win` | 1,038,152 | 778,190 | 761,648 (0) | 444 (0) |
| `deep-win` | 61,347,947 | 47,679,180 | 45,072,205 (0) | 470,483 (21) |

Every mask rejection was illegal under the crate oracle on all three cells. On
`deep-win`, 21 castling/EP calls in check are legal; the probe does not split
them (hypothesis: en passant, which any implementation must pass to the
oracle). A prototype of this rule, wired outside the crate, measures
**−724,605,146** instructions on `core` (7,816,219,508 → 7,091,614,362,
**9.271%**), −84,243,461 on `easy-win` (5.134%) and −5,295,132,123 on
`deep-win` (6.400%), with identical counters/mate and a byte-identical `.pvr`
on all three cells, and 9/9 proofs accepted by both checkers.

**Proposal (crate-side shape, not copied from the prover).** Extend the
node-local legality context so an in-check node can reject the quiet moves
that cannot possibly answer the check. The checkers are already available to
callers in `StateInfo`; the context must **take them as input** (or from a
`&StateInfo`), never recompute them, or the extra attackers query can eat the
gain. A plausible shape, names not binding:

```rust
impl Board {
    pub fn legality_context_with_state(&self, counts: Counts, state: &StateInfo)
        -> LegalityContext;
}
```

`LegalityContext`/`LegalNode` gain the node's commoner square and one stored
`Bitboard` (the block mask). Build once per node:

- exactly **one** checker, that checker is a slider (bishop, rook or queen),
  and the two commoners do not touch → `block = between(our_commoner, checker)`;
- otherwise → `block` empty, plus a flag that the side to move is in check.

In `legal(m)`, before the existing body:

- a **quiet non-commoner** move (destination empty, not en passant, origin not
  the commoner) while in check is `false` unless `block` contains `to`;
- an empty `block` in check makes every such move `false`;
- castling while in check is `false`.

Everything else goes to the existing body unchanged: captures, en passant,
commoner moves, king moves (including a touching enemy commoner), and any
quiet move outside check. The rule **only rejects**; it never returns `true`
for a move the oracle would reject, so a wrong mask can only lose wins, and
the prototype's 0-of-5.88M rejection test is the evidence it does not.

**Acceptance.** An in-crate test over in-check positions — either generated or
from the conformance corpus — that `legal(m)` equals the crate's existing
`legal_in_view` oracle for every pseudo-legal move. It must cover: one slider
check (block inside and outside the mask), double check, knight and pawn check,
kings touching, castling in check, en passant in check, a promotion push that
blocks, and a quiet move that would open a second line to the commoner. The
cost target is the prototype's **0.72e9** whole-binary fall on `core`; the
per-node `between` and mask test are about 0.01e9 of it.

**Not promised.** The prover-side prototype reuses occupancy it already has;
a crate-side shape may recompute more and cost more. `W10` states the measured
prototype as its target, not a guarantee. The prototype itself is diagnostic
only — `MD5` forbids prover-side legality, so nothing from it is committed to
`src/`.

## Not asking for

- A prover-side fallback or a second legality implementation. If the crate
  cannot own the rule, the item stays open.
- A change to captures, king moves, en passant or atomic blast semantics.
- `unsafe-optimizations` as a dependency default; that is an owner decision
  on the build, not a crate API ask.
- An unreleased dependency: an item is usable only in a released version
  recorded by a plan and a provenance pin.
