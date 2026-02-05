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

## Build

```bash
make            # Release 빌드 → ./Gomoku
make clean      # target/ 제거
make fclean     # clean + ./Gomoku 제거
make re         # fclean + all
```

## Test

```bash
make test           # Debug 테스트
make test-release   # Release 테스트 (빠름)

# 또는 직접
cargo test --lib
cargo test --lib --release
```

## Usage

```bash
./Gomoku    # CLI 테스트 실행
```

## Project Structure

```
Gomoku/
├── Cargo.toml          # 패키지 설정
├── Makefile            # 빌드 스크립트
├── src/
│   ├── lib.rs          # 라이브러리 진입점
│   ├── main.rs         # CLI 바이너리
│   ├── engine.rs       # AI 엔진 통합
│   ├── board/          # Bitboard 표현
│   ├── rules/          # 캡처, 승리, 금지
│   ├── eval/           # 휴리스틱 평가
│   └── search/         # Alpha-Beta, VCF/VCT, TT
└── docs/
    └── ARCHITECTURE.md # 설계 결정
```

## Documentation

- [Architecture Decisions](docs/ARCHITECTURE.md)

## License

42 School Project
