//! Section-owned, non-recursive work item foundation (ADR 0013).
use crate::{
    AppState, RequestId,
    auth::ApiJson,
    context::{WorkItemContext, WorkItemSectionContext},
    error::{ApiError, ErrorCode},
    organizations::{slug_from_text, validate_slug},
    rbac::{self, PermissionKey},
};
use axum::{Extension, Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

pub const WORK_ITEMS_CREATE: PermissionKey = PermissionKey("work_items:create");
pub const WORK_ITEMS_UPDATE: PermissionKey = PermissionKey("work_items:update");
pub const WORK_ITEMS_ARCHIVE: PermissionKey = PermissionKey("work_items:archive");

/// Server-resolved route identity, never deserialized from request bodies.
#[derive(Clone, Copy)]
pub struct WorkItemScope {
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub section_id: Uuid,
}

#[derive(Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct WorkItemPublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub section_id: Uuid,
    pub name: String,
    pub slug: String,
    pub position: i32,
    pub status: String,
}

const COLUMNS: &str = "id, tenant_id AS organization_id, workspace_id, project_id, section_id, name, slug::text AS slug, position, status";
const SCOPE: &str = "tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4";
const VISIBLE: &str = "deleted_at IS NULL AND status <> 'archived'";

#[derive(Debug)]
pub enum WorkItemError {
    NotAccessible,
    Forbidden,
    InvalidFields(serde_json::Value),
    DatabaseError,
}
impl std::fmt::Display for WorkItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotAccessible => "resource not found",
            Self::Forbidden => "permission denied",
            Self::InvalidFields(_) => "invalid fields",
            Self::DatabaseError => "database operation failed",
        })
    }
}
impl std::error::Error for WorkItemError {}

fn invalid(field: &str) -> WorkItemError {
    WorkItemError::InvalidFields(serde_json::json!({field: ["Invalid or unavailable"]}))
}
fn database_error(error: sqlx::Error) -> WorkItemError {
    if let sqlx::Error::Database(ref db) = error
        && db.constraint() == Some("work_items_section_slug_key")
    {
        return invalid("slug");
    }
    WorkItemError::DatabaseError
}
fn api_error(error: WorkItemError, request_id: &RequestId) -> ApiError {
    match error {
        WorkItemError::NotAccessible => {
            ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone())
        }
        WorkItemError::Forbidden => rbac::permission_denied(request_id),
        WorkItemError::InvalidFields(fields) => {
            ApiError::invalid_fields(fields, request_id.0.clone())
        }
        WorkItemError::DatabaseError => {
            ApiError::new(ErrorCode::InternalError, request_id.0.clone())
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkItemRequest {
    pub name: String,
    pub slug: Option<String>,
}
#[derive(Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateWorkItemRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub position: Option<i32>,
    pub status: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct WorkItemMutationResponse {
    pub data: WorkItemPublic,
}
#[derive(Serialize, ToSchema)]
pub struct WorkItemListResponse {
    pub data: Vec<WorkItemPublic>,
}

fn name_value(value: &str) -> Result<String, WorkItemError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(invalid("name"));
    }
    Ok(value.to_owned())
}
fn slug_value(value: &str) -> Result<String, WorkItemError> {
    slug_from_text(value.trim())
        .filter(|s| validate_slug(s))
        .ok_or_else(|| invalid("slug"))
}

pub async fn find_accessible(
    pool: &PgPool,
    scope: WorkItemScope,
    id: Uuid,
) -> Result<Option<WorkItemPublic>, WorkItemError> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM work_items WHERE {SCOPE} AND id = $5 AND {VISIBLE}"
    ))
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(database_error)
}
pub async fn visible_for_section(
    pool: &PgPool,
    scope: WorkItemScope,
) -> Result<Vec<WorkItemPublic>, WorkItemError> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM work_items WHERE {SCOPE} AND {VISIBLE} ORDER BY position, id"
    ))
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .fetch_all(pool)
    .await
    .map_err(database_error)
}

/// Match section serialization order: project first, then the direct section.
/// Locks hold parent lifecycle and both memberships stable until commit.
/// A revoke committed before these checks is rejected; one racing after them
/// waits for this transaction (no stale authorization window before the write).
/// Processes (ADR 0015) reuse this exact prefix of the lock order.
pub(crate) async fn lock_parent(
    tx: &mut PgConnection,
    scope: WorkItemScope,
    actor: Uuid,
) -> Result<(), WorkItemError> {
    let project = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM projects WHERE tenant_id = $1 AND workspace_id = $2 AND id = $3 \
         AND deleted_at IS NULL AND status <> 'archived' FOR UPDATE",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?;
    if project.is_none() {
        return Err(WorkItemError::NotAccessible);
    }
    let section = sqlx::query_scalar::<_, Uuid>(
        "SELECT s.id FROM sections s \
         JOIN workspaces w ON w.id = s.workspace_id AND w.tenant_id = s.tenant_id \
         JOIN organizations o ON o.id = w.tenant_id \
         JOIN organization_memberships om ON om.tenant_id = o.id AND om.user_id = $5 \
         JOIN workspace_memberships wm ON wm.tenant_id = o.id AND wm.workspace_id = w.id AND wm.user_id = $5 \
         WHERE s.tenant_id = $1 AND s.workspace_id = $2 AND s.project_id = $3 AND s.id = $4 \
         AND s.deleted_at IS NULL AND s.status = 'active' AND w.deleted_at IS NULL AND o.deleted_at IS NULL \
         AND om.status = 'active' AND om.deleted_at IS NULL AND wm.status = 'active' AND wm.deleted_at IS NULL \
         FOR SHARE OF s, o, w, om, wm")
        .bind(scope.organization_id).bind(scope.workspace_id).bind(scope.project_id).bind(scope.section_id).bind(actor)
        .fetch_optional(&mut *tx).await.map_err(database_error)?;
    if section.is_none() {
        return Err(WorkItemError::NotAccessible);
    }
    Ok(())
}
async fn require_permission(
    tx: &mut PgConnection,
    scope: WorkItemScope,
    actor: Uuid,
    key: PermissionKey,
) -> Result<(), WorkItemError> {
    if !rbac::lock_workspace_permission_in_tx(
        tx,
        scope.organization_id,
        scope.workspace_id,
        actor,
        key,
    )
    .await
    .map_err(|_| WorkItemError::DatabaseError)?
    {
        return Err(WorkItemError::Forbidden);
    }
    Ok(())
}

pub async fn create_work_item(
    pool: &PgPool,
    scope: WorkItemScope,
    actor: Uuid,
    input: &CreateWorkItemRequest,
) -> Result<WorkItemPublic, WorkItemError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    require_permission(&mut tx, scope, actor, WORK_ITEMS_CREATE).await?;
    let name = name_value(&input.name)?;
    let slug = slug_value(
        input
            .slug
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(&name),
    )?;
    // Bigint intermediate makes exhausted ordering a validation failure, not overflow.
    let next: i64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(max(position)::bigint, -1) + 1 FROM work_items WHERE {SCOPE}"
    ))
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(database_error)?;
    let position = i32::try_from(next).map_err(|_| invalid("position"))?;
    let row = sqlx::query_as(&format!("INSERT INTO work_items (id, tenant_id, workspace_id, project_id, section_id, name, slug, position) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING {COLUMNS}"))
        .bind(Uuid::now_v7()).bind(scope.organization_id).bind(scope.workspace_id).bind(scope.project_id).bind(scope.section_id)
        .bind(name).bind(slug).bind(position).fetch_one(&mut *tx).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(row)
}

pub async fn update_work_item(
    pool: &PgPool,
    scope: WorkItemScope,
    id: Uuid,
    actor: Uuid,
    input: &UpdateWorkItemRequest,
) -> Result<WorkItemPublic, WorkItemError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    let current: WorkItemPublic = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM work_items WHERE {SCOPE} AND id = $5 AND {VISIBLE} FOR UPDATE"
    ))
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?
    .ok_or(WorkItemError::NotAccessible)?;
    require_permission(&mut tx, scope, actor, WORK_ITEMS_UPDATE).await?;
    if input.status.as_deref().map(str::trim) == Some("archived") {
        require_permission(&mut tx, scope, actor, WORK_ITEMS_ARCHIVE).await?;
    }
    let name = match &input.name {
        Some(v) => name_value(v)?,
        None => current.name,
    };
    let slug = match &input.slug {
        Some(v) => slug_value(v)?,
        None => current.slug,
    };
    let position = input.position.unwrap_or(current.position);
    if position < 0 {
        return Err(invalid("position"));
    }
    let status = input
        .status
        .as_deref()
        .map(str::trim)
        .unwrap_or(&current.status);
    if !matches!(status, "active" | "completed" | "archived") {
        return Err(invalid("status"));
    }
    let row = sqlx::query_as(&format!("UPDATE work_items SET name = $6, slug = $7, position = $8, status = $9, updated_at = now() WHERE {SCOPE} AND id = $5 AND {VISIBLE} RETURNING {COLUMNS}"))
        .bind(scope.organization_id).bind(scope.workspace_id).bind(scope.project_id).bind(scope.section_id).bind(id)
        .bind(name).bind(slug).bind(position).bind(status).fetch_one(&mut *tx).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(row)
}

async fn check_permission(
    state: &AppState,
    context: &WorkItemSectionContext,
    key: PermissionKey,
    request_id: &RequestId,
) -> Result<(), ApiError> {
    let scope = context.scope;
    let allowed = rbac::authorize_workspace(
        &state.pool,
        scope.organization_id,
        scope.workspace_id,
        context.user_id,
        key,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(rbac::permission_denied(request_id));
    }
    Ok(())
}

#[utoipa::path(post, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items",
request_body = CreateWorkItemRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path)),
responses((status = 201, body = WorkItemMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn create_work_item_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemSectionContext,
    body: Result<ApiJson<CreateWorkItemRequest>, ApiError>,
) -> Result<(StatusCode, Json<WorkItemMutationResponse>), ApiError> {
    check_permission(&state, &context, WORK_ITEMS_CREATE, &request_id).await?;
    let ApiJson(body) = body?;
    let data = create_work_item(&state.pool, context.scope, context.user_id, &body)
        .await
        .map_err(|e| api_error(e, &request_id))?;
    Ok((StatusCode::CREATED, Json(WorkItemMutationResponse { data })))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path)),
responses((status = 200, body = WorkItemListResponse),(status = 401, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope)))]
pub async fn list_work_items(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemSectionContext,
) -> Result<Json<WorkItemListResponse>, ApiError> {
    let data = visible_for_section(&state.pool, context.scope)
        .await
        .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(WorkItemListResponse { data }))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path)),
responses((status = 200, body = WorkItemPublic),(status = 401, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope)))]
pub async fn get_work_item(context: WorkItemContext) -> Result<Json<WorkItemPublic>, ApiError> {
    Ok(Json(context.work_item))
}

#[utoipa::path(patch, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}",
request_body = UpdateWorkItemRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path)),
responses((status = 200, body = WorkItemMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn update_work_item_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemContext,
    body: Result<ApiJson<UpdateWorkItemRequest>, ApiError>,
) -> Result<Json<WorkItemMutationResponse>, ApiError> {
    check_permission(&state, &context.parent, WORK_ITEMS_UPDATE, &request_id).await?;
    let ApiJson(body) = body?;
    let data = update_work_item(
        &state.pool,
        context.parent.scope,
        context.work_item.id,
        context.parent.user_id,
        &body,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(WorkItemMutationResponse { data }))
}
