use axum::{extract::{Path, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{
    auth::{require_role, AuthUser},
    error::{is_unique_violation, ApiError},
    models::*,
    util, AppState,
};

#[derive(Deserialize)]
pub struct NewItem {
    pub name: String,
    pub description: Option<String>,
    pub serial_no: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateManifest {
    pub items: Vec<NewItem>,
}

/// 参展商提前提交逐件清单（每个参展商一份，提交后只能通过变更申请调整）
pub async fn create_manifest(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateManifest>,
) -> Result<Json<Value>, ApiError> {
    require_role(&user, &["exhibitor"])?;
    let exhibitor_id = user
        .exhibitor_id
        .ok_or_else(|| ApiError::Forbidden("该账号未关联参展商".into()))?;
    if body.items.is_empty() {
        return Err(ApiError::BadRequest("清单至少需要一件展品".into()));
    }

    let mut tx = st.pool.begin().await?;
    let manifest_id = match sqlx::query("INSERT INTO manifests (exhibitor_id) VALUES (?)")
        .bind(exhibitor_id)
        .execute(&mut *tx)
        .await
    {
        Ok(r) => r.last_insert_rowid(),
        Err(e) if is_unique_violation(&e) => {
            return Err(ApiError::Conflict(
                "该参展商已提交过清单；临时替换/提前撤展等请走变更申请，不能改写入馆记录".into(),
            ))
        }
        Err(e) => return Err(e.into()),
    };

    let mut item_ids = Vec::new();
    for it in &body.items {
        let name = it.name.trim();
        if name.is_empty() {
            return Err(ApiError::BadRequest("展品名称不能为空".into()));
        }
        let r = sqlx::query(
            "INSERT INTO items (manifest_id, exhibitor_id, name, description, serial_no)
             VALUES (?,?,?,?,?)",
        )
        .bind(manifest_id)
        .bind(exhibitor_id)
        .bind(name)
        .bind(it.description.clone().unwrap_or_default())
        .bind(it.serial_no.clone().unwrap_or_default())
        .execute(&mut *tx)
        .await?;
        let item_id = r.last_insert_rowid();
        util::log_event(&mut tx, Some(item_id), "item_declared", user.id, None, None, None,
            &format!("清单登记：{name}")).await?;
        item_ids.push(item_id);
    }
    tx.commit().await?;
    Ok(Json(json!({ "manifest_id": manifest_id, "item_ids": item_ids })))
}

/// 清单列表：参展商只能看自己的
pub async fn list_manifests(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Value>, ApiError> {
    let rows = if user.role == "exhibitor" {
        sqlx::query_as::<_, Manifest>(
            "SELECT * FROM manifests WHERE exhibitor_id = ? ORDER BY id")
            .bind(user.exhibitor_id.unwrap_or(-1))
            .fetch_all(&st.pool).await?
    } else {
        sqlx::query_as::<_, Manifest>("SELECT * FROM manifests ORDER BY id")
            .fetch_all(&st.pool).await?
    };
    let mut out = Vec::new();
    for m in rows {
        let exhibitor = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
            .bind(m.exhibitor_id).fetch_one(&st.pool).await?;
        let items = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE manifest_id = ? ORDER BY id")
            .bind(m.id).fetch_all(&st.pool).await?;
        out.push(json!({ "manifest": m, "exhibitor": exhibitor, "items": items }));
    }
    Ok(Json(json!(out)))
}

pub async fn get_manifest(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let m = sqlx::query_as::<_, Manifest>("SELECT * FROM manifests WHERE id = ?")
        .bind(id).fetch_optional(&st.pool).await?
        .ok_or_else(|| ApiError::NotFound("清单不存在".into()))?;
    if user.role == "exhibitor" && user.exhibitor_id != Some(m.exhibitor_id) {
        return Err(ApiError::Forbidden("只能查看本展商的清单".into()));
    }
    let exhibitor = sqlx::query_as::<_, Exhibitor>("SELECT * FROM exhibitors WHERE id = ?")
        .bind(m.exhibitor_id).fetch_one(&st.pool).await?;
    let items = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE manifest_id = ? ORDER BY id")
        .bind(id).fetch_all(&st.pool).await?;
    Ok(Json(json!({ "manifest": m, "exhibitor": exhibitor, "items": items })))
}
