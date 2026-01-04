//! Health Check Module for Kubernetes Probes
//!
//! This module provides comprehensive health check endpoints for:
//! - Liveness probes: Is the application running?
//! - Readiness probes: Is the application ready to receive traffic?
//! - Startup probes: Has the application finished starting up?
//!
//! # Kubernetes Integration
//!
//! ```yaml
//! livenessProbe:
//!   httpGet:
//!     path: /health/live
//!     port: 8080
//!   initialDelaySeconds: 10
//!   periodSeconds: 10
//!
//! readinessProbe:
//!   httpGet:
//!     path: /health/ready
//!     port: 8080
//!   initialDelaySeconds: 5
//!   periodSeconds: 5
//!
//! startupProbe:
//!   httpGet:
//!     path: /health/startup
//!     port: 8080
//!   failureThreshold: 30
//!   periodSeconds: 10
//! ```

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::server::AppState;

// ============================================================================
// Health Status Types
// ============================================================================

/// Overall health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            HealthStatus::Healthy => StatusCode::OK,
            HealthStatus::Degraded => StatusCode::OK, // 200 to keep pod running
            HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// Component health check result
#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// Liveness probe response
#[derive(Debug, Serialize)]
pub struct LivenessResponse {
    pub status: HealthStatus,
    pub timestamp: String,
}

/// Readiness probe response
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: HealthStatus,
    pub timestamp: String,
    pub checks: Vec<ComponentHealth>,
}

/// Startup probe response
#[derive(Debug, Serialize)]
pub struct StartupResponse {
    pub ready: bool,
    pub status: HealthStatus,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Comprehensive health response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub version: String,
    pub uptime_secs: u64,
    pub timestamp: String,
    pub checks: Vec<ComponentHealth>,
}

// ============================================================================
// Health Checker
// ============================================================================

/// Health checker for managing application health state
#[derive(Clone)]
pub struct HealthChecker {
    /// Whether the application has completed startup
    startup_complete: Arc<AtomicBool>,
    /// Whether the application is ready to receive traffic
    ready: Arc<AtomicBool>,
    /// Start time for uptime calculation
    start_time: Instant,
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self {
            startup_complete: Arc::new(AtomicBool::new(false)),
            ready: Arc::new(AtomicBool::new(false)),
            start_time: Instant::now(),
        }
    }

    /// Mark startup as complete
    pub fn mark_startup_complete(&self) {
        self.startup_complete.store(true, Ordering::SeqCst);
        tracing::info!("Health check: startup marked as complete");
    }

    /// Mark application as ready
    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
        tracing::info!("Health check: application marked as ready");
    }

    /// Mark application as not ready
    pub fn mark_not_ready(&self) {
        self.ready.store(false, Ordering::SeqCst);
        tracing::warn!("Health check: application marked as not ready");
    }

    /// Check if startup is complete
    pub fn is_startup_complete(&self) -> bool {
        self.startup_complete.load(Ordering::SeqCst)
    }

    /// Check if application is ready
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

// ============================================================================
// Health Check Router
// ============================================================================

/// Create health check router
pub fn create_health_router(state: AppState) -> Router {
    Router::new()
        // Liveness probe - is the application alive?
        .route("/health/live", get(liveness_probe))
        // Readiness probe - is the application ready to serve traffic?
        .route("/health/ready", get(readiness_probe))
        // Startup probe - has the application finished starting?
        .route("/health/startup", get(startup_probe))
        // Comprehensive health check
        .route("/health", get(health_check))
        .with_state(state)
}

// ============================================================================
// Health Check Handlers
// ============================================================================

/// Liveness probe handler
///
/// Returns 200 if the application process is running.
/// This should always succeed unless the process is dead.
async fn liveness_probe() -> impl IntoResponse {
    let response = LivenessResponse {
        status: HealthStatus::Healthy,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    (StatusCode::OK, Json(response))
}

/// Readiness probe handler
///
/// Returns 200 if the application is ready to receive traffic.
/// Checks critical dependencies like registry connectivity.
async fn readiness_probe(State(state): State<AppState>) -> impl IntoResponse {
    let mut checks = Vec::new();
    let mut overall_status = HealthStatus::Healthy;

    // Check registry health
    let registry_start = Instant::now();
    let registry_stats = state.registry.stats().await;
    let registry_latency = registry_start.elapsed().as_millis() as u64;

    checks.push(ComponentHealth {
        name: "registry".to_string(),
        status: HealthStatus::Healthy,
        message: Some(format!("{} instances registered", registry_stats.total_instances)),
        latency_ms: Some(registry_latency),
    });

    // Check schedule cache health
    let cache_start = Instant::now();
    let cache_status = state.cache.status().await;
    let cache_latency = cache_start.elapsed().as_millis() as u64;

    let cache_health = if cache_status.is_valid {
        HealthStatus::Healthy
    } else {
        overall_status = HealthStatus::Degraded;
        HealthStatus::Degraded
    };

    checks.push(ComponentHealth {
        name: "schedule_cache".to_string(),
        status: cache_health,
        message: Some(if cache_status.has_schedule {
            "schedule loaded".to_string()
        } else {
            "no schedule cached".to_string()
        }),
        latency_ms: Some(cache_latency),
    });

    // Check uptime (degraded if just started)
    let uptime = state.start_time.elapsed().as_secs();
    if uptime < 10 {
        overall_status = HealthStatus::Degraded;
        checks.push(ComponentHealth {
            name: "startup".to_string(),
            status: HealthStatus::Degraded,
            message: Some(format!("warming up ({uptime}s uptime)")),
            latency_ms: None,
        });
    }

    let response = ReadinessResponse {
        status: overall_status,
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks,
    };

    (overall_status.status_code(), Json(response))
}

/// Startup probe handler
///
/// Returns 200 once the application has finished starting up.
/// Used by Kubernetes to know when to start liveness/readiness probes.
async fn startup_probe(State(state): State<AppState>) -> impl IntoResponse {
    // Consider startup complete after 5 seconds
    let uptime = state.start_time.elapsed().as_secs();
    let ready = uptime >= 5;

    let (status, message) = if ready {
        (HealthStatus::Healthy, None)
    } else {
        (
            HealthStatus::Unhealthy,
            Some(format!("starting up... ({uptime}s elapsed)")),
        )
    };

    let response = StartupResponse {
        ready,
        status,
        timestamp: chrono::Utc::now().to_rfc3339(),
        message,
    };

    (status.status_code(), Json(response))
}

/// Comprehensive health check handler
///
/// Returns detailed health information about all components.
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let mut checks = Vec::new();
    let mut overall_status = HealthStatus::Healthy;

    // Check registry
    let registry_start = Instant::now();
    let registry_stats = state.registry.stats().await;
    let registry_latency = registry_start.elapsed().as_millis() as u64;

    let registry_status = if registry_stats.online > 0 || registry_stats.total_instances == 0 {
        HealthStatus::Healthy
    } else {
        overall_status = HealthStatus::Degraded;
        HealthStatus::Degraded
    };

    checks.push(ComponentHealth {
        name: "instance_registry".to_string(),
        status: registry_status,
        message: Some(format!(
            "{} total, {} online, {} offline",
            registry_stats.total_instances, registry_stats.online, registry_stats.offline
        )),
        latency_ms: Some(registry_latency),
    });

    // Check schedule cache
    let cache_start = Instant::now();
    let cache_status = state.cache.status().await;
    let cache_latency = cache_start.elapsed().as_millis() as u64;

    checks.push(ComponentHealth {
        name: "schedule_cache".to_string(),
        status: if cache_status.is_valid {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        },
        message: Some(format!(
            "valid: {}, has_schedule: {}",
            cache_status.is_valid, cache_status.has_schedule
        )),
        latency_ms: Some(cache_latency),
    });

    // Check scheduler
    checks.push(ComponentHealth {
        name: "rotation_scheduler".to_string(),
        status: HealthStatus::Healthy,
        message: Some("operational".to_string()),
        latency_ms: None,
    });

    let uptime = state.start_time.elapsed().as_secs();

    let response = HealthResponse {
        status: overall_status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: uptime,
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks,
    };

    (overall_status.status_code(), Json(response))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_as_str() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(HealthStatus::Degraded.as_str(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.as_str(), "unhealthy");
    }

    #[test]
    fn test_health_status_code() {
        assert_eq!(HealthStatus::Healthy.status_code(), StatusCode::OK);
        assert_eq!(HealthStatus::Degraded.status_code(), StatusCode::OK);
        assert_eq!(
            HealthStatus::Unhealthy.status_code(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn test_health_checker() {
        let checker = HealthChecker::new();

        assert!(!checker.is_startup_complete());
        assert!(!checker.is_ready());

        checker.mark_startup_complete();
        assert!(checker.is_startup_complete());

        checker.mark_ready();
        assert!(checker.is_ready());

        checker.mark_not_ready();
        assert!(!checker.is_ready());
    }

    #[test]
    fn test_component_health_serialization() {
        let health = ComponentHealth {
            name: "test".to_string(),
            status: HealthStatus::Healthy,
            message: Some("ok".to_string()),
            latency_ms: Some(5),
        };

        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"status\":\"healthy\""));
    }
}
