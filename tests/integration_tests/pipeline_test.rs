//! End-to-end pipeline integration tests
//!
//! Tests the complete workflow:
//! 1. URL submission
//! 2. HTTP fetch (mocked)
//! 3. HTML parsing
//! 4. Article storage
//! 5. Statistics tracking

use baram::crawler::pipeline::{CrawlerPipeline, PipelineBuilder, PipelineConfig};
use baram::crawler::fetcher::NaverFetcher;
use baram::parser::ArticleParser;
use baram::storage::MarkdownWriter;
use std::time::Duration;
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixtures::{
    expected_article_title, sample_article_url,
    SAMPLE_ARTICLE_HTML, SAMPLE_ARTICLE_HTML_ALT,
};

// ============================================================================
// Complete Pipeline Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_single_article_success() {
    // Arrange: Create temp directory for output
    let temp_dir = TempDir::new().unwrap();

    // Setup mock HTTP server
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex("/mnews/article/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    // Build pipeline with mock server
    let config = PipelineConfig {
        fetcher_workers: 1,
        parser_workers: 1,
        storage_workers: 1,
        channel_buffer_size: 10,
        output_dir: temp_dir.path().to_path_buf(),
        requests_per_second: 10,
        request_timeout: Duration::from_secs(5),
        crawl_comments: false,
        max_retries: 1,
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    // Act: Run pipeline with single URL
    let url = sample_article_url("0014000001");
    // Modify URL to use mock server
    let mock_url = url.replace("https://n.news.naver.com", &mock_server.uri());

    let stats = pipeline.run(vec![mock_url]).await.unwrap();

    // Assert: Check statistics
    assert_eq!(stats.total_jobs, 1);
    assert!(
        stats.success_count >= 1 || stats.skipped_count >= 1,
        "Should have at least one success or skip (success={}, skip={})",
        stats.success_count,
        stats.skipped_count
    );
    assert!(stats.bytes_fetched > 0);
}

#[tokio::test]
async fn test_pipeline_multiple_articles() {
    // Arrange
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    // Mock different responses for different articles
    Mock::given(method("GET"))
        .and(path_regex("/mnews/article/001/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex("/mnews/article/002/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML_ALT))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        fetcher_workers: 2,
        parser_workers: 2,
        storage_workers: 1,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    // Act: Process multiple articles
    let urls = vec![
        sample_article_url("0014000001").replace("https://n.news.naver.com", &mock_server.uri()),
        sample_article_url("0014000002").replace("https://n.news.naver.com", &mock_server.uri()),
        "https://n.news.naver.com/mnews/article/002/0014000003?sid=105"
            .replace("https://n.news.naver.com", &mock_server.uri()),
    ];

    let stats = pipeline.run(urls).await.unwrap();

    // Assert
    assert_eq!(stats.total_jobs, 3);
    let completed = stats.success_count + stats.failed_count + stats.skipped_count;
    assert_eq!(completed, 3, "All jobs should complete");
    assert!(stats.bytes_fetched > 0);
}

#[tokio::test]
async fn test_pipeline_duplicate_detection() {
    // Arrange
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    // Act: Submit same URL twice
    let url = sample_article_url("0014000001").replace("https://n.news.naver.com", &mock_server.uri());
    let urls = vec![url.clone(), url.clone()];

    let stats = pipeline.run(urls).await.unwrap();

    // Assert: Second one should be skipped
    assert_eq!(stats.total_jobs, 2);
    assert!(
        stats.skipped_count >= 1,
        "Duplicate should be detected and skipped"
    );
}

#[tokio::test]
async fn test_pipeline_progress_tracking() {
    // Arrange
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        fetcher_workers: 2,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    // Initial stats should be zero
    let initial_stats = pipeline.stats();
    assert_eq!(initial_stats.success_count, 0);
    assert_eq!(initial_stats.failed_count, 0);

    // Act: Run pipeline
    let urls = vec![
        sample_article_url("0014000001").replace("https://n.news.naver.com", &mock_server.uri()),
        sample_article_url("0014000002").replace("https://n.news.naver.com", &mock_server.uri()),
    ];

    let stats = pipeline.run(urls).await.unwrap();

    // Assert: Stats should be updated
    assert!(stats.success_count + stats.failed_count + stats.skipped_count > 0);
}

// ============================================================================
// Component Integration Tests
// ============================================================================

#[tokio::test]
async fn test_fetcher_parser_integration() {
    // Test that fetcher output can be parsed by parser
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    // Create fetcher with mock server
    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        10,
        1,
        Duration::from_secs(5),
    )
    .unwrap();

    // Fetch article
    let url = "/mnews/article/001/0014000001?sid=105";
    let response = fetcher.fetch(url).await.unwrap();
    let html = response.text().await.unwrap();

    // Parse fetched HTML
    let parser = ArticleParser::new();
    let article = parser.parse_with_fallback(&html, url);

    // Assert: Article should be parsed successfully
    assert!(article.is_ok(), "Parser should handle fetcher output");
    let article = article.unwrap();
    assert_eq!(article.title, expected_article_title());
    assert!(article.content.contains("네이버가 새로운"));
}

#[tokio::test]
async fn test_parser_storage_integration() {
    // Test that parser output can be stored
    let temp_dir = TempDir::new().unwrap();

    // Parse sample HTML
    let parser = ArticleParser::new();
    let url = sample_article_url("0014000001");
    let mut article = parser
        .parse_with_fallback(SAMPLE_ARTICLE_HTML, &url)
        .unwrap();

    // Set required fields
    article.category = "science".to_string();

    // Store article
    let writer = MarkdownWriter::new(temp_dir.path()).unwrap();
    let result = writer.save(&article);

    // Assert: Storage should succeed
    assert!(result.is_ok(), "Storage should handle parser output");
    let path = result.unwrap();
    assert!(path.exists());
}

// ============================================================================
// Builder Pattern Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_builder() {
    let temp_dir = TempDir::new().unwrap();

    let pipeline = PipelineBuilder::new()
        .fetcher_workers(10)
        .parser_workers(5)
        .storage_workers(3)
        .channel_buffer_size(500)
        .output_dir(temp_dir.path().to_path_buf())
        .requests_per_second(15)
        .request_timeout(Duration::from_secs(20))
        .crawl_comments(false)
        .build()
        .await;

    assert!(pipeline.is_ok());
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_concurrent_processing() {
    // Test that multiple workers can process concurrently
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    // Mock with small delay to simulate network latency
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(SAMPLE_ARTICLE_HTML)
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        fetcher_workers: 5, // Multiple workers should speed up processing
        parser_workers: 3,
        storage_workers: 2,
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    // Generate 10 unique URLs
    let urls: Vec<String> = (1..=10)
        .map(|i| {
            format!(
                "{}/mnews/article/001/001400000{i}?sid=105",
                mock_server.uri()
            )
        })
        .collect();

    let start = std::time::Instant::now();
    let stats = pipeline.run(urls).await.unwrap();
    let elapsed = start.elapsed();

    // Assert: Should complete in reasonable time with concurrent workers
    assert_eq!(stats.total_jobs, 10);
    println!(
        "Processed {} articles in {:?} ({} successful, {} failed, {} skipped)",
        stats.total_jobs, elapsed, stats.success_count, stats.failed_count, stats.skipped_count
    );

    // With concurrent workers, should complete reasonably fast
    // Note: In CI or under load, this may take longer
    assert!(
        elapsed < Duration::from_secs(10),
        "Should process in reasonable time, took {:?}",
        elapsed
    );

    // Should have processed all articles
    assert_eq!(stats.total_jobs, 10);
    let total_processed = stats.success_count + stats.failed_count + stats.skipped_count;
    assert_eq!(total_processed, 10, "All articles should be processed");
}

#[tokio::test]
async fn test_pipeline_success_rate() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_ARTICLE_HTML))
        .mount(&mock_server)
        .await;

    let config = PipelineConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let pipeline = CrawlerPipeline::new(config).await.unwrap();

    let urls = vec![
        format!("{}/mnews/article/001/0014000001?sid=105", mock_server.uri()),
        format!("{}/mnews/article/001/0014000002?sid=105", mock_server.uri()),
    ];

    let stats = pipeline.run(urls).await.unwrap();

    // Calculate success rate
    let success_rate = stats.success_rate();

    // Should have high success rate with valid mocked responses
    assert!(
        success_rate > 0.5,
        "Success rate should be > 50%, got {}",
        success_rate
    );
}
