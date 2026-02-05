# Gomoku AI Engine - Architecture Decisions

> Rust 기반 Ninuki-renju 오목 AI 엔진의 핵심 설계 결정 문서

## 목차
1. [언어 선택](#1-언어-선택)
2. [보드 표현](#2-보드-표현)
3. [검색 우선순위](#3-검색-우선순위)
4. [패턴 점수 체계](#4-패턴-점수-체계)
5. [휴리스틱 평가](#5-휴리스틱-평가)
6. [VCF/VCT 위협 검색](#6-vcfvct-위협-검색)
7. [Transposition Table](#7-transposition-table)
8. [Zobrist 해싱](#8-zobrist-해싱)

---

## 1. 언어 선택

### Decision
Python → Rust로 AI 엔진 재구현

### Rationale
- Python: ~255-466 NPS (Nodes Per Second)
- C++ 참조: ~10M NPS
- Rust: C++ 수준 성능 + 메모리 안전성
- 0.5초 내 depth 10+ 검색 요구사항 충족

### Trade-offs
- (+) 30x~50x 성능 향상
- (+) 안전한 메모리 관리 (no segfaults)
- (-) 개발 시간 증가
- (-) Python GUI와 FFI 필요 (PyO3)

---

## 2. 보드 표현

### Decision
6 x u64 배열 Bitboard (총 384비트, 361셀 사용)

```rust
pub struct Bitboard {
    bits: [u64; 6],  // 6 * 64 = 384 >= 361
}
```

### Rationale
- O(1) 돌 배치/제거/확인
- 비트 연산으로 빠른 패턴 검출
- 캐시 친화적 메모리 레이아웃 (48바이트)

### Trade-offs
- (+) 빠른 연산 (비트 AND/OR/XOR)
- (+) 효율적인 복사 (Clone 저렴)
- (-) 디버깅 시 가독성 낮음
- (-) 위치 계산 복잡도 (word_idx = pos / 64, bit_idx = pos % 64)

---

## 3. 검색 우선순위

### Decision
5단계 우선순위 검색 파이프라인

```
1. Immediate Win (즉시 승리)
2. VCF - Victory by Continuous Fours
3. VCT - Victory by Continuous Threats
4. Defense (상대 위협 방어)
5. Alpha-Beta Search (일반 탐색)
```

### Rationale
- 강제 승리(VCF/VCT) 우선 → 최적 플레이
- 방어 단계 분리 → 치명적 위협 대응 보장
- Alpha-Beta는 최후 수단 (시간 소모 큼)

### Trade-offs
- (+) 강제 승리 놓치지 않음
- (+) 빠른 승리 경로 발견
- (-) VCT가 sparse board에서 느림 → 별도 최적화 필요
- (-) 검색 단계 복잡도 증가

---

## 4. 패턴 점수 체계

### Decision
계층적 상수 기반 점수 체계

```rust
pub const FIVE: i32 = 1_000_000;
pub const OPEN_FOUR: i32 = 100_000;
pub const CLOSED_FOUR: i32 = 50_000;
pub const OPEN_THREE: i32 = 10_000;
pub const CLOSED_THREE: i32 = 1_000;
pub const OPEN_TWO: i32 = 500;
pub const CLOSED_TWO: i32 = 50;
```

### Rationale
- 10배 간격 → 상위 패턴이 하위 패턴 조합보다 항상 우선
- FIVE = CAPTURE_WIN = 1,000,000 → 두 승리 조건 동등
- 명시적 상수 → 매직 넘버 제거, 튜닝 용이

### Trade-offs
- (+) 패턴 우선순위 명확
- (+) 버그 추적 용이 (점수로 패턴 역추적)
- (-) 미세 조정 어려움 (10배 간격 고정)
- (-) 복합 패턴 평가 제한적

---

## 5. 휴리스틱 평가

### Decision A: 방어 가중치 1.5배

```rust
pub const DEFENSE_MULTIPLIER: f32 = 1.5;
```

### Rationale
- 상대 위협 과소평가 방지
- 공격보다 방어 우선 (잃지 않는 것이 먼저)

### Trade-offs
- (+) 안정적인 방어 플레이
- (-) 공격적 기회 놓칠 수 있음

---

### Decision B: 라인 시작점 평가

```rust
fn evaluate_line(board, pos, dr, dc, color) -> i32 {
    // 음수 방향에 같은 색 돌이 있으면 스킵 (시작점 아님)
    let prev_r = pos.row - dr;
    let prev_c = pos.col - dc;
    if board.get(prev_pos) == color {
        return 0;  // 중복 카운팅 방지
    }
    // 양수 방향만 카운트...
}
```

### Rationale
- 3칸 연속 패턴이 3번 카운트되는 버그 수정
- 각 라인 세그먼트를 정확히 1번만 평가

### Trade-offs
- (+) 정확한 패턴 점수
- (+) 간단한 로직 (분할 계산 불필요)
- (-) 모든 돌에서 방향 체크 필요

---

## 6. VCF/VCT 위협 검색

### Decision A: 깊이 제한

```rust
max_vcf_depth: 30  // VCF: 연속 4 위협만
max_vct_depth: 20  // VCT: 모든 위협 (4 + 열린 3)
```

### Rationale
- VCF는 강제성 높음 → 깊은 탐색 가능
- VCT는 분기 폭 넓음 → 상대적으로 얕게

### Trade-offs
- (+) 대부분의 강제 승리 발견
- (-) 극히 드문 장거리 VCT 놓칠 수 있음

---

### Decision B: Sparse Board 스킵

```rust
// stone_count < 8 이면 VCT 스킵
if board.stone_count() >= 8 {
    let vct_result = self.threat_searcher.search_vct(board, color);
    // ...
}
```

### Rationale
- 돌이 적은 보드에서 의미 있는 VCT 불가능
- VCT 검색은 exponential 비용
- 8개 미만에서 VCT 실행 시 수백 초 소요

### Trade-offs
- (+) 초반 응답 시간 대폭 개선
- (-) 이론적으로 매우 드문 조기 VCT 놓칠 수 있음
- → 실전에서 영향 없음 (8개 미만으로 VCT 성립 불가)

---

## 7. Transposition Table

### Decision
깊이 기반 교체 정책

```rust
let should_replace = match &self.entries[idx] {
    None => true,
    Some(e) => e.hash == hash || e.depth <= depth,
};
```

### Rationale
- 같은 위치: 항상 최신 정보로 갱신
- 다른 위치 충돌: 더 깊은 검색 결과 보존
- 얕은 결과는 재계산 비용 낮음

### Trade-offs
- (+) 깊은 검색 결과 보존
- (+) 간단한 구현
- (-) 최근 접근 정보 미반영 (LRU 미사용)

---

## 8. Zobrist 해싱

### Decision
결정론적 LCG 기반 해시 생성

```rust
fn lcg(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
    *seed
}
```

### Rationale
- 고정 시드 → 세션 간 일관된 해시
- LCG는 빠르고 충분한 분산 제공
- XOR 기반 점진적 갱신 O(1)

### Trade-offs
- (+) 재현 가능한 결과 (디버깅 용이)
- (+) 배치/제거 모두 XOR로 처리 (역연산)
- (-) 암호학적 안전성 없음 (불필요)

---

## 디렉토리 구조

```
engine/
├── src/
│   ├── lib.rs           # 라이브러리 진입점
│   ├── main.rs          # CLI 테스트
│   ├── engine.rs        # AI 엔진 통합
│   ├── board/
│   │   ├── bitboard.rs  # 6 x u64 비트보드
│   │   ├── board.rs     # Board 구조체
│   │   └── pos.rs       # 위치 타입
│   ├── rules/
│   │   ├── capture.rs   # X-O-O-X 캡처
│   │   ├── win.rs       # 승리 조건
│   │   └── forbidden.rs # 쌍삼 금지
│   ├── eval/
│   │   ├── patterns.rs  # 점수 상수
│   │   └── heuristic.rs # 평가 함수
│   └── search/
│       ├── alphabeta.rs # Alpha-Beta + ID
│       ├── threat.rs    # VCF/VCT
│       ├── tt.rs        # Transposition Table
│       └── zobrist.rs   # Zobrist 해싱
└── Cargo.toml
```

---

## 테스트

```bash
# 전체 테스트 (debug)
cargo test --lib

# Release 테스트 (빠름)
cargo test --lib --release

# 특정 모듈
cargo test --lib alphabeta
cargo test --lib threat
```

**테스트 현황**: 160 unit tests + 11 doc tests
