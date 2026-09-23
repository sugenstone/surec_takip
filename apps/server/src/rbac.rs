//! RBAC authorization foundation (First Agent Mission step 13).
//!
//! Boundary (ADR 0008): membership/context resolves ELIGIBILITY; this module
//! resolves PERMISSIONS on top. Authorization always evaluates permission
//! keys — never role names or labels — and always joins ACTIVE memberships,
//! so stale assignments are inert even if a row survives.

use crate::error::ErrorCode;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{ApiError, RequestId};

/// Stable machine-readable permission key. New keys are added only when a
/// real enforcement point exists for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PermissionKey(pub &'static str);

/// Create workspaces inside an organization (organization-scope authority).
pub const WORKSPACES_CREATE: PermissionKey = PermissionKey("workspaces:create");

/// Seed built-in roles for a new organization and grant the Owner role its
/// current management permissions. Runs INSIDE the organization-creation
/// transaction: organization, membership, roles and the Owner assignment
/// commit atomically — no administrable window and no orphan tenant.
pub async fn bootstrap_builtin_roles(
    transaction: &mut PgConnection,
    tenant_id: Uuid,
    owner: Uuid,
) -> Result<(), ApiError> {
    let owner_role = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO roles (id, tenant_id, name, description, is_system) \
         VALUES ($1, $2, 'owner', 'Built-in owner role (all current tenant management permissions)', true) \
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    sqlx::query(
        "INSERT INTO roles (id, tenant_id, name, description, is_system) \
         VALUES ($1, $2, 'member', 'Built-in member role (no additional permissions yet)', true)",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    // Grants reference the permission catalog by stable key. Extending the
    // grant list is a deliberate bootstrap decision, never automatic flow
    // (ADR 0008: Owner does not implicitly gain future permissions). The
    // project/section keys mirror the 007/008 migration backfills.
    for key in [
        WORKSPACES_CREATE.0,
        crate::invitations::MEMBERS_INVITE.0,
        crate::projects::PROJECTS_CREATE.0,
        crate::projects::PROJECTS_UPDATE.0,
        crate::projects::PROJECTS_ARCHIVE.0,
        crate::sections::SECTIONS_CREATE.0,
        crate::sections::SECTIONS_UPDATE.0,
        crate::sections::SECTIONS_ARCHIVE.0,
    ] {
        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_id, scope) \
             SELECT $1, p.id, 'organization' FROM permissions p WHERE p.key = $2",
        )
        .bind(owner_role)
        .bind(key)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    }
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, NULL)",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(owner)
    .bind(owner_role)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    Ok(())
}

/// Effective organization-scope permission: at least one non-deleted role of
/// this tenant grants the permission at organization scope through an
/// organization-wide assignment (workspace_id IS NULL), AND the user's
/// organization membership is active. Defense in depth: even a stale
/// assignment row stays inert without an active membership.
pub async fn authorize_organization(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    permission: PermissionKey,
) -> Result<bool, ApiError> {
    let granted = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS( \
            SELECT 1 FROM membership_roles mr \
            JOIN roles r ON r.id = mr.role_id AND r.tenant_id = mr.tenant_id AND r.deleted_at IS NULL \
            JOIN role_permissions rp ON rp.role_id = r.id AND rp.scope = 'organization' \
            JOIN permissions p ON p.id = rp.permission_id AND p.key = $3 \
            JOIN organization_memberships om ON om.tenant_id = mr.tenant_id AND om.user_id = mr.user_id \
                 AND om.status = 'active' AND om.deleted_at IS NULL \
            WHERE mr.tenant_id = $1 AND mr.user_id = $2 AND mr.workspace_id IS NULL)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(permission.0)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    Ok(granted)
}

/// Effective workspace-scope permission. Organization-wide assignments
/// (workspace_id IS NULL, granted at 'organization' scope) apply across the
/// tenant including its workspaces; workspace-scoped assignments apply only
/// to their workspace. Eligibility (both memberships) is proven by the
/// context extractor; the active-organization-membership join is kept here
/// as defense in depth.
pub async fn authorize_workspace(
    pool: &PgPool,
    tenant_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    permission: PermissionKey,
) -> Result<bool, ApiError> {
    let granted = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS( \
            SELECT 1 FROM membership_roles mr \
            JOIN roles r ON r.id = mr.role_id AND r.tenant_id = mr.tenant_id AND r.deleted_at IS NULL \
            JOIN role_permissions rp ON rp.role_id = r.id \
                 AND ((mr.workspace_id IS NULL AND rp.scope = 'organization') \
                   OR (mr.workspace_id = $3 AND rp.scope = 'workspace')) \
            JOIN permissions p ON p.id = rp.permission_id AND p.key = $4 \
            JOIN organization_memberships om ON om.tenant_id = mr.tenant_id AND om.user_id = mr.user_id \
                 AND om.status = 'active' AND om.deleted_at IS NULL \
            JOIN workspace_memberships wsm ON wsm.workspace_id = $3 AND wsm.user_id = mr.user_id \
                 AND wsm.status = 'active' AND wsm.deleted_at IS NULL \
            WHERE mr.tenant_id = $1 AND mr.user_id = $2 \
              AND (mr.workspace_id IS NULL OR mr.workspace_id = $3))",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(workspace_id)
    .bind(permission.0)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    Ok(granted)
}

/// Transactional workspace-scope re-validation for security-critical
/// mutations: the same evaluation as `authorize_workspace`, executed inside
/// the mutation transaction so a revocation racing the write cannot slip
/// through (TOCTOU, ADR 0008/0011).
pub async fn authorize_workspace_in_tx(
    transaction: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    permission: PermissionKey,
) -> Result<bool, ApiError> {
    let granted = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS( \
            SELECT 1 FROM membership_roles mr \
            JOIN roles r ON r.id = mr.role_id AND r.tenant_id = mr.tenant_id AND r.deleted_at IS NULL \
            JOIN role_permissions rp ON rp.role_id = r.id \
                 AND ((mr.workspace_id IS NULL AND rp.scope = 'organization') \
                   OR (mr.workspace_id = $3 AND rp.scope = 'workspace')) \
            JOIN permissions p ON p.id = rp.permission_id AND p.key = $4 \
            JOIN organization_memberships om ON om.tenant_id = mr.tenant_id AND om.user_id = mr.user_id \
                 AND om.status = 'active' AND om.deleted_at IS NULL \
            JOIN workspace_memberships wsm ON wsm.workspace_id = $3 AND wsm.user_id = mr.user_id \
                 AND wsm.status = 'active' AND wsm.deleted_at IS NULL \
            WHERE mr.tenant_id = $1 AND mr.user_id = $2 \
              AND (mr.workspace_id IS NULL OR mr.workspace_id = $3))",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(workspace_id)
    .bind(permission.0)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    Ok(granted)
}

/// Transactional re-validation for security-critical mutations: the same
/// permission evaluation, executed inside the mutation transaction so a
/// revocation racing the write cannot slip through (TOCTOU, ADR 0008).
pub async fn authorize_organization_in_tx(
    transaction: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    permission: PermissionKey,
) -> Result<bool, ApiError> {
    let granted = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS( \
            SELECT 1 FROM membership_roles mr \
            JOIN roles r ON r.id = mr.role_id AND r.tenant_id = mr.tenant_id AND r.deleted_at IS NULL \
            JOIN role_permissions rp ON rp.role_id = r.id AND rp.scope = 'organization' \
            JOIN permissions p ON p.id = rp.permission_id AND p.key = $3 \
            JOIN organization_memberships om ON om.tenant_id = mr.tenant_id AND om.user_id = mr.user_id \
                 AND om.status = 'active' AND om.deleted_at IS NULL \
            WHERE mr.tenant_id = $1 AND mr.user_id = $2 AND mr.workspace_id IS NULL)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(permission.0)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    Ok(granted)
}

/// Deactivate an organization membership AND physically delete all of the
/// user's role assignments in that tenant, in one transaction. Physical
/// deletion is the privilege-resurrection prevention (ADR 0008): a later
/// reactivation starts with NO old privileges; only explicit new
/// assignments restore authority.
pub async fn deactivate_membership_with_assignments(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), ApiError> {
    let internal = || ApiError::new(ErrorCode::InternalError, String::new());
    let mut transaction = pool.begin().await.map_err(|_| internal())?;
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'active'",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| internal())?;
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(tenant_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| internal())?;
    transaction.commit().await.map_err(|_| internal())?;
    Ok(())
}

/// Deactivate a workspace membership AND physically delete that user's
/// workspace-scoped role assignments for it, in one transaction. Workspace
/// variant of the resurrection-prevention lifecycle (ADR 0007/0008).
pub async fn deactivate_workspace_membership_with_assignments(
    pool: &PgPool,
    tenant_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), ApiError> {
    let internal = || ApiError::new(ErrorCode::InternalError, String::new());
    let mut transaction = pool.begin().await.map_err(|_| internal())?;
    sqlx::query(
        "UPDATE workspace_memberships SET status = 'deleted', deleted_at = now() \
         WHERE workspace_id = $1 AND user_id = $2 AND status = 'active'",
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| internal())?;
    sqlx::query(
        "DELETE FROM membership_roles \
         WHERE tenant_id = $1 AND user_id = $2 AND workspace_id = $3",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(workspace_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| internal())?;
    transaction.commit().await.map_err(|_| internal())?;
    Ok(())
}

/// Reactivate a soft-deleted workspace membership WITHOUT restoring any
/// workspace-scoped role assignments.
pub async fn reactivate_workspace_membership(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE workspace_memberships SET status = 'active', deleted_at = NULL \
         WHERE workspace_id = $1 AND user_id = $2 AND status = 'deleted'",
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    Ok(())
}

/// Explicit, trusted Owner assignment for pre-RBAC organizations (used by
/// the user-admin CLI; no HTTP surface). Requires an ACTIVE organization
/// membership; idempotent when already assigned. Errors are stable CLI
/// codes, never internal details.
pub async fn assign_owner(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), &'static str> {
    let owner_role = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM roles \
         WHERE tenant_id = $1 AND name = 'owner' AND is_system AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "OWNER_LOOKUP_FAILED")?;
    let Some(owner_role) = owner_role else {
        return Err("OWNER_ROLE_MISSING");
    };
    let member: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| "MEMBERSHIP_LOOKUP_FAILED")?;
    if member == 0 {
        return Err("USER_NOT_AN_ACTIVE_MEMBER");
    }
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, NULL) ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(user_id)
    .bind(owner_role)
    .execute(pool)
    .await
    .map_err(|_| "OWNER_ASSIGN_FAILED")?;
    Ok(())
}

/// Reactivate a soft-deleted organization membership WITHOUT restoring any
/// role assignments — they were physically deleted at deactivation.
pub async fn reactivate_membership(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE organization_memberships SET status = 'active', deleted_at = NULL \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'deleted'",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, String::new()))?;
    Ok(())
}

/// Standard denial for an eligible-but-unprivileged caller. 403, stable
/// PERMISSION_DENIED code, no role/permission internals leaked.
pub fn permission_denied(request_id: &RequestId) -> ApiError {
    ApiError::new(ErrorCode::PermissionDenied, request_id.0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_keys_are_stable_machine_identifiers() {
        assert_eq!(WORKSPACES_CREATE.0, "workspaces:create");
    }
}
