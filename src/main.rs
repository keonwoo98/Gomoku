//! Gomoku AI Engine CLI
//!
//! A command-line interface for testing the Gomoku AI engine.
//! Demonstrates the engine's capabilities with various test scenarios.

use gomoku::{AIEngine, Board, Pos, Stone, BOARD_SIZE};

fn main() {
    println!("===========================================");
    println!("       Gomoku AI Engine v0.1.0");
    println!("===========================================\n");

    // Use smaller depth for faster demo (default depth 10 is for real games)
    let mut engine = AIEngine::with_config(32, 6, 500);

    // Test 1: Empty board - should play center
    println!("--- Test 1: Empty Board ---");
    test_empty_board(&mut engine);

    // Test 2: Find winning move
    println!("\n--- Test 2: Find Winning Move ---");
    test_winning_move(&mut engine);

    // Test 3: Block opponent win
    println!("\n--- Test 3: Block Opponent Win ---");
    test_block_opponent(&mut engine);

    // Test 4: Capture win
    println!("\n--- Test 4: Capture Win ---");
    test_capture_win(&mut engine);

    // Test 5: Opening response
    println!("\n--- Test 5: Opening Response ---");
    test_opening_response(&mut engine);

    // Test 6: Performance benchmark
    println!("\n--- Test 6: Performance Benchmark ---");
    test_performance(&mut engine);

    println!("\n===========================================");
    println!("          All Tests Completed!");
    println!("===========================================");
}

fn test_empty_board(engine: &mut AIEngine) {
    let board = Board::new();
    let result = engine.get_move_with_stats(&board, Stone::Black);

    if let Some(m) = result.best_move {
        println!("  Black plays: ({}, {})", m.row, m.col);
        println!("  Search type: {:?}", result.search_type);
        println!("  Time: {}ms", result.time_ms);
        println!("  Expected: Center (9, 9)");
        if m == Pos::new(9, 9) {
            println!("  Result: PASS");
        } else {
            println!("  Result: DIFFERENT (but valid)");
        }
    } else {
        println!("  Result: FAIL - No move found");
    }
}

fn test_winning_move(engine: &mut AIEngine) {
    let mut board = Board::new();
    // Black has 4 in a row, needs one more
    for i in 0..4 {
        board.place_stone(Pos::new(9, i), Stone::Black);
    }

    let result = engine.get_move_with_stats(&board, Stone::Black);

    if let Some(m) = result.best_move {
        println!("  Position: Black has 4 at row 9, cols 0-3");
        println!("  Black plays: ({}, {})", m.row, m.col);
        println!("  Search type: {:?}", result.search_type);
        println!("  Time: {}ms", result.time_ms);
        println!("  Expected: (9, 4) - Immediate Win");
        if m == Pos::new(9, 4) {
            println!("  Result: PASS");
        } else {
            println!("  Result: FAIL - Wrong move");
        }
    } else {
        println!("  Result: FAIL - No move found");
    }
}

fn test_block_opponent(engine: &mut AIEngine) {
    let mut board = Board::new();
    // White has 4 in a row, Black must block
    for i in 0..4 {
        board.place_stone(Pos::new(9, i), Stone::White);
    }
    // Add a black stone so it's not just empty
    board.place_stone(Pos::new(10, 5), Stone::Black);

    let result = engine.get_move_with_stats(&board, Stone::Black);

    if let Some(m) = result.best_move {
        println!("  Position: White has 4 at row 9, cols 0-3");
        println!("  Black plays: ({}, {})", m.row, m.col);
        println!("  Search type: {:?}", result.search_type);
        println!("  Time: {}ms", result.time_ms);
        println!("  Expected: (9, 4) - Defense");
        if m == Pos::new(9, 4) {
            println!("  Result: PASS");
        } else {
            println!("  Result: FAIL - Wrong move");
        }
    } else {
        println!("  Result: FAIL - No move found");
    }
}

fn test_capture_win(engine: &mut AIEngine) {
    let mut board = Board::new();
    // Black has 4 captures (8 stones), one more capture wins
    board.black_captures = 4;

    // Set up capturable pair: Black at (9,8), White at (9,9), (9,10), empty at (9,11)
    board.place_stone(Pos::new(9, 8), Stone::Black);
    board.place_stone(Pos::new(9, 9), Stone::White);
    board.place_stone(Pos::new(9, 10), Stone::White);

    let result = engine.get_move_with_stats(&board, Stone::Black);

    if let Some(m) = result.best_move {
        println!("  Position: Black has 4 captures, capturable pair at (9,9)-(9,10)");
        println!("  Black plays: ({}, {})", m.row, m.col);
        println!("  Search type: {:?}", result.search_type);
        println!("  Time: {}ms", result.time_ms);
        println!("  Expected: (9, 11) - Capture Win");
        if m == Pos::new(9, 11) {
            println!("  Result: PASS");
        } else {
            println!("  Result: DIFFERENT - {}",
                if m.row == 9 { "Same row at least" } else { "Unexpected move" });
        }
    } else {
        println!("  Result: FAIL - No move found");
    }
}

fn test_opening_response(engine: &mut AIEngine) {
    let mut board = Board::new();
    // Black plays center
    board.place_stone(Pos::new(9, 9), Stone::Black);

    let result = engine.get_move_with_stats(&board, Stone::White);

    if let Some(m) = result.best_move {
        println!("  Position: Black at center (9, 9)");
        println!("  White responds: ({}, {})", m.row, m.col);
        println!("  Search type: {:?}", result.search_type);
        println!("  Time: {}ms", result.time_ms);
        println!("  Nodes: {}", result.nodes);

        // Check if response is reasonable (adjacent to center)
        let dr = (m.row as i32 - 9).abs();
        let dc = (m.col as i32 - 9).abs();
        if dr <= 2 && dc <= 2 {
            println!("  Result: PASS - Adjacent to center");
        } else {
            println!("  Result: QUESTIONABLE - Far from center");
        }
    } else {
        println!("  Result: FAIL - No move found");
    }
}

fn test_performance(engine: &mut AIEngine) {
    let mut board = Board::new();

    // Create a mid-game position
    let moves = [
        (9, 9, Stone::Black),
        (10, 10, Stone::White),
        (8, 8, Stone::Black),
        (10, 8, Stone::White),
        (9, 7, Stone::Black),
        (9, 10, Stone::White),
        (7, 9, Stone::Black),
        (11, 9, Stone::White),
    ];

    for (r, c, stone) in moves.iter() {
        board.place_stone(Pos::new(*r, *c), *stone);
    }

    println!("  Position: Mid-game with {} stones", board.stone_count());

    // Warm up
    let _ = engine.get_move(&board, Stone::Black);
    engine.clear_cache();

    // Benchmark
    let iterations = 5;
    let mut total_time = 0u64;
    let mut total_nodes = 0u64;

    for i in 0..iterations {
        engine.clear_cache();
        let result = engine.get_move_with_stats(&board, Stone::Black);
        total_time += result.time_ms;
        total_nodes += result.nodes;
        if i == 0 {
            if let Some(m) = result.best_move {
                println!("  Best move: ({}, {})", m.row, m.col);
                println!("  Search type: {:?}", result.search_type);
            }
        }
    }

    let avg_time = total_time / iterations as u64;
    let avg_nodes = total_nodes / iterations as u64;

    println!("  Average time: {}ms", avg_time);
    println!("  Average nodes: {}", avg_nodes);
    println!("  Nodes/sec: {:.0}", (avg_nodes as f64) / (avg_time as f64 / 1000.0));

    if avg_time <= 500 {
        println!("  Result: PASS - Under 500ms target");
    } else {
        println!("  Result: SLOW - Over 500ms target");
    }
}

/// Print board state (for debugging)
#[allow(dead_code)]
fn print_board(board: &Board) {
    print!("   ");
    for c in 0..BOARD_SIZE {
        print!("{:2}", c);
    }
    println!();

    for r in 0..BOARD_SIZE {
        print!("{:2} ", r);
        for c in 0..BOARD_SIZE {
            let pos = Pos::new(r as u8, c as u8);
            let ch = match board.get(pos) {
                Stone::Black => " X",
                Stone::White => " O",
                Stone::Empty => " .",
            };
            print!("{}", ch);
        }
        println!();
    }
}
