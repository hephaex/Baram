//! Schedule trigger system
//!
//! This module provides mechanisms for triggering schedule generation
//! and execution at specific times (23:00 KST for daily rotation).

use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use super::error::{SchedulerError, SchedulerResult};
use super::rotation::RotationScheduler;
use super::schedule::{DailySchedule, ScheduleCache};

// ============================================================================
// Trigger Configuration
// ============================================================================

/// Configuration for the schedule trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Time to trigger daily rotation (in 24h format, e.g., "23:00")
    pub rotation_time: String,

    /// Timezone for the trigger (e.g., "Asia/Seoul")
    pub timezone: String,

    /// Whether to trigger immediately on startup if past rotation time
    pub trigger_on_startup: bool,

    /// How many minutes before rotation time to start preparing
    pub preparation_minutes: u32,

    /// Enable hourly triggers for crawling
    pub enable_hourly_triggers: bool,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            rotation_time: "23:00".to_string(),
            timezone: "Asia/Seoul".to_string(),
            trigger_on_startup: true,
            preparation_minutes: 5,
            enable_hourly_triggers: true,
        }
    }
}

impl TriggerConfig {
    /// Create a new config builder
    pub fn builder() -> TriggerConfigBuilder {
        TriggerConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> SchedulerResult<()> {
        // Validate rotation time format
        if NaiveTime::parse_from_str(&self.rotation_time, "%H:%M").is_err() {
            return Err(SchedulerError::trigger_config(
                "rotation_time",
                format!("Invalid time format '{}'. Expected HH:MM", self.rotation_time),
            ));
        }

        // Validate timezone (basic check)
        if self.timezone.is_empty() {
            return Err(SchedulerError::trigger_config(
                "timezone",
                "Timezone cannot be empty",
            ));
        }

        Ok(())
    }

    /// Parse the rotation time
    pub fn parse_rotation_time(&self) -> SchedulerResult<NaiveTime> {
        NaiveTime::parse_from_str(&self.rotation_time, "%H:%M").map_err(|_| {
            SchedulerError::trigger_config(
                "rotation_time",
                format!("Invalid time: {}", self.rotation_time),
            )
        })
    }
}

/// Builder for TriggerConfig
#[derive(Debug, Default)]
pub struct TriggerConfigBuilder {
    rotation_time: Option<String>,
    timezone: Option<String>,
    trigger_on_startup: Option<bool>,
    preparation_minutes: Option<u32>,
    enable_hourly_triggers: Option<bool>,
}

impl TriggerConfigBuilder {
    /// Set rotation time
    pub fn rotation_time(mut self, time: impl Into<String>) -> Self {
        self.rotation_time = Some(time.into());
        self
    }

    /// Set timezone
    pub fn timezone(mut self, tz: impl Into<String>) -> Self {
        self.timezone = Some(tz.into());
        self
    }

    /// Set trigger on startup
    pub fn trigger_on_startup(mut self, value: bool) -> Self {
        self.trigger_on_startup = Some(value);
        self
    }

    /// Set preparation minutes
    pub fn preparation_minutes(mut self, minutes: u32) -> Self {
        self.preparation_minutes = Some(minutes);
        self
    }

    /// Enable hourly triggers
    pub fn enable_hourly_triggers(mut self, value: bool) -> Self {
        self.enable_hourly_triggers = Some(value);
        self
    }

    /// Build the config
    pub fn build(self) -> SchedulerResult<TriggerConfig> {
        let config = TriggerConfig {
            rotation_time: self.rotation_time.unwrap_or_else(|| "23:00".to_string()),
            timezone: self.timezone.unwrap_or_else(|| "Asia/Seoul".to_string()),
            trigger_on_startup: self.trigger_on_startup.unwrap_or(true),
            preparation_minutes: self.preparation_minutes.unwrap_or(5),
            enable_hourly_triggers: self.enable_hourly_triggers.unwrap_or(true),
        };
        config.validate()?;
        Ok(config)
    }
}

// ============================================================================
// Trigger Events
// ============================================================================

/// Events emitted by the trigger system
#[derive(Debug, Clone)]
pub enum TriggerEvent {
    /// Daily rotation triggered (at 23:00)
    DailyRotation {
        schedule: DailySchedule,
        triggered_at: DateTime<Utc>,
    },

    /// Hourly crawl slot triggered
    HourlyCrawl {
        hour: u8,
        triggered_at: DateTime<Utc>,
    },

    /// Preparation period started (before rotation)
    PreparationStarted {
        rotation_in_minutes: u32,
    },

    /// Schedule was regenerated (e.g., due to error recovery)
    ScheduleRegenerated {
        reason: String,
        schedule: DailySchedule,
    },
}

// ============================================================================
// Schedule Trigger
// ============================================================================

/// Main trigger system for schedule management
pub struct ScheduleTrigger {
    config: TriggerConfig,
    scheduler: RotationScheduler,
    cache: Arc<ScheduleCache>,
    event_sender: broadcast::Sender<TriggerEvent>,
    is_running: Arc<RwLock<bool>>,
}

impl ScheduleTrigger {
    /// Create a new schedule trigger
    pub fn new(config: TriggerConfig, cache: Arc<ScheduleCache>) -> SchedulerResult<Self> {
        config.validate()?;

        let (event_sender, _) = broadcast::channel(100);

        Ok(Self {
            config,
            scheduler: RotationScheduler::new(),
            cache,
            event_sender,
            is_running: Arc::new(RwLock::new(false)),
        })
    }

    /// Create with default config
    pub fn with_defaults(cache: Arc<ScheduleCache>) -> SchedulerResult<Self> {
        Self::new(TriggerConfig::default(), cache)
    }

    /// Subscribe to trigger events
    pub fn subscribe(&self) -> broadcast::Receiver<TriggerEvent> {
        self.event_sender.subscribe()
    }

    /// Get the current schedule (from cache or generate new)
    pub async fn get_current_schedule(&self) -> SchedulerResult<DailySchedule> {
        // Try cache first
        if let Some(schedule) = self.cache.get().await {
            let today = Local::now().date_naive();
            if schedule.date == today {
                return Ok(schedule);
            }
        }

        // Generate new schedule
        let today = Local::now().date_naive();
        let schedule = self.scheduler.generate_daily_schedule(today);
        self.cache.update(schedule.clone()).await?;

        Ok(schedule)
    }

    /// Generate schedule for tomorrow (called at 23:00)
    pub async fn generate_tomorrow_schedule(&self) -> SchedulerResult<DailySchedule> {
        let tomorrow = Local::now().date_naive() + Duration::days(1);
        let schedule = self.scheduler.generate_daily_schedule(tomorrow);
        self.cache.update(schedule.clone()).await?;

        // Emit event
        let _ = self.event_sender.send(TriggerEvent::DailyRotation {
            schedule: schedule.clone(),
            triggered_at: Utc::now(),
        });

        Ok(schedule)
    }

    /// Calculate duration until next rotation time
    pub fn duration_until_rotation(&self) -> SchedulerResult<Duration> {
        let rotation_time = self.config.parse_rotation_time()?;
        let now = Local::now();
        let today = now.date_naive();

        // Create target datetime for today's rotation
        let target_today = today.and_time(rotation_time);
        let target_dt = Local.from_local_datetime(&target_today).unwrap();

        if now < target_dt {
            // Rotation is later today
            Ok(target_dt.signed_duration_since(now))
        } else {
            // Rotation is tomorrow
            let tomorrow = today + Duration::days(1);
            let target_tomorrow = tomorrow.and_time(rotation_time);
            let target_dt = Local.from_local_datetime(&target_tomorrow).unwrap();
            Ok(target_dt.signed_duration_since(now))
        }
    }

    /// Calculate duration until next hour
    pub fn duration_until_next_hour() -> Duration {
        let now = Local::now();
        let next_hour = (now + Duration::hours(1))
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        next_hour.signed_duration_since(now)
    }

    /// Check if it's time for rotation (within tolerance)
    pub fn is_rotation_time(&self, tolerance_seconds: i64) -> SchedulerResult<bool> {
        let duration = self.duration_until_rotation()?;
        Ok(duration.num_seconds().abs() <= tolerance_seconds)
    }

    /// Start the trigger loop (runs until stopped)
    pub async fn start(&self) -> SchedulerResult<()> {
        *self.is_running.write().await = true;

        // Check if we should trigger on startup
        if self.config.trigger_on_startup {
            self.check_and_generate_schedule().await?;
        }

        // Main trigger loop
        while *self.is_running.read().await {
            let sleep_duration = self.calculate_next_sleep_duration()?;

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration.to_std().unwrap_or(std::time::Duration::from_secs(60))) => {
                    self.handle_trigger().await?;
                }
                _ = self.wait_for_stop() => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Stop the trigger loop
    pub async fn stop(&self) {
        *self.is_running.write().await = false;
    }

    /// Check if trigger is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    // Internal: Wait for stop signal
    async fn wait_for_stop(&self) {
        loop {
            if !*self.is_running.read().await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    // Internal: Calculate next sleep duration
    fn calculate_next_sleep_duration(&self) -> SchedulerResult<Duration> {
        if self.config.enable_hourly_triggers {
            // Sleep until next hour
            let until_hour = Self::duration_until_next_hour();
            let until_rotation = self.duration_until_rotation()?;

            // Wake up at whichever comes first
            if until_hour < until_rotation {
                Ok(until_hour)
            } else {
                Ok(until_rotation)
            }
        } else {
            // Only wake up for rotation
            self.duration_until_rotation()
        }
    }

    // Internal: Handle trigger event
    async fn handle_trigger(&self) -> SchedulerResult<()> {
        let now = Local::now();

        // Check if it's rotation time (23:00)
        if self.is_rotation_time(60)? {
            self.generate_tomorrow_schedule().await?;
        }

        // Check if it's a new hour (for hourly triggers)
        if self.config.enable_hourly_triggers && now.minute() == 0 {
            let _ = self.event_sender.send(TriggerEvent::HourlyCrawl {
                hour: now.hour() as u8,
                triggered_at: Utc::now(),
            });
        }

        Ok(())
    }

    // Internal: Check and generate schedule if needed
    async fn check_and_generate_schedule(&self) -> SchedulerResult<()> {
        let today = Local::now().date_naive();

        // Check if we have a valid schedule for today
        if let Some(schedule) = self.cache.get_for_date(today).await {
            if schedule.is_valid() {
                return Ok(());
            }
        }

        // Generate today's schedule
        let schedule = self.scheduler.generate_daily_schedule(today);
        self.cache.update(schedule.clone()).await?;

        let _ = self.event_sender.send(TriggerEvent::ScheduleRegenerated {
            reason: "Startup initialization".to_string(),
            schedule,
        });

        Ok(())
    }

    /// Manually trigger rotation (for testing or emergency override)
    pub async fn force_rotation(&self) -> SchedulerResult<DailySchedule> {
        let tomorrow = Local::now().date_naive() + Duration::days(1);
        let schedule = self.scheduler.generate_daily_schedule(tomorrow);
        self.cache.update(schedule.clone()).await?;

        let _ = self.event_sender.send(TriggerEvent::ScheduleRegenerated {
            reason: "Manual force rotation".to_string(),
            schedule: schedule.clone(),
        });

        Ok(schedule)
    }

    /// Get trigger status
    pub async fn status(&self) -> TriggerStatus {
        let is_running = *self.is_running.read().await;
        let until_rotation = self.duration_until_rotation().ok();
        let until_next_hour = Self::duration_until_next_hour();
        let cache_status = self.cache.status().await;

        TriggerStatus {
            is_running,
            config: self.config.clone(),
            seconds_until_rotation: until_rotation.map(|d| d.num_seconds()),
            seconds_until_next_hour: until_next_hour.num_seconds(),
            cache_valid: cache_status.is_valid,
            cache_has_schedule: cache_status.has_schedule,
        }
    }
}

/// Trigger status information
#[derive(Debug, Clone)]
pub struct TriggerStatus {
    pub is_running: bool,
    pub config: TriggerConfig,
    pub seconds_until_rotation: Option<i64>,
    pub seconds_until_next_hour: i64,
    pub cache_valid: bool,
    pub cache_has_schedule: bool,
}

impl TriggerStatus {
    /// Format as display string
    pub fn display(&self) -> String {
        let mut output = String::from("Trigger Status\n");
        output.push_str(&format!("{:-<40}\n", ""));
        output.push_str(&format!("Running: {}\n", self.is_running));
        output.push_str(&format!("Rotation Time: {}\n", self.config.rotation_time));
        output.push_str(&format!("Timezone: {}\n", self.config.timezone));

        if let Some(secs) = self.seconds_until_rotation {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            output.push_str(&format!("Until Rotation: {hours}h {mins}m\n"));
        }

        let mins = self.seconds_until_next_hour / 60;
        let secs = self.seconds_until_next_hour % 60;
        output.push_str(&format!("Until Next Hour: {mins}m {secs}s\n"));

        output.push_str(&format!("Cache Valid: {}\n", self.cache_valid));
        output.push_str(&format!("Has Schedule: {}\n", self.cache_has_schedule));

        output
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_config_default() {
        let config = TriggerConfig::default();
        assert_eq!(config.rotation_time, "23:00");
        assert_eq!(config.timezone, "Asia/Seoul");
        assert!(config.trigger_on_startup);
    }

    #[test]
    fn test_trigger_config_validate() {
        let valid = TriggerConfig::default();
        assert!(valid.validate().is_ok());

        let invalid = TriggerConfig {
            rotation_time: "invalid".to_string(),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_trigger_config_builder() {
        let config = TriggerConfig::builder()
            .rotation_time("22:30")
            .timezone("UTC")
            .trigger_on_startup(false)
            .preparation_minutes(10)
            .build()
            .unwrap();

        assert_eq!(config.rotation_time, "22:30");
        assert_eq!(config.timezone, "UTC");
        assert!(!config.trigger_on_startup);
        assert_eq!(config.preparation_minutes, 10);
    }

    #[test]
    fn test_trigger_config_parse_rotation_time() {
        let config = TriggerConfig::default();
        let time = config.parse_rotation_time().unwrap();
        assert_eq!(time.hour(), 23);
        assert_eq!(time.minute(), 0);
    }

    #[tokio::test]
    async fn test_schedule_trigger_creation() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = ScheduleTrigger::with_defaults(cache).unwrap();

        assert!(!trigger.is_running().await);
    }

    #[tokio::test]
    async fn test_schedule_trigger_get_current_schedule() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = ScheduleTrigger::with_defaults(cache).unwrap();

        let schedule = trigger.get_current_schedule().await.unwrap();
        let today = Local::now().date_naive();

        assert_eq!(schedule.date, today);
        assert!(schedule.is_valid());
    }

    #[tokio::test]
    async fn test_schedule_trigger_subscribe() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = ScheduleTrigger::with_defaults(cache).unwrap();

        let mut receiver = trigger.subscribe();

        // Force rotation should send an event
        trigger.force_rotation().await.unwrap();

        // Should receive the event
        let event = receiver.try_recv();
        assert!(event.is_ok());

        match event.unwrap() {
            TriggerEvent::ScheduleRegenerated { reason, .. } => {
                assert!(reason.contains("Manual"));
            }
            _ => panic!("Unexpected event type"),
        }
    }

    #[test]
    fn test_duration_until_next_hour() {
        let duration = ScheduleTrigger::duration_until_next_hour();

        // Should be less than 1 hour
        assert!(duration.num_seconds() <= 3600);
        assert!(duration.num_seconds() >= 0);
    }

    #[tokio::test]
    async fn test_schedule_trigger_status() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = ScheduleTrigger::with_defaults(cache).unwrap();

        let status = trigger.status().await;

        assert!(!status.is_running);
        assert_eq!(status.config.rotation_time, "23:00");
    }

    #[tokio::test]
    async fn test_force_rotation() {
        let cache = Arc::new(ScheduleCache::new());
        let trigger = ScheduleTrigger::with_defaults(cache.clone()).unwrap();

        let schedule = trigger.force_rotation().await.unwrap();
        let tomorrow = Local::now().date_naive() + Duration::days(1);

        assert_eq!(schedule.date, tomorrow);

        // Cache should be updated
        let cached = cache.get().await.unwrap();
        assert_eq!(cached.date, tomorrow);
    }
}
