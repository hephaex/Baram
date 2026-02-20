---
name: build-test
description: cargo clippy, test, build --release를 순차 실행하여 코드 품질을 확인합니다.
disable-model-invocation: true
argument-hint: [--quick (clippy+test only) | --release (full build)]
allowed-tools: Bash
---

Baram 코드 빌드 및 테스트를 실행합니다.

Arguments: $ARGUMENTS

## 수행 단계

### 1. Clippy (린트)
```bash
cargo clippy 2>&1
```
경고나 에러가 있으면 보고.

### 2. 테스트
```bash
cargo test 2>&1
```
실패한 테스트가 있으면 상세 내용 보고.

### 3. 릴리스 빌드 (`--quick` 미지정 시)
`--quick` 인자가 없으면 릴리스 빌드도 수행:
```bash
cargo build --release 2>&1
```

## 출력 형식

한국어로 요약:
```
## 빌드 결과
- Clippy: [통과/경고 N개/에러 N개]
- Test: [N개 통과 / N개 실패]
- Release build: [성공/실패/스킵]
```
