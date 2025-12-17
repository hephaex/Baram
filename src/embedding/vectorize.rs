//! Vector embedding generation using Candle
//!
//! This module provides text-to-vector embedding using transformer models:
//! - BERT-based models for Korean text
//! - CPU/GPU fallback support
//! - Batch processing for efficiency
//! - Mean pooling and L2 normalization

use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use hf_hub::{api::sync::Api, Repo, RepoType};
use std::path::PathBuf;
use tokenizers::Tokenizer;

/// Embedding model configuration
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Model identifier (HuggingFace model ID or local path)
    pub model_id: String,

    /// Embedding dimension
    pub embedding_dim: usize,

    /// Maximum sequence length
    pub max_seq_length: usize,

    /// Use GPU if available
    pub use_gpu: bool,

    /// Batch size for inference
    pub batch_size: usize,

    /// Normalize embeddings (L2)
    pub normalize: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_id: "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2".to_string(),
            embedding_dim: 384,
            max_seq_length: 512,
            use_gpu: true,
            batch_size: 32,
            normalize: true,
        }
    }
}

/// Embedding generation statistics
#[derive(Debug, Clone, Default)]
pub struct EmbeddingStats {
    /// Total texts embedded
    pub texts_embedded: usize,

    /// Total batches processed
    pub batches_processed: usize,

    /// Average embedding time (ms)
    pub avg_time_ms: f64,

    /// Device used (cpu/cuda)
    pub device: String,
}

/// Vector embedding generator
pub struct Embedder {
    /// BERT model
    model: BertModel,

    /// Tokenizer
    tokenizer: Tokenizer,

    /// Device (CPU or GPU)
    device: Device,

    /// Configuration
    config: EmbeddingConfig,

    /// Statistics
    stats: EmbeddingStats,
}

impl Embedder {
    /// Create a new embedder from HuggingFace model
    pub fn from_pretrained(config: EmbeddingConfig) -> Result<Self> {
        // Determine device
        let device = if config.use_gpu {
            Device::cuda_if_available(0).unwrap_or(Device::Cpu)
        } else {
            Device::Cpu
        };

        let device_name = match &device {
            Device::Cpu => "cpu",
            Device::Cuda(_) => "cuda",
            Device::Metal(_) => "metal",
        };

        tracing::info!(
            model = %config.model_id,
            device = device_name,
            "Loading embedding model"
        );

        // Download model from HuggingFace Hub
        let api = Api::new().context("Failed to create HuggingFace API")?;
        let repo = api.repo(Repo::new(config.model_id.clone(), RepoType::Model));

        let tokenizer_path = repo
            .get("tokenizer.json")
            .context("Failed to download tokenizer")?;

        let config_path = repo
            .get("config.json")
            .context("Failed to download config")?;

        let weights_path = repo
            .get("model.safetensors")
            .or_else(|_| repo.get("pytorch_model.bin"))
            .context("Failed to download model weights")?;

        Self::from_files(tokenizer_path, config_path, weights_path, config, device)
    }

    /// Create embedder from local files
    pub fn from_files(
        tokenizer_path: PathBuf,
        config_path: PathBuf,
        weights_path: PathBuf,
        config: EmbeddingConfig,
        device: Device,
    ) -> Result<Self> {
        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {e}"))?;

        // Load model config
        let bert_config: BertConfig = serde_json::from_str(
            &std::fs::read_to_string(&config_path).context("Failed to read config file")?,
        )
        .context("Failed to parse config")?;

        // Load model weights
        let vb = if weights_path.extension().map(|e| e == "safetensors").unwrap_or(false) {
            unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)
                    .context("Failed to load safetensors")?
            }
        } else {
            // For .bin files (PyTorch format)
            VarBuilder::from_pth(&weights_path, DType::F32, &device)
                .context("Failed to load PyTorch weights")?
        };

        // Build model
        let model = BertModel::load(vb, &bert_config).context("Failed to build BERT model")?;

        let device_name = match &device {
            Device::Cpu => "cpu".to_string(),
            Device::Cuda(_) => "cuda".to_string(),
            Device::Metal(_) => "metal".to_string(),
        };

        Ok(Self {
            model,
            tokenizer,
            device,
            config,
            stats: EmbeddingStats {
                device: device_name,
                ..Default::default()
            },
        })
    }

    /// Generate embedding for a single text
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[text.to_string()])?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }

    /// Generate embeddings for multiple texts (batched)
    pub fn embed_batch(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let start_time = std::time::Instant::now();

        let mut all_embeddings = Vec::with_capacity(texts.len());

        // Process in batches
        for batch in texts.chunks(self.config.batch_size) {
            let batch_embeddings = self.embed_batch_internal(batch)?;
            all_embeddings.extend(batch_embeddings);
            self.stats.batches_processed += 1;
        }

        // Update statistics
        let elapsed_ms = start_time.elapsed().as_millis() as f64;
        self.stats.texts_embedded += texts.len();

        let total_time = self.stats.avg_time_ms * (self.stats.texts_embedded - texts.len()) as f64
            + elapsed_ms;
        self.stats.avg_time_ms = total_time / self.stats.texts_embedded as f64;

        Ok(all_embeddings)
    }

    /// Internal batch embedding
    fn embed_batch_internal(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Tokenize all texts
        let encodings: Vec<_> = texts
            .iter()
            .map(|text| {
                self.tokenizer
                    .encode(text.as_str(), true)
                    .map_err(|e| anyhow::anyhow!("Tokenization failed: {e}"))
            })
            .collect::<Result<Vec<_>>>()?;

        // Pad sequences to same length
        let max_len = encodings
            .iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0)
            .min(self.config.max_seq_length);

        let batch_size = encodings.len();

        // Create input tensors
        let mut input_ids_vec = Vec::with_capacity(batch_size * max_len);
        let mut attention_mask_vec = Vec::with_capacity(batch_size * max_len);
        let mut token_type_ids_vec = Vec::with_capacity(batch_size * max_len);

        for encoding in &encodings {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();
            let type_ids = encoding.get_type_ids();

            let seq_len = ids.len().min(max_len);

            // Add actual tokens
            input_ids_vec.extend(ids.iter().take(seq_len).map(|&x| x as i64));
            attention_mask_vec.extend(mask.iter().take(seq_len).map(|&x| x as i64));
            token_type_ids_vec.extend(type_ids.iter().take(seq_len).map(|&x| x as i64));

            // Add padding
            let padding_len = max_len - seq_len;
            input_ids_vec.extend(std::iter::repeat(0i64).take(padding_len));
            attention_mask_vec.extend(std::iter::repeat(0i64).take(padding_len));
            token_type_ids_vec.extend(std::iter::repeat(0i64).take(padding_len));
        }

        // Create tensors
        let input_ids =
            Tensor::from_vec(input_ids_vec, (batch_size, max_len), &self.device)?;
        let attention_mask =
            Tensor::from_vec(attention_mask_vec, (batch_size, max_len), &self.device)?;
        let token_type_ids =
            Tensor::from_vec(token_type_ids_vec, (batch_size, max_len), &self.device)?;

        // Forward pass
        let output = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

        // Mean pooling
        let embeddings = self.mean_pooling(&output, &attention_mask)?;

        // L2 normalize if configured
        let embeddings = if self.config.normalize {
            self.l2_normalize(&embeddings)?
        } else {
            embeddings
        };

        // Convert to Vec<Vec<f32>>
        let embeddings_vec = embeddings.to_vec2::<f32>()?;

        Ok(embeddings_vec)
    }

    /// Mean pooling over sequence dimension with attention mask
    fn mean_pooling(&self, hidden_states: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        // Expand attention mask to hidden size
        let mask = attention_mask.unsqueeze(2)?.to_dtype(DType::F32)?;

        // Apply mask and sum
        let masked = hidden_states.broadcast_mul(&mask)?;
        let summed = masked.sum(1)?;

        // Divide by actual sequence lengths
        let lengths = mask.sum(1)?.clamp(1e-9, f64::MAX)?;
        let pooled = summed.broadcast_div(&lengths)?;

        Ok(pooled)
    }

    /// L2 normalize embeddings
    fn l2_normalize(&self, embeddings: &Tensor) -> Result<Tensor> {
        let norms = embeddings.sqr()?.sum_keepdim(1)?.sqrt()?;
        let norms = norms.clamp(1e-12, f64::MAX)?;
        let normalized = embeddings.broadcast_div(&norms)?;
        Ok(normalized)
    }

    /// Get embedding dimension
    pub fn embedding_dim(&self) -> usize {
        self.config.embedding_dim
    }

    /// Get statistics
    pub fn stats(&self) -> &EmbeddingStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = EmbeddingStats {
            device: self.stats.device.clone(),
            ..Default::default()
        };
    }

    /// Check if using GPU
    pub fn is_gpu(&self) -> bool {
        matches!(self.device, Device::Cuda(_) | Device::Metal(_))
    }

    /// Get device name
    pub fn device_name(&self) -> &str {
        &self.stats.device
    }
}

/// Compute cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Compute dot product between two vectors
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// L2 normalize a vector in place
pub fn l2_normalize_vec(vec: &mut [f32]) {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.embedding_dim, 384);
        assert_eq!(config.max_seq_length, 512);
        assert!(config.normalize);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &c);
        assert!(sim.abs() < 1e-6);

        let d = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &d);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let dot = dot_product(&a, &b);
        assert!((dot - 32.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_vec() {
        let mut vec = vec![3.0, 4.0];
        l2_normalize_vec(&mut vec);
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_stats_default() {
        let stats = EmbeddingStats::default();
        assert_eq!(stats.texts_embedded, 0);
        assert_eq!(stats.batches_processed, 0);
    }

    // Integration tests require model download
    #[test]
    #[ignore = "Requires model download"]
    fn test_embedder_from_pretrained() {
        let config = EmbeddingConfig::default();
        let embedder = Embedder::from_pretrained(config);
        assert!(embedder.is_ok());
    }
}
