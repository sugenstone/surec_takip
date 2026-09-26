//! Tenant request contexts (First Agent Mission step 10).
//!
//! A resolved context is authorization/scoping state derived from the
//! authenticated user and database relationships — never from UX cookies
//! (ADR 0007). Organization-scoped routes resolve `OrganizationContext`;
//! workspace-scoped routes resolve `WorkspaceContext` on top of it. Routes
//! that need no workspace never receive a fake/default one.

use crate::{
    AppState, auth,
    error::{ApiError, ErrorCode},
    organizations::{self, OrganizationRow},
    projects::{self, ProjectRow},
    sections::{self, SectionRow},
    workspaces::{self, WorkspaceRow},
};
use axum::extract::{FromRequestParts, Path};
use serde::Deserialize;
use uuid::Uuid;

// Route identity is extracted by NAME, not by tuple arity: serde struct
// deserialization ignores extra path parameters, so these extractors keep
// working on future deeper routes (e.g. .../workspaces/{w}/projects/{p}).
// Tuple/String extraction would instead reject any route whose total
// parameter count differs.
#[derive(Deserialize)]
struct OrganizationRoute {
    organization_id: String,
}

#[derive(Deserialize)]
struct WorkspaceRoute {
    organization_id: String,
    workspace_id: String,
}

#[derive(Deserialize)]
struct ProjectRoute {
    organization_id: String,
    workspace_id: String,
    project_id: String,
}

#[derive(Deserialize)]
struct SectionRoute {
    organization_id: String,
    workspace_id: String,
    project_id: String,
    section_id: String,
}

/// Organization-scoped tenant context.
///
/// Canonical invariant (ADR 0006): `organizations.id` IS the tenant id, so
/// `tenant_id` and `organization_id` carry the same server-resolved value
/// under both of its canonical names. Future repository queries scope by
/// `tenant_id`; API-facing code uses `organization_id`.
#[derive(Clone)]
pub struct OrganizationContext {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Uuid,
    pub organization: OrganizationRow,
}

/// Workspace-scoped tenant context. Only constructible when the full step 9
/// access invariant holds: active organization membership AND active
/// workspace membership AND the workspace belongs to the requested
/// organization AND neither row is soft-deleted.
#[derive(Clone)]
pub struct WorkspaceContext {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub workspace: WorkspaceRow,
}

fn request_id_of(parts: &axum::http::request::Parts) -> String {
    parts
        .extensions
        .get::<crate::RequestId>()
        .map(|id| id.0.clone())
        .unwrap_or_default()
}

fn not_found(parts: &axum::http::request::Parts) -> ApiError {
    ApiError::new(ErrorCode::ResourceNotFound, request_id_of(parts))
}

impl FromRequestParts<AppState> for OrganizationContext {
    type Rejection = ApiError;

    /// Resolution order (ADR 0007): authentication first (401), then the
    /// raw route id (malformed behaves exactly like unknown), then the
    /// authoritative membership lookup. Inaccessible organizations are
    /// indistinguishable from nonexistent ones.
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current = auth::resolve_current_user(parts, state).await?;
        let route = Path::<OrganizationRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let raw = route.0.organization_id;
        // Malformed ids resolve exactly like unknown ones.
        let Ok(organization_id) = raw.parse::<Uuid>() else {
            return Err(not_found(parts));
        };
        match organizations::find_for_member(&state.pool, organization_id, current.user.id).await {
            Ok(Some(organization)) => Ok(OrganizationContext {
                user_id: current.user.id,
                tenant_id: organization.id,
                organization_id: organization.id,
                organization,
            }),
            Ok(None) => Err(not_found(parts)),
            Err(_) => Err(ApiError::new(
                ErrorCode::InternalError,
                request_id_of(parts),
            )),
        }
    }
}

impl FromRequestParts<AppState> for WorkspaceContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current = auth::resolve_current_user(parts, state).await?;
        let path = Path::<WorkspaceRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let WorkspaceRoute {
            organization_id: raw_organization,
            workspace_id: raw_workspace,
        } = path.0;
        let not_found = || not_found(parts);
        // Malformed ids resolve exactly like unknown ones.
        let Ok(organization_id) = raw_organization.parse::<Uuid>() else {
            return Err(not_found());
        };
        let Ok(workspace_id) = raw_workspace.parse::<Uuid>() else {
            return Err(not_found());
        };
        // Parent-child authority and both memberships live in this single
        // authoritative query (workspaces::find_accessible / ACCESS_FILTER).
        match workspaces::find_accessible(
            &state.pool,
            organization_id,
            workspace_id,
            current.user.id,
        )
        .await
        {
            Ok(Some(workspace)) => Ok(WorkspaceContext {
                user_id: current.user.id,
                tenant_id: workspace.organization_id,
                organization_id: workspace.organization_id,
                workspace_id: workspace.id,
                workspace,
            }),
            Ok(None) => Err(not_found()),
            Err(_) => Err(ApiError::new(
                ErrorCode::InternalError,
                request_id_of(parts),
            )),
        }
    }
}

/// Project-scoped tenant context (step 17, ADR 0011). Only constructible
/// when the full parent chain holds: the WorkspaceContext invariant AND the
/// project belonging to the resolved workspace/tenant AND not soft-deleted.
/// The route parent chain is authoritative — a project id alone never
/// resolves anything, so wrong-parent combinations are uniform 404s.
///
/// This resolves request ELIGIBILITY only; mutations must re-prove
/// memberships and permission inside their transaction.
#[derive(Clone)]
pub struct ProjectContext {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub project: ProjectRow,
}

impl FromRequestParts<AppState> for ProjectContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current = auth::resolve_current_user(parts, state).await?;
        let path = Path::<ProjectRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let ProjectRoute {
            organization_id: raw_organization,
            workspace_id: raw_workspace,
            project_id: raw_project,
        } = path.0;
        let not_found = || not_found(parts);
        // Malformed ids resolve exactly like unknown ones.
        let Ok(organization_id) = raw_organization.parse::<Uuid>() else {
            return Err(not_found());
        };
        let Ok(workspace_id) = raw_workspace.parse::<Uuid>() else {
            return Err(not_found());
        };
        let Ok(project_id) = raw_project.parse::<Uuid>() else {
            return Err(not_found());
        };
        // Parent authority and both memberships first (same authoritative
        // query as WorkspaceContext), then the project scoped to the resolved
        // parent pair in one query — no separate "then check tenant" step.
        let workspace = workspaces::find_accessible(
            &state.pool,
            organization_id,
            workspace_id,
            current.user.id,
        )
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?;
        let Some(workspace) = workspace else {
            return Err(not_found());
        };
        let project =
            projects::find_accessible(&state.pool, organization_id, workspace_id, project_id)
                .await
                .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?;
        match project {
            Some(project) => Ok(ProjectContext {
                user_id: current.user.id,
                tenant_id: workspace.organization_id,
                organization_id: workspace.organization_id,
                workspace_id: workspace.id,
                project_id: project.id,
                project,
            }),
            None => Err(not_found()),
        }
    }
}

/// Section-scoped tenant context (step 18, ADR 0012). Only constructible
/// when the FULL parent chain holds: the ProjectContext invariant AND the
/// section belonging to the resolved project AND not soft-deleted. The
/// route parent chain stays authoritative — a section id alone never
/// resolves anything. Resolves request ELIGIBILITY only; mutations re-prove
/// everything inside their transaction.
#[derive(Clone)]
pub struct SectionContext {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub section_id: Uuid,
    pub section: SectionRow,
}

impl FromRequestParts<AppState> for SectionContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current = auth::resolve_current_user(parts, state).await?;
        let path = Path::<SectionRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let SectionRoute {
            organization_id: raw_organization,
            workspace_id: raw_workspace,
            project_id: raw_project,
            section_id: raw_section,
        } = path.0;
        let not_found = || not_found(parts);
        // Malformed ids resolve exactly like unknown ones.
        let Ok(organization_id) = raw_organization.parse::<Uuid>() else {
            return Err(not_found());
        };
        let Ok(workspace_id) = raw_workspace.parse::<Uuid>() else {
            return Err(not_found());
        };
        let Ok(project_id) = raw_project.parse::<Uuid>() else {
            return Err(not_found());
        };
        let Ok(section_id) = raw_section.parse::<Uuid>() else {
            return Err(not_found());
        };
        // Parent chain first (same authoritative queries as ProjectContext),
        // then the section scoped to the resolved project in one query.
        let workspace = workspaces::find_accessible(
            &state.pool,
            organization_id,
            workspace_id,
            current.user.id,
        )
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?;
        let Some(workspace) = workspace else {
            return Err(not_found());
        };
        let project =
            projects::find_accessible(&state.pool, organization_id, workspace_id, project_id)
                .await
                .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?;
        let Some(project) = project else {
            return Err(not_found());
        };
        let section = sections::find_accessible(
            &state.pool,
            organization_id,
            workspace_id,
            project_id,
            section_id,
        )
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?;
        match section {
            Some(section) => Ok(SectionContext {
                user_id: current.user.id,
                tenant_id: workspace.organization_id,
                organization_id: workspace.organization_id,
                workspace_id: workspace.id,
                project_id: project.id,
                section_id: section.id,
                section,
            }),
            None => Err(not_found()),
        }
    }
}

/// Work item collection context adds its own parent lifecycle policy without
/// changing the existing Sections/Projects read and reactivation behavior.
#[derive(Clone)]
pub struct WorkItemSectionContext {
    pub user_id: Uuid,
    pub scope: crate::work_items::WorkItemScope,
}
impl FromRequestParts<AppState> for WorkItemSectionContext {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let project = ProjectContext::from_request_parts(parts, state).await?;
        if project.project.status == "archived" {
            return Err(not_found(parts));
        }
        let Path(route) = Path::<SectionRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let section_id = route
            .section_id
            .parse::<Uuid>()
            .map_err(|_| not_found(parts))?;
        let section = sections::find_accessible(
            &state.pool,
            project.organization_id,
            project.workspace_id,
            project.project_id,
            section_id,
        )
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?
        .filter(|s| s.status == "active")
        .ok_or_else(|| not_found(parts))?;
        Ok(Self {
            user_id: project.user_id,
            scope: crate::work_items::WorkItemScope {
                organization_id: project.organization_id,
                workspace_id: project.workspace_id,
                project_id: project.project_id,
                section_id: section.id,
            },
        })
    }
}

#[derive(Deserialize)]
struct WorkItemRoute {
    work_item_id: String,
}

pub struct WorkItemContext {
    pub parent: WorkItemSectionContext,
    pub work_item: crate::work_items::WorkItemPublic,
}
impl FromRequestParts<AppState> for WorkItemContext {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let parent = WorkItemSectionContext::from_request_parts(parts, state).await?;
        let Path(route) = Path::<WorkItemRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let id = route
            .work_item_id
            .parse::<Uuid>()
            .map_err(|_| not_found(parts))?;
        let work_item = crate::work_items::find_accessible(&state.pool, parent.scope, id)
            .await
            .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?
            .ok_or_else(|| not_found(parts))?;
        Ok(Self { parent, work_item })
    }
}

#[derive(Deserialize)]
struct ProcessRoute {
    process_id: String,
}

/// Process context (STEP 20, ADR 0015): the FULL work item chain first, then
/// the process scoped to that exact work item. A process id alone never
/// resolves anything; wrong-parent, foreign, archived and unknown processes
/// are one uniform 404. Eligibility only; mutations re-prove in-transaction.
pub struct ProcessContext {
    pub parent: WorkItemContext,
    pub process: crate::processes::ProcessPublic,
}
impl FromRequestParts<AppState> for ProcessContext {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let parent = WorkItemContext::from_request_parts(parts, state).await?;
        let Path(route) = Path::<ProcessRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let id = route
            .process_id
            .parse::<Uuid>()
            .map_err(|_| not_found(parts))?;
        let scope = crate::processes::ProcessScope::of(&parent);
        let process = crate::processes::find_accessible(&state.pool, scope, id)
            .await
            .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?
            .ok_or_else(|| not_found(parts))?;
        Ok(Self { parent, process })
    }
}

#[derive(Deserialize)]
struct ExecutionRoute {
    execution_id: String,
}

/// Process execution context (STEP 21A, ADR 0016): the FULL
/// Org→Workspace→Project→Section→WorkItem→Process chain first, then the
/// execution scoped to that exact process. An execution id alone never
/// resolves anything; wrong-parent, foreign and unknown executions are one
/// uniform 404. Eligibility only; mutations re-prove everything
/// in-transaction.
pub struct ProcessExecutionContext {
    pub parent: ProcessContext,
    pub execution: crate::process_executions::ExecutionPublic,
}
impl FromRequestParts<AppState> for ProcessExecutionContext {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let parent = ProcessContext::from_request_parts(parts, state).await?;
        let Path(route) = Path::<ExecutionRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let id = route
            .execution_id
            .parse::<Uuid>()
            .map_err(|_| not_found(parts))?;
        let scope = crate::process_executions::ExecutionScope::of(&parent);
        let execution = crate::process_executions::find_accessible(&state.pool, scope, id)
            .await
            .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?
            .ok_or_else(|| not_found(parts))?;
        Ok(Self { parent, execution })
    }
}

#[derive(Deserialize)]
struct TimeSessionRoute {
    time_session_id: String,
}

/// Time session context (STEP 21D, ADR 0019): the FULL
/// Org→…→Process→Execution chain first, then the session scoped to that
/// exact execution. A session id alone never resolves anything;
/// wrong-parent, foreign and unknown sessions are one uniform 404.
/// Eligibility only; mutations re-prove everything in-transaction.
pub struct TimeSessionContext {
    pub parent: ProcessExecutionContext,
    pub session: crate::time_sessions::TimeSessionPublic,
}
impl FromRequestParts<AppState> for TimeSessionContext {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let parent = ProcessExecutionContext::from_request_parts(parts, state).await?;
        let Path(route) = Path::<TimeSessionRoute>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found(parts))?;
        let id = route
            .time_session_id
            .parse::<Uuid>()
            .map_err(|_| not_found(parts))?;
        let scope = crate::process_executions::ExecutionScope::of(&parent.parent);
        let session =
            crate::time_sessions::find_accessible(&state.pool, scope, parent.execution.id, id)
                .await
                .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id_of(parts)))?
                .ok_or_else(|| not_found(parts))?;
        Ok(Self { parent, session })
    }
}
