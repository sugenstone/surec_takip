pub mod auth;
pub mod config;
pub mod context;
pub mod database;
pub mod error;
pub mod invitations;
pub mod migrations;
pub mod organizations;
pub mod password;
pub mod permissions;
pub mod projects;
pub mod rbac;
pub mod users;
pub mod workspaces;

use crate::password::PasswordService;
use axum::{
    Extension, Json, Router,
    extract::{MatchedPath, Request, State},
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post},
};
use error::{ApiError, ErrorCode};
use serde::Serialize;
use sqlx::PgPool;
use std::{sync::Arc, time::Instant};
use utoipa::{OpenApi, ToSchema};

#[derive(Clone)]
pub struct RequestId(pub String);

// Shared application state: every handler receives it through the router.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: config::AuthConfig,
    pub passwords: Arc<PasswordService>,
    // Argon2 hash of an unguessable placeholder used only to equalize the
    // timing of login attempts against unknown emails.
    pub dummy_hash: Arc<String>,
}

impl AppState {
    pub fn new(pool: PgPool, auth: config::AuthConfig) -> Result<Self, &'static str> {
        let passwords = PasswordService::new(&auth).map_err(|_| "PASSWORD_SERVICE_INIT_FAILED")?;
        let dummy_hash = passwords
            .hash("timing-equalization-placeholder")
            .map_err(|_| "PASSWORD_SERVICE_INIT_FAILED")?;
        Ok(Self {
            pool,
            auth,
            passwords: Arc::new(passwords),
            dummy_hash: Arc::new(dummy_hash),
        })
    }
}

#[derive(Serialize, ToSchema)]
pub struct HealthData {
    pub status: HealthStatus,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Ready,
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub data: HealthData,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/ready", get(ready))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/auth/me", get(auth::me))
        .route(
            "/api/v1/organizations",
            post(organizations::create_organization).get(organizations::list_organizations),
        )
        .route(
            "/api/v1/organizations/{organization_id}",
            get(organizations::get_organization),
        )
        .route(
            "/api/v1/organizations/{organization_id}/workspaces",
            post(workspaces::create_workspace).get(workspaces::list_workspaces),
        )
        .route(
            "/api/v1/organizations/{organization_id}/permissions",
            get(organizations::list_permissions),
        )
        .route(
            "/api/v1/organizations/{organization_id}/roles",
            get(organizations::list_roles),
        )
        .route(
            "/api/v1/organizations/{organization_id}/effective-permissions",
            get(permissions::effective_permissions),
        )
        .route(
            "/api/v1/organizations/{organization_id}/invitations",
            post(invitations::create_invitation).get(invitations::list_invitations),
        )
        .route(
            "/api/v1/organizations/{organization_id}/invitations/{invitation_id}",
            delete(invitations::revoke_invitation),
        )
        .route(
            "/api/v1/invitations/accept",
            post(invitations::accept_invitation),
        )
        .route(
            "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}",
            get(workspaces::get_workspace),
        )
        .route(
            "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects",
            post(projects::create_project_handler).get(projects::list_projects),
        )
        .route(
            "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}",
            get(projects::get_project).patch(projects::update_project_handler),
        )
        .route(
            "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/effective-permissions",
            get(permissions::effective_workspace_permissions),
        )
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn(request_context))
        .with_state(state)
}

#[utoipa::path(get, path = "/api/v1/health", responses((status = 200, body = HealthResponse)))]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        data: HealthData {
            status: HealthStatus::Ok,
        },
    })
}

#[utoipa::path(get, path = "/api/v1/ready", responses(
    (status = 200, body = HealthResponse),
    (status = 503, body = error::ErrorEnvelope)
))]
async fn ready(
    State(state): State<AppState>,
    Extension(id): Extension<RequestId>,
) -> Result<Json<HealthResponse>, ApiError> {
    if !database::is_ready(&state.pool).await {
        return Err(ApiError::new(ErrorCode::ServiceNotReady, id.0));
    }
    Ok(Json(HealthResponse {
        data: HealthData {
            status: HealthStatus::Ready,
        },
    }))
}

async fn not_found(Extension(id): Extension<RequestId>) -> ApiError {
    ApiError::new(ErrorCode::ResourceNotFound, id.0)
}

async fn method_not_allowed(Extension(id): Extension<RequestId>) -> ApiError {
    ApiError::new(ErrorCode::MethodNotAllowed, id.0)
}

async fn request_context(mut request: Request, next: Next) -> Response {
    let request_id = uuid::Uuid::now_v7().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let method = request.method().clone();
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let started = Instant::now();
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    // Route templates only: raw paths, query strings, cookies and credentials never enter logs.
    tracing::info!(%request_id, %method, %route, status = response.status().as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "http_request");
    response
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        ready,
        auth::login,
        auth::logout,
        auth::me,
        organizations::create_organization,
        organizations::list_organizations,
        organizations::get_organization,
        workspaces::create_workspace,
        workspaces::list_workspaces,
        workspaces::get_workspace,
        projects::create_project_handler,
        projects::list_projects,
        projects::get_project,
        projects::update_project_handler,
        permissions::effective_workspace_permissions,
        organizations::list_permissions,
        organizations::list_roles,
        permissions::effective_permissions,
        invitations::create_invitation,
        invitations::list_invitations,
        invitations::revoke_invitation,
        invitations::accept_invitation,
    ),
    components(schemas(
        HealthResponse,
        HealthData,
        HealthStatus,
        organizations::CreateOrganizationRequest,
        organizations::CreateOrganizationResponse,
        organizations::CreateOrganizationData,
        organizations::OrganizationPublic,
        organizations::MembershipPublic,
        organizations::OrganizationListResponse,
        workspaces::CreateWorkspaceRequest,
        workspaces::CreateWorkspaceResponse,
        workspaces::CreateWorkspaceData,
        workspaces::WorkspacePublic,
        workspaces::WorkspaceMembershipPublic,
        workspaces::WorkspaceListResponse,
        projects::CreateProjectRequest,
        projects::UpdateProjectRequest,
        projects::ProjectPublic,
        projects::ProjectMutationResponse,
        projects::ProjectListResponse,
        organizations::PermissionPublic,
        organizations::PermissionListResponse,
        organizations::RolePublic,
        organizations::RoleListResponse,
        permissions::EffectivePermissionsData,
        permissions::EffectivePermissionsResponse,
        invitations::CreateInvitationRequest,
        invitations::CreateInvitationResponse,
        invitations::CreateInvitationData,
        invitations::InvitationPublic,
        invitations::InvitationListResponse,
        invitations::AcceptInvitationRequest,
        invitations::AcceptInvitationResponse,
        invitations::AcceptInvitationData,
        auth::LoginRequest,
        auth::LoginResponse,
        auth::LoginData,
        auth::UserPublic,
        auth::OrganizationSummary,
        auth::MeResponse,
        auth::MeData,
        auth::AckResponse,
        auth::AckData,
        error::ErrorEnvelope,
        error::ErrorBody,
        error::ErrorCode
    ))
)]
pub struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
