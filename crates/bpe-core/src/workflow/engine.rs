use crate::audit::logger::AuditLogger;
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use super::state_machine::StepStateMachine;
use uuid::Uuid;

/// Core workflow execution engine.
pub struct WorkflowEngine;

impl WorkflowEngine {
    // ---- Definition CRUD ----

    pub async fn create_definition(
        pool: &PgPool,
        org_id: Uuid,
        req: &CreateDefinitionRequest,
    ) -> Result<WorkflowDefinition, BpeError> {
        let client = pool.get().await?;
        let step_templates_json = serde_json::to_value(&req.step_templates)
            .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?;
        let category = req.category.as_deref().unwrap_or("general");
        let source = req.source.as_deref().unwrap_or("manual");

        let row = client
            .query_one(
                "INSERT INTO bpe.workflow_definitions
                    (organization_id, name, description, category, step_templates, source)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id, organization_id, name, description, category, step_templates,
                           is_learned, source, version, is_active, times_used,
                           avg_completion_minutes, success_rate, created_at, updated_at, created_by",
                &[&org_id, &req.name, &req.description, &category, &step_templates_json, &source],
            )
            .await?;

        Ok(row_to_definition(&row))
    }

    pub async fn list_definitions(
        pool: &PgPool,
        org_id: Uuid,
        category: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedDefinitions, BpeError> {
        let client = pool.get().await?;
        let offset = (page - 1) * per_page;

        let count_row = client
            .query_one(
                "SELECT count(*) FROM bpe.workflow_definitions
                 WHERE organization_id = $1 AND is_active = true
                   AND ($2::text IS NULL OR category = $2)",
                &[&org_id, &category],
            )
            .await?;
        let total: i64 = count_row.get(0);

        let rows = client
            .query(
                "SELECT id, organization_id, name, description, category, step_templates,
                        is_learned, source, version, is_active, times_used,
                        avg_completion_minutes, success_rate, created_at, updated_at, created_by
                 FROM bpe.workflow_definitions
                 WHERE organization_id = $1 AND is_active = true
                   AND ($2::text IS NULL OR category = $2)
                 ORDER BY name
                 LIMIT $3 OFFSET $4",
                &[&org_id, &category, &per_page, &offset],
            )
            .await?;

        let data = rows.iter().map(row_to_definition).collect();
        Ok(PaginatedDefinitions { data, page, per_page, total })
    }

    pub async fn get_definition(pool: &PgPool, id: Uuid) -> Result<WorkflowDefinition, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, name, description, category, step_templates,
                        is_learned, source, version, is_active, times_used,
                        avg_completion_minutes, success_rate, created_at, updated_at, created_by
                 FROM bpe.workflow_definitions WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Workflow definition {id} not found")))?;

        Ok(row_to_definition(&row))
    }

    pub async fn update_definition(
        pool: &PgPool,
        id: Uuid,
        req: &UpdateDefinitionRequest,
    ) -> Result<WorkflowDefinition, BpeError> {
        let existing = Self::get_definition(pool, id).await?;

        let name = req.name.as_deref().unwrap_or(&existing.name);
        let description = req.description.as_ref().or(existing.description.as_ref());
        let category_val = req.category.as_deref().unwrap_or(&existing.category);
        let is_active = req.is_active.unwrap_or(existing.is_active);

        let step_templates_json = if let Some(st) = &req.step_templates {
            serde_json::to_value(st).map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?
        } else {
            serde_json::to_value(&existing.step_templates)
                .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?
        };

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.workflow_definitions
                 SET name=$1, description=$2, category=$3, step_templates=$4,
                     is_active=$5, updated_at=now()
                 WHERE id=$6
                 RETURNING id, organization_id, name, description, category, step_templates,
                           is_learned, source, version, is_active, times_used,
                           avg_completion_minutes, success_rate, created_at, updated_at, created_by",
                &[&name, &description, &category_val, &step_templates_json, &is_active, &id],
            )
            .await?;

        Ok(row_to_definition(&row))
    }

    pub async fn delete_definition(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute("DELETE FROM bpe.workflow_definitions WHERE id = $1", &[&id])
            .await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Workflow definition {id} not found")));
        }
        Ok(())
    }

    // ---- Execution lifecycle ----

    pub async fn create_execution(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
        title: &str,
        description: Option<&str>,
        prompt: Option<&str>,
        definition_id: Option<Uuid>,
        target_entity_id: Option<Uuid>,
    ) -> Result<WorkflowExecution, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.workflow_executions
                    (organization_id, definition_id, title, description, original_prompt,
                     target_entity_id, status, initiated_by, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6, 'draft', $7, '{}'::jsonb)
                 RETURNING id, organization_id, definition_id, title, description, original_prompt,
                           target_entity_id, linked_task_id, linked_goal_id, status,
                           started_at, completed_at, cancelled_at, initiated_by, metadata,
                           created_at, updated_at",
                &[&org_id, &definition_id, &title, &description, &prompt, &target_entity_id, &user_id],
            )
            .await?;

        let execution = row_to_execution(&row);

        // Audit
        let after = serde_json::to_value(&execution).ok();
        if let Err(e) = AuditLogger::log_change(
            pool, org_id, "workflow_execution.created", "workflow_execution", execution.id,
            Some(user_id), None, after.as_ref(), serde_json::json!({}),
        ).await {
            tracing::warn!("Audit log failed for workflow_execution.created: {e}");
        }

        Ok(execution)
    }

    pub async fn get_execution(pool: &PgPool, id: Uuid) -> Result<WorkflowExecution, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, definition_id, title, description, original_prompt,
                        target_entity_id, linked_task_id, linked_goal_id, status,
                        started_at, completed_at, cancelled_at, initiated_by, metadata,
                        created_at, updated_at
                 FROM bpe.workflow_executions WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Workflow execution {id} not found")))?;

        Ok(row_to_execution(&row))
    }

    pub async fn list_executions(
        pool: &PgPool,
        org_id: Uuid,
        query: &ListExecutionsQuery,
    ) -> Result<PaginatedExecutions, BpeError> {
        let client = pool.get().await?;
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        let count_row = client
            .query_one(
                "SELECT count(*) FROM bpe.workflow_executions
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR status = $2)
                   AND ($3::uuid IS NULL OR definition_id = $3)",
                &[&org_id, &query.status, &query.definition_id],
            )
            .await?;
        let total: i64 = count_row.get(0);

        let rows = client
            .query(
                "SELECT id, organization_id, definition_id, title, description, original_prompt,
                        target_entity_id, linked_task_id, linked_goal_id, status,
                        started_at, completed_at, cancelled_at, initiated_by, metadata,
                        created_at, updated_at
                 FROM bpe.workflow_executions
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR status = $2)
                   AND ($3::uuid IS NULL OR definition_id = $3)
                 ORDER BY created_at DESC
                 LIMIT $4 OFFSET $5",
                &[&org_id, &query.status, &query.definition_id, &per_page, &offset],
            )
            .await?;

        let data = rows.iter().map(row_to_execution).collect();
        Ok(PaginatedExecutions { data, page, per_page, total })
    }

    /// Confirm a draft execution, optionally adjusting steps.
    pub async fn confirm(
        pool: &PgPool,
        execution_id: Uuid,
        steps: Option<Vec<ConfirmStep>>,
    ) -> Result<WorkflowExecution, BpeError> {
        let execution = Self::get_execution(pool, execution_id).await?;
        if execution.status != "draft" {
            return Err(BpeError::BadRequest(format!(
                "Cannot confirm execution in '{}' status", execution.status
            )));
        }

        // Use a single connection + transaction for step adjustments and status update
        let mut client = pool.get().await?;
        let txn = client.transaction().await.map_err(|e| {
            BpeError::Database(format!("Failed to start transaction: {e}"))
        })?;

        // Apply step adjustments if provided
        if let Some(step_adjustments) = steps {
            for adj in &step_adjustments {
                match (&adj.name, &adj.assigned_to) {
                    (Some(name), Some(assigned_to)) => {
                        txn.execute(
                            "UPDATE bpe.workflow_steps SET name=$3, assigned_to=$4, updated_at=now() WHERE execution_id=$1 AND step_order=$2",
                            &[&execution_id, &adj.step_order, name, assigned_to],
                        ).await?;
                    }
                    (Some(name), None) => {
                        txn.execute(
                            "UPDATE bpe.workflow_steps SET name=$3, updated_at=now() WHERE execution_id=$1 AND step_order=$2",
                            &[&execution_id, &adj.step_order, name],
                        ).await?;
                    }
                    (None, Some(assigned_to)) => {
                        txn.execute(
                            "UPDATE bpe.workflow_steps SET assigned_to=$3, updated_at=now() WHERE execution_id=$1 AND step_order=$2",
                            &[&execution_id, &adj.step_order, assigned_to],
                        ).await?;
                    }
                    (None, None) => {}
                }
            }
        }

        let row = txn
            .query_one(
                "UPDATE bpe.workflow_executions SET status='confirmed', updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, definition_id, title, description, original_prompt,
                           target_entity_id, linked_task_id, linked_goal_id, status,
                           started_at, completed_at, cancelled_at, initiated_by, metadata,
                           created_at, updated_at",
                &[&execution_id],
            )
            .await?;

        let result = row_to_execution(&row);

        txn.commit().await.map_err(|e| {
            BpeError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        // Audit (outside transaction — non-critical)
        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_execution.confirmed", "workflow_execution",
            execution_id, result.initiated_by, None, None, serde_json::json!({}),
        ).await {
            tracing::warn!("Audit log failed for workflow_execution.confirmed: {e}");
        }

        Ok(result)
    }

    /// Start a confirmed execution: set status to running, mark steps with no dependencies as ready.
    pub async fn start(pool: &PgPool, execution_id: Uuid) -> Result<WorkflowExecution, BpeError> {
        let execution = Self::get_execution(pool, execution_id).await?;
        if execution.status != "confirmed" {
            return Err(BpeError::BadRequest(format!(
                "Cannot start execution in '{}' status", execution.status
            )));
        }

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.workflow_executions SET status='running', started_at=now(), updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, definition_id, title, description, original_prompt,
                           target_entity_id, linked_task_id, linked_goal_id, status,
                           started_at, completed_at, cancelled_at, initiated_by, metadata,
                           created_at, updated_at",
                &[&execution_id],
            )
            .await?;

        // Mark steps with no dependencies (empty array) as ready
        client
            .execute(
                "UPDATE bpe.workflow_steps SET status='ready', updated_at=now()
                 WHERE execution_id=$1 AND status='pending' AND (dependencies IS NULL OR dependencies = '{}')",
                &[&execution_id],
            )
            .await?;

        let result = row_to_execution(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_execution.started", "workflow_execution",
            execution_id, result.initiated_by, None, None, serde_json::json!({}),
        ).await {
            tracing::warn!("Audit log failed for workflow_execution.started: {e}");
        }

        Ok(result)
    }

    pub async fn pause(pool: &PgPool, execution_id: Uuid) -> Result<WorkflowExecution, BpeError> {
        let execution = Self::get_execution(pool, execution_id).await?;
        if execution.status != "running" {
            return Err(BpeError::BadRequest(format!(
                "Cannot pause execution in '{}' status", execution.status
            )));
        }

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.workflow_executions SET status='paused', updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, definition_id, title, description, original_prompt,
                           target_entity_id, linked_task_id, linked_goal_id, status,
                           started_at, completed_at, cancelled_at, initiated_by, metadata,
                           created_at, updated_at",
                &[&execution_id],
            )
            .await?;

        let result = row_to_execution(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_execution.paused", "workflow_execution",
            execution_id, result.initiated_by, None, None, serde_json::json!({}),
        ).await {
            tracing::warn!("Audit log failed for workflow_execution.paused: {e}");
        }

        Ok(result)
    }

    pub async fn resume(pool: &PgPool, execution_id: Uuid) -> Result<WorkflowExecution, BpeError> {
        let execution = Self::get_execution(pool, execution_id).await?;
        if execution.status != "paused" {
            return Err(BpeError::BadRequest(format!(
                "Cannot resume execution in '{}' status", execution.status
            )));
        }

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.workflow_executions SET status='running', updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, definition_id, title, description, original_prompt,
                           target_entity_id, linked_task_id, linked_goal_id, status,
                           started_at, completed_at, cancelled_at, initiated_by, metadata,
                           created_at, updated_at",
                &[&execution_id],
            )
            .await?;

        let result = row_to_execution(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_execution.resumed", "workflow_execution",
            execution_id, result.initiated_by, None, None, serde_json::json!({}),
        ).await {
            tracing::warn!("Audit log failed for workflow_execution.resumed: {e}");
        }

        Ok(result)
    }

    pub async fn cancel(
        pool: &PgPool,
        execution_id: Uuid,
        user_id: Option<Uuid>,
    ) -> Result<WorkflowExecution, BpeError> {
        let execution = Self::get_execution(pool, execution_id).await?;
        if execution.status == "completed" || execution.status == "cancelled" {
            return Err(BpeError::BadRequest(format!(
                "Cannot cancel execution in '{}' status", execution.status
            )));
        }

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.workflow_executions SET status='cancelled', cancelled_at=now(), updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, definition_id, title, description, original_prompt,
                           target_entity_id, linked_task_id, linked_goal_id, status,
                           started_at, completed_at, cancelled_at, initiated_by, metadata,
                           created_at, updated_at",
                &[&execution_id],
            )
            .await?;

        let result = row_to_execution(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_execution.cancelled", "workflow_execution",
            execution_id, user_id, None, None, serde_json::json!({}),
        ).await {
            tracing::warn!("Audit log failed for workflow_execution.cancelled: {e}");
        }

        Ok(result)
    }

    // ---- Step operations ----

    pub async fn get_steps(pool: &PgPool, execution_id: Uuid) -> Result<Vec<WorkflowStep>, BpeError> {
        let client = pool.get().await?;
        let rows = client
            .query(
                "SELECT id, organization_id, execution_id, step_order, name, description,
                        step_type, status, dependencies, estimated_duration_minutes,
                        actual_duration_minutes, started_at, completed_at,
                        integration_type, integration_config, integration_result,
                        approval_rule_id, approval_request_id, assigned_to,
                        input_data, output_data, error_message, retry_count, max_retries,
                        created_at, updated_at
                 FROM bpe.workflow_steps
                 WHERE execution_id = $1
                 ORDER BY step_order",
                &[&execution_id],
            )
            .await?;

        Ok(rows.iter().map(row_to_step).collect())
    }

    pub async fn complete_step(
        pool: &PgPool,
        step_id: Uuid,
        output_data: Option<serde_json::Value>,
        user_id: Option<Uuid>,
    ) -> Result<WorkflowStep, BpeError> {
        let step = Self::get_step(pool, step_id).await?;
        StepStateMachine::validate_transition(&step.status, "completed")?;

        // Use a single connection + transaction for atomicity
        let mut client = pool.get().await?;
        let txn = client.transaction().await.map_err(|e| {
            BpeError::Database(format!("Failed to start transaction: {e}"))
        })?;

        let row = txn
            .query_one(
                "UPDATE bpe.workflow_steps
                 SET status='completed', output_data=$2, completed_at=now(), updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, execution_id, step_order, name, description,
                           step_type, status, dependencies, estimated_duration_minutes,
                           actual_duration_minutes, started_at, completed_at,
                           integration_type, integration_config, integration_result,
                           approval_rule_id, approval_request_id, assigned_to,
                           input_data, output_data, error_message, retry_count, max_retries,
                           created_at, updated_at",
                &[&step_id, &output_data],
            )
            .await?;

        let result = row_to_step(&row);
        let execution_id = result.execution_id;

        // Advance ready steps within the same transaction
        Self::advance_ready_steps_txn(&txn, execution_id).await?;
        let exec_terminal = Self::check_execution_complete_txn(&txn, execution_id).await?;

        txn.commit().await.map_err(|e| {
            BpeError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        // Audit (outside transaction — non-critical)
        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_step.completed", "workflow_step",
            step_id, user_id, None, None,
            serde_json::json!({ "execution_id": execution_id, "step_order": result.step_order }),
        ).await {
            tracing::warn!("Audit log failed for workflow_step.completed: {e}");
        }

        // Audit execution auto-completion/failure
        if let Some(terminal_status) = exec_terminal {
            let event_type = format!("workflow_execution.{terminal_status}");
            if let Err(e) = AuditLogger::log_change(
                pool, result.organization_id, &event_type, "workflow_execution",
                execution_id, None, None, None,
                serde_json::json!({ "trigger": "auto", "final_step": step_id }),
            ).await {
                tracing::warn!("Audit log failed for {event_type}: {e}");
            }
        }

        Ok(result)
    }

    pub async fn skip_step(
        pool: &PgPool,
        step_id: Uuid,
        reason: Option<&str>,
        user_id: Option<Uuid>,
    ) -> Result<WorkflowStep, BpeError> {
        let step = Self::get_step(pool, step_id).await?;
        StepStateMachine::validate_transition(&step.status, "skipped")?;

        let output = reason.map(|r| serde_json::json!({ "skip_reason": r }));

        // Use a single connection + transaction for atomicity
        let mut client = pool.get().await?;
        let txn = client.transaction().await.map_err(|e| {
            BpeError::Database(format!("Failed to start transaction: {e}"))
        })?;

        let row = txn
            .query_one(
                "UPDATE bpe.workflow_steps
                 SET status='skipped', output_data=$2, completed_at=now(), updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, execution_id, step_order, name, description,
                           step_type, status, dependencies, estimated_duration_minutes,
                           actual_duration_minutes, started_at, completed_at,
                           integration_type, integration_config, integration_result,
                           approval_rule_id, approval_request_id, assigned_to,
                           input_data, output_data, error_message, retry_count, max_retries,
                           created_at, updated_at",
                &[&step_id, &output],
            )
            .await?;

        let result = row_to_step(&row);
        let execution_id = result.execution_id;

        Self::advance_ready_steps_txn(&txn, execution_id).await?;
        let exec_terminal = Self::check_execution_complete_txn(&txn, execution_id).await?;

        txn.commit().await.map_err(|e| {
            BpeError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_step.skipped", "workflow_step",
            step_id, user_id, None, None,
            serde_json::json!({ "execution_id": execution_id, "reason": reason }),
        ).await {
            tracing::warn!("Audit log failed for workflow_step.skipped: {e}");
        }

        // Audit execution auto-completion/failure
        if let Some(terminal_status) = exec_terminal {
            let event_type = format!("workflow_execution.{terminal_status}");
            if let Err(e) = AuditLogger::log_change(
                pool, result.organization_id, &event_type, "workflow_execution",
                execution_id, None, None, None,
                serde_json::json!({ "trigger": "auto", "final_step": step_id }),
            ).await {
                tracing::warn!("Audit log failed for {event_type}: {e}");
            }
        }

        Ok(result)
    }

    pub async fn retry_step(
        pool: &PgPool,
        step_id: Uuid,
        user_id: Option<Uuid>,
    ) -> Result<WorkflowStep, BpeError> {
        let step = Self::get_step(pool, step_id).await?;
        let before_status = step.status.clone();
        StepStateMachine::validate_transition(&step.status, "ready")?;

        if step.retry_count >= step.max_retries {
            return Err(BpeError::BadRequest(format!(
                "Step has reached maximum retries ({})", step.max_retries
            )));
        }

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.workflow_steps
                 SET status='ready', error_message=NULL, retry_count=retry_count+1, updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, execution_id, step_order, name, description,
                           step_type, status, dependencies, estimated_duration_minutes,
                           actual_duration_minutes, started_at, completed_at,
                           integration_type, integration_config, integration_result,
                           approval_rule_id, approval_request_id, assigned_to,
                           input_data, output_data, error_message, retry_count, max_retries,
                           created_at, updated_at",
                &[&step_id],
            )
            .await?;

        let result = row_to_step(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_step.retried", "workflow_step",
            step_id, user_id, None, None,
            serde_json::json!({
                "execution_id": result.execution_id,
                "step_order": result.step_order,
                "from_status": before_status,
                "retry_count": result.retry_count,
            }),
        ).await {
            tracing::warn!("Audit log failed for workflow_step.retried: {e}");
        }

        Ok(result)
    }

    pub async fn assign_step(
        pool: &PgPool,
        step_id: Uuid,
        user_id: Uuid,
        actor_id: Option<Uuid>,
    ) -> Result<WorkflowStep, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.workflow_steps
                 SET assigned_to=$2, updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, execution_id, step_order, name, description,
                           step_type, status, dependencies, estimated_duration_minutes,
                           actual_duration_minutes, started_at, completed_at,
                           integration_type, integration_config, integration_result,
                           approval_rule_id, approval_request_id, assigned_to,
                           input_data, output_data, error_message, retry_count, max_retries,
                           created_at, updated_at",
                &[&step_id, &user_id],
            )
            .await
            .map_err(|_| BpeError::NotFound(format!("Workflow step {step_id} not found")))?;

        let result = row_to_step(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "workflow_step.assigned", "workflow_step",
            step_id, actor_id, None, None,
            serde_json::json!({
                "execution_id": result.execution_id,
                "step_order": result.step_order,
                "assigned_to": user_id.to_string(),
            }),
        ).await {
            tracing::warn!("Audit log failed for workflow_step.assigned: {e}");
        }

        Ok(result)
    }

    /// Execute a workflow from a definition: create the execution and spawn steps from templates.
    pub async fn execute_definition(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
        definition_id: Uuid,
        target_entity_id: Option<Uuid>,
        context: Option<&serde_json::Value>,
    ) -> Result<WorkflowExecution, BpeError> {
        let definition = Self::get_definition(pool, definition_id).await?;

        let title = format!("Execution of: {}", definition.name);
        let execution = Self::create_execution(
            pool,
            org_id,
            user_id,
            &title,
            definition.description.as_deref(),
            None,
            Some(definition_id),
            target_entity_id,
        )
        .await?;

        // Create steps from step templates
        let mut client = pool.get().await?;
        let txn = client.transaction().await.map_err(|e| {
            BpeError::Database(format!("Failed to start transaction: {e}"))
        })?;
        for (i, tmpl) in definition.step_templates.iter().enumerate() {
            let step_order = (i + 1) as i32;
            let deps: Vec<i32> = tmpl.dependencies.clone();
            let input_data = context.cloned();

            // For ruflo_agent steps, enrich integration_config with step context
            // so the auto-routing can infer the correct agent type at execution time
            let integration_config = if tmpl.step_type == "ruflo_agent" || tmpl.integration_type.as_deref() == Some("ruflo_agent") {
                let mut config = tmpl.integration_config.clone().unwrap_or(serde_json::json!({}));
                if let Some(obj) = config.as_object_mut() {
                    obj.entry("step_name".to_string()).or_insert(serde_json::json!(tmpl.name));
                    if let Some(desc) = &tmpl.description {
                        obj.entry("step_description".to_string()).or_insert(serde_json::json!(desc));
                    }
                    obj.entry("workflow_category".to_string()).or_insert(serde_json::json!(definition.category));
                    // Collect step types of dependency steps for routing context
                    let preceding_types: Vec<&str> = tmpl.dependencies.iter()
                        .filter_map(|&dep_idx| {
                            let idx = if dep_idx > 0 { (dep_idx - 1) as usize } else { dep_idx as usize };
                            definition.step_templates.get(idx).map(|s| s.step_type.as_str())
                        })
                        .collect();
                    obj.entry("preceding_step_types".to_string()).or_insert(serde_json::json!(preceding_types));
                }
                Some(config)
            } else {
                tmpl.integration_config.clone()
            };

            txn
                .execute(
                    "INSERT INTO bpe.workflow_steps
                        (organization_id, execution_id, step_order, name, description,
                         step_type, status, dependencies, estimated_duration_minutes,
                         integration_type, integration_config, input_data)
                     VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, $9, $10, $11)",
                    &[
                        &org_id, &execution.id, &step_order, &tmpl.name, &tmpl.description,
                        &tmpl.step_type, &deps, &tmpl.estimated_duration_minutes,
                        &tmpl.integration_type, &integration_config, &input_data,
                    ],
                )
                .await?;
        }

        txn.commit().await.map_err(|e| {
            BpeError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        Ok(execution)
    }

    /// Public accessor for step by ID (used for org access checks in routes).
    pub async fn get_step_public(pool: &PgPool, step_id: Uuid) -> Result<WorkflowStep, BpeError> {
        Self::get_step(pool, step_id).await
    }

    // ---- Internal helpers ----

    async fn get_step(pool: &PgPool, step_id: Uuid) -> Result<WorkflowStep, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, execution_id, step_order, name, description,
                        step_type, status, dependencies, estimated_duration_minutes,
                        actual_duration_minutes, started_at, completed_at,
                        integration_type, integration_config, integration_result,
                        approval_rule_id, approval_request_id, assigned_to,
                        input_data, output_data, error_message, retry_count, max_retries,
                        created_at, updated_at
                 FROM bpe.workflow_steps WHERE id = $1",
                &[&step_id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Workflow step {step_id} not found")))?;

        Ok(row_to_step(&row))
    }

    /// After a step completes or is skipped, check if any pending steps have all dependencies met
    /// and mark them as ready. Transaction-aware version.
    async fn advance_ready_steps_txn(
        txn: &tokio_postgres::Transaction<'_>,
        execution_id: Uuid,
    ) -> Result<(), BpeError> {
        txn.execute(
            "UPDATE bpe.workflow_steps s SET status = 'ready', updated_at = now()
             WHERE s.execution_id = $1 AND s.status = 'pending'
               AND NOT EXISTS (
                 SELECT 1 FROM unnest(s.dependencies) AS dep(step_order)
                 WHERE dep.step_order NOT IN (
                   SELECT step_order FROM bpe.workflow_steps
                   WHERE execution_id = $1 AND status IN ('completed', 'skipped')
                 )
               )",
            &[&execution_id],
        ).await?;
        Ok(())
    }

    /// If all steps are completed or skipped, mark the execution as completed. Transaction-aware version.
    /// Returns: Some("completed") or Some("failed") if execution status changed, None otherwise.
    async fn check_execution_complete_txn(
        txn: &tokio_postgres::Transaction<'_>,
        execution_id: Uuid,
    ) -> Result<Option<&'static str>, BpeError> {
        let row = txn
            .query_one(
                "SELECT count(*) AS total,
                        count(*) FILTER (WHERE status NOT IN ('completed', 'skipped')) AS remaining,
                        count(*) FILTER (WHERE status = 'failed') AS failed_count,
                        count(*) FILTER (WHERE status IN ('ready', 'in_progress', 'waiting_approval', 'waiting_integration')) AS active_count
                 FROM bpe.workflow_steps WHERE execution_id = $1",
                &[&execution_id],
            )
            .await?;

        let total: i64 = row.get("total");
        let remaining: i64 = row.get("remaining");
        let failed_count: i64 = row.get("failed_count");
        let active_count: i64 = row.get("active_count");

        if remaining == 0 && total > 0 {
            txn.execute(
                "UPDATE bpe.workflow_executions SET status='completed', completed_at=now(), updated_at=now() WHERE id=$1",
                &[&execution_id],
            ).await?;
            Ok(Some("completed"))
        } else if failed_count > 0 && active_count == 0 && remaining > 0 {
            txn.execute(
                "UPDATE bpe.workflow_executions SET status='failed', updated_at=now() WHERE id=$1 AND status='running'",
                &[&execution_id],
            ).await?;
            Ok(Some("failed"))
        } else {
            Ok(None)
        }
    }
}

// ---- Row converters ----

fn row_to_definition(row: &tokio_postgres::Row) -> WorkflowDefinition {
    let step_templates_json: serde_json::Value = row.get("step_templates");
    WorkflowDefinition {
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
    }
}

fn row_to_execution(row: &tokio_postgres::Row) -> WorkflowExecution {
    WorkflowExecution {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        definition_id: row.get("definition_id"),
        title: row.get("title"),
        description: row.get("description"),
        original_prompt: row.get("original_prompt"),
        target_entity_id: row.get("target_entity_id"),
        linked_task_id: row.get("linked_task_id"),
        linked_goal_id: row.get("linked_goal_id"),
        status: row.get("status"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        cancelled_at: row.get("cancelled_at"),
        initiated_by: row.get("initiated_by"),
        metadata: row.get("metadata"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_step(row: &tokio_postgres::Row) -> WorkflowStep {
    WorkflowStep {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        execution_id: row.get("execution_id"),
        step_order: row.get("step_order"),
        name: row.get("name"),
        description: row.get("description"),
        step_type: row.get("step_type"),
        status: row.get("status"),
        dependencies: row.get("dependencies"),
        estimated_duration_minutes: row.get("estimated_duration_minutes"),
        actual_duration_minutes: row.get("actual_duration_minutes"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        integration_type: row.get("integration_type"),
        integration_config: row.get("integration_config"),
        integration_result: row.get("integration_result"),
        approval_rule_id: row.get("approval_rule_id"),
        approval_request_id: row.get("approval_request_id"),
        assigned_to: row.get("assigned_to"),
        input_data: row.get("input_data"),
        output_data: row.get("output_data"),
        error_message: row.get("error_message"),
        retry_count: row.get("retry_count"),
        max_retries: row.get("max_retries"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
