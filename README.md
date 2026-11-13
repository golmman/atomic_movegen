# atomic-movegen

A Rust library for legal move generation in the standard `atomic` variant,
validated against the [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish)
reference implementation.

## Features

- Legal move generation (pseudo-legal + legality filtering) with a
  per-node fast-legality ladder: quiet moves, captures, en-passant blasts,
  commoner relocations, explosions, and self-explosions are decided from
  node-constant bitboards, and only the remaining candidates reach
  `Board::legal`
- Single-move queries without building a move list: `Board::is_pseudo_legal()`,
  `Board::is_legal()`, and state-aware `Board::is_legal_with_state()`
- Stable 16-bit `Move` wire encoding (`Move::to_bits()` / `Move::from_bits()`),
  plus `Hash`/`Ord` for use as a map or set key
- UCI move parsing and formatting: syntax-only `Move::from_uci()`,
  board-aware `Board::move_from_uci()` (resolves castling and en passant),
  allocation-free `Move::write_uci()`, and `Display`
- FEN parsing and output (standard notation only — `K`/`k` label the commoner)
- SAN parsing and formatting: `Board::san()` / `Board::write_san()` emit
  `O-O`/`O-O-O`, standard disambiguation, `=Q`-style promotions, and a
  `#`/`+` suffix taken from the child outcome; `Board::move_from_san()` resolves
  SAN text to exactly one legal move (and rejects ambiguous text). The commoner
  uses the letter `K`, as in FEN
- Piece-set construction without a FEN string: `Setup` (+ `Board::from_setup()`
  and the validating `Board::from_setup_with()`) builds a board from
  `(Square, Piece)` pairs and state fields, and `Board::pieces()` iterates the
  placement in ascending square order without allocating
- Opt-in semantic validation for untrusted FENs: `Board::validate()`,
  `Board::from_fen_with()`, `FenPolicy`, and `ValidationErrors`. The default
  `Board::from_fen()` stays permissive and unchanged.
- Perft (performance test) recursion
- Blast-on-capture, commoner royalty (one commoner per side, no promotion to
  commoner), pawn blast immunity (except for the capturing pawn at the blast
  square)
- Canonical blast helpers: `Board::blast_zone()` for the 3×3 geometry and
  `Board::blast_casualties()` for the pieces a capture removes, split by color
  and including the en-passant victim
- Rule conveniences: `attacks::attacks_from()`,
  `Board::attackers_to_color()`, board-aware `Board::is_zeroing()` /
  `Board::is_double_push()`, and order-preserving `MoveList::retain()`
- Hot-path helpers: allocation-free `Board::null_for()` (the null counterpart
  of a position without a FEN round-trip), the public early-exit
  `movegen::for_each_pseudo_legal()` and `movegen::for_each_legal()` streams,
  typed `Board::has_castling_right()` with `CastlingSide`, and
  `attacks::between()` / `attacks::xray_*_attacks()` for engine-side legality
  shortcuts. `movegen::for_each_legal()` and `movegen::has_legal_move()` use the
  same fast-legality ladder as `generate_legal()`
- Move effects without a board write: `EffectContext` (built once per position
  with `Board::effect_context()`) classifies every pseudo-legal candidate into a
  `MoveEffect` — capture, blast casualties, checks or explodes the enemy
  commoner, keeps the mover's commoner, and legality. `EffectContext::is_legal`
  agrees with `Board::legal` for pseudo-legal input, and
  `EffectContext::outcome_after` / `Board::outcome_after()` agree with the
  child's `Board::outcome()` for legal input
- Incremental Zobrist hashing (`Board::hash()`) and FSF-compatible FEN output
- Canonical en-passant keys and FEN output: `Board::legal_ep_square()` and
  `EnPassantMode::Legal` via `hash_with()`, `fen_with()`, and
  `hash_after_with()`. The default `hash()`/`fen()`/`hash_after()` keep the
  Fairy-Stockfish `Always` policy, so existing keys and output are unchanged.
- Game-over detection (`Board::outcome()`, state-aware `outcome_with_state()` /
  `outcome_with()`, and the strict `Board::is_terminal()`)
- Correct terminal precedence: mate outranks the 50-move clock, and only the
  two commoners remaining is a WDL draw. There is deliberately no
  insufficient-material table (K+B/K+N/K+R vs K can be an immediate win), and
  touching commoners are not automatically drawn when other pieces remain.
- Pure safe Rust by default, zero dependencies. The opt-in
  `unsafe-optimizations` feature removes bounds checks from the magic
  bitboard lookup; see [Feature flags](#feature-flags).

## Usage (library)

Add `atomic-movegen` to your `Cargo.toml`:

```toml
[dependencies]
atomic-movegen = "2.4.0"
```

```rust
use atomic_movegen::Board;
use atomic_movegen::perft;

let mut board = Board::new();
let nodes = perft(&mut board, 3);
assert_eq!(nodes, 8902);
```

For untrusted FEN input, use the opt-in validation layer:

```rust
use atomic_movegen::{Board, FenLoadError, FenPolicy};

let board = Board::from_fen_with(
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    FenPolicy::Strict,
).unwrap();
assert!(board.validate().is_ok());

let bad = Board::from_fen_with("8/8/8/8/8/8/8/8 w - - 0 1", FenPolicy::Strict);
assert!(matches!(bad, Err(FenLoadError::InvalidPosition(_))));
```

For move-effect classification without a child board:

```rust
use atomic_movegen::{Board, Outcome, StateInfo};

let board = Board::from_fen("4R1k1/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
let ctx = board.effect_context();

let m = board.move_from_uci("e8g8").unwrap(); // Rook takes the enemy commoner
let effect = ctx.effect(&board, m);
assert!(effect.is_capture);
assert!(effect.explodes_enemy_commoner);
assert!(effect.leaves_own_commoner);
assert!(effect.is_legal);
assert_eq!(ctx.outcome_after(&board, m), Some(Outcome::Loss));
```

`MoveEffect` never calls `do_move`; castling is the only case that delegates to
`Board::legal` (its pass-through-square rule is a separate rule surface), and
`outcome_after` falls back to a cloned board only where the child's
“has a legal move?” answer decides checkmate or stalemate. The effect path is
validated against `Board::legal` and against a cloned `do_move` child for every
pseudo-legal move of the 41-position perft corpus and of seeded random
play-outs (`tests/effects.rs`).

For SAN interchange and piece-set construction:

```rust
use atomic_movegen::{Board, Setup};

let board = Board::from_fen(
    "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
).unwrap();
let short = board.move_from_uci("e1g1").unwrap();
assert_eq!(board.san(short).as_deref(), Some("O-O"));
assert_eq!(board.move_from_san("0-0"), Some(short)); // zero variant accepted

// Build the same position piece by piece.
let mut setup = Setup::new();
for (square, piece) in board.pieces() {
    setup.place(square, piece);
}
setup
    .set_side_to_move(board.side_to_move())
    .set_castling_rights(board.castling_rights());
let rebuilt = Board::from_setup(&setup).unwrap();
assert_eq!(rebuilt.fen(), board.fen());
assert_eq!(rebuilt.hash(), board.hash());
```

`Board::move_from_san()` returns `None` for ambiguous, malformed, or illegal
text — it never picks one of several candidates. It accepts `O-O`/`O-O-O` and
`0-0`/`0-0-0`, one trailing `+`/`#`, an optional capture `x`, promotions as
`=Q`/`Q` (either case; `K` is rejected because the commoner is not a promotion
target), and disambiguation by file, rank, both, or a full origin square.
`Board::san()` and `Board::write_san()` only format **legal** moves.

## Usage (CLI perft)

```sh
cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 4
```

Output: `197326`

## Verify against known perft values

The repository ships with 41 test positions and their expected node counts at
depths 1–6 in [`perft_values.md`](./tests/perft_values.md).  Use the `verify_perft`
example to run all of them:

```sh
# Quick check at depth 3 (~5 s)
cargo run --example verify_perft 3

# Full verification at depth 6 — use --release for acceptable runtime
cargo run --release --example verify_perft
```

The tool exits with status 0 if every position matches, or 1 on any mismatch.
Depth 6 traverses billions of nodes; the `--release` flag is essential for a
timely result at that depth.

## Other CLI tools

All example binaries ship with the repo:

```sh
cargo run --example perft_divide "FEN" DEPTH   # per-move perft breakdown
cargo run --example list_moves "FEN"          # list all legal moves
cargo run --example fen_after "FEN" MOVE      # FEN + moves after playing MOVE (e.g. e2e4)
cargo run --example debug_moves "FEN"         # pseudo-legal vs. legal counts, duplicates
cargo run --example pawn_debug "FEN"          # pawn inspections and manual move checks
cargo run --example test_d5g8                 # attack-table sanity check (hardcoded position)
cargo run --release --example effect_bench    # effect-path vs board-write microbenchmark
cargo run --example san "FEN"                 # legal moves as SAN + UCI (round-trip checked)
cargo run --release --example verify_san [DEPTH] [PATH]  # SAN/Setup round-trip over the perft corpus
```

`verify_san` walks every corpus position in `tests/perft_values.md` to `DEPTH`
(default 3) and checks, for every legal move, that SAN round-trips and that a
`Setup`-rebuilt board matches the original FEN, key, and legal move list. At
depth 3 that is ~507 k positions and ~14 M moves in ~6 s (release). It exits 0
if everything passes and 1 otherwise.

## Feature flags

- `unsafe-optimizations` (off by default): removes the bounds check from the
  magic-bitboard table lookup. All indices remain provably in bounds under
  the magic-table construction invariants; enable it only if you audit that
  argument. It does not change any public API or move-generation semantics.

The combined child-key accessor `Board::hash_and_rule50_after(m)` returns
`(hash_after(m), rule50_after(m))` while classifying the move once, which is
useful when prefetching child keys for every candidate move.

```sh
cargo build --features unsafe-optimizations
cargo test --features unsafe-optimizations
```

## Development

```sh
cargo test        # unit + integration tests (perft assertions, move verification)
cargo clippy      # lints
cargo fmt         # formatting
cargo doc         # API docs
```

Correctness is verified against the [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish)
reference implementation (bundled in this repo as a git submodule) via the
expected values in [`tests/perft_values.md`](./tests/perft_values.md) and the
move lists in [`tests/moves.md`](./tests/moves.md).

## License

MIT
