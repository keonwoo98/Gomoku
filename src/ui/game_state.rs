//! Game state management for the Gomoku GUI

use crate::{AIEngine, Board, MoveResult, Pos, Stone, ai_log, pos_to_notation, rules};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

/// Opening rule variants for game start
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningRule {
    /// No restrictions
    Standard,
    /// Move 1: center, Move 3: ≥3 intersections from center
    Pro,
    /// After move 3, second player may swap colors
    Swap,
}

impl Default for OpeningRule {
    fn default() -> Self {
        OpeningRule::Standard
    }
}

/// Game mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    /// Player vs AI
    PvE {
        human_color: Stone,
    },
    /// Player vs Player (hotseat)
    PvP {
        show_suggestions: bool,
    },
    /// AI vs AI (spectator mode)
    AiVsAi,
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::PvE { human_color: Stone::Black }
    }
}

/// AI computation state
pub enum AiState {
    Idle,
    Thinking {
        receiver: Receiver<(MoveResult, AIEngine)>,
        start_time: Instant,
    },
    /// Timed out but still waiting for the thread to finish so we can reclaim the engine.
    /// This prevents losing the 64MB TT cache on timeout.
    Reclaiming {
        receiver: Receiver<(MoveResult, AIEngine)>,
    },
}

/// Capture animation state
#[derive(Clone)]
pub struct CaptureAnimation {
    pub positions: Vec<Pos>,
    pub start_time: Instant,
    pub captured_color: Stone,
}

impl CaptureAnimation {
    pub fn new(positions: Vec<Pos>, color: Stone) -> Self {
        Self {
            positions,
            start_time: Instant::now(),
            captured_color: color,
        }
    }

    /// Returns animation progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        let elapsed = self.start_time.elapsed().as_secs_f32();
        (elapsed / 0.6).min(1.0) // 0.6 second animation
    }

    pub fn is_complete(&self) -> bool {
        self.progress() >= 1.0
    }
}

/// Cumulative AI statistics across a game
#[derive(Default, Clone)]
pub struct AiStats {
    /// Number of AI moves made
    pub move_count: u32,
    /// Total time spent by AI (ms)
    pub total_time_ms: u64,
    /// Total nodes searched across all moves
    pub total_nodes: u64,
    /// Maximum depth reached in any single move
    pub max_depth: i8,
    /// Fastest move (ms)
    pub min_time_ms: u64,
    /// Slowest move (ms)
    pub max_time_ms: u64,
    /// History of per-move times for display
    pub move_times: Vec<u64>,
    /// History of per-move depths
    pub move_depths: Vec<i8>,
}

impl AiStats {
    pub fn record(&mut self, result: &MoveResult) {
        self.move_count += 1;
        self.total_time_ms += result.time_ms;
        self.total_nodes += result.nodes;
        if result.depth > self.max_depth {
            self.max_depth = result.depth;
        }
        if self.move_count == 1 {
            self.min_time_ms = result.time_ms;
            self.max_time_ms = result.time_ms;
        } else {
            if result.time_ms < self.min_time_ms {
                self.min_time_ms = result.time_ms;
            }
            if result.time_ms > self.max_time_ms {
                self.max_time_ms = result.time_ms;
            }
        }
        self.move_times.push(result.time_ms);
        self.move_depths.push(result.depth);
    }

    /// Average time excluding non-search moves (depth=0 from VCF/Defense/Opening).
    /// This reflects actual alpha-beta search time, matching avg_depth() filtering.
    pub fn avg_time_ms(&self) -> f64 {
        let search_times: Vec<_> = self.move_times.iter().zip(self.move_depths.iter())
            .filter(|(_, &d)| d > 0)
            .map(|(&t, _)| t)
            .collect();
        if search_times.is_empty() { 0.0 } else { search_times.iter().sum::<u64>() as f64 / search_times.len() as f64 }
    }

    /// Average depth excluding non-search moves (depth=0 from VCF/Defense/Opening).
    /// This reflects actual alpha-beta search depth, which is what evaluators check.
    pub fn avg_depth(&self) -> f64 {
        let search_depths: Vec<_> = self.move_depths.iter().filter(|&&d| d > 0).collect();
        if search_depths.is_empty() { 0.0 } else { search_depths.iter().map(|&&d| d as f64).sum::<f64>() / search_depths.len() as f64 }
    }

    /// Time range for search moves only (depth > 0).
    /// Returns (min_ms, max_ms). Returns raw values if no search moves exist.
    pub fn search_time_range(&self) -> (u64, u64) {
        let mut min_t = u64::MAX;
        let mut max_t = 0u64;
        let mut found = false;
        for (&t, &d) in self.move_times.iter().zip(self.move_depths.iter()) {
            if d > 0 {
                found = true;
                if t < min_t { min_t = t; }
                if t > max_t { max_t = t; }
            }
        }
        if found { (min_t, max_t) } else { (self.min_time_ms, self.max_time_ms) }
    }

    pub fn avg_nps(&self) -> u64 {
        if self.total_time_ms == 0 { 0 } else { self.total_nodes * 1000 / self.total_time_ms / 1000 }
    }
}

/// Main game state
pub struct GameState {
    pub board: Board,
    pub mode: GameMode,
    pub current_turn: Stone,
    pub game_over: Option<GameResult>,
    pub last_move: Option<Pos>,
    pub move_history: Vec<(Pos, Stone)>,
    pub last_ai_result: [Option<MoveResult>; 2],
    pub ai_state: AiState,
    pub move_timer: MoveTimer,
    pub suggested_move: Option<Pos>,
    pub message: Option<String>,
    pub capture_animation: Option<CaptureAnimation>,
    pub ai_stats: [AiStats; 2],
    /// Review mode: when Some(index), shows board at move #index
    pub review_index: Option<usize>,
    /// Redo stack: each entry is a group of moves (1 for PvP, 2 for PvE)
    pub redo_groups: Vec<Vec<(Pos, Stone)>>,
    /// Opening rule for this game
    pub opening_rule: OpeningRule,
    /// Swap rule: waiting for swap decision after 3rd move
    pub swap_pending: bool,
    /// Per-color last move duration [Black, White]
    pub last_move_time: [Option<std::time::Duration>; 2],

    // Persistent AI engine (reuses TT across moves)
    ai_engine: Option<AIEngine>,

    // AI engine configuration
    ai_depth: i8,
    ai_time_limit_ms: u64,
}

/// Game result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameResult {
    pub winner: Stone,
    pub win_type: WinType,
    pub winning_line: Option<[Pos; 5]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinType {
    FiveInRow,
    Capture,
}

/// Move timer for tracking thinking time
pub struct MoveTimer {
    pub start_time: Option<Instant>,
    pub last_move_duration: Option<Duration>,
    pub ai_thinking_time: Option<Duration>,
}

impl Default for MoveTimer {
    fn default() -> Self {
        Self {
            start_time: Some(Instant::now()),
            last_move_duration: None,
            ai_thinking_time: None,
        }
    }
}

impl MoveTimer {
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn stop(&mut self) -> Duration {
        let duration = self.elapsed();
        self.last_move_duration = Some(duration);
        self.start_time = None;
        duration
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.map_or(Duration::ZERO, |t| t.elapsed())
    }

    pub fn set_ai_time(&mut self, duration: Duration) {
        self.ai_thinking_time = Some(duration);
    }
}

impl GameState {
    pub fn new(mode: GameMode) -> Self {
        Self::with_opening_rule(mode, OpeningRule::Standard)
    }

    pub fn with_opening_rule(mode: GameMode, opening_rule: OpeningRule) -> Self {
        Self {
            board: Board::new(),
            mode,
            current_turn: Stone::Black,
            game_over: None,
            last_move: None,
            move_history: Vec::new(),
            last_ai_result: [None, None],
            ai_state: AiState::Idle,
            move_timer: MoveTimer::default(),
            suggested_move: None,
            message: None,
            capture_animation: None,
            ai_stats: [AiStats::default(), AiStats::default()],
            review_index: None,
            redo_groups: Vec::new(),
            opening_rule,
            swap_pending: false,
            last_move_time: [None, None],
            ai_engine: Some(AIEngine::with_config(64, 20, 500)),
            ai_depth: 20,
            ai_time_limit_ms: 500,
        }
    }

    pub fn reset(&mut self) {
        self.board = Board::new();
        self.current_turn = Stone::Black;
        self.game_over = None;
        self.last_move = None;
        self.move_history.clear();
        self.last_ai_result = [None, None];
        self.ai_state = AiState::Idle;
        self.move_timer = MoveTimer::default();
        self.suggested_move = None;
        self.message = None;
        self.capture_animation = None;
        self.ai_stats = [AiStats::default(), AiStats::default()];
        self.review_index = None;
        self.redo_groups.clear();
        self.swap_pending = false;
        self.last_move_time = [None, None];
        if let Some(ref mut engine) = self.ai_engine {
            engine.clear_cache();
        }
    }

    /// Execute color swap (Swap rule)
    pub fn execute_swap(&mut self) {
        self.swap_pending = false;
        match &mut self.mode {
            GameMode::PvE { human_color } => {
                *human_color = human_color.opponent();
            }
            GameMode::PvP { .. } | GameMode::AiVsAi => {
                // PvP/AiVsAi: conceptual swap
            }
        }
        self.message = Some("Colors swapped!".to_string());
    }

    /// Decline color swap (Swap rule)
    pub fn decline_swap(&mut self) {
        self.swap_pending = false;
        self.message = Some("Swap declined, game continues.".to_string());
    }

    /// Check if it's the human's turn
    pub fn is_human_turn(&self) -> bool {
        match self.mode {
            GameMode::PvE { human_color } => self.current_turn == human_color,
            GameMode::PvP { .. } => true,
            GameMode::AiVsAi => false,
        }
    }

    /// Check if it's the AI's turn
    pub fn is_ai_turn(&self) -> bool {
        match self.mode {
            GameMode::PvE { human_color } => self.current_turn != human_color,
            GameMode::PvP { .. } => false,
            GameMode::AiVsAi => true,
        }
    }

    /// Check if AI is currently thinking
    pub fn is_ai_thinking(&self) -> bool {
        matches!(self.ai_state, AiState::Thinking { .. })
    }

    /// Attempt to place a stone at the given position
    pub fn try_place_stone(&mut self, pos: Pos) -> Result<(), String> {
        if self.game_over.is_some() {
            return Err("Game is over".to_string());
        }

        if self.is_ai_thinking() {
            return Err("AI is thinking".to_string());
        }

        if !self.is_human_turn() {
            return Err("Not your turn".to_string());
        }

        // Pro rule validation
        if self.opening_rule == OpeningRule::Pro {
            let move_num = self.move_history.len() + 1;
            if move_num == 1 && pos != Pos::new(9, 9) {
                return Err("Pro rule: First move must be at center (K10)".to_string());
            }
            if move_num == 3 {
                let center = 9i32;
                let dr = (i32::from(pos.row) - center).abs();
                let dc = (i32::from(pos.col) - center).abs();
                if dr.max(dc) < 3 {
                    return Err("Pro rule: 3rd move must be ≥3 intersections from center".to_string());
                }
            }
        }

        // Check if move is valid
        if !self.board.is_empty(pos) {
            return Err("Position is occupied".to_string());
        }
        if rules::is_double_three(&self.board, pos, self.current_turn) {
            return Err("Forbidden: Double-three".to_string());
        }
        if !rules::is_valid_move(&self.board, pos, self.current_turn) {
            return Err("Invalid move".to_string());
        }

        // New move invalidates redo history
        self.redo_groups.clear();

        // Place the stone
        self.execute_move(pos);
        Ok(())
    }

    /// Execute a move (for both human and AI)
    fn execute_move(&mut self, pos: Pos) {
        let color = self.current_turn;
        let is_human = !self.is_ai_turn();
        let move_num = self.move_history.len() + 1;

        // Place stone and handle captures
        self.board.place_stone(pos, color);
        let captured_positions = rules::execute_captures(&mut self.board, pos, color);
        let capture_count = captured_positions.len() / 2; // Each capture is a pair

        // Log moves for game reconstruction
        let color_str = if color == Stone::Black { "Black" } else { "White" };
        let cap_str = if capture_count > 0 {
            format!(" +{}cap [{}]", capture_count,
                captured_positions.iter().map(|p| pos_to_notation(*p)).collect::<Vec<_>>().join(", "))
        } else {
            String::new()
        };
        if is_human {
            ai_log(&format!("  >> Human #{}: {} plays {}{}",
                move_num, color_str, pos_to_notation(pos), cap_str));
        } else {
            ai_log(&format!("  >> AI #{}: {} plays {}{}",
                move_num, color_str, pos_to_notation(pos), cap_str));
        }

        // Start capture animation if any captures occurred
        if !captured_positions.is_empty() {
            self.capture_animation = Some(CaptureAnimation::new(
                captured_positions,
                color.opponent(), // Captured stones are opponent's color
            ));
        }

        // Record move
        self.move_history.push((pos, color));
        self.last_move = Some(pos);
        self.suggested_move = None;

        // Stop timer and record per-color duration
        let duration = self.move_timer.stop();
        let idx = if color == Stone::Black { 0 } else { 1 };
        self.last_move_time[idx] = Some(duration);

        // Check for win
        if let Some(result) = self.check_win(pos, color) {
            let winner_str = if result.winner == Stone::Black { "BLACK" } else { "WHITE" };
            let win_type_str = match result.win_type {
                WinType::FiveInRow => "5-in-a-row",
                WinType::Capture => "capture",
            };
            ai_log(&format!("\n*** GAME OVER: {} WINS by {} (move #{}) ***",
                winner_str, win_type_str, move_num));
            self.game_over = Some(result);
            return;
        }

        // Switch turn
        self.current_turn = color.opponent();
        self.move_timer.start();

        // Swap rule: after 3rd move, trigger swap decision
        if self.opening_rule == OpeningRule::Swap && self.move_history.len() == 3 {
            self.swap_pending = true;
        }

        // Clear message
        self.message = None;
    }

    /// Check for win condition
    fn check_win(&self, pos: Pos, color: Stone) -> Option<GameResult> {
        // Check capture win
        let total_captures = if color == Stone::Black {
            self.board.black_captures
        } else {
            self.board.white_captures
        };

        if total_captures >= 5 {
            return Some(GameResult {
                winner: color,
                win_type: WinType::Capture,
                winning_line: None,
            });
        }

        // Check if the OPPONENT already had a five from a previous turn.
        // In Ninuki-renju, a breakable five gives the opponent one chance to
        // capture and break it. If they fail (don't break it), the five-holder wins.
        let opponent = color.opponent();
        if let Some(opp_five) = rules::find_five_positions(&self.board, opponent) {
            let winning_line = if opp_five.len() >= 5 {
                Some([opp_five[0], opp_five[1], opp_five[2], opp_five[3], opp_five[4]])
            } else {
                None
            };
            return Some(GameResult {
                winner: opponent,
                win_type: WinType::FiveInRow,
                winning_line,
            });
        }

        // Check five-in-a-row by the current player
        if let Some(line) = self.find_winning_line(pos, color) {
            let line_vec: Vec<Pos> = line.to_vec();
            if !rules::can_break_five_by_capture(&self.board, &line_vec, color) {
                return Some(GameResult {
                    winner: color,
                    win_type: WinType::FiveInRow,
                    winning_line: Some(line),
                });
            }
            // Five is breakable — opponent gets one chance to break it
        }

        None
    }

    /// Find the winning line if exists
    fn find_winning_line(&self, pos: Pos, color: Stone) -> Option<[Pos; 5]> {
        let directions: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

        for (dr, dc) in directions {
            let mut line = Vec::new();

            // Count in negative direction
            let mut r = pos.row as i8;
            let mut c = pos.col as i8;
            while r >= 0 && r < 19 && c >= 0 && c < 19 {
                let p = Pos::new(r as u8, c as u8);
                if self.board.get(p) == color {
                    line.insert(0, p);
                    r -= dr;
                    c -= dc;
                } else {
                    break;
                }
            }

            // Count in positive direction (skip center)
            r = pos.row as i8 + dr;
            c = pos.col as i8 + dc;
            while r >= 0 && r < 19 && c >= 0 && c < 19 {
                let p = Pos::new(r as u8, c as u8);
                if self.board.get(p) == color {
                    line.push(p);
                    r += dr;
                    c += dc;
                } else {
                    break;
                }
            }

            if line.len() >= 5 {
                return Some([line[0], line[1], line[2], line[3], line[4]]);
            }
        }

        None
    }

    /// Start AI thinking
    pub fn start_ai_thinking(&mut self) {
        if !self.is_ai_turn() || self.is_ai_thinking() || self.game_over.is_some() {
            return;
        }

        // If still reclaiming engine from a timed-out search, try once more before proceeding
        if matches!(self.ai_state, AiState::Reclaiming { .. }) {
            self.try_reclaim_engine();
            if matches!(self.ai_state, AiState::Reclaiming { .. }) {
                // Still waiting — skip this frame, will retry next frame
                return;
            }
        }

        let board = self.board.clone();
        let color = self.current_turn;

        // Take engine out (will be returned after search)
        let mut engine = match self.ai_engine.take() {
            Some(e) => e,
            None => AIEngine::with_config(64, self.ai_depth, self.ai_time_limit_ms),
        };

        let (tx, rx) = channel();

        thread::spawn(move || {
            let result = engine.get_move_with_stats(&board, color);
            let _ = tx.send((result, engine));
        });

        self.ai_state = AiState::Thinking {
            receiver: rx,
            start_time: Instant::now(),
        };
    }

    /// Check if AI has finished thinking
    pub fn check_ai_result(&mut self) {
        // Try to reclaim engine from a previously timed-out search.
        // This runs every frame and recovers the engine + TT cache once the thread finishes.
        self.try_reclaim_engine();

        // Check if AI has timed out (5 seconds)
        let should_force_move = match &self.ai_state {
            AiState::Thinking { start_time, .. } => {
                start_time.elapsed() > Duration::from_secs(5)
            }
            _ => false,
        };

        // If timed out, transition to Reclaiming (keep receiver!) and play fallback
        if should_force_move {
            // Take the current state and extract the receiver for background reclamation
            let old_state = std::mem::replace(&mut self.ai_state, AiState::Idle);
            if let AiState::Thinking { receiver, .. } = old_state {
                self.ai_state = AiState::Reclaiming { receiver };
            }
            self.message = Some("AI timeout - quick move".to_string());

            if let Some(fallback) = self.find_fallback_move() {
                let fallback = self.validate_pro_rule_ai_move(fallback);
                self.execute_move(fallback);
            }
            return;
        }

        let result = match &self.ai_state {
            AiState::Thinking { receiver, start_time } => {
                match receiver.try_recv() {
                    Ok((result, engine)) => Some((result, engine, start_time.elapsed())),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.ai_state = AiState::Idle;
                        self.message = Some("AI error".to_string());
                        return;
                    }
                }
            }
            _ => None,
        };

        if let Some((move_result, engine, elapsed)) = result {
            self.ai_state = AiState::Idle;
            self.ai_engine = Some(engine); // Return engine for reuse
            let idx = if self.current_turn == Stone::Black { 0 } else { 1 };
            self.ai_stats[idx].record(&move_result);
            self.last_ai_result[idx] = Some(move_result.clone());
            self.move_timer.set_ai_time(elapsed);

            if let Some(pos) = move_result.best_move {
                // Validate AI move against Pro rule
                let pos = self.validate_pro_rule_ai_move(pos);
                self.execute_move(pos);
            } else {
                self.message = Some("AI could not find a move".to_string());
            }
        }
    }

    /// Validate AI move against Pro rule constraints.
    /// Returns the original move if valid, or a corrected move if not.
    fn validate_pro_rule_ai_move(&self, pos: Pos) -> Pos {
        if self.opening_rule != OpeningRule::Pro {
            return pos;
        }
        let move_num = self.move_history.len() + 1;
        if move_num == 1 {
            // First move must be center
            return Pos::new(9, 9);
        }
        if move_num == 3 {
            let center = 9i32;
            let dr = (i32::from(pos.row) - center).abs();
            let dc = (i32::from(pos.col) - center).abs();
            if dr.max(dc) < 3 {
                // AI chose a position too close to center — find best valid alternative
                let mut best: Option<Pos> = None;
                let mut best_dist = i32::MAX;
                for r in 0..19u8 {
                    for c in 0..19u8 {
                        let p = Pos::new(r, c);
                        if !self.board.is_empty(p) {
                            continue;
                        }
                        let pr = (i32::from(r) - center).abs();
                        let pc = (i32::from(c) - center).abs();
                        if pr.max(pc) < 3 {
                            continue;
                        }
                        // Pick the closest valid position to AI's original choice
                        let dist = (i32::from(r) - i32::from(pos.row)).abs()
                            + (i32::from(c) - i32::from(pos.col)).abs();
                        if dist < best_dist {
                            best_dist = dist;
                            best = Some(p);
                        }
                    }
                }
                if let Some(alt) = best {
                    return alt;
                }
            }
        }
        pos
    }

    /// Try to reclaim the AI engine from a timed-out search thread.
    /// Called every frame — once the thread finishes, we get the engine back
    /// with its full TT cache intact, avoiding expensive re-creation.
    fn try_reclaim_engine(&mut self) {
        if let AiState::Reclaiming { receiver } = &self.ai_state {
            match receiver.try_recv() {
                Ok((_result, engine)) => {
                    self.ai_engine = Some(engine);
                    self.ai_state = AiState::Idle;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Thread still running — will try again next frame
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Thread panicked or dropped sender — give up gracefully
                    if self.ai_engine.is_none() {
                        self.ai_engine = Some(AIEngine::with_config(
                            64, self.ai_depth, self.ai_time_limit_ms,
                        ));
                    }
                    self.ai_state = AiState::Idle;
                }
            }
        }
    }

    /// Find a quick fallback move when AI times out
    fn find_fallback_move(&self) -> Option<Pos> {
        let color = self.current_turn;

        // 1. Try to find a winning move
        for r in 0..19u8 {
            for c in 0..19u8 {
                let pos = Pos::new(r, c);
                if rules::is_valid_move(&self.board, pos, color) {
                    let mut test = self.board.clone();
                    test.place_stone(pos, color);
                    rules::execute_captures(&mut test, pos, color);
                    if rules::check_winner(&test) == Some(color) {
                        return Some(pos);
                    }
                }
            }
        }

        // 2. Try to block opponent's winning move
        let opponent = color.opponent();
        for r in 0..19u8 {
            for c in 0..19u8 {
                let pos = Pos::new(r, c);
                if rules::is_valid_move(&self.board, pos, opponent) {
                    let mut test = self.board.clone();
                    test.place_stone(pos, opponent);
                    rules::execute_captures(&mut test, pos, opponent);
                    if rules::check_winner(&test) == Some(opponent) {
                        // Opponent would win here, so block it
                        if rules::is_valid_move(&self.board, pos, color) {
                            return Some(pos);
                        }
                    }
                }
            }
        }

        // 3. Find any valid move near existing stones
        if let Some(last) = self.last_move {
            for dr in -2i8..=2 {
                for dc in -2i8..=2 {
                    let r = last.row as i8 + dr;
                    let c = last.col as i8 + dc;
                    if r >= 0 && r < 19 && c >= 0 && c < 19 {
                        let pos = Pos::new(r as u8, c as u8);
                        if rules::is_valid_move(&self.board, pos, color) {
                            return Some(pos);
                        }
                    }
                }
            }
        }

        // 4. Any valid move near center
        for r in 7..12u8 {
            for c in 7..12u8 {
                let pos = Pos::new(r, c);
                if rules::is_valid_move(&self.board, pos, color) {
                    return Some(pos);
                }
            }
        }

        None
    }

    /// Get AI thinking elapsed time
    pub fn ai_thinking_elapsed(&self) -> Option<Duration> {
        match &self.ai_state {
            AiState::Thinking { start_time, .. } => Some(start_time.elapsed()),
            AiState::Idle | AiState::Reclaiming { .. } => None,
        }
    }

    /// Request move suggestion for PvP mode
    pub fn request_suggestion(&mut self) {
        if self.game_over.is_some() || self.is_ai_thinking() {
            return;
        }

        let board = self.board.clone();
        let color = self.current_turn;

        // Run quick suggestion (lower depth)
        let mut engine = AIEngine::with_config(16, 4, 200);
        let result = engine.get_move_with_stats(&board, color);

        self.suggested_move = result.best_move;
        let idx = if color == Stone::Black { 0 } else { 1 };
        self.last_ai_result[idx] = Some(result);
    }

    /// Undo last move
    pub fn undo(&mut self) {
        if self.move_history.is_empty() || self.is_ai_thinking() {
            return;
        }

        // Exit review mode if active
        self.review_index = None;

        // For PvE, undo two moves (human + AI); AiVsAi undo one move
        let undo_count = match self.mode {
            GameMode::PvE { .. } if self.move_history.len() >= 2 => 2,
            _ => 1,
        };

        // Save undone moves for redo
        let keep = self.move_history.len().saturating_sub(undo_count);
        let redo_moves: Vec<_> = self.move_history[keep..].to_vec();
        self.redo_groups.push(redo_moves);

        // Truncate and replay
        let moves: Vec<_> = self.move_history[..keep].to_vec();

        self.board = Board::new();
        self.current_turn = Stone::Black;
        self.game_over = None;
        self.last_move = None;
        self.suggested_move = None;
        self.capture_animation = None;
        self.move_history.clear();

        for (pos, color) in moves {
            self.board.place_stone(pos, color);
            rules::execute_captures(&mut self.board, pos, color);
            self.move_history.push((pos, color));
            self.last_move = Some(pos);
            self.current_turn = color.opponent();
        }

        self.move_timer.start();
    }

    /// Redo last undone move(s)
    pub fn redo(&mut self) {
        if self.redo_groups.is_empty() || self.is_ai_thinking() {
            return;
        }

        // Exit review mode if active
        self.review_index = None;

        if let Some(moves) = self.redo_groups.pop() {
            for (pos, _color) in moves {
                if self.game_over.is_some() {
                    break;
                }
                self.execute_move(pos);
            }
        }
    }

    /// Build a board from a subset of moves (for review mode)
    pub fn build_review_board(&self, up_to: usize) -> (Board, Option<Pos>) {
        let mut board = Board::new();
        let mut last = None;
        for &(pos, color) in self.move_history.iter().take(up_to) {
            board.place_stone(pos, color);
            rules::execute_captures(&mut board, pos, color);
            last = Some(pos);
        }
        (board, last)
    }

    /// Navigate review mode
    pub fn review_prev(&mut self) {
        if self.game_over.is_none() { return; }
        let current = self.review_index.unwrap_or(self.move_history.len());
        if current > 0 {
            self.review_index = Some(current - 1);
        }
    }

    pub fn review_next(&mut self) {
        if self.game_over.is_none() { return; }
        if let Some(idx) = self.review_index {
            if idx < self.move_history.len() {
                let next = idx + 1;
                if next >= self.move_history.len() {
                    self.review_index = None; // Back to final position
                } else {
                    self.review_index = Some(next);
                }
            }
        }
    }

    /// Check if currently reviewing a past position
    pub fn is_reviewing(&self) -> bool {
        self.review_index.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reproduce the exact game position where the UI incorrectly declared a win.
    /// F13-G12-H11-J10-K9 diagonal five is breakable because White can capture
    /// J10+H10 by placing at K10 (pattern: K10(W)-J10(B)-H10(B)-G10(W)).
    #[test]
    fn test_five_in_row_breakable_by_capture() {
        let mut state = GameState::new(GameMode::PvP { show_suggestions: false });

        // Build the exact board position from the game:
        // Black stones: J10, H10, K9, K11, J9, M9, K12, H11, G12
        // White stones: H12, L10, G10, K8, M8, H9, N9, K13, J11, L8
        // (K10 and L9 were captured by J11, freeing those positions)

        let moves: Vec<(u8, u8, Stone)> = vec![
            // White at G10 (9,6) — key flanking stone for capture
            (9, 6, Stone::White),
            // Black stones forming the diagonal
            (8, 9, Stone::Black),  // K9
            (9, 8, Stone::Black),  // J10
            (9, 7, Stone::Black),  // H10 (captured with J10 if White places K10)
            (10, 7, Stone::Black), // H11
            (11, 6, Stone::Black), // G12
            // White stones to make position realistic
            (10, 8, Stone::White), // J11
            (11, 7, Stone::White), // H12
        ];

        for (r, c, color) in moves {
            state.board.place_stone(Pos::new(r, c), color);
        }

        // Now Black plays F13 = Pos(12, 5) completing diagonal five
        state.current_turn = Stone::Black;
        // Place the stone manually to test check_win
        let f13 = Pos::new(12, 5);
        state.board.place_stone(f13, Stone::Black);

        // The five F13-G12-H11-J10-K9 should NOT be declared a win
        // because White can capture J10+H10 by placing at K10(9,9):
        // K10(W,9,9) - J10(B,9,8) - H10(B,9,7) - G10(W,9,6) = X-O-O-X
        let result = state.check_win(f13, Stone::Black);
        assert!(
            result.is_none(),
            "Five F13-G12-H11-J10-K9 should NOT be a win: White can break it by capturing J10+H10"
        );
    }

    /// Verify that a genuine unbreakable five IS declared as a win.
    #[test]
    fn test_five_in_row_unbreakable_wins() {
        let mut state = GameState::new(GameMode::PvP { show_suggestions: false });

        // Simple horizontal five with no White stones nearby to capture
        for i in 5..10 {
            state.board.place_stone(Pos::new(9, i), Stone::Black);
        }
        // Add a distant White stone
        state.board.place_stone(Pos::new(0, 0), Stone::White);

        let result = state.check_win(Pos::new(9, 7), Stone::Black);
        assert!(result.is_some(), "Unbreakable five should be declared a win");
        assert_eq!(result.unwrap().win_type, WinType::FiveInRow);
    }

    /// When a breakable five exists and the opponent fails to break it,
    /// the five-holder should win.
    #[test]
    fn test_breakable_five_wins_when_not_broken() {
        let mut state = GameState::new(GameMode::PvP { show_suggestions: false });

        // Same breakable five setup: Black diagonal K9-J10-H11-G12-F13
        // White at G10 enables capture of J10+H10 via K10
        state.board.place_stone(Pos::new(9, 6), Stone::White);  // G10
        state.board.place_stone(Pos::new(8, 9), Stone::Black);  // K9
        state.board.place_stone(Pos::new(9, 8), Stone::Black);  // J10
        state.board.place_stone(Pos::new(9, 7), Stone::Black);  // H10 (capture target)
        state.board.place_stone(Pos::new(10, 7), Stone::Black); // H11
        state.board.place_stone(Pos::new(11, 6), Stone::Black); // G12
        state.board.place_stone(Pos::new(12, 5), Stone::Black); // F13 (completes five)
        state.board.place_stone(Pos::new(10, 8), Stone::White); // J11
        state.board.place_stone(Pos::new(11, 7), Stone::White); // H12

        // Black's five is breakable — check_win returns None (opponent gets a chance)
        let f13 = Pos::new(12, 5);
        let result = state.check_win(f13, Stone::Black);
        assert!(result.is_none(), "Breakable five should not win immediately");

        // White plays A1 — does NOT break the five
        let a1 = Pos::new(0, 0);
        state.board.place_stone(a1, Stone::White);

        // Now check_win should detect Black's surviving five → Black wins
        let result = state.check_win(a1, Stone::White);
        assert!(result.is_some(), "Black should win: White failed to break the five");
        assert_eq!(result.unwrap().winner, Stone::Black);
        assert_eq!(result.unwrap().win_type, WinType::FiveInRow);
    }

    /// When a breakable five is successfully broken by capture,
    /// the game should continue.
    #[test]
    fn test_breakable_five_broken_by_capture_continues() {
        let mut state = GameState::new(GameMode::PvP { show_suggestions: false });

        // Same breakable five setup
        state.board.place_stone(Pos::new(9, 6), Stone::White);  // G10
        state.board.place_stone(Pos::new(8, 9), Stone::Black);  // K9
        state.board.place_stone(Pos::new(9, 8), Stone::Black);  // J10
        state.board.place_stone(Pos::new(9, 7), Stone::Black);  // H10
        state.board.place_stone(Pos::new(10, 7), Stone::Black); // H11
        state.board.place_stone(Pos::new(11, 6), Stone::Black); // G12
        state.board.place_stone(Pos::new(12, 5), Stone::Black); // F13
        state.board.place_stone(Pos::new(10, 8), Stone::White); // J11
        state.board.place_stone(Pos::new(11, 7), Stone::White); // H12

        // Black's five is breakable
        let result = state.check_win(Pos::new(12, 5), Stone::Black);
        assert!(result.is_none());

        // White places K10 (9,9) — captures J10(9,8)+H10(9,7) via G10(9,6)
        // Pattern: K10(W) - J10(B) - H10(B) - G10(W) = X-O-O-X
        let k10 = Pos::new(9, 9);
        state.board.place_stone(k10, Stone::White);
        rules::execute_captures(&mut state.board, k10, Stone::White);

        // Black's five should be broken (J10 removed from the diagonal)
        assert!(
            rules::find_five_positions(&state.board, Stone::Black).is_none(),
            "Black's five should be broken after capture"
        );

        // check_win should return None — game continues
        let result = state.check_win(k10, Stone::White);
        assert!(result.is_none(), "Game should continue after five is broken by capture");
    }
}
