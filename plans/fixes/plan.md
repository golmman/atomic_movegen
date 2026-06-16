# Fix Plan

## Priority order

1. Bug fixes first (`between_bb`, `src/main.rs` deletion)
2. Documentation (crate docs, README)
3. Testing (integration tests, edge case unit tests)

---

## Item: `between_bb` returns wrong value for non-aligned squares

### Description

`between_bb` in `src/bitboard.rs` returns `square_bb(s2)` (i.e., a bitboard containing only the destination square) when the two squares are not on the same rank, file, or diagonal. It should return `Bitboard::EMPTY`. This is currently latent — `pinned()` in `board.rs` only calls it with aligned squares — but it is incorrect code and could cause subtle bugs if future code calls `between_bb` with non-aligned squares.

### Files to modify

- `src/bitboard.rs:106`

### Changes needed

Change line 106 from:

```rust
        return square_bb(s2);
```

to:

```rust
        return Bitboard::EMPTY;
```

### Verification

- `cargo test` — existing test `test_between_bb` passes (it tests aligned squares C1–F4, which are on the same diagonal and should still work).
- Optionally add a new test in the `#[cfg(test)]` block of `bitboard.rs`:

```rust
#[test]
fn test_between_bb_non_aligned() {
    let between = between_bb(Square::A1, Square::B3);
    assert_eq!(between, Bitboard::EMPTY);
}
```

---

## Item: Delete stale `src/main.rs`

### Description

`src/main.rs` contains only `fn main() { println!("Hello, world!"); }`. It is a leftover from `cargo init`. The crate has an `examples/perft.rs` binary and is primarily a library. This file is unused and should be deleted.

### Files to modify

- `src/main.rs` — delete entire file

### Changes needed

Remove `src/main.rs` from the filesystem.

### Verification

- `cargo build` succeeds (the lib and example still compile)
- `cargo test` still passes all 25 tests
- `cargo run --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 1` outputs `20`

---

## Item: Add crate-level documentation

### Description

`src/lib.rs` has no `//!` doc comment at the crate root. The crate should describe itself, mention atomic chess rules, and provide a usage example.

### Files to modify

- `src/lib.rs`

### Changes needed

Add a `//!` doc comment at the top of `src/lib.rs` (before the `pub mod` declarations):

```rust
//! `atomic-movegen` — atomic chess move generation in Rust.
//!
//! This crate implements legal move generation, FEN parsing, and perft for
//! [atomic chess](https://en.wikipedia.org/wiki/Atomic_chess).
//!
//! # Atomic chess rules implemented
//!
//! - **Blast on capture:** capturing (or en passant) destroys all non-pawn
//!   pieces in a 3×3 king-move blast zone centered on the capture square,
//!   including the capturer itself if it is not a pawn.
//! - **Pawns are blast-immune:** pawns are never removed by a blast.
//! - **COMMONER replaces KING:** pieces move like kings but are pseudo-royal.
//!   Losing all COMMONERs means loss. Adjacent COMMONERs (even own) are illegal
//!   (extinction pseudo-royal rule).
//! - **No check/mate in the usual sense:** the game ends when a side has no
//!   COMMONERs left.
//!
//! # Example
//!
//! ```rust
//! use atomic_movegen::board::Board;
//! use atomic_movegen::perft;
//!
//! let mut board = Board::new();
//! let nodes = perft(&mut board, 3);
//! assert_eq!(nodes, 8902);
//! ```
```

### Verification

- `cargo doc --no-deps` produces documentation without warnings
- `cargo test` — the doc-test in the example runs and passes

---

## Item: Expand `README.md`

### Description

`README.md` currently contains only `# atomic_movegen`. It needs a library description, usage example (perft CLI invocation), and brief notes on atomic chess and the crate.

### Files to modify

- `README.md`

### Changes needed

Replace the single-line content with:

```markdown
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
```

### Verification

- README renders correctly on GitHub/crates.io
- Code examples are accurate

---

## Item: Add integration tests

### Description

The plan mentions test scripts (`tests/perft.sh`, `tests/test_atomic_movegen.sh`) but none exist. Add Rust integration tests that verify perft numbers against Fairy-Stockfish reference values.

### Files to modify

- Create `tests/perft_tests.rs`

### Changes needed

```rust
use atomic_movegen::board::Board;
use atomic_movegen::perft;

const STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const POS2_FEN: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

#[test]
fn perft_starting_depth_1() {
    let mut board = Board::from_fen(STARTING_FEN).unwrap();
    assert_eq!(perft(&mut board, 1), 20);
}

#[test]
fn perft_starting_depth_2() {
    let mut board = Board::from_fen(STARTING_FEN).unwrap();
    assert_eq!(perft(&mut board, 2), 400);
}

#[test]
fn perft_starting_depth_3() {
    let mut board = Board::from_fen(STARTING_FEN).unwrap();
    assert_eq!(perft(&mut board, 3), 8902);
}

#[test]
fn perft_starting_depth_4() {
    let mut board = Board::from_fen(STARTING_FEN).unwrap();
    assert_eq!(perft(&mut board, 4), 197326);
}

#[test]
fn perft_pos2_depth_2() {
    let mut board = Board::from_fen(POS2_FEN).unwrap();
    assert_eq!(perft(&mut board, 2), 1939);
}
```

### Verification

- `cargo test --test perft_tests` runs the integration tests and all pass

---

## Item: Add edge case unit tests

### Description

The plan describes unit tests for do_move/undo_move state restoration, self-explosion, blast zone, and pinned piece captures. These are absent.

### Files to modify

- `src/board.rs` — add tests to the existing `#[cfg(test)] mod tests` block

### Changes needed

Add the following test functions to `src/board.rs` in the `#[cfg(test)] mod tests` block:

1. **`test_do_undo_restores_state`** — Perform a simple pawn move, record the FEN, undo, and verify FEN matches the original.

```rust
#[test]
fn test_do_undo_restores_state() {
    let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let mut board = Board::from_fen(fen).unwrap();
    let mut state = StateInfo::new();

    // e4
    let m = Move::make_move(Square::E2, Square::E4);
    board.do_move(m, &mut state);
    board.undo_move(m, &state);

    assert_eq!(board.fen(), fen);
}
```

2. **`test_do_undo_capture_restores`** — Perform a capture, undo, and verify state is restored.

```rust
#[test]
fn test_do_undo_capture_restores() {
    let fen = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1";
    let mut board = Board::from_fen(fen).unwrap();
    let mut state = StateInfo::new();

    // exd5 (white pawn captures black pawn on d5 — en passant not applicable here)
    // Actually use a simpler capture: white pawn on e4 takes black pawn on d5
    // Let's use a FEN with a direct capture available
    let fen2 = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
    let mut board2 = Board::from_fen(fen2).unwrap();
    let mut state2 = StateInfo::new();

    let m = Move::make_move(Square::E4, Square::D5);
    board2.do_move(m, &mut state2);
    board2.undo_move(m, &state2);

    assert_eq!(board2.fen(), fen2);
}
```

3. **`test_self_explosion_illegal`** — A position where a COMMONER would be in the blast zone of a capture, verifying `legal()` returns false.

```rust
#[test]
fn test_self_explosion_illegal() {
    // White commoner on e1, white rook on e2, black pawn on e3
    // Rook takes pawn on e3 — blast zone includes e1, destroying the commoner
    let fen = "4k3/8/8/8/8/4p3/4R3/4K3 w - - 0 1";
    let board = Board::from_fen(fen).unwrap();
    let mut moves = Vec::new();
    crate::movegen::generate_legal(&board, &mut moves);
    // The rook on e2 has captures to e3 (pawn), but it's self-explosion
    // since the commoner on e1 would be blasted. So no legal captures.
    for &m in &moves {
        assert!(
            m.from_sq() != Square::E2 || m.to_sq() != Square::E3,
            "rook capture on e3 should be illegal (self-explosion)"
        );
    }
}
```

4. **`test_blast_zone_removes_pieces`** — Verify that after a capture, all non-pawn pieces in the 3×3 blast zone around the capture square are removed, including the capturer if it is not a pawn.

```rust
#[test]
fn test_blast_zone_removes_pieces() {
    // White rook on e4, black knight on e5, black pawn on f5
    // Rook captures knight — blast zone around e5 (d4-f4, d5-f5, d6-f6)
    // removes: rook (non-pawn capturer), knight, but NOT the pawn on f5
    let fen = "4k3/8/8/4np2/4R3/8/8/4K3 w - - 0 1";
    let mut board = Board::from_fen(fen).unwrap();
    let mut state = StateInfo::new();
    let m = Move::make_move(Square::E4, Square::E5);
    board.do_move(m, &mut state);
    // The rook and knight should be gone; the black pawn on f5 should remain
    assert!(board.piece_on(Square::E4) == NO_PIECE, "rook at e4 should be gone");
    assert!(board.piece_on(Square::E5) == NO_PIECE, "knight at e5 should be gone");
    assert!(board.piece_on(Square::F5) == B_PAWN, "pawn at f5 should survive");
}
```

5. **`test_pinned_piece_capture_explodes_pinner`** — A pinned piece can legally capture if the explosion removes the pinning piece.

```rust
#[test]
fn test_pinned_piece_capture_explodes_pinner() {
    // Black rook on e8 (pinning), white bishop on e4, white pawn on e5,
    // white commoner on e2. The bishop on e4 is pinned by the rook on e8.
    // But bishop captures pawn on e5 — blast destroys the rook on e8,
    // so the pin is removed and the move is legal.
    let fen = "4k2r/8/8/4p3/4B3/8/4K3/8 w - - 0 1";
    let board = Board::from_fen(fen).unwrap();
    let mut moves = Vec::new();
    crate::movegen::generate_legal(&board, &mut moves);
    let has_bishop_e5 = moves.iter().any(|&m| {
        m.from_sq() == Square::E4 && m.to_sq() == Square::E5
    });
    assert!(has_bishop_e5, "bishop capture on e5 should be legal (blast removes pinning rook)");
}
```

6. **`test_en_passant_blast`** — En passant capture triggers blast at the capture square.

```rust
#[test]
fn test_en_passant_blast() {
    // White pawn on d5, black pawn on c5 (just double-pushed), black knight on d4
    // White plays dxc6 en passant — blast at c6
    let fen = "4k3/8/8/2Pp4/4n3/8/8/4K3 b - - 0 1";
    let mut board = Board::from_fen(fen).unwrap();
    let mut state = StateInfo::new();
    // First push black pawn from c7 to c5 to set up EP
    // Actually, let's set up the EP position directly
    let fen2 = "4k3/8/8/2Pp4/8/8/8/4K3 w KQkq d6 0 2";
    let mut board2 = Board::from_fen(fen2).unwrap();
    let mut state2 = StateInfo::new();
    let m = Move::make_enpassant(Square::C5, Square::D6);
    board2.do_move(m, &mut state2);
    // After EP capture + blast: pawns on c5 and d5 are gone,
    // commoners should remain (out of blast zone)
    assert!(board2.piece_on(Square::C5) == NO_PIECE);
    assert!(board2.piece_on(Square::D5) == NO_PIECE);
}
```

### Verification

- `cargo test` — all new unit tests pass
- `cargo test --test perft_tests` — existing perft tests still pass

---

## Summary of all files to create/modify/delete

| Action | File |
|---|---|
| Edit | `src/bitboard.rs:106` | `square_bb(s2)` → `Bitboard::EMPTY` |
| Delete | `src/main.rs` | Remove stale stub |
| Edit | `src/lib.rs` | Add `//!` crate-level doc comment |
| Edit | `README.md` | Full library description and examples |
| Create | `tests/perft_tests.rs` | Integration tests for perft reference values |
| Edit | `src/board.rs` | Add 6 edge case unit tests to `#[cfg(test)] mod tests` |
