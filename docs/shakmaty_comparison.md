# Speed comparison: `atomic-movegen` vs `shakmaty` (atomic chess)

**Question:** which move generator is faster for atomic chess — this crate
(`atomic-movegen` 2.3.1) or [`shakmaty`](https://github.com/niklasf/shakmaty)
(0.30.2, `Atomic` variant)?

**Answer: `atomic-movegen` is consistently faster — about 2.3–2.4× on perft
(2.8× on the start position at depth 6) and about 3× on move generation in
isolation. `shakmaty` was faster on none of the 41 benchmark positions.**

All numbers below were measured on the same machine, with the same compiler and
identical build flags, after verifying that both engines generate exactly the
same legal moves.

---

## 1. Verdict (headline numbers)

| Metric | `atomic-movegen` | `shakmaty` | Speedup |
| --- | --- | --- | --- |
| Perft depth 4, 41 positions (13,495,921 nodes) | 153.6 Mn/s | 65.3 Mn/s | **2.35×** |
| Perft depth 5, 41 positions (399,109,774 nodes) | 153.4 Mn/s | 65.1 Mn/s | **2.35×** |
| Perft depth 6, start position (118,926,425 nodes) | 198.1 Mn/s | 70.0 Mn/s | **2.83×** |
| Move generation only, 1,170 positions | 4.7 ns/legal move | 15.1 ns/legal move | **3.19×** |

Per-position speedup range at depth 5: **1.12× – 2.82×**, median **2.10×**,
mean **2.04×**. 0 of 41 positions favour `shakmaty`.

Throughout, "Mn/s" means perft nodes per second, single-threaded.

---

## 2. Methodology

### 2.1 Environment

| Item | Value |
| --- | --- |
| CPU | Apple M1 (aarch64; `/proc/cpuinfo` reports Icestorm-M1 + Firestorm-M1 cores; Asahi Linux kernel `7.1.13-402.asahi.fc44.aarch64+16k`), 4 vCPU quota, no SMT |
| Cache | L1d 768 KiB over 8 instances, L2 16 MiB over 2 instances (`lscpu`), 16 KiB pages |
| OS | Linux (container) |
| Toolchain | `rustc 1.98.1` / `cargo 1.98.1` |
| `atomic-movegen` | 2.3.1, path dependency `../atomic_movegen` |
| `shakmaty` | 0.30.2, `features = ["variant"]` (default features otherwise) |

Both libraries are compiled in the **same** benchmark binary with the same
optimisation profile (`opt-level = 3`, `lto = "fat"`, `codegen-units = 1`),
which is exactly `atomic-movegen`'s own `[profile.release]`. Benchmarks are
single-threaded; nothing is pinned to a core (`taskset`/`perf` are unavailable in
this container).

### 2.2 What is timed

Three independent measurements, each driving the libraries through their own
public APIs:

1. **Perft** — `atomic_movegen::perft(&mut Board, depth)` vs
   `shakmaty::perft(&Atomic, depth)`. This is the realistic end-to-end metric:
   generation + make/unmake + recursion. Both functions have the same structure
   (recurse over the legal move list) and live in their respective crates, so
   neither is handicapped by harness code.
2. **Move generation only** — `movegen::generate_legal(&board, &mut list)` vs
   `position.legal_moves()`, over a fixed corpus of positions, with a
   `black_box` on the returned slice so the generation cannot be optimised away.
3. **Cost split** — generation alone vs generation + playing every legal move, to
   see where the difference comes from.

Timing: `std::time::Instant` around each timed block, **best of N** runs
(N = 3–9 depending on the block) to suppress noise. Node counts are always
compared between the two engines and with the oracle values, so no time is
measured on a wrong answer.

### 2.3 Positions

The 41 positions and their perft values from
[`tests/perft_values.md`](../tests/perft_values.md) (start position, openings,
middlegames, tactical endgames, pawn-only endings). The movegen-only benchmarks
additionally use the ~1,170 positions reachable in one ply from those 41 FENs
plus the start position's depth-2 tree, so the corpus contains ordinary
middlegame positions rather than only the benchmark FENs.

---

## 3. Correctness pre-check (is the comparison apples-to-apples?)

Before trusting any timing, both engines were verified against the
Fairy-Stockfish oracle values:

* `atomic-movegen`: 41/41 positions match the expected node counts at
  **depths 1–6** — 12.46 G nodes over all six depths, 12,046,524,862 of them at
  depth 6 alone. This is also what the crate's own test-suite asserts.
* `shakmaty` 0.30.2: 41/41 positions match the same values at depths 1–6
  (verified in the same run: `correctness: 0 mismatches ... depth 1..=6`).

A stronger check was done on the movegen-only corpus: for 1,618 positions the
**set of legal moves** (as sorted `(from, to, promotion)` triples) was compared
and was **identical** for both libraries in 1,618/1,618 positions
(`move-set differences: 0 / 1618`; snippet in §7.4). Both therefore implement the same rules
(blast on capture, pawn blast immunity, pseudo-royal last commoner), and the
timings measure the same work.

---

## 4. Results

### 4.1 Perft throughput, all 41 positions

```
perft depth 3: n=488312      atomic 3.528 ms (138.4 Mn/s)  shakmaty  7.664 ms ( 63.7 Mn/s)  2.17x
perft depth 4: n=13495921    atomic 87.96 ms (153.4 Mn/s)  shakmaty 206.51 ms ( 65.4 Mn/s)  2.35x
perft depth 5: n=399109774   atomic  2.604 s (153.3 Mn/s)  shakmaty  6.120 s ( 65.2 Mn/s)  2.35x
```

The ratio is stable from depth 3 to depth 5, i.e. it is not a fixed-overhead
artefact: both engines amortise per-node costs the same way, and
`atomic-movegen` simply does less work per node.

### 4.2 Start position, depth 6

```
depth 6, rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
  atomic-movegen: 600.3 ms  (198.1 Mn/s)
  shakmaty:      1698.7 ms  ( 70.0 Mn/s)   -> 2.83x
```

The start position is the "widest" tree (largest branching factor, most
captures into crowded 3×3 blast zones) and shows the largest gap.

### 4.3 Per-position detail, depth 5

"atomic ms" / "shakmaty ms" are wall times (best of 3); "Mn/s" are nodes per
second; "speedup" = shakmaty time / atomic time.

| # | nodes | atomic ms | Mn/s | shakmaty ms | Mn/s | speedup | FEN |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 4864979 | 23.8 | 204.64 | 67.1 | 72.46 | 2.82x | `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1` |
| 2 | 51225398 | 274.9 | 186.37 | 730.8 | 70.09 | 2.66x | `rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4` |
| 3 | 4643560 | 32.2 | 144.00 | 74.3 | 62.46 | 2.31x | `rnb1kb1r/p5p1/2pNp2p/1p1q1p2/3P1Pn1/N5P1/PPP4P/R1BQKB1R b KQkq - 6 10` |
| 4 | 5470489 | 48.9 | 111.85 | 102.8 | 53.24 | 2.10x | `r4b1r/p1N1k1p1/2pNp2p/5p2/1n1P1P2/6PP/PP6/R1B1K3 b Q - 4 17` |
| 5 | 1178188 | 9.9 | 118.58 | 18.6 | 63.22 | 1.88x | `r1k4r/p4Np1/N1p1p2p/5p2/3P1P2/6PP/8/5K2 w - - 0 22` |
| 6 | 1123137 | 10.2 | 110.40 | 20.0 | 56.09 | 1.97x | `r1k1r3/p5p1/N1p4p/2N2p2/5P2/6PP/8/5K2 w - - 2 25` |
| 7 | 510726 | 5.7 | 90.14 | 11.1 | 46.22 | 1.95x | `r1k5/p7/N1p5/2N4p/7P/8/5r2/4K3 w - - 0 31` |
| 8 | 120067 | 1.4 | 88.76 | 2.1 | 56.65 | 1.57x | `r2k4/p7/N1p5/7p/7P/8/2K5/8 w - - 2 34` |
| 9 | 530284 | 7.2 | 73.14 | 10.2 | 52.19 | 1.40x | `3r4/8/8/5k1p/4KN1P/p1p5/8/8 w - - 6 44` |
| 10 | 1106412 | 19.6 | 56.32 | 25.4 | 43.53 | 1.29x | `5r2/8/4k1N1/4K2p/7P/p7/8/2q5 w - - 0 48` |
| 11 | 448630 | 9.5 | 47.46 | 13.2 | 33.90 | 1.40x | `8/4N3/8/4K2p/1r3qkP/8/8/q7 w - - 16 59` |
| 12 | 11357575 | 89.1 | 127.46 | 128.4 | 88.44 | 1.44x | `8/4N3/8/7p/1r3q1P/6K1/7k/q7 b - - 21 61` |
| 13 | 53833718 | 364.4 | 147.72 | 881.4 | 61.08 | 2.42x | `rnbqkbnr/ppp4p/4p1p1/3pNp2/7Q/4P3/PPPP1PPP/RNB1KB1R b KQkq - 1 5` |
| 14 | 9587386 | 62.8 | 152.64 | 145.8 | 65.77 | 2.32x | `r1b1Brk1/ppp5/6pp/3p4/5p2/P3PP2/1P4PP/R4RK1 b - - 1 15` |
| 15 | 11427491 | 74.2 | 154.03 | 170.4 | 67.07 | 2.30x | `r4r1k/pp3B2/4b1pp/2p1P3/1P3p2/P2p1P2/6PP/R4RK1 w - - 0 20` |
| 16 | 19827612 | 147.9 | 134.08 | 323.1 | 61.37 | 2.18x | `r4r1k/pp3B2/4b1pp/4P3/5p2/P4P2/6PP/2Rq1RK1 w - - 0 22` |
| 17 | 1105307 | 10.5 | 104.80 | 19.0 | 58.23 | 1.80x | `5r1k/pp3B2/4b1pp/4P3/5p2/P4P2/6PP/6K1 w - - 0 25` |
| 18 | 367216 | 4.3 | 85.21 | 6.6 | 55.97 | 1.52x | `5r1k/p7/4P1pp/8/8/P4P2/1p5P/6K1 w - - 0 31` |
| 19 | 342968 | 4.5 | 75.64 | 7.5 | 45.89 | 1.65x | `5r1k/p3P3/6pp/8/8/P4P2/7P/1q4K1 w - - 0 32` |
| 20 | 5204468 | 31.3 | 166.42 | 80.6 | 64.54 | 2.58x | `5r1k/p3P3/6pp/8/8/P4P2/6KP/1q6 b - - 1 32` |
| 21 | 22933256 | 151.0 | 151.89 | 354.3 | 64.73 | 2.35x | `r1bqkbnr/pppp2pp/2n5/4ppB1/1P1P4/7N/P1P1PPPP/RN1QKB1R b KQkq - 1 4` |
| 22 | 25911511 | 176.1 | 147.11 | 408.7 | 63.41 | 2.32x | `r1b1kbnr/pppp1Np1/2n5/5p1p/1P1Pp3/4P3/P1P2PPP/RN1QKB1R b KQkq - 1 7` |
| 23 | 22943916 | 147.1 | 155.98 | 337.5 | 67.98 | 2.29x | `r1b2rk1/ppp3p1/5n2/3p3p/1PBPpp2/4PP1P/P1P3P1/RN1QK2R w KQ - 0 12` |
| 24 | 27551989 | 154.5 | 178.28 | 395.0 | 69.75 | 2.56x | `r1b2rk1/ppp5/5n2/3p2pp/1PBP4/4P2P/P1P5/RN1Q1R1K w - - 0 15` |
| 25 | 2730671 | 21.4 | 127.88 | 45.2 | 60.37 | 2.12x | `r1b4k/pp6/8/3p2pp/1P6/2P1P2P/P7/RQ5K b - - 0 20` |
| 26 | 16852028 | 101.1 | 166.74 | 252.9 | 66.64 | 2.50x | `5r1k/pp6/8/3p1bpp/1P6/2P1P2P/P7/R4QK1 b - - 4 22` |
| 27 | 870201 | 8.0 | 108.24 | 14.7 | 59.25 | 1.83x | `7k/pp6/8/1PP5/6pp/7P/P3b3/4R1K1 b - - 0 28` |
| 28 | 64138 | 0.8 | 76.72 | 1.4 | 45.32 | 1.69x | `8/pp6/7k/1PP4R/P7/7p/7K/5b2 b - - 6 33` |
| 29 | 68206 | 0.8 | 86.98 | 1.4 | 47.84 | 1.82x | `8/8/7k/P6R/8/7p/4b2K/8 b - - 7 42` |
| 30 | 5890 | 0.1 | 45.34 | 0.1 | 40.58 | 1.12x | `8/8/8/8/5R2/6kp/7K/8 b - - 3 47` |
| 31 | 7772721 | 54.4 | 142.99 | 127.5 | 60.98 | 2.34x | `r1bqk2r/pp4pp/2p5/1B1p1pNQ/1b3P2/2N1P3/PP3nPP/R1B1K2R b KQkq - 2 11` |
| 32 | 22058800 | 140.4 | 157.06 | 339.2 | 65.02 | 2.42x | `r1b2rk1/1p4pp/p1p5/1B3p2/Pb1p1P2/2N1P1PP/1P3n2/R1B2RK1 b - - 0 15` |
| 33 | 54748199 | 325.8 | 168.04 | 829.7 | 65.99 | 2.55x | `r4rk1/1p4pp/p1p1b3/5p2/PbB2P2/BPN1P1PP/3p1n2/R4RK1 b - - 3 18` |
| 34 | 834846 | 7.0 | 120.06 | 14.6 | 57.30 | 2.10x | `r5k1/1p4pp/p1p5/8/P4P2/BP4PP/5n2/R2r2K1 w - - 1 22` |
| 35 | 3716784 | 27.7 | 134.37 | 60.3 | 61.68 | 2.18x | `r5k1/7p/p2B2p1/1pp2P2/P7/1P1n2PP/8/6K1 b - - 0 26` |
| 36 | 237457 | 2.5 | 95.82 | 4.6 | 51.83 | 1.85x | `r5k1/7p/p2B4/2p3P1/Pp5P/1P5n/8/6K1 w - - 1 30` |
| 37 | 1261700 | 10.2 | 123.79 | 22.2 | 56.96 | 2.17x | `r5k1/7p/p2B3P/6P1/Ppp2n2/1P6/8/5K2 b - - 0 32` |
| 38 | 1935593 | 16.4 | 118.11 | 34.4 | 56.29 | 2.10x | `r7/5k1P/p2B4/8/Pp3n2/1Pp5/8/5K2 w - - 1 35` |
| 39 | 806923 | 6.9 | 117.43 | 13.8 | 58.39 | 2.01x | `8/5k2/p7/8/Pp3n2/1P6/1Bp5/5K2 b - - 1 37` |
| 40 | 74716 | 1.3 | 56.23 | 1.9 | 40.26 | 1.40x | `8/5k2/p7/8/P7/1P1n4/2p5/5K2 w - - 0 39` |
| 41 | 1454618 | 8.3 | 174.71 | 20.2 | 71.87 | 2.43x | `8/5k2/p7/8/P7/1P1n1K2/8/2q5 b - - 1 40` |
| **Σ** | **399109774** | **2594.1** | **153.85** | **6117.8** | **65.24** | **2.36x** | |

Observations:

* The gap is *largest* on open, high-branching middlegame positions (2.3–2.8×)
  and *smallest* on sparse pawn endings (1.1–1.9×). This is the expected shape:
  `shakmaty`'s cost grows with the number of pseudo-legal moves (it clones the
  position per pseudo-legal move to test legality), and it has little
  per-position overhead that could dominate in tiny trees. Position 30 (5,890
  nodes) is the only near-tie at 1.12×.
* `atomic-movegen`'s absolute throughput varies with position type too
  (45–205 Mn/s), but it stays ahead everywhere.

### 4.4 Move generation only

Corpus: 1,170 positions (one ply from each of the 41 benchmark FENs plus the
start position's depth-2 tree), 20 passes each, best of 7:

```
movegen: 1170 positions x20
  atomic-movegen: 110 ns/call   (4.7 ns per legal move, 23.2 moves/call)
  shakmaty:       350 ns/call   (15.1 ns per legal move)
  -> 3.19x
```

A second, larger corpus (1,618 positions: a sample of the start position's
depth-3 tree plus the depth-2 trees of positions 9–41; 26.4 moves/call; best of
9) gives the same ordering with slightly higher absolute costs (larger working
set):

```
generation: 32360 calls  atomic 174.3 ns/call (6.61 ns/move)
                         shakmaty 413.0 ns/call (15.66 ns/move)  -> 2.37x
```

So the difference is **not** in recursion or tree-walking code: it is already
present at the level of a single `legal_moves()` call.

### 4.5 Where the time goes

The same 1,618-position corpus, 32,360 calls / 853,620 moves (best of 5 here vs
best of 9 in §4.4, hence the few-% difference in the absolute ns); "make/play"
is the delta between "generation + making every legal move" and "generation
only":

| | `atomic-movegen` | `shakmaty` |
| --- | --- | --- |
| generation only | 186.9 ns/call | 440.0 ns/call |
| generation + make all legal moves | 552.7 ns/call | 614.3 ns/call |
| make/play, attributed per move | 13.9 ns/move | 6.6 ns/move |
| clone position | 4.95 ns/call | 2.83 ns/call |
| `size_of` position | 144 bytes | 136 bytes |

Two things follow:

1. **Generation dominates** both engines. For the depth-5 run, there are
   14.0 M "expand a node" events (nodes at depths 0–4) over 2.604 s / 6.120 s,
   i.e. **186 ns (atomic) vs 437 ns (shakmaty) per expanded node** — almost
   exactly the isolated generation costs above. Making/unmaking moves is a small
   fraction of perft time in both.
2. **`shakmaty`'s make-move is actually ~2× cheaper than
   `atomic-movegen`'s** (6.6 ns clone+`play_unchecked` vs 13.9 ns
   `do_move`+`undo_move`, which also restores the full `StateInfo`). Its problem
   is purely that it needs ~2.35× more time to *produce* the legal move list, and
   that it additionally clones the position once per node inside `perft`.

(Caveat: the 13.9 ns figure includes two `black_box(&board)` barriers that are
needed to keep the optimiser from removing the make/unmake effect; the derived
"per expanded node" numbers above are the robust way to read this table.)

---

## 5. Why `atomic-movegen` wins

`shakmaty` is a general-purpose library covering standard chess, Antichess,
Atomic, Crazyhouse, Horde, King of the Hill, Racing Kings and Three-check. Its
`Atomic` implementation is deliberately the simple version, and the source says
so (`src/position.rs`, v0.30.2):

```rust
// Atomic move generation could be implemented more efficiently.
// For simplicity we filter all pseudo legal moves.
moves.retain(|m| {
    let mut after = self.clone();
    after.play_unchecked(*m);
    if let Some(our_king) = after.board().king_of(self.turn()) {
        (after.board.kings() & after.board().by_color(!self.turn())).is_empty()
            || after.king_attackers(our_king, !self.turn(), after.board.occupied()).is_empty()
    } else {
        false
    }
});
```

i.e. **one full position clone + make + attack query per pseudo-legal move**,
including quiet moves that can never be illegal in atomic chess; and
`perft()` adds another clone per legal move. There is no public undo API, so a
`shakmaty` user cannot avoid the clone-based pattern without reimplementing the
variant.

`atomic-movegen` is specialised for this one variant and attacks the legality
problem directly:

* the costly "does the blast kill my own commoner" test is pure bitboard
  arithmetic on a *virtual* post-move occupancy in `Board::legal` — no board
  mutation, no clone (`src/board.rs`);
* on top of that, a node-constant fast path (`TrivialLegality`) accepts a move
  without any legality computation at all when the side is not in check, the move
  is not en passant/castling, is not a capture, and its origin is neither a
  commoner nor pinned — which covers the majority of quiet moves;
* checker/pin/commoner-count information (`StateInfo`) is maintained
  incrementally; make/unmake restores it from the saved state, so there is no
  cloning per node either;
* magic bitboards for sliders, precomputed 3×3 blast masks, stack-allocated
  `MoveList`, incremental Zobrist hashing.

This is exactly the "same rules, less work per node" situation: identical node
counts and identical move sets, 2.35× fewer nanoseconds per node.

---

## 6. Caveats

* **Generality is not measured here.** This compares a specialised atomic
  move generator with a deliberately unoptimised variant inside a multi-variant
  library. It says nothing about `shakmaty`'s standard-chess move generator,
  which is a different, carefully optimised code path.
* **API-level cost is included.** `shakmaty`'s perft figure includes
  clone-per-node and clone-per-pseudo-legal-move costs. That is the cost a
  `shakmaty` user pays (there is no `undo_move` in its public API), so it is the
  fair end-to-end number — but a hypothetical "shakmaty with undo" would narrow
  the gap somewhat. Section 4.5 quantifies this: making moves is ~2× cheaper in
  `shakmaty`, but it is a minor part of the total.
* **One machine, one toolchain, one version.** Absolute throughput on this Apple
  M1 (aarch64, ≈3 GHz) is not transferable to x86 or to other Rust
  versions; the **ratios** were stable across all depths (2.17× at depth 3,
  2.35× at depths 4 and 5, 2.83× on the depth-6 start position) and across all
  41 positions (1.12×–2.82×), which suggests the ratio is robust.
* **Movegen-only throughput depends on the working set.** The same code measured
  4.7 ns/legal move on a 1,170-position corpus and 6.6 ns/legal move on a
  1,618-position corpus (both engines show this effect). Prefer the perft
  numbers, which traverse real trees.
* **Best-of-N timing** suppresses noise but is slightly optimistic in absolute
  terms; both engines get the same treatment.
* The benchmark harness lives outside the crate (`atomic-movegen` must stay
  zero-dependency); the crate itself was not modified for this comparison.

---

## 7. Reproduction

### 7.1 Setup

```sh
mkdir atomic-bench && cd atomic-bench
cargo init --bin
```

`Cargo.toml`:

```toml
[package]
name = "atomic-bench"
version = "0.1.0"
edition = "2021"

[dependencies]
atomic-movegen = { path = "../atomic_movegen" }          # 2.3.1
shakmaty = { version = "0.30", features = ["variant"] }  # 0.30.2

# identical to atomic-movegen's own release profile
[profile.release]
lto = "fat"
codegen-units = 1
opt-level = 3
```

Then `cargo run --release -- <path>/tests/perft_values.md 6 5` with the harness
below (`perft_values.md` is the table from this repo's `tests/`). It prints the
correctness check, the depth-4/5 perft comparison and the movegen-only
comparison.

### 7.2 Harness source

```rust
//! Minimal comparison harness: atomic-movegen vs shakmaty (Atomic).
//! Usage: atomic-bench <tests/perft_values.md> [max_check_depth] [reps]

use std::hint::black_box;
use std::time::{Duration, Instant};

use atomic_movegen::movegen::generate_legal;
use atomic_movegen::{Board as Ab, MoveList as Aml};
use shakmaty::fen::Fen;
use shakmaty::variant::Atomic;
use shakmaty::{CastlingMode, FromSetup, Position, Setup};

fn positions(path: &str) -> Vec<(String, Vec<u64>)> {
    let text = std::fs::read_to_string(path).expect("perft file");
    let mut out = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cols.len() < 8 || cols[0] == "#" || cols[0].starts_with("---") {
            continue;
        }
        out.push((
            cols[7].trim_matches('`').to_string(),
            cols[1..7].iter().map(|c| c.parse().unwrap()).collect(),
        ));
    }
    out
}

fn shak(fen: &str) -> Atomic {
    let setup = Setup::from(Fen::from_ascii(fen.as_bytes()).expect("fen"));
    Atomic::from_setup(setup, CastlingMode::Standard).expect("position")
}

fn ab_perft(fen: &str, d: u32) -> u64 {
    atomic_movegen::perft(&mut Ab::from_fen(fen).expect("fen"), d)
}

fn sk_perft(fen: &str, d: u32) -> u64 {
    shakmaty::perft(&shak(fen), d)
}

fn best_of<F: FnMut() -> u64>(reps: u32, mut f: F) -> (Duration, u64) {
    let (mut best, mut out) = (Duration::MAX, 0);
    for _ in 0..reps {
        let t = Instant::now();
        out = black_box(f());
        best = best.min(t.elapsed());
    }
    (best, out)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let oracle = positions(&args[1]);
    let fens: Vec<String> = oracle.iter().map(|(f, _)| f.clone()).collect();
    let checks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
    let reps: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);

    // 1) correctness: both engines must reproduce the oracle perft values
    let mut bad = 0;
    for (fen, expected) in &oracle {
        for d in 1..=checks.min(expected.len() as u32) {
            let want = expected[d as usize - 1];
            for (name, got) in [("atomic", ab_perft(fen, d)), ("shakmaty", sk_perft(fen, d))] {
                if got != want {
                    println!("MISMATCH pos {fen} depth {d}: {name} {got} != {want}");
                    bad += 1;
                }
            }
        }
    }
    println!("correctness: {bad} mismatches over {} positions, depth 1..={checks}", fens.len());

    // 2) perft throughput over all positions
    for depth in [4, 5] {
        let (ta, na) = best_of(reps, || fens.iter().map(|f| ab_perft(f, depth)).sum());
        let (ts, ns) = best_of(reps, || fens.iter().map(|f| sk_perft(f, depth)).sum());
        assert_eq!(na, ns);
        println!(
            "perft d{depth}: n={na} atomic {ta:?} ({:.1} Mn/s) shakmaty {ts:?} ({:.1} Mn/s) \
             atomic is {:.2}x faster",
            na as f64 / ta.as_secs_f64() / 1e6,
            ns as f64 / ts.as_secs_f64() / 1e6,
            ts.as_secs_f64() / ta.as_secs_f64()
        );
    }

    // 3) move generation alone, on positions one ply from every benchmark FEN
    //    plus the start position's depth-2 tree
    fn walk(b: &mut Ab, d: u32, out: &mut Vec<String>) {
        if d == 0 {
            out.push(b.fen());
            return;
        }
        let (mut list, mut st) = (Aml::new(), atomic_movegen::board::StateInfo::new());
        generate_legal(b, &mut list);
        for &m in list.as_slice() {
            b.do_move(m, &mut st);
            walk(b, d - 1, out);
            b.undo_move(m, &st);
        }
    }
    let mut leaves: Vec<String> = Vec::new();
    for fen in &fens {
        walk(&mut Ab::from_fen(fen).unwrap(), 1, &mut leaves);
    }
    walk(&mut Ab::from_fen(&fens[0]).unwrap(), 2, &mut leaves);

    let ab: Vec<Ab> = leaves.iter().map(|f| Ab::from_fen(f).unwrap()).collect();
    let sk: Vec<Atomic> = leaves.iter().map(|f| shak(f)).collect();
    let iters = 20u32;
    let (ta, na) = best_of(reps, || {
        let mut sum = 0;
        let mut list = Aml::new();
        for _ in 0..iters {
            for b in &ab {
                list.clear();
                generate_legal(black_box(b), &mut list);
                sum += black_box(list.as_slice()).len() as u64;
            }
        }
        sum
    });
    let (ts, ns) = best_of(reps, || {
        let mut sum = 0;
        for _ in 0..iters {
            for p in &sk {
                sum += black_box(p.legal_moves()).len() as u64;
            }
        }
        sum
    });
    assert_eq!(na, ns);
    let calls = (leaves.len() * iters as usize) as f64;
    println!(
        "movegen: {} positions x{iters}  atomic {:.0} ns/call  shakmaty {:.0} ns/call  \
         atomic is {:.2}x faster ({:.1} vs {:.1} ns per legal move, {:.1} moves/call)",
        leaves.len(),
        ta.as_secs_f64() * 1e9 / calls,
        ts.as_secs_f64() * 1e9 / calls,
        ts.as_secs_f64() / ta.as_secs_f64(),
        ta.as_secs_f64() * 1e9 / na as f64,
        ts.as_secs_f64() * 1e9 / na as f64,
        na as f64 / calls
    );
}
```

### 7.3 Recorded output (this document)

```
$ cargo run --release -- ../atomic_movegen/tests/perft_values.md 6 5
correctness: 0 mismatches over 41 positions, depth 1..=6
perft d4: n=13495921 atomic 87.87554ms (153.6 Mn/s) shakmaty 206.656764ms (65.3 Mn/s) atomic is 2.35x faster
perft d5: n=399109774 atomic 2.602153061s (153.4 Mn/s) shakmaty 6.127200109s (65.1 Mn/s) atomic is 2.35x faster
movegen: 1170 positions x20  atomic 110 ns/call  shakmaty 350 ns/call  atomic is 3.19x faster (4.7 vs 15.1 ns per legal move, 23.2 moves/call)
```

The per-position table (§4.3), the cost split (§4.5) and the start-position
depth-6 figure (§4.2) came from two extra modes of the same harness (a
per-position loop and a generation-vs-generation+make loop, plus a `startpos`
FEN file fed to the per-position loop). They use the same timing helper and the
same position sets.

### 7.4 Cross-checks used in this document

The legal-move-set comparison (§3) is a few lines on top of the corpus above:

```rust
// for each FEN, compare sorted (from, to, promotion) keys of both move lists
let mut al = Aml::new();
generate_legal(&board, &mut al);
let mut a: Vec<_> = al.as_slice().iter().map(|&m| {
    let promo = match m.move_type() {
        atomic_movegen::types::MoveType::Promotion => match m.promotion_type() {
            atomic_movegen::PieceType::Knight => 1,
            atomic_movegen::PieceType::Bishop => 2,
            atomic_movegen::PieceType::Rook => 3,
            atomic_movegen::PieceType::Queen => 4,
            _ => 9,
        },
        _ => 0,
    };
    (m.from_sq() as u8, m.to_sq() as u8, promo)
}).collect();
let sm = pos.legal_moves();
let mut s: Vec<_> = sm.as_slice().iter().map(|&m| {
    let promo = match m {
        shakmaty::Move::Normal { promotion, .. } => promotion.map_or(0, |r| match r {
            shakmaty::Role::Knight => 1,
            shakmaty::Role::Bishop => 2,
            shakmaty::Role::Rook => 3,
            shakmaty::Role::Queen => 4,
            _ => 9,
        }),
        _ => 0,
    };
    (m.from().map_or(64, |sq| sq.to_u32() as u8), m.to().to_u32() as u8, promo)
}).collect();
a.sort_unstable();
s.sort_unstable();
assert_eq!(a, s, "legal move sets differ for {fen}");
```

The per-position table and the generation/make split come from two extra modes
of the same harness: (a) a loop that calls `best_of` per FEN instead of over the
whole list, and (b) a loop that additionally plays every legal move
(`do_move`+`undo_move` with a `black_box(&board)` barrier vs
`clone()`+`play_unchecked` with `black_box(&child)`), then subtracts the
generation-only time.

Useful cross-checks:

```sh
# atomic-movegen alone must reproduce the same numbers
cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 5
# -> 4864979
cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6
# -> 118926425
```

---

## 8. Summary

| Question | Result |
| --- | --- |
| Which generates legal moves faster per call? | `atomic-movegen`, ~3× (4.7 vs 15.1 ns per legal move) |
| Which runs perft faster? | `atomic-movegen`, ~2.35× at depths 3–5 across 41 positions |
| Largest / smallest observed advantage? | 2.83× (start position, depth 6) / 1.12× (tiny 5,890-node pawn ending) |
| Any position where `shakmaty` wins? | No (0/41) |
| Same rules / same results? | Yes: identical node counts at depths 1–6 for all 41 positions (12.46 G nodes; 12.05 G at depth 6 alone) and identical legal-move sets on 1,618 positions |
| Why? | `shakmaty` clones the position per pseudo-legal move to filter legality (its own code comments call the atomic movegen unoptimised); `atomic-movegen` uses incremental pin/checker state, magic bitboards and blast masks so most moves never need a cloned board |
