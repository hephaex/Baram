//! Coordinator client for crawler instances
//!
//! This module provides a client for crawler instances to communicate
//! with the coordinator server.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::scheduler::rotation::CrawlerInstance;
use crate::scheduler::schedule::ScheduleCache;

use super::registry::{HeartbeatRequest, HeartbeatResponse, RegisterRequest, RegisterResponse};

// ============================================================================
// Client Configuration
// ============================================================================

/// Configuration for the coordinator client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Coordinator server URL
    pub coordinator_url: String,

    /// Request timeout
    pub timeout: Duration,

    /// Retry count for failed requests
    pub retry_count: u32,

    /// Retry delay
    pub retry_delay: Duration,

    /// Instance ID
    pub instance_id: CrawlerInstance,
}

impl ClientConfig {
    /// Create a new client config
    pub fn new(coordinator_url: impl Into<String>, instance_id: CrawlerInstance) -> Self {
        Self {
            coordinator_url: coordinator_url.into(),
            instance_id,
            timeout: Duration::from_secs(10),
            retry_count: 3,
            retry_delay: Duration::from_secs(1),
        }
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set retry count
    pub fn with_retry_count(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }
}

// ============================================================================
// API Response Wrapper
// ============================================================================

/// Generic API response from coordinator
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

// ============================================================================
// Schedule Response Types
// ============================================================================

/// Schedule response from API
#[derive(Debug, Deserialize)]
pub struct ScheduleResponse {
    pub date: String,
    pub slots: Vec<SlotResponse>,
}

#[derive(Debug, Deserialize)]
pub struct SlotResponse {
    pub hour: u8,
    pub instance: String,
    pub categories: Vec<String>,
}

// ============================================================================
// Coordinator Client
// ============================================================================

/// Client for communicating with the Coordinator server
pub struct CoordinatorClient {
    config: ClientConfig,
    http_client: Client,
    cache: ScheduleCache,
}

impl CoordinatorClient {
    /// Create a new coordinator client
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        let http_client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| ClientError::InitError(e.to_string()))?;

        Ok(Self {
            config,
            http_client,
            cache: ScheduleCache::new().with_validity_hours(24),
        })
    }

    /// Register this instance with the coordinator
    pub async fn register(
        &self,
        ip_address: &str,
        port: u16,
    ) -> Result<RegisterResponse, ClientError> {
        let request = RegisterRequest {
            instance_id: self.config.instance_id.id().to_string(),
            ip_address: ip_address.to_string(),
            port,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            metadata: std::collections::HashMap::new(),
        };

        let url = format!("{}/api/instances/register", self.config.coordinator_url);

        self.post_with_retry(&url, &request).await
    }

    /// Send heartbeat to coordinator
    pub async fn heartbeat(
        &self,
        articles_crawled: u64,
        error_count: u64,
        current_category: Option<String>,
    ) -> Result<HeartbeatResponse, ClientError> {
        let request = HeartbeatRequest {
            instance_id: self.config.instance_id.id().to_string(),
            articles_crawled,
            error_count,
            current_category,
        };

        let url = format!("{}/api/instances/heartbeat", self.config.coordinator_url);

        self.post_with_retry(&url, &request).await
    }

    /// Get today's schedule
    pub async fn get_today_schedule(&self) -> Result<ScheduleResponse, ClientError> {
        let url = format!("{}/api/schedule/today", self.config.coordinator_url);
        self.get_with_retry(&url).await
    }

    /// Get schedule for a specific date
    pub async fn get_schedule(&self, date: &str) -> Result<ScheduleResponse, ClientError> {
        let url = format!("{}/api/schedule/{}", self.config.coordinator_url, date);
        self.get_with_retry(&url).await
    }

    /// Get schedule with fallback to cache
    pub async fn get_schedule_with_fallback(&self) -> Result<ScheduleResponse, ClientError> {
        // Try to fetch from coordinator
        match self.get_today_schedule().await {
            Ok(schedule) => Ok(schedule),
            Err(e) => {
                tracing::warn!("Failed to get schedule from coordinator: {}", e);

                // Check cache
                if let Some(cached) = self.cache.get().await {
                    tracing::info!("Using cached schedule");
                    return Ok(ScheduleResponse {
                        date: cached.date.to_string(),
                        slots: cached
                            .slots
                            .iter()
                            .map(|s| SlotResponse {
                                hour: s.hour,
                                instance: s.instance.id().to_string(),
                                categories: s
                                    .categories
                                    .iter()
                                    .map(|c| c.id().to_string())
                                    .collect(),
                            })
                            .collect(),
                    });
                }

                Err(e)
            }
        }
    }

    /// Check coordinator health
    pub async fn health_check(&self) -> Result<HealthStatus, ClientError> {
        let url = format!("{}/api/health", self.config.coordinator_url);

        let response: ApiResponse<HealthResponse> = self.get_with_retry(&url).await?;

        if let Some(health) = response.data {
            Ok(HealthStatus {
                healthy: health.status == "healthy",
                version: health.version,
                uptime_secs: health.uptime_secs,
            })
        } else {
            Err(ClientError::InvalidResponse(
                "Missing health data".to_string(),
            ))
        }
    }

    /// Get the slots assigned to this instance for today
    pub async fn get_my_slots(&self) -> Result<Vec<SlotResponse>, ClientError> {
        let schedule = self.get_today_schedule().await?;
        let my_id = self.config.instance_id.id();

        Ok(schedule
            .slots
            .into_iter()
            .filter(|s| s.instance == my_id)
            .collect())
    }

    /// Check if this instance should crawl at a given hour
    pub async fn should_crawl_at(&self, hour: u8) -> Result<Option<Vec<String>>, ClientError> {
        let schedule = self.get_schedule_with_fallback().await?;
        let my_id = self.config.instance_id.id();

        for slot in schedule.slots {
            if slot.hour == hour && slot.instance == my_id {
                return Ok(Some(slot.categories));
            }
        }

        Ok(None)
    }

    // Internal: GET request with retry
    async fn get_with_retry<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
    ) -> Result<T, ClientError> {
        let mut last_error = None;

        for attempt in 0..=self.config.retry_count {
            if attempt > 0 {
                tokio::time::sleep(self.config.retry_delay).await;
            }

            match self.http_client.get(url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<T>().await {
                            Ok(data) => return Ok(data),
                            Err(e) => {
                                last_error = Some(ClientError::ParseError(e.to_string()));
                            }
                        }
                    } else {
                        last_error = Some(ClientError::HttpError {
                            status: response.status().as_u16(),
                            message: response.text().await.unwrap_or_default(),
                        });
                    }
                }
                Err(e) => {
                    last_error = Some(ClientError::NetworkError(e.to_string()));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ClientError::NetworkError("Unknown error".to_string())))
    }

    // Internal: POST request with retry
    async fn post_with_retry<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R, ClientError> {
        let mut last_error = None;

        for attempt in 0..=self.config.retry_count {
            if attempt > 0 {
                tokio::time::sleep(self.config.retry_delay).await;
            }

            match self.http_client.post(url).json(body).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<R>().await {
                            Ok(data) => return Ok(data),
                            Err(e) => {
                                last_error = Some(ClientError::ParseError(e.to_string()));
                            }
                        }
                    } else {
                        last_error = Some(ClientError::HttpError {
                            status: response.status().as_u16(),
                            message: response.text().await.unwrap_or_default(),
                        });
                    }
                }
                Err(e) => {
                    last_error = Some(ClientError::NetworkError(e.to_string()));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ClientError::NetworkError("Unknown error".to_string())))
    }
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_secs: u64,
}

/// Health status from coordinator
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub version: String,
    pub uptime_secs: u64,
}

// ============================================================================
// Client Errors
// ============================================================================

/// Client errors
#[derive(Debug, Clone)]
pub enum ClientError {
    /// Initialization error
    InitError(String),

    /// Network error
    NetworkError(String),

    /// HTTP error
    HttpError { status: u16, message: String },

    /// Parse error
    ParseError(String),

    /// Invalid response
    InvalidResponse(String),

    /// Coordinator unavailable
    CoordinatorUnavailable,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitError(msg) => write!(f, "Initialization error: {msg}"),
            Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
            Self::HttpError { status, message } => {
                write!(f, "HTTP error ({status}): {message}")
            }
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
            Self::InvalidResponse(msg) => write!(f, "Invalid response: {msg}"),
            Self::CoordinatorUnavailable => write!(f, "Coordinator unavailable"),
        }
    }
}

impl std::error::Error for ClientError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_creation() {
        let config = ClientConfig::new("http://localhost:8080", CrawlerInstance::Main);

        assert_eq!(config.coordinator_url, "http://localhost:8080");
        assert_eq!(config.instance_id, CrawlerInstance::Main);
        assert_eq!(config.retry_count, 3);
    }

    #[test]
    fn test_client_config_with_timeout() {
        let config = ClientConfig::new("http://localhost:8080", CrawlerInstance::Main)
            .with_timeout(Duration::from_secs(30))
            .with_retry_count(5);

        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.retry_count, 5);
    }

    #[test]
    fn test_client_creation() {
        let config = ClientConfig::new("http://localhost:8080", CrawlerInstance::Main);
        let client = CoordinatorClient::new(config);
        assert!(client.is_ok());
    }
}
