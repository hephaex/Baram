//! URL extraction and normalization for Naver News articles
//!
//! This module provides functionality for extracting, normalizing, and validating
//! Naver News article URLs with various formats and security considerations.

use regex::Regex;
use std::collections::HashSet;
use url::Url;

use crate::utils::error::ParseError;

/// Article URL extractor from list pages
///
/// Handles multiple URL formats:
/// - Modern format: `/mnews/article/{oid}/{aid}`
/// - Standard format: `/article/{oid}/{aid}`
/// - Old format: `/news/read.naver?oid={oid}&aid={aid}`
/// - Mobile URLs: `https://m.news.naver.com/...`
pub struct UrlExtractor {
    /// Pattern for article URLs: /mnews/article/{oid}/{aid}
    article_pattern: Regex,
    /// Pattern for old format: /news/read.naver?...
    old_format_pattern: Regex,
    /// Pattern to detect mobile URLs
    /// Reserved for future mobile URL detection features
    #[allow(dead_code)]
    mobile_pattern: Regex,
}

impl UrlExtractor {
    /// Create a new URL extractor with compiled regex patterns
    #[must_use]
    pub fn new() -> Self {
        Self {
            // Matches: /mnews/article/001/0014123456 or /article/001/0014123456
            article_pattern: Regex::new(r"/(?:mnews/)?article/(\d{3})/(\d{10,})").unwrap(),
            // Matches old format: oid=001&aid=0014123456
            old_format_pattern: Regex::new(r"oid=(\d{3})&aid=(\d{10,})").unwrap(),
            // Matches mobile URL prefix
            mobile_pattern: Regex::new(r"^https?://m\.").unwrap(),
        }
    }

    /// Extract article URLs from list page HTML
    ///
    /// Parses HTML content to find all Naver News article URLs, normalizes them,
    /// and returns a deduplicated vector.
    ///
    /// # Arguments
    ///
    /// * `html` - HTML content of a news list page
    ///
    /// # Returns
    ///
    /// Vector of normalized, deduplicated article URLs
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::UrlExtractor;
    ///
    /// let extractor = UrlExtractor::new();
    /// let html = r#"<a href="https://n.news.naver.com/mnews/article/001/0014123456">Article</a>"#;
    /// let urls = extractor.extract_urls(html);
    /// assert!(!urls.is_empty());
    /// ```
    pub fn extract_urls(&self, html: &str) -> Vec<String> {
        let mut urls = HashSet::new();

        // Extract all href attributes
        let href_pattern = Regex::new(r#"href=["']([^"']+)["']"#).unwrap();

        for cap in href_pattern.captures_iter(html) {
            if let Some(url_match) = cap.get(1) {
                let url = url_match.as_str();

                // Try to normalize the URL
                if let Some(normalized) = self.normalize_url(url) {
                    // Validate before adding
                    if self.is_valid_article_url(&normalized) {
                        urls.insert(normalized);
                    }
                }
            }
        }

        // Convert to sorted vector for deterministic output
        let mut result: Vec<String> = urls.into_iter().collect();
        result.sort();
        result
    }

    /// Normalize URL to standard desktop format
    ///
    /// Converts mobile URLs and various formats to the standard format:
    /// `https://n.news.naver.com/mnews/article/{oid}/{aid}`
    ///
    /// # Arguments
    ///
    /// * `url` - URL to normalize (can be relative or absolute)
    ///
    /// # Returns
    ///
    /// Normalized URL, or `None` if the URL cannot be normalized
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::UrlExtractor;
    ///
    /// let extractor = UrlExtractor::new();
    /// let mobile = "https://m.news.naver.com/article/001/0014123456";
    /// let normalized = extractor.normalize_url(mobile).unwrap();
    /// assert!(normalized.starts_with("https://n.news.naver.com"));
    /// ```
    pub fn normalize_url(&self, url: &str) -> Option<String> {
        // Extract IDs first
        let (oid, aid) = self.extract_ids(url).ok()?;

        // Return standard format
        Some(format!(
            "https://n.news.naver.com/mnews/article/{oid}/{aid}"
        ))
    }

    /// Extract oid (publisher ID) and aid (article ID) from URL
    ///
    /// Supports multiple URL formats and query parameters.
    ///
    /// # Arguments
    ///
    /// * `url` - URL containing article identifiers
    ///
    /// # Returns
    ///
    /// Tuple of (oid, aid) strings
    ///
    /// # Errors
    ///
    /// Returns `ParseError::IdExtractionFailed` if IDs cannot be extracted
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::UrlExtractor;
    ///
    /// let extractor = UrlExtractor::new();
    /// let url = "https://n.news.naver.com/mnews/article/001/0014123456";
    /// let (oid, aid) = extractor.extract_ids(url).unwrap();
    /// assert_eq!(oid, "001");
    /// assert_eq!(aid, "0014123456");
    /// ```
    pub fn extract_ids(&self, url: &str) -> Result<(String, String), ParseError> {
        // Try modern format first
        if let Some(caps) = self.article_pattern.captures(url) {
            let oid = caps.get(1).unwrap().as_str().to_string();
            let aid = caps.get(2).unwrap().as_str().to_string();
            return Ok((oid, aid));
        }

        // Try old format
        if let Some(caps) = self.old_format_pattern.captures(url) {
            let oid = caps.get(1).unwrap().as_str().to_string();
            let aid = caps.get(2).unwrap().as_str().to_string();
            return Ok((oid, aid));
        }

        Err(ParseError::IdExtractionFailed)
    }

    /// Validate URL is a valid Naver news article URL
    ///
    /// Checks if the URL contains valid article identifiers and is from
    /// an allowed domain.
    ///
    /// # Arguments
    ///
    /// * `url` - URL to validate
    ///
    /// # Returns
    ///
    /// `true` if the URL is a valid article URL
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::UrlExtractor;
    ///
    /// let extractor = UrlExtractor::new();
    /// assert!(extractor.is_valid_article_url(
    ///     "https://n.news.naver.com/mnews/article/001/0014123456"
    /// ));
    /// assert!(!extractor.is_valid_article_url("https://google.com"));
    /// ```
    pub fn is_valid_article_url(&self, url: &str) -> bool {
        // Must be able to extract IDs
        if self.extract_ids(url).is_err() {
            return false;
        }

        // Must be from allowed domain
        if !validators::is_allowed_domain(url) {
            return false;
        }

        // Must pass safety checks
        validators::is_safe_url(url)
    }

    /// Convert relative URL to absolute
    ///
    /// # Arguments
    ///
    /// * `url` - URL to convert (can be relative or absolute)
    /// * `base` - Base URL for resolving relative URLs
    ///
    /// # Returns
    ///
    /// Absolute URL string
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::UrlExtractor;
    ///
    /// let extractor = UrlExtractor::new();
    /// let absolute = extractor.to_absolute(
    ///     "/mnews/article/001/0014123456",
    ///     "https://n.news.naver.com"
    /// );
    /// assert!(absolute.starts_with("https://"));
    /// ```
    pub fn to_absolute(&self, url: &str, base: &str) -> String {
        // If already absolute, return as-is
        if url.starts_with("http://") || url.starts_with("https://") {
            return url.to_string();
        }

        // Parse base URL
        let base_url = match Url::parse(base) {
            Ok(u) => u,
            Err(_) => return url.to_string(),
        };

        // Join with relative URL
        match base_url.join(url) {
            Ok(absolute) => absolute.to_string(),
            Err(_) => url.to_string(),
        }
    }
}

impl Default for UrlExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// URL validation and security functions
pub mod validators {
    use url::Url;

    /// Allowed Naver news domains
    const ALLOWED_DOMAINS: &[&str] = &[
        "n.news.naver.com",
        "news.naver.com",
        "m.news.naver.com",
        "entertain.naver.com",
        "sports.naver.com",
        "sports.news.naver.com",
    ];

    /// Check if URL is from allowed Naver domains
    ///
    /// # Arguments
    ///
    /// * `url` - URL to check
    ///
    /// # Returns
    ///
    /// `true` if the URL is from an allowed domain
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::validators;
    ///
    /// assert!(validators::is_allowed_domain("https://n.news.naver.com/article/001/123"));
    /// assert!(!validators::is_allowed_domain("https://evil.com/fake"));
    /// ```
    pub fn is_allowed_domain(url: &str) -> bool {
        let parsed = match Url::parse(url) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let host = match parsed.host_str() {
            Some(h) => h,
            None => return false,
        };

        ALLOWED_DOMAINS.contains(&host)
    }

    /// SSRF prevention - block internal/private IPs
    ///
    /// Blocks dangerous URLs including:
    /// - localhost, 127.0.0.1
    /// - Private IPs: 10.x.x.x, 172.16-31.x.x, 192.168.x.x
    /// - Link-local: 169.254.x.x
    /// - File URLs
    ///
    /// # Arguments
    ///
    /// * `url` - URL to check
    ///
    /// # Returns
    ///
    /// `true` if the URL is safe to fetch
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::validators;
    ///
    /// assert!(!validators::is_safe_url("http://127.0.0.1/admin"));
    /// assert!(!validators::is_safe_url("http://localhost/secret"));
    /// assert!(validators::is_safe_url("https://n.news.naver.com/article/001/123"));
    /// ```
    pub fn is_safe_url(url: &str) -> bool {
        let parsed = match Url::parse(url) {
            Ok(u) => u,
            Err(_) => return false,
        };

        // Block file:// URLs
        if parsed.scheme() == "file" {
            return false;
        }

        // Only allow http and https
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return false;
        }

        let host = match parsed.host_str() {
            Some(h) => h,
            None => return false,
        };

        // Block localhost
        if host == "localhost" || host == "127.0.0.1" || host == "::1" {
            return false;
        }

        // Block private IP ranges
        if is_private_ip(host) {
            return false;
        }

        true
    }

    /// Check if host is a private IP address
    fn is_private_ip(host: &str) -> bool {
        // Try to parse as IP address
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() != 4 {
            return false;
        }

        let octets: Vec<u8> = parts.iter().filter_map(|s| s.parse::<u8>().ok()).collect();

        if octets.len() != 4 {
            return false;
        }

        // Check private ranges
        // 10.0.0.0/8
        if octets[0] == 10 {
            return true;
        }

        // 172.16.0.0/12
        if octets[0] == 172 && (16..=31).contains(&octets[1]) {
            return true;
        }

        // 192.168.0.0/16
        if octets[0] == 192 && octets[1] == 168 {
            return true;
        }

        // 169.254.0.0/16 (link-local)
        if octets[0] == 169 && octets[1] == 254 {
            return true;
        }

        false
    }

    /// Validate URL format and safety
    ///
    /// Comprehensive validation combining domain and safety checks.
    ///
    /// # Arguments
    ///
    /// * `url` - URL to validate
    ///
    /// # Returns
    ///
    /// `Ok(())` if valid, `Err(String)` with error message otherwise
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::url::validators;
    ///
    /// assert!(validators::validate_url("https://n.news.naver.com/article/001/123").is_ok());
    /// assert!(validators::validate_url("http://localhost/admin").is_err());
    /// ```
    pub fn validate_url(url: &str) -> Result<(), String> {
        // Parse URL
        if Url::parse(url).is_err() {
            return Err(format!("Invalid URL format: {url}"));
        }

        // Check if safe
        if !is_safe_url(url) {
            return Err(format!("Unsafe URL (SSRF risk): {url}"));
        }

        // Check if allowed domain
        if !is_allowed_domain(url) {
            return Err(format!("Domain not allowed: {url}"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ids_new_format() {
        let extractor = UrlExtractor::new();
        let url = "https://n.news.naver.com/mnews/article/001/0014123456";
        let (oid, aid) = extractor.extract_ids(url).unwrap();
        assert_eq!(oid, "001");
        assert_eq!(aid, "0014123456");
    }

    #[test]
    fn test_extract_ids_old_format() {
        let extractor = UrlExtractor::new();
        let url = "https://news.naver.com/main/read.naver?oid=001&aid=0014123456";
        let (oid, aid) = extractor.extract_ids(url).unwrap();
        assert_eq!(oid, "001");
        assert_eq!(aid, "0014123456");
    }

    #[test]
    fn test_extract_ids_no_mnews_prefix() {
        let extractor = UrlExtractor::new();
        let url = "https://n.news.naver.com/article/001/0014123456";
        let (oid, aid) = extractor.extract_ids(url).unwrap();
        assert_eq!(oid, "001");
        assert_eq!(aid, "0014123456");
    }

    #[test]
    fn test_extract_ids_invalid_url() {
        let extractor = UrlExtractor::new();
        let url = "https://google.com/search";
        let result = extractor.extract_ids(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_url() {
        let extractor = UrlExtractor::new();
        // Mobile to desktop
        let mobile = "https://m.news.naver.com/article/001/0014123456";
        let normalized = extractor.normalize_url(mobile).unwrap();
        assert!(normalized.starts_with("https://n.news.naver.com"));
        assert!(normalized.contains("/mnews/article/001/0014123456"));
    }

    #[test]
    fn test_normalize_url_old_format() {
        let extractor = UrlExtractor::new();
        let old = "https://news.naver.com/main/read.naver?oid=001&aid=0014123456";
        let normalized = extractor.normalize_url(old).unwrap();
        assert_eq!(
            normalized,
            "https://n.news.naver.com/mnews/article/001/0014123456"
        );
    }

    #[test]
    fn test_normalize_url_invalid() {
        let extractor = UrlExtractor::new();
        let invalid = "https://google.com/search";
        let result = extractor.normalize_url(invalid);
        assert!(result.is_none());
    }

    #[test]
    fn test_is_valid_article_url() {
        let extractor = UrlExtractor::new();
        assert!(
            extractor.is_valid_article_url("https://n.news.naver.com/mnews/article/001/0014123456")
        );
        assert!(!extractor.is_valid_article_url("https://google.com/search"));
    }

    #[test]
    fn test_is_valid_article_url_various_domains() {
        let extractor = UrlExtractor::new();
        assert!(extractor
            .is_valid_article_url("https://news.naver.com/main/read.naver?oid=001&aid=0014123456"));
        assert!(extractor.is_valid_article_url("https://sports.naver.com/article/001/0014123456"));
    }

    #[test]
    fn test_ssrf_prevention() {
        assert!(!validators::is_safe_url("http://127.0.0.1/admin"));
        assert!(!validators::is_safe_url("http://localhost/secret"));
        assert!(!validators::is_safe_url("http://192.168.1.1/"));
        assert!(!validators::is_safe_url("http://10.0.0.1/"));
        assert!(!validators::is_safe_url("file:///etc/passwd"));
        assert!(validators::is_safe_url(
            "https://n.news.naver.com/article/001/123"
        ));
    }

    #[test]
    fn test_ssrf_prevention_link_local() {
        assert!(!validators::is_safe_url("http://169.254.1.1/"));
    }

    #[test]
    fn test_ssrf_prevention_private_ranges() {
        assert!(!validators::is_safe_url("http://172.16.0.1/"));
        assert!(!validators::is_safe_url("http://172.31.255.255/"));
    }

    #[test]
    fn test_allowed_domains() {
        assert!(validators::is_allowed_domain(
            "https://n.news.naver.com/article/001/123"
        ));
        assert!(validators::is_allowed_domain(
            "https://sports.naver.com/article/001/123"
        ));
        assert!(!validators::is_allowed_domain("https://evil.com/fake"));
    }

    #[test]
    fn test_allowed_domains_mobile() {
        assert!(validators::is_allowed_domain(
            "https://m.news.naver.com/article/001/123"
        ));
    }

    #[test]
    fn test_url_deduplication() {
        let extractor = UrlExtractor::new();
        let html = r#"
            <a href="https://n.news.naver.com/mnews/article/001/0014123456">Link 1</a>
            <a href="https://n.news.naver.com/mnews/article/001/0014123456">Link 2</a>
            <a href="https://n.news.naver.com/mnews/article/002/0014123457">Link 3</a>
        "#;
        let urls = extractor.extract_urls(html);
        assert_eq!(urls.len(), 2); // Deduplicated
    }

    #[test]
    fn test_extract_urls_mixed_formats() {
        let extractor = UrlExtractor::new();
        let html = r#"
            <a href="https://n.news.naver.com/mnews/article/001/0014123456">Modern</a>
            <a href="https://news.naver.com/main/read.naver?oid=002&aid=0014123457">Old</a>
            <a href="https://m.news.naver.com/article/003/0014123458">Mobile</a>
        "#;
        let urls = extractor.extract_urls(html);
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn test_to_absolute() {
        let extractor = UrlExtractor::new();

        // Already absolute
        let absolute = "https://n.news.naver.com/article/001/123";
        assert_eq!(
            extractor.to_absolute(absolute, "https://n.news.naver.com"),
            absolute
        );

        // Relative URL
        let relative = "/mnews/article/001/0014123456";
        let result = extractor.to_absolute(relative, "https://n.news.naver.com");
        assert!(result.starts_with("https://n.news.naver.com"));
        assert!(result.contains("/mnews/article/001/0014123456"));
    }

    #[test]
    fn test_validate_url() {
        assert!(validators::validate_url("https://n.news.naver.com/article/001/123").is_ok());
        assert!(validators::validate_url("http://localhost/admin").is_err());
        assert!(validators::validate_url("https://evil.com/fake").is_err());
    }

    #[test]
    fn test_extract_urls_filters_invalid() {
        let extractor = UrlExtractor::new();
        let html = r#"
            <a href="https://n.news.naver.com/mnews/article/001/0014123456">Valid</a>
            <a href="https://google.com/search">Invalid domain</a>
            <a href="http://localhost/admin">Localhost</a>
        "#;
        let urls = extractor.extract_urls(html);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("001/0014123456"));
    }
}
