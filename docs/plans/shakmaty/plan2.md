# Plan 2 — do not initialize the unused `MoveList` tail (feature-gated)

## Scope

One internal change and one documentation change:

- **`MoveList` storage:** behind the existing `unsafe-optimizations` feature,
  store `[MaybeUninit<Move>; MAX_MOVES]` and write/read only the initialized
  prefix `0..len`. The default build keeps `[Move; MAX_MOVES]` and is
  byte-for-byte the same logic as 2.7.0.
- **`docs/perf/perf_guide.md`:** record the A/B methodology that this session
  needed (ABBA interleaving; whole-run perft wall time cannot resolve <1 %).

This is the only `shakmaty`-inspired performance item that survived
measurement (`analysis2.md`): **≈4 % perft** with the feature enabled, no
change to the default build, no public API change, no move-order/hash/perft
change.

## Current state (2.7.0)

`src/types.rs`:

```rust
pub struct MoveList {
    moves: [Move; MAX_MOVES], // 256 × 2 bytes = 512 bytes
    len: usize,               // + 8 = 520 bytes
}

pub fn new() -> Self {
    MoveList { moves: [Move::NONE; MAX_MOVES], len: 0 }
}
```

Every `MoveList::new()` writes 520 bytes; a typical node writes ~25–35 moves
and reads `len`. `perft`, `generate_legal` callers and the examples create one
per node. `MAX_MOVES = 256`; `set_len` is used once (`generate_legal_in_view`).

Measured (ABBA, four-position probe, 278.7 M nodes): baseline 1,385–1,400 ms;
uninitialized tail 1,331–1,338 ms (≈0.96×). With the feature on (tail +
unchecked magic) 1,309–1,350 ms. See `analysis2.md` §3.

## Decisions

1. **Sound representation, not `assume_init()` on uninitialized integers.**
   The field is `[MaybeUninit<Move>; MAX_MOVES]`; `new` uses
   `[MaybeUninit::uninit(); MAX_MOVES]`; `push` uses `.write()`; reads go
   through `unsafe { assume_init }` only for slots `< len`. Creating
   `MaybeUninit::<[Move; MAX_MOVES]>::uninit().assume_init()` is *not* allowed
   (the intermediate value would be invalid), even though it happens to
   benchmark the same.
2. **Feature-gate, do not change the default build.** The default build keeps
   the existing zero-initialized array and the existing methods. `AGENTS.md`:
   avoid `unsafe` by default; allow it behind the documented
   `unsafe-optimizations` feature when measured. The conditional field means
   the two builds have the same size/layout (520 bytes) and the same public
   behaviour.
3. **No `MAX_MOVES`, `Move`, or API change.** `MoveList` stays 520 bytes, stays
   `Send + Sync`, stays `Clone`, `Default`, `IntoIterator`, `Index`.
4. **`Index`/`IndexMut` semantics are tightened and documented.** Today
   `list[i]` for `len <= i < MAX_MOVES` returns/aliases the zeroed tail
   (`Move::NONE`); with uninitialized storage that read would be UB. Both
   impls therefore assert `index < self.len` in **both** builds (the default
   build already panics for `i >= MAX_MOVES`; only the nonsensical
   `len <= i < MAX_MOVES` range changes). Document the precondition on the
   impls and mention it in `CHANGELOG.md`. No crate code indexes outside
   `len`.
5. **`Debug` prints the initialized prefix.** Replace the derive with a manual
   impl using `as_slice()` so the uninitialized tail is never read by
   `{:?}`. Output for the initialized part is unchanged; the old tail of
   `Move::NONE` entries disappears (debug-only, not a semver contract).
6. **`retain`/`set_len`/`as_slice`/`as_mut_slice` keep their contracts.**
   `set_len` gains a sentence: entries below the new `len` must already be
   initialized (only `generate_legal_in_view` uses it, and it only sets a
   value `<= orig_len`).

## Implementation

In `src/types.rs`:

```rust
#[derive(Clone)]
pub struct MoveList {
    #[cfg(feature = "unsafe-optimizations")]
    moves: [MaybeUninit<Move>; MAX_MOVES],
    #[cfg(not(feature = "unsafe-optimizations"))]
    moves: [Move; MAX_MOVES],
    len: usize,
}

impl fmt::Debug for MoveList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoveList")
            .field("moves", &self.as_slice())
            .field("len", &self.len)
            .finish()
    }
}
```

- `new`: `MaybeUninit::uninit()` array in the feature build, `Move::NONE`
  array otherwise.
- `push`: `self.moves[self.len].write(m)` in the feature build; keep
  `self.moves[self.len] = m` otherwise.
- `retain`: read with `assume_init` under `read < len`; write with `.write()`.
- `as_slice` / `as_mut_slice`: `slice::from_raw_parts[_mut](ptr.cast::<Move>(),
  len)` under the feature; existing slices otherwise.
- `Index` / `IndexMut`: `assert!(index < self.len)` in both builds, then the
  feature build dereferences `self.moves[index].as_ptr()` /
  `as_mut_ptr()` with a `SAFETY` comment; the default build keeps
  `&self.moves[index]`.
- Module-level doc note on `MoveList`: in the `unsafe-optimizations` build the
  tail is uninitialized; only `0..len` is ever read; the `unsafe` is confined
  to this module and justified by the `push`/`retain`/`set_len` invariants.

No other source file needs a change (`movegen.rs` already only touches
`0..len`; `perft`/examples use the public API).

Docs:

- `src/lib.rs` feature list: add the move-list tail to the
  `unsafe-optimizations` description.
- `README.md` and `CHANGELOG.md` (`[Unreleased]`): one line each.
- `docs/perf/perf_guide.md`: a short “Comparing two builds” section: alternate
  A/B in ABBA order, discard A-then-B-only runs (they biased this session by
  ~4 %), state that whole-run perft wall time cannot resolve <1 %, and point
  at `effect_bench` for in-process rows.

## Tests

Unit tests in `src/types.rs` (run in both feature states):

- `MoveList::new` is empty, `default() == new()` shape, size is 520 bytes
  (`size_of::<MoveList>()`), `Clone` keeps `len` and moves.
- `push`/`as_slice`/`as_mut_slice`/iterate/`retain`/`clear`/`set_len` round
  trips with 0, 1, 32 and `MAX_MOVES` moves.
- `Index`/`IndexMut` inside `len` work; at `len` they panic
  (`#[should_panic]`) in both builds.
- `format!("{list:?}")` contains the pushed moves and does not contain
  `MAX_MOVES` entries (debug prefix only).
- `push` past `MAX_MOVES` still debug-asserts / silently drops in release
  (existing behaviour).
- `assert_send_sync::<MoveList>()` already exists in `src/lib.rs`.

Integration: none new. The existing `move_order_golden`, `public_api`,
`perft_tests`, `verify_moves`, `conformance`, `effects`, `terminal_after`,
`child_state` suites already pin the observable behaviour.

## Verification

All global gates of the handoff-3 index still apply to any source change:

- `cargo test` and `cargo test --features unsafe-optimizations`
- `cargo clippy --all-targets -- -D warnings` and the same with
  `--features unsafe-optimizations`
- `cargo fmt --check`, `cargo doc` (no warnings), `cargo test --doc`
- `cargo test --test perft_tests --test move_order_golden --test verify_moves
  --test conformance --test effects --test terminal_after --test child_state
  --test public_api`
- `cargo run --release --example verify_perft 5` (41/41) and depth 6 when time
  permits; default **and** `--features unsafe-optimizations`
- `cargo run --release --example effect_bench 9 2000` (three runs, medians;
  no row may regress — `MoveList` is outside these rows, so this is a guard,
  not a target)

Perft A/B (the result of the plan), ABBA-interleaved, three rounds, same
machine and profile:

| Build | Median total | Gate |
| --- | ---: | --- |
| default (2.7.0-equivalent code) | baseline | must be within the 2.7.0 spread (no regression) |
| `--features unsafe-optimizations` | ≤ 0.97× default | the reported win; if > 0.99×, **do not ship** the change |

Report the per-position numbers too (the start position is the clear winner;
sparse endgames gain least). If Miri is available, run the new `src/types.rs`
unit tests with `--features unsafe-optimizations` under Miri in addition to
the normal suite; record whether it was available.

## Files to touch

- `src/types.rs` — `MoveList` storage/methods, manual `Debug`, unit tests.
- `src/lib.rs` — feature documentation only.
- `README.md`, `CHANGELOG.md` (`[Unreleased]`).
- `docs/perf/perf_guide.md` — A/B methodology note.
- `docs/plans/shakmaty/report2.md` — final report.

## Non-goals

- No bitboard-shift (unordered) pawn generation. It measured ≈4 % but changes
  the public move order; the order-preserving port measured ≈9 % slower.
  Record both numbers in the report; do not reopen without a major-release
  decision.
- No change to the magic lookup, `MAX_MOVES`, `Move`, `Board`, the generators,
  the ladder, `EffectContext`, or any handoff-3 plan.
- No `unsafe` outside `unsafe-optimizations`; no new dependency; no new public
  API.

## Final task

Write `docs/plans/shakmaty/report2.md`: implemented code (with the `SAFETY`
arguments), the ABBA table for default vs feature-on (absolute and ratio, per
position and total), the unchanged-perft/hash/move-order evidence, Miri status,
changed files, problems (including the A-then-B measurement bias and how it
was detected), deviations with their measurements, unresolved parts (whether a
future unordered generator is worth a major release) and next steps.
