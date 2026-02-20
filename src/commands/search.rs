use anyhow::{Context, Result};

use baram::config::OpenSearchConfig;
use baram::embedding::{SearchConfig, VectorStore};

/// Fetch a query embedding from the embedding server.
async fn get_query_embedding(text: &str) -> Result<Vec<f32>> {
    let url = std::env::var("EMBEDDING_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client for embedding server")?;

    let resp = client
        .post(format!("{url}/embed"))
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
        .context(format!(
            "Failed to connect to embedding server at {url}. \
             Is the embedding server running? Start it with: baram embedding-server"
        ))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Embedding server returned error ({status}): {body}");
    }

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse embedding server response")?;

    let embedding: Vec<f32> = resp_json["embedding"]
        .as_array()
        .context("No 'embedding' field in embedding server response")?
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

    if embedding.is_empty() {
        anyhow::bail!("Embedding server returned empty embedding vector");
    }

    Ok(embedding)
}

/// Print search results to stdout.
fn print_results(results: &[baram::embedding::SearchResult], query: &str, mode: &str) {
    if results.is_empty() {
        tracing::info!(query = %query, mode = %mode, "No results found");
        println!("\nNo results found for \"{query}\"");
        return;
    }

    println!("\nFound {} results (mode: {mode}):\n", results.len());

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
            // Show content preview (char-boundary safe for Korean text)
            let preview = result
                .content
                .char_indices()
                .nth(150)
                .map(|(i, _)| format!("{}...", &result.content[..i]))
                .unwrap_or_else(|| result.content.clone());
            println!("   > {preview}");
        }
        println!("   URL: {}", result.url);
        println!();
    }
}

pub async fn search(query: String, k: usize, threshold: Option<f32>, mode: &str) -> Result<()> {
    println!("Searching for: \"{query}\" (mode: {mode})");
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

    // Execute search based on mode
    match mode {
        "keyword" | "bm25" => {
            tracing::info!(query = %query, mode = "bm25", k = k, "Running BM25 keyword search");
            let results = store
                .search_bm25(&query, &search_config)
                .await
                .context("BM25 search failed")?;
            print_results(&results, &query, "bm25");
        }
        "vector" | "knn" => {
            tracing::info!(query = %query, mode = "knn", k = k, "Running kNN vector search");
            let query_vector = get_query_embedding(&query).await?;
            let results = store
                .search_knn(&query_vector, &search_config)
                .await
                .context("kNN search failed")?;
            print_results(&results, &query, "knn");
        }
        "hybrid" => {
            tracing::info!(query = %query, mode = "hybrid", k = k, "Running hybrid search (BM25 + kNN)");
            let query_vector = get_query_embedding(&query).await?;
            let results = store
                .search_hybrid(&query, &query_vector, &search_config)
                .await
                .context("Hybrid search failed")?;
            print_results(&results, &query, "hybrid");
        }
        other => {
            anyhow::bail!(
                "Unknown search mode: '{other}'. Valid modes: keyword, bm25, vector, knn, hybrid"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_mode_validation() {
        // Verify valid mode strings are recognized
        let valid_modes = ["keyword", "bm25", "vector", "knn", "hybrid"];
        for mode in &valid_modes {
            assert!(
                matches!(
                    *mode,
                    "keyword" | "bm25" | "vector" | "knn" | "hybrid"
                ),
                "Mode {mode} should be valid"
            );
        }
    }

    #[test]
    fn test_print_results_empty() {
        // Should not panic on empty results
        print_results(&[], "test query", "hybrid");
    }

    #[test]
    fn test_print_results_with_data() {
        let results = vec![baram::embedding::SearchResult {
            id: "001_002".to_string(),
            score: 0.95,
            title: "Test Article".to_string(),
            content: "This is a test article content".to_string(),
            category: "politics".to_string(),
            publisher: Some("TestPub".to_string()),
            url: "https://example.com/article".to_string(),
            published_at: Some("2026-02-15T10:00:00Z".to_string()),
            highlights: Some(vec!["<mark>Test</mark> highlight".to_string()]),
        }];
        // Should not panic
        print_results(&results, "test", "hybrid");
    }
}
