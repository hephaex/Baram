//! Repository pattern abstractions for database operations
//!
//! This module provides trait-based abstractions for data access,
//! enabling:
//! - Loose coupling between business logic and storage implementations
//! - Easy testing with mock implementations
//! - Swappable storage backends
//!
//! # Repository Traits
//!
//! - [`ArticleRepository`] - PostgreSQL article storage operations
//! - [`CrawlMetadataRepository`] - SQLite crawl metadata operations
//! - [`CheckpointRepository`] - State checkpoint operations
//!
//! # Example
//!
//! ```ignore
//! use baram::storage::repository::{ArticleRepository, CrawlMetadataRepository};
//!
//! async fn process_article<R: ArticleRepository>(repo: &R, article: &Article) -> Result<()> {
//!     repo.store(article).await?;
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashSet;

use crate::models::ParsedArticle;
use crate::parser::Article;

use super::{CrawlRecord, CrawlStats, CrawlStatus};

// ============================================================================
// Article Repository (PostgreSQL)
// ============================================================================

/// Repository trait for article storage operations
///
/// Abstracts PostgreSQL operations for storing and retrieving articles.
/// Note: Uses `?Send` as the implementing Database contains SQLite connection which is not Sync.
#[async_trait(?Send)]
pub trait ArticleRepository {
    /// Store an article
    async fn store(&self, article: &Article) -> Result<()>;

    /// Retrieve an article by ID
    async fn get(&self, id: &str) -> Result<Option<Article>>;

    /// Check if an article exists by URL
    async fn exists_by_url(&self, url: &str) -> Result<bool>;

    /// Delete an article by ID
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Get articles by category with pagination
    async fn find_by_category(
        &self,
        category: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Article>>;

    /// Count articles by category
    async fn count_by_category(&self, category: &str) -> Result<usize>;
}

// ============================================================================
// Crawl Metadata Repository (SQLite)
// ============================================================================

/// Repository trait for crawl metadata operations
///
/// Abstracts SQLite operations for tracking crawled URLs and deduplication.
/// Note: Does not require `Sync` as SQLite Connection is not thread-safe.
pub trait CrawlMetadataRepository: Send {
    /// Check if URL has been successfully crawled
    fn is_url_crawled(&self, url: &str) -> Result<bool>;

    /// Check if content hash already exists (deduplication)
    fn is_content_duplicate(&self, hash: &str) -> Result<bool>;

    /// Mark URL as crawled with status
    fn mark_crawled(
        &self,
        id: &str,
        url: &str,
        content_hash: &str,
        status: CrawlStatus,
        error_message: Option<&str>,
    ) -> Result<()>;

    /// Record successful crawl from parsed article
    fn record_success(&self, article: &ParsedArticle) -> Result<()>;

    /// Record failed crawl
    fn record_failure(&self, url: &str, error: &str) -> Result<()>;

    /// Get crawl record by URL
    fn get_record(&self, url: &str) -> Result<Option<CrawlRecord>>;

    /// Get crawl statistics
    fn get_stats(&self) -> Result<CrawlStats>;

    /// Filter URLs that haven't been crawled (batch operation)
    fn filter_uncrawled(&self, urls: &[String]) -> Result<Vec<String>>;

    /// Batch check URLs for crawl status
    fn batch_check_urls(&self, urls: &[String]) -> Result<Vec<(String, bool)>>;
}

// ============================================================================
// Checkpoint Repository
// ============================================================================

/// Repository trait for checkpoint/state operations
/// Note: Does not require `Sync` as SQLite Connection is not thread-safe.
pub trait CheckpointRepository: Send {
    /// Save checkpoint state
    fn save(&self, key: &str, value: &str) -> Result<()>;

    /// Load checkpoint state
    fn load(&self, key: &str) -> Result<Option<String>>;

    /// Delete checkpoint
    fn delete(&self, key: &str) -> Result<bool>;

    /// List all checkpoint keys
    fn list_keys(&self) -> Result<Vec<String>>;
}

// ============================================================================
// Mock Implementations for Testing
// ============================================================================

/// Mock article repository for testing
#[derive(Default)]
pub struct MockArticleRepository {
    articles: std::sync::RwLock<std::collections::HashMap<String, Article>>,
}

impl MockArticleRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all stored articles (for testing)
    pub fn get_all(&self) -> Vec<Article> {
        self.articles.read().unwrap().values().cloned().collect()
    }

    /// Clear all articles (for testing)
    pub fn clear(&self) {
        self.articles.write().unwrap().clear();
    }
}

#[async_trait(?Send)]
impl ArticleRepository for MockArticleRepository {
    async fn store(&self, article: &Article) -> Result<()> {
        self.articles
            .write()
            .unwrap()
            .insert(article.id.to_string(), article.clone());
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<Article>> {
        Ok(self.articles.read().unwrap().get(id).cloned())
    }

    async fn exists_by_url(&self, url: &str) -> Result<bool> {
        Ok(self
            .articles
            .read()
            .unwrap()
            .values()
            .any(|a| a.url == url))
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        Ok(self.articles.write().unwrap().remove(id).is_some())
    }

    async fn find_by_category(
        &self,
        category: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Article>> {
        let articles: Vec<_> = self
            .articles
            .read()
            .unwrap()
            .values()
            .filter(|a| a.category.as_deref() == Some(category))
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(articles)
    }

    async fn count_by_category(&self, category: &str) -> Result<usize> {
        let count = self
            .articles
            .read()
            .unwrap()
            .values()
            .filter(|a| a.category.as_deref() == Some(category))
            .count();
        Ok(count)
    }
}

/// Mock crawl metadata repository for testing
#[derive(Default)]
pub struct MockCrawlMetadataRepository {
    records: std::sync::RwLock<std::collections::HashMap<String, CrawlRecord>>,
    hashes: std::sync::RwLock<HashSet<String>>,
}

impl MockCrawlMetadataRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all records (for testing)
    pub fn get_all_records(&self) -> Vec<CrawlRecord> {
        self.records.read().unwrap().values().cloned().collect()
    }

    /// Clear all data (for testing)
    pub fn clear(&self) {
        self.records.write().unwrap().clear();
        self.hashes.write().unwrap().clear();
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
        Ok(self.hashes.read().unwrap().contains(hash))
    }

    fn mark_crawled(
        &self,
        id: &str,
        url: &str,
        content_hash: &str,
        status: CrawlStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let record = CrawlRecord {
            id: id.to_string(),
            url: url.to_string(),
            content_hash: content_hash.to_string(),
            crawled_at: chrono::Utc::now(),
            status,
            error_message: error_message.map(String::from),
        };

        self.records.write().unwrap().insert(url.to_string(), record);

        if !content_hash.is_empty() {
            self.hashes.write().unwrap().insert(content_hash.to_string());
        }

        Ok(())
    }

    fn record_success(&self, article: &ParsedArticle) -> Result<()> {
        let hash = article.content_hash.as_deref().unwrap_or("");
        self.mark_crawled(&article.id(), &article.url, hash, CrawlStatus::Success, None)
    }

    fn record_failure(&self, url: &str, error: &str) -> Result<()> {
        self.mark_crawled("", url, "", CrawlStatus::Failed, Some(error))
    }

    fn get_record(&self, url: &str) -> Result<Option<CrawlRecord>> {
        Ok(self.records.read().unwrap().get(url).cloned())
    }

    fn get_stats(&self) -> Result<CrawlStats> {
        let records = self.records.read().unwrap();
        let total = records.len();
        let success = records
            .values()
            .filter(|r| r.status == CrawlStatus::Success)
            .count();
        let failed = records
            .values()
            .filter(|r| r.status == CrawlStatus::Failed)
            .count();
        let skipped = records
            .values()
            .filter(|r| r.status == CrawlStatus::Skipped)
            .count();

        Ok(CrawlStats {
            total,
            success,
            failed,
            skipped,
        })
    }

    fn filter_uncrawled(&self, urls: &[String]) -> Result<Vec<String>> {
        let records = self.records.read().unwrap();
        let uncrawled: Vec<String> = urls
            .iter()
            .filter(|url| {
                !records
                    .get(*url)
                    .map(|r| r.status == CrawlStatus::Success)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        Ok(uncrawled)
    }

    fn batch_check_urls(&self, urls: &[String]) -> Result<Vec<(String, bool)>> {
        let records = self.records.read().unwrap();
        let results: Vec<_> = urls
            .iter()
            .map(|url| {
                let is_crawled = records
                    .get(url)
                    .map(|r| r.status == CrawlStatus::Success)
                    .unwrap_or(false);
                (url.clone(), is_crawled)
            })
            .collect();
        Ok(results)
    }
}

/// Mock checkpoint repository for testing
#[derive(Default)]
pub struct MockCheckpointRepository {
    checkpoints: std::sync::RwLock<std::collections::HashMap<String, String>>,
}

impl MockCheckpointRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all checkpoints (for testing)
    pub fn clear(&self) {
        self.checkpoints.write().unwrap().clear();
    }
}

impl CheckpointRepository for MockCheckpointRepository {
    fn save(&self, key: &str, value: &str) -> Result<()> {
        self.checkpoints
            .write()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Option<String>> {
        Ok(self.checkpoints.read().unwrap().get(key).cloned())
    }

    fn delete(&self, key: &str) -> Result<bool> {
        Ok(self.checkpoints.write().unwrap().remove(key).is_some())
    }

    fn list_keys(&self) -> Result<Vec<String>> {
        Ok(self.checkpoints.read().unwrap().keys().cloned().collect())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_mock_article_repository() {
        let repo = MockArticleRepository::new();

        let article = Article {
            id: Uuid::new_v4(),
            url: "https://example.com/article1".to_string(),
            title: "Test Article".to_string(),
            body: "Article body".to_string(),
            author: Some("Author".to_string()),
            published_at: None,
            category: Some("news".to_string()),
            content_hash: "hash123".to_string(),
            comments: vec![],
        };

        // Store article
        repo.store(&article).await.unwrap();

        // Retrieve article
        let retrieved = repo.get(&article.id.to_string()).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test Article");

        // Check exists by URL
        assert!(repo.exists_by_url("https://example.com/article1").await.unwrap());
        assert!(!repo.exists_by_url("https://example.com/nonexistent").await.unwrap());

        // Find by category
        let articles = repo.find_by_category("news", 10, 0).await.unwrap();
        assert_eq!(articles.len(), 1);

        // Count by category
        assert_eq!(repo.count_by_category("news").await.unwrap(), 1);
        assert_eq!(repo.count_by_category("other").await.unwrap(), 0);

        // Delete article
        assert!(repo.delete(&article.id.to_string()).await.unwrap());
        assert!(repo.get(&article.id.to_string()).await.unwrap().is_none());
    }

    #[test]
    fn test_mock_crawl_metadata_repository() {
        let repo = MockCrawlMetadataRepository::new();

        // Initially not crawled
        assert!(!repo.is_url_crawled("https://example.com/article1").unwrap());

        // Mark as crawled
        repo.mark_crawled(
            "001_001",
            "https://example.com/article1",
            "hash123",
            CrawlStatus::Success,
            None,
        )
        .unwrap();

        // Now should be crawled
        assert!(repo.is_url_crawled("https://example.com/article1").unwrap());

        // Hash should exist
        assert!(repo.is_content_duplicate("hash123").unwrap());

        // Get record
        let record = repo.get_record("https://example.com/article1").unwrap();
        assert!(record.is_some());
        assert_eq!(record.unwrap().id, "001_001");

        // Get stats
        let stats = repo.get_stats().unwrap();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    fn test_mock_crawl_metadata_batch_operations() {
        let repo = MockCrawlMetadataRepository::new();

        // Add some crawled URLs
        repo.mark_crawled("1", "https://example.com/1", "h1", CrawlStatus::Success, None)
            .unwrap();
        repo.mark_crawled("2", "https://example.com/2", "h2", CrawlStatus::Success, None)
            .unwrap();

        // Filter uncrawled
        let urls = vec![
            "https://example.com/1".to_string(),
            "https://example.com/2".to_string(),
            "https://example.com/3".to_string(),
        ];

        let uncrawled = repo.filter_uncrawled(&urls).unwrap();
        assert_eq!(uncrawled.len(), 1);
        assert_eq!(uncrawled[0], "https://example.com/3");

        // Batch check
        let results = repo.batch_check_urls(&urls).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].1); // URL 1 is crawled
        assert!(results[1].1); // URL 2 is crawled
        assert!(!results[2].1); // URL 3 is not crawled
    }

    #[test]
    fn test_mock_checkpoint_repository() {
        let repo = MockCheckpointRepository::new();

        // Save checkpoint
        repo.save("last_page", "5").unwrap();

        // Load checkpoint
        let value = repo.load("last_page").unwrap();
        assert_eq!(value, Some("5".to_string()));

        // Load nonexistent
        assert!(repo.load("nonexistent").unwrap().is_none());

        // List keys
        let keys = repo.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"last_page".to_string()));

        // Delete checkpoint
        assert!(repo.delete("last_page").unwrap());
        assert!(repo.load("last_page").unwrap().is_none());
    }

    #[test]
    fn test_record_success_and_failure() {
        let repo = MockCrawlMetadataRepository::new();

        // Record success
        let article = ParsedArticle {
            oid: "001".to_string(),
            aid: "0001".to_string(),
            url: "https://example.com/success".to_string(),
            content_hash: Some("hash_success".to_string()),
            ..Default::default()
        };
        repo.record_success(&article).unwrap();

        // Record failure
        repo.record_failure("https://example.com/failed", "Connection timeout")
            .unwrap();

        // Verify stats
        let stats = repo.get_stats().unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.failed, 1);
    }
}
