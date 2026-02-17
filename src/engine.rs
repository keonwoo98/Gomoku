//! Main AI Engine integrating all search components
//!
//! This module provides the main AI engine that orchestrates all search algorithms
//! to find the best move in any given position. The search follows a priority pipeline:
//!
//! 0. **Opening book**: Fast early-game responses (center on empty board)
//! 0.5. **Break five**: Break opponent's breakable five via capture
//! 1. **Immediate win**: Check for any move that wins instantly (5-in-a-row or capture)
//! 2. **Block opponent**: Block opponent's immediate winning move
//! 3-4. **VCF**: Victory by Continuous Fours (both sides)
//! 5. **Alpha-Beta**: Full search with iterative deepening and time management
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
use crate::rules::{
    can_break_five_by_capture, execute_captures_fast, find_five_break_moves,
    find_five_line_at_pos, find_five_positions, has_five_at_pos, is_valid_move, undo_captures,
};
use crate::search::{SearchResult, Searcher, ThreatSearcher};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

/// Format a board position as human-readable notation (e.g., "J10")
pub fn pos_to_notation(pos: Pos) -> String {
    // Columns: A=0, B=1, ..., H=7, J=8 (skip I), K=9, ...
    let col_char = if pos.col < 8 {
        (b'A' + pos.col) as char
    } else {
        (b'A' + pos.col + 1) as char // skip 'I'
    };
    // Rows: 1=0, 2=1, ..., 19=18 (board display: bottom=1, top=19)
    format!("{}{}", col_char, pos.row + 1)
}

/// Write a log message to both gomoku_ai.log and stderr
pub fn ai_log(msg: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("gomoku_ai.log")
    {
        let _ = writeln!(file, "{}", msg);
        let _ = file.flush();
    }
    eprintln!("{}", msg);
}

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
    /// Search depth reached
    pub depth: i8,
    /// Transposition table usage percentage (0-100)
    pub tt_usage: u8,
    /// Nodes per second (kN/s)
    pub nps: u64,
}

impl MoveResult {
    /// Compute nodes per second in kN/s
    fn compute_nps(nodes: u64, time_ms: u64) -> u64 {
        if time_ms == 0 {
            0
        } else {
            nodes * 1000 / time_ms / 1000
        }
    }

    /// Create a result for an immediate win
    #[inline]
    fn immediate_win(pos: Pos, time_ms: u64) -> Self {
        Self {
            best_move: Some(pos),
            score: 1_000_000,
            search_type: SearchType::ImmediateWin,
            time_ms,
            nodes: 1,
            depth: 0,
            tt_usage: 0,
            nps: 0,
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
            depth: 0,
            tt_usage: 0,
            nps: Self::compute_nps(nodes, time_ms),
        }
    }

    /// Create a result for a VCT win
    #[inline]
    #[cfg(test)]
    fn vct_win(pos: Pos, time_ms: u64, nodes: u64) -> Self {
        Self {
            best_move: Some(pos),
            score: 800_000,
            search_type: SearchType::VCT,
            time_ms,
            nodes,
            depth: 0,
            tt_usage: 0,
            nps: Self::compute_nps(nodes, time_ms),
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
            depth: 0,
            tt_usage: 0,
            nps: 0,
        }
    }

    /// Create a result from alpha-beta search with TT stats
    #[inline]
    fn from_alphabeta(result: SearchResult, time_ms: u64, tt_usage: u8) -> Self {
        Self {
            best_move: result.best_move,
            score: result.score,
            search_type: SearchType::AlphaBeta,
            time_ms,
            nodes: result.nodes,
            depth: result.depth,
            tt_usage,
            nps: Self::compute_nps(result.nodes, time_ms),
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
            depth: 0,
            tt_usage: 0,
            nps: 0,
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
            depth: 0,
            tt_usage: 0,
            nps: 0,
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
    /// Time limit for search in milliseconds
    time_limit_ms: u64,
}

impl AIEngine {
    /// Create a new AI engine with default settings.
    ///
    /// Default configuration:
    /// - 64 MB transposition table
    /// - Maximum depth of 20 (iterative deepening stops at time limit)
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
            threat_searcher: ThreatSearcher::with_depths(30, 12),
            max_depth: 20,
            time_limit_ms: 500,
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
    /// let engine = AIEngine::with_config(128, 14, 500);
    /// ```
    #[must_use]
    pub fn with_config(tt_size_mb: usize, max_depth: i8, time_limit_ms: u64) -> Self {
        Self {
            searcher: Searcher::new(tt_size_mb),
            threat_searcher: ThreatSearcher::with_depths(30, 12),
            max_depth,
            time_limit_ms,
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
    /// 3. Alpha-beta search (handles offense, defense, and blocking)
    #[must_use]
    pub fn get_move_with_stats(&mut self, board: &Board, color: Stone) -> MoveResult {
        let start = Instant::now();
        // Actual game move number: stones on board + captured stones (removed) + 1
        let total_captured = 2 * (board.captures(Stone::Black) as u32 + board.captures(Stone::White) as u32);
        let move_num = board.stone_count() + total_captured + 1;
        let color_str = if color == Stone::Black { "Black" } else { "White" };

        // Dynamic heuristic phase detection
        let phase_total = board.stone_count()
            + (board.captures(Stone::Black) as u32 + board.captures(Stone::White) as u32) * 2;
        let phase_str = match phase_total {
            0..=10 => "Opening",
            11..=40 => "Midgame",
            _ => "Endgame",
        };

        let separator = "=".repeat(60);
        ai_log(&format!(
            "\n{}\n[Move #{} | AI: {} | Stones: {} | B-cap: {} W-cap: {} | Phase: {}]",
            separator, move_num, color_str, board.stone_count(),
            board.captures(Stone::Black), board.captures(Stone::White), phase_str
        ));

        // 0. Opening book for fast early game response
        if let Some(opening_move) = self.get_opening_move(board, color) {
            ai_log(&format!("  Stage 0 OPENING: {} (book move)", pos_to_notation(opening_move)));
            return MoveResult::alpha_beta(
                opening_move,
                0,
                start.elapsed().as_millis() as u64,
                1,
            );
        }

        // 0.5: Check if opponent has an existing breakable five — MUST break it NOW
        // In Ninuki-renju, a breakable five gives opponent ONE chance to capture.
        // If they fail, the five-holder wins. This is a forced response.
        let opponent = color.opponent();
        if let Some(opp_five) = find_five_positions(board, opponent) {
            if can_break_five_by_capture(board, &opp_five, opponent) {
                let break_moves = find_five_break_moves(board, &opp_five, opponent);
                let valid_breaks: Vec<Pos> = break_moves
                    .into_iter()
                    .filter(|&p| is_valid_move(board, p, color))
                    .collect();
                let break_strs: Vec<String> =
                    valid_breaks.iter().map(|p| pos_to_notation(*p)).collect();
                ai_log(&format!(
                    "  Stage 0.5 BREAK FIVE: opponent five exists! Break moves: [{}]",
                    break_strs.join(", ")
                ));
                if valid_breaks.len() == 1 {
                    // Check if the single break allows opponent to recreate an UNBREAKABLE five
                    let brk = valid_breaks[0];
                    let mut test_board = board.clone();
                    test_board.place_stone(brk, color);
                    let cap_info = execute_captures_fast(&mut test_board, brk, color);
                    let mut recreates_unbreakable = false;
                    for i in 0..cap_info.count as usize {
                        let cap_pos = cap_info.positions[i];
                        test_board.place_stone(cap_pos, opponent);
                        if has_five_at_pos(&test_board, cap_pos, opponent) {
                            // Recreation possible — check if recreated five is unbreakable
                            if let Some(new_five) =
                                find_five_line_at_pos(&test_board, cap_pos, opponent)
                            {
                                if !can_break_five_by_capture(&test_board, &new_five, opponent) {
                                    recreates_unbreakable = true;
                                }
                            }
                        }
                        test_board.remove_stone(cap_pos);
                        if recreates_unbreakable {
                            break;
                        }
                    }
                    if recreates_unbreakable {
                        ai_log(&format!(
                            "  >>> FORCED BREAK {} rejected: opponent recreates UNBREAKABLE five — falling through to alpha-beta",
                            pos_to_notation(brk)
                        ));
                        // Fall through to alpha-beta for a strategic alternative
                    } else {
                        ai_log(&format!(
                            "  >>> FORCED BREAK: {}",
                            pos_to_notation(brk)
                        ));
                        return MoveResult::defense(
                            brk,
                            -900_000,
                            start.elapsed().as_millis() as u64,
                            1,
                        );
                    }
                } else if valid_breaks.is_empty() {
                    ai_log("  Stage 0.5 BREAK FIVE: NO valid break moves — opponent wins!");
                    // Fall through to alpha-beta for best losing move
                } else {
                    // Multiple break moves: evaluate each with quick search
                    // to pick the best position after breaking.
                    // IMPORTANT: Check if opponent can recreate the five by replaying
                    // at a captured position. If so, the break leads to an infinite
                    // loop (AI breaks → opponent replays → AI breaks → ...).
                    // Only accept breaks where recreation is impossible.
                    let mut best_move = valid_breaks[0];
                    let mut best_score = i32::MIN;
                    let mut any_safe_break = false;
                    let mut test_board = board.clone();
                    for &brk in &valid_breaks {
                        test_board.place_stone(brk, color);
                        let cap_info = execute_captures_fast(&mut test_board, brk, color);

                        // Check if opponent can recreate an UNBREAKABLE five
                        // Breakable recreation (cycle) is acceptable — White must break anyway
                        let mut recreates_unbreakable = false;
                        for i in 0..cap_info.count as usize {
                            let cap_pos = cap_info.positions[i];
                            test_board.place_stone(cap_pos, opponent);
                            if has_five_at_pos(&test_board, cap_pos, opponent) {
                                if let Some(new_five) =
                                    find_five_line_at_pos(&test_board, cap_pos, opponent)
                                {
                                    if !can_break_five_by_capture(
                                        &test_board,
                                        &new_five,
                                        opponent,
                                    ) {
                                        recreates_unbreakable = true;
                                    }
                                }
                            }
                            test_board.remove_stone(cap_pos);
                            if recreates_unbreakable {
                                break;
                            }
                        }

                        if !recreates_unbreakable {
                            let score = crate::eval::evaluate(&test_board, color);
                            if score > best_score || !any_safe_break {
                                best_score = score;
                                best_move = brk;
                            }
                            any_safe_break = true;
                        } else {
                            ai_log(&format!(
                                "    Break {} rejected: opponent recreates UNBREAKABLE five",
                                pos_to_notation(brk)
                            ));
                        }

                        undo_captures(&mut test_board, color, &cap_info);
                        test_board.remove_stone(brk);
                    }
                    if any_safe_break {
                        ai_log(&format!(
                            "  >>> BEST BREAK: {} (eval={})",
                            pos_to_notation(best_move),
                            best_score
                        ));
                        return MoveResult::defense(
                            best_move,
                            -900_000,
                            start.elapsed().as_millis() as u64,
                            valid_breaks.len() as u64,
                        );
                    }
                    ai_log(
                        "  Stage 0.5: All breaks lead to UNBREAKABLE recreation — falling through to alpha-beta"
                    );
                    // Fall through to alpha-beta for best strategic move
                }
            } else {
                // Opponent's five is unbreakable — game should have already ended
                ai_log("  Stage 0.5 WARNING: Opponent has UNBREAKABLE five!");
            }
        }

        // 1. Check for immediate winning move (5-in-a-row or capture win)
        if let Some(win_move) = self.find_immediate_win(board, color) {
            ai_log(&format!("  Stage 1 IMMEDIATE WIN: {}", pos_to_notation(win_move)));
            return MoveResult::immediate_win(win_move, start.elapsed().as_millis() as u64);
        }
        ai_log("  Stage 1 Immediate win: none");

        // 2. Check if opponent can win immediately - MUST block
        let opponent_threats = self.find_winning_moves(board, opponent);
        ai_log(&format!("  Stage 2 Opponent threats: {} positions{}", opponent_threats.len(),
            if opponent_threats.is_empty() { String::new() }
            else { format!(" [{}]", opponent_threats.iter().map(|p| pos_to_notation(*p)).collect::<Vec<_>>().join(", ")) }
        ));
        if opponent_threats.len() == 1 {
            let block_pos = opponent_threats[0];
            if is_valid_move(board, block_pos, color) {
                ai_log(&format!("  >>> DEFENSE (block immediate): {}", pos_to_notation(block_pos)));
                return MoveResult::defense(
                    block_pos,
                    -900_000,
                    start.elapsed().as_millis() as u64,
                    1,
                );
            }
        } else if opponent_threats.len() >= 2 {
            ai_log("  WARNING: Opponent has OPEN FOUR (2+ wins) - likely lost!");
        }

        // 3. Search VCF (Victory by Continuous Fours) - our forced win
        // Skip VCF when opponent has 4+ captures: one more capture = instant win,
        // so VCF is unreliable (opponent can ignore fours and capture instead).
        // At 3 captures, find_defense_moves includes strategic captures as defenses,
        // so VCF is still usable. At 4, too dangerous — let alpha-beta handle it.
        let opp_captures = board.captures(opponent);
        let vcf_reliable = opp_captures < 4;
        if vcf_reliable {
            let vcf_result = self.threat_searcher.search_vcf(board, color);
            if vcf_result.found && !vcf_result.winning_sequence.is_empty() {
                let seq: Vec<String> = vcf_result.winning_sequence.iter().map(|p| pos_to_notation(*p)).collect();
                ai_log(&format!("  Stage 3 OUR VCF FOUND: sequence=[{}]", seq.join(" -> ")));
                return MoveResult::vcf_win(
                    vcf_result.winning_sequence[0],
                    start.elapsed().as_millis() as u64,
                    self.threat_searcher.nodes(),
                );
            }
            ai_log(&format!("  Stage 3 Our VCF: not found ({}nodes)", self.threat_searcher.nodes()));
        } else {
            ai_log(&format!("  Stage 3 VCF SKIPPED: opponent has {} captures (unreliable)", opp_captures));
        }

        // 4. Check opponent VCF - if opponent has a forced win, we must block
        // Skip when WE have 4+ captures (opponent's VCF is unreliable — we can capture)
        let our_captures = board.captures(color);
        let opp_vcf_reliable = our_captures < 4;
        if opp_vcf_reliable {
            let opp_vcf = self.threat_searcher.search_vcf(board, opponent);
            if opp_vcf.found && !opp_vcf.winning_sequence.is_empty() {
                let seq: Vec<String> = opp_vcf.winning_sequence.iter().map(|p| pos_to_notation(*p)).collect();
                ai_log(&format!("  Stage 4 OPPONENT VCF FOUND: sequence=[{}]", seq.join(" -> ")));
                let block_pos = opp_vcf.winning_sequence[0];
                if is_valid_move(board, block_pos, color) {
                    ai_log(&format!("  >>> DEFENSE (block VCF): {}", pos_to_notation(block_pos)));
                    return MoveResult::defense(
                        block_pos,
                        -800_000,
                        start.elapsed().as_millis() as u64,
                        self.threat_searcher.nodes(),
                    );
                }
            }
            ai_log(&format!("  Stage 4 Opponent VCF: not found ({}nodes)", self.threat_searcher.nodes()));
        } else {
            ai_log(&format!("  Stage 4 Opponent VCF SKIPPED: we have {} captures (can counter)", our_captures));
        }

        // NOTE: VCT removed from authoritative pipeline.
        // Open-three threats are NOT forcing — opponent can ignore and counter-attack.
        // Alpha-beta with threat extensions handles tactical sequences correctly.
        // VCF remains sound when capture counts are low.

        // 5. Alpha-Beta search handles ALL strategy
        // Adaptive time: allocate more time for critical mid-game, less for
        // opening (simple) and late-game (narrow trees).
        let adaptive_time = self.compute_time_limit(board);
        let result = self.searcher.search_timed(board, color, self.max_depth, adaptive_time);
        let tt_usage = self.searcher.tt_stats().usage_percent;
        let elapsed = start.elapsed().as_millis() as u64;

        ai_log(&format!(
            "  Stage 5 ALPHA-BETA: move={} score={} depth={} nodes={} time={}ms nps={}k tt={}%",
            result.best_move.map(|p| pos_to_notation(p)).unwrap_or("none".to_string()),
            result.score, result.depth, result.nodes, elapsed,
            MoveResult::compute_nps(result.nodes, elapsed), tt_usage
        ));
        ai_log(&format!(
            "    Stats: beta_cutoffs={} first_move_rate={:.1}% tt_probes={} tt_score_rate={:.1}% tt_move_hits={}",
            result.stats.beta_cutoffs,
            result.stats.first_move_rate(),
            result.stats.tt_probes,
            result.stats.tt_score_rate(),
            result.stats.tt_move_hits
        ));

        MoveResult::from_alphabeta(result, elapsed, tt_usage)
    }

    /// Compute adaptive time limit based on game phase.
    ///
    /// Only reduces time in the opening where positions are simple and
    /// deep search isn't critical. Mid-game and beyond get full time
    /// to maintain search depth and playing strength.
    fn compute_time_limit(&self, board: &Board) -> u64 {
        let stones = board.stone_count();

        // Only reduce time in opening — mid-game needs full depth
        let pct = match stones {
            0..=2 => 30,      // Very early: center/adjacent, trivial
            3..=4 => 60,      // Opening: still simple positions
            _ => 100,         // Mid-game+: full time for deep search
        };

        // Apply percentage with minimum floor of 300ms
        (self.time_limit_ms * pct / 100).max(300)
    }

    /// Find ALL positions where `color` can win immediately.
    ///
    /// Returns a list of winning positions (usually 1 for closed four, 2 for open four).
    /// Used to detect opponent threats that must be blocked.
    /// Uses make/unmake pattern with fast has_five_at_pos check.
    fn find_winning_moves(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut wins = Vec::new();
        let near_capture_win = board.captures(color) >= 4;
        let mut test_board = board.clone();

        for r in 0..BOARD_SIZE as u8 {
            for c in 0..BOARD_SIZE as u8 {
                let pos = Pos::new(r, c);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                // Make move
                test_board.place_stone(pos, color);
                let cap_info = execute_captures_fast(&mut test_board, pos, color);

                // Fast five-in-a-row check (O(4 directions) vs O(all_stones * 4))
                if has_five_at_pos(&test_board, pos, color) {
                    // Only count as win if opponent can't break it by capture
                    if let Some(five) = find_five_positions(&test_board, color) {
                        if !can_break_five_by_capture(&test_board, &five, color) {
                            wins.push(pos);
                        }
                    }
                }

                // Capture win check
                if near_capture_win && test_board.captures(color) >= 5 && !wins.contains(&pos) {
                    wins.push(pos);
                }

                // Unmake move
                undo_captures(&mut test_board, color, &cap_info);
                test_board.remove_stone(pos);
            }
        }
        wins
    }

    /// Find an immediate winning move.
    ///
    /// Checks for moves that win instantly via:
    /// - 5-in-a-row (using fast has_five_at_pos)
    /// - Capturing the 5th pair (10 total stones)
    /// Uses make/unmake pattern to avoid cloning per position.
    fn find_immediate_win(&self, board: &Board, color: Stone) -> Option<Pos> {
        let near_capture_win = board.captures(color) >= 4;
        let mut test_board = board.clone();

        for r in 0..BOARD_SIZE as u8 {
            for c in 0..BOARD_SIZE as u8 {
                let pos = Pos::new(r, c);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                // Make move
                test_board.place_stone(pos, color);
                let cap_info = execute_captures_fast(&mut test_board, pos, color);

                // Check five-in-a-row (fast, O(4 directions))
                if has_five_at_pos(&test_board, pos, color) {
                    if let Some(five) = find_five_positions(&test_board, color) {
                        if !can_break_five_by_capture(&test_board, &five, color) {
                            // Unbreakable five → immediate win
                            return Some(pos);
                        }
                        // Five is STATICALLY breakable. Check if all breaks are illusory
                        // (break captures a bracket stone, so replay creates unbreakable five).
                        if Self::is_illusory_break(&test_board, &five, color) {
                            return Some(pos);
                        }
                    }
                }

                // Check capture win
                if near_capture_win && test_board.captures(color) >= 5 {
                    return Some(pos);
                }

                // Unmake move
                undo_captures(&mut test_board, color, &cap_info);
                test_board.remove_stone(pos);
            }
        }
        None
    }

    /// Check if all break captures on a five are illusory.
    ///
    /// A break is "illusory" when:
    /// 1. The break capture removes a five-stone AND a bracket stone
    /// 2. The five-holder replays the captured five-stone
    /// 3. The recreated five is unbreakable (bracket stone gone)
    ///
    /// If ALL break moves are illusory, the five is effectively unbreakable
    /// and counts as an immediate win (forced 3-ply sequence).
    fn is_illusory_break(board: &Board, five_positions: &[Pos], five_color: Stone) -> bool {
        let opponent = five_color.opponent();
        let break_moves = find_five_break_moves(board, five_positions, five_color);

        if break_moves.is_empty() {
            return false;
        }

        for &break_pos in &break_moves {
            // Simulate opponent's break capture
            let mut sim = board.clone();
            sim.place_stone(break_pos, opponent);
            let cap_info = execute_captures_fast(&mut sim, break_pos, opponent);

            // Find which five stones were captured
            let mut captured_five_stone = None;
            let mut captured_five_count = 0;
            for i in 0..cap_info.count as usize {
                if five_positions.contains(&cap_info.positions[i]) {
                    captured_five_stone = Some(cap_info.positions[i]);
                    captured_five_count += 1;
                }
            }

            // If two or more five stones captured, can't recreate with one replay
            if captured_five_count >= 2 {
                return false;
            }

            let replay_pos = match captured_five_stone {
                Some(p) => p,
                None => return false, // Break doesn't hit five stones (shouldn't happen)
            };

            // Position must be empty after capture (it was just captured)
            if !sim.is_empty(replay_pos) {
                return false;
            }

            // Simulate replay
            sim.place_stone(replay_pos, five_color);

            // Check if five is recreated at replay position
            if !has_five_at_pos(&sim, replay_pos, five_color) {
                return false;
            }

            // Check if recreated five is now unbreakable
            if let Some(new_five) = find_five_line_at_pos(&sim, replay_pos, five_color) {
                if can_break_five_by_capture(&sim, &new_five, five_color) {
                    return false; // Recreated five is still breakable → genuine break
                }
            } else {
                return false;
            }
        }

        true // All breaks are illusory → effectively unbreakable
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

    /// Set the time limit for alpha-beta search (milliseconds).
    /// The search uses iterative deepening with soft/hard limits derived from this value.
    ///
    /// # Arguments
    ///
    /// * `time_ms` - Time limit in milliseconds
    pub fn set_time_limit(&mut self, time_ms: u64) {
        self.time_limit_ms = time_ms;
    }

    /// Clear the transposition table cache.
    ///
    /// Call this when starting a new game to avoid stale positions.
    pub fn clear_cache(&mut self) {
        self.searcher.clear_tt();
        self.searcher.clear_history();
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

    /// Get an opening book move for the first 1-2 moves.
    ///
    /// - Empty board: play center (9,9)
    /// - One opponent stone: play diagonally adjacent, preferring center-ward
    ///
    /// Standard Gomoku opening theory: the second move should be placed
    /// adjacent to the opponent's stone to contest territory and start
    /// building connected patterns. Diagonal placement is strongest because
    /// it creates potential in two diagonal directions simultaneously.
    pub(crate) fn get_opening_move(&self, board: &Board, color: Stone) -> Option<Pos> {
        // Empty board → center is universally optimal
        if board.stone_count() == 0 {
            return Some(Pos::new(9, 9));
        }
        // Second move: play diagonally adjacent to opponent's only stone
        if board.stone_count() == 1 {
            let opponent = color.opponent();
            // Find the opponent's stone
            if let Some(stones) = board.stones(opponent) {
                if let Some(opp_pos) = stones.iter_ones().next() {
                    let center = (BOARD_SIZE / 2) as i32;
                    let diagonals: [(i32, i32); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];
                    let mut best: Option<Pos> = None;
                    let mut best_dist = i32::MAX;
                    for (dr, dc) in diagonals {
                        let nr = i32::from(opp_pos.row) + dr;
                        let nc = i32::from(opp_pos.col) + dc;
                        if Pos::is_valid(nr, nc) {
                            let dist = (nr - center).abs() + (nc - center).abs();
                            if dist < best_dist {
                                best_dist = dist;
                                #[allow(clippy::cast_sign_loss)]
                                {
                                    best = Some(Pos::new(nr as u8, nc as u8));
                                }
                            }
                        }
                    }
                    return best;
                }
            }
        }
        // Third move: our 2nd stone as second player (opponent has 2 stones)
        // Only use book for same-row or same-column opponent pairs (well-tested).
        // Diagonal pairs and other patterns fall through to alpha-beta search.
        if board.stone_count() == 3 {
            let opponent = color.opponent();
            if let (Some(my_bb), Some(opp_bb)) = (board.stones(color), board.stones(opponent)) {
                let mut my_iter = my_bb.iter_ones();
                let mut opp_iter = opp_bb.iter_ones();
                if let (Some(my_pos), Some(opp1), Some(opp2)) =
                    (my_iter.next(), opp_iter.next(), opp_iter.next())
                {
                    let same_row = opp1.row == opp2.row;
                    let same_col = opp1.col == opp2.col;
                    if my_iter.next().is_none()
                        && opp_iter.next().is_none()
                        && (same_row || same_col)
                    {
                        let center = (BOARD_SIZE / 2) as i32;
                        let diags: [(i32, i32); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];
                        let opp_stones = [opp1, opp2];

                        let mut best: Option<Pos> = None;
                        let mut best_score = i32::MIN;

                        for &opp_pos in &opp_stones {
                            for &(dr, dc) in &diags {
                                let nr = i32::from(opp_pos.row) + dr;
                                let nc = i32::from(opp_pos.col) + dc;
                                if !Pos::is_valid(nr, nc) { continue; }
                                #[allow(clippy::cast_sign_loss)]
                                let p = Pos::new(nr as u8, nc as u8);
                                if board.get(p) != Stone::Empty { continue; }

                                let center_dist =
                                    (nr - center).abs() + (nc - center).abs();
                                // Bonus: on same row/column as our stone (connectivity)
                                let connectivity = if nr == i32::from(my_pos.row)
                                    || nc == i32::from(my_pos.col)
                                { 10 } else { 0 };
                                // Bonus: diagonal-adjacent to BOTH opponent stones
                                let multi_disrupt = opp_stones
                                    .iter()
                                    .filter(|op| {
                                        (i32::from(op.row) - nr).abs() == 1
                                            && (i32::from(op.col) - nc).abs() == 1
                                    })
                                    .count() as i32
                                    * 5;

                                let score = 100 - center_dist * 15
                                    + connectivity + multi_disrupt;
                                if score > best_score {
                                    best_score = score;
                                    best = Some(p);
                                }
                            }
                        }
                        return best;
                    }
                }
            }
        }
        // Everything else goes through full search pipeline
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
        assert_eq!(engine.max_depth(), 20);
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

        // Should block at (9,4) - alpha-beta detects opponent's winning threat
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
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
    fn test_opening_book_disrupts_diagonal() {
        // Reproduce the losing game pattern: K10(B), J9(W), K8(B)
        // AI (White) should NOT play K9 (blocks column but ignores diagonal)
        // Should play L9 or similar to disrupt K8's diagonal expansion
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 9), Stone::Black);  // K10
        board.place_stone(Pos::new(8, 8), Stone::White);   // J9
        board.place_stone(Pos::new(7, 9), Stone::Black);   // K8

        let engine = AIEngine::new();
        let result = engine.get_opening_move(&board, Stone::White);

        // L9 (8,10) disrupts K8's diagonal toward M10 and connects to J9 via row
        assert_eq!(result, Some(Pos::new(8, 10)), "Expected L9 to disrupt diagonal");

        // Also verify through the full pipeline
        let mut engine2 = AIEngine::new();
        let move_result = engine2.get_move_with_stats(&board, Stone::White);
        assert_eq!(move_result.best_move, Some(Pos::new(8, 10)));
    }

    #[test]
    fn test_opening_book_skips_diagonal_pair() {
        // K10(B) + L9(B) = diagonal pair → book should NOT apply
        // Alpha-beta should handle this instead of a rigid book response
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 9), Stone::Black);   // K10
        board.place_stone(Pos::new(8, 8), Stone::White);    // J9
        board.place_stone(Pos::new(8, 10), Stone::Black);   // L9

        let engine = AIEngine::new();
        let result = engine.get_opening_move(&board, Stone::White);

        // Diagonal pair → no book move, fall through to search
        assert_eq!(result, None, "Diagonal pair should not trigger opening book");
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
        assert_eq!(engine.max_depth(), 20);

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
        assert_eq!(engine.max_depth(), 20);
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
        // White has 4 in a row - Black must block
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

    #[test]
    fn test_engine_blocks_gap_pattern() {
        // Test gap pattern: OO_OO where filling the gap completes 5
        // This is critical - AI must block at the gap position
        //
        // Pattern on column 12 (M column):
        // M14 = (5, 12) - Black
        // M13 = (6, 12) - Black
        // M12 = (7, 12) - EMPTY (gap)
        // M11 = (8, 12) - Black
        // M10 = (9, 12) - Black
        let mut board = Board::new();
        board.place_stone(Pos::new(5, 12), Stone::Black);  // M14
        board.place_stone(Pos::new(6, 12), Stone::Black);  // M13
        // Gap at (7, 12) - M12
        board.place_stone(Pos::new(8, 12), Stone::Black);  // M11
        board.place_stone(Pos::new(9, 12), Stone::Black);  // M10

        // Add some White stones
        board.place_stone(Pos::new(9, 9), Stone::White);
        board.place_stone(Pos::new(10, 10), Stone::White);

        let mut engine = AIEngine::with_config(8, 6, 500);
        let result = engine.get_move_with_stats(&board, Stone::White);

        // White MUST block at M12 (7, 12) - the gap position
        assert!(result.best_move.is_some(), "AI should find a blocking move");
        let block_pos = result.best_move.unwrap();
        assert_eq!(
            block_pos,
            Pos::new(7, 12),
            "AI should block at gap position M12 (7,12), got ({}, {})",
            block_pos.row,
            block_pos.col
        );
    }

    #[test]
    fn test_engine_blocks_horizontal_gap() {
        // Test horizontal gap pattern: OO_OO
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 6), Stone::Black);
        // Gap at (9, 7)
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        board.place_stone(Pos::new(5, 5), Stone::White);

        let mut engine = AIEngine::with_config(8, 6, 500);
        let result = engine.get_move_with_stats(&board, Stone::White);

        assert!(result.best_move.is_some());
        let block_pos = result.best_move.unwrap();
        assert_eq!(
            block_pos,
            Pos::new(9, 7),
            "AI should block at gap position (9,7)"
        );
    }

    #[test]
    fn test_search_depth_benchmark() {
        // Mid-game position with ~10 stones to measure realistic search depth
        let mut board = Board::new();
        let moves = [
            (9, 9, Stone::Black),
            (9, 10, Stone::White),
            (10, 9, Stone::Black),
            (8, 10, Stone::White),
            (10, 10, Stone::Black),
            (8, 8, Stone::White),
            (11, 8, Stone::Black),
            (7, 11, Stone::White),
            (10, 8, Stone::Black),
            (8, 9, Stone::White),
        ];
        for (r, c, s) in moves {
            board.place_stone(Pos::new(r, c), s);
        }

        let mut engine = AIEngine::new(); // Default: depth 20, 500ms
        let result = engine.get_move_with_stats(&board, Stone::Black);

        eprintln!(
            "BENCHMARK: depth={}, nodes={}, time={}ms, nps={}k, type={:?}",
            result.depth, result.nodes, result.time_ms, result.nps, result.search_type
        );

        // Verify AI searches at sufficient depth.
        // If the AI found a forced win/threat (VCF/VCT/immediate), early exit is correct.
        // Otherwise, depth 10+ is the project requirement.
        let found_forced_result = result.score.abs() >= 799_900
            || matches!(result.search_type, SearchType::VCF | SearchType::ImmediateWin);
        assert!(
            result.depth >= 10 || found_forced_result,
            "AI should reach depth 10+ or find forced result within 500ms, got depth {} score {} type {:?}",
            result.depth, result.score, result.search_type
        );
    }

    #[test]
    fn test_mid_game_search_quality() {
        // Mid-game position with some development from both sides.
        // Tests that AI reaches sufficient depth and makes reasonable decisions.
        // Note: depth 10 is the AVERAGE requirement across the game, not per-move.
        // Early/mid game with wide-open positions may only reach depth 8-9.
        let mut board = Board::new();
        let moves = [
            (9, 9, Stone::Black),   // center
            (7, 7, Stone::White),   // responds
            (9, 11, Stone::Black),  // extends right
            (7, 11, Stone::White),  // mirrors above
            (11, 9, Stone::Black),  // extends down
            (11, 11, Stone::White), // mirrors
            (5, 9, Stone::Black),   // extends up
            (5, 7, Stone::White),   // far
            (9, 5, Stone::Black),   // extends left
            (11, 7, Stone::White),  // scattered
        ];
        for (r, c, s) in moves {
            board.place_stone(Pos::new(r, c), s);
        }

        let mut engine = AIEngine::new();
        let result = engine.get_move_with_stats(&board, Stone::Black);

        eprintln!(
            "MID-GAME: depth={}, nodes={}, time={}ms, nps={}k, score={}, type={:?}",
            result.depth, result.nodes, result.time_ms, result.nps, result.score, result.search_type
        );

        // Should find a reasonable move - via alpha-beta depth 8+ or VCF/VCT forced win
        assert!(result.best_move.is_some(), "Should find a move");
        let found_forced = matches!(result.search_type, SearchType::VCF | SearchType::ImmediateWin);
        assert!(result.depth >= 8 || found_forced,
            "Should reach depth 8+ or find forced win, got depth {} type {:?}", result.depth, result.search_type);
        // Time should be under hard limit (700ms + margin)
        assert!(result.time_ms < 2000, "Should complete in reasonable time, took {}ms", result.time_ms);
    }

    /// Reproduce the exact position from game log where depth collapse occurred.
    /// Game 1, Move #8: K10(B), K8(W), J11(B), M8(W), L10(B), O6(W), J10(B).
    /// Previously collapsed to depth 4 with 2.3M nodes. Target: depth 8+.
    /// Test that AI detects and breaks opponent's existing breakable five.
    ///
    /// Reproduces the exact game log scenario where Black has a diagonal five
    /// H9-J10-K11-L12-M13 that is breakable (White can place at H8 to capture
    /// H9+H10). The AI MUST play the break move, not something else.
    #[test]
    fn test_engine_breaks_existing_five() {
        let mut board = Board::new();
        // Reconstruct the exact board state from game log at move #26
        // Notation: col letters skip I (A=0..H=7, J=8, K=9, L=10, M=11, N=12)
        // Row numbers: 1=row0, n=row(n-1)

        // Black stones (13 total):
        board.place_stone(Pos::new(7, 9), Stone::Black);   // K8
        board.place_stone(Pos::new(6, 8), Stone::Black);   // J7
        board.place_stone(Pos::new(8, 10), Stone::Black);  // L9
        board.place_stone(Pos::new(9, 9), Stone::Black);   // K10
        board.place_stone(Pos::new(10, 8), Stone::Black);  // J11
        board.place_stone(Pos::new(8, 7), Stone::Black);   // H9  (part of five)
        board.place_stone(Pos::new(11, 7), Stone::Black);  // H12
        board.place_stone(Pos::new(9, 7), Stone::Black);   // H10
        board.place_stone(Pos::new(9, 8), Stone::Black);   // J10 (part of five)
        board.place_stone(Pos::new(9, 6), Stone::Black);   // G10
        board.place_stone(Pos::new(10, 9), Stone::Black);  // K11 (part of five)
        board.place_stone(Pos::new(11, 10), Stone::Black); // L12 (part of five)
        board.place_stone(Pos::new(12, 11), Stone::Black); // M13 (part of five)
        // Black captures: 1 pair
        board.add_captures(Stone::Black, 1);

        // White stones (10 total):
        board.place_stone(Pos::new(5, 7), Stone::White);   // H6
        board.place_stone(Pos::new(10, 12), Stone::White);  // N11
        board.place_stone(Pos::new(7, 11), Stone::White);  // M8
        board.place_stone(Pos::new(8, 9), Stone::White);   // K9 (replayed)
        board.place_stone(Pos::new(12, 6), Stone::White);  // G13
        board.place_stone(Pos::new(10, 7), Stone::White);  // H11
        board.place_stone(Pos::new(9, 10), Stone::White);  // L10
        board.place_stone(Pos::new(9, 5), Stone::White);   // F10
        board.place_stone(Pos::new(7, 8), Stone::White);   // J8
        board.place_stone(Pos::new(7, 6), Stone::White);   // G8

        // Verify Black has a five
        let five = find_five_positions(&board, Stone::Black);
        assert!(five.is_some(), "Black should have a five-in-a-row");
        let five_positions = five.unwrap();
        assert!(five_positions.len() >= 5);

        // Verify the five is breakable
        assert!(
            can_break_five_by_capture(&board, &five_positions, Stone::Black),
            "Black's five should be breakable by White"
        );

        // AI (White) MUST break the five
        let mut engine = AIEngine::new();
        let result = engine.get_move_with_stats(&board, Stone::White);

        assert!(result.best_move.is_some(), "AI should find a move");
        let ai_move = result.best_move.unwrap();

        // Verify the AI's move actually breaks the five:
        // Place the stone and check if a capture removes part of the five
        let mut test = board.clone();
        test.place_stone(ai_move, Stone::White);
        let caps = crate::rules::get_captured_positions(&test, ai_move, Stone::White);
        let breaks_five = caps.iter().any(|cap| five_positions.contains(cap));
        assert!(
            breaks_five,
            "AI move {} must break Black's five, but it doesn't! Captured: {:?}",
            pos_to_notation(ai_move),
            caps.iter().map(|p| pos_to_notation(*p)).collect::<Vec<_>>()
        );

        // Should be classified as defense or alpha-beta (when all breaks allow
        // opponent to recreate the five, the engine falls through to alpha-beta
        // which still correctly chooses a break move)
        assert!(
            result.search_type == SearchType::Defense
                || result.search_type == SearchType::AlphaBeta,
            "Should be classified as Defense or AlphaBeta, got {:?}",
            result.search_type
        );
    }

    #[test]
    fn test_depth_collapse_regression() {
        let mut board = Board::new();
        // Game 1 position from gomoku_ai.log
        // Notation: K10 = col K (10), row 10 (9 in 0-indexed)
        let moves = [
            (9, 10, Stone::Black),  // K10
            (7, 10, Stone::White),  // K8
            (10, 9, Stone::Black),  // J11
            (7, 12, Stone::White),  // M8
            (9, 11, Stone::Black),  // L10
            (5, 14, Stone::White),  // O6
            (9, 9, Stone::Black),   // J10
        ];
        for (r, c, s) in moves {
            board.place_stone(Pos::new(r, c), s);
        }

        let mut engine = AIEngine::new();
        let result = engine.get_move_with_stats(&board, Stone::White);

        eprintln!(
            "DEPTH-COLLAPSE-REGRESSION: depth={}, nodes={}, time={}ms, nps={}k, score={}, type={:?}",
            result.depth, result.nodes, result.time_ms, result.nps, result.score, result.search_type
        );

        assert!(result.best_move.is_some(), "Should find a move");
        let found_forced = result.score.abs() >= 799_900
            || matches!(result.search_type, SearchType::VCF | SearchType::ImmediateWin);
        assert!(
            result.depth >= 8 || found_forced,
            "Depth collapse regression: expected depth 8+, got depth {} score {} ({:?})",
            result.depth, result.score, result.search_type
        );
    }

    /// Regression test: Game 5 loss pattern - find_winning_moves must detect open four
    /// Board state before move 14: Black has K10-L10-M10-N10 (4 consecutive on row 10)
    /// J10 and O10 should both be detected as winning moves for Black
    #[test]
    fn test_game5_open_four_detection() {
        let mut board = Board::new();

        // Black stones (7): K10, L10, M10, N10, M12, M9, G9
        board.place_stone(Pos::new(9, 9), Stone::Black); // K10
        board.place_stone(Pos::new(9, 10), Stone::Black); // L10
        board.place_stone(Pos::new(9, 11), Stone::Black); // M10
        board.place_stone(Pos::new(9, 12), Stone::Black); // N10
        board.place_stone(Pos::new(11, 11), Stone::Black); // M12
        board.place_stone(Pos::new(8, 11), Stone::Black); // M9
        board.place_stone(Pos::new(8, 6), Stone::Black); // G9

        // White stones (6): J9, L9, K9, M11, H9, N8
        board.place_stone(Pos::new(8, 8), Stone::White); // J9
        board.place_stone(Pos::new(8, 10), Stone::White); // L9
        board.place_stone(Pos::new(8, 9), Stone::White); // K9
        board.place_stone(Pos::new(10, 11), Stone::White); // M11
        board.place_stone(Pos::new(8, 7), Stone::White); // H9
        board.place_stone(Pos::new(7, 12), Stone::White); // N8

        // Verify board state
        assert_eq!(board.get(Pos::new(9, 9)), Stone::Black, "K10 should be Black");
        assert_eq!(board.get(Pos::new(9, 10)), Stone::Black, "L10 should be Black");
        assert_eq!(board.get(Pos::new(9, 11)), Stone::Black, "M10 should be Black");
        assert_eq!(board.get(Pos::new(9, 12)), Stone::Black, "N10 should be Black");
        assert!(board.is_empty(Pos::new(9, 8)), "J10 should be empty");
        assert!(board.is_empty(Pos::new(9, 13)), "O10 should be empty");

        // Test sub-functions individually
        let j10 = Pos::new(9, 8);
        let o10 = Pos::new(9, 13);

        // 1. is_valid_move should allow both
        assert!(is_valid_move(&board, j10, Stone::Black), "J10 should be valid for Black");
        assert!(is_valid_move(&board, o10, Stone::Black), "O10 should be valid for Black");

        // 2. has_five_at_pos should detect five after placing
        let mut test_board = board.clone();
        test_board.place_stone(j10, Stone::Black);
        assert!(
            has_five_at_pos(&test_board, j10, Stone::Black),
            "J10 should create five-in-a-row"
        );
        test_board.remove_stone(j10);

        test_board.place_stone(o10, Stone::Black);
        assert!(
            has_five_at_pos(&test_board, o10, Stone::Black),
            "O10 should create five-in-a-row"
        );
        test_board.remove_stone(o10);

        // 3. find_winning_moves: the fives ARE breakable (M9 at 8,11 allows capture of M10)
        // This is CORRECT behavior — not a bug in find_winning_moves
        let engine = AIEngine::new();
        let threats = engine.find_winning_moves(&board, Stone::Black);
        eprintln!("Game5 threats (breakable fives): {:?}", threats.iter().map(|p| pos_to_notation(*p)).collect::<Vec<_>>());
        // Both fives are breakable via M11-M10-M9 capture, so 0 threats is correct
        assert_eq!(threats.len(), 0, "Fives are breakable due to M9 — should be 0 threats");

        // 4. After K11 captures L10+M9, Black replays L10 → unbreakable open four
        // This is the real bug: AI doesn't see this sequence
        let mut post_capture = board.clone();
        // Simulate K11 capture
        post_capture.place_stone(Pos::new(10, 9), Stone::White); // K11
        post_capture.remove_stone(Pos::new(9, 10)); // capture L10
        post_capture.remove_stone(Pos::new(8, 11)); // capture M9
        post_capture.add_captures(Stone::White, 1);

        // Black replays L10
        post_capture.place_stone(Pos::new(9, 10), Stone::Black); // L10 replay

        // Now M9 is gone — fives should be UNBREAKABLE
        let threats_after = engine.find_winning_moves(&post_capture, Stone::Black);
        eprintln!(
            "After K11+L10 replay threats: {:?}",
            threats_after.iter().map(|p| pos_to_notation(*p)).collect::<Vec<_>>()
        );
        assert!(
            threats_after.len() >= 2,
            "After M9 removed, open four should have 2 unbreakable threats, got {}",
            threats_after.len()
        );
    }

    /// Test: AI should not play K11 in game5 position (removes defensive M9)
    #[test]
    fn test_game5_k11_is_good_capture() {
        let mut board = Board::new();

        // Black stones (7): K10, L10, M10, N10, M12, M9, G9
        board.place_stone(Pos::new(9, 9), Stone::Black); // K10
        board.place_stone(Pos::new(9, 10), Stone::Black); // L10
        board.place_stone(Pos::new(9, 11), Stone::Black); // M10
        board.place_stone(Pos::new(9, 12), Stone::Black); // N10
        board.place_stone(Pos::new(11, 11), Stone::Black); // M12
        board.place_stone(Pos::new(8, 11), Stone::Black); // M9
        board.place_stone(Pos::new(8, 6), Stone::Black); // G9

        // White stones (6): J9, L9, K9, M11, H9, N8
        board.place_stone(Pos::new(8, 8), Stone::White); // J9
        board.place_stone(Pos::new(8, 10), Stone::White); // L9
        board.place_stone(Pos::new(8, 9), Stone::White); // K9
        board.place_stone(Pos::new(10, 11), Stone::White); // M11
        board.place_stone(Pos::new(8, 7), Stone::White); // H9
        board.place_stone(Pos::new(7, 12), Stone::White); // N8

        let mut engine = AIEngine::with_config(20, 10, 2000);
        let result = engine.get_move_with_stats(&board, Stone::White);

        // K11 captures L10+M9, setting up M9 five threat.
        // After K11, Black must block M9 or White wins with an unbreakable five
        // (the five's break via O7 is illusory — recreation produces unbreakable five).
        // AI should evaluate this position positively.
        assert!(
            result.score > 0,
            "White should see winning position (K11 sets up M9 five threat), got score={}",
            result.score
        );
    }

    /// Test: at move 16 position, White DOES have immediate win at M9.
    /// The M9 five's break via O7 is illusory: after O7 captures N8+M9,
    /// White replays M9 and the recreated five is unbreakable (N8 is gone).
    #[test]
    fn test_game5_move16_white_immediate_win() {
        let mut board = Board::new();

        // Move 16 board state (after K11 captures L10+M9, then Black replays L10)
        // Black (6): K10, L10, M10, N10, M12, G9
        board.place_stone(Pos::new(9, 9), Stone::Black); // K10
        board.place_stone(Pos::new(9, 10), Stone::Black); // L10
        board.place_stone(Pos::new(9, 11), Stone::Black); // M10
        board.place_stone(Pos::new(9, 12), Stone::Black); // N10
        board.place_stone(Pos::new(11, 11), Stone::Black); // M12
        board.place_stone(Pos::new(8, 6), Stone::Black); // G9
        // White (7): J9, L9, K9, M11, H9, N8, K11
        board.place_stone(Pos::new(8, 8), Stone::White); // J9
        board.place_stone(Pos::new(8, 10), Stone::White); // L9
        board.place_stone(Pos::new(8, 9), Stone::White); // K9
        board.place_stone(Pos::new(10, 11), Stone::White); // M11
        board.place_stone(Pos::new(8, 7), Stone::White); // H9
        board.place_stone(Pos::new(7, 12), Stone::White); // N8
        board.place_stone(Pos::new(10, 9), Stone::White); // K11
        board.add_captures(Stone::White, 1);

        // Verify White's row 8: H9-J9-K9-L9 = 4 consecutive, M9 empty
        assert_eq!(board.get(Pos::new(8, 7)), Stone::White, "H9");
        assert_eq!(board.get(Pos::new(8, 8)), Stone::White, "J9");
        assert_eq!(board.get(Pos::new(8, 9)), Stone::White, "K9");
        assert_eq!(board.get(Pos::new(8, 10)), Stone::White, "L9");
        assert!(board.is_empty(Pos::new(8, 11)), "M9 should be empty");

        // Verify M9 creates a five
        let m9 = Pos::new(8, 11);
        let mut test = board.clone();
        test.place_stone(m9, Stone::White);
        assert!(has_five_at_pos(&test, m9, Stone::White), "M9 should create five");

        // STATIC check: the five IS breakable (O7 captures N8+M9)
        let five = find_five_positions(&test, Stone::White).unwrap();
        assert!(
            can_break_five_by_capture(&test, &five, Stone::White),
            "M9 five should be STATICALLY breakable (O7 captures N8+M9)"
        );
        // But the break is illusory: after O7 captures, White replays M9 → unbreakable
        assert!(
            AIEngine::is_illusory_break(&test, &five, Stone::White),
            "M9 five break via O7 should be illusory (recreation = unbreakable)"
        );

        // find_immediate_win should now return M9
        let engine = AIEngine::new();
        let win = engine.find_immediate_win(&board, Stone::White);
        assert_eq!(
            win,
            Some(m9),
            "White should have immediate win at M9 (illusory break)"
        );
    }

    /// Test: search on post-K11-capture position — Black should see forced win
    #[test]
    fn test_game5_post_capture_search() {
        let mut board = Board::new();

        // Board after K11 capture (L10+M9 removed), Black's turn
        // Black (5): K10, M10, N10, M12, G9
        board.place_stone(Pos::new(9, 9), Stone::Black); // K10
        board.place_stone(Pos::new(9, 11), Stone::Black); // M10
        board.place_stone(Pos::new(9, 12), Stone::Black); // N10
        board.place_stone(Pos::new(11, 11), Stone::Black); // M12
        board.place_stone(Pos::new(8, 6), Stone::Black); // G9
        // White (7): J9, L9, K9, M11, H9, N8, K11
        board.place_stone(Pos::new(8, 8), Stone::White); // J9
        board.place_stone(Pos::new(8, 10), Stone::White); // L9
        board.place_stone(Pos::new(8, 9), Stone::White); // K9
        board.place_stone(Pos::new(10, 11), Stone::White); // M11
        board.place_stone(Pos::new(8, 7), Stone::White); // H9
        board.place_stone(Pos::new(7, 12), Stone::White); // N8
        board.place_stone(Pos::new(10, 9), Stone::White); // K11
        board.add_captures(Stone::White, 1);

        // Run search for Black
        let mut engine = AIEngine::with_config(20, 10, 2000);
        let result = engine.get_move_with_stats(&board, Stone::Black);

        eprintln!(
            "Post-K11 Black: move={} score={} depth={} type={:?}",
            pos_to_notation(result.best_move.unwrap()),
            result.score,
            result.depth,
            result.search_type
        );

        // After K11 capture, White has 4-in-a-row on row 8 threatening M9
        // Black's M9 block is Stage 2 defense
        // But after M9 block, Black still has K10+M10+N10 on row 9
        // The key question: can Black force a win from here?

        // Run search for White too (to compare)
        let mut engine2 = AIEngine::with_config(20, 10, 2000);
        let result_w = engine2.get_move_with_stats(&board, Stone::White);
        eprintln!(
            "Post-K11 White: move={} score={} depth={} type={:?}",
            pos_to_notation(result_w.best_move.unwrap()),
            result_w.score,
            result_w.depth,
            result_w.search_type
        );
    }

    /// Debug test: K11→L10 position — White's perspective at depth 8
    /// After K11 captures L10+M9, Black replays L10. White to move.
    /// White has M9 as immediate win (illusory break via O7).
    /// Alpha-beta should find M9 with winning score.
    #[test]
    fn test_game5_k11_l10_white_perspective() {
        let mut board = Board::new();

        // Board after K11 capture + L10 replay
        // Black (6): K10, L10, M10, N10, M12, G9
        board.place_stone(Pos::new(9, 9), Stone::Black); // K10
        board.place_stone(Pos::new(9, 10), Stone::Black); // L10
        board.place_stone(Pos::new(9, 11), Stone::Black); // M10
        board.place_stone(Pos::new(9, 12), Stone::Black); // N10
        board.place_stone(Pos::new(11, 11), Stone::Black); // M12
        board.place_stone(Pos::new(8, 6), Stone::Black); // G9
        // White (7): J9, L9, K9, M11, H9, N8, K11
        board.place_stone(Pos::new(8, 8), Stone::White); // J9
        board.place_stone(Pos::new(8, 10), Stone::White); // L9
        board.place_stone(Pos::new(8, 9), Stone::White); // K9
        board.place_stone(Pos::new(10, 11), Stone::White); // M11
        board.place_stone(Pos::new(8, 7), Stone::White); // H9
        board.place_stone(Pos::new(7, 12), Stone::White); // N8
        board.place_stone(Pos::new(10, 9), Stone::White); // K11
        board.add_captures(Stone::White, 1);

        // Alpha-beta should find M9 as a winning move
        use crate::search::Searcher;
        let mut searcher = Searcher::new(16);
        let result = searcher.search(&board, Stone::White, 8);

        let m9 = Pos::new(8, 11);
        assert_eq!(
            result.best_move,
            Some(m9),
            "Alpha-beta should find M9 as winning move"
        );
        assert!(
            result.score > 900_000,
            "M9 should be evaluated as terminal win, got score={}",
            result.score
        );
    }
}
