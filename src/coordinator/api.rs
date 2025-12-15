//! REST API handlers for the Coordinator server
//!
//! This module defines the API routes and handlers for the coordinator.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::scheduler::rotation::CrawlerInstance;
use crate::scheduler::schedule::DailySchedule;

use super::registry::{
    HeartbeatRequest, InstanceInfo, RegisterRequest, RegistryStats,
};
use super::server::AppState;

// ============================================================================
// API Response Types
// ============================================================================

/// Generic API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

impl<T: Serialize + Default> ApiResponse<T> {
    pub fn error_with_default(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: Some(T::default()),
            error: Some(message.into()),
        }
    }
}

/// Simple error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: message.into(),
        }
    }
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}

/// Schedule response
#[derive(Debug, Default, Serialize)]
pub struct ScheduleResponse {
    pub date: String,
    pub slots: Vec<SlotResponse>,
}

#[derive(Debug, Default, Serialize)]
pub struct SlotResponse {
    pub hour: u8,
    pub instance: String,
    pub categories: Vec<String>,
}

impl From<&DailySchedule> for ScheduleResponse {
    fn from(schedule: &DailySchedule) -> Self {
        Self {
            date: schedule.date.to_string(),
            slots: schedule
                .slots
                .iter()
                .map(|s| SlotResponse {
                    hour: s.hour,
                    instance: s.instance.id().to_string(),
                    categories: s.categories.iter().map(|c| c.id().to_string()).collect(),
                })
                .collect(),
        }
    }
}

/// Instance list response
#[derive(Debug, Serialize)]
pub struct InstancesResponse {
    pub instances: Vec<InstanceInfo>,
    pub stats: RegistryStats,
}

/// Override request
#[derive(Debug, Deserialize)]
pub struct OverrideRequest {
    pub hour: u8,
    pub instance: String,
}

// ============================================================================
// API Routes
// ============================================================================

/// Create the API router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health endpoints
        .route("/api/health", get(health_check))
        // Schedule endpoints
        .route("/api/schedule/today", get(get_today_schedule))
        .route("/api/schedule/tomorrow", get(get_tomorrow_schedule))
        .route("/api/schedule/:date", get(get_schedule_by_date))
        // Instance endpoints
        .route("/api/instances", get(list_instances))
        .route("/api/instances/:id", get(get_instance))
        .route("/api/instances/register", post(register_instance))
        .route("/api/instances/heartbeat", post(heartbeat))
        .route("/api/instances/:id/maintenance", post(set_maintenance))
        // Stats endpoints
        .route("/api/stats", get(get_stats))
        .with_state(state)
}

// ============================================================================
// Health Handlers
// ============================================================================

/// Health check endpoint
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();

    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: uptime,
    }))
}

// ============================================================================
// Schedule Handlers
// ============================================================================

/// Get today's schedule
async fn get_today_schedule(State(state): State<AppState>) -> impl IntoResponse {
    match state.trigger.get_current_schedule().await {
        Ok(schedule) => (StatusCode::OK, Json(ApiResponse::success(ScheduleResponse::from(&schedule)))),
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<ScheduleResponse>::error_with_default("Failed to get schedule")),
        ),
    }
}

/// Get tomorrow's schedule
async fn get_tomorrow_schedule(State(state): State<AppState>) -> impl IntoResponse {
    match state.trigger.generate_tomorrow_schedule().await {
        Ok(schedule) => (StatusCode::OK, Json(ApiResponse::success(ScheduleResponse::from(&schedule)))),
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<ScheduleResponse>::error_with_default("Failed to generate schedule")),
        ),
    }
}

/// Get schedule for a specific date
async fn get_schedule_by_date(
    State(state): State<AppState>,
    Path(date_str): Path<String>,
) -> impl IntoResponse {
    // Parse date
    let date = match chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<ScheduleResponse>::error_with_default(format!(
                    "Invalid date format: {}. Expected YYYY-MM-DD",
                    date_str
                ))),
            );
        }
    };

    // Generate schedule for the date
    let schedule = state.scheduler.generate_daily_schedule(date);

    (StatusCode::OK, Json(ApiResponse::success(ScheduleResponse::from(&schedule))))
}

// ============================================================================
// Instance Handlers
// ============================================================================

/// List all registered instances
async fn list_instances(State(state): State<AppState>) -> impl IntoResponse {
    let instances = state.registry.get_all_instances().await;
    let stats = state.registry.stats().await;

    Json(ApiResponse::success(InstancesResponse { instances, stats }))
}

/// Get a specific instance
async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    let instance = match CrawlerInstance::from_id(&id) {
        Ok(i) => i,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!("Invalid instance ID: {}", id))),
            ).into_response();
        }
    };

    match state.registry.get_instance(instance).await {
        Some(info) => (StatusCode::OK, Json(ApiResponse::success(info))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(format!("Instance not found: {}", id))),
        ).into_response(),
    }
}

/// Register a new instance
async fn register_instance(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> axum::response::Response {
    match state.registry.register(request).await {
        Ok(response) => (StatusCode::OK, Json(ApiResponse::success(response))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e.to_string())),
        ).into_response(),
    }
}

/// Process heartbeat from instance
async fn heartbeat(
    State(state): State<AppState>,
    Json(request): Json<HeartbeatRequest>,
) -> axum::response::Response {
    match state.registry.heartbeat(request).await {
        Ok(response) => (StatusCode::OK, Json(ApiResponse::success(response))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e.to_string())),
        ).into_response(),
    }
}

/// Set maintenance mode for an instance
#[derive(Debug, Deserialize)]
pub struct MaintenanceRequest {
    pub enabled: bool,
}

async fn set_maintenance(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<MaintenanceRequest>,
) -> axum::response::Response {
    let instance = match CrawlerInstance::from_id(&id) {
        Ok(i) => i,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!("Invalid instance ID: {}", id))),
            ).into_response();
        }
    };

    match state.registry.set_maintenance(instance, request.enabled).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success(format!(
                "Maintenance mode {} for {}",
                if request.enabled { "enabled" } else { "disabled" },
                id
            ))),
        ).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(ErrorResponse::new(e.to_string()))).into_response(),
    }
}

// ============================================================================
// Stats Handlers
// ============================================================================

/// Get coordinator stats
async fn get_stats(State(state): State<AppState>) -> impl IntoResponse {
    let registry_stats = state.registry.stats().await;
    let cache_status = state.cache.status().await;

    #[derive(Serialize)]
    struct StatsResponse {
        registry: RegistryStats,
        cache_valid: bool,
        cache_has_schedule: bool,
        uptime_secs: u64,
    }

    Json(ApiResponse::success(StatsResponse {
        registry: registry_stats,
        cache_valid: cache_status.is_valid,
        cache_has_schedule: cache_status.has_schedule,
        uptime_secs: state.start_time.elapsed().as_secs(),
    }))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success("test data");
        assert!(response.success);
        assert!(response.data.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_error_response() {
        let response = ErrorResponse::new("test error");
        assert!(!response.success);
        assert_eq!(response.error, "test error");
    }

    #[test]
    fn test_schedule_response_from_daily_schedule() {
        use crate::scheduler::rotation::RotationScheduler;
        use chrono::NaiveDate;

        let scheduler = RotationScheduler::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let schedule = scheduler.generate_daily_schedule(date);

        let response = ScheduleResponse::from(&schedule);

        assert_eq!(response.date, "2024-01-15");
        assert_eq!(response.slots.len(), 24);
    }
}
