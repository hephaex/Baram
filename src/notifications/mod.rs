//! Notification system for monitoring and alerting
//!
//! This module provides a comprehensive notification system that monitors
//! crawling activities, detects anomalies, and sends alerts through various channels.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────┐
//! │      NotificationManager                   │
//! │  - Alert generation                        │
//! │  - Condition evaluation                    │
//! │  - Channel routing                         │
//! │  - Alert lifecycle management              │
//! └────────────────────────────────────────────┘
//!                     │
//!         ┌───────────┼───────────┐
//!         ▼           ▼           ▼
//!   ┌─────────┐ ┌─────────┐ ┌─────────┐
//!   │ Webhook │ │  Email  │ │  Slack  │
//!   │ Channel │ │ Channel │ │ Channel │
//!   └─────────┘ └─────────┘ └─────────┘
//! ```
//!
//! # Features
//!
//! - **Alert Conditions**: Keyword spikes, entity surges, volume anomalies
//! - **Severity Levels**: Info, Warning, Critical
//! - **Multiple Channels**: Webhook, Email, Slack (extensible)
//! - **Alert Lifecycle**: Created → Triggered → Acknowledged → Resolved
//! - **Deduplication**: Prevent alert spam with time-based deduplication
//!
//! # Example
//!
//! ```rust,ignore
//! use baram::notifications::{NotificationManager, AlertCondition, AlertSeverity};
//!
//! let mut manager = NotificationManager::new();
//!
//! // Register a webhook channel
//! manager.add_webhook_channel("https://hooks.example.com/alerts")?;
//!
//! // Create a condition for keyword spikes
//! let condition = AlertCondition::KeywordSpike {
//!     keyword: "경제위기".to_string(),
//!     threshold: 10,
//!     window_minutes: 60,
//! };
//!
//! // Trigger alert when condition is met
//! manager.check_and_alert(condition, AlertSeverity::Warning).await?;
//! ```

pub mod channels;
pub mod conditions;
mod manager;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Re-exports
pub use channels::webhook::WebhookChannel;
pub use channels::Channel;
pub use conditions::AlertCondition;
pub use manager::NotificationManager;

/// Severity level of an alert
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Informational alerts for tracking purposes
    Info,
    /// Warning alerts that require attention
    Warning,
    /// Critical alerts requiring immediate action
    Critical,
}

impl AlertSeverity {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    /// Get Korean description
    pub fn korean_desc(&self) -> &'static str {
        match self {
            Self::Info => "정보",
            Self::Warning => "경고",
            Self::Critical => "긴급",
        }
    }

    /// Get emoji representation
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Info => "ℹ️",
            Self::Warning => "⚠️",
            Self::Critical => "🚨",
        }
    }
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Alert status in the lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertStatus {
    /// Alert created but not yet triggered
    Created,
    /// Alert condition met and notification sent
    Triggered,
    /// Alert acknowledged by user
    Acknowledged,
    /// Alert condition resolved
    Resolved,
}

impl AlertStatus {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Triggered => "triggered",
            Self::Acknowledged => "acknowledged",
            Self::Resolved => "resolved",
        }
    }
}

impl std::fmt::Display for AlertStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An alert instance with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert identifier
    pub id: String,
    /// Alert condition that triggered this alert
    pub condition: AlertCondition,
    /// Severity level
    pub severity: AlertSeverity,
    /// Current status
    pub status: AlertStatus,
    /// Alert message
    pub message: String,
    /// Additional context and metadata
    pub metadata: HashMap<String, String>,
    /// When the alert was created
    pub created_at: DateTime<Utc>,
    /// When the alert was triggered
    pub triggered_at: Option<DateTime<Utc>>,
    /// When the alert was acknowledged
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// Who acknowledged the alert
    pub acknowledged_by: Option<String>,
    /// When the alert was resolved
    pub resolved_at: Option<DateTime<Utc>>,
}

impl Alert {
    /// Create a new alert
    pub fn new(condition: AlertCondition, severity: AlertSeverity, message: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            condition,
            severity,
            status: AlertStatus::Created,
            message,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            triggered_at: None,
            acknowledged_at: None,
            acknowledged_by: None,
            resolved_at: None,
        }
    }

    /// Add metadata to the alert
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Mark alert as triggered
    pub fn trigger(&mut self) {
        self.status = AlertStatus::Triggered;
        self.triggered_at = Some(Utc::now());
    }

    /// Acknowledge the alert
    pub fn acknowledge(&mut self, acknowledged_by: String) {
        self.status = AlertStatus::Acknowledged;
        self.acknowledged_at = Some(Utc::now());
        self.acknowledged_by = Some(acknowledged_by);
    }

    /// Resolve the alert
    pub fn resolve(&mut self) {
        self.status = AlertStatus::Resolved;
        self.resolved_at = Some(Utc::now());
    }

    /// Check if alert is active (triggered but not acknowledged or resolved)
    pub fn is_active(&self) -> bool {
        self.status == AlertStatus::Triggered
    }

    /// Get duration since creation
    pub fn duration_since_creation(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }

    /// Get duration since trigger
    pub fn duration_since_trigger(&self) -> Option<chrono::Duration> {
        self.triggered_at.map(|t| Utc::now() - t)
    }

    /// Format alert for display
    pub fn format_message(&self) -> String {
        format!(
            "[{severity}] {message}\nCondition: {condition}\nStatus: {status}\nCreated: {created}",
            severity = self.severity.as_str().to_uppercase(),
            message = self.message,
            condition = self.condition.description(),
            status = self.status.as_str(),
            created = self.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_severity_display() {
        assert_eq!(AlertSeverity::Info.as_str(), "info");
        assert_eq!(AlertSeverity::Warning.as_str(), "warning");
        assert_eq!(AlertSeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_alert_severity_korean() {
        assert_eq!(AlertSeverity::Info.korean_desc(), "정보");
        assert_eq!(AlertSeverity::Warning.korean_desc(), "경고");
        assert_eq!(AlertSeverity::Critical.korean_desc(), "긴급");
    }

    #[test]
    fn test_alert_creation() {
        let condition = AlertCondition::KeywordSpike {
            keyword: "test".to_string(),
            threshold: 10,
            window_minutes: 60,
        };

        let alert = Alert::new(
            condition,
            AlertSeverity::Warning,
            "Test alert".to_string(),
        );

        assert_eq!(alert.status, AlertStatus::Created);
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert_eq!(alert.message, "Test alert");
        assert!(alert.triggered_at.is_none());
    }

    #[test]
    fn test_alert_lifecycle() {
        let condition = AlertCondition::VolumeAnomaly {
            category: "politics".to_string(),
            threshold_stddev: 2.0,
        };

        let mut alert = Alert::new(
            condition,
            AlertSeverity::Critical,
            "Volume anomaly detected".to_string(),
        );

        // Initial state
        assert_eq!(alert.status, AlertStatus::Created);
        assert!(!alert.is_active());

        // Trigger
        alert.trigger();
        assert_eq!(alert.status, AlertStatus::Triggered);
        assert!(alert.is_active());
        assert!(alert.triggered_at.is_some());

        // Acknowledge
        alert.acknowledge("admin".to_string());
        assert_eq!(alert.status, AlertStatus::Acknowledged);
        assert!(!alert.is_active());
        assert_eq!(alert.acknowledged_by, Some("admin".to_string()));

        // Resolve
        alert.resolve();
        assert_eq!(alert.status, AlertStatus::Resolved);
        assert!(alert.resolved_at.is_some());
    }

    #[test]
    fn test_alert_with_metadata() {
        let condition = AlertCondition::EntitySurge {
            entity: "삼성전자".to_string(),
            threshold: 20,
            window_minutes: 30,
        };

        let alert = Alert::new(condition, AlertSeverity::Info, "Entity surge".to_string())
            .with_metadata("source".to_string(), "crawler".to_string())
            .with_metadata("instance".to_string(), "main".to_string());

        assert_eq!(alert.metadata.get("source"), Some(&"crawler".to_string()));
        assert_eq!(
            alert.metadata.get("instance"),
            Some(&"main".to_string())
        );
    }

    #[test]
    fn test_alert_format_message() {
        let condition = AlertCondition::KeywordSpike {
            keyword: "경제위기".to_string(),
            threshold: 15,
            window_minutes: 120,
        };

        let alert = Alert::new(
            condition,
            AlertSeverity::Warning,
            "Keyword spike detected".to_string(),
        );

        let formatted = alert.format_message();
        assert!(formatted.contains("WARNING"));
        assert!(formatted.contains("Keyword spike detected"));
        assert!(formatted.contains("경제위기"));
    }
}
