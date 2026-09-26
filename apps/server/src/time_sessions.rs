//! TIME SESSIONS (STEP 21D, ADR 0019): tracked labor intervals under
//! process executions.
//!
//! One row = one contiguous interval of ONE worker actively working on ONE
//! execution attempt. Open = `ended_at IS NULL`; there is no status column.
//! Pause = close the row; resume = INSERT a new row; terminal execution
//! transitions close every open row at the transition timestamp.
//!
//! Identity boundaries (approved invariants): the WORKER is who accumulates
//! labor; `started_by`/`ended_by` are who performed the API action; the
//! assignee columns (STEP 21C) record responsibility. All three may differ.
//! V1 is self-service: the authenticated actor is always the worker.
use crate::{
    AppState, RequestId,
    auth::ApiJson,
    context::{ProcessExecutionContext, TimeSessionContext, WorkItemContext},
    error::{ApiError, ErrorCode},
    process_executions::{self, ExecutionError, ExecutionScope},
    rbac::{self, PermissionKey},
    work_items::WorkItemScope,
};
use axum::{Extension, Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

pub const TIME_SESSIONS_START: PermissionKey = PermissionKey("time_sessions:start");
pub const TIME_SESSIONS_STOP: PermissionKey = PermissionKey("time_sessions:stop");

/// Minimal public worker identity — never membership internals or email.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct SessionWorkerPublic {
    pub id: Uuid,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TimeSessionPublic {
    pub id: Uuid,
    pub process_execution_id: Uuid,
    pub worker: SessionWorkerPublic,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub started_by_user_id: Uuid,
    pub ended_by_user_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: Uuid,
    process_execution_id: Uuid,
    worker_id: Uuid,
    worker_display_name: String,
    started_at: String,
    ended_at: Option<String>,
    started_by_user_id: Uuid,
    ended_by_user_id: Option<Uuid>,
}
impl From<SessionRow> for TimeSessionPublic {
    fn from(row: SessionRow) -> Self {
        Self {
            id: row.id,
            process_execution_id: row.process_execution_id,
            worker: SessionWorkerPublic {
                id: row.worker_id,
                display_name: row.worker_display_name,
            },
            started_at: row.started_at,
            ended_at: row.ended_at,
            started_by_user_id: row.started_by_user_id,
            ended_by_user_id: row.ended_by_user_id,
        }
    }
}

// Timestamps serialize as ISO-8601 UTC strings; the database clock writes
// them, never the client. The worker join has no status filter: historical
// sessions of revoked users must stay attributable (ADR 0019).
const COLUMNS: &str = "s.id, s.process_execution_id, s.worker_user_id AS worker_id, \
    u.display_name AS worker_display_name, \
    to_char(s.started_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS started_at, \
    to_char(s.ended_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS ended_at, \
    s.started_by_user_id, s.ended_by_user_id";
const FROM: &str = "process_execution_time_sessions s JOIN users u ON u.id = s.worker_user_id";
const SCOPE: &str = "s.tenant_id = $1 AND s.workspace_id = $2 AND s.project_id = $3 AND s.section_id = $4 AND s.work_item_id = $5 AND s.process_id = $6";

#[derive(Debug)]
pub enum SessionError {
    NotAccessible,
    Forbidden,
    InvalidFields(serde_json::Value),
    /// Execution already terminal, or the session is already closed.
    StateConflict,
    /// The worker already has an open session somewhere in this tenant.
    ActiveSessionExists,
    DatabaseError,
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotAccessible => "resource not found",
            Self::Forbidden => "permission denied",
            Self::InvalidFields(_) => "invalid fields",
            Self::StateConflict => "state conflict",
            Self::ActiveSessionExists => "active session exists",
            Self::DatabaseError => "database operation failed",
        })
    }
}
impl std::error::Error for SessionError {}

fn database_error(error: sqlx::Error) -> SessionError {
    // Structural backstop: a raced second open session for the same worker
    // surfaces as the semantic conflict, never a raw 23505.
    if let sqlx::Error::Database(ref db) = error
        && db.constraint() == Some("process_execution_time_sessions_one_open_per_worker")
    {
        return SessionError::ActiveSessionExists;
    }
    SessionError::DatabaseError
}
fn api_error(error: SessionError, request_id: &RequestId) -> ApiError {
    match error {
        SessionError::NotAccessible => {
            ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone())
        }
        SessionError::Forbidden => rbac::permission_denied(request_id),
        SessionError::InvalidFields(fields) => {
            ApiError::invalid_fields(fields, request_id.0.clone())
        }
        SessionError::StateConflict => {
            ApiError::new(ErrorCode::StateConflict, request_id.0.clone())
        }
        SessionError::ActiveSessionExists => {
            ApiError::new(ErrorCode::ActiveSessionExists, request_id.0.clone())
        }
        SessionError::DatabaseError => {
            ApiError::new(ErrorCode::InternalError, request_id.0.clone())
        }
    }
}

/// V1 self-service start: the authenticated actor is the worker and the
/// starting actor. The body accepts nothing — unknown fields are rejected.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StartSessionRequest {}

#[derive(Serialize, ToSchema)]
pub struct SessionMutationResponse {
    pub data: TimeSessionPublic,
    /// Authoritative server timestamp for client clock-offset correction.
    pub server_time: String,
}
#[derive(Serialize, ToSchema)]
pub struct SessionListResponse {
    pub data: Vec<TimeSessionPublic>,
    pub server_time: String,
}

fn scope_lock_error(error: ExecutionError) -> SessionError {
    match error {
        ExecutionError::NotAccessible => SessionError::NotAccessible,
        ExecutionError::Forbidden => SessionError::Forbidden,
        _ => SessionError::DatabaseError,
    }
}

/// Shared write-path preamble: the full parent chain (project FOR UPDATE →
/// memberships FOR SHARE → work item FOR SHARE → permission FOR SHARE →
/// process FOR SHARE), then the permission check, then the execution row
/// FOR UPDATE. Everything the time-session commands need to re-prove inside
/// the transaction lives behind this lock sequence — the same order as
/// execution transitions, so session writes can never deadlock them.
async fn lock_execution(
    tx: &mut PgConnection,
    scope: ExecutionScope,
    execution_id: Uuid,
    actor: Uuid,
    permission: PermissionKey,
) -> Result<String, SessionError> {
    process_executions::lock_parent(tx, scope, actor)
        .await
        .map_err(scope_lock_error)?;
    process_executions::require_permission(tx, scope, actor, permission)
        .await
        .map_err(scope_lock_error)?;
    let sql = format!(
        "SELECT status FROM process_executions WHERE {} AND id = $7 FOR UPDATE",
        process_executions::SCOPE
    );
    let status = sqlx::query_scalar::<_, String>(&sql);
    let status = process_executions::bind_scope_scalar(status, scope)
        .bind(execution_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or(SessionError::NotAccessible)?;
    Ok(status)
}

/// START WORK: open a session for the authenticated worker on an ACTIVE
/// execution. Worker eligibility is the actor's own dual membership already
/// proven FOR SHARE by lock_parent; the tenant-wide open-session check plus
/// the partial unique index enforce one open session per worker.
pub async fn start_session(
    pool: &PgPool,
    scope: ExecutionScope,
    execution_id: Uuid,
    actor: Uuid,
) -> Result<(TimeSessionPublic, String), SessionError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    let status = lock_execution(&mut tx, scope, execution_id, actor, TIME_SESSIONS_START).await?;
    if status != "active" {
        return Err(SessionError::StateConflict);
    }
    let open: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM process_execution_time_sessions \
         WHERE tenant_id = $1 AND worker_user_id = $2 AND ended_at IS NULL",
    )
    .bind(scope.organization_id)
    .bind(actor)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?;
    if open.is_some() {
        return Err(SessionError::ActiveSessionExists);
    }
    let row = insert_session(&mut tx, scope, execution_id, actor).await?;
    let now = process_executions::server_time(&mut tx)
        .await
        .map_err(scope_lock_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok((row, now))
}

async fn insert_session(
    tx: &mut PgConnection,
    scope: ExecutionScope,
    execution_id: Uuid,
    actor: Uuid,
) -> Result<TimeSessionPublic, SessionError> {
    let sql = format!(
        "WITH inserted AS ( \
            INSERT INTO process_execution_time_sessions \
            (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, process_execution_id, worker_user_id, started_by_user_id) \
            VALUES ($7, $1, $2, $3, $4, $5, $6, $8, $9, $9) \
            RETURNING * \
        ) \
        SELECT {COLUMNS} FROM inserted s JOIN users u ON u.id = s.worker_user_id"
    );
    let row = process_executions::bind_scope(sqlx::query_as::<_, SessionRow>(&sql), scope)
        .bind(Uuid::now_v7())
        .bind(execution_id)
        .bind(actor)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    Ok(TimeSessionPublic::from(row))
}

/// STOP WORK: close the worker's own open session. `worker_user_id` is the
/// self-service owner — a second member cannot stop it even though both
/// hold time_sessions:stop. Terminal execution transitions close sessions
/// through a different path and are not subject to this rule.
pub async fn stop_session(
    pool: &PgPool,
    scope: ExecutionScope,
    execution_id: Uuid,
    session_id: Uuid,
    actor: Uuid,
) -> Result<(TimeSessionPublic, String), SessionError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    // The execution row is locked FOR UPDATE regardless of its status: it
    // serializes stop against complete/cancel without reopening anything.
    lock_execution(&mut tx, scope, execution_id, actor, TIME_SESSIONS_STOP).await?;
    let sql = format!(
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} \
         AND s.process_execution_id = $7 AND s.id = $8 FOR UPDATE OF s"
    );
    let session = process_executions::bind_scope(sqlx::query_as::<_, SessionRow>(&sql), scope)
        .bind(execution_id)
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or(SessionError::NotAccessible)?;
    if session.worker_id != actor {
        return Err(SessionError::Forbidden);
    }
    if session.ended_at.is_some() {
        return Err(SessionError::StateConflict);
    }
    let row = close_session(&mut tx, scope, execution_id, session_id, actor).await?;
    let now = process_executions::server_time(&mut tx)
        .await
        .map_err(scope_lock_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok((row, now))
}

async fn close_session(
    tx: &mut PgConnection,
    scope: ExecutionScope,
    execution_id: Uuid,
    session_id: Uuid,
    actor: Uuid,
) -> Result<TimeSessionPublic, SessionError> {
    // The WHERE clause keeps the write idempotent-safe: a raced second close
    // matches zero rows and surfaces as StateConflict, never a re-write of
    // ended_at/ended_by.
    let affected = sqlx::query(
        "UPDATE process_execution_time_sessions SET ended_at = now(), ended_by_user_id = $8 \
         WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4 \
           AND work_item_id = $5 AND process_id = $6 \
           AND process_execution_id = $7 AND id = $9 AND ended_at IS NULL",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(scope.work_item_id)
    .bind(scope.process_id)
    .bind(execution_id)
    .bind(actor)
    .bind(session_id)
    .execute(&mut *tx)
    .await
    .map_err(database_error)?
    .rows_affected();
    if affected == 0 {
        return Err(SessionError::StateConflict);
    }
    find_session_in_tx(tx, scope, execution_id, session_id).await
}

async fn find_session_in_tx(
    tx: &mut PgConnection,
    scope: ExecutionScope,
    execution_id: Uuid,
    session_id: Uuid,
) -> Result<TimeSessionPublic, SessionError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} \
         AND s.process_execution_id = $7 AND s.id = $8"
    );
    process_executions::bind_scope(sqlx::query_as::<_, SessionRow>(&sql), scope)
        .bind(execution_id)
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .map(TimeSessionPublic::from)
        .ok_or(SessionError::NotAccessible)
}

/// Nested read for the context extractor: a session resolves only through
/// its full chain plus its execution — never by bare id.
pub async fn find_accessible(
    pool: &PgPool,
    scope: ExecutionScope,
    execution_id: Uuid,
    session_id: Uuid,
) -> Result<Option<TimeSessionPublic>, SessionError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} \
         AND s.process_execution_id = $7 AND s.id = $8"
    );
    process_executions::bind_scope(sqlx::query_as::<_, SessionRow>(&sql), scope)
        .bind(execution_id)
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(database_error)
        .map(|row| row.map(TimeSessionPublic::from))
}

/// All intervals of one attempt, oldest first — history, not just live state.
pub async fn list_for_execution(
    pool: &PgPool,
    scope: ExecutionScope,
    execution_id: Uuid,
) -> Result<(Vec<TimeSessionPublic>, String), SessionError> {
    let mut conn = pool.acquire().await.map_err(database_error)?;
    let sql = format!(
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} AND s.process_execution_id = $7 \
         ORDER BY s.started_at, s.id"
    );
    let rows = process_executions::bind_scope(sqlx::query_as::<_, SessionRow>(&sql), scope)
        .bind(execution_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(database_error)?
        .into_iter()
        .map(TimeSessionPublic::from)
        .collect();
    let now = process_executions::server_time(&mut conn)
        .await
        .map_err(scope_lock_error)?;
    Ok((rows, now))
}

/// Only OPEN sessions of a work item — the "who is working right now" read
/// behind the process page's live worker chips. One set-wise query for the
/// whole page; never per-process lookups.
pub async fn list_open_for_work_item(
    pool: &PgPool,
    scope: WorkItemScope,
    work_item_id: Uuid,
) -> Result<(Vec<TimeSessionPublic>, String), SessionError> {
    let mut conn = pool.acquire().await.map_err(database_error)?;
    let rows = sqlx::query_as::<_, SessionRow>(&format!(
        "SELECT {COLUMNS} FROM {FROM} \
             WHERE s.tenant_id = $1 AND s.workspace_id = $2 AND s.project_id = $3 \
               AND s.section_id = $4 AND s.work_item_id = $5 AND s.ended_at IS NULL \
             ORDER BY s.started_at, s.id"
    ))
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(work_item_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(database_error)?
    .into_iter()
    .map(TimeSessionPublic::from)
    .collect();
    let now = process_executions::server_time(&mut conn)
        .await
        .map_err(scope_lock_error)?;
    Ok((rows, now))
}

/// ADR 0019: called INSIDE the execution terminal transaction (complete and
/// cancel share it). Every open session of the attempt closes at the same
/// transaction timestamp as the execution's terminal column, with
/// `ended_by` = the transition actor — not the worker.
pub(crate) async fn close_open_sessions_in_tx(
    tx: &mut PgConnection,
    scope: ExecutionScope,
    execution_id: Uuid,
    actor: Uuid,
) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "UPDATE process_execution_time_sessions SET ended_at = now(), ended_by_user_id = $8 \
         WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 AND section_id = $4 \
           AND work_item_id = $5 AND process_id = $6 \
           AND process_execution_id = $7 AND ended_at IS NULL",
    )
    .bind(scope.organization_id)
    .bind(scope.workspace_id)
    .bind(scope.project_id)
    .bind(scope.section_id)
    .bind(scope.work_item_id)
    .bind(scope.process_id)
    .bind(execution_id)
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map(|done| done.rows_affected())
}

fn actor_of(context: &ProcessExecutionContext) -> Uuid {
    context.parent.parent.parent.user_id
}

#[utoipa::path(post, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions/{execution_id}/time-sessions",
request_body = StartSessionRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path),("execution_id" = Uuid, Path)),
responses((status = 201, body = SessionMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 409, body = crate::error::ErrorEnvelope)))]
pub async fn start_session_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProcessExecutionContext,
    body: Result<ApiJson<StartSessionRequest>, ApiError>,
) -> Result<(StatusCode, Json<SessionMutationResponse>), ApiError> {
    // The body carries nothing, but it must parse and carry no unknown
    // fields: client-supplied worker ids or timestamps are a structured
    // 400, never silently ignored worker/actor spoofing.
    body?;
    let (data, server_time) = start_session(
        &state.pool,
        ExecutionScope::of(&context.parent),
        context.execution.id,
        actor_of(&context),
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok((
        StatusCode::CREATED,
        Json(SessionMutationResponse { data, server_time }),
    ))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions/{execution_id}/time-sessions",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path),("execution_id" = Uuid, Path)),
responses((status = 200, body = SessionListResponse),(status = 401, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope)))]
pub async fn list_execution_sessions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProcessExecutionContext,
) -> Result<Json<SessionListResponse>, ApiError> {
    let (data, server_time) = list_for_execution(
        &state.pool,
        ExecutionScope::of(&context.parent),
        context.execution.id,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(SessionListResponse { data, server_time }))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/time-sessions",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path)),
responses((status = 200, body = SessionListResponse),(status = 401, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope)))]
pub async fn list_work_item_open_sessions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkItemContext,
) -> Result<Json<SessionListResponse>, ApiError> {
    let (data, server_time) =
        list_open_for_work_item(&state.pool, context.parent.scope, context.work_item.id)
            .await
            .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(SessionListResponse { data, server_time }))
}

#[utoipa::path(post, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions/{execution_id}/time-sessions/{time_session_id}/stop",
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path),("execution_id" = Uuid, Path),("time_session_id" = Uuid, Path)),
responses((status = 200, body = SessionMutationResponse),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 409, body = crate::error::ErrorEnvelope)))]
pub async fn stop_session_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: TimeSessionContext,
) -> Result<Json<SessionMutationResponse>, ApiError> {
    let (data, server_time) = stop_session(
        &state.pool,
        ExecutionScope::of(&context.parent.parent),
        context.parent.execution.id,
        context.session.id,
        context.parent.parent.parent.parent.user_id,
    )
    .await
    .map_err(|e| api_error(e, &request_id))?;
    Ok(Json(SessionMutationResponse { data, server_time }))
}
