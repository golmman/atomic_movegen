# Plan 6 — Conformance corpus as data (MG13)

## Scope

Export a small, frozen conformance corpus that a downstream can run against its
own rules implementation. Each case records:

- a FEN;
- the legal moves in generation order;
- the WDL outcome and terminal reason (when decided);
- the literal `is_terminal` flag;
- the Zobrist keys under both en-passant modes;
- the half-move clock and game ply after the position.

The corpus is public data, not a new rules implementation. The crate's own test
suite verifies every case against the engine, so the published data cannot
drift silently.

## Current-tree assessment

- `tests/move_order_golden.rs` already contains ordered pseudo-legal and legal
  UCI lists for a useful set of positions.
- `tests/perft_values.md` and `tests/perft_tests.rs` contain the perft corpus.
- `tests/moves.md` contains legal move lists, sorted rather than ordered.
- There is no public data module and no single place that records outcome,
  keys, clocks, and terminality together.
- Plan 5 adds `TerminalReason`/`Terminal` and `ZOBRIST_VERSION`; plan 6 should
  follow it so the corpus can include reasons.

## Decisions and pushback

1. **The corpus is Rust data, not a generated file at build time.** A public
   `ConformanceCase` struct and a `pub static CASES` array are easy to consume,
   require no parsing dependency, and work in `no_std`-ish consumers (the
   crate itself still uses `std`).
2. **Moves are UCI strings in generation order.** UCI is already stable and
   unambiguous for the corpus. SAN is human-friendly but adds parser surface;
   do not use SAN here.
3. **Include both pseudo-legal and legal lists.** The handoff asks for legal
   moves; the pseudo list is also useful for order/legality differentials.
   Keep both fields, clearly named.
4. **Keys are recorded under both ep modes.** `hash_always` and `hash_legal`
   make the ep canonicalization contract observable. Include
   `ZOBRIST_VERSION` on the case or as a crate constant (plan 5).
5. **The corpus is curated, not the full perft tree.** Aim for roughly 25–40
   cases covering the rules and edge cases. Do not publish millions of nodes.
6. **No runtime verification API that can fail silently.** The data is inert;
   a test verifies it. Optionally add a small `ConformanceCase::check()` helper
   only if it does not complicate the public surface.
7. **Do not include GPL-derived data.** Generate expected values from this
   crate's own engine and its existing FSF-verified perft/move fixtures. The
   corpus is MIT, like the crate.

## Proposed API

### `src/conformance.rs` (new module)

```rust
/// One frozen conformance case.
#[derive(Debug, Clone, Copy)]
pub struct ConformanceCase {
    /// Short human-readable name.
    pub name: &'static str,
    /// Position FEN, including clocks.
    pub fen: &'static str,
    /// Pseudo-legal moves in generation order, as UCI strings.
    pub pseudo_legal: &'static [&'static str],
    /// Legal moves in generation order, as UCI strings.
    pub legal: &'static [&'static str],
    /// WDL outcome from the side-to-move perspective, if decided.
    pub outcome: Option<Outcome>,
    /// Terminal reason mirroring `Board::outcome`, if decided.
    pub terminal_reason: Option<TerminalReason>,
    /// Literal `Board::is_terminal`.
    pub is_terminal: bool,
    /// `Board::hash()` / `hash_with(EnPassantMode::Always)`.
    pub hash_always: u64,
    /// `Board::hash_with(EnPassantMode::Legal)`.
    pub hash_legal: u64,
    /// Half-move clock of the position.
    pub rule50: u16,
    /// Raw game ply of the position.
    pub game_ply: u16,
}

/// The frozen conformance corpus.
pub static CASES: &[ConformanceCase] = &[ /* ... */ ];
```

Re-export `ConformanceCase` and `CASES` as `CONFORMANCE_CASES` from the crate
root if that matches the existing re-export style; at minimum expose
`atomic_movegen::conformance::{CASES, ConformanceCase}`.

## Corpus contents

Include at least:

1. start position;
2. a quiet middlegame;
3. a capture-heavy middlegame;
4. a position with a legal en-passant capture;
5. a position with a non-capturable stored ep target (to exercise
   `hash_with(Legal)`);
6. castling available for both sides/colors;
7. castling through an attacked square (illegal castling);
8. a promotion push and a promotion capture, all four promotion pieces;
9. touching commoners (immunity);
10. side to move in check with one escape;
11. checkmate (no moves while attacked);
12. stalemate (no moves while safe);
13. commoner extinction (side to move and opponent);
14. K vs K material draw;
15. a 50-move draw;
16. a checkmate on the 50-move clock (mate outranks clock);
17. a pinned piece that cannot move;
18. a capture whose blast removes the enemy commoner;
19. a self-explosion capture that is illegal;
20. a position from each of the 14 `perft_tests` FENs.
21. a position where the largest observed legal move count (68) occurs;
22. a promotion-heavy position from `tests/move_order_golden.rs`.

The exact list can be adjusted to keep the file readable. Record the source of
each FEN in a comment (perft position number, golden name, or hand-written
fixture).

## Implementation

1. Add `src/conformance.rs` and `pub mod conformance;` in `lib.rs`.
2. Define `ConformanceCase` and `CASES` as above.
3. Generate the expected values with a temporary example or test helper:

   ```sh
   cargo run --example gen_conformance -- "FEN"
   ```

   The helper prints the FEN, ordered pseudo/legal UCI lists, outcome, reason,
   `is_terminal`, keys, `rule50`, and `game_ply`. Do not commit the generator as
   a public example unless it is useful; a `#[cfg(test)]` helper is enough.
4. Paste the values into `CASES`. Keep one case per block with a short comment.
5. Add `tests/conformance.rs` that verifies every case:

   - FEN parses;
   - `generate_pseudo_legal` UCI order equals `pseudo_legal`;
   - `generate_legal` UCI order equals `legal`;
   - `outcome()` equals `outcome`;
   - `terminal().map(|t| t.reason)` equals `terminal_reason` (after plan 5);
   - `is_terminal()` equals `is_terminal`;
   - `hash()` equals `hash_always`;
   - `hash_with(EnPassantMode::Legal)` equals `hash_legal`;
   - `rule50()`/`game_ply()` equal the fields;
   - `ZOBRIST_VERSION` matches the version the corpus was generated with.
6. Add a doc example that consumes one case:

   ```rust
   use atomic_movegen::conformance::CASES;
   for case in CASES {
       assert!(!case.name.is_empty());
   }
   ```

7. Add a short `README.md` section and a `CHANGELOG.md` entry. Mention that
   downstream differentials should treat any difference not announced as a
   rules fix as a veto, matching handoff §5.

## Test design

- `conformance_corpus_verifies`: the full invariant list above.
- `conformance_corpus_covers_rules`: count cases by category (castling, ep,
  promotion, check, mate, stalemate, extinction, material, clock) so a future
  edit cannot remove a category silently.
- `conformance_names_unique`: names and FENs are unique (or intentionally
  duplicate with different comments; enforce unique names at least).
- `conformance_legal_is_subset_of_pseudo`: every legal UCI appears in the
  pseudo list.
- `conformance_keys_match`: if the corpus is regenerated after a key change,
  this test fails until the data and `ZOBRIST_VERSION` are updated together.

## Verification

- `cargo test`
- `cargo test --test conformance`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (at least depth 5)
- `cargo test --test move_order_golden`
- `cargo test --test verify_moves`

## Files to touch

- `src/conformance.rs` — new data module.
- `src/lib.rs` — module/re-export and docs.
- `tests/conformance.rs` — verification test.
- `README.md` / `CHANGELOG.md` — corpus description and usage.
- `docs/plans/feedback/report_handoff1_6.md` — final report.

## Non-goals

- No generated build script, no new dependency, no JSON/YAML parser.
- No full perft tree or million-position corpus.
- No GPL-derived fixtures or data.
- No replacement of `tests/move_order_golden.rs`; the corpus is an additional,
  public, reusable view.
- No new rules logic; the test verifies the existing engine.

## Final task

Write `docs/plans/feedback/report_handoff1_6.md` with the final case count, the
category coverage, the exact public data shape, verification results, and any
cases that had to be excluded because the expected value was ambiguous.
