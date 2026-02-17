//! GUI module for the Gomoku game
//!
//! This module provides a native Rust GUI using egui/eframe.

mod app;
mod board_view;
mod game_state;
mod theme;

pub use app::GomokuApp;
pub use game_state::{GameMode, GameState, OpeningRule};
