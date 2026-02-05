//! Game state management for the Gomoku GUI

use crate::{AIEngine, Board, MoveResult, Pos, Stone};
use std::sync::mpsc::{channel, Receiver, Sender};
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
        receiver: Receiver<MoveResult>,
        start_time: Instant,
    },
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
            ai_depth: 6,
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
        if !crate::rules::is_valid_move(&self.board, pos, self.current_turn) {
            return Err("Invalid move (forbidden or occupied)".to_string());
        }

        // Place the stone
        self.execute_move(pos);
        Ok(())
    }

    /// Execute a move (for both human and AI)
    fn execute_move(&mut self, pos: Pos) {
        let color = self.current_turn;

        // Place stone and handle captures
        self.board.place_stone(pos, color);
        let captured_positions = crate::rules::execute_captures(&mut self.board, pos, color);
        let capture_count = captured_positions.len() / 2; // Each capture is a pair

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
    fn check_win(&self, pos: Pos, color: Stone, captures: usize) -> Option<GameResult> {
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
        let depth = self.ai_depth;
        let time_limit = self.ai_time_limit_ms;

        let (tx, rx) = channel();

        thread::spawn(move || {
            let mut engine = AIEngine::with_config(32, depth, time_limit);
            let result = engine.get_move_with_stats(&board, color);
            let _ = tx.send(result);
        });

        self.ai_state = AiState::Thinking {
            receiver: rx,
            start_time: Instant::now(),
        };
    }

    /// Check if AI has finished thinking
    pub fn check_ai_result(&mut self) {
        let result = match &self.ai_state {
            AiState::Thinking { receiver, start_time } => {
                match receiver.try_recv() {
                    Ok(result) => Some((result, start_time.elapsed())),
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

        if let Some((move_result, elapsed)) = result {
            self.ai_state = AiState::Idle;
            self.last_ai_result = Some(move_result.clone());
            self.move_timer.set_ai_time(elapsed);

            if let Some(pos) = move_result.best_move {
                self.execute_move(pos);
            } else {
                self.message = Some("AI could not find a move".to_string());
            }
        }
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

        // For PvE, undo two moves (human + AI)
        let undo_count = match self.mode {
            GameMode::PvE { .. } if self.move_history.len() >= 2 => 2,
            _ => 1,
        };

        // Simple undo: reset and replay
        let moves_to_keep = self.move_history.len().saturating_sub(undo_count);
        let moves: Vec<_> = self.move_history.drain(..moves_to_keep).collect();

        self.board = Board::new();
        self.current_turn = Stone::Black;
        self.game_over = None;
        self.last_move = None;
        self.suggested_move = None;

        for (pos, color) in moves {
            self.board.place_stone(pos, color);
            crate::rules::execute_captures(&mut self.board, pos, color);
            self.move_history.push((pos, color));
            self.last_move = Some(pos);
            self.current_turn = color.opponent();
        }

        self.move_timer.start();
    }
}
