use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use baram::config::Config;
use baram::i18n;

// Initialize rust-i18n for the binary crate
rust_i18n::i18n!("locales", fallback = "en");

mod commands;

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

        /// Only index files modified after this datetime (YYYY-MM-DD or YYYY-MM-DDTHH:MM:SS)
        #[arg(long)]
        since: Option<String>,
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

        /// Use LLM for Said relation extraction (requires Ollama)
        #[arg(long, default_value = "false")]
        llm: bool,

        /// Maximum concurrent LLM requests
        #[arg(long, default_value = "4")]
        max_concurrent: usize,
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
    // Initialize i18n from environment variable (BARAM_LANG)
    i18n::init_from_env();

    let cli = Cli::parse();

    // Initialize tracing/logging
    setup_tracing(&cli.log_format, cli.verbose)?;

    tracing::info!("{}", rust_i18n::t!("cli.app.starting"));

    // Load config
    let config = if cli.config.exists() {
        Config::from_file(&cli.config)?
    } else {
        tracing::warn!(
            path = %cli.config.display(),
            "{}",
            rust_i18n::t!("cli.config.not_found")
        );
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
            commands::crawl(
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
            since,
        } => {
            tracing::info!(
                input = %input,
                batch_size = %batch_size,
                force = %force,
                since = ?since,
                "Starting index command"
            );
            commands::index(input, batch_size, force, since).await?;
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
            commands::search(query, k, threshold).await?;
        }

        Commands::Ontology {
            input,
            format,
            output,
            llm,
            max_concurrent,
        } => {
            tracing::info!(
                input = %input,
                format = %format,
                output = ?output,
                llm = llm,
                max_concurrent = max_concurrent,
                "Starting ontology command"
            );
            commands::ontology(input, format, output, llm, max_concurrent).await?;
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
            commands::resume(checkpoint, max_articles, output).await?;
        }

        Commands::Stats { database } => {
            commands::stats(database)?;
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
            commands::embedding_server(host, port, model, max_seq_length, batch_size, use_gpu)
                .await?;
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
            commands::distributed_crawler(commands::DistributedCrawlerParams {
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
            commands::coordinator_server(commands::CoordinatorParams {
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

    tracing::info!("{}", rust_i18n::t!("cli.app.completed"));
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
