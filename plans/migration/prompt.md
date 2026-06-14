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
