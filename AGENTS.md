# AGENTS.md

## Goal

A fast, correct legal move generator for atomic chess in pure safe Rust. Use
the C++ reference implementation at `./Fairy-Stockfish` as the oracle for
correctness. The rules of atomic chess are proven to be correctly implemented
by this library; rely on the existing tests.

## Toolchain

- Rust **2024 edition**, MSRV **1.85** (see `Cargo.toml`).
- Zero-dependency crate; do not add external dependencies.

## Commands

```sh
cargo build
cargo test                                  # unit + integration tests (incl. perft_tests, verify_moves)
cargo clippy
cargo fmt
cargo doc

# Examples
cargo run --example perft "FEN" DEPTH                 # node count at a depth
cargo run --example perft_divide "FEN" DEPTH          # per-move perft breakdown
cargo run --example list_moves "FEN"                 # list all legal moves
cargo run --example fen_after "FEN" MOVE              # FEN + moves after playing MOVE (e.g. e2e4)

# Verification against the oracle values in tests/perft_values.md
cargo run --example verify_perft [MAX_DEPTH] [PATH]   # default MAX_DEPTH=6, PATH=tests/perft_values.md
# Depth 6 traverses billions of nodes — use --release:
cargo run --release --example verify_perft

# Targeted test invocations
cargo test --test verify_moves                        # moves against tests/moves.md
cargo test --test perft_tests                         # perft assertions
```

There are 41 positions in `tests/perft_values.md`, each with expected node
counts at depths 1–6. `verify_perft` exits 0 if all match, 1 on any mismatch.

## Source layout

Modules under `src/`:

- `lib.rs` — crate root, public exports (`Board`, `perft`, …).
- `board.rs` — board representation, FEN parsing/make/unmake.
- `movegen.rs` — pseudo-legal and legal move generation.
- `attacks.rs` — attack lookups for sliding/leaping pieces.
- `magic.rs` — magic bitboards for sliders.
- `bitboard.rs` — `Bitboard` type and operations.
- `types.rs` — `Square`, `Move`, `MoveList`, piece/color types.
- `zobrist.rs` — Zobrist hashing.
- `util.rs` — shared helpers.

Example binaries live under `examples/`; integration tests under `tests/`.

## Conventions

- Follow standard Rust 2024 edition idioms.
- Use `cargo clippy`, `cargo fmt`, `cargo test`, and `cargo doc` to ensure correctness and code quality.
- Keep the crate zero-dependency.
- Avoid `unsafe` by default; prefer safe Rust. If `unsafe` is needed for a
  measurable performance win, document it clearly and guard it appropriately.
- Name public API types and functions clearly; prefer full words over abbreviations.
- Keep `pub` surface minimal; document all public items with doc comments.
- Add `# Panics` and `# Errors` sections to public methods where applicable.
- Enable `#![warn(missing_docs)]` and keep the build warning-free.
- Example binaries go under `examples/`.
- Tests go in a `#[cfg(test)] mod tests` at the bottom of each module.
- The most important quality attributes for this library, in order from most to least important:
  - correctness, performance, maintainability, testability, consistency.
- Only use reading `git` commands, never writing ones (no `git add`, `git rm`, `git commit`, etc.).
