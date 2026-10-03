use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{auth::AuthUser, error::ApiError, models::*, AppState};

pub async fn list_users(
    State(st): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let users = sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY id")
        .fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for u in users {
        let exhibitor = match u.exhibitor_id {
            Some(e) => Some(sqlx::query_scalar::<_, String>("SELECT name FROM exhibitors WHERE id = ?")
                .bind(e).fetch_one(&st.pool).await?),
            None => None,
        };
        out.push(json!({ "user": u, "exhibitor_name": exhibitor }));
    }
    Ok(Json(json!(out)))
}

pub async fn list_exhibitors(
    State(st): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors ORDER BY id")
        .fetch_all(&st.pool).await?;
    Ok(Json(json!(rows)))
}

pub async fn list_booths(
    State(st): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let booths = sqlx::query_as::<_, Booth>("SELECT * FROM booths ORDER BY hall, code")
        .fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for b in booths {
        let owner = match b.owner_exhibitor_id {
            Some(e) => Some(sqlx::query_scalar::<_, String>("SELECT name FROM exhibitors WHERE id = ?")
                .bind(e).fetch_one(&st.pool).await?),
            None => None,
        };
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM placements WHERE booth_id = ? AND removed_at IS NULL")
            .bind(b.id).fetch_one(&st.pool).await?;
        out.push(json!({ "booth": b, "owner_name": owner, "item_count": count }));
    }
    Ok(Json(json!(out)))
}

/// 展位看板：各展位当前展品、未占位在库展品、各展商数量核对
pub async fn board(
    State(st): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Result<Json<Value>, ApiError> {
    let booths = sqlx::query_as::<_, Booth>("SELECT * FROM booths ORDER BY hall, code")
        .fetch_all(&st.pool).await?;
    let mut booth_out = Vec::new();
    for b in booths {
        let owner = match b.owner_exhibitor_id {
            Some(e) => Some(sqlx::query_scalar::<_, String>("SELECT name FROM exhibitors WHERE id = ?")
                .bind(e).fetch_one(&st.pool).await?),
            None => None,
        };
        let items = sqlx::query(
            "SELECT i.id, i.name, i.serial_no, i.status, e.name AS exhibitor_name,
                    (SELECT COUNT(*) FROM exceptions x WHERE x.item_id = i.id AND x.status = 'open') AS open_exceptions
             FROM placements p
             JOIN items i ON i.id = p.item_id
             JOIN exhibitors e ON e.id = i.exhibitor_id
             WHERE p.booth_id = ? AND p.removed_at IS NULL
             ORDER BY i.id")
            .bind(b.id)
            .fetch_all(&st.pool).await?;
        let items: Vec<Value> = items.iter().map(|r| {
            use sqlx::Row;
            json!({
                "id": r.get::<i64, _>("id"),
                "name": r.get::<String, _>("name"),
                "serial_no": r.get::<String, _>("serial_no"),
                "status": r.get::<String, _>("status"),
                "exhibitor_name": r.get::<String, _>("exhibitor_name"),
                "open_exceptions": r.get::<i64, _>("open_exceptions"),
            })
        }).collect();
        booth_out.push(json!({ "booth": b, "owner_name": owner, "items": items }));
    }

    let unplaced = sqlx::query(
        "SELECT i.id, i.name, i.serial_no, e.name AS exhibitor_name,
                (SELECT b.box_code FROM box_items bi JOIN boxes b ON b.id = bi.box_id WHERE bi.item_id = i.id) AS box_code
         FROM items i JOIN exhibitors e ON e.id = i.exhibitor_id
         WHERE i.status = 'received' ORDER BY i.id")
        .fetch_all(&st.pool).await?;
    let unplaced: Vec<Value> = unplaced.iter().map(|r| {
        use sqlx::Row;
        json!({
            "id": r.get::<i64, _>("id"),
            "name": r.get::<String, _>("name"),
            "serial_no": r.get::<String, _>("serial_no"),
            "exhibitor_name": r.get::<String, _>("exhibitor_name"),
            "box_code": r.get::<Option<String>, _>("box_code"),
        })
    }).collect();

    let exhibitors = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors ORDER BY id")
        .fetch_all(&st.pool).await?;
    let mut recs = Vec::new();
    for e in exhibitors {
        let rec = reconciliation_of(&st.pool, e.id).await?;
        recs.push(json!({ "exhibitor": e, "reconciliation": rec }));
    }

    Ok(Json(json!({ "booths": booth_out, "unplaced": unplaced, "exhibitors": recs })))
}

/// 数量核对：入馆 = 在场 + 已核实 + 已出馆 + 缺件结案，且无未处理异常
async fn reconciliation_of(pool: &sqlx::SqlitePool, exhibitor_id: i64) -> Result<Value, ApiError> {
    let rows = sqlx::query(
        "SELECT status, COUNT(*) AS c FROM items WHERE exhibitor_id = ? GROUP BY status")
        .bind(exhibitor_id).fetch_all(pool).await?;
    let mut by_status = std::collections::HashMap::new();
    for r in &rows {
        use sqlx::Row;
        by_status.insert(r.get::<String, _>("status"), r.get::<i64, _>("c"));
    }
    let get = |k: &str| *by_status.get(k).unwrap_or(&0);
    let declared_total: i64 = by_status.values().sum();
    let received = declared_total - get("declared");
    let on_site = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM placements p JOIN items i ON i.id = p.item_id
         WHERE i.exhibitor_id = ? AND p.removed_at IS NULL")
        .bind(exhibitor_id).fetch_one(pool).await?;
    let open_exceptions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM exceptions e JOIN items i ON i.id = e.item_id
         WHERE i.exhibitor_id = ? AND e.status = 'open'")
        .bind(exhibitor_id).fetch_one(pool).await?;
    let verified_out = get("verified_out");
    let exited = get("exited");
    let missing_closed = get("missing_closed");
    let accounted = verified_out + exited + missing_closed;
    Ok(json!({
        "declared": declared_total,
        "not_received": get("declared"),
        "received": received,
        "on_site": on_site,
        "verified_out": verified_out,
        "exited": exited,
        "missing_closed": missing_closed,
        "open_exceptions": open_exceptions,
        "accounted_for_exit": accounted,
        "balanced": received == accounted && open_exceptions == 0,
    }))
}

pub async fn reconciliation(
    State(st): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
        .bind(id).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound("参展商不存在".into()))?;
    Ok(Json(reconciliation_of(&st.pool, id).await?))
}

#[derive(Deserialize)]
pub struct ItemQuery {
    pub exhibitor_id: Option<i64>,
    pub status: Option<String>,
}

/// 展品列表（含当前展位、箱号），供收箱/占位/撤展等页面选择
pub async fn list_items(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ItemQuery>,
) -> Result<Json<Value>, ApiError> {
    let mut sql = String::from(
        "SELECT i.*, e.name AS exhibitor_name,
                (SELECT b.code FROM placements p JOIN booths b ON b.id = p.booth_id
                 WHERE p.item_id = i.id AND p.removed_at IS NULL) AS booth_code,
                (SELECT b.box_code FROM box_items bi JOIN boxes b ON b.id = bi.box_id
                 WHERE bi.item_id = i.id) AS box_code,
                (SELECT COUNT(*) FROM exceptions x WHERE x.item_id = i.id AND x.status = 'open') AS open_exceptions
         FROM items i JOIN exhibitors e ON e.id = i.exhibitor_id WHERE 1=1");
    if user.role == "exhibitor" {
        sql.push_str(&format!(" AND i.exhibitor_id = {}", user.exhibitor_id.unwrap_or(-1)));
    } else if let Some(e) = q.exhibitor_id {
        sql.push_str(&format!(" AND i.exhibitor_id = {e}"));
    }
    if let Some(s) = &q.status {
        let safe: String = s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        sql.push_str(&format!(" AND i.status = '{safe}'"));
    }
    sql.push_str(" ORDER BY i.id");
    let rows = sqlx::query(&sql).fetch_all(&st.pool).await?;
    let out: Vec<Value> = rows.iter().map(|r| {
        use sqlx::Row;
        json!({
            "id": r.get::<i64, _>("id"),
            "manifest_id": r.get::<i64, _>("manifest_id"),
            "exhibitor_id": r.get::<i64, _>("exhibitor_id"),
            "exhibitor_name": r.get::<String, _>("exhibitor_name"),
            "name": r.get::<String, _>("name"),
            "description": r.get::<String, _>("description"),
            "serial_no": r.get::<String, _>("serial_no"),
            "status": r.get::<String, _>("status"),
            "origin": r.get::<String, _>("origin"),
            "booth_code": r.get::<Option<String>, _>("booth_code"),
            "box_code": r.get::<Option<String>, _>("box_code"),
            "open_exceptions": r.get::<i64, _>("open_exceptions"),
        })
    }).collect();
    Ok(Json(json!(out)))
}

/// 展品详情 + 完整流转记录；最后一条事件的经手人即“最后经手人”
pub async fn item_detail(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
        .bind(id).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound("展品不存在".into()))?;
    if user.role == "exhibitor" && user.exhibitor_id != Some(item.exhibitor_id) {
        return Err(ApiError::Forbidden("只能查看本展商的展品".into()));
    }
    let exhibitor = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
        .bind(item.exhibitor_id).fetch_one(&st.pool).await?;

    let booth = sqlx::query(
        "SELECT b.code, b.name FROM placements p JOIN booths b ON b.id = p.booth_id
         WHERE p.item_id = ? AND p.removed_at IS NULL")
        .bind(id).fetch_optional(&st.pool).await?;
    let current_booth = booth.map(|r| {
        use sqlx::Row;
        json!({ "code": r.get::<String, _>("code"), "name": r.get::<String, _>("name") })
    });

    let box_code = sqlx::query_scalar::<_, String>(
        "SELECT b.box_code FROM box_items bi JOIN boxes b ON b.id = bi.box_id WHERE bi.item_id = ?")
        .bind(id).fetch_optional(&st.pool).await?;

    let exceptions = sqlx::query_as::<_, ExceptionCase>(
        "SELECT * FROM exceptions WHERE item_id = ? ORDER BY id DESC")
        .bind(id).fetch_all(&st.pool).await?;

    let events = sqlx::query(
        "SELECT e.id, e.event_type, e.note, e.created_at,
                u.name AS actor_name, u.role AS actor_role,
                b.code AS booth_code, x.box_code AS box_code, p.pass_code AS pass_code
         FROM item_events e
         JOIN users u ON u.id = e.actor_id
         LEFT JOIN booths b ON b.id = e.booth_id
         LEFT JOIN boxes x ON x.id = e.box_id
         LEFT JOIN exit_passes p ON p.id = e.pass_id
         WHERE e.item_id = ? ORDER BY e.id")
        .bind(id).fetch_all(&st.pool).await?;
    let events: Vec<Value> = events.iter().map(|r| {
        use sqlx::Row;
        json!({
            "id": r.get::<i64, _>("id"),
            "event_type": r.get::<String, _>("event_type"),
            "note": r.get::<String, _>("note"),
            "created_at": r.get::<String, _>("created_at"),
            "actor_name": r.get::<String, _>("actor_name"),
            "actor_role": r.get::<String, _>("actor_role"),
            "booth_code": r.get::<Option<String>, _>("booth_code"),
            "box_code": r.get::<Option<String>, _>("box_code"),
            "pass_code": r.get::<Option<String>, _>("pass_code"),
        })
    }).collect();
    let last_handler = events.last().map(|e| json!({
        "name": e["actor_name"], "event_type": e["event_type"], "at": e["created_at"],
    }));

    Ok(Json(json!({
        "item": item,
        "exhibitor": exhibitor,
        "current_booth": current_booth,
        "box_code": box_code,
        "exceptions": exceptions,
        "events": events,
        "last_handler": last_handler,
    })))
}
