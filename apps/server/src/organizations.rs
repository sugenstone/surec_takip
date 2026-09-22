use crate::{
    AppState, RequestId,
    auth::ApiJson,
    auth::CurrentUser,
    error::{ApiError, ErrorCode},
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

pub const ORGANIZATION_STATUS_ACTIVE: &str = "active";
pub const MEMBERSHIP_STATUS_ACTIVE: &str = "active";
pub const MEMBERSHIP_STATUS_DELETED: &str = "deleted";

// Visibility rule shared by every read path: the membership must be active
// and not soft-deleted, and the organization must not be soft-deleted.
const VISIBLE_MEMBERSHIP_FILTER: &str =
    "m.status = 'active' AND m.deleted_at IS NULL AND o.deleted_at IS NULL";

const ORGANIZATION_COLUMNS: &str = "o.id, o.name, o.slug::text AS slug, o.status, o.default_timezone, o.default_locale, o.default_currency";

const VISIBLE_ORGANIZATIONS_SQL: &str = "SELECT {COLUMNS} \
     FROM organizations o JOIN organization_memberships m ON m.tenant_id = o.id \
     WHERE m.user_id = $1 AND {FILTER} ORDER BY o.created_at, o.id";

#[derive(Debug)]
pub enum OrganizationError {
    SlugAlreadyTaken,
    DatabaseError,
}

impl std::fmt::Display for OrganizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            OrganizationError::SlugAlreadyTaken => "slug already exists",
            OrganizationError::DatabaseError => "database operation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OrganizationError {}

#[derive(Clone, sqlx::FromRow)]
pub struct OrganizationRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub default_timezone: String,
    pub default_locale: Option<String>,
    pub default_currency: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct MembershipRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub status: String,
    pub joined_at: Option<String>,
}

pub struct NewOrganization {
    pub name: String,
    pub slug: String,
}

// ---------------------------------------------------------------------------
// Slug handling
// ---------------------------------------------------------------------------

const NAME_MAX_CHARS: usize = 200;
const SLUG_MAX_CHARS: usize = 64;

// Turkish-aware transliteration keeps derived slugs readable for the default
// locale; the map is intentionally small and domain-independent.
fn transliterate(character: char) -> char {
    match character {
        'ç' | 'Ç' => 'c',
        'ğ' | 'Ğ' => 'g',
        'ı' | 'İ' | 'I' => 'i',
        'ö' | 'Ö' => 'o',
        'ş' | 'Ş' => 's',
        'ü' | 'Ü' => 'u',
        'â' | 'Â' => 'a',
        'ê' | 'Ê' => 'e',
        'ô' | 'Ô' => 'o',
        'û' | 'Û' => 'u',
        _ => character,
    }
}

/// Lowercase, transliterated, `[a-z0-9-]` slug derived from free text.
/// Returns None when no usable characters remain.
pub fn slug_from_text(text: &str) -> Option<String> {
    let mut slug = String::new();
    for character in text.chars().map(transliterate) {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    // Hyphens are only pushed after an alphanumeric, so only the length clamp
    // can leave a trailing hyphen.
    let mut slug: String = slug.chars().take(SLUG_MAX_CHARS).collect();
    while slug.ends_with('-') {
        slug.pop();
    }
    (!slug.is_empty()).then_some(slug)
}

pub fn validate_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= SLUG_MAX_CHARS
        && slug.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

async fn insert_organization(
    connection: &mut PgConnection,
    organization: &NewOrganization,
) -> Result<OrganizationRow, OrganizationError> {
    let row = sqlx::query_as::<_, OrganizationRow>(
        "INSERT INTO organizations (id, name, slug, status, default_timezone) \
         VALUES ($1, $2, $3, $4, 'UTC') \
         RETURNING id, name, slug::text AS slug, status, default_timezone, \
         default_locale, default_currency",
    )
    .bind(Uuid::now_v7())
    .bind(&organization.name)
    .bind(&organization.slug)
    .bind(ORGANIZATION_STATUS_ACTIVE)
    .fetch_one(connection)
    .await;
    match row {
        Ok(row) => Ok(row),
        // 23505 = unique_violation on organizations_slug_key.
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            Err(OrganizationError::SlugAlreadyTaken)
        }
        Err(_) => Err(OrganizationError::DatabaseError),
    }
}

async fn insert_membership(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<MembershipRow, OrganizationError> {
    let row = sqlx::query_as::<_, MembershipRow>(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now()) \
         RETURNING id, tenant_id, user_id, status, \
         to_char(joined_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS joined_at",
    )
    .bind(Uuid::now_v7())
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(connection)
    .await;
    match row {
        Ok(row) => Ok(row),
        Err(_) => Err(OrganizationError::DatabaseError),
    }
}

/// Organization and creator membership commit atomically: a half-created
/// tenant must never exist.
pub async fn create_with_membership(
    pool: &PgPool,
    organization: &NewOrganization,
    creator: Uuid,
) -> Result<(OrganizationRow, MembershipRow), OrganizationError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| OrganizationError::DatabaseError)?;
    let organization = insert_organization(&mut transaction, organization).await?;
    let membership = insert_membership(&mut transaction, organization.id, creator).await?;
    transaction
        .commit()
        .await
        .map_err(|_| OrganizationError::DatabaseError)?;
    Ok((organization, membership))
}

/// Organizations visible to a user: active, non-deleted membership joined to
/// a non-deleted organization, in deterministic creation order. Shared by the
/// organization list endpoint and /auth/me.
pub async fn visible_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<OrganizationRow>, OrganizationError> {
    let sql = VISIBLE_ORGANIZATIONS_SQL
        .replace("{COLUMNS}", ORGANIZATION_COLUMNS)
        .replace("{FILTER}", VISIBLE_MEMBERSHIP_FILTER);
    sqlx::query_as::<_, OrganizationRow>(&sql)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(|_| OrganizationError::DatabaseError)
}

/// Single-organization access check. Every miss (unknown id, non-member,
/// deleted membership, deleted organization) returns None so responses stay
/// indistinguishable.
pub async fn find_for_member(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<Option<OrganizationRow>, OrganizationError> {
    let sql = format!(
        "SELECT {ORGANIZATION_COLUMNS} \
         FROM organizations o JOIN organization_memberships m ON m.tenant_id = o.id \
         WHERE o.id = $1 AND m.user_id = $2 AND {VISIBLE_MEMBERSHIP_FILTER}"
    );
    sqlx::query_as::<_, OrganizationRow>(&sql)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(executor)
        .await
        .map_err(|_| OrganizationError::DatabaseError)
}

// ---------------------------------------------------------------------------
// HTTP contracts
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MembershipPublic {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub status: String,
    pub joined_at: Option<String>,
}

impl From<MembershipRow> for MembershipPublic {
    fn from(row: MembershipRow) -> Self {
        Self {
            id: row.id,
            organization_id: row.tenant_id,
            user_id: row.user_id,
            status: row.status,
            joined_at: row.joined_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct OrganizationPublic {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub default_timezone: String,
    pub default_locale: Option<String>,
    pub default_currency: Option<String>,
}

impl From<OrganizationRow> for OrganizationPublic {
    fn from(row: OrganizationRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            slug: row.slug,
            status: row.status,
            default_timezone: row.default_timezone,
            default_locale: row.default_locale,
            default_currency: row.default_currency,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct CreateOrganizationData {
    pub organization: OrganizationPublic,
    pub membership: MembershipPublic,
}

#[derive(Serialize, ToSchema)]
pub struct CreateOrganizationResponse {
    pub data: CreateOrganizationData,
}

#[derive(Serialize, ToSchema)]
pub struct OrganizationListResponse {
    pub data: Vec<OrganizationPublic>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post,
    path = "/api/v1/organizations",
    request_body = CreateOrganizationRequest,
    responses(
        (status = 201, body = CreateOrganizationResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 422, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn create_organization(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    ApiJson(body): ApiJson<CreateOrganizationRequest>,
) -> Result<Response, ApiError> {
    let name = body.name.trim().to_owned();
    let mut fields = serde_json::Map::new();
    if name.is_empty() {
        fields.insert("name".into(), serde_json::json!(["Required"]));
    } else if name.chars().count() > NAME_MAX_CHARS {
        fields.insert("name".into(), serde_json::json!(["Too long"]));
    }
    // Explicit slugs and name-derived slugs go through the same
    // normalization; rejection blames the field the user actually supplied.
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

    let result = create_with_membership(
        &state.pool,
        &NewOrganization { name, slug },
        current.user.id,
    )
    .await;
    match result {
        Ok((organization, membership)) => {
            let body = Json(CreateOrganizationResponse {
                data: CreateOrganizationData {
                    organization: OrganizationPublic::from(organization),
                    membership: MembershipPublic::from(membership),
                },
            });
            Ok((StatusCode::CREATED, body).into_response())
        }
        Err(OrganizationError::SlugAlreadyTaken) => Err(ApiError::invalid_fields(
            serde_json::json!({ "slug": ["Already taken"] }),
            request_id.0,
        )),
        Err(OrganizationError::DatabaseError) => {
            Err(ApiError::new(ErrorCode::InternalError, request_id.0))
        }
    }
}

#[utoipa::path(get,
    path = "/api/v1/organizations",
    responses(
        (status = 200, body = OrganizationListResponse),
        (status = 401, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn list_organizations(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Json<OrganizationListResponse>, ApiError> {
    let organizations = visible_for_user(&state.pool, current.user.id)
        .await
        .map_err(|_| ApiError::new(ErrorCode::InternalError, request_id.0))?;
    Ok(Json(OrganizationListResponse {
        data: organizations
            .into_iter()
            .map(OrganizationPublic::from)
            .collect(),
    }))
}

#[utoipa::path(get,
    path = "/api/v1/organizations/{organization_id}",
    params(("organization_id" = Uuid, Path, description = "Organization id")),
    responses(
        (status = 200, body = OrganizationPublic),
        (status = 401, body = crate::error::ErrorEnvelope),
        (status = 404, body = crate::error::ErrorEnvelope),
    )
)]
pub async fn get_organization(
    context: crate::context::OrganizationContext,
) -> Result<Json<OrganizationPublic>, ApiError> {
    // Authentication, membership and malformed/unknown-id semantics are all
    // resolved by the OrganizationContext extractor (single boundary).
    Ok(Json(OrganizationPublic::from(context.organization)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_derivation_handles_turkish_and_separators() {
        assert_eq!(
            slug_from_text("Çelik İş A.Ş.").as_deref(),
            Some("celik-is-a-s")
        );
        assert_eq!(
            slug_from_text("  Meridyen   Ops  ").as_deref(),
            Some("meridyen-ops")
        );
        assert_eq!(slug_from_text("ACME").as_deref(), Some("acme"));
        assert_eq!(slug_from_text("a").as_deref(), Some("a"));
        assert_eq!(slug_from_text("---***---"), None);
        assert_eq!(
            slug_from_text("İstanbul Üretim").as_deref(),
            Some("istanbul-uretim")
        );
        let long = "a".repeat(100);
        assert_eq!(
            slug_from_text(&long).map(|slug| slug.len()),
            Some(SLUG_MAX_CHARS)
        );
    }

    #[test]
    fn slug_validation_rejects_invalid_shapes() {
        assert!(validate_slug("acme"));
        assert!(validate_slug("acme-ops-2"));
        assert!(!validate_slug(""));
        assert!(!validate_slug("-acme"));
        assert!(!validate_slug("acme-"));
        assert!(!validate_slug("acme--ops"));
        assert!(!validate_slug("Acme"));
        assert!(!validate_slug("acme_ops"));
        assert!(!validate_slug(&"a".repeat(SLUG_MAX_CHARS + 1)));
    }
}
