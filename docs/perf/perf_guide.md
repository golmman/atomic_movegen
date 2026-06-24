# Rust performance profiling with `perf` on Linux

This guide explains how to find performance bottlenecks in a Rust application on Linux, especially how much CPU time is spent in different functions.

## Is `perf` the right tool?

Yes. For CPU bottlenecks in native Rust applications on Linux, `perf` is usually the right first serious tool.

`perf` can show:

- which functions consume the most CPU time
- call stacks leading to hot functions
- kernel vs userspace CPU cost
- hardware counter data such as cache misses and branch misses
- context switches, page faults, and other runtime events

For Rust specifically, `perf` works well because Rust compiles to native machine code.

## Do not profile a normal debug build

A normal debug build is built with little or no optimization:

```bash
cargo build
```

That is useful for debugging correctness, but it is usually bad for performance profiling because the generated code is very different from production code.

A debug build may have:

- much less inlining
- different function call structure
- different register allocation
- much slower code
- misleading hotspots

Profiling a debug build often tells you where the unoptimized program is slow, not where the real release binary is slow.

## Use release mode with debug symbols

Instead, profile optimized release code with debug symbols enabled.

In `Cargo.toml`:

```toml
[profile.release]
debug = true
```

This keeps release optimizations but includes symbol information so tools like `perf` can map machine-code addresses back to Rust function names and source locations.

This means:

```text
optimized production-like code + readable profiler output
```

Debug symbols generally increase binary size, but they usually do not meaningfully slow down the program.

## Build for profiling

Recommended build command:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release
```

Frame pointers make stack unwinding more reliable for profilers.

If you want a dedicated profiling profile, you can add this to `Cargo.toml`:

```toml
[profile.profiling]
inherits = "release"
debug = true
lto = false
```

Then build with:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling
```

The binary will be under:

```text
target/profiling/
```

## Basic CPU profiling with `perf`

Build the release binary first:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release
```

Then record a profile:

```bash
perf record -F 999 -g --call-graph dwarf target/release/your_binary
```

If your binary takes arguments:

```bash
perf record -F 999 -g --call-graph dwarf target/release/your_binary arg1 arg2
```

Then inspect the result:

```bash
perf report
```

You should see output with an `Overhead` column, for example:

```text
Overhead  Command      Shared Object        Symbol
  35.20%  my_binary    my_binary            my_crate::parser::parse_message
  18.74%  my_binary    my_binary            my_crate::db::decode_row
   9.12%  my_binary    libc.so.6            memcpy
   6.87%  my_binary    my_binary            alloc::raw_vec::RawVec::grow
```

The `Overhead` percentage is approximately the percentage of sampled CPU time attributed to that function or stack.

## Flat function-level report

For a terminal-friendly report:

```bash
perf report --stdio
```

To sort by symbol:

```bash
perf report --stdio --sort=symbol
```

## Flamegraphs

A flamegraph is often the easiest way to visualize where CPU time goes.

Install the Rust flamegraph tool:

```bash
cargo install flamegraph
```

Then run:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo flamegraph --release
```

If your binary takes arguments:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo flamegraph --release -- arg1 arg2
```

This produces:

```text
flamegraph.svg
```

How to read it:

- wider boxes mean more CPU time
- top boxes are leaf functions where CPU was directly spent
- lower boxes are callers
- a wide stack means that path consumed a lot of CPU

## Profiling a server

If the Rust application is a server, run it under `perf`:

```bash
perf record -F 999 -g --call-graph dwarf target/release/your_server
```

Then generate representative load from another terminal or machine.

Stop the server with `Ctrl-C`, then inspect:

```bash
perf report
```

Try to profile realistic traffic. Artificial or tiny workloads can produce misleading profiles.

## Quick runtime counters with `perf stat`

Before recording a full profile, `perf stat` can give a quick overview:

```bash
perf stat target/release/your_binary
```

More detailed counters:

```bash
perf stat -d target/release/your_binary
```

This can show:

- total runtime
- CPU cycles
- instructions
- instructions per cycle
- cache misses
- branch misses
- context switches
- page faults

You can also record specific events:

```bash
perf record -e cycles,instructions,cache-misses,branches,branch-misses -g target/release/your_binary
```

## CPU time vs wall-clock time

`perf` primarily shows CPU time.

That means it is excellent for finding CPU-heavy functions, but it may not fully explain slowness caused by waiting on:

- disk I/O
- network I/O
- databases
- locks
- sleeping
- async tasks waiting to be polled
- external services

If the application is slow but `perf` does not show much CPU activity, the bottleneck may be waiting rather than computation.

In that case, combine `perf` with application-level instrumentation.

## Async Rust and Tokio

For async Rust applications, especially Tokio-based services, CPU profiling is only part of the picture.

Useful tools include:

- `tracing`
- `tracing-subscriber`
- `tokio-console`

These help answer questions like:

- which request path is slow?
- which async task is busy?
- which task is blocked or waiting?
- are tasks yielding properly?
- are locks or channels causing contention?

Use `perf` for CPU hotspots and `tracing` or `tokio-console` for async runtime behavior.

## Memory and allocation profiling

If `perf` shows allocator-heavy functions such as `malloc`, `free`, `memcpy`, `RawVec::grow`, or lots of cloning-related code, investigate memory allocation.

Useful tools:

### `heaptrack`

```bash
heaptrack target/release/your_binary
heaptrack_gui heaptrack.your_binary.*
```

Good for finding allocation-heavy code paths.

### Valgrind Massif

```bash
valgrind --tool=massif target/release/your_binary
ms_print massif.out.*
```

Good for heap growth analysis, but much slower than native execution.

## Deterministic profiling with Callgrind

For smaller, repeatable workloads, `callgrind` can be useful:

```bash
valgrind --tool=callgrind target/release/your_binary
kcachegrind callgrind.out.*
```

This is much slower than `perf`, but it gives deterministic instruction-level data.

Use it for small workloads, not high-throughput production-like runs.

## Benchmarking changes

Profiling tells you where time goes. Benchmarking tells you whether a change helped.

For whole-program command-line benchmarks:

```bash
hyperfine './target/release/your_binary arg1 arg2'
```

For Rust microbenchmarks, use Criterion:

```bash
cargo add --dev criterion
```

Criterion is useful after you have identified a suspicious function or algorithm and want reliable before/after measurements.

## Recommended workflow

1. Build optimized code with symbols:

   ```bash
   RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release
   ```

2. Get a quick overview:

   ```bash
   perf stat -d target/release/your_binary
   ```

3. Record CPU profile:

   ```bash
   perf record -F 999 -g --call-graph dwarf target/release/your_binary
   ```

4. Inspect function-level CPU time:

   ```bash
   perf report
   ```

5. Generate a flamegraph if visual inspection helps:

   ```bash
   RUSTFLAGS="-C force-frame-pointers=yes" cargo flamegraph --release
   ```

6. If the bottleneck is not CPU-bound, add tracing, async instrumentation, or memory profiling depending on the symptoms.

## Tool selection guide

| Need | Tool |
|---|---|
| CPU hotspots by function | `perf report` |
| Visual CPU call stacks | `cargo flamegraph` |
| Hardware counters | `perf stat`, `perf record -e ...` |
| Async Tokio task behavior | `tokio-console`, `tracing` |
| Application-level request timing | `tracing` |
| Allocation bottlenecks | `heaptrack` |
| Heap growth over time | `massif` |
| Deterministic instruction counts | `callgrind` |
| Whole-program benchmark comparison | `hyperfine` |
| Function-level microbenchmarks | `criterion` |

## Common pitfalls

- Profiling a debug build instead of optimized release code.
- Forgetting debug symbols, resulting in poor function names.
- Profiling an unrealistic workload.
- Treating CPU time as wall-clock time.
- Ignoring async waiting, lock contention, database time, or I/O time.
- Over-optimizing a function that is not significant in realistic profiles.
- Comparing performance without stable benchmarks.

## Bottom line

For Rust on Linux, start with:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release
perf record -F 999 -g --call-graph dwarf target/release/your_binary
perf report
```

Or use a flamegraph:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo flamegraph --release
```

Use release builds with debug symbols so you profile production-like optimized code while still getting readable Rust function names.
