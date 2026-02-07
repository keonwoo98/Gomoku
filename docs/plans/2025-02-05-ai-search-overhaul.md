# AI Search Engine Overhaul Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform the AI from a depth-4 search with no move ordering into a competitive depth-10+ engine with proper time management, move ordering, PVS, and killer/history heuristics.

**Architecture:** The search pipeline follows a 5-stage priority system (immediate win → VCF → VCT → defense → alpha-beta). The alpha-beta search uses iterative deepening with aspiration windows, PVS (Principal Variation Search), TT-driven move ordering, killer moves, and history heuristic. Time management controls the iterative deepening loop to stay within 450ms.

**Tech Stack:** Rust, existing bitboard representation, Zobrist hashing, transposition table

---

## Critical Bugs Found During Analysis

Before improving search strength, these bugs must be fixed:

1. **Engine not persisted across moves** (`game_state.rs:346`): Creates a NEW `AIEngine` per move, losing all TT data between moves. This is catastrophic for performance.
2. **Depth capped to 4** (`engine.rs:403`): `effective_depth = 4.min(self.max_depth)` hard-caps search to depth 4.
3. **No move ordering** (`alphabeta.rs:299`): The TODO comment says it all.

---

### Task 1: Persist AIEngine Across Moves

**Files:**
- Modify: `src/ui/game_state.rs:65-82` (GameState struct)
- Modify: `src/ui/game_state.rs:137-154` (GameState::new)
- Modify: `src/ui/game_state.rs:156-168` (GameState::reset)
- Modify: `src/ui/game_state.rs:333-355` (start_ai_thinking)

**Context:** Currently `start_ai_thinking()` creates a new `AIEngine` in every thread spawn. The TT (transposition table) is lost after every move. The engine should be persistent and shared via `Arc<Mutex<>>` or simply reused.

**Step 1: Add AIEngine field to GameState**

```rust
// In GameState struct (game_state.rs)
pub struct GameState {
    // ... existing fields ...
    ai_engine: Option<AIEngine>, // Persistent engine
}
```

**Step 2: Initialize in `new()` and `reset()`**

```rust
// In GameState::new()
ai_engine: Some(AIEngine::with_config(64, 10, 450)),

// In GameState::reset()
if let Some(ref mut engine) = self.ai_engine {
    engine.clear_cache();
}
```

**Step 3: Pass engine to AI thread via take/give pattern**

```rust
// In start_ai_thinking()
let mut engine = match self.ai_engine.take() {
    Some(e) => e,
    None => AIEngine::with_config(64, 10, 450),
};

let (tx, rx) = channel();
thread::spawn(move || {
    let result = engine.get_move_with_stats(&board, color);
    let _ = tx.send((result, engine));
});
```

**Step 4: Receive engine back after AI move**

Modify `check_ai_result()` to receive `(MoveResult, AIEngine)` instead of just `MoveResult`. Store the engine back in `self.ai_engine`.

**Step 5: Verify**

Run: `make test`
Expected: All 162 tests pass

**Step 6: Commit**

```
feat: persist AIEngine across moves for TT reuse
```

---

### Task 2: Add Move Ordering to Alpha-Beta

**Files:**
- Modify: `src/search/alphabeta.rs:54-59` (Searcher struct - add killer/history tables)
- Modify: `src/search/alphabeta.rs:254-301` (generate_moves - add ordering)
- Modify: `src/search/alphabeta.rs:166-238` (alpha_beta - update killer/history on cutoff)

**Context:** Move ordering is the single most impactful improvement. With good ordering, alpha-beta prunes 90%+ of the tree. Without it, depth 4 explores as many nodes as depth 10+ with ordering.

**Step 1: Add ordering state to Searcher**

```rust
pub struct Searcher {
    zobrist: ZobristTable,
    tt: TranspositionTable,
    nodes: u64,
    max_depth: i8,
    // NEW: Move ordering state
    killer_moves: [[Option<Pos>; 2]; 64], // 2 killers per depth, max 64 depth
    history: [[[i32; BOARD_SIZE]; BOARD_SIZE]; 2], // [color_idx][row][col]
}
```

Initialize all to zero/None in `new()`. Clear in `search()` start.

**Step 2: Score moves in generate_moves**

Add a `score_move()` helper and sort moves:

```rust
fn generate_moves_ordered(
    &self,
    board: &Board,
    color: Stone,
    tt_move: Option<Pos>,
    depth: i8,
) -> Vec<Pos> {
    let mut moves = self.generate_candidate_positions(board, color);

    // Score each move for ordering
    let mut scored: Vec<(Pos, i32)> = moves.into_iter()
        .map(|m| (m, self.score_move(board, m, color, tt_move, depth)))
        .collect();

    // Sort descending by score
    scored.sort_unstable_by(|a, b| b.1.cmp(&a.1));

    scored.into_iter().map(|(m, _)| m).collect()
}
```

**Move scoring priorities:**
1. TT best move: `1_000_000`
2. Winning move (creates 5): `900_000`
3. Killer move match: `500_000` / `490_000`
4. Capture move: `100_000 + capture_count * 50_000`
5. Creates open four: `80_000`
6. Blocks opponent four: `70_000`
7. Creates open three: `30_000`
8. History score: `history[color_idx][row][col]`
9. Center proximity: `(18 - manhattan_distance) * 10`

**Step 3: Update killer moves on beta cutoff**

In `alpha_beta()`, when `score >= beta`:

```rust
if score >= beta {
    // Update killer moves (non-capture only)
    let ply = (self.max_depth - depth) as usize;
    if ply < 64 {
        if self.killer_moves[ply][0] != Some(mov) {
            self.killer_moves[ply][1] = self.killer_moves[ply][0];
            self.killer_moves[ply][0] = Some(mov);
        }
    }
    // Update history heuristic
    let cidx = if color == Stone::Black { 0 } else { 1 };
    self.history[cidx][mov.row as usize][mov.col as usize] += depth as i32 * depth as i32;

    entry_type = EntryType::LowerBound;
    break;
}
```

**Step 4: Pass TT move to generate_moves**

In `alpha_beta()`, extract TT move from probe and pass it to `generate_moves_ordered()`:

```rust
let tt_move = self.tt.get_best_move(hash);
let moves = self.generate_moves_ordered(board, color, tt_move, depth);
```

**Step 5: Write test for move ordering**

```rust
#[test]
fn test_move_ordering_tt_first() {
    let mut searcher = Searcher::new(16);
    let mut board = Board::new();
    board.place_stone(Pos::new(9, 9), Stone::Black);

    let tt_move = Some(Pos::new(9, 10));
    let moves = searcher.generate_moves_ordered(&board, Stone::White, tt_move, 4);

    // TT move should be first
    assert_eq!(moves[0], Pos::new(9, 10));
}
```

**Step 6: Verify**

Run: `make test`
Expected: All tests pass

**Step 7: Commit**

```
feat: add move ordering with TT/killer/history heuristics
```

---

### Task 3: Add Time Management to Iterative Deepening

**Files:**
- Modify: `src/search/alphabeta.rs:54-59` (Searcher struct - add time fields)
- Modify: `src/search/alphabeta.rs:100-125` (search - add time management)
- Modify: `src/search/alphabeta.rs:166-238` (alpha_beta - add time checks)

**Context:** Currently iterative deepening runs to `max_depth` unconditionally. With time management, the search runs as deep as possible within the time budget, aborting mid-search when time runs out.

**Step 1: Add time tracking to Searcher**

```rust
pub struct Searcher {
    // ... existing ...
    start_time: Option<Instant>,
    time_limit: Option<Duration>,
    stopped: bool,
}
```

**Step 2: Modify search() to accept time limit**

```rust
pub fn search_timed(
    &mut self,
    board: &Board,
    color: Stone,
    max_depth: i8,
    time_limit_ms: u64,
) -> SearchResult {
    self.start_time = Some(Instant::now());
    self.time_limit = Some(Duration::from_millis(time_limit_ms));
    self.stopped = false;
    self.nodes = 0;
    // Clear killer moves for new search
    self.killer_moves = [[None; 2]; 64];

    let mut best_result = SearchResult { ... };

    for depth in 1..=max_depth {
        if self.stopped { break; }

        let result = self.search_root(board, color, depth);

        if self.stopped { break; } // Don't use partial results

        best_result = result;
        best_result.depth = depth;

        if best_result.score >= PatternScore::FIVE - 100 { break; }

        // Time check: if >40% of budget used, don't start next depth
        if let (Some(start), Some(limit)) = (self.start_time, self.time_limit) {
            if start.elapsed() > limit * 2 / 5 {
                break;
            }
        }
    }

    best_result.nodes = self.nodes;
    best_result
}
```

**Step 3: Add time check in alpha_beta**

Every 1024 nodes, check if time is up:

```rust
fn alpha_beta(&mut self, ...) -> i32 {
    self.nodes += 1;

    // Time check every 1024 nodes
    if self.nodes & 1023 == 0 {
        if let (Some(start), Some(limit)) = (self.start_time, self.time_limit) {
            if start.elapsed() >= limit {
                self.stopped = true;
                return 0;
            }
        }
    }

    if self.stopped { return 0; }

    // ... rest of search ...
}
```

**Step 4: Verify**

Run: `make test`
Expected: All tests pass

**Step 5: Commit**

```
feat: add time management to iterative deepening search
```

---

### Task 4: Raise Depth Limit and Wire Up Time-Managed Search

**Files:**
- Modify: `src/engine.rs:400-406` (remove depth=4 cap, use search_timed)

**Step 1: Replace hardcoded depth 4 with time-managed search**

```rust
// In get_move_with_stats(), replace lines 400-406:
// OLD: let effective_depth = 4.min(self.max_depth);
// NEW:
let result = self.searcher.search_timed(board, color, self.max_depth, 450);
MoveResult::from_alphabeta(result, start.elapsed().as_millis() as u64)
```

The engine's `max_depth` is already 10 (set in `AIEngine::new()`). With time management, the searcher will go as deep as possible within 450ms.

**Step 2: Verify AI plays stronger**

Run: `make test`
Then: `./Gomoku` and play a few moves. AI should be noticeably stronger.

**Step 3: Commit**

```
feat: remove depth-4 cap, enable full time-managed search
```

---

### Task 5: Add PVS (Principal Variation Search)

**Files:**
- Modify: `src/search/alphabeta.rs:166-238` (alpha_beta)

**Context:** PVS searches the first move with full window and remaining moves with null window. If a null-window search fails high, it re-searches with full window. This typically reduces nodes by 30-50%.

**Step 1: Implement PVS in alpha_beta**

```rust
fn alpha_beta(&mut self, board, color, depth, mut alpha, beta) -> i32 {
    // ... existing TT probe, terminal checks ...

    let mut best_score = -INF;
    let mut best_move = None;
    let mut entry_type = EntryType::UpperBound;
    let mut first_move = true;

    for mov in moves {
        let mut new_board = board.clone();
        new_board.place_stone(mov, color);
        execute_captures(&mut new_board, mov, color);

        let score;
        if first_move {
            // Full window search for first (expected best) move
            score = -self.alpha_beta(&new_board, color.opponent(), depth - 1, -beta, -alpha);
            first_move = false;
        } else {
            // Null-window search
            score = -self.alpha_beta(&new_board, color.opponent(), depth - 1, -alpha - 1, -alpha);
            if score > alpha && score < beta {
                // Re-search with full window
                score = -self.alpha_beta(&new_board, color.opponent(), depth - 1, -beta, -alpha);
            }
        }

        if self.stopped { return 0; }

        if score > best_score { ... }
        if score >= beta { ... killer/history update ... break; }
        if score > alpha { alpha = score; entry_type = EntryType::Exact; }
    }

    // TT store ...
    best_score
}
```

**Step 2: Verify**

Run: `make test`
Expected: All tests pass. Node count should decrease for same depth.

**Step 3: Commit**

```
feat: add PVS (Principal Variation Search) to alpha-beta
```

---

### Task 6: Improve Evaluation - Detect Gap Patterns

**Files:**
- Modify: `src/eval/heuristic.rs:104-159` (evaluate_line)

**Context:** Current eval only detects CONSECUTIVE patterns. Patterns like `O_OOO` (4 with gap) or `OO_OO` should also be scored, as they are one move from completing 5.

**Step 1: Modify evaluate_line to detect gap patterns**

```rust
fn evaluate_line(board: &Board, pos: Pos, dr: i32, dc: i32, color: Stone) -> i32 {
    // Skip if not start of line (same as before)
    // ...

    let mut count = 1;
    let mut open_ends = 0;
    let mut gap_count = 0;
    let mut total_span = 1;

    // Check negative end
    // ... (same as before for open_ends) ...

    // Extend in positive direction, allowing one gap
    let mut r = pos.row as i32 + dr;
    let mut c = pos.col as i32 + dc;
    let mut found_gap = false;
    while Pos::is_valid(r, c) {
        let p = Pos::new(r as u8, c as u8);
        match board.get(p) {
            s if s == color => {
                count += 1;
                total_span += 1;
            }
            Stone::Empty if !found_gap => {
                // Check if stone follows this gap
                let nr = r + dr;
                let nc = c + dc;
                if Pos::is_valid(nr, nc) && board.get(Pos::new(nr as u8, nc as u8)) == color {
                    found_gap = true;
                    gap_count += 1;
                    total_span += 1;
                    r += dr;
                    c += dc;
                    continue;
                }
                open_ends += 1;
                break;
            }
            _ => break,
        }
        r += dr;
        c += dc;
    }

    // Score with gap awareness
    match (count, open_ends, gap_count) {
        (5.., _, _) => PatternScore::FIVE,
        (4, _, 1) if total_span == 5 => PatternScore::OPEN_FOUR, // OO_OO type
        (4, 2, 0) => PatternScore::OPEN_FOUR,
        (4, 1, 0) => PatternScore::CLOSED_FOUR,
        (4, _, _) => PatternScore::CLOSED_FOUR, // 4 with gap
        (3, 2, _) => PatternScore::OPEN_THREE,
        (3, 1, _) => PatternScore::CLOSED_THREE,
        (2, 2, _) => PatternScore::OPEN_TWO,
        (2, 1, _) => PatternScore::CLOSED_TWO,
        _ => 0,
    }
}
```

**Step 2: Verify**

Run: `make test`
Expected: All tests pass

**Step 3: Commit**

```
feat: detect gap patterns in evaluation function
```

---

### Task 7: Final Integration Test

**Files:** No new files

**Step 1: Run full test suite**

```bash
make test-release
```
Expected: All tests pass, fast execution

**Step 2: Run manual play test**

```bash
./Gomoku
```

Verify:
- AI responds within 500ms per move
- AI blocks obvious threats (4-in-a-row, gap patterns)
- AI plays aggressively (creates own threats)
- Debug panel shows depth > 4

**Step 3: Final commit if any tweaks needed**

---

## Execution Order & Dependencies

```
Task 1 (persist engine) ─────┐
                               ├──→ Task 4 (raise depth + wire up)
Task 2 (move ordering) ──────┤
                               ├──→ Task 5 (PVS)
Task 3 (time management) ────┘

Task 6 (gap eval) ── independent, can be done anytime

Task 7 (integration test) ── depends on all above
```

Tasks 1, 2, 3 can be done in parallel. Task 4 depends on 2+3. Task 5 depends on 2+3. Task 6 is independent. Task 7 is last.
