//! Error types for the baram crawler
//!
//! This module defines custom error types used throughout the application.

use thiserror::Error;

/// Errors that can occur during HTTP fetching operations
#[derive(Error, Debug)]
pub enum FetchError {
    /// HTTP request error
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimit,

    /// Server error with status code
    #[error("Server error: {0}")]
    ServerError(u16),

    /// Request timeout
    #[error("Request timeout")]
    Timeout,

    /// Maximum retry attempts exceeded
    #[error("Maximum retry attempts exceeded")]
    MaxRetriesExceeded,

    /// Content decoding error
    #[error("Decoding error: {0}")]
    Decode(String),

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

/// Errors that can occur during parsing operations
#[derive(Error, Debug)]
pub enum ParseError {
    /// Title not found in HTML
    #[error("Title not found in article")]
    TitleNotFound,

    /// Content not found in HTML
    #[error("Content not found in article")]
    ContentNotFound,

    /// Invalid URL format
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    /// Failed to extract article IDs
    #[error("Failed to extract article ID from URL")]
    IdExtractionFailed,

    /// Article not found (404)
    #[error("Article not found")]
    ArticleNotFound,

    /// Unknown or unsupported format
    #[error("Unknown or unsupported format")]
    UnknownFormat,
}

/// General crawler errors
#[derive(Error, Debug)]
pub enum CrawlerError {
    /// Fetch error
    #[error("Fetch error: {0}")]
    Fetch(#[from] FetchError),

    /// Parse error
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    /// Invalid date format
    #[error("Invalid date format: {0}")]
    InvalidDate(String),

    /// No articles found
    #[error("No articles found")]
    NoArticlesFound,

    /// Rate limited
    #[error("Rate limited")]
    RateLimited,
}

impl FetchError {
    /// Check if this error is recoverable (can be retried)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            FetchError::RateLimit | FetchError::Timeout | FetchError::ServerError(_)
        )
    }

    /// Get Korean description for the error
    pub fn korean_desc(&self) -> &'static str {
        match self {
            FetchError::Http(_) => "HTTP 요청 실패",
            FetchError::RateLimit => "요청 한도 초과",
            FetchError::ServerError(_) => "서버 오류",
            FetchError::Timeout => "요청 시간 초과",
            FetchError::MaxRetriesExceeded => "최대 재시도 횟수 초과",
            FetchError::Decode(_) => "디코딩 오류",
            FetchError::InvalidUrl(_) => "잘못된 URL",
        }
    }
}

impl ParseError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(self, ParseError::ArticleNotFound)
    }

    /// Get Korean description for the error
    pub fn korean_desc(&self) -> &'static str {
        match self {
            ParseError::TitleNotFound => "제목을 찾을 수 없음",
            ParseError::ContentNotFound => "내용을 찾을 수 없음",
            ParseError::InvalidUrl(_) => "잘못된 URL",
            ParseError::IdExtractionFailed => "ID 추출 실패",
            ParseError::ArticleNotFound => "기사를 찾을 수 없음",
            ParseError::UnknownFormat => "알 수 없는 형식",
        }
    }
}

impl CrawlerError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            CrawlerError::Fetch(e) => e.is_recoverable(),
            CrawlerError::Parse(e) => e.is_recoverable(),
            CrawlerError::RateLimited => true,
            CrawlerError::InvalidDate(_) | CrawlerError::NoArticlesFound => false,
        }
    }

    /// Get Korean description for the error
    pub fn korean_desc(&self) -> &'static str {
        match self {
            CrawlerError::Fetch(e) => e.korean_desc(),
            CrawlerError::Parse(e) => e.korean_desc(),
            CrawlerError::InvalidDate(_) => "잘못된 날짜 형식",
            CrawlerError::NoArticlesFound => "기사를 찾을 수 없음",
            CrawlerError::RateLimited => "요청 한도 초과",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_error_is_recoverable() {
        assert!(FetchError::RateLimit.is_recoverable());
        assert!(FetchError::Timeout.is_recoverable());
        assert!(FetchError::ServerError(500).is_recoverable());
        assert!(!FetchError::InvalidUrl("test".to_string()).is_recoverable());
    }

    #[test]
    fn test_fetch_error_korean_desc() {
        assert_eq!(FetchError::RateLimit.korean_desc(), "요청 한도 초과");
        assert_eq!(FetchError::Timeout.korean_desc(), "요청 시간 초과");
    }

    #[test]
    fn test_parse_error_is_recoverable() {
        assert!(ParseError::ArticleNotFound.is_recoverable());
        assert!(!ParseError::TitleNotFound.is_recoverable());
    }

    #[test]
    fn test_parse_error_korean_desc() {
        assert_eq!(ParseError::TitleNotFound.korean_desc(), "제목을 찾을 수 없음");
        assert_eq!(ParseError::ArticleNotFound.korean_desc(), "기사를 찾을 수 없음");
    }

    #[test]
    fn test_crawler_error_is_recoverable() {
        assert!(CrawlerError::RateLimited.is_recoverable());
        assert!(!CrawlerError::NoArticlesFound.is_recoverable());

        let fetch_err = CrawlerError::Fetch(FetchError::Timeout);
        assert!(fetch_err.is_recoverable());
    }

    #[test]
    fn test_crawler_error_korean_desc() {
        assert_eq!(CrawlerError::RateLimited.korean_desc(), "요청 한도 초과");
        assert_eq!(CrawlerError::NoArticlesFound.korean_desc(), "기사를 찾을 수 없음");
    }
}
