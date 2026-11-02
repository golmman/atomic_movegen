# Plan 1 — Terminal rules, state-aware `outcome`, and material draws (P0-1/P0-2/P0-3)

## Scope

Address the three P0 items of `feedback2.md`:

- **P0-1**: make terminal precedence correct, including mate vs. the 50-move
  clock, and pin all cases in one regression table.
- **P0-2**: add `Board::outcome_with_state`, `Board::outcome_with`,
  `Board::is_material_draw`, and make `outcome()` reuse populated state.
- **P0-3**: deliberately do **not** port FSF/shakmaty insufficient-material
  draws. The only material shortcut is “only the two commoners remain”, which
  is a WDL draw. Document it and pin it with K+B/K+N/K+R counterexamples.

These three items are implemented together because they all depend on the same
precedence function. Splitting them would create two rewrites of `outcome`.

## Current-tree assessment

The feedback was written against released 2.3.1, but the working tree already
contains unreleased fixes from `docs/plans/shakmaty/plan1.md`. A direct probe of
the current tree gives:

| FEN | Current tree | Required |
| --- | --- | --- |
| `8/8/8/8/8/8/4k3/4K3 w - - 0 1` (touching commoners) | 4 moves, `checkers=0` | already correct |
| `7K/8/8/8/8/8/1Q6/k7 b - - 0 1` (mate) | `Loss`, 0 moves | already correct |
| `7K/8/8/8/8/8/1Q6/k7 b - - 100 1` (mate on clock) | `Draw` | **`Loss`** |
| `8/8/8/8/8/8/8/K7 w - - 0 1` (opponent extinct) | `Win`, 0 moves | already correct |
| `4k3/8/8/8/8/8/8/4K3 w - - 0 1` (K vs K) | `None` | **`Draw`** |
| `8/8/8/8/8/8/8/8 w - - 0 1` (empty) | `Loss` | accepted (`Loss`) |

So P0-1 is reduced to (a) moving the 50-move branch behind the mate/move
existence question, (b) adding the commoner-only WDL draw, and (c) writing the
full precedence table so none of these can drift again. The released-2.3.1
touching-commoner bug is already fixed in the tree and must stay covered.

## Decisions and pushback

1. **Precedence** (documented and tested as one ordered list):
   1. side-to-move commoner extinct → `Loss`;
   2. opponent commoner extinct → `Win`;
   3. only the two commoners on the board → `Draw` (WDL shortcut);
   4. no legal moves: in check → `Loss`, otherwise `Draw`;
   5. `rule50 >= 100` → `Draw`.

   Branch 4 must be evaluated before branch 5 when the side to move is in
   check. This is FSF’s
   `rule50 > 99 && (!checkers() || MoveList<LEGAL>(pos).size())` ordering:
   a mate on the clock’s last ply is a loss, not a draw.

2. **Own extinction wins the artificial both-extinct tie.** We check the side to
   move first, so a completely empty board is `Loss`. This is unreachable in
   play but appears in consumer regression fixtures.

3. **`outcome()` includes the commoner-only material draw; `is_terminal()` does
   not.** `outcome()` is the WDL view. `is_terminal()` remains the literal
   rules/game-over predicate: extinction, no legal moves, or the 50-move rule.
   A K-vs-K board with `rule50 < 100` is `outcome() == Some(Draw)` but
   `is_terminal() == false`. This is needed so a search can use `outcome()` as
   a leaf value without falsely claiming the game ended early. Document the
   distinction in both methods.

4. **`is_material_draw()` is conservative and is not “insufficient material”.**
   It is true only when both colors have exactly one commoner and
   `occupied().count() == 2`. There is no table of K+B/K+N/K+R/... endings.
   The feedback’s tablebase counterexample is correct: a lone bishop, knight,
   or rook can capture the enemy commoner immediately when supported by its
   own commoner and outside the blast.

5. **The outcome implementation stays allocation-free in terminal hot paths.**
   Extinction, material, and the non-check 50-move branch are pure board/state
   tests. The existence query is only made when it can change the answer:
   no legal moves must be known, or the side is in check on the 50-move branch.

## Implementation

### `src/board.rs`

Add a private classifier. Keep one precedence implementation only:

```rust
fn outcome_impl(
    &self,
    state: &StateInfo,
    moves_exist: Option<bool>,
) -> Option<Outcome> {
    if state.commoners_count == 0 {
        return Some(Outcome::Loss);
    }
    if state.them_commoners_count == 0 {
        return Some(Outcome::Win);
    }
    if self.is_material_draw() {
        return Some(Outcome::Draw);
    }

    // The clock is a draw without movegen when there is no check. When the
    // side to move is in check, mate outranks the clock, so resolve the
    // existence question first.
    if self.rule50 >= 100 && state.checkers.is_empty() {
        return Some(Outcome::Draw);
    }

    let moves_exist = moves_exist.unwrap_or_else(|| {
        crate::movegen::has_legal_move_with_state(self, state)
    });

    if !moves_exist {
        return Some(if state.checkers.is_empty() {
            Outcome::Draw
        } else {
            Outcome::Loss
        });
    }
    if self.rule50 >= 100 {
        return Some(Outcome::Draw);
    }
    None
}
```

Public API:

```rust
impl Board {
    /// Populates a temporary StateInfo, then classifies.
    pub fn outcome(&self) -> Option<Outcome>;

    /// `state` must be populated by `Board::populate_state`.
    /// May run the existence query when no move is known to exist.
    pub fn outcome_with_state(&self, state: &StateInfo) -> Option<Outcome>;

    /// Hot-path form. `moves_exist` must be
    /// `has_legal_move_with_state(self, state)`.
    pub fn outcome_with(&self, state: &StateInfo, moves_exist: bool)
        -> Option<Outcome>;

    /// True only when both colors have exactly one commoner and nothing else.
    pub fn is_material_draw(&self) -> bool;

    /// Literal game-over predicate; excludes the commoner-only WDL shortcut.
    pub fn is_terminal(&self) -> bool;
}
```

Implementation notes:

- `outcome()` creates one `StateInfo`, calls `populate_state`, then
  `outcome_with_state`. Do not call `generate_legal` and do not build a
  `MoveList`.
- `outcome_with_state` passes `None` for `moves_exist`; `outcome_with` passes
  `Some(moves_exist)`.
- `is_material_draw`:
  `self.occupied().count() == 2
   && self.commoners(Color::White).count() == 1
   && self.commoners(Color::Black).count() == 1`.
  The color checks prevent artificial “two commoners of one color” boards from
  being mistaken for this state.
- `is_terminal` (strict):

  ```rust
  let mut state = StateInfo::new();
  self.populate_state(&mut state);
  self.commoners(self.side_to_move).is_empty()
      || self.commoners(self.side_to_move.flip()).is_empty()
      || self.rule50 >= 100
      || !crate::movegen::has_legal_move_with_state(self, &state)
  ```

  (Any `rule50 >= 100` position is over by the 50-move rule, whether the result
  is a draw or a mate loss.) Do not call `outcome()` from `is_terminal()`.

### Documentation

Update rustdoc and public docs in:

- `src/board.rs` (`outcome`, `outcome_with_state`, `outcome_with`,
  `is_material_draw`, `is_terminal`) with the exact precedence list and the
  “WDL shortcut vs. literal game over” distinction.
- `src/lib.rs` rules summary: no insufficient-material draw; only commoners
  remaining is a WDL draw that runs into the 50-move rule.
- `README.md` feature list and any terminal/mating wording.
- `CHANGELOG.md` under `Unreleased`: the two P0-1 fixes, the new outcome API,
  and the explicit rejection of insufficiency tables.

Do not edit the historical `docs/plans/shakmaty/report1.md`. If
`docs/plans/shakmaty/analysis.md` is kept as live guidance, add a short note to
§3.5/§8 that its follow-up “port FSF insufficient material” is superseded by
`feedback2.md` P0-3.

## Tests

Add to `src/board.rs` tests:

1. `terminal_precedence_table` — one table containing **all** feedback FENs:

   | FEN | outcome | legal | checkers | `is_terminal` |
   | --- | --- | --- | --- | --- |
   | touching commoners `8/8/8/8/8/8/4k3/4K3 w - - 0 1` | `None` | 4 | empty | false |
   | checkmate `7K/8/8/8/8/8/1Q6/k7 b - - 0 1` | `Loss` | 0 | non-empty | true |
   | checkmate at rule50 100 | `Loss` | 0 | non-empty | true |
   | opponent extinct `8/8/8/8/8/8/8/K7 w - - 0 1` | `Win` | 0 | empty | true |
   | K vs K `4k3/8/8/8/8/8/8/4K3 w - - 0 1` | `Draw` | 5 | empty | false |
   | K vs K rule50 100 | `Draw` | 5 | empty | true |
   | empty board `8/8/8/8/8/8/8/8 w - - 0 1` | `Loss` | 0 | empty | true |

   For each row assert `outcome`, `generate_legal().len()`, `checkers().count()`,
   `has_legal_move`, and `is_terminal`.

2. `outcome_state_aware_equivalence` — deterministic fixtures plus seeded random
   playouts (reuse the existing xorshift helper pattern): after populating
   `state`, `outcome()`, `outcome_with_state(&state)`, and
   `outcome_with(&state, has_legal_move_with_state(...))` must agree.

3. `outcome_with_known_move` — for a non-terminal position, pass
   `moves_exist = true` and `false`; `false` must be reported as
   draw/loss according to `checkers`, even though the caller contract says the
   value is authoritative.

4. `material_draw_is_commoner_only`:
   - `is_material_draw` is true for K vs K, including touching commoners.
   - It is false for each immediate-win counterexample:
     - `8/8/8/8/8/3B4/4k3/K7 w - - 0 1` (`d3e2`)
     - `8/8/8/8/3N4/8/4k3/K7 w - - 0 1` (`d4e2`)
     - `8/8/8/8/8/8/4k3/K3R3 w - - 0 1` (`e1e2`)
   - For each counterexample assert the listed capture is legal, the original
     position has `outcome() == None`, and after `do_move` the opponent has no
     commoner and is `Loss`. This is the explicit guard against a future
     “insufficient material” table.
   - Assert there is no public predicate that classifies K+B/K+N/K+R vs K as a
     draw.

5. Keep/update the existing `test_outcome_*` tests and remove duplicated
   assertions if they are folded into the table.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo doc`
- `cargo run --release --example verify_perft` (41/41, depths 1–6)
- Re-run the feedback probe (temporary external crate or a small example) and
  confirm all six rows match.
- Quick timing check: `outcome()` on a non-terminal middlegame must no longer
  build a `MoveList`; compare against `has_legal_move` + `populate_state`.
  No formal threshold is required, but a regression of more than noise should
  be investigated before merging.

## Files to touch

- `src/board.rs` — precedence classifier, public outcome API, material shortcut,
  strict terminal predicate, docs, tests.
- `src/lib.rs` — rules/docs.
- `README.md` — feature/docs wording.
- `CHANGELOG.md` — unreleased entries.
- `docs/plans/shakmaty/analysis.md` — optional correction note for the
  superseded material-draw follow-up.

## Non-goals

- No insufficient-material table (K+B/K+N/K+R vs K etc.).
- No new `Outcome` variants or a `Known/Unknown` rewrite.
- No changes to move generation or perft.

## Final task

Write `docs/plans/feedback/f2_report1.md` with:

- item-by-item status;
- the final precedence table and probe output;
- changed files;
- any problems with stale `StateInfo` contracts or artificial FENs;
- verification commands/results;
- remaining gaps (e.g. if `is_terminal` still needs a `_with_state` form).
