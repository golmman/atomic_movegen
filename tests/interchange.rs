//! Interchange tests: SAN round-trips and piece-set construction over the
//! perft corpus.
//!
//! [`Board::san`] and [`Board::move_from_san`] must be exact inverses on every
//! legal move of a real tree, and [`Board::from_setup`] must reproduce a board
//! parsed from FEN bit for bit (placement, state, key, and legal move list in
//! order). Both properties are checked here over the 41-position oracle corpus
//! in `tests/perft_values.md`, walked to a bounded depth so that `cargo test`
//! stays fast; `examples/verify_san.rs` runs the same walk to deeper depths.

use atomic_movegen::movegen::{generate_legal, generate_legal_with_state};
use atomic_movegen::{Board, MoveList, Setup, StateInfo};

/// Read the FEN column of the perft table (the 41-position oracle corpus).
fn perft_corpus() -> Vec<String> {
    let content = std::fs::read_to_string("tests/perft_values.md")
        .expect("tests/perft_values.md must be readable");
    let mut fens = Vec::new();
    for line in content.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let Some(start) = line.find('`') else {
            continue;
        };
        let Some(len) = line[start + 1..].find('`') else {
            continue;
        };
        fens.push(line[start + 1..start + 1 + len].to_owned());
    }
    assert_eq!(fens.len(), 41, "expected 41 perft corpus positions");
    fens
}

/// Depth-first walk of a position tree, calling `visit` for every node.
///
/// Each node gets its own make/unmake [`StateInfo`]: the undo data is per node,
/// not per depth (exactly as in `perft`).
fn walk(board: &mut Board, depth: u32, visit: &mut impl FnMut(&Board)) {
    visit(board);
    if depth == 0 {
        return;
    }
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    let mut moves = MoveList::new();
    generate_legal_with_state(board, &state, &mut moves);
    let mut undo = StateInfo::new();
    for i in 0..moves.len() {
        let m = moves.as_slice()[i];
        board.do_move(m, &mut undo);
        walk(board, depth - 1, visit);
        board.undo_move(m, &undo);
    }
}

/// SAN coverage counters, used to keep the test non-vacuous.
#[derive(Default, Debug)]
struct Coverage {
    positions: u64,
    moves: u64,
    checks: u64,
    mates: u64,
    disambiguated: u64,
    castles: u64,
    promotions: u64,
    captures: u64,
}

impl Coverage {
    fn record(&mut self, san: &str) {
        self.moves += 1;
        if san.ends_with('#') {
            self.mates += 1;
        } else if san.ends_with('+') {
            self.checks += 1;
        }
        if san.starts_with("O-O") {
            self.castles += 1;
            return;
        }
        if san.contains('=') {
            self.promotions += 1;
        }
        if san.contains('x') {
            self.captures += 1;
        }
        // A piece move is `Nf3` (3) or `Nxf3` (4) unless it is disambiguated.
        let body = san.trim_end_matches(['+', '#']);
        if body.starts_with(['N', 'B', 'R', 'Q', 'K']) {
            let plain = if body.contains('x') { 4 } else { 3 };
            if body.len() > plain {
                self.disambiguated += 1;
            }
        }
    }
}

#[test]
fn san_roundtrips_the_perft_corpus() {
    let corpus = perft_corpus();
    let mut coverage = Coverage::default();

    // Depth 2 over every corpus position: every node is visited, so the
    // round-trip is exact for the whole tree up to that depth.
    for fen in &corpus {
        let mut board = Board::from_fen(fen).expect("corpus FEN must parse");
        walk(&mut board, 2, &mut |board| {
            coverage.positions += 1;
            let mut moves = MoveList::new();
            generate_legal(board, &mut moves);
            for &m in moves.as_slice() {
                let san = board
                    .san(m)
                    .unwrap_or_else(|| panic!("{} / {m}: legal move without SAN", board.fen()));
                assert_eq!(
                    board.move_from_san(&san),
                    Some(m),
                    "{} / {san} / {m}",
                    board.fen()
                );
                coverage.record(&san);
            }
        });
    }

    eprintln!("{coverage:?}");
    assert!(coverage.positions > 10_000, "{coverage:?}");
    assert!(coverage.moves > 100_000, "{coverage:?}");
    // Non-vacuity: the corpus exercises every SAN shape, not just quiet pawn
    // and piece moves.
    assert!(coverage.checks > 0, "{coverage:?}");
    assert!(coverage.mates > 0, "{coverage:?}");
    assert!(coverage.disambiguated > 0, "{coverage:?}");
    assert!(coverage.castles > 0, "{coverage:?}");
    assert!(coverage.promotions > 0, "{coverage:?}");
    assert!(coverage.captures > 0, "{coverage:?}");
}

/// The SAN suffix must be exactly what the child position says: `#` when the
/// opponent's `Outcome` is a loss (mate, stalemate while attacked, extinction),
/// `+` when the opponent's commoner is merely attacked, and nothing otherwise.
#[test]
fn san_suffix_matches_the_child_position() {
    let corpus = perft_corpus();
    let mut positions = 0u64;
    let mut checked = 0u64;
    let (mut hashes, mut checks, mut barren) = (0u64, 0u64, 0u64);

    for fen in &corpus {
        let mut board = Board::from_fen(fen).expect("corpus FEN must parse");
        walk(&mut board, 2, &mut |board| {
            positions += 1;
            let mut moves = MoveList::new();
            generate_legal(board, &mut moves);
            for &m in moves.as_slice() {
                checked += 1;
                let san = board.san(m).expect("legal move has SAN");

                let mut child = board.clone();
                let mut state = StateInfo::new();
                child.do_move(m, &mut state);
                let expected = if child.outcome() == Some(atomic_movegen::Outcome::Loss) {
                    Some('#')
                } else if !child.checkers().is_empty() {
                    Some('+')
                } else {
                    None
                };
                let actual = san.chars().last().filter(|c| *c == '#' || *c == '+');
                assert_eq!(actual, expected, "{} / {san} / {m}", board.fen());
                match expected {
                    Some('#') => hashes += 1,
                    Some('+') => checks += 1,
                    Some(_) => unreachable!("only + and # are possible"),
                    None => barren += 1,
                }
            }
        });
    }

    eprintln!(
        "checked {checked} moves in {positions} positions: {hashes} '#' / {checks} '+' / {barren} none"
    );
    assert!(positions > 10_000, "expected a real corpus");
    assert!(hashes > 0 && checks > 0 && barren > 0, "suffix coverage");
}

#[test]
fn setup_reproduces_the_perft_corpus() {
    let corpus = perft_corpus();
    let mut checked = 0u64;

    for fen in &corpus {
        let mut board = Board::from_fen(fen).expect("corpus FEN must parse");
        // The corpus roots plus two plies of children: enough to cover
        // castling rights, en-passant targets, and clock values from real play.
        walk(&mut board, 2, &mut |board| {
            checked += 1;
            let mut setup = Setup::new();
            for (square, piece) in board.pieces() {
                setup.place(square, piece);
            }
            setup
                .set_side_to_move(board.side_to_move())
                .set_castling_rights(board.castling_rights())
                .set_ep_square(board.ep_square())
                .set_rule50(board.rule50())
                .set_game_ply(board.game_ply());

            let rebuilt = Board::from_setup(&setup)
                .unwrap_or_else(|e| panic!("{}: setup rejected: {e}", board.fen()));
            assert_eq!(rebuilt.fen(), board.fen(), "{}", board.fen());
            assert_eq!(rebuilt.hash(), board.hash(), "{}", board.fen());
            assert_eq!(
                rebuilt.hash_with(atomic_movegen::EnPassantMode::Legal),
                board.hash_with(atomic_movegen::EnPassantMode::Legal),
                "{}",
                board.fen()
            );
            assert_eq!(rebuilt.pieces().count(), board.pieces().count());

            let mut expected = MoveList::new();
            let mut actual = MoveList::new();
            generate_legal(board, &mut expected);
            generate_legal(&rebuilt, &mut actual);
            assert_eq!(
                actual.as_slice(),
                expected.as_slice(),
                "{}: legal moves",
                board.fen()
            );
            assert_eq!(rebuilt.outcome(), board.outcome(), "{}", board.fen());
        });
    }

    assert!(checked > 10_000, "expected a real corpus, got {checked}");
}
