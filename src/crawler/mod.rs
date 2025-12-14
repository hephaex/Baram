//! Web crawling functionality with rate limiting
//!
//! This module implements the core crawling logic for fetching news articles
//! from Naver News with proper rate limiting and error handling.

pub mod comment;
pub mod fetcher;
pub mod headers;
pub mod list;
pub mod url;

use anyhow::{Context, Result};
use governor::{Quota, RateLimiter};
use reqwest::{Client, Response};
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::config::Config;

/// Main crawler structure
pub struct Crawler {
    /// HTTP client
    client: Client,

    /// Rate limiter
    rate_limiter: Arc<
        RateLimiter<
            governor::state::direct::NotKeyed,
            governor::state::InMemoryState,
            governor::clock::DefaultClock,
        >,
    >,

    /// Concurrency semaphore
    semaphore: Arc<Semaphore>,

    /// Configuration
    #[allow(dead_code)]
    config: Config,
}

impl Crawler {
    /// Create a new crawler instance
    pub fn new(config: Config) -> Result<Self> {
        config.validate().context("Invalid configuration")?;

        let client = Client::builder()
            .user_agent(&config.crawler.user_agent)
            .timeout(config.request_timeout())
            .cookie_store(config.crawler.enable_cookies)
            .gzip(true)
            .build()
            .context("Failed to create HTTP client")?;

        // Create rate limiter based on configuration
        let rate = NonZeroU32::new(config.crawler.rate_limit as u32)
            .context("Invalid rate limit value")?;
        let quota = Quota::per_second(rate);
        let rate_limiter = Arc::new(RateLimiter::direct(quota));

        let semaphore = Arc::new(Semaphore::new(config.crawler.max_concurrent_requests));

        Ok(Self {
            client,
            rate_limiter,
            semaphore,
            config,
        })
    }

    /// Fetch a URL with rate limiting
    pub async fn fetch(&self, url: &str) -> Result<Response> {
        // Wait for rate limiter
        self.rate_limiter.until_ready().await;

        // Acquire semaphore permit for concurrency control
        let _permit = self
            .semaphore
            .acquire()
            .await
            .context("Failed to acquire semaphore permit")?;

        tracing::debug!(url = %url, "Fetching URL");

        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to send request")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Request failed with status: {status}");
        }

        Ok(response)
    }

    /// Fetch and decode response body as text
    pub async fn fetch_text(&self, url: &str) -> Result<String> {
        let response = self.fetch(url).await?;
        let text = response
            .text()
            .await
            .context("Failed to read response body")?;
        Ok(text)
    }

    /// Crawl a list of URLs concurrently
    pub async fn crawl_urls(&self, urls: Vec<String>) -> Vec<Result<String>> {
        let futures = urls
            .into_iter()
            .map(|url| async move { self.fetch_text(&url).await });

        futures::future::join_all(futures).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawler_creation() {
        let config = Config::default();
        let crawler = Crawler::new(config);
        assert!(crawler.is_ok());
    }

    #[test]
    fn test_invalid_config_fails() {
        let mut config = Config::default();
        config.crawler.max_concurrent_requests = 0;
        let crawler = Crawler::new(config);
        assert!(crawler.is_err());
    }
}
