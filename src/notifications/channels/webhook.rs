//! Webhook notification channel
//!
//! This module provides a webhook channel for sending alerts via HTTP POST requests.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{Channel, ChannelError, ChannelResult, DeliveryStatus};
use crate::notifications::Alert;

/// Webhook channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL endpoint
    pub url: String,
    /// Optional authentication token (sent as Bearer token)
    pub auth_token: Option<String>,
    /// Custom headers to include in requests
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Maximum retry attempts on failure
    #[serde(default = "default_retries")]
    pub max_retries: u32,
}

fn default_timeout() -> u64 {
    10
}

fn default_retries() -> u32 {
    3
}

impl WebhookConfig {
    /// Create a new webhook configuration
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            auth_token: None,
            headers: std::collections::HashMap::new(),
            timeout_secs: default_timeout(),
            max_retries: default_retries(),
        }
    }

    /// Set authentication token
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Add a custom header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.url.is_empty() {
            return Err("Webhook URL cannot be empty".to_string());
        }

        // Basic URL validation
        if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            return Err("Webhook URL must start with http:// or https://".to_string());
        }

        if self.timeout_secs == 0 {
            return Err("Timeout must be greater than 0".to_string());
        }

        Ok(())
    }
}

/// Webhook notification channel
///
/// Sends alerts as JSON payloads via HTTP POST requests.
///
/// # Payload Format
///
/// The webhook sends alerts in the following JSON format:
///
/// ```json
/// {
///   "id": "alert-uuid",
///   "severity": "warning",
///   "status": "triggered",
///   "message": "Alert message",
///   "condition": {
///     "type": "keyword_spike",
///     "keyword": "경제위기",
///     "threshold": 10,
///     "window_minutes": 60
///   },
///   "metadata": {
///     "source": "crawler",
///     "instance": "main"
///   },
///   "created_at": "2024-01-01T12:00:00Z",
///   "triggered_at": "2024-01-01T12:05:00Z"
/// }
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use baram::notifications::channels::webhook::{WebhookChannel, WebhookConfig};
///
/// let config = WebhookConfig::new("https://hooks.example.com/alerts")
///     .with_auth_token("secret-token")
///     .with_header("X-Custom-Header", "value")
///     .with_timeout(15)
///     .with_max_retries(5);
///
/// let channel = WebhookChannel::new(config)?;
/// channel.send(&alert).await?;
/// ```
pub struct WebhookChannel {
    config: WebhookConfig,
    client: Client,
}

impl WebhookChannel {
    /// Create a new webhook channel
    pub fn new(config: WebhookConfig) -> ChannelResult<Self> {
        config
            .validate()
            .map_err(|e| ChannelError::InvalidConfig(e))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| ChannelError::Other(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self { config, client })
    }

    /// Create a simple webhook channel with just a URL
    pub fn from_url(url: impl Into<String>) -> ChannelResult<Self> {
        Self::new(WebhookConfig::new(url))
    }

    /// Get the webhook URL
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Build the webhook payload from an alert
    fn build_payload(&self, alert: &Alert) -> serde_json::Value {
        serde_json::json!({
            "id": alert.id,
            "severity": alert.severity.as_str(),
            "status": alert.status.as_str(),
            "message": alert.message,
            "condition": alert.condition,
            "metadata": alert.metadata,
            "created_at": alert.created_at.to_rfc3339(),
            "triggered_at": alert.triggered_at.map(|t| t.to_rfc3339()),
            "acknowledged_at": alert.acknowledged_at.map(|t| t.to_rfc3339()),
            "acknowledged_by": alert.acknowledged_by,
            "resolved_at": alert.resolved_at.map(|t| t.to_rfc3339()),
        })
    }

    /// Send the request with retry logic
    async fn send_with_retry(&self, payload: &serde_json::Value) -> ChannelResult<()> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                // Exponential backoff: 1s, 2s, 4s, 8s...
                let delay = Duration::from_secs(2_u64.pow(attempt - 1));
                tokio::time::sleep(delay).await;
                tracing::debug!(
                    "Retrying webhook request (attempt {}/{})",
                    attempt + 1,
                    self.config.max_retries + 1
                );
            }

            let mut request = self.client.post(&self.config.url);

            // Add authentication if configured
            if let Some(token) = &self.config.auth_token {
                request = request.bearer_auth(token);
            }

            // Add custom headers
            for (key, value) in &self.config.headers {
                request = request.header(key, value);
            }

            // Send request
            match request.json(payload).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        tracing::info!(
                            "Webhook delivered successfully to {} (status: {})",
                            self.config.url,
                            response.status()
                        );
                        return Ok(());
                    } else {
                        let status = response.status();
                        let body = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unable to read response body".to_string());

                        last_error = Some(ChannelError::Other(format!(
                            "HTTP {status}: {body}"
                        )));

                        // Don't retry on client errors (4xx)
                        if status.is_client_error() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(ChannelError::HttpError(e));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ChannelError::Other("Unknown error".to_string())))
    }
}

#[async_trait]
impl Channel for WebhookChannel {
    fn name(&self) -> &str {
        "webhook"
    }

    async fn send(&self, alert: &Alert) -> ChannelResult<DeliveryStatus> {
        let payload = self.build_payload(alert);

        match self.send_with_retry(&payload).await {
            Ok(()) => Ok(DeliveryStatus::success_with_message(
                "webhook",
                format!("Delivered to {}", self.config.url),
            )),
            Err(e) => {
                tracing::error!("Failed to deliver webhook to {}: {}", self.config.url, e);
                Ok(DeliveryStatus::failure("webhook", e.to_string()))
            }
        }
    }

    async fn health_check(&self) -> ChannelResult<bool> {
        // Try to send a HEAD or GET request to check if the endpoint is reachable
        match self.client.head(&self.config.url).send().await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("Webhook health check failed for {}: {}", self.config.url, e);
                Ok(false)
            }
        }
    }

    fn config(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name(),
            "url": self.config.url,
            "timeout_secs": self.config.timeout_secs,
            "max_retries": self.config.max_retries,
            "has_auth": self.config.auth_token.is_some(),
            "custom_headers": self.config.headers.keys().collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::{AlertCondition, AlertSeverity};

    #[test]
    fn test_webhook_config_validation() {
        // Valid config
        let valid = WebhookConfig::new("https://example.com/webhook");
        assert!(valid.validate().is_ok());

        // Invalid: empty URL
        let empty_url = WebhookConfig::new("");
        assert!(empty_url.validate().is_err());

        // Invalid: no protocol
        let no_protocol = WebhookConfig::new("example.com/webhook");
        assert!(no_protocol.validate().is_err());

        // Invalid: zero timeout
        let zero_timeout = WebhookConfig::new("https://example.com").with_timeout(0);
        assert!(zero_timeout.validate().is_err());
    }

    #[test]
    fn test_webhook_config_builder() {
        let config = WebhookConfig::new("https://example.com/webhook")
            .with_auth_token("secret-token")
            .with_header("X-Custom", "value")
            .with_timeout(30)
            .with_max_retries(5);

        assert_eq!(config.url, "https://example.com/webhook");
        assert_eq!(config.auth_token, Some("secret-token".to_string()));
        assert_eq!(config.headers.get("X-Custom"), Some(&"value".to_string()));
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_webhook_creation() {
        let config = WebhookConfig::new("https://example.com/webhook");
        let channel = WebhookChannel::new(config);
        assert!(channel.is_ok());

        let channel = channel.unwrap();
        assert_eq!(channel.name(), "webhook");
        assert_eq!(channel.url(), "https://example.com/webhook");
    }

    #[test]
    fn test_webhook_from_url() {
        let channel = WebhookChannel::from_url("https://example.com/alerts");
        assert!(channel.is_ok());

        let invalid = WebhookChannel::from_url("not-a-url");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_webhook_payload_building() {
        let config = WebhookConfig::new("https://example.com/webhook");
        let channel = WebhookChannel::new(config).unwrap();

        let condition = AlertCondition::KeywordSpike {
            keyword: "test".to_string(),
            threshold: 5,
            window_minutes: 30,
        };

        let mut alert = Alert::new(condition, AlertSeverity::Warning, "Test alert".to_string());
        alert.trigger();

        let payload = channel.build_payload(&alert);

        assert_eq!(payload["severity"], "warning");
        assert_eq!(payload["status"], "triggered");
        assert_eq!(payload["message"], "Test alert");
        assert!(payload["triggered_at"].is_string());
    }

    #[test]
    fn test_webhook_config_serialization() {
        let config = WebhookConfig::new("https://example.com/webhook")
            .with_auth_token("token")
            .with_timeout(20);

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: WebhookConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.url, deserialized.url);
        assert_eq!(config.auth_token, deserialized.auth_token);
        assert_eq!(config.timeout_secs, deserialized.timeout_secs);
    }
}
