//! Text sanitization utilities for cleaning extracted article content
//!
//! This module provides functions to clean and normalize text extracted from
//! HTML pages, removing unwanted characters, normalizing whitespace, and
//! decoding HTML entities.

use regex::Regex;
use std::sync::LazyLock;

// Pre-compiled regex patterns for performance
static WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").unwrap());

static MULTI_NEWLINE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

static TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());

static BYLINE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)(^.*기자\s*=.*$|.*기자$|\S+@\S+\.\S+)").unwrap());

/// Sanitize extracted text content
///
/// This function applies multiple cleaning steps:
/// 1. Remove zero-width characters
/// 2. Remove control characters (except newline/tab)
/// 3. Decode HTML entities
/// 4. Normalize whitespace
/// 5. Trim each line
/// 6. Remove excessive blank lines
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::sanitize_text;
///
/// let dirty = "Hello\u{200B}World  \n\n\n\nTest";
/// let clean = sanitize_text(dirty);
/// assert!(!clean.contains('\u{200B}'));
/// ```
pub fn sanitize_text(text: &str) -> String {
    let mut result = text.to_string();

    result = remove_zero_width(&result);
    result = remove_control_chars(&result);
    result = decode_html_entities(&result);
    result = normalize_whitespace(&result);
    result = trim_lines(&result);
    result = collapse_newlines(&result);

    result.trim().to_string()
}

/// Remove zero-width spaces and similar invisible characters
///
/// Removes:
/// - \u{200B} Zero-width space
/// - \u{200C} Zero-width non-joiner
/// - \u{200D} Zero-width joiner
/// - \u{200E} Left-to-right mark
/// - \u{200F} Right-to-left mark
/// - \u{2028} Line separator
/// - \u{2029} Paragraph separator
/// - \u{202A}-\u{202F} Various formatting characters
/// - \u{FEFF} Byte order mark (BOM)
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::remove_zero_width;
///
/// let text = "가\u{200B}나\u{FEFF}다";
/// let clean = remove_zero_width(text);
/// assert_eq!(clean, "가나다");
/// ```
pub fn remove_zero_width(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(*c,
                '\u{200B}'..='\u{200F}' |
                '\u{2028}'..='\u{202F}' |
                '\u{FEFF}'
            )
        })
        .collect()
}

/// Remove control characters except newline and tab
///
/// Keeps \n and \t but removes all other control chars (0x00-0x1F, 0x7F)
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::remove_control_chars;
///
/// let text = "Hello\x00World\x07Test\nNewline";
/// let clean = remove_control_chars(text);
/// assert!(!clean.contains('\x00'));
/// assert!(clean.contains('\n')); // Newline preserved
/// ```
pub fn remove_control_chars(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

/// Decode common HTML entities to plain text
///
/// Decodes:
/// - &nbsp; -> space
/// - &amp; -> &
/// - &lt; -> <
/// - &gt; -> >
/// - &quot; -> "
/// - &#39; -> '
/// - &#x27; -> '
/// - &#xa0; -> space (non-breaking space)
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::decode_html_entities;
///
/// let text = "&lt;div&gt;Hello &amp; World&lt;/div&gt;";
/// let decoded = decode_html_entities(text);
/// assert_eq!(decoded, "<div>Hello & World</div>");
/// ```
pub fn decode_html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&#xa0;", " ")
        .replace("&#160;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
}

/// Normalize multiple spaces/tabs to single space
///
/// Converts sequences of spaces and tabs to a single space.
/// Does NOT affect newlines - those are handled separately.
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::normalize_whitespace;
///
/// let text = "Hello    World\t\tTest";
/// let normalized = normalize_whitespace(text);
/// assert_eq!(normalized, "Hello World Test");
/// ```
pub fn normalize_whitespace(text: &str) -> String {
    WHITESPACE_REGEX.replace_all(text, " ").to_string()
}

/// Trim whitespace from each line
///
/// Removes leading and trailing whitespace from each line
/// while preserving the line structure.
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::trim_lines;
///
/// let text = "  Line 1  \n  Line 2  ";
/// let trimmed = trim_lines(text);
/// assert_eq!(trimmed, "Line 1\nLine 2");
/// ```
pub fn trim_lines(text: &str) -> String {
    text.lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collapse excessive newlines to maximum of 2
///
/// Replaces 3+ consecutive newlines with just 2 newlines.
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::collapse_newlines;
///
/// let text = "Para 1\n\n\n\n\nPara 2";
/// let collapsed = collapse_newlines(text);
/// assert_eq!(collapsed, "Para 1\n\nPara 2");
/// ```
pub fn collapse_newlines(text: &str) -> String {
    MULTI_NEWLINE_REGEX.replace_all(text, "\n\n").to_string()
}

/// Extract plain text from HTML, removing all tags
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::strip_html_tags;
///
/// let html = "<p>Hello <strong>World</strong></p>";
/// let plain = strip_html_tags(html);
/// assert_eq!(plain, "Hello World");
/// ```
pub fn strip_html_tags(html: &str) -> String {
    TAG_REGEX.replace_all(html, "").to_string()
}

/// Check if text contains meaningful content
///
/// Returns false if text is empty or only whitespace
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::has_content;
///
/// assert!(has_content("Hello"));
/// assert!(!has_content(""));
/// assert!(!has_content("   \n\t  "));
/// ```
pub fn has_content(text: &str) -> bool {
    !text.trim().is_empty()
}

/// Truncate text to max length with ellipsis
///
/// If text is longer than max_len, truncates and adds "..."
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::truncate;
///
/// let text = "Hello World";
/// assert_eq!(truncate(text, 5), "He...");
/// assert_eq!(truncate(text, 20), "Hello World");
/// ```
pub fn truncate(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

/// Remove reporter/journalist byline patterns
///
/// Common patterns:
/// - "○○○ 기자" at the end
/// - "기자 = ○○○" at the beginning
/// - Email addresses
///
/// # Examples
///
/// ```
/// use baram::parser::sanitize::remove_byline;
///
/// let text = "기사 내용입니다.\n홍길동 기자";
/// let clean = remove_byline(text);
/// assert!(!clean.contains("기자"));
/// ```
pub fn remove_byline(text: &str) -> String {
    let result = BYLINE_REGEX.replace_all(text, "");
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_text_full() {
        let dirty = "Hello\u{200B}World  \n\n\n\nTest";
        let clean = sanitize_text(dirty);
        assert!(!clean.contains('\u{200B}'));
        assert!(!clean.contains("    ")); // No multiple spaces
    }

    #[test]
    fn test_remove_zero_width() {
        let text = "가\u{200B}나\u{FEFF}다";
        let clean = remove_zero_width(text);
        assert_eq!(clean, "가나다");
    }

    #[test]
    fn test_remove_control_chars() {
        let text = "Hello\x00World\x07Test\nNewline";
        let clean = remove_control_chars(text);
        assert!(!clean.contains('\x00'));
        assert!(!clean.contains('\x07'));
        assert!(clean.contains('\n')); // Newline preserved
    }

    #[test]
    fn test_decode_html_entities() {
        let text = "&lt;div&gt;Hello &amp; World&lt;/div&gt;";
        let decoded = decode_html_entities(text);
        assert_eq!(decoded, "<div>Hello & World</div>");
    }

    #[test]
    fn test_normalize_whitespace() {
        let text = "Hello    World\t\tTest";
        let normalized = normalize_whitespace(text);
        assert_eq!(normalized, "Hello World Test");
    }

    #[test]
    fn test_trim_lines() {
        let text = "  Line 1  \n  Line 2  ";
        let trimmed = trim_lines(text);
        assert_eq!(trimmed, "Line 1\nLine 2");
    }

    #[test]
    fn test_collapse_newlines() {
        let text = "Para 1\n\n\n\n\nPara 2";
        let collapsed = collapse_newlines(text);
        assert_eq!(collapsed, "Para 1\n\nPara 2");
    }

    #[test]
    fn test_strip_html_tags() {
        let html = "<p>Hello <strong>World</strong></p>";
        let plain = strip_html_tags(html);
        assert_eq!(plain, "Hello World");
    }

    #[test]
    fn test_has_content() {
        assert!(has_content("Hello"));
        assert!(!has_content(""));
        assert!(!has_content("   \n\t  "));
    }

    #[test]
    fn test_truncate() {
        let text = "Hello World";
        assert_eq!(truncate(text, 5), "He...");
        assert_eq!(truncate(text, 20), "Hello World");
    }

    #[test]
    fn test_truncate_korean() {
        let text = "안녕하세요 반갑습니다";
        let truncated = truncate(text, 5);
        assert_eq!(truncated, "안녕...");
    }

    #[test]
    fn test_remove_byline() {
        let text = "기사 내용입니다.\n홍길동 기자";
        let clean = remove_byline(text);
        assert!(!clean.contains("기자"));
    }

    #[test]
    fn test_nbsp_handling() {
        let text = "Hello&nbsp;World&#xa0;Test";
        let decoded = decode_html_entities(text);
        assert_eq!(decoded, "Hello World Test");
    }

    #[test]
    fn test_zero_width_comprehensive() {
        let text = "Test\u{200B}\u{200C}\u{200D}\u{200E}\u{200F}Complete";
        let clean = remove_zero_width(text);
        assert_eq!(clean, "TestComplete");
    }

    #[test]
    fn test_line_separator_removal() {
        let text = "Line1\u{2028}Line2\u{2029}Line3";
        let clean = remove_zero_width(text);
        assert_eq!(clean, "Line1Line2Line3");
    }

    #[test]
    fn test_bom_removal() {
        let text = "\u{FEFF}Content";
        let clean = remove_zero_width(text);
        assert_eq!(clean, "Content");
    }

    #[test]
    fn test_tab_preservation() {
        let text = "Col1\tCol2\tCol3";
        let clean = remove_control_chars(text);
        assert!(clean.contains('\t'));
    }

    #[test]
    fn test_sanitize_complex_html_entities() {
        let text = "&nbsp;&#160;&#xa0;&amp;&lt;&gt;&quot;&#39;&#x27;&apos;";
        let decoded = decode_html_entities(text);
        assert_eq!(decoded, "   &<>\"'''");
    }

    #[test]
    fn test_empty_lines_trimming() {
        let text = "\n\n  \n\n\nContent\n\n  \n\n";
        let clean = sanitize_text(text);
        assert_eq!(clean, "Content");
    }

    #[test]
    fn test_mixed_whitespace_normalization() {
        let text = "Word1   \t  \t   Word2";
        let normalized = normalize_whitespace(text);
        assert_eq!(normalized, "Word1 Word2");
    }

    #[test]
    fn test_byline_email_removal() {
        let text = "기사 내용\nreporter@example.com";
        let clean = remove_byline(text);
        assert!(!clean.contains('@'));
    }

    #[test]
    fn test_byline_pattern_beginning() {
        let text = "기자 = 홍길동\n기사 내용입니다.";
        let clean = remove_byline(text);
        assert_eq!(clean, "기사 내용입니다.");
    }

    #[test]
    fn test_truncate_exact_length() {
        let text = "12345";
        assert_eq!(truncate(text, 5), "12345");
    }

    #[test]
    fn test_truncate_zero_length() {
        let text = "Hello";
        assert_eq!(truncate(text, 0), "...");
    }

    #[test]
    fn test_has_content_with_mixed_whitespace() {
        assert!(!has_content("\n\t   \r"));
        assert!(has_content("\n\t a \r"));
    }

    #[test]
    fn test_strip_html_nested_tags() {
        let html = "<div><p>Para <span>with <em>nested</em> tags</span></p></div>";
        let plain = strip_html_tags(html);
        assert_eq!(plain, "Para with nested tags");
    }

    #[test]
    fn test_full_pipeline() {
        let dirty = "\u{FEFF}  &lt;div&gt;\u{200B}Hello    World\n\n\n\n&amp;  Test&lt;/div&gt;  ";
        let clean = sanitize_text(dirty);
        assert!(!clean.contains('\u{FEFF}'));
        assert!(!clean.contains('\u{200B}'));
        assert!(clean.contains("Hello World"));
        assert!(clean.contains('&'));
        assert!(clean.contains('<'));
    }
}
