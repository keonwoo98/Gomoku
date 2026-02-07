//! Board structure with capture tracking

use super::bitboard::Bitboard;
use super::{Pos, Stone, BOARD_SIZE};

/// Game board with capture tracking
#[derive(Debug, Clone)]
pub struct Board {
    /// Black stones bitboard
    pub black: Bitboard,
    /// White stones bitboard
    pub white: Bitboard,
    /// Number of pairs captured by each side (0-5, 5 = win)
    pub black_captures: u8,
    pub white_captures: u8,
    /// Move history for undo (reserved for future use)
    #[allow(dead_code)]
    history: Vec<MoveRecord>,
}

/// Record of a move for undo functionality (reserved for future use)
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct MoveRecord {
    pos: Pos,
    stone: Stone,
    captured: Vec<Pos>,
}

impl Board {
    pub fn new() -> Self {
        Self {
            black: Bitboard::new(),
            white: Bitboard::new(),
            black_captures: 0,
            white_captures: 0,
            history: Vec::with_capacity(361),
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        BOARD_SIZE
    }

    /// Get stone at position
    #[inline]
    pub fn get(&self, pos: Pos) -> Stone {
        if self.black.get(pos) {
            Stone::Black
        } else if self.white.get(pos) {
            Stone::White
        } else {
            Stone::Empty
        }
    }

    /// Check if position is empty
    #[inline]
    pub fn is_empty(&self, pos: Pos) -> bool {
        !self.black.get(pos) && !self.white.get(pos)
    }

    /// Place a stone (without capture processing)
    /// Use `make_move` for game moves
    #[inline]
    pub fn place_stone(&mut self, pos: Pos, stone: Stone) {
        match stone {
            Stone::Black => self.black.set(pos),
            Stone::White => self.white.set(pos),
            Stone::Empty => {}
        }
    }

    /// Remove a stone
    #[inline]
    pub fn remove_stone(&mut self, pos: Pos) {
        self.black.clear(pos);
        self.white.clear(pos);
    }

    /// Get bitboard for a color (returns None for Empty)
    #[inline]
    pub fn stones(&self, stone: Stone) -> Option<&Bitboard> {
        match stone {
            Stone::Black => Some(&self.black),
            Stone::White => Some(&self.white),
            Stone::Empty => None,
        }
    }

    /// Get mutable bitboard for a color (returns None for Empty)
    #[inline]
    pub fn stones_mut(&mut self, stone: Stone) -> Option<&mut Bitboard> {
        match stone {
            Stone::Black => Some(&mut self.black),
            Stone::White => Some(&mut self.white),
            Stone::Empty => None,
        }
    }

    /// Get capture count for a color
    #[inline]
    pub fn captures(&self, stone: Stone) -> u8 {
        match stone {
            Stone::Black => self.black_captures,
            Stone::White => self.white_captures,
            Stone::Empty => 0,
        }
    }

    /// Add captures for a color (saturating, max 255)
    #[inline]
    pub fn add_captures(&mut self, stone: Stone, count: u8) {
        match stone {
            Stone::Black => self.black_captures = self.black_captures.saturating_add(count),
            Stone::White => self.white_captures = self.white_captures.saturating_add(count),
            Stone::Empty => {}
        }
    }

    /// Subtract captures for a color (saturating, min 0) - used for unmake
    #[inline]
    pub fn sub_captures(&mut self, stone: Stone, count: u8) {
        match stone {
            Stone::Black => self.black_captures = self.black_captures.saturating_sub(count),
            Stone::White => self.white_captures = self.white_captures.saturating_sub(count),
            Stone::Empty => {}
        }
    }

    /// Total stones on board
    #[inline]
    pub fn stone_count(&self) -> u32 {
        self.black.count() + self.white.count()
    }

    /// Check if board is empty
    #[inline]
    pub fn is_board_empty(&self) -> bool {
        self.black.is_empty() && self.white.is_empty()
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}
