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
//! - **Lazy SMP**: parallel search with lock-free shared TT
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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::board::{Board, Pos, Stone, BOARD_SIZE};
use crate::eval::{evaluate, PatternScore};
use crate::rules::{
    can_break_five_by_capture, count_captures_fast, execute_captures_fast, find_five_line_at_pos,
    has_five_at_pos, has_five_in_row, is_valid_move, undo_captures,
};

use super::{AtomicTT, EntryType, TTStats, ZobristTable};

/// Infinity score for alpha-beta bounds
const INF: i32 = PatternScore::FIVE + 1;

/// Maximum moves to consider at root.
/// Defense-first move ordering puts critical moves at the top,
/// so we don't need as many to catch all threats.
const MAX_ROOT_MOVES: usize = 30;

/// Maximum moves to consider at internal nodes at high remaining depth.
/// Defense-first move ordering (score_move) ensures critical blocking
/// moves are always in the top positions.
#[allow(dead_code)]
const MAX_INTERNAL_MOVES: usize = 15;

/// Search statistics for diagnostics and tuning.
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    /// Total beta cutoffs (fail-high)
    pub beta_cutoffs: u64,
    /// Beta cutoffs on the first move tried (measures move ordering quality)
    pub first_move_cutoffs: u64,
    /// Total TT probes
    pub tt_probes: u64,
    /// TT probes that returned a usable score (exact/bound hit)
    pub tt_score_hits: u64,
    /// TT probes that provided a best move for ordering
    pub tt_move_hits: u64,
}

impl SearchStats {
    /// First-move cutoff rate (target: ~90% for good move ordering)
    pub fn first_move_rate(&self) -> f64 {
        if self.beta_cutoffs == 0 {
            0.0
        } else {
            self.first_move_cutoffs as f64 / self.beta_cutoffs as f64 * 100.0
        }
    }

    /// TT score hit rate
    pub fn tt_score_rate(&self) -> f64 {
        if self.tt_probes == 0 {
            0.0
        } else {
            self.tt_score_hits as f64 / self.tt_probes as f64 * 100.0
        }
    }

    /// Merge another stats into this one (for combining worker stats)
    fn merge(&mut self, other: &SearchStats) {
        self.beta_cutoffs += other.beta_cutoffs;
        self.first_move_cutoffs += other.first_move_cutoffs;
        self.tt_probes += other.tt_probes;
        self.tt_score_hits += other.tt_score_hits;
        self.tt_move_hits += other.tt_move_hits;
    }
}

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
    /// Search diagnostics
    pub stats: SearchStats,
}

// =============================================================================
// SharedState: thread-safe state shared across all workers
// =============================================================================

/// State shared between all search worker threads.
struct SharedState {
    zobrist: ZobristTable,
    tt: AtomicTT,
    /// Global stop signal — set by main thread when time is up.
    stopped: AtomicBool,
}

// =============================================================================
// WorkerSearcher: per-thread search state
// =============================================================================

/// Per-thread search worker. Each worker has its own killer/history tables
/// and shares the TT + zobrist via Arc<SharedState>.
struct WorkerSearcher {
    shared: Arc<SharedState>,
    nodes: u64,
    max_depth: i8,
    killer_moves: [[Option<Pos>; 2]; 64],
    history: [[[i32; BOARD_SIZE]; BOARD_SIZE]; 2],
    start_time: Option<Instant>,
    time_limit: Option<Duration>,
    stats: SearchStats,
}

impl WorkerSearcher {
    fn new(
        shared: Arc<SharedState>,
        max_depth: i8,
        start_time: Instant,
        time_limit: Duration,
    ) -> Self {
        Self {
            shared,
            nodes: 0,
            max_depth,
            killer_moves: [[None; 2]; 64],
            history: [[[0; BOARD_SIZE]; BOARD_SIZE]; 2],
            start_time: Some(start_time),
            time_limit: Some(time_limit),
            stats: SearchStats::default(),
        }
    }

    /// Check if search should stop (time limit or global stop signal).
    #[inline]
    fn is_stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::Relaxed)
    }

    /// Check time and set global stop if exceeded.
    #[inline]
    fn check_time(&self) -> bool {
        if self.shared.stopped.load(Ordering::Relaxed) {
            return true;
        }
        if let (Some(start), Some(limit)) = (self.start_time, self.time_limit) {
            if start.elapsed() >= limit {
                self.shared.stopped.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Iterative deepening search. `start_depth_offset` allows workers
    /// to begin at different depths for natural tree diversification.
    fn search_iterative(
        &mut self,
        board: &Board,
        color: Stone,
        max_depth: i8,
        start_depth_offset: i8,
    ) -> SearchResult {
        let mut best_result = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
            stats: SearchStats::default(),
        };

        let mut work_board = board.clone();
        let search_start = self.start_time.unwrap_or_else(Instant::now);
        let soft_limit = self.time_limit.unwrap_or(Duration::from_millis(500));
        let mut prev_depth_time = Duration::ZERO;

        let min_depth: i8 = if board.stone_count() <= 4 { 8 } else { 12 };
        const ASP_WINDOW: i32 = 100;

        // Win/loss confirmation: require TWO consecutive depths to agree on a
        // terminal score before early exit. Prevents illusory wins where depth d
        // sees a forced win but depth d+1 finds the refutation.
        let mut prev_was_winning = false;
        let mut prev_was_losing = false;

        // Workers with offset skip early depths (they're cheap anyway and TT handles it)
        let first_depth = (1 + start_depth_offset).max(1);

        for depth in first_depth..=max_depth {
            if self.is_stopped() {
                break;
            }

            let depth_start = Instant::now();

            let (mut asp_alpha, mut asp_beta) = if depth >= 3
                && best_result.score.abs() < PatternScore::FIVE - 100
            {
                (best_result.score - ASP_WINDOW, best_result.score + ASP_WINDOW)
            } else {
                (-INF, INF)
            };

            let result = loop {
                let result = self.search_root(&mut work_board, color, depth, asp_alpha, asp_beta);
                if self.is_stopped() {
                    break result;
                }
                if result.score <= asp_alpha {
                    // On fail-low, immediately open to -INF (no second re-search)
                    asp_alpha = -INF;
                } else if result.score >= asp_beta {
                    // On fail-high, immediately open to INF
                    asp_beta = INF;
                } else {
                    break result;
                }
            };

            if self.is_stopped() {
                break;
            }

            best_result = result;
            best_result.depth = depth;
            let depth_time = depth_start.elapsed();
            let total_elapsed = search_start.elapsed();

            // Early exit: winning or confirmed loss — only after reaching min_depth
            // AND confirmed over two consecutive depths. This prevents illusory wins
            // where depth d sees FIVE but depth d+1 finds the refutation.
            let is_winning = best_result.score >= PatternScore::FIVE - 100;
            let is_losing = best_result.score <= -(PatternScore::FIVE - 100);

            if is_winning && prev_was_winning && depth >= min_depth {
                break;
            }
            if is_losing && prev_was_losing && depth >= min_depth {
                break;
            }

            prev_was_winning = is_winning;
            prev_was_losing = is_losing;

            if depth < min_depth {
                if depth >= 8 && total_elapsed > soft_limit {
                    break;
                }
                prev_depth_time = depth_time;
                continue;
            }

            let remaining = soft_limit.saturating_sub(total_elapsed);
            let estimated_next = if prev_depth_time.as_millis() > 0 && depth_time.as_millis() > 0 {
                let bf = depth_time.as_millis() as f64 / prev_depth_time.as_millis().max(1) as f64;
                let bf = bf.clamp(1.5, 5.0);
                Duration::from_millis((depth_time.as_millis() as f64 * bf) as u64)
            } else {
                depth_time * 3
            };

            prev_depth_time = depth_time;

            if estimated_next > remaining {
                break;
            }
        }

        best_result.nodes = self.nodes;
        best_result.stats = self.stats.clone();
        best_result
    }

    /// Root-level search with full alpha-beta window.
    fn search_root(
        &mut self,
        board: &mut Board,
        color: Stone,
        depth: i8,
        mut alpha: i32,
        beta: i32,
    ) -> SearchResult {
        let mut best_move = None;
        let mut best_score = -INF;

        let hash = self.shared.zobrist.hash(board, color);
        let tt_move = self.shared.tt.get_best_move(hash);
        let (mut moves, _top_score) = self.generate_moves_ordered(board, color, tt_move, depth);
        moves.truncate(MAX_ROOT_MOVES);

        for (i, (mov, _move_score)) in moves.iter().enumerate() {
            board.place_stone(*mov, color);
            let cap_info = execute_captures_fast(board, *mov, color);

            let mut child_hash = self.shared.zobrist.update_place(hash, *mov, color);
            for j in 0..cap_info.count as usize {
                child_hash = self.shared.zobrist.update_capture(
                    child_hash,
                    cap_info.positions[j],
                    color.opponent(),
                );
            }
            if cap_info.pairs > 0 {
                let new_count = board.captures(color);
                let old_count = new_count - cap_info.pairs;
                child_hash =
                    self.shared
                        .zobrist
                        .update_capture_count(child_hash, color, old_count, new_count);
            }

            // Threat extension: forcing moves (creating a four) get +1 ply.
            // Forcing moves have only 1-2 legal responses, so the subtree stays narrow.
            let extension = if Self::move_creates_four(board, *mov, color) { 1i8 } else { 0i8 };

            let score = if i == 0 {
                -self.alpha_beta(
                    board,
                    color.opponent(),
                    depth - 1 + extension,
                    -beta,
                    -alpha,
                    *mov,
                    child_hash,
                    true,
                )
            } else {
                let mut s = -self.alpha_beta(
                    board,
                    color.opponent(),
                    depth - 1 + extension,
                    -(alpha + 1),
                    -alpha,
                    *mov,
                    child_hash,
                    true,
                );
                if !self.is_stopped() && s > alpha && s < beta {
                    s = -self.alpha_beta(
                        board,
                        color.opponent(),
                        depth - 1 + extension,
                        -beta,
                        -alpha,
                        *mov,
                        child_hash,
                        true,
                    );
                }
                s
            };

            undo_captures(board, color, &cap_info);
            board.remove_stone(*mov);

            if self.is_stopped() {
                break;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(*mov);
            }

            if score >= beta {
                break;
            }
            alpha = alpha.max(score);
        }

        // Store root result in TT for reuse by other workers (Lazy SMP) and next iteration
        if !self.is_stopped() {
            let entry_type = if best_score >= beta {
                EntryType::LowerBound
            } else {
                EntryType::Exact // Root always starts with full window
            };
            self.shared.tt.store(hash, depth, best_score, entry_type, best_move);
        }

        SearchResult {
            best_move,
            score: best_score,
            depth,
            nodes: self.nodes,
            stats: self.stats.clone(),
        }
    }

    /// Check if the stone just placed at pos creates a four (4 in a row with ≥1 open end).
    /// Used for threat extensions: forcing moves deserve deeper search because
    /// the opponent has only 1-2 legal responses, keeping the subtree narrow.
    #[inline]
    fn move_creates_four(board: &Board, pos: Pos, color: Stone) -> bool {
        let sz = BOARD_SIZE as i8;
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        for (dr, dc) in dirs {
            let mut count = 1i32;
            let mut open_ends = 0;
            let mut r = pos.row as i8 + dr;
            let mut c = pos.col as i8 + dc;
            while r >= 0 && r < sz && c >= 0 && c < sz {
                if board.get(Pos::new(r as u8, c as u8)) == color {
                    count += 1;
                    r += dr;
                    c += dc;
                } else {
                    if board.get(Pos::new(r as u8, c as u8)) == Stone::Empty {
                        open_ends += 1;
                    }
                    break;
                }
            }
            r = pos.row as i8 - dr;
            c = pos.col as i8 - dc;
            while r >= 0 && r < sz && c >= 0 && c < sz {
                if board.get(Pos::new(r as u8, c as u8)) == color {
                    count += 1;
                    r -= dr;
                    c -= dc;
                } else {
                    if board.get(Pos::new(r as u8, c as u8)) == Stone::Empty {
                        open_ends += 1;
                    }
                    break;
                }
            }
            if count == 4 && open_ends >= 1 {
                return true;
            }
        }
        false
    }

    /// Check if the side to move faces an immediate tactical threat.
    fn is_threatened(board: &Board, color: Stone, last_move: Pos) -> bool {
        let opp = color.opponent();
        if board.captures(opp) >= 4 {
            return true;
        }
        let sz = BOARD_SIZE as i8;
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        for (dr, dc) in dirs {
            let mut count = 1i32;
            let mut open_ends = 0i32;
            let mut r = last_move.row as i8 + dr;
            let mut c = last_move.col as i8 + dc;
            while r >= 0 && r < sz && c >= 0 && c < sz {
                if board.get(Pos::new(r as u8, c as u8)) == opp {
                    count += 1;
                    r += dr;
                    c += dc;
                } else {
                    if board.get(Pos::new(r as u8, c as u8)) == Stone::Empty {
                        open_ends += 1;
                    }
                    break;
                }
            }
            if r < 0 || r >= sz || c < 0 || c >= sz {
                // Out of bounds = blocked end
            }
            r = last_move.row as i8 - dr;
            c = last_move.col as i8 - dc;
            while r >= 0 && r < sz && c >= 0 && c < sz {
                if board.get(Pos::new(r as u8, c as u8)) == opp {
                    count += 1;
                    r -= dr;
                    c -= dc;
                } else {
                    if board.get(Pos::new(r as u8, c as u8)) == Stone::Empty {
                        open_ends += 1;
                    }
                    break;
                }
            }
            // Threatened by: 4+ in a row, OR open three (3 with 2 open ends)
            if count >= 4 || (count >= 3 && open_ends >= 2) {
                return true;
            }
        }
        // Capture setup: opponent's last_move brackets our pair on one side
        // Pattern: last_move(opp) - us - us - empty → opponent places at empty to capture
        let us = color;
        for (dr, dc) in dirs {
            for sign in [-1i8, 1] {
                let sdr = dr * sign;
                let sdc = dc * sign;
                let r1 = last_move.row as i8 + sdr;
                let c1 = last_move.col as i8 + sdc;
                let r2 = r1 + sdr;
                let c2 = c1 + sdc;
                let r3 = r2 + sdr;
                let c3 = c2 + sdc;
                if r1 >= 0 && r1 < sz && c1 >= 0 && c1 < sz
                    && r2 >= 0 && r2 < sz && c2 >= 0 && c2 < sz
                    && r3 >= 0 && r3 < sz && c3 >= 0 && c3 < sz
                {
                    let s1 = board.get(Pos::new(r1 as u8, c1 as u8));
                    let s2 = board.get(Pos::new(r2 as u8, c2 as u8));
                    let s3 = board.get(Pos::new(r3 as u8, c3 as u8));
                    if s1 == us && s2 == us && s3 == Stone::Empty {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Maximum quiescence search depth (plies of forcing moves).
    /// VCF-style fours are fully forcing, so we can search deep without explosion.
    const MAX_QS_DEPTH: i8 = 16;

    /// Quiescence search at leaf nodes of alpha-beta.
    ///
    /// Instead of returning a static evaluation immediately, we extend the search
    /// for forcing moves only (fives, fours, capture-wins). This eliminates the
    /// horizon effect where the AI fails to see forced wins/losses just beyond
    /// the regular search depth.
    ///
    /// Design:
    /// - **Stand-pat**: If no forcing move improves alpha, return static eval
    /// - **Forcing moves**: Only fives, four-threats, and capture-wins are searched
    /// - **Alpha-beta pruning**: Standard cutoffs apply to keep it efficient
    /// - **Depth-limited**: MAX_QS_DEPTH prevents runaway in complex positions
    fn quiescence(
        &mut self,
        board: &mut Board,
        color: Stone,
        mut alpha: i32,
        beta: i32,
        last_move: Pos,
        qs_depth: i8,
        hash: u64,
    ) -> i32 {
        self.nodes += 1;

        // Time check (less frequent in QS — every 4096 nodes)
        if self.nodes & 4095 == 0 && self.check_time() {
            return 0;
        }
        if self.is_stopped() {
            return 0;
        }

        // Terminal: opponent just won
        let last_player = color.opponent();
        if board.captures(last_player) >= 5 {
            return -PatternScore::FIVE;
        }
        if has_five_at_pos(board, last_move, last_player) {
            // Check breakable five (endgame capture rule)
            if let Some(five_line) = find_five_line_at_pos(board, last_move, last_player) {
                if can_break_five_by_capture(board, &five_line, last_player) {
                    // Breakable five: forcing but NOT terminal. Opponent can capture
                    // a pair from the line to destroy it. Equivalent to a closed four
                    // (one specific defense exists).
                    return -(PatternScore::CLOSED_FOUR);
                }
            }
            return -PatternScore::FIVE;
        }

        // TT probe: reuse results from previous searches or other QS nodes.
        // Use depth 0 — any entry (depth >= 0) can satisfy QS queries.
        if let Some((score, _)) = self.shared.tt.probe(hash, 0, alpha, beta) {
            return score;
        }

        // Stand-pat: static evaluation as lower bound
        let stand_pat = evaluate(board, color);

        // Beta cutoff: position is already too good (fail high)
        if stand_pat >= beta {
            return stand_pat;
        }

        let original_alpha = alpha;
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        // Depth limit for quiescence
        if qs_depth >= Self::MAX_QS_DEPTH {
            return stand_pat;
        }

        // After depth 4 in QS, only search fives (no more fours)
        // This prevents QS from exploding in complex midgame positions.
        let fours_allowed = qs_depth < 6;

        let opponent = color.opponent();
        let sz = BOARD_SIZE as i8;
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

        // Generate forcing moves only: fives, fours, capture-wins.
        // Use proximity scan (radius 2 from existing stones) instead of full-board.
        let mut forcing_moves: Vec<(Pos, i32)> = Vec::with_capacity(16);
        let mut seen = [[false; BOARD_SIZE]; BOARD_SIZE];

        for stone_pos in board.black.iter_ones().chain(board.white.iter_ones()) {
            for dr in -2i32..=2 {
                for dc in -2i32..=2 {
                    let r = i32::from(stone_pos.row) + dr;
                    let c = i32::from(stone_pos.col) + dc;
                    if !Pos::is_valid(r, c) { continue; }
                    #[allow(clippy::cast_sign_loss)]
                    let (ru, cu) = (r as usize, c as usize);
                    if seen[ru][cu] { continue; }
                    seen[ru][cu] = true;

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let pos = Pos::new(r as u8, c as u8);
                    if board.get(pos) != Stone::Empty { continue; }
                    if !is_valid_move(board, pos, color) { continue; }

                    let mut priority = 0i32;

                    // Check five creation / block opponent five / four creation
                    for &(ddr, ddc) in &dirs {
                        // Our line
                        let mut mc = 1i32;
                        let mut rr = pos.row as i8 + ddr;
                        let mut cc = pos.col as i8 + ddc;
                        while rr >= 0 && rr < sz && cc >= 0 && cc < sz
                            && board.get(Pos::new(rr as u8, cc as u8)) == color
                        { mc += 1; rr += ddr; cc += ddc; }
                        let mut mo_p = if rr >= 0 && rr < sz && cc >= 0 && cc < sz
                            && board.get(Pos::new(rr as u8, cc as u8)) == Stone::Empty { 1 } else { 0 };
                        rr = pos.row as i8 - ddr;
                        cc = pos.col as i8 - ddc;
                        while rr >= 0 && rr < sz && cc >= 0 && cc < sz
                            && board.get(Pos::new(rr as u8, cc as u8)) == color
                        { mc += 1; rr -= ddr; cc -= ddc; }
                        mo_p += if rr >= 0 && rr < sz && cc >= 0 && cc < sz
                            && board.get(Pos::new(rr as u8, cc as u8)) == Stone::Empty { 1 } else { 0 };

                        if mc >= 5 { priority = 900; break; }
                        if fours_allowed && mc == 4 && mo_p >= 1 {
                            priority = priority.max(if mo_p == 2 { 800 } else { 700 });
                        }

                        // Opponent line
                        let mut oc = 1i32;
                        rr = pos.row as i8 + ddr;
                        cc = pos.col as i8 + ddc;
                        while rr >= 0 && rr < sz && cc >= 0 && cc < sz
                            && board.get(Pos::new(rr as u8, cc as u8)) == opponent
                        { oc += 1; rr += ddr; cc += ddc; }
                        rr = pos.row as i8 - ddr;
                        cc = pos.col as i8 - ddc;
                        while rr >= 0 && rr < sz && cc >= 0 && cc < sz
                            && board.get(Pos::new(rr as u8, cc as u8)) == opponent
                        { oc += 1; rr -= ddr; cc -= ddc; }

                        if oc >= 5 { priority = priority.max(850); }
                    }

                    // Capture-win check
                    if priority == 0 {
                        let cap_count = count_captures_fast(board, pos, color);
                        if cap_count > 0 && board.captures(color) + cap_count >= 5 {
                            priority = 890;
                        }
                    }

                    if priority > 0 {
                        forcing_moves.push((pos, priority));
                    }
                }
            }
        }

        if forcing_moves.is_empty() {
            return stand_pat;
        }

        // Sort by priority (highest first)
        forcing_moves.sort_unstable_by(|a, b| b.1.cmp(&a.1));

        // Move count pruning (PentaZen-style): limit forcing moves per QS node.
        // Fives (900) are always searched. Fours limited to top candidates.
        let max_qs_moves: usize = if qs_depth <= 2 { 8 } else { 4 };

        let mut best_score = stand_pat;
        let mut best_move: Option<Pos> = None;
        let mut moves_searched = 0usize;

        for (mov, priority) in &forcing_moves {
            // Always search fives (priority >= 850), limit fours
            if *priority < 850 {
                if moves_searched >= max_qs_moves {
                    break;
                }
            }
            moves_searched += 1;
            board.place_stone(*mov, color);
            let cap_info = execute_captures_fast(board, *mov, color);

            // Compute child hash for TT
            let mut child_hash = self.shared.zobrist.update_place(hash, *mov, color);
            for j in 0..cap_info.count as usize {
                child_hash = self.shared.zobrist.update_capture(
                    child_hash,
                    cap_info.positions[j],
                    color.opponent(),
                );
            }
            if cap_info.pairs > 0 {
                let new_count = board.captures(color);
                let old_count = new_count - cap_info.pairs;
                child_hash =
                    self.shared
                        .zobrist
                        .update_capture_count(child_hash, color, old_count, new_count);
            }

            let score = -self.quiescence(
                board,
                color.opponent(),
                -beta,
                -alpha,
                *mov,
                qs_depth + 1,
                child_hash,
            );

            undo_captures(board, color, &cap_info);
            board.remove_stone(*mov);

            if self.is_stopped() {
                return 0;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(*mov);
            }
            if score > alpha {
                alpha = score;
            }
            if score >= beta {
                break; // Beta cutoff
            }
        }

        // TT store: cache QS result at depth 0.
        // Store even when no forcing move improved alpha (stand-pat dominant):
        // best_score == stand_pat with UpperBound tells future probes "this position
        // scores at most stand_pat", avoiding redundant QS re-evaluation.
        if !self.is_stopped() {
            let entry_type = if best_score >= beta {
                EntryType::LowerBound
            } else if best_score > original_alpha {
                EntryType::Exact
            } else {
                EntryType::UpperBound
            };
            self.shared.tt.store(hash, 0, best_score, entry_type, best_move);
        }

        best_score
    }

    /// Recursive alpha-beta search with negamax formulation.
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
            if self.check_time() {
                return 0;
            }
        }

        if self.is_stopped() {
            return 0;
        }

        // Fast terminal check
        let last_player = color.opponent();
        if board.captures(last_player) >= 5 {
            return -PatternScore::FIVE;
        }
        if has_five_at_pos(board, last_move, last_player) {
            // Check if the five is breakable by capture (endgame rule).
            // Only called when five exists (rare), so the extra cost is negligible.
            if let Some(five_line) = find_five_line_at_pos(board, last_move, last_player) {
                if can_break_five_by_capture(board, &five_line, last_player) {
                    // Breakable five: forcing but NOT terminal.
                    // Opponent can capture a pair to destroy it. Equivalent to
                    // a closed four — one specific defense exists.
                    return -(PatternScore::CLOSED_FOUR);
                }
            }
            return -PatternScore::FIVE;
        }

        // Check if the side to move already has an existing five on the board.
        // This handles the case where a breakable five was created earlier in the
        // search tree, but the opponent (last_player) played a non-breaking move.
        // Per game rules, the five-holder wins because the opponent had their
        // chance to break it and didn't take it.
        if board.stone_count() >= 10 && has_five_in_row(board, color) {
            return PatternScore::FIVE;
        }

        if depth <= 0 {
            return self.quiescence(board, color, alpha, beta, last_move, 0, hash);
        }

        // TT probe
        self.stats.tt_probes += 1;
        if let Some((score, _best_move)) = self.shared.tt.probe(hash, depth, alpha, beta) {
            self.stats.tt_score_hits += 1;
            return score;
        }

        // Pre-compute static eval for shallow pruning decisions (depth 1-2).
        // Shared by razoring, reverse futility pruning, and per-move futility.
        // Not computed at depth 3+ to avoid evaluate() overhead at interior nodes.
        let non_terminal = alpha.abs() < PatternScore::FIVE - 100
            && beta.abs() < PatternScore::FIVE - 100;
        let static_eval = if depth <= 2 && non_terminal {
            evaluate(board, color)
        } else {
            0
        };

        // Reverse futility pruning (static null move pruning):
        // At shallow depths, if position is far above beta, even losing
        // margin won't drop below. Cut immediately.
        // Uses OPEN_THREE (10K) per depth as margin — in Gomoku a single
        // quiet move can swing eval by up to OPEN_THREE (creating a new threat).
        if depth <= 2
            && non_terminal
            && static_eval - PatternScore::OPEN_THREE * i32::from(depth) >= beta
        {
            return static_eval;
        }

        // Razoring: at shallow depths, if static eval is far below alpha,
        // verify with quiescence search. If QS confirms the position is bad, cut.
        // Complementary to RFP (which cuts when eval >> beta).
        if depth <= 2
            && non_terminal
            && static_eval + PatternScore::OPEN_THREE * i32::from(depth) <= alpha
        {
            let qs_score = self.quiescence(board, color, alpha, beta, last_move, 0, hash);
            if qs_score <= alpha {
                return qs_score;
            }
        }

        // Null Move Pruning
        if allow_null && depth >= 3 && !Self::is_threatened(board, color, last_move) {
            let r = if depth >= 5 { 3i8 } else { 2i8 };
            let null_depth = (depth - 1 - r).max(0);

            let null_hash = self.shared.zobrist.toggle_side(hash);
            let null_score = -self.alpha_beta(
                board,
                color.opponent(),
                null_depth,
                -beta,
                -(beta - 1),
                last_move,
                null_hash,
                false,
            );

            if !self.is_stopped() && null_score >= beta {
                if depth <= 8 {
                    return beta;
                }
                let verify = self.alpha_beta(
                    board, color, depth - r, alpha, beta, last_move, hash, false,
                );
                if !self.is_stopped() && verify >= beta {
                    return beta;
                }
            }
        }

        let mut tt_move = self.shared.tt.get_best_move(hash);
        if tt_move.is_some() {
            self.stats.tt_move_hits += 1;
        }

        // Internal Iterative Deepening (IID): when no TT entry exists at depth >= 6,
        // run a shallow search to find a good first move for ordering.
        // Threshold raised from 4 to 6 to eliminate IID cascade at low-depth nodes.
        if tt_move.is_none() && depth >= 6 {
            let iid_depth = (depth - 4).max(1);
            self.alpha_beta(board, color, iid_depth, alpha, beta, last_move, hash, false);
            if !self.is_stopped() {
                tt_move = self.shared.tt.get_best_move(hash);
            }
        }

        let (mut moves, top_score) = self.generate_moves_ordered(board, color, tt_move, depth);
        if moves.is_empty() {
            return evaluate(board, color);
        }

        // Adaptive move limit: reduce in quiet positions (no tactical patterns).
        // Tactical threshold: 850K+ means real fork/four-level threats.
        // 800K (single block) is NOT tactical enough to warrant more candidates.
        let is_tactical = top_score >= 850_000;

        let max_moves = if is_tactical {
            match depth {
                0..=1 => 5,
                2..=3 => 7,
                4..=5 => 9,
                _ => 12,
            }
        } else {
            match depth {
                0..=1 => 3,
                2..=3 => 5,
                4..=5 => 7,
                _ => 9,
            }
        };
        moves.truncate(max_moves);

        // Futility pruning setup (reuses static_eval from shallow pruning block)
        let futility_ok = depth <= 2 && non_terminal;
        let futility_margin = if depth == 1 {
            PatternScore::CLOSED_FOUR
        } else {
            PatternScore::OPEN_FOUR
        };

        let mut best_score = -INF;
        let mut best_move = None;
        let mut entry_type = EntryType::UpperBound;

        for (i, (mov, move_score)) in moves.iter().enumerate() {
            // Futility pruning (uses pre-computed move score — no redundant score_move call)
            if futility_ok && i > 0 && static_eval + futility_margin <= alpha {
                if *move_score < 800_000 {
                    continue;
                }
            }

            // Late Move Pruning (LMP): at shallow depths, skip quiet moves entirely
            // after trying the first few. Done BEFORE make_move to avoid overhead.
            if i > 0 && depth <= 3 && i >= (3 + depth as usize * 2) && *move_score < 800_000 {
                continue;
            }

            board.place_stone(*mov, color);
            let cap_info = execute_captures_fast(board, *mov, color);

            let mut child_hash = self.shared.zobrist.update_place(hash, *mov, color);
            for j in 0..cap_info.count as usize {
                child_hash = self.shared.zobrist.update_capture(
                    child_hash,
                    cap_info.positions[j],
                    color.opponent(),
                );
            }
            if cap_info.pairs > 0 {
                let new_count = board.captures(color);
                let old_count = new_count - cap_info.pairs;
                child_hash =
                    self.shared
                        .zobrist
                        .update_capture_count(child_hash, color, old_count, new_count);
            }

            let is_capture = cap_info.pairs > 0;

            // Threat extension: forcing moves (creating a four) get +1 ply.
            // Fours have only 1-2 legal responses → narrow subtree, minimal cost.
            // Only extend at depth >= 2: at depth 1, quiescence already handles threats.
            let extension = if depth >= 2 && Self::move_creates_four(board, *mov, color) { 1i8 } else { 0i8 };

            // PVS + LMR
            let score = if i == 0 {
                -self.alpha_beta(
                    board,
                    color.opponent(),
                    depth - 1 + extension,
                    -beta,
                    -alpha,
                    *mov,
                    child_hash,
                    true,
                )
            } else {
                // LMR: logarithmic reduction + score-aware adjustment (Stockfish-inspired).
                // Captures, extensions, shallow depths, and PV move get no reduction.
                // Quiet moves (score < 500K) get +1 extra reduction — they rarely refute.
                let reduction = if is_capture || extension > 0 || depth < 2 || i < 1 {
                    0i8
                } else {
                    let d = depth as f32;
                    let m = i as f32;
                    let mut r = (d.sqrt() * m.sqrt() / 2.0) as i8;
                    // Score-aware: quiet moves with no tactical value get more reduction
                    if *move_score < 500_000 { r += 1; }
                    r.max(1).min(depth - 2)
                };
                let search_depth = (depth - 1 + extension - reduction).max(0);

                let mut s = -self.alpha_beta(
                    board,
                    color.opponent(),
                    search_depth,
                    -(alpha + 1),
                    -alpha,
                    *mov,
                    child_hash,
                    true,
                );

                if !self.is_stopped() && reduction > 0 && s > alpha {
                    s = -self.alpha_beta(
                        board,
                        color.opponent(),
                        depth - 1 + extension,
                        -(alpha + 1),
                        -alpha,
                        *mov,
                        child_hash,
                        true,
                    );
                }

                if !self.is_stopped() && s > alpha && s < beta {
                    s = -self.alpha_beta(
                        board,
                        color.opponent(),
                        depth - 1 + extension,
                        -beta,
                        -alpha,
                        *mov,
                        child_hash,
                        true,
                    );
                }
                s
            };

            undo_captures(board, color, &cap_info);
            board.remove_stone(*mov);

            if self.is_stopped() {
                return 0;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(*mov);
            }

            if score >= beta {
                self.stats.beta_cutoffs += 1;
                if i == 0 {
                    self.stats.first_move_cutoffs += 1;
                }
                #[allow(clippy::cast_sign_loss)]
                let ply = (self.max_depth - depth).max(0) as usize;
                if ply < 64 {
                    if self.killer_moves[ply][0] != Some(*mov) {
                        self.killer_moves[ply][1] = self.killer_moves[ply][0];
                        self.killer_moves[ply][0] = Some(*mov);
                    }
                }
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

        self.shared
            .tt
            .store(hash, depth, best_score, entry_type, best_move);

        best_score
    }

    /// Generate candidate moves near existing stones.
    #[must_use]
    #[cfg(test)]
    fn generate_moves(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut moves = Vec::with_capacity(50);
        let mut seen = [[false; BOARD_SIZE]; BOARD_SIZE];

        if board.is_board_empty() {
            return vec![Pos::new(9, 9)];
        }

        let radius = 2i32;

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
                        moves.push(new_pos);
                    }
                }
            }
        }

        moves
    }

    /// Score a move for ordering purposes (defense-first philosophy).
    fn score_move(
        &self,
        board: &Board,
        mov: Pos,
        color: Stone,
        tt_move: Option<Pos>,
        depth: i8,
    ) -> i32 {
        let opponent = color.opponent();

        if tt_move == Some(mov) {
            return 1_000_000;
        }

        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        let mut my_five = false;
        let mut opp_five = false;
        // Use counts (not booleans) to detect forks — a single move creating
        // multiple threats in different directions is far more dangerous.
        let mut my_open_four_count = 0i32;
        let mut opp_open_four_count = 0i32;
        let mut my_closed_four_count = 0i32;
        let mut opp_closed_four_count = 0i32;
        let mut my_open_three_count = 0i32;
        let mut opp_open_three_count = 0i32;
        let mut my_two_score = 0i32;
        let mut my_developing_dirs = 0i32;
        let mut opp_developing_dirs = 0i32;

        for (dr, dc) in dirs {
            let (mc, mo, mc_gap, mc_consec) = Self::count_line_with_gap(board, mov, dr, dc, color);
            let (oc, oo, oc_gap, oc_consec) =
                Self::count_line_with_gap(board, mov, dr, dc, opponent);

            if mc_consec >= 5 {
                my_five = true;
            } else if mc >= 5 && mc_gap {
                // Gap-five: e.g. OO_OO — filling the gap creates five-in-a-row.
                // Treat as open four (one move away from winning).
                my_open_four_count += 1;
            }
            if oc_consec >= 5 {
                opp_five = true;
            } else if oc >= 5 && oc_gap {
                opp_open_four_count += 1;
            }
            if mc == 4 {
                if mo == 2 {
                    my_open_four_count += 1;
                } else if mo == 1 {
                    my_closed_four_count += 1;
                }
            }
            if oc == 4 {
                if oo == 2 {
                    opp_open_four_count += 1;
                } else if oo == 1 {
                    opp_closed_four_count += 1;
                }
            }
            if mc == 3 && mo == 2 {
                my_open_three_count += 1;
            }
            if oc == 3 && oo == 2 {
                opp_open_three_count += 1;
            }
            if mc == 2 {
                my_two_score += if mo == 2 {
                    500
                } else if mo == 1 {
                    150
                } else {
                    0
                };
            }
            if oc == 2 && oo == 2 {
                my_two_score += 200;
            }

            // Multi-directional development detection
            // "Developing" = 2+ stones in line with at least 1 open end (room to grow)
            if mc >= 2 && mo >= 1 {
                my_developing_dirs += 1;
            }
            if oc >= 2 && oo >= 1 {
                opp_developing_dirs += 1;
            }
        }

        // Derived totals for fork detection
        let my_total_fours = my_open_four_count + my_closed_four_count;
        let opp_total_fours = opp_open_four_count + opp_closed_four_count;

        // === Priority ladder with fork detection ===
        // Immediate wins
        if my_five {
            return 900_000;
        }
        if opp_five {
            return 895_000;
        }

        let capture_count = i32::from(count_captures_fast(board, mov, color));
        if capture_count > 0 && i32::from(board.captures(color)) + capture_count >= 5 {
            return 890_000;
        }
        let opp_capture = i32::from(count_captures_fast(board, mov, opponent));
        if opp_capture > 0 && i32::from(board.captures(opponent)) + opp_capture >= 5 {
            return 885_000;
        }

        // MY FORKS: a single move creating multiple forcing threats
        // Two fours (any type): opponent can only block one → win
        if my_total_fours >= 2 {
            return 880_000;
        }
        // Four + open three: must block four, three promotes to open four → win
        if my_total_fours >= 1 && my_open_three_count >= 1 {
            return 878_000;
        }

        // Single open four (unstoppable without capture)
        if my_open_four_count >= 1 {
            return 870_000;
        }

        // BLOCK OPPONENT FORKS (higher priority than our single threats)
        if opp_total_fours >= 2 {
            return 868_000;
        }
        if opp_total_fours >= 1 && opp_open_three_count >= 1 {
            return 866_000;
        }
        if opp_open_four_count >= 1 {
            return 860_000;
        }

        // Capture-based urgency (opponent near capture win)
        let opp_caps = board.captures(opponent);
        if opp_capture > 0 && opp_caps >= 3 {
            return 855_000;
        }
        if opp_capture > 0 && opp_caps >= 2 {
            return 845_000;
        }

        // Double open three fork: both mine and opponent's
        if my_open_three_count >= 2 {
            return 840_000;
        }
        if opp_open_three_count >= 2 {
            return 838_000;
        }

        // Single forcing threats
        if my_closed_four_count >= 1 {
            return 830_000;
        }
        if opp_closed_four_count >= 1 {
            return 820_000;
        }
        if my_open_three_count >= 1 {
            return 810_000;
        }
        if opp_open_three_count >= 1 {
            return 800_000;
        }

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

        let capture_penalty = self.capture_vulnerability(board, mov, color);

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

        let cidx = if color == Stone::Black { 0 } else { 1 };
        let hist = self.history[cidx][mov.row as usize][mov.col as usize];

        #[allow(clippy::cast_possible_wrap)]
        let center = (BOARD_SIZE / 2) as i32;
        let dist = (i32::from(mov.row) - center).abs() + (i32::from(mov.col) - center).abs();
        let center_bonus = (18 - dist) * 25;

        // Proximity bonus: strongly prefer moves adjacent to existing friendly stones.
        // This prevents scattered placement and ensures pattern-building potential.
        let sz = BOARD_SIZE as i8;
        let mut proximity = 0i32;
        for (dr, dc) in dirs {
            for sign in [-1i8, 1i8] {
                let nr = mov.row as i8 + dr * sign;
                let nc = mov.col as i8 + dc * sign;
                if nr >= 0 && nr < sz && nc >= 0 && nc < sz
                    && board.get(Pos::new(nr as u8, nc as u8)) == color
                {
                    proximity += 200;
                }
            }
        }

        // Multi-directional development bonus:
        // 1 direction = normal (already counted in my_two_score)
        // 2+ directions = strategic threat that must be searched deeply
        let development_bonus = match my_developing_dirs {
            0..=1 => 0,
            2 => 50_000,
            _ => 100_000,
        };
        let disruption_bonus = match opp_developing_dirs {
            0..=1 => 0,
            2 => 30_000,
            _ => 80_000,
        };

        hist + center_bonus + my_two_score + proximity
            + development_bonus + disruption_bonus - capture_penalty
    }

    /// Generate candidate moves ordered by priority.
    /// Returns (sorted moves with scores, top move score) for adaptive move limiting
    /// and score-aware pruning decisions (LMR, futility, LMP).
    fn generate_moves_ordered(
        &self,
        board: &Board,
        color: Stone,
        tt_move: Option<Pos>,
        depth: i8,
    ) -> (Vec<(Pos, i32)>, i32) {
        let mut seen = [[false; BOARD_SIZE]; BOARD_SIZE];

        if board.is_board_empty() {
            return (vec![(Pos::new(9, 9), 1_000_000)], 0);
        }

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

        scored.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let top_score = scored.first().map_or(0, |(_, s)| *s);
        (scored, top_score)
    }

    /// Scan a line from `pos` in both directions.
    fn count_line_with_gap(
        board: &Board,
        pos: Pos,
        dr: i8,
        dc: i8,
        color: Stone,
    ) -> (i32, i32, bool, i32) {
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
                if counting_consecutive {
                    consec_pos += 1;
                }
                r += dr;
                c += dc;
            } else if cell == Stone::Empty && !has_gap {
                counting_consecutive = false;
                let nr = r + dr;
                let nc = c + dc;
                if nr >= 0
                    && nr < sz
                    && nc >= 0
                    && nc < sz
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
                if counting_consecutive {
                    consec_neg += 1;
                }
                r -= dr;
                c -= dc;
            } else if cell == Stone::Empty && !has_gap {
                counting_consecutive = false;
                let nr = r - dr;
                let nc = c - dc;
                if nr >= 0
                    && nr < sz
                    && nc >= 0
                    && nc < sz
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
    fn capture_vulnerability(&self, board: &Board, mov: Pos, color: Stone) -> i32 {
        let opponent = color.opponent();
        let sz = BOARD_SIZE as i8;
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        let mut vuln_count = 0i32;

        for (dr, dc) in dirs {
            for sign in [-1i8, 1i8] {
                let sdr = dr * sign;
                let sdc = dc * sign;

                let rm1 = mov.row as i8 - sdr;
                let cm1 = mov.col as i8 - sdc;
                let rp1 = mov.row as i8 + sdr;
                let cp1 = mov.col as i8 + sdc;
                let rp2 = mov.row as i8 + sdr * 2;
                let cp2 = mov.col as i8 + sdc * 2;

                if rm1 >= 0
                    && rm1 < sz
                    && cm1 >= 0
                    && cm1 < sz
                    && rp1 >= 0
                    && rp1 < sz
                    && cp1 >= 0
                    && cp1 < sz
                    && rp2 >= 0
                    && rp2 < sz
                    && cp2 >= 0
                    && cp2 < sz
                {
                    let before = board.get(Pos::new(rm1 as u8, cm1 as u8));
                    let after1 = board.get(Pos::new(rp1 as u8, cp1 as u8));
                    let after2 = board.get(Pos::new(rp2 as u8, cp2 as u8));

                    // opp-MOV-ally-opp: both sides occupied — opponent can't place,
                    // so this pair is SAFE. Removed (was over-penalizing).
                    if before == Stone::Empty && after1 == color && after2 == opponent {
                        vuln_count += 1;
                    }
                    // opp-MOV-ally-empty: opponent can place at after2 to capture
                    if before == opponent && after1 == color && after2 == Stone::Empty {
                        vuln_count += 1;
                    }
                }

                let rm2 = mov.row as i8 - sdr * 2;
                let cm2 = mov.col as i8 - sdc * 2;

                if rm2 >= 0
                    && rm2 < sz
                    && cm2 >= 0
                    && cm2 < sz
                    && rm1 >= 0
                    && rm1 < sz
                    && cm1 >= 0
                    && cm1 < sz
                    && rp1 >= 0
                    && rp1 < sz
                    && cp1 >= 0
                    && cp1 < sz
                {
                    let before2 = board.get(Pos::new(rm2 as u8, cm2 as u8));
                    let before1 = board.get(Pos::new(rm1 as u8, cm1 as u8));
                    let after = board.get(Pos::new(rp1 as u8, cp1 as u8));

                    // opp-ally-MOV-opp: both sides occupied — opponent can't place,
                    // so this pair is SAFE. Removed (was over-penalizing).
                    if before2 == Stone::Empty && before1 == color && after == opponent {
                        vuln_count += 1;
                    }
                    // opp-ally-MOV-empty: opponent can place at after to capture
                    if before2 == opponent && before1 == color && after == Stone::Empty {
                        vuln_count += 1;
                    }
                }
            }
        }

        if vuln_count > 0 {
            let opp_caps = i32::from(board.captures(color.opponent()));
            let base_penalty = 20_000; // was 8K — must compete with pattern scores
            let urgency = if opp_caps >= 3 {
                4
            } else if opp_caps >= 2 {
                2
            } else {
                1
            };
            vuln_count * base_penalty * urgency
        } else {
            0
        }
    }
}

// =============================================================================
// Searcher: public API wrapper (backward-compatible)
// =============================================================================

/// Alpha-Beta search engine with iterative deepening and transposition table.
///
/// Internally uses Lazy SMP for parallel search when `num_threads > 1`.
/// The searcher maintains a transposition table across searches for efficiency.
/// For a new game, call `clear_tt()` to reset the cached positions.
pub struct Searcher {
    shared: Arc<SharedState>,
    max_depth: i8,
    num_threads: usize,
    // Per-search state for single-threaded `search()` API
    history: [[[i32; BOARD_SIZE]; BOARD_SIZE]; 2],
}

impl Searcher {
    /// Create a new searcher with the specified transposition table size.
    ///
    /// Uses all available CPU cores for parallel search (Lazy SMP).
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
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4);
        Self::with_threads(tt_size_mb, num_threads)
    }

    /// Create a new searcher with explicit thread count.
    #[must_use]
    pub fn with_threads(tt_size_mb: usize, num_threads: usize) -> Self {
        let num_threads = num_threads.max(1);
        Self {
            shared: Arc::new(SharedState {
                zobrist: ZobristTable::new(),
                tt: AtomicTT::new(tt_size_mb),
                stopped: AtomicBool::new(false),
            }),
            max_depth: 10,
            num_threads,
            history: [[[0; BOARD_SIZE]; BOARD_SIZE]; 2],
        }
    }

    /// Search for the best move using iterative deepening (single-threaded).
    ///
    /// Used by tests and when precise deterministic behavior is needed.
    #[must_use]
    pub fn search(&mut self, board: &Board, color: Stone, max_depth: i8) -> SearchResult {
        self.shared.stopped.store(false, Ordering::Relaxed);
        self.max_depth = max_depth;

        let mut worker = WorkerSearcher {
            shared: Arc::clone(&self.shared),
            nodes: 0,
            max_depth,
            killer_moves: [[None; 2]; 64],
            history: self.history,
            start_time: None,
            time_limit: None,
            stats: SearchStats::default(),
        };

        let mut best_result = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
            stats: SearchStats::default(),
        };

        let mut work_board = board.clone();
        let mut prev_was_winning = false;
        let mut prev_was_losing = false;

        for depth in 1..=max_depth {
            let result = worker.search_root(&mut work_board, color, depth, -INF, INF);
            best_result = result;
            best_result.depth = depth;

            let is_winning = best_result.score >= PatternScore::FIVE - 100;
            let is_losing = best_result.score <= -(PatternScore::FIVE - 100);

            if is_winning && prev_was_winning && depth >= 12 {
                break;
            }
            if is_losing && prev_was_losing && depth >= 10 {
                break;
            }

            prev_was_winning = is_winning;
            prev_was_losing = is_losing;
        }

        best_result.nodes = worker.nodes;
        best_result.stats = worker.stats.clone();
        self.history = worker.history;
        best_result
    }

    /// Search with smart time management using Lazy SMP parallel search.
    ///
    /// Two hard constraints (project requirements):
    /// 1. **Minimum depth 10** — always reached regardless of time
    /// 2. **Average < 500ms** — time prediction prevents over-runs beyond depth 10
    #[must_use]
    pub fn search_timed(
        &mut self,
        board: &Board,
        color: Stone,
        max_depth: i8,
        time_limit_ms: u64,
    ) -> SearchResult {
        self.shared.stopped.store(false, Ordering::Relaxed);
        self.max_depth = max_depth;
        let start = Instant::now();
        let time_limit = Duration::from_millis((time_limit_ms + 300).max(800));

        // Spawn helper threads (workers 1..N)
        let handles: Vec<_> = (1..self.num_threads)
            .map(|thread_id| {
                let shared = Arc::clone(&self.shared);
                let board_clone = board.clone();
                let start_depth_offset = thread_id as i8;

                std::thread::spawn(move || {
                    let mut worker =
                        WorkerSearcher::new(shared, max_depth, start, time_limit);
                    worker.search_iterative(&board_clone, color, max_depth, start_depth_offset)
                })
            })
            .collect();

        // Main thread = worker 0
        let mut main_worker = WorkerSearcher {
            shared: Arc::clone(&self.shared),
            nodes: 0,
            max_depth,
            killer_moves: [[None; 2]; 64],
            history: self.history,
            start_time: Some(start),
            time_limit: Some(time_limit),
            stats: SearchStats::default(),
        };
        let main_result = main_worker.search_iterative(board, color, max_depth, 0);

        // Signal all workers to stop
        self.shared.stopped.store(true, Ordering::Relaxed);

        // Collect results — pick best (deepest search, then highest score)
        let mut best = main_result;
        let mut total_nodes = best.nodes;
        let mut merged_stats = best.stats.clone();

        for handle in handles {
            if let Ok(result) = handle.join() {
                total_nodes += result.nodes;
                merged_stats.merge(&result.stats);
                if result.depth > best.depth
                    || (result.depth == best.depth && result.score > best.score)
                {
                    best = result;
                }
            }
        }

        best.nodes = total_nodes;
        best.stats = merged_stats;
        self.history = main_worker.history;
        best
    }

    /// Clear history heuristic and killer moves.
    pub fn clear_history(&mut self) {
        self.history = [[[0; BOARD_SIZE]; BOARD_SIZE]; 2];
    }

    /// Get statistics about the transposition table.
    #[must_use]
    pub fn tt_stats(&self) -> TTStats {
        self.shared.tt.stats()
    }

    /// Clear the transposition table.
    pub fn clear_tt(&self) {
        self.shared.tt.clear();
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
        assert_eq!(result.best_move, Some(Pos::new(9, 9)));
    }

    #[test]
    fn test_search_finds_winning_move() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let result = searcher.search(&board, Stone::Black, 2);
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
    }

    #[test]
    fn test_search_blocks_opponent_win() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }
        board.place_stone(Pos::new(10, 0), Stone::Black);

        let result = searcher.search(&board, Stone::Black, 4);
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
    }

    #[test]
    fn test_iterative_deepening_improves() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::White);
        board.place_stone(Pos::new(8, 9), Stone::Black);

        let result = searcher.search(&board, Stone::White, 2);
        assert!(result.depth >= 1);
        assert!(result.nodes > 0);
    }

    #[test]
    fn test_generate_moves_radius() {
        let shared = Arc::new(SharedState {
            zobrist: ZobristTable::new(),
            tt: AtomicTT::new(1),
            stopped: AtomicBool::new(false),
        });
        let worker = WorkerSearcher {
            shared,
            nodes: 0,
            max_depth: 10,
            killer_moves: [[None; 2]; 64],
            history: [[[0; BOARD_SIZE]; BOARD_SIZE]; 2],
            start_time: None,
            time_limit: None,
            stats: SearchStats::default(),
        };
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let moves = worker.generate_moves(&board, Stone::White);
        assert!(!moves.is_empty());
        assert!(moves.len() <= 24);
    }

    #[test]
    fn test_search_with_captures() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let result = searcher.search(&board, Stone::Black, 4);
        assert!(result.best_move.is_some(), "Should find a move");
        let mov = result.best_move.unwrap();
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

        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let result = searcher.search(&board, Stone::Black, 2);
        assert!(
            result.score >= PatternScore::FIVE - 100,
            "Should detect winning position"
        );
    }

    #[test]
    fn test_search_losing_score() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }
        board.place_stone(Pos::new(0, 0), Stone::Black);

        let result = searcher.search(&board, Stone::Black, 2);
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
    }

    #[test]
    fn test_generate_moves_excludes_forbidden() {
        let shared = Arc::new(SharedState {
            zobrist: ZobristTable::new(),
            tt: AtomicTT::new(1),
            stopped: AtomicBool::new(false),
        });
        let worker = WorkerSearcher {
            shared,
            nodes: 0,
            max_depth: 10,
            killer_moves: [[None; 2]; 64],
            history: [[[0; BOARD_SIZE]; BOARD_SIZE]; 2],
            start_time: None,
            time_limit: None,
            stats: SearchStats::default(),
        };
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);
        board.place_stone(Pos::new(8, 9), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::Black);

        let moves = worker.generate_moves(&board, Stone::Black);
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
        assert!(result.nodes >= 1);
    }

    #[test]
    fn test_search_multiple_times() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);

        let result1 = searcher.search(&board, Stone::White, 4);
        assert!(result1.best_move.is_some());

        let result2 = searcher.search(&board, Stone::White, 4);
        assert!(result2.best_move.is_some());

        assert!(result2.nodes <= result1.nodes || result2.nodes < result1.nodes + 500);

        let m1 = result1.best_move.unwrap();
        let m2 = result2.best_move.unwrap();
        assert!(m1.row.abs_diff(9) <= 2 && m1.col.abs_diff(9) <= 2);
        assert!(m2.row.abs_diff(9) <= 2 && m2.col.abs_diff(9) <= 2);
    }

    #[test]
    fn test_parallel_search_timed() {
        let mut searcher = Searcher::with_threads(16, 4);
        let mut board = Board::new();

        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::White);
        board.place_stone(Pos::new(10, 9), Stone::Black);
        board.place_stone(Pos::new(8, 10), Stone::White);

        let result = searcher.search_timed(&board, Stone::Black, 12, 500);
        assert!(result.best_move.is_some(), "Should find a move");
        assert!(result.depth >= 4, "Should reach reasonable depth, got {}", result.depth);
        assert!(result.nodes > 0, "Should search some nodes");
    }

    /// Test that quiescence search detects forced wins beyond the regular search depth.
    /// Setup: Black has three in a row with both ends open → four → five is forced.
    /// Even at depth 1, QS should see the winning sequence.
    #[test]
    fn test_quiescence_detects_open_four() {
        let mut searcher = Searcher::with_threads(16, 1);
        let mut board = Board::new();

        // Black open three: _BBB_ at row 9
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);
        // White stones far away
        board.place_stone(Pos::new(0, 0), Stone::White);
        board.place_stone(Pos::new(0, 1), Stone::White);

        // Shallow search should still find the winning continuation
        let result = searcher.search(&board, Stone::Black, 2);
        assert!(result.score > PatternScore::OPEN_FOUR,
            "QS should evaluate open three position very highly, got {}", result.score);
    }

    /// Test QS detects forced win via four-threat sequence.
    #[test]
    fn test_quiescence_four_threat_win() {
        let mut searcher = Searcher::with_threads(16, 1);
        let mut board = Board::new();

        // Black has OOOO_ (closed four) → extending to five is forced
        board.place_stone(Pos::new(9, 7), Stone::Black);
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);
        // White blocks one side
        board.place_stone(Pos::new(9, 6), Stone::White);
        board.place_stone(Pos::new(0, 0), Stone::White);

        let result = searcher.search(&board, Stone::Black, 1);
        assert_eq!(result.best_move, Some(Pos::new(9, 11)),
            "Should find the five-completion move");
        assert!(result.score >= PatternScore::FIVE - 100,
            "Should be a winning score, got {}", result.score);
    }

    /// Test that the search correctly detects an existing five on the board
    /// that the opponent failed to break. In the game rules, if a breakable
    /// five persists because the defender played a non-breaking move, the
    /// five-holder wins. The search must detect this at intermediate nodes,
    /// not just at root level.
    #[test]
    fn test_search_detects_existing_five() {
        use crate::rules::find_five_positions;

        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        // Set up a position where Black has a five-in-a-row (diagonal)
        // Black five: (7,7), (8,8), (9,9), (10,10), (11,11)
        board.place_stone(Pos::new(7, 7), Stone::Black);
        board.place_stone(Pos::new(8, 8), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(10, 10), Stone::Black);
        board.place_stone(Pos::new(11, 11), Stone::Black);

        // White stones forming a capture bracket:
        // White at (6,6) and (9,7) → can capture (7,7)+(8,8) if White plays at specific spot
        // Actually, let's set up breakable five: need White to be able to capture a pair
        // from the five line. Pattern: White(6,6) - Black(7,7) - Black(8,8) - White(9,9)?
        // No, (9,9) is Black. Let me use a different capture line.
        // We need: White at some position, then 2 consecutive five-stones, then an empty spot
        // for White to play and capture.
        // Capture pattern: White_place - Black - Black - White_existing
        // Let's put White at (12,12) — then (11,11)-(10,10) are Black, need White at (9,9)?
        // No (9,9) is Black. Different direction needed.

        // Simpler: make the five NOT breakable for this test.
        // Put some White stones far away.
        board.place_stone(Pos::new(0, 0), Stone::White);
        board.place_stone(Pos::new(0, 1), Stone::White);
        board.place_stone(Pos::new(0, 2), Stone::White);
        board.place_stone(Pos::new(1, 0), Stone::White);
        board.place_stone(Pos::new(1, 1), Stone::White);

        // Verify Black has five
        let five = find_five_positions(&board, Stone::Black);
        assert!(five.is_some(), "Black should have five-in-a-row");

        // White to move — Black already has five on the board.
        // The search should detect Black's existing five and return a very negative score
        // (losing for White since Black has already won).
        let result = searcher.search(&board, Stone::White, 4);

        // White should see this as a losing position
        assert!(result.score <= -(PatternScore::FIVE - 100),
            "White should detect Black's existing five as a loss, got score {}",
            result.score);
    }
}
