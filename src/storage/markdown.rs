//! Markdown file storage with Handlebars template engine
//!
//! This module handles rendering articles to Markdown format and
//! saving them to the filesystem.
//!
//! # Comment Tree Rendering
//!
//! Comments are rendered with hierarchical indentation using blockquotes:
//! - Top-level comments start without indentation
//! - Each reply level adds one `>` prefix
//! - Maximum depth is configurable (default: 10 levels)

use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde::Serialize;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::crawler::comment::Comment;
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
    /// use baram::storage::MarkdownWriter;
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
    /// use baram::storage::MarkdownWriter;
    /// use baram::models::ParsedArticle;
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
        format!(
            "{}_{}{}.md",
            article.oid,
            article.aid,
            if sanitized_title.is_empty() {
                String::new()
            } else {
                format!("_{sanitized_title}")
            }
        )
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

    sanitized.trim().replace(' ', "_").to_lowercase()
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

// ============================================================================
// Comment Rendering
// ============================================================================

/// Configuration for comment rendering
#[derive(Debug, Clone)]
pub struct CommentRenderConfig {
    /// Maximum depth for nested replies (default: 10)
    pub max_depth: usize,

    /// Show author ID (masked) along with name
    pub show_author_id: bool,

    /// Show like/dislike counts
    pub show_reactions: bool,

    /// Show timestamp
    pub show_timestamp: bool,

    /// Mark best comments
    pub highlight_best: bool,

    /// Show deleted comments placeholder
    pub show_deleted_placeholder: bool,
}

impl Default for CommentRenderConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            show_author_id: false,
            show_reactions: true,
            show_timestamp: true,
            highlight_best: true,
            show_deleted_placeholder: false,
        }
    }
}

/// Renders comments to Markdown format with hierarchical structure
pub struct CommentRenderer {
    config: CommentRenderConfig,
}

impl CommentRenderer {
    /// Create a new comment renderer with default config
    pub fn new() -> Self {
        Self {
            config: CommentRenderConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: CommentRenderConfig) -> Self {
        Self { config }
    }

    /// Render a single comment to Markdown
    ///
    /// # Arguments
    /// * `comment` - Comment to render
    /// * `depth` - Current nesting depth (0 = top-level)
    ///
    /// # Returns
    /// Markdown string for the comment
    pub fn render_comment(&self, comment: &Comment, depth: usize) -> String {
        let mut output = String::new();

        // Calculate effective depth (capped at max_depth)
        let effective_depth = depth.min(self.config.max_depth);

        // Build blockquote prefix for indentation
        let prefix = if effective_depth > 0 {
            ">".repeat(effective_depth) + " "
        } else {
            String::new()
        };

        // Handle deleted comments
        if comment.is_deleted {
            if self.config.show_deleted_placeholder {
                // Writing to String cannot fail
                let _ = writeln!(output, "{prefix}*[삭제된 댓글입니다]*\n");
            }
            return output;
        }

        // Comment header: author and metadata
        let mut header_parts = Vec::new();

        // Author
        if self.config.show_author_id {
            header_parts.push(format!("**{}** ({})", comment.author, comment.author_id));
        } else {
            header_parts.push(format!("**{}**", comment.author));
        }

        // Best comment badge
        if self.config.highlight_best && comment.is_best {
            header_parts.push("⭐ **BEST**".to_string());
        }

        // Timestamp
        if self.config.show_timestamp {
            header_parts.push(comment.created_at.format("%Y-%m-%d %H:%M").to_string());
        }

        // Reactions
        if self.config.show_reactions {
            header_parts.push(format!("👍 {} | 👎 {}", comment.likes, comment.dislikes));
        }

        // Write header (writing to String cannot fail)
        let _ = writeln!(output, "{}{}", prefix, header_parts.join(" | "));
        let _ = writeln!(output, "{prefix}");

        // Comment content (wrap each line with prefix)
        for line in comment.content.lines() {
            let _ = writeln!(output, "{prefix}{line}");
        }
        let _ = writeln!(output, "{prefix}");

        // Recursively render replies
        if !comment.replies.is_empty() {
            for reply in &comment.replies {
                let reply_md = self.render_comment(reply, depth + 1);
                output.push_str(&reply_md);
            }
        }

        output
    }

    /// Render multiple comments (comment tree) to Markdown
    ///
    /// # Arguments
    /// * `comments` - List of top-level comments with nested replies
    ///
    /// # Returns
    /// Complete Markdown string for all comments
    pub fn render_comments(&self, comments: &[Comment]) -> String {
        let mut output = String::new();

        if comments.is_empty() {
            return output;
        }

        // Writing to String cannot fail
        let _ = writeln!(output, "## 댓글 ({} 개)\n", Self::count_total(comments));

        for (i, comment) in comments.iter().enumerate() {
            let comment_md = self.render_comment(comment, 0);
            output.push_str(&comment_md);

            // Add separator between top-level comments (except last)
            if i < comments.len() - 1 {
                let _ = writeln!(output, "---\n");
            }
        }

        output
    }

    /// Count total comments including nested replies
    fn count_total(comments: &[Comment]) -> usize {
        comments.iter().map(|c| c.total_count()).sum()
    }

    /// Render comment summary statistics
    pub fn render_stats(&self, comments: &[Comment]) -> String {
        let total = Self::count_total(comments);
        let top_level = comments.len();
        let replies = total - top_level;
        let best_count = Self::count_best(comments);
        let max_depth = Self::calculate_max_depth(comments);

        format!(
            "📊 **댓글 통계**: 총 {total} 개 (최상위 {top_level} | 답글 {replies} | BEST {best_count}) | 최대 깊이: {max_depth}"
        )
    }

    /// Count best comments
    fn count_best(comments: &[Comment]) -> usize {
        fn count_recursive(comment: &Comment) -> usize {
            let mut count = if comment.is_best { 1 } else { 0 };
            for reply in &comment.replies {
                count += count_recursive(reply);
            }
            count
        }
        comments.iter().map(count_recursive).sum()
    }

    /// Calculate maximum depth
    fn calculate_max_depth(comments: &[Comment]) -> usize {
        fn depth_of(comment: &Comment, current: usize) -> usize {
            if comment.replies.is_empty() {
                current
            } else {
                comment
                    .replies
                    .iter()
                    .map(|r| depth_of(r, current + 1))
                    .max()
                    .unwrap_or(current)
            }
        }
        comments.iter().map(|c| depth_of(c, 1)).max().unwrap_or(0)
    }
}

impl Default for CommentRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Article with Comments
// ============================================================================

/// Rendered article data including comments
#[derive(Debug, Serialize)]
pub struct ArticleWithCommentsData {
    // Article fields
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub publisher: String,
    pub author: String,
    pub published_at: String,
    pub crawled_at: String,
    pub url: String,
    pub oid: String,
    pub aid: String,
    pub content_hash: String,

    // Comment fields
    pub has_comments: bool,
    pub comment_count: usize,
    pub comments_markdown: String,
    pub comment_stats: String,
}

impl ArticleWithCommentsData {
    /// Create from article and comments
    pub fn from_article_and_comments(
        article: &ParsedArticle,
        comments: &[Comment],
        renderer: &CommentRenderer,
    ) -> Self {
        let comment_count = comments.iter().map(|c| c.total_count()).sum();

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
            has_comments: !comments.is_empty(),
            comment_count,
            comments_markdown: renderer.render_comments(comments),
            comment_stats: renderer.render_stats(comments),
        }
    }
}

/// Extended markdown writer that supports comments
pub struct ArticleWithCommentsWriter<'a> {
    /// Handlebars template engine
    handlebars: Handlebars<'a>,

    /// Output directory
    output_dir: PathBuf,

    /// Comment renderer
    comment_renderer: CommentRenderer,
}

/// Default template for article with comments
const ARTICLE_WITH_COMMENTS_TEMPLATE: &str = r#"---
id: {{id}}
title: "{{title}}"
category: {{category}}
publisher: {{publisher}}
author: {{author}}
published_at: {{published_at}}
crawled_at: {{crawled_at}}
url: {{url}}
oid: {{oid}}
aid: {{aid}}
content_hash: {{content_hash}}
comment_count: {{comment_count}}
---

# {{title}}

**{{publisher}}** | {{published_at}} | {{category}}

---

{{content}}

---

{{#if has_comments}}
{{comment_stats}}

{{comments_markdown}}
{{else}}
*댓글이 없습니다.*
{{/if}}

---

*Crawled at: {{crawled_at}}*
*Source: [원문 보기]({{url}})*
"#;

impl<'a> ArticleWithCommentsWriter<'a> {
    /// Create a new writer with default templates
    pub fn new(output_dir: &Path) -> Result<Self> {
        Self::with_config(output_dir, CommentRenderConfig::default())
    }

    /// Create with custom comment render config
    pub fn with_config(output_dir: &Path, comment_config: CommentRenderConfig) -> Result<Self> {
        let mut handlebars = Handlebars::new();

        // Register default template
        handlebars
            .register_template_string("article", DEFAULT_TEMPLATE)
            .context("Failed to register article template")?;

        // Register template with comments
        handlebars
            .register_template_string("article_with_comments", ARTICLE_WITH_COMMENTS_TEMPLATE)
            .context("Failed to register article_with_comments template")?;

        fs::create_dir_all(output_dir).context("Failed to create output directory")?;

        Ok(Self {
            handlebars,
            output_dir: output_dir.to_path_buf(),
            comment_renderer: CommentRenderer::with_config(comment_config),
        })
    }

    /// Render article with comments to markdown string
    pub fn render(&self, article: &ParsedArticle, comments: &[Comment]) -> Result<String> {
        let data = ArticleWithCommentsData::from_article_and_comments(
            article,
            comments,
            &self.comment_renderer,
        );

        self.handlebars
            .render("article_with_comments", &data)
            .context("Failed to render article with comments template")
    }

    /// Save article with comments to markdown file
    pub fn save(&self, article: &ParsedArticle, comments: &[Comment]) -> Result<PathBuf> {
        let markdown = self.render(article, comments)?;
        let filename = self.generate_filename(article, !comments.is_empty());
        let filepath = self.output_dir.join(&filename);

        let mut file = File::create(&filepath)
            .with_context(|| format!("Failed to create file: {}", filepath.display()))?;

        file.write_all(markdown.as_bytes())
            .with_context(|| format!("Failed to write to file: {}", filepath.display()))?;

        tracing::debug!(
            path = %filepath.display(),
            comments = comments.len(),
            "Saved article with comments to markdown"
        );

        Ok(filepath)
    }

    /// Generate filename for article with comments
    fn generate_filename(&self, article: &ParsedArticle, has_comments: bool) -> String {
        let sanitized_title = sanitize_filename(&article.title, 50);
        let suffix = if has_comments { "_with_comments" } else { "" };

        format!(
            "{}_{}{}{}.md",
            article.oid,
            article.aid,
            if sanitized_title.is_empty() {
                String::new()
            } else {
                format!("_{sanitized_title}")
            },
            suffix
        )
    }

    /// Get output directory
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Get comment renderer
    pub fn comment_renderer(&self) -> &CommentRenderer {
        &self.comment_renderer
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
        assert_eq!(
            sanitize_filename("한글 제목 테스트", 50),
            "한글_제목_테스트"
        );
        // "Very Long " (10 chars) -> trim -> "Very Long" -> replace -> "Very_Long" -> lowercase
        assert_eq!(
            sanitize_filename("Very Long Title That Should Be Truncated", 10),
            "very_long"
        );
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
            article.aid = format!("{i:010}");
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
        result
            .failed
            .push(("failed_id".to_string(), "error".to_string()));

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

    // ========================================================================
    // Comment Renderer Tests
    // ========================================================================

    fn create_test_comment(id: &str, content: &str, likes: i64) -> Comment {
        Comment {
            id: id.to_string(),
            parent_id: None,
            content: content.to_string(),
            author: "테스터".to_string(),
            author_id: "test****".to_string(),
            created_at: Utc::now(),
            modified_at: None,
            likes,
            dislikes: 0,
            reply_count: 0,
            is_best: false,
            is_deleted: false,
            replies: vec![],
        }
    }

    fn create_test_reply(id: &str, parent_id: &str, content: &str) -> Comment {
        Comment {
            id: id.to_string(),
            parent_id: Some(parent_id.to_string()),
            content: content.to_string(),
            author: "답글러".to_string(),
            author_id: "reply****".to_string(),
            created_at: Utc::now(),
            modified_at: None,
            likes: 0,
            dislikes: 0,
            reply_count: 0,
            is_best: false,
            is_deleted: false,
            replies: vec![],
        }
    }

    #[test]
    fn test_comment_renderer_creation() {
        let renderer = CommentRenderer::new();
        assert_eq!(renderer.config.max_depth, 10);
        assert!(renderer.config.show_reactions);
    }

    #[test]
    fn test_render_single_comment() {
        let renderer = CommentRenderer::new();
        let comment = create_test_comment("1", "테스트 댓글입니다.", 10);

        let md = renderer.render_comment(&comment, 0);

        assert!(md.contains("**테스터**"));
        assert!(md.contains("테스트 댓글입니다."));
        assert!(md.contains("👍 10"));
    }

    #[test]
    fn test_render_comment_with_depth() {
        let renderer = CommentRenderer::new();
        let comment = create_test_comment("1", "답글입니다.", 5);

        // Depth 1 - should have one ">"
        let md = renderer.render_comment(&comment, 1);
        assert!(md.starts_with("> **테스터**"));

        // Depth 2 - should have ">>"
        let md2 = renderer.render_comment(&comment, 2);
        assert!(md2.starts_with(">> **테스터**"));
    }

    #[test]
    fn test_render_deleted_comment() {
        let renderer = CommentRenderer::with_config(CommentRenderConfig {
            show_deleted_placeholder: true,
            ..Default::default()
        });

        let mut comment = create_test_comment("1", "삭제될 댓글", 0);
        comment.is_deleted = true;

        let md = renderer.render_comment(&comment, 0);
        assert!(md.contains("[삭제된 댓글입니다]"));
    }

    #[test]
    fn test_render_deleted_comment_hidden() {
        let renderer = CommentRenderer::new(); // show_deleted_placeholder = false

        let mut comment = create_test_comment("1", "삭제될 댓글", 0);
        comment.is_deleted = true;

        let md = renderer.render_comment(&comment, 0);
        assert!(md.is_empty());
    }

    #[test]
    fn test_render_best_comment() {
        let renderer = CommentRenderer::new();

        let mut comment = create_test_comment("1", "베스트 댓글!", 100);
        comment.is_best = true;

        let md = renderer.render_comment(&comment, 0);
        assert!(md.contains("⭐ **BEST**"));
    }

    #[test]
    fn test_render_comment_with_replies() {
        let renderer = CommentRenderer::new();

        let mut parent = create_test_comment("1", "부모 댓글", 10);
        let reply = create_test_reply("2", "1", "자식 답글");
        parent.replies = vec![reply];

        let md = renderer.render_comment(&parent, 0);

        // Parent should not have prefix
        assert!(md.contains("부모 댓글"));
        // Reply should have ">" prefix
        assert!(md.contains("> **답글러**"));
        assert!(md.contains("> 자식 답글"));
    }

    #[test]
    fn test_render_nested_replies() {
        let renderer = CommentRenderer::new();

        let mut grandchild = create_test_reply("3", "2", "손자 답글");
        grandchild.replies = vec![];

        let mut child = create_test_reply("2", "1", "자식 답글");
        child.replies = vec![grandchild];

        let mut parent = create_test_comment("1", "부모 댓글", 10);
        parent.replies = vec![child];

        let md = renderer.render_comment(&parent, 0);

        // Check hierarchical indentation
        assert!(md.contains("부모 댓글")); // No prefix
        assert!(md.contains("> 자식 답글")); // One >
        assert!(md.contains(">> 손자 답글")); // Two >>
    }

    #[test]
    fn test_render_comments_list() {
        let renderer = CommentRenderer::new();

        let comments = vec![
            create_test_comment("1", "첫 번째 댓글", 5),
            create_test_comment("2", "두 번째 댓글", 10),
        ];

        let md = renderer.render_comments(&comments);

        assert!(md.contains("## 댓글 (2 개)"));
        assert!(md.contains("첫 번째 댓글"));
        assert!(md.contains("두 번째 댓글"));
        // Should have separator between comments
        assert!(md.contains("---"));
    }

    #[test]
    fn test_render_empty_comments() {
        let renderer = CommentRenderer::new();
        let comments: Vec<Comment> = vec![];

        let md = renderer.render_comments(&comments);
        assert!(md.is_empty());
    }

    #[test]
    fn test_render_stats() {
        let renderer = CommentRenderer::new();

        let mut parent = create_test_comment("1", "부모", 10);
        parent.is_best = true;
        let reply = create_test_reply("2", "1", "답글");
        parent.replies = vec![reply];

        let comments = vec![parent];
        let stats = renderer.render_stats(&comments);

        assert!(stats.contains("총 2 개"));
        assert!(stats.contains("최상위 1"));
        assert!(stats.contains("답글 1"));
        assert!(stats.contains("BEST 1"));
        assert!(stats.contains("최대 깊이: 2"));
    }

    #[test]
    fn test_max_depth_limit() {
        let renderer = CommentRenderer::with_config(CommentRenderConfig {
            max_depth: 3,
            ..Default::default()
        });

        let comment = create_test_comment("1", "깊은 댓글", 0);

        // Depth 5 should be capped at max_depth (3)
        let md = renderer.render_comment(&comment, 5);

        // Should have exactly 3 ">" characters
        let prefix_count = md
            .lines()
            .next()
            .unwrap()
            .chars()
            .take_while(|c| *c == '>')
            .count();
        assert_eq!(prefix_count, 3);
    }

    #[test]
    fn test_comment_render_config_custom() {
        let config = CommentRenderConfig {
            max_depth: 5,
            show_author_id: true,
            show_reactions: false,
            show_timestamp: false,
            highlight_best: false,
            show_deleted_placeholder: true,
        };

        let renderer = CommentRenderer::with_config(config);
        let comment = create_test_comment("1", "테스트", 10);

        let md = renderer.render_comment(&comment, 0);

        // Should show author ID
        assert!(md.contains("(test****)"));
        // Should NOT show reactions
        assert!(!md.contains("👍"));
    }

    // ========================================================================
    // Article with Comments Writer Tests
    // ========================================================================

    #[test]
    fn test_article_with_comments_writer_creation() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ArticleWithCommentsWriter::new(temp_dir.path());
        assert!(writer.is_ok());
    }

    #[test]
    fn test_render_article_with_comments() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ArticleWithCommentsWriter::new(temp_dir.path()).unwrap();

        let article = create_test_article();
        let comments = vec![
            create_test_comment("1", "좋은 기사네요!", 50),
            create_test_comment("2", "동의합니다.", 20),
        ];

        let md = writer.render(&article, &comments);
        assert!(md.is_ok());

        let content = md.unwrap();
        // Article content
        assert!(content.contains("테스트 기사 제목입니다"));
        assert!(content.contains("테스트 기사의 본문 내용입니다"));
        // Comment count
        assert!(content.contains("comment_count: 2"));
        // Comments
        assert!(content.contains("좋은 기사네요!"));
        assert!(content.contains("동의합니다."));
        // Stats
        assert!(content.contains("댓글 통계"));
    }

    #[test]
    fn test_render_article_without_comments() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ArticleWithCommentsWriter::new(temp_dir.path()).unwrap();

        let article = create_test_article();
        let comments: Vec<Comment> = vec![];

        let md = writer.render(&article, &comments);
        assert!(md.is_ok());

        let content = md.unwrap();
        assert!(content.contains("댓글이 없습니다"));
    }

    #[test]
    fn test_save_article_with_comments() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ArticleWithCommentsWriter::new(temp_dir.path()).unwrap();

        let article = create_test_article();
        let comments = vec![create_test_comment("1", "테스트 댓글", 10)];

        let path = writer.save(&article, &comments);
        assert!(path.is_ok());

        let filepath = path.unwrap();
        assert!(filepath.exists());
        assert!(filepath.to_string_lossy().contains("_with_comments"));
    }

    #[test]
    fn test_save_article_without_comments_filename() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ArticleWithCommentsWriter::new(temp_dir.path()).unwrap();

        let article = create_test_article();
        let comments: Vec<Comment> = vec![];

        let path = writer.save(&article, &comments);
        assert!(path.is_ok());

        let filepath = path.unwrap();
        // Should NOT have "_with_comments" suffix
        assert!(!filepath.to_string_lossy().contains("_with_comments"));
    }

    #[test]
    fn test_article_with_comments_data() {
        let renderer = CommentRenderer::new();
        let article = create_test_article();

        let mut parent = create_test_comment("1", "부모 댓글", 10);
        let reply = create_test_reply("2", "1", "답글");
        parent.replies = vec![reply];
        let comments = vec![parent];

        let data =
            ArticleWithCommentsData::from_article_and_comments(&article, &comments, &renderer);

        assert!(data.has_comments);
        assert_eq!(data.comment_count, 2); // 1 parent + 1 reply
        assert!(!data.comments_markdown.is_empty());
        assert!(!data.comment_stats.is_empty());
    }
}
