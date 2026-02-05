# Gomoku AI Engine

[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/Tests-160%20passing-brightgreen.svg)]()
[![42 School](https://img.shields.io/badge/42-School%20Project-blue.svg)]()

> High-performance Rust-based Gomoku AI engine implementing Ninuki-renju rules with VCF/VCT threat search, Alpha-Beta pruning, and advanced position evaluation.

## Features

### Core Engine
- **19x19 board** with 6 x u64 Bitboard representation (O(1) operations)
- **VCF/VCT threat search** for forced win detection
- **Alpha-Beta pruning** with Iterative Deepening
- **Transposition Table** with Zobrist hashing
- **Move ordering** with killer moves, history heuristic, and countermoves

### Game Rules (Ninuki-renju)
- **Five-in-a-row** win condition (5+ consecutive stones)
- **Capture victory** (10 captured stones = win)
- **Pair capture** (`X-O-O-X` pattern removes the O-O pair)
- **Double-three forbidden** (creating two open-threes simultaneously is illegal)
- **Capture exception** (double-three via capture is allowed)

### Performance
- **< 0.5 seconds** average response time per move
- **Depth 10+** search capability
- **~15,000 NPS** (Nodes Per Second) in release mode
- **160 unit tests** with comprehensive coverage

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
# Run the CLI
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
│   ├── main.rs             # CLI binary entry point
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
│   └── search/             # Search algorithms
│       ├── mod.rs          # Module exports
│       ├── alphabeta.rs    # Alpha-Beta + Iterative Deepening
│       ├── threat.rs       # VCF/VCT threat space search
│       ├── tt.rs           # Transposition Table
│       └── zobrist.rs      # Zobrist hashing
│
└── docs/
    └── ARCHITECTURE.md     # Design decisions document
```

## Architecture Overview

### Search Priority Pipeline

The AI uses a 5-stage priority pipeline to find the best move:

```
┌─────────────────────────────────────────────────────────────┐
│  1. Immediate Win     → Can we win this turn?               │
│  2. VCF Search        → Victory by Continuous Fours         │
│  3. VCT Search        → Victory by Continuous Threats       │
│  4. Defense           → Block opponent's winning threats    │
│  5. Alpha-Beta        → General position evaluation         │
└─────────────────────────────────────────────────────────────┘
```

### Key Algorithms

| Algorithm | Purpose | Depth |
|-----------|---------|-------|
| VCF (Victory by Continuous Fours) | Find forced wins via consecutive four-threats | 30 |
| VCT (Victory by Continuous Threats) | Find forced wins via four + open-three threats | 20 |
| Alpha-Beta | General tree search with pruning | Variable |
| Iterative Deepening | Time management and move ordering | 1 → max |

### Pattern Scoring

```rust
FIVE:         1,000,000  // Winning position
OPEN_FOUR:      100,000  // Unstoppable threat
CLOSED_FOUR:     50,000  // Forcing move
OPEN_THREE:      10,000  // Strong threat
CLOSED_THREE:     1,000  // Moderate threat
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
cargo test --lib threat       # VCF/VCT search
cargo test --lib capture      # Capture rules
cargo test --lib forbidden    # Double-three rules
cargo test --lib heuristic    # Evaluation

# Single test
cargo test --lib test_five_in_row_wins

# With output
cargo test --lib -- --nocapture
```

### Test Statistics

| Metric | Value |
|--------|-------|
| Unit tests | 160 |
| Doc tests | 11 |
| Debug mode time | ~31 seconds |
| Release mode time | ~2.6 seconds |

## Performance

### Benchmarks

| Metric | Target | Actual |
|--------|--------|--------|
| Response time | < 0.5s | ✅ ~0.1-0.3s |
| Search depth | ≥ 10 | ✅ 12+ |
| Nodes/second | High | ~15,000 NPS |

### Optimizations

1. **Bitboard**: O(1) stone operations with bit manipulation
2. **Transposition Table**: Avoid re-evaluating known positions
3. **Move Ordering**: Killer moves, history heuristic for better pruning
4. **VCT Skip**: Skip expensive VCT on sparse boards (< 8 stones)
5. **Zobrist Hashing**: O(1) incremental hash updates

## Documentation

| Document | Description |
|----------|-------------|
| [README.md](README.md) | Project overview (this file) |
| [CLAUDE.md](CLAUDE.md) | Instructions for Claude Code AI assistant |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Design decisions with rationale |

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
