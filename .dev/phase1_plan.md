# Phase 1: Core Crawler Implementation Plan

> **Project**: baram - Rust Naver News Crawler
> **Phase**: 1 - Core Crawler Implementation
> **Duration**: 5 days + 1 buffer day
> **Goal**: Build reliable HTML parsing and file storage pipeline
>
> **Copyright**: Copyright (c) 2024 hephaex@gmail.com
> **License**: GPL v3
> **Repository**: https://github.com/hephaex/baram

---

## Overview

Phase 1 establishes the foundation of the baram crawler system. By the end of this phase, the system will be able to:

1. Fetch news articles from Naver News with anti-bot protection
2. Parse article content with fallback strategies for different formats
3. Save articles as Markdown files
4. Track crawled articles in SQLite to prevent duplicates
5. Provide basic CLI interface for crawling operations

### Success Criteria (Definition of Done)

```
Successfully crawl 100 articles from politics/economy sections
with no encoding issues and save as valid UTF-8 Markdown files
```

---

## Dependency Graph

```
                      Day 1
                        │
            ┌───────────┼───────────┐
            │           │           │
            ▼           ▼           ▼
       [Cargo.toml] [config/]  [utils/error.rs]
            │           │           │
            └───────────┼───────────┘
                        │
                        ▼
                      Day 2
                        │
            ┌───────────┴───────────┐
            │                       │
            ▼                       ▼
    [crawler/fetcher.rs]    [encoding_rs]
            │                       │
            └───────────┬───────────┘
                        │
                        ▼
                      Day 3
                        │
            ┌───────────┴───────────┐
            │                       │
            ▼                       ▼
    [crawler/list.rs]       [URL collector]
            │                       │
            └───────────┬───────────┘
                        │
                        ▼
                      Day 4
                        │
            ┌───────────┼───────────┐
            │           │           │
            ▼           ▼           ▼
    [parser/html.rs] [Fallback] [Selectors]
            │           │           │
            └───────────┼───────────┘
                        │
                        ▼
                      Day 5
                        │
    ┌───────────────────┼───────────────────┐
    │                   │                   │
    ▼                   ▼                   ▼
[storage/markdown.rs] [storage/db.rs]  [CLI main.rs]
```

---

## Day 1: Project Setup, Config Module, Error Types

### Objective
Initialize Rust project structure and implement foundational modules.

### Files to Create

| File | Purpose | Dependencies |
|------|---------|--------------|
| `Cargo.toml` | Project dependencies | None |
| `src/lib.rs` | Library root | None |
| `src/main.rs` | CLI entry point (stub) | lib.rs |
| `src/config/mod.rs` | Config module exports | None |
| `src/config/settings.rs` | Configuration struct | toml, serde |
| `src/utils/mod.rs` | Utils module exports | None |
| `src/utils/error.rs` | Error types | thiserror |
| `src/models.rs` | Data structures | serde, chrono |
| `config.toml` | Default configuration | None |

### Implementation Details

#### 1.1 Cargo.toml

```toml
[package]
name = "baram"
version = "0.1.0"
edition = "2021"
authors = ["hephaex <hephaex@gmail.com>"]
description = "High-performance Naver News Crawler with Vector DB and Ontology"
license = "GPL-3.0"
repository = "https://github.com/hephaex/baram"

[dependencies]
# Async runtime
tokio = { version = "1.35", features = ["full"] }

# HTTP client
reqwest = { version = "0.11.23", features = ["json", "cookies", "gzip", "rustls-tls"] }

# HTML parsing
scraper = "0.18.1"

# Encoding (EUC-KR support)
encoding_rs = "0.8.33"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8.8"

# Date/time
chrono = { version = "0.4.31", features = ["serde"] }

# CLI
clap = { version = "4.4", features = ["derive", "env"] }

# Database
rusqlite = { version = "0.30.0", features = ["bundled"] }

# Rate limiting
governor = "0.6.3"

# Template engine
handlebars = "5.1"

# Logging
tracing = "0.1.40"
tracing-subscriber = { version = "0.3.18", features = ["env-filter"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Utilities
url = "2.5"
regex = "1.10"
once_cell = "1.19"
rand = "0.8"
sha2 = "0.10"

[dev-dependencies]
tokio-test = "0.4"
wiremock = "0.5"
tempfile = "3.9"
```

#### 1.2 src/config/settings.rs - Functions to Implement

```rust
// src/config/settings.rs

/// Load configuration from file with environment variable override
pub fn load_config(path: Option<&str>) -> Result<Config, ConfigError>

/// Configuration root structure
pub struct Config {
    pub crawler: CrawlerConfig,
    pub storage: StorageConfig,
    pub logging: LoggingConfig,
}

/// Crawler-specific settings
pub struct CrawlerConfig {
    pub requests_per_second: u32,  // Default: 2
    pub max_retries: u32,          // Default: 3
    pub timeout_secs: u64,         // Default: 30
    pub user_agents: Vec<String>,  // User-Agent pool
}

/// Storage settings
pub struct StorageConfig {
    pub output_dir: PathBuf,       // Default: ./output/raw
    pub checkpoint_dir: PathBuf,   // Default: ./output/checkpoints
    pub sqlite_path: PathBuf,      // Default: ./data/metadata.db
}

/// Logging configuration
pub struct LoggingConfig {
    pub level: String,             // Default: "info"
    pub format: String,            // Default: "pretty" | "json"
}

impl Default for Config { ... }
impl Config {
    /// Merge with environment variables (NTIMES_CRAWLER_RPS, etc.)
    pub fn with_env_override(self) -> Self { ... }
}
```

#### 1.3 src/utils/error.rs - Error Types

```rust
// src/utils/error.rs
use thiserror::Error;

/// Top-level application error
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Crawler error: {0}")]
    Crawler(#[from] CrawlerError),

    #[error("Parser error: {0}")]
    Parser(#[from] ParseError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("Invalid configuration: {0}")]
    Validation(String),
}

/// HTTP fetcher errors
#[derive(Error, Debug)]
pub enum FetchError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Rate limited by server")]
    RateLimit,

    #[error("Server error: {0}")]
    ServerError(u16),

    #[error("Request timeout")]
    Timeout,

    #[error("Max retries exceeded")]
    MaxRetriesExceeded,

    #[error("Decoding error: {0}")]
    Decode(String),
}

/// HTML parser errors
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Title not found in HTML")]
    TitleNotFound,

    #[error("Content not found in HTML")]
    ContentNotFound,

    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),

    #[error("Failed to extract oid/aid from URL")]
    IdExtractionFailed,

    #[error("Article has been deleted or is unavailable")]
    ArticleNotFound,

    #[error("Unknown article format")]
    UnknownFormat,
}

/// Storage errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Template error: {0}")]
    Template(#[from] handlebars::RenderError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

#### 1.4 src/models.rs - Core Data Structures

```rust
// src/models.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Parsed news article
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParsedArticle {
    pub oid: String,                      // Publisher ID (e.g., "001")
    pub aid: String,                      // Article ID (e.g., "0014123456")
    pub title: String,
    pub content: String,
    pub url: String,
    pub category: String,                 // politics, economy, society, etc.
    pub publisher: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub crawled_at: DateTime<Utc>,
    pub content_hash: Option<String>,     // SHA256 for deduplication
}

impl ParsedArticle {
    /// Generate unique ID: {oid}_{aid}
    pub fn id(&self) -> String {
        format!("{}_{}", self.oid, self.aid)
    }

    /// Calculate content hash
    pub fn compute_hash(&mut self) {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(self.content.as_bytes());
        self.content_hash = Some(format!("{:x}", hasher.finalize()));
    }
}

/// News category enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NewsCategory {
    Politics = 100,
    Economy = 101,
    Society = 102,
    Culture = 103,
    World = 104,
    IT = 105,
}

impl NewsCategory {
    pub fn from_section_id(id: u32) -> Option<Self> { ... }
    pub fn to_section_id(&self) -> u32 { ... }
    pub fn as_str(&self) -> &'static str { ... }
}

/// Crawl checkpoint for resume functionality
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrawlState {
    pub completed_articles: std::collections::HashSet<String>,
    pub last_category: Option<String>,
    pub last_page: u32,
    pub last_url: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl CrawlState {
    pub fn load(path: &std::path::Path) -> Self { ... }
    pub fn save(&self, path: &std::path::Path) -> Result<(), std::io::Error> { ... }
    pub fn is_completed(&self, article_id: &str) -> bool { ... }
    pub fn mark_completed(&mut self, article_id: &str) { ... }
}
```

### Test Cases for Day 1

| Test ID | Description | Function |
|---------|-------------|----------|
| TC-D1-001 | Load valid config.toml | `test_load_valid_config` |
| TC-D1-002 | Handle missing config file | `test_missing_config_fallback` |
| TC-D1-003 | Environment variable override | `test_env_override` |
| TC-D1-004 | ParsedArticle id generation | `test_article_id_format` |
| TC-D1-005 | Content hash calculation | `test_content_hash` |
| TC-D1-006 | CrawlState serialization roundtrip | `test_crawl_state_serde` |
| TC-D1-007 | NewsCategory conversion | `test_category_conversion` |

```rust
// tests/config_test.rs
#[test]
fn test_load_valid_config() {
    let config = load_config(Some("./config.toml")).unwrap();
    assert_eq!(config.crawler.requests_per_second, 2);
}

#[test]
fn test_env_override() {
    std::env::set_var("NTIMES_CRAWLER_RPS", "5");
    let config = Config::default().with_env_override();
    assert_eq!(config.crawler.requests_per_second, 5);
}

// tests/models_test.rs
#[test]
fn test_article_id_format() {
    let article = ParsedArticle {
        oid: "001".to_string(),
        aid: "0014123456".to_string(),
        ..Default::default()
    };
    assert_eq!(article.id(), "001_0014123456");
}
```

### Acceptance Criteria for Day 1

- [ ] `cargo build` succeeds without errors
- [ ] `cargo test` passes all Day 1 tests
- [ ] `cargo clippy` reports no warnings
- [ ] Config loads from `config.toml` successfully
- [ ] Error types compile with `thiserror`
- [ ] Models serialize/deserialize with `serde`

### Risk Areas - Day 1

| Risk | Mitigation |
|------|------------|
| Dependency version conflicts | Pin exact versions in Cargo.toml |
| Config structure changes | Use `#[serde(default)]` for backward compatibility |

---

## Day 2: HTTP Fetcher with Anti-Bot Headers, Retry Logic, EUC-KR Encoding

### Objective
Implement reliable HTTP client with rate limiting, retry logic, and Korean encoding support.

### Files to Create

| File | Purpose | Dependencies |
|------|---------|--------------|
| `src/crawler/mod.rs` | Crawler module exports | None |
| `src/crawler/fetcher.rs` | HTTP fetcher | reqwest, governor, encoding_rs |
| `src/crawler/headers.rs` | Anti-bot header management | reqwest |

### Implementation Details

#### 2.1 src/crawler/fetcher.rs - Core Functions

```rust
// src/crawler/fetcher.rs

/// User-Agent pool (update periodically)
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ...",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ...",
    // ... more agents
];

/// HTTP Fetcher with rate limiting and retry logic
pub struct NaverFetcher {
    client: reqwest::Client,
    rate_limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    max_retries: u32,
    base_url: Option<String>,  // For testing with mock server
}

impl NaverFetcher {
    /// Create new fetcher with rate limiting
    ///
    /// # Arguments
    /// * `requests_per_second` - Max requests per second (recommended: 2)
    pub fn new(requests_per_second: u32) -> Result<Self, FetchError>

    /// Create fetcher with custom config
    pub fn with_config(
        requests_per_second: u32,
        max_retries: u32,
        timeout: Duration,
    ) -> Result<Self, FetchError>

    /// Create fetcher with mock server base URL (for testing)
    pub fn with_base_url(base_url: &str, requests_per_second: u32) -> Result<Self, FetchError>

    /// Fetch article HTML with referer injection
    ///
    /// # Arguments
    /// * `url` - Full article URL
    /// * `section_id` - News section ID for referer (100=politics, 101=economy, etc.)
    ///
    /// # Returns
    /// * Decoded HTML string (UTF-8)
    pub async fn fetch_article(&self, url: &str, section_id: u32) -> Result<String, FetchError>

    /// Fetch with exponential backoff retry
    async fn fetch_with_retry(&self, url: &str, section_id: u32) -> Result<String, FetchError>

    /// Decode response bytes handling EUC-KR
    ///
    /// # Encoding Detection
    /// 1. Check Content-Type header for charset
    /// 2. Try UTF-8 first
    /// 3. Fallback to EUC-KR if UTF-8 fails
    async fn decode_response(&self, response: Response) -> Result<String, FetchError>

    /// Decode raw bytes with explicit charset
    pub fn decode_bytes(&self, bytes: &[u8], content_type: &str) -> Result<String, FetchError>

    /// Build request headers with random User-Agent
    fn build_headers(&self, referer: &str) -> HeaderMap

    /// Select random User-Agent from pool
    fn random_user_agent(&self) -> &'static str
}
```

#### 2.2 src/crawler/headers.rs - Header Management

```rust
// src/crawler/headers.rs

/// Build anti-bot headers for Naver News
pub fn build_naver_headers(user_agent: &str, referer: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert(USER_AGENT, HeaderValue::from_str(user_agent).unwrap());
    headers.insert(REFERER, HeaderValue::from_str(referer).unwrap());
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8"
        )
    );
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("ko-KR,ko;q=0.9,en-US;q=0.8,en;q=0.7")
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br")
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("document")
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("navigate")
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin")
    );

    headers
}

/// Generate referer URL for given section
pub fn section_referer(section_id: u32) -> String {
    format!(
        "https://news.naver.com/main/main.naver?mode=LSD&mid=shm&sid1={}",
        section_id
    )
}
```

### Test Cases for Day 2

| Test ID | Description | Function |
|---------|-------------|----------|
| TC-D2-001 | Successful fetch with mock server | `test_fetch_success` |
| TC-D2-002 | Rate limit handling (429 response) | `test_rate_limit_retry` |
| TC-D2-003 | Server error retry (5xx) | `test_server_error_retry` |
| TC-D2-004 | Timeout handling | `test_timeout_handling` |
| TC-D2-005 | EUC-KR decoding | `test_euc_kr_decoding` |
| TC-D2-006 | UTF-8 decoding | `test_utf8_decoding` |
| TC-D2-007 | Max retries exceeded | `test_max_retries_exceeded` |
| TC-D2-008 | User-Agent rotation | `test_user_agent_rotation` |
| TC-D2-009 | Referer header injection | `test_referer_injection` |

```rust
// tests/fetcher_test.rs
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_fetch_success() {
    let mock_server = MockServer::start().await;
    let html = "<html><body>Test</body></html>";

    Mock::given(method("GET"))
        .and(path("/article/001/123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 10).unwrap();
    let result = fetcher.fetch_article("/article/001/123", 100).await;

    assert!(result.is_ok());
    assert!(result.unwrap().contains("Test"));
}

#[tokio::test]
async fn test_rate_limit_retry() {
    let mock_server = MockServer::start().await;

    // First request returns 429, second succeeds
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 10).unwrap();
    let result = fetcher.fetch_article("/test", 100).await;

    assert!(result.is_ok());
}

#[test]
fn test_euc_kr_decoding() {
    let euc_kr_bytes = include_bytes!("fixtures/html/euc_kr_sample.html");
    let fetcher = NaverFetcher::new(2).unwrap();

    let result = fetcher.decode_bytes(euc_kr_bytes, "text/html; charset=euc-kr");

    assert!(result.is_ok());
    assert!(result.unwrap().contains("한글"));
}
```

### Acceptance Criteria for Day 2

- [ ] Fetcher successfully retrieves HTML from mock server
- [ ] Rate limiter prevents more than N requests per second
- [ ] Retry logic handles 429 and 5xx errors with backoff
- [ ] EUC-KR encoded pages decode correctly
- [ ] UTF-8 pages decode correctly
- [ ] Referer header matches section ID
- [ ] User-Agent rotates from pool

### Risk Areas - Day 2

| Risk | Mitigation |
|------|------------|
| Rate limiting too aggressive | Start with 1 req/sec, adjust based on testing |
| User-Agent detection | Update UA pool monthly |
| EUC-KR detection fails | Check meta charset tag as fallback |

---

## Day 3: News List Traversal, URL Collector, Pagination

### Objective
Implement news list page navigation and article URL collection.

### Files to Create

| File | Purpose | Dependencies |
|------|---------|--------------|
| `src/crawler/list.rs` | News list crawler | fetcher.rs |
| `src/crawler/url.rs` | URL extraction & validation | regex, url |

### Implementation Details

#### 3.1 src/crawler/list.rs - News List Navigation

```rust
// src/crawler/list.rs

/// News list page crawler
pub struct NewsListCrawler {
    fetcher: NaverFetcher,
    url_extractor: UrlExtractor,
}

impl NewsListCrawler {
    pub fn new(fetcher: NaverFetcher) -> Self

    /// Fetch article URLs from a category
    ///
    /// # Arguments
    /// * `category` - News category
    /// * `date` - Target date (YYYYMMDD format)
    /// * `max_pages` - Maximum pages to crawl (0 = unlimited)
    ///
    /// # Returns
    /// * Vector of article URLs
    pub async fn collect_urls(
        &self,
        category: NewsCategory,
        date: &str,
        max_pages: u32,
    ) -> Result<Vec<String>, CrawlerError>

    /// Fetch single list page
    async fn fetch_list_page(
        &self,
        category: NewsCategory,
        date: &str,
        page: u32,
    ) -> Result<(Vec<String>, bool), CrawlerError>

    /// Build list page URL
    fn build_list_url(&self, category: NewsCategory, date: &str, page: u32) -> String

    /// Check if there are more pages
    fn has_next_page(&self, html: &str, current_page: u32) -> bool
}

/// URL patterns for different Naver News sections
pub struct ListUrlBuilder;

impl ListUrlBuilder {
    /// Main news list URL
    /// Example: https://news.naver.com/main/list.naver?mode=LSD&mid=shm&sid1=100&date=20240115&page=1
    pub fn main_list(category: NewsCategory, date: &str, page: u32) -> String

    /// Section ranking list URL
    pub fn ranking_list(category: NewsCategory, page: u32) -> String
}
```

#### 3.2 src/crawler/url.rs - URL Extraction

```rust
// src/crawler/url.rs

/// Article URL extractor from list pages
pub struct UrlExtractor {
    article_pattern: Regex,
    mobile_pattern: Regex,
}

impl UrlExtractor {
    pub fn new() -> Self {
        Self {
            // Matches: /mnews/article/001/0014123456
            article_pattern: Regex::new(r"/mnews/article/(\d+)/(\d+)").unwrap(),
            // Matches mobile URLs for conversion
            mobile_pattern: Regex::new(r"m\.news\.naver\.com").unwrap(),
        }
    }

    /// Extract article URLs from list page HTML
    ///
    /// # Returns
    /// * Deduplicated vector of article URLs
    pub fn extract_urls(&self, html: &str) -> Vec<String>

    /// Normalize URL to desktop version
    pub fn normalize_url(&self, url: &str) -> String

    /// Extract oid and aid from URL
    ///
    /// # Example
    /// ```
    /// let url = "https://n.news.naver.com/mnews/article/001/0014123456";
    /// let (oid, aid) = extractor.extract_ids(url)?;
    /// assert_eq!(oid, "001");
    /// assert_eq!(aid, "0014123456");
    /// ```
    pub fn extract_ids(&self, url: &str) -> Result<(String, String), ParseError>

    /// Validate URL is a valid Naver news article
    pub fn is_valid_article_url(&self, url: &str) -> bool

    /// Convert relative URL to absolute
    pub fn to_absolute(&self, url: &str, base: &str) -> String
}

/// URL validation utilities
pub mod validators {
    /// Check if URL is from allowed Naver domains
    pub fn is_allowed_domain(url: &str) -> bool {
        let allowed = [
            "n.news.naver.com",
            "news.naver.com",
            "entertain.naver.com",
            "sports.news.naver.com",
        ];
        // ... implementation
    }

    /// Prevent SSRF - block internal IPs
    pub fn is_safe_url(url: &str) -> bool {
        // Block localhost, private IPs, etc.
    }
}
```

### Test Cases for Day 3

| Test ID | Description | Function |
|---------|-------------|----------|
| TC-D3-001 | Extract URLs from list page HTML | `test_url_extraction` |
| TC-D3-002 | URL deduplication | `test_url_dedup` |
| TC-D3-003 | Mobile to desktop URL conversion | `test_mobile_conversion` |
| TC-D3-004 | Extract oid/aid from URL | `test_id_extraction` |
| TC-D3-005 | Validate article URL format | `test_url_validation` |
| TC-D3-006 | Pagination detection | `test_pagination_detection` |
| TC-D3-007 | Build list page URL | `test_list_url_builder` |
| TC-D3-008 | SSRF prevention | `test_ssrf_prevention` |

```rust
// tests/url_test.rs
#[test]
fn test_url_extraction() {
    let html = include_str!("fixtures/html/list_page.html");
    let extractor = UrlExtractor::new();

    let urls = extractor.extract_urls(html);

    assert!(!urls.is_empty());
    assert!(urls.iter().all(|u| u.contains("/mnews/article/")));
}

#[test]
fn test_id_extraction() {
    let extractor = UrlExtractor::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014123456";

    let (oid, aid) = extractor.extract_ids(url).unwrap();

    assert_eq!(oid, "001");
    assert_eq!(aid, "0014123456");
}

#[test]
fn test_ssrf_prevention() {
    assert!(!validators::is_safe_url("http://127.0.0.1/admin"));
    assert!(!validators::is_safe_url("http://localhost/"));
    assert!(!validators::is_safe_url("http://192.168.1.1/"));
    assert!(validators::is_safe_url("https://n.news.naver.com/article/001/123"));
}
```

### Acceptance Criteria for Day 3

- [ ] URL extraction finds all article links in list page
- [ ] URLs are properly deduplicated
- [ ] Mobile URLs convert to desktop format
- [ ] oid/aid extraction works for all URL formats
- [ ] Pagination correctly identifies next page
- [ ] SSRF prevention blocks internal IPs

### Risk Areas - Day 3

| Risk | Mitigation |
|------|------------|
| List page HTML structure changes | Use multiple selector fallbacks |
| Pagination logic varies by section | Test with all 6 categories |
| URL patterns differ by article type | Comprehensive regex pattern |

---

## Day 4: Article Parser with Fallback (Entertainment/Sports/Card News)

### Objective
Implement robust HTML parser with fallback strategies for different article formats.

### Files to Create

| File | Purpose | Dependencies |
|------|---------|--------------|
| `src/parser/mod.rs` | Parser module exports | None |
| `src/parser/html.rs` | Main article parser | scraper |
| `src/parser/selectors.rs` | CSS selectors by format | None |
| `src/parser/sanitize.rs` | Text sanitization | regex |

### Implementation Details

#### 4.1 src/parser/html.rs - Core Parser

```rust
// src/parser/html.rs

/// Article HTML parser with multi-format support
pub struct ArticleParser {
    general_selectors: GeneralSelectors,
    entertainment_selectors: EntertainmentSelectors,
    sports_selectors: SportsSelectors,
    card_selectors: CardNewsSelectors,
}

impl ArticleParser {
    pub fn new() -> Self

    /// Parse article with automatic format detection and fallback
    ///
    /// # Fallback Order
    /// 1. General news format (div#dic_area)
    /// 2. Entertainment format (div.article_body)
    /// 3. Sports format (div.news_end)
    /// 4. Card/Photo news format (multiple selectors)
    pub fn parse_with_fallback(&self, html: &str, url: &str) -> Result<ParsedArticle, ParseError>

    /// Parse standard news article
    fn parse_general(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError>

    /// Parse entertainment news
    fn parse_entertainment(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError>

    /// Parse sports news
    fn parse_sports(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError>

    /// Parse card/photo news
    fn parse_card(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError>

    /// Extract title from document
    fn extract_title(&self, document: &Html, selectors: &[Selector]) -> Option<String>

    /// Extract content from document
    fn extract_content(&self, document: &Html, selector: &Selector) -> Option<String>

    /// Extract publication date
    fn extract_date(&self, document: &Html) -> Option<DateTime<Utc>>

    /// Extract publisher name
    fn extract_publisher(&self, document: &Html) -> Option<String>

    /// Clean and normalize extracted text
    fn sanitize_content(&self, content: &str) -> String

    /// Remove noise elements (ads, related articles, etc.)
    fn remove_noise(&self, document: &mut Html)
}

/// Detect article format from HTML structure
pub fn detect_format(html: &str) -> ArticleFormat {
    let doc = Html::parse_document(html);

    if doc.select(&Selector::parse("#dic_area").unwrap()).next().is_some() {
        ArticleFormat::General
    } else if doc.select(&Selector::parse(".article_body").unwrap()).next().is_some() {
        ArticleFormat::Entertainment
    } else if doc.select(&Selector::parse(".news_end").unwrap()).next().is_some() {
        ArticleFormat::Sports
    } else {
        ArticleFormat::Card
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArticleFormat {
    General,
    Entertainment,
    Sports,
    Card,
    Unknown,
}
```

#### 4.2 src/parser/selectors.rs - CSS Selectors

```rust
// src/parser/selectors.rs

/// Selectors for general news format
pub struct GeneralSelectors {
    pub title: Selector,
    pub content: Selector,
    pub date: Selector,
    pub publisher: Selector,
    pub author: Selector,
}

impl GeneralSelectors {
    pub fn new() -> Self {
        Self {
            title: Selector::parse("#title_area span, .media_end_head_title").unwrap(),
            content: Selector::parse("#dic_area, #articleBodyContents").unwrap(),
            date: Selector::parse(".media_end_head_info_datestamp_time, ._ARTICLE_DATE_TIME").unwrap(),
            publisher: Selector::parse(".media_end_head_top_logo img, .press_logo img").unwrap(),
            author: Selector::parse(".byline, .journalist_name").unwrap(),
        }
    }
}

/// Selectors for entertainment news
pub struct EntertainmentSelectors {
    pub title: Selector,
    pub content: Selector,
    pub date: Selector,
}

impl EntertainmentSelectors {
    pub fn new() -> Self {
        Self {
            title: Selector::parse(".end_tit, h2.end_tit").unwrap(),
            content: Selector::parse(".article_body, #articeBody").unwrap(),
            date: Selector::parse(".article_info .author em, .info_date").unwrap(),
        }
    }
}

/// Selectors for sports news
pub struct SportsSelectors {
    pub title: Selector,
    pub content: Selector,
    pub date: Selector,
}

impl SportsSelectors {
    pub fn new() -> Self {
        Self {
            title: Selector::parse(".news_headline .title, h4.title").unwrap(),
            content: Selector::parse(".news_end, #newsEndContents").unwrap(),
            date: Selector::parse(".info span, .news_date").unwrap(),
        }
    }
}

/// Selectors for card/photo news
pub struct CardNewsSelectors {
    pub title: Vec<Selector>,
    pub content: Vec<Selector>,
    pub captions: Selector,
}

impl CardNewsSelectors {
    pub fn new() -> Self {
        Self {
            title: vec![
                Selector::parse("h2.end_tit").unwrap(),
                Selector::parse(".media_end_head_title").unwrap(),
                Selector::parse("h3.tit_view").unwrap(),
            ],
            content: vec![
                Selector::parse("div.end_ct_area").unwrap(),
                Selector::parse("div.card_area").unwrap(),
                Selector::parse("div.content_area").unwrap(),
            ],
            captions: Selector::parse("em.img_desc, .txt, figcaption").unwrap(),
        }
    }
}

/// Noise elements to remove
pub const NOISE_SELECTORS: &[&str] = &[
    "em.img_desc",           // Image captions (usually noise)
    "div.link_news",         // Related article links
    ".end_photo_org",        // Photo area
    ".vod_player_wrap",      // Video player
    "script",
    "style",
    "noscript",
    "iframe",
    ".ad_wrap",              // Advertisements
    ".reporter_area",        // Reporter info
];
```

#### 4.3 src/parser/sanitize.rs - Text Cleaning

```rust
// src/parser/sanitize.rs

/// Sanitize extracted text content
pub fn sanitize_text(text: &str) -> String {
    let mut result = text.to_string();

    // 1. Remove zero-width characters
    result = remove_zero_width(&result);

    // 2. Remove control characters (except newline/tab)
    result = remove_control_chars(&result);

    // 3. Normalize whitespace
    result = normalize_whitespace(&result);

    // 4. Trim lines
    result = trim_lines(&result);

    result
}

/// Remove zero-width spaces and similar characters
pub fn remove_zero_width(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(*c,
            '\u{200B}'..='\u{200F}' |  // Zero-width spaces
            '\u{2028}'..='\u{202F}' |  // Line/paragraph separators
            '\u{FEFF}'                  // BOM
        ))
        .collect()
}

/// Remove control characters except newline and tab
pub fn remove_control_chars(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

/// Normalize multiple whitespace to single space
pub fn normalize_whitespace(text: &str) -> String {
    let re = Regex::new(r"[ \t]+").unwrap();
    re.replace_all(text, " ").to_string()
}

/// Trim each line and remove excessive blank lines
pub fn trim_lines(text: &str) -> String {
    text.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert HTML entities to plain text
pub fn decode_html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
```

### Test Cases for Day 4

| Test ID | Description | Function |
|---------|-------------|----------|
| TC-D4-001 | Parse general news | `test_parse_general_news` |
| TC-D4-002 | Parse entertainment news | `test_parse_entertainment` |
| TC-D4-003 | Parse sports news | `test_parse_sports` |
| TC-D4-004 | Parse card news | `test_parse_card_news` |
| TC-D4-005 | Fallback chain works | `test_fallback_chain` |
| TC-D4-006 | Handle deleted article | `test_deleted_article` |
| TC-D4-007 | Zero-width space removal | `test_zero_width_removal` |
| TC-D4-008 | Control character removal | `test_control_char_removal` |
| TC-D4-009 | Format detection | `test_format_detection` |
| TC-D4-010 | Date extraction | `test_date_extraction` |

```rust
// tests/parser_test.rs
#[test]
fn test_parse_general_news() {
    let html = include_str!("fixtures/html/general_news_001.html");
    let parser = ArticleParser::new();

    let result = parser.parse_with_fallback(html, "https://n.news.naver.com/mnews/article/001/0014123456");

    assert!(result.is_ok());
    let article = result.unwrap();
    assert!(!article.title.is_empty());
    assert!(!article.content.is_empty());
    assert_eq!(article.oid, "001");
}

#[test]
fn test_fallback_chain() {
    let entertainment_html = include_str!("fixtures/html/entertainment_001.html");
    let parser = ArticleParser::new();

    // Should fail general parse, succeed with entertainment fallback
    let result = parser.parse_with_fallback(entertainment_html, "https://entertain.naver.com/...");

    assert!(result.is_ok());
}

#[test]
fn test_zero_width_removal() {
    let text = "hello\u{200B}world\u{FEFF}test";
    let clean = sanitize_text(text);

    assert!(!clean.contains('\u{200B}'));
    assert!(!clean.contains('\u{FEFF}'));
    assert_eq!(clean, "hello world test");
}
```

### Acceptance Criteria for Day 4

- [ ] General news articles parse correctly
- [ ] Entertainment format fallback works
- [ ] Sports format fallback works
- [ ] Card/photo news fallback works
- [ ] Zero-width characters removed from output
- [ ] Control characters removed from output
- [ ] Deleted articles return appropriate error
- [ ] Date parsing works for multiple formats

### Risk Areas - Day 4

| Risk | Mitigation |
|------|------------|
| Selector changes break parsing | Multiple fallback selectors |
| New article format not handled | Log unknown formats for analysis |
| Date format variations | Support multiple date patterns |

---

## Day 5: Markdown Template, SQLite Storage, CLI

### Objective
Implement file storage, database tracking, and command-line interface.

### Files to Create

| File | Purpose | Dependencies |
|------|---------|--------------|
| `src/storage/mod.rs` | Storage module exports | None |
| `src/storage/markdown.rs` | Markdown generation | handlebars |
| `src/storage/database.rs` | SQLite operations | rusqlite |
| `src/storage/checkpoint.rs` | Resume state management | serde_json |
| `src/main.rs` | CLI implementation | clap |
| `templates/article.hbs` | Markdown template | None |

### Implementation Details

#### 5.1 src/storage/markdown.rs - Markdown Generation

```rust
// src/storage/markdown.rs

/// Markdown file generator using Handlebars
pub struct MarkdownGenerator {
    engine: Handlebars<'static>,
    output_dir: PathBuf,
}

impl MarkdownGenerator {
    /// Create generator with template
    pub fn new(output_dir: PathBuf) -> Result<Self, StorageError>

    /// Generate and save markdown file for article
    ///
    /// # Returns
    /// * Path to saved file
    pub fn save_article(&self, article: &ParsedArticle) -> Result<PathBuf, StorageError>

    /// Render article to markdown string
    pub fn render(&self, article: &ParsedArticle) -> Result<String, StorageError>

    /// Generate safe filename from article
    ///
    /// # Format
    /// {YYYYMMDD}_{category}_{oid}_{aid}_{title_slug}.md
    pub fn generate_filename(article: &ParsedArticle) -> String

    /// Convert title to URL-safe slug
    fn slugify(title: &str) -> String

    /// Sanitize filename (remove special characters)
    fn sanitize_filename(name: &str) -> String
}

/// Article context for template rendering
#[derive(Serialize)]
pub struct ArticleContext {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub publisher: String,
    pub author: String,
    pub published_at: String,
    pub crawled_at: String,
    pub url: String,
    pub oid: String,
    pub aid: String,
}

impl From<&ParsedArticle> for ArticleContext {
    fn from(article: &ParsedArticle) -> Self { ... }
}
```

#### 5.2 templates/article.hbs - Markdown Template

```handlebars
---
id: {{id}}
title: "{{title}}"
category: {{category}}
publisher: {{publisher}}
author: {{author}}
published_at: {{published_at}}
crawled_at: {{crawled_at}}
url: {{url}}
oid: {{oid}}
aid: {{aid}}
---

# {{title}}

**{{publisher}}** | {{published_at}} | {{category}}

---

{{content}}

---

*Crawled at: {{crawled_at}}*
*Source: [{{url}}]({{url}})*
```

#### 5.3 src/storage/database.rs - SQLite Operations

```rust
// src/storage/database.rs

/// SQLite metadata database
pub struct MetadataDB {
    conn: rusqlite::Connection,
}

impl MetadataDB {
    /// Open or create database
    pub fn new(path: &str) -> Result<Self, StorageError>

    /// Initialize schema
    fn init_schema(&self) -> Result<(), StorageError>

    /// Check if article exists by oid+aid
    pub fn article_exists(&self, oid: &str, aid: &str) -> Result<bool, StorageError>

    /// Check if article exists by URL
    pub fn article_exists_by_url(&self, url: &str) -> Result<bool, StorageError>

    /// Insert article record
    pub fn insert_article(&self, article: &ParsedArticle, file_path: &str) -> Result<(), StorageError>

    /// Update indexed status
    pub fn mark_indexed(&self, article_id: &str) -> Result<(), StorageError>

    /// Get unindexed articles
    pub fn get_unindexed_articles(&self, limit: u32) -> Result<Vec<ArticleRecord>, StorageError>

    /// Get article count by category
    pub fn count_by_category(&self) -> Result<HashMap<String, u32>, StorageError>

    /// Get total article count
    pub fn total_count(&self) -> Result<u32, StorageError>
}

/// Database record for articles
#[derive(Debug)]
pub struct ArticleRecord {
    pub id: String,
    pub oid: String,
    pub aid: String,
    pub title: String,
    pub category: String,
    pub file_path: String,
    pub indexed: bool,
    pub created_at: DateTime<Utc>,
}

// SQL Schema
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS articles (
    id TEXT PRIMARY KEY,
    oid TEXT NOT NULL,
    aid TEXT NOT NULL,
    title TEXT NOT NULL,
    category TEXT NOT NULL,
    url TEXT NOT NULL UNIQUE,
    file_path TEXT NOT NULL,
    content_hash TEXT,
    indexed INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    UNIQUE(oid, aid)
);

CREATE INDEX IF NOT EXISTS idx_articles_category ON articles(category);
CREATE INDEX IF NOT EXISTS idx_articles_indexed ON articles(indexed);
CREATE INDEX IF NOT EXISTS idx_articles_created ON articles(created_at);
"#;
```

#### 5.4 src/storage/checkpoint.rs - Resume State

```rust
// src/storage/checkpoint.rs

/// Checkpoint manager for resume functionality
pub struct CheckpointManager {
    checkpoint_dir: PathBuf,
}

impl CheckpointManager {
    pub fn new(checkpoint_dir: PathBuf) -> Self

    /// Load checkpoint state or create new
    pub fn load(&self, name: &str) -> CrawlState

    /// Save checkpoint state
    pub fn save(&self, name: &str, state: &CrawlState) -> Result<(), StorageError>

    /// Get checkpoint path
    fn checkpoint_path(&self, name: &str) -> PathBuf

    /// List available checkpoints
    pub fn list_checkpoints(&self) -> Vec<String>

    /// Delete checkpoint
    pub fn delete(&self, name: &str) -> Result<(), StorageError>
}
```

#### 5.5 src/main.rs - CLI Implementation

```rust
// src/main.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "baram")]
#[command(about = "Naver News Crawler with Vector DB")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config file path
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Crawl news articles
    Crawl {
        /// News category (politics, economy, society, culture, world, it)
        #[arg(short, long)]
        category: String,

        /// Maximum articles to crawl
        #[arg(short, long, default_value = "100")]
        max_articles: u32,

        /// Target date (YYYYMMDD)
        #[arg(short, long)]
        date: Option<String>,

        /// Include comments
        #[arg(long)]
        with_comments: bool,
    },

    /// Crawl single URL
    CrawlUrl {
        /// Article URL
        url: String,

        /// Include comments
        #[arg(long)]
        with_comments: bool,
    },

    /// Resume interrupted crawl
    Resume {
        /// Checkpoint name
        #[arg(short, long)]
        checkpoint: String,
    },

    /// Show statistics
    Stats,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level);

    // Load config
    let config = load_config(Some(&cli.config))?;

    match cli.command {
        Commands::Crawl { category, max_articles, date, with_comments } => {
            run_crawl(config, &category, max_articles, date.as_deref(), with_comments).await
        }
        Commands::CrawlUrl { url, with_comments } => {
            run_crawl_url(config, &url, with_comments).await
        }
        Commands::Resume { checkpoint } => {
            run_resume(config, &checkpoint).await
        }
        Commands::Stats => {
            show_stats(config)
        }
    }
}

async fn run_crawl(
    config: Config,
    category: &str,
    max_articles: u32,
    date: Option<&str>,
    _with_comments: bool,  // Phase 2
) -> Result<(), AppError> {
    let category = NewsCategory::from_str(category)?;
    let date = date.unwrap_or(&today_string());

    // Initialize components
    let fetcher = NaverFetcher::new(config.crawler.requests_per_second)?;
    let list_crawler = NewsListCrawler::new(fetcher.clone());
    let parser = ArticleParser::new();
    let markdown_gen = MarkdownGenerator::new(config.storage.output_dir.clone())?;
    let db = MetadataDB::new(config.storage.sqlite_path.to_str().unwrap())?;
    let checkpoint = CheckpointManager::new(config.storage.checkpoint_dir.clone());

    // Load or create crawl state
    let mut state = checkpoint.load(&format!("crawl_{}_{}", category.as_str(), date));

    // Collect URLs
    tracing::info!("Collecting article URLs for {} on {}", category.as_str(), date);
    let urls = list_crawler.collect_urls(category, date, 0).await?;

    let mut crawled = 0;
    for url in urls {
        if crawled >= max_articles {
            break;
        }

        let article_id = UrlExtractor::new().extract_ids(&url)
            .map(|(oid, aid)| format!("{}_{}", oid, aid))
            .unwrap_or_default();

        if state.is_completed(&article_id) {
            tracing::debug!("Skipping already crawled: {}", article_id);
            continue;
        }

        if db.article_exists_by_url(&url)? {
            tracing::debug!("Skipping existing in DB: {}", url);
            state.mark_completed(&article_id);
            continue;
        }

        // Fetch and parse
        match fetcher.fetch_article(&url, category.to_section_id()).await {
            Ok(html) => {
                match parser.parse_with_fallback(&html, &url) {
                    Ok(mut article) => {
                        article.category = category.as_str().to_string();
                        article.compute_hash();

                        // Save to file
                        let file_path = markdown_gen.save_article(&article)?;

                        // Record in database
                        db.insert_article(&article, file_path.to_str().unwrap())?;

                        state.mark_completed(&article_id);
                        crawled += 1;

                        tracing::info!("[{}/{}] Saved: {}", crawled, max_articles, article.title);
                    }
                    Err(e) => {
                        tracing::warn!("Parse error for {}: {}", url, e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Fetch error for {}: {}", url, e);
            }
        }

        // Save checkpoint periodically
        if crawled % 10 == 0 {
            checkpoint.save(&format!("crawl_{}_{}", category.as_str(), date), &state)?;
        }
    }

    // Final checkpoint save
    checkpoint.save(&format!("crawl_{}_{}", category.as_str(), date), &state)?;

    tracing::info!("Crawl complete: {} articles saved", crawled);
    Ok(())
}
```

### Test Cases for Day 5

| Test ID | Description | Function |
|---------|-------------|----------|
| TC-D5-001 | Markdown file creation | `test_markdown_creation` |
| TC-D5-002 | Filename sanitization | `test_filename_sanitization` |
| TC-D5-003 | SQLite insert/query | `test_db_insert_query` |
| TC-D5-004 | Duplicate detection | `test_duplicate_detection` |
| TC-D5-005 | Checkpoint save/load | `test_checkpoint_persistence` |
| TC-D5-006 | Checkpoint corruption recovery | `test_checkpoint_recovery` |
| TC-D5-007 | CLI argument parsing | `test_cli_parsing` |
| TC-D5-008 | End-to-end crawl (integration) | `test_e2e_crawl` |

```rust
// tests/storage_test.rs
#[test]
fn test_markdown_creation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let gen = MarkdownGenerator::new(temp_dir.path().to_path_buf()).unwrap();

    let article = ParsedArticle {
        oid: "001".to_string(),
        aid: "0014123456".to_string(),
        title: "Test Article".to_string(),
        content: "Article content here.".to_string(),
        category: "politics".to_string(),
        ..Default::default()
    };

    let path = gen.save_article(&article).unwrap();

    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("Test Article"));
}

#[test]
fn test_filename_sanitization() {
    let article = ParsedArticle {
        title: "Test/Article:With*Special?Characters".to_string(),
        ..Default::default()
    };

    let filename = MarkdownGenerator::generate_filename(&article);

    assert!(!filename.contains('/'));
    assert!(!filename.contains(':'));
    assert!(!filename.contains('*'));
    assert!(!filename.contains('?'));
}

#[test]
fn test_db_insert_query() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let db = MetadataDB::new(temp_file.path().to_str().unwrap()).unwrap();

    let article = ParsedArticle {
        oid: "001".to_string(),
        aid: "0014123456".to_string(),
        url: "https://example.com/article".to_string(),
        ..Default::default()
    };

    db.insert_article(&article, "/path/to/file.md").unwrap();

    assert!(db.article_exists("001", "0014123456").unwrap());
    assert!(db.article_exists_by_url("https://example.com/article").unwrap());
}
```

### Acceptance Criteria for Day 5

- [ ] Markdown files saved with correct format
- [ ] Filenames are sanitized (no special characters)
- [ ] SQLite database tracks all crawled articles
- [ ] Duplicate articles are skipped
- [ ] Checkpoint saves/loads correctly
- [ ] Corrupted checkpoint returns empty state
- [ ] CLI commands work as expected
- [ ] End-to-end crawl completes successfully

### Risk Areas - Day 5

| Risk | Mitigation |
|------|------------|
| File path collisions | Include timestamp or hash in filename |
| SQLite concurrent access | Use WAL mode |
| Checkpoint corruption | Atomic write with temp file |

---

## Day 6: Buffer Day

### Purpose
Address any issues from Days 1-5, code review, and prepare for Phase 2.

### Activities

1. **Bug Fixes**: Address any failing tests or discovered issues
2. **Code Review**: Review all implemented code for quality
3. **Documentation**: Update inline documentation and README
4. **Performance**: Profile and optimize if needed
5. **Phase 2 Prep**: Review comment API requirements

---

## File Structure Summary

```
baram/
├── .dev/
│   └── phase1_plan.md           # This file
├── src/
│   ├── lib.rs                   # Library root
│   ├── main.rs                  # CLI entry point
│   ├── models.rs                # Data structures
│   ├── config/
│   │   ├── mod.rs
│   │   └── settings.rs          # Configuration
│   ├── crawler/
│   │   ├── mod.rs
│   │   ├── fetcher.rs           # HTTP client
│   │   ├── headers.rs           # Anti-bot headers
│   │   ├── list.rs              # List page crawler
│   │   └── url.rs               # URL extraction
│   ├── parser/
│   │   ├── mod.rs
│   │   ├── html.rs              # HTML parser
│   │   ├── selectors.rs         # CSS selectors
│   │   └── sanitize.rs          # Text cleaning
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── markdown.rs          # Markdown generation
│   │   ├── database.rs          # SQLite operations
│   │   └── checkpoint.rs        # Resume state
│   └── utils/
│       ├── mod.rs
│       └── error.rs             # Error types
├── templates/
│   └── article.hbs              # Markdown template
├── tests/
│   ├── fixtures/
│   │   ├── html/
│   │   │   └── .gitkeep
│   │   └── jsonp/
│   │       └── .gitkeep
│   ├── config_test.rs
│   ├── models_test.rs
│   ├── fetcher_test.rs
│   ├── url_test.rs
│   ├── parser_test.rs
│   ├── storage_test.rs
│   └── integration_test.rs
├── output/
│   ├── raw/                     # Markdown files
│   └── checkpoints/             # Resume state
├── config.toml                  # Default config
└── Cargo.toml                   # Dependencies
```

---

## Risk Matrix

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Naver HTML structure change | High | Medium | Multiple selector fallbacks, regression tests |
| Rate limiting/blocking | High | Medium | Conservative rate limit, User-Agent rotation |
| EUC-KR encoding edge cases | Medium | Low | Comprehensive encoding detection |
| Parser fails on new format | Medium | Medium | Fallback chain, log unknown formats |
| SQLite performance | Low | Low | Use WAL mode, proper indexing |
| File system errors | Medium | Low | Atomic writes, error recovery |

---

## Test Coverage Targets

| Module | Target | Priority |
|--------|--------|----------|
| `crawler/fetcher.rs` | >= 85% | P0 |
| `parser/html.rs` | >= 90% | P0 |
| `storage/database.rs` | >= 85% | P0 |
| `storage/markdown.rs` | >= 80% | P1 |
| `utils/error.rs` | >= 75% | P2 |
| **Overall** | **>= 80%** | - |

---

## Verification Commands

```bash
# Build and test
cargo build --release
cargo test --all-features

# Lint and format
cargo clippy -- -D warnings
cargo fmt --check

# Run crawler
cargo run -- crawl --category politics --max-articles 100

# Check output
ls -la output/raw/*.md | wc -l  # Should be 100
file output/raw/*.md | grep -v "UTF-8"  # Should be empty

# Check database
sqlite3 data/metadata.db "SELECT COUNT(*) FROM articles"  # Should be 100
```

---

## Phase 1 Milestone Verification

```
Criteria: Successfully crawl 100 articles from politics/economy sections
          with no encoding issues and save as valid UTF-8 Markdown files

Verification Steps:
1. Run: cargo run -- crawl --category politics --max-articles 50
2. Run: cargo run -- crawl --category economy --max-articles 50
3. Verify: ls output/raw/*.md | wc -l  # >= 100
4. Verify: file output/raw/*.md | grep -c "UTF-8"  # >= 100
5. Verify: sqlite3 data/metadata.db "SELECT COUNT(*) FROM articles"  # >= 100
6. Manual: Open 5 random articles, verify content is readable
```

---

## Handoff to Phase 2

### Completed Components
- HTTP Fetcher with rate limiting
- HTML Parser with fallbacks
- Markdown storage
- SQLite tracking
- Basic CLI

### Ready for Phase 2
- Comment API integration
- JSONP parsing
- Recursive reply collection
- Actor model pipeline
- Async concurrency

---

> **Document Version**: 1.0
> **Created**: Phase 1 Planning
> **Last Updated**: 2024-01-15
> **Author**: baram Development Team
