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

#[derive(Debug, Clone)]
pub struct StateInfo {
    pub castling_rights: u8,
    pub ep_square: Option<Square>,
    pub rule50: u8,
    pub captured_count: u8,
    pub captured: [(Square, Piece); 9],
    pub cap_sq: Option<Square>,
    pub cap_piece: Piece,
}

impl StateInfo {
    pub fn new() -> Self {
        StateInfo {
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

impl Board {
    pub fn new() -> Self {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        Board::from_fen(fen).expect("Failed to create starting position")
    }

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
            Some(parse_square(parts[3]))
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
                    let p = self.squares[idx];
                    let c = match p.color() {
                        Color::White => 'W',
                        Color::Black => 'B',
                    };
                    let t = match p.type_of() {
                        PieceType::Pawn => 'P',
                        PieceType::Knight => 'N',
                        PieceType::Bishop => 'B',
                        PieceType::Rook => 'R',
                        PieceType::Queen => 'Q',
                        PieceType::Commoner => 'C',
                    };
                    fen.push(if c == 'W' { t } else { t.to_ascii_lowercase() });
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
            Some(sq) => fen.push_str(&square_str(sq)),
            None => fen.push('-'),
        }

        fen.push(' ');
        fen.push_str(&self.rule50.to_string());
        fen.push(' ');
        fen.push_str(&self.game_ply.to_string());

        fen
    }

    pub fn piece_on(&self, sq: Square) -> Piece {
        self.squares[sq as usize]
    }

    pub fn empty(&self, sq: Square) -> bool {
        self.squares[sq as usize] == NO_PIECE
    }

    pub fn pieces(&self) -> Bitboard {
        self.by_color[0] | self.by_color[1]
    }

    pub fn pieces_color(&self, c: Color) -> Bitboard {
        self.by_color[c as usize]
    }

    pub fn pieces_pt(&self, pt: PieceType) -> Bitboard {
        self.by_type[pt as usize]
    }

    pub fn pieces_color_pt(&self, c: Color, pt: PieceType) -> Bitboard {
        self.by_color[c as usize] & self.by_type[pt as usize]
    }

    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub fn castling_rights(&self) -> u8 {
        self.castling_rights
    }

    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    pub fn commoners(&self, c: Color) -> Bitboard {
        self.pieces_color_pt(c, PieceType::Commoner)
    }

    pub fn occupied(&self) -> Bitboard {
        self.pieces()
    }

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

    pub fn checkers(&self) -> Bitboard {
        let us = self.side_to_move;
        let them = us.flip();
        let commoners = self.commoners(us);
        if commoners.is_empty() {
            return Bitboard::EMPTY;
        }

        let mut checkers = Bitboard::EMPTY;
        let occupied = self.occupied();

        let mut c = commoners;
        while !c.is_empty() {
            let ksq = c.pop_lsb();
            checkers = checkers
                | (attacks::rook_attacks(ksq, occupied)
                    & self.by_type[PieceType::Rook as usize]
                    & self.pieces_color(them))
                | (attacks::bishop_attacks(ksq, occupied)
                    & self.by_type[PieceType::Bishop as usize]
                    & self.pieces_color(them))
                | (attacks::queen_attacks(ksq, occupied)
                    & self.by_type[PieceType::Queen as usize]
                    & self.pieces_color(them))
                | (attacks::knight_attacks(ksq)
                    & self.by_type[PieceType::Knight as usize]
                    & self.pieces_color(them));
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

    pub fn pinned(&self, c: Color) -> Bitboard {
        let mut pinned = Bitboard::EMPTY;
        let commoners = self.commoners(c);
        let them = c.flip();
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
                if !between.more_than_one() {
                    pinned = pinned | between;
                }
            }
        }

        pinned
    }

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

        // Handle castling
        if m.move_type() == MoveType::Castling {
            let (kfrom, kto, rfrom, rto) = match (us, to > from) {
                (Color::White, true) => (Square::E1, Square::G1, Square::H1, Square::F1),
                (Color::White, false) => (Square::E1, Square::C1, Square::A1, Square::D1),
                (Color::Black, true) => (Square::E8, Square::G8, Square::H8, Square::F8),
                (Color::Black, false) => (Square::E8, Square::C8, Square::A8, Square::D8),
            };
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

        // Handle en passant
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
            // Record the captured piece and remove it
            let cap_piece = self.squares[to as usize];
            state.cap_sq = Some(to);
            state.cap_piece = cap_piece;
            self.remove_piece(to);
        }

        // Move the piece
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
            let mut to_blast = blast_zone & self.pieces();

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

        // Update castling rights (handles rook movement AND blast removal)
        self.update_castling_rights(from, to, us);

        // Also handle blast-related rook removal
        if is_capture || m.move_type() == MoveType::EnPassant {
            // White king-side
            if self.castling_rights & WK_CASTLE != 0 {
                if self.squares[Square::H1 as usize] != make_piece(Color::White, PieceType::Rook) {
                    self.castling_rights &= !WK_CASTLE;
                }
            }
            // White queen-side
            if self.castling_rights & WQ_CASTLE != 0 {
                if self.squares[Square::A1 as usize] != make_piece(Color::White, PieceType::Rook) {
                    self.castling_rights &= !WQ_CASTLE;
                }
            }
            // Black king-side
            if self.castling_rights & BK_CASTLE != 0 {
                if self.squares[Square::H8 as usize] != make_piece(Color::Black, PieceType::Rook) {
                    self.castling_rights &= !BK_CASTLE;
                }
            }
            // Black queen-side
            if self.castling_rights & BQ_CASTLE != 0 {
                if self.squares[Square::A8 as usize] != make_piece(Color::Black, PieceType::Rook) {
                    self.castling_rights &= !BQ_CASTLE;
                }
            }
        }

        // Update en passant square
        if pt == PieceType::Pawn && (to as i8 - from as i8).abs() == 16 {
            self.ep_square = Some(match us {
                Color::White => Square::from_index(from as i8 + 8),
                Color::Black => Square::from_index(from as i8 - 8),
            });
        } else {
            self.ep_square = None;
        }

        // Update rule50
        if pt == PieceType::Pawn || is_capture {
            self.rule50 = 0;
        } else {
            self.rule50 += 1;
        }

        self.side_to_move = them;
        self.game_ply += 1;
    }

    pub fn undo_move(&mut self, m: Move, state: &StateInfo) {
        self.castling_rights = state.castling_rights;
        self.ep_square = state.ep_square;
        self.rule50 = state.rule50;

        self.side_to_move = self.side_to_move.flip();
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();

        // Handle castling
        if m.move_type() == MoveType::Castling {
            let (kfrom, kto, rfrom, rto) = match (us, to > from) {
                (Color::White, true) => (Square::E1, Square::G1, Square::H1, Square::F1),
                (Color::White, false) => (Square::E1, Square::C1, Square::A1, Square::D1),
                (Color::Black, true) => (Square::E8, Square::G8, Square::H8, Square::F8),
                (Color::Black, false) => (Square::E8, Square::C8, Square::A8, Square::D8),
            };
            self.move_piece(kto, kfrom);
            self.move_piece(rto, rfrom);
            self.game_ply -= 1;
            return;
        }

        // Restore blast victims (includes capturer if non-pawn) in reverse order
        let mut i = state.captured_count;
        while i > 0 {
            i -= 1;
            let (sq, piece) = state.captured[i as usize];
            self.place_piece(piece, sq);
        }

        // Move the piece back from `to` to `from`
        // For non-pawn captures: the piece was blasted and just restored via captured_pieces
        // For pawn captures: the piece survived at `to`
        if m.move_type() == MoveType::Promotion {
            let pawn = make_piece(us, PieceType::Pawn);
            self.remove_piece(to);
            self.place_piece(pawn, from);
        } else {
            self.move_piece(to, from);
        }

        // Restore the original captured piece from regular capture
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

    pub fn legal(&self, m: Move) -> bool {
        let from = m.from_sq();
        let to = m.to_sq();
        let us = self.side_to_move;
        let them = us.flip();

        // Moving piece must exist
        if self.piece_on(from) == NO_PIECE {
            return false;
        }

        // Castling: check pass-through squares BEFORE the move
        // (with current board state, before king/rook move)
        if m.move_type() == MoveType::Castling {
            let ksq = from;
            let occupied = self.occupied();
            let pass_through = if to > ksq {
                [ksq, Square::from_index(ksq as i8 + 1)]
            } else {
                [Square::from_index(ksq as i8 - 1), ksq]
            };
            for &sq in &pass_through {
                // Blast adjacency immunity: skip attack check if an enemy
                // commoner is adjacent (mutual destruction would protect it)
                let adjacent_enemy_commoners = self.commoners(them) & attacks::king_attacks(sq);
                if adjacent_enemy_commoners.is_empty() {
                    let rook_attackers = attacks::rook_attacks(sq, occupied)
                        & self.by_type[PieceType::Rook as usize]
                        & self.by_color[them as usize];
                    let bishop_attackers = attacks::bishop_attacks(sq, occupied)
                        & self.by_type[PieceType::Bishop as usize]
                        & self.by_color[them as usize];
                    let queen_attackers = attacks::queen_attacks(sq, occupied)
                        & self.by_type[PieceType::Queen as usize]
                        & self.by_color[them as usize];
                    let knight_attackers = attacks::knight_attacks(sq)
                        & self.by_type[PieceType::Knight as usize]
                        & self.by_color[them as usize];
                    let commoner_attackers = attacks::king_attacks(sq)
                        & self.by_type[PieceType::Commoner as usize]
                        & self.by_color[them as usize];
                    let pawn_attackers = attacks::pawn_attacks(self.side_to_move, sq)
                        & self.by_type[PieceType::Pawn as usize]
                        & self.by_color[them as usize];
                    if (rook_attackers
                        | bishop_attackers
                        | queen_attackers
                        | knight_attackers
                        | commoner_attackers
                        | pawn_attackers)
                        != Bitboard::EMPTY
                    {
                        return false;
                    }
                }
            }
            // Fall through to destination check below
        }

        // Pre-compute occupied after the move (without cloning/do_move)
        let mut occupied = self.pieces() ^ from; // remove moving piece from origin
        let mut kto = to; // king/square destination (may differ from `to` for castling)

        if m.move_type() == MoveType::Castling {
            // Adjust for castling: king moves to kto_actual, rook moves rfrom->rto
            let (kto_actual, rfrom, rto) = match (us, to > from) {
                (Color::White, true) => (Square::G1, Square::H1, Square::F1),
                (Color::White, false) => (Square::C1, Square::A1, Square::D1),
                (Color::Black, true) => (Square::G8, Square::H8, Square::F8),
                (Color::Black, false) => (Square::C8, Square::A8, Square::D8),
            };
            kto = kto_actual;
            occupied = occupied ^ Bitboard::square_bb(rfrom); // remove rook from origin
            occupied = occupied | Bitboard::square_bb(rto); // add rook at destination
        } else if m.move_type() == MoveType::EnPassant {
            // Remove the captured pawn (the e.p. target)
            let capsq = match us {
                Color::White => Square::from_index(to as i8 - 8),
                Color::Black => Square::from_index(to as i8 + 8),
            };
            occupied = occupied & !Bitboard::square_bb(capsq);
        } else if self.piece_on(to) != NO_PIECE {
            // Regular capture: the captured piece at `to` will be removed
            // by the blast (to is always blasted), so no explicit removal needed.
        }

        // Add the moving piece at its destination
        occupied = occupied | Bitboard::square_bb(kto);

        // Apply blast on capture
        // Castling encodes the rook's square as `to`, not a capture target.
        let is_capture = m.move_type() != MoveType::Castling
            && (m.move_type() == MoveType::EnPassant || self.piece_on(to) != NO_PIECE);

        if is_capture {
            // Use PRE-MOVE board state for blast computation (matching reference).
            // Adjacent blast removes non-pawn pieces within king-attacks of kto.
            // Ground zero (kto) is always removed (Bug #1 fix).
            let pre_pawns = self.by_type[PieceType::Pawn as usize];
            let pre_non_pawns = self.pieces() ^ pre_pawns;
            let blast_adjacent = attacks::king_attacks(kto) & pre_non_pawns;
            occupied = occupied & !(blast_adjacent | Bitboard::square_bb(kto));
        }

        // Self-explosion check: ensure at least one own commoner survived.
        // Note: self.commoners(us) is the PRE-MOVE position. Our commoner
        // might have moved to `kto` (non-capture) or could have been destroyed
        // by blast (capture). We add the moved commoner if it survived.
        let mut our_commoners = self.commoners(us) & occupied;

        // If the moving piece was a commoner and the move is not a capture
        // (no blast → piece survives at kto), add it at its new location.
        if m.move_type() == MoveType::Castling {
            // Castling: the king moved to kto (no blast)
            our_commoners = our_commoners | Bitboard::square_bb(kto);
        } else if !is_capture {
            let moving_piece = self.piece_on(from);
            if moving_piece != NO_PIECE && moving_piece.type_of() == PieceType::Commoner {
                // Non-capture: the commoner survived at kto (no blast destruction)
                our_commoners = our_commoners | Bitboard::square_bb(kto);
            }
        }
        // For captures: the piece at kto is ALWAYS destroyed by the blast
        // (Bug #1 fix), so the moved commoner does NOT survive at kto.

        if our_commoners.is_empty() {
            return false;
        }

        // Extinction pseudo-royal: in atomic chess, only the last 1 commoner
        // per side is "pseudo-royal" (extinctionPieceCount=0, threshold=1).
        // Commoners beyond the threshold are not protected from attacks.
        //
        // Reference (Fairy-Stockfish position.cpp lines 1156-1188):
        //   pseudoRoyals = st->pseudoRoyals & pieces(sideToMove);
        //   // Computed at state-setup: only pieces with count <= threshold+1
        //   if (!(pseudoRoyalsTheirs & ~occupied)) // skip if enemy PR destroyed
        //       while (pseudoRoyals)              // for each own PR
        //           // check adjacency immunity + attackers
        let our_pr_count = self.commoners(us).count();
        let them_pr_count = self.commoners(them).count();

        // Only check attacks if we have ≤1 commoner (pseudo-royal threshold).
        if our_pr_count <= 1 {
            // Skip the attack check entirely if we destroyed the enemy's last
            // pseudo-royal commoner (winning move — no need to verify safety).
            let enemy_pr_destroyed =
                them_pr_count <= 1 && (self.commoners(them) & occupied).is_empty();

            if !enemy_pr_destroyed {
                // Use PRE-BLAST enemy commoner positions for adjacency immunity
                // (Bug #2 fix).
                let them_commoners = self.commoners(them);
                // Filter opponent pieces to those that survived the blast
                let enemy_survivors = self.by_color[them as usize] & occupied;

                let mut c = our_commoners;
                while !c.is_empty() {
                    let ksq = c.pop_lsb();

                    // Blast adjacency immunity: if an enemy commoner is adjacent
                    // (pre-blast, even if destroyed by blast), this commoner is
                    // immune to being "in check" (mutual destruction).
                    let adjacent_enemy = them_commoners & attacks::king_attacks(ksq);
                    if adjacent_enemy.is_empty() {
                        // No adjacent enemy commoner — check attackers normally.
                        // Use post-blast occupied for blocking and filter opponent
                        // pieces to those that survived the blast.
                        let rook_attackers = attacks::rook_attacks(ksq, occupied)
                            & self.by_type[PieceType::Rook as usize]
                            & enemy_survivors;
                        let bishop_attackers = attacks::bishop_attacks(ksq, occupied)
                            & self.by_type[PieceType::Bishop as usize]
                            & enemy_survivors;
                        let queen_attackers = attacks::queen_attacks(ksq, occupied)
                            & self.by_type[PieceType::Queen as usize]
                            & enemy_survivors;
                        let knight_attackers = attacks::knight_attacks(ksq)
                            & self.by_type[PieceType::Knight as usize]
                            & enemy_survivors;
                        let pawn_attackers = attacks::pawn_attacks(us, ksq)
                            & self.by_type[PieceType::Pawn as usize]
                            & enemy_survivors;
                        if (rook_attackers
                            | bishop_attackers
                            | queen_attackers
                            | knight_attackers
                            | pawn_attackers)
                            != Bitboard::EMPTY
                        {
                            return false;
                        }
                    }
                }
            }
        }

        true
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        for rank in (0..8).rev() {
            write!(f, "{} ", rank + 1)?;
            for file in 0..8 {
                let _idx = rank * 8 + file;
                let _sq = match file {
                    0 => Square::A1,
                    1 => Square::B1,
                    2 => Square::C1,
                    3 => Square::D1,
                    4 => Square::E1,
                    5 => Square::F1,
                    6 => Square::G1,
                    7 => Square::H1,
                    _ => unreachable!(),
                };
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
                    let piece = self.squares[wrapped_idx];
                    let c = match piece.color() {
                        Color::White => 'W',
                        Color::Black => 'B',
                    };
                    let t = match piece.type_of() {
                        PieceType::Pawn => 'P',
                        PieceType::Knight => 'N',
                        PieceType::Bishop => 'B',
                        PieceType::Rook => 'R',
                        PieceType::Queen => 'Q',
                        PieceType::Commoner => 'C',
                    };
                    write!(f, " {}", if c == 'W' { t } else { t.to_ascii_lowercase() })?;
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

fn parse_square(s: &str) -> Square {
    if s.len() < 2 {
        return Square::A1;
    }
    let file = match s.chars().nth(0).unwrap() {
        'a' => 0,
        'b' => 1,
        'c' => 2,
        'd' => 3,
        'e' => 4,
        'f' => 5,
        'g' => 6,
        'h' => 7,
        _ => 0,
    };
    let rank = match s.chars().nth(1).unwrap() {
        '1' => 0,
        '2' => 1,
        '3' => 2,
        '4' => 3,
        '5' => 4,
        '6' => 5,
        '7' => 6,
        '8' => 7,
        _ => 0,
    };
    make_square(
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
    )
}

fn square_str(sq: Square) -> String {
    let f = match file_of(sq) {
        File::A => 'a',
        File::B => 'b',
        File::C => 'c',
        File::D => 'd',
        File::E => 'e',
        File::F => 'f',
        File::G => 'g',
        File::H => 'h',
    };
    let r = match rank_of(sq) {
        Rank::R1 => '1',
        Rank::R2 => '2',
        Rank::R3 => '3',
        Rank::R4 => '4',
        Rank::R5 => '5',
        Rank::R6 => '6',
        Rank::R7 => '7',
        Rank::R8 => '8',
    };
    format!("{}{}", f, r)
}

// Helper to allow Square from index
impl Square {
    pub fn from_index(idx: i8) -> Square {
        static SQUARES: [Square; 64] = [
            Square::A1,
            Square::B1,
            Square::C1,
            Square::D1,
            Square::E1,
            Square::F1,
            Square::G1,
            Square::H1,
            Square::A2,
            Square::B2,
            Square::C2,
            Square::D2,
            Square::E2,
            Square::F2,
            Square::G2,
            Square::H2,
            Square::A3,
            Square::B3,
            Square::C3,
            Square::D3,
            Square::E3,
            Square::F3,
            Square::G3,
            Square::H3,
            Square::A4,
            Square::B4,
            Square::C4,
            Square::D4,
            Square::E4,
            Square::F4,
            Square::G4,
            Square::H4,
            Square::A5,
            Square::B5,
            Square::C5,
            Square::D5,
            Square::E5,
            Square::F5,
            Square::G5,
            Square::H5,
            Square::A6,
            Square::B6,
            Square::C6,
            Square::D6,
            Square::E6,
            Square::F6,
            Square::G6,
            Square::H6,
            Square::A7,
            Square::B7,
            Square::C7,
            Square::D7,
            Square::E7,
            Square::F7,
            Square::G7,
            Square::H7,
            Square::A8,
            Square::B8,
            Square::C8,
            Square::D8,
            Square::E8,
            Square::F8,
            Square::G8,
            Square::H8,
        ];
        if idx >= 0 && idx < 64 {
            SQUARES[idx as usize]
        } else {
            Square::NONE
        }
    }

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
        let mut moves = Vec::new();
        crate::movegen::generate_legal(&board, &mut moves);
        // The capture should be LEGAL because e1 survives and is not under attack.
        let has_rook_d4 = moves
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
        let mut moves = Vec::new();
        crate::movegen::generate_legal(&board, &mut moves);
        for &m in &moves {
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
        let mut moves = Vec::new();
        crate::movegen::generate_legal(&board, &mut moves);
        let has_rook_e4 = moves
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
