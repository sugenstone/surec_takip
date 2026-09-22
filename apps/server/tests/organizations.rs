use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState, config::AuthConfig, organizations, password::PasswordService, router, users::NewUser,
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

async fn login_token(router: axum::Router, email: &str) -> String {
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
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().uri(uri).method(method);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(token) = token {
        builder = builder.header(header::COOKIE, format!("platform_session={token}"));
    }
    let body = Body::from(
        body.map(|body| body.to_string())
            .unwrap_or_else(|| "{}".to_owned()),
    );
    router
        .oneshot(
            builder
                .body(body)
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

async fn create_org_via_api(
    router: axum::Router,
    token: &str,
    body: serde_json::Value,
) -> axum::response::Response {
    request(
        router,
        "POST",
        "/api/v1/organizations",
        Some(token),
        Some(body),
    )
    .await
}

// Fixture membership without an invitation flow: tests write memberships
// directly, mirroring how invitation acceptance will insert rows later.
async fn add_membership(pool: &PgPool, organization_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(organization_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("fixture membership must be insertable"));
}

async fn org_id_by_slug(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM organizations WHERE lower(slug::text) = lower($1)",
    )
    .bind(slug)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("organization must exist"))
}

// ---------------------------------------------------------------------------
// Creation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_organization_returns_201_with_organization_and_creator_membership(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "creator@example.test").await;
    let token = login_token(app.clone(), "creator@example.test").await;

    let response =
        create_org_via_api(app, &token, serde_json::json!({ "name": "Çelik İş A.Ş." })).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    assert_eq!(body["data"]["organization"]["name"], "Çelik İş A.Ş.");
    assert_eq!(body["data"]["organization"]["slug"], "celik-is-a-s");
    assert_eq!(body["data"]["organization"]["status"], "active");
    assert_eq!(body["data"]["membership"]["status"], "active");
    assert_eq!(
        body["data"]["membership"]["organization_id"],
        body["data"]["organization"]["id"]
    );
    let joined_at = body["data"]["membership"]["joined_at"]
        .as_str()
        .unwrap_or_default();
    assert!(joined_at.ends_with('Z'), "joined_at must be ISO-8601 UTC");
    let memberships: i64 = sqlx::query_scalar("SELECT count(*) FROM organization_memberships")
        .fetch_one(&pool)
        .await?;
    assert_eq!(memberships, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_names_and_slugs_are_rejected(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "validator@example.test").await;
    let token = login_token(app.clone(), "validator@example.test").await;
    for (payload, field) in [
        (serde_json::json!({ "name": "   " }), "name"),
        (serde_json::json!({ "name": "---***---" }), "name"),
        (serde_json::json!({ "name": "x".repeat(201) }), "name"),
        // Explicit slugs are normalized; only an unusable slug is rejected.
        (serde_json::json!({ "name": "Acme", "slug": "***" }), "slug"),
    ] {
        let response = create_org_via_api(app.clone(), &token, payload).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        assert!(
            body["error"]["details"]["fields"][field].is_array(),
            "field {field} must be reported"
        );
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_slug_is_rejected_case_insensitively(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "slug@example.test").await;
    let token = login_token(app.clone(), "slug@example.test").await;
    let first = create_org_via_api(
        app.clone(),
        &token,
        serde_json::json!({ "name": "Acme Corp" }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    // citext UNIQUE: different casing of the same normalized slug collides.
    let second = create_org_via_api(
        app.clone(),
        &token,
        serde_json::json!({ "name": "Other", "slug": "ACME  CORP" }),
    )
    .await;
    assert_eq!(second.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(second).await;
    assert_eq!(
        body["error"]["details"]["fields"]["slug"][0],
        "Already taken"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_duplicate_slug_has_a_single_winner(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "race-slug@example.test").await;
    let token = login_token(app.clone(), "race-slug@example.test").await;
    let first = create_org_via_api(
        app.clone(),
        &token,
        serde_json::json!({ "name": "Race Org" }),
    );
    let second = create_org_via_api(app, &token, serde_json::json!({ "name": "race org" }));
    let (first, second) = tokio::join!(first, second);
    let created = usize::from(first.status() == StatusCode::CREATED)
        + usize::from(second.status() == StatusCode::CREATED);
    assert_eq!(created, 1, "exactly one concurrent slug creation may win");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_membership_creation_leaves_no_orphan_organization(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // A nonexistent creator user forces the membership insert (FK) to fail
    // after the organization insert already succeeded inside the same
    // transaction; the rollback must remove the organization too.
    let result = organizations::create_with_membership(
        &pool,
        &organizations::NewOrganization {
            name: "Orphan Check".to_owned(),
            slug: "orphan-check".to_owned(),
        },
        Uuid::now_v7(),
    )
    .await;
    assert!(result.is_err());
    let orphans: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organizations WHERE lower(slug::text) = 'orphan-check'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(orphans, 0, "organization creation must be atomic");
    Ok(())
}

// ---------------------------------------------------------------------------
// Membership
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn user_belonging_to_multiple_organizations_sees_them_all_deterministically(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "multi@example.test").await;
    let token = login_token(app.clone(), "multi@example.test").await;
    for name in ["Alpha Şirket", "Beta Şirket"] {
        let response =
            create_org_via_api(app.clone(), &token, serde_json::json!({ "name": name })).await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let list = body_json(
        request(
            app.clone(),
            "GET",
            "/api/v1/organizations",
            Some(&token),
            None,
        )
        .await,
    )
    .await;
    let names: Vec<&str> = list["data"]
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"))
        .iter()
        .map(|organization| organization["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(names, ["Alpha Şirket", "Beta Şirket"], "creation order");

    let me = body_json(request(app, "GET", "/api/v1/auth/me", Some(&token), None).await).await;
    let me_slugs: Vec<&str> = me["data"]["organizations"]
        .as_array()
        .unwrap_or_else(|| panic!("me organizations must be an array"))
        .iter()
        .map(|organization| organization["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(me_slugs, ["alpha-sirket", "beta-sirket"]);
    assert!(
        me["data"]["organizations"][0]["role_summary"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn different_users_may_share_an_organization(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "owner@example.test").await;
    let other = create_user(&pool, "other@example.test").await;
    let owner_token = login_token(app.clone(), "owner@example.test").await;
    let other_token = login_token(app.clone(), "other@example.test").await;
    let response = create_org_via_api(
        app.clone(),
        &owner_token,
        serde_json::json!({ "name": "Shared Org" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let shared_id = org_id_by_slug(&pool, "shared-org").await;
    add_membership(&pool, shared_id, other).await;

    for token in [&owner_token, &other_token] {
        let list = body_json(
            request(
                app.clone(),
                "GET",
                "/api/v1/organizations",
                Some(token),
                None,
            )
            .await,
        )
        .await;
        assert_eq!(list["data"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            list["data"][0]["id"].as_str().unwrap_or_default(),
            shared_id.to_string()
        );
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_membership_is_blocked_by_the_database_constraint(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "dupe-owner@example.test").await;
    let member = create_user(&pool, "dupe-member@example.test").await;
    let token = login_token(app.clone(), "dupe-owner@example.test").await;
    create_org_via_api(app, &token, serde_json::json!({ "name": "Dupe Org" })).await;
    let org_id = org_id_by_slug(&pool, "dupe-org").await;
    add_membership(&pool, org_id, member).await;

    let first = sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(member);
    let second = sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(member);
    let (first, second) = tokio::join!(first.execute(&pool), second.execute(&pool));
    let winners = usize::from(first.is_ok()) + usize::from(second.is_ok());
    assert_eq!(winners, 0, "duplicate membership rows must be rejected");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(member)
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Isolation / visibility
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn non_member_cannot_fetch_organization_and_misses_are_indistinguishable(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "island-owner@example.test").await;
    create_user(&pool, "outsider@example.test").await;
    let owner_token = login_token(app.clone(), "island-owner@example.test").await;
    let outsider_token = login_token(app.clone(), "outsider@example.test").await;
    create_org_via_api(
        app.clone(),
        &owner_token,
        serde_json::json!({ "name": "Private Org" }),
    )
    .await;
    let org_id = org_id_by_slug(&pool, "private-org").await;

    let cases = [
        format!("/api/v1/organizations/{org_id}"),
        format!("/api/v1/organizations/{}", Uuid::now_v7()),
        "/api/v1/organizations/not-a-uuid".to_owned(),
    ];
    let mut previous_body: Option<serde_json::Value> = None;
    for uri in cases {
        let response = request(app.clone(), "GET", &uri, Some(&outsider_token), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND", "{uri}");
        if let Some(previous) = &previous_body {
            assert_eq!(
                previous["error"]["code"], body["error"]["code"],
                "not-found responses must be indistinguishable"
            );
            assert_eq!(previous["error"]["message"], body["error"]["message"]);
        }
        previous_body = Some(body);
    }
    // The member still reaches the same organization.
    let response = request(
        app,
        "GET",
        &format!("/api/v1/organizations/{org_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn soft_deleted_membership_is_excluded_everywhere(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "alive@example.test").await;
    let leaver = create_user(&pool, "leaver@example.test").await;
    let owner_token = login_token(app.clone(), "alive@example.test").await;
    let leaver_token = login_token(app.clone(), "leaver@example.test").await;
    create_org_via_api(
        app.clone(),
        &owner_token,
        serde_json::json!({ "name": "Former Org" }),
    )
    .await;
    let org_id = org_id_by_slug(&pool, "former-org").await;
    add_membership(&pool, org_id, leaver).await;
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(leaver)
    .execute(&pool)
    .await?;

    let list = body_json(
        request(
            app.clone(),
            "GET",
            "/api/v1/organizations",
            Some(&leaver_token),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(list["data"].as_array().map(Vec::len), Some(0));
    let me = body_json(
        request(
            app.clone(),
            "GET",
            "/api/v1/auth/me",
            Some(&leaver_token),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        me["data"]["organizations"].as_array().map(Vec::len),
        Some(0)
    );
    let fetch = request(
        app,
        "GET",
        &format!("/api/v1/organizations/{org_id}"),
        Some(&leaver_token),
        None,
    )
    .await;
    assert_eq!(fetch.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn soft_deleted_organization_is_excluded_everywhere(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "ghost-owner@example.test").await;
    let token = login_token(app.clone(), "ghost-owner@example.test").await;
    create_org_via_api(
        app.clone(),
        &token,
        serde_json::json!({ "name": "Ghost Org" }),
    )
    .await;
    let org_id = org_id_by_slug(&pool, "ghost-org").await;
    sqlx::query("UPDATE organizations SET deleted_at = now() WHERE id = $1")
        .bind(org_id)
        .execute(&pool)
        .await?;

    let list = body_json(
        request(
            app.clone(),
            "GET",
            "/api/v1/organizations",
            Some(&token),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(list["data"].as_array().map(Vec::len), Some(0));
    let me =
        body_json(request(app.clone(), "GET", "/api/v1/auth/me", Some(&token), None).await).await;
    assert_eq!(
        me["data"]["organizations"].as_array().map(Vec::len),
        Some(0)
    );
    let fetch = request(
        app,
        "GET",
        &format!("/api/v1/organizations/{org_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(fetch.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn auth_me_never_leaks_foreign_organizations(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "member-a@example.test").await;
    let user_b = create_user(&pool, "member-b@example.test").await;
    let token_a = login_token(app.clone(), "member-a@example.test").await;
    let token_b = login_token(app.clone(), "member-b@example.test").await;
    create_org_via_api(
        app.clone(),
        &token_a,
        serde_json::json!({ "name": "Org A" }),
    )
    .await;
    create_org_via_api(
        app.clone(),
        &token_b,
        serde_json::json!({ "name": "Org B" }),
    )
    .await;
    create_org_via_api(
        app.clone(),
        &token_a,
        serde_json::json!({ "name": "Org Shared" }),
    )
    .await;
    let shared = org_id_by_slug(&pool, "org-shared").await;
    add_membership(&pool, shared, user_b).await;

    let me_a =
        body_json(request(app.clone(), "GET", "/api/v1/auth/me", Some(&token_a), None).await).await;
    let slugs_a: Vec<&str> = me_a["data"]["organizations"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|organization| organization["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(slugs_a, ["org-a", "org-shared"]);

    let me_b = body_json(request(app, "GET", "/api/v1/auth/me", Some(&token_b), None).await).await;
    let slugs_b: Vec<&str> = me_b["data"]["organizations"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|organization| organization["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        slugs_b,
        ["org-b", "org-shared"],
        "user B must never see Org A"
    );
    Ok(())
}
