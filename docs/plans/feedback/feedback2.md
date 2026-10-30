# Feedback for `atomic-movegen`, round 2

**From:** the `atomic_solver` project — a pure atomic proof solver plus an
offline proof-tree/proof-DB pipeline.
**Against:** released `atomic-movegen` 2.3.1 (crates.io source, unpacked and
verified directly).
**Supersedes:** `feedback.md` (written against 1.0.0) and complements
`shakmaty_analysis.md`. Round-1 items 1–4 and 6–8 shipped as requested; item 5
(`outcome()`/`is_terminal()`) exists but is unusable for the reasons in P0-1/
P0-2, so it is re-opened here rather than re-asked from scratch.

This is a wishlist, not a bug report: it ranks the changes that would remove
the most consumer-side workarounds, duplication, or risk for us. Everything
marked *verified* was reproduced against 2.3.1 with the probes in §6; nothing
here modifies the crate.

---

## 0. Priority summary

| # | Ask | Type | Why it matters to us | Size |
| --- | --- | --- | --- | --- |
| P0-1 | Ship the rules fixes described in `shakmaty_analysis.md` §7, and pin the full outcome precedence | correctness | 2.3.1 still has the touching-commoner `checkers` bug, checkmate-as-draw, and movegen on terminal boards (verified) | small |
| P0-2 | One state-aware, correctly ordered `outcome` / terminal API | correctness + API | we keep four independent copies of the same taxonomy, and cannot use `Board::outcome()` at all | small–medium |
| P0-3 | Do **not** port shakmaty/FSF insufficient-material draws; document commoner-only as the only safe material draw | correctness (pushback) | our 3-man tablebase shows K+B/K+N/K+R vs K contains real wins, so an insufficiency table would misclassify wins as draws | doc + tests |
| P1-4 | Stable `Move` wire encoding, `from_uci`/`Display`, `Hash`/`Ord` | API + perf | the proof DB persists move codes; our UCI fallback costs 1.1–1.5 µs per node | small |
| P1-5 | Single-move legality/pseudo-legality queries | API | validation sites currently build (and sometimes allocate) a full move list to test one move | small |
| P1-6 | En-passant normalization (`legal_ep_square`, hash/FEN mode) | correctness of keys | 95 % of ep-set positions carry an ep square with no legal ep capture, so they hash as distinct positions | small |
| P1-7 | `Board::validate()` / strict-FEN flags | correctness + API | we ingest FENs from a proof DB; a permissive parser is how impossible boards leak into tests | medium |
| P2-8 | Canonical blast helper | API | 4 consumer modules re-derive atomic blast semantics | small |
| P2-9 | Small conveniences | API | removes duplicated dispatch/boilerplate | trivial |
| P2-10 | Optional perf items | performance | modest; correctness should land first | small |

---

## 1. Baseline and method

* Checked the crates.io tarball for **2.3.1** (`/opt/cargo/registry/src/.../atomic-movegen-2.3.1`).
  All claims below are about that release, not about an upstream branch.
* Probes are tiny **external** crates depending on `atomic-movegen = "=2.3.1"`;
  the crate itself was not modified. Source snippets and commands are in §6.
* Environment: aarch64 container, 4 cores, rustc 1.98.1, release profile,
  `lto = "fat"`, `codegen-units = 1` (the crate's own release profile).

### 1.1 Status of round-1 feedback

All round-1 asks are nominally present in 2.3.1: `hash_after` /
`rule50_after`, `rule50()` / `game_ply()` / `fullmove_number()`,
`generate_legal_with_state`, `has_legal_move[_with_state]`, the `Move`/`MoveList`
helpers, `outcome()` / `is_terminal()` / `is_capture()`, `Option`-returning
`Piece::color()` / `type_of()`, the pseudo-royal adjacency docs, and
`attacks::init()` removed. Thanks — those removed real boilerplate from our
wrapper. The one exception is `outcome()`: it is present but returns wrong
results (P0-1), which is why P0-2 re-opens the terminal API.

### 1.2 Caveat: the `shakmaty_analysis.md` fixes are not in 2.3.1

`shakmaty_analysis.md` §7 says the touching-commoner `checkers` bug, the
checkmate-as-draw outcome, and movegen on terminal boards were fixed. They are
**not** fixed in the released 2.3.1 (see the probe table in §2.1). Either the
work is unreleased or the analysis was run against a patched local tree. The
wishlist below assumes the *released* behavior is the integration target; the
first ask is simply to close that gap, with tests that pin the whole precedence
rather than one branch.

---

## 2. P0 items

### P0-1. Ship the rules fixes and pin the full terminal precedence

Verified 2.3.1 behavior (probe in §6.1):

| FEN | expected | 2.3.1 |
| --- | --- | --- |
| `8/8/8/8/8/8/4k3/4K3 w - - 0 1` (touching commoners) | no check | `checkers = 1`, 4 legal moves |
| `7K/8/8/8/8/8/1Q6/k7 b - - 0 1` (checkmate) | `Loss` | `Some(Draw)` |
| `7K/8/8/8/8/8/1Q6/k7 b - - 100 1` (mate at rule50 100) | `Loss` | `Some(Draw)` |
| `8/8/8/8/8/8/8/K7 w - - 0 1` (opponent already extinct) | 0 legal moves | 3 legal moves, `Outcome::Win` |
| `4k3/8/8/8/8/8/8/4K3 w - - 0 1` (K vs K) | `Draw` (WDL) | `None` |
| `8/8/8/8/8/8/8/8 w - - 0 1` (no pieces) | rejected or `Loss` | accepted, `Loss` |

Please add regression tests for **all** of these in one place. Two rule
details worth writing down, because they cause the most consumer drift:

1. **Mate outranks the 50-move clock.** Fairy-Stockfish's
   `Position::is_optional_game_end` encodes this as
   `rule50 > 99 && (!checkers() || MoveList<LEGAL>(pos).size())`; a checkmate
   on the clock's last ply is a loss, not a draw. We rely on that ordering.
2. **Touching commoners are immune to every attack**, so `checkers()` is
   empty whenever the commoners are adjacent, and empty when either side has no
   commoner. This is a real consumer-visible bug for us: `move_class` derives
   its `Check` class directly from `checkers()`, so touching-commoner children
   are currently misclassified as checks and get the check starting proof
   number. (`eval_guide` independently implements the touching exclusion for
   its own hand evaluation — that stays — but it should agree with the library
   on what a check is.)

### P0-2. One state-aware, correctly ordered terminal API

This is the single most valuable API change for us. Our solver cannot call
`Board::outcome()`: it is wrong (P0-1), it always builds a `MoveList`, and it
cannot reuse a `StateInfo` or an already-known "has legal move" answer. So we
maintain four independent copies of the taxonomy, plus several call sites that
consume the wrapper:

* `src/position.rs` — `Position::outcome_from_state` (the authoritative one),
* `src/search/dfpn/children.rs` — two reordered fast paths in the child ladder,
* `src/search/preflight/region.rs` — `classify_terminal`,
* `examples/egtb_gen3/table.rs` — `classify_terminal`, with a comment saying it
  deliberately does **not** use `Board::outcome()`,
* `src/proof_tree/validate.rs`, `examples/proofdb/verify.rs`, `src/search/dfpn/pv.rs`,
  and `src/search/dfpn/cheap_probe.rs` — consumers of the wrapper above.

That is four implementations of a precedence with three ordering subtleties
(extinction, mate-vs-clock, commoner-only draw), which is exactly the shape
that already drifted once (P0-1).

Proposal (all additive):

```rust
impl Board {
    /// Correct rules/WDL precedence; allocates a MoveList internally.
    pub fn outcome(&self) -> Option<Outcome>;

    /// Same result, reusing a populated StateInfo. Still answers the
    /// moves-empty branch with the early-exit existence query.
    pub fn outcome_with_state(&self, state: &StateInfo) -> Option<Outcome>;

    /// Same result when the caller already knows whether a legal move exists
    /// (the hot-path form: no movegen, no allocation).
    pub fn outcome_with(&self, state: &StateInfo, moves_exist: bool) -> Option<Outcome>;

    /// WDL dead-material shortcut: only commoners left (atomic K vs K).
    /// Documented as equivalent to the eventual 50-move draw, not a literal
    /// "game over" rule.
    pub fn is_material_draw(&self) -> bool;
}
```

Precedence we recommend (and already implement):

1. side-to-move commoner extinct → `Loss`; opponent extinct → `Win`;
2. no legal moves: in check (`state.checkers` non-empty) → `Loss`, else `Draw`;
3. `rule50 >= 100` → `Draw`;
4. only commoners on the board (`occupied == 2`) → `Draw`.

Notes:

* Put the movegen-free branches first: branch 1 always, branch 4 always, and
  branch 3 when `state.checkers` is empty (when in check, the 50-move branch
  needs the existence query to exclude mate, exactly FSF's `!checkers() ||
  legal-move` guard). That is what makes `outcome_with_state` usable in a
  search loop.
* Keep a strict `is_game_over()`/`is_terminal()` that excludes branch 4 if you
  want a literal rules predicate; branch 4 is a WDL shortcut (a K-vs-K game
  ends by the 50-move rule, not by material rule).
* For the artificial both-extinct case, pick an order and document it. We
  check own extinction first (→ `Loss`); FSF's `is_immediate_game_end` checks
  the opponent first (→ `Win`). It is unreachable in play, but our regression
  fixtures contain such FENs.

Why it pays: `outcome()` currently costs one full `generate_legal`
(63–125 ns in our measurements). With a populated state and a known
`moves_exist`, it is 14–24 ns, and in the common non-terminal case it is a
couple of bitboard tests. More importantly, it removes four copies of a subtle
precedence that already drifted once.

### P0-3. Material draws: the only safe material draw is commoner-only

`shakmaty_analysis.md` §3.5 / follow-up 1 recommends porting
FSF/shakmaty's insufficient-material rules (K vs K, K+B vs K, K+N vs K). We
think that is wrong for atomic and would be a correctness regression if it
became an `Outcome` branch. Two reasons:

1. **Any piece that can reach the enemy commoner can win by capturing it**
   (provided the mover's own commoner is outside the blast), and the enemy
   commoner can never capture back — a commoner capture destroys the capturer,
   so it is illegal. A lone bishop with king support is therefore not
   "insufficient": e.g. `8/8/8/8/8/3B4/4k3/K7 w - - 0 1` has the immediate
   legal win `d3xe2` (black is left with no commoner). Declaring K+B vs K a
   draw would override a proven win.
2. **Our 3-man tablebase generator agrees that these are not forced draws.**
   `cargo run --release --example egtb_gen3 -- --material b --samples 0
   --prove-samples 0 --out /tmp/kb3.bin` reports 30,660 winning placements per
   strong color for K+B vs K (K+N: 18,448; K+R: 48,776; all discovered at
   1 ply, i.e. immediate captures). The remaining placements are unresolved
   (WDL draw candidates), not proven-drawn irrelevant positions.

What we would like instead:

* Document the atomic rule precisely: **there is no insufficient-material
  draw in the FIDE sense; a board with only the two commoners has WDL value
  draw** (no capture is legal for either side and no check exists, so the game
  runs into the 50-move rule). Structural dead positions (e.g. a permanently
  blocked pawn) can exist, but they are not material predicates; they are
  covered by the 50-move rule, not by a material table.
* If you add a helper, make it a conservative **cannot-win** predicate
  (`occupied == 2` / no non-commoner pieces) with a reference test table that
  includes the winning K+B/K+N/K+R placements above, so the predicate cannot be
  mistaken for shakmaty's nine-position table.
* Do not import a "K+B vs K is a draw" claim from `shakmaty`; it does not hold
  under atomic blast semantics. (FSF's own `has_insufficient_material` is
  explicitly conservative; matching that conservatism is fine — matching the
  material-set table is not.)

---

## 3. P1 items

### P1-4. `Move` as a stable wire format, plus UCI parsing/formatting

Our proof DB stores, per node, both a `move_uci` string and a 16-bit
`move_code`, and cross-checks them during merge/verify. We derive the code in
`src/notation.rs` from the public accessors because the layout is only a doc
comment and the inner `u16` is private. The DB is the long-lived artifact; a
layout or `PROMOTION_PIECES`-order change would break every stored code (the
fold catches the disagreement, but it is a hard migration).

Wish list:

```rust
impl Move {
    /// Stable wire encoding (documented as semver-stable).
    pub const fn to_bits(self) -> u16;
    /// Inverse; `from_bits(0) == Some(Move::NONE)` (document the sentinel).
    pub fn from_bits(bits: u16) -> Option<Move>;
    /// Syntax-only UCI parse (`e2e4`, `e7e8q`). Castling destinations are
    /// board-dependent, so they are resolved by `Board::move_from_uci`.
    pub fn from_uci(s: &str) -> Option<Move>;
}
impl Board {
    /// Board-aware UCI parse: resolves castling (`e1g1` → king-takes-rook
    /// encoding) and validates promotion/legality.
    pub fn move_from_uci(&self, s: &str) -> Option<Move>;
}
impl fmt::Display for Move { /* no allocation */ }
impl Move { pub fn write_uci(self, out: &mut impl fmt::Write) -> fmt::Result; }
```

Also derive (or hand-implement) `Hash`, `PartialOrd`, `Ord` on `Move`
(order via `to_bits`). This is already on our own follow-up list for
`Move`-keyed collections and canonical ordering.

Measured cost of our current fallback: `uci_to_move` generates all legal moves
and compares `to_uci()` strings — **1.1–1.5 µs per lookup** (20 and 43 legal
moves; generation alone is 66–129 ns), with one `String` allocation per
candidate move. Every proof-DB replay node and every `--moves <uci path>` /
harvest path segment pays it. A `Board::move_from_uci` should be tens of ns.

### P1-5. Single-move legality queries

`Board::legal(m, &StateInfo)` exists but needs a populated state, so validation
code that has not populated one either calls `generate_legal` or clones the
whole list. Concrete consumer sites:

* `src/reconstruct/walker.rs`: `self.pos.legal_moves_vec().contains(&best_move)`
  allocates a `Vec<Move>` to check one TT best move;
* `src/proof_tree/validate.rs` / `examples/proofdb/verify.rs`: build the full
  legal list per node (needed for AND-completeness, but a per-move check would
  simplify the Win path);
* `src/notation.rs::uci_to_move`, `Position::try_do_move`;
* a defensive guard around TT/snapshot `best_move` playback (we currently
  `do_move` an unverified recorded move in `best_move_repeats_path`).

Wish list:

```rust
impl Board {
    /// Populates state internally; no MoveList, no allocation.
    pub fn is_legal(&self, m: Move) -> bool;
    pub fn is_legal_with_state(&self, m: Move, state: &StateInfo) -> bool; // == legal()
    /// Cheap shape check for untrusted input (from/on-board piece, target
    /// reachable for that piece, promotion/EP/castling shape). No legality
    /// (self-check/blast) guarantee.
    pub fn is_pseudo_legal(&self, m: Move) -> bool;
}
```

### P1-6. En-passant normalization

`do_move` sets `ep_square` after every double push, FSF-style, and `hash()`
always includes it. Measurements on 1,511,707 random-playout positions
(§6.1):

* 8.06 % carry an ep square;
* only 0.38 % of all positions have a **legal** ep capture (4.66 % of ep-set
  positions);
* 75.8 % of ep-set positions have no adjacent enemy pawn at all (the remaining
  23.3 % have one, but the ep capture is illegal);
* so 95.3 % of ep-set positions hash as a different position from the identical
  placement reached via another move order.

Concrete demo: after `1.Nf3 Nc6 2.e4` vs `1.e4 Nc6 2.Nf3` the placement, side
and castling rights are identical, but the hashes are
`11376966775415242283` vs `7120124017209882689`; the first has `e3` and no
legal ep capture. (`Board::fen()` after `1.e4` likewise prints `e3`, which is
FSF-compatible and fine for FEN interchange — the problem is only keys.)

We do **not** claim a repetition-soundness bug: an ep-set position is
necessarily a fresh double-push position, so it cannot recur in a repetition.
The cost is lost TT transpositions (we key by `Board::hash()` plus our own
rule50 key) and inconsistent FEN-derived keys in tooling.

Wish list (either is enough for us):

```rust
impl Board {
    /// `Some(sq)` only when at least one legal ep capture exists.
    pub fn legal_ep_square(&self) -> Option<Square>;
}
```

and/or a mode on the key/FEN surface:

```rust
pub enum EnPassantMode { Always, Legal }
impl Board {
    pub fn hash_with(&self, ep: EnPassantMode) -> u64;
    pub fn fen_with(&self, ep: EnPassantMode) -> String;
}
```

If you prefer to change the default, make it an explicit major-version
decision and keep `hash_after` consistent (our `keys_after` relies on
`hash_after` matching `do_move` + `hash`).

### P1-7. `Board::validate()` / strict FEN flags

`from_fen` is intentionally permissive, which we value: we build terminal and
artificial fixtures (including multi-commoner boards in `static_tactics` unit
tests). But we also ingest FENs from the proof DB, shard sidecars, and external
checker exports, where "this is not an atomic position" should be a first-class
error rather than a silent test distortion. `shakmaty_analysis.md` §5.1 is the
right model.

Wish list:

```rust
pub struct ValidationErrors(u32); // bitflags-style, or a Vec<ValidationError>
impl Board {
    /// Opt-in; does not change from_fen's permissiveness.
    pub fn validate(&self) -> Result<(), ValidationErrors>;
}
pub enum FenPolicy { Strict, AllowImpossibleCheck, /* … */ }
impl Board { pub fn from_fen_with(fen: &str, policy: FenPolicy) -> Result<Self, FenError>; }
```

Atomic-specific checks we would use: exactly one commoner per side (or zero
for terminal fixtures, behind a flag), no pawn on rank 1/8, no promotion to
commoner (already impossible), side-not-to-move not attackable, ep square
consistent with the last move and capturable, castling rights consistent with
piece placement, piece counts reachable from a legal start. The validator must
remain **opt-in** so our artificial multi-commoner fixtures keep working.

---

## 4. P2 items

### P2-8. Canonical blast helper

Four consumer modules re-derive the blast: `search/ordering.rs` (material
delta), `search/dfpn/static_tactics.rs` (explosion targets),
`search/dfpn/quiet_group.rs` (king-explosion target shape), and
`search/dfpn/move_class.rs` (cheaply blastable). The 3×3 geometry is trivial;
the subtle parts are pawn immunity, the capturer's own destruction, en-passant
ground zero, and castling/promotion having no blast. A canonical helper would
remove drift risk:

```rust
impl Board {
    /// Squares whose non-pawn pieces a blast on `sq` destroys (3×3 minus pawns).
    pub fn blast_zone(sq: Square) -> Bitboard;
    /// Exactly what `do_move(m)` would remove via the blast, split by color,
    /// plus the captured victim square for en passant. No board mutation.
    pub fn blast_casualties(&self, m: Move) -> (Bitboard, Bitboard);
}
```

If the return shape is awkward, even `blast_zone` plus a documented
`is_blast_capture`/`blast_center(m)` pair would let us delete the duplicated
rules comments.

### P2-9. Small conveniences

* `attacks::attacks_from(pt, color, sq, occupied)` — we duplicate this exact
  match in `search/ordering.rs` and `search/dfpn/eval_guide.rs`; sliders,
  knight, king, pawn are already public.
* `Board::attackers_to_color(sq, color, occupied)` — `attackers_to` returns
  both colors and every consumer call site intersects immediately.
* `Move::is_zeroing()` (rule50 reset) and `Move::is_double_push()` —
  `rule50_after` covers the first, but the predicate is useful for heuristics.
* `MoveList::retain` — our in-place compaction in `generate_legal_with_state`
  is fine; a helper would just shorten consumer code.
* Document the max legal-move count you observe (we see 62; standard chess tops
  at 218) and keep the debug assertion in `MoveList::push`. A release-mode
  silent drop would be a nasty failure mode; if 256 is provably unreachable,
  say so.

### P2-10. Optional performance items

These are real but small for us, because movegen is no longer the dominant cost
after our lean plans (child-key prefetch and the TT pre-probe removed most
per-child make/unmake).

* **Unchecked magic index** behind an `unsafe-optimizations` feature (the
  analysis measured ~1.5 % on this aarch64 host). We would take it behind a
  feature; we would not want it on by default in a crate we audit.
* **Combined child key**: `hash_after` + `rule50_after` both classify the move;
  a single `(hash, rule50)` accessor would remove duplicated work in our
  per-child non-mutating pre-pass. Expected gain ~1–2 ns/child; cheap to add.
* **Lazy `populate_state`**: `has_legal_move` currently pays `checkers` +
  `pinned` up front (8.7 ns on our m22 fixture) even when the first candidate
  is trivially legal. `TrivialLegality` needs `pinned` for `safe_origins`, so
  this is not a trivial change; mentioning it only as a measured split, not a
  request we need.

Measured splits on our reference positions: `populate_state` 8.7–31.9 ns,
`has_legal_move_with_state` 5.4–14.8 ns, `generate_legal` 63–125 ns,
`outcome()` 63–125 ns. The last line is the argument for P0-2.

---

## 5. Not on our wishlist (and why)

* **`generate_captures` / quiescence generators.** Our solver has no
  quiescence search; leaf classification needs a *legal-move existence* answer,
  not a capture list, and our static-tactics closure consumes the frame's
  existing legal list. It may be useful to engines, but it is not a lever for
  us.
* **SAN parsing/formatting, Chess960 castling, stacking variants.** Out of
  scope; UCI is the interchange format for the proof DB and external checkers.
* **`Arbitrary`/`serde`/`bincode`, board symmetry transforms, Miri hooks.**
  We already have seeded-playout equivalence tests and our own binary format;
  we do not want the dependencies.
* **Hyperbola-quintessence fallback.** Only interesting for a small-footprint
  or no-`unsafe` build; we are not asking for one.
* **Random perft file.** That is upstream test coverage, not a consumer need;
  the two programfox positions already helped us.
* **A big rules-model rewrite.** We explicitly like the current shape: packed
  moves, `&mut MoveList`, `StateInfo`, incremental hash. Do not trade that for
  `shakmaty`-style cloning or generality.

---

## 6. Reproduction

### 6.1 Behavior probes (P0-1, P0-3, P1-4, P1-6)

A throwaway crate:

```toml
# Cargo.toml
[package]
name = "probe"
version = "0.1.0"
edition = "2021"
[dependencies]
atomic-movegen = "=2.3.1"
```

```rust
use atomic_movegen::board::{Board, StateInfo};
use atomic_movegen::movegen::generate_legal;
use atomic_movegen::MoveList;

fn describe(fen: &str) {
    let b = Board::from_fen(fen).unwrap();
    let mut moves = MoveList::new();
    generate_legal(&b, &mut moves);
    let mut state = StateInfo::new();
    b.populate_state(&mut state);
    println!("{fen}: outcome={:?} legal={} checkers={} occupied={}",
        b.outcome(), moves.len(), state.checkers.count(), b.occupied().count());
}

fn main() {
    describe("8/8/8/8/8/8/4k3/4K3 w - - 0 1");   // touching commoners
    describe("7K/8/8/8/8/8/1Q6/k7 b - - 0 1");     // checkmate
    describe("7K/8/8/8/8/8/1Q6/k7 b - - 100 1");   // mate at rule50 100
    describe("8/8/8/8/8/8/8/K7 w - - 0 1");        // opponent extinct
    describe("4k3/8/8/8/8/8/8/4K3 w - - 0 1");     // K vs K
    describe("8/8/8/8/8/8/8/8 w - - 0 1");        // no pieces at all
}
```

Observed on 2.3.1:

```
8/8/8/8/8/8/4k3/4K3 w - - 0 1: outcome=None legal=4 checkers=1 occupied=2
7K/8/8/8/8/8/1Q6/k7 b - - 0 1: outcome=Some(Draw) legal=0 checkers=1 occupied=3
7K/8/8/8/8/8/1Q6/k7 b - - 100 1: outcome=Some(Draw) legal=0 checkers=1 occupied=3
8/8/8/8/8/8/8/K7 w - - 0 1: outcome=Some(Win) legal=3 checkers=0 occupied=1
4k3/8/8/8/8/8/8/4K3 w - - 0 1: outcome=None legal=5 checkers=0 occupied=2
8/8/8/8/8/8/8/8 w - - 0 1: outcome=Some(Loss) legal=0 checkers=0 occupied=0
```

K+B vs K counterexample to "insufficient material":

```
fen = "8/8/8/8/8/3B4/4k3/K7 w - - 0 1"
d3e2 is legal
after Bxe2: black_commoners=0, black outcome=Loss  (white wins immediately)
```

UCI-lookup cost (`uci_to_move` workalike: generate + compare `to_uci()`):

```
20 legal moves: 1108 ns/lookup (55.4 ns/move); generation only 66 ns
43 legal moves: 1483 ns/lookup (34.5 ns/move); generation only 129 ns
```

En-passant prevalence (20,000 seeded random walks, 120 plies each):

```
positions=1,511,707
ep_set=121,851 (8.060%)
ep_with_legal_capture=5,682 (0.376% of positions, 4.66% of ep-set positions)
ep_with_no_adjacent_enemy_pawn=92,326 (75.77% of ep-set positions)
ep_adjacent_pawn_but_no_legal_capture=28,439 (23.34% of ep-set positions)
ep_without_legal_capture=116,169 (7.685% of positions, 95.34% of ep-set positions)
```

Transposition demo:

```
1.Nf3 Nc6 2.e4   hash 11376966775415242283  ep = e3, legal ep moves = 0
1.e4   Nc6 2.Nf3 hash 7120124017209882689   ep = None
same placement, same side, same castling rights
```

### 6.2 Cost probes (P0-2, P2-10)

Same external crate, timing loops over the start position, an m22-like
middlegame, and a transposition-dense position:

```
--- rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
  populate_state: 31.9 ns
  has_legal_move_with_state: 8.3 ns
  has_legal_move (incl populate): 16.8 ns
  generate_legal (incl populate): 63.1 ns
  outcome(): 63.4 ns
--- 4r2k/3p4/2pB2p1/p6p/5pPP/2N1PP2/P1PP4/1R4RK w - - 0 22
  populate_state: 8.7 ns
  has_legal_move_with_state: 5.4 ns
  generate_legal (incl populate): 124.5 ns
  outcome(): 124.5 ns
--- 3r3k/2rB3P/p7/P4p2/1p3Pp1/1P4P1/2p1p3/2R1R2K b - - 5 41
  populate_state: 8.8 ns
  has_legal_move_with_state: 14.8 ns
  generate_legal (incl populate): 78.0 ns
  outcome(): 77.8 ns
```

### 6.3 Three-man material tables (P0-3)

```sh
cargo run --release --example egtb_gen3 -- --material b --samples 0 --prove-samples 0 --out /tmp/kb3.bin
cargo run --release --example egtb_gen3 -- --material n --samples 0 --prove-samples 0 --out /tmp/kn3.bin
cargo run --release --example egtb_gen3 -- --material r --samples 0 --prove-samples 0 --out /tmp/kr3.bin
```

```
[table b] win=[30660,30660] loss=[0,0] draw=[469308,469308] max_dtm=1pl  (K+B vs K)
[table n] win=[18448,18448] loss=[0,0] draw=[481520,481520] max_dtm=1pl  (K+N vs K)
[table r] win=[48776,48776] loss=[0,0] draw=[451192,451192] max_dtm=1pl  (K+R vs K)
```

(The generator uses the solver's terminal taxonomy, counts wins per strong-side
color, and reports unresolved WDL states as draw candidates; the point here is
only that each ending contains real one-ply wins, so a material-set draw rule
would be wrong.)

---

## 7. Summary of concrete API additions we would consume

If you only take five things, take these:

1. Release the P0-1 fixes (touching `checkers`, mate, terminal movegen) with a
   precedence test suite.
2. `Board::outcome_with(&StateInfo, moves_exist: bool)` (plus the corrected
   `outcome()`), and a documented commoner-only material shortcut.
3. `Move::to_bits`/`from_bits` (stable), `Board::move_from_uci`,
   `impl Display for Move`, `Hash`/`Ord`.
4. `Board::is_legal` / `Board::is_pseudo_legal`.
5. `Board::legal_ep_square()` and `Board::validate()` (opt-in).

Everything else on the list is cheaper for us to keep working around than a
bad default would be to undo — especially the insufficient-material table.
