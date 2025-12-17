//! Vector embedding and OpenSearch integration
//!
//! This module handles text embedding generation and vector search
//! operations using OpenSearch with Korean (Nori) analyzer support.
//!
//! # Architecture
//!
//! - `tokenizer` - Text tokenization and chunking
//! - `vectorize` - Embedding generation using Candle/BERT
//! - `VectorStore` - OpenSearch client for indexing and search

pub mod tokenizer;
pub mod vectorize;

pub use tokenizer::{ChunkConfig, TextChunk, TextTokenizer, TokenizerStats};
pub use vectorize::{
    cosine_similarity, dot_product, l2_normalize_vec, Embedder, EmbeddingConfig, EmbeddingStats,
};

use anyhow::{Context, Result};
use opensearch::{
    http::transport::{SingleNodeConnectionPool, TransportBuilder},
    BulkOperation, BulkParts, DeleteByQueryParts, IndexParts, OpenSearch, SearchParts,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use url::Url;

use crate::config::OpenSearchConfig;
use crate::models::ParsedArticle;

/// Document to be indexed in OpenSearch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDocument {
    /// Unique document ID
    pub id: String,

    /// Article OID
    pub oid: String,

    /// Article AID
    pub aid: String,

    /// Article title
    pub title: String,

    /// Full article content
    pub content: String,

    /// Article category
    pub category: String,

    /// Publisher name
    pub publisher: Option<String>,

    /// Author name
    pub author: Option<String>,

    /// Article URL
    pub url: String,

    /// Publication date (ISO 8601)
    pub published_at: Option<String>,

    /// Crawl timestamp (ISO 8601)
    pub crawled_at: String,

    /// Comment count
    pub comment_count: Option<i32>,

    /// Embedding vector
    pub embedding: Vec<f32>,

    /// Chunk index (for chunked documents)
    pub chunk_index: Option<i32>,

    /// Chunk text (if different from content)
    pub chunk_text: Option<String>,
}

/// Search result from OpenSearch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Document ID
    pub id: String,

    /// Relevance score
    pub score: f32,

    /// Article title
    pub title: String,

    /// Article content (may be truncated)
    pub content: String,

    /// Category
    pub category: String,

    /// Publisher
    pub publisher: Option<String>,

    /// Article URL
    pub url: String,

    /// Publication date
    pub published_at: Option<String>,

    /// Highlight snippets
    pub highlights: Option<Vec<String>>,
}

/// Bulk indexing result
#[derive(Debug, Clone, Default)]
pub struct BulkResult {
    /// Successfully indexed count
    pub success: usize,

    /// Failed count
    pub failed: usize,

    /// Error messages
    pub errors: Vec<String>,

    /// Time taken in milliseconds
    pub took_ms: u64,
}

/// Search configuration
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Number of results to return
    pub k: usize,

    /// Minimum score threshold
    pub min_score: Option<f32>,

    /// Filter by category
    pub category: Option<String>,

    /// Filter by date range (start)
    pub date_from: Option<String>,

    /// Filter by date range (end)
    pub date_to: Option<String>,

    /// Use hybrid search (BM25 + k-NN)
    pub hybrid: bool,

    /// Weight for BM25 in hybrid search (0.0 - 1.0)
    pub bm25_weight: f32,

    /// Include highlights
    pub include_highlights: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            k: 10,
            min_score: None,
            category: None,
            date_from: None,
            date_to: None,
            hybrid: true,
            bm25_weight: 0.3,
            include_highlights: true,
        }
    }
}

/// OpenSearch vector store client
pub struct VectorStore {
    /// OpenSearch client
    client: OpenSearch,

    /// Index name
    index_name: String,

    /// Configuration
    config: OpenSearchConfig,
}

impl VectorStore {
    /// Create a new vector store
    pub fn new(config: &OpenSearchConfig) -> Result<Self> {
        let url = Url::parse(&config.url).context("Invalid OpenSearch URL")?;

        let conn_pool = SingleNodeConnectionPool::new(url);
        let mut transport_builder = TransportBuilder::new(conn_pool)
            .disable_proxy()
            .timeout(Duration::from_secs(60));

        // Add basic auth if credentials provided
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            transport_builder = transport_builder.auth(opensearch::auth::Credentials::Basic(
                username.clone(),
                password.clone(),
            ));
        }

        let transport = transport_builder
            .build()
            .context("Failed to build OpenSearch transport")?;

        let client = OpenSearch::new(transport);

        Ok(Self {
            client,
            index_name: config.index_name.clone(),
            config: config.clone(),
        })
    }

    /// Check if index exists
    pub async fn index_exists(&self) -> Result<bool> {
        let response = self
            .client
            .indices()
            .exists(opensearch::indices::IndicesExistsParts::Index(&[
                &self.index_name
            ]))
            .send()
            .await
            .context("Failed to check index existence")?;

        Ok(response.status_code().is_success())
    }

    /// Create index with nori analyzer and k-NN mapping
    pub async fn create_index(&self, embedding_dim: usize) -> Result<()> {
        let body = json!({
            "settings": {
                "number_of_shards": 1,
                "number_of_replicas": 0,
                "index": {
                    "knn": true,
                    "knn.algo_param.ef_search": 100,
                    "refresh_interval": "5s"
                },
                "analysis": {
                    "tokenizer": {
                        "nori_mixed": {
                            "type": "nori_tokenizer",
                            "decompound_mode": "mixed",
                            "discard_punctuation": true
                        }
                    },
                    "analyzer": {
                        "nori_analyzer": {
                            "type": "custom",
                            "tokenizer": "nori_mixed",
                            "filter": ["lowercase", "nori_posfilter", "nori_readingform"]
                        },
                        "nori_search_analyzer": {
                            "type": "custom",
                            "tokenizer": "nori_mixed",
                            "filter": ["lowercase", "nori_posfilter"]
                        }
                    },
                    "filter": {
                        "nori_posfilter": {
                            "type": "nori_part_of_speech",
                            "stoptags": [
                                "E", "IC", "J", "MAG", "MM", "SP", "SSC", "SSO",
                                "SC", "SE", "XPN", "XSA", "XSN", "XSV", "UNA", "NA", "VSV"
                            ]
                        }
                    }
                }
            },
            "mappings": {
                "properties": {
                    "id": { "type": "keyword" },
                    "oid": { "type": "keyword" },
                    "aid": { "type": "keyword" },
                    "title": {
                        "type": "text",
                        "analyzer": "nori_analyzer",
                        "search_analyzer": "nori_search_analyzer",
                        "fields": {
                            "keyword": { "type": "keyword", "ignore_above": 256 }
                        }
                    },
                    "content": {
                        "type": "text",
                        "analyzer": "nori_analyzer",
                        "search_analyzer": "nori_search_analyzer"
                    },
                    "category": { "type": "keyword" },
                    "publisher": { "type": "keyword" },
                    "author": { "type": "keyword" },
                    "url": { "type": "keyword", "index": false },
                    "published_at": {
                        "type": "date",
                        "format": "strict_date_optional_time||epoch_millis"
                    },
                    "crawled_at": {
                        "type": "date",
                        "format": "strict_date_optional_time||epoch_millis"
                    },
                    "comment_count": { "type": "integer" },
                    "embedding": {
                        "type": "knn_vector",
                        "dimension": embedding_dim,
                        "method": {
                            "name": "hnsw",
                            "space_type": "cosinesimil",
                            "engine": "nmslib",
                            "parameters": {
                                "ef_construction": 128,
                                "m": 16
                            }
                        }
                    },
                    "chunk_index": { "type": "integer" },
                    "chunk_text": {
                        "type": "text",
                        "analyzer": "nori_analyzer",
                        "search_analyzer": "nori_search_analyzer"
                    }
                }
            }
        });

        let response = self
            .client
            .indices()
            .create(opensearch::indices::IndicesCreateParts::Index(
                &self.index_name,
            ))
            .body(body)
            .send()
            .await
            .context("Failed to create index")?;

        if !response.status_code().is_success() {
            let error_body = response.text().await?;
            anyhow::bail!("Index creation failed: {error_body}");
        }

        tracing::info!(index = %self.index_name, "Index created successfully");
        Ok(())
    }

    /// Delete index
    pub async fn delete_index(&self) -> Result<()> {
        self.client
            .indices()
            .delete(opensearch::indices::IndicesDeleteParts::Index(&[
                &self.index_name
            ]))
            .send()
            .await
            .context("Failed to delete index")?;

        Ok(())
    }

    /// Index a single document
    pub async fn index_document(&self, doc: &IndexDocument) -> Result<()> {
        let response = self
            .client
            .index(IndexParts::IndexId(&self.index_name, &doc.id))
            .body(doc)
            .send()
            .await
            .context("Failed to index document")?;

        if !response.status_code().is_success() {
            let error_body = response.text().await?;
            anyhow::bail!("Document indexing failed: {error_body}");
        }

        Ok(())
    }

    /// Bulk index multiple documents
    pub async fn bulk_index(&self, documents: &[IndexDocument]) -> Result<BulkResult> {
        if documents.is_empty() {
            return Ok(BulkResult::default());
        }

        let start_time = std::time::Instant::now();

        // Build bulk operations
        let mut ops: Vec<BulkOperation<Value>> = Vec::with_capacity(documents.len() * 2);

        for doc in documents {
            let doc_json = serde_json::to_value(doc)?;
            ops.push(BulkOperation::index(doc_json).id(&doc.id).into());
        }

        let response = self
            .client
            .bulk(BulkParts::Index(&self.index_name))
            .body(ops)
            .send()
            .await
            .context("Failed to execute bulk index")?;

        let took_ms = start_time.elapsed().as_millis() as u64;

        // Parse response
        let response_body: Value = response.json().await?;

        let mut result = BulkResult {
            took_ms,
            ..Default::default()
        };

        if let Some(items) = response_body["items"].as_array() {
            for item in items {
                if let Some(index_result) = item.get("index") {
                    let status = index_result["status"].as_u64().unwrap_or(0);
                    if (200..300).contains(&status) {
                        result.success += 1;
                    } else {
                        result.failed += 1;
                        if let Some(error) = index_result.get("error") {
                            result.errors.push(error.to_string());
                        }
                    }
                }
            }
        }

        tracing::info!(
            success = result.success,
            failed = result.failed,
            took_ms = result.took_ms,
            "Bulk indexing completed"
        );

        Ok(result)
    }

    /// Search using k-NN vector similarity
    pub async fn search_knn(
        &self,
        query_vector: &[f32],
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        let mut query = json!({
            "knn": {
                "embedding": {
                    "vector": query_vector,
                    "k": config.k
                }
            }
        });

        // Add filters if specified
        if let Some(category) = &config.category {
            query["knn"]["embedding"]["filter"] = json!({
                "term": { "category": category }
            });
        }

        self.execute_search(query, config).await
    }

    /// Search using BM25 text matching
    pub async fn search_bm25(
        &self,
        query_text: &str,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        let mut should = vec![
            json!({
                "match": {
                    "title": {
                        "query": query_text,
                        "boost": 2.0
                    }
                }
            }),
            json!({
                "match": {
                    "content": {
                        "query": query_text
                    }
                }
            }),
        ];

        // Add chunk_text if available
        should.push(json!({
            "match": {
                "chunk_text": {
                    "query": query_text
                }
            }
        }));

        let mut query = json!({
            "bool": {
                "should": should,
                "minimum_should_match": 1
            }
        });

        // Add category filter
        if let Some(category) = &config.category {
            query["bool"]["filter"] = json!([
                { "term": { "category": category } }
            ]);
        }

        // Add date range filter
        if config.date_from.is_some() || config.date_to.is_some() {
            let mut range = json!({});
            if let Some(from) = &config.date_from {
                range["gte"] = json!(from);
            }
            if let Some(to) = &config.date_to {
                range["lte"] = json!(to);
            }
            if query["bool"]["filter"].is_null() {
                query["bool"]["filter"] = json!([]);
            }
            query["bool"]["filter"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "range": { "published_at": range } }));
        }

        self.execute_search(json!({ "query": query }), config).await
    }

    /// Hybrid search combining k-NN and BM25
    pub async fn search_hybrid(
        &self,
        query_text: &str,
        query_vector: &[f32],
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        // Build hybrid query using script_score
        let knn_weight = 1.0 - config.bm25_weight;

        let query = json!({
            "query": {
                "script_score": {
                    "query": {
                        "bool": {
                            "should": [
                                {
                                    "match": {
                                        "title": {
                                            "query": query_text,
                                            "boost": 2.0
                                        }
                                    }
                                },
                                {
                                    "match": {
                                        "content": {
                                            "query": query_text
                                        }
                                    }
                                }
                            ],
                            "minimum_should_match": 0
                        }
                    },
                    "script": {
                        "source": format!(
                            "_score * {} + (1 + cosineSimilarity(params.query_vector, 'embedding')) * {}",
                            config.bm25_weight, knn_weight
                        ),
                        "params": {
                            "query_vector": query_vector
                        }
                    }
                }
            },
            "size": config.k
        });

        self.execute_search(query, config).await
    }

    /// Execute search query and parse results
    async fn execute_search(
        &self,
        mut query: Value,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        // Set size
        query["size"] = json!(config.k);

        // Add source fields
        query["_source"] = json!([
            "id",
            "title",
            "content",
            "category",
            "publisher",
            "url",
            "published_at"
        ]);

        // Add highlighting if requested
        if config.include_highlights {
            query["highlight"] = json!({
                "fields": {
                    "title": { "number_of_fragments": 1 },
                    "content": { "number_of_fragments": 3, "fragment_size": 150 }
                },
                "pre_tags": ["<mark>"],
                "post_tags": ["</mark>"]
            });
        }

        // Add min_score if specified
        if let Some(min_score) = config.min_score {
            query["min_score"] = json!(min_score);
        }

        let response = self
            .client
            .search(SearchParts::Index(&[&self.index_name]))
            .body(query)
            .send()
            .await
            .context("Failed to execute search")?;

        let response_body: Value = response.json().await?;

        // Parse results
        let mut results = Vec::new();

        if let Some(hits) = response_body["hits"]["hits"].as_array() {
            for hit in hits {
                let source = &hit["_source"];
                let highlights = hit.get("highlight").and_then(|h| {
                    let mut snippets = Vec::new();
                    if let Some(title_highlights) = h["title"].as_array() {
                        for hl in title_highlights {
                            if let Some(s) = hl.as_str() {
                                snippets.push(s.to_string());
                            }
                        }
                    }
                    if let Some(content_highlights) = h["content"].as_array() {
                        for hl in content_highlights {
                            if let Some(s) = hl.as_str() {
                                snippets.push(s.to_string());
                            }
                        }
                    }
                    if snippets.is_empty() {
                        None
                    } else {
                        Some(snippets)
                    }
                });

                results.push(SearchResult {
                    id: source["id"].as_str().unwrap_or_default().to_string(),
                    score: hit["_score"].as_f64().unwrap_or(0.0) as f32,
                    title: source["title"].as_str().unwrap_or_default().to_string(),
                    content: source["content"]
                        .as_str()
                        .map(|s| truncate_string(s, 500))
                        .unwrap_or_default(),
                    category: source["category"].as_str().unwrap_or_default().to_string(),
                    publisher: source["publisher"].as_str().map(String::from),
                    url: source["url"].as_str().unwrap_or_default().to_string(),
                    published_at: source["published_at"].as_str().map(String::from),
                    highlights,
                });
            }
        }

        Ok(results)
    }

    /// Delete documents by query
    pub async fn delete_by_query(&self, field: &str, value: &str) -> Result<usize> {
        let query = json!({
            "query": {
                "term": {
                    field: value
                }
            }
        });

        let response = self
            .client
            .delete_by_query(DeleteByQueryParts::Index(&[&self.index_name]))
            .body(query)
            .send()
            .await
            .context("Failed to delete by query")?;

        let response_body: Value = response.json().await?;
        let deleted = response_body["deleted"].as_u64().unwrap_or(0) as usize;

        Ok(deleted)
    }

    /// Get document count
    pub async fn count(&self) -> Result<usize> {
        let response = self
            .client
            .count(opensearch::CountParts::Index(&[&self.index_name]))
            .send()
            .await
            .context("Failed to count documents")?;

        let response_body: Value = response.json().await?;
        let count = response_body["count"].as_u64().unwrap_or(0) as usize;

        Ok(count)
    }

    /// Refresh index
    pub async fn refresh(&self) -> Result<()> {
        self.client
            .indices()
            .refresh(opensearch::indices::IndicesRefreshParts::Index(&[
                &self.index_name
            ]))
            .send()
            .await
            .context("Failed to refresh index")?;

        Ok(())
    }

    /// Get index name
    pub fn index_name(&self) -> &str {
        &self.index_name
    }
}

/// Helper to truncate string at word boundary
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    let mut truncated: String = s.chars().take(max_len).collect();

    // Find last space to avoid cutting words
    if let Some(last_space) = truncated.rfind(' ') {
        truncated.truncate(last_space);
    }

    truncated.push_str("...");
    truncated
}

/// Convert ParsedArticle to IndexDocument
pub fn article_to_document(
    article: &ParsedArticle,
    embedding: Vec<f32>,
    chunk_index: Option<i32>,
    chunk_text: Option<String>,
) -> IndexDocument {
    IndexDocument {
        id: format!("{}_{}", article.oid, article.aid),
        oid: article.oid.clone(),
        aid: article.aid.clone(),
        title: article.title.clone(),
        content: article.content.clone(),
        category: article.category.clone(),
        publisher: article.publisher.clone(),
        author: article.author.clone(),
        url: article.url.clone(),
        published_at: article.published_at.map(|dt| dt.to_rfc3339()),
        crawled_at: chrono::Utc::now().to_rfc3339(),
        comment_count: None, // Comment count not stored in ParsedArticle
        embedding,
        chunk_index,
        chunk_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert_eq!(config.k, 10);
        assert!(config.hybrid);
        assert!((config.bm25_weight - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_truncate_string() {
        let s = "This is a test string that is quite long";
        let truncated = truncate_string(s, 20);
        assert!(truncated.len() <= 23); // 20 + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_string_short() {
        let s = "Short";
        let truncated = truncate_string(s, 20);
        assert_eq!(truncated, "Short");
    }

    #[test]
    fn test_bulk_result_default() {
        let result = BulkResult::default();
        assert_eq!(result.success, 0);
        assert_eq!(result.failed, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_index_document_serialization() {
        let doc = IndexDocument {
            id: "001_001".to_string(),
            oid: "001".to_string(),
            aid: "001".to_string(),
            title: "Test Article".to_string(),
            content: "Content".to_string(),
            category: "politics".to_string(),
            publisher: Some("Test Publisher".to_string()),
            author: None,
            url: "https://example.com".to_string(),
            published_at: None,
            crawled_at: "2024-01-01T00:00:00Z".to_string(),
            comment_count: Some(10),
            embedding: vec![0.1, 0.2, 0.3],
            chunk_index: None,
            chunk_text: None,
        };

        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("Test Article"));
    }

    #[test]
    fn test_article_to_document() {
        let article = ParsedArticle {
            oid: "001".to_string(),
            aid: "002".to_string(),
            title: "Test".to_string(),
            content: "Content".to_string(),
            url: "https://example.com".to_string(),
            category: "tech".to_string(),
            ..Default::default()
        };

        let embedding = vec![0.1, 0.2, 0.3];
        let doc = article_to_document(&article, embedding, None, None);

        assert_eq!(doc.id, "001_002");
        assert_eq!(doc.title, "Test");
        assert_eq!(doc.embedding.len(), 3);
    }

    // Integration tests require running OpenSearch
    #[tokio::test]
    #[ignore = "Requires running OpenSearch"]
    async fn test_vector_store_creation() {
        let config = OpenSearchConfig {
            url: "http://localhost:9200".to_string(),
            index_name: "test-index".to_string(),
            username: None,
            password: None,
        };

        let store = VectorStore::new(&config);
        assert!(store.is_ok());
    }
}
