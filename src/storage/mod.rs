//! Storage operations for SQLite, PostgreSQL, and Markdown files
//!
//! This module handles data persistence:
//! - SQLite for crawl metadata and deduplication
//! - PostgreSQL for raw article data (optional)
//! - Async PostgreSQL deduplication for distributed crawling
//! - Markdown files for article output
//! - Checkpointing for resumable crawls

pub mod checkpoint;
pub mod dedup;
pub mod markdown;

pub use checkpoint::{
    AsyncCheckpointManager, CheckpointManager, CheckpointStats, ConcurrencyConfig,
    ConcurrencyMonitor, CrawlState, FailedUrl,
};
pub use dedup::{
    create_shared_checker, AsyncDedupChecker, DedupCheckResult, DedupConfig, DedupRecord,
    DedupStats, PoolStatus, SharedDedupChecker,
};
pub use markdown::{
    ArticleStorage, ArticleWithCommentsData, ArticleWithCommentsWriter, BatchSaveResult,
    CommentRenderConfig, CommentRenderer, MarkdownWriter,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod, Runtime};
use sha2::{Digest, Sha256};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use tokio_postgres::NoTls;

use crate::config::DatabaseConfig;
use crate::models::ParsedArticle;
use crate::parser::Article;

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

/// Crawl status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrawlStatus {
    Success,
    Failed,
    Skipped,
}

impl CrawlStatus {
    fn as_str(&self) -> &'static str {
        match self {
            CrawlStatus::Success => "success",
            CrawlStatus::Failed => "failed",
            CrawlStatus::Skipped => "skipped",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "success" => CrawlStatus::Success,
            "failed" => CrawlStatus::Failed,
            "skipped" => CrawlStatus::Skipped,
            _ => CrawlStatus::Failed,
        }
    }
}

/// Database management wrapper
pub struct Database {
    /// SQLite connection for metadata
    sqlite: Option<Connection>,

    /// PostgreSQL connection pool
    postgres: Option<Pool>,
}

impl Database {
    /// Create a new database instance
    pub fn new(_config: &DatabaseConfig) -> Result<Self> {
        Ok(Self {
            sqlite: None,
            postgres: None,
        })
    }

    /// Initialize SQLite connection
    pub fn init_sqlite(&mut self, path: &Path) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path).context("Failed to open SQLite database")?;

        // Enable WAL mode for better concurrency
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        self.create_sqlite_schema(&conn)?;
        self.sqlite = Some(conn);

        tracing::info!(path = %path.display(), "SQLite database initialized");
        Ok(())
    }

    /// Initialize PostgreSQL connection pool
    pub async fn init_postgres(&mut self, url: &str) -> Result<()> {
        let mut cfg = PoolConfig::new();
        cfg.url = Some(url.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .context("Failed to create PostgreSQL connection pool")?;

        self.postgres = Some(pool);

        Ok(())
    }

    /// Create SQLite schema
    fn create_sqlite_schema(&self, conn: &Connection) -> Result<()> {
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

    /// Create PostgreSQL schema for articles
    async fn create_postgres_schema(&self) -> Result<()> {
        let pool = self
            .postgres
            .as_ref()
            .context("PostgreSQL not initialized")?;

        let client = pool.get().await.context("Failed to get connection")?;

        client
            .execute(
                r#"
                CREATE TABLE IF NOT EXISTS articles (
                    id UUID PRIMARY KEY,
                    url TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL,
                    body TEXT NOT NULL,
                    author TEXT,
                    published_at TIMESTAMPTZ,
                    category TEXT,
                    content_hash TEXT NOT NULL,
                    comments JSONB NOT NULL DEFAULT '[]',
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                );

                CREATE INDEX IF NOT EXISTS idx_articles_url ON articles(url);
                CREATE INDEX IF NOT EXISTS idx_articles_content_hash ON articles(content_hash);
                CREATE INDEX IF NOT EXISTS idx_articles_published_at ON articles(published_at);
                CREATE INDEX IF NOT EXISTS idx_articles_category ON articles(category);
                "#,
                &[],
            )
            .await
            .context("Failed to create PostgreSQL schema")?;

        tracing::info!("PostgreSQL articles schema initialized");
        Ok(())
    }

    /// Store article in PostgreSQL
    pub async fn store_article(&self, article: &Article) -> Result<()> {
        let pool = self
            .postgres
            .as_ref()
            .context("PostgreSQL not initialized")?;

        // Ensure schema exists
        self.create_postgres_schema().await?;

        let client = pool.get().await.context("Failed to get connection")?;

        // Serialize comments to JSON
        let comments_json =
            serde_json::to_value(&article.comments).context("Failed to serialize comments")?;

        // Insert or update article
        client
            .execute(
                r#"
                INSERT INTO articles (
                    id, url, title, body, author, published_at, category, content_hash, comments
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (url) DO UPDATE SET
                    title = EXCLUDED.title,
                    body = EXCLUDED.body,
                    author = EXCLUDED.author,
                    published_at = EXCLUDED.published_at,
                    category = EXCLUDED.category,
                    content_hash = EXCLUDED.content_hash,
                    comments = EXCLUDED.comments,
                    updated_at = NOW()
                "#,
                &[
                    &article.id,
                    &article.url,
                    &article.title,
                    &article.body,
                    &article.author,
                    &article.published_at,
                    &article.category,
                    &article.content_hash,
                    &comments_json,
                ],
            )
            .await
            .context("Failed to store article")?;

        tracing::debug!(article_id = %article.id, url = %article.url, "Article stored");
        Ok(())
    }

    /// Retrieve article by ID
    pub async fn get_article(&self, id: &str) -> Result<Option<Article>> {
        let pool = self
            .postgres
            .as_ref()
            .context("PostgreSQL not initialized")?;

        let client = pool.get().await.context("Failed to get connection")?;

        // Parse UUID
        let article_id = uuid::Uuid::parse_str(id).context("Invalid UUID format")?;

        // Query article
        let row = client
            .query_opt(
                r#"
                SELECT id, url, title, body, author, published_at, category, content_hash, comments
                FROM articles
                WHERE id = $1
                "#,
                &[&article_id],
            )
            .await
            .context("Failed to query article")?;

        match row {
            Some(row) => {
                let comments_json: serde_json::Value = row.get(8);
                let comments: Vec<crate::parser::Comment> =
                    serde_json::from_value(comments_json)
                        .context("Failed to deserialize comments")?;

                let article = Article {
                    id: row.get(0),
                    url: row.get(1),
                    title: row.get(2),
                    body: row.get(3),
                    author: row.get(4),
                    published_at: row.get(5),
                    category: row.get(6),
                    content_hash: row.get(7),
                    comments,
                };

                tracing::debug!(article_id = %article.id, "Article retrieved");
                Ok(Some(article))
            }
            None => Ok(None),
        }
    }

    /// Check if URL has been crawled
    ///
    /// # Arguments
    /// * `url` - URL to check
    ///
    /// # Returns
    /// True if URL has been successfully crawled
    pub fn is_url_crawled(&self, url: &str) -> Result<bool> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM crawl_metadata WHERE url = ?1 AND status = 'success')",
                params![url],
                |row| row.get(0),
            )
            .context("Failed to check URL")?;

        Ok(exists)
    }

    /// Check if content hash exists (for deduplication)
    ///
    /// # Arguments
    /// * `hash` - Content hash to check
    ///
    /// # Returns
    /// True if content with this hash already exists
    pub fn is_content_duplicate(&self, hash: &str) -> Result<bool> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM crawl_metadata WHERE content_hash = ?1)",
                params![hash],
                |row| row.get(0),
            )
            .context("Failed to check content hash")?;

        Ok(exists)
    }

    /// Mark URL as crawled
    ///
    /// # Arguments
    /// * `id` - Article ID (oid_aid)
    /// * `url` - Article URL
    /// * `content_hash` - SHA256 hash of content
    /// * `status` - Crawl status
    /// * `error_message` - Optional error message for failed crawls
    pub fn mark_url_crawled(
        &self,
        id: &str,
        url: &str,
        content_hash: &str,
        status: CrawlStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

        let now = Utc::now().to_rfc3339();

        // Use URL hash as fallback ID if empty (for failures)
        let effective_id = if id.is_empty() {
            let hash = Sha256::digest(url.as_bytes());
            format!("fail_{:x}", hash).chars().take(40).collect::<String>()
        } else {
            id.to_string()
        };

        // Use INSERT OR REPLACE to handle both id and url conflicts
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

    /// Record successful crawl
    pub fn record_success(&self, article: &ParsedArticle) -> Result<()> {
        let hash = article.content_hash.as_deref().unwrap_or("");
        self.mark_url_crawled(
            &article.id(),
            &article.url,
            hash,
            CrawlStatus::Success,
            None,
        )
    }

    /// Record failed crawl
    pub fn record_failure(&self, url: &str, error: &str) -> Result<()> {
        self.mark_url_crawled("", url, "", CrawlStatus::Failed, Some(error))
    }

    /// Get crawl record by URL
    pub fn get_crawl_record(&self, url: &str) -> Result<Option<CrawlRecord>> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

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
                        status: CrawlStatus::from_str(&row.get::<_, String>(4)?),
                        error_message: row.get(5)?,
                    })
                },
            )
            .optional()
            .context("Failed to get crawl record")?;

        Ok(record)
    }

    /// Get crawl statistics
    pub fn get_stats(&self) -> Result<CrawlStats> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

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

    /// Save checkpoint state
    pub fn save_checkpoint(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

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

    /// Load checkpoint state
    pub fn load_checkpoint(&self, key: &str) -> Result<Option<String>> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

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

    /// Filter URLs that haven't been crawled
    ///
    /// Uses batch query with `WHERE url IN (...)` for O(1) database round trips
    /// instead of O(n) individual queries.
    pub fn filter_uncrawled(&self, urls: &[String]) -> Result<Vec<String>> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

        if urls.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite has a limit on number of parameters (default 999)
        // Process in chunks to avoid hitting this limit
        const CHUNK_SIZE: usize = 500;

        let mut uncrawled = Vec::new();

        for chunk in urls.chunks(CHUNK_SIZE) {
            let crawled_in_chunk = self.get_crawled_urls_batch(conn, chunk)?;
            for url in chunk {
                if !crawled_in_chunk.contains(url) {
                    uncrawled.push(url.clone());
                }
            }
        }

        Ok(uncrawled)
    }

    /// Batch check URLs for crawl status
    ///
    /// Uses batch query with `WHERE url IN (...)` for O(1) database round trips
    /// instead of O(n) individual queries.
    pub fn batch_check_urls(&self, urls: &[String]) -> Result<Vec<(String, bool)>> {
        let conn = self.sqlite.as_ref().context("SQLite not initialized")?;

        if urls.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite has a limit on number of parameters (default 999)
        // Process in chunks to avoid hitting this limit
        const CHUNK_SIZE: usize = 500;

        let mut results = Vec::with_capacity(urls.len());

        for chunk in urls.chunks(CHUNK_SIZE) {
            let crawled_in_chunk = self.get_crawled_urls_batch(conn, chunk)?;
            for url in chunk {
                results.push((url.clone(), crawled_in_chunk.contains(url)));
            }
        }

        Ok(results)
    }

    /// Get set of URLs that have been successfully crawled from a batch
    ///
    /// Internal helper for batch URL operations. Uses a single query with
    /// `WHERE url IN (...)` clause for efficiency.
    fn get_crawled_urls_batch(
        &self,
        conn: &Connection,
        urls: &[String],
    ) -> Result<std::collections::HashSet<String>> {
        use std::collections::HashSet;

        if urls.is_empty() {
            return Ok(HashSet::new());
        }

        // Build parameterized query: WHERE url IN (?, ?, ?, ...)
        let placeholders: String = urls.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT url FROM crawl_metadata WHERE url IN ({placeholders}) AND status = 'success'"
        );

        let mut stmt = conn.prepare(&query).context("Failed to prepare batch query")?;

        // Bind all URL parameters
        let params: Vec<&dyn rusqlite::ToSql> = urls.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let crawled_urls: HashSet<String> = stmt
            .query_map(params.as_slice(), |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(crawled_urls)
    }
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
    /// Success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.success as f64 / self.total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_db() -> (Database, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let config = DatabaseConfig {
            sqlite_path: temp_file.path().to_path_buf(),
            postgres_url: String::from("postgresql://localhost/test"),
            pool_size: 5,
        };

        let mut db = Database::new(&config).unwrap();
        db.init_sqlite(temp_file.path()).unwrap();
        (db, temp_file)
    }

    #[test]
    fn test_sqlite_initialization() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = DatabaseConfig {
            sqlite_path: temp_file.path().to_path_buf(),
            postgres_url: String::from("postgresql://localhost/test"),
            pool_size: 5,
        };

        let mut db = Database::new(&config).unwrap();
        assert!(db.init_sqlite(temp_file.path()).is_ok());
    }

    #[test]
    fn test_is_url_crawled() {
        let (db, _temp) = create_test_db();

        // URL should not be crawled initially
        assert!(!db.is_url_crawled("https://example.com/article1").unwrap());

        // Mark as crawled
        db.mark_url_crawled(
            "001_001",
            "https://example.com/article1",
            "hash123",
            CrawlStatus::Success,
            None,
        )
        .unwrap();

        // Now should be crawled
        assert!(db.is_url_crawled("https://example.com/article1").unwrap());
    }

    #[test]
    fn test_is_content_duplicate() {
        let (db, _temp) = create_test_db();

        // Hash should not exist initially
        assert!(!db.is_content_duplicate("unique_hash").unwrap());

        // Add a record with this hash
        db.mark_url_crawled(
            "001_001",
            "https://example.com/article1",
            "unique_hash",
            CrawlStatus::Success,
            None,
        )
        .unwrap();

        // Now hash should exist
        assert!(db.is_content_duplicate("unique_hash").unwrap());
    }

    #[test]
    fn test_record_success() {
        let (db, _temp) = create_test_db();

        let article = ParsedArticle {
            oid: "001".to_string(),
            aid: "0001".to_string(),
            url: "https://example.com/article".to_string(),
            content_hash: Some("hash123".to_string()),
            ..Default::default()
        };

        db.record_success(&article).unwrap();
        assert!(db.is_url_crawled(&article.url).unwrap());
    }

    #[test]
    fn test_record_failure() {
        let (db, _temp) = create_test_db();

        db.record_failure("https://example.com/failed", "Connection timeout")
            .unwrap();

        let record = db
            .get_crawl_record("https://example.com/failed")
            .unwrap()
            .unwrap();
        assert_eq!(record.status, CrawlStatus::Failed);
        assert_eq!(record.error_message, Some("Connection timeout".to_string()));
    }

    #[test]
    fn test_get_stats() {
        let (db, _temp) = create_test_db();

        // Add some records
        db.mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
            .unwrap();
        db.mark_url_crawled("2", "url2", "h2", CrawlStatus::Success, None)
            .unwrap();
        db.mark_url_crawled("3", "url3", "h3", CrawlStatus::Failed, Some("error"))
            .unwrap();

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.success, 2);
        assert_eq!(stats.failed, 1);
    }

    #[test]
    fn test_checkpoint() {
        let (db, _temp) = create_test_db();

        // Save checkpoint
        db.save_checkpoint("last_page", "5").unwrap();

        // Load checkpoint
        let value = db.load_checkpoint("last_page").unwrap();
        assert_eq!(value, Some("5".to_string()));

        // Non-existent key
        let missing = db.load_checkpoint("missing_key").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_filter_uncrawled() {
        let (db, _temp) = create_test_db();

        // Mark one URL as crawled
        db.mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
            .unwrap();

        let urls = vec!["url1".to_string(), "url2".to_string(), "url3".to_string()];

        let uncrawled = db.filter_uncrawled(&urls).unwrap();
        assert_eq!(uncrawled.len(), 2);
        assert!(uncrawled.contains(&"url2".to_string()));
        assert!(uncrawled.contains(&"url3".to_string()));
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
    fn test_filter_uncrawled_empty() {
        let (db, _temp) = create_test_db();

        let urls: Vec<String> = vec![];
        let uncrawled = db.filter_uncrawled(&urls).unwrap();
        assert!(uncrawled.is_empty());
    }

    #[test]
    fn test_batch_check_urls() {
        let (db, _temp) = create_test_db();

        // Mark some URLs as crawled
        db.mark_url_crawled("1", "url1", "h1", CrawlStatus::Success, None)
            .unwrap();
        db.mark_url_crawled("2", "url3", "h3", CrawlStatus::Success, None)
            .unwrap();

        let urls = vec![
            "url1".to_string(),
            "url2".to_string(),
            "url3".to_string(),
            "url4".to_string(),
        ];

        let results = db.batch_check_urls(&urls).unwrap();

        assert_eq!(results.len(), 4);
        assert_eq!(results[0], ("url1".to_string(), true));
        assert_eq!(results[1], ("url2".to_string(), false));
        assert_eq!(results[2], ("url3".to_string(), true));
        assert_eq!(results[3], ("url4".to_string(), false));
    }

    #[test]
    fn test_batch_check_urls_empty() {
        let (db, _temp) = create_test_db();

        let urls: Vec<String> = vec![];
        let results = db.batch_check_urls(&urls).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_filter_uncrawled_large_batch() {
        let (db, _temp) = create_test_db();

        // Create a large batch of URLs (more than CHUNK_SIZE of 500)
        let urls: Vec<String> = (0..600).map(|i| format!("url{}", i)).collect();

        // Mark every 10th URL as crawled
        for i in (0..600).step_by(10) {
            db.mark_url_crawled(
                &format!("{}", i),
                &format!("url{}", i),
                &format!("h{}", i),
                CrawlStatus::Success,
                None,
            )
            .unwrap();
        }

        let uncrawled = db.filter_uncrawled(&urls).unwrap();

        // Should have 540 uncrawled (600 - 60 crawled)
        assert_eq!(uncrawled.len(), 540);

        // Verify some specific ones
        assert!(!uncrawled.contains(&"url0".to_string())); // crawled
        assert!(uncrawled.contains(&"url1".to_string())); // not crawled
        assert!(!uncrawled.contains(&"url10".to_string())); // crawled
        assert!(uncrawled.contains(&"url11".to_string())); // not crawled
    }
}
