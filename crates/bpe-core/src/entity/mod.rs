pub mod attributes;
pub mod interactions;
pub mod models;
pub mod registry;
pub mod relationships;

use crate::db::PgPool;
use crate::error::BpeError;
use models::*;
use uuid::Uuid;

/// Core entity CRUD operations.
pub struct EntityManager;

impl EntityManager {
    /// Create a new entity.
    pub async fn create(
        pool: &PgPool,
        org_id: Uuid,
        created_by: Uuid,
        req: &CreateEntityRequest,
    ) -> Result<Entity, BpeError> {
        // Validate entity type exists and belongs to org
        let entity_type = registry::EntityTypeRegistry::get(pool, req.entity_type_id).await?;
        if entity_type.organization_id != org_id {
            return Err(BpeError::Forbidden("Entity type does not belong to this organization".into()));
        }

        // Validate attributes against field definitions
        let attrs = req.attributes.clone().unwrap_or(serde_json::json!({}));
        let all_fields: Vec<FieldDef> = entity_type
            .core_fields
            .iter()
            .chain(entity_type.custom_fields.iter())
            .cloned()
            .collect();
        attributes::validate_attributes(&all_fields, &attrs)?;

        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.entities (organization_id, entity_type_id, linked_user_id, display_name, attributes, created_by)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id, organization_id, entity_type_id, linked_user_id, display_name, attributes, status, created_at, updated_at, created_by",
                &[&org_id, &req.entity_type_id, &req.linked_user_id, &req.display_name, &attrs, &created_by],
            )
            .await?;

        let mut entity = row_to_entity(&row);
        entity.entity_type_name = Some(entity_type.name);
        Ok(entity)
    }

    /// Get entity by ID with type name.
    pub async fn get(pool: &PgPool, id: Uuid) -> Result<Entity, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT e.id, e.organization_id, e.entity_type_id, e.linked_user_id, e.display_name,
                        e.attributes, e.status, e.created_at, e.updated_at, e.created_by,
                        et.name AS entity_type_name
                 FROM bpe.entities e
                 JOIN bpe.entity_types et ON et.id = e.entity_type_id
                 WHERE e.id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Entity {id} not found")))?;

        let mut entity = row_to_entity(&row);
        entity.entity_type_name = row.try_get("entity_type_name").ok();
        Ok(entity)
    }

    /// List entities with filtering and pagination.
    /// Uses nullable parameter pattern to collapse 4 query branches into 1.
    pub async fn list(pool: &PgPool, org_id: Uuid, query: &ListEntitiesQuery) -> Result<PaginatedResponse<Entity>, BpeError> {
        let client = pool.get().await?;
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;
        let status_val = query.status.clone().unwrap_or_else(|| "active".into());
        let search_pattern: Option<String> = query.search.as_ref().map(|s| format!("%{s}%"));

        let total: i64 = client.query_one(
            "SELECT count(*)
             FROM bpe.entities e
             JOIN bpe.entity_types et ON et.id = e.entity_type_id
             WHERE e.organization_id = $1
               AND e.status = $2
               AND ($3::text IS NULL OR et.name = $3)
               AND ($4::text IS NULL OR e.display_name ILIKE $4)",
            &[&org_id, &status_val, &query.entity_type, &search_pattern],
        ).await?.get(0);

        let rows = client.query(
            "SELECT e.id, e.organization_id, e.entity_type_id, e.linked_user_id,
                    e.display_name, e.attributes, e.status, e.created_at, e.updated_at,
                    e.created_by, et.name AS entity_type_name
             FROM bpe.entities e
             JOIN bpe.entity_types et ON et.id = e.entity_type_id
             WHERE e.organization_id = $1
               AND e.status = $2
               AND ($3::text IS NULL OR et.name = $3)
               AND ($4::text IS NULL OR e.display_name ILIKE $4)
             ORDER BY e.display_name
             LIMIT $5 OFFSET $6",
            &[&org_id, &status_val, &query.entity_type, &search_pattern, &per_page, &offset],
        ).await?;

        Ok(build_paginated(rows, page, per_page, total))
    }

    /// Update an entity.
    pub async fn update(pool: &PgPool, id: Uuid, req: &UpdateEntityRequest) -> Result<Entity, BpeError> {
        let existing = Self::get(pool, id).await?;

        let display_name = req.display_name.as_deref().unwrap_or(&existing.display_name);
        let status = req.status.as_deref().unwrap_or(&existing.status);
        let attributes = req.attributes.as_ref().unwrap_or(&existing.attributes);

        // If attributes changed, validate them
        if req.attributes.is_some() {
            let entity_type = registry::EntityTypeRegistry::get(pool, existing.entity_type_id).await?;
            let all_fields: Vec<FieldDef> = entity_type
                .core_fields
                .iter()
                .chain(entity_type.custom_fields.iter())
                .cloned()
                .collect();
            attributes::validate_attributes(&all_fields, attributes)?;
        }

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.entities SET display_name=$1, attributes=$2, status=$3, updated_at=now()
                 WHERE id=$4
                 RETURNING id, organization_id, entity_type_id, linked_user_id, display_name, attributes, status, created_at, updated_at, created_by",
                &[&display_name, attributes, &status, &id],
            )
            .await?;

        Ok(row_to_entity(&row))
    }

    /// Soft-delete (archive) an entity.
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute(
                "UPDATE bpe.entities SET status = 'archived', updated_at = now() WHERE id = $1",
                &[&id],
            )
            .await?;

        if n == 0 {
            return Err(BpeError::NotFound(format!("Entity {id} not found")));
        }
        Ok(())
    }
}

fn build_paginated(rows: Vec<tokio_postgres::Row>, page: i64, per_page: i64, total: i64) -> PaginatedResponse<Entity> {
    let data = rows
        .iter()
        .map(|r| {
            let mut entity = row_to_entity(r);
            entity.entity_type_name = r.try_get("entity_type_name").ok();
            entity
        })
        .collect();
    PaginatedResponse { data, page, per_page, total }
}

fn row_to_entity(row: &tokio_postgres::Row) -> Entity {
    Entity {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        entity_type_id: row.get("entity_type_id"),
        linked_user_id: row.get("linked_user_id"),
        display_name: row.get("display_name"),
        attributes: row.get("attributes"),
        status: row.get("status"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        created_by: row.get("created_by"),
        entity_type_name: None,
    }
}
