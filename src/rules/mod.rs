//! Game rules for Gomoku with Ninuki-renju variant
//!
//! This module implements the rule set for Gomoku including:
//! - Capture rules (pair capture)
//! - Win conditions (5-in-a-row, capture win)
//! - Forbidden moves (double-three)

pub mod capture;
pub mod forbidden;
pub mod win;

// Re-exports for convenient access
pub use capture::{count_captures, execute_captures, get_captured_positions, has_capture};
pub use forbidden::{count_free_threes, is_double_three, is_valid_move};
pub use win::{can_break_five_by_capture, check_winner, find_five_positions, has_five_in_row};
