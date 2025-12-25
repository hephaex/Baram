# ktime Module Architecture

## Overview

This document describes the module architecture for the ktime Naver News Crawler, designed following Rust best practices and the Actor Model pattern.

## Module Structure

```
src/
├── utils/
│   ├── mod.rs           # Utility module re-exports
│   └── error.rs         # Comprehensive error types
├── config/
│   ├── mod.rs           # Configuration module re-exports
│   └── settings.rs      # Configuration structures and loaders
├── crawler/
│   ├── mod.rs           # Crawler module re-exports
│   └── types.rs         # Core data types
└── main.rs              # Application entry point
```

## Core Modules

### 1. utils::error (`src/utils/error.rs`)

Comprehensive error handling using `thiserror` crate.

#### Error Types:

- **AppError**: Top-level application error
- **CrawlerError**: HTTP requests, parsing, rate limiting
  - HttpError, RateLimitError, ParseError
  - InvalidUrl, EncodingError
  - CommentApiError, JsonpParseError
  - AntiBotDetected, RetryExhausted
  - Timeout, ArticleNotFound

- **StorageError**: Database and file operations
  - SqliteError, PostgresError
  - FileError, SerializationError
  - DuplicateEntry, TransactionError
  - CheckpointError, MarkdownError

- **VectorError**: Vector database operations
  - IndexError, SearchError, EmbeddingError
  - TokenizationError, ModelLoadError
  - BulkOperationError, DimensionMismatch

- **OntologyError**: Knowledge graph extraction
  - ExtractionError, ValidationError
  - PromptError, HallucinationDetected
  - TemplateError, GraphError

- **ConfigError**: Configuration validation
  - FileNotFound, InvalidToml
  - MissingField, InvalidValue
  - EnvVarNotFound, PathError

#### Result Type Aliases:

```rust
pub type CrawlerResult<T> = Result<T, CrawlerError>;
pub type StorageResult<T> = Result<T, StorageError>;
pub type VectorResult<T> = Result<T, VectorError>;
pub type OntologyResult<T> = Result<T, OntologyError>;
pub type ConfigResult<T> = Result<T, ConfigError>;
pub type AppResult<T> = Result<T, AppError>;
```

### 2. config::settings (`src/config/settings.rs`)

Configuration management with TOML and environment variable support.

#### Configuration Structures:

- **AppConfig**: Main configuration container
  - AppMetadata: Application info (name, version, environment)
  - CrawlerConfig: Crawler settings
  - PostgresConfig: Database connection
  - OpenSearchConfig: Vector search
  - StorageConfig: File and checkpoint storage
  - EmbeddingConfig: ML model settings
  - OntologyConfig: LLM extraction settings
  - LoggingConfig: Logging configuration

#### Key Features:

- Environment variable expansion: `${VAR}` or `${VAR:-default}`
- Hierarchical validation
- Default value providers
- Path validation and creation
- Type-safe configuration with strong defaults

#### CrawlerConfig Details:

```rust
pub struct CrawlerConfig {
    pub base_url: String,
    pub requests_per_second: u32,      // Rate limiting
    pub max_retries: u32,              // Retry attempts
    pub timeout_secs: u64,             // Request timeout
    pub user_agents: Vec<String>,      // User-Agent pool
    pub max_concurrent_workers: usize, // Actor workers
    pub channel_buffer_size: usize,    // mpsc buffer
    pub enable_cookie_jar: bool,
    pub enable_compression: bool,
    pub follow_redirects: bool,
    pub max_redirects: usize,
    pub backoff_base_ms: u64,          // Exponential backoff
    pub backoff_max_ms: u64,
    pub comments: CommentConfig,
}
```

### 3. crawler::types (`src/crawler/types.rs`)

Core data structures for crawling operations.

#### Main Types:

**NewsCategory** (enum):
- Politics, Economy, Society, Culture, World, IT, Entertainment, Sports
- Methods: `code()`, `korean_name()`, `from_code()`, `from_str()`

**ParsedArticle** (struct):
```rust
pub struct ParsedArticle {
    pub oid: String,                  // Publisher ID
    pub aid: String,                  // Article ID
    pub title: String,
    pub content: String,
    pub url: String,
    pub category: NewsCategory,
    pub publisher: Option<String>,
    pub published_at: DateTime<Utc>,
    pub crawled_at: DateTime<Utc>,
    pub content_hash: Option<String>, // SHA-256
    pub author: Option<String>,
    pub subtitle: Option<String>,
    pub tags: Vec<String>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub related_articles: Vec<String>,
    pub thumbnail_url: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

Methods:
- `id()`: Returns `{oid}_{aid}`
- `generate_hash()`: Computes SHA-256 hash
- `date()`, `date_path()`: Date formatting
- `title_slug()`: URL-safe title
- `has_comments()`: Boolean check

**Comment** (struct):
```rust
pub struct Comment {
    pub comment_id: String,
    pub parent_id: Option<String>,    // For replies
    pub article_id: String,
    pub author: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub like_count: u64,
    pub dislike_count: u64,
    pub depth: u32,                   // Nesting level
    pub is_deleted: bool,
    pub is_hidden: bool,
    pub reply_count: u64,
    pub replies: Vec<Comment>,        // Recursive
    pub comment_type: CommentType,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

Methods:
- `new()`, `new_reply()`: Constructors
- `is_visible()`: Filter deleted/hidden
- `total_replies()`: Recursive count
- `flatten()`: DFS traversal
- `add_reply()`: Add child comment

**CrawlState** (struct):
```rust
pub struct CrawlState {
    pub session_id: String,
    pub category: NewsCategory,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub total_articles: Option<usize>,
    pub articles_crawled: usize,
    pub articles_failed: usize,
    pub articles_skipped: usize,      // Duplicates
    pub comments_crawled: usize,
    pub current_page: u32,
    pub last_article_id: Option<String>,
    pub failed_articles: Vec<String>, // For retry
    pub status: CrawlStatus,
    pub error_message: Option<String>,
    pub stats: CrawlStatistics,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

Methods:
- `new()`: Initialize new session
- `mark_success()`, `mark_failed()`, `mark_skipped()`: Update progress
- `completion_percentage()`: Progress calculation
- `crawl_rate()`: Articles per minute
- `estimated_time_remaining()`: ETA

**CrawlStatus** (enum):
- Running, Paused, Completed, Failed, Cancelled

**CrawlStatistics** (struct):
- Performance metrics
- Error distribution
- HTTP status codes
- Category counts

## Configuration File (`config.toml`)

### Structure:

```toml
[app]
name = "ktime"
version = "0.1.0"
environment = "development"

[crawler]
base_url = "https://news.naver.com"
requests_per_second = 5
max_retries = 3
timeout_secs = 30
user_agents = [...]
max_concurrent_workers = 10
channel_buffer_size = 1000
# ... more settings

[crawler.comments]
enabled = true
max_pages = 100
max_reply_depth = 10

[storage]
output_dir = "./output/raw"
checkpoint_dir = "./output/checkpoints"
sqlite_path = "./output/crawler.db"

# Optional: PostgreSQL, OpenSearch, Embedding, Ontology
```

### Environment Variable Expansion:

```toml
password = "${DB_PASSWORD}"
api_key = "${LLM_API_KEY:-default_key}"
```

## Design Patterns

### 1. Error Handling

- Use `thiserror` for domain errors
- Provide detailed context in error messages
- Include source location when wrapping errors
- Type aliases for cleaner function signatures

### 2. Configuration

- Single source of truth (config.toml)
- Environment-specific overrides
- Validation at load time
- Type-safe defaults

### 3. Data Types

- Strong typing with enums
- Builder pattern where applicable
- Recursive structures for comments
- Comprehensive metadata fields

### 4. Serialization

- `serde` for all structs
- chrono for timestamps
- HashMap for flexible metadata
- SHA-256 for deduplication

## Usage Examples

### Loading Configuration:

```rust
use ktime::config::AppConfig;

let config = AppConfig::load("config.toml")?;
config.validate()?;
```

### Error Handling:

```rust
use ktime::utils::{CrawlerError, CrawlerResult};

fn fetch_article(url: &str) -> CrawlerResult<String> {
    // ... fetch logic
    Err(CrawlerError::Timeout {
        url: url.to_string(),
        duration_secs: 30,
    })
}
```

### Working with Types:

```rust
use ktime::crawler::{NewsCategory, ParsedArticle, CrawlState};

let mut article = ParsedArticle::default();
article.oid = "001".to_string();
article.aid = "12345".to_string();
article.generate_hash();

let state = CrawlState::new("session1".to_string(), NewsCategory::Politics);
state.mark_success(article.id());
```

## Testing

Each module includes comprehensive unit tests:

- Error type display formatting
- Error conversion chains
- Configuration validation
- Default values
- Type methods and helpers

Run tests:
```bash
cargo test
```

## Next Steps

1. Implement fetcher module (`src/crawler/fetcher.rs`)
2. Implement parser module (`src/crawler/parser.rs`)
3. Implement storage module (`src/storage/mod.rs`)
4. Create Actor Model pipeline (`src/pipeline/mod.rs`)
5. Build CLI interface (`src/main.rs`)

## References

- Sprint Plan: `/home/mare/ktime/sprint_plan.md`
- Development Spec: `/home/mare/ktime/development_spec.md`
- Configuration: `/home/mare/ktime/config.toml`

---

Copyright (c) 2024 hephaex@gmail.com
License: GPL v3
