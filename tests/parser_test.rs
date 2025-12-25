//! Parser integration tests using HTML fixture files
//!
//! Day 4 implementation: Article parser tests for all formats
//! - General news (n.news.naver.com)
//! - Entertainment news (entertain.naver.com)
//! - Sports news (sports.naver.com)
//! - Card/Photo news

use baram::parser::{detect_format, ArticleFormat, ArticleParser};
use std::fs;

/// Test fixture paths
const FIXTURES_DIR: &str = "tests/fixtures/html";

fn load_fixture(filename: &str) -> String {
    let path = format!("{FIXTURES_DIR}/{filename}");
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to load fixture: {path}"))
}

// ============================================================================
// Format Detection Tests
// ============================================================================

#[test]
fn test_detect_format_general_news() {
    let html = load_fixture("general_news.html");
    let format = detect_format(&html);
    assert_eq!(format, ArticleFormat::General);
}

#[test]
fn test_detect_format_entertainment_news() {
    let html = load_fixture("entertainment_news.html");
    let format = detect_format(&html);
    // Entertainment uses end_body_wrp which maps to Entertainment format
    assert!(
        matches!(
            format,
            ArticleFormat::Entertainment | ArticleFormat::General
        ),
        "Expected Entertainment or General, got {format:?}"
    );
}

#[test]
fn test_detect_format_sports_news() {
    let html = load_fixture("sports_news.html");
    let format = detect_format(&html);
    assert_eq!(format, ArticleFormat::Sports);
}

#[test]
fn test_detect_format_card_news() {
    let html = load_fixture("card_news.html");
    let format = detect_format(&html);
    assert_eq!(format, ArticleFormat::Card);
}

// ============================================================================
// General News Parser Tests
// ============================================================================

#[test]
fn test_parse_general_news_title() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000001";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok(), "Failed to parse general news");

    let article = result.unwrap();
    assert!(article.title.contains("정치 뉴스"));
    assert!(article.title.contains("테스트 기사"));
}

#[test]
fn test_parse_general_news_content() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000001";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.content.contains("테스트 기사의 본문"));
    assert!(article.content.contains("정치 뉴스"));
    assert!(article.content.contains("네이버 뉴스 크롤러"));
}

#[test]
fn test_parse_general_news_date() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000001";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.published_at.is_some());

    let date = article.published_at.unwrap();
    assert_eq!(date.format("%Y-%m-%d").to_string(), "2024-12-15");
}

#[test]
fn test_parse_general_news_ids() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000001";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert_eq!(article.oid, "001");
    assert_eq!(article.aid, "0014000001");
}

#[test]
fn test_parse_general_news_hash() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000001";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.content_hash.is_some());
    let hash = article.content_hash.as_ref().unwrap();
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64); // SHA-256 hex
}

// ============================================================================
// Entertainment News Parser Tests
// ============================================================================

#[test]
fn test_parse_entertainment_news_title() {
    let html = load_fixture("entertainment_news.html");
    let parser = ArticleParser::new();
    let url = "https://entertain.naver.com/read?oid=001&aid=0014000002";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok(), "Failed to parse entertainment news");

    let article = result.unwrap();
    assert!(article.title.contains("연예 뉴스"));
}

#[test]
fn test_parse_entertainment_news_content() {
    let html = load_fixture("entertainment_news.html");
    let parser = ArticleParser::new();
    let url = "https://entertain.naver.com/read?oid=001&aid=0014000002";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.content.contains("연예 뉴스 테스트 본문"));
    assert!(article.content.contains("엔터테인먼트"));
}

#[test]
fn test_parse_entertainment_news_category() {
    let html = load_fixture("entertainment_news.html");
    let parser = ArticleParser::new();
    let url = "https://entertain.naver.com/read?oid=001&aid=0014000002";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert_eq!(article.category, "entertainment");
}

// ============================================================================
// Sports News Parser Tests
// ============================================================================

#[test]
fn test_parse_sports_news_title() {
    let html = load_fixture("sports_news.html");
    let parser = ArticleParser::new();
    let url = "https://sports.naver.com/news/read?oid=001&aid=0014000003";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok(), "Failed to parse sports news");

    let article = result.unwrap();
    assert!(article.title.contains("스포츠 뉴스"));
}

#[test]
fn test_parse_sports_news_content() {
    let html = load_fixture("sports_news.html");
    let parser = ArticleParser::new();
    let url = "https://sports.naver.com/news/read?oid=001&aid=0014000003";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.content.contains("스포츠 뉴스 테스트 본문"));
    assert!(article.content.contains("축구 경기"));
}

#[test]
fn test_parse_sports_news_category() {
    let html = load_fixture("sports_news.html");
    let parser = ArticleParser::new();
    let url = "https://sports.naver.com/news/read?oid=001&aid=0014000003";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert_eq!(article.category, "sports");
}

// ============================================================================
// Card News Parser Tests
// ============================================================================

#[test]
fn test_parse_card_news_title() {
    let html = load_fixture("card_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000004";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok(), "Failed to parse card news");

    let article = result.unwrap();
    assert!(article.title.contains("카드 뉴스"));
}

#[test]
fn test_parse_card_news_captions() {
    let html = load_fixture("card_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000004";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    // Card news should extract captions
    assert!(article.content.contains("카드 설명"));
}

#[test]
fn test_parse_card_news_category() {
    let html = load_fixture("card_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000004";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert_eq!(article.category, "card");
}

// ============================================================================
// Deleted Article Tests
// ============================================================================

#[test]
fn test_parse_deleted_article() {
    let html = load_fixture("deleted_article.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000005";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_err());
}

#[test]
fn test_parse_deleted_article_detection() {
    let deleted_patterns = [
        "<body>삭제된 기사입니다</body>",
        "<body>이 기사는 삭제되었거나 존재하지 않습니다</body>",
        "<body>페이지를 찾을 수 없습니다</body>",
        "<body>서비스 되지 않는 기사입니다</body>",
    ];

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000005";

    for pattern in &deleted_patterns {
        let result = parser.parse_with_fallback(pattern, url);
        assert!(result.is_err(), "Should detect deleted article: {pattern}");
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_parse_special_characters_in_title() {
    let html = r#"
        <html>
        <body>
            <div id="title_area"><span>특수문자 테스트: "따옴표" &amp; &lt;꺾쇠&gt; '작은따옴표'</span></div>
            <div id="dic_area">본문 내용입니다.</div>
        </body>
        </html>
    "#;

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000006";

    let result = parser.parse_with_fallback(html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.title.contains("특수문자"));
}

#[test]
fn test_parse_empty_content_fallback() {
    // HTML with title but no content in main area
    let html = r#"
        <html>
        <body>
            <div id="title_area"><span>제목만 있는 기사</span></div>
            <div id="dic_area"></div>
        </body>
        </html>
    "#;

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000007";

    let result = parser.parse_with_fallback(html, url);
    // Should fail because content is empty
    assert!(result.is_err());
}

#[test]
fn test_parse_unicode_content() {
    let html = r#"
        <html>
        <body>
            <div id="title_area"><span>유니코드 테스트 🎉🎊</span></div>
            <div id="dic_area">이모지와 특수문자: 한글 English 中文 日本語</div>
        </body>
        </html>
    "#;

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000008";

    let result = parser.parse_with_fallback(html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.content.contains("한글"));
    assert!(article.content.contains("English"));
}

#[test]
fn test_parse_whitespace_normalization() {
    let html = r#"
        <html>
        <body>
            <div id="title_area"><span>   공백  테스트   </span></div>
            <div id="dic_area">
                줄바꿈

                여러 개


                있는 본문
            </div>
        </body>
        </html>
    "#;

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000009";

    let result = parser.parse_with_fallback(html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    // Whitespace should be normalized
    assert!(!article.title.starts_with(' '));
    assert!(!article.title.ends_with(' '));
}

#[test]
fn test_parse_noise_removal() {
    let html = r#"
        <html>
        <body>
            <div id="title_area"><span>노이즈 제거 테스트</span></div>
            <div id="dic_area">
                본문 내용입니다.
                <script>alert('test');</script>
                <style>.hidden{display:none;}</style>
                더 많은 본문 내용.
            </div>
        </body>
        </html>
    "#;

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000010";

    let result = parser.parse_with_fallback(html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(!article.content.contains("alert"));
    assert!(!article.content.contains("display:none"));
    assert!(article.content.contains("본문 내용"));
}

// ============================================================================
// Fallback Chain Tests
// ============================================================================

#[test]
fn test_fallback_to_entertainment() {
    // HTML that has entertainment structure but not general
    let html = r#"
        <html>
        <body>
            <h2 class="end_tit">연예 포맷 테스트</h2>
            <div class="article_body">
                <p>연예 형식의 본문입니다.</p>
            </div>
        </body>
        </html>
    "#;

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000011";

    let result = parser.parse_with_fallback(html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.title.contains("연예 포맷"));
}

#[test]
fn test_fallback_to_sports() {
    // HTML that has sports structure only
    let html = r#"
        <html>
        <body>
            <div class="news_headline">
                <h4 class="title">스포츠 포맷 테스트</h4>
            </div>
            <div class="news_end">
                <p>스포츠 형식의 본문입니다.</p>
            </div>
        </body>
        </html>
    "#;

    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000012";

    let result = parser.parse_with_fallback(html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert!(article.title.contains("스포츠 포맷"));
}

// ============================================================================
// URL Extraction Tests
// ============================================================================

#[test]
fn test_parse_old_format_url() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://news.naver.com/main/read.naver?oid=001&aid=0014000013";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert_eq!(article.oid, "001");
    assert_eq!(article.aid, "0014000013");
}

#[test]
fn test_parse_mobile_format_url() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://m.news.naver.com/mnews/article/001/0014000014";

    let result = parser.parse_with_fallback(&html, url);
    assert!(result.is_ok());

    let article = result.unwrap();
    assert_eq!(article.oid, "001");
    assert_eq!(article.aid, "0014000014");
}

// ============================================================================
// Multiple Format Parsing Tests
// ============================================================================

#[test]
fn test_parse_all_fixture_formats() {
    let fixtures = [
        (
            "general_news.html",
            "https://n.news.naver.com/mnews/article/001/0014000001",
        ),
        (
            "entertainment_news.html",
            "https://entertain.naver.com/read?oid=001&aid=0014000002",
        ),
        (
            "sports_news.html",
            "https://sports.naver.com/news/read?oid=001&aid=0014000003",
        ),
        (
            "card_news.html",
            "https://n.news.naver.com/mnews/article/001/0014000004",
        ),
    ];

    let parser = ArticleParser::new();

    for (fixture, url) in &fixtures {
        let html = load_fixture(fixture);
        let result = parser.parse_with_fallback(&html, url);
        assert!(
            result.is_ok(),
            "Failed to parse {fixture}: {:?}",
            result.err()
        );

        let article = result.unwrap();
        assert!(!article.title.is_empty(), "Empty title for {fixture}");
        assert!(!article.content.is_empty(), "Empty content for {fixture}");
        assert!(!article.oid.is_empty(), "Empty oid for {fixture}");
        assert!(!article.aid.is_empty(), "Empty aid for {fixture}");
    }
}

// ============================================================================
// Content Hash Consistency Tests
// ============================================================================

#[test]
fn test_content_hash_consistency() {
    let html = load_fixture("general_news.html");
    let parser = ArticleParser::new();
    let url = "https://n.news.naver.com/mnews/article/001/0014000001";

    let result1 = parser.parse_with_fallback(&html, url);
    let result2 = parser.parse_with_fallback(&html, url);

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let article1 = result1.unwrap();
    let article2 = result2.unwrap();

    assert_eq!(article1.content_hash, article2.content_hash);
}

#[test]
fn test_different_content_different_hash() {
    let parser = ArticleParser::new();

    let html1 = r#"
        <html><body>
            <div id="title_area"><span>제목 1</span></div>
            <div id="dic_area">본문 내용 1</div>
        </body></html>
    "#;

    let html2 = r#"
        <html><body>
            <div id="title_area"><span>제목 2</span></div>
            <div id="dic_area">본문 내용 2</div>
        </body></html>
    "#;

    let url = "https://n.news.naver.com/mnews/article/001/0014000015";

    let result1 = parser.parse_with_fallback(html1, url);
    let result2 = parser.parse_with_fallback(html2, url);

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let article1 = result1.unwrap();
    let article2 = result2.unwrap();

    assert_ne!(article1.content_hash, article2.content_hash);
}

#[test]
fn test_parse_mobile_sports_article() {
    use baram::parser::html::{detect_format, ArticleParser};
    use baram::parser::selectors::ArticleFormat;

    // Mobile sports article HTML structure (m.sports.naver.com)
    let html = r#"
    <!DOCTYPE html>
    <html lang="ko">
    <head>
        <title>테스트 스포츠 기사</title>
    </head>
    <body>
        <h2 class="ArticleHead_article_title__qh8GV">3년 간의 재개발 마친 '조선협객전 클래식' 사전예약 실시</h2>
        <div class="DateInfo_info_item__3yQPs">
            <em class="date">2025.12.23. 오후 6:31</em>
        </div>
        <article class="Article_comp_news_article__XIpve" id="comp_news_article">
            <div class="_article_content">
                스마트나우는 '조선협객전 클래식'의 정식 출시에 앞서 사전예약을 시작한다.
                약 3년 동안 재개발을 통해 그래픽, 전투, 시스템, 운영 전반을 새롭게 재정립하며 
                '지금 다시 즐길 수 있는 클래식'을 목표로 하고 있다.
            </div>
        </article>
        <em class="JournalistCard_press_name__s3Eup">포모스</em>
        <em class="JournalistCard_name__0ZSAO">최종봉 기자</em>
    </body>
    </html>
    "#;

    // Test format detection
    let format = detect_format(html);
    assert_eq!(
        format,
        ArticleFormat::Sports,
        "Should detect mobile sports format"
    );

    // Test parsing
    let parser = ArticleParser::new();
    let result = parser.parse_with_fallback(
        html,
        "https://n.news.naver.com/mnews/article/236/0000252917",
    );

    assert!(
        result.is_ok(),
        "Should successfully parse mobile sports article"
    );

    let article = result.unwrap();
    assert!(
        article.title.contains("조선협객전"),
        "Title should contain game name"
    );
    assert_eq!(article.category, "sports", "Category should be sports");
    assert!(
        article.content.contains("스마트나우"),
        "Content should contain publisher name"
    );
    assert_eq!(article.oid, "236", "OID should be extracted");
    assert_eq!(article.aid, "0000252917", "AID should be extracted");
}
