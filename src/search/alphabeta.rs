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

use crate::board::{Bitboard, Board, Pos, Stone, BOARD_SIZE};
use crate::eval::{evaluate, PatternScore};
use crate::rules::{
    can_break_five_by_capture, count_captures_fast, execute_captures_fast,
    find_five_break_moves, find_five_line_at_pos, has_five_at_pos, has_five_in_row, is_valid_move,
    undo_captures,
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
    countermove: [[[Option<Pos>; BOARD_SIZE]; BOARD_SIZE]; 2],
    last_move_for_ordering: Option<Pos>,
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
            countermove: [[[None; BOARD_SIZE]; BOARD_SIZE]; 2],
            last_move_for_ordering: None,
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

        let min_depth: i8 = if board.stone_count() <= 4 { 8 } else { 10 };
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

            // History gravity: halve all history scores at each new depth.
            // Ensures recent search results outweigh stale move ordering data.
            if depth > first_depth {
                for color_hist in &mut self.history {
                    for row in color_hist.iter_mut() {
                        for val in row.iter_mut() {
                            *val >>= 1;
                        }
                    }
                }
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
        self.last_move_for_ordering = None;
        let (mut moves, _top_score) = self.generate_moves_ordered(board, color, tt_move, depth);
        // Lazy double-three: keep the first MAX_ROOT_MOVES valid moves.
        // Forbidden (double-three) moves may score high, so we can't truncate
        // first — that would displace valid defensive moves from the top-N.
        let mut valid_count = 0;
        moves.retain(|(mov, _)| {
            if valid_count >= MAX_ROOT_MOVES {
                return false;
            }
            if is_valid_move(board, *mov, color) {
                valid_count += 1;
                true
            } else {
                false
            }
        });

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
        // Gap pattern threat: opponent has 3+ stones with one gap in any direction
        // from last_move. E.g., O_OO or OO_O — filling gap creates an open four.
        // This catches threats missed by the consecutive scan above.
        for (dr, dc) in dirs {
            for sign in [-1i8, 1] {
                let sdr = dr * sign;
                let sdc = dc * sign;
                let mut gap_count = 1i32; // last_move stone
                let mut gap_used = false;
                let mut gap_open_ends = 0i32;
                // Scan positive direction (from last_move)
                for i in 1..=4i8 {
                    let gr = last_move.row as i8 + sdr * i;
                    let gc = last_move.col as i8 + sdc * i;
                    if gr < 0 || gr >= sz || gc < 0 || gc >= sz { break; }
                    let s = board.get(Pos::new(gr as u8, gc as u8));
                    if s == opp {
                        gap_count += 1;
                    } else if s == Stone::Empty && !gap_used {
                        // Check stone after gap
                        let nr = gr + sdr;
                        let nc = gc + sdc;
                        if nr >= 0 && nr < sz && nc >= 0 && nc < sz
                            && board.get(Pos::new(nr as u8, nc as u8)) == opp
                        {
                            gap_used = true;
                            continue; // skip gap, next iteration picks up the stone
                        }
                        gap_open_ends += 1;
                        break;
                    } else {
                        break;
                    }
                }
                // Scan negative direction
                for i in 1..=4i8 {
                    let gr = last_move.row as i8 - sdr * i;
                    let gc = last_move.col as i8 - sdc * i;
                    if gr < 0 || gr >= sz || gc < 0 || gc >= sz { break; }
                    let s = board.get(Pos::new(gr as u8, gc as u8));
                    if s == opp {
                        gap_count += 1;
                    } else if s == Stone::Empty {
                        gap_open_ends += 1;
                        break;
                    } else {
                        break;
                    }
                }
                // Gap pattern: 3+ stones with gap AND open ends → strong threat
                if gap_used && (gap_count >= 4 || (gap_count >= 3 && gap_open_ends >= 2)) {
                    return true;
                }
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
                    // Breakable five: search break moves even in quiescence.
                    // Uses depth=0 so the break-move search recurses into alpha_beta
                    // which enters quiescence for the post-break position.
                    return self.search_five_break(
                        board, color, 0, alpha, beta, &five_line, last_player, hash,
                    );
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

    /// Search only break moves when opponent has a breakable five.
    /// Called from both alpha_beta and quiescence when `can_break_five_by_capture` is true.
    /// The side to move MUST play a capture that removes a stone from the five,
    /// otherwise they lose (has_five_in_row at next ply returns +FIVE for the five-holder).
    fn search_five_break(
        &mut self,
        board: &mut Board,
        color: Stone,
        depth: i8,
        mut alpha: i32,
        beta: i32,
        five_positions: &[Pos],
        five_color: Stone,
        hash: u64,
    ) -> i32 {
        let break_moves = find_five_break_moves(board, five_positions, five_color);
        if break_moves.is_empty() {
            return -PatternScore::FIVE;
        }

        let mut best = -PatternScore::FIVE;
        for break_pos in &break_moves {
            let break_pos = *break_pos;
            if !board.is_empty(break_pos) {
                continue;
            }

            // Make move
            board.place_stone(break_pos, color);
            let cap_info = execute_captures_fast(board, break_pos, color);

            // Update Zobrist hash
            let mut child_hash = self.shared.zobrist.update_place(hash, break_pos, color);
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

            // Recurse: depth-1 into normal alpha-beta (handles depth<=0 → quiescence)
            let search_depth = (depth - 1).max(0);
            let score = -self.alpha_beta(
                board,
                color.opponent(),
                search_depth,
                -beta,
                -alpha,
                break_pos,
                child_hash,
                true,
            );

            // Unmake move
            undo_captures(board, color, &cap_info);
            board.remove_stone(break_pos);

            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    if score >= beta {
                        break;
                    }
                }
            }

            if self.is_stopped() {
                return best;
            }
        }
        best
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
                    // Breakable five: search only break moves (captures that destroy the five).
                    // The old fixed-score return (-CLOSED_FOUR) missed post-break threats,
                    // causing the AI to play self-destructive captures like K11 in Game 5.
                    return self.search_five_break(
                        board, color, depth, alpha, beta, &five_line, last_player, hash,
                    );
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

        // Pre-compute static eval for pruning decisions.
        // Used by NMP (all depths), RFP (depth 1-3), razoring (depth 1-3),
        // and per-move futility (depth 1-3). evaluate() is O(stones*4) ≈ 1-5μs.
        let non_terminal = alpha.abs() < PatternScore::FIVE - 100
            && beta.abs() < PatternScore::FIVE - 100;
        let static_eval = if non_terminal {
            evaluate(board, color)
        } else {
            0
        };

        // Reverse futility pruning (static null move pruning):
        // At shallow depths, if position is far above beta, even losing
        // margin won't drop below. Cut immediately.
        // Uses OPEN_THREE (10K) per depth as margin — in Gomoku a single
        // quiet move can swing eval by up to OPEN_THREE (creating a new threat).
        if depth <= 3
            && non_terminal
            && static_eval - PatternScore::OPEN_THREE * i32::from(depth) >= beta
        {
            return static_eval;
        }

        // Razoring: at shallow depths, if static eval is far below alpha,
        // verify with quiescence search. If QS confirms the position is bad, cut.
        // Complementary to RFP (which cuts when eval >> beta).
        if depth <= 3
            && non_terminal
            && static_eval + PatternScore::OPEN_THREE * i32::from(depth) <= alpha
        {
            let qs_score = self.quiescence(board, color, alpha, beta, last_move, 0, hash);
            if qs_score <= alpha {
                return qs_score;
            }
        }

        // Null Move Pruning
        // Gate: static_eval >= beta ensures we only try NMP when position is good.
        // This prevents NMP from pruning in positions where opponent has strong
        // patterns (captures removed our stones, opponent can rebuild threats).
        // R=2 fixed: R=3 was too aggressive, missing critical opponent responses
        // (e.g., opponent replaying captured position to create open four).
        if allow_null && depth >= 3
            && non_terminal
            && static_eval >= beta
            && !Self::is_threatened(board, color, last_move)
        {
            let r = 2i8;
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

        self.last_move_for_ordering = Some(last_move);
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
        // Lazy double-three: keep the first max_moves valid moves.
        // Scan sorted list and accept valid moves until we have enough.
        // This avoids truncate-then-retain which can displace defensive moves.
        {
            let mut valid_count = 0;
            moves.retain(|(mov, _)| {
                if valid_count >= max_moves {
                    return false;
                }
                if is_valid_move(board, *mov, color) {
                    valid_count += 1;
                    true
                } else {
                    false
                }
            });
        }

        // Futility pruning setup (reuses static_eval from shallow pruning block)
        let futility_ok = depth <= 3 && non_terminal;
        let futility_margin = match depth {
            1 => PatternScore::CLOSED_FOUR,
            2 => PatternScore::OPEN_FOUR,
            _ => PatternScore::OPEN_FOUR + PatternScore::OPEN_THREE, // depth 3: 110K
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
            // after trying the first few. Done BEFORE make_move for zero overhead.
            // Note: threshold intentionally exceeds move limits at these depths,
            // so this mainly serves as a safety net for positions with many candidates.
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

                // Countermove: record best response to opponent's last move
                let opp_idx = if color == Stone::Black { 1 } else { 0 };
                self.countermove[opp_idx][last_move.row as usize][last_move.col as usize] = Some(*mov);

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

        // Direct bitboard access: 1 lookup per check vs board.get()'s 2.
        let my_bb = board.stones(color).unwrap();
        let opp_bb = board.stones(opponent).unwrap();

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
            // Merged scan: single bidirectional pass produces both my and opp patterns.
            // Halves cell lookups vs two separate count_line_with_gap calls.
            let (mc, mo, mc_gap, mc_consec, oc, oo, oc_gap, oc_consec) =
                Self::count_line_both(my_bb, opp_bb, mov, dr, dc);

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

        // Immediate capture penalty: detect if placing here creates a pair
        // that opponent can capture next turn or set up in 2 moves.
        // Pattern 1: opp-ME-ally-empty → opponent plays at empty to capture (1-move, 150K)
        // Pattern 2: empty-ME-ally-empty → both flanks open, capturable in 2 moves (50K, scales w/ caps)
        let mut immediate_cap_penalty = 0i32;
        {
            let r = mov.row as i8;
            let c = mov.col as i8;
            let sz = BOARD_SIZE as i8;
            let opp_caps = i32::from(board.captures(opponent));
            let setup_weight = if opp_caps >= 3 { 100_000 } else if opp_caps >= 2 { 75_000 } else { 50_000 };
            for &(dr, dc) in &dirs {
                for sign in [-1i8, 1i8] {
                    // Cells relative to mov: -1*sign, 0(mov), +1*sign, +2*sign
                    let r1 = r - sign * dr;
                    let c1 = c - sign * dc;
                    let r2 = r + sign * dr;
                    let c2 = c + sign * dc;
                    let r3 = r + 2 * sign * dr;
                    let c3 = c + 2 * sign * dc;

                    if r1 < 0 || r1 >= sz || c1 < 0 || c1 >= sz { continue; }
                    if r2 < 0 || r2 >= sz || c2 < 0 || c2 >= sz { continue; }
                    if r3 < 0 || r3 >= sz || c3 < 0 || c3 >= sz { continue; }

                    let p1 = Pos::new(r1 as u8, c1 as u8);
                    let p2 = Pos::new(r2 as u8, c2 as u8);
                    let p3 = Pos::new(r3 as u8, c3 as u8);

                    let p1_empty = !my_bb.get(p1) && !opp_bb.get(p1);
                    let p3_empty = !my_bb.get(p3) && !opp_bb.get(p3);

                    // opp @ p1, ally @ p2, empty @ p3 → 1-move capture threat
                    if opp_bb.get(p1) && my_bb.get(p2) && p3_empty {
                        immediate_cap_penalty += 150_000;
                    }
                    // empty @ p1, ally @ p2, empty @ p3 → 2-move setup threat
                    if p1_empty && my_bb.get(p2) && p3_empty {
                        immediate_cap_penalty += setup_weight;
                    }
                }
            }
        }

        let capture_penalty =
            Self::capture_vulnerability(my_bb, opp_bb, mov, board.captures(opponent))
            + immediate_cap_penalty;

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

        // Countermove bonus: if this move is the best recorded response to opponent's last move
        if let Some(lm) = self.last_move_for_ordering {
            let opp_idx = if color == Stone::Black { 1 } else { 0 };
            if self.countermove[opp_idx][lm.row as usize][lm.col as usize] == Some(mov) {
                return 400_000 - capture_penalty;
            }
        }

        let cidx = if color == Stone::Black { 0 } else { 1 };
        let hist = self.history[cidx][mov.row as usize][mov.col as usize];

        #[allow(clippy::cast_possible_wrap)]
        let center = (BOARD_SIZE / 2) as i32;
        let dist = (i32::from(mov.row) - center).abs() + (i32::from(mov.col) - center).abs();
        let center_bonus = (18 - dist) * 25;

        // Proximity bonus: strongly prefer moves adjacent to existing friendly stones.
        // Direct bitboard: 1 lookup per neighbor vs board.get()'s 2.
        let sz = BOARD_SIZE as i8;
        let mut proximity = 0i32;
        for (dr, dc) in dirs {
            for sign in [-1i8, 1i8] {
                let nr = mov.row as i8 + dr * sign;
                let nc = mov.col as i8 + dc * sign;
                if nr >= 0 && nr < sz && nc >= 0 && nc < sz
                    && my_bb.get(Pos::new(nr as u8, nc as u8))
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

                    // Lazy double-three: only check is_empty here (2 bb ops).
                    // Full is_valid_move (80+ bb ops for double-three) deferred to
                    // the search loop where adaptive limits prune most candidates.
                    if board.is_empty(new_pos) {
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

    /// Scan a line from `pos` in both directions for both colors simultaneously.
    ///
    /// Merges two separate scans into one bidirectional pass, halving cell lookups.
    /// Uses direct bitboard access (1 op per check) instead of board.get() (2 ops).
    ///
    /// Returns (my_count, my_open, my_gap, my_consec, opp_count, opp_open, opp_gap, opp_consec).
    fn count_line_both(
        my_bb: &Bitboard,
        opp_bb: &Bitboard,
        pos: Pos,
        dr: i8,
        dc: i8,
    ) -> (i32, i32, bool, i32, i32, i32, bool, i32) {
        let sz = BOARD_SIZE as i8;

        // My color accumulators
        let mut mc = 1i32;
        let mut mo = 0i32;
        let mut m_gap = false;
        let mut mc_pos = 0i32;
        let mut mc_neg = 0i32;

        // Opponent color accumulators
        let mut oc = 1i32;
        let mut oo = 0i32;
        let mut o_gap = false;
        let mut oc_pos = 0i32;
        let mut oc_neg = 0i32;

        // === Positive direction ===
        {
            let mut r = pos.row as i8 + dr;
            let mut c = pos.col as i8 + dc;
            let mut my_active = true;
            let mut my_consec = true;
            let mut opp_active = true;
            let mut opp_consec = true;

            while (my_active || opp_active) && r >= 0 && r < sz && c >= 0 && c < sz {
                let p = Pos::new(r as u8, c as u8);
                let is_my = my_bb.get(p);
                let is_opp = if is_my { false } else { opp_bb.get(p) };

                if is_my {
                    if my_active {
                        mc += 1;
                        if my_consec {
                            mc_pos += 1;
                        }
                    }
                    if opp_active {
                        opp_active = false;
                    }
                } else if is_opp {
                    if opp_active {
                        oc += 1;
                        if opp_consec {
                            oc_pos += 1;
                        }
                    }
                    if my_active {
                        my_active = false;
                    }
                } else {
                    // Empty cell
                    if my_active {
                        if !m_gap {
                            my_consec = false;
                            let nr = r + dr;
                            let nc = c + dc;
                            if nr >= 0
                                && nr < sz
                                && nc >= 0
                                && nc < sz
                                && my_bb.get(Pos::new(nr as u8, nc as u8))
                            {
                                m_gap = true;
                            } else {
                                mo += 1;
                                my_active = false;
                            }
                        } else {
                            mo += 1;
                            my_active = false;
                        }
                    }
                    if opp_active {
                        if !o_gap {
                            opp_consec = false;
                            let nr = r + dr;
                            let nc = c + dc;
                            if nr >= 0
                                && nr < sz
                                && nc >= 0
                                && nc < sz
                                && opp_bb.get(Pos::new(nr as u8, nc as u8))
                            {
                                o_gap = true;
                            } else {
                                oo += 1;
                                opp_active = false;
                            }
                        } else {
                            oo += 1;
                            opp_active = false;
                        }
                    }
                }

                r += dr;
                c += dc;
            }
        }

        // === Negative direction ===
        {
            let mut r = pos.row as i8 - dr;
            let mut c = pos.col as i8 - dc;
            let mut my_active = true;
            let mut my_consec = true;
            let mut opp_active = true;
            let mut opp_consec = true;

            while (my_active || opp_active) && r >= 0 && r < sz && c >= 0 && c < sz {
                let p = Pos::new(r as u8, c as u8);
                let is_my = my_bb.get(p);
                let is_opp = if is_my { false } else { opp_bb.get(p) };

                if is_my {
                    if my_active {
                        mc += 1;
                        if my_consec {
                            mc_neg += 1;
                        }
                    }
                    if opp_active {
                        opp_active = false;
                    }
                } else if is_opp {
                    if opp_active {
                        oc += 1;
                        if opp_consec {
                            oc_neg += 1;
                        }
                    }
                    if my_active {
                        my_active = false;
                    }
                } else {
                    // Empty cell
                    if my_active {
                        if !m_gap {
                            my_consec = false;
                            let nr = r - dr;
                            let nc = c - dc;
                            if nr >= 0
                                && nr < sz
                                && nc >= 0
                                && nc < sz
                                && my_bb.get(Pos::new(nr as u8, nc as u8))
                            {
                                m_gap = true;
                            } else {
                                mo += 1;
                                my_active = false;
                            }
                        } else {
                            mo += 1;
                            my_active = false;
                        }
                    }
                    if opp_active {
                        if !o_gap {
                            opp_consec = false;
                            let nr = r - dr;
                            let nc = c - dc;
                            if nr >= 0
                                && nr < sz
                                && nc >= 0
                                && nc < sz
                                && opp_bb.get(Pos::new(nr as u8, nc as u8))
                            {
                                o_gap = true;
                            } else {
                                oo += 1;
                                opp_active = false;
                            }
                        } else {
                            oo += 1;
                            opp_active = false;
                        }
                    }
                }

                r -= dr;
                c -= dc;
            }
        }

        let mc_consec = 1 + mc_pos + mc_neg;
        let oc_consec = 1 + oc_pos + oc_neg;
        (mc, mo, m_gap, mc_consec, oc, oo, o_gap, oc_consec)
    }

    /// Check if placing our stone at `mov` makes it part of a capturable pair.
    /// Uses direct bitboard access (1 lookup) instead of board.get() (2 lookups).
    fn capture_vulnerability(
        my_bb: &Bitboard,
        opp_bb: &Bitboard,
        mov: Pos,
        opp_captures: u8,
    ) -> i32 {
        let sz = BOARD_SIZE as i8;
        let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        let mut vuln_count = 0i32;
        let mut setup_vuln_count = 0i32;

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
                    let p_rm1 = Pos::new(rm1 as u8, cm1 as u8);
                    let p_rp1 = Pos::new(rp1 as u8, cp1 as u8);
                    let p_rp2 = Pos::new(rp2 as u8, cp2 as u8);

                    let rm1_empty = !my_bb.get(p_rm1) && !opp_bb.get(p_rm1);
                    let rp2_empty = !my_bb.get(p_rp2) && !opp_bb.get(p_rp2);

                    // empty-MOV-ally-opp: opponent can place at before to capture
                    if rm1_empty && my_bb.get(p_rp1) && opp_bb.get(p_rp2) {
                        vuln_count += 1;
                    }
                    // opp-MOV-ally-empty: opponent can place at after2 to capture
                    if opp_bb.get(p_rm1) && my_bb.get(p_rp1) && rp2_empty {
                        vuln_count += 1;
                    }
                    // empty-MOV-ally-empty: 2-move capturable pair (both flanks open)
                    if rm1_empty && my_bb.get(p_rp1) && rp2_empty {
                        setup_vuln_count += 1;
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
                    let p_rm2 = Pos::new(rm2 as u8, cm2 as u8);
                    let p_rm1 = Pos::new(rm1 as u8, cm1 as u8);
                    let p_rp1 = Pos::new(rp1 as u8, cp1 as u8);

                    let rm2_empty = !my_bb.get(p_rm2) && !opp_bb.get(p_rm2);
                    let rp1_empty = !my_bb.get(p_rp1) && !opp_bb.get(p_rp1);

                    // empty-ally-MOV-opp: opponent can place at before2 to capture
                    if rm2_empty && my_bb.get(p_rm1) && opp_bb.get(p_rp1) {
                        vuln_count += 1;
                    }
                    // opp-ally-MOV-empty: opponent can place at after to capture
                    if opp_bb.get(p_rm2) && my_bb.get(p_rm1) && rp1_empty {
                        vuln_count += 1;
                    }
                    // empty-ally-MOV-empty: 2-move capturable pair (both flanks open)
                    if rm2_empty && my_bb.get(p_rm1) && rp1_empty {
                        setup_vuln_count += 1;
                    }
                }
            }
        }

        let total = vuln_count + setup_vuln_count;
        if total > 0 {
            let opp_caps = i32::from(opp_captures);
            let base_penalty = 20_000;
            let urgency = if opp_caps >= 3 {
                4
            } else if opp_caps >= 2 {
                2
            } else {
                1
            };
            // Immediate threats get full penalty, setup threats get half
            vuln_count * base_penalty * urgency + setup_vuln_count * (base_penalty / 2) * urgency
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
            countermove: [[[None; BOARD_SIZE]; BOARD_SIZE]; 2],
            last_move_for_ordering: None,
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
        // Add completion buffer: allows iterative deepening to finish
        // the current depth after soft limit is reached.
        let time_limit = Duration::from_millis(time_limit_ms + 150);

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
            countermove: [[[None; BOARD_SIZE]; BOARD_SIZE]; 2],
            last_move_for_ordering: None,
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
            countermove: [[[None; BOARD_SIZE]; BOARD_SIZE]; 2],
            last_move_for_ordering: None,
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
            countermove: [[[None; BOARD_SIZE]; BOARD_SIZE]; 2],
            last_move_for_ordering: None,
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

    /// Reference implementation of count_line_with_gap (original, pre-optimization).
    /// Used to verify count_line_both produces identical results.
    fn ref_count_line_with_gap(
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

    #[test]
    fn test_count_line_both_equivalence() {
        // Test on multiple board configurations
        let configs: Vec<Vec<(u8, u8, Stone)>> = vec![
            // Config 1: horizontal line with gap
            vec![
                (9, 9, Stone::Black), (9, 10, Stone::Black), (9, 12, Stone::Black),
                (9, 7, Stone::White), (9, 13, Stone::White),
            ],
            // Config 2: diagonal stones
            vec![
                (5, 5, Stone::Black), (6, 6, Stone::Black), (8, 8, Stone::Black),
                (7, 7, Stone::White), (9, 9, Stone::White),
            ],
            // Config 3: mixed captures scenario
            vec![
                (10, 10, Stone::Black), (10, 11, Stone::Black), (10, 12, Stone::White),
                (10, 13, Stone::White), (10, 14, Stone::Black),
                (11, 10, Stone::Black), (12, 10, Stone::Black),
            ],
            // Config 4: dense center
            vec![
                (9, 8, Stone::Black), (9, 9, Stone::White), (9, 10, Stone::Black),
                (9, 11, Stone::White), (9, 12, Stone::Black),
                (8, 9, Stone::Black), (10, 9, Stone::Black),
                (8, 10, Stone::White), (10, 10, Stone::White),
            ],
            // Config 5: edge positions
            vec![
                (0, 0, Stone::Black), (0, 1, Stone::Black), (0, 2, Stone::Black),
                (1, 0, Stone::White), (1, 1, Stone::White),
            ],
            // Config 6: corner gap patterns
            vec![
                (17, 17, Stone::Black), (17, 16, Stone::Black), (17, 14, Stone::Black),
                (18, 18, Stone::White), (16, 16, Stone::White),
            ],
            // Config 7: empty board (trivial)
            vec![],
        ];

        let directions: [(i8, i8); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];
        let mut total_checks = 0;

        for (cfg_idx, stones) in configs.iter().enumerate() {
            let mut board = Board::new();
            for &(r, c, color) in stones {
                board.place_stone(Pos::new(r, c), color);
            }

            let black_bb = board.stones(Stone::Black).unwrap();
            let white_bb = board.stones(Stone::White).unwrap();

            // Check every empty position in a relevant area
            for r in 0u8..BOARD_SIZE as u8 {
                for c in 0u8..BOARD_SIZE as u8 {
                    let pos = Pos::new(r, c);
                    if board.get(pos) != Stone::Empty {
                        continue;
                    }

                    for &(dr, dc) in &directions {
                        // Reference: two separate calls
                        let (bc, bo, bg, bcon) =
                            ref_count_line_with_gap(&board, pos, dr, dc, Stone::Black);
                        let (wc, wo, wg, wcon) =
                            ref_count_line_with_gap(&board, pos, dr, dc, Stone::White);

                        // Optimized: single merged call
                        let (mc, mo, mg, mcon, oc, oo, og, ocon) =
                            WorkerSearcher::count_line_both(black_bb, white_bb, pos, dr, dc);

                        assert_eq!(
                            (bc, bo, bg, bcon), (mc, mo, mg, mcon),
                            "Black mismatch at cfg={} pos=({},{}) dir=({},{}): \
                             ref=({},{},{},{}) new=({},{},{},{})",
                            cfg_idx, r, c, dr, dc,
                            bc, bo, bg, bcon, mc, mo, mg, mcon
                        );
                        assert_eq!(
                            (wc, wo, wg, wcon), (oc, oo, og, ocon),
                            "White mismatch at cfg={} pos=({},{}) dir=({},{}): \
                             ref=({},{},{},{}) new=({},{},{},{})",
                            cfg_idx, r, c, dr, dc,
                            wc, wo, wg, wcon, oc, oo, og, ocon
                        );
                        total_checks += 1;
                    }
                }
            }
        }
        assert!(total_checks > 5000, "Should have checked many positions, got {}", total_checks);
    }
}
