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

// Requests may carry extra UX-context cookies to prove the backend ignores
// them during tenant resolution.
async fn request(
    router: axum::Router,
    uri: &str,
    session: Option<&str>,
    context_cookies: Option<(&str, &str)>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(uri);
    let mut cookie_parts: Vec<String> = Vec::new();
    if let Some(session) = session {
        cookie_parts.push(format!("platform_session={session}"));
    }
    if let Some((organization, workspace)) = context_cookies {
        cookie_parts.push(format!("organization={organization}"));
        cookie_parts.push(format!("workspace={workspace}"));
    }
    if !cookie_parts.is_empty() {
        builder = builder.header(header::COOKIE, cookie_parts.join("; "));
    }
    let response = router
        .oneshot(
            builder
                .body(Body::empty())
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

async fn create_org_via_api(router: axum::Router, token: &str, name: &str) -> Uuid {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v1/organizations")
                .method("POST")
                .header("content-type", "application/json")
                .header(header::COOKIE, format!("platform_session={token}"))
                .body(Body::from(serde_json::json!({ "name": name }).to_string()))
                .unwrap_or_else(|_| panic!("request must build")),
        )
        .await
        .unwrap_or_else(|_| panic!("request must respond"));
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .unwrap_or_else(|_| panic!("body must be readable"));
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!());
    body["data"]["organization"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("organization id must parse"))
}

async fn create_workspace_via_api(
    router: axum::Router,
    token: &str,
    organization: Uuid,
    name: &str,
) -> Uuid {
    let response = router
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/organizations/{organization}/workspaces"))
                .method("POST")
                .header("content-type", "application/json")
                .header(header::COOKIE, format!("platform_session={token}"))
                .body(Body::from(serde_json::json!({ "name": name }).to_string()))
                .unwrap_or_else(|_| panic!("request must build")),
        )
        .await
        .unwrap_or_else(|_| panic!("request must respond"));
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .unwrap_or_else(|_| panic!("body must be readable"));
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!());
    body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("workspace id must parse"))
}

fn org_path(id: &str) -> String {
    format!("/api/v1/organizations/{id}")
}

fn ws_path(org: &str, ws: &str) -> String {
    format!("/api/v1/organizations/{org}/workspaces/{ws}")
}

// ---------------------------------------------------------------------------
// OrganizationContext (step 10 §19)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn organization_context_resolves_only_for_visible_members(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    let user = create_user(&pool, "ctx-org@example.test").await;
    create_user(&pool, "ctx-stranger@example.test").await;
    let token = login_token(app.clone(), "ctx-org@example.test").await;
    let stranger_token = login_token(app.clone(), "ctx-stranger@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Ctx Org").await;

    // Unauthenticated is 401 even for a nonexistent organization: the
    // context resolves authentication before tenant state.
    let (status, body) = request(app.clone(), &org_path(&org.to_string()), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "AUTH_REQUIRED");

    // Valid member resolves.
    let (status, _) = request(app.clone(), &org_path(&org.to_string()), Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);

    // Deleted organization membership stops context resolution.
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await?;
    let (status, body) =
        request(app.clone(), &org_path(&org.to_string()), Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
    // Restore for the remaining cases.
    sqlx::query(
        "UPDATE organization_memberships SET status = 'active', deleted_at = NULL \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await?;

    // Deleted organization is indistinguishable from not found.
    sqlx::query("UPDATE organizations SET deleted_at = now() WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;
    let (status, body) =
        request(app.clone(), &org_path(&org.to_string()), Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
    sqlx::query("UPDATE organizations SET deleted_at = NULL WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;

    // Foreign member, random id and malformed id share the same 404 body.
    let mut previous: Option<serde_json::Value> = None;
    for uri in [
        org_path(&org.to_string()),                    // stranger: foreign org
        org_path(&Uuid::now_v7().to_string()),         // random
        "/api/v1/organizations/not-a-uuid".to_owned(), // malformed
    ] {
        let (status, body) = request(app.clone(), &uri, Some(&stranger_token), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND", "{uri}");
        if let Some(previous) = &previous {
            assert_eq!(previous["error"]["message"], body["error"]["message"]);
        }
        previous = Some(body);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WorkspaceContext (step 10 §20 + §21)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_context_enforces_the_full_access_invariant(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    let user = create_user(&pool, "ctx-ws@example.test").await;
    let outsider = create_user(&pool, "ctx-ws-out@example.test").await;
    let token = login_token(app.clone(), "ctx-ws@example.test").await;
    let outsider_token = login_token(app.clone(), "ctx-ws-out@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Ctx Ws Org").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Ctx Ws").await;

    let good = ws_path(&org.to_string(), &ws.to_string());
    let (status, _) = request(app.clone(), &good, Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);

    // Workspace exists but caller has no memberships at all: 404.
    let (status, _) = request(app.clone(), &good, Some(&outsider_token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Honest-row attack: outsider gains a workspace membership row without
    // an organization membership; context must still refuse.
    sqlx::query(
        "INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) \
         VALUES ($1, $2, $3, $4, 'active')",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(ws)
    .bind(outsider)
    .execute(&pool)
    .await?;
    let (status, body) = request(app.clone(), &good, Some(&outsider_token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    // Soft-deleted organization membership denies despite ws membership.
    sqlx::query(
        "UPDATE organization_memberships SET status = 'deleted', deleted_at = now() \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await?;
    let (status, _) = request(app.clone(), &good, Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query(
        "UPDATE organization_memberships SET status = 'active', deleted_at = NULL \
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await?;

    // Soft-deleted workspace membership denies despite org membership.
    sqlx::query(
        "UPDATE workspace_memberships SET status = 'deleted', deleted_at = now() \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(ws)
    .bind(user)
    .execute(&pool)
    .await?;
    let (status, _) = request(app.clone(), &good, Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query(
        "UPDATE workspace_memberships SET status = 'active', deleted_at = NULL \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(ws)
    .bind(user)
    .execute(&pool)
    .await?;

    // Deleted organization and deleted workspace deny.
    sqlx::query("UPDATE organizations SET deleted_at = now() WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;
    let (status, _) = request(app.clone(), &good, Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE organizations SET deleted_at = NULL WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(ws)
        .execute(&pool)
        .await?;
    let (status, _) = request(app.clone(), &good, Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE workspaces SET deleted_at = NULL WHERE id = $1")
        .bind(ws)
        .execute(&pool)
        .await?;

    // Foreign workspace, random workspace, malformed workspace: uniform 404.
    for uri in [
        good.clone(),
        ws_path(&org.to_string(), &Uuid::now_v7().to_string()),
        ws_path(&org.to_string(), "not-a-uuid"),
        ws_path("not-a-uuid", &ws.to_string()),
    ] {
        let (status, body) = request(app.clone(), &uri, Some(&outsider_token), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND", "{uri}");
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn dual_org_parent_confusion_fails_at_the_context_layer(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "ctx-dual@example.test").await;
    let token = login_token(app.clone(), "ctx-dual@example.test").await;
    let org_a = create_org_via_api(app.clone(), &token, "Dual Ctx A").await;
    let org_b = create_org_via_api(app.clone(), &token, "Dual Ctx B").await;
    let ws_b = create_workspace_via_api(app.clone(), &token, org_b, "Belongs To B").await;

    // Member of BOTH organizations; wrong parent-child combination is 404.
    let (status, body) = request(
        app,
        &ws_path(&org_a.to_string(), &ws_b.to_string()),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn manipulated_context_cookies_never_change_tenant_resolution(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "ctx-cookie@example.test").await;
    create_user(&pool, "ctx-cookie-other@example.test").await;
    let token = login_token(app.clone(), "ctx-cookie@example.test").await;
    let other_token = login_token(app.clone(), "ctx-cookie-other@example.test").await;
    let org = create_org_via_api(app.clone(), &token, "Cookie Org").await;
    let foreign_org = create_org_via_api(app.clone(), &other_token, "Cookie Foreign").await;
    let ws = create_workspace_via_api(app.clone(), &token, org, "Cookie Ws").await;
    let foreign_ws =
        create_workspace_via_api(app.clone(), &other_token, foreign_org, "Foreign Ws").await;

    // UX-context cookies pointing at foreign/foreign-tenant ids must not
    // change any outcome: the resolver reads only session + route + DB.
    for (org_cookie, ws_cookie) in [
        (foreign_org.to_string(), foreign_ws.to_string()),
        (Uuid::now_v7().to_string(), Uuid::now_v7().to_string()),
        ("not-a-uuid".to_owned(), "not-a-uuid".to_owned()),
    ] {
        let (status, _) = request(
            app.clone(),
            &org_path(&org.to_string()),
            Some(&token),
            Some((&org_cookie, &ws_cookie)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "own org stays accessible");
        let (status, _) = request(
            app.clone(),
            &org_path(&foreign_org.to_string()),
            Some(&token),
            Some((&org_cookie, &ws_cookie)),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "foreign org stays denied");
        let (status, _) = request(
            app.clone(),
            &ws_path(&org.to_string(), &ws.to_string()),
            Some(&token),
            Some((&org_cookie, &ws_cookie)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "own workspace stays accessible");
        let (status, _) = request(
            app.clone(),
            &ws_path(&foreign_org.to_string(), &foreign_ws.to_string()),
            Some(&token),
            Some((&org_cookie, &ws_cookie)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "foreign workspace stays denied"
        );
    }
    Ok(())
}
