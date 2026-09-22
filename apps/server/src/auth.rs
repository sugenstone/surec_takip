use crate::{
    AppState, RequestId,
    error::{ApiError, ErrorCode},
    organizations,
    users::{self, AuthenticatedSession, USER_STATUS_ACTIVE, UserRow},
};
use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, FromRequest, FromRequestParts, Request, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use utoipa::ToSchema;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "platform_session";
// 256 bits of entropy; base64url keeps the value cookie-safe without quoting.
const SESSION_TOKEN_BYTES: usize = 32;

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

pub fn generate_session_token() -> Result<String, &'static str> {
    let mut bytes = [0u8; SESSION_TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

// Only the SHA-256 digest of the opaque token is ever stored or compared;
// the raw token exists solely inside the client's HttpOnly cookie.
pub fn digest_token(token: &str) -> String {
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

pub fn session_cookie(token: &str, max_age_seconds: u64, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn cleared_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; \
         Expires=Thu, 01 Jan 1970 00:00:00 GMT{}",
        if secure { "; Secure" } else { "" }
    )
}

// Minimal cookie header parsing: values are base64url tokens, never quoted.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|header| header.split(';'))
        .map(|pair| pair.trim())
        .filter_map(|pair| pair.split_once('='))
        .find(|(cookie_name, _)| *cookie_name == name)
        .map(|(_, value)| value.to_owned())
}

// ---------------------------------------------------------------------------
// Request/response contracts
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserPublic {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub locale: Option<String>,
    pub timezone: Option<String>,
}

impl From<&UserRow> for UserPublic {
    fn from(user: &UserRow) -> Self {
        Self {
            id: user.id,
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            locale: user.locale.clone(),
            timezone: user.timezone.clone(),
        }
    }
}

// The user's own membership view; role_summary fills in during the RBAC phase.
#[derive(Serialize, ToSchema)]
pub struct OrganizationSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role_summary: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct LoginData {
    pub user: UserPublic,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub data: LoginData,
}

#[derive(Serialize, ToSchema)]
pub struct MeData {
    pub user: UserPublic,
    pub organizations: Vec<OrganizationSummary>,
}

#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    pub data: MeData,
}

#[derive(Serialize, ToSchema)]
pub struct AckData {}

#[derive(Serialize, ToSchema)]
pub struct AckResponse {
    pub data: AckData,
}

// ---------------------------------------------------------------------------
// Extractors
// ---------------------------------------------------------------------------

// Structured JSON extraction: malformed bodies become the standard envelope
// without echoing any part of the rejected body back to the client.
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let request_id = request
            .extensions()
            .get::<RequestId>()
            .map(|id| id.0.clone())
            .unwrap_or_default();
        match axum::Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(ApiJson(value)),
            Err(_) => Err(ApiError::malformed_json(request_id)),
        }
    }
}

pub struct CurrentUser {
    pub session_id: Uuid,
    pub user: UserRow,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .map(|id| id.0.clone())
            .unwrap_or_default();
        let Some(token) = cookie_value(&parts.headers, SESSION_COOKIE) else {
            return Err(ApiError::new(ErrorCode::AuthRequired, request_id));
        };
        match users::find_active_session(&state.pool, &digest_token(&token)).await {
            Ok(Some(AuthenticatedSession { session_id, user })) => {
                Ok(CurrentUser { session_id, user })
            }
            Ok(None) => Err(ApiError::new(ErrorCode::AuthRequired, request_id)),
            Err(_) => Err(ApiError::new(ErrorCode::InternalError, request_id)),
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, body = LoginResponse),
        (status = 400, body = crate::error::ErrorEnvelope),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<LoginRequest>,
) -> Result<Response, ApiError> {
    validate_login(&body, &request_id.0)?;
    let stored = users::find_by_email(&state.pool, body.email.trim())
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    let credentials_ok = match &stored {
        Some(user) => {
            // Password verification runs before the status check so disabled
            // accounts cannot be distinguished by response timing.
            let password_ok = user
                .password_hash
                .as_deref()
                .is_some_and(|hash| state.passwords.verify(&body.password, hash));
            password_ok && user.status == USER_STATUS_ACTIVE
        }
        None => {
            // Unknown email: burn the same verification cost so timing does
            // not reveal whether the account exists.
            let _ = state.passwords.verify(&body.password, &state.dummy_hash);
            false
        }
    };
    let Some(user) = stored.filter(|_| credentials_ok) else {
        return Err(ApiError::new(
            ErrorCode::AuthInvalidCredentials,
            request_id.0,
        ));
    };
    let token = generate_session_token()
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    users::insert_session(
        &state.pool,
        &users::NewSession {
            user_id: user.id,
            token_hash: digest_token(&token),
            ttl_seconds: state.auth.session_ttl.as_secs() as i64,
            ip: Some(address.ip().to_string()),
            user_agent: headers
                .get(header::USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
        },
    )
    .await
    .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    let cookie = session_cookie(
        &token,
        state.auth.session_ttl.as_secs(),
        state.auth.session_cookie_secure,
    );
    let mut response = Json(LoginResponse {
        data: LoginData {
            user: UserPublic::from(&user),
        },
    })
    .into_response();
    insert_cookie(&mut response, cookie, &request_id.0)?;
    Ok(response)
}

#[utoipa::path(post,
    path = "/api/v1/auth/logout",
    responses(
        (status = 200, body = AckResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn logout(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Response, ApiError> {
    users::revoke_session(&state.pool, current.session_id)
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0.clone()))?;
    let cookie = cleared_session_cookie(state.auth.session_cookie_secure);
    let mut response = Json(AckResponse { data: AckData {} }).into_response();
    insert_cookie(&mut response, cookie, &request_id.0)?;
    Ok(response)
}

#[utoipa::path(get,
    path = "/api/v1/auth/me",
    responses(
        (status = 200, body = MeResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn me(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Json<MeResponse>, ApiError> {
    // Only organizations the user actively belongs to; foreign organizations
    // never enter the response (visibility filter in organizations module).
    let organizations = organizations::visible_for_user(&state.pool, current.user.id)
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(MeResponse {
        data: MeData {
            user: UserPublic::from(&current.user),
            organizations: organizations
                .into_iter()
                .map(|organization| OrganizationSummary {
                    id: organization.id,
                    name: organization.name,
                    slug: organization.slug,
                    role_summary: Vec::new(),
                })
                .collect(),
        },
    }))
}

fn insert_cookie(
    response: &mut Response,
    cookie: String,
    request_id: &str,
) -> Result<(), ApiError> {
    let value = cookie
        .parse()
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.to_owned()))?;
    response.headers_mut().insert(header::SET_COOKIE, value);
    Ok(())
}

fn validate_login(body: &LoginRequest, request_id: &str) -> Result<(), ApiError> {
    let mut fields = serde_json::Map::new();
    let email = body.email.trim();
    if email.is_empty() {
        fields.insert("email".into(), serde_json::json!(["Required"]));
    } else if email.len() > 254 {
        fields.insert("email".into(), serde_json::json!(["Too long"]));
    }
    if body.password.is_empty() {
        fields.insert("password".into(), serde_json::json!(["Required"]));
    } else if body.password.len() > 1024 {
        fields.insert("password".into(), serde_json::json!(["Too long"]));
    }
    if fields.is_empty() {
        Ok(())
    } else {
        Err(ApiError::invalid_fields(
            serde_json::Value::Object(fields),
            request_id.to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_carries_security_attributes() {
        let cookie = session_cookie("token-value", 43_200, true);
        assert!(cookie.starts_with("platform_session=token-value;"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Max-Age=43200"));
        assert!(cookie.contains("Secure"));
        let insecure = session_cookie("token-value", 60, false);
        assert!(!insecure.contains("Secure"));
    }

    #[test]
    fn cleared_cookie_expires_immediately() {
        let cookie = cleared_session_cookie(false);
        assert!(cookie.starts_with("platform_session=;"));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("Expires=Thu, 01 Jan 1970"));
        assert!(cleared_session_cookie(true).contains("Secure"));
    }

    #[test]
    fn token_digest_is_deterministic_and_not_the_token() {
        let token =
            generate_session_token().unwrap_or_else(|_| panic!("token generation must succeed"));
        assert_eq!(token.len(), 43);
        assert!(
            token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        );
        let first = digest_token(&token);
        assert_eq!(first, digest_token(&token));
        assert_ne!(first, token);
        let other = generate_session_token().unwrap_or_else(|_| panic!());
        assert_ne!(token, other);
        assert_ne!(first, digest_token(&other));
    }

    #[test]
    fn cookie_header_parsing_finds_only_the_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            "other=a; platform_session=abc123; last=z"
                .parse()
                .unwrap_or_else(|_| panic!()),
        );
        assert_eq!(
            cookie_value(&headers, SESSION_COOKIE).as_deref(),
            Some("abc123")
        );
        assert_eq!(cookie_value(&headers, "missing"), None);
    }
}
