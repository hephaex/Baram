//! Integration tests for the distributed scheduler system
//!
//! These tests verify the complete workflow of:
//! - Schedule generation and distribution
//! - Instance rotation and category assignment
//! - Failover handling
//! - Manual overrides

use chrono::NaiveDate;
use ntimes::scheduler::{
    CategoryAssigner, CategoryPriority, CrawlerInstance, DistributionConfig, FailoverConfig,
    FailoverManager, FailoverReason, NewsCategory, OverrideManager, OverrideRequest,
    RotationScheduler, ScheduleCache, ScheduleDistributor,
};
use std::sync::Arc;

// ============================================================================
// Rotation Integration Tests
// ============================================================================

#[test]
fn test_full_day_rotation_coverage() {
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let schedule = scheduler.generate_daily_schedule(date);

    // Verify all 24 hours are covered
    assert_eq!(schedule.slots.len(), 24);

    // Verify each hour has correct index
    for (i, slot) in schedule.slots.iter().enumerate() {
        assert_eq!(slot.hour as usize, i);
    }

    // Verify all instances get assigned
    let main_slots = schedule.slots_for_instance(CrawlerInstance::Main);
    let sub1_slots = schedule.slots_for_instance(CrawlerInstance::Sub1);
    let sub2_slots = schedule.slots_for_instance(CrawlerInstance::Sub2);

    // Each instance should have exactly 8 slots (24/3)
    assert_eq!(main_slots.len(), 8);
    assert_eq!(sub1_slots.len(), 8);
    assert_eq!(sub2_slots.len(), 8);
}

#[test]
fn test_rotation_determinism_across_calls() {
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();

    // Generate schedule multiple times
    let schedules: Vec<_> = (0..5)
        .map(|_| scheduler.generate_daily_schedule(date))
        .collect();

    // All schedules should be identical
    for (i, schedule) in schedules.iter().enumerate().skip(1) {
        assert_eq!(
            schedule.slots.len(),
            schedules[0].slots.len(),
            "Schedule {} has different slot count",
            i
        );

        for (j, slot) in schedule.slots.iter().enumerate() {
            assert_eq!(
                slot.instance, schedules[0].slots[j].instance,
                "Schedule {} differs at hour {}",
                i, j
            );
        }
    }
}

#[test]
fn test_rotation_varies_by_date() {
    let scheduler = RotationScheduler::new();

    // Get rotation for multiple consecutive dates
    let dates: Vec<_> = (1..=7)
        .map(|d| NaiveDate::from_ymd_opt(2024, 1, d).unwrap())
        .collect();

    let rotations: Vec<_> = dates
        .iter()
        .map(|d| scheduler.get_daily_rotation(*d))
        .collect();

    // Not all rotations should be identical (statistical property)
    let unique_rotations: std::collections::HashSet<_> = rotations.iter().collect();
    assert!(
        unique_rotations.len() > 1,
        "Expected variety in rotations across different dates"
    );
}

// ============================================================================
// Category Assignment Integration Tests
// ============================================================================

#[test]
fn test_category_coverage_over_day() {
    let mut assigner = CategoryAssigner::new().with_categories_per_slot(2);
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let rotation = scheduler.get_daily_rotation(date);

    let schedule = assigner.generate_schedule(date, &rotation);

    // Count category occurrences
    let mut category_counts: std::collections::HashMap<NewsCategory, usize> =
        std::collections::HashMap::new();

    for slot in &schedule.slots {
        for cat in &slot.categories {
            *category_counts.entry(*cat).or_insert(0) += 1;
        }
    }

    // All categories should appear at least once
    for category in NewsCategory::all() {
        assert!(
            category_counts.contains_key(&category),
            "Category {:?} was never assigned",
            category
        );
    }
}

#[test]
fn test_category_priority_affects_distribution() {
    let mut assigner = CategoryAssigner::new()
        .with_strategy(ntimes::scheduler::AssignmentStrategy::Weighted)
        .with_categories_per_slot(2);

    // Set high priority for Politics
    assigner.set_priority(NewsCategory::Politics, CategoryPriority::Critical);

    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let rotation = scheduler.get_daily_rotation(date);

    let schedule = assigner.generate_schedule(date, &rotation);

    // Count Politics appearances
    let politics_count = schedule
        .slots
        .iter()
        .filter(|s| s.categories.contains(&NewsCategory::Politics))
        .count();

    // High priority should result in more assignments
    assert!(
        politics_count >= 12,
        "Politics should appear in at least half the slots with Critical priority"
    );
}

#[test]
fn test_disabled_category_not_assigned() {
    let mut assigner = CategoryAssigner::new().with_categories_per_slot(2);

    // Disable Culture category
    assigner.set_enabled(NewsCategory::Culture, false);

    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let rotation = scheduler.get_daily_rotation(date);

    let schedule = assigner.generate_schedule(date, &rotation);

    // Culture should not appear
    for slot in &schedule.slots {
        assert!(
            !slot.categories.contains(&NewsCategory::Culture),
            "Disabled category Culture was assigned at hour {}",
            slot.hour
        );
    }
}

// ============================================================================
// Distribution Integration Tests
// ============================================================================

#[tokio::test]
async fn test_distribution_workflow() {
    let config = DistributionConfig::default();
    let distributor = ScheduleDistributor::new(config);

    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let schedule = scheduler.generate_daily_schedule(date);

    // Subscribe to events
    let _receiver = distributor.subscribe();

    // Distribute schedule
    let result = distributor.distribute(schedule.clone()).await;
    assert!(result.success);
    assert_eq!(result.date, date);

    // Verify schedule is stored
    let stored = distributor.get_schedule().await.unwrap();
    assert_eq!(stored.date, date);
    assert_eq!(stored.slots.len(), 24);

    // Verify all instances have state
    for instance in CrawlerInstance::all() {
        let state = distributor.get_instance_state(instance).await;
        assert!(state.is_some(), "Instance {:?} should have state", instance);
        assert!(!state.unwrap().assigned_hours.is_empty());
    }
}

#[tokio::test]
async fn test_distribution_acknowledgment() {
    let distributor = ScheduleDistributor::new(DistributionConfig::default());
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let schedule = scheduler.generate_daily_schedule(date);

    distributor.distribute(schedule).await;

    // Initially no acknowledgments
    assert!(!distributor.all_acknowledged().await);
    assert_eq!(distributor.pending_instances().await.len(), 3);

    // Acknowledge each instance
    for instance in CrawlerInstance::all() {
        distributor.acknowledge(instance).await;
    }

    // All should be acknowledged
    assert!(distributor.all_acknowledged().await);
    assert_eq!(distributor.pending_instances().await.len(), 0);
}

#[tokio::test]
async fn test_distribution_update() {
    let distributor = ScheduleDistributor::new(DistributionConfig::default());
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let schedule = scheduler.generate_daily_schedule(date);

    distributor.distribute(schedule).await;

    // Update hours 10, 11, 12 to Sub2
    let result = distributor
        .update_hours(
            date,
            vec![10, 11, 12],
            CrawlerInstance::Sub2,
            ntimes::scheduler::UpdateReason::ManualOverride,
        )
        .await;

    assert!(result.success);

    // Verify update took effect
    let updated = distributor.get_schedule().await.unwrap();
    for hour in [10, 11, 12] {
        let slot = updated.get_slot(hour).unwrap();
        assert_eq!(
            slot.instance,
            CrawlerInstance::Sub2,
            "Hour {} should be assigned to Sub2",
            hour
        );
    }
}

// ============================================================================
// Failover Integration Tests
// ============================================================================

#[tokio::test]
async fn test_failover_on_consecutive_failures() {
    let config = FailoverConfig {
        max_failures: 3,
        failover_cooldown_secs: 0, // Disable cooldown for testing
        ..Default::default()
    };
    let manager = FailoverManager::new(config);

    // Make Sub1 and Sub2 healthy first
    for _ in 0..3 {
        manager.process_heartbeat(CrawlerInstance::Sub1).await;
        manager.process_heartbeat(CrawlerInstance::Sub2).await;
    }

    // Simulate failures for Main
    for i in 0..3 {
        manager
            .process_failure(CrawlerInstance::Main, Some(format!("Error {}", i)))
            .await;
    }

    // Check Main is now unhealthy
    let health = manager.get_health(CrawlerInstance::Main).await.unwrap();
    assert_eq!(health.status, ntimes::scheduler::HealthStatus::Unhealthy);

    // Check failover history
    let history = manager.get_history().await;
    assert_eq!(history.len(), 1, "Should have one failover event");
    assert_eq!(history[0].failed_instance, CrawlerInstance::Main);
}

#[tokio::test]
async fn test_failover_target_selection() {
    let config = FailoverConfig {
        failover_order: vec![
            CrawlerInstance::Main,
            CrawlerInstance::Sub1,
            CrawlerInstance::Sub2,
        ],
        ..Default::default()
    };
    let manager = FailoverManager::new(config);

    // Make Main healthy
    for _ in 0..3 {
        manager.process_heartbeat(CrawlerInstance::Main).await;
    }

    // Failover from Sub1 should go to Main (first in order that's healthy)
    let result = manager
        .initiate_failover(CrawlerInstance::Sub1, FailoverReason::ManualOverride)
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().target_instance, CrawlerInstance::Main);
}

#[tokio::test]
async fn test_failover_cooldown() {
    let config = FailoverConfig {
        failover_cooldown_secs: 300, // 5 minutes
        ..Default::default()
    };
    let manager = FailoverManager::new(config);

    // Make others healthy
    for _ in 0..3 {
        manager.process_heartbeat(CrawlerInstance::Sub1).await;
    }

    // First failover should succeed
    let result1 = manager
        .initiate_failover(CrawlerInstance::Main, FailoverReason::ManualOverride)
        .await;
    assert!(result1.is_ok());

    // Second immediate failover should fail due to cooldown
    let result2 = manager
        .initiate_failover(CrawlerInstance::Main, FailoverReason::ManualOverride)
        .await;
    assert!(result2.is_err());
}

#[tokio::test]
async fn test_maintenance_mode() {
    let config = FailoverConfig {
        failover_cooldown_secs: 0,
        ..Default::default()
    };
    let manager = FailoverManager::new(config);

    // Make others healthy first
    for _ in 0..3 {
        manager.process_heartbeat(CrawlerInstance::Sub1).await;
        manager.process_heartbeat(CrawlerInstance::Sub2).await;
    }

    // Put Main in maintenance - this triggers failover which marks it unhealthy
    manager.set_maintenance(CrawlerInstance::Main, true).await;

    let health = manager.get_health(CrawlerInstance::Main).await.unwrap();
    // Maintenance triggers failover, so status becomes Unhealthy
    assert!(!health.status.can_handle_work());

    // Verify a failover event was created for maintenance
    let history = manager.get_history().await;
    assert!(history.iter().any(|e| matches!(e.reason, FailoverReason::Maintenance)));
}

// ============================================================================
// Override Integration Tests
// ============================================================================

#[tokio::test]
async fn test_override_workflow() {
    let manager = OverrideManager::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    // Apply override
    let request = OverrideRequest {
        date,
        hours: vec![14, 15, 16],
        instance: CrawlerInstance::Sub2,
        reason: "Testing override".to_string(),
        operator: Some("test_user".to_string()),
    };

    let override_result = manager.apply_override(request.clone()).await;
    assert!(override_result.is_ok());

    // Verify override is active
    let active = manager.get_active_overrides().await;
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].request.hours, vec![14, 15, 16]);
    assert_eq!(active[0].request.instance, CrawlerInstance::Sub2);

    // Get overrides for date
    let date_overrides = manager.get_overrides_for_date(date).await;
    assert_eq!(date_overrides.len(), 1);

    // Cancel override
    manager.cancel_override(&active[0].id).await.unwrap();

    // Should be no active overrides now
    assert_eq!(manager.get_active_overrides().await.len(), 0);
}

#[tokio::test]
async fn test_override_invalid_hour() {
    let manager = OverrideManager::new();

    let request = OverrideRequest {
        date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        hours: vec![24], // Invalid
        instance: CrawlerInstance::Main,
        reason: "Invalid test".to_string(),
        operator: None,
    };

    let result = manager.apply_override(request).await;
    assert!(result.is_err());
}

// ============================================================================
// Cache Integration Tests
// ============================================================================

#[tokio::test]
async fn test_cache_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("schedule_cache.json");

    // Create and populate cache
    {
        let cache = ScheduleCache::with_file(&cache_path);
        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = scheduler.generate_daily_schedule(date);

        cache.update(schedule).await.unwrap();
        assert!(cache_path.exists());
    }

    // Load from new cache instance
    {
        let cache = ScheduleCache::with_file(&cache_path);
        let loaded = cache.load_from_file().await.unwrap();
        assert!(loaded);

        let schedule = cache.get().await.unwrap();
        assert_eq!(schedule.date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
        assert_eq!(schedule.slots.len(), 24);
    }
}

#[tokio::test]
async fn test_cache_validity() {
    let cache = ScheduleCache::new().with_validity_hours(24);

    // Initially invalid
    assert!(!cache.is_valid().await);

    // After update, should be valid
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let schedule = scheduler.generate_daily_schedule(date);

    cache.update(schedule).await.unwrap();
    assert!(cache.is_valid().await);
}

// ============================================================================
// End-to-End Integration Tests
// ============================================================================

#[tokio::test]
async fn test_complete_scheduling_workflow() {
    // 1. Create scheduler and generate schedule
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let schedule = scheduler.generate_daily_schedule(date);

    // 2. Set up distributor
    let distributor = Arc::new(ScheduleDistributor::new(DistributionConfig::default()));

    // 3. Set up failover manager with distributor
    let failover = FailoverManager::new(FailoverConfig {
        failover_cooldown_secs: 0,
        ..Default::default()
    })
    .with_distributor(distributor.clone());

    // 4. Distribute schedule
    let dist_result = distributor.distribute(schedule.clone()).await;
    assert!(dist_result.success);

    // 5. Simulate healthy instances
    for _ in 0..3 {
        failover.process_heartbeat(CrawlerInstance::Main).await;
        failover.process_heartbeat(CrawlerInstance::Sub1).await;
        failover.process_heartbeat(CrawlerInstance::Sub2).await;
    }

    // 6. All instances should be healthy
    let stats = failover.stats().await;
    assert_eq!(stats.healthy_count, 3);

    // 7. Simulate Main failure
    for _ in 0..3 {
        failover
            .process_failure(CrawlerInstance::Main, Some("Connection timeout".to_string()))
            .await;
    }

    // 8. Check failover occurred
    let history = failover.get_history().await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].failed_instance, CrawlerInstance::Main);

    // 9. Verify failover event has correct data
    let event = &history[0];
    assert!(!event.affected_hours.is_empty(), "Failover should affect some hours");
    assert_ne!(event.target_instance, CrawlerInstance::Main, "Target should not be the failed instance");

    // 10. Verify Main is now unhealthy
    let main_health = failover.get_health(CrawlerInstance::Main).await.unwrap();
    assert_eq!(main_health.status, ntimes::scheduler::HealthStatus::Unhealthy);

    // 11. Verify stats reflect the failure
    let stats_after = failover.stats().await;
    assert_eq!(stats_after.unhealthy_count, 1);
    assert_eq!(stats_after.total_failovers, 1);
}

#[tokio::test]
async fn test_schedule_summary_accuracy() {
    let scheduler = RotationScheduler::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let schedule = scheduler.generate_daily_schedule(date);

    let summary = schedule.summary();

    // Verify summary is accurate
    assert_eq!(summary.date, date);
    assert_eq!(summary.total_slots, 24);
    assert_eq!(summary.instance_distribution.len(), 3);

    // Total instance assignments should equal 24
    let total_assigned: usize = summary.instance_distribution.values().sum();
    assert_eq!(total_assigned, 24);
}
