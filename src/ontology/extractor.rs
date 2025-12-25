//! LLM-based entity and relation extraction
//!
//! This module provides extraction of entities and relations from news articles
//! using structured prompts and pattern matching.
//!
//! ## Features
//! - Regex-based entity extraction (NER-like patterns)
//! - LLM prompt generation for advanced extraction
//! - LLM response parsing (JSON format)
//! - Triple (Subject-Predicate-Object) output format
//! - Hallucination verification against source text

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::models::ParsedArticle;

/// Extraction configuration
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Minimum entity name length
    pub min_entity_length: usize,

    /// Maximum entities per article
    pub max_entities: usize,

    /// Maximum relations per article
    pub max_relations: usize,

    /// Confidence threshold for relations
    pub confidence_threshold: f32,

    /// Enable hallucination check
    pub hallucination_check: bool,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            min_entity_length: 2,
            max_entities: 50,
            max_relations: 100,
            confidence_threshold: 0.5,
            hallucination_check: true,
        }
    }
}

impl ExtractionConfig {
    /// Create a new builder for ExtractionConfig
    pub fn builder() -> ExtractionConfigBuilder {
        ExtractionConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), super::error::OntologyError> {
        if self.min_entity_length == 0 {
            return Err(super::error::OntologyError::invalid_config(
                "min_entity_length",
                "0",
                "Must be at least 1",
            ));
        }
        if self.max_entities == 0 {
            return Err(super::error::OntologyError::invalid_config(
                "max_entities",
                "0",
                "Must be at least 1",
            ));
        }
        if self.confidence_threshold < 0.0 || self.confidence_threshold > 1.0 {
            return Err(super::error::OntologyError::invalid_config(
                "confidence_threshold",
                self.confidence_threshold.to_string(),
                "Must be between 0.0 and 1.0",
            ));
        }
        Ok(())
    }
}

/// Builder for ExtractionConfig with fluent API
#[derive(Debug, Clone, Default)]
pub struct ExtractionConfigBuilder {
    min_entity_length: Option<usize>,
    max_entities: Option<usize>,
    max_relations: Option<usize>,
    confidence_threshold: Option<f32>,
    hallucination_check: Option<bool>,
}

impl ExtractionConfigBuilder {
    /// Set minimum entity name length
    pub fn min_entity_length(mut self, len: usize) -> Self {
        self.min_entity_length = Some(len);
        self
    }

    /// Set maximum entities per article
    pub fn max_entities(mut self, max: usize) -> Self {
        self.max_entities = Some(max);
        self
    }

    /// Set maximum relations per article
    pub fn max_relations(mut self, max: usize) -> Self {
        self.max_relations = Some(max);
        self
    }

    /// Set confidence threshold for relations
    pub fn confidence_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = Some(threshold);
        self
    }

    /// Enable or disable hallucination check
    pub fn hallucination_check(mut self, enable: bool) -> Self {
        self.hallucination_check = Some(enable);
        self
    }

    /// Build the config with validation
    pub fn build(self) -> Result<ExtractionConfig, super::error::OntologyError> {
        let config = ExtractionConfig {
            min_entity_length: self.min_entity_length.unwrap_or(2),
            max_entities: self.max_entities.unwrap_or(50),
            max_relations: self.max_relations.unwrap_or(100),
            confidence_threshold: self.confidence_threshold.unwrap_or(0.5),
            hallucination_check: self.hallucination_check.unwrap_or(true),
        };
        config.validate()?;
        Ok(config)
    }

    /// Build without validation (for testing)
    pub fn build_unchecked(self) -> ExtractionConfig {
        ExtractionConfig {
            min_entity_length: self.min_entity_length.unwrap_or(2),
            max_entities: self.max_entities.unwrap_or(50),
            max_relations: self.max_relations.unwrap_or(100),
            confidence_threshold: self.confidence_threshold.unwrap_or(0.5),
            hallucination_check: self.hallucination_check.unwrap_or(true),
        }
    }
}

// ============================================================================
// Hallucination Verification
// ============================================================================

/// Verification failure reason
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum VerificationFailure {
    /// Subject not found in source text
    SubjectNotFound,
    /// Object not found in source text
    ObjectNotFound,
    /// Evidence sentence not found in source text
    EvidenceNotFound,
    /// Subject found but with different context
    SubjectContextMismatch,
    /// Relation type doesn't match evidence
    RelationMismatch,
    /// Confidence too low after verification
    LowConfidence,
}

impl VerificationFailure {
    /// Get Korean description
    pub fn korean_desc(&self) -> &'static str {
        match self {
            VerificationFailure::SubjectNotFound => "주어가 원문에 없음",
            VerificationFailure::ObjectNotFound => "목적어가 원문에 없음",
            VerificationFailure::EvidenceNotFound => "증거 문장이 원문에 없음",
            VerificationFailure::SubjectContextMismatch => "주어의 문맥이 불일치",
            VerificationFailure::RelationMismatch => "관계 유형이 불일치",
            VerificationFailure::LowConfidence => "신뢰도가 너무 낮음",
        }
    }
}

/// Detailed verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the relation passed verification
    pub verified: bool,

    /// Original confidence
    pub original_confidence: f32,

    /// Adjusted confidence after verification
    pub adjusted_confidence: f32,

    /// Failure reasons (empty if verified)
    pub failures: Vec<VerificationFailure>,

    /// Subject match details
    pub subject_match: MatchDetail,

    /// Object match details
    pub object_match: MatchDetail,

    /// Evidence match details
    pub evidence_match: MatchDetail,
}

/// Match detail for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDetail {
    /// Whether a match was found
    pub found: bool,

    /// Match type (exact, fuzzy, partial)
    pub match_type: MatchType,

    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,

    /// Matched text (if found)
    pub matched_text: Option<String>,

    /// Position in source text
    pub position: Option<(usize, usize)>,
}

impl Default for MatchDetail {
    fn default() -> Self {
        Self {
            found: false,
            match_type: MatchType::None,
            similarity: 0.0,
            matched_text: None,
            position: None,
        }
    }
}

/// Type of match found
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchType {
    /// Exact match
    Exact,
    /// Case-insensitive match
    CaseInsensitive,
    /// Fuzzy match (high similarity)
    Fuzzy,
    /// Partial match (substring)
    Partial,
    /// No match
    None,
}

/// Hallucination verifier with configurable options
pub struct HallucinationVerifier {
    /// Minimum similarity for fuzzy match
    pub fuzzy_threshold: f32,

    /// Minimum similarity for partial match
    pub partial_threshold: f32,

    /// Confidence boost for exact match
    pub exact_match_boost: f32,

    /// Confidence penalty for fuzzy match
    pub fuzzy_match_penalty: f32,

    /// Confidence penalty for no match
    pub no_match_penalty: f32,

    /// Minimum confidence after verification
    pub min_confidence: f32,
}

impl Default for HallucinationVerifier {
    fn default() -> Self {
        Self {
            fuzzy_threshold: 0.8,
            partial_threshold: 0.5,
            exact_match_boost: 1.2,
            fuzzy_match_penalty: 0.9,
            no_match_penalty: 0.4,
            min_confidence: 0.3,
        }
    }
}

impl HallucinationVerifier {
    /// Create a new verifier with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a strict verifier (higher thresholds)
    pub fn strict() -> Self {
        Self {
            fuzzy_threshold: 0.9,
            partial_threshold: 0.7,
            exact_match_boost: 1.1,
            fuzzy_match_penalty: 0.8,
            no_match_penalty: 0.3,
            min_confidence: 0.5,
        }
    }

    /// Create a lenient verifier (lower thresholds)
    pub fn lenient() -> Self {
        Self {
            fuzzy_threshold: 0.6,
            partial_threshold: 0.3,
            exact_match_boost: 1.3,
            fuzzy_match_penalty: 0.95,
            no_match_penalty: 0.6,
            min_confidence: 0.2,
        }
    }

    /// Verify a relation against source text
    pub fn verify(&self, relation: &ExtractedRelation, source_text: &str) -> VerificationResult {
        let mut failures = Vec::new();
        let original_confidence = relation.confidence;

        // Verify subject
        let subject_match = self.find_match(&relation.subject, source_text);
        if !subject_match.found {
            failures.push(VerificationFailure::SubjectNotFound);
        }

        // Verify object (if not empty)
        let object_match = if relation.object.is_empty() {
            MatchDetail {
                found: true,
                match_type: MatchType::Exact,
                similarity: 1.0,
                matched_text: None,
                position: None,
            }
        } else {
            let m = self.find_match(&relation.object, source_text);
            if !m.found {
                failures.push(VerificationFailure::ObjectNotFound);
            }
            m
        };

        // Verify evidence
        let evidence_match = if relation.evidence.is_empty() {
            MatchDetail {
                found: true,
                match_type: MatchType::Exact,
                similarity: 1.0,
                matched_text: None,
                position: None,
            }
        } else {
            let m = self.find_match(&relation.evidence, source_text);
            if !m.found {
                failures.push(VerificationFailure::EvidenceNotFound);
            }
            m
        };

        // Adjust confidence based on matches
        let adjusted_confidence: f32 = self.calculate_adjusted_confidence(
            original_confidence,
            &subject_match,
            &object_match,
            &evidence_match,
        );

        // Check minimum confidence
        if adjusted_confidence < self.min_confidence {
            failures.push(VerificationFailure::LowConfidence);
        }

        let verified = failures.is_empty() && adjusted_confidence >= self.min_confidence;

        VerificationResult {
            verified,
            original_confidence,
            adjusted_confidence,
            failures,
            subject_match,
            object_match,
            evidence_match,
        }
    }

    /// Find a match in source text
    fn find_match(&self, query: &str, source: &str) -> MatchDetail {
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() {
            return MatchDetail {
                found: true,
                match_type: MatchType::Exact,
                similarity: 1.0,
                matched_text: None,
                position: None,
            };
        }

        // Try exact match
        if let Some(pos) = source.find(query_trimmed) {
            return MatchDetail {
                found: true,
                match_type: MatchType::Exact,
                similarity: 1.0,
                matched_text: Some(query_trimmed.to_string()),
                position: Some((pos, pos + query_trimmed.len())),
            };
        }

        // Try case-insensitive match
        let query_lower = query_trimmed.to_lowercase();
        let source_lower = source.to_lowercase();
        if let Some(pos) = source_lower.find(&query_lower) {
            return MatchDetail {
                found: true,
                match_type: MatchType::CaseInsensitive,
                similarity: 0.95,
                matched_text: Some(source[pos..pos + query_trimmed.len()].to_string()),
                position: Some((pos, pos + query_trimmed.len())),
            };
        }

        // Try partial match (query is substring or source contains query)
        // Split into words and check for partial matches
        let query_words: Vec<&str> = query_trimmed.split_whitespace().collect();
        let mut matched_words = 0;

        for word in &query_words {
            if word.len() >= 2 && source.contains(*word) {
                matched_words += 1;
            }
        }

        if !query_words.is_empty() {
            let partial_ratio = matched_words as f32 / query_words.len() as f32;
            if partial_ratio >= self.partial_threshold {
                return MatchDetail {
                    found: true,
                    match_type: MatchType::Partial,
                    similarity: partial_ratio,
                    matched_text: None,
                    position: None,
                };
            }
        }

        // Try fuzzy match using character overlap
        let similarity = self.calculate_similarity(query_trimmed, source);
        if similarity >= self.fuzzy_threshold {
            return MatchDetail {
                found: true,
                match_type: MatchType::Fuzzy,
                similarity,
                matched_text: None,
                position: None,
            };
        }

        // No match found
        MatchDetail::default()
    }

    /// Calculate string similarity (character-level Jaccard + containment)
    fn calculate_similarity(&self, a: &str, b: &str) -> f32 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        // Check for containment
        if b.contains(a) {
            return a.len() as f32 / b.len().min(a.len() * 3) as f32;
        }

        // Character bigram similarity
        let bigrams_a: HashSet<(char, char)> = a.chars().zip(a.chars().skip(1)).collect();
        let bigrams_b: HashSet<(char, char)> = b.chars().zip(b.chars().skip(1)).collect();

        if bigrams_a.is_empty() || bigrams_b.is_empty() {
            // Fall back to character overlap
            let chars_a: HashSet<char> = a.chars().collect();
            let chars_b: HashSet<char> = b.chars().collect();
            let intersection = chars_a.intersection(&chars_b).count();
            let union = chars_a.union(&chars_b).count();
            return if union == 0 {
                0.0
            } else {
                intersection as f32 / union as f32
            };
        }

        let intersection = bigrams_a.intersection(&bigrams_b).count();
        let union = bigrams_a.union(&bigrams_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// Calculate adjusted confidence based on match quality
    fn calculate_adjusted_confidence(
        &self,
        original: f32,
        subject: &MatchDetail,
        object: &MatchDetail,
        evidence: &MatchDetail,
    ) -> f32 {
        let mut confidence = original;

        // Apply subject match factor
        confidence *= match subject.match_type {
            MatchType::Exact => self.exact_match_boost,
            MatchType::CaseInsensitive => 1.0,
            MatchType::Fuzzy => self.fuzzy_match_penalty,
            MatchType::Partial => self.fuzzy_match_penalty * subject.similarity,
            MatchType::None => self.no_match_penalty,
        };

        // Apply object match factor (weighted less)
        confidence *= match object.match_type {
            MatchType::Exact => 1.05,
            MatchType::CaseInsensitive => 1.0,
            MatchType::Fuzzy => 0.95,
            MatchType::Partial => 0.9,
            MatchType::None => 0.7,
        };

        // Apply evidence match factor
        confidence *= match evidence.match_type {
            MatchType::Exact => self.exact_match_boost,
            MatchType::CaseInsensitive => 1.0,
            MatchType::Fuzzy => self.fuzzy_match_penalty,
            MatchType::Partial => 0.85,
            MatchType::None => self.no_match_penalty,
        };

        // Clamp to valid range
        confidence.clamp(0.0, 1.0)
    }

    /// Verify multiple relations and return results
    pub fn verify_batch(
        &self,
        relations: &[ExtractedRelation],
        source_text: &str,
    ) -> Vec<VerificationResult> {
        relations
            .iter()
            .map(|r| self.verify(r, source_text))
            .collect()
    }

    /// Verify and update relations in place
    pub fn verify_and_update(
        &self,
        relations: &mut [ExtractedRelation],
        source_text: &str,
    ) -> VerificationSummary {
        let mut summary = VerificationSummary::default();

        for relation in relations {
            let result = self.verify(relation, source_text);

            relation.verified = result.verified;
            relation.confidence = result.adjusted_confidence;

            summary.total += 1;
            if result.verified {
                summary.verified += 1;
            } else {
                summary.failed += 1;
                for failure in &result.failures {
                    *summary.failure_counts.entry(failure.clone()).or_insert(0) += 1;
                }
            }
        }

        summary
    }
}

/// Summary of batch verification
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationSummary {
    /// Total relations verified
    pub total: usize,

    /// Number that passed verification
    pub verified: usize,

    /// Number that failed verification
    pub failed: usize,

    /// Count by failure reason
    pub failure_counts: HashMap<VerificationFailure, usize>,
}

impl VerificationSummary {
    /// Get verification rate as percentage
    pub fn verification_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.verified as f64 / self.total as f64) * 100.0
        }
    }

    /// Get most common failure reason
    pub fn most_common_failure(&self) -> Option<&VerificationFailure> {
        self.failure_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(failure, _)| failure)
    }
}

/// Extracted entity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedEntity {
    /// Entity text as found in article
    pub text: String,

    /// Normalized/canonical name
    pub canonical_name: Option<String>,

    /// Entity type
    pub entity_type: EntityType,

    /// Start position in text
    pub start: usize,

    /// End position in text
    pub end: usize,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,

    /// Source: where this entity was found
    pub source: EntitySource,
}

/// Entity type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EntityType {
    /// Person name
    Person,
    /// Organization (company, government, etc.)
    Organization,
    /// Location (country, city, etc.)
    Location,
    /// Date or time expression
    DateTime,
    /// Monetary value
    Money,
    /// Percentage
    Percentage,
    /// Event name
    Event,
    /// Product or service
    Product,
    /// Legal or policy term
    Policy,
    /// Unknown/other
    Other,
}

impl EntityType {
    /// Get Korean label for entity type
    pub fn korean_label(&self) -> &'static str {
        match self {
            EntityType::Person => "인물",
            EntityType::Organization => "기관",
            EntityType::Location => "장소",
            EntityType::DateTime => "날짜/시간",
            EntityType::Money => "금액",
            EntityType::Percentage => "비율",
            EntityType::Event => "사건",
            EntityType::Product => "제품",
            EntityType::Policy => "정책",
            EntityType::Other => "기타",
        }
    }

    /// Get RDF type URI
    pub fn rdf_type(&self) -> &'static str {
        match self {
            EntityType::Person => "schema:Person",
            EntityType::Organization => "schema:Organization",
            EntityType::Location => "schema:Place",
            EntityType::DateTime => "schema:DateTime",
            EntityType::Money => "schema:MonetaryAmount",
            EntityType::Percentage => "schema:QuantitativeValue",
            EntityType::Event => "schema:Event",
            EntityType::Product => "schema:Product",
            EntityType::Policy => "schema:GovernmentService",
            EntityType::Other => "schema:Thing",
        }
    }

    /// Parse entity type from string (case-insensitive)
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "person" | "인물" | "per" => EntityType::Person,
            "organization" | "org" | "기관" | "회사" | "company" => EntityType::Organization,
            "location" | "loc" | "장소" | "place" | "gpe" => EntityType::Location,
            "datetime" | "date" | "time" | "날짜" | "시간" => EntityType::DateTime,
            "money" | "금액" | "monetary" | "currency" => EntityType::Money,
            "percentage" | "percent" | "비율" | "퍼센트" => EntityType::Percentage,
            "event" | "사건" | "이벤트" => EntityType::Event,
            "product" | "제품" | "서비스" | "service" => EntityType::Product,
            "policy" | "정책" | "법률" | "law" => EntityType::Policy,
            _ => EntityType::Other,
        }
    }
}

/// Source of entity extraction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntitySource {
    /// Extracted from title
    Title,
    /// Extracted from content body
    Content,
    /// Both title and content
    Both,
}

/// Extracted relation (triple)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedRelation {
    /// Subject entity text
    pub subject: String,

    /// Subject entity type
    pub subject_type: EntityType,

    /// Predicate/relation type
    pub predicate: RelationType,

    /// Object entity text
    pub object: String,

    /// Object entity type
    pub object_type: EntityType,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,

    /// Evidence text (sentence where relation was found)
    pub evidence: String,

    /// Verified against source text
    pub verified: bool,
}

/// Relation type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RelationType {
    /// Person works for organization
    WorksFor,
    /// Person is member of organization
    MemberOf,
    /// Person leads/heads organization
    Leads,
    /// Entity is located in place
    LocatedIn,
    /// Entity owns something
    Owns,
    /// Entity founded organization
    Founded,
    /// Person/org made statement
    Said,
    /// Entity participated in event
    ParticipatedIn,
    /// Entity announced something
    Announced,
    /// Entity criticized something/someone
    Criticized,
    /// Entity supported something/someone
    Supported,
    /// Entity opposed something/someone
    Opposed,
    /// Entity invested in something
    InvestedIn,
    /// Entity acquired something
    Acquired,
    /// Entity merged with something
    MergedWith,
    /// Related to (generic)
    RelatedTo,
    /// Unknown relation
    Unknown,
}

impl RelationType {
    /// Get Korean label
    pub fn korean_label(&self) -> &'static str {
        match self {
            RelationType::WorksFor => "근무",
            RelationType::MemberOf => "소속",
            RelationType::Leads => "대표",
            RelationType::LocatedIn => "위치",
            RelationType::Owns => "소유",
            RelationType::Founded => "설립",
            RelationType::Said => "발언",
            RelationType::ParticipatedIn => "참여",
            RelationType::Announced => "발표",
            RelationType::Criticized => "비판",
            RelationType::Supported => "지지",
            RelationType::Opposed => "반대",
            RelationType::InvestedIn => "투자",
            RelationType::Acquired => "인수",
            RelationType::MergedWith => "합병",
            RelationType::RelatedTo => "관련",
            RelationType::Unknown => "미상",
        }
    }

    /// Get RDF predicate URI
    pub fn rdf_predicate(&self) -> &'static str {
        match self {
            RelationType::WorksFor => "schema:worksFor",
            RelationType::MemberOf => "schema:memberOf",
            RelationType::Leads => "schema:founder",
            RelationType::LocatedIn => "schema:location",
            RelationType::Owns => "schema:owns",
            RelationType::Founded => "schema:founder",
            RelationType::Said => "schema:author",
            RelationType::ParticipatedIn => "schema:participant",
            RelationType::Announced => "schema:publicationDate",
            RelationType::Criticized => "baram:criticized",
            RelationType::Supported => "baram:supported",
            RelationType::Opposed => "baram:opposed",
            RelationType::InvestedIn => "schema:investor",
            RelationType::Acquired => "schema:acquiredFrom",
            RelationType::MergedWith => "baram:mergedWith",
            RelationType::RelatedTo => "schema:relatedTo",
            RelationType::Unknown => "baram:unknown",
        }
    }

    /// Parse relation type from string (case-insensitive)
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "worksfor" | "works_for" | "근무" | "employed_by" => RelationType::WorksFor,
            "memberof" | "member_of" | "소속" | "belongs_to" => RelationType::MemberOf,
            "leads" | "대표" | "heads" | "ceo_of" | "president_of" => RelationType::Leads,
            "locatedin" | "located_in" | "위치" | "based_in" | "in" => RelationType::LocatedIn,
            "owns" | "소유" | "owner_of" | "has" => RelationType::Owns,
            "founded" | "설립" | "established" | "created" => RelationType::Founded,
            "said" | "발언" | "stated" | "claimed" | "말했다" => RelationType::Said,
            "participatedin" | "participated_in" | "참여" | "attended" | "joined" => {
                RelationType::ParticipatedIn
            }
            "announced" | "발표" | "revealed" | "disclosed" => RelationType::Announced,
            "criticized" | "비판" | "condemned" | "attacked" => RelationType::Criticized,
            "supported" | "지지" | "backed" | "endorsed" => RelationType::Supported,
            "opposed" | "반대" | "against" | "rejected" => RelationType::Opposed,
            "investedin" | "invested_in" | "투자" | "funded" => RelationType::InvestedIn,
            "acquired" | "인수" | "bought" | "purchased" => RelationType::Acquired,
            "mergedwith" | "merged_with" | "합병" | "combined_with" => RelationType::MergedWith,
            "relatedto" | "related_to" | "관련" | "associated_with" => RelationType::RelatedTo,
            _ => RelationType::Unknown,
        }
    }
}

/// LLM prompt template for relation extraction
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    /// System prompt
    pub system: String,

    /// User prompt template (with {title} and {content} placeholders)
    pub user_template: String,

    /// Expected JSON schema
    pub output_schema: String,
}

impl Default for PromptTemplate {
    fn default() -> Self {
        Self {
            system: r#"You are a Korean news article analyzer. Extract entities and relationships from the given article.

Rules:
1. Only extract entities explicitly mentioned in the text
2. Do not infer or hallucinate information not present
3. Use the exact text as it appears for entity names
4. Assign confidence scores based on clarity of the relation
5. Output must be valid JSON"#.to_string(),

            user_template: r#"Analyze this Korean news article and extract entities and relationships.

Title: {title}

Content: {content}

Extract:
1. Entities: People, Organizations, Locations, Dates, Events
2. Relations: Who did what to whom, organizational affiliations, locations

Output JSON format:
{output_schema}"#.to_string(),

            output_schema: r#"{
  "entities": [
    {"text": "entity name", "type": "Person|Organization|Location|DateTime|Event|Other", "confidence": 0.95}
  ],
  "relations": [
    {"subject": "entity1", "predicate": "relation_type", "object": "entity2", "confidence": 0.9, "evidence": "source sentence"}
  ]
}"#.to_string(),
        }
    }
}

// ============================================================================
// LLM Response Parsing
// ============================================================================

/// LLM response for entity extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmEntityResponse {
    /// Entity text
    pub text: String,
    /// Entity type as string
    #[serde(rename = "type")]
    pub entity_type: String,
    /// Confidence score
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    0.7
}

/// LLM response for relation extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRelationResponse {
    /// Subject entity
    pub subject: String,
    /// Predicate/relation type
    pub predicate: String,
    /// Object entity
    pub object: String,
    /// Confidence score
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    /// Evidence sentence
    #[serde(default)]
    pub evidence: String,
}

/// Complete LLM extraction response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmExtractionResponse {
    /// Extracted entities
    #[serde(default)]
    pub entities: Vec<LlmEntityResponse>,
    /// Extracted relations
    #[serde(default)]
    pub relations: Vec<LlmRelationResponse>,
}

impl LlmExtractionResponse {
    /// Parse LLM response from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        // Try to extract JSON from markdown code blocks if present
        let cleaned = Self::extract_json_from_response(json);
        serde_json::from_str(&cleaned).with_context(|| {
            format!(
                "Failed to parse LLM response: {}",
                &cleaned[..cleaned.len().min(200)]
            )
        })
    }

    /// Extract JSON from LLM response (handles markdown code blocks)
    fn extract_json_from_response(response: &str) -> String {
        let trimmed = response.trim();

        // Try to find JSON in code blocks
        if let Some(start) = trimmed.find("```json") {
            let json_start = start + 7;
            if let Some(end) = trimmed[json_start..].find("```") {
                return trimmed[json_start..json_start + end].trim().to_string();
            }
        }

        // Try to find JSON in generic code blocks
        if let Some(start) = trimmed.find("```") {
            let block_start = start + 3;
            // Skip language identifier if present
            let json_start = trimmed[block_start..]
                .find('\n')
                .map(|n| block_start + n + 1)
                .unwrap_or(block_start);
            if let Some(end) = trimmed[json_start..].find("```") {
                return trimmed[json_start..json_start + end].trim().to_string();
            }
        }

        // Try to find raw JSON object
        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                if end > start {
                    return trimmed[start..=end].to_string();
                }
            }
        }

        // Return as-is
        trimmed.to_string()
    }

    /// Convert to ExtractedEntity list
    pub fn to_entities(&self, source: EntitySource) -> Vec<ExtractedEntity> {
        self.entities
            .iter()
            .map(|e| ExtractedEntity {
                text: e.text.clone(),
                canonical_name: None,
                entity_type: EntityType::from_string(&e.entity_type),
                start: 0, // Position not available from LLM
                end: 0,
                confidence: e.confidence,
                source,
            })
            .collect()
    }

    /// Convert to ExtractedRelation list
    pub fn to_relations(&self) -> Vec<ExtractedRelation> {
        self.relations
            .iter()
            .map(|r| ExtractedRelation {
                subject: r.subject.clone(),
                subject_type: EntityType::Other, // Will be resolved later
                predicate: RelationType::from_string(&r.predicate),
                object: r.object.clone(),
                object_type: EntityType::Other,
                confidence: r.confidence,
                evidence: r.evidence.clone(),
                verified: false,
            })
            .collect()
    }
}

// ============================================================================
// Triple Output Format (JSON-LD compatible)
// ============================================================================

/// RDF-style triple (Subject-Predicate-Object)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Triple {
    /// Subject URI or identifier
    #[serde(rename = "@id")]
    pub subject_id: String,

    /// Subject display name
    pub subject: String,

    /// Subject type
    pub subject_type: EntityType,

    /// Predicate URI
    pub predicate: String,

    /// Predicate display name
    pub predicate_label: String,

    /// Object URI or identifier
    pub object_id: String,

    /// Object display name
    pub object: String,

    /// Object type
    pub object_type: EntityType,

    /// Confidence score
    pub confidence: f32,

    /// Source evidence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,

    /// Verification status
    pub verified: bool,
}

impl Triple {
    /// Create a new triple from extracted relation
    pub fn from_relation(relation: &ExtractedRelation, article_id: &str) -> Self {
        let subject_id = format!("baram:entity/{}/{}", article_id, slug(&relation.subject));
        let object_id = format!("baram:entity/{}/{}", article_id, slug(&relation.object));

        Self {
            subject_id,
            subject: relation.subject.clone(),
            subject_type: relation.subject_type,
            predicate: relation.predicate.rdf_predicate().to_string(),
            predicate_label: relation.predicate.korean_label().to_string(),
            object_id,
            object: relation.object.clone(),
            object_type: relation.object_type,
            confidence: relation.confidence,
            evidence: if relation.evidence.is_empty() {
                None
            } else {
                Some(relation.evidence.clone())
            },
            verified: relation.verified,
        }
    }

    /// Convert to N-Triples format string
    pub fn to_ntriples(&self) -> String {
        format!(
            "<{}> <{}> <{}> .",
            self.subject_id, self.predicate, self.object_id
        )
    }

    /// Convert to Turtle format string
    pub fn to_turtle(&self) -> String {
        format!(
            "{} {} {} .",
            turtle_escape(&self.subject_id),
            turtle_escape(&self.predicate),
            turtle_escape(&self.object_id)
        )
    }
}

/// Collection of triples with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleStore {
    /// JSON-LD context
    #[serde(rename = "@context")]
    pub context: TripleContext,

    /// Article identifier
    pub article_id: String,

    /// Article title
    pub article_title: String,

    /// Extraction timestamp
    pub extracted_at: String,

    /// All triples
    pub triples: Vec<Triple>,

    /// Extracted entities
    pub entities: Vec<ExtractedEntity>,

    /// Statistics
    pub stats: TripleStats,
}

/// JSON-LD context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleContext {
    pub schema: String,
    pub baram: String,
}

impl Default for TripleContext {
    fn default() -> Self {
        Self {
            schema: "https://schema.org/".to_string(),
            baram: "https://baram.example.org/ontology/".to_string(),
        }
    }
}

/// Statistics for triple extraction
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TripleStats {
    pub total_entities: usize,
    pub total_relations: usize,
    pub verified_relations: usize,
    pub entity_types: HashMap<String, usize>,
    pub relation_types: HashMap<String, usize>,
}

impl TripleStore {
    /// Create from extraction result
    pub fn from_extraction(result: &ExtractionResult, article_title: &str) -> Self {
        let triples: Vec<Triple> = result
            .relations
            .iter()
            .map(|r| Triple::from_relation(r, &result.article_id))
            .collect();

        let mut entity_types = HashMap::new();
        for entity in &result.entities {
            *entity_types
                .entry(entity.entity_type.korean_label().to_string())
                .or_insert(0) += 1;
        }

        let mut relation_types = HashMap::new();
        for rel in &result.relations {
            *relation_types
                .entry(rel.predicate.korean_label().to_string())
                .or_insert(0) += 1;
        }

        let verified_count = result.relations.iter().filter(|r| r.verified).count();

        Self {
            context: TripleContext::default(),
            article_id: result.article_id.clone(),
            article_title: article_title.to_string(),
            extracted_at: chrono::Utc::now().to_rfc3339(),
            triples,
            entities: result.entities.clone(),
            stats: TripleStats {
                total_entities: result.entities.len(),
                total_relations: result.relations.len(),
                verified_relations: verified_count,
                entity_types,
                relation_types,
            },
        }
    }

    /// Export to JSON-LD format
    pub fn to_json_ld(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize TripleStore to JSON-LD")
    }

    /// Export to Turtle format
    pub fn to_turtle(&self) -> String {
        let mut output = String::new();

        // Prefixes
        output.push_str("@prefix schema: <https://schema.org/> .\n");
        output.push_str("@prefix baram: <https://baram.example.org/ontology/> .\n");
        output.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

        // Article metadata
        output.push_str(&format!(
            "# Article: {}\n# Extracted: {}\n\n",
            self.article_title, self.extracted_at
        ));

        // Triples
        for triple in &self.triples {
            output.push_str(&format!(
                "# {} {} {}\n",
                triple.subject, triple.predicate_label, triple.object
            ));
            output.push_str(&triple.to_turtle());
            output.push('\n');
            if let Some(evidence) = &triple.evidence {
                output.push_str(&format!("# Evidence: {evidence}\n"));
            }
            output.push('\n');
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

    /// Get only verified triples
    pub fn verified_triples(&self) -> Vec<&Triple> {
        self.triples.iter().filter(|t| t.verified).collect()
    }
}

/// Helper: Create URL-safe slug from text
fn slug(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// Helper: Escape string for Turtle format
fn turtle_escape(s: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") || s.contains(':') {
        format!("<{s}>")
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Entity and relation extractor
pub struct RelationExtractor {
    /// Configuration
    config: ExtractionConfig,

    /// Prompt template
    prompt: PromptTemplate,

    /// Person name patterns (Korean)
    person_patterns: Vec<Regex>,

    /// Organization patterns
    org_patterns: Vec<Regex>,

    /// Location patterns
    location_patterns: Vec<Regex>,

    /// Relation trigger patterns
    relation_patterns: HashMap<RelationType, Vec<Regex>>,

    /// Known entity cache (reserved for incremental extraction)
    #[allow(dead_code)]
    known_entities: HashSet<String>,
}

impl RelationExtractor {
    /// Create a new extractor with default config
    pub fn new() -> Self {
        Self::with_config(ExtractionConfig::default())
    }

    /// Create extractor with custom config
    pub fn with_config(config: ExtractionConfig) -> Self {
        Self {
            config,
            prompt: PromptTemplate::default(),
            person_patterns: Self::build_person_patterns(),
            org_patterns: Self::build_org_patterns(),
            location_patterns: Self::build_location_patterns(),
            relation_patterns: Self::build_relation_patterns(),
            known_entities: HashSet::new(),
        }
    }

    /// Build person name patterns
    fn build_person_patterns() -> Vec<Regex> {
        vec![
            // Korean names with title: 홍길동 대표, 김철수 장관
            Regex::new(r"([가-힣]{2,4})\s*(대표|장관|의원|대통령|총리|사장|회장|원장|교수|박사|기자|작가|배우|감독)").unwrap(),
            // Names with quotes: '홍길동' or "홍길동"
            Regex::new(r#"['"]([가-힣]{2,4})['"]"#).unwrap(),
            // Names followed by 씨, 님
            Regex::new(r"([가-힣]{2,4})\s*(씨|님)").unwrap(),
        ]
    }

    /// Build organization patterns
    fn build_org_patterns() -> Vec<Regex> {
        vec![
            // Companies: 삼성전자, LG전자
            Regex::new(r"([가-힣A-Za-z]+)(전자|그룹|은행|증권|보험|건설|제약|바이오|엔터|통신)")
                .unwrap(),
            // Government: 기획재정부, 외교부
            Regex::new(r"([가-힣]+)(부|처|청|원|위원회|공사|공단)").unwrap(),
            // Political parties
            Regex::new(r"(국민의힘|더불어민주당|정의당|국민의당|무소속)").unwrap(),
        ]
    }

    /// Build location patterns
    fn build_location_patterns() -> Vec<Regex> {
        vec![
            // Korean cities/provinces
            Regex::new(r"(서울|부산|대구|인천|광주|대전|울산|세종|경기|강원|충북|충남|전북|전남|경북|경남|제주)(시|도|특별시|광역시)?").unwrap(),
            // Districts
            Regex::new(r"([가-힣]+)(구|군|읍|면|동)").unwrap(),
            // Countries
            Regex::new(r"(미국|중국|일본|러시아|북한|영국|프랑스|독일|호주|캐나다|인도)").unwrap(),
        ]
    }

    /// Build relation trigger patterns
    fn build_relation_patterns() -> HashMap<RelationType, Vec<Regex>> {
        let mut patterns = HashMap::new();

        patterns.insert(RelationType::Said, vec![
            Regex::new(r"([가-힣A-Za-z]+)[은는이가]\s*(말했다|밝혔다|전했다|설명했다|언급했다|주장했다|강조했다)").unwrap(),
        ]);

        patterns.insert(
            RelationType::WorksFor,
            vec![Regex::new(r"([가-힣]+)\s+([가-힣A-Za-z]+)\s*(소속|근무)").unwrap()],
        );

        patterns.insert(
            RelationType::Leads,
            vec![Regex::new(r"([가-힣]+)\s+(대표|회장|사장|원장|총장)").unwrap()],
        );

        patterns.insert(
            RelationType::Announced,
            vec![Regex::new(r"([가-힣A-Za-z]+)[은는이가]\s*(발표했다|공개했다|선언했다)").unwrap()],
        );

        patterns.insert(RelationType::Criticized, vec![
            Regex::new(r"([가-힣A-Za-z]+)[은는이가]\s*([가-힣A-Za-z]+)[을를]\s*(비판했다|비난했다|질타했다)").unwrap(),
        ]);

        patterns.insert(RelationType::Supported, vec![
            Regex::new(r"([가-힣A-Za-z]+)[은는이가]\s*([가-힣A-Za-z]+)[을를]\s*(지지했다|찬성했다|옹호했다)").unwrap(),
        ]);

        patterns.insert(
            RelationType::InvestedIn,
            vec![
                Regex::new(r"([가-힣A-Za-z]+)[은는이가]\s*([가-힣A-Za-z]+)에\s*(투자|출자)")
                    .unwrap(),
            ],
        );

        patterns.insert(
            RelationType::Acquired,
            vec![
                Regex::new(r"([가-힣A-Za-z]+)[은는이가]\s*([가-힣A-Za-z]+)[을를]\s*(인수|매입)")
                    .unwrap(),
            ],
        );

        patterns
    }

    /// Extract entities from text
    pub fn extract_entities(&self, text: &str, source: EntitySource) -> Vec<ExtractedEntity> {
        let mut entities = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Extract persons
        for pattern in &self.person_patterns {
            for cap in pattern.captures_iter(text) {
                if let Some(m) = cap.get(1) {
                    let name = m.as_str().to_string();
                    if name.len() >= self.config.min_entity_length && !seen.contains(&name) {
                        seen.insert(name.clone());
                        entities.push(ExtractedEntity {
                            text: name,
                            canonical_name: None,
                            entity_type: EntityType::Person,
                            start: m.start(),
                            end: m.end(),
                            confidence: 0.8,
                            source,
                        });
                    }
                }
            }
        }

        // Extract organizations
        for pattern in &self.org_patterns {
            for cap in pattern.captures_iter(text) {
                if let Some(m) = cap.get(0) {
                    let name = m.as_str().to_string();
                    if name.len() >= self.config.min_entity_length && !seen.contains(&name) {
                        seen.insert(name.clone());
                        entities.push(ExtractedEntity {
                            text: name,
                            canonical_name: None,
                            entity_type: EntityType::Organization,
                            start: m.start(),
                            end: m.end(),
                            confidence: 0.85,
                            source,
                        });
                    }
                }
            }
        }

        // Extract locations
        for pattern in &self.location_patterns {
            for cap in pattern.captures_iter(text) {
                if let Some(m) = cap.get(0) {
                    let name = m.as_str().to_string();
                    if name.len() >= self.config.min_entity_length && !seen.contains(&name) {
                        seen.insert(name.clone());
                        entities.push(ExtractedEntity {
                            text: name,
                            canonical_name: None,
                            entity_type: EntityType::Location,
                            start: m.start(),
                            end: m.end(),
                            confidence: 0.9,
                            source,
                        });
                    }
                }
            }
        }

        // Extract money amounts
        let money_pattern =
            Regex::new(r"(\d+(?:,\d{3})*(?:\.\d+)?)\s*(원|달러|위안|엔|유로|억|조)").unwrap();
        for cap in money_pattern.captures_iter(text) {
            if let Some(m) = cap.get(0) {
                let name = m.as_str().to_string();
                if !seen.contains(&name) {
                    seen.insert(name.clone());
                    entities.push(ExtractedEntity {
                        text: name,
                        canonical_name: None,
                        entity_type: EntityType::Money,
                        start: m.start(),
                        end: m.end(),
                        confidence: 0.95,
                        source,
                    });
                }
            }
        }

        // Extract percentages
        let pct_pattern = Regex::new(r"(\d+(?:\.\d+)?)\s*(%|퍼센트|프로)").unwrap();
        for cap in pct_pattern.captures_iter(text) {
            if let Some(m) = cap.get(0) {
                let name = m.as_str().to_string();
                if !seen.contains(&name) {
                    seen.insert(name.clone());
                    entities.push(ExtractedEntity {
                        text: name,
                        canonical_name: None,
                        entity_type: EntityType::Percentage,
                        start: m.start(),
                        end: m.end(),
                        confidence: 0.95,
                        source,
                    });
                }
            }
        }

        // Limit entities
        entities.truncate(self.config.max_entities);
        entities
    }

    /// Extract relations from text given entities
    pub fn extract_relations(
        &self,
        text: &str,
        entities: &[ExtractedEntity],
    ) -> Vec<ExtractedRelation> {
        let mut relations = Vec::new();

        // Split text into sentences
        let sentences: Vec<&str> = text
            .split(['.', '。', '!', '?'])
            .filter(|s| !s.trim().is_empty())
            .collect();

        // Build entity lookup
        let entity_texts: HashSet<&str> = entities.iter().map(|e| e.text.as_str()).collect();

        for sentence in sentences {
            // Check each relation pattern
            for (relation_type, patterns) in &self.relation_patterns {
                for pattern in patterns {
                    if let Some(cap) = pattern.captures(sentence) {
                        // Extract subject and object from capture groups
                        if cap.len() >= 2 {
                            let subject = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                            let object = cap.get(2).map(|m| m.as_str()).unwrap_or("");

                            // Validate entities exist in our entity list
                            let subject_valid = entity_texts.contains(subject)
                                || entities.iter().any(|e| e.text.contains(subject));
                            let object_valid = object.is_empty()
                                || entity_texts.contains(object)
                                || entities.iter().any(|e| e.text.contains(object));

                            if subject_valid && object_valid && !subject.is_empty() {
                                let subject_type = entities
                                    .iter()
                                    .find(|e| e.text.contains(subject))
                                    .map(|e| e.entity_type)
                                    .unwrap_or(EntityType::Other);

                                let object_type = entities
                                    .iter()
                                    .find(|e| e.text.contains(object))
                                    .map(|e| e.entity_type)
                                    .unwrap_or(EntityType::Other);

                                let relation = ExtractedRelation {
                                    subject: subject.to_string(),
                                    subject_type,
                                    predicate: *relation_type,
                                    object: object.to_string(),
                                    object_type,
                                    confidence: 0.7,
                                    evidence: sentence.trim().to_string(),
                                    verified: false,
                                };

                                relations.push(relation);
                            }
                        }
                    }
                }
            }
        }

        // Limit relations
        relations.truncate(self.config.max_relations);
        relations
    }

    /// Verify relation against source text (hallucination check)
    pub fn verify_relation(&self, relation: &mut ExtractedRelation, text: &str) -> bool {
        if !self.config.hallucination_check {
            relation.verified = true;
            return true;
        }

        // Check if subject appears in text
        let subject_found = text.contains(&relation.subject);

        // Check if object appears in text (if not empty)
        let object_found = relation.object.is_empty() || text.contains(&relation.object);

        // Check if evidence sentence is in text
        let evidence_found = relation.evidence.is_empty()
            || text.contains(&relation.evidence)
            || text.contains(relation.evidence.trim());

        relation.verified = subject_found && object_found && evidence_found;

        if relation.verified {
            relation.confidence *= 1.2; // Boost confidence for verified
            relation.confidence = relation.confidence.min(1.0);
        } else {
            relation.confidence *= 0.5; // Reduce confidence for unverified
        }

        relation.verified
    }

    /// Full extraction from article
    pub fn extract_from_article(&self, article: &ParsedArticle) -> ExtractionResult {
        let mut all_entities = Vec::new();
        let mut all_relations = Vec::new();

        // Extract from title
        let title_entities = self.extract_entities(&article.title, EntitySource::Title);
        all_entities.extend(title_entities);

        // Extract from content
        let content_entities = self.extract_entities(&article.content, EntitySource::Content);
        all_entities.extend(content_entities);

        // Deduplicate entities
        all_entities = self.deduplicate_entities(all_entities);

        // Extract relations
        let full_text = format!("{}\n{}", article.title, article.content);
        let mut relations = self.extract_relations(&full_text, &all_entities);

        // Verify relations
        for relation in &mut relations {
            self.verify_relation(relation, &full_text);
        }

        // Filter by confidence
        relations.retain(|r| r.confidence >= self.config.confidence_threshold);
        all_relations.extend(relations);

        ExtractionResult {
            article_id: format!("{}_{}", article.oid, article.aid),
            entities: all_entities,
            relations: all_relations,
        }
    }

    /// Deduplicate entities by text
    fn deduplicate_entities(&self, entities: Vec<ExtractedEntity>) -> Vec<ExtractedEntity> {
        let mut seen: HashMap<String, ExtractedEntity> = HashMap::new();

        for entity in entities {
            let key = entity.text.to_lowercase();
            if let Some(existing) = seen.get_mut(&key) {
                // Merge: keep higher confidence and combine sources
                if entity.confidence > existing.confidence {
                    existing.confidence = entity.confidence;
                }
                if entity.source != existing.source {
                    existing.source = EntitySource::Both;
                }
            } else {
                seen.insert(key, entity);
            }
        }

        seen.into_values().collect()
    }

    /// Generate LLM prompt for article
    pub fn generate_prompt(&self, article: &ParsedArticle) -> String {
        self.prompt
            .user_template
            .replace("{title}", &article.title)
            .replace("{content}", &article.content)
            .replace("{output_schema}", &self.prompt.output_schema)
    }

    /// Get system prompt
    pub fn system_prompt(&self) -> &str {
        &self.prompt.system
    }
}

impl Default for RelationExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    /// Article ID
    pub article_id: String,

    /// Extracted entities
    pub entities: Vec<ExtractedEntity>,

    /// Extracted relations
    pub relations: Vec<ExtractedRelation>,
}

impl ExtractionResult {
    /// Get entity count by type
    pub fn entity_counts(&self) -> HashMap<EntityType, usize> {
        let mut counts = HashMap::new();
        for entity in &self.entities {
            *counts.entry(entity.entity_type).or_insert(0) += 1;
        }
        counts
    }

    /// Get relation count by type
    pub fn relation_counts(&self) -> HashMap<RelationType, usize> {
        let mut counts = HashMap::new();
        for relation in &self.relations {
            *counts.entry(relation.predicate).or_insert(0) += 1;
        }
        counts
    }

    /// Get verified relations only
    pub fn verified_relations(&self) -> Vec<&ExtractedRelation> {
        self.relations.iter().filter(|r| r.verified).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_article() -> ParsedArticle {
        ParsedArticle {
            oid: "001".to_string(),
            aid: "0001".to_string(),
            title: "삼성전자 이재용 회장, 반도체 투자 확대 발표".to_string(),
            content: "삼성전자 이재용 회장은 15일 서울에서 기자회견을 열고 반도체 분야에 10조원을 투자하겠다고 밝혔다. 이 회장은 \"글로벌 경쟁력 강화를 위해 투자를 확대할 것\"이라고 말했다.".to_string(),
            url: "https://example.com/news/1".to_string(),
            category: "economy".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_extraction_config_default() {
        let config = ExtractionConfig::default();
        assert_eq!(config.min_entity_length, 2);
        assert!(config.hallucination_check);
    }

    #[test]
    fn test_extraction_config_builder() {
        let config = ExtractionConfig::builder()
            .min_entity_length(3)
            .max_entities(100)
            .confidence_threshold(0.7)
            .hallucination_check(false)
            .build()
            .unwrap();

        assert_eq!(config.min_entity_length, 3);
        assert_eq!(config.max_entities, 100);
        assert!((config.confidence_threshold - 0.7).abs() < 0.01);
        assert!(!config.hallucination_check);
    }

    #[test]
    fn test_extraction_config_builder_validation() {
        // Invalid confidence threshold
        let result = ExtractionConfig::builder()
            .confidence_threshold(1.5)
            .build();
        assert!(result.is_err());

        // Invalid min_entity_length
        let result = ExtractionConfig::builder().min_entity_length(0).build();
        assert!(result.is_err());
    }

    #[test]
    fn test_extraction_config_validate() {
        let valid_config = ExtractionConfig::default();
        assert!(valid_config.validate().is_ok());

        let invalid_config = ExtractionConfig {
            confidence_threshold: -0.1,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_entity_type_korean_label() {
        assert_eq!(EntityType::Person.korean_label(), "인물");
        assert_eq!(EntityType::Organization.korean_label(), "기관");
    }

    #[test]
    fn test_relation_type_korean_label() {
        assert_eq!(RelationType::Said.korean_label(), "발언");
        assert_eq!(RelationType::WorksFor.korean_label(), "근무");
    }

    #[test]
    fn test_extract_entities() {
        let extractor = RelationExtractor::new();
        let text = "삼성전자 이재용 회장이 서울에서 발표했다.";

        let entities = extractor.extract_entities(text, EntitySource::Content);

        // Should find organizations and locations
        assert!(!entities.is_empty());

        // Check for Samsung
        let has_samsung = entities.iter().any(|e| e.text.contains("삼성"));
        assert!(has_samsung);
    }

    #[test]
    fn test_extract_money() {
        let extractor = RelationExtractor::new();
        let text = "총 10조원을 투자하고 1,000억원의 이익을 냈다.";

        let entities = extractor.extract_entities(text, EntitySource::Content);

        let money_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Money)
            .collect();

        assert!(!money_entities.is_empty());
    }

    #[test]
    fn test_extract_from_article() {
        let extractor = RelationExtractor::new();
        let article = sample_article();

        let result = extractor.extract_from_article(&article);

        assert!(!result.entities.is_empty());
        assert_eq!(result.article_id, "001_0001");
    }

    #[test]
    fn test_verify_relation() {
        let extractor = RelationExtractor::new();
        let text = "이재용 회장이 발표했다.";

        let mut relation = ExtractedRelation {
            subject: "이재용".to_string(),
            subject_type: EntityType::Person,
            predicate: RelationType::Said,
            object: String::new(),
            object_type: EntityType::Other,
            confidence: 0.7,
            evidence: "이재용 회장이 발표했다".to_string(),
            verified: false,
        };

        let verified = extractor.verify_relation(&mut relation, text);
        assert!(verified);
        assert!(relation.verified);
    }

    #[test]
    fn test_hallucination_detection() {
        let extractor = RelationExtractor::new();
        let text = "삼성전자가 발표했다.";

        let mut relation = ExtractedRelation {
            subject: "LG전자".to_string(), // Not in text
            subject_type: EntityType::Organization,
            predicate: RelationType::Announced,
            object: String::new(),
            object_type: EntityType::Other,
            confidence: 0.7,
            evidence: "LG전자가 발표했다".to_string(),
            verified: false,
        };

        let verified = extractor.verify_relation(&mut relation, text);
        assert!(!verified);
        assert!(!relation.verified);
    }

    #[test]
    fn test_generate_prompt() {
        let extractor = RelationExtractor::new();
        let article = sample_article();

        let prompt = extractor.generate_prompt(&article);

        assert!(prompt.contains(&article.title));
        assert!(prompt.contains("entities"));
    }

    #[test]
    fn test_extraction_result_counts() {
        let result = ExtractionResult {
            article_id: "test".to_string(),
            entities: vec![
                ExtractedEntity {
                    text: "Test1".to_string(),
                    canonical_name: None,
                    entity_type: EntityType::Person,
                    start: 0,
                    end: 5,
                    confidence: 0.9,
                    source: EntitySource::Content,
                },
                ExtractedEntity {
                    text: "Test2".to_string(),
                    canonical_name: None,
                    entity_type: EntityType::Person,
                    start: 10,
                    end: 15,
                    confidence: 0.8,
                    source: EntitySource::Content,
                },
            ],
            relations: vec![],
        };

        let counts = result.entity_counts();
        assert_eq!(counts.get(&EntityType::Person), Some(&2));
    }

    // ========================================================================
    // LLM Response Parsing Tests
    // ========================================================================

    #[test]
    fn test_entity_type_from_string() {
        assert_eq!(EntityType::from_string("Person"), EntityType::Person);
        assert_eq!(
            EntityType::from_string("ORGANIZATION"),
            EntityType::Organization
        );
        assert_eq!(EntityType::from_string("인물"), EntityType::Person);
        assert_eq!(EntityType::from_string("기관"), EntityType::Organization);
        assert_eq!(EntityType::from_string("unknown_type"), EntityType::Other);
    }

    #[test]
    fn test_relation_type_from_string() {
        assert_eq!(RelationType::from_string("said"), RelationType::Said);
        assert_eq!(
            RelationType::from_string("works_for"),
            RelationType::WorksFor
        );
        assert_eq!(RelationType::from_string("발언"), RelationType::Said);
        assert_eq!(
            RelationType::from_string("unknown_relation"),
            RelationType::Unknown
        );
    }

    #[test]
    fn test_llm_response_parsing() {
        let json = r#"{
            "entities": [
                {"text": "삼성전자", "type": "Organization", "confidence": 0.95},
                {"text": "이재용", "type": "Person", "confidence": 0.9}
            ],
            "relations": [
                {"subject": "이재용", "predicate": "leads", "object": "삼성전자", "confidence": 0.85, "evidence": "이재용 회장"}
            ]
        }"#;

        let response = LlmExtractionResponse::from_json(json).unwrap();
        assert_eq!(response.entities.len(), 2);
        assert_eq!(response.relations.len(), 1);
        assert_eq!(response.entities[0].text, "삼성전자");
        assert_eq!(response.relations[0].subject, "이재용");
    }

    #[test]
    fn test_llm_response_with_code_block() {
        let json = r#"Here is the extraction result:
```json
{
    "entities": [{"text": "테스트", "type": "Other"}],
    "relations": []
}
```
"#;

        let response = LlmExtractionResponse::from_json(json).unwrap();
        assert_eq!(response.entities.len(), 1);
        assert_eq!(response.entities[0].text, "테스트");
    }

    #[test]
    fn test_llm_response_to_entities() {
        let response = LlmExtractionResponse {
            entities: vec![LlmEntityResponse {
                text: "삼성전자".to_string(),
                entity_type: "Organization".to_string(),
                confidence: 0.9,
            }],
            relations: vec![],
        };

        let entities = response.to_entities(EntitySource::Content);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, EntityType::Organization);
    }

    #[test]
    fn test_llm_response_to_relations() {
        let response = LlmExtractionResponse {
            entities: vec![],
            relations: vec![LlmRelationResponse {
                subject: "이재용".to_string(),
                predicate: "leads".to_string(),
                object: "삼성전자".to_string(),
                confidence: 0.85,
                evidence: "이재용 회장이 이끄는 삼성전자".to_string(),
            }],
        };

        let relations = response.to_relations();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].predicate, RelationType::Leads);
    }

    // ========================================================================
    // Triple Output Tests
    // ========================================================================

    #[test]
    fn test_triple_from_relation() {
        let relation = ExtractedRelation {
            subject: "이재용".to_string(),
            subject_type: EntityType::Person,
            predicate: RelationType::Leads,
            object: "삼성전자".to_string(),
            object_type: EntityType::Organization,
            confidence: 0.9,
            evidence: "이재용 회장".to_string(),
            verified: true,
        };

        let triple = Triple::from_relation(&relation, "001_0001");
        assert!(triple.subject_id.contains("baram:entity"));
        assert_eq!(triple.predicate, "schema:founder");
        assert_eq!(triple.predicate_label, "대표");
        assert!(triple.verified);
    }

    #[test]
    fn test_triple_to_ntriples() {
        let relation = ExtractedRelation {
            subject: "테스트".to_string(),
            subject_type: EntityType::Person,
            predicate: RelationType::Said,
            object: "내용".to_string(),
            object_type: EntityType::Other,
            confidence: 0.8,
            evidence: String::new(),
            verified: true,
        };

        let triple = Triple::from_relation(&relation, "test");
        let ntriples = triple.to_ntriples();
        assert!(ntriples.contains("<"));
        assert!(ntriples.contains(">"));
        assert!(ntriples.ends_with(" ."));
    }

    #[test]
    fn test_triple_store_from_extraction() {
        let result = ExtractionResult {
            article_id: "001_0001".to_string(),
            entities: vec![ExtractedEntity {
                text: "삼성전자".to_string(),
                canonical_name: None,
                entity_type: EntityType::Organization,
                start: 0,
                end: 4,
                confidence: 0.9,
                source: EntitySource::Title,
            }],
            relations: vec![ExtractedRelation {
                subject: "이재용".to_string(),
                subject_type: EntityType::Person,
                predicate: RelationType::Leads,
                object: "삼성전자".to_string(),
                object_type: EntityType::Organization,
                confidence: 0.85,
                evidence: "이재용 회장".to_string(),
                verified: true,
            }],
        };

        let store = TripleStore::from_extraction(&result, "테스트 기사");
        assert_eq!(store.triples.len(), 1);
        assert_eq!(store.stats.total_entities, 1);
        assert_eq!(store.stats.total_relations, 1);
        assert_eq!(store.stats.verified_relations, 1);
    }

    #[test]
    fn test_triple_store_to_json_ld() {
        let result = ExtractionResult {
            article_id: "test".to_string(),
            entities: vec![],
            relations: vec![],
        };

        let store = TripleStore::from_extraction(&result, "Test Article");
        let json = store.to_json_ld().unwrap();

        assert!(json.contains("@context"));
        assert!(json.contains("schema"));
        assert!(json.contains("baram"));
    }

    #[test]
    fn test_triple_store_to_turtle() {
        let result = ExtractionResult {
            article_id: "001_0001".to_string(),
            entities: vec![],
            relations: vec![ExtractedRelation {
                subject: "A".to_string(),
                subject_type: EntityType::Person,
                predicate: RelationType::WorksFor,
                object: "B".to_string(),
                object_type: EntityType::Organization,
                confidence: 0.9,
                evidence: "A works for B".to_string(),
                verified: true,
            }],
        };

        let store = TripleStore::from_extraction(&result, "Test");
        let turtle = store.to_turtle();

        assert!(turtle.contains("@prefix schema:"));
        assert!(turtle.contains("@prefix baram:"));
        assert!(turtle.contains("# Evidence:"));
    }

    #[test]
    fn test_slug_generation() {
        assert_eq!(slug("Hello World"), "hello_world");
        assert_eq!(slug("삼성전자"), "삼성전자");
        assert_eq!(slug("test-value_123"), "test-value_123");
    }

    #[test]
    fn test_triple_stats() {
        let stats = TripleStats {
            total_entities: 10,
            total_relations: 5,
            verified_relations: 3,
            entity_types: [("인물".to_string(), 5), ("기관".to_string(), 5)].into(),
            relation_types: [("발언".to_string(), 3), ("대표".to_string(), 2)].into(),
        };

        assert_eq!(stats.total_entities, 10);
        assert_eq!(stats.verified_relations, 3);
    }

    // ========================================================================
    // Hallucination Verification Tests
    // ========================================================================

    #[test]
    fn test_verification_failure_korean_desc() {
        assert_eq!(
            VerificationFailure::SubjectNotFound.korean_desc(),
            "주어가 원문에 없음"
        );
        assert_eq!(
            VerificationFailure::ObjectNotFound.korean_desc(),
            "목적어가 원문에 없음"
        );
    }

    #[test]
    fn test_match_type_variants() {
        let exact = MatchType::Exact;
        let fuzzy = MatchType::Fuzzy;
        let none = MatchType::None;

        assert_eq!(exact, MatchType::Exact);
        assert_ne!(fuzzy, none);
    }

    #[test]
    fn test_hallucination_verifier_default() {
        let verifier = HallucinationVerifier::new();
        assert_eq!(verifier.fuzzy_threshold, 0.8);
        assert_eq!(verifier.min_confidence, 0.3);
    }

    #[test]
    fn test_hallucination_verifier_strict() {
        let verifier = HallucinationVerifier::strict();
        assert_eq!(verifier.fuzzy_threshold, 0.9);
        assert_eq!(verifier.min_confidence, 0.5);
    }

    #[test]
    fn test_hallucination_verifier_lenient() {
        let verifier = HallucinationVerifier::lenient();
        assert_eq!(verifier.fuzzy_threshold, 0.6);
        assert_eq!(verifier.min_confidence, 0.2);
    }

    #[test]
    fn test_verify_exact_match() {
        let verifier = HallucinationVerifier::new();
        let source = "삼성전자 이재용 회장이 발표했다.";

        let relation = ExtractedRelation {
            subject: "이재용".to_string(),
            subject_type: EntityType::Person,
            predicate: RelationType::Said,
            object: "".to_string(),
            object_type: EntityType::Other,
            confidence: 0.8,
            evidence: "이재용 회장이 발표했다".to_string(),
            verified: false,
        };

        let result = verifier.verify(&relation, source);
        assert!(result.verified);
        assert!(result.subject_match.found);
        assert_eq!(result.subject_match.match_type, MatchType::Exact);
    }

    #[test]
    fn test_verify_subject_not_found() {
        let verifier = HallucinationVerifier::new();
        let source = "삼성전자가 발표했다.";

        let relation = ExtractedRelation {
            subject: "LG전자".to_string(), // Not in source
            subject_type: EntityType::Organization,
            predicate: RelationType::Announced,
            object: "".to_string(),
            object_type: EntityType::Other,
            confidence: 0.8,
            evidence: "".to_string(),
            verified: false,
        };

        let result = verifier.verify(&relation, source);
        assert!(!result.verified);
        assert!(result
            .failures
            .contains(&VerificationFailure::SubjectNotFound));
    }

    #[test]
    fn test_verify_partial_match() {
        let verifier = HallucinationVerifier::lenient();
        let source = "삼성전자 이재용 회장이 서울에서 발표했다.";

        let relation = ExtractedRelation {
            subject: "이재용 회장".to_string(),
            subject_type: EntityType::Person,
            predicate: RelationType::Said,
            object: "".to_string(),
            object_type: EntityType::Other,
            confidence: 0.8,
            evidence: "이재용 회장이 발표".to_string(),
            verified: false,
        };

        let result = verifier.verify(&relation, source);
        assert!(result.subject_match.found);
    }

    #[test]
    fn test_verify_batch() {
        let verifier = HallucinationVerifier::new();
        let source = "삼성전자 이재용 회장이 발표했다. SK하이닉스도 참여했다.";

        let relations = vec![
            ExtractedRelation {
                subject: "이재용".to_string(),
                subject_type: EntityType::Person,
                predicate: RelationType::Said,
                object: "".to_string(),
                object_type: EntityType::Other,
                confidence: 0.8,
                evidence: "".to_string(),
                verified: false,
            },
            ExtractedRelation {
                subject: "SK하이닉스".to_string(),
                subject_type: EntityType::Organization,
                predicate: RelationType::ParticipatedIn,
                object: "".to_string(),
                object_type: EntityType::Other,
                confidence: 0.7,
                evidence: "".to_string(),
                verified: false,
            },
        ];

        let results = verifier.verify_batch(&relations, source);
        assert_eq!(results.len(), 2);
        assert!(results[0].verified);
        assert!(results[1].verified);
    }

    #[test]
    fn test_verify_and_update() {
        let verifier = HallucinationVerifier::new();
        let source = "삼성전자가 발표했다.";

        let mut relations = vec![
            ExtractedRelation {
                subject: "삼성전자".to_string(),
                subject_type: EntityType::Organization,
                predicate: RelationType::Announced,
                object: "".to_string(),
                object_type: EntityType::Other,
                confidence: 0.7,
                evidence: "".to_string(),
                verified: false,
            },
            ExtractedRelation {
                subject: "애플".to_string(), // Not in source
                subject_type: EntityType::Organization,
                predicate: RelationType::Announced,
                object: "".to_string(),
                object_type: EntityType::Other,
                confidence: 0.7,
                evidence: "".to_string(),
                verified: false,
            },
        ];

        let summary = verifier.verify_and_update(&mut relations, source);

        assert_eq!(summary.total, 2);
        assert_eq!(summary.verified, 1);
        assert_eq!(summary.failed, 1);
        assert!(relations[0].verified);
        assert!(!relations[1].verified);
    }

    #[test]
    fn test_verification_summary_rate() {
        let summary = VerificationSummary {
            total: 10,
            verified: 8,
            failed: 2,
            failure_counts: [(VerificationFailure::SubjectNotFound, 2)].into(),
        };

        assert_eq!(summary.verification_rate(), 80.0);
        assert_eq!(
            summary.most_common_failure(),
            Some(&VerificationFailure::SubjectNotFound)
        );
    }

    #[test]
    fn test_match_detail_default() {
        let detail = MatchDetail::default();
        assert!(!detail.found);
        assert_eq!(detail.match_type, MatchType::None);
        assert_eq!(detail.similarity, 0.0);
    }

    #[test]
    fn test_confidence_adjustment() {
        let verifier = HallucinationVerifier::new();
        let source = "테스트 문장입니다.";

        let relation = ExtractedRelation {
            subject: "테스트".to_string(),
            subject_type: EntityType::Other,
            predicate: RelationType::Unknown,
            object: "".to_string(),
            object_type: EntityType::Other,
            confidence: 0.5,
            evidence: "".to_string(),
            verified: false,
        };

        let result = verifier.verify(&relation, source);
        // Exact match should boost confidence
        assert!(result.adjusted_confidence > result.original_confidence);
    }
}
