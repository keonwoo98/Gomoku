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

use std::sync::atomic::{AtomicU64, Ordering};

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

// =============================================================================
// Lock-free AtomicTT for Lazy SMP parallel search
// =============================================================================

/// Pack a TT entry into a u64 for atomic storage.
///
/// Layout (42 bits used):
/// ```text
/// bits [0..7]   depth (i8 → u8: +128 offset)        8 bits
/// bits [8..28]  score (i32 → u21: +1_048_576)       21 bits
/// bits [29..30] entry_type (0=Exact,1=LB,2=UB)       2 bits
/// bits [31]     has_move (bool)                       1 bit
/// bits [32..36] row (u5, 0-18)                        5 bits
/// bits [37..41] col (u5, 0-18)                        5 bits
/// ```
fn pack_entry(depth: i8, score: i32, entry_type: EntryType, best_move: Option<Pos>) -> u64 {
    let d = (depth as i16 + 128) as u64 & 0xFF;
    // Clamp score to 21-bit range [-1_048_575, 1_048_575] to prevent silent overflow.
    // In practice scores rarely exceed FIVE (1M), but this is cheap insurance.
    let clamped = score.clamp(-1_048_575, 1_048_575);
    let s = (clamped as i64 + 1_048_576) as u64 & 0x1F_FFFF; // 21 bits
    let t = match entry_type {
        EntryType::Exact => 0u64,
        EntryType::LowerBound => 1u64,
        EntryType::UpperBound => 2u64,
    };
    let (has_move, row, col) = match best_move {
        Some(p) => (1u64, p.row as u64, p.col as u64),
        None => (0u64, 0u64, 0u64),
    };
    d | (s << 8) | (t << 29) | (has_move << 31) | (row << 32) | (col << 37)
}

/// Unpack a u64 back into TT entry fields.
fn unpack_entry(data: u64) -> (i8, i32, EntryType, Option<Pos>) {
    let d = (data & 0xFF) as i16 - 128;
    let depth = d as i8;
    let s = ((data >> 8) & 0x1F_FFFF) as i64 - 1_048_576;
    let score = s as i32;
    let t = (data >> 29) & 0x3;
    let entry_type = match t {
        0 => EntryType::Exact,
        1 => EntryType::LowerBound,
        _ => EntryType::UpperBound,
    };
    let has_move = ((data >> 31) & 1) != 0;
    let best_move = if has_move {
        let row = ((data >> 32) & 0x1F) as u8;
        let col = ((data >> 37) & 0x1F) as u8;
        Some(Pos::new(row, col))
    } else {
        None
    };
    (depth, score, entry_type, best_move)
}

/// Lock-free transposition table for Lazy SMP parallel search.
///
/// Uses XOR trick (Hyatt 1994): each slot stores `(key, data)` where
/// `key = hash ^ data`. On probe, validity is checked via `key ^ data == hash`.
/// Torn reads (partial writes from concurrent threads) fail the hash check
/// and are treated as cache misses — safe and lock-free.
///
/// All methods take `&self` (not `&mut self`), enabling `Arc<AtomicTT>` sharing.
pub struct AtomicTT {
    keys: Vec<AtomicU64>,
    data: Vec<AtomicU64>,
    size: usize,
}

// AtomicTT is Send+Sync automatically because all its fields (Vec<AtomicU64>, usize)
// are Send+Sync. No manual unsafe impl needed.

impl AtomicTT {
    /// Create a new atomic transposition table with the given size in megabytes.
    #[must_use]
    pub fn new(size_mb: usize) -> Self {
        // Each slot = 2 x AtomicU64 = 16 bytes
        let slot_size = 16usize;
        let size = ((size_mb * 1024 * 1024) / slot_size).max(1024);

        let mut keys = Vec::with_capacity(size);
        let mut data = Vec::with_capacity(size);
        for _ in 0..size {
            keys.push(AtomicU64::new(0));
            data.push(AtomicU64::new(0));
        }

        Self { keys, data, size }
    }

    /// Probe the table for a position.
    ///
    /// Returns `Some((score, best_move))` if valid entry found.
    /// Score is 0 if entry exists but depth insufficient (best_move still returned).
    #[must_use]
    pub fn probe(&self, hash: u64, depth: i8, alpha: i32, beta: i32) -> Option<(i32, Option<Pos>)> {
        let idx = (hash as usize) % self.size;
        let key = self.keys[idx].load(Ordering::Relaxed);
        let raw_data = self.data[idx].load(Ordering::Relaxed);

        // Empty slot
        if key == 0 && raw_data == 0 {
            return None;
        }

        // XOR verification: torn read → hash mismatch → safe miss
        if key ^ raw_data != hash {
            return None;
        }

        let (entry_depth, score, entry_type, best_move) = unpack_entry(raw_data);

        if entry_depth >= depth {
            match entry_type {
                EntryType::Exact => return Some((score, best_move)),
                EntryType::LowerBound if score >= beta => return Some((score, best_move)),
                EntryType::UpperBound if score <= alpha => return Some((score, best_move)),
                _ => {}
            }
        }

        // Score not usable at this depth/window — callers use get_best_move() for ordering
        None
    }

    /// Get best move from the table for move ordering.
    #[must_use]
    pub fn get_best_move(&self, hash: u64) -> Option<Pos> {
        let idx = (hash as usize) % self.size;
        let key = self.keys[idx].load(Ordering::Relaxed);
        let raw_data = self.data[idx].load(Ordering::Relaxed);

        if key == 0 && raw_data == 0 {
            return None;
        }
        if key ^ raw_data != hash {
            return None;
        }

        let (_depth, _score, _entry_type, best_move) = unpack_entry(raw_data);
        best_move
    }

    /// Store a position in the table (&self — safe for concurrent access).
    ///
    /// Uses depth-preferred replacement: replaces if deeper or same hash.
    /// XOR trick: stores key = hash ^ data so concurrent reads can detect torn writes.
    pub fn store(
        &self,
        hash: u64,
        depth: i8,
        score: i32,
        entry_type: EntryType,
        best_move: Option<Pos>,
    ) {
        let idx = (hash as usize) % self.size;

        // Check replacement policy: replace if empty, same hash, or deeper
        let existing_data = self.data[idx].load(Ordering::Relaxed);
        let existing_key = self.keys[idx].load(Ordering::Relaxed);
        if existing_data != 0 || existing_key != 0 {
            let existing_hash = existing_key ^ existing_data;
            if existing_hash != hash {
                // Different position: only replace if deeper
                let (existing_depth, _, _, _) = unpack_entry(existing_data);
                if depth < existing_depth {
                    return;
                }
            }
        }

        let packed = pack_entry(depth, score, entry_type, best_move);
        let key = hash ^ packed;
        // Write data first, then key. This ordering means a concurrent reader
        // either sees old (key, data) pair or gets a hash mismatch on torn read.
        self.data[idx].store(packed, Ordering::Relaxed);
        self.keys[idx].store(key, Ordering::Relaxed);
    }

    /// Clear all entries (&self — safe for concurrent access).
    pub fn clear(&self) {
        for i in 0..self.size {
            self.keys[i].store(0, Ordering::Relaxed);
            self.data[i].store(0, Ordering::Relaxed);
        }
    }

    /// Get statistics about table usage.
    ///
    /// Note: This is approximate under concurrent access.
    #[must_use]
    pub fn stats(&self) -> TTStats {
        let mut used = 0usize;
        // Sample every 64th entry for speed (approximate is fine for stats)
        let step = if self.size > 65536 { 64 } else { 1 };
        let mut sampled = 0usize;
        let mut i = 0;
        while i < self.size {
            sampled += 1;
            let k = self.keys[i].load(Ordering::Relaxed);
            let d = self.data[i].load(Ordering::Relaxed);
            if k != 0 || d != 0 {
                used += 1;
            }
            i += step;
        }
        let estimated_used = if step > 1 {
            used * self.size / sampled
        } else {
            used
        };
        TTStats {
            size: self.size,
            used: estimated_used,
            usage_percent: (estimated_used as f64 / self.size as f64 * 100.0) as u8,
        }
    }
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

    // =========================================================================
    // AtomicTT tests
    // =========================================================================

    #[test]
    fn test_pack_unpack_roundtrip() {
        let cases: Vec<(i8, i32, EntryType, Option<Pos>)> = vec![
            (5, 100, EntryType::Exact, Some(Pos::new(9, 9))),
            (-3, -500_000, EntryType::LowerBound, None),
            (0, 0, EntryType::UpperBound, Some(Pos::new(0, 0))),
            (15, 999_999, EntryType::Exact, Some(Pos::new(18, 18))),
            (-128, -1_048_575, EntryType::LowerBound, Some(Pos::new(0, 18))),
            (127, 1_048_575, EntryType::UpperBound, Some(Pos::new(18, 0))),
        ];
        for (depth, score, et, bm) in cases {
            let packed = pack_entry(depth, score, et, bm);
            let (d, s, t, m) = unpack_entry(packed);
            assert_eq!(d, depth, "depth mismatch for ({}, {})", depth, score);
            assert_eq!(s, score, "score mismatch for ({}, {})", depth, score);
            assert_eq!(t, et, "type mismatch for ({}, {})", depth, score);
            assert_eq!(m, bm, "move mismatch for ({}, {})", depth, score);
        }
    }

    #[test]
    fn test_atomic_tt_store_probe_exact() {
        let tt = AtomicTT::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));

        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        let (score, best_move) = result.unwrap();
        assert_eq!(score, 100);
        assert_eq!(best_move, Some(Pos::new(9, 9)));
    }

    #[test]
    fn test_atomic_tt_depth_requirement() {
        let tt = AtomicTT::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 3, 100, EntryType::Exact, Some(Pos::new(5, 5)));

        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_none()); // Depth insufficient → None (use get_best_move for ordering)
    }

    #[test]
    fn test_atomic_tt_bounds() {
        let tt = AtomicTT::new(1);

        // LowerBound
        let hash_lb = 0x111;
        tt.store(hash_lb, 5, 200, EntryType::LowerBound, None);
        assert_eq!(tt.probe(hash_lb, 5, -1000, 150).unwrap().0, 200); // 200 >= 150
        assert!(tt.probe(hash_lb, 5, -1000, 300).is_none()); // 200 < 300 → not usable

        // UpperBound
        let hash_ub = 0x222;
        tt.store(hash_ub, 5, 50, EntryType::UpperBound, None);
        assert_eq!(tt.probe(hash_ub, 5, 100, 1000).unwrap().0, 50); // 50 <= 100
        assert!(tt.probe(hash_ub, 5, 30, 1000).is_none()); // 50 > 30 → not usable
    }

    #[test]
    fn test_atomic_tt_hash_mismatch() {
        let tt = AtomicTT::new(1);
        tt.store(0xAABBCCDD_11223344, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));

        // Different hash should return None (XOR check fails)
        let result = tt.probe(0xFFEEDDCC_44332211, 5, -1000, 1000);
        assert!(result.is_none());
    }

    #[test]
    fn test_atomic_tt_get_best_move() {
        let tt = AtomicTT::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));
        assert_eq!(tt.get_best_move(hash), Some(Pos::new(9, 9)));
        assert!(tt.get_best_move(0xFFFF_FFFF_FFFF_FFFF).is_none());
    }

    #[test]
    fn test_atomic_tt_clear() {
        let tt = AtomicTT::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, None);
        tt.clear();

        assert!(tt.probe(hash, 5, -1000, 1000).is_none());
    }

    #[test]
    fn test_atomic_tt_stats() {
        let tt = AtomicTT::new(1);
        let stats = tt.stats();
        assert_eq!(stats.used, 0);

        tt.store(0x111, 5, 100, EntryType::Exact, None);
        tt.store(0x222, 5, 100, EntryType::Exact, None);

        let stats = tt.stats();
        assert!(stats.used >= 2);
    }

    #[test]
    fn test_atomic_tt_replacement_policy() {
        let tt = AtomicTT::new(1);
        let hash = 0x123456789ABCDEF0;

        // Store shallow, then deeper — deeper replaces
        tt.store(hash, 3, 100, EntryType::Exact, Some(Pos::new(5, 5)));
        tt.store(hash, 5, 200, EntryType::Exact, Some(Pos::new(9, 9)));
        assert_eq!(tt.probe(hash, 5, -1000, 1000).unwrap().0, 200);
    }

    #[test]
    fn test_atomic_tt_concurrent_safety() {
        use std::sync::Arc;
        use std::thread;

        let tt = Arc::new(AtomicTT::new(1));
        let mut handles = Vec::new();

        // Spawn 4 threads writing different entries concurrently
        for t in 0..4u64 {
            let tt = Arc::clone(&tt);
            handles.push(thread::spawn(move || {
                for i in 0..1000u64 {
                    let hash = t * 100_000 + i;
                    tt.store(hash, 5, (i as i32) * 10, EntryType::Exact, Some(Pos::new(9, 9)));
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Verify: some entries should be readable (exact count depends on collisions)
        let stats = tt.stats();
        assert!(stats.used > 0, "Should have some entries after concurrent writes");
    }
}
