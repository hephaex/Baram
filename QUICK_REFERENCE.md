# Quick Reference - nTimes Module Architecture

## File Locations

```
/home/mare/nTimes/
├── config.toml                    # Configuration file
├── src/
│   ├── utils/
│   │   ├── mod.rs                 # Module exports
│   │   └── error.rs               # Error types (342 lines)
│   ├── config/
│   │   ├── mod.rs                 # Module exports
│   │   └── settings.rs            # Configuration (809 lines)
│   └── crawler/
│       ├── mod.rs                 # Module exports
│       └── types.rs               # Data types (399 lines)
├── ARCHITECTURE.md                # Detailed documentation
└── MODULE_SETUP_SUMMARY.md        # Setup summary
```

## Error Types Quick Reference

```rust
use ntimes::utils::*;

// Top-level error
AppError
  ├── Crawler(CrawlerError)
  ├── Storage(StorageError)
  ├── Vector(VectorError)
  ├── Ontology(OntologyError)
  └── Config(ConfigError)

// Result aliases
type CrawlerResult<T> = Result<T, CrawlerError>;
type StorageResult<T> = Result<T, StorageError>;
type VectorResult<T> = Result<T, VectorError>;
type OntologyResult<T> = Result<T, OntologyError>;
type ConfigResult<T> = Result<T, ConfigError>;
type AppResult<T> = Result<T, AppError>;
```

## Configuration Quick Reference

```rust
use ntimes::config::AppConfig;

// Load configuration
let config = AppConfig::load("config.toml")?;

// Access settings
config.app.name                              // "nTimes"
config.app.environment                       // "development"
config.crawler.requests_per_second           // 5
config.crawler.max_concurrent_workers        // 10
config.crawler.timeout_secs                  // 30
config.crawler.comments.max_pages            // 100
config.storage.output_dir                    // "./output/raw"
config.postgres                              // Option<PostgresConfig>
config.opensearch                            // Option<OpenSearchConfig>
config.embedding                             // Option<EmbeddingConfig>
config.ontology                              // Option<OntologyConfig>
```

## Data Types Quick Reference

```rust
use ntimes::crawler::*;

// News categories
NewsCategory::Politics      // "100"
NewsCategory::Economy       // "101"
NewsCategory::Society       // "102"
NewsCategory::Culture       // "103"
NewsCategory::World         // "104"
NewsCategory::IT            // "105"
NewsCategory::Entertainment // "106"
NewsCategory::Sports        // "107"

// Parsed article
ParsedArticle {
    oid: String,                    // Publisher ID
    aid: String,                    // Article ID
    title: String,
    content: String,
    url: String,
    category: NewsCategory,
    published_at: DateTime<Utc>,
    crawled_at: DateTime<Utc>,
    // ... 13 more fields
}

// Comment (recursive)
Comment {
    comment_id: String,
    parent_id: Option<String>,
    article_id: String,
    replies: Vec<Comment>,          // Recursive
    depth: u32,
    is_deleted: bool,
    // ... 8 more fields
}

// Crawl state (checkpointing)
CrawlState {
    session_id: String,
    category: NewsCategory,
    articles_crawled: usize,
    articles_failed: usize,
    articles_skipped: usize,
    status: CrawlStatus,            // Running, Paused, Completed, Failed
    stats: CrawlStatistics,
    // ... 7 more fields
}
```

## Common Patterns

### Error Handling

```rust
use ntimes::utils::{CrawlerError, CrawlerResult};

fn fetch_url(url: &str) -> CrawlerResult<String> {
    if url.is_empty() {
        return Err(CrawlerError::InvalidUrl {
            url: url.to_string(),
            reason: "URL is empty".to_string(),
        });
    }
    
    // ... HTTP request
    
    Ok(response)
}

// Convert to AppError
let result: AppResult<String> = fetch_url(url)
    .map_err(|e| e.into());
```

### Configuration with Environment Variables

```toml
# In config.toml
password = "${DB_PASSWORD}"
api_key = "${LLM_API_KEY:-default_value}"
```

```bash
# Set environment variables
export DB_PASSWORD="secret123"
export LLM_API_KEY="sk-..."
```

### Working with Articles

```rust
use ntimes::crawler::{ParsedArticle, NewsCategory};

let mut article = ParsedArticle {
    oid: "001".to_string(),
    aid: "12345".to_string(),
    title: "Breaking News".to_string(),
    content: "Article content...".to_string(),
    category: NewsCategory::Politics,
    ..Default::default()
};

// Generate hash for deduplication
article.generate_hash();

// Get unique ID
let id = article.id();  // "001_12345"

// Get date path for file organization
let path = article.date_path();  // "2024/12/14"
```

### Crawl State Management

```rust
use ntimes::crawler::{CrawlState, NewsCategory, CrawlStatus};

// Initialize
let mut state = CrawlState::new(
    "session_20241214_001".to_string(),
    NewsCategory::Politics
);
state.total_articles = Some(100);

// Update progress
state.mark_success("001_12345".to_string());
state.mark_failed("001_12346".to_string());
state.mark_skipped();

// Check progress
let progress = state.completion_percentage();  // 3.0%
let rate = state.crawl_rate();                // articles/min

// Serialize for checkpoint
let json = serde_json::to_string(&state)?;
```

## Configuration Sections

| Section | Optional | Purpose |
|---------|----------|---------|
| `[app]` | No | Application metadata |
| `[crawler]` | No | HTTP client settings |
| `[crawler.comments]` | No | Comment API settings |
| `[storage]` | No | File system settings |
| `[postgres]` | Yes | PostgreSQL connection |
| `[opensearch]` | Yes | Vector search |
| `[embedding]` | Yes | ML model settings |
| `[ontology]` | Yes | LLM extraction |
| `[logging]` | No | Logging configuration |

## Environment Variables

```bash
# Required for optional features
export DB_PASSWORD="your_password"
export OPENSEARCH_PASSWORD="your_password"
export LLM_API_KEY="your_api_key"

# Test configuration
cargo test --lib config::settings::tests::test_env_var_expansion
```

## Key Methods

### NewsCategory
- `code()` → &str: Naver category code ("100"-"107")
- `korean_name()` → &str: Korean name
- `from_code(code)` → Option<NewsCategory>
- `from_str(s)` → Option<NewsCategory>
- `all()` → Vec<NewsCategory>

### ParsedArticle
- `id()` → String: Unique ID (oid_aid)
- `generate_hash()`: SHA-256 hash
- `date()` → String: YYYY-MM-DD
- `date_path()` → String: YYYY/MM/DD
- `title_slug()` → String: URL-safe title
- `has_comments()` → bool

### Comment
- `new(...)` → Self: Create root comment
- `is_visible()` → bool: !deleted && !hidden
- `total_replies()` → usize: Recursive count
- `add_reply(comment)`: Add child comment

### CrawlState
- `new(...)` → Self: Initialize session
- `mark_success(id)`: Increment crawled
- `mark_failed(id)`: Increment failed
- `mark_skipped()`: Increment skipped
- `completion_percentage()` → f64
- `crawl_rate()` → f64: Articles/minute

## Testing

```bash
# Test all modules
cargo test

# Test specific module
cargo test --lib utils::error::tests
cargo test --lib config::settings::tests
cargo test --lib crawler::types::tests

# Test with output
cargo test -- --nocapture

# Test specific function
cargo test test_env_var_expansion
```

---

**Quick Start**: See `ARCHITECTURE.md` for detailed documentation
**Sprint Plan**: `/home/mare/nTimes/sprint_plan.md`
**Development Spec**: `/home/mare/nTimes/development_spec.md`
