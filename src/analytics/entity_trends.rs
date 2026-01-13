//! Entity trend analysis with mention tracking and co-occurrence patterns
//!
//! This module provides functionality for:
//! - Tracking entity mentions over time
//! - Co-occurrence analysis between entities
//! - Network graph construction for entity relationships
//! - Temporal evolution of entity relevance

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use thiserror::Error;

/// Errors that can occur during entity trend analysis
#[derive(Debug, Error)]
pub enum EntityError {
    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Insufficient co-occurrence data for entities: {0} and {1}")]
    InsufficientCooccurrence(String, String),

    #[error("Invalid threshold: {0}")]
    InvalidThreshold(f64),
}

/// Result type for entity analysis operations
pub type EntityResult<T> = Result<T, EntityError>;

/// Entity type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    /// Person name
    Person,

    /// Organization
    Organization,

    /// Location
    Location,

    /// Product or brand
    Product,

    /// Event
    Event,

    /// Other or unknown
    Other,
}

impl EntityType {
    /// Parse entity type from string
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "person" | "per" => Self::Person,
            "organization" | "org" => Self::Organization,
            "location" | "loc" | "gpe" => Self::Location,
            "product" | "prod" => Self::Product,
            "event" => Self::Event,
            _ => Self::Other,
        }
    }

    /// Get string representation
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "organization",
            Self::Location => "location",
            Self::Product => "product",
            Self::Event => "event",
            Self::Other => "other",
        }
    }
}

/// Entity mention in a specific context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMention {
    /// When the entity was mentioned
    pub timestamp: DateTime<Utc>,

    /// Document/article ID where mentioned
    pub document_id: String,

    /// Sentiment score if available (-1.0 to 1.0)
    pub sentiment: Option<f64>,

    /// Context snippet around the mention
    pub context: Option<String>,
}

impl EntityMention {
    /// Create a new entity mention
    #[must_use]
    pub fn new(timestamp: DateTime<Utc>, document_id: String) -> Self {
        Self {
            timestamp,
            document_id,
            sentiment: None,
            context: None,
        }
    }

    /// Create mention with sentiment
    #[must_use]
    pub fn with_sentiment(mut self, sentiment: f64) -> Self {
        self.sentiment = Some(sentiment.clamp(-1.0, 1.0));
        self
    }

    /// Create mention with context
    #[must_use]
    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

/// Entity with temporal tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Canonical name of the entity
    pub name: String,

    /// Entity type
    pub entity_type: EntityType,

    /// All mentions ordered by time
    mentions: BTreeMap<DateTime<Utc>, Vec<EntityMention>>,

    /// Alternative names/aliases
    pub aliases: HashSet<String>,

    /// Total mention count
    pub total_mentions: u64,
}

impl Entity {
    /// Create a new entity
    #[must_use]
    pub fn new(name: String, entity_type: EntityType) -> Self {
        Self {
            name,
            entity_type,
            mentions: BTreeMap::new(),
            aliases: HashSet::new(),
            total_mentions: 0,
        }
    }

    /// Add an alias for this entity
    pub fn add_alias(&mut self, alias: String) {
        self.aliases.insert(alias);
    }

    /// Record a mention of this entity
    pub fn add_mention(&mut self, mention: EntityMention) {
        let timestamp = mention.timestamp;
        self.mentions
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push(mention);
        self.total_mentions += 1;
    }

    /// Get mentions in a time range
    #[must_use]
    pub fn mentions_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<&EntityMention> {
        self.mentions
            .range(start..=end)
            .flat_map(|(_, mentions)| mentions.iter())
            .collect()
    }

    /// Get mention count in a time range
    #[must_use]
    pub fn count_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
        self.mentions
            .range(start..=end)
            .map(|(_, mentions)| mentions.len() as u64)
            .sum()
    }

    /// Calculate average sentiment over all mentions
    #[must_use]
    pub fn average_sentiment(&self) -> Option<f64> {
        let sentiments: Vec<f64> = self
            .mentions
            .values()
            .flat_map(|m| m.iter())
            .filter_map(|m| m.sentiment)
            .collect();

        if sentiments.is_empty() {
            None
        } else {
            Some(sentiments.iter().sum::<f64>() / sentiments.len() as f64)
        }
    }

    /// Get unique documents mentioning this entity
    #[must_use]
    pub fn document_ids(&self) -> HashSet<String> {
        self.mentions
            .values()
            .flat_map(|m| m.iter())
            .map(|m| m.document_id.clone())
            .collect()
    }
}

/// Co-occurrence relationship between two entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cooccurrence {
    /// First entity
    pub entity_a: String,

    /// Second entity
    pub entity_b: String,

    /// Number of documents where both appear
    pub count: u64,

    /// Pointwise Mutual Information score
    pub pmi: f64,

    /// List of document IDs where they co-occur
    pub document_ids: Vec<String>,
}

impl Cooccurrence {
    /// Create a new co-occurrence relationship
    #[must_use]
    pub fn new(entity_a: String, entity_b: String, document_ids: Vec<String>) -> Self {
        Self {
            entity_a,
            entity_b,
            count: document_ids.len() as u64,
            pmi: 0.0,
            document_ids,
        }
    }

    /// Calculate PMI score
    ///
    /// PMI(a,b) = log2(P(a,b) / (P(a) * P(b)))
    ///
    /// # Arguments
    /// * `p_a` - Probability of entity A appearing
    /// * `p_b` - Probability of entity B appearing
    /// * `p_ab` - Probability of A and B appearing together
    pub fn calculate_pmi(&mut self, p_a: f64, p_b: f64, p_ab: f64) {
        if p_a > 0.0 && p_b > 0.0 && p_ab > 0.0 {
            self.pmi = (p_ab / (p_a * p_b)).log2();
        } else {
            self.pmi = 0.0;
        }
    }
}

/// Entity relationship network with co-occurrence tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNetwork {
    /// All entities in the network
    entities: HashMap<String, Entity>,

    /// Document-to-entities mapping for co-occurrence
    document_entities: HashMap<String, HashSet<String>>,

    /// Cached co-occurrence relationships
    #[serde(skip)]
    cooccurrence_cache: HashMap<(String, String), Cooccurrence>,
}

impl EntityNetwork {
    /// Create a new entity network
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            document_entities: HashMap::new(),
            cooccurrence_cache: HashMap::new(),
        }
    }

    /// Add an entity to the network
    pub fn add_entity(&mut self, entity: Entity) {
        self.entities.insert(entity.name.clone(), entity);
        self.cooccurrence_cache.clear();
    }

    /// Record entity mentions in a document
    ///
    /// # Arguments
    /// * `document_id` - Document identifier
    /// * `entity_mentions` - List of (entity_name, mention) tuples
    pub fn record_document(
        &mut self,
        document_id: String,
        entity_mentions: Vec<(String, EntityMention)>,
    ) {
        let entity_names: HashSet<String> = entity_mentions
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        // Add mentions to entities
        for (entity_name, mention) in entity_mentions {
            self.entities
                .entry(entity_name.clone())
                .or_insert_with(|| Entity::new(entity_name.clone(), EntityType::Other))
                .add_mention(mention);
        }

        // Track document-entity relationship
        self.document_entities.insert(document_id, entity_names);
        self.cooccurrence_cache.clear();
    }

    /// Get an entity by name
    #[must_use]
    pub fn get_entity(&self, name: &str) -> Option<&Entity> {
        self.entities.get(name)
    }

    /// Get all entity names
    #[must_use]
    pub fn entity_names(&self) -> Vec<&str> {
        self.entities.keys().map(String::as_str).collect()
    }

    /// Calculate co-occurrence between two entities
    ///
    /// # Arguments
    /// * `entity_a` - First entity name
    /// * `entity_b` - Second entity name
    pub fn cooccurrence(&mut self, entity_a: &str, entity_b: &str) -> EntityResult<Cooccurrence> {
        // Normalize order for caching
        let (a, b) = if entity_a < entity_b {
            (entity_a, entity_b)
        } else {
            (entity_b, entity_a)
        };

        let key = (a.to_string(), b.to_string());

        // Check cache
        if let Some(cached) = self.cooccurrence_cache.get(&key) {
            return Ok(cached.clone());
        }

        // Get entities
        let entity_a_obj = self.entities.get(a)
            .ok_or_else(|| EntityError::EntityNotFound(a.to_string()))?;
        let entity_b_obj = self.entities.get(b)
            .ok_or_else(|| EntityError::EntityNotFound(b.to_string()))?;

        // Find documents containing both
        let docs_a = entity_a_obj.document_ids();
        let docs_b = entity_b_obj.document_ids();
        let common_docs: Vec<String> = docs_a
            .intersection(&docs_b)
            .cloned()
            .collect();

        if common_docs.is_empty() {
            return Err(EntityError::InsufficientCooccurrence(
                a.to_string(),
                b.to_string(),
            ));
        }

        // Calculate PMI
        let total_docs = self.document_entities.len() as f64;
        let p_a = docs_a.len() as f64 / total_docs;
        let p_b = docs_b.len() as f64 / total_docs;
        let p_ab = common_docs.len() as f64 / total_docs;

        let mut cooccur = Cooccurrence::new(a.to_string(), b.to_string(), common_docs);
        cooccur.calculate_pmi(p_a, p_b, p_ab);

        // Cache result
        self.cooccurrence_cache.insert(key, cooccur.clone());

        Ok(cooccur)
    }

    /// Find all entities that co-occur with a given entity
    ///
    /// # Arguments
    /// * `entity_name` - The entity to find relationships for
    /// * `min_count` - Minimum co-occurrence count threshold
    ///
    /// # Returns
    /// List of (entity_name, co-occurrence) sorted by count descending
    pub fn find_related(
        &mut self,
        entity_name: &str,
        min_count: u64,
    ) -> EntityResult<Vec<(String, Cooccurrence)>> {
        let _entity = self.entities.get(entity_name)
            .ok_or_else(|| EntityError::EntityNotFound(entity_name.to_string()))?;

        // Collect entity keys first to avoid borrow issues
        let other_names: Vec<_> = self.entities
            .keys()
            .filter(|name| name.as_str() != entity_name)
            .cloned()
            .collect();

        let mut related = Vec::new();

        for other_name in other_names {
            if let Ok(cooccur) = self.cooccurrence(entity_name, &other_name) {
                if cooccur.count >= min_count {
                    related.push((other_name, cooccur));
                }
            }
        }

        // Sort by count descending
        related.sort_by(|a, b| b.1.count.cmp(&a.1.count));

        Ok(related)
    }

    /// Get top entities by mention count
    ///
    /// # Arguments
    /// * `limit` - Maximum number of entities to return
    /// * `entity_type` - Optional filter by entity type
    ///
    /// # Returns
    /// Entities sorted by total mentions descending
    #[must_use]
    pub fn top_entities(&self, limit: usize, entity_type: Option<EntityType>) -> Vec<&Entity> {
        let mut entities: Vec<_> = self.entities.values().collect();

        // Filter by type if specified
        if let Some(etype) = entity_type {
            entities.retain(|e| e.entity_type == etype);
        }

        entities.sort_by(|a, b| b.total_mentions.cmp(&a.total_mentions));
        entities.truncate(limit);
        entities
    }

    /// Build co-occurrence matrix for network analysis
    ///
    /// # Arguments
    /// * `min_pmi` - Minimum PMI score threshold
    ///
    /// # Returns
    /// List of all co-occurrences meeting the threshold
    pub fn cooccurrence_matrix(&mut self, min_pmi: f64) -> Vec<Cooccurrence> {
        let entity_names: Vec<_> = self.entities.keys().cloned().collect();
        let mut matrix = Vec::new();

        for i in 0..entity_names.len() {
            for j in (i + 1)..entity_names.len() {
                if let Ok(cooccur) = self.cooccurrence(&entity_names[i], &entity_names[j]) {
                    if cooccur.pmi >= min_pmi {
                        matrix.push(cooccur);
                    }
                }
            }
        }

        // Sort by PMI descending
        matrix.sort_by(|a, b| b.pmi.partial_cmp(&a.pmi).unwrap_or(std::cmp::Ordering::Equal));

        matrix
    }

    /// Get trending entities based on recent activity
    ///
    /// # Arguments
    /// * `window_days` - Number of recent days to consider
    /// * `limit` - Maximum number of entities to return
    ///
    /// # Returns
    /// Entities sorted by recent mention count
    #[must_use]
    pub fn trending_entities(&self, window_days: i64, limit: usize) -> Vec<(&Entity, u64)> {
        let cutoff = Utc::now() - chrono::Duration::days(window_days);
        let now = Utc::now();

        let mut trending: Vec<_> = self.entities
            .values()
            .map(|entity| {
                let recent_count = entity.count_in_range(cutoff, now);
                (entity, recent_count)
            })
            .filter(|(_, count)| *count > 0)
            .collect();

        trending.sort_by(|a, b| b.1.cmp(&a.1));
        trending.truncate(limit);
        trending
    }
}

impl Default for EntityNetwork {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_creation() {
        let entity = Entity::new("Apple Inc.".to_string(), EntityType::Organization);
        assert_eq!(entity.name, "Apple Inc.");
        assert_eq!(entity.entity_type, EntityType::Organization);
        assert_eq!(entity.total_mentions, 0);
    }

    #[test]
    fn test_entity_mention() {
        let mut entity = Entity::new("Tesla".to_string(), EntityType::Organization);
        let mention = EntityMention::new(Utc::now(), "doc1".to_string());

        entity.add_mention(mention);
        assert_eq!(entity.total_mentions, 1);
    }

    #[test]
    fn test_entity_aliases() {
        let mut entity = Entity::new("USA".to_string(), EntityType::Location);
        entity.add_alias("United States".to_string());
        entity.add_alias("US".to_string());

        assert_eq!(entity.aliases.len(), 2);
        assert!(entity.aliases.contains("US"));
    }

    #[test]
    fn test_entity_network() {
        let mut network = EntityNetwork::new();

        let entity1 = Entity::new("Apple".to_string(), EntityType::Organization);
        let entity2 = Entity::new("Tim Cook".to_string(), EntityType::Person);

        network.add_entity(entity1);
        network.add_entity(entity2);

        assert_eq!(network.entity_names().len(), 2);
        assert!(network.get_entity("Apple").is_some());
    }

    #[test]
    fn test_document_recording() {
        let mut network = EntityNetwork::new();
        let now = Utc::now();

        let mentions = vec![
            ("Apple".to_string(), EntityMention::new(now, "doc1".to_string())),
            ("Tim Cook".to_string(), EntityMention::new(now, "doc1".to_string())),
        ];

        network.record_document("doc1".to_string(), mentions);

        assert_eq!(network.entity_names().len(), 2);
        assert_eq!(network.get_entity("Apple").unwrap().total_mentions, 1);
    }

    #[test]
    fn test_cooccurrence() {
        let mut network = EntityNetwork::new();
        let now = Utc::now();

        // Record two documents with co-occurring entities
        network.record_document(
            "doc1".to_string(),
            vec![
                ("Apple".to_string(), EntityMention::new(now, "doc1".to_string())),
                ("iPhone".to_string(), EntityMention::new(now, "doc1".to_string())),
            ],
        );

        network.record_document(
            "doc2".to_string(),
            vec![
                ("Apple".to_string(), EntityMention::new(now, "doc2".to_string())),
                ("iPhone".to_string(), EntityMention::new(now, "doc2".to_string())),
            ],
        );

        // Add documents without Apple/iPhone to make PMI positive
        // PMI is positive when entities co-occur MORE than expected by chance
        network.record_document(
            "doc3".to_string(),
            vec![
                ("Samsung".to_string(), EntityMention::new(now, "doc3".to_string())),
            ],
        );
        network.record_document(
            "doc4".to_string(),
            vec![
                ("Google".to_string(), EntityMention::new(now, "doc4".to_string())),
            ],
        );

        let cooccur = network.cooccurrence("Apple", "iPhone").unwrap();
        assert_eq!(cooccur.count, 2);
        // With 4 total docs, p_a = p_b = 0.5, p_ab = 0.5
        // PMI = log2(0.5 / (0.5 * 0.5)) = log2(2) = 1.0
        assert!(cooccur.pmi > 0.0, "PMI should be positive: {}", cooccur.pmi);
    }

    #[test]
    fn test_top_entities() {
        let mut network = EntityNetwork::new();
        let now = Utc::now();

        let mut entity1 = Entity::new("Popular".to_string(), EntityType::Other);
        entity1.add_mention(EntityMention::new(now, "doc1".to_string()));
        entity1.add_mention(EntityMention::new(now, "doc2".to_string()));
        entity1.add_mention(EntityMention::new(now, "doc3".to_string()));

        let mut entity2 = Entity::new("Less Popular".to_string(), EntityType::Other);
        entity2.add_mention(EntityMention::new(now, "doc1".to_string()));

        network.add_entity(entity1);
        network.add_entity(entity2);

        let top = network.top_entities(1, None);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].name, "Popular");
    }

    #[test]
    fn test_sentiment_tracking() {
        let mention1 = EntityMention::new(Utc::now(), "doc1".to_string())
            .with_sentiment(0.8);
        let mention2 = EntityMention::new(Utc::now(), "doc2".to_string())
            .with_sentiment(-0.2);

        assert_eq!(mention1.sentiment, Some(0.8));
        assert_eq!(mention2.sentiment, Some(-0.2));

        let mut entity = Entity::new("Company".to_string(), EntityType::Organization);
        entity.add_mention(mention1);
        entity.add_mention(mention2);

        let avg = entity.average_sentiment().unwrap();
        assert!((avg - 0.3).abs() < 0.01); // (0.8 + (-0.2)) / 2 = 0.3
    }
}
