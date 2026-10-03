use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, Auth};

fn now() -> String {
    Utc::now().to_rfc3339()
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

/// 参展商身份只能操作自己的记录；场馆人员不限。
async fn ensure_exhibitor_scope(
    pool: &SqlitePool,
    auth: &Auth,
    exhibitor_id: &str,
) -> ApiResult<()> {
    if auth.role() == "exhibitor" {
        let own: Option<(String,)> =
            sqlx::query_as("SELECT id FROM exhibitors WHERE id=?1 AND user_id=?2")
                .bind(exhibitor_id)
                .bind(auth.id())
                .fetch_optional(pool)
                .await?;
        if own.is_none() {
            return Err(ApiError::forbidden("只能操作本参展商的记录"));
        }
    }
    Ok(())
}

/// 按 id 或 (参展商, 展品编码) 找展品。
async fn find_exhibit(
    pool: &SqlitePool,
    ident: &str,
    exhibitor_id: Option<&str>,
) -> ApiResult<Value> {
    let row: Option<(String, String, String, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, exhibitor_id, manifest_id, code, name, active, status FROM exhibits \
         WHERE id=?1 OR (code=?1 AND (?2 IS NULL OR exhibitor_id=?2)) LIMIT 1",
    )
    .bind(ident)
    .bind(exhibitor_id)
    .fetch_optional(pool)
    .await?;
    let (id, eid, mid, code, name, active, status) =
        row.ok_or_else(|| ApiError::not_found("展品不存在"))?;
    Ok(json!({
        "id": id, "exhibitor_id": eid, "manifest_id": mid,
        "code": code, "name": name, "active": active, "status": status,
    }))
}

async fn log_event(
    tx: &mut Transaction<'_, Sqlite>,
    exhibit_id: &str,
    event_type: &str,
    actor_id: Option<&str>,
    crate_id: Option<&str>,
    booth_id: Option<&str>,
    detail: Option<&str>,
) -> ApiResult<()> {
    let (seq,): (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(seq),0)+1 FROM movement_events WHERE exhibit_id=?1")
            .bind(exhibit_id)
            .fetch_one(&mut **tx)
            .await?;
    sqlx::query(
        "INSERT INTO movement_events(id,exhibit_id,seq,event_type,actor_id,crate_id,booth_id,detail,created_at) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
    )
    .bind(Uuid::new_v4().to_string()).bind(exhibit_id).bind(seq).bind(event_type)
    .bind(actor_id).bind(crate_id).bind(booth_id).bind(detail).bind(now())
    .execute(&mut **tx).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 基础数据
// ---------------------------------------------------------------------------

pub async fn me(State(s): State<AppState>, auth: Auth) -> impl axum::response::IntoResponse {
    let mut v = serde_json::to_value(&auth.0).unwrap();
    let eid: Option<(String,)> =
        sqlx::query_as("SELECT id FROM exhibitors WHERE user_id=?1")
            .bind(auth.id())
            .fetch_optional(&s.pool)
            .await
            .unwrap();
    v["exhibitor_id"] = json!(eid.map(|x| x.0));
    Json(v)
}

pub async fn list_users(State(s): State<AppState>, _auth: Auth) -> Json<Value> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id,name,role FROM users ORDER BY id")
            .fetch_all(&s.pool)
            .await
            .unwrap();
    Json(json!(rows
        .into_iter()
        .map(|(id, name, role)| json!({"id": id, "name": name, "role": role}))
        .collect::<Vec<_>>()))
}

pub async fn list_exhibitors(State(s): State<AppState>, _auth: Auth) -> Json<Value> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id,name FROM exhibitors ORDER BY id")
            .fetch_all(&s.pool)
            .await
            .unwrap();
    Json(json!(rows
        .into_iter()
        .map(|(id, name)| json!({"id": id, "name": name}))
        .collect::<Vec<_>>()))
}

pub async fn list_booths(State(s): State<AppState>, _auth: Auth) -> Json<Value> {
    let rows: Vec<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.id,b.code,b.name,b.owner_exhibitor_id,COALESCE(e.name,''),COALESCE(b.loan_note,'') \
         FROM booths b LEFT JOIN exhibitors e ON e.id=b.borrower_exhibitor_id ORDER BY b.code",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    Json(json!(rows
        .into_iter()
        .map(|(id, code, name, owner, borrower, note)| json!({
            "id": id, "code": code, "name": name,
            "owner_exhibitor_id": owner,
            "borrower_exhibitor_name": if borrower.is_empty() { Value::Null } else { json!(borrower) },
            "loan_note": note,
        }))
        .collect::<Vec<_>>()))
}

/// 全局看板：每家参展商数量核对 + 每个展位当前展品。
pub async fn overview(State(s): State<AppState>, _auth: Auth) -> Json<Value> {
    let exhibitors: Vec<(String, String)> =
        sqlx::query_as("SELECT id,name FROM exhibitors ORDER BY id")
            .fetch_all(&s.pool)
            .await
            .unwrap();

    let mut out = Vec::new();
    for (eid, ename) in exhibitors {
        let declared: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1",
        )
        .bind(&eid)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let received: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1 \
             AND status IN ('RECEIVED','PLACED','OUT')",
        )
        .bind(&eid)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let placed: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1 AND status='PLACED'",
        )
        .bind(&eid)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let out_n: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1 AND status='OUT'",
        )
        .bind(&eid)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let missing: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM exceptions x JOIN exhibits e ON e.id=x.exhibit_id \
             WHERE e.exhibitor_id=?1 AND e.active=1 AND x.kind='missing' AND x.status='open'",
        )
        .bind(&eid)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let damaged: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM exceptions x JOIN exhibits e ON e.id=x.exhibit_id \
             WHERE e.exhibitor_id=?1 AND e.active=1 AND x.kind='damaged' AND x.status='open'",
        )
        .bind(&eid)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let written_off: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT x.exhibit_id) FROM exceptions x JOIN exhibits e ON e.id=x.exhibit_id \
             WHERE e.exhibitor_id=?1 AND e.active=1 AND x.status='written_off'",
        )
        .bind(&eid)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let declared_n = declared.0;
        let accounted = out_n.0 + written_off.0;
        out.push(json!({
            "exhibitor_id": eid, "exhibitor_name": ename,
            "declared": declared_n, "received": received.0,
            "placed": placed.0, "out": out_n.0,
            "open_missing": missing.0, "open_damaged": damaged.0,
            "written_off": written_off.0,
            "balanced": declared_n == received.0 && declared_n == accounted
                        && missing.0 == 0 && damaged.0 == 0,
        }));
    }

    let booths: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT p.id, b.code, b.name, e.code, e.name FROM placements p \
         JOIN booths b ON b.id=p.booth_id JOIN exhibits e ON e.id=p.exhibit_id \
         WHERE p.active=1 ORDER BY b.code, e.code",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    let mut booth_map: HashMap<String, Value> = HashMap::new();
    for (pid, bcode, bname, ecode, ename) in booths {
        let entry = booth_map
            .entry(bcode.clone())
            .or_insert_with(|| json!({"booth": bname, "items": Vec::<Value>::new()}));
        entry["items"].as_array_mut().unwrap().push(json!({
            "placement_id": pid, "exhibit_code": ecode, "exhibit_name": ename,
        }));
    }

    Json(json!({ "exhibitors": out, "booths": booth_map }))
}

// ---------------------------------------------------------------------------
// 清单提交（版本化）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ManifestBody {
    exhibitor_id: String,
    title: String,
    items: Vec<ManifestItem>,
}
#[derive(Deserialize)]
pub struct ManifestItem {
    code: String,
    name: String,
}

pub async fn submit_manifest(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<ManifestBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    auth.require_role(&["exhibitor"])?;
    ensure_exhibitor_scope(&s.pool, &auth, &body.exhibitor_id).await?;
    if body.items.is_empty() {
        return Err(ApiError::bad_request("清单至少包含一件展品"));
    }

    let mut tx = s.pool.begin().await?;
    let version: i64 = sqlx::query_as::<_, (i64,)>(
        "SELECT COALESCE(MAX(version),0)+1 FROM manifests WHERE exhibitor_id=?1",
    )
    .bind(&body.exhibitor_id)
    .fetch_one(&mut *tx)
    .await?
    .0;
    if version > 1 {
        return Err(ApiError::bad_request(
            "清单已存在；临时替换展品请走「变更申请」，不能直接重报覆盖入馆记录",
        ));
    }

    let manifest_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO manifests(id,exhibitor_id,version,title,status,change_kind,declared_count,created_at) \
         VALUES(?1,?2,1,?3,'submitted','initial',?4,?5)",
    )
    .bind(&manifest_id).bind(&body.exhibitor_id).bind(&body.title)
    .bind(body.items.len() as i64).bind(now())
    .execute(&mut *tx).await?;

    for it in &body.items {
        let eid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO exhibits(id,exhibitor_id,manifest_id,code,name,status,created_at) \
             VALUES(?1,?2,?3,?4,?5,'DECLARED',?6)",
        )
        .bind(&eid).bind(&body.exhibitor_id).bind(&manifest_id)
        .bind(&it.code).bind(&it.name).bind(now())
        .execute(&mut *tx).await?;
        log_event(&mut tx, &eid, "DECLARED", Some(auth.id()), None, None,
                  Some(&body.title)).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({
        "manifest_id": manifest_id, "version": 1, "count": body.items.len()
    }))))
}

pub async fn list_manifests(
    State(s): State<AppState>,
    auth: Auth,
    Path(eid): Path<String>,
) -> ApiResult<Json<Value>> {
    ensure_exhibitor_scope(&s.pool, &auth, &eid).await?;
    let versions: Vec<(String, i64, String, String, i64, String)> = sqlx::query_as(
        "SELECT id,version,title,status,declared_count,created_at FROM manifests \
         WHERE exhibitor_id=?1 ORDER BY version",
    )
    .bind(&eid)
    .fetch_all(&s.pool)
    .await?;
    let items: Vec<(String, String, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id,code,name,status,active,replaced_by_id FROM exhibits \
         WHERE exhibitor_id=?1 ORDER BY manifest_id, code",
    )
    .bind(&eid)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(json!({
        "versions": versions.into_iter().map(|(id,v,t,st,c,at)| json!({
            "id": id, "version": v, "title": t, "status": st, "declared_count": c, "created_at": at
        })).collect::<Vec<_>>(),
        "items": items.into_iter().map(|(id,code,name,status,active,replaced)| json!({
            "id": code, "code": code, "name": name, "status": status,
            "active": active == 1, "replaced_by_id": replaced,
            "exhibit_row_id": id,
        })).collect::<Vec<_>>(),
    })))
}

// ---------------------------------------------------------------------------
// 按箱接收入馆
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CrateBody {
    code: String,
    exhibitor_id: String,
    exhibit_codes: Vec<String>,
    note: Option<String>,
}

pub async fn receive_crate(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<CrateBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    auth.require_role(&["gatekeeper"])?;
    if body.exhibit_codes.is_empty() {
        return Err(ApiError::bad_request("箱内至少关联一件展品"));
    }
    let dup: Option<(String,)> = sqlx::query_as("SELECT id FROM crates WHERE code=?1")
        .bind(&body.code)
        .fetch_optional(&s.pool)
        .await?;
    if dup.is_some() {
        return Err(ApiError::conflict(format!("箱号 {} 已接收，禁止重复入馆", body.code)));
    }
    let manifest_exists: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM manifests WHERE exhibitor_id=?1 AND status='submitted'",
    )
    .bind(&body.exhibitor_id)
    .fetch_optional(&s.pool)
    .await?;
    if manifest_exists.is_none() {
        return Err(ApiError::bad_request("该参展商尚未提交清单"));
    }

    let mut tx = s.pool.begin().await?;
    // 校验箱内每一件：在本参展商清单内、有效、尚未入馆
    let mut ids = Vec::new();
    for code in &body.exhibit_codes {
        let row: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT id, active, status FROM exhibits WHERE exhibitor_id=?1 AND code=?2 AND active=1",
        )
        .bind(&body.exhibitor_id).bind(code)
        .fetch_optional(&mut *tx).await?;
        match row {
            None => return Err(ApiError::bad_request(format!("展品 {code} 不在当前有效清单内"))),
            Some((_, 0, _)) => return Err(ApiError::bad_request(format!("展品 {code} 已失效"))),
            Some((_, _, st)) if st != "DECLARED" => {
                return Err(ApiError::conflict(format!("展品 {code} 已入馆（当前状态 {st}），不能重复接收")));
            }
            Some((id, _, _)) => ids.push((id, code.clone())),
        }
    }

    let crate_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO crates(id,code,exhibitor_id,receiver_id,note,sealed,received_at) \
         VALUES(?1,?2,?3,?4,?5,1,?6)",
    )
    .bind(&crate_id).bind(&body.code).bind(&body.exhibitor_id)
    .bind(auth.id()).bind(body.note.as_deref()).bind(now())
    .execute(&mut *tx).await?;

    for (id, code) in &ids {
        sqlx::query("INSERT INTO crate_items(crate_id,exhibit_id,received_at) VALUES(?1,?2,?3)")
            .bind(&crate_id).bind(id).bind(now())
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE exhibits SET status='RECEIVED', last_handler_id=?1 WHERE id=?2")
            .bind(auth.id()).bind(id)
            .execute(&mut *tx).await?;
        log_event(&mut tx, id, "RECEIVED", Some(auth.id()), Some(&crate_id), None,
                  Some(&format!("按箱 {} 封箱接收", body.code))).await?;
        let _ = code;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({
        "crate_id": crate_id, "code": body.code, "received": ids.len()
    }))))
}

pub async fn list_crates(State(s): State<AppState>, _auth: Auth) -> ApiResult<Json<Value>> {
    let crates: Vec<(String, String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id,c.code,c.exhibitor_id,u.name,COALESCE(c.note,''),c.received_at \
         FROM crates c JOIN users u ON u.id=c.receiver_id ORDER BY c.received_at",
    )
    .fetch_all(&s.pool)
    .await?;
    let mut result = Vec::new();
    for (id, code, eid, receiver, note, at) in crates {
        let items: Vec<(String, String)> = sqlx::query_as(
            "SELECT e.code,e.name FROM crate_items ci JOIN exhibits e ON e.id=ci.exhibit_id \
             WHERE ci.crate_id=?1 ORDER BY e.code",
        )
        .bind(&id)
        .fetch_all(&s.pool)
        .await?;
        result.push(json!({
            "id": id, "code": code, "exhibitor_id": eid, "receiver": receiver,
            "note": note, "received_at": at,
            "items": items.into_iter().map(|(c,n)| json!({"code":c,"name":n})).collect::<Vec<_>>(),
        }));
    }
    Ok(Json(json!(result)))
}

// ---------------------------------------------------------------------------
// 展品查询 / 追溯
// ---------------------------------------------------------------------------

pub async fn list_exhibits(
    State(s): State<AppState>,
    _auth: Auth,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String, String, String, String, i64, String, String, String)> =
        sqlx::query_as(
            "SELECT e.id,e.exhibitor_id,e.code,e.name,e.status,e.active, \
             COALESCE(u.name,''), COALESCE((SELECT b.code FROM placements p JOIN booths b ON b.id=p.booth_id \
                WHERE p.exhibit_id=e.id AND p.active=1),''), \
             COALESCE((SELECT p.id FROM placements p WHERE p.exhibit_id=e.id AND p.active=1),'') \
             FROM exhibits e LEFT JOIN users u ON u.id=e.last_handler_id \
             WHERE (?1 IS NULL OR e.exhibitor_id=?1) ORDER BY e.code",
        )
        .bind(q.get("exhibitor_id"))
        .fetch_all(&s.pool)
        .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(id, eid, code, name, status, active, handler, booth, pid)| json!({
            "id": id, "exhibitor_id": eid, "code": code, "name": name,
            "status": status, "active": active == 1,
            "last_handler": if handler.is_empty() { Value::Null } else { json!(handler) },
            "booth": if booth.is_empty() { Value::Null } else { json!(booth) },
            "placement_id": if pid.is_empty() { Value::Null } else { json!(pid) },
        }))
        .collect::<Vec<_>>())))
}

pub async fn trace_exhibit(
    State(s): State<AppState>,
    _auth: Auth,
    Path(ident): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let ex = find_exhibit(&s.pool, &ident, q.get("exhibitor_id").map(|x| x.as_str())).await?;
    let eid = ex["id"].as_str().unwrap();

    let events: Vec<(i64, String, Option<String>, Option<String>, Option<String>, Option<String>, String)> =
        sqlx::query_as(
            "SELECT m.seq,m.event_type,COALESCE(u.name,''),COALESCE(c.code,''),COALESCE(b.code,''), \
             COALESCE(m.detail,''),m.created_at \
             FROM movement_events m LEFT JOIN users u ON u.id=m.actor_id \
             LEFT JOIN crates c ON c.id=m.crate_id LEFT JOIN booths b ON b.id=m.booth_id \
             WHERE m.exhibit_id=?1 ORDER BY m.seq",
        )
        .bind(eid)
        .fetch_all(&s.pool)
        .await?;

    let exc: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT x.id,x.kind,x.status,COALESCE(x.note,''),u.name,x.created_at \
         FROM exceptions x JOIN users u ON u.id=x.opened_by WHERE x.exhibit_id=?1 ORDER BY x.created_at",
    )
    .bind(eid)
    .fetch_all(&s.pool)
    .await?;

    let photos: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id,fs_path,created_at FROM photos WHERE exhibit_id=?1 ORDER BY created_at",
    )
    .bind(eid)
    .fetch_all(&s.pool)
    .await?;

    Ok(Json(json!({
        "exhibit": ex,
        "events": events.into_iter().map(|(seq,ty,actor,crate_,booth,detail,at)| json!({
            "seq": seq, "type": ty,
            "actor": if actor.as_deref().unwrap_or("").is_empty() { Value::Null } else { json!(actor) },
            "crate": if crate_.as_deref().unwrap_or("").is_empty() { Value::Null } else { json!(crate_) },
            "booth": if booth.as_deref().unwrap_or("").is_empty() { Value::Null } else { json!(booth) },
            "detail": detail, "at": at,
        })).collect::<Vec<_>>(),
        "exceptions": exc.into_iter().map(|(id,k,st,note,by,at)| json!({
            "id": id, "kind": k, "status": st, "note": note, "opened_by": by, "created_at": at
        })).collect::<Vec<_>>(),
        "photos": photos.into_iter().map(|(id,p,at)| json!({
            "id": id, "url": format!("/uploads/{}", p.rsplit('/').next().unwrap_or("")), "at": at
        })).collect::<Vec<_>>(),
    })))
}

// ---------------------------------------------------------------------------
// 布展占位 / 转移 / 撤下
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PlaceBody {
    exhibit_id: String,
    booth_id: String,
}

pub async fn place_exhibit(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<PlaceBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    auth.require_role(&["venue"])?;
    let mut tx = s.pool.begin().await?;

    let ex: Option<(String, String, i64, String)> = sqlx::query_as(
        "SELECT id,exhibitor_id,active,status FROM exhibits WHERE id=?1 AND active=1",
    )
    .bind(&body.exhibit_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (id, ex_exhibitor, _, status) =
        ex.ok_or_else(|| ApiError::bad_request("展品不存在或已失效"))?;
    if !matches!(status.as_str(), "RECEIVED" | "PLACED") {
        return Err(ApiError::bad_request(format!("展品当前状态 {status}，不能占位")));
    }

    // 展位归属/借用校验
    let booth: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT owner_exhibitor_id, borrower_exhibitor_id FROM booths WHERE id=?1")
            .bind(&body.booth_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (owner, borrower) = booth.ok_or_else(|| ApiError::not_found("展位不存在"))?;
    if owner != ex_exhibitor && borrower.as_deref() != Some(ex_exhibitor.as_str()) {
        return Err(ApiError::bad_request(
            "该展位不属于此参展商，也未通过「转借展位」审批借用",
        ));
    }

    // 核心校验：同一件展品不能同时出现在两个展位
    let conflict: Option<(String,)> = sqlx::query_as(
        "SELECT b.code FROM placements p JOIN booths b ON b.id=p.booth_id \
         WHERE p.exhibit_id=?1 AND p.active=1",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((bcode,)) = conflict {
        return Err(ApiError::conflict(format!(
            "该展品已在展位 {bcode} 生效占位；同一件展品不能同时出现在两个展位，请先撤下或走转移"
        )));
    }

    let pid = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO placements(id,exhibit_id,booth_id,active,placed_by,placed_at) \
         VALUES(?1,?2,?3,1,?4,?5)",
    )
    .bind(&pid).bind(&id).bind(&body.booth_id).bind(auth.id()).bind(now())
    .execute(&mut *tx).await?;
    sqlx::query("UPDATE exhibits SET status='PLACED', last_handler_id=?1 WHERE id=?2")
        .bind(auth.id()).bind(&id)
        .execute(&mut *tx).await?;
    log_event(&mut tx, &id, "PLACED", Some(auth.id()), None, Some(&body.booth_id), None).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({"placement_id": pid}))))
}

#[derive(Deserialize)]
pub struct TransferBody {
    booth_id: String,
}

pub async fn transfer_exhibit(
    State(s): State<AppState>,
    auth: Auth,
    Path(ident): Path<String>,
    Json(body): Json<TransferBody>,
) -> ApiResult<Json<Value>> {
    auth.require_role(&["venue"])?;
    let ex = find_exhibit(&s.pool, &ident, None).await?;
    let id = ex["id"].as_str().unwrap().to_string();
    let ex_exhibitor = ex["exhibitor_id"].as_str().unwrap().to_string();

    let mut tx = s.pool.begin().await?;
    let current: Option<(String, String, String)> = sqlx::query_as(
        "SELECT p.id, p.booth_id, b.code FROM placements p JOIN booths b ON b.id=p.booth_id \
         WHERE p.exhibit_id=?1 AND p.active=1",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?;
    let (old_pid, old_booth_id, old_booth) =
        current.ok_or_else(|| ApiError::bad_request("展品当前未在任何展位，不能转移"))?;

    let target: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id,owner_exhibitor_id,borrower_exhibitor_id FROM booths WHERE id=?1",
    )
    .bind(&body.booth_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (_, owner, borrower) = target.ok_or_else(|| ApiError::not_found("目标展位不存在"))?;
    if owner != ex_exhibitor && borrower.as_deref() != Some(ex_exhibitor.as_str()) {
        return Err(ApiError::bad_request("目标展位未借用给该展品所属参展商"));
    }
    if old_booth_id == body.booth_id {
        return Err(ApiError::conflict("展品已在该展位，无需转移"));
    }

    sqlx::query("UPDATE placements SET active=0, removed_at=?1, removed_by=?2 WHERE id=?3")
        .bind(now()).bind(auth.id()).bind(old_pid)
        .execute(&mut *tx).await?;
    let pid = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO placements(id,exhibit_id,booth_id,active,placed_by,placed_at) VALUES(?1,?2,?3,1,?4,?5)",
    )
    .bind(&pid).bind(&id).bind(&body.booth_id).bind(auth.id()).bind(now())
    .execute(&mut *tx).await?;
    log_event(&mut tx, &id, "TRANSFER", Some(auth.id()), None, Some(&body.booth_id),
              Some(&format!("{old_booth} → 转移"))).await?;
    sqlx::query("UPDATE exhibits SET last_handler_id=?1 WHERE id=?2")
        .bind(auth.id()).bind(&id)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"placement_id": pid, "from": old_booth})))
}

pub async fn retract_placement(
    State(s): State<AppState>,
    auth: Auth,
    Path(pid): Path<String>,
) -> ApiResult<Json<Value>> {
    auth.require_role(&["venue"])?;
    let mut tx = s.pool.begin().await?;
    let row: Option<(String,)> =
        sqlx::query_as("SELECT exhibit_id FROM placements WHERE id=?1 AND active=1")
            .bind(&pid)
            .fetch_optional(&mut *tx)
            .await?;
    let eid = row.ok_or_else(|| ApiError::not_found("生效中的占位不存在"))?.0;
    sqlx::query("UPDATE placements SET active=0, removed_at=?1, removed_by=?2 WHERE id=?3")
        .bind(now()).bind(auth.id()).bind(&pid)
        .execute(&mut *tx).await?;
    sqlx::query("UPDATE exhibits SET status='RECEIVED', last_handler_id=?1 WHERE id=?2")
        .bind(auth.id()).bind(&eid)
        .execute(&mut *tx).await?;
    log_event(&mut tx, &eid, "RETRACT", Some(auth.id()), None, None, Some("展位撤下")).await?;
    tx.commit().await?;
    Ok(Json(json!({"retracted": pid})))
}

// ---------------------------------------------------------------------------
// 异常（缺件/破损）+ 现场照片
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExceptionBody {
    kind: String,
    note: Option<String>,
}

pub async fn open_exception(
    State(s): State<AppState>,
    auth: Auth,
    Path(ident): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<ExceptionBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    if body.kind != "missing" && body.kind != "damaged" {
        return Err(ApiError::bad_request("异常类型只能是 missing 或 damaged"));
    }
    let ex = find_exhibit(&s.pool, &ident, q.get("exhibitor_id").map(|x| x.as_str())).await?;
    let eid = ex["id"].as_str().unwrap();

    // 缺件：展品应尚未出馆
    if ex["status"] == "OUT" {
        return Err(ApiError::bad_request("展品已出馆，不能再登记缺件/破损"));
    }
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM exceptions WHERE exhibit_id=?1 AND kind=?2 AND status='open'",
    )
    .bind(eid).bind(&body.kind)
    .fetch_optional(&s.pool)
    .await?;
    if existing.is_some() {
        return Err(ApiError::conflict("该异常已登记且尚未处理"));
    }

    let id = Uuid::new_v4().to_string();
    let mut tx = s.pool.begin().await?;
    sqlx::query(
        "INSERT INTO exceptions(id,exhibit_id,kind,note,opened_by,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
    )
    .bind(&id).bind(eid).bind(&body.kind).bind(body.note.as_deref())
    .bind(auth.id()).bind(now())
    .execute(&mut *tx).await?;
    log_event(&mut tx, eid, "EXCEPTION", Some(auth.id()), None, None,
              Some(&format!("登记{}: {}", if body.kind=="missing" {"缺件"} else {"破损"},
                            body.note.clone().unwrap_or_default()))).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({"exception_id": id, "kind": body.kind}))))
}

pub async fn list_exceptions(State(s): State<AppState>, _auth: Auth) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String, String, String, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT x.id,e.exhibitor_id,e.code,e.name,x.kind,x.status,COALESCE(x.note,''),u.name \
             FROM exceptions x JOIN exhibits e ON e.id=x.exhibit_id JOIN users u ON u.id=x.opened_by \
             ORDER BY x.created_at DESC",
        )
        .fetch_all(&s.pool)
        .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(id, eid, code, name, kind, status, note, by)| json!({
            "id": id, "exhibitor_id": eid, "exhibit_code": code, "exhibit_name": name,
            "kind": kind, "status": status, "note": note, "opened_by": by,
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct ResolveBody {
    status: String, // resolved_found | written_off
    note: Option<String>,
}

/// 异常放行/核销必须由场馆经理处理——参展商无权自行批准异常放行。
pub async fn resolve_exception(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<ResolveBody>,
) -> ApiResult<Json<Value>> {
    auth.require_role(&["manager"])?;
    if body.status != "resolved_found" && body.status != "written_off" {
        return Err(ApiError::bad_request("处理结果只能是 resolved_found 或 written_off"));
    }
    let mut tx = s.pool.begin().await?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT exhibit_id,status FROM exceptions WHERE id=?1")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?;
    let (eid, st) = row.ok_or_else(|| ApiError::not_found("异常不存在"))?;
    if st != "open" {
        return Err(ApiError::conflict("异常已处理，不能重复处理"));
    }
    sqlx::query(
        "UPDATE exceptions SET status=?1,resolved_by=?2,resolved_at=?3, \
         note=CASE WHEN ?4 IS NULL THEN note ELSE note || ' | 处理: ' || ?4 END WHERE id=?5",
    )
    .bind(&body.status).bind(auth.id()).bind(now()).bind(body.note.as_deref()).bind(&id)
    .execute(&mut *tx).await?;
    log_event(&mut tx, &eid, "RESOLVED", Some(auth.id()), None, None,
              Some(&format!("异常处理结果：{}", body.status))).await?;
    tx.commit().await?;
    Ok(Json(json!({"resolved": id, "result": body.status})))
}

pub async fn upload_photo(
    State(s): State<AppState>,
    auth: Auth,
    Path(ident): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let ex = find_exhibit(&s.pool, &ident, q.get("exhibitor_id").map(|x| x.as_str())).await?;
    let eid = ex["id"].as_str().unwrap().to_string();

    let mut exception_id: Option<String> = None;
    let mut saved: Vec<Value> = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "exception_id" {
            exception_id = field.text().await.ok();
            continue;
        }
        if name != "photo" {
            continue;
        }
        let ct = field.content_type().unwrap_or("application/octet-stream").to_string();
        let ext = match ct.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => "bin",
        };
        let bytes = field.bytes().await.map_err(|e| ApiError::bad_request(e.to_string()))?;
        if bytes.is_empty() {
            return Err(ApiError::bad_request("照片内容为空"));
        }
        let fname = format!("{}.{}", Uuid::new_v4(), ext);
        let fpath = s.upload_dir.join(&fname);
        tokio::fs::write(&fpath, &bytes).await.map_err(|e| ApiError::Status(
            StatusCode::INTERNAL_SERVER_ERROR, format!("写文件失败: {e}")))?;

        let id = Uuid::new_v4().to_string();
        let rel = format!("uploads/{}", fname);
        sqlx::query(
            "INSERT INTO photos(id,fs_path,content_type,exhibit_id,exception_id,uploaded_by,created_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
        )
        .bind(&id).bind(&rel).bind(ct).bind(&eid)
        .bind(exception_id.as_deref()).bind(auth.id()).bind(now())
        .execute(&s.pool).await?;
        saved.push(json!({"id": id, "url": format!("/uploads/{fname}")}));
    }
    if saved.is_empty() {
        return Err(ApiError::bad_request("未找到 photo 字段"));
    }
    Ok((StatusCode::CREATED, Json(json!({"photos": saved}))))
}

// ---------------------------------------------------------------------------
// 变更申请：临时替换 / 转借展位 / 提前撤展（经理审批后生效，入馆记录不动）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ChangeBody {
    kind: String,
    exhibitor_id: String,
    payload: Value,
}

pub async fn request_change(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<ChangeBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    auth.require_role(&["exhibitor"])?;
    ensure_exhibitor_scope(&s.pool, &auth, &body.exhibitor_id).await?;
    if !["replace_exhibit", "loan_booth", "early_out"].contains(&body.kind.as_str()) {
        return Err(ApiError::bad_request("变更类型非法"));
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO change_requests(id,kind,exhibitor_id,payload,status,requested_by,created_at) \
         VALUES(?1,?2,?3,?4,'pending',?5,?6)",
    )
    .bind(&id).bind(&body.kind).bind(&body.exhibitor_id)
    .bind(body.payload.to_string()).bind(auth.id()).bind(now())
    .execute(&s.pool).await?;
    Ok((StatusCode::CREATED, Json(json!({"change_id": id, "status": "pending"}))))
}

pub async fn list_changes(State(s): State<AppState>, _auth: Auth) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String, String, String, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT c.id,c.kind,c.exhibitor_id,c.payload,c.status,u.name,c.created_at,COALESCE(c.review_note,'') \
             FROM change_requests c JOIN users u ON u.id=c.requested_by ORDER BY c.created_at DESC",
        )
        .fetch_all(&s.pool)
        .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(id, kind, eid, payload, status, by, at, note)| json!({
            "id": id, "kind": kind, "exhibitor_id": eid,
            "payload": serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null),
            "status": status, "requested_by": by, "created_at": at, "review_note": note,
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct ReviewBody {
    approve: bool,
    note: Option<String>,
}

pub async fn review_change(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<ReviewBody>,
) -> ApiResult<Json<Value>> {
    auth.require_role(&["manager"])?;
    let mut tx = s.pool.begin().await?;
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT kind,exhibitor_id,payload,status FROM change_requests WHERE id=?1",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?;
    let (kind, eid, payload_str, status) =
        row.ok_or_else(|| ApiError::not_found("变更申请不存在"))?;
    if status != "pending" {
        return Err(ApiError::conflict("该申请已审批，不能重复审批"));
    }
    let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);

    if !body.approve {
        sqlx::query("UPDATE change_requests SET status='rejected',reviewed_by=?1,reviewed_at=?2,review_note=?3 WHERE id=?4")
            .bind(auth.id()).bind(now()).bind(body.note.as_deref()).bind(&id)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(Json(json!({"id": id, "status": "rejected"})));
    }

    match kind.as_str() {
        "replace_exhibit" => {
            // 旧展品失效留痕（原入馆/箱单/流转记录一律保留），新版本清单 + 新展品重新走入馆
            let old_code = payload["old_exhibit_code"].as_str()
                .ok_or_else(|| ApiError::bad_request("payload.old_exhibit_code 必填"))?;
            let new_code = payload["new_code"].as_str()
                .ok_or_else(|| ApiError::bad_request("payload.new_code 必填"))?;
            let new_name = payload["new_name"].as_str().unwrap_or(new_code);

            let old: Option<(String, String)> = sqlx::query_as(
                "SELECT id,manifest_id FROM exhibits WHERE exhibitor_id=?1 AND code=?2 AND active=1",
            )
            .bind(&eid).bind(old_code)
            .fetch_optional(&mut *tx).await?;
            let (old_id, old_manifest) =
                old.ok_or_else(|| ApiError::bad_request("被替换展品不在有效清单中"))?;

            let (prev_v,): (i64,) = sqlx::query_as(
                "SELECT COALESCE(MAX(version),0) FROM manifests WHERE exhibitor_id=?1",
            )
            .bind(&eid).fetch_one(&mut *tx).await?;
            let new_manifest = Uuid::new_v4().to_string();
            let active_count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1",
            )
            .bind(&eid).fetch_one(&mut *tx).await?;
            sqlx::query("UPDATE manifests SET status='superseded' WHERE id=?1")
                .bind(&old_manifest).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO manifests(id,exhibitor_id,version,title,status,change_kind,change_id,declared_count,created_at) \
                 VALUES(?1,?2,?3,'临时替换后的清单','submitted','replace_exhibit',?4,?5,?6)",
            )
            .bind(&new_manifest).bind(&eid).bind(prev_v + 1).bind(&id)
            .bind(active_count.0 /* 替换前有效件数，下面旧-1新+1，总数不变 */)
            .bind(now())
            .execute(&mut *tx).await?;

            let new_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO exhibits(id,exhibitor_id,manifest_id,code,name,status,created_at) \
                 VALUES(?1,?2,?3,?4,?5,'DECLARED',?6)",
            )
            .bind(&new_id).bind(&eid).bind(&new_manifest).bind(new_code).bind(new_name).bind(now())
            .execute(&mut *tx).await?;
            // 旧件若仍占位，先释放唯一占位
            sqlx::query("UPDATE placements SET active=0, removed_at=?1, removed_by=?2 \
                         WHERE exhibit_id=?3 AND active=1")
                .bind(now()).bind(auth.id()).bind(&old_id)
                .execute(&mut *tx).await?;
            sqlx::query("UPDATE exhibits SET active=0, status='REPLACED', replaced_by_id=?1, last_handler_id=?2 WHERE id=?3")
                .bind(&new_id).bind(auth.id()).bind(&old_id)
                .execute(&mut *tx).await?;
            log_event(&mut tx, &old_id, "REPLACE", Some(auth.id()), None, None,
                      Some(&format!("临时替换为 {new_code}（原入馆记录保留）"))).await?;
            log_event(&mut tx, &new_id, "DECLARED", Some(auth.id()), None, None,
                      Some(&format!("替换 {old_code} 的新展品，待入馆核对"))).await?;
        }
        "loan_booth" => {
            let booth_id = payload["booth_id"].as_str()
                .ok_or_else(|| ApiError::bad_request("payload.booth_id 必填"))?;
            let borrower = payload["borrower_exhibitor_id"].as_str()
                .ok_or_else(|| ApiError::bad_request("payload.borrower_exhibitor_id 必填"))?;
            let note = payload["note"].as_str();
            let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM exhibitors WHERE id=?1")
                .bind(borrower).fetch_optional(&mut *tx).await?;
            if exists.is_none() {
                return Err(ApiError::bad_request("借用方参展商不存在"));
            }
            sqlx::query("UPDATE booths SET borrower_exhibitor_id=?1, loan_note=?2 WHERE id=?3")
                .bind(borrower).bind(note).bind(booth_id)
                .execute(&mut *tx).await?;
        }
        "early_out" => {
            // 仅登记审批结果；实际出馆仍由闸口逐件扫码/checkout，事件留 EARLY_OUT
            let codes = payload["exhibit_codes"].as_array()
                .ok_or_else(|| ApiError::bad_request("payload.exhibit_codes 必填"))?;
            for c in codes {
                let code = c.as_str().unwrap_or("");
                let row: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM exhibits WHERE exhibitor_id=?1 AND code=?2 AND active=1",
                )
                .bind(&eid).bind(code).fetch_optional(&mut *tx).await?;
                let ex_id = row.ok_or_else(|| ApiError::bad_request(format!("展品 {code} 不在有效清单")))?.0;
                log_event(&mut tx, &ex_id, "EARLY_OUT", Some(auth.id()), None, None,
                          Some("批准提前撤展，待闸口核验出馆")).await?;
            }
        }
        _ => unreachable!(),
    }

    sqlx::query("UPDATE change_requests SET status='approved',reviewed_by=?1,reviewed_at=?2,review_note=?3 WHERE id=?4")
        .bind(auth.id()).bind(now()).bind(body.note.as_deref()).bind(&id)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"id": id, "status": "approved"})))
}

// ---------------------------------------------------------------------------
// 撤展逐件核实 + 出馆
// ---------------------------------------------------------------------------

/// 单件核验出馆（闸口执行）。存在未处理异常时禁止出馆。
pub async fn checkout_exhibit(
    State(s): State<AppState>,
    auth: Auth,
    Path(ident): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    auth.require_role(&["gatekeeper"])?;
    let ex = find_exhibit(&s.pool, &ident, q.get("exhibitor_id").map(|x| x.as_str())).await?;
    let id = ex["id"].as_str().unwrap().to_string();
    let status = ex["status"].as_str().unwrap();

    if !matches!(status, "RECEIVED" | "PLACED") {
        return Err(ApiError::conflict(format!("展品当前状态 {status}，无法核验出馆")));
    }
    let open: Option<(String, String)> = sqlx::query_as(
        "SELECT id,kind FROM exceptions WHERE exhibit_id=?1 AND status='open'",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await?;
    if let Some((_, kind)) = open {
        return Err(ApiError::conflict(format!(
            "该展品存在未处理的{}异常，须由场馆经理处理后才能放行",
            if kind == "missing" { "缺件" } else { "破损" }
        )));
    }

    let mut tx = s.pool.begin().await?;
    sqlx::query("UPDATE placements SET active=0, removed_at=?1, removed_by=?2 WHERE exhibit_id=?3 AND active=1")
        .bind(now()).bind(auth.id()).bind(&id)
        .execute(&mut *tx).await?;
    sqlx::query("UPDATE exhibits SET status='OUT', last_handler_id=?1 WHERE id=?2")
        .bind(auth.id()).bind(&id)
        .execute(&mut *tx).await?;
    log_event(&mut tx, &id, "CHECK_OUT", Some(auth.id()), None, None, Some("撤展核验实物，准予出馆")).await?;
    tx.commit().await?;
    Ok(Json(json!({"checked_out": id})))
}

#[derive(Deserialize)]
pub struct VerifyBody {
    items: Vec<VerifyItem>,
}
#[derive(Deserialize)]
pub struct VerifyItem {
    code: String,
    result: String, // out | missing | damaged
    note: Option<String>,
}

/// 按原清单逐件核实：实物在→出馆；缺/损→登记异常（附说明，照片另行上传）。
pub async fn verify_exhibitor(
    State(s): State<AppState>,
    auth: Auth,
    Path(eid): Path<String>,
    Json(body): Json<VerifyBody>,
) -> ApiResult<Json<Value>> {
    auth.require_role(&["gatekeeper", "venue"])?;
    let mut checked_out = Vec::new();
    let mut exceptions = Vec::new();

    for item in &body.items {
        let ex = find_exhibit(&s.pool, &item.code, Some(&eid)).await?;
        let id = ex["id"].as_str().unwrap().to_string();
        match item.result.as_str() {
            "out" => {
                if !matches!(ex["status"].as_str().unwrap(), "RECEIVED" | "PLACED") {
                    return Err(ApiError::conflict(format!(
                        "{} 当前状态 {}，无法标记实物出馆", item.code, ex["status"]
                    )));
                }
                let mut tx = s.pool.begin().await?;
                sqlx::query("UPDATE placements SET active=0, removed_at=?1, removed_by=?2 WHERE exhibit_id=?3 AND active=1")
                    .bind(now()).bind(auth.id()).bind(&id)
                    .execute(&mut *tx).await?;
                sqlx::query("UPDATE exhibits SET status='OUT', last_handler_id=?1 WHERE id=?2")
                    .bind(auth.id()).bind(&id)
                    .execute(&mut *tx).await?;
                log_event(&mut tx, &id, "CHECK_OUT", Some(auth.id()), None, None,
                          Some("撤展批量核验：实物相符")).await?;
                tx.commit().await?;
                checked_out.push(item.code.clone());
            }
            "missing" | "damaged" => {
                let exists: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM exceptions WHERE exhibit_id=?1 AND kind=?2 AND status='open'",
                )
                .bind(&id).bind(&item.result)
                .fetch_optional(&s.pool).await?;
                if exists.is_none() {
                    let xid = Uuid::new_v4().to_string();
                    let mut tx = s.pool.begin().await?;
                    sqlx::query(
                        "INSERT INTO exceptions(id,exhibit_id,kind,note,opened_by,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
                    )
                    .bind(&xid).bind(&id).bind(&item.result).bind(&item.note.as_deref())
                    .bind(auth.id()).bind(now())
                    .execute(&mut *tx).await?;
                    log_event(&mut tx, &id, "EXCEPTION", Some(auth.id()), None, None,
                              Some(&format!("撤展核验登记{}", if item.result=="missing" {"缺件"} else {"破损"}))).await?;
                    tx.commit().await?;
                    exceptions.push(json!({"code": item.code, "exception_id": xid, "kind": item.result}));
                }
            }
            other => return Err(ApiError::bad_request(format!("核验结果非法: {other}"))),
        }
    }
    Ok(Json(json!({"checked_out": checked_out, "exceptions": exceptions})))
}

// ---------------------------------------------------------------------------
// 出馆放行凭证（仅场馆经理可签发；数量必须相符）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PassBody {
    exhibitor_id: String,
}

async fn reconciliation(pool: &SqlitePool, eid: &str) -> ApiResult<Value> {
    let q = |sql: &'static str| async move {
        sqlx::query_as::<_, (i64,)>(sql).bind(eid).fetch_one(pool).await.map(|r| r.0)
    };
    let declared = q("SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1").await?;
    let received = q(
        "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1 \
         AND status IN ('RECEIVED','PLACED','OUT')",
    )
    .await?;
    let out = q("SELECT COUNT(*) FROM exhibits WHERE exhibitor_id=?1 AND active=1 AND status='OUT'").await?;
    let written_off = q(
        "SELECT COUNT(DISTINCT exhibit_id) FROM exceptions WHERE status='written_off' \
         AND exhibit_id IN (SELECT id FROM exhibits WHERE exhibitor_id=?1 AND active=1)",
    )
    .await?;
    let open_missing = q(
        "SELECT COUNT(*) FROM exceptions x JOIN exhibits e ON e.id=x.exhibit_id \
         WHERE e.exhibitor_id=?1 AND e.active=1 AND x.status='open' AND x.kind='missing'",
    )
    .await?;
    let open_damaged = q(
        "SELECT COUNT(*) FROM exceptions x JOIN exhibits e ON e.id=x.exhibit_id \
         WHERE e.exhibitor_id=?1 AND e.active=1 AND x.status='open' AND x.kind='damaged'",
    )
    .await?;
    Ok(json!({
        "declared": declared, "received": received, "out": out,
        "written_off": written_off,
        "accounted": out + written_off,
        "open_missing": open_missing, "open_damaged": open_damaged,
    }))
}

pub async fn issue_pass(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<PassBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    // 只有场馆经理能签发放行凭证——参展商/闸口都无权批准异常放行
    auth.require_role(&["manager"])?;

    let counts = reconciliation(&s.pool, &body.exhibitor_id).await?;
    let declared = counts["declared"].as_i64().unwrap();
    let received = counts["received"].as_i64().unwrap();
    let accounted = counts["accounted"].as_i64().unwrap();
    let open_missing = counts["open_missing"].as_i64().unwrap();
    let open_damaged = counts["open_damaged"].as_i64().unwrap();

    let mut problems = Vec::new();
    if declared == 0 {
        problems.push("尚未提交清单".to_string());
    }
    if declared != received {
        problems.push(format!("入馆数量不符：申报 {declared}，实际入馆 {received}"));
    }
    if declared != accounted {
        problems.push(format!(
            "去向数量不符：申报 {declared}，已出馆+核销 {accounted}（缺件须由经理核销后才能平账）"
        ));
    }
    if open_missing > 0 || open_damaged > 0 {
        problems.push(format!(
            "尚有未处理异常：缺件 {open_missing} 件、破损 {open_damaged} 件"
        ));
    }
    if !problems.is_empty() {
        return Err(ApiError::conflict(format!("数量核对未通过：{}", problems.join("；"))));
    }

    let mut tx = s.pool.begin().await?;
    let code = format!("PASS-{}", &Uuid::new_v4().simple().to_string()[..8].to_uppercase());
    let pass_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO passes(id,code,exhibitor_id,declared_count,received_count,out_count,\
         missing_count,damaged_count,approved_by,created_at) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
    )
    .bind(&pass_id).bind(&code).bind(&body.exhibitor_id)
    .bind(declared).bind(received).bind(counts["out"].as_i64().unwrap())
    .bind(0i64).bind(0i64)
    .bind(auth.id()).bind(now())
    .execute(&mut *tx).await?;

    let items: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT e.id,e.code, CASE WHEN EXISTS(SELECT 1 FROM exceptions x WHERE x.exhibit_id=e.id AND x.status='written_off') \
         THEN 'written_off' ELSE e.status END FROM exhibits e WHERE e.exhibitor_id=?1 AND e.active=1",
    )
    .bind(&body.exhibitor_id)
    .fetch_all(&mut *tx).await?;
    for (ex_id, _, state) in &items {
        sqlx::query("INSERT INTO pass_items(pass_id,exhibit_id,state) VALUES(?1,?2,?3)")
            .bind(&pass_id).bind(ex_id).bind(state)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(json!({
        "pass_id": pass_id, "code": code,
        "counts": counts,
        "items": items.into_iter().map(|(_, c, st)| json!({"code": c, "state": st})).collect::<Vec<_>>(),
        "approved_by": auth.0.name,
    }))))
}

pub async fn list_passes(
    State(s): State<AppState>,
    _auth: Auth,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String, String, i64, i64, i64, i64, i64, String, String)> = sqlx::query_as(
        "SELECT p.id,p.code,p.exhibitor_id,p.declared_count,p.received_count,p.out_count,\
         p.missing_count,p.damaged_count,u.name,p.created_at FROM passes p \
         JOIN users u ON u.id=p.approved_by \
         WHERE (?1 IS NULL OR p.exhibitor_id=?1) ORDER BY p.created_at DESC",
    )
    .bind(q.get("exhibitor_id"))
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(id, code, eid, d, r, o, m, dm, by, at)| json!({
            "id": id, "code": code, "exhibitor_id": eid,
            "declared": d, "received": r, "out": o, "missing": m, "damaged": dm,
            "approved_by": by, "created_at": at,
        }))
        .collect::<Vec<_>>())))
}

// ---------------------------------------------------------------------------
// 闸口扫码（模拟）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ScanBody {
    code: String,
}

pub async fn gate_scan(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<ScanBody>,
) -> ApiResult<Json<Value>> {
    auth.require_role(&["gatekeeper"])?;
    let code = body.code.trim();
    let (result, detail): (&str, Value) = if let Some((pid, eid, d, r, o)) = sqlx::query_as::<_, (String, String, i64, i64, i64)>(
        "SELECT id,exhibitor_id,declared_count,received_count,out_count FROM passes WHERE code=?1",
    )
    .bind(code)
    .fetch_optional(&s.pool)
    .await?
    {
        let prior: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM gate_scans WHERE code=?1 AND result='allow'")
            .bind(code).fetch_one(&s.pool).await?;
        if prior.0 > 0 {
            ("deny", json!({"reason": "该放行凭证已扫码使用过", "pass_id": pid}))
        } else {
            ("allow", json!({
                "kind": "exit_pass", "pass_id": pid, "exhibitor_id": eid,
                "declared": d, "received": r, "out": o,
                "message": "凭证有效，闸口放行"
            }))
        }
    } else if let Some((cid, note)) = sqlx::query_as::<_, (String, String)>(
        "SELECT id, COALESCE(note,'') FROM crates WHERE code=?1",
    )
    .bind(code)
    .fetch_optional(&s.pool)
    .await?
    {
        let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM crate_items WHERE crate_id=?1")
            .bind(&cid).fetch_one(&s.pool).await?;
        ("allow", json!({"kind": "crate", "crate_id": cid, "items": n.0,
                         "message": "入馆箱单（已封箱接收）", "note": note}))
    } else if let Some((eid, ename, estate, eexh)) = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT id,name,status,exhibitor_id FROM exhibits WHERE code=?1 AND active=1 LIMIT 1",
    )
    .bind(code)
    .fetch_optional(&s.pool)
    .await?
    {
        if estate == "OUT" {
            let in_pass: Option<(String,)> = sqlx::query_as(
                "SELECT pi.pass_id FROM pass_items pi WHERE pi.exhibit_id=?1",
            )
            .bind(&eid).fetch_optional(&s.pool).await?;
            let early: Option<(String,)> = sqlx::query_as(
                "SELECT c.id FROM change_requests c WHERE c.kind='early_out' AND c.status='approved' \
                 AND c.exhibitor_id=?2 AND json_extract(c.payload,'$.exhibit_codes') LIKE ?1",
            )
            .bind(format!("%\"{code}\"%")).bind(&eexh)
            .fetch_optional(&s.pool).await?;
            if in_pass.is_some() || early.is_some() {
                ("allow", json!({"kind": "exhibit", "exhibit": ename,
                                 "basis": if in_pass.is_some() {"放行凭证"} else {"提前撤展审批"},
                                 "message": "单件核验通过，放行"}))
            } else {
                ("deny", json!({"kind": "exhibit", "exhibit": ename,
                                "reason": "展品虽已出馆但无放行凭证/提前撤展审批"}))
            }
        } else {
            ("deny", json!({"kind": "exhibit", "exhibit": ename,
                            "reason": format!("展品当前状态为 {estate}，不满足出馆条件")}))
        }
    } else {
        ("deny", json!({"reason": "未识别的条码（不是箱号/展品编码/放行凭证）"}))
    };

    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO gate_scans(id,code,result,detail,scanner_id,created_at) VALUES(?1,?2,?3,?4,?5,?6)")
        .bind(&id).bind(code).bind(result).bind(detail.to_string())
        .bind(auth.id()).bind(now())
        .execute(&s.pool).await?;
    Ok(Json(json!({"result": result, "detail": detail})))
}

pub async fn list_scans(State(s): State<AppState>, _auth: Auth) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT code,result,COALESCE(detail,''),u.name,created_at FROM gate_scans g \
         JOIN users u ON u.id=g.scanner_id ORDER BY g.created_at DESC LIMIT 100",
    )
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(code, result, detail, by, at)| json!({
            "code": code, "result": result,
            "detail": serde_json::from_str::<Value>(&detail).unwrap_or(Value::Null),
            "scanner": by, "at": at,
        }))
        .collect::<Vec<_>>())))
}
