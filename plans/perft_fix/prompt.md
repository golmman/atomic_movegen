Create a plan to fix the atomic move generator.

Currently the expected perft values are not met by the tests.

The target perft values are in `./perft_values.md`.

Find the reference implementation at `/Users/d.kretschmann/projects/dirk/golmman/Fairy-Stockfish`.

The perft values are generated via `echo -e "setoption name UCI_Variant value atomic\nposition fen '<FEN>'\ngo perft <depth>" | /Users/d.kretschmann/projects/dirk/golmman/Fairy-Stockfish/src/stockfish`, where `<FEN>` and `<depth>` have to be replaced with FEN-position and tree depth.

Don't implement anything yet, just build the plan and store it at `./plans/perft_fix/plan.md`.
