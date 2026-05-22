use crate::error::BpeError;
use super::models::{IntegrationResult, RufloAgentRequest, RufloAgentResponse};
use std::time::Duration;

/// HTTP client for the Ruflo AI agent sidecar service.
pub struct RufloClient {
    base_url: String,
    client: reqwest::Client,
}

impl RufloClient {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// Check if the Ruflo sidecar is reachable.
    pub async fn health_check(&self) -> Result<bool, BpeError> {
        let url = format!("{}/health", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Spawn an agent and wait for its result (synchronous mode).
    /// The agent executes the task and returns the output directly.
    pub async fn spawn_agent(
        &self,
        req: &RufloAgentRequest,
    ) -> Result<RufloAgentResponse, BpeError> {
        let url = format!("{}/api/agent/spawn", self.base_url);
        let start = std::time::Instant::now();

        let response = self.client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| BpeError::Internal(format!("Ruflo agent spawn failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BpeError::Internal(format!(
                "Ruflo agent spawn returned {status}: {body}"
            )));
        }

        let mut agent_resp: RufloAgentResponse = response.json().await
            .map_err(|e| BpeError::Internal(format!("Failed to parse Ruflo response: {e}")))?;

        if agent_resp.duration_ms.is_none() {
            agent_resp.duration_ms = Some(start.elapsed().as_millis() as i64);
        }

        Ok(agent_resp)
    }

    /// Spawn an agent asynchronously with a callback URL.
    /// The agent will POST its result to the callback when done.
    pub async fn spawn_agent_async(
        &self,
        req: &RufloAgentRequest,
    ) -> Result<RufloAgentResponse, BpeError> {
        let url = format!("{}/api/agent/spawn-async", self.base_url);

        let response = self.client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| BpeError::Internal(format!("Ruflo async spawn failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BpeError::Internal(format!(
                "Ruflo async spawn returned {status}: {body}"
            )));
        }

        response.json().await
            .map_err(|e| BpeError::Internal(format!("Failed to parse Ruflo response: {e}")))
    }

    /// Get the status/result of a previously spawned agent.
    pub async fn agent_status(&self, agent_id: &str) -> Result<RufloAgentResponse, BpeError> {
        let url = format!("{}/api/agent/{}/status", self.base_url, agent_id);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| BpeError::Internal(format!("Ruflo agent status check failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BpeError::Internal(format!(
                "Ruflo agent status returned {status}: {body}"
            )));
        }

        response.json().await
            .map_err(|e| BpeError::Internal(format!("Failed to parse Ruflo response: {e}")))
    }

    /// List available agent types from Ruflo.
    pub async fn list_agent_types(&self) -> Result<Vec<String>, BpeError> {
        let url = format!("{}/api/agent/types", self.base_url);

        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                #[derive(serde::Deserialize)]
                struct TypesResponse {
                    types: Vec<String>,
                }
                let body: TypesResponse = resp.json().await
                    .map_err(|e| BpeError::Internal(format!("Failed to parse agent types: {e}")))?;
                Ok(body.types)
            }
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                tracing::warn!("Failed to list Ruflo agent types: {body}");
                // Return default types if Ruflo is unavailable
                Ok(default_agent_types())
            }
            Err(e) => {
                tracing::warn!("Ruflo unreachable for agent types: {e}");
                Ok(default_agent_types())
            }
        }
    }
}

/// Convert a Ruflo agent response to a generic IntegrationResult.
pub fn ruflo_response_to_result(resp: &RufloAgentResponse) -> IntegrationResult {
    let success = resp.status == "completed" && resp.error.is_none();
    IntegrationResult {
        success,
        output: serde_json::json!({
            "agent_id": resp.agent_id,
            "status": resp.status,
            "output": resp.output,
        }),
        error: resp.error.clone(),
        duration_ms: resp.duration_ms.unwrap_or(0),
    }
}

/// Default agent types when Ruflo is unreachable.
/// Returns a static slice to avoid allocating a Vec<String> on every call.
fn default_agent_types() -> Vec<String> {
    static DEFAULTS: &[&str] = &[
        "researcher",
        "coder",
        "reviewer",
        "planner",
        "analyzer",
        "tester",
    ];
    DEFAULTS.iter().map(|s| (*s).to_string()).collect()
}

/// Check if any of the given fields contain any of the given keywords (case-insensitive).
fn contains_any(fields: &[&str], keywords: &[&str]) -> bool {
    for field in fields {
        let lower = field.to_ascii_lowercase();
        for kw in keywords {
            if lower.contains(kw) {
                return true;
            }
        }
    }
    false
}

/// Automatically select the best Ruflo agent type based on workflow step context.
///
/// Uses the step name, description, prompt, preceding step types, and workflow category
/// to infer the right agent without requiring the user to pick one manually.
pub fn infer_agent_type(
    step_name: &str,
    step_description: &str,
    prompt: &str,
    workflow_category: &str,
    preceding_step_types: &[&str],
) -> &'static str {
    let fields = [step_name, step_description, prompt, workflow_category];

    // Reviewer: follows approval steps, or mentions review/validate/check/audit/compliance
    if preceding_step_types.iter().any(|t| *t == "approval")
        || contains_any(&fields, &["review", "validate", "audit", "compliance", "check", "verify", "inspect", "quality"])
    {
        return "reviewer";
    }

    // Coder: mentions code/implement/build/develop/fix/bug/deploy/script
    if contains_any(&fields, &["code", "implement", "build", "develop", "fix", "bug", "deploy", "script", "program", "refactor"]) {
        return "coder";
    }

    // Tester: mentions test/qa/regression/coverage
    if contains_any(&fields, &["test", "qa ", "regression", "coverage", "acceptance"]) {
        return "tester";
    }

    // Planner: mentions plan/design/architect/organize/schedule
    if contains_any(&fields, &["plan", "design", "architect", "organize", "schedule", "decompos", "breakdown"]) {
        return "planner";
    }

    // Analyzer: mentions analyze/data/metrics/report/statistics/trend
    if contains_any(&fields, &["analyz", "data", "metric", "report", "statistic", "trend", "dashboard", "insight"]) {
        return "analyzer";
    }

    // Researcher: mentions research/gather/find/search/investigate/summarize
    if contains_any(&fields, &["research", "gather", "find", "search", "investigat", "summariz", "discover", "learn", "explore"]) {
        return "researcher";
    }

    // Category-based fallback
    match workflow_category.to_ascii_lowercase().as_str() {
        "compliance" => "reviewer",
        "it" => "coder",
        "finance" => "analyzer",
        _ => "researcher", // safe default
    }
}
