//! STEP 21D: time sessions — per-execution tracked labor intervals.
//! V1 is self-service: the authenticated actor is the worker. Worker and
//! actor identities still diverge on terminal transitions — completing or
//! cancelling an execution closes open sessions with `ended_by` = the
//! transition actor, not the worker.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState, config::AuthConfig, password::PasswordService, router, users::NewUser,
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
    let mut builder = Request::builder().uri(uri).method(method);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(token) = token {
        builder = builder.header(header::COOKIE, format!("platform_session={token}"));
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

/// Mirrors invitation acceptance: the tenant's built-in member role at
/// ORGANIZATION scope (workspace_id NULL).
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

#[derive(Clone, Copy)]
struct Scope {
    org: Uuid,
    ws: Uuid,
    project: Uuid,
    section: Uuid,
    work_item: Uuid,
    process: Uuid,
}

fn base(s: &Scope) -> String {
    format!(
        "/api/v1/organizations/{}/workspaces/{}/projects/{}/sections/{}/work-items/{}",
        s.org, s.ws, s.project, s.section, s.work_item
    )
}
fn process_uri(s: &Scope) -> String {
    format!("{}/processes/{}", base(s), s.process)
}
fn executions_uri(s: &Scope) -> String {
    format!("{}/executions", process_uri(s))
}
fn execution_uri(s: &Scope, id: Uuid, action: &str) -> String {
    format!("{}/{id}/{action}", executions_uri(s))
}
fn sessions_uri(s: &Scope, execution: Uuid) -> String {
    format!("{}/{execution}/time-sessions", executions_uri(s))
}
fn stop_uri(s: &Scope, execution: Uuid, session: Uuid) -> String {
    format!("{}/{session}/stop", sessions_uri(s, execution))
}
fn work_item_sessions_uri(s: &Scope) -> String {
    format!("{}/time-sessions", base(s))
}

struct Fixture {
    app: axum::Router,
    token: String,
    actor: Uuid,
    scope: Scope,
}
impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        let email = format!("sessions-owner-{}@example.test", Uuid::now_v7());
        let actor = create_user(pool, &email).await;
        let app = router(state(pool));
        let token = login_token(app.clone(), &email).await;
        let org = post_id(
            &app,
            &token,
            "/api/v1/organizations",
            json!({"name": format!("Sessions Org {}", Uuid::now_v7())}),
            "/data/organization/id",
        )
        .await;
        let ws = post_id(
            &app,
            &token,
            &format!("/api/v1/organizations/{org}/workspaces"),
            json!({"name": "Sessions Ws"}),
            "/data/workspace/id",
        )
        .await;
        let project = post_id(
            &app,
            &token,
            &format!("/api/v1/organizations/{org}/workspaces/{ws}/projects"),
            json!({"name": "Gardenia"}),
            "/data/id",
        )
        .await;
        let section = post_id(
            &app,
            &token,
            &format!("/api/v1/organizations/{org}/workspaces/{ws}/projects/{project}/sections"),
            json!({"name": "Daire 1"}),
            "/data/id",
        )
        .await;
        let work_item = post_id(
            &app,
            &token,
            &format!(
                "/api/v1/organizations/{org}/workspaces/{ws}/projects/{project}/sections/{section}/work-items"
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
                base(&Scope {
                    org,
                    ws,
                    project,
                    section,
                    work_item,
                    process: Uuid::nil()
                })
            ),
            json!({"name": "Montaj"}),
            "/data/id",
        )
        .await;
        Self {
            app,
            token,
            actor,
            scope: Scope {
                org,
                ws,
                project,
                section,
                work_item,
                process,
            },
        }
    }
    /// Eligible workspace member WITHOUT any role grants (no permissions).
    async fn member(&self, pool: &PgPool, email: &str) -> (Uuid, String) {
        let user = create_user(pool, email).await;
        add_member(pool, self.scope.org, self.scope.ws, user).await;
        (user, login_token(self.app.clone(), email).await)
    }
    /// Member holding the built-in member role: time_sessions:* included.
    async fn builtin_member(&self, pool: &PgPool, email: &str) -> (Uuid, String) {
        let (user, token) = self.member(pool, email).await;
        assign_member_role(pool, self.scope.org, user).await;
        (user, token)
    }
    /// Start an execution attempt; executions never auto-open sessions.
    async fn start_execution(&self) -> Uuid {
        let response = request(
            self.app.clone(),
            "POST",
            &executions_uri(&self.scope),
            Some(&self.token),
            Some(json!({})),
        )
        .await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        uuid_of(&body["data"]["id"])
    }
    /// Second process in the same scope, with its own active execution —
    /// for the tenant-wide open-session uniqueness test. Returns
    /// (process id, execution id).
    async fn second_execution(&self) -> (Uuid, Uuid) {
        let process = post_id(
            &self.app,
            &self.token,
            &format!("{}/processes", base(&self.scope)),
            json!({"name": "Diğer Süreç"}),
            "/data/id",
        )
        .await;
        let other = Scope {
            process,
            ..self.scope
        };
        let response = request(
            self.app.clone(),
            "POST",
            &executions_uri(&other),
            Some(&self.token),
            Some(json!({})),
        )
        .await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        (process, uuid_of(&body["data"]["id"]))
    }
    async fn start_session(
        &self,
        token: &str,
        execution: Uuid,
        body: Option<Value>,
    ) -> axum::response::Response {
        request(
            self.app.clone(),
            "POST",
            &sessions_uri(&self.scope, execution),
            Some(token),
            body.or(Some(json!({}))),
        )
        .await
    }
    async fn start_session_for(&self, token: &str, execution: Uuid) -> Value {
        let response = self.start_session(token, execution, None).await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"].clone()
    }
    async fn stop_session(
        &self,
        token: &str,
        execution: Uuid,
        session: Uuid,
    ) -> axum::response::Response {
        request(
            self.app.clone(),
            "POST",
            &stop_uri(&self.scope, execution, session),
            Some(token),
            None,
        )
        .await
    }
    async fn list_sessions(&self, token: &str, execution: Uuid) -> Value {
        let response = request(
            self.app.clone(),
            "GET",
            &sessions_uri(&self.scope, execution),
            Some(token),
            None,
        )
        .await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }
    async fn work_item_sessions(&self, token: &str) -> Value {
        let response = request(
            self.app.clone(),
            "GET",
            &work_item_sessions_uri(&self.scope),
            Some(token),
            None,
        )
        .await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }
    async fn transition(&self, execution: Uuid, action: &str, body: Option<Value>) -> StatusCode {
        let response = request(
            self.app.clone(),
            "POST",
            &execution_uri(&self.scope, execution, action),
            Some(&self.token),
            body,
        )
        .await;
        response.status()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn start_requires_active_execution_and_permission(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    // Unknown execution id → uniform 404, same as any bad parent segment.
    let response = f.start_session(&f.token, Uuid::now_v7(), None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let execution = f.start_execution().await;
    // Member without role grants is forbidden even in their own workspace.
    let (_, member_token) = f.member(&pool, "sessions-member@example.test").await;
    let response = f.start_session(&member_token, execution, None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // A member holding the built-in member role can track time.
    let (builtin_user, builtin_token) = f
        .builtin_member(&pool, "sessions-builtin@example.test")
        .await;
    let session = f.start_session_for(&builtin_token, execution).await;
    assert_eq!(session["worker"]["id"], builtin_user.to_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn start_on_terminal_execution_is_a_state_conflict(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    assert_eq!(
        f.transition(execution, "complete", None).await,
        StatusCode::OK
    );
    let response = f.start_session(&f.token, execution, None).await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "STATE_CONFLICT");
}

#[sqlx::test(migrations = "../../migrations")]
async fn start_creates_session_and_lists_history(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    let session = f.start_session_for(&f.token, execution).await;
    assert_eq!(session["worker"]["id"], f.actor.to_string());
    assert!(
        !session["worker"]["display_name"]
            .as_str()
            .unwrap_or("")
            .is_empty()
    );
    assert_eq!(session["started_by_user_id"], f.actor.to_string());
    assert_eq!(session["process_execution_id"], execution.to_string());
    assert!(session["ended_at"].is_null());
    assert!(session["ended_by_user_id"].is_null());
    assert!(!session["started_at"].is_null());
    let sessions = f.list_sessions(&f.token, execution).await;
    assert_eq!(sessions.as_array().map(|list| list.len()), Some(1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_open_session_per_worker_across_the_tenant(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    f.start_session_for(&f.token, execution).await;
    // Same execution again → conflict.
    let response = f.start_session(&f.token, execution, None).await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "ACTIVE_SESSION_EXISTS");
    // A different execution in the same tenant is also blocked: the
    // worker can only be "currently working" in one place.
    let (other_process, other_execution) = f.second_execution().await;
    let other_scope = Scope {
        process: other_process,
        ..f.scope
    };
    let response = request(
        f.app.clone(),
        "POST",
        &sessions_uri(&other_scope, other_execution),
        Some(&f.token),
        Some(json!({})),
    )
    .await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "ACTIVE_SESSION_EXISTS");
}

#[sqlx::test(migrations = "../../migrations")]
async fn multiple_workers_can_hold_open_sessions_on_one_execution(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (member, member_token) = f
        .builtin_member(&pool, "sessions-coworker@example.test")
        .await;
    let execution = f.start_execution().await;
    let owner_session = f.start_session_for(&f.token, execution).await;
    let member_session = f.start_session_for(&member_token, execution).await;
    assert_eq!(member_session["worker"]["id"], member.to_string());
    // The work-item open-sessions read shows both live workers.
    let open = f.work_item_sessions(&f.token).await;
    let rows = open.as_array().unwrap_or_else(|| panic!("list"));
    assert_eq!(rows.len(), 2);
    let workers: Vec<_> = rows
        .iter()
        .map(|row| uuid_of(&row["worker"]["id"]))
        .collect();
    assert!(workers.contains(&f.actor));
    assert!(workers.contains(&member));
    let _ = owner_session;
}

#[sqlx::test(migrations = "../../migrations")]
async fn stop_closes_session_and_records_ended_by(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    let session = f.start_session_for(&f.token, execution).await;
    let session_id = uuid_of(&session["id"]);
    let response = f.stop_session(&f.token, execution, session_id).await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["worker"]["id"], f.actor.to_string());
    assert_eq!(body["data"]["ended_by_user_id"], f.actor.to_string());
    assert!(!body["data"]["ended_at"].is_null());
    // The closing write is single-shot: a repeated stop is a conflict,
    // never a re-write of the closed interval.
    let response = f.stop_session(&f.token, execution, session_id).await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "STATE_CONFLICT");
    // An unknown session id is a uniform 404.
    let response = f.stop_session(&f.token, execution, Uuid::now_v7()).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_worker_can_stop_their_own_session(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (_, member_token) = f
        .builtin_member(&pool, "sessions-stopper@example.test")
        .await;
    let execution = f.start_execution().await;
    let session = f.start_session_for(&f.token, execution).await;
    let session_id = uuid_of(&session["id"]);
    // The member holds time_sessions:stop but it is not their session.
    let response = f.stop_session(&member_token, execution, session_id).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // The worker themselves can.
    let response = f.stop_session(&f.token, execution, session_id).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn pause_resume_is_close_plus_new_row(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    let first = f.start_session_for(&f.token, execution).await;
    f.stop_session(&f.token, execution, uuid_of(&first["id"]))
        .await;
    let second = f.start_session_for(&f.token, execution).await;
    f.stop_session(&f.token, execution, uuid_of(&second["id"]))
        .await;
    let sessions = f.list_sessions(&f.token, execution).await;
    let rows = sessions.as_array().unwrap_or_else(|| panic!("list"));
    assert_eq!(rows.len(), 2, "pause/resume records two intervals");
    assert!(rows.iter().all(|row| !row["ended_at"].is_null()));
    assert!(
        rows.iter()
            .all(|row| row["worker"]["id"] == f.actor.to_string())
    );
    // History is ordered oldest first.
    assert!(rows[0]["started_at"].as_str() <= rows[1]["started_at"].as_str());
}

#[sqlx::test(migrations = "../../migrations")]
async fn execution_cancel_auto_closes_open_sessions(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (member, member_token) = f
        .builtin_member(&pool, "sessions-worker@example.test")
        .await;
    let execution = f.start_execution().await;
    f.start_session_for(&member_token, execution).await;
    // The OWNER cancels while the member's session is open: actor truth
    // (ended_by = owner) and labor truth (worker = member) stay distinct.
    assert_eq!(
        f.transition(
            execution,
            "cancel",
            Some(json!({"cancel_reason": "plan değişti"}))
        )
        .await,
        StatusCode::OK
    );
    let sessions = f.list_sessions(&f.token, execution).await;
    let rows = sessions.as_array().unwrap_or_else(|| panic!("list"));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["worker"]["id"], member.to_string());
    assert_eq!(rows[0]["ended_by_user_id"], f.actor.to_string());
    assert!(!rows[0]["ended_at"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn execution_complete_auto_closes_open_sessions(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    f.start_session_for(&f.token, execution).await;
    assert_eq!(
        f.transition(execution, "complete", None).await,
        StatusCode::OK
    );
    let sessions = f.list_sessions(&f.token, execution).await;
    let rows = sessions.as_array().unwrap_or_else(|| panic!("list"));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["ended_by_user_id"], f.actor.to_string());
    assert!(!rows[0]["ended_at"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn sessions_are_scoped_to_each_execution_attempt(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let first = f.start_execution().await;
    f.start_session_for(&f.token, first).await;
    f.transition(first, "cancel", Some(json!({"cancel_reason": "retry"})))
        .await;
    let second = f.start_execution().await;
    f.start_session_for(&f.token, second).await;
    let old_sessions = f.list_sessions(&f.token, first).await;
    let new_sessions = f.list_sessions(&f.token, second).await;
    let old = old_sessions.as_array().unwrap_or_else(|| panic!("list"));
    let new = new_sessions.as_array().unwrap_or_else(|| panic!("list"));
    assert_eq!(old.len(), 1);
    assert_eq!(new.len(), 1);
    assert_eq!(old[0]["process_execution_id"], first.to_string());
    assert!(!old[0]["ended_at"].is_null());
    assert_eq!(new[0]["process_execution_id"], second.to_string());
    assert!(new[0]["ended_at"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn reassignment_does_not_touch_open_session(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (member, _) = f
        .builtin_member(&pool, "sessions-assignee@example.test")
        .await;
    let execution = f.start_execution().await;
    f.start_session_for(&f.token, execution).await;
    // Reassign the process while a session is open.
    let response = request(
        f.app.clone(),
        "PUT",
        &format!("{}/assignment", process_uri(&f.scope)),
        Some(&f.token),
        Some(json!({"user_id": member})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let sessions = f.list_sessions(&f.token, execution).await;
    let rows = sessions.as_array().unwrap_or_else(|| panic!("list"));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["worker"]["id"], f.actor.to_string());
    assert!(rows[0]["ended_at"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn cross_tenant_process_sessions_are_not_found(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    let session = f.start_session_for(&f.token, execution).await;
    let session_id = uuid_of(&session["id"]);
    let other = Fixture::new(&pool).await;
    // The other tenant's token cannot see or mutate any of it.
    let response = f.start_session(&other.token, execution, None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = request(
        f.app.clone(),
        "GET",
        &sessions_uri(&f.scope, execution),
        Some(&other.token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = f.stop_session(&other.token, execution, session_id).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = request(
        f.app.clone(),
        "GET",
        &work_item_sessions_uri(&f.scope),
        Some(&other.token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reads_need_membership_only_and_malformed_body_is_400(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    f.start_session_for(&f.token, execution).await;
    // Session reads require only workspace membership (no grants), like
    // every other read path in this codebase.
    let (_, member_token) = f.member(&pool, "sessions-reader@example.test").await;
    let sessions = f.list_sessions(&member_token, execution).await;
    assert_eq!(sessions.as_array().map(|list| list.len()), Some(1));
    // A body with foreign fields is rejected — worker identity can never
    // be spoofed through the self-service endpoint.
    let response = f
        .start_session(
            &f.token,
            execution,
            Some(json!({"worker_user_id": Uuid::now_v7()})),
        )
        .await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sessions_table_enforces_single_open_session_per_worker(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let execution = f.start_execution().await;
    f.start_session_for(&f.token, execution).await;
    // Bypassing the API with a second open row for the same worker must
    // hit the partial unique index, not silently create a duplicate.
    let duplicate = sqlx::query(
        "INSERT INTO process_execution_time_sessions \
         (id, tenant_id, workspace_id, project_id, section_id, work_item_id, \
          process_id, process_execution_id, worker_user_id, started_at, started_by_user_id) \
         SELECT $1, tenant_id, workspace_id, project_id, section_id, work_item_id, \
          process_id, process_execution_id, worker_user_id, now(), started_by_user_id \
         FROM process_execution_time_sessions WHERE process_execution_id = $2",
    )
    .bind(Uuid::now_v7())
    .bind(execution)
    .execute(&pool)
    .await;
    assert!(
        duplicate.is_err(),
        "one open session per worker must hold at the DB level"
    );
}
