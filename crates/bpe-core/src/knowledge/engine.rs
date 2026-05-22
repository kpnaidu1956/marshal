use crate::audit::logger::AuditLogger;
use crate::db::PgPool;
use crate::error::BpeError;
use crate::workflow::engine::WorkflowEngine;
use super::models::*;
use uuid::Uuid;

pub struct KnowledgeEngine;

impl KnowledgeEngine {
    /// Learn a step sequence from a completed workflow execution.
    pub async fn learn_from_execution(
        pool: &PgPool,
        org_id: Uuid,
        req: &LearnFromExecutionRequest,
    ) -> Result<LearnedSequence, BpeError> {
        // Verify execution exists and is completed
        let execution = WorkflowEngine::get_execution(pool, req.execution_id).await?;
        if execution.status != "completed" {
            return Err(BpeError::BadRequest(format!(
                "Cannot learn from execution in '{}' status — must be completed",
                execution.status
            )));
        }

        // Get the steps from the execution
        let steps = WorkflowEngine::get_steps(pool, req.execution_id).await?;
        if steps.is_empty() {
            return Err(BpeError::BadRequest("Execution has no steps to learn from".into()));
        }

        // Build the learned step sequence
        let learned_steps: Vec<serde_json::Value> = steps.iter().map(|s| {
            serde_json::json!({
                "step_order": s.step_order,
                "name": s.name,
                "description": s.description,
                "step_type": s.step_type,
                "dependencies": s.dependencies,
                "estimated_duration_minutes": s.estimated_duration_minutes,
                "actual_duration_minutes": s.actual_duration_minutes,
                "integration_type": s.integration_type,
            })
        }).collect();

        let steps_json = serde_json::to_value(&learned_steps)
            .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?;

        // Calculate average completion time from actual step durations
        let total_minutes: f64 = steps.iter()
            .filter_map(|s| s.actual_duration_minutes)
            .map(|m| m as f64)
            .sum();
        let avg_minutes = if total_minutes > 0.0 { Some(total_minutes) } else { None };

        // Build embedding text for future semantic search
        let embedding_text = format!(
            "{} {} {}",
            req.task_category,
            req.entity_type_names.as_ref().map(|v| v.join(" ")).unwrap_or_default(),
            steps.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" "),
        );

        let entity_types = req.entity_type_names.clone().unwrap_or_default();

        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.learned_sequences
                    (organization_id, task_category, entity_type_names, steps,
                     source_execution_id, avg_completion_minutes, embedding_text)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, organization_id, task_category, entity_type_names, steps,
                           source_execution_id, times_suggested, times_accepted,
                           times_modified, times_rejected, avg_completion_minutes,
                           embedding_text, version, superseded_by, is_active,
                           created_at, updated_at",
                &[
                    &org_id, &req.task_category, &entity_types, &steps_json,
                    &req.execution_id, &avg_minutes, &embedding_text,
                ],
            )
            .await?;

        let seq = row_to_sequence(&row);

        // Also mark the source workflow definition as learned
        if let Some(def_id) = execution.definition_id {
            let _ = client
                .execute(
                    "UPDATE bpe.workflow_definitions SET is_learned = true WHERE id = $1",
                    &[&def_id],
                )
                .await;
        }

        if let Err(e) = AuditLogger::log_change(
            pool, org_id, "knowledge.learned", "learned_sequence", seq.id,
            None, None, None,
            serde_json::json!({
                "task_category": req.task_category,
                "execution_id": req.execution_id,
                "step_count": learned_steps.len(),
            }),
        ).await {
            tracing::warn!("Audit log failed for knowledge.learned: {e}");
        }

        Ok(seq)
    }

    /// Learn a step sequence from a completed goal's tasks.
    /// This creates a LearnedSequence directly from task data,
    /// without requiring a BPE workflow execution.
    pub async fn learn_from_goal(
        pool: &PgPool,
        org_id: Uuid,
        req: &LearnFromGoalRequest,
    ) -> Result<LearnedSequence, BpeError> {
        if req.tasks.is_empty() {
            return Err(BpeError::BadRequest("No tasks to learn from".into()));
        }

        // Build step sequence from tasks
        let learned_steps: Vec<serde_json::Value> = req.tasks.iter().enumerate().map(|(i, t)| {
            serde_json::json!({
                "step_order": if t.sequence_order > 0 { t.sequence_order } else { (i + 1) as i32 },
                "name": t.title,
                "description": t.description,
                "step_type": "manual",
                "priority": t.priority,
                "status": t.status,
            })
        }).collect();

        let steps_json = serde_json::to_value(&learned_steps)
            .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?;

        // Build embedding text for semantic search
        let embedding_text = format!(
            "{} {} {}",
            req.task_category,
            req.goal_title,
            req.tasks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>().join(" "),
        );

        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.learned_sequences
                    (organization_id, task_category, entity_type_names, steps,
                     avg_completion_minutes, embedding_text)
                 VALUES ($1, $2, '{}', $3, NULL, $4)
                 RETURNING id, organization_id, task_category, entity_type_names, steps,
                           source_execution_id, times_suggested, times_accepted,
                           times_modified, times_rejected, avg_completion_minutes,
                           embedding_text, version, superseded_by, is_active,
                           created_at, updated_at",
                &[&org_id, &req.task_category, &steps_json, &embedding_text],
            )
            .await?;

        let seq = row_to_sequence(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, org_id, "knowledge.learned_from_goal", "learned_sequence", seq.id,
            None, None, None,
            serde_json::json!({
                "task_category": req.task_category,
                "goal_id": req.goal_id,
                "goal_title": req.goal_title,
                "step_count": learned_steps.len(),
            }),
        ).await {
            tracing::warn!("Audit log failed for knowledge.learned_from_goal: {e}");
        }

        Ok(seq)
    }

    /// Suggest learned sequences for a given task category or prompt.
    pub async fn suggest(
        pool: &PgPool,
        org_id: Uuid,
        req: &SuggestRequest,
    ) -> Result<Vec<SequenceSuggestion>, BpeError> {
        let limit = req.limit.unwrap_or(5).min(20);
        let client = pool.get().await?;

        let rows = client
            .query(
                "SELECT id, task_category, steps, times_suggested, times_accepted,
                        times_modified, times_rejected, avg_completion_minutes, version
                 FROM bpe.learned_sequences
                 WHERE organization_id = $1
                   AND is_active = true
                   AND superseded_by IS NULL
                   AND ($2::text IS NULL OR task_category = $2)
                   AND ($3::text IS NULL OR $3 = ANY(entity_type_names))
                 ORDER BY
                   CASE WHEN times_suggested > 0
                        THEN times_accepted::float / times_suggested
                        ELSE 0.5 END DESC,
                   times_accepted DESC,
                   created_at DESC
                 LIMIT $4",
                &[&org_id, &req.task_category, &req.entity_type_name, &limit],
            )
            .await?;

        // If a prompt is provided, do basic keyword matching for scoring
        let prompt_words: Vec<String> = req.prompt.as_ref()
            .map(|p| p.to_lowercase().split_whitespace().map(String::from).collect())
            .unwrap_or_default();

        let suggestions: Vec<SequenceSuggestion> = rows.iter().enumerate().map(|(i, row)| {
            let id: Uuid = row.get("id");
            let task_category: String = row.get("task_category");
            let steps: serde_json::Value = row.get("steps");
            let suggested: i32 = row.get("times_suggested");
            let accepted: i32 = row.get("times_accepted");
            let avg_minutes: Option<f64> = row.get("avg_completion_minutes");
            let version: i32 = row.get("version");

            let acceptance_rate = if suggested > 0 {
                accepted as f64 / suggested as f64
            } else {
                0.5
            };

            // Base score from acceptance rate and position
            let mut score = acceptance_rate * 0.7 + (1.0 - i as f64 / limit as f64) * 0.3;

            // Boost by keyword match if prompt given
            if !prompt_words.is_empty() {
                let cat_lower = task_category.to_lowercase();
                let step_text = steps.to_string().to_lowercase();
                let matches = prompt_words.iter()
                    .filter(|w| cat_lower.contains(w.as_str()) || step_text.contains(w.as_str()))
                    .count();
                let match_ratio = matches as f64 / prompt_words.len() as f64;
                score = score * 0.6 + match_ratio * 0.4;
            }

            SequenceSuggestion {
                id,
                task_category,
                steps,
                acceptance_rate,
                avg_completion_minutes: avg_minutes,
                version,
                score,
            }
        }).collect();

        // Record that these were suggested
        for s in &suggestions {
            let _ = client
                .execute(
                    "UPDATE bpe.learned_sequences SET times_suggested = times_suggested + 1, updated_at = now() WHERE id = $1",
                    &[&s.id],
                )
                .await;
        }

        Ok(suggestions)
    }

    /// Record feedback on a suggestion (accepted, modified, rejected).
    pub async fn record_feedback(
        pool: &PgPool,
        sequence_id: Uuid,
        outcome: &str,
    ) -> Result<LearnedSequence, BpeError> {
        let col = match outcome {
            "accepted" => "times_accepted",
            "modified" => "times_modified",
            "rejected" => "times_rejected",
            other => return Err(BpeError::BadRequest(format!(
                "Invalid outcome: '{}'. Must be accepted, modified, or rejected", other
            ))),
        };

        let client = pool.get().await?;
        // Use separate branches for each column to avoid dynamic SQL
        let row = match outcome {
            "accepted" => client.query_one(
                "UPDATE bpe.learned_sequences SET times_accepted = times_accepted + 1, updated_at = now()
                 WHERE id = $1
                 RETURNING id, organization_id, task_category, entity_type_names, steps,
                           source_execution_id, times_suggested, times_accepted,
                           times_modified, times_rejected, avg_completion_minutes,
                           embedding_text, version, superseded_by, is_active,
                           created_at, updated_at",
                &[&sequence_id],
            ).await,
            "modified" => client.query_one(
                "UPDATE bpe.learned_sequences SET times_modified = times_modified + 1, updated_at = now()
                 WHERE id = $1
                 RETURNING id, organization_id, task_category, entity_type_names, steps,
                           source_execution_id, times_suggested, times_accepted,
                           times_modified, times_rejected, avg_completion_minutes,
                           embedding_text, version, superseded_by, is_active,
                           created_at, updated_at",
                &[&sequence_id],
            ).await,
            _ => client.query_one(
                "UPDATE bpe.learned_sequences SET times_rejected = times_rejected + 1, updated_at = now()
                 WHERE id = $1
                 RETURNING id, organization_id, task_category, entity_type_names, steps,
                           source_execution_id, times_suggested, times_accepted,
                           times_modified, times_rejected, avg_completion_minutes,
                           embedding_text, version, superseded_by, is_active,
                           created_at, updated_at",
                &[&sequence_id],
            ).await,
        };

        let row = row.map_err(|_| BpeError::NotFound(format!("Learned sequence {sequence_id} not found")))?;
        let _ = col; // suppress warning

        Ok(row_to_sequence(&row))
    }

    /// List learned sequences for an organization.
    pub async fn list(
        pool: &PgPool,
        org_id: Uuid,
        category: Option<&str>,
        active_only: bool,
    ) -> Result<Vec<LearnedSequence>, BpeError> {
        let client = pool.get().await?;
        let rows = if active_only {
            client.query(
                "SELECT id, organization_id, task_category, entity_type_names, steps,
                        source_execution_id, times_suggested, times_accepted,
                        times_modified, times_rejected, avg_completion_minutes,
                        embedding_text, version, superseded_by, is_active,
                        created_at, updated_at
                 FROM bpe.learned_sequences
                 WHERE organization_id = $1 AND is_active = true AND superseded_by IS NULL
                   AND ($2::text IS NULL OR task_category = $2)
                 ORDER BY times_accepted DESC, created_at DESC",
                &[&org_id, &category],
            ).await?
        } else {
            client.query(
                "SELECT id, organization_id, task_category, entity_type_names, steps,
                        source_execution_id, times_suggested, times_accepted,
                        times_modified, times_rejected, avg_completion_minutes,
                        embedding_text, version, superseded_by, is_active,
                        created_at, updated_at
                 FROM bpe.learned_sequences
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR task_category = $2)
                 ORDER BY created_at DESC",
                &[&org_id, &category],
            ).await?
        };

        Ok(rows.iter().map(row_to_sequence).collect())
    }

    /// Get a single learned sequence.
    pub async fn get(pool: &PgPool, id: Uuid) -> Result<LearnedSequence, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, task_category, entity_type_names, steps,
                        source_execution_id, times_suggested, times_accepted,
                        times_modified, times_rejected, avg_completion_minutes,
                        embedding_text, version, superseded_by, is_active,
                        created_at, updated_at
                 FROM bpe.learned_sequences WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Learned sequence {id} not found")))?;

        Ok(row_to_sequence(&row))
    }

    /// Deactivate a learned sequence.
    pub async fn deactivate(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute(
                "UPDATE bpe.learned_sequences SET is_active = false, updated_at = now() WHERE id = $1",
                &[&id],
            )
            .await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Learned sequence {id} not found")));
        }
        Ok(())
    }

    /// Promote a learned sequence into a reusable workflow definition.
    pub async fn promote_to_definition(
        pool: &PgPool,
        sequence_id: Uuid,
        org_id: Uuid,
        name: &str,
        description: Option<&str>,
        category: Option<&str>,
    ) -> Result<crate::workflow::models::WorkflowDefinition, BpeError> {
        let seq = Self::get(pool, sequence_id).await?;

        // Convert learned steps into step_templates format
        let step_templates: Vec<crate::workflow::models::StepTemplate> = seq.steps
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|s| crate::workflow::models::StepTemplate {
                name: s["name"].as_str().unwrap_or("Step").to_string(),
                description: s["description"].as_str().map(String::from),
                step_type: s["step_type"].as_str().unwrap_or("manual").to_string(),
                dependencies: s["dependencies"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_i64().map(|n| n as i32)).collect())
                    .unwrap_or_default(),
                estimated_duration_minutes: s["actual_duration_minutes"].as_i64()
                    .or(s["estimated_duration_minutes"].as_i64())
                    .map(|n| n as i32),
                integration_type: s["integration_type"].as_str().map(String::from),
                integration_config: None,
                assigned_role: None,
            })
            .collect();

        let cat = category.unwrap_or(&seq.task_category);
        let step_json = serde_json::to_value(&step_templates)
            .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?;

        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.workflow_definitions
                    (organization_id, name, description, category, step_templates,
                     source, is_learned)
                 VALUES ($1, $2, $3, $4, $5, 'learned', true)
                 RETURNING id, organization_id, name, description, category, step_templates,
                           is_learned, source, version, is_active, times_used,
                           avg_completion_minutes, success_rate, created_at, updated_at, created_by",
                &[&org_id, &name, &description, &cat, &step_json],
            )
            .await?;

        // Parse the returned row into WorkflowDefinition
        let step_templates_json: serde_json::Value = row.get("step_templates");
        let def = crate::workflow::models::WorkflowDefinition {
            id: row.get("id"),
            organization_id: row.get("organization_id"),
            name: row.get("name"),
            description: row.get("description"),
            category: row.get("category"),
            step_templates: serde_json::from_value(step_templates_json).unwrap_or_default(),
            is_learned: row.get("is_learned"),
            source: row.get("source"),
            version: row.get("version"),
            is_active: row.get("is_active"),
            times_used: row.get("times_used"),
            avg_completion_minutes: row.get("avg_completion_minutes"),
            success_rate: row.get("success_rate"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            created_by: row.get("created_by"),
        };

        if let Err(e) = AuditLogger::log_change(
            pool, org_id, "knowledge.promoted", "learned_sequence", sequence_id,
            None, None, None,
            serde_json::json!({
                "definition_id": def.id,
                "definition_name": name,
            }),
        ).await {
            tracing::warn!("Audit log failed for knowledge.promoted: {e}");
        }

        Ok(def)
    }
}

// ---- Row converter ----

fn row_to_sequence(row: &tokio_postgres::Row) -> LearnedSequence {
    LearnedSequence {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        task_category: row.get("task_category"),
        entity_type_names: row.get("entity_type_names"),
        steps: row.get("steps"),
        source_execution_id: row.get("source_execution_id"),
        times_suggested: row.get("times_suggested"),
        times_accepted: row.get("times_accepted"),
        times_modified: row.get("times_modified"),
        times_rejected: row.get("times_rejected"),
        avg_completion_minutes: row.get("avg_completion_minutes"),
        embedding_text: row.get("embedding_text"),
        version: row.get("version"),
        superseded_by: row.get("superseded_by"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
