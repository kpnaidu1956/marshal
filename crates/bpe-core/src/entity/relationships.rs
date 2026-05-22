use crate::db::PgPool;
use crate::entity::models::*;
use crate::error::BpeError;
use uuid::Uuid;

/// Manages relationships between entities.
pub struct RelationshipManager;

impl RelationshipManager {
    /// Add a relationship between two entities.
    pub async fn add(
        pool: &PgPool,
        org_id: Uuid,
        source_entity_id: Uuid,
        req: &CreateRelationshipRequest,
    ) -> Result<EntityRelationship, BpeError> {
        if source_entity_id == req.target_entity_id {
            return Err(BpeError::BadRequest("Cannot create self-relationship".into()));
        }

        let client = pool.get().await?;
        let metadata = req.metadata.clone().unwrap_or(serde_json::json!({}));

        let row = client
            .query_one(
                "INSERT INTO bpe.entity_relationships (organization_id, source_entity_id, target_entity_id, relationship_type, metadata)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING id, organization_id, source_entity_id, target_entity_id, relationship_type, metadata, is_active, created_at, updated_at",
                &[&org_id, &source_entity_id, &req.target_entity_id, &req.relationship_type, &metadata],
            )
            .await?;

        Ok(row_to_relationship(&row))
    }

    /// List relationships for an entity (both directions).
    pub async fn list_for_entity(pool: &PgPool, entity_id: Uuid) -> Result<Vec<EntityRelationship>, BpeError> {
        let client = pool.get().await?;
        let rows = client
            .query(
                "SELECT r.id, r.organization_id, r.source_entity_id, r.target_entity_id,
                        r.relationship_type, r.metadata, r.is_active, r.created_at, r.updated_at,
                        s.display_name AS source_display_name, t.display_name AS target_display_name
                 FROM bpe.entity_relationships r
                 JOIN bpe.entities s ON s.id = r.source_entity_id
                 JOIN bpe.entities t ON t.id = r.target_entity_id
                 WHERE (r.source_entity_id = $1 OR r.target_entity_id = $1) AND r.is_active = true
                 ORDER BY r.created_at DESC",
                &[&entity_id],
            )
            .await?;

        Ok(rows.iter().map(|r| {
            let mut rel = row_to_relationship(r);
            rel.source_display_name = r.try_get("source_display_name").ok();
            rel.target_display_name = r.try_get("target_display_name").ok();
            rel
        }).collect())
    }

    /// Remove a relationship.
    pub async fn remove(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute("UPDATE bpe.entity_relationships SET is_active = false, updated_at = now() WHERE id = $1", &[&id])
            .await?;

        if n == 0 {
            return Err(BpeError::NotFound(format!("Relationship {id} not found")));
        }
        Ok(())
    }
}

fn row_to_relationship(row: &tokio_postgres::Row) -> EntityRelationship {
    EntityRelationship {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        source_entity_id: row.get("source_entity_id"),
        target_entity_id: row.get("target_entity_id"),
        relationship_type: row.get("relationship_type"),
        metadata: row.get("metadata"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        source_display_name: None,
        target_display_name: None,
    }
}
