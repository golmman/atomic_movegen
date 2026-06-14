### Summary

I want a rust library for the move generation of **atomic** chess.

### Ideas

- reference implementation: `~/projects/dirk/golmman/Fairy-Stockfish`
  - the C++ code is correct, clean and highly performant
  - translate this move generator to rust in this repository
  - Fairy-Stockfish supports many other variants, for this project we only want **atomic** though
- don't use unsafe rust if not absolutely necessary
  - document every use of unsafe rust
- test via "perft"
  - perft in the reference determines the number of nodes for a given depth
  - implement perft in this project as well
  - the README of the reference lists established perft numbers which can be testet against
- the new rust library is intended to be used as a dependency in other rust projects
  - add a cli example project which performs perft so the correctness can be manually verified

### Task

Question my ideas and push back where necessary.
Create a detailled implementation plan and write it to `./plans/migration/plan.md`.

---

Plan written to ./plans/migration/plan.md. Here's a summary of my main pushbacks:
1. "Translate the move generator" is misleading — the movegen is a thin layer over attack tables, board state, and variant rules. ~80% of the work is types, bitboards, attacks, FEN parsing, and legality. The plan is bottom-up with clear intermediate deliverables.
2. Don't copy Stockfish's monolith — it's architected for a full UCI engine with NNUE, search, threading, variants, etc. Ours should be a clean Rust library with no eval/search/uci baggage. Variant with 100+ fields → a few const bools.
3. No unsafe needed — magic bitboards work with const arrays. Skip pext. If added later, gate behind a feature flag.
4. Edition 2024 → 2021 for tooling compatibility.
5. The perft numbers are in tests/perft.sh, not README — six atomic positions with expected node counts, including chess960 — which means we likely need 960 castling support.
6. Open questions in the plan — nocheckatomic variant support, magic init strategy (const vs lazy), and allocation strategy for move lists.

---

My replies:
1. the goal is to have a standalone rust library for atomic movegen, if that means we need to copy the attack-tables, board state, etc. then we need to do it as well.
2. exactly, we don't want evaluation, search, etc., only the movegen.
3. very good, if still needed for some reason it must be documented.
4. why would edition 2021 be better for this project?
5. look again, in the README there is a section "## Perft numbers for atomic" with a table of FENs and numbers to test against
6. open questions:
  1. only ONE variante: atomic, no other variant
  2. no chess960 castling
  3. prefer const where possible
  4. use a simple Vec for the first implementation

---

A few things I'd flag:
1. Perft test cost. The README table tests go to depth 5 and 6. Position 2 at depth 6 is 1.6 billion nodes. A debug build will take hours — these really need --release. Worth adding a note that smoke tests (perft.sh depth-2/4 positions) are for fast iteration and the full table only runs in release.
2. Zobrist hashing — is it worth the complexity? The plan mentions hash keys in StateInfo. Stockfish uses them for position dedup (draw detection) and transposition tables — neither of which we need for perft. If we skip Zobrist entirely, we simplify do_move/undo_move significantly. For a movegen-only library, I'd drop it unless you have a use case.
3. do_move signature. Stockfish takes StateInfo* as an out-parameter. In Rust the alternatives are:
- fn do_move(&mut self, m: Move, state: &mut StateInfo) — caller manages memory, matches C++ semantics
- fn do_move(&mut self, m: Move) -> StateInfo — returns owned StateInfo, simpler but forces copy on undo
   Which do you prefer?
4. CLI format for examples/perft. Should it match Stockfish (go perft <depth>)? Or something simpler like <fen> <depth> printing the total? And should it support divide output (per-move breakdown)?
5. no_std ambition. Any plans for embedded/wasm targets? If not, Vec and std::error::Error are fine; if yes, the design changes early (no Vec, different error model).

---

My replies:
1. i agree, let's take shallower depths for smoke tests
2. agreed, let's skip the zobrist hashing
3. performance is important, so i guess the first option is best here, right?
4. it can be very simple, also no divide output necessary
5. no plans for embedded/wasm/no_std
