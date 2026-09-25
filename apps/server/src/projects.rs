//! Project domain foundation (First Agent Mission step 17, ADR 0011).
//!
//! Projects are the first product-domain aggregate and live strictly below
//! the generic SaaS foundation: a project belongs to exactly one workspace
//! of exactly one organization. Route parents are authoritative — a project
//! id alone never resolves anything (ADR 0011). Request-context eligibility
//! (WorkspaceContext/ProjectContext) is separate from transaction-time
//! re-validation: every mutation re-proves memberships and permission inside
//! its transaction (TOCTOU).

use crate::{
    AppState, RequestId,
    auth::ApiJson,
    context::WorkspaceContext,
    error::{ApiError, ErrorCode},
    organizations::{slug_from_text, validate_slug},
    workspaces,
};
use axum::{
    Extension, Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

/// Create projects inside a workspace.
pub const PROJECTS_CREATE: crate::rbac::PermissionKey =
    crate::rbac::PermissionKey("projects:create");
/// Update project content and non-archive lifecycle transitions.
pub const PROJECTS_UPDATE: crate::rbac::PermissionKey =
    crate::rbac::PermissionKey("projects:update");
/// Transition a project INTO the archived status (update alone must not be
/// able to archive indirectly).
pub const PROJECTS_ARCHIVE: crate::rbac::PermissionKey =
    crate::rbac::PermissionKey("projects:archive");

pub const PROJECT_STATUS_ACTIVE: &str = "active";
pub const PROJECT_STATUS_COMPLETED: &str = "completed";
pub const PROJECT_STATUS_ARCHIVED: &str = "archived";

const NAME_MAX_CHARS: usize = 200;
const DESCRIPTION_MAX_CHARS: usize = 10_000;

const PROJECT_COLUMNS: &str = "p.id, p.tenant_id AS organization_id, p.workspace_id, \
     p.name, p.slug::text AS slug, p.description, p.status";

/// Canonical V1 status lifecycle (ADR 0011). Identity (status unchanged) is
/// allowed; `archived -> completed` is deliberately NOT reachable — an
/// archived project must be returned to active first.
pub fn transition_allowed(from: &str, to: &str) -> bool {
    from == to
        || matches!(
            (from, to),
            ("active", "completed")
                | ("active", "archived")
                | ("completed", "active")
                | ("completed", "archived")
                | ("archived", "active")
        )
}

#[derive(Debug)]
pub enum ProjectError {
    SlugAlreadyTaken,
    NotAccessible,
    Forbidden,
    InvalidTransition,
    /// Archiving is blocked while an active process execution exists
    /// anywhere in the project (ADR 0016).
    StateConflict,
    DatabaseError,
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            ProjectError::SlugAlreadyTaken => "slug already exists",
            ProjectError::NotAccessible => "workspace or project is not accessible",
            ProjectError::Forbidden => "permission denied",
            ProjectError::InvalidTransition => "status transition is not allowed",
            ProjectError::StateConflict => "state conflict",
            ProjectError::DatabaseError => "database operation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProjectError {}

#[derive(Clone, sqlx::FromRow)]
pub struct ProjectRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub status: String,
}

pub struct NewProject {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
}

/// Resolved field values for a partial update; `None` keeps the stored value.
#[derive(Default)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

async fn insert_project(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    project: &NewProject,
) -> Result<ProjectRow, ProjectError> {
    let row = sqlx::query_as::<_, ProjectRow>(
        "INSERT INTO projects (id, tenant_id, workspace_id, name, slug, description) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, tenant_id AS organization_id, workspace_id, \
                   name, slug::text AS slug, description, status",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(workspace_id)
    .bind(&project.name)
    .bind(&project.slug)
    .bind(&project.description)
    .fetch_one(connection)
    .await;
    match row {
        Ok(row) => Ok(row),
        // 23505 = unique_violation on projects_workspace_slug_key.
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            Err(ProjectError::SlugAlreadyTaken)
        }
        Err(_) => Err(ProjectError::DatabaseError),
    }
}

/// Single authoritative scoped lookup: id + resolved workspace + resolved
/// tenant + not soft-deleted in ONE query (ADR 0011). Callers must already
/// hold a WorkspaceContext-equivalent resolution for the parent pair; a miss
/// is indistinguishable from unknown/foreign/deleted.
pub async fn find_accessible(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
) -> Result<Option<ProjectRow>, ProjectError> {
    let sql = format!(
        "SELECT {PROJECT_COLUMNS} FROM projects p \
         WHERE p.id = $1 AND p.workspace_id = $2 AND p.tenant_id = $3 AND p.deleted_at IS NULL"
    );
    sqlx::query_as::<_, ProjectRow>(&sql)
        .bind(project_id)
        .bind(workspace_id)
        .bind(organization_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ProjectError::DatabaseError)
}

/// Projects of one resolved workspace: deleted excluded, active/completed/
/// archived included (the UI filters by status), newest first with id as the
/// deterministic tie-breaker (ADR 0011).
pub async fn visible_for_workspace(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
) -> Result<Vec<ProjectRow>, ProjectError> {
    let sql = format!(
        "SELECT {PROJECT_COLUMNS} FROM projects p \
         WHERE p.workspace_id = $1 AND p.tenant_id = $2 AND p.deleted_at IS NULL \
         ORDER BY p.created_at DESC, p.id"
    );
    sqlx::query_as::<_, ProjectRow>(&sql)
        .bind(workspace_id)
        .bind(organization_id)
        .fetch_all(pool)
        .await
        .map_err(|_| ProjectError::DatabaseError)
}

/// Row-level-locked scoped lookup for mutations: locking prevents concurrent
/// PATCHes from overwriting each other mid-transaction while the scope filter
/// keeps foreign/deleted rows unreachable.
async fn lock_scoped(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
) -> Result<Option<ProjectRow>, ProjectError> {
    let sql = format!(
        "SELECT {PROJECT_COLUMNS} FROM projects p \
         WHERE p.id = $1 AND p.workspace_id = $2 AND p.tenant_id = $3 AND p.deleted_at IS NULL \
         FOR UPDATE"
    );
    sqlx::query_as::<_, ProjectRow>(&sql)
        .bind(project_id)
        .bind(workspace_id)
        .bind(tenant_id)
        .fetch_optional(connection)
        .await
        .map_err(|_| ProjectError::DatabaseError)
}

/// Project creation runs inside one transaction that re-proves every security
/// fact on the transaction's snapshot (ADR 0011): active organization and
/// workspace memberships, parent authority (workspace belongs to the tenant
/// and is not deleted) and the projects:create permission. A racing revoke
/// cannot slip between handler checks and commit.
pub async fn create_project(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project: &NewProject,
    creator: Uuid,
) -> Result<ProjectRow, ProjectError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ProjectError::DatabaseError)?;
    // Parent authority + both memberships + non-deleted rows, re-checked on
    // the transaction snapshot (workspaces::find_accessible / ACCESS_FILTER).
    let workspace =
        workspaces::find_accessible(&mut *transaction, organization_id, workspace_id, creator)
            .await
            .map_err(|_| ProjectError::DatabaseError)?;
    if workspace.is_none() {
        return Err(ProjectError::NotAccessible);
    }
    // Permission revalidated INSIDE the transaction (workspace scope: an
    // organization-wide grant also applies, a foreign workspace grant does not).
    let authorized = crate::rbac::authorize_workspace_in_tx(
        &mut transaction,
        organization_id,
        workspace_id,
        creator,
        PROJECTS_CREATE,
    )
    .await
    .map_err(|_| ProjectError::DatabaseError)?;
    if !authorized {
        return Err(ProjectError::Forbidden);
    }
    let created = insert_project(&mut transaction, organization_id, workspace_id, project).await?;
    transaction
        .commit()
        .await
        .map_err(|_| ProjectError::DatabaseError)?;
    Ok(created)
}

/// Project update runs inside one transaction with the same re-validation as
/// creation, plus a row lock and domain transition validation. Setting
/// `status = archived` additionally requires projects:archive; transitions
/// INTO archived are the only place it is demanded (ADR 0011).
pub async fn update_project(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    patch: &ProjectPatch,
    actor: Uuid,
) -> Result<ProjectRow, ProjectError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ProjectError::DatabaseError)?;
    let workspace =
        workspaces::find_accessible(&mut *transaction, organization_id, workspace_id, actor)
            .await
            .map_err(|_| ProjectError::DatabaseError)?;
    if workspace.is_none() {
        return Err(ProjectError::NotAccessible);
    }
    let authorized = crate::rbac::authorize_workspace_in_tx(
        &mut transaction,
        organization_id,
        workspace_id,
        actor,
        PROJECTS_UPDATE,
    )
    .await
    .map_err(|_| ProjectError::DatabaseError)?;
    if !authorized {
        return Err(ProjectError::Forbidden);
    }
    // Only requests that SET status to archived demand projects:archive, so
    // the permission has a real, reachable enforcement point.
    if patch.status.as_deref() == Some(PROJECT_STATUS_ARCHIVED) {
        let may_archive = crate::rbac::authorize_workspace_in_tx(
            &mut transaction,
            organization_id,
            workspace_id,
            actor,
            PROJECTS_ARCHIVE,
        )
        .await
        .map_err(|_| ProjectError::DatabaseError)?;
        if !may_archive {
            return Err(ProjectError::Forbidden);
        }
    }
    let current = lock_scoped(&mut transaction, organization_id, workspace_id, project_id)
        .await?
        .ok_or(ProjectError::NotAccessible)?;
    // Resolve the final field values, then validate the resulting transition.
    let name = patch.name.clone().unwrap_or(current.name);
    let slug = patch.slug.clone().unwrap_or(current.slug);
    // Explicit empty description clears the field; omitting it preserves.
    let description = match &patch.description {
        Some(text) if text.is_empty() => None,
        Some(text) => Some(text.clone()),
        None => current.description,
    };
    let status = patch
        .status
        .clone()
        .unwrap_or_else(|| current.status.clone());
    if !transition_allowed(&current.status, &status) {
        return Err(ProjectError::InvalidTransition);
    }
    // ADR 0016: a project carrying ANY active execution cannot be archived.
    // The project row lock (lock_scoped FOR UPDATE) is the same serialization
    // START takes via work_items::lock_parent, so the check cannot race.
    if status == PROJECT_STATUS_ARCHIVED
        && current.status != PROJECT_STATUS_ARCHIVED
        && crate::process_executions::active_execution_exists_for_project(
            &mut transaction,
            organization_id,
            workspace_id,
            project_id,
        )
        .await
        .map_err(|_| ProjectError::DatabaseError)?
    {
        return Err(ProjectError::StateConflict);
    }
    let row = sqlx::query_as::<_, ProjectRow>(
        "UPDATE projects SET name = $4, slug = $5, description = $6, status = $7, updated_at = now() \
         WHERE id = $1 AND workspace_id = $2 AND tenant_id = $3 AND deleted_at IS NULL \
         RETURNING id, tenant_id AS organization_id, workspace_id, \
                   name, slug::text AS slug, description, status",
    )
    .bind(project_id)
    .bind(workspace_id)
    .bind(organization_id)
    .bind(&name)
    .bind(&slug)
    .bind(&description)
    .bind(&status)
    .fetch_one(&mut *transaction)
    .await;
    match row {
        Ok(row) => {
            transaction
                .commit()
                .await
                .map_err(|_| ProjectError::DatabaseError)?;
            Ok(row)
        }
        // 23505 = unique_violation on projects_workspace_slug_key.
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            Err(ProjectError::SlugAlreadyTaken)
        }
        Err(_) => Err(ProjectError::DatabaseError),
    }
}

// ---------------------------------------------------------------------------
// HTTP contracts
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
}

/// Ownership (tenant/workspace) and lifecycle authority never enter the body:
/// scope comes from the authenticated session plus the route, and new
/// projects always start `active` (ADR 0011).
#[derive(Deserialize, ToSchema)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    /// Empty string clears the stored description.
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectPublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub status: String,
}

impl From<ProjectRow> for ProjectPublic {
    fn from(row: ProjectRow) -> Self {
        Self {
            id: row.id,
            organization_id: row.organization_id,
            workspace_id: row.workspace_id,
            name: row.name,
            slug: row.slug,
            description: row.description,
            status: row.status,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ProjectMutationResponse {
    pub data: ProjectPublic,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectListResponse {
    pub data: Vec<ProjectPublic>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects",
    request_body = CreateProjectRequest,
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
    ),
    responses(
        (status = 201, body = ProjectMutationResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 403, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn create_project_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkspaceContext,
    ApiJson(body): ApiJson<CreateProjectRequest>,
) -> Result<Response, ApiError> {
    let not_found = || ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone());
    let organization_id = context.organization_id;
    let workspace_id = context.workspace_id;
    let name = body.name.trim().to_owned();
    let mut fields = serde_json::Map::new();
    if name.is_empty() {
        fields.insert("name".into(), serde_json::json!(["Required"]));
    } else if name.chars().count() > NAME_MAX_CHARS {
        fields.insert("name".into(), serde_json::json!(["Too long"]));
    }
    let description = body
        .description
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned);
    if description
        .as_deref()
        .is_some_and(|text| text.chars().count() > DESCRIPTION_MAX_CHARS)
    {
        fields.insert("description".into(), serde_json::json!(["Too long"]));
    }
    let explicit_slug = body
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|slug| !slug.is_empty());
    let slug = slug_from_text(explicit_slug.unwrap_or(&name));
    let slug_valid = slug.as_deref().is_some_and(validate_slug);
    if !slug_valid {
        if explicit_slug.is_some() {
            fields.insert("slug".into(), serde_json::json!(["Invalid format"]));
        } else {
            fields.insert("name".into(), serde_json::json!(["Cannot derive a slug"]));
        }
    }
    let slug = match (fields.is_empty(), slug) {
        (true, Some(slug)) => slug,
        (false, _) => {
            return Err(ApiError::invalid_fields(
                serde_json::Value::Object(fields),
                request_id.0,
            ));
        }
        _ => return Err(ApiError::new(ErrorCode::InternalError, request_id.0)),
    };

    // Eligibility is proven by the context; permission is checked here and
    // RE-CHECKED inside the mutation transaction (TOCTOU, ADR 0011).
    let allowed = crate::rbac::authorize_workspace(
        &state.pool,
        organization_id,
        workspace_id,
        context.user_id,
        PROJECTS_CREATE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(crate::rbac::permission_denied(&request_id));
    }
    let result = create_project(
        &state.pool,
        organization_id,
        workspace_id,
        &NewProject {
            name,
            slug,
            description,
        },
        context.user_id,
    )
    .await;
    match result {
        Ok(project) => Ok((
            StatusCode::CREATED,
            Json(ProjectMutationResponse {
                data: ProjectPublic::from(project),
            }),
        )
            .into_response()),
        // Parent pair inaccessible (or nonexistent): uniform 404.
        Err(ProjectError::NotAccessible) => Err(not_found()),
        // Permission revoked between the handler check and the transaction.
        Err(ProjectError::Forbidden) => Err(crate::rbac::permission_denied(&request_id)),
        Err(ProjectError::SlugAlreadyTaken) => Err(ApiError::invalid_fields(
            serde_json::json!({ "slug": ["Already taken"] }),
            request_id.0,
        )),
        Err(ProjectError::InvalidTransition) => Err(ApiError::invalid_fields(
            serde_json::json!({ "status": ["Invalid transition"] }),
            request_id.0,
        )),
        Err(ProjectError::StateConflict) => {
            Err(ApiError::new(ErrorCode::StateConflict, request_id.0))
        }
        Err(ProjectError::DatabaseError) => {
            Err(ApiError::new(ErrorCode::InternalError, request_id.0))
        }
    }
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects",
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
    ),
    responses(
        (status = 200, body = ProjectListResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn list_projects(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: WorkspaceContext,
) -> Result<Json<ProjectListResponse>, ApiError> {
    // Reads are eligibility-based (no projects:read in V1, ADR 0011): the
    // WorkspaceContext already proved both memberships and parent authority,
    // and the query is scoped to the resolved workspace only.
    let projects =
        visible_for_workspace(&state.pool, context.organization_id, context.workspace_id)
            .await
            .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(ProjectListResponse {
        data: projects.into_iter().map(ProjectPublic::from).collect(),
    }))
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}",
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
        ("project_id" = Uuid, Path, description = "Project id"),
    ),
    responses(
        (status = 200, body = ProjectPublic),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn get_project(
    context: crate::context::ProjectContext,
) -> Result<Json<ProjectPublic>, ApiError> {
    // The full resolution chain (both memberships, parent authority, project
    // scoped to the resolved workspace/tenant, not deleted) happened in the
    // ProjectContext extractor; misses are uniform 404s.
    Ok(Json(ProjectPublic::from(context.project)))
}

#[utoipa::path(patch,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}",
    request_body = UpdateProjectRequest,
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
        ("project_id" = Uuid, Path, description = "Project id"),
    ),
    responses(
        (status = 200, body = ProjectMutationResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 403, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn update_project_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: crate::context::ProjectContext,
    ApiJson(body): ApiJson<UpdateProjectRequest>,
) -> Result<Response, ApiError> {
    let not_found = || ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone());
    let mut fields = serde_json::Map::new();
    let name = body.name.as_deref().map(str::trim);
    match name {
        Some("") => fields.insert("name".into(), serde_json::json!(["Required"])),
        Some(text) if text.chars().count() > NAME_MAX_CHARS => {
            fields.insert("name".into(), serde_json::json!(["Too long"]))
        }
        _ => None,
    };
    let description = body
        .description
        .as_deref()
        .map(str::trim)
        .map(str::to_owned);
    if description
        .as_deref()
        .is_some_and(|text| !text.is_empty() && text.chars().count() > DESCRIPTION_MAX_CHARS)
    {
        fields.insert("description".into(), serde_json::json!(["Too long"]));
    }
    // Explicit slugs are normalized from free text and validated; the stored
    // slug survives untouched when the field is omitted.
    let slug = body
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(slug_from_text);
    match &slug {
        Some(None) => fields.insert("slug".into(), serde_json::json!(["Invalid format"])),
        Some(Some(value)) if !validate_slug(value) => {
            fields.insert("slug".into(), serde_json::json!(["Invalid format"]))
        }
        _ => None,
    };
    let status = body
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if status.is_some_and(|value| {
        value != PROJECT_STATUS_ACTIVE
            && value != PROJECT_STATUS_COMPLETED
            && value != PROJECT_STATUS_ARCHIVED
    }) {
        fields.insert("status".into(), serde_json::json!(["Invalid value"]));
    }
    if !fields.is_empty() {
        return Err(ApiError::invalid_fields(
            serde_json::Value::Object(fields),
            request_id.0,
        ));
    }
    let patch = ProjectPatch {
        name: name.filter(|text| !text.is_empty()).map(str::to_owned),
        slug: slug.flatten(),
        description,
        status: status.map(str::to_owned),
    };

    // Handler-level fast check; the transaction re-proves everything anyway.
    let allowed = crate::rbac::authorize_workspace(
        &state.pool,
        context.organization_id,
        context.workspace_id,
        context.user_id,
        PROJECTS_UPDATE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(crate::rbac::permission_denied(&request_id));
    }
    let result = update_project(
        &state.pool,
        context.organization_id,
        context.workspace_id,
        context.project_id,
        &patch,
        context.user_id,
    )
    .await;
    match result {
        Ok(project) => Ok((
            StatusCode::OK,
            Json(ProjectMutationResponse {
                data: ProjectPublic::from(project),
            }),
        )
            .into_response()),
        Err(ProjectError::NotAccessible) => Err(not_found()),
        Err(ProjectError::Forbidden) => Err(crate::rbac::permission_denied(&request_id)),
        Err(ProjectError::SlugAlreadyTaken) => Err(ApiError::invalid_fields(
            serde_json::json!({ "slug": ["Already taken"] }),
            request_id.0,
        )),
        Err(ProjectError::InvalidTransition) => Err(ApiError::invalid_fields(
            serde_json::json!({ "status": ["Invalid transition"] }),
            request_id.0,
        )),
        Err(ProjectError::StateConflict) => {
            Err(ApiError::new(ErrorCode::StateConflict, request_id.0))
        }
        Err(ProjectError::DatabaseError) => {
            Err(ApiError::new(ErrorCode::InternalError, request_id.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_keys_are_stable_machine_identifiers() {
        assert_eq!(PROJECTS_CREATE.0, "projects:create");
        assert_eq!(PROJECTS_UPDATE.0, "projects:update");
        assert_eq!(PROJECTS_ARCHIVE.0, "projects:archive");
    }

    #[test]
    fn canonical_v1_status_transitions() {
        assert!(transition_allowed("active", "completed"));
        assert!(transition_allowed("active", "archived"));
        assert!(transition_allowed("completed", "active"));
        assert!(transition_allowed("completed", "archived"));
        assert!(transition_allowed("archived", "active"));
        // Identity keeps a PATCH idempotent.
        assert!(transition_allowed("active", "active"));
        assert!(transition_allowed("archived", "archived"));
        // archived -> completed must return to active first (ADR 0011).
        assert!(!transition_allowed("archived", "completed"));
        assert!(transition_allowed("completed", "completed"));
        assert!(!transition_allowed("active", "unknown"));
    }
}
