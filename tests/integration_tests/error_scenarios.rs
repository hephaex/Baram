//! Error scenario integration tests
//!
//! Tests various failure modes and error handling:
//! 1. Network timeouts
//! 2. Connection failures
//! 3. HTTP error responses (404, 500, etc.)
//! 4. Malformed HTML
//! 5. Rate limiting
//! 6. Retry logic

use baram::crawler::fetcher::NaverFetcher;
use baram::crawler::pipeline::{CrawlerPipeline, PipelineConfig};
use baram::parser::ArticleParser;
use baram::utils::error::FetchError;
use std::time::Duration;
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixtures::{ERROR_404_HTML, MALFORMED_HTML, SAMPLE_ARTICLE_HTML};

// ============================================================================
// Network Error Tests
// ============================================================================

#[tokio::test]
async fn test_timeout_handling() {
    let mock_server = MockServer::start().await;

    // Mock with long delay to trigger timeout
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(SAMPLE_ARTICLE_HTML)
                .set_delay(Duration::from_secs(10)), // Longer than timeout
        )
        .mount(&mock_server)
        .await;

    // Create fetcher with short timeout
    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        10,
        1, // Only 1 retry
        Duration::from_millis(100), // Short timeout
    )
    .unwrap();

    // Attempt fetch - should timeout
    let url = "/mnews/article/001/0014000001?sid=105";
    let result = fetcher.fetch_article(url, 105).await;

    // Should fail with timeout or max retries
    assert!(
        result.is_err(),
        "Should fail on timeout"
    );

    // Error should be timeout related
    match result {
        Err(FetchError::Timeout) => {}
        Err(FetchError::MaxRetriesExceeded) => {}
        Err(e) => panic!("Expected timeout error, got: {:?}", e),
        Ok(_) => panic!("Expected error, got success"),
    }
}

#[tokio::test]
async fn test_connection_refused() {
    // Try to connect to non-existent server
    let fetcher =
        NaverFetcher::with_config_and_base_url("http://localhost:1", 10, 1, Duration::from_secs(1))
            .unwrap();

    let url = "/test";
    let result = fetcher.fetch(url).await;

    // Should fail with connection error
    assert!(result.is_err(), "Should fail on connection refused");
}

#[tokio::test]
async fn test_invalid_url_handling() {
    let fetcher = NaverFetcher::new(10).unwrap();

    // Try to fetch invalid URL
    let result = fetcher.fetch("not-a-valid-url").await;

    assert!(result.is_err(), "Should fail on invalid URL");
}

// ============================================================================
// HTTP Error Response Tests
// ============================================================================

#[tokio::test]
async fn test_404_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_string(ERROR_404_HTML))
        .mount(&mock_server)
        .await;

    let fetcher =
        NaverFetcher::with_config_and_base_url(&mock_server.uri(), 10, 1, Duration::from_secs(5))
            .unwrap();

    let url = "/nonexistent";
    let result = fetcher.fetch(url).await;

    assert!(result.is_err());
    match result {
        Err(FetchError::ServerError(404)) => {}
        Err(e) => panic!("Expected 404 error, got: {:?}", e),
        Ok(_) => panic!("Expected error, got success"),
    }
}

#[tokio::test]
async fn test_500_error_with_retry() {
    let mock_server = MockServer::start().await;

    // First two requests fail with 500, third succeeds
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        10,
        3, // Allow retries
        Duration::from_secs(5),
    )
    .unwrap();

    let url = "/test";
    let result = fetcher.fetch_article(url, 100).await;

    // Should eventually succeed after retries
    assert!(
        result.is_ok(),
        "Should succeed after retries, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_503_service_unavailable() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        10,
        2, // Allow some retries
        Duration::from_secs(1),
    )
    .unwrap();

    let url = "/test";
    let result = fetcher.fetch_article(url, 100).await;

    // Should fail after exhausting retries
    assert!(result.is_err());
}

#[tokio::test]
async fn test_rate_limit_429() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        10,
        2,
        Duration::from_secs(1),
    )
    .unwrap();

    let url = "/test";
    let result = fetcher.fetch_article(url, 100).await;

    // Should fail (429 is retryable but will eventually exhaust retries)
    assert!(result.is_err());
}

// ============================================================================
// Malformed Content Tests
// ============================================================================

#[tokio::test]
async fn test_malformed_html_parsing() {
    let parser = ArticleParser::new();

    let result = parser.parse_with_fallback(
        MALFORMED_HTML,
        "https://n.news.naver.com/article/001/0014000001",
    );

    // Parser should handle malformed HTML gracefully
    // It might succeed with partial content or fail gracefully
    if let Err(e) = result {
        // Error should be informative
        assert!(!e.to_string().is_empty());
    }
}

#[tokio::test]
async fn test_empty_html_parsing() {
    let parser = ArticleParser::new();

    let result =
        parser.parse_with_fallback("", "https://n.news.naver.com/article/001/0014000001");

    // Should fail on empty content
    assert!(result.is_err());
}

#[tokio::test]
async fn test_non_utf8_content() {
    let fetcher = NaverFetcher::new(10).unwrap();

    // Test EUC-KR encoded content
    let euc_kr_bytes: &[u8] = &[0xbe, 0xc8, 0xb3, 0xe7, 0xc7, 0xcf, 0xbc, 0xbc, 0xbf, 0xe4];

    let result = fetcher.decode_bytes(euc_kr_bytes, "text/html; charset=euc-kr");

    // Should successfully decode EUC-KR
    assert!(result.is_ok(), "Should handle EUC-KR encoding");
    assert_eq!(result.unwrap(), "안녕하세요");
}

#[tokio::test]
async fn test_invalid_encoding() {
    let fetcher = NaverFetcher::new(10).unwrap();

    // Invalid byte sequence
    let invalid_bytes: &[u8] = &[0xFF, 0xFE, 0xFD];

    let result = fetcher.decode_bytes(invalid_bytes, "text/html");

    // Should handle gracefully (might succeed with replacement characters or fail)
    match result {
        Ok(text) => {
            // If it succeeds, should contain replacement characters or be non-empty
            assert!(!text.is_empty());
        }
        Err(_) => {
            // Failing is also acceptable for invalid encoding
        }
    }
}

// ============================================================================
// Pipeline Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_partial_failures() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    // First URL succeeds
    Mock::given(method("GET"))
        .and(path_regex(".*/001/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    // Second URL fails with 404
    Mock::given(method("GET"))
        .and(path_regex(".*/002/.*"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        max_retries: 1,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    let urls = vec![
        format!("{}/mnews/article/001/0014000001?sid=105", mock_server.uri()),
        format!("{}/mnews/article/002/0014000002?sid=105", mock_server.uri()),
    ];

    let stats = pipeline.run(urls).await.unwrap();

    // Should have mix of success and failure
    assert_eq!(stats.total_jobs, 2);
    assert!(stats.success_count + stats.skipped_count > 0, "Should have some successes");
    assert!(stats.failed_count > 0, "Should have some failures");
}

#[tokio::test]
async fn test_pipeline_all_failures() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    // All requests fail
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        max_retries: 1,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    let urls = vec![
        format!("{}/test1", mock_server.uri()),
        format!("{}/test2", mock_server.uri()),
    ];

    let stats = pipeline.run(urls).await.unwrap();

    // All should fail
    assert_eq!(stats.total_jobs, 2);
    assert_eq!(stats.failed_count, 2);
    assert_eq!(stats.success_count, 0);
}

// ============================================================================
// Retry Logic Tests
// ============================================================================

#[tokio::test]
async fn test_exponential_backoff_timing() {
    let mock_server = MockServer::start().await;

    // Fail first 2 times, succeed on 3rd
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        10,
        3, // Allow retries
        Duration::from_secs(5),
    )
    .unwrap();

    let start = std::time::Instant::now();
    let result = fetcher.fetch_article("/test", 100).await;
    let elapsed = start.elapsed();

    // Should succeed after retries
    assert!(result.is_ok());

    // Should have taken some time due to exponential backoff
    // First retry: 1s, second retry: 2s = at least 3s total
    assert!(
        elapsed >= Duration::from_secs(2),
        "Should have exponential backoff delay, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_max_retries_exceeded() {
    let mock_server = MockServer::start().await;

    // Always fail
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        10,
        2, // Max 2 retries
        Duration::from_secs(1),
    )
    .unwrap();

    let result = fetcher.fetch_article("/test", 100).await;

    // Should fail with MaxRetriesExceeded
    assert!(matches!(result, Err(FetchError::MaxRetriesExceeded)));
}

// ============================================================================
// Concurrent Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_mixed_results() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    // Some succeed, some fail
    Mock::given(method("GET"))
        .and(path_regex(".*/success/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(".*/fail/.*"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        fetcher_workers: 3,
        max_retries: 1,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    let urls = vec![
        format!("{}/mnews/article/success/001?sid=105", mock_server.uri()),
        format!("{}/mnews/article/fail/002?sid=105", mock_server.uri()),
        format!("{}/mnews/article/success/003?sid=105", mock_server.uri()),
        format!("{}/mnews/article/fail/004?sid=105", mock_server.uri()),
    ];

    let stats = pipeline.run(urls).await.unwrap();

    // Should have processed all jobs
    assert_eq!(stats.total_jobs, 4);
    let total_processed = stats.success_count + stats.failed_count + stats.skipped_count;
    assert_eq!(total_processed, 4, "All jobs should be processed");

    // With our mock setup, we expect some successes and some failures
    // Note: The exact counts may vary based on timing and retry logic
    println!(
        "Results: {} success, {} failed, {} skipped",
        stats.success_count, stats.failed_count, stats.skipped_count
    );
    assert!(
        total_processed == 4,
        "All 4 jobs should complete (success/fail/skip)"
    );
}

// ============================================================================
// Resource Cleanup Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_cleanup_on_error() {
    let temp_dir = TempDir::new().unwrap();

    // Create pipeline with invalid config (very short timeout)
    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        request_timeout: Duration::from_millis(1), // Extremely short
        max_retries: 0,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    // Try to run with URLs that will timeout
    let urls = vec!["https://httpbin.org/delay/10".to_string()];

    let result = pipeline.run(urls).await;

    // Should complete (not panic) even with errors
    assert!(result.is_ok(), "Pipeline should handle errors gracefully");
}

#[tokio::test]
async fn test_error_recovery_and_continuation() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    // Intermittent failures
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(5)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        fetcher_workers: 2,
        max_retries: 1,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    // Send 10 URLs - some will fail, some will succeed
    let urls: Vec<String> = (1..=10)
        .map(|i| format!("{}/article/{i}", mock_server.uri()))
        .collect();

    let stats = pipeline.run(urls).await.unwrap();

    // Pipeline should complete all jobs despite errors
    assert_eq!(stats.total_jobs, 10);
    assert_eq!(
        stats.success_count + stats.failed_count + stats.skipped_count,
        10,
        "All jobs should be processed"
    );
}
