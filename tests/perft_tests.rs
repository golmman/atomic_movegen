use atomic_movegen::board::Board;
use atomic_movegen::perft;

/// All 12 atomic perft positions from tests/perft_values.md
const POSITIONS: &[(&str, &[u64])] = &[
    // Position 1: starting position
    (
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        &[20, 400, 8902, 197326, 4864979, 118926425],
    ),
    // Position 2
    (
        "rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4",
        &[37, 1191, 43364, 1402237, 51225398, 1667574955],
    ),
    // Position 3
    (
        "rnb1kb1r/p5p1/2pNp2p/1p1q1p2/3P1Pn1/N5P1/PPP4P/R1BQKB1R b KQkq - 6 10",
        &[5, 156, 4848, 150519, 4643560, 146715064],
    ),
    // Position 4
    (
        "r4b1r/p1N1k1p1/2pNp2p/5p2/1n1P1P2/6PP/PP6/R1B1K3 b Q - 4 17",
        &[21, 546, 10566, 269557, 5470489, 139075307],
    ),
    // Position 5
    (
        "r1k4r/p4Np1/N1p1p2p/5p2/3P1P2/6PP/8/5K2 w - - 0 22",
        &[18, 233, 4307, 63774, 1178188, 19292503],
    ),
    // Position 6
    (
        "r1k1r3/p5p1/N1p4p/2N2p2/5P2/6PP/8/5K2 w - - 2 25",
        &[15, 260, 4114, 70412, 1123137, 20277102],
    ),
    // Position 7
    (
        "r1k5/p7/N1p5/2N4p/7P/8/5r2/4K3 w - - 0 31",
        &[11, 202, 2388, 41979, 510726, 9323466],
    ),
    // Position 8
    (
        "r2k4/p7/N1p5/7p/7P/8/2K5/8 w - - 2 34",
        &[12, 90, 1037, 10737, 120067, 1581592],
    ),
    // Position 9
    (
        "3r4/8/8/5k1p/4KN1P/p1p5/8/8 w - - 6 44",
        &[11, 227, 2472, 48708, 530284, 10261578],
    ),
    // Position 10
    (
        "5r2/8/4k1N1/4K2p/7P/p7/8/2q5 w - - 0 48",
        &[10, 386, 3513, 124504, 1106412, 38665634],
    ),
    // Position 11
    (
        "8/4N3/8/4K2p/1r3qkP/8/8/q7 w - - 16 59",
        &[3, 166, 1136, 60502, 448630, 22312112],
    ),
    // Position 12
    (
        "8/4N3/8/7p/1r3q1P/6K1/7k/q7 b - - 21 61",
        &[57, 463, 25637, 210798, 11357575, 96323713],
    ),
];

/// Test all 12 positions at depth 1 — all verified to match Fairy-Stockfish.
#[test]
fn perft_all_positions_depth_1() {
    for (i, (fen, expected)) in POSITIONS.iter().enumerate() {
        let mut board = Board::from_fen(fen).unwrap();
        let result = perft(&mut board, 1);
        assert_eq!(
            result,
            expected[0],
            "Position {} (depth 1): expected {}, got {}",
            i + 1,
            expected[0],
            result
        );
    }
}

/// Test starting position at depths 2 and 3 (verified to match).
#[test]
fn perft_starting_depth_2() {
    let mut board = Board::from_fen(POSITIONS[0].0).unwrap();
    assert_eq!(perft(&mut board, 2), POSITIONS[0].1[1]);
}

#[test]
fn perft_starting_depth_3() {
    let mut board = Board::from_fen(POSITIONS[0].0).unwrap();
    assert_eq!(perft(&mut board, 3), POSITIONS[0].1[2]);
}

/// Debug depth-4 tests for every position, to identify which ones fail.
#[test]
fn perft_all_positions_depth_4() {
    let mut any_fail = false;
    for (i, (fen, expected)) in POSITIONS.iter().enumerate() {
        if expected.len() < 4 {
            continue;
        }
        let mut board = Board::from_fen(fen).unwrap();
        let result = perft(&mut board, 4);
        if result != expected[3] {
            eprintln!(
                "FAIL Position {} (depth 4): expected {}, got {} (diff {})",
                i + 1,
                expected[3],
                result,
                result as i64 - expected[3] as i64
            );
            any_fail = true;
        } else {
            eprintln!(
                "OK   Position {} (depth 4): expected {}, got {}",
                i + 1,
                expected[3],
                result
            );
        }
    }
    assert!(!any_fail, "Some depth-4 tests failed");
}
