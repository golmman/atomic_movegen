# Hardening session — wishlist 2 follow-up

## Scope

The wishlist-2 plans (W6–W9) landed. This session treats the released-in-waiting
2.10.0 tree as hostile input rather than as the differential corpus:

- every newly public predicate must be total for arbitrary 16-bit `Move`
  encodings, including moves that are well-formed by the origin-piece check but
  impossible by geometry;
- a stale or hand-written `StateInfo`/`Counts` must not reach an out-of-range
  attack-table index;
- the same questions must be asked of the older paths that now share the new
  legality body (`Board::legal`, `Board::legal_ep_square`).

## Non-goals

- No new public API, no dependency, no `unsafe`.
- No behaviour change for pseudo-legal input, a populated `StateInfo`, or the
  existing oracle values.
- No attempt to close W7's or W8's absolute instruction targets; those remain
  measured open items from the wishlist-2 reports.

## Method

1. Audit the new code paths (`LegalData`/`legal_in`, `LegalityContext`,
   `ChildEffect`/`child_effect`, `null_for_advanced`).
2. Exhaustively sweep all `Move::from_bits(0..=u16::MAX)` over a curated board
   set that includes both colors, castling, a live ep target, terminal boards,
   and back-rank pawns (a permissive FEN can express them).
3. Fix any panic, add the smallest regression test at the public boundary, and
   re-run the sweep.
4. Re-run all release gates, including depth-6 perft and the unused-feature
   build.

## Gates

`cargo test`, `cargo test --features unsafe-optimizations`,
`cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`,
`cargo test --doc`, `cargo run --release --example verify_perft 6`,
perft timing (no regression above ~2 %), and the hot-path benchmark.
