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
use uuid::Uuid;

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
pub struct Parser {
    /// Title selector
    title_selector: Selector,

    /// Body selector
    body_selector: Selector,

    /// Author selector
    author_selector: Selector,

    /// Date selector
    #[allow(dead_code)]
    date_selector: Selector,
}

impl Parser {
    /// Create a new parser instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            title_selector: Selector::parse("h1, .article_title, .title")
                .map_err(|e| anyhow::anyhow!("Invalid title selector: {e:?}"))?,
            body_selector: Selector::parse(".article_body, .article_content, #articleBodyContents")
                .map_err(|e| anyhow::anyhow!("Invalid body selector: {e:?}"))?,
            author_selector: Selector::parse(".author, .byline, .writer")
                .map_err(|e| anyhow::anyhow!("Invalid author selector: {e:?}"))?,
            date_selector: Selector::parse(".date, .publish_date, time")
                .map_err(|e| anyhow::anyhow!("Invalid date selector: {e:?}"))?,
        })
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

        let content_hash = self.compute_content_hash(&title, &body);

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
            .select(&self.title_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .context("Title not found")
    }

    fn extract_body(&self, document: &Html) -> Result<String> {
        document
            .select(&self.body_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .context("Body not found")
    }

    fn extract_author(&self, document: &Html) -> Option<String> {
        document
            .select(&self.author_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    fn extract_date(&self, _document: &Html) -> Option<DateTime<Utc>> {
        // TODO: Implement date parsing
        None
    }

    fn compute_content_hash(&self, title: &str, body: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(title.as_bytes());
        hasher.update(body.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new().expect("Failed to create default parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = Parser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_content_hash_computation() {
        let parser = Parser::new().unwrap();
        let hash1 = parser.compute_content_hash("Title", "Body");
        let hash2 = parser.compute_content_hash("Title", "Body");
        let hash3 = parser.compute_content_hash("Different", "Content");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
