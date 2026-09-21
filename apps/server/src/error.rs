use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ResourceNotFound,
    MethodNotAllowed,
    ServiceNotReady,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub details: serde_json::Value,
    pub request_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

pub struct ApiError {
    code: ErrorCode,
    request_id: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, request_id: String) -> Self {
        Self { code, request_id }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self.code {
            ErrorCode::ResourceNotFound => (StatusCode::NOT_FOUND, "Resource not found."),
            ErrorCode::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed."),
            ErrorCode::ServiceNotReady => {
                (StatusCode::SERVICE_UNAVAILABLE, "Service is not ready.")
            }
        };
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code,
                    message: message.into(),
                    details: serde_json::json!({}),
                    request_id: self.request_id,
                },
            }),
        )
            .into_response()
    }
}
