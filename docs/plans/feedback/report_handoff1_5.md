# Report — Plan 5: Terminal reasons and public contracts (MG10, MG11, MG12, MG14)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| MG10 `TerminalReason` / `Terminal` | Done | New public types in `src/types.rs`, re-exported at the crate root. |
| MG10 `Board::terminal` / `terminal_with_state` / `terminal_with` | Done | One private `terminal_impl` classifier; `outcome_impl` now delegates to it, so the precedence cannot drift. |
| MG10 terminal tests | Done | `terminal_reason_table` (one row per branch) plus terminal agreement folded into the existing `assert_outcome_equivalence` (fixtures + seeded random playouts). |
| MG11 `ZOBRIST_VERSION` | Done | `pub const ZOBRIST_VERSION: u32 = 1` in `src/lib.rs`. |
| MG11 key contract docs | Done | On `Board::hash`, `Board::hash_with`, the crate docs, and the README. |
| MG11 key tests | Done | `zobrist_golden_keys`, `zobrist_version_is_stable`, `hash_ep_modes`. |
| MG12 generation-order contract | Done | Stated on `generate_pseudo_legal`, `generate_legal`, `generate_legal_with_state`, `for_each_pseudo_legal`, the crate docs, and the README. Order unchanged. |
| MG12 order tests | Done | Existing `for_each_pseudo_legal_matches_list`, `legal_stream_matches_legal_list`, `for_each_pseudo_legal_order_matches_golden`, and `tests/move_order_golden.rs` already pin it; no duplicate corpus added. |
| MG14 `Send + Sync` | Done | `send_sync_assertions` compile-time test. |
| MG14 allocation-free move path | Documented | Contract stated in crate docs and README. No global allocator hook added (see decisions). |
| MG14 `Clone` round-trip | Done | `clone_roundtrip` test. |
| MG14 `#[non_exhaustive]` semver policy | Done | Documented on `Color`, `File`, `Rank`, `CastlingSide`, `Outcome`, `EnPassantMode`, `PieceType`, `MoveType`, `TerminalReason`, and in the crate docs/README. |
| README / CHANGELOG | Done | New "Contracts" section and Unreleased entries. |
| Final report | Done | This file. |

No existing behaviour changed: move order, perft values, Zobrist keys, FEN
output, and `Outcome` semantics are untouched. `Outcome` gained a `Hash` derive
(additive) so `Terminal` can derive `Hash`.

## Terminal reason table (as implemented and tested)

| FEN | `outcome` | `reason` | `is_terminal` |
| --- | --- | --- | --- |
| `4k3/8/8/8/8/8/8/8 w - - 0 1` (side to move extinct) | `Loss` | `Extinction` | `true` |
| `8/8/8/8/8/8/8/4K3 w - - 0 1` (opponent extinct) | `Win` | `Extinction` | `true` |
| `4k3/8/8/8/8/8/8/4K3 w - - 0 1` (K vs K) | `Draw` | `MaterialDraw` | `false` |
| `7K/8/8/8/8/8/1Q6/k7 b - - 0 1` (checkmate) | `Loss` | `NoMovesInCheck` | `true` |
| `7K/8/8/8/8/8/1Q6/k7 b - - 100 1` (mate on the clock's last ply) | `Loss` | `NoMovesInCheck` | `true` |
| `7k/8/8/8/8/8/nn6/Kn6 w - - 0 1` (stalemate) | `Draw` | `Stalemate` | `true` |
| `4k3/8/8/8/8/8/8/4K2R w - - 100 1` (clock, moves exist) | `Draw` | `FiftyMoveRule` | `true` |

The test asserts `terminal().unwrap().outcome == outcome()`, the exact
`Terminal`, `is_terminal`, and that `terminal_with_state` / `terminal_with`
agree with `terminal`. The same agreement is checked at every position of the
existing seeded random playouts through `assert_outcome_equivalence`.

## Golden Zobrist keys

| Position | `hash()` / `hash_with(Always)` | `hash_with(Legal)` |
| --- | --- | --- |
| startpos `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1` | `0x5ab84d1cfc77f79e` | `0x5ab84d1cfc77f79e` |
| ep `rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2` | `0x4c47301aad9e4ede` | `0x01965184bb131d9a` |
| castling `r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1` | `0x6c5568b92aee6ad4` | `0x6c5568b92aee6ad4` |
| promotion `n1n5/PPPk4/8/8/8/8/4Kppp/5N1N w - - 0 1` | `0xebf46b7088e725bf` | `0xebf46b7088e725bf` |

`hash_ep_modes` additionally checks that a non-capturable ep target is
canonicalized away by `Legal` while `Always` keeps it.

## Exact contract text

### Generation order (crate docs, `for_each_pseudo_legal`, README)

> Generation order is a public compatibility contract: pawn moves per pawn in
> ascending origin-square order (single push, four promotions in
> `PROMOTION_PIECES` order, double push, captures in ascending destination
> order, en passant), then knights, bishops, rooks, queens, commoners, then
> castling (king side before queen side). `generate_legal` preserves the
> pseudo-legal order after filtering. The order may only change in a major
> release and is enforced by `tests/move_order_golden.rs`.

### Key contract (`Board::hash`, crate docs, README)

> The key table and its XOR composition are compatible with Fairy-Stockfish
> under `EnPassantMode::Always`. They are stable across the 2.x line: any change
> to the table or to how pieces, side, castling, or the ep file are combined is
> a major-version change and bumps `ZOBRIST_VERSION`. Persisted transposition
> tables should store that version with their data and invalidate on a
> mismatch. `EnPassantMode::Legal` is a canonicalization layered on the stable
> base keys; it is not a second key table.

### Engineering contract (crate docs, README)

- `Board`, `StateInfo`, `MoveList`, `Move`, `EffectContext`, `Setup`, and the
  public enums are `Send + Sync`.
- The move path does not allocate: `generate_pseudo_legal`, `generate_legal`,
  `generate_legal_with_state`, `has_legal_move`, `has_legal_move_with_state`,
  `for_each_pseudo_legal`, `for_each_legal`, `for_each_legal_with_state`,
  `Board::legal`, `is_legal`, `is_legal_with_state`, `is_pseudo_legal`,
  `do_move`/`undo_move`, `hash`, `hash_with`, `hash_after`, `hash_after_with`,
  `hash_and_rule50_after`, `rule50_after`, `outcome`, `outcome_with_state`,
  `outcome_with`, `terminal`, `terminal_with_state`, `terminal_with`,
  `is_terminal`, `blast_casualties`, `attackers_to`, `attackers_to_color`, and
  `Move::write_uci`.
- Allocations are allowed in interchange (`fen`, `to_uci`, `san`, `Setup`
  construction) and in convenience methods that return `String`.
- `#[non_exhaustive]` enums may gain variants in a minor release; consumers must
  use a wildcard arm. Removing or renaming a variant is a major release.
- The crate is zero-dependency and safe-Rust-by-default; the only opt-in
  `unsafe` is the `unsafe-optimizations` magic-table lookup.

## Decisions and pushback

1. **`TerminalReason` follows `outcome`, not `is_terminal`.** The material draw
   is included, and the difference is documented on `TerminalReason`,
   `Board::terminal`, and the crate docs. This lets a consumer explain every
   `Some(Outcome)`.
2. **One `Terminal` struct, not a tuple.** Public `outcome`/`reason` fields are
   self-describing and extensible.
3. **Reasons are side-agnostic.** The `Outcome` carries the perspective; the
   extinct side is available from `Board::commoners(color)`.
4. **No allocation test.** A dependency-free counting allocator would require a
   `#[global_allocator]` in the test binary, which is fragile and can perturb
   other tests. The plan allowed documenting the contract instead; the
   allocation-free list is stated in the crate docs and README and enforced by
   code review. No global allocator hook was added.
5. **`Outcome` gained `Hash`.** Required for `Terminal: Hash`; additive and
   non-breaking.
6. **`File`/`Rank` semver notes added too.** They are also `#[non_exhaustive]`
   public enums, so the policy is stated on them for consistency even though the
   plan listed only the others.

## Verification

| Gate | Result |
| --- | --- |
| `cargo test` | Pass: 155 unit (6 new), 10 `effects`, 3 `interchange`, 1 `move_order_golden`, 2 `null_for`, 4 `perft_tests`, 1 `verify_moves`, 8 doctests. |
| `cargo test --release` | Pass (same suite). |
| `cargo test --features unsafe-optimizations` | Pass (same suite; no magic/attack code touched). |
| `cargo clippy --all-targets -- -D warnings` | Pass. |
| `cargo fmt --check` | Pass. |
| `cargo doc --no-deps` | Pass, no warnings. |
| `cargo run --release --example verify_perft 5` | **41/41**, 2.32 s. |
| `cargo test --test move_order_golden` | Pass, unchanged. |
| `cargo test --test verify_moves` | Pass, unchanged. |

Depth 6 was not re-run for this plan (additive API/docs only, no generator,
attack, or hashing code touched); depth 5 is green and the plan's minimum.

## Changed files

- `src/types.rs` — `TerminalReason`, `Terminal`, `Hash` on `Outcome`, and
  `# Semver` notes on `File`, `Rank`, `Color`, `CastlingSide`, `Outcome`,
  `EnPassantMode`, `PieceType`, `MoveType`, `TerminalReason`.
- `src/board.rs` — `Board::terminal`, `terminal_with_state`, `terminal_with`,
  the shared `terminal_impl` (with `outcome_impl` delegating to it), key-contract
  docs on `hash`/`hash_with`, terminal agreement in
  `assert_outcome_equivalence`, and the `terminal_reason_table`,
  `zobrist_golden_keys`, `zobrist_version_is_stable`, `hash_ep_modes` tests.
- `src/movegen.rs` — generation-order contract on `generate_pseudo_legal`,
  `generate_legal`, `generate_legal_with_state`, `for_each_pseudo_legal`.
- `src/lib.rs` — `ZOBRIST_VERSION`, root re-exports of `Terminal` and
  `TerminalReason`, crate-doc sections for keys, terminal reasons, generation
  order, and the engineering contract, plus `send_sync_assertions` and
  `clone_roundtrip` tests.
- `README.md` — terminal-reason feature bullet and a new "Contracts" section.
- `CHANGELOG.md` — Unreleased "Added" entries for terminal reasons,
  `ZOBRIST_VERSION`, the published contracts, and `Outcome: Hash`.
- `docs/plans/feedback/report_handoff1_5.md` — this report.

No dependency, feature flag, or `unsafe` was added; no `Fairy-Stockfish` code
was consulted or linked.

## Problems encountered

1. **`Terminal: Hash` needed `Outcome: Hash`.** `Outcome` did not derive `Hash`.
   Adding the derive is additive and non-breaking, so it was added rather than
   dropping `Hash` from `Terminal`.
2. **The first `hash_ep_modes` fixture was capturable.** `3pP3` with a white
   pawn on `e5` and ep target `d6` is a legal en-passant capture, so `Legal`
   correctly kept the key. The fixture was changed to `3p4` (no adjacent white
   pawn), which is the non-capturable case the test intends.
3. **Doc-link targets.** The plan's engineering list named `Board::to_uci` and
   `Board::attackers_to_with_state`, which do not exist. The crate docs use the
   real API (`Move::to_uci`, `Board::attackers_to_color`) and `PROMOTION_PIECES`
   is linked through `types::PROMOTION_PIECES` because it is not re-exported at
   the root.

## Deviations from the plan

- **No allocation test / global allocator hook.** Documented instead, as the
  plan's fallback permits.
- **`File`/`Rank` semver notes added** beyond the listed enums.
- **`Outcome` gained `Hash`** (not explicitly listed, required by `Terminal`).

## Unresolved parts / missing tests

- **No allocation proof.** The allocation-free contract is documentation plus
  code review; there is no automated guard.
- **No external differential for terminal reasons.** The oracle is the crate's
  own `outcome`/`is_terminal` and the FSF precedence already encoded there.
- **`TerminalReason` does not say which side is extinct** (by design); callers
  query `Board::commoners`.
- **No `Terminal`-aware SAN suffix.** SAN still emits `#` from the child
  `Outcome`; it could use `TerminalReason` to distinguish extinction from mate
  without changing the text, but that is not needed and was left out.

## Next steps

1. **Plan 6 (conformance corpus)** can now freeze `Terminal`/`TerminalReason`
   alongside FEN → moves, outcome, keys, and clocks, and record
   `ZOBRIST_VERSION` in the corpus header.
2. If a downstream consumer needs an allocation guarantee, add a
   dependency-free counting allocator behind a dedicated test target (not a
   global hook in the normal build).
3. If the key schema ever changes, bump `ZOBRIST_VERSION`, update
   `zobrist_golden_keys`, and treat it as a major release.
