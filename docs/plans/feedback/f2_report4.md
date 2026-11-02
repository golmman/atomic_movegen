# Report 4 — `Board::validate()` and strict FEN loading (P1-7)

## Status

| Item | Status | Notes |
| --- | --- | --- |
| `ValidationErrors` bitflags | Done | Manual zero-dependency type with documented constants, bit ops, `Display`, and `Error`. |
| `Board::validate()` | Done | Runs the strict policy and returns all accumulated flags. |
| `FenPolicy` | Done | `Strict`, `AllowTerminal`, `AllowImpossibleCheck`, `Permissive`; `#[non_exhaustive]`. |
| `Board::from_fen_with()` | Done | Additive loader returning `FenLoadError::{Syntax, InvalidPosition}`. |
| `from_fen` unchanged | Done | Still permissive; only its documentation gained a pointer to the new API. |
| Exports and docs | Done | `src/lib.rs`, `README.md`, `CHANGELOG.md`, and rustdoc for every new public item. |
| Verification | Done | Full tests, clippy, fmt, docs, depth-6 perft, and a 41-position corpus validation check all pass. |

## Validation flags

| Flag | Meaning |
| --- | --- |
| `EMPTY_BOARD` | The board has no pieces. Rejected by `Strict` and `AllowImpossibleCheck`; permitted by `AllowTerminal`. |
| `MISSING_COMMONER_WHITE` | White has zero commoners. |
| `MISSING_COMMONER_BLACK` | Black has zero commoners. |
| `TOO_MANY_COMMONERS_WHITE` | White has more than one commoner. |
| `TOO_MANY_COMMONERS_BLACK` | Black has more than one commoner. |
| `PAWNS_ON_BACKRANK` | At least one pawn is on rank 1 or rank 8. |
| `OPPOSITE_CHECK` | The side that just moved (`side_to_move.flip()`) is attacked. Uses the normal check detector, including touching-commoner immunity and terminal guards. |
| `INVALID_EP_SQUARE` | The stored ep target has the wrong rank, the enemy pawn behind it is absent, the double-push origin is occupied, or the target itself is occupied. |
| `INVALID_CASTLING_RIGHTS` | A set castling right lacks the required commoner on E or rook on A/H. |
| `TOO_MUCH_MATERIAL` | A side exceeds `pawns <= 8`, `total <= 16`, or, per non-pawn type, `count <= initial + (8 - pawns)`. Initial counts are N=2, B=2, R=2, Q=1, commoner=1. |

The material check is only the standard conservative “legal start plus
promotions” bound. It cannot prove reachability.

## Policy matrix

| Check | `Strict` | `AllowTerminal` | `AllowImpossibleCheck` | `Permissive` |
| --- | --- | --- | --- | --- |
| Empty board | reject | allow | reject | allow |
| Zero commoners per side | reject | allow | reject | allow |
| Multiple commoners | reject | reject | reject | allow |
| Pawns on back rank | check | check | check | skip |
| Castling consistency | check | check | check | skip |
| Material bound | check | check | check | skip |
| Ep geometry | check | check | check | skip |
| Opposite check | check | check | skip | skip |
| Legal ep capture required | no | no | no | no |

`from_fen_with(fen, Permissive)` is behaviorally identical to `from_fen` for
valid syntax and still returns `FenLoadError::Syntax` for malformed FEN text.

## Accepted and rejected fixtures

Accepted by `Strict`:

| FEN | Why |
| --- | --- |
| `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1` | Starting position. |
| `rnbqkbnr/ppp2pp1/4p2p/3p4/3PP3/7N/PPP2PPP/RNBQKB1R w KQkq - 0 4` | Capture-rich perft corpus position. |
| `r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1` | All castling rights with the required pieces. |
| `rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2` | FSF-style ep target with no legal ep capture. |
| `4k3/PPPPPPPP/8/8/8/8/8/4K3 b - - 0 1` | Promotion-heavy; the side to move may be in check. |

Rejected by `Strict` (each also still parses with `from_fen`):

| FEN | Expected flag(s) |
| --- | --- |
| `8/8/8/8/8/8/8/8 w - - 0 1` | `EMPTY_BOARD`, both missing-commoner flags |
| `4k3/8/8/8/8/8/8/8 w - - 0 1` | `MISSING_COMMONER_WHITE` |
| `8/8/8/8/8/8/8/4K3 w - - 0 1` | `MISSING_COMMONER_BLACK` |
| `4k3/8/8/8/8/8/8/3KK3 w - - 0 1` | `TOO_MANY_COMMONERS_WHITE` |
| `3kk3/8/8/8/8/8/8/4K3 w - - 0 1` | `TOO_MANY_COMMONERS_BLACK` |
| `4k2P/8/8/8/8/8/8/p3K3 w - - 0 1` | `PAWNS_ON_BACKRANK` |
| `4k3/4p3/8/4p3/8/8/8/4K3 w - e6 0 1` | `INVALID_EP_SQUARE` (double-push origin occupied) |
| `4k3/8/4n3/4p3/8/8/8/4K3 w - e6 0 1` | `INVALID_EP_SQUARE` (target occupied) |
| `4k3/QQQQQQQQ/QQ2QQQQ/8/8/qq2qqqq/qqqqqqqq/4K3 w - - 0 1` | `TOO_MUCH_MATERIAL` (and `OPPOSITE_CHECK`) |
| `4k3/4R3/8/8/8/8/8/4K3 w - - 0 1` | `OPPOSITE_CHECK` |

Policy-specific fixtures:

| FEN | Result |
| --- | --- |
| `8/8/8/8/8/8/8/K7 w - - 0 1` | `AllowTerminal` accepts (one commoner total). |
| `8/8/8/8/8/8/8/8 w - - 0 1` | `AllowTerminal` accepts (empty terminal fixture). |
| `4k3/8/8/8/8/8/8/3KK3 w - - 0 1` | `AllowTerminal` still rejects two white commoners. |
| `4k3/4R3/8/8/8/8/8/4K3 w - - 0 1` | `Strict` rejects, `AllowImpossibleCheck` accepts. |

## Perft-corpus false-positive check

A temporary example (`examples/validate_corpus.rs`, created, run, then removed)
extracted all 41 FENs from `tests/perft_values.md` and ran `Board::validate()`:

```text
checked 41 corpus FENs
```

No corpus position produced a validation error. In particular, the stricter
commoner count, material bound, and opposite-check rule did not reject any
position that the existing perft suite uses. Ep targets in the corpus occur
only in FENs such as the starting position and other real-game positions; none
were rejected merely because no legal ep capture exists.

## Verification

Commands and results:

```text
cargo test
  94 unit tests passed, 1 move-order golden passed, 4 perft tests passed,
  1 verify_moves test passed, 1 doctest passed

cargo clippy --all-targets -- -D warnings
  Finished with no warnings

cargo fmt --check
  Clean

cargo doc
  Generated documentation with no warnings

cargo run --release --example verify_perft
  Test cases: 41
  Result: 41/41 passed, 0/41 failed (total 85.226 s)
```

The `from_fen` path was not changed, so the move-order golden and perft numbers
are expected to be identical; they are.

## Problems encountered and decisions

1. **Opposite-check fixture direction.** The first test FEN placed the side to
   move in check (`.../4r3/4K3 w ...`). That is legal and must be accepted.
   `OPPOSITE_CHECK` applies to the side that just moved, so the test was
   corrected to put the black commoner under attack after White’s move
   (`4k3/4R3/8/8/8/8/8/4K3 w - - 0 1`).
2. **`INVALID_CASTLING_RIGHTS` is defensive.** `from_fen` already rejects a
   castling right without the required pieces, so this flag cannot currently be
   triggered through the public FEN loader. The check remains in
   `validation_errors` because `validate()` is a board-level semantic check and
   future constructors must not be able to bypass it.
3. **No legal-ep-capture requirement.** Following the plan and FSF behavior,
   validation only checks geometrical consistency with a double push. Requiring
   a legal ep capture would reject ordinary FSF FENs; `legal_ep_square()` /
   `EnPassantMode::Legal` remain the API for the stronger question.
4. **Material check ordering.** Commoner-count flags and `TOO_MUCH_MATERIAL` can
   both appear on artificial positions. This is intentional: checks accumulate
   rather than short-circuit, so diagnostics show every structural problem.

## Missing tests / remaining gaps

- `INVALID_CASTLING_RIGHTS` has no direct unit test because the permissive
  parser prevents constructing an inconsistent `Board` through public API.
  If an internal unchecked constructor is added, add a test for it.
- No property test over randomly mutated FENs was added; the fixed rejected
  fixtures cover every reachable flag. A future fuzz/binomial test could check
  that `from_fen_with(Strict)` never panics for arbitrary text.
- Reachability, legal-ep-capture existence, and retrograde analysis remain out
  of scope.

## Changed files

| File | Change |
| --- | --- |
| `src/board.rs` | `ValidationErrors`, `FenPolicy`, `FenLoadError`, `Board::validate`, `validation_errors`, `Board::from_fen_with`, `from_fen` doc note, and six new tests (including a proof-DB-style ingestion workflow). |
| `src/lib.rs` | Re-exports `FenLoadError`, `FenPolicy`, and `ValidationErrors`. |
| `README.md` | Feature bullet and untrusted-FEN usage example. |
| `CHANGELOG.md` | Unreleased Added entry. |
| `docs/plans/feedback/f2_report4.md` | This report. |
