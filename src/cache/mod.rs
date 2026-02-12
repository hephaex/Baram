//! Redis caching layer for embeddings and search results
//!
//! This module provides caching functionality to improve performance:
//! - Embedding cache: Cache text embeddings by content hash
//! - Search cache: Cache search query results with short TTL
//! - Metadata cache: Cache frequently accessed article metadata
//!
//! # Example
//!
//! ```rust,ignore
//! use baram::cache::{Cache, CacheConfig};
//!
//! let config = CacheConfig::from_env()?;
//! let cache = Cache::new(&config).await?;
//!
//! // Cache embedding
//! cache.set_embedding("hash123", &embedding).await?;
//! let cached = cache.get_embedding("hash123").await?;
//! ```

use anyhow::{Context, Result};
use deadpool_redis::{Config as PoolConfig, Pool, Runtime};
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Redis URL (e.g., redis://localhost:6379)
    pub url: String,

    /// Connection pool size
    pub pool_size: usize,

    /// Embedding cache TTL in seconds (default: 24 hours)
    pub embedding_ttl: u64,

    /// Search cache TTL in seconds (default: 5 minutes)
    pub search_ttl: u64,

    /// Metadata cache TTL in seconds (default: 1 hour)
    pub metadata_ttl: u64,

    /// Key prefix for namespacing
    pub key_prefix: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            pool_size: 10,
            embedding_ttl: 86400, // 24 hours
            search_ttl: 300,      // 5 minutes
            metadata_ttl: 3600,   // 1 hour
            key_prefix: "baram".to_string(),
        }
    }
}

impl CacheConfig {
    /// Create config from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            pool_size: std::env::var("REDIS_POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            embedding_ttl: std::env::var("CACHE_EMBEDDING_TTL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(86400),
            search_ttl: std::env::var("CACHE_SEARCH_TTL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            metadata_ttl: std::env::var("CACHE_METADATA_TTL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600),
            key_prefix: std::env::var("CACHE_KEY_PREFIX").unwrap_or_else(|_| "baram".to_string()),
        })
    }
}

/// Cached embedding data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEmbedding {
    /// The embedding vector
    pub embedding: Vec<f32>,
    /// Model used for generation
    pub model: String,
    /// Timestamp when cached
    pub cached_at: i64,
}

/// Cached search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSearchResult {
    /// Search results
    pub results: Vec<SearchResultItem>,
    /// Query that was executed
    pub query: String,
    /// Timestamp when cached
    pub cached_at: i64,
}

/// Individual search result item for caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: String,
    pub score: f32,
    pub title: String,
    pub content: String,
    pub category: String,
    pub publisher: Option<String>,
    pub url: String,
    pub published_at: Option<String>,
    pub highlights: Option<Vec<String>>,
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total cache hits
    pub hits: u64,
    /// Total cache misses
    pub misses: u64,
    /// Total bytes read from cache
    pub bytes_read: u64,
    /// Total bytes written to cache
    pub bytes_written: u64,
}

impl CacheStats {
    /// Calculate hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Redis cache client
pub struct Cache {
    /// Connection pool
    pool: Pool,
    /// Configuration
    config: CacheConfig,
    /// Statistics (reserved for future use)
    #[allow(dead_code)]
    stats: std::sync::atomic::AtomicU64,
}

impl Cache {
    /// Create a new cache instance
    pub async fn new(config: &CacheConfig) -> Result<Self> {
        let pool_config = PoolConfig::from_url(&config.url);
        let pool = pool_config
            .builder()
            .map_err(|e| anyhow::anyhow!("Failed to create pool builder: {e}"))?
            .max_size(config.pool_size)
            .runtime(Runtime::Tokio1)
            .build()
            .context("Failed to create Redis connection pool")?;

        // Test connection
        let mut conn = pool.get().await.context("Failed to get Redis connection")?;

        let _: String = redis::cmd("PING")
            .query_async(&mut *conn)
            .await
            .context("Failed to ping Redis")?;

        tracing::info!(url = %config.url, "Connected to Redis");

        Ok(Self {
            pool,
            config: config.clone(),
            stats: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Create a cache instance, returning None if Redis is unavailable
    pub async fn try_new(config: &CacheConfig) -> Option<Self> {
        match Self::new(config).await {
            Ok(cache) => Some(cache),
            Err(e) => {
                tracing::warn!(error = %e, "Redis cache unavailable, continuing without cache");
                None
            }
        }
    }

    // =========================================================================
    // Key Generation
    // =========================================================================

    /// Generate cache key for embedding
    fn embedding_key(&self, content_hash: &str) -> String {
        format!("{}:embed:{}", self.config.key_prefix, content_hash)
    }

    /// Generate cache key for search query
    fn search_key(&self, query_hash: &str) -> String {
        format!("{}:search:{}", self.config.key_prefix, query_hash)
    }

    /// Generate cache key for article metadata
    fn metadata_key(&self, article_id: &str) -> String {
        format!("{}:meta:{}", self.config.key_prefix, article_id)
    }

    /// Hash content for cache key
    pub fn hash_content(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Hash search query for cache key
    pub fn hash_query(query: &str, k: usize, category: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(query.as_bytes());
        hasher.update(k.to_le_bytes());
        if let Some(cat) = category {
            hasher.update(cat.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    // =========================================================================
    // Embedding Cache
    // =========================================================================

    /// Get cached embedding by content hash
    pub async fn get_embedding(&self, content_hash: &str) -> Result<Option<CachedEmbedding>> {
        self.get(&self.embedding_key(content_hash)).await
    }

    /// Cache an embedding
    pub async fn set_embedding(
        &self,
        content_hash: &str,
        embedding: &[f32],
        model: &str,
    ) -> Result<()> {
        let cached = CachedEmbedding {
            embedding: embedding.to_vec(),
            model: model.to_string(),
            cached_at: chrono::Utc::now().timestamp(),
        };
        self.set(
            &self.embedding_key(content_hash),
            &cached,
            Duration::from_secs(self.config.embedding_ttl),
        )
        .await
    }

    /// Get or compute embedding with caching
    pub async fn get_or_compute_embedding<F, Fut>(
        &self,
        content: &str,
        model: &str,
        compute_fn: F,
    ) -> Result<Vec<f32>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Vec<f32>>>,
    {
        let content_hash = Self::hash_content(content);

        // Try to get from cache
        if let Some(cached) = self.get_embedding(&content_hash).await? {
            tracing::debug!(hash = %content_hash, "Embedding cache hit");
            return Ok(cached.embedding);
        }

        tracing::debug!(hash = %content_hash, "Embedding cache miss");

        // Compute embedding
        let embedding = compute_fn().await?;

        // Cache the result
        if let Err(e) = self.set_embedding(&content_hash, &embedding, model).await {
            tracing::warn!(error = %e, "Failed to cache embedding");
        }

        Ok(embedding)
    }

    // =========================================================================
    // Search Cache
    // =========================================================================

    /// Get cached search results
    pub async fn get_search(&self, query_hash: &str) -> Result<Option<CachedSearchResult>> {
        self.get(&self.search_key(query_hash)).await
    }

    /// Cache search results
    pub async fn set_search(&self, query_hash: &str, results: &CachedSearchResult) -> Result<()> {
        self.set(
            &self.search_key(query_hash),
            results,
            Duration::from_secs(self.config.search_ttl),
        )
        .await
    }

    /// Invalidate search cache (called when new articles are indexed)
    pub async fn invalidate_search_cache(&self) -> Result<u64> {
        let pattern = format!("{}:search:*", self.config.key_prefix);
        self.delete_pattern(&pattern).await
    }

    // =========================================================================
    // Metadata Cache
    // =========================================================================

    /// Get cached article metadata
    pub async fn get_metadata<T: DeserializeOwned>(&self, article_id: &str) -> Result<Option<T>> {
        self.get(&self.metadata_key(article_id)).await
    }

    /// Cache article metadata
    pub async fn set_metadata<T: Serialize>(&self, article_id: &str, metadata: &T) -> Result<()> {
        self.set(
            &self.metadata_key(article_id),
            metadata,
            Duration::from_secs(self.config.metadata_ttl),
        )
        .await
    }

    // =========================================================================
    // Generic Operations
    // =========================================================================

    /// Get value from cache
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let mut conn = self.pool.get().await.context("Failed to get connection")?;

        let value: Option<Vec<u8>> = conn.get(key).await.context("Failed to get from cache")?;

        match value {
            Some(bytes) => {
                let decoded: T = rmp_serde_decode(&bytes)?;
                Ok(Some(decoded))
            }
            None => Ok(None),
        }
    }

    /// Set value in cache with TTL
    async fn set<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) -> Result<()> {
        let mut conn = self.pool.get().await.context("Failed to get connection")?;

        let bytes = rmp_serde_encode(value)?;

        conn.set_ex::<_, _, ()>(key, bytes, ttl.as_secs())
            .await
            .context("Failed to set cache")?;

        Ok(())
    }

    /// Delete keys matching pattern
    async fn delete_pattern(&self, pattern: &str) -> Result<u64> {
        let mut conn = self.pool.get().await.context("Failed to get connection")?;

        // Use SCAN to find keys (safer for production)
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(pattern)
            .query_async(&mut *conn)
            .await
            .context("Failed to scan keys")?;

        if keys.is_empty() {
            return Ok(0);
        }

        let count = keys.len() as u64;

        // Delete all matching keys
        let _: () = conn.del(keys).await.context("Failed to delete keys")?;

        tracing::info!(pattern = %pattern, count = count, "Invalidated cache entries");

        Ok(count)
    }

    /// Check if cache is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let mut conn = self.pool.get().await?;
        let result: String = redis::cmd("PING").query_async(&mut *conn).await?;
        Ok(result == "PONG")
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> Result<CacheInfo> {
        let mut conn = self.pool.get().await?;

        let info: String = redis::cmd("INFO")
            .arg("stats")
            .query_async(&mut *conn)
            .await?;

        // Parse basic stats
        let mut hits = 0u64;
        let mut misses = 0u64;
        let mut memory = 0u64;

        for line in info.lines() {
            if let Some(value) = line.strip_prefix("keyspace_hits:") {
                hits = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = line.strip_prefix("keyspace_misses:") {
                misses = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = line.strip_prefix("used_memory:") {
                memory = value.trim().parse().unwrap_or(0);
            }
        }

        Ok(CacheInfo {
            hits,
            misses,
            memory_bytes: memory,
            hit_rate: if hits + misses > 0 {
                hits as f64 / (hits + misses) as f64
            } else {
                0.0
            },
        })
    }

    /// Get config reference
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }
}

/// Cache information
#[derive(Debug, Clone)]
pub struct CacheInfo {
    pub hits: u64,
    pub misses: u64,
    pub memory_bytes: u64,
    pub hit_rate: f64,
}

// ============================================================================
// Serialization helpers using JSON (simpler than MessagePack for now)
// ============================================================================

fn rmp_serde_encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).context("Failed to serialize value")
}

fn rmp_serde_decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).context("Failed to deserialize value")
}

// ============================================================================
// Optional cache wrapper for graceful degradation
// ============================================================================

/// Optional cache that gracefully handles Redis unavailability
pub struct OptionalCache {
    inner: Option<Cache>,
}

impl OptionalCache {
    /// Create with an optional cache
    pub fn new(cache: Option<Cache>) -> Self {
        Self { inner: cache }
    }

    /// Create from config, returning empty cache if Redis unavailable
    pub async fn from_config(config: &CacheConfig) -> Self {
        Self {
            inner: Cache::try_new(config).await,
        }
    }

    /// Check if cache is available
    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    /// Get cached embedding
    pub async fn get_embedding(&self, content_hash: &str) -> Option<CachedEmbedding> {
        match &self.inner {
            Some(cache) => cache.get_embedding(content_hash).await.ok().flatten(),
            None => None,
        }
    }

    /// Set cached embedding
    pub async fn set_embedding(&self, content_hash: &str, embedding: &[f32], model: &str) {
        if let Some(cache) = &self.inner {
            let _ = cache.set_embedding(content_hash, embedding, model).await;
        }
    }

    /// Get cached search results
    pub async fn get_search(&self, query_hash: &str) -> Option<CachedSearchResult> {
        match &self.inner {
            Some(cache) => cache.get_search(query_hash).await.ok().flatten(),
            None => None,
        }
    }

    /// Set cached search results
    pub async fn set_search(&self, query_hash: &str, results: &CachedSearchResult) {
        if let Some(cache) = &self.inner {
            let _ = cache.set_search(query_hash, results).await;
        }
    }

    /// Invalidate search cache
    pub async fn invalidate_search_cache(&self) -> u64 {
        match &self.inner {
            Some(cache) => cache.invalidate_search_cache().await.unwrap_or(0),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.embedding_ttl, 86400);
        assert_eq!(config.search_ttl, 300);
        assert_eq!(config.metadata_ttl, 3600);
    }

    #[test]
    fn test_hash_content() {
        let hash1 = Cache::hash_content("test content");
        let hash2 = Cache::hash_content("test content");
        let hash3 = Cache::hash_content("different content");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA256 hex
    }

    #[test]
    fn test_hash_query() {
        let hash1 = Cache::hash_query("test query", 10, None);
        let hash2 = Cache::hash_query("test query", 10, None);
        let hash3 = Cache::hash_query("test query", 10, Some("politics"));

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);

        stats.hits = 75;
        stats.misses = 25;
        assert!((stats.hit_rate() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_cached_embedding_serialization() {
        let cached = CachedEmbedding {
            embedding: vec![0.1, 0.2, 0.3],
            model: "test-model".to_string(),
            cached_at: 1234567890,
        };

        let bytes = rmp_serde_encode(&cached).unwrap();
        let decoded: CachedEmbedding = rmp_serde_decode(&bytes).unwrap();

        assert_eq!(decoded.embedding.len(), 3);
        assert_eq!(decoded.model, "test-model");
    }

    #[test]
    fn test_optional_cache_unavailable() {
        let cache = OptionalCache::new(None);
        assert!(!cache.is_available());
    }

    // Integration tests require running Redis
    #[tokio::test]
    #[ignore = "Requires running Redis"]
    async fn test_cache_connection() {
        let config = CacheConfig::default();
        let cache = Cache::new(&config).await;
        assert!(cache.is_ok());
    }

    #[tokio::test]
    #[ignore = "Requires running Redis"]
    async fn test_embedding_cache() {
        let config = CacheConfig::default();
        let cache = Cache::new(&config).await.unwrap();

        let hash = "test_hash_123";
        let embedding = vec![0.1, 0.2, 0.3, 0.4];

        // Set embedding
        cache
            .set_embedding(hash, &embedding, "test-model")
            .await
            .unwrap();

        // Get embedding
        let cached = cache.get_embedding(hash).await.unwrap();
        assert!(cached.is_some());

        let cached = cached.unwrap();
        assert_eq!(cached.embedding.len(), 4);
        assert_eq!(cached.model, "test-model");
    }
}
