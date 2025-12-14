//! Actor Model based crawling pipeline
//!
//! This module implements a Producer-Consumer pattern using tokio::mpsc channels
//! for high-performance concurrent crawling.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │   URL       │     │   Fetcher   │     │   Parser    │     │   Storage   │
//! │  Producer   │────▶│   Workers   │────▶│   Workers   │────▶│   Workers   │
//! └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
//!       │                   │                   │                   │
//!       │              mpsc channel        mpsc channel        mpsc channel
//!       │                   │                   │                   │
//!       └───────────────────┴───────────────────┴───────────────────┘
//!                                    │
//!                              Progress/Stats
//! ```
//!
//! # Example
//!
//! ```no_run
//! use ntimes::crawler::pipeline::{CrawlerPipeline, PipelineConfig};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = PipelineConfig::default();
//! let pipeline = CrawlerPipeline::new(config).await?;
//!
//! let urls = vec!["https://example.com/article/1".to_string()];
//! let stats = pipeline.run(urls).await?;
//!
//! println!("Processed {} articles", stats.success_count);
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::crawler::fetcher::NaverFetcher;
use crate::models::ParsedArticle;
use crate::parser::ArticleParser;
use crate::storage::MarkdownWriter;

// ============================================================================
// Configuration
// ============================================================================

/// Pipeline configuration
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Number of fetcher workers
    pub fetcher_workers: usize,

    /// Number of parser workers
    pub parser_workers: usize,

    /// Number of storage workers
    pub storage_workers: usize,

    /// Channel buffer size
    pub channel_buffer_size: usize,

    /// Output directory for markdown files
    pub output_dir: PathBuf,

    /// Requests per second limit
    pub requests_per_second: u32,

    /// Request timeout
    pub request_timeout: Duration,

    /// Enable comment crawling
    pub crawl_comments: bool,

    /// Maximum retries per URL
    pub max_retries: u32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            fetcher_workers: 5,
            parser_workers: 3,
            storage_workers: 2,
            channel_buffer_size: 1000,
            output_dir: PathBuf::from("./output/raw"),
            requests_per_second: 5,
            request_timeout: Duration::from_secs(30),
            crawl_comments: true,
            max_retries: 3,
        }
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Message from URL producer to Fetcher
#[derive(Debug, Clone)]
pub struct FetchJob {
    /// URL to fetch
    pub url: String,

    /// Job ID for tracking
    pub job_id: u64,

    /// Retry count
    pub retry_count: u32,
}

/// Message from Fetcher to Parser
#[derive(Debug)]
pub struct ParseJob {
    /// Original URL
    pub url: String,

    /// Job ID
    pub job_id: u64,

    /// Fetched HTML content
    pub html: String,

    /// Category (extracted from URL)
    pub category: String,
}

/// Message from Parser to Storage
#[derive(Debug)]
pub struct StoreJob {
    /// Job ID
    pub job_id: u64,

    /// Parsed article
    pub article: ParsedArticle,
}

/// Result message for tracking
#[derive(Debug, Clone)]
pub enum JobResult {
    /// Successfully processed
    Success {
        job_id: u64,
        article_id: String,
    },
    /// Failed to process
    Failed {
        job_id: u64,
        url: String,
        error: String,
    },
    /// Skipped (duplicate or filtered)
    Skipped {
        job_id: u64,
        reason: String,
    },
}

// ============================================================================
// Pipeline Statistics
// ============================================================================

/// Pipeline statistics (thread-safe)
#[derive(Debug, Default)]
pub struct PipelineStats {
    /// Total jobs submitted
    pub total_jobs: AtomicU64,

    /// Successfully completed jobs
    pub success_count: AtomicU64,

    /// Failed jobs
    pub failed_count: AtomicU64,

    /// Skipped jobs
    pub skipped_count: AtomicU64,

    /// Total bytes fetched
    pub bytes_fetched: AtomicU64,

    /// Articles with comments
    pub with_comments: AtomicU64,
}

impl PipelineStats {
    /// Create new stats counter
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record successful job
    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record failed job
    pub fn record_failure(&self) {
        self.failed_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record skipped job
    pub fn record_skip(&self) {
        self.skipped_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record bytes fetched
    pub fn record_bytes(&self, bytes: u64) {
        self.bytes_fetched.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Get snapshot of current stats
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            total_jobs: self.total_jobs.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
            failed_count: self.failed_count.load(Ordering::Relaxed),
            skipped_count: self.skipped_count.load(Ordering::Relaxed),
            bytes_fetched: self.bytes_fetched.load(Ordering::Relaxed),
            with_comments: self.with_comments.load(Ordering::Relaxed),
        }
    }

    /// Calculate completion percentage
    pub fn completion_percentage(&self) -> f64 {
        let total = self.total_jobs.load(Ordering::Relaxed);
        if total == 0 {
            return 100.0;
        }
        let completed = self.success_count.load(Ordering::Relaxed)
            + self.failed_count.load(Ordering::Relaxed)
            + self.skipped_count.load(Ordering::Relaxed);
        (completed as f64 / total as f64) * 100.0
    }
}

/// Snapshot of pipeline statistics
#[derive(Debug, Clone, Default)]
pub struct StatsSnapshot {
    pub total_jobs: u64,
    pub success_count: u64,
    pub failed_count: u64,
    pub skipped_count: u64,
    pub bytes_fetched: u64,
    pub with_comments: u64,
}

impl StatsSnapshot {
    /// Success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.success_count + self.failed_count + self.skipped_count;
        if total == 0 {
            return 1.0;
        }
        self.success_count as f64 / total as f64
    }
}

// ============================================================================
// Pipeline Implementation
// ============================================================================

/// Main crawler pipeline using Actor Model
pub struct CrawlerPipeline {
    config: PipelineConfig,
    stats: Arc<PipelineStats>,
}

impl CrawlerPipeline {
    /// Create a new pipeline
    pub async fn new(config: PipelineConfig) -> Result<Self> {
        // Create output directory
        tokio::fs::create_dir_all(&config.output_dir)
            .await
            .context("Failed to create output directory")?;

        Ok(Self {
            config,
            stats: PipelineStats::new(),
        })
    }

    /// Run the pipeline with given URLs
    pub async fn run(&self, urls: Vec<String>) -> Result<StatsSnapshot> {
        let total_urls = urls.len() as u64;
        self.stats.total_jobs.store(total_urls, Ordering::Relaxed);

        tracing::info!(
            total = total_urls,
            fetcher_workers = self.config.fetcher_workers,
            parser_workers = self.config.parser_workers,
            storage_workers = self.config.storage_workers,
            "Starting crawler pipeline"
        );

        // Create channels
        let (fetch_tx, fetch_rx) = mpsc::channel::<FetchJob>(self.config.channel_buffer_size);
        let (parse_tx, parse_rx) = mpsc::channel::<ParseJob>(self.config.channel_buffer_size);
        let (store_tx, store_rx) = mpsc::channel::<StoreJob>(self.config.channel_buffer_size);
        let (result_tx, mut result_rx) = mpsc::channel::<JobResult>(self.config.channel_buffer_size);

        // Spawn workers
        let fetcher_handles = self.spawn_fetcher_workers(
            fetch_rx,
            parse_tx,
            result_tx.clone(),
        );

        let parser_handles = self.spawn_parser_workers(
            parse_rx,
            store_tx,
            result_tx.clone(),
        );

        let storage_handles = self.spawn_storage_workers(
            store_rx,
            result_tx.clone(),
        );

        // Spawn result collector
        let stats = Arc::clone(&self.stats);
        let result_handle = tokio::spawn(async move {
            while let Some(result) = result_rx.recv().await {
                match result {
                    JobResult::Success { job_id, article_id } => {
                        stats.record_success();
                        tracing::debug!(job_id, article_id, "Job completed successfully");
                    }
                    JobResult::Failed { job_id, url, error } => {
                        stats.record_failure();
                        tracing::warn!(job_id, url, error, "Job failed");
                    }
                    JobResult::Skipped { job_id, reason } => {
                        stats.record_skip();
                        tracing::debug!(job_id, reason, "Job skipped");
                    }
                }
            }
        });

        // Send URLs to fetch channel
        for (idx, url) in urls.into_iter().enumerate() {
            let job = FetchJob {
                url,
                job_id: idx as u64,
                retry_count: 0,
            };

            if fetch_tx.send(job).await.is_err() {
                tracing::error!("Failed to send fetch job - channel closed");
                break;
            }
        }

        // Close fetch channel to signal completion
        drop(fetch_tx);

        // Wait for all workers to complete
        for handle in fetcher_handles {
            let _ = handle.await;
        }

        for handle in parser_handles {
            let _ = handle.await;
        }

        for handle in storage_handles {
            let _ = handle.await;
        }

        // Close result channel and wait for collector
        drop(result_tx);
        let _ = result_handle.await;

        let snapshot = self.stats.snapshot();
        tracing::info!(
            success = snapshot.success_count,
            failed = snapshot.failed_count,
            skipped = snapshot.skipped_count,
            bytes = snapshot.bytes_fetched,
            "Pipeline completed"
        );

        Ok(snapshot)
    }

    /// Spawn fetcher worker tasks
    fn spawn_fetcher_workers(
        &self,
        fetch_rx: mpsc::Receiver<FetchJob>,
        parse_tx: mpsc::Sender<ParseJob>,
        result_tx: mpsc::Sender<JobResult>,
    ) -> Vec<JoinHandle<()>> {
        let fetch_rx = Arc::new(tokio::sync::Mutex::new(fetch_rx));
        let mut handles = Vec::with_capacity(self.config.fetcher_workers);

        for worker_id in 0..self.config.fetcher_workers {
            let fetch_rx = Arc::clone(&fetch_rx);
            let parse_tx = parse_tx.clone();
            let result_tx = result_tx.clone();
            let stats = Arc::clone(&self.stats);
            let rps = self.config.requests_per_second;
            let timeout = self.config.request_timeout;
            let max_retries = self.config.max_retries;

            let handle = tokio::spawn(async move {
                let fetcher = match NaverFetcher::with_config(rps, max_retries, timeout) {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::error!(worker_id, error = %e, "Failed to create fetcher");
                        return;
                    }
                };

                loop {
                    let job = {
                        let mut rx = fetch_rx.lock().await;
                        rx.recv().await
                    };

                    let job = match job {
                        Some(j) => j,
                        None => break, // Channel closed
                    };

                    tracing::debug!(worker_id, job_id = job.job_id, url = %job.url, "Fetching");

                    // Fetch URL and get response
                    let fetch_result = fetcher.fetch(&job.url).await;

                    match fetch_result {
                        Ok(response) => {
                            // Extract text from response
                            match response.text().await {
                                Ok(html) => {
                                    stats.record_bytes(html.len() as u64);

                                    // Extract category from URL
                                    let category = extract_category_from_url(&job.url);

                                    let parse_job = ParseJob {
                                        url: job.url,
                                        job_id: job.job_id,
                                        html,
                                        category,
                                    };

                                    if parse_tx.send(parse_job).await.is_err() {
                                        tracing::error!("Parse channel closed");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    let _ = result_tx.send(JobResult::Failed {
                                        job_id: job.job_id,
                                        url: job.url,
                                        error: format!("Failed to read response: {}", e),
                                    }).await;
                                }
                            }
                        }
                        Err(e) => {
                            // Retry logic
                            if job.retry_count < max_retries {
                                tracing::warn!(
                                    job_id = job.job_id,
                                    retry = job.retry_count + 1,
                                    "Retrying fetch"
                                );
                                // In a real implementation, we'd re-queue the job
                                // For now, just record the failure
                            }

                            let _ = result_tx.send(JobResult::Failed {
                                job_id: job.job_id,
                                url: job.url,
                                error: e.to_string(),
                            }).await;
                        }
                    }
                }

                tracing::debug!(worker_id, "Fetcher worker shutting down");
            });

            handles.push(handle);
        }

        handles
    }

    /// Spawn parser worker tasks
    fn spawn_parser_workers(
        &self,
        parse_rx: mpsc::Receiver<ParseJob>,
        store_tx: mpsc::Sender<StoreJob>,
        result_tx: mpsc::Sender<JobResult>,
    ) -> Vec<JoinHandle<()>> {
        let parse_rx = Arc::new(tokio::sync::Mutex::new(parse_rx));
        let mut handles = Vec::with_capacity(self.config.parser_workers);

        for worker_id in 0..self.config.parser_workers {
            let parse_rx = Arc::clone(&parse_rx);
            let store_tx = store_tx.clone();
            let result_tx = result_tx.clone();

            let handle = tokio::spawn(async move {
                let parser = ArticleParser::new();

                loop {
                    let job = {
                        let mut rx = parse_rx.lock().await;
                        rx.recv().await
                    };

                    let job = match job {
                        Some(j) => j,
                        None => break,
                    };

                    tracing::debug!(worker_id, job_id = job.job_id, "Parsing");

                    match parser.parse_with_fallback(&job.html, &job.url) {
                        Ok(mut article) => {
                            // Set category from URL extraction
                            article.category = job.category;

                            let store_job = StoreJob {
                                job_id: job.job_id,
                                article,
                            };

                            if store_tx.send(store_job).await.is_err() {
                                tracing::error!("Store channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = result_tx.send(JobResult::Failed {
                                job_id: job.job_id,
                                url: job.url,
                                error: format!("Parse error: {}", e),
                            }).await;
                        }
                    }
                }

                tracing::debug!(worker_id, "Parser worker shutting down");
            });

            handles.push(handle);
        }

        handles
    }

    /// Spawn storage worker tasks
    fn spawn_storage_workers(
        &self,
        store_rx: mpsc::Receiver<StoreJob>,
        result_tx: mpsc::Sender<JobResult>,
    ) -> Vec<JoinHandle<()>> {
        let store_rx = Arc::new(tokio::sync::Mutex::new(store_rx));
        let mut handles = Vec::with_capacity(self.config.storage_workers);
        let output_dir = self.config.output_dir.clone();

        for worker_id in 0..self.config.storage_workers {
            let store_rx = Arc::clone(&store_rx);
            let result_tx = result_tx.clone();
            let output_dir = output_dir.clone();

            let handle = tokio::spawn(async move {
                let writer = match MarkdownWriter::new(&output_dir) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::error!(worker_id, error = %e, "Failed to create writer");
                        return;
                    }
                };

                loop {
                    let job = {
                        let mut rx = store_rx.lock().await;
                        rx.recv().await
                    };

                    let job = match job {
                        Some(j) => j,
                        None => break,
                    };

                    tracing::debug!(worker_id, job_id = job.job_id, "Storing");

                    // Check if already exists
                    if writer.exists(&job.article) {
                        let _ = result_tx.send(JobResult::Skipped {
                            job_id: job.job_id,
                            reason: "File already exists".to_string(),
                        }).await;
                        continue;
                    }

                    match writer.save(&job.article) {
                        Ok(path) => {
                            let _ = result_tx.send(JobResult::Success {
                                job_id: job.job_id,
                                article_id: path.display().to_string(),
                            }).await;
                        }
                        Err(e) => {
                            let _ = result_tx.send(JobResult::Failed {
                                job_id: job.job_id,
                                url: job.article.url.clone(),
                                error: format!("Storage error: {}", e),
                            }).await;
                        }
                    }
                }

                tracing::debug!(worker_id, "Storage worker shutting down");
            });

            handles.push(handle);
        }

        handles
    }

    /// Get current statistics
    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract category from Naver News URL
fn extract_category_from_url(url: &str) -> String {
    // URLs like: https://n.news.naver.com/mnews/article/001/0014000001?sid=100
    // sid mapping: 100=politics, 101=economy, 102=society, 103=culture, 104=world, 105=science
    if let Some(sid_pos) = url.find("sid=") {
        let sid_start = sid_pos + 4;
        let sid_end = url[sid_start..]
            .find('&')
            .map(|i| sid_start + i)
            .unwrap_or(url.len());

        match &url[sid_start..sid_end] {
            "100" => return "politics".to_string(),
            "101" => return "economy".to_string(),
            "102" => return "society".to_string(),
            "103" => return "culture".to_string(),
            "104" => return "world".to_string(),
            "105" => return "science".to_string(),
            _ => {}
        }
    }

    // Try to extract from URL path
    if url.contains("/politics/") {
        "politics".to_string()
    } else if url.contains("/economy/") {
        "economy".to_string()
    } else if url.contains("/society/") {
        "society".to_string()
    } else if url.contains("/culture/") || url.contains("/life/") {
        "culture".to_string()
    } else if url.contains("/world/") {
        "world".to_string()
    } else if url.contains("/it/") || url.contains("/science/") {
        "science".to_string()
    } else {
        "general".to_string()
    }
}

// ============================================================================
// Pipeline Builder
// ============================================================================

/// Builder for CrawlerPipeline
pub struct PipelineBuilder {
    config: PipelineConfig,
}

impl PipelineBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: PipelineConfig::default(),
        }
    }

    /// Set number of fetcher workers
    pub fn fetcher_workers(mut self, count: usize) -> Self {
        self.config.fetcher_workers = count;
        self
    }

    /// Set number of parser workers
    pub fn parser_workers(mut self, count: usize) -> Self {
        self.config.parser_workers = count;
        self
    }

    /// Set number of storage workers
    pub fn storage_workers(mut self, count: usize) -> Self {
        self.config.storage_workers = count;
        self
    }

    /// Set channel buffer size
    pub fn channel_buffer_size(mut self, size: usize) -> Self {
        self.config.channel_buffer_size = size;
        self
    }

    /// Set output directory
    pub fn output_dir(mut self, path: PathBuf) -> Self {
        self.config.output_dir = path;
        self
    }

    /// Set requests per second
    pub fn requests_per_second(mut self, rps: u32) -> Self {
        self.config.requests_per_second = rps;
        self
    }

    /// Set request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.request_timeout = timeout;
        self
    }

    /// Enable/disable comment crawling
    pub fn crawl_comments(mut self, enabled: bool) -> Self {
        self.config.crawl_comments = enabled;
        self
    }

    /// Build the pipeline
    pub async fn build(self) -> Result<CrawlerPipeline> {
        CrawlerPipeline::new(self.config).await
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.fetcher_workers, 5);
        assert_eq!(config.parser_workers, 3);
        assert_eq!(config.storage_workers, 2);
        assert_eq!(config.channel_buffer_size, 1000);
    }

    #[test]
    fn test_extract_category_from_url_sid() {
        assert_eq!(
            extract_category_from_url("https://n.news.naver.com/article/001/001?sid=100"),
            "politics"
        );
        assert_eq!(
            extract_category_from_url("https://n.news.naver.com/article/001/001?sid=101"),
            "economy"
        );
        assert_eq!(
            extract_category_from_url("https://n.news.naver.com/article/001/001?sid=102"),
            "society"
        );
        assert_eq!(
            extract_category_from_url("https://n.news.naver.com/article/001/001?sid=103"),
            "culture"
        );
        assert_eq!(
            extract_category_from_url("https://n.news.naver.com/article/001/001?sid=104"),
            "world"
        );
        assert_eq!(
            extract_category_from_url("https://n.news.naver.com/article/001/001?sid=105"),
            "science"
        );
    }

    #[test]
    fn test_extract_category_from_url_path() {
        assert_eq!(
            extract_category_from_url("https://news.naver.com/politics/article/001/001"),
            "politics"
        );
        assert_eq!(
            extract_category_from_url("https://news.naver.com/economy/article/001/001"),
            "economy"
        );
        assert_eq!(
            extract_category_from_url("https://news.naver.com/unknown/article/001/001"),
            "general"
        );
    }

    #[test]
    fn test_stats_snapshot() {
        let stats = PipelineStats::new();
        stats.record_success();
        stats.record_success();
        stats.record_failure();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.success_count, 2);
        assert_eq!(snapshot.failed_count, 1);
        assert!((snapshot.success_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_pipeline_builder() {
        let config = PipelineBuilder::new()
            .fetcher_workers(10)
            .parser_workers(5)
            .storage_workers(3)
            .channel_buffer_size(500)
            .requests_per_second(10)
            .config;

        assert_eq!(config.fetcher_workers, 10);
        assert_eq!(config.parser_workers, 5);
        assert_eq!(config.storage_workers, 3);
        assert_eq!(config.channel_buffer_size, 500);
        assert_eq!(config.requests_per_second, 10);
    }

    #[test]
    fn test_fetch_job_creation() {
        let job = FetchJob {
            url: "https://example.com".to_string(),
            job_id: 1,
            retry_count: 0,
        };
        assert_eq!(job.job_id, 1);
        assert_eq!(job.retry_count, 0);
    }

    #[test]
    fn test_job_result_variants() {
        let success = JobResult::Success {
            job_id: 1,
            article_id: "test".to_string(),
        };
        assert!(matches!(success, JobResult::Success { .. }));

        let failed = JobResult::Failed {
            job_id: 2,
            url: "https://example.com".to_string(),
            error: "timeout".to_string(),
        };
        assert!(matches!(failed, JobResult::Failed { .. }));

        let skipped = JobResult::Skipped {
            job_id: 3,
            reason: "duplicate".to_string(),
        };
        assert!(matches!(skipped, JobResult::Skipped { .. }));
    }

    #[test]
    fn test_completion_percentage() {
        let stats = PipelineStats::new();
        stats.total_jobs.store(100, Ordering::Relaxed);
        stats.success_count.store(50, Ordering::Relaxed);
        stats.failed_count.store(10, Ordering::Relaxed);

        let percentage = stats.completion_percentage();
        assert!((percentage - 60.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_pipeline_creation() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let config = PipelineConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let pipeline = CrawlerPipeline::new(config).await;
        assert!(pipeline.is_ok());
    }
}
