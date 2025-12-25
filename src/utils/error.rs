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
