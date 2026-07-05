use crate::attacks;
use crate::bitboard::*;
use crate::types::*;
use std::fmt;

fn char_to_piece(c: char) -> Option<Piece> {
    match c {
        'P' => Some(W_PAWN),
        'N' => Some(W_KNIGHT),
        'B' => Some(W_BISHOP),
        'R' => Some(W_ROOK),
        'Q' => Some(W_QUEEN),
        'C' => Some(W_COMMONER),
        'K' => Some(W_COMMONER),
        'p' => Some(B_PAWN),
        'n' => Some(B_KNIGHT),
        'b' => Some(B_BISHOP),
        'r' => Some(B_ROOK),
        'q' => Some(B_QUEEN),
        'c' => Some(B_COMMONER),
        'k' => Some(B_COMMONER),
        _ => None,
    }
}

/// Cached state for a position, used during move generation and legality checks.
///
/// Fields are populated by [`Board::populate_state`] and consumed by
/// [`Board::legal`] and [`generate_legal`](crate::movegen::generate_legal).
#[derive(Debug, Clone, Copy)]
pub struct StateInfo {
    // Hot fields (read in legal() and generate_legal())
    pub checkers: Bitboard,
    pub pinned: Bitboard,
    pub commoners_count: u32,
    pub them_commoners_count: u32,

    // Cold fields (read in undo_move, write in do_move / populate_state)
    pub castling_rights: u8,
    pub ep_square: Option<Square>,
    pub rule50: u8,
    pub captured_count: u8,
    pub captured: [(Square, Piece); 9],
    pub cap_sq: Option<Square>,
    pub cap_piece: Piece,
}

impl Default for StateInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl StateInfo {
    /// Create a new `StateInfo` with all fields zeroed/empty.
    pub fn new() -> Self {
        StateInfo {
            checkers: Bitboard::EMPTY,
            pinned: Bitboard::EMPTY,
            commoners_count: 0,
            them_commoners_count: 0,
            castling_rights: 0,
            ep_square: None,
            rule50: 0,
            captured_count: 0,
            captured: [(Square::NONE, NO_PIECE); 9],
            cap_sq: None,
            cap_piece: NO_PIECE,
        }
    }
}

/// A chessboard with atomic chess rules.
///
/// Maintains piece placement, side-to-move, castling rights, en-passant
/// square, and the half-move clock. Supports FEN serialization/deserialization,
/// making and unmaking moves, and legality checking.
#[derive(Debug, Clone)]
pub struct Board {
    squares: [Piece; 64],
    by_color: [Bitboard; 2],
    by_type: [Bitboard; 6],
    side_to_move: Color,
    castling_rights: u8,
    ep_square: Option<Square>,
    rule50: u8,
    game_ply: u16,
}

pub(crate) const WK_CASTLE: u8 = 1;
pub(crate) const WQ_CASTLE: u8 = 2;
pub(crate) const BK_CASTLE: u8 = 4;
pub(crate) const BQ_CASTLE: u8 = 8;

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    /// Create a board in the standard starting position.
    pub fn new() -> Self {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        Board::from_fen(fen).expect("Failed to create starting position")
    }

    /// Parse a board from a FEN string.
    ///
    /// Accepts standard FEN with 4–6 space-separated fields. The piece
    /// character set includes standard chess pieces plus `C`/`c` and `K`/`k`
    /// for commoners (king-like pseudo-royal pieces).
    pub fn from_fen(fen: &str) -> Result<Self, String> {
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(format!(
                "FEN too short: expected at least 4 parts, got {}",
                parts.len()
            ));
        }

        let mut squares = [NO_PIECE; 64];
        let mut by_color = [Bitboard::EMPTY; 2];
        let mut by_type = [Bitboard::EMPTY; 6];

        // FEN piece placement: rank 8 to rank 1, left to right
        // Our board indices: rank 1 = 0-7, rank 2 = 8-15, ..., rank 8 = 56-63
        let rows: Vec<&str> = parts[0].split('/').collect();
        if rows.len() != 8 {
            return Err(format!("Expected 8 ranks in FEN, got {}", rows.len()));
        }
        for (ri, row) in rows.iter().enumerate() {
            let rank_idx = 7 - ri; // rank 0 (index 7) = rank 8 in FEN
            let mut col = 0usize;
            for c in row.chars() {
                if c.is_ascii_digit() {
                    col += c.to_digit(10).unwrap() as usize;
                } else if let Some(piece) = char_to_piece(c) {
                    let sq_idx = rank_idx * 8 + col;
                    if sq_idx < 64 {
                        squares[sq_idx] = piece;
                        let sq = Square::from_u8(sq_idx as u8);
                        let bb = Bitboard::square_bb(sq);
                        by_color[piece.color() as usize] = by_color[piece.color() as usize] | bb;
                        by_type[piece.type_of() as usize] = by_type[piece.type_of() as usize] | bb;
                    }
                    col += 1;
                }
            }
        }

        let side_to_move = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(format!("Invalid side to move: {}", parts[1])),
        };

        let mut castling_rights = 0u8;
        for c in parts[2].chars() {
            match c {
                'K' => castling_rights |= WK_CASTLE,
                'Q' => castling_rights |= WQ_CASTLE,
                'k' => castling_rights |= BK_CASTLE,
                'q' => castling_rights |= BQ_CASTLE,
                '-' => break,
                _ => {}
            }
        }

        let ep_square = if parts[3] == "-" {
            None
        } else {
            Some(crate::types::parse_sq(parts[3]))
        };

        let rule50 = if parts.len() > 4 {
            parts[4].parse::<u8>().unwrap_or(0)
        } else {
            0
        };

        let game_ply = if parts.len() > 5 {
            parts[5].parse::<u16>().unwrap_or(1)
        } else {
            1
        };

        Ok(Board {
            squares,
            by_color,
            by_type,
            side_to_move,
            castling_rights,
            ep_square,
            rule50,
            game_ply,
        })
    }

    /// Serialize the board to a FEN string.
    pub fn fen(&self) -> String {
        let mut fen = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                let idx = rank * 8 + file;
                if self.squares[idx] == NO_PIECE {
                    empty += 1;
                } else {
                    if empty > 0 {
                        fen.push_str(&empty.to_string());
                        empty = 0;
                    }
                    fen.push(self.squares[idx].ascii_char());
                }
            }
            if empty > 0 {
                fen.push_str(&empty.to_string());
            }
            if rank > 0 {
                fen.push('/');
            }
        }

        fen.push(' ');
        fen.push(match self.side_to_move {
            Color::White => 'w',
            Color::Black => 'b',
        });

        fen.push(' ');
        let mut has_castling = false;
        if self.castling_rights & WK_CASTLE != 0 {
            fen.push('K');
            has_castling = true;
        }
        if self.castling_rights & WQ_CASTLE != 0 {
            fen.push('Q');
            has_castling = true;
        }
        if self.castling_rights & BK_CASTLE != 0 {
            fen.push('k');
            has_castling = true;
        }
        if self.castling_rights & BQ_CASTLE != 0 {
            fen.push('q');
            has_castling = true;
        }
        if !has_castling {
            fen.push('-');
        }

        fen.push(' ');
        match self.ep_square {
            Some(sq) => fen.push_str(&crate::types::sq_str(sq)),
            None => fen.push('-'),
        }

        fen.push(' ');
        fen.push_str(&self.rule50.to_string());
        fen.push(' ');
        fen.push_str(&self.game_ply.to_string());

        fen
    }

    /// Return the piece on a square (or [`NO_PIECE`] if empty).
    #[inline(always)]
    pub fn piece_on(&self, sq: Square) -> Piece {
        self.squares[sq as usize]
    }

    /// Return `true` if the given square is empty.
    pub fn empty(&self, sq: Square) -> bool {
        self.squares[sq as usize] == NO_PIECE
    }

    /// Return the bitboard of all occupied squares.
    #[inline(always)]
    pub fn occupied(&self) -> Bitboard {
        self.by_color[0] | self.by_color[1]
    }

    /// Return all pieces of a given color.
    #[inline(always)]
    pub fn pieces_color(&self, c: Color) -> Bitboard {
        self.by_color[c as usize]
    }

    /// Return all pieces of a given type.
    #[inline(always)]
    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard {
        self.by_type[pt as usize]
    }

    /// Return all pieces of a given color and type.
    #[inline(always)]
    pub fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard {
        self.by_color[c as usize] & self.by_type[pt as usize]
    }

    /// Return the side to move.
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// Return the castling rights bitmask.
    pub fn castling_rights(&self) -> u8 {
        self.castling_rights
    }

    /// Return the en-passant target square, if any.
    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// Return the bitboard of commoners (king-like pseudo-royal pieces) for a color.
    #[inline(always)]
    pub fn commoners(&self, c: Color) -> Bitboard {
        self.pieces_color_pt(c, PieceType::Commoner)
    }

    /// Return all pieces that attack `sq` on the given occupancy.
    ///
    /// Includes pawns, knights, bishops, rooks, queens, and commoners of either
    /// color that attack `sq`.
    pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
        let mut attackers = Bitboard::EMPTY;

        // Pawn attacks: the attackers come from the opposite direction
        let white_pawn_attacks = attacks::pawn_attacks(Color::White, sq);
        attackers =
            attackers | (white_pawn_attacks & self.pieces_color_pt(Color::Black, PieceType::Pawn));
        let black_pawn_attacks = attacks::pawn_attacks(Color::Black, sq);
        attackers =
            attackers | (black_pawn_attacks & self.pieces_color_pt(Color::White, PieceType::Pawn));

        // Knight attacks
        let knight_atk = attacks::knight_attacks(sq);
        attackers = attackers | (knight_atk & (self.by_type[PieceType::Knight as usize]));

        // Bishop/Queen attacks
        let bishop_atk = attacks::bishop_attacks(sq, occupied);
        attackers = attackers
            | (bishop_atk
                & (self.by_type[PieceType::Bishop as usize]
                    | self.by_type[PieceType::Queen as usize]));

        // Rook/Queen attacks
        let rook_atk = attacks::rook_attacks(sq, occupied);
        attackers = attackers
            | (rook_atk
                & (self.by_type[PieceType::Rook as usize]
                    | self.by_type[PieceType::Queen as usize]));

        // Commoner (king) attacks
        let king_atk = attacks::king_attacks(sq);
        attackers = attackers | (king_atk & self.by_type[PieceType::Commoner as usize]);

        attackers
    }

    pub(crate) fn compute_checkers(&self, us: Color) -> Bitboard {
        let them = us.flip();
        let commoners = self.commoners(us);
        if commoners.is_empty() {
            return Bitboard::EMPTY;
        }

        let mut checkers = Bitboard::EMPTY;
        let occupied = self.occupied();

        let them_bb = self.pieces_color(them);
        let mut c = commoners;
        while !c.is_empty() {
            let ksq = c.pop_lsb();
            let rook_atk = attacks::rook_attacks(ksq, occupied);
            let bishop_atk = attacks::bishop_attacks(ksq, occupied);
            let queen_atk = rook_atk | bishop_atk;
            checkers = checkers
                | (rook_atk & self.by_type[PieceType::Rook as usize] & them_bb)
                | (bishop_atk & self.by_type[PieceType::Bishop as usize] & them_bb)
                | (queen_atk & self.by_type[PieceType::Queen as usize] & them_bb)
                | (attacks::knight_attacks(ksq)
                    & self.by_type[PieceType::Knight as usize]
                    & them_bb)
                | (attacks::pawn_attacks(us, ksq)
                    & self.by_type[PieceType::Pawn as usize]
                    & them_bb);
        }

        // Adjacent commoner check (extinction pseudo-royal)
        let them_commoners = self.commoners(them);
        if !them_commoners.is_empty() && !commoners.is_empty() {
            let mut tc = them_commoners;
            while !tc.is_empty() {
                let tksq = tc.pop_lsb();
                if attacks::king_attacks(tksq) & commoners != Bitboard::EMPTY {
                    checkers = checkers | Bitboard::square_bb(tksq);
                }
            }
        }

        checkers
    }

    /// Return the bitboard of enemy pieces checking the side to move.
    pub fn checkers(&self) -> Bitboard {
        self.compute_checkers(self.side_to_move)
    }

    pub(crate) fn compute_pinned(&self, us: Color) -> Bitboard {
        let mut pinned = Bitboard::EMPTY;
        let commoners = self.commoners(us);
        let them = us.flip();
        let occupied = self.occupied();

        let mut c_iter = commoners;
        while !c_iter.is_empty() {
            let ksq = c_iter.pop_lsb();

            let mut snipers = ((self.by_type[PieceType::Rook as usize]
                | self.by_type[PieceType::Queen as usize])
                & attacks::rook_attacks(ksq, Bitboard::EMPTY))
                | ((self.by_type[PieceType::Bishop as usize]
                    | self.by_type[PieceType::Queen as usize])
                    & attacks::bishop_attacks(ksq, Bitboard::EMPTY));
            snipers = snipers & self.pieces_color(them);

            let mut s = snipers;
            while !s.is_empty() {
                let sniper_sq = s.pop_lsb();
                let between = between_bb(ksq, sniper_sq) & occupied;
                if between.count() == 1 {
                    pinned = pinned | between;
                }
            }
        }

        pinned
    }

    /// Return the bitboard of pieces of the given color that are pinned
    /// (cannot move without exposing a commoner to capture).
    pub fn pinned(&self, c: Color) -> Bitboard {
        self.compute_pinned(c)
    }

    /// Fill cached state fields (checkers, pinned, commoner counts) for the
    /// current position so that `legal()` can read them instead of recomputing.
    pub fn populate_state(&self, state: &mut StateInfo) {
        state.checkers = self.compute_checkers(self.side_to_move);
        state.pinned = self.compute_pinned(self.side_to_move);
        state.commoners_count = self.commoners(self.side_to_move).count();
        state.them_commoners_count = self.commoners(self.side_to_move.flip()).count();
    }

    /// Make `m` on the board, storing undo information in `state`.
    ///
    /// Handles all move types (normal, promotion, en-passant, castling) as well
    /// as atomic-blast removal on captures.
    pub fn do_move(&mut self, m: Move, state: &mut StateInfo) {
        state.castling_rights = self.castling_rights;
        state.ep_square = self.ep_square;
        state.rule50 = self.rule50;
        state.captured_count = 0;
        state.cap_sq = None;
        state.cap_piece = NO_PIECE;

        let us = self.side_to_move;
        let them = us.flip();
        let from = m.from_sq();
        let to = m.to_sq();
        let piece = self.squares[from as usize];
        let pt = piece.type_of();
        let is_capture = !self.empty(to);

        if m.move_type() == MoveType::Castling {
            let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
            self.move_piece(kfrom, kto);
            self.move_piece(rfrom, rto);
            self.castling_rights &= match us {
                Color::White => !(WK_CASTLE | WQ_CASTLE),
                Color::Black => !(BK_CASTLE | BQ_CASTLE),
            };
            self.ep_square = None;
            self.rule50 += 1;
            self.side_to_move = them;
            self.game_ply += 1;
            return;
        }

        if m.move_type() == MoveType::EnPassant {
            let ep_cap = match us {
                Color::White => Square::from_index(to as i8 - 8),
                Color::Black => Square::from_index(to as i8 + 8),
            };
            let cap_piece = self.squares[ep_cap as usize];
            state.captured[state.captured_count as usize] = (ep_cap, cap_piece);
            state.captured_count += 1;
            self.remove_piece(ep_cap);
        } else if is_capture {
            let cap_piece = self.squares[to as usize];
            state.cap_sq = Some(to);
            state.cap_piece = cap_piece;
            self.remove_piece(to);
        }

        if m.move_type() == MoveType::Promotion {
            let prom_pt = m.promotion_type();
            let prom_piece = make_piece(us, prom_pt);
            self.remove_piece(from);
            self.squares[to as usize] = prom_piece;
            self.by_color[us as usize] = self.by_color[us as usize] | Bitboard::square_bb(to);
            self.by_type[prom_pt as usize] =
                self.by_type[prom_pt as usize] | Bitboard::square_bb(to);
        } else {
            self.move_piece(from, to);
        }

        // Blast on capture
        if is_capture || m.move_type() == MoveType::EnPassant {
            // Blast zone = king attacks from `to`, minus pawn squares
            let blast_zone = attacks::king_attacks(to) & !self.by_type[PieceType::Pawn as usize];
            let mut to_blast = blast_zone & self.occupied();

            // Always blast the capturer at ground zero (pawns are NOT immune at `to`).
            to_blast = to_blast | Bitboard::square_bb(to);

            let mut b = to_blast;
            while !b.is_empty() {
                let bsq = b.pop_lsb();
                let bpiece = self.squares[bsq as usize];
                if bpiece != NO_PIECE {
                    state.captured[state.captured_count as usize] = (bsq, bpiece);
                    state.captured_count += 1;
                    self.remove_piece(bsq);
                }
            }
        }

        self.update_castling_rights(from, to, us);

        if is_capture || m.move_type() == MoveType::EnPassant {
            // White king-side
            if self.castling_rights & WK_CASTLE != 0
                && self.squares[Square::H1 as usize] != make_piece(Color::White, PieceType::Rook)
            {
                self.castling_rights &= !WK_CASTLE;
            }
            // White queen-side
            if self.castling_rights & WQ_CASTLE != 0
                && self.squares[Square::A1 as usize] != make_piece(Color::White, PieceType::Rook)
            {
                self.castling_rights &= !WQ_CASTLE;
            }
            // Black king-side
            if self.castling_rights & BK_CASTLE != 0
                && self.squares[Square::H8 as usize] != make_piece(Color::Black, PieceType::Rook)
            {
                self.castling_rights &= !BK_CASTLE;
            }
            // Black queen-side
            if self.castling_rights & BQ_CASTLE != 0
                && self.squares[Square::A8 as usize] != make_piece(Color::Black, PieceType::Rook)
            {
                self.castling_rights &= !BQ_CASTLE;
            }
        }

        if pt == PieceType::Pawn && (to as i8 - from as i8).abs() == 16 {
            self.ep_square = Some(match us {
                Color::White => Square::from_index(from as i8 + 8),
                Color::Black => Square::from_index(from as i8 - 8),
            });
        } else {
            self.ep_square = None;
        }

        if pt == PieceType::Pawn || is_capture {
            self.rule50 = 0;
        } else {
            self.rule50 += 1;
        }

        self.side_to_move = them;
        self.game_ply += 1;

        self.populate_state(state);
    }

    /// Unmake `m`, restoring the board to its state before [`do_move`](Self::do_move).
    ///
    /// `state` must be the same [`StateInfo`] that was passed to `do_move`.
    pub fn undo_move(&mut self, m: Move, state: &StateInfo) {
        self.castling_rights = state.castling_rights;
        self.ep_square = state.ep_square;
        self.rule50 = state.rule50;

        self.side_to_move = self.side_to_move.flip();
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();

        if m.move_type() == MoveType::Castling {
            let (kfrom, kto, rfrom, rto) = castling_squares(us, to > from);
            self.move_piece(kto, kfrom);
            self.move_piece(rto, rfrom);
            self.game_ply -= 1;
            return;
        }

        let mut i = state.captured_count;
        while i > 0 {
            i -= 1;
            let (sq, piece) = state.captured[i as usize];
            self.place_piece(piece, sq);
        }

        if m.move_type() == MoveType::Promotion {
            let pawn = make_piece(us, PieceType::Pawn);
            self.remove_piece(to);
            self.place_piece(pawn, from);
        } else {
            self.move_piece(to, from);
        }

        if let Some(sq) = state.cap_sq {
            self.place_piece(state.cap_piece, sq);
        }

        self.game_ply -= 1;
    }

    fn move_piece(&mut self, from: Square, to: Square) {
        let piece = self.squares[from as usize];
        debug_assert!(piece != NO_PIECE);
        self.squares[to as usize] = piece;
        self.squares[from as usize] = NO_PIECE;

        let from_bb = Bitboard::square_bb(from);
        let to_bb = Bitboard::square_bb(to);

        let c = piece.color();
        let pt = piece.type_of();
        self.by_color[c as usize] = (self.by_color[c as usize] ^ from_bb) | to_bb;
        self.by_type[pt as usize] = (self.by_type[pt as usize] ^ from_bb) | to_bb;
    }

    fn remove_piece(&mut self, sq: Square) {
        let piece = self.squares[sq as usize];
        if piece == NO_PIECE {
            return;
        }
        self.squares[sq as usize] = NO_PIECE;
        let sq_bb = Bitboard::square_bb(sq);
        self.by_color[piece.color() as usize] = self.by_color[piece.color() as usize] ^ sq_bb;
        self.by_type[piece.type_of() as usize] = self.by_type[piece.type_of() as usize] ^ sq_bb;
    }

    fn place_piece(&mut self, piece: Piece, sq: Square) {
        debug_assert!(self.squares[sq as usize] == NO_PIECE);
        self.squares[sq as usize] = piece;
        let sq_bb = Bitboard::square_bb(sq);
        self.by_color[piece.color() as usize] = self.by_color[piece.color() as usize] | sq_bb;
        self.by_type[piece.type_of() as usize] = self.by_type[piece.type_of() as usize] | sq_bb;
    }

    fn update_castling_rights(&mut self, from: Square, to: Square, _us: Color) {
        // King side
        if from == Square::E1 || from == Square::H1 || to == Square::H1 {
            self.castling_rights &= !WK_CASTLE;
        }
        if from == Square::E1 || from == Square::A1 || to == Square::A1 {
            self.castling_rights &= !WQ_CASTLE;
        }
        if from == Square::E8 || from == Square::H8 || to == Square::H8 {
            self.castling_rights &= !BK_CASTLE;
        }
        if from == Square::E8 || from == Square::A8 || to == Square::A8 {
            self.castling_rights &= !BQ_CASTLE;
        }
    }

    /// Check whether `m` is legal under atomic chess rules.
    ///
    /// Considers blast-zone effects (self-explosion), castling pass-through
    /// safety, pseudo-royal adjacency, and commoner extinction.
    ///
    /// `state` must contain up-to-date cached fields for the current position.
    pub fn legal(&self, m: Move, state: &StateInfo) -> bool {
        let from = m.from_sq();
        let to = m.to_sq();
        let us = self.side_to_move;
        let them = us.flip();

        let piece = self.piece_on(from);
        if piece == NO_PIECE {
            return false;
        }

        let is_capture = m.move_type() != MoveType::Castling
            && (m.move_type() == MoveType::EnPassant || self.piece_on(to) != NO_PIECE);

        if m.move_type() == MoveType::Castling {
            let ksq = from;
            let occupied = self.occupied();
            let pass_through = if to > ksq {
                [ksq, Square::from_index(ksq as i8 + 1)]
            } else {
                [Square::from_index(ksq as i8 - 1), ksq]
            };
            for &sq in &pass_through {
                let adjacent_enemy_commoners = self.commoners(them) & attacks::king_attacks(sq);
                if adjacent_enemy_commoners.is_empty() {
                    let atk = attackers_to(self, sq, occupied, self.by_color[them as usize])
                        | (attacks::king_attacks(sq)
                            & self.by_type[PieceType::Commoner as usize]
                            & self.by_color[them as usize]);
                    if atk != Bitboard::EMPTY {
                        return false;
                    }
                }
            }
        }

        let mut occupied = self.occupied() ^ from;
        let mut kto = to;

        if m.move_type() == MoveType::Castling {
            let (_, kto_actual, rfrom, rto) = castling_squares(us, to > from);
            kto = kto_actual;
            occupied = occupied ^ Bitboard::square_bb(rfrom);
            occupied = occupied | Bitboard::square_bb(rto);
        } else if m.move_type() == MoveType::EnPassant {
            let capsq = match us {
                Color::White => Square::from_index(to as i8 - 8),
                Color::Black => Square::from_index(to as i8 + 8),
            };
            occupied = occupied & !Bitboard::square_bb(capsq);
        }

        occupied = occupied | Bitboard::square_bb(kto);

        if is_capture {
            let pre_pawns = self.by_type[PieceType::Pawn as usize];
            let pre_non_pawns = self.occupied() ^ pre_pawns;
            let blast_adjacent = attacks::king_attacks(kto) & pre_non_pawns;
            occupied = occupied & !(blast_adjacent | Bitboard::square_bb(kto));
        }

        let mut our_commoners = self.commoners(us) & occupied;

        if m.move_type() == MoveType::Castling {
            our_commoners = our_commoners | Bitboard::square_bb(kto);
        } else if !is_capture {
            let moving_piece = self.piece_on(from);
            if moving_piece != NO_PIECE && moving_piece.type_of() == PieceType::Commoner {
                our_commoners = our_commoners | Bitboard::square_bb(kto);
            }
        }

        if our_commoners.is_empty() {
            return false;
        }

        let our_pr_count = state.commoners_count as usize;
        let them_pr_count = state.them_commoners_count as usize;

        if our_pr_count <= 1 {
            let enemy_pr_destroyed =
                them_pr_count <= 1 && (self.commoners(them) & occupied).is_empty();

            if !enemy_pr_destroyed {
                let them_commoners = self.commoners(them);
                let enemy_survivors = self.by_color[them as usize] & occupied;

                let mut c = our_commoners;
                while !c.is_empty() {
                    let ksq = c.pop_lsb();
                    let adjacent_enemy = them_commoners & attacks::king_attacks(ksq);
                    if adjacent_enemy.is_empty()
                        && attackers_to(self, ksq, occupied, enemy_survivors) != Bitboard::EMPTY
                    {
                        return false;
                    }
                }
            }
        }

        true
    }
}

/// Return the bitboard of enemy sliding/leaper pieces (rook, bishop, queen,
/// knight, pawn) that attack `sq` on the given `occupied` board, filtered to
/// the enemy pieces in `enemy_bb`.  Commoner (king) attacks are *not* included
/// (they are handled separately via adjacency-immunity in the pseudo-royal
/// rules).
#[inline(always)]
fn attackers_to(board: &Board, sq: Square, occupied: Bitboard, enemy_bb: Bitboard) -> Bitboard {
    let rook_atk = attacks::rook_attacks(sq, occupied);
    let bishop_atk = attacks::bishop_attacks(sq, occupied);
    let queen_atk = rook_atk | bishop_atk;
    rook_atk & board.by_type[PieceType::Rook as usize] & enemy_bb
        | bishop_atk & board.by_type[PieceType::Bishop as usize] & enemy_bb
        | queen_atk & board.by_type[PieceType::Queen as usize] & enemy_bb
        | attacks::knight_attacks(sq) & board.by_type[PieceType::Knight as usize] & enemy_bb
        | attacks::pawn_attacks(board.side_to_move, sq)
            & board.by_type[PieceType::Pawn as usize]
            & enemy_bb
}

/// Return the castling square data for a given color and side.
fn castling_squares(us: Color, kingside: bool) -> (Square, Square, Square, Square) {
    match (us, kingside) {
        (Color::White, true) => (Square::E1, Square::G1, Square::H1, Square::F1),
        (Color::White, false) => (Square::E1, Square::C1, Square::A1, Square::D1),
        (Color::Black, true) => (Square::E8, Square::G8, Square::H8, Square::F8),
        (Color::Black, false) => (Square::E8, Square::C8, Square::A8, Square::D8),
    }
}

/// Returns `true` when `m` is trivially legal — i.e. not a capture, not a
/// commoner move, not en-passant, the moving piece is unpinned, there are no
/// checkers, and at least one own commoner still exists.
///
/// When this returns `true` the move is guaranteed legal without needing the
/// full `legal()` check (blast, pseudo-royal, castling pass-through).
#[inline(always)]
pub(crate) fn is_move_trivially_legal(board: &Board, m: Move, state: &StateInfo) -> bool {
    if !state.checkers.is_empty() {
        return false;
    }
    if state.commoners_count == 0 {
        return false;
    }

    let from = m.from_sq();
    let pt = board.piece_on(from).type_of();
    if pt == PieceType::Commoner {
        return false;
    }

    let mt = m.move_type();
    if mt == MoveType::EnPassant {
        return false;
    }

    // A non-Castling move that captures a piece (EnPassant already handled above).
    let is_capture = mt != MoveType::Castling && board.piece_on(m.to_sq()) != NO_PIECE;
    if is_capture {
        return false;
    }

    // Castling reaches here (not a capture, not en-passant, not a commoner move).
    // Castling still needs the full pass-through check, so reject fast-path.
    if mt == MoveType::Castling {
        return false;
    }

    // Check pin: a pinned piece might expose a commoner.
    if (state.pinned & Bitboard::square_bb(from)) != Bitboard::EMPTY {
        return false;
    }

    true
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        for rank in (0..8).rev() {
            write!(f, "{} ", rank + 1)?;
            for file in 0..8 {
                let wrapped_sq = make_square(
                    match file {
                        0 => File::A,
                        1 => File::B,
                        2 => File::C,
                        3 => File::D,
                        4 => File::E,
                        5 => File::F,
                        6 => File::G,
                        7 => File::H,
                        _ => unreachable!(),
                    },
                    match rank {
                        0 => Rank::R1,
                        1 => Rank::R2,
                        2 => Rank::R3,
                        3 => Rank::R4,
                        4 => Rank::R5,
                        5 => Rank::R6,
                        6 => Rank::R7,
                        7 => Rank::R8,
                        _ => unreachable!(),
                    },
                );
                let wrapped_idx = wrapped_sq as usize;
                if self.squares[wrapped_idx] == NO_PIECE {
                    write!(f, " .")?;
                } else {
                    write!(f, " {}", self.squares[wrapped_idx].ascii_char())?;
                }
            }
            writeln!(f)?;
        }
        writeln!(f, "   a b c d e f g h")?;
        writeln!(f, "Side to move: {:?}", self.side_to_move)?;
        writeln!(f, "Castling: {:b}", self.castling_rights)?;
        writeln!(f, "EP: {:?}", self.ep_square)?;
        writeln!(f, "FEN: {}", self.fen())?;
        Ok(())
    }
}

impl Square {
    /// Construct a [`Square`] from its 0–63 index. Returns [`Square::NONE`]
    /// for out-of-range values.
    pub fn from_index(idx: i8) -> Square {
        if (0..64).contains(&idx) {
            crate::types::SQUARES[idx as usize]
        } else {
            Square::NONE
        }
    }

    /// Construct a [`Square`] from its 0–63 index as a `u8`.
    pub fn from_u8(idx: u8) -> Square {
        Square::from_index(idx as i8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starting_position() {
        let board = Board::new();
        assert_eq!(board.piece_on(Square::E1), W_COMMONER);
        assert_eq!(board.piece_on(Square::D1), W_QUEEN);
        assert_eq!(board.piece_on(Square::E8), B_COMMONER);
        assert_eq!(board.side_to_move(), Color::White);
        assert_eq!(
            board.castling_rights,
            WK_CASTLE | WQ_CASTLE | BK_CASTLE | BQ_CASTLE
        );
    }

    #[test]
    fn test_fen_roundtrip() {
        let board = Board::new();
        let fen = board.fen();
        let board2 = Board::from_fen(&fen).unwrap();
        assert_eq!(board.fen(), board2.fen());
    }

    #[test]
    fn test_custom_fen() {
        let fen = "8/8/8/8/8/8/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.piece_on(Square::E1), W_COMMONER);
    }

    #[test]
    fn test_checkers() {
        // Position where white queen gives check to black commoner
        let fen = "4k3/8/8/8/8/8/8/4Q2K b - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let checkers = board.checkers();
        assert!(!checkers.is_empty(), "Expected checkers, got empty");
    }

    #[test]
    fn test_pinned() {
        // Black rook on e4, white pawn on e3, white commoner on e2 - pawn is pinned
        let fen = "4k3/8/8/8/4r3/4P3/4K3/8 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let pinned = board.pinned(Color::White);
        assert!(!pinned.is_empty(), "Expected pinned pieces, got empty");
    }

    #[test]
    fn test_do_undo_restores_state() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let mut board = Board::from_fen(fen).unwrap();
        let orig_fen = board.fen();
        let mut state = StateInfo::new();

        // e4
        let m = Move::make_move(Square::E2, Square::E4);
        board.do_move(m, &mut state);
        board.undo_move(m, &state);

        assert_eq!(board.fen(), orig_fen);
    }

    #[test]
    fn test_do_undo_capture_restores() {
        let fen2 = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
        let mut board2 = Board::from_fen(fen2).unwrap();
        let orig_fen = board2.fen();
        let mut state2 = StateInfo::new();

        let m = Move::make_move(Square::E4, Square::D5);
        board2.do_move(m, &mut state2);
        board2.undo_move(m, &state2);

        assert_eq!(board2.fen(), orig_fen);
    }

    #[test]
    fn test_self_explosion_legal_with_surviving_commoner() {
        // White commoners on d3 AND e1; white rook on d5, black pawn on d4.
        // Rook takes pawn on d4 — blast zone (c3-e5) includes d3, destroying
        // the commoner on d3, but the commoner on e1 survives.
        // This is NOT self-explosion — only the LAST commoner being destroyed
        // makes a move illegal.
        let fen = "4k3/8/8/3R4/3p4/3C4/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        // The capture should be LEGAL because e1 survives and is not under attack.
        let has_rook_d4 = moves
            .as_slice()
            .iter()
            .any(|&m| m.from_sq() == Square::D5 && m.to_sq() == Square::D4);
        assert!(
            has_rook_d4,
            "rook capture on d4 should be legal (e1 commoner survives)"
        );
    }

    #[test]
    fn test_self_explosion_illegal_last_commoner() {
        // White commoner ONLY on d3; white rook on d5, black pawn on d4.
        // Rook takes pawn on d4 — blast zone includes d3, destroying the
        // LAST (only) commoner → self-explosion, illegal.
        let fen = "4k3/8/8/3R4/3p4/3C4/8/8 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        for &m in moves.as_slice() {
            assert!(
                m.from_sq() != Square::D5 || m.to_sq() != Square::D4,
                "rook capture on d4 should be illegal (last commoner destroyed)"
            );
        }
    }

    #[test]
    fn test_blast_zone_removes_pieces() {
        // White rook on e4, black knight on e5, black pawn on f5
        // Rook captures knight — blast zone around e5 (d4-f4, d5-f5, d6-f6)
        // removes: rook (non-pawn capturer), knight, but NOT the pawn on f5
        let fen = "4k3/8/8/4np2/4R3/8/8/4K3 w - - 0 1";
        let mut board = Board::from_fen(fen).unwrap();
        let mut state = StateInfo::new();
        let m = Move::make_move(Square::E4, Square::E5);
        board.do_move(m, &mut state);
        // The rook and knight should be gone; the black pawn on f5 should remain
        assert!(
            board.piece_on(Square::E4) == NO_PIECE,
            "rook at e4 should be gone"
        );
        assert!(
            board.piece_on(Square::E5) == NO_PIECE,
            "knight at e5 should be gone"
        );
        assert!(
            board.piece_on(Square::F5) == B_PAWN,
            "pawn at f5 should survive"
        );
    }

    #[test]
    fn test_pinned_piece_capture_explodes_pinner() {
        // Black rook on e5 (pinning), white rook on e3, black pawn on e4,
        // white commoner on e1. The rook on e3 is pinned by the rook on e5
        // (both on e-file, commoner on e1 behind).
        // But rook captures pawn on e4 — blast zone (d3-f5) destroys the
        // rook on e5, so the pin is removed and the move is legal.
        let fen = "4k3/8/8/4r3/4p3/4R3/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mut moves = MoveList::new();
        crate::movegen::generate_legal(&board, &mut moves);
        let has_rook_e4 = moves
            .as_slice()
            .iter()
            .any(|&m| m.from_sq() == Square::E3 && m.to_sq() == Square::E4);
        assert!(
            has_rook_e4,
            "rook capture on e4 should be legal (blast removes pinning rook)"
        );
    }

    #[test]
    fn test_en_passant_blast() {
        // White pawn on d5, black pawn on c5 (just double-pushed), black knight on d4
        // White plays dxc6 en passant — blast at c6
        let fen2 = "4k3/8/8/2Pp4/8/8/8/4K3 w KQkq d6 0 2";
        let mut board2 = Board::from_fen(fen2).unwrap();
        let mut state2 = StateInfo::new();
        let m = Move::make_enpassant(Square::C5, Square::D6);
        board2.do_move(m, &mut state2);
        // After EP capture + blast: pawns on c5 and d5 are gone,
        // commoners should remain (out of blast zone)
        assert!(board2.piece_on(Square::C5) == NO_PIECE);
        assert!(board2.piece_on(Square::D5) == NO_PIECE);
    }
}
