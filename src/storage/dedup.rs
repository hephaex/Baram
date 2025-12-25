//! Async deduplication for distributed crawlers
//!
//! This module provides PostgreSQL-based deduplication for distributed crawling:
//! - Async article ID and URL deduplication
//! - Content hash checking to avoid duplicate content
//! - Batch operations for efficient network usage
//! - Connection pooling for high throughput

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::types::ToSql;
use tokio_postgres::NoTls;

// ============================================================================
// Configuration
// ============================================================================

/// Deduplication configuration
#[derive(Debug, Clone)]
pub struct DedupConfig {
    /// PostgreSQL connection URL
    pub database_url: String,

    /// Connection pool size
    pub pool_size: usize,

    /// Connection timeout
    pub connect_timeout: Duration,

    /// Statement timeout
    pub statement_timeout: Duration,

    /// Cache size for in-memory deduplication
    pub cache_size: usize,

    /// Cache TTL
    pub cache_ttl: Duration,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            database_url: "postgresql://localhost/ktime".to_string(),
            pool_size: 10,
            connect_timeout: Duration::from_secs(10),
            statement_timeout: Duration::from_secs(30),
            cache_size: 10000,
            cache_ttl: Duration::from_secs(3600),
        }
    }
}

impl DedupConfig {
    /// Create config from environment variables
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .or_else(|_| std::env::var("POSTGRES_URL"))
            .unwrap_or_else(|_| "postgresql://localhost/ktime".to_string());

        let pool_size = std::env::var("DB_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let cache_size = std::env::var("DEDUP_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10000);

        Ok(Self {
            database_url,
            pool_size,
            cache_size,
            ..Default::default()
        })
    }

    /// Set database URL
    pub fn with_database_url(mut self, url: &str) -> Self {
        self.database_url = url.to_string();
        self
    }

    /// Set pool size
    pub fn with_pool_size(mut self, size: usize) -> Self {
        self.pool_size = size;
        self
    }

    /// Set cache size
    pub fn with_cache_size(mut self, size: usize) -> Self {
        self.cache_size = size;
        self
    }
}

// ============================================================================
// Deduplication Record
// ============================================================================

/// Record of a crawled article for deduplication
#[derive(Debug, Clone)]
pub struct DedupRecord {
    /// Article ID (oid_aid format)
    pub article_id: String,

    /// Article URL
    pub url: String,

    /// SHA256 hash of content
    pub content_hash: String,

    /// When the article was crawled
    pub crawled_at: DateTime<Utc>,

    /// Which crawler instance crawled it
    pub crawled_by: String,

    /// Whether crawl was successful
    pub success: bool,
}

impl DedupRecord {
    /// Create a new dedup record
    pub fn new(article_id: &str, url: &str, content_hash: &str, crawled_by: &str) -> Self {
        Self {
            article_id: article_id.to_string(),
            url: url.to_string(),
            content_hash: content_hash.to_string(),
            crawled_at: Utc::now(),
            crawled_by: crawled_by.to_string(),
            success: true,
        }
    }

    /// Mark as failed
    pub fn with_failure(mut self) -> Self {
        self.success = false;
        self
    }
}

// ============================================================================
// Deduplication Check Results
// ============================================================================

/// Result of deduplication check
#[derive(Debug, Clone)]
pub struct DedupCheckResult {
    /// URLs that are new (not in database)
    pub new_urls: Vec<String>,

    /// URLs that have already been crawled
    pub existing_urls: Vec<String>,

    /// URLs with duplicate content hash
    pub duplicate_content: Vec<String>,

    /// Total checked
    pub total_checked: usize,
}

impl DedupCheckResult {
    /// Get count of new URLs
    pub fn new_count(&self) -> usize {
        self.new_urls.len()
    }

    /// Get count of existing URLs
    pub fn existing_count(&self) -> usize {
        self.existing_urls.len()
    }

    /// Get deduplication ratio (0.0 = all new, 1.0 = all existing)
    pub fn dedup_ratio(&self) -> f64 {
        if self.total_checked == 0 {
            return 0.0;
        }
        self.existing_count() as f64 / self.total_checked as f64
    }
}

// ============================================================================
// In-Memory Cache
// ============================================================================

/// In-memory cache for fast deduplication
struct DedupCache {
    /// Cached URLs
    urls: HashSet<String>,

    /// Cached content hashes
    hashes: HashSet<String>,

    /// Maximum cache size
    max_size: usize,
}

impl DedupCache {
    fn new(max_size: usize) -> Self {
        Self {
            urls: HashSet::with_capacity(max_size),
            hashes: HashSet::with_capacity(max_size / 2),
            max_size,
        }
    }

    fn contains_url(&self, url: &str) -> bool {
        self.urls.contains(url)
    }

    fn contains_hash(&self, hash: &str) -> bool {
        self.hashes.contains(hash)
    }

    fn insert_url(&mut self, url: String) {
        if self.urls.len() >= self.max_size {
            // Simple eviction: clear half the cache
            let to_remove: Vec<_> = self.urls.iter().take(self.max_size / 2).cloned().collect();
            for item in to_remove {
                self.urls.remove(&item);
            }
        }
        self.urls.insert(url);
    }

    fn insert_hash(&mut self, hash: String) {
        if self.hashes.len() >= self.max_size / 2 {
            let to_remove: Vec<_> = self
                .hashes
                .iter()
                .take(self.max_size / 4)
                .cloned()
                .collect();
            for item in to_remove {
                self.hashes.remove(&item);
            }
        }
        self.hashes.insert(hash);
    }

    fn clear(&mut self) {
        self.urls.clear();
        self.hashes.clear();
    }
}

// ============================================================================
// Async Deduplication Checker
// ============================================================================

/// Async deduplication checker using PostgreSQL
pub struct AsyncDedupChecker {
    /// PostgreSQL connection pool
    pool: Pool,

    /// In-memory cache
    cache: tokio::sync::RwLock<DedupCache>,

    /// Configuration
    config: DedupConfig,
}

impl AsyncDedupChecker {
    /// Create a new async deduplication checker
    pub async fn new(config: DedupConfig) -> Result<Self> {
        let mut pool_config = PoolConfig::new();
        pool_config.url = Some(config.database_url.clone());
        pool_config.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = pool_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .context("Failed to create PostgreSQL connection pool")?;

        // Test connection
        let client = pool
            .get()
            .await
            .context("Failed to connect to PostgreSQL")?;
        client.simple_query("SELECT 1").await?;

        let cache = DedupCache::new(config.cache_size);

        Ok(Self {
            pool,
            cache: tokio::sync::RwLock::new(cache),
            config,
        })
    }

    /// Initialize database schema
    pub async fn init_schema(&self) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .batch_execute(
                r#"
                CREATE TABLE IF NOT EXISTS crawl_dedup (
                    id SERIAL PRIMARY KEY,
                    article_id VARCHAR(50) NOT NULL UNIQUE,
                    url TEXT NOT NULL,
                    content_hash VARCHAR(64) NOT NULL,
                    crawled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                    crawled_by VARCHAR(20) NOT NULL,
                    success BOOLEAN NOT NULL DEFAULT TRUE
                );

                CREATE INDEX IF NOT EXISTS idx_crawl_dedup_url
                    ON crawl_dedup(url);

                CREATE INDEX IF NOT EXISTS idx_crawl_dedup_hash
                    ON crawl_dedup(content_hash);

                CREATE INDEX IF NOT EXISTS idx_crawl_dedup_crawled_at
                    ON crawl_dedup(crawled_at);

                CREATE INDEX IF NOT EXISTS idx_crawl_dedup_instance
                    ON crawl_dedup(crawled_by);
                "#,
            )
            .await
            .context("Failed to create dedup schema")?;

        tracing::info!("Deduplication schema initialized");
        Ok(())
    }

    /// Check if article ID exists
    pub async fn exists_by_id(&self, article_id: &str) -> Result<bool> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM crawl_dedup WHERE article_id = $1)",
                &[&article_id],
            )
            .await?;

        Ok(row.get(0))
    }

    /// Check if URL exists
    pub async fn exists_by_url(&self, url: &str) -> Result<bool> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if cache.contains_url(url) {
                return Ok(true);
            }
        }

        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM crawl_dedup WHERE url = $1 AND success = TRUE)",
                &[&url],
            )
            .await?;

        let exists: bool = row.get(0);

        // Update cache if exists
        if exists {
            let mut cache = self.cache.write().await;
            cache.insert_url(url.to_string());
        }

        Ok(exists)
    }

    /// Check if content hash exists (for duplicate content detection)
    pub async fn exists_by_hash(&self, content_hash: &str) -> Result<bool> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if cache.contains_hash(content_hash) {
                return Ok(true);
            }
        }

        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM crawl_dedup WHERE content_hash = $1)",
                &[&content_hash],
            )
            .await?;

        let exists: bool = row.get(0);

        if exists {
            let mut cache = self.cache.write().await;
            cache.insert_hash(content_hash.to_string());
        }

        Ok(exists)
    }

    /// Batch check URLs for deduplication
    ///
    /// Returns a DedupCheckResult with new and existing URLs
    pub async fn batch_check_urls(&self, urls: &[String]) -> Result<DedupCheckResult> {
        if urls.is_empty() {
            return Ok(DedupCheckResult {
                new_urls: vec![],
                existing_urls: vec![],
                duplicate_content: vec![],
                total_checked: 0,
            });
        }

        let mut new_urls = Vec::new();
        let mut existing_urls = Vec::new();

        // Check cache first
        let urls_to_check: Vec<String>;
        {
            let cache = self.cache.read().await;
            urls_to_check = urls
                .iter()
                .filter(|url| {
                    if cache.contains_url(url) {
                        existing_urls.push((*url).clone());
                        false
                    } else {
                        true
                    }
                })
                .cloned()
                .collect();
        }

        if urls_to_check.is_empty() {
            return Ok(DedupCheckResult {
                new_urls,
                existing_urls,
                duplicate_content: vec![],
                total_checked: urls.len(),
            });
        }

        // Query database for remaining URLs
        let client = self.pool.get().await?;

        // Build parameterized query for batch check
        let params: Vec<&(dyn ToSql + Sync)> = urls_to_check
            .iter()
            .map(|s| s as &(dyn ToSql + Sync))
            .collect();

        let placeholders: Vec<String> =
            (1..=urls_to_check.len()).map(|i| format!("${i}")).collect();

        let query = format!(
            "SELECT url FROM crawl_dedup WHERE url IN ({}) AND success = TRUE",
            placeholders.join(", ")
        );

        let rows = client.query(&query, &params).await?;

        let db_existing: HashSet<String> = rows.iter().map(|row| row.get(0)).collect();

        // Update cache and categorize results
        {
            let mut cache = self.cache.write().await;
            for url in &urls_to_check {
                if db_existing.contains(url) {
                    existing_urls.push(url.clone());
                    cache.insert_url(url.clone());
                } else {
                    new_urls.push(url.clone());
                }
            }
        }

        Ok(DedupCheckResult {
            new_urls,
            existing_urls,
            duplicate_content: vec![],
            total_checked: urls.len(),
        })
    }

    /// Record a crawled article
    pub async fn record_crawl(&self, record: &DedupRecord) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .execute(
                r#"
                INSERT INTO crawl_dedup (article_id, url, content_hash, crawled_at, crawled_by, success)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (article_id) DO UPDATE SET
                    url = EXCLUDED.url,
                    content_hash = EXCLUDED.content_hash,
                    crawled_at = EXCLUDED.crawled_at,
                    crawled_by = EXCLUDED.crawled_by,
                    success = EXCLUDED.success
                "#,
                &[
                    &record.article_id,
                    &record.url,
                    &record.content_hash,
                    &record.crawled_at,
                    &record.crawled_by,
                    &record.success,
                ],
            )
            .await
            .context("Failed to record crawl")?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert_url(record.url.clone());
            cache.insert_hash(record.content_hash.clone());
        }

        Ok(())
    }

    /// Batch record crawled articles
    pub async fn batch_record_crawls(&self, records: &[DedupRecord]) -> Result<usize> {
        if records.is_empty() {
            return Ok(0);
        }

        let client = self.pool.get().await?;

        // Use COPY for batch insert (more efficient)
        let statement = client
            .prepare(
                r#"
                INSERT INTO crawl_dedup (article_id, url, content_hash, crawled_at, crawled_by, success)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (article_id) DO UPDATE SET
                    url = EXCLUDED.url,
                    content_hash = EXCLUDED.content_hash,
                    crawled_at = EXCLUDED.crawled_at,
                    crawled_by = EXCLUDED.crawled_by,
                    success = EXCLUDED.success
                "#,
            )
            .await?;

        let mut count = 0;
        for record in records {
            let result = client
                .execute(
                    &statement,
                    &[
                        &record.article_id,
                        &record.url,
                        &record.content_hash,
                        &record.crawled_at,
                        &record.crawled_by,
                        &record.success,
                    ],
                )
                .await;

            if result.is_ok() {
                count += 1;
            }
        }

        // Update cache
        {
            let mut cache = self.cache.write().await;
            for record in records {
                cache.insert_url(record.url.clone());
                cache.insert_hash(record.content_hash.clone());
            }
        }

        Ok(count)
    }

    /// Get crawl statistics by instance
    pub async fn get_stats_by_instance(&self, instance: &str) -> Result<DedupStats> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT
                    COUNT(*) as total,
                    COUNT(*) FILTER (WHERE success = TRUE) as success,
                    COUNT(*) FILTER (WHERE success = FALSE) as failed,
                    MIN(crawled_at) as first_crawl,
                    MAX(crawled_at) as last_crawl
                FROM crawl_dedup
                WHERE crawled_by = $1
                "#,
                &[&instance],
            )
            .await?;

        Ok(DedupStats {
            total: row.get::<_, i64>(0) as usize,
            success: row.get::<_, i64>(1) as usize,
            failed: row.get::<_, i64>(2) as usize,
            first_crawl: row.get(3),
            last_crawl: row.get(4),
        })
    }

    /// Get total statistics
    pub async fn get_total_stats(&self) -> Result<DedupStats> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT
                    COUNT(*) as total,
                    COUNT(*) FILTER (WHERE success = TRUE) as success,
                    COUNT(*) FILTER (WHERE success = FALSE) as failed,
                    MIN(crawled_at) as first_crawl,
                    MAX(crawled_at) as last_crawl
                FROM crawl_dedup
                "#,
                &[],
            )
            .await?;

        Ok(DedupStats {
            total: row.get::<_, i64>(0) as usize,
            success: row.get::<_, i64>(1) as usize,
            failed: row.get::<_, i64>(2) as usize,
            first_crawl: row.get(3),
            last_crawl: row.get(4),
        })
    }

    /// Clear the in-memory cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get pool status
    pub fn pool_status(&self) -> PoolStatus {
        let status = self.pool.status();
        PoolStatus {
            size: status.size,
            available: status.available,
            waiting: status.waiting,
            max_size: self.config.pool_size,
        }
    }
}

// ============================================================================
// Statistics Types
// ============================================================================

/// Deduplication statistics
#[derive(Debug, Clone)]
pub struct DedupStats {
    /// Total records
    pub total: usize,

    /// Successful crawls
    pub success: usize,

    /// Failed crawls
    pub failed: usize,

    /// First crawl time
    pub first_crawl: Option<DateTime<Utc>>,

    /// Last crawl time
    pub last_crawl: Option<DateTime<Utc>>,
}

impl DedupStats {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.success as f64 / self.total as f64
    }
}

/// Connection pool status
#[derive(Debug, Clone)]
pub struct PoolStatus {
    /// Current pool size
    pub size: usize,

    /// Available connections
    pub available: usize,

    /// Waiting requests
    pub waiting: usize,

    /// Maximum pool size
    pub max_size: usize,
}

impl PoolStatus {
    /// Calculate utilization (0.0 - 1.0)
    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            return 0.0;
        }
        (self.size - self.available) as f64 / self.max_size as f64
    }
}

// ============================================================================
// Shared Deduplication Checker
// ============================================================================

/// Thread-safe wrapper for async dedup checker
pub type SharedDedupChecker = Arc<AsyncDedupChecker>;

/// Create a shared dedup checker
pub async fn create_shared_checker(config: DedupConfig) -> Result<SharedDedupChecker> {
    let checker = AsyncDedupChecker::new(config).await?;
    checker.init_schema().await?;
    Ok(Arc::new(checker))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_config_default() {
        let config = DedupConfig::default();
        assert_eq!(config.pool_size, 10);
        assert_eq!(config.cache_size, 10000);
    }

    #[test]
    fn test_dedup_config_builder() {
        let config = DedupConfig::default()
            .with_database_url("postgresql://test/db")
            .with_pool_size(20)
            .with_cache_size(5000);

        assert_eq!(config.database_url, "postgresql://test/db");
        assert_eq!(config.pool_size, 20);
        assert_eq!(config.cache_size, 5000);
    }

    #[test]
    fn test_dedup_record_creation() {
        let record = DedupRecord::new("001_0001", "https://example.com", "hash123", "main");

        assert_eq!(record.article_id, "001_0001");
        assert_eq!(record.url, "https://example.com");
        assert_eq!(record.content_hash, "hash123");
        assert_eq!(record.crawled_by, "main");
        assert!(record.success);
    }

    #[test]
    fn test_dedup_record_failure() {
        let record =
            DedupRecord::new("001_0001", "https://example.com", "hash123", "main").with_failure();

        assert!(!record.success);
    }

    #[test]
    fn test_dedup_check_result() {
        let result = DedupCheckResult {
            new_urls: vec!["url1".to_string(), "url2".to_string()],
            existing_urls: vec!["url3".to_string()],
            duplicate_content: vec![],
            total_checked: 3,
        };

        assert_eq!(result.new_count(), 2);
        assert_eq!(result.existing_count(), 1);
        assert!((result.dedup_ratio() - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_dedup_cache() {
        let mut cache = DedupCache::new(100);

        assert!(!cache.contains_url("url1"));
        cache.insert_url("url1".to_string());
        assert!(cache.contains_url("url1"));

        assert!(!cache.contains_hash("hash1"));
        cache.insert_hash("hash1".to_string());
        assert!(cache.contains_hash("hash1"));
    }

    #[test]
    fn test_dedup_cache_eviction() {
        let mut cache = DedupCache::new(10);

        // Insert more than max_size
        for i in 0..20 {
            cache.insert_url(format!("url{i}"));
        }

        // Some should have been evicted
        assert!(cache.urls.len() <= 15);
    }

    #[test]
    fn test_dedup_stats() {
        let stats = DedupStats {
            total: 100,
            success: 95,
            failed: 5,
            first_crawl: None,
            last_crawl: None,
        };

        assert!((stats.success_rate() - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_pool_status_utilization() {
        let status = PoolStatus {
            size: 10,
            available: 3,
            waiting: 0,
            max_size: 10,
        };

        assert!((status.utilization() - 0.7).abs() < 0.01);
    }
}
