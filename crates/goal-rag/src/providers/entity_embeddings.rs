//! Entity embedding store for tasks, goals, comments, and messages
//!
//! Stores vector embeddings of structured entities in PostgreSQL using pgvector.
//! Enables semantic similarity search, workflow pattern detection, and
//! embedding-based sentiment analysis.

use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use crate::providers::EmbeddingProvider;

/// Entity data prepared for embedding
#[derive(Debug, Clone)]
pub struct EntityForEmbedding {
    pub organization_id: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub content: String,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub sentiment: Option<f32>,
    pub actor_id: Option<Uuid>,
    pub parent_entity_type: Option<String>,
    pub parent_entity_id: Option<Uuid>,
    pub change_type: String,
    pub source_tool: Option<String>,
}

/// Result of a similarity search against entity embeddings
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntitySearchResult {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub content: String,
    pub similarity: f32,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub sentiment: Option<f32>,
    pub actor_id: Option<Uuid>,
    pub parent_entity_type: Option<String>,
    pub parent_entity_id: Option<Uuid>,
    pub source_tool: Option<String>,
}

/// Result of a backfill operation
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackfillResult {
    pub entity_type: String,
    pub total_found: usize,
    pub embedded: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// Embedding counts by entity type
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingStats {
    pub organization_id: String,
    pub total: i64,
    pub by_type: Vec<TypeCount>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TypeCount {
    pub entity_type: String,
    pub count: i64,
}

/// Cached anchor embeddings for sentiment computation
struct SentimentAnchors {
    positive: Vec<f32>,
    negative: Vec<f32>,
}

/// pgvector-based store for entity embeddings
pub struct EntityEmbeddingStore {
    pool: Arc<PgPool>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    dimensions: usize,
    sentiment_anchors: Option<SentimentAnchors>,
}

impl EntityEmbeddingStore {
    /// Create a new entity embedding store, initializing the schema if needed
    pub async fn new(
        pool: Arc<PgPool>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        dimensions: usize,
    ) -> Result<Self> {
        let mut store = Self {
            pool,
            embedding_provider,
            dimensions,
            sentiment_anchors: None,
        };
        store.init_schema().await?;
        store.init_sentiment_anchors().await;
        Ok(store)
    }

    /// Initialize the entity_embeddings table and indexes
    async fn init_schema(&self) -> Result<()> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        // Ensure pgvector extension exists
        client
            .execute("CREATE EXTENSION IF NOT EXISTS vector", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to create vector extension: {}", e)))?;

        let create_table = format!(
            r#"
            CREATE TABLE IF NOT EXISTS entity_embeddings (
                id UUID PRIMARY KEY,
                organization_id TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id UUID NOT NULL,
                content TEXT NOT NULL,
                embedding vector({dimensions}),
                status TEXT,
                priority TEXT,
                sentiment REAL,
                actor_id UUID,
                parent_entity_type TEXT,
                parent_entity_id UUID,
                change_type TEXT NOT NULL,
                embedded_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(entity_type, entity_id)
            )
            "#,
            dimensions = self.dimensions
        );
        client
            .execute(&create_table, &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to create entity_embeddings table: {}", e)))?;

        // Add source_tool column if missing (safe for existing deployments)
        client
            .execute(
                "ALTER TABLE entity_embeddings ADD COLUMN IF NOT EXISTS source_tool TEXT",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to add source_tool column: {}", e)))?;

        // Create indexes
        let indexes = [
            "CREATE INDEX IF NOT EXISTS idx_ee_org ON entity_embeddings(organization_id)",
            "CREATE INDEX IF NOT EXISTS idx_ee_type_org ON entity_embeddings(organization_id, entity_type)",
            "CREATE INDEX IF NOT EXISTS idx_ee_parent ON entity_embeddings(parent_entity_type, parent_entity_id)",
            "CREATE INDEX IF NOT EXISTS idx_ee_sentiment ON entity_embeddings(organization_id, sentiment)",
        ];
        for idx in &indexes {
            client.execute(*idx, &[]).await.map_err(|e| {
                Error::Internal(format!("Failed to create index: {}", e))
            })?;
        }

        // HNSW vector index
        client
            .execute(
                r#"
                CREATE INDEX IF NOT EXISTS idx_ee_embedding_hnsw
                ON entity_embeddings
                USING hnsw (embedding vector_cosine_ops)
                WITH (m = 16, ef_construction = 64)
                "#,
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to create HNSW index: {}", e)))?;

        tracing::info!(
            "entity_embeddings schema initialized (dimensions: {}, index: HNSW)",
            self.dimensions
        );
        Ok(())
    }

    /// Pre-compute sentiment anchor embeddings for fast sentiment analysis
    async fn init_sentiment_anchors(&mut self) {
        let positive_anchor = "Great work, this is exactly what we needed. Thank you for the excellent results.";
        let negative_anchor = "This is blocking our progress. We are falling behind and the situation is getting worse.";
        match self.embedding_provider.embed_batch(&[
            positive_anchor.to_string(),
            negative_anchor.to_string(),
        ]).await {
            Ok(embeddings) if embeddings.len() == 2 => {
                self.sentiment_anchors = Some(SentimentAnchors {
                    positive: embeddings[0].clone(),
                    negative: embeddings[1].clone(),
                });
                tracing::info!("Sentiment anchor embeddings cached");
            }
            Ok(_) => {
                tracing::warn!("Unexpected anchor embedding count, sentiment caching disabled");
            }
            Err(e) => {
                tracing::warn!("Failed to cache sentiment anchors (will embed on demand): {}", e);
            }
        }
    }

    /// Get a PostgreSQL client from the connection pool
    pub async fn pool_client(&self) -> Result<deadpool_postgres::Client> {
        self.pool.get().await
    }

    /// Generate embedding for entity text and upsert into the table
    pub async fn embed_and_store(&self, entity: &EntityForEmbedding) -> Result<()> {
        if entity.content.trim().is_empty() {
            return Ok(()); // Skip empty content
        }

        let embedding = self.embedding_provider.embed(&entity.content).await?;
        let embedding_vec = pgvector::Vector::from(embedding);

        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        client
            .execute(
                r#"
                INSERT INTO entity_embeddings (
                    id, organization_id, entity_type, entity_id, content, embedding,
                    status, priority, sentiment, actor_id,
                    parent_entity_type, parent_entity_id, change_type, embedded_at, source_tool
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), $14)
                ON CONFLICT (entity_type, entity_id) DO UPDATE SET
                    content = EXCLUDED.content,
                    embedding = EXCLUDED.embedding,
                    status = EXCLUDED.status,
                    priority = EXCLUDED.priority,
                    sentiment = EXCLUDED.sentiment,
                    actor_id = EXCLUDED.actor_id,
                    change_type = EXCLUDED.change_type,
                    embedded_at = NOW(),
                    source_tool = COALESCE(EXCLUDED.source_tool, entity_embeddings.source_tool)
                "#,
                &[
                    &Uuid::new_v4(),
                    &entity.organization_id,
                    &entity.entity_type,
                    &entity.entity_id,
                    &entity.content,
                    &embedding_vec,
                    &entity.status,
                    &entity.priority,
                    &entity.sentiment,
                    &entity.actor_id,
                    &entity.parent_entity_type,
                    &entity.parent_entity_id,
                    &entity.change_type,
                    &entity.source_tool,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upsert entity embedding: {}", e)))?;

        tracing::debug!(
            entity_type = %entity.entity_type,
            entity_id = %entity.entity_id,
            "Stored entity embedding"
        );
        Ok(())
    }

    /// Search for entities similar to a query text
    pub async fn search_similar(
        &self,
        query_text: &str,
        org_id: &str,
        entity_type: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<EntitySearchResult>> {
        let query_embedding = self.embedding_provider.embed(query_text).await?;
        self.search_by_embedding(&query_embedding, org_id, entity_type, top_k).await
    }

    /// Search by a pre-computed embedding vector
    pub async fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        org_id: &str,
        entity_type: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<EntitySearchResult>> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let query_vec = pgvector::Vector::from(query_embedding.to_vec());
        let limit = top_k as i64;

        let rows = match entity_type {
            Some(et) => {
                client.query(
                    r#"
                    SELECT entity_type, entity_id, content, status, priority, sentiment,
                           actor_id, parent_entity_type, parent_entity_id, source_tool,
                           1 - (embedding <=> $1) as similarity
                    FROM entity_embeddings
                    WHERE organization_id = $2 AND entity_type = $3
                    ORDER BY embedding <=> $1
                    LIMIT $4
                    "#,
                    &[&query_vec, &org_id, &et, &limit],
                ).await
            }
            None => {
                client.query(
                    r#"
                    SELECT entity_type, entity_id, content, status, priority, sentiment,
                           actor_id, parent_entity_type, parent_entity_id, source_tool,
                           1 - (embedding <=> $1) as similarity
                    FROM entity_embeddings
                    WHERE organization_id = $2
                    ORDER BY embedding <=> $1
                    LIMIT $3
                    "#,
                    &[&query_vec, &org_id, &limit],
                ).await
            }
        }.map_err(|e| Error::Internal(format!("Failed to search entity embeddings: {}", e)))?;

        Ok(rows.iter().map(|row| EntitySearchResult {
            entity_type: row.get("entity_type"),
            entity_id: row.get("entity_id"),
            content: row.get("content"),
            similarity: {
                let sim: f64 = row.get("similarity");
                sim as f32
            },
            status: row.get("status"),
            priority: row.get("priority"),
            sentiment: row.get("sentiment"),
            actor_id: row.get("actor_id"),
            parent_entity_type: row.get("parent_entity_type"),
            parent_entity_id: row.get("parent_entity_id"),
            source_tool: row.get("source_tool"),
        }).collect())
    }

    /// Find entities similar to an existing entity
    pub async fn search_similar_to_entity(
        &self,
        entity_type: &str,
        entity_id: &Uuid,
        org_id: &str,
        search_type: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<EntitySearchResult>> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        // Get the embedding of the reference entity
        let row = client.query_opt(
            r#"
            SELECT embedding FROM entity_embeddings
            WHERE entity_type = $1 AND entity_id = $2 AND organization_id = $3
            "#,
            &[&entity_type, entity_id, &org_id],
        ).await.map_err(|e| Error::Internal(format!("Failed to get entity embedding: {}", e)))?;

        let row = row.ok_or_else(|| Error::Validation(format!(
            "No embedding found for {} {}", entity_type, entity_id
        )))?;

        let embedding: pgvector::Vector = row.get("embedding");
        let embedding_slice: Vec<f32> = embedding.to_vec();

        // Search for similar entities (excluding the reference entity itself)
        let limit = (top_k + 1) as i64; // +1 to account for self-match
        let query_vec = pgvector::Vector::from(embedding_slice);

        let rows = match search_type {
            Some(st) => {
                client.query(
                    r#"
                    SELECT entity_type, entity_id, content, status, priority, sentiment,
                           actor_id, parent_entity_type, parent_entity_id, source_tool,
                           1 - (embedding <=> $1) as similarity
                    FROM entity_embeddings
                    WHERE organization_id = $2 AND entity_type = $3
                          AND NOT (entity_type = $4 AND entity_id = $5)
                    ORDER BY embedding <=> $1
                    LIMIT $6
                    "#,
                    &[&query_vec, &org_id, &st, &entity_type, entity_id, &limit],
                ).await
            }
            None => {
                client.query(
                    r#"
                    SELECT entity_type, entity_id, content, status, priority, sentiment,
                           actor_id, parent_entity_type, parent_entity_id, source_tool,
                           1 - (embedding <=> $1) as similarity
                    FROM entity_embeddings
                    WHERE organization_id = $2
                          AND NOT (entity_type = $3 AND entity_id = $4)
                    ORDER BY embedding <=> $1
                    LIMIT $5
                    "#,
                    &[&query_vec, &org_id, &entity_type, entity_id, &limit],
                ).await
            }
        }.map_err(|e| Error::Internal(format!("Failed to search similar entities: {}", e)))?;

        Ok(rows.iter().take(top_k).map(|row| EntitySearchResult {
            entity_type: row.get("entity_type"),
            entity_id: row.get("entity_id"),
            content: row.get("content"),
            similarity: {
                let sim: f64 = row.get("similarity");
                sim as f32
            },
            status: row.get("status"),
            priority: row.get("priority"),
            sentiment: row.get("sentiment"),
            actor_id: row.get("actor_id"),
            parent_entity_type: row.get("parent_entity_type"),
            parent_entity_id: row.get("parent_entity_id"),
            source_tool: row.get("source_tool"),
        }).collect())
    }

    /// Backfill embeddings for existing entities in a source table
    pub async fn backfill(
        &self,
        org_id: &str,
        entity_type: &str,
        batch_size: usize,
    ) -> Result<BackfillResult> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let mut result = BackfillResult {
            entity_type: entity_type.to_string(),
            total_found: 0,
            embedded: 0,
            skipped: 0,
            errors: 0,
        };

        let limit = batch_size as i64;

        loop {
            // Fetch entities not yet embedded
            let rows = match entity_type {
                "task" => {
                    client.query(
                        r#"
                        SELECT t.id, t.title, t.description, t.status, t.priority,
                               t.assigned_to, t.goal_id
                        FROM api.tasks t
                        LEFT JOIN entity_embeddings ee
                          ON ee.entity_id = t.id
                          AND ee.entity_type = 'task'
                          AND ee.organization_id = $2
                        WHERE t.organization_id = $1::uuid
                          AND ee.id IS NULL
                        ORDER BY t.created_at DESC
                        LIMIT $3
                        "#,
                        &[&Uuid::parse_str(org_id).map_err(|e| Error::Validation(format!("Invalid org UUID: {}", e)))?, &org_id, &limit],
                    ).await
                }
                "goal" => {
                    client.query(
                        r#"
                        SELECT g.id, g.title, g.description, g.status,
                               g.created_by, g.parent_goal_id
                        FROM api.goals g
                        LEFT JOIN entity_embeddings ee
                          ON ee.entity_id = g.id
                          AND ee.entity_type = 'goal'
                          AND ee.organization_id = $2
                        WHERE g.organization_id = $1::uuid
                          AND ee.id IS NULL
                        ORDER BY g.created_at DESC
                        LIMIT $3
                        "#,
                        &[&Uuid::parse_str(org_id).map_err(|e| Error::Validation(format!("Invalid org UUID: {}", e)))?, &org_id, &limit],
                    ).await
                }
                "task_comment" => {
                    client.query(
                        r#"
                        SELECT tc.id, tc.content, tc.author_id, tc.task_id
                        FROM api.task_comments tc
                        LEFT JOIN entity_embeddings ee
                          ON ee.entity_id = tc.id
                          AND ee.entity_type = 'task_comment'
                          AND ee.organization_id = $2
                        WHERE tc.organization_id = $1::uuid
                          AND ee.id IS NULL
                        ORDER BY tc.created_at DESC
                        LIMIT $3
                        "#,
                        &[&Uuid::parse_str(org_id).map_err(|e| Error::Validation(format!("Invalid org UUID: {}", e)))?, &org_id, &limit],
                    ).await
                }
                "message" => {
                    client.query(
                        r#"
                        SELECT m.id, m.content, m.sender_id
                        FROM api.messages m
                        LEFT JOIN entity_embeddings ee
                          ON ee.entity_id = m.id
                          AND ee.entity_type = 'message'
                          AND ee.organization_id = $2
                        WHERE m.organization_id = $1::uuid
                          AND ee.id IS NULL
                        ORDER BY m.created_at DESC
                        LIMIT $3
                        "#,
                        &[&Uuid::parse_str(org_id).map_err(|e| Error::Validation(format!("Invalid org UUID: {}", e)))?, &org_id, &limit],
                    ).await
                }
                "chat_message" => {
                    client.query(
                        r#"
                        SELECT cm.id, cm.content, cm.conversation_id
                        FROM api.chat_messages cm
                        LEFT JOIN entity_embeddings ee
                          ON ee.entity_id = cm.id
                          AND ee.entity_type = 'chat_message'
                          AND ee.organization_id = $2
                        WHERE cm.organization_id = $1::uuid
                          AND ee.id IS NULL
                        ORDER BY cm.created_at DESC
                        LIMIT $3
                        "#,
                        &[&Uuid::parse_str(org_id).map_err(|e| Error::Validation(format!("Invalid org UUID: {}", e)))?, &org_id, &limit],
                    ).await
                }
                _ => return Err(Error::Validation(format!("Unknown entity type: {}", entity_type))),
            }.map_err(|e| Error::Internal(format!("Failed to fetch entities for backfill: {}", e)))?;

            if rows.is_empty() {
                break;
            }

            result.total_found += rows.len();

            // Build entities and collect texts for batch embedding
            let mut entities: Vec<EntityForEmbedding> = Vec::with_capacity(rows.len());
            let mut texts: Vec<String> = Vec::with_capacity(rows.len());

            for row in &rows {
                let entity = match entity_type {
                    "task" => {
                        let title: String = row.get("title");
                        let desc: Option<String> = row.get("description");
                        let content = format!("{}: {}", title, desc.as_deref().unwrap_or(""));
                        EntityForEmbedding {
                            organization_id: org_id.to_string(),
                            entity_type: "task".to_string(),
                            entity_id: row.get("id"),
                            content: content.clone(),
                            status: row.get("status"),
                            priority: row.get("priority"),
                            sentiment: None,
                            actor_id: row.get("assigned_to"),
                            parent_entity_type: row.try_get::<_, Option<Uuid>>("goal_id").ok().flatten().map(|_| "goal".to_string()),
                            parent_entity_id: row.try_get("goal_id").ok().flatten(),
                            change_type: "backfill".to_string(),
                            source_tool: Some("backfill".to_string()),
                        }
                    }
                    "goal" => {
                        let title: String = row.get("title");
                        let desc: Option<String> = row.get("description");
                        let content = format!("{}: {}", title, desc.as_deref().unwrap_or(""));
                        EntityForEmbedding {
                            organization_id: org_id.to_string(),
                            entity_type: "goal".to_string(),
                            entity_id: row.get("id"),
                            content: content.clone(),
                            status: row.get("status"),
                            priority: None,
                            sentiment: None,
                            actor_id: row.get("created_by"),
                            parent_entity_type: row.try_get::<_, Option<Uuid>>("parent_goal_id").ok().flatten().map(|_| "goal".to_string()),
                            parent_entity_id: row.try_get("parent_goal_id").ok().flatten(),
                            change_type: "backfill".to_string(),
                            source_tool: Some("backfill".to_string()),
                        }
                    }
                    "task_comment" => {
                        let content: String = row.get("content");
                        EntityForEmbedding {
                            organization_id: org_id.to_string(),
                            entity_type: "task_comment".to_string(),
                            entity_id: row.get("id"),
                            content: content.clone(),
                            status: None,
                            priority: None,
                            sentiment: None,
                            actor_id: row.get("author_id"),
                            parent_entity_type: Some("task".to_string()),
                            parent_entity_id: row.try_get("task_id").ok().flatten(),
                            change_type: "backfill".to_string(),
                            source_tool: Some("backfill".to_string()),
                        }
                    }
                    "message" => {
                        let content: String = row.get("content");
                        EntityForEmbedding {
                            organization_id: org_id.to_string(),
                            entity_type: "message".to_string(),
                            entity_id: row.get("id"),
                            content: content.clone(),
                            status: None,
                            priority: None,
                            sentiment: None,
                            actor_id: row.get("sender_id"),
                            parent_entity_type: None,
                            parent_entity_id: None,
                            change_type: "backfill".to_string(),
                            source_tool: Some("backfill".to_string()),
                        }
                    }
                    "chat_message" => {
                        let content: String = row.get("content");
                        EntityForEmbedding {
                            organization_id: org_id.to_string(),
                            entity_type: "chat_message".to_string(),
                            entity_id: row.get("id"),
                            content: content.clone(),
                            status: None,
                            priority: None,
                            sentiment: None,
                            actor_id: None,
                            parent_entity_type: Some("conversation".to_string()),
                            parent_entity_id: row.try_get("conversation_id").ok().flatten(),
                            change_type: "backfill".to_string(),
                            source_tool: Some("backfill".to_string()),
                        }
                    }
                    _ => unreachable!(),
                };

                if entity.content.trim().is_empty() {
                    result.skipped += 1;
                    continue;
                }
                texts.push(entity.content.clone());
                entities.push(entity);
            }

            if texts.is_empty() {
                break;
            }

            // Batch embed
            let embeddings = self.embedding_provider.embed_batch(&texts).await?;

            // Store each with its embedding
            for (entity, embedding) in entities.iter().zip(embeddings.iter()) {
                match self.store_with_embedding(entity, embedding).await {
                    Ok(_) => result.embedded += 1,
                    Err(e) => {
                        tracing::warn!(
                            entity_type = %entity.entity_type,
                            entity_id = %entity.entity_id,
                            "Backfill embedding failed: {}", e
                        );
                        result.errors += 1;
                    }
                }
            }

            tracing::info!(
                entity_type = %entity_type,
                batch_embedded = entities.len(),
                total_so_far = result.embedded,
                "Backfill batch complete"
            );

            // If we got fewer than batch_size, we're done
            if (rows.len() as i64) < limit {
                break;
            }
        }

        Ok(result)
    }

    /// Store an entity with a pre-computed embedding
    async fn store_with_embedding(&self, entity: &EntityForEmbedding, embedding: &[f32]) -> Result<()> {
        let embedding_vec = pgvector::Vector::from(embedding.to_vec());

        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        client
            .execute(
                r#"
                INSERT INTO entity_embeddings (
                    id, organization_id, entity_type, entity_id, content, embedding,
                    status, priority, sentiment, actor_id,
                    parent_entity_type, parent_entity_id, change_type, embedded_at, source_tool
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), $14)
                ON CONFLICT (entity_type, entity_id) DO UPDATE SET
                    content = EXCLUDED.content,
                    embedding = EXCLUDED.embedding,
                    status = EXCLUDED.status,
                    priority = EXCLUDED.priority,
                    sentiment = EXCLUDED.sentiment,
                    actor_id = EXCLUDED.actor_id,
                    change_type = EXCLUDED.change_type,
                    embedded_at = NOW(),
                    source_tool = COALESCE(EXCLUDED.source_tool, entity_embeddings.source_tool)
                "#,
                &[
                    &Uuid::new_v4(),
                    &entity.organization_id,
                    &entity.entity_type,
                    &entity.entity_id,
                    &entity.content,
                    &embedding_vec,
                    &entity.status,
                    &entity.priority,
                    &entity.sentiment,
                    &entity.actor_id,
                    &entity.parent_entity_type,
                    &entity.parent_entity_id,
                    &entity.change_type,
                    &entity.source_tool,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to store entity embedding: {}", e)))?;

        Ok(())
    }

    /// Get embedding statistics for an organization
    pub async fn get_stats(&self, org_id: &str) -> Result<EmbeddingStats> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let total_row = client
            .query_one(
                "SELECT COUNT(*) as count FROM entity_embeddings WHERE organization_id = $1",
                &[&org_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to count embeddings: {}", e)))?;

        let total: i64 = total_row.get("count");

        let type_rows = client
            .query(
                r#"
                SELECT entity_type, COUNT(*) as count
                FROM entity_embeddings
                WHERE organization_id = $1
                GROUP BY entity_type
                ORDER BY count DESC
                "#,
                &[&org_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get type counts: {}", e)))?;

        let by_type = type_rows
            .iter()
            .map(|row| TypeCount {
                entity_type: row.get("entity_type"),
                count: row.get("count"),
            })
            .collect();

        Ok(EmbeddingStats {
            organization_id: org_id.to_string(),
            total,
            by_type,
        })
    }

    /// Delete an entity's embedding
    pub async fn delete_entity(&self, entity_type: &str, entity_id: &Uuid) -> Result<bool> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let deleted = client
            .execute(
                "DELETE FROM entity_embeddings WHERE entity_type = $1 AND entity_id = $2",
                &[&entity_type, entity_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to delete entity embedding: {}", e)))?;

        Ok(deleted > 0)
    }

    /// Resolve an organization identifier to the UUID string used in entity_embeddings.
    ///
    /// If org_id is already a UUID, returns it. Otherwise, looks up the UUID from
    /// api.organizations by matching the slug to the name (lowercased, spaces→hyphens).
    /// Cache for org slug → UUID string resolution (60-second TTL)
    fn org_id_cache() -> &'static std::sync::RwLock<std::collections::HashMap<String, (String, std::time::Instant)>> {
        static CACHE: std::sync::LazyLock<std::sync::RwLock<std::collections::HashMap<String, (String, std::time::Instant)>>> =
            std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));
        &CACHE
    }

    pub async fn resolve_org_id(&self, org_id: &str) -> Result<String> {
        // Already a UUID? Return as-is.
        if Uuid::parse_str(org_id).is_ok() {
            return Ok(org_id.to_string());
        }

        // Check cache first
        if let Ok(cache) = Self::org_id_cache().read() {
            if let Some((uuid_str, ts)) = cache.get(org_id) {
                if ts.elapsed().as_secs() < 60 {
                    return Ok(uuid_str.clone());
                }
            }
        }

        // Slug → UUID lookup via api.organizations
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let row = client.query_opt(
            "SELECT id::text FROM api.organizations WHERE lower(replace(name, ' ', '-')) = $1",
            &[&org_id],
        ).await.map_err(|e| Error::Internal(format!("Failed to look up org UUID: {}", e)))?;

        match row {
            Some(r) => {
                let uuid_str: String = r.get("id");
                if let Ok(mut cache) = Self::org_id_cache().write() {
                    cache.insert(org_id.to_string(), (uuid_str.clone(), std::time::Instant::now()));
                }
                Ok(uuid_str)
            }
            None => Err(Error::Validation(format!(
                "Organization '{}' not found in entity embeddings", org_id
            ))),
        }
    }

    /// Compute embedding-based sentiment by comparing to cached anchor texts
    ///
    /// If anchors are cached (normal case), makes 1 API call to embed the input text.
    /// If anchors are not cached, falls back to embedding all texts together.
    pub async fn compute_sentiment(&self, text: &str) -> Result<f32> {
        if let Some(ref anchors) = self.sentiment_anchors {
            // Fast path: only embed the input text (1 API call)
            let text_emb = self.embedding_provider.embed(text).await?;
            let pos_sim = cosine_similarity(&text_emb, &anchors.positive);
            let neg_sim = cosine_similarity(&text_emb, &anchors.negative);
            let score = (pos_sim - neg_sim) / (pos_sim + neg_sim + 0.001);
            Ok(score.clamp(-1.0, 1.0))
        } else {
            // Fallback: embed everything together
            let positive_anchor = "Great work, this is exactly what we needed. Thank you for the excellent results.";
            let negative_anchor = "This is blocking our progress. We are falling behind and the situation is getting worse.";
            let embeddings = self.embedding_provider.embed_batch(&[
                text.to_string(),
                positive_anchor.to_string(),
                negative_anchor.to_string(),
            ]).await?;

            let text_emb = &embeddings[0];
            let pos_emb = &embeddings[1];
            let neg_emb = &embeddings[2];

            let pos_sim = cosine_similarity(text_emb, pos_emb);
            let neg_sim = cosine_similarity(text_emb, neg_emb);

            let score = (pos_sim - neg_sim) / (pos_sim + neg_sim + 0.001);
            Ok(score.clamp(-1.0, 1.0))
        }
    }
}

/// Cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
