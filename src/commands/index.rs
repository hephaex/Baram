use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

use baram::config::OpenSearchConfig;
use baram::embedding::VectorStore;
use baram::storage::checkpoint::CheckpointManager;
use baram::utils::retry::{with_retry, RetryConfig};

#[derive(Serialize, Deserialize, Clone)]
struct IndexCheckpoint {
    last_processed_batch: usize,
    total_success: usize,
    total_failed: usize,
    processed_doc_ids: std::collections::HashSet<String>,
}

/// Extract document ID ({oid}_{aid}) from markdown filename.
///
/// Filename format: {oid}_{aid}_{sanitized_title}.md
/// Both oid and aid are purely numeric.
fn extract_doc_id_from_filename(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let mut parts = stem.splitn(3, '_');
    let oid = parts.next()?;
    let aid = parts.next()?;

    if oid.chars().all(|c| c.is_ascii_digit()) && aid.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("{oid}_{aid}"))
    } else {
        None
    }
}

pub async fn index(input: String, batch_size: usize, force: bool, since: Option<String>) -> Result<()> {
    use std::fs;

    println!("Indexing articles from: {input}");
    println!("================================");

    // Initialize checkpoint manager
    let checkpoint_dir = PathBuf::from("./checkpoints");
    let checkpoint_mgr = CheckpointManager::with_interval(&checkpoint_dir, 10)?;

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

    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        anyhow::bail!("Input path does not exist: {input}");
    }

    let checkpoint_name = format!("index_{}",
        input_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    );

    // Load checkpoint BEFORE file scanning (key optimization)
    let mut checkpoint_state: IndexCheckpoint = checkpoint_mgr
        .load(&checkpoint_name)?
        .unwrap_or(IndexCheckpoint {
            last_processed_batch: 0,
            total_success: 0,
            total_failed: 0,
            processed_doc_ids: std::collections::HashSet::new(),
        });

    if !index_exists {
        println!("Creating index '{}'...", opensearch_config.index_name);
        store
            .create_index(384)
            .await
            .context("Failed to create index")?;
        println!("Index created successfully.");
    } else if force {
        println!("Force reindex: deleting existing index...");
        store.delete_index().await?;
        store.create_index(384).await?;
        // Clear checkpoint for force reindex
        checkpoint_mgr.delete(&checkpoint_name)?;
        checkpoint_state = IndexCheckpoint {
            last_processed_batch: 0,
            total_success: 0,
            total_failed: 0,
            processed_doc_ids: std::collections::HashSet::new(),
        };
        println!("Index recreated, checkpoint cleared.");
    } else {
        println!("Index '{}' already exists.", opensearch_config.index_name);
    }

    // Parse --since filter
    let since_time: Option<SystemTime> = if let Some(since_str) = &since {
        let naive = if since_str.contains('T') {
            chrono::NaiveDateTime::parse_from_str(since_str, "%Y-%m-%dT%H:%M:%S")
                .context("Invalid --since format. Expected YYYY-MM-DDTHH:MM:SS")?
        } else {
            chrono::NaiveDate::parse_from_str(since_str, "%Y-%m-%d")
                .context("Invalid --since format. Expected YYYY-MM-DD")?
                .and_hms_opt(0, 0, 0)
                .unwrap()
        };
        let dt: chrono::DateTime<chrono::Utc> =
            chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc);
        Some(dt.into())
    } else {
        None
    };

    // Collect markdown files with pre-filtering
    let mut documents: Vec<baram::embedding::IndexDocument> = Vec::new();

    if input_path.is_dir() {
        let entries: Vec<_> = fs::read_dir(&input_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
            .collect();

        let total_files = entries.len();

        // Pre-filter by --since (file mtime)
        let time_filtered: Vec<_> = match &since_time {
            None => entries,
            Some(since) => {
                entries.into_iter()
                    .filter(|e| {
                        e.metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(|mtime| mtime >= *since)
                            .unwrap_or(true)
                    })
                    .collect()
            }
        };

        let after_time_filter = time_filtered.len();

        // Pre-filter by checkpoint: extract ID from filename, skip already indexed
        let unprocessed: Vec<_> = if checkpoint_state.processed_doc_ids.is_empty() {
            time_filtered
        } else {
            time_filtered.into_iter()
                .filter(|entry| {
                    match extract_doc_id_from_filename(&entry.path()) {
                        Some(id) => !checkpoint_state.processed_doc_ids.contains(&id),
                        None => true, // can't extract ID → parse it anyway
                    }
                })
                .collect()
        };

        let skipped_by_checkpoint = after_time_filter - unprocessed.len();

        println!(
            "Found {} markdown files ({} new, {} already indexed{})",
            total_files,
            unprocessed.len(),
            skipped_by_checkpoint,
            if since.is_some() {
                format!(", {} filtered by --since", total_files - after_time_filter)
            } else {
                String::new()
            }
        );

        if checkpoint_state.last_processed_batch > 0 {
            println!(
                "Resuming from checkpoint: {} documents previously indexed",
                checkpoint_state.processed_doc_ids.len()
            );
        }

        // Parallel file parsing with buffer_unordered
        {
            use futures::stream::{self, StreamExt};
            let paths: Vec<PathBuf> = unprocessed.iter().map(|e| e.path()).collect();
            let concurrency = std::thread::available_parallelism()
                .map(|n| n.get().min(8))
                .unwrap_or(4);

            let results: Vec<_> = stream::iter(paths)
                .map(|path| {
                    tokio::task::spawn_blocking(move || {
                        let res = parse_markdown_to_document(&path);
                        (path, res)
                    })
                })
                .buffer_unordered(concurrency)
                .collect()
                .await;

            for result in results {
                match result {
                    Ok((_, Ok(doc))) => documents.push(doc),
                    Ok((path, Err(e))) => {
                        tracing::warn!(path = %path.display(), error = %e, "Failed to parse markdown");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Parse task panicked");
                    }
                }
            }
        }
    } else {
        // Single file
        documents.push(parse_markdown_to_document(&input_path)?);
    }

    if documents.is_empty() {
        println!("No new documents to index.");
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
    let mut total_success = checkpoint_state.total_success;
    let mut total_failed = checkpoint_state.total_failed;
    let client = reqwest::Client::new();
    let retry_config = RetryConfig::with_delays(2, 1000, 5000);

    for (batch_num, batch) in documents.chunks(batch_size).enumerate() {
        let actual_batch_num = checkpoint_state.last_processed_batch + batch_num;
        print!(
            "\rProcessing batch {}/{}...",
            actual_batch_num + 1,
            checkpoint_state.last_processed_batch + documents.len().div_ceil(batch_size)
        );
        std::io::Write::flush(&mut std::io::stdout())?;

        // Generate embeddings in batch (single API call for entire batch)
        let batch_with_embeddings: Vec<baram::embedding::IndexDocument> = if use_embeddings {
            let texts: Vec<String> = batch.iter()
                .map(|doc| {
                    let text = format!("{} {}", doc.title, doc.content);
                    text.chars().take(2000).collect()
                })
                .collect();

            match with_retry(&retry_config, || async {
                generate_embeddings_batch(&client, &embedding_server_url, &texts).await
            }).await {
                Ok(embeddings) => {
                    batch.iter().zip(embeddings.into_iter())
                        .map(|(doc, emb)| {
                            let mut new_doc = doc.clone();
                            new_doc.embedding = emb;
                            new_doc
                        })
                        .collect()
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Batch embedding failed, using dummy embeddings");
                    batch.to_vec()
                }
            }
        } else {
            batch.to_vec()
        };

        // Bulk index with retry
        let result = with_retry(&retry_config, || async {
            store.bulk_index(&batch_with_embeddings).await
        }).await?;

        total_success += result.success;
        total_failed += result.failed;

        // Update checkpoint state
        for doc in batch {
            checkpoint_state.processed_doc_ids.insert(doc.id.clone());
        }
        checkpoint_state.last_processed_batch = actual_batch_num + 1;
        checkpoint_state.total_success = total_success;
        checkpoint_state.total_failed = total_failed;

        // Save checkpoint periodically
        if checkpoint_mgr.should_auto_save() {
            checkpoint_mgr.save(&checkpoint_name, &checkpoint_state)?;
            tracing::debug!("Checkpoint saved at batch {}", actual_batch_num + 1);
        }

        // Print errors if any
        if !result.errors.is_empty() {
            eprintln!("\nErrors in batch {}:", actual_batch_num + 1);
            for (i, err) in result.errors.iter().take(3).enumerate() {
                eprintln!("  {}: {}", i + 1, err);
            }
            if result.errors.len() > 3 {
                eprintln!("  ... and {} more errors", result.errors.len() - 3);
            }
        }
    }

    // Final checkpoint save
    checkpoint_mgr.save(&checkpoint_name, &checkpoint_state)?;

    println!("\n\nIndexing Complete");
    println!("=================");
    println!("Successful: {total_success}");
    println!("Failed: {total_failed}");

    // Refresh index
    store.refresh().await?;

    let count = store.count().await?;
    println!("Total documents in index: {count}");

    // Delete checkpoint on successful completion
    if total_failed == 0 {
        checkpoint_mgr.delete(&checkpoint_name)?;
        println!("Checkpoint deleted (all documents indexed successfully)");
    } else {
        println!("Checkpoint saved for retry of failed documents");
    }

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

/// Generate embeddings for a batch of texts using the batch API endpoint
async fn generate_embeddings_batch(
    client: &reqwest::Client,
    server_url: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    #[derive(Serialize)]
    struct BatchEmbedRequest<'a> {
        texts: &'a [String],
    }

    #[derive(Deserialize)]
    struct BatchEmbedResponse {
        embeddings: Vec<Vec<f32>>,
    }

    let response = client
        .post(format!("{server_url}/embed/batch"))
        .json(&BatchEmbedRequest { texts })
        .send()
        .await
        .context("Failed to send batch embedding request")?;

    let batch_response: BatchEmbedResponse = response
        .json()
        .await
        .context("Failed to parse batch embedding response")?;

    Ok(batch_response.embeddings)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_doc_id_standard_filename() {
        let path = std::path::PathBuf::from(
            "001_0015812889_강남구_국민권익위_청렴도_평가서.md"
        );
        assert_eq!(
            extract_doc_id_from_filename(&path),
            Some("001_0015812889".to_string())
        );
    }

    #[test]
    fn test_extract_doc_id_no_title() {
        let path = std::path::PathBuf::from("422_0000832799.md");
        assert_eq!(
            extract_doc_id_from_filename(&path),
            Some("422_0000832799".to_string())
        );
    }

    #[test]
    fn test_extract_doc_id_invalid_format() {
        let path = std::path::PathBuf::from("unknown.md");
        assert_eq!(extract_doc_id_from_filename(&path), None);
    }

    #[test]
    fn test_extract_doc_id_non_numeric_oid() {
        let path = std::path::PathBuf::from("abc_0015812889_title.md");
        assert_eq!(extract_doc_id_from_filename(&path), None);
    }

    #[test]
    fn test_extract_doc_id_three_digit_oid() {
        let path = std::path::PathBuf::from(
            "661_0000071158_강득구_지방선거_이후_합당이.md"
        );
        assert_eq!(
            extract_doc_id_from_filename(&path),
            Some("661_0000071158".to_string())
        );
    }
}
