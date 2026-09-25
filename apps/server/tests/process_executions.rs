use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState,
    config::AuthConfig,
    migrations::MIGRATOR,
    password::PasswordService,
    process_executions::{self, ExecutionError, ExecutionScope, StartExecutionRequest},
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

/// Mirrors invitation acceptance: the tenant's built-in member role assigned
/// at ORGANIZATION scope (workspace_id NULL).
async fn assign_member_role(pool: &PgPool, org: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         SELECT $1, $2, $3, r.id, NULL FROM roles r \
         WHERE r.tenant_id = $2 AND r.name = 'member' AND r.is_system",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(user)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("member role must be assignable"));
}

/// Workspace-scoped custom role holding exactly `keys` (created on first use,
/// extended on later calls so grants can be added step by step).
async fn grant(pool: &PgPool, org: Uuid, ws: Uuid, user: Uuid, keys: &[&str]) {
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id = $1 AND name = 'ws-exec-role'")
            .bind(org)
            .fetch_optional(pool)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
    let role = match existing {
        Some(role) => role,
        None => {
            let role: Uuid = sqlx::query_scalar(
                "INSERT INTO roles (id, tenant_id, name, is_system) \
                 VALUES ($1, $2, 'ws-exec-role', false) RETURNING id",
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
fn item_uri(org: Uuid, ws: Uuid, project: Uuid, section: Uuid, work_item: Uuid) -> String {
    format!(
        "{}/projects/{project}/sections/{section}/work-items/{work_item}",
        base(org, ws)
    )
}
fn process_uri(scope: &ExecutionScope) -> String {
    format!(
        "{}/processes/{}",
        item_uri(
            scope.organization_id,
            scope.workspace_id,
            scope.project_id,
            scope.section_id,
            scope.work_item_id
        ),
        scope.process_id
    )
}
fn executions_uri(scope: &ExecutionScope) -> String {
    format!("{}/executions", process_uri(scope))
}
fn execution_uri(scope: &ExecutionScope, id: Uuid, action: &str) -> String {
    format!("{}/{id}/{action}", executions_uri(scope))
}
fn work_item_executions_uri(scope: &ExecutionScope) -> String {
    format!(
        "{}/executions",
        item_uri(
            scope.organization_id,
            scope.workspace_id,
            scope.project_id,
            scope.section_id,
            scope.work_item_id
        )
    )
}

struct Fixture {
    app: axum::Router,
    token: String,
    actor: Uuid,
    scope: ExecutionScope,
}
impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        let email = format!("exec-owner-{}@example.test", Uuid::now_v7());
        let actor = create_user(pool, &email).await;
        let app = router(state(pool));
        let token = login_token(app.clone(), &email).await;
        let org = post_id(
            &app,
            &token,
            "/api/v1/organizations",
            json!({"name": format!("Exec Org {}", Uuid::now_v7())}),
            "/data/organization/id",
        )
        .await;
        let ws = post_id(
            &app,
            &token,
            &format!("/api/v1/organizations/{org}/workspaces"),
            json!({"name": "Exec Ws"}),
            "/data/workspace/id",
        )
        .await;
        let project = post_id(
            &app,
            &token,
            &format!("{}/projects", base(org, ws)),
            json!({"name": "Project"}),
            "/data/id",
        )
        .await;
        let section = post_id(
            &app,
            &token,
            &format!("{}/projects/{project}/sections", base(org, ws)),
            json!({"name": "Section"}),
            "/data/id",
        )
        .await;
        let work_item = post_id(
            &app,
            &token,
            &format!(
                "{}/projects/{project}/sections/{section}/work-items",
                base(org, ws)
            ),
            json!({"name": "Tezgah"}),
            "/data/id",
        )
        .await;
        let process = post_id(
            &app,
            &token,
            &format!(
                "{}/processes",
                item_uri(org, ws, project, section, work_item)
            ),
            json!({"name": "Kesim"}),
            "/data/id",
        )
        .await;
        Self {
            app,
            token,
            actor,
            scope: ExecutionScope {
                organization_id: org,
                workspace_id: ws,
                project_id: project,
                section_id: section,
                work_item_id: work_item,
                process_id: process,
            },
        }
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
    /// Member holding the built-in member role (invitation-acceptance shape).
    async fn builtin_member(&self, pool: &PgPool, email: &str) -> (Uuid, String) {
        let (user, token) = self.member(pool, email).await;
        assign_member_role(pool, self.scope.organization_id, user).await;
        (user, token)
    }
    async fn start(&self) -> axum::response::Response {
        self.start_as(None, None).await
    }
    async fn start_as(&self, token: Option<&str>, body: Option<Value>) -> axum::response::Response {
        request(
            self.app.clone(),
            "POST",
            &executions_uri(&self.scope),
            Some(token.unwrap_or(&self.token)),
            body.or(Some(json!({}))),
        )
        .await
    }
    async fn started(&self) -> Uuid {
        let response = self.start().await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        uuid_of(&body["data"]["id"])
    }
    async fn complete(&self, id: Uuid) -> axum::response::Response {
        self.complete_as(id, None).await
    }
    async fn complete_as(&self, id: Uuid, token: Option<&str>) -> axum::response::Response {
        request(
            self.app.clone(),
            "POST",
            &execution_uri(&self.scope, id, "complete"),
            Some(token.unwrap_or(&self.token)),
            None,
        )
        .await
    }
    async fn cancel(&self, id: Uuid) -> axum::response::Response {
        self.cancel_as(id, None, None).await
    }
    async fn cancel_as(
        &self,
        id: Uuid,
        token: Option<&str>,
        body: Option<Value>,
    ) -> axum::response::Response {
        request(
            self.app.clone(),
            "POST",
            &execution_uri(&self.scope, id, "cancel"),
            Some(token.unwrap_or(&self.token)),
            body.or(Some(json!({}))),
        )
        .await
    }
    async fn history(&self) -> Vec<Value> {
        self.history_as(None).await
    }
    async fn history_as(&self, token: Option<&str>) -> Vec<Value> {
        let response = request(
            self.app.clone(),
            "GET",
            &work_item_executions_uri(&self.scope),
            Some(token.unwrap_or(&self.token)),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        body_json(response).await["data"]
            .as_array()
            .unwrap_or_else(|| panic!("history must be an array"))
            .clone()
    }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn start_complete_lifecycle_with_actor_and_time_history(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let response = f
        .start_as(None, Some(json!({"start_reason": "  İlk deneme  "})))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    let attempt = &body["data"];
    assert_eq!(attempt["attempt_no"], 1);
    assert_eq!(attempt["status"], "active");
    assert_eq!(attempt["process_id"], f.scope.process_id.to_string());
    assert_eq!(attempt["work_item_id"], f.scope.work_item_id.to_string());
    assert_eq!(attempt["started_by_user_id"], f.actor.to_string());
    assert_eq!(attempt["start_reason"], "İlk deneme");
    // DB-authoritative timestamps serialize as ISO-8601 UTC strings.
    assert!(
        attempt["started_at"]
            .as_str()
            .is_some_and(|s| s.ends_with('Z'))
    );
    for field in [
        "completed_at",
        "cancelled_at",
        "completed_by_user_id",
        "cancelled_by_user_id",
        "cancel_reason",
    ] {
        assert_eq!(
            attempt[field],
            Value::Null,
            "{field} must be null while active"
        );
    }
    assert!(
        body["server_time"]
            .as_str()
            .is_some_and(|s| s.ends_with('Z'))
    );
    // Same actor may complete the attempt it started.
    let id = uuid_of(&attempt["id"]);
    let response = f.complete(id).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let done = &body["data"];
    assert_eq!(done["status"], "completed");
    assert_eq!(done["completed_by_user_id"], f.actor.to_string());
    assert!(
        done["completed_at"]
            .as_str()
            .is_some_and(|s| s.ends_with('Z'))
    );
    assert_eq!(done["started_at"], attempt["started_at"]);
    assert!(body["server_time"].as_str().is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_records_reason_and_actor(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.started().await;
    let response = f
        .cancel_as(id, None, Some(json!({"cancel_reason": " Malzeme yok "})))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["data"]["status"], "cancelled");
    assert_eq!(body["data"]["cancel_reason"], "Malzeme yok");
    assert_eq!(body["data"]["cancelled_by_user_id"], f.actor.to_string());
    assert!(body["data"]["cancelled_at"].as_str().is_some());
    // Cancellation without a reason stores NULL.
    let second = f.started().await;
    let response = f.cancel_as(second, None, Some(json!({}))).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await["data"]["cancel_reason"],
        Value::Null
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn retry_creates_sequential_immutable_attempts(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let first = f.started().await;
    assert_eq!(f.complete(first).await.status(), StatusCode::OK);
    let second = f.started().await;
    assert_eq!(f.cancel(second).await.status(), StatusCode::OK);
    let third = f.started().await;
    let history = f.history().await;
    let attempts: Vec<i64> = history
        .iter()
        .map(|row| row["attempt_no"].as_i64().unwrap_or(-1))
        .collect();
    assert_eq!(attempts, vec![1, 2, 3]);
    let statuses: Vec<&str> = history
        .iter()
        .map(|row| row["status"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(statuses, vec!["completed", "cancelled", "active"]);
    // Terminal attempts are immutable operational history.
    for (id, action) in [(first, "complete"), (first, "cancel"), (second, "complete")] {
        let response = request(
            f.app.clone(),
            "POST",
            &execution_uri(&f.scope, id, action),
            Some(&f.token),
            Some(json!({})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{id} {action}");
        assert_eq!(body_json(response).await["error"]["code"], "STATE_CONFLICT");
    }
    assert_eq!(f.complete(third).await.status(), StatusCode::OK);
    let history = f.history().await;
    assert_eq!(history.len(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn pending_is_implicit_and_double_start_conflicts(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    assert!(f.history().await.is_empty(), "pending has no stored row");
    let id = f.started().await;
    let response = f.start().await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(response).await["error"]["code"], "STATE_CONFLICT");
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM process_executions WHERE process_id = $1 AND status = 'active'",
    )
    .bind(f.scope.process_id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(active, 1);
    assert_eq!(f.cancel(id).await.status(), StatusCode::OK);
    // After cancel a fresh attempt can start — no stored pending either way.
    let next = f.started().await;
    assert_ne!(next, id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reasons_are_bounded_and_client_fields_rejected(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let long = "x".repeat(501);
    let response = f.start_as(None, Some(json!({"start_reason": long}))).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    // Whitespace-only normalizes to NULL.
    let response = f.start_as(None, Some(json!({"start_reason": "   "}))).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    assert_eq!(body["data"]["start_reason"], Value::Null);
    let id = uuid_of(&body["data"]["id"]);
    // Scope/actor/state fields are server-controlled: unknown fields 400.
    let response = f
        .cancel_as(
            id,
            None,
            Some(json!({"cancelled_by_user_id": Uuid::now_v7()})),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = f
        .cancel_as(id, None, Some(json!({"cancel_reason": long})))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------------------------------------------------------------------------
// Isolation + error precedence
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn foreign_scope_and_ids_are_uniform_404s(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let other = Fixture::new(&pool).await;
    let id = f.started().await;
    // Every wrong-scope combination is the same 404 fingerprint.
    let wrong = ExecutionScope {
        organization_id: other.scope.organization_id,
        ..f.scope
    };
    let expected = error_fingerprint(
        request(
            f.app.clone(),
            "POST",
            &executions_uri(&wrong),
            Some(&f.token),
            Some(json!({})),
        )
        .await,
    )
    .await;
    for scope in [
        ExecutionScope {
            workspace_id: other.scope.workspace_id,
            ..f.scope
        },
        ExecutionScope {
            project_id: other.scope.project_id,
            ..f.scope
        },
        ExecutionScope {
            section_id: other.scope.section_id,
            ..f.scope
        },
        ExecutionScope {
            work_item_id: other.scope.work_item_id,
            ..f.scope
        },
        ExecutionScope {
            process_id: other.scope.process_id,
            ..f.scope
        },
        ExecutionScope {
            process_id: Uuid::now_v7(),
            ..f.scope
        },
    ] {
        let fingerprint = error_fingerprint(
            request(
                f.app.clone(),
                "POST",
                &executions_uri(&scope),
                Some(&f.token),
                Some(json!({})),
            )
            .await,
        )
        .await;
        assert_eq!(fingerprint, expected);
    }
    // Correct process under wrong execution / wrong process under correct
    // execution: still uniform 404.
    for uri in [
        execution_uri(&f.scope, Uuid::now_v7(), "complete"),
        execution_uri(
            &ExecutionScope {
                process_id: other.scope.process_id,
                ..f.scope
            },
            id,
            "complete",
        ),
    ] {
        let fingerprint =
            error_fingerprint(request(f.app.clone(), "POST", &uri, Some(&f.token), None).await)
                .await;
        assert_eq!(fingerprint, expected);
    }
    // Cross-tenant history read is a 404 as well.
    let response = request(
        f.app.clone(),
        "GET",
        &work_item_executions_uri(&ExecutionScope {
            work_item_id: other.scope.work_item_id,
            ..f.scope
        }),
        Some(&f.token),
        None,
    )
    .await;
    assert_eq!(error_fingerprint(response).await, expected);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unauthenticated_requests_get_401_before_any_routing(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    for (method, uri) in [
        ("POST", executions_uri(&f.scope)),
        ("GET", work_item_executions_uri(&f.scope)),
        ("POST", execution_uri(&f.scope, Uuid::now_v7(), "complete")),
        ("POST", execution_uri(&f.scope, Uuid::now_v7(), "cancel")),
    ] {
        let response = request(f.app.clone(), method, &uri, None, None).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
    }
    // Hostile extra cookies do not leak past authentication.
    let response = request_with_extra_cookies(
        f.app.clone(),
        "POST",
        &executions_uri(&f.scope),
        None,
        Some(json!({})),
        &["platform_session=forged".into(), "admin=1".into()],
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// RBAC
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn builtin_member_executes_but_never_administers(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (_member, member_token) = f.builtin_member(&pool, "exec-member@example.test").await;
    let response = f.start_as(Some(&member_token), Some(json!({}))).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let id = uuid_of(&body_json(response).await["data"]["id"]);
    assert_eq!(
        f.cancel_as(id, Some(&member_token), None).await.status(),
        StatusCode::OK
    );
    let response = f.start_as(Some(&member_token), None).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let id = uuid_of(&body_json(response).await["data"]["id"]);
    assert_eq!(
        f.complete_as(id, Some(&member_token)).await.status(),
        StatusCode::OK
    );
    let history = f.history_as(Some(&member_token)).await;
    assert_eq!(history.len(), 2);
    // Member grants never unlock process-definition administration.
    let uri = process_uri(&f.scope);
    let response = request(
        f.app.clone(),
        "PATCH",
        &uri,
        Some(&member_token),
        Some(json!({"name": "Hijack"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = request(
        f.app.clone(),
        "POST",
        &format!(
            "{}/processes",
            item_uri(
                f.scope.organization_id,
                f.scope.workspace_id,
                f.scope.project_id,
                f.scope.section_id,
                f.scope.work_item_id,
            )
        ),
        Some(&member_token),
        Some(json!({"name": "Nope"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn execution_permissions_decompose(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (user, token) = f.member(&pool, "decomp@example.test").await;
    grant(
        &pool,
        f.scope.organization_id,
        f.scope.workspace_id,
        user,
        &["process_executions:start"],
    )
    .await;
    // start alone: allowed; complete/cancel denied.
    let response = f.start_as(Some(&token), None).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let id = uuid_of(&body_json(response).await["data"]["id"]);
    assert_eq!(
        f.complete_as(id, Some(&token)).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.cancel_as(id, Some(&token), None).await.status(),
        StatusCode::FORBIDDEN
    );
    // Grant complete: complete works; cancel still denied.
    grant(
        &pool,
        f.scope.organization_id,
        f.scope.workspace_id,
        user,
        &["process_executions:complete"],
    )
    .await;
    assert_eq!(
        f.complete_as(id, Some(&token)).await.status(),
        StatusCode::OK
    );
    let id = f.started().await;
    assert_eq!(
        f.cancel_as(id, Some(&token), None).await.status(),
        StatusCode::FORBIDDEN
    );
    grant(
        &pool,
        f.scope.organization_id,
        f.scope.workspace_id,
        user,
        &["process_executions:cancel"],
    )
    .await;
    assert_eq!(
        f.cancel_as(id, Some(&token), None).await.status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn member_role_without_workspace_membership_cannot_execute(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let user = create_user(&pool, "org-only@example.test").await;
    sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(f.scope.organization_id)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assign_member_role(&pool, f.scope.organization_id, user).await;
    let token = login_token(f.app.clone(), "org-only@example.test").await;
    // Org-scoped grant never bypasses the workspace eligibility gate.
    let response = f.start_as(Some(&token), None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// TOCTOU: revocations committed before the write transaction are rejected
// ---------------------------------------------------------------------------

async fn revoked_state_start(pool: &PgPool, revoke: &str, forbidden: bool) {
    let f = Fixture::new(pool).await;
    exec(pool, revoke).await;
    let result = process_executions::start_execution(
        pool,
        f.scope,
        f.actor,
        &StartExecutionRequest::default(),
    )
    .await;
    if forbidden {
        assert!(
            matches!(result, Err(ExecutionError::Forbidden)),
            "{result:?}"
        );
    } else {
        assert!(
            matches!(result, Err(ExecutionError::NotAccessible)),
            "{result:?}"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM process_executions")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_org_membership(pool: PgPool) {
    revoked_state_start(
        &pool,
        "UPDATE organization_memberships SET status='deleted'",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_workspace_membership(pool: PgPool) {
    revoked_state_start(
        &pool,
        "UPDATE workspace_memberships SET deleted_at=now()",
        false,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_permission_grants(pool: PgPool) {
    revoked_state_start(
        &pool,
        "DELETE FROM role_permissions WHERE permission_id IN \
         (SELECT id FROM permissions WHERE key LIKE 'process_executions:%')",
        true,
    )
    .await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_role_assignments(pool: PgPool) {
    revoked_state_start(&pool, "DELETE FROM membership_roles", true).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_archived_process(pool: PgPool) {
    revoked_state_start(&pool, "UPDATE processes SET status='archived'", false).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_archived_ancestors(pool: PgPool) {
    for statement in [
        "UPDATE work_items SET status='archived'",
        "UPDATE sections SET status='archived'",
        "UPDATE projects SET status='archived'",
        "UPDATE workspaces SET deleted_at=now()",
        "UPDATE organizations SET deleted_at=now()",
    ] {
        let f = Fixture::new(&pool).await;
        exec(&pool, statement).await;
        let result = process_executions::start_execution(
            &pool,
            f.scope,
            f.actor,
            &StartExecutionRequest::default(),
        )
        .await;
        assert!(
            matches!(result, Err(ExecutionError::NotAccessible)),
            "{statement}: {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Concurrency: project-level serialization + structural backstops
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_starts_create_exactly_one_active_attempt(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let uri = executions_uri(&f.scope);
    let (a, b, c, d, e) = tokio::join!(
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
    );
    let statuses = [a, b, c, d, e].map(|r| r.status());
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1,
        "exactly one start wins: {statuses:?}"
    );
    for status in statuses {
        assert!(
            status == StatusCode::CREATED || status == StatusCode::CONFLICT,
            "{status}"
        );
    }
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT attempt_no, status FROM process_executions WHERE process_id = $1 ORDER BY attempt_no",
    )
    .bind(f.scope.process_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(rows, vec![(1, "active".to_owned())]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_terminal_transitions_resolve_once(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.started().await;
    let complete = execution_uri(&f.scope, id, "complete");
    let cancel = execution_uri(&f.scope, id, "cancel");
    let (a, b, c) = tokio::join!(
        request(f.app.clone(), "POST", &complete, Some(&f.token), None),
        request(f.app.clone(), "POST", &complete, Some(&f.token), None),
        request(
            f.app.clone(),
            "POST",
            &cancel,
            Some(&f.token),
            Some(json!({}))
        ),
    );
    let statuses = [a.status(), b.status(), c.status()];
    assert_eq!(statuses.iter().filter(|s| **s == StatusCode::OK).count(), 1);
    let status: String = sqlx::query_scalar("SELECT status FROM process_executions WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        status == "completed" || status == "cancelled",
        "one terminal outcome, never both: {status}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_retries_never_duplicate_attempt_numbers(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.started().await;
    assert_eq!(f.complete(id).await.status(), StatusCode::OK);
    let uri = executions_uri(&f.scope);
    let (a, b, c) = tokio::join!(
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
        request(f.app.clone(), "POST", &uri, Some(&f.token), Some(json!({}))),
    );
    let statuses = [a.status(), b.status(), c.status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1
    );
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT attempt_no, status FROM process_executions WHERE process_id = $1 ORDER BY attempt_no",
    )
    .bind(f.scope.process_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        rows,
        vec![(1, "completed".to_owned()), (2, "active".to_owned())],
        "retry appends exactly one next attempt"
    );
}

// ---------------------------------------------------------------------------
// Archive guards (ADR 0016): no active execution may be hidden by archiving
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn process_archive_is_blocked_while_active_and_unblocks_after_terminal(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let id = f.started().await;
    let response = request(
        f.app.clone(),
        "PATCH",
        &process_uri(&f.scope),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(response).await["error"]["code"], "STATE_CONFLICT");
    assert_eq!(f.complete(id).await.status(), StatusCode::OK);
    let response = request(
        f.app.clone(),
        "PATCH",
        &process_uri(&f.scope),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn work_item_archive_is_blocked_while_active(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    f.started().await;
    let uri = item_uri(
        f.scope.organization_id,
        f.scope.workspace_id,
        f.scope.project_id,
        f.scope.section_id,
        f.scope.work_item_id,
    );
    let response = request(
        f.app.clone(),
        "PATCH",
        &uri,
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn section_archive_covers_the_whole_subtree(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    // Kat1 → child(Daire) → grandchild(Mutfak) → work item → process → ACTIVE.
    // Two descendant levels prove the subtree walk is truly recursive.
    let child = post_id(
        &f.app,
        &f.token,
        &format!(
            "{}/projects/{}/sections",
            base(f.scope.organization_id, f.scope.workspace_id),
            f.scope.project_id
        ),
        json!({"name": "Daire", "parent_section_id": f.scope.section_id}),
        "/data/id",
    )
    .await;
    let grandchild = post_id(
        &f.app,
        &f.token,
        &format!(
            "{}/projects/{}/sections",
            base(f.scope.organization_id, f.scope.workspace_id),
            f.scope.project_id
        ),
        json!({"name": "Mutfak", "parent_section_id": child}),
        "/data/id",
    )
    .await;
    let item = post_id(
        &f.app,
        &f.token,
        &format!(
            "{}/projects/{}/sections/{}/work-items",
            base(f.scope.organization_id, f.scope.workspace_id),
            f.scope.project_id,
            grandchild
        ),
        json!({"name": "Tezgah"}),
        "/data/id",
    )
    .await;
    let process = post_id(
        &f.app,
        &f.token,
        &format!(
            "{}/processes",
            item_uri(
                f.scope.organization_id,
                f.scope.workspace_id,
                f.scope.project_id,
                grandchild,
                item
            )
        ),
        json!({"name": "Montaj"}),
        "/data/id",
    )
    .await;
    let scope = ExecutionScope {
        section_id: grandchild,
        work_item_id: item,
        process_id: process,
        ..f.scope
    };
    let response = request(
        f.app.clone(),
        "POST",
        &executions_uri(&scope),
        Some(&f.token),
        Some(json!({})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    // Archiving the ANCESTOR section must be blocked by the descendant's
    // active execution.
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!(
            "{}/projects/{}/sections/{}",
            base(f.scope.organization_id, f.scope.workspace_id),
            f.scope.project_id,
            f.scope.section_id
        ),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn project_archive_is_blocked_while_active(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    f.started().await;
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!(
            "{}/projects/{}",
            base(f.scope.organization_id, f.scope.workspace_id),
            f.scope.project_id
        ),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// START × archive races: exactly one outcome wins; never both.
#[sqlx::test(migrations = "../../migrations")]
async fn start_and_archive_races_serialize(pool: PgPool) {
    for target in ["process", "work_item", "section", "project"] {
        let f = Fixture::new(&pool).await;
        let archive_uri = match target {
            "process" => process_uri(&f.scope),
            "work_item" => item_uri(
                f.scope.organization_id,
                f.scope.workspace_id,
                f.scope.project_id,
                f.scope.section_id,
                f.scope.work_item_id,
            ),
            "section" => format!(
                "{}/projects/{}/sections/{}",
                base(f.scope.organization_id, f.scope.workspace_id),
                f.scope.project_id,
                f.scope.section_id
            ),
            _ => format!(
                "{}/projects/{}",
                base(f.scope.organization_id, f.scope.workspace_id),
                f.scope.project_id
            ),
        };
        let start_uri = executions_uri(&f.scope);
        let (started, archived) = tokio::join!(
            request(
                f.app.clone(),
                "POST",
                &start_uri,
                Some(&f.token),
                Some(json!({}))
            ),
            request(
                f.app.clone(),
                "PATCH",
                &archive_uri,
                Some(&f.token),
                Some(json!({"status": "archived"}))
            ),
        );
        let (s, a) = (started.status(), archived.status());
        let active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM process_executions WHERE process_id = $1 AND status = 'active'",
        )
        .bind(f.scope.process_id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
        // Exactly one legal outcome: start won (201 + archive 409) or archive
        // won (200 + start 404). The invariant: archived ancestor + active
        // execution must never coexist.
        let (table, target_id) = match target {
            "process" => ("processes", f.scope.process_id),
            "work_item" => ("work_items", f.scope.work_item_id),
            "section" => ("sections", f.scope.section_id),
            _ => ("projects", f.scope.project_id),
        };
        let archived_row: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE id = $1 AND status = 'archived'"
        ))
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(
            !(active == 1 && archived_row == 1),
            "{target}: start={s} archive={a} left archived+active"
        );
        assert!(
            (s == StatusCode::CREATED && a == StatusCode::CONFLICT)
                || (a == StatusCode::OK && s == StatusCode::NOT_FOUND),
            "{target}: start={s} archive={a}"
        );
    }
}

// ---------------------------------------------------------------------------
// Direct SQL structural invariants
// ---------------------------------------------------------------------------

async fn scope_ids(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) {
    let f = Fixture::new(pool).await;
    let s = f.scope;
    (
        s.organization_id,
        s.workspace_id,
        s.project_id,
        s.section_id,
        s.work_item_id,
        s.process_id,
        f.actor,
    )
}

fn insert_sql() -> &'static str {
    "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
}

#[sqlx::test(migrations = "../../migrations")]
async fn composite_fk_rejects_every_wrong_parent(pool: PgPool) {
    let (org, ws, project, section, item, process, actor) = scope_ids(&pool).await;
    for (label, o, w, p, s, i, pr) in [
        (
            "tenant",
            Uuid::now_v7(),
            ws,
            project,
            section,
            item,
            process,
        ),
        (
            "workspace",
            org,
            Uuid::now_v7(),
            project,
            section,
            item,
            process,
        ),
        ("project", org, ws, Uuid::now_v7(), section, item, process),
        ("section", org, ws, project, Uuid::now_v7(), item, process),
        (
            "work item",
            org,
            ws,
            project,
            section,
            Uuid::now_v7(),
            process,
        ),
        ("process", org, ws, project, section, item, Uuid::now_v7()),
    ] {
        let result = sqlx::query(insert_sql())
            .bind(Uuid::now_v7())
            .bind(o)
            .bind(w)
            .bind(p)
            .bind(s)
            .bind(i)
            .bind(pr)
            .bind(1)
            .bind("active")
            .bind(actor)
            .execute(&pool)
            .await;
        assert!(result.is_err(), "{label}: mismatched parent must fail FK");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn check_constraints_enforce_attempt_status_and_terminal_matrix(pool: PgPool) {
    let (org, ws, project, section, item, process, actor) = scope_ids(&pool).await;
    // attempt_no 0
    assert!(
        sqlx::query(insert_sql())
            .bind(Uuid::now_v7())
            .bind(org)
            .bind(ws)
            .bind(project)
            .bind(section)
            .bind(item)
            .bind(process)
            .bind(0)
            .bind("active")
            .bind(actor)
            .execute(&pool)
            .await
            .is_err(),
        "attempt_no 0 rejected"
    );
    // invalid status
    assert!(
        sqlx::query(insert_sql())
            .bind(Uuid::now_v7())
            .bind(org)
            .bind(ws)
            .bind(project)
            .bind(section)
            .bind(item)
            .bind(process)
            .bind(1)
            .bind("pending")
            .bind(actor)
            .execute(&pool)
            .await
            .is_err(),
        "unknown status rejected"
    );
    // active row carrying terminal columns
    assert!(
        sqlx::query(
            "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id, completed_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,1,'active',$8,now())"
        )
        .bind(Uuid::now_v7())
        .bind(org).bind(ws).bind(project).bind(section).bind(item).bind(process)
        .bind(actor)
        .execute(&pool).await.is_err(),
        "active + completed_at rejected"
    );
    // completed without completed_by
    assert!(
        sqlx::query(
            "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id, completed_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,1,'completed',$8,now())"
        )
        .bind(Uuid::now_v7())
        .bind(org).bind(ws).bind(project).bind(section).bind(item).bind(process)
        .bind(actor)
        .execute(&pool).await.is_err(),
        "completed without completed_by rejected"
    );
    // cancelled without cancelled_by
    assert!(
        sqlx::query(
            "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id, cancelled_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,1,'cancelled',$8,now())"
        )
        .bind(Uuid::now_v7())
        .bind(org).bind(ws).bind(project).bind(section).bind(item).bind(process)
        .bind(actor)
        .execute(&pool).await.is_err(),
        "cancelled without cancelled_by rejected"
    );
    // completed_at before started_at
    assert!(
        sqlx::query(
            "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id, started_at, completed_at, completed_by_user_id) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,1,'completed',$8,now(),now() - interval '1 hour',$8)"
        )
        .bind(Uuid::now_v7())
        .bind(org).bind(ws).bind(project).bind(section).bind(item).bind(process)
        .bind(actor)
        .execute(&pool).await.is_err(),
        "completed_at < started_at rejected"
    );
    // reason > 500 chars
    assert!(
        sqlx::query(
            "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id, start_reason) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,1,'active',$8,repeat('x',501))"
        )
        .bind(Uuid::now_v7())
        .bind(org).bind(ws).bind(project).bind(section).bind(item).bind(process)
        .bind(actor)
        .execute(&pool).await.is_err(),
        "reason > 500 rejected"
    );
    // blank reason rejected (service normalizes to NULL instead)
    assert!(
        sqlx::query(
            "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id, start_reason) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,1,'active',$8,'   ')"
        )
        .bind(Uuid::now_v7())
        .bind(org).bind(ws).bind(project).bind(section).bind(item).bind(process)
        .bind(actor)
        .execute(&pool).await.is_err(),
        "blank reason rejected"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn unique_active_and_attempt_number_are_structural(pool: PgPool) {
    let (org, ws, project, section, item, process, actor) = scope_ids(&pool).await;
    let insert = |attempt: i32, status: &'static str| {
        sqlx::query(insert_sql())
            .bind(Uuid::now_v7())
            .bind(org)
            .bind(ws)
            .bind(project)
            .bind(section)
            .bind(item)
            .bind(process)
            .bind(attempt)
            .bind(status)
            .bind(actor)
            .execute(&pool)
    };
    assert!(insert(1, "active").await.is_ok());
    // second active attempt — partial unique index
    assert!(
        insert(2, "active").await.is_err(),
        "two active rows rejected"
    );
    // duplicate attempt_no — unique constraint
    assert!(
        insert(1, "completed").await.is_err(),
        "duplicate attempt_no rejected"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn migration_011_rolls_back_and_reapplies(pool: PgPool) {
    // Dependent data blocks the down migration (no CASCADE).
    let f = Fixture::new(&pool).await;
    f.started().await;
    exec(
        &pool,
        "CREATE TABLE exec_probe (execution_id uuid REFERENCES process_executions (id))",
    )
    .await;
    assert!(
        platform_server::migrations::revert_last(&pool)
            .await
            .is_err()
    );
    exec(&pool, "DROP TABLE exec_probe").await;
    platform_server::migrations::revert_last(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'process_executions')",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(!exists);
    MIGRATOR
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    platform_server::migrations::verify(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
}

// ---------------------------------------------------------------------------
// Terminal immutability at the API surface
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn history_lists_all_attempts_under_the_work_item(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    // Second process under the same work item, each with attempts.
    let second_process = post_id(
        &f.app,
        &f.token,
        &format!(
            "{}/processes",
            item_uri(
                f.scope.organization_id,
                f.scope.workspace_id,
                f.scope.project_id,
                f.scope.section_id,
                f.scope.work_item_id
            )
        ),
        json!({"name": "Montaj"}),
        "/data/id",
    )
    .await;
    let first = f.started().await;
    assert_eq!(f.complete(first).await.status(), StatusCode::OK);
    f.started().await;
    let scope2 = ExecutionScope {
        process_id: second_process,
        ..f.scope
    };
    let response = request(
        f.app.clone(),
        "POST",
        &executions_uri(&scope2),
        Some(&f.token),
        Some(json!({})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let history = f.history().await;
    assert_eq!(history.len(), 3);
    // Deterministic order: grouped by process, ascending attempt number.
    let mut by_process: std::collections::BTreeMap<String, Vec<i64>> = Default::default();
    for row in &history {
        by_process
            .entry(row["process_id"].as_str().unwrap_or("").to_owned())
            .or_default()
            .push(row["attempt_no"].as_i64().unwrap_or(-1));
    }
    assert_eq!(by_process[&f.scope.process_id.to_string()], vec![1, 2]);
    assert_eq!(by_process[&second_process.to_string()], vec![1]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_validation_happens_before_state_conflict(pool: PgPool) {
    // Error order: 422 field validation precedes 409 state conflict.
    let f = Fixture::new(&pool).await;
    let id = f.started().await;
    assert_eq!(f.complete(id).await.status(), StatusCode::OK);
    let response = f
        .cancel_as(id, None, Some(json!({"cancel_reason": "x".repeat(501)})))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let response = f.cancel_as(id, None, Some(json!({}))).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_context_survives_future_deeper_routes(pool: PgPool) {
    // Named route extraction: a future route nested BELOW executions still
    // binds execution_id (and the whole parent chain) by name.
    let f = Fixture::new(&pool).await;
    let execution = f.started().await;
    async fn handler(
        context: platform_server::context::ProcessExecutionContext,
    ) -> axum::Json<Value> {
        axum::Json(json!({
            "id": context.execution.id,
            "process": context.parent.process.id,
            "work_item": context.parent.parent.work_item.id,
        }))
    }
    let app = axum::Router::new()
        .route(
            "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes/{process_id}/executions/{execution_id}/steps/{step_id}",
            axum::routing::get(handler),
        )
        .with_state(state(&pool));
    let base = executions_uri(&f.scope);
    let response = request(
        app.clone(),
        "GET",
        &format!("{base}/{execution}/steps/extra"),
        Some(&f.token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["id"], execution.to_string());
    assert_eq!(body["process"], f.scope.process_id.to_string());
    assert_eq!(body["work_item"], f.scope.work_item_id.to_string());
    // An unknown execution under the same deeper shape is a uniform 404.
    let response = request(
        app,
        "GET",
        &format!("{base}/{}/steps/extra", Uuid::now_v7()),
        Some(&f.token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
