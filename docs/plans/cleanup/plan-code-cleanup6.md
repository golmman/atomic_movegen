# Code Cleanup Plan 6

## Goal

A small, behaviour-neutral cleanup of the wishlist-3 evasion-mask code. No
public API, rule, key, generated answer, or hot-path shape is intended to
change.

## Items

### 1. Build the mask from the node snapshot parts

`LegalNode::from_view_with_checkers` already computes the side to move,
occupancy, and both commoner sets. `EvasionMask::from_view` recomputed the same
values. Replace the production path with a private `EvasionMask::from_parts`
that takes those values, and keep a `#[cfg(test)]` view constructor for the
unit tests. This preserves the single source of checker data while removing
duplicate bitboard lookups at node construction.

### 2. Keep the test seam explicit

The test-only constructor is documented as such, so production code cannot
accidentally keep two mask-building paths.

## Non-goals

- No change to the pass-set rule, sentinel, rejection test, or shared body.
- No new API, benchmark row, example behaviour, or package metadata change.
- No reflow of unrelated code or docs.

## Gates

`cargo test`, `cargo test --all-features`, `cargo test --release`, both clippy
configurations, `cargo fmt --check`, `cargo doc`, doctests, depth-6 perft, and
hot-path timing.
