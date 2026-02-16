//! Win condition checking for Ninuki-renju (Pente-style Gomoku)
//!
//! Win conditions:
//! 1. Five or more stones in a row
//! 2. Capture 10 opponent stones (5 pairs)
//!
//! Endgame capture rule: A 5-in-a-row only wins if the opponent
//! cannot break it by capturing a pair from the line.

use crate::board::{Board, Pos, Stone};

use super::capture::get_captured_positions;

/// Direction vectors for line checking (4 directions)
const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1),  // Horizontal
    (1, 0),  // Vertical
    (1, 1),  // Diagonal SE
    (1, -1), // Diagonal SW
];

/// Check if there's 5+ in a row for the given color
pub fn has_five_in_row(board: &Board, stone: Stone) -> bool {
    find_five_positions(board, stone).is_some()
}

/// Fast five-in-a-row check at a specific position.
///
/// Only checks 4 directions from the given position. No allocation.
/// Much faster than `has_five_in_row` which iterates ALL stones.
#[inline]
pub fn has_five_at_pos(board: &Board, pos: Pos, color: Stone) -> bool {
    let sz = 19i8;
    let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
    for (dr, dc) in dirs {
        let mut count = 1i32;
        // Positive direction
        let mut r = pos.row as i8 + dr;
        let mut c = pos.col as i8 + dc;
        while r >= 0 && r < sz && c >= 0 && c < sz {
            if board.get(Pos::new(r as u8, c as u8)) == color {
                count += 1;
                r += dr;
                c += dc;
            } else {
                break;
            }
        }
        // Negative direction
        r = pos.row as i8 - dr;
        c = pos.col as i8 - dc;
        while r >= 0 && r < sz && c >= 0 && c < sz {
            if board.get(Pos::new(r as u8, c as u8)) == color {
                count += 1;
                r -= dr;
                c -= dc;
            } else {
                break;
            }
        }
        if count >= 5 {
            return true;
        }
    }
    false
}

/// Fast five-in-a-row position finder at a specific position.
///
/// Like `has_five_at_pos` but returns the positions forming the five.
/// Only checks 4 directions from the given position. Only call when
/// `has_five_at_pos` already returned true (rare path, no perf concern).
pub fn find_five_line_at_pos(board: &Board, pos: Pos, color: Stone) -> Option<Vec<Pos>> {
    let sz = 19i8;
    let dirs: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
    for (dr, dc) in dirs {
        let mut line = vec![pos];
        // Positive direction
        let mut r = pos.row as i8 + dr;
        let mut c = pos.col as i8 + dc;
        while r >= 0 && r < sz && c >= 0 && c < sz {
            if board.get(Pos::new(r as u8, c as u8)) == color {
                line.push(Pos::new(r as u8, c as u8));
                r += dr;
                c += dc;
            } else {
                break;
            }
        }
        // Negative direction
        r = pos.row as i8 - dr;
        c = pos.col as i8 - dc;
        while r >= 0 && r < sz && c >= 0 && c < sz {
            if board.get(Pos::new(r as u8, c as u8)) == color {
                line.push(Pos::new(r as u8, c as u8));
                r -= dr;
                c -= dc;
            } else {
                break;
            }
        }
        if line.len() >= 5 {
            return Some(line);
        }
    }
    None
}

/// Find the positions of a 5-in-a-row if exists
///
/// Returns Some(Vec<Pos>) with at least 5 positions if a winning line exists,
/// None otherwise.
pub fn find_five_positions(board: &Board, stone: Stone) -> Option<Vec<Pos>> {
    let stones = board.stones(stone)?;

    for pos in stones.iter_ones() {
        for &(dr, dc) in &DIRECTIONS {
            let mut line = vec![pos];

            // Extend in negative direction first
            for i in 1..5 {
                let r = pos.row as i32 - dr * i;
                let c = pos.col as i32 - dc * i;
                if !Pos::is_valid(r, c) {
                    break;
                }
                let prev = Pos::new(r as u8, c as u8);
                if board.get(prev) == stone {
                    line.insert(0, prev);
                } else {
                    break;
                }
            }

            // Extend in positive direction
            for i in 1..5 {
                let r = pos.row as i32 + dr * i;
                let c = pos.col as i32 + dc * i;
                if !Pos::is_valid(r, c) {
                    break;
                }
                let next = Pos::new(r as u8, c as u8);
                if board.get(next) == stone {
                    line.push(next);
                } else {
                    break;
                }
            }

            if line.len() >= 5 {
                return Some(line);
            }
        }
    }
    None
}

/// Check if opponent can break the 5-in-row by capture
///
/// Returns true if the 5-in-row can be broken by the opponent
/// placing a stone that captures part of the winning line.
/// This is a STATIC game-rule check (no look-ahead for recreation).
pub fn can_break_five_by_capture(board: &Board, five_positions: &[Pos], five_color: Stone) -> bool {
    let opponent = five_color.opponent();

    // For each empty position within radius 2 of the five stones.
    // Radius 2 is needed because capture pattern X-O-O-X means the
    // capturing stone can be up to 2 steps away from the nearest
    // five-stone (e.g., placing at distance 2 captures the pair in between).
    for &pos in five_positions {
        for dr in -2i32..=2 {
            for dc in -2i32..=2 {
                if dr == 0 && dc == 0 {
                    continue;
                }

                let r = pos.row as i32 + dr;
                let c = pos.col as i32 + dc;

                if !Pos::is_valid(r, c) {
                    continue;
                }

                let adj_pos = Pos::new(r as u8, c as u8);
                if !board.is_empty(adj_pos) {
                    continue;
                }

                // Check if opponent placing here would capture part of the five
                let would_capture = get_captured_positions(board, adj_pos, opponent);
                for cap in would_capture {
                    if five_positions.contains(&cap) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Find all positions where opponent can break the five by capture.
///
/// Like `can_break_five_by_capture` but returns the actual positions
/// where the opponent could place a stone to break the five.
/// Used by the engine to force a break move when the opponent has a
/// breakable five on the board.
pub fn find_five_break_moves(board: &Board, five_positions: &[Pos], five_color: Stone) -> Vec<Pos> {
    let opponent = five_color.opponent();
    let mut break_moves = Vec::new();

    for &pos in five_positions {
        for dr in -2i32..=2 {
            for dc in -2i32..=2 {
                if dr == 0 && dc == 0 {
                    continue;
                }

                let r = pos.row as i32 + dr;
                let c = pos.col as i32 + dc;

                if !Pos::is_valid(r, c) {
                    continue;
                }

                let adj_pos = Pos::new(r as u8, c as u8);
                if !board.is_empty(adj_pos) {
                    continue;
                }
                if break_moves.contains(&adj_pos) {
                    continue;
                }

                let would_capture = get_captured_positions(board, adj_pos, opponent);
                for cap in would_capture {
                    if five_positions.contains(&cap) {
                        break_moves.push(adj_pos);
                        break;
                    }
                }
            }
        }
    }
    break_moves
}

/// Check for a winner
///
/// Returns `Some(Stone)` if there's a winner, `None` otherwise.
///
/// Win conditions checked:
/// 1. Capture win: 5 pairs (10 stones) captured
/// 2. Five-in-a-row win (unless opponent can break it by capture)
pub fn check_winner(board: &Board) -> Option<Stone> {
    // Check capture win (10 captures = 5 pairs)
    if board.captures(Stone::Black) >= 5 {
        return Some(Stone::Black);
    }
    if board.captures(Stone::White) >= 5 {
        return Some(Stone::White);
    }

    // Check 5-in-a-row win
    for stone in [Stone::Black, Stone::White] {
        if let Some(five) = find_five_positions(board, stone) {
            // Endgame capture rule: if opponent can break it, no win yet
            if !can_break_five_by_capture(board, &five, stone) {
                return Some(stone);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_five_in_row_horizontal() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }
        assert!(has_five_in_row(&board, Stone::Black));
        assert!(!has_five_in_row(&board, Stone::White));
    }

    #[test]
    fn test_five_in_row_vertical() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(i, 9), Stone::Black);
        }
        assert!(has_five_in_row(&board, Stone::Black));
    }

    #[test]
    fn test_five_in_row_diagonal() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(i, i), Stone::White);
        }
        assert!(has_five_in_row(&board, Stone::White));
    }

    #[test]
    fn test_six_in_row_also_wins() {
        let mut board = Board::new();
        for i in 0..6 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }
        assert!(has_five_in_row(&board, Stone::Black));
    }

    #[test]
    fn test_four_in_row_not_win() {
        let mut board = Board::new();
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }
        assert!(!has_five_in_row(&board, Stone::Black));
    }

    #[test]
    fn test_capture_win() {
        let mut board = Board::new();
        board.add_captures(Stone::Black, 5);
        assert_eq!(check_winner(&board), Some(Stone::Black));
    }

    #[test]
    fn test_capture_win_white() {
        let mut board = Board::new();
        board.add_captures(Stone::White, 5);
        assert_eq!(check_winner(&board), Some(Stone::White));
    }

    #[test]
    fn test_breakable_five() {
        // Five at row 9, cols 5-9 (Black), with White bracket at (7,7)
        // and extra Black at (8,7). White can capture (9,7)+(8,7) via (10,7).
        //
        //         col: 5 6 7 8 9
        // row 7:       . . W . .    <- bracket stone
        // row 8:       . . B . .    <- extra black (captured with five-stone)
        // row 9:       B B B B B    <- five in a row
        // row 10:      . . _ . .    <- White places here for break
        let mut board = Board::new();
        board.place_stone(Pos::new(7, 7), Stone::White);
        for i in 5..10 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }
        board.place_stone(Pos::new(8, 7), Stone::Black);

        let five = find_five_positions(&board, Stone::Black).unwrap();
        // STATIC check: the five IS physically breakable
        assert!(can_break_five_by_capture(&board, &five, Stone::Black));
    }

    #[test]
    fn test_unbreakable_five_wins() {
        let mut board = Board::new();
        // 5 blacks with no capture threat
        for i in 5..10 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }
        assert_eq!(check_winner(&board), Some(Stone::Black));
    }

    #[test]
    fn test_no_winner() {
        let board = Board::new();
        assert_eq!(check_winner(&board), None);
    }

    #[test]
    fn test_diagonal_sw_five() {
        let mut board = Board::new();
        // Diagonal from (4, 8) to (8, 4)
        for i in 0..5 {
            board.place_stone(Pos::new(4 + i, 8 - i), Stone::White);
        }
        assert!(has_five_in_row(&board, Stone::White));
        assert_eq!(check_winner(&board), Some(Stone::White));
    }

    #[test]
    fn test_five_at_board_edge() {
        let mut board = Board::new();
        // 5 blacks at bottom edge
        for i in 0..5 {
            board.place_stone(Pos::new(18, i), Stone::Black);
        }
        assert!(has_five_in_row(&board, Stone::Black));
        assert_eq!(check_winner(&board), Some(Stone::Black));
    }

    #[test]
    fn test_five_at_corner() {
        let mut board = Board::new();
        // Diagonal from (14, 14) to (18, 18)
        for i in 0..5 {
            board.place_stone(Pos::new(14 + i, 14 + i), Stone::White);
        }
        assert!(has_five_in_row(&board, Stone::White));
        assert_eq!(check_winner(&board), Some(Stone::White));
    }

    #[test]
    fn test_empty_not_five() {
        let board = Board::new();
        assert!(!has_five_in_row(&board, Stone::Black));
        assert!(!has_five_in_row(&board, Stone::White));
        assert!(find_five_positions(&board, Stone::Empty).is_none());
    }

    #[test]
    fn test_capture_beats_five() {
        // If both have winning conditions, capture is checked first
        let mut board = Board::new();
        board.add_captures(Stone::White, 5);
        for i in 0..5 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }
        // White wins by capture (checked first)
        assert_eq!(check_winner(&board), Some(Stone::White));
    }
}
