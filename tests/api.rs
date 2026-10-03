// 端到端集成测试：基于内存 SQLite 起完整 Axum 服务，覆盖
// 1) 同一件展品不能同时出现在两个展位（重复占位 → 409）
// 2) 入馆/在场/出馆数量核对（不平账不能签发；经理核销后可签发）
// 3) 无权限人员尝试异常放行/签发 → 403
// 另含：按箱接收、变更重新确认、闸口扫码一次性。

use axum::http::StatusCode;
use expo_trace::router;
use expo_trace::AppState;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

mod common;
use common::*;

async fn app() -> (axum::Router, sqlx::SqlitePool) {
    // 每个用例独立内存库
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .unwrap();
    expo_trace::seed(&pool).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = AppState {
        pool: pool.clone(),
        upload_dir: dir.path().to_path_buf(),
    };
    // tempdir 不能提前 drop（照片测试用到），这里故意泄漏目录句柄
    std::mem::forget(dir);
    (router(state), pool)
}

async fn submit_alpha_manifest(app: &axum::Router) {
    let items = serde_json::json!([
        {"code": "A-001", "name": "青花瓷瓶"},
        {"code": "A-002", "name": "玉雕摆件"},
        {"code": "A-003", "name": "丝绸长卷"},
    ]);
    let res = post(app, "/api/manifests", "u-alpha", &json_o("items", items)).await;
    assert_eq!(res.status(), StatusCode::CREATED);
}

fn json_o(key: &str, val: serde_json::Value) -> serde_json::Value {
    let mut full = serde_json::json!({
        "exhibitor_id": "e-alpha",
        "title": "测试清单",
    });
    full[key] = val;
    full
}

async fn receive_all(app: &axum::Router) {
    let body = serde_json::json!({
        "code": "CRATE-A-01", "exhibitor_id": "e-alpha",
        "exhibit_codes": ["A-001", "A-002", "A-003"],
        "note": "整箱",
    });
    let res = post(app, "/api/crates", "u-gate", &body).await;
    assert_eq!(res.status(), StatusCode::CREATED, "按箱接收应成功");
}

async fn find_exhibit_id(pool: &sqlx::SqlitePool, code: &str) -> String {
    sqlx::query_as::<_, (String,)>("SELECT id FROM exhibits WHERE code=?1")
        .bind(code)
        .fetch_one(pool)
        .await
        .unwrap()
        .0
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn manifest_cannot_be_resubmitted_directly() {
    let (app, _) = app().await;
    submit_alpha_manifest(&app).await;
    // 第二次直接重报必须被拒：不能覆盖已入馆流程
    let items = serde_json::json!([{"code": "X", "name": "Y"}]);
    let res = post(&app, "/api/manifests", "u-alpha", &json_o("items", items)).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn crate_cannot_be_received_twice() {
    let (app, _) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;
    // 重复箱号
    let body = serde_json::json!({
        "code": "CRATE-A-01", "exhibitor_id": "e-alpha",
        "exhibit_codes": ["A-001"],
    });
    let res = post(&app, "/api/crates", "u-gate", &body).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    // 已入馆展品再随别的箱接收
    let body = serde_json::json!({
        "code": "CRATE-A-02", "exhibitor_id": "e-alpha",
        "exhibit_codes": ["A-001"],
    });
    let res = post(&app, "/api/crates", "u-gate", &body).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn duplicate_placement_is_rejected_but_transfer_works() {
    let (app, pool) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;
    let id = find_exhibit_id(&pool, "A-001").await;

    // 首次占位 A1 成功
    let res = post(&app, "/api/placements", "u-venue", &serde_json::json!({
        "exhibit_id": id, "booth_id": "b-a1",
    }))
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // 同一件展品再占 A2 —— 必须 409（数据库有部分唯一索引兜底，应用层先拦截）
    let res = post(&app, "/api/placements", "u-venue", &serde_json::json!({
        "exhibit_id": id, "booth_id": "b-a2",
    }))
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // 生效占位仍然只有一个，且在 A1
    let n: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM placements WHERE exhibit_id=?1 AND active=1",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n.0, 1, "同一件展品不能同时出现在两个展位");
    let booth: (String,) = sqlx::query_as(
        "SELECT b.code FROM placements p JOIN booths b ON b.id=p.booth_id WHERE p.exhibit_id=?1 AND p.active=1",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(booth.0, "A1");

    // 转移到 A2：旧占位失效、新生效，仍只有一条
    let res = post(&app, "/api/exhibits/A-001/transfer", "u-venue", &serde_json::json!({
        "booth_id": "b-a2",
    }))
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let n: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM placements WHERE exhibit_id=?1 AND active=1",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n.0, 1);
}

#[tokio::test]
async fn cannot_place_on_booth_not_owned_or_loaned() {
    let (app, pool) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;
    let id = find_exhibit_id(&pool, "A-001").await;
    // 甲方展品放到乙方 B1，未走转借审批 → 拒绝
    let res = post(&app, "/api/placements", "u-venue", &serde_json::json!({
        "exhibit_id": id, "booth_id": "b-b1",
    }))
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unauthorized_roles_cannot_approve_or_issue_pass() {
    let (app, _) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;

    // 参展商直接签发凭证 → 403
    let res = post(&app, "/api/passes", "u-alpha", &serde_json::json!({"exhibitor_id": "e-alpha"})).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // 闸口签发 → 403
    let res = post(&app, "/api/passes", "u-gate", &serde_json::json!({"exhibitor_id": "e-alpha"})).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // 布展人员签发 → 403
    let res = post(&app, "/api/passes", "u-venue", &serde_json::json!({"exhibitor_id": "e-alpha"})).await;
    assert_eq!(StatusCode::FORBIDDEN, res.status());

    // 先登记一个缺件，参展商尝试自行核销 → 403
    let res = post(&app, "/api/exhibits/A-002/exception", "u-venue",
        &serde_json::json!({"kind": "missing", "note": "找不到了"})).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = read_json(get(&app, "/api/exceptions", "u-alpha").await).await;
    let xid = body.as_array().unwrap().iter()
        .find(|x| x["exhibit_code"] == "A-002").unwrap()["id"].as_str().unwrap().to_string();
    let res = post(&app, &format!("/api/exceptions/{xid}/resolve"), "u-alpha",
        &serde_json::json!({"status": "written_off"})).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "参展商无权自行批准异常放行");
}

#[tokio::test]
async fn full_quantities_must_balance_before_pass() {
    let (app, pool) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;

    // 3 件全部核验出馆
    let res = post(&app, "/api/verify/e-alpha", "u-gate", &serde_json::json!({
        "items": [
            {"code": "A-001", "result": "out"},
            {"code": "A-002", "result": "out"},
            {"code": "A-003", "result": "out"},
        ],
    })).await;
    assert_eq!(res.status(), StatusCode::OK);

    // 经理签发 → 成功，数量 3/3/3
    let res = post(&app, "/api/passes", "u-mgr", &serde_json::json!({"exhibitor_id": "e-alpha"})).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = read_json(res).await;
    assert_eq!(body["counts"]["declared"], 3);
    assert_eq!(body["counts"]["received"], 3);
    assert_eq!(body["counts"]["accounted"], 3);
    assert!(body["code"].as_str().unwrap().starts_with("PASS-"));

    // 数据库核对：入馆、在场、出馆
    let received: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id='e-alpha' AND active=1 AND status IN ('RECEIVED','PLACED','OUT')",
    ).fetch_one(&pool).await.unwrap();
    let out: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM exhibits WHERE exhibitor_id='e-alpha' AND active=1 AND status='OUT'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!((received.0, out.0), (3, 3));
}

#[tokio::test]
async fn missing_item_blocks_pass_until_manager_writes_off() {
    let (app, _) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;

    // A-001/A-003 出馆；A-002 缺件
    let res = post(&app, "/api/verify/e-alpha", "u-gate", &serde_json::json!({
        "items": [
            {"code": "A-001", "result": "out"},
            {"code": "A-003", "result": "out"},
            {"code": "A-002", "result": "missing", "note": "撤展清点缺失"},
        ],
    })).await;
    assert_eq!(res.status(), StatusCode::OK);

    // 未处理缺件：经理也不能签发 → 409（数量不平 + 异常未处理）
    let res = post(&app, "/api/passes", "u-mgr", &serde_json::json!({"exhibitor_id": "e-alpha"})).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // 闸口对缺件展品单件放行 → 409
    let res = post(&app, "/api/exhibits/A-002/checkout", "u-gate",
        &serde_json::json!({})).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // 经理核销
    let body = read_json(get(&app, "/api/exceptions", "u-mgr").await).await;
    let xid = body.as_array().unwrap().iter()
        .find(|x| x["exhibit_code"] == "A-002").unwrap()["id"].as_str().unwrap().to_string();
    let res = post(&app, &format!("/api/exceptions/{xid}/resolve"), "u-mgr",
        &serde_json::json!({"status": "written_off", "note": "确认缺件，责任另行处理，准予平账"})).await;
    assert_eq!(res.status(), StatusCode::OK);

    // 平账后签发成功：出馆 2 + 核销 1 = 申报 3
    let res = post(&app, "/api/passes", "u-mgr", &serde_json::json!({"exhibitor_id": "e-alpha"})).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = read_json(res).await;
    assert_eq!(body["counts"]["out"], 2);
    assert_eq!(body["counts"]["accounted"], 3);
}

#[tokio::test]
async fn replacement_creates_new_manifest_version_and_keeps_history() {
    let (app, pool) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;

    // 参展商发起临时替换 A-002 -> A-002-X（不是直接改入馆记录）
    let res = post(&app, "/api/changes", "u-alpha", &serde_json::json!({
        "kind": "replace_exhibit", "exhibitor_id": "e-alpha",
        "payload": {"old_exhibit_code": "A-002", "new_code": "A-002-X", "new_name": "替换展品"},
    })).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let ch = read_json(res).await;
    let cid = ch["change_id"].as_str().unwrap();

    // 未审批前：新展品不存在
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM exhibits WHERE code='A-002-X'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n.0, 0);

    // 经理批准
    let res = post(&app, &format!("/api/changes/{cid}/review"), "u-mgr",
        &serde_json::json!({"approve": true})).await;
    assert_eq!(res.status(), StatusCode::OK);

    // 旧件失效留痕、新件已申报待入馆；原 RECEIVED 事件仍在
    let (active, status): (i64, String) = sqlx::query_as(
        "SELECT active, status FROM exhibits WHERE code='A-002'").fetch_one(&pool).await.unwrap();
    assert_eq!((active, status.as_str()), (0, "REPLACED"));
    let (nstatus,): (String,) = sqlx::query_as(
        "SELECT status FROM exhibits WHERE code='A-002-X'").fetch_one(&pool).await.unwrap();
    assert_eq!(nstatus, "DECLARED");
    let old_events: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM movement_events m JOIN exhibits e ON e.id=m.exhibit_id WHERE e.code='A-002'")
        .fetch_one(&pool).await.unwrap();
    assert!(old_events.0 >= 2, "旧件入馆等历史记录必须保留");

    // 清单出现 v2 且 v1 superseded
    let versions: Vec<(i64, String)> = sqlx::query_as(
        "SELECT version,status FROM manifests WHERE exhibitor_id='e-alpha' ORDER BY version")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(versions, vec![(1, "superseded".to_string()), (2, "submitted".to_string())]);

    // 新件必须重新走按箱入馆才能布展/出馆
    let err = post(&app, "/api/verify/e-alpha", "u-gate", &serde_json::json!({
        "items": [{"code": "A-002-X", "result": "out"}]
    })).await;
    assert_eq!(err.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn gate_pass_scan_is_one_time_only() {
    let (app, _) = app().await;
    submit_alpha_manifest(&app).await;
    receive_all(&app).await;
    let _ = post(&app, "/api/verify/e-alpha", "u-gate", &serde_json::json!({
        "items": [
            {"code": "A-001", "result": "out"},
            {"code": "A-002", "result": "out"},
            {"code": "A-003", "result": "out"},
        ],
    })).await;
    let pass = read_json(post(&app, "/api/passes", "u-mgr",
        &serde_json::json!({"exhibitor_id": "e-alpha"})).await).await;
    let code = pass["code"].as_str().unwrap();

    let r1 = read_json(post(&app, "/api/gate/scan", "u-gate", &serde_json::json!({"code": code})).await).await;
    assert_eq!(r1["result"], "allow");
    let r2 = read_json(post(&app, "/api/gate/scan", "u-gate", &serde_json::json!({"code": code})).await).await;
    assert_eq!(r2["result"], "deny", "放行凭证只能使用一次");

    // 参展商不能操作闸口
    let r3 = post(&app, "/api/gate/scan", "u-alpha", &serde_json::json!({"code": code})).await;
    assert_eq!(r3.status(), StatusCode::FORBIDDEN);
}
