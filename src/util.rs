use sqlx::{Sqlite, Transaction};

/// 生成放行凭证号，如 EP-3F9A1C2B
pub fn pass_code() -> String {
    let raw = uuid::Uuid::new_v4().simple().to_string();
    format!("EP-{}", raw[..8].to_uppercase())
}

/// 追加一条逐件流转审计事件（在事务内调用）
#[allow(clippy::too_many_arguments)]
pub async fn log_event(
    tx: &mut Transaction<'_, Sqlite>,
    item_id: Option<i64>,
    event_type: &str,
    actor_id: i64,
    booth_id: Option<i64>,
    box_id: Option<i64>,
    pass_id: Option<i64>,
    note: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO item_events (item_id, event_type, actor_id, booth_id, box_id, pass_id, note)
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind(item_id)
    .bind(event_type)
    .bind(actor_id)
    .bind(booth_id)
    .bind(box_id)
    .bind(pass_id)
    .bind(note)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
