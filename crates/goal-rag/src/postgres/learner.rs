//! Database learning pipeline
//!
//! Processes database changes in real-time and feeds them into the
//! pattern learning system for continuous improvement.

use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use super::listener::{ChangeEvent, ChangeType};
use super::pool::PgPool;
use super::schema::InteractionData;
use crate::analytics::{
    AnalyticsDb, InteractionClassification, InteractionType, InteractionSource,
    WorkflowTimeline, LearningSystem, UrgencyLevel, ExtractedEntities,
};
use crate::analytics::pattern_learner::PatternLearner;
use crate::error::Result;
use crate::providers::entity_embeddings::{EntityEmbeddingStore, EntityForEmbedding};

/// Database learner that processes changes and learns patterns
pub struct DatabaseLearner {
    pool: PgPool,
    analytics_db: Arc<AnalyticsDb>,
    /// Buffer of recent interactions per organization
    interaction_buffer: RwLock<HashMap<String, Vec<InteractionData>>>,
    /// Batch size before triggering pattern learning
    batch_size: usize,
    /// Pattern learner instance
    pattern_learner: PatternLearner,
    /// Entity embedding store for vectorizing entities on change
    entity_embedding_store: Option<Arc<EntityEmbeddingStore>>,
}

impl DatabaseLearner {
    /// Create a new database learner
    pub fn new(
        pool: PgPool,
        analytics_db: Arc<AnalyticsDb>,
        batch_size: usize,
        entity_embedding_store: Option<Arc<EntityEmbeddingStore>>,
    ) -> Self {
        Self {
            pool,
            analytics_db,
            interaction_buffer: RwLock::new(HashMap::new()),
            batch_size,
            pattern_learner: PatternLearner::default(),
            entity_embedding_store,
        }
    }

    /// Start the learning loop
    pub async fn start(&self, mut rx: mpsc::Receiver<ChangeEvent>) -> Result<()> {
        tracing::info!(batch_size = self.batch_size, "Database learner started");

        while let Some(event) = rx.recv().await {
            if let Err(e) = self.process_event(&event).await {
                tracing::error!(
                    table = %event.table,
                    error = %e,
                    "Failed to process change event"
                );
            }
        }

        tracing::info!("Database learner stopped");
        Ok(())
    }

    /// Process a single change event
    async fn process_event(&self, event: &ChangeEvent) -> Result<()> {
        // Convert to interaction data
        let interaction = self.event_to_interaction(event)?;

        // Get organization ID (default if none)
        let org_id = interaction.organization_id
            .map(|u| u.to_string())
            .unwrap_or_else(|| "_default".to_string());

        // Store the classification
        self.store_classification(&interaction, event).await?;

        // Generate and store entity embedding (non-blocking)
        if let Some(ref store) = self.entity_embedding_store {
            if let Some(ref content) = interaction.content {
                if !content.trim().is_empty() {
                    // Normalize table names to singular entity types
                    let entity_type = match interaction.entity_type.as_str() {
                        "tasks" => "task",
                        "goals" => "goal",
                        "task_comments" => "task_comment",
                        "users" => "user",
                        "chat_messages" => "chat_message",
                        "messages" => "message",
                        other => other,
                    };

                    let entity = EntityForEmbedding {
                        organization_id: org_id.clone(),
                        entity_type: entity_type.to_string(),
                        entity_id: interaction.entity_id,
                        content: content.clone(),
                        status: interaction.metadata.get("status")
                            .and_then(|v| v.as_str()).map(String::from),
                        priority: interaction.metadata.get("priority")
                            .and_then(|v| v.as_str()).map(String::from),
                        sentiment: Some(self.compute_sentiment_for_classification(&interaction.content).await),
                        actor_id: interaction.actor_id,
                        parent_entity_type: match entity_type {
                            "task_comment" => Some("task".to_string()),
                            "chat_message" => Some("conversation".to_string()),
                            _ => None,
                        },
                        parent_entity_id: interaction.metadata.get("task_id")
                            .or_else(|| interaction.metadata.get("conversation_id"))
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok()),
                        change_type: interaction.change_type.clone(),
                        source_tool: None,
                    };

                    let store_clone = Arc::clone(store);
                    tokio::spawn(async move {
                        if let Err(e) = store_clone.embed_and_store(&entity).await {
                            tracing::warn!(
                                entity_type = %entity.entity_type,
                                entity_id = %entity.entity_id,
                                "Failed to generate entity embedding: {}", e
                            );
                        }
                    });
                }
            }
        }

        // Add to buffer
        let should_learn = {
            let mut buffer = self.interaction_buffer.write().await;
            let org_buffer = buffer.entry(org_id.clone()).or_default();
            org_buffer.push(interaction);
            org_buffer.len() >= self.batch_size
        };

        // Trigger learning if batch size reached
        if should_learn {
            self.run_learning_cycle(&org_id).await?;
        }

        Ok(())
    }

    /// Convert a change event to interaction data
    fn event_to_interaction(&self, event: &ChangeEvent) -> Result<InteractionData> {
        let change_type_str = event.change_type.to_string().to_lowercase();

        // Extract organization_id from event or data
        let org_id = event.organization_id.or_else(|| {
            event.data.get("organization_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
        });

        // Extract row_id
        let row_id = event.row_id.or_else(|| {
            event.data.get("id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
        }).unwrap_or_else(Uuid::new_v4);

        // Extract actor_id based on table type
        let actor_id = match event.table.as_str() {
            "tasks" => event.data.get("assignee_id")
                .or_else(|| event.data.get("creator_id")),
            "goals" => event.data.get("owner_id"),
            "task_comments" => event.data.get("author_id")  // task_comments uses author_id
                .or_else(|| event.data.get("user_id")),
            "messages" | "chat_messages" => event.data.get("user_id")
                .or_else(|| event.data.get("sender_id")),
            _ => event.data.get("user_id"),
        }.and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        // Extract content based on table type
        let content = match event.table.as_str() {
            "tasks" => {
                let title = event.data.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let desc = event.data.get("description").and_then(|v| v.as_str()).unwrap_or("");
                Some(format!("{}: {}", title, desc))
            }
            "goals" => {
                let title = event.data.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let desc = event.data.get("description").and_then(|v| v.as_str()).unwrap_or("");
                Some(format!("{}: {}", title, desc))
            }
            "messages" | "chat_messages" | "task_comments" => {
                event.data.get("content").and_then(|v| v.as_str()).map(|s| s.to_string())
            }
            _ => serde_json::to_string(&event.data).ok(),
        };

        Ok(InteractionData {
            entity_type: event.table.clone(),
            entity_id: row_id,
            organization_id: org_id,
            actor_id,
            change_type: change_type_str,
            content,
            metadata: event.data.clone(),
            timestamp: event.timestamp,
        })
    }

    /// Store the interaction as a classification in the analytics database
    async fn store_classification(&self, interaction: &InteractionData, event: &ChangeEvent) -> Result<()> {
        let org_id = interaction.organization_id
            .map(|u| u.to_string())
            .unwrap_or_else(|| "_default".to_string());

        // Determine interaction type based on change and entity type
        let interaction_type = self.classify_interaction(interaction, event);

        // Map entity type to interaction source
        let source_type = match interaction.entity_type.as_str() {
            "task_comments" => InteractionSource::TaskComment,
            "goals" | "goal_comments" => InteractionSource::GoalComment,
            "messages" | "chat_messages" => InteractionSource::Message,
            _ => InteractionSource::ActivityLog,
        };

        let classification = InteractionClassification {
            id: Uuid::new_v4(),
            organization_id: org_id,
            source_type,
            source_id: interaction.entity_id.to_string(),
            task_id: if interaction.entity_type == "tasks" {
                Some(interaction.entity_id.to_string())
            } else {
                interaction.metadata.get("task_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            },
            goal_id: if interaction.entity_type == "goals" {
                Some(interaction.entity_id.to_string())
            } else {
                interaction.metadata.get("goal_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            },
            sender_id: interaction.actor_id
                .map(|u| u.to_string())
                .unwrap_or_else(|| "system".to_string()),
            content: interaction.content.clone().unwrap_or_default(),
            interaction_type,
            secondary_types: Vec::new(),
            confidence_score: 0.9, // High confidence for direct DB events
            entities: ExtractedEntities::default(),
            sentiment: self.compute_sentiment_for_classification(&interaction.content).await,
            urgency_level: self.detect_urgency(interaction),
            references_interaction_id: None,
            original_created_at: interaction.timestamp,
            classified_at: Utc::now(),
        };

        self.analytics_db.insert_classification(&classification)?;

        tracing::debug!(
            entity_type = %interaction.entity_type,
            change_type = %interaction.change_type,
            interaction_type = ?classification.interaction_type,
            "Stored interaction classification"
        );

        Ok(())
    }

    /// Classify the interaction type based on context
    fn classify_interaction(&self, interaction: &InteractionData, event: &ChangeEvent) -> InteractionType {
        match (&interaction.entity_type[..], event.change_type, &interaction.change_type[..]) {
            // Task changes
            ("tasks", ChangeType::Insert, _) => InteractionType::Assignment,
            ("tasks", ChangeType::Update, _) => {
                // Check if status changed to completed
                if let Some(status) = interaction.metadata.get("status").and_then(|v| v.as_str()) {
                    if status == "completed" || status == "done" {
                        return InteractionType::StatusUpdate;
                    }
                }
                InteractionType::StatusUpdate
            }
            ("tasks", ChangeType::Delete, _) => InteractionType::StatusUpdate,

            // Goal changes
            ("goals", ChangeType::Insert, _) => InteractionType::Direction,
            ("goals", ChangeType::Update, _) => InteractionType::StatusUpdate,
            ("goals", ChangeType::Delete, _) => InteractionType::Direction,

            // Comments and messages - classify as feedback/questions based on content
            ("task_comments", _, _) => InteractionType::Feedback,
            ("chat_messages", _, _) => InteractionType::Other,
            ("messages", _, _) => InteractionType::Other,

            // User changes
            ("users", ChangeType::Insert, _) => InteractionType::Other,
            ("users", ChangeType::Update, _) => InteractionType::StatusUpdate,

            // Default
            _ => InteractionType::Other,
        }
    }

    /// Detect urgency level from interaction content
    fn detect_urgency(&self, interaction: &InteractionData) -> UrgencyLevel {
        let content = interaction.content.as_deref().unwrap_or("");
        let content_lower = content.to_lowercase();

        // Check for priority in metadata
        if let Some(priority) = interaction.metadata.get("priority").and_then(|v| v.as_str()) {
            match priority.to_lowercase().as_str() {
                "critical" | "urgent" | "p0" | "p1" => return UrgencyLevel::Critical,
                "high" | "p2" => return UrgencyLevel::High,
                "low" | "p4" | "p5" => return UrgencyLevel::Low,
                _ => {}
            }
        }

        // Check content for urgency keywords
        if content_lower.contains("urgent") || content_lower.contains("asap") ||
           content_lower.contains("critical") || content_lower.contains("emergency") {
            UrgencyLevel::Critical
        } else if content_lower.contains("important") || content_lower.contains("priority") {
            UrgencyLevel::High
        } else {
            UrgencyLevel::Medium
        }
    }

    /// Compute sentiment using embedding-based analysis, falling back to keyword-based
    async fn compute_sentiment_for_classification(&self, content: &Option<String>) -> f32 {
        let text = match content {
            Some(t) if !t.is_empty() => t,
            _ => return 0.0,
        };
        if let Some(ref store) = self.entity_embedding_store {
            match store.compute_sentiment(text).await {
                Ok(score) => return score,
                Err(e) => {
                    tracing::debug!("Embedding sentiment failed, using keyword fallback: {}", e);
                }
            }
        }
        Self::keyword_sentiment(content)
    }

    /// Keyword-based sentiment analysis (fallback when embedding provider is unavailable)
    fn keyword_sentiment(content: &Option<String>) -> f32 {
        let text = match content {
            Some(t) if !t.is_empty() => t.to_lowercase(),
            _ => return 0.0,
        };

        // Positive indicators
        let positive_words = [
            "great", "thanks", "thank", "good", "excellent", "awesome", "perfect",
            "wonderful", "amazing", "helpful", "appreciate", "well done", "nice",
            "love", "happy", "pleased", "fantastic", "brilliant", "superb",
        ];

        // Negative indicators
        let negative_words = [
            "problem", "issue", "bug", "error", "broken", "fail", "wrong",
            "bad", "terrible", "awful", "frustrated", "annoyed", "disappointed",
            "stuck", "blocked", "confused", "difficult", "impossible", "hate",
        ];

        let mut score: f32 = 0.0;

        for word in &positive_words {
            if text.contains(word) {
                score += 0.3;
            }
        }

        for word in &negative_words {
            if text.contains(word) {
                score -= 0.3;
            }
        }

        // Clamp to [-1, 1] range
        score.clamp(-1.0, 1.0)
    }

    /// Run the learning cycle for an organization
    async fn run_learning_cycle(&self, organization_id: &str) -> Result<()> {
        // Get and clear the buffer
        let interactions = {
            let mut buffer = self.interaction_buffer.write().await;
            buffer.remove(organization_id).unwrap_or_default()
        };

        if interactions.is_empty() {
            return Ok(());
        }

        tracing::info!(
            organization_id = %organization_id,
            interaction_count = interactions.len(),
            "Running learning cycle"
        );

        // Build workflow timelines from recent interactions
        let timelines = self.build_timelines_from_interactions(&interactions)?;

        if !timelines.is_empty() {
            // Learn patterns from timelines
            let patterns = self.pattern_learner.learn_patterns(organization_id, &timelines);

            // Store learned patterns
            for pattern in patterns {
                if pattern.confidence_score >= 0.5 {
                    self.analytics_db.upsert_pattern(&pattern)?;
                    tracing::info!(
                        pattern_name = %pattern.pattern_name,
                        pattern_type = ?pattern.pattern_type,
                        confidence = pattern.confidence_score,
                        occurrences = pattern.occurrence_count,
                        "Learned new pattern"
                    );
                }
            }

            // Apply learning adjustments to existing patterns
            let learning_system = LearningSystem::new(&self.analytics_db);
            let result = learning_system.apply_learning_adjustments(organization_id)?;

            if result.patterns_adjusted > 0 {
                tracing::info!(
                    organization_id = %organization_id,
                    patterns_analyzed = result.patterns_analyzed,
                    patterns_adjusted = result.patterns_adjusted,
                    confidence_increased = result.confidence_increased,
                    confidence_decreased = result.confidence_decreased,
                    "Applied learning adjustments"
                );
            }
        }

        Ok(())
    }

    /// Build workflow timelines from a batch of interactions
    fn build_timelines_from_interactions(&self, interactions: &[InteractionData]) -> Result<Vec<WorkflowTimeline>> {
        let mut timelines: HashMap<String, WorkflowTimeline> = HashMap::new();

        for interaction in interactions {
            // Group by task or goal
            let key = match interaction.entity_type.as_str() {
                "tasks" => format!("task:{}", interaction.entity_id),
                "goals" => format!("goal:{}", interaction.entity_id),
                "task_comments" => {
                    if let Some(task_id) = interaction.metadata.get("task_id").and_then(|v| v.as_str()) {
                        format!("task:{}", task_id)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            let org_id = interaction.organization_id
                .map(|u| u.to_string())
                .unwrap_or_else(|| "_default".to_string());

            let timeline = timelines.entry(key.clone()).or_insert_with(|| {
                let (entity_type, entity_id) = key.split_once(':').unwrap_or(("unknown", &key));
                WorkflowTimeline {
                    id: Uuid::new_v4(),
                    organization_id: org_id.clone(),
                    entity_type: entity_type.to_string(),
                    entity_id: entity_id.to_string(),
                    total_interactions: 0,
                    total_participants: 0,
                    total_duration_hours: None,
                    phases: Vec::new(),
                    key_events: Vec::new(),
                    bottlenecks: Vec::new(),
                    status: "in_progress".to_string(),
                    opened_at: interaction.timestamp,
                    closed_at: None,
                    last_analyzed_at: Utc::now(),
                }
            });

            // Update timeline stats
            timeline.total_interactions += 1;

            // Track participants
            if interaction.actor_id.is_some() {
                // This is simplified - in practice we'd track unique participants
                timeline.total_participants = (timeline.total_participants + 1).min(10);
            }

            // Check for status changes
            if interaction.change_type == "update" {
                if let Some(status) = interaction.metadata.get("status").and_then(|v| v.as_str()) {
                    if status == "completed" || status == "done" {
                        timeline.status = "completed".to_string();
                        timeline.closed_at = Some(interaction.timestamp);

                        // Calculate duration
                        let duration = interaction.timestamp - timeline.opened_at;
                        timeline.total_duration_hours = Some(duration.num_hours() as f64);
                    }
                }
            }
        }

        Ok(timelines.into_values().collect())
    }

    /// Force a learning cycle for an organization (useful for testing or manual triggers)
    pub async fn force_learning_cycle(&self, organization_id: &str) -> Result<()> {
        self.run_learning_cycle(organization_id).await
    }

    /// Get learning statistics
    pub async fn get_stats(&self) -> LearnerStats {
        let buffer = self.interaction_buffer.read().await;
        let total_buffered: usize = buffer.values().map(|v| v.len()).sum();
        let orgs_with_data = buffer.len();

        LearnerStats {
            organizations_tracked: orgs_with_data,
            total_buffered_interactions: total_buffered,
            batch_size: self.batch_size,
            pool_status: self.pool.status(),
        }
    }
}

/// Statistics about the learner
#[derive(Debug, Clone)]
pub struct LearnerStats {
    pub organizations_tracked: usize,
    pub total_buffered_interactions: usize,
    pub batch_size: usize,
    pub pool_status: super::pool::PoolStatus,
}
