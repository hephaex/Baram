//! nTimes - Advanced Naver News Crawler
//!
//! A comprehensive news crawling system with vector search and ontology extraction capabilities.
//!
//! # Architecture
//!
//! The library is organized into several modules:
//!
//! - [`config`] - Configuration management and settings
//! - [`crawler`] - Web crawling logic with rate limiting
//! - [`parser`] - HTML parsing and data extraction
//! - [`models`] - Core data structures and types
//! - [`storage`] - Database operations (SQLite, PostgreSQL)
//! - [`embedding`] - Vector embedding and OpenSearch integration
//! - [`ontology`] - Knowledge graph and ontology extraction
//! - [`utils`] - Common utilities and helpers
//!
//! # Example
//!
//! ```no_run
//! use ntimes::crawler::Crawler;
//! use ntimes::config::Config;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = Config::from_env()?;
//!     let crawler = Crawler::new(config)?;
//!     // crawler.run().await?;
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod crawler;
pub mod embedding;
pub mod models;
pub mod ontology;
pub mod parser;
pub mod scheduler;
pub mod storage;
pub mod utils;

/// Re-export commonly used types
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::crawler::Crawler;
    pub use crate::models::{CrawlState, CrawlStats, NewsCategory, ParsedArticle};
    pub use crate::parser::Article;
    pub use crate::storage::{ArticleStorage, Database, MarkdownWriter};
}

// Direct re-exports for convenience
pub use models::{CrawlState, CrawlStats, NewsCategory, ParsedArticle};
