//! Test fixtures for integration tests
//!
//! Provides sample HTML data and helper functions for testing

/// Sample HTML article content for testing
pub const SAMPLE_ARTICLE_HTML: &str = r#"
<!DOCTYPE html>
<html lang="ko">
<head>
    <meta charset="UTF-8">
    <title>테스트 뉴스 기사</title>
</head>
<body>
    <div id="dic_area">
        <h2 id="title_area">
            <span class="media_end_head_headline">네이버 차세대 옴니모달 AI 공개, 수능 전과목 1등급</span>
        </h2>
        <div class="go_trans _article_content">
            <p>네이버가 새로운 인공지능 모델을 공개했습니다.</p>
            <p>이번에 공개된 AI는 수능 전 과목에서 1등급을 받을 수 있는 성능을 보였습니다.</p>
            <p>전문가들은 이것이 한국 AI 산업의 중요한 이정표라고 평가하고 있습니다.</p>
        </div>
        <div class="article_info">
            <em class="media_end_categorize_info">
                <a href="/article/001/">언론사</a> |
                <span class="media_end_categorize_date">2024-01-15 14:30</span>
            </em>
        </div>
    </div>
</body>
</html>
"#;

/// Sample article HTML with different structure
pub const SAMPLE_ARTICLE_HTML_ALT: &str = r#"
<!DOCTYPE html>
<html lang="ko">
<head>
    <meta charset="UTF-8">
    <title>경제 뉴스</title>
</head>
<body>
    <div id="dic_area">
        <h2 id="title_area">
            <span>반도체 국가산단 조성 가속</span>
        </h2>
        <div class="_article_content">
            <p>용인 반도체 국가산단 조성이 가속화되고 있습니다.</p>
            <p>LH와 삼성전자가 부지 매입 계약을 체결했습니다.</p>
        </div>
    </div>
</body>
</html>
"#;

/// Sample comment JSON response
pub const SAMPLE_COMMENTS_JSON: &str = r#"{
    "success": true,
    "result": {
        "commentList": [
            {
                "contents": "좋은 기사입니다",
                "userName": "user1",
                "modTime": "2024-01-15 15:00:00",
                "sympathyCount": 10,
                "antipathyCount": 0
            },
            {
                "contents": "유익한 정보 감사합니다",
                "userName": "user2",
                "modTime": "2024-01-15 15:30:00",
                "sympathyCount": 5,
                "antipathyCount": 1
            }
        ]
    }
}"#;

/// Sample error HTML (404 page)
pub const ERROR_404_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>404 Not Found</title></head>
<body>
    <h1>페이지를 찾을 수 없습니다</h1>
</body>
</html>
"#;

/// Sample malformed HTML
pub const MALFORMED_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Broken</title>
<body>
    <div id="content">
        <p>This is missing closing tags
    </div>
"#;

/// Create a sample Naver News article URL
pub fn sample_article_url(article_id: &str) -> String {
    format!(
        "https://n.news.naver.com/mnews/article/001/{article_id}?sid=105"
    )
}

/// Create a sample comment API URL
pub fn sample_comment_url(oid: &str, aid: &str) -> String {
    format!(
        "https://apis.naver.com/commentBox/cbox/web_naver_list_jsonp.json?ticket=news&templateId=default_society&pool=cbox5&lang=ko&country=KR&objectId=news{oid}%2C{aid}&pageSize=20&indexSize=10&groupId=&listType=OBJECT&pageType=more&page=1&refresh=true&sort=FAVORITE"
    )
}

/// Create expected parsed article content
pub fn expected_article_title() -> &'static str {
    "네이버 차세대 옴니모달 AI 공개, 수능 전과목 1등급"
}

/// Create expected parsed article content
pub fn expected_article_content() -> String {
    vec![
        "네이버가 새로운 인공지능 모델을 공개했습니다.",
        "이번에 공개된 AI는 수능 전 과목에서 1등급을 받을 수 있는 성능을 보였습니다.",
        "전문가들은 이것이 한국 AI 산업의 중요한 이정표라고 평가하고 있습니다.",
    ]
    .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_fixtures_not_empty() {
        assert!(!SAMPLE_ARTICLE_HTML.is_empty());
        assert!(!SAMPLE_ARTICLE_HTML_ALT.is_empty());
        assert!(!SAMPLE_COMMENTS_JSON.is_empty());
        assert!(!ERROR_404_HTML.is_empty());
    }

    #[test]
    fn test_url_generation() {
        let url = sample_article_url("0014000001");
        assert!(url.contains("article/001/0014000001"));
        assert!(url.contains("sid=105"));
    }

    #[test]
    fn test_comment_url_generation() {
        let url = sample_comment_url("001", "0014000001");
        assert!(url.contains("objectId=news001"));
        assert!(url.contains("0014000001"));
    }
}
