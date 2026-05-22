//! Per-request RBAC permission service with 5-minute cache.
//! Checks feature+action permissions from api.role_permissions and api.group_permissions.
//! Also checks document-level ACLs from api.document_acls.

use crate::db::PgPool;
use crate::error::BpeError;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use uuid::Uuid;

/// Cached permission set for a user
#[derive(Debug, Clone)]
struct CachedPermissions {
    /// feature → set of allowed actions (read, write, delete, admin)
    features: HashMap<String, HashSet<String>>,
    expires_at: Instant,
}

/// In-flight load tracker to prevent thundering herd on cache miss.
/// Multiple concurrent requests for the same (user, org) will share
/// a single DB query result instead of each issuing their own.
type InflightMap = DashMap<(Uuid, Uuid), std::sync::Arc<tokio::sync::Mutex<()>>>;

/// Permission service with in-memory cache (5-minute TTL)
pub struct PermissionService {
    cache: DashMap<(Uuid, Uuid), CachedPermissions>, // (user_id, org_id) → permissions
    inflight: InflightMap,
    ttl: Duration,
    last_eviction: AtomicU64, // epoch seconds of last eviction run
}

impl PermissionService {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
            inflight: DashMap::new(),
            ttl: Duration::from_secs(300), // 5 minutes
            last_eviction: AtomicU64::new(0),
        }
    }

    /// Check if a user has permission for a feature + action.
    /// Returns Ok(true) if allowed, Ok(false) if denied.
    pub async fn check_feature(
        &self,
        pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
        feature: &str,
        action: &str,
    ) -> Result<bool, BpeError> {
        let perms = self.get_or_load(pool, user_id, org_id).await?;

        // Check if user has the specific feature + action
        if let Some(actions) = perms.features.get(feature) {
            if actions.contains(action) || actions.contains("admin") {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if a user can access a specific document.
    /// Returns true if:
    /// - No ACLs exist for the document (default open to all org members)
    /// - User has a matching ACL entry (by user, role, or group)
    ///
    /// Uses a single combined query instead of 3-4 sequential round-trips.
    pub async fn check_document(
        &self,
        pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
        document_id: Uuid,
    ) -> Result<bool, BpeError> {
        let client = pool.get().await?;

        // Single query that checks all ACL grant types at once.
        // Returns 'no_acls' if no ACLs exist (open to all), 'granted' if
        // user/role/group match found, or no rows if denied.
        let row = client.query_opt(
            "WITH acl_check AS (
                SELECT EXISTS(
                    SELECT 1 FROM api.document_acls
                    WHERE document_id = $1 AND organization_id = $2
                ) AS has_acls
            )
            SELECT CASE
                WHEN NOT (SELECT has_acls FROM acl_check) THEN 'no_acls'
                WHEN EXISTS(
                    SELECT 1 FROM api.document_acls
                    WHERE document_id = $1 AND grant_type = 'user' AND grant_id = $3
                ) THEN 'granted'
                WHEN EXISTS(
                    SELECT 1 FROM api.document_acls da
                    WHERE da.document_id = $1 AND da.grant_type = 'role'
                    AND da.grant_id IN (
                        SELECT r.id FROM api.user_roles ur
                        JOIN api.roles r ON r.name = ur.role::text AND r.organization_id = $2
                        WHERE ur.user_id = $3 AND ur.organization_id = $2
                    )
                ) THEN 'granted'
                WHEN EXISTS(
                    SELECT 1 FROM api.document_acls da
                    WHERE da.document_id = $1 AND da.grant_type = 'group'
                    AND da.grant_id IN (
                        SELECT group_id FROM api.user_groups WHERE user_id = $3 AND organization_id = $2
                    )
                ) THEN 'granted'
                ELSE 'denied'
            END AS result",
            &[&document_id, &org_id, &user_id],
        ).await.map_err(|e| BpeError::Database(format!("Document ACL check failed: {e}")))?;

        match row {
            Some(r) => {
                let result: String = r.get("result");
                Ok(result != "denied")
            }
            None => Ok(false),
        }
    }

    /// Get user's full permission set (for login response / frontend)
    pub async fn get_user_permissions(
        &self,
        pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
    ) -> Result<HashMap<String, Vec<String>>, BpeError> {
        let perms = self.get_or_load(pool, user_id, org_id).await?;
        Ok(perms.features.iter()
            .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
            .collect())
    }

    /// Invalidate cache for a specific user
    pub fn invalidate_user(&self, user_id: Uuid, org_id: Uuid) {
        self.cache.remove(&(user_id, org_id));
    }

    /// Invalidate all cached permissions
    pub fn invalidate_all(&self) {
        self.cache.clear();
    }

    /// Periodically evict expired entries to prevent memory leaks.
    /// Runs at most once per minute to avoid overhead on hot paths.
    fn maybe_evict_expired(&self) {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let last = self.last_eviction.load(Ordering::Relaxed);
        // Run eviction at most once per 60 seconds
        if now_secs.saturating_sub(last) < 60 {
            return;
        }
        // CAS to ensure only one thread runs eviction
        if self.last_eviction.compare_exchange(last, now_secs, Ordering::AcqRel, Ordering::Relaxed).is_err() {
            return;
        }

        let now = Instant::now();
        self.cache.retain(|_, v| v.expires_at > now);
        // Also clean up any stale inflight entries (should be empty normally)
        self.inflight.retain(|key, _| self.cache.contains_key(key));
    }

    /// Get permissions from cache or load from DB.
    /// Uses inflight deduplication to prevent thundering herd:
    /// if N concurrent requests miss the cache for the same user,
    /// only one DB query is issued and the others wait on it.
    async fn get_or_load(
        &self,
        pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
    ) -> Result<CachedPermissions, BpeError> {
        let key = (user_id, org_id);

        // Fast path: check cache
        if let Some(cached) = self.cache.get(&key) {
            if cached.expires_at > Instant::now() {
                return Ok(cached.clone());
            }
        }

        // Trigger periodic eviction (non-blocking, runs at most once/min)
        self.maybe_evict_expired();

        // Get or create an inflight mutex for this key to deduplicate DB loads
        let lock = self.inflight
            .entry(key)
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        let _guard = lock.lock().await;

        // Double-check cache after acquiring lock (another request may have populated it)
        if let Some(cached) = self.cache.get(&key) {
            if cached.expires_at > Instant::now() {
                return Ok(cached.clone());
            }
        }

        // Load from DB (single query for this key)
        let perms = self.load_permissions(pool, user_id, org_id).await?;
        self.cache.insert(key, perms.clone());
        // Clean up inflight entry
        self.inflight.remove(&key);
        Ok(perms)
    }

    /// Load combined permissions from roles + groups
    async fn load_permissions(
        &self,
        pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
    ) -> Result<CachedPermissions, BpeError> {
        let client = pool.get().await?;

        let rows = client.query(
            "SELECT DISTINCT rp.feature::text AS feature, rp.action::text AS action
             FROM api.user_roles ur
             JOIN api.roles r ON r.name = ur.role::text AND r.organization_id = $2
             JOIN api.role_permissions rp ON rp.role_id = r.id
             WHERE ur.user_id = $1 AND ur.organization_id = $2

             UNION

             SELECT DISTINCT gp.feature::text AS feature, gp.action::text AS action
             FROM api.user_groups ug
             JOIN api.group_permissions gp ON gp.group_id = ug.group_id
             WHERE ug.user_id = $1 AND ug.organization_id = $2",
            &[&user_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Permission query failed: {e}")))?;

        let mut features: HashMap<String, HashSet<String>> = HashMap::new();
        for row in &rows {
            let feature: String = row.get("feature");
            let action: String = row.get("action");
            features.entry(feature).or_default().insert(action);
        }

        Ok(CachedPermissions {
            features,
            expires_at: Instant::now() + self.ttl,
        })
    }
}

/// Convenience function for route handlers to check permissions
pub async fn require_feature_access(
    pool: &PgPool,
    perms: &PermissionService,
    user_id: &str,
    org_id: Uuid,
    is_admin: bool,
    feature: &str,
    action: &str,
) -> Result<(), BpeError> {
    // Platform admins bypass all permission checks
    if is_admin {
        return Ok(());
    }

    let uid = user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id".into()))?;

    let allowed = perms.check_feature(pool, uid, org_id, feature, action).await?;
    if !allowed {
        return Err(BpeError::Forbidden(format!(
            "You do not have '{}' access to '{}'", action, feature
        )));
    }

    Ok(())
}
