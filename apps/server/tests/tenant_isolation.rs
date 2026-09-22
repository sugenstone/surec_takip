//! Cross-tenant security milestone (First Agent Mission steps 11-12).
//!
//! Adversarial matrix against the real PostgreSQL-backed API. The fixture is
//! one coherent world; every test documents the ATTACK, the EXPECTED result
//! and WHY it must fail. Classification follows ADR 0006/0007:
//!
//! - IMPOSSIBLE BY SCHEMA: composite FK rejects the row outright.
//! - POSSIBLE ROW BUT ACCESS DENIED BY QUERY: honest-tenant rows and stale
//!   memberships exist yet every read path joins organization membership.
//! - POSSIBLE AND VALID ACCESS: the legitimate membership chains.

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState, config::AuthConfig, password::PasswordService, router, users::NewUser,
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

async fn login(router: axum::Router, email: &str) -> String {
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
    session: Option<&str>,
    context_cookies: &[(&str, &str)],
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(uri).method(method);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let mut cookies: Vec<String> = Vec::new();
    if let Some(session) = session {
        cookies.push(format!("platform_session={session}"));
    }
    for (name, value) in context_cookies {
        cookies.push(format!("{name}={value}"));
    }
    if !cookies.is_empty() {
        builder = builder.header(header::COOKIE, cookies.join("; "));
    }
    let body = Body::from(
        body.map(|body| body.to_string())
            .unwrap_or_else(|| "{}".to_owned()),
    );
    let response = router
        .oneshot(
            builder
                .body(body)
                .unwrap_or_else(|_| panic!("request must build")),
        )
        .await
        .unwrap_or_else(|_| panic!("request must respond"));
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .unwrap_or_else(|_| panic!("body must be readable"));
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("body must be JSON"));
    (status, body)
}

async fn create_org(router: axum::Router, session: &str, name: &str) -> Uuid {
    let (status, body) = request(
        router,
        "POST",
        "/api/v1/organizations",
        Some(session),
        &[],
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["organization"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("organization id must parse"))
}

async fn create_workspace(router: axum::Router, session: &str, org: Uuid, name: &str) -> Uuid {
    let (status, body) = request(
        router,
        "POST",
        &format!("/api/v1/organizations/{org}/workspaces"),
        Some(session),
        &[],
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("workspace id must parse"))
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

async fn add_ws_membership(pool: &PgPool, tenant: Uuid, ws: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(tenant)
    .bind(ws)
    .bind(user)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("ws membership must be insertable"));
}

async fn bare_workspace(pool: &PgPool, tenant: Uuid, name: &str, slug: &str) -> Uuid {
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

fn org_uri(org: &str) -> String {
    format!("/api/v1/organizations/{org}")
}

fn ws_uri(org: &str, ws: &str) -> String {
    format!("/api/v1/organizations/{org}/workspaces/{ws}")
}

/// The adversarial world:
///
/// - U1: ORG_A member (A1), ORG_SHARED member (S1) — the A-side insider.
/// - U2: ORG_B member (B1), ORG_SHARED member (S1) — the B-side insider.
/// - U3: ORG_A org member with NO workspace memberships anywhere, plus
///   honest-tenant workspace membership rows in S1 (ORG_SHARED) and B2
///   (ORG_B) while holding no organization membership there.
/// - U4: member of ORG_A + ORG_B + ORG_SHARED (A1, B1, S1) — the
///   dual-tenant attacker used for parent-child confusion.
/// - A2/B2: workspaces without any membership (org-member-but-not-ws-member).
struct World {
    app: axum::Router,
    u1: Uuid,
    u3: Uuid,
    t1: String,
    t2: String,
    t3: String,
    t4: String,
    org_a: Uuid,
    org_b: Uuid,
    org_shared: Uuid,
    a1: Uuid,
    a2: Uuid,
    b1: Uuid,
    b2: Uuid,
    s1: Uuid,
}

async fn world(pool: &PgPool) -> World {
    let app = router(state(pool));
    let u1 = create_user(pool, "u1@iso.test").await;
    let u2 = create_user(pool, "u2@iso.test").await;
    let u3 = create_user(pool, "u3@iso.test").await;
    create_user(pool, "u4@iso.test").await;
    let t1 = login(app.clone(), "u1@iso.test").await;
    let t2 = login(app.clone(), "u2@iso.test").await;
    let t3 = login(app.clone(), "u3@iso.test").await;
    let t4 = login(app.clone(), "u4@iso.test").await;

    let org_a = create_org(app.clone(), &t1, "Iso Org A").await;
    let org_b = create_org(app.clone(), &t2, "Iso Org B").await;
    let org_shared = create_org(app.clone(), &t1, "Iso Shared").await;
    add_org_membership(pool, org_shared, u2).await;

    let a1 = create_workspace(app.clone(), &t1, org_a, "Iso A1").await;
    let b1 = create_workspace(app.clone(), &t2, org_b, "Iso B1").await;
    let s1 = create_workspace(app.clone(), &t1, org_shared, "Iso S1").await;
    add_ws_membership(pool, org_shared, s1, u2).await;
    let a2 = bare_workspace(pool, org_a, "Iso A2", "iso-a2").await;
    let b2 = bare_workspace(pool, org_b, "Iso B2", "iso-b2").await;

    // U3: organization member of A only, plus honest-tenant rows elsewhere.
    add_org_membership(pool, org_a, u3).await;
    add_ws_membership(pool, org_shared, s1, u3).await;
    add_ws_membership(pool, org_b, b2, u3).await;

    World {
        app,
        u1,
        u3,
        t1,
        t2,
        t3,
        t4,
        org_a,
        org_b,
        org_shared,
        a1,
        a2,
        b1,
        b2,
        s1,
    }
}

async fn uuid_of(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| panic!("user must exist"))
}

// ---------------------------------------------------------------------------
// §5 Organization read isolation grid
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn organization_grid_matches_active_memberships_exactly(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;

    // ATTACK: each user probes every organization by direct GET.
    // EXPECT: 200 only for (U1,A),(U1,SHARED),(U2,B),(U2,SHARED),(U3,A).
    // WHY: server-resolved membership; U3's honest rows grant nothing.
    let grid = [
        (&w.t1, w.org_a, StatusCode::OK),
        (&w.t1, w.org_shared, StatusCode::OK),
        (&w.t1, w.org_b, StatusCode::NOT_FOUND),
        (&w.t2, w.org_b, StatusCode::OK),
        (&w.t2, w.org_shared, StatusCode::OK),
        (&w.t2, w.org_a, StatusCode::NOT_FOUND),
        (&w.t3, w.org_a, StatusCode::OK),
        (&w.t3, w.org_shared, StatusCode::NOT_FOUND),
        (&w.t3, w.org_b, StatusCode::NOT_FOUND),
    ];
    for (token, org, expected) in grid {
        let (status, body) = request(
            w.app.clone(),
            "GET",
            &org_uri(&org.to_string()),
            Some(token),
            &[],
            None,
        )
        .await;
        assert_eq!(status, expected, "org {org} for this user");
        if expected == StatusCode::NOT_FOUND {
            assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
        }
    }

    // Lists and /auth/me expose exactly the same membership sets.
    let (_, list1) = request(
        w.app.clone(),
        "GET",
        "/api/v1/organizations",
        Some(&w.t1),
        &[],
        None,
    )
    .await;
    let slugs1: Vec<&str> = list1["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|o| o["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(slugs1, ["iso-org-a", "iso-shared"]);
    let (_, me3) = request(
        w.app.clone(),
        "GET",
        "/api/v1/auth/me",
        Some(&w.t3),
        &[],
        None,
    )
    .await;
    let me3_orgs: Vec<&str> = me3["data"]["organizations"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|o| o["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        me3_orgs,
        ["iso-org-a"],
        "honest rows must not leak into /auth/me"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// §6 Workspace read isolation grid
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_grid_enforces_both_memberships_and_parent(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;

    // ATTACK: probe (org, workspace) pairs across tenants and memberships.
    // EXPECT: allowed only with BOTH memberships and a matching parent.
    // WHY: ACCESS_FILTER joins organization membership; parent is authoritative.
    let grid = [
        // user, org, workspace, expected
        (&w.t1, w.org_a, w.a1, StatusCode::OK), // valid chain
        (&w.t1, w.org_a, w.a2, StatusCode::NOT_FOUND), // org member, ws non-member
        (&w.t1, w.org_shared, w.s1, StatusCode::OK), // shared, both members
        (&w.t1, w.org_b, w.b1, StatusCode::NOT_FOUND), // foreign tenant
        (&w.t1, w.org_a, w.b1, StatusCode::NOT_FOUND), // wrong parent
        (&w.t1, w.org_b, w.s1, StatusCode::NOT_FOUND), // wrong parent (shared ws via B)
        (&w.t2, w.org_b, w.b1, StatusCode::OK),
        (&w.t2, w.org_shared, w.s1, StatusCode::OK),
        (&w.t2, w.org_b, w.b2, StatusCode::NOT_FOUND), // org member, ws non-member
        (&w.t2, w.org_a, w.a1, StatusCode::NOT_FOUND), // foreign tenant
        (&w.t3, w.org_a, w.a1, StatusCode::NOT_FOUND), // org member, NO ws membership
        (&w.t3, w.org_shared, w.s1, StatusCode::NOT_FOUND), // honest row, no org membership
        (&w.t3, w.org_b, w.b2, StatusCode::NOT_FOUND), // honest row, no org membership
    ];
    for (token, org, ws, expected) in grid {
        let (status, body) = request(
            w.app.clone(),
            "GET",
            &ws_uri(&org.to_string(), &ws.to_string()),
            Some(token),
            &[],
            None,
        )
        .await;
        assert_eq!(status, expected, "org {org} ws {ws}");
        if expected == StatusCode::NOT_FOUND {
            assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
        }
    }

    // Lists leak nothing: U1 sees only A1 under ORG_A; ORG_B listing for U1
    // is not-found (no org membership), and U3 sees nothing under ORG_A.
    let (_, l1a) = request(
        w.app.clone(),
        "GET",
        &format!("/api/v1/organizations/{}/workspaces", w.org_a),
        Some(&w.t1),
        &[],
        None,
    )
    .await;
    let slugs: Vec<&str> = l1a["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .map(|x| x["slug"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(slugs, ["iso-a1"], "A2 must stay invisible");
    let (status, _) = request(
        w.app.clone(),
        "GET",
        &format!("/api/v1/organizations/{}/workspaces", w.org_b),
        Some(&w.t1),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, l3a) = request(
        w.app.clone(),
        "GET",
        &format!("/api/v1/organizations/{}/workspaces", w.org_a),
        Some(&w.t3),
        &[],
        None,
    )
    .await;
    assert_eq!(l3a["data"].as_array().map(Vec::len), Some(0));
    Ok(())
}

// ---------------------------------------------------------------------------
// §7 Parent-child confusion (attacker holds memberships in every org)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn parent_child_confusion_denied_even_for_multi_tenant_member(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;
    let u4 = uuid_of(&pool, "u4@iso.test").await;
    add_org_membership(&pool, w.org_a, u4).await;
    add_org_membership(&pool, w.org_b, u4).await;
    add_org_membership(&pool, w.org_shared, u4).await;
    add_ws_membership(&pool, w.org_a, w.a1, u4).await;
    add_ws_membership(&pool, w.org_b, w.b1, u4).await;
    add_ws_membership(&pool, w.org_shared, w.s1, u4).await;

    // ATTACK: mismatched (route org, workspace) pairs while the attacker is a
    // legitimate member of BOTH tenants and BOTH workspaces involved.
    // EXPECT: uniform 404 with identical bodies.
    // WHY: workspace.tenant_id = route.organization_id is authoritative.
    let attacks = [
        (w.org_a, w.b1),
        (w.org_b, w.a1),
        (w.org_shared, w.a1),
        (w.org_a, w.s1),
        (w.org_b, w.s1),
        (w.org_shared, w.b1),
    ];
    let mut first: Option<serde_json::Value> = None;
    for (org, ws) in attacks {
        let (status, body) = request(
            w.app.clone(),
            "GET",
            &ws_uri(&org.to_string(), &ws.to_string()),
            Some(&w.t4),
            &[],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "org {org} ws {ws}");
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
        if let Some(first) = &first {
            assert_eq!(first["error"]["message"], body["error"]["message"]);
            assert_eq!(first["error"]["details"], body["error"]["details"]);
        }
        first = Some(body);
    }
    // The same workspaces under their TRUE parents remain accessible to U4.
    for (org, ws) in [(w.org_a, w.a1), (w.org_b, w.b1), (w.org_shared, w.s1)] {
        let (status, _) = request(
            w.app.clone(),
            "GET",
            &ws_uri(&org.to_string(), &ws.to_string()),
            Some(&w.t4),
            &[],
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "true parent must work: org {org} ws {ws}"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// §8 Honest-tenant abnormal rows (POSSIBLE ROW BUT ACCESS DENIED BY QUERY)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn honest_tenant_membership_rows_exist_but_never_grant_access(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;

    // The abnormal rows genuinely exist in the database (schema permits them);
    // proving row presence rules out "access denied because row absent".
    let honest: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_memberships WHERE user_id = $1 AND status = 'active'",
    )
    .bind(w.u3)
    .fetch_one(&pool)
    .await?;
    assert_eq!(honest, 2, "fixture must contain two honest-tenant rows");
    // The rows are tenant-correct (composite FK would reject otherwise);
    // asserted explicitly so the proof does not rest on the FK alone.
    let s1_tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM workspaces WHERE id = $1")
        .bind(w.s1)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        s1_tenant, w.org_shared,
        "row tenant must match the workspace's real tenant"
    );
    let u3_shared_org: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_memberships          WHERE tenant_id = $1 AND user_id = $2 AND status = 'active'",
    )
    .bind(w.org_shared)
    .bind(w.u3)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        u3_shared_org, 0,
        "U3 must NOT be an organization member here"
    );

    let checks = [
        org_uri(&w.org_shared.to_string()),
        org_uri(&w.org_b.to_string()),
        ws_uri(&w.org_shared.to_string(), &w.s1.to_string()),
        ws_uri(&w.org_b.to_string(), &w.b2.to_string()),
        format!("/api/v1/organizations/{}/workspaces", w.org_shared),
        format!("/api/v1/organizations/{}/workspaces", w.org_b),
    ];
    for uri in checks {
        let (status, body) = request(w.app.clone(), "GET", &uri, Some(&w.t3), &[], None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND", "{uri}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// §10 + §11 Session binding and cookie attack matrix
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn authorization_is_bound_to_session_and_immune_to_cookie_manipulation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;

    // ATTACK: session-swap — U1's session probes U2-only resources and vice
    // versa; the shared workspace stays open to BOTH true members.
    // EXPECT: identity comes from the session, never from anything else.
    let org_swaps = [
        (&w.t1, w.org_b, StatusCode::NOT_FOUND),
        (&w.t2, w.org_a, StatusCode::NOT_FOUND),
        (&w.t1, w.org_shared, StatusCode::OK),
        (&w.t2, w.org_shared, StatusCode::OK),
    ];
    for (token, org, expected) in org_swaps {
        let (status, _) = request(
            w.app.clone(),
            "GET",
            &org_uri(&org.to_string()),
            Some(token),
            &[],
            None,
        )
        .await;
        assert_eq!(status, expected, "org {org}");
    }
    let ws_swaps = [
        (&w.t1, w.org_b, w.b1, StatusCode::NOT_FOUND),
        (&w.t2, w.org_a, w.a1, StatusCode::NOT_FOUND),
        (&w.t1, w.org_shared, w.s1, StatusCode::OK),
        (&w.t2, w.org_shared, w.s1, StatusCode::OK),
    ];
    for (token, org, ws, expected) in ws_swaps {
        let (status, _) = request(
            w.app.clone(),
            "GET",
            &ws_uri(&org.to_string(), &ws.to_string()),
            Some(token),
            &[],
            None,
        )
        .await;
        assert_eq!(status, expected, "org {org} ws {ws}");
    }

    // The two sessions are genuinely distinct records bound to distinct
    // users; the access matrix above depends on this separation.
    let (u1_sessions, u2_sessions): (i64, i64) = sqlx::query_as(
        "SELECT             (SELECT count(*) FROM sessions s JOIN users u ON u.id = s.user_id WHERE u.email = 'u1@iso.test' AND s.revoked_at IS NULL),             (SELECT count(*) FROM sessions s JOIN users u ON u.id = s.user_id WHERE u.email = 'u2@iso.test' AND s.revoked_at IS NULL)",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        (u1_sessions, u2_sessions),
        (1, 1),
        "one live session per user"
    );
    assert_ne!(w.t1, w.t2, "session tokens must differ");

    // ATTACK: hostile UX cookies (foreign valid ids, random, malformed,
    // cross-combined) ride along a valid U1 session.
    // EXPECT: identical outcomes to the cookie-less requests.
    let foreign_org = w.org_b.to_string();
    let foreign_ws = w.b1.to_string();
    let random_org = Uuid::now_v7().to_string();
    let random_ws = Uuid::now_v7().to_string();
    let shared_org = w.org_shared.to_string();
    let own_ws = w.a1.to_string();
    let cookie_sets: Vec<Vec<(&str, &str)>> = vec![
        vec![
            ("organization", foreign_org.as_str()),
            ("workspace", foreign_ws.as_str()),
        ],
        vec![
            ("organization", random_org.as_str()),
            ("workspace", random_ws.as_str()),
        ],
        vec![("organization", "not-a-uuid"), ("workspace", "not-a-uuid")],
        vec![
            ("organization", shared_org.as_str()),
            ("workspace", foreign_ws.as_str()),
        ],
        vec![
            ("organization", foreign_org.as_str()),
            ("workspace", own_ws.as_str()),
        ],
    ];
    for cookies in &cookie_sets {
        let (status, _) = request(
            w.app.clone(),
            "GET",
            &org_uri(&w.org_a.to_string()),
            Some(&w.t1),
            cookies,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "own org unaffected by cookies");
        let (status, _) = request(
            w.app.clone(),
            "GET",
            &org_uri(&w.org_b.to_string()),
            Some(&w.t1),
            cookies,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "foreign org stays denied");
        let (status, _) = request(
            w.app.clone(),
            "GET",
            &ws_uri(&w.org_a.to_string(), &w.a1.to_string()),
            Some(&w.t1),
            cookies,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "own workspace unaffected");
        let (status, _) = request(
            w.app.clone(),
            "GET",
            &ws_uri(&w.org_b.to_string(), &w.b1.to_string()),
            Some(&w.t1),
            cookies,
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "foreign workspace stays denied"
        );
        let (status, _) = request(
            w.app.clone(),
            "POST",
            &format!("/api/v1/organizations/{}/workspaces", w.org_b),
            Some(&w.t1),
            cookies,
            Some(serde_json::json!({ "name": "Cookie Intrusion" })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "mutation must stay denied");
    }
    let intrusions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces WHERE lower(slug::text) = 'cookie-intrusion'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(intrusions, 0, "no hostile workspace may exist");
    Ok(())
}

// ---------------------------------------------------------------------------
// §12 Enumeration resistance
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn inaccessible_resources_are_indistinguishable(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;
    // An organization that exists but is soft-deleted (created by U1, then
    // deleted directly in the DB as security-state setup).
    let dead_org = create_org(w.app.clone(), &w.t1, "Iso Dead").await;
    sqlx::query("UPDATE organizations SET deleted_at = now() WHERE id = $1")
        .bind(dead_org)
        .execute(&pool)
        .await?;

    // ATTACK: authenticated U2 probes every inaccessible flavor.
    // EXPECT: one status, one stable code, one public message, one shape.
    // WHY: existence information is itself a leak (API_CONTRACT §1/§3).
    let probes = [
        org_uri(&w.org_a.to_string()),                   // foreign org
        org_uri(&Uuid::now_v7().to_string()),            // random org
        org_uri("not-a-uuid"),                           // malformed org
        org_uri(&dead_org.to_string()),                  // deleted org
        ws_uri(&w.org_a.to_string(), &w.a1.to_string()), // foreign ws
        ws_uri(&w.org_b.to_string(), &Uuid::now_v7().to_string()), // random ws
        ws_uri(&w.org_b.to_string(), "not-a-uuid"),      // malformed ws
        ws_uri(&w.org_b.to_string(), &w.s1.to_string()), // wrong parent (shared ws)
        ws_uri(&w.org_b.to_string(), &w.b2.to_string()), // ws non-member (bare B2)
    ];
    let mut reference: Option<serde_json::Value> = None;
    for uri in probes {
        let (status, body) = request(w.app.clone(), "GET", &uri, Some(&w.t2), &[], None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND", "{uri}");
        let public = serde_json::json!({
            "code": body["error"]["code"],
            "message": body["error"]["message"],
            "details": body["error"]["details"],
        });
        if let Some(reference) = &reference {
            assert_eq!(&public, reference, "{uri} must be indistinguishable");
        }
        reference = Some(public);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// §13 + §14 Mutation isolation and server-derived identity
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn mutations_derive_identity_from_session_and_route_only(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;

    // SECURITY PROPERTY: neither mutation DTO carries any user/tenant
    // identity field — creator and tenant come from session + route context.
    // create organization: exactly one membership, bound to the caller.
    let org = create_org(w.app.clone(), &w.t1, "Mutation Org").await;
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT user_id, status FROM organization_memberships WHERE tenant_id = $1")
            .bind(org)
            .fetch_all(&pool)
            .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].0, w.u1,
        "creator membership must belong to the session user"
    );
    assert_eq!(rows[0].1, "active");
    let (_, u2_orgs) = request(
        w.app.clone(),
        "GET",
        "/api/v1/organizations",
        Some(&w.t2),
        &[],
        None,
    )
    .await;
    let contains = u2_orgs["data"]
        .as_array()
        .unwrap_or_else(|| panic!("array"))
        .iter()
        .any(|o| o["id"].as_str() == Some(org.to_string().as_str()));
    assert!(!contains, "another user must not see the new organization");

    // create workspace: tenant from the route, creator from the session.
    let ws = create_workspace(w.app.clone(), &w.t1, w.org_a, "Mutation Ws").await;
    let (tenant, member): (Uuid, Uuid) = sqlx::query_as(
        "SELECT w.tenant_id, wm.user_id FROM workspaces w \
             JOIN workspace_memberships wm ON wm.workspace_id = w.id WHERE w.id = $1",
    )
    .bind(ws)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        tenant, w.org_a,
        "tenant must come from the resolved route organization"
    );
    assert_eq!(
        member, w.u1,
        "workspace membership must belong to the session user"
    );

    // Cross-tenant mutation: U1 posting into ORG_B is denied with no rows.
    let (status, body) = request(
        w.app.clone(),
        "POST",
        &format!("/api/v1/organizations/{}/workspaces", w.org_b),
        Some(&w.t1),
        &[],
        Some(serde_json::json!({ "name": "Cross Tenant Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces WHERE lower(slug::text) = 'cross-tenant-ws'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(rows, 0, "no workspace may exist");
    let memberships: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_memberships wm          JOIN workspaces w ON w.id = wm.workspace_id          WHERE lower(w.slug::text) = 'cross-tenant-ws'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(memberships, 0, "no workspace membership may exist");
    Ok(())
}

// ---------------------------------------------------------------------------
// §15 TOCTOU — deterministic transactional revalidation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn membership_revocation_between_context_and_transaction_blocks_creation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;

    // ATTACK (deterministic T1/T2 interleaving): U1's context WOULD resolve
    // (membership is valid now), the membership is revoked (T2), and only
    // then does the mutation transaction's authority check run (T1).
    // EXPECT: NotAccessible; zero workspace and zero membership rows.
    // WHY: create_with_membership re-validates inside the transaction
    // (ADR 0007: context validation never replaces transactional invariants).
    // Pre-state proof: the context-equivalent access is genuinely valid now.
    let (status, _) = request(
        w.app.clone(),
        "GET",
        &ws_uri(&w.org_a.to_string(), &w.a1.to_string()),
        Some(&w.t1),
        &[],
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "pre-state: membership must be valid"
    );
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(w.org_a)
    .bind(w.u1)
    .execute(&pool)
    .await?;

    let result = platform_server::workspaces::create_with_membership(
        &pool,
        w.org_a,
        &platform_server::workspaces::NewWorkspace {
            name: "Toctou Ws".to_owned(),
            slug: "toctou-ws".to_owned(),
        },
        w.u1,
        platform_server::rbac::WORKSPACES_CREATE,
    )
    .await;
    assert!(
        matches!(
            result,
            Err(platform_server::workspaces::WorkspaceError::NotAccessible)
        ),
        "revoked membership must reject the mutation inside the transaction"
    );
    let workspaces: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE lower(slug::text) = 'toctou-ws'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(workspaces, 0, "no orphan workspace");
    let memberships: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_memberships wm \
         JOIN workspaces w ON w.id = wm.workspace_id \
         JOIN organization_memberships om ON om.tenant_id = w.tenant_id AND om.user_id = wm.user_id \
         WHERE lower(w.slug::text) = 'toctou-ws'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(memberships, 0, "no membership row");
    Ok(())
}

// ---------------------------------------------------------------------------
// §16 Asymmetric soft-delete revocation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn revoking_one_users_membership_never_touches_another_users_access(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = world(&pool).await;

    // ATTACK: U1 loses ORG_SHARED membership while U2 keeps theirs.
    // EXPECT: U1 loses everything under SHARED; U2 is completely unaffected.
    // WHY: memberships are per-user rows; visibility is per-user evaluation.
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(w.org_shared)
    .bind(w.u1)
    .execute(&pool)
    .await?;

    for uri in [
        org_uri(&w.org_shared.to_string()),
        ws_uri(&w.org_shared.to_string(), &w.s1.to_string()),
        format!("/api/v1/organizations/{}/workspaces", w.org_shared),
    ] {
        let (status, _) = request(w.app.clone(), "GET", &uri, Some(&w.t1), &[], None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "revoked user: {uri}");
    }
    for uri in [
        org_uri(&w.org_shared.to_string()),
        ws_uri(&w.org_shared.to_string(), &w.s1.to_string()),
        format!("/api/v1/organizations/{}/workspaces", w.org_shared),
    ] {
        let (status, _) = request(w.app.clone(), "GET", &uri, Some(&w.t2), &[], None).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "other member must be unaffected: {uri}"
        );
    }
    Ok(())
}
