//! Markdown file storage with Handlebars template engine
//!
//! This module handles rendering articles to Markdown format and
//! saving them to the filesystem.

use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::models::ParsedArticle;

/// Default article template
const DEFAULT_TEMPLATE: &str = include_str!("../../templates/article.hbs");

/// Template data for rendering
#[derive(Debug, Serialize)]
struct ArticleTemplateData {
    id: String,
    title: String,
    content: String,
    category: String,
    publisher: String,
    author: String,
    published_at: String,
    crawled_at: String,
    url: String,
    oid: String,
    aid: String,
    content_hash: String,
}

impl From<&ParsedArticle> for ArticleTemplateData {
    fn from(article: &ParsedArticle) -> Self {
        Self {
            id: article.id(),
            title: article.title.clone(),
            content: article.content.clone(),
            category: article.category.clone(),
            publisher: article.publisher.clone().unwrap_or_default(),
            author: article.author.clone().unwrap_or_default(),
            published_at: article
                .published_at
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default(),
            crawled_at: article.crawled_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            url: article.url.clone(),
            oid: article.oid.clone(),
            aid: article.aid.clone(),
            content_hash: article.content_hash.clone().unwrap_or_default(),
        }
    }
}

/// Markdown writer with Handlebars template engine
pub struct MarkdownWriter<'a> {
    /// Handlebars template engine
    handlebars: Handlebars<'a>,

    /// Output directory
    output_dir: PathBuf,
}

impl<'a> MarkdownWriter<'a> {
    /// Create a new MarkdownWriter with default template
    ///
    /// # Arguments
    /// * `output_dir` - Directory to save markdown files
    ///
    /// # Example
    /// ```no_run
    /// use ntimes::storage::MarkdownWriter;
    /// use std::path::Path;
    ///
    /// let writer = MarkdownWriter::new(Path::new("./output/raw")).unwrap();
    /// ```
    pub fn new(output_dir: &Path) -> Result<Self> {
        let mut handlebars = Handlebars::new();

        // Register default template
        handlebars
            .register_template_string("article", DEFAULT_TEMPLATE)
            .context("Failed to register default article template")?;

        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir).context("Failed to create output directory")?;

        Ok(Self {
            handlebars,
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// Create with custom template file
    ///
    /// # Arguments
    /// * `output_dir` - Directory to save markdown files
    /// * `template_path` - Path to custom Handlebars template
    pub fn with_template(output_dir: &Path, template_path: &Path) -> Result<Self> {
        let mut handlebars = Handlebars::new();

        // Register custom template
        handlebars
            .register_template_file("article", template_path)
            .context("Failed to register custom template")?;

        fs::create_dir_all(output_dir).context("Failed to create output directory")?;

        Ok(Self {
            handlebars,
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// Render article to markdown string
    ///
    /// # Arguments
    /// * `article` - Article to render
    ///
    /// # Returns
    /// Rendered markdown string
    pub fn render(&self, article: &ParsedArticle) -> Result<String> {
        let data = ArticleTemplateData::from(article);
        self.handlebars
            .render("article", &data)
            .context("Failed to render article template")
    }

    /// Save article to markdown file
    ///
    /// # Arguments
    /// * `article` - Article to save
    ///
    /// # Returns
    /// Path to saved file
    ///
    /// # Example
    /// ```no_run
    /// use ntimes::storage::MarkdownWriter;
    /// use ntimes::models::ParsedArticle;
    /// use std::path::Path;
    ///
    /// let writer = MarkdownWriter::new(Path::new("./output")).unwrap();
    /// let article = ParsedArticle::default();
    /// let path = writer.save(&article).unwrap();
    /// println!("Saved to: {:?}", path);
    /// ```
    pub fn save(&self, article: &ParsedArticle) -> Result<PathBuf> {
        let markdown = self.render(article)?;
        let filename = self.generate_filename(article);
        let filepath = self.output_dir.join(&filename);

        let mut file = File::create(&filepath)
            .with_context(|| format!("Failed to create file: {}", filepath.display()))?;

        file.write_all(markdown.as_bytes())
            .with_context(|| format!("Failed to write to file: {}", filepath.display()))?;

        tracing::debug!(path = %filepath.display(), "Saved article to markdown");
        Ok(filepath)
    }

    /// Save multiple articles
    ///
    /// # Arguments
    /// * `articles` - Articles to save
    ///
    /// # Returns
    /// Vector of saved file paths
    pub fn save_batch(&self, articles: &[ParsedArticle]) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::with_capacity(articles.len());

        for article in articles {
            let path = self.save(article)?;
            paths.push(path);
        }

        Ok(paths)
    }

    /// Generate filename for article
    ///
    /// Format: {oid}_{aid}_{sanitized_title}.md
    fn generate_filename(&self, article: &ParsedArticle) -> String {
        let sanitized_title = sanitize_filename(&article.title, 50);
        format!("{}_{}{}.md", article.oid, article.aid,
            if sanitized_title.is_empty() {
                String::new()
            } else {
                format!("_{sanitized_title}")
            })
    }

    /// Get output directory
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Check if article file already exists
    pub fn exists(&self, article: &ParsedArticle) -> bool {
        let filename = self.generate_filename(article);
        self.output_dir.join(filename).exists()
    }
}

/// Sanitize string for use as filename
///
/// # Arguments
/// * `s` - String to sanitize
/// * `max_len` - Maximum length
///
/// # Returns
/// Sanitized string safe for use as filename
fn sanitize_filename(s: &str, max_len: usize) -> String {
    let sanitized: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
        .take(max_len)
        .collect();

    sanitized
        .trim()
        .replace(' ', "_")
        .to_lowercase()
}

/// Result of batch save operation
#[derive(Debug)]
pub struct BatchSaveResult {
    /// Successfully saved articles
    pub saved: Vec<PathBuf>,

    /// Failed articles with error messages
    pub failed: Vec<(String, String)>,

    /// Skipped articles (already exist)
    pub skipped: Vec<String>,
}

impl BatchSaveResult {
    /// Create new empty result
    pub fn new() -> Self {
        Self {
            saved: Vec::new(),
            failed: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// Total articles processed
    pub fn total(&self) -> usize {
        self.saved.len() + self.failed.len() + self.skipped.len()
    }

    /// Success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total() == 0 {
            return 1.0;
        }
        self.saved.len() as f64 / self.total() as f64
    }
}

impl Default for BatchSaveResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Extended markdown writer with batch operations and skip logic
pub struct ArticleStorage<'a> {
    writer: MarkdownWriter<'a>,
    skip_existing: bool,
}

impl<'a> ArticleStorage<'a> {
    /// Create new article storage
    pub fn new(output_dir: &Path, skip_existing: bool) -> Result<Self> {
        Ok(Self {
            writer: MarkdownWriter::new(output_dir)?,
            skip_existing,
        })
    }

    /// Save article with optional skip logic
    pub fn save(&self, article: &ParsedArticle) -> Result<Option<PathBuf>> {
        if self.skip_existing && self.writer.exists(article) {
            tracing::debug!(id = %article.id(), "Skipping existing article");
            return Ok(None);
        }

        self.writer.save(article).map(Some)
    }

    /// Save batch with detailed result
    pub fn save_batch(&self, articles: &[ParsedArticle]) -> BatchSaveResult {
        let mut result = BatchSaveResult::new();

        for article in articles {
            if self.skip_existing && self.writer.exists(article) {
                result.skipped.push(article.id());
                continue;
            }

            match self.writer.save(article) {
                Ok(path) => result.saved.push(path),
                Err(e) => result.failed.push((article.id(), e.to_string())),
            }
        }

        tracing::info!(
            saved = result.saved.len(),
            failed = result.failed.len(),
            skipped = result.skipped.len(),
            "Batch save completed"
        );

        result
    }

    /// Get underlying writer
    pub fn writer(&self) -> &MarkdownWriter<'a> {
        &self.writer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_article() -> ParsedArticle {
        ParsedArticle {
            oid: "001".to_string(),
            aid: "0014000001".to_string(),
            title: "테스트 기사 제목입니다".to_string(),
            content: "테스트 기사의 본문 내용입니다.".to_string(),
            url: "https://n.news.naver.com/mnews/article/001/0014000001".to_string(),
            category: "politics".to_string(),
            publisher: Some("테스트언론사".to_string()),
            author: Some("홍길동".to_string()),
            published_at: Some(Utc::now()),
            crawled_at: Utc::now(),
            content_hash: Some("abc123".to_string()),
        }
    }

    #[test]
    fn test_markdown_writer_creation() {
        let temp_dir = TempDir::new().unwrap();
        let writer = MarkdownWriter::new(temp_dir.path());
        assert!(writer.is_ok());
    }

    #[test]
    fn test_render_article() {
        let temp_dir = TempDir::new().unwrap();
        let writer = MarkdownWriter::new(temp_dir.path()).unwrap();
        let article = create_test_article();

        let markdown = writer.render(&article);
        assert!(markdown.is_ok());

        let md = markdown.unwrap();
        assert!(md.contains("테스트 기사 제목입니다"));
        assert!(md.contains("테스트 기사의 본문 내용입니다"));
        assert!(md.contains("테스트언론사"));
    }

    #[test]
    fn test_save_article() {
        let temp_dir = TempDir::new().unwrap();
        let writer = MarkdownWriter::new(temp_dir.path()).unwrap();
        let article = create_test_article();

        let path = writer.save(&article);
        assert!(path.is_ok());

        let filepath = path.unwrap();
        assert!(filepath.exists());
        assert!(filepath.to_string_lossy().ends_with(".md"));
    }

    #[test]
    fn test_generate_filename() {
        let temp_dir = TempDir::new().unwrap();
        let writer = MarkdownWriter::new(temp_dir.path()).unwrap();
        let article = create_test_article();

        let filename = writer.generate_filename(&article);
        assert!(filename.starts_with("001_0014000001"));
        assert!(filename.ends_with(".md"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello World", 50), "hello_world");
        assert_eq!(sanitize_filename("Test<>File", 50), "testfile");
        assert_eq!(sanitize_filename("한글 제목 테스트", 50), "한글_제목_테스트");
        // "Very Long " (10 chars) -> trim -> "Very Long" -> replace -> "Very_Long" -> lowercase
        assert_eq!(sanitize_filename("Very Long Title That Should Be Truncated", 10), "very_long");
    }

    #[test]
    fn test_article_exists() {
        let temp_dir = TempDir::new().unwrap();
        let writer = MarkdownWriter::new(temp_dir.path()).unwrap();
        let article = create_test_article();

        assert!(!writer.exists(&article));
        writer.save(&article).unwrap();
        assert!(writer.exists(&article));
    }

    #[test]
    fn test_batch_save() {
        let temp_dir = TempDir::new().unwrap();
        let writer = MarkdownWriter::new(temp_dir.path()).unwrap();

        let mut articles = Vec::new();
        for i in 0..3 {
            let mut article = create_test_article();
            article.aid = format!("{:010}", i);
            articles.push(article);
        }

        let paths = writer.save_batch(&articles);
        assert!(paths.is_ok());
        assert_eq!(paths.unwrap().len(), 3);
    }

    #[test]
    fn test_article_storage_skip_existing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ArticleStorage::new(temp_dir.path(), true).unwrap();
        let article = create_test_article();

        // First save should succeed
        let result1 = storage.save(&article);
        assert!(result1.is_ok());
        assert!(result1.unwrap().is_some());

        // Second save should be skipped
        let result2 = storage.save(&article);
        assert!(result2.is_ok());
        assert!(result2.unwrap().is_none());
    }

    #[test]
    fn test_batch_save_result() {
        let mut result = BatchSaveResult::new();
        result.saved.push(PathBuf::from("file1.md"));
        result.saved.push(PathBuf::from("file2.md"));
        result.skipped.push("skipped_id".to_string());
        result.failed.push(("failed_id".to_string(), "error".to_string()));

        assert_eq!(result.total(), 4);
        assert_eq!(result.success_rate(), 0.5);
    }

    #[test]
    fn test_batch_save_with_skip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ArticleStorage::new(temp_dir.path(), true).unwrap();

        let article = create_test_article();
        storage.save(&article).unwrap();

        // Create batch with same article
        let articles = vec![article.clone(), article];
        let result = storage.save_batch(&articles);

        assert_eq!(result.skipped.len(), 2);
        assert_eq!(result.saved.len(), 0);
    }
}
