# Plan 4 — `Board::validate()` and strict FEN loading (P1-7)

## Scope

Add an opt-in semantic validator and policy-based FEN loading without changing
`Board::from_fen`’s permissive behavior:

- `ValidationErrors` bitflags;
- `Board::validate() -> Result<(), ValidationErrors>`;
- `FenPolicy` and `Board::from_fen_with(fen, policy)`;
- a FEN-loading error type that preserves the existing `FenError` for syntax.

The permissive parser remains the default. Artificial/terminal fixtures keep
working exactly as they do today.

## Current-tree assessment

`from_fen` is syntax-plus-a-few-local-checks:

- it validates rank widths, side to move, castling letters, and whether the
  named castling pieces exist;
- it checks the ep square’s rank and that an enemy pawn sits behind it;
- it accepts missing commoners, multiple commoners per side, pawns on the back
  rank, impossible ep geometry, and a checked side-not-to-move.

That is valuable for tests. The consumer’s proof DB needs a way to reject such
positions on ingestion. The feedback’s P1-7 is therefore a new validation layer,
not a stricter parser.

## Decisions and pushback

1. **`from_fen` stays permissive and unchanged.** `from_fen_with` is additive.
2. **Strict validation does not reject an uncapturable ep square.** FSF sets an
   ep square after every double push, even when no legal ep capture exists
   (95 % of ep-set positions in the feedback’s sample). Rejecting those would
   make ordinary FSF FENs fail. `validate()` checks that the ep target is
   *geometrically consistent with a double push*, not that a legal capture
   exists. Use `legal_ep_square()`/`EnPassantMode::Legal` for the latter.
3. **`FenError` remains syntax-only.** Adding a `Validation` variant would be a
   breaking change for downstream exhaustive matches. `from_fen_with` returns a
   new `FenLoadError`:

   ```rust
   pub enum FenLoadError {
       Syntax(FenError),
       InvalidPosition(ValidationErrors),
   }
   ```

   It implements `Display`, `Error`, and `From<FenError>`. If a future major
   release is acceptable, this can be collapsed into `FenError`.
4. **Policies are explicit and conservative.** `Strict` requires exactly one
   commoner per side. `AllowTerminal` permits zero or one commoner per side for
   artificial terminal fixtures but still rejects multiple commoners.
   `AllowImpossibleCheck` skips only the side-not-to-move check.
   `Permissive` reproduces today’s `from_fen` behavior. The enum is
   `#[non_exhaustive]`.
5. **Validation is not a reachability proof.** The material check is the
   standard conservative “no more than a legal start plus promotions” bound; it
   cannot prove a position was reached legally. Document that.

## Implementation

### `ValidationErrors`

In `src/board.rs`, add a small manual bitflags type (zero dependencies):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValidationErrors(u32);

impl ValidationErrors {
    pub const EMPTY_BOARD: Self;
    pub const MISSING_COMMONER_WHITE: Self;
    pub const MISSING_COMMONER_BLACK: Self;
    pub const TOO_MANY_COMMONERS_WHITE: Self;
    pub const TOO_MANY_COMMONERS_BLACK: Self;
    pub const PAWNS_ON_BACKRANK: Self;
    pub const OPPOSITE_CHECK: Self;
    pub const INVALID_EP_SQUARE: Self;
    pub const INVALID_CASTLING_RIGHTS: Self;
    pub const TOO_MUCH_MATERIAL: Self;

    pub const fn empty() -> Self;
    pub const fn bits(self) -> u32;
    pub const fn is_empty(self) -> bool;
    pub const fn contains(self, other: Self) -> bool;
    pub const fn union(self, other: Self) -> Self;
    pub const fn intersection(self, other: Self) -> Self;
}

impl BitOr / BitAnd / BitOrAssign / BitAndAssign for ValidationErrors;
impl fmt::Display for ValidationErrors;  // comma-separated names
impl Error for ValidationErrors;
```

Keep all constants public and documented. `Display` is needed for the error
trait and consumer diagnostics. `EMPTY_BOARD` is set when `occupied()` is zero;
the missing-commoner bits may also be set.

### `Board::validate`

```rust
pub fn validate(&self) -> Result<(), ValidationErrors> {
    let errors = self.validation_errors(FenPolicy::Strict);
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn validation_errors(&self, policy: FenPolicy) -> ValidationErrors { ... }
```

Checks, in this order (accumulate, do not early-return):

1. **Board/commoners**
   - `occupied().count() == 0` → `EMPTY_BOARD` under `Strict`; `AllowTerminal`
     permits the empty terminal fixture.
   - Count commoners per color.
     - `Strict`: exactly 1 each; 0 → `MISSING_COMMONER_*`, >1 →
       `TOO_MANY_COMMONERS_*`.
     - `AllowTerminal`: 0 or 1 each; >1 → `TOO_MANY_COMMONERS_*`.
     - `Permissive`: skip all commoner checks.
2. **Pawns on back ranks**
   - `(pieces_pt(Pawn) & BB_RANK_1 | ... & BB_RANK_8)` non-empty →
     `PAWNS_ON_BACKRANK`.
3. **Castling rights**
   - Re-check the same home-square-piece conditions as `from_fen` for each set
     right. A right without the commoner and rook on E/H or E/A →
     `INVALID_CASTLING_RIGHTS`.
4. **Material bounds** (conservative)
   - Per side: `pawns <= 8`, `total pieces <= 16`.
   - Let `free = 8 - pawns` (saturating at 0). For each non-pawn type:
     `count <= initial_count + free`, where initial counts are
     `N=2, B=2, R=2, Q=1, Commoner=1`.
   - Any violation → `TOO_MUCH_MATERIAL`.
5. **En-passant consistency**
   If `ep_square` is `Some(sq)`:
   - rank must be R6 for White to move or R3 for Black to move;
   - the square behind `sq` (R5/R4 direction) must contain an enemy pawn;
   - the double-push origin one square further behind that pawn must be empty;
   - `sq` itself must be empty.
   - Any failure → `INVALID_EP_SQUARE`.
   - Do **not** require an adjacent capturer or legal ep capture.
6. **Opposite check** (skip when `policy == AllowImpossibleCheck` or
   `Permissive`)
   - Let `just_moved = self.side_to_move.flip()`.
   - If both sides have a commoner and `compute_checkers(just_moved)` is
     non-empty, the side that just moved is in check → `OPPOSITE_CHECK`.
   - This uses the same touching-commoner immunity and terminal guards as
     normal check detection.

`AllowTerminal` still runs checks 2–6. `AllowImpossibleCheck` still requires
exactly one commoner per side. `Permissive` returns no errors.

### `FenPolicy` and `from_fen_with`

In `src/board.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FenPolicy {
    /// All semantic checks; exactly one commoner per side.
    Strict,
    /// Permit zero or one commoner per side for terminal fixtures.
    AllowTerminal,
    /// Strict except for the side-not-to-move check.
    AllowImpossibleCheck,
    /// Syntax only, exactly like `from_fen`.
    Permissive,
}

impl Board {
    pub fn from_fen_with(
        fen: &str,
        policy: FenPolicy,
    ) -> Result<Self, FenLoadError>;
}
```

Implementation:

```rust
let board = Board::from_fen(fen).map_err(FenLoadError::Syntax)?;
let errors = board.validation_errors(policy);
if errors.is_empty() {
    Ok(board)
} else {
    Err(FenLoadError::InvalidPosition(errors))
}
```

`Permissive` must be byte-for-byte equivalent to `from_fen` (test it).

### Exports and docs

- `src/lib.rs`: `pub use board::{Board, EnPassantMode?, FenLoadError, FenPolicy,
  ValidationErrors};` (only the types added by this plan; `EnPassantMode` is
  plan 3).
- `Board::from_fen`: add a “use `from_fen_with` / `validate` for untrusted
  input” note; do not change behavior.
- `CHANGELOG.md`, `README.md`.
- Document each `ValidationErrors` constant and each `FenPolicy` variant.

## Tests

Add to `src/board.rs` tests:

1. `validate_accepts_legal_positions`:
   - starting position;
   - capture-rich middlegame FENs from the test corpus;
   - castling-rights position `r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1`;
   - a normal ep-set FEN with no legal ep capture (must pass — do not reject
     FSF-style ep squares);
   - promotion-heavy legal position.

2. `validate_rejects_structural_errors` (one FEN per flag):
   - empty board → `EMPTY_BOARD`/missing commoners;
   - missing white/black commoner → missing bits;
   - two white or two black commoners → too-many bits;
   - pawn on rank 8 and on rank 1 → `PAWNS_ON_BACKRANK`;
   - impossible ep origin occupied: `4k3/4p3/8/4p3/8/8/8/4K3 w - e6 0 1`;
   - impossible ep square occupied: `4k3/8/4n3/4p3/8/8/8/4K3 w - e6 0 1`;
   - too much material (e.g. ten queens plus commoner per side) →
     `TOO_MUCH_MATERIAL`;
   - side-not-to-move in check → `OPPOSITE_CHECK` (choose a legal-looking FEN
     where the side that just moved is attacked).

3. `validate_is_opt_in`:
   - every FEN used above still parses with `from_fen`;
   - `from_fen` does not return validation errors.

4. `fen_policy_behavior`:
   - `from_fen_with(Strict)` accepts legal positions and returns
     `FenLoadError::InvalidPosition` for missing commoners;
   - `AllowTerminal` accepts one- and zero-commoner terminal fixtures
     (`8/8/8/8/8/8/8/K7 w - - 0 1`, empty board) but still rejects multiple
     commoners;
   - `AllowImpossibleCheck` accepts the opposite-check FEN;
   - `Permissive` matches `from_fen` for every test FEN, including invalid
     ones;
   - a syntax error still returns `FenLoadError::Syntax(FenError::...)`.

5. `validation_errors_display_and_bits`: `contains`, union/intersection,
   `Display` lists all set flags, and `is_empty` is correct.

6. Re-run the existing perft/move-order gates; `from_fen` behavior must be
   unchanged.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft`
- Test at least one proof-DB-like workflow: parse a malformed FEN with
  `from_fen`, reject it with `from_fen_with(Strict)`, and still accept the
  artificial multi-commoner fixture with `from_fen`.

## Files to touch

- `src/board.rs` — `ValidationErrors`, `FenPolicy`, `FenLoadError`,
  `validate`, `validation_errors`, `from_fen_with`, tests.
- `src/lib.rs` — re-exports and docs.
- `CHANGELOG.md`, `README.md`.

## Non-goals

- No change to `from_fen` strictness.
- No full reachability proof or retrograde analysis.
- No rejection of uncapturable ep squares.
- No SAN/Position-error model copied wholesale from shakmaty.

## Final task

Write `docs/plans/feedback/f2_report4.md` with the validation flag table, policy
matrix, rejected/accepted FENs, any false-positive risks found in the perft
corpus, and verification results.
