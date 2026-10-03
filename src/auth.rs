use axum::{extract::FromRequestParts, http::request::Parts};
use crate::{error::ApiError, models::User, AppState};

/// 从 X-User-Id 请求头解析当前用户（演示用简化认证）
pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<i64>().ok())
            .ok_or(ApiError::Unauthorized)?;
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        Ok(AuthUser(user))
    }
}

/// 角色守卫：当前用户必须属于给定角色之一
pub fn require_role(user: &User, roles: &[&str]) -> Result<(), ApiError> {
    if roles.contains(&user.role.as_str()) {
        Ok(())
    } else {
        Err(ApiError::Forbidden(format!(
            "角色「{}」无权执行此操作",
            role_label(&user.role)
        )))
    }
}

pub fn role_label(role: &str) -> &'static str {
    match role {
        "exhibitor" => "参展商",
        "receiver" => "入口接收员",
        "staff" => "场馆人员",
        "manager" => "场馆主管",
        _ => "未知角色",
    }
}
