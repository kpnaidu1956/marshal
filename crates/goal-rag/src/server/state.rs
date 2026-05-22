//! Application state for the RAG server

use dashmap::DashMap;
use parking_lot::RwLock;
#[cfg(not(feature = "postgres"))]
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::{BackendProvider, RagConfig};
use crate::server::middleware::ProductionControls;
use crate::error::{Error, Result};
use crate::generation::OllamaClient;
use crate::ingestion::ExternalParser;
use crate::learning::{AnswerCache, KnowledgeStore};
use crate::processing::{JobQueue, ProcessingWorker};
use crate::providers::{
    EmbeddingProvider, LlmProvider, VectorStoreProvider,
    local::LocalVectorStore,
    ollama::{OllamaEmbedder, OllamaLlm},
    openai::{OpenAiLlm, OpenAiEmbedder},
    anthropic::AnthropicLlm,
};
#[cfg(feature = "gcp")]
use crate::providers::gcp::{DocumentAiClient, GcsDocumentStore};
use crate::retrieval::VectorStore;
#[cfg(not(feature = "postgres"))]
use crate::storage::FileRegistryDb;
use crate::storage::{FileRegistryDbStats, SyncStatus};
#[cfg(feature = "postgres")]
use crate::storage::PgFileRegistry;
use crate::types::{Chunk, Document, FileRecord, FileRecordParams, FileRecordStatus, SkipReason};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    /// Configuration
    config: RagConfig,
    /// Production controls (rate limiting, circuit breaker, backpressure)
    production_controls: Arc<ProductionControls>,
    /// Vector store for chunks (provider abstraction)
    vector_store_provider: Arc<dyn VectorStoreProvider>,
    /// Legacy local vector store (only for Local backend, None for GCP)
    vector_store: Option<Arc<VectorStore>>,
    /// Embedding provider (Ollama or Vertex AI)
    embedding_provider: Arc<dyn EmbeddingProvider>,
    /// LLM provider (Ollama or Gemini)
    llm_provider: Arc<dyn LlmProvider>,
    /// Ollama client (legacy, for backwards compatibility)
    ollama: Arc<OllamaClient>,
    /// External parser for legacy formats
    external_parser: Arc<ExternalParser>,
    /// Job queue for async processing
    job_queue: Arc<JobQueue>,
    /// Knowledge store for learning
    knowledge_store: KnowledgeStore,
    /// Answer cache with document-based invalidation
    answer_cache: AnswerCache,
    /// Document registry (in-memory cache, backed by database)
    documents: DashMap<Uuid, Document>,
    /// Chunk metadata store (for Vertex AI lookups)
    chunks: DashMap<Uuid, Chunk>,
    /// File registry (in-memory cache for fast lookups)
    file_registry: DashMap<String, FileRecord>,
    /// Database for persistent storage
    #[cfg(feature = "postgres")]
    database: Arc<PgFileRegistry>,
    #[cfg(not(feature = "postgres"))]
    database: Arc<FileRegistryDb>,
    /// Ready state
    ready: RwLock<bool>,
    /// GCS document store (only for GCP backend)
    #[cfg(feature = "gcp")]
    document_store: Option<Arc<GcsDocumentStore>>,
    /// Document AI client for advanced PDF extraction (only for GCP backend)
    #[cfg(feature = "gcp")]
    document_ai: Option<Arc<DocumentAiClient>>,
    /// PostgreSQL pool for learning from database changes
    #[cfg(feature = "postgres")]
    pg_pool: Option<Arc<crate::postgres::PgPool>>,
    /// Analytics database for storing learned patterns
    analytics_db: Option<Arc<crate::analytics::AnalyticsDb>>,
    /// Entity embedding store for tasks, goals, comments, messages
    #[cfg(feature = "postgres")]
    entity_embedding_store: Option<Arc<crate::providers::entity_embeddings::EntityEmbeddingStore>>,
}

impl AppState {
    /// Create new application state
    pub async fn new(config: RagConfig) -> Result<Self> {
        tracing::info!("Initializing RAG application state (backend: {:?})...", config.backend);

        // Local vector store - only created for Local backend
        // GCP backend keeps this as None, Local backend assigns Some()
        #[allow(unused_assignments)]
        let mut local_vector_store: Option<Arc<VectorStore>> = None;

        // Initialize Ollama client (for local backend, also used as fallback)
        let ollama = Arc::new(OllamaClient::new(&config.llm));
        tracing::info!("Ollama client initialized (using {} for embeddings)", config.llm.embed_model);

        // Initialize document store (GCP only)
        #[cfg(feature = "gcp")]
        let mut gcs_document_store: Option<Arc<GcsDocumentStore>> = None;
        #[cfg(feature = "gcp")]
        let mut document_ai_client: Option<Arc<DocumentAiClient>> = None;

        let storage_dir = config.vector_db.storage_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        // Initialize primary database
        #[cfg(feature = "postgres")]
        let (database, pg_pool_shared) = {
            let pg_config = config.postgres.as_ref().ok_or_else(|| {
                Error::Config("postgres feature enabled but postgres config is missing".to_string())
            })?;
            let pool = Arc::new(crate::postgres::PgPool::new(pg_config.clone()).await.map_err(|e| {
                Error::Config(format!("Failed to create PG pool: {}", e))
            })?);
            let registry = Arc::new(PgFileRegistry::new(Arc::clone(&pool)).await?);
            tracing::info!("PostgreSQL file registry initialized");
            (registry, pool)
        };

        #[cfg(not(feature = "postgres"))]
        let database = {
            let db_path = storage_dir.join("rag_registry.db");
            let db = Arc::new(FileRegistryDb::new(&db_path)?);
            tracing::info!("Database initialized at {:?}", db_path);
            db
        };

        // Initialize providers based on backend
        let (embedding_provider, llm_provider, vector_store_provider): (
            Arc<dyn EmbeddingProvider>,
            Arc<dyn LlmProvider>,
            Arc<dyn VectorStoreProvider>,
        ) = match config.backend {
            BackendProvider::Local => {
                // Select LLM provider based on config or LLM_PROVIDER env var
                let llm_provider_name = std::env::var("LLM_PROVIDER")
                    .unwrap_or_else(|_| config.llm.provider.clone());

                let (embedder, llm): (Arc<dyn EmbeddingProvider>, Arc<dyn LlmProvider>) =
                    match llm_provider_name.as_str() {
                        "openai" => {
                            let llm = Arc::new(OpenAiLlm::from_env()?);
                            // Use OpenAI embeddings too unless EMBED_PROVIDER says otherwise
                            let embed_provider = std::env::var("EMBED_PROVIDER")
                                .unwrap_or_else(|_| "openai".into());
                            let embedder: Arc<dyn EmbeddingProvider> = if embed_provider == "ollama" {
                                Arc::new(OllamaEmbedder::new(&config.llm, config.embeddings.dimensions))
                            } else {
                                Arc::new(OpenAiEmbedder::from_env()?)
                            };
                            (embedder, llm)
                        }
                        "anthropic" => {
                            let llm = Arc::new(AnthropicLlm::from_env()?);
                            // Anthropic has no embeddings API — use OpenAI or Ollama
                            let embed_provider = std::env::var("EMBED_PROVIDER")
                                .unwrap_or_else(|_| "ollama".into());
                            let embedder: Arc<dyn EmbeddingProvider> = if embed_provider == "openai" {
                                Arc::new(OpenAiEmbedder::from_env()?)
                            } else {
                                Arc::new(OllamaEmbedder::new(&config.llm, config.embeddings.dimensions))
                            };
                            (embedder, llm)
                        }
                        _ => {
                            // Default: Ollama for both
                            let embedder = Arc::new(OllamaEmbedder::new(
                                &config.llm,
                                config.embeddings.dimensions,
                            ));
                            let llm = Arc::new(OllamaLlm::new(&config.llm));
                            (embedder, llm)
                        }
                    };

                tracing::info!(
                    "LLM provider: {} ({}), Embedding provider: {}",
                    llm.name(),
                    llm.model(),
                    embedder.name()
                );

                // Check if pgvector is enabled via postgres config
                #[cfg(feature = "postgres")]
                let use_pgvector = config.postgres.as_ref().map(|p| p.use_pgvector).unwrap_or(false);
                #[cfg(not(feature = "postgres"))]
                let use_pgvector = false;

                #[cfg(feature = "postgres")]
                if use_pgvector {
                    // Use pgvector for vector storage (instant startup, no rebuild)
                    let pg_pool = Arc::clone(&pg_pool_shared);

                    let vector_provider: Arc<dyn VectorStoreProvider> = Arc::new(
                        crate::providers::pgvector::PgVectorStore::new(
                            Arc::clone(&pg_pool),
                            config.embeddings.dimensions,
                        ).await?
                    );

                    tracing::info!(
                        "Using local backend with pgvector ({} embeddings + PostgreSQL vectors, dimensions: {})",
                        embedder.name(),
                        config.embeddings.dimensions
                    );

                    return Ok(Self {
                        inner: Arc::new(AppStateInner {
                            config: config.clone(),
                            production_controls: Arc::new(ProductionControls::from_config(&config.server.rate_limit)),
                            vector_store_provider: vector_provider,
                            vector_store: None, // No local HNSW store when using pgvector
                            embedding_provider: embedder,
                            llm_provider: llm,
                            ollama,
                            external_parser: Arc::new(ExternalParser::new(config.external_parser.clone())),
                            job_queue: Arc::new(JobQueue::new(num_cpus::get().min(4), database.clone()).0),
                            knowledge_store: KnowledgeStore::new(storage_dir.join("knowledge.json")),
                            answer_cache: AnswerCache::new(1000, 3600),
                            documents: DashMap::new(),
                            chunks: DashMap::new(),
                            file_registry: DashMap::new(),
                            database,
                            ready: RwLock::new(true),
                            #[cfg(feature = "gcp")]
                            document_store: None,
                            #[cfg(feature = "gcp")]
                            document_ai: None,
                            #[cfg(feature = "postgres")]
                            pg_pool: Some(pg_pool),
                            analytics_db: None,
                            #[cfg(feature = "postgres")]
                            entity_embedding_store: None,
                        }),
                    });
                }

                // Default: Use local HNSW vector store (requires index rebuild on startup)
                tracing::info!("Using local backend (Ollama + HNSW)");

                let vector_store = Arc::new(VectorStore::new(&config)?);
                tracing::info!("Local vector store initialized");

                // Pass database for FTS-based string search
                #[cfg(feature = "postgres")]
                let vector_provider = Arc::new(LocalVectorStore::new(
                    Arc::clone(&vector_store),
                    Arc::clone(&database),
                ));
                #[cfg(not(feature = "postgres"))]
                let vector_provider = Arc::new(LocalVectorStore::new(
                    Arc::clone(&vector_store),
                    Arc::clone(&database),
                ));

                // Store for later use
                local_vector_store = Some(vector_store);

                (embedder, llm, vector_provider)
            }
            BackendProvider::Gcp => {
                #[cfg(feature = "gcp")]
                {
                    use crate::config::HybridMode;
                    use crate::providers::gcp::{GcpAuth, GeminiClient, VertexAiEmbedder, VertexVectorSearch};

                    let gcp_config = config.gcp.as_ref().ok_or_else(|| {
                        Error::Config("GCP backend selected but gcp config is missing".to_string())
                    })?;

                    let auth = Arc::new(GcpAuth::from_service_account(
                        &gcp_config.service_account_key_path,
                        gcp_config.project_id.clone(),
                    )?);

                    // Get the effective hybrid mode
                    let hybrid_mode = gcp_config.effective_hybrid_mode();

                    // Choose embedding provider based on hybrid mode
                    let embedder: Arc<dyn EmbeddingProvider> = match hybrid_mode {
                        HybridMode::FullGcp => {
                            tracing::info!("Using FULL GCP backend: Vertex AI embeddings + Vertex AI vectors + Gemini LLM");
                            Arc::new(VertexAiEmbedder::new(
                                Arc::clone(&auth),
                                gcp_config.location.clone(),
                                Some(gcp_config.embedding_model.clone()),
                            ))
                        }
                        HybridMode::HybridVertex => {
                            tracing::info!("Using HYBRID VERTEX mode: Ollama embeddings + Vertex AI vectors + Gemini LLM");
                            Arc::new(OllamaEmbedder::new(
                                &config.llm,
                                config.embeddings.dimensions,
                            ))
                        }
                        HybridMode::HybridLocal => {
                            tracing::info!("Using HYBRID LOCAL mode: Ollama embeddings + Local HNSW + GCS storage + Gemini LLM");
                            Arc::new(OllamaEmbedder::new(
                                &config.llm,
                                config.embeddings.dimensions,
                            ))
                        }
                    };

                    // Gemini LLM for all GCP modes
                    let llm = Arc::new(GeminiClient::new(
                        Arc::clone(&auth),
                        gcp_config.location.clone(),
                        Some(gcp_config.generation_model.clone()),
                    ));

                    // Check if pgvector is enabled via postgres config
                    #[cfg(feature = "postgres")]
                    let use_pgvector = config.postgres.as_ref().map(|p| p.use_pgvector).unwrap_or(false);
                    #[cfg(not(feature = "postgres"))]
                    let use_pgvector = false;

                    // Choose vector store based on hybrid mode
                    let vector_provider: Arc<dyn VectorStoreProvider> = match hybrid_mode {
                        HybridMode::FullGcp | HybridMode::HybridVertex => {
                            // Use Vertex AI Vector Search
                            let db_for_vertex = Arc::clone(&database);
                            let provider = Arc::new(VertexVectorSearch::new(
                                Arc::clone(&auth),
                                gcp_config.location.clone(),
                                gcp_config.vector_search_index.clone(),
                                gcp_config.vector_search_endpoint.clone(),
                                gcp_config.vector_search_public_domain.clone(),
                                gcp_config.deployed_index_id.clone(),
                                db_for_vertex,
                            ));
                            tracing::info!("Vector store: Vertex AI Vector Search");
                            provider
                        }
                        HybridMode::HybridLocal => {
                            // Check if pgvector is enabled for instant startup
                            #[cfg(feature = "postgres")]
                            if use_pgvector {
                                let pg_pool = Arc::clone(&pg_pool_shared);

                                let provider: Arc<dyn VectorStoreProvider> = Arc::new(
                                    crate::providers::pgvector::PgVectorStore::new(
                                        pg_pool,
                                        config.embeddings.dimensions,
                                    ).await?
                                );
                                tracing::info!("Vector store: pgvector (instant startup, no HNSW rebuild)");
                                provider
                            } else {
                                // Use local HNSW vector store (no Vertex AI rate limits!)
                                let vector_store = Arc::new(VectorStore::new(&config)?);
                                local_vector_store = Some(Arc::clone(&vector_store));
                                let provider: Arc<dyn VectorStoreProvider> = Arc::new(LocalVectorStore::new(
                                    vector_store,
                                    Arc::clone(&database),
                                ));
                                tracing::info!("Vector store: Local HNSW (no Vertex AI rate limits)");
                                provider
                            }

                            #[cfg(not(feature = "postgres"))]
                            {
                                // Use local HNSW vector store (no Vertex AI rate limits!)
                                let vector_store = Arc::new(VectorStore::new(&config)?);
                                local_vector_store = Some(Arc::clone(&vector_store));
                                let provider: Arc<dyn VectorStoreProvider> = Arc::new(LocalVectorStore::new(
                                    vector_store,
                                    Arc::clone(&database),
                                ));
                                tracing::info!("Vector store: Local HNSW (no Vertex AI rate limits)");
                                provider
                            }
                        }
                    };

                    // Initialize GCS document store (used in all GCP modes for document backup)
                    let document_store = GcsDocumentStore::new(
                        Arc::clone(&auth),
                        gcp_config.gcs_bucket.clone(),
                        Some(gcp_config.gcs_originals_prefix.clone()),
                        Some(gcp_config.gcs_plaintext_prefix.clone()),
                    ).await?;
                    gcs_document_store = Some(Arc::new(document_store));

                    // Initialize Document AI client if processor is configured
                    if let Some(ref processor_name) = gcp_config.document_ai_processor {
                        if gcp_config.use_document_ai_fallback {
                            let doc_ai = DocumentAiClient::new(
                                Arc::clone(&auth),
                                processor_name.clone(),
                            );
                            document_ai_client = Some(Arc::new(doc_ai));
                            tracing::info!(
                                "Document AI initialized (processor: {})",
                                processor_name
                            );
                        }
                    }

                    let embedding_source = match hybrid_mode {
                        HybridMode::FullGcp => gcp_config.embedding_model.clone(),
                        HybridMode::HybridVertex | HybridMode::HybridLocal => {
                            format!("ollama/{}", config.llm.embed_model)
                        }
                    };
                    let vector_source = match hybrid_mode {
                        HybridMode::FullGcp | HybridMode::HybridVertex => "vertex_ai".to_string(),
                        HybridMode::HybridLocal => {
                            if use_pgvector { "pgvector".to_string() } else { "local_hnsw".to_string() }
                        }
                    };
                    tracing::info!(
                        "GCP providers initialized (mode: {:?}, embedding: {}, vectors: {}, llm: {}, gcs: {}, document_ai: {})",
                        hybrid_mode,
                        embedding_source,
                        vector_source,
                        gcp_config.generation_model,
                        gcp_config.gcs_bucket,
                        if document_ai_client.is_some() { "enabled" } else { "disabled" }
                    );

                    (embedder, llm, vector_provider)
                }
                #[cfg(not(feature = "gcp"))]
                {
                    return Err(Error::Config(
                        "GCP backend selected but gcp feature is not enabled. \
                         Rebuild with --features gcp".to_string()
                    ));
                }
            }
        };

        // Initialize external parser for legacy formats
        let external_parser = Arc::new(ExternalParser::new(config.external_parser.clone()));
        tracing::info!("External parser initialized (enabled: {})", config.external_parser.enabled);

        // Initialize knowledge store for learning
        let knowledge_path = storage_dir.join("knowledge.json");
        let knowledge_store = KnowledgeStore::new(knowledge_path);
        tracing::info!("Knowledge store initialized");

        // Initialize answer cache (1000 entries, 1 hour TTL)
        let answer_cache = AnswerCache::new(1000, 3600);
        tracing::info!("Answer cache initialized");

        // Load file registry from database into memory cache
        let file_registry = DashMap::new();
        #[cfg(feature = "postgres")]
        {
            match database.list_file_records().await {
                Ok(records) => {
                    for record in records {
                        file_registry.insert(record.filename.clone(), record);
                    }
                    tracing::info!("Loaded {} file records from PostgreSQL", file_registry.len());
                }
                Err(e) => {
                    tracing::warn!("Failed to load file registry from PostgreSQL: {}", e);
                }
            }
        }
        #[cfg(not(feature = "postgres"))]
        {
            match database.list_file_records() {
                Ok(records) => {
                    for record in records {
                        file_registry.insert(record.filename.clone(), record);
                    }
                    tracing::info!("Loaded {} file records from database", file_registry.len());
                }
                Err(e) => {
                    tracing::warn!("Failed to load file registry from database: {}", e);
                }
            }
        }

        // Load documents from database
        #[cfg(feature = "postgres")]
        let documents = Self::load_documents_pg(&database).await;
        #[cfg(not(feature = "postgres"))]
        let documents = Self::load_documents(&database, &storage_dir.join("documents.json"));
        tracing::info!("Loaded {} documents from registry", documents.len());

        // Initialize job queue and start workers
        let worker_count = num_cpus::get().min(4);  // Max 4 workers
        let (job_queue, receiver) = JobQueue::new(worker_count, database.clone());
        let job_queue = Arc::new(job_queue);
        tracing::info!("Job queue initialized with {} workers", worker_count);

        // Check for incomplete jobs from previous session
        #[cfg(feature = "postgres")]
        let incomplete_jobs = job_queue.get_incomplete_jobs().await;
        #[cfg(not(feature = "postgres"))]
        let incomplete_jobs = job_queue.get_incomplete_jobs();
        if !incomplete_jobs.is_empty() {
            tracing::info!(
                "Found {} incomplete jobs from previous session - will resume after startup",
                incomplete_jobs.len()
            );
        }

        // Initialize production controls (rate limiting, circuit breaker, backpressure)
        let production_controls = Arc::new(ProductionControls::from_config(&config.server.rate_limit));
        tracing::info!(
            "Production controls initialized (enabled: {}, queue_limit: {})",
            config.server.rate_limit.enabled,
            config.server.rate_limit.max_queue_depth
        );

        // Initialize analytics database
        let analytics_db_path = storage_dir.join("analytics.db");
        let analytics_db = match crate::analytics::AnalyticsDb::new(&analytics_db_path) {
            Ok(db) => {
                tracing::info!("Analytics database initialized at {:?}", analytics_db_path);
                Some(Arc::new(db))
            }
            Err(e) => {
                tracing::warn!("Failed to initialize analytics database: {}", e);
                None
            }
        };

        // Reuse shared PostgreSQL pool for learning and analytics
        #[cfg(feature = "postgres")]
        let pg_pool: Option<Arc<crate::postgres::PgPool>> = Some(Arc::clone(&pg_pool_shared));

        // Initialize entity embedding store (if postgres is available)
        #[cfg(feature = "postgres")]
        let entity_embedding_store: Option<Arc<crate::providers::entity_embeddings::EntityEmbeddingStore>> = if let Some(ref pool) = pg_pool {
            let embed_dims = embedding_provider.dimensions();
            match crate::providers::entity_embeddings::EntityEmbeddingStore::new(
                Arc::clone(pool),
                Arc::clone(&embedding_provider),
                embed_dims,
            ).await {
                Ok(store) => {
                    tracing::info!("Entity embedding store initialized (dimensions: {})", embed_dims);
                    Some(Arc::new(store))
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize entity embedding store: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Create the state first (without the worker running)
        let state = Self {
            inner: Arc::new(AppStateInner {
                config,
                production_controls,
                vector_store_provider,
                vector_store: local_vector_store,
                embedding_provider,
                llm_provider,
                ollama,
                external_parser,
                job_queue: job_queue.clone(),
                knowledge_store,
                answer_cache,
                documents,
                chunks: DashMap::new(),
                file_registry,
                database,
                ready: RwLock::new(true),
                #[cfg(feature = "gcp")]
                document_store: gcs_document_store,
                #[cfg(feature = "gcp")]
                document_ai: document_ai_client,
                #[cfg(feature = "postgres")]
                pg_pool: pg_pool.clone(),
                analytics_db: analytics_db.clone(),
                #[cfg(feature = "postgres")]
                entity_embedding_store: entity_embedding_store.clone(),
            }),
        };

        // Start PostgreSQL change listener if configured
        #[cfg(feature = "postgres")]
        if let (Some(ref pool), Some(ref analytics)) = (&pg_pool, &analytics_db) {
            let pg_config = match state.inner.config.postgres.clone() {
                Some(cfg) => cfg,
                None => {
                    tracing::error!("PostgreSQL config disappeared between pool init and learner start");
                    return Ok(state);
                }
            };
            if pg_config.learning_enabled {
                let pool_clone = (**pool).clone();
                let analytics_clone = Arc::clone(analytics);
                let batch_size = pg_config.learning_batch_size;

                let learner = Arc::new(crate::postgres::DatabaseLearner::new(
                    pool_clone.clone(),
                    analytics_clone,
                    batch_size,
                    entity_embedding_store.clone(),
                ));

                let (tx, rx) = tokio::sync::mpsc::channel(1000);
                let listener = crate::postgres::ChangeListener::new(pool_clone);

                // Start the listener in background
                tokio::spawn(async move {
                    if let Err(e) = listener.start(tx).await {
                        tracing::error!("PostgreSQL listener error: {}", e);
                    }
                });

                // Start the learner in background
                tokio::spawn(async move {
                    if let Err(e) = learner.start(rx).await {
                        tracing::error!("Database learner error: {}", e);
                    }
                });

                tracing::info!("PostgreSQL change listener and learner started");
            }
        }

        // Start background worker with a clone of the state
        let worker_state = state.clone();
        let worker = ProcessingWorker::new(worker_state, job_queue.clone());
        tokio::spawn(async move {
            worker.run(receiver).await;
        });

        // Resume incomplete jobs from previous session
        if !incomplete_jobs.is_empty() {
            let resume_queue = job_queue.clone();
            tokio::spawn(async move {
                // Wait a bit for the worker to start
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                for job_record in incomplete_jobs {
                    let job_id = job_record.id;
                    tracing::info!(
                        "Resuming job {} ({} files processed, {} pending)",
                        job_id,
                        job_record.files_processed,
                        job_record.total_files - job_record.files_processed
                    );
                    match resume_queue.resume_job(job_record).await {
                        Some(id) => {
                            tracing::info!("Successfully resumed job {}", id);
                        }
                        None => {
                            tracing::warn!("Failed to resume job {} (no pending files with data)", job_id);
                        }
                    }
                }
            });
        }

        Ok(state)
    }

    /// Load documents from PostgreSQL
    #[cfg(feature = "postgres")]
    async fn load_documents_pg(database: &Arc<PgFileRegistry>) -> DashMap<Uuid, Document> {
        let documents = DashMap::new();
        match database.list_documents().await {
            Ok(docs) if !docs.is_empty() => {
                tracing::info!("Loaded {} documents from PostgreSQL", docs.len());
                for doc in docs {
                    documents.insert(doc.id, doc);
                }
            }
            Ok(_) => {
                tracing::debug!("No documents in PostgreSQL");
            }
            Err(e) => {
                tracing::warn!("Failed to load documents from PostgreSQL: {}", e);
            }
        }
        documents
    }

    /// Load documents from SQLite, with migration from JSON if needed
    #[cfg(not(feature = "postgres"))]
    fn load_documents(database: &Arc<FileRegistryDb>, json_path: &PathBuf) -> DashMap<Uuid, Document> {
        let documents = DashMap::new();

        // First, try to load from SQLite
        match database.list_documents() {
            Ok(docs) if !docs.is_empty() => {
                tracing::info!("Loaded {} documents from SQLite database", docs.len());
                for doc in docs {
                    documents.insert(doc.id, doc);
                }
                return documents;
            }
            Ok(_) => {
                tracing::debug!("No documents in SQLite, checking JSON for migration");
            }
            Err(e) => {
                tracing::warn!("Failed to load documents from SQLite: {}", e);
            }
        }

        // If SQLite is empty, try to migrate from JSON
        if json_path.exists() {
            match fs::read_to_string(json_path) {
                Ok(content) => {
                    match serde_json::from_str::<Vec<Document>>(&content) {
                        Ok(docs) => {
                            if !docs.is_empty() {
                                tracing::info!("Migrating {} documents from JSON to SQLite", docs.len());
                                let mut migrated = 0;
                                for doc in docs {
                                    // Insert into SQLite
                                    if let Err(e) = database.upsert_document(&doc) {
                                        tracing::warn!("Failed to migrate document {}: {}", doc.filename, e);
                                    } else {
                                        migrated += 1;
                                    }
                                    documents.insert(doc.id, doc);
                                }
                                tracing::info!("Successfully migrated {} documents to SQLite", migrated);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse documents.json: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read documents.json: {}", e);
                }
            }
        }

        documents
    }

    /// Save a single document to database (and in-memory cache)
    fn save_document(&self, doc: &Document) {
        #[cfg(feature = "postgres")]
        {
            let db = self.inner.database.clone();
            let doc = doc.clone();
            tokio::spawn(async move {
                if let Err(e) = db.upsert_document(&doc).await {
                    tracing::error!("Failed to save document {} to PG: {}", doc.filename, e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        {
            if let Err(e) = self.inner.database.upsert_document(doc) {
                tracing::error!("Failed to save document {} to SQLite: {}", doc.filename, e);
            }
        }
    }

    /// Delete a document from database
    fn delete_document_from_db(&self, id: &Uuid) {
        #[cfg(feature = "postgres")]
        {
            let db = self.inner.database.clone();
            let id = *id;
            tokio::spawn(async move {
                if let Err(e) = db.delete_document(&id).await {
                    tracing::error!("Failed to delete document {} from PG: {}", id, e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        {
            if let Err(e) = self.inner.database.delete_document(id) {
                tracing::error!("Failed to delete document {} from SQLite: {}", id, e);
            }
        }
    }

    /// Get database reference
    #[cfg(feature = "postgres")]
    pub fn database(&self) -> &Arc<PgFileRegistry> {
        &self.inner.database
    }

    /// Get database reference
    #[cfg(not(feature = "postgres"))]
    pub fn database(&self) -> &Arc<FileRegistryDb> {
        &self.inner.database
    }

    /// Get analytics database reference (if initialized)
    pub fn analytics_db(&self) -> Option<&Arc<crate::analytics::AnalyticsDb>> {
        self.inner.analytics_db.as_ref()
    }

    /// Get PostgreSQL pool reference (if initialized)
    #[cfg(feature = "postgres")]
    pub fn pg_pool(&self) -> Option<&Arc<crate::postgres::PgPool>> {
        self.inner.pg_pool.as_ref()
    }

    /// Get entity embedding store reference (if initialized)
    #[cfg(feature = "postgres")]
    pub fn entity_embedding_store(&self) -> Option<&Arc<crate::providers::entity_embeddings::EntityEmbeddingStore>> {
        self.inner.entity_embedding_store.as_ref()
    }

    /// Sync file registry from GCS bucket
    /// Returns (files_synced, failed_count)
    #[cfg(feature = "gcp")]
    pub async fn sync_from_gcs(&self) -> Result<(usize, usize)> {
        let document_store = self.document_store()
            .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

        let start = std::time::Instant::now();
        let files = document_store.sync_from_bucket().await?;

        let mut synced = 0;
        let mut failed = 0;

        for file_info in &files {
            // Update database
            #[cfg(feature = "postgres")]
            let sync_result = self.inner.database.sync_from_gcs(
                &file_info.filename,
                file_info.document_id,
                file_info.content_hash.as_deref().unwrap_or(""),
                file_info.file_size,
                &file_info.file_type,
                file_info.has_plaintext,
                &file_info.original_uri,
                file_info.plaintext_uri.as_deref(),
            ).await;
            #[cfg(not(feature = "postgres"))]
            let sync_result = self.inner.database.sync_from_gcs(
                &file_info.filename,
                file_info.document_id,
                file_info.content_hash.as_deref().unwrap_or(""),
                file_info.file_size,
                &file_info.file_type,
                file_info.has_plaintext,
                &file_info.original_uri,
                file_info.plaintext_uri.as_deref(),
            );
            if let Err(e) = sync_result {
                tracing::warn!("Failed to sync file {}: {}", file_info.filename, e);
                failed += 1;
                continue;
            }

            // Update in-memory cache
            let status = if file_info.has_plaintext {
                FileRecordStatus::Success
            } else {
                FileRecordStatus::Failed
            };

            // Extract organization_id from the GCS path (format: originals/{org_id}/{doc_id}.{ext})
            let org_id = file_info.original_uri
                .split('/')
                .skip(3) // Skip gs://, bucket, prefix
                .next()
                .filter(|s| !s.contains('.'))  // Skip if it looks like a filename (has extension)
                .unwrap_or("unknown")
                .to_string();

            let record = FileRecord {
                id: file_info.document_id,
                organization_id: org_id,
                filename: file_info.filename.clone(),
                content_hash: file_info.content_hash.clone().unwrap_or_default(),
                file_size: file_info.file_size,
                file_type: crate::types::FileType::from_extension(&file_info.file_type),
                status,
                document_id: Some(file_info.document_id),
                chunks_created: None,
                skip_reason: None,
                error_message: if file_info.has_plaintext { None } else {
                    Some("No plaintext found - processing may have failed".to_string())
                },
                failed_at_stage: None,
                job_id: None,
                first_seen_at: chrono::Utc::now(),
                last_processed_at: chrono::Utc::now(),
                upload_count: 1,
                original_url: Some(file_info.original_uri.clone()),
                plaintext_url: file_info.plaintext_uri.clone(),
            };

            self.inner.file_registry.insert(file_info.filename.clone(), record);
            synced += 1;
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        #[cfg(feature = "postgres")]
        if let Err(e) = self.inner.database.update_sync_status(synced, duration_ms).await {
            tracing::warn!("Failed to update sync status: {}", e);
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.inner.database.update_sync_status(synced, duration_ms) {
            tracing::warn!("Failed to update sync status: {}", e);
        }

        tracing::info!(
            "GCS sync complete: {} files synced, {} failed, took {}ms",
            synced, failed, duration_ms
        );

        Ok((synced, failed))
    }

    /// Sync file registry from GCS bucket for a specific organization
    /// Returns (files_synced, failed_count)
    #[cfg(feature = "gcp")]
    pub async fn sync_from_gcs_for_org(&self, organization_id: &str) -> Result<(usize, usize)> {
        let document_store = self.document_store()
            .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

        let start = std::time::Instant::now();
        // Use org-specific sync if available, otherwise filter manually
        let files = document_store.sync_from_bucket_for_org(organization_id).await?;

        let mut synced = 0;
        let mut failed = 0;

        for file_info in &files {
            // Update database
            #[cfg(feature = "postgres")]
            let sync_result = self.inner.database.sync_from_gcs(
                &file_info.filename,
                file_info.document_id,
                file_info.content_hash.as_deref().unwrap_or(""),
                file_info.file_size,
                &file_info.file_type,
                file_info.has_plaintext,
                &file_info.original_uri,
                file_info.plaintext_uri.as_deref(),
            ).await;
            #[cfg(not(feature = "postgres"))]
            let sync_result = self.inner.database.sync_from_gcs(
                &file_info.filename,
                file_info.document_id,
                file_info.content_hash.as_deref().unwrap_or(""),
                file_info.file_size,
                &file_info.file_type,
                file_info.has_plaintext,
                &file_info.original_uri,
                file_info.plaintext_uri.as_deref(),
            );
            if let Err(e) = sync_result {
                tracing::warn!("Failed to sync file {}: {}", file_info.filename, e);
                failed += 1;
                continue;
            }

            // Update in-memory cache
            let status = if file_info.has_plaintext {
                FileRecordStatus::Success
            } else {
                FileRecordStatus::Failed
            };

            let record = FileRecord {
                id: file_info.document_id,
                organization_id: organization_id.to_string(),
                filename: file_info.filename.clone(),
                content_hash: file_info.content_hash.clone().unwrap_or_default(),
                file_size: file_info.file_size,
                file_type: crate::types::FileType::from_extension(&file_info.file_type),
                status,
                document_id: Some(file_info.document_id),
                chunks_created: None,
                skip_reason: None,
                error_message: if file_info.has_plaintext { None } else {
                    Some("No plaintext found - processing may have failed".to_string())
                },
                failed_at_stage: None,
                job_id: None,
                first_seen_at: chrono::Utc::now(),
                last_processed_at: chrono::Utc::now(),
                upload_count: 1,
                original_url: Some(file_info.original_uri.clone()),
                plaintext_url: file_info.plaintext_uri.clone(),
            };

            self.inner.file_registry.insert(file_info.filename.clone(), record);
            synced += 1;
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        #[cfg(feature = "postgres")]
        if let Err(e) = self.inner.database.update_sync_status(synced, duration_ms).await {
            tracing::warn!("Failed to update sync status: {}", e);
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.inner.database.update_sync_status(synced, duration_ms) {
            tracing::warn!("Failed to update sync status: {}", e);
        }

        tracing::info!(
            "GCS sync for org '{}' complete: {} files synced, {} failed, took {}ms",
            organization_id, synced, failed, duration_ms
        );

        Ok((synced, failed))
    }

    /// Get GCS sync status
    pub async fn get_sync_status(&self) -> Option<SyncStatus> {
        #[cfg(feature = "postgres")]
        {
            self.inner.database.get_sync_status().await.ok().flatten()
        }
        #[cfg(not(feature = "postgres"))]
        {
            self.inner.database.get_sync_status().ok().flatten()
        }
    }

    /// Get external parser
    pub fn external_parser(&self) -> &ExternalParser {
        &self.inner.external_parser
    }

    /// Get job queue
    pub fn job_queue(&self) -> &Arc<JobQueue> {
        &self.inner.job_queue
    }

    /// Get knowledge store
    pub fn knowledge_store(&self) -> &KnowledgeStore {
        &self.inner.knowledge_store
    }

    /// Get answer cache
    pub fn answer_cache(&self) -> &AnswerCache {
        &self.inner.answer_cache
    }

    /// Get document timestamps for cache validation
    pub fn get_document_timestamps(&self) -> std::collections::HashMap<Uuid, chrono::DateTime<chrono::Utc>> {
        self.inner
            .documents
            .iter()
            .map(|entry| (*entry.key(), entry.value().ingested_at))
            .collect()
    }

    /// Get configuration
    pub fn config(&self) -> &RagConfig {
        &self.inner.config
    }

    /// Get production controls (rate limiting, circuit breaker, backpressure)
    pub fn production_controls(&self) -> &Arc<ProductionControls> {
        &self.inner.production_controls
    }

    /// Get vector store (only available for Local backend)
    /// Prefer using vector_store_provider() for new code
    pub fn vector_store(&self) -> Option<&Arc<VectorStore>> {
        self.inner.vector_store.as_ref()
    }

    /// Get Ollama client (for embeddings and generation)
    /// NOTE: Prefer using embedding_provider() and llm_provider() for new code
    pub fn ollama(&self) -> &Arc<OllamaClient> {
        &self.inner.ollama
    }

    /// Get embedding provider (Ollama or Vertex AI based on config)
    pub fn embedding_provider(&self) -> &Arc<dyn EmbeddingProvider> {
        &self.inner.embedding_provider
    }

    /// Get LLM provider (Ollama or Gemini based on config)
    pub fn llm_provider(&self) -> &Arc<dyn LlmProvider> {
        &self.inner.llm_provider
    }

    /// Get vector store provider (Local HNSW or Vertex AI Vector Search)
    pub fn vector_store_provider(&self) -> &Arc<dyn VectorStoreProvider> {
        &self.inner.vector_store_provider
    }

    /// Get GCS document store (only available with GCP backend)
    #[cfg(feature = "gcp")]
    pub fn document_store(&self) -> Option<&Arc<GcsDocumentStore>> {
        self.inner.document_store.as_ref()
    }

    /// Get Document AI client (only available with GCP backend and processor configured)
    #[cfg(feature = "gcp")]
    pub fn document_ai(&self) -> Option<&Arc<DocumentAiClient>> {
        self.inner.document_ai.as_ref()
    }

    /// Extract text content from file bytes using the external parser
    ///
    /// This method uses the escalation parsing strategy for PDFs and
    /// appropriate parsers for other file types.
    #[cfg(feature = "gcp")]
    pub async fn extract_text_from_bytes(&self, filename: &str, data: &[u8]) -> Result<String> {
        let external_parser = &self.inner.external_parser;
        let characteristics = external_parser.analyze_file(filename, data);

        let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();

        // For PDFs, use escalation parsing
        if ext == "pdf" {
            match external_parser.parse_with_full_escalation(filename, data, &characteristics).await {
                Ok(result) => {
                    tracing::info!(
                        "[{}] Escalation succeeded with '{}': {} chars",
                        filename, result.method, result.content.len()
                    );
                    return Ok(result.content);
                }
                Err(e) => {
                    tracing::error!("[{}] Escalation parsing failed: {}", filename, e);
                    // Try Document AI as last resort if available
                    if let Some(doc_ai) = self.document_ai() {
                        tracing::info!("[{}] Trying Document AI as final fallback...", filename);
                        match doc_ai.process_pdf(data, filename).await {
                            Ok(result) => {
                                tracing::info!(
                                    "[{}] Document AI succeeded: {} chars",
                                    filename, result.text.len()
                                );
                                return Ok(result.text);
                            }
                            Err(doc_ai_err) => {
                                tracing::error!("[{}] Document AI failed: {}", filename, doc_ai_err);
                            }
                        }
                    }
                    return Err(e);
                }
            }
        }

        // For legacy formats, convert first
        if ExternalParser::needs_conversion(filename) {
            match external_parser.convert_with_libreoffice(filename, data).await {
                Ok(converted) => {
                    // Parse the converted content directly (avoid recursive call)
                    let stem = std::path::Path::new(filename)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("document");
                    let new_ext = match ext.as_str() {
                        "doc" => "docx",
                        "ppt" => "pptx",
                        "xls" => "xlsx",
                        _ => "docx",
                    };
                    let new_filename = format!("{}.{}", stem, new_ext);
                    // Parse the converted file directly
                    match crate::ingestion::FileParser::parse(&new_filename, &converted) {
                        Ok(parsed) => return Ok(parsed.content),
                        Err(e) => {
                            tracing::warn!("[{}] Parsing converted file failed: {}", filename, e);
                            // Fall through to try other methods
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("[{}] LibreOffice conversion failed: {}", filename, e);
                    // Fall through to try native parsing
                }
            }
        }

        // For other formats, try native parsing
        match crate::ingestion::FileParser::parse(filename, data) {
            Ok(parsed) => Ok(parsed.content),
            Err(e) => {
                tracing::warn!("[{}] Native parsing failed: {}", filename, e);
                // Try unstructured as fallback for office docs
                match external_parser.parse_with_unstructured(filename, data).await {
                    Ok(parsed) => Ok(parsed.content),
                    Err(unstructured_err) => {
                        tracing::error!("[{}] All parsing methods failed", filename);
                        Err(Error::Internal(format!(
                            "Failed to extract text from {}: native: {}, unstructured: {}",
                            filename, e, unstructured_err
                        )))
                    }
                }
            }
        }
    }

    /// Get documents map
    pub fn documents(&self) -> &DashMap<Uuid, Document> {
        &self.inner.documents
    }

    /// Check if the server is ready
    pub fn is_ready(&self) -> bool {
        *self.inner.ready.read()
    }

    /// Set ready state
    pub fn set_ready(&self, ready: bool) {
        *self.inner.ready.write() = ready;
    }

    /// Add a document to the registry (persisted to SQLite)
    pub fn add_document(&self, doc: Document) {
        self.save_document(&doc);
        self.inner.documents.insert(doc.id, doc);
    }

    /// Get a document by ID
    pub fn get_document(&self, id: &Uuid) -> Option<Document> {
        self.inner.documents.get(id).map(|d| d.clone())
    }

    /// Remove a document (persisted to SQLite)
    pub fn remove_document(&self, id: &Uuid) -> Option<Document> {
        let removed = self.inner.documents.remove(id).map(|(_, d)| d);
        if removed.is_some() {
            self.delete_document_from_db(id);
        }
        removed
    }

    /// List all documents
    pub fn list_documents(&self) -> Vec<Document> {
        self.inner
            .documents
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Find document by filename
    pub fn find_by_filename(&self, filename: &str) -> Option<Document> {
        self.inner
            .documents
            .iter()
            .find(|entry| entry.value().filename == filename)
            .map(|entry| entry.value().clone())
    }

    /// Find document by content hash
    pub fn find_by_hash(&self, content_hash: &str) -> Option<Document> {
        self.inner
            .documents
            .iter()
            .find(|entry| entry.value().content_hash == content_hash)
            .map(|entry| entry.value().clone())
    }

    /// Store chunks in the local chunk store (for Vertex AI metadata lookup)
    pub fn store_chunks(&self, chunks: &[Chunk]) {
        for chunk in chunks {
            self.inner.chunks.insert(chunk.id, chunk.clone());
        }
    }

    /// Get a chunk by ID from the local store, with database fallback
    /// This is critical for Vertex AI queries which return truncated metadata (500 chars)
    pub async fn get_chunk(&self, id: &Uuid) -> Option<Chunk> {
        // First check in-memory cache
        if let Some(chunk) = self.inner.chunks.get(id) {
            return Some(chunk.clone());
        }

        // Fallback to database (rag_chunks for PG, chunks_content for SQLite)
        #[cfg(feature = "postgres")]
        let db_result = self.inner.database.get_chunk_by_id(id).await;
        #[cfg(not(feature = "postgres"))]
        let db_result = self.inner.database.get_chunk_by_id(id);

        match db_result {
            Ok(Some(chunk)) => {
                // Cache it for future lookups
                self.inner.chunks.insert(*id, chunk.clone());
                tracing::debug!("Chunk {} loaded from database and cached", id);
                Some(chunk)
            }
            Ok(None) => {
                tracing::warn!("Chunk {} not found in cache or database", id);
                None
            }
            Err(e) => {
                tracing::error!("Failed to load chunk {} from database: {}", id, e);
                None
            }
        }
    }

    /// Check if file should be processed (returns action to take)
    /// Returns: (should_process, existing_doc_to_delete)
    pub fn check_file_status(&self, filename: &str, content_hash: &str) -> FileStatus {
        // First, check file registry (includes files synced from GCS)
        if let Some(record) = self.inner.file_registry.get(filename) {
            if record.content_hash == content_hash && record.status == FileRecordStatus::Success {
                // Same file, same content, already successfully processed - skip
                tracing::info!(
                    "File '{}' already exists in registry (hash: {}..., synced from GCS)",
                    filename,
                    &content_hash[..content_hash.len().min(12)]
                );
                return FileStatus::ExistsInRegistry(record.clone());
            }
        }

        // Check by content hash in file registry
        if let Some(record) = self.get_file_record_by_hash(content_hash) {
            if record.status == FileRecordStatus::Success {
                tracing::info!(
                    "File with same content already exists as '{}' (hash: {}...)",
                    record.filename,
                    &content_hash[..content_hash.len().min(12)]
                );
                return FileStatus::DuplicateInRegistry(record);
            }
        }

        // Check if exact same content exists in documents (by hash)
        if let Some(existing) = self.find_by_hash(content_hash) {
            if existing.filename == filename {
                // Same file, same content - skip
                return FileStatus::Unchanged(existing);
            } else {
                // Different filename, same content - could be a rename or duplicate
                // We'll still skip since content is identical
                return FileStatus::Duplicate(existing);
            }
        }

        // Check if file with same name exists but different content
        if let Some(existing) = self.find_by_filename(filename) {
            // Same filename, different content - file was modified
            return FileStatus::Modified(existing);
        }

        // New file
        FileStatus::New
    }

    /// Delete document and its chunks (async version using provider)
    pub async fn delete_document_with_chunks(&self, doc_id: &Uuid) -> crate::error::Result<usize> {
        // Invalidate cached answers that cite this document
        self.inner.answer_cache.invalidate_by_document(doc_id);

        // Delete chunks from vector store provider (works for both Local and GCP)
        let deleted = self.inner.vector_store_provider.delete_by_document(doc_id).await?;

        // Remove from document registry
        self.inner.documents.remove(doc_id);

        Ok(deleted)
    }

    // ==================== File Registry Methods ====================

    /// Record a successful file processing
    #[allow(clippy::too_many_arguments)]
    pub fn record_file_success(
        &self,
        organization_id: &str,
        filename: &str,
        content_hash: &str,
        file_size: u64,
        file_type: crate::types::FileType,
        document_id: Uuid,
        chunks_created: u32,
        job_id: Option<Uuid>,
    ) {
        let record = FileRecord::success(
            FileRecordParams {
                organization_id: organization_id.to_string(),
                filename: filename.to_string(),
                content_hash: content_hash.to_string(),
                file_size,
                file_type,
                job_id,
            },
            document_id,
            chunks_created,
        );
        // Save to database
        #[cfg(feature = "postgres")]
        {
            let db = self.inner.database.clone();
            let r = record.clone();
            tokio::spawn(async move {
                if let Err(e) = db.upsert_file_record(&r).await {
                    tracing::error!("Failed to save file record to PG: {}", e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.inner.database.upsert_file_record(&record) {
            tracing::error!("Failed to save file record to database: {}", e);
        }
        // Update in-memory cache
        self.inner.file_registry.insert(filename.to_string(), record);
    }

    /// Record a skipped file
    #[allow(clippy::too_many_arguments)]
    pub fn record_file_skipped(
        &self,
        organization_id: &str,
        filename: &str,
        content_hash: &str,
        file_size: u64,
        file_type: crate::types::FileType,
        skip_reason: SkipReason,
        job_id: Option<Uuid>,
    ) {
        let record = if let Some(mut existing) = self.inner.file_registry.get_mut(filename) {
            existing.update_for_reupload(job_id);
            existing.status = FileRecordStatus::Skipped;
            existing.skip_reason = Some(skip_reason);
            existing.clone()
        } else {
            FileRecord::skipped(
                FileRecordParams {
                    organization_id: organization_id.to_string(),
                    filename: filename.to_string(),
                    content_hash: content_hash.to_string(),
                    file_size,
                    file_type,
                    job_id,
                },
                skip_reason,
            )
        };
        // Save to database
        #[cfg(feature = "postgres")]
        {
            let db = self.inner.database.clone();
            let r = record.clone();
            tokio::spawn(async move {
                if let Err(e) = db.upsert_file_record(&r).await {
                    tracing::error!("Failed to save file record to PG: {}", e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.inner.database.upsert_file_record(&record) {
            tracing::error!("Failed to save file record to database: {}", e);
        }
        // Update in-memory cache
        self.inner.file_registry.insert(filename.to_string(), record);
    }

    /// Record a failed file
    #[allow(clippy::too_many_arguments)]
    pub fn record_file_failed(
        &self,
        organization_id: &str,
        filename: &str,
        content_hash: &str,
        file_size: u64,
        file_type: crate::types::FileType,
        error_message: &str,
        failed_at_stage: &str,
        job_id: Option<Uuid>,
    ) {
        let record = if let Some(mut existing) = self.inner.file_registry.get_mut(filename) {
            existing.update_for_reupload(job_id);
            existing.mark_failed(error_message.to_string(), failed_at_stage.to_string());
            existing.clone()
        } else {
            FileRecord::failed(
                FileRecordParams {
                    organization_id: organization_id.to_string(),
                    filename: filename.to_string(),
                    content_hash: content_hash.to_string(),
                    file_size,
                    file_type,
                    job_id,
                },
                error_message.to_string(),
                failed_at_stage.to_string(),
            )
        };
        // Save to database
        #[cfg(feature = "postgres")]
        {
            let db = self.inner.database.clone();
            let r = record.clone();
            tokio::spawn(async move {
                if let Err(e) = db.upsert_file_record(&r).await {
                    tracing::error!("Failed to save file record to PG: {}", e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.inner.database.upsert_file_record(&record) {
            tracing::error!("Failed to save file record to database: {}", e);
        }
        // Update in-memory cache
        self.inner.file_registry.insert(filename.to_string(), record);
    }

    /// Get file record by filename
    pub fn get_file_record(&self, filename: &str) -> Option<FileRecord> {
        self.inner.file_registry.get(filename).map(|r| r.clone())
    }

    /// Get file record by content hash
    pub fn get_file_record_by_hash(&self, content_hash: &str) -> Option<FileRecord> {
        self.inner
            .file_registry
            .iter()
            .find(|entry| entry.value().content_hash == content_hash)
            .map(|entry| entry.value().clone())
    }

    /// List all file records for an organization
    pub fn list_file_records(&self, organization_id: &str) -> Vec<FileRecord> {
        self.inner
            .file_registry
            .iter()
            .filter(|entry| entry.value().organization_id == organization_id)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// List successful file records for an organization
    pub fn list_successful_files(&self, organization_id: &str) -> Vec<FileRecord> {
        self.inner
            .file_registry
            .iter()
            .filter(|entry| {
                entry.value().organization_id == organization_id
                    && entry.value().status == FileRecordStatus::Success
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// List failed file records for an organization
    pub fn list_failed_files(&self, organization_id: &str) -> Vec<FileRecord> {
        self.inner
            .file_registry
            .iter()
            .filter(|entry| {
                entry.value().organization_id == organization_id
                    && entry.value().status == FileRecordStatus::Failed
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// List skipped file records for an organization
    pub fn list_skipped_files(&self, organization_id: &str) -> Vec<FileRecord> {
        self.inner
            .file_registry
            .iter()
            .filter(|entry| {
                entry.value().organization_id == organization_id
                    && entry.value().status == FileRecordStatus::Skipped
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get file registry statistics for an organization
    ///
    /// Computes all stats in a single pass over the registry for efficiency.
    pub fn file_registry_stats(&self, organization_id: &str) -> FileRegistryStats {
        let mut total = 0usize;
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;

        for entry in self.inner.file_registry.iter() {
            if entry.value().organization_id == organization_id {
                total += 1;
                match entry.value().status {
                    FileRecordStatus::Success => success += 1,
                    FileRecordStatus::Failed => failed += 1,
                    FileRecordStatus::Skipped => skipped += 1,
                    _ => {}
                }
            }
        }

        FileRegistryStats { total, success, failed, skipped }
    }

    /// Remove a file record
    pub fn remove_file_record(&self, filename: &str) -> Option<FileRecord> {
        // Remove from database (fire-and-forget for async)
        #[cfg(feature = "postgres")]
        {
            let db = self.inner.database.clone();
            let fname = filename.to_string();
            tokio::spawn(async move {
                if let Err(e) = db.delete_file_record(&fname).await {
                    tracing::error!("Failed to delete file record from PG: {}", e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.inner.database.delete_file_record(filename) {
            tracing::error!("Failed to delete file record from database: {}", e);
        }
        // Remove from in-memory cache
        self.inner.file_registry.remove(filename).map(|(_, r)| r)
    }

    /// Clear all failed file records for an organization (for retry)
    pub async fn clear_failed_files(&self, organization_id: &str) -> usize {
        // Clear from database
        #[cfg(feature = "postgres")]
        let db_count = match self.inner.database.clear_failed_files().await {
            Ok(count) => count,
            Err(e) => {
                tracing::error!("Failed to clear failed files from PG: {}", e);
                0
            }
        };
        #[cfg(not(feature = "postgres"))]
        let db_count = match self.inner.database.clear_failed_files() {
            Ok(count) => count,
            Err(e) => {
                tracing::error!("Failed to clear failed files from database: {}", e);
                0
            }
        };

        // Clear from in-memory cache for this organization only
        let failed_keys: Vec<String> = self.inner
            .file_registry
            .iter()
            .filter(|e| {
                e.value().organization_id == organization_id
                    && e.value().status == FileRecordStatus::Failed
            })
            .map(|e| e.key().clone())
            .collect();

        for key in &failed_keys {
            self.inner.file_registry.remove(key);
        }

        db_count.max(failed_keys.len())
    }

    /// Get database statistics
    pub async fn database_stats(&self) -> FileRegistryDbStats {
        #[cfg(feature = "postgres")]
        {
            self.inner.database.get_stats().await.unwrap_or(FileRegistryDbStats {
                total: 0,
                success: 0,
                failed: 0,
                skipped: 0,
            })
        }
        #[cfg(not(feature = "postgres"))]
        {
            self.inner.database.get_stats().unwrap_or(FileRegistryDbStats {
                total: 0,
                success: 0,
                failed: 0,
                skipped: 0,
            })
        }
    }
}

/// Statistics for file registry
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileRegistryStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Status of a file for deduplication
#[derive(Debug, Clone)]
pub enum FileStatus {
    /// File is new, process it
    New,
    /// File exists with same content - skip processing
    Unchanged(Document),
    /// Same content exists under different filename - skip
    Duplicate(Document),
    /// File exists but content changed - delete old and reprocess
    Modified(Document),
    /// File already exists in registry (synced from GCS) - skip
    ExistsInRegistry(FileRecord),
    /// Same content exists in registry under different filename - skip
    DuplicateInRegistry(FileRecord),
}
