//! Schedule distribution mechanism
//!
//! This module handles the distribution of schedules from the coordinator
//! to crawler instances, including event broadcasting and synchronization.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use super::rotation::CrawlerInstance;
use super::schedule::DailySchedule;

// ============================================================================
// Distribution Events
// ============================================================================

/// Events broadcast to instances during schedule distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DistributionEvent {
    /// New schedule generated and ready for distribution
    ScheduleReady {
        date: NaiveDate,
        generated_at: DateTime<Utc>,
    },

    /// Schedule has been updated (e.g., due to failover)
    ScheduleUpdated {
        date: NaiveDate,
        reason: UpdateReason,
        affected_hours: Vec<u8>,
    },

    /// Instance assignment changed
    AssignmentChanged {
        instance: CrawlerInstance,
        hour: u8,
        previous: Option<CrawlerInstance>,
    },

    /// Rotation triggered (daily at 23:00 KST)
    RotationTriggered {
        from_date: NaiveDate,
        to_date: NaiveDate,
    },

    /// Emergency override activated
    EmergencyOverride {
        reason: String,
        affected_instance: CrawlerInstance,
    },
}

/// Reasons for schedule updates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateReason {
    /// Regular daily rotation
    DailyRotation,
    /// Instance failover
    Failover,
    /// Manual override by operator
    ManualOverride,
    /// Category rebalancing
    Rebalance,
    /// Emergency situation
    Emergency,
}

// ============================================================================
// Distribution State
// ============================================================================

/// State of a distributed schedule for an instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceScheduleState {
    /// Instance this state is for
    pub instance: CrawlerInstance,

    /// Date of the current schedule
    pub schedule_date: NaiveDate,

    /// Assigned hours for this instance
    pub assigned_hours: Vec<u8>,

    /// Last sync timestamp
    pub last_sync: DateTime<Utc>,

    /// Whether this instance has acknowledged the schedule
    pub acknowledged: bool,

    /// Schedule version
    pub version: u32,
}

impl InstanceScheduleState {
    /// Create new state for an instance
    pub fn new(instance: CrawlerInstance, date: NaiveDate) -> Self {
        Self {
            instance,
            schedule_date: date,
            assigned_hours: Vec::new(),
            last_sync: Utc::now(),
            acknowledged: false,
            version: 1,
        }
    }

    /// Update assigned hours
    pub fn update_hours(&mut self, hours: Vec<u8>) {
        self.assigned_hours = hours;
        self.last_sync = Utc::now();
        self.version += 1;
        self.acknowledged = false;
    }

    /// Mark as acknowledged
    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
    }

    /// Check if sync is stale (older than max_age seconds)
    pub fn is_stale(&self, max_age_secs: i64) -> bool {
        let age = Utc::now() - self.last_sync;
        age.num_seconds() > max_age_secs
    }
}

// ============================================================================
// Schedule Distributor
// ============================================================================

/// Handles distribution of schedules to crawler instances
///
/// The distributor maintains the current schedule state and broadcasts
/// updates to all connected instances.
pub struct ScheduleDistributor {
    /// Current schedule
    schedule: RwLock<Option<DailySchedule>>,

    /// Per-instance state
    instance_states: RwLock<HashMap<CrawlerInstance, InstanceScheduleState>>,

    /// Event broadcaster
    event_tx: broadcast::Sender<DistributionEvent>,

    /// Distribution configuration
    config: DistributionConfig,

    /// Statistics
    stats: RwLock<DistributionStats>,
}

/// Configuration for schedule distribution
#[derive(Debug, Clone)]
pub struct DistributionConfig {
    /// Maximum time to wait for acknowledgments (seconds)
    pub ack_timeout_secs: u64,

    /// Retry count for failed distributions
    pub retry_count: u32,

    /// Whether to require all instances to acknowledge
    pub require_all_acks: bool,

    /// Maximum sync staleness (seconds)
    pub max_sync_staleness_secs: i64,

    /// Event channel capacity
    pub event_channel_capacity: usize,
}

impl Default for DistributionConfig {
    fn default() -> Self {
        Self {
            ack_timeout_secs: 30,
            retry_count: 3,
            require_all_acks: false,
            max_sync_staleness_secs: 300, // 5 minutes
            event_channel_capacity: 100,
        }
    }
}

/// Statistics for distribution
#[derive(Debug, Clone, Default)]
pub struct DistributionStats {
    /// Total distributions performed
    pub total_distributions: u64,

    /// Successful distributions
    pub successful_distributions: u64,

    /// Failed distributions
    pub failed_distributions: u64,

    /// Total events broadcast
    pub events_broadcast: u64,

    /// Acknowledgments received
    pub acks_received: u64,

    /// Last distribution timestamp
    pub last_distribution: Option<DateTime<Utc>>,
}

impl ScheduleDistributor {
    /// Create a new schedule distributor
    pub fn new(config: DistributionConfig) -> Self {
        let (event_tx, _) = broadcast::channel(config.event_channel_capacity);

        Self {
            schedule: RwLock::new(None),
            instance_states: RwLock::new(HashMap::new()),
            event_tx,
            config,
            stats: RwLock::new(DistributionStats::default()),
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(DistributionConfig::default())
    }

    /// Subscribe to distribution events
    pub fn subscribe(&self) -> broadcast::Receiver<DistributionEvent> {
        self.event_tx.subscribe()
    }

    /// Distribute a schedule to all instances
    pub async fn distribute(&self, schedule: DailySchedule) -> DistributionResult {
        let date = schedule.date;

        // Update stored schedule
        *self.schedule.write().await = Some(schedule.clone());

        // Update instance states
        let mut states = self.instance_states.write().await;
        for instance in CrawlerInstance::all() {
            let hours: Vec<u8> = schedule
                .slots
                .iter()
                .filter(|s| s.instance == instance)
                .map(|s| s.hour)
                .collect();

            let state = states
                .entry(instance)
                .or_insert_with(|| InstanceScheduleState::new(instance, date));

            state.schedule_date = date;
            state.update_hours(hours);
        }
        drop(states);

        // Broadcast event
        let event = DistributionEvent::ScheduleReady {
            date,
            generated_at: schedule.generated_at,
        };

        let broadcast_count = self.broadcast_event(event).await;

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_distributions += 1;
        stats.successful_distributions += 1;
        stats.last_distribution = Some(Utc::now());
        drop(stats);

        DistributionResult {
            success: true,
            date,
            instances_notified: broadcast_count,
            errors: Vec::new(),
        }
    }

    /// Update schedule for specific hours (e.g., failover)
    pub async fn update_hours(
        &self,
        date: NaiveDate,
        hours: Vec<u8>,
        new_instance: CrawlerInstance,
        reason: UpdateReason,
    ) -> DistributionResult {
        let mut schedule = self.schedule.write().await;

        if let Some(ref mut sched) = *schedule {
            if sched.date != date {
                return DistributionResult {
                    success: false,
                    date,
                    instances_notified: 0,
                    errors: vec!["Schedule date mismatch".to_string()],
                };
            }

            // Track previous assignments for notification
            let mut changes = Vec::new();

            // Update the slots
            for hour in &hours {
                if let Some(slot) = sched.slots.iter_mut().find(|s| s.hour == *hour) {
                    let previous = slot.instance;
                    slot.instance = new_instance;
                    changes.push((previous, *hour));
                }
            }

            // Update instance states
            let mut states = self.instance_states.write().await;
            for instance in CrawlerInstance::all() {
                if let Some(state) = states.get_mut(&instance) {
                    let new_hours: Vec<u8> = sched
                        .slots
                        .iter()
                        .filter(|s| s.instance == instance)
                        .map(|s| s.hour)
                        .collect();
                    state.update_hours(new_hours);
                }
            }
            drop(states);

            // Broadcast update event
            let event = DistributionEvent::ScheduleUpdated {
                date,
                reason,
                affected_hours: hours,
            };

            let broadcast_count = self.broadcast_event(event).await;

            // Broadcast individual assignment changes
            for (previous, hour) in changes {
                let change_event = DistributionEvent::AssignmentChanged {
                    instance: new_instance,
                    hour,
                    previous: Some(previous),
                };
                self.broadcast_event(change_event).await;
            }

            DistributionResult {
                success: true,
                date,
                instances_notified: broadcast_count,
                errors: Vec::new(),
            }
        } else {
            DistributionResult {
                success: false,
                date,
                instances_notified: 0,
                errors: vec!["No schedule loaded".to_string()],
            }
        }
    }

    /// Trigger daily rotation
    pub async fn trigger_rotation(&self, from_date: NaiveDate, to_date: NaiveDate) {
        let event = DistributionEvent::RotationTriggered { from_date, to_date };
        self.broadcast_event(event).await;
    }

    /// Trigger emergency override
    pub async fn emergency_override(&self, instance: CrawlerInstance, reason: String) {
        let event = DistributionEvent::EmergencyOverride {
            reason,
            affected_instance: instance,
        };
        self.broadcast_event(event).await;
    }

    /// Get current schedule
    pub async fn get_schedule(&self) -> Option<DailySchedule> {
        self.schedule.read().await.clone()
    }

    /// Get state for a specific instance
    pub async fn get_instance_state(
        &self,
        instance: CrawlerInstance,
    ) -> Option<InstanceScheduleState> {
        self.instance_states.read().await.get(&instance).cloned()
    }

    /// Get all instance states
    pub async fn get_all_states(&self) -> HashMap<CrawlerInstance, InstanceScheduleState> {
        self.instance_states.read().await.clone()
    }

    /// Acknowledge schedule for an instance
    pub async fn acknowledge(&self, instance: CrawlerInstance) -> bool {
        let mut states = self.instance_states.write().await;
        if let Some(state) = states.get_mut(&instance) {
            state.acknowledge();

            let mut stats = self.stats.write().await;
            stats.acks_received += 1;
            return true;
        }
        false
    }

    /// Check if all instances have acknowledged
    pub async fn all_acknowledged(&self) -> bool {
        let states = self.instance_states.read().await;
        states.values().all(|s| s.acknowledged)
    }

    /// Get pending (non-acknowledged) instances
    pub async fn pending_instances(&self) -> Vec<CrawlerInstance> {
        self.instance_states
            .read()
            .await
            .iter()
            .filter(|(_, s)| !s.acknowledged)
            .map(|(i, _)| *i)
            .collect()
    }

    /// Get stale instances
    pub async fn stale_instances(&self) -> Vec<CrawlerInstance> {
        self.instance_states
            .read()
            .await
            .iter()
            .filter(|(_, s)| s.is_stale(self.config.max_sync_staleness_secs))
            .map(|(i, _)| *i)
            .collect()
    }

    /// Get distribution statistics
    pub async fn stats(&self) -> DistributionStats {
        self.stats.read().await.clone()
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        *self.stats.write().await = DistributionStats::default();
    }

    // Internal: Broadcast an event
    async fn broadcast_event(&self, event: DistributionEvent) -> usize {
        let count = self.event_tx.receiver_count();
        let _ = self.event_tx.send(event);

        let mut stats = self.stats.write().await;
        stats.events_broadcast += 1;

        count
    }
}

/// Result of a distribution operation
#[derive(Debug, Clone)]
pub struct DistributionResult {
    /// Whether distribution was successful
    pub success: bool,

    /// Date of the distributed schedule
    pub date: NaiveDate,

    /// Number of instances notified
    pub instances_notified: usize,

    /// Any errors encountered
    pub errors: Vec<String>,
}

// ============================================================================
// Schedule Receiver
// ============================================================================

/// Client-side receiver for schedule distributions
///
/// Used by crawler instances to receive schedule updates from the coordinator.
pub struct ScheduleReceiver {
    /// Current schedule for this instance
    current_schedule: RwLock<Option<InstanceScheduleState>>,

    /// Instance ID
    instance: CrawlerInstance,

    /// Last received event
    last_event: RwLock<Option<DistributionEvent>>,
}

impl ScheduleReceiver {
    /// Create a new schedule receiver
    pub fn new(instance: CrawlerInstance) -> Self {
        Self {
            current_schedule: RwLock::new(None),
            instance,
            last_event: RwLock::new(None),
        }
    }

    /// Handle a distribution event
    pub async fn handle_event(&self, event: DistributionEvent) {
        *self.last_event.write().await = Some(event.clone());

        match event {
            DistributionEvent::ScheduleReady { date, .. } => {
                let mut schedule = self.current_schedule.write().await;
                *schedule = Some(InstanceScheduleState::new(self.instance, date));
            }
            DistributionEvent::ScheduleUpdated { date, .. } => {
                let mut schedule = self.current_schedule.write().await;
                if let Some(ref mut s) = *schedule {
                    if s.schedule_date == date {
                        s.version += 1;
                        s.last_sync = Utc::now();
                    }
                }
            }
            _ => {}
        }
    }

    /// Get current schedule state
    pub async fn current_state(&self) -> Option<InstanceScheduleState> {
        self.current_schedule.read().await.clone()
    }

    /// Get last received event
    pub async fn last_event(&self) -> Option<DistributionEvent> {
        self.last_event.read().await.clone()
    }

    /// Get assigned hours for current schedule
    pub async fn assigned_hours(&self) -> Vec<u8> {
        self.current_schedule
            .read()
            .await
            .as_ref()
            .map(|s| s.assigned_hours.clone())
            .unwrap_or_default()
    }
}

// ============================================================================
// Thread-safe Distributor Handle
// ============================================================================

/// Thread-safe handle to a ScheduleDistributor
pub type DistributorHandle = Arc<ScheduleDistributor>;

/// Create a new distributor handle
pub fn create_distributor(config: DistributionConfig) -> DistributorHandle {
    Arc::new(ScheduleDistributor::new(config))
}

/// Create a distributor with default configuration
pub fn create_default_distributor() -> DistributorHandle {
    Arc::new(ScheduleDistributor::with_defaults())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::rotation::RotationScheduler;

    fn create_test_schedule(date: NaiveDate) -> DailySchedule {
        let scheduler = RotationScheduler::new();
        scheduler.generate_daily_schedule(date)
    }

    #[test]
    fn test_instance_schedule_state() {
        let mut state = InstanceScheduleState::new(
            CrawlerInstance::Main,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        );

        assert!(!state.acknowledged);
        assert_eq!(state.version, 1);

        state.update_hours(vec![0, 3, 6, 9]);
        assert_eq!(state.version, 2);
        assert!(!state.acknowledged);

        state.acknowledge();
        assert!(state.acknowledged);
    }

    #[tokio::test]
    async fn test_distributor_distribute() {
        let distributor = ScheduleDistributor::with_defaults();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = create_test_schedule(date);

        let result = distributor.distribute(schedule).await;

        assert!(result.success);
        assert_eq!(result.date, date);

        // Check schedule is stored
        let stored = distributor.get_schedule().await;
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().date, date);
    }

    #[tokio::test]
    async fn test_distributor_instance_states() {
        let distributor = ScheduleDistributor::with_defaults();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = create_test_schedule(date);

        distributor.distribute(schedule).await;

        // All instances should have states
        for instance in CrawlerInstance::all() {
            let state = distributor.get_instance_state(instance).await;
            assert!(state.is_some());
            let state = state.unwrap();
            assert!(!state.assigned_hours.is_empty());
        }
    }

    #[tokio::test]
    async fn test_distributor_acknowledge() {
        let distributor = ScheduleDistributor::with_defaults();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = create_test_schedule(date);

        distributor.distribute(schedule).await;

        // Initially none acknowledged
        assert!(!distributor.all_acknowledged().await);
        assert_eq!(distributor.pending_instances().await.len(), 3);

        // Acknowledge one
        assert!(distributor.acknowledge(CrawlerInstance::Main).await);
        assert_eq!(distributor.pending_instances().await.len(), 2);

        // Acknowledge all
        distributor.acknowledge(CrawlerInstance::Sub1).await;
        distributor.acknowledge(CrawlerInstance::Sub2).await;
        assert!(distributor.all_acknowledged().await);
    }

    #[tokio::test]
    async fn test_distributor_update_hours() {
        let distributor = ScheduleDistributor::with_defaults();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = create_test_schedule(date);

        distributor.distribute(schedule).await;

        // Update hour 5 to be handled by Sub2
        let result = distributor
            .update_hours(date, vec![5], CrawlerInstance::Sub2, UpdateReason::Failover)
            .await;

        assert!(result.success);

        // Verify the update
        let schedule = distributor.get_schedule().await.unwrap();
        let slot = schedule.get_slot(5).unwrap();
        assert_eq!(slot.instance, CrawlerInstance::Sub2);
    }

    #[tokio::test]
    async fn test_distributor_event_subscription() {
        let distributor = ScheduleDistributor::with_defaults();
        let mut receiver = distributor.subscribe();

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = create_test_schedule(date);

        // Distribute (which broadcasts event)
        distributor.distribute(schedule).await;

        // Check we received the event
        let event = receiver.try_recv();
        assert!(event.is_ok());
        match event.unwrap() {
            DistributionEvent::ScheduleReady { date: d, .. } => {
                assert_eq!(d, date);
            }
            _ => panic!("Expected ScheduleReady event"),
        }
    }

    #[tokio::test]
    async fn test_schedule_receiver() {
        let receiver = ScheduleReceiver::new(CrawlerInstance::Main);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        // Handle schedule ready event
        receiver
            .handle_event(DistributionEvent::ScheduleReady {
                date,
                generated_at: Utc::now(),
            })
            .await;

        let state = receiver.current_state().await;
        assert!(state.is_some());
        assert_eq!(state.unwrap().schedule_date, date);
    }

    #[tokio::test]
    async fn test_distributor_stats() {
        let distributor = ScheduleDistributor::with_defaults();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = create_test_schedule(date);

        distributor.distribute(schedule).await;
        distributor.acknowledge(CrawlerInstance::Main).await;

        let stats = distributor.stats().await;
        assert_eq!(stats.total_distributions, 1);
        assert_eq!(stats.successful_distributions, 1);
        assert_eq!(stats.acks_received, 1);
        assert!(stats.last_distribution.is_some());
    }

    #[test]
    fn test_distribution_config_default() {
        let config = DistributionConfig::default();
        assert_eq!(config.ack_timeout_secs, 30);
        assert_eq!(config.retry_count, 3);
        assert!(!config.require_all_acks);
    }
}
