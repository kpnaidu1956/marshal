//! Trial lifecycle management — background tasks and enforcement.
//!
//! - Hourly lifecycle task: expire trials, send warnings, purge stale data
//! - Trial status enforcement middleware check
//! - Rate limiting for registration endpoints

#[cfg(feature = "postgres")]
use std::sync::Arc;

#[cfg(feature = "postgres")]
use crate::postgres::PgPool;
#[cfg(feature = "postgres")]
use super::notifications::ResendClient;

// ---------------------------------------------------------------------------
// Trial Lifecycle Background Task
// ---------------------------------------------------------------------------

/// Spawn the trial lifecycle background task.
/// Runs hourly: expire trials, send warnings, clean up stale data.
///
/// NOTE: The ResendClient should be configured with a request timeout (see notifications.rs).
/// TODO: Warnings may be sent multiple times on task restart. A proper fix would add a
/// `last_warning_sent` column to avoid duplicate notifications.
#[cfg(feature = "postgres")]
pub fn spawn_lifecycle_task(pool: Arc<PgPool>, resend: Option<Arc<ResendClient>>) {
    tokio::spawn(async move {
        tracing::info!("Trial lifecycle task started (runs hourly)");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = run_lifecycle_tick(&pool, resend.as_deref()).await {
                tracing::error!("Trial lifecycle tick failed: {}", e);
            }
        }
    });
}

#[cfg(feature = "postgres")]
async fn run_lifecycle_tick(pool: &PgPool, resend: Option<&ResendClient>) -> Result<(), String> {
    let mut client = pool.get().await.map_err(|e| e.to_string())?;

    // 1. Expire past-due trials
    let expired_count = client.execute(
        "UPDATE api.organizations SET trial_status = 'expired'
         WHERE trial_status = 'active' AND trial_expires_at < now()",
        &[],
    ).await.map_err(|e| e.to_string())?;
    if expired_count > 0 {
        tracing::info!("Expired {} trial(s)", expired_count);
    }

    // 2. Send warnings for trials expiring within 7 days
    if let Some(resend) = resend {
        let warning_rows = client.query(
            "SELECT o.name, u.email, EXTRACT(DAY FROM o.trial_expires_at - now())::int as days
             FROM api.organizations o
             JOIN api.users u ON u.organization_id = o.id
             JOIN api.user_roles ur ON ur.user_id = u.id AND ur.role = 'admin' AND ur.organization_id = o.id
             WHERE o.trial_status = 'active'
               AND o.trial_expires_at BETWEEN now() AND now() + interval '7 days'
               AND EXTRACT(DAY FROM o.trial_expires_at - now())::int IN (7, 3, 1)",
            &[],
        ).await.map_err(|e| e.to_string())?;

        for row in &warning_rows {
            let org_name: String = row.get(0);
            let email: String = row.get(1);
            let days: i32 = row.get(2);
            if let Err(e) = resend.send_trial_warning(&email, &org_name, days as i64).await {
                tracing::warn!("Failed to send trial warning to {}: {}", email, e);
            }
        }

        // 3. Send expired notifications
        let expired_rows = client.query(
            "SELECT o.name, u.email
             FROM api.organizations o
             JOIN api.users u ON u.organization_id = o.id
             JOIN api.user_roles ur ON ur.user_id = u.id AND ur.role = 'admin' AND ur.organization_id = o.id
             WHERE o.trial_status = 'expired'
               AND o.trial_expires_at BETWEEN now() - interval '1 hour' AND now()",
            &[],
        ).await.map_err(|e| e.to_string())?;

        for row in &expired_rows {
            let org_name: String = row.get(0);
            let email: String = row.get(1);
            if let Err(e) = resend.send_trial_expired(&email, &org_name).await {
                tracing::warn!("Failed to send trial expired to {}: {}", email, e);
            }
        }
    }

    // 4. Expire stale join requests
    let stale_count = client.execute(
        "UPDATE trial.join_requests SET status = 'expired'
         WHERE status = 'pending' AND expires_at < now()",
        &[],
    ).await.map_err(|e| e.to_string())?;
    if stale_count > 0 {
        tracing::info!("Expired {} stale join request(s)", stale_count);
    }

    // 5. Delete unverified orgs (48hr deadline) - with cascade to avoid FK violations
    let unverified_orgs = client.query(
        "SELECT id::text, name FROM api.organizations
         WHERE domain_verified = false
           AND trial_started_at < now() - interval '48 hours'
           AND trial_status = 'active'",
        &[],
    ).await.map_err(|e| e.to_string())?;
    for row in &unverified_orgs {
        let uid: String = row.get(0);
        let uname: String = row.get(1);
        let org_uuid: uuid::Uuid = match uid.parse() {
            Ok(u) => u,
            Err(_) => continue,
        };
        // Cascade delete children first
        let _ = client.execute("DELETE FROM trial.org_quotas WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = client.execute("DELETE FROM trial.eula_acceptances WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = client.execute("DELETE FROM api.user_roles WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = client.execute("DELETE FROM api.roles WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = client.execute("DELETE FROM api.users WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = client.execute("DELETE FROM api.organizations WHERE id = $1", &[&org_uuid]).await;
        tracing::info!("Deleted unverified org: {} ({})", uname, uid);
    }

    // 6. Purge data for orgs expired or suspended > 30 days
    let purge_rows = client.query(
        "SELECT id::text, name FROM api.organizations
         WHERE trial_status IN ('expired', 'suspended')
           AND trial_expires_at < now() - interval '30 days'",
        &[],
    ).await.map_err(|e| e.to_string())?;

    for row in &purge_rows {
        let org_id: String = row.get(0);
        let org_name: String = row.get(1);
        tracing::warn!("Purging expired org: {} ({})", org_name, org_id);
        // Delete in order respecting FK constraints
        let org_uuid: uuid::Uuid = match org_id.parse() {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("Failed to parse org UUID '{}': {}", org_id, e);
                continue;
            }
        };

        let txn = match client.transaction().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to start purge transaction for {}: {}", org_name, e);
                continue;
            }
        };

        if let Err(e) = txn.execute("DELETE FROM trial.subscriptions WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at trial.subscriptions: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM trial.eula_acceptances WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at trial.eula_acceptances: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM trial.join_requests WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at trial.join_requests: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM trial.org_quotas WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at trial.org_quotas: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.user_roles WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.user_roles: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.user_groups WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.user_groups: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.role_permissions WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.role_permissions: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.roles WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.roles: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.groups WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.groups: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        // BPE schema tables (may not exist in trial DB — ignore errors)
        let _ = txn.execute("DELETE FROM bpe.notifications WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.audit_events WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.learned_sequences WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.report_templates WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.approval_decisions WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.approval_requests WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.approval_rules WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.workflow_steps WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.workflow_executions WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.workflow_definitions WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.integration_credentials WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.entity_interactions WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.entity_relationships WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.entities WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM bpe.entity_types WHERE organization_id = $1", &[&org_uuid]).await;
        // Timekeeping schema tables (may not exist in trial DB — ignore errors)
        let _ = txn.execute("DELETE FROM timekeeping.audit_trail WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.validation_flags WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.timecard_approvals WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.timecard_certifications WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.timecard_periods WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.leave_balances WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.time_entries WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.absences WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.roster_assignments WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.shift_roster WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.kelly_schedule_config WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.pay_codes WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.stations WHERE organization_id = $1", &[&org_uuid]).await;
        let _ = txn.execute("DELETE FROM timekeeping.employees WHERE organization_id = $1", &[&org_uuid]).await;

        if let Err(e) = txn.execute("DELETE FROM api.document_acls WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.document_acls: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.messages WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.messages: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.conversations WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.conversations: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.tasks WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.tasks: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.goals WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.goals: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.users WHERE organization_id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.users: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM api.organizations WHERE id = $1", &[&org_uuid]).await {
            tracing::error!("Purge failed for {} at api.organizations: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        // RAG data uses text org_id (slug)
        let org_slug = org_name.to_lowercase().replace(' ', "-");
        if let Err(e) = txn.execute("DELETE FROM rag_chunks WHERE organization_id = $1", &[&org_slug]).await {
            tracing::error!("Purge failed for {} at rag_chunks: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM rag_file_registry WHERE organization_id = $1", &[&org_slug]).await {
            tracing::error!("Purge failed for {} at rag_file_registry: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }
        if let Err(e) = txn.execute("DELETE FROM entity_embeddings WHERE organization_id = $1", &[&org_slug]).await {
            tracing::error!("Purge failed for {} at entity_embeddings: {}", org_name, e);
            let _ = txn.rollback().await;
            continue;
        }

        if let Err(e) = txn.commit().await {
            tracing::error!("Purge commit failed for {}: {}", org_name, e);
            continue;
        }
        tracing::info!("Purged org {} ({})", org_name, org_id);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Trial Status Enforcement (called from auth middleware)
// ---------------------------------------------------------------------------

/// Check if the organization's trial is still valid.
/// Returns Ok(()) if active/converted, Err with message if expired/suspended.
#[cfg(feature = "postgres")]
pub async fn check_trial_status(
    pool: &PgPool,
    organization_id: &str,
) -> Result<(), TrialBlockReason> {
    let org_uuid: uuid::Uuid = organization_id.parse()
        .map_err(|_| TrialBlockReason::InvalidOrg)?;

    let client = pool.get().await
        .map_err(|_| TrialBlockReason::DatabaseError)?;

    let row = client.query_opt(
        "SELECT trial_status, trial_expires_at FROM api.organizations WHERE id = $1",
        &[&org_uuid],
    ).await.map_err(|_| TrialBlockReason::DatabaseError)?;

    let row = row.ok_or(TrialBlockReason::InvalidOrg)?;
    let status: Option<String> = row.get(0);

    match status.as_deref() {
        Some("active") | Some("converted") | None => Ok(()),
        Some("suspended") => Err(TrialBlockReason::Suspended),
        Some("expired") => {
            // Check if within 7-day grace period (read-only)
            let expires: Option<chrono::DateTime<chrono::Utc>> = row.get(1);
            if let Some(exp) = expires {
                let grace_end = exp + chrono::Duration::days(7);
                if chrono::Utc::now() < grace_end {
                    return Err(TrialBlockReason::GracePeriod);
                }
            }
            Err(TrialBlockReason::Expired)
        }
        _ => Err(TrialBlockReason::Expired),
    }
}

/// Reasons why a trial org might be blocked.
#[derive(Debug, Clone)]
pub enum TrialBlockReason {
    /// Trial expired, past grace period — full block
    Expired,
    /// Within 7-day grace period — reads allowed, writes blocked
    GracePeriod,
    /// Admin suspended the org
    Suspended,
    /// Org not found
    InvalidOrg,
    /// Database connection error
    DatabaseError,
}

impl TrialBlockReason {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Expired => "Trial expired. Contact sales to continue.",
            Self::GracePeriod => "Trial expired. Read-only access during grace period.",
            Self::Suspended => "Account suspended. Contact support.",
            Self::InvalidOrg => "Organization not found.",
            Self::DatabaseError => "Service temporarily unavailable.",
        }
    }

    pub fn is_write_blocked(&self) -> bool {
        matches!(self, Self::Expired | Self::GracePeriod | Self::Suspended)
    }

    pub fn is_fully_blocked(&self) -> bool {
        matches!(self, Self::Expired | Self::Suspended | Self::InvalidOrg | Self::DatabaseError)
    }
}

// ---------------------------------------------------------------------------
// Rate Limiting for Registration
// ---------------------------------------------------------------------------

/// Check signup rate limits. Returns Ok(()) if allowed, Err with message if throttled.
#[cfg(feature = "postgres")]
pub async fn check_signup_rate_limit(
    pool: &PgPool,
    ip_address: &str,
    email_domain: &str,
) -> Result<(), String> {
    let client = pool.get().await.map_err(|e| e.to_string())?;

    // Max 5 attempts per IP per hour
    let ip_count = client.query_one(
        "SELECT COUNT(*) FROM trial.signup_attempts
         WHERE ip_address = $1::inet AND created_at > now() - interval '1 hour'",
        &[&ip_address],
    ).await.map_err(|e| e.to_string())?;
    let ip_count: i64 = ip_count.get(0);
    if ip_count >= 5 {
        return Err("Too many signup attempts from this IP. Please try again later.".into());
    }

    // Max 3 orgs per IP per day
    let daily_count = client.query_one(
        "SELECT COUNT(*) FROM trial.signup_attempts
         WHERE ip_address = $1::inet AND outcome = 'success' AND created_at > now() - interval '1 day'",
        &[&ip_address],
    ).await.map_err(|e| e.to_string())?;
    let daily_count: i64 = daily_count.get(0);
    if daily_count >= 3 {
        return Err("Maximum organizations per day reached. Please try again tomorrow.".into());
    }

    // Max 3 attempts per domain per day (prevent domain squatting)
    let domain_count = client.query_one(
        "SELECT COUNT(*) FROM trial.signup_attempts
         WHERE email_domain = $1 AND created_at > now() - interval '1 day'",
        &[&email_domain],
    ).await.map_err(|e| e.to_string())?;
    let domain_count: i64 = domain_count.get(0);
    if domain_count >= 3 {
        return Err("Too many registration attempts for this domain. Please try again later.".into());
    }

    Ok(())
}

/// Record a signup attempt for rate limiting tracking.
#[cfg(feature = "postgres")]
pub async fn record_signup_attempt(
    pool: &PgPool,
    ip_address: &str,
    email: &str,
    email_domain: &str,
    outcome: &str,
) {
    let client = match pool.get().await {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = client.execute(
        "INSERT INTO trial.signup_attempts (ip_address, email, email_domain, outcome)
         VALUES ($1::inet, $2, $3, $4)",
        &[&ip_address, &email, &email_domain, &outcome],
    ).await;
}

// ---------------------------------------------------------------------------
// Platform Admin Restriction for Trial Deployment
// ---------------------------------------------------------------------------

/// In the trial deployment, platform_admin is restricted to a service account.
/// Regular users cannot have this role — registration always sets is_platform_admin=false.
/// This function checks if a login attempt is trying to use platform_admin privileges
/// and blocks it unless the email matches the super-admin allowlist.
#[cfg(feature = "postgres")]
pub fn is_trial_super_admin(email: &str) -> bool {
    // Super-admin emails are configured via SUPER_ADMIN_EMAILS env var (comma-separated).
    // Returns false if the env var is not set or empty.
    let emails = std::env::var("SUPER_ADMIN_EMAILS").unwrap_or_default();
    if emails.is_empty() {
        return false;
    }
    emails.split(',').any(|e| e.trim().eq_ignore_ascii_case(email))
}
