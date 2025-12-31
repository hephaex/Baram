//! Distributed crawler communication tests
//!
//! Tests the interaction between coordinator and worker instances:
//! 1. Instance registration
//! 2. Heartbeat and health monitoring
//! 3. Schedule distribution

use baram::coordinator::registry::{HeartbeatRequest, InstanceRegistry, RegisterRequest};
use baram::coordinator::InstanceStatus;
use baram::scheduler::{CrawlerInstance, RotationScheduler};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// Instance Registration Tests
// ============================================================================

#[tokio::test]
async fn test_instance_registration() {
    // Create instance registry
    let registry = InstanceRegistry::new(30, 10); // 30 sec timeout, max 10 instances

    // Create registration request
    let request = RegisterRequest {
        instance_id: CrawlerInstance::Main.id().to_string(),
        ip_address: "127.0.0.1".to_string(),
        port: 8080,
        version: Some("test-version".to_string()),
        metadata: HashMap::new(),
    };

    // Register an instance
    let response = registry.register(request).await;

    // Verify registration
    assert!(response.is_ok(), "Registration should succeed");
    let response = response.unwrap();
    assert!(response.success);
    assert_eq!(response.instance, CrawlerInstance::Main);
    assert!(response.heartbeat_interval_secs > 0);

    // Verify instance is in registry
    let instances = registry.get_all_instances().await;
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].instance, CrawlerInstance::Main);
    assert_eq!(instances[0].status, InstanceStatus::Online);
}

#[tokio::test]
async fn test_multiple_instance_registration() {
    let registry = InstanceRegistry::new(30, 10);

    // Register multiple instances
    for (instance, port) in [
        (CrawlerInstance::Main, 8080),
        (CrawlerInstance::Sub1, 8081),
        (CrawlerInstance::Sub2, 8082),
    ] {
        let request = RegisterRequest {
            instance_id: instance.id().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port,
            version: Some("v1.0.0".to_string()),
            metadata: HashMap::new(),
        };
        registry.register(request).await.unwrap();
    }

    // Verify all instances are registered
    let instances = registry.get_all_instances().await;
    assert_eq!(instances.len(), 3);

    // Verify each instance
    let instance_ids: Vec<_> = instances.iter().map(|i| i.instance).collect();
    assert!(instance_ids.contains(&CrawlerInstance::Main));
    assert!(instance_ids.contains(&CrawlerInstance::Sub1));
    assert!(instance_ids.contains(&CrawlerInstance::Sub2));
}

// ============================================================================
// Heartbeat Tests
// ============================================================================

#[tokio::test]
async fn test_heartbeat_updates_status() {
    let registry = InstanceRegistry::new(30, 10);

    // Register instance
    let register_req = RegisterRequest {
        instance_id: CrawlerInstance::Main.id().to_string(),
        ip_address: "127.0.0.1".to_string(),
        port: 8080,
        version: Some("v1.0.0".to_string()),
        metadata: HashMap::new(),
    };
    registry.register(register_req).await.unwrap();

    // Send heartbeat
    let heartbeat_req = HeartbeatRequest {
        instance_id: CrawlerInstance::Main.id().to_string(),
        articles_crawled: 10,
        error_count: 0,
        current_category: Some("politics".to_string()),
    };

    let response = registry.heartbeat(heartbeat_req).await;
    assert!(response.is_ok(), "Heartbeat should succeed");

    // Verify heartbeat was processed
    let info = registry.get_instance(CrawlerInstance::Main).await.unwrap();
    assert_eq!(info.articles_crawled, 10);
    assert_eq!(info.error_count, 0);
    assert_eq!(info.status, InstanceStatus::Online);
}

#[tokio::test]
async fn test_heartbeat_timeout_detection() {
    let registry = InstanceRegistry::new(1, 10); // 1 second timeout

    // Register instance
    let request = RegisterRequest {
        instance_id: CrawlerInstance::Main.id().to_string(),
        ip_address: "127.0.0.1".to_string(),
        port: 8080,
        version: None,
        metadata: HashMap::new(),
    };
    registry.register(request).await.unwrap();

    // Wait for timeout
    sleep(Duration::from_millis(1500)).await;

    // Update stale statuses
    registry.update_statuses().await;

    // Check instance status - should be offline or degraded
    let info = registry.get_instance(CrawlerInstance::Main).await.unwrap();
    assert_ne!(
        info.status,
        InstanceStatus::Online,
        "Instance should not be online after timeout"
    );
}

// ============================================================================
// Instance Status Management Tests
// ============================================================================

#[tokio::test]
async fn test_instance_deregistration() {
    let registry = InstanceRegistry::new(30, 10);

    // Register instance
    let request = RegisterRequest {
        instance_id: CrawlerInstance::Main.id().to_string(),
        ip_address: "127.0.0.1".to_string(),
        port: 8080,
        version: None,
        metadata: HashMap::new(),
    };
    registry.register(request).await.unwrap();

    assert_eq!(registry.get_all_instances().await.len(), 1);

    // Deregister
    let result = registry.unregister(CrawlerInstance::Main).await;
    assert!(result.is_some());

    // Should no longer be in registry
    assert_eq!(registry.get_all_instances().await.len(), 0);
}

// ============================================================================
// Active Instance Queries
// ============================================================================

#[tokio::test]
async fn test_get_active_instances() {
    let registry = InstanceRegistry::new(30, 10);

    // Register multiple instances
    for instance in [CrawlerInstance::Main, CrawlerInstance::Sub1] {
        let request = RegisterRequest {
            instance_id: instance.id().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080 + instance as u16,
            version: None,
            metadata: HashMap::new(),
        };
        registry.register(request).await.unwrap();
    }

    // Set one to maintenance
    registry
        .set_maintenance(CrawlerInstance::Sub1, true)
        .await
        .unwrap();

    // Get active instances (available for work)
    let all_instances = registry.get_all_instances().await;
    let active: Vec<_> = all_instances
        .iter()
        .filter(|i| i.status.is_available())
        .collect();

    // Only Main should be available (Sub1 is in maintenance)
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].instance, CrawlerInstance::Main);
}

// ============================================================================
// Schedule Distribution Tests
// ============================================================================

#[tokio::test]
async fn test_schedule_generation_and_distribution() {
    // Create scheduler
    let scheduler = RotationScheduler::new();
    let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    // Generate schedule
    let schedule = scheduler.generate_daily_schedule(date);

    // Verify schedule is complete
    assert_eq!(schedule.slots.len(), 24);

    // Verify all instances have assigned hours
    let main_hours = schedule.slots_for_instance(CrawlerInstance::Main);
    let sub1_hours = schedule.slots_for_instance(CrawlerInstance::Sub1);
    let sub2_hours = schedule.slots_for_instance(CrawlerInstance::Sub2);

    assert!(!main_hours.is_empty());
    assert!(!sub1_hours.is_empty());
    assert!(!sub2_hours.is_empty());

    // Total should be 24 hours
    assert_eq!(main_hours.len() + sub1_hours.len() + sub2_hours.len(), 24);
}

// ============================================================================
// Concurrent Operations Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_heartbeats() {
    use std::sync::Arc;

    let registry = Arc::new(InstanceRegistry::new(30, 10));

    // Register multiple instances
    for instance in CrawlerInstance::all() {
        let request = RegisterRequest {
            instance_id: instance.id().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080 + instance as u16,
            version: Some("v1.0.0".to_string()),
            metadata: HashMap::new(),
        };
        registry.register(request).await.unwrap();
    }

    // Send concurrent heartbeats
    let handles: Vec<_> = CrawlerInstance::all()
        .into_iter()
        .map(|instance| {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move {
                for i in 0..10 {
                    let heartbeat = HeartbeatRequest {
                        instance_id: instance.id().to_string(),
                        articles_crawled: i,
                        error_count: 0,
                        current_category: None,
                    };
                    registry.heartbeat(heartbeat).await.unwrap();
                    sleep(Duration::from_millis(10)).await;
                }
            })
        })
        .collect();

    // Wait for all heartbeats to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // All instances should still be registered
    let instances = registry.get_all_instances().await;
    assert_eq!(instances.len(), 3);
}

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_all_active() {
    let registry = InstanceRegistry::new(30, 10);

    // Register all instances
    for instance in CrawlerInstance::all() {
        let request = RegisterRequest {
            instance_id: instance.id().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080 + instance as u16,
            version: None,
            metadata: HashMap::new(),
        };
        registry.register(request).await.unwrap();

        // Send heartbeat
        let heartbeat = HeartbeatRequest {
            instance_id: instance.id().to_string(),
            articles_crawled: 0,
            error_count: 0,
            current_category: None,
        };
        registry.heartbeat(heartbeat).await.unwrap();
    }

    // Get health summary
    let all_instances = registry.get_all_instances().await;
    let active_count = all_instances.iter().filter(|i| i.status.is_available()).count();
    assert_eq!(active_count, 3, "All instances should be active");
}

#[tokio::test]
async fn test_health_check_mixed_status() {
    let registry = InstanceRegistry::new(30, 10);

    // Register instances with different statuses
    for instance in [CrawlerInstance::Main, CrawlerInstance::Sub1] {
        let request = RegisterRequest {
            instance_id: instance.id().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080 + instance as u16,
            version: None,
            metadata: HashMap::new(),
        };
        registry.register(request).await.unwrap();
    }

    // Set Sub1 to maintenance
    registry
        .set_maintenance(CrawlerInstance::Sub1, true)
        .await
        .unwrap();

    // Get active instances (should only include Main)
    let all_instances = registry.get_all_instances().await;
    let active: Vec<_> = all_instances
        .iter()
        .filter(|i| i.status.is_available())
        .collect();

    assert_eq!(active.len(), 1);
    assert_eq!(active[0].instance, CrawlerInstance::Main);
}
