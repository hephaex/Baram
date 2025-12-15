//! Coordinator configuration

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Configuration for the Coordinator server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Server bind address
    pub bind_address: SocketAddr,

    /// Heartbeat timeout in seconds
    pub heartbeat_timeout_secs: u64,

    /// Expected heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,

    /// Enable CORS for API
    pub enable_cors: bool,

    /// Maximum registered instances
    pub max_instances: usize,

    /// Schedule cache file path (optional)
    pub schedule_cache_path: Option<String>,

    /// Enable request logging
    pub enable_request_logging: bool,

    /// API key for authentication (optional)
    pub api_key: Option<String>,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:8080".parse().unwrap(),
            heartbeat_timeout_secs: 90,
            heartbeat_interval_secs: 30,
            enable_cors: true,
            max_instances: 10,
            schedule_cache_path: None,
            enable_request_logging: true,
            api_key: None,
        }
    }
}

impl CoordinatorConfig {
    /// Create a new config builder
    pub fn builder() -> CoordinatorConfigBuilder {
        CoordinatorConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.heartbeat_timeout_secs <= self.heartbeat_interval_secs {
            return Err(ConfigError::InvalidValue {
                field: "heartbeat_timeout_secs".to_string(),
                reason: "Timeout must be greater than interval".to_string(),
            });
        }

        if self.max_instances == 0 {
            return Err(ConfigError::InvalidValue {
                field: "max_instances".to_string(),
                reason: "Must allow at least 1 instance".to_string(),
            });
        }

        Ok(())
    }
}

/// Builder for CoordinatorConfig
#[derive(Debug, Default)]
pub struct CoordinatorConfigBuilder {
    bind_address: Option<SocketAddr>,
    heartbeat_timeout_secs: Option<u64>,
    heartbeat_interval_secs: Option<u64>,
    enable_cors: Option<bool>,
    max_instances: Option<usize>,
    schedule_cache_path: Option<String>,
    enable_request_logging: Option<bool>,
    api_key: Option<String>,
}

impl CoordinatorConfigBuilder {
    /// Set bind address
    pub fn bind_address(mut self, addr: SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }

    /// Set bind address from string
    pub fn bind_address_str(mut self, addr: &str) -> Result<Self, ConfigError> {
        self.bind_address = Some(addr.parse().map_err(|_| ConfigError::InvalidValue {
            field: "bind_address".to_string(),
            reason: format!("Invalid address: {}", addr),
        })?);
        Ok(self)
    }

    /// Set heartbeat timeout
    pub fn heartbeat_timeout_secs(mut self, secs: u64) -> Self {
        self.heartbeat_timeout_secs = Some(secs);
        self
    }

    /// Set heartbeat interval
    pub fn heartbeat_interval_secs(mut self, secs: u64) -> Self {
        self.heartbeat_interval_secs = Some(secs);
        self
    }

    /// Enable/disable CORS
    pub fn enable_cors(mut self, enable: bool) -> Self {
        self.enable_cors = Some(enable);
        self
    }

    /// Set max instances
    pub fn max_instances(mut self, max: usize) -> Self {
        self.max_instances = Some(max);
        self
    }

    /// Set schedule cache path
    pub fn schedule_cache_path(mut self, path: impl Into<String>) -> Self {
        self.schedule_cache_path = Some(path.into());
        self
    }

    /// Enable/disable request logging
    pub fn enable_request_logging(mut self, enable: bool) -> Self {
        self.enable_request_logging = Some(enable);
        self
    }

    /// Set API key
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Build the config
    pub fn build(self) -> Result<CoordinatorConfig, ConfigError> {
        let config = CoordinatorConfig {
            bind_address: self.bind_address.unwrap_or_else(|| "0.0.0.0:8080".parse().unwrap()),
            heartbeat_timeout_secs: self.heartbeat_timeout_secs.unwrap_or(90),
            heartbeat_interval_secs: self.heartbeat_interval_secs.unwrap_or(30),
            enable_cors: self.enable_cors.unwrap_or(true),
            max_instances: self.max_instances.unwrap_or(10),
            schedule_cache_path: self.schedule_cache_path,
            enable_request_logging: self.enable_request_logging.unwrap_or(true),
            api_key: self.api_key,
        };

        config.validate()?;
        Ok(config)
    }
}

/// Configuration errors
#[derive(Debug, Clone)]
pub enum ConfigError {
    InvalidValue { field: String, reason: String },
    MissingField { field: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidValue { field, reason } => {
                write!(f, "Invalid value for '{}': {}", field, reason)
            }
            Self::MissingField { field } => {
                write!(f, "Missing required field: {}", field)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CoordinatorConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.heartbeat_timeout_secs, 90);
        assert_eq!(config.heartbeat_interval_secs, 30);
    }

    #[test]
    fn test_config_builder() {
        let config = CoordinatorConfig::builder()
            .heartbeat_timeout_secs(120)
            .heartbeat_interval_secs(60)
            .max_instances(5)
            .build()
            .unwrap();

        assert_eq!(config.heartbeat_timeout_secs, 120);
        assert_eq!(config.heartbeat_interval_secs, 60);
        assert_eq!(config.max_instances, 5);
    }

    #[test]
    fn test_config_validation_fails() {
        // Timeout must be greater than interval
        let result = CoordinatorConfig::builder()
            .heartbeat_timeout_secs(30)
            .heartbeat_interval_secs(60)
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_config_builder_with_address() {
        let config = CoordinatorConfig::builder()
            .bind_address_str("127.0.0.1:9000")
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(config.bind_address.port(), 9000);
    }
}
