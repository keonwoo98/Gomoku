//! Game rules for Gomoku with Ninuki-renju variant
//!
//! This module implements the rule set for Gomoku including:
//! - Capture rules (pair capture)
//! - Win conditions (5-in-a-row, capture win)
//! - Forbidden moves (to be added)

pub mod capture;
pub mod win;

// Re-exports for convenient access
pub use capture::{count_captures, execute_captures, get_captured_positions, has_capture};
pub use win::{can_break_five_by_capture, check_winner, find_five_positions, has_five_in_row};
