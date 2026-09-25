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
    users::NewUser,
    work_items::{
        self, CreateWorkItemRequest, UpdateWorkItemRequest, WorkItemError, WorkItemScope,
    },
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

async fn grant_workspace_scoped(
    pool: &PgPool,
    tenant: Uuid,
    workspace: Uuid,
    user: Uuid,
    keys: &[&str],
) {
    let role: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (id, tenant_id, name, is_system) \
         VALUES ($1, $2, 'ws-work-item-role', false) RETURNING id",
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

struct Fixture {
    app: axum::Router,
    token: String,
    actor: Uuid,
    scope: WorkItemScope,
}
impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        let actor = create_user(pool, "work-owner@example.test").await;
        let app = router(state(pool));
        let token = login_token(app.clone(), "work-owner@example.test").await;
        let org = create_org_via_api(app.clone(), &token, "Work Org").await;
        let ws = create_workspace_via_api(app.clone(), &token, org, "Work Ws").await;
        let project = create_project_via_api(app.clone(), &token, org, ws, "Project").await;
        let section =
            create_section_via_api(app.clone(), &token, org, ws, project, "Section", None).await;
        Self {
            app,
            token,
            actor,
            scope: WorkItemScope {
                organization_id: org,
                workspace_id: ws,
                project_id: project,
                section_id: section,
            },
        }
    }
    fn path(&self) -> String {
        path(self.scope)
    }
    async fn call(
        &self,
        method: &str,
        suffix: &str,
        body: Option<serde_json::Value>,
    ) -> axum::response::Response {
        request(
            self.app.clone(),
            method,
            &format!("{}{suffix}", self.path()),
            Some(&self.token),
            body,
        )
        .await
    }
    async fn create(&self, name: &str) -> Uuid {
        let response = self
            .call("POST", "", Some(serde_json::json!({"name":name})))
            .await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("expected response value"))
            .parse()
            .unwrap_or_else(|error| panic!("operation must succeed: {error}"))
    }
    async fn member(&self, pool: &PgPool) -> (Uuid, String) {
        let user = create_user(pool, "work-member@example.test").await;
        add_org_membership(pool, self.scope.organization_id, user).await;
        add_ws_membership(
            pool,
            self.scope.organization_id,
            self.scope.workspace_id,
            user,
        )
        .await;
        (
            user,
            login_token(self.app.clone(), "work-member@example.test").await,
        )
    }
}
fn path(s: WorkItemScope) -> String {
    format!(
        "{}/work-items",
        section_item_path(
            s.organization_id,
            s.workspace_id,
            s.project_id,
            s.section_id
        )
    )
}
fn input() -> CreateWorkItemRequest {
    CreateWorkItemRequest {
        name: "New work".into(),
        slug: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_crud_archive_and_hidden_detail(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("İşçilik Örneği").await;
    let item = body_json(f.call("GET", &format!("/{id}"), None).await).await;
    assert_eq!(item["slug"], "iscilik-ornegi");
    assert_eq!(item["section_id"], f.scope.section_id.to_string());
    assert_eq!(item["status"], "active");
    for status in ["completed", "active", "archived"] {
        let r=f.call("PATCH", &format!("/{id}"), Some(serde_json::json!({"name":"Updated", "slug":"updated", "position":7,"status":status}))).await;
        assert_eq!(r.status(), StatusCode::OK);
        let item = body_json(r).await;
        assert_eq!(item["data"]["status"], status);
        assert_eq!(item["data"]["position"], 7);
        assert_eq!(item["data"]["name"], "Updated");
    }
    assert!(
        body_json(f.call("GET", "", None).await).await["data"]
            .as_array()
            .unwrap_or_else(|| panic!("expected response value"))
            .is_empty()
    );
    for method in ["GET", "PATCH"] {
        let archived = f
            .call(
                method,
                &format!("/{id}"),
                Some(serde_json::json!({"status":"active"})),
            )
            .await;
        assert_eq!(archived.status(), StatusCode::NOT_FOUND);
        let unknown = f
            .call(
                method,
                &format!("/{}", Uuid::now_v7()),
                Some(serde_json::json!({"status":"active"})),
            )
            .await;
        assert_eq!(
            error_fingerprint(archived).await,
            error_fingerprint(unknown).await
        );
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM work_items WHERE id=$1 AND status='archived'")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn validation_and_identity_injection(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Original").await;
    for body in [
        serde_json::json!({"name":" "}),
        serde_json::json!({"name":"x".repeat(201)}),
        serde_json::json!({"name":"x","slug":"!!!"}),
    ] {
        assert_eq!(
            f.call("POST", "", Some(body)).await.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    for body in [
        serde_json::json!({"name":" "}),
        serde_json::json!({"position":-1}),
        serde_json::json!({"status":"bogus"}),
        serde_json::json!({"slug":""}),
    ] {
        assert_eq!(
            f.call("PATCH", &format!("/{id}"), Some(body))
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    for key in [
        "tenant_id",
        "organization_id",
        "workspace_id",
        "project_id",
        "section_id",
        "user_id",
        "parent_work_item_id",
    ] {
        assert_eq!(
            f.call(
                "POST",
                "",
                Some(serde_json::json!({"name":"Attack",key:Uuid::now_v7()}))
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            f.call(
                "PATCH",
                &format!("/{id}"),
                Some(serde_json::json!({key:Uuid::now_v7()}))
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        f.call(
            "POST",
            "",
            Some(serde_json::json!({"name":"Attack","status":"completed"}))
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        body_json(f.call("GET", &format!("/{id}"), None).await).await["name"],
        "Original"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn auth_eligibility_permission_validation_order(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Work").await;
    let (_, member) = f.member(&pool).await;
    let outsider = create_user(&pool, "outsider@example.test").await;
    assert_ne!(outsider, f.actor);
    let foreign = login_token(f.app.clone(), "outsider@example.test").await;
    for (token, expected, code) in [
        (None, StatusCode::UNAUTHORIZED, "AUTH_REQUIRED"),
        (
            Some(foreign.as_str()),
            StatusCode::NOT_FOUND,
            "RESOURCE_NOT_FOUND",
        ),
        (
            Some(member.as_str()),
            StatusCode::FORBIDDEN,
            "PERMISSION_DENIED",
        ),
        (
            Some(f.token.as_str()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
        ),
    ] {
        for (method, suffix) in [("POST", String::new()), ("PATCH", format!("/{id}"))] {
            let response = request(
                f.app.clone(),
                method,
                &format!("{}{suffix}", f.path()),
                token,
                Some(serde_json::json!({"name":""})),
            )
            .await;
            assert_eq!(response.status(), expected);
            assert_eq!(body_json(response).await["error"]["code"], code);
        }
    }
    for suffix in [String::new(), format!("/{id}")] {
        assert_eq!(
            request(
                f.app.clone(),
                "GET",
                &format!("{}{suffix}", f.path()),
                None,
                None
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn strong_attacker_wrong_nesting_and_enumeration_parity(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Secret").await;
    let s = f.scope;
    let other_section = create_section_via_api(
        f.app.clone(),
        &f.token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        "Sibling",
        None,
    )
    .await;
    let other_project = create_project_via_api(
        f.app.clone(),
        &f.token,
        s.organization_id,
        s.workspace_id,
        "Other Project",
    )
    .await;
    let other_ws =
        create_workspace_via_api(f.app.clone(), &f.token, s.organization_id, "Other Ws").await;
    let other_org = create_org_via_api(f.app.clone(), &f.token, "Other Org").await;
    let paths = [
        path(WorkItemScope {
            section_id: other_section,
            ..s
        }),
        path(WorkItemScope {
            project_id: other_project,
            ..s
        }),
        path(WorkItemScope {
            workspace_id: other_ws,
            ..s
        }),
        path(WorkItemScope {
            organization_id: other_org,
            ..s
        }),
    ];
    for method in ["GET", "PATCH"] {
        let expected = error_fingerprint(
            f.call(
                method,
                &format!("/{}", Uuid::now_v7()),
                Some(serde_json::json!({"name":"Attack"})),
            )
            .await,
        )
        .await;
        for uri in paths
            .iter()
            .map(|p| format!("{p}/{id}"))
            .chain([format!("{}/broken", f.path())])
        {
            let r = request(
                f.app.clone(),
                method,
                &uri,
                Some(&f.token),
                Some(serde_json::json!({"name":"Attack"})),
            )
            .await;
            assert_eq!(r.status(), StatusCode::NOT_FOUND);
            assert_eq!(error_fingerprint(r).await, expected);
        }
    }
    // A real second item cannot be addressed through the first item's section.
    let sibling = work_items::create_work_item(
        &pool,
        WorkItemScope {
            section_id: other_section,
            ..s
        },
        f.actor,
        &input(),
    )
    .await
    .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(
        f.call("GET", &format!("/{}", sibling.id), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn composite_fk_prevents_cross_scope_rows(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let s = f.scope;
    let org = create_org_via_api(f.app.clone(), &f.token, "Foreign Org").await;
    let ws = create_workspace_via_api(f.app.clone(), &f.token, s.organization_id, "Other Ws").await;
    let project = create_project_via_api(
        f.app.clone(),
        &f.token,
        s.organization_id,
        s.workspace_id,
        "Other Project",
    )
    .await;
    for bad in [
        WorkItemScope {
            organization_id: org,
            ..s
        },
        WorkItemScope {
            workspace_id: ws,
            ..s
        },
        WorkItemScope {
            project_id: project,
            ..s
        },
        WorkItemScope {
            section_id: Uuid::now_v7(),
            ..s
        },
    ] {
        let error=sqlx::query("INSERT INTO work_items (id,tenant_id,workspace_id,project_id,section_id,name,slug,position) VALUES ($1,$2,$3,$4,$5,'Invalid','invalid',0)")
            .bind(Uuid::now_v7()).bind(bad.organization_id).bind(bad.workspace_id).bind(bad.project_id).bind(bad.section_id).execute(&pool).await.err().unwrap_or_else(|| panic!("invalid row must be rejected"));
        assert_eq!(
            error
                .as_database_error()
                .unwrap_or_else(|| panic!("expected response value"))
                .code()
                .as_deref(),
            Some("23503")
        );
        assert_eq!(
            error
                .as_database_error()
                .unwrap_or_else(|| panic!("expected response value"))
                .constraint(),
            Some("work_items_section_fk")
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM work_items")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn slug_namespace_preserved_after_archive_and_soft_delete(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Work").await;
    assert_eq!(
        f.call(
            "POST",
            "",
            Some(serde_json::json!({"name":"Other","slug":"WORK"}))
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    f.call(
        "PATCH",
        &format!("/{id}"),
        Some(serde_json::json!({"status":"archived"})),
    )
    .await;
    assert_eq!(
        f.call("POST", "", Some(serde_json::json!({"name":"Work"})))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    sqlx::query("UPDATE work_items SET deleted_at=now() WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(
        f.call("GET", &format!("/{id}"), None).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.call("POST", "", Some(serde_json::json!({"name":"Work"})))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let s = f.scope;
    let section = create_section_via_api(
        f.app.clone(),
        &f.token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        "Sibling",
        None,
    )
    .await;
    let same = work_items::create_work_item(
        &pool,
        WorkItemScope {
            section_id: section,
            ..s
        },
        f.actor,
        &CreateWorkItemRequest {
            name: "Work".into(),
            slug: None,
        },
    )
    .await;
    assert!(same.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn order_append_equal_positions_and_partial_updates(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let a = f.create("A").await;
    let b = f.create("B").await;
    let c = f.create("C").await;
    for (id, pos) in [(a, 4), (b, 0), (c, 0)] {
        assert_eq!(
            f.call(
                "PATCH",
                &format!("/{id}"),
                Some(serde_json::json!({"position":pos}))
            )
            .await
            .status(),
            StatusCode::OK
        );
    }
    let rows = body_json(f.call("GET", "", None).await).await;
    let ids: Vec<Uuid> = rows["data"]
        .as_array()
        .unwrap_or_else(|| panic!("expected response value"))
        .iter()
        .map(|r| {
            r["id"]
                .as_str()
                .unwrap_or_else(|| panic!("expected response value"))
                .parse()
                .unwrap_or_else(|error| panic!("operation must succeed: {error}"))
        })
        .collect();
    let mut ties = vec![b, c];
    ties.sort();
    ties.push(a);
    assert_eq!(ids, ties);
    let d = f.create("D").await;
    assert_eq!(
        body_json(f.call("GET", &format!("/{d}"), None).await).await["position"],
        5
    );
    assert_eq!(
        body_json(f.call("GET", &format!("/{a}"), None).await).await["name"],
        "A"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn grant_revoke_and_archive_requires_both_permissions(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Work").await;
    let (user, token) = f.member(&pool).await;
    let s = f.scope;
    assert_eq!(
        request(f.app.clone(), "GET", &f.path(), Some(&token), None)
            .await
            .status(),
        StatusCode::OK
    );
    grant_workspace_scoped(
        &pool,
        s.organization_id,
        s.workspace_id,
        user,
        &["work_items:create", "work_items:update"],
    )
    .await;
    assert_eq!(
        request(
            f.app.clone(),
            "POST",
            &f.path(),
            Some(&token),
            Some(serde_json::json!({"name":"Allowed"}))
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let uri = format!("{}/{id}", f.path());
    assert_eq!(
        request(
            f.app.clone(),
            "PATCH",
            &uri,
            Some(&token),
            Some(serde_json::json!({"status":"archived","name":""}))
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            f.app.clone(),
            "PATCH",
            &uri,
            Some(&token),
            Some(serde_json::json!({"name":"Edited"}))
        )
        .await
        .status(),
        StatusCode::OK
    );
    sqlx::query("INSERT INTO role_permissions (role_id,permission_id,scope) SELECT r.id,p.id,'workspace' FROM roles r CROSS JOIN permissions p WHERE r.tenant_id=$1 AND r.name='ws-work-item-role' AND p.key='work_items:archive'").bind(s.organization_id).execute(&pool).await.unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(
        request(
            f.app.clone(),
            "PATCH",
            &uri,
            Some(&token),
            Some(serde_json::json!({"status":"archived"}))
        )
        .await
        .status(),
        StatusCode::OK
    );
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id=$1 AND user_id=$2")
        .bind(s.organization_id)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(
        request(
            f.app.clone(),
            "POST",
            &f.path(),
            Some(&token),
            Some(serde_json::json!({"name":"Denied"}))
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

// Deterministic TOCTOU: first prove request eligibility/permission, then
// commit the adversarial state change, then call the write service with the
// previously resolved scope. No sleeps and no test-only production hooks.
async fn revoked_state(pool: PgPool, change: &str, forbidden: bool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Original").await;
    let s = f.scope;
    assert!(
        platform_server::rbac::authorize_workspace(
            &pool,
            s.organization_id,
            s.workspace_id,
            f.actor,
            work_items::WORK_ITEMS_CREATE
        )
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"))
    );
    sqlx::query(change)
        .bind(s.organization_id)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    let create = work_items::create_work_item(&pool, s, f.actor, &input()).await;
    let update = work_items::update_work_item(
        &pool,
        s,
        id,
        f.actor,
        &UpdateWorkItemRequest {
            name: Some("Unauthorized".into()),
            ..Default::default()
        },
    )
    .await;
    for result in [create, update] {
        if forbidden {
            assert!(matches!(result, Err(WorkItemError::Forbidden)));
        } else {
            assert!(matches!(result, Err(WorkItemError::NotAccessible)));
        }
    }
    let rows: Vec<String> = sqlx::query_scalar("SELECT name FROM work_items WHERE tenant_id=$1")
        .bind(s.organization_id)
        .fetch_all(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(rows, vec!["Original"]);
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_org_membership(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE organization_memberships SET status='deleted' WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_workspace_membership(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE workspace_memberships SET deleted_at=now() WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_permission_grants(pool: PgPool) {
    revoked_state(pool,"DELETE FROM role_permissions WHERE role_id IN (SELECT id FROM roles WHERE tenant_id=$1) AND permission_id IN (SELECT id FROM permissions WHERE key LIKE 'work_items:%')",true).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_role_assignments(pool: PgPool) {
    revoked_state(
        pool,
        "DELETE FROM membership_roles WHERE tenant_id=$1",
        true,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_archived_project(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE projects SET status='archived' WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_deleted_project(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE projects SET deleted_at=now() WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_archived_section(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE sections SET status='archived' WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_deleted_section(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE sections SET deleted_at=now() WHERE tenant_id=$1",
        false,
    )
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn memberships_and_parent_lifecycle_gate_all_http_routes(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Work").await;
    // Constant SQL selected by the test, not request interpolation.
    for (revoke, restore) in [
        (
            "UPDATE organization_memberships SET status='deleted'",
            "UPDATE organization_memberships SET status='active'",
        ),
        (
            "UPDATE workspace_memberships SET status='deleted'",
            "UPDATE workspace_memberships SET status='active'",
        ),
        (
            "UPDATE projects SET status='archived'",
            "UPDATE projects SET status='active'",
        ),
        (
            "UPDATE sections SET status='archived'",
            "UPDATE sections SET status='active'",
        ),
        (
            "UPDATE workspaces SET deleted_at=now()",
            "UPDATE workspaces SET deleted_at=NULL",
        ),
        (
            "UPDATE organizations SET deleted_at=now()",
            "UPDATE organizations SET deleted_at=NULL",
        ),
    ] {
        sqlx::query(revoke)
            .execute(&pool)
            .await
            .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
        for (method, suffix) in [
            ("GET", String::new()),
            ("POST", String::new()),
            ("GET", format!("/{id}")),
            ("PATCH", format!("/{id}")),
        ] {
            assert_eq!(
                f.call(method, &suffix, Some(serde_json::json!({"name":"Changed"})))
                    .await
                    .status(),
                StatusCode::NOT_FOUND,
                "{revoke} {method}"
            );
        }
        sqlx::query(restore)
            .execute(&pool)
            .await
            .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    }
    assert_eq!(
        body_json(f.call("GET", &format!("/{id}"), None).await).await["name"],
        "Work"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_same_slug_has_one_winner(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let body = input();
    let (a, b) = tokio::join!(
        work_items::create_work_item(&pool, f.scope, f.actor, &body),
        work_items::create_work_item(&pool, f.scope, f.actor, &body)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(WorkItemError::InvalidFields(_)))
            || matches!(b, Err(WorkItemError::InvalidFields(_)))
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM work_items")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn hostile_cookies_do_not_change_work_item_scope(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Work").await;
    let cookies = vec![
        format!("organization={}", Uuid::now_v7()),
        format!("workspace={}", Uuid::now_v7()),
    ];
    for (method, suffix, body, status) in [
        ("GET", format!("/{id}"), None, StatusCode::OK),
        (
            "POST",
            String::new(),
            Some(serde_json::json!({"name":"Cookie independent"})),
            StatusCode::CREATED,
        ),
        (
            "PATCH",
            format!("/{id}"),
            Some(serde_json::json!({"name":"Updated"})),
            StatusCode::OK,
        ),
    ] {
        assert_eq!(
            request_with_extra_cookies(
                f.app.clone(),
                method,
                &format!("{}{suffix}", f.path()),
                Some(&f.token),
                body,
                &cookies
            )
            .await
            .status(),
            status
        );
    }
    let outsider = create_user(&pool, "cookie-outsider@example.test").await;
    assert_ne!(outsider, f.actor);
    let token = login_token(f.app.clone(), "cookie-outsider@example.test").await;
    let cookies = vec![
        format!("organization={}", f.scope.organization_id),
        format!("workspace={}", f.scope.workspace_id),
    ];
    assert_eq!(
        request_with_extra_cookies(
            f.app.clone(),
            "GET",
            &format!("{}/{id}", f.path()),
            Some(&token),
            None,
            &cookies
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_grants_cannot_escape_scope(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let s = f.scope;
    let (user, token) = f.member(&pool).await;
    let ws = create_workspace_via_api(f.app.clone(), &f.token, s.organization_id, "Other Ws").await;
    let project =
        create_project_via_api(f.app.clone(), &f.token, s.organization_id, ws, "Other P").await;
    let section = create_section_via_api(
        f.app.clone(),
        &f.token,
        s.organization_id,
        ws,
        project,
        "Other S",
        None,
    )
    .await;
    add_ws_membership(&pool, s.organization_id, ws, user).await;
    grant_workspace_scoped(
        &pool,
        s.organization_id,
        s.workspace_id,
        user,
        &["work_items:create"],
    )
    .await;
    for (scope, expected) in [
        (s, StatusCode::CREATED),
        (
            WorkItemScope {
                workspace_id: ws,
                project_id: project,
                section_id: section,
                ..s
            },
            StatusCode::FORBIDDEN,
        ),
    ] {
        assert_eq!(
            request(
                f.app.clone(),
                "POST",
                &path(scope),
                Some(&token),
                Some(serde_json::json!({"name":"Scoped"}))
            )
            .await
            .status(),
            expected
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_context_survives_future_deeper_routes(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Work").await;
    async fn handler(
        context: platform_server::context::WorkItemContext,
    ) -> axum::Json<serde_json::Value> {
        axum::Json(
            serde_json::json!({"id":context.work_item.id,"section":context.parent.scope.section_id}),
        )
    }
    let app=axum::Router::new().route("/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/future/{future_id}",axum::routing::get(handler)).with_state(state(&pool));
    let r = request(
        app,
        "GET",
        &format!("{}/{id}/future/extra", f.path()),
        Some(&f.token),
        None,
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
    let b = body_json(r).await;
    assert_eq!(b["id"], id.to_string());
    assert_eq!(b["section"], f.scope.section_id.to_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn migration_backfill_preserves_assignments_and_member_has_no_grants(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let org = f.scope.organization_id;
    let before: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM membership_roles ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    // Locate the pre-work-items version BY NAME: a positional revert_last
    // would break as soon as a later migration joins the chain (the same
    // pattern projects/rbac/sections tests already follow).
    let up_migrations: Vec<(i64, std::borrow::Cow<'_, str>)> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| (m.version, m.description.clone()))
        .collect();
    let work_items_index = up_migrations
        .iter()
        .position(|(_, description)| description.contains("work items"))
        .unwrap_or_else(|| panic!("the work_items migration must exist in the chain"));
    platform_server::migrations::MIGRATOR
        .undo(&pool, up_migrations[work_items_index - 1].0)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM permissions WHERE key LIKE 'work_items:%'")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(count, 0);
    MIGRATOR
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    let grants:Vec<String>=sqlx::query_scalar("SELECT p.key FROM role_permissions rp JOIN roles r ON r.id=rp.role_id JOIN permissions p ON p.id=rp.permission_id WHERE r.tenant_id=$1 AND r.is_system AND r.name='owner' AND p.key LIKE 'work_items:%' ORDER BY p.key").bind(org).fetch_all(&pool).await.unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(
        grants,
        vec![
            "work_items:archive",
            "work_items:create",
            "work_items:update"
        ]
    );
    let after: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM membership_roles ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(before, after);
    // ADR 0016: the 011 backfill grants built-in Members exactly the three
    // execution keys at organization scope — and nothing else.
    let member_keys: Vec<String> = sqlx::query_scalar(
        "SELECT p.key FROM role_permissions rp JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.name = 'member' AND r.tenant_id = $1 ORDER BY p.key",
    )
    .bind(org)
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|error| panic!("operation must succeed: {error}"));
    assert_eq!(
        member_keys,
        vec![
            "process_executions:cancel",
            "process_executions:complete",
            "process_executions:start"
        ]
    );
}
