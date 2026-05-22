//! ONNX-based embedding generation
//!
//! Uses all-MiniLM-L6-v2 model for fast, high-quality 384-dimensional embeddings.

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::PathBuf;
use tokenizers::Tokenizer;

use crate::config::EmbeddingConfig;
use crate::error::{Error, Result};

/// ONNX-based text embedder
pub struct OnnxEmbedder {
    /// ONNX Runtime session
    session: Session,
    /// HuggingFace tokenizer
    tokenizer: Tokenizer,
    /// Embedding dimensions
    dimensions: usize,
    /// Maximum sequence length
    max_length: usize,
    /// Batch size
    batch_size: usize,
}

impl OnnxEmbedder {
    /// Create a new ONNX embedder
    pub async fn new(config: &EmbeddingConfig) -> Result<Self> {
        tracing::info!("Initializing ONNX embedder with model: {}", config.model);

        // Ensure cache directory exists
        std::fs::create_dir_all(&config.cache_dir).map_err(|e| {
            Error::Config(format!("Failed to create cache directory: {}", e))
        })?;

        // Model paths
        let model_path = config.cache_dir.join("model.onnx");
        let tokenizer_path = config.cache_dir.join("tokenizer.json");

        // Download model if not cached
        if !model_path.exists() {
            download_model(&config.model, &model_path).await?;
        }

        // Download tokenizer if not cached
        if !tokenizer_path.exists() {
            download_tokenizer(&config.model, &tokenizer_path).await?;
        }

        // Load ONNX session
        let session = Session::builder()
            .map_err(|e| Error::Embedding(format!("Failed to create session builder: {}", e)))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| Error::Embedding(format!("Failed to set optimization level: {}", e)))?
            .with_intra_threads(4)
            .map_err(|e| Error::Embedding(format!("Failed to set threads: {}", e)))?
            .commit_from_file(&model_path)
            .map_err(|e| Error::Embedding(format!("Failed to load model: {}", e)))?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| Error::Embedding(format!("Failed to load tokenizer: {}", e)))?;

        tracing::info!("ONNX embedder initialized successfully");

        Ok(Self {
            session,
            tokenizer,
            dimensions: config.dimensions,
            max_length: config.max_length,
            batch_size: config.batch_size,
        })
    }

    /// Get embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Embed a single text
    pub fn embed_one(&mut self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[text])?;
        embeddings.into_iter().next().ok_or_else(|| {
            Error::Embedding("Empty embedding result".to_string())
        })
    }

    /// Embed multiple texts
    pub fn embed_batch(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        // Process in batches
        for batch in texts.chunks(self.batch_size) {
            let batch_embeddings = self.embed_batch_internal(batch)?;
            all_embeddings.extend(batch_embeddings);
        }

        Ok(all_embeddings)
    }

    /// Internal batch embedding
    fn embed_batch_internal(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let batch_size = texts.len();

        // Tokenize
        let encodings = self.tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| Error::Embedding(format!("Tokenization failed: {}", e)))?;

        // Prepare inputs
        let max_len = encodings.iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0)
            .min(self.max_length);

        let mut input_ids = vec![0i64; batch_size * max_len];
        let mut attention_mask = vec![0i64; batch_size * max_len];
        let mut token_type_ids = vec![0i64; batch_size * max_len];

        for (i, encoding) in encodings.iter().enumerate() {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();
            let types = encoding.get_type_ids();

            let len = ids.len().min(max_len);

            for j in 0..len {
                input_ids[i * max_len + j] = ids[j] as i64;
                attention_mask[i * max_len + j] = mask[j] as i64;
                token_type_ids[i * max_len + j] = types[j] as i64;
            }
        }

        // Create tensors using ORT 2.0 API
        let input_ids_tensor = Tensor::from_array((
            vec![batch_size, max_len],
            input_ids.clone().into_boxed_slice(),
        )).map_err(|e| Error::Embedding(format!("Input tensor creation failed: {}", e)))?;

        let attention_mask_tensor = Tensor::from_array((
            vec![batch_size, max_len],
            attention_mask.clone().into_boxed_slice(),
        )).map_err(|e| Error::Embedding(format!("Attention mask tensor creation failed: {}", e)))?;

        let token_type_ids_tensor = Tensor::from_array((
            vec![batch_size, max_len],
            token_type_ids.into_boxed_slice(),
        )).map_err(|e| Error::Embedding(format!("Token type tensor creation failed: {}", e)))?;

        // Build inputs vector
        let inputs = vec![
            ("input_ids", input_ids_tensor.into_dyn()),
            ("attention_mask", attention_mask_tensor.into_dyn()),
            ("token_type_ids", token_type_ids_tensor.into_dyn()),
        ];

        // Run inference
        let outputs = self.session
            .run(inputs)
            .map_err(|e| Error::Embedding(format!("Inference failed: {}", e)))?;

        // Extract embeddings (last_hidden_state)
        let output_iter: Vec<_> = outputs.iter().collect();
        let output = output_iter
            .iter()
            .find(|(name, _)| *name == "last_hidden_state")
            .or_else(|| output_iter.first())
            .map(|(_, v)| v)
            .ok_or_else(|| Error::Embedding("No output tensor".to_string()))?;

        let (tensor_shape, tensor_data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Embedding(format!("Failed to extract tensor: {}", e)))?;

        // Convert Shape to Vec<usize>
        let dims: Vec<usize> = tensor_shape.iter().map(|&d| d as usize).collect();
        let hidden_size = dims.get(2).copied().unwrap_or(self.dimensions);

        // Mean pooling with attention mask
        let mut embeddings = Vec::with_capacity(batch_size);

        for i in 0..batch_size {
            let mut sum = vec![0.0f32; hidden_size];
            let mut count = 0.0f32;

            for j in 0..max_len {
                let mask_val = attention_mask[i * max_len + j] as f32;
                if mask_val > 0.0 {
                    for (k, sum_val) in sum.iter_mut().enumerate() {
                        let idx = i * max_len * hidden_size + j * hidden_size + k;
                        if idx < tensor_data.len() {
                            *sum_val += tensor_data[idx] * mask_val;
                        }
                    }
                    count += mask_val;
                }
            }

            // Normalize by count
            if count > 0.0 {
                for val in &mut sum {
                    *val /= count;
                }
            }

            // L2 normalize
            let norm: f32 = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for val in &mut sum {
                    *val /= norm;
                }
            }

            embeddings.push(sum);
        }

        Ok(embeddings)
    }

    /// Compute cosine similarity between two embeddings
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }
}

/// Download ONNX model
async fn download_model(model_name: &str, path: &PathBuf) -> Result<()> {
    let url = format!(
        "https://huggingface.co/sentence-transformers/{}/resolve/main/onnx/model.onnx",
        model_name
    );

    tracing::info!("Downloading model from: {}", url);

    let response = reqwest::get(&url).await.map_err(|e| {
        Error::Embedding(format!("Failed to download model: {}", e))
    })?;

    if !response.status().is_success() {
        return Err(Error::Embedding(format!(
            "Model download failed: HTTP {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        Error::Embedding(format!("Failed to read model bytes: {}", e))
    })?;

    std::fs::write(path, &bytes).map_err(|e| {
        Error::Embedding(format!("Failed to save model: {}", e))
    })?;

    tracing::info!("Model downloaded successfully ({} bytes)", bytes.len());

    Ok(())
}

/// Download tokenizer
async fn download_tokenizer(model_name: &str, path: &PathBuf) -> Result<()> {
    let url = format!(
        "https://huggingface.co/sentence-transformers/{}/resolve/main/tokenizer.json",
        model_name
    );

    tracing::info!("Downloading tokenizer from: {}", url);

    let response = reqwest::get(&url).await.map_err(|e| {
        Error::Embedding(format!("Failed to download tokenizer: {}", e))
    })?;

    if !response.status().is_success() {
        return Err(Error::Embedding(format!(
            "Tokenizer download failed: HTTP {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        Error::Embedding(format!("Failed to read tokenizer bytes: {}", e))
    })?;

    std::fs::write(path, &bytes).map_err(|e| {
        Error::Embedding(format!("Failed to save tokenizer: {}", e))
    })?;

    tracing::info!("Tokenizer downloaded successfully");

    Ok(())
}
