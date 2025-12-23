//! CSS selectors for different Naver News article formats
//!
//! This module provides specialized selectors for parsing various types of
//! Naver News articles including general news, entertainment, sports, and card news.

use scraper::Selector;

/// Selectors for general news format (n.news.naver.com)
pub struct GeneralSelectors {
    pub title: Vec<Selector>,
    pub content: Vec<Selector>,
    pub date: Vec<Selector>,
    pub publisher: Vec<Selector>,
    pub author: Vec<Selector>,
}

impl GeneralSelectors {
    pub fn new() -> Self {
        Self {
            title: vec![
                Selector::parse("#title_area span").unwrap(),
                Selector::parse(".media_end_head_title").unwrap(),
                Selector::parse("h2.media_end_head_headline").unwrap(),
            ],
            content: vec![
                Selector::parse("#dic_area").unwrap(),
                Selector::parse("#articleBodyContents").unwrap(),
                Selector::parse("article#dic_area").unwrap(),
            ],
            date: vec![
                Selector::parse(".media_end_head_info_datestamp_time").unwrap(),
                Selector::parse("._ARTICLE_DATE_TIME").unwrap(),
                Selector::parse("span.media_end_head_info_datestamp_time").unwrap(),
            ],
            publisher: vec![
                Selector::parse(".media_end_head_top_logo img").unwrap(),
                Selector::parse(".press_logo img").unwrap(),
                Selector::parse("a.media_end_head_top_logo_img img").unwrap(),
            ],
            author: vec![
                Selector::parse(".byline").unwrap(),
                Selector::parse(".journalist_name").unwrap(),
                Selector::parse("span.byline_s").unwrap(),
            ],
        }
    }
}

impl Default for GeneralSelectors {
    fn default() -> Self {
        Self::new()
    }
}

/// Selectors for entertainment news format (entertain.naver.com)
pub struct EntertainmentSelectors {
    pub title: Vec<Selector>,
    pub content: Vec<Selector>,
    pub date: Vec<Selector>,
}

impl EntertainmentSelectors {
    pub fn new() -> Self {
        Self {
            title: vec![
                Selector::parse(".end_tit").unwrap(),
                Selector::parse("h2.end_tit").unwrap(),
                Selector::parse(".article_tit").unwrap(),
            ],
            content: vec![
                Selector::parse(".article_body").unwrap(),
                Selector::parse("#articeBody").unwrap(),
                Selector::parse("div.end_body_wrp").unwrap(),
            ],
            date: vec![
                Selector::parse(".article_info .author em").unwrap(),
                Selector::parse(".info_date").unwrap(),
                Selector::parse("span.author em").unwrap(),
            ],
        }
    }
}

impl Default for EntertainmentSelectors {
    fn default() -> Self {
        Self::new()
    }
}

/// Selectors for sports news format (sports.naver.com, m.sports.naver.com)
pub struct SportsSelectors {
    pub title: Vec<Selector>,
    pub content: Vec<Selector>,
    pub date: Vec<Selector>,
    pub publisher: Vec<Selector>,
    pub author: Vec<Selector>,
}

impl SportsSelectors {
    pub fn new() -> Self {
        Self {
            title: vec![
                // Desktop sports.naver.com
                Selector::parse(".news_headline .title").unwrap(),
                Selector::parse("h4.title").unwrap(),
                Selector::parse(".NewsEndMain_article_title__j5ND9").unwrap(),
                // Mobile m.sports.naver.com (esports/game)
                Selector::parse("h2.ArticleHead_article_title__qh8GV").unwrap(),
                Selector::parse(".ArticleHead_article_title__qh8GV").unwrap(),
                Selector::parse("h2[class*='article_title']").unwrap(),
            ],
            content: vec![
                // Desktop sports.naver.com
                Selector::parse(".news_end").unwrap(),
                Selector::parse("#newsEndContents").unwrap(),
                Selector::parse("div.NewsEndMain_article_body__D5MUB").unwrap(),
                // Mobile m.sports.naver.com (esports/game)
                Selector::parse("article.Article_comp_news_article__XIpve").unwrap(),
                Selector::parse("article[class*='_article_body']").unwrap(),
                Selector::parse("div._article_content").unwrap(),
                Selector::parse("article#comp_news_article").unwrap(),
            ],
            date: vec![
                // Desktop sports.naver.com
                Selector::parse(".info span").unwrap(),
                Selector::parse(".news_date").unwrap(),
                Selector::parse("em.date").unwrap(),
                // Mobile m.sports.naver.com
                Selector::parse(".DateInfo_info_item__3yQPs em.date").unwrap(),
                Selector::parse(".DateInfo_article_head_date_info__CS6Gx em.date").unwrap(),
                Selector::parse("div[class*='DateInfo'] em.date").unwrap(),
            ],
            publisher: vec![
                Selector::parse(".JournalistCard_press_name__s3Eup").unwrap(),
                Selector::parse("em[class*='press_name']").unwrap(),
                Selector::parse(".press_name").unwrap(),
            ],
            author: vec![
                Selector::parse(".JournalistCard_name__0ZSAO").unwrap(),
                Selector::parse("em[class*='name']").unwrap(),
                Selector::parse(".journalist_name").unwrap(),
            ],
        }
    }
}

impl Default for SportsSelectors {
    fn default() -> Self {
        Self::new()
    }
}

/// Selectors for card/photo news format
pub struct CardNewsSelectors {
    pub title: Vec<Selector>,
    pub content: Vec<Selector>,
    pub captions: Vec<Selector>,
}

impl CardNewsSelectors {
    pub fn new() -> Self {
        Self {
            title: vec![
                Selector::parse("h2.end_tit").unwrap(),
                Selector::parse(".media_end_head_title").unwrap(),
                Selector::parse("h3.tit_view").unwrap(),
            ],
            content: vec![
                Selector::parse("div.end_ct_area").unwrap(),
                Selector::parse("div.card_area").unwrap(),
                Selector::parse("div.content_area").unwrap(),
            ],
            captions: vec![
                Selector::parse("em.img_desc").unwrap(),
                Selector::parse(".txt").unwrap(),
                Selector::parse("figcaption").unwrap(),
            ],
        }
    }
}

impl Default for CardNewsSelectors {
    fn default() -> Self {
        Self::new()
    }
}

/// Selectors for noise elements to remove during parsing
pub struct NoiseSelectors {
    pub elements: Vec<Selector>,
}

impl NoiseSelectors {
    pub fn new() -> Self {
        let selectors = vec![
            "em.img_desc",      // Image captions
            "div.link_news",    // Related article links
            ".end_photo_org",   // Photo area
            ".vod_player_wrap", // Video player
            "script",
            "style",
            "noscript",
            "iframe",
            ".ad_wrap",       // Advertisements
            ".reporter_area", // Reporter info
            ".byline_wrap",   // Byline area
            ".copyright",     // Copyright notice
            ".source",        // Source attribution
        ];

        Self {
            elements: selectors
                .iter()
                .filter_map(|s| Selector::parse(s).ok())
                .collect(),
        }
    }
}

impl Default for NoiseSelectors {
    fn default() -> Self {
        Self::new()
    }
}

/// Article format types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArticleFormat {
    General,
    Entertainment,
    Sports,
    Card,
    Unknown,
}

impl std::fmt::Display for ArticleFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArticleFormat::General => write!(f, "General"),
            ArticleFormat::Entertainment => write!(f, "Entertainment"),
            ArticleFormat::Sports => write!(f, "Sports"),
            ArticleFormat::Card => write!(f, "Card"),
            ArticleFormat::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_general_selectors_creation() {
        let selectors = GeneralSelectors::new();
        assert!(!selectors.title.is_empty());
        assert!(!selectors.content.is_empty());
        assert!(!selectors.date.is_empty());
        assert!(!selectors.publisher.is_empty());
        assert!(!selectors.author.is_empty());
    }

    #[test]
    fn test_general_selectors_default() {
        let selectors = GeneralSelectors::default();
        assert_eq!(selectors.title.len(), 3);
        assert_eq!(selectors.content.len(), 3);
    }

    #[test]
    fn test_entertainment_selectors_creation() {
        let selectors = EntertainmentSelectors::new();
        assert!(!selectors.title.is_empty());
        assert!(!selectors.content.is_empty());
        assert!(!selectors.date.is_empty());
    }

    #[test]
    fn test_entertainment_selectors_default() {
        let selectors = EntertainmentSelectors::default();
        assert_eq!(selectors.title.len(), 3);
    }

    #[test]
    fn test_sports_selectors_creation() {
        let selectors = SportsSelectors::new();
        assert!(!selectors.content.is_empty());
        assert!(!selectors.title.is_empty());
        assert!(!selectors.date.is_empty());
        assert!(!selectors.publisher.is_empty());
        assert!(!selectors.author.is_empty());
    }

    #[test]
    fn test_sports_selectors_default() {
        let selectors = SportsSelectors::default();
        // Desktop (3) + Mobile (4) = 7 content selectors
        assert!(selectors.content.len() >= 6);
        // Desktop (3) + Mobile (3) = 6 title selectors
        assert!(selectors.title.len() >= 6);
    }

    #[test]
    fn test_card_news_selectors_creation() {
        let selectors = CardNewsSelectors::new();
        assert!(!selectors.title.is_empty());
        assert!(!selectors.content.is_empty());
        assert!(!selectors.captions.is_empty());
    }

    #[test]
    fn test_card_news_selectors_default() {
        let selectors = CardNewsSelectors::default();
        assert_eq!(selectors.title.len(), 3);
    }

    #[test]
    fn test_noise_selectors() {
        let noise = NoiseSelectors::new();
        assert!(!noise.elements.is_empty());
        assert!(noise.elements.len() >= 13);
    }

    #[test]
    fn test_noise_selectors_default() {
        let noise = NoiseSelectors::default();
        assert!(!noise.elements.is_empty());
    }

    #[test]
    fn test_article_format_display() {
        assert_eq!(format!("{}", ArticleFormat::General), "General");
        assert_eq!(format!("{}", ArticleFormat::Sports), "Sports");
        assert_eq!(format!("{}", ArticleFormat::Entertainment), "Entertainment");
        assert_eq!(format!("{}", ArticleFormat::Card), "Card");
        assert_eq!(format!("{}", ArticleFormat::Unknown), "Unknown");
    }

    #[test]
    fn test_article_format_equality() {
        assert_eq!(ArticleFormat::General, ArticleFormat::General);
        assert_ne!(ArticleFormat::General, ArticleFormat::Sports);
    }

    #[test]
    fn test_article_format_debug() {
        let format = ArticleFormat::General;
        let debug_str = format!("{format:?}");
        assert!(debug_str.contains("General"));
    }

    #[test]
    fn test_article_format_clone() {
        let format = ArticleFormat::Sports;
        let cloned = format;
        assert_eq!(format, cloned);
    }
}
