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
pub fn can_break_five_by_capture(board: &Board, five_positions: &[Pos], five_color: Stone) -> bool {
    let opponent = five_color.opponent();

    // For each empty position adjacent to the five
    for &pos in five_positions {
        for dr in -1i32..=1 {
            for dc in -1i32..=1 {
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
        let mut board = Board::new();
        // Setup: 5 blacks in a row, but white can capture 2 of them
        // Capture pattern: W _ B B W where _ is empty for White to place
        //
        // Board setup:
        // W B B B B B _ W
        // 0 1 2 3 4 5 6 7
        //
        // If White places at position 6, pattern is: W[7] - B[5] - B[4] - W[?]
        // We need: W[other] - B[x] - B[y] - W[placing]
        //
        // Correct setup for breakable five:
        // _ W B B B B B W _
        // 0 1 2 3 4 5 6 7 8
        // White at 1 and 7. If White places at 0: W[0]-B[2]-B[3] needs W at 4? No.
        //
        // Actually: X-O-O-X pattern requires:
        // placing_stone - opponent - opponent - existing_own_stone
        //
        // For White to capture B-B from the five:
        // Need: W(placing) - B - B - W(existing)
        //
        // Setup: W _ B B B B B W _
        //        0 1 2 3 4 5 6 7 8
        // If White places at 8: W[8] - B[6] - B[5] - W[need at 4]
        // We need White at position 4 too!
        //
        // Correct breakable five setup:
        // _ _ B B W B B _ _
        // 0 1 2 3 4 5 6 7 8
        // This is NOT 5 in a row anymore.
        //
        // Let's try a different approach - White can break with capture from the side
        // W _ B B B B B _ _
        // 0 1 2 3 4 5 6 7 8
        // And add another W at distance 3 from position 1:
        // W _ B B W B B _ _  (W at 0 and 4, but now B's are split)
        //
        // Better approach: Make 5 blacks, with setup allowing capture of edge pair
        // _ W B B B B B W _
        // 0 1 2 3 4 5 6 7 8
        // Add White at position 4 (inside the line? No that breaks the 5)
        //
        // Working scenario: Create 5-in-row where capture can hit 2 stones
        // B B B B B with W _ W pattern around 2 of them
        //
        // Row 9: B B B B B (cols 5-9)
        // Row 8, col 7: W (above col 7)
        // Row 10, col 7: empty (below col 7, for White to place)
        // Row 11, col 7: W (creates W-B-B-W vertically if place at row 10)
        //
        // Vertical capture through horizontal five:
        //         col: 5 6 7 8 9
        // row 8:       . . W . .
        // row 9:       B B B B B  (five in a row)
        // row 10:      . . _ . .  (White places here)
        // row 11:      . . W . .
        //
        // White places at (10, 7): checks W[10,7] - B[9,7] - B[8,7] - W[need]
        // Actually capture is: placed - opp+1 - opp+2 - own+3
        // So: W[10,7] - B[9,7](wrong, need opponent in +1 direction)
        //
        // Going upward: dr=-1, so positions are:
        // placed: (10,7), +1: (9,7)=B, +2: (8,7)=W (not opponent!)
        //
        // Need to place White so that two B's are between two W's
        // Pattern in one direction: W(place) - B - B - W(exist)
        //
        // Fix: Put White at row 7, then place at row 10
        //         col: 5 6 7 8 9
        // row 7:       . . W . .
        // row 8:       . . B . .  <- extra black to make the vertical capture work
        // row 9:       B B B B B  (five in a row, col 7 is part of it)
        // row 10:      . . _ . .  (White places here)
        //
        // Wait, that's only 1 black between the W's, not 2.
        //
        // Let me reconsider the capture rule:
        // Pattern: placed_stone(pos) - opp(+1) - opp(+2) - our_stone(+3)
        //
        // For row-based vertical capture going upward (dr=-1):
        // White places at (10, 7)
        // Check: (10,7)+(-1,0)*1 = (9,7) should be Black
        // Check: (10,7)+(-1,0)*2 = (8,7) should be Black
        // Check: (10,7)+(-1,0)*3 = (7,7) should be White
        //
        // So we need:
        // row 7, col 7: W
        // row 8, col 7: B
        // row 9, col 7: B (part of the five)
        // row 10, col 7: _ (White places here)
        //
        // But now the five at row 9 cols 5-9 includes (9,7)
        // We also need (8,7) to be Black, but that's outside the five
        //
        // Actually, the five is: (9,5), (9,6), (9,7), (9,8), (9,9)
        // The capture would take (9,7) and (8,7), but (8,7) is not in the five!
        // So this would NOT break the five (only removes 1 stone from it).
        //
        // For capture to break the five, BOTH captured stones must be IN the five.
        // So we need to capture a PAIR within the five (horizontal captures).
        //
        // Horizontal capture of pair within the five:
        // Row 9: W _ B B B B B _ W
        //        0 1 2 3 4 5 6 7 8
        //
        // To capture (9,2) and (9,3), White at (9,1) needs W at (9,4)
        // But (9,4) is Black in the five!
        //
        // This means: A CONTINUOUS five cannot be broken by capture horizontally!
        // The only way to break is via perpendicular capture through 2 adjacent stones.
        //
        // Perpendicular capture setup:
        // We need to capture 2 ADJACENT blacks that are both in the five.
        // That means vertical/diagonal capture through 2 horizontally adjacent cells.
        //
        // But a vertical capture through row 9 would be:
        // W at (7, x), B at (8, x), B at (9, x) <- in five, W places at (10, x)
        // This only captures 1 from the five + 1 outside.
        //
        // Actually wait - let me reread. The capture is along a LINE.
        // So to capture 2 blacks from a horizontal five, the capture must be horizontal too!
        //
        // For horizontal capture W-B-B-W, the B-B must be adjacent horizontally.
        // In a continuous five B-B-B-B-B, every adjacent pair is surrounded by more B's!
        // B[B-B]B-B -- the brackets show a pair, but it's B-B-B, not W-B-B-W
        //
        // Conclusion: A CONTINUOUS five-in-a-row CANNOT be broken by capture!
        // The test scenario is fundamentally flawed.
        //
        // For a breakable five, we need a GAPPED five or specific board setup.
        // But Gomoku five-in-row means CONSECUTIVE stones, so no gaps.
        //
        // Actually re-reading the endgame rule: it says if opponent CAN capture
        // to break the five. Let me check if this is even possible...
        //
        // Alternative interpretation: Maybe the rule applies to "about to complete 5"
        // rather than "already completed 5"? Or the five is broken by removing
        // ANY stone from it (not necessarily via a horizontal capture through it).
        //
        // Looking at it differently: We capture 2 stones. If either of them is
        // part of the five, the five is broken (reduced to 4).
        //
        // So capturing (9,7) and (8,7) where (9,7) is in the five WOULD break it!
        //
        // Let's set up that scenario:
        // Row 9: B B B B B (cols 5-9, the five)
        // Row 7, col 7: W
        // Row 8, col 7: B (additional black for capture)
        // Row 10, col 7: _ (White places here to capture (8,7) and (9,7))
        //
        board.place_stone(Pos::new(7, 7), Stone::White); // W at top
        for i in 5..10 {
            board.place_stone(Pos::new(9, i), Stone::Black); // Five in a row
        }
        board.place_stone(Pos::new(8, 7), Stone::Black); // Extra B for capture

        // Now if White places at (10, 7), captures (8,7) and (9,7)
        // (9,7) is part of the five, so the five would be broken

        let five = find_five_positions(&board, Stone::Black).unwrap();
        let can_break = can_break_five_by_capture(&board, &five, Stone::Black);
        assert!(can_break, "White should be able to break the five by capturing (8,7)-(9,7)");
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
