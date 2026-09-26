//! Process EXECUTION attempts (STEP 21A, ADR 0016).
//!
//! `processes` rows are definitions (what should happen); `process_executions`
//! rows are immutable attempts (what actually happened). A definition has
//! 0..N attempts, at most one `active` at a time; terminal attempts never
//! change. There is no stored `pending`: a process with no active attempt is
//! pending implicitly. No timers, progress, assignees or sessions live here —
//! elapsed time derives from timestamps and the DB clock is authoritative.
use crate::{
    AppState, RequestId,
    auth::ApiJson,
    context::{ProcessContext, ProcessExecutionContext, WorkItemContext},
    error::{ApiError, ErrorCode},
    processes::{self, ProcessError, ProcessScope},
    rbac::{self, PermissionKey},
    work_items::WorkItemScope,
};
use axum::{Extension, Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

pub const PROCESS_EXECUTIONS_START: PermissionKey = PermissionKey("process_executions:start");
pub const PROCESS_EXECUTIONS_COMPLETE: PermissionKey = PermissionKey("process_executions:complete");
pub const PROCESS_EXECUTIONS_CANCEL: PermissionKey = PermissionKey("process_executions:cancel");

const REASON_MAX_CHARS: usize = 500;

/// Server-resolved route identity: the full parent chain plus the process.
/// Never deserialized from request bodies.
#[derive(Clone, Copy)]
pub struct ExecutionScope {
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub section_id: Uuid,
    pub work_item_id: Uuid,
    pub process_id: Uuid,
}
impl ExecutionScope {
    pub fn of(context: &ProcessContext) -> Self {
        let s = context.parent.parent.scope;
        Self {
            organization_id: s.organization_id,
            workspace_id: s.workspace_id,
            project_id: s.project_id,
            section_id: s.section_id,
            work_item_id: context.parent.work_item.id,
            process_id: context.process.id,
        }
    }
    fn process_scope(self) -> ProcessScope {
        ProcessScope {
            organization_id: self.organization_id,
            workspace_id: self.workspace_id,
            project_id: self.project_id,
            section_id: self.section_id,
            work_item_id: self.work_item_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ExecutionPublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub section_id: Uuid,
    pub work_item_id: Uuid,
    pub process_id: Uuid,
    pub attempt_no: i32,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub start_reason: Option<String>,
    pub cancel_reason: Option<String>,
    pub started_by_user_id: Uuid,
    pub completed_by_user_id: Option<Uuid>,
    pub cancelled_by_user_id: Option<Uuid>,
    /// Responsibility snapshot (STEP 21C, ADR 0018): who the process was
    /// assigned to when this attempt began. Written once at INSERT, never
    /// updated; distinct from the actor fields above.
    pub assignee_user_id: Option<Uuid>,
}

// Timestamps serialize as ISO-8601 UTC strings (invitations convention); the
// database clock writes them, never the client.
const TS: &str = "YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"";
const COLUMNS: &str = "id, tenant_id AS organization_id, workspace_id, project_id, section_id, \
    work_item_id, process_id, attempt_no, status, \
    to_char(started_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS started_at, \
    to_char(completed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS completed_at, \
    to_char(cancelled_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS cancelled_at, \
    start_reason, cancel_reason, started_by_user_id, completed_by_user_id, cancelled_by_user_id, \
    assignee_user_id";
pub(crate) const SCOPE: &str = "tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4 AND work_item_id = $5 AND process_id = $6";

#[derive(Debug)]
pub enum ExecutionError {
    NotAccessible,
    Forbidden,
    InvalidFields(serde_json::Value),
    /// The target exists and is visible, but its current state does not allow
    /// the requested transition (already active / already terminal).
    StateConflict,
    DatabaseError,
}
impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotAccessible => "resource not found",
            Self::Forbidden => "permission denied",
            Self::InvalidFields(_) => "invalid fields",
            Self::StateConflict => "state conflict",
            Self::DatabaseError => "database operation failed",
        })
    }
}
impl std::error::Error for ExecutionError {}

fn invalid(field: &str) -> ExecutionError {
    ExecutionError::InvalidFields(serde_json::json!({field: ["Invalid or unavailable"]}))
}
fn database_error(error: sqlx::Error) -> ExecutionError {
    // Structural backstops: a second concurrent active attempt or a duplicate
    // attempt number surfaces as a state conflict, never a raw 23505.
    if let sqlx::Error::Database(ref db) = error
        && matches!(
            db.constraint(),
            Some("process_executions_one_active" | "process_executions_attempt_key")
        )
    {
        return ExecutionError::StateConflict;
    }
    ExecutionError::DatabaseError
}
fn api_error(error: ExecutionError, request_id: &RequestId) -> ApiError {
    match error {
        ExecutionError::NotAccessible => {
            ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone())
        }
        ExecutionError::Forbidden => rbac::permission_denied(request_id),
        ExecutionError::InvalidFields(fields) => {
            ApiError::invalid_fields(fields, request_id.0.clone())
        }
        ExecutionError::StateConflict => {
            ApiError::new(ErrorCode::StateConflict, request_id.0.clone())
        }
        ExecutionError::DatabaseError => {
            ApiError::new(ErrorCode::InternalError, request_id.0.clone())
        }
    }
}

#[derive(Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StartExecutionRequest {
    /// Optional bounded note for operational history (covers retry reasons).
    pub start_reason: Option<String>,
}
#[derive(Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelExecutionRequest {
    /// Optional bounded note explaining why the attempt was abandoned.
    pub cancel_reason: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct ExecutionMutationResponse {
    pub data: ExecutionPublic,
    /// Authoritative server timestamp for client clock-offset correction.
    pub server_time: String,
}
#[derive(Serialize, ToSchema)]
pub struct ExecutionListResponse {
    pub data: Vec<ExecutionPublic>,
    pub server_time: String,
}

fn reason_value(
    field: &'static str,
    value: Option<&String>,
) -> Result<Option<String>, ExecutionError> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > REASON_MAX_CHARS {
        return Err(invalid(field));
    }
    Ok(Some(value.to_owned()))
}

pub(crate) fn bind_scope<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    scope: ExecutionScope,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    query
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
        .bind(scope.process_id)
}
pub(crate) fn bind_scope_scalar<'q, T>(
    query: sqlx::query::QueryScalar<'q, sqlx::Postgres, T, sqlx::postgres::PgArguments>,
    scope: ExecutionScope,
) -> sqlx::query::QueryScalar<'q, sqlx::Postgres, T, sqlx::postgres::PgArguments> {
    query
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
        .bind(scope.process_id)
}

/// Authoritative server timestamp for client timer anchoring.
pub(crate) async fn server_time(executor: &mut PgConnection) -> Result<String, ExecutionError> {
    sqlx::query_scalar::<_, String>(&format!("SELECT to_char(now() AT TIME ZONE 'UTC', '{TS}')"))
        .fetch_one(executor)
        .await
        .map_err(database_error)
}

pub async fn find_accessible(
    pool: &PgPool,
    scope: ExecutionScope,
    id: Uuid,
) -> Result<Option<ExecutionPublic>, ExecutionError> {
    let sql = format!("SELECT {COLUMNS} FROM process_executions WHERE {SCOPE} AND id = $7");
    bind_scope(sqlx::query_as(&sql), scope)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(database_error)
}

/// All attempts under one work item, deterministic order for the detail page:
/// grouped by process, oldest attempt first.
pub async fn list_for_work_item(
    pool: &PgPool,
    scope: WorkItemScope,
    work_item_id: Uuid,
) -> Result<(Vec<ExecutionPublic>, String), ExecutionError> {
    let mut conn = pool.acquire().await.map_err(database_error)?;
    let sql = format!(
        "SELECT {COLUMNS} FROM process_executions \
        WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4 AND work_item_id = $5 \
        ORDER BY process_id, attempt_no"
    );
    let rows = sqlx::query_as::<_, ExecutionPublic>(&sql)
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(work_item_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(database_error)?;
    let now = server_time(&mut conn).await?;
    Ok((rows, now))
}

/// Lock order (ADR 0016) extends the STEP 20 chain unchanged: project
/// FOR UPDATE → direct section + org/workspace + both memberships FOR SHARE →
/// work item FOR SHARE → permission support rows FOR SHARE → process
/// FOR SHARE → execution rows FOR UPDATE. The project lock serializes all
/// writes in the project, so starts, transitions, retries and archive guards
/// cannot interleave.
pub(crate) async fn lock_parent(
    tx: &mut PgConnection,
    scope: ExecutionScope,
    actor: Uuid,
) -> Result<(), ExecutionError> {
    // Identical chain to process definitions: project FOR UPDATE → section +
    // org/workspace + memberships FOR SHARE → work item FOR SHARE →
    // permission rows FOR SHARE. The process row lock is appended last.
    processes::lock_parent(tx, scope.process_scope(), actor)
        .await
        .map_err(|e| match e {
            ProcessError::NotAccessible => ExecutionError::NotAccessible,
            _ => ExecutionError::DatabaseError,
        })?;
    let process = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM processes WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 \
         AND section_id = $4 AND work_item_id = $5 AND id = $6 \
         AND deleted_at IS NULL AND status = 'active' FOR SHARE",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(scope.work_item_id)
    .bind(scope.process_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?;
    process.map(|_| ()).ok_or(ExecutionError::NotAccessible)
}

pub(crate) async fn require_permission(
    tx: &mut PgConnection,
    scope: ExecutionScope,
    actor: Uuid,
    key: PermissionKey,
) -> Result<(), ExecutionError> {
    if !rbac::lock_workspace_permission_in_tx(
        tx,
        scope.organization_id,
        scope.workspace_id,
        actor,
        key,
    )
    .await
    .map_err(|_| ExecutionError::DatabaseError)?
    {
        return Err(ExecutionError::Forbidden);
    }
    Ok(())
}

/// START: create the next attempt for a visible process. `attempt_no` is
/// `max+1` under the project lock, so concurrent starts and retries can
/// never interleave; `process_executions_one_active` and the attempt key are
/// structural backstops.
pub async fn start_execution(
    pool: &PgPool,
    scope: ExecutionScope,
    actor: Uuid,
    input: &StartExecutionRequest,
) -> Result<(ExecutionPublic, String), ExecutionError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    require_permission(&mut tx, scope, actor, PROCESS_EXECUTIONS_START).await?;
    let start_reason = reason_value("start_reason", input.start_reason.as_ref())?;
    let sql = format!("SELECT id FROM process_executions WHERE {SCOPE} AND status = 'active'");
    let active: Option<Uuid> = bind_scope_scalar(sqlx::query_scalar(&sql), scope)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?;
    if active.is_some() {
        return Err(ExecutionError::StateConflict);
    }
    let sql = format!(
        "SELECT COALESCE(max(attempt_no)::bigint, 0) + 1 FROM process_executions WHERE {SCOPE}"
    );
    let next: i64 = bind_scope_scalar(sqlx::query_scalar(&sql), scope)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let attempt_no = i32::try_from(next).map_err(|_| invalid("attempt_no"))?;
    // Responsibility snapshot (ADR 0018): read the process's CURRENT assignee
    // inside this transaction. The process row is already FOR SHARE-locked by
    // lock_parent, so a concurrent reassignment serializes — the snapshot is
    // always one complete, consistent value, never a mixed read.
    let assignee = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT assignee_user_id FROM processes \
         WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 \
           AND section_id = $4 AND work_item_id = $5 AND id = $6",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(scope.work_item_id)
    .bind(scope.process_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?
    .ok_or(ExecutionError::NotAccessible)?;
    let sql = format!(
        "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, start_reason, started_by_user_id, assignee_user_id) \
         VALUES ($7, $1, $2, $3, $4, $5, $6, $8, $9, $10, $11) RETURNING {COLUMNS}"
    );
    let row = bind_scope(sqlx::query_as(&sql), scope)
        .bind(Uuid::now_v7())
        .bind(attempt_no)
        .bind(start_reason)
        .bind(actor)
        .bind(assignee)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let now = server_time(&mut tx).await?;
    tx.commit().await.map_err(database_error)?;
    Ok((row, now))
}

/// Terminal transition shared by COMPLETE and CANCEL: the execution row is
/// locked FOR UPDATE inside the full scope and must still be `active`.
/// Terminal attempts are immutable — a second transition is a 409.
async fn transition_execution(
    pool: &PgPool,
    scope: ExecutionScope,
    id: Uuid,
    actor: Uuid,
    permission: PermissionKey,
    target: &'static str,
    cancel_reason_input: Option<&String>,
) -> Result<(ExecutionPublic, String), ExecutionError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    require_permission(&mut tx, scope, actor, permission).await?;
    // Validation sits after eligibility (404/403) and before the state
    // conflict check, matching the repository's error ordering.
    let cancel_reason = reason_value("cancel_reason", cancel_reason_input)?;
    let sql =
        format!("SELECT {COLUMNS} FROM process_executions WHERE {SCOPE} AND id = $7 FOR UPDATE");
    let current: ExecutionPublic = bind_scope(sqlx::query_as(&sql), scope)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or(ExecutionError::NotAccessible)?;
    if current.status != "active" {
        return Err(ExecutionError::StateConflict);
    }
    let set = match target {
        "completed" => "status = 'completed', completed_at = now(), completed_by_user_id = $8",
        _ => {
            "status = 'cancelled', cancelled_at = now(), cancelled_by_user_id = $8, cancel_reason = $9"
        }
    };
    let sql = format!(
        "UPDATE process_executions SET {set}, updated_at = now() \
         WHERE {SCOPE} AND id = $7 AND status = 'active' RETURNING {COLUMNS}"
    );
    let query = bind_scope(sqlx::query_as(&sql), scope).bind(id).bind(actor);
    let query = if target == "cancelled" {
        query.bind(cancel_reason)
    } else {
        query
    };
    let row = query.fetch_one(&mut *tx).await.map_err(database_error)?;
    // ADR 0019: a terminal transition closes every open time session of this
    // attempt atomically. now() = transaction_timestamp(), so each
    // session.ended_at equals the execution's terminal timestamp exactly and
    // ended_by records the transition actor (not the worker).
    crate::time_sessions::close_open_sessions_in_tx(&mut tx, scope, id, actor)
        .await
        .map_err(database_error)?;
    let now = server_time(&mut tx).await?;
    tx.commit().await.map_err(database_error)?;
    Ok((row, now))
}

pub async fn complete_execution(
    pool: &PgPool,
    scope: ExecutionScope,
    id: Uuid,
    actor: Uuid,
) -> Result<(ExecutionPublic, String), ExecutionError> {
    transition_execution(
        pool,
        scope,
        id,
        actor,
        PROCESS_EXECUTIONS_COMPLETE,
        "completed",
        None,
    )
    .await
}

pub async fn cancel_execution(
    pool: &PgPool,
    scope: ExecutionScope,
    id: Uuid,
    actor: Uuid,
    input: &CancelExecutionRequest,
) -> Result<(ExecutionPublic, String), ExecutionError> {
    transition_execution(
        pool,
        scope,
        id,
        actor,
        PROCESS_EXECUTIONS_CANCEL,
        "cancelled",
        input.cancel_reason.as_ref(),
    )
    .await
}

/// Active-execution guards for ancestor/definition archive paths (ADR 0016).
/// Each runs inside the caller's transaction under the already-held project
/// lock, so an archive can never commit alongside a racing START.
pub async fn active_execution_exists_for_process(
    tx: &mut PgConnection,
    scope: ProcessScope,
    process_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM process_executions \
         WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4 \
         AND work_item_id = $5 AND process_id = $6 AND status = 'active')",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(scope.work_item_id)
    .bind(process_id)
    .fetch_one(&mut *tx)
    .await
}
pub async fn active_execution_exists_for_work_item(
    tx: &mut PgConnection,
    scope: WorkItemScope,
    work_item_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM process_executions \
         WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4 \
         AND work_item_id = $5 AND status = 'active')",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(work_item_id)
    .fetch_one(&mut *tx)
    .await
}
/// Section guard covers the ENTIRE subtree (target + descendants), via the
/// same UNION-based cycle-safe walk as `sections::is_self_or_descendant`.
pub async fn active_execution_exists_for_section_subtree(
    tx: &mut PgConnection,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "WITH RECURSIVE subtree AS ( \
            SELECT id FROM sections \
            WHERE id = $4 AND project_id = $3 AND workspace_id = $2 AND tenant_id = $1 \
            UNION \
            SELECT s.id FROM sections s \
            JOIN subtree d ON s.parent_section_id = d.id \
            WHERE s.project_id = $3 AND s.workspace_id = $2 AND s.tenant_id = $1 \
        ) \
        SELECT EXISTS(SELECT 1 FROM process_executions e \
            JOIN subtree st ON e.section_id = st.id \
            WHERE e.tenant_id = $1 AND e.workspace_id = $2 AND e.project_id = $3 \
              AND e.status = 'active')",
    )
    .bind(organization_id)
    .bind(workspace_id)
    .bind(project_id)
    .bind(section_id)
    .fetch_one(&mut *tx)
    .await
}
pub async fn active_execution_exists_for_project(
    tx: &mut PgConnection,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM process_executions \
         WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND status = 'active')",
    )
    .bind(organization_id)
    .bind(workspace_id)
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
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

#[utoipa::path(post, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions",
request_body = StartExecutionRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path)),
responses((status = 201, body = ExecutionMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 409, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn start_execution_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProcessContext,
    body: Result<ApiJson<StartExecutionRequest>, ApiError>,
) -> Result<(StatusCode, Json<ExecutionMutationResponse>), ApiError> {
    check_permission(
        &state,
        &context.parent,
        PROCESS_EXECUTIONS_START,
        &request_id,
    )
    .await?;
    let ApiJson(body) = body?;
    let (data, server_time) = start_execution(
        &state.pool,
        ExecutionScope::of(&context),
        context.parent.parent.user_id,
        &body,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok((
        StatusCode::CREATED,
        Json(ExecutionMutationResponse { data, server_time }),
    ))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/executions",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path)),
responses((status = 200, body = ExecutionListResponse),(status = 401, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope)))]
pub async fn list_work_item_executions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemContext,
) -> Result<Json<ExecutionListResponse>, ApiError> {
    let (data, server_time) =
        list_for_work_item(&state.pool, context.parent.scope, context.work_item.id)
            .await
            .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(ExecutionListResponse { data, server_time }))
}

#[utoipa::path(post, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions/{execution_id}/complete",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path),("execution_id" = Uuid, Path)),
responses((status = 200, body = ExecutionMutationResponse),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 409, body = crate::error::ErrorEnvelope)))]
pub async fn complete_execution_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProcessExecutionContext,
) -> Result<Json<ExecutionMutationResponse>, ApiError> {
    check_permission(
        &state,
        &context.parent.parent,
        PROCESS_EXECUTIONS_COMPLETE,
        &request_id,
    )
    .await?;
    let (data, server_time) = complete_execution(
        &state.pool,
        ExecutionScope::of(&context.parent),
        context.execution.id,
        context.parent.parent.parent.user_id,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(ExecutionMutationResponse { data, server_time }))
}

#[utoipa::path(post, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions/{execution_id}/cancel",
request_body = CancelExecutionRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path),("execution_id" = Uuid, Path)),
responses((status = 200, body = ExecutionMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 409, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn cancel_execution_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProcessExecutionContext,
    body: Result<ApiJson<CancelExecutionRequest>, ApiError>,
) -> Result<Json<ExecutionMutationResponse>, ApiError> {
    check_permission(
        &state,
        &context.parent.parent,
        PROCESS_EXECUTIONS_CANCEL,
        &request_id,
    )
    .await?;
    let ApiJson(body) = body?;
    let (data, server_time) = cancel_execution(
        &state.pool,
        ExecutionScope::of(&context.parent),
        context.execution.id,
        context.parent.parent.parent.user_id,
        &body,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(ExecutionMutationResponse { data, server_time }))
}
