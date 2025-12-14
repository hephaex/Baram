//! Common utilities and helper functions
//!
//! This module provides shared utilities used across the application.

pub mod error;

use anyhow::{Context, Result};
use encoding_rs::EUC_KR;
use regex::Regex;
use std::sync::OnceLock;
use url::Url;

/// Convert EUC-KR encoded bytes to UTF-8 string
pub fn decode_euc_kr(bytes: &[u8]) -> Result<String> {
    let (cow, _encoding, had_errors) = EUC_KR.decode(bytes);

    if had_errors {
        anyhow::bail!("Failed to decode EUC-KR content");
    }

    Ok(cow.into_owned())
}

/// Normalize whitespace in text
pub fn normalize_whitespace(text: &str) -> String {
    static WHITESPACE_RE: OnceLock<Regex> = OnceLock::new();

    let re = WHITESPACE_RE.get_or_init(|| Regex::new(r"\s+").expect("Invalid regex pattern"));

    re.replace_all(text.trim(), " ").to_string()
}

/// Extract domain from URL
pub fn extract_domain(url: &str) -> Result<String> {
    let parsed = Url::parse(url).context("Invalid URL")?;

    parsed
        .host_str()
        .map(|s| s.to_string())
        .context("No host in URL")
}

/// Sanitize filename by removing invalid characters
pub fn sanitize_filename(filename: &str) -> String {
    static INVALID_CHARS: OnceLock<Regex> = OnceLock::new();

    let re =
        INVALID_CHARS.get_or_init(|| Regex::new(r#"[<>:"/\\|?*]"#).expect("Invalid regex pattern"));

    re.replace_all(filename, "_").to_string()
}

/// Truncate text to a maximum length
pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let truncated = &text[..max_len.saturating_sub(3)];
        format!("{truncated}...")
    }
}

/// Format byte size as human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    if bytes == 0 {
        return String::from("0 B");
    }

    let base: f64 = 1024.0;
    let exponent = (bytes as f64).log(base).floor() as usize;
    let exponent = exponent.min(UNITS.len() - 1);

    let value = bytes as f64 / base.powi(exponent as i32);

    format!("{value:.2} {}", UNITS[exponent])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("  hello   world  "), "hello world");
        assert_eq!(normalize_whitespace("hello\n\nworld"), "hello world");
    }

    #[test]
    fn test_extract_domain() {
        let domain = extract_domain("https://news.naver.com/article/123");
        assert_eq!(domain.unwrap(), "news.naver.com");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("file<name>.txt"), "file_name_.txt");
        assert_eq!(
            sanitize_filename("valid_filename.txt"),
            "valid_filename.txt"
        );
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("very long text here", 10), "very lo...");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
    }
}
