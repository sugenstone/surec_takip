use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState,
    auth::{SESSION_COOKIE, digest_token},
    config::AuthConfig,
    password::PasswordService,
    router,
    users::{self, NewUser, StoreError},
};
use sqlx::PgPool;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower::ServiceExt;
use uuid::Uuid;

const TEST_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51_111);

fn state(pool: &PgPool) -> AppState {
    AppState::new(pool.clone(), AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message))
}

async fn create_user(pool: &PgPool, email: &str, password: &str, display_name: &str) -> Uuid {
    let service = PasswordService::new(&AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message));
    let user = users::insert_user(
        pool,
        &NewUser {
            email: email.to_owned(),
            password_hash: service
                .hash(password)
                .unwrap_or_else(|_| panic!("hashing must succeed")),
            display_name: display_name.to_owned(),
            locale: None,
        },
    )
    .await
    .unwrap_or_else(|_| panic!("test user must be creatable"));
    user.id
}

async fn login(pool: &PgPool, email: &str, password: &str) -> axum::response::Response {
    router(state(pool))
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .header(header::USER_AGENT, "integration-test/1.0")
                .extension(ConnectInfo(TEST_ADDRESS))
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": password }).to_string(),
                ))
                .unwrap_or_else(|_| panic!("login request must build")),
        )
        .await
        .unwrap_or_else(|_| panic!("login must respond"))
}

fn session_token(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie| cookie.split(';').next())
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_owned())
        .unwrap_or_else(|| panic!("login must set the session cookie"))
}

async fn get(pool: &PgPool, uri: &str, cookie: Option<&str>) -> axum::response::Response {
    let mut builder = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, format!("{SESSION_COOKIE}={cookie}"));
    }
    router(state(pool))
        .oneshot(
            builder
                .body(Body::empty())
                .unwrap_or_else(|_| panic!("request must build")),
        )
        .await
        .unwrap_or_else(|_| panic!("request must respond"))
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .unwrap_or_else(|_| panic!("body must be readable"));
    serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("body must be JSON"))
}

#[sqlx::test(migrations = "../../migrations")]
async fn login_success_sets_protected_cookie_and_returns_user_without_secrets(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    create_user(
        &pool,
        "Admin@Example.test",
        "correct-password",
        "Test Admin",
    )
    .await;
    let response = login(&pool, "admin@example.test", "correct-password").await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie_header = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_else(|| panic!("login must set a cookie"))
        .to_owned();
    for attribute in ["HttpOnly", "SameSite=Lax", "Path=/", "Max-Age=3600"] {
        assert!(cookie_header.contains(attribute), "missing {attribute}");
    }
    assert!(
        !cookie_header.contains("Secure"),
        "test config disables Secure"
    );

    // The raw token must never exist in the database, only its digest.
    let token = session_token(&response);
    let stored: Option<String> =
        sqlx::query_scalar("SELECT token_hash FROM sessions WHERE token_hash = $1")
            .bind(&token)
            .fetch_optional(&pool)
            .await?;
    assert!(stored.is_none(), "raw session token must not be stored");
    let digest_row: Option<String> =
        sqlx::query_scalar("SELECT token_hash FROM sessions WHERE token_hash = $1")
            .bind(digest_token(&token))
            .fetch_optional(&pool)
            .await?;
    assert!(digest_row.is_some(), "digest of the token must be stored");

    let body = body_json(response).await;
    assert_eq!(body["data"]["user"]["email"], "Admin@Example.test");
    assert_eq!(body["data"]["user"]["display_name"], "Test Admin");
    let raw = body.to_string();
    assert!(!raw.contains("correct-password"));
    assert!(!raw.contains("argon2"));
    assert!(!raw.contains("password_hash"));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_email_and_wrong_password_are_indistinguishable(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    create_user(&pool, "known@example.test", "right-password", "Known").await;
    let mut responses = Vec::new();
    for (email, password) in [
        ("ghost@example.test", "right-password"),
        ("known@example.test", "wrong-password"),
    ] {
        let response = login(&pool, email, password).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        responses.push(body_json(response).await);
    }
    let first = responses[0].clone();
    let second = responses[1].clone();
    assert_eq!(first["error"]["code"], "AUTH_INVALID_CREDENTIALS");
    assert_eq!(first["error"]["code"], second["error"]["code"]);
    assert_eq!(first["error"]["message"], second["error"]["message"]);
    assert_eq!(first["error"]["details"], second["error"]["details"]);
    let raw = serde_json::to_string(&second).unwrap_or_default();
    assert!(!raw.contains("right-password"));
    assert!(!raw.contains("wrong-password"));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn disabled_user_login_fails_with_the_generic_error(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    create_user(&pool, "disabled@example.test", "their-password", "Disabled").await;
    sqlx::query("UPDATE users SET status = 'disabled' WHERE email = 'disabled@example.test'")
        .execute(&pool)
        .await?;
    let response = login(&pool, "disabled@example.test", "their-password").await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "AUTH_INVALID_CREDENTIALS");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn session_expiry_follows_the_configured_ttl(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    create_user(&pool, "ttl@example.test", "password", "Ttl").await;
    let response = login(&pool, "ttl@example.test", "password").await;
    assert_eq!(response.status(), StatusCode::OK);
    let seconds: f64 = sqlx::query_scalar(
        "SELECT extract(epoch from (expires_at - created_at))::float8 FROM sessions",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        (seconds - 3600.0).abs() < 1.0,
        "ttl must match config (3600s)"
    );
    let _ = response;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn me_roundtrip_requires_a_valid_session(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    create_user(&pool, "me@example.test", "password", "Me User").await;

    let anonymous = get(&pool, "/api/v1/auth/me", None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let tampered = get(&pool, "/api/v1/auth/me", Some("forged-token-value")).await;
    assert_eq!(tampered.status(), StatusCode::UNAUTHORIZED);

    let login_response = login(&pool, "me@example.test", "password").await;
    let token = session_token(&login_response);
    let authenticated = get(&pool, "/api/v1/auth/me", Some(&token)).await;
    assert_eq!(authenticated.status(), StatusCode::OK);
    let body = body_json(authenticated).await;
    assert_eq!(body["data"]["user"]["display_name"], "Me User");
    assert_eq!(
        body["data"]["organizations"].as_array().map(Vec::len),
        Some(0)
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_sessions_are_rejected(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let user_id = create_user(&pool, "expired@example.test", "password", "Expired").await;
    let token = platform_server::auth::generate_session_token()
        .unwrap_or_else(|_| panic!("token generation must succeed"));
    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, created_at, expires_at) \
         VALUES ($1, $2, $3, now() - interval '2 minutes', now() - interval '1 minute')",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(digest_token(&token))
    .execute(&pool)
    .await?;
    let response = get(&pool, "/api/v1/auth/me", Some(&token)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_revokes_the_session_and_clears_the_cookie(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    create_user(&pool, "logout@example.test", "password", "Logout").await;
    let token = session_token(&login(&pool, "logout@example.test", "password").await);

    let logout = router(state(&pool))
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/logout")
                .method("POST")
                .header(header::COOKIE, format!("{SESSION_COOKIE}={token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(logout.status(), StatusCode::OK);
    let clear_cookie = logout
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_else(|| panic!("logout must clear the cookie"));
    assert!(clear_cookie.contains("Max-Age=0"));

    // The old session must be unusable for both reads and repeated logout.
    assert_eq!(
        get(&pool, "/api/v1/auth/me", Some(&token)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let second_logout = router(state(&pool))
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/logout")
                .method("POST")
                .header(header::COOKIE, format!("{SESSION_COOKIE}={token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(second_logout.status(), StatusCode::UNAUTHORIZED);

    let revoked: Option<String> =
        sqlx::query_scalar("SELECT revoked_at::text FROM sessions WHERE token_hash = $1")
            .bind(digest_token(&token))
            .fetch_optional(&pool)
            .await?;
    assert!(revoked.is_some_and(|value| !value.is_empty()));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_email_in_any_case_is_rejected_by_the_database(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = PasswordService::new(&AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message));
    let hash = service
        .hash("password")
        .unwrap_or_else(|_| panic!("hashing must succeed"));
    users::insert_user(
        &pool,
        &NewUser {
            email: "unique@example.test".to_owned(),
            password_hash: hash.clone(),
            display_name: "First".to_owned(),
            locale: None,
        },
    )
    .await?;
    let duplicate = users::insert_user(
        &pool,
        &NewUser {
            email: "UNIQUE@Example.TEST".to_owned(),
            password_hash: hash,
            display_name: "Second".to_owned(),
            locale: None,
        },
    )
    .await;
    assert!(matches!(duplicate, Err(StoreError::EmailAlreadyExists)));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE lower(email::text) = 'unique@example.test'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_duplicate_creation_has_a_single_winner(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = PasswordService::new(&AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message));
    let hash = service
        .hash("password")
        .unwrap_or_else(|_| panic!("hashing must succeed"));
    let first_user = NewUser {
        email: "race@example.test".to_owned(),
        password_hash: hash.clone(),
        display_name: "Racer".to_owned(),
        locale: None,
    };
    let second_user = NewUser {
        email: "race@example.test".to_owned(),
        password_hash: hash,
        display_name: "Racer".to_owned(),
        locale: None,
    };
    let (first, second) = tokio::join!(
        users::insert_user(&pool, &first_user),
        users::insert_user(&pool, &second_user)
    );
    let winners = usize::from(first.is_ok()) + usize::from(second.is_ok());
    assert_eq!(winners, 1, "exactly one concurrent insert may succeed");
    Ok(())
}
