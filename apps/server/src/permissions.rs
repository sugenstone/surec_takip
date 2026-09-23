use crate::{
    AppState, RequestId,
    auth::CurrentUser,
    context::OrganizationContext,
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
