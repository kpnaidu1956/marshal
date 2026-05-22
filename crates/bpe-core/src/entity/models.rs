use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Field type enum for entity attribute definitions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Text,
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Email,
    Phone,
    Uuid,
    Currency,
    Enum(Vec<String>),
    JsonObject,
}

/// A field definition within an entity type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub label: String,
    #[serde(default)]
    pub required: bool,
    pub description: Option<String>,
    pub default_value: Option<serde_json::Value>,
}

/// An entity type (e.g. Employee, Supplier).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityType {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub core_fields: Vec<FieldDef>,
    pub custom_fields: Vec<FieldDef>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An entity instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub entity_type_id: Uuid,
    pub linked_user_id: Option<Uuid>,
    pub display_name: String,
    pub attributes: serde_json::Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Option<Uuid>,
    /// Joined from entity_types — included in responses.
    #[serde(skip_deserializing)]
    pub entity_type_name: Option<String>,
}

/// A relationship between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelationship {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub source_entity_id: Uuid,
    pub target_entity_id: Uuid,
    pub relationship_type: String,
    pub metadata: serde_json::Value,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Joined display names for API responses.
    #[serde(skip_deserializing)]
    pub source_display_name: Option<String>,
    #[serde(skip_deserializing)]
    pub target_display_name: Option<String>,
}

/// An interaction record for an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInteraction {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub entity_id: Uuid,
    pub interaction_type: String,
    pub source_type: Option<String>,
    pub source_id: Option<Uuid>,
    pub performed_by: Option<Uuid>,
    pub summary: String,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

// --- Request/Response DTOs ---

#[derive(Debug, Deserialize)]
pub struct CreateEntityTypeRequest {
    pub organization_id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub custom_fields: Option<Vec<FieldDef>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntityTypeRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub custom_fields: Option<Vec<FieldDef>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub organization_id: String,
    pub entity_type_id: Uuid,
    pub display_name: String,
    pub linked_user_id: Option<Uuid>,
    pub attributes: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntityRequest {
    pub display_name: Option<String>,
    pub attributes: Option<serde_json::Value>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRelationshipRequest {
    pub target_entity_id: Uuid,
    pub relationship_type: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInteractionRequest {
    pub interaction_type: String,
    pub summary: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ListEntitiesQuery {
    pub organization_id: String,
    pub entity_type: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
}
