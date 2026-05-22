use crate::db::PgPool;
use crate::entity::models::*;
use crate::error::BpeError;
use uuid::Uuid;

/// Manages entity types — system types, custom types, seeding.
pub struct EntityTypeRegistry;

impl EntityTypeRegistry {
    /// Seed system entity types for an organization if they don't exist.
    pub async fn seed_system_types(pool: &PgPool, org_id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;

        for def in SYSTEM_ENTITY_TYPES {
            let core_fields_json = serde_json::to_value((def.core_fields)())
                .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?;

            client
                .execute(
                    "INSERT INTO bpe.entity_types (organization_id, name, display_name, description, is_system, icon, color, core_fields)
                     VALUES ($1, $2, $3, $4, true, $5, $6, $7)
                     ON CONFLICT (organization_id, name) DO UPDATE SET core_fields = $7, updated_at = now()",
                    &[&org_id, &def.name, &def.display_name, &def.description, &def.icon, &def.color, &core_fields_json],
                )
                .await?;
        }

        tracing::info!("Seeded system entity types for org {org_id}");
        Ok(())
    }

    /// List all entity types for an organization.
    pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<EntityType>, BpeError> {
        let client = pool.get().await?;
        let rows = client
            .query(
                "SELECT id, organization_id, name, display_name, description, is_system, icon, color, core_fields, custom_fields, created_at, updated_at
                 FROM bpe.entity_types WHERE organization_id = $1 ORDER BY is_system DESC, name",
                &[&org_id],
            )
            .await?;

        Ok(rows.iter().map(|r| row_to_entity_type(r)).collect())
    }

    /// Get a single entity type by ID.
    pub async fn get(pool: &PgPool, id: Uuid) -> Result<EntityType, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, name, display_name, description, is_system, icon, color, core_fields, custom_fields, created_at, updated_at
                 FROM bpe.entity_types WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Entity type {id} not found")))?;

        Ok(row_to_entity_type(&row))
    }

    /// Create a custom entity type.
    pub async fn create(pool: &PgPool, org_id: Uuid, req: &CreateEntityTypeRequest) -> Result<EntityType, BpeError> {
        let client = pool.get().await?;
        let custom_fields = req.custom_fields.as_deref().unwrap_or(&[]);
        let custom_fields_json = serde_json::to_value(custom_fields)
            .map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?;

        let row = client
            .query_one(
                "INSERT INTO bpe.entity_types (organization_id, name, display_name, description, is_system, icon, color, custom_fields)
                 VALUES ($1, $2, $3, $4, false, $5, $6, $7)
                 RETURNING id, organization_id, name, display_name, description, is_system, icon, color, core_fields, custom_fields, created_at, updated_at",
                &[&org_id, &req.name, &req.display_name, &req.description, &req.icon, &req.color, &custom_fields_json],
            )
            .await
            .map_err(|e| {
                if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
                    BpeError::Conflict(format!("Entity type '{}' already exists", req.name))
                } else {
                    BpeError::from(e)
                }
            })?;

        Ok(row_to_entity_type(&row))
    }

    /// Update a custom entity type.
    pub async fn update(pool: &PgPool, id: Uuid, req: &UpdateEntityTypeRequest) -> Result<EntityType, BpeError> {
        let client = pool.get().await?;

        // Check it's not a system type — inline lookup using same connection
        let existing = {
            let row = client
                .query_opt(
                    "SELECT id, organization_id, name, display_name, description, is_system, icon, color, core_fields, custom_fields, created_at, updated_at
                     FROM bpe.entity_types WHERE id = $1",
                    &[&id],
                )
                .await?
                .ok_or_else(|| BpeError::NotFound(format!("Entity type {id} not found")))?;
            row_to_entity_type(&row)
        };
        if existing.is_system {
            return Err(BpeError::Forbidden("Cannot modify system entity types".into()));
        }

        let display_name = req.display_name.as_deref().unwrap_or(&existing.display_name);
        let description = req.description.as_ref().or(existing.description.as_ref());
        let icon = req.icon.as_ref().or(existing.icon.as_ref());
        let color = req.color.as_ref().or(existing.color.as_ref());
        let custom_fields_json = if let Some(cf) = &req.custom_fields {
            serde_json::to_value(cf).map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?
        } else {
            serde_json::to_value(&existing.custom_fields).map_err(|e| BpeError::Internal(format!("JSON error: {e}")))?
        };

        let row = client
            .query_one(
                "UPDATE bpe.entity_types SET display_name=$1, description=$2, icon=$3, color=$4, custom_fields=$5, updated_at=now()
                 WHERE id=$6
                 RETURNING id, organization_id, name, display_name, description, is_system, icon, color, core_fields, custom_fields, created_at, updated_at",
                &[&display_name, &description, &icon, &color, &custom_fields_json, &id],
            )
            .await?;

        Ok(row_to_entity_type(&row))
    }

    /// Delete a custom entity type (must have no entities).
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;

        // Inline lookup using same connection
        let existing = {
            let row = client
                .query_opt(
                    "SELECT id, organization_id, name, display_name, description, is_system, icon, color, core_fields, custom_fields, created_at, updated_at
                     FROM bpe.entity_types WHERE id = $1",
                    &[&id],
                )
                .await?
                .ok_or_else(|| BpeError::NotFound(format!("Entity type {id} not found")))?;
            row_to_entity_type(&row)
        };
        if existing.is_system {
            return Err(BpeError::Forbidden("Cannot delete system entity types".into()));
        }

        // Check no entities use this type
        let count: i64 = client
            .query_one("SELECT count(*) FROM bpe.entities WHERE entity_type_id = $1", &[&id])
            .await?
            .get(0);

        if count > 0 {
            return Err(BpeError::Conflict(format!("{count} entities use this type — archive them first")));
        }

        client.execute("DELETE FROM bpe.entity_types WHERE id = $1", &[&id]).await?;
        Ok(())
    }

    /// Resolve organization slug (or UUID string) to UUID.
    /// Uses a single pool connection for both UUID and slug lookup paths,
    /// avoiding redundant connection checkouts.
    pub async fn resolve_org_id(pool: &PgPool, slug: &str) -> Result<Uuid, BpeError> {
        let client = pool.get().await?;

        // If already a valid UUID, verify it exists and return directly
        if let Ok(uuid) = slug.parse::<Uuid>() {
            let exists = client
                .query_opt("SELECT id FROM api.organizations WHERE id = $1", &[&uuid])
                .await?;
            if exists.is_some() {
                return Ok(uuid);
            }
            // Fall through to slug lookup in case the UUID string happens to match a slug
        }

        let row = client
            .query_opt(
                "SELECT id FROM api.organizations WHERE lower(replace(name, ' ', '-')) = $1",
                &[&slug],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Organization '{slug}' not found")))?;

        Ok(row.get(0))
    }
}

fn row_to_entity_type(row: &tokio_postgres::Row) -> EntityType {
    let core_fields_json: serde_json::Value = row.get("core_fields");
    let custom_fields_json: serde_json::Value = row.get("custom_fields");

    EntityType {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        name: row.get("name"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        is_system: row.get("is_system"),
        icon: row.get("icon"),
        color: row.get("color"),
        core_fields: serde_json::from_value(core_fields_json).unwrap_or_default(),
        custom_fields: serde_json::from_value(custom_fields_json).unwrap_or_default(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// --- System entity type definitions ---

struct SystemTypeDef {
    name: &'static str,
    display_name: &'static str,
    description: &'static str,
    icon: &'static str,
    color: &'static str,
    core_fields: fn() -> Vec<FieldDef>,
}

fn field(name: &str, field_type: FieldType, label: &str, required: bool) -> FieldDef {
    FieldDef {
        name: name.into(),
        field_type,
        label: label.into(),
        required,
        description: None,
        default_value: None,
    }
}

fn employee_fields() -> Vec<FieldDef> {
    vec![
        field("first_name", FieldType::String, "First Name", true),
        field("last_name", FieldType::String, "Last Name", true),
        field("email", FieldType::Email, "Email", true),
        field("position", FieldType::String, "Position / Title", false),
        field("department", FieldType::String, "Department", false),
        field("grade", FieldType::String, "Grade / Level", false),
        field("hire_date", FieldType::Date, "Hire Date", false),
        field("manager_entity_id", FieldType::Uuid, "Manager (Entity ID)", false),
    ]
}

fn supplier_fields() -> Vec<FieldDef> {
    vec![
        field("company_name", FieldType::String, "Company Name", true),
        field("contact_name", FieldType::String, "Contact Name", false),
        field("email", FieldType::Email, "Email", false),
        field("phone", FieldType::Phone, "Phone", false),
        field("abn", FieldType::String, "ABN / Tax ID", false),
        field("payment_terms", FieldType::String, "Payment Terms", false),
        field("category", FieldType::String, "Category", false),
    ]
}

fn customer_fields() -> Vec<FieldDef> {
    vec![
        field("company_name", FieldType::String, "Company Name", true),
        field("contact_name", FieldType::String, "Contact Name", false),
        field("email", FieldType::Email, "Email", false),
        field("phone", FieldType::Phone, "Phone", false),
        field("account_number", FieldType::String, "Account Number", false),
        field("billing_address", FieldType::Text, "Billing Address", false),
    ]
}

fn shareholder_fields() -> Vec<FieldDef> {
    vec![
        field("name", FieldType::String, "Full Name", true),
        field("email", FieldType::Email, "Email", false),
        field("share_count", FieldType::Integer, "Share Count", false),
        field("share_class", FieldType::String, "Share Class", false),
        field("acquisition_date", FieldType::Date, "Acquisition Date", false),
    ]
}

fn contractor_fields() -> Vec<FieldDef> {
    vec![
        field("name", FieldType::String, "Full Name", true),
        field("company_name", FieldType::String, "Company Name", false),
        field("email", FieldType::Email, "Email", false),
        field("abn", FieldType::String, "ABN / Tax ID", false),
        field("contract_start", FieldType::Date, "Contract Start", false),
        field("contract_end", FieldType::Date, "Contract End", false),
        field("hourly_rate", FieldType::Decimal, "Hourly Rate", false),
    ]
}

static SYSTEM_ENTITY_TYPES: &[SystemTypeDef] = &[
    SystemTypeDef {
        name: "employee",
        display_name: "Employee",
        description: "Organization employee",
        icon: "user",
        color: "#3b82f6",
        core_fields: employee_fields,
    },
    SystemTypeDef {
        name: "supplier",
        display_name: "Supplier / Vendor",
        description: "External supplier or vendor",
        icon: "truck",
        color: "#f59e0b",
        core_fields: supplier_fields,
    },
    SystemTypeDef {
        name: "customer",
        display_name: "Customer",
        description: "Customer or client",
        icon: "building",
        color: "#10b981",
        core_fields: customer_fields,
    },
    SystemTypeDef {
        name: "shareholder",
        display_name: "Shareholder",
        description: "Company shareholder or investor",
        icon: "landmark",
        color: "#8b5cf6",
        core_fields: shareholder_fields,
    },
    SystemTypeDef {
        name: "contractor",
        display_name: "Contractor",
        description: "Independent contractor",
        icon: "hard-hat",
        color: "#ef4444",
        core_fields: contractor_fields,
    },
];
