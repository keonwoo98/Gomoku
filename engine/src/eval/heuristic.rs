//! Heuristic evaluation function for Gomoku board positions
//!
//! This module provides the core evaluation function for the minimax search.
//! It evaluates board positions based on:
//! - Win/loss detection
//! - Pattern scoring (fives, fours, threes, twos)
//! - Capture advantage
//! - Positional bonuses (center control)

use crate::board::{Board, Pos, Stone, BOARD_SIZE};
use crate::rules::win::check_winner;

use super::patterns::{capture_score, PatternScore};

/// Direction vectors for line checking (4 directions)
/// Each direction only needs to be checked once (we scan both ways from each stone)
const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1),  // Horizontal
    (1, 0),  // Vertical
    (1, 1),  // Diagonal SE
    (1, -1), // Diagonal SW
];

/// Maximum Manhattan distance from center on 19x19 board
const MAX_CENTER_DIST: i32 = 18;

/// Weight per distance unit from center
const POSITION_WEIGHT: i32 = 3;

/// Evaluate the board from the perspective of the given color.
///
/// Returns a score where:
/// - Positive values indicate advantage for `color`
/// - Negative values indicate disadvantage for `color`
/// - `PatternScore::FIVE` indicates immediate win
/// - `-PatternScore::FIVE` indicates immediate loss
///
/// # Arguments
/// * `board` - The current board state
/// * `color` - The color to evaluate for
///
/// # Returns
/// An i32 score representing the position evaluation
#[must_use]
pub fn evaluate(board: &Board, color: Stone) -> i32 {
    let opponent = color.opponent();

    // Check for immediate win/loss first (most important)
    if let Some(winner) = check_winner(board) {
        return if winner == color {
            PatternScore::FIVE
        } else {
            -PatternScore::FIVE
        };
    }

    // Capture score (non-linear, defense weighted)
    let cap_score = capture_score(board.captures(color), board.captures(opponent));

    // Pattern-based evaluation
    let my_patterns = evaluate_patterns(board, color);
    let opp_patterns = evaluate_patterns(board, opponent);

    // Defense is weighted higher - opponent threats hurt more than our own threats help
    // Use integer math to avoid floating point: multiply by 3, divide by 2 (1.5x)
    let pattern_score = my_patterns - (opp_patterns * 3 / 2);

    // Position score (center control bonus)
    let position_score = evaluate_positions(board, color) - evaluate_positions(board, opponent);

    cap_score + pattern_score + position_score
}

/// Evaluate pattern-based score for a color.
///
/// Scans all stones of the given color and evaluates line patterns
/// in all four directions. Each line segment is counted exactly once
/// by only evaluating from the "start" position (no same-color stone
/// in the negative direction).
fn evaluate_patterns(board: &Board, color: Stone) -> i32 {
    let Some(stones) = board.stones(color) else {
        return 0;
    };

    let mut score = 0;

    for pos in stones.iter_ones() {
        for &(dr, dc) in &DIRECTIONS {
            score += evaluate_line(board, pos, dr, dc, color);
        }
    }

    score
}

/// Evaluate a single line pattern from a position in a given direction.
///
/// Only counts the pattern if this position is the "start" of the line
/// (no same-color stone in the negative direction). This ensures each
/// line segment is counted exactly once, avoiding double-counting.
///
/// Counts consecutive stones and open ends to determine the pattern type.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn evaluate_line(board: &Board, pos: Pos, dr: i32, dc: i32, color: Stone) -> i32 {
    // Check if there's a same-color stone in the negative direction
    // If so, this position is NOT the start of the line - skip to avoid double counting
    let prev_r = i32::from(pos.row) - dr;
    let prev_c = i32::from(pos.col) - dc;
    if Pos::is_valid(prev_r, prev_c) {
        let prev_pos = Pos::new(prev_r as u8, prev_c as u8);
        if board.get(prev_pos) == color {
            return 0; // Not the start of this line segment
        }
    }

    // Count in positive direction only (since we're starting from the beginning)
    let mut count = 1; // Start with the stone at pos
    let mut open_ends = 0;

    // Check if there's an open end before our starting position
    if Pos::is_valid(prev_r, prev_c) {
        let prev_pos = Pos::new(prev_r as u8, prev_c as u8);
        if board.get(prev_pos) == Stone::Empty {
            open_ends += 1;
        }
        // If blocked by opponent or edge, open_ends stays 0 for this side
    }
    // Edge of board - not open (open_ends stays 0)

    // Extend in positive direction
    let mut r = i32::from(pos.row) + dr;
    let mut c = i32::from(pos.col) + dc;
    while Pos::is_valid(r, c) {
        // Safety: r and c are validated by is_valid to be in [0, BOARD_SIZE)
        let p = Pos::new(r as u8, c as u8);
        match board.get(p) {
            s if s == color => count += 1,
            Stone::Empty => {
                open_ends += 1;
                break;
            }
            _ => break, // Opponent stone blocks
        }
        r += dr;
        c += dc;
    }

    // Score based on pattern type
    match (count, open_ends) {
        (5.., _) => PatternScore::FIVE,
        (4, 2) => PatternScore::OPEN_FOUR,
        (4, 1) => PatternScore::CLOSED_FOUR,
        (3, 2) => PatternScore::OPEN_THREE,
        (3, 1) => PatternScore::CLOSED_THREE,
        (2, 2) => PatternScore::OPEN_TWO,
        (2, 1) => PatternScore::CLOSED_TWO,
        _ => 0,
    }
}

/// Evaluate positional bonuses for a color.
///
/// Stones closer to the center are worth more as they have more
/// potential for creating patterns in multiple directions.
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
fn evaluate_positions(board: &Board, color: Stone) -> i32 {
    let Some(stones) = board.stones(color) else {
        return 0;
    };

    // Center is at (9, 9) for a 19x19 board
    let center = (BOARD_SIZE / 2) as i32;
    let mut score = 0;

    for pos in stones.iter_ones() {
        // Manhattan distance from center
        let dist = (i32::from(pos.row) - center).abs() + (i32::from(pos.col) - center).abs();
        // Max distance is MAX_CENTER_DIST (corner to center)
        // Max bonus is MAX_CENTER_DIST * POSITION_WEIGHT per stone
        score += (MAX_CENTER_DIST - dist) * POSITION_WEIGHT;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_empty_board() {
        let board = Board::new();
        let score = evaluate(&board, Stone::Black);
        assert_eq!(score, 0, "Empty board should have score 0");
    }

    #[test]
    fn test_evaluate_center_bonus() {
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let score = evaluate(&board, Stone::Black);
        assert!(score > 0, "Center position should be valuable, got {}", score);
    }

    #[test]
    fn test_evaluate_corner_less_valuable() {
        let mut board_center = Board::new();
        board_center.place_stone(Pos::new(9, 9), Stone::Black);

        let mut board_corner = Board::new();
        board_corner.place_stone(Pos::new(0, 0), Stone::Black);

        let center_score = evaluate(&board_center, Stone::Black);
        let corner_score = evaluate(&board_corner, Stone::Black);

        assert!(
            center_score > corner_score,
            "Center ({}) should be more valuable than corner ({})",
            center_score,
            corner_score
        );
    }

    #[test]
    fn test_evaluate_winning_position() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let score = evaluate(&board, Stone::Black);
        assert_eq!(score, PatternScore::FIVE, "Five in a row should be winning score");
    }

    #[test]
    fn test_evaluate_losing_position() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }

        let score = evaluate(&board, Stone::Black);
        assert_eq!(score, -PatternScore::FIVE, "Opponent five should be losing score");
    }

    #[test]
    fn test_evaluate_capture_win() {
        let mut board = Board::new();
        board.add_captures(Stone::Black, 5);

        let score = evaluate(&board, Stone::Black);
        assert_eq!(score, PatternScore::FIVE, "Capture win should be winning score");
    }

    #[test]
    fn test_evaluate_capture_loss() {
        let mut board = Board::new();
        board.add_captures(Stone::White, 5);

        let score = evaluate(&board, Stone::Black);
        assert_eq!(score, -PatternScore::FIVE, "Opponent capture win should be losing");
    }

    #[test]
    fn test_evaluate_open_four() {
        let mut board = Board::new();
        // _OOOO_ pattern: stones at cols 1-4, empty at 0 and 5
        for i in 1..5 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let score = evaluate(&board, Stone::Black);
        assert!(score > 0, "Open four should have positive score, got {}", score);
        assert!(
            score < PatternScore::FIVE,
            "Open four should be less than win"
        );
    }

    #[test]
    fn test_evaluate_closed_four() {
        let mut board = Board::new();
        // XOOOO_ pattern: white at col 0, blacks at 1-4, empty at 5
        board.place_stone(Pos::new(9, 0), Stone::White);
        for i in 1..5 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let score = evaluate(&board, Stone::Black);
        assert!(score > 0, "Closed four should have positive score");
    }

    #[test]
    fn test_evaluate_open_three() {
        let mut board = Board::new();
        // _OOO_ pattern: stones at cols 1-3, empty at 0 and 4
        for i in 1..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let score = evaluate(&board, Stone::Black);
        assert!(score > 0, "Open three should have positive score");
    }

    #[test]
    fn test_evaluate_defense_weighted() {
        // Opponent's patterns should hurt more than our own help
        let mut board1 = Board::new();
        let mut board2 = Board::new();

        // Board1: We have open three
        for i in 1..4 {
            board1.place_stone(Pos::new(9, i), Stone::Black);
        }

        // Board2: Opponent has open three
        for i in 1..4 {
            board2.place_stone(Pos::new(9, i), Stone::White);
        }

        let our_advantage = evaluate(&board1, Stone::Black);
        let their_advantage = evaluate(&board2, Stone::Black);

        // Our advantage should be positive, their advantage negative
        assert!(
            our_advantage > 0,
            "Our pattern should give positive score, got {}",
            our_advantage
        );
        assert!(
            their_advantage < 0,
            "Their pattern should give negative score, got {}",
            their_advantage
        );
        // Defense is weighted 1.5x, so their threat should hurt more
        assert!(
            our_advantage < -their_advantage,
            "Defense should be weighted higher: our={}, theirs={}",
            our_advantage,
            their_advantage
        );
    }

    #[test]
    fn test_evaluate_multiple_patterns() {
        let mut board = Board::new();

        // Create two separate open twos for black
        board.place_stone(Pos::new(5, 5), Stone::Black);
        board.place_stone(Pos::new(5, 6), Stone::Black);

        board.place_stone(Pos::new(10, 10), Stone::Black);
        board.place_stone(Pos::new(10, 11), Stone::Black);

        let score = evaluate(&board, Stone::Black);
        assert!(
            score > 0,
            "Multiple patterns should give positive score, got {}",
            score
        );
    }

    #[test]
    fn test_evaluate_diagonal_pattern() {
        let mut board = Board::new();
        // Diagonal three
        for i in 0..3 {
            board.place_stone(Pos::new(5 + i, 5 + i), Stone::Black);
        }

        let score = evaluate(&board, Stone::Black);
        assert!(
            score > 0,
            "Diagonal pattern should be detected and scored positively"
        );
    }

    #[test]
    fn test_evaluate_symmetry() {
        // Same position should give same score regardless of perspective direction
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 9), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::White);

        let black_score = evaluate(&board, Stone::Black);
        let white_score = evaluate(&board, Stone::White);

        // Scores should be roughly opposite (not exactly due to position bonuses)
        // But the sign should be opposite if one has advantage
        assert!(
            (black_score > 0) != (white_score > 0) || (black_score == 0 && white_score == 0),
            "Scores should reflect opposite perspectives: black={}, white={}",
            black_score,
            white_score
        );
    }

    #[test]
    fn test_evaluate_captures_matter() {
        let mut board1 = Board::new();
        let mut board2 = Board::new();

        // Board1: No captures
        board1.place_stone(Pos::new(9, 9), Stone::Black);

        // Board2: Same stone + 2 captures
        board2.place_stone(Pos::new(9, 9), Stone::Black);
        board2.add_captures(Stone::Black, 2);

        let score1 = evaluate(&board1, Stone::Black);
        let score2 = evaluate(&board2, Stone::Black);

        assert!(
            score2 > score1,
            "Captures should increase score: without={}, with={}",
            score1,
            score2
        );
    }

    #[test]
    fn test_evaluate_near_capture_win() {
        let mut board = Board::new();
        board.add_captures(Stone::Black, 4);

        let score = evaluate(&board, Stone::Black);
        assert!(
            score >= PatternScore::NEAR_CAPTURE_WIN,
            "4 captures should be highly valuable, got {}",
            score
        );
    }
}
