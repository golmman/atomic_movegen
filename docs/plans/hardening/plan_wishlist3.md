# Hardening session — wishlist 3 follow-up

## Scope

The wishlist-3 implementation added `Board::legality_context_with_state` and a
rejection-only evasion mask. This session treats that surface as hostile input:

- arbitrary 16-bit `Move` encodings, including well-formed but impossible
  geometry;
- arbitrary/stale `StateInfo::checkers` patterns, including empty, full,
  occupied-square, and unrelated-square masks;
- permissive boards with multiple commoners, missing commoners, castling rights
  without rooks, impossible en-passant targets, and back-rank pawns;
- the rejection-only contract: a stale mask may reject more, but must never
  accept a move the shared legality body rejects.

## Non-goals

- No new public API, dependency, `unsafe`, or behaviour change for current
  checker data.
- No re-litigation of W10's measured fall-through rows.
- No change to keys, perft, move order, FEN, terminality or the existing
  check-blind context.

## Method

1. Probe the state-aware context against the shared body over curated
   permissive/edge FENs and every `Move::from_bits(0..=u16::MAX)`.
2. Repeat with deliberately stale checker bitboards and assert the
   rejection-only implication.
3. Add permanent regression coverage at the public boundary.
4. Re-run all release gates after the cleanup session.

## Gates

`cargo test`, `cargo test --all-features`, `cargo test --release`, both clippy
configurations, `cargo fmt --check`, rustdoc with warnings denied, doctests,
all integration suites, depth-6 perft, and perft/hot-path timing.
