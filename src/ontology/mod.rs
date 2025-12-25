//! Knowledge graph and ontology extraction
//!
//! This module handles extracting structured knowledge from Korean news articles
//! and building ontologies in various formats (JSON, RDF, Turtle, N-Triples, JSON-LD).
//!
//! # Features
//!
//! - **Entity Extraction**: Extract named entities (people, organizations, locations, etc.)
//!   using regex patterns optimized for Korean text
//! - **Relation Extraction**: Identify relationships between entities using LLM-based analysis
//! - **Hallucination Verification**: Validate extracted relations against source text
//! - **Entity Linking**: Link entities to external knowledge bases (Wikidata, DBpedia)
//! - **RDF Export**: Export knowledge graphs in multiple formats (Turtle, N-Triples, JSON-LD, RDF/XML)
//! - **Performance Profiling**: Track extraction statistics and memory usage
//!
//! # Submodules
//!
//! - [`extractor`] - Entity and relation extraction using regex patterns and LLM
//! - [`linker`] - Entity linking and normalization with Wikidata/DBpedia knowledge base
//! - [`storage`] - Triple persistence and indexing with JSON storage
//! - [`error`] - Custom error types for ontology operations
//! - [`stats`] - Statistics and profiling for extraction pipelines
//!
//! # Quick Start
//!
//! ## Basic Extraction
//!
//! ```ignore
//! use baram::ontology::{RelationExtractor, EntityLinker, TripleStore};
//!
//! // Extract entities and relations
//! let extractor = RelationExtractor::new();
//! let result = extractor.extract_from_article(&article)?;
//!
//! // Convert to triple store
//! let store = TripleStore::from_extraction(&result, &article.title);
//! println!("{}", store.to_turtle());
//! ```
//!
//! ## Entity Linking with Wikidata
//!
//! ```ignore
//! use baram::ontology::{EntityLinker, LinkerConfig};
//!
//! let linker = EntityLinker::with_config(LinkerConfig::strict());
//! let linked = linker.link("삼성전자");
//!
//! if let Some(entity) = linked {
//!     println!("Wikidata: {}", entity.wikidata_qid().unwrap_or_default());
//!     println!("RDF URI: {}", entity.rdf_uri);
//! }
//! ```
//!
//! ## RDF Export Formats
//!
//! ```ignore
//! use baram::ontology::{EntityLinker, LinkedTripleStore};
//!
//! let linker = EntityLinker::new();
//! let linked_store = linker.apply_to_triple_store(&store);
//!
//! // Multiple export formats
//! let turtle = linked_store.to_turtle();
//! let ntriples = linked_store.to_ntriples();
//! let jsonld = linked_store.to_json_ld()?;
//! let rdfxml = linked_store.to_rdf_xml();
//! ```
//!
//! ## Performance Profiling
//!
//! ```ignore
//! use baram::ontology::{ExtractionStats, BatchStats, PipelineProfiler};
//!
//! let mut stats = ExtractionStats::new("article_001");
//! stats.record_entity(EntityType::Person);
//! stats.record_relation(RelationType::Said, 0.9, true);
//! stats.estimate_memory();
//!
//! println!("{}", stats.summary());
//! ```
//!
//! # Error Handling
//!
//! This module uses [`OntologyError`] for specific error variants:
//!
//! ```ignore
//! use baram::ontology::{OntologyError, OntologyResult};
//!
//! fn process_article(article: &Article) -> OntologyResult<TripleStore> {
//!     // Returns OntologyError on failure with context
//! }
//! ```
//!
//! # Configuration
//!
//! All configuration structs support builder patterns with validation:
//!
//! ```ignore
//! use baram::ontology::{ExtractionConfig, LinkerConfig, StorageConfig};
//!
//! let extraction_config = ExtractionConfig::builder()
//!     .min_confidence(0.7)
//!     .max_entities(100)
//!     .build()?;
//!
//! let linker_config = LinkerConfig::builder()
//!     .similarity_threshold(0.8)
//!     .enable_external_linking(true)
//!     .build()?;
//!
//! let storage_config = StorageConfig::builder()
//!     .base_path("/data/ontology")
//!     .enable_compression(true)
//!     .build()?;
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::parser::Article;

// Submodules
pub mod error;
pub mod extractor;
pub mod linker;
pub mod stats;
pub mod storage;

// Re-export error types
pub use error::{OntologyError, OntologyResult};

// Re-export commonly used types from extractor
pub use extractor::{
    EntitySource, EntityType, ExtractedEntity, ExtractedRelation, ExtractionConfig,
    ExtractionConfigBuilder, ExtractionResult, LlmEntityResponse, LlmExtractionResponse,
    LlmRelationResponse, PromptTemplate, RelationExtractor, RelationType, Triple, TripleContext,
    TripleStats, TripleStore,
};

// Re-export verification types from extractor
pub use extractor::{
    HallucinationVerifier, MatchDetail, MatchType, VerificationFailure, VerificationResult,
    VerificationSummary,
};

// Re-export commonly used types from linker
pub use linker::{
    EntityLinker, KnowledgeBaseEntry, LinkedEntity, LinkedExtractionResult, LinkedRelation,
    LinkedTriple, LinkedTripleStore, LinkerConfig, LinkerConfigBuilder, LinkingStats,
};

// Re-export storage types
pub use storage::{
    IndexEntry, StorageConfig, StorageConfigBuilder, StorageIndex, StorageStats, TripleStorage,
};

// Re-export stats types
pub use stats::{
    format_bytes, parse_bytes, BatchStats, ExtractionStats, MemoryEstimator, PipelineProfiler,
    ProfileSummary, StageTiming,
};

/// Legacy ontology entity (for backwards compatibility)
/// Prefer using ExtractedEntity from extractor module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Entity ID
    pub id: String,

    /// Entity type (Person, Organization, Location, etc.)
    pub entity_type: EntityType,

    /// Entity name
    pub name: String,

    /// Entity properties
    pub properties: HashMap<String, String>,
}

/// Legacy relationship between entities (for backwards compatibility)
/// Prefer using ExtractedRelation from extractor module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// Source entity ID
    pub from: String,

    /// Relation type
    pub relation_type: String,

    /// Target entity ID
    pub to: String,

    /// Confidence score
    pub confidence: f32,
}

/// Legacy knowledge graph (for backwards compatibility)
/// Prefer using TripleStore from extractor module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    /// Entities in the graph
    pub entities: Vec<Entity>,

    /// Relations between entities
    pub relations: Vec<Relation>,
}

/// Legacy ontology extractor (for backwards compatibility)
/// Prefer using RelationExtractor from extractor module
pub struct OntologyExtractor {
    inner: RelationExtractor,
}

impl OntologyExtractor {
    /// Create a new ontology extractor
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RelationExtractor::new(),
        }
    }

    /// Extract entities and relations from an article
    pub fn extract(&self, _article: &Article) -> Result<KnowledgeGraph> {
        // Legacy implementation - returns empty graph
        // Use RelationExtractor.extract_from_article() for actual extraction
        Ok(KnowledgeGraph {
            entities: Vec::new(),
            relations: Vec::new(),
        })
    }

    /// Export knowledge graph to JSON
    pub fn export_json(&self, graph: &KnowledgeGraph) -> Result<String> {
        serde_json::to_string_pretty(graph).context("Failed to serialize knowledge graph")
    }

    /// Export knowledge graph to RDF Turtle format
    pub fn export_turtle(&self, _graph: &KnowledgeGraph) -> Result<String> {
        // Use TripleStore.to_turtle() for actual Turtle export
        Ok(String::new())
    }

    /// Export knowledge graph to RDF/XML format
    pub fn export_rdf(&self, _graph: &KnowledgeGraph) -> Result<String> {
        // RDF/XML export not yet implemented
        Ok(String::new())
    }

    /// Get the inner RelationExtractor for advanced usage
    pub fn relation_extractor(&self) -> &RelationExtractor {
        &self.inner
    }
}

impl Default for OntologyExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractor_creation() {
        let _extractor = OntologyExtractor::new();
        // Basic smoke test
    }

    #[test]
    fn test_knowledge_graph_serialization() {
        let graph = KnowledgeGraph {
            entities: vec![Entity {
                id: String::from("e1"),
                entity_type: EntityType::Person,
                name: String::from("Test Person"),
                properties: HashMap::new(),
            }],
            relations: Vec::new(),
        };

        let extractor = OntologyExtractor::new();
        let json = extractor.export_json(&graph);
        assert!(json.is_ok());
    }

    #[test]
    fn test_relation_extractor_access() {
        let extractor = OntologyExtractor::new();
        let _inner = extractor.relation_extractor();
    }
}
