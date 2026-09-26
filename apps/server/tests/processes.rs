use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState,
    config::AuthConfig,
    migrations::MIGRATOR,
    password::PasswordService,
    processes::{
        self, CreateProcessRequest, ProcessError, ProcessScope, ReorderProcessesRequest,
        UpdateProcessRequest,
    },
    router,
    users::NewUser,
};
use serde_json::{Value, json};
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
    platform_server::users::insert_user(
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
    .unwrap_or_else(|_| panic!("test user must be creatable"))
    .id
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
                    51_112,
                ))))
                .body(Body::from(
                    json!({ "email": email, "password": "password-1" }).to_string(),
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
    body: Option<Value>,
) -> axum::response::Response {
    request_with_extra_cookies(router, method, uri, token, body, &[]).await
}

async fn request_with_extra_cookies(
    router: axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
    extra_cookies: &[String],
) -> axum::response::Response {
    let mut builder = Request::builder().uri(uri).method(method);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let mut cookies: Vec<String> = token
        .map(|t| format!("platform_session={t}"))
        .into_iter()
        .collect();
    cookies.extend(extra_cookies.iter().cloned());
    if !cookies.is_empty() {
        builder = builder.header(header::COOKIE, cookies.join("; "));
    }
    let body = Body::from(body.map(|b| b.to_string()).unwrap_or_else(|| "{}".into()));
    router
        .oneshot(
            builder
                .body(body)
                .unwrap_or_else(|_| panic!("request must build")),
        )
        .await
        .unwrap_or_else(|_| panic!("request must respond"))
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576)
        .await
        .unwrap_or_else(|_| panic!("body must be readable"));
    serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("body must be JSON"))
}

// Error envelope without the per-request id: foreign vs unknown vs archived
// must be byte-equal apart from request_id.
async fn error_fingerprint(response: axum::response::Response) -> Value {
    let mut error = body_json(response).await["error"].clone();
    error["request_id"] = json!("<stripped>");
    error
}

fn uuid_of(value: &Value) -> Uuid {
    value
        .as_str()
        .unwrap_or_else(|| panic!("expected id string in {value}"))
        .parse()
        .unwrap_or_else(|error| panic!("id must parse: {error}"))
}

async fn post_id(app: &axum::Router, token: &str, uri: &str, body: Value, pick: &str) -> Uuid {
    let response = request(app.clone(), "POST", uri, Some(token), Some(body)).await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{uri}: {body}");
    uuid_of(&body.pointer(pick).cloned().unwrap_or_default())
}

async fn exec(pool: &PgPool, sql: &str) {
    sqlx::query(sql)
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"));
}

async fn add_member(pool: &PgPool, org: Uuid, ws: Uuid, user: Uuid) {
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
    add_ws_membership(pool, org, ws, user).await;
}

async fn add_ws_membership(pool: &PgPool, org: Uuid, ws: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(ws)
    .bind(user)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("ws membership must be insertable"));
}

/// Workspace-scoped custom role holding exactly `keys` (created on first use,
/// extended on later calls so grants can be added step by step).
async fn grant(pool: &PgPool, org: Uuid, ws: Uuid, user: Uuid, keys: &[&str]) {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM roles WHERE tenant_id = $1 AND name = 'ws-process-role'",
    )
    .bind(org)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    let role = match existing {
        Some(role) => role,
        None => {
            let role: Uuid = sqlx::query_scalar(
                "INSERT INTO roles (id, tenant_id, name, is_system) \
                 VALUES ($1, $2, 'ws-process-role', false) RETURNING id",
            )
            .bind(Uuid::now_v7())
            .bind(org)
            .fetch_one(pool)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
            sqlx::query(
                "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(Uuid::now_v7())
            .bind(org)
            .bind(user)
            .bind(role)
            .bind(ws)
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
            role
        }
    };
    for key in keys {
        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_id, scope) \
             SELECT $1, p.id, 'workspace' FROM permissions p WHERE p.key = $2",
        )
        .bind(role)
        .bind(key)
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    }
}

fn base(org: Uuid, ws: Uuid) -> String {
    format!("/api/v1/organizations/{org}/workspaces/{ws}")
}
fn path(s: ProcessScope) -> String {
    format!(
        "{}/projects/{}/sections/{}/work-items/{}/processes",
        base(s.organization_id, s.workspace_id),
        s.project_id,
        s.section_id,
        s.work_item_id
    )
}

struct Fixture {
    app: axum::Router,
    token: String,
    actor: Uuid,
    scope: ProcessScope,
}
impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        let actor = create_user(pool, "process-owner@example.test").await;
        let app = router(state(pool));
        let token = login_token(app.clone(), "process-owner@example.test").await;
        let org = post_id(
            &app,
            &token,
            "/api/v1/organizations",
            json!({"name": "Process Org"}),
            "/data/organization/id",
        )
        .await;
        let ws = Self::workspace(&app, &token, org, "Process Ws").await;
        let project = Self::project(&app, &token, org, ws, "Project").await;
        let section = Self::section(&app, &token, org, ws, project, "Section").await;
        let work_item = Self::work_item(&app, &token, org, ws, project, section, "Tezgah").await;
        Self {
            app,
            token,
            actor,
            scope: ProcessScope {
                organization_id: org,
                workspace_id: ws,
                project_id: project,
                section_id: section,
                work_item_id: work_item,
            },
        }
    }
    async fn workspace(app: &axum::Router, token: &str, org: Uuid, name: &str) -> Uuid {
        let uri = format!("/api/v1/organizations/{org}/workspaces");
        post_id(
            app,
            token,
            &uri,
            json!({"name": name}),
            "/data/workspace/id",
        )
        .await
    }
    async fn project(app: &axum::Router, token: &str, org: Uuid, ws: Uuid, name: &str) -> Uuid {
        let uri = format!("{}/projects", base(org, ws));
        post_id(app, token, &uri, json!({"name": name}), "/data/id").await
    }
    async fn section(
        app: &axum::Router,
        token: &str,
        org: Uuid,
        ws: Uuid,
        project: Uuid,
        name: &str,
    ) -> Uuid {
        let uri = format!("{}/projects/{project}/sections", base(org, ws));
        post_id(app, token, &uri, json!({"name": name}), "/data/id").await
    }
    async fn work_item(
        app: &axum::Router,
        token: &str,
        org: Uuid,
        ws: Uuid,
        project: Uuid,
        section: Uuid,
        name: &str,
    ) -> Uuid {
        let uri = format!(
            "{}/projects/{project}/sections/{section}/work-items",
            base(org, ws)
        );
        post_id(app, token, &uri, json!({"name": name}), "/data/id").await
    }
    fn path(&self) -> String {
        path(self.scope)
    }
    async fn call(
        &self,
        method: &str,
        suffix: &str,
        body: Option<Value>,
    ) -> axum::response::Response {
        let uri = format!("{}{suffix}", self.path());
        request(self.app.clone(), method, &uri, Some(&self.token), body).await
    }
    async fn create(&self, name: &str) -> Uuid {
        let uri = self.path();
        post_id(
            &self.app,
            &self.token,
            &uri,
            json!({"name": name}),
            "/data/id",
        )
        .await
    }
    async fn listed(&self) -> Vec<(Uuid, i64)> {
        let response = self.call("GET", "", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        body_json(response).await["data"]
            .as_array()
            .unwrap_or_else(|| panic!("list must be an array"))
            .iter()
            .map(|p| (uuid_of(&p["id"]), p["position"].as_i64().unwrap_or(-1)))
            .collect()
    }
    async fn ids(&self) -> Vec<Uuid> {
        self.listed().await.into_iter().map(|(id, _)| id).collect()
    }
    async fn reorder(&self, ids: &[Uuid]) -> axum::response::Response {
        self.call("PATCH", "/reorder", Some(json!({"process_ids": ids})))
            .await
    }
    async fn member(&self, pool: &PgPool, email: &str) -> (Uuid, String) {
        let user = create_user(pool, email).await;
        add_member(
            pool,
            self.scope.organization_id,
            self.scope.workspace_id,
            user,
        )
        .await;
        (user, login_token(self.app.clone(), email).await)
    }
}

fn input(name: &str) -> CreateProcessRequest {
    CreateProcessRequest {
        name: name.into(),
        slug: None,
        description: None,
        is_required: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_crud_defaults_and_archive_hides_row(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Taş Alımı").await;
    let process = body_json(f.call("GET", &format!("/{id}"), None).await).await;
    assert_eq!(process["slug"], "tas-alimi");
    assert_eq!(process["work_item_id"], f.scope.work_item_id.to_string());
    assert_eq!(process["section_id"], f.scope.section_id.to_string());
    assert_eq!(process["is_required"], true);
    assert_eq!(process["description"], Value::Null);
    assert_eq!(process["status"], "active");
    assert_eq!(process["position"], 0);
    // Definition rows carry no execution fields.
    for field in [
        "started_at",
        "finished_at",
        "assignee_id",
        "progress",
        "elapsed_seconds",
    ] {
        assert!(process.get(field).is_none(), "{field} must not exist");
    }
    let r = f
        .call(
            "PATCH",
            &format!("/{id}"),
            Some(json!({"name":"Kesim","description":"  Ölçüye göre  ","is_required":false})),
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let updated = body_json(r).await["data"].clone();
    assert_eq!(updated["name"], "Kesim");
    assert_eq!(updated["slug"], "tas-alimi", "omitted slug is preserved");
    assert_eq!(updated["description"], "Ölçüye göre");
    assert_eq!(updated["is_required"], false);
    let cleared = f
        .call("PATCH", &format!("/{id}"), Some(json!({"description":""})))
        .await;
    assert_eq!(body_json(cleared).await["data"]["description"], Value::Null);
    let archived = f
        .call(
            "PATCH",
            &format!("/{id}"),
            Some(json!({"status":"archived"})),
        )
        .await;
    assert_eq!(archived.status(), StatusCode::OK);
    assert_eq!(body_json(archived).await["data"]["status"], "archived");
    assert!(f.ids().await.is_empty());
    for method in ["GET", "PATCH"] {
        let body = Some(json!({"name":"Revive"}));
        let hidden = f.call(method, &format!("/{id}"), body.clone()).await;
        assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
        let unknown = f.call(method, &format!("/{}", Uuid::now_v7()), body).await;
        assert_eq!(
            error_fingerprint(hidden).await,
            error_fingerprint(unknown).await
        );
    }
    let kept: (String, String) = sqlx::query_as("SELECT name, status FROM processes WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(kept, ("Kesim".into(), "archived".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn validation_and_scope_injection_are_rejected(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Original").await;
    for body in [
        json!({"name":" "}),
        json!({"name":"x".repeat(201)}),
        json!({"name":"x","slug":"!!!"}),
        json!({"name":"x","description":"d".repeat(2001)}),
    ] {
        let r = f.call("POST", "", Some(body.clone())).await;
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body_json(r).await["error"]["code"], "VALIDATION_ERROR");
    }
    // Execution states are not configuration statuses.
    for body in [
        json!({"name":" "}),
        json!({"slug":""}),
        json!({"status":"bogus"}),
        json!({"status":"running"}),
        json!({"status":"completed"}),
        json!({"status":"pending"}),
        json!({"description":"d".repeat(2001)}),
    ] {
        let r = f.call("PATCH", &format!("/{id}"), Some(body.clone())).await;
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    for key in [
        "tenant_id",
        "organization_id",
        "workspace_id",
        "project_id",
        "section_id",
        "work_item_id",
        "user_id",
        "role_id",
        "position",
        "started_at",
        "assignee_id",
    ] {
        let attack = json!({"name":"Attack", key: Uuid::now_v7()});
        assert_eq!(
            f.call("POST", "", Some(attack.clone())).await.status(),
            StatusCode::BAD_REQUEST,
            "create {key}"
        );
        assert_eq!(
            f.call(
                "PATCH",
                &format!("/{id}"),
                Some(json!({key: Uuid::now_v7()}))
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST,
            "update {key}"
        );
        assert_eq!(
            f.call(
                "PATCH",
                "/reorder",
                Some(json!({"process_ids":[id], key: Uuid::now_v7()}))
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST,
            "reorder {key}"
        );
    }
    assert_eq!(
        f.call(
            "POST",
            "",
            Some(json!({"name":"Attack","status":"archived"}))
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.call(
            "PATCH",
            "/reorder",
            Some(json!({"process_ids":["not-a-uuid"]}))
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let stored = body_json(f.call("GET", &format!("/{id}"), None).await).await;
    assert_eq!(stored["name"], "Original");
    assert_eq!(stored["work_item_id"], f.scope.work_item_id.to_string());
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM processes")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(rows, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn auth_eligibility_permission_validation_order(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Process").await;
    let (_, member) = f.member(&pool, "process-member@example.test").await;
    create_user(&pool, "process-outsider@example.test").await;
    let foreign = login_token(f.app.clone(), "process-outsider@example.test").await;
    let cases = [
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
    ];
    for (token, expected, code) in cases {
        for (method, suffix, body) in [
            ("POST", String::new(), json!({"name":""})),
            ("PATCH", format!("/{id}"), json!({"name":""})),
            (
                "PATCH",
                "/reorder".to_owned(),
                json!({"process_ids":[Uuid::now_v7()]}),
            ),
        ] {
            let uri = format!("{}{suffix}", f.path());
            let r = request(f.app.clone(), method, &uri, token, Some(body)).await;
            assert_eq!(r.status(), expected, "{method} {suffix}");
            assert_eq!(body_json(r).await["error"]["code"], code);
        }
    }
    for suffix in [String::new(), format!("/{id}")] {
        let uri = format!("{}{suffix}", f.path());
        assert_eq!(
            request(f.app.clone(), "GET", &uri, None, None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(f.app.clone(), "GET", &uri, Some(&member), None)
                .await
                .status(),
            StatusCode::OK,
            "eligible members read"
        );
    }
    // Malformed bodies from an unprivileged member still get 403 first.
    let uri = format!("{}/reorder", f.path());
    let r = request(
        f.app.clone(),
        "PATCH",
        &uri,
        Some(&member),
        Some(json!({"x":1})),
    )
    .await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn strong_attacker_wrong_nesting_and_enumeration_parity(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Secret").await;
    let s = f.scope;
    let (app, token) = (&f.app, f.token.as_str());
    let other_item = Fixture::work_item(
        app,
        token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        s.section_id,
        "Other",
    )
    .await;
    let other_section = Fixture::section(
        app,
        token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        "Sib",
    )
    .await;
    let other_project =
        Fixture::project(app, token, s.organization_id, s.workspace_id, "Other P").await;
    let other_ws = Fixture::workspace(app, token, s.organization_id, "Other Ws").await;
    let other_org = post_id(
        app,
        token,
        "/api/v1/organizations",
        json!({"name":"Other Org"}),
        "/data/organization/id",
    )
    .await;
    let wrong = [
        ProcessScope {
            work_item_id: other_item,
            ..s
        },
        ProcessScope {
            section_id: other_section,
            ..s
        },
        ProcessScope {
            project_id: other_project,
            ..s
        },
        ProcessScope {
            workspace_id: other_ws,
            ..s
        },
        ProcessScope {
            organization_id: other_org,
            ..s
        },
    ];
    for method in ["GET", "PATCH"] {
        let body = Some(json!({"name":"Attack"}));
        let expected = error_fingerprint(
            f.call(method, &format!("/{}", Uuid::now_v7()), body.clone())
                .await,
        )
        .await;
        for uri in wrong
            .iter()
            .map(|scope| format!("{}/{id}", path(*scope)))
            .chain([format!("{}/broken", f.path())])
        {
            let r = request(app.clone(), method, &uri, Some(token), body.clone()).await;
            assert_eq!(r.status(), StatusCode::NOT_FOUND, "{method} {uri}");
            assert_eq!(error_fingerprint(r).await, expected);
        }
    }
    // A real process of another work item cannot be addressed or reordered
    // through this work item; the reorder rejection is the same generic 422.
    let foreign = processes::create_process(
        &pool,
        ProcessScope {
            work_item_id: other_item,
            ..s
        },
        f.actor,
        &input("Foreign"),
    )
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        f.call("GET", &format!("/{}", foreign.id), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let mixed = error_fingerprint(f.reorder(&[id, foreign.id]).await).await;
    let unknown = error_fingerprint(f.reorder(&[id, Uuid::now_v7()]).await).await;
    assert_eq!(mixed, unknown);
    assert_eq!(mixed["code"], "VALIDATION_ERROR");
    let foreign_after: (Uuid, i32) =
        sqlx::query_as("SELECT work_item_id, position FROM processes WHERE id = $1")
            .bind(foreign.id)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(foreign_after, (other_item, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn composite_fk_prevents_cross_scope_rows(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let s = f.scope;
    let (app, token) = (&f.app, f.token.as_str());
    let org = post_id(
        app,
        token,
        "/api/v1/organizations",
        json!({"name":"Foreign Org"}),
        "/data/organization/id",
    )
    .await;
    let ws = Fixture::workspace(app, token, s.organization_id, "Other Ws").await;
    let project = Fixture::project(app, token, s.organization_id, s.workspace_id, "Other").await;
    let section = Fixture::section(
        app,
        token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        "Sib",
    )
    .await;
    let other_item = Fixture::work_item(
        app,
        token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        section,
        "Other",
    )
    .await;
    for bad in [
        ProcessScope {
            organization_id: org,
            ..s
        },
        ProcessScope {
            workspace_id: ws,
            ..s
        },
        ProcessScope {
            project_id: project,
            ..s
        },
        // Right work item, wrong section column (the item lives in `section`).
        ProcessScope {
            work_item_id: other_item,
            ..s
        },
        ProcessScope {
            section_id: section,
            ..s
        },
        ProcessScope {
            work_item_id: Uuid::now_v7(),
            ..s
        },
    ] {
        let error = sqlx::query(
            "INSERT INTO processes (id, tenant_id, workspace_id, project_id, section_id, work_item_id, name, slug, position) \
             VALUES ($1, $2, $3, $4, $5, $6, 'Invalid', 'invalid', 0)",
        )
        .bind(Uuid::now_v7())
        .bind(bad.organization_id)
        .bind(bad.workspace_id)
        .bind(bad.project_id)
        .bind(bad.section_id)
        .bind(bad.work_item_id)
        .execute(&pool)
        .await
        .err()
        .unwrap_or_else(|| panic!("invalid parent chain must be rejected"));
        let db = error
            .as_database_error()
            .unwrap_or_else(|| panic!("expected a database error"));
        assert_eq!(db.code().as_deref(), Some("23503"));
        assert_eq!(db.constraint(), Some("processes_work_item_fk"));
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM processes")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(count, 0);
    // Execution/progress data has no column to live in.
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns WHERE table_name = 'processes' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    for forbidden in [
        "started_at",
        "finished_at",
        "elapsed_seconds",
        "assignee_id",
        "progress",
    ] {
        assert!(!columns.iter().any(|c| c == forbidden), "{forbidden}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn slug_namespace_is_work_item_local_and_survives_archive(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Montaj").await;
    for body in [
        json!({"name":"Other","slug":"MONTAJ"}),
        json!({"name":"montaj"}),
    ] {
        let r = f.call("POST", "", Some(body)).await;
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body_json(r).await["error"]["details"]["fields"]["slug"].is_array());
    }
    let second = f.create("Nakliye").await;
    let r = f
        .call(
            "PATCH",
            &format!("/{second}"),
            Some(json!({"slug":"Montaj"})),
        )
        .await;
    assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    f.call(
        "PATCH",
        &format!("/{id}"),
        Some(json!({"status":"archived"})),
    )
    .await;
    assert_eq!(
        f.call("POST", "", Some(json!({"name":"Montaj"})))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "archive keeps the slug occupied"
    );
    exec(
        &pool,
        "UPDATE processes SET deleted_at = now() WHERE slug = 'nakliye'",
    )
    .await;
    assert_eq!(
        f.call("POST", "", Some(json!({"name":"Nakliye"})))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "soft deletion keeps the slug occupied"
    );
    let s = f.scope;
    let other_item = Fixture::work_item(
        &f.app,
        &f.token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        s.section_id,
        "Ada",
    )
    .await;
    let same = processes::create_process(
        &pool,
        ProcessScope {
            work_item_id: other_item,
            ..s
        },
        f.actor,
        &input("Montaj"),
    )
    .await;
    assert!(same.is_ok(), "slugs are work-item local");
}

#[sqlx::test(migrations = "../../migrations")]
async fn append_and_server_authoritative_reorder(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let a = f.create("A").await;
    let b = f.create("B").await;
    let c = f.create("C").await;
    assert_eq!(f.listed().await, vec![(a, 0), (b, 1), (c, 2)]);
    let r = f.reorder(&[c, a, b]).await;
    assert_eq!(r.status(), StatusCode::OK);
    let returned: Vec<Uuid> = body_json(r).await["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|p| uuid_of(&p["id"]))
        .collect();
    assert_eq!(returned, vec![c, a, b]);
    assert_eq!(f.listed().await, vec![(c, 0), (a, 1), (b, 2)]);
    // Invalid permutations change nothing.
    for bad in [
        vec![c, a],
        vec![c, a, b, b],
        vec![c, a, a],
        vec![c, a, b, Uuid::now_v7()],
        vec![],
    ] {
        let r = f.reorder(&bad).await;
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY, "{bad:?}");
        assert!(body_json(r).await["error"]["details"]["fields"]["process_ids"].is_array());
    }
    assert_eq!(f.ids().await, vec![c, a, b]);
    // Archived processes are no longer part of the active ordering.
    f.call(
        "PATCH",
        &format!("/{a}"),
        Some(json!({"status":"archived"})),
    )
    .await;
    assert_eq!(
        f.reorder(&[b, a, c]).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(f.reorder(&[b, c]).await.status(), StatusCode::OK);
    assert_eq!(f.listed().await, vec![(b, 0), (c, 1)]);
    // Append goes after every stored row, archived included.
    let d = f.create("D").await;
    let listed = f.listed().await;
    assert_eq!(listed.last().map(|(id, _)| *id), Some(d));
    assert!(listed.last().map(|(_, p)| *p).unwrap_or(0) > 1);
    // Reordering an empty work item is a valid no-op.
    let s = f.scope;
    let empty = Fixture::work_item(
        &f.app,
        &f.token,
        s.organization_id,
        s.workspace_id,
        s.project_id,
        s.section_id,
        "Empty",
    )
    .await;
    let uri = format!(
        "{}/reorder",
        path(ProcessScope {
            work_item_id: empty,
            ..s
        })
    );
    let r = request(
        f.app.clone(),
        "PATCH",
        &uri,
        Some(&f.token),
        Some(json!({"process_ids": []})),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(body_json(r).await["data"], json!([]));
    // The static reorder segment does not shadow real process routes.
    assert_eq!(
        f.call("GET", "/reorder", None).await.status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        f.call("GET", &format!("/{d}"), None).await.status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_rejects_duplicate_active_positions(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let a = f.create("A").await;
    let s = f.scope;
    let insert = |status: &'static str| {
        sqlx::query(
            "INSERT INTO processes (id, tenant_id, workspace_id, project_id, section_id, work_item_id, name, slug, position, status) \
             VALUES ($1, $2, $3, $4, $5, $6, 'Dup', $7, 0, $8)",
        )
        .bind(Uuid::now_v7())
        .bind(s.organization_id)
        .bind(s.workspace_id)
        .bind(s.project_id)
        .bind(s.section_id)
        .bind(s.work_item_id)
        .bind(format!("dup-{status}"))
        .bind(status)
    };
    let error = insert("active")
        .execute(&pool)
        .await
        .err()
        .unwrap_or_else(|| panic!("duplicate active position must be rejected"));
    assert_eq!(
        error.as_database_error().and_then(|db| db.constraint()),
        Some("processes_active_position_unique")
    );
    // Archived rows keep historical positions outside the active order.
    insert("archived")
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(f.ids().await, vec![a]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn required_and_optional_are_stored_configuration(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let r = f
        .call(
            "POST",
            "",
            Some(json!({"name":"Nakliye","is_required":false})),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let created = body_json(r).await["data"].clone();
    assert_eq!(created["is_required"], false);
    let id = uuid_of(&created["id"]);
    let r = f
        .call(
            "PATCH",
            &format!("/{id}"),
            Some(json!({"is_required":true})),
        )
        .await;
    assert_eq!(body_json(r).await["data"]["is_required"], true);
    let r = f
        .call(
            "PATCH",
            &format!("/{id}"),
            Some(json!({"is_required":"yes"})),
        )
        .await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn permission_keys_are_decomposed_and_revocable(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let a = f.create("A").await;
    let b = f.create("B").await;
    let (user, token) = f.member(&pool, "process-grantee@example.test").await;
    let (org, ws) = (f.scope.organization_id, f.scope.workspace_id);
    let call = |method: &'static str, suffix: String, body: Value| {
        let (app, token) = (f.app.clone(), token.clone());
        let uri = format!("{}{suffix}", f.path());
        async move {
            request(app, method, &uri, Some(&token), Some(body))
                .await
                .status()
        }
    };
    let reorder = || call("PATCH", "/reorder".into(), json!({"process_ids":[b, a]}));
    let rename = || call("PATCH", format!("/{a}"), json!({"name":"Renamed"}));
    let archive = || call("PATCH", format!("/{b}"), json!({"status":"archived"}));
    grant(&pool, org, ws, user, &["processes:create"]).await;
    assert_eq!(
        call("POST", String::new(), json!({"name":"C"})).await,
        StatusCode::CREATED
    );
    assert_eq!(rename().await, StatusCode::FORBIDDEN);
    assert_eq!(reorder().await, StatusCode::FORBIDDEN);
    grant(&pool, org, ws, user, &["processes:update"]).await;
    assert_eq!(rename().await, StatusCode::OK);
    assert_eq!(
        archive().await,
        StatusCode::FORBIDDEN,
        "archive needs its own key"
    );
    assert_eq!(
        reorder().await,
        StatusCode::FORBIDDEN,
        "update does not imply reorder"
    );
    grant(&pool, org, ws, user, &["processes:reorder"]).await;
    let c = f.ids().await[2];
    let r = request(
        f.app.clone(),
        "PATCH",
        &format!("{}/reorder", f.path()),
        Some(&token),
        Some(json!({"process_ids":[b, a, c]})),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
    grant(&pool, org, ws, user, &["processes:archive"]).await;
    assert_eq!(archive().await, StatusCode::OK);
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        call("POST", String::new(), json!({"name":"D"})).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(rename().await, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn archive_without_update_is_forbidden(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let a = f.create("A").await;
    let (user, token) = f.member(&pool, "archive-only@example.test").await;
    grant(
        &pool,
        f.scope.organization_id,
        f.scope.workspace_id,
        user,
        &["processes:archive"],
    )
    .await;
    let r = request(
        f.app.clone(),
        "PATCH",
        &format!("{}/{a}", f.path()),
        Some(&token),
        Some(json!({"status":"archived"})),
    )
    .await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.ids().await, vec![a]);
}

// Deterministic TOCTOU: prove eligibility/permission first, commit the
// adversarial change, then call the write services with the stale scope.
// No sleeps and no test-only production hooks.
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
            processes::PROCESSES_CREATE
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"))
    );
    sqlx::query(change)
        .bind(s.organization_id)
        .execute(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let create = processes::create_process(&pool, s, f.actor, &input("New")).await;
    let update = processes::update_process(
        &pool,
        s,
        id,
        f.actor,
        &UpdateProcessRequest {
            name: Some("Unauthorized".into()),
            ..Default::default()
        },
    )
    .await;
    let archive = processes::update_process(
        &pool,
        s,
        id,
        f.actor,
        &UpdateProcessRequest {
            status: Some("archived".into()),
            ..Default::default()
        },
    )
    .await;
    let reorder = processes::reorder_processes(
        &pool,
        s,
        f.actor,
        &ReorderProcessesRequest {
            process_ids: vec![id],
        },
    )
    .await
    .map(|_| ());
    for result in [
        create.map(|_| ()),
        update.map(|_| ()),
        archive.map(|_| ()),
        reorder,
    ] {
        if forbidden {
            assert!(matches!(result, Err(ProcessError::Forbidden)), "{result:?}");
        } else {
            assert!(
                matches!(result, Err(ProcessError::NotAccessible)),
                "{result:?}"
            );
        }
    }
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT name, status FROM processes WHERE tenant_id = $1")
            .bind(s.organization_id)
            .fetch_all(&pool)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(rows, vec![("Original".into(), "active".into())]);
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
    revoked_state(pool, "DELETE FROM role_permissions WHERE role_id IN (SELECT id FROM roles WHERE tenant_id=$1) AND permission_id IN (SELECT id FROM permissions WHERE key LIKE 'processes:%')", true).await;
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
async fn tx_revalidates_archived_section(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE sections SET status='archived' WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_archived_work_item(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE work_items SET status='archived' WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_deleted_work_item(pool: PgPool) {
    revoked_state(
        pool,
        "UPDATE work_items SET deleted_at=now() WHERE tenant_id=$1",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_archived_process(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Original").await;
    exec(&pool, "UPDATE processes SET status='archived'").await;
    let result = processes::update_process(
        &pool,
        f.scope,
        id,
        f.actor,
        &UpdateProcessRequest {
            name: Some("Revived".into()),
            status: Some("active".into()),
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(result, Err(ProcessError::NotAccessible)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn memberships_and_parent_lifecycle_gate_all_http_routes(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Process").await;
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
            "UPDATE work_items SET status='archived'",
            "UPDATE work_items SET status='active'",
        ),
        (
            "UPDATE work_items SET deleted_at=now()",
            "UPDATE work_items SET deleted_at=NULL",
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
        exec(&pool, revoke).await;
        for (method, suffix, body) in [
            ("GET", String::new(), json!({})),
            ("POST", String::new(), json!({"name":"Changed"})),
            ("PATCH", "/reorder".to_owned(), json!({"process_ids":[id]})),
            ("GET", format!("/{id}"), json!({})),
            ("PATCH", format!("/{id}"), json!({"name":"Changed"})),
        ] {
            assert_eq!(
                f.call(method, &suffix, Some(body)).await.status(),
                StatusCode::NOT_FOUND,
                "{revoke} {method} {suffix}"
            );
        }
        exec(&pool, restore).await;
    }
    // A completed (non-archived) work item stays configurable.
    exec(&pool, "UPDATE work_items SET status='completed'").await;
    assert_eq!(
        f.call("GET", &format!("/{id}"), None).await.status(),
        StatusCode::OK
    );
    let stored = body_json(f.call("GET", &format!("/{id}"), None).await).await;
    assert_eq!(stored["name"], "Process");
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_same_slug_has_one_winner(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let body = input("Kesim");
    let (a, b) = tokio::join!(
        processes::create_process(&pool, f.scope, f.actor, &body),
        processes::create_process(&pool, f.scope, f.actor, &body)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(ProcessError::InvalidFields(_)))
            || matches!(b, Err(ProcessError::InvalidFields(_)))
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM processes")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_appends_get_distinct_positions(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let names = ["P1", "P2", "P3", "P4", "P5"].map(input);
    let results = futures_join(&pool, f.scope, f.actor, &names).await;
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut positions: Vec<i64> = f.listed().await.into_iter().map(|(_, p)| p).collect();
    positions.sort_unstable();
    assert_eq!(positions, vec![0, 1, 2, 3, 4]);
}

async fn futures_join(
    pool: &PgPool,
    scope: ProcessScope,
    actor: Uuid,
    inputs: &[CreateProcessRequest; 5],
) -> Vec<Result<processes::ProcessPublic, ProcessError>> {
    let [a, b, c, d, e] = inputs;
    let (a, b, c, d, e) = tokio::join!(
        processes::create_process(pool, scope, actor, a),
        processes::create_process(pool, scope, actor, b),
        processes::create_process(pool, scope, actor, c),
        processes::create_process(pool, scope, actor, d),
        processes::create_process(pool, scope, actor, e),
    );
    vec![a, b, c, d, e]
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_reorders_leave_one_valid_ordering(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let a = f.create("A").await;
    let b = f.create("B").await;
    let c = f.create("C").await;
    let first = ReorderProcessesRequest {
        process_ids: vec![c, b, a],
    };
    let second = ReorderProcessesRequest {
        process_ids: vec![b, a, c],
    };
    let (x, y) = tokio::join!(
        processes::reorder_processes(&pool, f.scope, f.actor, &first),
        processes::reorder_processes(&pool, f.scope, f.actor, &second),
    );
    assert!(
        x.is_ok() && y.is_ok(),
        "same-set reorders serialize: {x:?} {y:?}"
    );
    let listed = f.listed().await;
    let ids: Vec<Uuid> = listed.iter().map(|(id, _)| *id).collect();
    assert!(
        ids == first.process_ids || ids == second.process_ids,
        "{ids:?}"
    );
    assert_eq!(
        listed.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    // Reorder racing an append: whichever commits first, the final active set
    // is complete, duplicate-free and strictly ordered.
    let append = input("D");
    let (r, n) = tokio::join!(
        processes::reorder_processes(&pool, f.scope, f.actor, &first),
        processes::create_process(&pool, f.scope, f.actor, &append),
    );
    assert!(n.is_ok());
    assert!(r.is_ok() || matches!(r, Err(ProcessError::InvalidFields(_))));
    let listed = f.listed().await;
    assert_eq!(listed.len(), 4);
    let mut positions: Vec<i64> = listed.iter().map(|(_, p)| *p).collect();
    positions.dedup();
    assert_eq!(
        positions.len(),
        4,
        "strictly increasing positions: {listed:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_archive_and_reorder_leave_valid_ordering(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let a = f.create("A").await;
    let b = f.create("B").await;
    let c = f.create("C").await;
    // Whichever commits first (the project lock serializes them), the final
    // state must be the same: `a` archived and the remaining set ordered.
    let archive_input = UpdateProcessRequest {
        status: Some("archived".into()),
        ..Default::default()
    };
    let reorder_input = ReorderProcessesRequest {
        process_ids: vec![c, b, a],
    };
    let archive = processes::update_process(&pool, f.scope, a, f.actor, &archive_input);
    let reorder = processes::reorder_processes(&pool, f.scope, f.actor, &reorder_input);
    let (archived, reordered) = tokio::join!(archive, reorder);
    assert!(archived.is_ok(), "{archived:?}");
    // If the reorder ran first it succeeds; after the archive commits the
    // requested set is stale and must be rejected, never partially applied.
    assert!(reordered.is_ok() || matches!(reordered, Err(ProcessError::InvalidFields(_))));
    // Archive-first leaves [b, c] at their original positions; reorder-first
    // leaves [c, b] normalized to 0..1. Either way the active set is complete
    // and duplicate-free with strictly increasing positions.
    let listed = f.listed().await;
    let ids: Vec<Uuid> = listed.iter().map(|(id, _)| *id).collect();
    assert!(ids == vec![b, c] || ids == vec![c, b], "{ids:?}");
    let positions: Vec<i64> = listed.iter().map(|(_, p)| *p).collect();
    assert!(positions[0] < positions[1], "{positions:?}");
    let stored: (String,) = sqlx::query_as("SELECT status FROM processes WHERE id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(stored.0, "archived");
}

#[sqlx::test(migrations = "../../migrations")]
async fn hostile_cookies_do_not_change_process_scope(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.create("Process").await;
    let cookies = vec![
        format!("organization={}", Uuid::now_v7()),
        format!("workspace={}", Uuid::now_v7()),
    ];
    for (method, suffix, body, status) in [
        ("GET", format!("/{id}"), None, StatusCode::OK),
        (
            "POST",
            String::new(),
            Some(json!({"name":"Cookie"})),
            StatusCode::CREATED,
        ),
        (
            "PATCH",
            format!("/{id}"),
            Some(json!({"name":"Updated"})),
            StatusCode::OK,
        ),
    ] {
        let uri = format!("{}{suffix}", f.path());
        assert_eq!(
            request_with_extra_cookies(f.app.clone(), method, &uri, Some(&f.token), body, &cookies)
                .await
                .status(),
            status
        );
    }
    create_user(&pool, "cookie-outsider@example.test").await;
    let token = login_token(f.app.clone(), "cookie-outsider@example.test").await;
    let cookies = vec![
        format!("organization={}", f.scope.organization_id),
        format!("workspace={}", f.scope.workspace_id),
    ];
    let uri = format!("{}/{id}", f.path());
    assert_eq!(
        request_with_extra_cookies(f.app.clone(), "GET", &uri, Some(&token), None, &cookies)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_grants_cannot_escape_scope(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let s = f.scope;
    let (user, token) = f.member(&pool, "scoped-grantee@example.test").await;
    let (app, owner) = (&f.app, f.token.as_str());
    let ws = Fixture::workspace(app, owner, s.organization_id, "Other Ws").await;
    let project = Fixture::project(app, owner, s.organization_id, ws, "Other P").await;
    let section = Fixture::section(app, owner, s.organization_id, ws, project, "Other S").await;
    let item = Fixture::work_item(
        app,
        owner,
        s.organization_id,
        ws,
        project,
        section,
        "Other I",
    )
    .await;
    add_ws_membership(&pool, s.organization_id, ws, user).await;
    grant(
        &pool,
        s.organization_id,
        s.workspace_id,
        user,
        &["processes:create"],
    )
    .await;
    let other = ProcessScope {
        workspace_id: ws,
        project_id: project,
        section_id: section,
        work_item_id: item,
        ..s
    };
    for (scope, expected) in [(s, StatusCode::CREATED), (other, StatusCode::FORBIDDEN)] {
        assert_eq!(
            request(
                app.clone(),
                "POST",
                &path(scope),
                Some(&token),
                Some(json!({"name":"Scoped"}))
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
    let id = f.create("Process").await;
    async fn handler(context: platform_server::context::ProcessContext) -> axum::Json<Value> {
        axum::Json(json!({
            "id": context.process.id,
            "work_item": context.parent.work_item.id,
            "section": context.parent.parent.scope.section_id,
        }))
    }
    let app = axum::Router::new()
        .route(
            "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions/{execution_id}",
            axum::routing::get(handler),
        )
        .with_state(state(&pool));
    let r = request(
        app.clone(),
        "GET",
        &format!("{}/{id}/executions/extra", f.path()),
        Some(&f.token),
        None,
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
    let b = body_json(r).await;
    assert_eq!(b["id"], id.to_string());
    assert_eq!(b["work_item"], f.scope.work_item_id.to_string());
    assert_eq!(b["section"], f.scope.section_id.to_string());
    let r = request(
        app,
        "GET",
        &format!("{}/{}/executions/extra", f.path(), Uuid::now_v7()),
        Some(&f.token),
        None,
    )
    .await;
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn migration_backfill_preserves_assignments_and_member_has_no_grants(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let org = f.scope.organization_id;
    let before: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM membership_roles ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    // Locate the pre-processes version BY NAME, not by position.
    let up: Vec<(i64, String)> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| (m.version, m.description.to_string()))
        .collect();
    let index = up
        .iter()
        .position(|(_, description)| description == "processes")
        .unwrap_or_else(|| panic!("the processes migration must exist in the chain"));
    MIGRATOR
        .undo(&pool, up[index - 1].0)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM permissions WHERE key LIKE 'processes:%'")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(count, 0);
    MIGRATOR
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let grants: Vec<String> = sqlx::query_scalar(
        "SELECT p.key FROM role_permissions rp JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.tenant_id = $1 AND r.is_system AND r.name = 'owner' AND p.key LIKE 'processes:%' \
         ORDER BY p.key",
    )
    .bind(org)
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        grants,
        vec![
            "processes:archive",
            "processes:assign",
            "processes:create",
            "processes:reorder",
            "processes:update"
        ]
    );
    let after: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM membership_roles ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(before, after, "backfill never writes role assignments");
    // ADR 0016 + 0019: backfills grant built-in Members exactly the three
    // execution keys and the two time-session keys at organization scope —
    // and nothing else.
    let member_keys: Vec<String> = sqlx::query_scalar(
        "SELECT p.key FROM role_permissions rp JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.name = 'member' AND r.tenant_id = $1 ORDER BY p.key",
    )
    .bind(org)
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        member_keys,
        vec![
            "process_executions:cancel",
            "process_executions:complete",
            "process_executions:start",
            "time_sessions:start",
            "time_sessions:stop"
        ]
    );
    // A fresh organization's Owner bootstrap includes the same keys.
    let fresh = post_id(
        &f.app,
        &f.token,
        "/api/v1/organizations",
        json!({"name":"Fresh Org"}),
        "/data/organization/id",
    )
    .await;
    let fresh_grants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE r.tenant_id = $1 AND r.name = 'owner' AND p.key LIKE 'processes:%'",
    )
    .bind(fresh)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(fresh_grants, 5);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rollback_is_blocked_by_dependents_and_reapplies(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    f.create("Kept").await;
    // The newest migrations are 014 (sessions table), 013 (assignment
    // columns/grant) then 012 (index-only), all without dependents on the
    // executions table itself: a dependent object on the executions table
    // must block 011's down migration without CASCADE.
    exec(
        &pool,
        "CREATE TABLE process_dependency_probe (execution_id uuid REFERENCES process_executions (id))",
    )
    .await;
    platform_server::migrations::revert_last(&pool)
        .await
        .unwrap_or_else(|error| panic!("014 sessions down must apply: {error}"));
    // 013 and 012 have no dependents on the executions table; undo peels
    // them, then the probe blocks 011.
    assert!(MIGRATOR.undo(&pool, 20260924100000).await.is_err());
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM processes")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(kept, 1, "a blocked rollback keeps data");
    exec(&pool, "DROP TABLE process_dependency_probe").await;
    // Without dependents the down migration (executions table) succeeds and
    // the chain reapplies cleanly.
    MIGRATOR
        .undo(&pool, 20260924100000)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    MIGRATOR
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    platform_server::migrations::verify(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
}
