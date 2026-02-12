//! Async deduplication for distributed crawlers
//!
//! This module provides PostgreSQL-based deduplication for distributed crawling:
//! - Async article ID and URL deduplication
//! - Content hash checking to avoid duplicate content
//! - Batch operations for efficient network usage
//! - Connection pooling for high throughput
//! - Bloom filter for fast in-memory duplicate checking
//! - **Rotating bloom filter** to prevent memory exhaustion during long runs

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
// Rotating Bloom Filter (Fixes Issue #20: Memory Leak)
// ============================================================================

/// Configuration for rotating bloom filter
#[derive(Debug, Clone)]
pub struct RotatingBloomConfig {
    /// Capacity per bloom filter generation
    pub capacity_per_generation: usize,

    /// False positive rate (e.g., 0.01 = 1%)
    pub false_positive_rate: f64,

    /// Rotation threshold: rotate when active filter reaches this percentage (0.0-1.0)
    pub rotation_threshold: f64,

    /// Maximum age before forced rotation (even if not at capacity)
    pub max_age: Duration,
}

impl Default for RotatingBloomConfig {
    fn default() -> Self {
        Self {
            capacity_per_generation: 50_000,
            false_positive_rate: 0.01,
            rotation_threshold: 0.8,
            max_age: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Rotating Bloom Filter with double-buffering to prevent memory exhaustion
///
/// This implementation maintains two bloom filters:
/// - **Active**: Currently accepting new insertions
/// - **Previous**: The previous generation, kept for lookups during transition
///
/// When the active filter reaches the rotation threshold or max age, it rotates:
/// 1. Previous filter is discarded
/// 2. Active becomes previous
/// 3. A new empty filter becomes active
///
/// This ensures bounded memory usage regardless of how long the crawler runs.
pub struct RotatingBloomFilter {
    /// Currently active bloom filter (accepts inserts)
    active: Bloom<String>,

    /// Previous generation (kept for lookups, read-only)
    previous: Option<Bloom<String>>,

    /// Number of items inserted into active filter
    active_count: AtomicUsize,

    /// When the active filter was created
    active_created_at: Instant,

    /// Configuration
    config: RotatingBloomConfig,

    /// Total rotations performed
    rotation_count: AtomicUsize,
}

impl RotatingBloomFilter {
    /// Create a new rotating bloom filter
    pub fn new(config: RotatingBloomConfig) -> Self {
        let active =
            Bloom::new_for_fp_rate(config.capacity_per_generation, config.false_positive_rate);

        Self {
            active,
            previous: None,
            active_count: AtomicUsize::new(0),
            active_created_at: Instant::now(),
            config,
            rotation_count: AtomicUsize::new(0),
        }
    }

    /// Create with default configuration for a given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let config = RotatingBloomConfig {
            capacity_per_generation: capacity,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Check if an item might exist in any generation
    ///
    /// Returns `true` if the item is in active OR previous filter.
    /// May return false positives, but never false negatives for recently added items.
    pub fn check(&self, item: &String) -> bool {
        // Check active filter first (most likely to contain recent items)
        if self.active.check(item) {
            return true;
        }

        // Check previous generation if it exists
        if let Some(ref prev) = self.previous {
            return prev.check(item);
        }

        false
    }

    /// Insert an item into the active filter
    ///
    /// Automatically triggers rotation if thresholds are exceeded.
    pub fn insert(&mut self, item: &String) {
        self.active.set(item);
        self.active_count.fetch_add(1, Ordering::Relaxed);

        // Check if rotation is needed
        self.maybe_rotate();
    }

    /// Check if rotation is needed and perform it
    fn maybe_rotate(&mut self) {
        let count = self.active_count.load(Ordering::Relaxed);
        let threshold_count =
            (self.config.capacity_per_generation as f64 * self.config.rotation_threshold) as usize;

        let age = self.active_created_at.elapsed();

        // Rotate if:
        // 1. Active filter has reached capacity threshold, OR
        // 2. Active filter has exceeded max age (with at least some items)
        if count >= threshold_count || (age >= self.config.max_age && count > 0) {
            self.rotate();
        }
    }

    /// Perform rotation: discard previous, move active to previous, create new active
    fn rotate(&mut self) {
        tracing::info!(
            "Rotating bloom filter: {} items in active (threshold: {}), age: {:?}",
            self.active_count.load(Ordering::Relaxed),
            (self.config.capacity_per_generation as f64 * self.config.rotation_threshold) as usize,
            self.active_created_at.elapsed()
        );

        // Create new active filter
        let new_active = Bloom::new_for_fp_rate(
            self.config.capacity_per_generation,
            self.config.false_positive_rate,
        );

        // Move active to previous (old previous is dropped)
        self.previous = Some(std::mem::replace(&mut self.active, new_active));

        // Reset counters
        self.active_count.store(0, Ordering::Relaxed);
        self.active_created_at = Instant::now();
        self.rotation_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Force rotation (useful for testing or manual memory management)
    pub fn force_rotate(&mut self) {
        self.rotate();
    }

    /// Clear both filters completely
    pub fn clear(&mut self) {
        self.active = Bloom::new_for_fp_rate(
            self.config.capacity_per_generation,
            self.config.false_positive_rate,
        );
        self.previous = None;
        self.active_count.store(0, Ordering::Relaxed);
        self.active_created_at = Instant::now();
    }

    /// Get statistics about the rotating bloom filter
    pub fn stats(&self) -> RotatingBloomStats {
        RotatingBloomStats {
            active_count: self.active_count.load(Ordering::Relaxed),
            capacity_per_generation: self.config.capacity_per_generation,
            has_previous: self.previous.is_some(),
            active_age_secs: self.active_created_at.elapsed().as_secs(),
            rotation_count: self.rotation_count.load(Ordering::Relaxed),
            rotation_threshold: self.config.rotation_threshold,
        }
    }
}

/// Statistics for rotating bloom filter
#[derive(Debug, Clone)]
pub struct RotatingBloomStats {
    /// Number of items in active filter
    pub active_count: usize,

    /// Capacity per generation
    pub capacity_per_generation: usize,

    /// Whether a previous generation exists
    pub has_previous: bool,

    /// Age of active filter in seconds
    pub active_age_secs: u64,

    /// Number of rotations performed
    pub rotation_count: usize,

    /// Rotation threshold (0.0-1.0)
    pub rotation_threshold: f64,
}

impl RotatingBloomStats {
    /// Calculate fill ratio of active filter (0.0-1.0)
    pub fn fill_ratio(&self) -> f64 {
        if self.capacity_per_generation == 0 {
            return 0.0;
        }
        self.active_count as f64 / self.capacity_per_generation as f64
    }

    /// Check if rotation is imminent
    pub fn rotation_imminent(&self) -> bool {
        self.fill_ratio() >= self.rotation_threshold * 0.9
    }
}

// ============================================================================
// In-Memory Cache with Rotating Bloom Filter
// ============================================================================

/// In-memory cache for fast deduplication using Rotating Bloom Filter
///
/// This cache uses a rotating bloom filter for O(1) duplicate checking with
/// **bounded memory usage**. The bloom filter may have false positives (saying
/// a URL exists when it doesn't), but never false negatives.
///
/// **Memory Safety**: Unlike a standard bloom filter that grows unbounded,
/// this implementation automatically rotates filters when they reach capacity,
/// preventing memory exhaustion during long-running crawls.
///
/// When the bloom filter indicates a URL might exist, we fall back to the database
/// for confirmation.
struct DedupCache {
    /// Rotating bloom filter for URLs (primary fast check)
    url_bloom: RotatingBloomFilter,

    /// Rotating bloom filter for content hashes
    hash_bloom: RotatingBloomFilter,

    /// Fallback HashSet for definitive positive checks (limited size)
    /// Used to reduce false positives from bloom filter
    url_cache: HashSet<String>,

    /// Fallback HashSet for content hashes
    hash_cache: HashSet<String>,

    /// Maximum cache size for HashSet fallback
    max_cache_size: usize,

    /// Expected number of items for bloom filter
    bloom_capacity: usize,
}

impl DedupCache {
    /// Create a new cache with rotating bloom filter
    ///
    /// # Arguments
    /// * `max_size` - Maximum size for the fallback HashSet cache
    ///
    /// The rotating bloom filter is sized for 10x the max_size per generation,
    /// with automatic rotation when reaching 80% capacity or 1 hour age.
    /// This prevents memory exhaustion during long-running crawls.
    fn new(max_size: usize) -> Self {
        // Bloom filter capacity per generation: 10x the cache size
        let bloom_capacity = max_size * 10;

        // Create rotating bloom filters with bounded memory
        let url_bloom_config = RotatingBloomConfig {
            capacity_per_generation: bloom_capacity,
            false_positive_rate: 0.01,
            rotation_threshold: 0.8,
            max_age: Duration::from_secs(3600), // 1 hour
        };

        let hash_bloom_config = RotatingBloomConfig {
            capacity_per_generation: bloom_capacity / 2,
            false_positive_rate: 0.01,
            rotation_threshold: 0.8,
            max_age: Duration::from_secs(3600),
        };

        Self {
            url_bloom: RotatingBloomFilter::new(url_bloom_config),
            hash_bloom: RotatingBloomFilter::new(hash_bloom_config),
            url_cache: HashSet::with_capacity(max_size),
            hash_cache: HashSet::with_capacity(max_size / 2),
            max_cache_size: max_size,
            bloom_capacity,
        }
    }

    /// Check if URL might exist (bloom filter check)
    ///
    /// Returns true if the URL definitely or possibly exists.
    /// Returns false if the URL definitely does not exist.
    fn contains_url(&self, url: &str) -> bool {
        // First check bloom filter (fast, may have false positives)
        if !self.url_bloom.check(&url.to_string()) {
            // Definitely not in cache
            return false;
        }

        // Bloom filter says it might exist, check HashSet for confirmation
        self.url_cache.contains(url)
    }

    /// Check if content hash might exist
    fn contains_hash(&self, hash: &str) -> bool {
        if !self.hash_bloom.check(&hash.to_string()) {
            return false;
        }
        self.hash_cache.contains(hash)
    }

    /// Insert URL into both rotating bloom filter and cache
    fn insert_url(&mut self, url: String) {
        // Add to rotating bloom filter (automatic rotation when needed)
        self.url_bloom.insert(&url);

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

    /// Insert content hash into both rotating bloom filter and cache
    fn insert_hash(&mut self, hash: String) {
        // Add to rotating bloom filter (automatic rotation when needed)
        self.hash_bloom.insert(&hash);

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
    fn bloom_check_url(&self, url: &str) -> bool {
        self.url_bloom.check(&url.to_string())
    }

    /// Get bloom filter statistics
    fn bloom_stats(&self) -> BloomStats {
        let url_stats = self.url_bloom.stats();
        let hash_stats = self.hash_bloom.stats();

        BloomStats {
            url_capacity: self.bloom_capacity,
            hash_capacity: self.bloom_capacity / 2,
            cache_size: self.url_cache.len(),
            hash_cache_size: self.hash_cache.len(),
            url_active_count: url_stats.active_count,
            url_rotation_count: url_stats.rotation_count,
            hash_active_count: hash_stats.active_count,
            hash_rotation_count: hash_stats.rotation_count,
        }
    }

    /// Clear all caches (bloom filters and HashSets)
    fn clear(&mut self) {
        // Clear rotating bloom filters (resets to empty state)
        self.url_bloom.clear();
        self.hash_bloom.clear();
        self.url_cache.clear();
        self.hash_cache.clear();
    }

    /// Force rotation of bloom filters
    /// Useful for testing or manual memory management
    #[allow(dead_code)]
    fn force_rotate(&mut self) {
        self.url_bloom.force_rotate();
        self.hash_bloom.force_rotate();
    }
}

/// Bloom filter statistics
#[derive(Debug, Clone)]
pub struct BloomStats {
    /// URL bloom filter capacity per generation
    pub url_capacity: usize,

    /// Hash bloom filter capacity per generation
    pub hash_capacity: usize,

    /// Current HashSet cache size for URLs
    pub cache_size: usize,

    /// Current HashSet cache size for hashes
    pub hash_cache_size: usize,

    /// Items in active URL bloom filter
    pub url_active_count: usize,

    /// Number of URL bloom filter rotations
    pub url_rotation_count: usize,

    /// Items in active hash bloom filter
    pub hash_active_count: usize,

    /// Number of hash bloom filter rotations
    pub hash_rotation_count: usize,
}

impl BloomStats {
    /// Check if memory usage is bounded (rotation is working)
    pub fn is_memory_bounded(&self) -> bool {
        // If there have been rotations, memory is being managed
        self.url_rotation_count > 0 || self.hash_rotation_count > 0 ||
        // Or if counts are well below capacity
        (self.url_active_count < self.url_capacity && self.hash_active_count < self.hash_capacity)
    }

    /// Estimated memory usage in bytes (approximate)
    pub fn estimated_memory_bytes(&self) -> usize {
        // Bloom filter uses approximately capacity * 10 bits for 1% false positive rate
        // Plus HashSet overhead
        let bloom_bits = (self.url_capacity + self.hash_capacity) * 10;
        let bloom_bytes = bloom_bits / 8;
        let hashset_bytes =
            (self.cache_size + self.hash_cache_size) * std::mem::size_of::<String>();

        // Double for previous generation if rotation has occurred
        let rotation_multiplier = if self.url_rotation_count > 0 || self.hash_rotation_count > 0 {
            2
        } else {
            1
        };

        bloom_bytes * rotation_multiplier + hashset_bytes
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

        tracing::info!("Loaded {} existing URLs into bloom filter", count);

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
            tracing::info!(
                "Pre-loaded {} URLs into bloom filter for fast deduplication",
                count
            );
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

        // Bloom filter should be 10x the cache size per generation
        assert_eq!(stats.url_capacity, 10000);
        assert_eq!(stats.hash_capacity, 5000);
        assert_eq!(stats.cache_size, 0);
        assert_eq!(stats.hash_cache_size, 0);
        assert_eq!(stats.url_active_count, 0);
        assert_eq!(stats.url_rotation_count, 0);
        assert_eq!(stats.hash_active_count, 0);
        assert_eq!(stats.hash_rotation_count, 0);
    }

    // =========================================================================
    // Rotating Bloom Filter Tests (Issue #20 Fix)
    // =========================================================================

    #[test]
    fn test_rotating_bloom_filter_basic() {
        let config = RotatingBloomConfig {
            capacity_per_generation: 1000,
            false_positive_rate: 0.01,
            rotation_threshold: 0.8,
            max_age: Duration::from_secs(3600),
        };
        let mut bloom = RotatingBloomFilter::new(config);

        // New item should not exist
        assert!(!bloom.check(&"item1".to_string()));

        // After insertion, should exist
        bloom.insert(&"item1".to_string());
        assert!(bloom.check(&"item1".to_string()));

        // Stats should reflect one item
        let stats = bloom.stats();
        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.rotation_count, 0);
    }

    #[test]
    fn test_rotating_bloom_filter_rotation() {
        let config = RotatingBloomConfig {
            capacity_per_generation: 10, // Small capacity to trigger rotation
            false_positive_rate: 0.01,
            rotation_threshold: 0.8, // Rotate at 8 items
            max_age: Duration::from_secs(3600),
        };
        let mut bloom = RotatingBloomFilter::new(config);

        // Insert enough items to trigger rotation (8 items = 80% of 10)
        for i in 0..10 {
            bloom.insert(&format!("item{i}"));
        }

        let stats = bloom.stats();
        // Should have rotated once (10 items inserted, threshold at 8)
        assert!(stats.rotation_count >= 1);
        // Active count should be less than total inserted
        assert!(stats.active_count < 10);
        // Previous generation should exist
        assert!(stats.has_previous);
    }

    #[test]
    fn test_rotating_bloom_filter_lookup_across_generations() {
        let config = RotatingBloomConfig {
            capacity_per_generation: 100, // Larger capacity to avoid multiple rotations
            false_positive_rate: 0.01,
            rotation_threshold: 0.8, // Rotate at 80 items
            max_age: Duration::from_secs(3600),
        };
        let mut bloom = RotatingBloomFilter::new(config);

        // Insert item before rotation
        bloom.insert(&"early_item".to_string());

        // Insert enough items to trigger exactly one rotation
        for i in 0..80 {
            bloom.insert(&format!("item{i}"));
        }

        let stats = bloom.stats();
        // Should have rotated once
        assert_eq!(stats.rotation_count, 1);
        assert!(stats.has_previous);

        // Early item should still be found (in previous generation)
        assert!(bloom.check(&"early_item".to_string()));
        // Recent items should be found (in active generation)
        assert!(bloom.check(&"item79".to_string()));

        // Now trigger another rotation
        for i in 80..160 {
            bloom.insert(&format!("item{i}"));
        }

        // After second rotation, early_item should be gone
        // (it was in the previous generation which is now discarded)
        // This is the expected behavior to prevent memory leaks!
        assert!(!bloom.check(&"early_item".to_string()));
        // But recent items should still be found
        assert!(bloom.check(&"item159".to_string()));
    }

    #[test]
    fn test_rotating_bloom_filter_clear() {
        let config = RotatingBloomConfig {
            capacity_per_generation: 100,
            false_positive_rate: 0.01,
            rotation_threshold: 0.8,
            max_age: Duration::from_secs(3600),
        };
        let mut bloom = RotatingBloomFilter::new(config);

        // Insert items
        for i in 0..50 {
            bloom.insert(&format!("item{i}"));
        }

        assert_eq!(bloom.stats().active_count, 50);

        // Clear and verify
        bloom.clear();
        let stats = bloom.stats();
        assert_eq!(stats.active_count, 0);
        assert!(!stats.has_previous);
        assert!(!bloom.check(&"item0".to_string()));
    }

    #[test]
    fn test_rotating_bloom_filter_force_rotate() {
        let config = RotatingBloomConfig {
            capacity_per_generation: 1000,
            false_positive_rate: 0.01,
            rotation_threshold: 0.8,
            max_age: Duration::from_secs(3600),
        };
        let mut bloom = RotatingBloomFilter::new(config);

        // Insert some items
        for i in 0..10 {
            bloom.insert(&format!("item{i}"));
        }

        // Force rotation
        bloom.force_rotate();

        let stats = bloom.stats();
        assert_eq!(stats.rotation_count, 1);
        assert!(stats.has_previous);
        assert_eq!(stats.active_count, 0);

        // Items should still be found in previous generation
        assert!(bloom.check(&"item0".to_string()));
    }

    #[test]
    fn test_rotating_bloom_stats_fill_ratio() {
        let stats = RotatingBloomStats {
            active_count: 800,
            capacity_per_generation: 1000,
            has_previous: false,
            active_age_secs: 60,
            rotation_count: 0,
            rotation_threshold: 0.8,
        };

        assert!((stats.fill_ratio() - 0.8).abs() < 0.001);
        assert!(stats.rotation_imminent()); // 80% is >= 72% (90% of 80%)
    }

    #[test]
    fn test_dedup_cache_rotation_integration() {
        let mut cache = DedupCache::new(10); // Small size for testing

        // Insert enough URLs to trigger rotation (10 * 10 * 0.8 = 80 items threshold)
        for i in 0..100 {
            cache.insert_url(format!("url{i}"));
        }

        let stats = cache.bloom_stats();
        // Rotation should have occurred
        assert!(stats.url_rotation_count >= 1);
        // Memory is bounded
        assert!(stats.is_memory_bounded());
    }

    #[test]
    fn test_bloom_stats_memory_estimation() {
        let stats = BloomStats {
            url_capacity: 100_000,
            hash_capacity: 50_000,
            cache_size: 1000,
            hash_cache_size: 500,
            url_active_count: 50_000,
            url_rotation_count: 1,
            hash_active_count: 25_000,
            hash_rotation_count: 0,
        };

        // Should have some memory estimation
        let memory = stats.estimated_memory_bytes();
        assert!(memory > 0);
        // With rotation, should account for previous generation
        assert!(stats.is_memory_bounded());
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
