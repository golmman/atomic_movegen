# Code Cleanup Plan 5

## Goal

A small, behaviour-neutral cleanup of the wishlist-2 additions after the
hardening session. No public API, no rule, no key, no generated answer, and no
hot-path shape is intended to change.

## Items

### 1. One source for the double-push ep square

Three places derived the same value with the same `from ± 8` match:

- `Board::do_move` (`src/board.rs`);
- `position::child_view` (`src/position.rs`);
- `Board::child_effect` (`src/effects.rs`).

Add a crate-private `position::double_push_ep(us, from) -> Square` and use it in
all three. The hardening helper `is_valid_ep_target` lives beside it so the
"record" and "validate" rules are visible together.

### 2. Keep the totality contract in the docs

The hardening change makes the shared legality body total. Update the body,
`legal_with_counts`, and `LegalNode::legal` doc comments to say that a
commoner-less side answers `false`, while the callers still own the
`Counts`/`StateInfo` guard.

## Non-goals

- No refactor of the `LegalData` trait (the board path keeps borrowing the
  view; a node construction there was a measured regression in plan 2).
- No new benchmark rows, no `Cargo.toml` metadata change, no example changes.
- No reflow of pre-existing long documentation lines.

## Gates

`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
`cargo doc`, release perft A/B (no regression above ~2 %), and the existing
differential suites.
