//! Knowledge graph and ontology extraction
//!
//! This module handles extracting structured knowledge from articles
//! and building ontologies in various formats (JSON, RDF, Turtle).
//!
//! # Submodules
//!
//! - [`extractor`] - Entity and relation extraction using regex patterns and LLM
//! - [`linker`] - Entity linking and normalization with knowledge base
//!
//! # Usage
//!
//! ```ignore
//! use ntimes::ontology::{RelationExtractor, EntityLinker, TripleStore};
//!
//! let extractor = RelationExtractor::new();
//! let result = extractor.extract_from_article(&article);
//! let store = TripleStore::from_extraction(&result, &article.title);
//! println!("{}", store.to_turtle());
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::parser::Article;

// Submodules
pub mod extractor;
pub mod linker;

// Re-export commonly used types from extractor
pub use extractor::{
    EntitySource, EntityType, ExtractionConfig, ExtractionResult, ExtractedEntity,
    ExtractedRelation, LlmEntityResponse, LlmExtractionResponse, LlmRelationResponse,
    PromptTemplate, RelationExtractor, RelationType, Triple, TripleContext, TripleStats,
    TripleStore,
};

// Re-export commonly used types from linker
pub use linker::{EntityLinker, KnowledgeBaseEntry, LinkedEntity, LinkerConfig};

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
