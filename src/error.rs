//! Unified error handling for the baram crate
//!
//! This module provides a unified error type that consolidates all domain-specific
//! errors into a single `Error` enum, while maintaining the ability to use
//! domain-specific errors when needed.
//!
//! # Architecture
//!
//! - [`BaramErrorTrait`] - Common interface implemented by all error types
//! - [`ErrorCategory`] - Classification of errors for handling strategies
//! - [`Error`] - Unified error enum wrapping all domain-specific errors
//!
//! # Usage
//!
//! ```rust,ignore
//! use baram::error::{Error, ErrorCategory, BaramErrorTrait};
//!
//! fn handle_error(err: Error) {
//!     if err.is_recoverable() {
//!         println!("Retrying: {}", err.korean_desc());
//!     } else {
//!         eprintln!("Fatal error: {}", err);
//!     }
//! }
//! ```

use std::io;
use thiserror::Error;

// Re-export domain-specific errors for convenience
pub use crate::ontology::error::OntologyError;
pub use crate::scheduler::error::SchedulerError;
pub use crate::utils::error::{CrawlerError, FetchError, ParseError};

/// Common trait for all baram error types
///
/// This trait provides a unified interface for error handling across
/// all modules, enabling consistent error processing strategies.
pub trait BaramErrorTrait: std::error::Error {
    /// Check if this error is recoverable (can be retried)
    fn is_recoverable(&self) -> bool;

    /// Get localized description for user-facing messages
    fn localized_desc(&self) -> String;

    /// Get Korean description for user-facing messages (deprecated, use localized_desc)
    #[deprecated(since = "0.1.6", note = "Use localized_desc() instead")]
    fn korean_desc(&self) -> String {
        self.localized_desc()
    }

    /// Get the error category for handling strategies
    fn category(&self) -> ErrorCategory;
}

/// Classification of errors for handling strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Network-related errors (HTTP, timeout, rate limit)
    Network,
    /// Parsing and data extraction errors
    Parsing,
    /// Storage and I/O errors
    Storage,
    /// LLM and AI processing errors
    Llm,
    /// Configuration and validation errors
    Config,
    /// Scheduler and timing errors
    Scheduler,
    /// Other/unknown errors
    Other,
}

impl ErrorCategory {
    /// Get localized description for the category
    pub fn localized_desc(&self) -> String {
        match self {
            Self::Network => crate::i18n::t!("errors.category.network").to_string(),
            Self::Parsing => crate::i18n::t!("errors.category.parsing").to_string(),
            Self::Storage => crate::i18n::t!("errors.category.storage").to_string(),
            Self::Llm => crate::i18n::t!("errors.category.llm").to_string(),
            Self::Config => crate::i18n::t!("errors.category.config").to_string(),
            Self::Scheduler => crate::i18n::t!("errors.category.scheduler").to_string(),
            Self::Other => crate::i18n::t!("errors.category.other").to_string(),
        }
    }

    /// Get Korean description for the category (deprecated, use localized_desc)
    #[deprecated(since = "0.1.6", note = "Use localized_desc() instead")]
    pub fn korean_desc(&self) -> &'static str {
        match self {
            Self::Network => "네트워크 오류",
            Self::Parsing => "파싱 오류",
            Self::Storage => "저장소 오류",
            Self::Llm => "LLM 오류",
            Self::Config => "설정 오류",
            Self::Scheduler => "스케줄러 오류",
            Self::Other => "기타 오류",
        }
    }
}

/// Unified error type for the baram crate
///
/// This enum wraps all domain-specific errors, providing a single error type
/// that can be used across module boundaries while preserving the detailed
/// error information.
#[derive(Error, Debug)]
pub enum Error {
    /// Crawler-related errors (fetch, parse, crawl)
    #[error("Crawler error: {0}")]
    Crawler(#[from] CrawlerError),

    /// Fetch-specific errors
    #[error("Fetch error: {0}")]
    Fetch(#[from] FetchError),

    /// Parse-specific errors
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    /// Ontology extraction and processing errors
    #[error("Ontology error: {0}")]
    Ontology(#[from] OntologyError),

    /// Scheduler and timing errors
    #[error("Scheduler error: {0}")]
    Scheduler(#[from] SchedulerError),

    /// Database errors
    #[error("Database error: {0}")]
    Database(#[source] rusqlite::Error),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP client errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Configuration errors
    #[error("Config error: {0}")]
    Config(String),

    /// Generic error with context
    #[error("{context}")]
    Other {
        context: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl BaramErrorTrait for Error {
    fn is_recoverable(&self) -> bool {
        match self {
            Self::Crawler(e) => e.is_recoverable(),
            Self::Fetch(e) => e.is_recoverable(),
            Self::Parse(e) => e.is_recoverable(),
            Self::Ontology(e) => e.is_recoverable(),
            Self::Scheduler(e) => e.is_recoverable(),
            Self::Database(_) => false,
            Self::Io(_) => true, // I/O errors are often transient
            Self::Json(_) => false,
            Self::Http(_) => true, // HTTP errors are often transient
            Self::Config(_) => false,
            Self::Other { .. } => false,
        }
    }

    fn localized_desc(&self) -> String {
        match self {
            Self::Crawler(e) => e.localized_desc(),
            Self::Fetch(e) => e.localized_desc(),
            Self::Parse(e) => e.localized_desc(),
            Self::Ontology(e) => e.localized_desc(),
            Self::Scheduler(e) => e.localized_desc(),
            Self::Database(e) => format!("{}: {e}", crate::i18n::t!("errors.database.error").to_string()),
            Self::Io(e) => format!("{}: {e}", crate::i18n::t!("errors.io.error").to_string()),
            Self::Json(e) => format!("{}: {e}", crate::i18n::t!("errors.json.error").to_string()),
            Self::Http(e) => format!("{}: {e}", crate::i18n::t!("errors.http.error").to_string()),
            Self::Config(msg) => format!("{}: {msg}", crate::i18n::t!("errors.config.error").to_string()),
            Self::Other { context, .. } => context.clone(),
        }
    }

    fn category(&self) -> ErrorCategory {
        match self {
            Self::Crawler(_) | Self::Fetch(_) | Self::Http(_) => ErrorCategory::Network,
            Self::Parse(_) => ErrorCategory::Parsing,
            Self::Ontology(e) => match e {
                OntologyError::LlmResponseParseFailed { .. }
                | OntologyError::EmptyLlmResponse
                | OntologyError::InvalidLlmJson { .. } => ErrorCategory::Llm,
                OntologyError::StorageDirectoryNotFound { .. }
                | OntologyError::StorageDirectoryCreationFailed { .. }
                | OntologyError::StorageSaveFailed { .. }
                | OntologyError::StorageLoadFailed { .. }
                | OntologyError::IoError { .. } => ErrorCategory::Storage,
                OntologyError::InvalidConfig { .. } | OntologyError::MissingConfig { .. } => {
                    ErrorCategory::Config
                }
                _ => ErrorCategory::Other,
            },
            Self::Scheduler(_) => ErrorCategory::Scheduler,
            Self::Database(_) | Self::Io(_) => ErrorCategory::Storage,
            Self::Json(_) => ErrorCategory::Parsing,
            Self::Config(_) => ErrorCategory::Config,
            Self::Other { .. } => ErrorCategory::Other,
        }
    }
}

impl Error {
    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a generic error with context
    pub fn other(context: impl Into<String>) -> Self {
        Self::Other {
            context: context.into(),
            source: None,
        }
    }

    /// Create a generic error with context and source
    pub fn with_source(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Other {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }
}

// Conversion from rusqlite::Error
impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Self::Database(err)
    }
}

// Conversion from anyhow::Error
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self::Other {
            context: err.to_string(),
            source: None,
        }
    }
}

/// Result type alias using the unified Error type
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_category() {
        let fetch_err = Error::Fetch(FetchError::Timeout);
        assert_eq!(fetch_err.category(), ErrorCategory::Network);

        let parse_err = Error::Parse(ParseError::TitleNotFound);
        assert_eq!(parse_err.category(), ErrorCategory::Parsing);
    }

    #[test]
    fn test_is_recoverable() {
        let fetch_err = Error::Fetch(FetchError::Timeout);
        assert!(fetch_err.is_recoverable());

        let parse_err = Error::Parse(ParseError::TitleNotFound);
        assert!(!parse_err.is_recoverable());
    }

    #[test]
    fn test_korean_desc() {
        let fetch_err = Error::Fetch(FetchError::RateLimit);
        assert_eq!(fetch_err.korean_desc(), "요청 한도 초과");
    }

    #[test]
    fn test_error_conversion() {
        let crawler_err = CrawlerError::RateLimited;
        let unified: Error = crawler_err.into();
        assert!(matches!(unified, Error::Crawler(_)));
    }

    #[test]
    fn test_config_error() {
        let err = Error::config("Invalid API key");
        assert_eq!(err.category(), ErrorCategory::Config);
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_other_error() {
        let err = Error::other("Something went wrong");
        assert_eq!(err.category(), ErrorCategory::Other);
    }

    #[test]
    fn test_error_category_korean() {
        assert_eq!(ErrorCategory::Network.korean_desc(), "네트워크 오류");
        assert_eq!(ErrorCategory::Storage.korean_desc(), "저장소 오류");
    }
}
