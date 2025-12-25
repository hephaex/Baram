//! Configuration management for baram crawler
//!
//! This module handles loading and validating configuration from environment variables,
//! files, and command-line arguments.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Crawler configuration
    pub crawler: CrawlerConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// OpenSearch configuration
    pub opensearch: OpenSearchConfig,

    /// Logging configuration
    pub logging: LoggingConfig,
}

/// Crawler-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    /// Maximum number of concurrent requests
    pub max_concurrent_requests: usize,

    /// Rate limit (requests per second)
    pub rate_limit: f64,

    /// Request timeout in seconds
    pub request_timeout_secs: u64,

    /// User agent string
    pub user_agent: String,

    /// Enable cookie persistence
    pub enable_cookies: bool,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// SQLite database path
    pub sqlite_path: PathBuf,

    /// PostgreSQL connection string
    pub postgres_url: String,

    /// Maximum pool size
    pub pool_size: usize,
}

/// OpenSearch configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSearchConfig {
    /// OpenSearch endpoint URL
    pub url: String,

    /// Index name
    pub index_name: String,

    /// Username (optional)
    pub username: Option<String>,

    /// Password (optional)
    pub password: Option<String>,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,

    /// Log format (text, json)
    pub format: String,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let max_concurrent_requests = std::env::var("BARAM_MAX_CONCURRENT_REQUESTS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);

        let rate_limit = std::env::var("BARAM_RATE_LIMIT")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(2.0);

        let request_timeout_secs = std::env::var("BARAM_REQUEST_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);

        let user_agent = std::env::var("BARAM_USER_AGENT")
            .unwrap_or_else(|_| format!("baram/{}", env!("CARGO_PKG_VERSION")));

        let sqlite_path = std::env::var("BARAM_SQLITE_PATH")
            .unwrap_or_else(|_| String::from("data/metadata.db"))
            .into();

        let postgres_url = std::env::var("POSTGRES_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| String::from("postgresql://localhost/baram"));

        let opensearch_url = std::env::var("OPENSEARCH_URL")
            .unwrap_or_else(|_| String::from("http://localhost:9200"));

        let opensearch_index =
            std::env::var("OPENSEARCH_INDEX").unwrap_or_else(|_| String::from("baram-articles"));

        let opensearch_username = std::env::var("OPENSEARCH_USERNAME").ok();
        let opensearch_password = std::env::var("OPENSEARCH_PASSWORD").ok();

        let log_level = std::env::var("BARAM_LOG_LEVEL").unwrap_or_else(|_| String::from("info"));

        let log_format = std::env::var("BARAM_LOG_FORMAT").unwrap_or_else(|_| String::from("text"));

        Ok(Self {
            crawler: CrawlerConfig {
                max_concurrent_requests,
                rate_limit,
                request_timeout_secs,
                user_agent,
                enable_cookies: true,
            },
            database: DatabaseConfig {
                sqlite_path,
                postgres_url,
                pool_size: 10,
            },
            opensearch: OpenSearchConfig {
                url: opensearch_url,
                index_name: opensearch_index,
                username: opensearch_username,
                password: opensearch_password,
            },
            logging: LoggingConfig {
                level: log_level,
                format: log_format,
            },
        })
    }

    /// Load configuration from a file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML config file: {}", path.display()))?;

        Ok(config)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.crawler.max_concurrent_requests == 0 {
            anyhow::bail!("max_concurrent_requests must be greater than 0");
        }

        if self.crawler.rate_limit <= 0.0 {
            anyhow::bail!("rate_limit must be positive");
        }

        if self.database.pool_size == 0 {
            anyhow::bail!("pool_size must be greater than 0");
        }

        Ok(())
    }

    /// Get request timeout as Duration
    #[must_use]
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.crawler.request_timeout_secs)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            crawler: CrawlerConfig {
                max_concurrent_requests: 10,
                rate_limit: 2.0,
                request_timeout_secs: 30,
                user_agent: format!("baram/{}", env!("CARGO_PKG_VERSION")),
                enable_cookies: true,
            },
            database: DatabaseConfig {
                sqlite_path: PathBuf::from("data/metadata.db"),
                postgres_url: String::from("postgresql://localhost/baram"),
                pool_size: 10,
            },
            opensearch: OpenSearchConfig {
                url: String::from("http://localhost:9200"),
                index_name: String::from("baram-articles"),
                username: None,
                password: None,
            },
            logging: LoggingConfig {
                level: String::from("info"),
                format: String::from("text"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_concurrent_requests() {
        let mut config = Config::default();
        config.crawler.max_concurrent_requests = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_request_timeout_conversion() {
        let config = Config::default();
        let timeout = config.request_timeout();
        assert_eq!(timeout, Duration::from_secs(30));
    }
}
