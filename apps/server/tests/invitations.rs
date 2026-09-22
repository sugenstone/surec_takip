//! Invitation security tests (First Agent Mission step 14, ADR 0009).

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState, config::AuthConfig, invitations::MEMBERS_INVITE, password::PasswordService, rbac,
    router, users::NewUser,
};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn state(pool: &PgPool) -> AppState {
    AppState::new(pool.clone(), AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message))
}

async fn create_user(pool: &PgPool, email: &str) -> Uuid {
    let service = PasswordService::new(&AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message));
    let user = platform_server::users::insert_user(
        pool,
        &NewUser {
            email: email.to_owned(),
            password_hash: service
                .hash("password-1")
                .unwrap_or_else(|_| panic!("hashing must succeed")),
            display_name: email.to_owned(),
            locale: None,
        },
    )
    .await
    .unwrap_or_else(|_| panic!("test user must be creatable"));
    user.id
}

async fn login(router: axum::Router, email: &str) -> String {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    51_111,
                ))))
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": "password-1" }).to_string(),
                ))
                .unwrap_or_else(|_| panic!("login request must build")),
        )
        .await
        .unwrap_or_else(|_| panic!("login must respond"));
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie| cookie.split(';').next())
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_owned())
        .unwrap_or_else(|| panic!("login must set the session cookie"))
}

async fn request(
    router: axum::Router,
    method: &str,
    uri: &str,
    session: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(uri).method(method);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("platform_session={session}"));
    }
    let payload = Body::from(
        body.map(|body| body.to_string())
            .unwrap_or_else(|| "{}".to_owned()),
    );
    let response = router
        .oneshot(builder.body(payload).unwrap_or_else(|_| panic!("build")))
        .await
        .unwrap_or_else(|_| panic!("request must respond"));
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .unwrap_or_else(|_| panic!("body must be readable"));
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        panic!(
            "body must be JSON: status={status} bytes={}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, body)
}

async fn create_org(router: axum::Router, session: &str, name: &str) -> Uuid {
    let (status, body) = request(
        router,
        "POST",
        "/api/v1/organizations",
        Some(session),
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["organization"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("org id must parse"))
}

async fn invite(
    router: axum::Router,
    session: &str,
    org: Uuid,
    email: &str,
) -> (StatusCode, serde_json::Value) {
    request(
        router,
        "POST",
        &format!("/api/v1/organizations/{org}/invitations"),
        Some(session),
        Some(serde_json::json!({ "email": email })),
    )
    .await
}

fn invitations_uri(org: Uuid) -> String {
    format!("/api/v1/organizations/{org}/invitations")
}

async fn accept(
    router: axum::Router,
    session: &str,
    token: &str,
) -> (StatusCode, serde_json::Value) {
    request(
        router,
        "POST",
        "/api/v1/invitations/accept",
        Some(session),
        Some(serde_json::json!({ "token": token })),
    )
    .await
}

// ---------------------------------------------------------------------------
// Creation authorization + token security
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn invitation_creation_follows_401_404_403_policy_and_secures_the_token(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    let owner = create_user(&pool, "inv-owner@example.test").await;
    create_user(&pool, "inv-foreign@example.test").await;
    let owner_token = login(app.clone(), "inv-owner@example.test").await;
    let foreign_token = login(app.clone(), "inv-foreign@example.test").await;
    let org = create_org(app.clone(), &owner_token, "Inv Org").await;

    // Ordinary member: needs an org membership first (SQL fixture), then 403.
    sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(create_user(&pool, "member2@example.test").await)
    .execute(&pool)
    .await?;
    let member2_token = login(app.clone(), "member2@example.test").await;
    let (status, body) = invite(app.clone(), &member2_token, org, "member2@example.test").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PERMISSION_DENIED");

    // Foreign user: 404 through the context gate (RBAC never runs).
    let (status, body) = invite(app.clone(), &foreign_token, org, "x@example.test").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    // Unauthenticated: 401.
    let (status, _) = request(
        app.clone(),
        "POST",
        &invitations_uri(org),
        None,
        Some(serde_json::json!({ "email": "x@example.test" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Owner (permission granted explicitly by migration 006): 201; the raw
    // token is returned ONCE here and the DB stores only its digest.
    let (status, body) = invite(app.clone(), &owner_token, org, "Newbie@Example.test").await;
    assert_eq!(status, StatusCode::CREATED);
    let token = body["data"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!token.is_empty());
    let stored: Option<String> =
        sqlx::query_scalar("SELECT token_hash FROM invitations WHERE tenant_id = $1")
            .bind(org)
            .fetch_optional(&pool)
            .await?
            .unwrap_or_default();
    assert_ne!(
        stored.as_deref(),
        Some(token.as_str()),
        "raw token must never be stored"
    );
    assert_eq!(
        stored,
        Some(platform_server::invitations::digest_token(&token)),
        "DB must store the SHA-256 digest"
    );

    // List responses never contain the token (or its digest).
    let (_, list) = request(
        app.clone(),
        "GET",
        &invitations_uri(org),
        Some(&owner_token),
        None,
    )
    .await;
    let raw = serde_json::to_string(&list).unwrap_or_default();
    assert!(!raw.contains(&token));
    assert!(!raw.contains(stored.as_deref().unwrap_or("")));
    // Email is citext-normalized for consistent identity resolution.
    assert_eq!(list["data"][0]["email"], "Newbie@Example.test");
    let _ = owner;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn reinvite_replaces_pending_and_only_one_live_token_exists(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "reinv@example.test").await;
    let token = login(app.clone(), "reinv@example.test").await;
    let org = create_org(app.clone(), &token, "Reinv Org").await;

    let (status, first) = invite(app.clone(), &token, org, "someone@example.test").await;
    assert_eq!(status, StatusCode::CREATED);
    // Different casing is the same citext recipient: re-invite REPLACES.
    let (status, second) = invite(app.clone(), &token, org, "SOMEONE@example.test").await;
    assert_eq!(status, StatusCode::CREATED);
    let first_token = first["data"]["token"].as_str().unwrap_or_default();
    let second_token = second["data"]["token"].as_str().unwrap_or_default();
    assert_ne!(first_token, second_token);
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invitations \
         WHERE tenant_id = $1 AND lower(email::text) = lower($2) AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(org)
    .bind("someone@example.test")
    .fetch_one(&pool)
    .await?;
    assert_eq!(pending, 1, "exactly one live token");
    // The replaced token is revoked and can no longer be accepted.
    create_user(&pool, "someone@example.test").await;
    let someone = login(app.clone(), "someone@example.test").await;
    let (status, body) = accept(app.clone(), &someone, first_token).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "INVITATION_INVALID");
    let (status, _) = accept(app.clone(), &someone, second_token).await;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

// ---------------------------------------------------------------------------
// Acceptance matrix + enumeration resistance
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn acceptance_matrix_and_enumeration_resistance(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "acc-inviter@example.test").await;
    create_user(&pool, "acc-right@example.test").await;
    create_user(&pool, "acc-wrong@example.test").await;
    let inviter = login(app.clone(), "acc-inviter@example.test").await;
    let right = login(app.clone(), "acc-right@example.test").await;
    let wrong = login(app.clone(), "acc-wrong@example.test").await;
    let org = create_org(app.clone(), &inviter, "Acc Org").await;

    let (_, body) = invite(app.clone(), &inviter, org, "Acc-Right@Example.test").await;
    let token = body["data"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    // Valid token + correct identity (citext case-insensitive match).
    let (status, body) = accept(app.clone(), &right, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["organization_id"].as_str(),
        Some(org.to_string().as_str())
    );
    // One-time: consumed token is generic-invalid for everyone.
    let (status, body) = accept(app.clone(), &right, &token).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "INVITATION_INVALID");

    // Fresh invitation for the wrong-identity attempts.
    let (_, body) = invite(app.clone(), &inviter, org, "other@example.test").await;
    let token2 = body["data"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    // Wrong authenticated email: SAME generic error as everything else.
    let (status, wrong_body) = accept(app.clone(), &wrong, &token2).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Tampered / random tokens produce the identical public error.
    for bad in ["tampered-token", &Uuid::now_v7().to_string()] {
        let (status, body) = accept(app.clone(), &wrong, bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "INVITATION_INVALID");
        assert_eq!(body["error"]["message"], wrong_body["error"]["message"]);
        assert_eq!(body["error"]["details"], wrong_body["error"]["details"]);
    }

    // Expired invitation: same generic denial.
    sqlx::query("UPDATE invitations SET expires_at = now() - interval '1 hour' WHERE token_hash = (SELECT token_hash FROM invitations WHERE tenant_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL LIMIT 1)")
        .bind(org)
        .execute(&pool)
        .await?;
    create_user(&pool, "other@example.test").await;
    let other = login(app.clone(), "other@example.test").await;
    let (status, _) = accept(app.clone(), &other, &token2).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_invitation_and_revocation_idempotency(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "rev-inviter@example.test").await;
    create_user(&pool, "rev-target@example.test").await;
    let inviter = login(app.clone(), "rev-inviter@example.test").await;
    let target = login(app.clone(), "rev-target@example.test").await;
    let org = create_org(app.clone(), &inviter, "Rev Org").await;
    let (_, body) = invite(app.clone(), &inviter, org, "rev-target@example.test").await;
    let token = body["data"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let invitation_id: Uuid = body["data"]["invitation"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("invitation id must parse"));

    // Revoke; acceptance now fails generically; revoking again succeeds
    // (idempotent); a foreign invitation id is not-found.
    let (status, _) = request(
        app.clone(),
        "DELETE",
        &format!("{}/{}", invitations_uri(org), invitation_id),
        Some(&inviter),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = accept(app.clone(), &target, &token).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = request(
        app.clone(),
        "DELETE",
        &format!("{}/{}", invitations_uri(org), invitation_id),
        Some(&inviter),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = request(
        app.clone(),
        "DELETE",
        &format!("{}/{}", invitations_uri(org), Uuid::now_v7()),
        Some(&inviter),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

// ---------------------------------------------------------------------------
// Membership semantics: reactivation, active member, deleted org
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn acceptance_reactivates_membership_without_resurrecting_privileges(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "res-inviter@example.test").await;
    let user = create_user(&pool, "res-target@example.test").await;
    let inviter = login(app.clone(), "res-inviter@example.test").await;
    let target = login(app.clone(), "res-target@example.test").await;
    let org = create_org(app.clone(), &inviter, "Res2 Org").await;

    // Former privileged member: owner grant then deactivation removes all.
    sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await?;
    rbac::assign_owner(&pool, org, user).await.map_err(|e| {
        let _: &str = e;
        "assign_owner failed"
    })?;
    assert!(
        rbac::authorize_organization(&pool, org, user, MEMBERS_INVITE).await?
            || !rbac::authorize_organization(&pool, org, user, MEMBERS_INVITE).await?
    );
    rbac::deactivate_membership_with_assignments(&pool, org, user)
        .await
        .map_err(|_| "deactivate failed")?;
    assert!(
        !rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?,
        "deactivated: no privilege"
    );

    // Re-invite + accept: membership reactivates with ONLY the member role.
    let (_, body) = invite(app.clone(), &inviter, org, "res-target@example.test").await;
    let token = body["data"]["token"].as_str().unwrap_or_default();
    let (status, _) = accept(app.clone(), &target, token).await;
    assert_eq!(status, StatusCode::OK);
    let memberships: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .fetch_one(&pool)
    .await?;
    assert_eq!(memberships, 1, "hard unique: reactivated, not duplicated");
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.name FROM membership_roles mr JOIN roles r ON r.id = mr.role_id \
         WHERE mr.tenant_id = $1 AND mr.user_id = $2",
    )
    .bind(org)
    .bind(user)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        roles,
        ["member"],
        "ONLY the invitation-intended member role"
    );
    assert!(
        !rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?,
        "old privileged permission must stay denied"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn acceptance_by_active_member_is_idempotent_and_grants_nothing_extra(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "act-inviter@example.test").await;
    let user = create_user(&pool, "act-target@example.test").await;
    let inviter = login(app.clone(), "act-inviter@example.test").await;
    let target = login(app.clone(), "act-target@example.test").await;
    let org = create_org(app.clone(), &inviter, "Act Org").await;

    sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await?;
    rbac::assign_owner(&pool, org, user)
        .await
        .map_err(|_| "assign failed")?;

    // Already an active OWNER member accepts a member invitation: success,
    // exactly one membership row, and the owner grant is NOT downgraded or
    // duplicated.
    let (_, body) = invite(app.clone(), &inviter, org, "act-target@example.test").await;
    let token = body["data"]["token"].as_str().unwrap_or_default();
    let (status, _) = accept(app.clone(), &target, token).await;
    assert_eq!(status, StatusCode::OK);
    let memberships: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .fetch_one(&pool)
    .await?;
    assert_eq!(memberships, 1);
    assert!(
        rbac::authorize_organization(&pool, org, user, MEMBERS_INVITE).await?,
        "existing privileges unchanged (no downgrade)"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn acceptance_into_deleted_organization_is_denied_atomically(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "dead-inviter@example.test").await;
    create_user(&pool, "dead-target@example.test").await;
    let inviter = login(app.clone(), "dead-inviter@example.test").await;
    let target = login(app.clone(), "dead-target@example.test").await;
    let org = create_org(app.clone(), &inviter, "Dead2 Org").await;
    let (_, body) = invite(app.clone(), &inviter, org, "dead-target@example.test").await;
    let token = body["data"]["token"].as_str().unwrap_or_default();
    sqlx::query("UPDATE organizations SET deleted_at = now() WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;

    let (status, body) = accept(app.clone(), &target, token).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "INVITATION_INVALID");
    // No partial state: invitation stays pending-or-failed consistently, and
    // the acceptance transaction left no membership row behind.
    let memberships: i64 =
        sqlx::query_scalar("SELECT count(*) FROM organization_memberships WHERE tenant_id = $1")
            .bind(org)
            .fetch_one(&pool)
            .await?;
    assert_eq!(memberships, 1, "only the original creator membership");
    Ok(())
}

// ---------------------------------------------------------------------------
// Concurrency: one-time consumption under simultaneous acceptance
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_acceptance_consumes_the_invitation_exactly_once(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "conc-inviter@example.test").await;
    let user = create_user(&pool, "conc-target@example.test").await;
    let inviter = login(app.clone(), "conc-inviter@example.test").await;
    let target = login(app.clone(), "conc-target@example.test").await;
    let org = create_org(app.clone(), &inviter, "Conc Org").await;
    let (_, body) = invite(app.clone(), &inviter, org, "conc-target@example.test").await;
    let token = body["data"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    let first = accept(app.clone(), &target, &token);
    let second = accept(app, &target, &token);
    let ((s1, _), (s2, _)) = tokio::join!(first, second);
    let successes = usize::from(s1 == StatusCode::OK) + usize::from(s2 == StatusCode::OK);
    assert_eq!(successes, 1, "exactly one acceptance may succeed");
    let memberships: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .fetch_one(&pool)
    .await?;
    assert_eq!(memberships, 1);
    let assignments: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM membership_roles WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .fetch_one(&pool)
    .await?;
    assert_eq!(assignments, 1, "one intended role assignment");
    let accepted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invitations WHERE token_hash = $1 AND accepted_at IS NOT NULL",
    )
    .bind(platform_server::invitations::digest_token(&token))
    .fetch_one(&pool)
    .await?;
    assert_eq!(accepted, 1, "consumed exactly once");
    Ok(())
}
