use anyhow::{Context, Result};
use std::path::PathBuf;

use baram::llm::{LlmBackend, LlmClient};
use baram::models::ParsedArticle;
use baram::ontology::{RelationExtractor, RelationType, TripleStore};

pub async fn ontology(
    input: String,
    format: String,
    output: Option<String>,
    use_llm: bool,
) -> Result<()> {
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

    println!(
        "Processing {} articles for ontology extraction...",
        articles.len()
    );

    // Initialize LLM client if requested
    let llm_client = if use_llm {
        match LlmClient::from_env() {
            Ok(client) => {
                let backend_name = match client.backend() {
                    LlmBackend::Vllm => "vLLM",
                    LlmBackend::Ollama => "Ollama",
                };
                if client.is_available().await {
                    println!("LLM extraction enabled ({backend_name})");
                    Some(client)
                } else {
                    println!("Warning: {backend_name} not available, falling back to regex-only extraction");
                    None
                }
            }
            Err(e) => {
                println!("Warning: Failed to initialize LLM client: {e}");
                None
            }
        }
    } else {
        None
    };

    // Extract ontology from each article
    let extractor = RelationExtractor::new();
    let mut all_stores: Vec<TripleStore> = Vec::new();
    let mut total_entities = 0;
    let mut total_relations = 0;
    let mut total_said_relations = 0;
    let mut failed_llm_batches = 0;
    let mut failed_articles = Vec::new();

    // Batch size for LLM processing
    const LLM_BATCH_SIZE: usize = 2;

    // Pre-extract LLM Said relations in batches for better performance
    let mut llm_results: std::collections::HashMap<String, Vec<baram::llm::SaidRelation>> =
        std::collections::HashMap::new();

    if let Some(ref client) = llm_client {
        let total_batches = (articles.len() + LLM_BATCH_SIZE - 1) / LLM_BATCH_SIZE;
        println!("Processing {} batches for LLM extraction...", total_batches);

        for (batch_idx, chunk) in articles.chunks(LLM_BATCH_SIZE).enumerate() {
            print!(
                "\r  LLM batch {}/{} ({} articles)...",
                batch_idx + 1,
                total_batches,
                chunk.len()
            );
            std::io::Write::flush(&mut std::io::stdout())?;

            // Prepare batch
            let batch: Vec<baram::llm::ArticleInfo> = chunk
                .iter()
                .map(|a| baram::llm::ArticleInfo {
                    id: a.id().to_string(),
                    title: a.title.clone(),
                    content: a.content.clone(),
                })
                .collect();

            // Extract Said relations for batch with retry
            use baram::utils::retry::{with_retry_if, RetryConfig};

            let retry_config = RetryConfig::with_delays(2, 2000, 10_000);
            let client_ref = client;
            let batch_clone = batch.clone();

            match with_retry_if(
                &retry_config,
                || {
                    let batch = batch_clone.clone();
                    async move { client_ref.extract_said_batch(&batch).await }
                },
                |e| {
                    // Retry on network/timeout errors, not on validation errors
                    let err_str = e.to_string().to_lowercase();
                    err_str.contains("timeout")
                        || err_str.contains("connection")
                        || err_str.contains("network")
                        || err_str.contains("unavailable")
                },
            ).await {
                Ok(batch_results) => {
                    for (id, relations) in batch_results {
                        total_said_relations += relations.len();
                        llm_results.insert(id, relations);
                    }
                }
                Err(e) => {
                    failed_llm_batches += 1;
                    tracing::warn!(
                        batch = batch_idx,
                        error = %e,
                        "LLM batch extraction failed after retries, continuing with next batch"
                    );
                    // Record which articles failed for final report
                    for article_info in &batch_clone {
                        failed_articles.push((article_info.id.clone(), "LLM extraction failed"));
                    }
                }
            }
        }
        println!(
            "\n  LLM extraction: {} Said relations found",
            total_said_relations
        );
    }

    // Now process articles with regex extraction + merge LLM results
    let mut successful_articles = 0;
    for (idx, article) in articles.iter().enumerate() {
        print!(
            "\r  Building ontology {}/{} articles...",
            idx + 1,
            articles.len()
        );
        std::io::Write::flush(&mut std::io::stdout())?;

        // Regex-based extraction with error handling
        let result = match std::panic::catch_unwind(|| extractor.extract_from_article(article)) {
            Ok(mut result) => {
                // Merge LLM Said relations if available
                if let Some(said_relations) = llm_results.get(&article.id()) {
                    for said in said_relations {
                        let relation = baram::ontology::ExtractedRelation {
                            subject: said.speaker.clone(),
                            subject_type: baram::ontology::EntityType::Person,
                            predicate: RelationType::Said,
                            object: said.content.clone(),
                            object_type: baram::ontology::EntityType::Other,
                            confidence: said.confidence,
                            evidence: said.evidence.clone(),
                            verified: true,
                        };
                        result.relations.push(relation);
                    }
                }
                Some(result)
            }
            Err(e) => {
                tracing::warn!(
                    article_id = %article.id(),
                    error = ?e,
                    "Regex extraction panicked for article, skipping"
                );
                failed_articles.push((article.id(), "Regex extraction panicked"));
                None
            }
        };

        if let Some(result) = result {
            total_entities += result.entities.len();
            total_relations += result.relations.len();

            let store = TripleStore::from_extraction(&result, &article.title);
            all_stores.push(store);
            successful_articles += 1;
        }
    }
    println!();

    println!("Extraction complete:");
    println!("  Successful articles: {}/{}", successful_articles, articles.len());
    println!("  Total entities: {total_entities}");
    println!("  Total relations: {total_relations}");
    if total_said_relations > 0 {
        println!("  Said relations (LLM): {total_said_relations}");
    }

    // Report failures if any
    if !failed_articles.is_empty() {
        println!("\n  Failed articles: {}", failed_articles.len());
        for (id, reason) in &failed_articles {
            tracing::warn!(article_id = %id, reason = %reason, "Article processing failed");
        }
    }
    if failed_llm_batches > 0 {
        println!("  Failed LLM batches: {}", failed_llm_batches);
    }

    // Combine all stores and export
    let combined_output = match format.to_lowercase().as_str() {
        "json" | "json-ld" => {
            let combined: Vec<_> = all_stores
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "article_id": s.article_id,
                        "article_title": s.article_title,
                        "extracted_at": s.extracted_at,
                        "triples": s.triples,
                        "stats": s.stats,
                    })
                })
                .collect();
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
pub fn parse_markdown_to_article(path: &std::path::Path) -> Result<ParsedArticle> {
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
    let published_at = None;

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
