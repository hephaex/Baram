//! Event clustering module for news articles
//!
//! Groups news articles into events using embedding-based cosine similarity.
//! Each cluster represents a real-world event covered by multiple articles.
//!
//! # Architecture
//!
//! 1. Load article embeddings from OpenSearch
//! 2. Compute pairwise cosine similarity
//! 3. Incremental clustering with threshold-based merging
//! 4. (Optional) Generate event summaries via vLLM
//! 5. Output clusters as JSON files

pub mod engine;
pub mod models;
pub mod summary;

pub use engine::ClusterEngine;
pub use models::{ClusterConfig, ClusterMetadata, EventCluster, ClusterArticle, ClusterOutput};
pub use summary::ClusterSummarizer;
