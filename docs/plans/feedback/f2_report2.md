# Report 2 — Stable `Move` encoding, UCI parsing, and single-move queries

## Status

| Item | Status | Notes |
| --- | --- | --- |
| P1-4 stable `Move` wire encoding | Done | Added `to_bits`/`from_bits`; layout and `PROMOTION_PIECES` order are documented as semver-stable. |
| P1-4 UCI parse/format | Done | Added `Move::from_uci`, `Move::write_uci`, `Display`, and `Board::move_from_uci`. `to_uci` now allocates once and delegates to `write_uci`. |
| P1-4 `Hash`/`Ord` on `Move` | Done | Derived `Hash`, `PartialOrd`, and `Ord`; ordering follows the wire value. |
| P1-5 single-move predicates | Done | Added `Board::is_legal`, `Board::is_legal_with_state`, and `Board::is_pseudo_legal`; none builds or clones a `MoveList`. |
| Documentation | Done | Updated `Move`, `Board::legal`, new APIs, `src/lib.rs`, `README.md`, and `CHANGELOG.md`. |
| Verification | Done | Tests, clippy, fmt, docs, depth-6 perft, and an external lookup benchmark all pass. |

## Stable wire golden values

`Move::from_bits` treats `0` as the only sentinel. Every other equal-origin
code (63 of them) is rejected, and the remaining 65,472 codes decode. A few
hard-coded values pin the layout and promotion order:

| Move | Wire value (`u16`) |
| --- | ---: |
| `Move::NONE` | 0 |
| normal `e2e4` | 796 |
| castling `e1h1` (UCI `e1g1`) | 12,551 |
| castling `e1a1` (UCI `e1c1`) | 12,544 |
| castling `e8h8` (UCI `e8g8`) | 16,191 |
| castling `e8a8` (UCI `e8c8`) | 16,184 |
| en passant `c5d6` | 10,411 |
| promotion `a7a8q` | 7,224 |
| promotion `a7a8r` | 23,608 |
| promotion `a7a8b` | 39,992 |
| promotion `a7a8n` | 56,376 |

`from_bits` is structural only. It deliberately decodes non-canonical
promotion-type bits on non-promotion moves; `Board::is_pseudo_legal` rejects
those because the generator never emits them.

## Parser and predicate equivalence

* `move_bits_roundtrip_exhaustive`: all 65,536 `u16` values. `0` maps to
  `Move::NONE`, 63 non-zero equal-origin codes are `None`, and every remaining
  code round-trips exactly.
* `is_legal_classifies_all_wire_codes`: all 65,536 wire codes for five
  fixtures (start position, open castling position, en passant, promotion,
  terminal one-commoner board), 327,680 checks. `is_legal_with_state` agreed
  with membership in `generate_legal_with_state` for every code.
* `is_pseudo_legal_matches_generator` and `is_legal_matches_generated_list`:
  all 16 `TRIVIAL_LEGALITY_CORPUS` positions plus five seeded start positions,
  four seeds each, 80-ply walks. Every generator-emitted move was accepted by
  `is_pseudo_legal`; both `is_legal` forms matched generated-list membership.
* `move_from_uci_matches_full_list_lookup`: five seeded start positions, four
  seeds each, 100-ply walks. Every generated legal move's `to_uci()` parsed
  back to the identical `Move`, including castling, en passant, and promotions.
* `verify_perft`: 41/41 positions, depths 1–6, no move-generation changes.

## Measured UCI lookup cost

Temporary release benchmark (created, run, then removed), 300k–1M iterations
per operation after warm-up:

| Operation | Cost |
| --- | ---: |
| `Move::from_uci("g1f3")` syntax only | 28.4 ns |
| `Board::move_from_uci("g1f3")`, start position | 60.2 ns |
| Generate all legal moves + compare `to_uci()` strings, start position | 587.5 ns |
| `Board::move_from_uci("e1g1")`, Kiwipete castling | 30.4 ns |
| Generate all legal moves + compare strings, Kiwipete | 1,502.3 ns |

The board-aware lookup is roughly 10–50× cheaper than the old fallback and is
in the expected tens-of-nanoseconds range.

## Problems encountered and resolved

1. **Castling fallback is necessary.** The plan assumed `e1g1`/`e8g8` never
   needs a normal-move fallback. That is false when the origin holds a rook:
   with no black castling rights, a rook on `e8` can legally move to `g8`, and
   `to_uci()` is also `e8g8`. The random UCI round-trip test exposed this.
   `move_from_uci` now tries the internal castling encoding, then en passant,
   then the ordinary parsed move. The special candidate wins if legal; if not,
   the ordinary move is still checked.
2. **En passant needs board-aware resolution too.** `Move::from_uci` is
   syntax-only, so `c5d6` initially decoded as a normal move and failed the
   required en-passant lookup test. `move_from_uci` now maps a normal-looking
   move onto the current ep target to `Move::make_enpassant`. This is an
   explicit extension of the plan's sketch and is documented on the method.
3. **Non-canonical promotion bits.** `from_bits` decodes every structural
   pattern, including normal moves with non-zero promotion-type bits. Those are
   not generator output, so exact predicate equivalence required a crate-private
   canonical-bits check in `is_pseudo_legal`. `from_bits` itself remains
   structure-only as planned.
4. **Clippy/doc cleanup.** Removed an unreachable `PieceType` match arm exposed
   by the in-crate exhaustive enum, and fixed two redundant rustdoc links.

## Unsupported / intentionally ambiguous semantics

* `Move::from_uci` does not resolve castling or en passant; only
  `Board::move_from_uci` does. `e1g1` alone is a normal two-square move.
* `Move::write_uci` / `Display` print en passant like an ordinary capture and
  castling as the king destination. These texts are unambiguous only together
  with a board; that is inherent to UCI.
* `Board::is_pseudo_legal` is not a legality guarantee. It checks castling
  rights, home squares, and path emptiness, but not attacked pass-through
  squares, self-explosion, or check. Those remain in `legal` / `is_legal`.
* `Board::is_legal_with_state` trusts a caller-populated `StateInfo`; stale
  state can change the result, exactly as for the existing `legal` and
  `generate_legal_with_state`.
* `Board::legal` is still the low-level precondition-based predicate. The hot
  generator continues to call it directly and does not pay for the shape checks.

## Changed files

| File | Change |
| --- | --- |
| `src/types.rs` | `Move` derives, stable `to_bits`/`from_bits`, `from_uci`, `write_uci`, `Display`, canonical-bit helper, and tests. |
| `src/board.rs` | `is_pseudo_legal`, `is_legal`, `is_legal_with_state`, `move_from_uci`, castling/en-passant UCI resolution, `legal` docs, and tests. |
| `src/lib.rs` | Crate-level Move API section. |
| `README.md` | Feature list for stable encoding, UCI helpers, and single-move queries. |
| `CHANGELOG.md` | Unreleased Added entries and the stable-layout guarantee. |
| `docs/plans/feedback/f2_report2.md` | This report. |

## Remaining gaps / next steps

* No SAN parsing or formatting.
* No board-dependent `Move::is_zeroing` / `Move::is_double_push`; those belong
  on `Board` and are covered by later plans.
* The temporary benchmark example was removed; the measured numbers are
  recorded above.
* Plan 3 (en-passant normalization) and plan 6 (blast helpers) can use
  `is_pseudo_legal`, `is_legal`, and `move_from_uci` directly in their tests.
