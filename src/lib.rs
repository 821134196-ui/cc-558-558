pub mod auth;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod util;

use axum::{extract::DefaultBodyLimit, routing::{get, post}, Router};
use tower_http::{cors::CorsLayer, services::{ServeDir, ServeFile}};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub upload_dir: String,
}

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/users", get(routes::reports::list_users))
        .route("/exhibitors", get(routes::reports::list_exhibitors))
        .route("/booths", get(routes::reports::list_booths))
        .route("/board", get(routes::reports::board))
        .route("/manifests", post(routes::manifests::create_manifest).get(routes::manifests::list_manifests))
        .route("/manifests/{id}", get(routes::manifests::get_manifest))
        .route("/boxes", post(routes::receiving::receive_box).get(routes::receiving::list_boxes))
        .route("/boxes/{id}", get(routes::receiving::get_box))
        .route("/placements", post(routes::placements::place_item))
        .route("/transfers", post(routes::placements::transfer_item))
        .route("/teardown/verify", post(routes::teardown::verify_item))
        .route("/exceptions", post(routes::exceptions::report_exception).get(routes::exceptions::list_exceptions))
        .route("/exceptions/{id}/decide", post(routes::exceptions::decide_exception))
        .route("/amendments", post(routes::amendments::create_amendment).get(routes::amendments::list_amendments))
        .route("/amendments/{id}/decide", post(routes::amendments::decide_amendment))
        .route("/exit-passes", post(routes::exitpass::issue_pass).get(routes::exitpass::list_passes))
        .route("/exit-passes/{id}", get(routes::exitpass::get_pass))
        .route("/gate/scan", post(routes::exitpass::gate_scan))
        .route("/items", get(routes::reports::list_items))
        .route("/items/{id}", get(routes::reports::item_detail))
        .route("/exhibitors/{id}/reconciliation", get(routes::reports::reconciliation))
        .layer(DefaultBodyLimit::max(15 * 1024 * 1024)); // 现场照片上传上限 15MB

    Router::new()
        .nest("/api", api)
        .nest_service("/uploads", ServeDir::new(state.upload_dir.clone()))
        .fallback_service(
            ServeDir::new("web/dist").not_found_service(ServeFile::new("web/dist/index.html")),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
