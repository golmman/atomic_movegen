# Iteration 1 Recap

## Prior iterations

### Iteration 0 — Release profile tuning

Applied `lto = "fat"` and `codegen-units = 1` in `Cargo.toml`. This is a
one-line change with no risk and high impact (10–25 % expected speedup).

## What we know

- `StateInfo` already has fields for `checkers`, `pinned`,
  `commoners_count`, `them_commoners_count` but they are never populated.
- `generate_legal()` creates a fresh `StateInfo::new()` and passes it to
  `legal()`, so every call to `legal()` recomputes checkers/pinners from
  scratch.
- `do_move()` only fills `castling_rights`, `ep_square`, `rule50`, and the
  capture array — no checkers/pinners.
- `perft()` creates a `StateInfo` for the do/undo cycle but does not
  populate checkers/pinners.
