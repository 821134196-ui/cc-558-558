use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{
    auth::{require_role, AuthUser},
    error::ApiError,
    models::*,
    util, AppState,
};

#[derive(Deserialize)]
pub struct IssuePassReq {
    pub exhibitor_id: i64,
    /// 为空 = 整单放行；非空 = 部分放行（须有已确认的提前撤展变更）
    pub item_ids: Option<Vec<i64>>,
    pub amendment_id: Option<i64>,
}

/// 开具出馆放行凭证（仅场馆方）。
/// 整单放行：该展商所有已入馆展品必须全部核实出馆或异常结案，且无未处理异常。
/// 部分放行：必须对应一份已确认的提前撤展变更，且展品集合完全一致。
pub async fn issue_pass(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<IssuePassReq>,
) -> Result<Json<Value>, ApiError> {
    if user.role == "exhibitor" {
        return Err(ApiError::Forbidden("参展商无权自行开具放行凭证，须由场馆方开具".into()));
    }
    require_role(&user, &["staff", "manager"])?;

    let exhibitor = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
        .bind(body.exhibitor_id).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound("参展商不存在".into()))?;
    let items = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE exhibitor_id = ? ORDER BY id")
        .bind(exhibitor.id).fetch_all(&st.pool).await?;

    let open_exc = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM exceptions e JOIN items i ON i.id = e.item_id
         WHERE i.exhibitor_id = ? AND e.status = 'open'")
        .bind(exhibitor.id).fetch_one(&st.pool).await?;
    if open_exc > 0 {
        return Err(ApiError::Conflict(format!(
            "存在 {open_exc} 条未处理异常，处理完毕前不能放行"
        )));
    }

    let accounted = |s: &str| matches!(s, "verified_out" | "exited" | "missing_closed");
    let (kind, amendment_id, pass_items): (&str, Option<i64>, Vec<i64>) = match &body.item_ids {
        None => {
            // 整单放行：入馆数量必须与 核实出馆+已出馆+缺件结案 相符
            let unaccounted: Vec<String> = items.iter()
                .filter(|it| it.status != "declared" && !accounted(&it.status))
                .map(|it| format!("#{}「{}」({})", it.id, it.name, it.status))
                .collect();
            if !unaccounted.is_empty() {
                return Err(ApiError::Conflict(format!(
                    "入馆与出馆数量不符，还有 {} 件未核实出馆：{}",
                    unaccounted.len(), unaccounted.join("、")
                )));
            }
            let ids: Vec<i64> = items.iter()
                .filter(|it| matches!(it.status.as_str(), "verified_out" | "missing_closed"))
                .map(|it| it.id).collect();
            if ids.is_empty() {
                return Err(ApiError::BadRequest("没有可放行的展品".into()));
            }
            ("full", None, ids)
        }
        Some(ids) => {
            // 部分放行：必须有已确认的提前撤展变更，且展品集合一致
            let am_id = body.amendment_id
                .ok_or_else(|| ApiError::BadRequest("部分放行必须指定 amendment_id".into()))?;
            let am = sqlx::query_as::<_, Amendment>("SELECT * FROM amendments WHERE id = ?")
                .bind(am_id).fetch_optional(&st.pool).await?
                .ok_or_else(|| ApiError::NotFound("变更申请不存在".into()))?;
            if am.exhibitor_id != exhibitor.id || am.kind != "early_exit" || am.status != "confirmed" {
                return Err(ApiError::Conflict(
                    "部分放行须基于该展商已确认的提前撤展变更".into()));
            }
            let payload: Value = serde_json::from_str(&am.payload)
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            let mut allowed: Vec<i64> = payload["item_ids"].as_array()
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            let mut want = ids.clone();
            allowed.sort_unstable();
            want.sort_unstable();
            if allowed != want {
                return Err(ApiError::Conflict(
                    "部分放行的展品必须与已确认的提前撤展变更完全一致".into()));
            }
            for &id in ids {
                let it = items.iter().find(|it| it.id == id)
                    .ok_or_else(|| ApiError::BadRequest(format!("展品 #{id} 不属于该展商")))?;
                if !matches!(it.status.as_str(), "verified_out" | "missing_closed") {
                    return Err(ApiError::Conflict(format!(
                        "展品 #{}「{}」状态为 {}，尚未核实出馆", it.id, it.name, it.status
                    )));
                }
            }
            ("partial", Some(am_id), ids.clone())
        }
    };

    // 同一展品不能挂在两张未使用的凭证上
    for &id in &pass_items {
        let dup = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM exit_pass_items epi
             JOIN exit_passes ep ON ep.id = epi.pass_id
             WHERE epi.item_id = ? AND ep.status = 'issued'")
            .bind(id).fetch_one(&st.pool).await?;
        if dup > 0 {
            return Err(ApiError::Conflict(format!(
                "展品 #{id} 已包含在未使用的放行凭证中"
            )));
        }
    }

    let mut tx = st.pool.begin().await?;
    let code = util::pass_code();
    let pass_id = sqlx::query(
        "INSERT INTO exit_passes (exhibitor_id, pass_code, kind, amendment_id, issued_by)
         VALUES (?,?,?,?,?)")
        .bind(exhibitor.id).bind(&code).bind(kind).bind(amendment_id).bind(user.id)
        .execute(&mut *tx).await?.last_insert_rowid();
    for &id in &pass_items {
        sqlx::query("INSERT INTO exit_pass_items (pass_id, item_id) VALUES (?,?)")
            .bind(pass_id).bind(id).execute(&mut *tx).await?;
        util::log_event(&mut tx, Some(id), "pass_issued", user.id, None, None, Some(pass_id),
            &format!("放行凭证 {code} 开具")).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({
        "pass_id": pass_id, "pass_code": code, "kind": kind,
        "item_count": pass_items.len(), "exhibitor": exhibitor.name,
    })))
}

#[derive(Deserialize)]
pub struct ScanReq {
    pub pass_code: String,
}

/// 闸口扫码（模拟）：核销放行凭证，核实通过的展品标记出馆 exited。
pub async fn gate_scan(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ScanReq>,
) -> Result<Json<Value>, ApiError> {
    let code = body.pass_code.trim().to_uppercase();
    let pass = sqlx::query_as::<_, ExitPass>("SELECT * FROM exit_passes WHERE pass_code = ?")
        .bind(&code).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound(format!("凭证「{code}」不存在")))?;
    if pass.status == "used" {
        return Err(ApiError::Conflict(format!("凭证 {code} 已核销，不能重复使用")));
    }
    if pass.status != "issued" {
        return Err(ApiError::Conflict(format!("凭证 {code} 状态为 {}，不能出馆", pass.status)));
    }

    let mut tx = st.pool.begin().await?;
    let item_ids = sqlx::query_scalar::<_, i64>(
        "SELECT item_id FROM exit_pass_items WHERE pass_id = ?")
        .bind(pass.id).fetch_all(&mut *tx).await?;
    let mut exited = 0i64;
    let mut missing = 0i64;
    for id in item_ids {
        let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
            .bind(id).fetch_one(&mut *tx).await?;
        match item.status.as_str() {
            "verified_out" => {
                sqlx::query("UPDATE items SET status = 'exited' WHERE id = ?")
                    .bind(id).execute(&mut *tx).await?;
                util::log_event(&mut tx, Some(id), "item_exited", user.id, None, None, Some(pass.id),
                    &format!("凭 {code} 经闸口出馆")).await?;
                exited += 1;
            }
            "missing_closed" => {
                util::log_event(&mut tx, Some(id), "exit_accounted_missing", user.id, None, None, Some(pass.id),
                    &format!("缺件结案，凭 {code} 核销计入出馆核对")).await?;
                missing += 1;
            }
            _ => {} // 已出馆等状态不重复处理
        }
    }
    sqlx::query("UPDATE exit_passes SET status = 'used', used_at = datetime('now') WHERE id = ?")
        .bind(pass.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({
        "pass_code": code, "result": "放行成功",
        "exited_count": exited, "missing_accounted": missing,
    })))
}

#[derive(Deserialize)]
pub struct PassQuery {
    pub exhibitor_id: Option<i64>,
}

pub async fn list_passes(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<PassQuery>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_as::<_, ExitPass>("SELECT * FROM exit_passes ORDER BY id DESC")
        .fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for p in rows {
        if user.role == "exhibitor" && user.exhibitor_id != Some(p.exhibitor_id) {
            continue;
        }
        if let Some(e) = q.exhibitor_id {
            if p.exhibitor_id != e { continue; }
        }
        let exhibitor = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
            .bind(p.exhibitor_id).fetch_one(&st.pool).await?;
        let issuer = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(p.issued_by).fetch_one(&st.pool).await?;
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM exit_pass_items WHERE pass_id = ?")
            .bind(p.id).fetch_one(&st.pool).await?;
        out.push(json!({
            "pass": p, "exhibitor": exhibitor.name,
            "issuer": issuer.name, "item_count": count,
        }));
    }
    Ok(Json(json!(out)))
}

pub async fn get_pass(
    State(st): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = sqlx::query_as::<_, ExitPass>("SELECT * FROM exit_passes WHERE id = ?")
        .bind(id).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound("凭证不存在".into()))?;
    let items = sqlx::query_as::<_, Item>(
        "SELECT i.* FROM items i JOIN exit_pass_items epi ON epi.item_id = i.id WHERE epi.pass_id = ?")
        .bind(id).fetch_all(&st.pool).await?;
    Ok(Json(json!({ "pass": p, "items": items })))
}
