# atomic-movegen

A Rust library for legal move generation in the standard `atomic` variant,
validated against the [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish)
reference implementation.

## Features

- Legal move generation (pseudo-legal + legality filtering)
- Single-move queries without building a move list: `Board::is_pseudo_legal()`,
  `Board::is_legal()`, and state-aware `Board::is_legal_with_state()`
- Stable 16-bit `Move` wire encoding (`Move::to_bits()` / `Move::from_bits()`),
  plus `Hash`/`Ord` for use as a map or set key
- UCI move parsing and formatting: syntax-only `Move::from_uci()`,
  board-aware `Board::move_from_uci()` (resolves castling and en passant),
  allocation-free `Move::write_uci()`, and `Display`
- FEN parsing and output (standard notation only — `K`/`k` label the commoner)
- Opt-in semantic validation for untrusted FENs: `Board::validate()`,
  `Board::from_fen_with()`, `FenPolicy`, and `ValidationErrors`. The default
  `Board::from_fen()` stays permissive and unchanged.
- Perft (performance test) recursion
- Blast-on-capture, commoner royalty (one commoner per side, no promotion to
  commoner), pawn blast immunity (except for the capturing pawn at the blast
  square)
- Canonical blast helpers: `Board::blast_zone()` for the 3×3 geometry and
  `Board::blast_casualties()` for the pieces a capture removes, split by color
  and including the en-passant victim
- Rule conveniences: `attacks::attacks_from()`,
  `Board::attackers_to_color()`, board-aware `Board::is_zeroing()` /
  `Board::is_double_push()`, and order-preserving `MoveList::retain()`
- Incremental Zobrist hashing (`Board::hash()`) and FSF-compatible FEN output
- Canonical en-passant keys and FEN output: `Board::legal_ep_square()` and
  `EnPassantMode::Legal` via `hash_with()`, `fen_with()`, and
  `hash_after_with()`. The default `hash()`/`fen()`/`hash_after()` keep the
  Fairy-Stockfish `Always` policy, so existing keys and output are unchanged.
- Game-over detection (`Board::outcome()`, state-aware `outcome_with_state()` /
  `outcome_with()`, and the strict `Board::is_terminal()`)
- Correct terminal precedence: mate outranks the 50-move clock, and only the
  two commoners remaining is a WDL draw. There is deliberately no
  insufficient-material table (K+B/K+N/K+R vs K can be an immediate win), and
  touching commoners are not automatically drawn when other pieces remain.
- Pure safe Rust by default, zero dependencies. The opt-in
  `unsafe-optimizations` feature removes bounds checks from the magic
  bitboard lookup; see [Feature flags](#feature-flags).

## Usage (library)

Add `atomic-movegen` to your `Cargo.toml`:

```toml
[dependencies]
atomic-movegen = "2.4.0"
```

```rust
use atomic_movegen::Board;
use atomic_movegen::perft;

let mut board = Board::new();
let nodes = perft(&mut board, 3);
assert_eq!(nodes, 8902);
```

For untrusted FEN input, use the opt-in validation layer:

```rust
use atomic_movegen::{Board, FenLoadError, FenPolicy};

let board = Board::from_fen_with(
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    FenPolicy::Strict,
).unwrap();
assert!(board.validate().is_ok());

let bad = Board::from_fen_with("8/8/8/8/8/8/8/8 w - - 0 1", FenPolicy::Strict);
assert!(matches!(bad, Err(FenLoadError::InvalidPosition(_))));
```

## Usage (CLI perft)

```sh
cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 4
```

Output: `197326`

## Verify against known perft values

The repository ships with 41 test positions and their expected node counts at
depths 1–6 in [`perft_values.md`](./tests/perft_values.md).  Use the `verify_perft`
example to run all of them:

```sh
# Quick check at depth 3 (~5 s)
cargo run --example verify_perft 3

# Full verification at depth 6 — use --release for acceptable runtime
cargo run --release --example verify_perft
```

The tool exits with status 0 if every position matches, or 1 on any mismatch.
Depth 6 traverses billions of nodes; the `--release` flag is essential for a
timely result at that depth.

## Other CLI tools

All example binaries ship with the repo:

```sh
cargo run --example perft_divide "FEN" DEPTH   # per-move perft breakdown
cargo run --example list_moves "FEN"          # list all legal moves
cargo run --example fen_after "FEN" MOVE      # FEN + moves after playing MOVE (e.g. e2e4)
cargo run --example debug_moves "FEN"         # pseudo-legal vs. legal counts, duplicates
cargo run --example pawn_debug "FEN"          # pawn inspections and manual move checks
cargo run --example test_d5g8                 # attack-table sanity check (hardcoded position)
```

## Feature flags

- `unsafe-optimizations` (off by default): removes the bounds check from the
  magic-bitboard table lookup. All indices remain provably in bounds under
  the magic-table construction invariants; enable it only if you audit that
  argument. It does not change any public API or move-generation semantics.

The combined child-key accessor `Board::hash_and_rule50_after(m)` returns
`(hash_after(m), rule50_after(m))` while classifying the move once, which is
useful when prefetching child keys for every candidate move.

```sh
cargo build --features unsafe-optimizations
cargo test --features unsafe-optimizations
```

## Development

```sh
cargo test        # unit + integration tests (perft assertions, move verification)
cargo clippy      # lints
cargo fmt         # formatting
cargo doc         # API docs
```

Correctness is verified against the [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish)
reference implementation (bundled in this repo as a git submodule) via the
expected values in [`tests/perft_values.md`](./tests/perft_values.md) and the
move lists in [`tests/moves.md`](./tests/moves.md).

## License

MIT
