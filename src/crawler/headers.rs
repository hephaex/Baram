use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, REFERER,
    USER_AGENT,
};

/// Build anti-bot headers for Naver News
///
/// Creates a HeaderMap with browser-like headers to avoid being blocked by Naver's anti-bot systems.
///
/// # Arguments
///
/// * `user_agent` - User agent string (typically a modern browser UA)
/// * `referer` - Referer URL (typically Naver News main page or section page)
///
/// # Examples
///
/// ```
/// use baram::crawler::headers::build_naver_headers;
///
/// let headers = build_naver_headers(
///     "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
///     "https://news.naver.com"
/// );
/// ```
pub fn build_naver_headers(user_agent: &str, referer: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert(USER_AGENT, HeaderValue::from_str(user_agent).unwrap());
    headers.insert(REFERER, HeaderValue::from_str(referer).unwrap());
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("ko-KR,ko;q=0.9,en-US;q=0.8,en;q=0.7"),
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br"),
    );

    // Sec-Fetch headers for modern browser compatibility
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("document"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("navigate"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-user"),
        HeaderValue::from_static("?1"),
    );
    headers.insert(
        HeaderName::from_static("upgrade-insecure-requests"),
        HeaderValue::from_static("1"),
    );

    headers
}

/// Generate referer URL for given Naver news section ID
///
/// # Arguments
///
/// * `section_id` - News section ID:
///   - 100: Politics
///   - 101: Economy
///   - 102: Society
///   - 103: Culture
///   - 104: World
///   - 105: IT/Science
///
/// # Examples
///
/// ```
/// use baram::crawler::headers::section_referer;
///
/// let referer = section_referer(100);
/// assert_eq!(referer, "https://news.naver.com/main/main.naver?mode=LSD&mid=shm&sid1=100");
/// ```
pub fn section_referer(section_id: u32) -> String {
    format!("https://news.naver.com/main/main.naver?mode=LSD&mid=shm&sid1={section_id}")
}

/// Build headers for comment API (JSONP)
///
/// Creates headers suitable for making AJAX requests to Naver's comment API.
///
/// # Arguments
///
/// * `article_url` - The article URL to use as referer
///
/// # Examples
///
/// ```
/// use baram::crawler::headers::build_comment_headers;
///
/// let headers = build_comment_headers("https://news.naver.com/article/123/456");
/// ```
pub fn build_comment_headers(article_url: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert(
        HeaderName::from_static("x-requested-with"),
        HeaderValue::from_static("XMLHttpRequest"),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/javascript, */*; q=0.01"),
    );
    headers.insert(REFERER, HeaderValue::from_str(article_url).unwrap());
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("ko-KR,ko;q=0.9,en-US;q=0.8,en;q=0.7"),
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br"),
    );

    // Sec-Fetch headers for AJAX requests
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("empty"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("cors"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );

    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_naver_headers() {
        let headers = build_naver_headers("Mozilla/5.0", "https://news.naver.com");

        assert!(headers.contains_key(USER_AGENT));
        assert!(headers.contains_key(REFERER));
        assert!(headers.contains_key(ACCEPT));
        assert!(headers.contains_key(ACCEPT_LANGUAGE));
        assert!(headers.contains_key(ACCEPT_ENCODING));

        assert_eq!(
            headers.get(USER_AGENT).unwrap(),
            HeaderValue::from_static("Mozilla/5.0")
        );
        assert_eq!(
            headers.get(REFERER).unwrap(),
            HeaderValue::from_static("https://news.naver.com")
        );
        assert_eq!(
            headers.get(ACCEPT_LANGUAGE).unwrap(),
            HeaderValue::from_static("ko-KR,ko;q=0.9,en-US;q=0.8,en;q=0.7")
        );

        // Check Sec-Fetch headers
        assert!(headers.contains_key("sec-fetch-dest"));
        assert!(headers.contains_key("sec-fetch-mode"));
        assert!(headers.contains_key("sec-fetch-site"));
        assert!(headers.contains_key("sec-fetch-user"));
        assert!(headers.contains_key("upgrade-insecure-requests"));
    }

    #[test]
    fn test_section_referer() {
        assert_eq!(
            section_referer(100),
            "https://news.naver.com/main/main.naver?mode=LSD&mid=shm&sid1=100"
        );
    }

    #[test]
    fn test_all_section_referers() {
        // Test all 6 sections: 100-105
        let sections = [
            (100, "Politics"),
            (101, "Economy"),
            (102, "Society"),
            (103, "Culture"),
            (104, "World"),
            (105, "IT/Science"),
        ];

        for (section_id, _name) in sections {
            let referer = section_referer(section_id);
            assert!(referer.starts_with("https://news.naver.com/main/main.naver?"));
            assert!(referer.contains(&format!("sid1={section_id}")));
            assert!(referer.contains("mode=LSD"));
            assert!(referer.contains("mid=shm"));
        }
    }

    #[test]
    fn test_build_comment_headers() {
        let article_url = "https://news.naver.com/article/123/456";
        let headers = build_comment_headers(article_url);

        assert!(headers.contains_key("x-requested-with"));
        assert!(headers.contains_key(ACCEPT));
        assert!(headers.contains_key(REFERER));

        assert_eq!(
            headers.get("x-requested-with").unwrap(),
            HeaderValue::from_static("XMLHttpRequest")
        );
        assert_eq!(
            headers.get(ACCEPT).unwrap(),
            HeaderValue::from_static("application/json, text/javascript, */*; q=0.01")
        );
        assert_eq!(
            headers.get(REFERER).unwrap(),
            HeaderValue::from_str(article_url).unwrap()
        );

        // Check Sec-Fetch headers for AJAX
        assert_eq!(
            headers.get("sec-fetch-dest").unwrap(),
            HeaderValue::from_static("empty")
        );
        assert_eq!(
            headers.get("sec-fetch-mode").unwrap(),
            HeaderValue::from_static("cors")
        );
        assert_eq!(
            headers.get("sec-fetch-site").unwrap(),
            HeaderValue::from_static("same-origin")
        );
    }

    #[test]
    fn test_comment_headers_with_special_characters() {
        let article_url = "https://news.naver.com/article/test/123?query=value&other=data";
        let headers = build_comment_headers(article_url);

        assert_eq!(
            headers.get(REFERER).unwrap(),
            HeaderValue::from_str(article_url).unwrap()
        );
    }
}
