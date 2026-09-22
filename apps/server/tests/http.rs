use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use platform_server::{AppState, config::AuthConfig, router};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower::ServiceExt;

fn test_state(pool: sqlx::PgPool) -> AppState {
    AppState::new(pool, AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message))
}

#[tokio::test]
async fn liveness_does_not_require_database_and_unknown_routes_are_structured()
-> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost:1/unavailable")?;
    for (uri, method, expected) in [
        ("/api/v1/health", "GET", StatusCode::OK),
        (
            "/unknown/private-value?token=secret",
            "GET",
            StatusCode::NOT_FOUND,
        ),
        ("/api/v1/health", "POST", StatusCode::METHOD_NOT_ALLOWED),
    ] {
        let response = router(test_state(pool.clone()))
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method(method)
                    .header("x-request-id", "untrusted")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), expected);
        let id = response.headers()["x-request-id"].to_str()?.to_owned();
        assert!(uuid::Uuid::parse_str(&id).is_ok());
        let bytes = to_bytes(response.into_body(), 4096).await?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;
        if expected != StatusCode::OK {
            assert_eq!(body["error"]["request_id"], id);
            assert!(!String::from_utf8_lossy(&bytes).contains("private-value"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn readiness_fails_without_database_and_hides_connection_details()
-> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(100))
        .connect_lazy("postgres://localhost:1/unavailable")?;
    let response = router(test_state(pool))
        .oneshot(
            Request::builder()
                .uri("/api/v1/ready")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = to_bytes(response.into_body(), 4096).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["error"]["code"], "SERVICE_NOT_READY");
    assert!(!String::from_utf8_lossy(&bytes).contains("postgres"));
    Ok(())
}

#[tokio::test]
async fn protected_auth_endpoints_reject_missing_session_with_structured_error()
-> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost:1/unavailable")?;
    for uri in ["/api/v1/auth/me", "/api/v1/auth/logout"] {
        let response = router(test_state(pool.clone()))
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method("POST")
                    .body(Body::empty())?,
            )
            .await?;
        // me rejects POST as method-not-allowed; both endpoints must refuse
        // unauthenticated access without a database round-trip.
        let expected = if uri.ends_with("/logout") {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::METHOD_NOT_ALLOWED
        };
        assert_eq!(response.status(), expected, "{uri}");
    }
    let response = router(test_state(pool))
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = to_bytes(response.into_body(), 4096).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["error"]["code"], "AUTH_REQUIRED");
    Ok(())
}

#[tokio::test]
async fn login_rejects_malformed_and_invalid_bodies_without_echoing_them()
-> Result<(), Box<dyn std::error::Error>> {
    use axum::extract::ConnectInfo;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost:1/unavailable")?;
    let state = test_state(pool);
    // login reads ConnectInfo; tests inject it directly since oneshot has no socket.
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 51_111);
    let malformed = router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .extension(ConnectInfo(address))
                .body(Body::from(r#"{"email": "secret-attempt"#))?,
        )
        .await?;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(malformed.into_body(), 4096).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert!(!String::from_utf8_lossy(&bytes).contains("secret-attempt"));

    let missing_fields = router(state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .extension(ConnectInfo(address))
                .body(Body::from(r#"{"email": "", "password": ""}"#))?,
        )
        .await?;
    assert_eq!(missing_fields.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = to_bytes(missing_fields.into_body(), 4096).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(body["error"]["details"]["fields"]["email"][0], "Required");
    assert_eq!(
        body["error"]["details"]["fields"]["password"][0],
        "Required"
    );
    Ok(())
}

#[tokio::test]
async fn organization_endpoints_reject_unauthenticated_requests()
-> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost:1/unavailable")?;
    let state = test_state(pool);
    for (uri, method) in [
        ("/api/v1/organizations", "POST"),
        ("/api/v1/organizations", "GET"),
        (
            "/api/v1/organizations/01a0c5c7-f298-7144-a0f6-96ad1fe99c6f",
            "GET",
        ),
    ] {
        let response = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method(method)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))?,
            )
            .await?;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
        let bytes = to_bytes(response.into_body(), 4096).await?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(body["error"]["code"], "AUTH_REQUIRED", "{method} {uri}");
    }
    Ok(())
}

#[tokio::test]
async fn workspace_endpoints_reject_unauthenticated_requests()
-> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost:1/unavailable")?;
    let state = test_state(pool);
    for (uri, method) in [
        (
            "/api/v1/organizations/01a0c5c7-f298-7144-a0f6-96ad1fe99c6f/workspaces",
            "POST",
        ),
        (
            "/api/v1/organizations/01a0c5c7-f298-7144-a0f6-96ad1fe99c6f/workspaces",
            "GET",
        ),
        (
            "/api/v1/organizations/01a0c5c7-f298-7144-a0f6-96ad1fe99c6f/workspaces/01a0c5c7-f298-7144-a0f6-96ad1fe99c6f",
            "GET",
        ),
    ] {
        let response = router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method(method)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))?,
            )
            .await?;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
        let bytes = to_bytes(response.into_body(), 4096).await?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(body["error"]["code"], "AUTH_REQUIRED", "{method} {uri}");
    }
    Ok(())
}

// Pins the real axum path-extraction contract our context extractors rely on
// (apps/server/src/context.rs): struct extraction is by FIELD NAME and
// ignores extra path parameters, so OrganizationContext/WorkspaceContext stay
// reusable on future deeper routes (e.g. .../workspaces/{w}/projects/{p}).
// Tuple or plain-String extraction would reject any route whose total
// parameter count differs.
#[tokio::test]
async fn named_path_struct_extraction_ignores_extra_route_parameters()
-> Result<(), Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct WorkspaceRoute {
        organization_id: String,
        workspace_id: String,
    }
    async fn deep_handler(
        axum::extract::Path(route): axum::extract::Path<WorkspaceRoute>,
    ) -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({
            "organization_id": route.organization_id,
            "workspace_id": route.workspace_id,
        }))
    }
    let router = axum::Router::new().route(
        "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}",
        axum::routing::get(deep_handler),
    );
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v1/organizations/org-1/workspaces/ws-1/projects/prj-1")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "struct extraction must succeed on a three-parameter route"
    );
    let bytes = to_bytes(response.into_body(), 4096).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["organization_id"], "org-1");
    assert_eq!(body["workspace_id"], "ws-1");
    Ok(())
}

#[tokio::test]
async fn rbac_catalog_endpoints_reject_unauthenticated_requests()
-> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost:1/unavailable")?;
    let state = test_state(pool);
    for uri in [
        "/api/v1/organizations/01a0c5c7-f298-7144-a0f6-96ad1fe99c6f/permissions",
        "/api/v1/organizations/01a0c5c7-f298-7144-a0f6-96ad1fe99c6f/roles",
    ] {
        let response = router(state.clone())
            .oneshot(Request::builder().uri(uri).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
        let bytes = to_bytes(response.into_body(), 4096).await?;
        let body: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(body["error"]["code"], "AUTH_REQUIRED", "{uri}");
    }
    Ok(())
}
