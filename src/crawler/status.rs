//! Status reporting and error recovery for distributed crawlers
//!
//! This module provides:
//! - Periodic status reporting to the coordinator
//! - Error recovery mechanisms for common failure scenarios
//! - Health monitoring and self-healing capabilities

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::coordinator::client::{ClientError, CoordinatorClient};
use crate::scheduler::rotation::CrawlerInstance;

// ============================================================================
// Status Types
// ============================================================================

/// Overall health status of the crawler
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// All systems operational
    Healthy,
    /// Some issues detected but operational
    Degraded,
    /// Critical issues, may need intervention
    Unhealthy,
    /// Recovery in progress
    Recovering,
    /// Starting up
    Starting,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Starting
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Recovering => write!(f, "recovering"),
            Self::Starting => write!(f, "starting"),
        }
    }
}

/// Detailed crawler status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerStatus {
    /// Instance identifier
    pub instance_id: String,

    /// Current health status
    pub health: HealthStatus,

    /// Whether currently crawling
    pub is_crawling: bool,

    /// Current category being crawled
    pub current_category: Option<String>,

    /// Total articles crawled this session
    pub articles_crawled: u64,

    /// Total errors this session
    pub error_count: u64,

    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,

    /// Uptime in seconds
    pub uptime_secs: u64,

    /// Last successful crawl time
    pub last_success: Option<DateTime<Utc>>,

    /// Last error time
    pub last_error: Option<DateTime<Utc>>,

    /// Last error message
    pub last_error_message: Option<String>,

    /// Number of consecutive failures
    pub consecutive_failures: u32,

    /// Memory usage (if available)
    pub memory_mb: Option<u64>,

    /// CPU usage percentage (if available)
    pub cpu_percent: Option<f32>,

    /// Coordinator connection status
    pub coordinator_connected: bool,

    /// Database connection status
    pub database_connected: bool,
}

impl Default for CrawlerStatus {
    fn default() -> Self {
        Self {
            instance_id: String::new(),
            health: HealthStatus::Starting,
            is_crawling: false,
            current_category: None,
            articles_crawled: 0,
            error_count: 0,
            success_rate: 1.0,
            uptime_secs: 0,
            last_success: None,
            last_error: None,
            last_error_message: None,
            consecutive_failures: 0,
            memory_mb: None,
            cpu_percent: None,
            coordinator_connected: false,
            database_connected: false,
        }
    }
}

// ============================================================================
// Error Types for Recovery
// ============================================================================

/// Categories of errors for recovery handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Network-related errors (retryable)
    Network,
    /// Rate limiting or ban detected
    RateLimited,
    /// Database connection error
    Database,
    /// Coordinator communication error
    Coordinator,
    /// Parse error (usually not retryable)
    Parse,
    /// Unknown/other error
    Unknown,
}

impl ErrorCategory {
    /// Check if this error category is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network | Self::RateLimited | Self::Database | Self::Coordinator
        )
    }

    /// Get recommended backoff for this error type
    pub fn recommended_backoff(&self) -> Duration {
        match self {
            Self::Network => Duration::from_secs(5),
            Self::RateLimited => Duration::from_secs(60),
            Self::Database => Duration::from_secs(10),
            Self::Coordinator => Duration::from_secs(30),
            Self::Parse => Duration::from_secs(1),
            Self::Unknown => Duration::from_secs(5),
        }
    }

    /// Classify an error message
    pub fn classify(error_message: &str) -> Self {
        let lower = error_message.to_lowercase();

        // Check more specific patterns first
        if lower.contains("database")
            || lower.contains("postgres")
            || lower.contains("sql")
            || lower.contains("pool")
        {
            Self::Database
        } else if lower.contains("coordinator") || lower.contains("heartbeat") {
            Self::Coordinator
        } else if lower.contains("rate limit")
            || lower.contains("429")
            || lower.contains("too many requests")
            || lower.contains("blocked")
        {
            Self::RateLimited
        } else if lower.contains("timeout")
            || lower.contains("connection")
            || lower.contains("network")
            || lower.contains("dns")
        {
            Self::Network
        } else if lower.contains("parse")
            || lower.contains("invalid")
            || lower.contains("malformed")
        {
            Self::Parse
        } else {
            Self::Unknown
        }
    }
}

/// Error record for tracking
#[derive(Debug, Clone)]
pub struct ErrorRecord {
    /// When the error occurred
    pub timestamp: DateTime<Utc>,

    /// Error category
    pub category: ErrorCategory,

    /// Error message
    pub message: String,

    /// Context (e.g., URL, category)
    pub context: Option<String>,

    /// Whether recovery was attempted
    pub recovery_attempted: bool,

    /// Whether recovery succeeded
    pub recovered: bool,
}

// ============================================================================
// Status Reporter
// ============================================================================

/// Configuration for status reporting
#[derive(Debug, Clone)]
pub struct StatusReporterConfig {
    /// Instance ID
    pub instance_id: CrawlerInstance,

    /// How often to send status updates (seconds)
    pub report_interval_secs: u64,

    /// Maximum errors to keep in history
    pub max_error_history: usize,

    /// Consecutive failures before marking unhealthy
    pub unhealthy_threshold: u32,

    /// Enable automatic recovery
    pub enable_auto_recovery: bool,

    /// Maximum recovery attempts
    pub max_recovery_attempts: u32,
}

impl Default for StatusReporterConfig {
    fn default() -> Self {
        Self {
            instance_id: CrawlerInstance::Main,
            report_interval_secs: 30,
            max_error_history: 100,
            unhealthy_threshold: 5,
            enable_auto_recovery: true,
            max_recovery_attempts: 3,
        }
    }
}

impl StatusReporterConfig {
    /// Create config for a specific instance
    pub fn for_instance(instance: CrawlerInstance) -> Self {
        Self {
            instance_id: instance,
            ..Default::default()
        }
    }

    /// Set report interval
    pub fn with_report_interval(mut self, secs: u64) -> Self {
        self.report_interval_secs = secs;
        self
    }

    /// Set unhealthy threshold
    pub fn with_unhealthy_threshold(mut self, threshold: u32) -> Self {
        self.unhealthy_threshold = threshold;
        self
    }
}

/// Status reporter for crawler instances
pub struct StatusReporter {
    /// Configuration
    config: StatusReporterConfig,

    /// Current status
    status: Arc<RwLock<CrawlerStatus>>,

    /// Error history
    error_history: Arc<RwLock<VecDeque<ErrorRecord>>>,

    /// Start time
    start_time: Instant,

    /// Recovery manager
    recovery: Arc<RwLock<RecoveryManager>>,

    /// Shutdown signal
    shutdown: tokio::sync::watch::Sender<bool>,

    /// Shutdown receiver
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl StatusReporter {
    /// Create a new status reporter
    pub fn new(config: StatusReporterConfig) -> Self {
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);

        let status = CrawlerStatus {
            instance_id: config.instance_id.id().to_string(),
            health: HealthStatus::Starting,
            ..Default::default()
        };

        Self {
            config: config.clone(),
            status: Arc::new(RwLock::new(status)),
            error_history: Arc::new(RwLock::new(VecDeque::with_capacity(config.max_error_history))),
            start_time: Instant::now(),
            recovery: Arc::new(RwLock::new(RecoveryManager::new(config.clone()))),
            shutdown,
            shutdown_rx,
        }
    }

    /// Get current status
    pub async fn get_status(&self) -> CrawlerStatus {
        let mut status = self.status.read().await.clone();
        status.uptime_secs = self.start_time.elapsed().as_secs();
        status
    }

    /// Mark as healthy
    pub async fn mark_healthy(&self) {
        let mut status = self.status.write().await;
        status.health = HealthStatus::Healthy;
        status.consecutive_failures = 0;
    }

    /// Start crawling
    pub async fn start_crawling(&self, category: Option<String>) {
        let mut status = self.status.write().await;
        status.is_crawling = true;
        status.current_category = category;
    }

    /// Stop crawling
    pub async fn stop_crawling(&self) {
        let mut status = self.status.write().await;
        status.is_crawling = false;
        status.current_category = None;
    }

    /// Record a successful operation
    pub async fn record_success(&self, articles_count: u64) {
        let mut status = self.status.write().await;
        status.articles_crawled += articles_count;
        status.last_success = Some(Utc::now());
        status.consecutive_failures = 0;

        // Update success rate
        let total = status.articles_crawled + status.error_count;
        if total > 0 {
            status.success_rate = status.articles_crawled as f64 / total as f64;
        }

        // Update health if recovering
        if status.health == HealthStatus::Recovering {
            status.health = HealthStatus::Healthy;
        }
    }

    /// Record an error
    pub async fn record_error(&self, message: &str, context: Option<&str>) {
        let category = ErrorCategory::classify(message);

        // Update status
        {
            let mut status = self.status.write().await;
            status.error_count += 1;
            status.last_error = Some(Utc::now());
            status.last_error_message = Some(message.to_string());
            status.consecutive_failures += 1;

            // Update success rate
            let total = status.articles_crawled + status.error_count;
            if total > 0 {
                status.success_rate = status.articles_crawled as f64 / total as f64;
            }

            // Check if we should mark as unhealthy
            if status.consecutive_failures >= self.config.unhealthy_threshold {
                status.health = HealthStatus::Unhealthy;
            } else if status.consecutive_failures > 0 {
                status.health = HealthStatus::Degraded;
            }
        }

        // Add to error history
        {
            let mut history = self.error_history.write().await;
            if history.len() >= self.config.max_error_history {
                history.pop_front();
            }
            history.push_back(ErrorRecord {
                timestamp: Utc::now(),
                category,
                message: message.to_string(),
                context: context.map(String::from),
                recovery_attempted: false,
                recovered: false,
            });
        }

        // Check if we should attempt recovery
        if self.config.enable_auto_recovery && category.is_retryable() {
            self.attempt_recovery(category).await;
        }
    }

    /// Attempt recovery based on error category
    async fn attempt_recovery(&self, category: ErrorCategory) {
        let mut recovery = self.recovery.write().await;

        if recovery.can_attempt_recovery() {
            let mut status = self.status.write().await;
            status.health = HealthStatus::Recovering;

            tracing::info!(
                "Attempting recovery for {:?} error (attempt {})",
                category,
                recovery.attempts + 1
            );

            recovery.record_attempt();

            // Wait for recommended backoff
            let backoff = category.recommended_backoff();
            drop(status);
            drop(recovery);

            tokio::time::sleep(backoff).await;
        }
    }

    /// Update connection status
    pub async fn update_connections(&self, coordinator: bool, database: bool) {
        let mut status = self.status.write().await;
        status.coordinator_connected = coordinator;
        status.database_connected = database;
    }

    /// Get error history
    pub async fn get_error_history(&self) -> Vec<ErrorRecord> {
        self.error_history.read().await.iter().cloned().collect()
    }

    /// Get recent errors (last n)
    pub async fn get_recent_errors(&self, count: usize) -> Vec<ErrorRecord> {
        let history = self.error_history.read().await;
        history.iter().rev().take(count).cloned().collect()
    }

    /// Start the status reporting loop
    pub async fn start_reporting(&self, client: Arc<CoordinatorClient>) {
        let status = self.status.clone();
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config.report_interval_secs));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let current = status.read().await;

                        // Send heartbeat with status
                        match client.heartbeat(
                            current.articles_crawled,
                            current.error_count,
                            current.current_category.clone(),
                        ).await {
                            Ok(_) => {
                                tracing::debug!("Status report sent successfully");
                            }
                            Err(e) => {
                                tracing::warn!("Failed to send status report: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("Status reporter shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// Stop the status reporter
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
    }

    /// Reset error counts (e.g., on new day)
    pub async fn reset_daily(&self) {
        let mut status = self.status.write().await;
        status.articles_crawled = 0;
        status.error_count = 0;
        status.success_rate = 1.0;
        status.consecutive_failures = 0;
    }
}

// ============================================================================
// Recovery Manager
// ============================================================================

/// Manages recovery attempts
struct RecoveryManager {
    config: StatusReporterConfig,
    attempts: u32,
    last_attempt: Option<Instant>,
    cooldown: Duration,
}

impl RecoveryManager {
    fn new(config: StatusReporterConfig) -> Self {
        Self {
            config,
            attempts: 0,
            last_attempt: None,
            cooldown: Duration::from_secs(60),
        }
    }

    fn can_attempt_recovery(&self) -> bool {
        // Check if we've exceeded max attempts
        if self.attempts >= self.config.max_recovery_attempts {
            return false;
        }

        // Check cooldown
        if let Some(last) = self.last_attempt {
            if last.elapsed() < self.cooldown {
                return false;
            }
        }

        true
    }

    fn record_attempt(&mut self) {
        self.attempts += 1;
        self.last_attempt = Some(Instant::now());
    }

    fn reset(&mut self) {
        self.attempts = 0;
        self.last_attempt = None;
    }
}

// ============================================================================
// Error Recovery Actions
// ============================================================================

/// Recovery actions that can be taken
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    /// Wait and retry
    Backoff { duration: Duration },
    /// Reconnect to service
    Reconnect { service: String },
    /// Skip current item and continue
    Skip { reason: String },
    /// Reduce rate/concurrency
    Throttle { factor: f32 },
    /// Restart component
    Restart { component: String },
    /// Alert operator
    Alert { message: String },
}

impl RecoveryAction {
    /// Get recommended action for error category
    pub fn recommend(category: ErrorCategory, consecutive_failures: u32) -> Vec<Self> {
        let mut actions = Vec::new();

        match category {
            ErrorCategory::Network => {
                actions.push(Self::Backoff {
                    duration: Duration::from_secs(5 * consecutive_failures as u64),
                });
                if consecutive_failures > 3 {
                    actions.push(Self::Reconnect {
                        service: "network".to_string(),
                    });
                }
            }
            ErrorCategory::RateLimited => {
                actions.push(Self::Backoff {
                    duration: Duration::from_secs(60),
                });
                actions.push(Self::Throttle { factor: 0.5 });
                if consecutive_failures > 2 {
                    actions.push(Self::Alert {
                        message: "Rate limiting detected".to_string(),
                    });
                }
            }
            ErrorCategory::Database => {
                actions.push(Self::Backoff {
                    duration: Duration::from_secs(10),
                });
                actions.push(Self::Reconnect {
                    service: "database".to_string(),
                });
            }
            ErrorCategory::Coordinator => {
                actions.push(Self::Backoff {
                    duration: Duration::from_secs(30),
                });
                actions.push(Self::Reconnect {
                    service: "coordinator".to_string(),
                });
            }
            ErrorCategory::Parse => {
                actions.push(Self::Skip {
                    reason: "Parse error".to_string(),
                });
            }
            ErrorCategory::Unknown => {
                actions.push(Self::Backoff {
                    duration: Duration::from_secs(5),
                });
                if consecutive_failures > 5 {
                    actions.push(Self::Alert {
                        message: "Multiple unknown errors".to_string(),
                    });
                }
            }
        }

        actions
    }
}

// ============================================================================
// Health Check
// ============================================================================

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: HealthStatus,
    pub checks: Vec<ComponentCheck>,
    pub timestamp: DateTime<Utc>,
}

/// Individual component check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentCheck {
    pub name: String,
    pub healthy: bool,
    pub message: Option<String>,
    pub latency_ms: Option<u64>,
}

impl HealthCheck {
    /// Create a new health check
    pub fn new() -> Self {
        Self {
            status: HealthStatus::Starting,
            checks: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Add a component check
    pub fn add_check(&mut self, name: &str, healthy: bool, message: Option<&str>, latency_ms: Option<u64>) {
        self.checks.push(ComponentCheck {
            name: name.to_string(),
            healthy,
            message: message.map(String::from),
            latency_ms,
        });
    }

    /// Calculate overall status
    pub fn calculate_status(&mut self) {
        let total = self.checks.len();
        if total == 0 {
            self.status = HealthStatus::Starting;
            return;
        }

        let unhealthy_count = self.checks.iter().filter(|c| !c.healthy).count();

        self.status = if unhealthy_count == 0 {
            HealthStatus::Healthy
        } else if unhealthy_count * 2 < total {
            // Less than half unhealthy = Degraded
            HealthStatus::Degraded
        } else {
            // Half or more unhealthy = Unhealthy
            HealthStatus::Unhealthy
        };
    }

    /// Check if all components are healthy
    pub fn is_healthy(&self) -> bool {
        self.checks.iter().all(|c| c.healthy)
    }
}

impl Default for HealthCheck {
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
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }

    #[test]
    fn test_error_category_classify() {
        assert_eq!(
            ErrorCategory::classify("Connection timeout"),
            ErrorCategory::Network
        );
        assert_eq!(
            ErrorCategory::classify("Rate limit exceeded"),
            ErrorCategory::RateLimited
        );
        assert_eq!(
            ErrorCategory::classify("Database connection failed"),
            ErrorCategory::Database
        );
        assert_eq!(
            ErrorCategory::classify("Parse error: invalid JSON"),
            ErrorCategory::Parse
        );
    }

    #[test]
    fn test_error_category_is_retryable() {
        assert!(ErrorCategory::Network.is_retryable());
        assert!(ErrorCategory::RateLimited.is_retryable());
        assert!(ErrorCategory::Database.is_retryable());
        assert!(!ErrorCategory::Parse.is_retryable());
    }

    #[test]
    fn test_error_category_backoff() {
        assert!(ErrorCategory::RateLimited.recommended_backoff() > ErrorCategory::Network.recommended_backoff());
    }

    #[test]
    fn test_status_reporter_config() {
        let config = StatusReporterConfig::for_instance(CrawlerInstance::Sub1)
            .with_report_interval(60)
            .with_unhealthy_threshold(10);

        assert_eq!(config.instance_id, CrawlerInstance::Sub1);
        assert_eq!(config.report_interval_secs, 60);
        assert_eq!(config.unhealthy_threshold, 10);
    }

    #[tokio::test]
    async fn test_status_reporter_creation() {
        let config = StatusReporterConfig::default();
        let reporter = StatusReporter::new(config);

        let status = reporter.get_status().await;
        assert_eq!(status.health, HealthStatus::Starting);
        assert!(!status.is_crawling);
    }

    #[tokio::test]
    async fn test_status_reporter_record_success() {
        let config = StatusReporterConfig::default();
        let reporter = StatusReporter::new(config);

        reporter.mark_healthy().await;
        reporter.record_success(10).await;

        let status = reporter.get_status().await;
        assert_eq!(status.articles_crawled, 10);
        assert!(status.last_success.is_some());
        assert_eq!(status.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_status_reporter_record_error() {
        let mut config = StatusReporterConfig::for_instance(CrawlerInstance::Main)
            .with_unhealthy_threshold(3);
        // Disable auto recovery for this test
        config.enable_auto_recovery = false;
        let reporter = StatusReporter::new(config);

        reporter.mark_healthy().await;

        // Record errors (using non-retryable error to avoid recovery)
        reporter.record_error("Parse error invalid JSON", Some("url1")).await;
        let status = reporter.get_status().await;
        assert_eq!(status.health, HealthStatus::Degraded);

        reporter.record_error("Parse error invalid JSON", Some("url2")).await;
        reporter.record_error("Parse error invalid JSON", Some("url3")).await;

        let status = reporter.get_status().await;
        assert_eq!(status.health, HealthStatus::Unhealthy);
        assert_eq!(status.consecutive_failures, 3);
    }

    #[tokio::test]
    async fn test_status_reporter_crawling() {
        let config = StatusReporterConfig::default();
        let reporter = StatusReporter::new(config);

        reporter.start_crawling(Some("politics".to_string())).await;
        let status = reporter.get_status().await;
        assert!(status.is_crawling);
        assert_eq!(status.current_category, Some("politics".to_string()));

        reporter.stop_crawling().await;
        let status = reporter.get_status().await;
        assert!(!status.is_crawling);
        assert!(status.current_category.is_none());
    }

    #[test]
    fn test_recovery_action_recommend() {
        let actions = RecoveryAction::recommend(ErrorCategory::RateLimited, 3);
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|a| matches!(a, RecoveryAction::Throttle { .. })));
    }

    #[test]
    fn test_health_check() {
        let mut check = HealthCheck::new();
        check.add_check("database", true, None, Some(5));
        check.add_check("coordinator", true, None, Some(10));
        check.add_check("network", false, Some("timeout"), None);

        check.calculate_status();
        assert_eq!(check.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_health_check_all_healthy() {
        let mut check = HealthCheck::new();
        check.add_check("database", true, None, Some(5));
        check.add_check("coordinator", true, None, Some(10));

        check.calculate_status();
        assert_eq!(check.status, HealthStatus::Healthy);
        assert!(check.is_healthy());
    }
}
