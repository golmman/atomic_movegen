# Plan 5 — Terminal reasons and public contracts (MG10, MG11, MG12, MG14)

## Scope

This plan adds the missing “why” to terminality and turns the crate's implicit
engineering contracts into stated, tested contracts:

- **MG10**: a terminal reason alongside `Outcome`.
- **MG11**: the Zobrist key contract (FSF parity, stability, ep modes).
- **MG12**: generation order as a public compatibility contract.
- **MG14**: `Clone`, `Send + Sync`, no hidden allocation on the move path, and
  the semver policy for `#[non_exhaustive]` enums.

It is mostly additive API and documentation, with a small amount of testing
that pins the contracts.

## Current-tree assessment

- `Outcome` is WDL only. `outcome` documents the precedence, but consumers
  cannot ask why a position is decided.
- `hash()`/`hash_with()` are FSF-compatible for the selected ep mode, but the
  docs do not say whether the key table is stable across crate versions.
- `tests/move_order_golden.rs` pins generation order, but the rustdoc does not
  state that the order is a public contract.
- `Board` and `StateInfo` are `Clone`; no `Send + Sync` assertion exists.
- `#[non_exhaustive]` is used on `Color`, `Outcome`, `EnPassantMode`,
  `PieceType`, `MoveType`, and `Piece`, but the semver consequence is not
  documented.
- The hot path (`generate_*`, `legal`, `do_move`/`undo_move`, `hash*`,
  `outcome*`) does not allocate, but this is not stated as a contract.

## Decisions and pushback

1. **`TerminalReason` mirrors the WDL precedence, not the literal
   `is_terminal` predicate.** `outcome()` includes the commoner-only material
   draw; `is_terminal()` does not. The reason API follows `outcome()` so a
   consumer can explain every `Some(Outcome)` value. Document the difference
   explicitly.
2. **Use one `Terminal` value, not a tuple.** A struct with public `outcome`
   and `reason` fields is self-describing and extensible.
3. **Reasons are side-agnostic.** The outcome already carries the perspective.
   Variants:

   ```rust
   pub enum TerminalReason {
       /// A commoner is extinct (the side to move or the opponent).
       Extinction,
       /// No legal moves while the commoner is attacked.
       NoMovesInCheck,
       /// No legal moves while the commoner is safe.
       Stalemate,
       /// Only the two commoners remain (WDL material draw).
       MaterialDraw,
       /// The half-move clock reached 100 and no higher-precedence rule applies.
       FiftyMoveRule,
   }
   ```

   If consumers need which side is extinct, they can query
   `commoners(color)`; do not encode it in the reason and duplicate the
   perspective logic.
4. **Zobrist keys are stable for the 2.x line.** State: the key table and its
   use are FSF-compatible under `EnPassantMode::Always`; they are stable across
   minor/patch releases; any change is a major-version change and will bump
   `ZOBRIST_VERSION`. `EnPassantMode::Legal` is a canonicalization on top of
   the stable base keys, not a separate key table.
5. **Add `ZOBRIST_VERSION`.** A public `u32` constant lets persisted-table
   consumers record the key schema. Start at `1`; increment only when the key
   table or its XOR composition changes.
6. **Generation order is semver-stable.** The order is already an implicit
   compatibility surface. Publish it and keep `tests/move_order_golden.rs` as
   the enforcement. Do not add a second order test that can drift; extend the
   golden file when new fixtures are needed.
7. **No hidden allocation on the move path is a contract, not a benchmark.**
   Document the allocation-free functions. Do not add a global allocator hook
   or a dependency to prove it; use a test with a counting allocator only if it
   can be done dependency-free and without affecting normal builds.
8. **Semver policy for `#[non_exhaustive]` enums.** A new variant is a minor
   release; consumers must include a wildcard arm. Removing/renaming a variant
   is a major release. Document this on every public non-exhaustive enum and in
   the crate docs.

## Implementation

### `src/types.rs` — `TerminalReason`

Add:

```rust
/// Why a position is decided in the WDL view.
///
/// This mirrors [`Board::outcome`](crate::Board::outcome), including the
/// commoner-only material draw. It is not the same as
/// [`Board::is_terminal`](crate::Board::is_terminal), which excludes the
/// material shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TerminalReason {
    Extinction,
    NoMovesInCheck,
    Stalemate,
    MaterialDraw,
    FiftyMoveRule,
}

/// A decided position: the WDL result and the reason it is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Terminal {
    /// The WDL result from the side-to-move perspective.
    pub outcome: Outcome,
    /// The rule that decided the position.
    pub reason: TerminalReason,
}
```

Re-export both from `lib.rs`.

### `src/board.rs` — terminal API

Add:

```rust
impl Board {
    /// Return the WDL outcome and its reason, if decided.
    ///
    /// Uses the same precedence as [`outcome`](Self::outcome):
    /// 1. side-to-move commoner extinct → `Loss`/`Extinction`;
    /// 2. opponent commoner extinct → `Win`/`Extinction`;
    /// 3. only the two commoners remain → `Draw`/`MaterialDraw`;
    /// 4. no legal moves: attacked → `Loss`/`NoMovesInCheck`, safe →
    ///    `Draw`/`Stalemate`;
    /// 5. `rule50 >= 100` → `Draw`/`FiftyMoveRule`.
    ///
    /// Mate outranks the clock: the no-move branch is resolved before the
    /// 50-move branch when the side to move is in check.
    #[must_use]
    pub fn terminal(&self) -> Option<Terminal>;

    /// Same as [`terminal`](Self::terminal), reusing a populated [`StateInfo`].
    #[must_use]
    pub fn terminal_with_state(&self, state: &StateInfo) -> Option<Terminal>;

    /// Hot-path form when the caller already knows whether a legal move
    /// exists. `moves_exist` must be
    /// [`has_legal_move_with_state`](crate::movegen::has_legal_move_with_state).
    #[must_use]
    pub fn terminal_with(&self, state: &StateInfo, moves_exist: bool) -> Option<Terminal>;
}
```

Implement one private classifier shared with `outcome_impl`, so the precedence
cannot drift. Suggested shape: a private
`fn terminal_impl(&self, state, moves_exist) -> Option<Terminal>` and have
`outcome_impl` return `.map(|t| t.outcome)`. Keep the existing `outcome`/`is_terminal`
public behavior unchanged.

### `src/lib.rs` — `ZOBRIST_VERSION`

Add:

```rust
/// Version of the Zobrist key schema.
///
/// This is `1` for the current FSF-compatible key table. It is incremented
/// only when the table or its XOR composition changes; such a change is a
/// major-version change. Persisted transposition tables should store this
/// value with their data and invalidate on a mismatch.
pub const ZOBRIST_VERSION: u32 = 1;
```

Document on `Board::hash`/`hash_with`:

- FSF parity under `EnPassantMode::Always`;
- key stability across 2.x;
- `EnPassantMode::Legal` is a canonicalization, not a new table;
- `ZOBRIST_VERSION` for persisted tables.

### `src/movegen.rs` / `src/lib.rs` — generation order

Add an explicit order contract to:

- `generate_pseudo_legal`;
- `generate_legal`;
- `generate_legal_with_state`;
- `for_each_pseudo_legal` (plan 1);
- `for_each_legal*` (plan 2).

Text:

> Generation order is a public compatibility contract: pawn moves per pawn in
> ascending origin-square order (single push, four promotions in
> `PROMOTION_PIECES` order, double push, captures in ascending destination
> order, en passant), then knights, bishops, rooks, queens, commoners, then
> castling (king side before queen side). `generate_legal` preserves the
> pseudo-legal order after filtering. The order may only change in a major
> release and is enforced by `tests/move_order_golden.rs`.

Add the same statement to the crate-level docs and README. Do not change the
actual order.

### `src/lib.rs` / `README.md` — engineering contract

Add a section covering:

- `Board`, `StateInfo`, `MoveList`, `Move`, `EffectContext`, `Setup`, and the
  public enums are `Send + Sync`; add static assertions in tests.
- The move path does not allocate: `generate_pseudo_legal`,
  `generate_legal*`, `has_legal_move*`, `for_each_*`, `Board::legal`,
  `is_legal*`, `is_pseudo_legal`, `do_move`/`undo_move`, `hash*`,
  `rule50_after`, `outcome*`, `is_terminal`, `blast_casualties`,
  `attackers_to*`, and `Move::write_uci`.
- Allocations are allowed in interchange (`fen`, `to_uci`, `san`, `Setup`
  construction) and in user-facing convenience methods that return `String`.
- `#[non_exhaustive]` enums may gain variants in a minor release; consumers
  must use a wildcard arm. Removing/renaming is major.
- The crate is zero-dependency and safe-Rust-by-default; the only opt-in
  `unsafe` is the `unsafe-optimizations` magic-table lookup.

### `src/types.rs` — semver notes

Add the non-exhaustive semver note to `Color`, `Outcome`, `EnPassantMode`,
`PieceType`, `MoveType`, `Piece`, and the new `TerminalReason`/`CastlingSide`.
Do not remove `#[non_exhaustive]` from existing enums; that would be breaking
for downstream matches.

## Tests

### Terminal reason

`terminal_reason_table` with one row per branch:

| FEN | outcome | reason | `is_terminal` |
| --- | --- | --- | --- |
| side to move extinct | `Loss` | `Extinction` | true |
| opponent extinct | `Win` | `Extinction` | true |
| K vs K | `Draw` | `MaterialDraw` | false |
| checkmate | `Loss` | `NoMovesInCheck` | true |
| checkmate at `rule50 = 100` | `Loss` | `NoMovesInCheck` | true |
| stalemate | `Draw` | `Stalemate` | true |
| `rule50 = 100`, moves exist | `Draw` | `FiftyMoveRule` | true |

Assert `terminal().unwrap().outcome == outcome()` and
`terminal().unwrap().reason`. Also assert `terminal_with_state` and
`terminal_with` agree with `terminal` over fixtures and seeded random playouts.

### Key contract

- `zobrist_golden_keys`: hard-code `hash()`, `hash_with(Always)`,
  `hash_with(Legal)` for startpos, an ep position, a castling position, and a
  promotion position. These are the compatibility guard.
- `zobrist_version_is_stable`: assert `ZOBRIST_VERSION == 1`; update only with
  an announced key-schema change.
- `hash_ep_modes`: positions differing only by a non-capturable ep target have
  equal `hash_with(Legal)` and different `hash()`/`hash_with(Always)`.

### Order contract

- Keep `tests/move_order_golden.rs` unchanged except for additive fixtures.
- Add a test that `for_each_pseudo_legal`/`for_each_legal` match the list order
  (plan 1/2 tests can cover this; do not duplicate the golden corpus).
- Add a doc test or a comment in `movegen.rs` pointing at the golden file.

### Engineering contract

- `send_sync_assertions`: compile-time assertions for `Board`, `StateInfo`,
  `Move`, `MoveList`, `Bitboard`, `Color`, `Piece`, `Outcome`, `Terminal`,
  `EffectContext`, `Setup`, and the public errors.
- `clone_roundtrip`: `board.clone()` compares equal on FEN, hash, side, ep,
  rule50, game_ply, and castling rights.
- If a dependency-free counting allocator is feasible, add an allocation test
  for `generate_legal`, `has_legal_move`, `legal`, `do_move`/`undo_move`,
  `hash_after`, and `outcome` on a fixture. Otherwise document that the
  contract is enforced by code review and do not add a fragile global
  allocator test.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (at least depth 5)
- `cargo test --test move_order_golden`
- `cargo test --test verify_moves`

## Files to touch

- `src/types.rs` — `TerminalReason`, `Terminal`, semver notes.
- `src/board.rs` — terminal API, shared classifier, docs, tests.
- `src/movegen.rs` — order-contract docs.
- `src/lib.rs` — `ZOBRIST_VERSION`, re-exports, crate docs.
- `README.md` / `CHANGELOG.md` — terminal reason, key/order/engineering
  contracts.
- `docs/plans/feedback/report_handoff1_5.md` — final report.

## Non-goals

- No change to `Outcome` variants, precedence, or `is_terminal` semantics.
- No change to the Zobrist table or any hash value.
- No change to generation order.
- No global allocator hook in normal builds, no new dependency.
- No SAN/Setup (plan 4) or conformance corpus (plan 6).

## Final task

Write `docs/plans/feedback/report_handoff1_5.md` with the final terminal-reason
table, golden key values, the exact order/engineering text, test results, and
any contract decision that needs downstream confirmation.
