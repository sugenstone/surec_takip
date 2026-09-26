//! STEP 21C (ADR 0018): assignment domain — current process responsibility,
//! immutable execution snapshots, eligibility, stale assignees, concurrency
//! and tenant isolation. Distinct from actor history: `assignee_user_id`
//! records responsibility; `started_by`/`completed_by`/`cancelled_by` record
//! who performed each transition.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState,
    config::AuthConfig,
    migrations::MIGRATOR,
    password::PasswordService,
    processes::{self, ProcessError, UpdateAssignmentRequest},
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
fn assignment_uri(s: &Scope) -> String {
    format!("{}/assignment", process_uri(s))
}
fn start_uri(s: &Scope) -> String {
    format!("{}/executions", process_uri(s))
}
fn members_uri(org: Uuid, ws: Uuid) -> String {
    format!("/api/v1/organizations/{org}/workspaces/{ws}/members")
}

struct Fixture {
    app: axum::Router,
    token: String,
    actor: Uuid,
    scope: Scope,
}
impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        let email = format!("assign-owner-{}@example.test", Uuid::now_v7());
        let actor = create_user(pool, &email).await;
        let app = router(state(pool));
        let token = login_token(app.clone(), &email).await;
        let org = post_id(
            &app,
            &token,
            "/api/v1/organizations",
            json!({"name": format!("Assign Org {}", Uuid::now_v7())}),
            "/data/organization/id",
        )
        .await;
        let ws = post_id(
            &app,
            &token,
            &format!("/api/v1/organizations/{org}/workspaces"),
            json!({"name": "Assign Ws"}),
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
            &format!("/api/v1/organizations/{org}/workspaces/{ws}/projects/{project}/sections/{section}/work-items"),
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
    /// Member holding the built-in member role (invitation-acceptance shape):
    /// process_executions:* only, never processes:assign.
    async fn builtin_member(&self, pool: &PgPool, email: &str) -> (Uuid, String) {
        let (user, token) = self.member(pool, email).await;
        assign_member_role(pool, self.scope.org, user).await;
        (user, token)
    }
    async fn assign(&self, token: &str, user_id: Option<Uuid>) -> axum::response::Response {
        request(
            self.app.clone(),
            "PUT",
            &assignment_uri(&self.scope),
            Some(token),
            Some(json!({"user_id": user_id})),
        )
        .await
    }
    async fn get_process(&self, token: &str) -> Value {
        let response = request(
            self.app.clone(),
            "GET",
            &process_uri(&self.scope),
            Some(token),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        body_json(response).await
    }
    async fn start(&self, token: &str) -> axum::response::Response {
        request(
            self.app.clone(),
            "POST",
            &start_uri(&self.scope),
            Some(token),
            Some(json!({})),
        )
        .await
    }
    async fn complete(&self, token: &str, execution: Uuid) -> axum::response::Response {
        request(
            self.app.clone(),
            "POST",
            &format!("{}/{execution}/complete", start_uri(&self.scope)),
            Some(token),
            Some(json!({})),
        )
        .await
    }
    async fn members(&self, token: &str) -> axum::response::Response {
        request(
            self.app.clone(),
            "GET",
            &members_uri(self.scope.org, self.scope.ws),
            Some(token),
            None,
        )
        .await
    }
    fn process_scope(&self) -> processes::ProcessScope {
        processes::ProcessScope {
            organization_id: self.scope.org,
            workspace_id: self.scope.ws,
            project_id: self.scope.project,
            section_id: self.scope.section,
            work_item_id: self.scope.work_item,
        }
    }
}

fn assignee(process: &Value) -> &Value {
    &process["assignee"]
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn owner_assigns_eligible_member_and_it_survives_reload(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let response = f.assign(&f.token, Some(harun)).await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(uuid_of(&body["data"]["assignee"]["id"]), harun);
    assert_eq!(body["data"]["assignee"]["eligible"], json!(true));
    // Reload paths: detail and list must carry the same assignee (no N+1,
    // embedded summary, no raw membership internals).
    let detail = f.get_process(&f.token).await;
    assert_eq!(uuid_of(&detail["assignee"]["id"]), harun);
    assert_eq!(
        detail["assignee"]["display_name"],
        json!("harun@example.test")
    );
    assert!(detail["assignee"].get("email").is_none());
    let list = request(
        f.app.clone(),
        "GET",
        &format!("{}/processes", base(&f.scope)),
        Some(&f.token),
        None,
    )
    .await;
    let list = body_json(list).await;
    assert_eq!(uuid_of(&list["data"][0]["assignee"]["id"]), harun);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unassign_and_reassign_round_trip(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let (cihan, _) = f.member(&pool, "cihan@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        f.assign(&f.token, Some(cihan)).await.status(),
        StatusCode::OK,
        "reassign Harun -> Cihan"
    );
    assert_eq!(
        uuid_of(&assignee(&f.get_process(&f.token).await)["id"]),
        cihan
    );
    let response = f.assign(&f.token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        assignee(&f.get_process(&f.token).await).is_null(),
        "unassigned"
    );
    // Idempotent unassign.
    assert_eq!(f.assign(&f.token, None).await.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Eligibility (in-transaction, generic 422 — no existence oracle)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn ineligible_assignees_are_rejected(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let eligible = f.assign(&f.token, Some(harun)).await;
    assert_eq!(eligible.status(), StatusCode::OK);
    // Foreign workspace member (different org entirely).
    let foreign_org_email = format!("foreign-owner-{}@example.test", Uuid::now_v7());
    let _foreign_actor = create_user(&pool, &foreign_org_email).await;
    let foreign_app = router(state(&pool));
    let foreign_token = login_token(foreign_app, &foreign_org_email).await;
    let foreign_org = post_id(
        &router(state(&pool)),
        &foreign_token,
        "/api/v1/organizations",
        json!({"name": format!("Foreign {}", Uuid::now_v7())}),
        "/data/organization/id",
    )
    .await;
    let foreign_ws = post_id(
        &router(state(&pool)),
        &foreign_token,
        &format!("/api/v1/organizations/{foreign_org}/workspaces"),
        json!({"name": "Foreign Ws"}),
        "/data/workspace/id",
    )
    .await;
    let foreign_member = create_user(&pool, "foreign-member@example.test").await;
    add_member(&pool, foreign_org, foreign_ws, foreign_member).await;
    let response = f.assign(&f.token, Some(foreign_member)).await;
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "foreign workspace member"
    );
    // Org member but NOT workspace member.
    let org_only = create_user(&pool, "org-only@example.test").await;
    sqlx::query(
        "INSERT INTO organization_memberships (id, tenant_id, user_id, status, joined_at) \
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(Uuid::now_v7())
    .bind(f.scope.org)
    .bind(org_only)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("{e}"));
    // Inactive workspace membership.
    let stale_ws = create_user(&pool, "stale-ws@example.test").await;
    add_member(&pool, f.scope.org, f.scope.ws, stale_ws).await;
    sqlx::query("UPDATE workspace_memberships SET status='deleted' WHERE user_id = $1")
        .bind(stale_ws)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    // Soft-deleted workspace membership.
    let deleted_ws = create_user(&pool, "deleted-ws@example.test").await;
    add_member(&pool, f.scope.org, f.scope.ws, deleted_ws).await;
    sqlx::query("UPDATE workspace_memberships SET deleted_at = now() WHERE user_id = $1")
        .bind(deleted_ws)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    // Inactive organization membership.
    let stale_om = create_user(&pool, "stale-om@example.test").await;
    add_member(&pool, f.scope.org, f.scope.ws, stale_om).await;
    sqlx::query("UPDATE organization_memberships SET status='deleted' WHERE user_id = $1")
        .bind(stale_om)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    // Disabled user.
    let disabled = create_user(&pool, "disabled@example.test").await;
    add_member(&pool, f.scope.org, f.scope.ws, disabled).await;
    sqlx::query("UPDATE users SET status='disabled' WHERE id = $1")
        .bind(disabled)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    // Every ineligible candidate is one generic 422 — no existence oracle.
    for (candidate, label) in [
        (org_only, "org member without ws membership"),
        (stale_ws, "inactive ws membership"),
        (deleted_ws, "deleted ws membership"),
        (stale_om, "inactive org membership"),
        (disabled, "disabled user"),
        (Uuid::now_v7(), "unknown user id"),
    ] {
        let response = f.assign(&f.token, Some(candidate)).await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{label}: {body}");
        assert_eq!(body["error"]["code"], json!("VALIDATION_ERROR"), "{label}");
        assert!(
            body["error"]["details"]["fields"]["user_id"].is_array(),
            "{label}: error must name user_id"
        );
    }
    // The still-eligible assignee is untouched by all failed attempts.
    assert_eq!(
        uuid_of(&assignee(&f.get_process(&f.token).await)["id"]),
        harun
    );
}

// ---------------------------------------------------------------------------
// Authorization / route safety
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn member_without_assign_permission_gets_403_but_sees_assignee(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    let (member, member_token) = f.builtin_member(&pool, "member@example.test").await;
    let forbidden = f.assign(&member_token, Some(member)).await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN, "no self-assign");
    let denied_other = f.assign(&member_token, Some(harun)).await;
    assert_eq!(denied_other.status(), StatusCode::FORBIDDEN);
    let detail = request(
        f.app.clone(),
        "GET",
        &process_uri(&f.scope),
        Some(&member_token),
        None,
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK, "members can read assignee");
    assert_eq!(uuid_of(&body_json(detail).await["assignee"]["id"]), harun);
    let unauthenticated = f.assign("not-a-session", Some(harun)).await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn assignment_body_requires_explicit_user_id(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    // A missing `user_id` key is a malformed request — it must NEVER fall
    // back to a silent unassign.
    let missing = request(
        f.app.clone(),
        "PUT",
        &assignment_uri(&f.scope),
        Some(&f.token),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        missing.status(),
        StatusCode::BAD_REQUEST,
        "missing user_id must be rejected, not unassign"
    );
    assert_eq!(
        uuid_of(&assignee(&f.get_process(&f.token).await)["id"]),
        harun,
        "stored assignee survives the malformed request"
    );
    // Unknown fields are still rejected outright.
    let unknown = request(
        f.app.clone(),
        "PUT",
        &assignment_uri(&f.scope),
        Some(&f.token),
        Some(json!({"user_id": harun, "assignee_user_id": harun})),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    // Non-uuid user_id values are a body rejection, not a panic/500.
    let wrong_type = request(
        f.app.clone(),
        "PUT",
        &assignment_uri(&f.scope),
        Some(&f.token),
        Some(json!({"user_id": "harun"})),
    )
    .await;
    assert_eq!(wrong_type.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_parent_chain_is_uniform_404(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    for uri in [
        // Foreign process id under a valid chain.
        format!("{}/processes/{}/assignment", base(&f.scope), Uuid::now_v7()),
        // Wrong work item / section / project / workspace.
        format!(
            "{}/processes/{}/assignment",
            base(&Scope {
                work_item: Uuid::now_v7(),
                ..f.scope
            }),
            f.scope.process
        ),
        format!(
            "{}/processes/{}/assignment",
            base(&Scope {
                section: Uuid::now_v7(),
                ..f.scope
            }),
            f.scope.process
        ),
        format!(
            "{}/processes/{}/assignment",
            base(&Scope {
                project: Uuid::now_v7(),
                ..f.scope
            }),
            f.scope.process
        ),
        format!(
            "{}/processes/{}/assignment",
            base(&Scope {
                ws: Uuid::now_v7(),
                ..f.scope
            }),
            f.scope.process
        ),
        format!(
            "{}/processes/{}/assignment",
            base(&Scope {
                org: Uuid::now_v7(),
                ..f.scope
            }),
            f.scope.process
        ),
    ] {
        let response = request(
            f.app.clone(),
            "PUT",
            &uri,
            Some(&f.token),
            Some(json!({"user_id": harun})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn archived_process_cannot_be_reassigned(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    let archive = request(
        f.app.clone(),
        "PATCH",
        &process_uri(&f.scope),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    let archive_status = archive.status();
    let archive_body = body_json(archive).await;
    assert_eq!(archive_status, StatusCode::OK, "{archive_body}");
    let attempt = f.assign(&f.token, Some(harun)).await;
    assert_eq!(attempt.status(), StatusCode::NOT_FOUND, "archived → 404");
    // The stored value remains for history.
    let stored: Option<Uuid> =
        sqlx::query_scalar("SELECT assignee_user_id FROM processes WHERE id = $1")
            .bind(f.scope.process)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(stored, Some(harun), "archive must not clear assignment");
}

// ---------------------------------------------------------------------------
// Execution snapshot (§41 core invariant)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn execution_snapshot_survives_reassignment(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let (cihan, _) = f.member(&pool, "cihan@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    // Attempt 1 snapshots Harun.
    let first = f.start(&f.token).await;
    let body = body_json(first).await;
    assert_eq!(body["data"]["assignee_user_id"], json!(harun.to_string()));
    let first_id = uuid_of(&body["data"]["id"]);
    // Reassign the process; attempt 1 keeps Harun.
    assert_eq!(
        f.assign(&f.token, Some(cihan)).await.status(),
        StatusCode::OK
    );
    let detail = f.get_process(&f.token).await;
    assert_eq!(uuid_of(&assignee(&detail)["id"]), cihan);
    let history = request(
        f.app.clone(),
        "GET",
        &format!("{}/executions", base(&f.scope)),
        Some(&f.token),
        None,
    )
    .await;
    let history = body_json(history).await;
    assert_eq!(
        history["data"][0]["assignee_user_id"],
        json!(harun.to_string()),
        "running attempt keeps its snapshot"
    );
    // Complete attempt 1, then attempt 2 snapshots Cihan.
    assert_eq!(
        f.complete(&f.token, first_id).await.status(),
        StatusCode::OK
    );
    let second = f.start(&f.token).await;
    let body = body_json(second).await;
    assert_eq!(body["data"]["attempt_no"], json!(2));
    assert_eq!(body["data"]["assignee_user_id"], json!(cihan.to_string()));
    // Attempt 1 still reports Harun.
    let history = request(
        f.app.clone(),
        "GET",
        &format!("{}/executions", base(&f.scope)),
        Some(&f.token),
        None,
    )
    .await;
    let history = body_json(history).await;
    let first_attempt = history["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .find(|e| e["attempt_no"] == json!(1))
        .unwrap_or_else(|| panic!("attempt 1 must exist"));
    assert_eq!(first_attempt["assignee_user_id"], json!(harun.to_string()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn actor_and_assignee_are_independent_facts(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.builtin_member(&pool, "harun@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    // The OWNER (not the assignee) presses Start — both facts must coexist.
    let started = f.start(&f.token).await;
    let body = body_json(started).await;
    assert_eq!(body["data"]["assignee_user_id"], json!(harun.to_string()));
    assert_eq!(
        body["data"]["started_by_user_id"],
        json!(f.actor.to_string()),
        "actor is the authenticated user, not the assignee"
    );
    assert_ne!(f.actor, harun);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unassigned_process_executes_with_null_snapshot(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let started = f.start(&f.token).await;
    assert_eq!(
        started.status(),
        StatusCode::CREATED,
        "unassigned executable"
    );
    let body = body_json(started).await;
    assert!(body["data"]["assignee_user_id"].is_null());
    assert_eq!(
        body["data"]["started_by_user_id"],
        json!(f.actor.to_string())
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_forge_the_execution_snapshot(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let forged = request(
        f.app.clone(),
        "POST",
        &start_uri(&f.scope),
        Some(&f.token),
        Some(json!({"assignee_user_id": harun})),
    )
    .await;
    assert_eq!(
        forged.status(),
        StatusCode::BAD_REQUEST,
        "deny_unknown_fields rejects client-supplied snapshots"
    );
    let patch = request(
        f.app.clone(),
        "PATCH",
        &process_uri(&f.scope),
        Some(&f.token),
        Some(json!({"assignee_user_id": harun})),
    )
    .await;
    assert!(
        matches!(
            patch.status(),
            StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST
        ),
        "process PATCH must not smuggle assignment"
    );
}

// ---------------------------------------------------------------------------
// Stale assignment (locked decision §8/§9)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn stale_assignee_remains_readable_and_marked_ineligible(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    // Revoke Harun's workspace membership (soft lifecycle, like revocation).
    sqlx::query("UPDATE workspace_memberships SET status='deleted' WHERE user_id = $1")
        .bind(harun)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let detail = f.get_process(&f.token).await;
    assert_eq!(
        uuid_of(&assignee(&detail)["id"]),
        harun,
        "assignment retained"
    );
    assert_eq!(assignee(&detail)["eligible"], json!(false), "stale flag");
    assert_eq!(
        assignee(&detail)["display_name"],
        json!("harun@example.test"),
        "name remains attributable"
    );
    // The picker no longer offers Harun.
    let members = f.members(&f.token).await;
    assert_eq!(members.status(), StatusCode::OK);
    let members = body_json(members).await;
    let ids: Vec<Uuid> = members["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|m| uuid_of(&m["id"]))
        .collect();
    assert!(!ids.contains(&harun), "picker excludes ineligible members");
    // Harun cannot be (re-)assigned while ineligible.
    let response = f.assign(&f.token, Some(harun)).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

/// `eligible` on the read path is one flag over three dimensions — each
/// dimension must flip it independently, not just workspace membership.
#[sqlx::test(migrations = "../../migrations")]
async fn stale_dimensions_each_flip_the_eligible_flag(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    // Workspace revocation (baseline covered elsewhere) vs organization
    // revocation vs user deactivation — all must produce eligible=false.
    for (email, revoke) in [
        (
            "stale-org@example.test",
            "UPDATE organization_memberships SET status='deleted' WHERE user_id = $1",
        ),
        (
            "stale-user@example.test",
            "UPDATE users SET status='disabled' WHERE id = $1",
        ),
    ] {
        let (user, _) = f.member(&pool, email).await;
        assert_eq!(
            f.assign(&f.token, Some(user)).await.status(),
            StatusCode::OK
        );
        sqlx::query(revoke)
            .bind(user)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let detail = f.get_process(&f.token).await;
        assert_eq!(uuid_of(&assignee(&detail)["id"]), user, "{email} retained");
        assert_eq!(assignee(&detail)["eligible"], json!(false), "{email} stale");
        let members = body_json(f.members(&f.token).await).await;
        let ids: Vec<Uuid> = members["data"]
            .as_array()
            .unwrap_or_else(|| panic!("array"))
            .iter()
            .map(|m| uuid_of(&m["id"]))
            .collect();
        assert!(!ids.contains(&user), "{email} excluded from picker");
        // Reset to a clean unassigned state for the next dimension.
        assert_eq!(f.assign(&f.token, None).await.status(), StatusCode::OK);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn execution_snapshot_survives_membership_revocation(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    assert_eq!(f.start(&f.token).await.status(), StatusCode::CREATED);
    sqlx::query("UPDATE workspace_memberships SET status='deleted' WHERE user_id = $1")
        .bind(harun)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let snapshot: Option<Uuid> =
        sqlx::query_scalar("SELECT assignee_user_id FROM process_executions WHERE process_id = $1")
            .bind(f.scope.process)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(snapshot, Some(harun), "snapshot survives revocation");
}

// ---------------------------------------------------------------------------
// Concurrency
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_reassignments_serialize(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let (cihan, _) = f.member(&pool, "cihan@example.test").await;
    let uri = assignment_uri(&f.scope);
    let (a, b) = tokio::join!(
        request(
            f.app.clone(),
            "PUT",
            &uri,
            Some(&f.token),
            Some(json!({"user_id": harun})),
        ),
        request(
            f.app.clone(),
            "PUT",
            &uri,
            Some(&f.token),
            Some(json!({"user_id": cihan})),
        ),
    );
    for response in [a, b] {
        assert_eq!(response.status(), StatusCode::OK);
    }
    let final_assignee = assignee(&f.get_process(&f.token).await).clone();
    let id = uuid_of(&final_assignee["id"]);
    assert!(
        id == harun || id == cihan,
        "serialized writes leave one complete value"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn assign_and_start_race_snapshots_one_coherent_value(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let (cihan, _) = f.member(&pool, "cihan@example.test").await;
    assert_eq!(
        f.assign(&f.token, Some(harun)).await.status(),
        StatusCode::OK
    );
    let assign_uri = assignment_uri(&f.scope);
    let exec_uri = start_uri(&f.scope);
    let (assign, start) = tokio::join!(
        request(
            f.app.clone(),
            "PUT",
            &assign_uri,
            Some(&f.token),
            Some(json!({"user_id": cihan})),
        ),
        request(
            f.app.clone(),
            "POST",
            &exec_uri,
            Some(&f.token),
            Some(json!({})),
        ),
    );
    assert_eq!(assign.status(), StatusCode::OK);
    assert_eq!(start.status(), StatusCode::CREATED);
    let snapshot = body_json(start).await["data"]["assignee_user_id"]
        .as_str()
        .map(str::to_owned);
    assert!(
        snapshot == Some(harun.to_string()) || snapshot == Some(cihan.to_string()),
        "snapshot is one complete serialized value, never mixed: {snapshot:?}"
    );
    let detail = f.get_process(&f.token).await;
    assert_eq!(
        uuid_of(&assignee(&detail)["id"]),
        cihan,
        "current assignment is the last serialized write"
    );
}

// ---------------------------------------------------------------------------
// TOCTOU — revoked state between pre-validation and the write transaction
// ---------------------------------------------------------------------------

/// Pre-validation state changes between request resolution and the write
/// transaction must be re-proven inside it (domain-level calls, like the
/// revoked_state suites in processes.rs / process_executions.rs).
#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_assign_permission(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let scope = f.process_scope();
    let input = UpdateAssignmentRequest {
        user_id: Some(harun),
    };
    exec(
        &pool,
        "DELETE FROM role_permissions WHERE permission_id IN \
         (SELECT id FROM permissions WHERE key = 'processes:assign')",
    )
    .await;
    let result = processes::assign_process(&pool, scope, f.scope.process, f.actor, &input).await;
    assert!(matches!(result, Err(ProcessError::Forbidden)), "{result:?}");
    assert!(
        assignee(&f.get_process(&f.token).await).is_null(),
        "no write"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_actor_membership(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let scope = f.process_scope();
    let input = UpdateAssignmentRequest {
        user_id: Some(harun),
    };
    sqlx::query(
        "UPDATE workspace_memberships SET deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(scope.organization_id)
    .bind(f.actor)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("{e}"));
    let result = processes::assign_process(&pool, scope, f.scope.process, f.actor, &input).await;
    assert!(
        matches!(
            result,
            Err(ProcessError::NotAccessible | ProcessError::Forbidden)
        ),
        "{result:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn tx_revalidates_assignee_eligibility(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let scope = f.process_scope();
    // The assignee's membership is revoked after the (hypothetical) picker
    // resolved — the write transaction must re-prove it.
    sqlx::query("UPDATE workspace_memberships SET status='deleted' WHERE user_id = $1")
        .bind(harun)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let result = processes::assign_process(
        &pool,
        scope,
        f.scope.process,
        f.actor,
        &UpdateAssignmentRequest {
            user_id: Some(harun),
        },
    )
    .await;
    assert!(
        matches!(result, Err(ProcessError::InvalidFields(_))),
        "{result:?}"
    );
}

// ---------------------------------------------------------------------------
// Members endpoint + DB invariants
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn members_endpoint_lists_only_eligible_workspace_members(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let (harun, _) = f.member(&pool, "harun@example.test").await;
    let (cihan, _) = f.member(&pool, "cihan@example.test").await;
    let ineligible = create_user(&pool, "inactive@example.test").await;
    add_member(&pool, f.scope.org, f.scope.ws, ineligible).await;
    sqlx::query("UPDATE workspace_memberships SET deleted_at=now() WHERE user_id = $1")
        .bind(ineligible)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let members = body_json(f.members(&f.token).await).await;
    let ids: Vec<Uuid> = members["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|m| uuid_of(&m["id"]))
        .collect();
    assert!(ids.contains(&harun) && ids.contains(&cihan) && ids.contains(&f.actor));
    assert!(!ids.contains(&ineligible));
    for member in members["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
    {
        assert!(member.get("email").is_none(), "no email exposure");
        assert!(member.get("role_id").is_none(), "no internals");
    }
    // A member without processes:assign can still read the directory.
    let (_, member_token) = f.builtin_member(&pool, "plain@example.test").await;
    let response = f.members(&member_token).await;
    assert_eq!(response.status(), StatusCode::OK, "plain members may list");
}

#[sqlx::test(migrations = "../../migrations")]
async fn members_endpoint_rejects_foreign_scopes(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let stranger_email = format!("stranger-{}@example.test", Uuid::now_v7());
    let _ = create_user(&pool, &stranger_email).await;
    let stranger = login_token(router(state(&pool)), &stranger_email).await;
    let response = request(
        router(state(&pool)),
        "GET",
        &members_uri(f.scope.org, f.scope.ws),
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "foreign workspace"
    );
    let response = request(
        router(state(&pool)),
        "GET",
        &members_uri(Uuid::now_v7(), f.scope.ws),
        Some(&f.token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "foreign org");
    let unauthenticated = request(
        router(state(&pool)),
        "GET",
        &members_uri(f.scope.org, f.scope.ws),
        None,
        None,
    )
    .await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_level_invariants(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    MIGRATOR.run(&pool).await.ok();
    // Roles are per-tenant and created at organization bootstrap — build a
    // fixture first so the Owner/Member grant assertions see real rows.
    let f = Fixture::new(&pool).await;
    // Permission seeded, Owner granted at organization scope, Member not.
    let owner_grants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp \
         JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE p.key = 'processes:assign' AND r.name = 'owner' AND rp.scope = 'organization'",
    )
    .fetch_one(&pool)
    .await?;
    assert!(owner_grants >= 1, "owner org-scope grant seeded");
    let member_grants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp \
         JOIN roles r ON r.id = rp.role_id \
         JOIN permissions p ON p.id = rp.permission_id \
         WHERE p.key = 'processes:assign' AND r.name = 'member'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(member_grants, 0, "member must not receive processes:assign");
    // Composite FK blocks foreign-workspace assignees even at the SQL level.
    let foreign_email = format!("fk-owner-{}@example.test", Uuid::now_v7());
    let _foreign_actor = create_user(&pool, &foreign_email).await;
    let foreign_app = router(state(&pool));
    let foreign_token = login_token(foreign_app, &foreign_email).await;
    let foreign_org = post_id(
        &router(state(&pool)),
        &foreign_token,
        "/api/v1/organizations",
        json!({"name": format!("FK Org {}", Uuid::now_v7())}),
        "/data/organization/id",
    )
    .await;
    let foreign_ws = post_id(
        &router(state(&pool)),
        &foreign_token,
        &format!("/api/v1/organizations/{foreign_org}/workspaces"),
        json!({"name": "FK Ws"}),
        "/data/workspace/id",
    )
    .await;
    let foreign_member = create_user(&pool, "fk-member@example.test").await;
    add_member(&pool, foreign_org, foreign_ws, foreign_member).await;
    let stored = sqlx::query("UPDATE processes SET assignee_user_id = $1 WHERE id = $2")
        .bind(foreign_member)
        .bind(f.scope.process)
        .execute(&pool)
        .await;
    assert!(stored.is_err(), "foreign workspace assignee is unstorable");
    // Execution snapshot references users only — survives membership removal.
    let (harun, _) = f.member(&pool, "snap@example.test").await;
    exec(
        &pool,
        "UPDATE workspace_memberships SET status='deleted' WHERE user_id IN (SELECT id FROM users WHERE email='snap@example.test')",
    )
    .await;
    sqlx::query(
        "INSERT INTO process_executions (id, tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no, status, started_by_user_id, assignee_user_id) \
         SELECT $1, tenant_id, workspace_id, project_id, section_id, work_item_id, id, 1, 'active', $2, $3 FROM processes WHERE id = $4",
    )
    .bind(Uuid::now_v7())
    .bind(f.actor)
    .bind(harun)
    .bind(f.scope.process)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("snapshot of a stale member must be storable: {e}"));
    Ok(())
}
