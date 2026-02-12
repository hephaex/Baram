//! Notification manager for alert orchestration

use super::channels::Channel;
use super::{Alert, AlertCondition, AlertSeverity, AlertStatus};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// Notification manager that coordinates alerts and channels
#[derive(Default)]
pub struct NotificationManager {
    /// Registered notification channels
    channels: Vec<Box<dyn Channel + Send + Sync>>,

    /// Active alerts by ID
    alerts: HashMap<String, Alert>,

    /// Deduplication: last trigger time per condition key
    last_triggered: HashMap<String, DateTime<Utc>>,

    /// Minimum time between duplicate alerts (minutes)
    dedup_window_minutes: i64,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            alerts: HashMap::new(),
            last_triggered: HashMap::new(),
            dedup_window_minutes: 30,
        }
    }

    /// Set deduplication window in minutes
    pub fn with_dedup_window(mut self, minutes: i64) -> Self {
        self.dedup_window_minutes = minutes;
        self
    }

    /// Add a notification channel
    pub fn add_channel(&mut self, channel: Box<dyn Channel + Send + Sync>) {
        self.channels.push(channel);
    }

    /// Add a webhook channel with URL
    pub fn add_webhook_channel(&mut self, url: &str) -> Result<(), String> {
        let channel =
            super::channels::webhook::WebhookChannel::from_url(url).map_err(|e| e.to_string())?;
        self.add_channel(Box::new(channel));
        Ok(())
    }

    /// Check if an alert should be deduplicated
    fn should_deduplicate(&self, condition_key: &str) -> bool {
        if let Some(&last_time) = self.last_triggered.get(condition_key) {
            let now = Utc::now();
            let elapsed = now - last_time;
            elapsed < Duration::minutes(self.dedup_window_minutes)
        } else {
            false
        }
    }

    /// Create and optionally trigger an alert
    pub fn create_alert(
        &mut self,
        condition: AlertCondition,
        severity: AlertSeverity,
        message: String,
    ) -> Option<Alert> {
        let condition_key = format!("{}:{}", condition.condition_type(), condition.description());

        // Check deduplication
        if self.should_deduplicate(&condition_key) {
            return None;
        }

        let alert = Alert::new(condition, severity, message);
        let alert_id = alert.id.clone();

        self.alerts.insert(alert_id, alert.clone());
        self.last_triggered.insert(condition_key, Utc::now());

        Some(alert)
    }

    /// Trigger an existing alert and send notifications
    pub async fn trigger_alert(&mut self, alert_id: &str) -> Result<(), String> {
        let alert = self
            .alerts
            .get_mut(alert_id)
            .ok_or_else(|| format!("Alert not found: {}", alert_id))?;

        alert.trigger();

        // Send to all channels
        let errors: Vec<String> = Vec::new();
        for channel in &self.channels {
            if let Err(e) = channel.send(alert).await {
                tracing::error!("Failed to send alert to channel: {}", e);
            }
        }

        if !errors.is_empty() {
            return Err(errors.join("; "));
        }

        Ok(())
    }

    /// Create and immediately trigger an alert
    pub async fn alert(
        &mut self,
        condition: AlertCondition,
        severity: AlertSeverity,
        message: String,
    ) -> Result<Option<Alert>, String> {
        if let Some(mut alert) = self.create_alert(condition, severity, message) {
            self.trigger_alert(&alert.id.clone()).await?;
            alert.trigger();
            Ok(Some(alert))
        } else {
            Ok(None)
        }
    }

    /// Acknowledge an alert
    pub fn acknowledge_alert(&mut self, alert_id: &str, by: String) -> Result<(), String> {
        let alert = self
            .alerts
            .get_mut(alert_id)
            .ok_or_else(|| format!("Alert not found: {}", alert_id))?;

        alert.acknowledge(by);
        Ok(())
    }

    /// Resolve an alert
    pub fn resolve_alert(&mut self, alert_id: &str) -> Result<(), String> {
        let alert = self
            .alerts
            .get_mut(alert_id)
            .ok_or_else(|| format!("Alert not found: {}", alert_id))?;

        alert.resolve();
        Ok(())
    }

    /// Get all active alerts
    pub fn active_alerts(&self) -> Vec<&Alert> {
        self.alerts
            .values()
            .filter(|a| a.status == AlertStatus::Triggered)
            .collect()
    }

    /// Get all alerts
    pub fn all_alerts(&self) -> Vec<&Alert> {
        self.alerts.values().collect()
    }

    /// Get an alert by ID
    pub fn get_alert(&self, alert_id: &str) -> Option<&Alert> {
        self.alerts.get(alert_id)
    }

    /// Clean up old resolved alerts
    pub fn cleanup_old_alerts(&mut self, older_than_hours: i64) {
        let cutoff = Utc::now() - Duration::hours(older_than_hours);

        self.alerts.retain(|_, alert| match alert.resolved_at {
            Some(resolved) => resolved > cutoff,
            None => true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_creation() {
        let manager = NotificationManager::new();
        assert!(manager.alerts.is_empty());
        assert!(manager.channels.is_empty());
    }

    #[test]
    fn test_alert_creation_and_dedup() {
        let mut manager = NotificationManager::new().with_dedup_window(5);

        let condition = AlertCondition::KeywordSpike {
            keyword: "test".to_string(),
            threshold: 10,
            window_minutes: 60,
        };

        // First alert should be created
        let alert1 = manager.create_alert(
            condition.clone(),
            AlertSeverity::Warning,
            "Test alert".to_string(),
        );
        assert!(alert1.is_some());

        // Duplicate should be deduplicated
        let alert2 =
            manager.create_alert(condition, AlertSeverity::Warning, "Test alert".to_string());
        assert!(alert2.is_none());
    }

    #[test]
    fn test_alert_lifecycle() {
        let mut manager = NotificationManager::new();

        let condition = AlertCondition::VolumeAnomaly {
            category: "test".to_string(),
            threshold_stddev: 2.0,
        };

        let alert = manager
            .create_alert(condition, AlertSeverity::Info, "Test".to_string())
            .unwrap();

        let alert_id = alert.id.clone();

        // Acknowledge
        manager
            .acknowledge_alert(&alert_id, "admin".to_string())
            .unwrap();
        assert_eq!(
            manager.get_alert(&alert_id).unwrap().status,
            AlertStatus::Acknowledged
        );

        // Resolve
        manager.resolve_alert(&alert_id).unwrap();
        assert_eq!(
            manager.get_alert(&alert_id).unwrap().status,
            AlertStatus::Resolved
        );
    }
}
