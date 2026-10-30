# atomic-movegen

A Rust library for legal move generation in the standard `atomic` variant,
validated against the [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish)
reference implementation.

## Features

- Legal move generation (pseudo-legal + legality filtering)
- FEN parsing and output (standard notation only — `K`/`k` label the commoner)
- Perft (performance test) recursion
- Blast-on-capture, commoner royalty (one commoner per side, no promotion to
  commoner), pawn blast immunity (except for the capturing pawn at the blast
  square)
- Incremental Zobrist hashing (`Board::hash()`)
- Game-over detection (`Board::outcome()`, `Board::is_terminal()`)
- Pure safe Rust, zero dependencies

## Usage (library)

Add `atomic-movegen` to your `Cargo.toml`:

```toml
[dependencies]
atomic-movegen = "2.3.1"
```

```rust
use atomic_movegen::Board;
use atomic_movegen::perft;

let mut board = Board::new();
let nodes = perft(&mut board, 3);
assert_eq!(nodes, 8902);
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
