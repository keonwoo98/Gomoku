//! Zobrist hashing for position identification
//!
//! Zobrist hashing allows O(1) incremental hash updates when placing/removing stones.
//! This is essential for efficient transposition table lookups during search.
//!
//! # Example
//!
//! ```
//! use gomoku::board::{Board, Stone, Pos};
//! use gomoku::search::ZobristTable;
//!
//! let zt = ZobristTable::new();
//! let mut board = Board::new();
//!
//! // Compute initial hash
//! let hash1 = zt.hash(&board, Stone::Black);
//!
//! // Place a stone and compute new hash
//! let pos = Pos::new(9, 9);
//! board.place_stone(pos, Stone::Black);
//! let hash2 = zt.hash(&board, Stone::White);
//!
//! // Incremental update is equivalent to full recomputation
//! let hash_incremental = zt.update_place(hash1, pos, Stone::Black);
//! assert_eq!(hash_incremental, hash2);
//! ```

use crate::board::{Board, Pos, Stone, TOTAL_CELLS};

/// Zobrist hash table for position hashing.
///
/// Uses XOR-based hashing with precomputed random values for each
/// (position, stone color) combination. This allows O(1) incremental
/// updates when placing or removing stones.
pub struct ZobristTable {
    /// Random values for black stones at each position
    black: [u64; TOTAL_CELLS],
    /// Random values for white stones at each position
    white: [u64; TOTAL_CELLS],
    /// Random value XORed when black is to move
    black_to_move: u64,
    /// Random values for capture counts: [color][count 0..6]
    captures: [[u64; 6]; 2],
}

impl ZobristTable {
    /// Create a new Zobrist table with deterministic random values.
    ///
    /// Uses a linear congruential generator (LCG) with a fixed seed
    /// to ensure reproducible hashes across different runs.
    #[must_use]
    pub fn new() -> Self {
        // Use a simple LCG for deterministic "random" values
        // Same seed = same table = reproducible hashes
        // Constants from Knuth's MMIX LCG
        let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next_rand = || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            seed
        };

        let mut black = [0u64; TOTAL_CELLS];
        let mut white = [0u64; TOTAL_CELLS];

        for i in 0..TOTAL_CELLS {
            black[i] = next_rand();
            white[i] = next_rand();
        }

        let mut captures = [[0u64; 6]; 2];
        for color in 0..2 {
            for count in 0..6 {
                captures[color][count] = next_rand();
            }
        }

        Self {
            black,
            white,
            black_to_move: next_rand(),
            captures,
        }
    }

    /// Compute the full hash for a board position.
    ///
    /// This iterates over all stones on the board. For incremental updates
    /// during search, use `update_place` and `update_remove` instead.
    #[must_use]
    pub fn hash(&self, board: &Board, side_to_move: Stone) -> u64 {
        let mut h = 0u64;

        for pos in board.black.iter_ones() {
            h ^= self.black[pos.to_index()];
        }

        for pos in board.white.iter_ones() {
            h ^= self.white[pos.to_index()];
        }

        if side_to_move == Stone::Black {
            h ^= self.black_to_move;
        }

        // Include capture counts in hash to distinguish positions with same stones
        // but different capture counts (affects win conditions)
        h ^= self.captures[0][board.captures(Stone::Black).min(5) as usize];
        h ^= self.captures[1][board.captures(Stone::White).min(5) as usize];

        h
    }

    /// Incrementally update hash after placing a stone.
    ///
    /// This is O(1) and should be used during search instead of
    /// recomputing the full hash.
    ///
    /// Note: This also toggles the side-to-move component.
    #[inline]
    #[must_use]
    pub fn update_place(&self, hash: u64, pos: Pos, stone: Stone) -> u64 {
        let idx = pos.to_index();
        let stone_hash = match stone {
            Stone::Black => self.black[idx],
            Stone::White => self.white[idx],
            Stone::Empty => 0,
        };
        hash ^ stone_hash ^ self.black_to_move
    }

    /// Incrementally update hash after removing a stone.
    ///
    /// XOR is its own inverse, so this is identical to `update_place`.
    /// Provided for semantic clarity in code.
    ///
    /// Note: This also toggles the side-to-move component.
    #[inline]
    #[must_use]
    pub fn update_remove(&self, hash: u64, pos: Pos, stone: Stone) -> u64 {
        // XOR is its own inverse: a ^ b ^ b = a
        self.update_place(hash, pos, stone)
    }

    /// Update hash for a capture (removing opponent stones without toggling side).
    ///
    /// Use this when processing captures, as the side-to-move doesn't change
    /// during capture processing.
    #[inline]
    #[must_use]
    pub fn update_capture(&self, hash: u64, pos: Pos, stone: Stone) -> u64 {
        let idx = pos.to_index();
        let stone_hash = match stone {
            Stone::Black => self.black[idx],
            Stone::White => self.white[idx],
            Stone::Empty => 0,
        };
        hash ^ stone_hash
    }

    /// Toggle the side-to-move component of the hash.
    ///
    /// Used for null move pruning where the side changes without placing a stone.
    #[inline]
    #[must_use]
    pub fn toggle_side(&self, hash: u64) -> u64 {
        hash ^ self.black_to_move
    }

    /// Update hash when capture count changes for a color.
    ///
    /// XORs out the old capture count hash and XORs in the new one.
    /// Call this after `execute_captures_fast` updates the board's capture count.
    #[inline]
    #[must_use]
    pub fn update_capture_count(&self, hash: u64, color: Stone, old_count: u8, new_count: u8) -> u64 {
        let cidx = if color == Stone::Black { 0 } else { 1 };
        hash ^ self.captures[cidx][old_count.min(5) as usize]
             ^ self.captures[cidx][new_count.min(5) as usize]
    }
}

impl Default for ZobristTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zobrist_empty_board() {
        let zt = ZobristTable::new();
        let board = Board::new();

        let hash1 = zt.hash(&board, Stone::Black);
        let hash2 = zt.hash(&board, Stone::White);

        // Different side to move = different hash
        assert_ne!(hash1, hash2);
        // Empty board includes capture hash for (0,0) captures
        let cap_base = zt.captures[0][0] ^ zt.captures[1][0];
        assert_eq!(hash2, cap_base);
        assert_eq!(hash1, zt.black_to_move ^ cap_base);
    }

    #[test]
    fn test_zobrist_deterministic() {
        let zt1 = ZobristTable::new();
        let zt2 = ZobristTable::new();
        let board = Board::new();

        // Same table = same hash (deterministic random values)
        assert_eq!(
            zt1.hash(&board, Stone::Black),
            zt2.hash(&board, Stone::Black)
        );
    }

    #[test]
    fn test_zobrist_incremental() {
        let zt = ZobristTable::new();
        let mut board = Board::new();
        let pos = Pos::new(9, 9);

        let hash1 = zt.hash(&board, Stone::Black);
        board.place_stone(pos, Stone::Black);
        let hash2 = zt.hash(&board, Stone::White);

        // Incremental should match full computation
        let hash_inc = zt.update_place(hash1, pos, Stone::Black);
        assert_eq!(hash_inc, hash2);
    }

    #[test]
    fn test_zobrist_different_positions() {
        let zt = ZobristTable::new();
        let mut board1 = Board::new();
        let mut board2 = Board::new();

        board1.place_stone(Pos::new(9, 9), Stone::Black);
        board2.place_stone(Pos::new(10, 10), Stone::Black);

        let hash1 = zt.hash(&board1, Stone::White);
        let hash2 = zt.hash(&board2, Stone::White);

        // Different positions = different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_zobrist_same_position_different_path() {
        let zt = ZobristTable::new();
        let mut board1 = Board::new();
        let mut board2 = Board::new();

        // Path 1: Black at (9,9), then White at (10,10)
        board1.place_stone(Pos::new(9, 9), Stone::Black);
        board1.place_stone(Pos::new(10, 10), Stone::White);

        // Path 2: White at (10,10), then Black at (9,9)
        board2.place_stone(Pos::new(10, 10), Stone::White);
        board2.place_stone(Pos::new(9, 9), Stone::Black);

        // Same final position = same hash (path independent)
        let hash1 = zt.hash(&board1, Stone::Black);
        let hash2 = zt.hash(&board2, Stone::Black);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_zobrist_undo() {
        let zt = ZobristTable::new();
        let mut board = Board::new();
        let pos = Pos::new(9, 9);

        let hash_empty = zt.hash(&board, Stone::Black);

        board.place_stone(pos, Stone::Black);
        let hash_with_stone = zt.hash(&board, Stone::White);

        // Incremental update for place
        let hash_inc = zt.update_place(hash_empty, pos, Stone::Black);
        assert_eq!(hash_inc, hash_with_stone);

        // Incremental update for remove (should get back to original with side toggle)
        board.remove_stone(pos);
        let hash_after_remove = zt.hash(&board, Stone::Black);

        // Removing stone and toggling side should get back to original
        let hash_inc_remove = zt.update_remove(hash_with_stone, pos, Stone::Black);
        assert_eq!(hash_inc_remove, hash_after_remove);
        assert_eq!(hash_after_remove, hash_empty);
    }

    #[test]
    fn test_zobrist_capture_no_side_toggle() {
        let zt = ZobristTable::new();
        let mut board = Board::new();

        // Place some stones
        board.place_stone(Pos::new(5, 5), Stone::Black);
        board.place_stone(Pos::new(5, 6), Stone::White);
        board.place_stone(Pos::new(5, 7), Stone::White);

        let hash_before = zt.hash(&board, Stone::Black);

        // Simulate capture: remove white stones without toggling side
        let hash_after_cap1 = zt.update_capture(hash_before, Pos::new(5, 6), Stone::White);
        let hash_after_cap2 = zt.update_capture(hash_after_cap1, Pos::new(5, 7), Stone::White);

        // Verify by computing full hash after removing stones
        board.remove_stone(Pos::new(5, 6));
        board.remove_stone(Pos::new(5, 7));
        let hash_full = zt.hash(&board, Stone::Black);

        assert_eq!(hash_after_cap2, hash_full);
    }

    #[test]
    fn test_zobrist_symmetry() {
        let zt = ZobristTable::new();

        // XOR is commutative: order of operations shouldn't matter
        let h1 = zt.black[0] ^ zt.white[1] ^ zt.black[2];
        let h2 = zt.black[2] ^ zt.black[0] ^ zt.white[1];
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_zobrist_collision_resistance() {
        let zt = ZobristTable::new();

        // Test that nearby positions have different hashes
        let mut board1 = Board::new();
        let mut board2 = Board::new();

        board1.place_stone(Pos::new(9, 9), Stone::Black);
        board2.place_stone(Pos::new(9, 10), Stone::Black);

        let hash1 = zt.hash(&board1, Stone::Black);
        let hash2 = zt.hash(&board2, Stone::Black);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_zobrist_all_corners() {
        let zt = ZobristTable::new();
        let mut board = Board::new();

        // Place stones at all four corners
        let corners = [
            Pos::new(0, 0),
            Pos::new(0, 18),
            Pos::new(18, 0),
            Pos::new(18, 18),
        ];

        for &pos in &corners {
            board.place_stone(pos, Stone::Black);
        }

        let hash = zt.hash(&board, Stone::White);

        // Hash should be XOR of all four corner values + capture hashes for (0,0)
        let expected = zt.black[corners[0].to_index()]
            ^ zt.black[corners[1].to_index()]
            ^ zt.black[corners[2].to_index()]
            ^ zt.black[corners[3].to_index()]
            ^ zt.captures[0][0]
            ^ zt.captures[1][0];

        assert_eq!(hash, expected);
    }
}
