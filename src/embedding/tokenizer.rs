//! Text tokenization and chunking for embedding generation
//!
//! This module provides text preprocessing for vector embeddings:
//! - Tokenization using HuggingFace tokenizers
//! - Sliding window chunking for long texts
//! - Token statistics and validation

use anyhow::Result;
use hf_hub::{api::sync::Api, Repo, RepoType};
use std::path::Path;
use tokenizers::Tokenizer;

/// Configuration for text chunking
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Maximum tokens per chunk
    pub max_tokens: usize,

    /// Overlap tokens between chunks (sliding window)
    pub overlap_tokens: usize,

    /// Minimum chunk size (skip if smaller)
    pub min_chunk_tokens: usize,

    /// Whether to preserve sentence boundaries
    pub preserve_sentences: bool,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            overlap_tokens: 64,
            min_chunk_tokens: 32,
            preserve_sentences: true,
        }
    }
}

/// Text chunk with metadata
#[derive(Debug, Clone)]
pub struct TextChunk {
    /// Chunk text content
    pub text: String,

    /// Token count
    pub token_count: usize,

    /// Chunk index (0-based)
    pub chunk_index: usize,

    /// Total chunks for this text
    pub total_chunks: usize,

    /// Start character position in original text
    pub start_pos: usize,

    /// End character position in original text
    pub end_pos: usize,
}

/// Tokenizer statistics
#[derive(Debug, Clone, Default)]
pub struct TokenizerStats {
    /// Total texts processed
    pub texts_processed: usize,

    /// Total tokens generated
    pub total_tokens: usize,

    /// Total chunks created
    pub total_chunks: usize,

    /// Unknown token count
    pub unknown_tokens: usize,

    /// Average tokens per text
    pub avg_tokens_per_text: f64,
}

/// Text tokenizer wrapper with chunking support
pub struct TextTokenizer {
    /// HuggingFace tokenizer
    tokenizer: Tokenizer,

    /// Chunking configuration
    config: ChunkConfig,

    /// Statistics
    stats: TokenizerStats,

    /// Unknown token ID
    unk_token_id: Option<u32>,
}

impl TextTokenizer {
    /// Create a new tokenizer from a pretrained model (downloads from HuggingFace Hub)
    pub fn from_pretrained(model_name: &str) -> Result<Self> {
        // Download tokenizer from HuggingFace Hub
        let api =
            Api::new().map_err(|e| anyhow::anyhow!("Failed to create HuggingFace API: {e}"))?;
        let repo = api.repo(Repo::new(model_name.to_string(), RepoType::Model));

        let tokenizer_path = repo
            .get("tokenizer.json")
            .map_err(|e| anyhow::anyhow!("Failed to download tokenizer: {e}"))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {e}"))?;

        let unk_token_id = tokenizer.token_to_id("[UNK]");

        Ok(Self {
            tokenizer,
            config: ChunkConfig::default(),
            stats: TokenizerStats::default(),
            unk_token_id,
        })
    }

    /// Create a tokenizer from a local file
    pub fn from_file(path: &Path) -> Result<Self> {
        let tokenizer = Tokenizer::from_file(path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer from file: {e}"))?;

        let unk_token_id = tokenizer.token_to_id("[UNK]");

        Ok(Self {
            tokenizer,
            config: ChunkConfig::default(),
            stats: TokenizerStats::default(),
            unk_token_id,
        })
    }

    /// Set chunking configuration
    pub fn with_config(mut self, config: ChunkConfig) -> Self {
        self.config = config;
        self
    }

    /// Tokenize text and return token IDs
    pub fn tokenize(&mut self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {e}"))?;

        let ids = encoding.get_ids().to_vec();

        // Update statistics
        self.stats.texts_processed += 1;
        self.stats.total_tokens += ids.len();

        // Count unknown tokens
        if let Some(unk_id) = self.unk_token_id {
            let unk_count = ids.iter().filter(|&&id| id == unk_id).count();
            self.stats.unknown_tokens += unk_count;
        }

        self.stats.avg_tokens_per_text =
            self.stats.total_tokens as f64 / self.stats.texts_processed as f64;

        Ok(ids)
    }

    /// Decode token IDs back to text
    pub fn decode(&self, ids: &[u32]) -> Result<String> {
        self.tokenizer
            .decode(ids, true)
            .map_err(|e| anyhow::anyhow!("Decoding failed: {e}"))
    }

    /// Get token count for text without full tokenization overhead
    pub fn count_tokens(&self, text: &str) -> Result<usize> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("Token counting failed: {e}"))?;

        Ok(encoding.get_ids().len())
    }

    /// Chunk text using sliding window approach
    pub fn chunk_text(&mut self, text: &str) -> Result<Vec<TextChunk>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("Chunking failed: {e}"))?;

        let ids = encoding.get_ids();
        let offsets = encoding.get_offsets();

        if ids.len() <= self.config.max_tokens {
            // Text fits in a single chunk
            self.stats.total_chunks += 1;
            return Ok(vec![TextChunk {
                text: text.to_string(),
                token_count: ids.len(),
                chunk_index: 0,
                total_chunks: 1,
                start_pos: 0,
                end_pos: text.len(),
            }]);
        }

        let mut chunks = Vec::new();
        let mut start_idx = 0;
        let step = self.config.max_tokens - self.config.overlap_tokens;

        while start_idx < ids.len() {
            let end_idx = (start_idx + self.config.max_tokens).min(ids.len());

            // Get character positions from offsets
            let start_pos = offsets.get(start_idx).map(|(s, _)| *s).unwrap_or(0);
            let end_pos = offsets
                .get(end_idx.saturating_sub(1))
                .map(|(_, e)| *e)
                .unwrap_or(text.len());

            // Extract chunk text
            let chunk_text = if end_pos <= text.len() {
                text[start_pos..end_pos].to_string()
            } else {
                text[start_pos..].to_string()
            };

            let token_count = end_idx - start_idx;

            // Skip chunks that are too small (except the last one)
            if token_count >= self.config.min_chunk_tokens || start_idx + step >= ids.len() {
                chunks.push(TextChunk {
                    text: chunk_text,
                    token_count,
                    chunk_index: chunks.len(),
                    total_chunks: 0, // Will be updated later
                    start_pos,
                    end_pos,
                });
            }

            start_idx += step;

            // Prevent infinite loop
            if step == 0 {
                break;
            }
        }

        // Update total_chunks for all chunks
        let total = chunks.len();
        for chunk in &mut chunks {
            chunk.total_chunks = total;
        }

        self.stats.total_chunks += chunks.len();

        Ok(chunks)
    }

    /// Chunk text with sentence boundary preservation
    pub fn chunk_text_sentences(&mut self, text: &str) -> Result<Vec<TextChunk>> {
        if !self.config.preserve_sentences {
            return self.chunk_text(text);
        }

        // Split by sentence boundaries
        let sentences: Vec<&str> = split_sentences(text);

        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_tokens = 0;
        let mut chunk_start = 0;

        for sentence in sentences {
            let sentence_tokens = self.count_tokens(sentence)?;

            // If adding this sentence would exceed max tokens
            if current_tokens + sentence_tokens > self.config.max_tokens
                && !current_chunk.is_empty()
            {
                // Save current chunk
                let chunk_end = chunk_start + current_chunk.len();
                chunks.push(TextChunk {
                    text: current_chunk.trim().to_string(),
                    token_count: current_tokens,
                    chunk_index: chunks.len(),
                    total_chunks: 0,
                    start_pos: chunk_start,
                    end_pos: chunk_end,
                });

                // Start new chunk with overlap
                let overlap_text = get_overlap_text(&current_chunk, self.config.overlap_tokens);
                current_chunk = overlap_text;
                current_tokens = self.count_tokens(&current_chunk)?;
                chunk_start = chunk_end - current_chunk.len();
            }

            current_chunk.push_str(sentence);
            current_chunk.push(' ');
            current_tokens += sentence_tokens;
        }

        // Add final chunk
        if current_tokens >= self.config.min_chunk_tokens || chunks.is_empty() {
            let chunk_end = chunk_start + current_chunk.len();
            chunks.push(TextChunk {
                text: current_chunk.trim().to_string(),
                token_count: current_tokens,
                chunk_index: chunks.len(),
                total_chunks: 0,
                start_pos: chunk_start,
                end_pos: chunk_end,
            });
        }

        // Update total_chunks
        let total = chunks.len();
        for chunk in &mut chunks {
            chunk.total_chunks = total;
        }

        self.stats.total_chunks += chunks.len();

        Ok(chunks)
    }

    /// Get unknown token ratio (0.0 - 1.0)
    pub fn unknown_token_ratio(&self) -> f64 {
        if self.stats.total_tokens == 0 {
            return 0.0;
        }
        self.stats.unknown_tokens as f64 / self.stats.total_tokens as f64
    }

    /// Get statistics
    pub fn stats(&self) -> &TokenizerStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = TokenizerStats::default();
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.get_vocab_size(true)
    }
}

/// Split text into sentences (simple implementation)
fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;

    for (i, c) in text.char_indices() {
        if c == '.' || c == '!' || c == '?' || c == '。' || c == '！' || c == '？' {
            // Check if this is end of sentence (not abbreviation, etc.)
            let next_char = text[i..].chars().nth(1);
            if next_char
                .map(|c| c.is_whitespace() || c == '"' || c == '\'')
                .unwrap_or(true)
            {
                let end = i + c.len_utf8();
                if end > start {
                    sentences.push(&text[start..end]);
                    start = end;
                }
            }
        }
    }

    // Add remaining text
    if start < text.len() {
        let remaining = text[start..].trim();
        if !remaining.is_empty() {
            sentences.push(&text[start..]);
        }
    }

    sentences
}

/// Get overlap text from the end of a string (approximate by words)
fn get_overlap_text(text: &str, _target_tokens: usize) -> String {
    // Simple implementation: take last ~20% of words
    let words: Vec<&str> = text.split_whitespace().collect();
    let overlap_words = (words.len() / 5).max(1);
    words[words.len().saturating_sub(overlap_words)..].join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_config_default() {
        let config = ChunkConfig::default();
        assert_eq!(config.max_tokens, 512);
        assert_eq!(config.overlap_tokens, 64);
        assert_eq!(config.min_chunk_tokens, 32);
        assert!(config.preserve_sentences);
    }

    #[test]
    fn test_split_sentences() {
        let text = "Hello world. This is a test. How are you?";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 3);
        assert!(sentences[0].contains("Hello"));
        assert!(sentences[1].contains("test"));
        assert!(sentences[2].contains("you"));
    }

    #[test]
    fn test_split_sentences_korean() {
        // Test with standard Korean sentence endings
        let text = "안녕하세요. 테스트입니다. 잘 되나요?";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_get_overlap_text() {
        let text = "one two three four five six seven eight nine ten";
        let overlap = get_overlap_text(text, 2);
        assert!(!overlap.is_empty());
        assert!(overlap.contains("ten") || overlap.contains("nine"));
    }

    #[test]
    fn test_text_chunk_struct() {
        let chunk = TextChunk {
            text: "Hello world".to_string(),
            token_count: 2,
            chunk_index: 0,
            total_chunks: 1,
            start_pos: 0,
            end_pos: 11,
        };
        assert_eq!(chunk.chunk_index, 0);
        assert_eq!(chunk.total_chunks, 1);
    }

    #[test]
    fn test_tokenizer_stats_default() {
        let stats = TokenizerStats::default();
        assert_eq!(stats.texts_processed, 0);
        assert_eq!(stats.total_tokens, 0);
        assert_eq!(stats.unknown_tokens, 0);
    }

    // Integration tests require actual model files
    #[test]
    #[ignore = "Requires tokenizer model file"]
    fn test_tokenizer_from_file() {
        let path = Path::new("models/tokenizer.json");
        let tokenizer = TextTokenizer::from_file(path);
        assert!(tokenizer.is_ok());
    }
}
