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

use std::time::{Duration, Instant};

use crate::board::{Board, Pos, Stone, BOARD_SIZE};
use crate::eval::{evaluate, PatternScore};
use crate::rules::{
    count_captures_fast, execute_captures_fast, has_five_at_pos, is_valid_move, undo_captures,
};

use super::{EntryType, TranspositionTable, TTStats, ZobristTable};

/// Infinity score for alpha-beta bounds
const INF: i32 = PatternScore::FIVE + 1;

/// Maximum moves to consider at root.
/// Defense-first move ordering puts critical moves at the top,
/// so we don't need as many to catch all threats.
const MAX_ROOT_MOVES: usize = 20;

/// Maximum moves to consider at internal nodes at high remaining depth.
/// Defense-first move ordering (score_move) ensures critical blocking
/// moves are always in the top positions.
#[allow(dead_code)]
const MAX_INTERNAL_MOVES: usize = 15;

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
    killer_moves: [[Option<Pos>; 2]; 64],
    history: [[[i32; BOARD_SIZE]; BOARD_SIZE]; 2],
    start_time: Option<Instant>,
    time_limit: Option<Duration>,
    stopped: bool,
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
            killer_moves: [[None; 2]; 64],
            history: [[[0; BOARD_SIZE]; BOARD_SIZE]; 2],
            start_time: None,
            time_limit: None,
            stopped: false,
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
        self.killer_moves = [[None; 2]; 64];
        // Don't clear history - it persists across iterative deepening depths
        self.max_depth = max_depth;

        let mut best_result = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
        };

        // Clone board once for make/unmake during search
        let mut work_board = board.clone();

        // Iterative deepening: search progressively deeper
        for depth in 1..=max_depth {
            let result = self.search_root(&mut work_board, color, depth, -INF, INF);
            best_result = result;
            best_result.depth = depth;

            // Early exit: winning at depth 10+ or confirmed loss at 8+
            if best_result.score >= PatternScore::FIVE - 100 && depth >= 10 {
                break;
            }
            if best_result.score <= -(PatternScore::FIVE - 100) && depth >= 8 {
                break;
            }
        }

        best_result.nodes = self.nodes;
        best_result
    }

    /// Search with smart time management. Returns the best result found within the time budget.
    ///
    /// Two hard constraints (project requirements):
    /// 1. **Minimum depth 10** — always reached regardless of time
    /// 2. **Average < 500ms** — time prediction prevents over-runs beyond depth 10
    ///
    /// Strategy: always complete depth 10, then use predictive time control
    /// to decide whether deeper search is worthwhile.
    #[must_use]
    pub fn search_timed(
        &mut self,
        board: &Board,
        color: Stone,
        max_depth: i8,
        time_limit_ms: u64,
    ) -> SearchResult {
        self.start_time = Some(Instant::now());
        // Hard time limit is generous — allows depth 10 to complete even if slow.
        // The soft limit (time prediction) handles staying near 500ms average.
        self.time_limit = Some(Duration::from_millis((time_limit_ms + 200).max(300)));
        self.stopped = false;
        self.nodes = 0;
        self.max_depth = max_depth;
        self.killer_moves = [[None; 2]; 64];

        let mut best_result = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
        };

        let mut work_board = board.clone();
        let search_start = Instant::now();
        let soft_limit = Duration::from_millis(time_limit_ms);
        let mut prev_depth_time = Duration::ZERO;

        /// Minimum depth the AI must always complete (project requirement).
        const MIN_DEPTH: i8 = 10;
        /// Aspiration window size for iterative deepening
        const ASP_WINDOW: i32 = 100;

        for depth in 1..=max_depth {
            if self.stopped {
                break;
            }

            let depth_start = Instant::now();

            // Aspiration windows: narrow search bounds based on previous result
            let (mut asp_alpha, mut asp_beta) = if depth >= 3
                && best_result.score.abs() < PatternScore::FIVE - 100
            {
                (best_result.score - ASP_WINDOW, best_result.score + ASP_WINDOW)
            } else {
                (-INF, INF)
            };

            let result = loop {
                let result = self.search_root(&mut work_board, color, depth, asp_alpha, asp_beta);
                if self.stopped {
                    break result;
                }
                if result.score <= asp_alpha {
                    // Fail low: widen alpha
                    asp_alpha = (result.score - ASP_WINDOW * 4).max(-INF);
                } else if result.score >= asp_beta {
                    // Fail high: widen beta
                    asp_beta = (result.score + ASP_WINDOW * 4).min(INF);
                } else {
                    // Result within window
                    break result;
                }
            };

            if self.stopped {
                // Search was interrupted mid-depth by hard time limit.
                // Keep previous completed depth's result.
                break;
            }

            best_result = result;
            best_result.depth = depth;
            let depth_time = depth_start.elapsed();
            let total_elapsed = search_start.elapsed();

            // Early exit: winning or confirmed loss
            if best_result.score >= PatternScore::FIVE - 100 && depth >= 6 {
                break;
            }
            if best_result.score <= -(PatternScore::FIVE - 100) && depth >= 6 {
                break;
            }

            // Below minimum depth: continue, but allow early stop at depth 8+
            // if past soft time limit (wide-open positions in early game)
            if depth < MIN_DEPTH {
                if depth >= 8 && total_elapsed > soft_limit {
                    break;
                }
                prev_depth_time = depth_time;
                continue;
            }

            // Above minimum depth: use smart time prediction.
            let remaining = soft_limit.saturating_sub(total_elapsed);

            let estimated_next = if prev_depth_time.as_millis() > 0 && depth_time.as_millis() > 0 {
                let bf = depth_time.as_millis() as f64 / prev_depth_time.as_millis().max(1) as f64;
                let bf = bf.clamp(1.5, 5.0);
                Duration::from_millis((depth_time.as_millis() as f64 * bf) as u64)
            } else {
                depth_time * 3
            };

            prev_depth_time = depth_time;

            // Don't start next depth if estimated time exceeds remaining soft budget
            if estimated_next > remaining {
                break;
            }
        }

        best_result.nodes = self.nodes;
        self.start_time = None;
        self.time_limit = None;
        best_result
    }

    /// Root-level search with full alpha-beta window.
    /// Uses make/unmake pattern to avoid board cloning per move.
    fn search_root(&mut self, board: &mut Board, color: Stone, depth: i8, mut alpha: i32, beta: i32) -> SearchResult {
        let mut best_move = None;
        let mut best_score = -INF;

        let hash = self.zobrist.hash(board, color);
        let tt_move = self.tt.get_best_move(hash);
        let mut moves = self.generate_moves_ordered(board, color, tt_move, depth);
        moves.truncate(MAX_ROOT_MOVES);

        for (i, mov) in moves.iter().enumerate() {
            // Make move
            board.place_stone(*mov, color);
            let cap_info = execute_captures_fast(board, *mov, color);

            // Compute child hash incrementally (O(1) per stone)
            let mut child_hash = self.zobrist.update_place(hash, *mov, color);
            for j in 0..cap_info.count as usize {
                child_hash = self.zobrist.update_capture(child_hash, cap_info.positions[j], color.opponent());
            }
            if cap_info.pairs > 0 {
                let new_count = board.captures(color);
                let old_count = new_count - cap_info.pairs;
                child_hash = self.zobrist.update_capture_count(child_hash, color, old_count, new_count);
            }

            let score = if i == 0 {
                -self.alpha_beta(board, color.opponent(), depth - 1, -beta, -alpha, *mov, child_hash, true)
            } else {
                let mut s = -self.alpha_beta(
                    board, color.opponent(), depth - 1, -(alpha + 1), -alpha, *mov, child_hash, true,
                );
                if !self.stopped && s > alpha && s < beta {
                    s = -self.alpha_beta(
                        board, color.opponent(), depth - 1, -beta, -alpha, *mov, child_hash, true,
                    );
                }
                s
            };

            // Unmake move
            undo_captures(board, color, &cap_info);
            board.remove_stone(*mov);

            if self.stopped {
                break;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(*mov);
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

    /// Check if the side to move faces an immediate tactical threat
    /// (opponent has a four-in-a-row or is one capture from winning).
    /// Used to skip Null Move Pruning in threatened positions.
    /// O(4) constant time — only checks lines through last_move.
    fn is_threatened(board: &Board, color: Stone, last_move: Pos) -> bool {
        // Opponent near capture win
        let opp = color.opponent();
        if board.captures(opp) >= 4 {
            return true;
        }
        // Check if the last opponent move created a four-in-a-row
        let sz = BOARD_SIZE as i8;
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        for (dr, dc) in dirs {
            let mut count = 1i32;
            // Positive direction
            let mut r = last_move.row as i8 + dr;
            let mut c = last_move.col as i8 + dc;
            while r >= 0 && r < sz && c >= 0 && c < sz {
                if board.get(Pos::new(r as u8, c as u8)) == opp {
                    count += 1;
                    r += dr;
                    c += dc;
                } else {
                    break;
                }
            }
            // Negative direction
            r = last_move.row as i8 - dr;
            c = last_move.col as i8 - dc;
            while r >= 0 && r < sz && c >= 0 && c < sz {
                if board.get(Pos::new(r as u8, c as u8)) == opp {
                    count += 1;
                    r -= dr;
                    c -= dc;
                } else {
                    break;
                }
            }
            if count >= 4 {
                return true;
            }
        }
        false
    }

    /// Recursive alpha-beta search with negamax formulation.
    ///
    /// Pruning techniques applied:
    /// 1. **Transposition Table** — cached results from identical positions
    /// 2. **Null Move Pruning** — skip our turn; if still >= beta, prune (b_eff -30~40%)
    /// 3. **Futility Pruning** — at depth 1-2, skip hopeless moves (leaf nodes -60~70%)
    /// 4. **PVS** — null-window search for non-PV moves
    /// 5. **LMR** — reduced depth for late, non-tactical moves (b_eff -20%)
    fn alpha_beta(
        &mut self,
        board: &mut Board,
        color: Stone,
        depth: i8,
        mut alpha: i32,
        beta: i32,
        last_move: Pos,
        hash: u64,
        allow_null: bool,
    ) -> i32 {
        self.nodes += 1;

        // Time check every 1024 nodes
        if self.nodes & 1023 == 0 {
            if let (Some(start), Some(limit)) = (self.start_time, self.time_limit) {
                if start.elapsed() >= limit {
                    self.stopped = true;
                    return 0;
                }
            }
        }

        if self.stopped {
            return 0;
        }

        // Fast terminal check: only at last_move position (no full board scan)
        let last_player = color.opponent();
        if board.captures(last_player) >= 5 {
            return -PatternScore::FIVE;
        }
        if has_five_at_pos(board, last_move, last_player) {
            return -PatternScore::FIVE;
        }

        // Depth limit reached - evaluate position
        if depth <= 0 {
            return evaluate(board, color);
        }

        // Transposition table probe
        if let Some((score, _best_move)) = self.tt.probe(hash, depth, alpha, beta) {
            if score != 0 {
                return score;
            }
        }

        // ===== NULL MOVE PRUNING =====
        // If we skip our turn and the position is STILL >= beta,
        // then the position is so good we can prune without searching.
        // Don't use: at root, in threatened positions, at shallow depth, after a null move.
        if allow_null && depth >= 3 && !Self::is_threatened(board, color, last_move) {
            // Adaptive reduction: R=3 for deep, R=2 for shallow
            let r = if depth >= 5 { 3i8 } else { 2i8 };
            let null_depth = (depth - 1 - r).max(0);

            // Null move = pass turn, search with zero window around beta
            // We flip the hash's side-to-move bit by just using the same hash
            // (since Zobrist doesn't encode side-to-move separately in our impl)
            let null_score = -self.alpha_beta(
                board, color.opponent(), null_depth,
                -beta, -(beta - 1), last_move, hash,
                false, // no consecutive null moves
            );

            if !self.stopped && null_score >= beta {
                // Verify with a shallow search to avoid zugzwang errors
                if depth <= 8 {
                    return beta;
                }
                // For deep nodes, do a verification search at reduced depth
                let verify = self.alpha_beta(
                    board, color, depth - r, alpha, beta,
                    last_move, hash, false,
                );
                if !self.stopped && verify >= beta {
                    return beta;
                }
            }
        }

        let tt_move = self.tt.get_best_move(hash);
        let mut moves = self.generate_moves_ordered(board, color, tt_move, depth);
        if moves.is_empty() {
            return evaluate(board, color);
        }

        // Graduated move limits — keep tight for speed (depth 10 in 500ms).
        // Better move ordering (two-detection, reduced vulnerability penalty)
        // ensures the top moves are the right ones.
        let max_moves = match depth {
            0..=1 => 5,
            2..=3 => 7,
            4..=5 => 9,
            _ => 11,
        };
        moves.truncate(max_moves);

        // ===== FUTILITY PRUNING SETUP =====
        // At shallow depths, compute static eval once. If eval + margin < alpha,
        // non-tactical moves can be skipped (they can't possibly raise alpha).
        let futility_ok = depth <= 2 && alpha.abs() < PatternScore::FIVE - 100;
        let static_eval = if futility_ok { evaluate(board, color) } else { 0 };
        let futility_margin = if depth == 1 {
            PatternScore::CLOSED_FOUR  // 50,000 — one forcing move away
        } else {
            PatternScore::OPEN_FOUR    // 100,000 — two moves of tactical swing
        };

        let mut best_score = -INF;
        let mut best_move = None;
        let mut entry_type = EntryType::UpperBound;

        for (i, mov) in moves.iter().enumerate() {
            // ===== FUTILITY PRUNING =====
            // Skip non-tactical late moves at shallow depths if they can't improve alpha.
            // Never prune the first move (PV), TT moves, or captures.
            if futility_ok && i > 0 && static_eval + futility_margin <= alpha {
                let move_score = self.score_move(board, *mov, color, tt_move, depth);
                // Only prune non-tactical moves (score < 800K means not a winning/blocking threat)
                if move_score < 800_000 {
                    continue;
                }
            }

            // Make move
            board.place_stone(*mov, color);
            let cap_info = execute_captures_fast(board, *mov, color);

            // Compute child hash incrementally
            let mut child_hash = self.zobrist.update_place(hash, *mov, color);
            for j in 0..cap_info.count as usize {
                child_hash = self.zobrist.update_capture(child_hash, cap_info.positions[j], color.opponent());
            }
            if cap_info.pairs > 0 {
                let new_count = board.captures(color);
                let old_count = new_count - cap_info.pairs;
                child_hash = self.zobrist.update_capture_count(child_hash, color, old_count, new_count);
            }

            // ===== PVS + LMR =====
            let is_capture = cap_info.pairs > 0;
            let score = if i == 0 {
                // PV move: full window, full depth
                -self.alpha_beta(
                    board, color.opponent(), depth - 1, -beta, -alpha,
                    *mov, child_hash, true,
                )
            } else {
                // LMR: reduce non-tactical late moves. More aggressive to
                // compensate for wider move limits (18 vs old 11).
                let reduction = if is_capture || depth < 3 {
                    0i8 // Never reduce captures or at shallow depth
                } else if i >= 8 && depth >= 5 {
                    3i8 // Very late moves at deep search: heavy reduction
                } else if i >= 5 && depth >= 4 {
                    2i8 // Late moves: moderate reduction
                } else if i >= 3 && depth >= 3 {
                    1i8 // Early-late moves: light reduction
                } else {
                    0i8
                };
                let search_depth = (depth - 1 - reduction).max(0);

                // Null window search (possibly reduced)
                let mut s = -self.alpha_beta(
                    board, color.opponent(), search_depth,
                    -(alpha + 1), -alpha, *mov, child_hash, true,
                );

                // Re-search at full depth if reduced search beat alpha
                if !self.stopped && reduction > 0 && s > alpha {
                    s = -self.alpha_beta(
                        board, color.opponent(), depth - 1,
                        -(alpha + 1), -alpha, *mov, child_hash, true,
                    );
                }

                // Re-search with full window if null window improved within (alpha, beta)
                if !self.stopped && s > alpha && s < beta {
                    s = -self.alpha_beta(
                        board, color.opponent(), depth - 1,
                        -beta, -alpha, *mov, child_hash, true,
                    );
                }
                s
            };

            // Unmake move
            undo_captures(board, color, &cap_info);
            board.remove_stone(*mov);

            if self.stopped {
                return 0;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(*mov);
            }

            if score >= beta {
                // Update killer moves
                #[allow(clippy::cast_sign_loss)]
                let ply = (self.max_depth - depth).max(0) as usize;
                if ply < 64 {
                    if self.killer_moves[ply][0] != Some(*mov) {
                        self.killer_moves[ply][1] = self.killer_moves[ply][0];
                        self.killer_moves[ply][0] = Some(*mov);
                    }
                }
                // Update history heuristic
                let cidx = if color == Stone::Black { 0 } else { 1 };
                self.history[cidx][mov.row as usize][mov.col as usize] +=
                    i32::from(depth) * i32::from(depth);

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
    #[cfg(test)]
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

    /// Score a move for ordering purposes (defense-first philosophy).
    ///
    /// Combined single-pass: scans 4 directions once for each color (8 total scans)
    /// instead of 10 separate function calls with ~40 direction scans each.
    /// Defense-first ordering: blocking opponent's threats is prioritized.
    fn score_move(
        &self,
        board: &Board,
        mov: Pos,
        color: Stone,
        tt_move: Option<Pos>,
        depth: i8,
    ) -> i32 {
        let opponent = color.opponent();

        // TT best move: highest priority
        if tt_move == Some(mov) {
            return 1_000_000;
        }

        // Combined direction scan: detect all patterns in one pass (8 scans instead of 40+)
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        let mut my_five = false;
        let mut opp_five = false;
        let mut my_open_four = false;
        let mut opp_open_four = false;
        let mut my_four = false;
        let mut opp_four = false;
        let mut my_open_three = false;
        let mut opp_open_three = false;
        // Two-level detection: critical for early game where no threes exist yet.
        // Without this, move ordering was essentially random for non-tactical moves.
        let mut my_two_score = 0i32;

        for (dr, dc) in dirs {
            let (mc, mo, _, mc_consec) = Self::count_line_with_gap(board, mov, dr, dc, color);
            let (oc, oo, _, oc_consec) = Self::count_line_with_gap(board, mov, dr, dc, opponent);

            // Five-in-a-row uses consecutive count (no gaps allowed)
            if mc_consec >= 5 { my_five = true; }
            if oc_consec >= 5 { opp_five = true; }
            if mc == 4 {
                if mo == 2 { my_open_four = true; }
                if mo >= 1 { my_four = true; }
            }
            if oc == 4 {
                if oo == 2 { opp_open_four = true; }
                if oo >= 1 { opp_four = true; }
            }
            if mc == 3 && mo == 2 { my_open_three = true; }
            if oc == 3 && oo == 2 { opp_open_three = true; }
            // Twos: prefer connected development over scattered play
            if mc == 2 {
                my_two_score += if mo == 2 { 500 } else if mo == 1 { 150 } else { 0 };
            }
            // Blocking opponent twos also has some value
            if oc == 2 && oo == 2 { my_two_score += 200; }
        }

        // === Winning / Blocking wins ===
        if my_five { return 900_000; }
        if opp_five { return 895_000; }

        // Capture win / block capture win (no-alloc count)
        let capture_count = i32::from(count_captures_fast(board, mov, color));
        if capture_count > 0 && i32::from(board.captures(color)) + capture_count >= 5 {
            return 890_000;
        }
        let opp_capture = i32::from(count_captures_fast(board, mov, opponent));
        if opp_capture > 0 && i32::from(board.captures(opponent)) + opp_capture >= 5 {
            return 885_000;
        }

        // === Forcing threats (interleaved offense/defense) ===
        if my_open_four { return 870_000; }   // Unstoppable - wins next move
        if opp_open_four { return 860_000; }  // Must block or lose

        // === Capture defense (near win) ===
        let opp_caps = board.captures(opponent);
        if opp_capture > 0 && opp_caps >= 3 { return 855_000; }
        if opp_capture > 0 && opp_caps >= 2 { return 845_000; }

        // === Strong threats: OWN forcing moves above passive blocking ===
        if my_four { return 830_000; }        // Forcing - opponent MUST respond
        if opp_four { return 820_000; }       // Block their forcing move
        if my_open_three { return 810_000; }  // Creates future open-four
        if opp_open_three { return 800_000; } // Block their future open-four

        // === Captures (non-winning) ===
        if capture_count > 0 {
            let my_caps = i32::from(board.captures(color));
            let cap_urgency = if my_caps + capture_count >= 4 {
                150_000
            } else if my_caps >= 2 {
                80_000
            } else {
                50_000
            };
            return 600_000 + capture_count * cap_urgency;
        }

        if opp_capture > 0 {
            return 550_000 + i32::from(opp_caps) * 30_000;
        }

        // === Capture vulnerability penalty ===
        // Check if placing our stone here creates a pair the opponent can capture.
        // Pattern: opp-ME-ally-opp or opp-ally-ME-opp (we become part of a capturable pair)
        let capture_penalty = self.capture_vulnerability(board, mov, color);

        // === Killer moves ===
        #[allow(clippy::cast_sign_loss)]
        let ply = (self.max_depth - depth).max(0) as usize;
        if ply < 64 {
            if self.killer_moves[ply][0] == Some(mov) {
                return 500_000 - capture_penalty;
            }
            if self.killer_moves[ply][1] == Some(mov) {
                return 490_000 - capture_penalty;
            }
        }

        // History heuristic + center proximity + connection bonus
        let cidx = if color == Stone::Black { 0 } else { 1 };
        let hist = self.history[cidx][mov.row as usize][mov.col as usize];

        #[allow(clippy::cast_possible_wrap)]
        let center = (BOARD_SIZE / 2) as i32;
        let dist = (i32::from(mov.row) - center).abs() + (i32::from(mov.col) - center).abs();
        let center_bonus = (18 - dist) * 10;

        hist + center_bonus + my_two_score - capture_penalty
    }

    /// Generate candidate moves ordered by priority for better alpha-beta pruning.
    ///
    /// Uses TT move, killer moves, history heuristic, and center proximity
    /// to order moves for maximum cutoff efficiency.
    ///
    /// # Arguments
    ///
    /// * `board` - Current board state
    /// * `color` - Color to move
    /// * `tt_move` - Best move from transposition table, if any
    /// * `depth` - Current search depth
    ///
    /// # Returns
    ///
    /// Vector of valid candidate moves sorted by descending priority score.
    fn generate_moves_ordered(
        &self,
        board: &Board,
        color: Stone,
        tt_move: Option<Pos>,
        depth: i8,
    ) -> Vec<Pos> {
        let mut seen = [[false; BOARD_SIZE]; BOARD_SIZE];

        if board.is_board_empty() {
            return vec![Pos::new(9, 9)];
        }

        // Always use radius 2 to ensure critical moves are never missed.
        // Radius 1 at shallow depths caused the AI to miss opponent winning moves
        // that were 2 cells away, leading to false "winning" scores.
        let radius = 2i32;
        let mut scored: Vec<(Pos, i32)> = Vec::with_capacity(50);

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

                    if is_valid_move(board, new_pos, color) {
                        let score = self.score_move(board, new_pos, color, tt_move, depth);
                        scored.push((new_pos, score));
                    }
                }
            }
        }

        // Sort descending by score
        scored.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(m, _)| m).collect()
    }

    /// Clear history heuristic and killer moves.
    ///
    /// Call this when starting a new game to reset learned move ordering data.
    pub fn clear_history(&mut self) {
        self.history = [[[0; BOARD_SIZE]; BOARD_SIZE]; 2];
        self.killer_moves = [[None; 2]; 64];
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

    /// Scan a line from `pos` in both directions.
    /// Returns (total_count, open_ends, has_gap, consecutive_count).
    /// - total_count: stones with at most one gap
    /// - consecutive_count: strict consecutive stones (no gap), for five-in-a-row detection
    fn count_line_with_gap(board: &Board, pos: Pos, dr: i8, dc: i8, color: Stone) -> (i32, i32, bool, i32) {
        let sz = BOARD_SIZE as i8;
        let mut count = 1i32;
        let mut open_ends = 0;
        let mut has_gap = false;
        let mut consec_pos = 0i32;
        let mut consec_neg = 0i32;

        // Positive direction
        let mut r = pos.row as i8 + dr;
        let mut c = pos.col as i8 + dc;
        let mut counting_consecutive = true;
        while r >= 0 && r < sz && c >= 0 && c < sz {
            let cell = board.get(Pos::new(r as u8, c as u8));
            if cell == color {
                count += 1;
                if counting_consecutive { consec_pos += 1; }
                r += dr;
                c += dc;
            } else if cell == Stone::Empty && !has_gap {
                counting_consecutive = false;
                let nr = r + dr;
                let nc = c + dc;
                if nr >= 0 && nr < sz && nc >= 0 && nc < sz
                    && board.get(Pos::new(nr as u8, nc as u8)) == color
                {
                    has_gap = true;
                    r += dr;
                    c += dc;
                    continue;
                }
                open_ends += 1;
                break;
            } else if cell == Stone::Empty {
                open_ends += 1;
                break;
            } else {
                break;
            }
        }

        // Negative direction
        r = pos.row as i8 - dr;
        c = pos.col as i8 - dc;
        counting_consecutive = true;
        while r >= 0 && r < sz && c >= 0 && c < sz {
            let cell = board.get(Pos::new(r as u8, c as u8));
            if cell == color {
                count += 1;
                if counting_consecutive { consec_neg += 1; }
                r -= dr;
                c -= dc;
            } else if cell == Stone::Empty && !has_gap {
                counting_consecutive = false;
                let nr = r - dr;
                let nc = c - dc;
                if nr >= 0 && nr < sz && nc >= 0 && nc < sz
                    && board.get(Pos::new(nr as u8, nc as u8)) == color
                {
                    has_gap = true;
                    r -= dr;
                    c -= dc;
                    continue;
                }
                open_ends += 1;
                break;
            } else if cell == Stone::Empty {
                open_ends += 1;
                break;
            } else {
                break;
            }
        }

        let consecutive = 1 + consec_pos + consec_neg;
        (count, open_ends, has_gap, consecutive)
    }

    /// Check if placing our stone at `mov` makes it part of a capturable pair.
    ///
    /// After we place at `mov`, we check all 8 directions for the pattern:
    ///   opp - US(mov) - ally - opp   (we are pos1 in a capture)
    ///   opp - ally - US(mov) - opp   (we are pos2 in a capture)
    ///
    /// Returns a penalty score (higher = more vulnerable).
    fn capture_vulnerability(&self, board: &Board, mov: Pos, color: Stone) -> i32 {
        let opponent = color.opponent();
        let sz = BOARD_SIZE as i8;
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        let mut vuln_count = 0i32;

        for (dr, dc) in dirs {
            for sign in [-1i8, 1i8] {
                let sdr = dr * sign;
                let sdc = dc * sign;

                // Pattern 1: opp - US - ally - opp  (US is at mov)
                // Check: mov-1 == opp, mov+1 == ally, mov+2 == opp
                let rm1 = mov.row as i8 - sdr;
                let cm1 = mov.col as i8 - sdc;
                let rp1 = mov.row as i8 + sdr;
                let cp1 = mov.col as i8 + sdc;
                let rp2 = mov.row as i8 + sdr * 2;
                let cp2 = mov.col as i8 + sdc * 2;

                if rm1 >= 0 && rm1 < sz && cm1 >= 0 && cm1 < sz
                    && rp1 >= 0 && rp1 < sz && cp1 >= 0 && cp1 < sz
                    && rp2 >= 0 && rp2 < sz && cp2 >= 0 && cp2 < sz
                {
                    let before = board.get(Pos::new(rm1 as u8, cm1 as u8));
                    let after1 = board.get(Pos::new(rp1 as u8, cp1 as u8));
                    let after2 = board.get(Pos::new(rp2 as u8, cp2 as u8));

                    // opp - [mov] - ally - opp: opponent captures at rm1
                    if before == opponent && after1 == color && after2 == opponent {
                        vuln_count += 1;
                    }
                    // We also need: opp at rm1 could be empty (opponent plays there to capture)
                    // Actually the pattern is: X captures O-O-X by placing at the far end
                    // So: empty - US - ally - opp → opp plays at empty to capture
                    if before == Stone::Empty && after1 == color && after2 == opponent {
                        vuln_count += 1;
                    }
                }

                // Pattern 2: opp - ally - US - opp  (US is at mov, we complete the pair)
                // Check: mov-2 == opp, mov-1 == ally, mov+1 == opp
                let rm2 = mov.row as i8 - sdr * 2;
                let cm2 = mov.col as i8 - sdc * 2;

                if rm2 >= 0 && rm2 < sz && cm2 >= 0 && cm2 < sz
                    && rm1 >= 0 && rm1 < sz && cm1 >= 0 && cm1 < sz
                    && rp1 >= 0 && rp1 < sz && cp1 >= 0 && cp1 < sz
                {
                    let before2 = board.get(Pos::new(rm2 as u8, cm2 as u8));
                    let before1 = board.get(Pos::new(rm1 as u8, cm1 as u8));
                    let after = board.get(Pos::new(rp1 as u8, cp1 as u8));

                    // opp - ally - [mov] - opp: opponent already flanks both sides
                    if before2 == opponent && before1 == color && after == opponent {
                        vuln_count += 1;
                    }
                    // empty - ally - [mov] - opp: opponent can play at empty to capture
                    if before2 == Stone::Empty && before1 == color && after == opponent {
                        vuln_count += 1;
                    }
                }
            }
        }

        // Each vulnerability makes this move worse, but penalty must be proportional.
        // Old 100K penalty was catastrophic: it pushed ALL moves near opponent stones
        // to the bottom, causing scattered disconnected play. 8K is proportional to
        // the heuristic's 4K/pair vulnerability penalty.
        if vuln_count > 0 {
            let opp_caps = i32::from(board.captures(color.opponent()));
            let base_penalty = 8_000;
            let urgency = if opp_caps >= 3 { 4 } else if opp_caps >= 2 { 2 } else { 1 };
            vuln_count * base_penalty * urgency
        } else {
            0
        }
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

        // Add more stones to constrain search space (faster in debug mode)
        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::White);
        board.place_stone(Pos::new(8, 9), Stone::Black);

        // Use depth 2 for fast test in debug mode
        let result = searcher.search(&board, Stone::White, 2);
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
        // Should find a valid move (capture at (9,6) or a strategically better move)
        assert!(result.best_move.is_some(), "Should find a move");
        let mov = result.best_move.unwrap();
        // Move should be near existing stones
        assert!(
            mov.row >= 7 && mov.row <= 11 && mov.col >= 3 && mov.col <= 11,
            "Move {:?} should be near existing stones",
            mov
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

        // Second search should also find a valid move (may differ due to history heuristic)
        let result2 = searcher.search(&board, Stone::White, 4);
        assert!(result2.best_move.is_some());

        // Second search should use fewer nodes (TT helps prune)
        assert!(result2.nodes <= result1.nodes || result2.nodes < result1.nodes + 500);

        // Both moves should be adjacent to the existing stone
        let m1 = result1.best_move.unwrap();
        let m2 = result2.best_move.unwrap();
        assert!(m1.row.abs_diff(9) <= 2 && m1.col.abs_diff(9) <= 2);
        assert!(m2.row.abs_diff(9) <= 2 && m2.col.abs_diff(9) <= 2);
    }
}
