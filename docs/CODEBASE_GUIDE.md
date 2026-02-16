# Gomoku AI Engine - 코드베이스 완전 가이드

> 이 문서는 Gomoku AI 프로젝트의 **모든 알고리즘과 코드를 처음부터** 설명합니다.
> 게임 트리 탐색, 알파-베타 가지치기 등의 개념을 전혀 모르는 개발자도
> 이 문서를 읽고 전체 시스템을 이해할 수 있도록 작성되었습니다.

---

## 목차

1. [프로젝트 개요](#1-프로젝트-개요)
2. [게임 규칙 (Ninuki-renju)](#2-게임-규칙-ninuki-renju)
3. [보드 표현 - Bitboard](#3-보드-표현---bitboard)
4. [게임 규칙 구현 (rules/)](#4-게임-규칙-구현-rules)
5. [위치 평가 함수 (eval/)](#5-위치-평가-함수-eval)
6. [핵심 알고리즘: Minimax와 Alpha-Beta](#6-핵심-알고리즘-minimax와-alpha-beta)
7. [Zobrist 해싱과 Transposition Table](#7-zobrist-해싱과-transposition-table)
8. [반복 심화 탐색 (Iterative Deepening)](#8-반복-심화-탐색-iterative-deepening)
9. [가지치기 최적화들](#9-가지치기-최적화들)
10. [수 정렬 (Move Ordering)](#10-수-정렬-move-ordering)
11. [VCF 위협 탐색](#11-vcf-위협-탐색)
12. [AI 엔진 파이프라인](#12-ai-엔진-파이프라인)
13. [병렬 탐색 - Lazy SMP](#13-병렬-탐색---lazy-smp)
14. [전체 워크플로우: 한 수가 결정되기까지](#14-전체-워크플로우-한-수가-결정되기까지)
15. [파일 구조와 모듈 맵](#15-파일-구조와-모듈-맵)
16. [주요 상수와 설정값](#16-주요-상수와-설정값)

---

## 1. 프로젝트 개요

이 프로젝트는 **19x19 바둑판 위에서 플레이하는 Gomoku (오목) AI**입니다.
정확히는 **Ninuki-renju** 변형 규칙을 사용하며, 일반 오목과 달리
**캡처(돌 잡기)** 규칙이 추가되어 있습니다.

**기술 스택:**
- 언어: **Rust** (Python 대비 30~50배 성능 향상)
- GUI: **egui/eframe** (크로스 플랫폼 즉시모드 GUI)
- 알고리즘: **Negamax + Alpha-Beta 가지치기** + 다수의 최적화

**핵심 성능 목표:**
- 한 수당 평균 **0.5초 이내** 응답
- 최소 **depth 10** 이상의 탐색 깊이

---

## 2. 게임 규칙 (Ninuki-renju)

### 2.1 승리 조건 (둘 중 하나)

1. **5개 이상 연속** — 가로/세로/대각선으로 돌 5개 이상을 줄세우면 승리
2. **캡처 10개** — 상대 돌 10개(5쌍)를 잡으면 승리

### 2.2 캡처 규칙

```
X - O - O - X    →   X - _ - _ - X
(X가 양쪽에서 O-O 쌍을 감싸면 O-O가 제거됨)
```

- **정확히 2개(쌍)만** 잡을 수 있음 (1개나 3개 이상은 불가)
- 잡힌 자리는 다시 빈 칸이 됨
- **안전 규칙**: 상대 돌 사이에 끼어 놓는 것은 안전함
  - 예: `X-[빈칸]-O-X` 에서 O가 빈칸에 놓아도 잡히지 않음

### 2.3 깨질 수 있는 5목 (Breakable Five)

5목을 만들었더라도, 상대가 그 줄에서 캡처로 돌을 제거할 수 있다면
**즉시 승리가 아님**. 상대에게 한 번의 기회가 주어짐.

```
예: B-B-[B-B]-B-W  (W가 B-B 쌍을 잡을 수 있는 위치에 놓으면)
    B-B-_-_-B-W    (5목이 깨짐)
```

### 2.4 쌍삼 금지 (Double-Three)

한 수로 **열린 삼(free-three) 2개**를 동시에 만드는 것은 **금지**됨.

- **열린 삼**: 양쪽이 모두 열려 있는 3개 연속 돌
  - 연속형: `_-O-O-O-_`
  - 간격형: `_-O-O-_-O-_`
- **예외**: 캡처를 통해 만들어지는 쌍삼은 허용

---

## 3. 보드 표현 - Bitboard

### 3.1 개념

19x19 보드의 361개 교차점을 효율적으로 표현하기 위해 **비트보드(Bitboard)**를 사용합니다.

> **비트보드란?** 보드의 각 칸을 하나의 비트(0 또는 1)로 표현하는 자료구조.
> 361개 칸이 있으므로 6개의 64비트 정수(u64)가 필요합니다 (6 × 64 = 384 ≥ 361).

### 3.2 구현 (`src/board/bitboard.rs`)

```rust
pub struct Bitboard {
    bits: [u64; 6],   // 6개의 u64로 361비트 표현
}
```

**핵심 연산:**
- `set(pos)`: 특정 위치의 비트를 1로 설정 — O(1)
- `clear(pos)`: 특정 위치의 비트를 0으로 설정 — O(1)
- `get(pos)`: 특정 위치가 1인지 확인 — O(1)
- `count()`: 1인 비트의 총 개수 (`count_ones()` 사용) — O(1)
- `iter_ones()`: 모든 1인 위치를 순회 — `trailing_zeros`로 효율적 탐색

```rust
// 위치 → 비트 인덱스 변환
fn to_index(pos: Pos) -> usize {
    pos.row as usize * BOARD_SIZE + pos.col as usize  // 0~360
}

// 비트 인덱스 → (배열 인덱스, 비트 오프셋) 변환
let word = index / 64;     // 0~5 중 어떤 u64인지
let bit = index % 64;      // 그 u64의 몇 번째 비트인지
```

### 3.3 보드 구조 (`src/board/board.rs`)

```rust
pub struct Board {
    pub black: Bitboard,     // 흑돌 위치
    pub white: Bitboard,     // 백돌 위치
    captures: [u8; 2],       // [흑 캡처 수, 백 캡처 수]
}
```

- `place_stone(pos, color)`: 해당 색상의 비트보드에 비트 설정
- `remove_stone(pos)`: 양쪽 비트보드에서 비트 제거
- `get(pos)`: 흑/백 비트보드를 모두 확인하여 Stone::Black/White/Empty 반환
- `is_empty(pos)`: 두 비트보드 모두 0인지 확인 (get보다 빠름)
- `stone_count()`: 전체 돌 수 (`black.count() + white.count()`)

**왜 비트보드를 쓰는가?**
- 2D 배열 대비 캐시 효율이 좋음 (48바이트 vs 361바이트)
- 비트 연산으로 O(1)에 돌 설치/제거/확인
- `iter_ones()`로 모든 돌 위치를 효율적으로 순회

---

## 4. 게임 규칙 구현 (rules/)

### 4.1 캡처 (`src/rules/capture.rs`)

**핵심 함수:** `execute_captures_fast(board, pos, color)`

돌을 놓은 후 8방향(상하좌우 + 대각선 4방향)으로 캡처 패턴을 검사합니다.

```
패턴: 내돌(pos) - 상대 - 상대 - 내돌
                  ↑       ↑
              이 두 돌이 캡처됨
```

**제로 할당 구현**: `CaptureInfo` 구조체에 고정 크기 배열 사용
```rust
pub struct CaptureInfo {
    pub positions: [Pos; 16],  // 최대 16개 캡처 가능 (8방향 × 2개)
    pub count: u8,             // 실제 캡처된 돌 수
    pub pairs: u8,             // 캡처된 쌍의 수
}
```

**undo 지원**: `undo_captures(board, color, &cap_info)` — 캡처를 되돌림.
탐색 중 수를 놓았다가 되돌리는 **make/unmake 패턴**에 필수적.

### 4.2 승리 판정 (`src/rules/win.rs`)

```rust
pub fn check_winner(board: &Board) -> Option<Stone>
```

1. **캡처 승리 먼저 확인**: 어느 쪽이든 캡처 5쌍(10돌) 이상이면 승리
2. **5목 확인**: `has_five_at_pos()` — 마지막 놓은 돌에서 4방향만 스캔 (O(4))
3. **깨질 수 있는 5목**: `can_break_five_by_capture()` — 상대가 5목 줄에서
   캡처로 돌을 제거할 수 있는지 확인

**Illusory Break (환상적 깨기)**: 캡처로 5목을 깨더라도, 잡힌 돌의 "대괄호" 돌이
사라지면서 5목 소유자가 같은 자리에 다시 놓아 깨지지 않는 5목을 만들 수 있는 경우.
→ 이 경우 5목은 사실상 깨지지 않으므로 즉시 승리.

### 4.3 쌍삼 금지 (`src/rules/forbidden.rs`)

```rust
pub fn is_double_three(board: &Board, pos: Pos, color: Stone) -> bool
```

- 4방향을 모두 스캔하여 열린 삼(free-three)의 개수를 셈
- 2개 이상이면 쌍삼 → 해당 수는 금지
- 캡처가 발생하는 경우는 예외로 허용

**열린 삼 판정 기준** (`is_free_three()`):
- 돌 수가 정확히 3개
- 양쪽 끝이 모두 열려있음 (open_ends == 2)
- 전체 범위(span)가 4 이하 (갭 포함)

---

## 5. 위치 평가 함수 (eval/)

### 5.1 개념

> **평가 함수(Evaluation Function)**란?
> 현재 보드 상태가 누구에게 유리한지를 **숫자 하나**로 표현하는 함수.
> 양수 = 현재 턴 플레이어에게 유리, 음수 = 불리.

AI가 매 탐색 노드의 말단(leaf node)에서 이 함수를 호출하여
"이 위치에서 게임이 끝났다면 누가 이기고 있을까?"를 판단합니다.

### 5.2 패턴 점수 체계 (`src/eval/patterns.rs`)

```
FIVE         = 1,000,000    — 5목 완성 (승리)
OPEN_FOUR    =   100,000    — 양쪽 열린 4: 막을 수 없는 위협
CLOSED_FOUR  =    50,000    — 한쪽 막힌 4: 막을 수는 있지만 강제 수
OPEN_THREE   =    10,000    — 양쪽 열린 3: 강한 위협
CLOSED_THREE =     1,500    — 한쪽 막힌 3: 중간 위협
OPEN_TWO     =     1,000    — 양쪽 열린 2: 발전 가능성
CLOSED_TWO   =       200    — 한쪽 막힌 2: 약한 발전
```

**설계 원칙: 10배 간격** — 상위 패턴 하나가 하위 패턴 여러 개의 합보다 항상 큼.
예: OPEN_FOUR(100K) > CLOSED_FOUR(50K) + OPEN_THREE(10K) × 5

**캡처 점수** — 비선형 증가:
```
0쌍: 0    →   1쌍: 5,000    →   2쌍: 7,000
3쌍: 20,000   →   4쌍: 80,000   →   5쌍: 1,000,000 (승리)
```
3쌍 이후 급격히 증가 — 캡처 승리가 현실적인 위협이 됨.

### 5.3 평가 함수 상세 (`src/eval/heuristic.rs`)

```rust
pub fn evaluate(board: &Board, color: Stone) -> i32
```

**Negamax 대칭**: `evaluate(board, Black) == -evaluate(board, White)`
→ 항상 현재 턴 플레이어 기준으로 점수를 반환

**평가 = 캡처 점수 + 위치 점수 차이 - 취약성 패널티**

구성 요소:

1. **라인 패턴 점수** (`evaluate_line()`):
   - 각 돌에서 4방향으로 스캔
   - 연속된 같은 색 돌 수 + 양쪽 열린 끝 수 → 패턴 점수 대응표
   - **라인 시작 필터**: 이미 같은 색 돌이 이전에 있으면 건너뜀 (중복 계산 방지, 60% 호출 절감)

2. **콤보 보너스**: 한 색이 여러 방향에서 동시 위협을 가지면 추가 점수
   - 열린 4 + 아무거나 → +OPEN_FOUR
   - 닫힌 4 × 2 → +OPEN_FOUR
   - 열린 3 × 2 → +OPEN_FOUR

3. **위치 가중치** (center_bonus):
   - 중앙에 가까울수록 높은 점수: `(18 - 맨해튼거리) × 25`
   - 가중치 = 8 (center_bonus / 8 단위로 반영)

4. **연결성 보너스** (connectivity):
   - 4방향 양의 방향만 스캔 (단방향: 중복 없이 각 쌍을 정확히 1번 계산)
   - 인접한 같은 색 돌 발견 시 +160

5. **캡처 취약성 패널티**:
   - `empty-ally-ally-opp` 패턴 → 상대가 empty에 놓으면 ally 쌍이 잡힘
   - 패널티 가중치: 상대 캡처 수에 따라 지수적 증가
     - 0~1쌍: 10K / 2쌍: 20K / 3쌍: 40K / 4쌍: 80K

---

## 6. 핵심 알고리즘: Minimax와 Alpha-Beta

### 6.1 게임 트리란?

오목에서 "내가 여기에 놓으면 상대는 저기에 놓고, 그러면 나는..." 하는 식으로
**가능한 모든 수의 조합**을 트리(나무) 형태로 나타낸 것.

```
           현재 상태 (내 턴)
          /        |        \
       A에 놓기  B에 놓기  C에 놓기     (깊이 1: 내 수)
       /    \    /    \    /    \
     D에   E에 F에   G에 H에   I에     (깊이 2: 상대 수)
     ...   ... ...   ... ...   ...
```

### 6.2 Minimax

> 나는 점수를 **최대화**(Max), 상대는 점수를 **최소화**(Min)한다.

기본 아이디어:
1. 트리의 최하단(leaf)에서 평가 함수로 점수를 매김
2. 올라오면서 — 내 턴이면 자식 중 **최대값**, 상대 턴이면 **최소값** 선택
3. 최종적으로 루트에서 가장 높은 점수를 주는 수를 선택

### 6.3 Negamax

Minimax의 간소화 버전. **"상대 점수 = -내 점수"** 를 이용.

```rust
fn negamax(board, color, depth) -> i32 {
    if depth == 0 {
        return evaluate(board, color);  // 평가 함수 호출
    }

    let mut best = -INFINITY;
    for each move {
        make_move(board, move, color);
        let score = -negamax(board, opponent, depth - 1);
        unmake_move(board, move);
        best = max(best, score);
    }
    return best;
}
```

**핵심**: 재귀 호출 시 부호를 뒤집음 (`-negamax(...)`)
→ Max/Min 레이어를 별도로 구현할 필요 없음

### 6.4 Alpha-Beta 가지치기

Minimax는 모든 노드를 탐색하므로 너무 느림.
**Alpha-Beta**는 "어차피 선택되지 않을 가지"를 건너뛰어 탐색량을 줄임.

```rust
fn alpha_beta(board, color, depth, alpha, beta) -> i32 {
    // alpha: 현재까지 내가 보장받은 최소 점수
    // beta:  상대가 나에게 허용하는 최대 점수

    if depth == 0 { return evaluate(board, color); }

    for each move {
        make_move(board, move, color);
        let score = -alpha_beta(board, opponent, depth-1, -beta, -alpha);
        unmake_move(board, move);

        if score >= beta {
            return beta;  // Beta cutoff: 상대가 이 가지를 선택하지 않을 것
        }
        alpha = max(alpha, score);
    }
    return alpha;
}
```

**직관적 이해:**
- 내가 이미 점수 50을 보장받았는데 (alpha=50)
- 상대 입장에서 자기 다른 가지에서 40만 허용하면 (beta=40)
- 이 가지는 상대가 절대 선택 안 함 → **잘라버림 (prune)**

**실제 코드 위치**: `src/search/alphabeta.rs` → `WorkerSearcher::alpha_beta()`

---

## 7. Zobrist 해싱과 Transposition Table

### 7.1 Zobrist 해싱 (`src/search/zobrist.rs`)

> **문제**: 같은 보드 상태에 여러 경로로 도달할 수 있음.
> A→B와 B→A는 다른 경로지만 같은 결과 → 중복 탐색 낭비.

**해결**: 보드 상태마다 고유한 해시값을 계산하여 이미 탐색한 결과를 재사용.

**Zobrist 해싱 원리:**
1. 초기화: 모든 (위치, 색) 조합에 랜덤 64비트 값을 미리 생성
   ```rust
   black: [u64; 361],  // 각 위치에 흑돌이 있을 때의 값
   white: [u64; 361],  // 각 위치에 백돌이 있을 때의 값
   black_to_move: u64, // 흑 턴일 때의 값
   captures: [[u64; 6]; 2], // 캡처 카운트별 값
   ```

2. 해시 계산: 보드의 모든 돌에 대한 값을 XOR
   ```
   hash = black[pos1] XOR white[pos2] XOR ... XOR 턴 XOR 캡처수
   ```

3. **증분 업데이트 (O(1))**: 돌을 놓거나 제거할 때
   ```rust
   // XOR은 자기 역원: a XOR b XOR b = a
   new_hash = old_hash XOR stone_value XOR side_toggle;
   ```
   → 전체 해시를 다시 계산할 필요 없이 변경된 부분만 XOR

### 7.2 Transposition Table (`src/search/tt.rs`)

> **Transposition Table(TT)**이란?
> 이미 탐색한 보드 상태의 결과를 저장하는 해시 테이블.
> 같은 위치를 다시 만나면 저장된 결과를 즉시 반환.

**저장 정보 (TTEntry):**
```rust
struct TTEntry {
    hash: u64,             // 검증용 해시
    depth: i8,             // 이 결과가 탐색된 깊이
    score: i32,            // 평가 점수
    entry_type: EntryType, // Exact / LowerBound / UpperBound
    best_move: Option<Pos>,// 최선의 수
}
```

**EntryType 의미:**
- `Exact`: 정확한 점수 (alpha < score < beta)
- `LowerBound`: 실제 점수는 이 이상 (beta cutoff)
- `UpperBound`: 실제 점수는 이 이하 (fail-low)

**사용 규칙:**
- 저장된 탐색 깊이가 현재 요구 깊이 이상이어야 score 사용 가능
- 깊이가 부족해도 `best_move`는 수 정렬에 활용 가능

### 7.3 Lock-free AtomicTT (병렬 탐색용)

멀티스레드에서 동시 접근을 위한 **lock-free** 구현:

```
저장: key = hash XOR data, data = packed_entry
검증: if key XOR data == original_hash → 유효
```

- **XOR 트릭 (Hyatt 1994)**: key와 data를 동시에 읽었을 때 일관성 검증
- 한쪽만 업데이트된 "찢어진 읽기(torn read)" → 해시 불일치 → 안전한 캐시 미스

**데이터 패킹 (42비트):**
```
bits [0..7]   depth (8비트)
bits [8..28]  score (21비트)
bits [29..30] entry_type (2비트)
bits [31]     has_move (1비트)
bits [32..36] row (5비트)
bits [37..41] col (5비트)
```

---

## 8. 반복 심화 탐색 (Iterative Deepening)

### 8.1 개념

> 깊이 1부터 시작하여 점점 깊이를 늘려가며 탐색.
> 왜 처음부터 깊이 10으로 안 하고 1부터?

**이유 3가지:**

1. **시간 관리**: 각 깊이가 끝날 때마다 "다음 깊이까지 갈 시간이 있나?" 판단 가능
2. **수 정렬 개선**: 얕은 탐색의 결과(TT)가 깊은 탐색의 수 정렬을 도움
3. **언제든 중단 가능**: 시간 초과 시 마지막 완료된 깊이의 결과 사용

### 8.2 구현 (`search_iterative()`)

```rust
for depth in 1..=max_depth {
    // 1. Aspiration Window로 탐색
    let result = search_root(board, color, depth, asp_alpha, asp_beta);

    // 2. 시간 체크: 다음 깊이까지 갈 수 있는가?
    let estimated_next = depth_time * branching_factor;
    if estimated_next > remaining_time { break; }

    // 3. 2-depth 승리 확인: 연속 2개 깊이에서 승리를 확인해야 진짜 승리
    if is_winning && prev_was_winning { break; }
}
```

### 8.3 Aspiration Window

전체 [-INF, INF] 대신 이전 깊이 점수 ± 100의 좁은 창으로 탐색.
→ 좁은 창 = 더 많은 beta cutoff = 더 빠른 탐색

실패 시(fail-low/fail-high) → 즉시 전체 창으로 재탐색 (점진적 확장 없음)

### 8.4 2-depth 승리 확인

```
깊이 10에서 "나 승리!" → 정말?
깊이 11에서 "아니요, 상대가 반박함" → 거짓 승리!
```

연속 2개 깊이에서 모두 승리/패배를 확인해야 early exit 허용.
→ **환상적 승리(illusory win)** 방지

---

## 9. 가지치기 최적화들

기본 Alpha-Beta만으로는 depth 10이 불가능합니다.
다양한 가지치기(pruning) 기법으로 탐색량을 극적으로 줄입니다.

### 9.1 Null Move Pruning (NMP)

> **아이디어**: "내 턴을 건너뛰어도(패스해도) 점수가 좋으면, 실제로 수를 두면 더 좋겠지?"
> → 패스 후 얕은 탐색으로 확인, 여전히 좋으면 진짜 탐색 생략.

```
if 내 eval >= beta AND 위협 상태 아닌 경우:
    null_score = 상대 턴으로 (depth - 1 - R) 탐색  (R = 감소량 = 2)
    if null_score >= beta:
        return beta  (이 위치는 이미 충분히 좋음)
```

**안전장치:**
- `is_threatened()` 체크: 상대가 즉시 4목이나 열린 3 위협을 가하면 NMP 비활성
- `depth > 8`이면 검증 탐색(verification search) 실행
- `allow_null` 플래그로 연속 NMP 방지

**효과**: 노드 수 80% 감소, 시간 91% 절감 (프로젝트 최고 ROI)

### 9.2 Late Move Reduction (LMR)

> **아이디어**: 수 정렬에서 뒤쪽 수(Late Move)는 좋은 수일 확률이 낮음.
> → 이런 수는 줄인 깊이로 먼저 탐색하고, 의외로 좋으면 정상 깊이로 재탐색.

```
reduction = sqrt(depth) * sqrt(move_index) / 2
if move_score < 500K: reduction += 1  // 전술적 가치 없는 조용한 수

줄인 깊이로 PVS(null window) 탐색
→ alpha 넘으면 정상 깊이로 재탐색
```

**면제 조건**: PV 수(i=0), 캡처, 위협 확장, 얕은 깊이

### 9.3 Late Move Pruning (LMP)

> LMR보다 더 극단적: 얕은 깊이에서 뒤쪽 조용한 수를 아예 탐색 안 함.

```
if depth <= 3 AND move_index >= threshold AND move_score < 800K:
    skip  // make_move조차 안 함 → 오버헤드 제로
```

threshold = 3 + depth × 2 (depth 1: 5개, depth 2: 7개, depth 3: 9개)

### 9.4 Futility Pruning

> 얕은 깊이에서 정적 평가 + 여유값이 alpha 이하이면, 조용한 수는 건너뜀.

```
depth 1: margin = CLOSED_FOUR (50K)
depth 2: margin = OPEN_FOUR (100K)
depth 3: margin = OPEN_FOUR + OPEN_THREE (110K)

if static_eval + margin <= alpha AND move_score < 800K:
    skip
```

### 9.5 Reverse Futility Pruning (RFP)

> Futility의 반대: 정적 평가가 beta를 크게 넘으면 바로 cutoff.

```
if depth <= 3 AND static_eval - OPEN_THREE * depth >= beta:
    return static_eval
```

### 9.6 Razoring

> 정적 평가가 alpha를 크게 밑돌면, quiescence search로 확인 후 cutoff.

```
if depth <= 3 AND static_eval + OPEN_THREE * depth <= alpha:
    qs_score = quiescence(board, ...)
    if qs_score <= alpha:
        return qs_score
```

### 9.7 Principal Variation Search (PVS)

> 첫 번째 수(PV 수)만 전체 창으로 탐색, 나머지는 **null window** `[alpha, alpha+1]`로 탐색.
> null window 탐색이 실패하면(점수가 alpha를 넘으면) 전체 창으로 재탐색.

첫 번째 수가 최선이라는 가정 하에 나머지를 빠르게 기각하는 전략.

### 9.8 Threat Extension

> 4목(four)을 만드는 수는 강제적(1~2개의 응수만 가능).
> → 이런 수에 +1 깊이를 추가하여 전술적 연속성을 놓치지 않음.

```
if depth >= 2 AND move_creates_four(board, mov, color):
    extension = 1  // 사실상 무료 깊이 추가 (좁은 서브트리)
```

### 9.9 Internal Iterative Deepening (IID)

> TT에 정보가 없는 깊은 노드에서, 얕은 탐색을 먼저 실행하여
> 좋은 수를 찾고 이를 첫 번째로 탐색.

```
if tt_move 없음 AND depth >= 6:
    alpha_beta(depth - 4)  // 얕은 탐색
    tt_move = TT에서 결과 가져오기
```

threshold를 4에서 6으로 올림 — 낮은 깊이에서의 **IID 캐스케이드** 방지.

---

## 10. 수 정렬 (Move Ordering)

### 10.1 중요성

Alpha-Beta의 효율은 **수 정렬의 질**에 결정적으로 의존합니다.
최선의 수를 먼저 탐색하면 더 많은 가지를 잘라낼 수 있습니다.

목표 지표: **first-move cutoff rate ~90%** (첫 번째 수에서 beta cutoff 발생 비율)

### 10.2 우선순위 단계 (`score_move()`)

모든 후보 수는 점수를 부여받고, 높은 점수부터 탐색됩니다.

```
1,000,000  TT 수 (이전 탐색에서 찾은 최선수)
  900,000  나의 5목 (즉시 승리)
  895,000  상대 5목 차단
  890,000  캡처 승리 (5쌍 완성)
  885,000  상대 캡처 승리 차단
  880,000  나의 이중 4목 포크 (4+4)
  878,000  나의 4+3 포크 (4+열린3)
  870,000  나의 열린 4
  868,000  상대 이중 4목 차단
  866,000  상대 4+3 차단
  860,000  상대 열린 4 차단
  855,000  상대 캡처 긴급 (3+ 쌍)
  840,000  나의 이중 열린 3
  838,000  상대 이중 열린 3 차단
  830,000  나의 닫힌 4
  820,000  상대 닫힌 4 차단
  810,000  나의 열린 3
  800,000  상대 열린 3 차단
  600,000+ 캡처 수
  550,000+ 상대 캡처 위치 차단
  500,000  킬러 수 1
  490,000  킬러 수 2
  400,000  카운터무브
  나머지    히스토리 + 중앙 보너스 + 근접 보너스 + 발전 보너스
```

### 10.3 킬러 수 (Killer Moves)

같은 깊이(ply)에서 이전에 beta cutoff를 발생시킨 수를 기억.
→ 형제 노드에서 먼저 시도 (유사한 위치에서는 같은 수가 좋을 확률 높음)

```rust
killer_moves: [[Option<Pos>; 2]; 64]  // ply당 최대 2개
```

### 10.4 히스토리 휴리스틱 (History Heuristic)

각 (색, 위치) 조합이 beta cutoff를 발생시킨 빈도를 기록.
→ 자주 좋았던 수에 높은 점수 부여

```rust
history: [[[i32; 19]; 19]; 2]  // [색][행][열]

// Beta cutoff 시:
history[color][row][col] += depth * depth
```

**History Gravity**: 각 반복 심화 단계마다 모든 히스토리 값을 절반으로 줄임
→ 오래된 정보보다 최근 정보를 우선

### 10.5 카운터무브 (Countermove Heuristic)

상대의 마지막 수에 대한 최적 응수를 기록.

```rust
countermove: [[[Option<Pos>; 19]; 19]; 2]  // [상대색][행][열]
```

상대가 (r, c)에 놓았을 때 이전에 좋았던 응수를 먼저 시도.

### 10.6 캡처 취약성 패널티

수를 놓으면 자신의 돌이 잡힐 수 있는 패턴이 생기는 경우 감점:
- `opp-ME-ally-empty` : 1턴 내 캡처 위협 → -150K
- `empty-ME-ally-empty` : 2턴 내 캡처 셋업 → -50K~100K (상대 캡처 수에 비례)

---

## 11. VCF 위협 탐색

### 11.1 개념

> **VCF (Victory by Continuous Fours)**:
> 연속된 4목 위협만으로 강제 승리하는 수순을 탐색.

일반 Alpha-Beta보다 훨씬 빠름 (forcing move만 탐색하므로):
- 매 수가 4목 → 상대는 반드시 1~2곳 중 하나에서 방어
- 방어 후 다시 4목 → 반복
- 결국 방어 불가능한 5목 달성

### 11.2 구현 (`src/search/threat.rs`)

```rust
fn vcf_search_mut(board, color, depth, sequence) -> bool {
    // 1. 4목을 만드는 모든 수 찾기
    let threats = find_four_threats(board, color);

    for each threat_move {
        // 2. 수를 놓고 즉시 승리 확인
        make_move(threat_move);
        if has_five() && !breakable() { return true; }

        // 3. 깨질 수 있는 5목이면 건너뜀
        if breakable_five { unmake(); continue; }

        // 4. 상대 방어 수 찾기
        let defenses = find_defense_moves(board, threat_move, color);

        if defenses.is_empty() { return true; }   // 방어 불가 = 승리
        if defenses.len() == 1 {
            // 5. 유일한 방어: 상대가 여기에 놓을 수밖에 없음
            make_move(defense);
            let win = vcf_search_mut(board, color, depth+1);
            unmake_move(defense);
            if win { return true; }
        }
        // 여러 방어가 있으면 VCF 실패 (상대에게 선택지가 있음)
        unmake_move(threat_move);
    }
    return false;
}
```

### 11.3 방어 수 유형

VCF에서 상대의 방어 수:
1. **차단**: 4목의 열린 끝을 막기
2. **캡처로 깨기**: 4목 줄의 돌을 잡기
3. **전략적 캡처**: 상대 캡처 3쌍 이상이면 아무 캡처도 방어로 인정
   (캡처 승리에 가까워지면 4목 무시하고 캡처 전략이 유효)

### 11.4 VCF의 한계 (Ninuki-renju에서)

표준 오목에서 VCF는 완전히 올바르지만(sound), **Ninuki-renju에서는 unsound**:
- 상대가 4목을 무시하고 캡처할 수 있음
- 해결: 방어 수에 전략적 캡처를 포함
- 상대 캡처 4쌍 이상이면 VCF 자체를 비활성

---

## 12. AI 엔진 파이프라인

### 12.1 6단계 우선순위 파이프라인 (`src/engine.rs`)

AI가 한 수를 결정하는 과정은 **6단계의 우선순위 파이프라인**입니다.
각 단계는 이전 단계에서 수를 찾지 못했을 때만 실행됩니다.

```
┌─────────────────────────────────────────────────────────────┐
│  Stage 0:  Opening Book (오프닝 북)                          │
│  → 빈 보드: 중앙(K10)                                       │
│  → 3~4수: 상대 대각선 확장 방어                               │
│                                                              │
│  Stage 0.5: Break Five (5목 깨기)                            │
│  → 상대가 깨질 수 있는 5목을 가지고 있으면 캡처로 깨기         │
│  → 환상적 깨기(illusory break) 감지                          │
│  → 재생성 가능 여부 확인                                      │
│                                                              │
│  Stage 1: Immediate Win (즉시 승리)                          │
│  → 5목 완성 가능? (환상적 깨기 포함)                          │
│  → 캡처 승리 가능?                                           │
│                                                              │
│  Stage 2: Block Opponent (상대 위협 차단)                     │
│  → 상대의 열린 4, 캡처 승리 등 즉시 위협 차단                 │
│                                                              │
│  Stage 3: Our VCF (우리의 강제 승리 탐색)                     │
│  → 연속 4목으로 강제 승리 시퀀스 존재?                        │
│  → 상대 캡처 4쌍 이상이면 비활성                              │
│                                                              │
│  Stage 4: Opponent VCF (상대의 강제 승리 방어)                │
│  → 상대에게 VCF가 있다면 방어 수 탐색                         │
│                                                              │
│  Stage 5: Alpha-Beta Search (일반 탐색)                      │
│  → 적응적 시간 제한으로 깊은 탐색                             │
│  → 반복 심화 + 모든 가지치기 기법 적용                        │
└─────────────────────────────────────────────────────────────┘
```

### 12.2 시간 관리

```rust
fn compute_time_limit(base_limit_ms, stone_count) -> u64 {
    match stone_count {
        0..=2  => base * 0.30,  // 초반: 시간 절약
        3..=4  => base * 0.60,  // 초중반: 중간
        _      => base * 1.00,  // 중반 이후: 전체 사용
    }
    최소 300ms 보장
}
```

### 12.3 Quiescence Search (정적 탐색)

> Alpha-Beta의 깊이가 0에 도달하면 바로 evaluate()를 호출하지 않고,
> **강제적 수(forcing move)**만 추가로 탐색하여 **수평선 효과**를 제거.

**수평선 효과**: "3수 후에 5목이 완성되는데, 탐색 깊이가 2라서 못 봄"

```
Quiescence에서 탐색하는 수:
- 5목 완성 (priority 900)
- 상대 5목 차단 (priority 850)
- 캡처 승리 (priority 890)
- 4목 생성 (priority 700~800, depth 6까지만)

Stand-pat: 강제 수가 없거나 현재 eval이 이미 충분히 좋으면 static eval 반환
```

---

## 13. 병렬 탐색 - Lazy SMP

### 13.1 개념

> 여러 CPU 코어에서 **같은 위치**를 **다른 깊이**로 동시에 탐색.
> 결과는 공유 TT를 통해 자연스럽게 공유됨.

### 13.2 아키텍처

```
┌──────────────────────────────────────────┐
│           SharedState (Arc)               │
│  ┌─────────────────────────────────────┐ │
│  │  ZobristTable (해시 테이블)          │ │
│  │  AtomicTT (lock-free TT)           │ │
│  │  AtomicBool (전역 정지 신호)        │ │
│  └─────────────────────────────────────┘ │
│        ↑          ↑          ↑           │
│   Worker 0   Worker 1   Worker 2  ...    │
│   (depth d)  (depth d+1) (depth d+2)    │
│  ┌────────┐ ┌────────┐ ┌────────┐       │
│  │Killers │ │Killers │ │Killers │       │
│  │History │ │History │ │History │       │
│  │Counter │ │Counter │ │Counter │       │
│  └────────┘ └────────┘ └────────┘       │
└──────────────────────────────────────────┘
```

- 각 Worker는 독립적인 killer/history/countermove 테이블 보유
- TT만 공유 → lock 없이 자연스러운 정보 교환
- 워커 시작 깊이를 엇갈리게 하여 트리 다양성 확보
- 코어 수 자동 감지 (최대 8)

### 13.3 `search_timed()` 흐름

```rust
fn search_timed(board, color, max_depth, time_limit_ms) -> SearchResult {
    let shared = Arc::new(SharedState { ... });

    // Worker 0: 메인 스레드에서 실행
    let main_result = worker0.search_iterative(board, color, max_depth, 0);

    // Worker 1~N: 별도 스레드에서 엇갈린 깊이로 시작
    for i in 1..num_workers {
        spawn(move || worker_i.search_iterative(board, color, max_depth, i));
    }

    // 시간 초과 → stopped 신호 → 모든 워커 중단
    // 결과: 가장 깊이 탐색한 워커의 결과 + 통계 병합
}
```

---

## 14. 전체 워크플로우: 한 수가 결정되기까지

사용자가 돌을 놓은 후 AI가 응수를 결정하는 전체 과정:

```
1. GUI에서 사용자 클릭 → board.place_stone()
   ↓
2. engine.get_move_with_stats(board, AI_color) 호출
   ↓
3. [Stage 0] 오프닝 북 확인
   - 빈 보드? → 중앙 반환
   - 3~4수? → 대각선 방어 패턴 확인
   ↓ (없으면)
4. [Stage 0.5] 상대 깨질 수 있는 5목 확인
   - find_five_positions() → can_break_five_by_capture()
   - 환상적 깨기인지 is_illusory_break() 확인
   ↓ (없으면)
5. [Stage 1] 즉시 승리 확인
   - 모든 빈 칸에서 has_five_at_pos() 체크
   - 캡처 승리: captures + count_captures_fast >= 5
   ↓ (없으면)
6. [Stage 2] 상대 즉시 위협 차단
   - 상대의 열린4 차단, 캡처 승리 차단
   ↓ (없으면)
7. [Stage 3] 우리 VCF 탐색 (상대 캡처 < 4일 때만)
   - ThreatSearcher::search_vcf()
   ↓ (없으면)
8. [Stage 4] 상대 VCF 탐색 (우리 캡처 < 4일 때만)
   - 상대에게 VCF가 있다면 그 첫 수를 차단
   ↓ (없으면)
9. [Stage 5] Alpha-Beta 탐색
   a. compute_time_limit()으로 시간 예산 계산
   b. Searcher::search_timed() 호출
   c. Lazy SMP로 멀티코어 병렬 탐색 시작
   d. 각 워커: search_iterative()
      i.  depth 1부터 시작
      ii. 각 깊이에서 aspiration window로 search_root()
      iii. search_root() → generate_moves_ordered()로 후보 생성
      iv. 각 후보에 대해 alpha_beta() 재귀 호출
          - NMP, RFP, Razoring, Futility, LMP, LMR, PVS 적용
          - depth 0에서 quiescence() 호출
      v.  TT에 결과 저장
      vi. 시간 예측: 다음 깊이 가능? → 반복 or 중단
   e. 결과 수집: 가장 깊은 완료 깊이의 결과 사용
   ↓
10. 결과 반환: (best_move, score, depth, nodes, time)
    ↓
11. GUI에서 board.place_stone(best_move, AI_color)
    + 캡처 처리 + 승리 판정
```

---

## 15. 파일 구조와 모듈 맵

```
src/
├── lib.rs                  # 라이브러리 진입점, 모듈 re-export
├── main.rs                 # GUI 진입점 (egui/eframe)
├── engine.rs               # AI 엔진: 6단계 파이프라인
│
├── board/
│   ├── mod.rs              # 모듈 정의
│   ├── bitboard.rs         # 6×u64 비트보드 (set/clear/get/iter)
│   └── board.rs            # Board 구조체 (흑/백 비트보드 + 캡처)
│
├── rules/
│   ├── mod.rs              # 모듈 정의
│   ├── capture.rs          # X-O-O-X 캡처 + undo
│   ├── win.rs              # 승리 판정 (5목 + 캡처 + 깨질수있는5목)
│   └── forbidden.rs        # 쌍삼 금지 + 열린삼 판정
│
├── eval/
│   ├── mod.rs              # 모듈 정의
│   ├── patterns.rs         # 점수 상수 (FIVE=1M, OPEN_FOUR=100K, ...)
│   └── heuristic.rs        # evaluate() 함수 (패턴+위치+연결+취약성)
│
├── search/
│   ├── mod.rs              # 모듈 정의
│   ├── alphabeta.rs        # Alpha-Beta + ID + 모든 가지치기 + Lazy SMP
│   ├── threat.rs           # VCF/VCT 위협 탐색
│   ├── tt.rs               # Transposition Table (일반 + AtomicTT)
│   └── zobrist.rs          # Zobrist 해싱 (증분 업데이트)
│
└── ui/
    ├── mod.rs              # UI 모듈 정의
    └── game_state.rs       # 게임 상태 관리 (GUI ↔ Engine)
```

**의존 관계:**
```
engine → search, eval, rules, board
search → eval, rules, board
eval   → board, rules
rules  → board
board  → (독립)
ui     → engine, board, rules
```

---

## 16. 주요 상수와 설정값

| 상수 | 값 | 설명 |
|------|-----|------|
| `BOARD_SIZE` | 19 | 보드 크기 |
| `TOTAL_CELLS` | 361 | 총 교차점 수 (19×19) |
| `INF` | 1,000,001 | Alpha-Beta 무한값 |
| `MAX_ROOT_MOVES` | 30 | 루트에서 최대 후보 수 |
| `MAX_QS_DEPTH` | 16 | Quiescence 최대 깊이 |
| `ASP_WINDOW` | 100 | Aspiration window 크기 |
| `POSITION_WEIGHT` | 8 | 위치 가중치 스케일 |
| `soft_limit` | 500ms | 기본 시간 제한 |
| `min_depth` | 10 (8) | 최소 탐색 깊이 (초반은 8) |
| TT 크기 | 16MB | Transposition Table 크기 |
| VCF depth | 30 | VCF 최대 탐색 깊이 |
| VCT depth | 20 | VCT 최대 탐색 깊이 |

---

## 부록: 용어 사전

| 용어 | 설명 |
|------|------|
| Alpha | 현재까지 내가 보장받은 최소 점수 |
| Beta | 상대가 나에게 허용하는 최대 점수 |
| Beta cutoff | 점수 >= beta → 이 가지는 상대가 선택 안 함 |
| Branching factor | 각 노드의 평균 자식 수 |
| Depth | 탐색 깊이 (몇 수 앞까지 보는지) |
| Evaluation | 보드 상태의 점수 계산 |
| Forcing move | 상대가 응수할 수밖에 없는 수 (4목 등) |
| Horizon effect | 탐색 깊이 한계 너머의 위협을 못 보는 현상 |
| Leaf node | 탐색 트리의 말단 노드 |
| Make/unmake | 수를 놓고/되돌리는 패턴 (복사 없이 효율적 탐색) |
| Negamax | Max/Min을 부호 반전으로 통합한 Minimax |
| Node | 탐색 트리의 하나의 상태 |
| NPS | Nodes Per Second (초당 탐색 노드 수) |
| Ply | 한 수 (half-move) |
| Pruning | 탐색하지 않아도 되는 가지를 제거 |
| PV (Principal Variation) | 양쪽이 최선을 다할 때의 수순 |
| Quiescence | 강제 수만 추가 탐색하여 수평선 효과 제거 |
| TT | Transposition Table (위치 캐시) |
| VCF | Victory by Continuous Fours (연속 4목 승리) |
| Zobrist hash | XOR 기반 증분 해시 |
