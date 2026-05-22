use crate::audit::logger::AuditLogger;
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct ApprovalEngine;

impl ApprovalEngine {
    // ---- Rule CRUD ----

    pub async fn create_rule(
        pool: &PgPool,
        org_id: Uuid,
        req: &CreateRuleRequest,
    ) -> Result<ApprovalRule, BpeError> {
        let client = pool.get().await?;
        let conditions = req.conditions.clone().unwrap_or(serde_json::json!({"logic": "all", "conditions": []}));
        let approval_type = req.approval_type.as_deref().unwrap_or("single");
        let required = req.required_approvals.unwrap_or(1);
        let timeout = req.timeout_minutes.unwrap_or(0);
        let auto_approve = req.auto_approve_on_timeout.unwrap_or(false);
        let allow_delegation = req.allow_delegation.unwrap_or(true);

        let row = client
            .query_one(
                "INSERT INTO bpe.approval_rules
                    (organization_id, name, description, conditions, approval_type,
                     approver_user_ids, required_approvals, timeout_minutes,
                     escalation_user_id, auto_approve_on_timeout, allow_delegation)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 RETURNING id, organization_id, name, description, conditions, approval_type,
                           approver_user_ids, required_approvals, timeout_minutes,
                           escalation_user_id, auto_approve_on_timeout, allow_delegation,
                           is_active, created_at, updated_at",
                &[
                    &org_id, &req.name, &req.description, &conditions, &approval_type,
                    &req.approver_user_ids, &required, &timeout,
                    &req.escalation_user_id, &auto_approve, &allow_delegation,
                ],
            )
            .await?;

        Ok(row_to_rule(&row))
    }

    pub async fn list_rules(
        pool: &PgPool,
        org_id: Uuid,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedRules, BpeError> {
        let client = pool.get().await?;
        let offset = (page - 1) * per_page;

        let count_row = client
            .query_one(
                "SELECT count(*) FROM bpe.approval_rules
                 WHERE organization_id = $1 AND is_active = true",
                &[&org_id],
            )
            .await?;
        let total: i64 = count_row.get(0);

        let rows = client
            .query(
                "SELECT id, organization_id, name, description, conditions, approval_type,
                        approver_user_ids, required_approvals, timeout_minutes,
                        escalation_user_id, auto_approve_on_timeout, allow_delegation,
                        is_active, created_at, updated_at
                 FROM bpe.approval_rules
                 WHERE organization_id = $1 AND is_active = true
                 ORDER BY name
                 LIMIT $2 OFFSET $3",
                &[&org_id, &per_page, &offset],
            )
            .await?;

        let data = rows.iter().map(row_to_rule).collect();
        Ok(PaginatedRules { data, page, per_page, total })
    }

    pub async fn get_rule(pool: &PgPool, id: Uuid) -> Result<ApprovalRule, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, name, description, conditions, approval_type,
                        approver_user_ids, required_approvals, timeout_minutes,
                        escalation_user_id, auto_approve_on_timeout, allow_delegation,
                        is_active, created_at, updated_at
                 FROM bpe.approval_rules WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Approval rule {id} not found")))?;

        Ok(row_to_rule(&row))
    }

    pub async fn update_rule(
        pool: &PgPool,
        id: Uuid,
        req: &UpdateRuleRequest,
    ) -> Result<ApprovalRule, BpeError> {
        // Use a single connection for both the fetch and the update
        let client = pool.get().await?;

        let existing = {
            let row = client
                .query_opt(
                    "SELECT id, organization_id, name, description, conditions, approval_type,
                            approver_user_ids, required_approvals, timeout_minutes,
                            escalation_user_id, auto_approve_on_timeout, allow_delegation,
                            is_active, created_at, updated_at
                     FROM bpe.approval_rules WHERE id = $1",
                    &[&id],
                )
                .await?
                .ok_or_else(|| BpeError::NotFound(format!("Approval rule {id} not found")))?;
            row_to_rule(&row)
        };

        let name = req.name.as_deref().unwrap_or(&existing.name);
        let description = req.description.as_ref().or(existing.description.as_ref());
        let conditions = req.conditions.as_ref().unwrap_or(&existing.conditions);
        let approval_type = req.approval_type.as_deref().unwrap_or(&existing.approval_type);
        let approver_ids = req.approver_user_ids.as_ref().unwrap_or(&existing.approver_user_ids);
        let required = req.required_approvals.unwrap_or(existing.required_approvals);
        let timeout = req.timeout_minutes.unwrap_or(existing.timeout_minutes);
        let escalation = req.escalation_user_id.or(existing.escalation_user_id);
        let auto_approve = req.auto_approve_on_timeout.unwrap_or(existing.auto_approve_on_timeout);
        let allow_delegation = req.allow_delegation.unwrap_or(existing.allow_delegation);
        let is_active = req.is_active.unwrap_or(existing.is_active);

        let row = client
            .query_one(
                "UPDATE bpe.approval_rules
                 SET name=$1, description=$2, conditions=$3, approval_type=$4,
                     approver_user_ids=$5, required_approvals=$6, timeout_minutes=$7,
                     escalation_user_id=$8, auto_approve_on_timeout=$9, allow_delegation=$10,
                     is_active=$11, updated_at=now()
                 WHERE id=$12
                 RETURNING id, organization_id, name, description, conditions, approval_type,
                           approver_user_ids, required_approvals, timeout_minutes,
                           escalation_user_id, auto_approve_on_timeout, allow_delegation,
                           is_active, created_at, updated_at",
                &[
                    &name, &description, conditions, &approval_type,
                    approver_ids, &required, &timeout,
                    &escalation, &auto_approve, &allow_delegation, &is_active, &id,
                ],
            )
            .await?;

        Ok(row_to_rule(&row))
    }

    pub async fn delete_rule(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute("DELETE FROM bpe.approval_rules WHERE id = $1", &[&id])
            .await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Approval rule {id} not found")));
        }
        Ok(())
    }

    // ---- Request lifecycle ----

    pub async fn create_request(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
        req: &CreateRequestPayload,
    ) -> Result<ApprovalRequest, BpeError> {
        // Verify rule exists and is active
        let rule = Self::get_rule(pool, req.rule_id).await?;
        if !rule.is_active {
            return Err(BpeError::BadRequest("Approval rule is not active".into()));
        }

        // Calculate deadline from rule timeout
        let deadline_at: Option<chrono::DateTime<chrono::Utc>> = if rule.timeout_minutes > 0 {
            Some(chrono::Utc::now() + chrono::Duration::minutes(rule.timeout_minutes as i64))
        } else {
            None
        };

        let context_data = req.context_data.clone().unwrap_or(serde_json::json!({}));

        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.approval_requests
                    (organization_id, rule_id, workflow_execution_id, workflow_step_id,
                     title, description, context_data, status, requested_by, deadline_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, $9)
                 RETURNING id, organization_id, rule_id, workflow_execution_id, workflow_step_id,
                           title, description, context_data, status, requested_by,
                           current_approver_index, deadline_at, resolved_at, resolution_notes,
                           created_at, updated_at",
                &[
                    &org_id, &req.rule_id, &req.workflow_execution_id, &req.workflow_step_id,
                    &req.title, &req.description, &context_data, &user_id, &deadline_at,
                ],
            )
            .await?;

        let request = row_to_request(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, org_id, "approval_request.created", "approval_request", request.id,
            Some(user_id), None, None,
            serde_json::json!({ "rule_id": req.rule_id, "title": req.title }),
        ).await {
            tracing::warn!("Audit log failed for approval_request.created: {e}");
        }

        Ok(request)
    }

    pub async fn get_request(pool: &PgPool, id: Uuid) -> Result<ApprovalRequest, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, rule_id, workflow_execution_id, workflow_step_id,
                        title, description, context_data, status, requested_by,
                        current_approver_index, deadline_at, resolved_at, resolution_notes,
                        created_at, updated_at
                 FROM bpe.approval_requests WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Approval request {id} not found")))?;

        Ok(row_to_request(&row))
    }

    pub async fn list_requests(
        pool: &PgPool,
        org_id: Uuid,
        query: &ListRequestsQuery,
    ) -> Result<PaginatedRequests, BpeError> {
        let client = pool.get().await?;
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        let count_row = client
            .query_one(
                "SELECT count(*) FROM bpe.approval_requests
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR status = $2)
                   AND ($3::uuid IS NULL OR rule_id = $3)",
                &[&org_id, &query.status, &query.rule_id],
            )
            .await?;
        let total: i64 = count_row.get(0);

        let rows = client
            .query(
                "SELECT id, organization_id, rule_id, workflow_execution_id, workflow_step_id,
                        title, description, context_data, status, requested_by,
                        current_approver_index, deadline_at, resolved_at, resolution_notes,
                        created_at, updated_at
                 FROM bpe.approval_requests
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR status = $2)
                   AND ($3::uuid IS NULL OR rule_id = $3)
                 ORDER BY created_at DESC
                 LIMIT $4 OFFSET $5",
                &[&org_id, &query.status, &query.rule_id, &per_page, &offset],
            )
            .await?;

        let data = rows.iter().map(row_to_request).collect();
        Ok(PaginatedRequests { data, page, per_page, total })
    }

    /// Get pending approval requests for a specific user (they are in the approver list).
    pub async fn pending_for_user(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, BpeError> {
        let client = pool.get().await?;
        let rows = client
            .query(
                "SELECT r.id, r.organization_id, r.rule_id, r.workflow_execution_id,
                        r.workflow_step_id, r.title, r.description, r.context_data,
                        r.status, r.requested_by, r.current_approver_index,
                        r.deadline_at, r.resolved_at, r.resolution_notes,
                        r.created_at, r.updated_at
                 FROM bpe.approval_requests r
                 JOIN bpe.approval_rules rl ON rl.id = r.rule_id
                 WHERE r.organization_id = $1
                   AND r.status = 'pending'
                   AND $2 = ANY(rl.approver_user_ids)
                 ORDER BY r.created_at",
                &[&org_id, &user_id],
            )
            .await?;

        Ok(rows.iter().map(row_to_request).collect())
    }

    // ---- Decision ----

    pub async fn decide(
        pool: &PgPool,
        request_id: Uuid,
        user_id: Uuid,
        payload: &DecisionPayload,
    ) -> Result<ApprovalDecision, BpeError> {
        // Use a single DB connection for the entire decide operation
        let client = pool.get().await?;

        // Fetch request inline
        let request = {
            let row = client
                .query_opt(
                    "SELECT id, organization_id, rule_id, workflow_execution_id, workflow_step_id,
                            title, description, context_data, status, requested_by,
                            current_approver_index, deadline_at, resolved_at, resolution_notes,
                            created_at, updated_at
                     FROM bpe.approval_requests WHERE id = $1",
                    &[&request_id],
                )
                .await?
                .ok_or_else(|| BpeError::NotFound(format!("Approval request {request_id} not found")))?;
            row_to_request(&row)
        };

        if request.status != "pending" {
            return Err(BpeError::BadRequest(format!(
                "Cannot decide on request in '{}' status", request.status
            )));
        }

        // Validate decision value
        let decision = payload.decision.as_str();
        if !matches!(decision, "approved" | "rejected" | "request_changes") {
            return Err(BpeError::BadRequest(format!(
                "Invalid decision: '{}'. Must be approved, rejected, or request_changes", decision
            )));
        }

        // Fetch rule inline using the same connection
        let rule = {
            let row = client
                .query_opt(
                    "SELECT id, organization_id, name, description, conditions, approval_type,
                            approver_user_ids, required_approvals, timeout_minutes,
                            escalation_user_id, auto_approve_on_timeout, allow_delegation,
                            is_active, created_at, updated_at
                     FROM bpe.approval_rules WHERE id = $1",
                    &[&request.rule_id],
                )
                .await?
                .ok_or_else(|| BpeError::NotFound(format!("Approval rule {} not found", request.rule_id)))?;
            row_to_rule(&row)
        };

        let is_approver = rule.approver_user_ids.contains(&user_id);
        let is_delegated = payload.delegated_from.map_or(false, |from| rule.approver_user_ids.contains(&from));

        if !is_approver && !is_delegated {
            return Err(BpeError::Forbidden("You are not an approver for this rule".into()));
        }

        if is_delegated && !rule.allow_delegation {
            return Err(BpeError::Forbidden("Delegation is not allowed for this rule".into()));
        }

        // Record the decision
        let row = client
            .query_one(
                "INSERT INTO bpe.approval_decisions
                    (organization_id, request_id, decided_by, delegated_from, decision, notes)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id, organization_id, request_id, decided_by, delegated_from,
                           decision, notes, decided_at",
                &[
                    &request.organization_id, &request_id, &user_id,
                    &payload.delegated_from, &decision, &payload.notes,
                ],
            )
            .await?;

        let approval_decision = row_to_decision(&row);

        // Determine if request should be resolved based on rule type and approvals count
        // Inline check_resolution to use the same connection
        let should_resolve = if decision == "rejected" {
            true
        } else {
            let count_row = client
                .query_one(
                    "SELECT count(*) FROM bpe.approval_decisions
                     WHERE request_id = $1 AND decision = 'approved'",
                    &[&request.id],
                )
                .await?;
            let approval_count: i64 = count_row.get(0);

            match rule.approval_type.as_str() {
                "single" => true,
                "quorum" => approval_count >= rule.required_approvals as i64,
                "unanimous" | "sequential" => approval_count >= rule.approver_user_ids.len() as i64,
                _ => true,
            }
        };

        if should_resolve {
            let new_status = if decision == "approved" { "approved" } else { "rejected" };
            client
                .execute(
                    "UPDATE bpe.approval_requests
                     SET status=$2, resolved_at=now(), resolution_notes=$3, updated_at=now()
                     WHERE id=$1",
                    &[&request_id, &new_status, &payload.notes],
                )
                .await?;

            // If linked to a workflow step, update the step status
            if let Some(step_id) = request.workflow_step_id {
                let step_status = if decision == "approved" { "completed" } else { "failed" };
                let error_msg: Option<String> = if decision != "approved" {
                    Some(format!("Approval rejected: {}", payload.notes.as_deref().unwrap_or("no reason given")))
                } else {
                    None
                };
                client
                    .execute(
                        "UPDATE bpe.workflow_steps
                         SET status=$2, error_message=$3, completed_at=now(), updated_at=now()
                         WHERE id=$1 AND status IN ('waiting_approval')",
                        &[&step_id, &step_status, &error_msg],
                    )
                    .await?;
            }
        }

        // Audit
        if let Err(e) = AuditLogger::log_change(
            pool, request.organization_id, "approval_request.decided", "approval_request",
            request_id, Some(user_id), None, None,
            serde_json::json!({ "decision": decision, "request_title": request.title }),
        ).await {
            tracing::warn!("Audit log failed for approval_request.decided: {e}");
        }

        Ok(approval_decision)
    }

    /// Get all decisions for a request.
    pub async fn get_decisions(
        pool: &PgPool,
        request_id: Uuid,
    ) -> Result<Vec<ApprovalDecision>, BpeError> {
        let client = pool.get().await?;
        let rows = client
            .query(
                "SELECT id, organization_id, request_id, decided_by, delegated_from,
                        decision, notes, decided_at
                 FROM bpe.approval_decisions
                 WHERE request_id = $1
                 ORDER BY decided_at",
                &[&request_id],
            )
            .await?;

        Ok(rows.iter().map(row_to_decision).collect())
    }

    /// Cancel a pending approval request.
    pub async fn cancel_request(
        pool: &PgPool,
        request_id: Uuid,
        user_id: Uuid,
    ) -> Result<ApprovalRequest, BpeError> {
        // Use a single connection for both the fetch and the update
        let client = pool.get().await?;
        let request = {
            let row = client
                .query_opt(
                    "SELECT id, organization_id, rule_id, workflow_execution_id, workflow_step_id,
                            title, description, context_data, status, requested_by,
                            current_approver_index, deadline_at, resolved_at, resolution_notes,
                            created_at, updated_at
                     FROM bpe.approval_requests WHERE id = $1",
                    &[&request_id],
                )
                .await?
                .ok_or_else(|| BpeError::NotFound(format!("Approval request {request_id} not found")))?;
            row_to_request(&row)
        };
        if request.status != "pending" {
            return Err(BpeError::BadRequest(format!(
                "Cannot cancel request in '{}' status", request.status
            )));
        }
        let row = client
            .query_one(
                "UPDATE bpe.approval_requests
                 SET status='cancelled', resolved_at=now(), updated_at=now()
                 WHERE id=$1
                 RETURNING id, organization_id, rule_id, workflow_execution_id, workflow_step_id,
                           title, description, context_data, status, requested_by,
                           current_approver_index, deadline_at, resolved_at, resolution_notes,
                           created_at, updated_at",
                &[&request_id],
            )
            .await?;

        let result = row_to_request(&row);

        if let Err(e) = AuditLogger::log_change(
            pool, result.organization_id, "approval_request.cancelled", "approval_request",
            request_id, Some(user_id), None, None, serde_json::json!({}),
        ).await {
            tracing::warn!("Audit log failed for approval_request.cancelled: {e}");
        }

        Ok(result)
    }

}

// ---- Row converters ----

fn row_to_rule(row: &tokio_postgres::Row) -> ApprovalRule {
    ApprovalRule {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        name: row.get("name"),
        description: row.get("description"),
        conditions: row.get("conditions"),
        approval_type: row.get("approval_type"),
        approver_user_ids: row.get("approver_user_ids"),
        required_approvals: row.get("required_approvals"),
        timeout_minutes: row.get("timeout_minutes"),
        escalation_user_id: row.get("escalation_user_id"),
        auto_approve_on_timeout: row.get("auto_approve_on_timeout"),
        allow_delegation: row.get("allow_delegation"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_request(row: &tokio_postgres::Row) -> ApprovalRequest {
    ApprovalRequest {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        rule_id: row.get("rule_id"),
        workflow_execution_id: row.get("workflow_execution_id"),
        workflow_step_id: row.get("workflow_step_id"),
        title: row.get("title"),
        description: row.get("description"),
        context_data: row.get("context_data"),
        status: row.get("status"),
        requested_by: row.get("requested_by"),
        current_approver_index: row.get("current_approver_index"),
        deadline_at: row.get("deadline_at"),
        resolved_at: row.get("resolved_at"),
        resolution_notes: row.get("resolution_notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_decision(row: &tokio_postgres::Row) -> ApprovalDecision {
    ApprovalDecision {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        request_id: row.get("request_id"),
        decided_by: row.get("decided_by"),
        delegated_from: row.get("delegated_from"),
        decision: row.get("decision"),
        notes: row.get("notes"),
        decided_at: row.get("decided_at"),
    }
}
