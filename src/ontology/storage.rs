//! Triple storage and persistence
//!
//! This module provides file-based storage for extracted triples:
//! - JSON file persistence for TripleStore
//! - Incremental updates and appending
//! - Index-based retrieval by article ID
//! - Batch operations for multiple articles

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use super::extractor::{ExtractionResult, TripleStore};

/// Storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Base directory for storage
    pub base_dir: PathBuf,

    /// Whether to create directories if they don't exist
    pub create_dirs: bool,

    /// Pretty print JSON output
    pub pretty_json: bool,

    /// Enable compression (future feature)
    pub compress: bool,

    /// Maximum triples per file (0 = unlimited)
    pub max_triples_per_file: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("data/triples"),
            create_dirs: true,
            pretty_json: true,
            compress: false,
            max_triples_per_file: 0,
        }
    }
}

impl StorageConfig {
    /// Create a new builder for StorageConfig
    pub fn builder() -> StorageConfigBuilder {
        StorageConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), super::error::OntologyError> {
        // Check if base_dir is not empty
        if self.base_dir.as_os_str().is_empty() {
            return Err(super::error::OntologyError::invalid_config(
                "base_dir",
                "",
                "Base directory cannot be empty",
            ));
        }

        // Check if base_dir exists or can be created
        if !self.create_dirs && !self.base_dir.exists() {
            return Err(super::error::OntologyError::StorageDirectoryNotFound {
                path: self.base_dir.clone(),
            });
        }

        Ok(())
    }

    /// Get the index file path
    pub fn index_path(&self) -> PathBuf {
        self.base_dir.join("index.json")
    }

    /// Get the triples directory path
    pub fn triples_dir(&self) -> PathBuf {
        self.base_dir.join("triples")
    }
}

/// Builder for StorageConfig with fluent API
#[derive(Debug, Clone, Default)]
pub struct StorageConfigBuilder {
    base_dir: Option<PathBuf>,
    create_dirs: Option<bool>,
    pretty_json: Option<bool>,
    compress: Option<bool>,
    max_triples_per_file: Option<usize>,
}

impl StorageConfigBuilder {
    /// Set base directory for storage
    pub fn base_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(path.into());
        self
    }

    /// Enable or disable directory creation
    pub fn create_dirs(mut self, enable: bool) -> Self {
        self.create_dirs = Some(enable);
        self
    }

    /// Enable or disable pretty JSON output
    pub fn pretty_json(mut self, enable: bool) -> Self {
        self.pretty_json = Some(enable);
        self
    }

    /// Enable or disable compression
    pub fn compress(mut self, enable: bool) -> Self {
        self.compress = Some(enable);
        self
    }

    /// Set maximum triples per file (0 = unlimited)
    pub fn max_triples_per_file(mut self, max: usize) -> Self {
        self.max_triples_per_file = Some(max);
        self
    }

    /// Build the config with validation
    pub fn build(self) -> Result<StorageConfig, super::error::OntologyError> {
        let config = StorageConfig {
            base_dir: self
                .base_dir
                .unwrap_or_else(|| PathBuf::from("data/triples")),
            create_dirs: self.create_dirs.unwrap_or(true),
            pretty_json: self.pretty_json.unwrap_or(true),
            compress: self.compress.unwrap_or(false),
            max_triples_per_file: self.max_triples_per_file.unwrap_or(0),
        };
        config.validate()?;
        Ok(config)
    }

    /// Build without validation
    pub fn build_unchecked(self) -> StorageConfig {
        StorageConfig {
            base_dir: self
                .base_dir
                .unwrap_or_else(|| PathBuf::from("data/triples")),
            create_dirs: self.create_dirs.unwrap_or(true),
            pretty_json: self.pretty_json.unwrap_or(true),
            compress: self.compress.unwrap_or(false),
            max_triples_per_file: self.max_triples_per_file.unwrap_or(0),
        }
    }
}

/// Index entry for quick lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Article ID
    pub article_id: String,

    /// Article title
    pub title: String,

    /// File path where triples are stored
    pub file_path: String,

    /// Number of entities
    pub entity_count: usize,

    /// Number of relations/triples
    pub triple_count: usize,

    /// Number of verified triples
    pub verified_count: usize,

    /// Extraction timestamp
    pub extracted_at: String,

    /// File size in bytes
    pub file_size: u64,
}

/// Storage index for all articles
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageIndex {
    /// Version for compatibility
    pub version: String,

    /// Last updated timestamp
    pub updated_at: String,

    /// Total articles indexed
    pub total_articles: usize,

    /// Total triples across all articles
    pub total_triples: usize,

    /// Index entries by article ID
    pub entries: HashMap<String, IndexEntry>,
}

impl StorageIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self {
            version: "1.0".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            total_articles: 0,
            total_triples: 0,
            entries: HashMap::new(),
        }
    }

    /// Add or update an entry
    pub fn upsert(&mut self, entry: IndexEntry) {
        let triple_count = entry.triple_count;
        let article_id = entry.article_id.clone();

        // Update totals
        if let Some(existing) = self.entries.get(&article_id) {
            self.total_triples -= existing.triple_count;
        } else {
            self.total_articles += 1;
        }
        self.total_triples += triple_count;

        self.entries.insert(article_id, entry);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Remove an entry
    pub fn remove(&mut self, article_id: &str) -> Option<IndexEntry> {
        if let Some(entry) = self.entries.remove(article_id) {
            self.total_articles -= 1;
            self.total_triples -= entry.triple_count;
            self.updated_at = chrono::Utc::now().to_rfc3339();
            Some(entry)
        } else {
            None
        }
    }

    /// Get entry by article ID
    pub fn get(&self, article_id: &str) -> Option<&IndexEntry> {
        self.entries.get(article_id)
    }

    /// Check if article exists
    pub fn contains(&self, article_id: &str) -> bool {
        self.entries.contains_key(article_id)
    }

    /// Get all article IDs
    pub fn article_ids(&self) -> Vec<&String> {
        self.entries.keys().collect()
    }
}

/// Triple storage manager
pub struct TripleStorage {
    /// Configuration
    config: StorageConfig,

    /// Storage index
    index: StorageIndex,

    /// Index file path
    index_path: PathBuf,
}

impl TripleStorage {
    /// Create a new storage manager
    pub fn new(config: StorageConfig) -> Result<Self> {
        let index_path = config.base_dir.join("index.json");

        // Create directories if needed
        if config.create_dirs {
            fs::create_dir_all(&config.base_dir)
                .with_context(|| format!("Failed to create directory: {:?}", config.base_dir))?;
        }

        // Load or create index
        let index = if index_path.exists() {
            Self::load_index(&index_path)?
        } else {
            StorageIndex::new()
        };

        Ok(Self {
            config,
            index,
            index_path,
        })
    }

    /// Create with default config
    pub fn with_default_config() -> Result<Self> {
        Self::new(StorageConfig::default())
    }

    /// Load index from file
    fn load_index(path: &Path) -> Result<StorageIndex> {
        let file =
            File::open(path).with_context(|| format!("Failed to open index file: {path:?}"))?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).with_context(|| "Failed to parse index file")
    }

    /// Save index to file
    fn save_index(&self) -> Result<()> {
        let file = File::create(&self.index_path)
            .with_context(|| format!("Failed to create index file: {:?}", self.index_path))?;
        let writer = BufWriter::new(file);

        if self.config.pretty_json {
            serde_json::to_writer_pretty(writer, &self.index)
        } else {
            serde_json::to_writer(writer, &self.index)
        }
        .with_context(|| "Failed to write index file")
    }

    /// Generate file path for article
    fn article_file_path(&self, article_id: &str) -> PathBuf {
        // Sanitize article ID for file name
        let safe_id: String = article_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.config.base_dir.join(format!("{safe_id}.json"))
    }

    /// Save a TripleStore to storage
    pub fn save(&mut self, store: &TripleStore) -> Result<PathBuf> {
        let file_path = self.article_file_path(&store.article_id);

        // Write triple store to file
        let file = File::create(&file_path)
            .with_context(|| format!("Failed to create file: {file_path:?}"))?;
        let writer = BufWriter::new(file);

        if self.config.pretty_json {
            serde_json::to_writer_pretty(writer, store)
        } else {
            serde_json::to_writer(writer, store)
        }
        .with_context(|| "Failed to write triple store")?;

        // Get file size
        let metadata = fs::metadata(&file_path)?;
        let file_size = metadata.len();

        // Update index
        let entry = IndexEntry {
            article_id: store.article_id.clone(),
            title: store.article_title.clone(),
            file_path: file_path.to_string_lossy().to_string(),
            entity_count: store.stats.total_entities,
            triple_count: store.stats.total_relations,
            verified_count: store.stats.verified_relations,
            extracted_at: store.extracted_at.clone(),
            file_size,
        };
        self.index.upsert(entry);
        self.save_index()?;

        Ok(file_path)
    }

    /// Save extraction result directly
    pub fn save_extraction(
        &mut self,
        result: &ExtractionResult,
        article_title: &str,
    ) -> Result<PathBuf> {
        let store = TripleStore::from_extraction(result, article_title);
        self.save(&store)
    }

    /// Load a TripleStore by article ID
    pub fn load(&self, article_id: &str) -> Result<Option<TripleStore>> {
        let entry = match self.index.get(article_id) {
            Some(e) => e,
            None => return Ok(None),
        };

        let file_path = Path::new(&entry.file_path);
        if !file_path.exists() {
            return Ok(None);
        }

        let file =
            File::open(file_path).with_context(|| format!("Failed to open file: {file_path:?}"))?;
        let reader = BufReader::new(file);
        let store: TripleStore =
            serde_json::from_reader(reader).with_context(|| "Failed to parse triple store")?;

        Ok(Some(store))
    }

    /// Delete triples for an article
    pub fn delete(&mut self, article_id: &str) -> Result<bool> {
        let entry = match self.index.remove(article_id) {
            Some(e) => e,
            None => return Ok(false),
        };

        let file_path = Path::new(&entry.file_path);
        if file_path.exists() {
            fs::remove_file(file_path)
                .with_context(|| format!("Failed to delete file: {file_path:?}"))?;
        }

        self.save_index()?;
        Ok(true)
    }

    /// Check if article exists in storage
    pub fn exists(&self, article_id: &str) -> bool {
        self.index.contains(article_id)
    }

    /// Get index entry for article
    pub fn get_entry(&self, article_id: &str) -> Option<&IndexEntry> {
        self.index.get(article_id)
    }

    /// Get all article IDs
    pub fn list_articles(&self) -> Vec<&String> {
        self.index.article_ids()
    }

    /// Get storage statistics
    pub fn stats(&self) -> StorageStats {
        let mut total_file_size = 0u64;
        let mut total_verified = 0usize;
        let mut total_entities = 0usize;

        for entry in self.index.entries.values() {
            total_file_size += entry.file_size;
            total_verified += entry.verified_count;
            total_entities += entry.entity_count;
        }

        StorageStats {
            total_articles: self.index.total_articles,
            total_triples: self.index.total_triples,
            total_entities,
            total_verified,
            total_file_size,
            index_updated_at: self.index.updated_at.clone(),
        }
    }

    /// Batch save multiple extraction results
    pub fn save_batch(&mut self, results: &[(ExtractionResult, String)]) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for (result, title) in results {
            let path = self.save_extraction(result, title)?;
            paths.push(path);
        }
        Ok(paths)
    }

    /// Load multiple TripleStores by article IDs
    pub fn load_batch(&self, article_ids: &[&str]) -> Result<Vec<TripleStore>> {
        let mut stores = Vec::new();
        for id in article_ids {
            if let Some(store) = self.load(id)? {
                stores.push(store);
            }
        }
        Ok(stores)
    }

    /// Export all triples to a single JSON file
    pub fn export_all(&self, output_path: &Path) -> Result<usize> {
        let mut all_stores: Vec<TripleStore> = Vec::new();

        for article_id in self.index.article_ids() {
            if let Some(store) = self.load(article_id)? {
                all_stores.push(store);
            }
        }

        let file = File::create(output_path)
            .with_context(|| format!("Failed to create export file: {output_path:?}"))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &all_stores)
            .with_context(|| "Failed to write export file")?;

        Ok(all_stores.len())
    }

    /// Export all triples to N-Triples format
    pub fn export_ntriples(&self, output_path: &Path) -> Result<usize> {
        let mut file = File::create(output_path)
            .with_context(|| format!("Failed to create export file: {output_path:?}"))?;

        let mut total_triples = 0;

        for article_id in self.index.article_ids() {
            if let Some(store) = self.load(article_id)? {
                let ntriples = store.to_ntriples();
                if !ntriples.is_empty() {
                    writeln!(file, "# Article: {}", store.article_title)?;
                    writeln!(file, "{ntriples}")?;
                    writeln!(file)?;
                    total_triples += store.triples.len();
                }
            }
        }

        Ok(total_triples)
    }

    /// Export all triples to Turtle format
    pub fn export_turtle(&self, output_path: &Path) -> Result<usize> {
        let mut file = File::create(output_path)
            .with_context(|| format!("Failed to create export file: {output_path:?}"))?;

        // Write prefixes once
        writeln!(file, "@prefix schema: <https://schema.org/> .")?;
        writeln!(
            file,
            "@prefix ktime: <https://ktime.example.org/ontology/> ."
        )?;
        writeln!(file, "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .")?;
        writeln!(file)?;

        let mut total_triples = 0;

        for article_id in self.index.article_ids() {
            if let Some(store) = self.load(article_id)? {
                writeln!(file, "# ============================================")?;
                writeln!(file, "# Article: {}", store.article_title)?;
                writeln!(file, "# ID: {}", store.article_id)?;
                writeln!(file, "# Extracted: {}", store.extracted_at)?;
                writeln!(file, "# ============================================")?;

                for triple in &store.triples {
                    writeln!(file, "{}", triple.to_turtle())?;
                    if let Some(evidence) = &triple.evidence {
                        writeln!(file, "  # Evidence: {evidence}")?;
                    }
                }
                writeln!(file)?;
                total_triples += store.triples.len();
            }
        }

        Ok(total_triples)
    }

    /// Rebuild index from files
    pub fn rebuild_index(&mut self) -> Result<usize> {
        let mut new_index = StorageIndex::new();
        let mut count = 0;

        // Scan directory for JSON files
        for entry in fs::read_dir(&self.config.base_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false)
                && path.file_name().map(|n| n != "index.json").unwrap_or(false)
            {
                // Try to load as TripleStore
                if let Ok(file) = File::open(&path) {
                    let reader = BufReader::new(file);
                    if let Ok(store) = serde_json::from_reader::<_, TripleStore>(reader) {
                        let metadata = fs::metadata(&path)?;
                        let index_entry = IndexEntry {
                            article_id: store.article_id.clone(),
                            title: store.article_title.clone(),
                            file_path: path.to_string_lossy().to_string(),
                            entity_count: store.stats.total_entities,
                            triple_count: store.stats.total_relations,
                            verified_count: store.stats.verified_relations,
                            extracted_at: store.extracted_at,
                            file_size: metadata.len(),
                        };
                        new_index.upsert(index_entry);
                        count += 1;
                    }
                }
            }
        }

        self.index = new_index;
        self.save_index()?;
        Ok(count)
    }

    /// Get the base directory path
    pub fn base_dir(&self) -> &Path {
        &self.config.base_dir
    }
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Total articles stored
    pub total_articles: usize,

    /// Total triples across all articles
    pub total_triples: usize,

    /// Total entities across all articles
    pub total_entities: usize,

    /// Total verified triples
    pub total_verified: usize,

    /// Total file size in bytes
    pub total_file_size: u64,

    /// Index last updated
    pub index_updated_at: String,
}

impl StorageStats {
    /// Get verification rate as percentage
    pub fn verification_rate(&self) -> f64 {
        if self.total_triples == 0 {
            0.0
        } else {
            (self.total_verified as f64 / self.total_triples as f64) * 100.0
        }
    }

    /// Get average triples per article
    pub fn avg_triples_per_article(&self) -> f64 {
        if self.total_articles == 0 {
            0.0
        } else {
            self.total_triples as f64 / self.total_articles as f64
        }
    }

    /// Get human-readable file size
    pub fn file_size_human(&self) -> String {
        let size = self.total_file_size;
        if size < 1024 {
            format!("{size} B")
        } else if size < 1024 * 1024 {
            format!("{:.1} KB", size as f64 / 1024.0)
        } else if size < 1024 * 1024 * 1024 {
            format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::extractor::{
        EntitySource, EntityType, ExtractedEntity, ExtractedRelation, RelationType,
    };
    use tempfile::TempDir;

    fn create_test_storage() -> (TripleStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            base_dir: temp_dir.path().to_path_buf(),
            create_dirs: true,
            pretty_json: true,
            compress: false,
            max_triples_per_file: 0,
        };
        let storage = TripleStorage::new(config).unwrap();
        (storage, temp_dir)
    }

    fn create_test_result() -> ExtractionResult {
        ExtractionResult {
            article_id: "001_0001".to_string(),
            entities: vec![
                ExtractedEntity {
                    text: "삼성전자".to_string(),
                    canonical_name: None,
                    entity_type: EntityType::Organization,
                    start: 0,
                    end: 4,
                    confidence: 0.9,
                    source: EntitySource::Title,
                },
                ExtractedEntity {
                    text: "이재용".to_string(),
                    canonical_name: None,
                    entity_type: EntityType::Person,
                    start: 5,
                    end: 8,
                    confidence: 0.85,
                    source: EntitySource::Title,
                },
            ],
            relations: vec![ExtractedRelation {
                subject: "이재용".to_string(),
                subject_type: EntityType::Person,
                predicate: RelationType::Leads,
                object: "삼성전자".to_string(),
                object_type: EntityType::Organization,
                confidence: 0.9,
                evidence: "이재용 회장이 이끄는 삼성전자".to_string(),
                verified: true,
            }],
        }
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert!(config.create_dirs);
        assert!(config.pretty_json);
    }

    #[test]
    fn test_storage_config_builder() {
        let config = StorageConfig::builder()
            .base_dir("/tmp/test_triples")
            .create_dirs(false)
            .pretty_json(false)
            .max_triples_per_file(1000)
            .build_unchecked();

        assert_eq!(
            config.base_dir,
            std::path::PathBuf::from("/tmp/test_triples")
        );
        assert!(!config.create_dirs);
        assert!(!config.pretty_json);
        assert_eq!(config.max_triples_per_file, 1000);
    }

    #[test]
    fn test_storage_config_builder_validation() {
        // Empty base_dir should fail
        let config = StorageConfig {
            base_dir: std::path::PathBuf::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_storage_config_paths() {
        let config = StorageConfig::default();
        assert!(config.index_path().ends_with("index.json"));
        assert!(config.triples_dir().ends_with("triples"));
    }

    #[test]
    fn test_storage_index_new() {
        let index = StorageIndex::new();
        assert_eq!(index.total_articles, 0);
        assert_eq!(index.total_triples, 0);
        assert_eq!(index.version, "1.0");
    }

    #[test]
    fn test_storage_index_upsert() {
        let mut index = StorageIndex::new();

        let entry = IndexEntry {
            article_id: "test_001".to_string(),
            title: "Test Article".to_string(),
            file_path: "/tmp/test.json".to_string(),
            entity_count: 5,
            triple_count: 3,
            verified_count: 2,
            extracted_at: "2024-01-01T00:00:00Z".to_string(),
            file_size: 1024,
        };

        index.upsert(entry);
        assert_eq!(index.total_articles, 1);
        assert_eq!(index.total_triples, 3);
        assert!(index.contains("test_001"));
    }

    #[test]
    fn test_storage_save_load() {
        let (mut storage, _temp_dir) = create_test_storage();
        let result = create_test_result();

        // Save
        let path = storage.save_extraction(&result, "테스트 기사").unwrap();
        assert!(path.exists());

        // Load
        let loaded = storage.load("001_0001").unwrap().unwrap();
        assert_eq!(loaded.article_id, "001_0001");
        assert_eq!(loaded.triples.len(), 1);
    }

    #[test]
    fn test_storage_delete() {
        let (mut storage, _temp_dir) = create_test_storage();
        let result = create_test_result();

        storage.save_extraction(&result, "테스트 기사").unwrap();
        assert!(storage.exists("001_0001"));

        let deleted = storage.delete("001_0001").unwrap();
        assert!(deleted);
        assert!(!storage.exists("001_0001"));
    }

    #[test]
    fn test_storage_stats() {
        let (mut storage, _temp_dir) = create_test_storage();
        let result = create_test_result();

        storage.save_extraction(&result, "테스트 기사").unwrap();

        let stats = storage.stats();
        assert_eq!(stats.total_articles, 1);
        assert_eq!(stats.total_triples, 1);
        assert!(stats.total_file_size > 0);
    }

    #[test]
    fn test_storage_list_articles() {
        let (mut storage, _temp_dir) = create_test_storage();
        let result = create_test_result();

        storage.save_extraction(&result, "테스트 기사").unwrap();

        let articles = storage.list_articles();
        assert_eq!(articles.len(), 1);
        assert!(articles.contains(&&"001_0001".to_string()));
    }

    #[test]
    fn test_storage_batch_save() {
        let (mut storage, _temp_dir) = create_test_storage();

        let result1 = create_test_result();
        let mut result2 = create_test_result();
        result2.article_id = "001_0002".to_string();

        let batch = vec![
            (result1, "기사 1".to_string()),
            (result2, "기사 2".to_string()),
        ];

        let paths = storage.save_batch(&batch).unwrap();
        assert_eq!(paths.len(), 2);

        let stats = storage.stats();
        assert_eq!(stats.total_articles, 2);
    }

    #[test]
    fn test_storage_export_ntriples() {
        let (mut storage, temp_dir) = create_test_storage();
        let result = create_test_result();
        storage.save_extraction(&result, "테스트 기사").unwrap();

        let export_path = temp_dir.path().join("export.nt");
        let count = storage.export_ntriples(&export_path).unwrap();

        assert_eq!(count, 1);
        assert!(export_path.exists());
    }

    #[test]
    fn test_storage_stats_methods() {
        let stats = StorageStats {
            total_articles: 10,
            total_triples: 50,
            total_entities: 30,
            total_verified: 40,
            total_file_size: 1536,
            index_updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(stats.verification_rate(), 80.0);
        assert_eq!(stats.avg_triples_per_article(), 5.0);
        assert_eq!(stats.file_size_human(), "1.5 KB");
    }

    #[test]
    fn test_rebuild_index() {
        let (mut storage, _temp_dir) = create_test_storage();
        let result = create_test_result();
        storage.save_extraction(&result, "테스트 기사").unwrap();

        // Clear index in memory
        storage.index = StorageIndex::new();
        assert_eq!(storage.index.total_articles, 0);

        // Rebuild
        let count = storage.rebuild_index().unwrap();
        assert_eq!(count, 1);
        assert_eq!(storage.index.total_articles, 1);
    }
}
