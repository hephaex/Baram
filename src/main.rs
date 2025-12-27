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

use baram::config::{Config, DatabaseConfig};
use baram::coordinator::{CoordinatorConfig, CoordinatorServer};
use baram::crawler::distributed::DistributedRunner;
use baram::crawler::fetcher::NaverFetcher;
use baram::crawler::instance::InstanceConfig;
use baram::crawler::list::NewsListCrawler;
use baram::crawler::Crawler;
use baram::embedding::{Embedder, EmbeddingConfig};
use baram::models::{CrawlState, NewsCategory, ParsedArticle};
use baram::ontology::{RelationExtractor, TripleStore};
use baram::parser::ArticleParser;
use baram::scheduler::rotation::CrawlerInstance;
use baram::storage::{ArticleStorage, CrawlStatus, Database};

#[derive(Parser)]
#[command(
    name = "baram",
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

    /// Run distributed crawler mode
    Distributed {
        /// Instance ID (main, sub1, sub2)
        #[arg(short, long)]
        instance: String,

        /// Coordinator server URL
        #[arg(short = 'C', long, default_value = "http://localhost:8080")]
        coordinator: String,

        /// PostgreSQL database URL for deduplication
        #[arg(short, long)]
        database: String,

        /// Heartbeat interval in seconds
        #[arg(long, default_value = "30")]
        heartbeat_interval: u64,

        /// Requests per second
        #[arg(long, default_value = "1.0")]
        rps: f64,

        /// Output directory
        #[arg(short, long, default_value = "./output")]
        output: String,

        /// Include comments
        #[arg(long, default_value = "true")]
        with_comments: bool,

        /// Run once (execute current slot and exit)
        #[arg(long, default_value = "false")]
        once: bool,
    },

    /// Start coordinator server for distributed crawling
    Coordinator {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Heartbeat timeout in seconds
        #[arg(long, default_value = "90")]
        heartbeat_timeout: u64,

        /// Expected heartbeat interval in seconds
        #[arg(long, default_value = "30")]
        heartbeat_interval: u64,

        /// Maximum registered instances
        #[arg(long, default_value = "10")]
        max_instances: usize,

        /// Schedule cache file path
        #[arg(long)]
        schedule_cache: Option<String>,

        /// Disable CORS
        #[arg(long, default_value = "false")]
        disable_cors: bool,

        /// Disable request logging
        #[arg(long, default_value = "false")]
        disable_logging: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing/logging
    setup_tracing(&cli.log_format, cli.verbose)?;

    tracing::info!("baram Naver News Crawler starting");

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

        Commands::Distributed {
            instance,
            coordinator,
            database,
            heartbeat_interval,
            rps,
            output,
            with_comments,
            once,
        } => {
            tracing::info!(
                instance = %instance,
                coordinator = %coordinator,
                once = %once,
                "Starting distributed crawler"
            );
            distributed_crawler(DistributedCrawlerParams {
                instance,
                coordinator,
                database,
                heartbeat_interval,
                rps,
                output,
                with_comments,
                once,
            })
            .await?;
        }

        Commands::Coordinator {
            port,
            host,
            heartbeat_timeout,
            heartbeat_interval,
            max_instances,
            schedule_cache,
            disable_cors,
            disable_logging,
        } => {
            tracing::info!(
                host = %host,
                port = %port,
                max_instances = %max_instances,
                "Starting coordinator server"
            );
            coordinator_server(CoordinatorParams {
                host,
                port,
                heartbeat_timeout,
                heartbeat_interval,
                max_instances,
                schedule_cache,
                enable_cors: !disable_cors,
                enable_logging: !disable_logging,
            })
            .await?;
        }
    }

    tracing::info!("baram completed successfully");
    Ok(())
}

fn setup_tracing(format: &str, verbose: bool) -> Result<()> {
    let env_filter = if verbose {
        tracing_subscriber::EnvFilter::new("baram=debug,info")
    } else {
        tracing_subscriber::EnvFilter::new("baram=info,warn")
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
    use baram::config::OpenSearchConfig;
    use baram::embedding::VectorStore;
    use std::fs;

    println!("Indexing articles from: {input}");
    println!("================================");

    // Create OpenSearch client
    let opensearch_config = OpenSearchConfig {
        url: std::env::var("OPENSEARCH_URL")
            .unwrap_or_else(|_| "http://localhost:9200".to_string()),
        index_name: std::env::var("OPENSEARCH_INDEX")
            .unwrap_or_else(|_| "baram-articles".to_string()),
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

    let mut documents: Vec<baram::embedding::IndexDocument> = Vec::new();

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

    // Check for embedding server
    let embedding_server_url = std::env::var("EMBEDDING_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string());

    let use_embeddings = check_embedding_server(&embedding_server_url).await;
    if use_embeddings {
        println!("Embedding server available at {embedding_server_url}");
    } else {
        println!("Warning: Embedding server not available, using dummy embeddings");
    }

    // Index in batches
    let mut total_success = 0;
    let mut total_failed = 0;
    let client = reqwest::Client::new();

    for (batch_num, batch) in documents.chunks(batch_size).enumerate() {
        print!(
            "\rProcessing batch {}/{}...",
            batch_num + 1,
            documents.len().div_ceil(batch_size)
        );
        std::io::Write::flush(&mut std::io::stdout())?;

        // Generate embeddings if server is available
        let batch_with_embeddings: Vec<baram::embedding::IndexDocument> = if use_embeddings {
            let mut updated_batch = Vec::with_capacity(batch.len());
            for doc in batch {
                let text = format!("{} {}", doc.title, doc.content);
                match generate_embedding(&client, &embedding_server_url, &text).await {
                    Ok(embedding) => {
                        let mut new_doc = doc.clone();
                        new_doc.embedding = embedding;
                        updated_batch.push(new_doc);
                    }
                    Err(e) => {
                        tracing::warn!(doc_id = %doc.id, error = %e, "Failed to generate embedding");
                        updated_batch.push(doc.clone());
                    }
                }
            }
            updated_batch
        } else {
            batch.to_vec()
        };

        let result = store.bulk_index(&batch_with_embeddings).await?;
        total_success += result.success;
        total_failed += result.failed;

        // Print errors if any
        if !result.errors.is_empty() {
            eprintln!("\nErrors in batch {}:", batch_num + 1);
            for (i, err) in result.errors.iter().take(3).enumerate() {
                eprintln!("  {}: {}", i + 1, err);
            }
            if result.errors.len() > 3 {
                eprintln!("  ... and {} more errors", result.errors.len() - 3);
            }
        }
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

/// Check if embedding server is available
async fn check_embedding_server(url: &str) -> bool {
    let client = reqwest::Client::new();
    match client.get(format!("{url}/health")).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Generate embedding for text using embedding server
async fn generate_embedding(
    client: &reqwest::Client,
    server_url: &str,
    text: &str,
) -> Result<Vec<f32>> {
    #[derive(Serialize)]
    struct EmbedRequest<'a> {
        text: &'a str,
    }

    #[derive(Deserialize)]
    struct EmbedResponse {
        embedding: Vec<f32>,
    }

    // Truncate text to avoid token limit issues
    let truncated_text: String = text.chars().take(2000).collect();

    let response = client
        .post(format!("{server_url}/embed"))
        .json(&EmbedRequest { text: &truncated_text })
        .send()
        .await
        .context("Failed to send embedding request")?;

    let embed_response: EmbedResponse = response
        .json()
        .await
        .context("Failed to parse embedding response")?;

    Ok(embed_response.embedding)
}

fn parse_markdown_to_document(path: &std::path::Path) -> Result<baram::embedding::IndexDocument> {
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

    let mut frontmatter_delim_count = 0;
    let mut in_metadata = false;
    let mut body_lines = Vec::new();

    for line in &lines {
        // Handle YAML frontmatter delimiters (only first two --- are special)
        if line.starts_with("---") {
            if frontmatter_delim_count < 2 {
                frontmatter_delim_count += 1;
                in_metadata = frontmatter_delim_count == 1;
                continue;
            }
            // After frontmatter, --- is just a content separator
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
        } else if !line.is_empty() {
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

    // Convert published_at to ISO 8601 format
    let published_at_iso = published_at.and_then(|dt| {
        let dt = dt.trim();
        // Skip empty or invalid dates
        if dt.is_empty() || !dt.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return None;
        }
        // Try to parse "YYYY-MM-DD HH:MM" format and convert to ISO 8601
        if dt.contains('T') {
            Some(dt.to_string()) // Already in ISO format
        } else {
            // Replace space with T and add seconds + timezone
            Some(dt.replace(' ', "T") + ":00Z")
        }
    });

    Ok(baram::embedding::IndexDocument {
        id: format!("{oid}_{aid}"),
        oid,
        aid,
        title,
        content: article_content,
        category,
        publisher,
        author,
        url,
        published_at: published_at_iso,
        crawled_at: chrono::Utc::now().to_rfc3339(),
        comment_count: None,
        embedding,
        chunk_index: None,
        chunk_text: None,
    })
}

async fn search(query: String, k: usize, threshold: Option<f32>) -> Result<()> {
    use baram::config::OpenSearchConfig;
    use baram::embedding::{SearchConfig, VectorStore};

    println!("Searching for: \"{query}\"");
    println!("================================");

    // Create OpenSearch client with default config
    let opensearch_config = OpenSearchConfig {
        url: std::env::var("OPENSEARCH_URL")
            .unwrap_or_else(|_| "http://localhost:9200".to_string()),
        index_name: std::env::var("OPENSEARCH_INDEX")
            .unwrap_or_else(|_| "baram-articles".to_string()),
        username: std::env::var("OPENSEARCH_USER").ok(),
        password: std::env::var("OPENSEARCH_PASSWORD").ok(),
    };

    let store = VectorStore::new(&opensearch_config).context("Failed to connect to OpenSearch")?;

    // Check if index exists
    if !store.index_exists().await? {
        println!("Index '{}' does not exist.", opensearch_config.index_name);
        println!("Run 'baram index' first to create and populate the index.");
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
    use chrono::Utc;

    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        anyhow::bail!("Input path does not exist: {input}");
    }

    // Collect markdown files
    let mut articles: Vec<ParsedArticle> = Vec::new();

    if input_path.is_dir() {
        let entries: Vec<_> = std::fs::read_dir(&input_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .collect();

        println!("Found {} markdown files", entries.len());

        for entry in entries {
            let path = entry.path();
            match parse_markdown_to_article(&path) {
                Ok(article) => articles.push(article),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to parse markdown");
                }
            }
        }
    } else {
        articles.push(parse_markdown_to_article(&input_path)?);
    }

    if articles.is_empty() {
        println!("No articles to process.");
        return Ok(());
    }

    println!("Processing {} articles for ontology extraction...", articles.len());

    // Extract ontology from each article
    let extractor = RelationExtractor::new();
    let mut all_stores: Vec<TripleStore> = Vec::new();
    let mut total_entities = 0;
    let mut total_relations = 0;

    for (idx, article) in articles.iter().enumerate() {
        print!("\r  Processing {}/{} articles...", idx + 1, articles.len());
        std::io::Write::flush(&mut std::io::stdout())?;

        let result = extractor.extract_from_article(article);
        total_entities += result.entities.len();
        total_relations += result.relations.len();

        let store = TripleStore::from_extraction(&result, &article.title);
        all_stores.push(store);
    }
    println!();

    println!("Extraction complete:");
    println!("  Total entities: {total_entities}");
    println!("  Total relations: {total_relations}");

    // Combine all stores and export
    let combined_output = match format.to_lowercase().as_str() {
        "json" | "json-ld" => {
            let combined: Vec<_> = all_stores.iter().map(|s| {
                serde_json::json!({
                    "article_id": s.article_id,
                    "article_title": s.article_title,
                    "extracted_at": s.extracted_at,
                    "triples": s.triples,
                    "stats": s.stats,
                })
            }).collect();
            serde_json::to_string_pretty(&combined)?
        }
        "turtle" | "ttl" => {
            let mut output = String::new();
            output.push_str("@prefix schema: <https://schema.org/> .\n");
            output.push_str("@prefix baram: <https://baram.example.org/ontology/> .\n");
            output.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

            for store in &all_stores {
                output.push_str(&format!("# Article: {}\n", store.article_title));
                output.push_str(&store.to_turtle());
                output.push('\n');
            }
            output
        }
        "rdf" | "rdf-xml" => {
            let mut output = String::new();
            output.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
            output.push('\n');
            output.push_str(r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#""#);
            output.push_str(r#" xmlns:schema="https://schema.org/""#);
            output.push_str(r#" xmlns:baram="https://baram.example.org/ontology/">"#);
            output.push('\n');

            for store in &all_stores {
                for triple in &store.triples {
                    output.push_str(&format!(
                        "  <rdf:Description rdf:about=\"{}\">\n",
                        triple.subject_id
                    ));
                    output.push_str(&format!(
                        "    <{}>{}</{}>\n",
                        triple.predicate, triple.object, triple.predicate
                    ));
                    output.push_str("  </rdf:Description>\n");
                }
            }
            output.push_str("</rdf:RDF>\n");
            output
        }
        _ => anyhow::bail!("Unsupported format: {format}. Use json, turtle, or rdf."),
    };

    // Write output
    if let Some(output_path) = output {
        std::fs::write(&output_path, &combined_output)?;
        println!("Output written to: {output_path}");
    } else {
        println!("\n{combined_output}");
    }

    Ok(())
}

/// Parse markdown file to ParsedArticle for ontology extraction
fn parse_markdown_to_article(path: &std::path::Path) -> Result<ParsedArticle> {
    use chrono::Utc;

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let lines: Vec<&str> = content.lines().collect();

    // Extract title
    let title = lines
        .iter()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    // Extract metadata from frontmatter
    let mut oid = String::new();
    let mut aid = String::new();
    let mut category = String::new();
    let mut publisher = None;
    let mut author = None;
    let mut url = String::new();
    let mut published_at = None;

    let mut frontmatter_delim_count = 0;
    let mut in_metadata = false;
    let mut body_lines = Vec::new();

    for line in &lines {
        if line.starts_with("---") {
            if frontmatter_delim_count < 2 {
                frontmatter_delim_count += 1;
                in_metadata = frontmatter_delim_count == 1;
                continue;
            }
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
                    _ => {}
                }
            }
        } else if frontmatter_delim_count >= 2 && !line.starts_with('#') {
            body_lines.push(*line);
        }
    }

    let article_content = body_lines.join("\n");

    Ok(ParsedArticle {
        oid,
        aid,
        title,
        content: article_content,
        url,
        category,
        publisher,
        author,
        published_at,
        crawled_at: Utc::now(),
        content_hash: None,
    })
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

    axum::serve(listener, app).await.context("Server error")?;

    Ok(())
}

/// Root handler - welcome message
async fn root_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "baram Embedding Server",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "health": "GET /health",
            "embed": "POST /embed",
            "batch_embed": "POST /embed/batch"
        }
    }))
}

/// Health check handler
async fn health_handler(State(state): State<Arc<EmbeddingServerState>>) -> Json<HealthResponse> {
    let ready = state.ready.load(std::sync::atomic::Ordering::Relaxed);
    Json(HealthResponse {
        status: if ready {
            "healthy".to_string()
        } else {
            "loading".to_string()
        },
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
            Ok(Json(EmbedResponse {
                embedding,
                dimension,
            }))
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

// ============================================================================
// Distributed Crawler Implementation
// ============================================================================

/// Configuration parameters for distributed crawler
struct DistributedCrawlerParams {
    instance: String,
    coordinator: String,
    database: String,
    heartbeat_interval: u64,
    rps: f64,
    output: String,
    with_comments: bool,
    once: bool,
}

/// Start the distributed crawler
async fn distributed_crawler(params: DistributedCrawlerParams) -> Result<()> {
    let DistributedCrawlerParams {
        instance,
        coordinator,
        database,
        heartbeat_interval,
        rps,
        output,
        with_comments,
        once,
    } = params;
    println!("Starting Distributed Crawler");
    println!("============================");
    println!("  Instance ID: {instance}");
    println!("  Coordinator: {coordinator}");
    println!("  Database: {}...***", &database[..20.min(database.len())]);
    println!("  Heartbeat: {heartbeat_interval}s");
    println!("  Rate limit: {rps} req/s");
    println!("  Output: {output}");
    println!("  Comments: {with_comments}");
    println!("  Run once: {once}");
    println!();

    // Parse instance ID
    let instance_id = CrawlerInstance::from_id(&instance)
        .map_err(|_| anyhow::anyhow!("Invalid instance ID: {instance}. Valid: main, sub1, sub2"))?;

    // Create instance config
    let config = InstanceConfig::builder()
        .instance_id(instance_id)
        .coordinator_url(&coordinator)
        .database_url(&database)
        .heartbeat_interval_secs(heartbeat_interval)
        .requests_per_second(rps)
        .output_dir(&output)
        .include_comments(with_comments)
        .build()
        .context("Failed to build instance config")?;

    println!("{}", config.display());
    println!();

    // Create distributed runner with deduplication
    let runner = DistributedRunner::with_dedup(config)
        .await
        .context("Failed to create distributed runner")?;

    if once {
        // Run once mode: execute current slot and exit
        println!("Running in 'once' mode - executing current slot...");

        if let Some(slot) = runner
            .check_current_slot()
            .await
            .context("Failed to check current slot")?
        {
            println!(
                "Current slot: hour {} with categories {:?}",
                slot.hour, slot.categories
            );

            let result = runner.run_slot(&slot).await.context("Failed to run slot")?;

            println!("\nSlot Execution Complete");
            println!("=======================");
            println!("Hour: {}", result.hour);
            println!("Articles crawled: {}", result.articles_crawled);
            println!("Errors: {}", result.errors);
            println!("Categories: {:?}", result.categories);
            println!("Success rate: {:.1}%", result.success_rate() * 100.0);
        } else {
            println!("This instance is not scheduled for the current hour.");
            println!(
                "Use --instance to specify a different instance or wait for the scheduled slot."
            );
        }
    } else {
        // Continuous mode: start background tasks
        println!("Starting continuous distributed crawling...");
        println!("Press Ctrl+C to stop.\n");

        // Start the runner
        let handle = runner
            .start()
            .await
            .context("Failed to start distributed runner")?;

        // Get list of assigned slots for today
        match runner.get_my_slots().await {
            Ok(slots) => {
                if slots.is_empty() {
                    println!("No slots assigned for today.");
                } else {
                    println!("Assigned slots for today:");
                    for slot in &slots {
                        println!("  Hour {}: {:?}", slot.hour, slot.categories);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get assigned slots: {}", e);
            }
        }
        println!();

        // Wait for shutdown signal
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                println!("\nShutdown signal received, stopping...");
                handle.shutdown().await;
            }
            Err(e) => {
                tracing::error!("Failed to wait for Ctrl+C: {}", e);
            }
        }
    }

    println!("Distributed crawler stopped.");
    Ok(())
}

// ============================================================================
// Coordinator Server Implementation
// ============================================================================

/// Configuration parameters for coordinator server
struct CoordinatorParams {
    host: String,
    port: u16,
    heartbeat_timeout: u64,
    heartbeat_interval: u64,
    max_instances: usize,
    schedule_cache: Option<String>,
    enable_cors: bool,
    enable_logging: bool,
}

/// Start the coordinator server
async fn coordinator_server(params: CoordinatorParams) -> Result<()> {
    let CoordinatorParams {
        host,
        port,
        heartbeat_timeout,
        heartbeat_interval,
        max_instances,
        schedule_cache,
        enable_cors,
        enable_logging,
    } = params;

    println!("Starting Coordinator Server");
    println!("===========================");
    println!("  Host: {host}");
    println!("  Port: {port}");
    println!("  Heartbeat Timeout: {heartbeat_timeout}s");
    println!("  Heartbeat Interval: {heartbeat_interval}s");
    println!("  Max Instances: {max_instances}");
    println!(
        "  CORS: {}",
        if enable_cors { "enabled" } else { "disabled" }
    );
    println!(
        "  Request Logging: {}",
        if enable_logging {
            "enabled"
        } else {
            "disabled"
        }
    );
    if let Some(ref cache) = schedule_cache {
        println!("  Schedule Cache: {cache}");
    }
    println!();

    // Build bind address
    let bind_address = format!("{host}:{port}")
        .parse()
        .context("Invalid bind address")?;

    // Create coordinator configuration
    let config = CoordinatorConfig::builder()
        .bind_address(bind_address)
        .heartbeat_timeout_secs(heartbeat_timeout)
        .heartbeat_interval_secs(heartbeat_interval)
        .max_instances(max_instances)
        .enable_cors(enable_cors)
        .enable_request_logging(enable_logging);

    let config = if let Some(cache_path) = schedule_cache {
        config.schedule_cache_path(cache_path).build()?
    } else {
        config.build()?
    };

    // Create and start server
    let server = CoordinatorServer::new(config).context("Failed to create coordinator server")?;

    println!("{}", server.info().display());
    println!();
    println!("API Endpoints:");
    println!("  GET  /api/health              - Health check");
    println!("  GET  /metrics                 - Prometheus metrics endpoint");
    println!("  GET  /api/schedule/today      - Get today's schedule");
    println!("  GET  /api/schedule/tomorrow   - Get tomorrow's schedule");
    println!("  GET  /api/schedule/:date      - Get schedule by date (YYYY-MM-DD)");
    println!("  GET  /api/instances           - List all instances");
    println!("  GET  /api/instances/:id       - Get instance by ID");
    println!("  POST /api/instances/register  - Register new instance");
    println!("  POST /api/instances/heartbeat - Send heartbeat");
    println!("  GET  /api/stats               - Get coordinator stats");
    println!();
    println!("Coordinator server listening on http://{bind_address}");
    println!("Press Ctrl+C to stop.\n");

    // Start with graceful shutdown
    server
        .start_with_shutdown(async {
            match tokio::signal::ctrl_c().await {
                Ok(()) => {
                    tracing::info!("Shutdown signal received");
                }
                Err(e) => {
                    tracing::error!("Failed to wait for Ctrl+C: {}", e);
                }
            }
        })
        .await?;

    println!("Coordinator server stopped.");
    Ok(())
}
