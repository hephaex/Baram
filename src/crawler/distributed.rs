//! Distributed crawler runner
//!
//! This module provides the main runner for distributed crawling that integrates
//! with the coordinator server for schedule management and health reporting.

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

use chrono::Timelike;

use crate::coordinator::client::{ClientConfig, ClientError, CoordinatorClient, SlotResponse};
use crate::scheduler::rotation::CrawlerInstance;

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
/// - Graceful shutdown
pub struct DistributedRunner {
    /// Instance configuration
    config: InstanceConfig,

    /// Coordinator client
    coordinator: CoordinatorClient,

    /// Instance state
    state: Arc<RwLock<InstanceState>>,

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
            shutdown,
            shutdown_rx,
        })
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
        let ip = self.config.local_ip.clone().unwrap_or_else(|| "0.0.0.0".to_string());

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

        Ok(SlotResult {
            hour: slot.hour,
            articles_crawled,
            errors,
            categories: slot.categories.clone(),
        })
    }

    /// Crawl a single category
    async fn crawl_category(&self, _category: &str) -> Result<u64, RunnerError> {
        // TODO: Implement actual crawling logic
        // This should:
        // 1. Fetch article list from Naver
        // 2. Check duplicates in database
        // 3. Crawl new articles
        // 4. Save to storage

        // Placeholder implementation
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(0)
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
        let coordinator = self.coordinator_clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Wait until next hour boundary
            let now = chrono::Local::now();
            let next_hour = now.with_minute(0).unwrap().with_second(0).unwrap() + chrono::Duration::hours(1);
            let wait_duration = (next_hour - now).to_std().unwrap_or(Duration::from_secs(60));

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
                                // TODO: Trigger actual crawling
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

    /// Clone coordinator client (creates new client with same config)
    fn coordinator_clone(&self) -> CoordinatorClient {
        let client_config = ClientConfig::new(&self.config.coordinator_url, self.config.instance_id)
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
            Self::InitError(msg) => write!(f, "Initialization error: {}", msg),
            Self::CoordinatorError(msg) => write!(f, "Coordinator error: {}", msg),
            Self::CrawlError(msg) => write!(f, "Crawl error: {}", msg),
            Self::ConfigError(msg) => write!(f, "Config error: {}", msg),
            Self::ShutdownError(msg) => write!(f, "Shutdown error: {}", msg),
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
    let next_hour = now
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        + chrono::Duration::hours(1);

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

    (target - now)
        .to_std()
        .unwrap_or(Duration::from_secs(60))
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
