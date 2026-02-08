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
    count_captures_fast, execute_captures_fast, has_five_at_pos, is_valid_move, undo_captures,
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
        };

        let mut work_board = board.clone();
        let search_start = self.start_time.unwrap_or_else(Instant::now);
        let soft_limit = self.time_limit.unwrap_or(Duration::from_millis(500));
        let mut prev_depth_time = Duration::ZERO;

        let min_depth: i8 = if board.stone_count() <= 4 { 8 } else { 10 };
        const ASP_WINDOW: i32 = 100;

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
                    asp_alpha = (result.score - ASP_WINDOW * 4).max(-INF);
                } else if result.score >= asp_beta {
                    asp_beta = (result.score + ASP_WINDOW * 4).min(INF);
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

            // Early exit: winning or confirmed loss
            if best_result.score >= PatternScore::FIVE - 100 && depth >= 6 {
                break;
            }
            if best_result.score <= -(PatternScore::FIVE - 100) && depth >= 6 {
                break;
            }

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
        let mut moves = self.generate_moves_ordered(board, color, tt_move, depth);
        moves.truncate(MAX_ROOT_MOVES);

        for (i, mov) in moves.iter().enumerate() {
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

            let score = if i == 0 {
                -self.alpha_beta(
                    board,
                    color.opponent(),
                    depth - 1,
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
                    depth - 1,
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
                        depth - 1,
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

            alpha = alpha.max(score);
        }

        SearchResult {
            best_move,
            score: best_score,
            depth,
            nodes: self.nodes,
        }
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
            return -PatternScore::FIVE;
        }

        if depth <= 0 {
            return evaluate(board, color);
        }

        // TT probe
        if let Some((score, _best_move)) = self.shared.tt.probe(hash, depth, alpha, beta) {
            if score != 0 {
                return score;
            }
        }

        // Null Move Pruning
        if allow_null && depth >= 3 && !Self::is_threatened(board, color, last_move) {
            let r = if depth >= 5 { 3i8 } else { 2i8 };
            let null_depth = (depth - 1 - r).max(0);

            let null_score = -self.alpha_beta(
                board,
                color.opponent(),
                null_depth,
                -beta,
                -(beta - 1),
                last_move,
                hash,
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

        let tt_move = self.shared.tt.get_best_move(hash);
        let mut moves = self.generate_moves_ordered(board, color, tt_move, depth);
        if moves.is_empty() {
            return evaluate(board, color);
        }

        let max_moves = match depth {
            0..=1 => 7,
            2..=3 => 9,
            4..=5 => 12,
            _ => 15,
        };
        moves.truncate(max_moves);

        // Futility pruning setup
        let futility_ok = depth <= 2 && alpha.abs() < PatternScore::FIVE - 100;
        let static_eval = if futility_ok { evaluate(board, color) } else { 0 };
        let futility_margin = if depth == 1 {
            PatternScore::CLOSED_FOUR
        } else {
            PatternScore::OPEN_FOUR
        };

        let mut best_score = -INF;
        let mut best_move = None;
        let mut entry_type = EntryType::UpperBound;

        for (i, mov) in moves.iter().enumerate() {
            // Futility pruning
            if futility_ok && i > 0 && static_eval + futility_margin <= alpha {
                let move_score = self.score_move(board, *mov, color, tt_move, depth);
                if move_score < 800_000 {
                    continue;
                }
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

            // PVS + LMR
            let score = if i == 0 {
                -self.alpha_beta(
                    board,
                    color.opponent(),
                    depth - 1,
                    -beta,
                    -alpha,
                    *mov,
                    child_hash,
                    true,
                )
            } else {
                let reduction = if is_capture || depth < 3 {
                    0i8
                } else if i >= 8 && depth >= 5 {
                    3i8
                } else if i >= 5 && depth >= 4 {
                    2i8
                } else if i >= 3 && depth >= 3 {
                    1i8
                } else {
                    0i8
                };
                let search_depth = (depth - 1 - reduction).max(0);

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
                        depth - 1,
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
                        depth - 1,
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
        let mut my_open_four = false;
        let mut opp_open_four = false;
        let mut my_four = false;
        let mut opp_four = false;
        let mut my_open_three = false;
        let mut opp_open_three = false;
        let mut my_two_score = 0i32;

        for (dr, dc) in dirs {
            let (mc, mo, _, mc_consec) = Self::count_line_with_gap(board, mov, dr, dc, color);
            let (oc, oo, _, oc_consec) =
                Self::count_line_with_gap(board, mov, dr, dc, opponent);

            if mc_consec >= 5 {
                my_five = true;
            }
            if oc_consec >= 5 {
                opp_five = true;
            }
            if mc == 4 {
                if mo == 2 {
                    my_open_four = true;
                }
                if mo >= 1 {
                    my_four = true;
                }
            }
            if oc == 4 {
                if oo == 2 {
                    opp_open_four = true;
                }
                if oo >= 1 {
                    opp_four = true;
                }
            }
            if mc == 3 && mo == 2 {
                my_open_three = true;
            }
            if oc == 3 && oo == 2 {
                opp_open_three = true;
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
        }

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

        if my_open_four {
            return 870_000;
        }
        if opp_open_four {
            return 860_000;
        }

        let opp_caps = board.captures(opponent);
        if opp_capture > 0 && opp_caps >= 3 {
            return 855_000;
        }
        if opp_capture > 0 && opp_caps >= 2 {
            return 845_000;
        }

        if my_four {
            return 830_000;
        }
        if opp_four {
            return 820_000;
        }
        if my_open_three {
            return 810_000;
        }
        if opp_open_three {
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
        let center_bonus = (18 - dist) * 10;

        hist + center_bonus + my_two_score - capture_penalty
    }

    /// Generate candidate moves ordered by priority.
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
        scored.into_iter().map(|(m, _)| m).collect()
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

                    if before == opponent && after1 == color && after2 == opponent {
                        vuln_count += 1;
                    }
                    if before == Stone::Empty && after1 == color && after2 == opponent {
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

                    if before2 == opponent && before1 == color && after == opponent {
                        vuln_count += 1;
                    }
                    if before2 == Stone::Empty && before1 == color && after == opponent {
                        vuln_count += 1;
                    }
                }
            }
        }

        if vuln_count > 0 {
            let opp_caps = i32::from(board.captures(color.opponent()));
            let base_penalty = 8_000;
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
        };

        let mut best_result = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
        };

        let mut work_board = board.clone();

        for depth in 1..=max_depth {
            let result = worker.search_root(&mut work_board, color, depth, -INF, INF);
            best_result = result;
            best_result.depth = depth;

            if best_result.score >= PatternScore::FIVE - 100 && depth >= 10 {
                break;
            }
            if best_result.score <= -(PatternScore::FIVE - 100) && depth >= 8 {
                break;
            }
        }

        best_result.nodes = worker.nodes;
        // Persist history for future searches
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
        };
        let main_result = main_worker.search_iterative(board, color, max_depth, 0);

        // Signal all workers to stop
        self.shared.stopped.store(true, Ordering::Relaxed);

        // Collect results — pick best (deepest search, then highest score)
        let mut best = main_result;
        let mut total_nodes = best.nodes;

        for handle in handles {
            if let Ok(result) = handle.join() {
                total_nodes += result.nodes;
                if result.depth > best.depth
                    || (result.depth == best.depth && result.score > best.score)
                {
                    best = result;
                }
            }
        }

        best.nodes = total_nodes;
        // Persist history from main worker
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
}
