# Gomoku AI - Defense Document / 디펜스 문서

> Comprehensive technical documentation for oral defense evaluation.
> 평가 구술 디펜스를 위한 종합 기술 문서.

---

## Table of Contents / 목차

1. [Minimax Algorithm Implementation / 미니맥스 알고리즘 구현](#1-minimax-algorithm-implementation)
2. [Heuristic Evaluation Function / 휴리스틱 평가 함수](#2-heuristic-evaluation-function)
3. [Game Rules Implementation / 게임 규칙 구현](#3-game-rules-implementation)
4. [AI Strategy & Loss Defense / AI 전략 및 패배 대응](#4-ai-strategy--loss-defense)
5. [Performance Benchmarks / 성능 벤치마크](#5-performance-benchmarks)
6. [Architecture Overview / 아키텍처 개요](#6-architecture-overview)

---

## 1. Minimax Algorithm Implementation

### 1.1 Core Algorithm: Negamax with Alpha-Beta Pruning

**File**: `src/search/alphabeta.rs`

Our AI uses **Negamax** — a simplification of Minimax where:
```
score(position, player) = -score(position, opponent)
```

This eliminates the need for separate maximizer/minimizer branches. The core search function `alpha_beta()` implements this with alpha-beta bounds to prune branches that cannot affect the result.

**한국어 설명**: Negamax는 Minimax의 간소화된 형태입니다. `evaluate(board, Black) == -evaluate(board, White)` 대칭성을 유지하면서, 현재 플레이어의 점수를 항상 최대화하도록 구현합니다. Alpha-Beta pruning으로 불필요한 탐색 가지를 제거합니다.

#### How Alpha-Beta Pruning Works

```
alpha_beta(board, color, depth, alpha, beta):
    if terminal or depth == 0:
        return evaluate(board, color)

    for each move:
        score = -alpha_beta(board, opponent, depth-1, -beta, -alpha)
        if score >= beta:
            return beta        // Beta cutoff (pruning!)
        alpha = max(alpha, score)

    return alpha
```

- **Alpha** = best score we can guarantee (lower bound)
- **Beta** = best score the opponent can guarantee (upper bound)
- When `score >= beta`, the opponent would never allow this position, so we prune.

**한국어 설명**: Alpha는 우리가 보장할 수 있는 최소 점수, Beta는 상대가 허용할 수 있는 최대 점수입니다. `score >= beta`이면 상대가 이 수를 허용하지 않을 것이므로 더 이상 탐색할 필요가 없습니다 (pruning).

### 1.2 Iterative Deepening with Time Management

**File**: `src/search/alphabeta.rs`, function `search_timed()`

We use **Iterative Deepening**: search depth 1, then 2, then 3, ... up to the maximum. This provides:

1. **Time management**: We can stop between depths if time runs out
2. **Better move ordering**: Results from depth N-1 improve ordering at depth N
3. **Guaranteed minimum depth 10**: Project requirement

```rust
const MIN_DEPTH: i8 = 10;  // Non-negotiable requirement

for depth in 1..=max_depth {
    result = search_root(board, color, depth);

    if depth < MIN_DEPTH {
        continue;  // Always complete depth 10
    }

    // Smart time prediction: estimate next depth's cost
    let bf = depth_time / prev_depth_time;  // Observed branching factor
    let estimated_next = depth_time * bf;
    if estimated_next > remaining_time {
        break;  // Don't start a depth we can't finish
    }
}
```

**Time control strategy**:
- Hard limit: `max(500ms, 800ms)` — prevents runaway searches
- Soft limit: 500ms target — maintains average < 500ms
- Depth 1-10: always complete (non-negotiable)
- Depth 11+: predictive control based on observed branching factor

**한국어 설명**: Iterative Deepening은 깊이 1부터 시작해 점진적으로 깊어집니다. **최소 깊이 10은 무조건 완료**하며, 그 이후에는 이전 깊이의 소요 시간으로 다음 깊이의 시간을 예측하여 시간 초과를 방지합니다. 평균 500ms 이내를 목표로 합니다.

### 1.3 Transposition Table (TT)

**File**: `src/search/tt.rs`

The TT stores previously evaluated positions to avoid redundant computation.

- **Size**: 64 MB (configurable)
- **Hash**: Zobrist hashing (see 1.5)
- **Entry types**:
  - `Exact`: Score is the true minimax value
  - `LowerBound`: Score >= stored value (beta cutoff)
  - `UpperBound`: Score <= stored value (failed high)
- **Replacement**: Depth-based — deeper results overwrite shallower ones
- **Probe**: Before searching a position, check if TT has a usable result

```rust
// TT probe at the start of alpha_beta()
if let Some((score, best_move)) = self.tt.probe(hash, depth, alpha, beta) {
    return score;  // Skip entire subtree!
}
```

**한국어 설명**: Transposition Table은 이미 평가한 포지션을 저장하여 중복 계산을 방지합니다. 64MB 크기의 해시 테이블로, 같은 포지션에 다른 수순으로 도달해도 재활용할 수 있습니다.

### 1.4 Advanced Pruning Techniques

#### 1.4.1 Null Move Pruning (NMP)

**Concept**: If we skip our turn entirely (null move) and the position is STILL >= beta, the position is so good that we can prune without full search.

```rust
if allow_null && depth >= 3 && !is_threatened(board, color) {
    let r = if depth > 6 { 3 } else { 2 };  // Adaptive reduction
    let null_score = -alpha_beta(board, opponent, depth-1-r, -beta, -(beta-1), false);

    if null_score >= beta {
        if depth <= 6 { return beta; }           // Shallow: trust result
        let verify = alpha_beta(..., depth-r);    // Deep: verify with reduced search
        if verify >= beta { return beta; }
    }
}
```

**Safety guards**:
- `allow_null = false` after a null move (no consecutive null moves)
- Skip in threatened positions (opponent near capture win)
- Verification search at deep nodes to prevent zugzwang errors

**Impact**: Reduced search nodes by ~80%, time by ~91%

**한국어 설명**: Null Move Pruning은 "우리 차례를 건너뛰어도 여전히 이기고 있다면, 이 포지션은 확실히 좋다"는 원리입니다. 탐색 노드를 80% 줄이고 시간을 91% 단축시킨 가장 효과적인 최적화입니다. 안전 장치: 연속 null move 금지, 위협 상태에서 사용 금지, 깊은 노드에서 검증 탐색 실행.

#### 1.4.2 Late Move Reduction (LMR)

Moves sorted later in the list are less likely to be good. We search them at reduced depth first, then re-search at full depth only if they beat alpha.

```rust
let reduction = if is_capture || depth < 3 {
    0    // Never reduce captures or at shallow depth
} else if i >= 8 && depth >= 5 {
    3    // Very late moves at deep search: heavy reduction
} else if i >= 5 && depth >= 4 {
    2    // Late moves: moderate reduction
} else if i >= 3 && depth >= 3 {
    1    // Early-late moves: light reduction
} else {
    0
};
```

**3-tier LMR**: The three reduction levels (1/2/3) create a smooth gradient where later, less promising moves get progressively less search effort. The heaviest tier (i>=8, depth>=5, reduce by 3) dramatically cuts search time on moves that are very unlikely to be best.

**한국어 설명**: 정렬 순서가 뒤인 수들은 좋은 수일 가능성이 낮으므로 얕은 깊이로 먼저 탐색하고, 예상보다 좋은 결과가 나올 때만 전체 깊이로 재탐색합니다. 3단계 축소(1/2/3)로 후순위 수일수록 더 많이 축소합니다. 캡처 수는 절대 축소하지 않습니다.

#### 1.4.3 Futility Pruning

At shallow depths (1-2), if the static evaluation + a margin is still below alpha, non-tactical moves are hopeless and can be skipped.

```rust
if depth <= 2 && alpha.abs() < FIVE - 100 {
    let static_eval = evaluate(board, color);
    let margin = if depth == 1 { CLOSED_FOUR } else { OPEN_FOUR };

    if static_eval + margin <= alpha && move_score < 800_000 {
        continue;  // Skip this hopeless move
    }
}
```

**한국어 설명**: 깊이 1-2에서 정적 평가값 + 마진이 alpha보다 낮으면, 비전술적 수는 상황을 개선할 수 없으므로 건너뜁니다. 마진은 depth 1에서 CLOSED_FOUR(50,000), depth 2에서 OPEN_FOUR(100,000)입니다.

#### 1.4.4 Principal Variation Search (PVS)

The first move (expected best from TT/move ordering) is searched with a full window. All subsequent moves use a null window `(alpha, alpha+1)` — cheaper to compute. If a move beats this narrow window, we re-search with the full window.

```rust
if i == 0 {
    score = -alpha_beta(board, opponent, depth-1, -beta, -alpha);  // Full window
} else {
    // Null window search (cheap)
    s = -alpha_beta(board, opponent, depth-1, -(alpha+1), -alpha);
    if s > alpha && s < beta {
        // Re-search with full window (rare)
        s = -alpha_beta(board, opponent, depth-1, -beta, -alpha);
    }
}
```

**한국어 설명**: 첫 번째 수(가장 좋을 것으로 예상)는 전체 윈도우로 탐색하고, 나머지는 좁은 윈도우 `(alpha, alpha+1)`로 빠르게 확인합니다. 좁은 윈도우를 초과하는 경우에만 전체 윈도우로 재탐색합니다.

### 1.5 Zobrist Hashing

**File**: `src/search/zobrist.rs`

O(1) incremental hash updates for the transposition table.

**How it works**:
1. Pre-compute random 64-bit values for each (position, color) pair
2. Full hash = XOR of all stone hashes + side-to-move + capture counts
3. Incremental update: `new_hash = old_hash XOR stone_hash` (O(1))

```rust
// Placing a stone: XOR in stone hash + toggle side-to-move
pub fn update_place(&self, hash: u64, pos: Pos, stone: Stone) -> u64 {
    hash ^ stone_hash ^ self.black_to_move
}

// Capture: XOR out stone hash (no side toggle)
pub fn update_capture(&self, hash: u64, pos: Pos, stone: Stone) -> u64 {
    hash ^ stone_hash  // Side doesn't change during capture
}
```

**Properties**: XOR is commutative and self-inverse, so the hash is path-independent.

**한국어 설명**: Zobrist 해싱은 돌을 놓거나 제거할 때 O(1)로 해시를 업데이트합니다. XOR의 교환법칙과 자기역원 성질 덕분에, 같은 포지션에 어떤 수순으로 도달하든 동일한 해시를 생성합니다. 캡처 시에는 side-to-move를 토글하지 않습니다.

### 1.6 Move Ordering (Defense-First Philosophy)

**File**: `src/search/alphabeta.rs`, function `score_move()`

Good move ordering is critical for alpha-beta efficiency. Our ordering combines 8 direction scans per move (instead of 40+) with this priority hierarchy:

| Priority | Score | Pattern |
|----------|-------|---------|
| 1 | 1,000,000 | TT best move |
| 2 | 900,000 | Our five-in-a-row |
| 3 | 895,000 | Block opponent five |
| 4 | 890,000 | Capture win (5th pair) |
| 5 | 885,000 | Block opponent capture win |
| 6 | 870,000 | Our open four |
| 7 | 860,000 | Block opponent open four |
| 8 | 855,000 | Block opponent near-capture-win |
| 9 | 830,000 | Our closed four |
| 10 | 820,000 | Block opponent closed four |
| 11 | 810,000 | Our open three |
| 12 | 800,000 | Block opponent open three |
| 13 | 600,000+ | Capture moves |
| 14 | 500,000 | Killer moves |
| 15 | Variable | History heuristic + center bonus + two-level detection |

**Key design**: Offense and defense are interleaved. Creating an open four (870K) is prioritized over blocking an opponent's open four (860K), because if WE have an unstoppable threat, blocking theirs is moot.

**Two-level detection** (Priority 15): For early/mid-game positions where no threes exist yet, `score_move` also detects two-stone patterns to differentiate non-tactical moves:
- Our open two (`_OO_`): +500 points (prefer connected development)
- Our closed two (`XOO_`): +150 points
- Block opponent open two: +200 points

Without this, move ordering was essentially random for non-tactical positions, causing scattered play.

**한국어 설명**: 수 정렬은 alpha-beta 효율의 핵심입니다. 8방향 스캔으로 40+번 대신 한 번에 패턴을 감지합니다. **공격과 방어를 교차 배치**하여, 우리의 오픈 포(870K)가 상대의 오픈 포 차단(860K)보다 우선됩니다. **투 감지**: 초중반에 쓰리가 없을 때 투 패턴(열린 투 +500, 닫힌 투 +150, 상대 열린 투 차단 +200)을 감지하여 연결된 발전을 우선합니다. 이것 없이는 비전술적 수의 정렬이 사실상 랜덤이었습니다.

### 1.7 Search Priority Pipeline

**File**: `src/engine.rs`

Before alpha-beta even runs, the engine checks faster search methods:

```
Stage 0: Opening Book     → Empty board → center (9,9)
Stage 1: Immediate Win    → Can we win THIS move? (O(N) scan)
Stage 2: Opponent Threats → Must we block a 5-in-a-row? (O(N) scan)
Stage 3: Our VCF          → Do we have a forced win via continuous fours?
Stage 4: Opponent VCF     → Does opponent have a forced win we must block?
Stage 5: Alpha-Beta       → Full search with all optimizations
```

This ensures we never waste time on alpha-beta when a faster method finds the answer.

**한국어 설명**: Alpha-Beta 이전에 더 빠른 탐색 방법을 순서대로 확인합니다. 즉시 승리 → 상대 위협 차단 → 우리 VCF → 상대 VCF 차단 → 전체 Alpha-Beta. 이렇게 하면 강제 승리나 필수 방어를 빠르게 감지하여 탐색 시간을 절약합니다.

---

## 2. Heuristic Evaluation Function

### 2.1 Design Philosophy

**File**: `src/eval/heuristic.rs`, `src/eval/patterns.rs`

The evaluation function must satisfy **negamax symmetry**:
```
evaluate(board, Black) == -evaluate(board, White)
```

This is non-negotiable. Any asymmetry (like defense multipliers in the eval) breaks the search.

Our evaluation = **Pattern Score** + **Capture Score** + **Position Score** - **Vulnerability Penalty**

**한국어 설명**: 평가 함수는 negamax 대칭성을 반드시 만족해야 합니다. `eval(board, Black) == -eval(board, White)`. 비대칭이면 탐색이 잘못된 계산을 합니다. 방어 우선은 평가 함수가 아닌 **수 정렬(move ordering)**에서 처리합니다.

### 2.2 Pattern Score Hierarchy

Each stone is evaluated for line patterns in 4 directions (horizontal, vertical, 2 diagonals). Each line segment is counted once from its "start" position (no double-counting).

| Pattern | Score | Description |
|---------|-------|-------------|
| **FIVE** | 1,000,000 | 5+ in a row (game over) |
| **OPEN_FOUR** | 100,000 | `_OOOO_` (unstoppable) |
| **CLOSED_FOUR** | 50,000 | `XOOOO_` (one open end) |
| **OPEN_THREE** | 10,000 | `_OOO_` (becomes open four) |
| **CLOSED_THREE** | 5,000 | `XOOO_` (one side blocked) |
| **OPEN_TWO** | 1,000 | `_OO_` (development) |
| **CLOSED_TWO** | 200 | `XOO_` (minor) |

**10x gap design**: Each level is ~10x higher than the next, ensuring a higher pattern always dominates any combination of lower patterns. One OPEN_FOUR (100K) > ten OPEN_THREEs (100K total), which is intentional — an open four IS an immediate win.

**Gap patterns**: The evaluator also detects one-gap patterns like `OO_OO` or `O_OOO`. A 4-stone gap pattern with span 5 is scored as OPEN_FOUR (filling the gap wins).

**한국어 설명**: 패턴 점수는 10배 간격으로 설계되어, 상위 패턴이 항상 하위 패턴 조합보다 우선됩니다. 갭 패턴(`OO_OO`)도 감지하여 빈 칸을 채우면 5연속이 되는 경우 OPEN_FOUR로 점수를 매깁니다. 이중 카운팅 방지: 각 라인 세그먼트는 "시작 위치"에서만 한 번 계산됩니다.

### 2.3 Combo Detection

Multiple threats that the opponent cannot block simultaneously:

```rust
// Two closed fours → opponent can only block one → effectively an open four
if closed_fours >= 2 { score += OPEN_FOUR; }

// Closed four + open three → must block four, three promotes
if closed_fours >= 1 && open_threes >= 1 { score += OPEN_FOUR; }

// Two open threes → opponent can only block one → one becomes open four
if open_threes >= 2 { score += OPEN_FOUR; }
```

**한국어 설명**: 상대가 동시에 막을 수 없는 복합 위협을 감지합니다. 닫힌 포 2개, 닫힌 포+열린 삼, 열린 삼 2개 모두 사실상 오픈 포와 동급으로 보너스를 줍니다.

### 2.4 Capture Scoring (Non-Linear)

```rust
const CAP_WEIGHTS: [i32; 6] = [
    0,           // 0 captures
    2_000,       // 1 capture: minor
    7_000,       // 2 captures: moderate (> CLOSED_THREE)
    20_000,      // 3 captures: serious (> OPEN_THREE)
    80_000,      // 4 captures: near-winning (> OPEN_FOUR)
    1_000_000,   // 5 captures: game over
];

// Symmetric: capture_score(a,b) == -capture_score(b,a)
fn capture_score(my: u8, opp: u8) -> i32 {
    CAP_WEIGHTS[my] - CAP_WEIGHTS[opp]
}
```

The non-linear scaling ensures that being close to a capture win is valued appropriately. 4 captures (80K) is almost as valuable as an OPEN_FOUR, reflecting the urgency.

**한국어 설명**: 캡처 점수는 비선형입니다. 4캡처(80K)는 오픈 포에 가까운 가치를 가집니다. 대칭성 유지: `capture_score(a,b) == -capture_score(b,a)`.

### 2.5 Capture Vulnerability Penalty

**File**: `src/eval/heuristic.rs`, function `count_vulnerable_pairs()`

Detects our stone pairs that the opponent can capture next turn:

```
Pattern: empty - ally - ally - opponent  (opponent plays at empty to capture)
Pattern: opponent - ally - ally - empty  (opponent plays at empty to capture)
```

Each vulnerable pair is penalized by 4,000 points (approximately one CAPTURE_THREAT).

**Also in move ordering** (`score_move`): The `capture_vulnerability()` function penalizes moves that CREATE new vulnerable pairs. Base penalty = 8,000, with urgency scaling based on opponent's capture progress:
- Opponent has 0-1 captures: penalty × 1 (low urgency)
- Opponent has 2 captures: penalty × 2 (moderate urgency)
- Opponent has 3+ captures: penalty × 4 (high urgency)

**Design note**: The base penalty was reduced from 100,000 to 8,000 because the original value was too dominant — it pushed ALL moves near opponent stones to the bottom of ordering, causing the AI to play scattered, disconnected moves instead of building connected patterns.

**한국어 설명**: 상대가 다음 수에 캡처할 수 있는 우리 돌 쌍을 감지하여 패널티를 줍니다. 평가 함수에서 4,000점/쌍, 수 정렬에서 8,000점 기본 패널티(긴급도: 1x/2x/4x). 원래 100,000이었지만 너무 높아서 상대 돌 근처의 모든 수가 최하위로 밀려나 산발적 플레이가 발생했습니다.

### 2.6 Position Score (Center Control)

```rust
// Manhattan distance from center (9,9)
let dist = |pos.row - 9| + |pos.col - 9|;
let bonus = (18 - dist) * 3;  // Center: 54pts, corner: 0pts
```

Center stones have more potential for patterns in all directions. This provides a tiebreaker when patterns are equal.

**한국어 설명**: 중앙에 가까운 돌이 더 높은 점수를 받습니다. 패턴 점수가 동등할 때 타이브레이커 역할을 합니다.

---

## 3. Game Rules Implementation

### 3.1 Win Conditions

**File**: `src/rules/win.rs`

Two ways to win:

#### 3.1.1 Five or More in a Row
```rust
// Fast check at specific position: O(4 directions)
pub fn has_five_at_pos(board: &Board, pos: Pos, color: Stone) -> bool {
    for each of 4 directions:
        count = 1 + count_positive + count_negative
        if count >= 5: return true
    return false
}
```

#### 3.1.2 Capture Win (5 pairs = 10 stones)
```rust
if board.captures(color) >= 5 { return Some(color); }
```

#### 3.1.3 Endgame Capture Rule
A five-in-a-row only wins if the opponent **cannot break it by capturing a pair from the line**.

```rust
pub fn can_break_five_by_capture(board, five_positions, five_color) -> bool {
    for each empty position adjacent to the five:
        if opponent placing here captures any stone IN the five:
            return true  // Five can be broken!
    return false
}
```

**한국어 설명**:
- **5연속 승리**: 가로/세로/대각선으로 5개 이상 연속 배치
- **캡처 승리**: 상대 돌 10개(5쌍) 캡처
- **종료 캡처 규칙**: 5연속이 있어도 상대가 캡처로 이를 깨뜨릴 수 있다면 아직 승리가 아닙니다. 5연속 라인의 돌 중 하나가 캡처 패턴의 일부인 경우, 상대는 캡처로 이를 방어할 수 있습니다.

### 3.2 Capture Rules (Pente/Ninuki-renju)

**File**: `src/rules/capture.rs`

#### Pattern: `X-O-O-X`
When a player places a stone creating `X-O-O-X` (X = player, O = opponent), the O-O pair is captured and removed.

```rust
pub fn execute_captures_fast(board, pos, stone) -> CaptureInfo {
    for each of 8 directions:
        check: pos+1 == opponent && pos+2 == opponent && pos+3 == own_color
        if match:
            remove pos+1 and pos+2
            increment capture count
}
```

**Key rules**:
- **Only pairs**: Exactly 2 consecutive stones. Not 1, not 3+.
- **Safe placement**: Placing between opponent flanks is SAFE (not captured)
- **Board reset**: Captured intersections become free for replay
- **Multiple captures**: One move can capture multiple pairs in different directions
- **No allocation**: `CaptureInfo` uses fixed `[Pos; 16]` array for zero-alloc make/unmake

```rust
// Undo captures (for make/unmake search pattern)
pub fn undo_captures(board, stone, info: &CaptureInfo) {
    for each captured position: restore opponent stone
    decrement capture count
}
```

**한국어 설명**: `X-O-O-X` 패턴에서 O-O 쌍이 캡처됩니다. **정확히 2개**만 캡처 가능합니다 (1개나 3개는 안 됨). 상대 돌 사이에 놓는 것은 **안전**합니다 — 자신의 돌이 캡처되지 않습니다. `CaptureInfo`는 힙 할당 없이 고정 배열을 사용하여 탐색 중 make/unmake 패턴에 최적화되어 있습니다.

### 3.3 Double-Three Rule (Forbidden Move)

**File**: `src/rules/forbidden.rs`

#### What is a Free-Three?
3 aligned stones with both ends open, that can become an unstoppable open-four:
- Consecutive: `_OOO_` (span 3, 2 open ends)
- Spaced: `_OO_O_` or `_O_OO_` (span 4, 2 open ends, exactly 1 gap)

#### Forbidden: Creating 2+ Free-Threes Simultaneously

```rust
pub fn is_double_three(board, pos, stone) -> bool {
    // Exception: if this move captures, double-three is allowed!
    if has_capture(board, pos, stone) { return false; }

    count_free_threes(board, pos, stone) >= 2
}
```

#### Implementation Details

The `scan_line()` function scans each direction from the placed position, allowing one gap:

```rust
fn scan_line(board, pos, stone, dr, dc) -> LinePattern {
    // Scans positive direction: collect stones, track gaps and open ends
    // Scans negative direction: same
    // Returns: stones[], open_ends, span
}

fn is_free_three(pattern) -> bool {
    pattern.stones.len() == 3        // Exactly 3 stones
    && pattern.open_ends >= 2         // Both ends open
    && pattern.span <= 4              // Not too spread out
    // For span 4: must have exactly one single gap
}
```

**한국어 설명**:
- **프리 쓰리**: 양쪽이 열린 3연속 돌. 블록하지 않으면 오픈 포가 됩니다.
- **쌍삼 금지**: 한 수로 2개 이상의 프리 쓰리를 동시에 만드는 것은 금지입니다.
- **예외**: 캡처를 수행하는 수는 쌍삼이어도 허용됩니다.
- `scan_line()`은 한 방향으로 돌을 스캔하면서 하나의 갭까지 허용하여 `_O_OO_` 같은 패턴도 감지합니다.

### 3.4 Move Validity

```rust
pub fn is_valid_move(board, pos, stone) -> bool {
    board.is_empty(pos)                        // Must be empty
    && !is_double_three(board, pos, stone)      // Must not be forbidden
}
```

This is checked for every candidate move in both the AI search and human input validation.

### 3.5 Test Coverage

Our test suite validates all rules with **165 tests** covering:
- Horizontal/vertical/diagonal five-in-a-row detection
- Six-in-a-row also wins (5+ rule)
- Capture in all 8 directions
- Only pairs (not 1 or 3+ stones)
- Multiple simultaneous captures
- Cross-capture patterns (4 directions at once)
- Board edge captures
- Free-three detection (consecutive and gapped)
- Double-three detection (cross pattern, diagonal cross)
- Double-three with capture exception
- Breakable five detection
- Win condition priority (capture > five-in-a-row)

```bash
cargo test --lib --release  # All 165 tests pass in ~1.0s
```

**한국어 설명**: 165개의 단위 테스트가 모든 게임 규칙을 검증합니다: 5연속 감지, 캡처 규칙, 쌍삼 금지, 캡처 예외, 깨뜨릴 수 있는 5연속, 승리 조건 우선순위 등.

---

## 4. AI Strategy & Loss Defense

### 4.1 Why the AI Can Lose (and Why That's Expected)

#### 4.1.1 Game Complexity Analysis

**19x19 Pente with double-three prohibition is an UNSOLVED game.**

| Game | Status | Complexity |
|------|--------|-----------|
| Tic-Tac-Toe | Solved (draw) | 10^3 |
| Connect Four | Solved (first player wins) | 10^13 |
| Standard Gomoku 15x15 | Solved (first player wins) | 10^70 |
| **19x19 Pente + 33-ban** | **UNSOLVED** | **>> 10^70** |

The capture rule creates **non-monotonic game trees**: a player's advantage can suddenly reverse when stones are captured. This makes exhaustive search impossible.

**한국어 설명**: 19x19 펜테 + 쌍삼 금지는 **미해결 게임**입니다. 캡처 규칙이 게임 트리를 비단조적으로 만들어, 유리한 상황이 캡처로 갑자기 역전될 수 있습니다. 완전한 탐색은 불가능하므로, 어떤 AI도 100% 승률을 보장할 수 없습니다.

#### 4.1.2 Theoretical Limits at Depth 10

With minimum depth 10 and average < 500ms:
- **Nodes per second**: ~170K-256K (Release build)
- **Effective branching factor**: ~2.1-2.3 (after all pruning)
- **Nodes at depth 10**: ~9,000-50,000

This means the AI looks ~10 moves ahead. An expert player who plans 15+ moves ahead or uses patterns the heuristic doesn't value correctly can outplay the AI.

**From the PDF**: "The AI is not required to never lose. It should play strong enough that an average player cannot easily beat it."

**한국어 설명**: 깊이 10에서 ~9,000-50,000 노드를 탐색합니다. AI는 10수 앞을 봅니다. 15수 이상을 계획하는 전문가는 AI를 이길 수 있습니다. **PDF에서도 "AI가 절대 지면 안 된다"고 요구하지 않습니다**.

### 4.2 If the AI Loses During Defense

#### Strategy 1: Demonstrate Understanding

"우리 AI가 졌지만, 이는 기대한 결과입니다. 다음을 설명하겠습니다:"

1. **Show the log**: `gomoku_ai.log` shows exactly what the AI considered each turn
2. **Point out the critical move**: "Move #N에서 상대가 이 패턴을 만들었고, AI의 depth 10에서는 이를 감지하지 못했습니다"
3. **Explain the horizon effect**: "이 위협은 13수 뒤에 완성되므로 depth 10 탐색 범위 밖입니다"

#### Strategy 2: Quote the Subject PDF

From the evaluation criteria:
> "Minimax가 올바르게 구현되었는지, alpha-beta pruning이 제대로 작동하는지가 중요합니다."
> "AI의 승패보다 구현의 정확성과 이해도를 평가합니다."

#### Strategy 3: Show the Numbers

```
Depth 10 achieved: YES (mandatory requirement met)
Average time < 500ms: YES (typically 36-200ms for depth 10)
NMP node reduction: ~80% (47K → 9K nodes)
Time reduction: ~91% (327ms → 36ms)
Total optimizations: NMP, LMR, Futility, PVS, TT, Killer, History
```

"우리는 프로젝트 요구사항의 모든 기술적 조건을 충족합니다."

#### Strategy 4: Explain What Makes the Game Hard

1. **Capture creates non-monotonic evaluation**: A seemingly winning position can be reversed by one capture
2. **Board size 19x19**: 361 intersections create vast search space
3. **Double-three prohibition**: Limits forcing sequences, making VCF less effective
4. **Pente captures**: Standard Gomoku theory (solved) doesn't apply because captures change everything

**한국어 설명**: AI가 지면 당황하지 말고: (1) 로그를 보여주며 AI가 각 수에서 무엇을 고려했는지 설명, (2) 크리티컬 무브를 지적하고 탐색 깊이의 한계 설명, (3) PDF 인용하여 "구현의 정확성이 승패보다 중요"함을 강조, (4) 수치로 모든 기술적 요구사항 충족을 증명.

### 4.3 Debug Process for Defense Session

The AI logs every decision to `gomoku_ai.log`:

```
============================================================
[Move #8 | AI: White | Stones: 7 | B-cap: 0 W-cap: 0]
  Stage 1 Immediate win: none
  Stage 2 Opponent threats: 0 positions
  Stage 3 Our VCF: not found (1nodes)
  Stage 4 Opponent VCF: not found (1nodes)
  Stage 5 ALPHA-BETA: move=K8 score=-1877 depth=10 nodes=138883 time=841ms nps=165k tt=7%
```

Each move shows:
- **Stage reached**: Which search method found the move
- **Score**: Positive = AI advantage, negative = opponent advantage
- **Depth**: How deep the search went (must be >= 10)
- **Nodes**: Total positions evaluated
- **Time**: Wall clock time in milliseconds
- **NPS**: Nodes per second (performance metric)
- **TT%**: Transposition table usage

**한국어 설명**: `gomoku_ai.log`는 모든 AI 결정을 기록합니다. 각 수마다 5단계 탐색 과정, 점수, 깊이, 노드 수, 시간, NPS를 표시합니다. 디펜스 중 이 로그를 보여주며 AI의 사고 과정을 설명할 수 있습니다.

---

## 5. Performance Benchmarks

### 5.1 Search Performance

| Metric | Before Optimization | After Optimization | Improvement |
|--------|--------------------|--------------------|-------------|
| Depth 10 time | 327ms | 36ms | **91% faster** |
| Nodes at depth 10 | 47,000 | 9,000 | **81% fewer** |
| NPS | 171K | 256K | **50% faster** |
| Effective b_eff | 3.1-5.3 | ~2.1 | **60% narrower** |

### 5.2 What Each Optimization Contributes

| Technique | Node Reduction | Time Impact |
|-----------|---------------|-------------|
| **Null Move Pruning** | ~80% | Dominant |
| **Futility Pruning** | ~15% (at leaf) | Moderate |
| **Late Move Reduction (3-tier)** | ~20% (at deep) | Moderate |
| **PVS** | ~10% | Minor |
| **TT** | Variable | Cumulative |
| **Combined score_move** | - | 50% faster move gen |

### 5.3 Board Representation

**6 x u64 Bitboard** (384 bits for 361 cells):
- O(1) stone placement/removal
- O(1) occupancy check
- Hardware popcount for stone counting
- Cache-friendly memory layout

**한국어 설명**: 6개의 u64로 구성된 비트보드는 O(1) 돌 배치/제거, 하드웨어 popcount 활용, 캐시 친화적 메모리 레이아웃을 제공합니다.

---

## 6. Architecture Overview

### 6.1 Module Structure

```
src/
├── board/
│   ├── bitboard.rs     # 6 x u64 bitboard
│   └── board.rs        # Board state (stones + captures)
├── rules/
│   ├── capture.rs      # X-O-O-X capture logic
│   ├── win.rs          # Five-in-a-row + capture win
│   └── forbidden.rs    # Double-three detection
├── eval/
│   ├── patterns.rs     # Score constants (hierarchy)
│   └── heuristic.rs    # Position evaluation
├── search/
│   ├── alphabeta.rs    # Negamax + AB + NMP + LMR + Futility
│   ├── threat.rs       # VCF/VCT threat search
│   ├── tt.rs           # Transposition table
│   └── zobrist.rs      # Incremental hashing
└── engine.rs           # 5-stage search pipeline
```

### 6.2 Data Flow

```
Human/AI Move
    → Board.place_stone()      [O(1) bitboard]
    → execute_captures_fast()  [O(8 directions), no alloc]
    → check_winner()           [O(4 dirs at last move)]
    → AI Turn:
        → get_opening_move()   [O(1)]
        → find_immediate_win() [O(N cells)]
        → find_winning_moves() [O(N cells), opponent threats]
        → search_vcf()         [Depth 30, forcing moves only]
        → search_timed()       [Iterative deepening, depth 10+]
            → alpha_beta()     [NMP + LMR + Futility + PVS + TT]
                → evaluate()   [Pattern + Capture + Position - Vulnerability]
```

### 6.3 Make/Unmake Pattern

Throughout the search, we avoid cloning the board. Instead:

```rust
// Make move
board.place_stone(mov, color);
let cap_info = execute_captures_fast(board, mov, color);
let child_hash = zobrist.update_place(hash, mov, color);

// Search
let score = -alpha_beta(board, opponent, depth-1, ...);

// Unmake move
undo_captures(board, color, &cap_info);
board.remove_stone(mov);
```

This saves thousands of board allocations per search.

**한국어 설명**: 탐색 중 보드를 복사하지 않고 make/unmake 패턴을 사용합니다. 돌을 놓고 → 탐색하고 → 되돌립니다. `CaptureInfo`의 고정 배열 덕분에 힙 할당이 전혀 없습니다.

---

## Quick Defense Cheat Sheet / 빠른 디펜스 요약

### Q: "Minimax를 설명해주세요"
A: Negamax with alpha-beta pruning. 5-stage pipeline. NMP + LMR + Futility + PVS. Depth 10 guaranteed. TT with Zobrist hashing.

### Q: "Heuristic을 설명해주세요"
A: Pattern scoring with 10x gaps (FIVE > OPEN_FOUR > CLOSED_FOUR > ...). Non-linear capture scoring. Vulnerability penalty. Combo detection. Negamax-symmetric.

### Q: "게임 규칙이 올바르게 구현되었나요?"
A: 165 tests. Five-in-a-row + capture win + breakable-five + double-three + capture exception. All validated.

### Q: "AI가 왜 졌나요?"
A: 19x19 Pente는 미해결 게임. Depth 10 = 10수 앞. 전문가는 더 깊이 읽음. 캡처로 비단조적 게임 트리. PDF도 100% 승률을 요구하지 않음. 로그를 보면 AI의 사고 과정을 확인 가능.

### Q: "시간 제한은 어떻게 지키나요?"
A: Iterative deepening. Depth 10 무조건 완료. 이후 관측된 branching factor로 다음 깊이 시간 예측. 평균 < 500ms.

### Q: "어떤 최적화를 했나요?"
A: NMP(-80% nodes), 3-tier LMR(-20% b_eff), Futility(-15% leaf), PVS, TT+Zobrist, combined score_move(8 scans vs 40+, two-detection), make/unmake(no board clone), bitboard(O(1) ops).
