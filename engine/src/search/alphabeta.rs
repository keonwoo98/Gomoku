//! Alpha-Beta search with iterative deepening and transposition table
//!
//! This module implements the core search algorithm for the Gomoku AI.
//! It uses negamax with alpha-beta pruning and transposition table for efficiency.
//!
//! # Features
//!
//! - Iterative deepening for time management and move ordering
//! - Transposition table for avoiding redundant searches
//! - Early cutoff when winning move is found
//! - Move generation with proximity filtering
//!
//! # Example
//!
//! ```
//! use gomoku::board::{Board, Stone, Pos};
//! use gomoku::search::Searcher;
//!
//! let mut searcher = Searcher::new(16); // 16 MB transposition table
//! let board = Board::new();
//!
//! let result = searcher.search(&board, Stone::Black, 4);
//! if let Some(best_move) = result.best_move {
//!     println!("Best move: ({}, {})", best_move.row, best_move.col);
//! }
//! ```

use crate::board::{Board, Pos, Stone, BOARD_SIZE};
use crate::eval::{evaluate, PatternScore};
use crate::rules::{check_winner, execute_captures, is_valid_move};

use super::{EntryType, TranspositionTable, TTStats, ZobristTable};

/// Infinity score for alpha-beta bounds
const INF: i32 = PatternScore::FIVE + 1;

/// Search result containing the best move found and associated statistics.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Best move found, if any
    pub best_move: Option<Pos>,
    /// Evaluation score of the best move
    pub score: i32,
    /// Depth completed in iterative deepening
    pub depth: i8,
    /// Total nodes searched
    pub nodes: u64,
}

/// Alpha-Beta search engine with iterative deepening and transposition table.
///
/// The searcher maintains a transposition table across searches for efficiency.
/// For a new game, call `clear_tt()` to reset the cached positions.
pub struct Searcher {
    zobrist: ZobristTable,
    tt: TranspositionTable,
    nodes: u64,
    max_depth: i8,
}

impl Searcher {
    /// Create a new searcher with the specified transposition table size.
    ///
    /// # Arguments
    ///
    /// * `tt_size_mb` - Size of transposition table in megabytes
    ///
    /// # Example
    ///
    /// ```
    /// use gomoku::search::Searcher;
    ///
    /// let searcher = Searcher::new(32); // 32 MB table
    /// ```
    #[must_use]
    pub fn new(tt_size_mb: usize) -> Self {
        Self {
            zobrist: ZobristTable::new(),
            tt: TranspositionTable::new(tt_size_mb),
            nodes: 0,
            max_depth: 10,
        }
    }

    /// Search for the best move using iterative deepening.
    ///
    /// Performs iterative deepening alpha-beta search up to the specified
    /// maximum depth. Returns early if a winning move is found.
    ///
    /// # Arguments
    ///
    /// * `board` - Current board state
    /// * `color` - Color to move
    /// * `max_depth` - Maximum search depth
    ///
    /// # Returns
    ///
    /// `SearchResult` containing the best move, score, depth reached, and node count.
    #[must_use]
    pub fn search(&mut self, board: &Board, color: Stone, max_depth: i8) -> SearchResult {
        self.nodes = 0;
        self.max_depth = max_depth;

        let mut best_result = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
        };

        // Iterative deepening: search progressively deeper
        for depth in 1..=max_depth {
            let result = self.search_root(board, color, depth);
            best_result = result;
            best_result.depth = depth;

            // Early exit if found winning move
            if best_result.score >= PatternScore::FIVE - 100 {
                break;
            }
        }

        best_result.nodes = self.nodes;
        best_result
    }

    /// Root-level search with full alpha-beta window.
    ///
    /// This is separate from the recursive `alpha_beta` to handle
    /// root-specific logic and move ordering.
    fn search_root(&mut self, board: &Board, color: Stone, depth: i8) -> SearchResult {
        let mut best_move = None;
        let mut best_score = -INF;
        let mut alpha = -INF;
        let beta = INF;

        let moves = self.generate_moves(board, color);

        for mov in moves {
            let mut new_board = board.clone();
            new_board.place_stone(mov, color);
            execute_captures(&mut new_board, mov, color);

            let score = -self.alpha_beta(&new_board, color.opponent(), depth - 1, -beta, -alpha);

            if score > best_score {
                best_score = score;
                best_move = Some(mov);
            }

            alpha = alpha.max(score);
        }

        SearchResult {
            best_move,
            score: best_score,
            depth,
            nodes: self.nodes,
        }
    }

    /// Recursive alpha-beta search with negamax formulation.
    ///
    /// Uses the transposition table to avoid redundant searches and
    /// returns early on terminal positions or depth limit.
    fn alpha_beta(
        &mut self,
        board: &Board,
        color: Stone,
        depth: i8,
        mut alpha: i32,
        beta: i32,
    ) -> i32 {
        self.nodes += 1;

        // Check for terminal state (win/loss)
        if let Some(winner) = check_winner(board) {
            return if winner == color {
                PatternScore::FIVE
            } else {
                -PatternScore::FIVE
            };
        }

        // Depth limit reached - evaluate position
        if depth <= 0 {
            return evaluate(board, color);
        }

        // Transposition table probe
        let hash = self.zobrist.hash(board, color);
        if let Some((score, _best_move)) = self.tt.probe(hash, depth, alpha, beta) {
            // Score is usable if non-zero (per TT documentation)
            if score != 0 {
                return score;
            }
        }

        let moves = self.generate_moves(board, color);
        if moves.is_empty() {
            return evaluate(board, color);
        }

        let mut best_score = -INF;
        let mut best_move = None;
        let mut entry_type = EntryType::UpperBound;

        for mov in moves {
            let mut new_board = board.clone();
            new_board.place_stone(mov, color);
            execute_captures(&mut new_board, mov, color);

            let score =
                -self.alpha_beta(&new_board, color.opponent(), depth - 1, -beta, -alpha);

            if score > best_score {
                best_score = score;
                best_move = Some(mov);
            }

            if score >= beta {
                // Beta cutoff - this is a lower bound
                entry_type = EntryType::LowerBound;
                break;
            }

            if score > alpha {
                alpha = score;
                entry_type = EntryType::Exact;
            }
        }

        // Store result in transposition table
        self.tt
            .store(hash, depth, best_score, entry_type, best_move);

        best_score
    }

    /// Generate candidate moves near existing stones.
    ///
    /// Uses a proximity heuristic: only considers empty positions within
    /// a radius of 2 from any existing stone. This dramatically reduces
    /// the search space while keeping relevant moves.
    ///
    /// # Arguments
    ///
    /// * `board` - Current board state
    /// * `color` - Color to move
    ///
    /// # Returns
    ///
    /// Vector of valid candidate moves.
    #[must_use]
    fn generate_moves(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut moves = Vec::with_capacity(50);
        let mut seen = [[false; BOARD_SIZE]; BOARD_SIZE];

        // If board is empty, return center
        if board.is_board_empty() {
            return vec![Pos::new(9, 9)];
        }

        // Find moves near existing stones within radius
        let radius = 2i32;

        // Iterate over all stones (both colors)
        for pos in board.black.iter_ones().chain(board.white.iter_ones()) {
            for dr in -radius..=radius {
                for dc in -radius..=radius {
                    let r = i32::from(pos.row) + dr;
                    let c = i32::from(pos.col) + dc;

                    if !Pos::is_valid(r, c) {
                        continue;
                    }

                    #[allow(clippy::cast_sign_loss)]
                    let r_usize = r as usize;
                    #[allow(clippy::cast_sign_loss)]
                    let c_usize = c as usize;

                    if seen[r_usize][c_usize] {
                        continue;
                    }
                    seen[r_usize][c_usize] = true;

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let new_pos = Pos::new(r as u8, c as u8);

                    // Check validity (empty and not forbidden double-three)
                    if is_valid_move(board, new_pos, color) {
                        moves.push(new_pos);
                    }
                }
            }
        }

        // TODO: Add move ordering (TT move first, killer moves, etc.)
        moves
    }

    /// Get statistics about the transposition table.
    ///
    /// # Returns
    ///
    /// `TTStats` containing size, usage count, and percentage.
    #[must_use]
    pub fn tt_stats(&self) -> TTStats {
        self.tt.stats()
    }

    /// Clear the transposition table.
    ///
    /// Call this when starting a new game to avoid stale cached positions.
    pub fn clear_tt(&mut self) {
        self.tt.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_empty_board() {
        let mut searcher = Searcher::new(16);
        let board = Board::new();

        let result = searcher.search(&board, Stone::Black, 4);
        assert!(result.best_move.is_some());
        // Empty board should play center
        assert_eq!(result.best_move, Some(Pos::new(9, 9)));
    }

    #[test]
    fn test_search_finds_winning_move() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        // Black has 4 in a row - should find the winning 5th
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let result = searcher.search(&board, Stone::Black, 2);
        // Should extend to make 5 in a row
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
    }

    #[test]
    fn test_search_blocks_opponent_win() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        // White has 4 in a row - Black must block
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }
        // Add a black stone nearby so it's not empty board
        board.place_stone(Pos::new(10, 0), Stone::Black);

        let result = searcher.search(&board, Stone::Black, 4);
        // Black should block at (9,4)
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
    }

    #[test]
    fn test_iterative_deepening_improves() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(10, 10), Stone::White);

        // Use depth 4 for faster test in debug mode
        let result = searcher.search(&board, Stone::Black, 4);
        assert!(result.depth >= 1);
        assert!(result.nodes > 0);
    }

    #[test]
    fn test_generate_moves_radius() {
        let searcher = Searcher::new(1);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);

        let moves = searcher.generate_moves(&board, Stone::White);

        // Should generate moves within radius 2 of (9,9)
        // Excluding (9,9) which is occupied
        assert!(!moves.is_empty());
        // 5x5 grid = 25 positions, minus 1 occupied = 24 max
        assert!(moves.len() <= 24);
    }

    #[test]
    fn test_search_with_captures() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        // Setup capture opportunity: B _ W W B
        // Black at (9,5), White at (9,7) and (9,8), Black at (9,9)
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let result = searcher.search(&board, Stone::Black, 4);
        // Should find the capture move at (9,6)
        assert_eq!(
            result.best_move,
            Some(Pos::new(9, 6)),
            "Should find capture move"
        );
    }

    #[test]
    fn test_tt_stats_after_search() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);

        let _ = searcher.search(&board, Stone::White, 4);

        let stats = searcher.tt_stats();
        // Should have stored some entries
        assert!(stats.used > 0);
    }

    #[test]
    fn test_clear_tt() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);
        let _ = searcher.search(&board, Stone::White, 4);

        let stats_before = searcher.tt_stats();
        assert!(stats_before.used > 0);

        searcher.clear_tt();

        let stats_after = searcher.tt_stats();
        assert_eq!(stats_after.used, 0);
    }

    #[test]
    fn test_search_winning_score() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        // Black has 4 in a row and can win
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let result = searcher.search(&board, Stone::Black, 2);
        // Score should be very high (winning)
        assert!(
            result.score >= PatternScore::FIVE - 100,
            "Should detect winning position"
        );
    }

    #[test]
    fn test_search_losing_score() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        // White has 4 in a row and will win
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }
        // Black is far away
        board.place_stone(Pos::new(0, 0), Stone::Black);

        let result = searcher.search(&board, Stone::Black, 2);
        // Best move should be blocking
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
    }

    #[test]
    fn test_generate_moves_excludes_forbidden() {
        let searcher = Searcher::new(1);
        let mut board = Board::new();

        // Create a double-three setup at (9,9)
        // Horizontal: _ B _ B _ (place at 9 creates _ B B B _)
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);

        // Vertical: _ B _ B _
        board.place_stone(Pos::new(8, 9), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::Black);

        let moves = searcher.generate_moves(&board, Stone::Black);

        // (9,9) should be excluded due to double-three rule
        assert!(
            !moves.contains(&Pos::new(9, 9)),
            "Should exclude forbidden double-three move"
        );
    }

    #[test]
    fn test_search_node_count() {
        let mut searcher = Searcher::new(16);
        let board = Board::new();

        let result = searcher.search(&board, Stone::Black, 2);
        // Should have searched at least a few nodes
        assert!(result.nodes >= 1);
    }

    #[test]
    fn test_search_multiple_times() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);

        // First search
        let result1 = searcher.search(&board, Stone::White, 4);
        assert!(result1.best_move.is_some());

        // Second search should benefit from cached TT entries
        let result2 = searcher.search(&board, Stone::White, 4);
        assert_eq!(result1.best_move, result2.best_move);
    }
}
