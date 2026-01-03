//! Prometheus metrics for baram coordinator and crawler
//!
//! This module provides metrics tracking for:
//! - Coordinator: instance registration, heartbeats, errors
//! - Distributed Crawler: crawl duration, articles per category, dedup hits, pipeline stats
//!
//! # Usage
//!
//! Call `init_metrics()` at application startup to register all metrics.
//! If initialization fails, metrics operations become no-ops.

use prometheus::{
    register_counter, register_counter_vec, register_gauge, register_gauge_vec,
    register_histogram_vec, Counter, CounterVec, Encoder, Gauge, GaugeVec, HistogramVec,
    TextEncoder,
};
use std::sync::OnceLock;

// ============================================================================
// Metrics Storage
// ============================================================================

/// Container for all coordinator metrics
struct CoordinatorMetrics {
    registered_instances: Gauge,
    online_instances: Gauge,
    total_heartbeats: Counter,
    heartbeat_errors: Counter,
    articles_crawled: CounterVec,
    errors: CounterVec,
    api_requests: CounterVec,
    api_duration: HistogramVec,
}

/// Container for all crawler metrics
struct CrawlerMetrics {
    crawl_duration: HistogramVec,
    articles_per_category: CounterVec,
    dedup_hits: CounterVec,
    dedup_misses: CounterVec,
    pipeline_success: CounterVec,
    pipeline_failure: CounterVec,
    pipeline_skipped: CounterVec,
    slot_executions: CounterVec,
    slot_errors: CounterVec,
    current_hour: GaugeVec,
    is_crawling: GaugeVec,
}

/// Global storage for coordinator metrics
static COORDINATOR_METRICS: OnceLock<CoordinatorMetrics> = OnceLock::new();

/// Global storage for crawler metrics
static CRAWLER_METRICS: OnceLock<CrawlerMetrics> = OnceLock::new();

/// Flag to track if initialization was attempted
static METRICS_INIT_ATTEMPTED: OnceLock<bool> = OnceLock::new();

// ============================================================================
// Initialization
// ============================================================================

/// Initialize all Prometheus metrics
///
/// This function should be called once at application startup.
/// If metric registration fails, errors are logged and subsequent
/// metric operations become no-ops.
///
/// # Returns
///
/// `Ok(())` if all metrics were registered successfully,
/// `Err` with description if any registration failed.
///
/// # Example
///
/// ```ignore
/// if let Err(e) = baram::metrics::init_metrics() {
///     eprintln!("Warning: Metrics initialization failed: {}", e);
///     // Application can continue without metrics
/// }
/// ```
pub fn init_metrics() -> Result<(), Box<dyn std::error::Error>> {
    // Prevent double initialization
    if METRICS_INIT_ATTEMPTED.get().is_some() {
        return Ok(());
    }
    METRICS_INIT_ATTEMPTED.set(true).ok();

    // Register coordinator metrics
    let coordinator = CoordinatorMetrics {
        registered_instances: register_gauge!(
            "baram_coordinator_registered_instances",
            "Number of registered crawler instances"
        )?,
        online_instances: register_gauge!(
            "baram_coordinator_online_instances",
            "Number of currently online crawler instances"
        )?,
        total_heartbeats: register_counter!(
            "baram_coordinator_total_heartbeats",
            "Total number of heartbeats received"
        )?,
        heartbeat_errors: register_counter!(
            "baram_coordinator_heartbeat_errors_total",
            "Total number of heartbeat errors"
        )?,
        articles_crawled: register_counter_vec!(
            "baram_coordinator_articles_crawled_total",
            "Total articles crawled by instance",
            &["instance"]
        )?,
        errors: register_counter_vec!(
            "baram_coordinator_errors_total",
            "Total errors reported by instance",
            &["instance"]
        )?,
        api_requests: register_counter_vec!(
            "baram_coordinator_api_requests_total",
            "Total API requests by endpoint and status",
            &["endpoint", "status"]
        )?,
        api_duration: register_histogram_vec!(
            "baram_coordinator_api_request_duration_seconds",
            "API request duration in seconds",
            &["endpoint"],
            vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
        )?,
    };

    // Register crawler metrics
    let crawler = CrawlerMetrics {
        crawl_duration: register_histogram_vec!(
            "baram_crawler_crawl_duration_seconds",
            "Time spent crawling a category in seconds",
            &["instance", "category"],
            vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0]
        )?,
        articles_per_category: register_counter_vec!(
            "baram_crawler_articles_per_category_total",
            "Total articles crawled per category",
            &["instance", "category"]
        )?,
        dedup_hits: register_counter_vec!(
            "baram_crawler_dedup_hits_total",
            "Total deduplication cache hits (URLs already crawled)",
            &["instance"]
        )?,
        dedup_misses: register_counter_vec!(
            "baram_crawler_dedup_misses_total",
            "Total deduplication cache misses (new URLs)",
            &["instance"]
        )?,
        pipeline_success: register_counter_vec!(
            "baram_crawler_pipeline_success_total",
            "Total successful pipeline executions",
            &["instance", "category"]
        )?,
        pipeline_failure: register_counter_vec!(
            "baram_crawler_pipeline_failure_total",
            "Total failed pipeline executions",
            &["instance", "category"]
        )?,
        pipeline_skipped: register_counter_vec!(
            "baram_crawler_pipeline_skipped_total",
            "Total skipped articles in pipeline",
            &["instance", "category"]
        )?,
        slot_executions: register_counter_vec!(
            "baram_crawler_slot_executions_total",
            "Total slot executions",
            &["instance", "hour"]
        )?,
        slot_errors: register_counter_vec!(
            "baram_crawler_slot_errors_total",
            "Total slot execution errors",
            &["instance", "hour"]
        )?,
        current_hour: register_gauge_vec!(
            "baram_crawler_current_hour",
            "Current hour being crawled (0-23)",
            &["instance"]
        )?,
        is_crawling: register_gauge_vec!(
            "baram_crawler_is_crawling",
            "Whether the crawler is currently crawling (1 = yes, 0 = no)",
            &["instance"]
        )?,
    };

    // Store metrics - these should always succeed since we just created them
    COORDINATOR_METRICS.set(coordinator).map_err(|_| "Coordinator metrics already initialized")?;
    CRAWLER_METRICS.set(crawler).map_err(|_| "Crawler metrics already initialized")?;

    tracing::info!("Prometheus metrics initialized successfully");
    Ok(())
}

/// Check if metrics have been initialized
pub fn metrics_initialized() -> bool {
    COORDINATOR_METRICS.get().is_some() && CRAWLER_METRICS.get().is_some()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Encode all metrics to Prometheus text format
pub fn encode_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

/// Update coordinator instance metrics
pub fn update_coordinator_instance_metrics(registered: usize, online: usize) {
    if let Some(m) = COORDINATOR_METRICS.get() {
        m.registered_instances.set(registered as f64);
        m.online_instances.set(online as f64);
    }
}

/// Record a heartbeat
pub fn record_heartbeat(instance: &str, articles: u64, errors: u64) {
    let Some(m) = COORDINATOR_METRICS.get() else {
        return;
    };

    m.total_heartbeats.inc();

    if articles > 0 {
        m.articles_crawled
            .with_label_values(&[instance])
            .inc_by(articles as f64);
    }

    if errors > 0 {
        m.errors
            .with_label_values(&[instance])
            .inc_by(errors as f64);
    }
}

/// Record a heartbeat error
pub fn record_heartbeat_error() {
    if let Some(m) = COORDINATOR_METRICS.get() {
        m.heartbeat_errors.inc();
    }
}

/// Record API request
pub fn record_api_request(endpoint: &str, status: u16, duration_secs: f64) {
    let Some(m) = COORDINATOR_METRICS.get() else {
        return;
    };

    let status_str = status.to_string();
    m.api_requests
        .with_label_values(&[endpoint, &status_str])
        .inc();
    m.api_duration
        .with_label_values(&[endpoint])
        .observe(duration_secs);
}

/// Histogram timer guard that records duration on drop
pub struct MetricsTimer {
    timer: Option<prometheus::HistogramTimer>,
}

impl MetricsTimer {
    fn new(timer: prometheus::HistogramTimer) -> Self {
        Self { timer: Some(timer) }
    }

    /// Create a no-op timer when metrics are not initialized
    fn noop() -> Self {
        Self { timer: None }
    }
}

impl Drop for MetricsTimer {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.stop_and_record();
        }
    }
}

/// Start a crawl timer (returns a timer handle)
pub fn start_crawl_timer(instance: &str, category: &str) -> MetricsTimer {
    match CRAWLER_METRICS.get() {
        Some(m) => MetricsTimer::new(
            m.crawl_duration
                .with_label_values(&[instance, category])
                .start_timer(),
        ),
        None => MetricsTimer::noop(),
    }
}

/// Record articles crawled for a category
pub fn record_articles_crawled(instance: &str, category: &str, count: u64) {
    if let Some(m) = CRAWLER_METRICS.get() {
        m.articles_per_category
            .with_label_values(&[instance, category])
            .inc_by(count as f64);
    }
}

/// Record deduplication results
pub fn record_dedup_results(instance: &str, new_urls: usize, existing_urls: usize) {
    let Some(m) = CRAWLER_METRICS.get() else {
        return;
    };

    if new_urls > 0 {
        m.dedup_misses
            .with_label_values(&[instance])
            .inc_by(new_urls as f64);
    }
    if existing_urls > 0 {
        m.dedup_hits
            .with_label_values(&[instance])
            .inc_by(existing_urls as f64);
    }
}

/// Record pipeline execution results
pub fn record_pipeline_results(
    instance: &str,
    category: &str,
    success: u64,
    failed: u64,
    skipped: u64,
) {
    let Some(m) = CRAWLER_METRICS.get() else {
        return;
    };

    if success > 0 {
        m.pipeline_success
            .with_label_values(&[instance, category])
            .inc_by(success as f64);
    }
    if failed > 0 {
        m.pipeline_failure
            .with_label_values(&[instance, category])
            .inc_by(failed as f64);
    }
    if skipped > 0 {
        m.pipeline_skipped
            .with_label_values(&[instance, category])
            .inc_by(skipped as f64);
    }
}

/// Record slot execution
pub fn record_slot_execution(instance: &str, hour: u8, had_errors: bool) {
    let Some(m) = CRAWLER_METRICS.get() else {
        return;
    };

    let hour_str = hour.to_string();
    m.slot_executions
        .with_label_values(&[instance, &hour_str])
        .inc();

    if had_errors {
        m.slot_errors
            .with_label_values(&[instance, &hour_str])
            .inc();
    }
}

/// Update crawler state
pub fn update_crawler_state(instance: &str, is_crawling: bool, current_hour: Option<u8>) {
    let Some(m) = CRAWLER_METRICS.get() else {
        return;
    };

    m.is_crawling
        .with_label_values(&[instance])
        .set(if is_crawling { 1.0 } else { 0.0 });

    if let Some(hour) = current_hour {
        m.current_hour
            .with_label_values(&[instance])
            .set(hour as f64);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_metrics_initialized() {
        // Initialize metrics if not already done
        let _ = init_metrics();
    }

    #[test]
    fn test_init_metrics() {
        // Should succeed or return Ok if already initialized
        let result = init_metrics();
        assert!(result.is_ok());

        // Second call should also be Ok (idempotent)
        let result2 = init_metrics();
        assert!(result2.is_ok());
    }

    #[test]
    fn test_metrics_initialized() {
        ensure_metrics_initialized();
        assert!(metrics_initialized());
    }

    #[test]
    fn test_encode_metrics() {
        ensure_metrics_initialized();
        let result = encode_metrics();
        assert!(result.is_ok());
        let text = result.unwrap();
        // After initialization, we should see our metrics
        assert!(text.contains("baram_") || text.is_empty());
    }

    #[test]
    fn test_coordinator_metrics() {
        ensure_metrics_initialized();
        update_coordinator_instance_metrics(3, 2);
        // Verify it doesn't panic
    }

    #[test]
    fn test_heartbeat_recording() {
        ensure_metrics_initialized();
        record_heartbeat("main", 10, 2);
        // Verify it doesn't panic
    }

    #[test]
    fn test_api_request_recording() {
        ensure_metrics_initialized();
        record_api_request("/api/health", 200, 0.005);
        // Verify it doesn't panic
    }

    #[test]
    fn test_crawler_metrics() {
        ensure_metrics_initialized();
        record_articles_crawled("main", "politics", 50);
        record_dedup_results("main", 100, 50);
        record_pipeline_results("main", "politics", 90, 5, 5);
        record_slot_execution("main", 14, false);
        update_crawler_state("main", true, Some(14));
        // Verify it doesn't panic
    }

    #[test]
    fn test_crawl_timer() {
        ensure_metrics_initialized();
        let _timer = start_crawl_timer("main", "politics");
        // Timer should record duration when dropped
    }

    #[test]
    fn test_metrics_noop_without_init() {
        // These should not panic even if called before initialization
        // (in a fresh test environment where init hasn't been called)
        update_coordinator_instance_metrics(1, 1);
        record_heartbeat("test", 1, 0);
        record_api_request("/test", 200, 0.001);
        record_articles_crawled("test", "test", 1);
        record_dedup_results("test", 1, 1);
        record_pipeline_results("test", "test", 1, 0, 0);
        record_slot_execution("test", 0, false);
        update_crawler_state("test", false, None);
        let _timer = start_crawl_timer("test", "test");
    }
}
