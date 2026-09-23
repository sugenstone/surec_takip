//! RBAC authorization tests (First Agent Mission step 13, ADR 0008).
//!
//! Proves: schema tenant-consistency, cross-tenant assignment rejection,
//! permission allow/deny/union, atomic Owner bootstrap, deterministic
//! migration backfill, privilege-resurrection prevention (org variant),
//! workspace-create 403/404 policy, and permission TOCTOU revalidation.

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use platform_server::{
    AppState, config::AuthConfig, password::PasswordService, rbac, router, users::NewUser,
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
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(uri).method(method);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("platform_session={session}"));
    }
    let payload = Body::from(
        body.map(|body| body.to_string())
            .unwrap_or_else(|| "{}".to_owned()),
    );
    let response = router
        .oneshot(builder.body(payload).unwrap_or_else(|_| panic!("build")))
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
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["organization"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse::<Uuid>()
        .unwrap_or_else(|_| panic!("org id must parse"))
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

async fn owner_role_id(pool: &PgPool, org: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM roles WHERE tenant_id = $1 AND name = 'owner' AND is_system",
    )
    .bind(org)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("owner role must exist"))
}

async fn grant_owner(pool: &PgPool, org: Uuid, user: Uuid) {
    let role = owner_role_id(pool, org).await;
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, NULL)",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(user)
    .bind(role)
    .execute(pool)
    .await
    .unwrap_or_else(|_| panic!("owner grant must be insertable"));
}

fn ws_create_uri(org: Uuid) -> String {
    format!("/api/v1/organizations/{org}/workspaces")
}

// ---------------------------------------------------------------------------
// Bootstrap: atomic Owner infrastructure at organization creation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn organization_creation_bootstraps_owner_atomically(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "boot@example.test").await;
    create_user(&pool, "other@example.test").await;
    let token = login(app.clone(), "boot@example.test").await;
    let other_token = login(app.clone(), "other@example.test").await;

    let org = create_org(app.clone(), &token, "Boot Org").await;
    // Built-in roles + owner grant + permission exist for the creator only.
    let roles: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM roles WHERE tenant_id = $1 AND is_system AND deleted_at IS NULL",
    )
    .bind(org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(roles, 2, "owner + member built-ins");
    let grants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM role_permissions rp JOIN roles r ON r.id = rp.role_id \
         WHERE r.tenant_id = $1",
    )
    .bind(org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        grants, 11,
        "owner grants workspaces:create + members:invite + projects:* + sections:* + work_items:* at org scope"
    );
    let assignments: i64 =
        sqlx::query_scalar("SELECT count(*) FROM membership_roles WHERE tenant_id = $1")
            .bind(org)
            .fetch_one(&pool)
            .await?;
    assert_eq!(assignments, 1, "exactly the creator holds an assignment");

    // Creator can create workspaces; another authenticated user cannot even
    // see the organization (membership eligibility gate).
    let (status, _) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Boot Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = request(
        app.clone(),
        "GET",
        &format!("/api/v1/organizations/{org}"),
        Some(&other_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

// ---------------------------------------------------------------------------
// Permission policy on workspace creation
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_creation_requires_permission_with_403_404_policy(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "ws-owner@example.test").await;
    let member = create_user(&pool, "ws-member@example.test").await;
    create_user(&pool, "ws-out@example.test").await;
    let owner_token = login(app.clone(), "ws-owner@example.test").await;
    let member_token = login(app.clone(), "ws-member@example.test").await;
    let outsider_token = login(app.clone(), "ws-out@example.test").await;
    let org = create_org(app.clone(), &owner_token, "Policy Org").await;
    add_org_membership(&pool, org, member).await;

    // Owner (permission) -> 201.
    let (status, _) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&owner_token),
        Some(serde_json::json!({ "name": "Owner Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Eligible member WITHOUT permission -> 403 PERMISSION_DENIED (stable
    // code; no role internals in the response).
    let (status, body) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&member_token),
        Some(serde_json::json!({ "name": "Member Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "PERMISSION_DENIED");
    assert_eq!(body["error"]["details"], serde_json::json!({}));
    let member_ws: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE lower(slug::text) = 'member-ws'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(member_ws, 0);

    // Non-member -> 404 (eligibility precedes authorization).
    let (status, body) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&outsider_token),
        Some(serde_json::json!({ "name": "Out Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");

    // Granting Owner to the member lifts the denial; revoking restores it.
    grant_owner(&pool, org, member).await;
    let (status, _) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&member_token),
        Some(serde_json::json!({ "name": "Granted Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(org)
        .bind(member)
        .execute(&pool)
        .await?;
    let (status, _) = request(
        app,
        "POST",
        &ws_create_uri(org),
        Some(&member_token),
        Some(serde_json::json!({ "name": "Revoked Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    Ok(())
}

// ---------------------------------------------------------------------------
// Permission union across multiple roles
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn multiple_roles_grant_permissions_as_a_union(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "union@example.test").await;
    let token = login(app.clone(), "union@example.test").await;
    let org = create_org(app.clone(), &token, "Union Org").await;
    let user =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = 'union@example.test'")
            .fetch_one(&pool)
            .await?;

    // Two custom roles, EACH granting workspaces:create; the original owner
    // assignment is removed so only the union case is exercised.
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1")
        .bind(org)
        .execute(&pool)
        .await?;
    for name in ["custom-a", "custom-b"] {
        let role: Uuid = sqlx::query_scalar(
            "INSERT INTO roles (id, tenant_id, name, is_system) VALUES ($1, $2, $3, false) RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(org)
        .bind(name)
        .fetch_one(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_id, scope) \
             SELECT $1, p.id, 'organization' FROM permissions p WHERE p.key = 'workspaces:create'",
        )
        .bind(role)
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
             VALUES ($1, $2, $3, $4, NULL)",
        )
        .bind(Uuid::now_v7())
        .bind(org)
        .bind(user)
        .bind(role)
        .execute(&pool)
        .await?;
    }
    // Union: removing ONE granting role keeps the permission via the other.
    sqlx::query("DELETE FROM membership_roles mr USING roles r WHERE r.id = mr.role_id AND r.name = 'custom-a'")
        .execute(&pool)
        .await?;
    let allowed = rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?;
    assert!(allowed, "union of active roles must keep the permission");
    // Removing the second role revokes it.
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await?;
    let allowed = rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?;
    assert!(!allowed);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tenant consistency of assignments (DB-enforced)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn cross_tenant_and_invalid_assignments_are_rejected_by_the_database(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "cta-a@example.test").await;
    create_user(&pool, "cta-b@example.test").await;
    let token_a = login(app.clone(), "cta-a@example.test").await;
    let token_b = login(app.clone(), "cta-b@example.test").await;
    let org_a = create_org(app.clone(), &token_a, "Cta Org A").await;
    let org_b = create_org(app.clone(), &token_b, "Cta Org B").await;
    let user_a: Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'cta-a@example.test'")
            .fetch_one(&pool)
            .await?;
    let role_b = owner_role_id(&pool, org_b).await;

    // ATTACK: tenant-A row claiming tenant-B's role — composite FK rejects.
    let wrong = sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, NULL)",
    )
    .bind(Uuid::now_v7())
    .bind(org_a)
    .bind(user_a)
    .bind(role_b)
    .execute(&pool)
    .await;
    assert!(
        wrong.is_err(),
        "cross-tenant role assignment must fail at the DB level"
    );

    // Non-member of B cannot receive an assignment under B either: the
    // (tenant, user) membership FK rejects the row outright.
    let nonmember = sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, NULL)",
    )
    .bind(Uuid::now_v7())
    .bind(org_b)
    .bind(user_a)
    .bind(role_b)
    .execute(&pool)
    .await;
    assert!(
        nonmember.is_err(),
        "assignment to a non-member must fail at the DB level"
    );

    // Duplicate org-wide assignment is rejected by the partial unique index
    let third = create_user(&pool, "cta-c@example.test").await;
    add_org_membership(&pool, org_b, third).await;
    grant_owner(&pool, org_b, third).await;
    let duplicate = sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, NULL)",
    )
    .bind(Uuid::now_v7())
    .bind(org_b)
    .bind(third)
    .bind(role_b)
    .execute(&pool)
    .await;
    assert!(duplicate.is_err(), "duplicate assignment must be rejected");
    Ok(())
}

// ---------------------------------------------------------------------------
// Privilege resurrection prevention (executable ADR 0006/0007/0008 invariant)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn membership_reactivation_does_not_resurrect_old_privileges(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "res@example.test").await;
    let token = login(app.clone(), "res@example.test").await;
    let org = create_org(app.clone(), &token, "Res Org").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'res@example.test'")
        .fetch_one(&pool)
        .await?;

    // 1-3: membership + owner assignment + permission works.
    let allowed = rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?;
    assert!(allowed);

    // 4-5: deactivate (service transaction) removes membership AND
    // assignments; permission stops working.
    rbac::deactivate_membership_with_assignments(&pool, org, user).await?;
    let allowed = rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?;
    assert!(!allowed, "deactivated membership must lose the permission");

    // 6-7: reactivate; the OLD assignment must NOT become effective again —
    // it was physically deleted at deactivation.
    rbac::reactivate_membership(&pool, org, user).await?;
    let allowed = rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?;
    assert!(!allowed, "reactivation must not resurrect old privileges");
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM membership_roles WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(org)
    .bind(user)
    .fetch_one(&pool)
    .await?;
    assert_eq!(rows, 0, "assignments must be gone, not just ignored");

    // 8: an explicit new assignment restores authority.
    grant_owner(&pool, org, user).await;
    let allowed = rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?;
    assert!(allowed, "explicit re-assignment must work");
    Ok(())
}

// ---------------------------------------------------------------------------
// Workspace-scoped authorization semantics
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_authorization_combines_org_and_workspace_grants(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "scope@example.test").await;
    let token = login(app.clone(), "scope@example.test").await;
    let org = create_org(app.clone(), &token, "Scope Org").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'scope@example.test'")
        .fetch_one(&pool)
        .await?;
    let (status, body) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Scope Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ws: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("ws id must parse"));

    // Org-wide owner grant authorizes the workspace scope (inheritance A).
    let allowed = rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?;
    assert!(allowed, "org-scope grant must apply to workspace scope");

    // Remove org-wide assignments: without workspace-scoped grants the same
    // permission is denied at workspace scope.
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await?;
    let allowed = rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?;
    assert!(!allowed, "no org grant and no ws grant must deny");
    Ok(())
}

// ---------------------------------------------------------------------------
// Migration backfill for pre-RBAC organizations (deterministic)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn pre_rbac_organizations_get_no_automatic_owner_but_support_explicit_bootstrap(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    use platform_server::migrations::MIGRATOR;

    // Simulate a pre-RBAC organization: revert the RBAC migration, create an
    // organization with two members — the LATE joiner being the hypothetical
    // real administrator — then re-apply. Historical order in test data is
    // arbitrary by design: the migration must NOT guess.
    // Revert directly to the last pre-RBAC version, located BY NAME so later
    // migrations (e.g. 007_projects) cannot shift a hardcoded position
    // (revert_last verifies full history first, so it cannot be called twice
    // in a row).
    let up_migrations: Vec<(i64, std::borrow::Cow<'_, str>)> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| (m.version, m.description.clone()))
        .collect();
    let rbac_index = up_migrations
        .iter()
        .position(|(_, description)| description.contains("rbac"))
        .unwrap_or_else(|| panic!("the rbac migration must exist in the chain"));
    let target = up_migrations[rbac_index - 1].0;
    MIGRATOR.undo(&pool, target).await?;
    create_user(&pool, "early@example.test").await;
    let late = create_user(&pool, "late@example.test").await;
    let org: Uuid = sqlx::query_scalar(
        "INSERT INTO organizations (id, name, slug, status, default_timezone) \
         VALUES ($1, 'Old Org', 'old-org', 'active', 'UTC') RETURNING id",
    )
    .bind(Uuid::now_v7())
    .fetch_one(&pool)
    .await?;
    let early: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'early@example.test'")
        .fetch_one(&pool)
        .await?;
    add_org_membership(&pool, org, early).await;
    add_org_membership(&pool, org, late).await;
    MIGRATOR.run(&pool).await?;

    // Built-in roles and the owner grant ARE seeded...
    let roles: i64 =
        sqlx::query_scalar("SELECT count(*) FROM roles WHERE tenant_id = $1 AND is_system")
            .bind(org)
            .fetch_one(&pool)
            .await?;
    assert_eq!(roles, 2, "builtin roles are seeded");
    // ...but NO membership_roles row exists: no guessed Owner privilege.
    let assignments: i64 =
        sqlx::query_scalar("SELECT count(*) FROM membership_roles WHERE tenant_id = $1")
            .bind(org)
            .fetch_one(&pool)
            .await?;
    assert_eq!(assignments, 0, "no automatic owner backfill, ever");
    assert!(
        !rbac::authorize_organization(&pool, org, early, rbac::WORKSPACES_CREATE).await?,
        "earliest member must NOT be privileged by accident of order"
    );
    assert!(
        !rbac::authorize_organization(&pool, org, late, rbac::WORKSPACES_CREATE).await?,
        "no member is privileged without explicit bootstrap"
    );

    // Explicit trusted bootstrap: assign_owner refuses non-members and grants
    // exactly the chosen administrator.
    let outsider = create_user(&pool, "outsider@example.test").await;
    assert!(rbac::assign_owner(&pool, org, outsider).await.is_err());
    rbac::assign_owner(&pool, org, late).await?;
    assert!(
        rbac::authorize_organization(&pool, org, late, rbac::WORKSPACES_CREATE).await?,
        "explicit bootstrap grants the chosen administrator"
    );
    assert!(
        !rbac::authorize_organization(&pool, org, early, rbac::WORKSPACES_CREATE).await?,
        "other members stay unprivileged"
    );
    // Idempotent: repeating the bootstrap changes nothing.
    rbac::assign_owner(&pool, org, late).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM membership_roles WHERE tenant_id = $1")
            .bind(org)
            .fetch_one(&pool)
            .await?;
    assert_eq!(count, 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Workspace-scoped pre-grant and resurrection invariants
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_role_requires_membership_row_and_matching_tenants(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "wfk-a@example.test").await;
    create_user(&pool, "wfk-b@example.test").await;
    let token_a = login(app.clone(), "wfk-a@example.test").await;
    let token_b = login(app.clone(), "wfk-b@example.test").await;
    let org_a = create_org(app.clone(), &token_a, "Wfk Org A").await;
    let org_b = create_org(app.clone(), &token_b, "Wfk Org B").await;
    let user_a: Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'wfk-a@example.test'")
            .fetch_one(&pool)
            .await?;
    let (status, body) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org_a),
        Some(&token_a),
        Some(serde_json::json!({ "name": "Wfk A1" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let a1: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("ws id must parse"));
    let (status, body) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org_b),
        Some(&token_b),
        Some(serde_json::json!({ "name": "Wfk B1" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let b1: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("ws id must parse"));
    let role_a: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (id, tenant_id, name, is_system) VALUES ($1, $2, 'ws-role-a', false) RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(org_a)
    .fetch_one(&pool)
    .await?;

    // ATTACK 1 (pre-grant): ws-scoped assignment for a member WITHOUT the
    // workspace membership row — the (workspace_id, user_id) FK rejects it.
    // (user_a CREATED a1, so the API already gave them a membership; a fresh
    // org-only member is the correct attacker.)
    let org_only = create_user(&pool, "wfk-c@example.test").await;
    add_org_membership(&pool, org_a, org_only).await;
    let pregrant = sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(org_a)
    .bind(org_only)
    .bind(role_a)
    .bind(a1)
    .execute(&pool)
    .await;
    assert!(
        pregrant.is_err(),
        "workspace-scoped assignment must require the workspace membership row"
    );

    // ATTACK 2 (role tenant/workspace mismatch): a role of tenant A bound to
    // workspace B1 of tenant B — the roles composite FK rejects it.
    let mismatch = sqlx::query("UPDATE roles SET workspace_id = $1 WHERE id = $2")
        .bind(b1)
        .bind(role_a)
        .execute(&pool)
        .await;
    assert!(
        mismatch.is_err(),
        "a role cannot bind to another tenant's workspace"
    );

    // ATTACK 3 (cross-tenant ws assignment): tenant A row claiming B1 as its
    // workspace — the membership_roles composite FK rejects it. (user_b is
    // org_b's creator; a B1 membership row for user_a is inserted under
    // tenant B to satisfy the ws-membership FK, isolating the tenant check.)
    sqlx::query("INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) VALUES ($1, $2, $3, $4, 'active')")
        .bind(Uuid::now_v7())
        .bind(org_b)
        .bind(b1)
        .bind(user_a)
        .execute(&pool)
        .await?;
    let cross = sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(org_a)
    .bind(user_a)
    .bind(role_a)
    .bind(b1)
    .execute(&pool)
    .await;
    assert!(
        cross.is_err(),
        "cross-tenant workspace assignment must fail"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_membership_reactivation_does_not_resurrect_workspace_grants(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "wres@example.test").await;
    let token = login(app.clone(), "wres@example.test").await;
    let org = create_org(app.clone(), &token, "Wres Org").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'wres@example.test'")
        .fetch_one(&pool)
        .await?;
    let (status, body) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Wres Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ws: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("ws id must parse"));

    // Isolate the workspace-scoped grant: the creator's org-wide owner
    // assignment would otherwise mask the ws-scope lifecycle under test.
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2 AND workspace_id IS NULL")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await?;

    // A workspace-scoped role granting the permission at workspace scope.
    let role: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (id, tenant_id, name, is_system) VALUES ($1, $2, 'wres-ws-role', false) RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO role_permissions (role_id, permission_id, scope) \
         SELECT $1, p.id, 'workspace' FROM permissions p WHERE p.key = 'workspaces:create'",
    )
    .bind(role)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(user)
    .bind(role)
    .bind(ws)
    .execute(&pool)
    .await?;

    // 1-4: active org + ws membership + ws-scoped grant -> permission works.
    assert!(
        rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?,
        "pre-state: workspace grant works"
    );

    // 5-6: deactivate the WORKSPACE membership (service transaction deletes
    // the ws-scoped assignments); permission denied.
    rbac::deactivate_workspace_membership_with_assignments(&pool, org, ws, user).await?;
    assert!(
        !rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?,
        "deactivated workspace membership must lose the permission"
    );

    // 7-8: reactivate — the OLD workspace grant must NOT resurrect.
    rbac::reactivate_workspace_membership(&pool, ws, user).await?;
    assert!(
        !rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?,
        "reactivation must not resurrect workspace grants"
    );
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM membership_roles WHERE tenant_id = $1 AND user_id = $2 AND workspace_id = $3",
    )
    .bind(org)
    .bind(user)
    .bind(ws)
    .fetch_one(&pool)
    .await?;
    assert_eq!(rows, 0, "ws-scoped assignments must be physically gone");

    // Explicit re-assignment restores authority (org-wide owner still works
    // independently).
    sqlx::query(
        "INSERT INTO membership_roles (id, tenant_id, user_id, role_id, workspace_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(org)
    .bind(user)
    .bind(role)
    .bind(ws)
    .execute(&pool)
    .await?;
    assert!(
        rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?,
        "explicit re-assignment must work"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn authorize_workspace_denies_without_active_workspace_membership(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "wgate@example.test").await;
    let token = login(app.clone(), "wgate@example.test").await;
    let org = create_org(app.clone(), &token, "Wgate Org").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'wgate@example.test'")
        .fetch_one(&pool)
        .await?;
    let (status, body) = request(
        app.clone(),
        "POST",
        &ws_create_uri(org),
        Some(&token),
        Some(serde_json::json!({ "name": "Wgate Ws" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ws: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|_| panic!("ws id must parse"));

    // Org-wide owner grant exists; with the workspace membership soft-deleted
    // (directly, bypassing the lifecycle service) the service itself must
    // still deny — the wsm join is independent defense in depth.
    assert!(
        rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?,
        "pre-state: both memberships active"
    );
    sqlx::query(
        "UPDATE workspace_memberships SET status = 'deleted', deleted_at = now() \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(ws)
    .bind(user)
    .execute(&pool)
    .await?;
    assert!(
        !rbac::authorize_workspace(&pool, org, ws, user, rbac::WORKSPACES_CREATE).await?,
        "service must deny without an active workspace membership (own join)"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Permission TOCTOU: revocation between check and transaction
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn permission_revoked_between_check_and_transaction_blocks_creation(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(state(&pool));
    create_user(&pool, "ptoctou@example.test").await;
    let token = login(app.clone(), "ptoctou@example.test").await;
    let org = create_org(app.clone(), &token, "Ptoctou Org").await;
    let user: Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'ptoctou@example.test'")
            .fetch_one(&pool)
            .await?;

    // Handler-level check would pass; permission is then revoked (T2) before
    // the mutation transaction runs (T1) — the in-transaction revalidation
    // must reject with no rows written.
    assert!(
        rbac::authorize_organization(&pool, org, user, rbac::WORKSPACES_CREATE).await?,
        "pre-state: permission valid"
    );
    sqlx::query("DELETE FROM membership_roles WHERE tenant_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await?;
    let result = platform_server::workspaces::create_with_membership(
        &pool,
        org,
        &platform_server::workspaces::NewWorkspace {
            name: "Ptoctou Ws".to_owned(),
            slug: "ptoctou-ws".to_owned(),
        },
        user,
        rbac::WORKSPACES_CREATE,
    )
    .await;
    assert!(
        matches!(
            result,
            Err(platform_server::workspaces::WorkspaceError::Forbidden)
        ),
        "revoked permission must reject inside the transaction"
    );
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces WHERE lower(slug::text) = 'ptoctou-ws'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(rows, 0, "no workspace may be written");
    Ok(())
}
