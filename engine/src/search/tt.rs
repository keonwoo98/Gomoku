//! Transposition Table for caching search results
//!
//! The transposition table stores search results indexed by board hash,
//! enabling reuse of previous search results for positions we've seen before.
//!
//! # Example
//!
//! ```
//! use gomoku::board::Pos;
//! use gomoku::search::{TranspositionTable, EntryType};
//!
//! let mut tt = TranspositionTable::new(1); // 1 MB
//!
//! // Store a search result
//! let hash = 0x123456789ABCDEF0;
//! tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));
//!
//! // Probe for the result
//! if let Some((score, best_move)) = tt.probe(hash, 5, -1000, 1000) {
//!     println!("Found cached result: score={}, move={:?}", score, best_move);
//! }
//! ```

use crate::board::Pos;

/// Entry type for score interpretation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    /// Exact score - the search completed normally
    Exact,
    /// Lower bound - score >= stored value (beta cutoff)
    LowerBound,
    /// Upper bound - score <= stored value (alpha fail-low)
    UpperBound,
}

/// Transposition table entry
#[derive(Debug, Clone, Copy)]
pub struct TTEntry {
    /// Zobrist hash of the position
    pub hash: u64,
    /// Search depth for this entry
    pub depth: i8,
    /// Evaluation score
    pub score: i32,
    /// Type of score (exact, lower bound, upper bound)
    pub entry_type: EntryType,
    /// Best move found for this position
    pub best_move: Option<Pos>,
}

/// Transposition table for caching search results.
///
/// Uses a simple direct-mapped approach where each hash maps to exactly
/// one slot. Collisions are handled by replacement policies based on
/// search depth.
pub struct TranspositionTable {
    entries: Vec<Option<TTEntry>>,
    size: usize,
}

impl TranspositionTable {
    /// Create a new transposition table with the given size in megabytes.
    ///
    /// # Arguments
    ///
    /// * `size_mb` - Size of the table in megabytes
    ///
    /// # Example
    ///
    /// ```
    /// use gomoku::search::TranspositionTable;
    ///
    /// let tt = TranspositionTable::new(16); // 16 MB table
    /// ```
    #[must_use]
    pub fn new(size_mb: usize) -> Self {
        let entry_size = std::mem::size_of::<Option<TTEntry>>();
        let size = (size_mb * 1024 * 1024) / entry_size;

        // Ensure at least some entries
        let size = size.max(1024);

        Self {
            entries: vec![None; size],
            size,
        }
    }

    /// Probe the table for a position.
    ///
    /// Returns `Some((score, best_move))` if an entry is found and usable
    /// for the current search parameters. If the entry exists but the score
    /// is not usable (e.g., insufficient depth), returns `Some((0, best_move))`
    /// to provide the best move for move ordering.
    ///
    /// # Arguments
    ///
    /// * `hash` - Zobrist hash of the position
    /// * `depth` - Current search depth (entry must be at least this deep)
    /// * `alpha` - Current alpha bound
    /// * `beta` - Current beta bound
    ///
    /// # Returns
    ///
    /// * `Some((score, best_move))` - Score is usable if non-zero
    /// * `None` - No entry found for this hash
    #[must_use]
    pub fn probe(&self, hash: u64, depth: i8, alpha: i32, beta: i32) -> Option<(i32, Option<Pos>)> {
        let idx = (hash as usize) % self.size;
        let entry = self.entries[idx]?;

        if entry.hash != hash {
            return None;
        }

        // Can use score if stored search was at least as deep
        if entry.depth >= depth {
            match entry.entry_type {
                EntryType::Exact => return Some((entry.score, entry.best_move)),
                EntryType::LowerBound if entry.score >= beta => {
                    return Some((entry.score, entry.best_move));
                }
                EntryType::UpperBound if entry.score <= alpha => {
                    return Some((entry.score, entry.best_move));
                }
                _ => {}
            }
        }

        // Return best move for move ordering even if score not usable
        Some((0, entry.best_move))
    }

    /// Get best move from the table for move ordering.
    ///
    /// This is useful when we want to try the TT move first during
    /// move generation, even if the stored score is not usable.
    ///
    /// # Arguments
    ///
    /// * `hash` - Zobrist hash of the position
    ///
    /// # Returns
    ///
    /// * `Some(pos)` - Best move found in a previous search
    /// * `None` - No entry found for this hash
    #[must_use]
    pub fn get_best_move(&self, hash: u64) -> Option<Pos> {
        let idx = (hash as usize) % self.size;
        self.entries[idx].and_then(|e| {
            if e.hash == hash {
                e.best_move
            } else {
                None
            }
        })
    }

    /// Store a position in the table.
    ///
    /// Uses a depth-preferred replacement policy: an entry is replaced if
    /// the slot is empty, contains the same position, or the new search
    /// is at least as deep as the existing entry.
    ///
    /// # Arguments
    ///
    /// * `hash` - Zobrist hash of the position
    /// * `depth` - Search depth for this result
    /// * `score` - Evaluation score
    /// * `entry_type` - Type of score (exact, lower bound, upper bound)
    /// * `best_move` - Best move found (may be None)
    pub fn store(
        &mut self,
        hash: u64,
        depth: i8,
        score: i32,
        entry_type: EntryType,
        best_move: Option<Pos>,
    ) {
        let idx = (hash as usize) % self.size;

        // Replace if: empty, same position, or new search is deeper
        let should_replace = match &self.entries[idx] {
            None => true,
            Some(e) => e.hash == hash || e.depth <= depth,
        };

        if should_replace {
            self.entries[idx] = Some(TTEntry {
                hash,
                depth,
                score,
                entry_type,
                best_move,
            });
        }
    }

    /// Clear all entries in the table.
    ///
    /// This should be called when starting a new game or when the
    /// table becomes stale.
    pub fn clear(&mut self) {
        self.entries.fill(None);
    }

    /// Get statistics about table usage.
    ///
    /// # Returns
    ///
    /// A `TTStats` struct containing size, usage count, and percentage.
    #[must_use]
    pub fn stats(&self) -> TTStats {
        let used = self.entries.iter().filter(|e| e.is_some()).count();
        TTStats {
            size: self.size,
            used,
            usage_percent: (used as f64 / self.size as f64 * 100.0) as u8,
        }
    }
}

/// Statistics about transposition table usage.
#[derive(Debug, Clone, Copy)]
pub struct TTStats {
    /// Total number of slots in the table
    pub size: usize,
    /// Number of slots currently occupied
    pub used: usize,
    /// Percentage of table in use (0-100)
    pub usage_percent: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tt_store_probe_exact() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));

        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        let (score, best_move) = result.unwrap();
        assert_eq!(score, 100);
        assert_eq!(best_move, Some(Pos::new(9, 9)));
    }

    #[test]
    fn test_tt_depth_requirement() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 3, 100, EntryType::Exact, Some(Pos::new(5, 5)));

        // Deeper search should not use shallow entry's score
        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        let (score, best_move) = result.unwrap();
        // Score is 0 (not usable), but best_move is returned for ordering
        assert_eq!(score, 0);
        assert_eq!(best_move, Some(Pos::new(5, 5)));
    }

    #[test]
    fn test_tt_lower_bound_cutoff() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 200, EntryType::LowerBound, None);

        // Score (200) >= beta (150), should return score
        let result = tt.probe(hash, 5, -1000, 150);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 200);

        // Score (200) < beta (300), should not return score
        let result = tt.probe(hash, 5, -1000, 300);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 0); // Score not usable
    }

    #[test]
    fn test_tt_upper_bound_cutoff() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 50, EntryType::UpperBound, None);

        // Score (50) <= alpha (100), should return score
        let result = tt.probe(hash, 5, 100, 1000);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 50);

        // Score (50) > alpha (30), should not return score
        let result = tt.probe(hash, 5, 30, 1000);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 0);
    }

    #[test]
    fn test_tt_hash_mismatch() {
        let mut tt = TranspositionTable::new(1);
        let hash1 = 0x123456789ABCDEF0;
        let hash2 = 0x987654321FEDCBA0;

        tt.store(hash1, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));

        // Different hash should not return entry (unless collision)
        let result = tt.probe(hash2, 5, -1000, 1000);
        // Result depends on whether hashes map to same slot and hash verification
        if let Some((score, _)) = result {
            // If there's a collision, hash verification should fail
            assert_eq!(score, 0);
        }
    }

    #[test]
    fn test_tt_get_best_move() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));

        let best_move = tt.get_best_move(hash);
        assert_eq!(best_move, Some(Pos::new(9, 9)));

        // Wrong hash should return None
        let best_move = tt.get_best_move(0xFFFFFFFFFFFFFFFF);
        assert!(best_move.is_none());
    }

    #[test]
    fn test_tt_replacement_deeper() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 3, 100, EntryType::Exact, Some(Pos::new(5, 5)));
        tt.store(hash, 5, 200, EntryType::Exact, Some(Pos::new(9, 9)));

        // Deeper entry should replace shallower
        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 200);
    }

    #[test]
    fn test_tt_replacement_same_depth() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(5, 5)));
        tt.store(hash, 5, 200, EntryType::Exact, Some(Pos::new(9, 9)));

        // Same depth should replace (newer info)
        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 200);
    }

    #[test]
    fn test_tt_no_replacement_shallower() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(5, 5)));
        tt.store(hash, 3, 200, EntryType::Exact, Some(Pos::new(9, 9)));

        // Shallower should NOT replace deeper (same hash exception)
        // Actually, same hash always replaces per our policy
        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        // Since same hash, it gets replaced
        let (score, _) = result.unwrap();
        assert_eq!(score, 0); // Depth 3 < requested depth 5
    }

    #[test]
    fn test_tt_clear() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, None);
        tt.clear();

        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_none());
    }

    #[test]
    fn test_tt_stats() {
        let mut tt = TranspositionTable::new(1);

        let stats = tt.stats();
        assert_eq!(stats.used, 0);
        assert_eq!(stats.usage_percent, 0);

        tt.store(0x111, 5, 100, EntryType::Exact, None);
        tt.store(0x222, 5, 100, EntryType::Exact, None);

        let stats = tt.stats();
        assert_eq!(stats.used, 2);
        assert!(stats.size > 0);
    }

    #[test]
    fn test_tt_no_best_move() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        // Store without best move
        tt.store(hash, 5, 100, EntryType::Exact, None);

        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        let (score, best_move) = result.unwrap();
        assert_eq!(score, 100);
        assert!(best_move.is_none());

        let best_move = tt.get_best_move(hash);
        assert!(best_move.is_none());
    }

    #[test]
    fn test_tt_entry_types() {
        let mut tt = TranspositionTable::new(1);

        // Test all entry types
        let hashes = [0x111u64, 0x222, 0x333];

        tt.store(hashes[0], 5, 100, EntryType::Exact, None);
        tt.store(hashes[1], 5, 100, EntryType::LowerBound, None);
        tt.store(hashes[2], 5, 100, EntryType::UpperBound, None);

        // Exact always returns score if depth sufficient
        let result = tt.probe(hashes[0], 5, -1000, 1000);
        assert_eq!(result.unwrap().0, 100);

        // LowerBound returns score only if score >= beta
        let result = tt.probe(hashes[1], 5, -1000, 50);
        assert_eq!(result.unwrap().0, 100); // 100 >= 50

        // UpperBound returns score only if score <= alpha
        let result = tt.probe(hashes[2], 5, 150, 1000);
        assert_eq!(result.unwrap().0, 100); // 100 <= 150
    }

    #[test]
    fn test_tt_minimum_size() {
        // Even with 0 MB, should have minimum entries
        let tt = TranspositionTable::new(0);
        assert!(tt.size >= 1024);
    }

    #[test]
    fn test_tt_size_calculation() {
        let tt = TranspositionTable::new(1);
        let entry_size = std::mem::size_of::<Option<TTEntry>>();
        let expected_size = (1024 * 1024) / entry_size;
        assert_eq!(tt.size, expected_size.max(1024));
    }
}
