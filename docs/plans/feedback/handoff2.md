# atomic-movegen upstream handoff 2 — the child-view terminality ask (MG15–MG18)

**Status:** self-contained, standalone wishlist for the upstream
`atomic-movegen` repository. It supersedes
[`handoff1.md`](handoff1.md) as the live wishlist; every MG item of handoff1
that has not landed stays requested (its text is not repeated here). A new
handoff (`handoffN.md`) supersedes this one; a number is never reused.

**Baseline:** `atomic-movegen 2.6.0` (MIT). **Date:** 2026-11-13.
**Origin:** a downstream atomic-win prover adopted MG3 (2.6.0:
`Board::effect_context`, `EffectContext::{effect, is_legal, outcome_after}`)
and measured *what it actually decides* on the prover's product stack. The
answer, over 1,067,066 built children: **0.00 %**; the ceiling of any
effect-based status on the same tree is **0.08 %**. MG3's shortcuts
(extinction, the commoner-only draw, the non-check clock) are correct and
inert; the whole fallback share (100 %) is the one question the crate answers
by cloning and playing the move. This handoff asks for exactly that.

Nothing here is a plan or a promise; it is an ask with evidence and an
acceptance bar. No upstream PR has been opened.

---

## 1. Requirement (R6 restated, narrowed)

R6 asked for terminality and *why*. 2.6.0 answered the "why" for the
*current* position (`outcome`, `Terminal`) and answered the *child* question
only where the effect short-circuits. What a prover-class consumer needs is
the child's terminal state **without a board write**, because the child board
is the last per-child board write on its hot path:

- its legal move list is already in hand (`generate_legal_with_state` on the
  parent), so legality is not the question;
- its keys, clocks and blast casualties are already available
  (`hash_and_rule50_after`, `blast_casualties`);
- the remaining question is **exactly** "does the child have a legal move,
  and if not, what is it — mate, stalemate, extinction, material, clock?"

The consumer's own measurement, on its shipped guide stack: 100 % of the
children it builds are built for that question, 0 % for anything else; a
perfect answer would let it drop the build for 1,067,066 of 1,067,066
children in that share (`report29` §2 of the downstream). Separately, its
`MoveEffect`-only adopt path decided 0.00 % because the extinction children
it *could* decide never occur in that tree — so the value is entirely in the
fallback.

## 2. The ask

### MG15 — child-view terminality (R6; **P1, the flagship**)

**Proposal.** One query that answers the child's terminal state without a
board write, and that never lies:

```
/// Terminal state of the position reached by `m`, from the child's
/// side-to-move perspective, without building the child.
///
/// `None` = the child is not decided (the game continues).
pub fn EffectContext::terminal_after(
    &self, board: &Board, m: Move, state: &StateInfo,
) -> Option<Terminal>;           // Terminal{outcome, reason}: MG10
```

**Acceptance.** For every pseudo-legal move of the crate's own corpus, the
downstream's frozen fixture and long random play-outs:
`terminal_after(board, m, state)` equals
`{ let mut b = board.clone(); b.do_move(m, &mut st); b.terminal() }`,
including the precedence (extinction → material → no-moves → clock) and the
`from`/counter bookkeeping; perft identical; the existing
`outcome_after` differential unchanged.

**Implementation note (upstream's choice).** The "does the child have a legal
move" part is the hard half. Two shapes are plausible and either would
satisfy the ask: (a) generate the child's moves on a *view* (placement
bitboards + side + ep) with early exit, or (b) a legality-existence test
specialised for "the side to move has only these pieces", which is what the
fallback needs. The downstream does not need the child's move list, only the
answer.

### MG16 — a two-level `outcome_after` (R6; **P2**)

**Problem.** `outcome_after` returns `None` for *both* "illegal move" and
"legal, undecided child", and its doc says only the former. Any consumer that
trusts the doc must re-derive legality; one that trusts `None` as "open"
turns a bug into a lost win.

**Proposal.** `child_outcome(board, m) -> AfterState` with
`AfterState::{Illegal, Undecided, Terminal(Terminal)}`, or the same
information as `Option<Result<Terminal, Undecided>>`-style naming upstream
prefers. **Acceptance:** the illegal test is exact for pseudo-legal input
(equal to `Board::legal`), and the undecided arm is `Terminal`-free.

### MG17 — the post-move piece summary on `MoveEffect` (R3; **P2**)

**Problem.** A consumer's own *material* rule is not the crate's (the
downstream applies the broader Lichess atomic insufficient-material rule,
which `atomic-movegen` deliberately does not implement), so it must screen
every child for its own clause; today it can only do so from the parent's
bitboards plus `casualties`, i.e. by re-deriving the post-blast placement.

**Proposal.** Expose the summary the effect path already computes:
`MoveEffect::post_occupancy` / `post_by_color` / `post_by_type` (or a
`PostMove` newtype with `pieces_color`, `pieces_pt`, `occupied`). Read-only,
no rules opinion added. **Acceptance:** equal to the moved board's bitboards
on the full corpus (this is a content check, not a rules one); zero cost when
unused (`#[inline]` accessors over the existing struct).

### MG18 — `effect` without the legality ladder (R3; **P3**)

**Problem.** `MoveEffect::effect` always runs `is_legal_from` (an
`is_attacked_by` for the mover's commoner). A consumer that classifies moves
taken from `generate_legal_with_state` has already paid for legality and
discards the result.

**Proposal.** `EffectContext::effect_trusted(board, m) -> MoveEffect` with
`is_legal` documented as "not evaluated" (or `MoveEffect` without the field
plus a separate `is_legal`). **Acceptance:** byte-identical fields to `effect`
for legal input; the legality differential unaffected.

## 3. What the downstream will do with a release

Same protocol as handoff1 §5 (D-MG6): a published version only, an
integration `planN.md` in `docs/plans/movegen/` with the gate written before
data, a `reportN.md` recording each MG outcome, and no path dependency, fork
or unreleased crate in a commit. The prover-side gate for MG15 is already
written and cheap: re-run the committed Phase A driver
(`measurements/plan29/probe.py`) — the decided share must exceed 10 % of the
built children on the hand stack before any wall measurement is authorised.

## 4. Non-asks

Nothing about the search, evaluation, tablebases, threading or GPL interop
(handoff1 R9 stands unchanged). No API that exists only to serve one
downstream's search design: MG15–MG18 are all statements about *positions and
moves*, testable inside the crate.
