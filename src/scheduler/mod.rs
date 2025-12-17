//! Distributed crawler scheduling system
//!
//! This module provides a comprehensive scheduling infrastructure for distributed
//! crawling with anti-ban protection through IP rotation and load distribution.
//!
//! # Overview
//!
//! The scheduler system enables multiple crawler instances to work together,
//! distributing work across different IPs to avoid detection and rate limiting.
//! The system uses deterministic algorithms to ensure consistent behavior
//! across all instances without requiring constant coordination.
//!
//! # Features
//!
//! - **Deterministic Rotation**: Daily instance rotation using ChaCha8 seeded RNG
//! - **Hourly Slot Assignment**: Time-based crawling slot distribution (3 instances = 8 hours each)
//! - **Category Distribution**: Load balancing across 6 news categories
//! - **Schedule Caching**: Fault-tolerant schedule persistence with file backup
//! - **Automatic Failover**: Health monitoring and automatic work redistribution
//! - **Manual Override**: Operator controls for emergency situations
//! - **Event Broadcasting**: Real-time schedule updates via tokio channels
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────┐
//! │                     Coordinator Server                        │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │
//! │  │  Rotation   │  │   Failover  │  │     Distribution    │   │
//! │  │  Scheduler  │  │   Manager   │  │     Broadcaster     │   │
//! │  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘   │
//! │         │                │                     │              │
//! │         └────────────────┼─────────────────────┘              │
//! │                          │                                    │
//! │                   ┌──────▼──────┐                             │
//! │                   │  Schedule   │                             │
//! │                   │   Cache     │                             │
//! │                   └──────┬──────┘                             │
//! └──────────────────────────┼────────────────────────────────────┘
//!                            │
//!              ┌─────────────┼─────────────┐
//!              │             │             │
//!              ▼             ▼             ▼
//!         ┌────────┐   ┌────────┐   ┌────────┐
//!         │  Main  │   │  Sub1  │   │  Sub2  │
//!         │ (IP:A) │   │ (IP:B) │   │ (IP:C) │
//!         └────────┘   └────────┘   └────────┘
//! ```
//!
//! # Modules
//!
//! - [`rotation`] - Core rotation algorithm and instance/category definitions
//! - [`schedule`] - Schedule data structures and caching
//! - [`trigger`] - Time-based schedule triggers (23:00 KST daily rotation)
//! - [`distribution`] - Schedule broadcasting to instances
//! - [`assignment`] - Category-to-instance assignment strategies
//! - [`failover`] - Health monitoring and automatic failover
//!
//! # Quick Start
//!
//! ## Basic Schedule Generation
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
//! // Check which instance should crawl at 14:00
//! let instance = scheduler.get_instance_for_hour(today, 14)?;
//! println!("14:00 instance: {:?}", instance);
//!
//! // Generate full daily schedule
//! let schedule = scheduler.generate_daily_schedule(today);
//! for slot in &schedule.slots {
//!     println!("{:02}:00 - {} - {:?}", slot.hour, slot.instance, slot.categories);
//! }
//! ```
//!
//! ## Schedule Distribution
//!
//! ```ignore
//! use ntimes::scheduler::{ScheduleDistributor, DistributionConfig};
//!
//! let distributor = ScheduleDistributor::new(DistributionConfig::default());
//!
//! // Subscribe to schedule events
//! let mut receiver = distributor.subscribe();
//!
//! // Distribute schedule to all instances
//! let result = distributor.distribute(schedule).await;
//! assert!(result.success);
//!
//! // Wait for acknowledgments
//! while !distributor.all_acknowledged().await {
//!     tokio::time::sleep(Duration::from_secs(1)).await;
//! }
//! ```
//!
//! ## Failover Handling
//!
//! ```ignore
//! use ntimes::scheduler::{FailoverManager, FailoverConfig};
//!
//! let manager = FailoverManager::new(FailoverConfig::default());
//!
//! // Process heartbeats from instances
//! manager.process_heartbeat(CrawlerInstance::Main).await;
//!
//! // Check for stale instances
//! let stale = manager.check_stale_instances().await;
//! for instance in stale {
//!     println!("Instance {} is stale, failover initiated", instance);
//! }
//! ```
//!
//! ## Category Assignment
//!
//! ```ignore
//! use ntimes::scheduler::{CategoryAssigner, CategoryPriority, AssignmentStrategy};
//!
//! let mut assigner = CategoryAssigner::new()
//!     .with_strategy(AssignmentStrategy::Weighted)
//!     .with_categories_per_slot(3);
//!
//! // Set high priority for Politics
//! assigner.set_priority(NewsCategory::Politics, CategoryPriority::Critical);
//!
//! // Generate schedule with custom assignment
//! let schedule = assigner.generate_schedule(today, &rotation);
//! ```
//!
//! # Configuration
//!
//! ## Rotation Settings
//!
//! The rotation algorithm uses ChaCha8Rng with the date as seed, ensuring:
//! - Same date always produces same rotation order
//! - Different dates produce statistically varied rotations
//! - Reproducible results across all instances
//!
//! ## Failover Settings
//!
//! | Setting | Default | Description |
//! |---------|---------|-------------|
//! | `max_failures` | 3 | Consecutive failures before failover |
//! | `heartbeat_timeout_secs` | 60 | Seconds before instance considered stale |
//! | `failover_cooldown_secs` | 300 | Minimum time between failovers |
//! | `auto_recovery` | true | Automatically recover when instance returns |

pub mod assignment;
pub mod distribution;
pub mod error;
pub mod failover;
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
pub use failover::{
    FailoverConfig, FailoverError, FailoverEvent, FailoverManager, FailoverReason, HealthStatus,
    InstanceHealth, OverrideManager, OverrideRequest,
};
pub use rotation::{CrawlerInstance, NewsCategory, RotationScheduler};
pub use schedule::{DailySchedule, HourlySlot, ScheduleCache, ScheduleMetadata};
pub use trigger::{ScheduleTrigger, TriggerConfig};
