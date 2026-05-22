//! Document ACL and Group management for the BPE platform.
//!
//! Provides CRUD operations for:
//! - Document ACLs (grant user/role/group access to specific documents)
//! - Groups (create groups, manage members and permissions)

use crate::db::PgPool;
use crate::error::BpeError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Allowlists for enum validation (prevents SQL injection via format!())
const VALID_GRANT_TYPES: &[&str] = &["user", "role", "group"];
const VALID_ACL_ACTIONS: &[&str] = &["read", "write", "admin"];
const VALID_FEATURES: &[&str] = &["timekeeping", "roster", "reports", "audit", "approvals", "admin", "knowledge", "marshal", "analytics"];
const VALID_PERM_ACTIONS: &[&str] = &["read", "write", "delete", "admin"];

// ---------------------------------------------------------------------------
// Document ACL models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAcl {
    pub id: Uuid,
    pub document_id: Uuid,
    pub organization_id: Uuid,
    pub grant_type: String,
    pub grant_id: Uuid,
    pub grant_name: Option<String>,
    pub action: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDocumentAclRequest {
    pub organization_id: String,
    pub grant_type: String,
    pub grant_id: String,
    #[serde(default = "default_read")]
    pub action: String,
}

fn default_read() -> String { "read".into() }

// ---------------------------------------------------------------------------
// Group models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Uuid,
    pub member_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub organization_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupMember {
    pub user_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub organization_id: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupPermission {
    pub id: Uuid,
    pub group_id: Uuid,
    pub feature: String,
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct AddPermissionRequest {
    pub organization_id: String,
    pub feature: String,
    pub action: String,
}

// ---------------------------------------------------------------------------
// Document ACL operations
// ---------------------------------------------------------------------------

pub struct AclManager;

impl AclManager {
    /// List ACLs for a document (uses LEFT JOINs for name resolution)
    pub async fn list_document_acls(
        pool: &PgPool,
        document_id: Uuid,
        org_id: Uuid,
    ) -> Result<Vec<DocumentAcl>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT da.id, da.document_id, da.organization_id, da.grant_type,
                    da.grant_id, da.action::text AS action, da.created_by, da.created_at,
                    COALESCE(u.first_name || ' ' || u.last_name, g.name, r.name) AS grant_name
             FROM api.document_acls da
             LEFT JOIN api.users u ON da.grant_type = 'user' AND da.grant_id = u.id
             LEFT JOIN api.groups g ON da.grant_type = 'group' AND da.grant_id = g.id
             LEFT JOIN api.roles r ON da.grant_type = 'role' AND da.grant_id = r.id
             WHERE da.document_id = $1 AND da.organization_id = $2
             ORDER BY da.grant_type, da.created_at",
            &[&document_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("List doc ACLs: {e}")))?;

        Ok(rows.iter().map(|r| DocumentAcl {
            id: r.get("id"),
            document_id: r.get("document_id"),
            organization_id: r.get("organization_id"),
            grant_type: r.get("grant_type"),
            grant_id: r.get("grant_id"),
            grant_name: r.try_get("grant_name").ok().flatten(),
            action: r.get("action"),
            created_by: r.try_get("created_by").ok().flatten(),
            created_at: r.get("created_at"),
        }).collect())
    }

    /// Add an ACL grant to a document
    pub async fn create_document_acl(
        pool: &PgPool,
        document_id: Uuid,
        org_id: Uuid,
        grant_type: &str,
        grant_id: Uuid,
        action: &str,
        created_by: Uuid,
    ) -> Result<DocumentAcl, BpeError> {
        if !VALID_GRANT_TYPES.contains(&grant_type) {
            return Err(BpeError::BadRequest("grant_type must be 'user', 'role', or 'group'".into()));
        }
        if !VALID_ACL_ACTIONS.contains(&action) {
            return Err(BpeError::BadRequest("action must be 'read', 'write', or 'admin'".into()));
        }

        let client = pool.get().await?;
        // action is validated above — safe to embed in SQL (avoids tokio-postgres enum serialization issue)
        let sql = format!(
            "INSERT INTO api.document_acls (document_id, organization_id, grant_type, grant_id, action, created_by)
             VALUES ($1, $2, $3, $4, '{}'::api.permission_action, $5)
             ON CONFLICT (document_id, grant_type, grant_id, action) DO NOTHING
             RETURNING id, document_id, organization_id, grant_type, grant_id, action::text AS action, created_by, created_at",
            action
        );
        let row = client.query_opt(
            &sql,
            &[&document_id, &org_id, &grant_type, &grant_id, &created_by],
        ).await.map_err(|e| BpeError::Database(format!("Create doc ACL: {e}")))?;
        let row = row.ok_or_else(|| BpeError::Conflict("ACL entry already exists".into()))?;

        Ok(DocumentAcl {
            id: row.get("id"),
            document_id: row.get("document_id"),
            organization_id: row.get("organization_id"),
            grant_type: row.get("grant_type"),
            grant_id: row.get("grant_id"),
            grant_name: None,
            action: row.get("action"),
            created_by: row.try_get("created_by").ok().flatten(),
            created_at: row.get("created_at"),
        })
    }

    /// Remove an ACL grant (scoped by org + document)
    pub async fn delete_document_acl(
        pool: &PgPool,
        acl_id: Uuid,
        document_id: Uuid,
        org_id: Uuid,
    ) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let count = client.execute(
            "DELETE FROM api.document_acls WHERE id = $1 AND document_id = $2 AND organization_id = $3",
            &[&acl_id, &document_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Delete doc ACL: {e}")))?;

        if count == 0 {
            return Err(BpeError::NotFound("ACL entry not found".into()));
        }
        Ok(())
    }

    /// Remove ALL ACLs for a document (make it open again)
    pub async fn clear_document_acls(
        pool: &PgPool,
        document_id: Uuid,
        org_id: Uuid,
    ) -> Result<u64, BpeError> {
        let client = pool.get().await?;
        let count = client.execute(
            "DELETE FROM api.document_acls WHERE document_id = $1 AND organization_id = $2",
            &[&document_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Clear doc ACLs: {e}")))?;
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Group operations (all scoped by org_id)
// ---------------------------------------------------------------------------

pub struct GroupManager;

impl GroupManager {
    /// List groups for an organization
    pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<Group>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT g.id, g.name, g.description, g.organization_id, g.created_at,
                    (SELECT COUNT(*) FROM api.user_groups ug WHERE ug.group_id = g.id) AS member_count
             FROM api.groups g
             WHERE g.organization_id = $1
             ORDER BY g.name
             LIMIT 500",
            &[&org_id],
        ).await.map_err(|e| BpeError::Database(format!("List groups: {e}")))?;

        Ok(rows.iter().map(|r| Group {
            id: r.get("id"),
            name: r.get("name"),
            description: r.try_get("description").ok().flatten(),
            organization_id: r.get("organization_id"),
            member_count: r.get("member_count"),
            created_at: r.get("created_at"),
        }).collect())
    }

    /// Create a group
    pub async fn create(pool: &PgPool, org_id: Uuid, name: &str, description: Option<&str>) -> Result<Group, BpeError> {
        let client = pool.get().await?;
        let row = client.query_one(
            "INSERT INTO api.groups (name, description, organization_id)
             VALUES ($1, $2, $3)
             RETURNING id, name, description, organization_id, created_at",
            &[&name, &description, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Create group: {e}")))?;

        Ok(Group {
            id: row.get("id"),
            name: row.get("name"),
            description: row.try_get("description").ok().flatten(),
            organization_id: row.get("organization_id"),
            member_count: 0,
            created_at: row.get("created_at"),
        })
    }

    /// Update a group (org-scoped)
    pub async fn update(pool: &PgPool, group_id: Uuid, org_id: Uuid, name: Option<&str>, description: Option<&str>) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "UPDATE api.groups SET name = COALESCE($1, name), description = COALESCE($2, description)
             WHERE id = $3 AND organization_id = $4",
            &[&name, &description, &group_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Update group: {e}")))?;
        Ok(())
    }

    /// Delete a group (org-scoped, cascading cleanup)
    pub async fn delete(pool: &PgPool, group_id: Uuid, org_id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        // Verify group belongs to org first
        let exists = client.query_opt(
            "SELECT 1 FROM api.groups WHERE id = $1 AND organization_id = $2",
            &[&group_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Check group: {e}")))?;
        if exists.is_none() {
            return Err(BpeError::NotFound("Group not found".into()));
        }
        // Remove dependents then group
        client.execute("DELETE FROM api.user_groups WHERE group_id = $1 AND organization_id = $2", &[&group_id, &org_id])
            .await.map_err(|e| BpeError::Database(format!("Delete group members: {e}")))?;
        client.execute("DELETE FROM api.group_permissions WHERE group_id = $1 AND organization_id = $2", &[&group_id, &org_id])
            .await.map_err(|e| BpeError::Database(format!("Delete group permissions: {e}")))?;
        client.execute("DELETE FROM api.document_acls WHERE grant_type = 'group' AND grant_id = $1 AND organization_id = $2", &[&group_id, &org_id])
            .await.map_err(|e| BpeError::Database(format!("Delete group doc ACLs: {e}")))?;
        client.execute("DELETE FROM api.groups WHERE id = $1 AND organization_id = $2", &[&group_id, &org_id])
            .await.map_err(|e| BpeError::Database(format!("Delete group: {e}")))?;
        Ok(())
    }

    /// List members of a group (org-scoped)
    pub async fn list_members(pool: &PgPool, group_id: Uuid, org_id: Uuid) -> Result<Vec<GroupMember>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT u.id AS user_id, u.first_name, u.last_name, u.email, u.title
             FROM api.user_groups ug
             JOIN api.users u ON u.id = ug.user_id
             WHERE ug.group_id = $1 AND ug.organization_id = $2
             ORDER BY u.first_name, u.last_name",
            &[&group_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("List group members: {e}")))?;

        Ok(rows.iter().map(|r| GroupMember {
            user_id: r.get("user_id"),
            first_name: r.get("first_name"),
            last_name: r.get("last_name"),
            email: r.try_get("email").ok().flatten(),
            title: r.try_get("title").ok().flatten(),
        }).collect())
    }

    /// Add a user to a group
    pub async fn add_member(pool: &PgPool, group_id: Uuid, user_id: Uuid, org_id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "INSERT INTO api.user_groups (user_id, group_id, organization_id)
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            &[&user_id, &group_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Add group member: {e}")))?;
        Ok(())
    }

    /// Remove a user from a group (org-scoped)
    pub async fn remove_member(pool: &PgPool, group_id: Uuid, user_id: Uuid, org_id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "DELETE FROM api.user_groups WHERE group_id = $1 AND user_id = $2 AND organization_id = $3",
            &[&group_id, &user_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Remove group member: {e}")))?;
        Ok(())
    }

    /// List permissions for a group (org-scoped)
    pub async fn list_permissions(pool: &PgPool, group_id: Uuid, org_id: Uuid) -> Result<Vec<GroupPermission>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT id, group_id, feature::text AS feature, action::text AS action
             FROM api.group_permissions WHERE group_id = $1 AND organization_id = $2
             ORDER BY feature, action",
            &[&group_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("List group perms: {e}")))?;

        Ok(rows.iter().map(|r| GroupPermission {
            id: r.get("id"),
            group_id: r.get("group_id"),
            feature: r.get("feature"),
            action: r.get("action"),
        }).collect())
    }

    /// Add a permission to a group (with validation)
    pub async fn add_permission(pool: &PgPool, group_id: Uuid, org_id: Uuid, feature: &str, action: &str) -> Result<GroupPermission, BpeError> {
        // Validate against allowlists to prevent SQL injection via format!()
        if !VALID_FEATURES.contains(&feature) {
            return Err(BpeError::BadRequest(format!("Invalid feature: '{feature}'. Must be one of: {}", VALID_FEATURES.join(", "))));
        }
        if !VALID_PERM_ACTIONS.contains(&action) {
            return Err(BpeError::BadRequest(format!("Invalid action: '{action}'. Must be one of: {}", VALID_PERM_ACTIONS.join(", "))));
        }

        let client = pool.get().await?;
        let sql = format!(
            "INSERT INTO api.group_permissions (group_id, organization_id, feature, action)
             VALUES ($1, $2, '{}'::api.app_feature, '{}'::api.permission_action)
             ON CONFLICT DO NOTHING
             RETURNING id, group_id, feature::text AS feature, action::text AS action",
            feature, action
        );
        let row = client.query_opt(&sql, &[&group_id, &org_id])
            .await.map_err(|e| BpeError::Database(format!("Add group perm: {e}")))?;
        let row = row.ok_or_else(|| BpeError::Conflict("Permission already exists".into()))?;

        Ok(GroupPermission {
            id: row.get("id"),
            group_id: row.get("group_id"),
            feature: row.get("feature"),
            action: row.get("action"),
        })
    }

    /// Remove a permission from a group (org-scoped)
    pub async fn remove_permission(pool: &PgPool, perm_id: Uuid, org_id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "DELETE FROM api.group_permissions WHERE id = $1 AND organization_id = $2",
            &[&perm_id, &org_id],
        ).await.map_err(|e| BpeError::Database(format!("Remove group perm: {e}")))?;
        Ok(())
    }
}

/// Refresh the materialized view after ACL changes
pub async fn refresh_acl_view(pool: &PgPool) -> Result<(), BpeError> {
    let client = pool.get().await?;
    client.execute("REFRESH MATERIALIZED VIEW CONCURRENTLY api.user_accessible_documents", &[])
        .await
        .map_err(|e| BpeError::Database(format!("Refresh ACL view: {e}")))?;
    Ok(())
}
