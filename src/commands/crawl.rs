use anyhow::{Context, Result};
use std::path::PathBuf;

use baram::config::{Config, DatabaseConfig};
use baram::crawler::fetcher::NaverFetcher;
use baram::crawler::list::NewsListCrawler;
use baram::crawler::Crawler;
use baram::models::{CrawlState, NewsCategory};
use baram::parser::ArticleParser;
use baram::storage::{ArticleStorage, CrawlStatus, Database};

pub async fn crawl(
    config: Config,
    category: Option<String>,
    max_articles: usize,
    url: Option<String>,
    _with_comments: bool,
    output: PathBuf,
    skip_existing: bool,
) -> Result<()> {
    println!("Starting Naver News Crawl");
    println!("========================");

    // Initialize database for deduplication
    let db_path = output.parent().unwrap_or(&output).join("crawl.db");
    let db_config = DatabaseConfig {
        sqlite_path: db_path.clone(),
        postgres_url: String::new(),
        pool_size: 5,
    };
    let mut db = Database::new(&db_config)?;
    db.init_sqlite(&db_path)?;

    // Initialize storage
    let storage = ArticleStorage::new(&output, skip_existing)?;

    // Initialize parser
    let parser = ArticleParser::new();

    // Initialize crawler
    let crawler = Crawler::new(config.clone())?;

    // Track stats
    let mut state = CrawlState::new();

    // Get today's date for list crawling
    let today = chrono::Local::now().format("%Y%m%d").to_string();

    if let Some(url) = url {
        // Single URL crawl
        println!("Crawling single URL: {url}");
        crawl_single_url(&crawler, &parser, &storage, &db, &url, &mut state).await?;
    } else {
        // Category crawl
        let categories = if let Some(cat) = category {
            vec![parse_category(&cat)?]
        } else {
            // Default to politics if no category specified
            vec![NewsCategory::Politics]
        };

        // Create fetcher for list crawling
        let fetcher = NaverFetcher::new(config.crawler.rate_limit as u32)
            .context("Failed to create fetcher")?;
        let list_crawler = NewsListCrawler::new(fetcher);

        for cat in categories {
            println!(
                "\nCrawling category: {} ({})",
                cat.korean_name(),
                cat.as_str()
            );

            // Calculate max pages needed (roughly 20 articles per page)
            let max_pages = max_articles.div_ceil(20) as u32;

            // Get list of article URLs
            let urls = list_crawler
                .collect_urls(cat, &today, max_pages)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to collect URLs: {e}"))?;

            println!("Found {} article URLs", urls.len());

            // Filter out already crawled URLs
            let uncrawled_urls = if skip_existing {
                db.filter_uncrawled(&urls)?
            } else {
                urls.clone()
            };

            println!(
                "New articles to crawl: {} (skipped: {})",
                uncrawled_urls.len(),
                urls.len() - uncrawled_urls.len()
            );

            // Crawl each URL
            for (i, url) in uncrawled_urls.iter().enumerate().take(max_articles) {
                print!(
                    "\r[{}/{}] Crawling: {}...",
                    i + 1,
                    uncrawled_urls.len().min(max_articles),
                    truncate_url(url, 50)
                );
                std::io::Write::flush(&mut std::io::stdout())?;

                match crawl_single_url(&crawler, &parser, &storage, &db, url, &mut state).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(url = %url, error = %e, "Failed to crawl article");
                        state.record_error();
                        db.record_failure(url, &e.to_string())?;
                    }
                }

                // Small delay between requests
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            println!(); // New line after progress
        }
    }

    // Print summary
    println!("\nCrawl Summary");
    println!("=============");
    println!("Total processed: {}", state.stats().total_crawled);
    println!(
        "Successful: {}",
        state.stats().total_crawled - state.stats().total_errors
    );
    println!("Failed: {}", state.stats().total_errors);
    println!("Output directory: {}", output.display());
    println!("Database: {}", db_path.display());

    // Show database stats
    let db_stats = db.get_stats()?;
    println!("\nDatabase Stats");
    println!("--------------");
    println!("Total records: {}", db_stats.total);
    println!("Success: {}", db_stats.success);
    println!("Failed: {}", db_stats.failed);
    println!("Success rate: {:.1}%", db_stats.success_rate() * 100.0);

    Ok(())
}

async fn crawl_single_url(
    crawler: &Crawler,
    parser: &ArticleParser,
    storage: &ArticleStorage<'_>,
    db: &Database,
    url: &str,
    state: &mut CrawlState,
) -> Result<()> {
    // Fetch HTML
    let html = crawler.fetch_text(url).await?;

    // Parse article
    let article = parser.parse_with_fallback(&html, url)?;

    // Check for duplicate content
    if let Some(hash) = &article.content_hash {
        if db.is_content_duplicate(hash)? {
            tracing::debug!(url = %url, "Skipping duplicate content");
            db.mark_url_crawled(&article.id(), url, hash, CrawlStatus::Skipped, None)?;
            return Ok(());
        }
    }

    // Save to markdown
    if let Some(path) = storage.save(&article)? {
        tracing::debug!(path = %path.display(), "Saved article");
    }

    // Record in database
    db.record_success(&article)?;
    state.mark_completed(url);

    Ok(())
}

fn parse_category(s: &str) -> Result<NewsCategory> {
    match s.to_lowercase().as_str() {
        "politics" | "정치" => Ok(NewsCategory::Politics),
        "economy" | "경제" => Ok(NewsCategory::Economy),
        "society" | "사회" => Ok(NewsCategory::Society),
        "culture" | "생활/문화" | "생활" | "문화" => Ok(NewsCategory::Culture),
        "world" | "세계" => Ok(NewsCategory::World),
        "it" | "과학" | "it/과학" => Ok(NewsCategory::IT),
        _ => anyhow::bail!(
            "Unknown category: {s}. Valid: politics, economy, society, culture, world, it"
        ),
    }
}

fn truncate_url(url: &str, max_len: usize) -> &str {
    if url.len() <= max_len {
        url
    } else {
        &url[..max_len]
    }
}

pub async fn resume(checkpoint: PathBuf, max_articles: Option<usize>, output: PathBuf) -> Result<()> {
    println!("Resuming crawl from checkpoint: {}", checkpoint.display());

    // Load checkpoint database
    let db_config = DatabaseConfig {
        sqlite_path: checkpoint.clone(),
        postgres_url: String::new(),
        pool_size: 5,
    };
    let mut db = Database::new(&db_config)?;
    db.init_sqlite(&checkpoint)?;

    // Get stats
    let stats = db.get_stats()?;
    println!("\nCheckpoint Stats");
    println!("----------------");
    println!("Total: {}", stats.total);
    println!("Success: {}", stats.success);
    println!("Failed: {}", stats.failed);

    // Load last checkpoint state
    if let Some(last_category) = db.load_checkpoint("last_category")? {
        println!("Last category: {last_category}");
    }
    if let Some(last_page) = db.load_checkpoint("last_page")? {
        println!("Last page: {last_page}");
    }

    // Load config and continue crawling
    let config = Config::default();
    let max = max_articles.unwrap_or(100);

    println!("\nContinuing crawl with max {max} articles...");
    println!("Output directory: {}", output.display());

    // For now, just restart the crawl with the existing database
    // A full resume implementation would track the exact position
    crawl(config, None, max, None, false, output, true).await
}

pub fn stats(database: PathBuf) -> Result<()> {
    if !database.exists() {
        println!("Database not found: {}", database.display());
        println!("Run a crawl first to create the database.");
        return Ok(());
    }

    let db_config = DatabaseConfig {
        sqlite_path: database.clone(),
        postgres_url: String::new(),
        pool_size: 5,
    };
    let mut db = Database::new(&db_config)?;
    db.init_sqlite(&database)?;

    let stats = db.get_stats()?;

    println!("Crawl Statistics");
    println!("================");
    println!("Database: {}", database.display());
    println!();
    println!("Total records: {}", stats.total);
    println!(
        "  Success: {} ({:.1}%)",
        stats.success,
        if stats.total > 0 {
            stats.success as f64 / stats.total as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  Failed:  {} ({:.1}%)",
        stats.failed,
        if stats.total > 0 {
            stats.failed as f64 / stats.total as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  Skipped: {} ({:.1}%)",
        stats.skipped,
        if stats.total > 0 {
            stats.skipped as f64 / stats.total as f64 * 100.0
        } else {
            0.0
        }
    );

    Ok(())
}
