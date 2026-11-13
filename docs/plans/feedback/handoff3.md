# atomic-movegen upstream handoff 3 — the terminality ask, re-priced

**Status:** self-contained, standalone wishlist for the upstream
`atomic-movegen` repository. It supersedes [`handoff2.md`](handoff2.md) as the
live wishlist; every MG item of handoff1/handoff2 that has not landed stays
requested (its text is not repeated here). A new handoff (`handoffN.md`)
supersedes this one; a number is never reused.

**Baseline:** `atomic-movegen 2.7.0` (MIT). **Date:** 2026-11-13.
**Origin:** handoff2's MG15–MG18 **all shipped** in 2.7.0 and the downstream
prover measured what MG15 costs before integrating it — and declined the
integration. This handoff is the delta: MG15's *soundness* bar was met, its
*cost* was never on the bar, and the cost is the whole question. Nothing here
is a plan or a promise; no upstream PR has been opened.

---

## 1. What shipped, and why the answer is not the end of the ask

2.7.0 ships handoff2 §2 in full: `EffectContext::terminal_after`,
`child_outcome`/`AfterState` (MG16), `PostMove`/`post_move` (MG17) and
`MoveEffect::classification` (MG18). Everything landed against soundness
acceptance bars (differential against built children, precedence, pseudo-legal
input, the illegal/undecided split) — and those bars were met. A prover-class
consumer now has the *right* answers available without a board write.

The downstream's measurement (1,248 random play-out positions from its own
bench FENs, 39,665 legal moves, one `populate_state` + one `EffectContext` per
node, best of 5, five runs) says the board-free answers cost more than the
board they were meant to replace, on exactly the class of child that needs
them (*built-ish*: capture, promotion, en passant, castling or commoner move;
7,864 children):

| Path, ns per child | built-ish | captures |
| --- | ---: | ---: |
| today: `clone` + `do_move` + `outcome()` + `Legal` hash | 57.5 | 64.8 |
| build only (`clone` + `do_move`) | 32.0 | 42.4 |
| **MG15: `hash_and_rule50_after` + `terminal_after`** | **65.8 (1.15×)** | **83.3 (1.29×)** |
| `hash_and_rule50_after` alone | 18.2 | — |
| `ctx.post_move` (`analyze`) alone | 36.2 | — |
| `ctx.terminal_after` alone | 62.2 | — |
| `ctx.child_outcome` | 66.4 | — |
| `ctx.has_legal_move_after` | 61.8 | — |

Mechanism, and the reason it is not a tuning problem:

- `terminal_after` **re-derives** the post-move view (`analyze`) and only then
  runs the view movegen, so it pays the derivation *in addition to* the
  build it competes with — `terminal_after` alone already costs today's whole
  path (62.2 vs 57.5).
- The view movegen is **not cheaper** than `do_move` + the board movegen
  (≈26 ns against ≈23 ns, and it starts with an empty `StateInfo` to
  populate).
- `terminal_from(&PostMove)` is **private**, so a consumer holding a
  `PostMove` cannot ask it for the terminal state; combining `post_move` with
  terminality pays `analyze` twice. And `terminal_from` is the single
  implementation behind `terminal_after`, so the best amortised composite a
  consumer can assemble **is** `terminal_after`.

The one measurable payoff the crate could still offer a prover is therefore
not "no board write" but "no *second* derivation and no double movegen".

## 2. The ask, narrowed to two items (both P2)

### MG19 — the terminal state on `PostMove` (and the key next to it)

**Proposal.** Make the terminality of an already-derived child accessible on
the summary, without re-analyzing:

```
impl PostMove { pub fn terminal(&self) -> Option<Terminal>; }
```

ideally with the post-move key already on the same summary
(`PostMove::key()` / `hash_and_rule50()`), so one `analyze` yields key and
terminality together. **Acceptance:** for every pseudo-legal move of the
corpus, `post_move(b, m).unwrap().terminal()` equals
`b.clone().do_move(m, st) → terminal()`, same precedence as `terminal_after`;
zero behaviour change to `terminal_after`/`child_outcome` (they call it);
`post_move` + `terminal()` must be measurably **cheaper than `terminal_after`**
on the above corpus (the point of the item) and cheaper than
`clone`+`do_move`+`outcome()` on captures.

The downstream's target: `post_move`+`terminal()` ≈ 36 + ≈26 = **≈62 ns against
57.5** at current view-movegen cost — i.e. **still not a win**. MG19 is worth
asking only if the item is paired with a cheaper existential test (MG20), and
even then the ceiling is 1.15× on a convertible share the downstream's guide
keeps near 0 % (see §3).

### MG20 — a *specialised* existential-legality test (handoff2 shape (b))

**Problem.** `has_legal_move_after` costs the same as `terminal_after`
(61.8 vs 62.2) — it does not behave like an existential query: nothing in the
cost suggests a first-candidate early exit. What the fallback needs is
"does *this* side, with *these* pieces, have any legal move?", answerable from
the view's placement + checkers/pins + commoner attack set.

**Proposal.** The child-side existential test, early-exit by construction, with
the commoner-attack and self-blast rules handled once (a move that explodes
the mover's own commoner is not a legal move; a commoner move that leaves it
attacked is not legal). **Acceptance:** equal to
`!has_legal_move_in_view(..)` on every corpus child, including the
only-commoner-and-pawns children and the touching-commoner case; and a
per-child cost **≤ 12 ns** on the built-ish class — the value at which
`post_move` (36.2) + terminality (12) + a free `PostMove` key could reach
≈0.84× today's 57.5 ns path.

**Shape note.** The downstream's own candidate for this is a *sufficient*
screen (find one demonstrably legal quiet move: no check, no pin, no commoner
contact) rather than an exact test; an exact existential is the crate-shaped
version of the same idea and belongs upstream.

## 3. What the downstream will do — and the honest ceiling

The prover's product stack reads the child board for the **guide** on every
built child (hand evaluation, check/blast classification, the one-ply settle),
so a board-free terminality cannot move its tree or its wall at all today; the
realisable share is ~0 %. MG15 is closed downstream
(`measurements/plan30/`, item 37, D42) and the child build stays its
terminality path.

A future integration is authorised only against a **measured** pair:
MG19 + MG20, gated as one stack (5 M `deep-win` seed 0, counters identical,
median wall ≤ 0.95× control, then 120 s × seeds 0–2 + the proof veto). Without
a published version that clears the two bars above, no plan is written — a
re-proposal from the crate side without numbers re-opens a measured no-go.

## 4. Non-asks

Nothing about search, evaluation, tablebases, threading or GPL interop
(handoff1 R9 unchanged). MG16–MG18 need nothing further at their current cost:
`child_outcome` (66.4) and `classification` are the right shapes, only too
expensive to *replace* a build — and the downstream uses them where it already
has the board. MG16 and MG18 cost the downstream 0 ns: its `Mover`, legality
ladder and check/blast classification are its own code, so the crate's legality
and classification opinions are not on the product path at all. No API that
exists only to serve one downstream's search design: MG19/MG20 are statements
about positions and moves, testable inside the crate.
