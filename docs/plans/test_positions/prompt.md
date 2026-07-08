Use `Fairy-Stockfish/` to generate test positions for the "atomic" variant.

Start by compiling `Fairy-Stockfish/`

Then write a script which
* runs the Fairy-Stockfish binary
* which simulates random "atomic" variant games
* for every 7th ply records the FEN-string and the list of legal moves in that position
* adds the FEN-string and moves-list to a markdown table in `tests/moves.md`
* records 20 entries in total

----

Now add a test in the rust library which
* parses the FEN-positions and list of moves of `tests/moves.md`
* for each row it generates the moves of the given position via the lib and compares it to the list of moves from `tests/moves.md`

