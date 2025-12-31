use anyhow::{Context, Result};

use baram::config::OpenSearchConfig;
use baram::embedding::{SearchConfig, VectorStore};

pub async fn search(query: String, k: usize, threshold: Option<f32>) -> Result<()> {
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
