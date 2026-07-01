RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft

perf record -F 999 -g --call-graph dwarf target/profiling/examples/perft 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1' 6

perf report
perf report --stdio


* Analyze the generated `perf.data`
* Find performance bottlenecks in this rust application
* Analyze the reports of previous attempts in `docs/plans/perf_analysis`
* Compare to the reference implementation in `Fairy-Stockfish/`
* Compile a list of potential performance improvements
* Sort the list by highest potential first
* store your findings in `docs/plans/perf_analysis2/analysis.md`

---

Create a plan for the implementation of item 1 of `docs/plans/perf_analysis2/analysis.md`.
Tests against the baseline should be done via `cargo run --release --example verify_perft`.
Store the plan in `docs/plans/perf_analysis2/plan1.md`

---

Analyze where we finished last time: `docs/plans/perf_analysis2/report1.md`
Create a plan for the implementation of the next item(s) of `docs/plans/perf_analysis2/analysis.md`.
Tests against the baseline should be done via `cargo run --release --example verify_perft`.
Store the plan in `docs/plans/perf_analysis2/plan2.md`
