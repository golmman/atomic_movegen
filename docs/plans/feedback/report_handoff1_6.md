# Report — Plan 6: Conformance corpus as data (MG13)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| `src/conformance.rs` | Done | New public data module with `ConformanceCase` and `CASES`. |
| `ConformanceCase` shape | Done | FEN, ordered pseudo/legal UCI moves, outcome, terminal reason, `is_terminal`, both ep key modes, `rule50`, `game_ply`. |
| Frozen corpus | Done | 37 curated cases, including all 14 `tests/perft_tests.rs` positions. |
| Root re-export | Done | `CONFORMANCE_CASES` and `ConformanceCase` are re-exported at the crate root. |
| `tests/conformance.rs` | Done | Five tests verify every field, category coverage, unique names, legal/pseudo consistency, and the root alias. |
| Doc example | Done | Module-level doctest consumes `CASES`. |
| README / CHANGELOG | Done | New “Conformance corpus” section and Unreleased entry. |
| Final report | Done | This file. |

No engine behaviour changed. The corpus is inert data generated from this
crate's own engine, whose move generation and perft values are already verified
against Fairy-Stockfish. No GPL-derived data was used.

## Corpus size and category coverage

**37 cases.** The test `conformance_corpus_covers_rules` requires every
category below to remain present, and requires `perft-1` through `perft-14`
exactly.

| Category | Cases |
| --- | --- |
| Start position | `startpos` |
| Quiet middlegame | `quiet-middlegame` |
| Capture middlegame | `capture-middlegame`, `capture-heavy-middlegame` |
| En passant | `en-passant-legal`, `en-passant-non-capturable` |
| Castling | `castling-both`, `castling-through-check` |
| Promotion | `promotion-push-and-capture`, `promotion-capture-black` |
| Touching commoners | `touching-commoners` |
| Check | `check-single-escape` |
| Checkmate | `checkmate`, `checkmate-on-clock` |
| Stalemate | `stalemate` |
| Extinction | `extinction-side-to-move`, `extinction-opponent` |
| Material draw | `material-draw-k-vs-k` |
| 50-move rule | `fifty-move-draw`, `checkmate-on-clock` |
| Pin | `pinned-piece` |
| Blast win | `blast-removes-enemy-commoner` |
| Self-explosion | `self-explosion-capture` |
| Maximum legal count | `max-legal-68` (68 legal moves) |
| Perft corpus | `perft-1` … `perft-14` |

`startpos` and `perft-1` intentionally share a FEN; names are unique. The
`max-legal-68` FEN was found by a temporary depth-4 scan of the 41-position
perft corpus and is
`8/8/5q2/3N3p/1r5P/4q3/6Kk/8 b - - 25 63`.

## Exact public data shape

```rust
pub struct ConformanceCase {
    pub name: &'static str,
    pub fen: &'static str,
    pub pseudo_legal: &'static [&'static str],
    pub legal: &'static [&'static str],
    pub outcome: Option<Outcome>,
    pub terminal_reason: Option<TerminalReason>,
    pub is_terminal: bool,
    pub hash_always: u64,
    pub hash_legal: u64,
    pub rule50: u16,
    pub game_ply: u16,
}

pub static CASES: &[ConformanceCase] = &[ /* 37 cases */ ];
```

The crate root exposes:

```rust
pub use conformance::{CASES as CONFORMANCE_CASES, ConformanceCase};
```

`ConformanceCase` derives `Debug`, `Clone`, and `Copy`. Moves are UCI strings in
generation order. `hash_always` is `Board::hash()` /
`hash_with(EnPassantMode::Always)`; `hash_legal` is
`hash_with(EnPassantMode::Legal)`.

## Verification

| Gate | Result |
| --- | --- |
| `cargo test` | Pass: 156 unit, 5 `conformance`, 10 `effects`, 3 `interchange`, 1 `move_order_golden`, 2 `null_for`, 4 `perft_tests`, 1 `verify_moves`, 9 doctests. |
| `cargo test --test conformance` | Pass: 5/5. |
| `cargo clippy --all-targets -- -D warnings` | Pass. |
| `cargo fmt --check` | Pass. |
| `cargo doc --no-deps` | Pass, no warnings. |
| `cargo run --release --example verify_perft 5` | **41/41**, 2.31 s. |
| `cargo test --test move_order_golden` | Pass, unchanged. |
| `cargo test --test verify_moves` | Pass, unchanged. |
| `cargo test --features unsafe-optimizations --test conformance --test move_order_golden --test verify_moves` | Pass. |

The conformance test checks, for every case:

- the FEN parses;
- `generate_pseudo_legal` UCI order equals `pseudo_legal`;
- `generate_legal` UCI order equals `legal`;
- `outcome()` equals `outcome`;
- `terminal().map(|t| t.reason)` equals `terminal_reason`;
- `is_terminal()` equals `is_terminal`;
- `hash()` equals `hash_always`;
- `hash_with(EnPassantMode::Legal)` equals `hash_legal`;
- `rule50()` and `game_ply()` equal the fields;
- `ZOBRIST_VERSION` is the version the corpus was generated with (`1`).

Additional tests enforce unique names, category coverage, that the legal list is
an ordered subsequence of the pseudo-legal list, and that the root
`CONFORMANCE_CASES` alias matches the module data.

## Changed files

- `src/conformance.rs` — new public data module with `ConformanceCase`, `CASES`,
  source comments per case, and a module doctest.
- `src/lib.rs` — `pub mod conformance`, root re-exports, and a crate-doc
  “Conformance corpus” section.
- `tests/conformance.rs` — five verification tests.
- `README.md` — feature bullet and “Conformance corpus” usage section.
- `CHANGELOG.md` — Unreleased “Added” entry.
- `docs/plans/feedback/report_handoff1_6.md` — this report.

Temporary generators (`examples/gen_conformance.rs` and
`examples/find_max.rs`) were used to produce the data and the 68-move fixture,
then removed; no generator is committed.

## Problems encountered

1. **Three initial fixtures did not exercise their intended category.**
   - `extinction-opponent` originally had no white commoner, so it was a
     side-to-move extinction loss. It now uses
     `8/8/8/8/8/8/8/4K3 w - - 0 1` (opponent extinct, win).
   - `checkmate-on-clock` originally used a lowercase `k` on h8, creating an
     impossible two-black-commoner board. It now uses the report's
     `7K/8/8/8/8/8/1Q6/k7 b - - 100 1` (mate outranks the clock).
   - `self-explosion-capture` originally had no black commoner, so the position
     was already an extinction win. It now uses
     `4k3/8/8/8/8/8/8/3qK3 w - - 0 1`, where the commoner capture is a
     self-explosion and one other legal move remains.
2. **The “one escape” check fixture was replaced.** The first in-check fixture
   had four legal escapes. The corpus now uses
   `7k/5P2/8/8/8/8/8/K6R b - - 0 1`, which has exactly one legal move.
3. **The 68-move position was not in the 14-position `perft_tests` list.** A
   temporary depth-4 scan of the full 41-position perft corpus found it; the
   fixture is recorded as `max-legal-68`.

## Excluded or adjusted cases

No case was excluded because its expected value was ambiguous. The three
fixtures above were adjusted because their original FENs did not represent the
intended rule situation. The corpus deliberately does not include the full
perft tree, a generated build-time file, JSON/YAML, or any GPL-derived fixture.

## Unresolved parts / missing tests

- The corpus is curated, not exhaustive. It does not cover every possible
  atomic rule interaction, only the categories listed above.
- There is no runtime `ConformanceCase::check()` helper. The data is inert and
  the integration test is the verifier, which keeps the public surface small.
- The corpus does not store `ZOBRIST_VERSION` per case; the test pins the
  version the data was generated with. A future key-schema change must update
  the data, the assertion, and `ZOBRIST_VERSION` together.

## Next steps

1. Treat the corpus as frozen: any change to a case's expected values must be
   announced as a rules fix or a major-version change.
2. If a downstream needs a machine-readable export, add a separate example that
   serializes `CASES`; do not add a parser dependency to the crate.
3. If the key schema changes, bump `ZOBRIST_VERSION`, regenerate the corpus,
   update the version assertion, and update `zobrist_golden_keys`.
