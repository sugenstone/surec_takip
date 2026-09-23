use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState,
    config::AuthConfig,
    migrations::MIGRATOR,
    password::PasswordService,
    projects::{self, NewProject, PROJECTS_CREATE, ProjectError, ProjectPatch},
    router,
    users::NewUser,
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
    request_with_extra_cookies(router, method, uri, token, body, &[]).await
}

async fn request_with_extra_cookies(
    router: axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
    extra_cookies: &[String],
) -> axum::response::Response {
    let mut builder = Request::builder().uri(uri).method(method);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let mut cookie = String::new();
    if let Some(token) = token {
        cookie.push_str(&format!("platform_session={token}"));
    }
    for extra in extra_cookies {
        if !cookie.is_empty() {
            cookie.push_str("; ");
        }
        cookie.push_str(extra);
    }
    if !cookie.is_empty() {
        builder = builder.header(header::COOKIE, cookie);
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

// The error envelope without the per-request id, for indistinguishability
// comparisons (foreign vs unknown must be byte-equal apart from request_id).
async fn error_fingerprint(response: axum::response::Response) -> serde_json::Value {
    let body = body_json(response).await;
    let mut error = body["error"].clone();
    error["request_id"] = serde_json::json!("<stripped>");
    error
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

async fn create_workspace_via_api(
    router: axum::Router,
    token: &str,
    org: Uuid,
    name: &str,
) -> Uuid {
    let response = request(
        router,
        "POST",
        &format!("/api/v1/organizations/{org}/workspaces"),
        Some(token),
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body_json(response).await["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("workspace id must parse"))
}

async fn create_project_via_api(
    router: axum::Router,
    token: &str,
    org: Uuid,
    workspace: Uuid,
    name: &str,
) -> Uuid {
    let response = request(
        router,
        "POST",
        &projects_path(org, workspace),
        Some(token),
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body_json(response).await["data"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("project id must parse"))
}

fn projects_path(org: Uuid, workspace: Uuid) -> String {
    format!("/api/v1/organizations/{org}/workspaces/{workspace}/projects")
}

fn project_item_path(org: Uuid, workspace: Uuid, project: Uuid) -> String {
    format!("{}/{}", projects_path(org, workspace), project)
}

// Fixture helpers: memberships are written directly, mirroring how the
// invitation acceptance inserts rows.
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

async fn user_id_by_email(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| panic!("user must exist"))
}

/// Grants `keys` at WORKSPACE scope through a fresh custom role bound to one
/// workspace, mirroring how a future role-management surface would store it.
async fn grant_workspace_scoped(
    pool: &PgPool,
    tenant: Uuid,
    workspace: Uuid,
    user: Uuid,
    keys: &[&str],
) {
    let role: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (id, tenant_id, name, is_system) \
         VALUES ($1, $2, 'ws-project-role', false) RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(tenant)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("role must be insertable"));
    for key in keys {
        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_id, scope) \
             SELECT $1, p.id, 'workspace' FROM permissions p WHERE p.key = $2",
        )
        .bind(role)
        .bind(key)
        .execute(pool)
        .await
        .unwrap_or_else(|_| panic!("role permission must be insertable"));
    }
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(tenant)
    .bind(user)
    .bind(role)
    .bind(workspace)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("membership role must be insertable"));
}

// ---------------------------------------------------------------------------
// Owner CRUD + canonical lifecycle
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn owner_roundtrip_covers_crud_and_canonical_lifecycle(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-owner@example.test").await;
    let token = login_token(app.clone(), "proj-owner@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Project Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Main Line").await;

    // Create: slug derives from the name, status defaults to active.
    let response = request(
        app.clone(),
        "POST",
        &projects_path(org, workspace),
        Some(&token),
        Some(serde_json::json!({ "name": "Atölye Kurulum", "description": "İlk proje" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    assert_eq!(body["data"]["name"], "Atölye Kurulum");
    assert_eq!(body["data"]["slug"], "atolye-kurulum");
    assert_eq!(body["data"]["status"], "active");
    assert_eq!(body["data"]["workspace_id"], workspace.to_string());
    assert_eq!(body["data"]["organization_id"], org.to_string());
    let project: Uuid = body["data"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("project id must parse"));

    // List: one project, scoped to this workspace.
    let response = request(
        app.clone(),
        "GET",
        &projects_path(org, workspace),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["data"].as_array().map(Vec::len),
        Some(1),
        "list must contain exactly the created project"
    );

    // Get: single scoped lookup.
    let response = request(
        app.clone(),
        "GET",
        &project_item_path(org, workspace, project),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["name"], "Atölye Kurulum");

    // PATCH content: name + description, slug untouched.
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "name": "Atölye Kurulum v2", "description": "" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["data"]["name"], "Atölye Kurulum v2");
    assert_eq!(body["data"]["description"], serde_json::Value::Null);
    assert_eq!(body["data"]["slug"], "atolye-kurulum");

    // Canonical lifecycle: active -> completed -> archived -> active.
    for status in ["completed", "archived", "active"] {
        let response = request(
            app.clone(),
            "PATCH",
            &project_item_path(org, workspace, project),
            Some(&token),
            Some(serde_json::json!({ "status": status })),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["data"]["status"], status);
    }
    // active -> archived, then archived -> completed is NOT reachable.
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "status": "archived" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "status": "completed" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_validates_name_slug_and_description(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-valid@example.test").await;
    let token = login_token(app.clone(), "proj-valid@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Validation Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Validation Ws").await;

    let cases = [
        (serde_json::json!({ "name": "   " }), "name"),
        // Undecorable explicit slug: normalization leaves nothing usable.
        (
            serde_json::json!({ "name": "ok", "slug": "---***---" }),
            "slug",
        ),
        (serde_json::json!({ "name": "---***---" }), "name"),
        (
            serde_json::json!({ "name": "ok", "description": "x".repeat(10_001) }),
            "description",
        ),
    ];
    for (body, field) in cases {
        let response = request(
            app.clone(),
            "POST",
            &projects_path(org, workspace),
            Some(&token),
            Some(body),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}"
        );
        let error = body_json(response).await;
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
        assert!(
            error["error"]["details"]["fields"][field].is_array(),
            "field {field} must be reported"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0, "invalid creations must write zero rows");
    Ok(())
}

// ---------------------------------------------------------------------------
// Slug uniqueness + concurrency
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn slug_conflicts_are_case_insensitive_within_one_workspace(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-slug@example.test").await;
    let token = login_token(app.clone(), "proj-slug@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Slug Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Slug Ws").await;
    create_project_via_api(app.clone(), &token, org, workspace, "Alpha").await;

    // Same slug, different case: citext makes the unique key case-insensitive.
    let response = request(
        app.clone(),
        "POST",
        &projects_path(org, workspace),
        Some(&token),
        Some(serde_json::json!({ "name": "Different", "slug": "ALPHA" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = body_json(response).await;
    assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    assert!(error["error"]["details"]["fields"]["slug"].is_array());

    // PATCH into a conflicting slug fails the same way.
    let project = create_project_via_api(app.clone(), &token, org, workspace, "Beta").await;
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "slug": "alpha" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_slug_is_allowed_across_different_workspaces(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-cross@example.test").await;
    let token = login_token(app.clone(), "proj-cross@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Cross Org").await;
    let ws_a = create_workspace_via_api(app.clone(), &token, org, "Line A").await;
    let ws_b = create_workspace_via_api(app.clone(), &token, org, "Line B").await;

    create_project_via_api(app.clone(), &token, org, ws_a, "Shared").await;
    create_project_via_api(app.clone(), &token, org, ws_b, "Shared").await;

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE slug = 'shared'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        count, 2,
        "same slug must be allowed in different workspaces"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_same_slug_creation_has_a_single_winner(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-race@example.test").await;
    let token = login_token(app.clone(), "proj-race@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Race Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Race Ws").await;

    let path = projects_path(org, workspace);
    let (first, second) = tokio::join!(
        request(
            app.clone(),
            "POST",
            &path,
            Some(&token),
            Some(serde_json::json!({ "name": "One", "slug": "clash" })),
        ),
        request(
            app.clone(),
            "POST",
            &path,
            Some(&token),
            Some(serde_json::json!({ "name": "Two", "slug": "clash" })),
        ),
    );
    let statuses = [first.status(), second.status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CREATED)
            .count(),
        1,
        "exactly one concurrent create may win: {statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::UNPROCESSABLE_ENTITY)
            .count(),
        1
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE slug = 'clash'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Cross-tenant / wrong-parent isolation matrix
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn foreign_project_access_is_indistinguishable_from_unknown(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-a@example.test").await;
    create_user(&pool, "proj-b@example.test").await;
    let token_a = login_token(app.clone(), "proj-a@example.test").await;
    let token_b = login_token(app.clone(), "proj-b@example.test").await;
    let org_a = create_org_via_api(app.clone(), &token_a, "Tenant A").await;
    create_org_via_api(app.clone(), &token_b, "Tenant B").await;
    let ws_a = create_workspace_via_api(app.clone(), &token_a, org_a, "A Ws").await;
    let project = create_project_via_api(app.clone(), &token_a, org_a, ws_a, "Secret").await;

    // B is a legitimate member of its own tenant but must learn NOTHING about
    // A's project: foreign GET/PATCH == random-UUID GET/PATCH in every byte
    // except the request id.
    for method in ["GET", "PATCH"] {
        let body = (method == "PATCH").then(|| serde_json::json!({ "name": "Hijack" }));
        let foreign = request_with_extra_cookies(
            app.clone(),
            method,
            &project_item_path(org_a, ws_a, project),
            Some(&token_b),
            body.clone(),
            &[],
        )
        .await;
        let unknown = request(
            app.clone(),
            method,
            &project_item_path(org_a, ws_a, Uuid::now_v7()),
            Some(&token_b),
            body,
        )
        .await;
        assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
        assert_eq!(foreign.status(), unknown.status());
        assert_eq!(
            error_fingerprint(foreign).await,
            error_fingerprint(unknown).await,
            "foreign and unknown must be indistinguishable"
        );
    }

    // Mutation attempts created/changed zero rows.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE name = 'Hijack'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn mismatched_parent_combinations_fail_even_for_multi_workspace_members(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-swap@example.test").await;
    let token = login_token(app.clone(), "proj-swap@example.test").await;
    let org_1 = create_org_via_api(app.clone(), &token, "Swap Org One").await;
    let org_2 = create_org_via_api(app.clone(), &token, "Swap Org Two").await;
    let ws_1 = create_workspace_via_api(app.clone(), &token, org_1, "Ws One").await;
    let ws_2 = create_workspace_via_api(app.clone(), &token, org_2, "Ws Two").await;
    let project = create_project_via_api(app.clone(), &token, org_1, ws_1, "P").await;

    // The SAME user legitimately belongs to everything involved; a project of
    // ws_1 must still not resolve under ws_2, nor ws_1 under org_2.
    for uri in [
        project_item_path(org_1, ws_2, project), // workspace swap
        project_item_path(org_2, ws_1, project), // organization swap
        project_item_path(org_2, ws_2, project), // both swapped
    ] {
        let response = request(app.clone(), "GET", &uri, Some(&token), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        let response = request(
            app.clone(),
            "PATCH",
            &uri,
            Some(&token),
            Some(serde_json::json!({ "name": "Swapped" })),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
    // And the workspace-swap list never contains the foreign project.
    let response = request(
        app.clone(),
        "GET",
        &projects_path(org_2, ws_2),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await["data"].as_array().map(Vec::len),
        Some(0)
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_and_unknown_ids_share_identical_404_semantics(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-mal@example.test").await;
    let token = login_token(app.clone(), "proj-mal@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Malformed Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Malformed Ws").await;

    let unknown = request(
        app.clone(),
        "GET",
        &format!(
            "/api/v1/organizations/{org}/workspaces/{workspace}/projects/{}",
            Uuid::now_v7()
        ),
        Some(&token),
        None,
    )
    .await;
    let malformed = request(
        app.clone(),
        "GET",
        &format!("/api/v1/organizations/{org}/workspaces/{workspace}/projects/not-a-uuid"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    assert_eq!(malformed.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        error_fingerprint(malformed).await,
        error_fingerprint(unknown).await
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn membership_matrix_gates_every_project_route(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-admin@example.test").await;
    create_user(&pool, "proj-outsider@example.test").await;
    let token = login_token(app.clone(), "proj-admin@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Matrix Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Matrix Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "M").await;
    let outsider = user_id_by_email(&pool, "proj-outsider@example.test").await;

    // Case 1: org member WITHOUT workspace membership.
    add_org_membership(&pool, org, outsider).await;
    for (method, uri, body) in [
        ("GET", projects_path(org, workspace), None),
        ("GET", project_item_path(org, workspace, project), None),
        (
            "POST",
            projects_path(org, workspace),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
        (
            "PATCH",
            project_item_path(org, workspace, project),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
    ] {
        let response = request(app.clone(), method, &uri, None, body).await; // no session
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method}");
    }
    // Now with a session: uniform 404 (no existence leak).
    let token_outsider = login_token(app.clone(), "proj-outsider@example.test").await;
    for (method, uri, body) in [
        ("GET", projects_path(org, workspace), None),
        ("GET", project_item_path(org, workspace, project), None),
        (
            "POST",
            projects_path(org, workspace),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
        (
            "PATCH",
            project_item_path(org, workspace, project),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
    ] {
        let response = request(app.clone(), method, &uri, Some(&token_outsider), body).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
    }

    // Case 2: workspace membership row EXISTS but the organization membership
    // is soft-deleted afterwards — the abnormal row must stay inert.
    add_ws_membership(&pool, org, workspace, outsider).await;
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(outsider)
    .execute(&pool)
    .await?;
    for (method, uri, body) in [
        ("GET", projects_path(org, workspace), None),
        ("GET", project_item_path(org, workspace, project), None),
        (
            "PATCH",
            project_item_path(org, workspace, project),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
    ] {
        let response = request(app.clone(), method, &uri, Some(&token_outsider), body).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn hostile_context_cookies_do_not_affect_authorization(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-cookie@example.test").await;
    create_user(&pool, "proj-victim@example.test").await;
    let token = login_token(app.clone(), "proj-cookie@example.test").await;
    let victim_token = login_token(app.clone(), "proj-victim@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Cookie Org").await;
    let victim_org = create_org_via_api(app.clone(), &victim_token, "Victim Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Cookie Ws").await;
    let victim_ws =
        create_workspace_via_api(app.clone(), &victim_token, victim_org, "Victim Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "C").await;

    // Attacker sends the victim's org/workspace as UX cookies; the route is
    // authoritative, so nothing changes versus the cookie-less request.
    for uri in [
        projects_path(org, workspace),
        project_item_path(org, workspace, project),
    ] {
        let clean = request(app.clone(), "GET", &uri, Some(&token), None).await;
        let hostile = request_with_extra_cookies(
            app.clone(),
            "GET",
            &uri,
            Some(&token),
            None,
            &[
                format!("organization={victim_org}"),
                format!("workspace={victim_ws}"),
            ],
        )
        .await;
        assert_eq!(clean.status(), hostile.status());
    }
    // The hostile cookies do not grant access to the victim workspace either.
    let response = request_with_extra_cookies(
        app.clone(),
        "GET",
        &projects_path(victim_org, victim_ws),
        Some(&token),
        None,
        &[
            format!("organization={victim_org}"),
            format!("workspace={victim_ws}"),
        ],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn body_fields_cannot_override_route_ownership(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-own@example.test").await;
    create_user(&pool, "proj-other@example.test").await;
    let token = login_token(app.clone(), "proj-own@example.test").await;
    let other_token = login_token(app.clone(), "proj-other@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Own Org").await;
    let other_org = create_org_via_api(app.clone(), &other_token, "Other Org").await;
    let other_ws = create_workspace_via_api(app.clone(), &other_token, other_org, "Other Ws").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Own Ws").await;

    // Create claims foreign ownership in the body; the ignored fields must
    // not move the project out of the route's workspace.
    let response = request(
        app.clone(),
        "POST",
        &projects_path(org, workspace),
        Some(&token),
        Some(serde_json::json!({
            "name": "Owns",
            "tenant_id": other_org.to_string(),
            "organization_id": other_org.to_string(),
            "workspace_id": other_ws.to_string(),
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let stored: (Uuid, Uuid) =
        sqlx::query_as("SELECT tenant_id, workspace_id FROM projects WHERE name = 'Owns'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        stored,
        (org, workspace),
        "ownership must come from the route"
    );

    // PATCH attempts to move ownership: fields are not even part of the DTO.
    let project = create_project_via_api(app.clone(), &token, org, workspace, "Movable").await;
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({
            "workspace_id": other_ws.to_string(),
            "tenant_id": other_org.to_string(),
            "id": Uuid::now_v7().to_string(),
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let stored: (Uuid, Uuid) =
        sqlx::query_as("SELECT tenant_id, workspace_id FROM projects WHERE id = $1")
            .bind(project)
            .fetch_one(&pool)
            .await?;
    assert_eq!(stored, (org, workspace));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_rejects_cross_tenant_and_duplicate_project_rows(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-db@example.test").await;
    let token = login_token(app.clone(), "proj-db@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Db Org").await;
    let other_org = create_org_via_api(app.clone(), &token, "Db Org Two").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Db Ws").await;
    let other_ws = create_workspace_via_api(app.clone(), &token, other_org, "Db Ws Two").await;

    // tenant_id of org A + workspace of org B: the composite FK must refuse.
    let result = sqlx::query(
        "INSERT INTO projects (id, tenant_id, workspace_id, name, slug) \
         VALUES ($1, $2, $3, 'Bad', 'bad')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(other_ws)
    .execute(&pool)
    .await;
    assert!(result.is_err(), "cross-tenant project row must be rejected");

    // Duplicate slug inside one workspace (citext, direct DB insert).
    create_project_via_api(app.clone(), &token, org, workspace, "Dup").await;
    let result = sqlx::query(
        "INSERT INTO projects (id, tenant_id, workspace_id, name, slug) \
         VALUES ($1, $2, $3, 'Dup Two', 'DUP')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(workspace)
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "case-insensitive duplicate must be rejected"
    );

    // Unknown status is impossible at the DB level too.
    let result = sqlx::query(
        "INSERT INTO projects (id, tenant_id, workspace_id, name, slug, status) \
         VALUES ($1, $2, $3, 'Bad', 'bad-status', 'paused')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(workspace)
    .execute(&pool)
    .await;
    assert!(result.is_err(), "unknown status must violate the CHECK");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_excludes_deleted_rows_and_orders_newest_first(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-list@example.test").await;
    let token = login_token(app.clone(), "proj-list@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "List Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "List Ws").await;
    create_project_via_api(app.clone(), &token, org, workspace, "Oldest").await;
    create_project_via_api(app.clone(), &token, org, workspace, "Middle").await;
    create_project_via_api(app.clone(), &token, org, workspace, "Newest").await;

    // Soft-delete the middle row directly (delete endpoint is deferred).
    sqlx::query("UPDATE projects SET deleted_at = now() WHERE name = 'Middle'")
        .execute(&pool)
        .await?;

    let response = request(
        app.clone(),
        "GET",
        &projects_path(org, workspace),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let data = body_json(response).await["data"].clone();
    let names: Vec<&str> = data
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"))
        .iter()
        .map(|item| item["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(names, vec!["Newest", "Oldest"]);

    // A deleted project no longer resolves singly, and its error body is
    // indistinguishable from a never-existing project id (no existence leak
    // through the delete state).
    let deleted: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE name = 'Middle'")
        .fetch_one(&pool)
        .await?;
    let deleted_response = request(
        app.clone(),
        "GET",
        &project_item_path(org, workspace, deleted),
        Some(&token),
        None,
    )
    .await;
    let unknown_response = request(
        app,
        "GET",
        &project_item_path(org, workspace, Uuid::now_v7()),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(deleted_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        error_fingerprint(deleted_response).await,
        error_fingerprint(unknown_response).await,
        "deleted and unknown projects must be indistinguishable"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn unauthenticated_project_requests_are_rejected(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-auth@example.test").await;
    let token = login_token(app.clone(), "proj-auth@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Auth Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Auth Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "A").await;

    for (method, uri, body) in [
        ("GET", projects_path(org, workspace), None),
        (
            "POST",
            projects_path(org, workspace),
            Some(serde_json::json!({ "name": "X" })),
        ),
        ("GET", project_item_path(org, workspace, project), None),
        (
            "PATCH",
            project_item_path(org, workspace, project),
            Some(serde_json::json!({ "name": "X" })),
        ),
    ] {
        let response = request(app.clone(), method, &uri, None, body).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method}");
        assert_eq!(body_json(response).await["error"]["code"], "AUTH_REQUIRED");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RBAC matrix
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn member_reads_but_cannot_mutate_until_granted(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-rbowner@example.test").await;
    create_user(&pool, "proj-rbmember@example.test").await;
    let token = login_token(app.clone(), "proj-rbowner@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Rbac Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Rbac Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "R").await;
    let member = user_id_by_email(&pool, "proj-rbmember@example.test").await;
    add_org_membership(&pool, org, member).await;
    add_ws_membership(&pool, org, workspace, member).await;
    let member_token = login_token(app.clone(), "proj-rbmember@example.test").await;

    // Reads are eligibility-based: a plain member sees the workspace projects.
    let response = request(
        app.clone(),
        "GET",
        &projects_path(org, workspace),
        Some(&member_token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await["data"].as_array().map(Vec::len),
        Some(1)
    );
    let response = request(
        app.clone(),
        "GET",
        &project_item_path(org, workspace, project),
        Some(&member_token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Mutations are 403 (eligible but unprivileged) — never 404.
    for (method, uri, body) in [
        (
            "POST",
            projects_path(org, workspace),
            Some(serde_json::json!({ "name": "Nope" })),
        ),
        (
            "PATCH",
            project_item_path(org, workspace, project),
            Some(serde_json::json!({ "name": "Nope" })),
        ),
        (
            "PATCH",
            project_item_path(org, workspace, project),
            Some(serde_json::json!({ "status": "archived" })),
        ),
    ] {
        let response = request(app.clone(), method, &uri, Some(&member_token), body).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
        assert_eq!(
            body_json(response).await["error"]["code"],
            "PERMISSION_DENIED"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1, "member mutations must write zero rows");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn archive_permission_gates_only_transitions_into_archived(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-arch@example.test").await;
    let token = login_token(app.clone(), "proj-arch@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Archive Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Archive Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "Arch").await;
    let user = user_id_by_email(&pool, "proj-arch@example.test").await;

    // Strip the owner's archive grant: update must keep working, archiving
    // (even indirectly through PATCH) must not.
    sqlx::query(
        "DELETE FROM role_permissions rp USING permissions p \
         WHERE rp.permission_id = p.id AND p.key = 'projects:archive'",
    )
    .execute(&pool)
    .await?;

    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "name": "Renamed" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "status": "archived" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // Non-archive transitions stay permitted for update-only.
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "status": "completed" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Re-grant archive through a workspace-scoped role: now archiving works,
    // proving projects:archive is a real, reachable enforcement point.
    grant_workspace_scoped(&pool, org, workspace, user, &["projects:archive"]).await;
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "status": "archived" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["data"]["status"], "archived");
    // Unarchiving (archived -> active) needs update only.
    let response = request(
        app.clone(),
        "PATCH",
        &project_item_path(org, workspace, project),
        Some(&token),
        Some(serde_json::json!({ "status": "active" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_scoped_grant_cannot_escape_its_workspace(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-wsrole@example.test").await;
    create_user(&pool, "proj-wsbound@example.test").await;
    let token = login_token(app.clone(), "proj-wsrole@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Ws Role Org").await;
    let ws_a = create_workspace_via_api(app.clone(), &token, org, "Bound Ws").await;
    let ws_b = create_workspace_via_api(app.clone(), &token, org, "Other Ws").await;

    // A NON-owner member of both workspaces gets the grant bound to ws_a
    // only; the organization-wide Owner assignment of the creator must not
    // interfere with the scope being tested.
    let bound = user_id_by_email(&pool, "proj-wsbound@example.test").await;
    add_org_membership(&pool, org, bound).await;
    add_ws_membership(&pool, org, ws_a, bound).await;
    add_ws_membership(&pool, org, ws_b, bound).await;
    grant_workspace_scoped(&pool, org, ws_a, bound, &["projects:create"]).await;
    let bound_token = login_token(app.clone(), "proj-wsbound@example.test").await;

    let response = request(
        app.clone(),
        "POST",
        &projects_path(org, ws_a),
        Some(&bound_token),
        Some(serde_json::json!({ "name": "In Scope" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = request(
        app.clone(),
        "POST",
        &projects_path(org, ws_b),
        Some(&bound_token),
        Some(serde_json::json!({ "name": "Out Of Scope" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE workspace_id = $1")
        .bind(ws_b)
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn migration_backfills_owner_grants_without_assigning_owner_roles(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-backfill@example.test").await;
    let token = login_token(app.clone(), "proj-backfill@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Backfill Org").await;

    // Roles exist WITHOUT any project grant once 007 is reverted — the exact
    // state a pre-007 organization is in when the migration first runs. The
    // pre-007 version is located BY NAME so later migrations (e.g.
    // 008_sections) cannot shift a positional revert target.
    let up_migrations: Vec<(i64, std::borrow::Cow<'_, str>)> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| (m.version, m.description.clone()))
        .collect();
    let projects_index = up_migrations
        .iter()
        .position(|(_, description)| description.contains("projects"))
        .unwrap_or_else(|| panic!("the projects migration must exist in the chain"));
    MIGRATOR
        .undo(&pool, up_migrations[projects_index - 1].0)
        .await?;
    let grant_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp JOIN permissions p ON p.id = rp.permission_id \
         WHERE rp.role_id IN (SELECT id FROM roles WHERE tenant_id = $1) AND p.key LIKE 'projects:%'",
    )
    .bind(org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(grant_count, 0, "revert must remove all project grants");

    // Re-applying 007 must grant the new keys to the EXISTING owner role...
    MIGRATOR.run(&pool).await?;
    let owner_grants: Vec<String> = sqlx::query_scalar(
        "SELECT p.key FROM role_permissions rp \
         JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.tenant_id = $1 AND r.is_system AND r.name = 'owner' AND p.key LIKE 'projects:%' \
         ORDER BY p.key",
    )
    .bind(org)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        owner_grants,
        vec!["projects:archive", "projects:create", "projects:update"],
        "migration backfill must extend the Owner grant list"
    );
    // ...but never grant mutation permissions to the built-in Member role...
    let member_grants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp \
         JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.tenant_id = $1 AND r.is_system AND r.name = 'member' AND p.key LIKE 'projects:%'",
    )
    .bind(org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        member_grants, 0,
        "built-in Member must keep zero project grants"
    );
    // ...and never assign the Owner ROLE to any user (step 13 decision).
    let assignments: i64 =
        sqlx::query_scalar("SELECT count(*) FROM membership_roles WHERE tenant_id = $1")
            .bind(org)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        assignments, 1,
        "migration must not create role assignments (only bootstrap did)"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// TOCTOU: revocations racing the mutation transaction
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn membership_revocation_race_blocks_create_and_update(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-toctou@example.test").await;
    let token = login_token(app.clone(), "proj-toctou@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Toctou Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Toctou Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "T").await;
    let user = user_id_by_email(&pool, "proj-toctou@example.test").await;

    // Simulate a workspace-membership revoke landing between request-context
    // resolution and the mutation transaction: the service must deny and
    // write nothing (deterministic state transition, no timing guesses).
    platform_server::rbac::deactivate_workspace_membership_with_assignments(
        &pool, org, workspace, user,
    )
    .await?;
    let created = projects::create_project(
        &pool,
        org,
        workspace,
        &NewProject {
            name: "Raced".into(),
            slug: "raced".into(),
            description: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(ProjectError::NotAccessible)));
    let updated = projects::update_project(
        &pool,
        org,
        workspace,
        project,
        &ProjectPatch {
            name: Some("Raced".into()),
            ..ProjectPatch::default()
        },
        user,
    )
    .await;
    assert!(matches!(updated, Err(ProjectError::NotAccessible)));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE name = 'Raced'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0);
    let name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id = $1")
        .bind(project)
        .fetch_one(&pool)
        .await?;
    assert_eq!(name, "T");

    // Same revoke over the ORGANIZATION membership: equally denied. The
    // org-wide Owner assignment is untouched by the workspace-scoped
    // lifecycle above, so this deactivation is the first one to remove it.
    platform_server::rbac::reactivate_workspace_membership(&pool, workspace, user).await?;
    platform_server::rbac::deactivate_membership_with_assignments(&pool, org, user).await?;
    let created = projects::create_project(
        &pool,
        org,
        workspace,
        &NewProject {
            name: "Raced Two".into(),
            slug: "raced-two".into(),
            description: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(ProjectError::NotAccessible)));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn permission_revocation_race_blocks_create_and_update(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-prace@example.test").await;
    let token = login_token(app.clone(), "proj-prace@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "PRace Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "PRace Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "P").await;
    let user = user_id_by_email(&pool, "proj-prace@example.test").await;

    // Revoke projects:create at the grant level; the transaction must deny.
    sqlx::query(
        "DELETE FROM role_permissions rp USING permissions p \
         WHERE rp.permission_id = p.id AND p.key = 'projects:create'",
    )
    .execute(&pool)
    .await?;
    let created = projects::create_project(
        &pool,
        org,
        workspace,
        &NewProject {
            name: "Denied".into(),
            slug: "denied".into(),
            description: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(ProjectError::Forbidden)));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE name = 'Denied'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0);

    // Revoke the role ASSIGNMENT entirely; update must deny the same way.
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await?;
    let updated = projects::update_project(
        &pool,
        org,
        workspace,
        project,
        &ProjectPatch {
            name: Some("Denied".into()),
            ..ProjectPatch::default()
        },
        user,
    )
    .await;
    assert!(matches!(updated, Err(ProjectError::Forbidden)));
    let name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id = $1")
        .bind(project)
        .fetch_one(&pool)
        .await?;
    assert_eq!(name, "P");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_lifecycle_race_blocks_project_mutation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-wsdel@example.test").await;
    let token = login_token(app.clone(), "proj-wsdel@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "WsDel Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "WsDel Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, workspace, "W").await;
    let user = user_id_by_email(&pool, "proj-wsdel@example.test").await;

    // Workspace soft-deleted after context resolution: the mutation must
    // fail on the transaction snapshot, and reads stop resolving too.
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(workspace)
        .execute(&pool)
        .await?;
    let created = projects::create_project(
        &pool,
        org,
        workspace,
        &NewProject {
            name: "Ghost".into(),
            slug: "ghost".into(),
            description: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(ProjectError::NotAccessible)));
    let updated = projects::update_project(
        &pool,
        org,
        workspace,
        project,
        &ProjectPatch::default(),
        user,
    )
    .await;
    assert!(matches!(updated, Err(ProjectError::NotAccessible)));
    let response = request(
        app.clone(),
        "GET",
        &project_item_path(org, workspace, project),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

// ---------------------------------------------------------------------------
// Privilege resurrection regression (project-specific path)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_scoped_project_grants_do_not_resurrect_on_reactivation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "proj-res@example.test").await;
    let token = login_token(app.clone(), "proj-res@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Res Org").await;
    let workspace = create_workspace_via_api(app.clone(), &token, org, "Res Ws").await;
    let user = user_id_by_email(&pool, "proj-res@example.test").await;

    // Workspace-scoped create grant; verified working, then the membership is
    // deactivated (assignments physically deleted) and reactivated: the grant
    // must NOT come back (ADR 0008 invariant, project-specific evidence).
    grant_workspace_scoped(&pool, org, workspace, user, &["projects:create"]).await;
    // Remove the owner-wide assignment so ONLY the workspace grant can allow.
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND workspace_id IS NULL")
        .bind(org)
        .execute(&pool)
        .await?;
    let allowed =
        platform_server::rbac::authorize_workspace(&pool, org, workspace, user, PROJECTS_CREATE)
            .await?;
    assert!(allowed, "workspace-scoped grant must work while assigned");

    platform_server::rbac::deactivate_workspace_membership_with_assignments(
        &pool, org, workspace, user,
    )
    .await?;
    platform_server::rbac::reactivate_workspace_membership(&pool, workspace, user).await?;
    let allowed =
        platform_server::rbac::authorize_workspace(&pool, org, workspace, user, PROJECTS_CREATE)
            .await?;
    assert!(
        !allowed,
        "reactivation must not resurrect workspace-scoped project grants"
    );
    let created = projects::create_project(
        &pool,
        org,
        workspace,
        &NewProject {
            name: "Resurrect".into(),
            slug: "resurrect".into(),
            description: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(ProjectError::Forbidden)));
    Ok(())
}
