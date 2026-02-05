//! Bitboard implementation for fast pattern matching

use super::{TOTAL_CELLS, Pos};

/// Bitboard representation for fast pattern matching
/// Uses 6 x u64 to represent 361 cells (6 * 64 = 384 >= 361)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bitboard {
    bits: [u64; 6],
}

impl Bitboard {
    /// Create empty bitboard
    pub const fn new() -> Self {
        Self { bits: [0; 6] }
    }

    /// Set a bit at position
    #[inline]
    pub fn set(&mut self, pos: Pos) {
        let idx = pos.to_index();
        let word = idx / 64;
        let bit = idx % 64;
        self.bits[word] |= 1u64 << bit;
    }

    /// Clear a bit at position
    #[inline]
    pub fn clear(&mut self, pos: Pos) {
        let idx = pos.to_index();
        let word = idx / 64;
        let bit = idx % 64;
        self.bits[word] &= !(1u64 << bit);
    }

    /// Check if bit is set at position
    #[inline]
    pub fn get(&self, pos: Pos) -> bool {
        let idx = pos.to_index();
        let word = idx / 64;
        let bit = idx % 64;
        (self.bits[word] >> bit) & 1 == 1
    }

    /// Count total set bits (popcount)
    #[inline]
    pub fn count(&self) -> u32 {
        self.bits.iter().map(|b| b.count_ones()).sum()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&b| b == 0)
    }

    /// Iterate over set bit positions
    pub fn iter_ones(&self) -> BitboardIter {
        BitboardIter {
            bits: self.bits,
            word_idx: 0,
            current_word: self.bits[0],
        }
    }
}

/// Iterator over set bits in a Bitboard
pub struct BitboardIter {
    bits: [u64; 6],
    word_idx: usize,
    current_word: u64,
}

impl Iterator for BitboardIter {
    type Item = Pos;

    fn next(&mut self) -> Option<Self::Item> {
        // Find next set bit
        while self.current_word == 0 {
            self.word_idx += 1;
            if self.word_idx >= 6 {
                return None;
            }
            self.current_word = self.bits[self.word_idx];
        }

        // Get position of lowest set bit
        let bit_pos = self.current_word.trailing_zeros() as usize;
        let idx = self.word_idx * 64 + bit_pos;

        // Clear the bit we just found
        self.current_word &= self.current_word - 1;

        // Check if valid board position (361 cells, not 384)
        if idx < TOTAL_CELLS {
            Some(Pos::from_index(idx))
        } else {
            None
        }
    }
}
