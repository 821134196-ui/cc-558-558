pub mod db;
pub mod error;
pub mod handlers;
pub mod state;

pub use db::{migrate, seed};
pub use state::AppState;

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/me", axum::routing::get(handlers::me))
        .route("/users", axum::routing::get(handlers::list_users))
        .route("/exhibitors", axum::routing::get(handlers::list_exhibitors))
        .route("/booths", axum::routing::get(handlers::list_booths))
        .route("/overview", axum::routing::get(handlers::overview))
        .route("/manifests", axum::routing::post(handlers::submit_manifest))
        .route("/manifests/:eid", axum::routing::get(handlers::list_manifests))
        .route("/crates", axum::routing::post(handlers::receive_crate).get(handlers::list_crates))
        .route("/exhibits", axum::routing::get(handlers::list_exhibits))
        .route("/exhibits/:ident/trace", axum::routing::get(handlers::trace_exhibit))
        .route("/exhibits/:ident/photo", axum::routing::post(handlers::upload_photo))
        .route("/placements", axum::routing::post(handlers::place_exhibit))
        .route("/exhibits/:ident/transfer", axum::routing::post(handlers::transfer_exhibit))
        .route("/placements/:pid/retract", axum::routing::post(handlers::retract_placement))
        .route("/exceptions", axum::routing::get(handlers::list_exceptions))
        .route("/exhibits/:ident/exception", axum::routing::post(handlers::open_exception))
        .route("/exceptions/:id/resolve", axum::routing::post(handlers::resolve_exception))
        .route("/changes", axum::routing::post(handlers::request_change).get(handlers::list_changes))
        .route("/changes/:id/review", axum::routing::post(handlers::review_change))
        .route("/exhibits/:ident/checkout", axum::routing::post(handlers::checkout_exhibit))
        .route("/verify/:eid", axum::routing::post(handlers::verify_exhibitor))
        .route("/passes", axum::routing::post(handlers::issue_pass).get(handlers::list_passes))
        .route("/gate/scan", axum::routing::post(handlers::gate_scan))
        .route("/gate/scans", axum::routing::get(handlers::list_scans));

    let uploads = ServeDir::new(state.upload_dir.clone());
    let static_dir = ServeDir::new("static").append_index_html_on_directories(true);

    Router::new()
        .nest("/api", api)
        .nest_service("/uploads", uploads)
        .fallback_service(static_dir)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
