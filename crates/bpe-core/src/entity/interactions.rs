use crate::db::PgPool;
use crate::entity::models::*;
use crate::error::BpeError;
use uuid::Uuid;

/// Records and queries entity interactions.
pub struct InteractionTracker;

impl InteractionTracker {
    /// Record an interaction for an entity.
    pub async fn record(
        pool: &PgPool,
        org_id: Uuid,
        entity_id: Uuid,
        performed_by: Option<Uuid>,
        req: &CreateInteractionRequest,
    ) -> Result<EntityInteraction, BpeError> {
        let client = pool.get().await?;
        let details = req.details.clone().unwrap_or(serde_json::json!({}));

        let row = client
            .query_one(
                "INSERT INTO bpe.entity_interactions (organization_id, entity_id, interaction_type, source_type, performed_by, summary, details)
                 VALUES ($1, $2, $3, 'manual', $4, $5, $6)
                 RETURNING id, organization_id, entity_id, interaction_type, source_type, source_id, performed_by, summary, details, created_at",
                &[&org_id, &entity_id, &req.interaction_type, &performed_by, &req.summary, &details],
            )
            .await?;

        Ok(row_to_interaction(&row))
    }

    /// List interactions for an entity with pagination.
    pub async fn list(
        pool: &PgPool,
        entity_id: Uuid,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedResponse<EntityInteraction>, BpeError> {
        let client = pool.get().await?;
        let offset = (page - 1) * per_page;

        let total: i64 = client
            .query_one(
                "SELECT count(*) FROM bpe.entity_interactions WHERE entity_id = $1",
                &[&entity_id],
            )
            .await?
            .get(0);

        let rows = client
            .query(
                "SELECT id, organization_id, entity_id, interaction_type, source_type, source_id, performed_by, summary, details, created_at
                 FROM bpe.entity_interactions WHERE entity_id = $1
                 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
                &[&entity_id, &per_page, &offset],
            )
            .await?;

        Ok(PaginatedResponse {
            data: rows.iter().map(|r| row_to_interaction(r)).collect(),
            page,
            per_page,
            total,
        })
    }
}

fn row_to_interaction(row: &tokio_postgres::Row) -> EntityInteraction {
    EntityInteraction {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        entity_id: row.get("entity_id"),
        interaction_type: row.get("interaction_type"),
        source_type: row.get("source_type"),
        source_id: row.get("source_id"),
        performed_by: row.get("performed_by"),
        summary: row.get("summary"),
        details: row.get("details"),
        created_at: row.get("created_at"),
    }
}
