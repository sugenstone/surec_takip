use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use platform_server::router;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower::ServiceExt;

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
        let response = router(pool.clone())
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
    let response = router(pool)
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
