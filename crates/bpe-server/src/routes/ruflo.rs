use axum::{
    extract::{Path, State},
    Extension, Json,
};
use bpe_core::{
    auth::AuthClaims,
    error::BpeError,
    workflow::engine::WorkflowEngine,
};
use uuid::Uuid;

use crate::AppState;

/// GET /bpe/api/ruflo/health — Check if Ruflo sidecar is reachable
pub async fn ruflo_health(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let client = state.ruflo_client();
    let healthy = client.health_check().await.unwrap_or(false);
    Json(serde_json::json!({
        "ruflo_available": healthy,
    }))
}

/// GET /bpe/api/ruflo/agent-types — List available Ruflo agent types
pub async fn list_agent_types(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let client = state.ruflo_client();
    let types = client.list_agent_types().await?;
    Ok(Json(serde_json::json!({ "data": types })))
}

/// POST /bpe/api/ruflo/agent/spawn — Spawn a Ruflo agent directly (for testing)
/// Health check removed — let the actual spawn fail naturally with a clear error.
pub async fn spawn_agent(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Json(req): Json<bpe_core::integration::models::RufloAgentRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let client = state.ruflo_client();
    let resp = client.spawn_agent(&req).await?;
    Ok(Json(serde_json::json!({ "data": resp })))
}

/// POST /bpe/api/ruflo/callback/:step_id — Callback endpoint for async Ruflo agents
/// Called by Ruflo when an async agent completes its task.
/// Validates step exists and is in an active state before processing.
pub async fn agent_callback(
    State(state): State<AppState>,
    Path(step_id): Path<Uuid>,
    Json(body): Json<bpe_core::integration::models::RufloAgentResponse>,
) -> Result<Json<serde_json::Value>, BpeError> {
    // Verify step exists and is in an appropriate state for callback
    let step = WorkflowEngine::get_step_public(state.pool(), step_id).await?;
    if !["in_progress", "ready", "waiting_integration"].contains(&step.status.as_str()) {
        return Err(BpeError::BadRequest(format!(
            "Step {} is in '{}' state and cannot receive a callback",
            step_id, step.status
        )));
    }

    let result = bpe_core::integration::ruflo::ruflo_response_to_result(&body);

    // Store the integration result on the step
    let client = state.pool().get().await?;
    let result_json = serde_json::to_value(&result)
        .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?;

    client
        .execute(
            "UPDATE bpe.workflow_steps SET integration_result = $2, updated_at = now() WHERE id = $1",
            &[&step_id, &result_json],
        )
        .await?;

    // If the agent succeeded, complete the step (uses state machine + transaction)
    if result.success {
        let output = serde_json::json!({
            "ruflo_agent_id": body.agent_id,
            "ruflo_output": body.output,
        });
        WorkflowEngine::complete_step(state.pool(), step_id, Some(output), None).await?;
        Ok(Json(serde_json::json!({ "status": "step_completed" })))
    } else {
        // Mark step as failed via proper status update (includes error propagation)
        let error_msg = result.error.unwrap_or_else(|| "Ruflo agent failed with no error message".into());
        client
            .execute(
                "UPDATE bpe.workflow_steps SET status = 'failed', error_message = $2, updated_at = now()
                 WHERE id = $1 AND status IN ('in_progress', 'ready', 'waiting_integration')",
                &[&step_id, &error_msg],
            )
            .await?;

        // Check if execution should be marked as failed (no active steps remaining)
        let exec_check = client
            .query_one(
                "SELECT count(*) FILTER (WHERE status IN ('ready', 'in_progress', 'waiting_approval', 'waiting_integration')) AS active_count
                 FROM bpe.workflow_steps WHERE execution_id = $1",
                &[&step.execution_id],
            )
            .await?;
        let active_count: i64 = exec_check.get("active_count");
        if active_count == 0 {
            client
                .execute(
                    "UPDATE bpe.workflow_executions SET status = 'failed', updated_at = now()
                     WHERE id = $1 AND status = 'running'",
                    &[&step.execution_id],
                )
                .await?;
        }

        Ok(Json(serde_json::json!({ "status": "step_failed", "error": error_msg })))
    }
}
