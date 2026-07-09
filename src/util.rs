//! Shared utilities for table and index computations.

/// Compute the total number of entries needed for a flat attack table
/// indexed by the given bit counts (one per square).
///
/// For each square with `n` index bits, the table needs `2^n` entries.
/// The result is the sum of those values over all 64 squares.
pub(crate) const fn total_table_size(index_bits: &[u32; 64]) -> usize {
    let mut total = 0usize;
    let mut i = 0;
    while i < 64 {
        total += 1usize << index_bits[i];
        i += 1;
    }
    total
}
