# What can we learn from `shakmaty`?

**Question:** `shakmaty` is apparently less efficient than this project
(`docs/shakmaty_comparison.md`). Can we still learn something from its
implementation?

**Short answer: yes — a lot, but almost nothing about raw speed.** Its atomic
move generator is slow *by design* (it clones the position once per
pseudo-legal move to filter legality), so there is no hot-loop trick worth
copying. What is worth reading is everything around that loop: how it models
the rules, how it separates parsing from validation, how it tests, and how it
shapes its API.

This is a corrected version of an earlier draft. That draft treated positions
with several commoners per side as reachable and drew rule conclusions from
them. They are not reachable:

* Atomic chess gives each side exactly **one** commoner (the king-like piece).
* Pawns promote only to knight, bishop, rook, or queen. There is no promotion
  to commoner and there are no drops.
* Therefore a legal board has one or two commoners in total, and a board with
  only one commoner is **terminal**.
* Consequently, a move may never destroy the mover's own commoner. There is no
  “spare commoner” exception, because a spare commoner cannot exist.

Fairy-Stockfish's `extinctionPseudoRoyal` implementation is generalized so it
can also serve variants with several royal pieces (for example three-kings) and
so it can tolerate artificial FENs. That generalized “last commoner” logic is
not part of atomic play. All multi-commoner FENs and differential results from
the earlier draft have been removed from this document and from the regression
tests.

What remains are the issues that are reachable in real atomic positions:

1. **`Board::checkers` disagreed with move generation about touching
   commoners.** Legal move generation correctly treated two touching commoners
   as immune, but `checkers()` reported a check. **Fixed.** (The earlier draft
   described this as a move-generation bug involving a “spare commoner”; that
   description was wrong.)
2. **`Board::outcome` called every no-legal-moves position a draw.** In atomic
   chess a side whose commoner is attacked and has no escape *loses*
   (Fairy-Stockfish's `stalemate_value` for `extinctionPseudoRoyal`). The
   existing `test_outcome_draw_stalemate` fixture was actually a loss.
   **Fixed.**
3. **Terminal boards still generated moves.** A board with only one commoner is
   over; Fairy-Stockfish's `generate<LEGAL>` returns an empty list.
   **Fixed** in `generate_legal` / `has_legal_move` and in `Board::legal`.

Material draws (`Board::outcome` still ignores insufficient material) remain a
known gap.

---

## 1. Method

* Downloaded and read `shakmaty` 0.30.2 (16,834 lines under `src/`), focusing
  on `position.rs` (the `Atomic` impl and the generic movegen helpers),
  `board.rs`, `attacks.rs`, `bitboard.rs`, `m.rs`, `fen.rs`, `san.rs`,
  `uci.rs`, `zobrist.rs`, `perft.rs`, `tests/` and `benches/`.
* Built the C++ oracle (`Fairy-Stockfish`, `ARCH=armv8`) and used it for all
  correctness claims. Note that FSF's `d` command prints an empty `Checkers:`
  line for atomic (the variant has no `KING` piece type, so `checkersBB` is
  always zero); the authoritative signals are `go perft` and the search's
  `info depth 0 score ...` line.
* Built a small harness against the vendored `shakmaty` sources (it needs
  `arrayvec`, `bitflags`, `btoi`) to compare legal-move sets three ways.
* Differential testing used random **reachable** positions with exactly one
  commoner per side, at depths 2–3, plus hand-picked corner cases. Artificial
  FENs with several commoners were excluded because they do not describe atomic
  chess.
* Benchmarked the changes and one performance idea from `shakmaty` with the
  same `[profile.release]` as the crate (`lto = "fat"`, `codegen-units = 1`).

All harnesses live outside the crate; `atomic-movegen` stays zero-dependency.

---

## 2. Why `shakmaty` is slow (and what not to copy)

The core of its `Atomic` move generation (`src/position.rs:1535`):

```rust
// Atomic move generation could be implemented more efficiently.
// For simplicity we filter all pseudo legal moves.
moves.retain(|m| {
    let mut after = self.clone();
    after.play_unchecked(*m);
    if let Some(our_king) = after.board().king_of(self.turn()) {
        (after.board.kings() & after.board().by_color(!self.turn())).is_empty()
            || after.king_attackers(our_king, !self.turn(), after.board.occupied()).is_empty()
    } else {
        false
    }
});
```

and `perft` clones once more per node (`src/perft.rs:29`). There is no public
undo API, so a user cannot avoid the clone pattern. This is the dominant
explanation of the 2.35× perft gap; it is not a data-structure or codegen
issue.

Things that are *worse* than ours and should not be copied:

| Aspect | `shakmaty` | `atomic-movegen` |
| --- | --- | --- |
| `Move` | 8-byte enum (`Normal`/`EnPassant`/`Castle`/`Put`) | packed `u16` (2 bytes) |
| `MoveList` | `ArrayVec<Move, 450>` = 3,608 bytes | `[Move; 256]` + len = 520 bytes |
| Legality | clone + play + attack query per pseudo-legal move | virtual-occupancy bitboard test + trivial fast path |
| Make/unmake | clone per node (no undo) | `do_move`/`undo_move` with saved `StateInfo` |
| Scope | 8 variants, Chess960 castling, `no_std`, feature matrix | one variant, standard castling only |

Its `king_attackers` also applies unconditional adjacency immunity, which is
correct for standard atomic as long as each side has one commoner. The
multi-commoner branch is irrelevant here: `Atomic::from_setup` rejects positions
with more than one king anyway.

---

## 3. Correctness lessons (the valuable part)

### 3.1 The one-commoner invariant and touching immunity

Atomic chess has exactly one commoner per side. There is no mechanism to gain a
second one. This collapses FSF's “pseudo-royal only while it is the side's last
commoner” logic to a simpler rule for atomic:

* Every commoner on a non-terminal atomic board is that side's only commoner.
* A move may not leave it attacked and may not destroy it.
* If the two commoners touch, neither can capture the other: the blast would
  destroy the capturer's own commoner. No other enemy piece can capture our
  commoner either, because that blast would also destroy the enemy's own only
  commoner. The commoner is therefore immune to **all** attacks while touching
  the enemy commoner.
* If the commoners do not touch, the enemy commoner cannot attack ours (it only
  moves one square), so ordinary slider/knight/pawn attacks determine check.

`Board::compute_checkers` must therefore return empty when the commoners are
adjacent. The old implementation added the adjacent enemy commoner as a checker
even though `Board::legal` correctly allowed the position. Legal move generation
and `checkers()` disagreed.

Reachable evidence (FSF `go perft 1` / search score):

| FEN | FSF | old `atomic-movegen` | fixed |
| --- | --- | --- | --- |
| `8/8/8/8/8/8/4k3/4K3 w - - 0 1` (touching commoners) | 4 moves, no check | 4 moves, *reported check* | 4 moves, no check |
| `8/8/8/8/8/8/4k3/r3K3 w - - 0 1` (rook + touching commoners) | 4 moves (immune to the rook) | 4 moves | 4 moves, no check |
| `8/8/8/8/8/8/5k2/4K2R w K - 0 1` (castling through an adjacent enemy commoner) | 14 moves incl. `e1g1` | 14 moves incl. `e1g1` | 14 moves incl. `e1g1` |
| `8/8/8/8/8/8/8/K7 w - - 0 1` (only one commoner left) | **0 moves** (terminal) | generated moves | **0 moves** |

**Fix** (`src/board.rs`): `compute_checkers` returns empty if either side has no
commoner and applies touching immunity directly. `legal` uses the commoners that
survive the move (so a capture can blast an adjacent commoner away), rejects any
move that destroys our own commoner, and returns no moves at all for a terminal
board. Regression tests cover touching commoners, immunity to a rook, and
castling through the enemy commoner's adjacent square.

### 3.2 `checkers()` semantics

FSF's public `checkers` for atomic is always empty (see §1), so the useful
reference is `Position::checked_pseudo_royals` (`src/position.cpp:1013`), which
returns the *squares* of checked pseudo-royals, and `stalemate_value`
(`src/position.h:916`), which uses the same logic to distinguish “no moves and
attacked” (loss) from “no moves and safe” (stalemate draw).

Our `Board::checkers` keeps the more useful “attacker pieces” convention. For
reachable atomic positions it now follows the rules above: empty when the side
has no commoner, empty when the enemy has no commoner, and empty when the
commoners touch.

### 3.3 Game-over positions

A board with only one commoner is terminal. FSF's `generate<LEGAL>`
(`src/movegen.cpp:510`) returns an empty list when `is_immediate_game_end()` is
true (extinction: either side has no commoner). Our
`generate_legal`/`has_legal_move`/`Board::legal` now do the same using the
commoner counts already in `StateInfo`.

FSF's `stalemate_value` (`src/position.h:916`) then distinguishes:

* no legal moves + commoner attacked → **loss** (mate),
* no legal moves + commoner safe → **draw** (stalemate).

Our `Board::outcome` now matches. The old test fixture
`7k/6PP/8/8/8/8/8/7K b` (“black is boxed in”) is actually a loss: the g7 pawn
attacks h8, so it is not a stalemate. A true atomic stalemate is
`7k/8/8/8/8/8/nn6/Kn6 w`, which FSF scores `cp 0` (draw) and our `outcome`
reports as `Draw`.

### 3.4 Self-destruction

Because each side has exactly one commoner, a move that would destroy the
mover's own commoner is always illegal. There is no “destroy one of several
commoners and keep playing” case. If a capture destroys the enemy's only
commoner while the mover's commoner survives, the mover wins immediately. If the
blast would destroy both commoners, the move is still illegal because it
destroys the mover's own commoner.

`Board::legal` therefore rejects any move that reduces the mover's commoner
count, not merely moves that leave it with zero. In a legal atomic position the
two checks are identical; the stronger form also prevents an artificial,
unreachable multi-commoner FEN from silently enabling partial
self-destruction. The earlier draft's “multi-commoner self-destruction”
paragraph and its test fixture were based on an impossible position and have
been removed.

### 3.5 Material draws are still missing

`Board::outcome` handles extinction, the 50-move rule and stalemate, but not
insufficient material. FSF treats K vs K, K+B vs K and K+N vs K as draws (and
has colour-bound-bishop cases); our `outcome` returns `None` for all of them.
`shakmaty` has a compact, well-tested atomic-specific implementation
(`src/position.rs:1575`) with nine reference positions in its test suite.
**Caveat:** the two references disagree on several endings — FSF treats a rook
as a major piece with mating potential, and two knights as sufficient, while
`shakmaty` says a lone rook and two knights cannot mate a bare commoner. If we
add material draws, use FSF's conservative semantics (`src/apiutil.h:366`,
which never claims a draw when a win might exist) and add tests for the
disputed endings.

---

## 4. Testing lessons

`shakmaty`'s test suite is a good model for a rules library:

* **Perft suites as data files.** `tests/atomic.perft` uses an EPD-like format
  (`id` / `epd` / `perft depth nodes`) and contains, besides the start position,
  two “programfox” atomic positions and three atomic-Chess960 castling
  positions. We adopted the two programfox positions into
  `tests/perft_tests.rs` (values verified against FSF to depth 6):

  | Position | d1 | d2 | d3 | d4 | d5 | d6 |
  | --- | --- | --- | --- | --- | --- | --- |
  | `rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1` | 40 | 1,238 | 45,237 | 1,434,825 | 50,504,249 | 1,625,390,453 |
  | `rn1qkb1r/p5pp/2p5/3p4/N3P3/5P2/PPP4P/R1BQK3 w Qkq - 0 1` | 28 | 833 | 23,353 | 714,499 | 21,134,061 | 658,055,520 |

  (The 960 positions are out of scope: our crate only supports standard
  castling.)
* **A second implementation as a differential oracle.** `shakmaty` itself is
  useful for move-set comparison on reachable positions with one commoner per
  side. It cannot represent artificial multi-commoner positions, which is
  correct — those are not atomic positions. FSF remains the oracle for terminal
  positions and outcomes.
* **`arbitrary`-based fuzzing.** `shakmaty` implements `Arbitrary` for its
  vocabulary types and positions and has `cfg(fuzzing)` hooks. Our random
  playout tests (`has_legal_move_equivalence_random_playouts`) are the same
  idea without the dependency. They should keep generating only reachable
  positions; a strict `Board::validate()` would make that guarantee explicit.
* **Exhaustive attack-table tests.** `shakmaty`'s
  `test_sliding_attacks_exhaustive` iterates every subset of every slider mask
  via `Bitboard::carry_rippler` and compares the table lookup with the
  ray-casting reference. Our `magic.rs` tests do the same with an explicit
  `subset = subset.wrapping_sub(mask) & mask` loop — equivalent, no need to
  change.
* **Golden Zobrist values.** `tests/zobrist.rs` + `tests/zobrist.csv` check
  128-bit hashes against a reference file. Our Zobrist keys are internal
  (`splitmix64`) and not intended to match other engines, so a reference CSV is
  not applicable; the existing `recomputed_hash` round-trip tests cover the
  important property.
* **Miri awareness.** `#[cfg_attr(miri, ignore)]` on expensive tests; worth
  adding if we ever run Miri on the crate.
* **Benchmarks.** `shakmaty` uses `gungraun`/`criterion` with node-count
  assertions; our `examples/` + `docs/perf/` workflow already serves the same
  purpose without dependencies.

A cheap addition we did *not* make: a generated `tests/random_atomic.perft`
with a few hundred random FSF-verified positions at depths 1–3. Our
`tests/moves.md` checks depth-1 move sets; a perft file would additionally
exercise make/unmake and blast bookkeeping over random reachable positions.

---

## 5. API and modelling lessons

These are the places where `shakmaty` is genuinely better designed than us,
independent of speed:

### 5.1 Separate syntax parsing from semantic validation

`shakmaty` parses a `Fen` (syntax only) into a `Setup`, then calls
`FromSetup::from_setup` which validates and returns `PositionError<P>` with a
`PositionErrorKinds` bitflags set (`EMPTY_BOARD`, `PAWNS_ON_BACKRANK`,
`MISSING_KING`, `TOO_MANY_KINGS`, `OPPOSITE_CHECK`, `IMPOSSIBLE_CHECK`,
`INVALID_EP_SQUARE`, `INVALID_CASTLING_RIGHTS`, `TOO_MUCH_MATERIAL`, …). Callers
choose how strict to be (`ignore_invalid_ep_square`, `ignore_impossible_check`,
`ignore_too_much_material`, or `strict`).

Our `FenError` only covers *syntax*; `Board::from_fen` happily accepts
impossible positions (no commoner, pawns on the back rank, the side not to move
in check, an ep square with no pawn, and even several commoners per side). That
permissiveness is useful for constructing terminal test positions, but it is
also how the earlier draft ended up treating impossible multi-commoner boards as
real. A `Board::validate()` returning a `ValidationError`/bitflags (without
changing `from_fen`'s permissiveness) would be a small, high-value addition.
For atomic it should explicitly reject more than one commoner per side, since
there is no legal way to reach such a position.

### 5.2 En passant square semantics

`shakmaty` has `EnPassantMode::{Always, PseudoLegal, Legal}`
(`src/types.rs:184`) and `legal_ep_square()` / `pseudo_legal_ep_square()`. This
matters for hash consistency: two positions that differ only in a
non-capturable ep square should hash the same in a transposition table.

Our `do_move` sets `ep_square` after every double push and `hash()` always
includes it, so a double push with no adjacent enemy pawn changes the hash
even though no ep capture exists. FSF does the same (`st->epSquares` after a
double push), so this is not an oracle mismatch — but it is a real
engine-integration gotcha. At minimum, document it; better, add
`Board::legal_ep_square()` and/or an `EnPassantMode` parameter to `fen()` /
`hash()`.

### 5.3 Outcome and game-end predicates

`shakmaty`'s `Outcome::Known(Decisive { winner } | Draw) | Unknown` is more
explicit than our `Option<Outcome { Win, Loss, Draw }>` (which is relative to
the side to move). It also offers `is_check`, `is_checkmate`, `is_stalemate`,
`is_insufficient_material`, `is_game_over`, and `is_variant_end`. In atomic,
`is_checkmate` is not meaningful, but `is_stalemate` (no moves, not attacked)
and `has_insufficient_material` are. Our `outcome()` now covers the first; the
second is §3.5.

### 5.4 Selective move generation

`shakmaty`'s `Position` trait exposes `capture_moves()`, `en_passant_moves()`,
`castling_moves()`, `promotion_moves()`, `san_candidates(role, to)` and
`evasions()` in addition to `legal_moves()`. For a search engine, a
**capture-only generator** is the important one (quiescence search), and in
atomic it is especially valuable because captures trigger the blast. Our crate
only has all/pseudo-legal/legal/has-any. Adding
`generate_captures(board, &mut MoveList)` (and perhaps `generate_evasions`)
would be a natural, self-contained feature: the existing
`for_each_pseudo_legal` walk already has the target masks needed to restrict to
`them_bb` plus en passant/promotions.

### 5.5 Move parsing and formatting

`shakmaty` has `uci::UciMove` (parse/format), `san::{San, SanPlus}` (full SAN
with disambiguation, checks, castling, promotions) and `Move`'s `Display` in
long algebraic notation. We only format UCI (`Move::to_uci`). A
`Move::from_uci(&Board, &str)` is cheap and useful for FEN/move test data; full
SAN parsing is a bigger, optional feature.

### 5.6 Small conveniences worth stealing

* `Move::is_zeroing`, `is_conversion`, `is_capture`, `is_promotion`,
  `castling_side`, `to_mirrored` — we have some of these on `Board`, but not on
  `Move`.
* `Board::attacks_from(sq)` and `Board::attacks_to(sq, attacker, occupied)` —
  our `attackers_to(sq, occupied)` returns both colours; a colour-filtered
  variant saves an intersection at call sites (and mirrors the signature we
  already use internally in `square_attacked_by`).
* `Board::flip_vertical` / `flip_horizontal` / `flip_diagonal` / `rotate_180` /
  `swap_colors` / `mirror` — useful for symmetry property tests (perft must be
  invariant under mirroring and colour swap) and for opening books.
* `MoveList::retain` — we do in-place compaction manually in
  `generate_legal_with_state`; a `retain` helper would make that code shorter
  without changing performance.
* `#[must_use]` and rustdoc examples with ASCII board diagrams — our public
  API is already well documented; the diagrams are a nice touch for
  `attacks.rs`-style docs.

### 5.7 Things that are not applicable to us

* `build_move_list` (`src/m.rs:254`) uses `MaybeUninit` + an `#[inline(never)]`
  helper so that returning a `MoveList` by value benefits from RVO. Our API
  takes `&mut MoveList`, so there is no return copy to eliminate.
* `ArrayVec`, `bitflags`, `btoi`, `serde`, `bincode`, `arbitrary`,
  `nohash-hasher` — all dependencies we do not want.
* The generic `Position` trait, 8-variant support, Chess960 castling, and the
  `no_std`/feature matrix — generality we deliberately do not pay for.

---

## 6. Performance lessons (measured)

### 6.1 `unsafe { assert_unchecked }` on the magic index

`shakmaty`'s magic lookup (`src/attacks.rs:290-306`) skips the bounds check with
`unsafe { assert_unchecked(idx < ATTACKS.len()) }` and notes: *“Omitting bounds
checks is worth about 2% in move generation and perft.”* Our `magic.rs` uses a
plain checked index into a flat table.

Experiment: a copy of the crate with

```rust
let i = e.offset as usize + idx as usize;
debug_assert!(i < BISHOP_TABLE.len());
// SAFETY: every occupancy subset maps into the entry's range (verified by
// the exhaustive table tests).
unsafe { *BISHOP_TABLE.get_unchecked(i) }
```

(and the same for rooks), benchmarked on four positions (start position,
positions 2, 13 and 33 from `tests/perft_values.md`), best of 5, same release
profile:

| Build | d4 (4,792,859 nodes) | d5 (164,672,294 nodes) |
| --- | --- | --- |
| baseline | 180.4 Mn/s | 169.0 Mn/s |
| with correctness fixes | 180.8 Mn/s | 170.3 Mn/s |
| + unchecked magic index | **183.7 Mn/s** | **172.8 Mn/s** |

So the fixes are performance-neutral (slightly positive, because
`compute_checkers` now has a simple no-adjacency fast path), and the unchecked
index is worth **~1.5%** on this machine — consistent with `shakmaty`'s claim.
The crate already contains one `unsafe` block (`board.rs:1486`), so the
question is policy, not feasibility. Options: (a) take the 1.5%, document the
safety argument (already proven by the exhaustive table tests), or (b) put it
behind an `unsafe-optimizations` feature so the default build stays
unsafe-free.

### 6.2 Hyperbola quintessence as a no-unsafe fallback

`shakmaty` supports both fixed-shift magics and hyperbola quintessence
(`src/attacks.rs:313`), selected by the `magics` feature. Its docs
(`src/lib.rs:47`) note magics gain *“~20% perft on x86, ~5% on ARM”* — i.e. on
ARM (our benchmark machine class) the gap is small. Hyperbola needs only a few
KiB of tables versus our **341 KiB** magic tables (38,404 rook + 5,248 bishop
entries), and no `unsafe`. If we ever want a small-footprint or
no-unsafe build, porting that ~100-line module is straightforward. Not worth
changing the default.

### 6.3 Ideas that do not help

* `Bitboard::for_each` / `carry_rippler` are convenient, but our explicit
  `while !b.is_empty() { let sq = b.pop_lsb(); }` loops compile to the same
  code.
* `const fn`-generated tables: we already do this (`magic.rs`, `zobrist.rs`,
  `attacks.rs`).
* `#[inline]` on hot accessors: we already do this.
* Packed `Move`: ours is already 2 bytes; `shakmaty`'s enum is 8.
* `MoveList` capacity: ours is 256 (520 bytes) vs `shakmaty`'s 450 (3,608
  bytes). The largest legal-move count we observed in random atomic positions
  was 62; standard chess tops out at 218. 256 is fine, but a debug assertion
  or a comment documenting the bound would be cheap insurance.

---

## 7. Changes made in this pass

| File | Change |
| --- | --- |
| `src/board.rs` | `compute_checkers`: one-commoner semantics; touching enemy commoner returns no check; terminal boards have no checkers. `legal`: rejects all moves on a terminal board, rejects any destruction of the mover's commoners, and uses post-blast surviving commoners for the attack check. `outcome`: no moves + attacked commoner = loss, otherwise draw. `from_fen` docs describe the permissive parser and the one-commoner invariant. New/updated regression tests; removed invalid multi-commoner tests. |
| `src/movegen.rs` | `generate_legal_with_state` / `has_legal_move_with_state`: return no moves when either side has no commoner (terminal), matching FSF. Updated self-explosion wording. |
| `src/types.rs` | Documented that each side has at most one commoner and no promotion to commoner; added a test that `PROMOTION_PIECES` excludes the commoner. |
| `src/lib.rs`, `README.md`, `docs/shakmaty_comparison.md` | Corrected the rule description: one commoner per side, no promotion to commoner, touching immunity, terminal one-commoner boards. |
| `tests/perft_tests.rs` | Two programfox positions from `shakmaty`'s `tests/atomic.perft`, verified against FSF to depth 6. |
| `CHANGELOG.md` | Unreleased section describing the fixes, documentation correction, and additions. |

Verification performed:

* `cargo test` — 66 unit tests plus all integration and doc tests pass.
* `cargo clippy --all-targets`, `cargo fmt --check`, `cargo doc` — clean.
* `cargo run --release --example verify_perft 6` — **41/41 positions pass at
  depths 1–6**, so none of the reachable positions changed.
* Differential vs FSF on reachable positions (one commoner per side):
  * 300 random positions, depth 2 → **0 mismatches**;
  * 200 random positions, depth 3 → **0 mismatches**.
* Benchmark: the fixes are neutral-to-positive; unchecked magic index +1.5%.

---

## 8. Recommended follow-ups (priority order)

1. **Material draws in `Board::outcome`** (correctness). Port FSF's
   `has_insufficient_material` semantics (`src/apiutil.h:366`), use
   `shakmaty`'s nine atomic test positions as a starting point, and decide the
   disputed endings explicitly (FSF: K+R vs K and K+NN vs K are sufficient;
   `shakmaty`: a lone rook / two knights cannot mate).
2. **`Board::validate()` / strict FEN loading** (correctness/API). Return a
   bitflags-style set of problems, keep `from_fen` permissive, and include the
   atomic-specific checks: at most one commoner per side, no commoner promotion
   moves, and consistent terminal states. This would make impossible
   multi-commoner positions unreachable and is the single most transferable
   design idea from `shakmaty`.
3. **`generate_captures`** (feature/performance for search). Restrict the
   existing pseudo-legal walk to `them_bb | ep_square` and promotions; useful
   for quiescence search, where atomic captures are the main branching factor.
4. **`Move::from_uci`** (ergonomics). Cheap; makes FEN/move test data and
   examples easier to write. Full SAN parsing is optional and much larger.
5. **`EnPassantMode` / `legal_ep_square()`** (engine integration). Or at least
   document that the hash includes a non-capturable ep square.
6. **Unchecked magic index** (performance, +1.5%). Behind a feature or with a
   documented safety argument.
7. **Hyperbola-quintessence fallback** (portability). Only if a small-footprint
   or no-unsafe build is wanted.
8. **Random atomic perft file** (testing). A few hundred FSF-verified reachable
   positions at depths 1–3, in the `tests/*.perft` style.
9. **Symmetry property tests** (testing). `Board` mirror/colour-swap plus
   perft invariance, if the board transformations are added.

---

## Appendix A — Reproduction

```sh
# Oracle build (aarch64; on x86 use ARCH=x86-64)
cd Fairy-Stockfish/src && make -j4 ARCH=armv8 SUPPORTED_ARCH=true build

# Our numbers
cargo test
cargo run --release --example verify_perft 6
cargo run --release --example perft \
  "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1" 6
cargo run --release --example perft \
  "rn1qkb1r/p5pp/2p5/3p4/N3P3/5P2/PPP4P/R1BQK3 w Qkq - 0 1" 6

# FSF perft for a position (synchronous, one process per position)
printf 'setoption name UCI_Variant value atomic\nposition fen <FEN>\ngo perft 6\nquit\n' \
  | Fairy-Stockfish/src/stockfish | grep 'Nodes searched'
```

The differential harness (outside the crate, zero-dep constraint) generates
random reachable FENs with exactly one commoner per side, computes perft with
`atomic_movegen`, and compares against FSF `go perft N`.

## Appendix B — Exact corner-case FENs

All FENs below are reachable atomic-style positions except the explicit
terminal examples, which have only one commoner left and are therefore already
over.

```
# 1 vs 1, touching: no check, 4 legal moves (immune to the rook too)
8/8/8/8/8/8/4k3/4K3 w - - 0 1
8/8/8/8/8/8/4k3/r3K3 w - - 0 1

# castling through f1, which is adjacent to the black commoner on f2: legal
8/8/8/8/8/8/5k2/4K2R w K - 0 1

# no legal moves + attacked commoner = loss (FSF: mate 0)
7k/6PP/8/8/8/8/8/7K b - - 0 1
# no legal moves + safe commoner = stalemate draw (FSF: cp 0)
7k/8/8/8/8/8/nn6/Kn6 w - - 0 1

# terminal: only one commoner on the board, so no legal moves
8/8/8/8/8/8/8/K7 w - - 0 1
8/8/8/8/8/8/8/k7 w - - 0 1
```

An earlier version of this appendix also listed 1-vs-2 and 2-vs-1 commoner
FENs. Those are not atomic positions and have been removed.
