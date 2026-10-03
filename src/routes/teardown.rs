use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{
    auth::{require_role, AuthUser},
    error::ApiError,
    models::*,
    util, AppState,
};

#[derive(Deserialize)]
pub struct VerifyReq {
    pub item_id: i64,
}

/// 撤展逐件核实：实物与清单核对无误后标记 verified_out。
/// 核实通过后关闭展位占位；破损/缺件请走异常上报（需现场照片）。
pub async fn verify_item(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<VerifyReq>,
) -> Result<Json<Value>, ApiError> {
    require_role(&user, &["staff", "manager"])?;
    let mut tx = st.pool.begin().await?;

    let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
        .bind(body.item_id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::NotFound("展品不存在".into()))?;
    if item.status != "placed" && item.status != "received" {
        return Err(ApiError::Conflict(format!(
            "展品 #{}「{}」当前状态为 {}，不能核实出馆",
            item.id, item.name, item.status
        )));
    }
    // 若该展品有未处理异常，必须先处理完异常
    let open = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM exceptions WHERE item_id = ? AND status = 'open'")
        .bind(item.id).fetch_one(&mut *tx).await?;
    if open > 0 {
        return Err(ApiError::Conflict(format!(
            "展品 #{}「{}」存在未处理异常，请先处理异常", item.id, item.name
        )));
    }

    if let Some(p) = sqlx::query_as::<_, Placement>(
        "SELECT * FROM placements WHERE item_id = ? AND removed_at IS NULL")
        .bind(item.id).fetch_optional(&mut *tx).await?
    {
        sqlx::query("UPDATE placements SET removed_at = datetime('now'), removed_by = ?, note = '撤展核实' WHERE id = ?")
            .bind(user.id).bind(p.id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE items SET status = 'verified_out' WHERE id = ?")
        .bind(item.id).execute(&mut *tx).await?;
    util::log_event(&mut tx, Some(item.id), "item_verified_out", user.id, None, None, None,
        "撤展核实：实物与清单相符").await?;
    tx.commit().await?;
    Ok(Json(json!({ "item_id": item.id, "status": "verified_out" })))
}
