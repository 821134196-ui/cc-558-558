use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{
    auth::{require_role, AuthUser},
    error::{is_unique_violation, ApiError},
    models::*,
    util, AppState,
};

#[derive(Deserialize)]
pub struct PlaceReq {
    pub item_id: i64,
    pub booth_id: i64,
}

/// 布展登记：把已入馆展品放到展位上。
/// 数据库部分唯一索引保证同一件展品不会同时出现在两个展位。
pub async fn place_item(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<PlaceReq>,
) -> Result<Json<Value>, ApiError> {
    require_role(&user, &["staff", "manager"])?;
    let mut tx = st.pool.begin().await?;

    let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
        .bind(body.item_id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::NotFound("展品不存在".into()))?;
    if item.status == "placed" {
        return Err(ApiError::Conflict(format!(
            "展品 #{}「{}」已在展位上，不能重复占位；如需挪动请使用转移",
            item.id, item.name
        )));
    }
    if item.status != "received" {
        return Err(ApiError::Conflict(format!(
            "展品 #{}「{}」当前状态为 {}，不能登记占位",
            item.id, item.name, item.status
        )));
    }
    let booth = sqlx::query_as::<_, Booth>("SELECT * FROM booths WHERE id = ?")
        .bind(body.booth_id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::NotFound("展位不存在".into()))?;

    let placement_id = match sqlx::query(
        "INSERT INTO placements (item_id, booth_id, placed_by) VALUES (?,?,?)")
        .bind(item.id).bind(booth.id).bind(user.id)
        .execute(&mut *tx).await
    {
        Ok(r) => r.last_insert_rowid(),
        Err(e) if is_unique_violation(&e) => {
            return Err(ApiError::Conflict(format!(
                "展品 #{}「{}」已存在有效占位，不能同时出现在两个展位", item.id, item.name
            )))
        }
        Err(e) => return Err(e.into()),
    };
    sqlx::query("UPDATE items SET status = 'placed' WHERE id = ?")
        .bind(item.id).execute(&mut *tx).await?;
    util::log_event(&mut tx, Some(item.id), "item_placed", user.id, Some(booth.id), None, None,
        &format!("布展登记于展位 {}", booth.code)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "placement_id": placement_id, "item_id": item.id, "booth": booth.code })))
}

#[derive(Deserialize)]
pub struct TransferReq {
    pub item_id: i64,
    pub to_booth_id: i64,
    pub note: Option<String>,
}

/// 展位间转移：关闭原占位、开启新占位（追加记录而非修改入馆记录）
pub async fn transfer_item(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<TransferReq>,
) -> Result<Json<Value>, ApiError> {
    require_role(&user, &["staff", "manager"])?;
    let mut tx = st.pool.begin().await?;

    let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
        .bind(body.item_id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::NotFound("展品不存在".into()))?;
    let active = sqlx::query_as::<_, Placement>(
        "SELECT * FROM placements WHERE item_id = ? AND removed_at IS NULL")
        .bind(item.id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::Conflict(format!(
            "展品 #{}「{}」当前不在任何展位，无法转移", item.id, item.name
        )))?;
    if active.booth_id == body.to_booth_id {
        return Err(ApiError::BadRequest("展品已在该展位".into()));
    }
    let from = sqlx::query_as::<_, Booth>("SELECT * FROM booths WHERE id = ?")
        .bind(active.booth_id).fetch_one(&mut *tx).await?;
    let to = sqlx::query_as::<_, Booth>("SELECT * FROM booths WHERE id = ?")
        .bind(body.to_booth_id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::NotFound("目标展位不存在".into()))?;

    sqlx::query("UPDATE placements SET removed_at = datetime('now'), removed_by = ?, note = ? WHERE id = ?")
        .bind(user.id)
        .bind(format!("转移至 {}", to.code))
        .bind(active.id)
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO placements (item_id, booth_id, placed_by, note) VALUES (?,?,?,?)")
        .bind(item.id).bind(to.id).bind(user.id)
        .bind(body.note.clone().unwrap_or_default())
        .execute(&mut *tx).await?;
    util::log_event(&mut tx, Some(item.id), "item_transferred", user.id, Some(to.id), None, None,
        &format!("从 {} 转至 {}{}",
            from.code, to.code,
            body.note.as_deref().map(|n| format!("：{n}")).unwrap_or_default())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "item_id": item.id, "from": from.code, "to": to.code })))
}
