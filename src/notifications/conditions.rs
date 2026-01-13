//! Alert conditions for the notification system
//!
//! This module defines various conditions that can trigger alerts when met.
//! Each condition monitors specific patterns or anomalies in the crawled data.

use serde::{Deserialize, Serialize};

/// Alert condition types for monitoring
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertCondition {
    /// Triggered when a keyword appears more frequently than expected
    ///
    /// # Example
    ///
    /// Detect when "경제위기" appears more than 10 times in a 60-minute window:
    ///
    /// ```rust,ignore
    /// AlertCondition::KeywordSpike {
    ///     keyword: "경제위기".to_string(),
    ///     threshold: 10,
    ///     window_minutes: 60,
    /// }
    /// ```
    KeywordSpike {
        /// The keyword to monitor
        keyword: String,
        /// Minimum occurrences to trigger alert
        threshold: u32,
        /// Time window in minutes
        window_minutes: u32,
    },

    /// Triggered when an entity (person, organization) is mentioned unusually often
    ///
    /// # Example
    ///
    /// Detect when "삼성전자" is mentioned more than 20 times in 30 minutes:
    ///
    /// ```rust,ignore
    /// AlertCondition::EntitySurge {
    ///     entity: "삼성전자".to_string(),
    ///     threshold: 20,
    ///     window_minutes: 30,
    /// }
    /// ```
    EntitySurge {
        /// The entity to monitor (person, organization, location)
        entity: String,
        /// Minimum mentions to trigger alert
        threshold: u32,
        /// Time window in minutes
        window_minutes: u32,
    },

    /// Triggered when article volume deviates from the norm
    ///
    /// # Example
    ///
    /// Detect when politics articles exceed 2 standard deviations:
    ///
    /// ```rust,ignore
    /// AlertCondition::VolumeAnomaly {
    ///     category: "politics".to_string(),
    ///     threshold_stddev: 2.0,
    /// }
    /// ```
    VolumeAnomaly {
        /// News category to monitor ("politics", "economy", etc.)
        category: String,
        /// Number of standard deviations from mean
        threshold_stddev: f64,
    },

    /// Triggered when crawl error rate exceeds threshold
    ///
    /// # Example
    ///
    /// Alert when error rate exceeds 10% over 15 minutes:
    ///
    /// ```rust,ignore
    /// AlertCondition::ErrorRateThreshold {
    ///     threshold_percent: 10.0,
    ///     window_minutes: 15,
    /// }
    /// ```
    ErrorRateThreshold {
        /// Maximum acceptable error rate (0.0 - 100.0)
        threshold_percent: f64,
        /// Time window in minutes
        window_minutes: u32,
    },

    /// Triggered when crawl throughput drops below threshold
    ///
    /// # Example
    ///
    /// Alert when throughput drops below 5 articles/minute:
    ///
    /// ```rust,ignore
    /// AlertCondition::ThroughputDrop {
    ///     threshold_per_minute: 5.0,
    ///     window_minutes: 10,
    /// }
    /// ```
    ThroughputDrop {
        /// Minimum articles per minute
        threshold_per_minute: f64,
        /// Time window in minutes
        window_minutes: u32,
    },

    /// Triggered when a specific source fails repeatedly
    ///
    /// # Example
    ///
    /// Alert when naver.com fails 5 times:
    ///
    /// ```rust,ignore
    /// AlertCondition::SourceFailure {
    ///     source: "naver.com".to_string(),
    ///     failure_count: 5,
    /// }
    /// ```
    SourceFailure {
        /// Source identifier (domain, publisher ID, etc.)
        source: String,
        /// Number of consecutive failures
        failure_count: u32,
    },

    /// Custom condition with user-defined logic
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// AlertCondition::Custom {
    ///     name: "duplicate_detection".to_string(),
    ///     description: "High duplicate article rate detected".to_string(),
    ///     parameters: HashMap::from([
    ///         ("threshold".to_string(), "50".to_string()),
    ///         ("window".to_string(), "60".to_string()),
    ///     ]),
    /// }
    /// ```
    Custom {
        /// Condition name
        name: String,
        /// Human-readable description
        description: String,
        /// Condition-specific parameters
        parameters: std::collections::HashMap<String, String>,
    },
}

impl AlertCondition {
    /// Get a human-readable description of the condition
    pub fn description(&self) -> String {
        match self {
            Self::KeywordSpike {
                keyword,
                threshold,
                window_minutes,
            } => {
                format!(
                    "Keyword '{keyword}' appears >{threshold} times in {window_minutes}min window"
                )
            }
            Self::EntitySurge {
                entity,
                threshold,
                window_minutes,
            } => {
                format!(
                    "Entity '{entity}' mentioned >{threshold} times in {window_minutes}min window"
                )
            }
            Self::VolumeAnomaly {
                category,
                threshold_stddev,
            } => {
                format!(
                    "Article volume in '{category}' exceeds {threshold_stddev}σ from mean"
                )
            }
            Self::ErrorRateThreshold {
                threshold_percent,
                window_minutes,
            } => {
                format!(
                    "Error rate >{threshold_percent}% in {window_minutes}min window"
                )
            }
            Self::ThroughputDrop {
                threshold_per_minute,
                window_minutes,
            } => {
                format!(
                    "Throughput <{threshold_per_minute} articles/min in {window_minutes}min window"
                )
            }
            Self::SourceFailure {
                source,
                failure_count,
            } => {
                format!("Source '{source}' failed {failure_count} consecutive times")
            }
            Self::Custom {
                name,
                description,
                ..
            } => {
                format!("{name}: {description}")
            }
        }
    }

    /// Get Korean description of the condition
    pub fn korean_description(&self) -> String {
        match self {
            Self::KeywordSpike {
                keyword,
                threshold,
                window_minutes,
            } => {
                format!(
                    "키워드 '{keyword}'가 {window_minutes}분 내 {threshold}회 이상 출현"
                )
            }
            Self::EntitySurge {
                entity,
                threshold,
                window_minutes,
            } => {
                format!(
                    "개체명 '{entity}'가 {window_minutes}분 내 {threshold}회 이상 언급"
                )
            }
            Self::VolumeAnomaly {
                category,
                threshold_stddev,
            } => {
                format!(
                    "'{category}' 카테고리 기사량이 평균에서 {threshold_stddev} 표준편차 초과"
                )
            }
            Self::ErrorRateThreshold {
                threshold_percent,
                window_minutes,
            } => {
                format!(
                    "{window_minutes}분 내 오류율 {threshold_percent}% 초과"
                )
            }
            Self::ThroughputDrop {
                threshold_per_minute,
                window_minutes,
            } => {
                format!(
                    "{window_minutes}분 내 처리량이 분당 {threshold_per_minute}건 미만"
                )
            }
            Self::SourceFailure {
                source,
                failure_count,
            } => {
                format!("소스 '{source}' 연속 {failure_count}회 실패")
            }
            Self::Custom { description, .. } => description.clone(),
        }
    }

    /// Get the condition type as a string
    pub fn condition_type(&self) -> &'static str {
        match self {
            Self::KeywordSpike { .. } => "keyword_spike",
            Self::EntitySurge { .. } => "entity_surge",
            Self::VolumeAnomaly { .. } => "volume_anomaly",
            Self::ErrorRateThreshold { .. } => "error_rate_threshold",
            Self::ThroughputDrop { .. } => "throughput_drop",
            Self::SourceFailure { .. } => "source_failure",
            Self::Custom { .. } => "custom",
        }
    }

    /// Get the time window in minutes (if applicable)
    pub fn window_minutes(&self) -> Option<u32> {
        match self {
            Self::KeywordSpike {
                window_minutes, ..
            }
            | Self::EntitySurge {
                window_minutes, ..
            }
            | Self::ErrorRateThreshold {
                window_minutes, ..
            }
            | Self::ThroughputDrop {
                window_minutes, ..
            } => Some(*window_minutes),
            Self::VolumeAnomaly { .. }
            | Self::SourceFailure { .. }
            | Self::Custom { .. } => None,
        }
    }

    /// Validate condition parameters
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::KeywordSpike {
                keyword,
                threshold,
                window_minutes,
            } => {
                if keyword.is_empty() {
                    return Err("Keyword cannot be empty".to_string());
                }
                if *threshold == 0 {
                    return Err("Threshold must be greater than 0".to_string());
                }
                if *window_minutes == 0 {
                    return Err("Window minutes must be greater than 0".to_string());
                }
            }
            Self::EntitySurge {
                entity,
                threshold,
                window_minutes,
            } => {
                if entity.is_empty() {
                    return Err("Entity cannot be empty".to_string());
                }
                if *threshold == 0 {
                    return Err("Threshold must be greater than 0".to_string());
                }
                if *window_minutes == 0 {
                    return Err("Window minutes must be greater than 0".to_string());
                }
            }
            Self::VolumeAnomaly {
                category,
                threshold_stddev,
            } => {
                if category.is_empty() {
                    return Err("Category cannot be empty".to_string());
                }
                if *threshold_stddev <= 0.0 {
                    return Err("Threshold stddev must be greater than 0".to_string());
                }
            }
            Self::ErrorRateThreshold {
                threshold_percent,
                window_minutes,
            } => {
                if !(*threshold_percent >= 0.0 && *threshold_percent <= 100.0) {
                    return Err("Threshold percent must be between 0 and 100".to_string());
                }
                if *window_minutes == 0 {
                    return Err("Window minutes must be greater than 0".to_string());
                }
            }
            Self::ThroughputDrop {
                threshold_per_minute,
                window_minutes,
            } => {
                if *threshold_per_minute <= 0.0 {
                    return Err("Threshold per minute must be greater than 0".to_string());
                }
                if *window_minutes == 0 {
                    return Err("Window minutes must be greater than 0".to_string());
                }
            }
            Self::SourceFailure {
                source,
                failure_count,
            } => {
                if source.is_empty() {
                    return Err("Source cannot be empty".to_string());
                }
                if *failure_count == 0 {
                    return Err("Failure count must be greater than 0".to_string());
                }
            }
            Self::Custom { name, .. } => {
                if name.is_empty() {
                    return Err("Custom condition name cannot be empty".to_string());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_keyword_spike_description() {
        let condition = AlertCondition::KeywordSpike {
            keyword: "경제위기".to_string(),
            threshold: 10,
            window_minutes: 60,
        };

        let desc = condition.description();
        assert!(desc.contains("경제위기"));
        assert!(desc.contains("10"));
        assert!(desc.contains("60"));

        let korean = condition.korean_description();
        assert!(korean.contains("키워드"));
    }

    #[test]
    fn test_entity_surge_description() {
        let condition = AlertCondition::EntitySurge {
            entity: "삼성전자".to_string(),
            threshold: 20,
            window_minutes: 30,
        };

        assert_eq!(condition.condition_type(), "entity_surge");
        assert_eq!(condition.window_minutes(), Some(30));
    }

    #[test]
    fn test_volume_anomaly_description() {
        let condition = AlertCondition::VolumeAnomaly {
            category: "politics".to_string(),
            threshold_stddev: 2.5,
        };

        let desc = condition.description();
        assert!(desc.contains("politics"));
        assert!(desc.contains("2.5"));
        assert_eq!(condition.window_minutes(), None);
    }

    #[test]
    fn test_error_rate_threshold() {
        let condition = AlertCondition::ErrorRateThreshold {
            threshold_percent: 15.0,
            window_minutes: 10,
        };

        assert_eq!(condition.condition_type(), "error_rate_threshold");
        assert_eq!(condition.window_minutes(), Some(10));
    }

    #[test]
    fn test_throughput_drop() {
        let condition = AlertCondition::ThroughputDrop {
            threshold_per_minute: 5.0,
            window_minutes: 15,
        };

        let korean = condition.korean_description();
        assert!(korean.contains("처리량"));
    }

    #[test]
    fn test_source_failure() {
        let condition = AlertCondition::SourceFailure {
            source: "naver.com".to_string(),
            failure_count: 5,
        };

        let desc = condition.description();
        assert!(desc.contains("naver.com"));
        assert!(desc.contains("5"));
    }

    #[test]
    fn test_custom_condition() {
        let mut params = HashMap::new();
        params.insert("threshold".to_string(), "50".to_string());

        let condition = AlertCondition::Custom {
            name: "custom_check".to_string(),
            description: "Custom monitoring rule".to_string(),
            parameters: params,
        };

        assert_eq!(condition.condition_type(), "custom");
        assert!(condition.description().contains("custom_check"));
    }

    #[test]
    fn test_condition_validation() {
        // Valid condition
        let valid = AlertCondition::KeywordSpike {
            keyword: "test".to_string(),
            threshold: 5,
            window_minutes: 30,
        };
        assert!(valid.validate().is_ok());

        // Invalid: empty keyword
        let invalid_keyword = AlertCondition::KeywordSpike {
            keyword: String::new(),
            threshold: 5,
            window_minutes: 30,
        };
        assert!(invalid_keyword.validate().is_err());

        // Invalid: zero threshold
        let invalid_threshold = AlertCondition::KeywordSpike {
            keyword: "test".to_string(),
            threshold: 0,
            window_minutes: 30,
        };
        assert!(invalid_threshold.validate().is_err());

        // Invalid: zero window
        let invalid_window = AlertCondition::KeywordSpike {
            keyword: "test".to_string(),
            threshold: 5,
            window_minutes: 0,
        };
        assert!(invalid_window.validate().is_err());

        // Invalid: error rate out of range
        let invalid_error_rate = AlertCondition::ErrorRateThreshold {
            threshold_percent: 150.0,
            window_minutes: 10,
        };
        assert!(invalid_error_rate.validate().is_err());
    }

    #[test]
    fn test_condition_serialization() {
        let condition = AlertCondition::VolumeAnomaly {
            category: "economy".to_string(),
            threshold_stddev: 3.0,
        };

        let json = serde_json::to_string(&condition).unwrap();
        let deserialized: AlertCondition = serde_json::from_str(&json).unwrap();

        assert_eq!(condition, deserialized);
    }
}
