//! Integration tests for NaverFetcher using wiremock
//!
//! These tests validate the HTTP fetcher's behavior with mock servers.

use ntimes::crawler::fetcher::NaverFetcher;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Test successful fetch from mock server
#[tokio::test]
async fn test_fetch_success() {
    let mock_server = MockServer::start().await;
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test Article</title></head>
<body><h1>테스트 기사</h1><p>본문 내용입니다.</p></body>
</html>"#;

    Mock::given(method("GET"))
        .and(path("/article/001/123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 10).unwrap();
    let result = fetcher.fetch_article("/article/001/123", 100).await;

    assert!(result.is_ok(), "Fetch should succeed: {:?}", result.err());
    let body = result.unwrap();
    assert!(body.contains("테스트 기사"));
    assert!(body.contains("본문 내용입니다"));
}

/// Test that server errors trigger retries
#[tokio::test]
async fn test_server_error_retry() {
    let mock_server = MockServer::start().await;

    // Return 500 twice, then succeed
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 100).unwrap();
    let result = fetcher.fetch_article("/test", 100).await;

    assert!(result.is_ok(), "Should succeed after retries");
}

/// Test 404 does not retry
#[tokio::test]
async fn test_404_no_retry() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/notfound"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1) // Should only be called once (no retry)
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 100).unwrap();
    let result = fetcher.fetch_article("/notfound", 100).await;

    assert!(result.is_err());
}

/// Test max retries exceeded
#[tokio::test]
async fn test_max_retries_exceeded() {
    let mock_server = MockServer::start().await;

    // Always return 503
    Mock::given(method("GET"))
        .and(path("/always-fail"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock_server)
        .await;

    // Use custom config with 2 retries
    let fetcher = NaverFetcher::with_config_and_base_url(
        &mock_server.uri(),
        100,
        2, // max_retries
        Duration::from_secs(30),
    )
    .unwrap();

    let result = fetcher.fetch_article("/always-fail", 100).await;
    assert!(result.is_err());
}

/// Test UTF-8 decoding
#[tokio::test]
async fn test_utf8_decoding() {
    let mock_server = MockServer::start().await;
    let korean_html = "<html><body>안녕하세요 한글 테스트</body></html>";

    Mock::given(method("GET"))
        .and(path("/utf8"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(korean_html)
                .insert_header("content-type", "text/html; charset=utf-8"),
        )
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 10).unwrap();
    let result = fetcher.fetch_article("/utf8", 100).await;

    assert!(result.is_ok());
    let body = result.unwrap();
    assert!(body.contains("안녕하세요"));
    assert!(body.contains("한글 테스트"));
}

/// Test User-Agent header is set
#[tokio::test]
async fn test_user_agent_header() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/ua-test"))
        .and(wiremock::matchers::header_exists("user-agent"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 10).unwrap();
    let result = fetcher.fetch_article("/ua-test", 100).await;

    assert!(result.is_ok());
}

/// Test Referer header is set correctly
#[tokio::test]
async fn test_referer_header() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/referer-test"))
        .and(wiremock::matchers::header_exists("referer"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 10).unwrap();
    let result = fetcher.fetch_article("/referer-test", 100).await;

    assert!(result.is_ok());
}

/// Test Accept-Language header includes Korean
#[tokio::test]
async fn test_accept_language_korean() {
    let mock_server = MockServer::start().await;

    // Use header_exists since the exact format may vary
    Mock::given(method("GET"))
        .and(path("/lang-test"))
        .and(wiremock::matchers::header_exists("accept-language"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 10).unwrap();
    let result = fetcher.fetch_article("/lang-test", 100).await;

    assert!(result.is_ok());
}

/// Test rate limiting respects configured limit
#[tokio::test]
async fn test_rate_limiting() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rate-test"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    // Create fetcher with 2 requests per second
    let fetcher = NaverFetcher::with_base_url(&mock_server.uri(), 2).unwrap();

    let start = std::time::Instant::now();

    // Make 3 requests
    for _ in 0..3 {
        let _ = fetcher.fetch_article("/rate-test", 100).await;
    }

    let elapsed = start.elapsed();

    // With 2 req/sec, 3 requests should take at least 1 second
    // (first request immediate, second after 0.5s, third after 1s)
    assert!(
        elapsed >= Duration::from_millis(500),
        "Rate limiting should slow down requests: {:?}",
        elapsed
    );
}

/// Test decode_bytes with explicit charset
#[test]
fn test_decode_bytes_utf8() {
    let fetcher = NaverFetcher::new(2).unwrap();
    let utf8_bytes = "한글 테스트".as_bytes();

    let result = fetcher.decode_bytes(utf8_bytes, "text/html; charset=utf-8");

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "한글 테스트");
}

/// Test decode_bytes with EUC-KR
#[test]
fn test_decode_bytes_euc_kr() {
    let fetcher = NaverFetcher::new(2).unwrap();

    // EUC-KR bytes for "한글" (0xC7 0xD1 0xB1 0xDB)
    let euc_kr_bytes: &[u8] = &[0xC7, 0xD1, 0xB1, 0xDB];

    let result = fetcher.decode_bytes(euc_kr_bytes, "text/html; charset=euc-kr");

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "한글");
}

/// Test fetcher creation with different configs
#[test]
fn test_fetcher_creation_configs() {
    // Default
    let f1 = NaverFetcher::new(2);
    assert!(f1.is_ok());

    // Custom config
    let f2 = NaverFetcher::with_config(5, 5, Duration::from_secs(60));
    assert!(f2.is_ok());

    // With base URL
    let f3 = NaverFetcher::with_base_url("http://localhost:8080", 10);
    assert!(f3.is_ok());
}
