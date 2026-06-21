I want a plan for a repeatable iterative approach to increase the performance of this library.

Given the benchmark command

```sh
cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 5
```

I want a well documented iteration for every time the plan is executed:

| Stage          | File          | Task                                                                                                     |
| -------------- | ------------- | -------------------------------------------------------------------------------------------------------- |
| Setup          | N/A           | Create a new directory `mkdir -p docs/perf/iter/"$(date '+%Y-%m-%dT%H-%M-%S')"` for the following files. |
| Recap          | `1-recap.md`  | Summarize the previous attempts.                                                                         |
| Analysis       | `2-perf.md`   | Run `hyperfine` on the benchmark command and analyze the results.                                             |
| Plannning      | `3-plan.md`   | Create an implementation plan for proposed changes.                                                      |
| Implementation | `4-impl.md`   | Implement the implementation plan, test for regressions, then create a report.                           |
| Analysis       | `5-report.md` | Redo the `hyperfine` analysis on the benchmark command and create a final report.                             |

I am not a performance expert so push back if my ideas need polish.
At this moment, do your research and create the plan, don't execute it yet.
