use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Document {
    id: Uuid,
    path: String,
    content: String,
    hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Chunk {
    id: Uuid,
    doc_id: Uuid,
    content: String,
    embedding: Vec<f32>,
}

#[derive(Debug, Clone)]
struct SearchResult {
    chunk: Chunk,
    similarity: f32,
}

#[derive(Debug, Serialize)]
struct EmbedRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

// ============================================================================
// RAG SYSTEM
// ============================================================================

struct SimpleRAG {
    client: Client,
    chunks: Vec<Chunk>,
    documents: Vec<Document>,
}

impl SimpleRAG {
    /// Initialize the RAG system
    async fn new() -> Result<Self> {
        println!("🚀 Initializing RAG system...");
        
        let client = Client::new();
        
        // Test connection to Ollama
        match client
            .get("http://localhost:11434/api/tags")
            .send()
            .await
        {
            Ok(_) => println!("✅ Connected to Ollama"),
            Err(_) => {
                println!("❌ Cannot connect to Ollama.");
                println!("   Please install and start Ollama:");
                println!("   1. Install: brew install ollama");
                println!("   2. Start: ollama serve");
                println!("   3. Download model: ollama pull nomic-embed-text");
                anyhow::bail!("Ollama not running");
            }
        }

        Ok(Self {
            client,
            chunks: Vec::new(),
            documents: Vec::new(),
        })
    }

    /// Get embedding from Ollama
    async fn get_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbedRequest {
            model: "nomic-embed-text".to_string(),
            prompt: text.to_string(),
        };

        let response = self
            .client
            .post("http://localhost:11434/api/embeddings")
            .json(&request)
            .send()
            .await?;

        let embed_response: EmbedResponse = response.json().await?;
        Ok(embed_response.embedding)
    }

    /// Ingest all documents from a folder
    async fn ingest_folder(&mut self, folder_path: &Path) -> Result<()> {
        println!("\n📂 Scanning folder: {:?}", folder_path);

        for entry in WalkDir::new(folder_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if ext == "txt" || ext == "md" {
                        println!("  📄 Processing: {}", path.display());
                        self.ingest_file(path).await?;
                    }
                }
            }
        }

        println!("✅ Ingested {} documents", self.documents.len());
        println!("✅ Created {} chunks", self.chunks.len());

        Ok(())
    }

    /// Ingest a single file
    async fn ingest_file(&mut self, file_path: &Path) -> Result<()> {
        // Read file content
        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read file")?;

        // Create document
        let doc = Document {
            id: Uuid::new_v4(),
            path: file_path.to_string_lossy().to_string(),
            content: content.clone(),
            hash: self.hash_content(&content),
        };

        // Chunk the document
        let chunks = self.chunk_text(&content, &doc.id);

        // Generate embeddings for chunks
        for chunk in chunks {
            let embedding = self.get_embedding(&chunk.content).await?;
            self.chunks.push(Chunk {
                id: chunk.id,
                doc_id: chunk.doc_id,
                content: chunk.content,
                embedding,
            });
        }

        self.documents.push(doc);

        Ok(())
    }

    /// Split text into chunks
    fn chunk_text(&self, text: &str, doc_id: &Uuid) -> Vec<Chunk> {
        const CHUNK_SIZE: usize = 500;
        const OVERLAP: usize = 50;

        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;

        while start < chars.len() {
            let end = (start + CHUNK_SIZE).min(chars.len());
            let chunk_text: String = chars[start..end].iter().collect();

            chunks.push(Chunk {
                id: Uuid::new_v4(),
                doc_id: *doc_id,
                content: chunk_text.trim().to_string(),
                embedding: vec![], // Will be filled later
            });

            start = end - OVERLAP.min(end);
        }

        chunks
    }

    /// Search for relevant chunks
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResult>> {
        println!("\n🔍 Searching for: \"{}\"", query);

        // Generate query embedding
        let query_embedding = self.get_embedding(query).await?;

        // Calculate similarities
        let mut results: Vec<SearchResult> = self
            .chunks
            .iter()
            .map(|chunk| {
                let similarity = cosine_similarity(&query_embedding, &chunk.embedding);
                SearchResult {
                    chunk: chunk.clone(),
                    similarity,
                }
            })
            .collect();

        // Sort by similarity (highest first)
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());

        // Return top K
        Ok(results.into_iter().take(top_k).collect())
    }

    /// Generate answer based on retrieved context
    async fn generate_answer(&self, query: &str, results: &[SearchResult]) -> Result<String> {
        if results.is_empty() {
            return Ok("I couldn't find any relevant information in the knowledge base.".to_string());
        }

        // Combine context from top results
        let context: String = results
            .iter()
            .map(|r| r.chunk.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        // Use Ollama to generate answer
        let prompt = format!(
            "Based on the following context, answer the question. Only use information from the context.\n\nContext:\n{}\n\nQuestion: {}\n\nAnswer:",
            context, query
        );

        #[derive(Serialize)]
        struct GenerateRequest {
            model: String,
            prompt: String,
            stream: bool,
        }

        #[derive(Deserialize)]
        struct GenerateResponse {
            response: String,
        }

        let request = GenerateRequest {
            model: "llama3.2:1b".to_string(), // Small, fast model
            prompt,
            stream: false,
        };

        let response = self
            .client
            .post("http://localhost:11434/api/generate")
            .json(&request)
            .send()
            .await?;

        let generate_response: GenerateResponse = response.json().await?;
        
        Ok(format!(
            "{}\n\n---\nSources: {} chunks (relevance: {:.2}%)",
            generate_response.response,
            results.len(),
            results[0].similarity * 100.0
        ))
    }

    /// Save the index to disk
    async fn save(&self, path: &Path) -> Result<()> {
        println!("\n💾 Saving index to: {:?}", path);
        
        let data = serde_json::json!({
            "documents": self.documents,
            "chunks": self.chunks,
        });

        tokio::fs::write(path, serde_json::to_string_pretty(&data)?).await?;
        println!("✅ Saved successfully!");
        
        Ok(())
    }

    /// Load the index from disk
    async fn load(path: &Path) -> Result<Self> {
        println!("📂 Loading index from: {:?}", path);

        let content = tokio::fs::read_to_string(path).await?;
        let data: serde_json::Value = serde_json::from_str(&content)?;

        let documents: Vec<Document> = serde_json::from_value(data["documents"].clone())?;
        let chunks: Vec<Chunk> = serde_json::from_value(data["chunks"].clone())?;

        let client = Client::new();

        println!("✅ Loaded {} documents and {} chunks", documents.len(), chunks.len());

        Ok(Self {
            client,
            chunks,
            documents,
        })
    }

    fn hash_content(&self, content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

// ============================================================================
// UTILITIES
// ============================================================================

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

// ============================================================================
// MAIN APPLICATION
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    println!("🤖 Simple RAG System with Ollama");
    println!("=================================\n");

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage:");
        println!("  cargo run -- ingest <folder_path>");
        println!("  cargo run -- query \"your question here\"");
        println!("\nExample:");
        println!("  cargo run -- ingest ./test_docs");
        println!("  cargo run -- query \"What is Rust?\"");
        return Ok(());
    }

    let command = &args[1];
    let index_path = PathBuf::from("./rag_index.json");

    match command.as_str() {
        "ingest" => {
            if args.len() < 3 {
                anyhow::bail!("Please provide folder path: cargo run -- ingest <folder>");
            }

            let folder = PathBuf::from(&args[2]);
            let mut rag = SimpleRAG::new().await?;
            rag.ingest_folder(&folder).await?;
            rag.save(&index_path).await?;
        }
        
        "query" => {
            if args.len() < 3 {
                anyhow::bail!("Please provide a query: cargo run -- query \"your question\"");
            }

            let query = &args[2];
            
            // Load existing index
            let rag = SimpleRAG::load(&index_path).await?;
            
            // Search
            let results = rag.search(query, 3).await?;
            
            // Generate answer
            let answer = rag.generate_answer(query, &results).await?;
            
            println!("\n📝 Answer:\n{}", answer);
        }
        
        _ => {
            anyhow::bail!("Unknown command: {}. Use 'ingest' or 'query'", command);
        }
    }

    Ok(())
}

