//! Tests for models module

mod common;

use ktime::models::{CrawlState, CrawlStats, NewsCategory, ParsedArticle};

#[test]
fn test_article_id_generation() {
    let article = common::create_test_article();
    assert_eq!(article.id(), "001_0014123456");
}

#[test]
fn test_article_hash_computation() {
    let mut article = common::create_test_article();
    assert!(article.content_hash.is_none());

    article.compute_hash();

    assert!(article.content_hash.is_some());
    let hash = article.content_hash.as_ref().unwrap();
    assert_eq!(hash.len(), 64); // SHA256 produces 64 hex characters
}

#[test]
fn test_same_content_same_hash() {
    let mut article1 = ParsedArticle {
        content: "동일한 내용".to_string(),
        ..Default::default()
    };
    let mut article2 = ParsedArticle {
        content: "동일한 내용".to_string(),
        ..Default::default()
    };

    article1.compute_hash();
    article2.compute_hash();

    assert_eq!(article1.content_hash, article2.content_hash);
}

#[test]
fn test_different_content_different_hash() {
    let mut article1 = ParsedArticle {
        content: "내용 A".to_string(),
        ..Default::default()
    };
    let mut article2 = ParsedArticle {
        content: "내용 B".to_string(),
        ..Default::default()
    };

    article1.compute_hash();
    article2.compute_hash();

    assert_ne!(article1.content_hash, article2.content_hash);
}

#[test]
fn test_category_section_id_roundtrip() {
    for category in NewsCategory::all() {
        let id = category.to_section_id();
        let restored = NewsCategory::from_section_id(id);
        assert_eq!(Some(category), restored);
    }
}

#[test]
fn test_category_string_roundtrip() {
    for category in NewsCategory::all() {
        let s = category.as_str();
        let restored = NewsCategory::parse(s);
        assert_eq!(Some(category), restored);
    }
}

#[test]
fn test_category_korean_names() {
    assert_eq!(NewsCategory::Politics.korean_name(), "정치");
    assert_eq!(NewsCategory::Economy.korean_name(), "경제");
    assert_eq!(NewsCategory::IT.korean_name(), "IT/과학");
}

#[test]
fn test_category_from_korean() {
    assert_eq!(NewsCategory::parse("정치"), Some(NewsCategory::Politics));
    assert_eq!(NewsCategory::parse("경제"), Some(NewsCategory::Economy));
}

#[test]
fn test_invalid_section_id() {
    assert!(NewsCategory::from_section_id(999).is_none());
    assert!(NewsCategory::from_section_id(0).is_none());
}

#[test]
fn test_invalid_category_string() {
    assert!(NewsCategory::parse("invalid").is_none());
    assert!(NewsCategory::parse("").is_none());
}

#[test]
fn test_crawl_state_new() {
    let state = CrawlState::new();
    assert!(state.completed_articles.is_empty());
    assert_eq!(state.total_crawled, 0);
    assert!(state.started_at.is_some());
}

#[test]
fn test_crawl_state_mark_completed() {
    let mut state = CrawlState::new();

    state.mark_completed("001_123");

    assert!(state.is_completed("001_123"));
    assert!(!state.is_completed("001_456"));
    assert_eq!(state.total_crawled, 1);
}

#[test]
fn test_crawl_state_no_duplicate_counting() {
    let mut state = CrawlState::new();

    state.mark_completed("001_123");
    state.mark_completed("001_123"); // Same article

    assert_eq!(state.completed_articles.len(), 1);
    // Note: total_crawled still increments - this tracks attempts
    assert_eq!(state.total_crawled, 2);
}

#[test]
fn test_crawl_state_record_error() {
    let mut state = CrawlState::new();

    state.record_error();
    state.record_error();

    assert_eq!(state.total_errors, 2);
}

#[test]
fn test_crawl_stats_error_rate() {
    let stats = CrawlStats {
        total_crawled: 100,
        total_errors: 5,
        unique_articles: 100,
        duration_secs: 60,
    };

    assert!((stats.error_rate() - 5.0).abs() < f64::EPSILON);
}

#[test]
fn test_crawl_stats_error_rate_zero_crawled() {
    let stats = CrawlStats {
        total_crawled: 0,
        total_errors: 0,
        unique_articles: 0,
        duration_secs: 0,
    };

    assert_eq!(stats.error_rate(), 0.0);
}

#[test]
fn test_crawl_stats_crawl_rate() {
    let stats = CrawlStats {
        total_crawled: 120,
        total_errors: 0,
        unique_articles: 120,
        duration_secs: 60,
    };

    // 120 articles in 60 seconds = 120 per minute
    assert!((stats.crawl_rate() - 120.0).abs() < f64::EPSILON);
}

#[test]
fn test_all_categories_count() {
    let all = NewsCategory::all();
    assert_eq!(all.len(), 6);
}

#[test]
fn test_category_display() {
    assert_eq!(format!("{}", NewsCategory::Politics), "politics");
    assert_eq!(format!("{}", NewsCategory::IT), "it");
}
