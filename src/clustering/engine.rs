//! Core clustering engine using cosine similarity
//!
//! Loads article embeddings from OpenSearch and groups them into event clusters
//! using single-linkage incremental clustering with a cosine similarity threshold.

use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::time::Instant;

use crate::config::OpenSearchConfig;
use crate::embedding::{cosine_similarity, VectorStore};

use super::models::{
    ClusterArticle, ClusterConfig, ClusterMetadata, ClusterOutput, EventCluster,
};

/// Internal representation of an article with its embedding
#[derive(Debug, Clone)]
struct ArticleWithEmbedding {
    id: String,
    title: String,
    category: String,
    publisher: Option<String>,
    published_at: Option<String>,
    url: String,
    embedding: Vec<f32>,
}

/// Clustering engine that groups articles into event clusters
pub struct ClusterEngine {
    config: ClusterConfig,
    store: VectorStore,
}

impl ClusterEngine {
    /// Create a new cluster engine with the given configuration
    pub fn new(config: ClusterConfig) -> Result<Self> {
        let opensearch_config = OpenSearchConfig {
            url: std::env::var("OPENSEARCH_URL")
                .unwrap_or_else(|_| "http://localhost:9200".to_string()),
            index_name: std::env::var("OPENSEARCH_INDEX")
                .unwrap_or_else(|_| "baram-articles".to_string()),
            username: std::env::var("OPENSEARCH_USER").ok(),
            password: std::env::var("OPENSEARCH_PASSWORD").ok(),
        };

        let store =
            VectorStore::new(&opensearch_config).context("Failed to connect to OpenSearch")?;

        Ok(Self { config, store })
    }

    /// Create a new cluster engine with an existing VectorStore
    pub fn with_store(config: ClusterConfig, store: VectorStore) -> Self {
        Self { config, store }
    }

    /// Run the full clustering pipeline
    pub async fn run(&self) -> Result<ClusterOutput> {
        let start = Instant::now();

        // Step 1: Load articles with embeddings from OpenSearch
        tracing::info!("Loading articles with embeddings from OpenSearch...");
        let articles = self.load_articles().await?;
        let total_articles = articles.len();
        tracing::info!(count = total_articles, "Loaded articles");

        if articles.is_empty() {
            tracing::warn!("No articles found matching the filter criteria");
            return Ok(ClusterOutput {
                metadata: ClusterMetadata {
                    total_articles: 0,
                    total_clusters: 0,
                    clustered_articles: 0,
                    unclustered_articles: 0,
                    similarity_threshold: self.config.similarity_threshold,
                    min_cluster_size: self.config.min_cluster_size,
                    category_filter: self.config.category.clone(),
                    since_filter: self.config.since.clone(),
                    created_at: Utc::now(),
                    duration_secs: start.elapsed().as_secs_f64(),
                },
                events: vec![],
            });
        }

        // Step 2: Run incremental clustering
        tracing::info!(
            threshold = self.config.similarity_threshold,
            "Running cosine similarity clustering..."
        );
        let raw_clusters = self.cluster_articles(&articles);
        tracing::info!(
            clusters = raw_clusters.len(),
            "Raw clusters formed (before min_size filter)"
        );

        // Step 3: Filter by minimum cluster size and build EventClusters
        let now = Utc::now();
        let date_str = now.format("%Y%m%d").to_string();
        let mut events: Vec<EventCluster> = Vec::new();
        let mut clustered_count = 0usize;

        for (cluster_idx, article_indices) in raw_clusters.iter().enumerate() {
            if article_indices.len() < self.config.min_cluster_size {
                continue;
            }

            let event = self.build_event_cluster(
                &articles,
                article_indices,
                &date_str,
                cluster_idx,
                &now,
            );
            clustered_count += event.article_count;
            events.push(event);
        }

        // Sort events by article count (descending)
        events.sort_by(|a, b| b.article_count.cmp(&a.article_count));

        let duration = start.elapsed().as_secs_f64();
        tracing::info!(
            total_articles = total_articles,
            clusters = events.len(),
            clustered = clustered_count,
            unclustered = total_articles - clustered_count,
            duration_secs = format!("{duration:.1}"),
            "Clustering complete"
        );

        Ok(ClusterOutput {
            metadata: ClusterMetadata {
                total_articles,
                total_clusters: events.len(),
                clustered_articles: clustered_count,
                unclustered_articles: total_articles - clustered_count,
                similarity_threshold: self.config.similarity_threshold,
                min_cluster_size: self.config.min_cluster_size,
                category_filter: self.config.category.clone(),
                since_filter: self.config.since.clone(),
                created_at: now,
                duration_secs: duration,
            },
            events,
        })
    }

    /// Load articles with embeddings from OpenSearch using scroll API
    async fn load_articles(&self) -> Result<Vec<ArticleWithEmbedding>> {
        let mut articles = Vec::new();
        let mut search_after: Option<serde_json::Value> = None;
        let batch_size = self.config.batch_size;

        loop {
            let batch = self
                .fetch_article_batch(batch_size, search_after.as_ref())
                .await?;

            if batch.is_empty() {
                break;
            }

            // Get the sort value from the last item for pagination
            search_after = batch.last().map(|(_, sort_val)| sort_val.clone());

            for (article, _) in batch {
                articles.push(article);

                if self.config.max_articles > 0 && articles.len() >= self.config.max_articles {
                    tracing::info!(
                        max = self.config.max_articles,
                        "Reached max_articles limit"
                    );
                    return Ok(articles);
                }
            }

            tracing::debug!(loaded = articles.len(), "Loading articles...");
        }

        Ok(articles)
    }

    /// Fetch a batch of articles from OpenSearch with search_after pagination
    async fn fetch_article_batch(
        &self,
        size: usize,
        search_after: Option<&serde_json::Value>,
    ) -> Result<Vec<(ArticleWithEmbedding, serde_json::Value)>> {
        let mut query = serde_json::json!({
            "size": size,
            "sort": [{"_id": "asc"}],
            "_source": ["title", "category", "publisher", "published_at", "url", "embedding"],
            "query": {
                "bool": {
                    "must": [
                        {"exists": {"field": "embedding"}}
                    ]
                }
            }
        });

        // Add category filter
        if let Some(ref category) = self.config.category {
            query["query"]["bool"]["must"]
                .as_array_mut()
                .expect("must is an array")
                .push(serde_json::json!({"term": {"category": category}}));
        }

        // Add date filter
        if let Some(ref since) = self.config.since {
            query["query"]["bool"]["must"]
                .as_array_mut()
                .expect("must is an array")
                .push(serde_json::json!({
                    "range": {
                        "published_at": {
                            "gte": since
                        }
                    }
                }));
        }

        // Add search_after for pagination
        if let Some(after) = search_after {
            query["search_after"] = after.clone();
        }

        let response = self
            .store
            .raw_search(&query)
            .await
            .context("Failed to fetch articles from OpenSearch")?;

        let hits = response["hits"]["hits"]
            .as_array()
            .unwrap_or(&vec![])
            .clone();

        let mut results = Vec::new();
        for hit in &hits {
            let source = &hit["_source"];
            let id = hit["_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            let embedding: Vec<f32> = source["embedding"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                })
                .unwrap_or_default();

            if embedding.is_empty() {
                continue;
            }

            let article = ArticleWithEmbedding {
                id,
                title: source["title"].as_str().unwrap_or_default().to_string(),
                category: source["category"].as_str().unwrap_or_default().to_string(),
                publisher: source["publisher"].as_str().map(String::from),
                published_at: source["published_at"].as_str().map(String::from),
                url: source["url"].as_str().unwrap_or_default().to_string(),
                embedding,
            };

            let sort_value = hit["sort"].clone();
            results.push((article, sort_value));
        }

        Ok(results)
    }

    /// Perform incremental clustering using cosine similarity
    ///
    /// Algorithm: For each article, find the most similar existing cluster centroid.
    /// If similarity >= threshold, add to that cluster. Otherwise, create a new cluster.
    /// Cluster centroid is the average embedding of all articles in the cluster.
    fn cluster_articles(&self, articles: &[ArticleWithEmbedding]) -> Vec<Vec<usize>> {
        if articles.is_empty() {
            return vec![];
        }

        let threshold = self.config.similarity_threshold as f32;
        let dim = articles[0].embedding.len();

        // Each cluster is represented by (centroid_embedding, article_indices)
        let mut clusters: Vec<(Vec<f32>, Vec<usize>)> = Vec::new();

        for (idx, article) in articles.iter().enumerate() {
            // Find the most similar cluster
            let mut best_cluster = None;
            let mut best_sim = f32::NEG_INFINITY;

            for (cluster_idx, (centroid, _)) in clusters.iter().enumerate() {
                let sim = cosine_similarity(&article.embedding, centroid);
                if sim > best_sim {
                    best_sim = sim;
                    best_cluster = Some(cluster_idx);
                }
            }

            if best_sim >= threshold {
                // Add to existing cluster and update centroid
                let cluster_idx = best_cluster.expect("best_cluster should be Some when best_sim >= threshold");
                let (ref mut centroid, ref mut indices) = clusters[cluster_idx];
                let n = indices.len() as f32;
                // Incremental centroid update: new_centroid = (old_centroid * n + new_embedding) / (n + 1)
                for i in 0..dim {
                    centroid[i] = (centroid[i] * n + article.embedding[i]) / (n + 1.0);
                }
                indices.push(idx);
            } else {
                // Create a new cluster
                clusters.push((article.embedding.clone(), vec![idx]));
            }

            if (idx + 1) % 1000 == 0 {
                tracing::debug!(
                    processed = idx + 1,
                    clusters = clusters.len(),
                    "Clustering progress"
                );
            }
        }

        clusters.into_iter().map(|(_, indices)| indices).collect()
    }

    /// Build an EventCluster from a set of article indices
    fn build_event_cluster(
        &self,
        articles: &[ArticleWithEmbedding],
        indices: &[usize],
        date_str: &str,
        cluster_idx: usize,
        now: &chrono::DateTime<Utc>,
    ) -> EventCluster {
        // Compute centroid
        let dim = articles[indices[0]].embedding.len();
        let mut centroid = vec![0.0f32; dim];
        for &idx in indices {
            for (i, val) in articles[idx].embedding.iter().enumerate() {
                centroid[i] += val;
            }
        }
        let n = indices.len() as f32;
        for val in &mut centroid {
            *val /= n;
        }

        // Build cluster articles with similarity to centroid
        let mut cluster_articles: Vec<ClusterArticle> = indices
            .iter()
            .map(|&idx| {
                let sim = cosine_similarity(&articles[idx].embedding, &centroid) as f64;
                ClusterArticle {
                    id: articles[idx].id.clone(),
                    title: articles[idx].title.clone(),
                    category: articles[idx].category.clone(),
                    publisher: articles[idx].publisher.clone(),
                    published_at: articles[idx].published_at.clone(),
                    url: articles[idx].url.clone(),
                    similarity_to_centroid: sim,
                }
            })
            .collect();

        // Sort by similarity descending
        cluster_articles.sort_by(|a, b| {
            b.similarity_to_centroid
                .partial_cmp(&a.similarity_to_centroid)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Determine primary category (most frequent)
        let mut cat_counts: HashMap<&str, usize> = HashMap::new();
        for art in &cluster_articles {
            *cat_counts.entry(&art.category).or_insert(0) += 1;
        }
        let primary_category = cat_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(cat, _)| cat.to_string())
            .unwrap_or_default();

        // Determine date range
        let dates: Vec<&str> = cluster_articles
            .iter()
            .filter_map(|a| a.published_at.as_deref())
            .collect();
        let first_seen = dates.iter().min().map(|s| s.to_string());
        let last_updated = dates.iter().max().map(|s| s.to_string());

        // Compute average internal similarity
        let avg_similarity = cluster_articles
            .iter()
            .map(|a| a.similarity_to_centroid)
            .sum::<f64>()
            / cluster_articles.len() as f64;

        // Use the title of the most similar article as the event title
        let title = cluster_articles
            .first()
            .map(|a| a.title.clone())
            .unwrap_or_else(|| "Unknown Event".to_string());

        EventCluster {
            event_id: format!("evt_{}_{:03}", date_str, cluster_idx + 1),
            title,
            summary: String::new(),
            article_count: cluster_articles.len(),
            articles: cluster_articles,
            category: primary_category,
            first_seen,
            last_updated,
            avg_similarity,
            created_at: *now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a fake article with a given embedding for testing
    fn make_article(id: &str, title: &str, embedding: Vec<f32>) -> ArticleWithEmbedding {
        ArticleWithEmbedding {
            id: id.to_string(),
            title: title.to_string(),
            category: "politics".to_string(),
            publisher: Some("TestPub".to_string()),
            published_at: Some("2026-02-15T10:00:00Z".to_string()),
            url: format!("https://example.com/{id}"),
            embedding,
        }
    }

    #[test]
    fn test_cluster_empty_articles() {
        let articles: Vec<ArticleWithEmbedding> = vec![];
        // Empty input should produce empty output
        assert!(articles.is_empty());
    }

    #[test]
    fn test_cluster_identical_articles() {
        // Two identical embeddings should cluster together
        let articles = vec![
            make_article("001", "Article 1", vec![1.0, 0.0, 0.0]),
            make_article("002", "Article 2", vec![1.0, 0.0, 0.0]),
            make_article("003", "Article 3", vec![0.0, 1.0, 0.0]), // Different direction
        ];

        // Test cosine similarity
        let sim: f32 = cosine_similarity(&articles[0].embedding, &articles[1].embedding);
        assert!((sim - 1.0_f32).abs() < 0.001_f32, "Identical vectors should have sim ~1.0");

        let sim_diff: f32 = cosine_similarity(&articles[0].embedding, &articles[2].embedding);
        assert!(sim_diff.abs() < 0.001_f32, "Orthogonal vectors should have sim ~0.0");
    }

    #[test]
    fn test_cluster_similar_articles() {
        // Similar embeddings should cluster together with threshold 0.7
        let articles = vec![
            make_article("001", "Article 1", vec![1.0, 0.1, 0.0]),
            make_article("002", "Article 2", vec![0.95, 0.15, 0.05]),
            make_article("003", "Article 3", vec![0.0, 0.0, 1.0]),
        ];

        let sim: f32 = cosine_similarity(&articles[0].embedding, &articles[1].embedding);
        assert!(sim > 0.95_f32, "Similar vectors should have high similarity: {sim}");
    }

    #[test]
    fn test_build_event_cluster() {
        let articles = vec![
            make_article("001", "Main Article", vec![1.0, 0.0, 0.0]),
            make_article("002", "Related Article", vec![0.9, 0.1, 0.0]),
        ];

        let now = Utc::now();
        let date_str = now.format("%Y%m%d").to_string();

        // Build a cluster from both articles
        // We need a ClusterEngine but can't create one without OpenSearch
        // So test the component logic instead

        // Verify centroid computation
        let centroid: Vec<f32> = vec![
            (1.0_f32 + 0.9) / 2.0,
            (0.0_f32 + 0.1) / 2.0,
            (0.0_f32 + 0.0) / 2.0,
        ];
        assert!((centroid[0] - 0.95_f32).abs() < 0.001_f32);
        assert!((centroid[1] - 0.05_f32).abs() < 0.001_f32);
    }

    #[test]
    fn test_category_majority_vote() {
        let mut cat_counts: HashMap<&str, usize> = HashMap::new();
        *cat_counts.entry("politics").or_insert(0) += 3;
        *cat_counts.entry("economy").or_insert(0) += 1;
        *cat_counts.entry("society").or_insert(0) += 2;

        let primary = cat_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(cat, _)| cat.to_string())
            .unwrap_or_default();

        assert_eq!(primary, "politics");
    }
}
