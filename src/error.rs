use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    Status(StatusCode, String),
    Sqlx(sqlx::Error),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::BAD_REQUEST, msg.into())
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::UNAUTHORIZED, msg.into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::FORBIDDEN, msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::NOT_FOUND, msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::CONFLICT, msg.into())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        // 数据库唯一约束（如重复占位）兜底映射为 409
        if let sqlx::Error::Database(db) = &e {
            if db.message().contains("UNIQUE") || db.message().contains("constraint") {
                return ApiError::Status(
                    StatusCode::CONFLICT,
                    "数据冲突：可能是重复占位或唯一约束被触发".into(),
                );
            }
        }
        ApiError::Sqlx(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Status(code, msg) => {
                (code, Json(json!({ "error": msg }))).into_response()
            }
            ApiError::Sqlx(e) => {
                tracing::error!("db error: {e:?}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("数据库错误: {e}") })),
                )
                    .into_response()
            }
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
