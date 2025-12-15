//! Coordinator server implementation
//!
//! This module provides the main server that orchestrates
//! all coordinator components.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::scheduler::rotation::RotationScheduler;
use crate::scheduler::schedule::ScheduleCache;
use crate::scheduler::trigger::ScheduleTrigger;

use super::api::create_router;
use super::config::CoordinatorConfig;
use super::registry::InstanceRegistry;

// ============================================================================
// App State
// ============================================================================

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Instance registry
    pub registry: Arc<InstanceRegistry>,

    /// Schedule cache
    pub cache: Arc<ScheduleCache>,

    /// Rotation scheduler
    pub scheduler: RotationScheduler,

    /// Schedule trigger
    pub trigger: Arc<ScheduleTrigger>,

    /// Server start time
    pub start_time: Instant,

    /// Configuration
    pub config: CoordinatorConfig,
}

// ============================================================================
// Coordinator Server
// ============================================================================

/// Main Coordinator server
pub struct CoordinatorServer {
    config: CoordinatorConfig,
    state: AppState,
}

impl CoordinatorServer {
    /// Create a new coordinator server
    pub fn new(config: CoordinatorConfig) -> Result<Self, ServerError> {
        config.validate().map_err(|e| ServerError::ConfigError(e.to_string()))?;

        // Create schedule cache
        let cache = Arc::new(match &config.schedule_cache_path {
            Some(path) => ScheduleCache::with_file(path),
            None => ScheduleCache::new(),
        });

        // Create registry
        let registry = Arc::new(InstanceRegistry::new(
            config.heartbeat_timeout_secs,
            config.max_instances,
        ));

        // Create scheduler
        let scheduler = RotationScheduler::new();

        // Create trigger
        let trigger = Arc::new(
            ScheduleTrigger::with_defaults(cache.clone())
                .map_err(|e| ServerError::InitError(e.to_string()))?,
        );

        let state = AppState {
            registry,
            cache,
            scheduler,
            trigger,
            start_time: Instant::now(),
            config: config.clone(),
        };

        Ok(Self { config, state })
    }

    /// Get the application state
    pub fn state(&self) -> AppState {
        self.state.clone()
    }

    /// Build the router with all routes
    pub fn build_router(&self) -> Router {
        let mut router = create_router(self.state.clone());

        // Add CORS layer if enabled
        if self.config.enable_cors {
            router = router.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        // Add tracing layer if enabled
        if self.config.enable_request_logging {
            router = router.layer(TraceLayer::new_for_http());
        }

        router
    }

    /// Start the server
    pub async fn start(&self) -> Result<(), ServerError> {
        let router = self.build_router();
        let addr = self.config.bind_address;

        tracing::info!("Starting Coordinator server on {}", addr);

        // Start background tasks
        self.start_background_tasks();

        // Start the HTTP server
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?;

        axum::serve(listener, router)
            .await
            .map_err(|e| ServerError::ServeError(e.to_string()))?;

        Ok(())
    }

    /// Start with graceful shutdown
    pub async fn start_with_shutdown(
        &self,
        shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), ServerError> {
        let router = self.build_router();
        let addr = self.config.bind_address;

        tracing::info!("Starting Coordinator server on {} (with graceful shutdown)", addr);

        // Start background tasks
        self.start_background_tasks();

        // Start the HTTP server with graceful shutdown
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?;

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| ServerError::ServeError(e.to_string()))?;

        tracing::info!("Coordinator server shutdown complete");
        Ok(())
    }

    /// Start background tasks
    fn start_background_tasks(&self) {
        // Start status updater (updates instance statuses every 10 seconds)
        let registry = self.state.registry.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                registry.update_statuses().await;
            }
        });

        tracing::info!("Background tasks started");
    }

    /// Get server info
    pub fn info(&self) -> ServerInfo {
        ServerInfo {
            bind_address: self.config.bind_address,
            heartbeat_timeout_secs: self.config.heartbeat_timeout_secs,
            max_instances: self.config.max_instances,
            cors_enabled: self.config.enable_cors,
            request_logging_enabled: self.config.enable_request_logging,
        }
    }
}

/// Server information
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub bind_address: SocketAddr,
    pub heartbeat_timeout_secs: u64,
    pub max_instances: usize,
    pub cors_enabled: bool,
    pub request_logging_enabled: bool,
}

impl ServerInfo {
    /// Format as display string
    pub fn display(&self) -> String {
        format!(
            "Coordinator Server\n\
             {:-<40}\n\
             Bind Address: {}\n\
             Heartbeat Timeout: {}s\n\
             Max Instances: {}\n\
             CORS: {}\n\
             Request Logging: {}",
            "",
            self.bind_address,
            self.heartbeat_timeout_secs,
            self.max_instances,
            if self.cors_enabled { "enabled" } else { "disabled" },
            if self.request_logging_enabled { "enabled" } else { "disabled" }
        )
    }
}

// ============================================================================
// Server Errors
// ============================================================================

/// Server errors
#[derive(Debug, Clone)]
pub enum ServerError {
    /// Configuration error
    ConfigError(String),

    /// Initialization error
    InitError(String),

    /// Failed to bind to address
    BindError(String),

    /// Server error
    ServeError(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            Self::InitError(msg) => write!(f, "Initialization error: {}", msg),
            Self::BindError(msg) => write!(f, "Failed to bind: {}", msg),
            Self::ServeError(msg) => write!(f, "Server error: {}", msg),
        }
    }
}

impl std::error::Error for ServerError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let config = CoordinatorConfig::default();
        let server = CoordinatorServer::new(config);
        assert!(server.is_ok());
    }

    #[test]
    fn test_server_info() {
        let config = CoordinatorConfig::default();
        let server = CoordinatorServer::new(config).unwrap();
        let info = server.info();

        assert_eq!(info.max_instances, 10);
        assert!(info.cors_enabled);
    }

    #[test]
    fn test_server_with_custom_config() {
        let config = CoordinatorConfig::builder()
            .heartbeat_timeout_secs(120)
            .max_instances(5)
            .enable_cors(false)
            .build()
            .unwrap();

        let server = CoordinatorServer::new(config).unwrap();
        let info = server.info();

        assert_eq!(info.heartbeat_timeout_secs, 120);
        assert_eq!(info.max_instances, 5);
        assert!(!info.cors_enabled);
    }

    #[tokio::test]
    async fn test_app_state_components() {
        let config = CoordinatorConfig::default();
        let server = CoordinatorServer::new(config).unwrap();
        let state = server.state();

        // Test registry
        let instances = state.registry.get_all_instances().await;
        assert!(instances.is_empty());

        // Test cache
        let cache_status = state.cache.status().await;
        assert!(!cache_status.has_schedule);
    }
}
