//! Instance configuration for distributed crawling
//!
//! This module provides environment-based configuration for crawler instances
//! in the distributed crawling system.

use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

use crate::scheduler::rotation::CrawlerInstance;

// ============================================================================
// Instance Configuration
// ============================================================================

/// Configuration for a distributed crawler instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    /// Instance identifier (main, sub1, sub2)
    pub instance_id: CrawlerInstance,

    /// Coordinator server URL
    pub coordinator_url: String,

    /// Database URL for deduplication
    pub database_url: String,

    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,

    /// Schedule poll interval in seconds
    pub schedule_poll_interval_secs: u64,

    /// Requests per second limit
    pub requests_per_second: f64,

    /// Maximum retries for failed requests
    pub max_retries: u32,

    /// Request timeout in seconds
    pub timeout_secs: u64,

    /// Batch size for deduplication checks
    pub dedup_batch_size: usize,

    /// Whether to check duplicates before fetching
    pub check_before_fetch: bool,

    /// Output directory for markdown files
    pub output_dir: String,

    /// Whether to include comments
    pub include_comments: bool,

    /// Local IP address for registration
    pub local_ip: Option<String>,

    /// Local port for registration
    pub local_port: u16,
}

impl InstanceConfig {
    /// Create configuration from environment variables
    ///
    /// Environment variables:
    /// - `INSTANCE_ID`: Instance identifier (main, sub1, sub2) [required]
    /// - `COORDINATOR_URL`: Coordinator server URL [required]
    /// - `DATABASE_URL`: PostgreSQL connection URL [required]
    /// - `HEARTBEAT_INTERVAL`: Heartbeat interval in seconds [default: 30]
    /// - `SCHEDULE_POLL_INTERVAL`: Schedule poll interval in seconds [default: 60]
    /// - `REQUESTS_PER_SECOND`: Rate limit [default: 1.0]
    /// - `MAX_RETRIES`: Maximum retries [default: 3]
    /// - `TIMEOUT_SECS`: Request timeout [default: 30]
    /// - `DEDUP_BATCH_SIZE`: Batch size for dedup checks [default: 100]
    /// - `CHECK_BEFORE_FETCH`: Check duplicates before fetching [default: true]
    /// - `OUTPUT_DIR`: Output directory [default: ./output]
    /// - `INCLUDE_COMMENTS`: Include comments [default: true]
    /// - `LOCAL_IP`: Local IP for registration [optional]
    /// - `LOCAL_PORT`: Local port for registration [default: 8081]
    pub fn from_env() -> Result<Self, ConfigError> {
        let instance_id_str = env::var("INSTANCE_ID")
            .map_err(|_| ConfigError::MissingEnvVar("INSTANCE_ID".to_string()))?;

        let instance_id = CrawlerInstance::from_id(&instance_id_str)
            .map_err(|_| ConfigError::InvalidInstanceId(instance_id_str))?;

        let coordinator_url = env::var("COORDINATOR_URL")
            .map_err(|_| ConfigError::MissingEnvVar("COORDINATOR_URL".to_string()))?;

        let database_url = env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingEnvVar("DATABASE_URL".to_string()))?;

        Ok(Self {
            instance_id,
            coordinator_url,
            database_url,
            heartbeat_interval_secs: env::var("HEARTBEAT_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            schedule_poll_interval_secs: env::var("SCHEDULE_POLL_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            requests_per_second: env::var("REQUESTS_PER_SECOND")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),
            max_retries: env::var("MAX_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            timeout_secs: env::var("TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            dedup_batch_size: env::var("DEDUP_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            check_before_fetch: env::var("CHECK_BEFORE_FETCH")
                .ok()
                .map(|s| s.to_lowercase() == "true")
                .unwrap_or(true),
            output_dir: env::var("OUTPUT_DIR").unwrap_or_else(|_| "./output".to_string()),
            include_comments: env::var("INCLUDE_COMMENTS")
                .ok()
                .map(|s| s.to_lowercase() == "true")
                .unwrap_or(true),
            local_ip: env::var("LOCAL_IP").ok(),
            local_port: env::var("LOCAL_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8081),
        })
    }

    /// Create configuration with builder pattern
    pub fn builder() -> InstanceConfigBuilder {
        InstanceConfigBuilder::default()
    }

    /// Get heartbeat interval as Duration
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.heartbeat_interval_secs)
    }

    /// Get schedule poll interval as Duration
    pub fn schedule_poll_interval(&self) -> Duration {
        Duration::from_secs(self.schedule_poll_interval_secs)
    }

    /// Get request timeout as Duration
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    /// Get local address for registration
    pub fn local_address(&self) -> String {
        let ip = self.local_ip.clone().unwrap_or_else(|| "0.0.0.0".to_string());
        format!("{}:{}", ip, self.local_port)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.coordinator_url.is_empty() {
            return Err(ConfigError::InvalidValue(
                "coordinator_url".to_string(),
                "URL cannot be empty".to_string(),
            ));
        }

        if self.database_url.is_empty() {
            return Err(ConfigError::InvalidValue(
                "database_url".to_string(),
                "URL cannot be empty".to_string(),
            ));
        }

        if self.requests_per_second <= 0.0 {
            return Err(ConfigError::InvalidValue(
                "requests_per_second".to_string(),
                "Must be positive".to_string(),
            ));
        }

        Ok(())
    }

    /// Display configuration (with sensitive data masked)
    pub fn display(&self) -> String {
        format!(
            "Instance Configuration\n\
             {:-<50}\n\
             Instance ID: {}\n\
             Coordinator: {}\n\
             Database: {}...{}\n\
             Heartbeat Interval: {}s\n\
             Schedule Poll: {}s\n\
             Rate Limit: {} req/s\n\
             Max Retries: {}\n\
             Timeout: {}s\n\
             Dedup Batch: {}\n\
             Check Before Fetch: {}\n\
             Output Dir: {}\n\
             Include Comments: {}\n\
             Local Address: {}",
            "",
            self.instance_id,
            self.coordinator_url,
            &self.database_url[..20.min(self.database_url.len())],
            "***",
            self.heartbeat_interval_secs,
            self.schedule_poll_interval_secs,
            self.requests_per_second,
            self.max_retries,
            self.timeout_secs,
            self.dedup_batch_size,
            self.check_before_fetch,
            self.output_dir,
            self.include_comments,
            self.local_address(),
        )
    }
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            instance_id: CrawlerInstance::Main,
            coordinator_url: "http://localhost:8080".to_string(),
            database_url: "postgres://localhost/ntimes".to_string(),
            heartbeat_interval_secs: 30,
            schedule_poll_interval_secs: 60,
            requests_per_second: 1.0,
            max_retries: 3,
            timeout_secs: 30,
            dedup_batch_size: 100,
            check_before_fetch: true,
            output_dir: "./output".to_string(),
            include_comments: true,
            local_ip: None,
            local_port: 8081,
        }
    }
}

// ============================================================================
// Instance Config Builder
// ============================================================================

/// Builder for InstanceConfig
#[derive(Debug, Default)]
pub struct InstanceConfigBuilder {
    instance_id: Option<CrawlerInstance>,
    coordinator_url: Option<String>,
    database_url: Option<String>,
    heartbeat_interval_secs: Option<u64>,
    schedule_poll_interval_secs: Option<u64>,
    requests_per_second: Option<f64>,
    max_retries: Option<u32>,
    timeout_secs: Option<u64>,
    dedup_batch_size: Option<usize>,
    check_before_fetch: Option<bool>,
    output_dir: Option<String>,
    include_comments: Option<bool>,
    local_ip: Option<String>,
    local_port: Option<u16>,
}

impl InstanceConfigBuilder {
    pub fn instance_id(mut self, id: CrawlerInstance) -> Self {
        self.instance_id = Some(id);
        self
    }

    pub fn coordinator_url(mut self, url: impl Into<String>) -> Self {
        self.coordinator_url = Some(url.into());
        self
    }

    pub fn database_url(mut self, url: impl Into<String>) -> Self {
        self.database_url = Some(url.into());
        self
    }

    pub fn heartbeat_interval_secs(mut self, secs: u64) -> Self {
        self.heartbeat_interval_secs = Some(secs);
        self
    }

    pub fn schedule_poll_interval_secs(mut self, secs: u64) -> Self {
        self.schedule_poll_interval_secs = Some(secs);
        self
    }

    pub fn requests_per_second(mut self, rps: f64) -> Self {
        self.requests_per_second = Some(rps);
        self
    }

    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    pub fn dedup_batch_size(mut self, size: usize) -> Self {
        self.dedup_batch_size = Some(size);
        self
    }

    pub fn check_before_fetch(mut self, check: bool) -> Self {
        self.check_before_fetch = Some(check);
        self
    }

    pub fn output_dir(mut self, dir: impl Into<String>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    pub fn include_comments(mut self, include: bool) -> Self {
        self.include_comments = Some(include);
        self
    }

    pub fn local_ip(mut self, ip: impl Into<String>) -> Self {
        self.local_ip = Some(ip.into());
        self
    }

    pub fn local_port(mut self, port: u16) -> Self {
        self.local_port = Some(port);
        self
    }

    pub fn build(self) -> Result<InstanceConfig, ConfigError> {
        let config = InstanceConfig {
            instance_id: self.instance_id.ok_or_else(|| {
                ConfigError::MissingField("instance_id".to_string())
            })?,
            coordinator_url: self.coordinator_url.ok_or_else(|| {
                ConfigError::MissingField("coordinator_url".to_string())
            })?,
            database_url: self.database_url.ok_or_else(|| {
                ConfigError::MissingField("database_url".to_string())
            })?,
            heartbeat_interval_secs: self.heartbeat_interval_secs.unwrap_or(30),
            schedule_poll_interval_secs: self.schedule_poll_interval_secs.unwrap_or(60),
            requests_per_second: self.requests_per_second.unwrap_or(1.0),
            max_retries: self.max_retries.unwrap_or(3),
            timeout_secs: self.timeout_secs.unwrap_or(30),
            dedup_batch_size: self.dedup_batch_size.unwrap_or(100),
            check_before_fetch: self.check_before_fetch.unwrap_or(true),
            output_dir: self.output_dir.unwrap_or_else(|| "./output".to_string()),
            include_comments: self.include_comments.unwrap_or(true),
            local_ip: self.local_ip,
            local_port: self.local_port.unwrap_or(8081),
        };

        config.validate()?;
        Ok(config)
    }
}

// ============================================================================
// Configuration Errors
// ============================================================================

/// Configuration errors
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// Missing environment variable
    MissingEnvVar(String),

    /// Missing required field
    MissingField(String),

    /// Invalid instance ID
    InvalidInstanceId(String),

    /// Invalid value
    InvalidValue(String, String),

    /// Parse error
    ParseError(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnvVar(var) => write!(f, "Missing environment variable: {var}"),
            Self::MissingField(field) => write!(f, "Missing required field: {field}"),
            Self::InvalidInstanceId(id) => write!(f, "Invalid instance ID: {id}"),
            Self::InvalidValue(field, msg) => write!(f, "Invalid value for {field}: {msg}"),
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

// ============================================================================
// Instance State
// ============================================================================

/// Runtime state for a crawler instance
#[derive(Debug, Clone, Default)]
pub struct InstanceState {
    /// Total articles crawled in current session
    pub articles_crawled: u64,

    /// Total errors in current session
    pub error_count: u64,

    /// Current category being crawled
    pub current_category: Option<String>,

    /// Whether currently crawling
    pub is_crawling: bool,

    /// Last crawl timestamp
    pub last_crawl: Option<chrono::DateTime<chrono::Utc>>,

    /// Session start time
    pub session_start: chrono::DateTime<chrono::Utc>,
}

impl InstanceState {
    /// Create new instance state
    pub fn new() -> Self {
        Self {
            session_start: chrono::Utc::now(),
            ..Default::default()
        }
    }

    /// Record a successful crawl
    pub fn record_success(&mut self, count: u64) {
        self.articles_crawled += count;
        self.last_crawl = Some(chrono::Utc::now());
    }

    /// Record an error
    pub fn record_error(&mut self) {
        self.error_count += 1;
    }

    /// Set current category
    pub fn set_category(&mut self, category: Option<String>) {
        self.current_category = category;
    }

    /// Set crawling state
    pub fn set_crawling(&mut self, crawling: bool) {
        self.is_crawling = crawling;
    }

    /// Get session duration in seconds
    pub fn session_duration_secs(&self) -> i64 {
        (chrono::Utc::now() - self.session_start).num_seconds()
    }

    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        if self.articles_crawled == 0 {
            0.0
        } else {
            self.error_count as f64 / (self.articles_crawled + self.error_count) as f64
        }
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.articles_crawled = 0;
        self.error_count = 0;
        self.current_category = None;
        self.is_crawling = false;
        self.last_crawl = None;
        self.session_start = chrono::Utc::now();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_config_default() {
        let config = InstanceConfig::default();
        assert_eq!(config.instance_id, CrawlerInstance::Main);
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert_eq!(config.requests_per_second, 1.0);
    }

    #[test]
    fn test_instance_config_builder() {
        let config = InstanceConfig::builder()
            .instance_id(CrawlerInstance::Sub1)
            .coordinator_url("http://localhost:8080")
            .database_url("postgres://localhost/test")
            .requests_per_second(2.0)
            .build()
            .unwrap();

        assert_eq!(config.instance_id, CrawlerInstance::Sub1);
        assert_eq!(config.requests_per_second, 2.0);
    }

    #[test]
    fn test_instance_config_builder_missing_field() {
        let result = InstanceConfig::builder()
            .coordinator_url("http://localhost:8080")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_instance_config_validate() {
        let mut config = InstanceConfig::default();
        assert!(config.validate().is_ok());

        config.coordinator_url = "".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_instance_state() {
        let mut state = InstanceState::new();
        assert_eq!(state.articles_crawled, 0);
        assert!(!state.is_crawling);

        state.record_success(5);
        assert_eq!(state.articles_crawled, 5);

        state.record_error();
        assert_eq!(state.error_count, 1);

        // Error rate should be ~16.7% (1 error / 6 total)
        assert!(state.error_rate() > 0.16 && state.error_rate() < 0.17);
    }

    #[test]
    fn test_instance_config_display() {
        let config = InstanceConfig::default();
        let display = config.display();

        assert!(display.contains("Instance ID: main"));
        assert!(display.contains("Heartbeat Interval: 30s"));
    }

    #[test]
    fn test_local_address() {
        let mut config = InstanceConfig::default();
        assert_eq!(config.local_address(), "0.0.0.0:8081");

        config.local_ip = Some("192.168.1.100".to_string());
        config.local_port = 9000;
        assert_eq!(config.local_address(), "192.168.1.100:9000");
    }
}
