//! Common test utilities

use chrono::Utc;
use ntimes::models::ParsedArticle;

/// Create a test article with default values
pub fn create_test_article() -> ParsedArticle {
    ParsedArticle {
        oid: "001".to_string(),
        aid: "0014123456".to_string(),
        title: "테스트 기사 제목".to_string(),
        content: "테스트 기사 본문 내용입니다. 이것은 테스트를 위한 내용입니다.".to_string(),
        url: "https://n.news.naver.com/mnews/article/001/0014123456".to_string(),
        category: "politics".to_string(),
        publisher: Some("테스트언론사".to_string()),
        author: Some("홍길동".to_string()),
        published_at: Some(Utc::now()),
        crawled_at: Utc::now(),
        content_hash: None,
    }
}

/// Create article with specific oid/aid
#[allow(dead_code)]
pub fn create_article_with_id(oid: &str, aid: &str) -> ParsedArticle {
    ParsedArticle {
        oid: oid.to_string(),
        aid: aid.to_string(),
        title: format!("Article {oid}_{aid}"),
        content: "Test content".to_string(),
        url: format!("https://n.news.naver.com/mnews/article/{oid}/{aid}"),
        category: "politics".to_string(),
        crawled_at: Utc::now(),
        ..Default::default()
    }
}
