# Implementation Plan: Atomic Chess Move Generation in Rust

## Critique of the Ideas

### 1. "Translate this move generator" — too vague

Fairy-Stockfish's movegen is ~530 lines of `movegen.cpp` that calls into `Position` (1650 lines), `Bitboard` (787 lines), `Variant` (100+ fields), etc. The movegen itself is a thin layer on top of attack tables, board state, and variant rules. Saying "translate the movegen" misses 80% of the work. We need to build tables, board state, and legality from scratch anyway.

**Recommendation:** Define a bottom-up plan with clear, testable intermediate deliverables. The library needs: types → bitboard → attack tables → board representation → FEN parsing → pseudo-legal generation → legality (with atomic blast logic) → perft.

### 2. Don't copy Stockfish's architecture verbatim

Stockfish is a UCI engine, so its `Position` class is a monolith with 50+ public methods for search, evaluation, NNUE, etc. We only need atomic chess, so we can:
- Replace the 100-field `Variant` struct with a handful of `const bool` flags or an enum
- Skip chess960 castling — not needed for the perft positions we test against
- Drop all NNUE, evaluation, search, threading, TT, UCI/XBoard — none of that belongs in a movegen library
- Keep magic bitboards for sliding piece attacks (the core performance technique)

### 3. "Don't use unsafe Rust" — needs a concrete boundary

Magic bitboards require large precomputed lookup tables. In C++ these are `const` arrays, which map to Rust `const`/`static` safely. However:
- `pext` (BMI2 bit-manipulation instructions) requires the `_pext_u64` intrinsic, which is `unsafe` in Rust and has no stable fallback
- Fairy-Stockfish prefers magic bitboards over pext anyway; `HasPext` defaults to `false` unless `USE_PEXT` is defined

**Recommendation:** Use plain magic bitboards (multiply + shift), no `pext`. Zero unsafe needed. If pext is wanted later, gate it behind a Cargo feature flag and document the unsafe.

### 4. Perft numbers from the README

Confirmed — they are in the Fairy-Stockfish `README.md` under "## Perft numbers for atomic". 12 positions with depth 5 and 6 results. These are the numbers to validate against.

The `tests/perft.sh` script additionally has quick depth-2/4 smoke tests for several positions — useful for fast iteration.

### 5. Edition 2024 in Cargo.toml

Edition 2024 was stabilized in Rust 1.85 (Feb 2025, ~4 months old). For a greenfield project the practical differences are minor (no async, no closure captures, no temporary scope edge cases in this code). You confirmed you're happy with 2024 — keeping it is fine as long as CI uses Rust 1.85+. No change needed.

### 6. Library vs binary

The current crate only has `src/main.rs`. To make it a library dependency, we need `src/lib.rs`. The perft CLI should be a separate binary target or example.

**Recommendation:** Keep the library in `src/lib.rs` (or `src/` module tree). Add a `[[example]]` in Cargo.toml for a perft CLI binary.

---

## Implementation Plan

### Phase 0: Project Setup

- Restructure to `src/` library (lib.rs + modules) with binary example in `examples/perft.rs`
- Add `[[example]]` to `Cargo.toml`

### Phase 1: Core Types (`src/types.rs`)

Translate (`types.h`):
- `Square` enum (SQ_A1..SQ_H8, SQ_NONE) — 0..63
- `File` enum (FILE_A..FILE_H) + rank/file helpers
- `Rank` enum (RANK_1..RANK_8)
- `Bitboard` — newtype over `u64` with bitwise ops (BitAnd, BitOr, Not, Shl, Shr, etc.)
- `Color` enum (WHITE, BLACK) with `flip()` and `us` conversion
- `PieceType` enum (PAWN, KNIGHT, BISHOP, ROOK, QUEEN, COMMONER) — note KING is not a separate type in atomic; COMMONER replaces it
- `Piece` struct/type — encoding of (Color, PieceType) as a byte
- `Move` — 16/32-bit encoding:
  - bits 0-5: destination square
  - bits 6-11: origin square
  - bits 12-13: promotion piece type
  - bits 14-15: move type (NORMAL, PROMOTION, EN_PASSANT, CASTLING)
  - Accessors: `from()`, `to()`, `move_type()`, `promotion_type()` + `make_move()`
- `MoveType` enum (NORMAL, PROMOTION, EN_PASSANT, CASTLING)

PieceSquare tables and Zobrist keys live here too.

### Phase 2: Bitboard Utilities (`src/bitboard.rs`)

Translate (`bitboard.h`, `bitboard.cpp`):
- Predefined bitboard constants (FileABB..FileHBB, Rank1BB..Rank8BB, DarkSquares, etc.)
- `square_bb(sq) -> Bitboard` — single-square bitboard
- `file_bb(file)`, `rank_bb(rank)` — file/rank masks
- `shift<Direction>(bb)` — north/south/east/west shifts
- `pawn_attacks_bb(color, pawns)`, `pawn_attacks_bb(color, square)`
- `adjacent_files_bb(file)`
- `between_bb(sq1, sq2)` — squares between two squares (for pin detection)
- `line_bb(sq1, sq2)` — full line through two squares
- `aligned(sq1, sq2, sq3)` — three squares collinear check
- Popcount and LSB iteration helpers:
  - `popcount(bb) -> u32` — use `u64::count_ones()`
  - `lsb(bb) -> Square` — use `u64::trailing_zeros()`
  - `pop_lsb(bb) -> (Square, Bitboard)` — extract + clear LSB

### Phase 3: Attack Tables (`src/attacks.rs`)

Magic bitboard generation for bishops and rooks (queen = bishop | rook):
- Magic square multipliers (random or precomputed from Stockfish)
- Bishop/rook attack masks for each square
- `bishop_attacks(sq, occupied) -> Bitboard`
- `rook_attacks(sq, occupied) -> Bitboard`
- `queen_attacks(sq, occupied) -> Bitboard`
- `king_attacks(sq) -> Bitboard` — precomputed, no occupancy dependency (for blast zone)
- `knight_attacks(sq) -> Bitboard`
- `pawn_attacks(color, sq) -> Bitboard`
- Generic `attacks_bb(piece_type, sq, occupied) -> Bitboard`

**No unsafe needed** — magic lookup tables stored as `const`/`static` arrays.

### Phase 4: Board State (`src/board.rs`)

Translate simplified `Position` (from `position.h`/`position.cpp`):
- `Board` struct with:
  - `squares: [Piece; 64]` — the board array
  - `by_color: [Bitboard; 2]` — white/black occupancy
  - `by_type: [Bitboard; 6]` — per piece type (PAWN, KNIGHT, BISHOP, ROOK, QUEEN, COMMONER)
  - `side_to_move: Color`
  - `castling_rights: u8` — bitmask (WK, WQ, BK, BQ)
  - `ep_square: Option<Square>`
  - `rule50: u8` — halfmove clock (optional for perft)
  - `game_ply: u16`
  - `pieces: Bitboard` — all pieces (convenience)
- Methods:
  - `new()` — standard starting position
  - `from_fen(fen: &str) -> Result<Board, FenError>` — parse FEN
  - `fen() -> String` — serialize to FEN
  - `piece_on(sq) -> Piece`
  - `empty(sq) -> bool`
  - `pawn_attacks(color) -> Bitboard`
  - `king_attacks(color) -> Bitboard`
  - `checkers() -> Bitboard` — pieces giving check to current side's COMMONERs
  - `pinned(color) -> Bitboard` — pinned pieces for current side
  - `legal(move) -> bool` — legality check incorporating atomic rules

Atomic-specific logic — hardcoded for standard atomic rules:
- `blast_on_capture: true` — always, that's the defining rule
- `blast_immune: PAWN` — always, pawns survive explosions
- `king_type: COMMONER` — replaces KING, same movement but no royal check rules
- `extinction_value: -VALUE_MATE` — loss of last COMMONER = loss of game
- `extinction_pseudo_royal: true` — COMMONERs cannot be adjacent (treated as check)

### Phase 5: Move Generation (`src/movegen.rs`)

Translate (`movegen.cpp`) but simplified — no variants, no gating, no drops, no walling:
- `generate_pseudo_legal<MoveType>(board, moves: &mut Vec<Move>)`
  - Generates all moves of the given type (NORMAL, CAPTURES, etc.)
  - Includes: pawn single/double pushes, pawn captures, en passant, promotions, knight moves, bishop moves, rook moves, queen moves, COMMONER moves, castling
- `generate_legal(board, moves: &mut Vec<Move>)`
  - Pseudo-legal → filter through `board.legal()`

Key atomic legality rules (from position.cpp `legal()`):
1. COMMONER (pseudo-royal) not left in check
2. If capture: simulate blast removal from occupancy before checking pins/checks
3. "Self-explosion is illegal" — move is illegal if your COMMONER is in the blast zone after the move
4. Adjacent COMMONERs cannot be adjacent (treated as check — `extinction_pseudo_royal`)

### Phase 6: Do/Undo Move (`src/board.rs` continued)

Translate `do_move` / `undo_move` (from position.cpp):
- `fn do_move(&mut self, m: Move, state: &mut StateInfo)` — caller passes a `StateInfo` out-param for zero-copy undo
  - Update board arrays
  - Handle captures:
    - If `blast_on_capture`: compute blast zone = all non-pawn pieces (both colors) in 3x3 area around `to` square + the `to` square itself
    - Remove all blasted pieces from the board
  - Update castling rights
  - Update en passant
  - Update checkers/pinners
  - No Zobrist hashing — not needed for perft
- `fn undo_move(&mut self, m: Move, state: &StateInfo)` — restore previous state

`StateInfo` struct:
- castling rights copy
- ep square copy
- rule50 copy
- captured piece(s) — including all blasted pieces
- checkers/pinners (optional — can be recomputed if memory is the concern)

### Phase 7: Perft (`src/perft.rs`, `examples/perft.rs`)

- `fn perft(board: &Board, depth: u32) -> u64` — standard recursive perft, returns total node count
- Binary example: `cargo run --release --example perft -- "<fen>" <depth>` prints the total
- No divide output, no Zobrist, no transposition table

**Testing strategy (performance-aware):**
- Fast smoke tests (in-crate `#[test]`): positions from `perft.sh` and `test_atomic_movegen.sh` at depth 1–3
- Full validation (separate script or `#[ignore]` tests): the 12 README positions at a reduced depth (e.g., depth 4) — deep enough to catch bugs, shallow enough that release mode finishes quickly
- The full depth-5/6 README numbers are the final correctness target; run manually before releases

### Phase 8: Testing

Cargo tests (`#[cfg(test)]` in each module):
- Unit tests for bitboard operations
- Unit tests for attack table lookups (spot-check known values)
- Unit tests for move generation (match `tests/test_atomic_movegen.sh`)
- Integration tests with perft numbers

### Phase 9: Polish

- `README.md` with usage examples
- Documentation on crate docs (`//!` doc comments)
- CI (GitHub Actions) with `cargo test`

---

## Module Map

```
atomic-movegen/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs          # pub mod declarations, re-exports
│   ├── types.rs        # Square, Bitboard, Move, Piece, Color, etc.
│   ├── bitboard.rs     # bitboard utility functions (shift, between, etc.)
│   ├── attacks.rs      # magic bitboard attack tables
│   ├── board.rs        # Board + StateInfo + FEN + do/undo move + legality
│   └── movegen.rs      # pseudo-legal + legal move generation
└── examples/
    └── perft.rs        # CLI tool: perft <fen> <depth>
```

## Dependencies (Cargo.toml)

Minimal:
```toml
[package]
name = "atomic-movegen"
version = "0.1.0"
edition = "2024"

[dependencies]  # none expected for core library

[dev-dependencies]  # only if needed for test helpers

[[example]]
name = "perft"
```

## Known Edge Cases to Test

| Scenario | Notes |
|---|---|
| King (COMMONER) moves into blast zone | Illegal (self-explosion) |
| Pawn immune to blast | Pawns survive explosion |
| Blast removes own and enemy pieces | Verified in perft positions |
| Pinned piece captures and explodes king | The pin is irrelevant if the capture explodes the king (queen pinned by bishop test case) |
| Castling through check | Disallowed |
| Kings adjacent = check | Extinction pseudo-royal rule |
| En passant with explosion | EP capture can cause blast |
| All pieces exploded = draw? | Last piece(s) mutually destroyed |
| Promotion with explosion | Pawn promotes, then blast removes surrounding pieces |

## Decisions (from discussion)

| Question | Decision |
|---|---|
| Support nocheckatomic? | **No** — only standard atomic with extinction pseudo-royal |
| Chess960 castling? | **No** — standard castling only |
| Zobrist hashing | **Skip** — not needed for perft |
| `do_move` signature | `fn do_move(&mut self, m: Move, state: &mut StateInfo)` — caller-managed out-param for zero-copy undo |
| Perft output | **Total only** — no divide output |
| Perft depth for CI tests | **Depth 1–3** for fast smoke tests; full depth 5/6 run manually before releases |
| no_std | **No plans** — std is fine |
