//! HTML article parser with multi-format support
//!
//! This module provides the main article parsing functionality with automatic
//! format detection and fallback logic for different Naver News article types.

use chrono::{DateTime, NaiveDateTime, Utc};
use scraper::{Html, Selector};

use crate::crawler::url::UrlExtractor;
use crate::models::ParsedArticle;
use crate::parser::sanitize::{has_content, sanitize_text};
use crate::parser::selectors::{
    ArticleFormat, CardNewsSelectors, EntertainmentSelectors, GeneralSelectors, NoiseSelectors,
    SportsSelectors,
};
use crate::utils::error::ParseError;

/// Article HTML parser with multi-format support
///
/// Supports:
/// - General news (n.news.naver.com)
/// - Entertainment news (entertain.naver.com)
/// - Sports news (sports.naver.com)
/// - Card/Photo news
pub struct ArticleParser {
    general: GeneralSelectors,
    entertainment: EntertainmentSelectors,
    sports: SportsSelectors,
    card: CardNewsSelectors,
    noise: NoiseSelectors,
    url_extractor: UrlExtractor,
}

impl ArticleParser {
    #[must_use]
    pub fn new() -> Self {
        Self {
            general: GeneralSelectors::new(),
            entertainment: EntertainmentSelectors::new(),
            sports: SportsSelectors::new(),
            card: CardNewsSelectors::new(),
            noise: NoiseSelectors::new(),
            url_extractor: UrlExtractor::new(),
        }
    }

    /// Parse article with automatic format detection and fallback
    ///
    /// # Fallback Order
    /// 1. General news format (div#dic_area)
    /// 2. Entertainment format (div.article_body)
    /// 3. Sports format (div.news_end)
    /// 4. Card/Photo news format
    ///
    /// # Arguments
    /// * `html` - Raw HTML content
    /// * `url` - Article URL for ID extraction
    ///
    /// # Returns
    /// * ParsedArticle on success
    /// * ParseError if all formats fail
    ///
    /// # Errors
    /// Returns `ParseError::ArticleNotFound` if article is deleted/unavailable
    /// Returns `ParseError::UnknownFormat` if all parsers fail
    /// Returns `ParseError::IdExtractionFailed` if URL is invalid
    pub fn parse_with_fallback(&self, html: &str, url: &str) -> Result<ParsedArticle, ParseError> {
        // Check for deleted/unavailable article
        if self.is_deleted_article(html) {
            return Err(ParseError::ArticleNotFound);
        }

        let document = Html::parse_document(html);

        // Extract oid and aid from URL
        let (oid, aid) = self.url_extractor.extract_ids(url)?;

        // Detect format and try parsing
        let format = detect_format(html);

        // Try each format with fallback
        let result = match format {
            ArticleFormat::General => self.parse_general(&document, url),
            ArticleFormat::Entertainment => self.parse_entertainment(&document, url),
            ArticleFormat::Sports => self.parse_sports(&document, url),
            ArticleFormat::Card | ArticleFormat::Unknown => self.parse_card(&document, url),
        };

        // If detected format failed, try fallback chain
        match result {
            Ok(mut article) => {
                article.oid = oid;
                article.aid = aid;
                article.compute_hash();
                Ok(article)
            }
            Err(_) => self.try_fallback_chain(&document, url, &oid, &aid),
        }
    }

    /// Try all format parsers in fallback order
    fn try_fallback_chain(
        &self,
        document: &Html,
        url: &str,
        oid: &str,
        aid: &str,
    ) -> Result<ParsedArticle, ParseError> {
        // Try general
        if let Ok(mut article) = self.parse_general(document, url) {
            article.oid = oid.to_string();
            article.aid = aid.to_string();
            article.compute_hash();
            return Ok(article);
        }

        // Try entertainment
        if let Ok(mut article) = self.parse_entertainment(document, url) {
            article.oid = oid.to_string();
            article.aid = aid.to_string();
            article.compute_hash();
            return Ok(article);
        }

        // Try sports
        if let Ok(mut article) = self.parse_sports(document, url) {
            article.oid = oid.to_string();
            article.aid = aid.to_string();
            article.compute_hash();
            return Ok(article);
        }

        // Try card
        if let Ok(mut article) = self.parse_card(document, url) {
            article.oid = oid.to_string();
            article.aid = aid.to_string();
            article.compute_hash();
            return Ok(article);
        }

        Err(ParseError::UnknownFormat)
    }

    /// Parse general news format
    fn parse_general(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError> {
        let title = self
            .extract_first_match(document, &self.general.title)
            .ok_or(ParseError::TitleNotFound)?;

        let content = self
            .extract_content_text(document, &self.general.content)
            .ok_or(ParseError::ContentNotFound)?;

        if !has_content(&content) {
            return Err(ParseError::ContentNotFound);
        }

        let date = self.extract_first_match(document, &self.general.date);
        let publisher = self.extract_publisher(document, &self.general.publisher);
        let author = self.extract_first_match(document, &self.general.author);

        Ok(ParsedArticle {
            title: sanitize_text(&title),
            content: sanitize_text(&content),
            url: url.to_string(),
            published_at: date.and_then(|d| self.parse_date(&d)),
            publisher,
            author,
            crawled_at: Utc::now(),
            ..Default::default()
        })
    }

    /// Parse entertainment news format (desktop and mobile)
    fn parse_entertainment(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError> {
        let title = self
            .extract_first_match(document, &self.entertainment.title)
            .ok_or(ParseError::TitleNotFound)?;

        let content = self
            .extract_content_text(document, &self.entertainment.content)
            .ok_or(ParseError::ContentNotFound)?;

        if !has_content(&content) {
            return Err(ParseError::ContentNotFound);
        }

        let date = self.extract_first_match(document, &self.entertainment.date);
        let publisher = self.extract_first_match(document, &self.entertainment.publisher);
        let author = self.extract_first_match(document, &self.entertainment.author);

        Ok(ParsedArticle {
            title: sanitize_text(&title),
            content: sanitize_text(&content),
            url: url.to_string(),
            category: "entertainment".to_string(),
            published_at: date.and_then(|d| self.parse_date(&d)),
            publisher,
            author,
            crawled_at: Utc::now(),
            ..Default::default()
        })
    }

    /// Parse sports news format (desktop and mobile)
    fn parse_sports(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError> {
        let title = self
            .extract_first_match(document, &self.sports.title)
            .ok_or(ParseError::TitleNotFound)?;

        let content = self
            .extract_content_text(document, &self.sports.content)
            .ok_or(ParseError::ContentNotFound)?;

        if !has_content(&content) {
            return Err(ParseError::ContentNotFound);
        }

        let date = self.extract_first_match(document, &self.sports.date);
        let publisher = self.extract_first_match(document, &self.sports.publisher);
        let author = self.extract_first_match(document, &self.sports.author);

        Ok(ParsedArticle {
            title: sanitize_text(&title),
            content: sanitize_text(&content),
            url: url.to_string(),
            category: "sports".to_string(),
            published_at: date.and_then(|d| self.parse_date(&d)),
            publisher,
            author,
            crawled_at: Utc::now(),
            ..Default::default()
        })
    }

    /// Parse card/photo news format
    fn parse_card(&self, document: &Html, url: &str) -> Result<ParsedArticle, ParseError> {
        let title = self
            .extract_first_match(document, &self.card.title)
            .ok_or(ParseError::TitleNotFound)?;

        // For card news, try content areas first, then captions
        let content = self
            .extract_content_text(document, &self.card.content)
            .or_else(|| self.extract_captions(document))
            .ok_or(ParseError::ContentNotFound)?;

        Ok(ParsedArticle {
            title: sanitize_text(&title),
            content: sanitize_text(&content),
            url: url.to_string(),
            category: "card".to_string(),
            crawled_at: Utc::now(),
            ..Default::default()
        })
    }

    /// Extract first matching text from list of selectors
    fn extract_first_match(&self, document: &Html, selectors: &[Selector]) -> Option<String> {
        for selector in selectors {
            if let Some(element) = document.select(selector).next() {
                let text = element.text().collect::<String>();
                if has_content(&text) {
                    return Some(text);
                }
            }
        }
        None
    }

    /// Extract content text, cleaning noise elements
    fn extract_content_text(&self, document: &Html, selectors: &[Selector]) -> Option<String> {
        for selector in selectors {
            if let Some(element) = document.select(selector).next() {
                let html = element.html();
                let clean_html = self.remove_noise_from_html(&html);
                let text = Html::parse_fragment(&clean_html)
                    .root_element()
                    .text()
                    .collect::<String>();

                if has_content(&text) {
                    return Some(text);
                }
            }
        }
        None
    }

    /// Extract publisher name from img alt or text
    fn extract_publisher(&self, document: &Html, selectors: &[Selector]) -> Option<String> {
        for selector in selectors {
            if let Some(element) = document.select(selector).next() {
                // Try alt attribute first (for images)
                if let Some(alt) = element.value().attr("alt") {
                    if has_content(alt) {
                        return Some(alt.to_string());
                    }
                }
                // Try text content
                let text = element.text().collect::<String>();
                if has_content(&text) {
                    return Some(text);
                }
            }
        }
        None
    }

    /// Extract captions from card news images
    fn extract_captions(&self, document: &Html) -> Option<String> {
        let captions: Vec<String> = self
            .card
            .captions
            .iter()
            .flat_map(|selector| document.select(selector))
            .map(|el| el.text().collect::<String>())
            .filter(|text| has_content(text))
            .collect();

        if captions.is_empty() {
            None
        } else {
            Some(captions.join("\n\n"))
        }
    }

    /// Remove noise elements from HTML string
    fn remove_noise_from_html(&self, html: &str) -> String {
        let doc = Html::parse_fragment(html);
        let mut result = html.to_string();

        for selector in &self.noise.elements {
            for element in doc.select(selector) {
                let noise_html = element.html();
                result = result.replace(&noise_html, "");
            }
        }

        result
    }

    /// Check if article is deleted/unavailable
    /// Only checks title and error containers to avoid false positives
    /// when article content discusses deletion
    fn is_deleted_article(&self, html: &str) -> bool {
        let doc = Html::parse_document(html);

        // Indicators for deleted/unavailable articles
        let indicators = [
            "삭제된 기사",
            "없는 기사",
            "서비스 되지 않는",
            "페이지를 찾을 수 없습니다",
            "삭제되었거나",
            "존재하지 않는 기사",
            "기사가 삭제, 수정, 이동되었거나",
        ];

        // Check page title
        if let Ok(title_selector) = Selector::parse("title") {
            if let Some(title_el) = doc.select(&title_selector).next() {
                let title_text = title_el.text().collect::<String>();
                for indicator in &indicators {
                    if title_text.contains(indicator) {
                        return true;
                    }
                }
            }
        }

        // Check common error message containers
        let error_selectors = [
            ".error_content",
            ".deleted_content",
            ".article_error",
            ".news_error",
            "#ct > .error_msg",
            ".err_wrap",
        ];

        for sel_str in &error_selectors {
            if let Ok(selector) = Selector::parse(sel_str) {
                for element in doc.select(&selector) {
                    let text = element.text().collect::<String>();
                    for indicator in &indicators {
                        if text.contains(indicator) {
                            return true;
                        }
                    }
                }
            }
        }

        // Check if main content area is missing (another sign of deleted article)
        let content_selectors = ["#dic_area", ".article_body", ".news_end", "article"];
        let has_content = content_selectors.iter().any(|sel| {
            Selector::parse(sel)
                .map(|s| doc.select(&s).next().is_some())
                .unwrap_or(false)
        });

        // If no content area found and page is very short, likely deleted
        if !has_content && html.len() < 5000 {
            return true;
        }

        false
    }

    /// Parse date string to `DateTime<Utc>`
    fn parse_date(&self, date_str: &str) -> Option<DateTime<Utc>> {
        let formats = [
            "%Y.%m.%d. %H:%M",      // 2024.12.15. 14:30
            "%Y.%m.%d %H:%M",       // 2024.12.15 14:30
            "%Y-%m-%d %H:%M:%S",    // 2024-12-15 14:30:00
            "%Y-%m-%d %H:%M",       // 2024-12-15 14:30
            "%Y년 %m월 %d일 %H:%M", // 2024년 12월 15일 14:30
            "%Y.%m.%d.",            // 2024.12.15.
            "%Y.%m.%d",             // 2024.12.15
        ];

        let clean_date = date_str.trim();

        for format in &formats {
            if let Ok(dt) = NaiveDateTime::parse_from_str(clean_date, format) {
                return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
            }
        }

        // Try extracting just the date part if time parsing fails
        if let Some(date_only) = clean_date.split_whitespace().next() {
            for format in &["%Y.%m.%d.", "%Y.%m.%d", "%Y-%m-%d"] {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(date_only, format) {
                    let dt = date.and_hms_opt(0, 0, 0)?;
                    return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
                }
            }
        }

        None
    }
}

impl Default for ArticleParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect article format from HTML structure
#[must_use]
pub fn detect_format(html: &str) -> ArticleFormat {
    let doc = Html::parse_document(html);

    // Check for general news format
    if let Ok(selector) = Selector::parse("#dic_area") {
        if doc.select(&selector).next().is_some() {
            return ArticleFormat::General;
        }
    }

    // Check for entertainment format
    if let Ok(selector) = Selector::parse(".article_body, div.end_body_wrp") {
        if doc.select(&selector).next().is_some() {
            return ArticleFormat::Entertainment;
        }
    }

    // Check for sports format (desktop and mobile)
    if let Ok(selector) = Selector::parse(".news_end, div.NewsEndMain_article_body__D5MUB") {
        if doc.select(&selector).next().is_some() {
            return ArticleFormat::Sports;
        }
    }

    // Check for mobile sports/esports format (m.sports.naver.com, game.naver.com)
    if let Ok(selector) =
        Selector::parse("article.Article_comp_news_article__XIpve, article#comp_news_article")
    {
        if doc.select(&selector).next().is_some() {
            return ArticleFormat::Sports;
        }
    }

    // Check for sports format by class pattern
    if let Ok(selector) = Selector::parse("h2[class*='ArticleHead_article_title']") {
        if doc.select(&selector).next().is_some() {
            return ArticleFormat::Sports;
        }
    }

    // Check for card/photo format
    if let Ok(selector) = Selector::parse("div.end_ct_area, div.card_area") {
        if doc.select(&selector).next().is_some() {
            return ArticleFormat::Card;
        }
    }

    ArticleFormat::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = ArticleParser::new();
        assert!(!parser.general.title.is_empty());
    }

    #[test]
    fn test_detect_format_general() {
        let html = r#"<html><body><div id="dic_area">Content</div></body></html>"#;
        assert_eq!(detect_format(html), ArticleFormat::General);
    }

    #[test]
    fn test_detect_format_entertainment() {
        let html = r#"<html><body><div class="article_body">Content</div></body></html>"#;
        assert_eq!(detect_format(html), ArticleFormat::Entertainment);
    }

    #[test]
    fn test_detect_format_sports() {
        let html = r#"<html><body><div class="news_end">Content</div></body></html>"#;
        assert_eq!(detect_format(html), ArticleFormat::Sports);
    }

    #[test]
    fn test_detect_format_sports_mobile() {
        let html = r#"<html><body><article class="Article_comp_news_article__XIpve">Content</article></body></html>"#;
        assert_eq!(detect_format(html), ArticleFormat::Sports);
    }

    #[test]
    fn test_detect_format_sports_mobile_title() {
        let html = r#"<html><body><h2 class="ArticleHead_article_title__qh8GV">Title</h2></body></html>"#;
        assert_eq!(detect_format(html), ArticleFormat::Sports);
    }

    #[test]
    fn test_detect_format_unknown() {
        let html = r#"<html><body><div>Unknown</div></body></html>"#;
        assert_eq!(detect_format(html), ArticleFormat::Unknown);
    }

    #[test]
    fn test_is_deleted_article() {
        let parser = ArticleParser::new();

        // Deleted article with indicator in title
        let deleted_html = "<html><head><title>삭제된 기사입니다</title></head><body></body></html>";
        assert!(parser.is_deleted_article(deleted_html));

        // Deleted article with error container
        let deleted_html2 = r#"<html><body><div class="error_content">존재하지 않는 기사입니다</div></body></html>"#;
        assert!(parser.is_deleted_article(deleted_html2));

        // Normal article with content area
        let normal_html = r#"<html><body><div id="dic_area">정상 기사 내용</div></body></html>"#;
        assert!(!parser.is_deleted_article(normal_html));

        // Article that discusses deletion but has content (false positive fix)
        let article_about_deletion = r#"<html>
            <head><title>SBS 기사 삭제 논란</title></head>
            <body>
                <div id="dic_area">
                    현대차 요청으로 삭제된 기사가 논란이 되고 있다.
                    삭제된 기사는 음주운전 관련 내용이었다.
                </div>
            </body>
        </html>"#;
        assert!(!parser.is_deleted_article(article_about_deletion));
    }

    #[test]
    fn test_parse_date_formats() {
        let parser = ArticleParser::new();

        let test_cases = vec!["2024.12.15. 14:30", "2024.12.15 14:30", "2024-12-15 14:30"];

        for date_str in test_cases {
            let result = parser.parse_date(date_str);
            assert!(result.is_some(), "Failed to parse: {date_str}");
        }
    }

    #[test]
    fn test_parse_general_article() {
        let html = r#"
            <html>
            <body>
                <div id="title_area"><span>테스트 기사 제목</span></div>
                <div id="dic_area">테스트 기사 본문입니다.</div>
                <span class="media_end_head_info_datestamp_time">2024.12.15. 14:30</span>
            </body>
            </html>
        "#;

        let parser = ArticleParser::new();
        let result = parser.parse_with_fallback(
            html,
            "https://n.news.naver.com/mnews/article/001/0014123456",
        );

        assert!(result.is_ok());
        let article = result.unwrap();
        assert!(article.title.contains("테스트"));
        assert!(article.content.contains("본문"));
    }

    #[test]
    fn test_parse_deleted_article() {
        let html = "<html><body>삭제된 기사입니다</body></html>";
        let parser = ArticleParser::new();

        let result = parser.parse_with_fallback(
            html,
            "https://n.news.naver.com/mnews/article/001/0014123456",
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::ArticleNotFound));
    }

    #[test]
    fn test_extract_first_match() {
        let parser = ArticleParser::new();
        let html = r#"<div id="title_area"><span>Title Text</span></div>"#;
        let doc = Html::parse_document(html);

        let result = parser.extract_first_match(&doc, &parser.general.title);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Title"));
    }

    #[test]
    fn test_extract_publisher_from_alt() {
        let parser = ArticleParser::new();
        let html = r#"<div class="media_end_head_top_logo"><img alt="연합뉴스"/></div>"#;
        let doc = Html::parse_document(html);

        let result = parser.extract_publisher(&doc, &parser.general.publisher);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "연합뉴스");
    }

    #[test]
    fn test_remove_noise_from_html() {
        let parser = ArticleParser::new();
        let html = r#"<div>Content<script>alert('test');</script>More</div>"#;
        let clean = parser.remove_noise_from_html(html);
        assert!(!clean.contains("script"));
        assert!(clean.contains("Content"));
    }

    #[test]
    fn test_parse_date_korean_format() {
        let parser = ArticleParser::new();
        let date_str = "2024년 12월 15일 14:30";
        let result = parser.parse_date(date_str);
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_date_date_only() {
        let parser = ArticleParser::new();
        let date_str = "2024.12.15.";
        let result = parser.parse_date(date_str);
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_date_invalid() {
        let parser = ArticleParser::new();
        let date_str = "Invalid Date";
        let result = parser.parse_date(date_str);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_captions() {
        let parser = ArticleParser::new();
        let html = r#"
            <div>
                <em class="img_desc">Caption 1</em>
                <em class="img_desc">Caption 2</em>
            </div>
        "#;
        let doc = Html::parse_document(html);
        let result = parser.extract_captions(&doc);
        assert!(result.is_some());
        let captions = result.unwrap();
        assert!(captions.contains("Caption 1"));
        assert!(captions.contains("Caption 2"));
    }

    #[test]
    fn test_extract_captions_empty() {
        let parser = ArticleParser::new();
        let html = r#"<div>No captions</div>"#;
        let doc = Html::parse_document(html);
        let result = parser.extract_captions(&doc);
        assert!(result.is_none());
    }

    #[test]
    fn test_fallback_chain_all_fail() {
        let parser = ArticleParser::new();
        let html = r#"<html><body>No valid content</body></html>"#;
        let doc = Html::parse_document(html);

        let result = parser.try_fallback_chain(&doc, "http://test.com", "001", "123");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::UnknownFormat));
    }

    #[test]
    fn test_parser_default() {
        let parser = ArticleParser::default();
        assert!(!parser.general.title.is_empty());
    }
}
