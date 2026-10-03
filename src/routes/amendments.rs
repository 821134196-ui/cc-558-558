use axum::{extract::{Path, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{
    auth::{require_role, AuthUser},
    error::ApiError,
    models::*,
    util, AppState,
};

#[derive(Deserialize)]
pub struct CreateAmendment {
    pub kind: String,
    pub payload: Value,
    pub exhibitor_id: Option<i64>,
}

/// 发起变更申请：临时替换展品 / 转借展位 / 提前撤展。
/// 变更只追加新记录，确认前不生效，绝不改写入馆记录。
pub async fn create_amendment(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateAmendment>,
) -> Result<Json<Value>, ApiError> {
    require_role(&user, &["exhibitor", "staff", "manager"])?;
    let exhibitor_id = if user.role == "exhibitor" {
        user.exhibitor_id.ok_or_else(|| ApiError::Forbidden("该账号未关联参展商".into()))?
    } else {
        body.exhibitor_id.ok_or_else(|| ApiError::BadRequest("请指定 exhibitor_id".into()))?
    };

    // 按类型校验 payload，并生成可读摘要
    let summary = match body.kind.as_str() {
        "replacement" => {
            let item_id = body.payload.get("item_id").and_then(Value::as_i64)
                .ok_or_else(|| ApiError::BadRequest("replacement 需要 payload.item_id".into()))?;
            let new_name = body.payload.get("new_name").and_then(Value::as_str)
                .map(str::trim).filter(|s| !s.is_empty())
                .ok_or_else(|| ApiError::BadRequest("replacement 需要 payload.new_name".into()))?;
            let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
                .bind(item_id).fetch_optional(&st.pool).await?
                .ok_or_else(|| ApiError::NotFound("被替换展品不存在".into()))?;
            if item.exhibitor_id != exhibitor_id {
                return Err(ApiError::BadRequest("被替换展品不属于该参展商".into()));
            }
            format!("临时替换：#{}「{}」→ 新展品「{}」", item.id, item.name, new_name)
        }
        "booth_transfer" => {
            let booth_id = body.payload.get("booth_id").and_then(Value::as_i64)
                .ok_or_else(|| ApiError::BadRequest("booth_transfer 需要 payload.booth_id".into()))?;
            let to_ex = body.payload.get("to_exhibitor_id").and_then(Value::as_i64)
                .ok_or_else(|| ApiError::BadRequest("booth_transfer 需要 payload.to_exhibitor_id".into()))?;
            let booth = sqlx::query_as::<_, Booth>("SELECT * FROM booths WHERE id = ?")
                .bind(booth_id).fetch_optional(&st.pool).await?
                .ok_or_else(|| ApiError::NotFound("展位不存在".into()))?;
            if booth.owner_exhibitor_id != Some(exhibitor_id) {
                return Err(ApiError::BadRequest("只能转借本展商名下的展位".into()));
            }
            sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
                .bind(to_ex).fetch_optional(&st.pool).await?
                .ok_or_else(|| ApiError::NotFound("目标参展商不存在".into()))?;
            format!("转借展位 {} 给参展商 #{}", booth.code, to_ex)
        }
        "early_exit" => {
            let ids = body.payload.get("item_ids").and_then(Value::as_array)
                .ok_or_else(|| ApiError::BadRequest("early_exit 需要 payload.item_ids".into()))?;
            if ids.is_empty() {
                return Err(ApiError::BadRequest("提前撤展至少需要一件展品".into()));
            }
            for v in ids {
                let item_id = v.as_i64().ok_or_else(|| ApiError::BadRequest("item_ids 格式错误".into()))?;
                let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
                    .bind(item_id).fetch_optional(&st.pool).await?
                    .ok_or_else(|| ApiError::NotFound(format!("展品 #{item_id} 不存在")))?;
                if item.exhibitor_id != exhibitor_id {
                    return Err(ApiError::BadRequest(format!(
                        "展品 #{}「{}」不属于该参展商", item.id, item.name
                    )));
                }
            }
            format!("提前撤展 {} 件展品", ids.len())
        }
        other => return Err(ApiError::BadRequest(format!("不支持的变更类型：{other}"))),
    };

    let mut tx = st.pool.begin().await?;
    let id = sqlx::query(
        "INSERT INTO amendments (exhibitor_id, kind, payload, created_by) VALUES (?,?,?,?)")
        .bind(exhibitor_id).bind(&body.kind).bind(body.payload.to_string()).bind(user.id)
        .execute(&mut *tx).await?.last_insert_rowid();
    util::log_event(&mut tx, None, "amendment_requested", user.id, None, None, None,
        &format!("变更申请 #{id}（{}）：{summary}", body.kind)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "amendment_id": id, "summary": summary })))
}

#[derive(Deserialize)]
pub struct DecideAmendmentReq {
    pub approve: bool,
}

/// 审批变更：仅场馆人员/主管可确认；确认时才按类型生效（追加新记录）。
pub async fn decide_amendment(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<DecideAmendmentReq>,
) -> Result<Json<Value>, ApiError> {
    if user.role == "exhibitor" {
        return Err(ApiError::Forbidden("参展商无权自行确认变更，须由场馆方重新确认清单".into()));
    }
    require_role(&user, &["staff", "manager"])?;

    let mut tx = st.pool.begin().await?;
    let am = sqlx::query_as::<_, Amendment>("SELECT * FROM amendments WHERE id = ?")
        .bind(id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::NotFound("变更申请不存在".into()))?;
    if am.status != "pending" {
        return Err(ApiError::Conflict("该变更已处理，不能重复审批".into()));
    }
    let payload: Value = serde_json::from_str(&am.payload)
        .map_err(|e| ApiError::Internal(format!("变更内容解析失败: {e}")))?;

    let new_status = if body.approve { "confirmed" } else { "rejected" };
    sqlx::query("UPDATE amendments SET status = ?, decided_by = ?, decided_at = datetime('now') WHERE id = ?")
        .bind(new_status).bind(user.id).bind(am.id).execute(&mut *tx).await?;

    let mut effect = String::new();
    if body.approve {
        match am.kind.as_str() {
            "replacement" => {
                let old_id = payload["item_id"].as_i64().unwrap();
                let new_name = payload["new_name"].as_str().unwrap().to_string();
                let new_desc = payload.get("new_description").and_then(Value::as_str).unwrap_or("");
                let new_serial = payload.get("new_serial_no").and_then(Value::as_str).unwrap_or("");
                // 追加一条新清单项（origin=replacement），原入馆记录保持不变
                let manifest_id = sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM manifests WHERE exhibitor_id = ?")
                    .bind(am.exhibitor_id).fetch_one(&mut *tx).await?;
                let new_id = sqlx::query(
                    "INSERT INTO items (manifest_id, exhibitor_id, name, description, serial_no, origin)
                     VALUES (?,?,?,?,?,'replacement')")
                    .bind(manifest_id).bind(am.exhibitor_id)
                    .bind(&new_name).bind(new_desc).bind(new_serial)
                    .execute(&mut *tx).await?.last_insert_rowid();
                util::log_event(&mut tx, Some(new_id), "item_declared", user.id, None, None, None,
                    &format!("临时替换入场（变更 #{}），替代展品 #{}", am.id, old_id)).await?;
                util::log_event(&mut tx, Some(old_id), "item_replaced", user.id, None, None, None,
                    &format!("被新展品 #{}「{}」替换（变更 #{}）", new_id, new_name, am.id)).await?;
                effect = format!("新展品 #{new_id} 已加入清单，待按箱入馆");
            }
            "booth_transfer" => {
                let booth_id = payload["booth_id"].as_i64().unwrap();
                let to_ex = payload["to_exhibitor_id"].as_i64().unwrap();
                sqlx::query("UPDATE booths SET owner_exhibitor_id = ? WHERE id = ?")
                    .bind(to_ex).bind(booth_id).execute(&mut *tx).await?;
                util::log_event(&mut tx, None, "booth_transferred", user.id, Some(booth_id), None, None,
                    &format!("展位转借给参展商 #{to_ex}（变更 #{}）", am.id)).await?;
                effect = format!("展位 #{booth_id} 已转借给参展商 #{to_ex}");
            }
            "early_exit" => {
                effect = "提前撤展已确认，可为清单内指定展品开具部分放行凭证".into();
            }
            _ => {}
        }
        util::log_event(&mut tx, None, "amendment_confirmed", user.id, None, None, None,
            &format!("变更 #{}（{}）已确认", am.id, am.kind)).await?;
    } else {
        util::log_event(&mut tx, None, "amendment_rejected", user.id, None, None, None,
            &format!("变更 #{}（{}）已驳回", am.id, am.kind)).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "amendment_id": am.id, "status": new_status, "effect": effect })))
}

pub async fn list_amendments(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_as::<_, Amendment>("SELECT * FROM amendments ORDER BY id DESC")
        .fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for am in rows {
        if user.role == "exhibitor" && user.exhibitor_id != Some(am.exhibitor_id) {
            continue;
        }
        let exhibitor = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
            .bind(am.exhibitor_id).fetch_one(&st.pool).await?;
        let creator = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(am.created_by).fetch_one(&st.pool).await?;
        let decider = match am.decided_by {
            Some(d) => Some(sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
                .bind(d).fetch_one(&st.pool).await?.name),
            None => None,
        };
        out.push(json!({
            "amendment": am, "exhibitor": exhibitor.name,
            "creator": creator.name, "decider": decider,
        }));
    }
    Ok(Json(json!(out)))
}
