# Gomoku AI 프로젝트 설계 문서

> **프로젝트 목표**: 인간을 이길 수 있는 Gomoku AI 개발
> **작성일**: 2025-01-31
> **버전**: 1.0

---

## 목차

1. [프로젝트 개요](#1-프로젝트-개요)
2. [기술 스택](#2-기술-스택)
3. [아키텍처 개요](#3-아키텍처-개요)
4. [Min-Max 알고리즘](#4-min-max-알고리즘)
5. [Alpha-Beta Pruning](#5-alpha-beta-pruning)
6. [Iterative Deepening](#6-iterative-deepening)
7. [휴리스틱 함수](#7-휴리스틱-함수)
8. [Bitboard 구현](#8-bitboard-구현)
9. [Move Ordering 전략](#9-move-ordering-전략)
10. [게임 규칙 구현](#10-게임-규칙-구현)
11. [UI 구조](#11-ui-구조)
12. [디버그 패널](#12-디버그-패널)
13. [파일 구조 및 구현 순서](#13-파일-구조-및-구현-순서)

---

## 1. 프로젝트 개요

### 요구사항 요약

| 항목 | 요구사항 |
|------|----------|
| **실행 파일명** | `Gomoku` |
| **보드 크기** | 19x19 |
| **승리 조건** | 5개 이상 연속 OR 10개 캡처 |
| **특수 규칙** | 캡처, 쌍삼 금지, Endgame Capture |
| **AI 탐색 깊이** | 최소 10레벨 |
| **AI 응답 시간** | 평균 0.5초 이하 |
| **필수 UI 요소** | AI 계산 시간 타이머 |

### 게임 모드

1. **AI vs Human**: AI가 인간을 이겨야 함
2. **Human vs Human (Hotseat)**: 수 추천 기능 포함

---

## 2. 기술 스택

| 항목 | 선택 | 이유 |
|------|------|------|
| 언어 | Python | 빠른 개발, 풍부한 레퍼런스 |
| GUI | Pygame | 가장 대중적, 안정적, 2D 게임에 적합 |
| AI 알고리즘 | Alpha-Beta + Iterative Deepening | 시간 제한 준수, 안정성 |
| 휴리스틱 | 패턴 기반 점수 테이블 | 표준 접근법, 디버깅 용이 |
| 보드 표현 | Bitboard | 고성능 비트 연산 |
| Move Ordering | 복합 전략 | Alpha-Beta 효율 극대화 |

---

## 3. 아키텍처 개요

```
┌────────────────────────────────────────────────────────────────┐
│                        main.py                                  │
│                    (Game Loop & Entry)                          │
└──────────────────────┬─────────────────────────────────────────┘
                       │
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│   UI Layer   │ │  Game Layer  │ │   AI Layer   │
│  (Pygame)    │ │   (Rules)    │ │  (Engine)    │
└──────────────┘ └──────────────┘ └──────────────┘
        │              │              │
        │              ▼              │
        │       ┌──────────────┐      │
        └──────►│   Bitboard   │◄─────┘
                │ (Board State)│
                └──────────────┘
```

### 데이터 흐름

1. `main.py`: 게임 루프 실행, 모드 선택
2. **UI Layer**: 보드 렌더링, 입력 처리, 타이머/디버그 패널 표시
3. **Game Layer**: 규칙 검증 (캡처, 쌍삼금지, 승리조건), 상태 전이
4. **AI Layer**: 수 계산, Iterative Deepening으로 시간 내 최선의 수 반환
5. **Bitboard**: 모든 레이어가 공유하는 핵심 데이터 구조

---

## 4. Min-Max 알고리즘

### 핵심 아이디어

"상대방도 최선의 수를 둔다고 가정하고, 그 상황에서 나에게 가장 유리한 수를 선택한다."

```
나의 턴 (MAX): 가장 높은 점수를 선택
    │
    ├── 상대 턴 (MIN): 가장 낮은 점수를 선택
    │       │
    │       ├── 나의 턴 (MAX): 가장 높은 점수를 선택
    │       │       │
    │       │       └── ... (반복)
```

### 예시

```
현재 내 턴. 3가지 수가 가능:

         [나의 선택]
         /    |    \
       A      B      C        ← 내가 선택 가능한 수
      /|\    /|\    /|\
     3 5 2  1 4 6  7 2 1      ← 최종 보드 점수

상대방 턴 (MIN):
  A → min(3,5,2) = 2
  B → min(1,4,6) = 1
  C → min(7,2,1) = 1

나의 턴 (MAX):
  max(2, 1, 1) = 2 → A 선택!
```

### 의사 코드

```python
def minimax(board, depth, is_maximizing):
    if depth == 0 or game_over(board):
        return heuristic(board)

    if is_maximizing:
        best = -infinity
        for move in get_all_moves(board):
            board.make_move(move)
            score = minimax(board, depth-1, False)
            board.undo_move(move)
            best = max(best, score)
        return best
    else:
        best = +infinity
        for move in get_all_moves(board):
            board.make_move(move)
            score = minimax(board, depth-1, True)
            board.undo_move(move)
            best = min(best, score)
        return best
```

### 문제점

시간 복잡도: O(b^d)
- b = 분기 계수 ≈ 50
- d = 탐색 깊이 = 10
- 50^10 ≈ 9경 개 노드 → 0.5초 안에 불가능

**해결책 → Alpha-Beta Pruning**

---

## 5. Alpha-Beta Pruning

### 핵심 아이디어

"어차피 선택되지 않을 가지는 탐색하지 않는다."

### Alpha와 Beta의 의미

- **Alpha (α)**: 내가 지금까지 찾은 "최소한 이 정도는 보장된다" 값 (MAX가 업데이트)
- **Beta (β)**: 상대가 지금까지 찾은 "최대한 이 정도로 막을 수 있다" 값 (MIN이 업데이트)

### 가지치기 원리

```
              MAX (α=-∞, β=+∞)
             /              \
         MIN(A)            MIN(B) ← 가지치기 가능!
        /   |   \          /   \
       3    12   8        2    [X] ← 탐색 안 함

1. A 완료 → MAX의 α = 3
2. B의 첫 자식 = 2 < α(3)
   → MAX는 이미 3점 보장
   → B의 나머지 탐색 불필요! ✂️
```

### 의사 코드

```python
def alphabeta(board, depth, alpha, beta, is_maximizing):
    if depth == 0 or game_over(board):
        return heuristic(board)

    if is_maximizing:
        value = -infinity
        for move in get_all_moves(board):
            board.make_move(move)
            value = max(value, alphabeta(board, depth-1, alpha, beta, False))
            board.undo_move(move)
            alpha = max(alpha, value)
            if alpha >= beta:
                break  # β 컷오프
        return value
    else:
        value = +infinity
        for move in get_all_moves(board):
            board.make_move(move)
            value = min(value, alphabeta(board, depth-1, alpha, beta, True))
            board.undo_move(move)
            beta = min(beta, value)
            if beta <= alpha:
                break  # α 컷오프
        return value
```

### 성능 향상

- 최선의 경우: O(b^d) → O(b^(d/2))
- 50^10 = 9경 개 → 50^5 = 3억 개 (3천만 배 감소!)

**핵심**: Move Ordering이 좋을수록 가지치기 효율 증가!

---

## 6. Iterative Deepening

### 핵심 아이디어

"깊이 1부터 시작해서 점점 깊게 탐색. 시간이 다 되면 그때까지의 최선의 수를 반환."

```
시간 제한: 0.5초

깊이 1 탐색 → 0.001초 완료 → Best: (9,9)
깊이 2 탐색 → 0.005초 완료 → Best: (9,10)
...
깊이 10 탐색 → 0.35초 완료 → Best: (10,10)
깊이 11 탐색 → 0.48초 (진행 중)
⏰ 시간 초과! → 깊이 10의 결과 반환
```

### 왜 필요한가?

- 각 깊이마다 완전한 결과 보장
- 시간 내 도달한 최대 깊이의 결과 사용
- 안전하게 시간 제한 준수

### 중복 탐색은 낭비 아닌가?

트리 구조에서 대부분의 노드는 마지막 깊이에 존재:
- 깊이 1~9까지의 총합 < 깊이 10의 2%
- 중복 탐색 오버헤드는 미미함

### 추가 이점

깊이 N에서 찾은 Best Move를 깊이 N+1에서 가장 먼저 탐색
→ Alpha-Beta 가지치기 효율 극대화

### 의사 코드

```python
def iterative_deepening(board, time_limit=0.5):
    start_time = time.time()
    best_move = None

    for depth in range(1, MAX_DEPTH + 1):
        if time.time() - start_time > time_limit * 0.9:
            break

        move, score = alphabeta_root(board, depth)
        best_move = move

        if score >= WIN_SCORE:
            break

    return best_move
```

---

## 7. 휴리스틱 함수

### 휴리스틱이란?

"게임이 끝나지 않은 보드 상태가 얼마나 유리한지 점수로 평가"

- AI의 "두뇌" 역할
- 정확성 + 속도 균형 필요

### 패턴 기반 점수 테이블

| 패턴 이름 | 모양 | 점수 |
|-----------|------|------|
| FIVE | ●●●●● | +1,000,000 (승리) |
| OPEN_FOUR | _●●●●_ | +100,000 |
| FOUR (Half-open) | ●●●●_ | +10,000 |
| OPEN_THREE | _●●●_ | +5,000 |
| THREE (Half-open) | ●●●__ | +500 |
| OPEN_TWO | _●●_ | +100 |
| TWO | ●●___ | +10 |

범례: ● = 내 돌, _ = 빈칸

### 4방향 스캔

모든 돌에 대해 가로, 세로, 대각선(↘, ↗) 4방향 패턴 체크

### 공격 vs 방어 균형

```python
def heuristic(board, my_color):
    my_score = evaluate_patterns(board, my_color)
    opp_score = evaluate_patterns(board, opponent(my_color))
    return my_score - (opp_score * 1.1)  # 방어 약간 중요
```

### 캡처 점수 반영

| 상황 | 점수 |
|------|------|
| 캡처 가능한 수 | +3,000 |
| 캡처된 돌 1쌍당 | +5,000 |
| 8개 캡처 (4쌍) | +50,000 |
| 10개 캡처 (승리) | +1,000,000 |
| 캡처 위협 당함 | -2,000 |

---

## 8. Bitboard 구현

### Bitboard란?

"보드 상태를 비트(0/1)로 표현하여 비트 연산으로 빠르게 처리"

- 19x19 = 361칸
- black_stones = 361비트 정수
- white_stones = 361비트 정수

### 보드 좌표 → 비트 위치

```python
def pos_to_bit(row, col):
    return row * 19 + col  # 0 ~ 360
```

### 돌 놓기/제거

```python
def place_stone(self, row, col, is_black):
    bit = 1 << (row * 19 + col)
    if is_black:
        self.black |= bit
    else:
        self.white |= bit

def remove_stone(self, row, col, is_black):
    bit = 1 << (row * 19 + col)
    if is_black:
        self.black &= ~bit
    else:
        self.white &= ~bit
```

### 비트 연산 기초

| 연산자 | 의미 | 예시 |
|--------|------|------|
| & | AND | 1010 & 1100 = 1000 |
| \| | OR | 1010 \| 1100 = 1110 |
| ^ | XOR | 1010 ^ 1100 = 0110 |
| ~ | NOT | ~1010 = 0101 |
| << | 왼쪽 시프트 | 0001 << 2 = 0100 |
| >> | 오른쪽 시프트 | 0100 >> 2 = 0001 |

### 방향별 시프트 값

| 방향 | 시프트 값 |
|------|-----------|
| 가로 → | 1 |
| 세로 ↓ | 19 |
| 대각선 ↘ | 20 |
| 대각선 ↗ | 18 |

### 5연속 체크

```python
def check_five_in_row(bitboard, direction):
    shift = DIRECTIONS[direction]
    b = bitboard
    b = b & (b >> shift)  # 2연속
    b = b & (b >> shift)  # 3연속
    b = b & (b >> shift)  # 4연속
    b = b & (b >> shift)  # 5연속
    return b != 0
```

### 성능 비교

- 2D 배열: ~50μs
- Bitboard: ~0.1μs
- **약 500배 빠름!**

---

## 9. Move Ordering 전략

### 중요성

Alpha-Beta의 효율은 탐색 순서에 극도로 의존
- 좋은 순서: O(b^(d/2))
- 나쁜 순서: O(b^d)

### 복합 전략 (탐색 우선순위)

| 순위 | 종류 | 이유 |
|------|------|------|
| 1 | 이전 깊이의 Best Move | Iterative Deepening 활용 |
| 2 | 승리 수 | 5연속 완성 |
| 3 | 상대 승리 차단 | 상대 5연속 막기 |
| 4 | 캡처 가능한 수 | 상대 돌 제거 |
| 5 | 위협 수 | Open-4, Four 만들기 |
| 6 | Killer Moves | 같은 깊이에서 컷오프 유발 |
| 7 | History Heuristic | 과거에 좋았던 수 |
| 8 | 인접 칸 | 기존 돌 주변만 |

### 인접 칸 필터링

361칸 → 약 20~50칸으로 후보 축소 (기존 돌 주변 2칸)

### Killer Move Heuristic

"같은 깊이에서 컷오프를 일으킨 수는 다른 노드에서도 좋을 가능성 높음"

```python
killer_moves[depth] = [(7,8), (9,10)]  # 최근 2개 저장
```

### History Heuristic

"과거 탐색에서 좋은 결과를 낸 수는 점수 누적"

```python
history[move] += depth²
```

---

## 10. 게임 규칙 구현

### 승리 조건

1. **5개 이상 연속 정렬** (가로/세로/대각선)
2. **10개 캡처** (5쌍)

### 캡처 규칙

```
패턴: ● ○ ○ ● (내돌-상대-상대-내돌)
→ 상대 돌 2개 제거
```

- 정확히 2개만 캡처 가능
- 8방향 모두 체크
- 자살 수 허용 (캡처당하는 위치로 이동 가능)

### Free-Three (열린 삼)

"막지 않으면 막을 수 없는 4연속이 되는 3개 정렬"

- 패턴 1: `_ ● ● ● _` (연속형)
- 패턴 2: `_ ● ● _ ● _` (띄엄형)

### Double-Three (쌍삼) 금지

한 수로 2개의 Free-Three 생성 금지

**예외**: 캡처로 인한 쌍삼은 허용

### Endgame Capture

- 5연속을 만들어도 상대가 캡처로 끊을 수 있으면 게임 계속
- 4쌍(8개) 잃은 상태에서 상대가 1쌍 더 캡처 가능하면 상대 승리

---

## 11. UI 구조

### 화면 레이아웃

```
┌─────────────────────────────────────────────────────────────┐
│  GOMOKU                                    [New] [Mode] [?] │
├─────────────────────────────────────┬───────────────────────┤
│                                     │  PLAYER INFO          │
│                                     │  ● Black (Human)      │
│         19 x 19 BOARD               │  ○ White (AI)         │
│                                     │  AI TIMER: ⏱ 0.000s   │
│                                     │  [Suggest] [Undo]     │
├─────────────────────────────────────┴───────────────────────┤
│  Status: Black's turn.                                      │
└─────────────────────────────────────────────────────────────┘
```

### 핵심 상수

```python
WINDOW_WIDTH = 1000
WINDOW_HEIGHT = 720
BOARD_SIZE = 660
CELL_SIZE = 33
```

### AI 타이머 (필수!)

```python
def render_ai_timer(self, screen, panel_x):
    if self.state.ai_thinking:
        elapsed = time.time() - self.state.ai_start_time
        timer_text = f"⏱ {elapsed:.3f}s"
    else:
        timer_text = f"⏱ {self.state.last_ai_time:.3f}s"
```

---

## 12. 디버그 패널

### D키로 토글

```
┌─ AI Debug Panel ─────────────────────────────┐
│  ⏱ Thinking Time:  0.342s                   │
│  📊 Search Depth:   12                       │
│  🔢 Nodes Evaluated: 1,247,832              │
│  🎯 Best Move: (9, 10)  Score: +4,521       │
│  📋 Principal Variation:                     │
│     (9,10) → (8,9) → (10,11) → ...          │
│  🏆 Top 5 Candidates:                       │
│     1. (9,10)  +4,521                       │
│     2. (8,8)   +3,892                       │
│     ...                                      │
└──────────────────────────────────────────────┘
```

### 방어 세션 활용

- "왜 이 수를 뒀나요?" → Top Candidates
- "얼마나 깊이 탐색했나요?" → Search Depth
- "앞으로 어떻게 될 거라 예상하나요?" → Principal Variation
- "가지치기가 효과적인가요?" → Pruning Stats

---

## 13. 파일 구조 및 구현 순서

### 파일 구조

```
Gomoku/
├── main.py
├── Makefile
├── requirements.txt
├── src/
│   ├── game/
│   │   ├── board.py      # Bitboard
│   │   ├── rules.py      # 캡처, 쌍삼, 승리
│   │   └── state.py      # 게임 상태
│   ├── ai/
│   │   ├── engine.py     # Alpha-Beta + ID
│   │   ├── heuristic.py  # 패턴 평가
│   │   ├── movegen.py    # Move Ordering
│   │   └── patterns.py   # 패턴 정의
│   └── ui/
│       ├── renderer.py   # 렌더링
│       ├── input.py      # 입력
│       ├── panel.py      # 정보 패널
│       └── debug.py      # 디버그 패널
├── tests/
└── docs/plans/
```

### 구현 순서

| Phase | 내용 | 목표 |
|-------|------|------|
| 1 | 기반 구조 | Human vs Human 플레이 가능 |
| 2 | 규칙 완성 | 모든 규칙 정확히 동작 |
| 3 | AI 기초 | AI가 수를 둠 |
| 4 | 성능 최적화 | 0.5초 내 10레벨 |
| 5 | UI 완성 | 타이머, 디버그 패널 |
| 6 | 테스트 & 튜닝 | 안정적인 완성품 |

---

## 부록: 용어 정리

| 용어 | 설명 |
|------|------|
| Min-Max | 상대도 최선을 둔다고 가정하는 게임 트리 탐색 알고리즘 |
| Alpha-Beta | 불필요한 노드를 가지치기하는 Min-Max 최적화 |
| Iterative Deepening | 깊이를 점진적으로 증가시키는 탐색 기법 |
| Heuristic | 보드 상태의 유불리를 점수로 평가하는 함수 |
| Bitboard | 보드를 비트로 표현하여 빠른 연산을 가능케 하는 자료구조 |
| Move Ordering | 좋은 수를 먼저 탐색하여 가지치기 효율을 높이는 기법 |
| Killer Move | 같은 깊이에서 컷오프를 유발한 수 |
| History Heuristic | 과거에 좋았던 수에 가산점을 주는 기법 |
| Principal Variation | AI가 예상하는 최선의 수순 |
| Free-Three | 막지 않으면 열린 4가 되는 열린 3 |
| Double-Three | 한 수로 2개의 Free-Three를 만드는 금지된 수 |

---

*이 문서는 방어 세션에서 AI 구현을 설명하는 데 활용할 수 있습니다.*
