//! Hourly crawling trigger for distributed instances
//!
//! This module connects the scheduler trigger system with the distributed crawler,
//! implementing the hourly crawling logic based on category assignments.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, Timelike, Utc};
use tokio::sync::{broadcast, RwLock};

use crate::scheduler::rotation::{CrawlerInstance, NewsCategory, RotationScheduler};
use crate::scheduler::schedule::{HourlySlot, ScheduleCache};

use super::distributed::{DistributedRunner, RunnerError, SlotResult};

// ============================================================================
// Crawler Trigger Configuration
// ============================================================================

/// Configuration for the crawler trigger
#[derive(Debug, Clone)]
pub struct CrawlerTriggerConfig {
    /// Instance ID
    pub instance_id: CrawlerInstance,

    /// Delay before starting crawl after hour boundary (seconds)
    pub start_delay_secs: u64,

    /// Maximum duration for a single crawl cycle (seconds)
    pub max_crawl_duration_secs: u64,

    /// Retry count for failed category crawls
    pub retry_count: u32,

    /// Delay between retries (seconds)
    pub retry_delay_secs: u64,

    /// Enable parallel category crawling
    pub parallel_categories: bool,

    /// Maximum parallel category workers
    pub max_parallel_workers: usize,
}

impl Default for CrawlerTriggerConfig {
    fn default() -> Self {
        Self {
            instance_id: CrawlerInstance::Main,
            start_delay_secs: 5,
            max_crawl_duration_secs: 3300, // 55 minutes max per hour
            retry_count: 3,
            retry_delay_secs: 30,
            parallel_categories: false,
            max_parallel_workers: 2,
        }
    }
}

impl CrawlerTriggerConfig {
    /// Create config for a specific instance
    pub fn for_instance(instance: CrawlerInstance) -> Self {
        Self {
            instance_id: instance,
            ..Default::default()
        }
    }

    /// Set start delay
    pub fn with_start_delay(mut self, secs: u64) -> Self {
        self.start_delay_secs = secs;
        self
    }

    /// Set max crawl duration
    pub fn with_max_duration(mut self, secs: u64) -> Self {
        self.max_crawl_duration_secs = secs;
        self
    }

    /// Enable parallel categories
    pub fn with_parallel_categories(mut self, parallel: bool) -> Self {
        self.parallel_categories = parallel;
        self
    }
}

// ============================================================================
// Crawl Events
// ============================================================================

/// Events emitted by the crawler trigger
#[derive(Debug, Clone)]
pub enum CrawlEvent {
    /// Crawl started for a slot
    Started {
        hour: u8,
        categories: Vec<String>,
        started_at: chrono::DateTime<Utc>,
    },

    /// Category crawl completed
    CategoryCompleted {
        hour: u8,
        category: String,
        articles_count: u64,
        duration_ms: u64,
    },

    /// Category crawl failed
    CategoryFailed {
        hour: u8,
        category: String,
        error: String,
        retry_attempt: u32,
    },

    /// Slot crawl completed
    Completed {
        result: SlotResult,
        duration_ms: u64,
    },

    /// Slot was skipped (not assigned to this instance)
    Skipped { hour: u8, reason: String },

    /// Error occurred
    Error { hour: u8, error: String },
}

// ============================================================================
// Crawler Trigger
// ============================================================================

/// Main crawler trigger that executes hourly crawling
pub struct CrawlerTrigger {
    /// Configuration
    config: CrawlerTriggerConfig,

    /// Rotation scheduler for determining assignments
    scheduler: RotationScheduler,

    /// Schedule cache
    schedule_cache: Arc<ScheduleCache>,

    /// Event sender
    event_sender: broadcast::Sender<CrawlEvent>,

    /// Current crawl state
    state: Arc<RwLock<CrawlState>>,

    /// Shutdown signal
    shutdown: tokio::sync::watch::Sender<bool>,

    /// Shutdown receiver
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

/// Internal crawl state
#[derive(Debug, Clone, Default)]
struct CrawlState {
    /// Currently crawling
    is_crawling: bool,

    /// Current hour being crawled
    current_hour: Option<u8>,

    /// Current category being crawled
    current_category: Option<String>,

    /// Total articles crawled today
    articles_today: u64,

    /// Total errors today
    errors_today: u64,

    /// Last crawl time
    last_crawl: Option<chrono::DateTime<Utc>>,
}

impl CrawlerTrigger {
    /// Create a new crawler trigger
    pub fn new(config: CrawlerTriggerConfig, schedule_cache: Arc<ScheduleCache>) -> Self {
        let (event_sender, _) = broadcast::channel(100);
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);

        Self {
            config,
            scheduler: RotationScheduler::new(),
            schedule_cache,
            event_sender,
            state: Arc::new(RwLock::new(CrawlState::default())),
            shutdown,
            shutdown_rx,
        }
    }

    /// Subscribe to crawl events
    pub fn subscribe(&self) -> broadcast::Receiver<CrawlEvent> {
        self.event_sender.subscribe()
    }

    /// Check if this instance should crawl at the given hour
    pub fn should_crawl_at(&self, hour: u8) -> Option<Vec<NewsCategory>> {
        let today = Local::now().date_naive();
        let schedule = self.scheduler.generate_daily_schedule(today);

        schedule
            .slots
            .iter()
            .find(|slot| slot.hour == hour && slot.instance == self.config.instance_id)
            .map(|slot| slot.categories.clone())
    }

    /// Get the slot for the current hour if assigned to this instance
    pub async fn get_current_slot(&self) -> Option<HourlySlot> {
        let now = Local::now();
        let hour = now.hour() as u8;
        let today = now.date_naive();

        // Try cache first
        if let Some(schedule) = self.schedule_cache.get_for_date(today).await {
            return schedule
                .slots
                .into_iter()
                .find(|slot| slot.hour == hour && slot.instance == self.config.instance_id);
        }

        // Generate schedule
        let schedule = self.scheduler.generate_daily_schedule(today);
        schedule
            .slots
            .into_iter()
            .find(|slot| slot.hour == hour && slot.instance == self.config.instance_id)
    }

    /// Get all slots assigned to this instance today
    pub fn get_my_slots_today(&self) -> Vec<HourlySlot> {
        let today = Local::now().date_naive();
        let schedule = self.scheduler.generate_daily_schedule(today);

        schedule
            .slots
            .into_iter()
            .filter(|slot| slot.instance == self.config.instance_id)
            .collect()
    }

    /// Execute crawl for the current hour
    pub async fn execute_current_hour(
        &self,
        runner: &DistributedRunner,
    ) -> Result<Option<SlotResult>, CrawlerTriggerError> {
        let now = Local::now();
        let hour = now.hour() as u8;

        self.execute_hour(hour, runner).await
    }

    /// Execute crawl for a specific hour
    pub async fn execute_hour(
        &self,
        hour: u8,
        runner: &DistributedRunner,
    ) -> Result<Option<SlotResult>, CrawlerTriggerError> {
        // Check if this instance is assigned to this hour
        let slot = match self.get_slot_for_hour(hour) {
            Some(slot) => slot,
            None => {
                let _ = self.event_sender.send(CrawlEvent::Skipped {
                    hour,
                    reason: format!(
                        "Hour {} is not assigned to instance {}",
                        hour, self.config.instance_id
                    ),
                });
                return Ok(None);
            }
        };

        // Update state
        {
            let mut state = self.state.write().await;
            if state.is_crawling {
                return Err(CrawlerTriggerError::AlreadyCrawling);
            }
            state.is_crawling = true;
            state.current_hour = Some(hour);
        }

        let start_time = std::time::Instant::now();
        let categories: Vec<String> = slot.categories.iter().map(|c| c.to_string()).collect();

        // Emit start event
        let _ = self.event_sender.send(CrawlEvent::Started {
            hour,
            categories: categories.clone(),
            started_at: Utc::now(),
        });

        // Wait for start delay
        if self.config.start_delay_secs > 0 {
            tokio::time::sleep(Duration::from_secs(self.config.start_delay_secs)).await;
        }

        // Execute crawl
        let result = self.execute_categories(runner, &slot).await;

        // Update state
        {
            let mut state = self.state.write().await;
            state.is_crawling = false;
            state.current_hour = None;
            state.current_category = None;
            state.last_crawl = Some(Utc::now());

            if let Ok(ref r) = result {
                state.articles_today += r.articles_crawled;
                state.errors_today += r.errors;
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(slot_result) => {
                let _ = self.event_sender.send(CrawlEvent::Completed {
                    result: slot_result.clone(),
                    duration_ms,
                });
                Ok(Some(slot_result))
            }
            Err(e) => {
                let _ = self.event_sender.send(CrawlEvent::Error {
                    hour,
                    error: e.to_string(),
                });
                Err(e)
            }
        }
    }

    /// Get slot for a specific hour
    fn get_slot_for_hour(&self, hour: u8) -> Option<HourlySlot> {
        let today = Local::now().date_naive();
        let schedule = self.scheduler.generate_daily_schedule(today);

        schedule
            .slots
            .into_iter()
            .find(|slot| slot.hour == hour && slot.instance == self.config.instance_id)
    }

    /// Execute categories for a slot
    async fn execute_categories(
        &self,
        runner: &DistributedRunner,
        slot: &HourlySlot,
    ) -> Result<SlotResult, CrawlerTriggerError> {
        let mut articles_crawled = 0u64;
        let mut errors = 0u64;
        let categories: Vec<String> = slot.categories.iter().map(|c| c.to_string()).collect();

        if self.config.parallel_categories && categories.len() > 1 {
            // Parallel category crawling
            let results = self.execute_categories_parallel(runner, &categories).await;

            for (category, result) in categories.iter().zip(results.iter()) {
                match result {
                    Ok(count) => {
                        articles_crawled += count;
                        let _ = self.event_sender.send(CrawlEvent::CategoryCompleted {
                            hour: slot.hour,
                            category: category.clone(),
                            articles_count: *count,
                            duration_ms: 0, // Not tracked in parallel mode
                        });
                    }
                    Err(e) => {
                        errors += 1;
                        let _ = self.event_sender.send(CrawlEvent::CategoryFailed {
                            hour: slot.hour,
                            category: category.clone(),
                            error: e.to_string(),
                            retry_attempt: 0,
                        });
                    }
                }
            }
        } else {
            // Sequential category crawling
            for category in &categories {
                // Update current category
                {
                    let mut state = self.state.write().await;
                    state.current_category = Some(category.clone());
                }

                let cat_start = std::time::Instant::now();
                let result = self
                    .execute_category_with_retry(runner, category, slot.hour)
                    .await;
                let cat_duration = cat_start.elapsed().as_millis() as u64;

                match result {
                    Ok(count) => {
                        articles_crawled += count;
                        let _ = self.event_sender.send(CrawlEvent::CategoryCompleted {
                            hour: slot.hour,
                            category: category.clone(),
                            articles_count: count,
                            duration_ms: cat_duration,
                        });
                    }
                    Err(e) => {
                        errors += 1;
                        let _ = self.event_sender.send(CrawlEvent::CategoryFailed {
                            hour: slot.hour,
                            category: category.clone(),
                            error: e.to_string(),
                            retry_attempt: self.config.retry_count,
                        });
                    }
                }
            }
        }

        Ok(SlotResult {
            hour: slot.hour,
            articles_crawled,
            errors,
            categories,
        })
    }

    /// Execute categories in parallel
    async fn execute_categories_parallel(
        &self,
        runner: &DistributedRunner,
        categories: &[String],
    ) -> Vec<Result<u64, CrawlerTriggerError>> {
        use futures::stream::{self, StreamExt};

        // Use semaphore to limit parallelism
        let semaphore = Arc::new(tokio::sync::Semaphore::new(
            self.config.max_parallel_workers,
        ));

        // Create a stream of futures
        let results = stream::iter(categories)
            .map(|category| {
                let sem = semaphore.clone();
                let cat = category.clone();

                async move {
                    // Acquire permit
                    let _permit = sem.acquire().await.map_err(|_| {
                        CrawlerTriggerError::ExecutionError("Semaphore closed".to_string())
                    })?;

                    tracing::info!("Starting parallel crawl for category: {}", cat);

                    // Call the actual crawler implementation
                    runner
                        .crawl_category(&cat)
                        .await
                        .map_err(|e| CrawlerTriggerError::ExecutionError(e.to_string()))
                }
            })
            .buffer_unordered(self.config.max_parallel_workers)
            .collect::<Vec<_>>()
            .await;

        results
    }

    /// Execute a single category with retry
    async fn execute_category_with_retry(
        &self,
        runner: &DistributedRunner,
        category: &str,
        hour: u8,
    ) -> Result<u64, CrawlerTriggerError> {
        let mut last_error = None;

        for attempt in 0..=self.config.retry_count {
            if attempt > 0 {
                let _ = self.event_sender.send(CrawlEvent::CategoryFailed {
                    hour,
                    category: category.to_string(),
                    error: last_error.clone().unwrap_or_default(),
                    retry_attempt: attempt,
                });

                tokio::time::sleep(Duration::from_secs(self.config.retry_delay_secs)).await;
            }

            // Call the actual distributed runner's crawl_category method
            match self.crawl_category(runner, category).await {
                Ok(count) => return Ok(count),
                Err(e) => {
                    last_error = Some(e.to_string());
                    tracing::warn!(
                        "Category {} crawl failed (attempt {}): {:?}",
                        category,
                        attempt + 1,
                        last_error
                    );
                }
            }
        }

        Err(CrawlerTriggerError::CategoryCrawlFailed {
            category: category.to_string(),
            error: last_error.unwrap_or_else(|| "Unknown error".to_string()),
        })
    }

    /// Crawl a single category using the DistributedRunner
    async fn crawl_category(
        &self,
        runner: &DistributedRunner,
        category: &str,
    ) -> Result<u64, CrawlerTriggerError> {
        tracing::info!("Starting crawl for category: {}", category);

        // Delegate to the DistributedRunner's fully-implemented crawl_category method
        runner
            .crawl_category(category)
            .await
            .map_err(|e| CrawlerTriggerError::ExecutionError(e.to_string()))
    }

    /// Start the hourly trigger loop
    pub async fn start(&self, runner: Arc<DistributedRunner>) -> Result<(), CrawlerTriggerError> {
        tracing::info!(
            "Starting crawler trigger for instance {}",
            self.config.instance_id
        );

        let mut shutdown_rx = self.shutdown_rx.clone();

        // Calculate initial wait time
        let now = Local::now();
        let next_hour = (now + chrono::Duration::hours(1))
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap();
        let initial_wait = (next_hour - now)
            .to_std()
            .unwrap_or(Duration::from_secs(60));

        tracing::info!("Waiting {:?} until next hour boundary", initial_wait);

        // Wait until next hour
        tokio::select! {
            _ = tokio::time::sleep(initial_wait) => {}
            _ = shutdown_rx.changed() => {
                return Ok(());
            }
        }

        // Main hourly loop
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let hour = Local::now().hour() as u8;
                    tracing::info!("Hour {} triggered, checking assignment", hour);

                    if let Err(e) = self.execute_current_hour(&runner).await {
                        tracing::error!("Error executing hour {}: {}", hour, e);
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("Crawler trigger shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Stop the trigger
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
    }

    /// Get current state
    pub async fn state(&self) -> CrawlerTriggerState {
        let state = self.state.read().await;
        CrawlerTriggerState {
            instance_id: self.config.instance_id,
            is_crawling: state.is_crawling,
            current_hour: state.current_hour,
            current_category: state.current_category.clone(),
            articles_today: state.articles_today,
            errors_today: state.errors_today,
            last_crawl: state.last_crawl,
            assigned_slots: self.get_my_slots_today().len(),
        }
    }
}

/// Public state of the crawler trigger
#[derive(Debug, Clone)]
pub struct CrawlerTriggerState {
    pub instance_id: CrawlerInstance,
    pub is_crawling: bool,
    pub current_hour: Option<u8>,
    pub current_category: Option<String>,
    pub articles_today: u64,
    pub errors_today: u64,
    pub last_crawl: Option<chrono::DateTime<Utc>>,
    pub assigned_slots: usize,
}

// ============================================================================
// Errors
// ============================================================================

/// Errors from crawler trigger
#[derive(Debug, Clone)]
pub enum CrawlerTriggerError {
    /// Already crawling
    AlreadyCrawling,

    /// Category crawl failed after retries
    CategoryCrawlFailed { category: String, error: String },

    /// Execution error
    ExecutionError(String),

    /// Runner error
    RunnerError(String),
}

impl std::fmt::Display for CrawlerTriggerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyCrawling => write!(f, "Already crawling"),
            Self::CategoryCrawlFailed { category, error } => {
                write!(f, "Category {category} crawl failed: {error}")
            }
            Self::ExecutionError(msg) => write!(f, "Execution error: {msg}"),
            Self::RunnerError(msg) => write!(f, "Runner error: {msg}"),
        }
    }
}

impl std::error::Error for CrawlerTriggerError {}

impl From<RunnerError> for CrawlerTriggerError {
    fn from(e: RunnerError) -> Self {
        Self::RunnerError(e.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crawler_trigger_config_default() {
        let config = CrawlerTriggerConfig::default();
        assert_eq!(config.instance_id, CrawlerInstance::Main);
        assert_eq!(config.start_delay_secs, 5);
        assert_eq!(config.max_crawl_duration_secs, 3300);
    }

    #[test]
    fn test_crawler_trigger_config_for_instance() {
        let config = CrawlerTriggerConfig::for_instance(CrawlerInstance::Sub1);
        assert_eq!(config.instance_id, CrawlerInstance::Sub1);
    }

    #[test]
    fn test_crawler_trigger_config_builder() {
        let config = CrawlerTriggerConfig::for_instance(CrawlerInstance::Sub2)
            .with_start_delay(10)
            .with_max_duration(3000)
            .with_parallel_categories(true);

        assert_eq!(config.instance_id, CrawlerInstance::Sub2);
        assert_eq!(config.start_delay_secs, 10);
        assert_eq!(config.max_crawl_duration_secs, 3000);
        assert!(config.parallel_categories);
    }

    #[test]
    fn test_crawler_trigger_should_crawl() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = CrawlerTrigger::new(CrawlerTriggerConfig::default(), cache);

        // Check various hours
        let today = Local::now().date_naive();
        let scheduler = RotationScheduler::new();
        let schedule = scheduler.generate_daily_schedule(today);

        // Find the first hour assigned to Main
        let main_slot = schedule
            .slots
            .iter()
            .find(|s| s.instance == CrawlerInstance::Main);

        if let Some(slot) = main_slot {
            let result = trigger.should_crawl_at(slot.hour);
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_get_my_slots_today() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = CrawlerTrigger::new(CrawlerTriggerConfig::default(), cache);

        let slots = trigger.get_my_slots_today();

        // Main should have 8 slots (24 hours / 3 instances)
        assert_eq!(slots.len(), 8);
        assert!(slots.iter().all(|s| s.instance == CrawlerInstance::Main));
    }

    #[tokio::test]
    async fn test_crawler_trigger_state() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = CrawlerTrigger::new(CrawlerTriggerConfig::default(), cache);

        let state = trigger.state().await;
        assert_eq!(state.instance_id, CrawlerInstance::Main);
        assert!(!state.is_crawling);
        assert!(state.current_hour.is_none());
    }

    #[test]
    fn test_crawler_trigger_error_display() {
        let error = CrawlerTriggerError::CategoryCrawlFailed {
            category: "politics".to_string(),
            error: "timeout".to_string(),
        };

        assert!(error.to_string().contains("politics"));
        assert!(error.to_string().contains("timeout"));
    }
}
