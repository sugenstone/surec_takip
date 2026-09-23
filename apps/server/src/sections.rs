//! Sections / recursive hierarchy (First Agent Mission step 18, ADR 0012).
//!
//! ONE generic recursive Section entity: "Block", "Floor", "Apartment" are
//! user-chosen names, never database types. Adjacency-list parenting with
//! composite-FK integrity: a stored parent always carries the same
//! tenant/workspace/project triple as its child, so cross-project parenting
//! is impossible at the storage layer. Mutations serialize on the project
//! row lock inside one transaction that re-proves eligibility, permission,
//! parent validity and the acyclicity invariant (TOCTOU, ADR 0012).

use crate::{
    AppState, RequestId,
    auth::ApiJson,
    context::ProjectContext,
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

/// Create sections inside a project.
pub const SECTIONS_CREATE: crate::rbac::PermissionKey =
    crate::rbac::PermissionKey("sections:create");
/// Update, reorder and reparent sections (moves deliberately share this key:
/// one enforcement path, no separate V1 move permission).
pub const SECTIONS_UPDATE: crate::rbac::PermissionKey =
    crate::rbac::PermissionKey("sections:update");
/// Transition a section INTO the archived status only.
pub const SECTIONS_ARCHIVE: crate::rbac::PermissionKey =
    crate::rbac::PermissionKey("sections:archive");

pub const SECTION_STATUS_ACTIVE: &str = "active";
pub const SECTION_STATUS_ARCHIVED: &str = "archived";

const NAME_MAX_CHARS: usize = 200;

const SECTION_COLUMNS: &str = "s.id, s.tenant_id AS organization_id, s.workspace_id, \
     s.project_id, s.parent_section_id, s.name, s.slug::text AS slug, s.position, s.status";

/// Structural lifecycle: sections never "complete" — identity plus
/// active <-> archived in both directions (ADR 0012).
pub fn transition_allowed(from: &str, to: &str) -> bool {
    from == to || matches!((from, to), ("active", "archived") | ("archived", "active"))
}

#[derive(Debug)]
pub enum SectionError {
    SlugAlreadyTaken,
    NotAccessible,
    Forbidden,
    InvalidTransition,
    /// Parent is unknown in this project, wrong-scoped, archived, or the
    /// section itself (self-parenting is invalid).
    InvalidParent,
    CycleDetected,
    DatabaseError,
}

impl std::fmt::Display for SectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            SectionError::SlugAlreadyTaken => "slug already exists among siblings",
            SectionError::NotAccessible => "project or section is not accessible",
            SectionError::Forbidden => "permission denied",
            SectionError::InvalidTransition => "status transition is not allowed",
            SectionError::InvalidParent => "parent section is invalid",
            SectionError::CycleDetected => "reparenting would create a cycle",
            SectionError::DatabaseError => "database operation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SectionError {}

#[derive(Clone, sqlx::FromRow)]
pub struct SectionRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub parent_section_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub position: i32,
    pub status: String,
}

pub struct NewSection {
    pub name: String,
    pub slug: String,
    pub parent_section_id: Option<Uuid>,
}

/// Reparent intent for updates: keep the current parent, move to root, or
/// move under a specific section (ADR 0012).
#[derive(Clone, Copy, Default)]
pub enum ParentPatch {
    #[default]
    Keep,
    Root,
    Set(Uuid),
}

#[derive(Default)]
pub struct SectionPatch {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub parent: ParentPatch,
    pub position: Option<i32>,
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Locks the project row inside a section mutation transaction. All section
/// mutations for one project serialize here, which makes the in-transaction
/// cycle check race-free without distributed locks: two concurrent moves can
/// never interleave their checks and writes (ADR 0012).
async fn lock_project_row(
    transaction: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
) -> Result<bool, SectionError> {
    let locked = sqlx::query_scalar::<_, Uuid>(
        "SELECT p.id FROM projects p \
         WHERE p.id = $1 AND p.workspace_id = $2 AND p.tenant_id = $3 AND p.deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(project_id)
    .bind(workspace_id)
    .bind(tenant_id)
    .fetch_optional(transaction)
    .await
    .map_err(|_| SectionError::DatabaseError)?;
    Ok(locked.is_some())
}

/// Resolves a parent candidate inside the authoritative project boundary and
/// locks it: wrong-project/foreign parents are simply absent, and archived
/// or deleted parents are rejected (ADR 0012).
async fn lock_parent_in_project(
    transaction: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    parent_id: Uuid,
) -> Result<Option<SectionRow>, SectionError> {
    let sql = format!(
        "SELECT {SECTION_COLUMNS} FROM sections s \
         WHERE s.id = $1 AND s.project_id = $2 AND s.workspace_id = $3 AND s.tenant_id = $4 \
           AND s.deleted_at IS NULL AND s.status = 'active' \
         FOR UPDATE"
    );
    sqlx::query_as::<_, SectionRow>(&sql)
        .bind(parent_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(tenant_id)
        .fetch_optional(transaction)
        .await
        .map_err(|_| SectionError::DatabaseError)
}

/// True when `candidate` is the section itself or one of its transitive
/// descendants. UNION (not UNION ALL) keeps the walk terminating even on
/// hypothetically corrupt data; runs inside the mutation transaction under
/// the project row lock, never on a stale snapshot (ADR 0012).
async fn is_self_or_descendant(
    transaction: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
    candidate: Uuid,
) -> Result<bool, SectionError> {
    let hit = sqlx::query_scalar::<_, bool>(
        "WITH RECURSIVE descendants AS ( \
            SELECT id FROM sections \
            WHERE id = $1 AND project_id = $2 AND workspace_id = $3 AND tenant_id = $4 \
            UNION \
            SELECT s.id FROM sections s \
            JOIN descendants d ON s.parent_section_id = d.id \
            WHERE s.project_id = $2 AND s.workspace_id = $3 AND s.tenant_id = $4 \
          ) \
         SELECT EXISTS(SELECT 1 FROM descendants WHERE id = $5)",
    )
    .bind(section_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(tenant_id)
    .bind(candidate)
    .fetch_one(transaction)
    .await
    .map_err(|_| SectionError::DatabaseError)?;
    Ok(hit)
}

/// Next sibling position: append after the current maximum under the same
/// parent (NULL-safe via IS NOT DISTINCT FROM).
async fn next_position(
    transaction: &mut PgConnection,
    project_id: Uuid,
    parent_section_id: Option<Uuid>,
) -> Result<i32, SectionError> {
    sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(max(position), -1) + 1 FROM sections \
         WHERE project_id = $1 AND parent_section_id IS NOT DISTINCT FROM $2",
    )
    .bind(project_id)
    .bind(parent_section_id)
    .fetch_one(transaction)
    .await
    .map_err(|_| SectionError::DatabaseError)
}

/// Row-level-locked scoped lookup for mutations.
async fn lock_scoped(
    transaction: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
) -> Result<Option<SectionRow>, SectionError> {
    let sql = format!(
        "SELECT {SECTION_COLUMNS} FROM sections s \
         WHERE s.id = $1 AND s.project_id = $2 AND s.workspace_id = $3 AND s.tenant_id = $4 \
           AND s.deleted_at IS NULL \
         FOR UPDATE"
    );
    sqlx::query_as::<_, SectionRow>(&sql)
        .bind(section_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(tenant_id)
        .fetch_optional(transaction)
        .await
        .map_err(|_| SectionError::DatabaseError)
}

/// Single authoritative scoped lookup for reads: id ∧ resolved project ∧
/// resolved workspace/tenant ∧ not soft-deleted in ONE query. Callers hold
/// a ProjectContext-equivalent resolution; misses are uniform 404s.
pub async fn find_accessible(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
) -> Result<Option<SectionRow>, SectionError> {
    let sql = format!(
        "SELECT {SECTION_COLUMNS} FROM sections s \
         WHERE s.id = $1 AND s.project_id = $2 AND s.workspace_id = $3 AND s.tenant_id = $4 \
           AND s.deleted_at IS NULL"
    );
    sqlx::query_as::<_, SectionRow>(&sql)
        .bind(section_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(organization_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| SectionError::DatabaseError)
}

/// Flat ordered section list of one project (ADR 0012 tree strategy): a
/// SINGLE query — no N+1, no recursive walk — ordered so each sibling group
/// arrives already sorted; the frontend assembles the tree in memory.
pub async fn visible_for_project(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
) -> Result<Vec<SectionRow>, SectionError> {
    let sql = format!(
        "SELECT {SECTION_COLUMNS} FROM sections s \
         WHERE s.project_id = $1 AND s.workspace_id = $2 AND s.tenant_id = $3 \
           AND s.deleted_at IS NULL \
         ORDER BY s.parent_section_id NULLS FIRST, s.position, s.id"
    );
    sqlx::query_as::<_, SectionRow>(&sql)
        .bind(project_id)
        .bind(workspace_id)
        .bind(organization_id)
        .fetch_all(pool)
        .await
        .map_err(|_| SectionError::DatabaseError)
}

/// Section creation inside one transaction (ADR 0012): memberships and
/// parent authority re-proved on the transaction snapshot, permission
/// revalidated, the project row locked (mutation serialization), the parent
/// resolved inside the same project boundary, and the append position
/// computed freshly. Racing revocations cannot slip through.
pub async fn create_section(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section: &NewSection,
    creator: Uuid,
) -> Result<SectionRow, SectionError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| SectionError::DatabaseError)?;
    let workspace =
        workspaces::find_accessible(&mut *transaction, organization_id, workspace_id, creator)
            .await
            .map_err(|_| SectionError::DatabaseError)?;
    if workspace.is_none() {
        return Err(SectionError::NotAccessible);
    }
    if !lock_project_row(&mut transaction, organization_id, workspace_id, project_id).await? {
        return Err(SectionError::NotAccessible);
    }
    let authorized = crate::rbac::authorize_workspace_in_tx(
        &mut transaction,
        organization_id,
        workspace_id,
        creator,
        SECTIONS_CREATE,
    )
    .await
    .map_err(|_| SectionError::DatabaseError)?;
    if !authorized {
        return Err(SectionError::Forbidden);
    }
    if let Some(parent_id) = section.parent_section_id {
        let parent = lock_parent_in_project(
            &mut transaction,
            organization_id,
            workspace_id,
            project_id,
            parent_id,
        )
        .await?;
        if parent.is_none() {
            return Err(SectionError::InvalidParent);
        }
    }
    let position = next_position(&mut transaction, project_id, section.parent_section_id).await?;
    let row = sqlx::query_as::<_, SectionRow>(
        "INSERT INTO sections (id, tenant_id, workspace_id, project_id, parent_section_id, name, slug, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id, tenant_id AS organization_id, workspace_id, project_id, parent_section_id, \
                   name, slug::text AS slug, position, status",
    )
    .bind(Uuid::now_v7())
    .bind(organization_id)
    .bind(workspace_id)
    .bind(project_id)
    .bind(section.parent_section_id)
    .bind(&section.name)
    .bind(&section.slug)
    .bind(position)
    .fetch_one(&mut *transaction)
    .await;
    let created = match row {
        Ok(row) => row,
        // 23505 = sibling-scope slug unique violation.
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            return Err(SectionError::SlugAlreadyTaken);
        }
        Err(_) => return Err(SectionError::DatabaseError),
    };
    transaction
        .commit()
        .await
        .map_err(|_| SectionError::DatabaseError)?;
    Ok(created)
}

/// Section update / move inside one transaction with the same re-validation
/// discipline: memberships, project authority (row lock), permission, a
/// row-level section lock, fresh parent resolution in the same project, the
/// acyclicity check (self + descendant) under the serialization lock, and
/// transition validation. Reparenting resets the sibling position to append
/// unless the request supplies one (ADR 0012).
pub async fn update_section(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
    patch: &SectionPatch,
    actor: Uuid,
) -> Result<SectionRow, SectionError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| SectionError::DatabaseError)?;
    let workspace =
        workspaces::find_accessible(&mut *transaction, organization_id, workspace_id, actor)
            .await
            .map_err(|_| SectionError::DatabaseError)?;
    if workspace.is_none() {
        return Err(SectionError::NotAccessible);
    }
    if !lock_project_row(&mut transaction, organization_id, workspace_id, project_id).await? {
        return Err(SectionError::NotAccessible);
    }
    let authorized = crate::rbac::authorize_workspace_in_tx(
        &mut transaction,
        organization_id,
        workspace_id,
        actor,
        SECTIONS_UPDATE,
    )
    .await
    .map_err(|_| SectionError::DatabaseError)?;
    if !authorized {
        return Err(SectionError::Forbidden);
    }
    if patch.status.as_deref() == Some(SECTION_STATUS_ARCHIVED) {
        let may_archive = crate::rbac::authorize_workspace_in_tx(
            &mut transaction,
            organization_id,
            workspace_id,
            actor,
            SECTIONS_ARCHIVE,
        )
        .await
        .map_err(|_| SectionError::DatabaseError)?;
        if !may_archive {
            return Err(SectionError::Forbidden);
        }
    }
    let current = lock_scoped(
        &mut transaction,
        organization_id,
        workspace_id,
        project_id,
        section_id,
    )
    .await?
    .ok_or(SectionError::NotAccessible)?;

    let parent = match patch.parent {
        ParentPatch::Keep => current.parent_section_id,
        ParentPatch::Root => None,
        ParentPatch::Set(target) => {
            if target == section_id {
                return Err(SectionError::InvalidParent);
            }
            let parent = lock_parent_in_project(
                &mut transaction,
                organization_id,
                workspace_id,
                project_id,
                target,
            )
            .await?;
            if parent.is_none() {
                return Err(SectionError::InvalidParent);
            }
            if is_self_or_descendant(
                &mut transaction,
                organization_id,
                workspace_id,
                project_id,
                section_id,
                target,
            )
            .await?
            {
                return Err(SectionError::CycleDetected);
            }
            Some(target)
        }
    };
    let parent_changed = parent != current.parent_section_id;
    // Explicit position wins; a reparent without a position appends under
    // the new sibling group; otherwise the stored position is kept.
    let position = match (patch.position, parent_changed) {
        (Some(position), _) => position,
        (None, true) => next_position(&mut transaction, project_id, parent).await?,
        (None, false) => current.position,
    };
    let name = patch.name.clone().unwrap_or(current.name);
    let slug = patch.slug.clone().unwrap_or(current.slug);
    let status = patch
        .status
        .clone()
        .unwrap_or_else(|| current.status.clone());
    if !transition_allowed(&current.status, &status) {
        return Err(SectionError::InvalidTransition);
    }
    let row = sqlx::query_as::<_, SectionRow>(
        "UPDATE sections SET parent_section_id = $5, name = $6, slug = $7, position = $8, \
                 status = $9, updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND workspace_id = $3 AND tenant_id = $4 \
           AND deleted_at IS NULL \
         RETURNING id, tenant_id AS organization_id, workspace_id, project_id, parent_section_id, \
                   name, slug::text AS slug, position, status",
    )
    .bind(section_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(organization_id)
    .bind(parent)
    .bind(&name)
    .bind(&slug)
    .bind(position)
    .bind(&status)
    .fetch_one(&mut *transaction)
    .await;
    let updated = match row {
        Ok(row) => row,
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            return Err(SectionError::SlugAlreadyTaken);
        }
        Err(_) => return Err(SectionError::DatabaseError),
    };
    transaction
        .commit()
        .await
        .map_err(|_| SectionError::DatabaseError)?;
    Ok(updated)
}

// ---------------------------------------------------------------------------
// HTTP contracts
// ---------------------------------------------------------------------------

/// Three-state parent semantics for PATCH: absent keeps the parent, explicit
/// null moves the section to root, a UUID reparents (double-option pattern).
fn deserialize_parent_patch<'de, D>(deserializer: D) -> Result<Option<Option<Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateSectionRequest {
    pub name: String,
    pub slug: Option<String>,
    /// Optional parent; omitted or null creates a ROOT section.
    pub parent_section_id: Option<Uuid>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateSectionRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    /// Absent = keep; null = become a root section; UUID = reparent.
    #[serde(default, deserialize_with = "deserialize_parent_patch")]
    pub parent_section_id: Option<Option<Uuid>>,
    /// Sibling order; ignored when absent (a reparent appends instead).
    pub position: Option<i32>,
    pub status: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SectionPublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub parent_section_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub position: i32,
    pub status: String,
}

impl From<SectionRow> for SectionPublic {
    fn from(row: SectionRow) -> Self {
        Self {
            id: row.id,
            organization_id: row.organization_id,
            workspace_id: row.workspace_id,
            project_id: row.project_id,
            parent_section_id: row.parent_section_id,
            name: row.name,
            slug: row.slug,
            position: row.position,
            status: row.status,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct SectionMutationResponse {
    pub data: SectionPublic,
}

/// Flat ordered list (ADR 0012): one query per project, siblings pre-sorted;
/// clients assemble the tree in memory. Deliberately NOT a nested recursive
/// DTO — codegen ergonomics and future move/reorder stability win.
#[derive(Serialize, ToSchema)]
pub struct SectionListResponse {
    pub data: Vec<SectionPublic>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections",
    request_body = CreateSectionRequest,
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
        ("project_id" = Uuid, Path, description = "Parent project id"),
    ),
    responses(
        (status = 201, body = SectionMutationResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 403, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn create_section_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProjectContext,
    ApiJson(body): ApiJson<CreateSectionRequest>,
) -> Result<Response, ApiError> {
    let not_found = || ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone());
    let organization_id = context.organization_id;
    let workspace_id = context.workspace_id;
    let project_id = context.project_id;
    let name = body.name.trim().to_owned();
    let mut fields = serde_json::Map::new();
    if name.is_empty() {
        fields.insert("name".into(), serde_json::json!(["Required"]));
    } else if name.chars().count() > NAME_MAX_CHARS {
        fields.insert("name".into(), serde_json::json!(["Too long"]));
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

    let allowed = crate::rbac::authorize_workspace(
        &state.pool,
        organization_id,
        workspace_id,
        context.user_id,
        SECTIONS_CREATE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(crate::rbac::permission_denied(&request_id));
    }
    let result = create_section(
        &state.pool,
        organization_id,
        workspace_id,
        project_id,
        &NewSection {
            name,
            slug,
            parent_section_id: body.parent_section_id,
        },
        context.user_id,
    )
    .await;
    match result {
        Ok(section) => Ok((
            StatusCode::CREATED,
            Json(SectionMutationResponse {
                data: SectionPublic::from(section),
            }),
        )
            .into_response()),
        Err(SectionError::NotAccessible) => Err(not_found()),
        Err(SectionError::Forbidden) => Err(crate::rbac::permission_denied(&request_id)),
        Err(SectionError::SlugAlreadyTaken) => Err(ApiError::invalid_fields(
            serde_json::json!({ "slug": ["Already taken"] }),
            request_id.0,
        )),
        Err(SectionError::InvalidParent) => Err(ApiError::invalid_fields(
            serde_json::json!({ "parent_section_id": ["Invalid"] }),
            request_id.0,
        )),
        Err(SectionError::CycleDetected) => Err(ApiError::invalid_fields(
            serde_json::json!({ "parent_section_id": ["Would create a cycle"] }),
            request_id.0,
        )),
        Err(SectionError::InvalidTransition) => Err(ApiError::invalid_fields(
            serde_json::json!({ "status": ["Invalid transition"] }),
            request_id.0,
        )),
        Err(SectionError::DatabaseError) => {
            Err(ApiError::new(ErrorCode::InternalError, request_id.0))
        }
    }
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections",
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
        ("project_id" = Uuid, Path, description = "Parent project id"),
    ),
    responses(
        (status = 200, body = SectionListResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn list_sections(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: ProjectContext,
) -> Result<Json<SectionListResponse>, ApiError> {
    // Reads are eligibility-based (no sections:read in V1, ADR 0012): the
    // ProjectContext already proved the whole parent chain, and the query
    // is scoped to the resolved project only — one flat ordered query.
    let sections = visible_for_project(
        &state.pool,
        context.organization_id,
        context.workspace_id,
        context.project_id,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(SectionListResponse {
        data: sections.into_iter().map(SectionPublic::from).collect(),
    }))
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}",
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
        ("project_id" = Uuid, Path, description = "Parent project id"),
        ("section_id" = Uuid, Path, description = "Section id"),
    ),
    responses(
        (status = 200, body = SectionPublic),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn get_section(
    context: crate::context::SectionContext,
) -> Result<Json<SectionPublic>, ApiError> {
    // The full parent chain plus the section scoped to the resolved project
    // resolved in the SectionContext extractor; misses are uniform 404s.
    Ok(Json(SectionPublic::from(context.section)))
}

#[utoipa::path(patch,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}",
    request_body = UpdateSectionRequest,
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Parent workspace id"),
        ("project_id" = Uuid, Path, description = "Parent project id"),
        ("section_id" = Uuid, Path, description = "Section id"),
    ),
    responses(
        (status = 200, body = SectionMutationResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 403, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn update_section_handler(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: crate::context::SectionContext,
    ApiJson(body): ApiJson<UpdateSectionRequest>,
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
    if body.position.is_some_and(|position| position < 0) {
        fields.insert("position".into(), serde_json::json!(["Invalid"]));
    }
    let status = body
        .status
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty());
    if status
        .is_some_and(|value| value != SECTION_STATUS_ACTIVE && value != SECTION_STATUS_ARCHIVED)
    {
        fields.insert("status".into(), serde_json::json!(["Invalid value"]));
    }
    if !fields.is_empty() {
        return Err(ApiError::invalid_fields(
            serde_json::Value::Object(fields),
            request_id.0,
        ));
    }
    let patch = SectionPatch {
        name: name.filter(|text| !text.is_empty()).map(str::to_owned),
        slug: slug.flatten(),
        parent: match body.parent_section_id {
            None => ParentPatch::Keep,
            Some(None) => ParentPatch::Root,
            Some(Some(target)) => ParentPatch::Set(target),
        },
        position: body.position,
        status: status.map(str::to_owned),
    };

    // Handler-level fast check; the transaction re-proves everything anyway.
    let allowed = crate::rbac::authorize_workspace(
        &state.pool,
        context.organization_id,
        context.workspace_id,
        context.user_id,
        SECTIONS_UPDATE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(crate::rbac::permission_denied(&request_id));
    }
    let result = update_section(
        &state.pool,
        context.organization_id,
        context.workspace_id,
        context.project_id,
        context.section_id,
        &patch,
        context.user_id,
    )
    .await;
    match result {
        Ok(section) => Ok((
            StatusCode::OK,
            Json(SectionMutationResponse {
                data: SectionPublic::from(section),
            }),
        )
            .into_response()),
        Err(SectionError::NotAccessible) => Err(not_found()),
        Err(SectionError::Forbidden) => Err(crate::rbac::permission_denied(&request_id)),
        Err(SectionError::SlugAlreadyTaken) => Err(ApiError::invalid_fields(
            serde_json::json!({ "slug": ["Already taken"] }),
            request_id.0,
        )),
        Err(SectionError::InvalidParent) => Err(ApiError::invalid_fields(
            serde_json::json!({ "parent_section_id": ["Invalid"] }),
            request_id.0,
        )),
        Err(SectionError::CycleDetected) => Err(ApiError::invalid_fields(
            serde_json::json!({ "parent_section_id": ["Would create a cycle"] }),
            request_id.0,
        )),
        Err(SectionError::InvalidTransition) => Err(ApiError::invalid_fields(
            serde_json::json!({ "status": ["Invalid transition"] }),
            request_id.0,
        )),
        Err(SectionError::DatabaseError) => {
            Err(ApiError::new(ErrorCode::InternalError, request_id.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_keys_are_stable_machine_identifiers() {
        assert_eq!(SECTIONS_CREATE.0, "sections:create");
        assert_eq!(SECTIONS_UPDATE.0, "sections:update");
        assert_eq!(SECTIONS_ARCHIVE.0, "sections:archive");
    }

    #[test]
    fn structural_lifecycle_transitions() {
        assert!(transition_allowed("active", "archived"));
        assert!(transition_allowed("archived", "active"));
        // Identity keeps a PATCH idempotent.
        assert!(transition_allowed("active", "active"));
        assert!(transition_allowed("archived", "archived"));
        // Sections never "complete" (ADR 0012).
        assert!(!transition_allowed("active", "completed"));
        assert!(!transition_allowed("archived", "completed"));
        assert!(!transition_allowed("active", "unknown"));
    }
}
