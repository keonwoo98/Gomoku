//! Capture rules for Ninuki-renju (Pente-style pair capture)
//!
//! Capture pattern: X-O-O-X where X is the capturing player's stone
//! and O is the opponent's stone. Only exactly 2 stones can be captured.

use crate::board::{Board, Pos, Stone};

/// Direction vectors for capture checking (4 directions)
const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1),  // Horizontal →
    (1, 0),  // Vertical ↓
    (1, 1),  // Diagonal ↘
    (1, -1), // Diagonal ↙
];

/// Find positions that would be captured if stone is placed at pos.
///
/// Capture pattern: X-O-O-X where X is the placed stone (at pos) and
/// the existing stone at distance 3.
///
/// # Arguments
/// * `board` - Current board state
/// * `pos` - Position where stone will be placed
/// * `stone` - Color of the stone being placed
///
/// # Returns
/// Vector of positions that would be captured (always even, pairs of stones)
pub fn get_captured_positions(board: &Board, pos: Pos, stone: Stone) -> Vec<Pos> {
    let mut captured = Vec::new();
    let opponent = stone.opponent();

    for &(dr, dc) in &DIRECTIONS {
        // Check both directions along this line
        for sign in [-1i32, 1i32] {
            let dr = dr * sign;
            let dc = dc * sign;

            // Pattern: placed_stone(pos) - opp(+1) - opp(+2) - our_stone(+3)
            let r1 = pos.row as i32 + dr;
            let c1 = pos.col as i32 + dc;
            let r2 = pos.row as i32 + dr * 2;
            let c2 = pos.col as i32 + dc * 2;
            let r3 = pos.row as i32 + dr * 3;
            let c3 = pos.col as i32 + dc * 3;

            // Check bounds for the farthest position
            if !Pos::is_valid(r3, c3) {
                continue;
            }

            let pos1 = Pos::new(r1 as u8, c1 as u8);
            let pos2 = Pos::new(r2 as u8, c2 as u8);
            let pos3 = Pos::new(r3 as u8, c3 as u8);

            // Check pattern: [placed] - opp - opp - our
            if board.get(pos1) == opponent
                && board.get(pos2) == opponent
                && board.get(pos3) == stone
            {
                captured.push(pos1);
                captured.push(pos2);
            }
        }
    }

    captured
}

/// Execute captures and return captured positions.
///
/// This function:
/// 1. Finds all positions that would be captured
/// 2. Removes captured stones from the board
/// 3. Updates the capture count for the capturing player
///
/// # Arguments
/// * `board` - Mutable board to modify
/// * `pos` - Position where stone was just placed
/// * `stone` - Color of the stone that was placed
///
/// # Returns
/// Vector of positions that were captured
pub fn execute_captures(board: &mut Board, pos: Pos, stone: Stone) -> Vec<Pos> {
    let captured = get_captured_positions(board, pos, stone);

    for &cap_pos in &captured {
        board.remove_stone(cap_pos);
    }

    // Add capture count (pairs, not individual stones)
    let pairs = captured.len() / 2;
    board.add_captures(stone, pairs as u8);

    captured
}

/// Check if a move would result in any captures.
///
/// This is useful for quick checking without actually executing captures.
#[inline]
pub fn has_capture(board: &Board, pos: Pos, stone: Stone) -> bool {
    let opponent = stone.opponent();

    for &(dr, dc) in &DIRECTIONS {
        for sign in [-1i32, 1i32] {
            let dr = dr * sign;
            let dc = dc * sign;

            let r1 = pos.row as i32 + dr;
            let c1 = pos.col as i32 + dc;
            let r2 = pos.row as i32 + dr * 2;
            let c2 = pos.col as i32 + dc * 2;
            let r3 = pos.row as i32 + dr * 3;
            let c3 = pos.col as i32 + dc * 3;

            if !Pos::is_valid(r3, c3) {
                continue;
            }

            let pos1 = Pos::new(r1 as u8, c1 as u8);
            let pos2 = Pos::new(r2 as u8, c2 as u8);
            let pos3 = Pos::new(r3 as u8, c3 as u8);

            if board.get(pos1) == opponent
                && board.get(pos2) == opponent
                && board.get(pos3) == stone
            {
                return true;
            }
        }
    }

    false
}

/// Count how many pairs would be captured by a move.
#[inline]
pub fn count_captures(board: &Board, pos: Pos, stone: Stone) -> u8 {
    (get_captured_positions(board, pos, stone).len() / 2) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_horizontal() {
        let mut board = Board::new();
        // Setup: B _ W W B  (B places at _, captures W W)
        // Positions: 5, 6, 7, 8, 9
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(9, 6), Stone::Black);
        assert_eq!(captured.len(), 2);
        assert!(captured.contains(&Pos::new(9, 7)));
        assert!(captured.contains(&Pos::new(9, 8)));
    }

    #[test]
    fn test_capture_vertical() {
        let mut board = Board::new();
        // Vertical capture: B at row 5, _ at row 6, W W at rows 7-8, B at row 9
        board.place_stone(Pos::new(5, 9), Stone::Black);
        board.place_stone(Pos::new(7, 9), Stone::White);
        board.place_stone(Pos::new(8, 9), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(6, 9), Stone::Black);
        assert_eq!(captured.len(), 2);
        assert!(captured.contains(&Pos::new(7, 9)));
        assert!(captured.contains(&Pos::new(8, 9)));
    }

    #[test]
    fn test_capture_diagonal_se() {
        let mut board = Board::new();
        // Diagonal ↘ capture
        board.place_stone(Pos::new(5, 5), Stone::Black);
        board.place_stone(Pos::new(7, 7), Stone::White);
        board.place_stone(Pos::new(8, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(6, 6), Stone::Black);
        assert_eq!(captured.len(), 2);
        assert!(captured.contains(&Pos::new(7, 7)));
        assert!(captured.contains(&Pos::new(8, 8)));
    }

    #[test]
    fn test_capture_diagonal_sw() {
        let mut board = Board::new();
        // Diagonal ↙ capture
        board.place_stone(Pos::new(5, 9), Stone::Black);
        board.place_stone(Pos::new(7, 7), Stone::White);
        board.place_stone(Pos::new(8, 6), Stone::White);
        board.place_stone(Pos::new(9, 5), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(6, 8), Stone::Black);
        assert_eq!(captured.len(), 2);
        assert!(captured.contains(&Pos::new(7, 7)));
        assert!(captured.contains(&Pos::new(8, 6)));
    }

    #[test]
    fn test_no_capture_single_stone() {
        let mut board = Board::new();
        // B _ W B  (only 1 white stone - no capture)
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(9, 6), Stone::Black);
        assert_eq!(captured.len(), 0);
    }

    #[test]
    fn test_no_capture_three_stones() {
        let mut board = Board::new();
        // B _ W W W B  (3 white stones - no capture, must be exactly 2)
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::White);
        board.place_stone(Pos::new(9, 10), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(9, 6), Stone::Black);
        assert_eq!(captured.len(), 0);
    }

    #[test]
    fn test_execute_capture() {
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        // Place at 9,6 to capture
        board.place_stone(Pos::new(9, 6), Stone::Black);
        let captured = execute_captures(&mut board, Pos::new(9, 6), Stone::Black);

        assert_eq!(captured.len(), 2);
        assert_eq!(board.captures(Stone::Black), 1); // 1 pair
        assert!(board.is_empty(Pos::new(9, 7)));
        assert!(board.is_empty(Pos::new(9, 8)));
    }

    #[test]
    fn test_multiple_captures_same_move() {
        let mut board = Board::new();
        // Setup for 2 captures in one move (horizontal both directions)
        // B W W _ W W B
        // 3 4 5 6 7 8 9
        board.place_stone(Pos::new(9, 3), Stone::Black);
        board.place_stone(Pos::new(9, 4), Stone::White);
        board.place_stone(Pos::new(9, 5), Stone::White);
        // (9, 6) is where black will play
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        board.place_stone(Pos::new(9, 6), Stone::Black);
        let captured = execute_captures(&mut board, Pos::new(9, 6), Stone::Black);

        assert_eq!(captured.len(), 4); // 2 pairs = 4 stones
        assert_eq!(board.captures(Stone::Black), 2); // 2 pairs
    }

    #[test]
    fn test_has_capture() {
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        assert!(has_capture(&board, Pos::new(9, 6), Stone::Black));
        assert!(!has_capture(&board, Pos::new(9, 6), Stone::White));
        assert!(!has_capture(&board, Pos::new(0, 0), Stone::Black));
    }

    #[test]
    fn test_count_captures() {
        let mut board = Board::new();
        // Setup for 2 pairs capture
        board.place_stone(Pos::new(9, 3), Stone::Black);
        board.place_stone(Pos::new(9, 4), Stone::White);
        board.place_stone(Pos::new(9, 5), Stone::White);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        assert_eq!(count_captures(&board, Pos::new(9, 6), Stone::Black), 2);
    }

    #[test]
    fn test_white_captures_black() {
        let mut board = Board::new();
        // W _ B B W (White captures Black pair)
        board.place_stone(Pos::new(5, 5), Stone::White);
        board.place_stone(Pos::new(5, 7), Stone::Black);
        board.place_stone(Pos::new(5, 8), Stone::Black);
        board.place_stone(Pos::new(5, 9), Stone::White);

        board.place_stone(Pos::new(5, 6), Stone::White);
        let captured = execute_captures(&mut board, Pos::new(5, 6), Stone::White);

        assert_eq!(captured.len(), 2);
        assert_eq!(board.captures(Stone::White), 1);
        assert!(board.is_empty(Pos::new(5, 7)));
        assert!(board.is_empty(Pos::new(5, 8)));
    }

    #[test]
    fn test_capture_at_board_edge() {
        let mut board = Board::new();
        // Edge capture: B _ W W B starting from column 0
        board.place_stone(Pos::new(0, 0), Stone::Black);
        board.place_stone(Pos::new(0, 2), Stone::White);
        board.place_stone(Pos::new(0, 3), Stone::White);
        board.place_stone(Pos::new(0, 4), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(0, 1), Stone::Black);
        assert_eq!(captured.len(), 2);
    }

    #[test]
    fn test_no_capture_out_of_bounds() {
        let mut board = Board::new();
        // Near edge - should not crash
        board.place_stone(Pos::new(0, 0), Stone::Black);
        board.place_stone(Pos::new(0, 1), Stone::White);

        // Checking capture at edge should not panic
        let captured = get_captured_positions(&board, Pos::new(0, 2), Stone::Black);
        assert_eq!(captured.len(), 0);
    }

    #[test]
    fn test_cross_capture() {
        let mut board = Board::new();
        // Cross pattern: captures in 2 different directions
        //     B
        //     W
        // B W _ W B
        //     W
        //     B
        let center = Pos::new(9, 9);

        // Horizontal
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 10), Stone::White);
        board.place_stone(Pos::new(9, 11), Stone::White);
        board.place_stone(Pos::new(9, 12), Stone::Black);

        // Vertical
        board.place_stone(Pos::new(6, 9), Stone::Black);
        board.place_stone(Pos::new(7, 9), Stone::White);
        board.place_stone(Pos::new(8, 9), Stone::White);
        board.place_stone(Pos::new(10, 9), Stone::White);
        board.place_stone(Pos::new(11, 9), Stone::White);
        board.place_stone(Pos::new(12, 9), Stone::Black);

        board.place_stone(center, Stone::Black);
        let captured = execute_captures(&mut board, center, Stone::Black);

        // Should capture 4 pairs = 8 stones
        assert_eq!(captured.len(), 8);
        assert_eq!(board.captures(Stone::Black), 4);
    }
}
