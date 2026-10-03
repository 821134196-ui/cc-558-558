use axum::async_trait;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use sqlx::SqlitePool;

use crate::error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub upload_dir: std::path::PathBuf,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub role: String,
}

/// 从请求头 `X-User-Id` 识别当前操作人（演示用轻量鉴权）。
#[derive(Debug, Clone)]
pub struct Auth(pub User);

impl Auth {
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn role(&self) -> &str {
        &self.0.role
    }
    pub fn require_role(&self, roles: &[&str]) -> Result<(), ApiError> {
        if roles.contains(&self.0.role.as_str()) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "当前身份「{}」无权执行该操作（需要：{}）",
                self.0.name,
                roles.join(" / ")
            )))
        }
    }
}

#[async_trait]
impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let uid = parts
            .headers
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("缺少 X-User-Id 请求头"))?;

        let user = sqlx::query_as::<_, User>("SELECT id, name, role FROM users WHERE id = ?1")
            .bind(uid)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| ApiError::unauthorized("用户不存在"))?;

        Ok(Auth(user))
    }
}

// 让 handler 能直接从 State 取 pool
impl FromRef<AppState> for SqlitePool {
    fn from_ref(s: &AppState) -> SqlitePool {
        s.pool.clone()
    }
}
