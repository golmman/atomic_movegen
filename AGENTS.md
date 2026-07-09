# AGENTS.md

## Goal

A fast, correct legal move generator for atomic chess in pure Rust. Use the C++ reference implementation at `./Fairy-Stockfish` as the oracle for correctness.

## Commands

```sh
cargo build
cargo test
cargo clippy
cargo fmt
cargo run --example perft "FEN" DEPTH
cargo run --example verify_perft [MAX_DEPTH]     # run all 41 perft_values.md positions (depth 6 needs --release)
```

## Conventions

- Follow standard Rust 2024 edition idioms.
- Use `cargo clippy`, `cargo fmt` and `cargo test` to ensure correctness and code quality.
- Avoid `unsafe` — try to keep the crate entirely safe Rust.
- Name public API types and functions clearly; prefer full words over abbreviations.
- Keep `pub` surface minimal; document all public items with doc comments.
- Example binaries go under `examples/`.
- Tests go in a `#[cfg(test)] mod tests` at the bottom of each module.
- The most important quality attributes for this library are in order from most to least important:
  - correctness, performance, maintainability, testability, consistency
- only use reading `git` commands, never writing ones (no `git add`, `git rm`, `git commit`, etc.)
