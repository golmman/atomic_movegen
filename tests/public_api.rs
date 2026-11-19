//! The released public API surface, pinned by name.
//!
//! Every item the 2.9.0 release notes name must exist and resolve from the
//! crate root, so the notes cannot drift from the code. This test says nothing
//! about behaviour — the differential suites do that; it only fails when an
//! item promised in `CHANGELOG.md` / `README.md` is missing, renamed, or no
//! longer re-exported. The last test is the composite shape the `README.md`
//! advertises (summary + key + terminality from one derivation), so that
//! snippet cannot rot.

use atomic_movegen::board::FenError;
use atomic_movegen::movegen::{generate_legal_with_state, generate_pseudo_legal};
use atomic_movegen::{
    AfterState, Bitboard, BlastCasualties, Board, Board as BoardRoot, CONFORMANCE_CASES, Color,
    Counts, EffectContext, EnPassantMode, FenLoadError, FenPolicy, Move, MoveClassification,
    MoveEffect, MoveList, Outcome, PostMove, Square, StateInfo, Terminal, TerminalReason,
    conformance,
};

#[test]
fn crate_root_reexports_resolve() {
    // Types named in the 2.9.0 notes.
    let _: fn(&Board) -> Color = BoardRoot::side_to_move;
    let _: EffectContext = Board::new().effect_context();
    let _: Outcome = Outcome::Draw;
    let _: TerminalReason = TerminalReason::Stalemate;
    let _: Terminal = Terminal {
        outcome: Outcome::Loss,
        reason: TerminalReason::Extinction,
    };
    let _: AfterState = AfterState::Undecided;
    let _: BlastCasualties = BlastCasualties::default();
    let _: FenPolicy = FenPolicy::Lichess;
    let _: Bitboard = Board::blast_zone(Square::NONE);
    // The release adds additive traits to the two FEN error types.
    fn clone_eq<T: Clone + PartialEq + Eq + std::fmt::Debug>(_: &T) {}
    clone_eq(&FenError::TooShort { parts: 0 });
    clone_eq(&FenLoadError::InvalidPosition(
        atomic_movegen::ValidationErrors::EMPTY_BOARD,
    ));

    // The classification type and its two entry points.
    let board = Board::new();
    let ctx = board.effect_context();
    let m = Move::from_uci("e2e4").unwrap();
    let classification: MoveClassification = ctx.classification(&board, m);
    let effect: MoveEffect = ctx.effect(&board, m);
    assert_eq!(classification, effect.classification());

    // The child summary, its terminal state, and the keyed variant.
    let post: Option<PostMove> = ctx.post_move(&board, m);
    let post = post.expect("pseudo-legal move");
    let _: Option<Terminal> = post.terminal();
    let keyed: Option<(PostMove, u64)> = ctx.post_move_and_key(&board, m);
    let (keyed_post, key) = keyed.expect("pseudo-legal move");
    assert_eq!(keyed_post, post);
    assert_eq!(key, board.hash_after(m));

    // The mode-aware spelling of the same query.
    let mode_keyed: Option<(PostMove, u64)> =
        ctx.post_move_and_key_with(&board, m, EnPassantMode::Always);
    assert_eq!(mode_keyed, Some((post, key)));
    let legal_keyed: Option<(PostMove, u64)> =
        ctx.post_move_and_key_with(&board, m, EnPassantMode::Legal);
    assert_eq!(
        legal_keyed.map(|(_, key)| key),
        Some(board.hash_after_with(m, EnPassantMode::Legal))
    );

    // The one-ply child explosion query.
    let explosion: Option<Move> = ctx.exploding_capture_after(&board, m);
    assert_eq!(explosion, None);

    // The conformance corpora.
    assert!(!CONFORMANCE_CASES.is_empty());
    assert_eq!(CONFORMANCE_CASES.len(), conformance::CASES.len());
    assert!(!conformance::CHILD_CASES.is_empty());
}

#[test]
fn counts_only_state_resolves_and_answers() {
    let board = Board::new();
    let counts: Counts = board.counts();
    assert_eq!(counts.commoners(), 1);
    assert_eq!(counts.them_commoners(), 1);
    let _: Counts = Counts::new(1, 1);

    let mut full = StateInfo::new();
    board.populate_state(&mut full);
    assert_eq!(counts, Counts::from(&full));

    let mut cheap = StateInfo::new();
    board.populate_counts(&mut cheap);
    let e2e4 = Move::from_uci("e2e4").unwrap();
    assert_eq!(board.legal(e2e4, &cheap), board.legal(e2e4, &full));
    assert_eq!(
        board.is_legal_with_state(e2e4, &cheap),
        board.is_legal_with_state(e2e4, &full)
    );
}

#[test]
fn classification_is_usable_from_a_legal_list() {
    // The consuming shape the API exists for: a caller that already has the
    // legal list classifies without a second legality query.
    let board =
        Board::from_fen("r1bqkbnr/pppp2pp/2n5/4ppB1/1P1P4/7N/P1P1PPPP/RN1QKB1R b KQkq - 1 4")
            .unwrap();
    let mut state = StateInfo::new();
    board.populate_state(&mut state);
    let ctx = board.effect_context_with_state(&state);

    let mut legal = MoveList::new();
    generate_legal_with_state(&board, &state, &mut legal);
    assert!(!legal.is_empty());

    let mut captures = 0usize;
    for &m in legal.as_slice() {
        let c = ctx.classification(&board, m);
        assert_eq!(c, ctx.effect(&board, m).classification(), "{m}");
        assert!(
            c.leaves_own_commoner,
            "{m}: a legal move keeps the commoner"
        );
        if c.is_capture {
            captures += 1;
        }
    }
    assert!(captures > 0, "fixture should offer captures");

    // A classification never claims legality: the self-explosion of the
    // documented fixture is classified and is illegal.
    let board = Board::from_fen("8/8/8/8/8/8/8/3qK3 w - - 0 1").unwrap();
    let ctx = board.effect_context();
    let m = Move::from_uci("e1d1").unwrap();
    let c = ctx.classification(&board, m);
    assert!(!c.leaves_own_commoner);
    assert!(!ctx.is_legal(&board, m));
    assert_eq!(
        c,
        ctx.effect(&board, m).classification(),
        "classification is the effect without the verdict"
    );

    // And it agrees with `effect` for every pseudo-legal candidate, legal or
    // not.
    let mut pseudo = MoveList::new();
    generate_pseudo_legal(&board, &mut pseudo);
    for &m in pseudo.as_slice() {
        assert_eq!(
            ctx.classification(&board, m),
            ctx.effect(&board, m).classification(),
            "{m}"
        );
    }
}

#[test]
fn readme_composite_shape_compiles_and_answers() {
    // The exact shape the README advertises: one derivation yields the child's
    // key and its terminal state, and the key is `Board::hash_after`.
    fn child_key_and_terminal(board: &Board, m: Move) -> Option<(u64, Option<Outcome>)> {
        let ctx = board.effect_context();
        let (post, key) = ctx.post_move_and_key(board, m)?;
        Some((key, post.terminal().map(|t| t.outcome)))
    }

    let board = Board::from_fen("7k/8/8/5N2/3K4/8/8/R6R w - - 0 1").unwrap();
    let mate = Move::from_uci("a1g1").unwrap();
    assert_eq!(
        child_key_and_terminal(&board, mate),
        Some((board.hash_after(mate), Some(Outcome::Loss)))
    );

    let quiet = Move::from_uci("h1h2").unwrap();
    assert_eq!(
        child_key_and_terminal(&board, quiet),
        Some((board.hash_after(quiet), None))
    );

    // Same answer as the separate query the composite replaces.
    let ctx = board.effect_context();
    assert_eq!(
        child_key_and_terminal(&board, mate).unwrap().1,
        ctx.terminal_after(&board, mate).map(|t| t.outcome)
    );

    // Malformed input stays `None` (the sentinel included).
    assert_eq!(child_key_and_terminal(&board, Move::NONE), None);
}

#[test]
fn public_types_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MoveClassification>();
    assert_send_sync::<MoveEffect>();
    assert_send_sync::<PostMove>();
    assert_send_sync::<Counts>();
    assert_send_sync::<AfterState>();
    assert_send_sync::<EffectContext>();
    assert_send_sync::<Terminal>();
    assert_send_sync::<Outcome>();
}
