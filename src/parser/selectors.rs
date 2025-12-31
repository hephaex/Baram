//! CSS selectors for different Naver News article formats
//!
//! This module provides specialized selectors for parsing various types of
//! Naver News articles including general news, entertainment, sports, and card news.

use lazy_static::lazy_static;
use scraper::Selector;

// Helper macro to parse selectors safely at compile time
macro_rules! parse_selector {
    ($s:expr) => {
        Selector::parse($s).expect(concat!("Invalid CSS selector: ", $s))
    };
}

lazy_static! {
    // General news selectors
    static ref GENERAL_TITLE: Vec<Selector> = vec![
        parse_selector!("#title_area span"),
        parse_selector!(".media_end_head_title"),
        parse_selector!("h2.media_end_head_headline"),
    ];

    static ref GENERAL_CONTENT: Vec<Selector> = vec![
        parse_selector!("#dic_area"),
        parse_selector!("#articleBodyContents"),
        parse_selector!("article#dic_area"),
    ];

    static ref GENERAL_DATE: Vec<Selector> = vec![
        parse_selector!(".media_end_head_info_datestamp_time"),
        parse_selector!("._ARTICLE_DATE_TIME"),
        parse_selector!("span.media_end_head_info_datestamp_time"),
    ];

    static ref GENERAL_PUBLISHER: Vec<Selector> = vec![
        parse_selector!(".media_end_head_top_logo img"),
        parse_selector!(".press_logo img"),
        parse_selector!("a.media_end_head_top_logo_img img"),
    ];

    static ref GENERAL_AUTHOR: Vec<Selector> = vec![
        parse_selector!(".byline"),
        parse_selector!(".journalist_name"),
        parse_selector!("span.byline_s"),
    ];

    // Entertainment news selectors
    static ref ENTERTAINMENT_TITLE: Vec<Selector> = vec![
        parse_selector!(".end_tit"),
        parse_selector!("h2.end_tit"),
        parse_selector!(".article_tit"),
        parse_selector!("h2.ArticleHead_article_title__qh8GV"),
        parse_selector!(".ArticleHead_article_title__qh8GV"),
        parse_selector!("h2[class*='article_title']"),
    ];

    static ref ENTERTAINMENT_CONTENT: Vec<Selector> = vec![
        parse_selector!(".article_body"),
        parse_selector!("#articeBody"),
        parse_selector!("div.end_body_wrp"),
        parse_selector!("article.Article_comp_news_article__XIpve"),
        parse_selector!("article[class*='_article_body']"),
        parse_selector!("div._article_content"),
        parse_selector!("article#comp_news_article"),
    ];

    static ref ENTERTAINMENT_DATE: Vec<Selector> = vec![
        parse_selector!(".article_info .author em"),
        parse_selector!(".info_date"),
        parse_selector!("span.author em"),
        parse_selector!(".DateInfo_info_item__3yQPs em.date"),
        parse_selector!(".DateInfo_article_head_date_info__CS6Gx em.date"),
        parse_selector!("div[class*='DateInfo'] em.date"),
    ];

    static ref ENTERTAINMENT_PUBLISHER: Vec<Selector> = vec![
        parse_selector!(".JournalistCard_press_name__s3Eup"),
        parse_selector!("em[class*='press_name']"),
        parse_selector!(".press_name"),
    ];

    static ref ENTERTAINMENT_AUTHOR: Vec<Selector> = vec![
        parse_selector!(".JournalistCard_name__0ZSAO"),
        parse_selector!("em[class*='name']"),
        parse_selector!(".journalist_name"),
    ];

    // Sports news selectors
    static ref SPORTS_TITLE: Vec<Selector> = vec![
        parse_selector!(".news_headline .title"),
        parse_selector!("h4.title"),
        parse_selector!(".NewsEndMain_article_title__j5ND9"),
        parse_selector!("h2.ArticleHead_article_title__qh8GV"),
        parse_selector!(".ArticleHead_article_title__qh8GV"),
        parse_selector!("h2[class*='article_title']"),
    ];

    static ref SPORTS_CONTENT: Vec<Selector> = vec![
        parse_selector!(".news_end"),
        parse_selector!("#newsEndContents"),
        parse_selector!("div.NewsEndMain_article_body__D5MUB"),
        parse_selector!("article.Article_comp_news_article__XIpve"),
        parse_selector!("article[class*='_article_body']"),
        parse_selector!("div._article_content"),
        parse_selector!("article#comp_news_article"),
    ];

    static ref SPORTS_DATE: Vec<Selector> = vec![
        parse_selector!(".info span"),
        parse_selector!(".news_date"),
        parse_selector!("em.date"),
        parse_selector!(".DateInfo_info_item__3yQPs em.date"),
        parse_selector!(".DateInfo_article_head_date_info__CS6Gx em.date"),
        parse_selector!("div[class*='DateInfo'] em.date"),
    ];

    static ref SPORTS_PUBLISHER: Vec<Selector> = vec![
        parse_selector!(".JournalistCard_press_name__s3Eup"),
        parse_selector!("em[class*='press_name']"),
        parse_selector!(".press_name"),
    ];

    static ref SPORTS_AUTHOR: Vec<Selector> = vec![
        parse_selector!(".JournalistCard_name__0ZSAO"),
        parse_selector!("em[class*='name']"),
        parse_selector!(".journalist_name"),
    ];

    // Card news selectors
    static ref CARD_TITLE: Vec<Selector> = vec![
        parse_selector!("h2.end_tit"),
        parse_selector!(".media_end_head_title"),
        parse_selector!("h3.tit_view"),
    ];

    static ref CARD_CONTENT: Vec<Selector> = vec![
        parse_selector!("div.end_ct_area"),
        parse_selector!("div.card_area"),
        parse_selector!("div.content_area"),
    ];

    static ref CARD_CAPTIONS: Vec<Selector> = vec![
        parse_selector!("em.img_desc"),
        parse_selector!(".txt"),
        parse_selector!("figcaption"),
    ];

    // Noise selectors - elements to filter out
    static ref NOISE_ELEMENTS: Vec<Selector> = {
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

        selectors
            .iter()
            .filter_map(|s| Selector::parse(s).ok())
            .collect()
    };
}

/// Selectors for general news format (n.news.naver.com)
pub struct GeneralSelectors {
    pub title: &'static [Selector],
    pub content: &'static [Selector],
    pub date: &'static [Selector],
    pub publisher: &'static [Selector],
    pub author: &'static [Selector],
}

impl GeneralSelectors {
    pub fn new() -> Self {
        Self {
            title: &GENERAL_TITLE,
            content: &GENERAL_CONTENT,
            date: &GENERAL_DATE,
            publisher: &GENERAL_PUBLISHER,
            author: &GENERAL_AUTHOR,
        }
    }
}

impl Default for GeneralSelectors {
    fn default() -> Self {
        Self::new()
    }
}

/// Selectors for entertainment news format (entertain.naver.com, m.entertain.naver.com)
pub struct EntertainmentSelectors {
    pub title: &'static [Selector],
    pub content: &'static [Selector],
    pub date: &'static [Selector],
    pub publisher: &'static [Selector],
    pub author: &'static [Selector],
}

impl EntertainmentSelectors {
    pub fn new() -> Self {
        Self {
            title: &ENTERTAINMENT_TITLE,
            content: &ENTERTAINMENT_CONTENT,
            date: &ENTERTAINMENT_DATE,
            publisher: &ENTERTAINMENT_PUBLISHER,
            author: &ENTERTAINMENT_AUTHOR,
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
    pub title: &'static [Selector],
    pub content: &'static [Selector],
    pub date: &'static [Selector],
    pub publisher: &'static [Selector],
    pub author: &'static [Selector],
}

impl SportsSelectors {
    pub fn new() -> Self {
        Self {
            title: &SPORTS_TITLE,
            content: &SPORTS_CONTENT,
            date: &SPORTS_DATE,
            publisher: &SPORTS_PUBLISHER,
            author: &SPORTS_AUTHOR,
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
    pub title: &'static [Selector],
    pub content: &'static [Selector],
    pub captions: &'static [Selector],
}

impl CardNewsSelectors {
    pub fn new() -> Self {
        Self {
            title: &CARD_TITLE,
            content: &CARD_CONTENT,
            captions: &CARD_CAPTIONS,
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
    pub elements: &'static [Selector],
}

impl NoiseSelectors {
    pub fn new() -> Self {
        Self {
            elements: &NOISE_ELEMENTS,
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
        // Desktop (3) + Mobile (3) = 6 title selectors
        assert!(selectors.title.len() >= 6);
        // Should have publisher and author selectors
        assert!(!selectors.publisher.is_empty());
        assert!(!selectors.author.is_empty());
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
