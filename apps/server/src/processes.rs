//! Ordered process DEFINITIONS inside a work item (ADR 0015).
//!
//! Definitions plus current responsibility (STEP 21C, ADR 0018):
//! `assignee_user_id` is mutable responsibility metadata — not authorization,
//! not history. Runtime execution (attempts, timers) lives in
//! `process_executions` (ADR 0016); progress is derived (ADR 0017).
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
pub const PROCESSES_ASSIGN: PermissionKey = PermissionKey("processes:assign");

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

/// Minimal public identity of a process assignee (STEP 21C, ADR 0018).
/// `eligible` reports whether the assignee still satisfies membership/user
/// activation rules — a stale assignee remains displayed for history but
/// must not be presented as an active member.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct AssigneePublic {
    pub id: Uuid,
    pub display_name: String,
    pub eligible: bool,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
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
    /// Current responsible user; `null` means unassigned. Assignment is
    /// responsibility metadata, never authorization (ADR 0018).
    pub assignee: Option<AssigneePublic>,
}

#[derive(sqlx::FromRow)]
struct ProcessRow {
    id: Uuid,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
    work_item_id: Uuid,
    name: String,
    slug: String,
    description: Option<String>,
    position: i32,
    is_required: bool,
    status: String,
    assignee_user_id: Option<Uuid>,
    assignee_display_name: Option<String>,
    assignee_eligible: Option<bool>,
}
impl From<ProcessRow> for ProcessPublic {
    fn from(row: ProcessRow) -> Self {
        Self {
            id: row.id,
            organization_id: row.organization_id,
            workspace_id: row.workspace_id,
            project_id: row.project_id,
            section_id: row.section_id,
            work_item_id: row.work_item_id,
            name: row.name,
            slug: row.slug,
            description: row.description,
            position: row.position,
            is_required: row.is_required,
            status: row.status,
            assignee: row.assignee_user_id.map(|id| AssigneePublic {
                id,
                display_name: row.assignee_display_name.unwrap_or_default(),
                eligible: row.assignee_eligible.unwrap_or(false),
            }),
        }
    }
}

// Eligibility is derived set-based in one statement: active user + active
// organization membership + active workspace membership. The join stays
// valid for a stale assignee because the composite FK references the
// membership ROW, which survives revocation (soft lifecycle columns only).
const COLUMNS: &str = "p.id, p.tenant_id AS organization_id, p.workspace_id, p.project_id, p.section_id, p.work_item_id, p.name, p.slug::text AS slug, p.description, p.position, p.is_required, p.status, p.assignee_user_id, \
    u.display_name AS assignee_display_name, \
    (u.status = 'active' AND wm.id IS NOT NULL AND om.id IS NOT NULL) AS assignee_eligible";
const FROM: &str = "processes p \
    LEFT JOIN users u ON u.id = p.assignee_user_id \
    LEFT JOIN workspace_memberships wm ON wm.workspace_id = p.workspace_id \
        AND wm.user_id = p.assignee_user_id AND wm.status = 'active' AND wm.deleted_at IS NULL \
    LEFT JOIN organization_memberships om ON om.tenant_id = p.tenant_id \
        AND om.user_id = p.assignee_user_id AND om.status = 'active' AND om.deleted_at IS NULL";
const SCOPE: &str = "p.tenant_id = $1 AND p.workspace_id = $2 AND p.project_id = $3 AND p.section_id = $4 AND p.work_item_id = $5";
const VISIBLE: &str = "p.deleted_at IS NULL AND p.status = 'active'";

#[derive(Debug)]
pub enum ProcessError {
    NotAccessible,
    Forbidden,
    InvalidFields(serde_json::Value),
    /// Archiving is blocked while an active execution exists (ADR 0016).
    StateConflict,
    DatabaseError,
}
impl std::fmt::Display for ProcessError {
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
        ProcessError::StateConflict => {
            ApiError::new(ErrorCode::StateConflict, request_id.0.clone())
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
pub struct UpdateAssignmentRequest {
    /// A workspace-member user id to assign, or `null` to unassign. The key
    /// itself is required — an omitted `user_id` is a malformed request
    /// (400), never a silent unassign.
    #[schema(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub user_id: Option<Uuid>,
}
fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}
#[cfg(test)]
mod tests {
    use super::UpdateAssignmentRequest;
    use uuid::Uuid;

    #[test]
    fn assignment_body_requires_the_user_id_key() {
        assert!(
            serde_json::from_str::<UpdateAssignmentRequest>("{}").is_err(),
            "missing user_id must be malformed, not an implicit unassign"
        );
        let nulled = serde_json::from_str::<UpdateAssignmentRequest>(r#"{"user_id":null}"#)
            .unwrap_or_else(|error| panic!("explicit null must parse: {error}"));
        assert_eq!(nulled.user_id, None, "explicit null unassigns");
        let id = Uuid::now_v7();
        let parsed =
            serde_json::from_str::<UpdateAssignmentRequest>(&format!(r#"{{"user_id":"{id}"}}"#))
                .unwrap_or_else(|error| panic!("uuid user_id must parse: {error}"));
        assert_eq!(parsed.user_id, Some(id));
    }
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
    let sql = format!("SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} AND p.id = $6 AND {VISIBLE}");
    bind_scope(sqlx::query_as::<_, ProcessRow>(&sql), scope)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(database_error)
        .map(|row| row.map(ProcessPublic::from))
}
pub async fn visible_for_work_item(
    pool: &PgPool,
    scope: ProcessScope,
) -> Result<Vec<ProcessPublic>, ProcessError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} AND {VISIBLE} ORDER BY p.position, p.id"
    );
    bind_scope(sqlx::query_as::<_, ProcessRow>(&sql), scope)
        .fetch_all(pool)
        .await
        .map_err(database_error)
        .map(|rows| rows.into_iter().map(ProcessPublic::from).collect())
}

/// Re-read one process inside a mutation transaction so `RETURNING`-free
/// writes can still emit the joined assignee columns. `visible` toggles the
/// active-only predicate — an archive PATCH legitimately produces an
/// archived row that must still be returned.
async fn fetch_in_tx(
    tx: &mut PgConnection,
    scope: ProcessScope,
    id: Uuid,
    visible_only: bool,
) -> Result<ProcessPublic, ProcessError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} AND p.id = $6 AND p.deleted_at IS NULL {}",
        if visible_only {
            "AND p.status = 'active'"
        } else {
            ""
        }
    );
    bind_scope(sqlx::query_as::<_, ProcessRow>(&sql), scope)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .map(ProcessPublic::from)
        .ok_or(ProcessError::NotAccessible)
}

/// Lock order (ADR 0015) extends the work item order without reordering it:
/// project FOR UPDATE → direct section + org/workspace + both memberships
/// FOR SHARE → work item FOR SHARE → permission support rows FOR SHARE →
/// process rows FOR UPDATE. The project lock serializes all writes in the
/// project, so appends and reorders cannot interleave.
pub(crate) async fn lock_parent(
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
        format!("SELECT COALESCE(max(p.position)::bigint, -1) + 1 FROM processes p WHERE {SCOPE}");
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
    let sql = "INSERT INTO processes (id, tenant_id, workspace_id, project_id, section_id, work_item_id, name, slug, description, position, is_required) \
         VALUES ($6, $1, $2, $3, $4, $5, $7, $8, $9, $10, $11) RETURNING id";
    let id: Uuid = sqlx::query_scalar(sql)
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
        .bind(Uuid::now_v7())
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(position)
        .bind(input.is_required.unwrap_or(true))
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let row = fetch_in_tx(&mut tx, scope, id, true).await?;
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
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} AND p.id = $6 AND {VISIBLE} FOR UPDATE OF p"
    );
    let current: ProcessPublic = bind_scope(sqlx::query_as::<_, ProcessRow>(&sql), scope)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .map(ProcessPublic::from)
        .ok_or(ProcessError::NotAccessible)?;
    require_permission(&mut tx, scope, actor, PROCESSES_UPDATE).await?;
    let status = input
        .status
        .as_deref()
        .map(str::trim)
        .unwrap_or(&current.status);
    if status == "archived" {
        require_permission(&mut tx, scope, actor, PROCESSES_ARCHIVE).await?;
        // ADR 0016: a process with an ACTIVE execution cannot be archived;
        // the guard runs under the same project serialization as START.
        if current.status != "archived"
            && crate::process_executions::active_execution_exists_for_process(&mut tx, scope, id)
                .await
                .map_err(database_error)?
        {
            return Err(ProcessError::StateConflict);
        }
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
        "UPDATE processes p SET name = $7, slug = $8, description = $9, is_required = $10, status = $11, updated_at = now() \
         WHERE {SCOPE} AND p.id = $6 AND {VISIBLE}"
    );
    sqlx::query(&sql)
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(is_required)
        .bind(status)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    let row = fetch_in_tx(&mut tx, scope, id, false).await?;
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
        "SELECT p.id, p.position FROM processes p WHERE {SCOPE} AND {VISIBLE} ORDER BY p.position, p.id FOR UPDATE"
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
        "SELECT {COLUMNS} FROM {FROM} WHERE {SCOPE} AND {VISIBLE} ORDER BY p.position, p.id"
    );
    let rows: Vec<ProcessPublic> = bind_scope(sqlx::query_as::<_, ProcessRow>(&sql), scope)
        .fetch_all(&mut *tx)
        .await
        .map_err(database_error)?
        .into_iter()
        .map(ProcessPublic::from)
        .collect();
    tx.commit().await.map_err(database_error)?;
    Ok(rows)
}

/// ASSIGN (STEP 21C, ADR 0018): set or clear the responsible user of a
/// process. `user_id: null` unassigns; a uuid must currently be eligible
/// (active user + active organization membership + active workspace
/// membership). Eligibility is re-proven inside the transaction and the
/// membership row is locked FOR SHARE so a concurrent revocation serializes
/// instead of slipping a stale assignee past the check. Ineligible ids share
/// one generic 422 — never a cross-tenant existence oracle.
pub async fn assign_process(
    pool: &PgPool,
    scope: ProcessScope,
    id: Uuid,
    actor: Uuid,
    input: &UpdateAssignmentRequest,
) -> Result<ProcessPublic, ProcessError> {
    let mut tx = pool.begin().await.map_err(database_error)?;
    lock_parent(&mut tx, scope, actor).await?;
    require_permission(&mut tx, scope, actor, PROCESSES_ASSIGN).await?;
    let sql = format!(
        "SELECT p.id FROM processes p WHERE {SCOPE} AND p.id = $6 AND {VISIBLE} FOR UPDATE"
    );
    bind_scope(sqlx::query_as::<_, (Uuid,)>(&sql), scope)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or(ProcessError::NotAccessible)?;
    if let Some(assignee) = input.user_id {
        let eligible = sqlx::query_scalar::<_, Uuid>(
            "SELECT wm.id FROM workspace_memberships wm \
             JOIN organization_memberships om ON om.tenant_id = wm.tenant_id \
                 AND om.user_id = wm.user_id AND om.status = 'active' AND om.deleted_at IS NULL \
             JOIN users u ON u.id = wm.user_id AND u.status = 'active' \
             WHERE wm.tenant_id = $1 AND wm.workspace_id = $2 AND wm.user_id = $3 \
               AND wm.status = 'active' AND wm.deleted_at IS NULL \
             FOR SHARE OF wm, om, u",
        )
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(assignee)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?;
        if eligible.is_none() {
            return Err(invalid("user_id"));
        }
    }
    let sql = format!(
        "UPDATE processes p SET assignee_user_id = $6, updated_at = now() \
         WHERE {SCOPE} AND p.id = $7 AND {VISIBLE}"
    );
    sqlx::query(&sql)
        .bind(scope.organization_id)
        .bind(scope.workspace_id)
        .bind(scope.project_id)
        .bind(scope.section_id)
        .bind(scope.work_item_id)
        .bind(input.user_id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    let row = fetch_in_tx(&mut tx, scope, id, true).await?;
    tx.commit().await.map_err(database_error)?;
    Ok(row)
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

#[utoipa::path(put, path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/assignment",
request_body = UpdateAssignmentRequest,
params(("organization_id" = Uuid, Path),("workspace_id" = Uuid, Path),("project_id" = Uuid, Path),("section_id" = Uuid, Path),("work_item_id" = Uuid, Path),("process_id" = Uuid, Path)),
responses((status = 200, body = ProcessMutationResponse),(status = 400, body = crate::error::ErrorEnvelope),(status = 401, body = crate::error::ErrorEnvelope),(status = 403, body = crate::error::ErrorEnvelope),(status = 404, body = crate::error::ErrorEnvelope),(status = 422, body = crate::error::ErrorEnvelope)))]
pub async fn assign_process_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProcessContext,
    body: Result<ApiJson<UpdateAssignmentRequest>, ApiError>,
) -> Result<Json<ProcessMutationResponse>, ApiError> {
    check_permission(&state, &context.parent, PROCESSES_ASSIGN, &request_id).await?;
    let ApiJson(body) = body?;
    let data = assign_process(
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
