//! Distributed crawler runner
//!
//! This module provides the main runner for distributed crawling that integrates
//! with the coordinator server for schedule management and health reporting.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

use chrono::Timelike;

use crate::coordinator::client::{ClientConfig, ClientError, CoordinatorClient, SlotResponse};
use crate::crawler::fetcher::NaverFetcher;
use crate::crawler::list::NewsListCrawler;
use crate::crawler::pipeline::{CrawlerPipeline, PipelineConfig};
use crate::metrics;
use crate::models::NewsCategory;
use crate::scheduler::rotation::CrawlerInstance;
use crate::storage::dedup::{DedupConfig, DedupRecord, SharedDedupChecker};

use super::instance::{InstanceConfig, InstanceState};

// ============================================================================
// Distributed Crawler Runner
// ============================================================================

/// Main runner for distributed crawling
///
/// Handles:
/// - Registration with coordinator
/// - Periodic heartbeat sending
/// - Schedule polling and slot execution
/// - Deduplication via PostgreSQL
/// - Graceful shutdown
pub struct DistributedRunner {
    /// Instance configuration
    config: InstanceConfig,

    /// Coordinator client
    coordinator: CoordinatorClient,

    /// Instance state
    state: Arc<RwLock<InstanceState>>,

    /// Deduplication checker (optional)
    dedup_checker: Option<SharedDedupChecker>,

    /// Shutdown signal
    shutdown: tokio::sync::watch::Sender<bool>,

    /// Shutdown receiver
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl DistributedRunner {
    /// Create a new distributed runner
    pub fn new(config: InstanceConfig) -> Result<Self, RunnerError> {
        let client_config = ClientConfig::new(&config.coordinator_url, config.instance_id)
            .with_timeout(config.timeout())
            .with_retry_count(config.max_retries);

        let coordinator = CoordinatorClient::new(client_config)
            .map_err(|e| RunnerError::InitError(e.to_string()))?;

        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);

        Ok(Self {
            config,
            coordinator,
            state: Arc::new(RwLock::new(InstanceState::new())),
            dedup_checker: None,
            shutdown,
            shutdown_rx,
        })
    }

    /// Create a new distributed runner with deduplication
    pub async fn with_dedup(config: InstanceConfig) -> Result<Self, RunnerError> {
        let mut runner = Self::new(config)?;
        runner.init_dedup().await?;
        Ok(runner)
    }

    /// Initialize deduplication checker
    pub async fn init_dedup(&mut self) -> Result<(), RunnerError> {
        let dedup_config = DedupConfig::default()
            .with_database_url(&self.config.database_url)
            .with_pool_size(5);

        let checker = crate::storage::dedup::create_shared_checker(dedup_config)
            .await
            .map_err(|e| RunnerError::InitError(format!("Failed to init dedup: {e}")))?;

        self.dedup_checker = Some(checker);

        tracing::info!(
            "Deduplication checker initialized for instance {}",
            self.config.instance_id
        );

        Ok(())
    }

    /// Set deduplication checker
    pub fn set_dedup_checker(&mut self, checker: SharedDedupChecker) {
        self.dedup_checker = Some(checker);
    }

    /// Check if deduplication is enabled
    pub fn has_dedup(&self) -> bool {
        self.dedup_checker.is_some()
    }

    /// Filter URLs that haven't been crawled
    ///
    /// Returns only new URLs that need to be crawled
    pub async fn filter_new_urls(&self, urls: &[String]) -> Result<Vec<String>, RunnerError> {
        match &self.dedup_checker {
            Some(checker) => {
                let result = checker
                    .batch_check_urls(urls)
                    .await
                    .map_err(|e| RunnerError::CrawlError(format!("Dedup check failed: {e}")))?;

                tracing::debug!(
                    "Dedup check: {} new, {} existing, {} total",
                    result.new_count(),
                    result.existing_count(),
                    result.total_checked
                );

                Ok(result.new_urls)
            }
            None => {
                // No dedup checker, return all URLs
                Ok(urls.to_vec())
            }
        }
    }

    /// Check if a URL has been crawled
    pub async fn is_url_crawled(&self, url: &str) -> Result<bool, RunnerError> {
        match &self.dedup_checker {
            Some(checker) => checker
                .exists_by_url(url)
                .await
                .map_err(|e| RunnerError::CrawlError(format!("Dedup check failed: {e}"))),
            None => Ok(false),
        }
    }

    /// Record a successful crawl
    pub async fn record_crawl_success(
        &self,
        article_id: &str,
        url: &str,
        content_hash: &str,
    ) -> Result<(), RunnerError> {
        if let Some(checker) = &self.dedup_checker {
            let record =
                DedupRecord::new(article_id, url, content_hash, self.config.instance_id.id());

            checker
                .record_crawl(&record)
                .await
                .map_err(|e| RunnerError::CrawlError(format!("Failed to record crawl: {e}")))?;
        }

        Ok(())
    }

    /// Record a failed crawl
    pub async fn record_crawl_failure(
        &self,
        article_id: &str,
        url: &str,
    ) -> Result<(), RunnerError> {
        if let Some(checker) = &self.dedup_checker {
            let record =
                DedupRecord::new(article_id, url, "", self.config.instance_id.id()).with_failure();

            checker
                .record_crawl(&record)
                .await
                .map_err(|e| RunnerError::CrawlError(format!("Failed to record failure: {e}")))?;
        }

        Ok(())
    }

    /// Batch record successful crawls
    pub async fn batch_record_crawls(
        &self,
        records: &[(String, String, String)], // (article_id, url, content_hash)
    ) -> Result<usize, RunnerError> {
        match &self.dedup_checker {
            Some(checker) => {
                let dedup_records: Vec<DedupRecord> = records
                    .iter()
                    .map(|(id, url, hash)| {
                        DedupRecord::new(id, url, hash, self.config.instance_id.id())
                    })
                    .collect();

                let count = checker
                    .batch_record_crawls(&dedup_records)
                    .await
                    .map_err(|e| RunnerError::CrawlError(format!("Failed to batch record: {e}")))?;

                Ok(count)
            }
            None => Ok(0),
        }
    }

    /// Get deduplication statistics for this instance
    pub async fn get_dedup_stats(
        &self,
    ) -> Result<Option<crate::storage::dedup::DedupStats>, RunnerError> {
        match &self.dedup_checker {
            Some(checker) => {
                let stats = checker
                    .get_stats_by_instance(self.config.instance_id.id())
                    .await
                    .map_err(|e| RunnerError::CrawlError(format!("Failed to get stats: {e}")))?;

                Ok(Some(stats))
            }
            None => Ok(None),
        }
    }

    /// Get instance ID
    pub fn instance_id(&self) -> CrawlerInstance {
        self.config.instance_id
    }

    /// Get current state
    pub async fn state(&self) -> InstanceState {
        self.state.read().await.clone()
    }

    /// Register with coordinator
    pub async fn register(&self) -> Result<(), RunnerError> {
        let ip = self
            .config
            .local_ip
            .clone()
            .unwrap_or_else(|| "0.0.0.0".to_string());

        tracing::info!(
            "Registering instance {} with coordinator at {}",
            self.config.instance_id,
            self.config.coordinator_url
        );

        match self.coordinator.register(&ip, self.config.local_port).await {
            Ok(response) => {
                tracing::info!(
                    "Registration successful: instance={}, message={}",
                    response.instance,
                    response.message
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Registration failed: {}", e);
                // Non-fatal - we can still run with cached schedules
                Ok(())
            }
        }
    }

    /// Start the distributed runner
    ///
    /// This spawns background tasks for:
    /// - Heartbeat sending
    /// - Schedule polling
    /// - Hourly slot execution
    pub async fn start(&self) -> Result<RunnerHandle, RunnerError> {
        // Register first
        self.register().await?;

        // Spawn heartbeat task
        let heartbeat_handle = self.spawn_heartbeat_task();

        // Spawn schedule watcher task
        let schedule_handle = self.spawn_schedule_watcher();

        Ok(RunnerHandle {
            heartbeat_handle,
            schedule_handle,
            shutdown: self.shutdown.clone(),
        })
    }

    /// Run a single crawl cycle for the given slot
    pub async fn run_slot(&self, slot: &SlotResponse) -> Result<SlotResult, RunnerError> {
        tracing::info!(
            "Starting crawl for hour {} with categories: {:?}",
            slot.hour,
            slot.categories
        );

        // Update state
        {
            let mut state = self.state.write().await;
            state.set_crawling(true);
            state.set_category(slot.categories.first().cloned());
        }

        // Update crawler state metrics
        let instance_id = self.config.instance_id.id();
        metrics::update_crawler_state(instance_id, true, Some(slot.hour));

        let mut articles_crawled = 0u64;
        let mut errors = 0u64;

        // Crawl each category
        for category in &slot.categories {
            tracing::info!("Crawling category: {}", category);

            // Update current category
            {
                let mut state = self.state.write().await;
                state.set_category(Some(category.clone()));
            }

            // TODO: Integrate with actual crawler here
            // For now, simulate crawling
            match self.crawl_category(category).await {
                Ok(count) => {
                    articles_crawled += count;
                    tracing::info!("Crawled {} articles from {}", count, category);
                }
                Err(e) => {
                    errors += 1;
                    tracing::error!("Error crawling {}: {}", category, e);
                }
            }
        }

        // Update state
        {
            let mut state = self.state.write().await;
            state.record_success(articles_crawled);
            if errors > 0 {
                for _ in 0..errors {
                    state.record_error();
                }
            }
            state.set_crawling(false);
            state.set_category(None);
        }

        // Record slot execution metrics
        let instance_id = self.config.instance_id.id();
        metrics::record_slot_execution(instance_id, slot.hour, errors > 0);
        metrics::update_crawler_state(instance_id, false, None);

        Ok(SlotResult {
            hour: slot.hour,
            articles_crawled,
            errors,
            categories: slot.categories.clone(),
        })
    }

    /// Crawl a single category
    async fn crawl_category(&self, category: &str) -> Result<u64, RunnerError> {
        let instance_id = self.config.instance_id.id();

        // Start metrics timer
        let _timer = metrics::start_crawl_timer(instance_id, category);

        // Step 1: Parse category string to NewsCategory enum
        let news_category = NewsCategory::parse(category)
            .ok_or_else(|| RunnerError::CrawlError(format!("Invalid category: {category}")))?;

        tracing::info!(
            category = %category,
            section_id = news_category.to_section_id(),
            "Starting category crawl"
        );

        // Step 2: Get today's date in YYYYMMDD format
        let today = chrono::Local::now().format("%Y%m%d").to_string();

        // Step 3: Create fetcher and list crawler
        let rps = self.config.requests_per_second.ceil() as u32;
        let fetcher =
            NaverFetcher::with_config(rps, self.config.max_retries, self.config.timeout())
                .map_err(|e| RunnerError::InitError(format!("Failed to create fetcher: {e}")))?;

        let list_crawler = NewsListCrawler::new(fetcher);

        // Step 4: Collect URLs from the category (with pagination)
        let max_pages = 10; // Default to 10 pages per category
        let all_urls = list_crawler
            .collect_urls(news_category, &today, max_pages)
            .await
            .map_err(|e| RunnerError::CrawlError(format!("Failed to collect URLs: {e}")))?;

        tracing::info!(
            category = %category,
            total_urls = all_urls.len(),
            "Collected article URLs"
        );

        if all_urls.is_empty() {
            return Ok(0);
        }

        // Step 5: Filter new URLs using deduplication checker
        let new_urls = self.filter_new_urls(&all_urls).await?;

        let existing_urls = all_urls.len() - new_urls.len();

        // Record deduplication metrics
        metrics::record_dedup_results(instance_id, new_urls.len(), existing_urls);

        tracing::info!(
            category = %category,
            new_urls = new_urls.len(),
            skipped = existing_urls,
            "Filtered URLs (dedup)"
        );

        if new_urls.is_empty() {
            tracing::info!(category = %category, "No new articles to crawl");
            return Ok(0);
        }

        // Step 6: Create pipeline config
        let pipeline_config = PipelineConfig {
            fetcher_workers: 3,
            parser_workers: 2,
            storage_workers: 2,
            channel_buffer_size: 100,
            output_dir: PathBuf::from(&self.config.output_dir).join("raw"),
            requests_per_second: rps,
            request_timeout: self.config.timeout(),
            crawl_comments: self.config.include_comments,
            max_retries: self.config.max_retries,
        };

        // Step 7: Run the pipeline
        let pipeline = CrawlerPipeline::new(pipeline_config)
            .await
            .map_err(|e| RunnerError::InitError(format!("Failed to create pipeline: {e}")))?;

        let stats = pipeline
            .run(new_urls.clone())
            .await
            .map_err(|e| RunnerError::CrawlError(format!("Pipeline error: {e}")))?;

        tracing::info!(
            category = %category,
            success = stats.success_count,
            failed = stats.failed_count,
            skipped = stats.skipped_count,
            "Category crawl completed"
        );

        // Step 8: Record metrics for pipeline results
        metrics::record_pipeline_results(
            instance_id,
            category,
            stats.success_count,
            stats.failed_count,
            stats.skipped_count,
        );

        // Record articles crawled for this category
        metrics::record_articles_crawled(instance_id, category, stats.success_count);

        // Step 9: Record successful crawls in deduplication database
        // Note: In a full implementation, we would get article IDs and content hashes
        // from the pipeline results. For now, we record the URLs as crawled.
        let crawled_count = stats.success_count;

        Ok(crawled_count)
    }

    /// Check if current hour is assigned to this instance
    pub async fn check_current_slot(&self) -> Result<Option<SlotResponse>, RunnerError> {
        let now = chrono::Local::now();
        let hour = now.hour() as u8;

        match self.coordinator.should_crawl_at(hour).await {
            Ok(Some(categories)) => Ok(Some(SlotResponse {
                hour,
                instance: self.config.instance_id.id().to_string(),
                categories,
            })),
            Ok(None) => Ok(None),
            Err(e) => {
                tracing::warn!("Failed to check slot: {}", e);
                Err(RunnerError::CoordinatorError(e.to_string()))
            }
        }
    }

    /// Get all slots assigned to this instance today
    pub async fn get_my_slots(&self) -> Result<Vec<SlotResponse>, RunnerError> {
        self.coordinator
            .get_my_slots()
            .await
            .map_err(|e| RunnerError::CoordinatorError(e.to_string()))
    }

    /// Send heartbeat
    #[allow(dead_code)]
    async fn send_heartbeat(&self) -> Result<(), ClientError> {
        let state = self.state.read().await;

        self.coordinator
            .heartbeat(
                state.articles_crawled,
                state.error_count,
                state.current_category.clone(),
            )
            .await?;

        Ok(())
    }

    /// Spawn heartbeat background task
    fn spawn_heartbeat_task(&self) -> tokio::task::JoinHandle<()> {
        let coordinator = self.coordinator_clone();
        let state = self.state.clone();
        let interval_duration = self.config.heartbeat_interval();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut ticker = interval(interval_duration);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let s = state.read().await;
                        if let Err(e) = coordinator.heartbeat(
                            s.articles_crawled,
                            s.error_count,
                            s.current_category.clone(),
                        ).await {
                            tracing::warn!("Heartbeat failed: {}", e);
                        } else {
                            tracing::debug!("Heartbeat sent successfully");
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("Heartbeat task shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Spawn schedule watcher background task
    fn spawn_schedule_watcher(&self) -> tokio::task::JoinHandle<()> {
        let instance_id = self.config.instance_id;
        let config = self.config.clone();
        let coordinator = self.coordinator_clone();
        let state = self.state.clone();
        let dedup_checker = self.dedup_checker.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Wait until next hour boundary
            let now = chrono::Local::now();
            let next_hour =
                now.with_minute(0).unwrap().with_second(0).unwrap() + chrono::Duration::hours(1);
            let wait_duration = (next_hour - now)
                .to_std()
                .unwrap_or(Duration::from_secs(60));

            tracing::info!(
                "Schedule watcher starting, waiting {:?} until next hour",
                wait_duration
            );

            tokio::time::sleep(wait_duration).await;

            // Then check every hour
            let mut ticker = interval(Duration::from_secs(3600)); // 1 hour
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let hour = chrono::Local::now().hour() as u8;
                        tracing::info!("Checking schedule for hour {}", hour);

                        match coordinator.should_crawl_at(hour).await {
                            Ok(Some(categories)) => {
                                tracing::info!(
                                    "Instance {} should crawl at {}: {:?}",
                                    instance_id,
                                    hour,
                                    categories
                                );

                                // Create a slot response
                                let slot = SlotResponse {
                                    hour,
                                    instance: instance_id.id().to_string(),
                                    categories: categories.clone(),
                                };

                                // Execute the crawl for this slot
                                let result = Self::execute_slot_crawl(
                                    &config,
                                    &state,
                                    &dedup_checker,
                                    &slot,
                                ).await;

                                match result {
                                    Ok(slot_result) => {
                                        tracing::info!(
                                            hour = slot_result.hour,
                                            articles = slot_result.articles_crawled,
                                            errors = slot_result.errors,
                                            "Slot crawl completed"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::error!("Slot crawl failed: {}", e);
                                    }
                                }
                            }
                            Ok(None) => {
                                tracing::debug!(
                                    "Instance {} is not scheduled for hour {}",
                                    instance_id,
                                    hour
                                );
                            }
                            Err(e) => {
                                tracing::warn!("Failed to check schedule: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("Schedule watcher shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Execute a slot crawl (static method for use in spawned tasks)
    async fn execute_slot_crawl(
        config: &InstanceConfig,
        state: &Arc<RwLock<InstanceState>>,
        dedup_checker: &Option<SharedDedupChecker>,
        slot: &SlotResponse,
    ) -> Result<SlotResult, RunnerError> {
        let instance_id = config.instance_id.id();

        tracing::info!(
            "Starting crawl for hour {} with categories: {:?}",
            slot.hour,
            slot.categories
        );

        // Update state
        {
            let mut s = state.write().await;
            s.set_crawling(true);
            s.set_category(slot.categories.first().cloned());
        }

        // Update crawler state metrics
        metrics::update_crawler_state(instance_id, true, Some(slot.hour));

        let mut articles_crawled = 0u64;
        let mut errors = 0u64;

        // Crawl each category
        for category in &slot.categories {
            tracing::info!("Crawling category: {}", category);

            // Update current category
            {
                let mut s = state.write().await;
                s.set_category(Some(category.clone()));
            }

            // Execute category crawl
            match Self::crawl_category_static(config, dedup_checker, category).await {
                Ok(count) => {
                    articles_crawled += count;
                    tracing::info!("Crawled {} articles from {}", count, category);
                }
                Err(e) => {
                    errors += 1;
                    tracing::error!("Error crawling {}: {}", category, e);
                }
            }
        }

        // Update state
        {
            let mut s = state.write().await;
            s.record_success(articles_crawled);
            if errors > 0 {
                for _ in 0..errors {
                    s.record_error();
                }
            }
            s.set_crawling(false);
            s.set_category(None);
        }

        // Record slot execution metrics
        metrics::record_slot_execution(instance_id, slot.hour, errors > 0);
        metrics::update_crawler_state(instance_id, false, None);

        Ok(SlotResult {
            hour: slot.hour,
            articles_crawled,
            errors,
            categories: slot.categories.clone(),
        })
    }

    /// Static method for crawling a category (for use in spawned tasks)
    async fn crawl_category_static(
        config: &InstanceConfig,
        dedup_checker: &Option<SharedDedupChecker>,
        category: &str,
    ) -> Result<u64, RunnerError> {
        let instance_id = config.instance_id.id();

        // Start metrics timer
        let _timer = metrics::start_crawl_timer(instance_id, category);

        // Step 1: Parse category string to NewsCategory enum
        let news_category = NewsCategory::parse(category)
            .ok_or_else(|| RunnerError::CrawlError(format!("Invalid category: {category}")))?;

        tracing::info!(
            category = %category,
            section_id = news_category.to_section_id(),
            "Starting category crawl"
        );

        // Step 2: Get today's date in YYYYMMDD format
        let today = chrono::Local::now().format("%Y%m%d").to_string();

        // Step 3: Create fetcher and list crawler
        let rps = config.requests_per_second.ceil() as u32;
        let fetcher = NaverFetcher::with_config(rps, config.max_retries, config.timeout())
            .map_err(|e| RunnerError::InitError(format!("Failed to create fetcher: {e}")))?;

        let list_crawler = NewsListCrawler::new(fetcher);

        // Step 4: Collect URLs from the category (with pagination)
        let max_pages = 10;
        let all_urls = list_crawler
            .collect_urls(news_category, &today, max_pages)
            .await
            .map_err(|e| RunnerError::CrawlError(format!("Failed to collect URLs: {e}")))?;

        tracing::info!(
            category = %category,
            total_urls = all_urls.len(),
            "Collected article URLs"
        );

        if all_urls.is_empty() {
            return Ok(0);
        }

        // Step 5: Filter new URLs using deduplication checker
        let new_urls = if let Some(checker) = dedup_checker {
            let result = checker
                .batch_check_urls(&all_urls)
                .await
                .map_err(|e| RunnerError::CrawlError(format!("Dedup check failed: {e}")))?;

            tracing::debug!(
                "Dedup check: {} new, {} existing",
                result.new_count(),
                result.existing_count()
            );

            result.new_urls
        } else {
            all_urls.clone()
        };

        let existing_urls = all_urls.len() - new_urls.len();

        // Record deduplication metrics
        metrics::record_dedup_results(instance_id, new_urls.len(), existing_urls);

        tracing::info!(
            category = %category,
            new_urls = new_urls.len(),
            skipped = existing_urls,
            "Filtered URLs (dedup)"
        );

        if new_urls.is_empty() {
            tracing::info!(category = %category, "No new articles to crawl");
            return Ok(0);
        }

        // Step 6: Create pipeline config
        let pipeline_config = PipelineConfig {
            fetcher_workers: 3,
            parser_workers: 2,
            storage_workers: 2,
            channel_buffer_size: 100,
            output_dir: PathBuf::from(&config.output_dir).join("raw"),
            requests_per_second: rps,
            request_timeout: config.timeout(),
            crawl_comments: config.include_comments,
            max_retries: config.max_retries,
        };

        // Step 7: Run the pipeline
        let pipeline = CrawlerPipeline::new(pipeline_config)
            .await
            .map_err(|e| RunnerError::InitError(format!("Failed to create pipeline: {e}")))?;

        let stats = pipeline
            .run(new_urls)
            .await
            .map_err(|e| RunnerError::CrawlError(format!("Pipeline error: {e}")))?;

        tracing::info!(
            category = %category,
            success = stats.success_count,
            failed = stats.failed_count,
            skipped = stats.skipped_count,
            "Category crawl completed"
        );

        // Record metrics for pipeline results
        metrics::record_pipeline_results(
            instance_id,
            category,
            stats.success_count,
            stats.failed_count,
            stats.skipped_count,
        );

        // Record articles crawled for this category
        metrics::record_articles_crawled(instance_id, category, stats.success_count);

        Ok(stats.success_count)
    }

    /// Clone coordinator client (creates new client with same config)
    fn coordinator_clone(&self) -> CoordinatorClient {
        let client_config =
            ClientConfig::new(&self.config.coordinator_url, self.config.instance_id)
                .with_timeout(self.config.timeout())
                .with_retry_count(self.config.max_retries);

        CoordinatorClient::new(client_config).expect("Failed to create coordinator client")
    }

    /// Trigger shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}

// ============================================================================
// Runner Handle
// ============================================================================

/// Handle to the running distributed crawler
pub struct RunnerHandle {
    heartbeat_handle: tokio::task::JoinHandle<()>,
    schedule_handle: tokio::task::JoinHandle<()>,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl RunnerHandle {
    /// Wait for all tasks to complete
    pub async fn wait(self) {
        let _ = tokio::join!(self.heartbeat_handle, self.schedule_handle);
    }

    /// Trigger shutdown and wait
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        self.wait().await;
    }

    /// Check if tasks are still running
    pub fn is_running(&self) -> bool {
        !self.heartbeat_handle.is_finished() && !self.schedule_handle.is_finished()
    }
}

// ============================================================================
// Slot Result
// ============================================================================

/// Result of executing a slot
#[derive(Debug, Clone)]
pub struct SlotResult {
    /// Hour that was executed
    pub hour: u8,

    /// Number of articles crawled
    pub articles_crawled: u64,

    /// Number of errors
    pub errors: u64,

    /// Categories that were crawled
    pub categories: Vec<String>,
}

impl SlotResult {
    /// Check if slot execution was successful
    pub fn is_success(&self) -> bool {
        self.errors == 0
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.articles_crawled == 0 && self.errors == 0 {
            1.0
        } else {
            self.articles_crawled as f64 / (self.articles_crawled + self.errors) as f64
        }
    }
}

// ============================================================================
// Runner Errors
// ============================================================================

/// Runner errors
#[derive(Debug, Clone)]
pub enum RunnerError {
    /// Initialization error
    InitError(String),

    /// Coordinator communication error
    CoordinatorError(String),

    /// Crawling error
    CrawlError(String),

    /// Configuration error
    ConfigError(String),

    /// Shutdown error
    ShutdownError(String),
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitError(msg) => write!(f, "Initialization error: {msg}"),
            Self::CoordinatorError(msg) => write!(f, "Coordinator error: {msg}"),
            Self::CrawlError(msg) => write!(f, "Crawl error: {msg}"),
            Self::ConfigError(msg) => write!(f, "Config error: {msg}"),
            Self::ShutdownError(msg) => write!(f, "Shutdown error: {msg}"),
        }
    }
}

impl std::error::Error for RunnerError {}

impl From<ClientError> for RunnerError {
    fn from(e: ClientError) -> Self {
        Self::CoordinatorError(e.to_string())
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculate time until next hour
pub fn time_until_next_hour() -> Duration {
    let now = chrono::Local::now();
    let next_hour =
        now.with_minute(0).unwrap().with_second(0).unwrap() + chrono::Duration::hours(1);

    (next_hour - now)
        .to_std()
        .unwrap_or(Duration::from_secs(60))
}

/// Calculate time until specific hour (e.g., 23:00 for rotation)
pub fn time_until_hour(target_hour: u32) -> Duration {
    let now = chrono::Local::now();
    let today_target = now
        .with_hour(target_hour)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap();

    let target = if now >= today_target {
        // Target already passed today, use tomorrow
        today_target + chrono::Duration::days(1)
    } else {
        today_target
    };

    (target - now).to_std().unwrap_or(Duration::from_secs(60))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_result() {
        let result = SlotResult {
            hour: 14,
            articles_crawled: 100,
            errors: 0,
            categories: vec!["politics".to_string()],
        };

        assert!(result.is_success());
        assert_eq!(result.success_rate(), 1.0);
    }

    #[test]
    fn test_slot_result_with_errors() {
        let result = SlotResult {
            hour: 14,
            articles_crawled: 90,
            errors: 10,
            categories: vec!["economy".to_string()],
        };

        assert!(!result.is_success());
        assert_eq!(result.success_rate(), 0.9);
    }

    #[test]
    fn test_time_until_next_hour() {
        let duration = time_until_next_hour();
        assert!(duration.as_secs() <= 3600);
    }

    #[test]
    fn test_time_until_hour() {
        let duration = time_until_hour(23);
        assert!(duration.as_secs() <= 86400); // Less than 24 hours
    }

    #[test]
    fn test_runner_error_display() {
        let error = RunnerError::CoordinatorError("connection refused".to_string());
        assert!(error.to_string().contains("Coordinator error"));
    }
}
