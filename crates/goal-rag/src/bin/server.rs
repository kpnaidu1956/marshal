//! RAG Server binary
//!
//! Run with: cargo run -p goal-rag --bin goal-rag-server
//! With config: cargo run -p goal-rag --bin goal-rag-server -- --config config.toml

use goal_rag::{config::RagConfig, server::RagServer};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "goal_rag=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!(
        r#"
╔═══════════════════════════════════════════════════════════╗
║                      Goal RAG System                      ║
║           Document Q&A with Source Citations              ║
╚═══════════════════════════════════════════════════════════╝
"#
    );

    // Load configuration from file or use defaults
    let config = load_config()?;

    tracing::info!("Configuration loaded");
    tracing::info!("  - Embedding model: {}", config.embeddings.model);
    tracing::info!("  - Embedding dimensions: {}", config.embeddings.dimensions);
    tracing::info!("  - LLM model: {}", config.llm.generate_model);
    tracing::info!("  - Chunk size: {}", config.chunking.chunk_size);

    // Check Ollama
    tracing::info!("Checking Ollama at {}...", config.llm.base_url);
    let client = reqwest::Client::new();
    match client.get(format!("{}/api/tags", config.llm.base_url)).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("Ollama is running");
        }
        _ => {
            tracing::warn!("Ollama not available at {}", config.llm.base_url);
            tracing::warn!("Please start Ollama:");
            tracing::warn!("  1. Install: brew install ollama");
            tracing::warn!("  2. Start: ollama serve");
            tracing::warn!("  3. Pull models: ollama pull nomic-embed-text && ollama pull llama3.2:3b");
        }
    }

    // Create and start server
    let server = RagServer::new(config).await?;

    println!("\nServer starting...");
    println!("  API: http://{}", server.address());
    println!("  Health: http://{}/health", server.address());
    println!("  API Info: http://{}/api/info", server.address());
    println!("\nEndpoints:");
    println!("  POST /api/ingest    - Upload documents");
    println!("  POST /api/query     - Ask questions");
    println!("  GET  /api/documents - List documents");
    println!("\nPress Ctrl+C to stop\n");

    server.start().await?;

    Ok(())
}

/// Load configuration from file or environment
fn load_config() -> anyhow::Result<RagConfig> {
    // Check command line args for --config
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    // Try config file locations in order:
    // 1. --config argument
    // 2. RAG_CONFIG environment variable
    // 3. ./config.toml (current directory)
    // 4. ./crates/goal-rag/config.toml (workspace root)
    // 5. Default values
    let paths_to_try: Vec<PathBuf> = vec![
        config_path.clone(),
        std::env::var("RAG_CONFIG").ok().map(PathBuf::from),
        Some(PathBuf::from("config.toml")),
        Some(PathBuf::from("crates/goal-rag/config.toml")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in &paths_to_try {
        if path.exists() {
            tracing::info!("Loading config from: {}", path.display());
            let content = std::fs::read_to_string(path)?;
            let config: RagConfig = toml::from_str(&content)?;
            return Ok(config);
        }
    }

    // No config file found, use defaults
    tracing::info!("No config file found, using defaults");
    tracing::info!("  Searched: {:?}", paths_to_try);
    tracing::info!("  Create config.toml or set RAG_CONFIG env var");
    Ok(RagConfig::default())
}
