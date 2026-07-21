# AGENTS.md

## Goal

A fast, correct legal move generator for atomic chess in pure Rust. Use the C++ reference implementation at `./Fairy-Stockfish` as the oracle for correctness.

## Commands

```sh
cargo build
cargo test
cargo clippy
cargo fmt
cargo doc
cargo run --example perft "FEN" DEPTH
cargo run --example verify_perft [MAX_DEPTH]     # run all 41 tests/perft_values.md positions (depth 6 needs --release)
cargo test --test verify_moves                   # verify generated moves against tests/moves.md
```

## Conventions

- Follow standard Rust 2024 edition idioms.
- Use `cargo clippy`, `cargo fmt`, `cargo test` and `cargo doc` to ensure correctness and code quality.
- Keep the crate zero-dependency.
- Avoid `unsafe` by default; prefer safe Rust. If `unsafe` is needed for a
  measurable performance win, document it clearly and guard it appropriately.
- Name public API types and functions clearly; prefer full words over abbreviations.
- Keep `pub` surface minimal; document all public items with doc comments.
- Add `# Panics` and `# Errors` sections to public methods where applicable.
- Enable `#![warn(missing_docs)]` and keep the build warning-free.
- Example binaries go under `examples/`.
- Tests go in a `#[cfg(test)] mod tests` at the bottom of each module.
- The most important quality attributes for this library are in order from most to least important:
  - correctness, performance, maintainability, testability, consistency
- only use reading `git` commands, never writing ones (no `git add`, `git rm`, `git commit`, etc.)
- the rules of atomic chess are proven to be correctly implemented by this library, rely on the existing tests
