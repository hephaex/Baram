//! Async deduplication for distributed crawlers
//!
//! This module provides PostgreSQL-based deduplication for distributed crawling:
//! - Async article ID and URL deduplication
//! - Content hash checking to avoid duplicate content
//! - Batch operations for efficient network usage
//! - Connection pooling for high throughput
//! - Bloom filter for fast in-memory duplicate checking

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use bloomfilter::Bloom;
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
            database_url: "postgresql://localhost/baram".to_string(),
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
            .unwrap_or_else(|_| "postgresql://localhost/baram".to_string());

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
// In-Memory Cache with Bloom Filter
// ============================================================================

/// In-memory cache for fast deduplication using Bloom Filter with rotation
///
/// This cache uses a Bloom filter for O(1) duplicate checking with minimal memory usage.
/// The Bloom filter may have false positives (saying a URL exists when it doesn't),
/// but never false negatives (saying a URL doesn't exist when it does).
///
/// When the Bloom filter indicates a URL might exist, we fall back to the database
/// for confirmation.
///
/// ## Rotation Strategy
///
/// To prevent unbounded memory growth in long-running processes, this cache implements
/// a double-buffer rotation strategy:
/// 1. When the current bloom filter reaches 80% capacity, create a new one
/// 2. Keep the previous filter to avoid false negatives during rotation
/// 3. Check both current and previous filters when querying
/// 4. Discard the previous filter when a new rotation occurs
struct DedupCache {
    /// Current bloom filter for URLs (primary fast check)
    url_bloom: Bloom<String>,

    /// Previous bloom filter for URLs (kept during rotation for accuracy)
    prev_url_bloom: Option<Bloom<String>>,

    /// Current bloom filter for content hashes
    hash_bloom: Bloom<String>,

    /// Previous bloom filter for hashes (kept during rotation)
    prev_hash_bloom: Option<Bloom<String>>,

    /// Fallback HashSet for definitive positive checks (limited size)
    /// Used to reduce false positives from bloom filter
    url_cache: HashSet<String>,

    /// Fallback HashSet for content hashes
    hash_cache: HashSet<String>,

    /// Maximum cache size for HashSet fallback
    max_cache_size: usize,

    /// Expected number of items for bloom filter
    bloom_capacity: usize,

    /// Count of items added to current URL bloom filter
    url_bloom_count: usize,

    /// Count of items added to current hash bloom filter
    hash_bloom_count: usize,

    /// Threshold for rotation (percentage of capacity, e.g., 0.8 = 80%)
    rotation_threshold: f64,

    /// Number of rotations performed
    rotation_count: usize,
}

impl DedupCache {
    /// Create a new cache with bloom filter
    ///
    /// # Arguments
    /// * `max_size` - Maximum size for the fallback HashSet cache
    ///
    /// The bloom filter is sized for 10x the max_size to handle larger datasets
    /// with a false positive rate of 1%.
    fn new(max_size: usize) -> Self {
        Self::with_rotation_threshold(max_size, 0.8)
    }

    /// Create a new cache with custom rotation threshold
    ///
    /// # Arguments
    /// * `max_size` - Maximum size for the fallback HashSet cache
    /// * `rotation_threshold` - Trigger rotation when bloom filter reaches this percentage (0.0-1.0)
    fn with_rotation_threshold(max_size: usize, rotation_threshold: f64) -> Self {
        // Bloom filter capacity: 10x the cache size to handle more URLs
        let bloom_capacity = max_size * 10;

        // Create bloom filters with 1% false positive rate
        let url_bloom = Bloom::new_for_fp_rate(bloom_capacity, 0.01);
        let hash_bloom = Bloom::new_for_fp_rate(bloom_capacity / 2, 0.01);

        Self {
            url_bloom,
            prev_url_bloom: None,
            hash_bloom,
            prev_hash_bloom: None,
            url_cache: HashSet::with_capacity(max_size),
            hash_cache: HashSet::with_capacity(max_size / 2),
            max_cache_size: max_size,
            bloom_capacity,
            url_bloom_count: 0,
            hash_bloom_count: 0,
            rotation_threshold: rotation_threshold.clamp(0.5, 0.95),
            rotation_count: 0,
        }
    }

    /// Check if URL bloom filter needs rotation
    fn should_rotate_url_bloom(&self) -> bool {
        let threshold = (self.bloom_capacity as f64 * self.rotation_threshold) as usize;
        self.url_bloom_count >= threshold
    }

    /// Check if hash bloom filter needs rotation
    fn should_rotate_hash_bloom(&self) -> bool {
        let threshold = ((self.bloom_capacity / 2) as f64 * self.rotation_threshold) as usize;
        self.hash_bloom_count >= threshold
    }

    /// Rotate URL bloom filter
    fn rotate_url_bloom(&mut self) {
        // Move current to previous, create new current
        self.prev_url_bloom = Some(std::mem::replace(
            &mut self.url_bloom,
            Bloom::new_for_fp_rate(self.bloom_capacity, 0.01),
        ));
        self.url_bloom_count = 0;
        self.rotation_count += 1;

        tracing::info!(
            "Rotated URL bloom filter (rotation #{})",
            self.rotation_count
        );
    }

    /// Rotate hash bloom filter
    fn rotate_hash_bloom(&mut self) {
        self.prev_hash_bloom = Some(std::mem::replace(
            &mut self.hash_bloom,
            Bloom::new_for_fp_rate(self.bloom_capacity / 2, 0.01),
        ));
        self.hash_bloom_count = 0;
    }

    /// Check if URL might exist (bloom filter check)
    ///
    /// Returns true if the URL definitely or possibly exists.
    /// Returns false if the URL definitely does not exist.
    fn contains_url(&self, url: &str) -> bool {
        let url_string = url.to_string();

        // Check current bloom filter
        if !self.url_bloom.check(&url_string) {
            // Also check previous bloom filter if it exists
            if let Some(ref prev) = self.prev_url_bloom {
                if !prev.check(&url_string) {
                    // Definitely not in either bloom filter
                    return false;
                }
            } else {
                // No previous bloom filter and not in current
                return false;
            }
        }

        // Bloom filter says it might exist, check HashSet for confirmation
        self.url_cache.contains(url)
    }

    /// Check if content hash might exist
    fn contains_hash(&self, hash: &str) -> bool {
        let hash_string = hash.to_string();

        if !self.hash_bloom.check(&hash_string) {
            if let Some(ref prev) = self.prev_hash_bloom {
                if !prev.check(&hash_string) {
                    return false;
                }
            } else {
                return false;
            }
        }
        self.hash_cache.contains(hash)
    }

    /// Insert URL into both bloom filter and cache
    fn insert_url(&mut self, url: String) {
        // Check if rotation is needed before inserting
        if self.should_rotate_url_bloom() {
            self.rotate_url_bloom();
        }

        // Add to bloom filter and increment count
        self.url_bloom.set(&url);
        self.url_bloom_count += 1;

        // Add to HashSet with eviction policy
        if self.url_cache.len() >= self.max_cache_size {
            // Simple eviction: clear half the cache
            let to_remove: Vec<_> = self
                .url_cache
                .iter()
                .take(self.max_cache_size / 2)
                .cloned()
                .collect();
            for item in to_remove {
                self.url_cache.remove(&item);
            }
        }
        self.url_cache.insert(url);
    }

    /// Insert content hash into both bloom filter and cache
    fn insert_hash(&mut self, hash: String) {
        // Check if rotation is needed
        if self.should_rotate_hash_bloom() {
            self.rotate_hash_bloom();
        }

        self.hash_bloom.set(&hash);
        self.hash_bloom_count += 1;

        if self.hash_cache.len() >= self.max_cache_size / 2 {
            let to_remove: Vec<_> = self
                .hash_cache
                .iter()
                .take(self.max_cache_size / 4)
                .cloned()
                .collect();
            for item in to_remove {
                self.hash_cache.remove(&item);
            }
        }
        self.hash_cache.insert(hash);
    }

    /// Check bloom filter directly (for quick rejection without DB query)
    ///
    /// This is useful for filtering out definitely new URLs before batch DB queries.
    /// Checks both current and previous bloom filters.
    fn bloom_check_url(&self, url: &str) -> bool {
        let url_string = url.to_string();
        // Check current first
        if self.url_bloom.check(&url_string) {
            return true;
        }
        // Fall back to previous if it exists
        if let Some(ref prev) = self.prev_url_bloom {
            return prev.check(&url_string);
        }
        false
    }

    /// Get bloom filter statistics
    fn bloom_stats(&self) -> BloomStats {
        BloomStats {
            url_capacity: self.bloom_capacity,
            hash_capacity: self.bloom_capacity / 2,
            cache_size: self.url_cache.len(),
            hash_cache_size: self.hash_cache.len(),
            url_bloom_count: self.url_bloom_count,
            hash_bloom_count: self.hash_bloom_count,
            rotation_count: self.rotation_count,
            has_previous_bloom: self.prev_url_bloom.is_some(),
        }
    }

    /// Clear all caches (bloom filters and HashSets)
    fn clear(&mut self) {
        // Recreate bloom filters
        self.url_bloom = Bloom::new_for_fp_rate(self.bloom_capacity, 0.01);
        self.hash_bloom = Bloom::new_for_fp_rate(self.bloom_capacity / 2, 0.01);
        self.prev_url_bloom = None;
        self.prev_hash_bloom = None;
        self.url_cache.clear();
        self.hash_cache.clear();
        self.url_bloom_count = 0;
        self.hash_bloom_count = 0;
    }

    /// Force rotation of bloom filters
    ///
    /// This can be called periodically (e.g., daily) to ensure memory doesn't grow unbounded
    pub fn force_rotation(&mut self) {
        self.rotate_url_bloom();
        self.rotate_hash_bloom();
    }
}

/// Bloom filter statistics
#[derive(Debug, Clone)]
pub struct BloomStats {
    /// URL bloom filter capacity
    pub url_capacity: usize,

    /// Hash bloom filter capacity
    pub hash_capacity: usize,

    /// Current HashSet cache size for URLs
    pub cache_size: usize,

    /// Current HashSet cache size for hashes
    pub hash_cache_size: usize,

    /// Number of items in current URL bloom filter
    pub url_bloom_count: usize,

    /// Number of items in current hash bloom filter
    pub hash_bloom_count: usize,

    /// Number of rotations performed
    pub rotation_count: usize,

    /// Whether there's a previous bloom filter (double-buffer active)
    pub has_previous_bloom: bool,
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

    /// Load existing URLs from database into bloom filter
    ///
    /// This should be called after initialization to populate the bloom filter
    /// with all existing URLs from the database. This enables fast deduplication
    /// checks without querying the database for every URL.
    ///
    /// # Arguments
    /// * `limit` - Optional limit on number of URLs to load (default: load all)
    ///
    /// # Performance
    /// Loading 100,000 URLs takes approximately 1-2 seconds and uses minimal memory
    /// thanks to the bloom filter's space efficiency.
    pub async fn load_existing_urls(&self, limit: Option<usize>) -> Result<usize> {
        let client = self.pool.get().await?;

        let query = if let Some(limit) = limit {
            format!(
                "SELECT url, content_hash FROM crawl_dedup WHERE success = TRUE ORDER BY crawled_at DESC LIMIT {}",
                limit
            )
        } else {
            "SELECT url, content_hash FROM crawl_dedup WHERE success = TRUE".to_string()
        };

        let rows = client.query(&query, &[]).await?;

        let mut cache = self.cache.write().await;
        let count = rows.len();

        for row in rows {
            let url: String = row.get(0);
            let hash: String = row.get(1);
            cache.insert_url(url);
            cache.insert_hash(hash);
        }

        drop(cache);

        tracing::info!(
            "Loaded {} existing URLs into bloom filter",
            count
        );

        Ok(count)
    }

    /// Get bloom filter statistics
    pub async fn get_bloom_stats(&self) -> BloomStats {
        let cache = self.cache.read().await;
        cache.bloom_stats()
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

    /// Batch check URLs for deduplication using bloom filter
    ///
    /// This method uses a multi-tier checking approach for optimal performance:
    /// 1. Bloom filter: O(1) check to quickly identify definitely new URLs
    /// 2. HashSet cache: O(1) check for recently seen URLs
    /// 3. Database query: Batch check for remaining URLs
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
        let mut bloom_rejected_urls = Vec::new();

        // Phase 1: Bloom filter check - O(1) per URL
        // Quickly identify URLs that are definitely NOT in the database
        let urls_after_bloom: Vec<String>;
        {
            let cache = self.cache.read().await;
            urls_after_bloom = urls
                .iter()
                .filter(|url| {
                    // Bloom filter check: if false, URL is definitely new
                    if !cache.bloom_check_url(url) {
                        bloom_rejected_urls.push((*url).clone());
                        false
                    } else {
                        true
                    }
                })
                .cloned()
                .collect();
        }

        // Phase 2: HashSet cache check - O(1) per URL
        // For URLs that passed bloom filter, check HashSet for confirmation
        let urls_to_check: Vec<String>;
        {
            let cache = self.cache.read().await;
            urls_to_check = urls_after_bloom
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

        // URLs rejected by bloom filter are definitely new
        new_urls.extend(bloom_rejected_urls);

        // If all URLs have been categorized, return early
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

    /// Force rotation of bloom filters
    ///
    /// This can be called periodically (e.g., daily) to ensure memory doesn't grow unbounded.
    /// The rotation uses a double-buffer strategy to maintain accuracy during the transition.
    pub async fn force_bloom_rotation(&self) {
        let mut cache = self.cache.write().await;
        cache.force_rotation();
        tracing::info!("Forced bloom filter rotation");
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

/// Create a shared dedup checker with pre-loaded URLs
///
/// This function initializes the deduplication checker and loads existing URLs
/// from the database into the bloom filter for fast duplicate checking.
///
/// # Arguments
/// * `config` - Deduplication configuration
/// * `load_existing` - Whether to load existing URLs into bloom filter (default: true)
///
/// # Performance
/// Loading existing URLs adds 1-2 seconds to startup time but dramatically
/// improves crawling performance by avoiding database queries for duplicates.
pub async fn create_shared_checker(config: DedupConfig) -> Result<SharedDedupChecker> {
    let checker = AsyncDedupChecker::new(config).await?;
    checker.init_schema().await?;

    // Load existing URLs into bloom filter for fast deduplication
    // This is especially important for large databases (>10k URLs)
    match checker.load_existing_urls(None).await {
        Ok(count) => {
            tracing::info!("Pre-loaded {} URLs into bloom filter for fast deduplication", count);
        }
        Err(e) => {
            tracing::warn!("Failed to pre-load URLs into bloom filter: {}", e);
            tracing::warn!("Deduplication will fall back to database queries");
        }
    }

    Ok(Arc::new(checker))
}

/// Create a shared dedup checker without pre-loading URLs
///
/// Use this if you want to manually control when to load URLs, or if you're
/// starting with an empty database.
pub async fn create_shared_checker_without_preload(
    config: DedupConfig,
) -> Result<SharedDedupChecker> {
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

        // Some should have been evicted from HashSet
        assert!(cache.url_cache.len() <= 15);
    }

    #[test]
    fn test_bloom_filter_basic() {
        let mut cache = DedupCache::new(100);

        // New URL should not be in bloom filter
        assert!(!cache.bloom_check_url("https://new-url.com"));

        // After insertion, should be in bloom filter
        cache.insert_url("https://new-url.com".to_string());
        assert!(cache.bloom_check_url("https://new-url.com"));
    }

    #[test]
    fn test_bloom_filter_false_positive_handling() {
        let mut cache = DedupCache::new(100);

        // Insert a URL into bloom filter
        cache.insert_url("https://exists.com".to_string());

        // Bloom filter should say it exists
        assert!(cache.bloom_check_url("https://exists.com"));

        // contains_url should confirm it exists (checks both bloom and HashSet)
        assert!(cache.contains_url("https://exists.com"));

        // A URL not in the filter should be rejected quickly
        assert!(!cache.bloom_check_url("https://definitely-new.com"));
        assert!(!cache.contains_url("https://definitely-new.com"));
    }

    #[test]
    fn test_bloom_stats() {
        let cache = DedupCache::new(1000);
        let stats = cache.bloom_stats();

        // Bloom filter should be 10x the cache size
        assert_eq!(stats.url_capacity, 10000);
        assert_eq!(stats.hash_capacity, 5000);
        assert_eq!(stats.cache_size, 0);
        assert_eq!(stats.hash_cache_size, 0);
        assert_eq!(stats.url_bloom_count, 0);
        assert_eq!(stats.hash_bloom_count, 0);
        assert_eq!(stats.rotation_count, 0);
        assert!(!stats.has_previous_bloom);
    }

    #[test]
    fn test_bloom_rotation_threshold() {
        // Create cache with small capacity for testing (10 items, 80% threshold = 8)
        let mut cache = DedupCache::with_rotation_threshold(1, 0.8);
        // bloom_capacity = 1 * 10 = 10, threshold = 10 * 0.8 = 8

        // Insert 8 URLs (0-7) - count reaches threshold but rotation happens on next insert
        for i in 0..8 {
            cache.insert_url(format!("url{i}"));
        }
        assert_eq!(cache.url_bloom_count, 8);
        assert_eq!(cache.rotation_count, 0);
        assert!(cache.prev_url_bloom.is_none());

        // Insert 9th URL - should trigger rotation (count was >= 8 threshold)
        cache.insert_url("url8".to_string());
        assert_eq!(cache.rotation_count, 1);
        assert!(cache.prev_url_bloom.is_some());
        // After rotation, count resets to 1 (the newly inserted url8)
        assert_eq!(cache.url_bloom_count, 1);

        // Previous URLs should still be found via double-buffer
        assert!(cache.bloom_check_url("url0"));
        assert!(cache.bloom_check_url("url8"));
    }

    #[test]
    fn test_bloom_double_buffer() {
        let mut cache = DedupCache::with_rotation_threshold(1, 0.5);
        // bloom_capacity = 10, threshold = 5

        // Insert some URLs
        for i in 0..4 {
            cache.insert_url(format!("old_url{i}"));
        }

        // Force rotation
        cache.force_rotation();
        assert_eq!(cache.rotation_count, 1);

        // Old URLs should still be checkable via previous bloom
        assert!(cache.bloom_check_url("old_url0"));
        assert!(cache.bloom_check_url("old_url3"));

        // New URLs should be in current bloom
        cache.insert_url("new_url".to_string());
        assert!(cache.bloom_check_url("new_url"));

        // Second rotation - old bloom is discarded
        cache.force_rotation();
        assert_eq!(cache.rotation_count, 2);

        // Very old URLs may no longer be found (previous bloom replaced)
        // But new_url should be in prev_url_bloom now
        assert!(cache.bloom_check_url("new_url"));
    }

    #[test]
    fn test_bloom_stats_after_rotation() {
        let mut cache = DedupCache::with_rotation_threshold(1, 0.8);

        for i in 0..10 {
            cache.insert_url(format!("url{i}"));
        }

        let stats = cache.bloom_stats();
        assert!(stats.rotation_count >= 1);
        assert!(stats.has_previous_bloom);
        // After rotation, count should be low (only items added after rotation)
        assert!(stats.url_bloom_count < 10);
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
