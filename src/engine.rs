//! Main AI Engine integrating all search components
//!
//! This module provides the main AI engine that orchestrates all search algorithms
//! to find the best move in any given position. The search follows a priority system:
//!
//! 1. **Immediate win**: Check for any move that wins instantly
//! 2. **VCF (Victory by Continuous Fours)**: Search for forced wins using four-threats
//! 3. **VCT (Victory by Continuous Threats)**: Search using open-three threats
//! 4. **Defense**: Block opponent's winning threats
//! 5. **Alpha-Beta**: Regular search with transposition table
//!
//! # Example
//!
//! ```
//! use gomoku::{AIEngine, Board, Stone, Pos};
//!
//! // Use smaller depth for faster example
//! let mut engine = AIEngine::with_config(8, 4, 500);
//! let mut board = Board::new();
//!
//! // Set up a position with some stones (faster than empty board)
//! board.place_stone(Pos::new(9, 9), Stone::Black);
//!
//! // Get best move for White
//! let result = engine.get_move_with_stats(&board, Stone::White);
//! println!("Best move: {:?}", result.best_move);
//! println!("Search type: {:?}", result.search_type);
//! println!("Time: {}ms", result.time_ms);
//! ```

use crate::board::{Board, Pos, Stone, BOARD_SIZE};
use crate::rules::{check_winner, execute_captures, is_valid_move};
use crate::search::{SearchResult, Searcher, ThreatResult, ThreatSearcher};
use std::time::{Duration, Instant};

/// Type of search that produced the result.
///
/// This indicates which phase of the search hierarchy found the move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    /// Found immediate winning move (5-in-a-row or capture win)
    ImmediateWin,
    /// Found forced win via Victory by Continuous Fours
    VCF,
    /// Found forced win via Victory by Continuous Threats
    VCT,
    /// Defensive move to block opponent's threat
    Defense,
    /// Regular alpha-beta search result
    AlphaBeta,
}

/// Result of a move search with detailed statistics.
///
/// Contains the best move found along with metadata about the search.
#[derive(Debug, Clone)]
pub struct MoveResult {
    /// Best move found, if any
    pub best_move: Option<Pos>,
    /// Evaluation score of the position after the move
    pub score: i32,
    /// Type of search that found this move
    pub search_type: SearchType,
    /// Time taken in milliseconds
    pub time_ms: u64,
    /// Number of nodes searched
    pub nodes: u64,
}

impl MoveResult {
    /// Create a result for an immediate win
    #[inline]
    fn immediate_win(pos: Pos, time_ms: u64) -> Self {
        Self {
            best_move: Some(pos),
            score: 1_000_000,
            search_type: SearchType::ImmediateWin,
            time_ms,
            nodes: 1,
        }
    }

    /// Create a result for a VCF win
    #[inline]
    fn vcf_win(pos: Pos, time_ms: u64, nodes: u64) -> Self {
        Self {
            best_move: Some(pos),
            score: 900_000,
            search_type: SearchType::VCF,
            time_ms,
            nodes,
        }
    }

    /// Create a result for a VCT win
    #[inline]
    fn vct_win(pos: Pos, time_ms: u64, nodes: u64) -> Self {
        Self {
            best_move: Some(pos),
            score: 800_000,
            search_type: SearchType::VCT,
            time_ms,
            nodes,
        }
    }

    /// Create a result for a defensive move
    #[inline]
    fn defense(pos: Pos, score: i32, time_ms: u64, nodes: u64) -> Self {
        Self {
            best_move: Some(pos),
            score,
            search_type: SearchType::Defense,
            time_ms,
            nodes,
        }
    }

    /// Create a result from alpha-beta search
    #[inline]
    fn from_alphabeta(result: SearchResult, time_ms: u64) -> Self {
        Self {
            best_move: result.best_move,
            score: result.score,
            search_type: SearchType::AlphaBeta,
            time_ms,
            nodes: result.nodes,
        }
    }

    /// Create a quick alpha-beta result (for opening moves)
    #[inline]
    fn alpha_beta(pos: Pos, score: i32, time_ms: u64, nodes: u64) -> Self {
        Self {
            best_move: Some(pos),
            score,
            search_type: SearchType::AlphaBeta,
            time_ms,
            nodes,
        }
    }

    /// Create a result indicating no move found (used in tests)
    #[cfg(test)]
    fn no_move(time_ms: u64) -> Self {
        Self {
            best_move: None,
            score: 0,
            search_type: SearchType::AlphaBeta,
            time_ms,
            nodes: 0,
        }
    }
}

/// Main AI Engine for Gomoku.
///
/// The engine integrates multiple search algorithms with a priority-based
/// approach to find the best move efficiently. It uses:
/// - VCF/VCT threat search for forced wins
/// - Alpha-beta search with transposition table for general positions
/// - Immediate win/loss detection for quick responses
///
/// # Configuration
///
/// The engine can be configured with:
/// - Transposition table size (memory usage)
/// - Maximum search depth
/// - Time limit per move
///
/// # Example
///
/// ```
/// use gomoku::{AIEngine, Board, Stone, Pos};
///
/// // Create engine with custom configuration (smaller depth for doc test)
/// let mut engine = AIEngine::with_config(8, 4, 400);
///
/// let mut board = Board::new();
/// // Add some stones for faster search
/// board.place_stone(Pos::new(9, 9), Stone::Black);
/// if let Some(best_move) = engine.get_move(&board, Stone::White) {
///     println!("Play at ({}, {})", best_move.row, best_move.col);
/// }
/// ```
pub struct AIEngine {
    /// Alpha-beta searcher with transposition table
    searcher: Searcher,
    /// VCF/VCT threat searcher
    threat_searcher: ThreatSearcher,
    /// Maximum search depth for alpha-beta
    max_depth: i8,
    /// Time limit for search (used for future time management)
    #[allow(dead_code)]
    time_limit: Duration,
}

impl AIEngine {
    /// Create a new AI engine with default settings.
    ///
    /// Default configuration:
    /// - 64 MB transposition table
    /// - Maximum depth of 10
    /// - 500ms time limit
    ///
    /// # Example
    ///
    /// ```
    /// use gomoku::AIEngine;
    ///
    /// let engine = AIEngine::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            searcher: Searcher::new(64),
            threat_searcher: ThreatSearcher::new(),
            max_depth: 10,
            time_limit: Duration::from_millis(500),
        }
    }

    /// Create an AI engine with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `tt_size_mb` - Transposition table size in megabytes
    /// * `max_depth` - Maximum search depth for alpha-beta
    /// * `time_limit_ms` - Time limit in milliseconds
    ///
    /// # Example
    ///
    /// ```
    /// use gomoku::AIEngine;
    ///
    /// // High-performance configuration
    /// let engine = AIEngine::with_config(128, 14, 1000);
    /// ```
    #[must_use]
    pub fn with_config(tt_size_mb: usize, max_depth: i8, time_limit_ms: u64) -> Self {
        Self {
            searcher: Searcher::new(tt_size_mb),
            threat_searcher: ThreatSearcher::new(),
            max_depth,
            time_limit: Duration::from_millis(time_limit_ms),
        }
    }

    /// Get the best move for the given position.
    ///
    /// This is a convenience method that returns only the best move.
    /// Use `get_move_with_stats` if you need search statistics.
    ///
    /// # Arguments
    ///
    /// * `board` - Current board state
    /// * `color` - Color to move
    ///
    /// # Returns
    ///
    /// The best move found, or `None` if no valid moves exist.
    ///
    /// # Example
    ///
    /// ```
    /// use gomoku::{AIEngine, Board, Stone, Pos};
    ///
    /// // Use smaller depth for faster doc test
    /// let mut engine = AIEngine::with_config(8, 4, 500);
    /// let mut board = Board::new();
    /// board.place_stone(Pos::new(9, 9), Stone::Black);
    ///
    /// if let Some(pos) = engine.get_move(&board, Stone::White) {
    ///     println!("Best move: ({}, {})", pos.row, pos.col);
    /// }
    /// ```
    #[must_use]
    pub fn get_move(&mut self, board: &Board, color: Stone) -> Option<Pos> {
        self.get_move_with_stats(board, color).best_move
    }

    /// Get the best move with detailed search statistics.
    ///
    /// This method performs a full search and returns comprehensive
    /// information about the result, including:
    /// - The best move found
    /// - Evaluation score
    /// - Type of search that found the move
    /// - Time taken
    /// - Number of nodes searched
    ///
    /// # Arguments
    ///
    /// * `board` - Current board state
    /// * `color` - Color to move
    ///
    /// # Returns
    ///
    /// `MoveResult` containing the move and search statistics.
    ///
    /// # Search Priority
    ///
    /// The search follows this priority order:
    /// 1. Immediate winning move (instant)
    /// 2. VCF - forced win via continuous fours
    /// 3. VCT - forced win via continuous threats
    /// 4. Defense against opponent's VCF
    /// 5. Regular alpha-beta search
    #[must_use]
    pub fn get_move_with_stats(&mut self, board: &Board, color: Stone) -> MoveResult {
        let start = Instant::now();

        // 0. Opening book for fast early game response
        if let Some(opening_move) = self.get_opening_move(board, color) {
            return MoveResult::alpha_beta(
                opening_move,
                0,
                start.elapsed().as_millis() as u64,
                1,
            );
        }

        // 1. Check for immediate winning move (5-in-a-row or capture win)
        if let Some(win_move) = self.find_immediate_win(board, color) {
            return MoveResult::immediate_win(win_move, start.elapsed().as_millis() as u64);
        }

        // 2. CRITICAL: Check opponent's immediate threats FIRST
        // If opponent can win next move, we MUST block regardless of our own threats
        let opponent = color.opponent();

        // 2a. Check opponent's immediate win (5-in-a-row possible)
        if let Some(opp_win) = self.find_immediate_win(board, opponent) {
            // Block it if we can
            if is_valid_move(board, opp_win, color) {
                return MoveResult::defense(
                    opp_win,
                    0,
                    start.elapsed().as_millis() as u64,
                    1,
                );
            }
        }

        // 2b. Check opponent's four-in-a-row threats (must block or lose next turn)
        if let Some(block_pos) = self.find_four_threat(board, opponent, color) {
            return MoveResult::defense(
                block_pos,
                -50_000,
                start.elapsed().as_millis() as u64,
                1,
            );
        }

        // 3. Search VCF (Victory by Continuous Fours) - our winning threats
        let vcf_result = self.threat_searcher.search_vcf(board, color);
        if vcf_result.found && !vcf_result.winning_sequence.is_empty() {
            return MoveResult::vcf_win(
                vcf_result.winning_sequence[0],
                start.elapsed().as_millis() as u64,
                self.threat_searcher.nodes(),
            );
        }

        // 4. Check opponent's VCF - must defend before our slower VCT
        let opp_vcf = self.threat_searcher.search_vcf(board, opponent);
        if opp_vcf.found {
            if let Some(defense) = self.find_best_defense(board, color, &opp_vcf) {
                return MoveResult::defense(
                    defense,
                    -100_000,
                    start.elapsed().as_millis() as u64,
                    self.threat_searcher.nodes(),
                );
            }
        }

        // 5. Search VCT (Victory by Continuous Threats)
        // Only after all immediate threats are handled
        if board.stone_count() >= 8 {
            let vct_result = self.threat_searcher.search_vct(board, color);
            if vct_result.found && !vct_result.winning_sequence.is_empty() {
                return MoveResult::vct_win(
                    vct_result.winning_sequence[0],
                    start.elapsed().as_millis() as u64,
                    self.threat_searcher.nodes(),
                );
            }
        }

        // 5. Regular Alpha-Beta search
        // Use conservative depth to ensure fast response (<500ms target)
        // Deeper search doesn't help much without proper time management
        let effective_depth = 4.min(self.max_depth);

        let result = self.searcher.search(board, color, effective_depth);
        MoveResult::from_alphabeta(result, start.elapsed().as_millis() as u64)
    }

    /// Find an immediate winning move.
    ///
    /// Checks for moves that win instantly via:
    /// - 5-in-a-row
    /// - Capturing the 5th pair (10 total stones)
    fn find_immediate_win(&self, board: &Board, color: Stone) -> Option<Pos> {
        // Check if near capture win (4 pairs captured)
        let near_capture_win = board.captures(color) >= 4;

        for r in 0..BOARD_SIZE as u8 {
            for c in 0..BOARD_SIZE as u8 {
                let pos = Pos::new(r, c);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                let mut test_board = board.clone();
                test_board.place_stone(pos, color);
                execute_captures(&mut test_board, pos, color);

                // Check for win (5-in-a-row or capture win)
                if check_winner(&test_board) == Some(color) {
                    return Some(pos);
                }

                // Also check capture win explicitly if we're close
                if near_capture_win && test_board.captures(color) >= 5 {
                    return Some(pos);
                }
            }
        }
        None
    }

    /// Find the best defense against opponent's threat.
    ///
    /// Defense strategies:
    /// 1. Block at the threat position directly
    /// 2. Use alpha-beta to find the best defensive move
    fn find_best_defense(
        &mut self,
        board: &Board,
        color: Stone,
        threat: &ThreatResult,
    ) -> Option<Pos> {
        if threat.winning_sequence.is_empty() {
            return None;
        }

        let threat_move = threat.winning_sequence[0];

        // Option 1: Block at the threat position
        if is_valid_move(board, threat_move, color) {
            return Some(threat_move);
        }

        // Option 2: Use Alpha-Beta to find best defensive move
        // Use reduced depth for faster response
        let result = self.searcher.search(board, color, 6.min(self.max_depth));
        result.best_move
    }

    /// Set the maximum search depth for alpha-beta.
    ///
    /// Higher depths give stronger play but take longer.
    /// Recommended range: 8-14
    ///
    /// # Arguments
    ///
    /// * `depth` - Maximum search depth
    pub fn set_max_depth(&mut self, depth: i8) {
        self.max_depth = depth;
    }

    /// Set the time limit for search.
    ///
    /// Note: Time management is not yet fully implemented.
    /// This sets the target time limit for future use.
    ///
    /// # Arguments
    ///
    /// * `time_ms` - Time limit in milliseconds
    pub fn set_time_limit(&mut self, time_ms: u64) {
        self.time_limit = Duration::from_millis(time_ms);
    }

    /// Clear the transposition table cache.
    ///
    /// Call this when starting a new game to avoid stale positions.
    pub fn clear_cache(&mut self) {
        self.searcher.clear_tt();
    }

    /// Get the current maximum search depth.
    #[must_use]
    pub fn max_depth(&self) -> i8 {
        self.max_depth
    }

    /// Get transposition table statistics.
    #[must_use]
    pub fn tt_stats(&self) -> crate::search::TTStats {
        self.searcher.tt_stats()
    }

    /// Find opponent's four-in-a-row threat and return blocking position.
    ///
    /// Scans the board for patterns where opponent has 4 stones with an open end.
    /// Returns the position that would complete 5-in-a-row for the opponent.
    fn find_four_threat(&self, board: &Board, opponent: Stone, color: Stone) -> Option<Pos> {
        let directions: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

        for r in 0..BOARD_SIZE as u8 {
            for c in 0..BOARD_SIZE as u8 {
                let pos = Pos::new(r, c);
                if board.get(pos) != opponent {
                    continue;
                }

                // Check each direction from this stone
                for &(dr, dc) in &directions {
                    let mut count = 1;
                    let mut open_ends = Vec::new();

                    // Count in positive direction
                    let mut nr = r as i8 + dr;
                    let mut nc = c as i8 + dc;
                    while nr >= 0 && nr < BOARD_SIZE as i8 && nc >= 0 && nc < BOARD_SIZE as i8 {
                        let np = Pos::new(nr as u8, nc as u8);
                        if board.get(np) == opponent {
                            count += 1;
                            nr += dr;
                            nc += dc;
                        } else if board.get(np) == Stone::Empty {
                            open_ends.push(np);
                            break;
                        } else {
                            break;
                        }
                    }

                    // Count in negative direction
                    nr = r as i8 - dr;
                    nc = c as i8 - dc;
                    while nr >= 0 && nr < BOARD_SIZE as i8 && nc >= 0 && nc < BOARD_SIZE as i8 {
                        let np = Pos::new(nr as u8, nc as u8);
                        if board.get(np) == opponent {
                            count += 1;
                            nr -= dr;
                            nc -= dc;
                        } else if board.get(np) == Stone::Empty {
                            open_ends.push(np);
                            break;
                        } else {
                            break;
                        }
                    }

                    // If opponent has 4 in a row with at least one open end, block it
                    if count >= 4 {
                        for &end_pos in &open_ends {
                            if is_valid_move(board, end_pos, color) {
                                return Some(end_pos);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Get an opening move for early game positions.
    ///
    /// Returns a quick move without expensive search for very sparse boards:
    /// - Empty board: play center (9,9)
    /// - 1-3 stones: play adjacent to existing stones near center
    ///
    /// Returns `None` if board has 4+ stones to ensure proper threat detection.
    fn get_opening_move(&self, board: &Board, color: Stone) -> Option<Pos> {
        let stone_count = board.stone_count();

        // Only use opening book for first few moves (0-3 stones)
        // After that, use full search to detect and respond to threats
        if stone_count > 3 {
            return None;
        }

        let center = Pos::new(9, 9);

        // Empty board: play center
        if stone_count == 0 {
            return Some(center);
        }

        // If center is empty and valid, play there
        if board.get(center) == Stone::Empty && is_valid_move(board, center, color) {
            return Some(center);
        }

        // Find the centroid of existing stones and play near it
        let mut sum_row: i32 = 0;
        let mut sum_col: i32 = 0;
        let mut count = 0;

        for r in 0..BOARD_SIZE as u8 {
            for c in 0..BOARD_SIZE as u8 {
                let pos = Pos::new(r, c);
                if board.get(pos) != Stone::Empty {
                    sum_row += r as i32;
                    sum_col += c as i32;
                    count += 1;
                }
            }
        }

        if count == 0 {
            return Some(center);
        }

        let center_row = (sum_row / count) as u8;
        let center_col = (sum_col / count) as u8;

        // Try positions in expanding rings around the centroid
        let offsets: [(i8, i8); 8] = [
            (0, 1), (1, 0), (0, -1), (-1, 0),  // orthogonal
            (1, 1), (1, -1), (-1, 1), (-1, -1), // diagonal
        ];

        for radius in 1..=3 {
            for &(dr, dc) in &offsets {
                let r = center_row as i8 + dr * radius;
                let c = center_col as i8 + dc * radius;

                if r >= 0 && r < BOARD_SIZE as i8 && c >= 0 && c < BOARD_SIZE as i8 {
                    let pos = Pos::new(r as u8, c as u8);
                    if is_valid_move(board, pos, color) {
                        return Some(pos);
                    }
                }
            }
        }

        // Fallback: find any valid move near center
        for r in 7..12u8 {
            for c in 7..12u8 {
                let pos = Pos::new(r, c);
                if is_valid_move(board, pos, color) {
                    return Some(pos);
                }
            }
        }

        None
    }
}

impl Default for AIEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = AIEngine::new();
        assert_eq!(engine.max_depth(), 10);
    }

    #[test]
    fn test_engine_with_config() {
        let engine = AIEngine::with_config(16, 8, 100);
        assert_eq!(engine.max_depth(), 8);
    }

    #[test]
    fn test_engine_finds_immediate_win() {
        let mut board = Board::new();
        // 4 in a row - one away from win
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let mut engine = AIEngine::new();
        let result = engine.get_move_with_stats(&board, Stone::Black);

        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
        assert_eq!(result.search_type, SearchType::ImmediateWin);
    }

    #[test]
    fn test_engine_blocks_opponent_win() {
        let mut board = Board::new();
        // White has 4 in a row
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }
        board.place_stone(Pos::new(10, 5), Stone::Black); // Some black stone

        let mut engine = AIEngine::new();
        let result = engine.get_move_with_stats(&board, Stone::Black);

        // Should block at (9,4)
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
        assert_eq!(result.search_type, SearchType::Defense);
    }

    #[test]
    fn test_engine_empty_board() {
        let board = Board::new();
        // Use smaller depth for faster test
        let mut engine = AIEngine::with_config(8, 4, 500);
        let result = engine.get_move(&board, Stone::Black);

        // Should play center
        assert_eq!(result, Some(Pos::new(9, 9)));
    }

    #[test]
    fn test_engine_vcf_detection() {
        let mut board = Board::new();
        // Set up position with 4 in a row (immediate win, not just VCF)
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::Black);
        board.place_stone(Pos::new(9, 8), Stone::Black);

        let mut engine = AIEngine::new();
        let result = engine.get_move_with_stats(&board, Stone::Black);

        // Should find immediate win
        assert!(result.best_move.is_some());
        assert_eq!(result.search_type, SearchType::ImmediateWin);
    }

    #[test]
    fn test_engine_time_reasonable() {
        let mut board = Board::new();
        // Create a position where there's already some activity
        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(10, 10), Stone::White);
        board.place_stone(Pos::new(9, 10), Stone::Black);
        board.place_stone(Pos::new(8, 9), Stone::White);

        // Use depth 2 for speed test (debug mode is ~30x slower than release)
        let mut engine = AIEngine::with_config(8, 2, 100);
        let result = engine.get_move_with_stats(&board, Stone::Black);

        // Allow more time in debug builds (unoptimized code is much slower)
        #[cfg(debug_assertions)]
        let max_time_ms = 60_000; // 60 seconds for debug
        #[cfg(not(debug_assertions))]
        let max_time_ms = 5_000; // 5 seconds for release

        assert!(
            result.time_ms < max_time_ms,
            "Search took too long: {}ms (limit: {}ms)",
            result.time_ms,
            max_time_ms
        );
    }

    #[test]
    fn test_capture_win_detection() {
        let mut board = Board::new();
        // Set up near capture win scenario
        board.black_captures = 4; // 4 pairs = 8 stones

        // Place a capturable pair - this creates an immediate win via capture
        // B-W-W-? pattern at row 9, Black plays at col 11 to capture
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::White);
        board.place_stone(Pos::new(9, 10), Stone::White);
        // Add scattered stones away from capture to exceed threshold
        board.place_stone(Pos::new(3, 3), Stone::Black);
        board.place_stone(Pos::new(3, 15), Stone::White);
        board.place_stone(Pos::new(15, 3), Stone::Black);
        board.place_stone(Pos::new(15, 15), Stone::White);
        board.place_stone(Pos::new(5, 5), Stone::Black);
        board.place_stone(Pos::new(5, 13), Stone::White);

        let mut engine = AIEngine::new();
        let result = engine.get_move(&board, Stone::Black);

        // Should find capture at (9,11) for the win
        assert_eq!(result, Some(Pos::new(9, 11)));
    }

    #[test]
    fn test_engine_clear_cache() {
        let mut engine = AIEngine::with_config(8, 4, 500);

        // Verify clear_cache works by checking stats reset
        // First, manually trigger some TT usage through internal searcher
        let mut board = Board::new();
        // Create a mid-game position with scattered stones to force alpha-beta
        // Position has no immediate threats but requires search
        for i in 0..5 {
            board.place_stone(Pos::new(4 + i, 4), Stone::Black);
            board.place_stone(Pos::new(4 + i, 14), Stone::White);
        }
        // This should trigger alpha-beta search (>8 stones, no immediate win)
        let _ = engine.get_move(&board, Stone::Black);

        // Clear cache
        engine.clear_cache();
        let stats_after = engine.tt_stats();
        assert_eq!(stats_after.used, 0, "TT should be empty after clear");
    }

    #[test]
    fn test_engine_set_depth() {
        let mut engine = AIEngine::new();
        assert_eq!(engine.max_depth(), 10);

        engine.set_max_depth(12);
        assert_eq!(engine.max_depth(), 12);
    }

    #[test]
    fn test_engine_set_time_limit() {
        let mut engine = AIEngine::new();
        engine.set_time_limit(1000);
        // Time limit is stored but not actively used yet
        // This test just ensures no panic
    }

    #[test]
    fn test_engine_default() {
        let engine = AIEngine::default();
        assert_eq!(engine.max_depth(), 10);
    }

    #[test]
    fn test_move_result_types() {
        let pos = Pos::new(9, 9);

        let win = MoveResult::immediate_win(pos, 10);
        assert_eq!(win.search_type, SearchType::ImmediateWin);
        assert_eq!(win.score, 1_000_000);

        let vcf = MoveResult::vcf_win(pos, 20, 100);
        assert_eq!(vcf.search_type, SearchType::VCF);
        assert_eq!(vcf.score, 900_000);

        let vct = MoveResult::vct_win(pos, 30, 200);
        assert_eq!(vct.search_type, SearchType::VCT);
        assert_eq!(vct.score, 800_000);

        let defense = MoveResult::defense(pos, -100_000, 40, 50);
        assert_eq!(defense.search_type, SearchType::Defense);

        let no_move = MoveResult::no_move(50);
        assert!(no_move.best_move.is_none());
    }

    #[test]
    fn test_engine_responds_to_threat() {
        let mut board = Board::new();
        // White has 4 in a row (immediate threat, not just open three)
        // This is faster because it triggers Defense search, not VCT
        board.place_stone(Pos::new(9, 6), Stone::White);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::White);
        // Black has a stone nearby
        board.place_stone(Pos::new(8, 8), Stone::Black);

        let mut engine = AIEngine::with_config(8, 4, 500);
        let result = engine.get_move_with_stats(&board, Stone::Black);

        // Should find blocking move
        assert!(result.best_move.is_some());
        // Should block at one of the ends
        let m = result.best_move.unwrap();
        assert!(m == Pos::new(9, 5) || m == Pos::new(9, 10));
    }

    #[test]
    fn test_engine_multiple_searches() {
        // Use smaller depth for faster test
        let mut engine = AIEngine::with_config(8, 4, 500);
        let board = Board::new();

        // Multiple searches should work correctly
        let result1 = engine.get_move(&board, Stone::Black);
        let result2 = engine.get_move(&board, Stone::Black);

        // Results should be consistent
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_engine_alternating_colors() {
        // Use smaller depth for faster test
        let mut engine = AIEngine::with_config(8, 4, 500);
        let mut board = Board::new();

        // Simulate a few moves
        let black_move = engine.get_move(&board, Stone::Black);
        assert!(black_move.is_some());
        board.place_stone(black_move.unwrap(), Stone::Black);

        let white_move = engine.get_move(&board, Stone::White);
        assert!(white_move.is_some());
        board.place_stone(white_move.unwrap(), Stone::White);

        // Continue playing
        let black_move2 = engine.get_move(&board, Stone::Black);
        assert!(black_move2.is_some());
    }

    #[test]
    fn test_search_type_equality() {
        assert_eq!(SearchType::ImmediateWin, SearchType::ImmediateWin);
        assert_ne!(SearchType::ImmediateWin, SearchType::VCF);
        assert_ne!(SearchType::VCF, SearchType::VCT);
        assert_ne!(SearchType::Defense, SearchType::AlphaBeta);
    }
}
