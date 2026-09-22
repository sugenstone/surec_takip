//! Organization membership invitations (First Agent Mission step 14).
//!
//! Security model (ADR 0009):
//! - Raw token: 32 bytes CSPRNG, base64url, delivered ONCE in the creation
//!   response to an authorized inviter (email delivery is a later phase).
//! - Storage: SHA-256 digest only — identical principle to session tokens.
//! - Acceptance boundary: authenticated user + valid token + citext-equal
//!   account email; never OrganizationContext (the recipient is not yet a
//!   member). All acceptance failures return one generic error.
//! - Invitations always grant exactly the tenant's built-in `member` role —
//!   never Owner and never a client-chosen role (privilege-assignment is a
//!   separate future permission).
//! - Acceptance is one atomic transaction: invitation state transition,
//!   membership create-or-reactivate (WITHOUT resurrecting old grants —
//!   reactivation resets to the invitation-intended role only), role
//!   assignment and consumption commit together or not at all.

use crate::{
    AppState, RequestId,
    auth::{ApiJson, CurrentUser},
    context::OrganizationContext,
    error::{ApiError, ErrorCode},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::rbac;

/// Invite new members (organization scope). Enforced by the invitation
/// endpoints; granted explicitly to the built-in Owner role by migration
/// 006 and by new-organization bootstrap.
pub const MEMBERS_INVITE: rbac::PermissionKey = rbac::PermissionKey("members:invite");

const TOKEN_BYTES: usize = 32;
const DEFAULT_TTL_HOURS: i64 = 72;
const MAX_TTL_HOURS: i64 = 24 * 30;

pub fn digest_token(token: &str) -> String {
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

fn generate_token() -> Result<String, ApiError> {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

#[derive(Debug)]
pub enum InvitationError {
    DuplicatePending,
    InvalidTtl,
    DatabaseError,
}

impl std::fmt::Display for InvitationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            InvitationError::DuplicatePending => "a pending invitation already exists",
            InvitationError::InvalidTtl => "invalid expiration",
            InvitationError::DatabaseError => "database operation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for InvitationError {}

#[derive(sqlx::FromRow, Serialize, ToSchema)]
pub struct InvitationPublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub email: String,
    pub invited_by_user_id: Uuid,
    pub expires_at: String,
    pub accepted_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateInvitationRequest {
    pub email: String,
    /// Optional expiration in hours; defaults to 72, bounded to 720.
    pub expires_in_hours: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateInvitationData {
    pub invitation: InvitationPublic,
    /// Raw invitation token. Shown ONCE at creation because email delivery
    /// is not built yet; list endpoints never return it. The inviter is
    /// responsible for delivering it to the recipient over a trusted channel.
    pub token: String,
}

#[derive(Serialize, ToSchema)]
pub struct CreateInvitationResponse {
    pub data: CreateInvitationData,
}

#[derive(Serialize, ToSchema)]
pub struct InvitationListResponse {
    pub data: Vec<InvitationPublic>,
}

/// Route identity by field name (context.rs route-struct pattern): extra
/// path parameters are ignored, so this extractor stays future-safe.
#[derive(serde::Deserialize)]
pub struct RevokeInvitationRoute {
    #[allow(dead_code)]
    organization_id: String,
    invitation_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct AcceptInvitationRequest {
    pub token: String,
}

#[derive(Serialize, ToSchema)]
pub struct AcceptInvitationData {
    pub organization_id: Uuid,
}

#[derive(Serialize, ToSchema)]
pub struct AcceptInvitationResponse {
    pub data: AcceptInvitationData,
}

#[utoipa::path(post,
    path = "/api/v1/organizations/{organization_id}/invitations",
    request_body = CreateInvitationRequest,
    params(("organization_id" = Uuid, Path, description = "Organization id")),
    responses(
        (status = 201, body = CreateInvitationResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 403, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn create_invitation(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: OrganizationContext,
    ApiJson(body): ApiJson<CreateInvitationRequest>,
) -> Result<Response, ApiError> {
    let not_found = || ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone());
    let email = body.email.trim().to_owned();
    let mut fields = serde_json::Map::new();
    if email.is_empty() || !email.contains('@') || email.len() > 254 {
        fields.insert("email".into(), serde_json::json!(["Invalid"]));
    }
    let ttl_hours = body.expires_in_hours.unwrap_or(DEFAULT_TTL_HOURS);
    if !(1..=MAX_TTL_HOURS).contains(&ttl_hours) {
        fields.insert(
            "expires_in_hours".into(),
            serde_json::json!([format!("Must be between 1 and {MAX_TTL_HOURS}")]),
        );
    }
    if !fields.is_empty() {
        return Err(ApiError::invalid_fields(
            serde_json::Value::Object(fields),
            request_id.0,
        ));
    }

    // Eligibility is proven by the context; the permission is evaluated
    // server-side from the resolved organization (never from the body).
    let allowed = rbac::authorize_organization(
        &state.pool,
        context.organization_id,
        context.user_id,
        MEMBERS_INVITE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(rbac::permission_denied(&request_id));
    }

    // A pending invitation for this (tenant, email) replaces the old one:
    // exactly one live token per recipient at any time.
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    sqlx::query(
        "UPDATE invitations SET revoked_at = now(), updated_at = now() \
                 WHERE tenant_id = $1 AND lower(email::text) = lower($2) \
                 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(context.organization_id)
    .bind(&email)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    let token = generate_token()
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, Uuid, String, Option<String>, Option<String>, String)>(
        "INSERT INTO invitations (id, tenant_id, email, token_hash, invited_by_user_id, expires_at) \
         VALUES ($1, $2, $3, $4, $5, now() + ($6 * interval '1 hour')) \
         RETURNING id, tenant_id, email::text, invited_by_user_id, \
         to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), \
         NULL::text, NULL::text, \
         to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
    )
    .bind(Uuid::now_v7())
    .bind(context.organization_id)
    .bind(&email)
    .bind(digest_token(&token))
    .bind(context.user_id)
    .bind(ttl_hours)
    .fetch_one(&mut *transaction)
    .await;
    let row = match row {
        Ok(row) => row,
        Err(error) => {
            // DB detail goes to logs only; the response stays generic.
            tracing::error!(request_id = %request_id.0, error = %error, "invitation_insert_failed");
            let _ = transaction.rollback().await;
            return Err(ApiError::new(ErrorCode::InternalError, request_id.0));
        }
    };
    transaction.commit().await.map_err(|_| not_found())?;
    let (id, org, email_row, invited_by, expires_at, accepted_at, revoked_at, created_at) = row;
    let invitation = InvitationPublic {
        id,
        organization_id: org,
        email: email_row,
        invited_by_user_id: invited_by,
        expires_at,
        accepted_at,
        revoked_at,
        created_at,
    };
    let response = Json(CreateInvitationResponse {
        data: CreateInvitationData { invitation, token },
    });
    Ok((StatusCode::CREATED, response).into_response())
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}/invitations",
    params(("organization_id" = Uuid, Path, description = "Organization id")),
    responses(
        (status = 200, body = InvitationListResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 403, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn list_invitations(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: OrganizationContext,
) -> Result<Json<InvitationListResponse>, ApiError> {
    // Listing invitation metadata is part of the invite capability.
    let allowed = rbac::authorize_organization(
        &state.pool,
        context.organization_id,
        context.user_id,
        MEMBERS_INVITE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(rbac::permission_denied(&request_id));
    }
    let invitations = sqlx::query_as::<_, InvitationPublic>(
        "SELECT id, tenant_id AS organization_id, email::text AS email, invited_by_user_id, \
         to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at, \
         to_char(accepted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS accepted_at, \
         to_char(revoked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS revoked_at, \
         to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at \
         FROM invitations WHERE tenant_id = $1 ORDER BY created_at DESC, id",
    )
    .bind(context.organization_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(InvitationListResponse { data: invitations }))
}

#[utoipa::path(delete,
    path = "/api/v1/organizations/{organization_id}/invitations/{invitation_id}",
    params(
        ("organization_id" = Uuid, Path, description = "Organization id"),
        ("invitation_id" = Uuid, Path, description = "Invitation id"),
    ),
    responses(
        (status = 200, body = crate::auth::AckResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 403, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn revoke_invitation(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    context: OrganizationContext,
    Path(route): Path<RevokeInvitationRoute>,
) -> Result<Json<crate::auth::AckResponse>, ApiError> {
    let not_found = || ApiError::new(ErrorCode::ResourceNotFound, request_id.0.clone());
    let allowed = rbac::authorize_organization(
        &state.pool,
        context.organization_id,
        context.user_id,
        MEMBERS_INVITE,
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if !allowed {
        return Err(rbac::permission_denied(&request_id));
    }
    let Ok(invitation_id) = route.invitation_id.parse::<Uuid>() else {
        return Err(not_found());
    };
    // Scoped by the resolved tenant: a foreign tenant's invitation id is
    // indistinguishable from nonexistent.
    let result = sqlx::query(
        "UPDATE invitations SET revoked_at = now(), updated_at = now() \
         WHERE id = $1 AND tenant_id = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(invitation_id)
    .bind(context.organization_id)
    .execute(&state.pool)
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    if result.rows_affected() == 0 {
        // Idempotent: an already-revoked/accepted invitation in this tenant
        // reports success; anything else is not found.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM invitations WHERE id = $1 AND tenant_id = $2)",
        )
        .bind(invitation_id)
        .bind(context.organization_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
        if !exists {
            return Err(not_found());
        }
    }
    Ok(Json(crate::auth::AckResponse {
        data: crate::auth::AckData {},
    }))
}

#[utoipa::path(post,
    path = "/api/v1/invitations/accept",
    request_body = AcceptInvitationRequest,
    responses(
        (status = 200, body = AcceptInvitationResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn accept_invitation(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    ApiJson(body): ApiJson<AcceptInvitationRequest>,
) -> Result<Json<AcceptInvitationResponse>, ApiError> {
    let token = body.token.trim().to_owned();
    if token.is_empty() || token.len() > 128 {
        return Err(ApiError::invalid_fields(
            serde_json::json!({ "token": ["Required"] }),
            request_id.0,
        ));
    }
    match accept(&state.pool, &token, &current.user).await {
        Ok(organization_id) => Ok(Json(AcceptInvitationResponse {
            data: AcceptInvitationData { organization_id },
        })),
        // Every acceptance failure is one generic error: enumeration of
        // invitation state (expired/revoked/consumed/wrong-email/unknown)
        // must not be possible through observable differences.
        Err(_) => Err(ApiError::new(ErrorCode::InvitationInvalid, request_id.0)),
    }
}

/// Atomic acceptance. Every failure path rolls back completely: no partial
/// membership, no consumed token without membership, no roleless member.
async fn accept(pool: &PgPool, token: &str, user: &crate::users::UserRow) -> Result<Uuid, ()> {
    let mut transaction = pool.begin().await.map_err(|_| ())?;

    // Lock the invitation row by token digest FOR UPDATE so two concurrent
    // acceptances serialize; the conditional accepted_at update below makes
    // the second attempt fail deterministically.
    let invitation = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, email::text FROM invitations WHERE token_hash = $1 FOR UPDATE",
    )
    .bind(digest_token(token))
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ())?
    .ok_or(())?;

    // Identity proof: the authenticated account's email must equal the
    // invited email (citext semantics both sides via lower comparison).
    if invitation.1.to_lowercase() != user.email.to_lowercase() {
        return Err(());
    }

    // One-time consumption: pending -> accepted for exactly this attempt.
    let consumed = sqlx::query(
        "UPDATE invitations SET accepted_at = now(), accepted_by_user_id = $2, updated_at = now() \
         WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(invitation.0)
    .bind(user.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ())?;
    if consumed.rows_affected() != 1 {
        return Err(());
    }

    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM invitations WHERE id = $1")
        .bind(invitation.0)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| ())?;

    // The tenant must still be live; no membership into a deleted tenant.
    let org_alive: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM organizations WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(tenant_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| ())?;
    if !org_alive {
        return Err(());
    }

    // Determine whether this is a reactivation of a non-active membership.
    // Only THEN are dangling role assignments cleared (resurrection
    // prevention); an already-active member keeps existing grants untouched
    // — the invitation must never downgrade or duplicate privileges.
    let was_active: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM organization_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL)",
    )
    .bind(tenant_id)
    .bind(user.id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| ())?;
    if !was_active {
        sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
            .bind(tenant_id)
            .bind(user.id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| ())?;
    }
    let inserted = sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now()) \
         ON CONFLICT (tenant_id, user_id) DO UPDATE \
         SET status = 'active', deleted_at = NULL, joined_at = now(), updated_at = now()",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(user.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ())?;
    if inserted.rows_affected() != 1 {
        return Err(());
    }

    // Assign exactly the tenant's built-in member role (stable system
    // identity, resolved by tenant + is_system + name key — never a display
    // label and never a client-chosen role).
    let member_role: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM roles WHERE tenant_id = $1 AND is_system AND name = 'member' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ())?;
    let Some(member_role) = member_role else {
        return Err(());
    };
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, NULL) ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(user.id)
    .bind(member_role)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ())?;

    transaction.commit().await.map_err(|_| ())?;
    Ok(tenant_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_digest_is_deterministic_and_not_the_token() {
        let token = generate_token().unwrap_or_else(|_| panic!("generation must succeed"));
        assert_eq!(token.len(), 43);
        let digest = digest_token(&token);
        assert_eq!(digest, digest_token(&token));
        assert_ne!(digest, token);
        assert_ne!(generate_token().unwrap_or_else(|_| panic!()), token);
    }
}
