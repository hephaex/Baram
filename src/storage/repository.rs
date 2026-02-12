//! Repository Pattern for Database Abstraction
//!
//! This module provides trait-based repository abstractions to decouple
//! business logic from storage implementations, enabling:
//! - Easy testing with mock implementations
//! - Swappable storage backends (SQLite, PostgreSQL, etc.)
//! - Clear separation of concerns
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     Business Logic                          │
//! │              (crawler, ontology, etc.)                      │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Repository Traits                         │
//! │  ArticleRepository, CrawlMetadataRepository, etc.           │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!          ┌───────────────────┼───────────────────┐
//!          ▼                   ▼                   ▼
//! ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
//! │     SQLite      │ │   PostgreSQL    │ │      Mock       │
//! │  Implementation │ │ Implementation  │ │ Implementation  │
//! └─────────────────┘ └─────────────────┘ └─────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use baram::storage::repository::{CrawlMetadataRepository, SqliteCrawlMetadataRepository};
//!
//! // Production: use SQLite
//! let repo = SqliteCrawlMetadataRepository::new("crawl.db")?;
//!
//! // Testing: use Mock
//! let mock_repo = MockCrawlMetadataRepository::new();
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::models::ParsedArticle;
use crate::parser::Article;

// ============================================================================
// Core Types
// ============================================================================

/// Crawl status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrawlStatus {
    Success,
    Failed,
    Skipped,
}

impl CrawlStatus {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            CrawlStatus::Success => "success",
            CrawlStatus::Failed => "failed",
            CrawlStatus::Skipped => "skipped",
        }
    }
}

impl std::str::FromStr for CrawlStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "success" => CrawlStatus::Success,
            "failed" => CrawlStatus::Failed,
            "skipped" => CrawlStatus::Skipped,
            _ => CrawlStatus::Failed,
        })
    }
}

/// Crawl metadata record
#[derive(Debug, Clone)]
pub struct CrawlRecord {
    pub id: String,
    pub url: String,
    pub content_hash: String,
    pub crawled_at: DateTime<Utc>,
    pub status: CrawlStatus,
    pub error_message: Option<String>,
}

/// Crawl statistics
#[derive(Debug, Clone, Default)]
pub struct CrawlStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl CrawlStats {
    /// Calculate success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.success as f64 / self.total as f64
    }
}

// ============================================================================
// Repository Traits
// ============================================================================

/// Repository for crawl metadata operations
///
/// Handles URL deduplication, crawl status tracking, and statistics.
pub trait CrawlMetadataRepository: Send + Sync {
    /// Check if a URL has been successfully crawled
    fn is_url_crawled(&self, url: &str) -> Result<bool>;

    /// Check if content with given hash already exists
    fn is_content_duplicate(&self, hash: &str) -> Result<bool>;

    /// Mark a URL as crawled with given status
    fn mark_url_crawled(
        &self,
        id: &str,
        url: &str,
        content_hash: &str,
        status: CrawlStatus,
        error_message: Option<&str>,
    ) -> Result<()>;

    /// Record a successful crawl
    fn record_success(&self, article: &ParsedArticle) -> Result<()> {
        let hash = article.content_hash.as_deref().unwrap_or("");
        self.mark_url_crawled(
            &article.id(),
            &article.url,
            hash,
            CrawlStatus::Success,
            None,
        )
    }

    /// Record a failed crawl
    fn record_failure(&self, url: &str, error: &str) -> Result<()> {
        self.mark_url_crawled("", url, "", CrawlStatus::Failed, Some(error))
    }

    /// Get crawl record by URL
    fn get_crawl_record(&self, url: &str) -> Result<Option<CrawlRecord>>;

    /// Get crawl statistics
    fn get_stats(&self) -> Result<CrawlStats>;

    /// Filter URLs that haven't been crawled
    fn filter_uncrawled(&self, urls: &[String]) -> Result<Vec<String>>;

    /// Batch check URLs for crawl status
    fn batch_check_urls(&self, urls: &[String]) -> Result<Vec<(String, bool)>>;

    /// Save checkpoint state
    fn save_checkpoint(&self, key: &str, value: &str) -> Result<()>;

    /// Load checkpoint state
    fn load_checkpoint(&self, key: &str) -> Result<Option<String>>;
}

/// Repository for article storage operations (async)
#[async_trait]
pub trait ArticleRepository: Send + Sync {
    /// Store an article
    async fn store(&self, article: &Article) -> Result<()>;

    /// Get article by ID
    async fn get_by_id(&self, id: &str) -> Result<Option<Article>>;

    /// Get article by URL
    async fn get_by_url(&self, url: &str) -> Result<Option<Article>>;

    /// Check if article exists
    async fn exists(&self, id: &str) -> Result<bool>;

    /// Delete article by ID
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Count total articles
    async fn count(&self) -> Result<usize>;
}

// ============================================================================
// SQLite Implementation
// ============================================================================

/// SQLite implementation of CrawlMetadataRepository
///
/// Uses `Mutex` to ensure thread-safety for the SQLite connection.
pub struct SqliteCrawlMetadataRepository {
    conn: Mutex<Connection>,
}

impl SqliteCrawlMetadataRepository {
    /// Create a new SQLite repository
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path).context("Failed to open SQLite database")?;

        // Enable WAL mode for better concurrency
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        let repo = Self {
            conn: Mutex::new(conn),
        };
        repo.create_schema()?;

        tracing::info!(path = %path.display(), "SQLite repository initialized");
        Ok(repo)
    }

    /// Create in-memory repository (for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("Failed to create in-memory SQLite")?;
        let repo = Self {
            conn: Mutex::new(conn),
        };
        repo.create_schema()?;
        Ok(repo)
    }

    /// Create database schema
    fn create_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
                CREATE TABLE IF NOT EXISTS crawl_metadata (
                    id TEXT PRIMARY KEY,
                    url TEXT NOT NULL UNIQUE,
                    content_hash TEXT NOT NULL,
                    crawled_at TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'success',
                    error_message TEXT
                );

                CREATE INDEX IF NOT EXISTS idx_crawl_metadata_url
                    ON crawl_metadata(url);

                CREATE INDEX IF NOT EXISTS idx_crawl_metadata_status
                    ON crawl_metadata(status);

                CREATE INDEX IF NOT EXISTS idx_crawl_metadata_hash
                    ON crawl_metadata(content_hash);

                CREATE TABLE IF NOT EXISTS crawl_state (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                "#,
        )
        .context("Failed to create SQLite schema")?;

        Ok(())
    }

    /// Get crawled URLs from a batch (internal helper)
    fn get_crawled_urls_batch(
        &self,
        conn: &Connection,
        urls: &[String],
    ) -> Result<std::collections::HashSet<String>> {
        use std::collections::HashSet;

        if urls.is_empty() {
            return Ok(HashSet::new());
        }

        let placeholders: String = urls.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT url FROM crawl_metadata WHERE url IN ({placeholders}) AND status = 'success'"
        );

        let mut stmt = conn
            .prepare(&query)
            .context("Failed to prepare batch query")?;

        let params: Vec<&dyn rusqlite::ToSql> =
            urls.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let crawled_urls: HashSet<String> = stmt
            .query_map(params.as_slice(), |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(crawled_urls)
    }
}

impl CrawlMetadataRepository for SqliteCrawlMetadataRepository {
    fn is_url_crawled(&self, url: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM crawl_metadata WHERE url = ?1 AND status = 'success')",
                params![url],
                |row| row.get(0),
            )
            .context("Failed to check URL")?;

        Ok(exists)
    }

    fn is_content_duplicate(&self, hash: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM crawl_metadata WHERE content_hash = ?1)",
                params![hash],
                |row| row.get(0),
            )
            .context("Failed to check content hash")?;

        Ok(exists)
    }

    fn mark_url_crawled(
        &self,
        id: &str,
        url: &str,
        content_hash: &str,
        status: CrawlStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        // Use URL hash as fallback ID if empty
        let effective_id = if id.is_empty() {
            let hash = Sha256::digest(url.as_bytes());
            format!("fail_{hash:x}")
                .chars()
                .take(40)
                .collect::<String>()
        } else {
            id.to_string()
        };

        conn.execute(
            r#"
                INSERT OR REPLACE INTO crawl_metadata (id, url, content_hash, crawled_at, status, error_message)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
            params![effective_id, url, content_hash, now, status.as_str(), error_message],
        )
        .context("Failed to mark URL as crawled")?;

        Ok(())
    }

    fn get_crawl_record(&self, url: &str) -> Result<Option<CrawlRecord>> {
        let conn = self.conn.lock().unwrap();
        let record = conn
            .query_row(
                "SELECT id, url, content_hash, crawled_at, status, error_message
                 FROM crawl_metadata WHERE url = ?1",
                params![url],
                |row| {
                    Ok(CrawlRecord {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        content_hash: row.get(2)?,
                        crawled_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        status: row
                            .get::<_, String>(4)?
                            .parse()
                            .unwrap_or(CrawlStatus::Failed),
                        error_message: row.get(5)?,
                    })
                },
            )
            .optional()
            .context("Failed to get crawl record")?;

        Ok(record)
    }

    fn get_stats(&self) -> Result<CrawlStats> {
        let conn = self.conn.lock().unwrap();
        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM crawl_metadata", [], |row| row.get(0))?;

        let success: i64 = conn.query_row(
            "SELECT COUNT(*) FROM crawl_metadata WHERE status = 'success'",
            [],
            |row| row.get(0),
        )?;

        let failed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM crawl_metadata WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )?;

        let skipped: i64 = conn.query_row(
            "SELECT COUNT(*) FROM crawl_metadata WHERE status = 'skipped'",
            [],
            |row| row.get(0),
        )?;

        Ok(CrawlStats {
            total: total as usize,
            success: success as usize,
            failed: failed as usize,
            skipped: skipped as usize,
        })
    }

    fn filter_uncrawled(&self, urls: &[String]) -> Result<Vec<String>> {
        if urls.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().unwrap();
        const CHUNK_SIZE: usize = 500;
        let mut uncrawled = Vec::new();

        for chunk in urls.chunks(CHUNK_SIZE) {
            let crawled_in_chunk = self.get_crawled_urls_batch(&conn, chunk)?;
            for url in chunk {
                if !crawled_in_chunk.contains(url) {
                    uncrawled.push(url.clone());
                }
            }
        }

        Ok(uncrawled)
    }

    fn batch_check_urls(&self, urls: &[String]) -> Result<Vec<(String, bool)>> {
        if urls.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().unwrap();
        const CHUNK_SIZE: usize = 500;
        let mut results = Vec::with_capacity(urls.len());

        for chunk in urls.chunks(CHUNK_SIZE) {
            let crawled_in_chunk = self.get_crawled_urls_batch(&conn, chunk)?;
            for url in chunk {
                results.push((url.clone(), crawled_in_chunk.contains(url)));
            }
        }

        Ok(results)
    }

    fn save_checkpoint(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            r#"
                INSERT INTO crawl_state (key, value, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = excluded.updated_at
                "#,
            params![key, value, now],
        )
        .context("Failed to save checkpoint")?;

        Ok(())
    }

    fn load_checkpoint(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let value = conn
            .query_row(
                "SELECT value FROM crawl_state WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to load checkpoint")?;

        Ok(value)
    }
}

// ============================================================================
// Mock Implementation (for testing)
// ============================================================================

/// In-memory mock implementation of CrawlMetadataRepository
///
/// Useful for testing without database dependencies.
pub struct MockCrawlMetadataRepository {
    records: RwLock<HashMap<String, CrawlRecord>>,
    checkpoints: RwLock<HashMap<String, String>>,
}

impl MockCrawlMetadataRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            checkpoints: RwLock::new(HashMap::new()),
        }
    }

    /// Get the number of records
    pub fn len(&self) -> usize {
        self.records.read().unwrap().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.records.read().unwrap().is_empty()
    }

    /// Clear all records
    pub fn clear(&self) {
        self.records.write().unwrap().clear();
        self.checkpoints.write().unwrap().clear();
    }
}

impl Default for MockCrawlMetadataRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl CrawlMetadataRepository for MockCrawlMetadataRepository {
    fn is_url_crawled(&self, url: &str) -> Result<bool> {
        let records = self.records.read().unwrap();
        Ok(records
            .get(url)
            .map(|r| r.status == CrawlStatus::Success)
            .unwrap_or(false))
    }

    fn is_content_duplicate(&self, hash: &str) -> Result<bool> {
        let records = self.records.read().unwrap();
        Ok(records.values().any(|r| r.content_hash == hash))
    }

    fn mark_url_crawled(
        &self,
        id: &str,
        url: &str,
        content_hash: &str,
        status: CrawlStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let effective_id = if id.is_empty() {
            let hash = Sha256::digest(url.as_bytes());
            format!("fail_{hash:x}")
                .chars()
                .take(40)
                .collect::<String>()
        } else {
            id.to_string()
        };

        let record = CrawlRecord {
            id: effective_id,
            url: url.to_string(),
            content_hash: content_hash.to_string(),
            crawled_at: Utc::now(),
            status,
            error_message: error_message.map(String::from),
        };

        let mut records = self.records.write().unwrap();
        records.insert(url.to_string(), record);
        Ok(())
    }

    fn get_crawl_record(&self, url: &str) -> Result<Option<CrawlRecord>> {
        let records = self.records.read().unwrap();
        Ok(records.get(url).cloned())
    }

    fn get_stats(&self) -> Result<CrawlStats> {
        let records = self.records.read().unwrap();
        let mut stats = CrawlStats::default();

        for record in records.values() {
            stats.total += 1;
            match record.status {
                CrawlStatus::Success => stats.success += 1,
                CrawlStatus::Failed => stats.failed += 1,
                CrawlStatus::Skipped => stats.skipped += 1,
            }
        }

        Ok(stats)
    }

    fn filter_uncrawled(&self, urls: &[String]) -> Result<Vec<String>> {
        let records = self.records.read().unwrap();
        Ok(urls
            .iter()
            .filter(|url| {
                !records
                    .get(*url)
                    .map(|r| r.status == CrawlStatus::Success)
                    .unwrap_or(false)
            })
            .cloned()
            .collect())
    }

    fn batch_check_urls(&self, urls: &[String]) -> Result<Vec<(String, bool)>> {
        let records = self.records.read().unwrap();
        Ok(urls
            .iter()
            .map(|url| {
                let crawled = records
                    .get(url)
                    .map(|r| r.status == CrawlStatus::Success)
                    .unwrap_or(false);
                (url.clone(), crawled)
            })
            .collect())
    }

    fn save_checkpoint(&self, key: &str, value: &str) -> Result<()> {
        let mut checkpoints = self.checkpoints.write().unwrap();
        checkpoints.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn load_checkpoint(&self, key: &str) -> Result<Option<String>> {
        let checkpoints = self.checkpoints.read().unwrap();
        Ok(checkpoints.get(key).cloned())
    }
}

// ============================================================================
// Shared Repository Types
// ============================================================================

/// Thread-safe shared repository wrapper
pub type SharedCrawlMetadataRepository = Arc<dyn CrawlMetadataRepository>;

/// Create a shared SQLite repository
pub fn create_sqlite_repository(path: impl AsRef<Path>) -> Result<SharedCrawlMetadataRepository> {
    let repo = SqliteCrawlMetadataRepository::new(path)?;
    Ok(Arc::new(repo))
}

/// Create a shared mock repository
pub fn create_mock_repository() -> SharedCrawlMetadataRepository {
    Arc::new(MockCrawlMetadataRepository::new())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test repositories
    fn create_test_repos() -> Vec<Box<dyn CrawlMetadataRepository>> {
        vec![
            Box::new(SqliteCrawlMetadataRepository::in_memory().unwrap()),
            Box::new(MockCrawlMetadataRepository::new()),
        ]
    }

    #[test]
    fn test_is_url_crawled() {
        for repo in create_test_repos() {
            assert!(!repo.is_url_crawled("https://example.com/1").unwrap());

            repo.mark_url_crawled(
                "001",
                "https://example.com/1",
                "hash1",
                CrawlStatus::Success,
                None,
            )
            .unwrap();

            assert!(repo.is_url_crawled("https://example.com/1").unwrap());
            assert!(!repo.is_url_crawled("https://example.com/2").unwrap());
        }
    }

    #[test]
    fn test_is_content_duplicate() {
        for repo in create_test_repos() {
            assert!(!repo.is_content_duplicate("unique_hash").unwrap());

            repo.mark_url_crawled(
                "001",
                "https://example.com/1",
                "unique_hash",
                CrawlStatus::Success,
                None,
            )
            .unwrap();

            assert!(repo.is_content_duplicate("unique_hash").unwrap());
        }
    }

    #[test]
    fn test_record_failure() {
        for repo in create_test_repos() {
            repo.record_failure("https://example.com/failed", "Connection timeout")
                .unwrap();

            let record = repo
                .get_crawl_record("https://example.com/failed")
                .unwrap()
                .unwrap();

            assert_eq!(record.status, CrawlStatus::Failed);
            assert_eq!(record.error_message, Some("Connection timeout".to_string()));
        }
    }

    #[test]
    fn test_get_stats() {
        for repo in create_test_repos() {
            repo.mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
                .unwrap();
            repo.mark_url_crawled("2", "url2", "h2", CrawlStatus::Success, None)
                .unwrap();
            repo.mark_url_crawled("3", "url3", "h3", CrawlStatus::Failed, Some("error"))
                .unwrap();

            let stats = repo.get_stats().unwrap();
            assert_eq!(stats.total, 3);
            assert_eq!(stats.success, 2);
            assert_eq!(stats.failed, 1);
        }
    }

    #[test]
    fn test_filter_uncrawled() {
        for repo in create_test_repos() {
            repo.mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
                .unwrap();

            let urls = vec!["url1".to_string(), "url2".to_string(), "url3".to_string()];
            let uncrawled = repo.filter_uncrawled(&urls).unwrap();

            assert_eq!(uncrawled.len(), 2);
            assert!(uncrawled.contains(&"url2".to_string()));
            assert!(uncrawled.contains(&"url3".to_string()));
        }
    }

    #[test]
    fn test_batch_check_urls() {
        for repo in create_test_repos() {
            repo.mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
                .unwrap();
            repo.mark_url_crawled("2", "url3", "h3", CrawlStatus::Success, None)
                .unwrap();

            let urls = vec![
                "url1".to_string(),
                "url2".to_string(),
                "url3".to_string(),
                "url4".to_string(),
            ];

            let results = repo.batch_check_urls(&urls).unwrap();

            assert_eq!(results.len(), 4);
            assert_eq!(results[0], ("url1".to_string(), true));
            assert_eq!(results[1], ("url2".to_string(), false));
            assert_eq!(results[2], ("url3".to_string(), true));
            assert_eq!(results[3], ("url4".to_string(), false));
        }
    }

    #[test]
    fn test_checkpoint() {
        for repo in create_test_repos() {
            repo.save_checkpoint("last_page", "5").unwrap();

            let value = repo.load_checkpoint("last_page").unwrap();
            assert_eq!(value, Some("5".to_string()));

            let missing = repo.load_checkpoint("missing_key").unwrap();
            assert!(missing.is_none());
        }
    }

    #[test]
    fn test_crawl_stats_success_rate() {
        let stats = CrawlStats {
            total: 100,
            success: 95,
            failed: 5,
            skipped: 0,
        };
        assert!((stats.success_rate() - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_mock_repository_utilities() {
        let mock = MockCrawlMetadataRepository::new();

        assert!(mock.is_empty());
        assert_eq!(mock.len(), 0);

        mock.mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
            .unwrap();

        assert!(!mock.is_empty());
        assert_eq!(mock.len(), 1);

        mock.clear();
        assert!(mock.is_empty());
    }

    #[test]
    fn test_shared_repository_creation() {
        let mock_repo = create_mock_repository();

        mock_repo
            .mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
            .unwrap();

        assert!(mock_repo.is_url_crawled("url1").unwrap());
    }
}
