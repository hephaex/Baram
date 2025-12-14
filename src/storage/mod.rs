//! Database operations for SQLite and PostgreSQL
//!
//! This module handles data persistence with SQLite for metadata
//! and PostgreSQL for raw article data.

use anyhow::{Context, Result};
use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod, Runtime};
use rusqlite::Connection;
use std::path::Path;
use tokio_postgres::NoTls;

use crate::config::DatabaseConfig;
use crate::parser::Article;

/// Database management wrapper
pub struct Database {
    /// SQLite connection for metadata
    sqlite: Option<Connection>,

    /// PostgreSQL connection pool
    postgres: Option<Pool>,
}

impl Database {
    /// Create a new database instance
    pub fn new(_config: &DatabaseConfig) -> Result<Self> {
        Ok(Self {
            sqlite: None,
            postgres: None,
        })
    }

    /// Initialize SQLite connection
    pub fn init_sqlite(&mut self, path: &Path) -> Result<()> {
        let conn = Connection::open(path).context("Failed to open SQLite database")?;

        self.create_sqlite_schema(&conn)?;
        self.sqlite = Some(conn);

        Ok(())
    }

    /// Initialize PostgreSQL connection pool
    pub async fn init_postgres(&mut self, url: &str) -> Result<()> {
        let mut cfg = PoolConfig::new();
        cfg.url = Some(url.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .context("Failed to create PostgreSQL connection pool")?;

        self.postgres = Some(pool);

        Ok(())
    }

    /// Create SQLite schema
    fn create_sqlite_schema(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS crawl_metadata (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                content_hash TEXT NOT NULL,
                crawled_at DATETIME NOT NULL,
                status TEXT NOT NULL,
                error_message TEXT
            )",
            [],
        )
        .context("Failed to create crawl_metadata table")?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_crawl_metadata_url ON crawl_metadata(url)",
            [],
        )
        .context("Failed to create index")?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_crawl_metadata_status ON crawl_metadata(status)",
            [],
        )
        .context("Failed to create index")?;

        Ok(())
    }

    /// Store article in PostgreSQL
    pub async fn store_article(&self, _article: &Article) -> Result<()> {
        // TODO: Implement article storage
        Ok(())
    }

    /// Retrieve article by ID
    pub async fn get_article(&self, _id: &str) -> Result<Option<Article>> {
        // TODO: Implement article retrieval
        Ok(None)
    }

    /// Check if URL has been crawled
    pub fn is_url_crawled(&self, _url: &str) -> Result<bool> {
        // TODO: Implement URL checking
        Ok(false)
    }

    /// Mark URL as crawled
    pub fn mark_url_crawled(&self, _url: &str, _content_hash: &str) -> Result<()> {
        // TODO: Implement URL marking
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_sqlite_initialization() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = DatabaseConfig {
            sqlite_path: temp_file.path().to_path_buf(),
            postgres_url: String::from("postgresql://localhost/test"),
            pool_size: 5,
        };

        let mut db = Database::new(&config).unwrap();
        assert!(db.init_sqlite(temp_file.path()).is_ok());
    }
}
