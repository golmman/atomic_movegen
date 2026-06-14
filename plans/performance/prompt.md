Optimization	What it saves	Notes
PEXT	& mask * magic >> shift → single pext instruction	~30% faster sliding attack lookup; needs unsafe + CPU feature gating
Fixed-size MoveList	Heap alloc per movegen call (perft calls this millions of times)	Stack-allocated array of 256 Moves, no Vec
Bulk-counting perft	One level of recursion + undo at leaf nodes	At depth=1 or depth=2, just return move_count instead of recursing
Fixed-size StateInfo	Heap alloc per move (blast can remove up to ~9 pieces)	[(Square, Piece); 9] + len: u8 instead of Vec
Incremental checkers/pinners	Recompute from scratch on every legal() call	Store in StateInfo, update incrementally in do_move

PEXT is the biggest single-speedup for attack lookups. The alloc-removal changes (MoveList + fixed StateInfo) matter most for perft — in search they'd also be big, but for a movegen-only library they're the main perf lever since do_move/undo_move is the hottest path.
