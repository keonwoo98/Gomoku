use super::*;
use super::bitboard::Bitboard;
use super::board::Board;

#[test]
fn test_stone_opponent() {
    assert_eq!(Stone::Black.opponent(), Stone::White);
    assert_eq!(Stone::White.opponent(), Stone::Black);
    assert_eq!(Stone::Empty.opponent(), Stone::Empty);
}

#[test]
fn test_pos_new() {
    let pos = Pos::new(9, 9);
    assert_eq!(pos.row, 9);
    assert_eq!(pos.col, 9);
}

#[test]
fn test_pos_conversion() {
    let pos = Pos::new(9, 9); // Center
    assert_eq!(pos.to_index(), 9 * 19 + 9);
    assert_eq!(pos.to_index(), 180);

    let pos2 = Pos::from_index(180);
    assert_eq!(pos2.row, 9);
    assert_eq!(pos2.col, 9);
}

#[test]
fn test_pos_validity() {
    assert!(Pos::is_valid(0, 0));
    assert!(Pos::is_valid(18, 18));
    assert!(Pos::is_valid(9, 9));
    assert!(!Pos::is_valid(-1, 0));
    assert!(!Pos::is_valid(0, -1));
    assert!(!Pos::is_valid(19, 0));
    assert!(!Pos::is_valid(0, 19));
}

#[test]
fn test_board_constants() {
    assert_eq!(BOARD_SIZE, 19);
    assert_eq!(TOTAL_CELLS, 361);
}

#[test]
fn test_pos_ordering() {
    let pos1 = Pos::new(0, 0);
    let pos2 = Pos::new(0, 1);
    let pos3 = Pos::new(1, 0);
    
    assert!(pos1 < pos2);
    assert!(pos2 < pos3);
    assert!(pos1 < pos3);
}

#[test]
fn test_pos_corner_indices() {
    // Top-left
    assert_eq!(Pos::new(0, 0).to_index(), 0);
    // Top-right
    assert_eq!(Pos::new(0, 18).to_index(), 18);
    // Bottom-left
    assert_eq!(Pos::new(18, 0).to_index(), 342);
    // Bottom-right
    assert_eq!(Pos::new(18, 18).to_index(), 360);
}

// Bitboard tests

#[test]
fn test_bitboard_new() {
    let bb = Bitboard::new();
    assert!(bb.is_empty());
    assert_eq!(bb.count(), 0);
}

#[test]
fn test_bitboard_set_get() {
    let mut bb = Bitboard::new();
    let pos = Pos::new(9, 9);

    assert!(!bb.get(pos));
    bb.set(pos);
    assert!(bb.get(pos));
    assert_eq!(bb.count(), 1);
}

#[test]
fn test_bitboard_clear() {
    let mut bb = Bitboard::new();
    let pos = Pos::new(9, 9);

    bb.set(pos);
    assert!(bb.get(pos));
    bb.clear(pos);
    assert!(!bb.get(pos));
    assert_eq!(bb.count(), 0);
}

#[test]
fn test_bitboard_multiple_positions() {
    let mut bb = Bitboard::new();

    bb.set(Pos::new(0, 0));
    bb.set(Pos::new(9, 9));
    bb.set(Pos::new(18, 18));

    assert!(bb.get(Pos::new(0, 0)));
    assert!(bb.get(Pos::new(9, 9)));
    assert!(bb.get(Pos::new(18, 18)));
    assert!(!bb.get(Pos::new(5, 5)));
    assert_eq!(bb.count(), 3);
}

#[test]
fn test_bitboard_iter() {
    let mut bb = Bitboard::new();
    bb.set(Pos::new(0, 0));
    bb.set(Pos::new(5, 5));
    bb.set(Pos::new(10, 10));

    let positions: Vec<Pos> = bb.iter_ones().collect();
    assert_eq!(positions.len(), 3);
    assert!(positions.contains(&Pos::new(0, 0)));
    assert!(positions.contains(&Pos::new(5, 5)));
    assert!(positions.contains(&Pos::new(10, 10)));
}

#[test]
fn test_bitboard_word_boundaries() {
    let mut bb = Bitboard::new();
    
    // Test positions at word boundaries (64 bits each)
    // Word 0: indices 0-63
    // Word 1: indices 64-127
    // etc.
    bb.set(Pos::from_index(63));  // End of word 0
    bb.set(Pos::from_index(64));  // Start of word 1
    bb.set(Pos::from_index(127)); // End of word 1
    bb.set(Pos::from_index(128)); // Start of word 2

    assert!(bb.get(Pos::from_index(63)));
    assert!(bb.get(Pos::from_index(64)));
    assert!(bb.get(Pos::from_index(127)));
    assert!(bb.get(Pos::from_index(128)));
    assert_eq!(bb.count(), 4);
}

// Board tests

#[test]
fn test_board_new() {
    let board = Board::new();
    assert_eq!(board.stone_count(), 0);
    assert!(board.is_board_empty());
}

#[test]
fn test_board_place_get() {
    let mut board = Board::new();
    let pos = Pos::new(9, 9);

    assert_eq!(board.get(pos), Stone::Empty);
    board.place_stone(pos, Stone::Black);
    assert_eq!(board.get(pos), Stone::Black);

    board.remove_stone(pos);
    assert_eq!(board.get(pos), Stone::Empty);
}

#[test]
fn test_board_captures() {
    let mut board = Board::new();
    assert_eq!(board.captures(Stone::Black), 0);

    board.add_captures(Stone::Black, 2);
    assert_eq!(board.captures(Stone::Black), 2);
    assert_eq!(board.captures(Stone::White), 0);
}

#[test]
fn test_board_stone_count() {
    let mut board = Board::new();
    board.place_stone(Pos::new(0, 0), Stone::Black);
    board.place_stone(Pos::new(1, 1), Stone::White);
    board.place_stone(Pos::new(2, 2), Stone::Black);

    assert_eq!(board.stone_count(), 3);
    assert!(!board.is_board_empty());
}
