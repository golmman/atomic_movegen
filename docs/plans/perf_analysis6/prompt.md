**Generate new pert.data**
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
perf record -F 999 -g --call-graph dwarf target/profiling/examples/perft 'r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15' 6

# Start sampling, then run the perft command
sample perft 10 -wait -mayDie -fullPaths -file perft_profile.txt &
target/profiling/examples/perft 'r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15' 6
wait

**Create analysis via first prompt**

**Create plan**

---

The binary was generated via
```
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --example perft
```

The file `perft_profile.txt` was generate via
```
sample perft 10 -wait -mayDie -fullPaths -file perft_profile.txt &
target/profiling/examples/perft 'r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15' 6
wait
```

* Analyze the generated `perft_profile.txt`
* Find performance bottlenecks in this rust library
* Analyze the reports of previous attempts in `docs/plans/perf_analysis5`
* Compare to the reference implementation in `Fairy-Stockfish/`
* Think out of the box to find new ways for performance improvements
* Compile a list of potential performance improvements
* Sort the list by highest potential first
* Store your findings in `docs/plans/perf_analysis6/analysis.md`



* Analyze the generated `perf.data`
* Find performance bottlenecks in this rust application
* Analyze the reports of previous attempts in `docs/plans/perf_analysis4`
  * note that the previous attempts where done on an arm cpu, now you are on a x86_64 cpu
* Compare to the reference implementation in `Fairy-Stockfish/`
* Think out of the box to find new ways for performance improvements
* Compile a list of potential performance improvements
* Sort the list by highest potential first
* Store your findings in `docs/plans/perf_analysis5/analysis.md`

---

Create a plan for the implementation of item 1 of `docs/plans/perf_analysis5/analysis.md`.
Tests against the baseline should be done via `cargo run --release --example verify_perft`.
Store the plan in `docs/plans/perf_analysis5/plan1.md`

---

Analyze where we finished last time: `docs/plans/perf_analysis3/report1.md`
Create a plan for the implementation of the next item(s) of `docs/plans/perf_analysis3/analysis.md`.
Tests against the baseline should be done via `cargo run --release --example verify_perft`.
Store the plan in `docs/plans/perf_analysis3/plan2.md`
