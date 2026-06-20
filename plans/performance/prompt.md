The following list describes four known performance optimizations:

| Optimization                 | What it saves                                               | Notes                                               |
| ---------------------------- | ----------------------------------------------------------- | --------------------------------------------------- |
| PEXT                         | & mask \* magic >> shift → single pext instruction          | needs unsafe + CPU feature gating                   |
| Fixed-size StateInfo         | Heap alloc per move (blast can remove up to ~9 pieces)      | [(Square, Piece); 9] + len: u8 instead of Vec       |
| Incremental checkers/pinners | Recompute from scratch on every legal() call                | Store in StateInfo, update incrementally in do_move |
| Magic bitboards              | Use magic bitboards instead of ray-casting (sliding_attack) | -                                                   |

For each case create a dedicated implementation plan:
* `plans/performance/plan_pext.md`
* `plans/performance/plan_state_info.md`
* `plans/performance/plan_checkers_pinners.md`
* `plans/performance/plan_magic.md`

---

put the proposed order into a new file `plans/performance/notes.md

---

Make sure each plan references @Fairy-Stockfish/ as the reference implementation / source of truth. Fairy-Stockfish atomic variant is proven to be correct and very performant.

---

Look at the reports and propose further optimizations.
