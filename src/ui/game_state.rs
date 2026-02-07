//! Game state management for the Gomoku GUI

use crate::{AIEngine, Board, MoveResult, Pos, Stone, ai_log, pos_to_notation, rules};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

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

    pub fn avg_time_ms(&self) -> f64 {
        if self.move_count == 0 { 0.0 } else { self.total_time_ms as f64 / self.move_count as f64 }
    }

    pub fn avg_depth(&self) -> f64 {
        if self.move_depths.is_empty() { 0.0 } else { self.move_depths.iter().map(|&d| d as f64).sum::<f64>() / self.move_depths.len() as f64 }
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
    pub last_ai_result: Option<MoveResult>,
    pub ai_state: AiState,
    pub move_timer: MoveTimer,
    pub suggested_move: Option<Pos>,
    pub message: Option<String>,
    pub capture_animation: Option<CaptureAnimation>,
    pub ai_stats: AiStats,
    /// Review mode: when Some(index), shows board at move #index
    pub review_index: Option<usize>,
    /// Redo stack: each entry is a group of moves (1 for PvP, 2 for PvE)
    pub redo_groups: Vec<Vec<(Pos, Stone)>>,

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
        Self {
            board: Board::new(),
            mode,
            current_turn: Stone::Black,
            game_over: None,
            last_move: None,
            move_history: Vec::new(),
            last_ai_result: None,
            ai_state: AiState::Idle,
            move_timer: MoveTimer::default(),
            suggested_move: None,
            message: None,
            capture_animation: None,
            ai_stats: AiStats::default(),
            review_index: None,
            redo_groups: Vec::new(),
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
        self.last_ai_result = None;
        self.ai_state = AiState::Idle;
        self.move_timer = MoveTimer::default();
        self.suggested_move = None;
        self.message = None;
        self.capture_animation = None;
        self.ai_stats = AiStats::default();
        self.review_index = None;
        self.redo_groups.clear();
        if let Some(ref mut engine) = self.ai_engine {
            engine.clear_cache();
        }
    }

    /// Check if it's the human's turn
    pub fn is_human_turn(&self) -> bool {
        match self.mode {
            GameMode::PvE { human_color } => self.current_turn == human_color,
            GameMode::PvP { .. } => true,
        }
    }

    /// Check if it's the AI's turn
    pub fn is_ai_turn(&self) -> bool {
        match self.mode {
            GameMode::PvE { human_color } => self.current_turn != human_color,
            GameMode::PvP { .. } => false,
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

        // Log human moves for game reconstruction
        if is_human {
            let color_str = if color == Stone::Black { "Black" } else { "White" };
            let cap_str = if capture_count > 0 {
                format!(" +{}cap [{}]", capture_count,
                    captured_positions.iter().map(|p| pos_to_notation(*p)).collect::<Vec<_>>().join(", "))
            } else {
                String::new()
            };
            ai_log(&format!("  >> Human #{}: {} plays {}{}",
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

        // Stop timer
        self.move_timer.stop();

        // Check for win
        if let Some(result) = self.check_win(pos, color, capture_count) {
            self.game_over = Some(result);
            return;
        }

        // Switch turn
        self.current_turn = color.opponent();
        self.move_timer.start();

        // Clear message
        self.message = None;
    }

    /// Check for win condition
    fn check_win(&self, pos: Pos, color: Stone, _captures: usize) -> Option<GameResult> {
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

        // Check five-in-a-row
        if let Some(line) = self.find_winning_line(pos, color) {
            return Some(GameResult {
                winner: color,
                win_type: WinType::FiveInRow,
                winning_line: Some(line),
            });
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
        // First, check if AI has timed out (5 seconds)
        let should_force_move = match &self.ai_state {
            AiState::Thinking { start_time, .. } => {
                start_time.elapsed() > Duration::from_secs(5)
            }
            AiState::Idle => false,
        };

        // If timed out, force a quick fallback move
        if should_force_move {
            // Engine is lost in the thread; create a fresh one
            if self.ai_engine.is_none() {
                self.ai_engine = Some(AIEngine::with_config(64, self.ai_depth, self.ai_time_limit_ms));
            }
            self.ai_state = AiState::Idle;
            self.message = Some("AI timeout - quick move".to_string());

            // Find any valid move quickly
            if let Some(fallback) = self.find_fallback_move() {
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
            AiState::Idle => None,
        };

        if let Some((move_result, engine, elapsed)) = result {
            self.ai_state = AiState::Idle;
            self.ai_engine = Some(engine); // Return engine for reuse
            self.ai_stats.record(&move_result);
            self.last_ai_result = Some(move_result.clone());
            self.move_timer.set_ai_time(elapsed);

            if let Some(pos) = move_result.best_move {
                self.execute_move(pos);
            } else {
                self.message = Some("AI could not find a move".to_string());
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
            AiState::Idle => None,
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
        self.last_ai_result = Some(result);
    }

    /// Undo last move
    pub fn undo(&mut self) {
        if self.move_history.is_empty() || self.is_ai_thinking() {
            return;
        }

        // Exit review mode if active
        self.review_index = None;

        // For PvE, undo two moves (human + AI)
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
