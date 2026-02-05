# Gomoku AI Engine - Architecture Decisions

> Core design decisions for the Rust-based Ninuki-renju Gomoku AI Engine

## Table of Contents

1. [Language Selection](#1-language-selection)
2. [Board Representation](#2-board-representation)
3. [Search Priority Pipeline](#3-search-priority-pipeline)
4. [Pattern Scoring System](#4-pattern-scoring-system)
5. [Heuristic Evaluation](#5-heuristic-evaluation)
6. [VCF/VCT Threat Search](#6-vcfvct-threat-search)
7. [Transposition Table](#7-transposition-table)
8. [Zobrist Hashing](#8-zobrist-hashing)
9. [Move Ordering](#9-move-ordering)
10. [Capture Rules Implementation](#10-capture-rules-implementation)

---

## 1. Language Selection

### Decision
Reimplement AI engine from Python to Rust.

### Rationale
- **Python performance**: ~255-466 NPS (Nodes Per Second)
- **C++ reference**: ~10M NPS
- **Rust target**: C++ level performance with memory safety
- **Requirement**: 0.5 second response time with depth 10+ search

### Trade-offs

| Pros | Cons |
|------|------|
| 30x-50x performance improvement | Increased development time |
| Memory safety (no segfaults) | Steeper learning curve |
| Zero-cost abstractions | FFI needed for Python GUI (PyO3) |
| Excellent tooling (cargo, clippy) | Compile times longer than Python |

### Evidence
```
Python (before):  ~300 NPS → depth 6 in 0.5s
Rust (after):     ~15K NPS → depth 12+ in 0.5s
```

---

## 2. Board Representation

### Decision
Use 6 x u64 array Bitboard (384 bits total, 361 cells used for 19x19 board).

```rust
pub struct Bitboard {
    bits: [u64; 6],  // 6 * 64 = 384 >= 361
}
```

### Rationale
- **O(1) operations**: Stone placement, removal, and checking
- **Bit manipulation**: Fast pattern detection with AND/OR/XOR
- **Cache-friendly**: 48 bytes fits in L1 cache line
- **Efficient cloning**: Simple memory copy for search tree

### Trade-offs

| Pros | Cons |
|------|------|
| Constant-time operations | Lower debugging readability |
| Efficient bit-parallel patterns | Position calculation overhead |
| Cheap Clone implementation | Multi-word boundary handling |
| Cache-friendly memory layout | Bit manipulation complexity |

### Implementation Details
```rust
// Position to bit index conversion
let word_idx = pos / 64;  // Which u64 (0-5)
let bit_idx = pos % 64;   // Which bit (0-63)

// Stone operations
fn set(&mut self, pos: usize) {
    self.bits[pos / 64] |= 1u64 << (pos % 64);
}

fn clear(&mut self, pos: usize) {
    self.bits[pos / 64] &= !(1u64 << (pos % 64));
}

fn get(&self, pos: usize) -> bool {
    (self.bits[pos / 64] >> (pos % 64)) & 1 == 1
}
```

---

## 3. Search Priority Pipeline

### Decision
Implement 5-stage priority search pipeline.

```
1. Immediate Win    → Check if current player can win now
2. VCF Search       → Victory by Continuous Fours
3. VCT Search       → Victory by Continuous Threats
4. Defense          → Block opponent's winning threats
5. Alpha-Beta       → General position evaluation
```

### Rationale
- **Forced wins first**: VCF/VCT guarantee optimal play when winning sequence exists
- **Separate defense**: Ensures critical threats are never overlooked
- **Alpha-Beta last**: Most expensive, only used when no forcing sequences

### Trade-offs

| Pros | Cons |
|------|------|
| Never misses forced wins | VCT slow on sparse boards |
| Fast wins when available | Pipeline complexity |
| Defense prioritized | Multiple search phases overhead |
| Clear separation of concerns | State management between phases |

### Flow Diagram
```
┌──────────────┐
│ Immediate    │──win──→ Return winning move
│ Win Check    │
└──────┬───────┘
       │ no
┌──────▼───────┐
│ VCF Search   │──found──→ Return VCF sequence start
│ (depth 30)   │
└──────┬───────┘
       │ not found
┌──────▼───────┐
│ VCT Search   │──found──→ Return VCT sequence start
│ (depth 20)   │  (skip if stone_count < 8)
└──────┬───────┘
       │ not found
┌──────▼───────┐
│ Defense      │──threat──→ Return blocking move
│ Check        │
└──────┬───────┘
       │ no immediate threat
┌──────▼───────┐
│ Alpha-Beta   │──────────→ Return best evaluated move
│ Search       │
└──────────────┘
```

---

## 4. Pattern Scoring System

### Decision
Use hierarchical constant-based scoring with 10x gaps.

```rust
pub const FIVE: i32 = 1_000_000;        // Win
pub const OPEN_FOUR: i32 = 100_000;     // Unstoppable
pub const CLOSED_FOUR: i32 = 50_000;    // Forcing
pub const OPEN_THREE: i32 = 10_000;     // Strong threat
pub const CLOSED_THREE: i32 = 1_000;    // Moderate
pub const OPEN_TWO: i32 = 500;          // Development
pub const CLOSED_TWO: i32 = 50;         // Minor
```

### Rationale
- **10x gaps**: Higher patterns always dominate any combination of lower patterns
- **FIVE = CAPTURE_WIN**: Both victory conditions have equal value (1,000,000)
- **Explicit constants**: No magic numbers, easy tuning and debugging

### Trade-offs

| Pros | Cons |
|------|------|
| Clear pattern priority | Coarse-grained tuning |
| Easy bug tracking (score → pattern) | Complex patterns hard to score |
| Consistent evaluation | Fixed gaps may miss nuances |
| Simple implementation | No learning/adaptation |

### Mathematical Guarantee
```
OPEN_FOUR (100,000) > 9 * OPEN_THREE (90,000)
→ One open four is always better than nine open threes
```

---

## 5. Heuristic Evaluation

### Decision A: Defense Multiplier 1.5x

```rust
pub const DEFENSE_MULTIPLIER: f32 = 1.5;
```

### Rationale
- Prevents undervaluing opponent threats
- "Not losing" prioritized over "winning faster"
- Compensates for search horizon limitations

### Trade-offs

| Pros | Cons |
|------|------|
| Stable defensive play | May miss aggressive wins |
| Harder to beat | More conservative style |
| Fewer blunders | Longer games |

---

### Decision B: Line Starting Point Evaluation

```rust
fn evaluate_line(board, pos, dr, dc, color) -> i32 {
    // Skip if not line start (same color stone in negative direction)
    let prev_r = pos.row as i32 - dr;
    let prev_c = pos.col as i32 - dc;
    if board.get(prev_pos) == Some(color) {
        return 0;  // Not a starting point, avoid double counting
    }
    // Count only in positive direction...
}
```

### Rationale
- **Bug fix**: Prevents 3-stone line counted 3 times
- **Accuracy**: Each line segment evaluated exactly once
- **Efficiency**: Simple check, no complex segment tracking

### Trade-offs

| Pros | Cons |
|------|------|
| Accurate pattern scores | Direction check on every stone |
| Simple logic | Slightly more computation |
| No double counting | Must check all 4 directions |

---

## 6. VCF/VCT Threat Search

### Decision A: Depth Limits

```rust
max_vcf_depth: 30  // VCF: Continuous four threats only
max_vct_depth: 20  // VCT: All threats (fours + open threes)
```

### Rationale
- **VCF high depth**: Highly forcing (only fours), narrow branching
- **VCT lower depth**: Wider branching (includes open threes), exponential growth
- **Balance**: Catches most forced wins without timeout

### Trade-offs

| Pros | Cons |
|------|------|
| Finds most forced wins | Rare deep VCT missed |
| Reasonable computation time | Fixed limits may be suboptimal |
| Predictable performance | No adaptive depth |

---

### Decision B: Sparse Board Skip

```rust
// Skip VCT when stone_count < 8
if board.stone_count() >= 8 {
    let vct_result = self.threat_searcher.search_vct(board, color);
    // ...
}
```

### Rationale
- **Impossibility**: Meaningful VCT requires existing structure
- **Performance**: VCT on sparse boards is exponentially expensive
- **Evidence**: VCT on < 8 stones took 100+ seconds in testing

### Trade-offs

| Pros | Cons |
|------|------|
| Massive early-game speedup | Theoretical early VCT missed |
| Predictable response time | Hard-coded threshold |
| No practical impact | May need tuning for variants |

### Performance Evidence
```
Before: Opening moves ~180 seconds (VCT exploring empty board)
After:  Opening moves ~0.1 seconds (VCT skipped)
```

---

## 7. Transposition Table

### Decision
Use depth-based replacement policy.

```rust
let should_replace = match &self.entries[idx] {
    None => true,
    Some(e) => e.hash == hash || e.depth <= depth,
};
```

### Rationale
- **Same position**: Always update with latest information
- **Different position (collision)**: Keep deeper search results
- **Shallow results**: Cheap to recompute

### Trade-offs

| Pros | Cons |
|------|------|
| Preserves deep analysis | No recency consideration |
| Simple implementation | May evict useful shallow entries |
| Predictable behavior | No LRU optimization |
| Low overhead | Fixed replacement policy |

### Entry Structure
```rust
struct TTEntry {
    hash: u64,           // Zobrist hash for verification
    depth: u8,           // Search depth
    score: i32,          // Evaluation score
    flag: TTFlag,        // EXACT, LOWER_BOUND, UPPER_BOUND
    best_move: Option<Pos>,  // Best move found
}
```

---

## 8. Zobrist Hashing

### Decision
Use deterministic LCG (Linear Congruential Generator) for hash key generation.

```rust
fn lcg(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
    *seed
}
```

### Rationale
- **Fixed seed**: Consistent hashes across sessions (reproducible)
- **LCG speed**: Fast generation, sufficient distribution
- **XOR updates**: O(1) incremental hash updates

### Trade-offs

| Pros | Cons |
|------|------|
| Reproducible results | Not cryptographically secure |
| Fast hash generation | LCG period limitations |
| O(1) incremental updates | Potential hash clustering |
| Easy debugging | Fixed random sequence |

### Incremental Update
```rust
// Both place and remove use XOR (self-inverse)
pub fn update(&mut self, pos: Pos, stone: Stone) {
    self.hash ^= self.keys[stone as usize][pos.index()];
}
```

---

## 9. Move Ordering

### Decision
Multi-heuristic move ordering for Alpha-Beta efficiency.

```rust
struct MoveOrderer {
    killer_moves: Vec<[Option<Pos>; 2]>,  // Per-depth killers
    history: [[i32; 361]; 2],              // History heuristic
    countermoves: [Option<Pos>; 361],      // Counter-move heuristic
}
```

### Rationale
- **Killer moves**: Moves that caused cutoffs at same depth
- **History heuristic**: Accumulated success scores
- **Countermoves**: Effective responses to opponent moves
- **TT move**: Best move from previous search

### Move Priority Order
```
1. TT move (from transposition table)
2. Winning moves
3. Killer moves (depth-specific cutoff causers)
4. Countermoves
5. History-ordered remaining moves
```

### Trade-offs

| Pros | Cons |
|------|------|
| Better pruning efficiency | Memory overhead |
| Faster search | Update complexity |
| Adapts to position | May mis-order in new positions |

---

## 10. Capture Rules Implementation

### Decision
Implement Ninuki-renju capture with specific edge cases.

```rust
// Capture pattern: X-O-O-X
fn check_capture(board: &Board, pos: Pos, color: Stone) -> Vec<(Pos, Pos)> {
    let opponent = color.opponent();
    let mut captures = Vec::new();

    for (dr, dc) in DIRECTIONS {
        // Check pattern: self - opponent - opponent - self
        let p1 = pos.offset(dr, dc);
        let p2 = pos.offset(2*dr, 2*dc);
        let p3 = pos.offset(3*dr, 3*dc);

        if board.get(p1) == Some(opponent) &&
           board.get(p2) == Some(opponent) &&
           board.get(p3) == Some(color) {
            captures.push((p1, p2));
        }
    }
    captures
}
```

### Key Rules Implemented
1. **Pairs only**: Exactly 2 stones captured (not 1, not 3+)
2. **Safe placement**: Moving between opponent flankers is safe
3. **Capture count**: Track for victory condition (10 captures = win)
4. **Board reset**: Captured positions become empty

### Trade-offs

| Pros | Cons |
|------|------|
| Correct Ninuki-renju rules | Additional state tracking |
| Dynamic gameplay | Evaluation complexity |
| Strategic depth | Capture-break evaluation needed |

---

## Directory Structure

```
Gomoku/
├── Cargo.toml              # Package configuration
├── Makefile                # Build scripts
├── CLAUDE.md               # Claude Code instructions
├── README.md               # Project overview
├── src/
│   ├── lib.rs              # Library entry point
│   ├── main.rs             # CLI binary
│   ├── engine.rs           # AI engine integration
│   ├── board/
│   │   ├── mod.rs          # Module exports
│   │   ├── bitboard.rs     # 6 x u64 bitboard
│   │   └── board.rs        # Board struct
│   ├── rules/
│   │   ├── mod.rs          # Module exports
│   │   ├── capture.rs      # X-O-O-X capture
│   │   ├── win.rs          # Win conditions
│   │   └── forbidden.rs    # Double-three rule
│   ├── eval/
│   │   ├── mod.rs          # Module exports
│   │   ├── patterns.rs     # Score constants
│   │   └── heuristic.rs    # Evaluation function
│   └── search/
│       ├── mod.rs          # Module exports
│       ├── alphabeta.rs    # Alpha-Beta + ID
│       ├── threat.rs       # VCF/VCT search
│       ├── tt.rs           # Transposition Table
│       └── zobrist.rs      # Zobrist hashing
└── docs/
    └── ARCHITECTURE.md     # This file
```

---

## Testing Strategy

### Test Categories

| Category | Purpose | Example |
|----------|---------|---------|
| Unit tests | Individual function correctness | `test_bitboard_set_clear` |
| Rule tests | Game rule compliance | `test_capture_pair` |
| Search tests | Algorithm correctness | `test_vcf_finds_win` |
| Integration | Full pipeline behavior | `test_engine_finds_best_move` |
| Performance | Time constraints | `test_engine_time_reasonable` |

### Running Tests

```bash
# Full suite (release mode recommended)
cargo test --lib --release

# Specific module
cargo test --lib alphabeta
cargo test --lib threat

# With output
cargo test --lib -- --nocapture
```

### Test Statistics
- **Unit tests**: 160
- **Doc tests**: 11
- **Debug time**: ~31 seconds
- **Release time**: ~2.6 seconds

---

## Future Considerations

### Potential Improvements
1. **NNUE evaluation**: Neural network for position evaluation
2. **Monte Carlo**: MCTS for opening book generation
3. **Parallel search**: Multi-threaded Alpha-Beta (Lazy SMP)
4. **Opening book**: Pre-computed opening moves

### Known Limitations
1. **Fixed VCT depth**: May miss very deep forced wins
2. **No learning**: Static evaluation weights
3. **Single-threaded**: No parallel search
4. **No time management**: Fixed depth, no pondering
