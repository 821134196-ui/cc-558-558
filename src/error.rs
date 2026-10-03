use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "未登录或用户不存在，请设置 X-User-Id".into()),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}

/// 判断是否为唯一约束冲突（用于箱号、重复占位、重复入馆等）
pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(db) => db.is_unique_violation(),
        _ => false,
    }
}
