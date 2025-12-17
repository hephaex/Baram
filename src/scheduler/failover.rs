//! Failover logic and manual override system
//!
//! This module handles automatic failover when instances become unavailable,
//! and provides manual override capabilities for operators.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::distribution::{ScheduleDistributor, UpdateReason};
use super::rotation::CrawlerInstance;

// ============================================================================
// Instance Health Status
// ============================================================================

/// Health status of a crawler instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Instance is healthy and responding
    Healthy,
    /// Instance is degraded (slow responses, partial failures)
    Degraded,
    /// Instance is unhealthy (not responding)
    Unhealthy,
    /// Instance is in maintenance mode
    Maintenance,
    /// Instance status is unknown
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl HealthStatus {
    /// Check if instance can handle work
    pub fn can_handle_work(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }

    /// Get priority for failover target selection (higher is better)
    pub fn priority(&self) -> u8 {
        match self {
            Self::Healthy => 10,
            Self::Degraded => 5,
            Self::Unhealthy => 0,
            Self::Maintenance => 0,
            Self::Unknown => 1,
        }
    }
}

// ============================================================================
// Instance Health Record
// ============================================================================

/// Health record for a crawler instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceHealth {
    /// Instance identifier
    pub instance: CrawlerInstance,

    /// Current health status
    pub status: HealthStatus,

    /// Last heartbeat received
    pub last_heartbeat: Option<DateTime<Utc>>,

    /// Consecutive failure count
    pub failure_count: u32,

    /// Success count (reset on failure)
    pub success_count: u32,

    /// Last error message
    pub last_error: Option<String>,

    /// When status was last updated
    pub updated_at: DateTime<Utc>,
}

impl InstanceHealth {
    /// Create a new health record
    pub fn new(instance: CrawlerInstance) -> Self {
        Self {
            instance,
            status: HealthStatus::Unknown,
            last_heartbeat: None,
            failure_count: 0,
            success_count: 0,
            last_error: None,
            updated_at: Utc::now(),
        }
    }

    /// Record a successful heartbeat
    pub fn record_success(&mut self) {
        self.last_heartbeat = Some(Utc::now());
        self.success_count += 1;
        self.failure_count = 0;
        self.last_error = None;
        self.updated_at = Utc::now();

        // Update status based on success streak
        self.status = if self.success_count >= 3 {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        };
    }

    /// Record a failed heartbeat
    pub fn record_failure(&mut self, error: Option<String>) {
        self.failure_count += 1;
        self.success_count = 0;
        self.last_error = error;
        self.updated_at = Utc::now();

        // Update status based on failure count
        self.status = if self.failure_count >= 3 {
            HealthStatus::Unhealthy
        } else {
            HealthStatus::Degraded
        };
    }

    /// Set maintenance mode
    pub fn set_maintenance(&mut self, enabled: bool) {
        self.status = if enabled {
            HealthStatus::Maintenance
        } else {
            HealthStatus::Unknown
        };
        self.updated_at = Utc::now();
    }

    /// Check if heartbeat is stale
    pub fn is_stale(&self, max_age_secs: i64) -> bool {
        match self.last_heartbeat {
            Some(last) => (Utc::now() - last).num_seconds() > max_age_secs,
            None => true,
        }
    }

    /// Get seconds since last heartbeat
    pub fn seconds_since_heartbeat(&self) -> Option<i64> {
        self.last_heartbeat.map(|last| (Utc::now() - last).num_seconds())
    }
}

// ============================================================================
// Failover Configuration
// ============================================================================

/// Configuration for failover behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// Maximum consecutive failures before failover
    pub max_failures: u32,

    /// Heartbeat timeout in seconds
    pub heartbeat_timeout_secs: i64,

    /// Minimum time between failovers for same instance (seconds)
    pub failover_cooldown_secs: i64,

    /// Whether to auto-recover when instance comes back
    pub auto_recovery: bool,

    /// Whether to notify on failover
    pub notify_on_failover: bool,

    /// Preferred failover targets (ordered by preference)
    pub failover_order: Vec<CrawlerInstance>,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            max_failures: 3,
            heartbeat_timeout_secs: 60,
            failover_cooldown_secs: 300, // 5 minutes
            auto_recovery: true,
            notify_on_failover: true,
            failover_order: CrawlerInstance::all(),
        }
    }
}

// ============================================================================
// Failover Event
// ============================================================================

/// Event emitted when failover occurs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverEvent {
    /// When the failover occurred
    pub timestamp: DateTime<Utc>,

    /// Instance that failed
    pub failed_instance: CrawlerInstance,

    /// Instance taking over
    pub target_instance: CrawlerInstance,

    /// Hours affected
    pub affected_hours: Vec<u8>,

    /// Reason for failover
    pub reason: FailoverReason,

    /// Whether this was automatic or manual
    pub automatic: bool,
}

/// Reason for failover
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailoverReason {
    /// Instance stopped sending heartbeats
    HeartbeatTimeout,
    /// Too many consecutive failures
    ConsecutiveFailures,
    /// Instance entered maintenance mode
    Maintenance,
    /// Manual failover initiated by operator
    ManualOverride,
    /// Emergency failover
    Emergency,
}

// ============================================================================
// Failover Manager
// ============================================================================

/// Manages instance health and failover logic
pub struct FailoverManager {
    /// Health status for each instance
    health: RwLock<HashMap<CrawlerInstance, InstanceHealth>>,

    /// Failover configuration
    config: FailoverConfig,

    /// Last failover time for each instance (for cooldown)
    last_failover: RwLock<HashMap<CrawlerInstance, DateTime<Utc>>>,

    /// Failover history
    history: RwLock<Vec<FailoverEvent>>,

    /// Optional distributor for automatic schedule updates
    distributor: Option<Arc<ScheduleDistributor>>,
}

impl FailoverManager {
    /// Create a new failover manager
    pub fn new(config: FailoverConfig) -> Self {
        let mut health = HashMap::new();
        for instance in CrawlerInstance::all() {
            health.insert(instance, InstanceHealth::new(instance));
        }

        Self {
            health: RwLock::new(health),
            config,
            last_failover: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
            distributor: None,
        }
    }

    /// Create with default config
    pub fn with_defaults() -> Self {
        Self::new(FailoverConfig::default())
    }

    /// Set distributor for automatic schedule updates
    pub fn with_distributor(mut self, distributor: Arc<ScheduleDistributor>) -> Self {
        self.distributor = Some(distributor);
        self
    }

    /// Process a heartbeat from an instance
    pub async fn process_heartbeat(&self, instance: CrawlerInstance) {
        let mut health = self.health.write().await;
        if let Some(record) = health.get_mut(&instance) {
            record.record_success();
            tracing::debug!("Heartbeat received from {}", instance);
        }
    }

    /// Process a failure from an instance
    pub async fn process_failure(&self, instance: CrawlerInstance, error: Option<String>) {
        let should_failover = {
            let mut health = self.health.write().await;
            if let Some(record) = health.get_mut(&instance) {
                record.record_failure(error);
                record.failure_count >= self.config.max_failures
            } else {
                false
            }
        };

        if should_failover {
            tracing::warn!("Instance {} exceeded max failures, initiating failover", instance);
            if let Err(e) = self.initiate_failover(instance, FailoverReason::ConsecutiveFailures).await {
                tracing::error!("Failover failed: {}", e);
            }
        }
    }

    /// Check for stale instances and trigger failover if needed
    pub async fn check_stale_instances(&self) -> Vec<CrawlerInstance> {
        let stale_instances = {
            let health = self.health.read().await;
            health
                .values()
                .filter(|h| {
                    h.status.can_handle_work() && h.is_stale(self.config.heartbeat_timeout_secs)
                })
                .map(|h| h.instance)
                .collect::<Vec<_>>()
        };

        for instance in &stale_instances {
            tracing::warn!("Instance {} is stale, initiating failover", instance);
            if let Err(e) = self
                .initiate_failover(*instance, FailoverReason::HeartbeatTimeout)
                .await
            {
                tracing::error!("Failover for {} failed: {}", instance, e);
            }
        }

        stale_instances
    }

    /// Initiate failover for an instance
    pub async fn initiate_failover(
        &self,
        failed_instance: CrawlerInstance,
        reason: FailoverReason,
    ) -> Result<FailoverEvent, FailoverError> {
        // Check cooldown
        {
            let last = self.last_failover.read().await;
            if let Some(last_time) = last.get(&failed_instance) {
                let elapsed = (Utc::now() - *last_time).num_seconds();
                if elapsed < self.config.failover_cooldown_secs {
                    return Err(FailoverError::CooldownActive {
                        remaining_secs: self.config.failover_cooldown_secs - elapsed,
                    });
                }
            }
        }

        // Find best target instance
        let target = self.find_failover_target(failed_instance).await?;

        // Mark failed instance as unhealthy
        {
            let mut health = self.health.write().await;
            if let Some(record) = health.get_mut(&failed_instance) {
                record.status = HealthStatus::Unhealthy;
            }
        }

        // Get affected hours (hours assigned to failed instance)
        let affected_hours = self.get_instance_hours(failed_instance).await;

        // Update last failover time
        {
            let mut last = self.last_failover.write().await;
            last.insert(failed_instance, Utc::now());
        }

        // Create failover event
        let event = FailoverEvent {
            timestamp: Utc::now(),
            failed_instance,
            target_instance: target,
            affected_hours: affected_hours.clone(),
            reason,
            automatic: true,
        };

        // Record in history
        {
            let mut history = self.history.write().await;
            history.push(event.clone());
        }

        // Update schedule distribution if distributor is available
        if let Some(ref distributor) = self.distributor {
            let today = chrono::Local::now().date_naive();
            distributor
                .update_hours(today, affected_hours, target, UpdateReason::Failover)
                .await;
        }

        tracing::info!(
            "Failover complete: {} -> {} ({} hours affected)",
            failed_instance,
            target,
            event.affected_hours.len()
        );

        Ok(event)
    }

    /// Find best target instance for failover
    async fn find_failover_target(
        &self,
        exclude: CrawlerInstance,
    ) -> Result<CrawlerInstance, FailoverError> {
        let health = self.health.read().await;

        // First try preferred order
        for instance in &self.config.failover_order {
            if *instance == exclude {
                continue;
            }
            if let Some(record) = health.get(instance) {
                if record.status.can_handle_work() {
                    return Ok(*instance);
                }
            }
        }

        // If no preferred target available, find any healthy instance
        let mut candidates: Vec<_> = health
            .values()
            .filter(|h| h.instance != exclude && h.status.can_handle_work())
            .collect();

        candidates.sort_by(|a, b| b.status.priority().cmp(&a.status.priority()));

        candidates
            .first()
            .map(|h| h.instance)
            .ok_or(FailoverError::NoAvailableTarget)
    }

    /// Get hours currently assigned to an instance
    async fn get_instance_hours(&self, _instance: CrawlerInstance) -> Vec<u8> {
        // In a real implementation, this would query the current schedule
        // For now, return placeholder based on 3-instance rotation
        // Main: 0, 3, 6, 9, 12, 15, 18, 21
        // Sub1: 1, 4, 7, 10, 13, 16, 19, 22
        // Sub2: 2, 5, 8, 11, 14, 17, 20, 23
        match _instance {
            CrawlerInstance::Main => vec![0, 3, 6, 9, 12, 15, 18, 21],
            CrawlerInstance::Sub1 => vec![1, 4, 7, 10, 13, 16, 19, 22],
            CrawlerInstance::Sub2 => vec![2, 5, 8, 11, 14, 17, 20, 23],
        }
    }

    /// Get health status for an instance
    pub async fn get_health(&self, instance: CrawlerInstance) -> Option<InstanceHealth> {
        self.health.read().await.get(&instance).cloned()
    }

    /// Get health status for all instances
    pub async fn get_all_health(&self) -> HashMap<CrawlerInstance, InstanceHealth> {
        self.health.read().await.clone()
    }

    /// Set maintenance mode for an instance
    pub async fn set_maintenance(&self, instance: CrawlerInstance, enabled: bool) {
        let mut health = self.health.write().await;
        if let Some(record) = health.get_mut(&instance) {
            record.set_maintenance(enabled);
        }

        if enabled {
            drop(health);
            // Trigger failover when entering maintenance
            let _ = self
                .initiate_failover(instance, FailoverReason::Maintenance)
                .await;
        }
    }

    /// Get failover history
    pub async fn get_history(&self) -> Vec<FailoverEvent> {
        self.history.read().await.clone()
    }

    /// Get recent failover events (within last n hours)
    pub async fn get_recent_history(&self, hours: i64) -> Vec<FailoverEvent> {
        let cutoff = Utc::now() - chrono::Duration::hours(hours);
        self.history
            .read()
            .await
            .iter()
            .filter(|e| e.timestamp > cutoff)
            .cloned()
            .collect()
    }

    /// Clear failover history
    pub async fn clear_history(&self) {
        self.history.write().await.clear();
    }

    /// Get summary statistics
    pub async fn stats(&self) -> FailoverStats {
        let health = self.health.read().await;
        let history = self.history.read().await;

        let healthy_count = health.values().filter(|h| h.status == HealthStatus::Healthy).count();
        let degraded_count = health.values().filter(|h| h.status == HealthStatus::Degraded).count();
        let unhealthy_count = health.values().filter(|h| h.status == HealthStatus::Unhealthy).count();

        FailoverStats {
            total_instances: CrawlerInstance::count(),
            healthy_count,
            degraded_count,
            unhealthy_count,
            total_failovers: history.len(),
            recent_failovers: history.iter().filter(|e| {
                (Utc::now() - e.timestamp).num_hours() < 24
            }).count(),
        }
    }
}

/// Failover statistics
#[derive(Debug, Clone)]
pub struct FailoverStats {
    pub total_instances: usize,
    pub healthy_count: usize,
    pub degraded_count: usize,
    pub unhealthy_count: usize,
    pub total_failovers: usize,
    pub recent_failovers: usize,
}

/// Failover errors
#[derive(Debug, Clone)]
pub enum FailoverError {
    /// No available target for failover
    NoAvailableTarget,
    /// Failover cooldown is active
    CooldownActive { remaining_secs: i64 },
    /// Instance not found
    InstanceNotFound(CrawlerInstance),
    /// Schedule update failed
    ScheduleUpdateFailed(String),
}

impl std::fmt::Display for FailoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAvailableTarget => write!(f, "No available target for failover"),
            Self::CooldownActive { remaining_secs } => {
                write!(f, "Failover cooldown active, {remaining_secs} seconds remaining")
            }
            Self::InstanceNotFound(i) => write!(f, "Instance not found: {i}"),
            Self::ScheduleUpdateFailed(msg) => write!(f, "Schedule update failed: {msg}"),
        }
    }
}

impl std::error::Error for FailoverError {}

// ============================================================================
// Manual Override
// ============================================================================

/// Manual override request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideRequest {
    /// Target date
    pub date: NaiveDate,

    /// Hours to override
    pub hours: Vec<u8>,

    /// New instance assignment
    pub instance: CrawlerInstance,

    /// Reason for override
    pub reason: String,

    /// Operator identifier
    pub operator: Option<String>,
}

/// Manual override manager
pub struct OverrideManager {
    /// Active overrides
    overrides: RwLock<Vec<ActiveOverride>>,

    /// Optional distributor for schedule updates
    distributor: Option<Arc<ScheduleDistributor>>,
}

/// An active override
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveOverride {
    /// Override ID
    pub id: String,

    /// The override request
    pub request: OverrideRequest,

    /// When the override was created
    pub created_at: DateTime<Utc>,

    /// When the override expires (if any)
    pub expires_at: Option<DateTime<Utc>>,

    /// Whether the override is currently active
    pub active: bool,
}

impl OverrideManager {
    /// Create a new override manager
    pub fn new() -> Self {
        Self {
            overrides: RwLock::new(Vec::new()),
            distributor: None,
        }
    }

    /// Set distributor for automatic schedule updates
    pub fn with_distributor(mut self, distributor: Arc<ScheduleDistributor>) -> Self {
        self.distributor = Some(distributor);
        self
    }

    /// Apply a manual override
    pub async fn apply_override(&self, request: OverrideRequest) -> Result<ActiveOverride, OverrideError> {
        // Validate hours
        for hour in &request.hours {
            if *hour > 23 {
                return Err(OverrideError::InvalidHour(*hour));
            }
        }

        // Create override record
        let override_record = ActiveOverride {
            id: uuid::Uuid::new_v4().to_string(),
            request: request.clone(),
            created_at: Utc::now(),
            expires_at: None,
            active: true,
        };

        // Store override
        {
            let mut overrides = self.overrides.write().await;
            overrides.push(override_record.clone());
        }

        // Update schedule if distributor available
        if let Some(ref distributor) = self.distributor {
            distributor
                .update_hours(
                    request.date,
                    request.hours,
                    request.instance,
                    UpdateReason::ManualOverride,
                )
                .await;
        }

        tracing::info!(
            "Manual override applied: {} hours -> {}",
            override_record.request.hours.len(),
            request.instance
        );

        Ok(override_record)
    }

    /// Cancel an override
    pub async fn cancel_override(&self, id: &str) -> Result<(), OverrideError> {
        let mut overrides = self.overrides.write().await;

        if let Some(override_record) = overrides.iter_mut().find(|o| o.id == id) {
            override_record.active = false;
            Ok(())
        } else {
            Err(OverrideError::NotFound(id.to_string()))
        }
    }

    /// Get active overrides
    pub async fn get_active_overrides(&self) -> Vec<ActiveOverride> {
        self.overrides
            .read()
            .await
            .iter()
            .filter(|o| o.active)
            .cloned()
            .collect()
    }

    /// Get overrides for a specific date
    pub async fn get_overrides_for_date(&self, date: NaiveDate) -> Vec<ActiveOverride> {
        self.overrides
            .read()
            .await
            .iter()
            .filter(|o| o.request.date == date && o.active)
            .cloned()
            .collect()
    }

    /// Clear expired overrides
    pub async fn clear_expired(&self) {
        let now = Utc::now();
        let mut overrides = self.overrides.write().await;
        overrides.retain(|o| {
            if let Some(expires) = o.expires_at {
                expires > now
            } else {
                true
            }
        });
    }
}

impl Default for OverrideManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Override errors
#[derive(Debug, Clone)]
pub enum OverrideError {
    /// Invalid hour specified
    InvalidHour(u8),
    /// Override not found
    NotFound(String),
    /// Override already exists
    AlreadyExists,
    /// Schedule update failed
    ScheduleUpdateFailed(String),
}

impl std::fmt::Display for OverrideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHour(h) => write!(f, "Invalid hour: {h}"),
            Self::NotFound(id) => write!(f, "Override not found: {id}"),
            Self::AlreadyExists => write!(f, "Override already exists"),
            Self::ScheduleUpdateFailed(msg) => write!(f, "Schedule update failed: {msg}"),
        }
    }
}

impl std::error::Error for OverrideError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_can_handle_work() {
        assert!(HealthStatus::Healthy.can_handle_work());
        assert!(HealthStatus::Degraded.can_handle_work());
        assert!(!HealthStatus::Unhealthy.can_handle_work());
        assert!(!HealthStatus::Maintenance.can_handle_work());
    }

    #[test]
    fn test_instance_health_record_success() {
        let mut health = InstanceHealth::new(CrawlerInstance::Main);

        health.record_success();
        assert_eq!(health.failure_count, 0);
        assert_eq!(health.success_count, 1);
        assert!(health.last_heartbeat.is_some());

        // After 3 successes, should be healthy
        health.record_success();
        health.record_success();
        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_instance_health_record_failure() {
        let mut health = InstanceHealth::new(CrawlerInstance::Main);
        health.status = HealthStatus::Healthy;

        health.record_failure(Some("timeout".to_string()));
        assert_eq!(health.failure_count, 1);
        assert_eq!(health.status, HealthStatus::Degraded);

        // After 3 failures, should be unhealthy
        health.record_failure(None);
        health.record_failure(None);
        assert_eq!(health.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_instance_health_maintenance() {
        let mut health = InstanceHealth::new(CrawlerInstance::Sub1);

        health.set_maintenance(true);
        assert_eq!(health.status, HealthStatus::Maintenance);
        assert!(!health.status.can_handle_work());

        health.set_maintenance(false);
        assert_eq!(health.status, HealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_failover_manager_heartbeat() {
        let manager = FailoverManager::with_defaults();

        manager.process_heartbeat(CrawlerInstance::Main).await;

        let health = manager.get_health(CrawlerInstance::Main).await.unwrap();
        assert_eq!(health.success_count, 1);
        assert!(health.last_heartbeat.is_some());
    }

    #[tokio::test]
    async fn test_failover_manager_failure() {
        let config = FailoverConfig {
            max_failures: 2,
            ..Default::default()
        };
        let manager = FailoverManager::new(config);

        manager.process_failure(CrawlerInstance::Main, Some("error".to_string())).await;

        let health = manager.get_health(CrawlerInstance::Main).await.unwrap();
        assert_eq!(health.failure_count, 1);
    }

    #[tokio::test]
    async fn test_failover_manager_find_target() {
        let manager = FailoverManager::with_defaults();

        // Make Main healthy
        manager.process_heartbeat(CrawlerInstance::Main).await;
        manager.process_heartbeat(CrawlerInstance::Main).await;
        manager.process_heartbeat(CrawlerInstance::Main).await;

        // Should find Main as target when excluding Sub1
        let target = manager.find_failover_target(CrawlerInstance::Sub1).await;
        assert!(target.is_ok());
        assert_eq!(target.unwrap(), CrawlerInstance::Main);
    }

    #[tokio::test]
    async fn test_failover_manager_stats() {
        let manager = FailoverManager::with_defaults();

        // Make one instance healthy
        manager.process_heartbeat(CrawlerInstance::Main).await;
        manager.process_heartbeat(CrawlerInstance::Main).await;
        manager.process_heartbeat(CrawlerInstance::Main).await;

        let stats = manager.stats().await;
        assert_eq!(stats.total_instances, 3);
        assert_eq!(stats.healthy_count, 1);
    }

    #[tokio::test]
    async fn test_override_manager_apply() {
        let manager = OverrideManager::new();

        let request = OverrideRequest {
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            hours: vec![14, 15, 16],
            instance: CrawlerInstance::Sub2,
            reason: "Testing".to_string(),
            operator: Some("admin".to_string()),
        };

        let result = manager.apply_override(request).await;
        assert!(result.is_ok());

        let active = manager.get_active_overrides().await;
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn test_override_manager_cancel() {
        let manager = OverrideManager::new();

        let request = OverrideRequest {
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            hours: vec![14],
            instance: CrawlerInstance::Sub1,
            reason: "Test".to_string(),
            operator: None,
        };

        let override_record = manager.apply_override(request).await.unwrap();
        assert_eq!(manager.get_active_overrides().await.len(), 1);

        manager.cancel_override(&override_record.id).await.unwrap();
        assert_eq!(manager.get_active_overrides().await.len(), 0);
    }

    #[tokio::test]
    async fn test_override_invalid_hour() {
        let manager = OverrideManager::new();

        let request = OverrideRequest {
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            hours: vec![25], // Invalid
            instance: CrawlerInstance::Main,
            reason: "Test".to_string(),
            operator: None,
        };

        let result = manager.apply_override(request).await;
        assert!(matches!(result, Err(OverrideError::InvalidHour(25))));
    }

    #[test]
    fn test_failover_config_default() {
        let config = FailoverConfig::default();
        assert_eq!(config.max_failures, 3);
        assert_eq!(config.heartbeat_timeout_secs, 60);
        assert!(config.auto_recovery);
    }
}
