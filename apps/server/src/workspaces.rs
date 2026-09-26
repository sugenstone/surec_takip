use crate::{
    AppState, RequestId,
    auth::ApiJson,
    error::{ApiError, ErrorCode},
    organizations::{self, slug_from_text, validate_slug},
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

pub const WORKSPACE_MEMBERSHIP_STATUS_ACTIVE: &str = "active";

// Workspace access requires BOTH a valid organization membership AND a valid
// workspace membership, joined to non-deleted workspace/organization rows
// (ADR 0007). A stale workspace membership therefore grants nothing once the
// organization membership is gone or the organization is deleted.
const ACCESS_FILTER: &str = "wm.status = 'active' AND wm.deleted_at IS NULL \
     AND om.status = 'active' AND om.deleted_at IS NULL \
     AND o.deleted_at IS NULL AND w.deleted_at IS NULL";

const WORKSPACE_COLUMNS: &str =
    "w.id, w.tenant_id AS organization_id, w.name, w.slug::text AS slug";

#[derive(Debug)]
pub enum WorkspaceError {
    SlugAlreadyTaken,
    NotAccessible,
    Forbidden,
    DatabaseError,
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            WorkspaceError::SlugAlreadyTaken => "slug already exists",
            WorkspaceError::NotAccessible => "organization is not accessible",
            WorkspaceError::Forbidden => "permission denied",
            WorkspaceError::DatabaseError => "database operation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for WorkspaceError {}

#[derive(Clone, sqlx::FromRow)]
pub struct WorkspaceRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub slug: String,
}

#[derive(sqlx::FromRow)]
pub struct WorkspaceMembershipRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub status: String,
}

pub struct NewWorkspace {
    pub name: String,
    pub slug: String,
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

async fn insert_workspace(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    workspace: &NewWorkspace,
) -> Result<WorkspaceRow, WorkspaceError> {
    let row = sqlx::query_as::<_, WorkspaceRow>(
        "INSERT INTO workspaces (id, tenant_id, name, slug) VALUES ($1, $2, $3, $4) \
         RETURNING id, tenant_id AS organization_id, name, slug::text AS slug",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(&workspace.name)
    .bind(&workspace.slug)
    .fetch_one(connection)
    .await;
    match row {
        Ok(row) => Ok(row),
        // 23505 = unique_violation on workspaces_tenant_slug_key (per tenant).
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            Err(WorkspaceError::SlugAlreadyTaken)
        }
        Err(_) => Err(WorkspaceError::DatabaseError),
    }
}

async fn insert_membership(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<WorkspaceMembershipRow, WorkspaceError> {
    let row = sqlx::query_as::<_, WorkspaceMembershipRow>(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active') \
         RETURNING id, tenant_id, workspace_id, user_id, status",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(workspace_id)
    .bind(user_id)
    .fetch_one(connection)
    .await;
    match row {
        Ok(row) => Ok(row),
        Err(_) => Err(WorkspaceError::DatabaseError),
    }
}

/// Workspace creation runs inside one transaction: the creator's organization
/// membership is re-verified on the transaction's snapshot, the workspace is
/// inserted under the requested organization, and the creator's workspace
/// membership commits together with it. A failed membership insert rolls the
/// workspace back — no orphan workspace.
pub async fn create_with_membership(
    pool: &PgPool,
    organization_id: Uuid,
    workspace: &NewWorkspace,
    creator: Uuid,
    required: crate::rbac::PermissionKey,
) -> Result<(WorkspaceRow, WorkspaceMembershipRow), WorkspaceError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| WorkspaceError::DatabaseError)?;
    // Organization membership is a precondition; row-level locks are not
    // needed because no removal endpoint exists yet and every read path
    // re-validates both memberships anyway (ACCESS_FILTER).
    let member = organizations::find_for_member(&mut *transaction, organization_id, creator)
        .await
        .map_err(|_| WorkspaceError::DatabaseError)?;
    if member.is_none() {
        return Err(WorkspaceError::NotAccessible);
    }
    // Permission is revalidated INSIDE the transaction: a revocation racing
    // the write cannot slip through between the handler check and commit.
    let authorized = crate::rbac::authorize_organization_in_tx(
        &mut transaction,
        organization_id,
        creator,
        required,
    )
    .await
    .map_err(|_| WorkspaceError::DatabaseError)?;
    if !authorized {
        return Err(WorkspaceError::Forbidden);
    }
    let workspace = insert_workspace(&mut transaction, organization_id, workspace).await?;
    let membership =
        insert_membership(&mut transaction, organization_id, workspace.id, creator).await?;
    transaction
        .commit()
        .await
        .map_err(|_| WorkspaceError::DatabaseError)?;
    Ok((workspace, membership))
}

/// Workspaces the user can access inside one organization: active workspace
/// membership AND active organization membership AND non-deleted rows,
/// in deterministic creation order.
pub async fn visible_for_user(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<WorkspaceRow>, WorkspaceError> {
    let sql = format!(
        "SELECT {WORKSPACE_COLUMNS} \
         FROM workspaces w \
         JOIN workspace_memberships wm ON wm.workspace_id = w.id \
         JOIN organization_memberships om ON om.tenant_id = w.tenant_id AND om.user_id = wm.user_id \
         JOIN organizations o ON o.id = w.tenant_id \
         WHERE w.tenant_id = $1 AND wm.user_id = $2 AND {ACCESS_FILTER} \
         ORDER BY w.created_at, w.id"
    );
    sqlx::query_as::<_, WorkspaceRow>(&sql)
        .bind(organization_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(|_| WorkspaceError::DatabaseError)
}

/// Single-workspace access check. The workspace must belong to the requested
/// parent organization (w.tenant_id = organization_id), and both memberships
/// must be valid. Every miss returns None for indistinguishable 404s.
/// Accepts any executor so project mutations can re-run the same check on
/// their transaction snapshot (ADR 0011).
pub async fn find_accessible(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    organization_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Option<WorkspaceRow>, WorkspaceError> {
    let sql = format!(
        "SELECT {WORKSPACE_COLUMNS} \
         FROM workspaces w \
         JOIN workspace_memberships wm ON wm.workspace_id = w.id \
         JOIN organization_memberships om ON om.tenant_id = w.tenant_id AND om.user_id = wm.user_id \
         JOIN organizations o ON o.id = w.tenant_id \
         WHERE w.id = $1 AND w.tenant_id = $2 AND wm.user_id = $3 AND {ACCESS_FILTER}"
    );
    sqlx::query_as::<_, WorkspaceRow>(&sql)
        .bind(workspace_id)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(executor)
        .await
        .map_err(|_| WorkspaceError::DatabaseError)
}

// ---------------------------------------------------------------------------
// HTTP contracts
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub slug: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct WorkspacePublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub slug: String,
}

impl From<WorkspaceRow> for WorkspacePublic {
    fn from(row: WorkspaceRow) -> Self {
        Self {
            id: row.id,
            organization_id: row.organization_id,
            name: row.name,
            slug: row.slug,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct WorkspaceMembershipPublic {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub status: String,
}

impl From<WorkspaceMembershipRow> for WorkspaceMembershipPublic {
    fn from(row: WorkspaceMembershipRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            organization_id: row.tenant_id,
            user_id: row.user_id,
            status: row.status,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct CreateWorkspaceData {
    pub workspace: WorkspacePublic,
    pub membership: WorkspaceMembershipPublic,
}

#[derive(Serialize, ToSchema)]
pub struct CreateWorkspaceResponse {
    pub data: CreateWorkspaceData,
}

#[derive(Serialize, ToSchema)]
pub struct WorkspaceListResponse {
    pub data: Vec<WorkspacePublic>,
}

/// Minimal member identity for the assignee picker (STEP 21C, ADR 0018):
/// id + display name only — no email, roles, or membership internals.
#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct WorkspaceMemberPublic {
    pub id: Uuid,
    pub display_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct WorkspaceMemberListResponse {
    pub data: Vec<WorkspaceMemberPublic>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post,
    path = "/api/v1/organizations/{organization_id}/workspaces",
    request_body = CreateWorkspaceRequest,
    params(("organization_id" = Uuid, Path, description = "Organization id")),
    responses(
        (status = 201, body = CreateWorkspaceResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn create_workspace(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: crate::context::OrganizationContext,
    ApiJson(body): ApiJson<CreateWorkspaceRequest>,
) -> Result<Response, ApiError> {
    // The route is organization-scoped: the workspace does not exist yet, so
    // only OrganizationContext applies. create_with_membership re-validates
    // the organization membership inside the transaction (TOCTOU, ADR 0007).
    let not_found = || ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone());
    let organization_id = context.organization_id;
    let creator = context.user_id;
    let name = body.name.trim().to_owned();
    let mut fields = serde_json::Map::new();
    if name.is_empty() {
        fields.insert("name".into(), serde_json::json!(["Required"]));
    } else if name.chars().count() > 200 {
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

    // Eligibility is proven by the context; permission is checked here and
    // RE-CHECKED inside the mutation transaction (TOCTOU, ADR 0008).
    let allowed = crate::rbac::authorize_organization(
        &state.pool,
        organization_id,
        creator,
        crate::rbac::WORKSPACES_CREATE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(crate::rbac::permission_denied(&request_id));
    }
    let result = create_with_membership(
        &state.pool,
        organization_id,
        &NewWorkspace { name, slug },
        creator,
        crate::rbac::WORKSPACES_CREATE,
    )
    .await;
    match result {
        Ok((workspace, membership)) => {
            let body = Json(CreateWorkspaceResponse {
                data: CreateWorkspaceData {
                    workspace: WorkspacePublic::from(workspace),
                    membership: WorkspaceMembershipPublic::from(membership),
                },
            });
            Ok((StatusCode::CREATED, body).into_response())
        }
        // Not a visible member of the (possibly nonexistent) organization.
        Err(WorkspaceError::NotAccessible) => Err(not_found()),
        // Permission revoked between the handler check and the transaction.
        Err(WorkspaceError::Forbidden) => Err(crate::rbac::permission_denied(&request_id)),
        Err(WorkspaceError::SlugAlreadyTaken) => Err(ApiError::invalid_fields(
            serde_json::json!({ "slug": ["Already taken"] }),
            request_id.0,
        )),
        Err(WorkspaceError::DatabaseError) => {
            Err(ApiError::new(ErrorCode::InternalError, request_id.0))
        }
    }
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces",
    params(("organization_id" = Uuid, Path, description = "Organization id")),
    responses(
        (status = 200, body = WorkspaceListResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: crate::context::OrganizationContext,
) -> Result<Json<WorkspaceListResponse>, ApiError> {
    let workspaces = visible_for_user(&state.pool, context.organization_id, context.user_id)
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(WorkspaceListResponse {
        data: workspaces.into_iter().map(WorkspacePublic::from).collect(),
    }))
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}",
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Workspace id"),
    ),
    responses(
        (status = 200, body = WorkspacePublic),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn get_workspace(
    context: crate::context::WorkspaceContext,
) -> Result<Json<WorkspacePublic>, ApiError> {
    // The full step 9 access invariant (both memberships, parent-child
    // authority, visibility) is resolved by the WorkspaceContext extractor.
    Ok(Json(WorkspacePublic::from(context.workspace)))
}

/// Member directory (STEP 21C): only users that would currently satisfy
/// assignment eligibility — active user, active organization membership,
/// active workspace membership. Access gate is the ordinary workspace
/// context: any legitimate workspace member may see member names (operational
/// metadata); the write path alone requires processes:assign.
#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/members",
    params(
        ("organization_id" = Uuid, Path, description = "Parent organization id"),
        ("workspace_id" = Uuid, Path, description = "Workspace id"),
    ),
    responses(
        (status = 200, body = WorkspaceMemberListResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn list_workspace_members(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: crate::context::WorkspaceContext,
) -> Result<Json<WorkspaceMemberListResponse>, ApiError> {
    let members = sqlx::query_as::<_, WorkspaceMemberPublic>(
        "SELECT u.id, u.display_name \
         FROM workspace_memberships wm \
         JOIN organization_memberships om ON om.tenant_id = wm.tenant_id \
             AND om.user_id = wm.user_id \
             AND om.status = 'active' AND om.deleted_at IS NULL \
         JOIN users u ON u.id = wm.user_id AND u.status = 'active' \
         WHERE wm.tenant_id = $1 AND wm.workspace_id = $2 \
           AND wm.status = 'active' AND wm.deleted_at IS NULL \
         ORDER BY u.display_name, u.id",
    )
    .bind(context.organization_id)
    .bind(context.workspace_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(WorkspaceMemberListResponse { data: members }))
}
