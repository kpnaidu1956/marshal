//! Document ACL and Group management route handlers

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bpe_core::{
    acl::{AclManager, GroupManager, CreateDocumentAclRequest, CreateGroupRequest, UpdateGroupRequest, AddMemberRequest, AddPermissionRequest, refresh_acl_view},
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    permissions::require_feature_access,
};
use serde::Deserialize;
use uuid::Uuid;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    pub organization_id: String,
}

// ---------------------------------------------------------------------------
// Document ACL endpoints
// ---------------------------------------------------------------------------

/// GET /bpe/api/documents/:id/acls
pub async fn list_document_acls(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(document_id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "knowledge", "read").await?;

    let acls = AclManager::list_document_acls(state.pool(), document_id, org_id).await?;
    Ok(Json(serde_json::json!({ "data": acls })))
}

/// POST /bpe/api/documents/:id/acls
pub async fn create_document_acl(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(document_id): Path<Uuid>,
    Json(req): Json<CreateDocumentAclRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "knowledge", "admin").await?;

    let user_id = claims.user_id.parse::<Uuid>().map_err(|_| BpeError::Internal("Invalid user_id".into()))?;
    let grant_id = req.grant_id.parse::<Uuid>().map_err(|_| BpeError::BadRequest("Invalid grant_id".into()))?;

    let acl = AclManager::create_document_acl(state.pool(), document_id, org_id, &req.grant_type, grant_id, &req.action, user_id).await?;

    // Refresh materialized view (doc ACL changes don't affect feature permissions, so no invalidate_all)
    if let Err(e) = refresh_acl_view(state.pool()).await {
        tracing::warn!("Failed to refresh ACL view: {e}");
    }

    Ok(Json(serde_json::json!({ "data": acl })))
}

/// DELETE /bpe/api/documents/:id/acls/:acl_id
pub async fn delete_document_acl(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path((document_id, acl_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "knowledge", "admin").await?;

    AclManager::delete_document_acl(state.pool(), acl_id, document_id, org_id).await?;

    if let Err(e) = refresh_acl_view(state.pool()).await {
        tracing::warn!("Failed to refresh ACL view: {e}");
    }

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// DELETE /bpe/api/documents/:id/acls - Clear ALL ACLs (make doc open)
pub async fn clear_document_acls(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(document_id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "knowledge", "admin").await?;

    let count = AclManager::clear_document_acls(state.pool(), document_id, org_id).await?;

    if let Err(e) = refresh_acl_view(state.pool()).await {
        tracing::warn!("Failed to refresh ACL view: {e}");
    }

    Ok(Json(serde_json::json!({ "status": "cleared", "removed": count })))
}

// ---------------------------------------------------------------------------
// Group endpoints
// ---------------------------------------------------------------------------

/// GET /bpe/api/groups
pub async fn list_groups(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "read").await?;

    let groups = GroupManager::list(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": groups })))
}

/// POST /bpe/api/groups
pub async fn create_group(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;

    let group = GroupManager::create(state.pool(), org_id, &req.name, req.description.as_deref()).await?;
    Ok(Json(serde_json::json!({ "data": group })))
}

/// PUT /bpe/api/groups/:id
pub async fn update_group(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;

    GroupManager::update(state.pool(), id, org_id, req.name.as_deref(), req.description.as_deref()).await?;
    Ok(Json(serde_json::json!({ "status": "updated" })))
}

/// DELETE /bpe/api/groups/:id
pub async fn delete_group(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;

    GroupManager::delete(state.pool(), id, org_id).await?;
    state.permissions().invalidate_all();

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// GET /bpe/api/groups/:id/members
pub async fn list_group_members(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "read").await?;

    let members = GroupManager::list_members(state.pool(), id, org_id).await?;
    Ok(Json(serde_json::json!({ "data": members })))
}

/// POST /bpe/api/groups/:id/members
pub async fn add_group_member(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;

    let user_id = req.user_id.parse::<Uuid>().map_err(|_| BpeError::BadRequest("Invalid user_id".into()))?;
    GroupManager::add_member(state.pool(), id, user_id, org_id).await?;
    state.permissions().invalidate_user(user_id, org_id);

    Ok(Json(serde_json::json!({ "status": "added" })))
}

/// DELETE /bpe/api/groups/:id/members/:user_id
pub async fn remove_group_member(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;

    GroupManager::remove_member(state.pool(), id, user_id, org_id).await?;
    state.permissions().invalidate_user(user_id, org_id);

    Ok(Json(serde_json::json!({ "status": "removed" })))
}

/// GET /bpe/api/groups/:id/permissions
pub async fn list_group_permissions(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "read").await?;

    let perms = GroupManager::list_permissions(state.pool(), id, org_id).await?;
    Ok(Json(serde_json::json!({ "data": perms })))
}

/// POST /bpe/api/groups/:id/permissions
pub async fn add_group_permission(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddPermissionRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;

    let perm = GroupManager::add_permission(state.pool(), id, org_id, &req.feature, &req.action).await?;
    state.permissions().invalidate_all();

    Ok(Json(serde_json::json!({ "data": perm })))
}

/// DELETE /bpe/api/groups/:id/permissions/:perm_id
pub async fn remove_group_permission(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path((_id, perm_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;

    GroupManager::remove_permission(state.pool(), perm_id, org_id).await?;
    state.permissions().invalidate_all();

    Ok(Json(serde_json::json!({ "status": "removed" })))
}

// ---------------------------------------------------------------------------
// Permission introspection
// ---------------------------------------------------------------------------

/// GET /bpe/api/permissions/me
pub async fn my_permissions(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    let uid = claims.user_id.parse::<Uuid>().map_err(|_| BpeError::Internal("Invalid user_id".into()))?;

    let features = state.permissions().get_user_permissions(state.pool(), uid, org_id).await.unwrap_or_default();

    Ok(Json(serde_json::json!({
        "user_id": claims.user_id,
        "organization_id": query.organization_id,
        "is_admin": claims.is_platform_admin,
        "features": features,
    })))
}

/// POST /bpe/api/admin/cache/invalidate
pub async fn invalidate_cache(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
) -> Result<Json<serde_json::Value>, BpeError> {
    if !claims.is_platform_admin {
        return Err(BpeError::Forbidden("Admin access required".into()));
    }
    state.permissions().invalidate_all();
    Ok(Json(serde_json::json!({ "status": "cache_invalidated" })))
}
