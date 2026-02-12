//! Notification channels for delivering alerts
//!
//! This module provides various channels for sending notifications,
//! including webhooks, email, and messaging platforms.

pub mod webhook;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::notifications::Alert;

/// Result type for channel operations
pub type ChannelResult<T> = Result<T, ChannelError>;

/// Errors that can occur during channel operations
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    /// Invalid channel configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Channel temporarily unavailable
    #[error("Channel temporarily unavailable: {0}")]
    Unavailable(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimited(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Generic error
    #[error("Channel error: {0}")]
    Other(String),
}

/// Response from sending a notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryStatus {
    /// Whether the notification was successfully delivered
    pub success: bool,
    /// Channel that delivered (or failed to deliver) the notification
    pub channel: String,
    /// Optional message about the delivery
    pub message: Option<String>,
    /// Timestamp of delivery attempt
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl DeliveryStatus {
    /// Create a successful delivery status
    pub fn success(channel: impl Into<String>) -> Self {
        Self {
            success: true,
            channel: channel.into(),
            message: None,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a successful delivery status with a message
    pub fn success_with_message(channel: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: true,
            channel: channel.into(),
            message: Some(message.into()),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a failed delivery status
    pub fn failure(channel: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            channel: channel.into(),
            message: Some(message.into()),
            timestamp: chrono::Utc::now(),
        }
    }
}

impl fmt::Display for DeliveryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.success { "SUCCESS" } else { "FAILED" };
        write!(f, "[{status}] {}", self.channel)?;
        if let Some(msg) = &self.message {
            write!(f, ": {msg}")?;
        }
        Ok(())
    }
}

/// Trait for notification channels
///
/// Implement this trait to create custom notification channels.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Get the channel name
    fn name(&self) -> &str;

    /// Send an alert through this channel
    async fn send(&self, alert: &Alert) -> ChannelResult<DeliveryStatus>;

    /// Check if the channel is available
    async fn health_check(&self) -> ChannelResult<bool> {
        // Default implementation: always healthy
        Ok(true)
    }

    /// Get channel configuration as JSON
    fn config(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delivery_status_success() {
        let status = DeliveryStatus::success("webhook");
        assert!(status.success);
        assert_eq!(status.channel, "webhook");
        assert!(status.message.is_none());
    }

    #[test]
    fn test_delivery_status_success_with_message() {
        let status = DeliveryStatus::success_with_message("email", "Sent to admin@example.com");
        assert!(status.success);
        assert_eq!(
            status.message,
            Some("Sent to admin@example.com".to_string())
        );
    }

    #[test]
    fn test_delivery_status_failure() {
        let status = DeliveryStatus::failure("slack", "Connection timeout");
        assert!(!status.success);
        assert_eq!(status.channel, "slack");
        assert_eq!(status.message, Some("Connection timeout".to_string()));
    }

    #[test]
    fn test_delivery_status_display() {
        let success = DeliveryStatus::success_with_message("webhook", "Delivered");
        assert!(success.to_string().contains("SUCCESS"));
        assert!(success.to_string().contains("webhook"));

        let failure = DeliveryStatus::failure("email", "SMTP error");
        assert!(failure.to_string().contains("FAILED"));
        assert!(failure.to_string().contains("SMTP error"));
    }
}
