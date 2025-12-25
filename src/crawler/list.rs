//! News list page crawler with pagination support
//!
//! This module provides functionality to crawl Naver News list pages and extract
//! article URLs with pagination handling.

use std::collections::HashSet;

use crate::crawler::fetcher::NaverFetcher;
use crate::crawler::url::UrlExtractor;
use crate::models::NewsCategory;
use crate::utils::error::CrawlerError;

/// News list page crawler with pagination support
pub struct NewsListCrawler {
    fetcher: NaverFetcher,
    url_extractor: UrlExtractor,
}

impl NewsListCrawler {
    /// Create new list crawler
    ///
    /// # Arguments
    ///
    /// * `fetcher` - Configured NaverFetcher instance
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ktime::crawler::fetcher::NaverFetcher;
    /// use ktime::crawler::list::NewsListCrawler;
    ///
    /// let fetcher = NaverFetcher::new(10).unwrap();
    /// let crawler = NewsListCrawler::new(fetcher);
    /// ```
    #[must_use]
    pub fn new(fetcher: NaverFetcher) -> Self {
        Self {
            fetcher,
            url_extractor: UrlExtractor::new(),
        }
    }

    /// Collect article URLs from a category with pagination
    ///
    /// # Arguments
    ///
    /// * `category` - News category (Politics, Economy, etc.)
    /// * `date` - Target date in YYYYMMDD format (e.g., "20241215")
    /// * `max_pages` - Maximum pages to crawl (0 = unlimited, recommended: 10)
    ///
    /// # Returns
    ///
    /// Deduplicated vector of article URLs
    ///
    /// # Errors
    ///
    /// Returns `CrawlerError` if fetching or parsing fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use ktime::crawler::fetcher::NaverFetcher;
    /// # use ktime::crawler::list::NewsListCrawler;
    /// # use ktime::models::NewsCategory;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let fetcher = NaverFetcher::new(10)?;
    /// let crawler = NewsListCrawler::new(fetcher);
    ///
    /// let urls = crawler.collect_urls(
    ///     NewsCategory::Politics,
    ///     "20241215",
    ///     10
    /// ).await?;
    ///
    /// println!("Found {} articles", urls.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn collect_urls(
        &self,
        category: NewsCategory,
        date: &str,
        max_pages: u32,
    ) -> Result<Vec<String>, CrawlerError> {
        // Validate date format
        if !Self::is_valid_date_format(date) {
            return Err(CrawlerError::InvalidDate(format!(
                "Invalid date format: {date}. Expected YYYYMMDD"
            )));
        }

        let mut all_urls = HashSet::new();
        let mut page = 1;

        loop {
            // Check if we've reached max pages (0 means unlimited)
            if max_pages > 0 && page > max_pages {
                tracing::debug!(page, max_pages, "Reached maximum pages limit");
                break;
            }

            tracing::debug!(
                category = %category,
                date,
                page,
                "Fetching list page"
            );

            // Fetch the page
            let (urls, has_more) = self.fetch_list_page(category, date, page).await?;

            // Check if we got any URLs
            if urls.is_empty() {
                tracing::debug!(page, "No URLs found on page, stopping pagination");
                break;
            }

            // Add to our collection
            let new_urls = urls.len();
            for url in urls {
                all_urls.insert(url);
            }

            tracing::debug!(
                page,
                new_urls,
                total = all_urls.len(),
                has_more,
                "Processed list page"
            );

            // Check if there are more pages
            if !has_more {
                tracing::debug!("No more pages available");
                break;
            }

            page += 1;
        }

        if all_urls.is_empty() {
            return Err(CrawlerError::NoArticlesFound);
        }

        // Convert to sorted vector for deterministic output
        let mut result: Vec<String> = all_urls.into_iter().collect();
        result.sort();

        tracing::info!(
            category = %category,
            date,
            total_urls = result.len(),
            pages = page,
            "Completed URL collection"
        );

        Ok(result)
    }

    /// Fetch a single list page and extract URLs
    ///
    /// # Arguments
    ///
    /// * `category` - News category
    /// * `date` - Target date in YYYYMMDD format
    /// * `page` - Page number to fetch
    ///
    /// # Returns
    ///
    /// Tuple of (extracted URLs, has_more_pages)
    ///
    /// # Errors
    ///
    /// Returns `CrawlerError` if fetching or parsing fails
    async fn fetch_list_page(
        &self,
        category: NewsCategory,
        date: &str,
        page: u32,
    ) -> Result<(Vec<String>, bool), CrawlerError> {
        let url = self.build_list_url(category, date, page);

        tracing::trace!(url = %url, "Fetching URL");

        // Fetch the HTML
        let html = self
            .fetcher
            .fetch_article(&url, category.to_section_id())
            .await?;

        // Extract URLs
        let urls = self.url_extractor.extract_urls(&html);

        // Check if there are more pages
        let has_more = self.has_next_page(&html, page);

        Ok((urls, has_more))
    }

    /// Build the list page URL for Naver News
    ///
    /// # Arguments
    ///
    /// * `category` - News category
    /// * `date` - Target date in YYYYMMDD format
    /// * `page` - Page number
    ///
    /// # Returns
    ///
    /// Formatted URL string
    fn build_list_url(&self, category: NewsCategory, date: &str, page: u32) -> String {
        ListUrlBuilder::main_list(category, date, page)
    }

    /// Check if there are more pages based on HTML content
    ///
    /// This method checks for:
    /// - Pagination links in HTML (look for class="paging" or similar)
    /// - "다음" (Next) button presence
    /// - Empty article list (indicates end of pages)
    ///
    /// # Arguments
    ///
    /// * `html` - HTML content to check
    /// * `current_page` - Current page number
    ///
    /// # Returns
    ///
    /// `true` if there are more pages to fetch
    fn has_next_page(&self, html: &str, current_page: u32) -> bool {
        // Check for pagination element with next page link
        if html.contains(&format!("page={}", current_page + 1)) {
            return true;
        }

        // Check for "다음" (Next) button
        if html.contains("다음</a>") || html.contains("class=\"next\"") {
            return true;
        }

        // Check if article list is empty (no URLs found)
        let urls = self.url_extractor.extract_urls(html);
        !urls.is_empty()
    }

    /// Validate date format (YYYYMMDD)
    ///
    /// # Arguments
    ///
    /// * `date` - Date string to validate
    ///
    /// # Returns
    ///
    /// `true` if the date is in valid YYYYMMDD format
    fn is_valid_date_format(date: &str) -> bool {
        // Must be exactly 8 digits
        if date.len() != 8 {
            return false;
        }

        // Must be all digits
        if !date.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        // Parse components
        if let (Ok(year), Ok(month), Ok(day)) = (
            date[0..4].parse::<u32>(),
            date[4..6].parse::<u32>(),
            date[6..8].parse::<u32>(),
        ) {
            // Basic validation
            if !(2000..=2100).contains(&year) {
                return false;
            }
            if !(1..=12).contains(&month) {
                return false;
            }
            if !(1..=31).contains(&day) {
                return false;
            }
            return true;
        }

        false
    }
}

/// URL builder for different Naver News list formats
pub struct ListUrlBuilder;

impl ListUrlBuilder {
    /// Build main news list URL
    ///
    /// Format: `https://news.naver.com/main/list.naver?mode=LSD&mid=shm&sid1={section_id}&date={date}&page={page}`
    ///
    /// # Arguments
    ///
    /// * `category` - News category
    /// * `date` - Date in YYYYMMDD format
    /// * `page` - Page number
    ///
    /// # Returns
    ///
    /// Formatted URL string
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::list::ListUrlBuilder;
    /// use ktime::models::NewsCategory;
    ///
    /// let url = ListUrlBuilder::main_list(NewsCategory::Politics, "20241215", 1);
    /// assert!(url.contains("sid1=100"));
    /// assert!(url.contains("date=20241215"));
    /// assert!(url.contains("page=1"));
    /// ```
    #[must_use]
    pub fn main_list(category: NewsCategory, date: &str, page: u32) -> String {
        format!(
            "https://news.naver.com/main/list.naver?mode=LSD&mid=shm&sid1={}&date={}&page={}",
            category.to_section_id(),
            date,
            page
        )
    }

    /// Build section ranking list URL
    ///
    /// Format: `https://news.naver.com/main/ranking/popularDay.naver?mid=etc&sid1={section_id}`
    ///
    /// # Arguments
    ///
    /// * `category` - News category
    /// * `page` - Page number
    ///
    /// # Returns
    ///
    /// Formatted URL string
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::list::ListUrlBuilder;
    /// use ktime::models::NewsCategory;
    ///
    /// let url = ListUrlBuilder::ranking_list(NewsCategory::IT, 1);
    /// assert!(url.contains("sid1=105"));
    /// assert!(url.contains("popularDay"));
    /// ```
    #[must_use]
    pub fn ranking_list(category: NewsCategory, page: u32) -> String {
        format!(
            "https://news.naver.com/main/ranking/popularDay.naver?mid=etc&sid1={}&page={}",
            category.to_section_id(),
            page
        )
    }

    /// Build latest news list URL (newer format)
    ///
    /// Format: `https://news.naver.com/section/{section_id}`
    ///
    /// # Arguments
    ///
    /// * `category` - News category
    ///
    /// # Returns
    ///
    /// Formatted URL string
    ///
    /// # Examples
    ///
    /// ```
    /// use ktime::crawler::list::ListUrlBuilder;
    /// use ktime::models::NewsCategory;
    ///
    /// let url = ListUrlBuilder::section_latest(NewsCategory::Society);
    /// assert_eq!(url, "https://news.naver.com/section/102");
    /// ```
    #[must_use]
    pub fn section_latest(category: NewsCategory) -> String {
        format!(
            "https://news.naver.com/section/{}",
            category.to_section_id()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_list_url_politics() {
        let url = ListUrlBuilder::main_list(NewsCategory::Politics, "20241215", 1);
        assert!(url.contains("sid1=100"));
        assert!(url.contains("date=20241215"));
        assert!(url.contains("page=1"));
    }

    #[test]
    fn test_build_list_url_economy() {
        let url = ListUrlBuilder::main_list(NewsCategory::Economy, "20241215", 5);
        assert!(url.contains("sid1=101"));
        assert!(url.contains("page=5"));
    }

    #[test]
    fn test_ranking_url() {
        let url = ListUrlBuilder::ranking_list(NewsCategory::IT, 1);
        assert!(url.contains("sid1=105"));
        assert!(url.contains("popularDay"));
    }

    #[test]
    fn test_section_latest_url() {
        let url = ListUrlBuilder::section_latest(NewsCategory::Society);
        assert_eq!(url, "https://news.naver.com/section/102");
    }

    #[test]
    fn test_has_next_page_with_pagination() {
        let fetcher = NaverFetcher::new(10).unwrap();
        let crawler = NewsListCrawler::new(fetcher);
        let html = r#"<div class="paging"><a href="?page=2">2</a><a href="?page=3">다음</a></div>"#;
        assert!(crawler.has_next_page(html, 1));
    }

    #[test]
    fn test_has_next_page_empty() {
        let fetcher = NaverFetcher::new(10).unwrap();
        let crawler = NewsListCrawler::new(fetcher);
        let html = "<div>No articles</div>";
        assert!(!crawler.has_next_page(html, 1));
    }

    #[test]
    fn test_has_next_page_with_next_button() {
        let fetcher = NaverFetcher::new(10).unwrap();
        let crawler = NewsListCrawler::new(fetcher);
        let html = r#"<a class="next" href="?page=2">다음</a>"#;
        assert!(crawler.has_next_page(html, 1));
    }

    #[test]
    fn test_has_next_page_with_articles() {
        let fetcher = NaverFetcher::new(10).unwrap();
        let crawler = NewsListCrawler::new(fetcher);
        let html = r#"<a href="https://n.news.naver.com/mnews/article/001/0014123456">Article</a>"#;
        assert!(crawler.has_next_page(html, 1));
    }

    #[test]
    fn test_is_valid_date_format_valid() {
        assert!(NewsListCrawler::is_valid_date_format("20241215"));
        assert!(NewsListCrawler::is_valid_date_format("20200101"));
        assert!(NewsListCrawler::is_valid_date_format("20251231"));
    }

    #[test]
    fn test_is_valid_date_format_invalid() {
        // Wrong length
        assert!(!NewsListCrawler::is_valid_date_format("2024121"));
        assert!(!NewsListCrawler::is_valid_date_format("202412155"));

        // Non-digits
        assert!(!NewsListCrawler::is_valid_date_format("2024-12-15"));
        assert!(!NewsListCrawler::is_valid_date_format("abcd1215"));

        // Invalid ranges
        assert!(!NewsListCrawler::is_valid_date_format("19991215")); // Year too old
        assert!(!NewsListCrawler::is_valid_date_format("21011215")); // Year too far
        assert!(!NewsListCrawler::is_valid_date_format("20241315")); // Invalid month
        assert!(!NewsListCrawler::is_valid_date_format("20241232")); // Invalid day
    }

    #[test]
    fn test_all_categories_url_building() {
        for category in NewsCategory::all() {
            let url = ListUrlBuilder::main_list(category, "20241215", 1);
            assert!(url.contains(&format!("sid1={}", category.to_section_id())));
            assert!(url.contains("date=20241215"));
        }
    }
}
