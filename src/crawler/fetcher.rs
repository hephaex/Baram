//! HTTP fetcher with rate limiting and EUC-KR encoding support
//!
//! This module provides a specialized HTTP fetcher for Naver News articles
//! with features including:
//! - User-Agent rotation
//! - Rate limiting with governor
//! - Automatic retry with exponential backoff
//! - EUC-KR encoding detection and conversion
//! - Proper referer header generation

use crate::utils::error::FetchError;
use encoding_rs::{EUC_KR, UTF_8};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use rand::seq::SliceRandom;
use reqwest::{
    header::{
        HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, REFERER, USER_AGENT,
    },
    Client, Response,
};
use std::num::NonZeroU32;
use std::time::Duration;

/// Pool of realistic User-Agent strings for rotation
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
];

/// Naver News fetcher with advanced features
///
/// This fetcher handles the complexities of fetching Naver News articles,
/// including rate limiting, retry logic, and EUC-KR encoding.
pub struct NaverFetcher {
    /// HTTP client with configured timeout and compression
    client: Client,

    /// Rate limiter to control request frequency
    rate_limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,

    /// Maximum number of retry attempts for failed requests
    max_retries: u32,

    /// Base delay in milliseconds for exponential backoff
    base_delay_ms: u64,

    /// Optional base URL override for testing with mock servers
    base_url: Option<String>,
}

impl NaverFetcher {
    /// Create a new fetcher with default settings
    ///
    /// # Arguments
    ///
    /// * `requests_per_second` - Maximum number of requests per second
    ///
    /// # Errors
    ///
    /// Returns `FetchError::Http` if the HTTP client cannot be created
    pub fn new(requests_per_second: u32) -> Result<Self, FetchError> {
        Self::with_config(requests_per_second, 3, Duration::from_secs(30))
    }

    /// Create a new fetcher with custom configuration
    ///
    /// # Arguments
    ///
    /// * `requests_per_second` - Maximum number of requests per second
    /// * `max_retries` - Maximum number of retry attempts
    /// * `timeout` - Request timeout duration
    ///
    /// # Errors
    ///
    /// Returns `FetchError::Http` if the HTTP client cannot be created
    pub fn with_config(
        requests_per_second: u32,
        max_retries: u32,
        timeout: Duration,
    ) -> Result<Self, FetchError> {
        let client = Client::builder()
            .timeout(timeout)
            .gzip(true)
            .cookie_store(true)
            .build()?;

        let rate = NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::new(1).unwrap());
        let quota = Quota::per_second(rate);
        let rate_limiter = RateLimiter::direct(quota);

        Ok(Self {
            client,
            rate_limiter,
            max_retries,
            base_delay_ms: 1000,
            base_url: None,
        })
    }

    /// Create a new fetcher with a custom base URL for testing
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL to prepend to all requests
    /// * `requests_per_second` - Maximum number of requests per second
    ///
    /// # Errors
    ///
    /// Returns `FetchError::Http` if the HTTP client cannot be created
    pub fn with_base_url(base_url: &str, requests_per_second: u32) -> Result<Self, FetchError> {
        let mut fetcher = Self::new(requests_per_second)?;
        fetcher.base_url = Some(base_url.to_string());
        Ok(fetcher)
    }

    /// Create a new fetcher with custom config and base URL for testing
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL to prepend to all requests
    /// * `requests_per_second` - Maximum number of requests per second
    /// * `max_retries` - Maximum number of retry attempts
    /// * `timeout` - Request timeout duration
    ///
    /// # Errors
    ///
    /// Returns `FetchError::Http` if the HTTP client cannot be created
    pub fn with_config_and_base_url(
        base_url: &str,
        requests_per_second: u32,
        max_retries: u32,
        timeout: Duration,
    ) -> Result<Self, FetchError> {
        let mut fetcher = Self::with_config(requests_per_second, max_retries, timeout)?;
        fetcher.base_url = Some(base_url.to_string());
        Ok(fetcher)
    }

    /// Fetch an article with retry logic and rate limiting
    ///
    /// This is the main entry point for fetching articles. It handles:
    /// - Rate limiting
    /// - Retry with exponential backoff
    /// - EUC-KR encoding detection
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to fetch
    /// * `section_id` - Naver News section ID for referer generation
    ///
    /// # Errors
    ///
    /// Returns various `FetchError` variants depending on the failure mode
    pub async fn fetch_article(&self, url: &str, section_id: u32) -> Result<String, FetchError> {
        // Wait for rate limiter
        self.rate_limiter.until_ready().await;

        // Attempt fetch with retry logic
        self.fetch_with_retry(url, section_id).await
    }

    /// Fetch with exponential backoff retry logic
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to fetch
    /// * `section_id` - Naver News section ID for referer generation
    ///
    /// # Errors
    ///
    /// Returns `FetchError::MaxRetriesExceeded` if all retries fail
    async fn fetch_with_retry(&self, url: &str, section_id: u32) -> Result<String, FetchError> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            // Apply exponential backoff for retries
            if attempt > 0 {
                let delay = self.base_delay_ms * 2_u64.pow(attempt - 1);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            // Build referer based on section_id
            let referer = format!("https://news.naver.com/section/{section_id}");

            // Build headers with random user agent
            let headers = self.build_headers(&referer);

            // Construct full URL
            let full_url = if let Some(base) = &self.base_url {
                format!("{base}{url}")
            } else {
                url.to_string()
            };

            // Send request
            match self.client.get(&full_url).headers(headers).send().await {
                Ok(response) => {
                    let status = response.status();

                    // Check if we should retry based on status code
                    if status.is_success() {
                        // Success - decode and return
                        return self.decode_response(response).await;
                    } else if Self::should_retry(status.as_u16()) {
                        // Retryable error - continue loop
                        last_error = Some(FetchError::ServerError(status.as_u16()));
                        continue;
                    } else {
                        // Non-retryable error - return immediately
                        return Err(FetchError::ServerError(status.as_u16()));
                    }
                }
                Err(e) => {
                    // Check if error is timeout
                    if e.is_timeout() {
                        last_error = Some(FetchError::Timeout);
                    } else {
                        last_error = Some(FetchError::Http(e));
                    }
                }
            }
        }

        // All retries exhausted
        last_error
            .map(|_| Err(FetchError::MaxRetriesExceeded))
            .unwrap_or(Err(FetchError::MaxRetriesExceeded))
    }

    /// Determine if a status code should trigger a retry
    ///
    /// Retry on:
    /// - 429 (Too Many Requests)
    /// - 500 (Internal Server Error)
    /// - 502 (Bad Gateway)
    /// - 503 (Service Unavailable)
    /// - 504 (Gateway Timeout)
    ///
    /// Don't retry on:
    /// - 400 (Bad Request)
    /// - 401 (Unauthorized)
    /// - 403 (Forbidden)
    /// - 404 (Not Found)
    fn should_retry(status: u16) -> bool {
        matches!(status, 429 | 500 | 502 | 503 | 504)
    }

    /// Decode response body handling both UTF-8 and EUC-KR encodings
    ///
    /// # Arguments
    ///
    /// * `response` - The HTTP response to decode
    ///
    /// # Errors
    ///
    /// Returns `FetchError::Decode` if the content cannot be decoded
    async fn decode_response(&self, response: Response) -> Result<String, FetchError> {
        // Get Content-Type header and convert to owned String before consuming response
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Get response bytes
        let bytes = response.bytes().await?;

        // Decode bytes based on content type
        self.decode_bytes(&bytes, &content_type)
    }

    /// Decode bytes to UTF-8 string with encoding detection
    ///
    /// This method tries multiple strategies:
    /// 1. Check Content-Type header for charset
    /// 2. Try UTF-8 decoding
    /// 3. Fallback to EUC-KR if UTF-8 fails
    /// 4. Check HTML meta charset tag as last resort
    ///
    /// # Arguments
    ///
    /// * `bytes` - Raw response bytes
    /// * `content_type` - Content-Type header value
    ///
    /// # Errors
    ///
    /// Returns `FetchError::Decode` if decoding fails with all strategies
    pub fn decode_bytes(&self, bytes: &[u8], content_type: &str) -> Result<String, FetchError> {
        // Check if Content-Type specifies charset
        if content_type.to_lowercase().contains("charset=euc-kr") {
            return self.decode_euc_kr(bytes);
        }

        if content_type.to_lowercase().contains("charset=utf-8") {
            return self.decode_utf8(bytes);
        }

        // Try UTF-8 first (most common)
        if let Ok(text) = self.decode_utf8(bytes) {
            // Check if the decoded text looks valid (no replacement characters at start)
            if !text.starts_with('\u{FFFD}') {
                return Ok(text);
            }
        }

        // Fallback to EUC-KR for Naver News
        if let Ok(text) = self.decode_euc_kr(bytes) {
            return Ok(text);
        }

        // Check meta charset tag as last resort
        if let Ok(partial) = std::str::from_utf8(&bytes[..bytes.len().min(1024)]) {
            if partial.to_lowercase().contains("charset=euc-kr") {
                return self.decode_euc_kr(bytes);
            }
        }

        Err(FetchError::Decode(
            "Failed to decode content with UTF-8 or EUC-KR".to_string(),
        ))
    }

    /// Decode bytes as UTF-8
    fn decode_utf8(&self, bytes: &[u8]) -> Result<String, FetchError> {
        let (cow, _encoding, had_errors) = UTF_8.decode(bytes);

        if had_errors {
            return Err(FetchError::Decode("UTF-8 decoding errors".to_string()));
        }

        Ok(cow.into_owned())
    }

    /// Decode bytes as EUC-KR
    fn decode_euc_kr(&self, bytes: &[u8]) -> Result<String, FetchError> {
        let (cow, _encoding, had_errors) = EUC_KR.decode(bytes);

        if had_errors {
            return Err(FetchError::Decode("EUC-KR decoding errors".to_string()));
        }

        Ok(cow.into_owned())
    }

    /// Build HTTP headers for Naver News requests
    ///
    /// # Arguments
    ///
    /// * `referer` - The referer URL to include in headers
    fn build_headers(&self, referer: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();

        // Random user agent from pool
        let user_agent = self.random_user_agent();
        headers.insert(USER_AGENT, HeaderValue::from_static(user_agent));

        // Standard browser headers
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
            ),
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("ko-KR,ko;q=0.9,en-US;q=0.8,en;q=0.7"),
        );
        headers.insert(
            ACCEPT_ENCODING,
            HeaderValue::from_static("gzip, deflate, br"),
        );

        // Set referer
        if let Ok(referer_value) = HeaderValue::from_str(referer) {
            headers.insert(REFERER, referer_value);
        }

        headers
    }

    /// Get a random user agent from the pool
    fn random_user_agent(&self) -> &'static str {
        let mut rng = rand::thread_rng();
        USER_AGENTS.choose(&mut rng).unwrap_or(&USER_AGENTS[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agent_rotation() {
        let fetcher = NaverFetcher::new(10).unwrap();

        // Get multiple user agents and verify they're from our pool
        let mut agents = std::collections::HashSet::new();
        for _ in 0..100 {
            let agent = fetcher.random_user_agent();
            assert!(USER_AGENTS.contains(&agent));
            agents.insert(agent);
        }

        // With 100 iterations, we should see multiple different agents
        // (statistically very likely with 4 agents in pool)
        assert!(agents.len() > 1, "User agents should rotate");
    }

    #[test]
    fn test_decode_utf8() {
        let fetcher = NaverFetcher::new(10).unwrap();

        // Test valid UTF-8
        let text = "Hello, World! 안녕하세요";
        let bytes = text.as_bytes();
        let decoded = fetcher.decode_bytes(bytes, "text/html; charset=utf-8");

        assert!(decoded.is_ok());
        assert_eq!(decoded.unwrap(), text);
    }

    #[test]
    fn test_decode_euc_kr() {
        let fetcher = NaverFetcher::new(10).unwrap();

        // Test EUC-KR encoded Korean text
        // "안녕하세요" in EUC-KR encoding
        let euc_kr_bytes: &[u8] = &[0xbe, 0xc8, 0xb3, 0xe7, 0xc7, 0xcf, 0xbc, 0xbc, 0xbf, 0xe4];

        let decoded = fetcher.decode_bytes(euc_kr_bytes, "text/html; charset=euc-kr");

        assert!(decoded.is_ok());
        let text = decoded.unwrap();
        assert_eq!(text, "안녕하세요");
    }

    #[test]
    fn test_decode_euc_kr_fallback() {
        let fetcher = NaverFetcher::new(10).unwrap();

        // Test EUC-KR bytes without explicit charset in content-type
        // Should fallback to EUC-KR detection
        let euc_kr_bytes: &[u8] = &[0xbe, 0xc8, 0xb3, 0xe7, 0xc7, 0xcf, 0xbc, 0xbc, 0xbf, 0xe4];

        let decoded = fetcher.decode_bytes(euc_kr_bytes, "text/html");

        assert!(decoded.is_ok());
        let text = decoded.unwrap();
        assert_eq!(text, "안녕하세요");
    }

    #[test]
    fn test_referer_generation() {
        let fetcher = NaverFetcher::new(10).unwrap();

        let referer = "https://news.naver.com/section/100";
        let headers = fetcher.build_headers(referer);

        // Verify referer is set
        assert!(headers.contains_key(REFERER));
        assert_eq!(headers.get(REFERER).unwrap().to_str().unwrap(), referer);

        // Verify user agent is set
        assert!(headers.contains_key(USER_AGENT));

        // Verify accept headers
        assert!(headers.contains_key(ACCEPT));
        assert!(headers.contains_key(ACCEPT_LANGUAGE));
        assert!(headers.contains_key(ACCEPT_ENCODING));
    }

    #[test]
    fn test_should_retry() {
        // Retryable errors
        assert!(NaverFetcher::should_retry(429));
        assert!(NaverFetcher::should_retry(500));
        assert!(NaverFetcher::should_retry(502));
        assert!(NaverFetcher::should_retry(503));
        assert!(NaverFetcher::should_retry(504));

        // Non-retryable errors
        assert!(!NaverFetcher::should_retry(400));
        assert!(!NaverFetcher::should_retry(401));
        assert!(!NaverFetcher::should_retry(403));
        assert!(!NaverFetcher::should_retry(404));
        assert!(!NaverFetcher::should_retry(200));
    }

    #[test]
    fn test_fetcher_creation() {
        let fetcher = NaverFetcher::new(10);
        assert!(fetcher.is_ok());

        let fetcher = NaverFetcher::with_config(5, 3, Duration::from_secs(10));
        assert!(fetcher.is_ok());
    }

    #[test]
    fn test_fetcher_with_base_url() {
        let fetcher = NaverFetcher::with_base_url("http://localhost:8080", 10);
        assert!(fetcher.is_ok());

        let fetcher = fetcher.unwrap();
        assert_eq!(fetcher.base_url, Some("http://localhost:8080".to_string()));
    }

    #[test]
    fn test_decode_mixed_content() {
        let fetcher = NaverFetcher::new(10).unwrap();

        // Test UTF-8 content with English and Korean
        let utf8_text = "Title: 안녕 World";
        let decoded = fetcher.decode_bytes(utf8_text.as_bytes(), "text/html");
        assert!(decoded.is_ok());
        assert_eq!(decoded.unwrap(), utf8_text);
    }
}
