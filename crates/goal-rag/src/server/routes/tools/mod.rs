//! LLM-optimized tool interface for structured data access
//!
//! Provides typed CRUD tools, search, reports, and SQL escape hatch
//! for LLM agents to interact with PostgreSQL `api.*` schema.
//! Exposed as both HTTP endpoints and MCP-compatible tool definitions.

pub mod read;
pub mod report;
pub mod search;
pub mod sql;
pub mod write;

use axum::{
    extract::State,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::server::state::AppState;
use crate::validation::validate_organization_id;

// ============================================================================
// Tool Result (unified response for all tools)
// ============================================================================

/// Unified response from any tool execution
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    /// Whether the tool executed successfully
    pub success: bool,
    /// Structured result data
    pub data: Value,
    /// Human-readable summary for LLM context window efficiency
    pub summary: String,
    /// Number of rows/items returned
    pub row_count: usize,
    /// Tool execution time in milliseconds
    pub execution_ms: u64,
}

impl ToolResult {
    pub fn ok(data: Value, summary: String, row_count: usize, execution_ms: u64) -> Self {
        Self { success: true, data, summary, row_count, execution_ms }
    }

    pub fn err(message: String) -> Self {
        Self {
            success: false,
            data: Value::Null,
            summary: message,
            row_count: 0,
            execution_ms: 0,
        }
    }
}

// ============================================================================
// Tool Definitions (for MCP manifest)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub parameters: Value,
}

/// GET /api/tools/manifest - Return all tool definitions for MCP/agent discovery
pub async fn manifest() -> Json<Vec<ToolDefinition>> {
    Json(all_tool_definitions())
}

fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // Read tools
        ToolDefinition {
            name: "get_task",
            description: "Get a task by ID with assignee name, goal title, and recent comments",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["task_id", "organization_id"],
                "properties": {
                    "task_id": { "type": "string", "format": "uuid", "description": "Task UUID" },
                    "organization_id": { "type": "string", "description": "Organization slug" }
                }
            }),
        },
        ToolDefinition {
            name: "get_goal",
            description: "Get a goal by ID with child goals and task summary (count by status)",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["goal_id", "organization_id"],
                "properties": {
                    "goal_id": { "type": "string", "format": "uuid", "description": "Goal UUID" },
                    "organization_id": { "type": "string", "description": "Organization slug" }
                }
            }),
        },
        ToolDefinition {
            name: "get_user",
            description: "Get a user by ID with task count and recent activity",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["user_id", "organization_id"],
                "properties": {
                    "user_id": { "type": "string", "format": "uuid", "description": "User UUID" },
                    "organization_id": { "type": "string", "description": "Organization slug" }
                }
            }),
        },
        ToolDefinition {
            name: "list_tasks",
            description: "List tasks with optional filters (status, assigned_to, goal_id, priority, overdue)",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "status": { "type": "string", "enum": ["todo", "in_progress", "done", "blocked"] },
                    "assigned_to": { "type": "string", "format": "uuid" },
                    "goal_id": { "type": "string", "format": "uuid" },
                    "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                    "overdue": { "type": "boolean", "description": "Only tasks past due_date" },
                    "limit": { "type": "integer", "default": 50, "maximum": 200 },
                    "offset": { "type": "integer", "default": 0 }
                }
            }),
        },
        ToolDefinition {
            name: "list_goals",
            description: "List goals with optional filters and task counts",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "status": { "type": "string" },
                    "parent_goal_id": { "type": "string", "format": "uuid", "description": "Filter by parent goal" },
                    "limit": { "type": "integer", "default": 50, "maximum": 200 },
                    "offset": { "type": "integer", "default": 0 }
                }
            }),
        },
        ToolDefinition {
            name: "get_goal_tree",
            description: "Get hierarchical goal tree with task counts at each level",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "goal_id": { "type": "string", "format": "uuid", "description": "Root goal (null = full tree)" }
                }
            }),
        },
        ToolDefinition {
            name: "get_task_timeline",
            description: "Get chronological timeline of a task: comments, activity logs, status changes",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["task_id", "organization_id"],
                "properties": {
                    "task_id": { "type": "string", "format": "uuid" },
                    "organization_id": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "get_conversation",
            description: "Get messages in a conversation thread",
            category: "read",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["conversation_id", "organization_id"],
                "properties": {
                    "conversation_id": { "type": "string", "format": "uuid" },
                    "organization_id": { "type": "string" },
                    "limit": { "type": "integer", "default": 50, "maximum": 200 }
                }
            }),
        },
        // Write tools
        ToolDefinition {
            name: "create_task",
            description: "Create a new task. Requires title, description, priority, due_date, created_by. Optionally assigned_to, goal_id, status (defaults to 'Assigned')",
            category: "write",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["title", "description", "priority", "due_date", "created_by", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string", "description": "Organization slug" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                    "due_date": { "type": "string", "format": "date", "description": "YYYY-MM-DD" },
                    "created_by": { "type": "string", "format": "uuid", "description": "User who creates the task" },
                    "assigned_to": { "type": "string", "format": "uuid", "description": "User to assign (optional)" },
                    "goal_id": { "type": "string", "format": "uuid", "description": "Parent goal (optional)" },
                    "status": { "type": "string", "default": "Assigned" }
                }
            }),
        },
        ToolDefinition {
            name: "update_task",
            description: "Update a task. At least one field besides task_id is required. Supports: title, description, status, priority, assigned_to, due_date, goal_id",
            category: "write",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["task_id", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "task_id": { "type": "string", "format": "uuid" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "status": { "type": "string" },
                    "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                    "assigned_to": { "type": "string", "format": "uuid" },
                    "due_date": { "type": "string", "format": "date" },
                    "goal_id": { "type": "string", "format": "uuid" }
                }
            }),
        },
        ToolDefinition {
            name: "create_goal",
            description: "Create a new goal. Requires title. Optional: description, status (defaults to 'not_started'), target_date, parent_goal_id, created_by",
            category: "write",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["title", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "status": { "type": "string", "default": "not_started" },
                    "target_date": { "type": "string", "format": "date", "description": "YYYY-MM-DD" },
                    "parent_goal_id": { "type": "string", "format": "uuid" },
                    "created_by": { "type": "string", "format": "uuid" }
                }
            }),
        },
        ToolDefinition {
            name: "update_goal",
            description: "Update a goal. At least one field besides goal_id is required. Supports: title, description, status, progress (0-100), target_date",
            category: "write",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["goal_id", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "goal_id": { "type": "string", "format": "uuid" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "status": { "type": "string" },
                    "progress": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "target_date": { "type": "string", "format": "date" }
                }
            }),
        },
        ToolDefinition {
            name: "add_comment",
            description: "Add a comment to a task. Returns the created comment with author name",
            category: "write",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["task_id", "content", "author_id", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "task_id": { "type": "string", "format": "uuid" },
                    "content": { "type": "string" },
                    "author_id": { "type": "string", "format": "uuid" },
                    "is_private": { "type": "boolean", "default": false }
                }
            }),
        },
        ToolDefinition {
            name: "assign_task",
            description: "Assign a task to a user. Shortcut for updating assigned_to with activity logging",
            category: "write",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["task_id", "user_id", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "task_id": { "type": "string", "format": "uuid" },
                    "user_id": { "type": "string", "format": "uuid", "description": "User to assign the task to" }
                }
            }),
        },
        // Search tools
        ToolDefinition {
            name: "search_tasks",
            description: "Text search tasks by title or description. Returns matching tasks with assignee names",
            category: "search",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["query", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "query": { "type": "string", "description": "Text to search in title and description" },
                    "status": { "type": "string", "description": "Filter by status" },
                    "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                    "assigned_to": { "type": "string", "format": "uuid", "description": "Filter by assignee" },
                    "limit": { "type": "integer", "default": 20, "maximum": 100 }
                }
            }),
        },
        ToolDefinition {
            name: "search_users",
            description: "Text search users by name or email. Returns matching users with titles",
            category: "search",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["query", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "query": { "type": "string", "description": "Text to search in name and email" },
                    "limit": { "type": "integer", "default": 20, "maximum": 100 }
                }
            }),
        },
        ToolDefinition {
            name: "semantic_search",
            description: "Semantic similarity search across entity embeddings (tasks, goals, comments). Uses natural language understanding",
            category: "search",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["query", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "query": { "type": "string", "description": "Natural language query" },
                    "entity_type": { "type": "string", "enum": ["task", "goal", "task_comment", "message"], "description": "Filter by entity type (optional)" },
                    "top_k": { "type": "integer", "default": 10, "maximum": 50 }
                }
            }),
        },
        ToolDefinition {
            name: "find_similar",
            description: "Find entities similar to a given entity using embedding similarity",
            category: "search",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["entity_type", "entity_id", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "entity_type": { "type": "string", "enum": ["task", "goal", "task_comment", "message"] },
                    "entity_id": { "type": "string", "format": "uuid" },
                    "search_type": { "type": "string", "enum": ["task", "goal", "task_comment", "message"], "description": "Type of entities to search for (optional, defaults to all)" },
                    "top_k": { "type": "integer", "default": 10, "maximum": 50 }
                }
            }),
        },
        ToolDefinition {
            name: "enriched_search",
            description: "Cross-reference search across both document embeddings and entity embeddings. Returns unified results from uploaded documents (PDFs, etc.) and structured entities (tasks, goals, comments). Single embedding call, dual search.",
            category: "search",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["query", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "query": { "type": "string", "description": "Natural language query to search across documents and entities" },
                    "top_k": { "type": "integer", "default": 10, "maximum": 50, "description": "Max results per source (documents and entities)" },
                    "include": { "type": "string", "enum": ["both", "documents", "entities"], "default": "both", "description": "Which sources to search" }
                }
            }),
        },
        // Report tools
        ToolDefinition {
            name: "sprint_report",
            description: "Sprint metrics: task status breakdown, velocity (completed/created), overdue count, and top blockers for a time period",
            category: "report",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "since": { "type": "string", "format": "date", "description": "Start date YYYY-MM-DD (default: 14 days ago)" },
                    "until": { "type": "string", "format": "date", "description": "End date YYYY-MM-DD (default: today)" }
                }
            }),
        },
        ToolDefinition {
            name: "workload_report",
            description: "Per-user task distribution: active tasks, completed, in progress, overdue. Includes load balance stats (avg, min, max)",
            category: "report",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["organization_id"],
                "properties": {
                    "organization_id": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "goal_progress_report",
            description: "Goal hierarchy with completion percentages and at-risk flags. Shows overall org progress and goals nearing deadline with low completion",
            category: "report",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["organization_id"],
                "properties": {
                    "organization_id": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "activity_summary",
            description: "Recent org-wide activity feed: comments, status changes, assignments. Chronological with optional event_type filter",
            category: "report",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "since": { "type": "string", "format": "date", "description": "Start date YYYY-MM-DD (default: 7 days ago)" },
                    "event_type": { "type": "string", "enum": ["comment", "activity"], "description": "Filter to specific event type" },
                    "limit": { "type": "integer", "default": 50, "maximum": 200 }
                }
            }),
        },
        // SQL escape hatch
        ToolDefinition {
            name: "run_sql",
            description: "Execute read-only SQL against the api schema. $1 is automatically bound to the organization UUID. Tables: tasks, goals, users, task_comments, chat_messages, task_activity_logs, organizations. 5s timeout, 100 row max.",
            category: "sql",
            parameters: serde_json::json!({
                "type": "object",
                "required": ["sql", "organization_id"],
                "properties": {
                    "organization_id": { "type": "string" },
                    "sql": { "type": "string", "description": "SELECT or WITH query. Use $1 for organization_id (UUID). Tables can be unqualified (e.g. 'tasks') or qualified (e.g. 'api.tasks')" },
                    "limit": { "type": "integer", "default": 100, "maximum": 100, "description": "Max rows to return (default 100)" }
                }
            }),
        },
    ]
}

// ============================================================================
// Tool Execution Router
// ============================================================================

/// Request to execute a single tool
#[derive(Debug, Deserialize)]
pub struct ToolExecuteRequest {
    pub tool: String,
    pub params: Value,
}

/// POST /api/tools/execute - Execute a tool by name
pub async fn execute_tool(
    State(state): State<AppState>,
    Json(request): Json<ToolExecuteRequest>,
) -> Result<Json<ToolResult>> {
    let start = std::time::Instant::now();

    // Extract and validate organization_id from params
    let org_id = request.params.get("organization_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Validation("organization_id is required".to_string()))?;
    validate_organization_id(org_id)?;

    // Resolve org slug to UUID for api.* table queries
    let pool = state.pg_pool()
        .ok_or_else(|| Error::Internal("PostgreSQL pool not available".to_string()))?;
    let org_uuid = resolve_org_uuid(pool, org_id).await?;

    let result = match request.tool.as_str() {
        "get_task" => read::get_task(pool, &org_uuid, &request.params).await,
        "get_goal" => read::get_goal(pool, &org_uuid, &request.params).await,
        "get_user" => read::get_user(pool, &org_uuid, &request.params).await,
        "list_tasks" => read::list_tasks(pool, &org_uuid, &request.params).await,
        "list_goals" => read::list_goals(pool, &org_uuid, &request.params).await,
        "get_goal_tree" => read::get_goal_tree(pool, &org_uuid, &request.params).await,
        "get_task_timeline" => read::get_task_timeline(pool, &org_uuid, &request.params).await,
        "get_conversation" => read::get_conversation(pool, &org_uuid, &request.params).await,
        // Write tools
        "create_task" => write::create_task(pool, &org_uuid, &request.params).await,
        "update_task" => write::update_task(pool, &org_uuid, &request.params).await,
        "create_goal" => write::create_goal(pool, &org_uuid, &request.params).await,
        "update_goal" => write::update_goal(pool, &org_uuid, &request.params).await,
        "add_comment" => write::add_comment(pool, &org_uuid, &request.params).await,
        "assign_task" => write::assign_task(pool, &org_uuid, &request.params).await,
        // Search tools
        "search_tasks" => search::search_tasks(pool, &org_uuid, &request.params).await,
        "search_users" => search::search_users(pool, &org_uuid, &request.params).await,
        "semantic_search" => {
            let store = state.entity_embedding_store()
                .ok_or_else(|| Error::Internal("Entity embedding store not available".to_string()))?;
            search::semantic_search(store, &org_uuid, &request.params).await
        },
        "find_similar" => {
            let store = state.entity_embedding_store()
                .ok_or_else(|| Error::Internal("Entity embedding store not available".to_string()))?;
            search::find_similar(store, &org_uuid, &request.params).await
        },
        "enriched_search" => {
            let vector_store = state.vector_store_provider();
            let embedding_provider = state.embedding_provider();
            let entity_store = state.entity_embedding_store()
                .ok_or_else(|| Error::Internal("Entity embedding store not available".to_string()))?;
            search::enriched_search(vector_store, embedding_provider, entity_store, &org_uuid, &request.params).await
        },
        // Report tools
        "sprint_report" => report::sprint_report(pool, &org_uuid, &request.params).await,
        "workload_report" => report::workload_report(pool, &org_uuid, &request.params).await,
        "goal_progress_report" => report::goal_progress_report(pool, &org_uuid, &request.params).await,
        "activity_summary" => report::activity_summary(pool, &org_uuid, &request.params).await,
        // SQL escape hatch
        "run_sql" => sql::run_sql(pool, &org_uuid, &request.params).await,
        _ => {
            return Err(Error::Validation(format!("Unknown tool: {}", request.tool)));
        }
    };

    let mut result = result?;
    result.execution_ms = start.elapsed().as_millis() as u64;

    tracing::info!(
        tool = %request.tool,
        rows = result.row_count,
        ms = result.execution_ms,
        "Tool executed"
    );

    Ok(Json(result))
}

/// Batch tool execution for agents
#[derive(Debug, Deserialize)]
pub struct BatchExecuteRequest {
    pub calls: Vec<ToolExecuteRequest>,
}

/// POST /api/tools/batch - Execute multiple tools concurrently
pub async fn batch_execute(
    State(state): State<AppState>,
    Json(request): Json<BatchExecuteRequest>,
) -> Result<Json<Vec<ToolResult>>> {
    if request.calls.len() > 10 {
        return Err(Error::Validation("Maximum 10 tool calls per batch".to_string()));
    }

    // Run tool calls concurrently, limited to 4 at a time to prevent pool starvation
    use futures::stream::StreamExt;
    let results: Vec<ToolResult> = futures::stream::iter(request.calls.into_iter().map(|call| {
        let state = state.clone();
        async move {
            let req = ToolExecuteRequest { tool: call.tool, params: call.params };
            match execute_tool(State(state), Json(req)).await {
                Ok(Json(result)) => result,
                Err(e) => ToolResult::err(e.to_string()),
            }
        }
    }))
    .buffer_unordered(4)
    .collect()
    .await;

    Ok(Json(results))
}

// ============================================================================
// Helpers
// ============================================================================

/// Cache for org slug → UUID resolution (60-second TTL)
static ORG_UUID_CACHE: std::sync::LazyLock<std::sync::RwLock<std::collections::HashMap<String, (uuid::Uuid, std::time::Instant)>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));

const ORG_CACHE_TTL_SECS: u64 = 60;

/// Resolve organization slug to UUID via api.organizations (cached 60s)
pub async fn resolve_org_uuid(
    pool: &std::sync::Arc<crate::postgres::PgPool>,
    org_slug: &str,
) -> Result<uuid::Uuid> {
    // Check cache first
    if let Ok(cache) = ORG_UUID_CACHE.read() {
        if let Some((uuid, ts)) = cache.get(org_slug) {
            if ts.elapsed().as_secs() < ORG_CACHE_TTL_SECS {
                return Ok(*uuid);
            }
        }
    }

    let client = pool.get().await?;
    let row = client
        .query_opt(
            "SELECT id FROM api.organizations WHERE lower(replace(name, ' ', '-')) = $1",
            &[&org_slug],
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to resolve org: {}", e)))?
        .ok_or_else(|| Error::Validation(format!("Organization not found: {}", org_slug)))?;

    let uuid_val: uuid::Uuid = row.get(0);

    // Store in cache
    if let Ok(mut cache) = ORG_UUID_CACHE.write() {
        cache.insert(org_slug.to_string(), (uuid_val, std::time::Instant::now()));
    }

    Ok(uuid_val)
}

/// Parse a UUID from a JSON value
pub fn parse_uuid(params: &Value, field: &str) -> Result<uuid::Uuid> {
    let s = params.get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Validation(format!("{} is required", field)))?;
    uuid::Uuid::parse_str(s)
        .map_err(|_| Error::Validation(format!("Invalid UUID for {}: {}", field, s)))
}

/// Parse an optional UUID from a JSON value
pub fn parse_uuid_opt(params: &Value, field: &str) -> Result<Option<uuid::Uuid>> {
    match params.get(field).and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => {
            let id = uuid::Uuid::parse_str(s)
                .map_err(|_| Error::Validation(format!("Invalid UUID for {}: {}", field, s)))?;
            Ok(Some(id))
        }
        _ => Ok(None),
    }
}

/// Parse an optional string from a JSON value
pub fn parse_str_opt<'a>(params: &'a Value, field: &str) -> Option<&'a str> {
    params.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

/// Parse limit with default and max
pub fn parse_limit(params: &Value, default: i64, max: i64) -> i64 {
    params.get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
        .clamp(1, max)
}

/// Parse offset with default 0
pub fn parse_offset(params: &Value) -> i64 {
    params.get("offset")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0)
}
