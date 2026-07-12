//! Zobrist hash keys for incremental board hashing.
//!
//! The `ZOBRIST` static contains random keys for every piece on every square,
//! side to move, castling rights bitmask, and en-passant file. The keys are
//! generated at compile time by a simple splitmix64 PRNG so the table has no
//! runtime initialization cost.

/// All Zobrist keys used by [`Board`](crate::board::Board).
pub(crate) struct ZobristKeys {
    /// `piece[square][piece_raw]` for square 0-63 and piece encoding 0-15.
    /// Index 0 (empty square / `NO_PIECE`) is all zeros and is never XORed.
    pub piece: [[u64; 16]; 64],
    /// XORed into the hash when the side to move is `Black`.
    pub side: u64,
    /// `castling[rights]` keyed by the full castling-rights bitmask.
    pub castling: [u64; 16],
    /// `ep[file]` keyed by the en-passant file (index 8 = no ep, all zeros).
    pub ep: [u64; 9],
}

const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

#[allow(long_running_const_eval)]
const fn generate() -> ZobristKeys {
    let mut piece = [[0u64; 16]; 64];
    let mut castling = [0u64; 16];
    let mut ep = [0u64; 9];

    let mut seed = 0x135af329cfe0d4e7u64;

    let mut sq = 0usize;
    while sq < 64 {
        // Skip index 0: it represents NO_PIECE and contributes nothing.
        let mut p = 1usize;
        while p < 16 {
            piece[sq][p] = splitmix64(&mut seed);
            p += 1;
        }
        sq += 1;
    }

    let side = splitmix64(&mut seed);

    let mut c = 0usize;
    while c < 16 {
        castling[c] = splitmix64(&mut seed);
        c += 1;
    }

    let mut e = 0usize;
    while e < 9 {
        ep[e] = splitmix64(&mut seed);
        e += 1;
    }

    ZobristKeys {
        piece,
        side,
        castling,
        ep,
    }
}

/// Precomputed Zobrist keys for incremental board hashing.
pub(crate) static ZOBRIST: ZobristKeys = generate();
