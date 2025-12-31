use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use baram::coordinator::{CoordinatorConfig, CoordinatorServer};
use baram::crawler::distributed::DistributedRunner;
use baram::crawler::instance::InstanceConfig;
use baram::embedding::{Embedder, EmbeddingConfig};
use baram::scheduler::rotation::CrawlerInstance;

// ============================================================================
// Embedding Server Implementation
// ============================================================================

/// Shared state for embedding server
struct EmbeddingServerState {
    embedder: RwLock<Embedder>,
    model_name: String,
    ready: std::sync::atomic::AtomicBool,
}

/// Request for single text embedding
#[derive(Debug, Deserialize)]
struct EmbedRequest {
    text: String,
}

/// Request for batch text embedding
#[derive(Debug, Deserialize)]
struct BatchEmbedRequest {
    texts: Vec<String>,
}

/// Response for embedding requests
#[derive(Debug, Serialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
    dimension: usize,
}

/// Response for batch embedding requests
#[derive(Debug, Serialize)]
struct BatchEmbedResponse {
    embeddings: Vec<Vec<f32>>,
    count: usize,
    dimension: usize,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    model: String,
    ready: bool,
    device: String,
}

/// Error response
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Start the embedding server
pub async fn embedding_server(
    host: String,
    port: u16,
    model: String,
    max_seq_length: usize,
    batch_size: usize,
    use_gpu: bool,
) -> Result<()> {
    println!("Starting Embedding Server");
    println!("=========================");
    println!("  Host: {host}");
    println!("  Port: {port}");
    println!("  Model: {model}");
    println!("  Max Sequence Length: {max_seq_length}");
    println!("  Batch Size: {batch_size}");
    println!("  Use GPU: {use_gpu}");
    println!();

    // Initialize embedding model
    println!("Loading embedding model...");
    let config = EmbeddingConfig {
        model_id: model.clone(),
        embedding_dim: 1024, // multilingual-e5-large uses 1024 dimensions
        max_seq_length,
        use_gpu,
        batch_size,
        normalize: true,
    };

    let embedder = Embedder::from_pretrained(config).context("Failed to load embedding model")?;

    let device = if use_gpu {
        "cuda (if available)"
    } else {
        "cpu"
    };

    println!("Model loaded successfully!");
    println!("  Device: {device}");
    println!();

    // Create shared state
    let state = Arc::new(EmbeddingServerState {
        embedder: RwLock::new(embedder),
        model_name: model,
        ready: std::sync::atomic::AtomicBool::new(true),
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/embed", post(embed_handler))
        .route("/embed/batch", post(batch_embed_handler))
        .route("/", get(root_handler))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context(format!("Failed to bind to {addr}"))?;

    println!("Embedding server listening on http://{addr}");
    println!();
    println!("Endpoints:");
    println!("  GET  /health      - Health check");
    println!("  POST /embed       - Single text embedding");
    println!("  POST /embed/batch - Batch text embedding");
    println!();

    axum::serve(listener, app).await.context("Server error")?;

    Ok(())
}

/// Root handler - welcome message
async fn root_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "baram Embedding Server",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "health": "GET /health",
            "embed": "POST /embed",
            "batch_embed": "POST /embed/batch"
        }
    }))
}

/// Health check handler
async fn health_handler(State(state): State<Arc<EmbeddingServerState>>) -> Json<HealthResponse> {
    let ready = state.ready.load(std::sync::atomic::Ordering::Relaxed);
    Json(HealthResponse {
        status: if ready {
            "healthy".to_string()
        } else {
            "loading".to_string()
        },
        model: state.model_name.clone(),
        ready,
        device: "auto".to_string(),
    })
}

/// Single text embedding handler
async fn embed_handler(
    State(state): State<Arc<EmbeddingServerState>>,
    Json(request): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, (StatusCode, Json<ErrorResponse>)> {
    if request.text.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Text cannot be empty".to_string(),
            }),
        ));
    }

    let mut embedder = state.embedder.write().await;

    match embedder.embed(&request.text) {
        Ok(embedding) => {
            let dimension = embedding.len();
            Ok(Json(EmbedResponse {
                embedding,
                dimension,
            }))
        }
        Err(e) => {
            tracing::error!(error = %e, "Embedding failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Embedding failed: {e}"),
                }),
            ))
        }
    }
}

/// Batch text embedding handler
async fn batch_embed_handler(
    State(state): State<Arc<EmbeddingServerState>>,
    Json(request): Json<BatchEmbedRequest>,
) -> Result<Json<BatchEmbedResponse>, (StatusCode, Json<ErrorResponse>)> {
    if request.texts.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Texts array cannot be empty".to_string(),
            }),
        ));
    }

    if request.texts.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Maximum 100 texts per batch".to_string(),
            }),
        ));
    }

    let mut embedder = state.embedder.write().await;

    match embedder.embed_batch(&request.texts) {
        Ok(embeddings) => {
            let count = embeddings.len();
            let dimension = embeddings.first().map(|e| e.len()).unwrap_or(0);
            Ok(Json(BatchEmbedResponse {
                embeddings,
                count,
                dimension,
            }))
        }
        Err(e) => {
            tracing::error!(error = %e, "Batch embedding failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Batch embedding failed: {e}"),
                }),
            ))
        }
    }
}

// ============================================================================
// Distributed Crawler Implementation
// ============================================================================

/// Configuration parameters for distributed crawler
pub struct DistributedCrawlerParams {
    pub instance: String,
    pub coordinator: String,
    pub database: String,
    pub heartbeat_interval: u64,
    pub rps: f64,
    pub output: String,
    pub with_comments: bool,
    pub once: bool,
}

/// Start the distributed crawler
pub async fn distributed_crawler(params: DistributedCrawlerParams) -> Result<()> {
    let DistributedCrawlerParams {
        instance,
        coordinator,
        database,
        heartbeat_interval,
        rps,
        output,
        with_comments,
        once,
    } = params;
    println!("Starting Distributed Crawler");
    println!("============================");
    println!("  Instance ID: {instance}");
    println!("  Coordinator: {coordinator}");
    println!("  Database: {}...***", &database[..20.min(database.len())]);
    println!("  Heartbeat: {heartbeat_interval}s");
    println!("  Rate limit: {rps} req/s");
    println!("  Output: {output}");
    println!("  Comments: {with_comments}");
    println!("  Run once: {once}");
    println!();

    // Parse instance ID
    let instance_id = CrawlerInstance::from_id(&instance).map_err(|_| {
        anyhow::anyhow!("Invalid instance ID: {instance}. Valid: main, sub1, sub2")
    })?;

    // Create instance config
    let config = InstanceConfig::builder()
        .instance_id(instance_id)
        .coordinator_url(&coordinator)
        .database_url(&database)
        .heartbeat_interval_secs(heartbeat_interval)
        .requests_per_second(rps)
        .output_dir(&output)
        .include_comments(with_comments)
        .build()
        .context("Failed to build instance config")?;

    println!("{}", config.display());
    println!();

    // Create distributed runner with deduplication
    let runner = DistributedRunner::with_dedup(config)
        .await
        .context("Failed to create distributed runner")?;

    if once {
        // Run once mode: execute current slot and exit
        println!("Running in 'once' mode - executing current slot...");

        if let Some(slot) = runner
            .check_current_slot()
            .await
            .context("Failed to check current slot")?
        {
            println!(
                "Current slot: hour {} with categories {:?}",
                slot.hour, slot.categories
            );

            let result = runner.run_slot(&slot).await.context("Failed to run slot")?;

            println!("\nSlot Execution Complete");
            println!("=======================");
            println!("Hour: {}", result.hour);
            println!("Articles crawled: {}", result.articles_crawled);
            println!("Errors: {}", result.errors);
            println!("Categories: {:?}", result.categories);
            println!("Success rate: {:.1}%", result.success_rate() * 100.0);
        } else {
            println!("This instance is not scheduled for the current hour.");
            println!(
                "Use --instance to specify a different instance or wait for the scheduled slot."
            );
        }
    } else {
        // Continuous mode: start background tasks
        println!("Starting continuous distributed crawling...");
        println!("Press Ctrl+C to stop.\n");

        // Start the runner
        let handle = runner
            .start()
            .await
            .context("Failed to start distributed runner")?;

        // Get list of assigned slots for today
        match runner.get_my_slots().await {
            Ok(slots) => {
                if slots.is_empty() {
                    println!("No slots assigned for today.");
                } else {
                    println!("Assigned slots for today:");
                    for slot in &slots {
                        println!("  Hour {}: {:?}", slot.hour, slot.categories);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get assigned slots: {}", e);
            }
        }
        println!();

        // Wait for shutdown signal
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                println!("\nShutdown signal received, stopping...");
                handle.shutdown().await;
            }
            Err(e) => {
                tracing::error!("Failed to wait for Ctrl+C: {}", e);
            }
        }
    }

    println!("Distributed crawler stopped.");
    Ok(())
}

// ============================================================================
// Coordinator Server Implementation
// ============================================================================

/// Configuration parameters for coordinator server
pub struct CoordinatorParams {
    pub host: String,
    pub port: u16,
    pub heartbeat_timeout: u64,
    pub heartbeat_interval: u64,
    pub max_instances: usize,
    pub schedule_cache: Option<String>,
    pub enable_cors: bool,
    pub enable_logging: bool,
}

/// Start the coordinator server
pub async fn coordinator_server(params: CoordinatorParams) -> Result<()> {
    let CoordinatorParams {
        host,
        port,
        heartbeat_timeout,
        heartbeat_interval,
        max_instances,
        schedule_cache,
        enable_cors,
        enable_logging,
    } = params;

    println!("Starting Coordinator Server");
    println!("===========================");
    println!("  Host: {host}");
    println!("  Port: {port}");
    println!("  Heartbeat Timeout: {heartbeat_timeout}s");
    println!("  Heartbeat Interval: {heartbeat_interval}s");
    println!("  Max Instances: {max_instances}");
    println!(
        "  CORS: {}",
        if enable_cors { "enabled" } else { "disabled" }
    );
    println!(
        "  Request Logging: {}",
        if enable_logging {
            "enabled"
        } else {
            "disabled"
        }
    );
    if let Some(ref cache) = schedule_cache {
        println!("  Schedule Cache: {cache}");
    }
    println!();

    // Build bind address
    let bind_address = format!("{host}:{port}")
        .parse()
        .context("Invalid bind address")?;

    // Create coordinator configuration
    let config = CoordinatorConfig::builder()
        .bind_address(bind_address)
        .heartbeat_timeout_secs(heartbeat_timeout)
        .heartbeat_interval_secs(heartbeat_interval)
        .max_instances(max_instances)
        .enable_cors(enable_cors)
        .enable_request_logging(enable_logging);

    let config = if let Some(cache_path) = schedule_cache {
        config.schedule_cache_path(cache_path).build()?
    } else {
        config.build()?
    };

    // Create and start server
    let server = CoordinatorServer::new(config).context("Failed to create coordinator server")?;

    println!("{}", server.info().display());
    println!();
    println!("API Endpoints:");
    println!("  GET  /api/health              - Health check");
    println!("  GET  /metrics                 - Prometheus metrics endpoint");
    println!("  GET  /api/schedule/today      - Get today's schedule");
    println!("  GET  /api/schedule/tomorrow   - Get tomorrow's schedule");
    println!("  GET  /api/schedule/:date      - Get schedule by date (YYYY-MM-DD)");
    println!("  GET  /api/instances           - List all instances");
    println!("  GET  /api/instances/:id       - Get instance by ID");
    println!("  POST /api/instances/register  - Register new instance");
    println!("  POST /api/instances/heartbeat - Send heartbeat");
    println!("  GET  /api/stats               - Get coordinator stats");
    println!();
    println!("Coordinator server listening on http://{bind_address}");
    println!("Press Ctrl+C to stop.\n");

    // Start with graceful shutdown
    server
        .start_with_shutdown(async {
            match tokio::signal::ctrl_c().await {
                Ok(()) => {
                    tracing::info!("Shutdown signal received");
                }
                Err(e) => {
                    tracing::error!("Failed to wait for Ctrl+C: {}", e);
                }
            }
        })
        .await?;

    println!("Coordinator server stopped.");
    Ok(())
}
