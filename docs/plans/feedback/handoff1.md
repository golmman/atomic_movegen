# atomic-movegen upstream handoff 1 — requirements, hot-path API, release protocol

**Status:** self-contained, standalone wishlist for the upstream
`atomic-movegen` repository. A new handoff (`handoffN.md`) supersedes it; this
number is never reused.
It gives (1) the requirements a *dependent engine* places on the crate, written
from the point of view of an engine that replaces a GPL rules library with
this one, (2) two measured, additive API prototypes, (3) the remaining
wishlist, and (4) the protocol the downstream uses when a release appears.

**Baseline:** `atomic-movegen 2.4.0` (MIT). **Date:** 2026-11-13.
**Origin:** a downstream atomic-win prover profiled its hot path. It spends
15.5 µs per expansion against a hand-built reference engine's 1.42 µs per
node, ~54 % of it in move generation, legality and terminality. The 1.5×
adoption gate was missed (1.219×), so the stack is **not adopted**; the
upstream value is the API and the removed work, not a claimed speed-up. No
upstream PR has been opened and nothing here is released.

**How to read this.** Read it as a wishlist, not as a plan. §2 is the
contract behind every request: a request that
does not serve an R is an opinion, not a requirement. §3 is ready to review
today. §4 is the design of the one item that matters most (MG3) and the rest
of the list. §5 says what the downstream will do with a release, so the
upstream side knows the acceptance bar. All rules facts below are atomic
chess as the crate itself documents them; no third-party rules code was read
while writing the prototypes.

---

## 1. What a swap actually costs

An engine of this class owns seven things. Five of them are the crate's job;
two must stay outside it.

| Engine piece | Needs | Status in 2.4.0 |
| --- | --- | --- |
| position, bitboards, piece lookup | R2 | mostly present |
| FEN / UCI / SAN / JSON I/O | R1 | partly — no SAN, no piece-list construction |
| **mover: legality and move effects without building a child** | R3 | **the gap** |
| keys and child keys | R5 | present; stability unstated (MG11) |
| rules outcome | R6 | WDL only (MG10) |
| tablebase probing | outside (a separate crate, its own key space) | — |
| conformance tests against the rules | R7 | absent (MG13) |

The important conclusion is not a list of missing functions. It is this: if
the crate cannot answer "is this move legal, and what does it do to the
commoners" from the *move's effect*, then every prover-class consumer writes
its own second position representation next to `Board` — its own bitboards,
its own pins, its own hash updates. That is where the measured per-node gap
lives. The gap is a missing API, not only a slow loop.

---

## 2. Requirements (R1–R8) and the explicit non-requirements (R9)

Scenario: an engine that has been using a mature GPL rules library wants to
depend on this crate instead, keeping its search, its proof format and its
tests. What must be true for that to be a day's work?

### R1 — Interchange and parsing
A position must enter and leave without the consumer inventing an encoding.
Present: FEN in (syntax-only `from_fen`, policy-checked `from_fen_with`),
FEN out with an en-passant mode, UCI move parse and format, a board-aware UCI
lookup that resolves castling and en-passant encodings and requires legality.
**Absent:** SAN parse/format (MG8); construction and inspection from a piece
set rather than a string (MG9). Interchange must stay **data-level** — see R9.

### R2 — Position queries
Present: side to move, castling rights as bits, stored and legal en-passant
square, half-move clock, game ply, fullmove number, piece on a square,
per-colour and per-type bitboards, commoners, checkers and pins via
`StateInfo`, attackers to a square given an occupancy.
**Partial:** no typed castling-side query; no public "squares between" or
x-ray mask helper (an engine's legality shortcut needs both); `StateInfo` is
not re-exported at the crate root even though it is the hot-path type.

### R3 — Move generation, legality, and move effects
Present: legal list, pseudo-legal list, single-move legality with and without
`StateInfo`, pseudo-legal test, capture test, blast casualties, `MoveList`
with in-place `retain`, stable 16-bit move encoding, `Hash`/`Ord` on moves.
**Absent:** an early-exit stream over candidates (MG2); an early-exit stream
over *legal* moves; and — the flagship — a way to decide a move from its
effect without a board write (MG3). **Partial:** the generation order is
tested but not stated as a public contract (MG12).

### R4 — Making a move, and the null position
Present: `Clone` (copy-make is the expected pattern), `do_move`/`undo_move`
with a reusable `StateInfo`, hash and clock after a move without making it,
`hash_and_rule50_after` to classify once. **Absent:** the null counterpart of
a position — same pieces and rights, other side to move, no en-passant target
(MG1). A null-move search needs it on every node; the current workaround is a
FEN round-trip.

### R5 — Identity
Present: FSF-compatible Zobrist key, `hash_with` for the legal-en-passant
canonical form, `hash_after`/`hash_after_with` for child keys.
**Unstated:** whether the key table is stable across versions (MG11). A
consumer that migrates keeps its stored keys only if it is; a consumer with
persisted tables needs to know when to invalidate.

### R6 — Outcome and terminality
Present: WDL `Outcome` from the side to move, the literal game-over predicate,
and the rules precedence documented in full (mate outranks the clock, the
commoner-only draw, no insufficient-material table). **Absent:** *why* — a
terminal reason (extinction, no moves while attacked, stalemate, material,
clock), and a child's outcome without building the child (MG10, MG3).

### R7 — Rules documentation and conformance data
Present: the rules are documented unusually well, and perft exists.
**Absent:** an exported conformance corpus (positions → legal moves in order,
outcome, key) that a downstream can run against its own differential. Every
consumer builds this privately today; one published set would make the crate's
regression net reusable (MG13).

### R8 — Engineering contract
Present: plain data, `Clone`, no dependencies, no `unsafe` by default, a
documented feature flag whose invariants are argued. **Unstated:** `Send +
Sync` (needed by any threaded consumer), "no hidden allocation on the move
path", and the semver policy for the `#[non_exhaustive]` enums (MG14).

### R9 — Non-requirements (do not add)
- **No GPL interop, in any form** — not an optional dependency, not a trait
  impl, not a dev-dependency. An MIT crate must not depend on a GPL crate.
  Interchange with such libraries belongs to the consumer and is data-level:
  FEN, UCI, SAN, piece lists. This also rules out a "shakmaty compatibility"
  feature; the correct answer to that request is R1 + R9.
- **No tablebase probing** — a separate crate with its own file format and
  key space; all this crate owes it is a complete piece-placement view (R2).
- **No variant framework** — atomic only; the commoner is the whole rule
  difference and the crate already names it.
- **No eval, NNUE, search, threads, UCI loop, `async`, no network.**
- **No API that only one downstream lifecycle needs** — see §6.

---

## 3. Ready for upstream review

### MG1 — `Board::null_for(attacker)` (R4)

**Problem.** A null-move replay needs the same position with the other side to
move and no en-passant target. The consumer did this by serialising with
`fen_with(EnPassantMode::Legal)`, splitting/joining the FEN fields and calling
`from_fen` — an allocation and a parse on every null replay (~1.6 % of the
wall plus allocator time).

**Prototype** (in `src/board.rs`, next to the clock accessors):

```rust
/// The null counterpart of this position: same pieces and castling rights,
/// `attacker` to move, no en-passant target. Cheap in-place equivalent of the
/// FEN round-trip; the half-move clock and fullmove number are preserved, so
/// the derived game ply follows the new side exactly as the FEN path does.
///
/// The result is an ordinary `Board`: a caller that needs an ephemeral null
/// position must still keep it out of any keyed or stored state.
#[must_use]
pub fn null_for(&self, attacker: Color) -> Board { /* clone, flip side, clear
    ep_square, recompute game_ply from fullmove_number, recompute_hash */ }
```

**Differential.** A new `tests/null_for.rs` (registered with `[[test]]`, the
crate sets `autotests = false`) compares `null_for` against the FEN round-trip
on six fixtures for both attackers: three middlegame FENs, one with a legal
en-passant target, one with a 99-ply clock, and one with castling rights on
all four sides. Equality is asserted on FEN, `hash()`,
`hash_with(Legal)`, side to move, ep square, `rule50`, `game_ply` and castling
rights. The full suite, including golden move order and perft, stayed green.

### MG2 — public `for_each_pseudo_legal` (R3)

**Problem.** `generate_pseudo_legal` always materialises a `MoveList`. A
consumer that only needs the first accepted candidate (threat scan, "has any
legal move", a guide's early exit) pays for the whole list.

**Prototype.** The existing internal callback becomes public; body and order
are unchanged:

```rust
/// Walk the pseudo-legal candidate stream for the side to move, invoking `f`
/// on each candidate in generation order, stopping early if it returns
/// `ControlFlow::Break`.
///
/// Visits the same stages in the same order as `generate_pseudo_legal` (a thin
/// wrapper collecting the whole stream): pawn pushes and captures, promotions
/// and en passant per pawn → knights → bishops → rooks → queens → commoners →
/// castling. `f` must not mutate the board.
pub fn for_each_pseudo_legal<B>(
    board: &Board,
    f: &mut impl FnMut(Move) -> ControlFlow<B>,
) -> ControlFlow<B> { /* existing body */ }
```

**Tests.** The golden move-order and perft suites are the order gate; add a
differential (stream == list) and an early-exit test (`Break` after one
candidate visits exactly one). The suite stayed green with the visibility
change. **Open in this item:** whether a legal-stream sibling
(`for_each_legal`) is worth it; it would need MG3's effect path to pay off.

---

## 4. Wishlist

Priority: **P0** = the downstream's next plan needs it; **P1** = the highest
leverage; **P2** = correctness-sensitive or generally valuable; **P3** =
upstream value that the downstream does not need today. "Ready" means a
measured prototype exists.

### MG3 — node effect context + per-move effect query (R3; **P1, the flagship**)

**Problem.** Today a consumer that wants to know what a move does must build
the child (`do_move`) and run legality and terminality on it. For an
early-exit scan, a threat test or a child value, that is the dominant cost:
per-child `outcome` runs a full "has any legal move" search, and ~9 % of the
wall is `Board::legal` called on moves whose legality is decided by a pin or
a blast-square test.

**Proposed API (option A, recommended).** A position-constant effect context,
computed once per position, plus per-move queries that never write a board:

- context: checkers, pinned pieces, the mask that a move must not uncover,
  the squares from which each piece type would check the enemy commoner, the
  "between" masks for the commoner's lines (this needs `between`/x-ray masks
  public — see R2), and the blast-adjacency squares relevant to each commoner;
- per-move, all read-only: capture? (exists), blast casualties (exists),
  does the move check the enemy commoner, does it explode the enemy commoner,
  does it leave the mover with a commoner, is it legal (fast path + fallback
  to the full test).

With those, a consumer can classify every candidate without a board write and
the crate keeps one implementation of the rules. A sealed
`Board::outcome_after(m, state)` (option B) is the smaller surface: pick
explosion/check/mate from the effect and fall back to a legal search only
where stalemate is possible. It is easier to review but gives the consumer no
classification to build on, and it enlarges the crate's rules surface by one
more precedence function. **Recommend A, with B layered on top if wanted.**

**Risk:** high — this is a correctness surface. **Gate:** differential
against `Board::legal` for every pseudo-legal move over a fixture corpus, the
downstream's three bench positions and long random play-outs; differential
against `outcome` on the same set; perft identical at depth 5; and the
downstream's own veto (an independent rules implementation) green.

**Subsumes MG5.** A public context *is* the "node-constant attack data" of
MG5; keep them separate only if a measurement says the per-move accessors are
not enough.

### The rest

| ID | Proposal | R | Pri | Why | Risk | Status |
| --- | --- | --- | --- | --- | --- | --- |
| MG4 | Widen the trivial-legality ladder to captures/checks | R3 | P2 | `Board::legal` ~9 % of the wall; the fast path covers only quiet, non-capture, non-check, non-pinned moves | high | proposed (after MG3) |
| MG5 | `Board::legal` / node-constant attack data | R3 | P2 | same | high | **merge into MG3** unless measured separately |
| MG6 | `generate_pseudo_legal` micro-optimisation | R3 | P2 | still the largest single movegen symbol | medium | proposed after MG2; keep the order |
| MG7 | `from_fen`/`fen_with` performance | R1 | P3 | fallback only if MG1 does not land | low | fallback |
| MG8 | SAN parse/format | R1 | P3 | every engine with a PV or line surface needs it; the downstream does not | low | proposed |
| MG9 | Build/inspect a position from a piece set (Setup-like) | R1 | P3 | interchange with another library's types without a string round-trip; also the natural fixture builder | low | proposed |
| MG10 | Terminal reason alongside `Outcome` | R6 | P2 | WDL alone makes every consumer re-derive the rules; the rules precedence is already documented and tested | low | proposed |
| MG11 | Key contract: FSF parity, stability across versions, ep modes | R5 | P1 | a migrating consumer keeps its key space; persisted tables need an invalidation rule | low | proposed |
| MG12 | Generation order as a public contract | R3 | P2 | proofs, transcripts and golden tests depend on it | low | proposed (golden tests already exist) |
| MG13 | Conformance corpus as data (FEN → moves/outcome/key) | R7 | P2 | makes the crate's regression net reusable by every downstream | low | proposed |
| MG14 | Engineering contract: `Clone`, `Send + Sync`, no hidden allocation, semver policy | R8 | P2 | a threaded or copy-make consumer relies on all four; none is stated | low | proposed |

---

## 5. Release and adoption protocol (what the downstream will do)

A published version is adopted through exactly one integration plan in the
downstream's `movegen` initiative (`planN.md`); this section is that plan's
checklist and the upstream-side statement of the acceptance bar. For any
published version, in one session, in this order:

1. **Pin and lock.** Bump the dependency and `Cargo.lock`; confirm the crate's
   own harness passes as published (tests + perft).
2. **Golden differential.** Diff the frozen observable behaviour against the
   previous version: perft to depth 4–5, full legal move lists *in order*,
   `outcome`, `is_terminal`, `hash`/`hash_with`, `rule50`/`game_ply` after
   every legal move, on a fixed corpus. Any difference that is not an
   announced rules fix is a **veto**, not a note.
3. **Downstream gates.** `make test-full` and the independent-checker veto.
4. **Measure.** Same-session bench at the standard cap, a 10 s row, and
   per-expansion cost; three repeats, alternating builds, one process at a
   time; parsed results only.
5. **Keep or revert with data**, recorded in one line: version, gate result,
   ratios, and the reason. A regression, a divergence or a missing harness is
   a revert.

A release is never a precondition for the downstream's next plan: it may
measure with a local checkout through a measurement-only path dependency and
adopt after publication. The committed dependency is always a published
version — no fork, no `[patch]`, no committed path dependency.

---

## 6. Risks and open questions

- **Naming.** `null_for` is short but domain-specific. Alternatives:
  `with_side_to_move`, `null_move_board`. Decide before release; renaming is
  breaking.
- **`null_for` invariants.** It preserves `rule50` and the fullmove number and
  clears ep, matching the FEN round-trip. If `Board` gains cached state, the
  method must update it; keep the differential as the guard.
- **MG3 scope.** Option A risks an API that mirrors one consumer's search. The
  test: every field must be derivable from the rules alone, not from a search
  design; R3 is the bound. Option B is the conservative fallback.
- **Order contract.** Publishing the order (MG12) makes it a compatibility
  surface; say so and keep the golden test as the enforcement.
- **Callback shape.** `FnMut(Move) -> ControlFlow<B>` is allocation-free and
  supports early exit; an iterator is more ergonomic but adds a type and may
  change codegen. The callback is the measured form.
- **Re-export.** `for_each_pseudo_legal` is reachable as
  `atomic_movegen::movegen::for_each_pseudo_legal`; the crate root currently
  re-exports only types and `perft`. Decide from the existing module style —
  and note that `StateInfo` and `board` re-exports matter more for hot-path
  consumers than one function.
- **No upstream benchmark.** The measured 1.219× is a downstream workload. If
  upstream wants a performance claim, add a small benchmark or ask for a
  re-run after release.
- **Single owner on both sides.** Review is thin by construction. The
  independent checks are the golden rules differential and the external
  checker, not a second reviewer.

---

## 7. Measurement provenance

- Host: AMD Ryzen 9 5950X, 4 vCPU container, Linux; rustc 1.99.1.
- Baseline: published `atomic-movegen 2.4.0`; candidate: local checkout with
  MG1+MG2 reached through a measurement-only path dependency (never
  committed).
- Workload: downstream prover, `deep-win` FEN
  `rnb1kbnr/pppp2pp/5q2/4ppN1/3P4/8/PPP1PPPP/RNBQKB1R w KQkq - 0 1`,
  `--evals 5000000 --seed 0 --guide hand --memory 192`, release build with
  LTO, `codegen-units = 1`, `panic = "abort"`.
- Three repeats per build, alternating, one process at a time. Counters
  identical for control and candidate: `child_evals` 5,000,034; `expansions`
  168,910; `null_calls` 105,120; `null_replies` 2,782,739.

| Build | `child_evals` | Expansions | Median wall | Ratio | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| control, published 2.4.0 | 5,000,034 | 168,910 | 2.558 s | 1.0000 | 1.000× |
| S3 only (consumer-side) | 5,000,034 | 168,910 | 2.560 s | 1.0008 | 0.999× |
| S1+S2+S3 candidate | 5,000,034 | 168,910 | 2.099 s | **0.8206** | **1.219×** |

A 4 s `perf` self-time profile, control → candidate: `generate_pseudo_legal`
20.7 % → folded into the caller; `from_fen` 1.1 % → absent; `Board::legal`
9.3 % → 9.9 %; `do_move` 6.5 % → 6.5 %; `compute_checkers` 4.7 % → 6.1 %;
`immediate_explosion_bits` 6.8 % → 14.0 %. The FEN round-trip and the
materialised move lists disappear; what remains is the per-child
board/legality/terminality path — MG3/MG4, not MG1/MG2.

No third-party code was copied. The prototypes are original changes to the
MIT-licensed crate; every rules statement above is taken from the crate's own
documentation, not from another implementation.
