# Prompt: Create Fix Plan for atomic-movegen

We have verified the `atomic-movegen` Rust crate against its implementation plan (`plans/migration/plan.md`) and found several deviations and bugs.

Your task: produce an **implementation/fix plan** as a markdown document that another agent could follow to fix all issues. Do NOT implement anything — only plan.

## Current state

The project is at `/home/dirk/projects/dirk/golmman/atomic_movegen/`. All source files in `src/`, examples in `examples/`, and the existing plan at `plans/migration/plan.md`. The crate compiles, passes all 25 unit tests, and its perft numbers match Fairy-Stockfish exactly for both tested positions.

## Issues to plan for

### Bug: `between_bb` returns wrong value for non-aligned squares
- **File:** `src/bitboard.rs`, lines 105–107
- **Description:** When two squares are not on the same rank, file, or diagonal, `between_bb` incorrectly returns `square_bb(s2)` instead of `Bitboard::EMPTY`. This is currently latent (only called with aligned squares by `pinned()`) but is wrong code.
- **Fix:** Change line 106 from `return square_bb(s2);` to `return Bitboard::EMPTY;`.

### Stale `src/main.rs`
- **File:** `src/main.rs` (entire file, 3 lines: `fn main() { println!("Hello, world!"); }`)
- **Description:** Leftover from `cargo init`. The crate has been restructured as a library; this stub is unused and should be deleted.

### Missing crate-level documentation
- **File:** `src/lib.rs`
- **Description:** No `//!` doc comment at the crate root. The crate should have a brief description, mention of atomic chess rules, and a usage example (showing `Board::new()`, `perft()`, etc.).

### `README.md` is just a title
- **File:** `README.md` (single line: `# atomic_movegen`)
- **Description:** Needs a library description, usage example (perft CLI invocation), and brief notes on atomic chess and the crate.

### Missing integration tests
- **Description:** The plan mentions test scripts (`tests/perft.sh`, `tests/test_atomic_movegen.sh`) but none exist. Add integration tests that verify:
  - Starting position perft at depths 1–4 matches FS reference values (20, 400, 8902, 197326)
  - Position 2 perft at depth 2 matches FS (1939)
- These can be Rust integration tests (`tests/perft_tests.rs`) or shell scripts.

### Missing edge case unit tests
- **Description:** The plan describes tests for do_move/undo_move state restoration, self-explosion, blast zone, and pinned piece captures. These are absent. Plan what unit tests to add and what each should verify.

## Output format

Write the plan to `plans/fixes/fix_plan.md` in the project root. Structure it as:

```markdown
# Fix Plan

## Priority order
1. Bug fixes first (between_bb, main.rs deletion)
2. Documentation (crate docs, README)
3. Testing (integration tests, edge case unit tests)

## Item: [short name]

### Description
### Files to modify
### Changes needed
### Verification
```

Be specific about files, line numbers, and the exact changes needed. The agent reading this plan should be able to implement it without needing to re-analyze the codebase.

## Do NOT include
- CI (GitHub Actions) — explicitly out of scope
- Magic bitboard performance optimizations — the current ray-casting is correct
- Any changes to `plans/migration/plan.md`

## Reference values

| Position | Depth | FS perft | Our perft |
|----------|-------|----------|-----------|
| Starting position (`rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1`) | 1 | 20 | 20 |
| Starting position | 2 | 400 | 400 |
| Starting position | 3 | 8902 | 8902 |
| Starting position | 4 | 197326 | 197326 |
| Position 2 (`r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1`) | 2 | 1939 | 1939 |
