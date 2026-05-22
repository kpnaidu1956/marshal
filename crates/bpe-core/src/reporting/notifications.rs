use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct NotificationEngine;

impl NotificationEngine {
    /// Send a notification to a user.
    pub async fn create(
        pool: &PgPool,
        org_id: Uuid,
        req: &CreateNotificationRequest,
    ) -> Result<Notification, BpeError> {
        let channel = req.channel.as_deref().unwrap_or("in_app");
        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.notifications
                    (organization_id, recipient_user_id, source_type, source_id,
                     title, body, channel)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, organization_id, recipient_user_id, source_type, source_id,
                           title, body, channel, is_read, read_at,
                           email_sent, email_sent_at, created_at",
                &[
                    &org_id, &req.recipient_user_id, &req.source_type, &req.source_id,
                    &req.title, &req.body, &channel,
                ],
            )
            .await?;

        Ok(row_to_notification(&row))
    }

    /// List notifications for a user.
    pub async fn list_for_user(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
        unread_only: bool,
        page: i64,
        per_page: i64,
    ) -> Result<(Vec<Notification>, i64), BpeError> {
        let client = pool.get().await?;
        let offset = (page - 1) * per_page;

        let (count_row, rows) = if unread_only {
            let c = client.query_one(
                "SELECT count(*) FROM bpe.notifications
                 WHERE organization_id = $1 AND recipient_user_id = $2 AND is_read = false",
                &[&org_id, &user_id],
            ).await?;
            let r = client.query(
                "SELECT id, organization_id, recipient_user_id, source_type, source_id,
                        title, body, channel, is_read, read_at,
                        email_sent, email_sent_at, created_at
                 FROM bpe.notifications
                 WHERE organization_id = $1 AND recipient_user_id = $2 AND is_read = false
                 ORDER BY created_at DESC
                 LIMIT $3 OFFSET $4",
                &[&org_id, &user_id, &per_page, &offset],
            ).await?;
            (c, r)
        } else {
            let c = client.query_one(
                "SELECT count(*) FROM bpe.notifications
                 WHERE organization_id = $1 AND recipient_user_id = $2",
                &[&org_id, &user_id],
            ).await?;
            let r = client.query(
                "SELECT id, organization_id, recipient_user_id, source_type, source_id,
                        title, body, channel, is_read, read_at,
                        email_sent, email_sent_at, created_at
                 FROM bpe.notifications
                 WHERE organization_id = $1 AND recipient_user_id = $2
                 ORDER BY created_at DESC
                 LIMIT $3 OFFSET $4",
                &[&org_id, &user_id, &per_page, &offset],
            ).await?;
            (c, r)
        };

        let total: i64 = count_row.get(0);
        let data = rows.iter().map(row_to_notification).collect();
        Ok((data, total))
    }

    /// Get unread count for a user.
    pub async fn unread_count(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<i64, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_one(
                "SELECT count(*) FROM bpe.notifications
                 WHERE organization_id = $1 AND recipient_user_id = $2 AND is_read = false",
                &[&org_id, &user_id],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Mark notifications as read.
    pub async fn mark_read(
        pool: &PgPool,
        user_id: Uuid,
        notification_ids: &[Uuid],
    ) -> Result<i64, BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute(
                "UPDATE bpe.notifications
                 SET is_read = true, read_at = now()
                 WHERE recipient_user_id = $1 AND id = ANY($2) AND is_read = false",
                &[&user_id, &notification_ids],
            )
            .await?;
        Ok(n as i64)
    }

    /// Mark all notifications as read for a user.
    pub async fn mark_all_read(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<i64, BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute(
                "UPDATE bpe.notifications
                 SET is_read = true, read_at = now()
                 WHERE organization_id = $1 AND recipient_user_id = $2 AND is_read = false",
                &[&org_id, &user_id],
            )
            .await?;
        Ok(n as i64)
    }
}

fn row_to_notification(row: &tokio_postgres::Row) -> Notification {
    Notification {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        recipient_user_id: row.get("recipient_user_id"),
        source_type: row.get("source_type"),
        source_id: row.get("source_id"),
        title: row.get("title"),
        body: row.get("body"),
        channel: row.get("channel"),
        is_read: row.get("is_read"),
        read_at: row.get("read_at"),
        email_sent: row.get("email_sent"),
        email_sent_at: row.get("email_sent_at"),
        created_at: row.get("created_at"),
    }
}
