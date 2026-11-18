# Plan 5 — hardening, clean code, and the publish gate (final)

## Scope

The final plan of wishlist 1. It implements no wishlist feature. It closes the
series by hardening the crate, removing avoidable duplication, making the
release contract explicit, and preparing a publishable package.

The plan assumes the implementation reports for the plans that landed exist
(`report_wishlist1_1.md` … `report_wishlist1_4.md`, as applicable) and that
`CHANGELOG.md` contains an `## [Unreleased]` section. It does not assume that
all four wishlist plans landed: plan 4 is measurement-gated, and plan 2 may
remain outside the downstream until its parity gate passes. The report records
which plans are in the release.

Outcome of this plan:

- the default build contains **no `unsafe`**; the only `unsafe` is behind
  `unsafe-optimizations`;
- no untrusted FEN, UCI or SAN input can panic the public API, and the clock
  counters do not overflow in debug builds;
- structural validation has one source of truth per predicate, not three
  copies that can drift;
- public docs, `#[must_use]`/`# Panics`/`# Errors`, lints, and the public-API
  pin agree with the released surface;
- the package contents, metadata, README, CHANGELOG, and version are
  deterministic and consistent;
- `cargo package`/`cargo publish --dry-run` pass on a clean tree, so the owner
  can publish without further code changes.

Publishing itself is an owner action. This plan prepares and verifies the
package; it does not run `cargo publish`, create tags, or write git history.
`AGENTS.md` restricts this agent to reading git commands.

## Current-tree assessment (2.8.0 baseline)

| Area | Finding |
| --- | --- |
| Safe default | `src/board.rs::piece_key_raw` uses `get_unchecked` **unconditionally**. The README and `src/lib.rs` say the crate is “pure safe Rust by default” and that the only opt-in `unsafe` is the magic-table lookup. The first claim is false and protects nothing at the language level. `src/magic.rs` is already gated correctly. |
| Lint guard | There is no crate-level `forbid(unsafe_code)` for the default feature set, no `deny(unsafe_op_in_unsafe_fn)`, and no `undocumented_unsafe_blocks` warning. `#![warn(missing_docs)]` is present and currently clean. |
| Clock arithmetic | `Board::from_fen` computes `2 * (fullmove - 1) + …` in `u16`; a legal FEN with a large fullmove number can overflow in debug builds and wrap in release. `Board::do_move`, `undo_move`, `rule50_after`, and `hash_and_rule50_after` use unchecked `+ 1`/`- 1` on `u16` clocks. `null_for` already uses saturating arithmetic. |
| Structural validation | The castling “right backed by commoner + rook” check is repeated in `Board::from_fen`, `Board::from_setup`, and `Board::validation_errors`. The en-passant victim check is repeated in `from_fen` and `validation_errors`, with deliberately different strength (the parser is more permissive). Three copies can drift. |
| Public panics | `do_move`/`undo_move` have preconditions but no `# Panics` section. Raw geometry helpers (`attacks::king_attacks`, `knight_attacks`, `pawn_attacks`, `attacks_from`, `between`, the magic sliders) index tables by `Square`; `Square::NONE` panics and is not documented. `Board::blast_zone(Square::NONE)` inherits that panic. |
| Public API consistency | `FenPolicy`, `SetupError`, `AfterState`, and most `types` enums are `#[non_exhaustive]`; `FenError` and `FenLoadError` are not. `FenError`/`FenLoadError` do not implement `Clone`/`PartialEq`/`Eq`, so tests must use `matches!` instead of precise assertions. |
| Robustness tests | `Move::from_bits` has an exhaustive validity test, but the public predicates are not driven with arbitrary move encodings; there is no malformed-FEN/UCI/SAN no-panic corpus; parser overflow boundaries are untested. |
| Package | `Cargo.toml` uses an `exclude` list, so examples are auto-discovered into the package even though the earlier publish decision was “no examples”. Metadata has no `categories`, `documentation`, or `[package.metadata.docs.rs]`. Package content is not explicit, so a new top-level file can silently ship. |
| Release state | `Cargo.toml`, the README dependency snippet, `CHANGELOG.md`, and version references in `src/lib.rs`/`tests/public_api.rs` must be made consistent at release time. |

## Decisions and pushback

1. **Safe-by-default is mandatory, not a preference.** The unchecked Zobrist
   load moves under `unsafe-optimizations`. The default path uses a safe
   indexed load; if the compiler does not elide the bounds check, use masked
   indices (`[sq as usize & 63][raw as usize & 15]`) so the optimizer can
   prove the bounds, keeping the `debug_assert!`s. The gate is a perft
   regression of at most 2 % (median of three runs) and no regression of the
   `do_move`/`hash_after` bench rows. If the safe path is slower than that,
   optimize it; do not retain unconditional `unsafe` to protect a number.

2. **Hardening is a non-behavioral release.** Perft values, move order,
   hashes, FEN output, `ZOBRIST_VERSION`, `Outcome`/`Terminal` precedence and
   every existing API answer stay unchanged for every position a legal game
   can reach. Only two edge classes may change, and both are documented:
   - a large but `u16`-representable fullmove number no longer overflows the
     parser; a field that does not fit `u16` remains a `FenError` as today;
   - a clock at the `u16` boundary saturates (or the private counter widens)
     instead of panicking in debug builds.

3. **Do not merge contracts that are deliberately different.** `from_fen` is
   more permissive about an en-passant target than `validate()` (it checks the
   victim pawn, not that the origin and target squares are empty). Extract
   shared predicates that preserve each caller’s contract; do not route
   `from_fen` through `validation_errors` and accidentally tighten the parser.
   A behavior-freeze test pins `from_fen` acceptance before and after.

4. **No breaking API changes in the wishlist release.** `FenError` and
   `FenLoadError` do **not** become `#[non_exhaustive]` in this plan: that is a
   breaking change for exhaustive downstream matches. Adding `Clone`,
   `PartialEq`, and `Eq` to them is additive and is done. The
   `#[non_exhaustive]` question is recorded as a next-major item in the report.
   If the owner chooses a major version instead, add the attribute in the same
   release and say so in the CHANGELOG; do not mix it into a minor release.

5. **Keep the earlier package decision.** The previous publish decision was
   “only the package, no examples”. The current manifest does not implement
   that because Cargo auto-discovers examples. The plan replaces `exclude` with
   an explicit `include` list that keeps `src`, the tests and their markdown
   data (needed by `include_str!` in unit tests), README, LICENSE, CHANGELOG and
   the manifest, and leaves examples and `docs/` out. If the owner prefers to
   ship examples, add one line to `include`; do not change the decision
   silently.

6. **No new dependency and no required external tool.** Miri, sanitizers,
   `cargo-semver-checks` and `cargo-public-api` are optional audits. If they
   are not installed, the report says so and the in-repo tests remain the gate.
   The crate stays zero-dependency.

7. **The release version is chosen after the code is frozen.** If only the
   additive child-query work landed, the release is the next minor (2.9.0);
   if plan 2 also landed, follow the index’s release strategy. Do not bump the
   version before all implementation reports and gates are green.

## Work items

### 1. Baseline and behavior freeze (first, no code changes)

- Record `git status`, the last commit, `Cargo.toml` version, the
  `[Unreleased]` CHANGELOG section, and which wishlist reports exist.
- Run the existing full suite, `verify_perft` at depth 5 (depth 6 if time
  permits), and `effect_bench 9 2000` three times; record medians.
- Record `cargo package --list` before the manifest change, so the report can
  show exactly what moved out of the package.
- Record a parser acceptance snapshot for the mutation corpus below. Compare
  before/after; any difference not explained by the documented clock/fullmove
  edge behavior is a blocker.

### 2. Safe-by-default `unsafe`

- Move the unchecked Zobrist load in `piece_key_raw` behind
  `#[cfg(feature = "unsafe-optimizations")]`; add the safe branch:

  ```rust
  #[inline(always)]
  fn piece_key_raw(sq: Square, raw: u8) -> u64 {
      debug_assert!((sq as usize) < 64);
      debug_assert!((raw as usize) < 16);
      #[cfg(feature = "unsafe-optimizations")]
      {
          // SAFETY: in-bounds per the invariants above (debug-asserted).
          unsafe {
              *crate::zobrist::ZOBRIST
                  .piece
                  .get_unchecked(sq as usize)
                  .get_unchecked(raw as usize)
          }
      }
      #[cfg(not(feature = "unsafe-optimizations"))]
      {
          crate::zobrist::ZOBRIST.piece[sq as usize][raw as usize]
      }
  }
  ```

- Add crate-level policy in `src/lib.rs`:

  ```rust
  #![cfg_attr(not(feature = "unsafe-optimizations"), forbid(unsafe_code))]
  #![deny(unsafe_op_in_unsafe_fn)]
  #![warn(missing_docs)]
  #![warn(clippy::undocumented_unsafe_blocks)]
  ```

  (`missing_docs` is already present; keep it. Do not add blanket
  `clippy::pedantic`; audit with it but adopt only high-value lints.)
- Verify every remaining `unsafe` block has a `// SAFETY:` comment and is
  covered by the `unsafe-optimizations` gate (`board.rs` hash lookup and the two
  magic-table lookups).
- Update the feature-flag text in `README.md` and `src/lib.rs`: the feature
  now removes bounds checks from the Zobrist table lookup **and** the magic
  table lookup. The default build is safe Rust.

### 3. Clock and parser overflow hardening

- In `Board::from_fen`, compute the game ply in `u32` (`2 * (fullmove - 1) +
  …`); the parsed fullmove is already `u16`, so this cannot overflow. Keep the
  existing `FenError::ParseInt` for fields that do not parse as `u16`; do not
  add a new variant in a minor release.
- Widen the private `Board::game_ply` to `u32` (preferred). Then `do_move` and
  `undo_move` never wrap and restore exactly, including at the old `u16`
  boundary. Keep the public `game_ply() -> u16` getter and document that it
  saturates at `u16::MAX`; `fullmove_number()` computes in `u32` and saturates
  only if the fullmove itself exceeds `u16::MAX` (unreachable from a parsed FEN,
  whose fullmove field is `u16`). If the `Board` size/perft measurement shows a
  regression above 2 %, fall back to explicit `wrapping_add`/`wrapping_sub` on
  the `u16` field: that preserves the current release behavior and exact undo,
  but the wrap must be documented. Do not leave the debug panic.
- In `Board::do_move`, use `saturating_add(1)` for `rule50`; `StateInfo`
  already stores the pre-move value, so `undo_move` restores it exactly.
- In `Board::rule50_after` and `Board::hash_and_rule50_after`, use
  `saturating_add(1)` for the castling and quiet branches. The hash is
  unaffected.
- Update `Board::null_for` to the same `u32` game-ply arithmetic and keep its
  current “fullmove number preserved, side-to-move ply derived” behavior.
- Add `# Panics` sections to `do_move` and `undo_move` (and any other public
  method with a documented precondition that is not already covered), naming
  the malformed-move and stale-`StateInfo` cases. Add `debug_assert!`s where
  they check a caller precondition without touching the release path.
- Add tests at `rule50 = 65535`, fullmove `32768` and `65535`, and a long
  do/undo sequence that crosses the old `u16` game-ply boundary. Assert no panic
  in debug and release, exact undo restoration, the unchanged hash, and the
  documented saturation of the public getters.

### 4. One source of truth for structural validation

- Add crate-private predicates in `src/board.rs` (or a private
  `src/validation.rs`; no new public module):
  - `castling_rights_backed_by_pieces(rights, squares) -> bool`;
  - `ep_victim_pawn_present(side_to_move, ep, squares) -> bool`;
  - `ep_geometry_is_consistent(side_to_move, ep, squares) -> bool` for the
    stricter `validate()` predicate.
- Use the first in `from_fen`, `from_setup`, and `validation_errors`; use the
  second where only the victim check is intended; use the third only in
  `validation_errors`. Map failures to the existing `FenError`,
  `SetupError`, and `ValidationErrors` values exactly as today.
- Add a unit test that enumerates all 16 castling-right combinations against a
  small set of placements and asserts `from_fen`, `from_setup` and
  `validation_errors` agree on the structural castling subset. Add an
  en-passant table with valid, missing-victim, occupied-origin and
  occupied-target rows; assert each caller’s documented acceptance.
- Do not move public items between modules in this step. `board.rs` stays the
  public module; the goal is one predicate per rule, not a line-count refactor.

### 5. Public API and documentation consistency

- Add `Clone`, `PartialEq`, and `Eq` to `FenError` and `FenLoadError`
  (additive). Keep their existing `Display`/`Error`/`source` behavior.
- Audit public functions that take `Square`:
  - `Board::blast_zone` becomes total: return `Bitboard::EMPTY` for
    `Square::NONE` (one branch in a convenience method, not in the generator).
  - Raw `attacks` table helpers keep their no-branch hot path but document
    `# Panics` for `Square::NONE`. Do not add a branch to every attack lookup.
  - Add a test for `blast_zone(Square::NONE)` and a documented panic test only
    if the team wants to pin the raw-helper behavior; otherwise the doc is the
    contract.
- Run `cargo clippy --all-targets --all-features -- -W clippy::pedantic` as an
  audit. Adopt only lints that improve public docs, naming or consistency; do
  not silence the audit with a crate-level pedantic allow.
- Re-run the `#[must_use]` audit on public pure functions added by plans 1–4.
- Audit the blanket `#[allow(missing_docs)]` sites in `src/types.rs`. Document
  the non-obvious public variants (`Color`, `Outcome`, `MoveType`) where the
  attribute hides useful semantics; keep it on the generated square
  constants, where a per-variant doc would be pure noise.
- Ensure every public fallible method has `# Errors` and every public
  panicking method has `# Panics`; keep the doc wording aligned with the new
  safe/unsafe feature contract.
- Extend `tests/public_api.rs` so it pins the **released** version’s named
  items (including the plan 1–4 additions that landed), not only 2.8.0. Keep
  the compile-shape test for the README composite.

### 6. Robustness test suite

Add `tests/robustness.rs` (and its data to the package include list):

- **Arbitrary move bits:** for the 41-position perft corpus, the child
  fixtures and a few seeded play-out positions, iterate every `u16` code whose
  `Move::from_bits` is `Some` and call only the total public predicates:
  `is_pseudo_legal`, `is_legal`, `is_legal_with_state`, `is_capture`,
  `blast_casualties`, `is_zeroing`, `is_double_push`, `Move::write_uci`.
  Assert no panic. Do not feed arbitrary moves to methods documented to panic
  (`hash_after`, `rule50_after`, `san`) — their panic is the contract.
- **FEN no-panic:** a curated malformed corpus (empty, spaces only, non-ASCII,
  `0` counts, nine ranks, short ranks, invalid castling, impossible ep, huge
  clocks, huge fullmove) plus deterministic random ASCII strings bounded in
  length; assert `Board::from_fen` returns `Err` unless the string happens to
  be valid, and never panics.
- **Interchange no-panic:** deterministic random ASCII strings through
  `Move::from_uci`, `Board::move_from_uci`, `Board::move_from_san`,
  `Board::write_san` and `Board::san`; no panic, and `None`/`Err` for garbage.
- **Do/undo invariants:** on the corpus and seeded play-outs, for every legal
  move, clone the board and `StateInfo`, do the move, undo it, and assert FEN,
  hash, `rule50`, `game_ply`, pieces, side to move, castling rights and ep
  target are restored exactly. Include a boundary board with maximum clocks.
- **Parser acceptance freeze:** run the same mutation corpus before and after
  the structural-validation refactor and compare accept/reject and error
  variants. Only the documented clock/fullmove edge acceptance may differ.

### 7. Package and release metadata

- Replace the `exclude` list with an explicit `include` list (Cargo uses
  `include` alone when both are present):

  ```toml
  include = [
      "src/**/*.rs",
      "tests/**/*.rs",
      "tests/*.md",
      "Cargo.toml",
      "Cargo.lock",
      "README.md",
      "CHANGELOG.md",
      "LICENSE",
  ]
  ```

  This keeps the test data used by `include_str!`/`read_to_string` and leaves
  examples, `docs/`, `Fairy-Stockfish/`, `AGENTS.md`, `.git*`, perf artifacts
  and `tests/gen_moves.py` out. If the owner reverses the no-examples decision,
  add `"examples/**/*.rs"` and say so in the report; do not ship them by
  accident through auto-discovery. If examples stay excluded, make the README
  say that the example binaries live in the repository, so a reader of the
  packaged README does not expect them in the crates.io tarball.
- Add metadata:
  - `categories = ["game-development", "games"]`;
  - `documentation = "https://docs.rs/atomic-movegen"`;
  - `[package.metadata.docs.rs] all-features = true` (so the feature-gated
    safe/unsafe paths and docs are built);
  - keep `keywords`, `license`, `readme`, `repository`, `rust-version`.
- Confirm the package still builds and that `tests/perft_values.md` and
  `tests/moves.md` are present; `tests/common/mod.rs` is included by the
  `tests/**/*.rs` glob.
- `Cargo.lock` remains gitignored but is included in the package; it is a
  build artifact of the release and must name the released version after
  `cargo check`.

### 8. Release documentation and version

- Choose the version from the landed plans (index release strategy). Do not
  claim a feature that did not land.
- In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and
  add a `### Fixed`/`### Changed` block for this plan: safe default build,
  clock-boundary behavior, fullmove parser fix, structural-validation cleanup,
  package/docs changes. Keep historical measurement notes attributed
  to the version that measured them.
- Update `Cargo.toml` `version`; update the README dependency snippet; update
  `src/lib.rs`/`tests/public_api.rs` comments that name the current release.
  Do not rewrite historical “measured on 2.8.0” statements.
- Run `cargo check` so `Cargo.lock` matches the new version.

## Verification and release gates

All gates from `plan_wishlist1_index.md`, plus:

```sh
# behavior and safety
cargo test
cargo test --release
cargo test --features unsafe-optimizations
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps
cargo test --doc

# correctness and regression
cargo run --release --example verify_perft 6
cargo run --release --example effect_bench 9 2000   # three runs, medians

# package rehearsal (dirty tree only for rehearsal; never for a real publish)
cargo package --allow-dirty --list
cargo package --allow-dirty
cd target/package/atomic-movegen-X.Y.Z && cargo test && cd -
cargo publish --dry-run --allow-dirty
```

Gate table:

| Gate | Requirement |
| --- | --- |
| `forbid(unsafe_code)` default | compiles; `unsafe` only with `unsafe-optimizations` |
| safe-path cost | perft and `do_move`/`hash_after` medians regress ≤ 2 % vs. the baseline recorded in step 1 |
| clock boundaries | no panic in debug or release; `do_move`/`undo_move` exact; a fullmove that does not fit `u16` is still `Err` |
| parser freeze | no acceptance change outside the documented clock/fullmove edge behavior |
| public docs | `RUSTDOCFLAGS="-D warnings" cargo doc --all-features` clean |
| package content | explicit list matches the decision; test data present; no docs/examples/artifacts if excluded |
| packaged test | `cargo test` from the extracted package passes (proves relative `tests/*.md` paths work) |
| dry run | `cargo publish --dry-run` succeeds from a clean tree |
| no regressions | existing differentials, perft values, hashes, move order and keys unchanged |

Optional audits, recorded as run or unavailable: `cargo miri test` (only if a
nightly Miri toolchain is installed), a sanitizer build, `cargo semver-checks`,
and a build on a second target. None is a release blocker in this
zero-dependency crate.

## Files to touch

- `src/lib.rs` — crate lints, feature-flag docs, release-version references.
- `src/board.rs` — safe `piece_key_raw`, clock arithmetic, structural
  predicates, `# Panics`, `blast_zone` total behavior, error derives, tests.
- `src/setup.rs` — use the shared structural predicates; keep `SetupError`
  mapping; boundary tests.
- `src/attacks.rs`, `src/magic.rs` — `# Panics` docs / any safety-comment
  updates; no new branch in hot lookups.
- `tests/robustness.rs` — new no-panic/overflow/do-undo suite.
- `tests/public_api.rs` — pin the released API and version comments.
- `tests/conformance.rs` or `tests/interchange.rs` — only if the parser freeze
  needs a shared fixture; do not change stored keys or perft values.
- `Cargo.toml` — `include`, metadata, docs.rs, version.
- `README.md`, `CHANGELOG.md`, `LICENSE` — release text and package contents.
- `tests/robustness.rs` and any new fixture file must appear in `include`.
- `docs/plans/feedback/report_wishlist1_5.md` — final report.

## Non-goals

- No new wishlist feature, no new dependency, no new public module.
- No change to perft values, move order, `ZOBRIST_VERSION`, `hash()`/`fen()`
  output, `Outcome`/`Terminal` precedence, or existing API answers.
- No `FenError`/`FenLoadError` `#[non_exhaustive]` in a minor release; record
  it for the next major.
- No table padding or branching in the hot attack lookups just to make
  `Square::NONE` total; document the raw-helper precondition instead.
- No wholesale split of `board.rs` or renaming of public modules in this plan.
  Clean code here means one predicate per rule and consistent contracts, not a
  line-count target.
- No CI configuration, no git tag/commit/publish from the agent, and no
  changes under `Fairy-Stockfish/`.

## Final task

Write `docs/plans/feedback/report_wishlist1_5.md` with:

1. the release version and the list of wishlist plans that landed;
2. the safety audit table: every `unsafe` site, its feature gate, its
   `SAFETY` argument, and whether the default build is `forbid(unsafe_code)`;
3. the clock/overflow matrix: input, old behavior, new behavior, and the test
   that pins it;
4. the structural-validation refactor: predicates added, callers changed, and
   the parser-freeze result;
5. API/doc changes, including the `FenError`/`FenLoadError` decision and the
   `FenError`/`FenLoadError` trait additions;
6. the package manifest diff, the final `cargo package --list`, the extracted-
   package test result, and `cargo publish --dry-run` output summary;
7. the benchmark table (absolute ns and the ratios/gates above) before and
   after the safe-default and clock changes, with the baseline recorded in
   step 1;
8. changed files, problems encountered, deviations with their measurement,
   unresolved parts (at minimum: the deferred `#[non_exhaustive]` change and
   any optional audit that was unavailable), missing tests, and next steps;
9. the owner’s publish checklist (commit, tag `vX.Y.Z`, `cargo publish`,
   `git push --tags`) stated as manual actions, not performed by the agent.
