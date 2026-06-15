|Optimization|What it saves|Notes|
|---|---|---|
|PEXT	& mask * magic >> shift → single pext instruction	~30% faster sliding attack lookup; needs unsafe + CPU feature gating
|Fixed-size StateInfo	Heap alloc per move (blast can remove up to ~9 pieces)	[(Square, Piece); 9] + len: u8 instead of Vec
|Incremental checkers/pinners	Recompute from scratch on every legal() call	Store in StateInfo, update incrementally in do_move
|Uses ray-casting (sliding_attack) instead of magic bitboards. Plan suggested magic bitboards but also allowed simpler alternatives. Correct but slower.

PEXT is the biggest single-speedup for attack lookups. The alloc-removal changes (MoveList + fixed StateInfo) matter most for perft — in search they'd also be big, but for a movegen-only library they're the main perf lever since do_move/undo_move is the hottest path.
