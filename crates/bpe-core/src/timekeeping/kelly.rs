use chrono::NaiveDate;
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct KellySchedule;

impl KellySchedule {
    /// Get active Kelly schedule config for an organization.
    pub async fn get_config(pool: &PgPool, org_id: Uuid) -> Result<KellyScheduleConfig, BpeError> {
        let client = pool.get().await?;
        let row = client.query_opt(
            "SELECT * FROM timekeeping.kelly_schedule_config
             WHERE organization_id = $1 AND is_active = true",
            &[&org_id],
        ).await?
        .ok_or_else(|| BpeError::NotFound("No active Kelly schedule config found".into()))?;
        Ok(row_to_kelly_config(&row))
    }

    /// Upsert Kelly schedule config (deactivates previous).
    pub async fn upsert_config(pool: &PgPool, org_id: Uuid, req: &UpsertKellyConfigRequest) -> Result<KellyScheduleConfig, BpeError> {
        if req.cycle_length < 1 || req.cycle_length > 30 {
            return Err(BpeError::BadRequest("cycle_length must be 1-30".into()));
        }
        if req.shift_labels.is_empty() {
            return Err(BpeError::BadRequest("shift_labels cannot be empty".into()));
        }
        if req.rotation_pattern.len() != req.cycle_length as usize {
            return Err(BpeError::BadRequest(format!(
                "rotation_pattern length ({}) must equal cycle_length ({})",
                req.rotation_pattern.len(), req.cycle_length
            )));
        }
        for &idx in &req.rotation_pattern {
            if idx < 0 || idx >= req.shift_labels.len() as i32 {
                return Err(BpeError::BadRequest(format!(
                    "rotation_pattern index {} out of range for {} shift labels",
                    idx, req.shift_labels.len()
                )));
            }
        }

        let client = pool.get().await?;

        // Deactivate existing
        client.execute(
            "UPDATE timekeeping.kelly_schedule_config SET is_active = false WHERE organization_id = $1",
            &[&org_id],
        ).await?;

        let row = client.query_one(
            "INSERT INTO timekeeping.kelly_schedule_config
                (organization_id, epoch_date, cycle_length, shift_labels,
                 rotation_pattern, shift_start_time, shift_duration_hours)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING *",
            &[
                &org_id, &req.epoch_date, &req.cycle_length,
                &req.shift_labels, &req.rotation_pattern,
                &req.shift_start_time, &req.shift_duration_hours,
            ],
        ).await?;

        Ok(row_to_kelly_config(&row))
    }

    /// Compute which shift is on duty for a given date.
    pub fn compute_shift_for_date(config: &KellyScheduleConfig, date: NaiveDate) -> String {
        let days_since = (date - config.epoch_date).num_days();
        let cycle_len = config.cycle_length as i64;
        // Handle dates before epoch with positive modulo
        let cycle_day = ((days_since % cycle_len + cycle_len) % cycle_len) as usize;
        let shift_index = config.rotation_pattern[cycle_day] as usize;
        config.shift_labels[shift_index].clone()
    }

    /// Compute schedule for a date range.
    pub fn compute_range(config: &KellyScheduleConfig, start: NaiveDate, end: NaiveDate) -> Vec<KellyDayInfo> {
        let mut result = Vec::new();
        let mut date = start;
        while date <= end {
            result.push(KellyDayInfo {
                date,
                on_duty_shift: Self::compute_shift_for_date(config, date),
            });
            date = date.succ_opt().unwrap_or(date);
        }
        result
    }
}

fn row_to_kelly_config(row: &tokio_postgres::Row) -> KellyScheduleConfig {
    KellyScheduleConfig {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        epoch_date: row.get("epoch_date"),
        cycle_length: row.get("cycle_length"),
        shift_labels: row.get("shift_labels"),
        rotation_pattern: row.get("rotation_pattern"),
        shift_start_time: row.get("shift_start_time"),
        shift_duration_hours: row.get("shift_duration_hours"),
        is_active: row.get("is_active"),
    }
}
