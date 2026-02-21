use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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
    embedder: Embedder,
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
        embedder,
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

    match state.embedder.embed(&request.text) {
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

    match state.embedder.embed_batch(&request.texts) {
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
// REST API Server Implementation
// ============================================================================

/// Shared state for the API server
struct ApiServerState {
    store: baram::embedding::VectorStore,
    embedding_server_url: String,
    http_client: reqwest::Client,
}

/// Query parameters for the search endpoint
#[derive(Debug, Deserialize)]
struct SearchQuery {
    /// Search query text
    q: String,

    /// Search mode: hybrid (default), keyword/bm25, vector/knn
    #[serde(default = "default_search_mode")]
    mode: String,

    /// Number of results to return
    #[serde(default = "default_k")]
    k: usize,

    /// Minimum score threshold
    threshold: Option<f32>,

    /// Filter by category
    category: Option<String>,

    /// Filter by date range start (ISO 8601)
    date_from: Option<String>,

    /// Filter by date range end (ISO 8601)
    date_to: Option<String>,
}

fn default_search_mode() -> String {
    "hybrid".to_string()
}

fn default_k() -> usize {
    10
}

/// API search response
#[derive(Debug, Serialize)]
struct ApiSearchResponse {
    query: String,
    mode: String,
    total: usize,
    results: Vec<baram::embedding::SearchResult>,
}

/// API health response
#[derive(Debug, Serialize)]
struct ApiHealthResponse {
    status: String,
    service: String,
    version: String,
    opensearch_connected: bool,
    document_count: Option<usize>,
}

/// API error response
#[derive(Debug, Serialize)]
struct ApiErrorResponse {
    error: String,
    code: u16,
}

/// Fetch a query embedding from the embedding server
async fn fetch_query_embedding(
    client: &reqwest::Client,
    embedding_url: &str,
    text: &str,
) -> Result<Vec<f32>, (StatusCode, Json<ApiErrorResponse>)> {
    let resp = client
        .post(format!("{embedding_url}/embed"))
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to connect to embedding server at {}", embedding_url);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiErrorResponse {
                    error: format!(
                        "Embedding server unavailable at {embedding_url}: {e}. \
                         Start it with: baram embedding-server"
                    ),
                    code: 503,
                }),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::error!(status = %status, body = %body, "Embedding server error");
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiErrorResponse {
                error: format!("Embedding server error ({status}): {body}"),
                code: 502,
            }),
        ));
    }

    let resp_json: serde_json::Value = resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiErrorResponse {
                error: format!("Failed to parse embedding response: {e}"),
                code: 502,
            }),
        )
    })?;

    let embedding: Vec<f32> = resp_json["embedding"]
        .as_array()
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiErrorResponse {
                    error: "No 'embedding' field in embedding server response".to_string(),
                    code: 502,
                }),
            )
        })?
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

    if embedding.is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiErrorResponse {
                error: "Embedding server returned empty vector".to_string(),
                code: 502,
            }),
        ));
    }

    Ok(embedding)
}

/// GET /api/search — Search articles with hybrid/keyword/vector modes
async fn api_search_handler(
    State(state): State<Arc<ApiServerState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<ApiSearchResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    if params.q.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse {
                error: "Query parameter 'q' cannot be empty".to_string(),
                code: 400,
            }),
        ));
    }

    let k = params.k.min(100); // Cap at 100 results

    let search_config = baram::embedding::SearchConfig {
        k,
        min_score: params.threshold,
        category: params.category.clone(),
        date_from: params.date_from.clone(),
        date_to: params.date_to.clone(),
        include_highlights: true,
        ..Default::default()
    };

    let mode = params.mode.as_str();
    let results = match mode {
        "keyword" | "bm25" => {
            tracing::info!(query = %params.q, mode = "bm25", k = k, "API: BM25 search");
            state
                .store
                .search_bm25(&params.q, &search_config)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "BM25 search failed");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiErrorResponse {
                            error: format!("Search failed: {e}"),
                            code: 500,
                        }),
                    )
                })?
        }
        "vector" | "knn" => {
            tracing::info!(query = %params.q, mode = "knn", k = k, "API: kNN vector search");
            let query_vector =
                fetch_query_embedding(&state.http_client, &state.embedding_server_url, &params.q)
                    .await?;
            state
                .store
                .search_knn(&query_vector, &search_config)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "kNN search failed");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiErrorResponse {
                            error: format!("Search failed: {e}"),
                            code: 500,
                        }),
                    )
                })?
        }
        "hybrid" => {
            tracing::info!(query = %params.q, mode = "hybrid", k = k, "API: Hybrid search");
            let query_vector =
                fetch_query_embedding(&state.http_client, &state.embedding_server_url, &params.q)
                    .await?;
            state
                .store
                .search_hybrid(&params.q, &query_vector, &search_config)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "Hybrid search failed");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiErrorResponse {
                            error: format!("Search failed: {e}"),
                            code: 500,
                        }),
                    )
                })?
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse {
                    error: format!(
                        "Unknown search mode: '{other}'. Valid: hybrid, keyword, bm25, vector, knn"
                    ),
                    code: 400,
                }),
            ));
        }
    };

    let total = results.len();
    tracing::info!(query = %params.q, mode = mode, total = total, "Search completed");

    Ok(Json(ApiSearchResponse {
        query: params.q,
        mode: mode.to_string(),
        total,
        results,
    }))
}

/// GET /api/health — Health check with OpenSearch connectivity
async fn api_health_handler(
    State(state): State<Arc<ApiServerState>>,
) -> Json<ApiHealthResponse> {
    let (connected, count) = match state.store.count().await {
        Ok(c) => (true, Some(c)),
        Err(e) => {
            tracing::warn!(error = %e, "OpenSearch health check failed");
            (false, None)
        }
    };

    Json(ApiHealthResponse {
        status: if connected {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
        service: "baram API Server".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        opensearch_connected: connected,
        document_count: count,
    })
}

/// GET / — API root with endpoint listing
async fn api_root_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "baram API Server",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "health": "GET /api/health",
            "search": "GET /api/search?q=<query>&mode=hybrid|keyword|vector&k=10&category=...&date_from=...&date_to=...",
        }
    }))
}

/// Start the REST API server (`baram serve`)
pub async fn api_server(host: String, port: u16) -> Result<()> {
    tracing::info!(host = %host, port = %port, "Starting Baram API server");

    let opensearch_url = std::env::var("OPENSEARCH_URL")
        .unwrap_or_else(|_| "http://localhost:9200".to_string());
    let opensearch_index = std::env::var("OPENSEARCH_INDEX")
        .unwrap_or_else(|_| "baram-articles".to_string());
    let embedding_server_url = std::env::var("EMBEDDING_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string());

    let opensearch_config = baram::config::OpenSearchConfig {
        url: opensearch_url.clone(),
        index_name: opensearch_index.clone(),
        username: std::env::var("OPENSEARCH_USER").ok(),
        password: std::env::var("OPENSEARCH_PASSWORD").ok(),
    };

    let store = baram::embedding::VectorStore::new(&opensearch_config)
        .context("Failed to connect to OpenSearch")?;

    // Verify connectivity
    match store.count().await {
        Ok(count) => {
            tracing::info!(
                index = %opensearch_index,
                count = count,
                "Connected to OpenSearch"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "OpenSearch connectivity check failed — server will start but search may not work"
            );
        }
    }

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;

    let state = Arc::new(ApiServerState {
        store,
        embedding_server_url: embedding_server_url.clone(),
        http_client,
    });

    let app = Router::new()
        .route("/", get(api_root_handler))
        .route("/api/health", get(api_health_handler))
        .route("/api/search", get(api_search_handler))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context(format!("Failed to bind to {addr}"))?;

    tracing::info!(
        addr = %addr,
        opensearch = %opensearch_url,
        embedding = %embedding_server_url,
        "Baram API server listening"
    );

    println!("Baram API Server");
    println!("================");
    println!("  Listen: http://{addr}");
    println!("  OpenSearch: {opensearch_url} (index: {opensearch_index})");
    println!("  Embedding: {embedding_server_url}");
    println!();
    println!("Endpoints:");
    println!("  GET  /              - API info");
    println!("  GET  /api/health    - Health check");
    println!("  GET  /api/search    - Search articles");
    println!("    ?q=<query>            Search query (required)");
    println!("    &mode=hybrid          hybrid (default), keyword/bm25, vector/knn");
    println!("    &k=10                 Number of results (default: 10, max: 100)");
    println!("    &threshold=0.5        Minimum score threshold");
    println!("    &category=politics    Filter by category");
    println!("    &date_from=2026-01-01 Filter by start date");
    println!("    &date_to=2026-02-21   Filter by end date");
    println!();

    axum::serve(listener, app).await.context("API server error")?;

    Ok(())
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

    // Initialize Prometheus metrics for crawler
    if let Err(e) = baram::metrics::init_metrics() {
        tracing::warn!(
            "Failed to initialize metrics (metrics will be disabled): {}",
            e
        );
    } else {
        tracing::info!("Prometheus metrics initialized for crawler");
    }

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
    let instance_id = CrawlerInstance::from_id(&instance)
        .map_err(|_| anyhow::anyhow!("Invalid instance ID: {instance}. Valid: main, sub1, sub2"))?;

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

    // Initialize Prometheus metrics
    if let Err(e) = baram::metrics::init_metrics() {
        tracing::warn!(
            "Failed to initialize metrics (metrics will be disabled): {}",
            e
        );
    } else {
        tracing::info!("Prometheus metrics initialized");
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_search_mode() {
        assert_eq!(default_search_mode(), "hybrid");
    }

    #[test]
    fn test_default_k() {
        assert_eq!(default_k(), 10);
    }

    #[test]
    fn test_search_query_deserialization() {
        let json = serde_json::json!({
            "q": "test query",
            "mode": "keyword",
            "k": 5,
            "category": "politics"
        });

        let query: SearchQuery = serde_json::from_value(json).expect("should deserialize");
        assert_eq!(query.q, "test query");
        assert_eq!(query.mode, "keyword");
        assert_eq!(query.k, 5);
        assert_eq!(query.category.as_deref(), Some("politics"));
        assert!(query.threshold.is_none());
        assert!(query.date_from.is_none());
        assert!(query.date_to.is_none());
    }

    #[test]
    fn test_search_query_defaults() {
        let json = serde_json::json!({ "q": "test" });

        let query: SearchQuery = serde_json::from_value(json).expect("should deserialize");
        assert_eq!(query.q, "test");
        assert_eq!(query.mode, "hybrid");
        assert_eq!(query.k, 10);
    }

    #[test]
    fn test_api_search_response_serialization() {
        let response = ApiSearchResponse {
            query: "test".to_string(),
            mode: "hybrid".to_string(),
            total: 0,
            results: vec![],
        };
        let json = serde_json::to_value(&response).expect("should serialize");
        assert_eq!(json["query"], "test");
        assert_eq!(json["mode"], "hybrid");
        assert_eq!(json["total"], 0);
    }

    #[test]
    fn test_api_error_response_serialization() {
        let response = ApiErrorResponse {
            error: "not found".to_string(),
            code: 404,
        };
        let json = serde_json::to_value(&response).expect("should serialize");
        assert_eq!(json["error"], "not found");
        assert_eq!(json["code"], 404);
    }

    #[test]
    fn test_valid_search_modes() {
        let valid_modes = ["keyword", "bm25", "vector", "knn", "hybrid"];
        for mode in &valid_modes {
            assert!(
                matches!(
                    *mode,
                    "keyword" | "bm25" | "vector" | "knn" | "hybrid"
                ),
                "Mode '{mode}' should be valid"
            );
        }
    }
}
