//! Board representation for Gomoku

pub mod bitboard;
pub mod board;

#[cfg(test)]
mod tests;

// Re-exports
pub use bitboard::Bitboard;
pub use board::Board;

/// Board size (19x19)
pub const BOARD_SIZE: usize = 19;
pub const TOTAL_CELLS: usize = BOARD_SIZE * BOARD_SIZE; // 361

/// Stone colors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stone {
    Empty,
    Black,
    White,
}

impl Stone {
    /// Get opponent color
    #[inline]
    pub fn opponent(self) -> Stone {
        match self {
            Stone::Black => Stone::White,
            Stone::White => Stone::Black,
            Stone::Empty => Stone::Empty,
        }
    }
}

/// Position on the board
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pos {
    pub row: u8,
    pub col: u8,
}

impl Pos {
    #[inline]
    pub fn new(row: u8, col: u8) -> Self {
        debug_assert!(row < BOARD_SIZE as u8 && col < BOARD_SIZE as u8);
        Self { row, col }
    }

    #[inline]
    pub fn to_index(self) -> usize {
        self.row as usize * BOARD_SIZE + self.col as usize
    }

    #[inline]
    pub fn from_index(idx: usize) -> Self {
        Self {
            row: (idx / BOARD_SIZE) as u8,
            col: (idx % BOARD_SIZE) as u8,
        }
    }

    #[inline]
    pub fn is_valid(row: i32, col: i32) -> bool {
        row >= 0 && row < BOARD_SIZE as i32 && col >= 0 && col < BOARD_SIZE as i32
    }
}

impl PartialOrd for Pos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_index().cmp(&other.to_index())
    }
}
