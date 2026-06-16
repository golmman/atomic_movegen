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
