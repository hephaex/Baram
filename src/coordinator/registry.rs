//! Instance registry for tracking crawler instances
//!
//! This module manages the registration and health monitoring of
//! distributed crawler instances.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::scheduler::rotation::CrawlerInstance;

// ============================================================================
// Instance Status
// ============================================================================

/// Status of a registered instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceStatus {
    /// Instance is online and healthy
    Online,

    /// Instance missed heartbeats (may be unhealthy)
    Degraded,

    /// Instance is offline (no heartbeat for too long)
    Offline,

    /// Instance is in maintenance mode
    Maintenance,
}

impl InstanceStatus {
    /// Check if instance is available for work
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Online | Self::Degraded)
    }

    /// Get Korean label
    pub fn korean_label(&self) -> &'static str {
        match self {
            Self::Online => "온라인",
            Self::Degraded => "저하됨",
            Self::Offline => "오프라인",
            Self::Maintenance => "유지보수",
        }
    }
}

impl Default for InstanceStatus {
    fn default() -> Self {
        Self::Offline
    }
}

// ============================================================================
// Instance Info
// ============================================================================

/// Information about a registered instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    /// Instance identifier
    pub instance: CrawlerInstance,

    /// Current status
    pub status: InstanceStatus,

    /// IP address of the instance
    pub ip_address: String,

    /// Port the instance is listening on
    pub port: u16,

    /// When the instance was registered
    pub registered_at: DateTime<Utc>,

    /// Last heartbeat received
    pub last_heartbeat: DateTime<Utc>,

    /// Number of articles crawled in current session
    pub articles_crawled: u64,

    /// Number of errors encountered
    pub error_count: u64,

    /// Current crawling category (if active)
    pub current_category: Option<String>,

    /// Version of the crawler software
    pub version: Option<String>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl InstanceInfo {
    /// Create new instance info
    pub fn new(instance: CrawlerInstance, ip_address: String, port: u16) -> Self {
        let now = Utc::now();
        Self {
            instance,
            status: InstanceStatus::Online,
            ip_address,
            port,
            registered_at: now,
            last_heartbeat: now,
            articles_crawled: 0,
            error_count: 0,
            current_category: None,
            version: None,
            metadata: HashMap::new(),
        }
    }

    /// Update heartbeat timestamp
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Utc::now();
        if self.status == InstanceStatus::Offline || self.status == InstanceStatus::Degraded {
            self.status = InstanceStatus::Online;
        }
    }

    /// Check if heartbeat is stale
    pub fn is_heartbeat_stale(&self, timeout_secs: i64) -> bool {
        let age = Utc::now() - self.last_heartbeat;
        age.num_seconds() > timeout_secs
    }

    /// Get seconds since last heartbeat
    pub fn seconds_since_heartbeat(&self) -> i64 {
        (Utc::now() - self.last_heartbeat).num_seconds()
    }

    /// Get the full address
    pub fn address(&self) -> String {
        format!("{}:{}", self.ip_address, self.port)
    }

    /// Mark as offline
    pub fn mark_offline(&mut self) {
        self.status = InstanceStatus::Offline;
    }

    /// Mark as degraded
    pub fn mark_degraded(&mut self) {
        if self.status == InstanceStatus::Online {
            self.status = InstanceStatus::Degraded;
        }
    }

    /// Set maintenance mode
    pub fn set_maintenance(&mut self, enabled: bool) {
        if enabled {
            self.status = InstanceStatus::Maintenance;
        } else if self.status == InstanceStatus::Maintenance {
            self.status = InstanceStatus::Online;
        }
    }

    /// Update crawl stats
    pub fn update_stats(&mut self, articles: u64, errors: u64) {
        self.articles_crawled += articles;
        self.error_count += errors;
    }
}

// ============================================================================
// Registration Request/Response
// ============================================================================

/// Request to register an instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub instance_id: String,
    pub ip_address: String,
    pub port: u16,
    pub version: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Response to registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub success: bool,
    pub instance: CrawlerInstance,
    pub message: String,
    pub heartbeat_interval_secs: u64,
}

/// Heartbeat request from instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub instance_id: String,
    pub articles_crawled: u64,
    pub error_count: u64,
    pub current_category: Option<String>,
}

/// Heartbeat response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub message: String,
    pub should_crawl: bool,
    pub assigned_categories: Vec<String>,
}

// ============================================================================
// Instance Registry
// ============================================================================

/// Registry for tracking all crawler instances
pub struct InstanceRegistry {
    /// Registered instances
    instances: Arc<RwLock<HashMap<CrawlerInstance, InstanceInfo>>>,

    /// Heartbeat timeout in seconds
    heartbeat_timeout_secs: i64,

    /// Degraded threshold (fraction of timeout)
    degraded_threshold_secs: i64,

    /// Max instances allowed
    max_instances: usize,
}

impl InstanceRegistry {
    /// Create a new registry
    pub fn new(heartbeat_timeout_secs: u64, max_instances: usize) -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            heartbeat_timeout_secs: heartbeat_timeout_secs as i64,
            degraded_threshold_secs: (heartbeat_timeout_secs as i64) / 2,
            max_instances,
        }
    }

    /// Register a new instance
    pub async fn register(&self, request: RegisterRequest) -> Result<RegisterResponse, RegistryError> {
        let instance = CrawlerInstance::from_id(&request.instance_id)
            .map_err(|_| RegistryError::InvalidInstanceId(request.instance_id.clone()))?;

        let mut instances = self.instances.write().await;

        // Check if at capacity (excluding re-registration)
        if !instances.contains_key(&instance) && instances.len() >= self.max_instances {
            return Err(RegistryError::CapacityExceeded {
                current: instances.len(),
                max: self.max_instances,
            });
        }

        // Create or update instance info
        let mut info = InstanceInfo::new(instance, request.ip_address, request.port);
        info.version = request.version;
        info.metadata = request.metadata;

        instances.insert(instance, info);

        Ok(RegisterResponse {
            success: true,
            instance,
            message: format!("Instance {} registered successfully", instance.id()),
            heartbeat_interval_secs: (self.heartbeat_timeout_secs / 3) as u64,
        })
    }

    /// Process heartbeat from instance
    pub async fn heartbeat(&self, request: HeartbeatRequest) -> Result<HeartbeatResponse, RegistryError> {
        let instance = CrawlerInstance::from_id(&request.instance_id)
            .map_err(|_| RegistryError::InvalidInstanceId(request.instance_id.clone()))?;

        let mut instances = self.instances.write().await;

        let info = instances
            .get_mut(&instance)
            .ok_or_else(|| RegistryError::InstanceNotFound(instance))?;

        info.update_heartbeat();
        info.articles_crawled = request.articles_crawled;
        info.error_count = request.error_count;
        info.current_category = request.current_category;

        Ok(HeartbeatResponse {
            success: true,
            message: "Heartbeat received".to_string(),
            should_crawl: info.status.is_available(),
            assigned_categories: vec![], // Will be populated by schedule manager
        })
    }

    /// Get instance info
    pub async fn get_instance(&self, instance: CrawlerInstance) -> Option<InstanceInfo> {
        self.instances.read().await.get(&instance).cloned()
    }

    /// Get all instances
    pub async fn get_all_instances(&self) -> Vec<InstanceInfo> {
        self.instances.read().await.values().cloned().collect()
    }

    /// Get online instances
    pub async fn get_online_instances(&self) -> Vec<InstanceInfo> {
        self.instances
            .read()
            .await
            .values()
            .filter(|i| i.status.is_available())
            .cloned()
            .collect()
    }

    /// Unregister an instance
    pub async fn unregister(&self, instance: CrawlerInstance) -> Option<InstanceInfo> {
        self.instances.write().await.remove(&instance)
    }

    /// Update instance statuses based on heartbeat timestamps
    pub async fn update_statuses(&self) {
        let mut instances = self.instances.write().await;

        for info in instances.values_mut() {
            if info.status == InstanceStatus::Maintenance {
                continue;
            }

            let age = info.seconds_since_heartbeat();

            if age > self.heartbeat_timeout_secs {
                info.status = InstanceStatus::Offline;
            } else if age > self.degraded_threshold_secs {
                info.status = InstanceStatus::Degraded;
            } else {
                info.status = InstanceStatus::Online;
            }
        }
    }

    /// Get registry statistics
    pub async fn stats(&self) -> RegistryStats {
        let instances = self.instances.read().await;

        let mut online = 0;
        let mut degraded = 0;
        let mut offline = 0;
        let mut maintenance = 0;
        let mut total_articles = 0;
        let mut total_errors = 0;

        for info in instances.values() {
            match info.status {
                InstanceStatus::Online => online += 1,
                InstanceStatus::Degraded => degraded += 1,
                InstanceStatus::Offline => offline += 1,
                InstanceStatus::Maintenance => maintenance += 1,
            }
            total_articles += info.articles_crawled;
            total_errors += info.error_count;
        }

        RegistryStats {
            total_instances: instances.len(),
            online,
            degraded,
            offline,
            maintenance,
            total_articles,
            total_errors,
        }
    }

    /// Set maintenance mode for an instance
    pub async fn set_maintenance(&self, instance: CrawlerInstance, enabled: bool) -> Result<(), RegistryError> {
        let mut instances = self.instances.write().await;

        let info = instances
            .get_mut(&instance)
            .ok_or_else(|| RegistryError::InstanceNotFound(instance))?;

        info.set_maintenance(enabled);
        Ok(())
    }

    /// Start background task to periodically update statuses
    pub fn start_status_updater(self: Arc<Self>, interval_secs: u64) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;
                self.update_statuses().await;
            }
        })
    }
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_instances: usize,
    pub online: usize,
    pub degraded: usize,
    pub offline: usize,
    pub maintenance: usize,
    pub total_articles: u64,
    pub total_errors: u64,
}

impl RegistryStats {
    /// Get availability percentage
    pub fn availability(&self) -> f64 {
        if self.total_instances == 0 {
            0.0
        } else {
            ((self.online + self.degraded) as f64 / self.total_instances as f64) * 100.0
        }
    }

    /// Format as display string
    pub fn display(&self) -> String {
        format!(
            "Registry Stats\n\
             {:-<30}\n\
             Total Instances: {}\n\
             - Online: {}\n\
             - Degraded: {}\n\
             - Offline: {}\n\
             - Maintenance: {}\n\
             Availability: {:.1}%\n\
             Total Articles: {}\n\
             Total Errors: {}",
            "",
            self.total_instances,
            self.online,
            self.degraded,
            self.offline,
            self.maintenance,
            self.availability(),
            self.total_articles,
            self.total_errors
        )
    }
}

// ============================================================================
// Errors
// ============================================================================

/// Registry errors
#[derive(Debug, Clone)]
pub enum RegistryError {
    /// Invalid instance ID
    InvalidInstanceId(String),

    /// Instance not found
    InstanceNotFound(CrawlerInstance),

    /// Registry at capacity
    CapacityExceeded { current: usize, max: usize },

    /// Instance already registered
    AlreadyRegistered(CrawlerInstance),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInstanceId(id) => write!(f, "Invalid instance ID: {}", id),
            Self::InstanceNotFound(instance) => write!(f, "Instance not found: {}", instance),
            Self::CapacityExceeded { current, max } => {
                write!(f, "Registry at capacity: {}/{}", current, max)
            }
            Self::AlreadyRegistered(instance) => {
                write!(f, "Instance already registered: {}", instance)
            }
        }
    }
}

impl std::error::Error for RegistryError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_register_request(id: &str) -> RegisterRequest {
        RegisterRequest {
            instance_id: id.to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080,
            version: Some("1.0.0".to_string()),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_instance_status() {
        assert!(InstanceStatus::Online.is_available());
        assert!(InstanceStatus::Degraded.is_available());
        assert!(!InstanceStatus::Offline.is_available());
        assert!(!InstanceStatus::Maintenance.is_available());
    }

    #[test]
    fn test_instance_info_creation() {
        let info = InstanceInfo::new(CrawlerInstance::Main, "192.168.1.1".to_string(), 9000);

        assert_eq!(info.instance, CrawlerInstance::Main);
        assert_eq!(info.status, InstanceStatus::Online);
        assert_eq!(info.address(), "192.168.1.1:9000");
    }

    #[test]
    fn test_instance_info_heartbeat() {
        let mut info = InstanceInfo::new(CrawlerInstance::Main, "127.0.0.1".to_string(), 8080);
        info.status = InstanceStatus::Degraded;

        info.update_heartbeat();

        assert_eq!(info.status, InstanceStatus::Online);
        assert!(info.seconds_since_heartbeat() < 2);
    }

    #[tokio::test]
    async fn test_registry_register() {
        let registry = InstanceRegistry::new(90, 10);

        let request = create_register_request("main");
        let response = registry.register(request).await.unwrap();

        assert!(response.success);
        assert_eq!(response.instance, CrawlerInstance::Main);
    }

    #[tokio::test]
    async fn test_registry_heartbeat() {
        let registry = InstanceRegistry::new(90, 10);

        // Register first
        let request = create_register_request("main");
        registry.register(request).await.unwrap();

        // Send heartbeat
        let hb_request = HeartbeatRequest {
            instance_id: "main".to_string(),
            articles_crawled: 50,
            error_count: 2,
            current_category: Some("politics".to_string()),
        };

        let response = registry.heartbeat(hb_request).await.unwrap();
        assert!(response.success);

        // Check updated info
        let info = registry.get_instance(CrawlerInstance::Main).await.unwrap();
        assert_eq!(info.articles_crawled, 50);
        assert_eq!(info.error_count, 2);
    }

    #[tokio::test]
    async fn test_registry_capacity() {
        let registry = InstanceRegistry::new(90, 2);

        // Register two instances
        registry.register(create_register_request("main")).await.unwrap();
        registry.register(create_register_request("sub1")).await.unwrap();

        // Third should fail
        let result = registry.register(create_register_request("sub2")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = InstanceRegistry::new(90, 10);

        registry.register(create_register_request("main")).await.unwrap();

        let removed = registry.unregister(CrawlerInstance::Main).await;
        assert!(removed.is_some());

        let info = registry.get_instance(CrawlerInstance::Main).await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_registry_stats() {
        let registry = InstanceRegistry::new(90, 10);

        registry.register(create_register_request("main")).await.unwrap();
        registry.register(create_register_request("sub1")).await.unwrap();

        let stats = registry.stats().await;
        assert_eq!(stats.total_instances, 2);
        assert_eq!(stats.online, 2);
    }

    #[tokio::test]
    async fn test_registry_maintenance() {
        let registry = InstanceRegistry::new(90, 10);

        registry.register(create_register_request("main")).await.unwrap();
        registry.set_maintenance(CrawlerInstance::Main, true).await.unwrap();

        let info = registry.get_instance(CrawlerInstance::Main).await.unwrap();
        assert_eq!(info.status, InstanceStatus::Maintenance);
        assert!(!info.status.is_available());
    }

    #[test]
    fn test_registry_stats_availability() {
        let stats = RegistryStats {
            total_instances: 4,
            online: 2,
            degraded: 1,
            offline: 1,
            maintenance: 0,
            total_articles: 100,
            total_errors: 5,
        };

        // (2 + 1) / 4 = 75%
        assert!((stats.availability() - 75.0).abs() < 0.1);
    }
}
