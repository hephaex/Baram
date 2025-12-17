//! Checkpoint system for resumable crawling
//!
//! This module provides checkpoint management for crawler pipelines,
//! allowing interrupted crawls to resume from the last saved state.
//!
//! # Features
//!
//! - Automatic periodic checkpointing
//! - Crash recovery with state restoration
//! - Progress tracking across sessions
//! - JSON-based state serialization
//!
//! # Example
//!
//! ```no_run
//! use ntimes::storage::checkpoint::{CheckpointManager, CrawlState};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let manager = CheckpointManager::new(Path::new("./checkpoints"))?;
//!
//! // Save checkpoint
//! let state = CrawlState::new("politics", 100);
//! manager.save("crawl_session_1", &state)?;
//!
//! // Load checkpoint
//! if let Some(restored) = manager.load::<CrawlState>("crawl_session_1")? {
//!     println!("Resuming from URL index: {}", restored.current_index);
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Checkpoint State Types
// ============================================================================

/// State of a crawling session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlState {
    /// Session identifier
    pub session_id: String,

    /// Category being crawled
    pub category: String,

    /// Total URLs to process
    pub total_urls: usize,

    /// Current index in URL list
    pub current_index: usize,

    /// Successfully processed URLs
    pub processed_urls: HashSet<String>,

    /// Failed URLs (for retry)
    pub failed_urls: Vec<FailedUrl>,

    /// Skipped URLs (duplicates)
    pub skipped_urls: HashSet<String>,

    /// Checkpoint creation time
    pub created_at: DateTime<Utc>,

    /// Last update time
    pub updated_at: DateTime<Utc>,

    /// Pipeline statistics snapshot
    pub stats: CheckpointStats,
}

impl CrawlState {
    /// Create a new crawl state
    pub fn new(category: &str, total_urls: usize) -> Self {
        let now = Utc::now();
        Self {
            session_id: generate_session_id(),
            category: category.to_string(),
            total_urls,
            current_index: 0,
            processed_urls: HashSet::new(),
            failed_urls: Vec::new(),
            skipped_urls: HashSet::new(),
            created_at: now,
            updated_at: now,
            stats: CheckpointStats::default(),
        }
    }

    /// Mark URL as processed
    pub fn mark_processed(&mut self, url: &str) {
        self.processed_urls.insert(url.to_string());
        self.current_index += 1;
        self.updated_at = Utc::now();
    }

    /// Mark URL as failed
    pub fn mark_failed(&mut self, url: &str, error: &str) {
        self.failed_urls.push(FailedUrl {
            url: url.to_string(),
            error: error.to_string(),
            attempts: 1,
            last_attempt: Utc::now(),
        });
        self.current_index += 1;
        self.updated_at = Utc::now();
    }

    /// Mark URL as skipped
    pub fn mark_skipped(&mut self, url: &str) {
        self.skipped_urls.insert(url.to_string());
        self.current_index += 1;
        self.updated_at = Utc::now();
    }

    /// Get remaining URLs count
    pub fn remaining(&self) -> usize {
        self.total_urls.saturating_sub(self.current_index)
    }

    /// Get completion percentage
    pub fn completion_percentage(&self) -> f64 {
        if self.total_urls == 0 {
            return 100.0;
        }
        (self.current_index as f64 / self.total_urls as f64) * 100.0
    }

    /// Check if crawl is complete
    pub fn is_complete(&self) -> bool {
        self.current_index >= self.total_urls
    }

    /// Get URLs that need retry
    pub fn get_retry_urls(&self) -> Vec<String> {
        self.failed_urls
            .iter()
            .filter(|f| f.attempts < 3) // Max 3 retries
            .map(|f| f.url.clone())
            .collect()
    }
}

/// Failed URL record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedUrl {
    /// URL that failed
    pub url: String,

    /// Error message
    pub error: String,

    /// Number of attempts
    pub attempts: u32,

    /// Last attempt time
    pub last_attempt: DateTime<Utc>,
}

/// Statistics snapshot for checkpoint
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckpointStats {
    /// Successfully processed count
    pub success_count: u64,

    /// Failed count
    pub failed_count: u64,

    /// Skipped count
    pub skipped_count: u64,

    /// Bytes fetched
    pub bytes_fetched: u64,

    /// Articles with comments
    pub with_comments: u64,

    /// Processing duration in seconds
    pub duration_secs: u64,
}

// ============================================================================
// Checkpoint Manager
// ============================================================================

/// Manages checkpoint files for crawler sessions
pub struct CheckpointManager {
    /// Directory for checkpoint files
    checkpoint_dir: PathBuf,

    /// Auto-save interval (in processed items)
    auto_save_interval: usize,

    /// Current item counter
    item_counter: AtomicU64,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(checkpoint_dir: &Path) -> Result<Self> {
        fs::create_dir_all(checkpoint_dir).context("Failed to create checkpoint directory")?;

        Ok(Self {
            checkpoint_dir: checkpoint_dir.to_path_buf(),
            auto_save_interval: 100,
            item_counter: AtomicU64::new(0),
        })
    }

    /// Create with custom auto-save interval
    pub fn with_interval(checkpoint_dir: &Path, interval: usize) -> Result<Self> {
        let mut manager = Self::new(checkpoint_dir)?;
        manager.auto_save_interval = interval;
        Ok(manager)
    }

    /// Save checkpoint state
    pub fn save<T: Serialize>(&self, name: &str, state: &T) -> Result<PathBuf> {
        let filename = format!("{name}.checkpoint.json");
        let filepath = self.checkpoint_dir.join(&filename);

        // Write to temp file first, then rename (atomic)
        let temp_path = self.checkpoint_dir.join(format!("{filename}.tmp"));

        let file = File::create(&temp_path).with_context(|| {
            format!("Failed to create checkpoint file: {}", temp_path.display())
        })?;

        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, state).context("Failed to serialize checkpoint")?;

        // Atomic rename
        fs::rename(&temp_path, &filepath)
            .with_context(|| format!("Failed to rename checkpoint file: {}", filepath.display()))?;

        tracing::debug!(path = %filepath.display(), "Checkpoint saved");
        Ok(filepath)
    }

    /// Load checkpoint state
    pub fn load<T: for<'de> Deserialize<'de>>(&self, name: &str) -> Result<Option<T>> {
        let filename = format!("{name}.checkpoint.json");
        let filepath = self.checkpoint_dir.join(&filename);

        if !filepath.exists() {
            return Ok(None);
        }

        let file = File::open(&filepath)
            .with_context(|| format!("Failed to open checkpoint file: {}", filepath.display()))?;

        let reader = BufReader::new(file);
        let state = serde_json::from_reader(reader).context("Failed to deserialize checkpoint")?;

        tracing::debug!(path = %filepath.display(), "Checkpoint loaded");
        Ok(Some(state))
    }

    /// Check if checkpoint exists
    pub fn exists(&self, name: &str) -> bool {
        let filename = format!("{name}.checkpoint.json");
        self.checkpoint_dir.join(filename).exists()
    }

    /// Delete checkpoint
    pub fn delete(&self, name: &str) -> Result<()> {
        let filename = format!("{name}.checkpoint.json");
        let filepath = self.checkpoint_dir.join(&filename);

        if filepath.exists() {
            fs::remove_file(&filepath)
                .with_context(|| format!("Failed to delete checkpoint: {}", filepath.display()))?;
            tracing::debug!(path = %filepath.display(), "Checkpoint deleted");
        }

        Ok(())
    }

    /// List all checkpoints
    pub fn list(&self) -> Result<Vec<String>> {
        let mut checkpoints = Vec::new();

        for entry in fs::read_dir(&self.checkpoint_dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".checkpoint.json") {
                    let session_name = name.trim_end_matches(".checkpoint.json").to_string();
                    checkpoints.push(session_name);
                }
            }
        }

        Ok(checkpoints)
    }

    /// Increment counter and check if auto-save needed
    pub fn should_auto_save(&self) -> bool {
        let count = self.item_counter.fetch_add(1, Ordering::Relaxed) + 1;
        count % (self.auto_save_interval as u64) == 0
    }

    /// Reset item counter
    pub fn reset_counter(&self) {
        self.item_counter.store(0, Ordering::Relaxed);
    }

    /// Get checkpoint directory
    pub fn checkpoint_dir(&self) -> &Path {
        &self.checkpoint_dir
    }
}

// ============================================================================
// Async Checkpoint Manager
// ============================================================================

/// Thread-safe checkpoint manager for async pipelines
pub struct AsyncCheckpointManager {
    inner: CheckpointManager,
    current_state: Arc<RwLock<Option<CrawlState>>>,
}

impl AsyncCheckpointManager {
    /// Create a new async checkpoint manager
    pub fn new(checkpoint_dir: &Path) -> Result<Self> {
        Ok(Self {
            inner: CheckpointManager::new(checkpoint_dir)?,
            current_state: Arc::new(RwLock::new(None)),
        })
    }

    /// Initialize or resume a crawl session
    pub async fn init_session(&self, category: &str, urls: &[String]) -> Result<CrawlState> {
        let session_name = format!("{}_{}", category, Utc::now().format("%Y%m%d"));

        // Try to load existing checkpoint
        if let Some(mut state) = self.inner.load::<CrawlState>(&session_name)? {
            // Filter out already processed URLs
            let remaining: Vec<String> = urls
                .iter()
                .filter(|url| {
                    !state.processed_urls.contains(*url) && !state.skipped_urls.contains(*url)
                })
                .cloned()
                .collect();

            state.total_urls = urls.len();
            tracing::info!(
                session = session_name,
                remaining = remaining.len(),
                processed = state.processed_urls.len(),
                "Resuming crawl session"
            );

            *self.current_state.write().await = Some(state.clone());
            return Ok(state);
        }

        // Create new session
        let state = CrawlState::new(category, urls.len());

        tracing::info!(
            session = session_name,
            total = urls.len(),
            "Starting new crawl session"
        );

        *self.current_state.write().await = Some(state.clone());
        Ok(state)
    }

    /// Update state with processed URL
    pub async fn mark_processed(&self, url: &str) -> Result<()> {
        let mut state_guard = self.current_state.write().await;
        if let Some(state) = state_guard.as_mut() {
            state.mark_processed(url);

            // Auto-save check
            if self.inner.should_auto_save() {
                let session_name =
                    format!("{}_{}", state.category, state.created_at.format("%Y%m%d"));
                self.inner.save(&session_name, state)?;
            }
        }
        Ok(())
    }

    /// Update state with failed URL
    pub async fn mark_failed(&self, url: &str, error: &str) -> Result<()> {
        let mut state_guard = self.current_state.write().await;
        if let Some(state) = state_guard.as_mut() {
            state.mark_failed(url, error);

            if self.inner.should_auto_save() {
                let session_name =
                    format!("{}_{}", state.category, state.created_at.format("%Y%m%d"));
                self.inner.save(&session_name, state)?;
            }
        }
        Ok(())
    }

    /// Update state with skipped URL
    pub async fn mark_skipped(&self, url: &str) -> Result<()> {
        let mut state_guard = self.current_state.write().await;
        if let Some(state) = state_guard.as_mut() {
            state.mark_skipped(url);

            if self.inner.should_auto_save() {
                let session_name =
                    format!("{}_{}", state.category, state.created_at.format("%Y%m%d"));
                self.inner.save(&session_name, state)?;
            }
        }
        Ok(())
    }

    /// Force save current state
    pub async fn force_save(&self) -> Result<Option<PathBuf>> {
        let state_guard = self.current_state.read().await;
        if let Some(state) = state_guard.as_ref() {
            let session_name = format!("{}_{}", state.category, state.created_at.format("%Y%m%d"));
            let path = self.inner.save(&session_name, state)?;
            return Ok(Some(path));
        }
        Ok(None)
    }

    /// Get current state snapshot
    pub async fn get_state(&self) -> Option<CrawlState> {
        self.current_state.read().await.clone()
    }

    /// Finalize session (delete checkpoint on success)
    pub async fn finalize(&self, delete_on_success: bool) -> Result<()> {
        let state_guard = self.current_state.read().await;
        if let Some(state) = state_guard.as_ref() {
            let session_name = format!("{}_{}", state.category, state.created_at.format("%Y%m%d"));

            if delete_on_success && state.is_complete() {
                self.inner.delete(&session_name)?;
                tracing::info!(
                    session = session_name,
                    "Crawl session completed, checkpoint deleted"
                );
            } else {
                self.inner.save(&session_name, state)?;
                tracing::info!(
                    session = session_name,
                    remaining = state.remaining(),
                    "Crawl session saved for later resume"
                );
            }
        }
        Ok(())
    }
}

// ============================================================================
// Concurrency Tuning
// ============================================================================

/// Dynamic worker configuration based on system load
#[derive(Debug, Clone)]
pub struct ConcurrencyConfig {
    /// Minimum workers
    pub min_workers: usize,

    /// Maximum workers
    pub max_workers: usize,

    /// Target channel fill ratio (0.0 - 1.0)
    pub target_fill_ratio: f64,

    /// Adjustment interval in milliseconds
    pub adjust_interval_ms: u64,

    /// Enable adaptive scaling
    pub adaptive_scaling: bool,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            min_workers: 2,
            max_workers: 20,
            target_fill_ratio: 0.5,
            adjust_interval_ms: 5000,
            adaptive_scaling: true,
        }
    }
}

/// Monitors channel backpressure and adjusts worker count
pub struct ConcurrencyMonitor {
    config: ConcurrencyConfig,
    current_workers: AtomicU64,
    channel_capacity: usize,
}

impl ConcurrencyMonitor {
    /// Create a new concurrency monitor
    pub fn new(config: ConcurrencyConfig, channel_capacity: usize) -> Self {
        Self {
            current_workers: AtomicU64::new(config.min_workers as u64),
            config,
            channel_capacity,
        }
    }

    /// Calculate recommended worker count based on channel fill level
    pub fn recommend_workers(&self, current_queue_size: usize) -> usize {
        if !self.config.adaptive_scaling {
            return self.current_workers.load(Ordering::Relaxed) as usize;
        }

        let fill_ratio = current_queue_size as f64 / self.channel_capacity as f64;
        let current = self.current_workers.load(Ordering::Relaxed) as usize;

        let recommended = if fill_ratio > self.config.target_fill_ratio + 0.2 {
            // Channel filling up - reduce workers (backpressure)
            current.saturating_sub(1).max(self.config.min_workers)
        } else if fill_ratio < self.config.target_fill_ratio - 0.2 {
            // Channel draining - can add workers
            (current + 1).min(self.config.max_workers)
        } else {
            current
        };

        self.current_workers
            .store(recommended as u64, Ordering::Relaxed);
        recommended
    }

    /// Get current worker count
    pub fn current_workers(&self) -> usize {
        self.current_workers.load(Ordering::Relaxed) as usize
    }

    /// Check if backpressure is detected
    pub fn is_backpressure(&self, current_queue_size: usize) -> bool {
        let fill_ratio = current_queue_size as f64 / self.channel_capacity as f64;
        fill_ratio > 0.8
    }

    /// Get channel fill percentage
    pub fn fill_percentage(&self, current_queue_size: usize) -> f64 {
        (current_queue_size as f64 / self.channel_capacity as f64) * 100.0
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate unique session ID
fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    format!("session_{timestamp}")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_crawl_state_new() {
        let state = CrawlState::new("politics", 100);
        assert_eq!(state.category, "politics");
        assert_eq!(state.total_urls, 100);
        assert_eq!(state.current_index, 0);
        assert!(!state.is_complete());
    }

    #[test]
    fn test_crawl_state_mark_processed() {
        let mut state = CrawlState::new("economy", 10);
        state.mark_processed("https://example.com/1");

        assert_eq!(state.current_index, 1);
        assert!(state.processed_urls.contains("https://example.com/1"));
        assert_eq!(state.remaining(), 9);
    }

    #[test]
    fn test_crawl_state_mark_failed() {
        let mut state = CrawlState::new("society", 10);
        state.mark_failed("https://example.com/fail", "timeout");

        assert_eq!(state.current_index, 1);
        assert_eq!(state.failed_urls.len(), 1);
        assert_eq!(state.failed_urls[0].error, "timeout");
    }

    #[test]
    fn test_crawl_state_completion() {
        let mut state = CrawlState::new("test", 3);
        assert!((state.completion_percentage() - 0.0).abs() < 0.01);

        state.mark_processed("url1");
        assert!((state.completion_percentage() - 33.33).abs() < 1.0);

        state.mark_processed("url2");
        state.mark_processed("url3");
        assert!(state.is_complete());
        assert!((state.completion_percentage() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_checkpoint_manager_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let state = CrawlState::new("politics", 100);
        manager.save("test_session", &state).unwrap();

        assert!(manager.exists("test_session"));

        let loaded: CrawlState = manager.load("test_session").unwrap().unwrap();
        assert_eq!(loaded.category, "politics");
        assert_eq!(loaded.total_urls, 100);
    }

    #[test]
    fn test_checkpoint_manager_delete() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let state = CrawlState::new("test", 50);
        manager.save("to_delete", &state).unwrap();
        assert!(manager.exists("to_delete"));

        manager.delete("to_delete").unwrap();
        assert!(!manager.exists("to_delete"));
    }

    #[test]
    fn test_checkpoint_manager_list() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path()).unwrap();

        let state1 = CrawlState::new("politics", 10);
        let state2 = CrawlState::new("economy", 20);

        manager.save("session1", &state1).unwrap();
        manager.save("session2", &state2).unwrap();

        let list = manager.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"session1".to_string()));
        assert!(list.contains(&"session2".to_string()));
    }

    #[test]
    fn test_checkpoint_manager_auto_save() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::with_interval(temp_dir.path(), 5).unwrap();

        // Should return true every 5 items
        for i in 1..=10 {
            let should_save = manager.should_auto_save();
            if i % 5 == 0 {
                assert!(should_save, "Should auto-save at item {i}");
            } else {
                assert!(!should_save, "Should not auto-save at item {i}");
            }
        }
    }

    #[test]
    fn test_concurrency_config_default() {
        let config = ConcurrencyConfig::default();
        assert_eq!(config.min_workers, 2);
        assert_eq!(config.max_workers, 20);
        assert!(config.adaptive_scaling);
    }

    #[test]
    fn test_concurrency_monitor_recommend_workers() {
        let config = ConcurrencyConfig {
            min_workers: 2,
            max_workers: 10,
            target_fill_ratio: 0.5,
            adaptive_scaling: true,
            ..Default::default()
        };

        let monitor = ConcurrencyMonitor::new(config, 100);

        // Low fill - should increase
        let recommended = monitor.recommend_workers(10);
        assert!(recommended >= 2);

        // High fill - backpressure
        let recommended = monitor.recommend_workers(90);
        assert!(recommended <= 10);
    }

    #[test]
    fn test_concurrency_monitor_backpressure() {
        let config = ConcurrencyConfig::default();
        let monitor = ConcurrencyMonitor::new(config, 100);

        assert!(!monitor.is_backpressure(50));
        assert!(monitor.is_backpressure(85));
        assert!(monitor.is_backpressure(100));
    }

    #[test]
    fn test_fill_percentage() {
        let config = ConcurrencyConfig::default();
        let monitor = ConcurrencyMonitor::new(config, 100);

        assert!((monitor.fill_percentage(50) - 50.0).abs() < 0.01);
        assert!((monitor.fill_percentage(0) - 0.0).abs() < 0.01);
        assert!((monitor.fill_percentage(100) - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_get_retry_urls() {
        let mut state = CrawlState::new("test", 10);

        state.mark_failed("url1", "error1");
        state.mark_failed("url2", "error2");

        // Add a URL with too many attempts
        state.failed_urls.push(FailedUrl {
            url: "url3".to_string(),
            error: "error3".to_string(),
            attempts: 5, // Over limit
            last_attempt: Utc::now(),
        });

        let retry_urls = state.get_retry_urls();
        assert_eq!(retry_urls.len(), 2);
        assert!(retry_urls.contains(&"url1".to_string()));
        assert!(retry_urls.contains(&"url2".to_string()));
        assert!(!retry_urls.contains(&"url3".to_string()));
    }

    #[tokio::test]
    async fn test_async_checkpoint_manager() {
        let temp_dir = TempDir::new().unwrap();
        let manager = AsyncCheckpointManager::new(temp_dir.path()).unwrap();

        let urls = vec!["url1".to_string(), "url2".to_string(), "url3".to_string()];
        let state = manager.init_session("test", &urls).await.unwrap();

        assert_eq!(state.total_urls, 3);

        manager.mark_processed("url1").await.unwrap();
        manager.mark_failed("url2", "error").await.unwrap();
        manager.mark_skipped("url3").await.unwrap();

        let final_state = manager.get_state().await.unwrap();
        assert_eq!(final_state.current_index, 3);
        assert!(final_state.is_complete());
    }
}
