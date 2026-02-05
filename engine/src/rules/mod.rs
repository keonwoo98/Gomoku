//! Game rules for Gomoku with Ninuki-renju variant
//!
//! This module implements the rule set for Gomoku including:
//! - Capture rules (pair capture)
//! - Win conditions (to be added)
//! - Forbidden moves (to be added)

pub mod capture;

// Re-exports for convenient access
pub use capture::{count_captures, execute_captures, get_captured_positions, has_capture};
