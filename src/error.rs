//! Unified error handling for the Baram crawler
//!
//! This module provides a consolidated error handling system that:
//! - Re-exports all domain-specific error types
//! - Provides a common `BaramError` trait for consistent behavior
//! - Offers a unified `Error` enum for cross-domain error handling
//!
//! # Error Hierarchy
//!
//! ```text
//! Error (unified)
//! ├── Crawler
//! │   ├── FetchError
//! │   └── ParseError
//! ├── Ontology (OntologyError)
//! ├── Scheduler (SchedulerError)
//! ├── Storage (anyhow::Error)
//! └── Other (anyhow::Error)
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use baram::error::{Error, Result, FetchError, ParseError};
//!
//! fn do_something() -> Result<()> {
//!     // Domain errors automatically convert to unified Error
//!     let data = fetch_data()?;
//!     Ok(())
//! }
//! ```

use thiserror::Error;

// Re-export domain-specific errors for convenience
pub use crate::ontology::error::{OntologyError, OntologyResult};
pub use crate::scheduler::error::{SchedulerError, SchedulerResult};
pub use crate::utils::error::{CrawlerError, FetchError, ParseError};

/// Unified result type for Baram operations
pub type Result<T> = std::result::Result<T, Error>;

/// Common trait for all Baram errors
///
/// This trait provides a consistent interface across all error types.
pub trait BaramErrorTrait: std::error::Error + Send + Sync {
    /// Check if this error is recoverable (can be retried)
    fn is_recoverable(&self) -> bool;

    /// Get Korean description for the error
    fn korean_desc(&self) -> String;

    /// Get error category for metrics/logging
    fn category(&self) -> ErrorCategory;
}

/// Error category for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Network/HTTP errors
    Network,
    /// Parsing/extraction errors
    Parsing,
    /// Storage/database errors
    Storage,
    /// Configuration errors
    Config,
    /// Scheduler/timing errors
    Scheduler,
    /// Ontology/knowledge graph errors
    Ontology,
    /// LLM/AI related errors
    Llm,
    /// Other/unknown errors
    Other,
}

impl ErrorCategory {
    /// Get category name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::Network => "network",
            ErrorCategory::Parsing => "parsing",
            ErrorCategory::Storage => "storage",
            ErrorCategory::Config => "config",
            ErrorCategory::Scheduler => "scheduler",
            ErrorCategory::Ontology => "ontology",
            ErrorCategory::Llm => "llm",
            ErrorCategory::Other => "other",
        }
    }
}

/// Unified error type for cross-domain operations
///
/// This enum wraps all domain-specific errors and provides a single error type
/// that can be used throughout the application.
#[derive(Error, Debug)]
pub enum Error {
    /// Crawler errors (fetch, parse)
    #[error("Crawler error: {0}")]
    Crawler(#[from] CrawlerError),

    /// Fetch-specific errors
    #[error("Fetch error: {0}")]
    Fetch(#[from] FetchError),

    /// Parse-specific errors
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    /// Ontology/knowledge graph errors
    #[error("Ontology error: {0}")]
    Ontology(#[from] OntologyError),

    /// Scheduler errors
    #[error("Scheduler error: {0}")]
    Scheduler(#[from] SchedulerError),

    /// Storage/database errors
    #[error("Storage error: {0}")]
    Storage(#[source] anyhow::Error),

    /// Configuration errors
    #[error("Config error: {0}")]
    Config(String),

    /// Generic errors with context
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl Error {
    /// Create a storage error
    pub fn storage(err: impl Into<anyhow::Error>) -> Self {
        Error::Storage(err.into())
    }

    /// Create a config error
    pub fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    /// Create an error with context
    pub fn context(msg: impl Into<String>) -> Self {
        Error::Other(anyhow::anyhow!("{}", msg.into()))
    }
}

impl BaramErrorTrait for Error {
    fn is_recoverable(&self) -> bool {
        match self {
            Error::Crawler(e) => e.is_recoverable(),
            Error::Fetch(e) => e.is_recoverable(),
            Error::Parse(e) => e.is_recoverable(),
            Error::Ontology(e) => e.is_recoverable(),
            Error::Scheduler(e) => e.is_recoverable(),
            Error::Storage(_) => true, // Storage errors are often transient
            Error::Config(_) => false, // Config errors require fix
            Error::Other(_) => false,
        }
    }

    fn korean_desc(&self) -> String {
        match self {
            Error::Crawler(e) => e.korean_desc().to_string(),
            Error::Fetch(e) => e.korean_desc().to_string(),
            Error::Parse(e) => e.korean_desc().to_string(),
            Error::Ontology(e) => e.korean_desc(),
            Error::Scheduler(e) => e.korean_desc(),
            Error::Storage(_) => "저장소 오류".to_string(),
            Error::Config(_) => "설정 오류".to_string(),
            Error::Other(_) => "기타 오류".to_string(),
        }
    }

    fn category(&self) -> ErrorCategory {
        match self {
            Error::Crawler(CrawlerError::Fetch(_)) => ErrorCategory::Network,
            Error::Crawler(CrawlerError::Parse(_)) => ErrorCategory::Parsing,
            Error::Crawler(_) => ErrorCategory::Other,
            Error::Fetch(_) => ErrorCategory::Network,
            Error::Parse(_) => ErrorCategory::Parsing,
            Error::Ontology(e) => match e {
                OntologyError::LlmResponseParseFailed { .. }
                | OntologyError::EmptyLlmResponse
                | OntologyError::InvalidLlmJson { .. } => ErrorCategory::Llm,
                OntologyError::StorageSaveFailed { .. }
                | OntologyError::StorageLoadFailed { .. }
                | OntologyError::StorageDirectoryNotFound { .. }
                | OntologyError::StorageDirectoryCreationFailed { .. } => ErrorCategory::Storage,
                OntologyError::InvalidConfig { .. } | OntologyError::MissingConfig { .. } => {
                    ErrorCategory::Config
                }
                _ => ErrorCategory::Ontology,
            },
            Error::Scheduler(_) => ErrorCategory::Scheduler,
            Error::Storage(_) => ErrorCategory::Storage,
            Error::Config(_) => ErrorCategory::Config,
            Error::Other(_) => ErrorCategory::Other,
        }
    }
}

// Implement BaramErrorTrait for domain errors
impl BaramErrorTrait for FetchError {
    fn is_recoverable(&self) -> bool {
        self.is_recoverable()
    }

    fn korean_desc(&self) -> String {
        self.korean_desc().to_string()
    }

    fn category(&self) -> ErrorCategory {
        ErrorCategory::Network
    }
}

impl BaramErrorTrait for ParseError {
    fn is_recoverable(&self) -> bool {
        self.is_recoverable()
    }

    fn korean_desc(&self) -> String {
        self.korean_desc().to_string()
    }

    fn category(&self) -> ErrorCategory {
        ErrorCategory::Parsing
    }
}

impl BaramErrorTrait for CrawlerError {
    fn is_recoverable(&self) -> bool {
        self.is_recoverable()
    }

    fn korean_desc(&self) -> String {
        self.korean_desc().to_string()
    }

    fn category(&self) -> ErrorCategory {
        match self {
            CrawlerError::Fetch(_) => ErrorCategory::Network,
            CrawlerError::Parse(_) => ErrorCategory::Parsing,
            _ => ErrorCategory::Other,
        }
    }
}

impl BaramErrorTrait for OntologyError {
    fn is_recoverable(&self) -> bool {
        self.is_recoverable()
    }

    fn korean_desc(&self) -> String {
        self.korean_desc()
    }

    fn category(&self) -> ErrorCategory {
        match self {
            OntologyError::LlmResponseParseFailed { .. }
            | OntologyError::EmptyLlmResponse
            | OntologyError::InvalidLlmJson { .. } => ErrorCategory::Llm,
            OntologyError::StorageSaveFailed { .. }
            | OntologyError::StorageLoadFailed { .. }
            | OntologyError::StorageDirectoryNotFound { .. }
            | OntologyError::StorageDirectoryCreationFailed { .. } => ErrorCategory::Storage,
            OntologyError::InvalidConfig { .. } | OntologyError::MissingConfig { .. } => {
                ErrorCategory::Config
            }
            _ => ErrorCategory::Ontology,
        }
    }
}

impl BaramErrorTrait for SchedulerError {
    fn is_recoverable(&self) -> bool {
        self.is_recoverable()
    }

    fn korean_desc(&self) -> String {
        self.korean_desc()
    }

    fn category(&self) -> ErrorCategory {
        ErrorCategory::Scheduler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_from_fetch_error() {
        let fetch_err = FetchError::Timeout;
        let err: Error = fetch_err.into();
        assert!(err.is_recoverable());
        assert_eq!(err.category(), ErrorCategory::Network);
    }

    #[test]
    fn test_error_from_parse_error() {
        let parse_err = ParseError::TitleNotFound;
        let err: Error = parse_err.into();
        assert!(!err.is_recoverable());
        assert_eq!(err.category(), ErrorCategory::Parsing);
    }

    #[test]
    fn test_error_from_scheduler_error() {
        let sched_err = SchedulerError::invalid_hour(25);
        let err: Error = sched_err.into();
        assert!(!err.is_recoverable());
        assert_eq!(err.category(), ErrorCategory::Scheduler);
    }

    #[test]
    fn test_error_storage() {
        let err = Error::storage(anyhow::anyhow!("DB connection failed"));
        assert!(err.is_recoverable());
        assert_eq!(err.category(), ErrorCategory::Storage);
    }

    #[test]
    fn test_error_config() {
        let err = Error::config("Invalid port number");
        assert!(!err.is_recoverable());
        assert_eq!(err.category(), ErrorCategory::Config);
    }

    #[test]
    fn test_korean_desc() {
        let err = Error::Fetch(FetchError::Timeout);
        assert!(!err.korean_desc().is_empty());
    }

    #[test]
    fn test_error_category_as_str() {
        assert_eq!(ErrorCategory::Network.as_str(), "network");
        assert_eq!(ErrorCategory::Storage.as_str(), "storage");
    }
}
