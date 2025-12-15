//! Error types for the scheduler module

use std::fmt;

/// Result type for scheduler operations
pub type SchedulerResult<T> = Result<T, SchedulerError>;

/// Scheduler-specific errors
#[derive(Debug)]
pub enum SchedulerError {
    /// Invalid instance ID provided
    InvalidInstance {
        id: String,
        valid_options: Vec<String>,
    },

    /// Invalid hour value (must be 0-23)
    InvalidHour {
        hour: u32,
    },

    /// Schedule not found for date
    ScheduleNotFound {
        date: String,
    },

    /// Failed to generate schedule
    ScheduleGenerationFailed {
        reason: String,
    },

    /// Trigger configuration error
    TriggerConfigError {
        field: String,
        reason: String,
    },

    /// Trigger execution error
    TriggerExecutionFailed {
        reason: String,
    },

    /// Cache error
    CacheError {
        operation: String,
        reason: String,
    },

    /// Serialization/deserialization error
    SerializationError {
        reason: String,
    },

    /// IO error
    IoError {
        operation: String,
        reason: String,
    },

    /// Invalid timezone
    InvalidTimezone {
        tz: String,
    },
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInstance { id, valid_options } => {
                write!(
                    f,
                    "Invalid instance ID '{}'. Valid options: {}",
                    id,
                    valid_options.join(", ")
                )
            }
            Self::InvalidHour { hour } => {
                write!(f, "Invalid hour '{}'. Must be 0-23", hour)
            }
            Self::ScheduleNotFound { date } => {
                write!(f, "Schedule not found for date: {}", date)
            }
            Self::ScheduleGenerationFailed { reason } => {
                write!(f, "Failed to generate schedule: {}", reason)
            }
            Self::TriggerConfigError { field, reason } => {
                write!(f, "Trigger config error in '{}': {}", field, reason)
            }
            Self::TriggerExecutionFailed { reason } => {
                write!(f, "Trigger execution failed: {}", reason)
            }
            Self::CacheError { operation, reason } => {
                write!(f, "Cache error during '{}': {}", operation, reason)
            }
            Self::SerializationError { reason } => {
                write!(f, "Serialization error: {}", reason)
            }
            Self::IoError { operation, reason } => {
                write!(f, "IO error during '{}': {}", operation, reason)
            }
            Self::InvalidTimezone { tz } => {
                write!(f, "Invalid timezone: {}", tz)
            }
        }
    }
}

impl std::error::Error for SchedulerError {}

impl From<serde_json::Error> for SchedulerError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError {
            reason: err.to_string(),
        }
    }
}

impl From<std::io::Error> for SchedulerError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError {
            operation: "unknown".to_string(),
            reason: err.to_string(),
        }
    }
}

impl SchedulerError {
    /// Create an invalid instance error
    pub fn invalid_instance(id: impl Into<String>) -> Self {
        Self::InvalidInstance {
            id: id.into(),
            valid_options: vec!["main".to_string(), "sub1".to_string(), "sub2".to_string()],
        }
    }

    /// Create an invalid hour error
    pub fn invalid_hour(hour: u32) -> Self {
        Self::InvalidHour { hour }
    }

    /// Create a schedule not found error
    pub fn schedule_not_found(date: impl Into<String>) -> Self {
        Self::ScheduleNotFound { date: date.into() }
    }

    /// Create a trigger config error
    pub fn trigger_config(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::TriggerConfigError {
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Create a cache error
    pub fn cache_error(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::CacheError {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Create an IO error with context
    pub fn io_error(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::IoError {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Get Korean description for the error
    pub fn korean_desc(&self) -> String {
        match self {
            Self::InvalidInstance { id, .. } => {
                format!("잘못된 인스턴스 ID: '{}'", id)
            }
            Self::InvalidHour { hour } => {
                format!("잘못된 시간: {} (0-23 범위여야 함)", hour)
            }
            Self::ScheduleNotFound { date } => {
                format!("스케줄을 찾을 수 없음: {}", date)
            }
            Self::ScheduleGenerationFailed { reason } => {
                format!("스케줄 생성 실패: {}", reason)
            }
            Self::TriggerConfigError { field, reason } => {
                format!("트리거 설정 오류 ({}): {}", field, reason)
            }
            Self::TriggerExecutionFailed { reason } => {
                format!("트리거 실행 실패: {}", reason)
            }
            Self::CacheError { operation, reason } => {
                format!("캐시 오류 ({}): {}", operation, reason)
            }
            Self::SerializationError { reason } => {
                format!("직렬화 오류: {}", reason)
            }
            Self::IoError { operation, reason } => {
                format!("입출력 오류 ({}): {}", operation, reason)
            }
            Self::InvalidTimezone { tz } => {
                format!("잘못된 시간대: {}", tz)
            }
        }
    }

    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::CacheError { .. }
                | Self::TriggerExecutionFailed { .. }
                | Self::IoError { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_instance_error() {
        let err = SchedulerError::invalid_instance("invalid");
        assert!(err.to_string().contains("invalid"));
        assert!(err.to_string().contains("main"));
    }

    #[test]
    fn test_invalid_hour_error() {
        let err = SchedulerError::invalid_hour(25);
        assert!(err.to_string().contains("25"));
        assert!(err.to_string().contains("0-23"));
    }

    #[test]
    fn test_korean_desc() {
        let err = SchedulerError::invalid_hour(25);
        let desc = err.korean_desc();
        assert!(desc.contains("잘못된 시간"));
    }

    #[test]
    fn test_is_recoverable() {
        let cache_err = SchedulerError::cache_error("read", "timeout");
        assert!(cache_err.is_recoverable());

        let invalid_err = SchedulerError::invalid_hour(25);
        assert!(!invalid_err.is_recoverable());
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let scheduler_err: SchedulerError = json_err.into();
        assert!(matches!(scheduler_err, SchedulerError::SerializationError { .. }));
    }
}
