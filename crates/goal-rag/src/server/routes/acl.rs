//! Document ACL enforcement for goal-rag routes.
//!
//! Checks api.document_acls to determine if a user can access specific documents.
//! Default-open: if no ACLs exist for a document, all org members can access it.
//! Once any ACL row is added, only matching user/role/group grants have access.

use std::collections::HashSet;
use uuid::Uuid;

use crate::error::Error;
use crate::server::routes::auth::AuthClaims;
use crate::server::state::AppState;
use crate::providers::vector_store::SearchFilter;

/// Check if a user can access a specific document.
/// Returns true if no ACLs exist (default open) or user has a matching grant.
pub async fn check_document_access(
    client: &tokio_postgres::Client,
    user_id: &Uuid,
    org_id: &Uuid,
    document_id: &Uuid,
) -> Result<bool, Error> {
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
                WHERE document_id = $1 AND organization_id = $2 AND grant_type = 'user' AND grant_id = $3
            ) THEN 'granted'
            WHEN EXISTS(
                SELECT 1 FROM api.document_acls da
                WHERE da.document_id = $1 AND da.organization_id = $2 AND da.grant_type = 'role'
                AND da.grant_id IN (
                    SELECT r.id FROM api.user_roles ur
                    JOIN api.roles r ON r.name = ur.role::text
                    WHERE ur.user_id = $3
                )
            ) THEN 'granted'
            WHEN EXISTS(
                SELECT 1 FROM api.document_acls da
                WHERE da.document_id = $1 AND da.organization_id = $2 AND da.grant_type = 'group'
                AND da.grant_id IN (
                    SELECT group_id FROM api.user_groups WHERE user_id = $3 AND organization_id = $2
                )
            ) THEN 'granted'
            ELSE 'denied'
        END AS result",
        &[document_id, org_id, user_id],
    ).await.map_err(|e| Error::Internal(format!("Document ACL check failed: {e}")))?;

    match row {
        Some(r) => {
            let result: String = r.get("result");
            Ok(result != "denied")
        }
        None => Ok(false),
    }
}

/// Get all document IDs accessible to a user within their org.
/// Returns None if no ACLs are configured (all docs accessible — fast path).
/// Returns Some(vec) of accessible document UUIDs when ACLs exist.
pub async fn get_accessible_document_ids(
    client: &tokio_postgres::Client,
    user_id: &Uuid,
    org_id: &Uuid,
) -> Result<Option<Vec<Uuid>>, Error> {
    // Fast path: check if ANY document ACLs exist for this org
    let has_any = client.query_opt(
        "SELECT 1 FROM api.document_acls WHERE organization_id = $1 LIMIT 1",
        &[org_id],
    ).await.map_err(|e| Error::Internal(format!("ACL existence check failed: {e}")))?;

    if has_any.is_none() {
        return Ok(None); // No ACLs configured — all docs accessible
    }

    // Use materialized view for fast lookup
    let rows = client.query(
        "SELECT document_id FROM api.user_accessible_documents
         WHERE user_id = $1 AND organization_id = $2",
        &[user_id, org_id],
    ).await.map_err(|e| Error::Internal(format!("ACL doc query failed: {e}")))?;

    Ok(Some(rows.iter().map(|r| r.get("document_id")).collect()))
}

/// Extract user_id and org_id from AuthClaims.
/// Returns None for admin users (they bypass ACL checks).
/// Returns Err for invalid UUIDs (prevents silent bypass).
pub fn parse_acl_context(claims: &AuthClaims) -> Result<Option<(Uuid, Uuid)>, Error> {
    if claims.is_platform_admin {
        return Ok(None); // Admins bypass all ACL checks
    }
    let uid = Uuid::parse_str(&claims.user_id)
        .map_err(|_| Error::Internal("Invalid user_id in JWT".into()))?;
    let oid = Uuid::parse_str(&claims.organization_id)
        .map_err(|_| Error::Internal("Invalid organization_id in JWT".into()))?;
    Ok(Some((uid, oid)))
}

/// Enforce document ACL check for a single document.
/// Returns Ok(()) if access is allowed, Err(DocumentNotFound) if denied.
/// Uses DocumentNotFound instead of Forbidden to avoid leaking document existence.
pub async fn enforce_document_acl(
    state: &AppState,
    claims: &AuthClaims,
    document_id: &Uuid,
) -> Result<(), Error> {
    let ctx = parse_acl_context(claims)?;
    let (uid, oid) = match ctx {
        Some(pair) => pair,
        None => return Ok(()), // Admin bypass
    };

    let pool = state.pg_pool()
        .ok_or_else(|| Error::Internal("Database unavailable for access control".into()))?;
    let client = pool.get().await.map_err(|e| Error::Internal(e.to_string()))?;

    let allowed = check_document_access(&client, &uid, &oid, document_id).await?;
    if !allowed {
        return Err(Error::DocumentNotFound(format!("Document {} not found", document_id)));
    }
    Ok(())
}

/// Resolve the effective document filter by merging user-specified filter with ACL restrictions.
/// Returns the document_ids to use in SearchFilter (None = no filtering).
pub async fn resolve_acl_filter(
    state: &AppState,
    claims: &AuthClaims,
    user_doc_filter: Option<Vec<Uuid>>,
) -> Result<Option<Vec<Uuid>>, Error> {
    let ctx = parse_acl_context(claims)?;
    let (uid, oid) = match ctx {
        Some(pair) => pair,
        None => return Ok(user_doc_filter), // Admin bypass — use user filter as-is
    };

    let pool = state.pg_pool()
        .ok_or_else(|| Error::Internal("Database unavailable for access control".into()))?;
    let client = pool.get().await.map_err(|e| Error::Internal(e.to_string()))?;

    let acl_doc_ids = get_accessible_document_ids(&client, &uid, &oid).await?;

    Ok(match (user_doc_filter, acl_doc_ids) {
        (Some(uf), Some(af)) => {
            let acl_set: HashSet<Uuid> = af.into_iter().collect();
            Some(uf.into_iter().filter(|id| acl_set.contains(id)).collect())
        }
        (None, Some(af)) => Some(af),
        (Some(uf), None) => Some(uf),
        (None, None) => None,
    })
}

/// Resolve the effective document filter and build a SearchFilter.
pub async fn build_acl_search_filter(
    state: &AppState,
    claims: &AuthClaims,
    organization_id: String,
    user_doc_filter: Option<Vec<Uuid>>,
) -> Result<SearchFilter, Error> {
    let effective = resolve_acl_filter(state, claims, user_doc_filter).await?;
    Ok(SearchFilter::new(organization_id).with_documents(effective))
}
