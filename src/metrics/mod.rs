//! Prometheus metrics for nTimes coordinator and crawler
//!
//! This module provides metrics tracking for:
//! - Coordinator: instance registration, heartbeats, errors
//! - Distributed Crawler: crawl duration, articles per category, dedup hits, pipeline stats

use lazy_static::lazy_static;
use prometheus::{
    register_counter, register_counter_vec, register_gauge, register_gauge_vec,
    register_histogram_vec, Counter, CounterVec, Encoder, Gauge, GaugeVec, HistogramVec,
    TextEncoder,
};

// ============================================================================
// Coordinator Metrics
// ============================================================================

lazy_static! {
    // Instance metrics
    pub static ref COORDINATOR_REGISTERED_INSTANCES: Gauge =
        register_gauge!(
            "ntimes_coordinator_registered_instances",
            "Number of registered crawler instances"
        ).unwrap();

    pub static ref COORDINATOR_ONLINE_INSTANCES: Gauge =
        register_gauge!(
            "ntimes_coordinator_online_instances",
            "Number of currently online crawler instances"
        ).unwrap();

    pub static ref COORDINATOR_TOTAL_HEARTBEATS: Counter =
        register_counter!(
            "ntimes_coordinator_total_heartbeats",
            "Total number of heartbeats received"
        ).unwrap();

    pub static ref COORDINATOR_HEARTBEAT_ERRORS: Counter =
        register_counter!(
            "ntimes_coordinator_heartbeat_errors_total",
            "Total number of heartbeat errors"
        ).unwrap();

    // Article tracking from all instances
    pub static ref COORDINATOR_ARTICLES_CRAWLED: CounterVec =
        register_counter_vec!(
            "ntimes_coordinator_articles_crawled_total",
            "Total articles crawled by instance",
            &["instance"]
        ).unwrap();

    pub static ref COORDINATOR_ERRORS: CounterVec =
        register_counter_vec!(
            "ntimes_coordinator_errors_total",
            "Total errors reported by instance",
            &["instance"]
        ).unwrap();

    // API request metrics
    pub static ref COORDINATOR_API_REQUESTS: CounterVec =
        register_counter_vec!(
            "ntimes_coordinator_api_requests_total",
            "Total API requests by endpoint and status",
            &["endpoint", "status"]
        ).unwrap();

    pub static ref COORDINATOR_API_DURATION: HistogramVec =
        register_histogram_vec!(
            "ntimes_coordinator_api_request_duration_seconds",
            "API request duration in seconds",
            &["endpoint"],
            vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
        ).unwrap();
}

// ============================================================================
// Distributed Crawler Metrics
// ============================================================================

lazy_static! {
    // Crawl execution metrics
    pub static ref CRAWLER_CRAWL_DURATION: HistogramVec =
        register_histogram_vec!(
            "ntimes_crawler_crawl_duration_seconds",
            "Time spent crawling a category in seconds",
            &["instance", "category"],
            vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0]
        ).unwrap();

    pub static ref CRAWLER_ARTICLES_PER_CATEGORY: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_articles_per_category_total",
            "Total articles crawled per category",
            &["instance", "category"]
        ).unwrap();

    pub static ref CRAWLER_DEDUP_HITS: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_dedup_hits_total",
            "Total deduplication cache hits (URLs already crawled)",
            &["instance"]
        ).unwrap();

    pub static ref CRAWLER_DEDUP_MISSES: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_dedup_misses_total",
            "Total deduplication cache misses (new URLs)",
            &["instance"]
        ).unwrap();

    // Pipeline metrics
    pub static ref CRAWLER_PIPELINE_SUCCESS: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_pipeline_success_total",
            "Total successful pipeline executions",
            &["instance", "category"]
        ).unwrap();

    pub static ref CRAWLER_PIPELINE_FAILURE: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_pipeline_failure_total",
            "Total failed pipeline executions",
            &["instance", "category"]
        ).unwrap();

    pub static ref CRAWLER_PIPELINE_SKIPPED: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_pipeline_skipped_total",
            "Total skipped articles in pipeline",
            &["instance", "category"]
        ).unwrap();

    // Slot execution metrics
    pub static ref CRAWLER_SLOT_EXECUTIONS: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_slot_executions_total",
            "Total slot executions",
            &["instance", "hour"]
        ).unwrap();

    pub static ref CRAWLER_SLOT_ERRORS: CounterVec =
        register_counter_vec!(
            "ntimes_crawler_slot_errors_total",
            "Total slot execution errors",
            &["instance", "hour"]
        ).unwrap();

    // Current state gauges
    pub static ref CRAWLER_CURRENT_HOUR: GaugeVec =
        register_gauge_vec!(
            "ntimes_crawler_current_hour",
            "Current hour being crawled (0-23)",
            &["instance"]
        ).unwrap();

    pub static ref CRAWLER_IS_CRAWLING: GaugeVec =
        register_gauge_vec!(
            "ntimes_crawler_is_crawling",
            "Whether the crawler is currently crawling (1 = yes, 0 = no)",
            &["instance"]
        ).unwrap();
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
    COORDINATOR_REGISTERED_INSTANCES.set(registered as f64);
    COORDINATOR_ONLINE_INSTANCES.set(online as f64);
}

/// Record a heartbeat
pub fn record_heartbeat(instance: &str, articles: u64, errors: u64) {
    COORDINATOR_TOTAL_HEARTBEATS.inc();

    if articles > 0 {
        COORDINATOR_ARTICLES_CRAWLED
            .with_label_values(&[instance])
            .inc_by(articles as f64);
    }

    if errors > 0 {
        COORDINATOR_ERRORS
            .with_label_values(&[instance])
            .inc_by(errors as f64);
    }
}

/// Record API request
pub fn record_api_request(endpoint: &str, status: u16, duration_secs: f64) {
    let status_str = status.to_string();
    COORDINATOR_API_REQUESTS
        .with_label_values(&[endpoint, &status_str])
        .inc();
    COORDINATOR_API_DURATION
        .with_label_values(&[endpoint])
        .observe(duration_secs);
}

/// Start a crawl timer (returns a timer handle)
pub fn start_crawl_timer(instance: &str, category: &str) -> prometheus::HistogramTimer {
    CRAWLER_CRAWL_DURATION
        .with_label_values(&[instance, category])
        .start_timer()
}

/// Record articles crawled for a category
pub fn record_articles_crawled(instance: &str, category: &str, count: u64) {
    CRAWLER_ARTICLES_PER_CATEGORY
        .with_label_values(&[instance, category])
        .inc_by(count as f64);
}

/// Record deduplication results
pub fn record_dedup_results(instance: &str, new_urls: usize, existing_urls: usize) {
    if new_urls > 0 {
        CRAWLER_DEDUP_MISSES
            .with_label_values(&[instance])
            .inc_by(new_urls as f64);
    }
    if existing_urls > 0 {
        CRAWLER_DEDUP_HITS
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
    if success > 0 {
        CRAWLER_PIPELINE_SUCCESS
            .with_label_values(&[instance, category])
            .inc_by(success as f64);
    }
    if failed > 0 {
        CRAWLER_PIPELINE_FAILURE
            .with_label_values(&[instance, category])
            .inc_by(failed as f64);
    }
    if skipped > 0 {
        CRAWLER_PIPELINE_SKIPPED
            .with_label_values(&[instance, category])
            .inc_by(skipped as f64);
    }
}

/// Record slot execution
pub fn record_slot_execution(instance: &str, hour: u8, had_errors: bool) {
    let hour_str = hour.to_string();
    CRAWLER_SLOT_EXECUTIONS
        .with_label_values(&[instance, &hour_str])
        .inc();

    if had_errors {
        CRAWLER_SLOT_ERRORS
            .with_label_values(&[instance, &hour_str])
            .inc();
    }
}

/// Update crawler state
pub fn update_crawler_state(instance: &str, is_crawling: bool, current_hour: Option<u8>) {
    CRAWLER_IS_CRAWLING
        .with_label_values(&[instance])
        .set(if is_crawling { 1.0 } else { 0.0 });

    if let Some(hour) = current_hour {
        CRAWLER_CURRENT_HOUR
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

    #[test]
    fn test_encode_metrics() {
        let result = encode_metrics();
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("ntimes_"));
    }

    #[test]
    fn test_coordinator_metrics() {
        update_coordinator_instance_metrics(3, 2);
        assert_eq!(COORDINATOR_REGISTERED_INSTANCES.get(), 3.0);
        assert_eq!(COORDINATOR_ONLINE_INSTANCES.get(), 2.0);
    }

    #[test]
    fn test_heartbeat_recording() {
        let before = COORDINATOR_TOTAL_HEARTBEATS.get();
        record_heartbeat("main", 10, 2);
        assert!(COORDINATOR_TOTAL_HEARTBEATS.get() > before);
    }

    #[test]
    fn test_api_request_recording() {
        record_api_request("/api/health", 200, 0.005);
        // Verify it doesn't panic
    }

    #[test]
    fn test_crawler_metrics() {
        record_articles_crawled("main", "politics", 50);
        record_dedup_results("main", 100, 50);
        record_pipeline_results("main", "politics", 90, 5, 5);
        record_slot_execution("main", 14, false);
        update_crawler_state("main", true, Some(14));
        // Verify it doesn't panic
    }
}
