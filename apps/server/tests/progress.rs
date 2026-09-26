//! STEP 21B — Progress Engine integration suite (ADR 0017).
//!
//! Progress is derived, never stored: every assertion below hits the real
//! HTTP surface and checks the SAME response shape — `{ completed, active,
//! total, percent }` — embedded in detail AND list payloads.
//!
//! `progress()` also enforces the global invariants on every read, so every
//! test is simultaneously a snapshot-consistency probe:
//!   0 <= completed <= total, 0 <= active <= total,
//!   total = 0 => percent NULL, else percent = round-half-up(100*c/t).

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState, config::AuthConfig, migrations::MIGRATOR, password::PasswordService, router,
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

fn base(org: Uuid, ws: Uuid) -> String {
    format!("/api/v1/organizations/{org}/workspaces/{ws}")
}

// ---------------------------------------------------------------------------
// Progress assertions — every read validates the global invariants.
// ---------------------------------------------------------------------------

/// The canonical expected value: round-half-up integer percent.
fn expected_percent(completed: i64, total: i64) -> Option<i64> {
    (total > 0).then(|| (100 * completed + total / 2) / total)
}

/// Asserts the progress object shape, every invariant, AND the expected
/// counts — returning it so callers can also assert percent explicitly.
fn assert_progress(value: &Value, completed: i64, active: i64, total: i64) -> &Value {
    let progress = &value["progress"];
    assert_eq!(
        progress["completed"].as_i64().unwrap_or(-1),
        completed,
        "completed: {progress}"
    );
    assert_eq!(
        progress["active"].as_i64().unwrap_or(-1),
        active,
        "active: {progress}"
    );
    assert_eq!(
        progress["total"].as_i64().unwrap_or(-1),
        total,
        "total: {progress}"
    );
    // Global invariants hold on EVERY snapshot, old or new.
    let (c, a, t) = (completed, active, total);
    assert!(0 <= c && c <= t && 0 <= a && a <= t);
    match expected_percent(c, t) {
        None => assert!(
            progress["percent"].is_null(),
            "total=0 must produce percent=null, got {progress}"
        ),
        Some(percent) => assert_eq!(
            progress["percent"].as_i64().unwrap_or(-1),
            percent,
            "percent: {progress}"
        ),
    }
    progress
}

/// Invariant-only check for reads whose exact counts are not the subject.
fn assert_progress_consistent(progress: &Value) {
    let c = progress["completed"].as_i64().unwrap_or(-1);
    let a = progress["active"].as_i64().unwrap_or(-1);
    let t = progress["total"].as_i64().unwrap_or(-1);
    assert!(0 <= c && c <= t && 0 <= a && a <= t, "{progress}");
    if t == 0 {
        assert!(progress["percent"].is_null(), "{progress}");
    } else {
        assert_eq!(
            progress["percent"].as_i64().unwrap_or(-1),
            expected_percent(c, t).unwrap_or(-1),
            "{progress}"
        );
    }
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    app: axum::Router,
    token: String,
    org: Uuid,
    ws: Uuid,
    project: Uuid,
}

impl Fixture {
    async fn new(pool: &PgPool) -> Self {
        let email = format!("progress-owner-{}@example.test", Uuid::now_v7());
        create_user(pool, &email).await;
        let app = router(state(pool));
        let token = login_token(app.clone(), &email).await;
        let org = post_id(
            &app,
            &token,
            "/api/v1/organizations",
            json!({"name": format!("Progress Org {}", Uuid::now_v7())}),
            "/data/organization/id",
        )
        .await;
        let ws = post_id(
            &app,
            &token,
            &format!("/api/v1/organizations/{org}/workspaces"),
            json!({"name": "Progress Ws"}),
            "/data/workspace/id",
        )
        .await;
        let project = post_id(
            &app,
            &token,
            &format!("{}/projects", base(org, ws)),
            json!({"name": "Gardenia"}),
            "/data/id",
        )
        .await;
        Self {
            app,
            token,
            org,
            ws,
            project,
        }
    }

    fn project_uri(&self) -> String {
        format!("{}/projects/{}", base(self.org, self.ws), self.project)
    }

    async fn section(&self, name: &str, parent: Option<Uuid>) -> Uuid {
        let mut body = json!({"name": name});
        if let Some(parent) = parent {
            body["parent_section_id"] = json!(parent);
        }
        post_id(
            &self.app,
            &self.token,
            &format!("{}/sections", self.project_uri()),
            body,
            "/data/id",
        )
        .await
    }

    async fn work_item(&self, section: Uuid, name: &str) -> Uuid {
        post_id(
            &self.app,
            &self.token,
            &format!("{}/sections/{section}/work-items", self.project_uri()),
            json!({"name": name}),
            "/data/id",
        )
        .await
    }

    async fn process(&self, section: Uuid, item: Uuid, name: &str) -> Uuid {
        post_id(
            &self.app,
            &self.token,
            &format!(
                "{}/sections/{section}/work-items/{item}/processes",
                self.project_uri()
            ),
            json!({"name": name}),
            "/data/id",
        )
        .await
    }

    fn executions_uri(&self, section: Uuid, item: Uuid, process: Uuid) -> String {
        format!(
            "{}/sections/{section}/work-items/{item}/processes/{process}/executions",
            self.project_uri()
        )
    }

    async fn start(&self, section: Uuid, item: Uuid, process: Uuid) -> Uuid {
        let response = request(
            self.app.clone(),
            "POST",
            &self.executions_uri(section, item, process),
            Some(&self.token),
            Some(json!({})),
        )
        .await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        uuid_of(&body["data"]["id"])
    }

    async fn transition(&self, section: Uuid, item: Uuid, process: Uuid, id: Uuid, action: &str) {
        let response = request(
            self.app.clone(),
            "POST",
            &format!(
                "{}/{id}/{action}",
                self.executions_uri(section, item, process)
            ),
            Some(&self.token),
            Some(json!({})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "{action} must succeed");
    }

    /// Complete the next attempt started by `run` in one call.
    async fn complete_process(&self, section: Uuid, item: Uuid, process: Uuid) {
        let id = self.start(section, item, process).await;
        self.transition(section, item, process, id, "complete")
            .await;
    }

    // ---- progress readers -------------------------------------------------

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let response = request(self.app.clone(), "GET", uri, Some(&self.token), None).await;
        (response.status(), body_json(response).await)
    }

    async fn item_progress(&self, section: Uuid, item: Uuid) -> Value {
        let (status, body) = self
            .get(&format!(
                "{}/sections/{section}/work-items/{item}",
                self.project_uri()
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["progress"].clone()
    }

    async fn items(&self, section: Uuid) -> Vec<Value> {
        let (status, body) = self
            .get(&format!(
                "{}/sections/{section}/work-items",
                self.project_uri()
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"]
            .as_array()
            .unwrap_or_else(|| panic!("{body}"))
            .clone()
    }

    async fn section_progress(&self, section: Uuid) -> Value {
        let (status, body) = self
            .get(&format!("{}/sections/{section}", self.project_uri()))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["progress"].clone()
    }

    async fn sections(&self) -> Vec<Value> {
        let (status, body) = self.get(&format!("{}/sections", self.project_uri())).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"]
            .as_array()
            .unwrap_or_else(|| panic!("{body}"))
            .clone()
    }

    async fn project_progress(&self) -> Value {
        let (status, body) = self.get(&self.project_uri()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["progress"].clone()
    }

    async fn projects(&self) -> Vec<Value> {
        let (status, body) = self
            .get(&format!("{}/projects", base(self.org, self.ws)))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"]
            .as_array()
            .unwrap_or_else(|| panic!("{body}"))
            .clone()
    }
}

// ---------------------------------------------------------------------------
// Work item
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn work_item_progress_tracks_definition_states(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Daire 1", None).await;
    let item = f.work_item(section, "Tezgah").await;

    // Zero counted definitions → NULL, not a fake 0% or 100%.
    let progress = f.item_progress(section, item).await;
    assert_progress(&json!({ "progress": progress }), 0, 0, 0);

    let process = f.process(section, item, "Kesim").await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        0,
        0,
        1,
    );

    // In-flight attempt: not done, but counted as active.
    let id = f.start(section, item, process).await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        0,
        1,
        1,
    );

    f.transition(section, item, process, id, "complete").await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        1,
        0,
        1,
    );

    // RETRY on a completed definition → active rework REGRESSES it (ADR 0016).
    let retry = f.start(section, item, process).await;
    let progress = f.item_progress(section, item).await;
    assert_progress(&json!({ "progress": progress }), 0, 1, 1);
    assert_eq!(progress["percent"], 0);

    // Cancelling the rework restores the earlier completed attempt.
    f.transition(section, item, process, retry, "cancel").await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        1,
        0,
        1,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn active_retry_regresses_every_ancestor_level(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let root = f.section("Blok", None).await;
    let floor = f.section("Kat", Some(root)).await;
    let item = f.work_item(floor, "Tezgah").await;
    let mut processes = Vec::new();
    for index in 0..5 {
        processes.push(f.process(floor, item, &format!("P{index}")).await);
    }
    for process in &processes[..4] {
        f.complete_process(floor, item, *process).await;
    }
    for uri in [
        format!("{}/sections/{floor}/work-items/{item}", f.project_uri()),
        format!("{}/sections/{floor}", f.project_uri()),
        format!("{}/sections/{root}", f.project_uri()),
        f.project_uri(),
    ] {
        let (_, body) = f.get(&uri).await;
        assert_progress(&body, 4, 0, 5);
    }

    // RETRY on a completed definition: the regression must propagate to the
    // work item, its section, the grandparent section AND the project —
    // 4/5 (80%) becomes 3/1/5 (60%) everywhere, never only on the item.
    f.start(floor, item, processes[0]).await;
    for uri in [
        format!("{}/sections/{floor}/work-items/{item}", f.project_uri()),
        format!("{}/sections/{floor}", f.project_uri()),
        format!("{}/sections/{root}", f.project_uri()),
        f.project_uri(),
    ] {
        let (_, body) = f.get(&uri).await;
        assert_progress(&body, 3, 1, 5);
        assert_eq!(body["progress"]["percent"], 60, "{uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn repeated_attempt_history_never_fans_out_the_denominator(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let item = f.work_item(section, "Panel").await;
    let process = f.process(section, item, "Montaj").await;

    // cancel, cancel, complete, cancel, complete — five attempt rows, still
    // ONE definition unit: total=1, completed=1 (never 5 or 2).
    for _ in 0..2 {
        let id = f.start(section, item, process).await;
        f.transition(section, item, process, id, "cancel").await;
    }
    f.complete_process(section, item, process).await;
    let id = f.start(section, item, process).await;
    f.transition(section, item, process, id, "cancel").await;
    f.complete_process(section, item, process).await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        1,
        0,
        1,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 1, 0, 1);

    // Active retry over a multi-completed history: ONE counted process with
    // an active attempt — active=1 (processes, not attempts), completed=0.
    f.start(section, item, process).await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        0,
        1,
        1,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 0, 1, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_attempts_and_history_never_change_the_denominator(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Zemin", None).await;
    let item = f.work_item(section, "Panel").await;
    let process = f.process(section, item, "Montaj").await;

    // Attempt 1 cancelled, attempt 2 cancelled: still one unit, not done.
    for _ in 0..2 {
        let id = f.start(section, item, process).await;
        f.transition(section, item, process, id, "cancel").await;
    }
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        0,
        0,
        1,
    );

    // Attempt 3 completes: ONE process unit done despite three attempts.
    f.complete_process(section, item, process).await;
    let progress = f.item_progress(section, item).await;
    assert_progress(&json!({ "progress": progress }), 1, 0, 1);
    assert_eq!(progress["percent"], 100);
}

#[sqlx::test(migrations = "../../migrations")]
async fn archived_process_exits_numerator_and_denominator(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat 1", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let a = f.process(section, item, "Taş alımı").await;
    let b = f.process(section, item, "Kesim").await;
    let c = f.process(section, item, "Montaj").await;
    f.complete_process(section, item, a).await;
    f.complete_process(section, item, b).await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        2,
        0,
        3,
    );

    // Archiving the incomplete definition re-scopes the work: 2/3 → 2/2.
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!(
            "{}/sections/{section}/work-items/{item}/processes/{c}",
            f.project_uri()
        ),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let progress = f.item_progress(section, item).await;
    assert_progress(&json!({ "progress": progress }), 2, 0, 2);
    assert_eq!(progress["percent"], 100);
}

#[sqlx::test(migrations = "../../migrations")]
async fn archived_work_item_exits_section_and_project_aggregates(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let keep = f.work_item(section, "Kalan").await;
    let hide = f.work_item(section, "Arşivlenecek").await;
    let a = f.process(section, keep, "A").await;
    let b = f.process(section, hide, "B").await;
    f.process(section, hide, "C").await;
    f.complete_process(section, keep, a).await;
    f.complete_process(section, hide, b).await;
    assert_progress(
        &json!({ "progress": f.section_progress(section).await }),
        2,
        0,
        3,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 2, 0, 3);

    // Archiving the item removes BOTH its processes (including the completed
    // one's execution history) from every ancestor aggregate — while the
    // sibling item stays untouched.
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!("{}/sections/{section}/work-items/{hide}", f.project_uri()),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_progress(
        &json!({ "progress": f.section_progress(section).await }),
        1,
        0,
        1,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 1, 0, 1);
    // Archived items are also invisible in the section's item list.
    let items = f.items(section).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], keep.to_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn archived_completed_process_exits_numerator_and_denominator(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let a = f.process(section, item, "A").await;
    let b = f.process(section, item, "B").await;
    f.complete_process(section, item, a).await;
    f.complete_process(section, item, b).await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        2,
        0,
        2,
    );

    // Archiving a DONE definition shrinks numerator AND denominator — the
    // project can even reach percent=null once nothing counted remains.
    for process in [b, a] {
        let response = request(
            f.app.clone(),
            "PATCH",
            &format!(
                "{}/sections/{section}/work-items/{item}/processes/{process}",
                f.project_uri()
            ),
            Some(&f.token),
            Some(json!({"status": "archived"})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let progress = f.item_progress(section, item).await;
    assert_progress(&json!({ "progress": progress }), 0, 0, 0);
    assert!(progress["percent"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn soft_deleted_process_exits_the_denominator(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat 2", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let a = f.process(section, item, "A").await;
    let b = f.process(section, item, "B").await;
    f.complete_process(section, item, a).await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        1,
        0,
        2,
    );
    exec(
        &pool,
        &format!("UPDATE processes SET deleted_at = now() WHERE id = '{b}'"),
    )
    .await;
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        1,
        0,
        1,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn rounding_is_backend_authoritative_and_locked(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat 3", None).await;
    let six = f.work_item(section, "Altı süreç").await;
    let mut processes = Vec::new();
    for index in 0..6 {
        processes.push(f.process(section, six, &format!("P{index}")).await);
    }
    // 1/6 → 17
    f.complete_process(section, six, processes[0]).await;
    let progress = f.item_progress(section, six).await;
    assert_progress(&json!({ "progress": progress }), 1, 0, 6);
    assert_eq!(progress["percent"], 17);
    // 5/6 → 83
    for process in &processes[1..5] {
        f.complete_process(section, six, *process).await;
    }
    let progress = f.item_progress(section, six).await;
    assert_progress(&json!({ "progress": progress }), 5, 0, 6);
    assert_eq!(progress["percent"], 83);

    // 1/3 → 33, then 2/3 → 67 on a second item.
    let three = f.work_item(section, "Üç süreç").await;
    let mut trio = Vec::new();
    for index in 0..3 {
        trio.push(f.process(section, three, &format!("T{index}")).await);
    }
    f.complete_process(section, three, trio[0]).await;
    assert_eq!(f.item_progress(section, three).await["percent"], 33);
    f.complete_process(section, three, trio[1]).await;
    assert_eq!(f.item_progress(section, three).await["percent"], 67);
}

#[sqlx::test(migrations = "../../migrations")]
async fn manual_status_is_independent_of_derived_progress(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat 4", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let a = f.process(section, item, "A").await;
    f.process(section, item, "B").await;
    f.complete_process(section, item, a).await;
    // Work item manually marked completed while progress is 50% — legal.
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!("{}/sections/{section}/work-items/{item}", f.project_uri()),
        Some(&f.token),
        Some(json!({"status": "completed"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let (status, body) = f
        .get(&format!(
            "{}/sections/{section}/work-items/{item}",
            f.project_uri()
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "completed");
    assert_progress(&body, 1, 0, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_gives_each_work_item_its_own_aggregate(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat 5", None).await;
    let one = f.work_item(section, "Bir").await;
    let two = f.work_item(section, "İki").await;
    let empty = f.work_item(section, "Boş").await;
    let a = f.process(section, one, "A").await;
    let b = f.process(section, two, "B").await;
    f.process(section, two, "C").await;
    f.complete_process(section, one, a).await;
    f.start(section, two, b).await;

    let items = f.items(section).await;
    fn by_id(items: &[Value], id: Uuid) -> &Value {
        items
            .iter()
            .find(|row| row["id"] == id.to_string())
            .unwrap_or_else(|| panic!("{id} must be listed"))
    }
    assert_progress(by_id(&items, one), 1, 0, 1);
    assert_progress(by_id(&items, two), 0, 1, 2);
    assert_progress(by_id(&items, empty), 0, 0, 0);
    for item in &items {
        assert_progress_consistent(&item["progress"]);
    }
}

// ---------------------------------------------------------------------------
// Section
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn section_progress_is_leaf_weighted_not_child_averaged(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let root = f.section("Kat 1", None).await;
    let direct = f.work_item(root, "Direkt").await;
    let a = f.process(root, direct, "A").await;
    f.complete_process(root, direct, a).await;
    let child = f.section("Daire 1", Some(root)).await;
    let deep = f.work_item(child, "Derin").await;
    for index in 0..9 {
        f.process(child, deep, &format!("P{index}")).await;
    }
    // Child-average would report 50%; process weighting reports 1/10 = 10%.
    let progress = f.section_progress(root).await;
    assert_progress(&json!({ "progress": progress }), 1, 0, 10);
    assert_eq!(progress["percent"], 10);
    assert_progress(
        &json!({ "progress": f.section_progress(child).await }),
        0,
        0,
        9,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn deep_subtree_counts_each_process_exactly_once(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let root = f.section("Blok", None).await;
    let child = f.section("Kat", Some(root)).await;
    let grandchild = f.section("Daire", Some(child)).await;
    let item = f.work_item(grandchild, "Tezgah").await;
    let a = f.process(grandchild, item, "A").await;
    f.complete_process(grandchild, item, a).await;
    // One deep process counts ONCE at every ancestor level.
    assert_progress(
        &json!({ "progress": f.section_progress(root).await }),
        1,
        0,
        1,
    );
    assert_progress(
        &json!({ "progress": f.section_progress(child).await }),
        1,
        0,
        1,
    );
    assert_progress(
        &json!({ "progress": f.section_progress(grandchild).await }),
        1,
        0,
        1,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 1, 0, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn section_with_no_counted_work_is_null(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let root = f.section("Boş Blok", None).await;
    let child = f.section("Boş Kat", Some(root)).await;
    f.work_item(child, "İşsiz").await;
    let progress = f.section_progress(root).await;
    assert_progress(&json!({ "progress": progress }), 0, 0, 0);
    assert!(progress["percent"].is_null());
    assert_progress(&json!({ "progress": f.project_progress().await }), 0, 0, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn section_list_carries_each_childs_own_subtree(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let east = f.section("A Blok", None).await;
    let west = f.section("B Blok", None).await;
    let floor = f.section("Kat", Some(east)).await;
    let item_a = f.work_item(floor, "Tezgah").await;
    let item_b = f.work_item(west, "Panel").await;
    let pa = f.process(floor, item_a, "A").await;
    f.process(west, item_b, "B").await;
    f.complete_process(floor, item_a, pa).await;

    let sections = f.sections().await;
    let by_id = |id: Uuid| {
        sections
            .iter()
            .find(|row| row["id"] == id.to_string())
            .unwrap_or_else(|| panic!("{id} must be listed"))
    };
    // Each section's progress is ITS OWN subtree: A Blok includes Kat's work.
    assert_progress(by_id(east), 1, 0, 1);
    assert_progress(by_id(floor), 1, 0, 1);
    assert_progress(by_id(west), 0, 0, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn archived_section_hides_own_items_but_active_children_still_count(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let parent = f.section("Eski Blok", None).await;
    let own_item = f.work_item(parent, "Eski Tezgah").await;
    let own = f.process(parent, own_item, "A").await;
    let child = f.section("Yeni Daire", Some(parent)).await;
    let child_item = f.work_item(child, "Yeni Panel").await;
    let child_process = f.process(child, child_item, "B").await;
    f.complete_process(parent, own_item, own).await;
    assert_progress(
        &json!({ "progress": f.section_progress(parent).await }),
        1,
        0,
        2,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 1, 0, 2);

    // Archiving the parent hides its OWN work items (route-unreachable) but
    // the still-active child section keeps its own work counted.
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!("{}/sections/{parent}", f.project_uri()),
        Some(&f.token),
        Some(json!({"status": "archived"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_progress(
        &json!({ "progress": f.section_progress(parent).await }),
        0,
        0,
        1,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 0, 0, 1);
    f.complete_process(child, child_item, child_process).await;
    assert_progress(
        &json!({ "progress": f.section_progress(parent).await }),
        1,
        0,
        1,
    );
    assert_progress(&json!({ "progress": f.project_progress().await }), 1, 0, 1);
}

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn project_progress_and_list_use_the_same_leaf_weighting(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let east = f.section("A Blok", None).await;
    let west = f.section("B Blok", None).await;
    let item_e = f.work_item(east, "Tezgah").await;
    let item_w = f.work_item(west, "Panel").await;
    let pe = f.process(east, item_e, "A").await;
    for index in 0..3 {
        f.process(west, item_w, &format!("P{index}")).await;
    }
    f.complete_process(east, item_e, pe).await;
    let progress = f.project_progress().await;
    assert_progress(&json!({ "progress": progress }), 1, 0, 4);
    assert_eq!(progress["percent"], 25);

    let projects = f.projects().await;
    let row = projects
        .iter()
        .find(|row| row["id"] == f.project.to_string())
        .unwrap_or_else(|| panic!("project must be listed"));
    assert_progress(row, 1, 0, 4);
}

#[sqlx::test(migrations = "../../migrations")]
async fn structural_invariance_wrapper_and_reparent_preserve_project_progress(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let d1 = f.section("Daire 1", None).await;
    let d2 = f.section("Daire 2", None).await;
    let item1 = f.work_item(d1, "Tezgah").await;
    let item2 = f.work_item(d2, "Panel").await;
    let p1 = f.process(d1, item1, "A").await;
    f.process(d2, item2, "B").await;
    f.complete_process(d1, item1, p1).await;
    let before = f.project_progress().await;
    assert_eq!(before["percent"], 50);

    // Organizational wrapper: same counted set, same project progress.
    let blok = f.section("A Blok", None).await;
    for child in [d1, d2] {
        let response = request(
            f.app.clone(),
            "PATCH",
            &format!("{}/sections/{child}", f.project_uri()),
            Some(&f.token),
            Some(json!({"parent_section_id": blok})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let after_wrap = f.project_progress().await;
    assert_eq!(
        after_wrap, before,
        "wrapper must not change project progress"
    );
    // The wrapper's own subtree equals the whole project.
    assert_progress(
        &json!({ "progress": f.section_progress(blok).await }),
        1,
        0,
        2,
    );

    // Reparenting one subtree under the other changes the SHAPE, not the set.
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!("{}/sections/{d2}", f.project_uri()),
        Some(&f.token),
        Some(json!({"parent_section_id": d1})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        f.project_progress().await,
        before,
        "re-parenting must not change project progress"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn adding_processes_moves_progress_in_the_expected_direction(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let a = f.process(section, item, "A").await;
    f.complete_process(section, item, a).await;
    assert_eq!(f.item_progress(section, item).await["percent"], 100);

    // An incomplete counted definition cannot RAISE progress: 100 → 50.
    f.process(section, item, "B").await;
    assert_eq!(f.item_progress(section, item).await["percent"], 50);

    // A completed definition cannot LOWER it: 50 → 67.
    let c = f.process(section, item, "C").await;
    f.complete_process(section, item, c).await;
    let progress = f.item_progress(section, item).await;
    assert_progress(&json!({ "progress": progress }), 2, 0, 3);
    assert_eq!(progress["percent"], 67);
}

// ---------------------------------------------------------------------------
// Tenant / parent-chain isolation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn foreign_scopes_never_mix_into_aggregates(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let own = f.process(section, item, "Kendi").await;
    f.complete_process(section, item, own).await;

    // A SECOND organization+workspace+project with heavier finished work.
    let other = Fixture::new(&pool).await;
    let o_section = other.section("Yabancı", None).await;
    let o_item = other.work_item(o_section, "Yabancı iş").await;
    for index in 0..5 {
        let p = other.process(o_section, o_item, &format!("Y{index}")).await;
        other.complete_process(o_section, o_item, p).await;
    }
    assert_progress(
        &json!({ "progress": other.project_progress().await }),
        5,
        0,
        5,
    );

    // Our aggregates are untouched by foreign history.
    assert_progress(&json!({ "progress": f.project_progress().await }), 1, 0, 1);
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        1,
        0,
        1,
    );

    // Foreign ids resolve as uniform 404s — never "0%" (no existence leak).
    let foreign_paths = [
        format!("{}/projects/{}", base(f.org, f.ws), other.project),
        format!("{}/sections/{}", f.project_uri(), o_section),
        format!(
            "{}/sections/{o_section}/work-items/{o_item}",
            f.project_uri()
        ),
    ];
    for uri in foreign_paths {
        let (status, _) = f.get(&uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn member_reads_identical_progress_without_admin_rights(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let process = f.process(section, item, "Kesim").await;
    f.complete_process(section, item, process).await;

    let member_email = format!("progress-member-{}@example.test", Uuid::now_v7());
    let member = create_user(&pool, &member_email).await;
    add_member(&pool, f.org, f.ws, member).await;
    assign_member_role(&pool, f.org, member).await;
    let token = login_token(f.app.clone(), &member_email).await;

    for uri in [
        f.project_uri(),
        format!("{}/sections/{section}", f.project_uri()),
        format!("{}/sections/{section}/work-items/{item}", f.project_uri()),
    ] {
        let response = request(f.app.clone(), "GET", &uri, Some(&token), None).await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert_progress(&body, 1, 0, 1);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_reads_never_produce_mixed_snapshots(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let a = f.process(section, item, "A").await;
    let b = f.process(section, item, "B").await;
    f.complete_process(section, item, a).await;

    // COMPLETE racing parallel reads: every returned snapshot must satisfy
    // the invariants — old or new state, never a mixed one.
    let app = f.app.clone();
    let token = f.token.clone();
    let uri = format!("{}/sections/{section}/work-items/{item}", f.project_uri());
    let transition = async {
        let id = f.start(section, item, b).await;
        f.transition(section, item, b, id, "complete").await;
    };
    let reads = async {
        for _ in 0..8 {
            let mut builder = Request::builder().uri(&uri).method("GET");
            builder = builder.header(header::COOKIE, format!("platform_session={token}"));
            let response = app
                .clone()
                .oneshot(
                    builder
                        .body(Body::empty())
                        .unwrap_or_else(|_| panic!("req")),
                )
                .await
                .unwrap_or_else(|_| panic!("read"));
            assert_eq!(response.status(), StatusCode::OK);
            assert_progress_consistent(&body_json(response).await["progress"]);
        }
    };
    tokio::join!(transition, reads);
    // Final deterministic state.
    assert_progress(
        &json!({ "progress": f.item_progress(section, item).await }),
        2,
        0,
        2,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn progress_is_derived_and_never_client_writable(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let section = f.section("Kat", None).await;
    let item = f.work_item(section, "Tezgah").await;
    let process = f.process(section, item, "Kesim").await;
    f.complete_process(section, item, process).await;

    // Work-item writes REJECT unknown fields: an injected progress object is
    // a malformed body, not a silent override.
    let response = request(
        f.app.clone(),
        "PATCH",
        &format!("{}/sections/{section}/work-items/{item}", f.project_uri()),
        Some(&f.token),
        Some(json!({"name": "Hile", "progress": {"percent": 0, "total": 99}})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Project/section writes follow the established ignore-unknown-fields
    // policy; either way the derived value is unaffected.
    let response = request(
        f.app.clone(),
        "PATCH",
        &f.project_uri(),
        Some(&f.token),
        Some(json!({"progress": {"completed": 0, "active": 0, "total": 99, "percent": 0}})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_progress(&json!({ "progress": f.project_progress().await }), 1, 0, 1);
    assert_progress(
        &json!({ "progress": f.section_progress(section).await }),
        1,
        0,
        1,
    );
}

// ---------------------------------------------------------------------------
// Migration 012
// ---------------------------------------------------------------------------

async fn index_012_exists(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'process_executions_completed_idx')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"))
}

#[sqlx::test(migrations = "../../migrations")]
async fn migration_012_rolls_back_and_reapplies(pool: PgPool) {
    assert!(
        index_012_exists(&pool).await,
        "012 must create the partial index"
    );
    let predicate: String = sqlx::query_scalar(
        "SELECT pg_get_expr(i.indpred, i.indrelid) FROM pg_index i \
         JOIN pg_class c ON c.oid = i.indexrelid WHERE c.relname = 'process_executions_completed_idx'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        predicate.contains("status") && predicate.contains("completed"),
        "partial predicate must cover status='completed': {predicate}"
    );

    platform_server::migrations::revert_last(&pool)
        .await
        .unwrap_or_else(|error| panic!("013 down must apply: {error}"));
    MIGRATOR
        .undo(&pool, 20260926100000)
        .await
        .unwrap_or_else(|error| panic!("012 down must apply: {error}"));
    assert!(!index_012_exists(&pool).await);
    assert!(
        platform_server::migrations::verify(&pool).await.is_err(),
        "drift must be detected while 012 is unapplied"
    );
    MIGRATOR
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    platform_server::migrations::verify(&pool)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        index_012_exists(&pool).await,
        "reapply must recreate the index"
    );
    // Nothing but an index: no progress storage ever appears.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE column_name LIKE '%progress%' OR column_name LIKE '%percent%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(stored, 0, "no persisted progress columns may exist");
}
