// Core data structures for baram crawler

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;

/// Parsed news article
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParsedArticle {
    pub oid: String, // Publisher ID (e.g., "001")
    pub aid: String, // Article ID (e.g., "0014123456")
    pub title: String,
    pub content: String,
    pub url: String,
    pub category: String, // politics, economy, society, etc.
    pub publisher: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub crawled_at: DateTime<Utc>,
    pub content_hash: Option<String>, // SHA256 for deduplication
}

impl ParsedArticle {
    /// Generate unique ID: {oid}_{aid}
    pub fn id(&self) -> String {
        format!("{}_{}", self.oid, self.aid)
    }

    /// Calculate content hash using SHA256
    pub fn compute_hash(&mut self) {
        let mut hasher = Sha256::new();
        hasher.update(self.content.as_bytes());
        self.content_hash = Some(format!("{:x}", hasher.finalize()));
    }

    /// Create with current timestamp
    pub fn new_with_timestamp() -> Self {
        Self {
            crawled_at: Utc::now(),
            ..Default::default()
        }
    }
}

/// News category enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NewsCategory {
    Politics = 100,
    Economy = 101,
    Society = 102,
    Culture = 103,
    World = 104,
    IT = 105,
}

impl NewsCategory {
    /// Create from section ID
    pub fn from_section_id(id: u32) -> Option<Self> {
        match id {
            100 => Some(Self::Politics),
            101 => Some(Self::Economy),
            102 => Some(Self::Society),
            103 => Some(Self::Culture),
            104 => Some(Self::World),
            105 => Some(Self::IT),
            _ => None,
        }
    }

    /// Get section ID for URL building
    pub fn to_section_id(&self) -> u32 {
        *self as u32
    }

    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Politics => "politics",
            Self::Economy => "economy",
            Self::Society => "society",
            Self::Culture => "culture",
            Self::World => "world",
            Self::IT => "it",
        }
    }

    /// Get Korean name
    pub fn korean_name(&self) -> &'static str {
        match self {
            Self::Politics => "정치",
            Self::Economy => "경제",
            Self::Society => "사회",
            Self::Culture => "생활/문화",
            Self::World => "세계",
            Self::IT => "IT/과학",
        }
    }

    /// Create from string (supports both English and Korean names)
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "politics" | "정치" => Some(Self::Politics),
            "economy" | "경제" => Some(Self::Economy),
            "society" | "사회" => Some(Self::Society),
            "culture" | "생활/문화" | "문화" => Some(Self::Culture),
            "world" | "세계" => Some(Self::World),
            "it" | "it/과학" | "과학" => Some(Self::IT),
            _ => None,
        }
    }

    /// Get all categories
    pub fn all() -> Vec<Self> {
        vec![
            Self::Politics,
            Self::Economy,
            Self::Society,
            Self::Culture,
            Self::World,
            Self::IT,
        ]
    }
}

impl std::fmt::Display for NewsCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Crawl checkpoint for resume functionality
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrawlState {
    pub completed_articles: HashSet<String>,
    pub last_category: Option<String>,
    pub last_page: u32,
    pub last_url: Option<String>,
    pub total_crawled: u32,
    pub total_errors: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl CrawlState {
    /// Create new crawl state
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            started_at: Some(now),
            updated_at: now,
            ..Default::default()
        }
    }

    /// Load state from file, return default if not found or corrupted
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Save state to file
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Atomic write using temp file
        let temp_path = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&temp_path, content)?;
        std::fs::rename(temp_path, path)?;
        Ok(())
    }

    /// Check if article was already crawled
    pub fn is_completed(&self, article_id: &str) -> bool {
        self.completed_articles.contains(article_id)
    }

    /// Mark article as completed
    pub fn mark_completed(&mut self, article_id: &str) {
        self.completed_articles.insert(article_id.to_string());
        self.total_crawled += 1;
        self.updated_at = Utc::now();
    }

    /// Record an error
    pub fn record_error(&mut self) {
        self.total_errors += 1;
        self.updated_at = Utc::now();
    }

    /// Get progress stats
    pub fn stats(&self) -> CrawlStats {
        CrawlStats {
            total_crawled: self.total_crawled,
            total_errors: self.total_errors,
            unique_articles: self.completed_articles.len() as u32,
            duration_secs: self
                .started_at
                .map(|s| (Utc::now() - s).num_seconds() as u64)
                .unwrap_or(0),
        }
    }
}

/// Crawl statistics
#[derive(Debug, Clone, Serialize)]
pub struct CrawlStats {
    pub total_crawled: u32,
    pub total_errors: u32,
    pub unique_articles: u32,
    pub duration_secs: u64,
}

impl CrawlStats {
    /// Calculate error rate as percentage
    pub fn error_rate(&self) -> f64 {
        if self.total_crawled == 0 {
            0.0
        } else {
            (self.total_errors as f64 / self.total_crawled as f64) * 100.0
        }
    }

    /// Calculate crawl rate (articles per minute)
    pub fn crawl_rate(&self) -> f64 {
        if self.duration_secs == 0 {
            0.0
        } else {
            (self.total_crawled as f64 / self.duration_secs as f64) * 60.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_article_id_format() {
        let article = ParsedArticle {
            oid: "001".to_string(),
            aid: "0014123456".to_string(),
            ..Default::default()
        };
        assert_eq!(article.id(), "001_0014123456");
    }

    #[test]
    fn test_content_hash() {
        let mut article = ParsedArticle {
            content: "테스트 내용입니다.".to_string(),
            ..Default::default()
        };
        article.compute_hash();
        assert!(article.content_hash.is_some());
        assert_eq!(article.content_hash.as_ref().unwrap().len(), 64); // SHA256 hex = 64 chars
    }

    #[test]
    fn test_category_conversion() {
        assert_eq!(NewsCategory::Politics.to_section_id(), 100);
        assert_eq!(
            NewsCategory::from_section_id(100),
            Some(NewsCategory::Politics)
        );
        assert_eq!(NewsCategory::from_section_id(999), None);
    }

    #[test]
    fn test_category_from_str() {
        assert_eq!(
            NewsCategory::parse("politics"),
            Some(NewsCategory::Politics)
        );
        assert_eq!(NewsCategory::parse("정치"), Some(NewsCategory::Politics));
        assert_eq!(NewsCategory::parse("invalid"), None);
    }

    #[test]
    fn test_crawl_state_serde() {
        let mut state = CrawlState::new();
        state.mark_completed("001_123");
        state.mark_completed("001_456");

        let json = serde_json::to_string(&state).unwrap();
        let restored: CrawlState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.completed_articles.len(), 2);
        assert!(restored.is_completed("001_123"));
    }

    #[test]
    fn test_crawl_state_persistence() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_crawl_state.json");

        let mut state = CrawlState::new();
        state.mark_completed("test_article_1");
        state.save(&path).unwrap();

        let loaded = CrawlState::load(&path);
        assert!(loaded.is_completed("test_article_1"));

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_corrupted_checkpoint_returns_default() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("corrupted_state.json");

        std::fs::write(&path, "{ invalid json }").unwrap();

        let state = CrawlState::load(&path);
        assert!(state.completed_articles.is_empty());

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_stats_calculation() {
        let mut state = CrawlState::new();
        state.total_crawled = 100;
        state.total_errors = 5;

        let stats = state.stats();
        assert_eq!(stats.error_rate(), 5.0);
    }
}
