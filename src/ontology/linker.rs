//! Entity linking and normalization
//!
//! This module provides entity disambiguation and normalization:
//! - Canonical name resolution (이재명 대표 -> 이재명)
//! - Entity deduplication across articles
//! - Knowledge base integration
//! - Wikidata/DBpedia entity linking
//! - RDF URI generation for linked entities

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::extractor::{EntityType, ExtractionResult, ExtractedEntity, TripleStore};

/// Entity linking configuration
#[derive(Debug, Clone)]
pub struct LinkerConfig {
    /// Minimum similarity for matching (0.0 - 1.0)
    pub similarity_threshold: f32,

    /// Enable fuzzy matching
    pub fuzzy_matching: bool,

    /// Use title normalization (remove 씨, 님, etc.)
    pub normalize_titles: bool,

    /// Cache linked entities
    pub enable_cache: bool,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.8,
            fuzzy_matching: true,
            normalize_titles: true,
            enable_cache: true,
        }
    }
}

impl LinkerConfig {
    /// Create a new builder for LinkerConfig
    pub fn builder() -> LinkerConfigBuilder {
        LinkerConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), super::error::OntologyError> {
        if self.similarity_threshold < 0.0 || self.similarity_threshold > 1.0 {
            return Err(super::error::OntologyError::invalid_config(
                "similarity_threshold",
                self.similarity_threshold.to_string(),
                "Must be between 0.0 and 1.0",
            ));
        }
        Ok(())
    }

    /// Create a strict config (higher thresholds)
    pub fn strict() -> Self {
        Self {
            similarity_threshold: 0.9,
            fuzzy_matching: false,
            normalize_titles: true,
            enable_cache: true,
        }
    }

    /// Create a lenient config (lower thresholds)
    pub fn lenient() -> Self {
        Self {
            similarity_threshold: 0.6,
            fuzzy_matching: true,
            normalize_titles: true,
            enable_cache: true,
        }
    }
}

/// Builder for LinkerConfig with fluent API
#[derive(Debug, Clone, Default)]
pub struct LinkerConfigBuilder {
    similarity_threshold: Option<f32>,
    fuzzy_matching: Option<bool>,
    normalize_titles: Option<bool>,
    enable_cache: Option<bool>,
}

impl LinkerConfigBuilder {
    /// Set similarity threshold (0.0 - 1.0)
    pub fn similarity_threshold(mut self, threshold: f32) -> Self {
        self.similarity_threshold = Some(threshold);
        self
    }

    /// Enable or disable fuzzy matching
    pub fn fuzzy_matching(mut self, enable: bool) -> Self {
        self.fuzzy_matching = Some(enable);
        self
    }

    /// Enable or disable title normalization
    pub fn normalize_titles(mut self, enable: bool) -> Self {
        self.normalize_titles = Some(enable);
        self
    }

    /// Enable or disable caching
    pub fn enable_cache(mut self, enable: bool) -> Self {
        self.enable_cache = Some(enable);
        self
    }

    /// Build the config with validation
    pub fn build(self) -> Result<LinkerConfig, super::error::OntologyError> {
        let config = LinkerConfig {
            similarity_threshold: self.similarity_threshold.unwrap_or(0.8),
            fuzzy_matching: self.fuzzy_matching.unwrap_or(true),
            normalize_titles: self.normalize_titles.unwrap_or(true),
            enable_cache: self.enable_cache.unwrap_or(true),
        };
        config.validate()?;
        Ok(config)
    }

    /// Build without validation
    pub fn build_unchecked(self) -> LinkerConfig {
        LinkerConfig {
            similarity_threshold: self.similarity_threshold.unwrap_or(0.8),
            fuzzy_matching: self.fuzzy_matching.unwrap_or(true),
            normalize_titles: self.normalize_titles.unwrap_or(true),
            enable_cache: self.enable_cache.unwrap_or(true),
        }
    }
}

/// Linked entity with canonical form
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedEntity {
    /// Original text
    pub original: String,

    /// Canonical/normalized name
    pub canonical: String,

    /// Entity type
    pub entity_type: EntityType,

    /// Alternative names/aliases
    pub aliases: Vec<String>,

    /// External ID (e.g., Wikidata QID)
    pub external_id: Option<String>,

    /// All external IDs (wikidata, dbpedia, etc.)
    #[serde(default)]
    pub external_ids: HashMap<String, String>,

    /// Linking confidence
    pub confidence: f32,

    /// RDF URI for this entity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rdf_uri: Option<String>,

    /// Whether this entity was found in knowledge base
    pub in_knowledge_base: bool,
}

impl LinkedEntity {
    /// Generate RDF URI for this entity
    pub fn generate_rdf_uri(&self, base_uri: &str) -> String {
        if let Some(wikidata_id) = self.external_ids.get("wikidata") {
            format!("http://www.wikidata.org/entity/{wikidata_id}")
        } else if let Some(dbpedia_id) = self.external_ids.get("dbpedia") {
            format!("http://dbpedia.org/resource/{dbpedia_id}")
        } else {
            format!("{}{}", base_uri, url_encode(&self.canonical))
        }
    }

    /// Get Wikidata QID if available
    pub fn wikidata_qid(&self) -> Option<&str> {
        self.external_ids.get("wikidata").map(|s| s.as_str())
    }

    /// Get DBpedia URI if available
    pub fn dbpedia_uri(&self) -> Option<String> {
        self.external_ids
            .get("dbpedia")
            .map(|id| format!("http://dbpedia.org/resource/{id}"))
    }
}

/// URL-encode a string for use in URIs
fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                c.to_string()
            } else if c == ' ' {
                "_".to_string()
            } else {
                // Percent-encode non-ASCII characters byte by byte
                let mut buf = [0u8; 4];
                let bytes = c.encode_utf8(&mut buf);
                bytes.bytes().map(|b| format!("%{b:02X}")).collect()
            }
        })
        .collect()
}

/// Entity knowledge base entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBaseEntry {
    /// Canonical name
    pub canonical: String,

    /// Entity type
    pub entity_type: EntityType,

    /// Known aliases
    pub aliases: Vec<String>,

    /// External references
    pub external_ids: HashMap<String, String>,

    /// Additional properties
    pub properties: HashMap<String, String>,
}

/// Entity linker for normalization and disambiguation
pub struct EntityLinker {
    /// Configuration
    config: LinkerConfig,

    /// Known entities (canonical -> entry)
    knowledge_base: HashMap<String, KnowledgeBaseEntry>,

    /// Alias to canonical mapping
    alias_map: HashMap<String, String>,

    /// Linking cache
    cache: HashMap<String, LinkedEntity>,

    /// Title suffixes to remove
    title_suffixes: Vec<&'static str>,

    /// Organization suffixes (reserved for enhanced matching)
    #[allow(dead_code)]
    org_suffixes: Vec<&'static str>,
}

impl EntityLinker {
    /// Create a new entity linker
    pub fn new() -> Self {
        Self::with_config(LinkerConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: LinkerConfig) -> Self {
        let mut linker = Self {
            config,
            knowledge_base: HashMap::new(),
            alias_map: HashMap::new(),
            cache: HashMap::new(),
            title_suffixes: vec![
                "씨", "님", "대표", "회장", "사장", "원장", "총장", "장관", "의원",
                "대통령", "총리", "교수", "박사", "기자", "작가", "배우", "감독",
                "선수", "코치", "위원", "위원장", "본부장", "실장", "팀장", "부장",
            ],
            org_suffixes: vec![
                "그룹", "전자", "건설", "제약", "바이오", "엔터테인먼트", "엔터",
                "은행", "증권", "보험", "통신", "항공", "자동차", "중공업",
            ],
        };

        // Load default knowledge base
        linker.load_default_knowledge();

        linker
    }

    /// Load default knowledge base with common Korean entities
    fn load_default_knowledge(&mut self) {
        // Politicians with Wikidata IDs
        self.add_entry(KnowledgeBaseEntry {
            canonical: "윤석열".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["윤석열 대통령".to_string(), "윤 대통령".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q57549003".to_string()),
                ("dbpedia".to_string(), "Yoon_Suk-yeol".to_string()),
            ]
            .into(),
            properties: [("role".to_string(), "대통령".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "이재명".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["이재명 대표".to_string(), "이 대표".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q6512891".to_string()),
                ("dbpedia".to_string(), "Lee_Jae-myung".to_string()),
            ]
            .into(),
            properties: [("party".to_string(), "더불어민주당".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "한동훈".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["한동훈 대표".to_string(), "한 대표".to_string()],
            external_ids: [("wikidata".to_string(), "Q107192814".to_string())].into(),
            properties: [("party".to_string(), "국민의힘".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "이재용".to_string(),
            entity_type: EntityType::Person,
            aliases: vec![
                "이재용 회장".to_string(),
                "이 회장".to_string(),
                "Jay Y. Lee".to_string(),
            ],
            external_ids: [
                ("wikidata".to_string(), "Q491522".to_string()),
                ("dbpedia".to_string(), "Lee_Jae-yong_(businessman)".to_string()),
            ]
            .into(),
            properties: [("company".to_string(), "삼성전자".to_string())].into(),
        });

        // Major companies with Wikidata IDs
        self.add_entry(KnowledgeBaseEntry {
            canonical: "삼성전자".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec![
                "삼성".to_string(),
                "Samsung".to_string(),
                "Samsung Electronics".to_string(),
            ],
            external_ids: [
                ("wikidata".to_string(), "Q20718".to_string()),
                ("dbpedia".to_string(), "Samsung_Electronics".to_string()),
                ("stock".to_string(), "005930".to_string()),
            ]
            .into(),
            properties: [("industry".to_string(), "반도체".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "SK하이닉스".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["하이닉스".to_string(), "SK Hynix".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q487653".to_string()),
                ("dbpedia".to_string(), "SK_Hynix".to_string()),
                ("stock".to_string(), "000660".to_string()),
            ]
            .into(),
            properties: [("industry".to_string(), "반도체".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "현대자동차".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["현대차".to_string(), "Hyundai".to_string(), "현대".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q55931".to_string()),
                ("dbpedia".to_string(), "Hyundai_Motor_Company".to_string()),
                ("stock".to_string(), "005380".to_string()),
            ]
            .into(),
            properties: [("industry".to_string(), "자동차".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "LG전자".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["LG".to_string(), "LG Electronics".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q216047".to_string()),
                ("dbpedia".to_string(), "LG_Electronics".to_string()),
                ("stock".to_string(), "066570".to_string()),
            ]
            .into(),
            properties: [("industry".to_string(), "전자".to_string())].into(),
        });

        // Political parties with Wikidata IDs
        self.add_entry(KnowledgeBaseEntry {
            canonical: "국민의힘".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["국힘".to_string(), "여당".to_string(), "PPP".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q96165405".to_string()),
                ("dbpedia".to_string(), "People_Power_Party_(South_Korea)".to_string()),
            ]
            .into(),
            properties: [("type".to_string(), "정당".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "더불어민주당".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec![
                "민주당".to_string(),
                "더민주".to_string(),
                "야당".to_string(),
                "DPK".to_string(),
            ],
            external_ids: [
                ("wikidata".to_string(), "Q21207862".to_string()),
                (
                    "dbpedia".to_string(),
                    "Democratic_Party_of_Korea".to_string(),
                ),
            ]
            .into(),
            properties: [("type".to_string(), "정당".to_string())].into(),
        });

        // Government bodies
        self.add_entry(KnowledgeBaseEntry {
            canonical: "기획재정부".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["기재부".to_string(), "MOEF".to_string()],
            external_ids: [("wikidata".to_string(), "Q483867".to_string())].into(),
            properties: [("type".to_string(), "정부부처".to_string())].into(),
        });

        // Locations with Wikidata IDs
        self.add_entry(KnowledgeBaseEntry {
            canonical: "대한민국".to_string(),
            entity_type: EntityType::Location,
            aliases: vec![
                "한국".to_string(),
                "South Korea".to_string(),
                "Korea".to_string(),
            ],
            external_ids: [
                ("wikidata".to_string(), "Q884".to_string()),
                ("dbpedia".to_string(), "South_Korea".to_string()),
                ("iso".to_string(), "KR".to_string()),
            ]
            .into(),
            properties: HashMap::new(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "서울".to_string(),
            entity_type: EntityType::Location,
            aliases: vec!["서울시".to_string(), "서울특별시".to_string(), "Seoul".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q8684".to_string()),
                ("dbpedia".to_string(), "Seoul".to_string()),
            ]
            .into(),
            properties: [("country".to_string(), "대한민국".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "미국".to_string(),
            entity_type: EntityType::Location,
            aliases: vec![
                "미합중국".to_string(),
                "United States".to_string(),
                "USA".to_string(),
                "US".to_string(),
            ],
            external_ids: [
                ("wikidata".to_string(), "Q30".to_string()),
                ("dbpedia".to_string(), "United_States".to_string()),
                ("iso".to_string(), "US".to_string()),
            ]
            .into(),
            properties: HashMap::new(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "중국".to_string(),
            entity_type: EntityType::Location,
            aliases: vec![
                "중화인민공화국".to_string(),
                "China".to_string(),
                "PRC".to_string(),
            ],
            external_ids: [
                ("wikidata".to_string(), "Q148".to_string()),
                ("dbpedia".to_string(), "China".to_string()),
                ("iso".to_string(), "CN".to_string()),
            ]
            .into(),
            properties: HashMap::new(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "일본".to_string(),
            entity_type: EntityType::Location,
            aliases: vec!["Japan".to_string()],
            external_ids: [
                ("wikidata".to_string(), "Q17".to_string()),
                ("dbpedia".to_string(), "Japan".to_string()),
                ("iso".to_string(), "JP".to_string()),
            ]
            .into(),
            properties: HashMap::new(),
        });
    }

    /// Add entry to knowledge base
    pub fn add_entry(&mut self, entry: KnowledgeBaseEntry) {
        let canonical = entry.canonical.clone();

        // Map all aliases to canonical
        for alias in &entry.aliases {
            self.alias_map.insert(alias.to_lowercase(), canonical.clone());
        }

        // Also map canonical itself
        self.alias_map.insert(canonical.to_lowercase(), canonical.clone());

        self.knowledge_base.insert(canonical, entry);
    }

    /// Link an entity to its canonical form
    pub fn link(&mut self, entity: &ExtractedEntity) -> LinkedEntity {
        // Check cache first
        let cache_key = format!("{}:{:?}", entity.text, entity.entity_type);
        if self.config.enable_cache {
            if let Some(cached) = self.cache.get(&cache_key) {
                return cached.clone();
            }
        }

        // Normalize the entity text
        let normalized = self.normalize_text(&entity.text, entity.entity_type);

        // Try direct lookup
        if let Some(canonical) = self.alias_map.get(&normalized.to_lowercase()) {
            if let Some(kb_entry) = self.knowledge_base.get(canonical) {
                let rdf_uri = kb_entry
                    .external_ids
                    .get("wikidata")
                    .map(|qid| format!("http://www.wikidata.org/entity/{qid}"));

                let linked = LinkedEntity {
                    original: entity.text.clone(),
                    canonical: canonical.clone(),
                    entity_type: kb_entry.entity_type,
                    aliases: kb_entry.aliases.clone(),
                    external_id: kb_entry.external_ids.get("wikidata").cloned(),
                    external_ids: kb_entry.external_ids.clone(),
                    confidence: 0.95,
                    rdf_uri,
                    in_knowledge_base: true,
                };

                if self.config.enable_cache {
                    self.cache.insert(cache_key, linked.clone());
                }

                return linked;
            }
        }

        // Try fuzzy matching if enabled
        if self.config.fuzzy_matching {
            if let Some((canonical, similarity)) = self.fuzzy_match(&normalized) {
                if similarity >= self.config.similarity_threshold {
                    if let Some(kb_entry) = self.knowledge_base.get(&canonical) {
                        let rdf_uri = kb_entry
                            .external_ids
                            .get("wikidata")
                            .map(|qid| format!("http://www.wikidata.org/entity/{qid}"));

                        let linked = LinkedEntity {
                            original: entity.text.clone(),
                            canonical: canonical.clone(),
                            entity_type: kb_entry.entity_type,
                            aliases: kb_entry.aliases.clone(),
                            external_id: kb_entry.external_ids.get("wikidata").cloned(),
                            external_ids: kb_entry.external_ids.clone(),
                            confidence: similarity,
                            rdf_uri,
                            in_knowledge_base: true,
                        };

                        if self.config.enable_cache {
                            self.cache.insert(cache_key, linked.clone());
                        }

                        return linked;
                    }
                }
            }
        }

        // No match found, return normalized form
        let linked = LinkedEntity {
            original: entity.text.clone(),
            canonical: normalized.clone(),
            entity_type: entity.entity_type,
            aliases: vec![],
            external_id: None,
            external_ids: HashMap::new(),
            confidence: entity.confidence,
            rdf_uri: Some(format!(
                "https://ntimes.example.org/entity/{}",
                url_encode(&normalized)
            )),
            in_knowledge_base: false,
        };

        if self.config.enable_cache {
            self.cache.insert(cache_key, linked.clone());
        }

        linked
    }

    /// Normalize entity text
    fn normalize_text(&self, text: &str, entity_type: EntityType) -> String {
        let mut normalized = text.trim().to_string();

        if self.config.normalize_titles {
            match entity_type {
                EntityType::Person => {
                    // Remove title suffixes for people
                    for suffix in &self.title_suffixes {
                        if normalized.ends_with(suffix) {
                            normalized = normalized.trim_end_matches(suffix).trim().to_string();
                        }
                        // Also handle with space
                        let with_space = format!(" {suffix}");
                        if normalized.ends_with(&with_space) {
                            normalized = normalized.trim_end_matches(&with_space).trim().to_string();
                        }
                    }
                }
                EntityType::Organization => {
                    // Standardize organization names
                    // e.g., "삼성" -> might stay as is
                }
                _ => {}
            }
        }

        // Remove quotes (straight and curly quotes)
        normalized = normalized.trim_matches(|c| c == '\'' || c == '"' || c == '\u{201C}' || c == '\u{201D}').to_string();

        // Normalize whitespace
        normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");

        normalized
    }

    /// Fuzzy match against knowledge base
    fn fuzzy_match(&self, text: &str) -> Option<(String, f32)> {
        let text_lower = text.to_lowercase();
        let mut best_match: Option<(String, f32)> = None;

        for (canonical, entry) in &self.knowledge_base {
            // Check canonical
            let similarity = self.similarity(&text_lower, &canonical.to_lowercase());
            if similarity > best_match.as_ref().map(|(_, s)| *s).unwrap_or(0.0) {
                best_match = Some((canonical.clone(), similarity));
            }

            // Check aliases
            for alias in &entry.aliases {
                let similarity = self.similarity(&text_lower, &alias.to_lowercase());
                if similarity > best_match.as_ref().map(|(_, s)| *s).unwrap_or(0.0) {
                    best_match = Some((canonical.clone(), similarity));
                }
            }
        }

        best_match
    }

    /// Calculate string similarity (Jaccard-like for substrings)
    fn similarity(&self, a: &str, b: &str) -> f32 {
        if a == b {
            return 1.0;
        }

        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        // Check containment
        if a.contains(b) || b.contains(a) {
            let shorter = a.len().min(b.len());
            let longer = a.len().max(b.len());
            return shorter as f32 / longer as f32;
        }

        // Character-level Jaccard similarity
        let chars_a: std::collections::HashSet<char> = a.chars().collect();
        let chars_b: std::collections::HashSet<char> = b.chars().collect();

        let intersection = chars_a.intersection(&chars_b).count();
        let union = chars_a.union(&chars_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// Link multiple entities
    pub fn link_all(&mut self, entities: &[ExtractedEntity]) -> Vec<LinkedEntity> {
        entities.iter().map(|e| self.link(e)).collect()
    }

    /// Get knowledge base size
    pub fn knowledge_base_size(&self) -> usize {
        self.knowledge_base.len()
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Export knowledge base to JSON
    pub fn export_knowledge_base(&self) -> Result<String> {
        let entries: Vec<&KnowledgeBaseEntry> = self.knowledge_base.values().collect();
        Ok(serde_json::to_string_pretty(&entries)?)
    }

    /// Import knowledge base from JSON
    pub fn import_knowledge_base(&mut self, json: &str) -> Result<usize> {
        let entries: Vec<KnowledgeBaseEntry> = serde_json::from_str(json)?;
        let count = entries.len();

        for entry in entries {
            self.add_entry(entry);
        }

        Ok(count)
    }

    /// Link all entities in an ExtractionResult and return linked entities
    pub fn link_extraction_result(
        &mut self,
        result: &ExtractionResult,
    ) -> LinkedExtractionResult {
        let linked_entities: Vec<LinkedEntity> = result
            .entities
            .iter()
            .map(|e| self.link(e))
            .collect();

        // Build lookup for linked entities by original text
        let entity_map: HashMap<String, &LinkedEntity> = linked_entities
            .iter()
            .map(|e| (e.original.clone(), e))
            .collect();

        // Update relations with linked entity info
        let linked_relations: Vec<LinkedRelation> = result
            .relations
            .iter()
            .map(|r| {
                let subject_linked = entity_map.get(&r.subject);
                let object_linked = entity_map.get(&r.object);

                LinkedRelation {
                    subject: r.subject.clone(),
                    subject_canonical: subject_linked
                        .map(|e| e.canonical.clone())
                        .unwrap_or_else(|| r.subject.clone()),
                    subject_uri: subject_linked
                        .and_then(|e| e.rdf_uri.clone()),
                    subject_type: r.subject_type,
                    predicate: r.predicate,
                    object: r.object.clone(),
                    object_canonical: object_linked
                        .map(|e| e.canonical.clone())
                        .unwrap_or_else(|| r.object.clone()),
                    object_uri: object_linked.and_then(|e| e.rdf_uri.clone()),
                    object_type: r.object_type,
                    confidence: r.confidence,
                    evidence: r.evidence.clone(),
                    verified: r.verified,
                }
            })
            .collect();

        // Calculate statistics
        let kb_linked = linked_entities.iter().filter(|e| e.in_knowledge_base).count();
        let wikidata_linked = linked_entities
            .iter()
            .filter(|e| e.external_ids.contains_key("wikidata"))
            .count();

        LinkedExtractionResult {
            article_id: result.article_id.clone(),
            entities: linked_entities,
            relations: linked_relations,
            stats: LinkingStats {
                total_entities: result.entities.len(),
                kb_linked_entities: kb_linked,
                wikidata_linked_entities: wikidata_linked,
                total_relations: result.relations.len(),
            },
        }
    }

    /// Apply linking to a TripleStore and generate RDF output
    pub fn apply_to_triple_store(&mut self, store: &TripleStore) -> LinkedTripleStore {
        // Link all entities
        let linked_entities: Vec<LinkedEntity> = store
            .entities
            .iter()
            .map(|e| self.link(e))
            .collect();

        // Build entity lookup
        let entity_map: HashMap<String, &LinkedEntity> = linked_entities
            .iter()
            .map(|e| (e.original.clone(), e))
            .collect();

        // Update triples with linked URIs
        let linked_triples: Vec<LinkedTriple> = store
            .triples
            .iter()
            .map(|t| {
                let subject_linked = entity_map.get(&t.subject);
                let object_linked = entity_map.get(&t.object);

                LinkedTriple {
                    subject_uri: subject_linked
                        .and_then(|e| e.rdf_uri.clone())
                        .unwrap_or_else(|| t.subject_id.clone()),
                    subject: t.subject.clone(),
                    subject_canonical: subject_linked
                        .map(|e| e.canonical.clone())
                        .unwrap_or_else(|| t.subject.clone()),
                    subject_type: t.subject_type,
                    predicate_uri: t.predicate.clone(),
                    predicate_label: t.predicate_label.clone(),
                    object_uri: object_linked
                        .and_then(|e| e.rdf_uri.clone())
                        .unwrap_or_else(|| t.object_id.clone()),
                    object: t.object.clone(),
                    object_canonical: object_linked
                        .map(|e| e.canonical.clone())
                        .unwrap_or_else(|| t.object.clone()),
                    object_type: t.object_type,
                    confidence: t.confidence,
                    evidence: t.evidence.clone(),
                    verified: t.verified,
                }
            })
            .collect();

        LinkedTripleStore {
            article_id: store.article_id.clone(),
            article_title: store.article_title.clone(),
            extracted_at: store.extracted_at.clone(),
            entities: linked_entities,
            triples: linked_triples,
        }
    }

    /// Lookup entity by canonical name in knowledge base
    pub fn lookup(&self, canonical: &str) -> Option<&KnowledgeBaseEntry> {
        self.knowledge_base.get(canonical)
    }

    /// Get all entities with Wikidata IDs
    pub fn wikidata_entities(&self) -> Vec<&KnowledgeBaseEntry> {
        self.knowledge_base
            .values()
            .filter(|e| e.external_ids.contains_key("wikidata"))
            .collect()
    }
}

impl Default for EntityLinker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Linked Result Types
// ============================================================================

/// Relation with linked entity information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedRelation {
    /// Original subject text
    pub subject: String,
    /// Canonical subject name
    pub subject_canonical: String,
    /// Subject RDF URI
    pub subject_uri: Option<String>,
    /// Subject type
    pub subject_type: EntityType,
    /// Predicate
    pub predicate: super::extractor::RelationType,
    /// Original object text
    pub object: String,
    /// Canonical object name
    pub object_canonical: String,
    /// Object RDF URI
    pub object_uri: Option<String>,
    /// Object type
    pub object_type: EntityType,
    /// Confidence score
    pub confidence: f32,
    /// Evidence text
    pub evidence: String,
    /// Verified flag
    pub verified: bool,
}

/// Linked extraction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedExtractionResult {
    /// Article ID
    pub article_id: String,
    /// Linked entities
    pub entities: Vec<LinkedEntity>,
    /// Linked relations
    pub relations: Vec<LinkedRelation>,
    /// Linking statistics
    pub stats: LinkingStats,
}

/// Linking statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinkingStats {
    /// Total entities processed
    pub total_entities: usize,
    /// Entities linked to knowledge base
    pub kb_linked_entities: usize,
    /// Entities with Wikidata IDs
    pub wikidata_linked_entities: usize,
    /// Total relations
    pub total_relations: usize,
}

impl LinkingStats {
    /// Get KB linking rate as percentage
    pub fn kb_linking_rate(&self) -> f64 {
        if self.total_entities == 0 {
            0.0
        } else {
            (self.kb_linked_entities as f64 / self.total_entities as f64) * 100.0
        }
    }

    /// Get Wikidata linking rate as percentage
    pub fn wikidata_linking_rate(&self) -> f64 {
        if self.total_entities == 0 {
            0.0
        } else {
            (self.wikidata_linked_entities as f64 / self.total_entities as f64) * 100.0
        }
    }
}

/// Triple with linked URIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedTriple {
    /// Subject RDF URI
    pub subject_uri: String,
    /// Original subject text
    pub subject: String,
    /// Canonical subject name
    pub subject_canonical: String,
    /// Subject type
    pub subject_type: EntityType,
    /// Predicate RDF URI
    pub predicate_uri: String,
    /// Predicate label
    pub predicate_label: String,
    /// Object RDF URI
    pub object_uri: String,
    /// Original object text
    pub object: String,
    /// Canonical object name
    pub object_canonical: String,
    /// Object type
    pub object_type: EntityType,
    /// Confidence score
    pub confidence: f32,
    /// Evidence text
    pub evidence: Option<String>,
    /// Verified flag
    pub verified: bool,
}

impl LinkedTriple {
    /// Convert to N-Triples format
    pub fn to_ntriples(&self) -> String {
        format!(
            "<{}> <{}> <{}> .",
            self.subject_uri, self.predicate_uri, self.object_uri
        )
    }

    /// Convert to Turtle format with labels
    pub fn to_turtle(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "# {} {} {}\n",
            self.subject_canonical, self.predicate_label, self.object_canonical
        ));
        output.push_str(&format!(
            "<{}> <{}> <{}> .\n",
            self.subject_uri, self.predicate_uri, self.object_uri
        ));
        output
    }
}

/// TripleStore with linked entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedTripleStore {
    /// Article ID
    pub article_id: String,
    /// Article title
    pub article_title: String,
    /// Extraction timestamp
    pub extracted_at: String,
    /// Linked entities
    pub entities: Vec<LinkedEntity>,
    /// Linked triples
    pub triples: Vec<LinkedTriple>,
}

impl LinkedTripleStore {
    /// Export to Turtle format with full namespaces
    pub fn to_turtle(&self) -> String {
        let mut output = String::new();

        // Prefixes
        output.push_str("@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n");
        output.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
        output.push_str("@prefix schema: <https://schema.org/> .\n");
        output.push_str("@prefix wd: <http://www.wikidata.org/entity/> .\n");
        output.push_str("@prefix dbpedia: <http://dbpedia.org/resource/> .\n");
        output.push_str("@prefix ntimes: <https://ntimes.example.org/ontology/> .\n");
        output.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

        // Article metadata
        output.push_str(&format!(
            "# Article: {}\n# ID: {}\n# Extracted: {}\n\n",
            self.article_title, self.article_id, self.extracted_at
        ));

        // Entity declarations
        output.push_str("# === Entities ===\n\n");
        for entity in &self.entities {
            if let Some(uri) = &entity.rdf_uri {
                output.push_str(&format!("<{uri}>\n"));
                output.push_str(&format!("    a {} ;\n", entity.entity_type.rdf_type()));
                output.push_str(&format!(
                    "    rdfs:label \"{}\"@ko ;\n",
                    escape_turtle_string(&entity.canonical)
                ));

                if !entity.aliases.is_empty() {
                    let aliases: Vec<String> = entity
                        .aliases
                        .iter()
                        .map(|a| format!("\"{}\"@ko", escape_turtle_string(a)))
                        .collect();
                    output.push_str(&format!("    schema:alternateName {} ;\n", aliases.join(", ")));
                }

                if let Some(qid) = entity.external_ids.get("wikidata") {
                    output.push_str(&format!("    schema:sameAs wd:{qid} ;\n"));
                }

                if let Some(dbp) = entity.external_ids.get("dbpedia") {
                    output.push_str(&format!("    schema:sameAs dbpedia:{dbp} ;\n"));
                }

                output.push_str(&format!(
                    "    ntimes:confidence \"{}\"^^xsd:float .\n\n",
                    entity.confidence
                ));
            }
        }

        // Triples
        output.push_str("# === Relations ===\n\n");
        for triple in &self.triples {
            output.push_str(&triple.to_turtle());
            if let Some(evidence) = &triple.evidence {
                output.push_str(&format!(
                    "# Evidence: {}\n",
                    evidence.chars().take(100).collect::<String>()
                ));
            }
            output.push_str(&format!("# Confidence: {:.2}, Verified: {}\n\n", triple.confidence, triple.verified));
        }

        output
    }

    /// Export to N-Triples format
    pub fn to_ntriples(&self) -> String {
        self.triples
            .iter()
            .map(|t| t.to_ntriples())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Export to JSON-LD format with @graph
    pub fn to_json_ld(&self) -> Result<String> {
        let graph: Vec<serde_json::Value> = self
            .entities
            .iter()
            .filter_map(|e| {
                e.rdf_uri.as_ref().map(|uri| {
                    let mut obj = serde_json::json!({
                        "@id": uri,
                        "@type": e.entity_type.rdf_type(),
                        "rdfs:label": {
                            "@value": e.canonical,
                            "@language": "ko"
                        }
                    });

                    if let Some(qid) = e.external_ids.get("wikidata") {
                        obj["schema:sameAs"] = serde_json::json!({
                            "@id": format!("http://www.wikidata.org/entity/{}", qid)
                        });
                    }

                    obj
                })
            })
            .collect();

        let json_ld = serde_json::json!({
            "@context": {
                "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
                "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
                "schema": "https://schema.org/",
                "wd": "http://www.wikidata.org/entity/",
                "ntimes": "https://ntimes.example.org/ontology/"
            },
            "@graph": graph,
            "ntimes:articleId": self.article_id,
            "ntimes:articleTitle": self.article_title,
            "ntimes:extractedAt": self.extracted_at
        });

        serde_json::to_string_pretty(&json_ld)
            .map_err(|e| anyhow::anyhow!("Failed to serialize JSON-LD: {e}"))
    }

    /// Export to RDF/XML format
    pub fn to_rdf_xml(&self) -> String {
        let mut output = String::new();

        output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        output.push_str("<rdf:RDF\n");
        output.push_str("    xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"\n");
        output.push_str("    xmlns:rdfs=\"http://www.w3.org/2000/01/rdf-schema#\"\n");
        output.push_str("    xmlns:schema=\"https://schema.org/\"\n");
        output.push_str("    xmlns:ntimes=\"https://ntimes.example.org/ontology/\">\n\n");

        // Entities
        for entity in &self.entities {
            if let Some(uri) = &entity.rdf_uri {
                output.push_str(&format!("  <rdf:Description rdf:about=\"{}\">\n", xml_escape(uri)));
                output.push_str(&format!(
                    "    <rdf:type rdf:resource=\"{}\"/>\n",
                    entity.entity_type.rdf_type().replace("schema:", "https://schema.org/")
                ));
                output.push_str(&format!(
                    "    <rdfs:label xml:lang=\"ko\">{}</rdfs:label>\n",
                    xml_escape(&entity.canonical)
                ));

                if let Some(qid) = entity.external_ids.get("wikidata") {
                    output.push_str(&format!(
                        "    <schema:sameAs rdf:resource=\"http://www.wikidata.org/entity/{qid}\"/>\n"
                    ));
                }

                output.push_str("  </rdf:Description>\n\n");
            }
        }

        // Triples as statements
        for triple in &self.triples {
            output.push_str(&format!("  <rdf:Description rdf:about=\"{}\">\n", xml_escape(&triple.subject_uri)));
            output.push_str(&format!(
                "    <{} rdf:resource=\"{}\"/>\n",
                &triple.predicate_uri,
                xml_escape(&triple.object_uri)
            ));
            output.push_str("  </rdf:Description>\n\n");
        }

        output.push_str("</rdf:RDF>\n");
        output
    }

    /// Get only verified triples
    pub fn verified_triples(&self) -> Vec<&LinkedTriple> {
        self.triples.iter().filter(|t| t.verified).collect()
    }

    /// Get entities with Wikidata links
    pub fn wikidata_entities(&self) -> Vec<&LinkedEntity> {
        self.entities
            .iter()
            .filter(|e| e.external_ids.contains_key("wikidata"))
            .collect()
    }
}

/// Escape string for Turtle format
fn escape_turtle_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Escape string for XML
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::extractor::{EntitySource, ExtractedRelation, RelationType};

    #[test]
    fn test_linker_config_default() {
        let config = LinkerConfig::default();
        assert!(config.fuzzy_matching);
        assert!(config.normalize_titles);
    }

    #[test]
    fn test_linker_config_builder() {
        let config = LinkerConfig::builder()
            .similarity_threshold(0.9)
            .fuzzy_matching(false)
            .normalize_titles(false)
            .enable_cache(false)
            .build()
            .unwrap();

        assert!((config.similarity_threshold - 0.9).abs() < 0.01);
        assert!(!config.fuzzy_matching);
        assert!(!config.normalize_titles);
        assert!(!config.enable_cache);
    }

    #[test]
    fn test_linker_config_builder_validation() {
        // Invalid similarity threshold
        let result = LinkerConfig::builder()
            .similarity_threshold(1.5)
            .build();
        assert!(result.is_err());

        let result = LinkerConfig::builder()
            .similarity_threshold(-0.1)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_linker_config_presets() {
        let strict = LinkerConfig::strict();
        assert!((strict.similarity_threshold - 0.9).abs() < 0.01);
        assert!(!strict.fuzzy_matching);

        let lenient = LinkerConfig::lenient();
        assert!((lenient.similarity_threshold - 0.6).abs() < 0.01);
        assert!(lenient.fuzzy_matching);
    }

    #[test]
    fn test_normalize_person_name() {
        let linker = EntityLinker::new();
        let normalized = linker.normalize_text("이재명 대표", EntityType::Person);
        assert_eq!(normalized, "이재명");
    }

    #[test]
    fn test_normalize_with_quotes() {
        let linker = EntityLinker::new();
        let normalized = linker.normalize_text("\"삼성전자\"", EntityType::Organization);
        assert_eq!(normalized, "삼성전자");
    }

    #[test]
    fn test_link_known_entity() {
        let mut linker = EntityLinker::new();

        let entity = ExtractedEntity {
            text: "민주당".to_string(),
            canonical_name: None,
            entity_type: EntityType::Organization,
            start: 0,
            end: 3,
            confidence: 0.8,
            source: EntitySource::Content,
        };

        let linked = linker.link(&entity);

        assert_eq!(linked.canonical, "더불어민주당");
        assert!(linked.confidence > 0.9);
        assert!(linked.in_knowledge_base);
        assert!(linked.external_ids.contains_key("wikidata"));
    }

    #[test]
    fn test_link_unknown_entity() {
        let mut linker = EntityLinker::new();

        let entity = ExtractedEntity {
            text: "테스트회사".to_string(),
            canonical_name: None,
            entity_type: EntityType::Organization,
            start: 0,
            end: 5,
            confidence: 0.7,
            source: EntitySource::Content,
        };

        let linked = linker.link(&entity);

        // Should return normalized form since not in KB
        assert_eq!(linked.canonical, "테스트회사");
        assert!(linked.aliases.is_empty());
        assert!(!linked.in_knowledge_base);
        assert!(linked.rdf_uri.is_some());
    }

    #[test]
    fn test_link_with_wikidata() {
        let mut linker = EntityLinker::new();

        let entity = ExtractedEntity {
            text: "삼성전자".to_string(),
            canonical_name: None,
            entity_type: EntityType::Organization,
            start: 0,
            end: 4,
            confidence: 0.9,
            source: EntitySource::Content,
        };

        let linked = linker.link(&entity);
        assert!(linked.external_ids.contains_key("wikidata"));
        assert_eq!(linked.external_ids.get("wikidata"), Some(&"Q20718".to_string()));
        assert!(linked.rdf_uri.is_some());
        assert!(linked.rdf_uri.as_ref().unwrap().contains("wikidata.org"));
    }

    #[test]
    fn test_similarity() {
        let linker = EntityLinker::new();

        assert_eq!(linker.similarity("삼성", "삼성"), 1.0);
        // "삼성" is contained in "삼성전자", ratio is 6/12 = 0.5 (byte lengths)
        assert!(linker.similarity("삼성전자", "삼성") >= 0.5);
        assert_eq!(linker.similarity("", "test"), 0.0);
    }

    #[test]
    fn test_knowledge_base_size() {
        let linker = EntityLinker::new();
        assert!(linker.knowledge_base_size() > 0);
    }

    #[test]
    fn test_cache() {
        let mut linker = EntityLinker::new();

        let entity = ExtractedEntity {
            text: "삼성전자".to_string(),
            canonical_name: None,
            entity_type: EntityType::Organization,
            start: 0,
            end: 4,
            confidence: 0.9,
            source: EntitySource::Content,
        };

        // First call populates cache
        let _ = linker.link(&entity);

        // Second call should use cache
        let linked = linker.link(&entity);
        assert_eq!(linked.canonical, "삼성전자");

        // Clear and verify
        linker.clear_cache();
    }

    #[test]
    fn test_export_import_knowledge_base() {
        let linker = EntityLinker::new();

        let json = linker.export_knowledge_base().unwrap();
        assert!(json.contains("삼성전자"));
        assert!(json.contains("wikidata"));

        let mut new_linker = EntityLinker::with_config(LinkerConfig {
            enable_cache: false,
            ..Default::default()
        });
        new_linker.knowledge_base.clear();
        new_linker.alias_map.clear();

        let count = new_linker.import_knowledge_base(&json).unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_linked_entity_rdf_uri() {
        let entity = LinkedEntity {
            original: "삼성".to_string(),
            canonical: "삼성전자".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec![],
            external_id: Some("Q20718".to_string()),
            external_ids: [("wikidata".to_string(), "Q20718".to_string())].into(),
            confidence: 0.95,
            rdf_uri: None,
            in_knowledge_base: true,
        };

        let uri = entity.generate_rdf_uri("https://ntimes.example.org/entity/");
        assert!(uri.contains("wikidata.org"));
        assert!(uri.contains("Q20718"));
    }

    #[test]
    fn test_linked_entity_dbpedia_uri() {
        let entity = LinkedEntity {
            original: "서울".to_string(),
            canonical: "서울".to_string(),
            entity_type: EntityType::Location,
            aliases: vec![],
            external_id: None,
            external_ids: [("dbpedia".to_string(), "Seoul".to_string())].into(),
            confidence: 0.9,
            rdf_uri: None,
            in_knowledge_base: true,
        };

        let uri = entity.dbpedia_uri();
        assert!(uri.is_some());
        assert!(uri.unwrap().contains("dbpedia.org/resource/Seoul"));
    }

    #[test]
    fn test_link_extraction_result() {
        let mut linker = EntityLinker::new();

        let result = ExtractionResult {
            article_id: "test_001".to_string(),
            entities: vec![
                ExtractedEntity {
                    text: "삼성전자".to_string(),
                    canonical_name: None,
                    entity_type: EntityType::Organization,
                    start: 0,
                    end: 4,
                    confidence: 0.9,
                    source: EntitySource::Content,
                },
                ExtractedEntity {
                    text: "이재용".to_string(),
                    canonical_name: None,
                    entity_type: EntityType::Person,
                    start: 5,
                    end: 8,
                    confidence: 0.85,
                    source: EntitySource::Content,
                },
            ],
            relations: vec![
                ExtractedRelation {
                    subject: "이재용".to_string(),
                    subject_type: EntityType::Person,
                    predicate: RelationType::Leads,
                    object: "삼성전자".to_string(),
                    object_type: EntityType::Organization,
                    confidence: 0.8,
                    evidence: "이재용 회장".to_string(),
                    verified: true,
                },
            ],
        };

        let linked = linker.link_extraction_result(&result);

        assert_eq!(linked.stats.total_entities, 2);
        assert!(linked.stats.kb_linked_entities > 0);
        assert!(linked.stats.wikidata_linked_entities > 0);
        assert_eq!(linked.relations.len(), 1);
        assert!(linked.relations[0].subject_uri.is_some());
    }

    #[test]
    fn test_linking_stats() {
        let stats = LinkingStats {
            total_entities: 10,
            kb_linked_entities: 5,
            wikidata_linked_entities: 3,
            total_relations: 4,
        };

        assert_eq!(stats.kb_linking_rate(), 50.0);
        assert_eq!(stats.wikidata_linking_rate(), 30.0);
    }

    #[test]
    fn test_linked_triple_to_ntriples() {
        let triple = LinkedTriple {
            subject_uri: "http://example.org/entity/A".to_string(),
            subject: "A".to_string(),
            subject_canonical: "A".to_string(),
            subject_type: EntityType::Person,
            predicate_uri: "schema:worksFor".to_string(),
            predicate_label: "근무".to_string(),
            object_uri: "http://example.org/entity/B".to_string(),
            object: "B".to_string(),
            object_canonical: "B".to_string(),
            object_type: EntityType::Organization,
            confidence: 0.9,
            evidence: None,
            verified: true,
        };

        let ntriples = triple.to_ntriples();
        assert!(ntriples.contains("<http://example.org/entity/A>"));
        assert!(ntriples.ends_with(" ."));
    }

    #[test]
    fn test_linked_triple_store_to_turtle() {
        let store = LinkedTripleStore {
            article_id: "test_001".to_string(),
            article_title: "테스트 기사".to_string(),
            extracted_at: "2024-01-01T00:00:00Z".to_string(),
            entities: vec![
                LinkedEntity {
                    original: "삼성전자".to_string(),
                    canonical: "삼성전자".to_string(),
                    entity_type: EntityType::Organization,
                    aliases: vec!["삼성".to_string()],
                    external_id: Some("Q20718".to_string()),
                    external_ids: [("wikidata".to_string(), "Q20718".to_string())].into(),
                    confidence: 0.95,
                    rdf_uri: Some("http://www.wikidata.org/entity/Q20718".to_string()),
                    in_knowledge_base: true,
                },
            ],
            triples: vec![],
        };

        let turtle = store.to_turtle();
        assert!(turtle.contains("@prefix"));
        assert!(turtle.contains("schema:"));
        assert!(turtle.contains("rdfs:label"));
        assert!(turtle.contains("wd:Q20718"));
    }

    #[test]
    fn test_linked_triple_store_to_json_ld() {
        let store = LinkedTripleStore {
            article_id: "test_001".to_string(),
            article_title: "테스트 기사".to_string(),
            extracted_at: "2024-01-01T00:00:00Z".to_string(),
            entities: vec![
                LinkedEntity {
                    original: "서울".to_string(),
                    canonical: "서울".to_string(),
                    entity_type: EntityType::Location,
                    aliases: vec![],
                    external_id: Some("Q8684".to_string()),
                    external_ids: [("wikidata".to_string(), "Q8684".to_string())].into(),
                    confidence: 0.9,
                    rdf_uri: Some("http://www.wikidata.org/entity/Q8684".to_string()),
                    in_knowledge_base: true,
                },
            ],
            triples: vec![],
        };

        let json_ld = store.to_json_ld().unwrap();
        assert!(json_ld.contains("@context"));
        assert!(json_ld.contains("@graph"));
        assert!(json_ld.contains("wikidata.org"));
    }

    #[test]
    fn test_linked_triple_store_to_rdf_xml() {
        let store = LinkedTripleStore {
            article_id: "test_001".to_string(),
            article_title: "테스트".to_string(),
            extracted_at: "2024-01-01".to_string(),
            entities: vec![],
            triples: vec![],
        };

        let xml = store.to_rdf_xml();
        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<rdf:RDF"));
        assert!(xml.contains("</rdf:RDF>"));
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("hello world"), "hello_world");
        assert!(url_encode("삼성전자").contains("%"));
    }

    #[test]
    fn test_escape_functions() {
        assert_eq!(escape_turtle_string("test\"quote"), "test\\\"quote");
        assert_eq!(xml_escape("<test>"), "&lt;test&gt;");
    }

    #[test]
    fn test_wikidata_entities() {
        let linker = EntityLinker::new();
        let wikidata_entities = linker.wikidata_entities();
        assert!(!wikidata_entities.is_empty());

        for entry in wikidata_entities {
            assert!(entry.external_ids.contains_key("wikidata"));
        }
    }

    #[test]
    fn test_lookup_entity() {
        let linker = EntityLinker::new();

        let samsung = linker.lookup("삼성전자");
        assert!(samsung.is_some());
        assert_eq!(samsung.unwrap().entity_type, EntityType::Organization);

        let unknown = linker.lookup("존재하지않는회사");
        assert!(unknown.is_none());
    }
}
