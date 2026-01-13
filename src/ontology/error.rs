//! Custom error types for ontology operations
//!
//! This module provides specific error variants for different failure modes
//! in the ontology extraction pipeline, enabling better error handling
//! and more informative error messages.
//!
//! Uses `thiserror` for standardized error type definitions consistent
//! with the rest of the codebase.

use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for ontology operations
pub type OntologyResult<T> = Result<T, OntologyError>;

/// Custom error type for ontology operations
#[derive(Error, Debug)]
pub enum OntologyError {
    // =========================================================================
    // Extraction Errors
    // =========================================================================
    /// Failed to extract entities from article
    #[error("Extraction failed for article '{article_id}': {reason}")]
    ExtractionFailed { article_id: String, reason: String },

    /// No entities found in article
    #[error("No entities found in article '{article_id}'")]
    NoEntitiesFound { article_id: String },

    /// Entity type is invalid or unknown
    #[error("Invalid entity type: '{value}'")]
    InvalidEntityType { value: String },

    /// Relation type is invalid or unknown
    #[error("Invalid relation type: '{value}'")]
    InvalidRelationType { value: String },

    // =========================================================================
    // LLM Response Errors
    // =========================================================================
    /// Failed to parse LLM response
    #[error("Failed to parse LLM response: {reason}")]
    LlmResponseParseFailed {
        reason: String,
        raw_response: Option<String>,
    },

    /// LLM response is empty or malformed
    #[error("LLM response is empty or malformed")]
    EmptyLlmResponse,

    /// LLM returned invalid JSON
    #[error("Invalid JSON in LLM response: {reason}")]
    InvalidLlmJson { reason: String },

    // =========================================================================
    // Verification Errors
    // =========================================================================
    /// Hallucination detected - entity not in source
    #[error("Hallucination detected for '{entity}': {reason}")]
    HallucinationDetected { entity: String, reason: String },

    /// Verification failed for relation
    #[error("Verification failed for relation ({subject} {predicate} {object}): {reason}")]
    VerificationFailed {
        subject: String,
        predicate: String,
        object: String,
        reason: String,
    },

    // =========================================================================
    // Linking Errors
    // =========================================================================
    /// Entity linking failed
    #[error("Entity linking failed for '{entity}': {reason}")]
    LinkingFailed { entity: String, reason: String },

    /// Knowledge base entry not found
    #[error("Knowledge base entry not found: '{canonical_name}'")]
    KnowledgeBaseEntryNotFound { canonical_name: String },

    /// Invalid knowledge base format
    #[error("Invalid knowledge base: {reason}")]
    InvalidKnowledgeBase { reason: String },

    // =========================================================================
    // Storage Errors
    // =========================================================================
    /// Storage directory does not exist
    #[error("Storage directory not found: {path:?}")]
    StorageDirectoryNotFound { path: PathBuf },

    /// Failed to create storage directory
    #[error("Failed to create storage directory {path:?}: {reason}")]
    StorageDirectoryCreationFailed { path: PathBuf, reason: String },

    /// Failed to save triples to storage
    #[error("Failed to save article '{article_id}': {reason}")]
    StorageSaveFailed { article_id: String, reason: String },

    /// Failed to load triples from storage
    #[error("Failed to load article '{article_id}': {reason}")]
    StorageLoadFailed { article_id: String, reason: String },

    /// Article not found in storage
    #[error("Article not found in storage: '{article_id}'")]
    ArticleNotFound { article_id: String },

    /// Index is corrupted or invalid
    #[error("Storage index corrupted: {reason}")]
    IndexCorrupted { reason: String },

    // =========================================================================
    // Serialization Errors
    // =========================================================================
    /// JSON serialization failed
    #[error("JSON serialization failed: {reason}")]
    JsonSerializationFailed { reason: String },

    /// JSON deserialization failed
    #[error("JSON deserialization failed: {reason}")]
    JsonDeserializationFailed { reason: String },

    /// RDF/Turtle export failed
    #[error("RDF export to {format} failed: {reason}")]
    RdfExportFailed { format: String, reason: String },

    // =========================================================================
    // Configuration Errors
    // =========================================================================
    /// Invalid configuration value
    #[error("Invalid config '{field}' = '{value}': {reason}")]
    InvalidConfig {
        field: String,
        value: String,
        reason: String,
    },

    /// Missing required configuration
    #[error("Missing required config: '{field}'")]
    MissingConfig { field: String },

    // =========================================================================
    // I/O Errors
    // =========================================================================
    /// File I/O error
    #[error("I/O error during {operation}{}: {source}", path.as_ref().map(|p| format!(" on {p:?}")).unwrap_or_default())]
    IoError {
        operation: String,
        path: Option<PathBuf>,
        #[source]
        source: io::Error,
    },

    // =========================================================================
    // Generic Errors
    // =========================================================================
    /// Generic error with context
    #[error("{context}{}", source.as_ref().map(|s| format!(": {s}")).unwrap_or_default())]
    Other {
        context: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

// ============================================================================
// Conversion implementations
// ============================================================================

impl From<io::Error> for OntologyError {
    fn from(err: io::Error) -> Self {
        OntologyError::IoError {
            operation: "unknown".to_string(),
            path: None,
            source: err,
        }
    }
}

impl From<serde_json::Error> for OntologyError {
    fn from(err: serde_json::Error) -> Self {
        if err.is_syntax() || err.is_data() {
            OntologyError::JsonDeserializationFailed {
                reason: err.to_string(),
            }
        } else {
            OntologyError::JsonSerializationFailed {
                reason: err.to_string(),
            }
        }
    }
}

impl From<anyhow::Error> for OntologyError {
    fn from(err: anyhow::Error) -> Self {
        OntologyError::Other {
            context: err.to_string(),
            source: None,
        }
    }
}

// ============================================================================
// Helper constructors
// ============================================================================

impl OntologyError {
    /// Create an extraction error
    pub fn extraction_failed(article_id: impl Into<String>, reason: impl Into<String>) -> Self {
        OntologyError::ExtractionFailed {
            article_id: article_id.into(),
            reason: reason.into(),
        }
    }

    /// Create an LLM parse error
    pub fn llm_parse_failed(reason: impl Into<String>, raw: Option<String>) -> Self {
        OntologyError::LlmResponseParseFailed {
            reason: reason.into(),
            raw_response: raw,
        }
    }

    /// Create a storage save error
    pub fn storage_save_failed(article_id: impl Into<String>, reason: impl Into<String>) -> Self {
        OntologyError::StorageSaveFailed {
            article_id: article_id.into(),
            reason: reason.into(),
        }
    }

    /// Create a storage load error
    pub fn storage_load_failed(article_id: impl Into<String>, reason: impl Into<String>) -> Self {
        OntologyError::StorageLoadFailed {
            article_id: article_id.into(),
            reason: reason.into(),
        }
    }

    /// Create an I/O error with context
    pub fn io_error(
        operation: impl Into<String>,
        path: Option<PathBuf>,
        source: io::Error,
    ) -> Self {
        OntologyError::IoError {
            operation: operation.into(),
            path,
            source,
        }
    }

    /// Create a config validation error
    pub fn invalid_config(
        field: impl Into<String>,
        value: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        OntologyError::InvalidConfig {
            field: field.into(),
            value: value.into(),
            reason: reason.into(),
        }
    }

    /// Create a generic error with context
    pub fn other(context: impl Into<String>) -> Self {
        OntologyError::Other {
            context: context.into(),
            source: None,
        }
    }

    /// Check if this is a recoverable error
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            OntologyError::NoEntitiesFound { .. }
                | OntologyError::HallucinationDetected { .. }
                | OntologyError::ArticleNotFound { .. }
        )
    }

    /// Get localized description of error
    pub fn localized_desc(&self) -> String {
        self.korean_desc()
    }

    /// Get Korean description of error
    pub fn korean_desc(&self) -> String {
        match self {
            OntologyError::ExtractionFailed { .. } => "추출 실패".to_string(),
            OntologyError::NoEntitiesFound { .. } => "엔티티 없음".to_string(),
            OntologyError::InvalidEntityType { .. } => "잘못된 엔티티 유형".to_string(),
            OntologyError::InvalidRelationType { .. } => "잘못된 관계 유형".to_string(),
            OntologyError::LlmResponseParseFailed { .. } => "LLM 응답 파싱 실패".to_string(),
            OntologyError::EmptyLlmResponse => "빈 LLM 응답".to_string(),
            OntologyError::InvalidLlmJson { .. } => "잘못된 LLM JSON".to_string(),
            OntologyError::HallucinationDetected { .. } => "환각 감지됨".to_string(),
            OntologyError::VerificationFailed { .. } => "검증 실패".to_string(),
            OntologyError::LinkingFailed { .. } => "연결 실패".to_string(),
            OntologyError::KnowledgeBaseEntryNotFound { .. } => "지식베이스 항목 없음".to_string(),
            OntologyError::InvalidKnowledgeBase { .. } => "잘못된 지식베이스".to_string(),
            OntologyError::StorageDirectoryNotFound { .. } => "저장소 디렉토리 없음".to_string(),
            OntologyError::StorageDirectoryCreationFailed { .. } => {
                "디렉토리 생성 실패".to_string()
            }
            OntologyError::StorageSaveFailed { .. } => "저장 실패".to_string(),
            OntologyError::StorageLoadFailed { .. } => "로드 실패".to_string(),
            OntologyError::ArticleNotFound { .. } => "기사 없음".to_string(),
            OntologyError::IndexCorrupted { .. } => "인덱스 손상".to_string(),
            OntologyError::JsonSerializationFailed { .. } => "JSON 직렬화 실패".to_string(),
            OntologyError::JsonDeserializationFailed { .. } => "JSON 역직렬화 실패".to_string(),
            OntologyError::RdfExportFailed { .. } => "RDF 내보내기 실패".to_string(),
            OntologyError::InvalidConfig { .. } => "잘못된 설정".to_string(),
            OntologyError::MissingConfig { .. } => "누락된 설정".to_string(),
            OntologyError::IoError { .. } => "I/O 오류".to_string(),
            OntologyError::Other { .. } => "기타 오류".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction_error_display() {
        let err = OntologyError::extraction_failed("article_001", "No content");
        assert!(err.to_string().contains("article_001"));
        assert!(err.to_string().contains("No content"));
    }

    #[test]
    fn test_llm_parse_error() {
        let err = OntologyError::llm_parse_failed("Invalid JSON", Some("raw data".to_string()));
        assert!(err.to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_storage_error() {
        let err = OntologyError::storage_save_failed("art_001", "Disk full");
        assert!(err.to_string().contains("art_001"));
        assert!(err.to_string().contains("Disk full"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "File not found");
        let ont_err: OntologyError = io_err.into();
        assert!(matches!(ont_err, OntologyError::IoError { .. }));
    }

    #[test]
    fn test_json_error_conversion() {
        let json_str = "invalid json";
        let result: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        if let Err(e) = result {
            let ont_err: OntologyError = e.into();
            assert!(matches!(
                ont_err,
                OntologyError::JsonDeserializationFailed { .. }
            ));
        }
    }

    #[test]
    fn test_is_recoverable() {
        let recoverable = OntologyError::NoEntitiesFound {
            article_id: "test".to_string(),
        };
        assert!(recoverable.is_recoverable());

        let not_recoverable = OntologyError::IndexCorrupted {
            reason: "test".to_string(),
        };
        assert!(!not_recoverable.is_recoverable());
    }

    #[test]
    fn test_korean_desc() {
        let err = OntologyError::NoEntitiesFound {
            article_id: "test".to_string(),
        };
        assert_eq!(err.korean_desc(), "엔티티 없음");
    }

    #[test]
    fn test_config_error() {
        let err = OntologyError::invalid_config("max_entities", "-1", "Must be positive");
        assert!(err.to_string().contains("max_entities"));
        assert!(err.to_string().contains("Must be positive"));
    }
}
