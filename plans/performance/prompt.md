
### Known optimizations

| Optimization                 | What it saves                                               | Notes                                               |
| ---------------------------- | ----------------------------------------------------------- | --------------------------------------------------- |
| PEXT                         | & mask \* magic >> shift → single pext instruction          | needs unsafe + CPU feature gating                   |
| Fixed-size StateInfo         | Heap alloc per move (blast can remove up to ~9 pieces)      | [(Square, Piece); 9] + len: u8 instead of Vec       |
| Incremental checkers/pinners | Recompute from scratch on every legal() call                | Store in StateInfo, update incrementally in do_move |
| Magic bitboards              | Use magic bitboards instead of ray-casting (sliding_attack) | -                                                   |

