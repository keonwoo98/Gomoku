//! Double-three forbidden move rules for Gomoku
//!
//! A double-three is a move that creates two or more free-threes simultaneously.
//! Free-three: 3 stones in a row with both ends open, that can become an
//! unstoppable open-four if not blocked.
//!
//! Exception: Double-three via capture IS allowed.

use crate::board::{Board, Pos, Stone};

use super::capture::has_capture;
#[cfg(test)]
use super::capture::get_captured_positions;

/// Direction vectors for pattern checking (4 directions)
const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1),  // Horizontal
    (1, 0),  // Vertical
    (1, 1),  // Diagonal SE
    (1, -1), // Diagonal SW
];

/// Pattern information for a line segment
#[derive(Debug, Clone)]
struct LinePattern {
    /// Stones of the player in this line (relative positions from center)
    stones: Vec<i32>,
    /// Number of open ends (0, 1, or 2)
    open_ends: u8,
    /// Total span of the pattern
    span: u8,
}

/// Scan a line from the given position in both directions
/// Returns the pattern of stones and open ends
///
/// The scan allows one gap (empty cell) within the pattern to detect
/// patterns like `_OO_O_` (free-three with gap)
fn scan_line(board: &Board, pos: Pos, stone: Stone, dr: i32, dc: i32) -> LinePattern {
    let opponent = stone.opponent();
    let mut stones = vec![0i32]; // The placed stone at position 0
    let mut open_ends = 0u8;

    // Scan positive direction - collect stones and track open end
    let mut found_open_end_pos = false;
    let mut gap_pos: Option<i32> = None;

    for i in 1..=5 {
        let r = pos.row as i32 + dr * i;
        let c = pos.col as i32 + dc * i;

        if !Pos::is_valid(r, c) {
            // Hit boundary - not an open end
            break;
        }

        let check_pos = Pos::new(r as u8, c as u8);
        let cell = board.get(check_pos);

        if cell == stone {
            stones.push(i);
        } else if cell == opponent {
            // Blocked by opponent
            break;
        } else {
            // Empty cell
            if gap_pos.is_none() {
                // Check if there's a stone after this gap
                let next_r = pos.row as i32 + dr * (i + 1);
                let next_c = pos.col as i32 + dc * (i + 1);
                if Pos::is_valid(next_r, next_c) {
                    let next_pos = Pos::new(next_r as u8, next_c as u8);
                    if board.get(next_pos) == stone {
                        // There's a stone after this gap - this is part of pattern
                        gap_pos = Some(i);
                        continue;
                    }
                }
            }
            // This empty is an open end
            found_open_end_pos = true;
            break;
        }
    }
    if found_open_end_pos {
        open_ends += 1;
    }

    // Scan negative direction - collect stones and track open end
    let mut found_open_end_neg = false;
    let mut gap_neg: Option<i32> = None;

    for i in 1..=5 {
        let r = pos.row as i32 - dr * i;
        let c = pos.col as i32 - dc * i;

        if !Pos::is_valid(r, c) {
            // Hit boundary - not an open end
            break;
        }

        let check_pos = Pos::new(r as u8, c as u8);
        let cell = board.get(check_pos);

        if cell == stone {
            stones.push(-i);
        } else if cell == opponent {
            // Blocked by opponent
            break;
        } else {
            // Empty cell
            if gap_neg.is_none() {
                // Check if there's a stone after this gap
                let next_r = pos.row as i32 - dr * (i + 1);
                let next_c = pos.col as i32 - dc * (i + 1);
                if Pos::is_valid(next_r, next_c) {
                    let next_pos = Pos::new(next_r as u8, next_c as u8);
                    if board.get(next_pos) == stone {
                        // There's a stone after this gap - this is part of pattern
                        gap_neg = Some(-i);
                        continue;
                    }
                }
            }
            // This empty is an open end
            found_open_end_neg = true;
            break;
        }
    }
    if found_open_end_neg {
        open_ends += 1;
    }

    stones.sort();
    let span = if stones.is_empty() {
        0
    } else {
        (stones[stones.len() - 1] - stones[0] + 1) as u8
    };

    LinePattern {
        stones,
        open_ends,
        span,
    }
}

/// Check if a pattern forms a free-three
/// Free-three patterns:
/// 1. `_OOO_` - consecutive 3 with both ends open
/// 2. `_OO_O_` or `_O_OO_` - 3 with one gap, both ends open
///
/// Key: exactly 3 stones, both ends open, and when the gap is filled,
/// it becomes an open-four that cannot be blocked.
fn is_free_three(pattern: &LinePattern) -> bool {
    // Must have exactly 3 stones
    if pattern.stones.len() != 3 {
        return false;
    }

    // Must have both ends open
    if pattern.open_ends < 2 {
        return false;
    }

    // Check the span - for a free three:
    // - Consecutive `OOO`: span = 3
    // - One gap `OO_O` or `O_OO`: span = 4
    // Span > 4 means too spread out to form open-four
    if pattern.span > 4 {
        return false;
    }

    // For span = 4 (with gap), verify the pattern can become open-four
    // Pattern must be: stones at positions like [-1, 0, 2] or [-2, 0, 1]
    // where filling the gap creates `_OOOO_`
    if pattern.span == 4 {
        // Find the gap position
        let min = pattern.stones[0];
        let max = pattern.stones[2];

        // The three stones should have exactly one gap of size 1
        let has_single_gap = (max - min == 3)
            && ((pattern.stones[1] - pattern.stones[0] == 1
                && pattern.stones[2] - pattern.stones[1] == 2)
                || (pattern.stones[1] - pattern.stones[0] == 2
                    && pattern.stones[2] - pattern.stones[1] == 1));

        return has_single_gap;
    }

    // span = 3: consecutive three `OOO`
    true
}

/// Scan a line from the given position without allowing any gaps.
/// Only collects consecutive friendly stones in each direction.
fn scan_line_consecutive(board: &Board, pos: Pos, stone: Stone, dr: i32, dc: i32) -> LinePattern {
    let opponent = stone.opponent();
    let mut stones = vec![0i32]; // The placed stone at position 0
    let mut open_ends = 0u8;

    // Scan positive direction - consecutive only
    let mut found_open_end_pos = false;
    for i in 1..=5 {
        let r = pos.row as i32 + dr * i;
        let c = pos.col as i32 + dc * i;
        if !Pos::is_valid(r, c) {
            break;
        }
        let check_pos = Pos::new(r as u8, c as u8);
        let cell = board.get(check_pos);
        if cell == stone {
            stones.push(i);
        } else if cell == opponent {
            break;
        } else {
            found_open_end_pos = true;
            break;
        }
    }
    if found_open_end_pos {
        open_ends += 1;
    }

    // Scan negative direction - consecutive only
    let mut found_open_end_neg = false;
    for i in 1..=5 {
        let r = pos.row as i32 - dr * i;
        let c = pos.col as i32 - dc * i;
        if !Pos::is_valid(r, c) {
            break;
        }
        let check_pos = Pos::new(r as u8, c as u8);
        let cell = board.get(check_pos);
        if cell == stone {
            stones.push(-i);
        } else if cell == opponent {
            break;
        } else {
            found_open_end_neg = true;
            break;
        }
    }
    if found_open_end_neg {
        open_ends += 1;
    }

    stones.sort();
    let span = if stones.is_empty() {
        0
    } else {
        (stones[stones.len() - 1] - stones[0] + 1) as u8
    };

    LinePattern {
        stones,
        open_ends,
        span,
    }
}

/// Check if placing stone at pos creates a free-three in the given direction
/// This simulates placing the stone and then checks the pattern
fn creates_free_three_in_direction(
    board: &Board,
    pos: Pos,
    stone: Stone,
    dr: i32,
    dc: i32,
) -> bool {
    // scan_line starts with stones=[0] (the placed stone) and only reads
    // cells at distance 1-5 from pos. It never reads board.get(pos).
    // So we can safely analyze the original board without cloning.
    let pattern = scan_line(board, pos, stone, dr, dc);
    if is_free_three(&pattern) {
        return true;
    }
    // When gap-inclusive scan finds >3 stones, a consecutive subset might form
    // a free-three that gets hidden by the extra stone(s). Fallback to
    // consecutive-only scan to catch patterns like _BBB_ alongside a gap-connected 4th.
    if pattern.stones.len() > 3 {
        let consec = scan_line_consecutive(board, pos, stone, dr, dc);
        if is_free_three(&consec) {
            return true;
        }
    }
    false
}

/// Count how many free-threes would be created by placing stone at pos
pub fn count_free_threes(board: &Board, pos: Pos, stone: Stone) -> u8 {
    let mut count = 0;

    for &(dr, dc) in &DIRECTIONS {
        if creates_free_three_in_direction(board, pos, stone, dr, dc) {
            count += 1;
            // Early exit: double-three only needs 2+
            if count >= 2 {
                return count;
            }
        }
    }

    count
}

/// Check if move is a double-three (forbidden)
///
/// A double-three occurs when a single move creates two or more free-threes
/// simultaneously. This is forbidden unless the move also captures opponent stones.
///
/// # Arguments
/// * `board` - Current board state
/// * `pos` - Position being considered
/// * `stone` - Color of the stone being placed
///
/// # Returns
/// `true` if the move is a forbidden double-three, `false` otherwise
pub fn is_double_three(board: &Board, pos: Pos, stone: Stone) -> bool {
    // Exception: if this move captures, double-three is allowed
    // Use has_capture (no Vec allocation) instead of get_captured_positions
    if has_capture(board, pos, stone) {
        return false;
    }

    count_free_threes(board, pos, stone) >= 2
}

/// Check if a move is valid (not forbidden)
///
/// A move is valid if:
/// 1. The position is empty
/// 2. It doesn't create a double-three (unless it captures)
///
/// # Arguments
/// * `board` - Current board state
/// * `pos` - Position being considered
/// * `stone` - Color of the stone being placed
///
/// # Returns
/// `true` if the move is valid, `false` if forbidden
pub fn is_valid_move(board: &Board, pos: Pos, stone: Stone) -> bool {
    // Must be empty
    if !board.is_empty(pos) {
        return false;
    }

    // Must not be double-three (unless capture exception applies)
    if is_double_three(board, pos, stone) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_double_three_empty_board() {
        let board = Board::new();
        // Empty board - no double three possible
        assert!(!is_double_three(&board, Pos::new(9, 9), Stone::Black));
    }

    #[test]
    fn test_valid_move_empty_pos() {
        let board = Board::new();
        assert!(is_valid_move(&board, Pos::new(9, 9), Stone::Black));
    }

    #[test]
    fn test_invalid_move_occupied() {
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 9), Stone::Black);
        assert!(!is_valid_move(&board, Pos::new(9, 9), Stone::White));
    }

    #[test]
    fn test_free_three_consecutive_horizontal() {
        let mut board = Board::new();
        // Setup: _ B _ B _
        //        5 6 7 8 9
        // Place at 7 creates: _ B B B _
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 8), Stone::Black);

        // After placing at (9,7): creates free-three (consecutive)
        let free_threes = count_free_threes(&board, Pos::new(9, 7), Stone::Black);
        assert_eq!(free_threes, 1, "Should create exactly 1 free-three");
    }

    #[test]
    fn test_free_three_with_gap() {
        let mut board = Board::new();
        // Setup: _ B B _ _
        //        5 6 7 8 9
        // Place at 9 creates: _ B B _ B _ (gap pattern)
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::Black);

        // After placing at (9,9): creates free-three with gap
        let free_threes = count_free_threes(&board, Pos::new(9, 9), Stone::Black);
        assert_eq!(free_threes, 1, "Should create exactly 1 free-three (with gap)");
    }

    #[test]
    fn test_not_free_three_blocked() {
        let mut board = Board::new();
        // Setup: W B _ B _ (one end blocked by opponent)
        board.place_stone(Pos::new(9, 5), Stone::White); // Block
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 8), Stone::Black);

        // After placing at (9,7): NOT a free-three (left end blocked)
        let free_threes = count_free_threes(&board, Pos::new(9, 7), Stone::Black);
        assert_eq!(free_threes, 0, "Should not be free-three (blocked)");
    }

    #[test]
    fn test_double_three_cross_pattern() {
        let mut board = Board::new();
        // Create a + pattern where center creates double-three
        //     col: 7 8 9 10 11
        // row 7:   . . _ .  .
        // row 8:   . . B .  .
        // row 9:   _ B _ B  _
        // row 10:  . . B .  .
        // row 11:  . . _ .  .
        //
        // Horizontal: _ B _ B _ (place at 9 creates _ B B B _)
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);

        // Vertical: _ B _ B _ (place at 9 creates _ B B B _)
        board.place_stone(Pos::new(8, 9), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::Black);

        // Placing at (9,9) should be double-three
        assert!(
            is_double_three(&board, Pos::new(9, 9), Stone::Black),
            "Cross pattern should be double-three"
        );
        assert!(
            !is_valid_move(&board, Pos::new(9, 9), Stone::Black),
            "Double-three should be invalid move"
        );
    }

    #[test]
    fn test_double_three_diagonal_cross() {
        let mut board = Board::new();
        // Create diagonal cross pattern
        //     col: 7 8 9 10 11
        // row 7:   B . _ .  B
        // row 8:   . B _ B  .
        // row 9:   _ _ X _  _
        // row 10:  . B _ B  .
        // row 11:  B . _ .  B
        //
        // Diagonal SE: B at (8,8), (10,10)
        board.place_stone(Pos::new(8, 8), Stone::Black);
        board.place_stone(Pos::new(10, 10), Stone::Black);

        // Diagonal SW: B at (8,10), (10,8)
        board.place_stone(Pos::new(8, 10), Stone::Black);
        board.place_stone(Pos::new(10, 8), Stone::Black);

        // Placing at (9,9) should be double-three (two diagonal free-threes)
        let free_threes = count_free_threes(&board, Pos::new(9, 9), Stone::Black);
        assert_eq!(free_threes, 2, "Should create 2 diagonal free-threes");
        assert!(
            is_double_three(&board, Pos::new(9, 9), Stone::Black),
            "Diagonal cross should be double-three"
        );
    }

    #[test]
    fn test_single_free_three_allowed() {
        let mut board = Board::new();
        // Only horizontal: _ B _ B _
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);

        // Single free-three is allowed
        let free_threes = count_free_threes(&board, Pos::new(9, 9), Stone::Black);
        assert_eq!(free_threes, 1, "Should be exactly 1 free-three");
        assert!(
            !is_double_three(&board, Pos::new(9, 9), Stone::Black),
            "Single free-three should not be double-three"
        );
        assert!(
            is_valid_move(&board, Pos::new(9, 9), Stone::Black),
            "Single free-three should be valid"
        );
    }

    #[test]
    fn test_double_three_with_capture_allowed() {
        let mut board = Board::new();
        // Setup double-three pattern
        // Horizontal: _ B _ B _
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);

        // Vertical: _ B _ B _
        board.place_stone(Pos::new(8, 9), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::Black);

        // Add capture opportunity
        // We need: B W W _ pattern where _ is at (9,9)
        // Put B at (9,6) and W W at (9,7), (9,8)
        // But (9,8) already has Black, so we need different setup

        // Alternative: capture opportunity in another direction
        // B W W X (where X is (9,9))
        // Need B at (9,6) and W at (9,7), W at (9,8)
        // Conflict: (9,8) has Black

        // Let's create a different capture pattern using vertical
        // We have B at (8,9), want B at (6,9), W at (7,9), and place at where W is
        // This gets complex. Let's use a simpler approach:

        // Remove existing stones and create a capture-compatible pattern
        let mut board2 = Board::new();

        // Create capture pattern: B _ W W B (place at _ captures W W)
        // Put at row 9: B at col 5, W at col 7,8, B at col 9
        board2.place_stone(Pos::new(9, 5), Stone::Black);
        board2.place_stone(Pos::new(9, 7), Stone::White);
        board2.place_stone(Pos::new(9, 8), Stone::White);
        board2.place_stone(Pos::new(9, 9), Stone::Black);

        // Now add vertical free-three setup: _ B _ B _ at column 6
        board2.place_stone(Pos::new(8, 6), Stone::Black);
        board2.place_stone(Pos::new(10, 6), Stone::Black);

        // And another direction to make it double-three
        board2.place_stone(Pos::new(8, 8), Stone::Black);
        board2.place_stone(Pos::new(10, 4), Stone::Black);

        // Place at (9,6) - this captures W W and might create free-threes
        // Capture: B(9,5) [place](9,6) W(9,7) W(9,8) B(9,9)
        // After capture, W W removed, so horizontal pattern changes

        // First verify capture exists
        let captures = get_captured_positions(&board2, Pos::new(9, 6), Stone::Black);
        assert_eq!(captures.len(), 2, "Should capture 2 stones");

        // Even if this creates free-threes, it's allowed because of capture
        assert!(
            !is_double_three(&board2, Pos::new(9, 6), Stone::Black),
            "Double-three with capture should be allowed"
        );
    }

    #[test]
    fn test_four_stones_not_free_three() {
        let mut board = Board::new();
        // 4 stones in a row is NOT a free-three
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        // Place at 8 creates 4 in a row
        let free_threes = count_free_threes(&board, Pos::new(9, 8), Stone::Black);
        assert_eq!(free_threes, 0, "4 stones should not count as free-three");
    }

    #[test]
    fn test_two_stones_not_free_three() {
        let mut board = Board::new();
        // 2 stones is not enough for free-three
        board.place_stone(Pos::new(9, 8), Stone::Black);

        let free_threes = count_free_threes(&board, Pos::new(9, 9), Stone::Black);
        assert_eq!(free_threes, 0, "2 stones should not be free-three");
    }

    #[test]
    fn test_edge_position_blocked_by_edge() {
        let mut board = Board::new();
        // At edge, blocked by board boundary
        // Row 0: X B _ B _ (X = edge/out of bounds)
        //        -1 0 1 2 3
        // Actually: col 0 is at edge
        // Pattern: B _ B _ at col 0, 1, 2, 3
        board.place_stone(Pos::new(0, 0), Stone::Black); // At edge
        board.place_stone(Pos::new(0, 2), Stone::Black);

        // Place at 1 creates: B B B _ but left side is at edge (col -1 doesn't exist)
        // This is blocked on one side, so NOT a free-three
        let free_threes = count_free_threes(&board, Pos::new(0, 1), Stone::Black);
        assert_eq!(free_threes, 0, "Edge-blocked pattern should not be free-three");
    }

    #[test]
    fn test_edge_with_space_is_free_three() {
        let mut board = Board::new();
        // Row 0: _ B _ B _ (with space before first B)
        //        0 1 2 3 4
        // This IS a free-three because both ends are open
        board.place_stone(Pos::new(0, 1), Stone::Black);
        board.place_stone(Pos::new(0, 3), Stone::Black);

        // Place at 2 creates: _ B B B _ which is free-three
        // Test scan_line directly
        let mut temp = board.clone();
        temp.place_stone(Pos::new(0, 2), Stone::Black);
        let pattern = scan_line(&temp, Pos::new(0, 2), Stone::Black, 0, 1);

        // Debug: pattern should be stones at [-1, 0, 1] (cols 1, 2, 3)
        // Open ends: col 0 (empty) and col 4 (empty)
        assert_eq!(pattern.stones.len(), 3, "Should have 3 stones");
        assert_eq!(pattern.open_ends, 2, "Should have 2 open ends (col 0 and col 4)");
        assert_eq!(pattern.span, 3, "Span should be 3");

        let free_threes = count_free_threes(&board, Pos::new(0, 2), Stone::Black);
        assert_eq!(free_threes, 1, "Pattern with space before edge is free-three");
    }

    #[test]
    fn test_free_three_pattern_scan() {
        let mut board = Board::new();
        // _ B B _ B _ pattern (spaced three)
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::Black);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        // This already has 3 stones, placing at 8 makes 4
        // Let's test a different pattern
        let mut board2 = Board::new();
        // _ B _ B _ (placing at 7 makes consecutive 3: _ B B B _)
        board2.place_stone(Pos::new(9, 6), Stone::Black);
        board2.place_stone(Pos::new(9, 8), Stone::Black);

        let temp_board = {
            let mut b = board2.clone();
            b.place_stone(Pos::new(9, 7), Stone::Black);
            b
        };
        let pattern = scan_line(&temp_board, Pos::new(9, 7), Stone::Black, 0, 1);

        assert_eq!(pattern.stones.len(), 3, "Should have 3 stones");
        assert_eq!(pattern.open_ends, 2, "Should have 2 open ends");
        assert_eq!(pattern.span, 3, "Span should be 3 for consecutive");
    }

    #[test]
    fn test_is_free_three_logic() {
        // Test consecutive pattern
        let consecutive = LinePattern {
            stones: vec![-1, 0, 1],
            open_ends: 2,
            span: 3,
        };
        assert!(is_free_three(&consecutive), "Consecutive 3 open should be free-three");

        // Test gapped pattern
        let gapped = LinePattern {
            stones: vec![-1, 0, 2],
            open_ends: 2,
            span: 4,
        };
        assert!(is_free_three(&gapped), "Gapped 3 with span 4 should be free-three");

        // Test blocked pattern
        let blocked = LinePattern {
            stones: vec![-1, 0, 1],
            open_ends: 1,
            span: 3,
        };
        assert!(!is_free_three(&blocked), "Blocked should not be free-three");

        // Test 4 stones (not free-three)
        let four = LinePattern {
            stones: vec![-1, 0, 1, 2],
            open_ends: 2,
            span: 4,
        };
        assert!(!is_free_three(&four), "4 stones should not be free-three");

        // Test too spread pattern
        let spread = LinePattern {
            stones: vec![-2, 0, 3],
            open_ends: 2,
            span: 6,
        };
        assert!(!is_free_three(&spread), "Too spread should not be free-three");
    }

    /// Regression test: Game 1 Move #23 (H10) was a double-three that wasn't detected.
    /// Horizontal: F10-G10-H10 = _BBB_ (free-three) — BUT K10 exists at +2 via gap,
    /// making scan_line see 4 stones [-2,-1,0,2] instead of 3.
    /// Vertical: H10-H11-H12 = _BBB_ (free-three, correctly detected).
    /// With the consecutive fallback, both free-threes are now detected.
    #[test]
    fn test_double_three_with_gap_connected_stone() {
        let mut board = Board::new();
        // Reconstruct Game 1 state at move #23 (before placing H10)
        // Black stones (7 total):
        // K10 = Pos(9,9), H12 = Pos(11,7), G10 = Pos(9,6), F10 = Pos(9,5)
        // J7 = Pos(6,8), H11 = Pos(10,7), L8 = Pos(7,10)
        board.place_stone(Pos::new(9, 9), Stone::Black);  // K10
        board.place_stone(Pos::new(11, 7), Stone::Black); // H12
        board.place_stone(Pos::new(9, 6), Stone::Black);  // G10
        board.place_stone(Pos::new(9, 5), Stone::Black);  // F10
        board.place_stone(Pos::new(6, 8), Stone::Black);  // J7
        board.place_stone(Pos::new(10, 7), Stone::Black); // H11
        board.place_stone(Pos::new(7, 10), Stone::Black); // L8

        // White stones (5 total):
        // M11 = Pos(10,11), K14 = Pos(13,9), G9 = Pos(8,6)
        // J12 = Pos(11,8), K11 = Pos(10,9)
        board.place_stone(Pos::new(10, 11), Stone::White); // M11
        board.place_stone(Pos::new(13, 9), Stone::White);  // K14
        board.place_stone(Pos::new(8, 6), Stone::White);   // G9
        board.place_stone(Pos::new(11, 8), Stone::White);  // J12
        board.place_stone(Pos::new(10, 9), Stone::White);  // K11

        // H10 = Pos(9, 7) — should be forbidden double-three
        let pos = Pos::new(9, 7);
        let free_threes = count_free_threes(&board, pos, Stone::Black);
        assert_eq!(
            free_threes, 2,
            "H10 should create 2 free-threes (horizontal F10-G10-H10, vertical H10-H11-H12)"
        );
        assert!(
            is_double_three(&board, pos, Stone::Black),
            "H10 should be a forbidden double-three"
        );
        assert!(
            !is_valid_move(&board, pos, Stone::Black),
            "H10 should be an invalid move"
        );
    }

    /// Test that consecutive fallback doesn't falsely detect free-threes
    /// when the consecutive subset is blocked or has only 2 stones.
    #[test]
    fn test_consecutive_fallback_no_false_positive() {
        let mut board = Board::new();
        // Setup: W B B _ B _ (left end blocked by opponent)
        // Gap-inclusive scan: 3 stones [-1, 0, 2] but left blocked by W → open_ends=1
        // Consecutive scan: 2 stones [-1, 0] → not free-three (only 2)
        board.place_stone(Pos::new(9, 4), Stone::White); // blocker
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::Black);

        let free_threes = count_free_threes(&board, Pos::new(9, 6), Stone::Black);
        assert_eq!(free_threes, 0, "Blocked pattern should not be free-three");
    }

    #[test]
    fn test_triple_free_three() {
        let mut board = Board::new();
        // Create 3 free-threes at center (horizontal, vertical, diagonal)
        // This is definitely forbidden

        // Horizontal: _ B _ B _
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);

        // Vertical: _ B _ B _
        board.place_stone(Pos::new(8, 9), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::Black);

        // Diagonal SE: _ B _ B _
        board.place_stone(Pos::new(8, 8), Stone::Black);
        board.place_stone(Pos::new(10, 10), Stone::Black);

        let free_threes = count_free_threes(&board, Pos::new(9, 9), Stone::Black);
        assert!(free_threes >= 2, "Should be at least double-three");
        assert!(
            is_double_three(&board, Pos::new(9, 9), Stone::Black),
            "Triple free-three is still forbidden"
        );
    }
}
