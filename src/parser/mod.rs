//! HTML parsing and data extraction
//!
//! This module handles parsing Naver News HTML pages and extracting
//! structured article data.

pub mod html;
pub mod sanitize;
pub mod selectors;

// Re-export main parser and public types
pub use html::{detect_format, ArticleParser};
pub use selectors::ArticleFormat;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use uuid::Uuid;

// ============================================================================
// Static Selectors (compiled once at runtime, never fails for valid CSS)
// ============================================================================

/// Get title selector (compiled once, cached)
fn title_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| {
        // SAFETY: These are valid CSS selectors verified at compile time
        Selector::parse("h1, .article_title, .title")
            .expect("BUG: Invalid hardcoded title selector")
    })
}

/// Get body selector (compiled once, cached)
fn body_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| {
        Selector::parse(".article_body, .article_content, #articleBodyContents")
            .expect("BUG: Invalid hardcoded body selector")
    })
}

/// Get author selector (compiled once, cached)
fn author_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| {
        Selector::parse(".author, .byline, .writer")
            .expect("BUG: Invalid hardcoded author selector")
    })
}

/// Get date selector (compiled once, cached)
fn date_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| {
        Selector::parse(".date, .publish_date, time").expect("BUG: Invalid hardcoded date selector")
    })
}

/// Parsed news article
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    /// Unique article identifier
    pub id: Uuid,

    /// Article URL
    pub url: String,

    /// Article title
    pub title: String,

    /// Article body content
    pub body: String,

    /// Author name
    pub author: Option<String>,

    /// Publication date
    pub published_at: Option<DateTime<Utc>>,

    /// News category
    pub category: Option<String>,

    /// Content hash (SHA-256)
    pub content_hash: String,

    /// Associated comments
    pub comments: Vec<Comment>,
}

/// Article comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    /// Comment ID
    pub id: String,

    /// Comment author
    pub author: String,

    /// Comment text
    pub text: String,

    /// Comment timestamp
    pub created_at: DateTime<Utc>,

    /// Number of likes
    pub likes: i32,
}

/// HTML parser for Naver News articles
///
/// This parser uses statically cached selectors that are compiled once at runtime.
/// The Default implementation is guaranteed not to panic.
#[derive(Debug, Clone, Copy)]
pub struct Parser {
    // This struct uses zero-sized marker for future extensibility
    _private: (),
}

impl Parser {
    /// Create a new parser instance
    ///
    /// This is infallible as selectors are statically defined.
    /// Kept for backward compatibility; prefer `Parser::default()`.
    #[deprecated(since = "0.2.0", note = "Use Parser::default() instead")]
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    /// Parse HTML content into an Article
    pub fn parse(&self, url: String, html: &str) -> Result<Article> {
        let document = Html::parse_document(html);

        let title = self
            .extract_title(&document)
            .context("Failed to extract title")?;

        let body = self
            .extract_body(&document)
            .context("Failed to extract body")?;

        let author = self.extract_author(&document);
        let published_at = self.extract_date(&document);

        let content_hash = Self::compute_content_hash(&title, &body);

        Ok(Article {
            id: Uuid::new_v4(),
            url,
            title,
            body,
            author,
            published_at,
            category: None,
            content_hash,
            comments: Vec::new(),
        })
    }

    fn extract_title(&self, document: &Html) -> Result<String> {
        document
            .select(title_selector())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .context("Title not found")
    }

    fn extract_body(&self, document: &Html) -> Result<String> {
        document
            .select(body_selector())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .context("Body not found")
    }

    fn extract_author(&self, document: &Html) -> Option<String> {
        document
            .select(author_selector())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    fn extract_date(&self, document: &Html) -> Option<DateTime<Utc>> {
        // Try to find date element
        let date_element = document.select(date_selector()).next()?;

        // First, try to extract from datetime attribute (common in <time> elements)
        if let Some(datetime_attr) = date_element.value().attr("datetime") {
            if let Some(parsed) = Self::parse_date_string(datetime_attr) {
                return Some(parsed);
            }
        }

        // Fall back to text content parsing
        let date_text = date_element.text().collect::<String>();
        Self::parse_date_string(date_text.trim())
    }

    /// Parse various date formats into DateTime<Utc>
    fn parse_date_string(date_str: &str) -> Option<DateTime<Utc>> {
        use chrono::{NaiveDateTime, TimeZone};

        // Try ISO 8601 format first
        if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
            return Some(dt.with_timezone(&Utc));
        }

        // Handle Korean format with 오전/오후 (AM/PM)
        // Example: "2024.12.25. 오후 3:45"
        if let Some(parsed) = Self::parse_korean_datetime(date_str) {
            return Some(parsed);
        }

        // Try common formats
        let formats = [
            "%Y-%m-%d %H:%M:%S",    // 2024-12-25 15:45:00
            "%Y.%m.%d %H:%M",       // 2024.12.25 15:45
            "%Y.%m.%d. %H:%M",      // 2024.12.25. 15:45
            "%Y-%m-%d %H:%M",       // 2024-12-25 15:45
            "%Y/%m/%d %H:%M:%S",    // 2024/12/25 15:45:00
            "%Y/%m/%d %H:%M",       // 2024/12/25 15:45
            "%Y년 %m월 %d일 %H:%M", // Korean format: 2024년 12월 25일 15:45
        ];

        for format in &formats {
            if let Ok(naive_dt) = NaiveDateTime::parse_from_str(date_str, format) {
                // Assume KST (UTC+9) for Korean news sites
                let kst_offset = chrono::FixedOffset::east_opt(9 * 3600)?;
                return Some(
                    kst_offset
                        .from_local_datetime(&naive_dt)
                        .single()?
                        .with_timezone(&Utc),
                );
            }
        }

        None
    }

    /// Parse Korean datetime format with 오전/오후 (AM/PM)
    /// Example: "2024.12.25. 오후 3:45" -> DateTime
    fn parse_korean_datetime(date_str: &str) -> Option<DateTime<Utc>> {
        use chrono::{NaiveDate, NaiveTime, TimeZone};

        // Regex pattern for Korean datetime
        // Matches: YYYY.MM.DD. 오전/오후 H:MM or YYYY-MM-DD 오전/오후 H:MM
        let re = regex::Regex::new(
            r"(\d{4})[.-](\d{1,2})[.-](\d{1,2})[.]?\s*(오전|오후)\s*(\d{1,2}):(\d{2})",
        )
        .ok()?;

        let caps = re.captures(date_str)?;

        let year = caps.get(1)?.as_str().parse::<i32>().ok()?;
        let month = caps.get(2)?.as_str().parse::<u32>().ok()?;
        let day = caps.get(3)?.as_str().parse::<u32>().ok()?;
        let am_pm = caps.get(4)?.as_str();
        let hour = caps.get(5)?.as_str().parse::<u32>().ok()?;
        let minute = caps.get(6)?.as_str().parse::<u32>().ok()?;

        // Convert 12-hour to 24-hour format
        let hour_24 = match am_pm {
            "오전" => {
                // AM: 12 -> 0, 1-11 stays same
                if hour == 12 {
                    0
                } else {
                    hour
                }
            }
            "오후" => {
                // PM: 12 stays 12, 1-11 -> 13-23
                if hour == 12 {
                    12
                } else {
                    hour + 12
                }
            }
            _ => return None,
        };

        let naive_date = NaiveDate::from_ymd_opt(year, month, day)?;
        let naive_time = NaiveTime::from_hms_opt(hour_24, minute, 0)?;
        let naive_dt = naive_date.and_time(naive_time);

        // Assume KST (UTC+9) for Korean news sites
        let kst_offset = chrono::FixedOffset::east_opt(9 * 3600)?;
        Some(
            kst_offset
                .from_local_datetime(&naive_dt)
                .single()?
                .with_timezone(&Utc),
        )
    }

    fn compute_content_hash(title: &str, body: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(title.as_bytes());
        hasher.update(body.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl Default for Parser {
    /// Create a default Parser instance.
    ///
    /// This is guaranteed not to panic - selectors are statically defined
    /// and compiled lazily via OnceLock.
    fn default() -> Self {
        Self { _private: () }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn test_parser_creation() {
        let parser = Parser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_content_hash_computation() {
        let hash1 = Parser::compute_content_hash("Title", "Body");
        let hash2 = Parser::compute_content_hash("Title", "Body");
        let hash3 = Parser::compute_content_hash("Different", "Content");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_parse_iso8601_datetime() {
        let dt = Parser::parse_date_string("2024-12-25T15:45:00+09:00");
        assert!(dt.is_some());
        let parsed = dt.unwrap();
        assert_eq!(parsed.format("%Y-%m-%d").to_string(), "2024-12-25");
    }

    #[test]
    fn test_parse_korean_am_format() {
        // 오전 11:30 (11:30 AM)
        let dt = Parser::parse_date_string("2024.12.25. 오전 11:30");
        assert!(dt.is_some());
        let parsed = dt.unwrap();
        // In KST (UTC+9), 11:30 AM is 02:30 UTC
        assert_eq!(parsed.hour(), 2);
        assert_eq!(parsed.minute(), 30);
    }

    #[test]
    fn test_parse_korean_pm_format() {
        // 오후 3:45 (3:45 PM = 15:45)
        let dt = Parser::parse_date_string("2024.12.25. 오후 3:45");
        assert!(dt.is_some());
        let parsed = dt.unwrap();
        // In KST (UTC+9), 3:45 PM (15:45) is 06:45 UTC
        assert_eq!(parsed.hour(), 6);
        assert_eq!(parsed.minute(), 45);
    }

    #[test]
    fn test_parse_korean_noon() {
        // 오후 12:00 (12:00 PM = noon)
        let dt = Parser::parse_date_string("2024.12.25. 오후 12:00");
        assert!(dt.is_some());
        let parsed = dt.unwrap();
        // In KST (UTC+9), 12:00 PM is 03:00 UTC
        assert_eq!(parsed.hour(), 3);
    }

    #[test]
    fn test_parse_korean_midnight() {
        // 오전 12:00 (12:00 AM = midnight)
        let dt = Parser::parse_date_string("2024.12.25. 오전 12:00");
        assert!(dt.is_some());
        let parsed = dt.unwrap();
        // In KST (UTC+9), 12:00 AM is 15:00 UTC previous day
        // (24:00 - 9 = 15:00)
        assert_eq!(parsed.format("%Y-%m-%d").to_string(), "2024-12-24");
        assert_eq!(parsed.hour(), 15);
    }

    #[test]
    fn test_parse_standard_datetime_format() {
        let dt = Parser::parse_date_string("2024-12-25 15:45:00");
        assert!(dt.is_some());
        let parsed = dt.unwrap();
        assert_eq!(parsed.format("%Y-%m-%d").to_string(), "2024-12-25");
    }

    #[test]
    fn test_parse_dotted_datetime_format() {
        let dt = Parser::parse_date_string("2024.12.25 15:45");
        assert!(dt.is_some());
        let parsed = dt.unwrap();
        assert_eq!(parsed.format("%Y-%m-%d").to_string(), "2024-12-25");
    }

    #[test]
    fn test_parse_invalid_datetime() {
        let dt = Parser::parse_date_string("invalid date");
        assert!(dt.is_none());
    }

    #[test]
    fn test_extract_date_from_datetime_attribute() {
        let parser = Parser::default();
        let html = r#"
            <html>
                <time datetime="2024-12-25T15:45:00+09:00">2024년 12월 25일</time>
            </html>
        "#;
        let document = Html::parse_document(html);
        let dt = parser.extract_date(&document);
        assert!(dt.is_some());
    }

    #[test]
    fn test_extract_date_from_text_content() {
        let parser = Parser::default();
        let html = r#"
            <html>
                <div class="date">2024.12.25. 오후 3:45</div>
            </html>
        "#;
        let document = Html::parse_document(html);
        let dt = parser.extract_date(&document);
        assert!(dt.is_some());
    }

    #[test]
    fn test_extract_date_not_found() {
        let parser = Parser::default();
        let html = r#"
            <html>
                <div>No date here</div>
            </html>
        "#;
        let document = Html::parse_document(html);
        let dt = parser.extract_date(&document);
        assert!(dt.is_none());
    }
}
