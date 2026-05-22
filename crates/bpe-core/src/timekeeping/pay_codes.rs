use rust_decimal::prelude::FromPrimitive;
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct PayCodeManager;

impl PayCodeManager {
    pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<PayCode>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT * FROM timekeeping.pay_codes WHERE organization_id = $1 ORDER BY sort_order, display_name",
            &[&org_id],
        ).await?;
        Ok(rows.iter().map(row_to_pay_code).collect())
    }

    pub async fn create(pool: &PgPool, org_id: Uuid, req: &CreatePayCodeRequest) -> Result<PayCode, BpeError> {
        if !matches!(req.category.as_str(), "work" | "leave") {
            return Err(BpeError::BadRequest("category must be 'work' or 'leave'".into()));
        }
        let mult_dec = rust_decimal::Decimal::from_f64(req.multiplier)
            .unwrap_or(rust_decimal::Decimal::ONE);
        let client = pool.get().await?;
        let row = client.query_one(
            "INSERT INTO timekeeping.pay_codes
                (organization_id, code, display_name, category, multiplier,
                 is_overtime, counts_toward_flsa, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING *",
            &[
                &org_id, &req.code.to_uppercase(), &req.display_name, &req.category,
                &mult_dec, &req.is_overtime, &req.counts_toward_flsa, &req.sort_order,
            ],
        ).await?;
        Ok(row_to_pay_code(&row))
    }

    pub async fn update(pool: &PgPool, id: Uuid, req: &UpdatePayCodeRequest) -> Result<PayCode, BpeError> {
        let client = pool.get().await?;
        let mult_dec = req.multiplier.map(|m| rust_decimal::Decimal::from_f64(m)).flatten();
        let row = client.query_one(
            "UPDATE timekeeping.pay_codes SET
                display_name = COALESCE($2, display_name),
                multiplier = COALESCE($3, multiplier),
                is_overtime = COALESCE($4, is_overtime),
                counts_toward_flsa = COALESCE($5, counts_toward_flsa),
                is_active = COALESCE($6, is_active),
                sort_order = COALESCE($7, sort_order),
                updated_at = now()
             WHERE id = $1
             RETURNING *",
            &[&id, &req.display_name, &mult_dec, &req.is_overtime,
              &req.counts_toward_flsa, &req.is_active, &req.sort_order],
        ).await?;
        Ok(row_to_pay_code(&row))
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "UPDATE timekeeping.pay_codes SET is_active = false, updated_at = now() WHERE id = $1",
            &[&id],
        ).await?;
        Ok(())
    }
}

fn row_to_pay_code(row: &tokio_postgres::Row) -> PayCode {
    // PostgreSQL numeric -> read as rust_decimal or string, convert to f64
    let multiplier_str: rust_decimal::Decimal = row.get("multiplier");
    PayCode {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        code: row.get("code"),
        display_name: row.get("display_name"),
        category: row.get("category"),
        multiplier: multiplier_str.to_string().parse::<f64>().unwrap_or(1.0),
        is_overtime: row.get("is_overtime"),
        counts_toward_flsa: row.get("counts_toward_flsa"),
        is_active: row.get("is_active"),
        sort_order: row.get("sort_order"),
    }
}
