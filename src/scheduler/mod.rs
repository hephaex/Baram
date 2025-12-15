//! Distributed crawler scheduling system
//!
//! This module provides scheduling infrastructure for distributed crawling
//! to prevent IP bans through rotation and load distribution.
//!
//! # Features
//!
//! - **Deterministic Rotation**: Daily instance rotation using seeded RNG
//! - **Hourly Slot Assignment**: Time-based crawling slot distribution
//! - **Category Distribution**: Load balancing across news categories
//! - **Schedule Caching**: Fault-tolerant schedule persistence
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │         Coordinator Server          │
//! │  (23:00 KST rotation & scheduling)  │
//! └─────────────┬───────────────────────┘
//!               │
//!     ┌─────────┼─────────┐
//!     │         │         │
//!     ▼         ▼         ▼
//!   Main      Sub1      Sub2
//! (IP: A)   (IP: B)   (IP: C)
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use ntimes::scheduler::{RotationScheduler, CrawlerInstance};
//! use chrono::Local;
//!
//! let scheduler = RotationScheduler::new();
//! let today = Local::now().date_naive();
//!
//! // Get today's rotation order
//! let rotation = scheduler.get_daily_rotation(today);
//! println!("Today's order: {:?}", rotation);
//!
//! // Get which instance should crawl at a specific hour
//! let instance = scheduler.get_instance_for_hour(today, 14);
//! println!("14:00 instance: {:?}", instance);
//!
//! // Generate full daily schedule
//! let schedule = scheduler.generate_daily_schedule(today);
//! for slot in &schedule.slots {
//!     println!("{}:00 - {:?}: {:?}", slot.hour, slot.instance, slot.categories);
//! }
//! ```

pub mod assignment;
pub mod distribution;
pub mod error;
pub mod rotation;
pub mod schedule;
pub mod trigger;

// Re-export main types
pub use assignment::{AssignmentStrategy, CategoryAssigner, CategoryConfig, CategoryPriority};
pub use distribution::{
    create_default_distributor, create_distributor, DistributionConfig, DistributionEvent,
    DistributorHandle, ScheduleDistributor, ScheduleReceiver, UpdateReason,
};
pub use error::{SchedulerError, SchedulerResult};
pub use rotation::{CrawlerInstance, NewsCategory, RotationScheduler};
pub use schedule::{DailySchedule, HourlySlot, ScheduleCache, ScheduleMetadata};
pub use trigger::{ScheduleTrigger, TriggerConfig};
