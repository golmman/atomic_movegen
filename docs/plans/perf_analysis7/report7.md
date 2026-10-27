# Report 7 — Node-Hoisted Trivial-Legality Fast Path

**Plan:** `plan7.md` (this directory)
**Verdict:** **KEPT** — `src/board.rs`, `src/movegen.rs`, `src/types.rs` changed.
#13 median **−2.27 %** (7/7 pairwise wins), #14 median **−2.15 %** (7/7 pairwise wins),
perft 41/41, all goldens unchanged.

---

## 1. Step 0 — status of `analysis.md` §8 and why this plan exists

`analysis.md` §8 is **closed**. Its remaining items were measured by `report6.md` §6 at
0.07–0.26 % of runtime (`slider_blockers`/empty-board sliders, the `occupied` field, the castling
fast path). They are targets an order of magnitude too small to pay for themselves and must not be
scheduled.

Plan 7 is therefore a **profile-derived** item, not an `analysis.md` §5 item: it takes
`report6.md` §6 item 2, the `is_move_trivially_legal` predicate (7.84 % / 7.23 % self time), whose
commoner line alone was 5.6 % of #13. Nothing in `analysis.md` was rewritten; this report is the
record.

| Prior plan | Item | Result |
|---|---|---|
| 1 | §5.1 perft fast path | reverted |
| 2 | §5.2 local bitboard cache | reverted |
| 3 | §5.4 boolean `square_attacked_by` early exit | **kept** |
| 4 | §5.3 category-specialized legality | reverted |
| 5 | §5.5 table-driven pawn generation | **kept** (−6.9 %) |
| 6 | §5.6 `StateInfo` commoner cache | reverted |
| **7** | **profile item: trivial-legality predicate** | **kept** (−2.3 % / −2.2 %) |

---

## 2. Step 1 — re-baseline and gate profile (same session, pre-change tree)

`cargo build --release --examples` on the Plan-5/Plan-6 (unmodified `src/`) tree.

### 2.1 Fixed-FEN baseline timings

| #13 run | ms | | #14 run | ms |
|---|---|---|---|---|
| 1 | 10508 | | 1 | 1351 |
| 2 | 10551 | | 2 | 1348 |
| 3 | 10558 | | 3 | 1350 |
| 4 | 10529 | | 4 | 1353 |
| 5 | 10580 | | 5 | 1343 |
| 6 | 10570 | | 6 | 1354 |
| 7 | 10495 | | 7 | 1451 |
| **median** | **10551** | | **median** | **1351** |

Node counts: #13 `2 160 817 389`, #14 `245 931 633` (both correct).

### 2.2 Gate profile (`perf record -F 997 -g --call-graph dwarf`, #13 depth 6)

11 572 samples, ≈55.24e9 cycles. `--no-children -g none --sort=srcline,symbol`:

| Source line | Share | Meaning |
|---|---|---|
| `types.rs:789` | 7.70 % | `MoveList::push` len increment |
| `types.rs:788` | 2.08 % | `MoveList::push` store |
| `types.rs:787` | 2.00 % | `MoveList::push` capacity branch |
| **`board.rs:1589`** | **5.22 %** | `board.piece_on(from) == make_piece(us, Commoner)` |
| `board.rs:0` | 5.18 % | line-less bin inside `perft` |
| `movegen.rs:0` | 4.14 % | line-less bin inside `perft` |
| `movegen.rs:293` | 4.11 % | filter loop: read candidate |
| `board.rs:1397` | 3.93 % | `Board::legal` |
| `board.rs:1435` | 3.56 % | `Board::legal` |
| `movegen.rs:295` | 2.97 % | filter loop: store survivor |
| **`board.rs:1600`** | **2.29 %** | capture rejection branch |

Symbol level: `perft` 73.13 %, `Board::legal` 25.48 %, `__memset_avx2` 1.34 %.

**Gate result:** predicate = `board.rs:1589` (5.22 %) + `board.rs:1600` (2.29 %) = **7.51 %**, and the
commoner line alone is 5.22 % ≥ 3 %. The gate (`≥ ~5 %` for the predicate, `≥ ~3 %` for the commoner
line) **passes**, so the plan proceeds.

### 2.3 `perf annotate` on the line-less bins

`perf annotate -i … --stdio` (also with `--source`) produced **disassembly only** — this toolchain
does not interleave the Rust source (`grep '\.rs'` on the annotate dump finds no source rows), so the
`*.rs:0` bins cannot be resolved to a statement from the disassembly listing alone.

Fallback attribution used instead: `--sort=srcline,symbol`, which shows *which symbol* each line-less
bin belongs to. `board.rs:0` (5.18 %) belongs to `perft` (i.e. the inlined generator/filter/do_move
code), not to `Board::legal` (which carries its own `board.rs:0` at 0.84 %), and the predicate's cost
is fully attributed to *named* lines (`1589`, `1600`) — so the line-less bins were **not** hiding
predicate work. This is consistent with `report6.md` §8 and is the reason the gate decision above is
presented per named line rather than per bin.

---

## 3. Implementation

### 3.1 `src/types.rs` — crate-private accessors

```rust
impl Move {
    #[inline(always)] pub(crate) fn origin_index(self) -> usize { ((self.0 >> 6) & 0x3f) as usize }
    #[inline(always)] pub(crate) fn dest_index(self) -> usize { (self.0 & 0x3f) as usize }
    #[inline(always)] pub(crate) fn needs_full_legal_check(self) -> bool { ((self.0 >> 12) & 3) >= 2 }
}

impl Bitboard {
    #[inline(always)] pub(crate) fn contains(self, index: usize) -> bool { (self.0 >> index) & 1 != 0 }
}
```

**Deviation from the plan (naming):** the plan called the first two `from_index`/`to_index`.
`cargo clippy` emits `clippy::wrong_self_convention` ("methods called `from_*` usually take no
`self`") for `from_index(self)`, a *new* lib warning that the plan forbids. They were renamed to
`origin_index`/`dest_index`, which is also clearer about returning an index rather than a square.
No behavioural difference.

`Bitboard::contains` was added in `src/types.rs` (where `Bitboard` is declared), not in
`src/bitboard.rs` as the plan's risk table guessed (`src/bitboard.rs` does not define the type).

### 3.2 `src/board.rs` — `TrivialLegality`

The free function `is_move_trivially_legal` is **deleted**; the single remaining implementation is the
`Copy` struct:

```rust
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrivialLegality {
    checkers: Bitboard,      // state.checkers
    safe_origins: Bitboard,  // !(our_commoners | state.pinned), or EMPTY if no own commoner
}

impl TrivialLegality {
    #[inline(always)]
    pub(crate) fn new(board: &Board, state: &StateInfo) -> Self { /* + count/board debug_assert */ }

    #[inline(always)]
    pub(crate) fn is_trivially_legal(&self, board: &Board, m: Move) -> bool {
        debug_assert!(m != Move::NONE);
        if !self.checkers.is_empty() { return false; }
        if m.needs_full_legal_check() { return false; }
        if board.squares[m.dest_index()] != NO_PIECE { return false; }
        self.safe_origins.contains(m.origin_index())
    }
}
```

Per candidate this is now: two register tests, one shift+test, one `squares[dest]` load and one 2-bit
move-type test — no `side_to_move()` load, no `make_piece`, no `squares[from]` load, no
`Bitboard::square_bb` table load, no `SQUARES[]` conversion for either square.

**Capture test choice:** `board.squares[m.dest_index()]` (direct array index) was used, as the plan
preferred. The `piece_on(m.to_sq())` alternative was **not** tried, because the direct index clears the
acceptance threshold on the first shape; no measured comparison between the two exists.

### 3.3 `src/movegen.rs` — hoisted call sites

`generate_legal_with_state` and `has_legal_move_with_state` each build
`let trivial = TrivialLegality::new(board, state);` **once**, outside their loop, and call
`trivial.is_trivially_legal(board, m)`. Both loops remain single loops: no split by check state, no
split by move category (the Plan 4 failure mode), and the compaction order is untouched.

### 3.4 Tests added (`src/board.rs::tests`)

* `test_trivial_legality_matches_reference_ladder` — compares the new predicate against a **verbatim
  copy of the pre-refactor ladder** (`is_move_trivially_legal_reference`) for every pseudo-legal
  candidate of a 16-FEN corpus: startpos, the four `move_order_golden` middlegame/endgame/promotion
  positions, three en-passant positions, a castling position, two in-check positions, two pinned
  positions (rook and bishop pin), a no-commoner position, bare commoners, and #13/#14.
* `test_trivial_legality_accepts_only_legal_moves` — soundness: every fast-path accept must satisfy
  `board.legal(m, &state)`; called in debug, so the `debug_assert!`s are active.
* `test_trivial_legality_corpus_covers_accept_and_reject` — non-vacuity: the corpus must produce both
  accepts and rejects *and* exercise the checker guard, the pin guard, en passant/castling, and the
  no-commoner guard. (The first corpus draft had a non-pinning "bishop pin" FEN; this test was added
  partly to catch such gaps, and the FEN was replaced with `4k3/8/b7/8/8/3N4/8/5K2 w - - 0 1`.)

### 3.5 Diff

```
 src/board.rs   | 261 ++++++++++++++++++++++++++++++++++++++++++++++++---------
 src/movegen.rs |  12 +--
 src/types.rs   |  34 ++++++++
 3 files changed, 263 insertions(+), 44 deletions(-)
```

`git status` shows `M src/board.rs`, `M src/movegen.rs`, `M src/types.rs` and the untracked
`docs/` files only. `src/lib.rs`, `src/attacks.rs`, `src/magic.rs`, `src/zobrist.rs`, `src/util.rs`,
`examples/` and `tests/` are untouched.

---

## 4. Step 4 — correctness checks (all run **before** the final timings)

| Check | Result |
|---|---|
| `cargo fmt` | clean |
| `cargo clippy --all-targets` | no new warnings; only the 4 pre-existing `examples/test_d5g8.rs` warnings (`manual_contains`, `manual Range::contains`). One *new* warning was found and fixed in this session (`wrong_self_convention`, see §3.1). |
| `cargo test` | 61 lib + 1 + 4 + 1 + 1 + doctest, all pass (includes the three new tests) |
| `cargo doc` | warning-free |
| `cargo run --release --example verify_perft` | **41/41** at depths 1–6 (candidate tree) |
| `cargo test --test move_order_golden` | pass, goldens **unchanged** |
| `cargo test --test verify_moves` | pass |
| equivalence test vs. verbatim reference ladder | pass (16 FENs, every candidate) |
| fast-path ⇒ `legal` soundness | pass |
| corpus non-vacuity | pass |

---

## 5. Step 5 — performance

### 5.1 Same-session candidate fixed-FEN runs

| #13 run | ms | | #14 run | ms |
|---|---|---|---|---|
| 1 | 10352 | | 1 | 1314 |
| 2 | 10342 | | 2 | 1313 |
| 3 | 10276 | | 3 | 1346 |
| 4 | 10331 | | 4 | 1337 |
| 5 | 10289 | | 5 | 1323 |
| 6 | 10312 | | 6 | 1322 |
| 7 | 10285 | | 7 | 1316 |
| **median** | **10312** | | **median** | **1322** |

vs. baseline medians 10551 / 1351 → **−2.27 %** on #13, **−2.15 %** on #14.

### 5.2 A/B round

Candidate and baseline binaries were both built from the same toolchain in this session
(baseline sources restored from `git show HEAD:src/…`, candidate sources from a saved copy), then
alternated pair-by-pair:

| pair | #13 base | #13 cand | Δ | | #14 base | #14 cand | Δ |
|---|---|---|---|---|---|---|---|
| 1 | 10537 | 10421 | −1.10 % | | 1345 | 1317 | −2.08 % |
| 2 | 10675 | 10367 | −2.88 % | | 1348 | 1325 | −1.70 % |
| 3 | 10516 | 10315 | −1.91 % | | 1551 | 1313 | −15.34 % (baseline outlier) |
| 4 | 10512 | 10374 | −1.31 % | | 1341 | 1331 | −0.74 % |
| 5 | 10545 | 10501 | −0.41 % | | 1354 | 1313 | −3.02 % |
| 6 | 10527 | 10324 | −1.92 % | | 1347 | 1310 | −2.74 % |
| 7 | 10527 | 10298 | −2.17 % | | 1359 | 1316 | −3.16 % |
| **median** | 10527 | 10367 | **−1.91 %** | | 1348 | 1313 | **−2.74 %** |

Pairwise wins: **7/7** on #13 (best −2.88 %, worst −0.41 %) and **7/7** on #14.

### 5.3 Acceptance criteria

| Criterion | Required | Measured | Verdict |
|---|---|---|---|
| #13 median improvement | ≥ 1.0 % | −2.27 % (same-session), −1.91 % (A/B median) | pass |
| #13 pairwise wins | ≥ 6/7 | 7/7 | pass |
| #14 regression | ≤ 0.3 % | −2.15 % (improvement) | pass |
| perft | 41/41 | 41/41 | pass |

Not in the marginal band, so the marginal-band conditions did not apply.

### 5.4 Full suite (supporting evidence only)

Same-session `cargo run --release --example verify_perft` totals: baseline **70.778 s**
(#13: 10.975 s) → candidate **70.524 s** (#13: 10.636 s), 41/41 both. The suite total moved less than
the fixed-FEN loops because the other 40 positions are small and dominated by process/parse overhead;
no regression.

---

## 6. Post-change profile (#13, same flags as §2.2)

10 814 samples, ≈51.73e9 cycles (baseline: 11 572 samples, ≈55.24e9).

| Source line | Baseline | Candidate | Meaning |
|---|---|---|---|
| `board.rs:1589` / `:1627` | 5.22 % | **3.37 %** | commoner test → direct `squares[dest]` capture test |
| `board.rs:1600` / `:1621` | 2.29 % | 0.88 % | capture reject → checker guard |
| predicate total (named lines) | **7.51 %** | **4.25 %** | −43 % relative |
| origin test (`board.rs:1631`) | (inside `:1589` cost) | not separately attributed; absorbed into the loop bin | — |
| `board.rs:0` (in `perft`) | 5.18 % | 1.21 % | — |
| `movegen.rs:293/294/295/296` | 4.11 / 2.10 / 2.97 / — | — / 3.93 / 3.03 / 3.83 | filter loop |
| `movegen.rs:0` | 4.14 % | 5.93 % | line-less loop bin (attribution moved here) |
| `MoveList::push` (`types.rs:787–789` → `:821–823`) | 11.78 % | 11.99 % | unchanged (no new stores) |
| `Board::legal` (symbol) | 25.48 % | 27.24 % | same absolute cycles |

Reading the percentages is confounded by the smaller denominator, so absolute sample counts:

* predicate: 7.51 %·11 572 ≈ **869** → 4.25 %·10 814 ≈ **460** samples (−47 %);
* filter region (predicate + `movegen.rs:0` + named loop lines): ≈ **2 410** → ≈ **2 268** (−5.9 %);
* `Board::legal`: 25.48 %·55.24e9 ≈ 14.08e9 → 27.24 %·51.73e9 ≈ **14.09e9** cycles — unchanged,
  which is the expected signature of *not* becoming over-eager (the same candidates still fall through
  to the full check);
* `MoveList::push` did not grow in absolute terms (its share rose only because the total shrank).

`movegen.rs:0` grew by 1.8 pp while `board.rs:0` and the predicate lines fell; the loop's total
(abs. samples) went down, so this is sampler line attribution moving between bins inside the same
inlined loop, not new work.

---

## 7. Decision

**KEEP.** The change was judged against §5.3: #13 median ≥ 1 % (measured −2.27 %, A/B −1.91 %, 7/7
pairwise wins), no #14 regression (measured −2.15 %), perft 41/41 with unchanged goldens. It adds no
struct field, no node store, no `unsafe`, no dependency and no public API change, and it removes work
from every candidate of every node.

Why it worked where Plans 1/2/4/6 did not: the win mechanism is *removing a dependent load per
candidate* (`squares[from]` plus the `make_piece` compare), not re-shaping the node. The two
node-constant values live in registers across the loop, so no `StateInfo`/`Board` growth was needed —
the exact cost that sank the Plan-6 `StateInfo` cache.

---

## 8. Next steps for the plan queue

1. **`MoveList::push` + `MoveList::new` (≈12–16 %) — now clearly the largest remaining block.**
   It was unchanged by this plan (11.78 % → 11.99 % share). This needs a **new design**, not a retry:
   * filtering the pseudo-legal stream **during generation** (a generation-time predicate call instead
     of write-then-compact) so candidates are never stored and reloaded. `TrivialLegality` is a
     standalone `Copy` value precisely so a generation-time filter can reuse it unchanged; note that
     `for_each_pseudo_legal` already supports early exits, so a fused
     `for_each_legal`-style walk is the natural next experiment.
   * a proven-monotonic length without a per-push bounds branch.
   `analysis.md` §6 forbids retrying `MaybeUninit` and local-count batching.
2. **`Board::legal` is now the single largest symbol (≈27 %).** A profile of its hot lines
   (`board.rs:1397`, `:1435`, `magic.rs:470`) is the next analysis candidate — but Plans 2/4/6 show
   that per-node caching around it has not paid; any plan there needs a *new* mechanism.
3. **Closed, do not schedule:** `analysis.md` §5.1/§5.2/§5.3/§5.5 options 1–2/§5.6 (reverted or
   superseded), §5.7/§5.8/§5.9 and the castling fast path (< 0.3 %), the `EVASIONS`/`NON_EVASIONS`
   split and category-specialized legality (two reverts).
4. **Tooling follow-up:** `perf annotate` on this toolchain does not interleave Rust source, so the
   `*.rs:0` bins can only be attributed per symbol. If line-level attribution inside the inlined
   `perft` body matters for a future plan, the alternative is `cargo asm`/`objdump -dl` on the
   profiling binary or `perf annotate --stdio -l` with a source-mapped build.

---

## 9. Unresolved parts, missing tests, open questions

* Only the `board.squares[...]` shape was measured; the `piece_on(m.to_sq())` alternative from the
  plan was not benchmarked (acceptance was met with the preferred shape).
* The `*.rs:0` attribution gap (see §2.3/§8.4) remains unresolved at the tool level; the *sum* of
  the predicate's named lines is unambiguous, but a small part of the filter loop's cost may still sit
  in the line-less bins.
* No test asserts that `TrivialLegality::new` is called exactly once per node (a regression here would
  be a performance bug, not a correctness bug, so it is left to review/profile).
* `has_legal_move_with_state`'s early return (`commoners(board.side_to_move()).is_empty()`) now
  duplicates information already inside `TrivialLegality::new`; it was deliberately **kept** because
  removing it would change which function performs the first board read, and this plan's scope was the
  predicate body.
