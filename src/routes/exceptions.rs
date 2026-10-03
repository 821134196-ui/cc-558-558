use axum::{extract::{Multipart, Path, Query, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{
    auth::{require_role, AuthUser},
    error::ApiError,
    models::*,
    util, AppState,
};

/// 上报缺件/破损异常（multipart：item_id, kind, note, photo）。
/// 现场照片必传，保存到本地 uploads/ 目录。
pub async fn report_exception(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    mut mp: Multipart,
) -> Result<Json<Value>, ApiError> {
    let mut item_id: Option<i64> = None;
    let mut kind: Option<String> = None;
    let mut note = String::new();
    let mut photo: Option<(String, Vec<u8>)> = None;

    while let Some(field) = mp.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        match field.name() {
            Some("item_id") => {
                item_id = Some(field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?
                    .trim().parse::<i64>().map_err(|_| ApiError::BadRequest("item_id 格式错误".into()))?);
            }
            Some("kind") => {
                kind = Some(field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?);
            }
            Some("note") => {
                note = field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
            }
            Some("photo") => {
                let fname = field.file_name().unwrap_or("photo.jpg").to_string();
                let data = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?.to_vec();
                photo = Some((fname, data));
            }
            _ => {}
        }
    }

    let item_id = item_id.ok_or_else(|| ApiError::BadRequest("缺少 item_id".into()))?;
    let kind = kind.ok_or_else(|| ApiError::BadRequest("缺少 kind".into()))?;
    if kind != "missing" && kind != "damaged" {
        return Err(ApiError::BadRequest("kind 只能是 missing(缺件) 或 damaged(破损)".into()));
    }
    let (fname, data) = photo.ok_or_else(|| ApiError::BadRequest("缺件/破损必须上传现场照片".into()))?;
    if data.is_empty() {
        return Err(ApiError::BadRequest("照片内容为空".into()));
    }

    let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
        .bind(item_id).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound("展品不存在".into()))?;
    if user.role == "exhibitor" && user.exhibitor_id != Some(item.exhibitor_id) {
        return Err(ApiError::Forbidden("只能上报本展商的展品异常".into()));
    }
    if item.status != "received" && item.status != "placed" {
        return Err(ApiError::Conflict(format!(
            "展品 #{}「{}」当前状态为 {}，不能上报异常", item.id, item.name, item.status
        )));
    }
    let open = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM exceptions WHERE item_id = ? AND status = 'open'")
        .bind(item.id).fetch_one(&st.pool).await?;
    if open > 0 {
        return Err(ApiError::Conflict(format!(
            "展品 #{}「{}」已有未处理异常", item.id, item.name
        )));
    }

    // 保存现场照片到本地
    let ext = fname.rsplit('.').next().unwrap_or("jpg")
        .chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
    let ext = if ext.is_empty() { "jpg".into() } else { ext };
    let filename = format!("{}.{}", uuid::Uuid::new_v4().simple(), ext);
    let dir = std::path::Path::new(&st.upload_dir);
    tokio::fs::create_dir_all(dir).await.ok();
    tokio::fs::write(dir.join(&filename), &data).await
        .map_err(|e| ApiError::Internal(format!("照片保存失败: {e}")))?;
    let photo_path = format!("/uploads/{filename}");

    let mut tx = st.pool.begin().await?;
    let ex_id = sqlx::query(
        "INSERT INTO exceptions (item_id, kind, note, photo_path, reported_by) VALUES (?,?,?,?,?)")
        .bind(item.id).bind(&kind).bind(&note).bind(&photo_path).bind(user.id)
        .execute(&mut *tx).await?.last_insert_rowid();
    let kind_label = if kind == "missing" { "缺件" } else { "破损" };
    util::log_event(&mut tx, Some(item.id), "exception_reported", user.id, None, None, None,
        &format!("上报{kind_label}异常：{note}")).await?;
    tx.commit().await?;
    Ok(Json(json!({ "exception_id": ex_id, "photo_path": photo_path })))
}

#[derive(Deserialize)]
pub struct DecideReq {
    pub approve: bool,
}

/// 审批异常：仅场馆人员/主管可批准，参展商无权自行批准异常放行。
/// 批准缺件 -> 展品结案 missing_closed；批准破损 -> 展品按破损核实出馆 verified_out。
pub async fn decide_exception(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<DecideReq>,
) -> Result<Json<Value>, ApiError> {
    if user.role == "exhibitor" {
        return Err(ApiError::Forbidden("参展商无权批准异常放行，须由场馆方审批".into()));
    }
    require_role(&user, &["staff", "manager"])?;

    let mut tx = st.pool.begin().await?;
    let ex = sqlx::query_as::<_, ExceptionCase>("SELECT * FROM exceptions WHERE id = ?")
        .bind(id).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError::NotFound("异常记录不存在".into()))?;
    if ex.status != "open" {
        return Err(ApiError::Conflict("该异常已处理，不能重复审批".into()));
    }
    let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
        .bind(ex.item_id).fetch_one(&mut *tx).await?;

    let new_status = if body.approve { "approved" } else { "rejected" };
    sqlx::query("UPDATE exceptions SET status = ?, decided_by = ?, decided_at = datetime('now') WHERE id = ?")
        .bind(new_status).bind(user.id).bind(ex.id).execute(&mut *tx).await?;

    if body.approve {
        // 关闭该展品的有效占位（缺件：实物已不在展位；破损：核实后撤下）
        if let Some(p) = sqlx::query_as::<_, Placement>(
            "SELECT * FROM placements WHERE item_id = ? AND removed_at IS NULL")
            .bind(item.id).fetch_optional(&mut *tx).await?
        {
            sqlx::query("UPDATE placements SET removed_at = datetime('now'), removed_by = ?, note = '异常结案' WHERE id = ?")
                .bind(user.id).bind(p.id).execute(&mut *tx).await?;
        }
        if ex.kind == "missing" {
            sqlx::query("UPDATE items SET status = 'missing_closed' WHERE id = ?")
                .bind(item.id).execute(&mut *tx).await?;
            util::log_event(&mut tx, Some(item.id), "item_missing_closed", user.id, None, None, None,
                &format!("缺件异常 #{} 经批准结案", ex.id)).await?;
        } else {
            sqlx::query("UPDATE items SET status = 'verified_out' WHERE id = ?")
                .bind(item.id).execute(&mut *tx).await?;
            util::log_event(&mut tx, Some(item.id), "item_verified_out", user.id, None, None, None,
                &format!("破损异常 #{} 经批准，按破损核实出馆", ex.id)).await?;
        }
        util::log_event(&mut tx, Some(item.id), "exception_approved", user.id, None, None, None,
            &format!("异常 #{} 批准", ex.id)).await?;
    } else {
        util::log_event(&mut tx, Some(item.id), "exception_rejected", user.id, None, None, None,
            &format!("异常 #{} 驳回", ex.id)).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "exception_id": ex.id, "status": new_status })))
}

#[derive(Deserialize)]
pub struct ExQuery {
    pub status: Option<String>,
}

pub async fn list_exceptions(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ExQuery>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_as::<_, ExceptionCase>("SELECT * FROM exceptions ORDER BY id DESC")
        .fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for ex in rows {
        if let Some(s) = &q.status {
            if &ex.status != s { continue; }
        }
        let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
            .bind(ex.item_id).fetch_one(&st.pool).await?;
        if user.role == "exhibitor" && user.exhibitor_id != Some(item.exhibitor_id) {
            continue;
        }
        let reporter = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(ex.reported_by).fetch_one(&st.pool).await?;
        let decider = match ex.decided_by {
            Some(d) => Some(sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
                .bind(d).fetch_one(&st.pool).await?.name),
            None => None,
        };
        out.push(json!({
            "exception": ex, "item": item,
            "reporter": reporter.name, "decider": decider,
        }));
    }
    Ok(Json(json!(out)))
}
