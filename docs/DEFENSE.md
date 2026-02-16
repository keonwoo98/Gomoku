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
7. [Code Review & Concepts / 코드 리뷰 & 개념 설명](#7-code-review--concepts)

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

**File**: `src/search/alphabeta.rs`, function `search_iterative()`

We use **Iterative Deepening**: search depth 1, then 2, then 3, ... up to the maximum. This provides:

1. **Time management**: We can stop between depths if time runs out
2. **Better move ordering**: Results from depth N-1 improve ordering at depth N
3. **Guaranteed minimum depth**: 10 (mid-game) or 8 (opening, ≤4 stones)

```rust
// Dynamic minimum depth based on game phase
let min_depth: i8 = if board.stone_count() <= 4 { 8 } else { 10 };

for depth in 1..=max_depth {
    result = search_root(board, color, depth);

    // 2-depth win confirmation: both depth d AND d+1 must agree
    // on a terminal score before early exit (prevents illusory wins)
    if is_winning && prev_was_winning && depth >= min_depth { break; }
    if is_losing  && prev_was_losing  && depth >= min_depth { break; }

    if depth < min_depth { continue; }

    // Predictive time control
    let bf = depth_time / prev_depth_time;  // Observed branching factor
    let estimated_next = depth_time * bf;   // bf clamped to [1.5, 5.0]
    if estimated_next > remaining_time { break; }
}
```

**Time control strategy**:
- Soft limit: 500ms target
- Hard limit: soft_limit + 150ms (completion buffer)
- Adaptive opening: 0-2 stones→30%, 3-4 stones→60%, 5+→100% of time_limit
- Minimum floor: 300ms even in opening

**2-depth win confirmation**: Iterative deepening requires TWO consecutive depths to agree on a terminal score (FIVE ± 100) before early exit. This prevents illusory wins where depth d sees a forced win but depth d+1 finds the refutation.

**한국어 설명**: Iterative Deepening은 깊이 1부터 시작해 점진적으로 깊어집니다. **최소 깊이 10은 무조건 완료**(오프닝은 8). **2-depth 확인**: 깊이 d와 d+1 모두 승리/패배에 동의해야 조기 종료합니다. 이전 깊이의 소요 시간으로 다음 깊이의 시간을 예측하여 시간 초과를 방지합니다.

### 1.3 Aspiration Windows

**File**: `src/search/alphabeta.rs`, inside `search_iterative()`

Instead of searching with (-INF, INF), we narrow the window around the previous depth's score:

```rust
const ASP_WINDOW: i32 = 100;

// At depth >= 3 with non-terminal score:
(asp_alpha, asp_beta) = (prev_score - 100, prev_score + 100);

// On fail-low or fail-high, immediately open to full window
if result.score <= asp_alpha { asp_alpha = -INF; }  // No gradual widening
if result.score >= asp_beta  { asp_beta = INF; }
```

**Key design**: No gradual widening on failure. Immediate full-window re-search prevents repeated re-searches in losing positions (was a depth collapse root cause).

**한국어 설명**: 이전 깊이의 점수 ± 100 범위로 좁은 윈도우를 사용합니다. 실패 시 즉시 전체 윈도우로 확장 — 점진적 확장은 패배 포지션에서 반복 재탐색을 유발하여 depth collapse를 일으켰습니다.

### 1.4 Lazy SMP (Parallel Search)

**File**: `src/search/alphabeta.rs`

Multi-threaded parallel search using lock-free shared transposition table:

```
┌──────────────────────────────────────────┐
│ SharedState (Arc)                        │
│  ├── zobrist: ZobristTable               │
│  ├── tt: AtomicTT (lock-free, XOR trick) │
│  └── stopped: AtomicBool                 │
├──────────────────────────────────────────┤
│ Worker 0 (main thread)                   │
│  ├── killer_moves, history, countermove   │
│  └── starts at depth 1                   │
│ Worker 1                                 │
│  ├── independent killer/history tables    │
│  └── starts at depth 2 (staggered)       │
│ Worker N-1                               │
│  ├── independent killer/history tables    │
│  └── starts at depth N (staggered)       │
└──────────────────────────────────────────┘
```

- **AtomicTT**: Lock-free transposition table using XOR trick (42-bit packing). No mutexes, no contention.
- **Staggered depths**: Workers start at different depths for natural tree diversification. Worker `i` starts at depth `1+i`.
- **Auto-detect cores**: `std::thread::available_parallelism()`, max 8 threads.
- **Result aggregation**: Pick deepest search result, then highest score. Merge node counts and stats from all workers.
- **Global stop signal**: `AtomicBool` — when main thread detects time expired, all workers stop.

**한국어 설명**: Lazy SMP는 여러 스레드가 동시에 탐색하며 lock-free TT를 공유합니다. 각 워커는 독립적인 killer/history 테이블을 가지고 다른 깊이에서 시작하여 트리 다양성을 확보합니다. AtomicTT는 뮤텍스 없이 XOR 트릭으로 원자적 읽기/쓰기를 수행합니다.

### 1.5 Transposition Table (TT)

**File**: `src/search/tt.rs`

The TT stores previously evaluated positions to avoid redundant computation.

- **Size**: 64 MB (configurable)
- **Hash**: Zobrist hashing (see 1.6)
- **Entry types**:
  - `Exact`: Score is the true minimax value
  - `LowerBound`: Score >= stored value (beta cutoff)
  - `UpperBound`: Score <= stored value (failed high)
- **Replacement**: Depth-based — deeper results overwrite shallower ones
- **Lock-free**: AtomicTT uses XOR trick for thread-safe read/write without locks

```rust
// TT probe at the start of alpha_beta()
if let Some((score, best_move)) = self.tt.probe(hash, depth, alpha, beta) {
    return score;  // Skip entire subtree!
}
```

**한국어 설명**: Transposition Table은 이미 평가한 포지션을 저장하여 중복 계산을 방지합니다. 64MB 크기의 해시 테이블로, 같은 포지션에 다른 수순으로 도달해도 재활용할 수 있습니다. Lock-free AtomicTT로 멀티스레드 안전.

### 1.6 Zobrist Hashing

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

**한국어 설명**: Zobrist 해싱은 돌을 놓거나 제거할 때 O(1)로 해시를 업데이트합니다. XOR의 교환법칙과 자기역원 성질 덕분에, 같은 포지션에 어떤 수순으로 도달하든 동일한 해시를 생성합니다.

### 1.7 Advanced Pruning Techniques

#### 1.7.1 Null Move Pruning (NMP)

**Concept**: If we skip our turn entirely (null move) and the position is STILL >= beta, the position is so good that we can prune without full search.

```rust
if allow_null && depth >= 3 && !is_threatened(board, color, last_move)
    && static_eval >= beta  // Only NMP when position looks good
{
    let r = 2i8;  // Fixed R=2 (R=3 was too aggressive, missed opponent responses)
    let null_score = -alpha_beta(board, opponent, depth-1-r, -beta, -(beta-1), false);

    if null_score >= beta {
        if depth <= 8 { return beta; }           // Shallow: trust result
        let verify = alpha_beta(..., depth-r);    // Deep: verify with reduced search
        if verify >= beta { return beta; }
    }
}
```

**Safety guards**:
- `allow_null = false` after a null move (no consecutive null moves)
- Skip in threatened positions (opponent near capture win or has four-in-a-row)
- `static_eval >= beta` gate prevents NMP when position is bad (opponent rebuilt threats after capture)
- Verification search at depth > 8 to prevent zugzwang errors
- **R=2 fixed**: R=3 was too aggressive, missing critical opponent responses (e.g., replaying captured position)

**Impact**: Reduced search nodes by ~80%, time by ~91%

**한국어 설명**: Null Move Pruning은 "우리 차례를 건너뛰어도 여전히 이기고 있다면, 이 포지션은 확실히 좋다"는 원리입니다. R=2 고정 (R=3은 너무 공격적). static_eval >= beta 조건으로 나쁜 포지션에서 NMP 방지. 탐색 노드를 80% 줄이고 시간을 91% 단축시킨 가장 효과적인 최적화입니다.

#### 1.7.2 Reverse Futility Pruning (RFP) / Static Null Move

At shallow depths (1-3), if the static evaluation is far above beta, even losing a full threat won't drop below beta. Cut immediately.

```rust
if depth <= 3 && non_terminal
    && static_eval - OPEN_THREE * depth >= beta
{
    return static_eval;  // Position too good, can't drop below beta
}
```

Margin = OPEN_THREE (10,000) per depth level. In Gomoku, a single quiet move can swing eval by at most ~OPEN_THREE.

**한국어 설명**: 깊이 1-3에서 정적 평가값이 beta보다 훨씬 높으면, 한 위협을 잃어도 beta 아래로 떨어지지 않으므로 즉시 커트합니다. 마진 = depth × 10,000.

#### 1.7.3 Razoring

Complementary to RFP — at shallow depths (1-3), if static eval is far below alpha, verify with quiescence search. If QS confirms the position is bad, cut.

```rust
if depth <= 3 && non_terminal
    && static_eval + OPEN_THREE * depth <= alpha
{
    let qs_score = quiescence(board, color, alpha, beta, ...);
    if qs_score <= alpha { return qs_score; }
}
```

**한국어 설명**: RFP의 반대 — 정적 평가값이 alpha보다 훨씬 낮으면 QS로 확인 후 커트. RFP는 "너무 좋은" 포지션, razoring은 "너무 나쁜" 포지션을 처리합니다.

#### 1.7.4 Futility Pruning

At shallow depths (1-3), if static eval + margin is still below alpha, non-tactical moves (score < 800K) are hopeless and can be skipped.

```rust
if depth <= 3 && non_terminal && i > 0 {
    let margin = match depth {
        1 => CLOSED_FOUR,   // 50,000
        2 => OPEN_FOUR,     // 100,000
        _ => OPEN_FOUR + OPEN_THREE,  // 110,000 (depth 3)
    };
    if static_eval + margin <= alpha && move_score < 800_000 {
        continue;  // Skip this hopeless move
    }
}
```

**Key**: Uses pre-computed `move_score` from `generate_moves_ordered()` — no redundant `score_move()` call.

**한국어 설명**: 깊이 1-3에서 정적 평가값 + 마진이 alpha보다 낮으면, 비전술적 수는 상황을 개선할 수 없으므로 건너뜁니다. move_score가 이미 계산되어 있어 추가 비용 없음.

#### 1.7.5 Late Move Reduction (LMR)

Moves sorted later in the list are less likely to be good. We search them at reduced depth first, then re-search at full depth only if they beat alpha.

```rust
// Logarithmic formula (Stockfish-inspired)
let r = (depth.sqrt() * move_index.sqrt() / 2.0) as i8;
// Score-aware: quiet moves (< 500K) get +1 extra reduction
if move_score < 500_000 { r += 1; }
r = r.clamp(1, depth - 2);
```

**Exemptions**: Only PV move (i=0) is exempt. Captures and extensions also skip LMR.

**한국어 설명**: 정렬 후순위 수일수록 낮은 깊이로 탐색합니다. 로그 공식 `sqrt(d)*sqrt(m)/2`로 부드러운 축소. 조용한 수(< 500K)는 추가 축소. PV move만 면제. 캡처와 extension도 축소하지 않습니다.

#### 1.7.6 Late Move Pruning (LMP)

At very shallow depths (≤3), skip quiet moves entirely after trying the first few. Done **before** `make_move` for zero overhead.

```rust
// Threshold: 3 + depth * 2 (depth 1: skip after 5th, depth 3: skip after 9th)
if i > 0 && depth <= 3 && i >= (3 + depth * 2) && move_score < 800_000 {
    continue;  // Skip entirely — no make_move cost
}
```

**Impact**: HIGH ROI. Late quiet moves at shallow depths almost never improve alpha.

**한국어 설명**: 깊이 ≤ 3에서 조용한 수를 완전히 건너뜁니다. make_move 전에 체크하므로 오버헤드가 0입니다. 높은 ROI — 얕은 깊이의 후순위 조용한 수는 거의 alpha를 개선하지 않습니다.

#### 1.7.7 Principal Variation Search (PVS)

The first move (expected best from TT/move ordering) is searched with a full window. All subsequent moves use a null window `(alpha, alpha+1)` — cheaper to compute. If a move beats this narrow window, we re-search with the full window.

```rust
if i == 0 {
    score = -alpha_beta(board, opponent, depth-1, -beta, -alpha);  // Full window
} else {
    // LMR reduced search → null window
    s = -alpha_beta(board, opponent, reduced_depth, -(alpha+1), -alpha);
    // If LMR was applied and s > alpha, re-search at full depth
    if reduction > 0 && s > alpha {
        s = -alpha_beta(board, opponent, depth-1, -(alpha+1), -alpha);
    }
    // If s is between alpha and beta, full window re-search
    if s > alpha && s < beta {
        s = -alpha_beta(board, opponent, depth-1, -beta, -alpha);
    }
}
```

**한국어 설명**: 첫 번째 수는 전체 윈도우, 나머지는 좁은 윈도우로 탐색합니다. LMR 축소된 탐색 → 실패 시 전체 깊이 재탐색 → 여전히 좋으면 전체 윈도우 재탐색. 3단계 파이프라인.

#### 1.7.8 Internal Iterative Deepening (IID)

When no TT entry exists at depth >= 6, run a shallow search (depth-4) first for better move ordering.

```rust
if tt_move.is_none() && depth >= 6 {
    let iid_depth = (depth - 4).max(1);
    alpha_beta(board, color, iid_depth, alpha, beta, ...);
    tt_move = tt.get_best_move(hash);  // Now we have a good first move
}
```

**Threshold raised from 4 to 6**: IID at depth >= 4 with depth/2 formula triggered recursive IID cascade at every non-TT node, causing exponential blowup. Raising to >= 6 with depth-4 eliminated this.

**한국어 설명**: TT 엔트리가 없을 때 얕은 탐색으로 좋은 첫 번째 수를 찾습니다. 임계값을 4→6으로 올려 IID 캐스케이드(재귀적 IID가 모든 노드에서 발동)를 제거했습니다.

#### 1.7.9 Threat Extensions

Forcing moves (creating a four) get +1 ply extension. Fours have only 1-2 legal responses, so the subtree is narrow and the cost is minimal.

```rust
// Only at depth >= 2 (at depth 1, quiescence already handles threats)
let extension = if depth >= 2 && move_creates_four(board, mov, color) { 1 } else { 0 };
```

This replaces the removed VCT search — threat extensions give VCT-like tactical depth within the sound alpha-beta framework, without unsound assumptions about open-three forcing.

**한국어 설명**: 4를 만드는 수는 +1 ply 연장합니다. 4는 1-2개의 합법적 응수만 있어 비용이 적습니다. 제거된 VCT를 대체 — VCT는 열린 삼이 강제적이라고 가정했지만, 상대가 무시하고 반격할 수 있어 unsound했습니다.

### 1.8 VCF Quiescence Search

**File**: `src/search/alphabeta.rs`, function `quiescence()`

At leaf nodes (depth ≤ 0), extends search for forcing moves only:

```
Stand-pat score: evaluate(board, color)
If stand_pat >= beta → return (position already too good)
If stand_pat > alpha → update alpha

Extend search for:
  - Fives (any qs_depth)
  - Fours (only when qs_depth < 6)
  - Capture-wins (5th pair capture)
MAX_QS_DEPTH = 16
```

This eliminates the **horizon effect** where the AI can't see a forced win/loss just beyond its search depth because it's only 1-2 forcing moves away.

**한국어 설명**: 리프 노드에서 강제 수(5, 4, 캡처 승리)만 추가 탐색합니다. Stand-pat pruning으로 이미 좋은 포지션은 즉시 반환. 호라이즌 효과를 제거하여 탐색 깊이 바로 너머의 강제 승리/패배를 감지합니다.

### 1.9 Move Ordering (Defense-First Philosophy)

**File**: `src/search/alphabeta.rs`, function `score_move()`

Good move ordering is critical for alpha-beta efficiency. Our ordering combines 8 direction scans per move (instead of 40+) with this priority hierarchy:

| Priority | Score | Pattern |
|----------|-------|---------|
| 1 | 1,000,000 | TT best move |
| 2 | 900,000 | Our five-in-a-row |
| 3 | 895,000 | Block opponent five |
| 4 | 890,000 | Capture win (5th pair) |
| 5 | 885,000 | Block opponent capture win |
| 6 | 880,000 | Our double-four fork |
| 7 | 878,000 | Our four + open three fork |
| 8 | 870,000 | Our open four |
| 9 | 868,000 | Block opponent double-four fork |
| 10 | 866,000 | Block opponent four + three fork |
| 11 | 860,000 | Block opponent open four |
| 12 | 855,000 | Block near-capture-win (opp 3+ caps) |
| 13 | 845,000 | Block capture threat (opp 2+ caps) |
| 14 | 840,000 | Our double open-three |
| 15 | 838,000 | Block opponent double open-three |
| 16 | 830,000 | Our closed four |
| 17 | 820,000 | Block opponent closed four |
| 18 | 810,000 | Our open three |
| 19 | 800,000 | Block opponent open three |
| 20 | 600,000+ | Capture moves (with urgency scaling) |
| 21 | 550,000+ | Block opponent captures |
| 22 | 500,000 | Killer move (slot 0) |
| 23 | 490,000 | Killer move (slot 1) |
| 24 | 400,000 | Countermove heuristic |
| 25 | Variable | History heuristic + center bonus + two-detection |

**Fork detection**: Uses **counts** (not booleans) per direction. Two fours = 880K, four+three = 878K. This catches multi-directional threats that a boolean approach would miss.

**Capture vulnerability penalty**: Subtracted from non-tactical moves. Two types:
- `opp-ME-ally-empty` → 150K (1-move capture threat)
- `empty-ME-ally-empty` → 50K-100K (2-move setup, scales with opponent captures)

**Countermove heuristic**: Table `[2][19][19]` mapping opponent's last_move → best response. Recorded on beta cutoff. +400K ordering bonus.

**History gravity**: All history table scores halved (`>>= 1`) at each iterative deepening depth for recency bias.

**한국어 설명**: 수 정렬은 alpha-beta 효율의 핵심입니다. 8방향 스캔으로 한 번에 패턴 감지. 포크 감지(방향별 카운트), 캡처 취약성 패널티, countermove 휴리스틱, history gravity를 조합합니다. 공격과 방어를 교차 배치하여 우리 포크(880K)가 상대 오픈포 차단(860K)보다 우선됩니다.

### 1.10 Adaptive Move Limiting

At internal nodes, limit candidate moves based on tactical state:

```rust
// Tactical: top move score >= 850K (fork/four level threats exist)
let max_moves = if is_tactical {
    match depth { 0..=1 => 5, 2..=3 => 7, 4..=5 => 9, _ => 12 }
} else {
    match depth { 0..=1 => 3, 2..=3 => 5, 4..=5 => 7, _ => 9 }
};
```

Root node: `MAX_ROOT_MOVES = 30`. All moves are validated for double-three rule (`is_valid_move`).

**한국어 설명**: 전술적 포지션(850K+ 수가 있는)에서는 더 많은 후보를 탐색하고, 조용한 포지션에서는 줄입니다. 루트: 30개.

### 1.11 Search Priority Pipeline

**File**: `src/engine.rs`

Before alpha-beta runs, the engine checks faster search methods:

```
Stage 0:   Opening Book       → Empty board → center (9,9)
                                 2nd move → diagonal adjacent
                                 3rd move → row/col pairs only
Stage 0.5: Break Opponent Five → Must break existing breakable five NOW
                                 (with recreation check: skip if recreates unbreakable five)
Stage 1:   Immediate Win       → Can we win THIS move? (5-in-a-row or capture)
                                 Includes illusory break detection
Stage 2:   Opponent Threats    → Must we block a 5-in-a-row? (O(N) scan)
Stage 3:   Our VCF             → Forced win via continuous fours
                                 (skipped when opponent has 4+ captures)
Stage 4:   Opponent VCF        → Does opponent have forced win we must block?
                                 (skipped when we have 4+ captures)
Stage 5:   Alpha-Beta          → Full search with all optimizations
                                 (adaptive time based on game phase)
```

**Stage 0.5 Break Five**: When opponent has an existing breakable five, we MUST capture a pair to destroy it. The engine checks if the break allows opponent to recreate an UNBREAKABLE five (by replaying at captured position). If all breaks lead to unbreakable recreation, falls through to alpha-beta.

**Illusory break detection** (Stage 1): A five-in-a-row is "breakable" if opponent can capture a pair from the line. But if the break capture removes a bracket stone, the five-holder can replay the captured stone and recreate an **unbreakable** five. If ALL break moves are illusory, the five is effectively unbreakable = immediate win.

**한국어 설명**: Alpha-Beta 이전에 더 빠른 탐색을 순서대로 확인합니다. Stage 0.5은 상대의 깨뜨릴 수 있는 5연속에 대한 필수 응답입니다. Stage 1은 허상 브레이크 감지를 포함 — 브레이크 캡처가 bracket 돌을 제거하면 리플레이로 깨뜨릴 수 없는 5연속을 재생성합니다.

---

## 2. Heuristic Evaluation Function

### 2.1 Design Philosophy

**File**: `src/eval/heuristic.rs`, `src/eval/patterns.rs`

The evaluation function must satisfy **negamax symmetry**:
```
evaluate(board, Black) == -evaluate(board, White)
```

This is non-negotiable. Any asymmetry (like defense multipliers in the eval) breaks the search.

Our evaluation = **Capture Score** + (**My Pattern Score** - **Opponent Pattern Score**) - **Vulnerability Penalty**

```rust
pub fn evaluate(board: &Board, color: Stone) -> i32 {
    let cap_score = capture_score(my_captures, opp_captures);
    let (my_score, my_vuln) = evaluate_color(board, color);
    let (opp_score, opp_vuln) = evaluate_color(board, opponent);
    let vuln_penalty = my_vuln * vuln_weight(opp_caps) - opp_vuln * vuln_weight(my_caps);
    cap_score + (my_score - opp_score) - vuln_penalty
}
```

**한국어 설명**: 평가 함수는 negamax 대칭성을 반드시 만족해야 합니다. 비대칭이면 탐색이 잘못된 계산을 합니다. 방어 우선은 평가 함수가 아닌 **수 정렬(move ordering)**에서 처리합니다.

### 2.2 Pattern Score Hierarchy

Each stone is evaluated for line patterns in 4 directions. Each line segment is counted once from its "start" position (line-start filter, ~60% call reduction).

| Pattern | Score | Description |
|---------|-------|-------------|
| **FIVE** | 1,000,000 | 5+ in a row (game over) |
| **OPEN_FOUR** | 100,000 | `_OOOO_` (unstoppable) |
| **CLOSED_FOUR** | 50,000 | `XOOOO_` (one open end) |
| **OPEN_THREE** | 10,000 | `_OOO_` (becomes open four) |
| **CLOSED_THREE** | 1,500 | `XOOO_` (one side blocked, half as dangerous) |
| **OPEN_TWO** | 1,000 | `_OO_` (development) |
| **CLOSED_TWO** | 200 | `XOO_` (minor) |

**10x gap design**: Each level is ~10x higher than the next. One OPEN_FOUR (100K) > ten OPEN_THREEs (100K total).

**Gap patterns**: The evaluator detects one-gap patterns like `OO_OO` or `O_OOO`. A gap-five (mc >= 5 with gap) is scored as OPEN_FOUR (filling the gap wins). 4-stone gap with span 5 → OPEN_FOUR.

**Direct bitboard access**: `evaluate_line()` uses `my_bb.get(p)` (1 lookup) instead of `board.get(p)` (2 lookups) for ~2x speedup.

**한국어 설명**: 패턴 점수는 10배 간격. CLOSED_THREE는 1,500 (OPEN_THREE의 15% — 한쪽이 막혀 위험성이 절반). 갭 패턴도 감지. Direct bitboard access로 ~2배 속도 향상.

### 2.3 Combo Detection

Multiple threats that the opponent cannot block simultaneously:

```rust
// Open four + any (closed four or open three) → overwhelming advantage
if open_fours >= 1 && (closed_fours >= 1 || open_threes >= 1) { score += OPEN_FOUR; }
// Two closed fours → opponent can only block one → effectively an open four
if closed_fours >= 2 { score += OPEN_FOUR; }
// Closed four + open three → must block four, three promotes
if closed_fours >= 1 && open_threes >= 1 { score += OPEN_FOUR; }
// Two open threes → opponent can only block one → one becomes open four
if open_threes >= 2 { score += OPEN_FOUR; }
```

**Multi-directional development bonus** (open twos):
```rust
if open_twos >= 4 { score += 8_000; }
else if open_twos >= 3 { score += 5_000; }
else if open_twos >= 2 { score += 3_000; }
```

**한국어 설명**: 상대가 동시에 막을 수 없는 복합 위협을 감지합니다. 열린 투 여러 개도 보너스.

### 2.4 Capture Scoring (Non-Linear)

```rust
const CAP_WEIGHTS: [i32; 6] = [
    0,           // 0 captures
    5_000,       // 1 capture: significant (> CLOSED_THREE, forces caution)
    7_000,       // 2 captures: moderate
    20_000,      // 3 captures: serious (> OPEN_THREE)
    80_000,      // 4 captures: near-winning (> OPEN_FOUR)
    1_000_000,   // 5 captures: game over
];

// Symmetric: capture_score(a,b) == -capture_score(b,a)
fn capture_score(my: u8, opp: u8) -> i32 {
    CAP_WEIGHTS[my] - CAP_WEIGHTS[opp]
}
```

**CAP_WEIGHTS[1] = 5,000** (increased from 2,000): Game log analysis showed the AI undervalued the cost of giving up the first capture. At 2K, allowing one capture barely exceeded CLOSED_THREE (1,500). At 5K, first capture is a significant strategic cost, forcing the AI to avoid creating capturable pairs.

**CAPTURE_PAIR = 5,000**: Matches CAP_WEIGHTS[1] for consistency.

**한국어 설명**: 캡처 점수는 비선형이며 negamax 대칭. CAP_WEIGHTS[1]을 2K→5K로 증가하여 첫 캡처의 전략적 비용을 강화. 게임 로그 분석에서 AI가 첫 캡처를 과소평가하여 패배한 사례를 수정.

### 2.5 Capture Vulnerability Penalty

**File**: `src/eval/heuristic.rs`

Detects our stone pairs that the opponent can capture next turn:

```
Pattern: empty - ally - ally - opponent  (opponent plays at empty to capture)
Pattern: opponent - ally - ally - empty  (opponent plays at empty to capture)
```

**Vulnerability weight scales with opponent captures** (exponential danger):

```rust
fn vuln_weight(opp_captures: u8) -> i32 {
    match opp_captures {
        0..=1 => 10_000,   // OPEN_THREE level
        2     => 20_000,   // Actively hunting
        3     => 40_000,   // Serious strategic threat
        _     => 80_000,   // Near-OPEN_FOUR (one more = instant loss)
    }
}
```

**Negamax symmetry proven**: `my_vuln * f(opp_caps) - opp_vuln * f(my_caps)` — swapping perspective negates the formula.

**한국어 설명**: 상대가 다음 수에 캡처할 수 있는 돌 쌍을 감지합니다. 패널티 가중치는 상대 캡처 수에 따라 지수적으로 증가: 0-1캡처 → 10K, 4+ → 80K. Negamax 대칭 유지.

### 2.6 Position Score (Center Control)

```rust
const POSITION_WEIGHT: i32 = 8;  // Higher weight discourages scattered play

let dist = |pos.row - 9| + |pos.col - 9|;   // Manhattan from center
let bonus = (18 - dist) * POSITION_WEIGHT;   // Center: 144pts, corner: 0pts
```

**Connectivity bonus**: Unidirectional (positive direction only), 160 points per adjacent same-color pair. Each pair is counted exactly once.

```rust
for &(dr, dc) in &DIRECTIONS {  // 4 positive half-directions
    if neighbor exists && same color → score += 160;
}
```

**한국어 설명**: 중앙 돌은 +144점, 코너 돌은 0점. POSITION_WEIGHT=8로 산발적 배치를 방지. 인접 동색 돌 쌍마다 +160점 (단방향 카운트로 중복 없음).

---

## 3. Game Rules Implementation

### 3.1 Win Conditions

**File**: `src/rules/win.rs`

Two ways to win:

#### 3.1.1 Five or More in a Row
```rust
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

#### 3.1.3 Endgame Capture Rule (Breakable Five)
A five-in-a-row only wins if the opponent **cannot break it by capturing a pair from the line**.

```rust
pub fn can_break_five_by_capture(board, five_positions, five_color) -> bool {
    for each empty position adjacent to the five:
        if opponent placing here captures any stone IN the five:
            return true  // Five can be broken!
    return false
}
```

**Illusory break**: If the break capture removes a bracket stone, and the five-holder replays → recreated five is unbreakable → the break was meaningless → five-holder wins.

**한국어 설명**:
- **5연속 승리**: 가로/세로/대각선으로 5개 이상 연속 배치
- **캡처 승리**: 상대 돌 10개(5쌍) 캡처
- **깨뜨릴 수 있는 5연속**: 5연속이 있어도 상대가 캡처로 이를 깨뜨릴 수 있다면 아직 승리가 아닙니다
- **허상 브레이크**: bracket 돌이 캡처되면 리플레이로 깨뜨릴 수 없는 5연속 재생성 → 사실상 승리

### 3.2 Capture Rules (Pente/Ninuki-renju)

**File**: `src/rules/capture.rs`

#### Pattern: `X-O-O-X`
When a player places a stone creating `X-O-O-X`, the O-O pair is captured and removed.

```rust
pub fn execute_captures_fast(board, pos, stone) -> CaptureInfo {
    for each of 8 directions:
        check: pos+1 == opponent && pos+2 == opponent && pos+3 == own_color
        if match: remove pos+1 and pos+2, increment capture count
}
```

**Key rules**:
- **Only pairs**: Exactly 2 consecutive stones. Not 1, not 3+.
- **Safe placement**: Placing between opponent flanks is SAFE (not captured)
- **Board reset**: Captured intersections become free for replay
- **Multiple captures**: One move can capture multiple pairs in different directions
- **No allocation**: `CaptureInfo` uses fixed `[Pos; 16]` array for zero-alloc make/unmake

**한국어 설명**: `X-O-O-X` 패턴에서 O-O 쌍이 캡처됩니다. 정확히 2개만, 상대 돌 사이에 놓는 것은 안전, `CaptureInfo`는 힙 할당 없는 고정 배열.

### 3.3 Double-Three Rule (Forbidden Move)

**File**: `src/rules/forbidden.rs`

#### What is a Free-Three?
3 aligned stones with both ends open, that can become an unstoppable open-four:
- Consecutive: `_OOO_` (span 3, 2 open ends)
- Spaced: `_OO_O_` or `_O_OO_` (span 4, 2 open ends, exactly 1 gap)

#### Forbidden: Creating 2+ Free-Threes Simultaneously

```rust
pub fn is_double_three(board, pos, stone) -> bool {
    if has_capture(board, pos, stone) { return false; }  // Capture exception!
    count_free_threes(board, pos, stone) >= 2
}
```

**한국어 설명**: 쌍삼 금지 — 한 수로 2개 이상의 프리 쓰리를 만드는 것은 금지. 캡처를 수행하는 수는 예외.

### 3.4 Test Coverage

**196 tests** covering all rules:

```bash
cargo test --lib --release  # All 196 tests pass in ~0.8s
```

- Five-in-a-row (horizontal/vertical/diagonal, 6+ also wins)
- Capture (all 8 directions, only pairs, multiple simultaneous, edge cases)
- Breakable five detection + illusory break
- Double-three (cross pattern, diagonal, capture exception)
- Search depth/time benchmarks
- Negamax symmetry validation
- Depth collapse regression tests
- VCF detection and blocking

**한국어 설명**: 196개의 단위 테스트가 모든 게임 규칙과 탐색 기능을 검증합니다.

---

## 4. AI Strategy & Loss Defense

### 4.1 Why the AI Can Lose (and Why That's Expected)

**19x19 Pente with double-three prohibition is an UNSOLVED game.**

| Game | Status | Complexity |
|------|--------|-----------|
| Tic-Tac-Toe | Solved (draw) | 10^3 |
| Connect Four | Solved (first player wins) | 10^13 |
| Standard Gomoku 15x15 | Solved (first player wins) | 10^70 |
| **19x19 Pente + 33-ban** | **UNSOLVED** | **>> 10^70** |

The capture rule creates **non-monotonic game trees**: advantage can suddenly reverse when stones are captured.

**한국어 설명**: 19x19 펜테 + 쌍삼 금지는 미해결 게임. 캡처 규칙이 비단조적 게임 트리를 만들어 완전한 탐색이 불가능합니다.

### 4.2 If the AI Loses During Defense

#### Strategy 1: Demonstrate Understanding

1. **Show the log**: `gomoku_ai.log` shows exactly what the AI considered each turn
2. **Point out the critical move**: "Move #N에서 상대가 이 패턴을 만들었고, AI의 depth 10-17에서는 이를 감지하지 못했습니다"
3. **Explain the horizon effect**: "이 위협은 N수 뒤에 완성되므로 탐색 깊이 범위 밖입니다"

#### Strategy 2: Quote the Subject PDF

> "Minimax가 올바르게 구현되었는지, alpha-beta pruning이 제대로 작동하는지가 중요합니다."
> "AI의 승패보다 구현의 정확성과 이해도를 평가합니다."

#### Strategy 3: Show the Numbers

```
Depth 10-17 achieved:     YES (mandatory requirement met)
Average time < 500ms:     YES (typically 100-300ms per move)
Lazy SMP parallelism:     YES (auto-detect cores, max 8 threads)
NPS (Release):            ~1,000K+ with Lazy SMP
Node reduction (NMP):     ~80%
Pruning techniques:       NMP, LMR, LMP, Futility, RFP, Razoring, PVS, IID, Threat Ext., VCF QS
Move ordering:            TT, Killer, History, Countermove, Defense-first, Fork detection
Tests passing:            196/196
```

#### Strategy 4: Known Limitations

1. **Capture creates non-monotonic evaluation**: Seemingly winning positions can reverse
2. **Board size 19x19**: 361 intersections = vast search space
3. **Tactical horizon effect**: Aggressive pruning (RFP/futility at depth 3) can miss 2-move capture sequences
4. **Double-three prohibition**: Limits forcing sequences, makes VCF less effective

**한국어 설명**: AI가 지면 당황하지 말고: 로그를 보여주고, 탐색 깊이 한계를 설명하고, PDF를 인용하여 "구현의 정확성이 승패보다 중요"함을 강조하고, 수치로 모든 기술적 요구사항 충족을 증명.

### 4.3 Debug Process for Defense Session

The AI logs every decision to `gomoku_ai.log`:

```
============================================================
[Move #8 | AI: White | Stones: 7 | B-cap: 0 W-cap: 0]
  Stage 0 OPENING: L9 (book move)
============================================================
[Move #12 | AI: White | Stones: 11 | B-cap: 0 W-cap: 1]
  Stage 1 Immediate win: none
  Stage 2 Opponent threats: 0 positions
  Stage 3 Our VCF: not found (15nodes)
  Stage 4 Opponent VCF: not found (8nodes)
  Stage 5 ALPHA-BETA: move=K8 score=12350 depth=14 nodes=285000 time=380ms nps=750k tt=12%
    Stats: beta_cutoffs=18500 first_move_rate=87.3% tt_probes=42000 tt_score_rate=31.2% tt_move_hits=8500
```

Each move shows: stage reached, score, depth, nodes, time, NPS, TT usage, and search quality metrics (first-move cutoff rate, TT hit rates).

---

## 5. Performance Benchmarks

### 5.1 Search Performance

| Metric | Value | Notes |
|--------|-------|-------|
| Average response time | ~100-300ms | Per move in mid-game |
| Search depth | 10-17 | Depends on position complexity |
| NPS (single thread) | ~170-256K | Release build |
| NPS (Lazy SMP) | ~1,000K+ | Multi-threaded, release build |
| Effective b_eff | ~2.1-2.3 | After all pruning |
| Tests | 196 passing | In ~0.8s (release) |

### 5.2 What Each Optimization Contributes

| Technique | Node Reduction | Time Impact |
|-----------|---------------|-------------|
| **Null Move Pruning** | ~80% | Dominant (91% time reduction) |
| **Late Move Pruning** | ~30% (shallow) | HIGH ROI |
| **Late Move Reduction** | ~20% (deep) | Moderate |
| **Futility Pruning** | ~15% (leaf) | Moderate |
| **Reverse Futility** | ~10% (shallow) | Moderate |
| **Razoring** | ~5% (shallow) | Minor |
| **PVS** | ~10% | Minor |
| **TT** | Variable | Cumulative across depths |
| **Threat Extensions** | Negative (adds nodes) | Improves quality |
| **VCF Quiescence** | Negative (adds nodes) | Eliminates horizon effect |
| **Lazy SMP** | - | ~3-5x NPS increase |
| **Combined score_move** | - | 50% faster move gen |
| **Direct bitboard access** | - | ~2x faster evaluate() |

### 5.3 Board Representation

**6 x u64 Bitboard** (384 bits for 361 cells):
- O(1) stone placement/removal
- O(1) occupancy check
- Hardware popcount for stone counting
- Cache-friendly memory layout
- Direct bitboard access eliminates double-lookup in evaluation

**한국어 설명**: 6개의 u64로 구성된 비트보드는 O(1) 돌 배치/제거, 하드웨어 popcount 활용, 캐시 친화적 메모리 레이아웃을 제공합니다.

---

## 6. Architecture Overview

### 6.1 Module Structure

```
src/
├── board/
│   ├── bitboard.rs     # 6 x u64 bitboard (direct access for eval)
│   └── board.rs        # Board state (stones + captures)
├── rules/
│   ├── capture.rs      # X-O-O-X capture logic (no alloc)
│   ├── win.rs          # Five-in-a-row + capture win + breakable five
│   └── forbidden.rs    # Double-three detection (consecutive + gapped)
├── eval/
│   ├── patterns.rs     # Score constants (hierarchy + non-linear captures)
│   └── heuristic.rs    # Position evaluation (direct bitboard, combos, vuln)
├── search/
│   ├── alphabeta.rs    # Negamax + AB + PVS + NMP + LMR + LMP + Futility
│   │                   # + RFP + Razoring + IID + Threat Ext. + VCF QS
│   │                   # + Lazy SMP (SharedState + WorkerSearcher)
│   ├── threat.rs       # VCF threat space search (depth 30)
│   ├── tt.rs           # Lock-free AtomicTT (XOR trick, 42-bit packing)
│   └── zobrist.rs      # Incremental Zobrist hashing (O(1) updates)
└── engine.rs           # 6-stage search pipeline + opening book
                        # + break-five + illusory break detection
```

### 6.2 Data Flow

```
Human/AI Move
    → Board.place_stone()      [O(1) bitboard]
    → execute_captures_fast()  [O(8 directions), no alloc]
    → check_winner()           [O(4 dirs at last move)]
    → AI Turn:
        → get_opening_move()   [O(1) book lookup]
        → find_five_positions()[O(N) breakable five check]
        → find_immediate_win() [O(N) + illusory break detection]
        → find_winning_moves() [O(N), opponent threats]
        → search_vcf()         [Depth 30, forcing fours only]
        → search_timed()       [Lazy SMP parallel iterative deepening]
            → alpha_beta()     [NMP+LMR+LMP+Futility+RFP+Razoring+PVS+IID+ThreatExt]
                → quiescence() [VCF QS: fives, fours, capture-wins]
                → evaluate()   [Direct bitboard: Pattern+Capture+Position-Vuln]
```

### 6.3 Make/Unmake Pattern

Throughout the search, we avoid cloning the board. Instead:

```rust
// Make move
board.place_stone(mov, color);
let cap_info = execute_captures_fast(board, mov, color);
let child_hash = zobrist.update_place(hash, mov, color);
// + update hash for each captured stone and capture count

// Search
let score = -alpha_beta(board, opponent, depth-1, ...);

// Unmake move
undo_captures(board, color, &cap_info);
board.remove_stone(mov);
```

This saves thousands of board allocations per search.

**한국어 설명**: 탐색 중 보드를 복사하지 않고 make/unmake 패턴을 사용합니다. `CaptureInfo`의 고정 배열 덕분에 힙 할당이 전혀 없습니다.

---

## 7. Code Review & Concepts / 코드 리뷰 & 개념 설명

### 7.1 핵심 개념: 왜 Negamax인가?

일반 Minimax에서는 maximizing player와 minimizing player를 별도로 처리해야 합니다:

```
// Minimax (복잡한 버전)
if maximizing:
    for each move: best = max(best, minimax(child, false))
else:
    for each move: best = min(best, minimax(child, true))
```

Negamax는 `score(A) = -score(B)` 대칭성을 이용하여 한 가지 경우만 처리합니다:

```
// Negamax (간단한 버전)
for each move:
    score = -negamax(child)  // 상대 점수의 반전 = 내 점수
    best = max(best, score)
```

**핵심 제약**: 평가 함수가 반드시 대칭이어야 합니다. `evaluate(board, Black) + evaluate(board, White) == 0`. 방어 보너스 같은 비대칭 요소는 evaluate()가 아닌 move ordering (score_move)에서 처리합니다.

### 7.2 핵심 개념: Alpha-Beta가 왜 정확한가?

Alpha-Beta pruning이 최적의 수를 놓치지 않는 이유:

1. **Alpha** = "내가 이미 보장한 최소 점수". 다른 경로에서 이미 alpha만큼을 보장받음.
2. **Beta** = "상대가 나에게 허용할 최대 점수". 상대 입장에서 다른 경로가 더 좋음.
3. **score >= beta** → 상대가 이 경로를 선택하지 않을 것 → 더 탐색해도 무의미.

```
       A (max)
      / \
     B   C (min)
    /|   |
   5  ?  3
```
A에서 B의 왼쪽 자식 = 5. B의 beta = 5.
C = 3. A의 alpha = max(5, 3?) → B의 alpha = 5.
B의 오른쪽 자식이 6이면: score(6) >= beta(5) → 커트! A는 이미 B(5)를 선택 가능.

### 7.3 핵심 개념: Transposition Table (TT)

같은 포지션에 다른 수순으로 도달하는 경우가 많습니다:
- A→B→C와 B→A→C는 같은 보드 상태

Zobrist 해싱은 XOR의 수학적 성질을 이용합니다:
- `a XOR b XOR c == b XOR a XOR c` (교환법칙)
- `a XOR a == 0` (자기역원 — 돌 제거 = 같은 값으로 다시 XOR)

TT에는 3가지 타입의 결과가 저장됩니다:
- **Exact**: 이 포지션의 정확한 minimax 값 (alpha < score < beta)
- **LowerBound**: 최소 이만큼 좋음 (beta cutoff 발생)
- **UpperBound**: 최대 이만큼 좋음 (alpha를 개선하지 못함)

### 7.4 핵심 개념: Null Move Pruning (NMP)

NMP의 직관: "내가 패스해도 여전히 좋다면, 정말로 수를 두면 더 좋을 것이다."

```
현재 포지션: 내 점수 = +8000 (beta = +5000)
패스 후 탐색: -(-7000) = +7000 >= beta → 커트!
```

**위험**: Gomoku에서는 "패스가 이득"인 상황(zugzwang)이 거의 없지만, 캡처 위협이 있을 때 NMP가 위험할 수 있습니다. 그래서 `is_threatened()` 체크가 필수입니다.

### 7.5 핵심 개념: 왜 LMR이 안전한가?

수 정렬이 좋으면 첫 번째 수가 최선일 확률이 ~90%. 나머지 수를 얕은 깊이로 먼저 탐색하고, 예상외로 좋으면 전체 깊이로 재탐색합니다.

**최악의 경우**: 좋은 수를 놓침 → 재탐색이 필요하지만 발생하지 않음 (최선 수의 점수가 낮게 나옴). 이는 PVS가 방지합니다: alpha를 넘으면 반드시 전체 윈도우로 재탐색.

### 7.6 코드 구조 리뷰

#### `src/eval/heuristic.rs` — 평가 함수

핵심 함수 3개:
1. **`evaluate(board, color)`**: 최상위 함수. 캡처 점수 + 패턴 점수 차이 - 취약성 패널티.
2. **`evaluate_color(board, color)`**: 한 색의 모든 돌에 대해 패턴+위치+연결성+취약성을 한 번에 계산. 직접 bitboard 접근 사용.
3. **`evaluate_line(my_bb, opp_bb, pos, dr, dc, prev_open)`**: 한 방향의 라인 패턴을 분석. 갭 허용 (OO_OO 등).

**Line-start filter**: 이전 위치에 같은 색 돌이 있으면 건너뜀 → 각 라인 세그먼트를 정확히 한 번만 계산. ~60% 함수 호출 제거.

#### `src/search/alphabeta.rs` — 탐색 엔진

핵심 구조:
- **`SharedState`**: 스레드 간 공유 (zobrist + tt + stopped 플래그)
- **`WorkerSearcher`**: 스레드별 상태 (killer/history/countermove 테이블)
- **`Searcher`**: 공개 API (search_timed, search, clear_tt 등)

핵심 함수:
1. **`search_timed()`**: Lazy SMP 진입점. 워커 스레드 생성 → 병렬 탐색 → 결과 병합.
2. **`search_iterative()`**: 반복 심화. aspiration window + 2-depth 확인 + history gravity.
3. **`search_root()`**: 루트 노드 탐색. MAX_ROOT_MOVES=30, PVS+LMR, threat extension.
4. **`alpha_beta()`**: 재귀 탐색. RFP→razoring→NMP→IID→move gen→futility→LMP→PVS+LMR.
5. **`quiescence()`**: VCF 정지 탐색. 강제 수만 확장. MAX_QS_DEPTH=16.
6. **`score_move()`**: 수 정렬. 8방향 스캔으로 포크 감지, 캡처 취약성 패널티.

#### `src/engine.rs` — AI 엔진

6단계 파이프라인 + 오프닝 북:
- 빠른 단계(0-2)는 O(N) 스캔으로 즉시 응답
- VCF(3-4)는 강제 4연속 탐색
- Alpha-Beta(5)는 적응적 시간 관리로 전체 탐색

### 7.7 학습한 교훈 (실전에서 발견한 버그와 해결)

| 문제 | 원인 | 해결 |
|------|------|------|
| Depth collapse (깊이 4에서 멈춤) | IID 캐스케이드 + LMR이 첫 2수 면제 + 넓은 aspiration | IID ≥6, LMR PV-only, LMP, 즉시 윈도우 확장 |
| 허위 승리 (AI가 승리라고 판단했지만 패배) | 단일 깊이 terminal 체크 | 2-depth 확인 (깊이 d와 d+1 모두 동의) |
| 무한 루프 (break→recreate→break→...) | Stage 0.5가 재생성 체크 안 함 | break 전 상대가 재생성 가능한지 검증 |
| 첫 캡처 과소평가 | CAP_WEIGHTS[1]=2,000이 CLOSED_THREE(1,500)과 비슷 | 5,000으로 증가 |
| VCT unsound | 열린 삼이 강제라고 가정 | VCT 제거, threat extension으로 대체 |
| VCF unsound with captures | 상대가 4를 무시하고 캡처 가능 | 전략적 캡처를 방어 수에 포함 |
| NMP R=3 too aggressive | 상대 응수를 놓침 (리플레이 → 오픈포) | R=2 고정 |

---

## Quick Defense Cheat Sheet / 빠른 디펜스 요약

### Q: "Minimax를 설명해주세요"
A: Negamax with alpha-beta pruning. 6-stage pipeline. Lazy SMP parallel search. NMP + LMR + LMP + Futility + RFP + Razoring + PVS + IID + Threat Extensions + VCF Quiescence. Depth 10-17. TT with Zobrist hashing. Aspiration windows with 2-depth win confirmation.

### Q: "Heuristic을 설명해주세요"
A: Direct bitboard access로 ~2x 빠른 패턴 스코링. 10x gap hierarchy (FIVE 1M > OPEN_FOUR 100K > ... > CLOSED_TWO 200). Gap pattern detection. Non-linear capture scoring (CAP_WEIGHTS: 0/5K/7K/20K/80K/1M). Capture vulnerability scaling (10K-80K by opponent captures). Combo detection. Open two development bonus. Negamax-symmetric.

### Q: "게임 규칙이 올바르게 구현되었나요?"
A: 196 tests. Five-in-a-row + capture win + breakable five + illusory break + double-three + capture exception. All validated.

### Q: "AI가 왜 졌나요?"
A: 19x19 Pente는 미해결 게임. Depth 10-17 = 10-17수 앞. 캡처로 비단조적 게임 트리. 전술적 horizon effect (aggressive pruning이 2수 시퀀스를 놓침). PDF도 100% 승률을 요구하지 않음. 로그를 보면 AI의 사고 과정을 확인 가능.

### Q: "시간 제한은 어떻게 지키나요?"
A: Iterative deepening + Lazy SMP. Min depth 10 무조건 완료. Adaptive time: opening 30-60%, mid-game 100%. 관측된 branching factor로 예측. 평균 < 500ms. Hard limit = soft + 150ms.

### Q: "어떤 최적화를 했나요?"
A: **탐색**: NMP(-80% nodes), LMR(logarithmic), LMP(zero overhead), Futility(depth 1-3), RFP(depth 1-3), Razoring(depth 1-3), PVS, IID(depth >= 6), Threat Extensions, VCF Quiescence, Aspiration Windows, 2-depth Confirmation, Adaptive Move Limits. **병렬**: Lazy SMP (lock-free AtomicTT, staggered depths, auto-detect cores). **수 정렬**: TT + Killer + History + Countermove + Fork detection + Capture vulnerability + Two-detection. **평가**: Direct bitboard access, line-start filter(-60% calls), unidirectional connectivity. **보드**: 6×u64 bitboard, make/unmake(no clone), allocation-free captures, incremental Zobrist.
