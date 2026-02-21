use anyhow::{Context, Result};
use std::path::Path;

use baram::clustering::{ClusterConfig, ClusterEngine, ClusterSummarizer};

/// Run the event clustering pipeline
pub async fn cluster(
    category: Option<String>,
    since: Option<String>,
    threshold: f64,
    min_size: usize,
    max_articles: usize,
    output: String,
    summarize: bool,
) -> Result<()> {
    println!("Event Clustering");
    println!("================");
    println!("  Similarity threshold: {threshold}");
    println!("  Min cluster size: {min_size}");
    if let Some(ref cat) = category {
        println!("  Category filter: {cat}");
    }
    if let Some(ref since_date) = since {
        println!("  Since: {since_date}");
    }
    if max_articles > 0 {
        println!("  Max articles: {max_articles}");
    }
    println!("  Output: {output}");
    println!("  Summarize (vLLM): {summarize}");
    println!();

    // Build clustering config
    let config = ClusterConfig {
        similarity_threshold: threshold,
        min_cluster_size: min_size,
        batch_size: 500,
        generate_summaries: summarize,
        category,
        since,
        max_articles,
        output_dir: output.clone(),
    };

    // Create clustering engine
    let engine = ClusterEngine::new(config).context("Failed to create clustering engine")?;

    // Run clustering
    let mut result = engine.run().await.context("Clustering failed")?;

    // Generate summaries if requested
    if summarize && !result.events.is_empty() {
        println!("Generating event summaries with vLLM...");
        match ClusterSummarizer::new() {
            Ok(summarizer) => {
                if summarizer.is_available().await {
                    let success = summarizer
                        .summarize_all(&mut result.events)
                        .await
                        .context("Summary generation failed")?;
                    println!("  Generated {success}/{} summaries", result.events.len());
                } else {
                    tracing::warn!("vLLM service not available, skipping summaries");
                    println!("  Warning: vLLM service not available, skipping summaries");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create summarizer");
                println!("  Warning: Failed to create summarizer: {e}");
            }
        }
        println!();
    }

    // Save results
    let output_path = Path::new(&output);
    tokio::fs::create_dir_all(output_path)
        .await
        .context(format!("Failed to create output directory: {output}"))?;

    // Save full output as JSON
    let output_file = output_path.join("clusters.json");
    let json = serde_json::to_string_pretty(&result)
        .context("Failed to serialize cluster output")?;
    tokio::fs::write(&output_file, &json)
        .await
        .context(format!(
            "Failed to write output file: {}",
            output_file.display()
        ))?;

    // Print summary
    println!("Results");
    println!("=======");
    println!(
        "  Total articles: {}",
        result.metadata.total_articles
    );
    println!(
        "  Clusters formed: {}",
        result.metadata.total_clusters
    );
    println!(
        "  Clustered articles: {}",
        result.metadata.clustered_articles
    );
    println!(
        "  Unclustered: {}",
        result.metadata.unclustered_articles
    );
    println!(
        "  Processing time: {:.1}s",
        result.metadata.duration_secs
    );
    println!();

    // Print top clusters
    let top_n = 10.min(result.events.len());
    if top_n > 0 {
        println!("Top {top_n} Events:");
        println!("{}", "-".repeat(80));
        for (i, event) in result.events.iter().take(top_n).enumerate() {
            println!(
                "{}. [{}] {} ({} articles, sim: {:.2})",
                i + 1,
                event.category,
                event.title,
                event.article_count,
                event.avg_similarity
            );
            if !event.summary.is_empty() {
                println!("   {}", event.summary);
            }
            if let (Some(first), Some(last)) = (&event.first_seen, &event.last_updated) {
                println!("   Period: {first} ~ {last}");
            }
            println!();
        }
    }

    println!(
        "Output saved to: {}",
        output_file.display()
    );
    tracing::info!(
        clusters = result.metadata.total_clusters,
        articles = result.metadata.total_articles,
        file = %output_file.display(),
        "Clustering results saved"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use baram::clustering::{ClusterConfig, ClusterOutput, ClusterMetadata};

    #[test]
    fn test_cluster_output_deserialization() {
        let json = serde_json::json!({
            "metadata": {
                "total_articles": 100,
                "total_clusters": 5,
                "clustered_articles": 80,
                "unclustered_articles": 20,
                "similarity_threshold": 0.75,
                "min_cluster_size": 2,
                "created_at": "2026-02-21T10:00:00Z",
                "duration_secs": 5.0
            },
            "events": []
        });

        let output: ClusterOutput =
            serde_json::from_value(json).expect("should deserialize");
        assert_eq!(output.metadata.total_articles, 100);
        assert_eq!(output.metadata.total_clusters, 5);
    }

    #[test]
    fn test_config_construction() {
        let config = ClusterConfig {
            similarity_threshold: 0.8,
            min_cluster_size: 3,
            category: Some("politics".to_string()),
            since: Some("2026-02-01".to_string()),
            ..Default::default()
        };

        assert!((config.similarity_threshold - 0.8).abs() < f64::EPSILON);
        assert_eq!(config.min_cluster_size, 3);
        assert_eq!(config.category.as_deref(), Some("politics"));
    }
}
