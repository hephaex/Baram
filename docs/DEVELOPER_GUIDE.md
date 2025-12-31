# Baram Developer Guide

이 가이드는 Baram 프로젝트에 기여하거나 확장하려는 개발자를 위한 문서입니다.

## 개발 환경 설정

### 요구사항

- Rust 1.75+
- SQLite 3.x
- (선택) Ollama 또는 vLLM (온톨로지 추출용)

### 빌드

```bash
# 개발 빌드
cargo build

# 릴리즈 빌드
cargo build --release

# 테스트 실행
cargo test

# 린트
cargo clippy
```

## 새 파서 추가

네이버 뉴스 외 다른 소스를 크롤링하려면 파서를 추가합니다.

### 1. 셀렉터 정의

`src/parser/selectors.rs`에 새 셀렉터를 추가:

```rust
// lazy_static으로 컴파일 타임 초기화
lazy_static! {
    pub static ref MY_SOURCE_SELECTORS: MySourceSelectors = MySourceSelectors::new();
}

pub struct MySourceSelectors {
    pub title: Selector,
    pub content: Selector,
    pub author: Selector,
    pub date: Selector,
}

impl MySourceSelectors {
    fn new() -> Self {
        Self {
            title: Selector::parse("h1.article-title").unwrap(),
            content: Selector::parse("div.article-body").unwrap(),
            author: Selector::parse("span.author-name").unwrap(),
            date: Selector::parse("time.published").unwrap(),
        }
    }
}
```

### 2. 파서 구현

`src/parser/`에 새 파서 파일 생성:

```rust
// src/parser/my_source.rs
use super::selectors::MY_SOURCE_SELECTORS;
use crate::crawler::ParsedArticle;
use scraper::Html;

pub fn parse_article(html: &str) -> Result<ParsedArticle, ParserError> {
    let document = Html::parse_document(html);

    let title = document
        .select(&MY_SOURCE_SELECTORS.title)
        .next()
        .map(|el| el.text().collect::<String>())
        .ok_or(ParserError::MissingTitle)?;

    let content = document
        .select(&MY_SOURCE_SELECTORS.content)
        .next()
        .map(|el| el.text().collect::<String>())
        .ok_or(ParserError::MissingContent)?;

    Ok(ParsedArticle {
        title,
        content,
        ..Default::default()
    })
}
```

### 3. 모듈 등록

`src/parser/mod.rs`에 모듈 추가:

```rust
mod my_source;
pub use my_source::parse_article as parse_my_source;
```

## 새 온톨로지 규칙 추가

### 1. 관계 타입 추가

`src/ontology/extractor.rs`의 프롬프트 템플릿 수정:

```rust
const EXTRACTION_PROMPT: &str = r#"
다음 관계 타입 중 하나를 선택하세요:
- 발언 (schema:author): 인물이 말한 내용
- 소속 (schema:memberOf): 인물의 소속 기관
- 직책 (schema:jobTitle): 인물의 직위
- 새로운관계 (schema:newRelation): 새로운 관계 설명  <- 추가
"#;
```

### 2. 검증 규칙 추가

`src/ontology/linker.rs`에 검증 로직 추가:

```rust
impl EntityLinker {
    fn validate_relation(&self, triple: &Triple) -> bool {
        match triple.predicate.as_str() {
            "schema:author" => self.validate_said_relation(triple),
            "schema:memberOf" => self.validate_membership(triple),
            "schema:newRelation" => self.validate_new_relation(triple), // 추가
            _ => true,
        }
    }

    fn validate_new_relation(&self, triple: &Triple) -> bool {
        // 새 관계 검증 로직
        !triple.object.is_empty() && triple.confidence > 0.7
    }
}
```

### 3. 통계 추가

`src/ontology/stats.rs`에 통계 항목 추가:

```rust
pub struct OntologyStats {
    pub entity_types: HashMap<String, usize>,
    pub relation_types: HashMap<String, usize>,
    // 새 통계 필드 추가
    pub new_relation_count: usize,
}
```

## 새 검색 필터 추가

### 1. 필터 타입 정의

`src/commands/search.rs`에 필터 추가:

```rust
#[derive(Debug, Clone)]
pub struct SearchFilters {
    pub category: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub publisher: Option<String>,
    // 새 필터 추가
    pub author: Option<String>,
}
```

### 2. SQL 쿼리 수정

`src/storage/mod.rs`의 검색 쿼리 수정:

```rust
pub fn search_articles(&self, filters: &SearchFilters) -> Result<Vec<Article>> {
    let mut query = String::from("SELECT * FROM articles WHERE 1=1");
    let mut params: Vec<Box<dyn ToSql>> = vec![];

    if let Some(author) = &filters.author {
        query.push_str(" AND author = ?");
        params.push(Box::new(author.clone()));
    }

    // 기존 필터 로직...
}
```

### 3. CLI 옵션 추가

`src/main.rs`의 CLI 정의 수정:

```rust
#[derive(Parser)]
struct SearchArgs {
    #[arg(short, long)]
    query: String,

    #[arg(long)]
    author: Option<String>,  // 새 옵션 추가
}
```

## 테스트 작성 가이드

### 단위 테스트

각 모듈 파일 하단에 테스트 모듈 추가:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_article() {
        let html = r#"<html><h1>Test Title</h1></html>"#;
        let result = parse_article(html);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().title, "Test Title");
    }

    #[test]
    fn test_parse_invalid_html() {
        let html = "";
        let result = parse_article(html);
        assert!(result.is_err());
    }
}
```

### 통합 테스트

`tests/integration_tests/`에 테스트 파일 추가:

```rust
// tests/integration_tests/my_feature_test.rs
use baram::parser::parse_my_source;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_full_pipeline() {
    // 테스트 환경 설정
    let config = TestConfig::new();

    // 테스트 실행
    let result = run_pipeline(&config).await;

    // 검증
    assert!(result.is_ok());
}
```

### 테스트 픽스처

`tests/integration_tests/fixtures.rs`에 공통 픽스처 추가:

```rust
pub fn sample_article_html() -> &'static str {
    include_str!("fixtures/sample_article.html")
}

pub fn sample_config() -> Config {
    Config {
        crawler: CrawlerConfig {
            requests_per_second: 1,
            ..Default::default()
        },
        ..Default::default()
    }
}
```

## 에러 처리 패턴

### 새 에러 타입 추가

`src/utils/error.rs`에 에러 타입 추가:

```rust
#[derive(Debug, thiserror::Error)]
pub enum MyModuleError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Processing failed: {reason}")]
    ProcessingFailed { reason: String },

    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

pub type MyModuleResult<T> = Result<T, MyModuleError>;
```

### 에러 전파

```rust
use crate::utils::MyModuleError;

fn process_data(input: &str) -> MyModuleResult<Output> {
    if input.is_empty() {
        return Err(MyModuleError::InvalidInput("empty input".into()));
    }

    let result = do_something(input)
        .map_err(|e| MyModuleError::ProcessingFailed {
            reason: e.to_string()
        })?;

    Ok(result)
}
```

## 비동기 패턴

### 재시도 로직 사용

```rust
use crate::utils::retry::{with_retry, RetryConfig};

async fn fetch_with_retry(url: &str) -> Result<String> {
    let config = RetryConfig {
        max_retries: 3,
        initial_delay_ms: 1000,
        max_delay_ms: 30000,
        backoff_multiplier: 2.0,
    };

    with_retry(&config, || async {
        reqwest::get(url).await?.text().await
    }).await
}
```

### 병렬 처리

```rust
use futures::stream::{self, StreamExt};

async fn process_articles(articles: Vec<Article>) -> Vec<Result<Output>> {
    stream::iter(articles)
        .map(|article| async move {
            process_single(article).await
        })
        .buffer_unordered(10)  // 동시 10개 처리
        .collect()
        .await
}
```

## 코드 스타일

### 네이밍 규칙

- 함수: `snake_case` (예: `parse_article`)
- 구조체: `PascalCase` (예: `ParsedArticle`)
- 상수: `SCREAMING_SNAKE_CASE` (예: `MAX_RETRIES`)
- 모듈: `snake_case` (예: `ontology_extractor`)

### 문서화

```rust
/// 기사를 파싱하여 구조화된 데이터로 변환합니다.
///
/// # Arguments
///
/// * `html` - 파싱할 HTML 문자열
///
/// # Returns
///
/// 파싱된 기사 또는 에러
///
/// # Examples
///
/// ```
/// let article = parse_article("<html>...</html>")?;
/// println!("{}", article.title);
/// ```
pub fn parse_article(html: &str) -> Result<ParsedArticle> {
    // ...
}
```

## 디버깅

### 로깅

```rust
use tracing::{info, debug, warn, error};

fn process_article(article: &Article) {
    info!(article_id = %article.id, "Processing article");
    debug!(?article, "Article details");

    if let Err(e) = validate(article) {
        warn!(error = %e, "Validation warning");
    }
}
```

### 환경 변수

```bash
# 로그 레벨 설정
RUST_LOG=debug cargo run

# 특정 모듈만 디버그
RUST_LOG=baram::ontology=debug cargo run

# 백트레이스 활성화
RUST_BACKTRACE=1 cargo run
```

## 기여 가이드

1. 이슈 확인 또는 생성
2. 브랜치 생성: `feature/issue-N-description`
3. 코드 작성 및 테스트
4. PR 생성 (제목 형식: `feat:`, `fix:`, `docs:` 등)
5. 리뷰 및 머지

## 참고 자료

- [ARCHITECTURE.md](../ARCHITECTURE.md) - 시스템 아키텍처
- [OPERATIONS.md](./OPERATIONS.md) - 운영 가이드
- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
