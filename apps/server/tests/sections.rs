use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState,
    config::AuthConfig,
    migrations::MIGRATOR,
    password::PasswordService,
    router,
    sections::{self, NewSection, ParentPatch, SectionError, SectionPatch},
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
    let bytes = to_bytes(response.into_body(), 1_048_576)
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
    ws: Uuid,
    name: &str,
) -> Uuid {
    let response = request(
        router,
        "POST",
        &format!("/api/v1/organizations/{org}/workspaces/{ws}/projects"),
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

async fn create_section_via_api(
    router: axum::Router,
    token: &str,
    org: Uuid,
    ws: Uuid,
    project: Uuid,
    name: &str,
    parent: Option<Uuid>,
) -> Uuid {
    let response = request(
        router,
        "POST",
        &sections_path(org, ws, project),
        Some(token),
        Some(serde_json::json!({ "name": name, "parent_section_id": parent })),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "section '{name}' must be creatable"
    );
    body_json(response).await["data"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("section id must parse"))
}

fn sections_path(org: Uuid, ws: Uuid, project: Uuid) -> String {
    format!("/api/v1/organizations/{org}/workspaces/{ws}/projects/{project}/sections")
}

fn section_item_path(org: Uuid, ws: Uuid, project: Uuid, section: Uuid) -> String {
    format!("{}/{}", sections_path(org, ws, project), section)
}

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

async fn grant_workspace_scoped(
    pool: &PgPool,
    tenant: Uuid,
    workspace: Uuid,
    user: Uuid,
    keys: &[&str],
) {
    let role: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (id, tenant_id, name, is_system) \
         VALUES ($1, $2, 'ws-section-role', false) RETURNING id",
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
// Creation, depth, ordering, slug policy
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn owner_creates_root_and_arbitrarily_deep_sections(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-owner@example.test").await;
    let token = login_token(app.clone(), "sec-owner@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Sections Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Sections Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "Tower").await;

    // Depth-5 chain: the generic entity carries any business meaning.
    let mut parent: Option<Uuid> = None;
    let mut chain = Vec::new();
    for name in ["A Blok", "Kat 1", "Daire 1", "Oda", "Balkon"] {
        let id = create_section_via_api(app.clone(), &token, org, ws, project, name, parent).await;
        chain.push((name, id));
        parent = Some(id);
    }
    for (expected_index, (name, id)) in chain.iter().enumerate() {
        let response = request(
            app.clone(),
            "GET",
            &section_item_path(org, ws, project, *id),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["name"], *name);
        assert_eq!(body["project_id"], project.to_string());
        let expected_parent = if expected_index == 0 {
            serde_json::Value::Null
        } else {
            chain[expected_index - 1].1.to_string().into()
        };
        assert_eq!(body["parent_section_id"], expected_parent);
    }
    let response = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(
        body_json(response).await["data"].as_array().map(Vec::len),
        Some(5)
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn flat_list_is_deterministically_ordered(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-order@example.test").await;
    let token = login_token(app.clone(), "sec-order@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Order Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Order Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;

    let root_b =
        create_section_via_api(app.clone(), &token, org, ws, project, "Root B", None).await;
    let root_a =
        create_section_via_api(app.clone(), &token, org, ws, project, "Root A", None).await;
    let child_b1 = create_section_via_api(
        app.clone(),
        &token,
        org,
        ws,
        project,
        "Child B1",
        Some(root_b),
    )
    .await;
    let child_a1 = create_section_via_api(
        app.clone(),
        &token,
        org,
        ws,
        project,
        "Child A1",
        Some(root_a),
    )
    .await;

    // Creation order defines positions; the flat list groups by parent
    // (roots first) and orders siblings by (position, id).
    let response = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    let data = body_json(response).await["data"].clone();
    let ids: Vec<String> = data
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"))
        .iter()
        .map(|item| item["id"].as_str().unwrap_or_default().to_owned())
        .collect();
    // Documented flat order (ADR 0012): roots first by (position, id) — root_b
    // was created first, so it holds position 0 — then children grouped by
    // parent id with (position, id) inside each group. Deterministic; the
    // client tree builder sorts siblings itself.
    let expected: Vec<String> = vec![root_b, root_a, child_b1, child_a1]
        .into_iter()
        .map(|id| id.to_string())
        .collect();
    assert_eq!(ids, expected, "flat list must follow the documented order");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn sibling_slug_conflicts_are_rejected_case_insensitively(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-slug@example.test").await;
    let token = login_token(app.clone(), "sec-slug@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Slug Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Slug Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let floor =
        create_section_via_api(app.clone(), &token, org, ws, project, "Floor 1", None).await;
    create_section_via_api(
        app.clone(),
        &token,
        org,
        ws,
        project,
        "Apartment 1",
        Some(floor),
    )
    .await;

    // Same slug, different case, same parent -> citext conflict.
    let response = request(
        app.clone(),
        "POST",
        &sections_path(org, ws, project),
        Some(&token),
        Some(
            serde_json::json!({ "name": "Different", "slug": "APARTMENT 1", "parent_section_id": floor }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "VALIDATION_ERROR"
    );

    // PATCH into a conflicting slug under the same parent fails too.
    let other = create_section_via_api(
        app.clone(),
        &token,
        org,
        ws,
        project,
        "Apartment 2",
        Some(floor),
    )
    .await;
    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, other),
        Some(&token),
        Some(serde_json::json!({ "slug": "apartment-1" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Two roots with the same slug collide (root partial unique).
    create_section_via_api(app.clone(), &token, org, ws, project, "Floor 2", None).await;
    let response = request(
        app,
        "POST",
        &sections_path(org, ws, project),
        Some(&token),
        Some(serde_json::json!({ "name": "Other", "slug": "floor-2" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_slug_and_name_are_allowed_in_different_sibling_scopes(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-scope@example.test").await;
    let token = login_token(app.clone(), "sec-scope@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Scope Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Scope Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let floor1 =
        create_section_via_api(app.clone(), &token, org, ws, project, "Floor 1", None).await;
    let floor2 =
        create_section_via_api(app.clone(), &token, org, ws, project, "Floor 2", None).await;

    // "Daire 1" under both floors: same name AND derived slug, different parents.
    create_section_via_api(
        app.clone(),
        &token,
        org,
        ws,
        project,
        "Daire 1",
        Some(floor1),
    )
    .await;
    create_section_via_api(
        app.clone(),
        &token,
        org,
        ws,
        project,
        "Daire 1",
        Some(floor2),
    )
    .await;
    // A root may share a slug with a child of another parent.
    create_section_via_api(app.clone(), &token, org, ws, project, "Daire 1", None).await;

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE slug = 'daire-1'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 3, "sibling-scoped uniqueness must allow all three");
    Ok(())
}

// ---------------------------------------------------------------------------
// Parent integrity: cross-project/tenant rejection at API and DB level
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn foreign_project_parent_is_rejected_at_creation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-fp@example.test").await;
    let token = login_token(app.clone(), "sec-fp@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Fp Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Fp Ws").await;
    let project_a = create_project_via_api(app.clone(), &token, org, ws, "Project A").await;
    let project_b = create_project_via_api(app.clone(), &token, org, ws, "Project B").await;
    let foreign_root =
        create_section_via_api(app.clone(), &token, org, ws, project_b, "B Root", None).await;

    // Parent from ANOTHER project under project A: the server resolves the
    // parent inside project A's boundary only, so it is simply invalid.
    let response = request(
        app.clone(),
        "POST",
        &sections_path(org, ws, project_a),
        Some(&token),
        Some(serde_json::json!({ "name": "A Child", "parent_section_id": foreign_root })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = body_json(response).await;
    assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    assert!(error["error"]["details"]["fields"]["parent_section_id"].is_array());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE project_id = $1")
        .bind(project_a)
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0, "rejected create must write zero rows");

    // Reparenting a section of project A under project B's section: the
    // section routes through project A, so the parent is invalid there.
    let a_root =
        create_section_via_api(app.clone(), &token, org, ws, project_a, "A Root", None).await;
    let response = request(
        app,
        "PATCH",
        &section_item_path(org, ws, project_a, a_root),
        Some(&token),
        Some(serde_json::json!({ "parent_section_id": foreign_root })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_parent_route_combinations_fail_even_for_multi_project_members(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-attack@example.test").await;
    let token = login_token(app.clone(), "sec-attack@example.test").await;
    let org_1 = create_org_via_api(app.clone(), &token, "Attack Org One").await;
    let org_2 = create_org_via_api(app.clone(), &token, "Attack Org Two").await;
    let ws_1 = create_workspace_via_api(app.clone(), &token, org_1, "Ws One").await;
    let ws_2 = create_workspace_via_api(app.clone(), &token, org_2, "Ws Two").await;
    let project_1 = create_project_via_api(app.clone(), &token, org_1, ws_1, "P One").await;
    let project_2 = create_project_via_api(app.clone(), &token, org_2, ws_2, "P Two").await;
    let section =
        create_section_via_api(app.clone(), &token, org_1, ws_1, project_1, "S", None).await;

    // The SAME user legitimately owns EVERYTHING involved; the section of
    // project_1 must not resolve under any wrong parent combination.
    for (org, ws, project) in [
        (org_1, ws_1, project_2), // project swap, same ws
        (org_2, ws_1, project_1), // org swap
        (org_2, ws_2, project_1), // both swapped
    ] {
        let uri = section_item_path(org, ws, project, section);
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
    // The wrong-project list never contains the foreign section.
    let response = request(
        app,
        "GET",
        &sections_path(org_2, ws_2, project_2),
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
async fn database_rejects_cross_project_and_cross_tenant_parent_rows(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-db@example.test").await;
    let token = login_token(app.clone(), "sec-db@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Db Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Db Ws").await;
    let project_a = create_project_via_api(app.clone(), &token, org, ws, "Db A").await;
    let project_b = create_project_via_api(app.clone(), &token, org, ws, "Db B").await;
    let parent_b =
        create_section_via_api(app.clone(), &token, org, ws, project_b, "Parent B", None).await;

    // Child claims project A but parent belongs to project B: rejected.
    let result = sqlx::query(
        "INSERT INTO sections (id, tenant_id, workspace_id, project_id, parent_section_id, name, slug, position) \
         VALUES ($1, $2, $3, $4, $5, 'Bad', 'bad', 0)",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(ws)
    .bind(project_a)
    .bind(parent_b)
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "cross-project parent row must be rejected by the composite FK"
    );
    // Cross-tenant project triple: rejected by the projects composite FK.
    let result = sqlx::query(
        "INSERT INTO sections (id, tenant_id, workspace_id, project_id, name, slug, position) \
         VALUES ($1, $2, $3, $4, 'Bad', 'bad', 0)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(ws)
    .bind(project_a)
    .execute(&pool)
    .await;
    assert!(result.is_err(), "cross-tenant section row must be rejected");
    // Unknown status cannot be stored either.
    let result = sqlx::query(
        "INSERT INTO sections (id, tenant_id, workspace_id, project_id, name, slug, position, status) \
         VALUES ($1, $2, $3, $4, 'Bad', 'bad', 0, 'completed')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(ws)
    .bind(project_a)
    .execute(&pool)
    .await;
    assert!(result.is_err(), "unknown status must violate the CHECK");
    Ok(())
}

// ---------------------------------------------------------------------------
// Cycle prevention and move semantics
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn self_parent_and_descendant_cycle_moves_are_rejected(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-cycle@example.test").await;
    let token = login_token(app.clone(), "sec-cycle@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Cycle Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Cycle Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let a = create_section_via_api(app.clone(), &token, org, ws, project, "A", None).await;
    let b = create_section_via_api(app.clone(), &token, org, ws, project, "B", Some(a)).await;
    let c = create_section_via_api(app.clone(), &token, org, ws, project, "C", Some(b)).await;

    for (label, target) in [("self", a), ("direct child", b), ("grandchild", c)] {
        let response = request(
            app.clone(),
            "PATCH",
            &section_item_path(org, ws, project, a),
            Some(&token),
            Some(serde_json::json!({ "parent_section_id": target })),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "moving A under {label} must be rejected"
        );
        let error = body_json(response).await;
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
        assert!(
            error["error"]["details"]["fields"]["parent_section_id"].is_array(),
            "cycle must map to the stable parent-field validation semantic"
        );
    }
    // The tree is unchanged after all rejected attempts.
    let b_parent: Option<Uuid> =
        sqlx::query_scalar("SELECT parent_section_id FROM sections WHERE id = $1")
            .bind(b)
            .fetch_one(&pool)
            .await?;
    assert_eq!(b_parent, Some(a));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn root_to_child_move_carries_the_whole_subtree(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-move@example.test").await;
    let token = login_token(app.clone(), "sec-move@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Move Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Move Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let a = create_section_via_api(app.clone(), &token, org, ws, project, "A", None).await;
    let a1 = create_section_via_api(app.clone(), &token, org, ws, project, "A1", Some(a)).await;
    let _a2 = create_section_via_api(app.clone(), &token, org, ws, project, "A2", Some(a1)).await;
    let d = create_section_via_api(app.clone(), &token, org, ws, project, "D", None).await;

    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, a),
        Some(&token),
        Some(serde_json::json!({ "parent_section_id": d })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await["data"]["parent_section_id"],
        d.to_string()
    );
    // Descendants keep pointing through the hierarchy: the subtree moved
    // logically without rewriting any descendant row.
    let a1_parent: Option<Uuid> =
        sqlx::query_scalar("SELECT parent_section_id FROM sections WHERE id = $1")
            .bind(a1)
            .fetch_one(&pool)
            .await?;
    assert_eq!(a1_parent, Some(a));
    // The reparent appended A at the end of D's children.
    let a_position: i32 = sqlx::query_scalar("SELECT position FROM sections WHERE id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await?;
    let max_position: i32 = sqlx::query_scalar(
        "SELECT max(position) FROM sections WHERE project_id = $1 AND parent_section_id = $2",
    )
    .bind(project)
    .bind(d)
    .fetch_one(&pool)
    .await?;
    assert_eq!(a_position, max_position);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn child_to_root_move_uses_explicit_null_parent(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-root@example.test").await;
    let token = login_token(app.clone(), "sec-root@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Root Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Root Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let root = create_section_via_api(app.clone(), &token, org, ws, project, "Root", None).await;
    let child =
        create_section_via_api(app.clone(), &token, org, ws, project, "Child", Some(root)).await;

    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, child),
        Some(&token),
        Some(serde_json::json!({ "parent_section_id": null })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["data"]["parent_section_id"], serde_json::Value::Null);
    // Explicit null moved the section to root scope, appending at the end.
    let position: i32 = sqlx::query_scalar("SELECT position FROM sections WHERE id = $1")
        .bind(child)
        .fetch_one(&pool)
        .await?;
    let root_position: i32 = sqlx::query_scalar("SELECT position FROM sections WHERE id = $1")
        .bind(root)
        .fetch_one(&pool)
        .await?;
    assert!(
        position > root_position,
        "moved section must append after existing roots"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn sibling_reorder_via_position_and_negative_rejection(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-reorder@example.test").await;
    let token = login_token(app.clone(), "sec-reorder@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Reorder Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Reorder Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let first = create_section_via_api(app.clone(), &token, org, ws, project, "First", None).await;
    let _second =
        create_section_via_api(app.clone(), &token, org, ws, project, "Second", None).await;
    let third = create_section_via_api(app.clone(), &token, org, ws, project, "Third", None).await;

    // A real reorder sets the whole sibling group consistently: Third to 0,
    // the others shifted up (position alone decides; equal positions would
    // fall back to the id tiebreak).
    for (section, position) in [(third, 0), (first, 1), (_second, 2)] {
        let response = request(
            app.clone(),
            "PATCH",
            &section_item_path(org, ws, project, section),
            Some(&token),
            Some(serde_json::json!({ "position": position })),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let response = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    let data = body_json(response).await["data"].clone();
    let names: Vec<String> = data
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"))
        .iter()
        .map(|item| item["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(names, vec!["Third", "First", "Second"]);

    // Negative positions are invalid input.
    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, first),
        Some(&token),
        Some(serde_json::json!({ "position": -1 })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

// ---------------------------------------------------------------------------
// RBAC matrix and lifecycle
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn member_reads_but_cannot_mutate_sections(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-rbowner@example.test").await;
    create_user(&pool, "sec-rbmember@example.test").await;
    let token = login_token(app.clone(), "sec-rbowner@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "SecRbac Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "SecRbac Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let section = create_section_via_api(app.clone(), &token, org, ws, project, "S", None).await;
    let member = user_id_by_email(&pool, "sec-rbmember@example.test").await;
    add_org_membership(&pool, org, member).await;
    add_ws_membership(&pool, org, ws, member).await;
    let member_token = login_token(app.clone(), "sec-rbmember@example.test").await;

    let response = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&member_token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        app.clone(),
        "GET",
        &section_item_path(org, ws, project, section),
        Some(&member_token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    for (method, uri, body) in [
        (
            "POST",
            sections_path(org, ws, project),
            Some(serde_json::json!({ "name": "Nope" })),
        ),
        (
            "PATCH",
            section_item_path(org, ws, project, section),
            Some(serde_json::json!({ "name": "Nope" })),
        ),
        (
            "PATCH",
            section_item_path(org, ws, project, section),
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
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE name = 'Nope'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn archive_permission_gates_only_archive_transition(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-arch@example.test").await;
    let token = login_token(app.clone(), "sec-arch@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "SecArch Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "SecArch Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let section = create_section_via_api(app.clone(), &token, org, ws, project, "S", None).await;
    let user = user_id_by_email(&pool, "sec-arch@example.test").await;

    // Strip the org-wide archive grant: update keeps working, archiving not.
    sqlx::query(
        "DELETE FROM role_permissions rp USING permissions p \
         WHERE rp.permission_id = p.id AND p.key = 'sections:archive'",
    )
    .execute(&pool)
    .await?;
    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, section),
        Some(&token),
        Some(serde_json::json!({ "name": "Renamed" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, section),
        Some(&token),
        Some(serde_json::json!({ "status": "archived" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Workspace-scoped archive grant restores the capability; unarchive
    // needs update only.
    grant_workspace_scoped(&pool, org, ws, user, &["sections:archive"]).await;
    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, section),
        Some(&token),
        Some(serde_json::json!({ "status": "archived" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["data"]["status"], "archived");
    sqlx::query("DELETE FROM membership_roles WHERE workspace_id = $1")
        .bind(ws)
        .execute(&pool)
        .await?;
    let response = request(
        app,
        "PATCH",
        &section_item_path(org, ws, project, section),
        Some(&token),
        Some(serde_json::json!({ "status": "active" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn moving_under_archived_parent_fails(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-archparent@example.test").await;
    let token = login_token(app.clone(), "sec-archparent@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Ap Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Ap Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let parent =
        create_section_via_api(app.clone(), &token, org, ws, project, "Parent", None).await;
    let child = create_section_via_api(app.clone(), &token, org, ws, project, "Child", None).await;

    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, parent),
        Some(&token),
        Some(serde_json::json!({ "status": "archived" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        app,
        "PATCH",
        &section_item_path(org, ws, project, child),
        Some(&token),
        Some(serde_json::json!({ "parent_section_id": parent })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = body_json(response).await;
    assert!(error["error"]["details"]["fields"]["parent_section_id"].is_array());
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn archived_sections_stay_listed_and_transitions_are_enforced(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-life@example.test").await;
    let token = login_token(app.clone(), "sec-life@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Life Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Life Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let section = create_section_via_api(app.clone(), &token, org, ws, project, "S", None).await;

    // Unknown status value rejected at the API.
    let response = request(
        app.clone(),
        "PATCH",
        &section_item_path(org, ws, project, section),
        Some(&token),
        Some(serde_json::json!({ "status": "completed" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // active -> archived -> active works; archived sections remain visible
    // (marked) in the flat list — nothing is hidden, archived is a state.
    for status in ["archived", "active"] {
        let response = request(
            app.clone(),
            "PATCH",
            &section_item_path(org, ws, project, section),
            Some(&token),
            Some(serde_json::json!({ "status": status })),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["data"]["status"], status);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Enumeration / isolation semantics
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn foreign_section_access_is_indistinguishable_from_unknown(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-a@example.test").await;
    create_user(&pool, "sec-b@example.test").await;
    let token_a = login_token(app.clone(), "sec-a@example.test").await;
    let token_b = login_token(app.clone(), "sec-b@example.test").await;
    let org_a = create_org_via_api(app.clone(), &token_a, "Sec Tenant A").await;
    create_org_via_api(app.clone(), &token_b, "Sec Tenant B").await;
    let ws_a = create_workspace_via_api(app.clone(), &token_a, org_a, "A Ws").await;
    let project_a = create_project_via_api(app.clone(), &token_a, org_a, ws_a, "PA").await;
    let section = create_section_via_api(
        app.clone(),
        &token_a,
        org_a,
        ws_a,
        project_a,
        "Secret",
        None,
    )
    .await;

    for method in ["GET", "PATCH"] {
        let body = (method == "PATCH").then(|| serde_json::json!({ "name": "Hijack" }));
        let foreign = request(
            app.clone(),
            method,
            &section_item_path(org_a, ws_a, project_a, section),
            Some(&token_b),
            body.clone(),
        )
        .await;
        let unknown = request(
            app.clone(),
            method,
            &section_item_path(org_a, ws_a, project_a, Uuid::now_v7()),
            Some(&token_b),
            body,
        )
        .await;
        assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            error_fingerprint(foreign).await,
            error_fingerprint(unknown).await,
            "foreign and unknown sections must be indistinguishable"
        );
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_and_unknown_ids_share_identical_404_semantics(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-mal@example.test").await;
    let token = login_token(app.clone(), "sec-mal@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "SecMal Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "SecMal Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;

    let unknown = request(
        app.clone(),
        "GET",
        &format!("{}/{}", sections_path(org, ws, project), Uuid::now_v7()),
        Some(&token),
        None,
    )
    .await;
    let malformed = request(
        app.clone(),
        "GET",
        &format!("{}/not-a-uuid", sections_path(org, ws, project)),
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
async fn deleted_section_is_uniform_404_and_excluded_from_list(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-del@example.test").await;
    let token = login_token(app.clone(), "sec-del@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Del Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Del Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    create_section_via_api(app.clone(), &token, org, ws, project, "Stays", None).await;
    let deleted = create_section_via_api(app.clone(), &token, org, ws, project, "Goes", None).await;

    sqlx::query("UPDATE sections SET deleted_at = now() WHERE id = $1")
        .bind(deleted)
        .execute(&pool)
        .await?;

    let response = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    let data = body_json(response).await["data"].clone();
    let names: Vec<String> = data
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"))
        .iter()
        .map(|item| item["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(names, vec!["Stays"]);
    let deleted_response = request(
        app.clone(),
        "GET",
        &section_item_path(org, ws, project, deleted),
        Some(&token),
        None,
    )
    .await;
    let unknown_response = request(
        app,
        "GET",
        &section_item_path(org, ws, project, Uuid::now_v7()),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(deleted_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        error_fingerprint(deleted_response).await,
        error_fingerprint(unknown_response).await,
        "deleted and unknown sections must be indistinguishable"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn membership_matrix_gates_every_section_route(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-admin@example.test").await;
    create_user(&pool, "sec-outsider@example.test").await;
    let token = login_token(app.clone(), "sec-admin@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Matrix Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Matrix Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let section = create_section_via_api(app.clone(), &token, org, ws, project, "S", None).await;
    let outsider = user_id_by_email(&pool, "sec-outsider@example.test").await;

    // Org member WITHOUT workspace membership: uniform 404 everywhere.
    add_org_membership(&pool, org, outsider).await;
    let outsider_token = login_token(app.clone(), "sec-outsider@example.test").await;
    for (method, uri, body) in [
        ("GET", sections_path(org, ws, project), None),
        ("GET", section_item_path(org, ws, project, section), None),
        (
            "POST",
            sections_path(org, ws, project),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
        (
            "PATCH",
            section_item_path(org, ws, project, section),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
    ] {
        let response = request(app.clone(), method, &uri, Some(&outsider_token), body).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
    }

    // Workspace membership row exists but the org membership is gone: the
    // abnormal row stays inert (ACCESS_FILTER).
    add_ws_membership(&pool, org, ws, outsider).await;
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(outsider)
    .execute(&pool)
    .await?;
    for (method, uri, body) in [
        ("GET", sections_path(org, ws, project), None),
        (
            "PATCH",
            section_item_path(org, ws, project, section),
            Some(serde_json::json!({ "name": "Sneak" })),
        ),
    ] {
        let response = request(app.clone(), method, &uri, Some(&outsider_token), body).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn hostile_context_cookies_and_body_fields_do_not_affect_authorization(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-cookie@example.test").await;
    create_user(&pool, "sec-victim@example.test").await;
    let token = login_token(app.clone(), "sec-cookie@example.test").await;
    let victim_token = login_token(app.clone(), "sec-victim@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Cookie Org").await;
    let victim_org = create_org_via_api(app.clone(), &victim_token, "Victim Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Cookie Ws").await;
    let victim_ws = create_workspace_via_api(app.clone(), &victim_token, victim_org, "V Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let victim_project =
        create_project_via_api(app.clone(), &victim_token, victim_org, victim_ws, "VP").await;

    // Hostile cookies do not change any outcome.
    let clean = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    let hostile = request_with_extra_cookies(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
        &[
            format!("organization={victim_org}"),
            format!("workspace={victim_ws}"),
        ],
    )
    .await;
    assert_eq!(clean.status(), hostile.status());
    let response = request_with_extra_cookies(
        app.clone(),
        "GET",
        &sections_path(victim_org, victim_ws, victim_project),
        Some(&token),
        None,
        &[
            format!("organization={victim_org}"),
            format!("workspace={victim_ws}"),
        ],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Body fields claiming foreign scope are ignored: the route rules.
    let response = request(
        app.clone(),
        "POST",
        &sections_path(org, ws, project),
        Some(&token),
        Some(serde_json::json!({
            "name": "Owns",
            "tenant_id": victim_org.to_string(),
            "workspace_id": victim_ws.to_string(),
            "project_id": victim_project.to_string(),
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let stored: (Uuid, Uuid) =
        sqlx::query_as("SELECT tenant_id, project_id FROM sections WHERE name = 'Owns'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(stored, (org, project));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn unauthenticated_section_requests_are_rejected(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-auth@example.test").await;
    let token = login_token(app.clone(), "sec-auth@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Auth Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Auth Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let section = create_section_via_api(app.clone(), &token, org, ws, project, "S", None).await;

    for (method, uri, body) in [
        ("GET", sections_path(org, ws, project), None),
        (
            "POST",
            sections_path(org, ws, project),
            Some(serde_json::json!({ "name": "X" })),
        ),
        ("GET", section_item_path(org, ws, project, section), None),
        (
            "PATCH",
            section_item_path(org, ws, project, section),
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
// TOCTOU: revocations and state changes racing the mutation transaction
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn membership_revocation_race_blocks_section_mutations(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-toctou@example.test").await;
    let token = login_token(app.clone(), "sec-toctou@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Toctou Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Toctou Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let section = create_section_via_api(app.clone(), &token, org, ws, project, "S", None).await;
    let user = user_id_by_email(&pool, "sec-toctou@example.test").await;

    platform_server::rbac::deactivate_workspace_membership_with_assignments(&pool, org, ws, user)
        .await?;
    let created = sections::create_section(
        &pool,
        org,
        ws,
        project,
        &NewSection {
            name: "Raced".into(),
            slug: "raced".into(),
            parent_section_id: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(SectionError::NotAccessible)));
    let updated = sections::update_section(
        &pool,
        org,
        ws,
        project,
        section,
        &SectionPatch {
            name: Some("Raced".into()),
            ..SectionPatch::default()
        },
        user,
    )
    .await;
    assert!(matches!(updated, Err(SectionError::NotAccessible)));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE name = 'Raced'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0);

    // Organization membership revoke is equally denied after reactivation.
    platform_server::rbac::reactivate_workspace_membership(&pool, ws, user).await?;
    platform_server::rbac::deactivate_membership_with_assignments(&pool, org, user).await?;
    let created = sections::create_section(
        &pool,
        org,
        ws,
        project,
        &NewSection {
            name: "Raced Two".into(),
            slug: "raced-two".into(),
            parent_section_id: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(SectionError::NotAccessible)));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn permission_revocation_race_blocks_section_mutations(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-prace@example.test").await;
    let token = login_token(app.clone(), "sec-prace@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "PRace Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "PRace Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let section = create_section_via_api(app.clone(), &token, org, ws, project, "S", None).await;
    let user = user_id_by_email(&pool, "sec-prace@example.test").await;

    sqlx::query(
        "DELETE FROM role_permissions rp USING permissions p \
         WHERE rp.permission_id = p.id AND p.key = 'sections:create'",
    )
    .execute(&pool)
    .await?;
    let created = sections::create_section(
        &pool,
        org,
        ws,
        project,
        &NewSection {
            name: "Denied".into(),
            slug: "denied".into(),
            parent_section_id: None,
        },
        user,
    )
    .await;
    assert!(matches!(created, Err(SectionError::Forbidden)));
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await?;
    let updated = sections::update_section(
        &pool,
        org,
        ws,
        project,
        section,
        &SectionPatch {
            name: Some("Denied".into()),
            ..SectionPatch::default()
        },
        user,
    )
    .await;
    assert!(matches!(updated, Err(SectionError::Forbidden)));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE name = 'Denied'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn parent_archived_race_is_rejected_on_fresh_transaction_state(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-parentrace@example.test").await;
    let token = login_token(app.clone(), "sec-parentrace@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "PTrace Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "PTrace Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let parent =
        create_section_via_api(app.clone(), &token, org, ws, project, "Parent", None).await;
    let child = create_section_via_api(app.clone(), &token, org, ws, project, "Child", None).await;
    let user = user_id_by_email(&pool, "sec-parentrace@example.test").await;

    // The parent was valid at request time; it is archived before the move
    // transaction runs: the fresh in-transaction resolution must reject.
    sqlx::query("UPDATE sections SET status = 'archived' WHERE id = $1")
        .bind(parent)
        .execute(&pool)
        .await?;
    let updated = sections::update_section(
        &pool,
        org,
        ws,
        project,
        child,
        &SectionPatch {
            parent: ParentPatch::Set(parent),
            ..SectionPatch::default()
        },
        user,
    )
    .await;
    assert!(matches!(updated, Err(SectionError::InvalidParent)));
    let child_parent: Option<Uuid> =
        sqlx::query_scalar("SELECT parent_section_id FROM sections WHERE id = $1")
            .bind(child)
            .fetch_one(&pool)
            .await?;
    assert_eq!(child_parent, None, "rejected move must change nothing");
    Ok(())
}

// ---------------------------------------------------------------------------
// Concurrency: cycles and slug races under the project row lock
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_inverse_moves_cannot_create_a_cycle(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-concur@example.test").await;
    let token = login_token(app.clone(), "sec-concur@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Concur Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Concur Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let a = create_section_via_api(app.clone(), &token, org, ws, project, "A", None).await;
    let b = create_section_via_api(app.clone(), &token, org, ws, project, "B", None).await;
    let user = user_id_by_email(&pool, "sec-concur@example.test").await;

    // A under B and B under A race: the project-row lock serializes the two
    // transactions, so at most one can succeed and the tree stays acyclic.
    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let patch_ab = SectionPatch {
        parent: ParentPatch::Set(b),
        ..SectionPatch::default()
    };
    let patch_ba = SectionPatch {
        parent: ParentPatch::Set(a),
        ..SectionPatch::default()
    };
    let (move_ab, move_ba) = tokio::join!(
        sections::update_section(&pool_a, org, ws, project, a, &patch_ab, user),
        sections::update_section(&pool_b, org, ws, project, b, &patch_ba, user),
    );
    let successes = [move_ab.is_ok(), move_ba.is_ok()]
        .iter()
        .filter(|ok| **ok)
        .count();
    assert!(
        successes <= 1,
        "inverse concurrent moves must not both succeed"
    );
    // Final state is acyclic: no self-parenting and no two-node cycle.
    let (parent_a, parent_b): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT \
           (SELECT parent_section_id FROM sections WHERE id = $1), \
           (SELECT parent_section_id FROM sections WHERE id = $2)",
    )
    .bind(a)
    .bind(b)
    .fetch_one(&pool)
    .await?;
    assert!(
        parent_a != Some(a) && parent_b != Some(b),
        "no self-parenting"
    );
    assert!(
        !(parent_a == Some(b) && parent_b == Some(a)),
        "no two-cycle"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_same_slug_creation_has_a_single_winner(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-slugrace@example.test").await;
    let token = login_token(app.clone(), "sec-slugrace@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "SlugRace Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "SlugRace Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;

    let path = sections_path(org, ws, project);
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
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE slug = 'clash'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn soft_deleted_section_keeps_occupying_its_sibling_slug(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-deadslug@example.test").await;
    let token = login_token(app.clone(), "sec-deadslug@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "DeadSlug Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "DeadSlug Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    let floor = create_section_via_api(app.clone(), &token, org, ws, project, "Kat 1", None).await;
    let gone = create_section_via_api(
        app.clone(),
        &token,
        org,
        ws,
        project,
        "Daire 1",
        Some(floor),
    )
    .await;

    // Soft-delete the row directly (delete endpoint deferred); the HARD
    // sibling-scope unique means the slug stays occupied (ADR 0012 §7).
    sqlx::query("UPDATE sections SET deleted_at = now() WHERE id = $1")
        .bind(gone)
        .execute(&pool)
        .await?;
    let response = request(
        app,
        "POST",
        &sections_path(org, ws, project),
        Some(&token),
        Some(serde_json::json!({ "name": "Yeni", "slug": "daire-1", "parent_section_id": floor })),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "a soft-deleted sibling must keep occupying its slug"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn equal_positions_fall_back_to_id_for_deterministic_order(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-tiebreak@example.test").await;
    let token = login_token(app.clone(), "sec-tiebreak@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Tiebreak Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Tiebreak Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "P").await;
    create_section_via_api(app.clone(), &token, org, ws, project, "Bir", None).await;
    create_section_via_api(app.clone(), &token, org, ws, project, "Iki", None).await;

    // Force an equal-position collision directly (concurrent appends could
    // produce the same state); the list must stay deterministic via id.
    sqlx::query("UPDATE sections SET position = 0 WHERE project_id = $1")
        .bind(project)
        .execute(&pool)
        .await?;
    let response = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    let data = body_json(response).await["data"].clone();
    let rows = data
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"));
    assert_eq!(rows.len(), 2);
    let ids: Vec<String> = rows
        .iter()
        .map(|row| row["id"].as_str().unwrap_or_default().to_owned())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "equal positions must order by id ascending");
    // A second identical read returns the same order (deterministic contract).
    let again = request(
        app,
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    let ids2: Vec<String> = body_json(again).await["data"]
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"))
        .iter()
        .map(|row| row["id"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(ids, ids2);
    Ok(())
}

// ---------------------------------------------------------------------------
// Grants: bootstrap + migration backfill
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn owner_bootstrap_grants_section_keys_without_role_assignments(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-grant@example.test").await;
    let token = login_token(app.clone(), "sec-grant@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Grant Org").await;

    let grants: Vec<String> = sqlx::query_scalar(
        "SELECT p.key FROM role_permissions rp \
         JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.tenant_id = $1 AND r.is_system AND r.name = 'owner' AND p.key LIKE 'sections:%' \
         ORDER BY p.key",
    )
    .bind(org)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        grants,
        vec!["sections:archive", "sections:create", "sections:update"]
    );
    let member_grants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp \
         JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.tenant_id = $1 AND r.is_system AND r.name = 'member' AND p.key LIKE 'sections:%'",
    )
    .bind(org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        member_grants, 0,
        "built-in Member keeps zero section grants"
    );
    let assignments: i64 =
        sqlx::query_scalar("SELECT count(*) FROM membership_roles WHERE tenant_id = $1")
            .bind(org)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        assignments, 1,
        "only the creator's bootstrap assignment exists"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn migration_backfills_owner_grants_without_assigning_owner_roles(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-backfill@example.test").await;
    let token = login_token(app.clone(), "sec-backfill@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Backfill Org").await;

    // Roles exist WITHOUT section grants once 008 is reverted — the exact
    // state a pre-008 organization is in when the migration first runs. The
    // pre-008 version is located BY NAME so future migrations cannot shift
    // a positional revert target (the projects/rbac tests follow the same
    // convention after 008 exposed the pattern).
    let up_migrations: Vec<(i64, std::borrow::Cow<'_, str>)> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| (m.version, m.description.clone()))
        .collect();
    let sections_index = up_migrations
        .iter()
        .position(|(_, description)| description.contains("sections"))
        .unwrap_or_else(|| panic!("the sections migration must exist in the chain"));
    MIGRATOR
        .undo(&pool, up_migrations[sections_index - 1].0)
        .await?;
    let grant_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp JOIN permissions p ON p.id = rp.permission_id \
         WHERE rp.role_id IN (SELECT id FROM roles WHERE tenant_id = $1) AND p.key LIKE 'sections:%'",
    )
    .bind(org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(grant_count, 0);
    let sections_gone: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'sections')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!sections_gone, "revert must remove the sections table");

    MIGRATOR.run(&pool).await?;
    let owner_grants: Vec<String> = sqlx::query_scalar(
        "SELECT p.key FROM role_permissions rp \
         JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.tenant_id = $1 AND r.is_system AND r.name = 'owner' AND p.key LIKE 'sections:%' \
         ORDER BY p.key",
    )
    .bind(org)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        owner_grants,
        vec!["sections:archive", "sections:create", "sections:update"],
        "migration backfill must extend the Owner grant list"
    );
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
// Bulk readiness / performance sanity: a realistic 15x10 hierarchy
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn realistic_hierarchy_retrieves_in_one_flat_ordered_query(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-bulk@example.test").await;
    let token = login_token(app.clone(), "sec-bulk@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Bulk Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Bulk Ws").await;
    let project = create_project_via_api(app.clone(), &token, org, ws, "Tower").await;

    // 15 floors x 10 apartments = 165 sections as NORMAL rows — the future
    // bulk generator will simply create many sections (ADR 0012 readiness).
    for floor in 1..=15 {
        let floor_id: Uuid = sqlx::query_scalar(
            "INSERT INTO sections (id, tenant_id, workspace_id, project_id, parent_section_id, name, slug, position) \
             VALUES ($1, $2, $3, $4, NULL, $5, $6, $7) RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(org)
        .bind(ws)
        .bind(project)
        .bind(format!("Kat {floor}"))
        .bind(format!("kat-{floor}"))
        .bind(floor - 1)
        .fetch_one(&pool)
        .await?;
        for apartment in 1..=10 {
            sqlx::query(
                "INSERT INTO sections (id, tenant_id, workspace_id, project_id, parent_section_id, name, slug, position) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(Uuid::now_v7())
            .bind(org)
            .bind(ws)
            .bind(project)
            .bind(floor_id)
            .bind(format!("Daire {apartment}"))
            .bind(format!("daire-{apartment}"))
            .bind(apartment - 1)
            .execute(&pool)
            .await?;
        }
    }

    // One API call returns the whole project tree flat and ordered; the
    // service issues exactly one SQL statement by construction.
    let response = request(
        app.clone(),
        "GET",
        &sections_path(org, ws, project),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let data = body_json(response).await["data"].clone();
    let rows = data
        .as_array()
        .unwrap_or_else(|| panic!("list must be an array"));
    assert_eq!(rows.len(), 165);
    // Roots arrive first in position order; each floor's children follow,
    // siblings sorted by (position, id) — deterministic and hierarchy-aware.
    let roots: Vec<&str> = rows
        .iter()
        .filter(|row| row["parent_section_id"].is_null())
        .map(|row| row["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(roots.len(), 15);
    assert_eq!(roots.first(), Some(&"Kat 1"));
    assert_eq!(roots.last(), Some(&"Kat 15"));
    // Every apartment row points at a floor of the SAME project.
    let orphan_parents: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sections s \
         WHERE s.project_id = $1 AND s.parent_section_id IS NOT NULL \
           AND NOT EXISTS (SELECT 1 FROM sections p WHERE p.id = s.parent_section_id AND p.project_id = $1)",
    )
    .bind(project)
    .fetch_one(&pool)
    .await?;
    assert_eq!(orphan_parents, 0);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_scoped_grant_cannot_escape_its_workspace(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "sec-wsrole@example.test").await;
    create_user(&pool, "sec-wsbound@example.test").await;
    let token = login_token(app.clone(), "sec-wsrole@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "WsRole Org").await;
    let ws_a = create_workspace_via_api(app.clone(), &token, org, "Bound Ws").await;
    let ws_b = create_workspace_via_api(app.clone(), &token, org, "Other Ws").await;
    let project_a = create_project_via_api(app.clone(), &token, org, ws_a, "PA").await;
    let project_b = create_project_via_api(app.clone(), &token, org, ws_b, "PB").await;

    let bound = user_id_by_email(&pool, "sec-wsbound@example.test").await;
    add_org_membership(&pool, org, bound).await;
    add_ws_membership(&pool, org, ws_a, bound).await;
    add_ws_membership(&pool, org, ws_b, bound).await;
    grant_workspace_scoped(&pool, org, ws_a, bound, &["sections:create"]).await;
    let bound_token = login_token(app.clone(), "sec-wsbound@example.test").await;

    let response = request(
        app.clone(),
        "POST",
        &sections_path(org, ws_a, project_a),
        Some(&bound_token),
        Some(serde_json::json!({ "name": "In Scope" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = request(
        app,
        "POST",
        &sections_path(org, ws_b, project_b),
        Some(&bound_token),
        Some(serde_json::json!({ "name": "Out Of Scope" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    Ok(())
}
