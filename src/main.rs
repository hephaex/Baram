use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use ntimes::config::{Config, DatabaseConfig};
use ntimes::crawler::fetcher::NaverFetcher;
use ntimes::crawler::list::NewsListCrawler;
use ntimes::crawler::Crawler;
use ntimes::embedding::{EmbeddingConfig, Embedder};
use ntimes::models::{CrawlState, NewsCategory};
use ntimes::parser::ArticleParser;
use ntimes::storage::{ArticleStorage, CrawlStatus, Database};

#[derive(Parser)]
#[command(
    name = "ntimes",
    version,
    about = "Advanced Naver News crawler with vector search and ontology extraction",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Log format (text, json)
    #[arg(long, global = true, default_value = "text")]
    log_format: String,

    /// Config file path
    #[arg(short, long, global = true, default_value = "config.toml")]
    config: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Crawl Naver news articles
    Crawl {
        /// News category to crawl (politics, economy, society, culture, world, it)
        #[arg(short = 'C', long)]
        category: Option<String>,

        /// Maximum number of articles to crawl
        #[arg(short, long, default_value = "100")]
        max_articles: usize,

        /// Specific article URL to crawl
        #[arg(short, long)]
        url: Option<String>,

        /// Include comments in crawl
        #[arg(long, default_value = "false")]
        with_comments: bool,

        /// Output directory for markdown files
        #[arg(short, long, default_value = "./output/raw")]
        output: PathBuf,

        /// Skip already crawled articles
        #[arg(long, default_value = "true")]
        skip_existing: bool,
    },

    /// Index articles into OpenSearch
    Index {
        /// Input file or database to index from
        #[arg(short, long)]
        input: String,

        /// Batch size for indexing
        #[arg(short, long, default_value = "50")]
        batch_size: usize,

        /// Force reindex existing documents
        #[arg(long, default_value = "false")]
        force: bool,
    },

    /// Search articles using vector similarity
    Search {
        /// Search query
        query: String,

        /// Number of results to return
        #[arg(short, long, default_value = "10")]
        k: usize,

        /// Minimum similarity threshold
        #[arg(long)]
        threshold: Option<f32>,
    },

    /// Extract ontology from articles
    Ontology {
        /// Input file or database
        #[arg(short, long)]
        input: String,

        /// Output format (json, turtle, rdf)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Resume crawling from checkpoint
    Resume {
        /// Checkpoint file path (SQLite database)
        #[arg(short = 'C', long)]
        checkpoint: PathBuf,

        /// Override max articles
        #[arg(long)]
        max_articles: Option<usize>,

        /// Output directory for markdown files
        #[arg(short, long, default_value = "./output/raw")]
        output: PathBuf,
    },

    /// Show crawl statistics
    Stats {
        /// SQLite database path
        #[arg(short, long, default_value = "./output/crawl.db")]
        database: PathBuf,
    },

    /// Start embedding server for vector generation
    EmbeddingServer {
        /// Port to listen on
        #[arg(short, long, default_value = "8090")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Model ID (HuggingFace model or local path)
        #[arg(short, long, default_value = "intfloat/multilingual-e5-large")]
        model: String,

        /// Maximum sequence length
        #[arg(long, default_value = "512")]
        max_seq_length: usize,

        /// Batch size for inference
        #[arg(long, default_value = "32")]
        batch_size: usize,

        /// Use GPU if available
        #[arg(long, default_value = "true")]
        use_gpu: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing/logging
    setup_tracing(&cli.log_format, cli.verbose)?;

    tracing::info!("nTimes Naver News Crawler starting");

    // Load config
    let config = if cli.config.exists() {
        Config::from_file(&cli.config)?
    } else {
        tracing::warn!(path = %cli.config.display(), "Config file not found, using defaults");
        Config::default()
    };

    match cli.command {
        Commands::Crawl {
            category,
            max_articles,
            url,
            with_comments,
            output,
            skip_existing,
        } => {
            tracing::info!(
                category = ?category,
                max_articles = %max_articles,
                url = ?url,
                with_comments = %with_comments,
                output = %output.display(),
                "Starting crawl command"
            );
            crawl(
                config,
                category,
                max_articles,
                url,
                with_comments,
                output,
                skip_existing,
            )
            .await?;
        }

        Commands::Index {
            input,
            batch_size,
            force,
        } => {
            tracing::info!(
                input = %input,
                batch_size = %batch_size,
                force = %force,
                "Starting index command"
            );
            index(input, batch_size, force).await?;
        }

        Commands::Search {
            query,
            k,
            threshold,
        } => {
            tracing::info!(
                query = %query,
                k = %k,
                threshold = ?threshold,
                "Starting search command"
            );
            search(query, k, threshold).await?;
        }

        Commands::Ontology {
            input,
            format,
            output,
        } => {
            tracing::info!(
                input = %input,
                format = %format,
                output = ?output,
                "Starting ontology command"
            );
            ontology(input, format, output).await?;
        }

        Commands::Resume {
            checkpoint,
            max_articles,
            output,
        } => {
            tracing::info!(
                checkpoint = %checkpoint.display(),
                max_articles = ?max_articles,
                "Starting resume command"
            );
            resume(checkpoint, max_articles, output).await?;
        }

        Commands::Stats { database } => {
            stats(database)?;
        }

        Commands::EmbeddingServer {
            port,
            host,
            model,
            max_seq_length,
            batch_size,
            use_gpu,
        } => {
            tracing::info!(
                host = %host,
                port = %port,
                model = %model,
                use_gpu = %use_gpu,
                "Starting embedding server"
            );
            embedding_server(host, port, model, max_seq_length, batch_size, use_gpu).await?;
        }
    }

    tracing::info!("nTimes completed successfully");
    Ok(())
}

fn setup_tracing(format: &str, verbose: bool) -> Result<()> {
    let env_filter = if verbose {
        tracing_subscriber::EnvFilter::new("ntimes=debug,info")
    } else {
        tracing_subscriber::EnvFilter::new("ntimes=info,warn")
    };

    match format {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().pretty())
                .init();
        }
    }

    Ok(())
}

async fn crawl(
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

async fn resume(checkpoint: PathBuf, max_articles: Option<usize>, output: PathBuf) -> Result<()> {
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

fn stats(database: PathBuf) -> Result<()> {
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

async fn index(input: String, batch_size: usize, force: bool) -> Result<()> {
    use ntimes::config::OpenSearchConfig;
    use ntimes::embedding::VectorStore;
    use std::fs;

    println!("Indexing articles from: {input}");
    println!("================================");

    // Create OpenSearch client
    let opensearch_config = OpenSearchConfig {
        url: std::env::var("OPENSEARCH_URL")
            .unwrap_or_else(|_| "http://localhost:9200".to_string()),
        index_name: std::env::var("OPENSEARCH_INDEX")
            .unwrap_or_else(|_| "ntimes-articles".to_string()),
        username: std::env::var("OPENSEARCH_USER").ok(),
        password: std::env::var("OPENSEARCH_PASSWORD").ok(),
    };

    let store = VectorStore::new(&opensearch_config).context("Failed to connect to OpenSearch")?;

    // Create index if it doesn't exist
    let index_exists = store.index_exists().await?;
    if !index_exists {
        println!("Creating index '{}'...", opensearch_config.index_name);
        // Use 384 dimensions for multilingual MiniLM
        store
            .create_index(384)
            .await
            .context("Failed to create index")?;
        println!("Index created successfully.");
    } else if force {
        println!("Force reindex: deleting existing index...");
        store.delete_index().await?;
        store.create_index(384).await?;
        println!("Index recreated.");
    } else {
        println!("Index '{}' already exists.", opensearch_config.index_name);
    }

    // Collect markdown files from input directory
    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        anyhow::bail!("Input path does not exist: {input}");
    }

    let mut documents: Vec<ntimes::embedding::IndexDocument> = Vec::new();

    if input_path.is_dir() {
        let entries: Vec<_> = fs::read_dir(&input_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
            .collect();

        println!("Found {} markdown files", entries.len());

        for entry in entries {
            let path = entry.path();
            match parse_markdown_to_document(&path) {
                Ok(doc) => documents.push(doc),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to parse markdown");
                }
            }
        }
    } else {
        // Single file
        documents.push(parse_markdown_to_document(&input_path)?);
    }

    if documents.is_empty() {
        println!("No documents to index.");
        return Ok(());
    }

    println!(
        "Indexing {} documents (batch size: {})...",
        documents.len(),
        batch_size
    );

    // Index in batches
    let mut total_success = 0;
    let mut total_failed = 0;

    for (batch_num, batch) in documents.chunks(batch_size).enumerate() {
        print!(
            "\rProcessing batch {}/{}...",
            batch_num + 1,
            documents.len().div_ceil(batch_size)
        );
        std::io::Write::flush(&mut std::io::stdout())?;

        let result = store.bulk_index(batch).await?;
        total_success += result.success;
        total_failed += result.failed;
    }

    println!("\n\nIndexing Complete");
    println!("=================");
    println!("Successful: {total_success}");
    println!("Failed: {total_failed}");

    // Refresh index
    store.refresh().await?;

    let count = store.count().await?;
    println!("Total documents in index: {count}");

    Ok(())
}

fn parse_markdown_to_document(path: &std::path::Path) -> Result<ntimes::embedding::IndexDocument> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    // Parse markdown to extract metadata
    let lines: Vec<&str> = content.lines().collect();

    // Extract title (first # heading)
    let title = lines
        .iter()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    // Extract metadata from YAML frontmatter or inline
    let mut oid = String::new();
    let mut aid = String::new();
    let mut category = String::new();
    let mut publisher = None;
    let mut author = None;
    let mut url = String::new();
    let mut published_at = None;

    let mut in_metadata = false;
    let mut body_lines = Vec::new();

    for line in &lines {
        if line.starts_with("---") {
            in_metadata = !in_metadata;
            continue;
        }

        if in_metadata {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                match key {
                    "oid" => oid = value.to_string(),
                    "aid" => aid = value.to_string(),
                    "category" => category = value.to_string(),
                    "publisher" => publisher = Some(value.to_string()),
                    "author" => author = Some(value.to_string()),
                    "url" => url = value.to_string(),
                    "published_at" | "date" => published_at = Some(value.to_string()),
                    _ => {}
                }
            }
        } else if !line.starts_with('#') && !line.is_empty() {
            body_lines.push(*line);
        }
    }

    // Build content from body
    let article_content = body_lines.join("\n");

    // Generate ID from filename if not available
    if oid.is_empty() || aid.is_empty() {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        if let Some((o, a)) = stem.split_once('_') {
            oid = o.to_string();
            aid = a.to_string();
        } else {
            oid = "000".to_string();
            aid = stem.to_string();
        }
    }

    // Create dummy embedding (will be replaced with real embedding later)
    let embedding = vec![0.0f32; 384];

    Ok(ntimes::embedding::IndexDocument {
        id: format!("{oid}_{aid}"),
        oid,
        aid,
        title,
        content: article_content,
        category,
        publisher,
        author,
        url,
        published_at,
        crawled_at: chrono::Utc::now().to_rfc3339(),
        comment_count: None,
        embedding,
        chunk_index: None,
        chunk_text: None,
    })
}

async fn search(query: String, k: usize, threshold: Option<f32>) -> Result<()> {
    use ntimes::config::OpenSearchConfig;
    use ntimes::embedding::{SearchConfig, VectorStore};

    println!("Searching for: \"{query}\"");
    println!("================================");

    // Create OpenSearch client with default config
    let opensearch_config = OpenSearchConfig {
        url: std::env::var("OPENSEARCH_URL")
            .unwrap_or_else(|_| "http://localhost:9200".to_string()),
        index_name: std::env::var("OPENSEARCH_INDEX")
            .unwrap_or_else(|_| "ntimes-articles".to_string()),
        username: std::env::var("OPENSEARCH_USER").ok(),
        password: std::env::var("OPENSEARCH_PASSWORD").ok(),
    };

    let store = VectorStore::new(&opensearch_config).context("Failed to connect to OpenSearch")?;

    // Check if index exists
    if !store.index_exists().await? {
        println!("Index '{}' does not exist.", opensearch_config.index_name);
        println!("Run 'ntimes index' first to create and populate the index.");
        return Ok(());
    }

    // Configure search
    let search_config = SearchConfig {
        k,
        min_score: threshold,
        include_highlights: true,
        ..Default::default()
    };

    // Perform BM25 text search
    let results = store
        .search_bm25(&query, &search_config)
        .await
        .context("Search failed")?;

    if results.is_empty() {
        println!("\nNo results found for \"{query}\"");
        return Ok(());
    }

    println!("\nFound {} results:\n", results.len());

    for (i, result) in results.iter().enumerate() {
        println!("{}. {} (score: {:.3})", i + 1, result.title, result.score);
        println!(
            "   Category: {} | Publisher: {}",
            result.category,
            result.publisher.as_deref().unwrap_or("Unknown")
        );
        if let Some(date) = &result.published_at {
            println!("   Published: {date}");
        }

        // Show highlights if available
        if let Some(highlights) = &result.highlights {
            for highlight in highlights.iter().take(2) {
                println!(
                    "   > {}",
                    highlight.replace("<mark>", "[").replace("</mark>", "]")
                );
            }
        } else {
            // Show content preview
            let preview = if result.content.len() > 150 {
                format!("{}...", &result.content[..150])
            } else {
                result.content.clone()
            };
            println!("   > {preview}");
        }
        println!("   URL: {}", result.url);
        println!();
    }

    Ok(())
}

async fn ontology(input: String, format: String, output: Option<String>) -> Result<()> {
    println!("Ontology functionality not yet implemented");
    println!("  Input: {input}");
    println!("  Format: {format}");
    if let Some(output) = output {
        println!("  Output: {output}");
    }
    Ok(())
}

// ============================================================================
// Embedding Server Implementation
// ============================================================================

/// Shared state for embedding server
struct EmbeddingServerState {
    embedder: RwLock<Embedder>,
    model_name: String,
    ready: std::sync::atomic::AtomicBool,
}

/// Request for single text embedding
#[derive(Debug, Deserialize)]
struct EmbedRequest {
    text: String,
}

/// Request for batch text embedding
#[derive(Debug, Deserialize)]
struct BatchEmbedRequest {
    texts: Vec<String>,
}

/// Response for embedding requests
#[derive(Debug, Serialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
    dimension: usize,
}

/// Response for batch embedding requests
#[derive(Debug, Serialize)]
struct BatchEmbedResponse {
    embeddings: Vec<Vec<f32>>,
    count: usize,
    dimension: usize,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    model: String,
    ready: bool,
    device: String,
}

/// Error response
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Start the embedding server
async fn embedding_server(
    host: String,
    port: u16,
    model: String,
    max_seq_length: usize,
    batch_size: usize,
    use_gpu: bool,
) -> Result<()> {
    println!("Starting Embedding Server");
    println!("=========================");
    println!("  Host: {host}");
    println!("  Port: {port}");
    println!("  Model: {model}");
    println!("  Max Sequence Length: {max_seq_length}");
    println!("  Batch Size: {batch_size}");
    println!("  Use GPU: {use_gpu}");
    println!();

    // Initialize embedding model
    println!("Loading embedding model...");
    let config = EmbeddingConfig {
        model_id: model.clone(),
        embedding_dim: 1024, // multilingual-e5-large uses 1024 dimensions
        max_seq_length,
        use_gpu,
        batch_size,
        normalize: true,
    };

    let embedder = Embedder::from_pretrained(config).context("Failed to load embedding model")?;

    let device = if use_gpu {
        "cuda (if available)"
    } else {
        "cpu"
    };

    println!("Model loaded successfully!");
    println!("  Device: {device}");
    println!();

    // Create shared state
    let state = Arc::new(EmbeddingServerState {
        embedder: RwLock::new(embedder),
        model_name: model,
        ready: std::sync::atomic::AtomicBool::new(true),
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/embed", post(embed_handler))
        .route("/embed/batch", post(batch_embed_handler))
        .route("/", get(root_handler))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context(format!("Failed to bind to {addr}"))?;

    println!("Embedding server listening on http://{addr}");
    println!();
    println!("Endpoints:");
    println!("  GET  /health      - Health check");
    println!("  POST /embed       - Single text embedding");
    println!("  POST /embed/batch - Batch text embedding");
    println!();

    axum::serve(listener, app)
        .await
        .context("Server error")?;

    Ok(())
}

/// Root handler - welcome message
async fn root_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "nTimes Embedding Server",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "health": "GET /health",
            "embed": "POST /embed",
            "batch_embed": "POST /embed/batch"
        }
    }))
}

/// Health check handler
async fn health_handler(
    State(state): State<Arc<EmbeddingServerState>>,
) -> Json<HealthResponse> {
    let ready = state.ready.load(std::sync::atomic::Ordering::Relaxed);
    Json(HealthResponse {
        status: if ready { "healthy".to_string() } else { "loading".to_string() },
        model: state.model_name.clone(),
        ready,
        device: "auto".to_string(),
    })
}

/// Single text embedding handler
async fn embed_handler(
    State(state): State<Arc<EmbeddingServerState>>,
    Json(request): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, (StatusCode, Json<ErrorResponse>)> {
    if request.text.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Text cannot be empty".to_string(),
            }),
        ));
    }

    let mut embedder = state.embedder.write().await;

    match embedder.embed(&request.text) {
        Ok(embedding) => {
            let dimension = embedding.len();
            Ok(Json(EmbedResponse { embedding, dimension }))
        }
        Err(e) => {
            tracing::error!(error = %e, "Embedding failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Embedding failed: {e}"),
                }),
            ))
        }
    }
}

/// Batch text embedding handler
async fn batch_embed_handler(
    State(state): State<Arc<EmbeddingServerState>>,
    Json(request): Json<BatchEmbedRequest>,
) -> Result<Json<BatchEmbedResponse>, (StatusCode, Json<ErrorResponse>)> {
    if request.texts.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Texts array cannot be empty".to_string(),
            }),
        ));
    }

    if request.texts.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Maximum 100 texts per batch".to_string(),
            }),
        ));
    }

    let mut embedder = state.embedder.write().await;

    match embedder.embed_batch(&request.texts) {
        Ok(embeddings) => {
            let count = embeddings.len();
            let dimension = embeddings.first().map(|e| e.len()).unwrap_or(0);
            Ok(Json(BatchEmbedResponse {
                embeddings,
                count,
                dimension,
            }))
        }
        Err(e) => {
            tracing::error!(error = %e, "Batch embedding failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Batch embedding failed: {e}"),
                }),
            ))
        }
    }
}
