use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState, config::AuthConfig, organizations::NewOrganization, password::PasswordService,
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

async fn create_org_via_api(router: axum::Router, token: &str, name: &str) -> Uuid {
    let response = request(
        router,
        "POST",
        "/api/v1/organizations",
        Some(token),
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body_json(response).await["data"]["organization"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("organization id must parse"))
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

async fn ws_id_by_slug(pool: &PgPool, tenant: Uuid, slug: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM workspaces WHERE tenant_id = $1 AND lower(slug::text) = lower($2)",
    )
    .bind(tenant)
    .bind(slug)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("workspace must exist"))
}

fn ws_path(org: Uuid) -> String {
    format!("/api/v1/organizations/{org}/workspaces")
}

fn ws_item_path(org: Uuid, workspace: Uuid) -> String {
    format!("{}/{}", ws_path(org), workspace)
}

// Fixture helpers: memberships are written directly, mirroring how the
// future invitation acceptance will insert rows.
async fn add_org_membership(pool: &PgPool, org: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(user)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("org membership must be insertable"));
}

async fn add_ws_membership(pool: &PgPool, tenant: Uuid, workspace: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(tenant)
    .bind(workspace)
    .bind(user)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("ws membership must be insertable"));
}

// A workspace row without creator membership, for non-member visibility tests.
async fn insert_bare_workspace(pool: &PgPool, tenant: Uuid, name: &str, slug: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id, tenant_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(tenant)
        .bind(name)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap_or_else(|_| panic!("bare workspace must be insertable"));
    id
}

// ---------------------------------------------------------------------------
// Creation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn member_creates_workspace_with_membership_in_requested_organization(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "ws-owner@example.test").await;
    let token = login_token(app.clone(), "ws-owner@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Ws Org").await;

    let response = request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "İstanbul Fabrika" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    assert_eq!(body["data"]["workspace"]["name"], "İstanbul Fabrika");
    assert_eq!(body["data"]["workspace"]["slug"], "istanbul-fabrika");
    assert_eq!(
        body["data"]["workspace"]["organization_id"]
            .as_str()
            .unwrap_or_default(),
        org.to_string()
    );
    assert_eq!(body["data"]["membership"]["status"], "active");
    let workspace_id: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("workspace id must parse"));
    let tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        tenant, org,
        "workspace must belong to the requested organization"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn non_member_cannot_create_workspace_in_foreign_organization(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "ws-a@example.test").await;
    create_user(&pool, "ws-b@example.test").await;
    let token_a = login_token(app.clone(), "ws-a@example.test").await;
    let token_b = login_token(app.clone(), "ws-b@example.test").await;
    let org_a = create_org_via_api(app.clone(), &token_a, "Foreign Org").await;
    create_org_via_api(app.clone(), &token_b, "Own Org").await;

    let response = request(
        app,
        "POST",
        &ws_path(org_a),
        Some(&token_b),
        Some(serde_json::json!({ "name": "Intruder" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_membership_creation_leaves_no_orphan_workspace(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // A nonexistent creator forces the membership insert (FK) to fail after
    // the workspace insert succeeded in the same transaction; the rollback
    // must remove the workspace too.
    let owner = create_user(&pool, "orphan-owner@example.test").await;
    platform_server::organizations::create_with_membership(
        &pool,
        &NewOrganization {
            name: "Orphan Org".to_owned(),
            slug: "orphan-org".to_owned(),
        },
        owner,
    )
    .await?;
    let org = org_id_by_slug(&pool, "orphan-org").await;
    let result = platform_server::workspaces::create_with_membership(
        &pool,
        org,
        &platform_server::workspaces::NewWorkspace {
            name: "Orphan Ws".to_owned(),
            slug: "orphan-ws".to_owned(),
        },
        Uuid::now_v7(),
        platform_server::rbac::WORKSPACES_CREATE,
    )
    .await;
    assert!(result.is_err());
    let orphans: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE lower(slug::text) = 'orphan-ws'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(orphans, 0, "workspace creation must be atomic");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_slug_is_per_organization_and_case_insensitive(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "slug-ws@example.test").await;
    create_user(&pool, "slug-ws2@example.test").await;
    let token = login_token(app.clone(), "slug-ws@example.test").await;
    let token_other = login_token(app.clone(), "slug-ws2@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Slug Org").await;
    let other_org = create_org_via_api(app.clone(), &token_other, "Slug Org Two").await;

    let first = request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Production" })),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    // Same slug in the same organization collides (case-insensitive citext).
    let second = request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Other", "slug": "PRODUCTION" })),
    )
    .await;
    assert_eq!(second.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(second).await;
    assert_eq!(
        body["error"]["details"]["fields"]["slug"][0],
        "Already taken"
    );
    // The same slug in a different organization is fine: uniqueness is per tenant.
    let elsewhere = request(
        app,
        "POST",
        &ws_path(other_org),
        Some(&token_other),
        Some(serde_json::json!({ "name": "Production" })),
    )
    .await;
    assert_eq!(elsewhere.status(), StatusCode::CREATED);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_duplicate_slug_has_a_single_winner(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "race-ws@example.test").await;
    let token = login_token(app.clone(), "race-ws@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Race Ws Org").await;
    let path = ws_path(org);
    let first = request(
        app.clone(),
        "POST",
        &path,
        Some(&token),
        Some(serde_json::json!({ "name": "Race Ws" })),
    );
    let second = request(
        app,
        "POST",
        &path,
        Some(&token),
        Some(serde_json::json!({ "name": "race ws" })),
    );
    let (first, second) = tokio::join!(first, second);
    let created = usize::from(first.status() == StatusCode::CREATED)
        + usize::from(second.status() == StatusCode::CREATED);
    assert_eq!(created, 1, "exactly one concurrent slug creation may win");
    Ok(())
}

// ---------------------------------------------------------------------------
// Visibility / isolation matrix
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_visibility_matrix_enforces_org_and_workspace_membership(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "mat-a@example.test").await;
    let user_b = create_user(&pool, "mat-b@example.test").await;
    let token_a = login_token(app.clone(), "mat-a@example.test").await;
    let token_b = login_token(app.clone(), "mat-b@example.test").await;

    let org_a = create_org_via_api(app.clone(), &token_a, "Matrix Org A").await;
    let org_b = create_org_via_api(app.clone(), &token_b, "Matrix Org B").await;
    let shared = create_org_via_api(app.clone(), &token_a, "Matrix Shared").await;
    add_org_membership(&pool, shared, user_b).await;

    // A1: A is member (creator). A2: exists in Org A but A is NOT a member.
    let a1 = request(
        app.clone(),
        "POST",
        &ws_path(org_a),
        Some(&token_a),
        Some(serde_json::json!({ "name": "A1" })),
    )
    .await;
    assert_eq!(a1.status(), StatusCode::CREATED);
    let a1_id = ws_id_by_slug(&pool, org_a, "a1").await;
    let a2_id = insert_bare_workspace(&pool, org_a, "A2", "a2").await;
    let b1 = request(
        app.clone(),
        "POST",
        &ws_path(org_b),
        Some(&token_b),
        Some(serde_json::json!({ "name": "B1" })),
    )
    .await;
    assert_eq!(b1.status(), StatusCode::CREATED);
    let b1_id = ws_id_by_slug(&pool, org_b, "b1").await;
    let shared_ws = request(
        app.clone(),
        "POST",
        &ws_path(shared),
        Some(&token_a),
        Some(serde_json::json!({ "name": "Shared Ws" })),
    )
    .await;
    assert_eq!(shared_ws.status(), StatusCode::CREATED);
    let shared_ws_id = ws_id_by_slug(&pool, shared, "shared-ws").await;
    add_ws_membership(&pool, shared, shared_ws_id, user_b).await;

    // User A: sees only A1 in Org A; A2 (no ws membership) never appears.
    let list_a =
        body_json(request(app.clone(), "GET", &ws_path(org_a), Some(&token_a), None).await).await;
    let slugs_a: Vec<&str> = list_a["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|w| w["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(slugs_a, ["a1"]);
    // User A cannot fetch A2 or B1; shared workspace works.
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(org_a, a2_id),
            Some(&token_a),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(org_b, b1_id),
            Some(&token_a),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(shared, shared_ws_id),
            Some(&token_a),
            None
        )
        .await
        .status(),
        StatusCode::OK
    );

    // User B: sees only B1 in Org B and Shared Ws in Shared Org.
    let list_b_org_b =
        body_json(request(app.clone(), "GET", &ws_path(org_b), Some(&token_b), None).await).await;
    let slugs_b: Vec<&str> = list_b_org_b["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|w| w["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(slugs_b, ["b1"]);
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(org_a, a1_id),
            Some(&token_b),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(shared, shared_ws_id),
            Some(&token_b),
            None
        )
        .await
        .status(),
        StatusCode::OK
    );

    // Listing workspaces of a foreign organization behaves as not found.
    assert_eq!(
        request(app, "GET", &ws_path(org_a), Some(&token_b), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn mismatched_parent_child_path_is_rejected_even_for_dual_members(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "dual@example.test").await;
    let token = login_token(app.clone(), "dual@example.test").await;
    let org_a = create_org_via_api(app.clone(), &token, "Dual Org A").await;
    let org_b = create_org_via_api(app.clone(), &token, "Dual Org B").await;
    let response = request(
        app.clone(),
        "POST",
        &ws_path(org_b),
        Some(&token),
        Some(serde_json::json!({ "name": "Belongs To B" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let b_ws = ws_id_by_slug(&pool, org_b, "belongs-to-b").await;

    // The user is a member of BOTH organizations, but the parent-child path
    // combination is wrong: must not succeed.
    let mismatch = request(
        app.clone(),
        "GET",
        &ws_item_path(org_a, b_ws),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(mismatch.status(), StatusCode::NOT_FOUND);

    // Random, malformed and mismatched ids are indistinguishable.
    let random = request(
        app.clone(),
        "GET",
        &ws_item_path(org_a, Uuid::now_v7()),
        Some(&token),
        None,
    )
    .await;
    let malformed = request(
        app,
        "GET",
        &format!("{}/not-a-uuid", ws_path(org_a)),
        Some(&token),
        None,
    )
    .await;
    for response in [mismatch, random, malformed] {
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Membership invariants
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_workspace_membership_is_blocked_by_the_database(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "dupe-ws@example.test").await;
    let member = create_user(&pool, "dupe-ws-member@example.test").await;
    let token = login_token(app.clone(), "dupe-ws@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Dupe Ws Org").await;
    request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Dupe Ws" })),
    )
    .await;
    let ws = ws_id_by_slug(&pool, org, "dupe-ws").await;
    add_ws_membership(&pool, org, ws, member).await;

    let first = sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(ws)
    .bind(member);
    let second = sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(ws)
    .bind(member);
    let (first, second) = tokio::join!(first.execute(&pool), second.execute(&pool));
    let winners = usize::from(first.is_ok()) + usize::from(second.is_ok());
    assert_eq!(winners, 0, "duplicate membership rows must be rejected");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_memberships WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(ws)
    .bind(member)
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn cross_tenant_workspace_membership_is_rejected_by_the_database(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    let user_a = create_user(&pool, "xt-a@example.test").await;
    create_user(&pool, "xt-b@example.test").await;
    let token_a = login_token(app.clone(), "xt-a@example.test").await;
    let token_b = login_token(app.clone(), "xt-b@example.test").await;
    let org_a = create_org_via_api(app.clone(), &token_a, "Xt Org A").await;
    let org_b = create_org_via_api(app.clone(), &token_b, "Xt Org B").await;
    request(
        app.clone(),
        "POST",
        &ws_path(org_b),
        Some(&token_b),
        Some(serde_json::json!({ "name": "B Workspace" })),
    )
    .await;
    let ws_b = ws_id_by_slug(&pool, org_b, "b-workspace").await;

    // Attempt to attach user A (tenant A) to workspace B while claiming
    // tenant A: the composite FK (tenant_id, workspace_id) rejects it.
    let wrong_tenant = sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(org_a)
    .bind(ws_b)
    .bind(user_a)
    .execute(&pool)
    .await;
    assert!(
        wrong_tenant.is_err(),
        "cross-tenant membership must fail at the DB level"
    );

    // Even an honest tenant_id row is still a cross-tenant relationship:
    // user A has no organization membership in tenant B, so the composite FK
    // is the storage-level guard and the read path adds the access guard.
    let honest_tenant = sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(org_b)
    .bind(ws_b)
    .bind(user_a)
    .execute(&pool)
    .await;
    assert!(
        honest_tenant.is_ok(),
        "storage allows the row, access must be decided at read time"
    );
    // User A is not an organization member of Org B: the list endpoint is 404.
    // Direct read path check:
    let visible = platform_server::workspaces::visible_for_user(&pool, org_b, user_a).await;
    assert!(
        visible.unwrap_or_default().is_empty(),
        "stale or foreign membership must not grant visibility"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Parent validity interactions
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn stale_workspace_membership_cannot_survive_organization_membership_deletion(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    let user = create_user(&pool, "stale@example.test").await;
    let token = login_token(app.clone(), "stale@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Stale Org").await;
    request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Stale Ws" })),
    )
    .await;
    let ws = ws_id_by_slug(&pool, org, "stale-ws").await;

    // Workspace membership exists and grants access.
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(org, ws),
            Some(&token),
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    // Organization membership is soft-deleted: the workspace membership must
    // stop granting access immediately (access path joins org membership).
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await?;
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(org, ws),
            Some(&token),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND,
        "stale workspace membership must not bypass deleted organization membership"
    );
    // Listing an organization the user no longer belongs to is not-found,
    // exactly like a foreign organization.
    assert_eq!(
        request(app, "GET", &ws_path(org), Some(&token), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleted_organization_hides_its_workspaces(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "deadorg@example.test").await;
    let token = login_token(app.clone(), "deadorg@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Dead Org").await;
    request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Dead Org Ws" })),
    )
    .await;
    let ws = ws_id_by_slug(&pool, org, "dead-org-ws").await;
    sqlx::query("UPDATE organizations SET deleted_at = now() WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;

    assert_eq!(
        request(app, "GET", &ws_item_path(org, ws), Some(&token), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn deleted_workspace_and_deleted_membership_are_hidden(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    let user = create_user(&pool, "life@example.test").await;
    let token = login_token(app.clone(), "life@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Life Org").await;
    request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Life One" })),
    )
    .await;
    request(
        app.clone(),
        "POST",
        &ws_path(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Life Two" })),
    )
    .await;
    let one = ws_id_by_slug(&pool, org, "life-one").await;
    let two = ws_id_by_slug(&pool, org, "life-two").await;

    // Multiple workspace memberships list deterministically.
    let list =
        body_json(request(app.clone(), "GET", &ws_path(org), Some(&token), None).await).await;
    let slugs: Vec<&str> = list["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|w| w["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(slugs, ["life-one", "life-two"]);

    // Soft-deleted workspace disappears everywhere.
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(two)
        .execute(&pool)
        .await?;
    let list =
        body_json(request(app.clone(), "GET", &ws_path(org), Some(&token), None).await).await;
    assert_eq!(list["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(org, two),
            Some(&token),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );

    // Soft-deleted workspace membership hides that workspace only.
    sqlx::query(
        "UPDATE workspace_memberships SET status = 'deleted', deleted_at = now() \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(one)
    .bind(user)
    .execute(&pool)
    .await?;
    // The same ACCESS_FILTER governs the single-resource endpoint.
    assert_eq!(
        request(
            app.clone(),
            "GET",
            &ws_item_path(org, one),
            Some(&token),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let list = body_json(request(app, "GET", &ws_path(org), Some(&token), None).await).await;
    assert_eq!(list["data"].as_array().map(Vec::len), Some(0));
    Ok(())
}
