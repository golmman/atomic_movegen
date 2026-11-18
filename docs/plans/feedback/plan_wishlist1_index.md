# wishlist 1 — implementation plan index and sequencing

## Purpose

This index turns `docs/plans/feedback/wishlist1.md` into independently
reviewable implementation plans. It is a plan, not an implementation. Each
numbered plan follows `AGENTS.md` and ends with an implementation report
(`docs/plans/feedback/report_wishlist1_N.md`).

The wishlist is an upstream request list written against `atomic-movegen
2.8.0`. This tree is that baseline. The four asks are additive or opt-in; none
of them changes move order, perft values, FEN output, existing keys, or
`Outcome`/`Terminal` semantics. Plan 5 is not a fifth wishlist ask: it is the
final hardening, clean-code and publish gate for whatever of plans 1–4 landed.

| # | Ask | Type | Priority | Plan |
| --- | --- | --- | --- | --- |
| W1 | Legal-EP child key on the effect path | rules/API + cost | P1 | 1 |
| W2 | Lichess-style FEN acceptance | parser parity | P2 | 2 |
| W3 | Immediate-explosion query on a child | one-ply fact | P2/P3 | 3 |
| W4 | Node-constant analysis view | guide geometry | P3 | 4 (measurement-gated) |
| — | Hardening, clean code and publish gate | release gate | final | 5 |

## Current-tree assessment (2.8.0)

| Item | Current state | Plan |
| --- | --- | --- |
| W1 | `EffectContext::post_move_and_key` returns a `PostMove` and an `EnPassantMode::Always` key from one derivation. There is no mode-aware paired query. `PostMove` carries no key and no parent hash. `Board::hash_after_with(EnPassantMode::Legal)` clones and plays a double push, then calls `hash_with(Legal)`. The exact child-side legal-EP test exists only as `Board::legal_ep_square`, which populates a `StateInfo`. | 1 |
| W2 | `Board::from_fen` is syntax-only except that it rejects castling rights not backed by pieces and an en-passant target not consistent with a double push. `FenPolicy` has `Strict`, `AllowTerminal`, `AllowImpossibleCheck`, and `Permissive`; `Strict` is not the shakmaty/Lichess acceptance predicate. There is no policy that *drops* invalid castling rights or an impossible ep target. | 2 |
| W3 | `EffectContext` can derive a child (`post_move`), classify it (`effect`, `classification`), and answer terminality and existence questions. There is no read-only "can the child side to move immediately explode the mover's commoner?" query. The prover therefore keeps its own `Mover::exposes`/`exploding_capture` logic. | 3 |
| W4 | `Board` exposes `checkers`, `pinned`, `attackers_to`, and the attack tables. `StateInfo` stores `checkers` and `pinned`. The internal legality ladders (`TrivialLegality`, `WidenedLegality`, `FastLegality`) already derive per-node masks, but there is no stable public analysis view over those facts. | 4 |
| H | The default build claims to be safe Rust but `board.rs::piece_key_raw` uses an unconditional `get_unchecked`; clock arithmetic can overflow in debug builds; structural castling/ep checks are duplicated; `cargo publish` content is implicit and includes examples the earlier publish decision excluded. | 5 |

## What each plan does

1. **`plan_wishlist1_1.md` — W1: legal-EP child key on the effect path.**
   Add `EffectContext::post_move_and_key_with(board, m, mode)`, backed by
   `position::legal_ep_square_in_view` so a double push derives the child once
   and validates the child's ep capture from the child view, without a clone
   and without populating a `StateInfo`. `EnPassantMode::Always` and
   `ZOBRIST_VERSION` are unchanged; `post_move_and_key` remains the `Always`
   spelling. Small risk, additive API.

2. **`plan_wishlist1_2.md` — W2: Lichess-style FEN acceptance.**
   Split FEN syntax parsing from structural validation, add
   `FenPolicy::Lichess`, and define exactly what is sanitized (castling rights,
   impossible ep target), what is accepted (the material bound is not a
   rejection), and what is rejected (the shakmaty-style structural class).
   `Board::from_fen` keeps its current behavior byte for byte. Medium-high
   risk because the parser is shared; the plan keeps the old path as the
   reference and pins the new policy with a frozen disposition table.

3. **`plan_wishlist1_3.md` — W3: immediate-explosion query on a child.**
   Add `EffectContext::exploding_capture_after(board, m) -> Option<Move>`: the
   first legal capture by the child side to move whose blast removes the side
   that played `m`'s commoner. It is read-only, uses the already-derived child
   view, and is differentially checked against an exhaustive search over the
   built child's legal moves. Cost is measured; no downstream guide
   integration is claimed. Medium risk (a new public rules query), additive.

4. **`plan_wishlist1_4.md` — W4: node-constant analysis view.**
   A small read-only `AnalysisContext` with masks for checkers, pins, check
   squares by role, blockers, pieces beside a commoner, attack maps, and
   uncovering pieces. **This plan starts with a measurement gate:** the
   downstream's `Mover::new` profile share and its field-by-field mapping must
   arrive first. If the share is small, the plan closes as a watchlist entry
   with no code, exactly as the wishlist permits.

5. **`plan_wishlist1_5.md` — hardening, clean code and publish gate.**
   Make the default build safe Rust, remove clock/FEN overflow panics, give
   the structural castling/en-passant checks one source of truth, align the
   public docs/lints/API pin, add no-panic robustness tests, freeze the
   package content, and prepare the CHANGELOG/README/version for the release.
   It adds no wishlist feature and performs no publish; it produces the final
   report and the owner's publish checklist.

## Dependency order

```text
plan 1 (W1: keyed child summary) --------+
                                         |
plan 3 (W3: explosion query) ------------+--> 2.9.0 feature window
                                         |
plan 2 (W2: Lichess FEN policy) ---------+--> 2.9.0 if ready, else 2.10.0
                                         |    (independent; parser may stay
                                         |     outside the downstream)
plan 4 (W4: analysis view) --------------+--> only after the measurement gate;
                                         |    otherwise watchlist, no release
                                         |
plan 5 (hardening + publish gate) -------+--> exactly one release of what landed
                                              (no feature of its own)
```

- Plan 1 is self-contained and lands first. Plan 3 can reuse its child-view
  legality helper and should coordinate its `effect_bench` rows with plan 1,
  but has no hard code dependency.
- Plan 2 is independent of 1 and 3. It is planned because the wishlist asks
  for it, not because the downstream must take it: the wishlist's
  "formulation option — keep the parser outside" explicitly permits the
  downstream to keep shakmaty for parsing. Plan 2 is therefore a standalone
  parser-parity release, not a blocker for plans 1 and 3.
- Plan 4 is not a normal implementation plan. It is a measurement-and-mapping
  plan whose first gate may close it without code.
- Plan 5 runs last and is the only plan that changes the release version. It
  must not start until the implementation reports for the landed plans exist;
  it hardens and packages exactly that tree.

## Release strategy

- **2.9.0:** plans 1 and 3, hardened and packaged by plan 5. Both add
  child-query API and share one measurement harness. No existing answer, key,
  move order, perft value, or FEN changes. Plan 2 may join 2.9.0 if its frozen
  parity table and parser split are green before the release window; otherwise
  it is the next release.
- **2.10.0:** plan 2, if it did not ship in 2.9.0. It only adds an opt-in FEN
  policy; the default parser and every existing policy keep their answers. Its
  priority is P2, but it is independent and the wishlist explicitly permits the
  downstream to keep shakmaty for parsing, so it does not block plans 1 and 3.
- **No release for plan 4 unless its gate passes.** If it does, it is a later
  minor release; if it does not, the report records the measurement and the
  watchlist verdict.
- **Plan 5 finalizes the release.** It chooses the version from what actually
  landed (2.9.0 for the additive child-query work; plan 2 follows the split
  above), runs the hardening and package gates, and updates `Cargo.toml`,
  `CHANGELOG.md`, the README dependency snippet and the public-API pin. It
  does not publish; the owner commits, tags and publishes.
- **No promise.** The plans may publish measured costs for the new queries, but
  they make no claim about any downstream search tree or wall clock.

## Global gates for every plan

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc` (no warnings; `cargo test --doc` for new doctests)
- `cargo run --release --example verify_perft` (41/41; depth 5 minimum, depth 6
  when time permits)
- `cargo test --test perft_tests`, `--test move_order_golden`,
  `--test verify_moves`, `--test conformance`, `--test effects`,
  `--test post_move`, `--test terminal_after`, `--test child_state`,
  `--test public_api`, `--test interchange`
- `cargo test --features unsafe-optimizations` when touching the Zobrist-key
  helpers (plan 1) or attack geometry (plan 4)
- `cargo run --release --example effect_bench 9 2000` before and after any
  change to the child path (plans 1 and 3; plan 4 adds its own analysis bench)
- Perft timing (`--release`, three runs, median) for any change that could
  affect `Board::legal`, `populate_state`, the generator, or the legality
  ladder. A regression above ~2 % is a blocker, not a note.
- No new dependency, no new `unsafe`, no GPL interop, no search/evaluation/
  tablebase/threading API, no second public position representation, and no
  API that exists only to serve one downstream's search design.
- Plan 5 adds: `cargo test --all-features`, `cargo test --release`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`, the
  safe-default perft/bench comparison, `cargo package --list` and
  `cargo publish --dry-run`.

## Non-goals (wishlist 1, handoff 3 §4, handoff 1 R9)

Do not add: an insufficient-material table, a proof-order/search API, a search/
eval/tablebase/threading API, a second position representation, a variant
framework, GPL interop, a public constructor for `PostMove`/`PositionView`, or
a change to the downstream's protocol. Do not change `Board::terminal`/
`outcome` precedence, the `Always` key, `ZOBRIST_VERSION`, the default FEN
parser, or move order while implementing any of these plans.

## Final task for each plan

Write `docs/plans/feedback/report_wishlist1_N.md` with item-by-item status,
the public (or crate-private) API as implemented, differential counts (visited
positions and moves), the benchmark table measured in the same run (absolute ns
and the ratios that plan defines), changed files, problems encountered,
deviations from the plan with the measurement that motivated them, unresolved
parts, missing tests and next steps. Plan 4's report must also state the
measurement-gate verdict and either the implemented API or the watchlist
decision. Plan 5's report must state the final release version, the landed
plans, the safety/overflow audit, the package diff and the extracted-package
and `cargo publish --dry-run` results, and the owner's manual publish
checklist.
