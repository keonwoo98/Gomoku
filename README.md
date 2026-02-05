# Gomoku AI Engine

Rust 기반 고성능 오목 AI 엔진 (Ninuki-renju 규칙)

## Features

- 19x19 보드, 6 x u64 Bitboard 표현
- VCF/VCT 위협 검색 (강제 승리 탐지)
- Alpha-Beta + Iterative Deepening + Transposition Table
- Ninuki-renju 캡처 규칙 (X-O-O-X 패턴)
- 쌍삼 금지 (캡처 예외)
- 160개 단위 테스트

## Requirements

- Rust 1.70+
- Cargo

## Build

```bash
make        # Release 빌드 → ./Gomoku 생성
make clean  # 빌드 아티팩트 제거
make re     # 클린 후 재빌드
```

## Usage

```bash
./Gomoku    # CLI 테스트 실행
```

## Test

```bash
cd engine
cargo test --lib           # Debug 테스트 (31초)
cargo test --lib --release # Release 테스트 (2.6초)
```

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for design decisions.

## License

42 School Project
