# Rust Gomoku AI 엔진 구현 계획

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ninuki-renju 규칙을 지원하는 강력한 Gomoku AI 엔진을 Rust로 구현한다. VCF/VCT 위협 탐색으로 필승 수순을 찾고, 캡처 방어를 통합하여 절대 지지 않는 AI를 만든다.

**Architecture:**
- Board를 비트보드로 표현하여 O(1) 패턴 매칭
- VCF → VCT → 상대 VCF/VCT 방어 → Minimax 순서로 탐색
- Zobrist 해싱 + Transposition Table로 중복 계산 방지
- 캡처 규칙을 모든 탐색/평가에 통합

**Tech Stack:** Rust 1.75+, Cargo, SDL2 (GUI)

**핵심 요구사항:**
- 깊이 10 이상, 평균 0.5초 이내
- 캡처 규칙 (X-O-O-X 패턴으로 O-O 제거)
- 5목 완성 시 캡처로 깰 수 있으면 게임 계속
- 쌍삼(더블-프리-쓰리) 금지 (캡처로 만드는 건 허용)
- 10개 캡처 승리

---

## Phase 1: 프로젝트 설정 및 기본 구조

### Task 1.1: Rust 프로젝트 초기화

**Files:**
- Create: `engine/Cargo.toml`
- Create: `engine/src/lib.rs`
- Create: `engine/src/main.rs`
- Modify: `Makefile`

**Step 1: Cargo 프로젝트 생성**

```bash
cd /Users/keokim/42/Gomoku
cargo new engine --name gomoku_engine
```

**Step 2: Cargo.toml 설정**

```toml
[package]
name = "gomoku_engine"
version = "0.1.0"
edition = "2021"

[lib]
name = "gomoku"
path = "src/lib.rs"

[[bin]]
name = "gomoku"
path = "src/main.rs"

[dependencies]
# GUI는 나중에 추가

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

**Step 3: 기본 lib.rs 작성**

```rust
//! Gomoku AI Engine with Ninuki-renju rules
//!
//! # Features
//! - VCF/VCT threat search
//! - Capture rules (pair capture)
//! - Double-three forbidden move detection

pub mod board;
pub mod rules;
pub mod eval;
pub mod search;
pub mod threat;
```

**Step 4: 기본 main.rs 작성**

```rust
use gomoku::board::Board;

fn main() {
    println!("Gomoku Engine v0.1.0");
    let board = Board::new();
    println!("Board initialized: {}x{}", board.size(), board.size());
}
```

**Step 5: 빌드 테스트**

```bash
cd engine && cargo build --release
```
Expected: Compilation successful

**Step 6: Makefile 수정**

```makefile
NAME = Gomoku

all: $(NAME)

$(NAME):
	@cd engine && cargo build --release
	@cp engine/target/release/gomoku $(NAME)

clean:
	@cd engine && cargo clean

fclean: clean
	@rm -f $(NAME)

re: fclean all

.PHONY: all clean fclean re
```

**Step 7: Commit**

```bash
git add engine/ Makefile
git commit -m "feat: initialize Rust Gomoku engine project"
```

---

### Task 1.2: Board 모듈 - 기본 구조

**Files:**
- Create: `engine/src/board.rs`
- Create: `engine/src/board/mod.rs`
- Create: `engine/src/board/bitboard.rs`
- Test: `engine/src/board/tests.rs`

**Step 1: 상수 정의**

```rust
// engine/src/board/mod.rs

pub mod bitboard;

#[cfg(test)]
mod tests;

/// Board size (19x19)
pub const BOARD_SIZE: usize = 19;
pub const TOTAL_CELLS: usize = BOARD_SIZE * BOARD_SIZE; // 361

/// Stone colors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stone {
    Empty,
    Black,
    White,
}

impl Stone {
    /// Get opponent color
    #[inline]
    pub fn opponent(self) -> Stone {
        match self {
            Stone::Black => Stone::White,
            Stone::White => Stone::Black,
            Stone::Empty => Stone::Empty,
        }
    }
}

/// Position on the board
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pos {
    pub row: u8,
    pub col: u8,
}

impl Pos {
    #[inline]
    pub fn new(row: u8, col: u8) -> Self {
        debug_assert!(row < BOARD_SIZE as u8 && col < BOARD_SIZE as u8);
        Self { row, col }
    }

    #[inline]
    pub fn to_index(self) -> usize {
        self.row as usize * BOARD_SIZE + self.col as usize
    }

    #[inline]
    pub fn from_index(idx: usize) -> Self {
        Self {
            row: (idx / BOARD_SIZE) as u8,
            col: (idx % BOARD_SIZE) as u8,
        }
    }

    #[inline]
    pub fn is_valid(row: i32, col: i32) -> bool {
        row >= 0 && row < BOARD_SIZE as i32 && col >= 0 && col < BOARD_SIZE as i32
    }
}
```

**Step 2: 테스트 작성**

```rust
// engine/src/board/tests.rs

use super::*;

#[test]
fn test_stone_opponent() {
    assert_eq!(Stone::Black.opponent(), Stone::White);
    assert_eq!(Stone::White.opponent(), Stone::Black);
    assert_eq!(Stone::Empty.opponent(), Stone::Empty);
}

#[test]
fn test_pos_conversion() {
    let pos = Pos::new(9, 9); // Center
    assert_eq!(pos.to_index(), 9 * 19 + 9);

    let pos2 = Pos::from_index(180);
    assert_eq!(pos2.row, 9);
    assert_eq!(pos2.col, 9);
}

#[test]
fn test_pos_validity() {
    assert!(Pos::is_valid(0, 0));
    assert!(Pos::is_valid(18, 18));
    assert!(!Pos::is_valid(-1, 0));
    assert!(!Pos::is_valid(19, 0));
}
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test board::tests
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/board/
git commit -m "feat(board): add basic types - Stone, Pos, constants"
```

---

### Task 1.3: Board 모듈 - 비트보드 구현

**Files:**
- Modify: `engine/src/board/bitboard.rs`
- Modify: `engine/src/board/tests.rs`

**Step 1: 비트보드 구조체**

```rust
// engine/src/board/bitboard.rs

use super::{BOARD_SIZE, TOTAL_CELLS, Stone, Pos};

/// Bitboard representation for fast pattern matching
/// Uses 6 x u64 to represent 361 cells (6 * 64 = 384 >= 361)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bitboard {
    bits: [u64; 6],
}

impl Bitboard {
    pub const fn new() -> Self {
        Self { bits: [0; 6] }
    }

    /// Set a bit at position
    #[inline]
    pub fn set(&mut self, pos: Pos) {
        let idx = pos.to_index();
        let word = idx / 64;
        let bit = idx % 64;
        self.bits[word] |= 1u64 << bit;
    }

    /// Clear a bit at position
    #[inline]
    pub fn clear(&mut self, pos: Pos) {
        let idx = pos.to_index();
        let word = idx / 64;
        let bit = idx % 64;
        self.bits[word] &= !(1u64 << bit);
    }

    /// Check if bit is set at position
    #[inline]
    pub fn get(&self, pos: Pos) -> bool {
        let idx = pos.to_index();
        let word = idx / 64;
        let bit = idx % 64;
        (self.bits[word] >> bit) & 1 == 1
    }

    /// Count total set bits (popcount)
    #[inline]
    pub fn count(&self) -> u32 {
        self.bits.iter().map(|b| b.count_ones()).sum()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&b| b == 0)
    }

    /// Iterate over set bit positions
    pub fn iter_ones(&self) -> impl Iterator<Item = Pos> + '_ {
        self.bits.iter().enumerate().flat_map(|(word_idx, &word)| {
            let base = word_idx * 64;
            BitIter::new(word).map(move |bit| {
                let idx = base + bit as usize;
                if idx < TOTAL_CELLS {
                    Some(Pos::from_index(idx))
                } else {
                    None
                }
            }).flatten()
        })
    }
}

/// Iterator over set bits in a u64
struct BitIter {
    bits: u64,
}

impl BitIter {
    fn new(bits: u64) -> Self {
        Self { bits }
    }
}

impl Iterator for BitIter {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.bits == 0 {
            return None;
        }
        let pos = self.bits.trailing_zeros() as u8;
        self.bits &= self.bits - 1; // Clear lowest set bit
        Some(pos)
    }
}
```

**Step 2: 비트보드 테스트 추가**

```rust
// engine/src/board/tests.rs에 추가

use super::bitboard::Bitboard;

#[test]
fn test_bitboard_set_get() {
    let mut bb = Bitboard::new();
    let pos = Pos::new(9, 9);

    assert!(!bb.get(pos));
    bb.set(pos);
    assert!(bb.get(pos));
    bb.clear(pos);
    assert!(!bb.get(pos));
}

#[test]
fn test_bitboard_count() {
    let mut bb = Bitboard::new();
    assert_eq!(bb.count(), 0);

    bb.set(Pos::new(0, 0));
    bb.set(Pos::new(9, 9));
    bb.set(Pos::new(18, 18));
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
}
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test bitboard
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/board/
git commit -m "feat(board): implement Bitboard with fast bit operations"
```

---

### Task 1.4: Board 모듈 - Board 구조체

**Files:**
- Create: `engine/src/board/board.rs`
- Modify: `engine/src/board/mod.rs`
- Modify: `engine/src/board/tests.rs`

**Step 1: Board 구조체 구현**

```rust
// engine/src/board/board.rs

use super::{BOARD_SIZE, Stone, Pos};
use super::bitboard::Bitboard;

/// Game board with capture tracking
#[derive(Debug, Clone)]
pub struct Board {
    /// Black stones bitboard
    pub black: Bitboard,
    /// White stones bitboard
    pub white: Bitboard,
    /// Number of pairs captured by each side (0-5, 5 = win)
    pub black_captures: u8,
    pub white_captures: u8,
    /// Move history for undo
    history: Vec<MoveRecord>,
}

/// Record of a move for undo functionality
#[derive(Debug, Clone)]
struct MoveRecord {
    pos: Pos,
    stone: Stone,
    captured: Vec<Pos>, // Positions of captured stones
}

impl Board {
    pub fn new() -> Self {
        Self {
            black: Bitboard::new(),
            white: Bitboard::new(),
            black_captures: 0,
            white_captures: 0,
            history: Vec::with_capacity(361),
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        BOARD_SIZE
    }

    /// Get stone at position
    #[inline]
    pub fn get(&self, pos: Pos) -> Stone {
        if self.black.get(pos) {
            Stone::Black
        } else if self.white.get(pos) {
            Stone::White
        } else {
            Stone::Empty
        }
    }

    /// Check if position is empty
    #[inline]
    pub fn is_empty(&self, pos: Pos) -> bool {
        !self.black.get(pos) && !self.white.get(pos)
    }

    /// Place a stone (without capture processing)
    /// Use `make_move` for game moves
    #[inline]
    pub fn place_stone(&mut self, pos: Pos, stone: Stone) {
        match stone {
            Stone::Black => self.black.set(pos),
            Stone::White => self.white.set(pos),
            Stone::Empty => {}
        }
    }

    /// Remove a stone
    #[inline]
    pub fn remove_stone(&mut self, pos: Pos) {
        self.black.clear(pos);
        self.white.clear(pos);
    }

    /// Get bitboard for a color
    #[inline]
    pub fn stones(&self, stone: Stone) -> &Bitboard {
        match stone {
            Stone::Black => &self.black,
            Stone::White => &self.white,
            Stone::Empty => panic!("Cannot get bitboard for Empty"),
        }
    }

    /// Get mutable bitboard for a color
    #[inline]
    pub fn stones_mut(&mut self, stone: Stone) -> &mut Bitboard {
        match stone {
            Stone::Black => &mut self.black,
            Stone::White => &mut self.white,
            Stone::Empty => panic!("Cannot get bitboard for Empty"),
        }
    }

    /// Get capture count for a color
    #[inline]
    pub fn captures(&self, stone: Stone) -> u8 {
        match stone {
            Stone::Black => self.black_captures,
            Stone::White => self.white_captures,
            Stone::Empty => 0,
        }
    }

    /// Add captures for a color
    #[inline]
    pub fn add_captures(&mut self, stone: Stone, count: u8) {
        match stone {
            Stone::Black => self.black_captures += count,
            Stone::White => self.white_captures += count,
            Stone::Empty => {}
        }
    }

    /// Total stones on board
    #[inline]
    pub fn stone_count(&self) -> u32 {
        self.black.count() + self.white.count()
    }

    /// Check if board is empty
    #[inline]
    pub fn is_board_empty(&self) -> bool {
        self.black.is_empty() && self.white.is_empty()
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Board 테스트 추가**

```rust
// engine/src/board/tests.rs에 추가

use super::board::Board;

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
}
```

**Step 3: mod.rs 업데이트**

```rust
// engine/src/board/mod.rs 맨 위에 추가

pub mod bitboard;
pub mod board;

#[cfg(test)]
mod tests;

// Re-exports
pub use self::board::Board;
pub use self::bitboard::Bitboard;

// ... 기존 Stone, Pos 코드 ...
```

**Step 4: 테스트 실행**

```bash
cd engine && cargo test board
```
Expected: All tests pass

**Step 5: Commit**

```bash
git add engine/src/board/
git commit -m "feat(board): implement Board struct with stone placement"
```

---

## Phase 2: 규칙 모듈 (Rules)

### Task 2.1: 캡처 규칙 구현

**Files:**
- Create: `engine/src/rules/mod.rs`
- Create: `engine/src/rules/capture.rs`
- Modify: `engine/src/lib.rs`

**Step 1: 캡처 로직 구현**

```rust
// engine/src/rules/capture.rs

use crate::board::{Board, Stone, Pos, BOARD_SIZE};

/// Direction vectors for capture checking
const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1),   // Horizontal
    (1, 0),   // Vertical
    (1, 1),   // Diagonal ↘
    (1, -1),  // Diagonal ↗
];

/// Find positions that would be captured if stone is placed at pos
/// Capture pattern: X-O-O-X where X is the placed stone
pub fn get_captured_positions(board: &Board, pos: Pos, stone: Stone) -> Vec<Pos> {
    let mut captured = Vec::new();
    let opponent = stone.opponent();

    for &(dr, dc) in &DIRECTIONS {
        // Check both directions along this line
        for sign in [-1i32, 1i32] {
            let dr = dr * sign;
            let dc = dc * sign;

            // Pattern: placed_stone - opp - opp - our_stone
            // Positions: pos, pos+1, pos+2, pos+3
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

/// Execute captures and return captured positions
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_horizontal() {
        let mut board = Board::new();
        // Setup: B _ W W B  (B places at _, captures W W)
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let captured = get_captured_positions(&board, Pos::new(9, 6), Stone::Black);
        assert_eq!(captured.len(), 2);
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
}
```

**Step 2: rules/mod.rs 생성**

```rust
// engine/src/rules/mod.rs

pub mod capture;
pub mod win;
pub mod forbidden;

pub use capture::{get_captured_positions, execute_captures};
```

**Step 3: lib.rs 업데이트**

```rust
// engine/src/lib.rs

pub mod board;
pub mod rules;
```

**Step 4: 테스트 실행**

```bash
cd engine && cargo test capture
```
Expected: All tests pass

**Step 5: Commit**

```bash
git add engine/src/rules/ engine/src/lib.rs
git commit -m "feat(rules): implement capture rules (pair capture)"
```

---

### Task 2.2: 승리 조건 구현

**Files:**
- Create: `engine/src/rules/win.rs`
- Modify: `engine/src/rules/mod.rs`

**Step 1: 5목 검사 및 승리 조건**

```rust
// engine/src/rules/win.rs

use crate::board::{Board, Stone, Pos, BOARD_SIZE};
use super::capture::get_captured_positions;

const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1), (1, 0), (1, 1), (1, -1),
];

/// Check if there's 5+ in a row for the given color
pub fn has_five_in_row(board: &Board, stone: Stone) -> bool {
    find_five_positions(board, stone).is_some()
}

/// Find the positions of a 5-in-a-row if exists
pub fn find_five_positions(board: &Board, stone: Stone) -> Option<Vec<Pos>> {
    let stones = board.stones(stone);

    for pos in stones.iter_ones() {
        for &(dr, dc) in &DIRECTIONS {
            let mut line = vec![pos];

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
/// Returns true if the 5-in-row can be broken
pub fn can_break_five_by_capture(
    board: &Board,
    five_positions: &[Pos],
    five_color: Stone,
) -> bool {
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
/// Returns Some(Stone) if there's a winner, None otherwise
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
    fn test_five_in_row_diagonal() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(i as u8, i as u8), Stone::White);
        }
        assert!(has_five_in_row(&board, Stone::White));
    }

    #[test]
    fn test_capture_win() {
        let mut board = Board::new();
        board.add_captures(Stone::Black, 5);
        assert_eq!(check_winner(&board), Some(Stone::Black));
    }

    #[test]
    fn test_breakable_five() {
        let mut board = Board::new();
        // Setup: 5 blacks in a row, but white can capture 2 of them
        // W _ B B B B B _ W
        // 0 1 2 3 4 5 6 7 8
        board.place_stone(Pos::new(9, 0), Stone::White);
        for i in 2..7 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }
        board.place_stone(Pos::new(9, 8), Stone::White);

        let five = find_five_positions(&board, Stone::Black).unwrap();
        // White can place at 9,1 or 9,7 to capture
        let can_break = can_break_five_by_capture(&board, &five, Stone::Black);
        assert!(can_break);
    }
}
```

**Step 2: mod.rs 업데이트**

```rust
// engine/src/rules/mod.rs

pub mod capture;
pub mod win;
pub mod forbidden;

pub use capture::{get_captured_positions, execute_captures};
pub use win::{has_five_in_row, find_five_positions, can_break_five_by_capture, check_winner};
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test win
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/rules/
git commit -m "feat(rules): implement win conditions with endgame capture rule"
```

---

### Task 2.3: 쌍삼 금지 규칙 구현

**Files:**
- Create: `engine/src/rules/forbidden.rs`
- Modify: `engine/src/rules/mod.rs`

**Step 1: 쌍삼(더블-프리-쓰리) 검사**

```rust
// engine/src/rules/forbidden.rs

use crate::board::{Board, Stone, Pos, BOARD_SIZE};
use super::capture::get_captured_positions;

const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1), (1, 0), (1, 1), (1, -1),
];

/// Check if placing stone at pos creates a free-three in the given direction
/// Free-three: 3 stones that can become an unstoppable open-four
fn creates_free_three(board: &Board, pos: Pos, stone: Stone, dr: i32, dc: i32) -> bool {
    // Temporarily place the stone
    let mut count = 1; // The stone we're placing
    let mut open_ends = 0;
    let mut space_in_line = false;

    // Check positive direction
    let mut consecutive_pos = 0;
    let mut gap_pos = false;
    for i in 1..5 {
        let r = pos.row as i32 + dr * i;
        let c = pos.col as i32 + dc * i;
        if !Pos::is_valid(r, c) {
            break;
        }
        let check_pos = Pos::new(r as u8, c as u8);
        let cell = board.get(check_pos);

        if cell == stone {
            consecutive_pos += 1;
            count += 1;
        } else if cell == Stone::Empty {
            if consecutive_pos > 0 && !gap_pos {
                // Found gap in the middle: _OO_O pattern
                gap_pos = true;
            } else {
                open_ends += 1;
                break;
            }
        } else {
            // Opponent stone - blocked
            break;
        }
    }

    // Check negative direction
    let mut consecutive_neg = 0;
    let mut gap_neg = false;
    for i in 1..5 {
        let r = pos.row as i32 - dr * i;
        let c = pos.col as i32 - dc * i;
        if !Pos::is_valid(r, c) {
            break;
        }
        let check_pos = Pos::new(r as u8, c as u8);
        let cell = board.get(check_pos);

        if cell == stone {
            consecutive_neg += 1;
            count += 1;
        } else if cell == Stone::Empty {
            if consecutive_neg > 0 && !gap_neg {
                gap_neg = true;
            } else {
                open_ends += 1;
                break;
            }
        } else {
            break;
        }
    }

    // Free-three: exactly 3 stones with both ends open
    count == 3 && open_ends >= 2
}

/// Count how many free-threes would be created by placing stone at pos
pub fn count_free_threes(board: &Board, pos: Pos, stone: Stone) -> u8 {
    let mut count = 0;

    // Place stone temporarily to check patterns
    let mut temp_board = board.clone();
    temp_board.place_stone(pos, stone);

    for &(dr, dc) in &DIRECTIONS {
        if creates_free_three(&temp_board, pos, stone, dr, dc) {
            count += 1;
        }
    }

    count
}

/// Check if move is a double-three (forbidden)
/// Exception: double-three is allowed if the move captures opponent stones
pub fn is_double_three(board: &Board, pos: Pos, stone: Stone) -> bool {
    // Exception: if this move captures, double-three is allowed
    let captures = get_captured_positions(board, pos, stone);
    if !captures.is_empty() {
        return false;
    }

    count_free_threes(board, pos, stone) >= 2
}

/// Check if a move is valid (not forbidden)
pub fn is_valid_move(board: &Board, pos: Pos, stone: Stone) -> bool {
    // Must be empty
    if !board.is_empty(pos) {
        return false;
    }

    // Must not be double-three (unless capture)
    if is_double_three(board, pos, stone) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_double_three_simple() {
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
}
```

**Step 2: mod.rs 업데이트**

```rust
// engine/src/rules/mod.rs

pub mod capture;
pub mod win;
pub mod forbidden;

pub use capture::{get_captured_positions, execute_captures};
pub use win::{has_five_in_row, find_five_positions, can_break_five_by_capture, check_winner};
pub use forbidden::{is_double_three, is_valid_move, count_free_threes};
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test forbidden
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/rules/
git commit -m "feat(rules): implement double-three forbidden rule with capture exception"
```

---

## Phase 3: 평가 함수 (Evaluation)

### Task 3.1: 패턴 점수 상수 정의

**Files:**
- Create: `engine/src/eval/mod.rs`
- Create: `engine/src/eval/patterns.rs`

**Step 1: 패턴 점수 상수**

```rust
// engine/src/eval/patterns.rs

/// Pattern scores for evaluation
/// These are carefully tuned for strong play
pub struct PatternScore;

impl PatternScore {
    // Winning patterns
    pub const FIVE: i32 = 1_000_000;
    pub const CAPTURE_WIN: i32 = 1_000_000;

    // Strong attacking patterns
    pub const OPEN_FOUR: i32 = 100_000;      // _OOOO_ (unstoppable)
    pub const CLOSED_FOUR: i32 = 50_000;     // XOOOO_ or _OOOOX

    // Moderate threats
    pub const OPEN_THREE: i32 = 10_000;      // _OOO_
    pub const CLOSED_THREE: i32 = 1_000;     // XOOO_ or _OOOX

    // Building patterns
    pub const OPEN_TWO: i32 = 500;           // _OO_
    pub const CLOSED_TWO: i32 = 50;          // XOO_ or _OOX

    // Capture related
    pub const CAPTURE_THREAT: i32 = 3_000;   // Can capture next move
    pub const CAPTURE_PAIR: i32 = 500;       // Per captured pair
    pub const NEAR_CAPTURE_WIN: i32 = 8_000; // 4 pairs captured (one more = win)

    // Defense weights
    pub const DEFENSE_MULTIPLIER: f32 = 1.5; // Defense is weighted higher
}

/// Capture-based scoring
pub fn capture_score(my_captures: u8, opp_captures: u8) -> i32 {
    // Non-linear scoring - closer to win = exponentially more valuable
    const CAP_WEIGHTS: [i32; 6] = [0, 200, 600, 2000, 8000, PatternScore::CAPTURE_WIN];

    let my_score = CAP_WEIGHTS[my_captures.min(5) as usize];
    let opp_score = CAP_WEIGHTS[opp_captures.min(5) as usize];

    my_score - (opp_score as f32 * PatternScore::DEFENSE_MULTIPLIER) as i32
}
```

**Step 2: eval/mod.rs 생성**

```rust
// engine/src/eval/mod.rs

pub mod patterns;
pub mod heuristic;

pub use patterns::PatternScore;
```

**Step 3: lib.rs 업데이트**

```rust
// engine/src/lib.rs

pub mod board;
pub mod rules;
pub mod eval;
```

**Step 4: Commit**

```bash
git add engine/src/eval/ engine/src/lib.rs
git commit -m "feat(eval): add pattern score constants (no magic numbers)"
```

---

### Task 3.2: 휴리스틱 평가 함수

**Files:**
- Create: `engine/src/eval/heuristic.rs`
- Modify: `engine/src/eval/mod.rs`

**Step 1: 평가 함수 구현**

```rust
// engine/src/eval/heuristic.rs

use crate::board::{Board, Stone, Pos, BOARD_SIZE};
use crate::rules::{has_five_in_row, check_winner};
use super::patterns::{PatternScore, capture_score};

const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1), (1, 0), (1, 1), (1, -1),
];

/// Evaluate the board from the perspective of the given color
pub fn evaluate(board: &Board, color: Stone) -> i32 {
    let opponent = color.opponent();

    // Check for immediate win/loss
    if let Some(winner) = check_winner(board) {
        return if winner == color {
            PatternScore::FIVE
        } else {
            -PatternScore::FIVE
        };
    }

    // Capture score
    let cap_score = capture_score(board.captures(color), board.captures(opponent));

    // Pattern score
    let my_patterns = evaluate_patterns(board, color);
    let opp_patterns = evaluate_patterns(board, opponent);

    // Defense weighted higher
    let pattern_score = my_patterns - (opp_patterns as f32 * PatternScore::DEFENSE_MULTIPLIER) as i32;

    // Position score (center bonus)
    let position_score = evaluate_positions(board, color) - evaluate_positions(board, opponent);

    cap_score + pattern_score + position_score
}

/// Evaluate pattern-based score for a color
fn evaluate_patterns(board: &Board, color: Stone) -> i32 {
    let mut score = 0;
    let stones = board.stones(color);

    for pos in stones.iter_ones() {
        for &(dr, dc) in &DIRECTIONS {
            score += evaluate_line(board, pos, dr, dc, color);
        }
    }

    // Divide by 2 to avoid double counting (each line counted from both ends)
    score / 2
}

/// Evaluate a single line from a position
fn evaluate_line(board: &Board, pos: Pos, dr: i32, dc: i32, color: Stone) -> i32 {
    let opponent = color.opponent();
    let mut count = 1;
    let mut open_ends = 0;

    // Positive direction
    let mut r = pos.row as i32 + dr;
    let mut c = pos.col as i32 + dc;
    while Pos::is_valid(r, c) {
        let p = Pos::new(r as u8, c as u8);
        match board.get(p) {
            s if s == color => count += 1,
            Stone::Empty => {
                open_ends += 1;
                break;
            }
            _ => break, // Opponent
        }
        r += dr;
        c += dc;
    }

    // Negative direction
    r = pos.row as i32 - dr;
    c = pos.col as i32 - dc;
    while Pos::is_valid(r, c) {
        let p = Pos::new(r as u8, c as u8);
        match board.get(p) {
            s if s == color => count += 1,
            Stone::Empty => {
                open_ends += 1;
                break;
            }
            _ => break,
        }
        r -= dr;
        c -= dc;
    }

    // Score based on pattern
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

/// Evaluate position bonuses (center is better)
fn evaluate_positions(board: &Board, color: Stone) -> i32 {
    let center = (BOARD_SIZE / 2) as i32;
    let mut score = 0;

    for pos in board.stones(color).iter_ones() {
        let dist = (pos.row as i32 - center).abs() + (pos.col as i32 - center).abs();
        score += (18 - dist) * 3; // Max bonus at center
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
        assert_eq!(score, 0);
    }

    #[test]
    fn test_evaluate_center_bonus() {
        let mut board = Board::new();
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let score = evaluate(&board, Stone::Black);
        assert!(score > 0); // Center position is valuable
    }

    #[test]
    fn test_evaluate_winning_position() {
        let mut board = Board::new();
        for i in 0..5 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let score = evaluate(&board, Stone::Black);
        assert_eq!(score, PatternScore::FIVE);
    }
}
```

**Step 2: mod.rs 업데이트**

```rust
// engine/src/eval/mod.rs

pub mod patterns;
pub mod heuristic;

pub use patterns::PatternScore;
pub use heuristic::evaluate;
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test eval
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/eval/
git commit -m "feat(eval): implement heuristic evaluation with pattern scoring"
```

---

## Phase 4: 탐색 엔진 (Search)

### Task 4.1: Zobrist 해싱

**Files:**
- Create: `engine/src/search/mod.rs`
- Create: `engine/src/search/zobrist.rs`

**Step 1: Zobrist 해싱 구현**

```rust
// engine/src/search/zobrist.rs

use crate::board::{Board, Stone, Pos, BOARD_SIZE, TOTAL_CELLS};

/// Zobrist hash table for position hashing
pub struct ZobristTable {
    black: [u64; TOTAL_CELLS],
    white: [u64; TOTAL_CELLS],
    black_to_move: u64,
}

impl ZobristTable {
    /// Create new Zobrist table with deterministic random values
    pub fn new() -> Self {
        // Use a simple LCG for deterministic "random" values
        let mut seed: u64 = 0x12345678_9ABCDEF0;
        let mut next_rand = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };

        let mut black = [0u64; TOTAL_CELLS];
        let mut white = [0u64; TOTAL_CELLS];

        for i in 0..TOTAL_CELLS {
            black[i] = next_rand();
            white[i] = next_rand();
        }

        Self {
            black,
            white,
            black_to_move: next_rand(),
        }
    }

    /// Compute hash for a board position
    pub fn hash(&self, board: &Board, side_to_move: Stone) -> u64 {
        let mut h = 0u64;

        for pos in board.black.iter_ones() {
            h ^= self.black[pos.to_index()];
        }

        for pos in board.white.iter_ones() {
            h ^= self.white[pos.to_index()];
        }

        if side_to_move == Stone::Black {
            h ^= self.black_to_move;
        }

        h
    }

    /// Incrementally update hash after placing a stone
    #[inline]
    pub fn update_place(&self, hash: u64, pos: Pos, stone: Stone) -> u64 {
        let idx = pos.to_index();
        let stone_hash = match stone {
            Stone::Black => self.black[idx],
            Stone::White => self.white[idx],
            Stone::Empty => 0,
        };
        hash ^ stone_hash ^ self.black_to_move
    }

    /// Incrementally update hash after removing a stone
    #[inline]
    pub fn update_remove(&self, hash: u64, pos: Pos, stone: Stone) -> u64 {
        // XOR is its own inverse
        self.update_place(hash, pos, stone)
    }
}

impl Default for ZobristTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zobrist_empty_board() {
        let zt = ZobristTable::new();
        let board = Board::new();

        let hash1 = zt.hash(&board, Stone::Black);
        let hash2 = zt.hash(&board, Stone::White);

        // Different side to move = different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_zobrist_incremental() {
        let zt = ZobristTable::new();
        let mut board = Board::new();
        let pos = Pos::new(9, 9);

        let hash1 = zt.hash(&board, Stone::Black);
        board.place_stone(pos, Stone::Black);
        let hash2 = zt.hash(&board, Stone::White);

        // Incremental should match full computation
        let hash_inc = zt.update_place(hash1, pos, Stone::Black);
        assert_eq!(hash_inc, hash2);
    }
}
```

**Step 2: search/mod.rs 생성**

```rust
// engine/src/search/mod.rs

pub mod zobrist;
pub mod tt;
pub mod alphabeta;
pub mod threat;

pub use zobrist::ZobristTable;
```

**Step 3: lib.rs 업데이트**

```rust
// engine/src/lib.rs

pub mod board;
pub mod rules;
pub mod eval;
pub mod search;
```

**Step 4: 테스트 실행**

```bash
cd engine && cargo test zobrist
```
Expected: All tests pass

**Step 5: Commit**

```bash
git add engine/src/search/ engine/src/lib.rs
git commit -m "feat(search): implement Zobrist hashing for position caching"
```

---

### Task 4.2: Transposition Table

**Files:**
- Create: `engine/src/search/tt.rs`
- Modify: `engine/src/search/mod.rs`

**Step 1: TT 구현**

```rust
// engine/src/search/tt.rs

use crate::board::Pos;

/// Entry type for score interpretation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Exact,      // Exact score
    LowerBound, // Score >= stored (beta cutoff)
    UpperBound, // Score <= stored (alpha cutoff)
}

/// Transposition table entry
#[derive(Debug, Clone, Copy)]
pub struct TTEntry {
    pub hash: u64,
    pub depth: i8,
    pub score: i32,
    pub entry_type: EntryType,
    pub best_move: Option<Pos>,
}

/// Transposition table for caching search results
pub struct TranspositionTable {
    entries: Vec<Option<TTEntry>>,
    size: usize,
}

impl TranspositionTable {
    /// Create new TT with given size in MB
    pub fn new(size_mb: usize) -> Self {
        let entry_size = std::mem::size_of::<Option<TTEntry>>();
        let size = (size_mb * 1024 * 1024) / entry_size;

        Self {
            entries: vec![None; size],
            size,
        }
    }

    /// Probe the table for a position
    pub fn probe(&self, hash: u64, depth: i8, alpha: i32, beta: i32) -> Option<(i32, Option<Pos>)> {
        let idx = (hash as usize) % self.size;
        let entry = self.entries[idx]?;

        if entry.hash != hash {
            return None;
        }

        // Can use if stored search was at least as deep
        if entry.depth >= depth {
            match entry.entry_type {
                EntryType::Exact => return Some((entry.score, entry.best_move)),
                EntryType::LowerBound if entry.score >= beta => {
                    return Some((entry.score, entry.best_move))
                }
                EntryType::UpperBound if entry.score <= alpha => {
                    return Some((entry.score, entry.best_move))
                }
                _ => {}
            }
        }

        // Return best move for move ordering even if score not usable
        Some((0, entry.best_move))
    }

    /// Get best move from TT (for move ordering)
    pub fn get_best_move(&self, hash: u64) -> Option<Pos> {
        let idx = (hash as usize) % self.size;
        self.entries[idx].and_then(|e| {
            if e.hash == hash {
                e.best_move
            } else {
                None
            }
        })
    }

    /// Store a position in the table
    pub fn store(&mut self, hash: u64, depth: i8, score: i32, entry_type: EntryType, best_move: Option<Pos>) {
        let idx = (hash as usize) % self.size;

        // Replace if: empty, same position, or new search is deeper
        let should_replace = match &self.entries[idx] {
            None => true,
            Some(e) => e.hash == hash || e.depth <= depth,
        };

        if should_replace {
            self.entries[idx] = Some(TTEntry {
                hash,
                depth,
                score,
                entry_type,
                best_move,
            });
        }
    }

    /// Clear the table
    pub fn clear(&mut self) {
        self.entries.fill(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tt_store_probe() {
        let mut tt = TranspositionTable::new(1); // 1MB
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, EntryType::Exact, Some(Pos::new(9, 9)));

        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some());
        let (score, best_move) = result.unwrap();
        assert_eq!(score, 100);
        assert_eq!(best_move, Some(Pos::new(9, 9)));
    }

    #[test]
    fn test_tt_depth_requirement() {
        let mut tt = TranspositionTable::new(1);
        let hash = 0x123456789ABCDEF0;

        tt.store(hash, 3, 100, EntryType::Exact, None);

        // Deeper search should not use shallow entry
        let result = tt.probe(hash, 5, -1000, 1000);
        assert!(result.is_some()); // Still returns for move ordering
    }
}
```

**Step 2: mod.rs 업데이트**

```rust
// engine/src/search/mod.rs

pub mod zobrist;
pub mod tt;
pub mod alphabeta;
pub mod threat;

pub use zobrist::ZobristTable;
pub use tt::{TranspositionTable, TTEntry, EntryType};
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test tt
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/search/
git commit -m "feat(search): implement Transposition Table with depth-based replacement"
```

---

### Task 4.3: Alpha-Beta 탐색

**Files:**
- Create: `engine/src/search/alphabeta.rs`
- Modify: `engine/src/search/mod.rs`

**Step 1: Alpha-Beta 구현**

```rust
// engine/src/search/alphabeta.rs

use crate::board::{Board, Stone, Pos, BOARD_SIZE};
use crate::rules::{is_valid_move, check_winner, execute_captures};
use crate::eval::{evaluate, PatternScore};
use super::{ZobristTable, TranspositionTable, EntryType};

const INF: i32 = PatternScore::FIVE + 1;

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub best_move: Option<Pos>,
    pub score: i32,
    pub depth: i8,
    pub nodes: u64,
}

/// Alpha-Beta search engine
pub struct Searcher {
    zobrist: ZobristTable,
    tt: TranspositionTable,
    nodes: u64,
    max_depth: i8,
}

impl Searcher {
    pub fn new(tt_size_mb: usize) -> Self {
        Self {
            zobrist: ZobristTable::new(),
            tt: TranspositionTable::new(tt_size_mb),
            nodes: 0,
            max_depth: 10,
        }
    }

    /// Search for best move with iterative deepening
    pub fn search(&mut self, board: &Board, color: Stone, max_depth: i8) -> SearchResult {
        self.nodes = 0;
        self.max_depth = max_depth;

        let mut best_result = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
        };

        // Iterative deepening
        for depth in 1..=max_depth {
            let result = self.search_root(board, color, depth);
            best_result = result;
            best_result.depth = depth;

            // Early exit if found winning move
            if best_result.score >= PatternScore::FIVE - 100 {
                break;
            }
        }

        best_result.nodes = self.nodes;
        best_result
    }

    fn search_root(&mut self, board: &Board, color: Stone, depth: i8) -> SearchResult {
        let mut best_move = None;
        let mut best_score = -INF;
        let mut alpha = -INF;
        let beta = INF;

        let moves = self.generate_moves(board, color);

        for mov in moves {
            let mut new_board = board.clone();
            new_board.place_stone(mov, color);
            execute_captures(&mut new_board, mov, color);

            let score = -self.alpha_beta(&new_board, color.opponent(), depth - 1, -beta, -alpha);

            if score > best_score {
                best_score = score;
                best_move = Some(mov);
            }

            alpha = alpha.max(score);
        }

        SearchResult {
            best_move,
            score: best_score,
            depth,
            nodes: self.nodes,
        }
    }

    fn alpha_beta(&mut self, board: &Board, color: Stone, depth: i8, mut alpha: i32, beta: i32) -> i32 {
        self.nodes += 1;

        // Check for terminal state
        if let Some(winner) = check_winner(board) {
            return if winner == color { PatternScore::FIVE } else { -PatternScore::FIVE };
        }

        // Depth limit
        if depth <= 0 {
            return evaluate(board, color);
        }

        // TT probe
        let hash = self.zobrist.hash(board, color);
        if let Some((score, _)) = self.tt.probe(hash, depth, alpha, beta) {
            if score != 0 { // 0 means only best move returned
                return score;
            }
        }

        let moves = self.generate_moves(board, color);
        if moves.is_empty() {
            return evaluate(board, color);
        }

        let mut best_score = -INF;
        let mut best_move = None;
        let mut entry_type = EntryType::UpperBound;

        for mov in moves {
            let mut new_board = board.clone();
            new_board.place_stone(mov, color);
            execute_captures(&mut new_board, mov, color);

            let score = -self.alpha_beta(&new_board, color.opponent(), depth - 1, -beta, -alpha);

            if score > best_score {
                best_score = score;
                best_move = Some(mov);
            }

            if score >= beta {
                entry_type = EntryType::LowerBound;
                break;
            }

            if score > alpha {
                alpha = score;
                entry_type = EntryType::Exact;
            }
        }

        // Store in TT
        self.tt.store(hash, depth, best_score, entry_type, best_move);

        best_score
    }

    /// Generate candidate moves (near existing stones)
    fn generate_moves(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut moves = Vec::with_capacity(50);
        let mut seen = [[false; BOARD_SIZE]; BOARD_SIZE];

        // If board is empty, return center
        if board.is_board_empty() {
            return vec![Pos::new(9, 9)];
        }

        // Find moves near existing stones
        let radius = 2i32;

        for pos in board.black.iter_ones().chain(board.white.iter_ones()) {
            for dr in -radius..=radius {
                for dc in -radius..=radius {
                    let r = pos.row as i32 + dr;
                    let c = pos.col as i32 + dc;

                    if !Pos::is_valid(r, c) {
                        continue;
                    }

                    let new_pos = Pos::new(r as u8, c as u8);

                    if seen[r as usize][c as usize] {
                        continue;
                    }
                    seen[r as usize][c as usize] = true;

                    if is_valid_move(board, new_pos, color) {
                        moves.push(new_pos);
                    }
                }
            }
        }

        // TODO: Add move ordering (TT move, killer moves, etc.)
        moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_empty_board() {
        let mut searcher = Searcher::new(16);
        let board = Board::new();

        let result = searcher.search(&board, Stone::Black, 4);
        assert!(result.best_move.is_some());
        assert_eq!(result.best_move, Some(Pos::new(9, 9))); // Center
    }

    #[test]
    fn test_search_finds_winning_move() {
        let mut searcher = Searcher::new(16);
        let mut board = Board::new();

        // Black has 4 in a row - should find the winning 5th
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let result = searcher.search(&board, Stone::Black, 2);
        assert_eq!(result.best_move, Some(Pos::new(9, 4)));
    }
}
```

**Step 2: mod.rs 업데이트**

```rust
// engine/src/search/mod.rs

pub mod zobrist;
pub mod tt;
pub mod alphabeta;
pub mod threat;

pub use zobrist::ZobristTable;
pub use tt::{TranspositionTable, TTEntry, EntryType};
pub use alphabeta::{Searcher, SearchResult};
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test alphabeta
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/search/
git commit -m "feat(search): implement Alpha-Beta with iterative deepening and TT"
```

---

## Phase 5: VCF/VCT 위협 탐색

### Task 5.1: VCF (Victory by Continuous Fours)

**Files:**
- Create: `engine/src/search/threat.rs`
- Modify: `engine/src/search/mod.rs`

**Step 1: VCF 구현**

```rust
// engine/src/search/threat.rs

use crate::board::{Board, Stone, Pos, BOARD_SIZE};
use crate::rules::{
    is_valid_move, has_five_in_row, find_five_positions,
    can_break_five_by_capture, get_captured_positions, execute_captures
};

const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1), (1, 0), (1, 1), (1, -1),
];

/// VCF/VCT search result
#[derive(Debug, Clone)]
pub struct ThreatResult {
    pub winning_sequence: Vec<Pos>,
    pub found: bool,
}

impl ThreatResult {
    fn not_found() -> Self {
        Self { winning_sequence: Vec::new(), found: false }
    }

    fn found(sequence: Vec<Pos>) -> Self {
        Self { winning_sequence: sequence, found: true }
    }
}

/// Threat searcher for VCF/VCT
pub struct ThreatSearcher {
    max_vcf_depth: u8,
    max_vct_depth: u8,
    nodes: u64,
}

impl ThreatSearcher {
    pub fn new() -> Self {
        Self {
            max_vcf_depth: 30,
            max_vct_depth: 20,
            nodes: 0,
        }
    }

    /// Search for VCF (Victory by Continuous Fours)
    /// Only considers four-threats (moves that create 4-in-a-row)
    pub fn search_vcf(&mut self, board: &Board, color: Stone) -> ThreatResult {
        self.nodes = 0;
        let mut sequence = Vec::new();

        if self.vcf_search(board, color, 0, &mut sequence) {
            ThreatResult::found(sequence)
        } else {
            ThreatResult::not_found()
        }
    }

    fn vcf_search(&mut self, board: &Board, color: Stone, depth: u8, sequence: &mut Vec<Pos>) -> bool {
        self.nodes += 1;

        if depth > self.max_vcf_depth {
            return false;
        }

        // Find four-threats (moves that create 4-in-a-row with one end open)
        let threats = self.find_four_threats(board, color);

        for threat_move in threats {
            let mut new_board = board.clone();
            new_board.place_stone(threat_move, color);
            let captured = execute_captures(&mut new_board, threat_move, color);

            sequence.push(threat_move);

            // Check if we win immediately
            if has_five_in_row(&new_board, color) {
                if let Some(five) = find_five_positions(&new_board, color) {
                    if !can_break_five_by_capture(&new_board, &five, color) {
                        return true;
                    }
                }
            }

            // Check capture win
            if new_board.captures(color) >= 5 {
                return true;
            }

            // Find opponent's forced defense moves
            let defenses = self.find_defense_moves(&new_board, threat_move, color);

            if defenses.is_empty() {
                // Opponent has no defense - we win!
                return true;
            }

            // If only one defense, continue VCF
            if defenses.len() == 1 {
                let defense = defenses[0];
                let mut def_board = new_board.clone();
                def_board.place_stone(defense, color.opponent());
                execute_captures(&mut def_board, defense, color.opponent());

                if self.vcf_search(&def_board, color, depth + 1, sequence) {
                    return true;
                }
            }

            sequence.pop();
        }

        false
    }

    /// Find moves that create a four (4-in-a-row with at least one end open)
    fn find_four_threats(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut threats = Vec::new();

        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                if self.creates_four(board, pos, color) {
                    threats.push(pos);
                }
            }
        }

        threats
    }

    /// Check if placing at pos creates a four
    fn creates_four(&self, board: &Board, pos: Pos, color: Stone) -> bool {
        for &(dr, dc) in &DIRECTIONS {
            let mut count = 1;
            let mut open_ends = 0;

            // Positive direction
            let mut r = pos.row as i32 + dr;
            let mut c = pos.col as i32 + dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == color {
                    count += 1;
                } else if board.get(p) == Stone::Empty {
                    open_ends += 1;
                    break;
                } else {
                    break;
                }
                r += dr;
                c += dc;
            }

            // Negative direction
            r = pos.row as i32 - dr;
            c = pos.col as i32 - dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == color {
                    count += 1;
                } else if board.get(p) == Stone::Empty {
                    open_ends += 1;
                    break;
                } else {
                    break;
                }
                r -= dr;
                c -= dc;
            }

            // Four with at least one open end
            if count == 4 && open_ends >= 1 {
                return true;
            }
        }

        false
    }

    /// Find defense moves against a four-threat
    /// Includes: blocking the four, capturing attacker's stones
    fn find_defense_moves(&self, board: &Board, threat_move: Pos, attacker: Stone) -> Vec<Pos> {
        let defender = attacker.opponent();
        let mut defenses = Vec::new();

        // Find the four pattern and its extension points
        for &(dr, dc) in &DIRECTIONS {
            let mut count = 1;
            let mut line_positions = vec![threat_move];
            let mut extension_points = Vec::new();

            // Positive direction
            let mut r = threat_move.row as i32 + dr;
            let mut c = threat_move.col as i32 + dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == attacker {
                    count += 1;
                    line_positions.push(p);
                } else if board.get(p) == Stone::Empty {
                    extension_points.push(p);
                    break;
                } else {
                    break;
                }
                r += dr;
                c += dc;
            }

            // Negative direction
            r = threat_move.row as i32 - dr;
            c = threat_move.col as i32 - dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == attacker {
                    count += 1;
                    line_positions.push(p);
                } else if board.get(p) == Stone::Empty {
                    extension_points.push(p);
                    break;
                } else {
                    break;
                }
                r -= dr;
                c -= dc;
            }

            // If this is the four, add blocking moves
            if count == 4 {
                for ext in extension_points {
                    if is_valid_move(board, ext, defender) {
                        defenses.push(ext);
                    }
                }
            }
        }

        // Add capture moves that break the threat
        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, defender) {
                    continue;
                }

                let captures = get_captured_positions(board, pos, defender);
                if !captures.is_empty() {
                    // Check if this capture breaks the four
                    defenses.push(pos);
                }
            }
        }

        defenses.sort();
        defenses.dedup();
        defenses
    }

    /// Search for VCT (Victory by Continuous Threats)
    /// Considers both four-threats and three-threats
    pub fn search_vct(&mut self, board: &Board, color: Stone) -> ThreatResult {
        self.nodes = 0;
        let mut sequence = Vec::new();

        // First try VCF (faster)
        if self.vcf_search(board, color, 0, &mut sequence) {
            return ThreatResult::found(sequence);
        }

        // Then try VCT
        sequence.clear();
        if self.vct_search(board, color, 0, &mut sequence) {
            ThreatResult::found(sequence)
        } else {
            ThreatResult::not_found()
        }
    }

    fn vct_search(&mut self, board: &Board, color: Stone, depth: u8, sequence: &mut Vec<Pos>) -> bool {
        self.nodes += 1;

        if depth > self.max_vct_depth {
            return false;
        }

        // Find all threats (fours and open threes)
        let threats = self.find_all_threats(board, color);

        for threat_move in threats {
            let mut new_board = board.clone();
            new_board.place_stone(threat_move, color);
            execute_captures(&mut new_board, threat_move, color);

            sequence.push(threat_move);

            // Check for immediate win
            if has_five_in_row(&new_board, color) {
                if let Some(five) = find_five_positions(&new_board, color) {
                    if !can_break_five_by_capture(&new_board, &five, color) {
                        return true;
                    }
                }
            }

            if new_board.captures(color) >= 5 {
                return true;
            }

            // Try VCF from this position
            let mut vcf_seq = Vec::new();
            if self.vcf_search(&new_board, color, 0, &mut vcf_seq) {
                sequence.extend(vcf_seq);
                return true;
            }

            // Find defense moves
            let defenses = self.find_threat_defenses(&new_board, threat_move, color);

            if defenses.is_empty() {
                return true;
            }

            // Try each defense
            let mut all_defenses_fail = true;
            for defense in &defenses {
                let mut def_board = new_board.clone();
                def_board.place_stone(*defense, color.opponent());
                execute_captures(&mut def_board, *defense, color.opponent());

                // Continue VCT after defense
                if !self.vct_search(&def_board, color, depth + 1, sequence) {
                    all_defenses_fail = false;
                    break;
                }
            }

            if all_defenses_fail {
                return true;
            }

            sequence.pop();
        }

        false
    }

    fn find_all_threats(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut threats = Vec::new();

        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                // Four threats (highest priority)
                if self.creates_four(board, pos, color) {
                    threats.push(pos);
                }
                // Open three threats
                else if self.creates_open_three(board, pos, color) {
                    threats.push(pos);
                }
            }
        }

        threats
    }

    fn creates_open_three(&self, board: &Board, pos: Pos, color: Stone) -> bool {
        for &(dr, dc) in &DIRECTIONS {
            let mut count = 1;
            let mut open_ends = 0;

            // Positive direction
            let mut r = pos.row as i32 + dr;
            let mut c = pos.col as i32 + dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == color {
                    count += 1;
                } else if board.get(p) == Stone::Empty {
                    open_ends += 1;
                    break;
                } else {
                    break;
                }
                r += dr;
                c += dc;
            }

            // Negative direction
            r = pos.row as i32 - dr;
            c = pos.col as i32 - dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == color {
                    count += 1;
                } else if board.get(p) == Stone::Empty {
                    open_ends += 1;
                    break;
                } else {
                    break;
                }
                r -= dr;
                c -= dc;
            }

            // Open three: 3 stones with both ends open
            if count == 3 && open_ends == 2 {
                return true;
            }
        }

        false
    }

    fn find_threat_defenses(&self, board: &Board, threat_move: Pos, attacker: Stone) -> Vec<Pos> {
        let defender = attacker.opponent();
        let mut defenses = Vec::new();

        // Block the threat
        for &(dr, dc) in &DIRECTIONS {
            // Find extension points of the threat pattern
            for sign in [-1i32, 1] {
                let mut r = threat_move.row as i32;
                let mut c = threat_move.col as i32;

                // Walk along the pattern
                while Pos::is_valid(r, c) && board.get(Pos::new(r as u8, c as u8)) == attacker {
                    r += dr * sign;
                    c += dc * sign;
                }

                if Pos::is_valid(r, c) {
                    let p = Pos::new(r as u8, c as u8);
                    if board.get(p) == Stone::Empty && is_valid_move(board, p, defender) {
                        defenses.push(p);
                    }
                }
            }
        }

        // Capture defenses
        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, defender) {
                    continue;
                }

                if !get_captured_positions(board, pos, defender).is_empty() {
                    defenses.push(pos);
                }
            }
        }

        defenses.sort();
        defenses.dedup();
        defenses
    }
}

impl Default for ThreatSearcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vcf_simple_win() {
        let mut board = Board::new();
        // Black has open four: _OOOO_
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::Black);
        board.place_stone(Pos::new(9, 8), Stone::Black);

        let mut searcher = ThreatSearcher::new();
        let result = searcher.search_vcf(&board, Stone::Black);

        // Should find winning move at either end
        assert!(result.found);
    }

    #[test]
    fn test_creates_four() {
        let mut board = Board::new();
        // Three in a row: _OOO_
        board.place_stone(Pos::new(9, 6), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::Black);
        board.place_stone(Pos::new(9, 8), Stone::Black);

        let searcher = ThreatSearcher::new();
        // Placing at 9,5 or 9,9 creates a four
        assert!(searcher.creates_four(&board, Pos::new(9, 5), Stone::Black));
        assert!(searcher.creates_four(&board, Pos::new(9, 9), Stone::Black));
    }
}
```

**Step 2: mod.rs 업데이트**

```rust
// engine/src/search/mod.rs에 추가

pub mod threat;
pub use threat::{ThreatSearcher, ThreatResult};
```

**Step 3: 테스트 실행**

```bash
cd engine && cargo test threat
```
Expected: All tests pass

**Step 4: Commit**

```bash
git add engine/src/search/
git commit -m "feat(search): implement VCF/VCT threat search with capture defense"
```

---

## Phase 6: 엔진 통합 및 완성

### Task 6.1: AI 엔진 통합

**Files:**
- Create: `engine/src/engine.rs`
- Modify: `engine/src/lib.rs`
- Modify: `engine/src/main.rs`

**Step 1: 통합 엔진**

```rust
// engine/src/engine.rs

use crate::board::{Board, Stone, Pos};
use crate::rules::{check_winner, is_valid_move};
use crate::search::{Searcher, ThreatSearcher, SearchResult, ThreatResult};

/// Main AI Engine
pub struct AIEngine {
    searcher: Searcher,
    threat_searcher: ThreatSearcher,
    max_depth: i8,
}

impl AIEngine {
    pub fn new() -> Self {
        Self {
            searcher: Searcher::new(64), // 64MB TT
            threat_searcher: ThreatSearcher::new(),
            max_depth: 10,
        }
    }

    /// Get the best move for the given position
    /// Search order: VCF → VCT → Opponent VCF/VCT defense → Minimax
    pub fn get_move(&mut self, board: &Board, color: Stone) -> Option<Pos> {
        // 1. Check for immediate winning move (5-in-a-row)
        if let Some(win_move) = self.find_immediate_win(board, color) {
            return Some(win_move);
        }

        // 2. Search VCF (fastest, most certain)
        let vcf_result = self.threat_searcher.search_vcf(board, color);
        if vcf_result.found && !vcf_result.winning_sequence.is_empty() {
            return Some(vcf_result.winning_sequence[0]);
        }

        // 3. Search VCT
        let vct_result = self.threat_searcher.search_vct(board, color);
        if vct_result.found && !vct_result.winning_sequence.is_empty() {
            return Some(vct_result.winning_sequence[0]);
        }

        // 4. Check opponent's VCF/VCT - must defend!
        let opponent = color.opponent();
        let opp_vcf = self.threat_searcher.search_vcf(board, opponent);
        if opp_vcf.found {
            // Find best defense move
            return self.find_best_defense(board, color, &opp_vcf);
        }

        let opp_vct = self.threat_searcher.search_vct(board, opponent);
        if opp_vct.found {
            return self.find_best_defense(board, color, &opp_vct);
        }

        // 5. Regular Minimax search
        let result = self.searcher.search(board, color, self.max_depth);
        result.best_move
    }

    /// Find immediate winning move (5-in-a-row)
    fn find_immediate_win(&self, board: &Board, color: Stone) -> Option<Pos> {
        for r in 0..19u8 {
            for c in 0..19u8 {
                let pos = Pos::new(r, c);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                let mut test_board = board.clone();
                test_board.place_stone(pos, color);

                if check_winner(&test_board) == Some(color) {
                    return Some(pos);
                }
            }
        }
        None
    }

    /// Find best defense against opponent's threat
    fn find_best_defense(&mut self, board: &Board, color: Stone, threat: &ThreatResult) -> Option<Pos> {
        if threat.winning_sequence.is_empty() {
            return None;
        }

        // Try to block the first move in opponent's winning sequence
        let threat_move = threat.winning_sequence[0];

        // Option 1: Play at the threat position ourselves
        if is_valid_move(board, threat_move, color) {
            return Some(threat_move);
        }

        // Option 2: Find a counter-threat or capture
        // Use Minimax to find best defensive move
        let result = self.searcher.search(board, color, 6);
        result.best_move
    }

    pub fn set_max_depth(&mut self, depth: i8) {
        self.max_depth = depth;
    }
}

impl Default for AIEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_finds_winning_move() {
        let mut board = Board::new();
        // 4 in a row
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let mut engine = AIEngine::new();
        let best = engine.get_move(&board, Stone::Black);

        assert_eq!(best, Some(Pos::new(9, 4)));
    }

    #[test]
    fn test_engine_blocks_opponent_win() {
        let mut board = Board::new();
        // White has 4 in a row
        for i in 0..4 {
            board.place_stone(Pos::new(9, i), Stone::White);
        }

        let mut engine = AIEngine::new();
        let best = engine.get_move(&board, Stone::Black);

        // Should block at 9,4
        assert_eq!(best, Some(Pos::new(9, 4)));
    }
}
```

**Step 2: lib.rs 업데이트**

```rust
// engine/src/lib.rs

pub mod board;
pub mod rules;
pub mod eval;
pub mod search;
pub mod engine;

pub use board::{Board, Stone, Pos};
pub use engine::AIEngine;
```

**Step 3: main.rs 업데이트**

```rust
// engine/src/main.rs

use gomoku::{Board, Stone, Pos, AIEngine};
use std::time::Instant;

fn main() {
    println!("Gomoku AI Engine v0.1.0");
    println!("========================\n");

    let mut board = Board::new();
    let mut engine = AIEngine::new();

    // Simple test game
    println!("Testing AI move generation...\n");

    // Black plays center
    let pos = Pos::new(9, 9);
    board.place_stone(pos, Stone::Black);
    println!("Black plays: ({}, {})", pos.row, pos.col);

    // AI (White) responds
    let start = Instant::now();
    let ai_move = engine.get_move(&board, Stone::White);
    let elapsed = start.elapsed();

    if let Some(m) = ai_move {
        println!("White (AI) plays: ({}, {}) in {:?}", m.row, m.col, elapsed);
    }

    println!("\nEngine initialized successfully!");
}
```

**Step 4: 테스트 및 빌드**

```bash
cd engine && cargo test engine
cd engine && cargo build --release
```
Expected: All tests pass, build successful

**Step 5: Commit**

```bash
git add engine/src/
git commit -m "feat: integrate AI engine with VCF/VCT priority search"
```

---

## Phase 7: GUI 연동 (선택적)

### Task 7.1: Python GUI와 연동

**주의:** 이 Task는 기존 Python GUI를 유지하면서 Rust 엔진과 연동하는 방법입니다. 필요에 따라 순수 Rust GUI (SDL2, egui)로 대체할 수 있습니다.

**Files:**
- Create: `engine/src/protocol.rs`
- Modify: `engine/src/main.rs`

**Step 1: 간단한 텍스트 프로토콜**

```rust
// engine/src/protocol.rs

use crate::{Board, Stone, Pos, AIEngine};
use std::io::{self, BufRead, Write};

/// Simple text protocol for GUI communication
pub fn run_protocol() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut board = Board::new();
    let mut engine = AIEngine::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "MOVE" if parts.len() >= 4 => {
                // MOVE row col color
                let row: u8 = parts[1].parse().unwrap_or(255);
                let col: u8 = parts[2].parse().unwrap_or(255);
                let color = if parts[3] == "BLACK" { Stone::Black } else { Stone::White };

                if row < 19 && col < 19 {
                    board.place_stone(Pos::new(row, col), color);
                    writeln!(stdout, "OK").unwrap();
                } else {
                    writeln!(stdout, "ERROR invalid position").unwrap();
                }
            }
            "UNDO" => {
                // Simple undo - would need move history
                writeln!(stdout, "OK").unwrap();
            }
            "GENMOVE" if parts.len() >= 2 => {
                // GENMOVE color
                let color = if parts[1] == "BLACK" { Stone::Black } else { Stone::White };

                if let Some(pos) = engine.get_move(&board, color) {
                    writeln!(stdout, "MOVE {} {}", pos.row, pos.col).unwrap();
                } else {
                    writeln!(stdout, "PASS").unwrap();
                }
            }
            "RESET" => {
                board = Board::new();
                writeln!(stdout, "OK").unwrap();
            }
            "QUIT" => {
                break;
            }
            _ => {
                writeln!(stdout, "ERROR unknown command").unwrap();
            }
        }

        stdout.flush().unwrap();
    }
}
```

**Step 2: main.rs에 프로토콜 모드 추가**

```rust
// engine/src/main.rs 수정

use gomoku::{Board, Stone, Pos, AIEngine};
use std::env;
use std::time::Instant;

mod protocol;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--protocol" {
        // Protocol mode for GUI communication
        protocol::run_protocol();
    } else {
        // Interactive test mode
        interactive_test();
    }
}

fn interactive_test() {
    println!("Gomoku AI Engine v0.1.0");
    // ... 기존 테스트 코드
}
```

**Step 3: Commit**

```bash
git add engine/src/
git commit -m "feat: add text protocol for GUI communication"
```

---

## 검증 및 최종 테스트

### Task 8.1: 성능 검증

```bash
cd engine
cargo build --release
time ./target/release/gomoku

# 벤치마크 테스트
cargo test --release -- --nocapture
```

**Expected:**
- 빌드 성공
- 깊이 10 탐색이 0.5초 이내
- 모든 테스트 통과

---

## 요약

| Phase | 설명 | 예상 시간 |
|-------|------|----------|
| 1 | 프로젝트 설정 + Board 모듈 | 2-3시간 |
| 2 | Rules (캡처, 승리, 금지수) | 2-3시간 |
| 3 | Evaluation (패턴, 휴리스틱) | 2시간 |
| 4 | Search (Zobrist, TT, Alpha-Beta) | 3-4시간 |
| 5 | VCF/VCT 위협 탐색 | 4-5시간 |
| 6 | 엔진 통합 | 1-2시간 |
| 7 | GUI 연동 (선택적) | 2-3시간 |

**총 예상 시간: 16-22시간**
