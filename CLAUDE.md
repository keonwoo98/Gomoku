# Gomoku AI Engine - Claude Code Instructions

> High-performance Rust-based Ninuki-renju Gomoku AI Engine

## Quick Reference

```bash
# Build
make                # Release build → ./Gomoku executable
make clean          # Remove target/ directory
make fclean         # clean + remove ./Gomoku
make re             # fclean + all

# Testing
make test           # Debug mode tests
make test-release   # Release mode tests (recommended)
cargo test --lib    # Direct cargo execution

# Specific module tests
cargo test --lib alphabeta    # Alpha-Beta search tests
cargo test --lib threat       # VCF/VCT threat search tests
cargo test --lib capture      # Capture rule tests
cargo test --lib forbidden    # Double-three rule tests
```

## Project Structure

```
Gomoku/
├── Cargo.toml              # Package configuration
├── Makefile                # Build scripts (all, clean, fclean, re, test)
├── CLAUDE.md               # Claude Code instructions (this file)
├── README.md               # Project overview
├── src/
│   ├── lib.rs              # Library entry point, module exports
│   ├── main.rs             # CLI binary entry point
│   ├── engine.rs           # AI engine integration (search pipeline)
│   ├── board/
│   │   ├── mod.rs          # Module definition
│   │   ├── bitboard.rs     # 6 x u64 bitboard implementation
│   │   └── board.rs        # Board struct with game state
│   ├── rules/
│   │   ├── mod.rs          # Module definition
│   │   ├── capture.rs      # X-O-O-X capture logic
│   │   ├── win.rs          # Win condition checking
│   │   └── forbidden.rs    # Double-three (33) forbidden move
│   ├── eval/
│   │   ├── mod.rs          # Module definition
│   │   ├── patterns.rs     # Score constants hierarchy
│   │   └── heuristic.rs    # Position evaluation function
│   └── search/
│       ├── mod.rs          # Module definition
│       ├── alphabeta.rs    # Alpha-Beta + Iterative Deepening
│       ├── threat.rs       # VCF/VCT threat space search
│       ├── tt.rs           # Transposition Table
│       └── zobrist.rs      # Zobrist hashing
└── docs/
    └── ARCHITECTURE.md     # Design decisions document
```

## Core Architecture

### Search Priority Pipeline

The AI engine uses a 5-stage priority pipeline for move selection:

```
┌─────────────────────────────────────────────────────────────┐
│  1. Immediate Win     → Check if we can win this turn       │
│  2. VCF Search        → Victory by Continuous Fours         │
│  3. VCT Search        → Victory by Continuous Threats       │
│         (only when stone_count >= 8)                        │
│  4. Defense           → Block opponent's winning threats    │
│  5. Alpha-Beta Search → General position evaluation         │
└─────────────────────────────────────────────────────────────┘
```

### Key Modules

| Module | Purpose | Key Files |
|--------|---------|-----------|
| `board` | 6 x u64 Bitboard representation (384 bits for 361 cells) | `bitboard.rs`, `board.rs` |
| `rules` | Game rules: capture (X-O-O-X), win conditions, forbidden moves | `capture.rs`, `win.rs`, `forbidden.rs` |
| `eval` | Heuristic evaluation with pattern scoring hierarchy | `patterns.rs`, `heuristic.rs` |
| `search` | Alpha-Beta + ID + TT, VCF/VCT threat search, Zobrist hashing | `alphabeta.rs`, `threat.rs`, `tt.rs`, `zobrist.rs` |
| `engine` | Integration layer orchestrating the search pipeline | `engine.rs` |

### Pattern Score Hierarchy

```rust
FIVE: i32 = 1_000_000;        // Winning condition
OPEN_FOUR: i32 = 100_000;     // Unstoppable threat
CLOSED_FOUR: i32 = 50_000;    // Forcing move
OPEN_THREE: i32 = 10_000;     // Strong threat
CLOSED_THREE: i32 = 1_000;    // Moderate threat
OPEN_TWO: i32 = 500;          // Development
CLOSED_TWO: i32 = 50;         // Minor development
```

**Design Principle**: 10x gaps ensure higher patterns always dominate lower pattern combinations.

---

## Game Rules (Ninuki-renju)

### Win Conditions

1. **Five or more in a row** - Align 5+ stones horizontally, vertically, or diagonally
2. **Capture victory** - Capture 10 opponent stones (5 pairs)

### Capture Rules

- **Pattern**: `X-O-O-X` → The O-O pair is captured and removed from board
- **Only pairs**: Exactly 2 consecutive stones can be captured (not 1, not 3+)
- **Safe placement**: Placing a stone BETWEEN opponent's flanking stones is SAFE
  - Example: `Blue-[empty]-Red-Blue` → Red can safely play at [empty]
- **Board reset**: Captured intersections become free to play again

### Double-Three Rule (Forbidden Move)

- **Free-three**: 3 aligned stones that can become an unstoppable open-four
  - Consecutive: `_-O-O-O-_` (both ends open)
  - Spaced: `_-O-O-_-O-_` (both ends open)
- **Forbidden**: A move creating TWO free-threes simultaneously is **illegal**
- **Exception**: Double-three created via capture IS allowed

### Endgame Capture Rule

- Five-in-a-row wins **ONLY IF** opponent cannot break it by capturing a pair
- If a player has lost 4 pairs (8 stones) and opponent can capture 5th pair → opponent wins
- This creates tension between aggressive play and defensive awareness

---

## Development Guidelines

### Sub-Agent Usage Guide

When working on this project, leverage specialized sub-agents for optimal results:

| Task Type | Recommended Sub-Agent | When to Use |
|-----------|----------------------|-------------|
| **Codebase Exploration** | `Explore` | Understanding code structure, finding patterns, answering "where is X?" |
| **Implementation Planning** | `Plan` | Designing architecture, planning implementation strategy |
| **Code Review** | `superpowers:code-reviewer` | Reviewing completed implementations against spec |
| **Rust Development** | `voltagent-lang:rust-engineer` | Rust-specific development, ownership patterns, lifetimes |
| **Performance Optimization** | `voltagent-qa-sec:performance-engineer` | Profiling, bottleneck analysis, optimization |
| **Test Automation** | `voltagent-qa-sec:test-automator` | Writing tests, improving coverage |
| **Security Audit** | `voltagent-qa-sec:security-auditor` | Security vulnerability assessment |
| **Algorithm Design** | `voltagent-data-ai:ai-engineer` | AI/ML algorithm design, search optimization |
| **Database Design** | `voltagent-data-ai:postgres-pro` | If adding persistence layer |
| **Documentation** | `voltagent-dev-exp:documentation-engineer` | Technical documentation |
| **Debugging** | `voltagent-qa-sec:debugger` | Complex bug investigation |
| **Refactoring** | `voltagent-dev-exp:refactoring-specialist` | Code quality improvements |

### Skills Usage Guide

Leverage these skills for common workflows:

| Skill | Purpose | When to Use |
|-------|---------|-------------|
| `/commit` | Create git commits | After completing a feature or fix |
| `/review-pr` | Review pull requests | Before merging changes |
| `superpowers:writing-plans` | Create implementation plans | Before starting multi-step features |
| `superpowers:subagent-driven-development` | Execute plans with subagents | Implementing planned features |
| `superpowers:executing-plans` | Execute plans in separate session | Large feature implementations |
| `superpowers:systematic-debugging` | Debug issues methodically | When encountering bugs |
| `superpowers:test-driven-development` | TDD workflow | Before writing implementation |
| `superpowers:verification-before-completion` | Verify work is complete | Before claiming completion |
| `superpowers:requesting-code-review` | Request code review | After completing implementation |
| `superpowers:brainstorming` | Explore requirements | Before creative work |
| `feature-dev:feature-dev` | Guided feature development | New feature implementation |
| `code-review:code-review` | Formal code review | PR review process |

### Workflow Examples

#### Adding a New Feature
```
1. Use `superpowers:brainstorming` to explore requirements
2. Use `Plan` sub-agent to design implementation
3. Use `superpowers:writing-plans` to document the plan
4. Use `superpowers:subagent-driven-development` to execute
5. Use `voltagent-qa-sec:test-automator` for test coverage
6. Use `superpowers:verification-before-completion` to verify
7. Use `/commit` to commit changes
```

#### Debugging a Performance Issue
```
1. Use `superpowers:systematic-debugging` to investigate
2. Use `voltagent-qa-sec:performance-engineer` for profiling
3. Use `Explore` sub-agent to find related code
4. Use `voltagent-lang:rust-engineer` for Rust-specific fixes
5. Run `make test-release` to verify fix
```

#### Code Quality Improvement
```
1. Use `Explore` to understand current structure
2. Use `voltagent-dev-exp:refactoring-specialist` for refactoring
3. Use `superpowers:code-reviewer` to review changes
4. Use `voltagent-qa-sec:test-automator` to add tests
```

### Code Quality Principles

- **DRY** (Don't Repeat Yourself): Extract common functionality
- **YAGNI** (You Aren't Gonna Need It): Only implement current requirements
- **TDD** (Test-Driven Development): Write tests before implementation
- **SOLID**: Follow SOLID principles for maintainable code
- **Rust Idioms**: Use Rust idioms (ownership, borrowing, iterators)

### Before Committing

Always run these checks:
```bash
cargo clippy              # Lint checks
cargo fmt --check         # Format checks
make test-release         # All tests pass
```

---

## Performance Requirements

| Requirement | Target | Notes |
|-------------|--------|-------|
| AI Response Time | < 0.5 seconds average | Per move |
| Search Depth | Minimum 10 levels | For full validation |
| Algorithm | Min-Max with Alpha-Beta | Required |
| Timer Display | Mandatory | No timer = no validation |
| Debug Mode | Show AI reasoning | For defense sessions |

### Performance Optimizations in Place

1. **Bitboard representation**: O(1) stone placement/removal/checking
2. **Zobrist hashing**: O(1) incremental hash updates
3. **Transposition Table**: Avoid redundant position evaluations
4. **Iterative Deepening**: Time management with best-first ordering
5. **VCT skip on sparse boards**: Skip expensive VCT when `stone_count < 8`
6. **Move ordering**: Killer moves, history heuristic for better pruning

---

## Key Design Decisions

For detailed rationale and trade-offs, see `docs/ARCHITECTURE.md`:

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust (from Python) | 30-50x performance improvement |
| Board Representation | 6 x u64 Bitboard | O(1) operations, cache-friendly |
| Search Pipeline | 5-stage priority | Never miss forced wins |
| Pattern Scores | 10x hierarchical gaps | Clear priority ordering |
| Defense Multiplier | 1.5x | Prioritize defense over attack |
| VCF Depth | 30 levels max | High forcing, deep search ok |
| VCT Depth | 20 levels max | Wide branching, shallower |
| TT Replacement | Depth-based | Preserve deep search results |
| Zobrist | Deterministic LCG | Reproducible debugging |

---

## Testing

### Test Statistics
- **Unit tests**: 160 tests
- **Doc tests**: 11 tests
- **Debug mode**: ~31 seconds
- **Release mode**: ~2.6 seconds

### Running Tests

```bash
# Full test suite (recommended)
cargo test --lib --release

# Debug mode (slower but better error messages)
cargo test --lib

# Specific module
cargo test --lib alphabeta      # Alpha-Beta search
cargo test --lib threat         # VCF/VCT search
cargo test --lib capture        # Capture rules
cargo test --lib forbidden      # Double-three rules
cargo test --lib heuristic      # Evaluation function
cargo test --lib zobrist        # Hash function

# Single test
cargo test --lib test_five_in_row_wins

# With output
cargo test --lib -- --nocapture
```

### Test Categories

| Category | Location | Purpose |
|----------|----------|---------|
| Board tests | `board/` | Bitboard operations, stone management |
| Rule tests | `rules/` | Capture, win detection, forbidden moves |
| Eval tests | `eval/` | Pattern detection, scoring accuracy |
| Search tests | `search/` | Alpha-Beta correctness, VCF/VCT |
| Integration | `engine.rs` | Full pipeline behavior |

---

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Test timeout | VCT on sparse board | Check `stone_count >= 8` guard |
| Slow debug tests | Debug mode overhead | Use `--release` for speed |
| Dead code warnings | Unused reserved fields | Add `#[allow(dead_code)]` |
| Hash collisions | TT size too small | Increase TT capacity |

### Debug Tips

```bash
# Run with verbose output
RUST_BACKTRACE=1 cargo test --lib -- --nocapture

# Run specific failing test
cargo test --lib test_name -- --nocapture

# Check for memory issues
cargo test --lib --release -- --test-threads=1
```

---

## Documentation

| Document | Purpose |
|----------|---------|
| [README.md](README.md) | Project overview and quick start |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Design decisions with rationale |
| [CLAUDE.md](CLAUDE.md) | Claude Code instructions (this file) |

---

## Contributing

1. Create a feature branch
2. Use TDD: write tests first
3. Follow Rust idioms and project patterns
4. Run `cargo clippy` and `cargo fmt`
5. Ensure all tests pass with `make test-release`
6. Use `/commit` for proper commit messages
7. Request code review with `superpowers:requesting-code-review`
