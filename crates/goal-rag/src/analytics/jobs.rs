//! Async job processing for analytics
//!
//! Handles background processing of analysis tasks including:
//! - Fetching interactions from PostgreSQL
//! - Classifying interactions with LLM
//! - Building timelines
//! - Matching patterns
//! - Generating recommendations

use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use super::classifier::HybridClassifier;
use super::pattern_learner::PatternLearner;
use super::recommender::Recommender;
use super::storage::AnalyticsDb;
use super::timeline::{ActivityEvent, ReconstructParams, TimelineReconstructor};
use super::types::*;
use crate::error::Result;
use crate::providers::interaction_classifier::InteractionClassifier;

/// Analytics job processor
pub struct AnalyticsJobProcessor {
    db: Arc<AnalyticsDb>,
    classifier: Arc<dyn InteractionClassifier>,
    timeline_builder: TimelineReconstructor,
    pattern_learner: PatternLearner,
    recommender: Recommender,
}

impl AnalyticsJobProcessor {
    /// Create a new job processor
    pub fn new(
        db: Arc<AnalyticsDb>,
        classifier: Arc<dyn InteractionClassifier>,
    ) -> Self {
        Self {
            db,
            classifier,
            timeline_builder: TimelineReconstructor::new(),
            pattern_learner: PatternLearner::default(),
            recommender: Recommender::basic(),
        }
    }

    /// Create with default Ollama classifier (slower, higher quality)
    pub fn with_ollama(db: Arc<AnalyticsDb>, ollama_url: &str, model: &str) -> Self {
        let classifier = Arc::new(HybridClassifier::new(ollama_url, model));
        Self::new(db, classifier)
    }

    /// Create with fast rule-based classifier (instant, keyword-based)
    pub fn with_rule_based(db: Arc<AnalyticsDb>) -> Self {
        use super::classifier::RuleBasedClassifier;
        let classifier = Arc::new(RuleBasedClassifier::new());
        Self::new(db, classifier)
    }

    /// Process an analysis job for a task
    pub async fn process_task_analysis(
        &self,
        job: &mut AnalysisJob,
        task_data: TaskAnalysisInput,
    ) -> Result<AnalysisResult> {
        // Update job status
        job.status = AnalysisJobStatus::FetchingData;
        job.current_stage = "fetching_data".to_string();
        job.updated_at = Utc::now();
        self.db.update_analysis_job(job)?;

        // Step 1: Collect all interactions
        let interactions = self.collect_task_interactions(&task_data);
        job.interactions_found = interactions.len() as u32;
        job.progress_percent = 20;
        self.db.update_analysis_job(job)?;

        // Clear old classifications for this task so re-analysis produces fresh results
        let deleted = self.db.delete_classifications_for_task(&job.entity_id)?;
        if deleted > 0 {
            tracing::info!(job_id = %job.id, deleted = deleted, "Cleared old classifications for re-analysis");
        }

        // Step 2: Classify interactions (skip if none found)
        let classifications = if interactions.is_empty() {
            tracing::info!(job_id = %job.id, "No interactions to classify, skipping classification step");
            job.status = AnalysisJobStatus::Classifying;
            job.current_stage = "classifying".to_string();
            job.interactions_classified = 0;
            job.progress_percent = 50;
            self.db.update_analysis_job(job)?;
            vec![]
        } else {
            job.status = AnalysisJobStatus::Classifying;
            job.current_stage = "classifying".to_string();
            self.db.update_analysis_job(job)?;

            let result = self.classify_interactions(
                &job.organization_id,
                &interactions,
                Some(&task_data.task_title),
                task_data.goal_title.as_deref(),
            ).await?;

            job.interactions_classified = result.len() as u32;
            job.progress_percent = 50;
            self.db.update_analysis_job(job)?;

            // Store classifications
            for classification in &result {
                self.db.insert_classification(classification)?;
            }

            result
        };

        // Step 3: Build timeline
        job.status = AnalysisJobStatus::BuildingTimeline;
        job.current_stage = "building_timeline".to_string();
        job.progress_percent = 60;
        self.db.update_analysis_job(job)?;

        let timeline = self.timeline_builder.reconstruct(ReconstructParams {
            organization_id: &job.organization_id,
            entity_type: "task",
            entity_id: &job.entity_id,
            classifications: &classifications,
            entity_status: &task_data.status,
            opened_at: task_data.created_at,
            closed_at: task_data.completed_at,
        });

        self.db.upsert_timeline(&timeline)?;

        // Step 4: Match patterns
        job.status = AnalysisJobStatus::MatchingPatterns;
        job.current_stage = "matching_patterns".to_string();
        job.progress_percent = 70;
        self.db.update_analysis_job(job)?;

        let patterns = self.db.get_patterns(&job.organization_id)?;
        job.patterns_matched = patterns.len() as u32;
        self.db.update_analysis_job(job)?;

        // Step 5: Generate recommendations
        job.status = AnalysisJobStatus::GeneratingRecommendations;
        job.current_stage = "generating_recommendations".to_string();
        job.progress_percent = 85;
        self.db.update_analysis_job(job)?;

        let recommendations = self.recommender.generate_for_timeline(&timeline, &patterns);
        job.recommendations_generated = recommendations.len() as u32;

        for rec in &recommendations {
            self.db.insert_recommendation(rec)?;
        }

        // Complete job
        job.status = AnalysisJobStatus::Complete;
        job.current_stage = "complete".to_string();
        job.progress_percent = 100;
        job.completed_at = Some(Utc::now());
        job.updated_at = Utc::now();
        self.db.update_analysis_job(job)?;

        Ok(AnalysisResult {
            job_id: job.id,
            timeline,
            classifications,
            recommendations,
            patterns_matched: patterns,
        })
    }

    /// Collect all interactions for a task
    fn collect_task_interactions(&self, task_data: &TaskAnalysisInput) -> Vec<RawInteraction> {
        let mut interactions = Vec::new();

        // Add task comments
        for comment in &task_data.comments {
            interactions.push(RawInteraction {
                source_type: InteractionSource::TaskComment,
                source_id: comment.id.clone(),
                task_id: Some(task_data.task_id.clone()),
                goal_id: task_data.goal_id.clone(),
                sender_id: comment.author_id.clone(),
                content: comment.content.clone(),
                created_at: comment.created_at,
            });
        }

        // Add related messages
        for message in &task_data.related_messages {
            interactions.push(RawInteraction {
                source_type: InteractionSource::Message,
                source_id: message.id.clone(),
                task_id: Some(task_data.task_id.clone()),
                goal_id: task_data.goal_id.clone(),
                sender_id: message.sender_id.clone(),
                content: message.content.clone(),
                created_at: message.created_at,
            });
        }

        // Add activity events (task actions like assignments, status changes, etc.)
        for event in &task_data.activity_events {
            interactions.push(RawInteraction {
                source_type: InteractionSource::ActivityLog,
                source_id: event.id.clone(),
                task_id: Some(task_data.task_id.clone()),
                goal_id: task_data.goal_id.clone(),
                sender_id: event.actor_id.clone(),
                content: synthesize_activity_content(event),
                created_at: event.timestamp,
            });
        }

        // Sort by timestamp
        interactions.sort_by_key(|i| i.created_at);

        interactions
    }

    /// Classify a batch of interactions
    async fn classify_interactions(
        &self,
        organization_id: &str,
        interactions: &[RawInteraction],
        task_title: Option<&str>,
        goal_title: Option<&str>,
    ) -> Result<Vec<InteractionClassification>> {
        let context = ClassificationContext {
            task_title: task_title.map(String::from),
            goal_title: goal_title.map(String::from),
            sender_name: None,
            thread_history: vec![],
        };

        let batch: Vec<(String, InteractionSource)> = interactions
            .iter()
            .map(|i| (i.content.clone(), i.source_type.clone()))
            .collect();

        tracing::info!(
            classifier = self.classifier.name(),
            batch_size = batch.len(),
            "Classifying interactions"
        );

        let results = self.classifier.classify_batch(&batch, Some(&context)).await?;

        tracing::info!(
            results_count = results.len(),
            "Classification complete"
        );

        let mut classifications = Vec::new();

        for (interaction, result) in interactions.iter().zip(results.into_iter()) {
            classifications.push(InteractionClassification {
                id: Uuid::new_v4(),
                organization_id: organization_id.to_string(),
                source_type: interaction.source_type.clone(),
                source_id: interaction.source_id.clone(),
                task_id: interaction.task_id.clone(),
                goal_id: interaction.goal_id.clone(),
                sender_id: interaction.sender_id.clone(),
                content: interaction.content.clone(),
                interaction_type: result.primary_type,
                secondary_types: result.secondary_types,
                confidence_score: result.confidence,
                entities: result.entities,
                sentiment: result.sentiment,
                urgency_level: result.urgency,
                references_interaction_id: None,
                original_created_at: interaction.created_at,
                classified_at: Utc::now(),
            });
        }

        Ok(classifications)
    }

    /// Learn patterns from completed tasks (batch operation)
    pub async fn learn_patterns_batch(
        &self,
        organization_id: &str,
    ) -> Result<Vec<WorkflowPattern>> {
        // Get all completed timelines for learning
        // TODO: Implement get_completed_timelines in storage
        let timelines: Vec<WorkflowTimeline> = vec![];

        if timelines.is_empty() {
            return Ok(vec![]);
        }

        let patterns = self.pattern_learner.learn_patterns(organization_id, &timelines);

        // Store learned patterns
        for pattern in &patterns {
            self.db.upsert_pattern(pattern)?;
        }

        Ok(patterns)
    }

    /// Generate org-wide recommendations
    pub async fn generate_org_recommendations(
        &self,
        organization_id: &str,
    ) -> Result<Vec<EfficiencyRecommendation>> {
        let patterns = self.db.get_patterns(organization_id)?;

        // TODO: Get recent timelines from storage
        let recent_timelines: Vec<WorkflowTimeline> = vec![];

        let recommendations = self.recommender.generate_org_recommendations(
            organization_id,
            &patterns,
            &recent_timelines,
        );

        for rec in &recommendations {
            self.db.insert_recommendation(rec)?;
        }

        Ok(recommendations)
    }
}

/// Raw interaction before classification
#[derive(Debug, Clone)]
pub struct RawInteraction {
    pub source_type: InteractionSource,
    pub source_id: String,
    pub task_id: Option<String>,
    pub goal_id: Option<String>,
    pub sender_id: String,
    pub content: String,
    pub created_at: chrono::DateTime<Utc>,
}

/// Input data for task analysis
#[derive(Debug, Clone)]
pub struct TaskAnalysisInput {
    pub task_id: String,
    pub task_title: String,
    pub goal_id: Option<String>,
    pub goal_title: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub comments: Vec<TaskComment>,
    pub related_messages: Vec<RelatedMessage>,
    pub activity_events: Vec<ActivityEvent>,
}

/// Task comment data
#[derive(Debug, Clone)]
pub struct TaskComment {
    pub id: String,
    pub author_id: String,
    pub content: String,
    pub created_at: chrono::DateTime<Utc>,
}

/// Related message data
#[derive(Debug, Clone)]
pub struct RelatedMessage {
    pub id: String,
    pub sender_id: String,
    pub content: String,
    pub created_at: chrono::DateTime<Utc>,
}

/// Result of analysis job
#[derive(Debug)]
pub struct AnalysisResult {
    pub job_id: Uuid,
    pub timeline: WorkflowTimeline,
    pub classifications: Vec<InteractionClassification>,
    pub recommendations: Vec<EfficiencyRecommendation>,
    pub patterns_matched: Vec<WorkflowPattern>,
}

/// Synthesize human-readable content from an activity event for classification
fn synthesize_activity_content(event: &ActivityEvent) -> String {
    let actor = event.actor_name.as_deref().unwrap_or("Someone");
    let changes = event.changes.as_ref();

    match event.action.as_str() {
        "assigned" | "assignment_changed" => {
            if let Some(assignee) = changes.and_then(|c| c.get("assignee")).and_then(|v| v.as_str()) {
                format!("Task assigned to {}", assignee)
            } else {
                format!("{} assigned the task", actor)
            }
        }
        "status_changed" => {
            let from = changes.and_then(|c| c.get("from")).and_then(|v| v.as_str()).unwrap_or("unknown");
            let to = changes.and_then(|c| c.get("to")).and_then(|v| v.as_str()).unwrap_or("unknown");
            format!("Status updated from {} to {}", from, to)
        }
        "started" | "in_progress" => format!("{} started working on the task", actor),
        "completed" | "closed" | "done" => format!("Task completed by {}", actor),
        "blocked" => format!("Task blocked by {}", actor),
        "unblocked" | "resolved" => format!("Task unblocked by {}", actor),
        "priority_changed" => {
            let from = changes.and_then(|c| c.get("from")).and_then(|v| v.as_str()).unwrap_or("unknown");
            let to = changes.and_then(|c| c.get("to")).and_then(|v| v.as_str()).unwrap_or("unknown");
            format!("Priority changed from {} to {}", from, to)
        }
        "submitted" | "review_requested" => format!("{} submitted the task for review", actor),
        "approved" | "sign_off" => format!("Task approved by {}", actor),
        "escalated" => format!("Task escalated by {}", actor),
        other => {
            if let Some(c) = changes {
                format!("Activity: {} - {}", other, serde_json::to_string(c).unwrap_or_default())
            } else {
                format!("Activity: {}", other)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would go here
}
