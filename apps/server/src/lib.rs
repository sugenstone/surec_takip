pub mod config;
pub mod database;
pub mod error;
pub mod migrations;

use axum::{
    Extension, Json, Router,
    extract::{MatchedPath, Request, State},
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use error::{ApiError, ErrorCode};
use serde::Serialize;
use sqlx::PgPool;
use std::time::Instant;
use utoipa::{OpenApi, ToSchema};

#[derive(Clone)]
pub struct RequestId(pub String);

#[derive(Serialize, ToSchema)]
pub struct HealthData {
    pub status: HealthStatus,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Ready,
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub data: HealthData,
}

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/ready", get(ready))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn(request_context))
        .with_state(pool)
}

#[utoipa::path(get, path = "/api/v1/health", responses((status = 200, body = HealthResponse)))]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        data: HealthData {
            status: HealthStatus::Ok,
        },
    })
}

#[utoipa::path(get, path = "/api/v1/ready", responses(
    (status = 200, body = HealthResponse),
    (status = 503, body = error::ErrorEnvelope)
))]
async fn ready(
    State(pool): State<PgPool>,
    Extension(id): Extension<RequestId>,
) -> Result<Json<HealthResponse>, ApiError> {
    if !database::is_ready(&pool).await {
        return Err(ApiError::new(ErrorCode::ServiceNotReady, id.0));
    }
    Ok(Json(HealthResponse {
        data: HealthData {
            status: HealthStatus::Ready,
        },
    }))
}

async fn not_found(Extension(id): Extension<RequestId>) -> ApiError {
    ApiError::new(ErrorCode::ResourceNotFound, id.0)
}

async fn method_not_allowed(Extension(id): Extension<RequestId>) -> ApiError {
    ApiError::new(ErrorCode::MethodNotAllowed, id.0)
}

async fn request_context(mut request: Request, next: Next) -> Response {
    let request_id = uuid::Uuid::now_v7().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let method = request.method().clone();
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let started = Instant::now();
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    // Route templates only: raw paths, query strings, cookies and credentials never enter logs.
    tracing::info!(%request_id, %method, %route, status = response.status().as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "http_request");
    response
}

#[derive(OpenApi)]
#[openapi(
    paths(health, ready),
    components(schemas(
        HealthResponse,
        HealthData,
        HealthStatus,
        error::ErrorEnvelope,
        error::ErrorBody,
        error::ErrorCode
    ))
)]
pub struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
