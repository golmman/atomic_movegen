# Implementation report — wishlist 1 plan 5 (hardening, clean code, publish gate)

Status: **implemented.** The tree is packaged and rehearsed as
`atomic-movegen 2.9.0`. No publish, tag, commit, or git write was performed.

## 1. Release version and landed plans

- **Release version:** `2.9.0` (minor: additive APIs only).
- **Landed wishlist plans:**
  - plan 1 — `EffectContext::post_move_and_key_with` (legal-EP child key);
  - plan 2 — `FenPolicy::Lichess` (sanitizing FEN acceptance);
  - plan 3 — `EffectContext::exploding_capture_after` (one-ply child threat).
- **Not landed:** plan 4 (W4 analysis view). Its report records the
  measurement gate as blocked/unevaluable, so it contributes no code to this
  release.
- **Baseline:** commit `532fdd7` (“finish feedback wishlist1 plan4”),
  `Cargo.toml` version `2.8.0`, clean working tree. The baseline benchmark and
  parser acceptance snapshots were taken before any code change.

## 2. Safety audit

Crate-level policy in `src/lib.rs`:

```rust
#![cfg_attr(not(feature = "unsafe-optimizations"), forbid(unsafe_code))]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![warn(clippy::undocumented_unsafe_blocks)]
```

| Site | Feature gate | SAFETY argument | Default build |
| --- | --- | --- | --- |
| `board.rs::piece_key_raw` Zobrist load | `unsafe-optimizations` only | `Square < 64`, piece raw encoding `< 16` (debug-asserted; enforced by the type encodings) | safe indexed load, `forbid(unsafe_code)` active |
| `magic.rs::bishop_attacks` table load | `unsafe-optimizations` only | `sq < 64` and `idx < 1 << (64 - shift)` by the shift and table-builder offset/size invariants | safe indexed load |
| `magic.rs::rook_attacks` table load | `unsafe-optimizations` only | same argument as the bishop lookup | safe indexed load |

There are no `unsafe fn` items. `cargo build`, `cargo test`, and `cargo doc`
pass with `forbid(unsafe_code)` under the default feature set, and with the
three gated blocks enabled under `unsafe-optimizations` / `--all-features`.
`cargo clippy --all-targets --all-features -- -D warnings` is clean.

## 3. Clock and overflow matrix

| Input / operation | Old behavior | New behavior | Test |
| --- | --- | --- | --- |
| FEN fullmove `65535`, White | debug panic in `2 * (fullmove - 1)`; release wrap | exact internal ply `131069`; `game_ply()` saturates at `u16::MAX`, `fullmove_number()` is `65535` | `clock_boundaries_saturate_and_undo_exactly` |
| FEN fullmove `65535`, Black | same overflow | exact internal ply `131070`; same public saturation | `clock_boundaries_saturate_and_undo_exactly` |
| FEN fullmove `65536` | `FenError::ParseInt` | unchanged: `FenError::ParseInt` | `clock_boundaries_saturate_and_undo_exactly`, robustness corpus |
| `do_move` from internal ply `65535` | `u16 += 1` debug panic / release wrap | widened `u32` counter increments exactly; public getter saturates | `clock_boundaries_saturate_and_undo_exactly`, `long_do_undo_crosses_the_old_clock_boundary_exactly` |
| `undo_move` across the old boundary | restore could wrap | exact internal restore (`65536 -> 65535`), exact hash/FEN | same two tests |
| quiet move at `rule50 = 65535` | `u16 += 1` debug panic | `saturating_add(1)`; `undo_move` restores the stored `65535` | `clock_boundaries_saturate_and_undo_exactly`, `do_undo_boundary_clocks_restore_exactly` |
| `rule50_after` / `hash_and_rule50_after` at `65535` | debug panic on increment | saturating clock; hash unchanged | `clock_boundaries_saturate_and_undo_exactly` |
| `null_for` at a large fullmove | saturating `u16` arithmetic | same fullmove rule in `u32` | existing `null_for` tests remain green |

The internal game-ply counter is now `u32`; `Board::game_ply()` keeps its
`u16` return type and documents saturation. No hash, key, FEN output, move
order, or perft answer changed for ordinary positions.

## 4. Structural validation refactor

Added crate-private predicates in `src/board.rs`:

- `castling_rights_backed_by_pieces(rights, squares) -> bool`;
- `ep_victim_pawn_present(side_to_move, ep, squares) -> bool`;
- the existing `ep_geometry_is_consistent(side_to_move, ep, squares)` is the
  stricter single source for the full double-push geometry.

Callers:

| Caller | Castling predicate | EP predicate | Contract kept |
| --- | --- | --- | --- |
| `Board::from_fen` | per-right call, preserving the four existing error strings | rank check + `ep_victim_pawn_present` | permissive: only rank and victim are required |
| `Board::from_setup` | whole mask | rank check + `ep_victim_pawn_present` | same permissive structural setup contract |
| `Board::validation_errors` (`Strict` etc.) | whole mask → `INVALID_CASTLING_RIGHTS` | `ep_geometry_is_consistent` → `INVALID_EP_SQUARE` | stricter: target and origin must be empty |
| `sanitize_lichess` / `lichess_validation_errors` | existing sanitizer behavior | `ep_geometry_is_consistent` | unchanged Lichess drop/reject behavior |

New unit tests:

- `structural_castling_predicate_agrees_across_constructors` enumerates all 16
  castling-right masks over five placements and checks `from_fen`,
  `from_setup`, and `validation_errors`.
- `structural_ep_predicates_preserve_each_caller_contract` covers valid,
  missing-victim, occupied-origin, occupied-target, and wrong-rank rows.

**Parser-freeze result:** a temporary snapshot runner parsed 1,074 frozen
Lichess mutations plus 3,365 generated side/castling/ep/clock mutations (4,439
cases total) with the baseline and final `Board::from_fen`. Output was
byte-identical (`diff` empty) when clocks stayed inside the old non-overflow
range. The only intended differences are the documented fullmove/clock
boundary cases above; the baseline itself panicked on `fullmove = 65535` in
debug, which this plan fixes.

## 5. Public API and documentation

- `FenError` and `FenLoadError` gained `Clone`, `PartialEq`, and `Eq`
  (additive). They intentionally did **not** become `#[non_exhaustive]`; that
  would be a breaking change and is deferred to a major release.
- `Board::blast_zone(Square::NONE)` is now total and returns
  `Bitboard::EMPTY`; `blast_zone_none_is_total` pins it.
- Raw attack helpers (`attacks::king_attacks`, `knight_attacks`,
  `pawn_attacks`, `attacks_from`, `between`; magic bishop/rook/queen) document
  their `Square::NONE` panic instead of adding a branch to every hot lookup.
- `Board::do_move` and `undo_move` gained `# Panics` sections naming the
  malformed-move and stale-`StateInfo` preconditions.
- The blanket `#[allow(missing_docs)]` was removed from `Color`, `Outcome`, and
  `MoveType`; their non-obvious variants are documented. It remains on the
  generated `Square` constants, `File`/`Rank`, and `PieceType`, where per-item
  docs would be noise.
- The `#[must_use]` audit of the plan 1–4 child-query surface found every new
  public pure query already annotated.
- A `clippy::pedantic` audit was run (`730` warnings, mostly pre-existing
  numeric/cast/style classes). It is not enforced crate-wide; no blanket
  `pedantic` allow was added, and the normal `-D warnings` gate is clean.
- `tests/public_api.rs` now pins the 2.9.0 surface, the `FenError` /
  `FenLoadError` trait additions, `FenPolicy::Lichess`, and total
  `blast_zone(Square::NONE)`.

## 6. Package and release rehearsal

`Cargo.toml` changes:

```diff
-version = "2.8.0"
+version = "2.9.0"
+documentation = "https://docs.rs/atomic-movegen"
+categories = ["game-development", "games"]
-exclude = ["Fairy-Stockfish/**", "docs/**", ...]
+include = [
+    "/src/**/*.rs", "/tests/**/*.rs", "/tests/*.md",
+    "/tests/data/*.tsv", "/Cargo.toml", "/Cargo.lock",
+    "/README.md", "/CHANGELOG.md", "/LICENSE",
+]
+
+[package.metadata.docs.rs]
+all-features = true
```

The leading slashes are a necessary deviation from the plan’s literal globs:
without them Cargo’s basename matching also pulled
`Fairy-Stockfish/README.md` and `Fairy-Stockfish/tests/js/README.md` into the
package. Anchoring the patterns removes both.

Final `cargo package --allow-dirty --list` (39 files):

```text
.cargo_vcs_info.json
CHANGELOG.md
Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE
README.md
src/{attacks,bitboard,board,conformance,effects,lib,magic,movegen,position,
     setup,types,util,zobrist}.rs
tests/{child_state,conformance,effects,ep_key,explosion,fen_lichess,
       interchange,move_order_golden,null_for,perft_tests,post_move,
       public_api,robustness,terminal_after,verify_moves}.rs
tests/common/mod.rs
tests/data/fen_lichess_mutations.tsv
tests/moves.md
tests/perft_values.md
```

No examples, `docs/`, `Fairy-Stockfish/`, `AGENTS.md`, git metadata, perf
artifacts, or `tests/gen_moves.py` are packaged. `Cargo.lock` names
`atomic-movegen 2.9.0` after `cargo check`. The published README now says the
example binaries live in the repository, not in the crates.io tarball.

Rehearsal results:

- `cargo package --allow-dirty`: **ok**, 39 files, 1.1 MiB / 245.6 KiB
  compressed. Cargo prints the expected “ignoring example … not included”
  warnings because the examples stay repository-only.
- Extracted `target/package/atomic-movegen-2.9.0 && cargo test`: **all green**
  — 185 unit tests, all 15 integration binaries (including the new
  `robustness` suite), and 18 doctests.
- `cargo publish --dry-run --allow-dirty`: **ok**; packaged and verified
  `2.9.0`, then “aborting upload due to dry run” (exit 0).

## 7. Benchmarks (before / after)

Baseline and final runs on the same machine; `effect_bench 9 2000` three runs
each, `verify_perft 6` once each. Effect-bench cells are medians of the three
run medians. Negative percentages are faster.

| Row / metric | 2.8.0 baseline | 2.9.0 safe default | Delta | Gate |
| --- | ---: | ---: | ---: | --- |
| `verify_perft 6`, total wall | 64.248 s | 64.699 s | **+0.70 %** | ≤ 2 % ✅ |
| `verify_perft` slowest case #13 | 10.025 s | 10.042 s | +0.17 % | ≤ 2 % ✅ |
| `hash_after` (per position, ns) | 6, 6, 6 | 6, 7, 6 | **0 % median** | no regression ✅ |
| `clone + do_move`, `all` class (ns) | 17.0 | 17.1 | **+0.6 %** | no regression ✅ |
| `generate_legal`, deep-win (ns) | 115 | 102 | -11.3 % | — |
| `generate_legal`, middlegame-13 (ns) | 112 | 117 | +4.5 % | within run noise |
| `generate_legal`, middlegame-21 (ns) | 129 | 115 | -10.9 % | — |
| `post_move_and_key`, `all` class (ns) | 25.9 | 27.0 | +4.2 % | not a gate row |
| `post_move_and_key_with(Legal)`, `all` (ns) | 26.4 | 26.8 | +1.5 % | not a gate row |
| `clone + do_move + hash_with(Legal)`, `all` (ns) | 18.8 | 19.1 | +1.6 % | not a gate row |
| double-push `post_move_and_key` (ns) | 23.8 | 25.9 | +8.8 % | small corpus, noise |
| `exploding_capture_after`, `all` (ns) | 79.8 | 81.6 | +2.3 % | not a gate row |

The safe default did not regress the two gate rows (`verify_perft` and
`do_move`/`hash_after`). The clock widening itself is a `u32` field and showed
no measurable cost above the safe-Zobrist change. Raw per-position
`generate_legal` values fluctuate with machine state; the 41-position perft
corpus is the stable regression gate.

## 8. Changed files, problems, deviations, unresolved parts

Changed:

- `src/lib.rs` — crate lints, safe-by-default and feature docs.
- `src/board.rs` — safe `piece_key_raw`; `u32` internal game ply; saturating
  clocks; shared structural predicates; total `blast_zone`; `# Panics`; error
  derives; new boundary/structural tests.
- `src/setup.rs` — shared structural predicates; widened internal ply call.
- `src/attacks.rs`, `src/magic.rs` — `Square::NONE` panic documentation.
- `src/types.rs` — documented `Color`, `Outcome`, `MoveType` variants.
- `tests/robustness.rs` — new arbitrary-bits / malformed FEN / interchange /
  do-undo / boundary suite.
- `tests/public_api.rs` — 2.9.0 API pin and trait additions.
- `Cargo.toml`, `Cargo.lock`, `README.md`, `CHANGELOG.md`.

Problems and deviations:

- The plan’s literal include globs were not anchored and therefore included
  `Fairy-Stockfish/**/README.md`. Leading `/` on every pattern fixes it.
- `cargo package` / `cargo publish --dry-run` emit non-fatal “ignoring example”
  warnings because the examples are intentionally repository-only under the
  kept “no examples in the package” decision. The README states this.
- The parser-freeze baseline could not include `fullmove = 65535` because the
  old code panicked; the freeze used safe clocks and the high-fullmove behavior
  is pinned separately as the documented fix.
- A `clippy::pedantic` audit reports 730 mostly pre-existing style/numeric
  warnings. No high-value correctness issue was found; adopting it wholesale
  would be a separate clean-up, not part of this release.
- Optional audits were unavailable: no nightly/Miri component, no
  `cargo-semver-checks`, no `cargo-public-api`, and only the host
  `x86_64-unknown-linux-gnu` target is installed. The in-repo gates remain the
  release gate.

Unresolved / next steps:

- `FenError` / `FenLoadError` `#[non_exhaustive]`: deferred to the next major
  release, as the plan requires.
- No committed parser-mutation fixture was added; the before/after freeze was
  performed with a temporary runner and is recorded here. A future release can
  turn that 4,439-case snapshot into a checked-in table if desired.
- No external fuzzer or proptest was added (crate stays zero-dependency); the
  deterministic random ASCII and arbitrary-`u16` tests are the in-repo
  substitute.
- `#[must_use]` on older constructors (`Board::new`, `StateInfo::new`) was left
  as-is; the audit scope was the plan 1–4 surface.

## 9. Owner’s publish checklist (manual; not performed)

1. Review and commit the working tree.
2. Confirm `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, README snippet, and
   `tests/public_api.rs` all say `2.9.0`.
3. From a clean tree, run the final gate:
   `cargo test`, `cargo test --release`,
   `cargo test --all-features`,
   `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo fmt --check`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`,
   `cargo run --release --example verify_perft 6`,
   and `cargo publish --dry-run`.
4. Create tag `v2.9.0`.
5. Run `cargo publish`.
6. Run `git push` and `git push --tags`.
