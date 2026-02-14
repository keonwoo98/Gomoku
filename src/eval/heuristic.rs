//! Heuristic evaluation function for Gomoku board positions
//!
//! This module provides the core evaluation function for the minimax search.
//! It evaluates board positions based on:
//! - Win/loss detection
//! - Pattern scoring (fives, fours, threes, twos)
//! - Capture advantage
//! - Positional bonuses (center control)

use crate::board::{Board, Pos, Stone, BOARD_SIZE};

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

/// Weight per distance unit from center.
/// Higher weight prevents scattered stone placement (O6, F12 type moves).
/// At weight 8: center stone gets 144pts, corner gets 0 — significant vs CLOSED_TWO (50).
const POSITION_WEIGHT: i32 = 8;

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

    // Quick capture-win check (O(1) - just reads stored count).
    // Alpha-beta already checks five-in-a-row via has_five_at_pos() at each node,
    // so by the time evaluate() is called at leaf nodes, no five-in-a-row exists.
    // We only need to check capture wins here.
    if board.captures(color) >= 5 {
        return PatternScore::FIVE;
    }
    if board.captures(opponent) >= 5 {
        return -PatternScore::FIVE;
    }

    let cap_score = capture_score(board.captures(color), board.captures(opponent));

    // Single-pass evaluation per color: patterns + position + vulnerability combined.
    // SYMMETRIC for negamax: evaluate(board, Black) == -evaluate(board, White).
    let (my_score, my_vuln) = evaluate_color(board, color);
    let (opp_score, opp_vuln) = evaluate_color(board, opponent);

    let my_caps = board.captures(color);
    let opp_caps = board.captures(opponent);
    let vuln_penalty = my_vuln * vuln_weight(opp_caps) - opp_vuln * vuln_weight(my_caps);

    cap_score + (my_score - opp_score) - vuln_penalty
}

/// Returns vulnerability penalty weight scaled by opponent's capture count.
/// Higher captures = much higher penalty per vulnerable pair (exponential danger).
///
/// A capturable pair gives the opponent a free capture opportunity — destroying
/// our pattern AND advancing toward capture-win. The penalty must be comparable
/// to pattern scores to prevent the AI from building "strong but fragile" positions.
///
/// At 0-1 caps: 10K = OPEN_THREE level — creating a capturable pair is as bad
///   as giving the opponent an open three (they gain a strong tactical option).
/// At 4+ caps: 80K = near OPEN_FOUR — one more capture wins, so any vulnerability
///   is near-lethal.
fn vuln_weight(opp_captures: u8) -> i32 {
    match opp_captures {
        0..=1 => 10_000,  // was 4K — vulnerability matters even early game
        2 => 20_000,      // was 10K — two captures means opponent is actively hunting
        3 => 40_000,      // was 25K — three captures = serious strategic threat
        _ => 80_000,      // was 60K — four captures = one more capture = instant loss
    }
}

/// Single-pass evaluation for one color.
///
/// Combines pattern scoring, position bonus, and capture vulnerability
/// into a single iteration over the color's stones. This is ~3x faster
/// than the previous 3-function approach (evaluate_patterns + evaluate_positions
/// + count_vulnerable_pairs) which each iterated all stones separately.
///
/// Returns (total_score, vulnerable_pair_count).
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn evaluate_color(board: &Board, color: Stone) -> (i32, i32) {
    let Some(stones) = board.stones(color) else {
        return (0, 0);
    };

    let opponent = color.opponent();
    let center = (BOARD_SIZE / 2) as i32;

    let mut score = 0;
    let mut open_fours = 0i32;
    let mut closed_fours = 0i32;
    let mut open_threes = 0i32;
    let mut vuln = 0i32;
    let mut open_twos = 0i32;

    for pos in stones.iter_ones() {
        // --- Pattern scoring (4 directions) ---
        for &(dr, dc) in &DIRECTIONS {
            let pattern_score = evaluate_line(board, pos, dr, dc, color);
            score += pattern_score;

            if pattern_score >= PatternScore::OPEN_FOUR {
                open_fours += 1;
            } else if pattern_score >= PatternScore::CLOSED_FOUR {
                closed_fours += 1;
            } else if pattern_score >= PatternScore::OPEN_THREE {
                open_threes += 1;
            } else if pattern_score >= PatternScore::OPEN_TWO
                && pattern_score < PatternScore::CLOSED_THREE
            {
                open_twos += 1;
            }
        }

        // --- Position bonus (center control) ---
        let dist = (i32::from(pos.row) - center).abs() + (i32::from(pos.col) - center).abs();
        score += (MAX_CENTER_DIST - dist) * POSITION_WEIGHT;

        // --- Connectivity bonus: reward stones near other friendly stones ---
        // This incentivizes clustered, connected play over scattered placement.
        // Each bond is counted from both sides of the pair (2×80=160 per adjacent pair).
        // Magnitude is small vs patterns (OPEN_TWO=1000) but provides meaningful
        // tiebreaker that prevents isolated stone placement.
        for &(dr, dc) in &DIRECTIONS {
            for sign in [1i32, -1i32] {
                let nr = i32::from(pos.row) + dr * sign;
                let nc = i32::from(pos.col) + dc * sign;
                if Pos::is_valid(nr, nc) && board.get(Pos::new(nr as u8, nc as u8)) == color {
                    score += 80;
                }
            }
        }

        // --- Vulnerability: ally-ally pair capturable by opponent ---
        for &(dr, dc) in &DIRECTIONS {
            let r1 = i32::from(pos.row) + dr;
            let c1 = i32::from(pos.col) + dc;
            if !Pos::is_valid(r1, c1) { continue; }
            let p1 = Pos::new(r1 as u8, c1 as u8);
            if board.get(p1) != color { continue; }

            let rb = i32::from(pos.row) - dr;
            let cb = i32::from(pos.col) - dc;
            let ra = r1 + dr;
            let ca = c1 + dc;

            let before = if Pos::is_valid(rb, cb) {
                board.get(Pos::new(rb as u8, cb as u8))
            } else {
                Stone::Empty
            };
            let after = if Pos::is_valid(ra, ca) {
                board.get(Pos::new(ra as u8, ca as u8))
            } else {
                Stone::Empty
            };

            // empty-ally-ally-opp: opponent plays at empty to capture
            if before == Stone::Empty && after == opponent && Pos::is_valid(rb, cb) {
                vuln += 1;
            }
            // opp-ally-ally-empty: opponent plays at empty to capture
            if before == opponent && after == Stone::Empty && Pos::is_valid(ra, ca) {
                vuln += 1;
            }
        }
    }

    // Multiple threat combination bonuses
    // These are CRITICAL: multi-direction threats are often unblockable.
    if open_fours >= 1 && (closed_fours >= 1 || open_threes >= 1) {
        score += PatternScore::OPEN_FOUR;
    }
    if closed_fours >= 2 {
        score += PatternScore::OPEN_FOUR;
    }
    if closed_fours >= 1 && open_threes >= 1 {
        score += PatternScore::OPEN_FOUR;
    }
    // Double open three: opponent can only block one → the other becomes open four → win.
    // Equivalent to open four in practice — must be scored at OPEN_FOUR level.
    if open_threes >= 2 {
        score += PatternScore::OPEN_FOUR; // 100K — virtually unblockable
    }

    // Multi-directional development bonus (open twos)
    // Multiple directions developing simultaneously are hard to block all at once
    if open_twos >= 4 {
        score += 8_000;
    } else if open_twos >= 3 {
        score += 5_000;
    } else if open_twos >= 2 {
        score += 3_000;
    }

    (score, vuln)
}

/// Evaluate a single line pattern from a position in a given direction.
///
/// Only counts the pattern if this position is the "start" of the line
/// (no same-color stone in the negative direction). This ensures each
/// line segment is counted exactly once, avoiding double-counting.
///
/// Counts consecutive stones and open ends to determine the pattern type.
/// Also detects one-gap patterns like `O_OOO` or `OO_OO` where filling
/// the gap completes five-in-a-row.
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

    // Count consecutive stones and detect one gap
    let mut count = 1; // Start with the stone at pos
    let mut open_ends = 0;
    let mut has_gap = false;
    let mut total_span = 1; // Total positions used (stones + gap)

    // Check if there's an open end before our starting position
    if Pos::is_valid(prev_r, prev_c) {
        let prev_pos = Pos::new(prev_r as u8, prev_c as u8);
        if board.get(prev_pos) == Stone::Empty {
            open_ends += 1;
        }
    }

    // Extend in positive direction, allowing one gap
    let mut r = i32::from(pos.row) + dr;
    let mut c = i32::from(pos.col) + dc;
    while Pos::is_valid(r, c) {
        let p = Pos::new(r as u8, c as u8);
        match board.get(p) {
            s if s == color => {
                count += 1;
                total_span += 1;
            }
            Stone::Empty if !has_gap => {
                // Check if there's a same-color stone after this empty cell
                let next_r = r + dr;
                let next_c = c + dc;
                if Pos::is_valid(next_r, next_c)
                    && board.get(Pos::new(next_r as u8, next_c as u8)) == color
                {
                    // Found a gap with a stone after it - continue scanning
                    has_gap = true;
                    total_span += 1; // Count the gap in span
                    r += dr;
                    c += dc;
                    continue;
                }
                // No stone after gap - this is an open end
                open_ends += 1;
                break;
            }
            Stone::Empty => {
                // Second empty cell (gap already used) - open end on positive side
                open_ends += 1;
                break;
            }
            _ => break, // Opponent stone blocks
        }
        r += dr;
        c += dc;
    }

    // Score based on pattern type
    // Gap patterns: count stones (not gap), but span determines if filling gap completes 5
    // Important: gap patterns are NEVER actual five-in-a-row (that requires consecutive stones).
    // Filling the gap is always one move away, so the best a gap pattern can be is OPEN_FOUR.
    if has_gap {
        match count {
            5.. => PatternScore::OPEN_FOUR, // 5+ stones with gap: filling gap wins (unstoppable)
            4 if total_span == 5 => PatternScore::OPEN_FOUR, // OO_OO or O_OOO in exactly 5 span
            4 => PatternScore::CLOSED_FOUR, // 4 with gap but wider span
            3 if open_ends == 2 => PatternScore::OPEN_THREE, // _O_OO_ or _OO_O_: filling gap → open four
            3 if open_ends == 1 => PatternScore::CLOSED_THREE, // XO_OO_ : filling gap → closed four
            _ => 0,
        }
    } else {
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

        // Note: evaluate() no longer checks five-in-a-row (alpha-beta does that).
        // But the pattern scoring should still produce a very high positive score.
        let score = evaluate(&board, Stone::Black);
        assert!(score >= PatternScore::FIVE, "Five in a row should produce very high score, got {}", score);
    }

    #[test]
    fn test_evaluate_losing_position() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }

        // Note: evaluate() no longer checks five-in-a-row (alpha-beta does that).
        // But the pattern scoring should still produce a very negative score.
        let score = evaluate(&board, Stone::Black);
        assert!(score <= -PatternScore::FIVE, "Opponent five should produce very low score, got {}", score);
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
    fn test_evaluate_negamax_symmetry() {
        // Negamax REQUIRES: evaluate(board, Black) == -evaluate(board, White)
        let mut board = Board::new();

        // Create a non-trivial position
        board.place_stone(Pos::new(9, 7), Stone::Black);
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::Black); // Open three for Black

        board.place_stone(Pos::new(5, 5), Stone::White);
        board.place_stone(Pos::new(5, 6), Stone::White); // Open two for White

        let black_score = evaluate(&board, Stone::Black);
        let white_score = evaluate(&board, Stone::White);

        assert_eq!(
            black_score, -white_score,
            "Negamax symmetry violated: eval(Black)={}, eval(White)={}, -eval(White)={}",
            black_score, white_score, -white_score
        );
    }

    #[test]
    fn test_evaluate_perspective_correct() {
        // Verify that evaluation correctly identifies advantage
        let mut board1 = Board::new();
        let mut board2 = Board::new();

        // Board1: Black has open three
        for i in 1..4 {
            board1.place_stone(Pos::new(9, i), Stone::Black);
        }

        // Board2: White has open three
        for i in 1..4 {
            board2.place_stone(Pos::new(9, i), Stone::White);
        }

        let our_advantage = evaluate(&board1, Stone::Black);
        let their_advantage = evaluate(&board2, Stone::Black);

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
