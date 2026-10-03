use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub exhibitor_id: Option<i64>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Exhibitor {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Booth {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub hall: String,
    pub owner_exhibitor_id: Option<i64>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Manifest {
    pub id: i64,
    pub exhibitor_id: i64,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Item {
    pub id: i64,
    pub manifest_id: i64,
    pub exhibitor_id: i64,
    pub name: String,
    pub description: String,
    pub serial_no: String,
    pub status: String,
    pub origin: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct BoxRow {
    pub id: i64,
    pub box_code: String,
    pub exhibitor_id: i64,
    pub received_by: i64,
    pub received_at: String,
    pub sealed: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Placement {
    pub id: i64,
    pub item_id: i64,
    pub booth_id: i64,
    pub placed_by: i64,
    pub placed_at: String,
    pub removed_at: Option<String>,
    pub removed_by: Option<i64>,
    pub note: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ExceptionCase {
    pub id: i64,
    pub item_id: i64,
    pub kind: String,
    pub note: String,
    pub photo_path: String,
    pub reported_by: i64,
    pub status: String,
    pub decided_by: Option<i64>,
    pub decided_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Amendment {
    pub id: i64,
    pub exhibitor_id: i64,
    pub kind: String,
    pub payload: String,
    pub status: String,
    pub created_by: i64,
    pub created_at: String,
    pub decided_by: Option<i64>,
    pub decided_at: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ExitPass {
    pub id: i64,
    pub exhibitor_id: i64,
    pub pass_code: String,
    pub kind: String,
    pub amendment_id: Option<i64>,
    pub status: String,
    pub issued_by: i64,
    pub created_at: String,
    pub used_at: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ItemEvent {
    pub id: i64,
    pub item_id: Option<i64>,
    pub event_type: String,
    pub actor_id: i64,
    pub booth_id: Option<i64>,
    pub box_id: Option<i64>,
    pub pass_id: Option<i64>,
    pub note: String,
    pub created_at: String,
}
