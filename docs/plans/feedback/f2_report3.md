# Report 3 — En-passant normalization (P1-6)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| `Board::legal_ep_square()` | Done | Returns `Some(ep)` only when at least one candidate ep capture passes the normal `legal` predicate; no `MoveList` is built. |
| `EnPassantMode { Always, Legal }` | Done | Public, documented, `#[non_exhaustive]`, and re-exported from `src/lib.rs`. |
| `Board::hash_with(mode)` | Done | `Always` is the unchanged `Board::hash`; `Legal` removes the ep-file key when no legal ep capture exists. |
| `Board::fen_with(mode)` | Done | Shared FEN writer; `Always` is the unchanged `Board::fen`, `Legal` prints `-` for a non-capturable target. |
| `Board::hash_after_with(m, mode)` | Done | `Always` delegates to `hash_after`; `Legal` clones/plays only a double pawn push, as documented. |
| Defaults unchanged | Done | `hash()`, `fen()`, and `hash_after()` remain FSF-compatible `Always`. |
| Documentation | Done | `Board::hash`, `fen`, `hash_after`, `ep_square`, the new APIs, `README.md`, `src/lib.rs`, and `CHANGELOG.md`. |
| Verification | Done | Tests, clippy, fmt, docs, and the full 41-position depth-6 perft suite pass. |

## Measured en-passant prevalence

A temporary probe (`examples/ep_probe.rs`, removed after measurement) ran
20,000 seeded random legal-playout walks of up to 120 plies from the starting
position, using the same walk shape as the feedback probe:

```
positions=1,505,668
ep_set=122,045 (8.106%)
ep_with_legal_capture=5,755 (0.382% of positions, 4.72% of ep-set positions)
ep_without_legal_capture=116,290 (7.723% of positions, 95.28% of ep-set positions)
```

This reproduces the feedback's 4.66% legal-ep share of ep-set positions
(4.72% here) and its 95.3% non-capturable share. The small prevalence
differences come from different random seeds, not from changed move
generation.

Note on an adjacent-pawn check: counting positions where no pawn of the side
to move attacks the target gives 95.25% of ep-set positions. The feedback's
75.77% "no adjacent enemy pawn" figure uses a looser adjacency definition
(pawns adjacent to the double-pushed pawn, including squares from which an ep
capture is geometrically impossible), so the two numbers are not directly
comparable. The legal-ep percentage is the meaningful cross-check and matches.

## Transposition before/after

Using the feedback's concrete FEN pair:

| Position | `hash()` | `hash_with(Legal)` | `fen()` ep field | `fen_with(Legal)` ep field | `legal_ep_square()` |
| --- | ---: | ---: | --- | --- | --- |
| `r1bqkbnr/pppppppp/2n5/8/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq e3 0 3` | `11376966775415242283` | `7120124017209882689` | `e3` | `-` | `None` |
| `r1bqkbnr/pppppppp/2n5/8/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 0 3` | `7120124017209882689` | `7120124017209882689` | `-` | `-` | `None` |

The placement, side to move, and castling rights are identical. `Always`
preserves the old distinct keys and FENs; `Legal` makes both positions share
`7120124017209882689` and the same FEN. The displayed `Always` hashes match the
feedback's values exactly.

## `legal_ep_square` correctness cases

All cases are pinned by unit tests in `src/board.rs`:

* **Legal capture exists:** `8/8/8/PP6/kP1Pp3/PN2P3/8/6K1 b - d3 0 1` has
  stored ep `d3`, returns `Some(d3)`, and `is_legal(e4d3)` is true. This is
  the existing en-passant-only fixture.
* **Target set but no legal capture:** the existing ep fixture
  `rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2` has
  `ep_square() == Some(d6)` but returns `None` from `legal_ep_square()`.
* **No target:** `8/8/8/8/8/8/8/4K2k w - - 0 1` returns `None`.
* **Pin:** `4r2k/8/8/3pP3/8/8/8/4K3 w - d6 0 2`. `e5xd6` would open the
  e-file from the rook on e8 to the commoner on e1; the blast at d6 does not
  reach e8. Stored ep is `d6`, `legal_ep_square()` is `None`, and
  `is_legal(e5d6)` is false.
* **Self-explosion:** `7k/4K3/8/3pP3/8/8/8/8 w - d6 0 2`. The blast at d6
  destroys the mover's own commoner on e7. Stored ep is `d6`,
  `legal_ep_square()` is `None`, and `is_legal(e5d6)` is false.
* **Terminal boards:** the commoner-count guard returns `None` before any
  candidate capture is tested.

The implementation checks only the stored target's at most two pawn origins,
uses the populated `StateInfo` for checkers/pins, and calls `legal` on each
candidate. It does not call `generate_legal` or build a `MoveList`.

## `hash_after_with` equivalence

The existing seeded random-playout harness
(`hash_after_equivalence_random_playouts`, 8,592 positions, every
pseudo-legal move at every position) now also checks `hash_after_with` for both
modes. For every move and mode it compares:

```text
board.hash_after_with(m, mode) ==
    { let mut b = board.clone(); b.do_move(m, &mut st); b.hash_with(mode) }
```

Result: all checks pass. The original `hash_after`/`rule50_after` assertions
remain unchanged. `ep_default_behavior_unchanged` additionally pins
`hash() == hash_with(Always)` and `fen() == fen_with(Always)` on ep-set,
non-ep, and legal-ep fixtures.

## Cost of the Legal policy

Temporary release benchmark (aarch64 container, crate release profile,
`black_box` around every call, 1M–5M iterations after warm-up):

| Operation | Position | Time |
| --- | --- | ---: |
| `hash_with(Always)` | any | 0.52 ns |
| `hash_with(Legal)` | no ep target | 0.61 ns |
| `hash_with(Legal)` | ep target, no legal capture | 9.13 ns |
| `hash_with(Legal)` | ep target, legal capture | 19.06 ns |
| `hash_after_with(Always)` | start position, `e2e4` double push | 3.69 ns |
| `hash_after_with(Legal)` | start position, `e2e4` double push | 22.62 ns |
| `hash_after_with(Always)` | start position, `g1f3` quiet | 4.69 ns |
| `hash_after_with(Legal)` | start position, `g1f3` quiet | 5.70 ns |

`Legal` is only expensive when an ep target is actually present (about 8.1% of
random-playout positions). `hash_after_with(Legal)` pays the clone/`do_move`
cost only on double pushes. The default `hash`, `fen`, and `hash_after` remain
field reads / the existing fast path. Consumers that need normalized keys in a
hot loop can call `legal_ep_square()` themselves and adjust their key, as
documented.

## Problems encountered and decisions

* The plan sketch's `legal_ep_square` uses the attack table symmetrically as
  intended; no issue was found with direction or colour.
* FEN parsing already validates that the ep victim square holds an enemy pawn
  and that the target rank matches the side to move, so the new method can
  trust the stored target's shape.
* `hash_after_with(Legal)` uses the plan's clone/`do_move` approach rather than
  duplicating the child ep-legality logic. This keeps it exactly consistent with
  `do_move` + `hash_with(Legal)` and keeps the code small. The trade-off is the
  measured ~23 ns double-push cost, documented on the method.
* No default behavior changed. The feedback explicitly left a default change as
  a possible major-version decision; this plan took the additive route.

## Verification run

```text
cargo test                                     88 unit + 1 move-order + 4 perft + 1 moves + 1 doctest, all pass
cargo clippy --all-targets -- -D warnings      pass
cargo fmt --check                              pass
cargo doc --no-deps                            pass
cargo run --release --example verify_perft     41/41 positions, depths 1-6, pass (80.1 s)
```

## Changed files

| File | Change |
| --- | --- |
| `src/types.rs` | `EnPassantMode` enum and documentation. |
| `src/lib.rs` | Re-export `EnPassantMode`; crate-level position-key/en-passant section. |
| `src/board.rs` | `legal_ep_square`, `hash_with`, `fen_with`, `hash_after_with`, shared FEN writer, docs, and tests. |
| `README.md` | Feature-list entry for canonical ep keys/FEN and unchanged defaults. |
| `CHANGELOG.md` | Unreleased Added entry. |
| `docs/plans/feedback/f2_report3.md` | This report. |

## Remaining gaps / next steps

* No `EnPassantMode::PseudoLegal` variant; the feedback asked only for
  `Always` and `Legal`, and a third mode would need a consumer.
* No repetition-detection implementation; this only normalizes keys.
* `hash_after_with(Legal)` remains a convenience rather than a hot-loop
  replacement for `hash_after`. A future incremental child-state API could
  avoid the clone on double pushes if a consumer needs it.
* The temporary prevalence/benchmark example was removed; its commands and
  measured numbers are recorded above.
