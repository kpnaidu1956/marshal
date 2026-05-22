use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct StationManager;

impl StationManager {
    pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<Station>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT * FROM timekeeping.stations WHERE organization_id = $1 ORDER BY station_number",
            &[&org_id],
        ).await?;
        Ok(rows.iter().map(row_to_station).collect())
    }

    pub async fn get(pool: &PgPool, id: Uuid) -> Result<Station, BpeError> {
        let client = pool.get().await?;
        let row = client.query_opt(
            "SELECT * FROM timekeeping.stations WHERE id = $1",
            &[&id],
        ).await?
        .ok_or_else(|| BpeError::NotFound(format!("Station {id} not found")))?;
        Ok(row_to_station(&row))
    }

    pub async fn create(pool: &PgPool, org_id: Uuid, req: &CreateStationRequest) -> Result<Station, BpeError> {
        let client = pool.get().await?;
        let row = client.query_one(
            "INSERT INTO timekeeping.stations (organization_id, name, station_number, address, min_staffing)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING *",
            &[&org_id, &req.name, &req.station_number, &req.address, &req.min_staffing],
        ).await?;
        Ok(row_to_station(&row))
    }

    pub async fn update(pool: &PgPool, id: Uuid, req: &UpdateStationRequest) -> Result<Station, BpeError> {
        let client = pool.get().await?;
        let row = client.query_one(
            "UPDATE timekeeping.stations SET
                name = COALESCE($2, name),
                address = COALESCE($3, address),
                min_staffing = COALESCE($4, min_staffing),
                is_active = COALESCE($5, is_active),
                updated_at = now()
             WHERE id = $1
             RETURNING *",
            &[&id, &req.name, &req.address, &req.min_staffing, &req.is_active],
        ).await?;
        Ok(row_to_station(&row))
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client.execute(
            "UPDATE timekeeping.stations SET is_active = false, updated_at = now() WHERE id = $1",
            &[&id],
        ).await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Station {id} not found")));
        }
        Ok(())
    }
}

fn row_to_station(row: &tokio_postgres::Row) -> Station {
    Station {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        name: row.get("name"),
        station_number: row.get("station_number"),
        address: row.try_get("address").ok().flatten(),
        min_staffing: row.get("min_staffing"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
