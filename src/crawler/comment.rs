//! Naver News Comment API client
//!
//! This module handles fetching and parsing comments from Naver News articles.
//! The comment API returns JSONP format which requires special parsing.

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::crawler::fetcher::NaverFetcher;

// ============================================================================
// JSONP Parser
// ============================================================================

/// Regex for extracting JSON from JSONP response
/// Format: _callback({...}) or jQuery12345({...})
static JSONP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z_$][a-zA-Z0-9_$]*\s*\(\s*(.*)\s*\);?\s*$").unwrap()
});

/// Parse JSONP response and extract JSON content
///
/// # Arguments
/// * `jsonp` - JSONP response string (e.g., `_callback({"key": "value"})`)
///
/// # Returns
/// JSON string without the callback wrapper
///
/// # Example
/// ```
/// use ntimes::crawler::comment::parse_jsonp;
///
/// let jsonp = r#"_callback({"success": true})"#;
/// let json = parse_jsonp(jsonp).unwrap();
/// assert_eq!(json, r#"{"success": true}"#);
/// ```
pub fn parse_jsonp(jsonp: &str) -> Result<String> {
    let trimmed = jsonp.trim();

    // Try to match JSONP pattern
    if let Some(captures) = JSONP_REGEX.captures(trimmed) {
        if let Some(json_match) = captures.get(1) {
            return Ok(json_match.as_str().to_string());
        }
    }

    // If no match, check if it's already valid JSON
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Ok(trimmed.to_string());
    }

    anyhow::bail!("Invalid JSONP format: unable to extract JSON content")
}

// ============================================================================
// API Response Structures
// ============================================================================

/// Root response from comment API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommentApiResponse {
    /// Success status
    pub success: bool,

    /// Response code (e.g., "200")
    pub code: String,

    /// Message (optional)
    #[serde(default)]
    pub message: Option<String>,

    /// Comment result data
    pub result: Option<CommentResult>,
}

/// Comment result containing list and pagination info
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentResult {
    /// Total comment count
    #[serde(default)]
    pub count: CommentCount,

    /// Page information
    #[serde(default)]
    pub page_info: Option<PageInfo>,

    /// List of comments
    #[serde(default)]
    pub comment_list: Vec<RawComment>,
}

/// Comment count information
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentCount {
    /// Total comment count
    #[serde(default)]
    pub comment: i64,

    /// Total reply count
    #[serde(default)]
    pub reply: i64,

    /// Deleted comment count
    #[serde(default)]
    pub deleted: i64,
}

/// Page information for pagination
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// Current page number
    #[serde(default)]
    pub page: i32,

    /// Total pages
    #[serde(default)]
    pub total_pages: i32,

    /// Page size
    #[serde(default)]
    pub page_size: i32,

    /// Index of first item
    #[serde(default)]
    pub index_size: i32,
}

/// Raw comment data from API
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawComment {
    /// Comment number (unique ID)
    #[serde(default)]
    pub comment_no: i64,

    /// Parent comment number (0 if top-level)
    #[serde(default)]
    pub parent_comment_no: i64,

    /// Comment content (may contain HTML)
    #[serde(default)]
    pub contents: String,

    /// Masked user ID
    #[serde(default)]
    pub masked_user_id: String,

    /// User display name
    #[serde(default)]
    pub user_name: String,

    /// Profile user ID (Naver ID, may be null)
    #[serde(default)]
    pub profile_user_id: Option<String>,

    /// Registration time (milliseconds timestamp)
    #[serde(default)]
    pub reg_time: i64,

    /// Modification time (milliseconds timestamp)
    #[serde(default)]
    pub mod_time: i64,

    /// Sympathy (like) count
    #[serde(default)]
    pub sympathy_count: i64,

    /// Antipathy (dislike) count
    #[serde(default)]
    pub antipathy_count: i64,

    /// Reply count
    #[serde(default)]
    pub reply_count: i64,

    /// Whether this is a best comment
    #[serde(default)]
    pub best: bool,

    /// Whether the comment is visible
    #[serde(default = "default_true")]
    pub visible: bool,

    /// Whether the comment is deleted
    #[serde(default)]
    pub deleted: bool,

    /// User ID type
    #[serde(default)]
    pub user_id_type: String,

    /// Whether user ID is exposed
    #[serde(default)]
    pub expose_user_id: bool,

    /// Ticket ID
    #[serde(default)]
    pub ticket: String,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Converted Comment Structure
// ============================================================================

/// Cleaned and normalized comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    /// Unique comment ID
    pub id: String,

    /// Parent comment ID (None if top-level)
    pub parent_id: Option<String>,

    /// Comment content (cleaned)
    pub content: String,

    /// Author name
    pub author: String,

    /// Author ID (masked)
    pub author_id: String,

    /// Creation time
    pub created_at: DateTime<Utc>,

    /// Modification time
    pub modified_at: Option<DateTime<Utc>>,

    /// Like count
    pub likes: i64,

    /// Dislike count
    pub dislikes: i64,

    /// Reply count
    pub reply_count: i64,

    /// Whether this is a best comment
    pub is_best: bool,

    /// Whether the comment is deleted
    pub is_deleted: bool,

    /// Nested replies (populated during tree building)
    #[serde(default)]
    pub replies: Vec<Comment>,
}

impl Comment {
    /// Check if this is a top-level comment (not a reply)
    pub fn is_top_level(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Check if this comment has replies
    pub fn has_replies(&self) -> bool {
        self.reply_count > 0 || !self.replies.is_empty()
    }

    /// Get total count including nested replies
    pub fn total_count(&self) -> usize {
        1 + self.replies.iter().map(|r| r.total_count()).sum::<usize>()
    }
}

// ============================================================================
// Comment Conversion
// ============================================================================

/// Convert raw API comment to cleaned Comment structure
///
/// # Arguments
/// * `raw` - Raw comment from API
///
/// # Returns
/// Cleaned Comment structure
///
/// # Example
/// ```
/// use ntimes::crawler::comment::{RawComment, convert_comment};
///
/// let raw = RawComment {
///     comment_no: 12345,
///     parent_comment_no: 0,
///     contents: "테스트 댓글입니다.".to_string(),
///     user_name: "테스터".to_string(),
///     masked_user_id: "test****".to_string(),
///     reg_time: 1702684800000,
///     mod_time: 0,
///     sympathy_count: 10,
///     antipathy_count: 2,
///     reply_count: 3,
///     best: false,
///     visible: true,
///     deleted: false,
///     ..Default::default()
/// };
///
/// let comment = convert_comment(&raw);
/// assert_eq!(comment.id, "12345");
/// assert_eq!(comment.likes, 10);
/// ```
pub fn convert_comment(raw: &RawComment) -> Comment {
    // Convert timestamp (milliseconds) to DateTime
    let created_at = timestamp_to_datetime(raw.reg_time);
    let modified_at = if raw.mod_time > 0 && raw.mod_time != raw.reg_time {
        Some(timestamp_to_datetime(raw.mod_time))
    } else {
        None
    };

    // Determine parent ID
    let parent_id = if raw.parent_comment_no > 0 {
        Some(raw.parent_comment_no.to_string())
    } else {
        None
    };

    // Clean content
    let content = clean_comment_content(&raw.contents);

    Comment {
        id: raw.comment_no.to_string(),
        parent_id,
        content,
        author: raw.user_name.clone(),
        author_id: raw.masked_user_id.clone(),
        created_at,
        modified_at,
        likes: raw.sympathy_count,
        dislikes: raw.antipathy_count,
        reply_count: raw.reply_count,
        is_best: raw.best,
        is_deleted: raw.deleted || !raw.visible,
        replies: Vec::new(),
    }
}

/// Convert multiple raw comments
pub fn convert_comments(raw_comments: &[RawComment]) -> Vec<Comment> {
    raw_comments
        .iter()
        .filter(|c| c.visible && !c.deleted)
        .map(convert_comment)
        .collect()
}

/// Convert timestamp (milliseconds) to DateTime<Utc>
fn timestamp_to_datetime(timestamp_ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .unwrap_or_else(Utc::now)
}

/// Clean comment content
///
/// - Remove HTML tags
/// - Decode HTML entities
/// - Normalize whitespace
fn clean_comment_content(content: &str) -> String {
    static HTML_TAG_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());

    static WHITESPACE_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s+").unwrap());

    // Remove HTML tags
    let no_tags = HTML_TAG_REGEX.replace_all(content, "");

    // Decode HTML entities
    let decoded = html_escape::decode_html_entities(&no_tags);

    // Normalize whitespace
    let normalized = WHITESPACE_REGEX.replace_all(&decoded, " ");

    normalized.trim().to_string()
}

// ============================================================================
// Comment Tree Builder
// ============================================================================

/// Build comment tree from flat list
///
/// # Arguments
/// * `comments` - Flat list of comments
///
/// # Returns
/// Hierarchical list of top-level comments with nested replies
pub fn build_comment_tree(comments: Vec<Comment>) -> Vec<Comment> {
    use std::collections::HashMap;

    // Separate top-level comments and replies
    let mut top_level: Vec<Comment> = Vec::new();
    let mut replies_map: HashMap<String, Vec<Comment>> = HashMap::new();

    for comment in comments {
        if let Some(ref parent_id) = comment.parent_id {
            replies_map
                .entry(parent_id.clone())
                .or_default()
                .push(comment);
        } else {
            top_level.push(comment);
        }
    }

    // Attach replies to their parents
    fn attach_replies(comment: &mut Comment, replies_map: &mut HashMap<String, Vec<Comment>>) {
        if let Some(mut replies) = replies_map.remove(&comment.id) {
            // Recursively attach nested replies
            for reply in &mut replies {
                attach_replies(reply, replies_map);
            }
            // Sort replies by creation time
            replies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            comment.replies = replies;
        }
    }

    for comment in &mut top_level {
        attach_replies(comment, &mut replies_map);
    }

    // Sort top-level by likes (best first) then by time
    top_level.sort_by(|a, b| {
        if a.is_best != b.is_best {
            b.is_best.cmp(&a.is_best)
        } else {
            b.likes.cmp(&a.likes)
        }
    });

    top_level
}

// ============================================================================
// Comment API Client
// ============================================================================

/// Naver News Comment API endpoints
pub mod api {
    /// Base URL for comment API
    pub const COMMENT_API_BASE: &str = "https://apis.naver.com/commentBox/cbox/web_naver_list_jsonp.json";

    /// Default ticket ID
    pub const TICKET: &str = "news";

    /// Default template ID
    pub const TEMPLATE_ID: &str = "default_it";

    /// Default pool
    pub const POOL: &str = "cbox5";

    /// Default language
    pub const LANG: &str = "ko";

    /// Default country
    pub const COUNTRY: &str = "KR";

    /// Default page size
    pub const PAGE_SIZE: u32 = 100;

    /// Maximum pages to fetch (safety limit)
    pub const MAX_PAGES: u32 = 100;
}

/// Comment API client
pub struct CommentClient {
    fetcher: NaverFetcher,
}

impl CommentClient {
    /// Create new comment client
    ///
    /// # Arguments
    /// * `rate_limit` - Requests per second
    pub fn new(rate_limit: u32) -> Result<Self> {
        let fetcher = NaverFetcher::new(rate_limit)?;
        Ok(Self { fetcher })
    }

    /// Create from existing fetcher
    pub fn with_fetcher(fetcher: NaverFetcher) -> Self {
        Self { fetcher }
    }

    /// Build comment API URL
    ///
    /// # Arguments
    /// * `oid` - News outlet ID
    /// * `aid` - Article ID
    /// * `page` - Page number (1-based)
    /// * `page_size` - Number of comments per page
    /// * `sort` - Sort order ("new" or "favorite")
    pub fn build_url(oid: &str, aid: &str, page: u32, page_size: u32, sort: &str) -> String {
        // Object ID format: news{oid},{aid}
        let object_id = format!("news{oid},{aid}");

        format!(
            "{}?ticket={}&templateId={}&pool={}&lang={}&country={}&objectId={}&pageSize={}&page={}&sort={}&_callback=_callback",
            api::COMMENT_API_BASE,
            api::TICKET,
            api::TEMPLATE_ID,
            api::POOL,
            api::LANG,
            api::COUNTRY,
            object_id,
            page_size,
            page,
            sort
        )
    }

    /// Fetch comments for an article
    ///
    /// # Arguments
    /// * `oid` - News outlet ID
    /// * `aid` - Article ID
    /// * `page` - Page number (1-based)
    /// * `sort` - Sort order ("new" or "favorite")
    ///
    /// # Returns
    /// Comment API response
    pub async fn fetch_comments(
        &self,
        oid: &str,
        aid: &str,
        page: u32,
        sort: &str,
    ) -> Result<CommentApiResponse> {
        let url = Self::build_url(oid, aid, page, api::PAGE_SIZE, sort);

        tracing::debug!(url = %url, "Fetching comments");

        let response = self.fetcher.fetch(&url).await?;
        let jsonp = response.text().await.context("Failed to read response")?;

        // Parse JSONP
        let json = parse_jsonp(&jsonp).context("Failed to parse JSONP")?;

        // Deserialize JSON
        let api_response: CommentApiResponse =
            serde_json::from_str(&json).context("Failed to deserialize comment response")?;

        if !api_response.success {
            anyhow::bail!(
                "Comment API error: {} - {}",
                api_response.code,
                api_response.message.unwrap_or_default()
            );
        }

        Ok(api_response)
    }

    /// Fetch all comments for an article (with pagination)
    ///
    /// # Arguments
    /// * `oid` - News outlet ID
    /// * `aid` - Article ID
    /// * `max_pages` - Maximum pages to fetch (0 = all)
    ///
    /// # Returns
    /// All comments as a flat list
    pub async fn fetch_all_comments(
        &self,
        oid: &str,
        aid: &str,
        max_pages: u32,
    ) -> Result<Vec<Comment>> {
        let mut all_comments = Vec::new();
        let mut page = 1;
        let max = if max_pages == 0 { api::MAX_PAGES } else { max_pages };

        loop {
            if page > max {
                tracing::debug!(page, max, "Reached maximum page limit");
                break;
            }

            let response = self.fetch_comments(oid, aid, page, "new").await?;

            let result = match response.result {
                Some(r) => r,
                None => break,
            };

            if result.comment_list.is_empty() {
                break;
            }

            // Convert and collect comments
            let comments = convert_comments(&result.comment_list);
            all_comments.extend(comments);

            // Check if there are more pages
            if let Some(page_info) = &result.page_info {
                if page >= page_info.total_pages as u32 {
                    break;
                }
            }

            page += 1;
        }

        tracing::info!(
            oid = %oid,
            aid = %aid,
            total = all_comments.len(),
            "Fetched all comments"
        );

        Ok(all_comments)
    }

    /// Fetch comments as a tree structure
    ///
    /// # Arguments
    /// * `oid` - News outlet ID
    /// * `aid` - Article ID
    /// * `max_pages` - Maximum pages to fetch
    ///
    /// # Returns
    /// Hierarchical comment tree
    pub async fn fetch_comment_tree(
        &self,
        oid: &str,
        aid: &str,
        max_pages: u32,
    ) -> Result<Vec<Comment>> {
        let flat_comments = self.fetch_all_comments(oid, aid, max_pages).await?;
        Ok(build_comment_tree(flat_comments))
    }
}

// ============================================================================
// Default Implementations
// ============================================================================

impl Default for RawComment {
    fn default() -> Self {
        Self {
            comment_no: 0,
            parent_comment_no: 0,
            contents: String::new(),
            masked_user_id: String::new(),
            user_name: String::new(),
            profile_user_id: None,
            reg_time: 0,
            mod_time: 0,
            sympathy_count: 0,
            antipathy_count: 0,
            reply_count: 0,
            best: false,
            visible: true,
            deleted: false,
            user_id_type: String::new(),
            expose_user_id: false,
            ticket: String::new(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jsonp_callback() {
        let jsonp = r#"_callback({"success": true, "code": "200"})"#;
        let json = parse_jsonp(jsonp).unwrap();
        assert_eq!(json, r#"{"success": true, "code": "200"}"#);
    }

    #[test]
    fn test_parse_jsonp_jquery() {
        let jsonp = r#"jQuery123456({"data": "test"})"#;
        let json = parse_jsonp(jsonp).unwrap();
        assert_eq!(json, r#"{"data": "test"}"#);
    }

    #[test]
    fn test_parse_jsonp_with_semicolon() {
        let jsonp = r#"callback({"value": 1});"#;
        let json = parse_jsonp(jsonp).unwrap();
        assert_eq!(json, r#"{"value": 1}"#);
    }

    #[test]
    fn test_parse_jsonp_plain_json() {
        let json_str = r#"{"already": "json"}"#;
        let result = parse_jsonp(json_str).unwrap();
        assert_eq!(result, json_str);
    }

    #[test]
    fn test_parse_jsonp_invalid() {
        let invalid = "not valid jsonp or json";
        assert!(parse_jsonp(invalid).is_err());
    }

    #[test]
    fn test_convert_comment_basic() {
        let raw = RawComment {
            comment_no: 12345,
            parent_comment_no: 0,
            contents: "테스트 댓글입니다.".to_string(),
            user_name: "테스터".to_string(),
            masked_user_id: "test****".to_string(),
            reg_time: 1702684800000, // 2023-12-16 00:00:00 UTC
            sympathy_count: 10,
            antipathy_count: 2,
            reply_count: 3,
            visible: true,
            ..Default::default()
        };

        let comment = convert_comment(&raw);

        assert_eq!(comment.id, "12345");
        assert!(comment.parent_id.is_none());
        assert_eq!(comment.content, "테스트 댓글입니다.");
        assert_eq!(comment.author, "테스터");
        assert_eq!(comment.likes, 10);
        assert_eq!(comment.dislikes, 2);
        assert_eq!(comment.reply_count, 3);
        assert!(!comment.is_deleted);
    }

    #[test]
    fn test_convert_comment_reply() {
        let raw = RawComment {
            comment_no: 12346,
            parent_comment_no: 12345,
            contents: "답글입니다.".to_string(),
            user_name: "답글러".to_string(),
            visible: true,
            ..Default::default()
        };

        let comment = convert_comment(&raw);

        assert_eq!(comment.id, "12346");
        assert_eq!(comment.parent_id, Some("12345".to_string()));
        assert!(!comment.is_top_level());
    }

    #[test]
    fn test_convert_comment_deleted() {
        let raw = RawComment {
            comment_no: 12347,
            deleted: true,
            visible: false,
            ..Default::default()
        };

        let comment = convert_comment(&raw);
        assert!(comment.is_deleted);
    }

    #[test]
    fn test_clean_comment_content() {
        // HTML tags
        let html = "<b>굵은</b> 텍스트와 <a href='#'>링크</a>";
        assert_eq!(clean_comment_content(html), "굵은 텍스트와 링크");

        // HTML entities
        let entities = "안녕&amp;하세요 &lt;테스트&gt;";
        assert_eq!(clean_comment_content(entities), "안녕&하세요 <테스트>");

        // Multiple whitespace
        let whitespace = "여러   공백이    있는   텍스트";
        assert_eq!(clean_comment_content(whitespace), "여러 공백이 있는 텍스트");
    }

    #[test]
    fn test_build_comment_tree() {
        let comments = vec![
            Comment {
                id: "1".to_string(),
                parent_id: None,
                content: "Top 1".to_string(),
                author: "A".to_string(),
                author_id: "a***".to_string(),
                created_at: Utc::now(),
                modified_at: None,
                likes: 10,
                dislikes: 0,
                reply_count: 2,
                is_best: false,
                is_deleted: false,
                replies: vec![],
            },
            Comment {
                id: "2".to_string(),
                parent_id: Some("1".to_string()),
                content: "Reply to 1".to_string(),
                author: "B".to_string(),
                author_id: "b***".to_string(),
                created_at: Utc::now(),
                modified_at: None,
                likes: 5,
                dislikes: 0,
                reply_count: 0,
                is_best: false,
                is_deleted: false,
                replies: vec![],
            },
            Comment {
                id: "3".to_string(),
                parent_id: None,
                content: "Top 2".to_string(),
                author: "C".to_string(),
                author_id: "c***".to_string(),
                created_at: Utc::now(),
                modified_at: None,
                likes: 20,
                dislikes: 0,
                reply_count: 0,
                is_best: true,
                is_deleted: false,
                replies: vec![],
            },
        ];

        let tree = build_comment_tree(comments);

        // Should have 2 top-level comments
        assert_eq!(tree.len(), 2);

        // Best comment should be first
        assert!(tree[0].is_best);
        assert_eq!(tree[0].id, "3");

        // Second should have a reply
        assert_eq!(tree[1].id, "1");
        assert_eq!(tree[1].replies.len(), 1);
        assert_eq!(tree[1].replies[0].id, "2");
    }

    #[test]
    fn test_build_url() {
        let url = CommentClient::build_url("001", "0014000001", 1, 100, "new");

        assert!(url.contains("objectId=news001,0014000001"));
        assert!(url.contains("page=1"));
        assert!(url.contains("pageSize=100"));
        assert!(url.contains("sort=new"));
    }

    #[test]
    fn test_comment_total_count() {
        let comment = Comment {
            id: "1".to_string(),
            parent_id: None,
            content: "Top".to_string(),
            author: "A".to_string(),
            author_id: "a***".to_string(),
            created_at: Utc::now(),
            modified_at: None,
            likes: 0,
            dislikes: 0,
            reply_count: 2,
            is_best: false,
            is_deleted: false,
            replies: vec![
                Comment {
                    id: "2".to_string(),
                    parent_id: Some("1".to_string()),
                    content: "Reply 1".to_string(),
                    author: "B".to_string(),
                    author_id: "b***".to_string(),
                    created_at: Utc::now(),
                    modified_at: None,
                    likes: 0,
                    dislikes: 0,
                    reply_count: 0,
                    is_best: false,
                    is_deleted: false,
                    replies: vec![],
                },
                Comment {
                    id: "3".to_string(),
                    parent_id: Some("1".to_string()),
                    content: "Reply 2".to_string(),
                    author: "C".to_string(),
                    author_id: "c***".to_string(),
                    created_at: Utc::now(),
                    modified_at: None,
                    likes: 0,
                    dislikes: 0,
                    reply_count: 0,
                    is_best: false,
                    is_deleted: false,
                    replies: vec![],
                },
            ],
        };

        assert_eq!(comment.total_count(), 3);
    }

    #[test]
    fn test_deserialize_api_response() {
        let json = r#"{
            "success": true,
            "code": "200",
            "result": {
                "count": {
                    "comment": 150,
                    "reply": 50
                },
                "pageInfo": {
                    "page": 1,
                    "totalPages": 2,
                    "pageSize": 100
                },
                "commentList": [
                    {
                        "commentNo": 12345,
                        "parentCommentNo": 0,
                        "contents": "테스트",
                        "userName": "유저",
                        "maskedUserId": "user****",
                        "regTime": 1702684800000,
                        "sympathyCount": 5,
                        "antipathyCount": 1,
                        "replyCount": 2,
                        "visible": true
                    }
                ]
            }
        }"#;

        let response: CommentApiResponse = serde_json::from_str(json).unwrap();

        assert!(response.success);
        assert_eq!(response.code, "200");

        let result = response.result.unwrap();
        assert_eq!(result.count.comment, 150);
        assert_eq!(result.comment_list.len(), 1);
        assert_eq!(result.comment_list[0].comment_no, 12345);
    }
}
