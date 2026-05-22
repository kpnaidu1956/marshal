use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Report models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTemplate {
    pub id: Uuid,
    pub organization_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub sql_template: String,
    pub parameters: serde_json::Value,
    pub columns: serde_json::Value,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportResult {
    pub template_id: Uuid,
    pub template_name: String,
    pub columns: serde_json::Value,
    pub rows: Vec<serde_json::Value>,
    pub row_count: usize,
    pub generated_at: DateTime<Utc>,
}

// --- Notification models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub recipient_user_id: Uuid,
    pub source_type: String,
    pub source_id: Uuid,
    pub title: String,
    pub body: Option<String>,
    pub channel: String,
    pub is_read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub email_sent: bool,
    pub email_sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// --- Request DTOs ---

#[derive(Debug, Deserialize)]
pub struct CreateReportTemplateRequest {
    pub organization_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub sql_template: String,
    pub parameters: Option<serde_json::Value>,
    pub columns: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateReportTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub sql_template: Option<String>,
    pub parameters: Option<serde_json::Value>,
    pub columns: Option<serde_json::Value>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct RunReportRequest {
    pub organization_id: String,
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ReportListQuery {
    pub organization_id: Option<String>,
    pub category: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateNotificationRequest {
    pub organization_id: String,
    pub recipient_user_id: Uuid,
    pub source_type: String,
    pub source_id: Uuid,
    pub title: String,
    pub body: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationListQuery {
    pub organization_id: String,
    pub unread_only: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MarkReadRequest {
    pub notification_ids: Vec<Uuid>,
}
