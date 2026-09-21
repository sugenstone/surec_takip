use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

pub const USER_STATUS_ACTIVE: &str = "active";
pub const USER_STATUS_DISABLED: &str = "disabled";

#[derive(sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: Option<String>,
    pub display_name: String,
    pub email_verified_at: Option<String>,
    pub status: String,
    pub locale: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug)]
pub enum StoreError {
    EmailAlreadyExists,
    DatabaseError,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            StoreError::EmailAlreadyExists => "email already exists",
            StoreError::DatabaseError => "database operation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for StoreError {}

pub struct NewUser {
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub locale: Option<String>,
}

// citext comparison must stay case-insensitive while binding parameters as
// plain text for sqlx compatibility, hence lower(...) on both sides.
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRow>, StoreError> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, email::text AS email, password_hash, display_name, \
         email_verified_at::text AS email_verified_at, status, locale, timezone \
         FROM users WHERE lower(email::text) = lower($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(|_| StoreError::DatabaseError)
}

pub async fn insert_user(pool: &PgPool, user: &NewUser) -> Result<UserRow, StoreError> {
    let id = Uuid::now_v7();
    // 23505 = unique_violation on users_email_key; anything else stays generic.
    let result = sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (id, email, password_hash, display_name, email_verified_at, status, locale, timezone) \
         VALUES ($1, $2, $3, $4, NULL, $5, $6, NULL) \
         RETURNING id, email::text AS email, password_hash, display_name, \
         email_verified_at::text AS email_verified_at, status, locale, timezone",
    )
    .bind(id)
    .bind(&user.email)
    .bind(&user.password_hash)
    .bind(&user.display_name)
    .bind(USER_STATUS_ACTIVE)
    .bind(&user.locale)
    .fetch_one(pool)
    .await;
    match result {
        Ok(row) => Ok(row),
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            Err(StoreError::EmailAlreadyExists)
        }
        Err(_) => Err(StoreError::DatabaseError),
    }
}

pub struct NewSession {
    pub user_id: Uuid,
    pub token_hash: String,
    pub ttl_seconds: i64,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

// The database clock owns expiry: now() + ttl avoids app/host clock skew.
pub async fn insert_session(pool: &PgPool, session: &NewSession) -> Result<(), StoreError> {
    let ip_metadata: Option<JsonValue> = session
        .ip
        .as_ref()
        .map(|ip| serde_json::json!({ "ip": ip }));
    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, expires_at, ip_metadata, user_agent) \
         VALUES ($1, $2, $3, now() + make_interval(secs => $4), $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(session.user_id)
    .bind(&session.token_hash)
    .bind(session.ttl_seconds)
    .bind(ip_metadata)
    // Keep user_agent storage bounded; it is metadata only, never security input.
    .bind(
        session
            .user_agent
            .as_ref()
            .map(|value| value.chars().take(512).collect::<String>()),
    )
    .execute(pool)
    .await
    .map_err(|_| StoreError::DatabaseError)?;
    Ok(())
}

pub struct AuthenticatedSession {
    pub session_id: Uuid,
    pub user: UserRow,
}

// Single authoritative lookup: digest match + not revoked + not expired +
// owning user still active. Any miss yields None (indistinguishable 401).
// password_hash is deliberately not fetched: session checks never need it.
#[derive(sqlx::FromRow)]
struct ActiveSessionRow {
    session_id: Uuid,
    id: Uuid,
    email: String,
    display_name: String,
    email_verified_at: Option<String>,
    status: String,
    locale: Option<String>,
    timezone: Option<String>,
}

pub async fn find_active_session(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<AuthenticatedSession>, StoreError> {
    let row = sqlx::query_as::<_, ActiveSessionRow>(
        "SELECT s.id AS session_id, u.id, u.email::text AS email, u.display_name, \
         u.email_verified_at::text AS email_verified_at, u.status, u.locale, u.timezone \
         FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now() \
         AND u.status = $2",
    )
    .bind(token_hash)
    .bind(USER_STATUS_ACTIVE)
    .fetch_optional(pool)
    .await
    .map_err(|_| StoreError::DatabaseError)?;
    Ok(row.map(|row| {
        let ActiveSessionRow {
            session_id,
            id,
            email,
            display_name,
            email_verified_at,
            status,
            locale,
            timezone,
        } = row;
        AuthenticatedSession {
            session_id,
            user: UserRow {
                id,
                email,
                password_hash: None,
                display_name,
                email_verified_at,
                status,
                locale,
                timezone,
            },
        }
    }))
}

pub async fn revoke_session(pool: &PgPool, session_id: Uuid) -> Result<bool, StoreError> {
    let result =
        sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
            .bind(session_id)
            .execute(pool)
            .await
            .map_err(|_| StoreError::DatabaseError)?;
    Ok(result.rows_affected() == 1)
}
