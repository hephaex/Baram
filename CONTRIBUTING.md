# Contributing to Baram

Baram 프로젝트에 기여해 주셔서 감사합니다! 이 문서는 프로젝트에 기여하는 방법을 안내합니다.

## 목차

- [행동 강령](#행동-강령)
- [개발 환경 설정](#개발-환경-설정)
- [기여 방법](#기여-방법)
- [코드 스타일](#코드-스타일)
- [커밋 메시지 규칙](#커밋-메시지-규칙)
- [Pull Request 프로세스](#pull-request-프로세스)
- [테스트 가이드](#테스트-가이드)
- [문서화](#문서화)

## 행동 강령

이 프로젝트는 모든 참여자가 환영받는 환경을 만들기 위해 노력합니다. 다음을 준수해 주세요:

- 존중하는 언어 사용
- 다양한 관점과 경험 존중
- 건설적인 비판 수용
- 커뮤니티에 최선인 것에 집중

## 개발 환경 설정

### 필수 요구사항

- **Rust**: 1.80 이상
- **Docker & Docker Compose**: 서비스 실행용
- **Git**: 버전 관리

### 설치 단계

```bash
# 1. 저장소 클론
git clone https://github.com/hephaex/baram.git
cd baram

# 2. Rust 도구 설치
rustup update stable
rustup component add clippy rustfmt

# 3. 개발 도구 설치 (선택)
cargo install cargo-watch cargo-audit

# 4. 환경 설정
make setup

# 5. Docker 서비스 시작
make start

# 6. 빌드 확인
cargo build
```

### 개발 서비스 실행

```bash
# 핵심 서비스 (PostgreSQL, OpenSearch, Redis)
make start

# 개발 도구 (pgAdmin, OpenSearch Dashboards)
make dev-tools

# 모든 서비스 상태 확인
make status
```

## 기여 방법

### 이슈 보고

버그를 발견했거나 기능을 제안하고 싶다면:

1. [기존 이슈](https://github.com/hephaex/baram/issues) 검색
2. 없다면 새 이슈 생성
3. 템플릿에 따라 상세히 작성

### 버그 리포트 템플릿

```markdown
## 버그 설명
[버그에 대한 간단한 설명]

## 재현 단계
1. '...' 실행
2. '...' 클릭
3. 에러 발생

## 예상 동작
[예상했던 동작]

## 실제 동작
[실제로 발생한 동작]

## 환경
- OS: [예: Ubuntu 22.04]
- Rust 버전: [예: 1.80.0]
- Baram 버전: [예: 0.1.6]
```

### 기능 제안

```markdown
## 기능 설명
[제안하는 기능에 대한 설명]

## 동기
[왜 이 기능이 필요한지]

## 제안하는 구현 방법
[가능하다면 구현 방법 제안]
```

## 코드 스타일

### Rust 코드 규칙

프로젝트는 표준 Rust 스타일 가이드를 따릅니다:

```bash
# 코드 포맷팅 (필수)
cargo fmt

# 린트 검사 (필수)
cargo clippy --all-targets --all-features -- -D warnings
```

### 주요 규칙

1. **에러 처리**: `unwrap()` 대신 `?` 연산자 또는 `expect("설명")` 사용
2. **문서화**: 공개 API에는 문서 주석(`///`) 필수
3. **테스트**: 새 기능에는 테스트 필수
4. **unsafe**: 반드시 `// SAFETY:` 주석과 함께 사용

```rust
// 좋은 예
/// 기사를 저장합니다.
///
/// # Arguments
/// * `article` - 저장할 기사
///
/// # Errors
/// 데이터베이스 연결 실패 시 에러 반환
pub async fn store_article(&self, article: &Article) -> Result<()> {
    // ...
}

// 나쁜 예
pub async fn store_article(&self, article: &Article) -> Result<()> {
    self.db.insert(article).unwrap(); // unwrap 사용 금지
}
```

### 명명 규칙

| 항목 | 규칙 | 예시 |
|------|------|------|
| 타입/구조체 | PascalCase | `ArticleParser` |
| 함수/메서드 | snake_case | `parse_article` |
| 상수 | SCREAMING_SNAKE_CASE | `MAX_RETRIES` |
| 모듈 | snake_case | `crawler_pipeline` |

## 커밋 메시지 규칙

[Conventional Commits](https://www.conventionalcommits.org/) 형식을 따릅니다:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Type

| Type | 설명 |
|------|------|
| `feat` | 새로운 기능 |
| `fix` | 버그 수정 |
| `docs` | 문서 변경 |
| `style` | 코드 포맷팅 (기능 변경 없음) |
| `refactor` | 리팩토링 |
| `test` | 테스트 추가/수정 |
| `chore` | 빌드/도구 변경 |
| `perf` | 성능 개선 |

### 예시

```bash
feat(crawler): add parallel category crawling support

- Implement execute_categories_parallel() with semaphore
- Add configurable max_parallel_workers
- Integrate with DistributedRunner

Closes #123
```

```bash
fix(parser): handle Korean date format with AM/PM

Parse dates like "2024.12.25. 오후 3:45" correctly.
```

## Pull Request 프로세스

### 1. 브랜치 생성

```bash
# 기능 브랜치
git checkout -b feat/my-feature

# 버그 수정 브랜치
git checkout -b fix/bug-description
```

### 2. 변경 사항 작업

```bash
# 코드 작성 후
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

### 3. PR 생성

PR 제목은 커밋 메시지 규칙을 따릅니다:

```
feat(crawler): add parallel category crawling
```

### PR 체크리스트

- [ ] `cargo fmt` 실행됨
- [ ] `cargo clippy -- -D warnings` 통과
- [ ] `cargo test` 통과
- [ ] 문서 업데이트 (필요시)
- [ ] 테스트 추가 (새 기능인 경우)

### 4. 코드 리뷰

- 리뷰어의 피드백에 응답
- 필요한 변경 사항 반영
- CI 검사 통과 확인

## 테스트 가이드

### 테스트 실행

```bash
# 모든 테스트
cargo test

# 특정 모듈 테스트
cargo test crawler::

# 통합 테스트 (Docker 서비스 필요)
make test-integration

# 특정 테스트
cargo test test_parse_korean_datetime
```

### 테스트 작성 규칙

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name_describes_behavior() {
        // Given: 초기 상태
        let input = "test input";

        // When: 동작 실행
        let result = function_under_test(input);

        // Then: 결과 검증
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_async_function() {
        // 비동기 테스트
    }
}
```

### 테스트 커버리지

새 기능에는 다음 테스트가 필요합니다:

1. **단위 테스트**: 개별 함수 테스트
2. **통합 테스트**: 모듈 간 상호작용 테스트 (해당되는 경우)
3. **에러 케이스**: 실패 시나리오 테스트

## 문서화

### 코드 문서화

```rust
//! 모듈 수준 문서
//!
//! 이 모듈은 뉴스 기사를 파싱합니다.

/// 구조체/함수 문서
///
/// # Examples
///
/// ```rust
/// let parser = Parser::new();
/// let article = parser.parse(html)?;
/// ```
///
/// # Errors
///
/// HTML이 유효하지 않으면 에러를 반환합니다.
pub fn parse(&self, html: &str) -> Result<Article> {
    // ...
}
```

### 문서 빌드

```bash
# 문서 생성 및 열기
cargo doc --open

# 문서 검사
cargo doc --no-deps
```

## 질문이 있으신가요?

- [GitHub Discussions](https://github.com/hephaex/baram/discussions)에서 질문
- [이슈](https://github.com/hephaex/baram/issues)에서 버그 보고

기여해 주셔서 감사합니다! 🎉
