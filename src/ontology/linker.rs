//! Entity linking and normalization
//!
//! This module provides entity disambiguation and normalization:
//! - Canonical name resolution (이재명 대표 -> 이재명)
//! - Entity deduplication across articles
//! - Knowledge base integration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::extractor::{EntityType, ExtractedEntity};

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

    /// Linking confidence
    pub confidence: f32,
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

    /// Organization suffixes
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
        // Politicians
        self.add_entry(KnowledgeBaseEntry {
            canonical: "윤석열".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["윤석열 대통령".to_string(), "윤 대통령".to_string()],
            external_ids: HashMap::new(),
            properties: [("role".to_string(), "대통령".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "이재명".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["이재명 대표".to_string(), "이 대표".to_string()],
            external_ids: HashMap::new(),
            properties: [("party".to_string(), "더불어민주당".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "한동훈".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["한동훈 대표".to_string(), "한 대표".to_string()],
            external_ids: HashMap::new(),
            properties: [("party".to_string(), "국민의힘".to_string())].into(),
        });

        // Major companies
        self.add_entry(KnowledgeBaseEntry {
            canonical: "삼성전자".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["삼성".to_string(), "Samsung".to_string(), "Samsung Electronics".to_string()],
            external_ids: [("stock".to_string(), "005930".to_string())].into(),
            properties: [("industry".to_string(), "반도체".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "SK하이닉스".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["하이닉스".to_string(), "SK Hynix".to_string()],
            external_ids: [("stock".to_string(), "000660".to_string())].into(),
            properties: [("industry".to_string(), "반도체".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "현대자동차".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["현대차".to_string(), "Hyundai".to_string(), "현대".to_string()],
            external_ids: [("stock".to_string(), "005380".to_string())].into(),
            properties: [("industry".to_string(), "자동차".to_string())].into(),
        });

        // Political parties
        self.add_entry(KnowledgeBaseEntry {
            canonical: "국민의힘".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["국힘".to_string(), "여당".to_string()],
            external_ids: HashMap::new(),
            properties: [("type".to_string(), "정당".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "더불어민주당".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["민주당".to_string(), "더민주".to_string(), "야당".to_string()],
            external_ids: HashMap::new(),
            properties: [("type".to_string(), "정당".to_string())].into(),
        });

        // Government bodies
        self.add_entry(KnowledgeBaseEntry {
            canonical: "기획재정부".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["기재부".to_string()],
            external_ids: HashMap::new(),
            properties: [("type".to_string(), "정부부처".to_string())].into(),
        });

        self.add_entry(KnowledgeBaseEntry {
            canonical: "대한민국".to_string(),
            entity_type: EntityType::Location,
            aliases: vec!["한국".to_string(), "South Korea".to_string(), "Korea".to_string()],
            external_ids: [("iso".to_string(), "KR".to_string())].into(),
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
                let linked = LinkedEntity {
                    original: entity.text.clone(),
                    canonical: canonical.clone(),
                    entity_type: kb_entry.entity_type,
                    aliases: kb_entry.aliases.clone(),
                    external_id: kb_entry.external_ids.get("wikidata").cloned(),
                    confidence: 0.95,
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
                        let linked = LinkedEntity {
                            original: entity.text.clone(),
                            canonical: canonical.clone(),
                            entity_type: kb_entry.entity_type,
                            aliases: kb_entry.aliases.clone(),
                            external_id: kb_entry.external_ids.get("wikidata").cloned(),
                            confidence: similarity,
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
            canonical: normalized,
            entity_type: entity.entity_type,
            aliases: vec![],
            external_id: None,
            confidence: entity.confidence,
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
                        let with_space = format!(" {}", suffix);
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

        // Remove quotes
        normalized = normalized.trim_matches(|c| c == '\'' || c == '"' || c == '"' || c == '"').to_string();

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
}

impl Default for EntityLinker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linker_config_default() {
        let config = LinkerConfig::default();
        assert!(config.fuzzy_matching);
        assert!(config.normalize_titles);
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
            source: super::super::extractor::EntitySource::Content,
        };

        let linked = linker.link(&entity);

        assert_eq!(linked.canonical, "더불어민주당");
        assert!(linked.confidence > 0.9);
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
            source: super::super::extractor::EntitySource::Content,
        };

        let linked = linker.link(&entity);

        // Should return normalized form since not in KB
        assert_eq!(linked.canonical, "테스트회사");
        assert!(linked.aliases.is_empty());
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
            source: super::super::extractor::EntitySource::Content,
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

        let mut new_linker = EntityLinker::with_config(LinkerConfig {
            enable_cache: false,
            ..Default::default()
        });
        new_linker.knowledge_base.clear();
        new_linker.alias_map.clear();

        let count = new_linker.import_knowledge_base(&json).unwrap();
        assert!(count > 0);
    }
}
