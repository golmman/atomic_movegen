# Report 6 — Node-Level Commoner-Bitboard Cache in `StateInfo`

**Plan:** `docs/plans/perf_analysis7/plan6.md` (item 7 of `analysis.md`, §5.6 — *retry
commoner-bitboard caching alone*)
**Result:** **REVERTED.** Test #13 median **+4.12 %**, test #14 median **+1.89 %** in the same-session
A/B round (0/7 pairwise runs faster on #13, 2/7 nominally faster on #14 by −5 ms). The plan's
Step 1 decision gate was already pointing at "stop, redirect", and the measurement confirmed that
the removed work (~0.7 % of runtime) is smaller than the cost of growing `StateInfo`
(64 → 80 bytes). The tree is byte-identical to the Plan 5 tree; only `docs/` changed.

---

## 1. Summary of what was tried

`Board::commoners()` (`board.rs:847`) is `by_color[c] & by_type[Commoner]` and was re-derived
several times per node: 5× in `populate_state` (via `compute_checkers`/`compute_pinned` and the two
`*_count` fields) and up to 5× per `Board::legal` call, which runs for every non-trivial candidate.

The plan's §4 design was implemented exactly:

- `StateInfo` gained `pub commoners: Bitboard` and `pub them_commoners: Bitboard` (pre-move sets),
  initialised to `Bitboard::EMPTY` in `StateInfo::new()`;
- `populate_state` became the single writer of all four hot fields and derived
  `commoners_count`/`them_commoners_count` from the cached bitboards;
- `compute_checkers`/`compute_pinned` were split into thin bitboard-loading wrappers plus
  `compute_checkers_with(us, commoners, them_commoners)` / `compute_pinned_with(us, commoners)`;
- `Board::legal` switched its four commoner reads to `state.*`, `is_move_trivially_legal` and
  `movegen::has_legal_move_with_state` switched to `state.commoners.is_empty()`, and two
  `debug_assert_eq!` guards were added at the top of `legal`;
- two unit tests were added (`test_populate_state_commoner_bitboards`,
  `test_default_state_declines_trivial_fast_path`).

The candidate patch touched **only** `src/board.rs` (~+80/−20 lines) and `src/movegen.rs` (+2/−2
lines) — no `Board` field, no pre-filter, no `EVASIONS` split, no change to `perft`, `do_move`,
`undo_move`, `MoveList` or the public API beyond the two additive `StateInfo` fields. The patch was
discarded by `git checkout -- src/board.rs src/movegen.rs`, so an exact `git diff --stat` is no
longer reproducible; `git status` now shows only the untracked plan file, i.e. the library is again
identical to the Plan 5 tree.

---

## 2. Step 1: fresh profile (decision gate) — the profile said "stop"

Same-session baseline on the untouched Plan 5 tree, 7 runs at depth 6:

```text
#13  10931 10756 10638 10649 10734 10758 10796   median 10756 ms  (nodes 2160817389)
#14   1395  1393  1387  1373  1406  1385  1387   median  1387 ms  (nodes  245931633)
```

Then `perf record -F 997 -g --call-graph dwarf` on the profiling profile, #13 at depth 6 (two
independent runs, 11 684 and 11 513 samples), plus one run on #14 (1 478 samples). Self time by
source line, aggregated per function (96.6–98.6 % of samples accounted for):

| Function | #13 run 1 | #13 run 2 | #14 |
|----------|-----------|-----------|-----|
| `MoveList::push` | **12.22 %** | **12.05 %** | **8.60 %** |
| `Board::legal` | 9.15 % | 9.60 % | 8.46 % |
| `generate_legal_with_state` (filter loop) | 8.43 % | 9.27 % | 7.65 % |
| `is_move_trivially_legal` | 7.98 % | 7.84 % | 7.23 % |
| `square_attacked_by` | 5.20 % | 4.81 % | 5.27 % |
| `bishop_attacks` | 5.11 % | 4.65 % | 4.69 % |
| `Move::make_move` | 4.76 % | 4.53 % | 5.23 % |
| `pieces_color_pt` (incl. every `commoners()`) | **0.33 %** | **0.37 %** | **0.21 %** |
| `compute_checkers` | 0.39 % | 0.33 % | 0.21 % |
| `compute_pinned` | 0.28 % | 0.26 % | 0.14 % |
| `populate_state` | ≈0.10 % | 0.15 % | 0.07 % |
| `generate_castling` | 0.27 % | 0.25 % | 0.07 % |
| `__memset_avx2_unaligned_erms` (`MoveList::new`) | 3.40 % | 1.66 % | 1.51 % |

The three commoner call-site source lines account for the *entire* target: `board.rs:556`
(`pieces_color_pt`) 0.33 %, `board.rs:1387` (`(self.commoners(them) & occupied).is_empty()`)
0.31 %, `board.rs:1334` (castling adjacent-commoner probe) 0.05 % — **0.69 % together**.

Gate evaluation (plan §5 Step 1):

| Criterion | Required | Measured | Verdict |
|-----------|----------|----------|---------|
| `Board::legal` self | ≥ ~10 % | 9.15 / 9.60 % | **not met** (borderline; +`square_attacked_by` = 14.4 %) |
| `pieces_color_pt` | ≥ ~2 % | 0.33 / 0.37 % | **not met** (6× below) |
| `is_move_trivially_legal` + `compute_checkers` + `compute_pinned` | ≥ ~15 % | 8.65 / 8.43 % | **not met** |

and the plan's explicit redirect condition — *"if `MoveList` push dominate and the commoner lines
are below ~1 %, stop"* — is met literally: `MoveList::push` is the single largest block and the
commoner lines total 0.69 %.

### 2.1 Deliberate deviation: the change was measured anyway

Because criterion 1 was within noise of its threshold (9.15–9.60 % vs "~10 %", and 14.4 % counting
`square_attacked_by`, which only `legal` calls), the plan's own Step 5 acceptance rule was executed
too, so that the report states a measured answer instead of a profile-attribution argument. That is
the only deviation from the plan: Steps 2–5 were run although the gate did not pass. Steps 6 (report)
and the redirect conclusions below are unchanged by the outcome.

---

## 3. Performance

All numbers are `target/release/examples/perft <FEN> 6`, wall clock around the process, same host
and session, 7 runs each.

### 3.1 Round 1 (candidate, first build)

```text
#13  10984 10952 11050 10965 10974 10995 11005   median 10984 ms   (+2.1 % vs 10756)
#14   1396  1419  1401  1389  1395  1395  1390   median  1395 ms   (+0.6 % vs  1387)
```

### 3.2 A/B round (`git stash` → baseline → `git stash pop` → candidate)

Baseline, immediately before:

```text
#13  10879 10694 10609 10649 10634 10634 10621   median 10634 ms
#14   1380  1407  1373  1378  1404  1366  1353   median  1378 ms
```

Candidate, immediately after rebuild:

```text
#13  11134 11026 11068 11087 11072 10972 11153   median 11072 ms
#14   1435  1403  1421  1435  1399  1404  1402   median  1404 ms
```

Pairwise, run order (candidate − baseline):

```text
#13   +255  +332  +459  +438  +438  +338  +532  ms   → 7/7 slower
#14    +55    −4   +48   +57    −5   +38   +49  ms   → 5/7 slower, 2/7 faster by 4–5 ms (noise)
```

Pairwise medians: **#13 +4.12 %**, **#14 +1.89 %**.

### 3.3 Post-revert confirmation

After `git checkout -- src/board.rs src/movegen.rs` and a rebuild, the baseline is restored:

```text
#13  10564 10532 10639 10564 10604 10630 10587   median 10587 ms  (nodes 2160817389)
#14   1360  1351  1420  1398  1384  1381  1399   median  1384 ms  (nodes  245931633)
```

The session drifted *downwards* by ~1.6 % over its lifetime (baseline medians 10756 → 10634 →
10587 ms), which is why only the A/B pair in §3.2 is used for the decision; the candidate was slower
in every round and by more than the drift.

### 3.4 Interpretation

The change is a straightforward regression on both positions. The measured profile explains it:

| | baseline (#13, run 2) | candidate (#13) |
|---|----------------|-----------------|
| `board.rs:1387` commoner call | 0.38 % (run 1: 0.31 %) | — (reads `state.them_commoners`) |
| `pieces_color_pt` | 0.37 % | 0.51 % (other callers remain) |
| `compute_checkers(_with)` | 0.33 % | 0.42 % |
| `compute_pinned(_with)` | 0.26 % | 0.07 % |
| **`populate_state`** | **0.15 %** | **0.33 %** |
| **`board.rs:0` (line-less code: spills/reloads)** | **5.75 %** | **7.42 %** |
| `Board::legal` self | 9.60 % | 8.55 % |

So the removed loads did disappear (`legal` −1.0 pp, `compute_pinned` −0.2 pp), but the node now
stores two extra `u64`s in `StateInfo` (`populate_state` doubled), and the inlined
`generate_legal_with_state` body keeps two more 8-byte values live, which shows up as ~1.7 pp of
unattributed spill/reload code in `board.rs`. Net: +4 %. This is the same mechanism that sank
Plans 1, 2 and 4, now with a directly visible cost account.

Measured size change: `StateInfo` **64 → 80 bytes** (+25 %), `MoveList` unchanged at 520 bytes
(measured with `std::mem::size_of` against the debug rlib of both builds).

---

## 4. Correctness (of the candidate build, before the revert)

| Check | Result |
|-------|--------|
| `cargo build` | clean |
| `cargo test` (debug, so the new `debug_assert_eq!`s were active) | **60 lib** (58 + 2 new) + 1 golden + 4 perft + 1 verify_moves + 1 doc test, all pass |
| `cargo test --test move_order_golden` | passes with unchanged goldens → move order byte-identical |
| `cargo fmt` | clean |
| `cargo clippy --all-targets` | no new warnings (only the 4 pre-existing ones in `examples/test_d5g8.rs`) |
| `cargo doc` | no warnings (new public fields documented) |
| perft node counts | `2 160 817 389` (#13) and `245 931 633` (#14) identical in all 28 candidate runs |
| new unit tests | `test_populate_state_commoner_bitboards` (bitboards + counts + `checkers`/`pinned` unchanged, 4 FENs incl. no-commoner and black-to-move), `test_default_state_declines_trivial_fast_path` |

On the **reverted** tree, the full verification was re-run:

| Check | Result |
|-------|--------|
| `cargo test` | 58 lib + 1 + 4 + 1 + 1, all pass |
| `cargo run --release --example verify_perft` | **41/41** at depths 1–6, total 71.287 s (slowest #13, 11.022 s) |

**Process note:** `verify_perft` (the 41-position suite) was *not* run on the candidate build before
the timing rounds — the ordering was `cargo test` → timing → revert. The candidate's correctness
evidence is therefore: `cargo test` (12-position perft to depth 4, move-order goldens, 60 unit
tests, `verify_moves`), identical depth-6 node counts on both profiled FENs in 28 runs, and the
profiled change being a pure substitution of equal values. A future plan should run `verify_perft`
before timing, as plan Step 4 specifies.

The `debug_assert_eq!` guard behaved as designed: the first version of the optional
"populated vs. unpopulated state" test deliberately called `legal` with a default `StateInfo` and
the assert fired, confirming that the "`state` must be populated" contract is now enforced in test
builds. The test was rewritten to assert the contract instead (`is_move_trivially_legal` declines
for an unpopulated state, `has_legal_move_with_state` reports no moves).

---

## 5. Decision: revert

Acceptance criteria from plan §5 Step 5 vs. measurement:

| Criterion | Required | Measured | Verdict |
|-----------|----------|----------|---------|
| #13 median | ≥ 0.5 % faster | **4.12 % slower** | fail |
| #13 pairwise | faster in ≥ 6 of 7 | 0 of 7 | fail |
| #14 median | ≤ 0.3 % regression | **1.89 % slower** | fail |

The plan mandates a revert in this case, and §4.5 explicitly forbids "tweaking the struct" or
stacking §5.7/§5.9 on top to rescue the measurement. `src/board.rs` and `src/movegen.rs` were
restored with `git checkout`; the baseline medians were re-confirmed afterwards (§3.3) and the
41-position suite passes again. Nothing from the candidate remains in the tree.

---

## 6. What the profile says to do instead

The remaining cost is **move construction and list writes**, not the cached-state items:

| Block | #13 | #14 |
|-------|-----|-----|
| `MoveList::push` | 12.05 % | 8.60 % |
| move plumbing: `make_move`, `to_sq`, `from_sq`, `move_type`, `square_bb`, `lsb`, `from_u8`, `flip`, `bitand`, `bitor`, `bitxor` | 20.27 % | 21.74 % |
| **move plumbing incl. `push`** | **32.32 %** | **30.34 %** |
| `MoveList::new` (`memset` of 512 bytes) | 1.7–3.4 % | 1.5 % |
| legality: `Board::legal` + `square_attacked_by` + `generate_legal_with_state` filter loop | 23.68 % | 21.38 % |
| `is_move_trivially_legal` | 7.84 % | 7.23 % |
| magic/pawn attack lookups (`bishop_attacks`, `rook_attacks`, `pawn_attacks`) | 7.75 % | 8.09 % |
| line-less bins (`board.rs:0` + `movegen.rs:0` + `types.rs:0`) | 12.00 % | 13.51 % |
| §5.6 target (`pieces_color_pt`/`commoners`) | **0.37 %** | **0.21 %** |
| §5.7/§5.9 target (`compute_pinned`) | **0.26 %** | **0.14 %** |
| §5.8 target (`occupied`, `board.rs` field reads) | **0.08 %** | **0.00 %** |
| castling (`generate_castling`) | 0.25 % | 0.07 % |

Consequences for the analysis' §8 list:

1. **§5.7 / §5.9 (`slider_blockers`, precomputed empty-board slider attacks) should be dropped
   before a plan is written.** `compute_pinned` is now 0.14–0.28 % of runtime; even a 50 % reduction
   is an order of magnitude below the 0.5 % keep threshold. Same for §5.8 (`occupied` field): the
   `occupied` helper is attributed 0.00–0.08 %.
2. **The castling fast path is not worth a plan.** `generate_castling` is 0.07–0.27 %; `report5.md`
   §9's suspicion that it was attribution noise is confirmed.
3. **`MoveList` push/zero-init remains the largest single target (12.2 % + 1.7–3.4 %), but analysis
   §6 records two failed attempts (`MaybeUninit`, local-count batching).** A new plan would need a
   genuinely different design (e.g. a proven-monotonic length without a per-push bounds branch, or
   restructuring so the pseudo-legal stream is filtered while it is generated instead of being
   written to a second list), and it must not change move order.
4. **`is_move_trivially_legal`'s branch ladder is the next individual line-level target**: the single
   line `if board.piece_on(from) == make_piece(us, PieceType::Commoner)` costs 5.6 % of total runtime
   on #13, i.e. more than half of the function, and the surrounding capture/en-passant/castling tests
   add another ~2 pp. Reordering or making those tests cheaper (they are per-candidate
   branch + `squares[]` load) is a more promising next item than anything left in §5.
5. **The three line-less bins are ~12–14 % and belong to the same blocks** (`push`/move construction
   and the inlined filter loop); they must be resolved before micro-targets below ~2 % can be
   chosen reliably (§8).

---

## 7. Problems encountered

1. **The decision gate and the acceptance criteria point the same way, but for different reasons.**
   The gate said "the target is too small"; the measurement said "the mechanism costs more than the
   target". Both are recorded above, so a future plan can cite either.
2. **`StateInfo` growth is measurable, not neutral.** +16 bytes doubled `populate_state` and added
   ~1.7 pp of line-less spill code in the hot inlined loop. Plan §4.5's documented fallback
   (threading the two bitboards as parameters through a private `legal_with`, leaving `StateInfo`
   untouched) is therefore the only remaining shape of §5.6 worth considering — but the removed work
   is 0.7 %, so it cannot pay for two extra arguments either. Recommend closing §5.6 as "not
   profitable at the current profile".
3. **The plan under-estimated the profile staleness risk in the opposite direction**: `report5.md`
   §9 already showed `pieces_color_pt` at 3.4–4.2 % of source-line samples *before* Plan 5; Plan 5
   removed most of those call sites (the pawn stage), and the post-Plan-5 figure is 0.33 %. The gate
   caught this correctly.
4. **`cargo fmt` runs before the diff is captured**: the candidate patch was formatted, measured, and
   reverted, so the report cannot quote an exact `git diff --stat`. Only the shape (+80/−20 in
   `board.rs`, +2/−2 in `movegen.rs`) is known.
5. **The debug assert changes the misuse behaviour of `has_legal_move_with_state`**: with an
   unpopulated `StateInfo` it now returns `false` (empty cached bitboards) instead of consulting the
   board. That is within its documented contract, and the new unit test pins it.

---

## 8. Unresolved parts / missing tests

- **`verify_perft` was not run on the candidate build** (§4 process note). The revert makes this a
  reporting gap rather than a correctness risk, but the ordering should be fixed next time.
- **`has_legal_move_with_state` is only exercised through the new unit test with a default state**;
  there is no test that a *stale* (populated-for-another-position) state is caught. The
  `debug_assert_eq!` in `legal` would catch it, but no test builds such a state deliberately.
- **The `board.rs:0` / `movegen.rs:0` / `types.rs:0` line-less bins total ~12 % (#13) and cannot be
  attributed** with `perf --sort=srcline`; they were the reason the gate was borderline and the
  reason the candidate's regression is explainable only qualitatively. Any future plan that depends
  on line-level attribution below ~2 % should first resolve those bins (e.g. `perf annotate` on the
  inlined `perft` body or a `-Z`/`#[inline(never)]` profiling build).
- **No measurement of `StateInfo` growth in isolation** (e.g. adding the two fields without using
  them) was taken; that would separate "struct size" from "extra live values in `legal`".

---

## 9. Next steps

1. **Update `analysis.md` §8**: mark §5.6 as *measured and not profitable*, and remove §5.7/§5.9/§5.8
   from the plan queue — all three targets are < 0.3 % of runtime now.
2. **Next plan candidate: `is_move_trivially_legal`'s per-candidate branch ladder** (7.2–8.0 %, with
   one line at 5.6 % on #13), measured alone. It is the cheapest remaining block to attack without
   touching the move-order contract.
3. **Then `MoveList::push` / `MoveList::new` (≈13–16 % combined)**, but only with a new design —
   analysis §6's prohibition on retrying `MaybeUninit`/local-count batching stands.
4. **Keep `StateInfo` at 64 bytes and `commoners_count`/`them_commoners_count` as the API.** No
   deprecation is warranted; the bitboard-cache experiment did not justify replacing them.
5. Do not retry §5.1, §5.2, §5.3, §5.5 options 1–2, or §5.6 without new profiling evidence.
