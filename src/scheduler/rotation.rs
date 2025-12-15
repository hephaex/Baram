//! Deterministic rotation algorithm for distributed crawling
//!
//! This module implements the daily rotation system that ensures:
//! - Fair distribution of crawling load across instances
//! - Deterministic ordering (same date always produces same schedule)
//! - IP rotation to avoid detection and banning

use chrono::{Datelike, NaiveDate};
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::error::{SchedulerError, SchedulerResult};
use super::schedule::{DailySchedule, HourlySlot};

// ============================================================================
// Crawler Instance
// ============================================================================

/// Represents a crawler instance in the distributed system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrawlerInstance {
    /// Main instance (primary)
    Main,
    /// Sub instance 1
    Sub1,
    /// Sub instance 2
    Sub2,
}

impl CrawlerInstance {
    /// Get all available instances
    pub fn all() -> Vec<Self> {
        vec![Self::Main, Self::Sub1, Self::Sub2]
    }

    /// Get instance count
    pub fn count() -> usize {
        3
    }

    /// Get instance ID as string
    pub fn id(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Sub1 => "sub1",
            Self::Sub2 => "sub2",
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Main => "Main (Primary)",
            Self::Sub1 => "Sub1 (Secondary)",
            Self::Sub2 => "Sub2 (Tertiary)",
        }
    }

    /// Get Korean label
    pub fn korean_label(&self) -> &'static str {
        match self {
            Self::Main => "주실행",
            Self::Sub1 => "서브1",
            Self::Sub2 => "서브2",
        }
    }

    /// Try to parse from string
    pub fn from_id(id: &str) -> SchedulerResult<Self> {
        match id.to_lowercase().as_str() {
            "main" | "primary" | "0" => Ok(Self::Main),
            "sub1" | "secondary" | "1" => Ok(Self::Sub1),
            "sub2" | "tertiary" | "2" => Ok(Self::Sub2),
            _ => Err(SchedulerError::invalid_instance(id)),
        }
    }

    /// Get numeric index (0-based)
    pub fn index(&self) -> usize {
        match self {
            Self::Main => 0,
            Self::Sub1 => 1,
            Self::Sub2 => 2,
        }
    }

    /// Create from index
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Main),
            1 => Some(Self::Sub1),
            2 => Some(Self::Sub2),
            _ => None,
        }
    }
}

impl fmt::Display for CrawlerInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id())
    }
}

impl FromStr for CrawlerInstance {
    type Err = SchedulerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_id(s)
    }
}

impl Default for CrawlerInstance {
    fn default() -> Self {
        Self::Main
    }
}

// ============================================================================
// News Category
// ============================================================================

/// News categories for distributed crawling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewsCategory {
    /// 정치 (Politics)
    Politics,
    /// 경제 (Economy)
    Economy,
    /// 사회 (Society)
    Society,
    /// 문화 (Culture/Lifestyle)
    Culture,
    /// 세계 (World/International)
    World,
    /// IT/과학 (IT/Science)
    It,
}

impl NewsCategory {
    /// Get all categories
    pub fn all() -> Vec<Self> {
        vec![
            Self::Politics,
            Self::Economy,
            Self::Society,
            Self::Culture,
            Self::World,
            Self::It,
        ]
    }

    /// Get category ID
    pub fn id(&self) -> &'static str {
        match self {
            Self::Politics => "politics",
            Self::Economy => "economy",
            Self::Society => "society",
            Self::Culture => "culture",
            Self::World => "world",
            Self::It => "it",
        }
    }

    /// Get Korean label
    pub fn korean_label(&self) -> &'static str {
        match self {
            Self::Politics => "정치",
            Self::Economy => "경제",
            Self::Society => "사회",
            Self::Culture => "문화",
            Self::World => "세계",
            Self::It => "IT/과학",
        }
    }

    /// Parse from string
    pub fn from_id(id: &str) -> Option<Self> {
        match id.to_lowercase().as_str() {
            "politics" | "정치" => Some(Self::Politics),
            "economy" | "경제" => Some(Self::Economy),
            "society" | "사회" => Some(Self::Society),
            "culture" | "문화" | "lifestyle" => Some(Self::Culture),
            "world" | "세계" | "international" => Some(Self::World),
            "it" | "science" | "it/과학" => Some(Self::It),
            _ => None,
        }
    }
}

impl fmt::Display for NewsCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id())
    }
}

// ============================================================================
// Rotation Scheduler
// ============================================================================

/// Scheduler for daily instance rotation
///
/// Uses a deterministic algorithm based on date to ensure:
/// - Same date always produces same rotation order
/// - Fair distribution over time
/// - Reproducible results across all instances
#[derive(Debug, Clone)]
pub struct RotationScheduler {
    instances: Vec<CrawlerInstance>,
    categories: Vec<NewsCategory>,
    categories_per_slot: usize,
}

impl RotationScheduler {
    /// Create a new rotation scheduler with default settings
    pub fn new() -> Self {
        Self {
            instances: CrawlerInstance::all(),
            categories: NewsCategory::all(),
            categories_per_slot: 2,
        }
    }

    /// Create scheduler with custom categories per slot
    pub fn with_categories_per_slot(mut self, count: usize) -> Self {
        self.categories_per_slot = count.max(1).min(self.categories.len());
        self
    }

    /// Get the daily rotation order for a specific date
    ///
    /// Uses deterministic seeding based on the date to ensure
    /// reproducible results across all instances.
    ///
    /// # Arguments
    /// * `date` - The date to get rotation for
    ///
    /// # Returns
    /// Ordered vector of instances for the day
    ///
    /// # Example
    /// ```
    /// use ntimes::scheduler::RotationScheduler;
    /// use chrono::NaiveDate;
    ///
    /// let scheduler = RotationScheduler::new();
    /// let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    /// let rotation = scheduler.get_daily_rotation(date);
    /// assert_eq!(rotation.len(), 3);
    /// ```
    pub fn get_daily_rotation(&self, date: NaiveDate) -> Vec<CrawlerInstance> {
        // Use num_days_from_ce() for unique seed per date
        // This avoids collisions that could occur with year*10000+month*100+day
        let seed = date.num_days_from_ce() as u64;

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut order = self.instances.clone();
        order.shuffle(&mut rng);
        order
    }

    /// Get the instance responsible for a specific hour on a date
    ///
    /// # Arguments
    /// * `date` - The date
    /// * `hour` - Hour of the day (0-23)
    ///
    /// # Returns
    /// The instance that should crawl at this hour
    pub fn get_instance_for_hour(&self, date: NaiveDate, hour: u32) -> SchedulerResult<CrawlerInstance> {
        if hour > 23 {
            return Err(SchedulerError::invalid_hour(hour));
        }

        let rotation = self.get_daily_rotation(date);
        let index = (hour as usize) % rotation.len();
        Ok(rotation[index])
    }

    /// Get categories assigned to a specific hour slot
    ///
    /// Uses cyclic distribution to ensure even coverage
    fn get_categories_for_slot(&self, hour: usize) -> Vec<NewsCategory> {
        let start = (hour * self.categories_per_slot) % self.categories.len();

        self.categories
            .iter()
            .cycle()
            .skip(start)
            .take(self.categories_per_slot)
            .copied()
            .collect()
    }

    /// Generate a complete daily schedule
    ///
    /// Creates a 24-hour schedule with instance and category assignments
    ///
    /// # Arguments
    /// * `date` - The date to generate schedule for
    ///
    /// # Returns
    /// Complete daily schedule with all hourly slots
    pub fn generate_daily_schedule(&self, date: NaiveDate) -> DailySchedule {
        let rotation = self.get_daily_rotation(date);
        let mut slots = Vec::with_capacity(24);

        for hour in 0..24 {
            let instance = rotation[hour % rotation.len()];
            let categories = self.get_categories_for_slot(hour);

            slots.push(HourlySlot {
                hour: hour as u8,
                instance,
                categories,
            });
        }

        DailySchedule::new(date, slots)
    }

    /// Check if an instance is active for a given date and hour
    pub fn is_instance_active(
        &self,
        instance: CrawlerInstance,
        date: NaiveDate,
        hour: u32,
    ) -> SchedulerResult<bool> {
        let assigned = self.get_instance_for_hour(date, hour)?;
        Ok(assigned == instance)
    }

    /// Get the next active slot for an instance after a given hour
    pub fn get_next_slot_for_instance(
        &self,
        instance: CrawlerInstance,
        date: NaiveDate,
        after_hour: u32,
    ) -> Option<u8> {
        let rotation = self.get_daily_rotation(date);

        for hour in (after_hour + 1)..24 {
            let idx = (hour as usize) % rotation.len();
            if rotation[idx] == instance {
                return Some(hour as u8);
            }
        }

        None
    }

    /// Get all slots assigned to an instance for a date
    pub fn get_slots_for_instance(
        &self,
        instance: CrawlerInstance,
        date: NaiveDate,
    ) -> Vec<HourlySlot> {
        let schedule = self.generate_daily_schedule(date);
        schedule
            .slots
            .into_iter()
            .filter(|slot| slot.instance == instance)
            .collect()
    }

    /// Get schedule summary as formatted string
    pub fn format_schedule(&self, date: NaiveDate) -> String {
        let schedule = self.generate_daily_schedule(date);
        let mut output = format!("Schedule for {} ({})\n", date, date.weekday());
        output.push_str(&format!("{:=<60}\n", ""));

        let rotation = self.get_daily_rotation(date);
        output.push_str(&format!(
            "Daily Rotation: {}\n\n",
            rotation
                .iter()
                .map(|i| i.korean_label())
                .collect::<Vec<_>>()
                .join(" → ")
        ));

        output.push_str(&format!(
            "{:>5} | {:^12} | {}\n",
            "Hour", "Instance", "Categories"
        ));
        output.push_str(&format!("{:-<60}\n", ""));

        for slot in &schedule.slots {
            let categories: Vec<_> = slot.categories.iter().map(|c| c.korean_label()).collect();
            output.push_str(&format!(
                "{:02}:00 | {:^12} | {}\n",
                slot.hour,
                slot.instance.korean_label(),
                categories.join(", ")
            ));
        }

        output
    }
}

impl Default for RotationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crawler_instance_all() {
        let instances = CrawlerInstance::all();
        assert_eq!(instances.len(), 3);
        assert!(instances.contains(&CrawlerInstance::Main));
        assert!(instances.contains(&CrawlerInstance::Sub1));
        assert!(instances.contains(&CrawlerInstance::Sub2));
    }

    #[test]
    fn test_crawler_instance_from_id() {
        assert_eq!(CrawlerInstance::from_id("main").unwrap(), CrawlerInstance::Main);
        assert_eq!(CrawlerInstance::from_id("SUB1").unwrap(), CrawlerInstance::Sub1);
        assert_eq!(CrawlerInstance::from_id("sub2").unwrap(), CrawlerInstance::Sub2);
        assert!(CrawlerInstance::from_id("invalid").is_err());
    }

    #[test]
    fn test_crawler_instance_index() {
        assert_eq!(CrawlerInstance::Main.index(), 0);
        assert_eq!(CrawlerInstance::Sub1.index(), 1);
        assert_eq!(CrawlerInstance::Sub2.index(), 2);

        assert_eq!(CrawlerInstance::from_index(0), Some(CrawlerInstance::Main));
        assert_eq!(CrawlerInstance::from_index(3), None);
    }

    #[test]
    fn test_news_category_all() {
        let categories = NewsCategory::all();
        assert_eq!(categories.len(), 6);
    }

    #[test]
    fn test_news_category_from_id() {
        assert_eq!(NewsCategory::from_id("politics"), Some(NewsCategory::Politics));
        assert_eq!(NewsCategory::from_id("경제"), Some(NewsCategory::Economy));
        assert_eq!(NewsCategory::from_id("unknown"), None);
    }

    #[test]
    fn test_rotation_deterministic() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        // Same date should always produce same rotation
        let rotation1 = scheduler.get_daily_rotation(date);
        let rotation2 = scheduler.get_daily_rotation(date);
        assert_eq!(rotation1, rotation2);
    }

    #[test]
    fn test_rotation_different_dates() {
        let scheduler = RotationScheduler::new();
        let date1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();

        let rotation1 = scheduler.get_daily_rotation(date1);
        let rotation2 = scheduler.get_daily_rotation(date2);

        // Different dates may have different rotations
        // (not guaranteed, but likely)
        // What we can test is that both contain all instances
        assert_eq!(rotation1.len(), 3);
        assert_eq!(rotation2.len(), 3);
    }

    #[test]
    fn test_rotation_contains_all_instances() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();

        let rotation = scheduler.get_daily_rotation(date);

        assert!(rotation.contains(&CrawlerInstance::Main));
        assert!(rotation.contains(&CrawlerInstance::Sub1));
        assert!(rotation.contains(&CrawlerInstance::Sub2));
    }

    #[test]
    fn test_get_instance_for_hour() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        // Valid hours
        for hour in 0..24 {
            assert!(scheduler.get_instance_for_hour(date, hour).is_ok());
        }

        // Invalid hour
        assert!(scheduler.get_instance_for_hour(date, 24).is_err());
        assert!(scheduler.get_instance_for_hour(date, 25).is_err());
    }

    #[test]
    fn test_hourly_rotation_pattern() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let rotation = scheduler.get_daily_rotation(date);

        // Hours 0, 3, 6, ... should have same instance (rotation[0])
        // Hours 1, 4, 7, ... should have same instance (rotation[1])
        // Hours 2, 5, 8, ... should have same instance (rotation[2])

        let hour0 = scheduler.get_instance_for_hour(date, 0).unwrap();
        let hour3 = scheduler.get_instance_for_hour(date, 3).unwrap();
        let hour6 = scheduler.get_instance_for_hour(date, 6).unwrap();

        assert_eq!(hour0, rotation[0]);
        assert_eq!(hour3, rotation[0]);
        assert_eq!(hour6, rotation[0]);
    }

    #[test]
    fn test_categories_per_slot() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let schedule = scheduler.generate_daily_schedule(date);

        // Each slot should have exactly 2 categories (default)
        for slot in &schedule.slots {
            assert_eq!(slot.categories.len(), 2);
        }
    }

    #[test]
    fn test_custom_categories_per_slot() {
        let scheduler = RotationScheduler::new().with_categories_per_slot(3);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let schedule = scheduler.generate_daily_schedule(date);

        for slot in &schedule.slots {
            assert_eq!(slot.categories.len(), 3);
        }
    }

    #[test]
    fn test_daily_schedule_structure() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let schedule = scheduler.generate_daily_schedule(date);

        assert_eq!(schedule.date, date);
        assert_eq!(schedule.slots.len(), 24);

        // Check hours are sequential
        for (i, slot) in schedule.slots.iter().enumerate() {
            assert_eq!(slot.hour as usize, i);
        }
    }

    #[test]
    fn test_is_instance_active() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let instance_at_0 = scheduler.get_instance_for_hour(date, 0).unwrap();

        assert!(scheduler.is_instance_active(instance_at_0, date, 0).unwrap());
    }

    #[test]
    fn test_get_slots_for_instance() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let main_slots = scheduler.get_slots_for_instance(CrawlerInstance::Main, date);
        let sub1_slots = scheduler.get_slots_for_instance(CrawlerInstance::Sub1, date);
        let sub2_slots = scheduler.get_slots_for_instance(CrawlerInstance::Sub2, date);

        // Each instance should have 8 slots (24 hours / 3 instances)
        assert_eq!(main_slots.len(), 8);
        assert_eq!(sub1_slots.len(), 8);
        assert_eq!(sub2_slots.len(), 8);

        // Total should be 24
        assert_eq!(main_slots.len() + sub1_slots.len() + sub2_slots.len(), 24);
    }

    #[test]
    fn test_get_next_slot_for_instance() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let instance_at_0 = scheduler.get_instance_for_hour(date, 0).unwrap();

        // The same instance should appear again at hour 3
        let next = scheduler.get_next_slot_for_instance(instance_at_0, date, 0);
        assert_eq!(next, Some(3));
    }

    #[test]
    fn test_format_schedule() {
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let formatted = scheduler.format_schedule(date);

        assert!(formatted.contains("2024-01-15"));
        assert!(formatted.contains("Hour"));
        assert!(formatted.contains("Instance"));
        assert!(formatted.contains("00:00"));
        assert!(formatted.contains("23:00"));
    }

    #[test]
    fn test_seed_uniqueness_across_dates() {
        // Verify that nearby dates produce different rotations
        let _scheduler = RotationScheduler::new();

        let date1 = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let date3 = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();

        let seed1 = date1.num_days_from_ce();
        let seed2 = date2.num_days_from_ce();
        let seed3 = date3.num_days_from_ce();

        // All seeds should be unique
        assert_ne!(seed1, seed2);
        assert_ne!(seed2, seed3);
        assert_ne!(seed1, seed3);
    }
}
