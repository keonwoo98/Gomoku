//! Evaluation module for Gomoku positions
//!
//! This module provides pattern recognition and scoring for board positions.
//! The evaluation considers:
//! - Line patterns (twos, threes, fours, fives)
//! - Capture counts and capture threats
//! - Defensive weighting
//! - Positional bonuses (center control)

pub mod heuristic;
pub mod patterns;

pub use heuristic::evaluate;
pub use patterns::{capture_score, PatternScore};
