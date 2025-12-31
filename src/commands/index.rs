use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use baram::config::OpenSearchConfig;
use baram::embedding::VectorStore;

pub async fn index(input: String, batch_size: usize, force: bool) -> Result<()> {
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
        .json(&EmbedRequest {
            text: &truncated_text,
        })
        .send()
        .await
        .context("Failed to send embedding request")?;

    let embed_response: EmbedResponse = response
        .json()
        .await
        .context("Failed to parse embedding response")?;

    Ok(embed_response.embedding)
}

pub fn parse_markdown_to_document(
    path: &std::path::Path,
) -> Result<baram::embedding::IndexDocument> {
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
