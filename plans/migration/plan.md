# Implementation Plan: Atomic Chess Move Generation in Rust

## Requirements

- Standard atomic chess only (COMMONER replaces KING, blast on capture, extinction pseudo-royal, no chess960 castling)
- Library crate (`atomic-movegen`) usable as a dependency, plus a CLI example for perft
- No non-essential Stockfish features: no NNUE, evaluation, search, UCI/XBoard, transposition tables, Zobrist hashing, or variant system
- Zero `unsafe` Rust; magic bitboards via safe const/static arrays (no pext). Any future unsafe must be documented with a comment
- Edition 2024, std only (no no_std)
- Reference implementation: `Fairy-Stockfish/src/` — C++ files to consult:
  - `types.h` — core types (Square, Bitboard, Move, Piece, Color)
  - `bitboard.h` / `bitboard.cpp` — bitboard constants and helpers
  - `position.h` / `position.cpp` — board state, do_move/undo_move, legality checking, FEN, blast logic
  - `movegen.h` / `movegen.cpp` — pseudo-legal and legal move generation
  - `variant.cpp` — atomic variant configuration (around line 524)

## Module Map

```
atomic-movegen/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs          # pub mod declarations, pub use re-exports
│   ├── types.rs        # Square, Bitboard, Move, Piece, Color, MoveType
│   ├── bitboard.rs     # bitboard utility functions (shift, between, etc.)
│   ├── attacks.rs      # magic bitboard attack tables
│   ├── board.rs        # Board + StateInfo + FEN + do/undo move + legality
│   └── movegen.rs      # pseudo-legal + legal move generation
└── examples/
    └── perft.rs        # CLI: perft <fen> <depth>
```

## Cargo.toml

```toml
[package]
name = "atomic-movegen"
version = "0.1.0"
edition = "2024"

[dependencies]

[[example]]
name = "perft"
```

## Implementation Phases

### Phase 0: Project Setup

- Restructure: `src/lib.rs` + modules, move existing `main.rs` content to `examples/perft.rs`
- Add `[[example]]` to `Cargo.toml`

### Phase 1: Core Types (`src/types.rs`)

Types translated from `types.h`:

- **Square** — `enum` over SQ_A1..SQ_H8 (0..63) + SQ_NONE sentinel
- **File** / **Rank** — enums with conversion helpers (`file_of(sq)`, `rank_of(sq)`, `make_square(file, rank)`, `relative_rank(color, rank)`)
- **Bitboard** — newtype over `u64`; implement `BitAnd`, `BitOr`, `BitXor`, `Not`, `Shl`, `Shr`, `Eq` so it behaves like a bitmask
- **Color** — `enum { White, Black }` with `flip()` and `us`/`usize` conversion
- **PieceType** — `enum { Pawn, Knight, Bishop, Rook, Queen, Commoner }` (COMMONER replaces KING in atomic)
- **Piece** — packed representation combining Color + PieceType, with `make_piece(color, pt)`, `color_of(p)`, `type_of(p)`
- **MoveType** — `enum { Normal, Promotion, EnPassant, Castling }`
- **Move** — 16-bit packed value:
  - bits 0–5: destination square (6 bits)
  - bits 6–11: origin square (6 bits)
  - bits 12–13: move type (2 bits)
  - bits 14–15: promotion piece type (2 bits, `PieceType as u8` when applicable)
  - Accessors: `from_sq()`, `to_sq()`, `move_type()`, `promotion_type()`
  - Constructors: `make_move(from, to)`, `make_promotion(from, to, pt)`, `make_enpassant(from, to)`, `make_castling(from, to)`

### Phase 2: Bitboard Utilities (`src/bitboard.rs`)

Translated from `bitboard.h`/`bitboard.cpp`:

- Constants: `FileABB`..`FileHBB`, `Rank1BB`..`Rank8BB`, `DarkSquares`, `AllSquares`
- `square_bb(sq) -> Bitboard` — single-square mask
- `file_bb(file)`, `rank_bb(rank)` — file/rank masks
- `shift::<Direction>(bb)` — directional shifts (North, South, East, West, plus combined N/E, N/W, S/E, S/W)
- `pawn_attacks_bb(color, pawns)` / `pawn_attacks_bb(color, sq)` — pawn attack generation
- `adjacent_files_bb(file)` — files left/right of given file
- `between_bb(sq1, sq2)` — squares strictly between two squares on a rank/file/diagonal (for pin detection)
- `line_bb(sq1, sq2)` — full line through two squares (rank, file, or diagonal)
- `aligned(sq1, sq2, sq3) -> bool` — three squares collinear
- Iteration helpers: `popcount(bb)` via `u64::count_ones`, `lsb(bb)` via `u64::trailing_zeros`, `pop_lsb(bb) -> (Square, Bitboard)`

### Phase 3: Attack Tables (`src/attacks.rs`)

Magic bitboard generation for sliding pieces. Reference: `bitboard.h` `struct Magic` and `init_pieces()` / `init()` in `bitboard.cpp`.

- Precompute magic multipliers and magic lookup tables for bishops and rooks (one table per square)
- Use the classic Stockfish magic bitboard approach: `(occupancy & mask) * magic >> shift` to index into a precomputed attack array
- Expose:
  - `bishop_attacks(sq, occupied) -> Bitboard`
  - `rook_attacks(sq, occupied) -> Bitboard`
  - `queen_attacks(sq, occupied) -> Bitboard` — bishop | rook
  - `king_attacks(sq) -> Bitboard` — precomputed, no occupancy dependency (also used for blast zone)
  - `knight_attacks(sq) -> Bitboard` — precomputed
  - `pawn_attacks(color, sq) -> Bitboard` — may live here or in bitboard.rs
  - `attacks_bb(pt, sq, occupied) -> Bitboard` — generic dispatch

Initialization strategy: prefer `const` arrays if the table generation can be expressed as const functions. If not feasible, use `std::sync::LazyLock` for runtime-once initialization. No `unsafe` — lookup tables are read-only.

### Phase 4: Board State (`src/board.rs`)

Translated from `position.h`/`position.cpp`. A simplified `Board` struct with only what atomic movegen needs.

**Fields:**

```rust
pub struct Board {
    squares: [Piece; 64],
    by_color: [Bitboard; 2],     // [white, black] occupancy
    by_type: [Bitboard; 6],      // Pawn..Commoner
    side_to_move: Color,
    castling_rights: u8,         // bitmask: WK=1, WQ=2, BK=4, BQ=8
    ep_square: Option<Square>,
    rule50: u8,
    game_ply: u16,
}
```

**Methods:**

- `Board::new() -> Self` — standard starting position
- `Board::from_fen(fen: &str) -> Result<Self, FenError>` — FEN input
- `fen(&self) -> String` — FEN output
- `piece_on(self, sq) -> Piece`, `empty(self, sq) -> bool`
- `us; pawn_attacks(color)`, `commoner_attacks(color)` — diagonal/sliding/straight attacks for given color (convenience for check/pin detection)
- `checkers() -> Bitboard` — pieces currently giving check to the side to move (sliding attacks + king adjacency check)
- `pinned(color) -> Bitboard` — pinned pieces for a given color

**Atomic rules (hardcoded, not configurable):**

- `blast_on_capture: true`
- `blast_immune: PAWN` only
- `king_type: COMMONER` (moves like a king but is pseudo-royal, not royal)
- `extinction_pseudo_royal: true` — COMMONERs cannot be adjacent; adjacency counts as check
- `extinction_value: -VALUE_MATE` — losing all COMMONERs = loss

**Legality checking** (`legal(m: Move) -> bool`, based on `Position::legal()` in `position.cpp`):

1. Determine the resulting occupancy after the move (move from square, remove captured piece)
2. If the move is a capture: simulate the blast — remove all non-pawn pieces (both colors) in the 3×3 king-move neighborhood around `to` (including `to` itself) from the occupancy
3. "Self-explosion is illegal": if the moving side's COMMONER is in the blast zone, the move is illegal
4. After the blast simulation, check that no COMMONER of the moving side is in check (i.e., attacked by an enemy piece)
5. Castling-specific: king cannot pass through or end on an attacked square

### Phase 5: Move Generation (`src/movegen.rs`)

Translated from `movegen.cpp` with all non-atomic features stripped (no gating, no drops, no walling, no Seirawan).

```rust
pub fn generate_pseudo_legal(board: &Board, moves: &mut Vec<Move>)
pub fn generate_legal(board: &Board, moves: &mut Vec<Move>)
```

**Pseudo-legal generation** — iterates over each piece of the side to move and adds its moves:

- **Pawns:** single push, double push (from rank 2/7), captures (including en passant), promotions (Q/R/B/N)
- **Knights:** all knight-attack squares that are empty or occupied by enemy
- **Bishops:** sliding attacks via magic bitboard, filtered to empty/enemy squares
- **Rooks:** same
- **Queens:** bishop + rook
- **COMMONERs:** all king-attack squares that are empty or occupied by enemy (no royal check restriction at this stage)
- **Castling:** king-side and queen-side if rights exist, path is clear, and king doesn't cross attacked squares

**Legal generation:** pseudo-legal → filter through `board.legal(m)`.

### Phase 6: Do/Undo Move (`src/board.rs` continued)

```rust
pub fn do_move(&mut self, m: Move, state: &mut StateInfo)
pub fn undo_move(&mut self, m: Move, state: &StateInfo)
```

**`do_move` steps:**

1. Save copies of mutable state into `StateInfo` (castling rights, ep square, rule50)
2. Update the board arrays (move piece from `from` to `to`, vacate `from`)
3. Handle the capture:
   - If `blast_on_capture`: compute blast zone = `king_attacks(to)` ∩ (all pieces except PAWN) ∪ {to}
   - Remove all blasted pieces from `squares`, `by_type`, `by_color`. Save them in `StateInfo` for undo
4. Handle en passant: remove the captured pawn
5. Handle castling: move the rook
6. Handle promotion: replace pawn with promoted piece
7. Update castling rights (lose rights if king/rook moves or rook is captured/blasted)
8. Update en passant square (if double pawn push)
9. Switch side to move
10. Increment game_ply

**`undo_move` steps:**

1. Restore castling rights, ep square, rule50 from `StateInfo`
2. Move the piece back from `to` to `from`
3. Restore any blasted/captured pieces to their squares
4. Reverse castling rook movement
5. Un-promote if applicable
6. Switch side to move
7. Decrement game_ply

**`StateInfo` struct:**

```rust
pub struct StateInfo {
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u8,
    captured_pieces: Vec<(Square, Piece)>,  // all pieces removed by blast/capture
}
```

### Phase 7: Perft (`src/perft.rs`, `examples/perft.rs`)

```rust
pub fn perft(board: &mut Board, depth: u32) -> u64
```

- Standard recursive perft: generate legal moves, `do_move`, recurse, `undo_move`, sum
- At depth 0: return 1
- No divide output, no transposition table, no Zobrist

**Example binary** (`examples/perft.rs`): accepts `<fen> <depth>` args, prints total node count.

**Testing strategy:**

- Fast smoke tests (`#[test]` in-crate): run the positions from `tests/perft.sh` and `tests/test_atomic_movegen.sh` at depth 1–3. These compile fast and catch most regressions.
- Full validation (`#[ignore]` tests or separate script): run the 12 README positions at depth 4. Deep enough to find bugs, shallow enough to complete in release mode quickly.
- Manual release validation: the full depth-5/6 README numbers — run before tagging a release.

### Phase 8: Testing

- **Unit tests** (`#[cfg(test)]` in each module):
  - `types.rs`: square/file/rank conversion invariants, move encoding round-trip, bitboard ops
  - `bitboard.rs`: shift constants, between_bb, line_bb, aligned, pop_lsb iteration
  - `attacks.rs`: spot-check known attack patterns for each piece type on specific squares
  - `board.rs`: FEN round-trip, starting position invariants, do_move/undo_move state restoration, legality of atomic edge cases
  - `movegen.rs`: move counts for known positions from `test_atomic_movegen.sh`
- **Integration tests** with perft numbers (see Phase 7)

### Phase 9: Polish

- Crate-level docs (`//!` in `lib.rs`) with usage example
- `README.md` with brief library description and perft example invocation
- CI (GitHub Actions) running `cargo test` and `cargo test --release` on push

## Known Edge Cases

| Scenario | Expected behavior |
|---|---|
| COMMONER moves into blast zone | Illegal — self-explosion |
| Pawn in blast zone | Survives — pawns are immune |
| Blast removes both own and enemy pieces | Verified by perft positions |
| Pinned piece captures and explodes enemy COMMONER | Legal — explosion removes the pinning piece |
| Castling through or into check | Disallowed |
| COMMONERs adjacent | Illegal — treated as check (extinction pseudo-royal) |
| En passant capture causes blast | EP pawn is removed, blast happens at capture square |
| Last piece exploded | That side loses (extinction) |
| Pawn promotes and blast occurs | Promote first, then blast removes surrounding pieces |
