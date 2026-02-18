# Gomoku AI Engine

[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/Tests-196%20passing-brightgreen.svg)]()
[![42 School](https://img.shields.io/badge/42-School%20Project-blue.svg)]()

> High-performance Rust-based Gomoku AI engine implementing Ninuki-renju rules with Alpha-Beta pruning, Lazy SMP parallelism, VCF threat search, and advanced position evaluation.

## Features

### Core Engine
- **19x19 board** with 6 x u64 Bitboard representation (O(1) operations)
- **Alpha-Beta pruning** with Iterative Deepening and PVS (Principal Variation Search)
- **Lazy SMP** multi-threaded parallel search (lock-free transposition table)
- **VCF threat search** for forced win detection via continuous fours
- **Transposition Table** with Zobrist hashing (incremental O(1) updates)
- **Move ordering** with killer moves, history heuristic, and countermove heuristic
- **Dynamic heuristic** with game-phase detection (Opening/Midgame/Endgame weight adjustment)

### Search Optimizations
- **Null Move Pruning (NMP)** — skip turn + reduced search, 80% node reduction
- **Late Move Reduction (LMR)** — logarithmic reduction for quiet late moves
- **Late Move Pruning (LMP)** — skip quiet moves entirely at shallow depths
- **Futility Pruning** — skip non-tactical moves when eval + margin <= alpha
- **Reverse Futility Pruning** — static eval - margin >= beta cutoff
- **Razoring** — reduced search when static eval far below alpha
- **Aspiration Windows** — narrow search window with immediate re-search on fail
- **Internal Iterative Deepening (IID)** — shallow search for move ordering at depth >= 6
- **Threat Extensions** — extend search by 1 ply for forcing four-threats
- **VCF Quiescence Search** — extend leaf nodes for fives, fours, and capture-wins

### Game Rules (Ninuki-renju)
- **Five-in-a-row** win condition (5+ consecutive stones)
- **Capture victory** (10 captured stones = win)
- **Pair capture** (`X-O-O-X` pattern removes the O-O pair)
- **Breakable five** rule (five-in-a-row only wins if opponent can't break it via capture)
- **Illusory break detection** (break that leads to unbreakable recreation = forced win)
- **Double-three forbidden** (creating two open-threes simultaneously is illegal)
- **Capture exception** (double-three via capture is allowed)
- **Opening rules** support (Standard, Pro, Swap)
- **AI vs AI** spectator mode with full debug panel

### Performance
- **< 0.5 seconds** average response time per move
- **Depth 10-17** search capability depending on position complexity
- **~1,000K+ NPS** (Nodes Per Second) in release mode with Lazy SMP
- **196 unit tests** with comprehensive coverage

## Requirements

- **Rust 1.70+** (with Cargo)
- **Make** (for build scripts)

## Quick Start

### Build

```bash
# Release build (creates ./Gomoku executable)
make

# Or using cargo directly
cargo build --release
```

### Run

```bash
# Run the GUI application
./Gomoku

# Or using cargo
cargo run --release
```

### Test

```bash
# Release mode tests (recommended - faster)
make test-release

# Debug mode tests (better error messages)
make test

# Or using cargo directly
cargo test --lib --release
```

## Build Commands

| Command | Description |
|---------|-------------|
| `make` | Build release binary → `./Gomoku` |
| `make clean` | Remove `target/` directory |
| `make fclean` | `clean` + remove `./Gomoku` |
| `make re` | `fclean` + `all` |
| `make test` | Run tests in debug mode |
| `make test-release` | Run tests in release mode |

## Project Structure

```
Gomoku/
├── Cargo.toml              # Package configuration
├── Makefile                # Build scripts (all, clean, fclean, re, test)
├── CLAUDE.md               # Claude Code AI assistant instructions
├── README.md               # This file
│
├── src/
│   ├── lib.rs              # Library entry point, module exports
│   ├── main.rs             # GUI binary entry point
│   ├── engine.rs           # AI engine integration layer
│   │
│   ├── board/              # Board representation
│   │   ├── mod.rs          # Module exports
│   │   ├── bitboard.rs     # 6 x u64 bitboard implementation
│   │   └── board.rs        # Board struct with game state
│   │
│   ├── rules/              # Game rules
│   │   ├── mod.rs          # Module exports
│   │   ├── capture.rs      # X-O-O-X capture logic
│   │   ├── win.rs          # Win condition checking
│   │   └── forbidden.rs    # Double-three (33) rule
│   │
│   ├── eval/               # Position evaluation
│   │   ├── mod.rs          # Module exports
│   │   ├── patterns.rs     # Score constants hierarchy
│   │   └── heuristic.rs    # Heuristic evaluation function
│   │
│   ├── search/             # Search algorithms
│   │   ├── mod.rs          # Module exports
│   │   ├── alphabeta.rs    # Alpha-Beta + Iterative Deepening + Lazy SMP
│   │   ├── threat.rs       # VCF threat space search
│   │   ├── tt.rs           # Transposition Table (lock-free AtomicTT)
│   │   └── zobrist.rs      # Zobrist hashing
│   │
│   └── ui/                 # GUI application
│       ├── mod.rs          # Module exports
│       ├── app.rs          # Main application and side panel
│       ├── board_view.rs   # Board rendering and interaction
│       ├── game_state.rs   # Game state management
│       └── theme.rs        # Color constants and theming
│
└── docs/
    ├── CODEBASE_GUIDE.md   # Complete codebase documentation
    └── DEFENSE.md          # Defense session preparation
```

## Architecture Overview

### Search Priority Pipeline

The AI uses a 6-stage priority pipeline to find the best move:

```
┌─────────────────────────────────────────────────────────────┐
│  0. Opening Book   → Pre-computed early game moves           │
│  0.5 Break Five    → Break opponent's existing five          │
│  1. Immediate Win  → Can we win this turn? (+ illusory break)│
│  2. Defense        → Block opponent's winning threats         │
│  3. VCF Search     → Victory by Continuous Fours             │
│  4. Opponent VCF   → Block opponent's forced win             │
│  5. Alpha-Beta     → General position evaluation             │
└─────────────────────────────────────────────────────────────┘
```

### Key Algorithms

| Algorithm | Purpose | Details |
|-----------|---------|---------|
| Alpha-Beta + PVS | General tree search with pruning | Depth 10-17, Lazy SMP parallel |
| VCF | Find forced wins via consecutive four-threats | Depth 30 max |
| Iterative Deepening | Time management and move ordering | 1 → max with 2-depth win confirmation |
| Lazy SMP | Multi-threaded search | Lock-free AtomicTT, auto-detect cores |
| VCF Quiescence | Extend leaf nodes for tactical moves | Fives, fours, capture-wins |

### Pattern Scoring

```rust
FIVE:         1,000,000  // Winning position
OPEN_FOUR:      100,000  // Unstoppable threat
CLOSED_FOUR:     50,000  // Forcing move
OPEN_THREE:      10,000  // Strong threat
CLOSED_THREE:     1,500  // Moderate threat
OPEN_TWO:           500  // Development
CLOSED_TWO:          50  // Minor development
```

## Game Rules

### Win Conditions

1. **Five-in-a-row**: Align 5 or more stones horizontally, vertically, or diagonally
2. **Capture victory**: Capture 10 opponent stones (5 pairs)

### Capture Mechanic

```
Pattern: X - O - O - X
         ↑   └─┬─┘   ↑
      Your   Captured  Your
      stone   pair    stone
```

- Only **pairs** (exactly 2 stones) can be captured
- Placing a stone **between** opponent's flanking stones is **safe**
- Captured intersections become **empty** and playable again

### Forbidden Moves (Double-Three)

Creating two "open threes" simultaneously is **forbidden**:

```
Open Three: _ O O O _  (both ends empty)
            _ O O _ O _ (spaced, both ends empty)
```

**Exception**: If the move also creates a capture, double-three is **allowed**.

### Endgame Capture Rule

- Five-in-a-row wins **only if** opponent cannot break it via capture
- If a break capture removes a bracket stone, the five-holder can replay to create an **unbreakable** five (illusory break)
- If you've lost 8 stones (4 pairs) and opponent can capture your 5th pair → opponent wins

## Testing

### Run All Tests

```bash
# Recommended: Release mode (faster)
cargo test --lib --release

# Debug mode (better error messages)
cargo test --lib
```

### Run Specific Tests

```bash
# By module
cargo test --lib alphabeta    # Alpha-Beta search
cargo test --lib threat       # VCF threat search
cargo test --lib capture      # Capture rules
cargo test --lib forbidden    # Double-three rules
cargo test --lib heuristic    # Evaluation
cargo test --lib win          # Win conditions

# Single test
cargo test --lib test_five_in_row_wins

# With output
cargo test --lib -- --nocapture
```

### Test Statistics

| Metric | Value |
|--------|-------|
| Unit tests | 196 |
| Doc tests | 11 |
| Release mode time | ~0.7 seconds |

## Performance

### Benchmarks

| Metric | Target | Actual |
|--------|--------|--------|
| Response time | < 0.5s | ~0.3-0.5s |
| Search depth | >= 10 | 10-17 |
| Nodes/second | High | ~1,000K+ NPS |

### Optimizations

1. **Bitboard**: O(1) stone operations with 6 x u64 bit manipulation
2. **Lazy SMP**: Lock-free parallel search with AtomicTT (XOR trick)
3. **Transposition Table**: Avoid re-evaluating known positions
4. **Move Ordering**: Killer moves, history heuristic, countermove heuristic
5. **Null Move Pruning**: 80% node reduction by skipping turns
6. **Late Move Reduction/Pruning**: Reduce or skip quiet moves at late positions
7. **Futility/Razoring/RFP**: Prune hopeless positions at shallow depths
8. **VCF Quiescence**: Extend search at leaf nodes for tactical moves
9. **Threat Extensions**: Extra ply for forcing four-threats
10. **Zobrist Hashing**: O(1) incremental hash updates
11. **Make/Unmake Pattern**: No board cloning per search node
12. **Allocation-free Captures**: Fixed-size arrays for capture info
13. **Dynamic Heuristic**: Game-phase-aware evaluation weights (Opening/Midgame/Endgame)

## Documentation

| Document | Description |
|----------|-------------|
| [README.md](README.md) | Project overview (this file) |
| [CLAUDE.md](CLAUDE.md) | Instructions for Claude Code AI assistant |
| [CODEBASE_GUIDE.md](docs/CODEBASE_GUIDE.md) | Complete codebase documentation |
| [DEFENSE.md](docs/DEFENSE.md) | Defense session preparation |

## Development

### Code Quality

```bash
# Lint check
cargo clippy

# Format check
cargo fmt --check

# Format code
cargo fmt
```

### Adding Features

1. Write tests first (TDD)
2. Implement the feature
3. Run `cargo clippy` and `cargo fmt`
4. Run `make test-release`
5. Update documentation if needed

## License

42 School Project

## Acknowledgments

- 42 School for the project specification
- Rust community for excellent tooling
- Chess programming community for search algorithm inspiration
