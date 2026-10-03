use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{
    auth::{require_role, AuthUser},
    error::{is_unique_violation, ApiError},
    models::*,
    util, AppState,
};

#[derive(Deserialize)]
pub struct ReceiveBoxReq {
    pub box_code: String,
    pub exhibitor_id: i64,
    pub item_ids: Vec<i64>,
}

/// 入口按箱接收：创建封箱并关联箱内展品，展品状态 declared -> received。
/// 任何一件校验失败整箱回滚，保证箱与展品关联一致。
pub async fn receive_box(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ReceiveBoxReq>,
) -> Result<Json<Value>, ApiError> {
    require_role(&user, &["receiver", "staff", "manager"])?;
    if body.box_code.trim().is_empty() {
        return Err(ApiError::BadRequest("箱号不能为空".into()));
    }
    if body.item_ids.is_empty() {
        return Err(ApiError::BadRequest("箱内至少需要关联一件展品".into()));
    }

    let mut tx = st.pool.begin().await?;
    sqlx::query_as::<_, Manifest>("SELECT * FROM manifests WHERE exhibitor_id = ?")
        .bind(body.exhibitor_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::BadRequest("该参展商尚未提交入馆清单".into()))?;

    let box_id = match sqlx::query("INSERT INTO boxes (box_code, exhibitor_id, received_by) VALUES (?,?,?)")
        .bind(body.box_code.trim())
        .bind(body.exhibitor_id)
        .bind(user.id)
        .execute(&mut *tx)
        .await
    {
        Ok(r) => r.last_insert_rowid(),
        Err(e) if is_unique_violation(&e) => {
            return Err(ApiError::Conflict(format!("箱号「{}」已存在", body.box_code)))
        }
        Err(e) => return Err(e.into()),
    };

    for &item_id in &body.item_ids {
        let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("展品 #{item_id} 不存在")))?;
        if item.exhibitor_id != body.exhibitor_id {
            return Err(ApiError::BadRequest(format!(
                "展品 #{}「{}」不属于该参展商", item.id, item.name
            )));
        }
        if item.status != "declared" {
            return Err(ApiError::Conflict(format!(
                "展品 #{}「{}」当前状态为 {}，不能重复入馆",
                item.id, item.name, item.status
            )));
        }
        if let Err(e) = sqlx::query("INSERT INTO box_items (box_id, item_id) VALUES (?,?)")
            .bind(box_id).bind(item_id)
            .execute(&mut *tx).await
        {
            if is_unique_violation(&e) {
                return Err(ApiError::Conflict(format!(
                    "展品 #{}「{}」已随其他箱入馆", item.id, item.name
                )));
            }
            return Err(e.into());
        }
        sqlx::query("UPDATE items SET status = 'received' WHERE id = ?")
            .bind(item_id).execute(&mut *tx).await?;
        util::log_event(&mut tx, Some(item_id), "item_received", user.id, None, Some(box_id), None,
            &format!("随箱「{}」入馆", body.box_code.trim())).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "box_id": box_id, "received_count": body.item_ids.len() })))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub exhibitor_id: Option<i64>,
}

pub async fn list_boxes(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let mut sql = "SELECT * FROM boxes".to_string();
    let mut conds = Vec::new();
    if user.role == "exhibitor" {
        conds.push(format!("exhibitor_id = {}", user.exhibitor_id.unwrap_or(-1)));
    } else if let Some(e) = q.exhibitor_id {
        conds.push(format!("exhibitor_id = {e}"));
    }
    if !conds.is_empty() {
        sql.push_str(&format!(" WHERE {}", conds.join(" AND ")));
    }
    sql.push_str(" ORDER BY id DESC");
    let boxes = sqlx::query_as::<_, BoxRow>(&sql).fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for b in boxes {
        let items = sqlx::query_as::<_, Item>(
            "SELECT i.* FROM items i JOIN box_items bi ON bi.item_id = i.id WHERE bi.box_id = ? ORDER BY i.id")
            .bind(b.id).fetch_all(&st.pool).await?;
        let receiver = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(b.received_by).fetch_one(&st.pool).await?;
        out.push(json!({ "box": b, "items": items, "receiver": receiver.name }));
    }
    Ok(Json(json!(out)))
}

pub async fn get_box(
    State(st): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let b = sqlx::query_as::<_, BoxRow>("SELECT * FROM boxes WHERE id = ?")
        .bind(id).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound("箱不存在".into()))?;
    let items = sqlx::query_as::<_, Item>(
        "SELECT i.* FROM items i JOIN box_items bi ON bi.item_id = i.id WHERE bi.box_id = ? ORDER BY i.id")
        .bind(id).fetch_all(&st.pool).await?;
    Ok(Json(json!({ "box": b, "items": items })))
}
