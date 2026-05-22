use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Domain models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationCredential {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub integration_type: String,
    pub name: String,
    // encrypted_credentials and encryption_nonce are never exposed via API
    pub last_test_at: Option<DateTime<Utc>>,
    pub last_test_success: Option<bool>,
    pub last_test_error: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Option<Uuid>,
}

/// Summary of a credential without sensitive data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSummary {
    pub id: Uuid,
    pub integration_type: String,
    pub name: String,
    pub is_active: bool,
    pub last_test_at: Option<DateTime<Utc>>,
    pub last_test_success: Option<bool>,
    pub last_test_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Result of executing an integration step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
    pub duration_ms: i64,
}

// --- Request DTOs ---

#[derive(Debug, Deserialize)]
pub struct CreateCredentialRequest {
    pub organization_id: String,
    pub integration_type: String,
    pub name: String,
    pub credentials: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCredentialRequest {
    pub name: Option<String>,
    pub credentials: Option<serde_json::Value>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    pub organization_id: String,
    pub integration_type: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteIntegrationRequest {
    pub organization_id: String,
    pub integration_type: String,
    pub credential_id: Option<Uuid>,
    pub action: String,
    pub parameters: Option<serde_json::Value>,
}

/// Supported integration types and their available actions.
#[derive(Debug, Clone, Serialize)]
pub struct IntegrationType {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub actions: &'static [&'static str],
    pub credential_fields: &'static [&'static str],
}

pub const INTEGRATION_TYPES: &[IntegrationType] = &[
    IntegrationType {
        name: "webhook",
        display_name: "Webhook",
        description: "Send HTTP requests to external endpoints",
        actions: &["post", "get", "put", "delete"],
        credential_fields: &["url", "headers"],
    },
    IntegrationType {
        name: "email",
        display_name: "Email (SMTP)",
        description: "Send emails via SMTP",
        actions: &["send"],
        credential_fields: &["host", "port", "username", "password", "from"],
    },
    IntegrationType {
        name: "slack",
        display_name: "Slack",
        description: "Post messages and interact with Slack",
        actions: &["post_message", "create_channel", "invite_user"],
        credential_fields: &["bot_token", "webhook_url"],
    },
    IntegrationType {
        name: "jira",
        display_name: "Jira",
        description: "Create and manage Jira issues",
        actions: &["create_issue", "update_issue", "transition_issue", "add_comment"],
        credential_fields: &["base_url", "email", "api_token"],
    },
    IntegrationType {
        name: "github",
        display_name: "GitHub",
        description: "Interact with GitHub repositories",
        actions: &["create_issue", "create_pr", "add_comment", "add_label"],
        credential_fields: &["token", "owner", "repo"],
    },
    IntegrationType {
        name: "custom_api",
        display_name: "Custom API",
        description: "Call any REST API endpoint",
        actions: &["request"],
        credential_fields: &["base_url", "auth_type", "auth_value"],
    },
    IntegrationType {
        name: "ruflo_agent",
        display_name: "Ruflo AI Agent",
        description: "Execute tasks via Ruflo AI agent orchestration",
        actions: &["spawn", "research", "generate", "review", "plan", "analyze"],
        credential_fields: &[],
    },
    IntegrationType {
        name: "quickbooks",
        display_name: "QuickBooks Online",
        description: "Sync invoices, expenses, payroll, and accounts with QuickBooks Online",
        actions: &["create_invoice", "update_invoice", "create_expense", "query_accounts", "sync_payroll"],
        credential_fields: &["realm_id", "client_id", "client_secret", "access_token", "refresh_token"],
    },
    IntegrationType {
        name: "netsuite",
        display_name: "NetSuite ERP",
        description: "Manage transactions, records, and GL entries in Oracle NetSuite",
        actions: &["create_transaction", "update_record", "search", "get_record", "delete_record"],
        credential_fields: &["account_id", "consumer_key", "consumer_secret", "token_id", "token_secret"],
    },
];

/// Request to spawn a Ruflo agent for a workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RufloAgentRequest {
    pub agent_type: String,
    pub prompt: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub context: serde_json::Value,
    /// Callback URL for the agent to POST results back to
    pub callback_url: Option<String>,
}

/// Response from Ruflo agent spawn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RufloAgentResponse {
    pub agent_id: String,
    pub status: String,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
}
