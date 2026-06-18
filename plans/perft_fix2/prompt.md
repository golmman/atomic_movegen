In the last session we fixed some bugs and improved the parity to the `perft_values.md` table up to depth 4.
See the report at `./plans/perft_fix/report.md`.

The expected perft values for depth 5 and 6 are not met by the current implementation.
We need to fix that.

The code in this repository has been migrated from the reference implementation at `./Fairy-Stockfish`.
The reference implementation is proven to be correct, so the issue must be in the migrated rust code.

Create a plan to fix the atomic move generator correctness by reaching parity with the `perft_values.md` table.
Don't implement anything yet, just build the plan and store it at `./plans/perft_fix2/plan.md`.
