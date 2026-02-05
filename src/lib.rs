//! Gomoku AI Engine with Ninuki-renju rules
//!
//! A high-performance Gomoku AI engine implementing Ninuki-renju variant rules:
//! - Standard 19x19 board
//! - 5-in-a-row to win (overlines allowed)
//! - Capture win: 10 captured stones (5 pairs)
//! - Pair capture rule: X-O-O-X pattern captures the O-O pair
//! - Double-three forbidden for Black
//!
//! # Architecture
//!
//! The engine is organized into several modules:
//! - [`board`]: Board representation with bitboards
//! - [`rules`]: Game rules (capture, win, forbidden moves)
//! - [`eval`]: Position evaluation and heuristics
//! - [`search`]: Search algorithms (alpha-beta, VCF/VCT)
//! - [`engine`]: Main AI engine integrating all components
//!
//! # Quick Start
//!
//! ```
//! use gomoku::{AIEngine, Board, Stone, Pos};
//!
//! // Create a new game with faster config for doc test
//! let mut board = Board::new();
//! let mut engine = AIEngine::with_config(8, 4, 500);
//!
//! // Set up position (faster than empty board)
//! board.place_stone(Pos::new(9, 9), Stone::Black);
//!
//! // AI responds as White
//! if let Some(pos) = engine.get_move(&board, Stone::White) {
//!     board.place_stone(pos, Stone::White);
//!     println!("AI plays at ({}, {})", pos.row, pos.col);
//! }
//! ```
//!
//! # Search Priority
//!
//! The AI engine follows this search priority:
//! 1. Immediate winning move (instant)
//! 2. VCF - Victory by Continuous Fours
//! 3. VCT - Victory by Continuous Threats
//! 4. Defense against opponent's threats
//! 5. Alpha-Beta search with transposition table
//!
//! # Performance
//!
//! The engine is optimized for:
//! - Sub-500ms response time for typical positions
//! - Memory-efficient bitboard representation
//! - Transposition table for avoiding redundant searches
//! - Move ordering for better pruning

pub mod board;
pub mod engine;
pub mod eval;
pub mod rules;
pub mod search;
pub mod ui;

// Re-export commonly used types for convenience
pub use board::{Board, Pos, Stone, BOARD_SIZE};
pub use engine::{AIEngine, MoveResult, SearchType};
