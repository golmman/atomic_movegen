# atomic-movegen

A Rust library for legal move generation in [atomic chess](https://en.wikipedia.org/wiki/Atomic_chess).

## Features

- Legal move generation (pseudo-legal + legality filtering)
- FEN parsing and output
- Perft (performance test) recursion
- Blast-on-capture, COMMONER pseudo-royalty, pawn blast immunity
- Zero `unsafe` Rust

## Usage (library)

```rust
use atomic_movegen::board::Board;
use atomic_movegen::perft;

let mut board = Board::new();
let nodes = perft(&mut board, 3);
assert_eq!(nodes, 8902);
```

## Usage (CLI perft)

```sh
cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 4
```

Output: `197326`

## Tested positions

| Position | Depth | Expected | Status |
|---|---|---|---|
| Starting position | 1 | 20 | ✓ |
| Starting position | 2 | 400 | ✓ |
| Starting position | 3 | 8902 | ✓ |
| Starting position | 4 | 197326 | ✓ |
| `r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1` | 2 | 1939 | ✓ |

## License

MIT
