# Plan 6 — Optional performance work and non-goals (P2-10)

## Scope

Address the optional P2-10 items without putting them ahead of correctness:

1. **Unchecked magic index** behind a new `unsafe-optimizations` feature.
2. **Combined child key** `(hash, rule50)` accessor.
3. Evaluate (but deliberately do not implement) lazy `populate_state`.

Also record the “not on the wishlist” items from feedback §5 as explicit
non-goals so they are not mistaken for missing work.

## Current-tree assessment

- `src/magic.rs` already uses safe indexing for `BISHOP_TABLE`/`ROOK_TABLE`.
  The analysis measured about 1.5 % on the consumer’s aarch64 host for unchecked
  indexing. The crate’s stated policy is safe Rust by default, so this must be
  opt-in and auditable.
- `Board::hash_after` and `Board::rule50_after` each look up the moving piece
  and classify the move independently. A combined accessor would save one piece
  lookup and one capture/pawn classification per child in the consumer’s
  non-mutating pre-pass.
- `has_legal_move_with_state` already requires `pinned` for
  `TrivialLegality::safe_origins`; lazily populating `checkers`/`pinned` is not a
  small change. The feedback explicitly says this is a measured observation, not
  a request. Do not change it in this plan.

## Decisions and pushback

1. **Feature name:** `unsafe-optimizations`, default off. It must not change
   public APIs or move-generation semantics. `cargo test` and all verifiers must
   pass with and without it.
2. **Only the table/entry index is unchecked.** Keep the square index bounded
   by the existing `Square` type/`SQUARE_NB` invariant and debug assertions.
   Document the invariant that makes the magic index in-bounds: the magic index
   is produced by `((occupied & mask) * magic) >> shift`, `shift = 64 -
   index_bits`, and the table builder allocates exactly `1 << index_bits`
   entries per square. The constant table is built with the same formula; a
   mismatch is caught by the exhaustive magic tests.
3. **The combined accessor is additive and named for what it returns:**
   `Board::hash_and_rule50_after(m) -> (u64, u16)`. It must produce exactly
   `(hash_after(m), rule50_after(m))`.
4. **Do not replace `hash_after`/`rule50_after`.** Keeping them separate avoids
   changing existing timings and lets the combined function share locals without
   forcing a hot-path refactor.
5. **Lazy `populate_state` is rejected for now.** Document the reason in the
   report; revisit only if a profiling pass shows `populate_state` dominating a
   consumer-relevant workload.

## Implementation

### 1. `unsafe-optimizations` feature

`Cargo.toml`:

```toml
[features]
default = []
unsafe-optimizations = []
```

`src/magic.rs`:

```rust
#[inline(always)]
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let e = &BISHOP_ENTRIES[sq as usize];
    let idx = ((occupied & e.mask).0.wrapping_mul(e.magic)) >> e.shift;
    #[cfg(feature = "unsafe-optimizations")]
    {
        // SAFETY: `idx < 1 << (64 - e.shift)` by the shift, and the table
        // builder allocates exactly that many entries at `e.offset`.
        // `sq < 64` is a `Square` invariant.
        unsafe {
            *BISHOP_TABLE
                .get_unchecked(e.offset as usize + idx as usize)
        }
    }
    #[cfg(not(feature = "unsafe-optimizations"))]
    {
        BISHOP_TABLE[e.offset as usize + idx as usize]
    }
}
```

Do the same for `rook_attacks`. Keep `queen_attacks` as the OR of the two.
Add a module-level safety note and a feature section in `README.md`:

> The `unsafe-optimizations` feature removes bounds checks from the magic
> table lookup. It is off by default. All indices remain provably in bounds
> under the magic-table construction invariants; enable it only if you audit
> that argument.

Do not add `unsafe` anywhere else in this plan. If clippy requires an allow,
scope it narrowly (`#[allow(unsafe_code)]` on the function) and explain why.

### 2. Combined child key

In `src/board.rs`:

```rust
/// `(hash_after(m), rule50_after(m))` with one shared move classification.
pub fn hash_and_rule50_after(&self, m: Move) -> (u64, u16) {
    // Fast path: compute the two results with one origin lookup where
    // practical. The simplest correct form is:
    (self.hash_after(m), self.rule50_after(m))
}
```

Implement the shared-local version only if it is straightforward:

- look up `piece` once;
- compute `is_pawn`, `is_capture`, `is_castling`, promotion once;
- pass those to the existing hash/rule50 logic via private helpers.

If that would require restructuring `hash_after` and risks a regression, keep
the simple two-call body, report the measured delta, and do not claim the
optimization. The contract is what matters:

```
hash_and_rule50_after(m) == (hash_after(m), rule50_after(m))
== after clone + do_move: (hash(), rule50())
```

Document the accessor as a consumer convenience for child-key prefetch, not a
replacement for either function. Add it to `CHANGELOG.md`.

### 3. Lazy `populate_state`

No code change. Add a note to the report:

- `TrivialLegality::new` needs `pinned` to build `safe_origins`;
- `has_legal_move_with_state` is already 5–15 ns on the consumer’s fixtures;
- deferring `checkers`/`pinned` would require a different trivial-legality
  design and is not justified by the feedback’s measurements.

## Tests

### Feature matrix

- `cargo test` (default)
- `cargo test --features unsafe-optimizations`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy --all-targets --features unsafe-optimizations -- -D warnings`
- `cargo fmt --check`
- `cargo doc` (default; optionally with the feature)
- `cargo run --release --example verify_perft` with and without the feature
  (at least depth 5; depth 6 if time permits)
- Existing exhaustive magic tests run in both modes; they are the primary
  correctness proof for the unchecked lookup.

### Combined key

- Add `hash_and_rule50_after_matches_separate`: reuse the existing
  `hash_after`/`rule50_after` random-playout harness; compare the tuple with
  both separate functions and with clone + `do_move`.

### Benchmark

Run the existing perft benchmark or the profiling guide’s harness with and
without `unsafe-optimizations`:

```sh
cargo run --release --example perft "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" 6
```

Report mean/min/max and CV. Do not keep the feature change if the measured gain
is within noise on the available host; the feature can stay as opt-in, but the
report must state whether it was actually beneficial there.

For the combined key, report a microbenchmark over a quiet middlegame if one is
available; expected gain is ~1–2 ns/child. If no stable measurement is possible,
say so rather than claiming a win.

## Files to touch

- `Cargo.toml` — feature declaration.
- `src/magic.rs` — cfg-gated unchecked lookups, safety docs.
- `src/board.rs` — `hash_and_rule50_after`, tests.
- `src/lib.rs` / `README.md` — feature documentation.
- `CHANGELOG.md` — feature/accessor under `Unreleased`.

## Non-goals (from feedback §5)

Do **not** add:

- `generate_captures`/quiescence generators;
- SAN parsing/formatting;
- Chess960 castling or stacking variants;
- `Arbitrary`/`serde`/`bincode`;
- board symmetry transforms or Miri hooks;
- hyperbola-quintessence fallback;
- a random perft file;
- a rules-model rewrite (packed moves, `&mut MoveList`, `StateInfo`, and
  incremental hash stay).

Also do not port an insufficient-material table; that is explicitly rejected by
P0-3 and plan 1.

## Final task

Write `docs/plans/feedback/f2_report6.md` with:

- feature build/test/verifier results;
- benchmark numbers and whether the feature is worth keeping;
- the combined-key equivalence result and measured/skipped benchmark;
- confirmation that lazy `populate_state` was intentionally deferred and why.
