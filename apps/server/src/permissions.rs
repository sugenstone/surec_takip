use crate::{
    AppState, RequestId,
    auth::CurrentUser,
    context::{OrganizationContext, WorkspaceContext},
    error::{ApiError, ErrorCode},
};
use axum::{Extension, Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct EffectivePermissionsData {
    pub permissions: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct EffectivePermissionsResponse {
    pub data: EffectivePermissionsData,
}

/// Effective organization-scope permission keys for the authenticated user.
/// Frontend uses this for action visibility only; the backend remains the
/// authorization authority (ADR 0010).
#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/effective-permissions",
    params(("organization_id" = Uuid, Path, description = "Organization id")),
    responses(
        (status = 200, body = EffectivePermissionsResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn effective_permissions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: OrganizationContext,
    _current: CurrentUser,
) -> Result<Json<EffectivePermissionsResponse>, ApiError> {
    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT p.key \
         FROM membership_roles mr \
         JOIN roles r ON r.id = mr.role_id AND r.tenant_id = mr.tenant_id AND r.deleted_at IS NULL \
         JOIN role_permissions rp ON rp.role_id = r.id AND rp.scope = 'organization' \
         JOIN permissions p ON p.id = rp.permission_id \
         JOIN organization_memberships om ON om.tenant_id = mr.tenant_id AND om.user_id = mr.user_id \
              AND om.status = 'active' AND om.deleted_at IS NULL \
         WHERE mr.tenant_id = $1 AND mr.user_id = $2 AND mr.workspace_id IS NULL \
         ORDER BY p.key",
    )
    .bind(context.organization_id)
    .bind(context.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(EffectivePermissionsResponse {
        data: EffectivePermissionsData { permissions },
    }))
}

/// Effective workspace-scope permission keys for the authenticated user:
/// organization-wide grants (workspace_id IS NULL at organization scope)
/// plus grants bound to this workspace. Mirrors the authorize_workspace
/// join semantics so workspace-scoped project roles surface correctly.
/// Frontend uses this for action visibility only (ADR 0011).
#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/effective-permissions",
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Workspace id"),
    ),
    responses(
        (status = 200, body = EffectivePermissionsResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn effective_workspace_permissions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkspaceContext,
    _current: CurrentUser,
) -> Result<Json<EffectivePermissionsResponse>, ApiError> {
    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT p.key \
         FROM membership_roles mr \
         JOIN roles r ON r.id = mr.role_id AND r.tenant_id = mr.tenant_id AND r.deleted_at IS NULL \
         JOIN role_permissions rp ON rp.role_id = r.id \
              AND ((mr.workspace_id IS NULL AND rp.scope = 'organization') \
                OR (mr.workspace_id = $2 AND rp.scope = 'workspace')) \
         JOIN permissions p ON p.id = rp.permission_id \
         JOIN organization_memberships om ON om.tenant_id = mr.tenant_id AND om.user_id = mr.user_id \
              AND om.status = 'active' AND om.deleted_at IS NULL \
         JOIN workspace_memberships wsm ON wsm.workspace_id = $2 AND wsm.user_id = mr.user_id \
              AND wsm.status = 'active' AND wsm.deleted_at IS NULL \
         WHERE mr.tenant_id = $1 AND mr.user_id = $3 \
           AND (mr.workspace_id IS NULL OR mr.workspace_id = $2) \
         ORDER BY p.key",
    )
    .bind(context.organization_id)
    .bind(context.workspace_id)
    .bind(context.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(EffectivePermissionsResponse {
        data: EffectivePermissionsData { permissions },
    }))
}
