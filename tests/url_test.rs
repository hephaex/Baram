//! Integration tests for URL extraction and list crawling
//!
//! These tests validate URL extraction, normalization, and list crawling functionality.

use ntimes::crawler::list::ListUrlBuilder;
use ntimes::crawler::url::{validators, UrlExtractor};
use ntimes::models::NewsCategory;

/// Test URL extraction from real list page HTML fixture
#[test]
fn test_url_extraction_from_fixture() {
    let html = include_str!("fixtures/html/list_page.html");
    let extractor = UrlExtractor::new();

    let urls = extractor.extract_urls(html);

    // Should find multiple unique URLs (duplicates removed)
    assert!(!urls.is_empty(), "Should extract URLs from list page");

    // All URLs should be valid article URLs
    for url in &urls {
        assert!(
            extractor.is_valid_article_url(url),
            "URL should be valid: {url}"
        );
    }
}

/// Test URL deduplication
#[test]
fn test_url_deduplication() {
    let html = include_str!("fixtures/html/list_page.html");
    let extractor = UrlExtractor::new();

    let urls = extractor.extract_urls(html);

    // Check for no duplicates
    let mut seen = std::collections::HashSet::new();
    for url in &urls {
        assert!(seen.insert(url), "Duplicate URL found: {url}");
    }
}

/// Test ID extraction from new format URLs
#[test]
fn test_id_extraction_new_format() {
    let extractor = UrlExtractor::new();

    let test_cases = vec![
        (
            "https://n.news.naver.com/mnews/article/001/0014500001",
            "001",
            "0014500001",
        ),
        (
            "https://n.news.naver.com/mnews/article/123/0012345678",
            "123",
            "0012345678",
        ),
        (
            "https://news.naver.com/article/456/0098765432",
            "456",
            "0098765432",
        ),
    ];

    for (url, expected_oid, expected_aid) in test_cases {
        let result = extractor.extract_ids(url);
        assert!(result.is_ok(), "Should extract IDs from: {url}");

        let (oid, aid) = result.unwrap();
        assert_eq!(oid, expected_oid, "OID mismatch for {url}");
        assert_eq!(aid, expected_aid, "AID mismatch for {url}");
    }
}

/// Test ID extraction from old format URLs
#[test]
fn test_id_extraction_old_format() {
    let extractor = UrlExtractor::new();

    let url =
        "https://news.naver.com/main/read.naver?mode=LSD&mid=shm&sid1=100&oid=006&aid=0014500006";
    let result = extractor.extract_ids(url);

    assert!(result.is_ok(), "Should extract IDs from old format URL");

    let (oid, aid) = result.unwrap();
    assert_eq!(oid, "006");
    assert_eq!(aid, "0014500006");
}

/// Test URL normalization
#[test]
fn test_url_normalization() {
    let extractor = UrlExtractor::new();

    // Test cases: (input, expected_contains)
    let test_cases = vec![
        (
            "https://m.news.naver.com/article/001/0014500001",
            "n.news.naver.com",
        ),
        (
            "https://news.naver.com/main/read.naver?oid=002&aid=0014500002",
            "mnews/article/002/0014500002",
        ),
    ];

    for (input, expected_contains) in test_cases {
        let normalized = extractor.normalize_url(input);
        assert!(normalized.is_some(), "Should normalize: {input}");
        assert!(
            normalized.as_ref().unwrap().contains(expected_contains),
            "Normalized URL should contain '{expected_contains}': {normalized:?}"
        );
    }
}

/// Test article URL validation
#[test]
fn test_article_url_validation() {
    let extractor = UrlExtractor::new();

    // Valid URLs
    let valid_urls = vec![
        "https://n.news.naver.com/mnews/article/001/0014500001",
        "https://news.naver.com/article/002/0014500002",
        "https://n.news.naver.com/mnews/article/123/0012345678?sid=100",
    ];

    for url in valid_urls {
        assert!(
            extractor.is_valid_article_url(url),
            "Should be valid: {url}"
        );
    }

    // Invalid URLs
    let invalid_urls = vec![
        "https://google.com/search",
        "https://evil.com/mnews/article/001/0014500001",
        "http://localhost/admin",
        "file:///etc/passwd",
    ];

    for url in invalid_urls {
        assert!(
            !extractor.is_valid_article_url(url),
            "Should be invalid: {url}"
        );
    }
}

/// Test SSRF prevention
#[test]
fn test_ssrf_prevention() {
    // Dangerous URLs that should be blocked
    let dangerous_urls = vec![
        "http://127.0.0.1/admin",
        "http://localhost/secret",
        "http://192.168.1.1/config",
        "http://10.0.0.1/internal",
        "http://172.16.0.1/private",
        "http://169.254.169.254/metadata", // AWS metadata
        "file:///etc/passwd",
    ];

    for url in dangerous_urls {
        assert!(
            !validators::is_safe_url(url),
            "Should block dangerous URL: {url}"
        );
    }

    // Safe URLs that should be allowed
    let safe_urls = vec![
        "https://n.news.naver.com/mnews/article/001/0014500001",
        "https://news.naver.com/main/list.naver",
        "https://sports.naver.com/article/001/123",
    ];

    for url in safe_urls {
        assert!(validators::is_safe_url(url), "Should allow safe URL: {url}");
    }
}

/// Test allowed domain validation
#[test]
fn test_allowed_domains() {
    // Allowed domains
    let allowed = vec![
        "https://n.news.naver.com/mnews/article/001/123",
        "https://news.naver.com/main/list.naver",
        "https://sports.naver.com/article/001/123",
        "https://entertain.naver.com/article/001/123",
    ];

    for url in allowed {
        assert!(
            validators::is_allowed_domain(url),
            "Should allow domain: {url}"
        );
    }

    // Disallowed domains
    let disallowed = vec![
        "https://google.com/search",
        "https://evil-naver.com/fake",
        "https://naver.com.evil.com/",
    ];

    for url in disallowed {
        assert!(
            !validators::is_allowed_domain(url),
            "Should block domain: {url}"
        );
    }
}

/// Test ListUrlBuilder for all categories
#[test]
fn test_list_url_builder_all_categories() {
    let categories = vec![
        (NewsCategory::Politics, 100),
        (NewsCategory::Economy, 101),
        (NewsCategory::Society, 102),
        (NewsCategory::Culture, 103),
        (NewsCategory::World, 104),
        (NewsCategory::IT, 105),
    ];

    for (category, expected_sid) in categories {
        let url = ListUrlBuilder::main_list(category, "20241215", 1);

        assert!(
            url.contains(&format!("sid1={expected_sid}")),
            "URL should contain sid1={expected_sid}: {url}"
        );
        assert!(url.contains("date=20241215"), "URL should contain date");
        assert!(url.contains("page=1"), "URL should contain page");
    }
}

/// Test ranking URL builder
#[test]
fn test_ranking_url_builder() {
    let url = ListUrlBuilder::ranking_list(NewsCategory::IT, 2);

    assert!(url.contains("popularDay"));
    assert!(url.contains("sid1=105"));
    assert!(url.contains("page=2"));
}

/// Test section latest URL builder
#[test]
fn test_section_latest_url_builder() {
    let url = ListUrlBuilder::section_latest(NewsCategory::Economy);
    assert_eq!(url, "https://news.naver.com/section/101");
}

/// Test relative to absolute URL conversion
#[test]
fn test_relative_to_absolute() {
    let extractor = UrlExtractor::new();

    let base = "https://news.naver.com/main/list.naver";
    let relative = "/mnews/article/001/0014500001";

    let absolute = extractor.to_absolute(relative, base);

    assert!(absolute.starts_with("https://"));
    assert!(absolute.contains("mnews/article/001/0014500001"));
}

/// Test URL validation function
#[test]
fn test_url_validation() {
    // Valid URL
    let result = validators::validate_url("https://n.news.naver.com/mnews/article/001/123");
    assert!(result.is_ok());

    // Invalid - dangerous URL
    let result = validators::validate_url("http://127.0.0.1/admin");
    assert!(result.is_err());

    // Invalid - not allowed domain
    let result = validators::validate_url("https://evil.com/fake");
    assert!(result.is_err());
}
