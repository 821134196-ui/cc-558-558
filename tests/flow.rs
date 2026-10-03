//! 端到端流程测试：重复占位、数量核对、越权放行、变更留痕
use expo_guard::{build_router, db, AppState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

// 种子用户: 1=王入口(receiver) 2=李场馆(staff) 3=赵主管(manager) 4=陈展商(exhibitor/展商1) 5=林展商(exhibitor/展商2)
const RECEIVER: i64 = 1;
const STAFF: i64 = 2;
const EXHIBITOR1: i64 = 4;

async fn setup() -> axum::Router {
    let pool = db::init_pool("sqlite::memory:", 1).await.expect("init db");
    let dir = std::env::temp_dir().join(format!("expo-test-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    build_router(AppState { pool, upload_dir: dir.to_string_lossy().into_owned() })
}

async fn req(app: &axum::Router, method: &str, path: &str, user: i64, body: Option<Value>) -> (u16, Value) {
    let mut b = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header("x-user-id", user.to_string());
    let payload = if let Some(v) = body {
        b = b.header("content-type", "application/json");
        axum::body::Body::from(serde_json::to_vec(&v).unwrap())
    } else {
        axum::body::Body::empty()
    };
    let resp = app.clone().oneshot(b.body(payload).unwrap()).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

async fn multipart_exception(app: &axum::Router, user: i64, item_id: i64, kind: &str, note: &str) -> (u16, Value) {
    let boundary = "TESTBOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"item_id\"\r\n\r\n{item_id}\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\n{kind}\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\n{note}\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"scene.jpg\"\r\n\
         Content-Type: image/jpeg\r\n\r\nFAKEJPEGDATA\r\n--{b}--\r\n",
        b = boundary
    );
    let r = axum::http::Request::builder()
        .method("POST")
        .uri("/api/exceptions")
        .header("x-user-id", user.to_string())
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(axum::body::Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(r).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 建展商1的清单（2件）并整箱入馆，返回展品 id
async fn declared_and_received(app: &axum::Router, names: &[&str]) -> Vec<i64> {
    let items: Vec<Value> = names.iter().map(|n| json!({"name": n, "serial_no": "SN"})).collect();
    let (s, v) = req(app, "POST", "/api/manifests", EXHIBITOR1, Some(json!({"items": items}))).await;
    assert_eq!(s, 200, "创建清单失败: {v}");
    let ids: Vec<i64> = v["item_ids"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
    let (s, v) = req(app, "POST", "/api/boxes", RECEIVER,
        Some(json!({"box_code": format!("BOX-{}", uuid::Uuid::new_v4().simple()), "exhibitor_id": 1, "item_ids": ids}))).await;
    assert_eq!(s, 200, "收箱失败: {v}");
    ids
}

#[tokio::test]
async fn duplicate_placement_rejected() {
    let app = setup().await;
    let ids = declared_and_received(&app, &["全息投影仪", "触控一体机"]).await;

    // 正常占位
    let (s, v) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": ids[0], "booth_id": 1}))).await;
    assert_eq!(s, 200, "首次占位应成功: {v}");
    // 重复占位到另一个展位 -> 409
    let (s, v) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": ids[0], "booth_id": 2}))).await;
    assert_eq!(s, 409, "重复占位必须被拒绝: {v}");
    assert!(v["error"].as_str().unwrap().contains("不能重复占位") || v["error"].as_str().unwrap().contains("两个展位"));
    // 未入馆的展品不能占位
    let (s, v) = req(&app, "POST", "/api/manifests", 5, Some(json!({"items": [{"name": "别人未入馆的展品"}]}))).await;
    assert_eq!(s, 200);
    let other_id = v["item_ids"][0].as_i64().unwrap();
    let (s, _) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": other_id, "booth_id": 3}))).await;
    assert_eq!(s, 409, "未入馆展品不能占位");

    // 看板确认该展品只出现在一个展位
    let (_, board) = req(&app, "GET", "/api/board", STAFF, None).await;
    let mut seen = 0;
    for b in board["booths"].as_array().unwrap() {
        for it in b["items"].as_array().unwrap() {
            if it["id"].as_i64().unwrap() == ids[0] { seen += 1; }
        }
    }
    assert_eq!(seen, 1, "同一件展品不能同时出现在两个展位");
}

#[tokio::test]
async fn counts_reconcile_through_exit() {
    let app = setup().await;
    let ids = declared_and_received(&app, &["机械臂", "传感器阵列"]).await;

    // 入馆后：入馆=2，在场=0，未平
    let (_, rec) = req(&app, "GET", "/api/exhibitors/1/reconciliation", STAFF, None).await;
    assert_eq!(rec["received"], 2);
    assert_eq!(rec["balanced"], false);

    for (i, booth) in [(0, 1), (1, 2)] {
        let (s, _) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": ids[i], "booth_id": booth}))).await;
        assert_eq!(s, 200);
    }
    let (_, rec) = req(&app, "GET", "/api/exhibitors/1/reconciliation", STAFF, None).await;
    assert_eq!(rec["on_site"], 2);

    // 只核实一件就放行 -> 409 数量不符
    let (s, _) = req(&app, "POST", "/api/teardown/verify", STAFF, Some(json!({"item_id": ids[0]}))).await;
    assert_eq!(s, 200);
    let (s, v) = req(&app, "POST", "/api/exit-passes", STAFF, Some(json!({"exhibitor_id": 1}))).await;
    assert_eq!(s, 409, "数量不符时不能放行: {v}");
    assert!(v["error"].as_str().unwrap().contains("数量不符"));

    // 全部核实后放行成功
    let (s, _) = req(&app, "POST", "/api/teardown/verify", STAFF, Some(json!({"item_id": ids[1]}))).await;
    assert_eq!(s, 200);
    let (_, rec) = req(&app, "GET", "/api/exhibitors/1/reconciliation", STAFF, None).await;
    assert_eq!(rec["balanced"], true, "入馆=核实出馆，数量应平衡");

    let (s, v) = req(&app, "POST", "/api/exit-passes", STAFF, Some(json!({"exhibitor_id": 1}))).await;
    assert_eq!(s, 200, "放行应成功: {v}");
    let code = v["pass_code"].as_str().unwrap().to_string();

    // 闸口扫码 -> 全部出馆
    let (s, v) = req(&app, "POST", "/api/gate/scan", RECEIVER, Some(json!({"pass_code": code}))).await;
    assert_eq!(s, 200, "闸口核销应成功: {v}");
    assert_eq!(v["exited_count"], 2);
    // 重复扫码 -> 409
    let (s, _) = req(&app, "POST", "/api/gate/scan", RECEIVER, Some(json!({"pass_code": code}))).await;
    assert_eq!(s, 409, "凭证不能重复核销");

    let (_, rec) = req(&app, "GET", "/api/exhibitors/1/reconciliation", STAFF, None).await;
    assert_eq!(rec["received"], 2);
    assert_eq!(rec["exited"], 2);
    assert_eq!(rec["balanced"], true, "入馆=出馆，全程数量相符");
}

#[tokio::test]
async fn exhibitor_cannot_approve_exception_or_release() {
    let app = setup().await;
    let ids = declared_and_received(&app, &["文物展柜"]).await;
    let (s, _) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": ids[0], "booth_id": 1}))).await;
    assert_eq!(s, 200);

    // 参展商上报缺件（含现场照片）
    let (s, v) = multipart_exception(&app, EXHIBITOR1, ids[0], "missing", "撤展时发现展柜缺失").await;
    assert_eq!(s, 200, "上报异常应成功: {v}");
    assert!(v["photo_path"].as_str().unwrap().starts_with("/uploads/"));
    let ex_id = v["exception_id"].as_i64().unwrap();

    // 有未处理异常 -> 不能放行
    let (s, _) = req(&app, "POST", "/api/exit-passes", STAFF, Some(json!({"exhibitor_id": 1}))).await;
    assert_eq!(s, 409, "未处理异常存在时不能放行");

    // 参展商尝试自行批准异常 -> 403
    let (s, v) = req(&app, "POST", &format!("/api/exceptions/{ex_id}/decide"), EXHIBITOR1, Some(json!({"approve": true}))).await;
    assert_eq!(s, 403, "参展商无权批准异常放行: {v}");
    // 参展商尝试自行开具放行凭证 -> 403
    let (s, _) = req(&app, "POST", "/api/exit-passes", EXHIBITOR1, Some(json!({"exhibitor_id": 1}))).await;
    assert_eq!(s, 403, "参展商无权自行放行");

    // 场馆方批准缺件 -> 展品结案，数量重新平衡
    let (s, v) = req(&app, "POST", &format!("/api/exceptions/{ex_id}/decide"), STAFF, Some(json!({"approve": true}))).await;
    assert_eq!(s, 200, "场馆方批准应成功: {v}");
    let (_, rec) = req(&app, "GET", "/api/exhibitors/1/reconciliation", STAFF, None).await;
    assert_eq!(rec["missing_closed"], 1);
    assert_eq!(rec["balanced"], true);

    // 放行 + 闸口核销：缺件计入核对但不出馆
    let (s, v) = req(&app, "POST", "/api/exit-passes", STAFF, Some(json!({"exhibitor_id": 1}))).await;
    assert_eq!(s, 200, "{v}");
    let code = v["pass_code"].as_str().unwrap().to_string();
    let (s, v) = req(&app, "POST", "/api/gate/scan", STAFF, Some(json!({"pass_code": code}))).await;
    assert_eq!(s, 200);
    assert_eq!(v["missing_accounted"], 1);
}

#[tokio::test]
async fn damaged_exception_with_photo_flow() {
    let app = setup().await;
    let ids = declared_and_received(&app, &["青花瓷瓶"]).await;
    let (s, _) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": ids[0], "booth_id": 1}))).await;
    assert_eq!(s, 200);

    // 不上传照片 -> 400
    let boundary = "NOPH";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"item_id\"\r\n\r\n{}\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\ndamaged\r\n--{b}--\r\n", ids[0], b = boundary);
    let r = axum::http::Request::builder()
        .method("POST").uri("/api/exceptions")
        .header("x-user-id", STAFF.to_string())
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(axum::body::Body::from(body)).unwrap();
    let resp = app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status().as_u16(), 400, "缺件/破损必须上传现场照片");

    // 上报破损 -> 场馆批准 -> 按破损核实出馆
    let (s, v) = multipart_exception(&app, STAFF, ids[0], "damaged", "瓶口磕碰").await;
    assert_eq!(s, 200, "{v}");
    let ex_id = v["exception_id"].as_i64().unwrap();
    let (s, _) = req(&app, "POST", &format!("/api/exceptions/{ex_id}/decide"), STAFF, Some(json!({"approve": true}))).await;
    assert_eq!(s, 200);
    let (_, rec) = req(&app, "GET", "/api/exhibitors/1/reconciliation", STAFF, None).await;
    assert_eq!(rec["verified_out"], 1, "破损批准后按核实出馆计");
}

#[tokio::test]
async fn amendment_replacement_and_early_exit() {
    let app = setup().await;
    let ids = declared_and_received(&app, &["展品甲", "展品乙", "展品丙"]).await;
    for (i, booth) in [(0, 1), (1, 2), (2, 3)] {
        let (s, _) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": ids[i], "booth_id": booth}))).await;
        assert_eq!(s, 200);
    }

    // 临时替换：参展商申请，参展商自行确认 -> 403；场馆确认后新展品入清单，原入馆记录不变
    let (s, v) = req(&app, "POST", "/api/amendments", EXHIBITOR1,
        Some(json!({"kind": "replacement", "payload": {"item_id": ids[0], "new_name": "展品甲-替换件", "new_serial_no": "SN-R1"}}))).await;
    assert_eq!(s, 200, "{v}");
    let am_id = v["amendment_id"].as_i64().unwrap();
    let (s, _) = req(&app, "POST", &format!("/api/amendments/{am_id}/decide"), EXHIBITOR1, Some(json!({"approve": true}))).await;
    assert_eq!(s, 403, "参展商无权自行确认变更");
    let (s, v) = req(&app, "POST", &format!("/api/amendments/{am_id}/decide"), STAFF, Some(json!({"approve": true}))).await;
    assert_eq!(s, 200, "{v}");

    let (_, items) = req(&app, "GET", "/api/items?exhibitor_id=1", STAFF, None).await;
    let arr = items.as_array().unwrap();
    assert_eq!(arr.len(), 4, "替换后清单应追加新展品");
    let repl = arr.iter().find(|i| i["origin"] == "replacement").unwrap();
    assert_eq!(repl["status"], "declared");
    let old = arr.iter().find(|i| i["id"] == ids[0]).unwrap();
    assert_eq!(old["status"], "placed", "原展品入馆记录不得被改动");

    // 提前撤展：确认后可开部分放行凭证
    let (s, v) = req(&app, "POST", "/api/amendments", EXHIBITOR1,
        Some(json!({"kind": "early_exit", "payload": {"item_ids": [ids[1]]}}))).await;
    assert_eq!(s, 200, "{v}");
    let am_id = v["amendment_id"].as_i64().unwrap();
    let (s, _) = req(&app, "POST", &format!("/api/amendments/{am_id}/decide"), STAFF, Some(json!({"approve": true}))).await;
    assert_eq!(s, 200);

    // 未核实出馆就开部分凭证 -> 409
    let (s, _) = req(&app, "POST", "/api/exit-passes", STAFF,
        Some(json!({"exhibitor_id": 1, "item_ids": [ids[1]], "amendment_id": am_id}))).await;
    assert_eq!(s, 409, "未核实出馆不能放行");
    let (s, _) = req(&app, "POST", "/api/teardown/verify", STAFF, Some(json!({"item_id": ids[1]}))).await;
    assert_eq!(s, 200);
    // 展品集合与变更不一致 -> 409
    let (s, _) = req(&app, "POST", "/api/exit-passes", STAFF,
        Some(json!({"exhibitor_id": 1, "item_ids": [ids[1], ids[2]], "amendment_id": am_id}))).await;
    assert_eq!(s, 409, "部分放行必须与变更一致");
    let (s, v) = req(&app, "POST", "/api/exit-passes", STAFF,
        Some(json!({"exhibitor_id": 1, "item_ids": [ids[1]], "amendment_id": am_id}))).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["kind"], "partial");
    let code = v["pass_code"].as_str().unwrap().to_string();
    let (s, v) = req(&app, "POST", "/api/gate/scan", STAFF, Some(json!({"pass_code": code}))).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["exited_count"], 1);
}

#[tokio::test]
async fn transfer_records_last_handler() {
    let app = setup().await;
    let ids = declared_and_received(&app, &["移动展柜"]).await;
    let (s, _) = req(&app, "POST", "/api/placements", STAFF, Some(json!({"item_id": ids[0], "booth_id": 1}))).await;
    assert_eq!(s, 200);
    let (s, v) = req(&app, "POST", "/api/transfers", STAFF,
        Some(json!({"item_id": ids[0], "to_booth_id": 5, "note": "调整动线"}))).await;
    assert_eq!(s, 200, "{v}");

    let (_, detail) = req(&app, "GET", &format!("/api/items/{}", ids[0]), STAFF, None).await;
    assert_eq!(detail["current_booth"]["code"], "B-02");
    let events = detail["events"].as_array().unwrap();
    let types: Vec<&str> = events.iter().map(|e| e["event_type"].as_str().unwrap()).collect();
    assert!(types.contains(&"item_received"));
    assert!(types.contains(&"item_placed"));
    assert!(types.contains(&"item_transferred"));
    // 最后经手人 = 执行转移的场馆人员
    assert_eq!(detail["last_handler"]["name"], "李场馆");
    assert_eq!(detail["last_handler"]["event_type"], "item_transferred");
}
