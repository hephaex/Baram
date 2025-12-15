//! Schedule data structures and caching
//!
//! This module provides structures for representing daily crawling schedules
//! and a caching mechanism for fault-tolerant operation.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::sync::RwLock;

use super::error::{SchedulerError, SchedulerResult};
use super::rotation::{CrawlerInstance, NewsCategory};

// ============================================================================
// Hourly Slot
// ============================================================================

/// A single hourly crawling slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlySlot {
    /// Hour of the day (0-23)
    pub hour: u8,

    /// Instance responsible for this slot
    pub instance: CrawlerInstance,

    /// Categories to crawl during this slot
    pub categories: Vec<NewsCategory>,
}

impl HourlySlot {
    /// Create a new hourly slot
    pub fn new(hour: u8, instance: CrawlerInstance, categories: Vec<NewsCategory>) -> Self {
        Self {
            hour,
            instance,
            categories,
        }
    }

    /// Check if this slot is for a specific instance
    pub fn is_for_instance(&self, instance: CrawlerInstance) -> bool {
        self.instance == instance
    }

    /// Get categories as string IDs
    pub fn category_ids(&self) -> Vec<&str> {
        self.categories.iter().map(|c| c.id()).collect()
    }

    /// Format as display string
    pub fn display(&self) -> String {
        let cats: Vec<_> = self.categories.iter().map(|c| c.korean_label()).collect();
        format!(
            "{:02}:00 - {} ({})",
            self.hour,
            self.instance.korean_label(),
            cats.join(", ")
        )
    }
}

// ============================================================================
// Daily Schedule
// ============================================================================

/// Complete schedule for a single day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySchedule {
    /// Date this schedule is for
    pub date: NaiveDate,

    /// All 24 hourly slots
    pub slots: Vec<HourlySlot>,

    /// When this schedule was generated
    #[serde(default = "Utc::now")]
    pub generated_at: DateTime<Utc>,

    /// Optional metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ScheduleMetadata>,
}

/// Optional schedule metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScheduleMetadata {
    /// Schedule version
    pub version: u32,

    /// Generator instance ID
    pub generated_by: Option<String>,

    /// Any notes
    pub notes: Option<String>,
}

impl DailySchedule {
    /// Create a new daily schedule
    pub fn new(date: NaiveDate, slots: Vec<HourlySlot>) -> Self {
        Self {
            date,
            slots,
            generated_at: Utc::now(),
            metadata: None,
        }
    }

    /// Create with metadata
    pub fn with_metadata(mut self, metadata: ScheduleMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get slot for a specific hour
    pub fn get_slot(&self, hour: u8) -> Option<&HourlySlot> {
        self.slots.iter().find(|s| s.hour == hour)
    }

    /// Get all slots for a specific instance
    pub fn slots_for_instance(&self, instance: CrawlerInstance) -> Vec<&HourlySlot> {
        self.slots
            .iter()
            .filter(|s| s.instance == instance)
            .collect()
    }

    /// Get the instance for a specific hour
    pub fn instance_at_hour(&self, hour: u8) -> Option<CrawlerInstance> {
        self.get_slot(hour).map(|s| s.instance)
    }

    /// Get categories for a specific hour
    pub fn categories_at_hour(&self, hour: u8) -> Vec<NewsCategory> {
        self.get_slot(hour)
            .map(|s| s.categories.clone())
            .unwrap_or_default()
    }

    /// Check if schedule is valid (has 24 slots)
    pub fn is_valid(&self) -> bool {
        self.slots.len() == 24 && self.slots.iter().enumerate().all(|(i, s)| s.hour as usize == i)
    }

    /// Get summary statistics
    pub fn summary(&self) -> ScheduleSummary {
        let mut instance_counts = std::collections::HashMap::new();
        let mut category_counts = std::collections::HashMap::new();

        for slot in &self.slots {
            *instance_counts.entry(slot.instance).or_insert(0) += 1;
            for cat in &slot.categories {
                *category_counts.entry(*cat).or_insert(0) += 1;
            }
        }

        ScheduleSummary {
            date: self.date,
            total_slots: self.slots.len(),
            instance_distribution: instance_counts,
            category_distribution: category_counts,
        }
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> SchedulerResult<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> SchedulerResult<Self> {
        serde_json::from_str(json).map_err(Into::into)
    }

    /// Save to file
    pub async fn save_to_file(&self, path: impl AsRef<Path>) -> SchedulerResult<()> {
        let json = self.to_json()?;
        tokio::fs::write(path.as_ref(), json)
            .await
            .map_err(|e| SchedulerError::io_error("save_schedule", e.to_string()))?;
        Ok(())
    }

    /// Load from file
    pub async fn load_from_file(path: impl AsRef<Path>) -> SchedulerResult<Self> {
        let json = tokio::fs::read_to_string(path.as_ref())
            .await
            .map_err(|e| SchedulerError::io_error("load_schedule", e.to_string()))?;
        Self::from_json(&json)
    }
}

/// Schedule summary statistics
#[derive(Debug, Clone)]
pub struct ScheduleSummary {
    pub date: NaiveDate,
    pub total_slots: usize,
    pub instance_distribution: std::collections::HashMap<CrawlerInstance, usize>,
    pub category_distribution: std::collections::HashMap<NewsCategory, usize>,
}

impl ScheduleSummary {
    /// Format as display string
    pub fn display(&self) -> String {
        let mut output = format!("Schedule Summary for {}\n", self.date);
        output.push_str(&format!("{:-<40}\n", ""));
        output.push_str(&format!("Total Slots: {}\n\n", self.total_slots));

        output.push_str("Instance Distribution:\n");
        for (instance, count) in &self.instance_distribution {
            output.push_str(&format!("  {}: {} slots\n", instance.korean_label(), count));
        }

        output.push_str("\nCategory Distribution:\n");
        for (category, count) in &self.category_distribution {
            output.push_str(&format!("  {}: {} slots\n", category.korean_label(), count));
        }

        output
    }
}

// ============================================================================
// Schedule Cache
// ============================================================================

/// Thread-safe cache for schedules
///
/// Provides fault-tolerant schedule storage with automatic expiration.
/// When the coordinator is unavailable, instances can use cached schedules.
pub struct ScheduleCache {
    /// Cached schedule
    cached_schedule: RwLock<Option<DailySchedule>>,

    /// When the cache was last updated
    last_update: RwLock<DateTime<Utc>>,

    /// Cache file path (optional)
    cache_file: Option<std::path::PathBuf>,

    /// Cache validity duration in hours
    validity_hours: u32,
}

impl ScheduleCache {
    /// Create a new in-memory cache
    pub fn new() -> Self {
        Self {
            cached_schedule: RwLock::new(None),
            last_update: RwLock::new(DateTime::from_timestamp(0, 0).unwrap()),
            cache_file: None,
            validity_hours: 24,
        }
    }

    /// Create cache with file persistence
    pub fn with_file(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            cached_schedule: RwLock::new(None),
            last_update: RwLock::new(DateTime::from_timestamp(0, 0).unwrap()),
            cache_file: Some(path.into()),
            validity_hours: 24,
        }
    }

    /// Set cache validity duration
    pub fn with_validity_hours(mut self, hours: u32) -> Self {
        self.validity_hours = hours;
        self
    }

    /// Update the cached schedule
    pub async fn update(&self, schedule: DailySchedule) -> SchedulerResult<()> {
        // Update in-memory cache
        *self.cached_schedule.write().await = Some(schedule.clone());
        *self.last_update.write().await = Utc::now();

        // Persist to file if configured
        if let Some(ref path) = self.cache_file {
            schedule.save_to_file(path).await?;
        }

        Ok(())
    }

    /// Get the cached schedule
    pub async fn get(&self) -> Option<DailySchedule> {
        self.cached_schedule.read().await.clone()
    }

    /// Get the cached schedule for a specific date
    pub async fn get_for_date(&self, date: NaiveDate) -> Option<DailySchedule> {
        let schedule = self.cached_schedule.read().await;
        schedule.as_ref().filter(|s| s.date == date).cloned()
    }

    /// Check if cache is valid (not expired)
    pub async fn is_valid(&self) -> bool {
        let last = *self.last_update.read().await;
        let age = Utc::now() - last;

        age.num_hours() < self.validity_hours as i64
    }

    /// Check if cache has a schedule for today
    pub async fn has_today(&self) -> bool {
        let today = chrono::Local::now().date_naive();
        self.get_for_date(today).await.is_some()
    }

    /// Get cache age in seconds
    pub async fn age_seconds(&self) -> i64 {
        let last = *self.last_update.read().await;
        (Utc::now() - last).num_seconds()
    }

    /// Clear the cache
    pub async fn clear(&self) {
        *self.cached_schedule.write().await = None;
        *self.last_update.write().await = DateTime::from_timestamp(0, 0).unwrap();
    }

    /// Load from file if available
    pub async fn load_from_file(&self) -> SchedulerResult<bool> {
        if let Some(ref path) = self.cache_file {
            if path.exists() {
                let schedule = DailySchedule::load_from_file(path).await?;
                *self.cached_schedule.write().await = Some(schedule);
                *self.last_update.write().await = Utc::now();
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Get cache status
    pub async fn status(&self) -> CacheStatus {
        let has_schedule = self.cached_schedule.read().await.is_some();
        let is_valid = self.is_valid().await;
        let age_secs = self.age_seconds().await;

        CacheStatus {
            has_schedule,
            is_valid,
            age_seconds: age_secs,
            file_path: self.cache_file.clone(),
        }
    }
}

impl Default for ScheduleCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache status information
#[derive(Debug, Clone)]
pub struct CacheStatus {
    pub has_schedule: bool,
    pub is_valid: bool,
    pub age_seconds: i64,
    pub file_path: Option<std::path::PathBuf>,
}

impl CacheStatus {
    /// Format as display string
    pub fn display(&self) -> String {
        let mut output = String::from("Cache Status\n");
        output.push_str(&format!("{:-<30}\n", ""));
        output.push_str(&format!("Has Schedule: {}\n", self.has_schedule));
        output.push_str(&format!("Is Valid: {}\n", self.is_valid));
        output.push_str(&format!("Age: {}s\n", self.age_seconds));
        if let Some(ref path) = self.file_path {
            output.push_str(&format!("File: {}\n", path.display()));
        }
        output
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_schedule() -> DailySchedule {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut slots = Vec::new();

        for hour in 0..24 {
            let instance = match hour % 3 {
                0 => CrawlerInstance::Main,
                1 => CrawlerInstance::Sub1,
                _ => CrawlerInstance::Sub2,
            };
            slots.push(HourlySlot {
                hour: hour as u8,
                instance,
                categories: vec![NewsCategory::Politics, NewsCategory::Economy],
            });
        }

        DailySchedule::new(date, slots)
    }

    #[test]
    fn test_hourly_slot_creation() {
        let slot = HourlySlot::new(
            14,
            CrawlerInstance::Main,
            vec![NewsCategory::Politics, NewsCategory::Economy],
        );

        assert_eq!(slot.hour, 14);
        assert_eq!(slot.instance, CrawlerInstance::Main);
        assert_eq!(slot.categories.len(), 2);
    }

    #[test]
    fn test_hourly_slot_display() {
        let slot = HourlySlot::new(14, CrawlerInstance::Main, vec![NewsCategory::Politics]);

        let display = slot.display();
        assert!(display.contains("14:00"));
        assert!(display.contains("주실행"));
        assert!(display.contains("정치"));
    }

    #[test]
    fn test_daily_schedule_is_valid() {
        let schedule = create_test_schedule();
        assert!(schedule.is_valid());

        // Invalid schedule (missing slots)
        let invalid = DailySchedule::new(
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            vec![],
        );
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_daily_schedule_get_slot() {
        let schedule = create_test_schedule();

        let slot = schedule.get_slot(14);
        assert!(slot.is_some());
        assert_eq!(slot.unwrap().hour, 14);

        let none = schedule.get_slot(24);
        assert!(none.is_none());
    }

    #[test]
    fn test_daily_schedule_slots_for_instance() {
        let schedule = create_test_schedule();

        let main_slots = schedule.slots_for_instance(CrawlerInstance::Main);
        assert_eq!(main_slots.len(), 8); // 0, 3, 6, 9, 12, 15, 18, 21
    }

    #[test]
    fn test_daily_schedule_summary() {
        let schedule = create_test_schedule();
        let summary = schedule.summary();

        assert_eq!(summary.total_slots, 24);
        assert_eq!(summary.instance_distribution.len(), 3);

        // Each instance should have 8 slots
        for (_, count) in &summary.instance_distribution {
            assert_eq!(*count, 8);
        }
    }

    #[test]
    fn test_daily_schedule_json_roundtrip() {
        let schedule = create_test_schedule();

        let json = schedule.to_json().unwrap();
        let parsed = DailySchedule::from_json(&json).unwrap();

        assert_eq!(parsed.date, schedule.date);
        assert_eq!(parsed.slots.len(), schedule.slots.len());
    }

    #[tokio::test]
    async fn test_schedule_cache_basic() {
        let cache = ScheduleCache::new();

        // Initially empty
        assert!(cache.get().await.is_none());
        assert!(!cache.is_valid().await);

        // Update cache
        let schedule = create_test_schedule();
        cache.update(schedule.clone()).await.unwrap();

        // Now has content
        assert!(cache.get().await.is_some());
        assert!(cache.is_valid().await);
    }

    #[tokio::test]
    async fn test_schedule_cache_get_for_date() {
        let cache = ScheduleCache::new();
        let schedule = create_test_schedule();
        cache.update(schedule).await.unwrap();

        // Correct date
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        assert!(cache.get_for_date(date).await.is_some());

        // Wrong date
        let other_date = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();
        assert!(cache.get_for_date(other_date).await.is_none());
    }

    #[tokio::test]
    async fn test_schedule_cache_clear() {
        let cache = ScheduleCache::new();
        let schedule = create_test_schedule();

        cache.update(schedule).await.unwrap();
        assert!(cache.get().await.is_some());

        cache.clear().await;
        assert!(cache.get().await.is_none());
    }

    #[tokio::test]
    async fn test_schedule_cache_status() {
        let cache = ScheduleCache::new();
        let schedule = create_test_schedule();

        let status_before = cache.status().await;
        assert!(!status_before.has_schedule);
        assert!(!status_before.is_valid);

        cache.update(schedule).await.unwrap();

        let status_after = cache.status().await;
        assert!(status_after.has_schedule);
        assert!(status_after.is_valid);
        assert!(status_after.age_seconds < 5);
    }

    #[tokio::test]
    async fn test_schedule_cache_with_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_path = temp_dir.path().join("schedule_cache.json");

        let cache = ScheduleCache::with_file(&cache_path);
        let schedule = create_test_schedule();

        cache.update(schedule).await.unwrap();

        // File should exist
        assert!(cache_path.exists());

        // Load in new cache instance
        let cache2 = ScheduleCache::with_file(&cache_path);
        let loaded = cache2.load_from_file().await.unwrap();
        assert!(loaded);
        assert!(cache2.get().await.is_some());
    }
}
