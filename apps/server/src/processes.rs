//! Ordered process DEFINITIONS inside a work item (ADR 0015).
//!
//! Configuration only: which processes exist, their order and whether they
//! are required. Runtime execution (start/finish, timers, assignees,
//! progress) is a separate future model that references these rows by id.
use crate::{
    AppState, RequestId,
    auth::ApiJson,
    context::{ProcessContext, WorkItemContext},
    error::{ApiError, ErrorCode},
    organizations::{slug_from_text, validate_slug},
    rbac::{self, PermissionKey},
    work_items::{self, WorkItemError, WorkItemScope},
};
use axum::{Extension, Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use std::collections::HashSet;
use utoipa::ToSchema;
use uuid::Uuid;

pub const PROCESSES_CREATE: PermissionKey = PermissionKey("processes:create");
pub const PROCESSES_UPDATE: PermissionKey = PermissionKey("processes:update");
pub const PROCESSES_ARCHIVE: PermissionKey = PermissionKey("processes:archive");
pub const PROCESSES_REORDER: PermissionKey = PermissionKey("processes:reorder");

const DESCRIPTION_MAX_CHARS: usize = 2000;

/// Server-resolved route identity, never deserialized from request bodies.
#[derive(Clone, Copy)]
pub struct ProcessScope {
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub section_id: Uuid,
    pub work_item_id: Uuid,
}
impl ProcessScope {
    pub fn of(context: &WorkItemContext) -> Self {
        let s = context.parent.scope;
        Self {
            organization_id: s.organization_id,
            workspace_id: s.workspace_id,
            project_id: s.project_id,
            section_id: s.section_id,
            work_item_id: context.work_item.id,
        }
    }
    fn work_item_scope(self) -> WorkItemScope {
        WorkItemScope {
            organization_id: self.organization_id,
            workspace_id: self.workspace_id,
            project_id: self.project_id,
            section_id: self.section_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ProcessPublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub section_id: Uuid,
    pub work_item_id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub position: i32,
    pub is_required: bool,
    pub status: String,
}

const COLUMNS: &str = "id, tenant_id AS organization_id, workspace_id, project_id, section_id, work_item_id, name, slug::text AS slug, description, position, is_required, status";
const SCOPE: &str = "tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4 AND work_item_id = $5";
const VISIBLE: &str = "deleted_at IS NULL AND status = 'active'";

#[derive(Debug)]
pub enum ProcessError {
    NotAccessible,
    Forbidden,
    InvalidFields(serde_json::Value),
    DatabaseError,
}
impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotAccessible => "resource not found",
            Self::Forbidden => "permission denied",
            Self::InvalidFields(_) => "invalid fields",
            Self::DatabaseError => "database operation failed",
        })
    }
}
impl std::error::Error for ProcessError {}

fn invalid(field: &str) -> ProcessError {
    ProcessError::InvalidFields(serde_json::json!({field: ["Invalid or unavailable"]}))
}
fn database_error(error: sqlx::Error) -> ProcessError {
    if let sqlx::Error::Database(ref db) = error
        && db.constraint() == Some("processes_work_item_slug_key")
    {
        return invalid("slug");
    }
    ProcessError::DatabaseError
}
fn api_error(error: ProcessError, request_id: &RequestId) -> ApiError {
    match error {
        ProcessError::NotAccessible => {
            ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone())
        }
        ProcessError::Forbidden => rbac::permission_denied(request_id),
        ProcessError::InvalidFields(fields) => {
            ApiError::invalid_fields(fields, request_id.0.clone())
        }
        ProcessError::DatabaseError => {
            ApiError::new(ErrorCode::InternalError, request_id.0.clone())
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProcessRequest {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    /// Defaults to true.
    pub is_required: Option<bool>,
}
#[derive(Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateProcessRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    /// Empty string clears the stored description.
    pub description: Option<String>,
    pub is_required: Option<bool>,
    /// Only `archived` changes state; archive is terminal in this V1 API.
    pub status: Option<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReorderProcessesRequest {
    /// Every active process of the work item, exactly once, in the desired order.
    pub process_ids: Vec<Uuid>,
}
#[derive(Serialize, ToSchema)]
pub struct ProcessMutationResponse {
    pub data: ProcessPublic,
}
#[derive(Serialize, ToSchema)]
pub struct ProcessListResponse {
    pub data: Vec<ProcessPublic>,
}

fn name_value(value: &str) -> Result<String, ProcessError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(invalid("name"));
    }
    Ok(value.to_owned())
}
fn slug_value(value: &str) -> Result<String, ProcessError> {
    slug_from_text(value.trim())
        .filter(|s| validate_slug(s))
        .ok_or_else(|| invalid("slug"))
}
fn description_value(value: &str) -> Result<Option<String>, ProcessError> {
    let value = value.trim();
    if value.chars().count() > DESCRIPTION_MAX_CHARS {
        return Err(invalid("description"));
    }
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

fn bind_scope<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    scope: ProcessScope,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    query
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
}

pub async fn find_accessible(
    pool: &PgPool,
    scope: ProcessScope,
    id: Uuid,
) -> Result<Option<ProcessPublic>, ProcessError> {
    let sql = format!("SELECT {COLUMNS} FROM processes WHERE {SCOPE} AND id = $6 AND {VISIBLE}");
    bind_scope(sqlx::query_as(&sql), scope)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(database_error)
}
pub async fn visible_for_work_item(
    pool: &PgPool,
    scope: ProcessScope,
) -> Result<Vec<ProcessPublic>, ProcessError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM processes WHERE {SCOPE} AND {VISIBLE} ORDER BY position, id"
    );
    bind_scope(sqlx::query_as(&sql), scope)
        .fetch_all(pool)
        .await
        .map_err(database_error)
}

/// Lock order (ADR 0015) extends the work item order without reordering it:
/// project FOR UPDATE → direct section + org/workspace + both memberships
/// FOR SHARE → work item FOR SHARE → permission support rows FOR SHARE →
/// process rows FOR UPDATE. The project lock serializes all writes in the
/// project, so appends and reorders cannot interleave.
async fn lock_parent(
    tx: &mut PgConnection,
    scope: ProcessScope,
    actor: Uuid,
) -> Result<(), ProcessError> {
    work_items::lock_parent(tx, scope.work_item_scope(), actor)
        .await
        .map_err(|e| match e {
            WorkItemError::NotAccessible => ProcessError::NotAccessible,
            _ => ProcessError::DatabaseError,
        })?;
    let item = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM work_items WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 \
         AND section_id = $4 AND id = $5 AND deleted_at IS NULL AND status <> 'archived' FOR SHARE",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(scope.work_item_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?;
    item.map(|_| ()).ok_or(ProcessError::NotAccessible)
}
async fn require_permission(
    tx: &mut PgConnection,
    scope: ProcessScope,
    actor: Uuid,
    key: PermissionKey,
) -> Result<(), ProcessError> {
    if !rbac::lock_workspace_permission_in_tx(
        tx,
        scope.organization_id,
        scope.workspace_id,
        actor,
        key,
    )
    .await
    .map_err(|_| ProcessError::DatabaseError)?
    {
        return Err(ProcessError::Forbidden);
    }
    Ok(())
}

pub async fn create_process(
    pool: &PgPool,
    scope: ProcessScope,
    actor: Uuid,
    input: &CreateProcessRequest,
) -> Result<ProcessPublic, ProcessError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    require_permission(&mut tx, scope, actor, PROCESSES_CREATE).await?;
    let name = name_value(&input.name)?;
    let slug = slug_value(
        input
            .slug
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(&name),
    )?;
    let description = match &input.description {
        Some(v) => description_value(v)?,
        None => None,
    };
    // Append after every stored row (archived included), so a new active
    // process can never collide with an active position. Bigint
    // intermediate turns exhausted ordering into validation, not overflow.
    let sql =
        format!("SELECT COALESCE(max(position)::bigint, -1) + 1 FROM processes WHERE {SCOPE}");
    let next: i64 = sqlx::query_scalar(&sql)
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let position = i32::try_from(next).map_err(|_| invalid("position"))?;
    let sql = format!(
        "INSERT INTO processes (id, tenant_id, workspace_id, project_id, section_id, work_item_id, name, slug, description, position, is_required) \
         VALUES ($6, $1, $2, $3, $4, $5, $7, $8, $9, $10, $11) RETURNING {COLUMNS}"
    );
    let row = bind_scope(sqlx::query_as(&sql), scope)
        .bind(Uuid::now_v7())
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(position)
        .bind(input.is_required.unwrap_or(true))
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(row)
}

pub async fn update_process(
    pool: &PgPool,
    scope: ProcessScope,
    id: Uuid,
    actor: Uuid,
    input: &UpdateProcessRequest,
) -> Result<ProcessPublic, ProcessError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    let sql = format!(
        "SELECT {COLUMNS} FROM processes WHERE {SCOPE} AND id = $6 AND {VISIBLE} FOR UPDATE"
    );
    let current: ProcessPublic = bind_scope(sqlx::query_as(&sql), scope)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or(ProcessError::NotAccessible)?;
    require_permission(&mut tx, scope, actor, PROCESSES_UPDATE).await?;
    let status = input
        .status
        .as_deref()
        .map(str::trim)
        .unwrap_or(&current.status);
    if status == "archived" {
        require_permission(&mut tx, scope, actor, PROCESSES_ARCHIVE).await?;
    }
    if !matches!(status, "active" | "archived") {
        return Err(invalid("status"));
    }
    let name = match &input.name {
        Some(v) => name_value(v)?,
        None => current.name,
    };
    let slug = match &input.slug {
        Some(v) => slug_value(v)?,
        None => current.slug,
    };
    let description = match &input.description {
        Some(v) => description_value(v)?,
        None => current.description,
    };
    let is_required = input.is_required.unwrap_or(current.is_required);
    let sql = format!(
        "UPDATE processes SET name = $7, slug = $8, description = $9, is_required = $10, status = $11, updated_at = now() \
         WHERE {SCOPE} AND id = $6 AND {VISIBLE} RETURNING {COLUMNS}"
    );
    let row = bind_scope(sqlx::query_as(&sql), scope)
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(is_required)
        .bind(status)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(row)
}

/// Server-authoritative reorder: the request must name every ACTIVE process
/// of this work item exactly once. Unknown, foreign, archived, duplicated or
/// missing ids share one validation error (no existence oracle). Positions
/// become 0..n-1 in two phases so the active-position unique index never
/// sees a transient duplicate.
pub async fn reorder_processes(
    pool: &PgPool,
    scope: ProcessScope,
    actor: Uuid,
    input: &ReorderProcessesRequest,
) -> Result<Vec<ProcessPublic>, ProcessError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    require_permission(&mut tx, scope, actor, PROCESSES_REORDER).await?;
    let requested: HashSet<Uuid> = input.process_ids.iter().copied().collect();
    if requested.len() != input.process_ids.len() {
        return Err(invalid("process_ids"));
    }
    let sql = format!(
        "SELECT id, position FROM processes WHERE {SCOPE} AND {VISIBLE} ORDER BY position, id FOR UPDATE"
    );
    let active: Vec<(Uuid, i32)> = sqlx::query_as(&sql)
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(database_error)?;
    if active.len() != requested.len() || active.iter().any(|(id, _)| !requested.contains(id)) {
        return Err(invalid("process_ids"));
    }
    if !active.is_empty() {
        // Phase 1 moves every row strictly above the current maximum; phase 2
        // targets 0..n-1, which is below every phase-1 value (max >= n-1).
        let base = i64::from(active.iter().map(|(_, p)| *p).max().unwrap_or(0)) + 1;
        let top = base + i64::try_from(active.len()).map_err(|_| invalid("process_ids"))?;
        let base = i32::try_from(base).map_err(|_| invalid("process_ids"))?;
        i32::try_from(top).map_err(|_| invalid("process_ids"))?;
        for position_sql in ["$6 + o.ord::integer", "o.ord::integer - 1"] {
            let sql = format!(
                "UPDATE processes p SET position = {position_sql}, updated_at = now() \
                 FROM unnest($7::uuid[]) WITH ORDINALITY AS o(id, ord) \
                 WHERE p.id = o.id AND p.tenant_id = $1 AND p.workspace_id = $2 \
                 AND p.project_id = $3 AND p.section_id = $4 AND p.work_item_id = $5 \
                 AND p.deleted_at IS NULL AND p.status = 'active'"
            );
            sqlx::query(&sql)
                .bind(scope.organization_id)
                .bind(scope.workspace_id)
                .bind(scope.project_id)
                .bind(scope.section_id)
                .bind(scope.work_item_id)
                .bind(base)
                .bind(&input.process_ids)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
    }
    let sql = format!(
        "SELECT {COLUMNS} FROM processes WHERE {SCOPE} AND {VISIBLE} ORDER BY position, id"
    );
    let rows = bind_scope(sqlx::query_as(&sql), scope)
        .fetch_all(&mut *tx)
        .await
        .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(rows)
}

async fn check_permission(
    state: &AppState,
    context: &WorkItemContext,
    key: PermissionKey,
    request_id: &RequestId,
) -> Result<(), ApiError> {
    let scope = context.parent.scope;
    let allowed = rbac::authorize_workspace(
        &state.pool,
        scope.organization_id,
        scope.workspace_id,
        context.parent.user_id,
        key,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(rbac::permission_denied(request_id));
    }
    Ok(())
}

#[utoipa::path(post, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes",
request_body = CreateProcessRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path)),
responses((status = 201, body = ProcessMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn create_process_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemContext,
    body: Result<ApiJson<CreateProcessRequest>, ApiError>,
) -> Result<(StatusCode, Json<ProcessMutationResponse>), ApiError> {
    check_permission(&state, &context, PROCESSES_CREATE, &request_id).await?;
    let ApiJson(body) = body?;
    let data = create_process(
        &state.pool,
        ProcessScope::of(&context),
        context.parent.user_id,
        &body,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok((StatusCode::CREATED, Json(ProcessMutationResponse { data })))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path)),
responses((status = 200, body = ProcessListResponse),(status = 401, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope)))]
pub async fn list_processes(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemContext,
) -> Result<Json<ProcessListResponse>, ApiError> {
    let data = visible_for_work_item(&state.pool, ProcessScope::of(&context))
        .await
        .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(ProcessListResponse { data }))
}

#[utoipa::path(patch, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/reorder",
request_body = ReorderProcessesRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path)),
responses((status = 200, body = ProcessListResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn reorder_processes_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemContext,
    body: Result<ApiJson<ReorderProcessesRequest>, ApiError>,
) -> Result<Json<ProcessListResponse>, ApiError> {
    check_permission(&state, &context, PROCESSES_REORDER, &request_id).await?;
    let ApiJson(body) = body?;
    let data = reorder_processes(
        &state.pool,
        ProcessScope::of(&context),
        context.parent.user_id,
        &body,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(ProcessListResponse { data }))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path)),
responses((status = 200, body = ProcessPublic),(status = 401, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope)))]
pub async fn get_process(context: ProcessContext) -> Result<Json<ProcessPublic>, ApiError> {
    Ok(Json(context.process))
}

#[utoipa::path(patch, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}",
request_body = UpdateProcessRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path)),
responses((status = 200, body = ProcessMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn update_process_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProcessContext,
    body: Result<ApiJson<UpdateProcessRequest>, ApiError>,
) -> Result<Json<ProcessMutationResponse>, ApiError> {
    check_permission(&state, &context.parent, PROCESSES_UPDATE, &request_id).await?;
    let ApiJson(body) = body?;
    let data = update_process(
        &state.pool,
        ProcessScope::of(&context.parent),
        context.process.id,
        context.parent.parent.user_id,
        &body,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(ProcessMutationResponse { data }))
}
