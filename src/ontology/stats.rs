//! Statistics and profiling for ontology extraction
//!
//! This module provides performance monitoring and memory estimation
//! for the ontology extraction pipeline.
//!
//! # Features
//!
//! - Extraction statistics (entity counts, relation counts, timing)
//! - Memory usage estimation
//! - Batch processing metrics
//! - Pipeline profiling

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::extractor::{EntityType, RelationType};

// ============================================================================
// Extraction Statistics
// ============================================================================

/// Statistics for a single extraction operation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionStats {
    /// Article ID
    pub article_id: String,

    /// Number of entities extracted
    pub entity_count: usize,

    /// Number of relations extracted
    pub relation_count: usize,

    /// Number of triples generated
    pub triple_count: usize,

    /// Number of verified relations
    pub verified_count: usize,

    /// Entity counts by type
    pub entities_by_type: HashMap<String, usize>,

    /// Relation counts by type
    pub relations_by_type: HashMap<String, usize>,

    /// Extraction duration in milliseconds
    pub duration_ms: u64,

    /// Estimated memory usage in bytes
    pub memory_bytes: usize,

    /// Source text length (characters)
    pub source_length: usize,

    /// Average confidence score
    pub avg_confidence: f32,
}

impl ExtractionStats {
    /// Create new stats for an article
    pub fn new(article_id: impl Into<String>) -> Self {
        Self {
            article_id: article_id.into(),
            ..Default::default()
        }
    }

    /// Record entity extraction
    pub fn record_entity(&mut self, entity_type: EntityType) {
        self.entity_count += 1;
        *self
            .entities_by_type
            .entry(entity_type.korean_label().to_string())
            .or_insert(0) += 1;
    }

    /// Record relation extraction
    pub fn record_relation(&mut self, relation_type: RelationType, confidence: f32, verified: bool) {
        self.relation_count += 1;
        if verified {
            self.verified_count += 1;
        }
        *self
            .relations_by_type
            .entry(relation_type.korean_label().to_string())
            .or_insert(0) += 1;

        // Update average confidence
        let total_conf = self.avg_confidence * (self.relation_count - 1) as f32 + confidence;
        self.avg_confidence = total_conf / self.relation_count as f32;
    }

    /// Set duration from Instant
    pub fn set_duration(&mut self, start: Instant) {
        self.duration_ms = start.elapsed().as_millis() as u64;
    }

    /// Set duration directly
    pub fn set_duration_ms(&mut self, ms: u64) {
        self.duration_ms = ms;
    }

    /// Estimate memory usage based on counts
    pub fn estimate_memory(&mut self) {
        // Rough estimates per item
        const ENTITY_SIZE: usize = 256; // ExtractedEntity average
        const RELATION_SIZE: usize = 384; // ExtractedRelation average
        const TRIPLE_SIZE: usize = 512; // Triple average

        self.memory_bytes = self.entity_count * ENTITY_SIZE
            + self.relation_count * RELATION_SIZE
            + self.triple_count * TRIPLE_SIZE;
    }

    /// Get verification rate as percentage
    pub fn verification_rate(&self) -> f64 {
        if self.relation_count == 0 {
            0.0
        } else {
            (self.verified_count as f64 / self.relation_count as f64) * 100.0
        }
    }

    /// Get extraction speed (entities per second)
    pub fn entities_per_second(&self) -> f64 {
        if self.duration_ms == 0 {
            0.0
        } else {
            self.entity_count as f64 / (self.duration_ms as f64 / 1000.0)
        }
    }

    /// Get extraction speed (characters per second)
    pub fn chars_per_second(&self) -> f64 {
        if self.duration_ms == 0 {
            0.0
        } else {
            self.source_length as f64 / (self.duration_ms as f64 / 1000.0)
        }
    }

    /// Format memory as human-readable string
    pub fn memory_formatted(&self) -> String {
        format_bytes(self.memory_bytes)
    }

    /// Get summary as formatted string
    pub fn summary(&self) -> String {
        format!(
            "Article: {} | Entities: {} | Relations: {} ({:.1}% verified) | Time: {}ms | Memory: {}",
            self.article_id,
            self.entity_count,
            self.relation_count,
            self.verification_rate(),
            self.duration_ms,
            self.memory_formatted()
        )
    }
}

// ============================================================================
// Batch Statistics
// ============================================================================

/// Statistics for batch extraction operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchStats {
    /// Total articles processed
    pub total_articles: usize,

    /// Successful extractions
    pub successful: usize,

    /// Failed extractions
    pub failed: usize,

    /// Total entities extracted
    pub total_entities: usize,

    /// Total relations extracted
    pub total_relations: usize,

    /// Total verified relations
    pub total_verified: usize,

    /// Total processing time in milliseconds
    pub total_duration_ms: u64,

    /// Total estimated memory usage
    pub total_memory_bytes: usize,

    /// Per-article statistics
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub article_stats: Vec<ExtractionStats>,

    /// Aggregate entity counts by type
    pub entities_by_type: HashMap<String, usize>,

    /// Aggregate relation counts by type
    pub relations_by_type: HashMap<String, usize>,

    /// Error messages for failed extractions
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

impl BatchStats {
    /// Create new batch stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Add stats from a successful extraction
    pub fn add_success(&mut self, stats: ExtractionStats) {
        self.total_articles += 1;
        self.successful += 1;
        self.total_entities += stats.entity_count;
        self.total_relations += stats.relation_count;
        self.total_verified += stats.verified_count;
        self.total_duration_ms += stats.duration_ms;
        self.total_memory_bytes += stats.memory_bytes;

        // Merge type counts
        for (k, v) in &stats.entities_by_type {
            *self.entities_by_type.entry(k.clone()).or_insert(0) += v;
        }
        for (k, v) in &stats.relations_by_type {
            *self.relations_by_type.entry(k.clone()).or_insert(0) += v;
        }

        self.article_stats.push(stats);
    }

    /// Record a failed extraction
    pub fn add_failure(&mut self, article_id: &str, error: &str) {
        self.total_articles += 1;
        self.failed += 1;
        self.errors.push(format!("{}: {}", article_id, error));
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_articles == 0 {
            0.0
        } else {
            (self.successful as f64 / self.total_articles as f64) * 100.0
        }
    }

    /// Get average entities per article
    pub fn avg_entities_per_article(&self) -> f64 {
        if self.successful == 0 {
            0.0
        } else {
            self.total_entities as f64 / self.successful as f64
        }
    }

    /// Get average relations per article
    pub fn avg_relations_per_article(&self) -> f64 {
        if self.successful == 0 {
            0.0
        } else {
            self.total_relations as f64 / self.successful as f64
        }
    }

    /// Get average processing time per article
    pub fn avg_duration_ms(&self) -> f64 {
        if self.successful == 0 {
            0.0
        } else {
            self.total_duration_ms as f64 / self.successful as f64
        }
    }

    /// Get overall verification rate
    pub fn verification_rate(&self) -> f64 {
        if self.total_relations == 0 {
            0.0
        } else {
            (self.total_verified as f64 / self.total_relations as f64) * 100.0
        }
    }

    /// Get throughput (articles per second)
    pub fn throughput(&self) -> f64 {
        if self.total_duration_ms == 0 {
            0.0
        } else {
            self.successful as f64 / (self.total_duration_ms as f64 / 1000.0)
        }
    }

    /// Format total memory as human-readable string
    pub fn memory_formatted(&self) -> String {
        format_bytes(self.total_memory_bytes)
    }

    /// Get summary as formatted string
    pub fn summary(&self) -> String {
        format!(
            "Batch: {}/{} articles ({:.1}% success) | Entities: {} | Relations: {} ({:.1}% verified) | Time: {:.1}s | Memory: {} | Throughput: {:.2} articles/s",
            self.successful,
            self.total_articles,
            self.success_rate(),
            self.total_entities,
            self.total_relations,
            self.verification_rate(),
            self.total_duration_ms as f64 / 1000.0,
            self.memory_formatted(),
            self.throughput()
        )
    }

    /// Get detailed report
    pub fn detailed_report(&self) -> String {
        let mut report = String::new();

        report.push_str("=== Batch Extraction Report ===\n\n");

        report.push_str(&format!("Articles Processed: {}\n", self.total_articles));
        report.push_str(&format!("  - Successful: {}\n", self.successful));
        report.push_str(&format!("  - Failed: {}\n", self.failed));
        report.push_str(&format!("  - Success Rate: {:.1}%\n\n", self.success_rate()));

        report.push_str(&format!("Total Entities: {}\n", self.total_entities));
        report.push_str(&format!(
            "  - Avg per Article: {:.1}\n",
            self.avg_entities_per_article()
        ));
        if !self.entities_by_type.is_empty() {
            report.push_str("  - By Type:\n");
            for (t, c) in &self.entities_by_type {
                report.push_str(&format!("    - {}: {}\n", t, c));
            }
        }

        report.push_str(&format!("\nTotal Relations: {}\n", self.total_relations));
        report.push_str(&format!(
            "  - Verified: {} ({:.1}%)\n",
            self.total_verified,
            self.verification_rate()
        ));
        report.push_str(&format!(
            "  - Avg per Article: {:.1}\n",
            self.avg_relations_per_article()
        ));
        if !self.relations_by_type.is_empty() {
            report.push_str("  - By Type:\n");
            for (t, c) in &self.relations_by_type {
                report.push_str(&format!("    - {}: {}\n", t, c));
            }
        }

        report.push_str(&format!(
            "\nPerformance:\n  - Total Time: {:.2}s\n  - Avg Time per Article: {:.0}ms\n  - Throughput: {:.2} articles/s\n",
            self.total_duration_ms as f64 / 1000.0,
            self.avg_duration_ms(),
            self.throughput()
        ));

        report.push_str(&format!(
            "\nMemory:\n  - Total Estimated: {}\n  - Avg per Article: {}\n",
            self.memory_formatted(),
            format_bytes(if self.successful > 0 {
                self.total_memory_bytes / self.successful
            } else {
                0
            })
        ));

        if !self.errors.is_empty() {
            report.push_str(&format!("\nErrors ({}):\n", self.errors.len()));
            for (i, err) in self.errors.iter().enumerate().take(10) {
                report.push_str(&format!("  {}. {}\n", i + 1, err));
            }
            if self.errors.len() > 10 {
                report.push_str(&format!("  ... and {} more\n", self.errors.len() - 10));
            }
        }

        report
    }
}

// ============================================================================
// Pipeline Profiler
// ============================================================================

/// Profiler for tracking pipeline stage durations
#[derive(Debug, Clone, Default)]
pub struct PipelineProfiler {
    /// Stage timings
    stages: HashMap<String, Duration>,

    /// Current stage start time
    current_stage: Option<(String, Instant)>,

    /// Total start time
    total_start: Option<Instant>,
}

impl PipelineProfiler {
    /// Create a new profiler
    pub fn new() -> Self {
        Self::default()
    }

    /// Start profiling
    pub fn start(&mut self) {
        self.total_start = Some(Instant::now());
        self.stages.clear();
        self.current_stage = None;
    }

    /// Begin a stage
    pub fn begin_stage(&mut self, name: impl Into<String>) {
        // End previous stage if any
        if let Some((prev_name, start)) = self.current_stage.take() {
            self.stages.insert(prev_name, start.elapsed());
        }
        self.current_stage = Some((name.into(), Instant::now()));
    }

    /// End current stage
    pub fn end_stage(&mut self) {
        if let Some((name, start)) = self.current_stage.take() {
            self.stages.insert(name, start.elapsed());
        }
    }

    /// Get stage duration
    pub fn stage_duration(&self, name: &str) -> Option<Duration> {
        self.stages.get(name).copied()
    }

    /// Get stage duration in milliseconds
    pub fn stage_ms(&self, name: &str) -> u64 {
        self.stages
            .get(name)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Get total elapsed time
    pub fn total_elapsed(&self) -> Duration {
        self.total_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// Get total elapsed milliseconds
    pub fn total_ms(&self) -> u64 {
        self.total_elapsed().as_millis() as u64
    }

    /// Get profile summary
    pub fn summary(&self) -> ProfileSummary {
        let total = self.total_elapsed();
        let stages: Vec<StageTiming> = self
            .stages
            .iter()
            .map(|(name, duration)| {
                let percentage = if total.as_nanos() > 0 {
                    (duration.as_nanos() as f64 / total.as_nanos() as f64) * 100.0
                } else {
                    0.0
                };
                StageTiming {
                    name: name.clone(),
                    duration_ms: duration.as_millis() as u64,
                    percentage,
                }
            })
            .collect();

        ProfileSummary {
            total_ms: total.as_millis() as u64,
            stages,
        }
    }

    /// Get formatted report
    pub fn report(&self) -> String {
        let summary = self.summary();
        let mut report = String::new();

        report.push_str(&format!("Pipeline Profile (Total: {}ms)\n", summary.total_ms));
        report.push_str(&format!("{:-<40}\n", ""));

        let mut sorted_stages = summary.stages;
        sorted_stages.sort_by(|a, b| b.duration_ms.cmp(&a.duration_ms));

        for stage in sorted_stages {
            report.push_str(&format!(
                "  {:20} {:>6}ms ({:>5.1}%)\n",
                stage.name, stage.duration_ms, stage.percentage
            ));
        }

        report
    }
}

/// Summary of pipeline profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSummary {
    /// Total duration in milliseconds
    pub total_ms: u64,

    /// Individual stage timings
    pub stages: Vec<StageTiming>,
}

/// Timing for a single stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageTiming {
    /// Stage name
    pub name: String,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Percentage of total time
    pub percentage: f64,
}

// ============================================================================
// Memory Estimation
// ============================================================================

/// Memory size estimator for ontology structures
pub struct MemoryEstimator;

impl MemoryEstimator {
    /// Estimate memory for extracted entities
    pub fn estimate_entities(count: usize, avg_text_len: usize) -> usize {
        // Base struct size + text allocation
        count * (std::mem::size_of::<super::extractor::ExtractedEntity>() + avg_text_len * 2)
    }

    /// Estimate memory for extracted relations
    pub fn estimate_relations(count: usize, avg_evidence_len: usize) -> usize {
        count
            * (std::mem::size_of::<super::extractor::ExtractedRelation>()
                + avg_evidence_len
                + 100) // subject + object
    }

    /// Estimate memory for triples
    pub fn estimate_triples(count: usize) -> usize {
        count * 512 // Average Triple size with evidence
    }

    /// Estimate memory for a TripleStore
    pub fn estimate_triple_store(
        entity_count: usize,
        triple_count: usize,
        title_len: usize,
    ) -> usize {
        let base = std::mem::size_of::<super::extractor::TripleStore>();
        let entities = Self::estimate_entities(entity_count, 10);
        let triples = Self::estimate_triples(triple_count);
        let title = title_len * 2;

        base + entities + triples + title + 256 // overhead
    }

    /// Estimate memory for knowledge base
    pub fn estimate_knowledge_base(entry_count: usize, avg_aliases: usize) -> usize {
        entry_count * (256 + avg_aliases * 32) // KnowledgeBaseEntry + aliases
    }

    /// Estimate memory for linking cache
    pub fn estimate_link_cache(entry_count: usize) -> usize {
        entry_count * 384 // LinkedEntity average
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Format bytes as human-readable string
pub fn format_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Parse human-readable byte string
pub fn parse_bytes(s: &str) -> Option<usize> {
    let s = s.trim().to_uppercase();

    let (num_str, multiplier) = if s.ends_with("GB") {
        (&s[..s.len() - 2], 1024 * 1024 * 1024)
    } else if s.ends_with("MB") {
        (&s[..s.len() - 2], 1024 * 1024)
    } else if s.ends_with("KB") {
        (&s[..s.len() - 2], 1024)
    } else if s.ends_with('B') {
        (&s[..s.len() - 1], 1)
    } else {
        (s.as_str(), 1)
    };

    num_str.trim().parse::<f64>().ok().map(|n| (n * multiplier as f64) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction_stats_new() {
        let stats = ExtractionStats::new("test_001");
        assert_eq!(stats.article_id, "test_001");
        assert_eq!(stats.entity_count, 0);
        assert_eq!(stats.relation_count, 0);
    }

    #[test]
    fn test_extraction_stats_record_entity() {
        let mut stats = ExtractionStats::new("test");
        stats.record_entity(EntityType::Person);
        stats.record_entity(EntityType::Person);
        stats.record_entity(EntityType::Organization);

        assert_eq!(stats.entity_count, 3);
        assert_eq!(stats.entities_by_type.get("인물"), Some(&2));
        assert_eq!(stats.entities_by_type.get("기관"), Some(&1));
    }

    #[test]
    fn test_extraction_stats_record_relation() {
        let mut stats = ExtractionStats::new("test");
        stats.record_relation(RelationType::Said, 0.8, true);
        stats.record_relation(RelationType::WorksFor, 0.6, false);

        assert_eq!(stats.relation_count, 2);
        assert_eq!(stats.verified_count, 1);
        assert!((stats.avg_confidence - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_extraction_stats_verification_rate() {
        let mut stats = ExtractionStats::new("test");
        stats.relation_count = 10;
        stats.verified_count = 7;

        assert!((stats.verification_rate() - 70.0).abs() < 0.1);
    }

    #[test]
    fn test_extraction_stats_estimate_memory() {
        let mut stats = ExtractionStats::new("test");
        stats.entity_count = 10;
        stats.relation_count = 5;
        stats.triple_count = 5;
        stats.estimate_memory();

        assert!(stats.memory_bytes > 0);
    }

    #[test]
    fn test_batch_stats_add_success() {
        let mut batch = BatchStats::new();

        let mut stats1 = ExtractionStats::new("art_001");
        stats1.entity_count = 5;
        stats1.relation_count = 3;
        stats1.verified_count = 2;
        stats1.duration_ms = 100;

        batch.add_success(stats1);

        assert_eq!(batch.total_articles, 1);
        assert_eq!(batch.successful, 1);
        assert_eq!(batch.total_entities, 5);
        assert_eq!(batch.total_relations, 3);
    }

    #[test]
    fn test_batch_stats_add_failure() {
        let mut batch = BatchStats::new();
        batch.add_failure("art_001", "Parse error");

        assert_eq!(batch.total_articles, 1);
        assert_eq!(batch.failed, 1);
        assert_eq!(batch.errors.len(), 1);
    }

    #[test]
    fn test_batch_stats_success_rate() {
        let mut batch = BatchStats::new();
        batch.successful = 8;
        batch.failed = 2;
        batch.total_articles = 10;

        assert!((batch.success_rate() - 80.0).abs() < 0.1);
    }

    #[test]
    fn test_pipeline_profiler() {
        let mut profiler = PipelineProfiler::new();
        profiler.start();

        profiler.begin_stage("extraction");
        std::thread::sleep(std::time::Duration::from_millis(10));
        profiler.end_stage();

        profiler.begin_stage("verification");
        std::thread::sleep(std::time::Duration::from_millis(5));
        profiler.end_stage();

        assert!(profiler.stage_ms("extraction") >= 10);
        assert!(profiler.stage_ms("verification") >= 5);
        assert!(profiler.total_ms() >= 15);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_parse_bytes() {
        assert_eq!(parse_bytes("500 B"), Some(500));
        assert_eq!(parse_bytes("1 KB"), Some(1024));
        assert_eq!(parse_bytes("1.5 KB"), Some(1536));
        assert_eq!(parse_bytes("1 MB"), Some(1048576));
        assert_eq!(parse_bytes("1 GB"), Some(1073741824));
    }

    #[test]
    fn test_memory_estimator() {
        let entities = MemoryEstimator::estimate_entities(10, 20);
        assert!(entities > 0);

        let relations = MemoryEstimator::estimate_relations(5, 100);
        assert!(relations > 0);

        let triples = MemoryEstimator::estimate_triples(5);
        assert!(triples > 0);
    }

    #[test]
    fn test_profile_summary() {
        let mut profiler = PipelineProfiler::new();
        profiler.start();
        profiler.begin_stage("test");
        profiler.end_stage();

        let summary = profiler.summary();
        assert!(!summary.stages.is_empty());
    }

    #[test]
    fn test_extraction_stats_summary() {
        let mut stats = ExtractionStats::new("test_001");
        stats.entity_count = 10;
        stats.relation_count = 5;
        stats.verified_count = 4;
        stats.duration_ms = 150;
        stats.estimate_memory();

        let summary = stats.summary();
        assert!(summary.contains("test_001"));
        assert!(summary.contains("10"));
    }

    #[test]
    fn test_batch_stats_detailed_report() {
        let mut batch = BatchStats::new();
        batch.successful = 10;
        batch.failed = 2;
        batch.total_articles = 12;
        batch.total_entities = 100;
        batch.total_relations = 50;
        batch.total_verified = 40;
        batch.total_duration_ms = 1000;
        batch.total_memory_bytes = 102400;

        let report = batch.detailed_report();
        assert!(report.contains("Batch Extraction Report"));
        assert!(report.contains("Success Rate"));
    }
}
