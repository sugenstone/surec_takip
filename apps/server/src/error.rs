use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ResourceNotFound,
    MethodNotAllowed,
    ServiceNotReady,
    AuthRequired,
    AuthInvalidCredentials,
    PermissionDenied,
    ValidationError,
    InternalError,
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

#[derive(Debug)]
pub struct ApiError {
    code: ErrorCode,
    status: StatusCode,
    message: &'static str,
    details: serde_json::Value,
    request_id: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    pub fn new(code: ErrorCode, request_id: String) -> Self {
        let (status, message) = match code {
            ErrorCode::ResourceNotFound => (StatusCode::NOT_FOUND, "Resource not found."),
            ErrorCode::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed."),
            ErrorCode::ServiceNotReady => {
                (StatusCode::SERVICE_UNAVAILABLE, "Service is not ready.")
            }
            ErrorCode::AuthRequired => (StatusCode::UNAUTHORIZED, "Authentication is required."),
            ErrorCode::AuthInvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "Email or password is incorrect.")
            }
            ErrorCode::PermissionDenied => (
                StatusCode::FORBIDDEN,
                "You do not have permission to perform this action.",
            ),
            ErrorCode::ValidationError => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Some fields are invalid.")
            }
            ErrorCode::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected error occurred.",
            ),
        };
        Self {
            code,
            status,
            message,
            details: serde_json::json!({}),
            request_id,
        }
    }

    // Malformed request bodies use 400 per the HTTP status conventions while
    // keeping the same stable VALIDATION_ERROR code as field-level failures.
    pub fn malformed_json(request_id: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: "The request body is not valid JSON.",
            ..Self::new(ErrorCode::ValidationError, request_id)
        }
    }

    pub fn invalid_fields(fields: serde_json::Value, request_id: String) -> Self {
        Self {
            details: serde_json::json!({ "fields": fields }),
            ..Self::new(ErrorCode::ValidationError, request_id)
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code,
                    message: self.message.into(),
                    details: self.details,
                    request_id: self.request_id,
                },
            }),
        )
            .into_response()
    }
}
