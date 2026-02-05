use super::*;

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
