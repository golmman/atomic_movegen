RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft

perf record -F 999 -g --call-graph dwarf target/profiling/examples/perft 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1' 6

perf report
perf report --stdio


* Analyze the generated `perf.data`
* Find performance bottlenecks in this rust application
* Compare to the reference implementation in `Fairy-Stockfish/`
* Compile a list of potential performance improvements
* Sort the list by highest potential first
* store your findings in `docs/plans/perf_analysis/analysis.md`

---

Create a plan for the implementation of item 1 of `docs/plans/perf_analysis/analysis.md`.
Store the plan in `docs/plans/perf_analysis/plan1.md`
